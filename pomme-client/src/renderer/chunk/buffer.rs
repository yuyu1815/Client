use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use azalea_core::position::{BlockPos, ChunkPos};
use glam::DVec3;
use pomme_gpu_allocator::vulkan::{Allocation, Allocator};
use pyronyx::vk;
use serde_json::json;

use super::mesher::{
    ChunkAABB, ChunkMeshData, EmittedChest, MeshTraceState, PackedVertex, SectionMesh,
};
use crate::renderer::{MAX_FRAMES_IN_FLIGHT, shader, util};

const BUCKET_VERTICES: u32 = 32768;
const BUCKET_INDICES: u32 = 49152;
const VERTEX_SIZE: u64 = size_of::<PackedVertex>() as u64;
const INDEX_SIZE: u64 = size_of::<u32>() as u64;
const BYTES_PER_BUCKET: u64 =
    BUCKET_VERTICES as u64 * VERTEX_SIZE + BUCKET_INDICES as u64 * INDEX_SIZE;

fn staging_section_fits(vertex_bytes: usize, index_bytes: usize, half: usize) -> bool {
    vertex_bytes <= half && index_bytes <= half
}

fn staging_upload_section_too_large(vertices: usize, indices: usize, half: usize) -> bool {
    vertices > 0
        && indices > 0
        && !staging_section_fits(
            vertices * VERTEX_SIZE as usize,
            indices * INDEX_SIZE as usize,
            half,
        )
}

fn staging_needs_flush(
    vertex_used: usize,
    index_used: usize,
    vertex_bytes: usize,
    index_bytes: usize,
    half: usize,
) -> bool {
    vertex_used + vertex_bytes > half || index_used + index_bytes > half
}

fn retire_on_failed_upload(pool_full: bool, staging_too_large: bool) -> bool {
    pool_full && !staging_too_large
}
const MIN_BUCKETS: u32 = 128;
const MAX_BUCKETS: u32 = 2048;
const VRAM_BUDGET_FRACTION: f64 = 0.25;
/// Per-section fade-in length, shared by the opaque indirect path and water.
const FADE_DURATION_MS: f32 = 1000.0;
/// Columns within this squared X/Z distance of the camera render opaque
/// immediately and never fade in.
const NEARBY_DIST_SQ: f32 = 768.0;

/// Whether a column's center is within the always-near X/Z radius of the eye
/// (vanilla `isNearby`), rebased in f64 for precision at extreme coordinates.
/// Also gates mesh-scheduling tiers (`in_game::apply_visibility`).
pub fn column_is_near(pos: ChunkPos, eye: DVec3) -> bool {
    let dx = pos.x as f64 * 16.0 + 8.0 - eye.x;
    let dz = pos.z as f64 * 16.0 + 8.0 - eye.z;
    dx * dx + dz * dz < NEARBY_DIST_SQ as f64
}

/// First-fit free-list sub-allocator over a fixed element range, coalescing on
/// free. Each section gets an exact-size vertex (and index) slice instead of
/// whole fixed buckets — vanilla's `UberGpuBuffer` model — so re-uploading one
/// section never disturbs the rest and there is no per-section bucket waste.
struct FreeList {
    capacity: u32,
    /// Free regions `(offset, len)`, sorted by offset and coalesced (no two
    /// adjacent).
    free: Vec<(u32, u32)>,
}

impl FreeList {
    fn new(capacity: u32) -> Self {
        Self {
            capacity,
            free: vec![(0, capacity)],
        }
    }

    fn reset(&mut self) {
        self.free.clear();
        self.free.push((0, self.capacity));
    }

    /// Allocate `n` contiguous elements; `None` if no region is large enough.
    fn alloc(&mut self, n: u32) -> Option<u32> {
        for i in 0..self.free.len() {
            let (off, len) = self.free[i];
            if len >= n {
                if len == n {
                    self.free.remove(i);
                } else {
                    self.free[i] = (off + n, len - n);
                }
                return Some(off);
            }
        }
        None
    }

    /// Return a region, coalescing with an adjacent free region on either side.
    fn free_region(&mut self, off: u32, n: u32) {
        let pos = self.free.partition_point(|&(o, _)| o < off);
        self.free.insert(pos, (off, n));
        if pos + 1 < self.free.len() {
            let (o, l) = self.free[pos];
            let (no, nl) = self.free[pos + 1];
            if o + l == no {
                self.free[pos] = (o, l + nl);
                self.free.remove(pos + 1);
            }
        }
        if pos > 0 {
            let (po, pl) = self.free[pos - 1];
            let (o, l) = self.free[pos];
            if po + pl == o {
                self.free[pos - 1] = (po, pl + l);
                self.free.remove(pos);
            }
        }
    }
}

fn compute_bucket_count(physical_device: vk::PhysicalDevice) -> u32 {
    let mem_props = physical_device.get_memory_properties();
    let mut device_local_bytes: u64 = 0;
    for i in 0..mem_props.memory_type_count as usize {
        let mem_type = mem_props.memory_types[i];
        if mem_type
            .property_flags
            .contains(vk::MemoryPropertyFlags::DeviceLocal)
        {
            let heap = mem_props.memory_heaps[mem_type.heap_index as usize];
            if heap.size > device_local_bytes {
                device_local_bytes = heap.size;
            }
        }
    }
    let budget = (device_local_bytes as f64 * VRAM_BUDGET_FRACTION) as u64;
    let buckets = (budget / BYTES_PER_BUCKET) as u32;
    let count = buckets.clamp(MIN_BUCKETS, MAX_BUCKETS);
    tracing::info!(
        "GPU VRAM: {} MB, chunk budget: {} MB, buckets: {}",
        device_local_bytes / (1024 * 1024),
        (count as u64 * BYTES_PER_BUCKET) / (1024 * 1024),
        count
    );
    count
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct ChunkMeta {
    /// Section-local vertex bounds; the cull shader rebases them via `origin`.
    aabb_min: [f32; 4],
    aabb_max: [f32; 4],
    index_count: u32,
    first_index: u32,
    vertex_offset: i32,
    visibility: u32,
    /// Absolute section origin as integers (vanilla `ChunkPosition`), bound
    /// as a per-instance vertex attribute; the vertex shader subtracts the
    /// camera block position in integer math, so no large f32 is ever
    /// formed.
    origin: [i32; 3],
    /// Read by the cull shader to split the solid/cutout draws; fills the
    /// fourth lane of `origin`'s 16-byte slot, keeping the struct at 64 bytes.
    solid_index_count: u32,
}

/// Copy already-packed `verts` into `dst` starting at byte `off`.
fn write_verts(dst: &mut [u8], off: usize, verts: &[PackedVertex]) {
    let bytes: &[u8] = bytemuck::cast_slice(verts);
    dst[off..off + bytes.len()].copy_from_slice(bytes);
}

/// Vertex input for the chunk pipeline: binding 0 is the packed per-vertex
/// pool, binding 1 is the meta buffer read per-instance (origin + fade),
/// indexed by the `first_instance` the cull shader writes.
pub fn chunk_vertex_bindings() -> [vk::VertexInputBindingDescription; 2] {
    [
        vk::VertexInputBindingDescription {
            binding: 0,
            stride: size_of::<PackedVertex>() as u32,
            input_rate: vk::VertexInputRate::Vertex,
        },
        vk::VertexInputBindingDescription {
            binding: 1,
            stride: size_of::<ChunkMeta>() as u32,
            input_rate: vk::VertexInputRate::Instance,
        },
    ]
}

pub fn chunk_vertex_attributes() -> [vk::VertexInputAttributeDescription; 7] {
    let pos_off = std::mem::offset_of!(PackedVertex, pos) as u32;
    let uv_off = std::mem::offset_of!(PackedVertex, uv) as u32;
    let sprite_off = std::mem::offset_of!(PackedVertex, sprite) as u32;
    let light_tint_off = std::mem::offset_of!(PackedVertex, light_tint) as u32;
    let origin_off = std::mem::offset_of!(ChunkMeta, origin) as u32;
    let vis_off = std::mem::offset_of!(ChunkMeta, visibility) as u32;
    [
        // binding 0 — packed vertex (pos split into xy + z lanes)
        vk::VertexInputAttributeDescription {
            location: 0,
            binding: 0,
            format: vk::Format::R16G16Unorm,
            offset: pos_off,
        },
        vk::VertexInputAttributeDescription {
            location: 1,
            binding: 0,
            format: vk::Format::R16Unorm,
            offset: pos_off + 4,
        },
        vk::VertexInputAttributeDescription {
            location: 2,
            binding: 0,
            format: vk::Format::R16G16Uint,
            offset: uv_off,
        },
        vk::VertexInputAttributeDescription {
            location: 3,
            binding: 0,
            format: vk::Format::R16Uint,
            offset: sprite_off,
        },
        vk::VertexInputAttributeDescription {
            location: 4,
            binding: 0,
            format: vk::Format::R8G8B8A8Unorm,
            offset: light_tint_off,
        },
        // binding 1 — per-instance meta (origin + fade)
        vk::VertexInputAttributeDescription {
            location: 5,
            binding: 1,
            format: vk::Format::R32G32B32Sint,
            offset: origin_off,
        },
        vk::VertexInputAttributeDescription {
            location: 6,
            binding: 1,
            format: vk::Format::R32Sfloat,
            offset: vis_off,
        },
    ]
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct DrawCommand {
    index_count: u32,
    instance_count: u32,
    first_index: u32,
    vertex_offset: i32,
    first_instance: u32,
}

/// Camera-relative frustum test of a section-local AABB, mirroring `cull.comp`
/// (the GPU opaque path); used by the CPU-driven water pass. The section
/// origin is rebased against the eye in f64 for precision at extreme
/// coordinates.
fn aabb_in_frustum(aabb: &ChunkAABB, origin: [i32; 3], planes: &[[f32; 4]; 6], eye: DVec3) -> bool {
    let base = (origin_dvec(origin) - eye).as_vec3();
    let mn = [
        base.x + aabb.min[0],
        base.y + aabb.min[1],
        base.z + aabb.min[2],
    ];
    let mx = [
        base.x + aabb.max[0],
        base.y + aabb.max[1],
        base.z + aabb.max[2],
    ];
    for p in planes {
        let d = p[0] * if p[0] >= 0.0 { mx[0] } else { mn[0] }
            + p[1] * if p[1] >= 0.0 { mx[1] } else { mn[1] }
            + p[2] * if p[2] >= 0.0 { mx[2] } else { mn[2] }
            + p[3];
        if d < 0.0 {
            return false;
        }
    }
    true
}

/// An integer section origin widened for f64 math.
fn origin_dvec(origin: [i32; 3]) -> DVec3 {
    DVec3::new(origin[0] as f64, origin[1] as f64, origin[2] as f64)
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct FrustumData {
    planes: [[f32; 4]; 6],
    chunk_count: u32,
    /// Camera block position (the render anchor as integers); the cull
    /// subtracts it from the absolute integer section origins.
    cam_block: [i32; 3],
    /// Eye position relative to `cam_block` (small, full precision).
    frac: [f32; 3],
    /// Pads the struct to a 16-byte multiple so the buffer always covers the
    /// std140 block size.
    _pad: f32,
}

/// One uploaded 16³ section: a self-contained indexed draw plus its tight AABB.
/// `first_index`/`index_count` are the section's index slice and
/// `vertex_offset` its vertex slice base; `vtx_len` is the slice length, kept
/// so the slices can be returned to the free-lists on removal. `uploaded_at`
/// drives the per-section fade so editing one section never re-fades the rest
/// of the column.
struct SectionAlloc {
    section_index: i32,
    aabb: ChunkAABB,
    /// Section world origin (`chunk*16`, `min_y + si*16`), used to rebase the
    /// quantized vertices and passed to the GPU via `ChunkMeta.origin`.
    origin: [i32; 3],
    first_index: u32,
    /// Opaque index count (the GPU-culled draw); water is excluded.
    index_count: u32,
    /// Leading indices belonging to the solid (no-discard) pass; the rest are
    /// cutout. Passed to the GPU via `ChunkMeta.origin[3]`.
    solid_index_count: u32,
    /// Translucent water index slice, stored right after the opaque indices in
    /// the same index allocation. Drawn in a separate blended pass.
    water_first_index: u32,
    water_index_count: u32,
    /// Total allocated index slice length (opaque + water), for freeing.
    idx_len: u32,
    vertex_offset: i32,
    vtx_len: u32,
    uploaded_at: std::time::Instant,
    /// Upload epoch this section's geometry came from; an older upload is
    /// rejected. See [`ChunkMeshData::upload_epoch`].
    epoch: u64,
    /// Only geometry included in this section's accepted GPU allocation.
    emitted_chests: Vec<EmittedChest>,
}

struct ChunkAlloc {
    sections: Vec<SectionAlloc>,
}

fn section_upload_time(
    now: std::time::Instant,
    was_present: bool,
    has_chest: bool,
) -> std::time::Instant {
    // ponytail: skip first fade for the *entire* chest-bearing section,
    // including other blocks; split chest draws if that visual matters.
    // Otherwise the BE would overlap partially transparent new geometry.
    if was_present || has_chest {
        now.checked_sub(std::time::Duration::from_secs(2))
            .unwrap_or(now)
    } else {
        now
    }
}

fn epoch_accepts(current: Option<u64>, incoming: u64) -> bool {
    incoming >= current.unwrap_or(0)
}

fn forget_column(
    chunks: &mut HashMap<ChunkPos, ChunkAlloc>,
    empty_epochs: &mut HashMap<(ChunkPos, i32), u64>,
    pos: &ChunkPos,
) -> Option<ChunkAlloc> {
    empty_epochs.retain(|(chunk, _), _| chunk != pos);
    chunks.remove(pos)
}

fn clear_columns(
    chunks: &mut HashMap<ChunkPos, ChunkAlloc>,
    empty_epochs: &mut HashMap<(ChunkPos, i32), u64>,
) {
    chunks.clear();
    empty_epochs.clear();
}

/// Retire accepted resident sections (also used on pool exhaustion to make
/// room after the frame fences); empty sections receive a tombstone epoch.
fn swap_accepted(
    chunks: &mut HashMap<ChunkPos, ChunkAlloc>,
    empty_epochs: &mut HashMap<(ChunkPos, i32), u64>,
    pos: ChunkPos,
    accepted: &std::collections::HashSet<i32>,
    planned: &std::collections::HashSet<i32>,
    epoch: u64,
) -> Vec<(u32, u32, u32, u32)> {
    let mut freed = Vec::new();
    if let Some(entry) = chunks.get_mut(&pos) {
        entry.sections.retain(|s| {
            if accepted.contains(&s.section_index) {
                freed.push((s.vertex_offset as u32, s.vtx_len, s.first_index, s.idx_len));
                false
            } else {
                true
            }
        });
    }
    for &si in accepted {
        if planned.contains(&si) {
            empty_epochs.remove(&(pos, si));
        } else {
            empty_epochs.insert((pos, si), epoch);
        }
    }
    if chunks.get(&pos).is_some_and(|c| c.sections.is_empty()) {
        chunks.remove(&pos);
    }
    freed
}

fn section_resident(
    chunks: &HashMap<ChunkPos, ChunkAlloc>,
    empty_epochs: &HashMap<(ChunkPos, i32), u64>,
    pos: &ChunkPos,
    section: i32,
    world_empty: bool,
) -> bool {
    chunks
        .get(pos)
        .is_some_and(|c| c.sections.iter().any(|s| s.section_index == section))
        || (world_empty && empty_epochs.contains_key(&(*pos, section)))
}

// Duplicated positions are ambiguous (e.g. a malformed section upload):
// never claim a unique resident shape unless exactly one draw owns the
// position.
fn resident_chest_open<'a>(
    sections: impl Iterator<Item = &'a SectionAlloc>,
    pos: &BlockPos,
) -> Option<bool> {
    let mut matches = sections
        .filter(|s| s.index_count > 0)
        .flat_map(|s| &s.emitted_chests)
        .filter(|chest| &chest.pos == pos);
    let open = matches.next()?.open;
    matches.next().is_none().then_some(open)
}

struct EditFrame {
    vertices: (vk::Buffer, Allocation),
    indices: (vk::Buffer, Allocation),
    meta: (vk::Buffer, Allocation),
    draws: Vec<(u32, u32, u32, i32, u32)>,
}

pub struct ChunkBufferStore {
    pub(crate) edits: super::edit::EditOverlay,
    edit_frames: Vec<EditFrame>,
    /// Capacity (in draws) of the per-frame meta/indirect buffers. Grown on
    /// demand because per-section packing yields many more draws than buckets.
    max_meta: usize,
    vertex_buffer: vk::Buffer,
    vertex_alloc: Allocation,
    index_buffer: vk::Buffer,
    index_alloc: Allocation,
    staging_buffer: vk::Buffer,
    staging_alloc: Allocation,
    staging_size: u64,
    transfer_pool: vk::CommandPool,
    transfer_cmd: vk::CommandBuffer,
    /// Signals completion of a batched staging->device transfer. Reused (reset
    /// before each submit) so a frame's uploads sync once instead of per-mesh.
    transfer_fence: vk::Fence,
    use_staging: bool,

    /// Exact-size sub-allocators over the vertex and index pools (in elements).
    vtx_free: FreeList,
    idx_free: FreeList,
    chunks: HashMap<ChunkPos, ChunkAlloc>,
    /// Retain the epoch of accepted empty sections so an older result cannot
    /// resurrect a section after its allocation has been removed.
    empty_epochs: HashMap<(ChunkPos, i32), u64>,
    /// Per-column bitmask of occlusion-visible section indices (bit `si`), from
    /// the CPU visibility graph. A column absent here defaults to fully
    /// visible, so freshly-loaded-but-not-yet-graphed columns still draw.
    chunk_visibility: HashMap<ChunkPos, u32>,
    cached_meta: Vec<ChunkMeta>,
    meta_dirty: bool,
    /// End of the current fade-in window. While `now < fade_until` the
    /// per-section fade values change each frame, so `cached_meta` must be
    /// rebuilt; an O(1) check replacing the old all-sections scan.
    fade_until: std::time::Instant,
    /// Eye position at the last front-to-back sort; the sort (an early-Z
    /// optimization) is only redone once the camera moves past a threshold.
    last_sort_cam: DVec3,
    /// Frame slots still needing the latest `cached_meta` uploaded. Set to
    /// `MAX_FRAMES_IN_FLIGHT` whenever the draw list changes, decremented per
    /// frame; at steady state the per-frame meta copy stops.
    meta_upload_pending: u32,

    compute_pipeline: vk::Pipeline,
    compute_layout: vk::PipelineLayout,
    compute_desc_layout: vk::DescriptorSetLayout,
    compute_pool: vk::DescriptorPool,
    compute_sets: Vec<vk::DescriptorSet>,

    meta_buffers: Vec<vk::Buffer>,
    meta_allocs: Vec<Allocation>,
    // Solid (no-discard, early-Z) draw list, written by the cull shader.
    indirect_buffers: Vec<vk::Buffer>,
    indirect_allocs: Vec<Allocation>,
    count_buffers: Vec<vk::Buffer>,
    count_allocs: Vec<Allocation>,
    // Cutout (discard) draw list. Same sections, the back of each section's
    // index slice; drawn in a second pass after solid lays down depth.
    indirect_cutout_buffers: Vec<vk::Buffer>,
    indirect_cutout_allocs: Vec<Allocation>,
    count_cutout_buffers: Vec<vk::Buffer>,
    count_cutout_allocs: Vec<Allocation>,
    frustum_buffers: Vec<vk::Buffer>,
    frustum_allocs: Vec<Allocation>,
    fade_enabled: bool,
    /// Post-cull section draw count read back from the GPU (lags a few frames);
    /// exposed for the debug overlay so occlusion's effect is visible.
    last_draw_count: u32,

    /// Monotonic frame counter, bumped once per rendered frame in
    /// `begin_frame`.
    frame_seq: u64,
    /// Slices freed by a re-mesh or unload, each tagged with the `frame_seq` at
    /// which it's safe to reclaim (`MAX_FRAMES_IN_FLIGHT` out, so no in-flight
    /// frame still draws it). Drained in `begin_frame`.
    pending_free: VecDeque<(u64, (u32, u32, u32, u32))>,
    trace_state: MeshTraceState,
}

impl ChunkBufferStore {
    pub fn new(
        device: &vk::Device,
        physical_device: vk::PhysicalDevice,
        graphics_family: u32,
        allocator: &Arc<Mutex<Allocator>>,
        trace_state: MeshTraceState,
    ) -> Self {
        let total_buckets = compute_bucket_count(physical_device);
        let vertex_size = total_buckets as u64 * BUCKET_VERTICES as u64 * VERTEX_SIZE;
        let index_size = total_buckets as u64 * BUCKET_INDICES as u64 * INDEX_SIZE;

        let dev_props = physical_device.get_properties();
        let use_staging = dev_props.device_type == vk::PhysicalDeviceType::DiscreteGpu;

        let (vertex_buffer, vertex_alloc, index_buffer, index_alloc) = if use_staging {
            let (vb, va) = util::create_device_buffer(
                device,
                allocator,
                vertex_size,
                vk::BufferUsageFlags::VertexBuffer,
                "vertex_pool",
            );
            let (ib, ia) = util::create_device_buffer(
                device,
                allocator,
                index_size,
                vk::BufferUsageFlags::IndexBuffer,
                "index_pool",
            );
            (vb, va, ib, ia)
        } else {
            let (vb, va) = util::create_host_buffer(
                device,
                allocator,
                vertex_size,
                vk::BufferUsageFlags::VertexBuffer,
                "vertex_pool",
            );
            let (ib, ia) = util::create_host_buffer(
                device,
                allocator,
                index_size,
                vk::BufferUsageFlags::IndexBuffer,
                "index_pool",
            );
            (vb, va, ib, ia)
        };

        // Discrete GPUs batch a frame's uploads through this buffer in one
        // transfer, so size it to hold several columns and keep sub-flushes rare.
        // The integrated path writes mapped memory directly and never touches it.
        let staging_size = if use_staging {
            BYTES_PER_BUCKET * 16
        } else {
            BYTES_PER_BUCKET * 4
        };
        let (staging_buffer, staging_alloc) = util::create_host_buffer(
            device,
            allocator,
            staging_size,
            vk::BufferUsageFlags::TransferSrc,
            "staging",
        );

        let pool_info = vk::CommandPoolCreateInfo {
            queue_family_index: graphics_family,
            flags: vk::CommandPoolCreateFlags::Transient
                | vk::CommandPoolCreateFlags::ResetCommandBuffer,
            ..Default::default()
        };
        let transfer_pool = device
            .create_command_pool(&pool_info, None)
            .expect("failed to create transfer pool");
        let cmd_info = vk::CommandBufferAllocateInfo {
            command_pool: transfer_pool,
            level: vk::CommandBufferLevel::Primary,
            command_buffer_count: 1,
            ..Default::default()
        };
        let mut transfer_cmd = vk::CommandBuffer::null();
        unsafe {
            device.allocate_command_buffers(&cmd_info, std::slice::from_mut(&mut transfer_cmd))
        }
        .expect("failed to alloc transfer cmd");

        let transfer_fence = device
            .create_fence(&vk::FenceCreateInfo::default(), None)
            .expect("failed to create transfer fence");

        tracing::info!(
            "Chunk buffers: {} (vertex={} MB, index={} MB, staging={} KB)",
            if use_staging {
                "DEVICE_LOCAL + staging"
            } else {
                "HOST_VISIBLE"
            },
            vertex_size / (1024 * 1024),
            index_size / (1024 * 1024),
            staging_size / 1024,
        );

        let vtx_free = FreeList::new(total_buckets * BUCKET_VERTICES);
        let idx_free = FreeList::new(total_buckets * BUCKET_INDICES);

        // Per-section packing yields many more draws than buckets, so pre-size
        // generously: growth (`ensure_meta_capacity`) needs a `device.wait_idle`
        // to safely rewrite the descriptor sets, and we don't want that stall
        // firing mid-stream. The remaining grow path stays as a rare safety net.
        let max_meta = (total_buckets * 16).max(8192) as usize;
        let meta_size = (max_meta * size_of::<ChunkMeta>()) as u64;
        let indirect_size = (max_meta * size_of::<DrawCommand>()) as u64;
        let count_size = 4u64;
        let frustum_size = size_of::<FrustumData>() as u64;

        let mut meta_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        let mut meta_allocs = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        let mut indirect_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        let mut indirect_allocs = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        let mut count_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        let mut count_allocs = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        let mut indirect_cutout_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        let mut indirect_cutout_allocs = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        let mut count_cutout_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        let mut count_cutout_allocs = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        let mut frustum_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        let mut frustum_allocs = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);

        // Zeroed so the first readback (before any cull has written the slot)
        // sees a real count, not whatever the allocation held.
        let create_count_buffer = |name: &str| {
            let (b, mut a) = util::create_host_buffer(
                device,
                allocator,
                count_size,
                vk::BufferUsageFlags::StorageBuffer | vk::BufferUsageFlags::IndirectBuffer,
                name,
            );
            a.mapped_slice_mut().unwrap()[..count_size as usize].fill(0);
            (b, a)
        };

        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            let (b, a) = util::create_host_buffer(
                device,
                allocator,
                meta_size,
                vk::BufferUsageFlags::StorageBuffer | vk::BufferUsageFlags::VertexBuffer,
                "chunk_meta",
            );
            meta_buffers.push(b);
            meta_allocs.push(a);

            let (b, a) = util::create_host_buffer(
                device,
                allocator,
                indirect_size,
                vk::BufferUsageFlags::StorageBuffer | vk::BufferUsageFlags::IndirectBuffer,
                "indirect_cmds",
            );
            indirect_buffers.push(b);
            indirect_allocs.push(a);

            let (b, a) = create_count_buffer("draw_count");
            count_buffers.push(b);
            count_allocs.push(a);

            let (b, a) = util::create_host_buffer(
                device,
                allocator,
                indirect_size,
                vk::BufferUsageFlags::StorageBuffer | vk::BufferUsageFlags::IndirectBuffer,
                "indirect_cmds_cutout",
            );
            indirect_cutout_buffers.push(b);
            indirect_cutout_allocs.push(a);

            let (b, a) = create_count_buffer("draw_count_cutout");
            count_cutout_buffers.push(b);
            count_cutout_allocs.push(a);

            let (b, a) = util::create_host_buffer(
                device,
                allocator,
                frustum_size,
                vk::BufferUsageFlags::UniformBuffer,
                "frustum_ubo",
            );
            frustum_buffers.push(b);
            frustum_allocs.push(a);
        }

        let compute_desc_layout = create_cull_desc_layout(device);
        let layout_info = vk::PipelineLayoutCreateInfo {
            set_layout_count: 1,
            set_layouts: &compute_desc_layout,
            ..Default::default()
        };
        let compute_layout = device
            .create_pipeline_layout(&layout_info, None)
            .expect("failed to create compute pipeline layout");

        let comp_spv = shader::include_spirv!("cull.comp.spv");
        let comp_module = shader::create_shader_module(device, comp_spv);
        let stage = vk::PipelineShaderStageCreateInfo {
            stage: vk::ShaderStageFlags::Compute,
            module: comp_module,
            name: c"main".as_ptr(),
            ..Default::default()
        };
        let pipe_info = [vk::ComputePipelineCreateInfo {
            stage,
            layout: compute_layout,
            ..Default::default()
        }];
        let mut compute_pipeline = vk::Pipeline::null();
        device
            .create_compute_pipelines(
                vk::PipelineCache::null(),
                &pipe_info,
                None,
                std::slice::from_mut(&mut compute_pipeline),
            )
            .expect("failed to create cull pipeline");
        device.destroy_shader_module(comp_module, None);

        let pool_sizes = [
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::StorageBuffer,
                // meta + solid indirect/count + cutout indirect/count = 5 per frame.
                descriptor_count: 5 * MAX_FRAMES_IN_FLIGHT as u32,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::UniformBuffer,
                descriptor_count: MAX_FRAMES_IN_FLIGHT as u32,
            },
        ];
        let pool_info = vk::DescriptorPoolCreateInfo {
            max_sets: MAX_FRAMES_IN_FLIGHT as u32,
            pool_size_count: pool_sizes.len() as u32,
            pool_sizes: pool_sizes.as_ptr(),
            ..Default::default()
        };
        let compute_pool = device
            .create_descriptor_pool(&pool_info, None)
            .expect("failed to create cull desc pool");

        let layouts: Vec<_> = (0..MAX_FRAMES_IN_FLIGHT)
            .map(|_| compute_desc_layout)
            .collect();
        let alloc_info = vk::DescriptorSetAllocateInfo {
            descriptor_pool: compute_pool,
            descriptor_set_count: layouts.len() as u32,
            set_layouts: layouts.as_ptr(),
            ..Default::default()
        };
        let mut compute_sets = vec![vk::DescriptorSet::null(); layouts.len()];
        device
            .allocate_descriptor_sets(&alloc_info, &mut compute_sets)
            .expect("failed to allocate cull desc sets");

        for i in 0..MAX_FRAMES_IN_FLIGHT {
            let (meta_info, mut meta_write) = desc_write(
                compute_sets[i],
                0,
                vk::DescriptorType::StorageBuffer,
                meta_buffers[i],
                meta_size,
            );

            let (frustum_info, mut frustum_write) = desc_write(
                compute_sets[i],
                1,
                vk::DescriptorType::UniformBuffer,
                frustum_buffers[i],
                frustum_size,
            );

            let (indirect_info, mut indirect_write) = desc_write(
                compute_sets[i],
                2,
                vk::DescriptorType::StorageBuffer,
                indirect_buffers[i],
                indirect_size,
            );

            let (count_info, mut count_write) = desc_write(
                compute_sets[i],
                3,
                vk::DescriptorType::StorageBuffer,
                count_buffers[i],
                count_size,
            );

            let (indirect_c_info, mut indirect_c_write) = desc_write(
                compute_sets[i],
                4,
                vk::DescriptorType::StorageBuffer,
                indirect_cutout_buffers[i],
                indirect_size,
            );

            let (count_c_info, mut count_c_write) = desc_write(
                compute_sets[i],
                5,
                vk::DescriptorType::StorageBuffer,
                count_cutout_buffers[i],
                count_size,
            );

            meta_write.buffer_info = meta_info.as_ptr();
            frustum_write.buffer_info = frustum_info.as_ptr();
            indirect_write.buffer_info = indirect_info.as_ptr();
            count_write.buffer_info = count_info.as_ptr();
            indirect_c_write.buffer_info = indirect_c_info.as_ptr();
            count_c_write.buffer_info = count_c_info.as_ptr();

            let writes = [
                meta_write,
                frustum_write,
                indirect_write,
                count_write,
                indirect_c_write,
                count_c_write,
            ];

            device.update_descriptor_sets(&writes, &[]);
        }

        // Fixed per-frame mapped buffers: edit upload never allocates, submits
        // a transfer, waits a GPU fence, or overwrites another in-flight slot.
        let edit_frames = (0..MAX_FRAMES_IN_FLIGHT)
            .map(|_| EditFrame {
                vertices: util::create_host_buffer(
                    device,
                    allocator,
                    (super::edit::MAX_EDIT_VERTICES * size_of::<PackedVertex>()) as u64,
                    vk::BufferUsageFlags::VertexBuffer,
                    "edit_vertices",
                ),
                indices: util::create_host_buffer(
                    device,
                    allocator,
                    (super::edit::MAX_EDIT_INDICES * size_of::<u32>()) as u64,
                    vk::BufferUsageFlags::IndexBuffer,
                    "edit_indices",
                ),
                meta: util::create_host_buffer(
                    device,
                    allocator,
                    (super::edit::MAX_EDIT_CELLS * size_of::<ChunkMeta>()) as u64,
                    vk::BufferUsageFlags::VertexBuffer,
                    "edit_meta",
                ),
                draws: Vec::with_capacity(super::edit::MAX_EDIT_CELLS),
            })
            .collect();

        Self {
            edits: Default::default(),
            edit_frames,
            max_meta,
            vertex_buffer,
            vertex_alloc,
            index_buffer,
            index_alloc,
            staging_buffer,
            staging_alloc,
            staging_size,
            transfer_pool,
            transfer_cmd,
            transfer_fence,
            use_staging,
            vtx_free,
            idx_free,
            chunks: HashMap::new(),
            empty_epochs: HashMap::new(),
            chunk_visibility: HashMap::new(),
            cached_meta: Vec::new(),
            meta_dirty: true,
            fade_until: std::time::Instant::now(),
            last_sort_cam: DVec3::MAX,
            meta_upload_pending: 0,
            compute_pipeline,
            compute_layout,
            compute_desc_layout,
            compute_pool,
            compute_sets,
            meta_buffers,
            meta_allocs,
            indirect_buffers,
            indirect_allocs,
            count_buffers,
            count_allocs,
            indirect_cutout_buffers,
            indirect_cutout_allocs,
            count_cutout_buffers,
            count_cutout_allocs,
            frustum_buffers,
            frustum_allocs,
            fade_enabled: false,
            last_draw_count: 0,
            frame_seq: 0,
            pending_free: VecDeque::new(),
            trace_state,
        }
    }

    /// Sections drawn last time this frame slot ran (post frustum + occlusion
    /// cull). Read back from the GPU count buffer, so it lags a few frames.
    pub fn sections_drawn(&self) -> u32 {
        self.last_draw_count
    }

    /// Whether `pos`'s column is near enough to the eye to render opaque
    /// immediately (a nearby column never fades in).
    fn column_nearby(&self, pos: ChunkPos, eye: DVec3) -> bool {
        !self.fade_enabled || column_is_near(pos, eye)
    }

    /// Submit the accumulated staging copies as a single transfer and block on
    /// a fence until it completes. One fence wait per call replaces the old
    /// per-mesh `queue.wait_idle`, so a frame's uploads synchronize once
    /// instead of once per mesh.
    fn flush_transfer(
        &mut self,
        device: &vk::Device,
        queue: vk::Queue,
        copy_v: &[vk::BufferCopy],
        copy_i: &[vk::BufferCopy],
    ) {
        if copy_v.is_empty() && copy_i.is_empty() {
            return;
        }
        let begin = vk::CommandBufferBeginInfo {
            flags: vk::CommandBufferUsageFlags::OneTimeSubmit,
            ..Default::default()
        };
        self.transfer_cmd.begin(&begin).unwrap();
        if !copy_v.is_empty() {
            self.transfer_cmd
                .copy_buffer(self.staging_buffer, self.vertex_buffer, copy_v);
        }
        if !copy_i.is_empty() {
            self.transfer_cmd
                .copy_buffer(self.staging_buffer, self.index_buffer, copy_i);
        }
        self.transfer_cmd.end().unwrap();
        let submit = [vk::SubmitInfo {
            command_buffer_count: 1,
            command_buffers: &self.transfer_cmd.handle(),
            ..Default::default()
        }];
        device.reset_fences(&[self.transfer_fence]).unwrap();
        queue.submit(&submit, self.transfer_fence).unwrap();
        device
            .wait_for_fences(&[self.transfer_fence], true, u64::MAX)
            .unwrap();
    }

    /// Upload a batch of mesh results, each replacing the sections in its
    /// `mesh.replaced` range. Staging copies for the whole batch are coalesced
    /// into as few transfers as the staging buffer holds (one per overflow,
    /// plus a final flush), each synchronized by a single fence wait — so a
    /// streaming frame stalls once, not once per mesh. Returns, per mesh
    /// that hit pool exhaustion, the section indices that were dropped and
    /// need re-meshing.
    pub fn upload_batch(
        &mut self,
        device: &vk::Device,
        allocator: &Arc<Mutex<Allocator>>,
        queue: vk::Queue,
        meshes: &[ChunkMeshData],
    ) -> Vec<(ChunkPos, Vec<i32>)> {
        let mut needs_remesh: Vec<(ChunkPos, Vec<i32>)> = Vec::new();
        if meshes.is_empty() {
            return needs_remesh;
        }

        // Sub-allocate an exact-size vertex + index slice for each non-empty
        // section. Indices stay section-local and `vertex_offset` rebases the draw,
        // so no packing or rebasing is needed — just one slice per section.
        struct Plan<'a> {
            section_index: i32,
            verts: &'a [PackedVertex],
            indices: &'a [u32],
            water_indices: &'a [u32],
            vtx_off: u32,
            idx_off: u32,
            solid_index_count: u32,
            aabb: ChunkAABB,
            origin: [i32; 3],
            emitted_chests: &'a [EmittedChest],
        }

        // Retired slices only reclaim in `begin_frame`; if rendering is paused
        // while meshing continues (e.g. minimized window) the backlog grows
        // unbounded. Past a sane bound, force a GPU wait and reclaim it all.
        const PENDING_FREE_DRAIN_THRESHOLD: usize = 8192;
        if self.pending_free.len() > PENDING_FREE_DRAIN_THRESHOLD {
            device.wait_idle().ok();
            while let Some((_, slice)) = self.pending_free.pop_front() {
                self.free_slice(slice);
            }
        }

        let staging_half = self.staging_size as usize / 2;
        // Copies accumulated for the current (not-yet-submitted) transfer, and the
        // running write cursors into each half of the staging buffer.
        let mut copy_v: Vec<vk::BufferCopy> = Vec::new();
        let mut copy_i: Vec<vk::BufferCopy> = Vec::new();
        let mut stg_v = 0usize;
        let mut stg_i = 0usize;

        for mesh in meshes {
            // The covered sections this job is authoritative for: reject any where
            // a newer upload (higher epoch) already landed. See
            // `ChunkMeshData::upload_epoch`.
            let accepted: std::collections::HashSet<i32> = mesh
                .replaced
                .clone()
                .filter(|si| {
                    let stored = self
                        .chunks
                        .get(&mesh.pos)
                        .and_then(|c| c.sections.iter().find(|s| s.section_index == *si))
                        .map(|s| s.epoch)
                        .or_else(|| self.empty_epochs.get(&(mesh.pos, *si)).copied());
                    epoch_accepts(stored, mesh.upload_epoch)
                })
                .collect();

            // Track replacement fade; on pool exhaustion old draws are retired
            // after failed allocations so their slices can be reclaimed.
            let was_present: std::collections::HashSet<i32> = self
                .chunks
                .get(&mesh.pos)
                .into_iter()
                .flat_map(|c| c.sections.iter())
                .filter(|s| accepted.contains(&s.section_index))
                .map(|s| s.section_index)
                .collect();

            let upload_secs: Vec<&SectionMesh> = mesh
                .sections
                .iter()
                .filter(|s| accepted.contains(&s.section_index))
                .collect();

            if upload_secs.is_empty() && accepted.is_empty() {
                continue;
            }

            // Inspect every accepted section before allocating: pool exhaustion
            // can stop planning before a later section exceeds staging capacity.
            let staging_too_large = self.use_staging
                && upload_secs.iter().any(|s| {
                    staging_upload_section_too_large(
                        s.vertices.len(),
                        s.indices.len() + s.water_indices.len(),
                        staging_half,
                    )
                });
            let mut plans: Vec<Plan> = Vec::with_capacity(upload_secs.len());
            // (vtx_off, vtx_len, idx_off, idx_len) taken for this mesh, for
            // rollback if the pool runs out partway through a column.
            let mut taken: Vec<(u32, u32, u32, u32)> = Vec::new();
            let mut pool_full = false;
            for sec in &upload_secs {
                let vcount = sec.vertices.len() as u32;
                // Opaque and water indices share one slice (opaque first, water after).
                let icount = (sec.indices.len() + sec.water_indices.len()) as u32;
                if vcount == 0 || icount == 0 {
                    continue;
                }
                let Some(vtx_off) = self.vtx_free.alloc(vcount) else {
                    self.free_slices(&taken);
                    tracing::debug!("Vertex pool full, skipping {:?}", mesh.pos);
                    pool_full = true;
                    break;
                };
                let Some(idx_off) = self.idx_free.alloc(icount) else {
                    self.vtx_free.free_region(vtx_off, vcount);
                    self.free_slices(&taken);
                    tracing::debug!("Index pool full, skipping {:?}", mesh.pos);
                    pool_full = true;
                    break;
                };
                taken.push((vtx_off, vcount, idx_off, icount));
                plans.push(Plan {
                    section_index: sec.section_index,
                    verts: &sec.vertices,
                    indices: &sec.indices,
                    water_indices: &sec.water_indices,
                    vtx_off,
                    idx_off,
                    solid_index_count: sec.solid_index_count,
                    aabb: sec.aabb,
                    origin: [
                        mesh.pos.x * 16,
                        mesh.min_y + sec.section_index * 16,
                        mesh.pos.z * 16,
                    ],
                    emitted_chests: &sec.emitted_chests,
                });
            }
            if pool_full || staging_too_large {
                if retire_on_failed_upload(pool_full, staging_too_large) {
                    // Release old geometry and chest claims; the next remesh can
                    // allocate after the in-flight frame fences reclaim its slices.
                    let freed = swap_accepted(
                        &mut self.chunks,
                        &mut self.empty_epochs,
                        mesh.pos,
                        &accepted,
                        &Default::default(),
                        mesh.upload_epoch,
                    );
                    self.retire_slices(freed);
                    self.meta_dirty = true;
                } else {
                    // An oversized staging section cannot be uploaded even if
                    // another section exhausted the pool. Roll back every new
                    // slice and leave all old draws, epochs and chest poses intact.
                    self.free_slices(&taken);
                }
                needs_remesh.push((mesh.pos, accepted.iter().copied().collect()));
                continue;
            }
            // On success, retire accepted old draws. Empty sections get a
            // tombstone epoch; staging-size failures leave old draws intact.
            let planned: std::collections::HashSet<_> =
                plans.iter().map(|p| p.section_index).collect();
            let freed = swap_accepted(
                &mut self.chunks,
                &mut self.empty_epochs,
                mesh.pos,
                &accepted,
                &planned,
                mesh.upload_epoch,
            );
            self.retire_slices(freed);
            self.meta_dirty = true;
            if mesh.timing.is_some() {
                self.edits.uploaded(
                    mesh.pos,
                    &accepted,
                    mesh.content_gen,
                    mesh.column_revision,
                    mesh.upload_epoch,
                );
            }
            if plans.is_empty() {
                continue;
            }

            if self.use_staging {
                for p in &plans {
                    let vbytes = p.verts.len() * VERTEX_SIZE as usize;
                    let opaque: &[u8] = bytemuck::cast_slice(p.indices);
                    let water: &[u8] = bytemuck::cast_slice(p.water_indices);
                    let ibytes = opaque.len() + water.len();
                    // Flush between sections, not chunks: a tall column can exceed
                    // staging capacity while each section still fits.
                    if staging_needs_flush(stg_v, stg_i, vbytes, ibytes, staging_half) {
                        self.flush_transfer(device, queue, &copy_v, &copy_i);
                        copy_v.clear();
                        copy_i.clear();
                        stg_v = 0;
                        stg_i = 0;
                    }
                    let buf = self.staging_alloc.mapped_slice_mut().unwrap();
                    write_verts(buf, stg_v, p.verts);
                    copy_v.push(vk::BufferCopy {
                        src_offset: stg_v as u64,
                        dst_offset: p.vtx_off as u64 * VERTEX_SIZE,
                        size: vbytes as u64,
                    });
                    stg_v += vbytes;

                    let off = staging_half + stg_i;
                    buf[off..off + opaque.len()].copy_from_slice(opaque);
                    buf[off + opaque.len()..off + ibytes].copy_from_slice(water);
                    copy_i.push(vk::BufferCopy {
                        src_offset: off as u64,
                        dst_offset: p.idx_off as u64 * INDEX_SIZE,
                        size: ibytes as u64,
                    });
                    stg_i += ibytes;
                }
            } else {
                {
                    let vbuf = self.vertex_alloc.mapped_slice_mut().unwrap();
                    for p in &plans {
                        let base = p.vtx_off as usize * VERTEX_SIZE as usize;
                        write_verts(vbuf, base, p.verts);
                    }
                }
                {
                    let ibuf = self.index_alloc.mapped_slice_mut().unwrap();
                    for p in &plans {
                        let opaque: &[u8] = bytemuck::cast_slice(p.indices);
                        let water: &[u8] = bytemuck::cast_slice(p.water_indices);
                        let off = p.idx_off as usize * INDEX_SIZE as usize;
                        ibuf[off..off + opaque.len()].copy_from_slice(opaque);
                        ibuf[off + opaque.len()..off + opaque.len() + water.len()]
                            .copy_from_slice(water);
                    }
                }
            }

            for sec in &upload_secs {
                let Some(plan) = plans.iter().find(|p| p.section_index == sec.section_index) else {
                    continue;
                };
                for record in &sec.trace {
                    let vertex_start = record["vertexStart"].as_u64().unwrap_or(0) as usize;
                    let vertex_count = record["vertexCount"].as_u64().unwrap_or(0) as usize;
                    let byte_start = vertex_start.saturating_mul(VERTEX_SIZE as usize);
                    let byte_end = byte_start
                        .saturating_add(vertex_count.saturating_mul(VERTEX_SIZE as usize));
                    let bytes: &[u8] = bytemuck::cast_slice(plan.verts);
                    let payload = bytes.get(byte_start..byte_end).unwrap_or(&[]);
                    use sha2::{Digest, Sha256};
                    let hash = Sha256::digest(payload);
                    let index_base = record["indexStartFinal"].as_u64().unwrap_or(0) as usize;
                    let index_count = record["indexCount"].as_u64().unwrap_or(0) as usize;
                    let full_indices = if record["indexList"].as_str() == Some("cutout") {
                        let mut values =
                            Vec::with_capacity(plan.indices.len() + plan.water_indices.len());
                        values.extend_from_slice(plan.indices);
                        values
                    } else {
                        plan.indices.to_vec()
                    };
                    let index_bytes: &[u8] = bytemuck::cast_slice(
                        full_indices
                            .get(index_base..index_base.saturating_add(index_count))
                            .unwrap_or(&[]),
                    );
                    let index_hash = Sha256::digest(index_bytes);
                    let mut upload = record.clone();
                    upload["meshPos"] = json!([mesh.pos.x, mesh.pos.z]);
                    upload["sectionGeneration"] = json!(mesh.content_gen);
                    upload["uploadEpoch"] = json!(mesh.upload_epoch);
                    upload["sectionVtxByteOffset"] = json!(plan.vtx_off as u64 * VERTEX_SIZE);
                    upload["actualVtxByteOffset"] = json!(
                        plan.vtx_off as u64 * VERTEX_SIZE + vertex_start as u64 * VERTEX_SIZE
                    );
                    upload["actualIndexByteOffset"] =
                        json!((plan.idx_off as u64 + index_base as u64) * INDEX_SIZE);
                    let draw_first_index = if record["indexList"].as_str() == Some("cutout") {
                        plan.idx_off + plan.solid_index_count
                    } else {
                        plan.idx_off
                    };
                    let draw_index_count = if record["indexList"].as_str() == Some("cutout") {
                        plan.indices.len() as u32 - plan.solid_index_count
                    } else {
                        plan.solid_index_count
                    };
                    upload["drawIndirectSection"] = json!({
                        "firstIndex": draw_first_index,
                        "indexCount": draw_index_count,
                        "vertexOffset": plan.vtx_off as i32,
                        "instanceCount": 1,
                        "firstInstance": "section meta index assigned during cached draw-list rebuild",
                    });
                    upload["stride"] = json!(VERTEX_SIZE);
                    upload["attributeFormatOffsets"] = json!({
                        "pos": {"format": "R16G16Unorm+R16Unorm", "offset": 0},
                        "uv": {"format": "R16G16Uint", "offset": 6},
                        "sprite": {"format": "R16Uint", "offset": 10},
                        "lightTint": {"format": "R8G8B8A8Unorm", "offset": 12},
                    });
                    upload["uploadPayloadSha256"] =
                        json!(hash.iter().map(|b| format!("{b:02x}")).collect::<String>());
                    upload["uploadIndexPayloadSha256"] = json!(
                        index_hash
                            .iter()
                            .map(|b| format!("{b:02x}"))
                            .collect::<String>()
                    );
                    upload["uploadPath"] = json!(if self.use_staging {
                        "ChunkMeshData -> staging buffer -> vkCmdCopyBuffer -> device-local vertex/index pool"
                    } else {
                        "ChunkMeshData -> mapped host-visible vertex/index pool"
                    });
                    self.trace_state.record(upload);
                }
            }

            let now = std::time::Instant::now();
            let new_sections = plans.iter().map(|p| SectionAlloc {
                section_index: p.section_index,
                aabb: p.aabb,
                origin: p.origin,
                first_index: p.idx_off,
                index_count: p.indices.len() as u32,
                solid_index_count: p.solid_index_count,
                water_first_index: p.idx_off + p.indices.len() as u32,
                water_index_count: p.water_indices.len() as u32,
                idx_len: (p.indices.len() + p.water_indices.len()) as u32,
                vertex_offset: p.vtx_off as i32,
                vtx_len: p.verts.len() as u32,
                uploaded_at: section_upload_time(
                    now,
                    was_present.contains(&p.section_index),
                    !p.emitted_chests.is_empty(),
                ),
                epoch: mesh.upload_epoch,
                emitted_chests: p.emitted_chests.to_vec(),
            });

            // Freshly revealed sections fade in, so extend the fade window the
            // cull's O(1) check reads; re-meshed-only uploads swap instantly.
            // Nearby columns never fade, so extending for them only forces
            // redundant rebuilds — skip them. `last_sort_cam` is the camera the
            // draw list is keyed to (unset => far, the safe default).
            let revealed = plans
                .iter()
                .any(|p| !was_present.contains(&p.section_index) && p.emitted_chests.is_empty());
            if revealed && !self.column_nearby(mesh.pos, self.last_sort_cam) {
                let dur = std::time::Duration::from_secs_f32(FADE_DURATION_MS / 1000.0);
                self.fade_until = self.fade_until.max(now + dur);
            }

            self.chunks
                .entry(mesh.pos)
                .or_insert_with(|| ChunkAlloc {
                    sections: Vec::new(),
                })
                .sections
                .extend(new_sections);
        }

        // Flush whatever remains accumulated from the last (or only) batch.
        if self.use_staging {
            self.flush_transfer(device, queue, &copy_v, &copy_i);
        }

        let total_sections: usize = self.chunks.values().map(|c| c.sections.len()).sum();
        self.ensure_meta_capacity(device, allocator, total_sections);

        needs_remesh
    }

    /// Grow the per-frame meta and indirect buffers so they can hold `needed`
    /// section draws. No-op while capacity suffices.
    fn ensure_meta_capacity(
        &mut self,
        device: &vk::Device,
        allocator: &Arc<Mutex<Allocator>>,
        needed: usize,
    ) {
        if needed <= self.max_meta {
            return;
        }
        let new_max = (needed.saturating_mul(3) / 2)
            .next_power_of_two()
            .max(self.max_meta * 2);

        // The meta/indirect buffers are referenced by every in-flight frame's
        // descriptor set; wait the GPU out before freeing them.
        device.wait_idle().ok();

        {
            let mut alloc = allocator.lock().unwrap();
            for i in 0..MAX_FRAMES_IN_FLIGHT {
                device.destroy_buffer(self.meta_buffers[i], None);
                alloc
                    .free(std::mem::replace(&mut self.meta_allocs[i], unsafe {
                        std::mem::zeroed()
                    }))
                    .ok();
                device.destroy_buffer(self.indirect_buffers[i], None);
                alloc
                    .free(std::mem::replace(&mut self.indirect_allocs[i], unsafe {
                        std::mem::zeroed()
                    }))
                    .ok();
                device.destroy_buffer(self.indirect_cutout_buffers[i], None);
                alloc
                    .free(std::mem::replace(
                        &mut self.indirect_cutout_allocs[i],
                        unsafe { std::mem::zeroed() },
                    ))
                    .ok();
            }
        }

        let meta_size = (new_max * size_of::<ChunkMeta>()) as u64;
        let indirect_size = (new_max * size_of::<DrawCommand>()) as u64;
        for i in 0..MAX_FRAMES_IN_FLIGHT {
            let (b, a) = util::create_host_buffer(
                device,
                allocator,
                meta_size,
                vk::BufferUsageFlags::StorageBuffer | vk::BufferUsageFlags::VertexBuffer,
                "chunk_meta",
            );
            self.meta_buffers[i] = b;
            self.meta_allocs[i] = a;

            let (b, a) = util::create_host_buffer(
                device,
                allocator,
                indirect_size,
                vk::BufferUsageFlags::StorageBuffer | vk::BufferUsageFlags::IndirectBuffer,
                "indirect_cmds",
            );
            self.indirect_buffers[i] = b;
            self.indirect_allocs[i] = a;

            let (b, a) = util::create_host_buffer(
                device,
                allocator,
                indirect_size,
                vk::BufferUsageFlags::StorageBuffer | vk::BufferUsageFlags::IndirectBuffer,
                "indirect_cmds_cutout",
            );
            self.indirect_cutout_buffers[i] = b;
            self.indirect_cutout_allocs[i] = a;

            let (meta_info, mut meta_write) = desc_write(
                self.compute_sets[i],
                0,
                vk::DescriptorType::StorageBuffer,
                self.meta_buffers[i],
                meta_size,
            );
            let (indirect_info, mut indirect_write) = desc_write(
                self.compute_sets[i],
                2,
                vk::DescriptorType::StorageBuffer,
                self.indirect_buffers[i],
                indirect_size,
            );
            let (indirect_c_info, mut indirect_c_write) = desc_write(
                self.compute_sets[i],
                4,
                vk::DescriptorType::StorageBuffer,
                self.indirect_cutout_buffers[i],
                indirect_size,
            );
            meta_write.buffer_info = meta_info.as_ptr();
            indirect_write.buffer_info = indirect_info.as_ptr();
            indirect_c_write.buffer_info = indirect_c_info.as_ptr();
            device.update_descriptor_sets(&[meta_write, indirect_write, indirect_c_write], &[]);
        }

        self.max_meta = new_max;
    }

    /// Return one slice's vertex and index ranges to the pools.
    fn free_slice(&mut self, (vo, vl, io, il): (u32, u32, u32, u32)) {
        self.vtx_free.free_region(vo, vl);
        self.idx_free.free_region(io, il);
    }

    /// Return slices immediately. Only safe for slices never submitted to a
    /// frame (e.g. rolling back allocations made earlier in the same `upload`);
    /// slices that may still be drawn by an in-flight frame must go through
    /// `retire_slices`.
    fn free_slices(&mut self, slices: &[(u32, u32, u32, u32)]) {
        for &slice in slices {
            self.free_slice(slice);
        }
    }

    /// Defer returning slices to the pools until `MAX_FRAMES_IN_FLIGHT` frames
    /// have passed, so the GPU can't still be reading them from an in-flight
    /// frame. Use for slices that were potentially drawn (re-mesh replacement,
    /// chunk unload).
    fn retire_slices(&mut self, slices: impl IntoIterator<Item = (u32, u32, u32, u32)>) {
        let retire_at = self.frame_seq + MAX_FRAMES_IN_FLIGHT as u64;
        for slice in slices {
            self.pending_free.push_back((retire_at, slice));
        }
    }

    /// Advance one frame and reclaim any slices whose retirement deadline has
    /// passed. Call once per rendered frame, right after the frame's fence has
    /// been waited (that wait guarantees the frame from `MAX_FRAMES_IN_FLIGHT`
    /// ago — and everything before it — is done on the GPU).
    pub fn begin_frame(&mut self) {
        self.frame_seq += 1;
        while self
            .pending_free
            .front()
            .is_some_and(|&(retire_at, _)| retire_at <= self.frame_seq)
        {
            let (_, slice) = self.pending_free.pop_front().unwrap();
            self.free_slice(slice);
        }
    }

    /// The accepted resident draw's chest pose, not the latest CPU snapshot.
    /// Does not imply the section is currently visible (fade/cull are
    /// separate).
    pub fn resident_chest_open(&self, pos: &BlockPos) -> Option<bool> {
        let chunk = ChunkPos::new(pos.x.div_euclid(16), pos.z.div_euclid(16));
        resident_chest_open(self.chunks.get(&chunk)?.sections.iter(), pos)
    }

    pub fn remove(&mut self, pos: &ChunkPos) {
        self.edits
            .cells
            .retain(|p, _| p.x.div_euclid(16) != pos.x || p.z.div_euclid(16) != pos.z);
        if let Some(alloc) = forget_column(&mut self.chunks, &mut self.empty_epochs, pos) {
            self.retire_slices(alloc.sections.iter().map(|sec| {
                (
                    sec.vertex_offset as u32,
                    sec.vtx_len,
                    sec.first_index,
                    sec.idx_len,
                )
            }));
            self.meta_dirty = true;
        }
    }

    pub fn clear(&mut self) {
        self.edits.cells.clear();
        clear_columns(&mut self.chunks, &mut self.empty_epochs);
        self.vtx_free.reset();
        self.idx_free.reset();
        self.pending_free.clear();
        self.cached_meta.clear();
        self.meta_dirty = true;
        self.fade_enabled = false;
    }

    pub fn chunk_count(&self) -> u32 {
        self.chunks.len() as u32
    }

    /// Accepted GPU section, including empty sections with no draw allocation.
    pub fn has_section(&self, pos: &ChunkPos, section: i32, world_empty: bool) -> bool {
        section_resident(&self.chunks, &self.empty_epochs, pos, section, world_empty)
    }

    /// Push the CPU visibility graph's per-column visible-section masks.
    /// Columns not present default to fully visible, so the cull only omits
    /// sections the graph proved occluded.
    pub fn set_chunk_visibility(&mut self, vis: HashMap<ChunkPos, u32>) {
        self.chunk_visibility = vis;
        self.meta_dirty = true;
    }

    /// `anchor` must be the same `Camera::anchor()` this frame's
    /// `CameraUniform` was built with, so the cull's block/fraction split
    /// matches the vertex shader's.
    pub fn dispatch_cull(
        &mut self,
        cmd: vk::CommandBuffer,
        frame: usize,
        frustum: &[[f32; 4]; 6],
        anchor: DVec3,
        eye: DVec3,
    ) {
        if self.chunks.is_empty() {
            return;
        }

        let now = std::time::Instant::now();
        // Re-sort only once the camera moves ~8 blocks; front-to-back order is an
        // early-Z optimization, so finer staleness is harmless.
        const SORT_RECAM_SQ: f64 = 64.0;

        // A fade in flight changes per-section visibility every frame, so the draw
        // list must rebuild; otherwise it only changes on edits/loads/visibility
        // (`meta_dirty`). The fade check is O(1) against `fade_until`.
        let any_fading = self.fade_enabled && now < self.fade_until;
        let content_changed = self.meta_dirty || any_fading;

        if content_changed {
            self.cached_meta.clear();
            for (pos, alloc) in self.chunks.iter() {
                // Near columns never fade; otherwise each section fades on its own
                // timer (X/Z distance is per-column).
                let nearby = self.column_nearby(*pos, eye);

                // CPU omission: the visibility graph's mask skips sections proven
                // occluded, so they never reach the GPU cull (absent => all draw).
                let col_vis = self.chunk_visibility.get(pos).copied().unwrap_or(u32::MAX);

                for sec in &alloc.sections {
                    if col_vis & (1u32 << sec.section_index) == 0 {
                        continue;
                    }
                    let vis = Self::section_visibility(nearby, sec, now);
                    self.cached_meta.push(ChunkMeta {
                        aabb_min: sec.aabb.min,
                        aabb_max: sec.aabb.max,
                        index_count: sec.index_count,
                        first_index: sec.first_index,
                        vertex_offset: sec.vertex_offset,
                        visibility: vis.to_bits(),
                        origin: sec.origin,
                        solid_index_count: sec.solid_index_count,
                    });
                }
            }
            self.meta_dirty = false;
        }

        let cam_moved = (eye - self.last_sort_cam).length_squared() > SORT_RECAM_SQ;
        if content_changed || cam_moved {
            // Section centers rebased against the eye in f64, for precision at
            // extreme coordinates.
            let center_dist_sq = |m: &ChunkMeta| {
                let center = DVec3::new(
                    ((m.aabb_min[0] + m.aabb_max[0]) * 0.5) as f64,
                    ((m.aabb_min[1] + m.aabb_max[1]) * 0.5) as f64,
                    ((m.aabb_min[2] + m.aabb_max[2]) * 0.5) as f64,
                );
                (origin_dvec(m.origin) + center - eye).length_squared()
            };
            self.cached_meta.sort_unstable_by(|a, b| {
                center_dist_sq(a)
                    .partial_cmp(&center_dist_sq(b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            self.last_sort_cam = eye;
            // Draw list reordered: every frame slot's meta buffer needs the refresh.
            self.meta_upload_pending = MAX_FRAMES_IN_FLIGHT as u32;
        }

        let count = self.cached_meta.len() as u32;
        // Each frame slot has its own meta buffer; copy only into slots that
        // haven't yet seen the current draw list. Steady state stops copying.
        if self.meta_upload_pending > 0 {
            let meta_bytes = bytemuck::cast_slice(&self.cached_meta);
            self.meta_allocs[frame].mapped_slice_mut().unwrap()[..meta_bytes.len()]
                .copy_from_slice(meta_bytes);
            self.meta_upload_pending -= 1;
        }

        let frustum_data = FrustumData {
            planes: *frustum,
            chunk_count: count,
            cam_block: anchor.as_ivec3().to_array(),
            frac: (eye - anchor).as_vec3().to_array(),
            _pad: 0.0,
        };
        let frustum_bytes = bytemuck::bytes_of(&frustum_data);
        self.frustum_allocs[frame].mapped_slice_mut().unwrap()[..frustum_bytes.len()]
            .copy_from_slice(frustum_bytes);

        // This frame slot's GPU work has completed (fence-waited at frame start),
        // so the count buffers still hold their previous cull result; capture the
        // total (solid + cutout draws) for the debug overlay before clearing them.
        {
            let read_and_clear = |a: &mut Allocation| {
                let s = a.mapped_slice_mut().unwrap();
                let n = u32::from_ne_bytes([s[0], s[1], s[2], s[3]]);
                s[..4].copy_from_slice(&0u32.to_ne_bytes());
                n
            };
            self.last_draw_count = read_and_clear(&mut self.count_allocs[frame])
                + read_and_clear(&mut self.count_cutout_allocs[frame]);
        }

        // macOS draws the whole indirect buffer (no drawIndirectCount), so slots
        // the cull shader leaves unfilled must read as no-op draws, not stale data.
        #[cfg(target_os = "macos")]
        for a in [
            &mut self.indirect_allocs[frame],
            &mut self.indirect_cutout_allocs[frame],
        ] {
            a.mapped_slice_mut().unwrap().fill(0);
        }

        cmd.bind_pipeline(vk::PipelineBindPoint::Compute, self.compute_pipeline);
        cmd.bind_descriptor_sets(
            vk::PipelineBindPoint::Compute,
            self.compute_layout,
            0,
            &[self.compute_sets[frame]],
            &[],
        );
        cmd.dispatch(count.div_ceil(64), 1, 1);

        let barrier = vk::MemoryBarrier {
            src_access_mask: vk::AccessFlags::ShaderWrite,
            dst_access_mask: vk::AccessFlags::IndirectCommandRead,
            ..Default::default()
        };
        cmd.pipeline_barrier(
            vk::PipelineStageFlags::ComputeShader,
            vk::PipelineStageFlags::DrawIndirect,
            vk::DependencyFlags::empty(),
            &[barrier],
            &[],
            &[],
        );

        if !self.fade_enabled {
            self.fade_enabled = true;
        }
    }

    /// Issue one render layer's indirect draws. `cutout` selects the discard
    /// pass's draw list (drawn after `solid`, which lays down depth); the
    /// caller binds the matching pipeline first. Both layers share the
    /// vertex/index/meta buffers and the cull-written draw lists.
    pub fn draw_indirect(&self, cmd: vk::CommandBuffer, frame: usize, cutout: bool) {
        if self.chunks.is_empty() {
            return;
        }

        let max_draws = self
            .chunks
            .values()
            .map(|c| c.sections.len() as u32)
            .sum::<u32>();
        let (indirect, count) = if cutout {
            (
                self.indirect_cutout_buffers[frame],
                self.count_cutout_buffers[frame],
            )
        } else {
            (self.indirect_buffers[frame], self.count_buffers[frame])
        };

        // Binding 0: packed vertex pool. Binding 1: the meta buffer, read per
        // instance for the section origin + fade (indexed by `first_instance`).
        cmd.bind_vertex_buffers(0, &[self.vertex_buffer, self.meta_buffers[frame]], &[0, 0]);
        cmd.bind_index_buffer(self.index_buffer, 0, vk::IndexType::Uint32);
        if cfg!(target_os = "macos") {
            cmd.draw_indexed_indirect(indirect, 0, max_draws, size_of::<DrawCommand>() as u32);
        } else {
            cmd.draw_indexed_indirect_count(
                indirect,
                0,
                count,
                0,
                max_draws,
                size_of::<DrawCommand>() as u32,
            );
        }
    }

    /// Per-section fade-in factor in `[0, 1]`: near columns appear instantly,
    /// the rest ramp over [`FADE_DURATION_MS`] from their upload time. Drives
    /// both the opaque indirect meta and the water pass so they fade in
    /// together.
    fn section_visibility(nearby: bool, sec: &SectionAlloc, now: std::time::Instant) -> f32 {
        if nearby {
            return 1.0;
        }
        let elapsed_ms = now.duration_since(sec.uploaded_at).as_secs_f32() * 1000.0;
        (elapsed_ms / FADE_DURATION_MS).min(1.0)
    }

    /// Draw the translucent water of every section that survives a CPU frustum
    /// cull. Reuses the shared vertex/index buffers (water indices live right
    /// after the opaque ones in each section's slice); the caller binds the
    /// blended water pipeline first. Not GPU-culled — water sections are a
    /// small subset, so a per-section draw is cheap and keeps the opaque
    /// indirect path untouched.
    ///
    /// `anchor` must be the same `Camera::anchor()` this frame's
    /// `CameraUniform` was built with: the push-constant origins are rebased
    /// against it and the shader adds back `camera_pos` (the eye's offset
    /// from that anchor).
    ///
    /// TODO: water isn't depth-sorted, so overlapping translucent surfaces
    /// (oceans at grazing angles, water seen through water) can blend out of
    /// order.
    pub fn draw_water(
        &self,
        cmd: vk::CommandBuffer,
        layout: vk::PipelineLayout,
        frustum: &[[f32; 4]; 6],
        anchor: DVec3,
        eye: DVec3,
    ) {
        if self.chunks.is_empty() {
            return;
        }

        cmd.bind_vertex_buffers(0, &[self.vertex_buffer], &[0]);
        cmd.bind_index_buffer(self.index_buffer, 0, vk::IndexType::Uint32);

        let now = std::time::Instant::now();
        for (pos, alloc) in self.chunks.iter() {
            let col_vis = self.chunk_visibility.get(pos).copied().unwrap_or(u32::MAX);
            let nearby = self.column_nearby(*pos, eye);
            for sec in &alloc.sections {
                if sec.water_index_count == 0
                    || col_vis & (1u32 << sec.section_index) == 0
                    || !aabb_in_frustum(&sec.aabb, sec.origin, frustum, eye)
                {
                    continue;
                }
                let vis = Self::section_visibility(nearby, sec, now);
                let rel = (origin_dvec(sec.origin) - anchor).as_vec3();
                let origin_fade = [rel.x, rel.y, rel.z, vis];
                cmd.push_constants(
                    layout,
                    vk::ShaderStageFlags::Vertex,
                    0,
                    bytemuck::bytes_of(&origin_fade),
                );
                cmd.draw_indexed(
                    sec.water_index_count,
                    1,
                    sec.water_first_index,
                    sec.vertex_offset,
                    0,
                );
            }
        }
    }

    /// Call only after the existing frame-slot fence. Includes air cells in
    /// the mask, but never issues empty geometry draws.
    pub(crate) fn prepare_edits(&mut self, frame: usize) -> Vec<[i32; 4]> {
        let gpu = &mut self.edit_frames[frame];
        gpu.draws.clear();
        let mut mask = Vec::with_capacity(self.edits.cells.len());
        let mut vo = 0usize;
        let mut io = 0usize;
        for cell in self.edits.cells.values() {
            let geometry = &cell.geometry;
            let p = geometry.pos;
            mask.push([p.x, p.y, p.z, 0]);
            let mesh = &geometry.mesh;
            if mesh.indices.is_empty() {
                continue;
            }
            write_verts(
                gpu.vertices.1.mapped_slice_mut().unwrap(),
                vo * VERTEX_SIZE as usize,
                &mesh.vertices,
            );
            let bytes: &[u8] = bytemuck::cast_slice(&mesh.indices);
            let off = io * INDEX_SIZE as usize;
            gpu.indices.1.mapped_slice_mut().unwrap()[off..off + bytes.len()]
                .copy_from_slice(bytes);
            let meta = ChunkMeta {
                aabb_min: mesh.aabb.min,
                aabb_max: mesh.aabb.max,
                index_count: mesh.indices.len() as u32,
                first_index: io as u32,
                vertex_offset: vo as i32,
                visibility: (-1.0f32).to_bits(),
                origin: geometry.origin,
                solid_index_count: mesh.solid_index_count,
            };
            let instance = gpu.draws.len() as u32;
            let bytes = bytemuck::bytes_of(&meta);
            let off = instance as usize * size_of::<ChunkMeta>();
            gpu.meta.1.mapped_slice_mut().unwrap()[off..off + bytes.len()].copy_from_slice(bytes);
            gpu.draws.push((
                io as u32,
                mesh.indices.len() as u32,
                mesh.solid_index_count,
                vo as i32,
                instance,
            ));
            vo += mesh.vertices.len();
            io += mesh.indices.len();
        }
        mask
    }

    /// Reuses terrain pipelines/packing/meta. No cave-cull dependency: edits
    /// must show even before the section visibility graph has been refreshed.
    pub(crate) fn draw_edits(&self, cmd: vk::CommandBuffer, frame: usize, cutout: bool) {
        let gpu = &self.edit_frames[frame];
        if gpu.draws.is_empty() {
            return;
        }
        cmd.bind_vertex_buffers(0, &[gpu.vertices.0, gpu.meta.0], &[0, 0]);
        cmd.bind_index_buffer(gpu.indices.0, 0, vk::IndexType::Uint32);
        for &(first, total, solid, vertex, instance) in &gpu.draws {
            let (first, count) = if cutout {
                (first + solid, total - solid)
            } else {
                (first, solid)
            };
            if count > 0 {
                cmd.draw_indexed(count, 1, first, vertex, instance);
            }
        }
    }

    pub fn destroy(&mut self, device: &vk::Device, allocator: &Arc<Mutex<Allocator>>) {
        let mut alloc = allocator.lock().unwrap();
        for gpu in self.edit_frames.drain(..) {
            for (buffer, allocation) in [gpu.vertices, gpu.indices, gpu.meta] {
                device.destroy_buffer(buffer, None);
                alloc.free(allocation).ok();
            }
        }

        device.destroy_buffer(self.vertex_buffer, None);
        device.destroy_buffer(self.index_buffer, None);

        alloc
            .free(std::mem::replace(&mut self.vertex_alloc, unsafe {
                std::mem::zeroed()
            }))
            .ok();
        alloc
            .free(std::mem::replace(&mut self.index_alloc, unsafe {
                std::mem::zeroed()
            }))
            .ok();

        for i in 0..MAX_FRAMES_IN_FLIGHT {
            device.destroy_buffer(self.meta_buffers[i], None);
            device.destroy_buffer(self.indirect_buffers[i], None);
            device.destroy_buffer(self.count_buffers[i], None);
            device.destroy_buffer(self.indirect_cutout_buffers[i], None);
            device.destroy_buffer(self.count_cutout_buffers[i], None);
            device.destroy_buffer(self.frustum_buffers[i], None);

            alloc
                .free(std::mem::replace(&mut self.meta_allocs[i], unsafe {
                    std::mem::zeroed()
                }))
                .ok();
            alloc
                .free(std::mem::replace(&mut self.indirect_allocs[i], unsafe {
                    std::mem::zeroed()
                }))
                .ok();
            alloc
                .free(std::mem::replace(&mut self.count_allocs[i], unsafe {
                    std::mem::zeroed()
                }))
                .ok();
            alloc
                .free(std::mem::replace(
                    &mut self.indirect_cutout_allocs[i],
                    unsafe { std::mem::zeroed() },
                ))
                .ok();
            alloc
                .free(std::mem::replace(
                    &mut self.count_cutout_allocs[i],
                    unsafe { std::mem::zeroed() },
                ))
                .ok();
            alloc
                .free(std::mem::replace(&mut self.frustum_allocs[i], unsafe {
                    std::mem::zeroed()
                }))
                .ok();
        }
        device.destroy_buffer(self.staging_buffer, None);
        alloc
            .free(std::mem::replace(&mut self.staging_alloc, unsafe {
                std::mem::zeroed()
            }))
            .ok();
        drop(alloc);

        device.destroy_fence(self.transfer_fence, None);
        device.destroy_command_pool(self.transfer_pool, None);
        device.destroy_pipeline(self.compute_pipeline, None);
        device.destroy_pipeline_layout(self.compute_layout, None);
        device.destroy_descriptor_pool(self.compute_pool, None);
        device.destroy_descriptor_set_layout(self.compute_desc_layout, None);
    }
}

fn create_cull_desc_layout(device: &vk::Device) -> vk::DescriptorSetLayout {
    let bindings = [
        vk::DescriptorSetLayoutBinding {
            binding: 0,
            descriptor_type: vk::DescriptorType::StorageBuffer,
            descriptor_count: 1,
            stage_flags: vk::ShaderStageFlags::Compute,
            ..Default::default()
        },
        vk::DescriptorSetLayoutBinding {
            binding: 1,
            descriptor_type: vk::DescriptorType::UniformBuffer,
            descriptor_count: 1,
            stage_flags: vk::ShaderStageFlags::Compute,
            ..Default::default()
        },
        vk::DescriptorSetLayoutBinding {
            binding: 2,
            descriptor_type: vk::DescriptorType::StorageBuffer,
            descriptor_count: 1,
            stage_flags: vk::ShaderStageFlags::Compute,
            ..Default::default()
        },
        vk::DescriptorSetLayoutBinding {
            binding: 3,
            descriptor_type: vk::DescriptorType::StorageBuffer,
            descriptor_count: 1,
            stage_flags: vk::ShaderStageFlags::Compute,
            ..Default::default()
        },
        vk::DescriptorSetLayoutBinding {
            binding: 4,
            descriptor_type: vk::DescriptorType::StorageBuffer,
            descriptor_count: 1,
            stage_flags: vk::ShaderStageFlags::Compute,
            ..Default::default()
        },
        vk::DescriptorSetLayoutBinding {
            binding: 5,
            descriptor_type: vk::DescriptorType::StorageBuffer,
            descriptor_count: 1,
            stage_flags: vk::ShaderStageFlags::Compute,
            ..Default::default()
        },
    ];
    let info = vk::DescriptorSetLayoutCreateInfo {
        binding_count: bindings.len() as u32,
        bindings: bindings.as_ptr(),
        ..Default::default()
    };
    device
        .create_descriptor_set_layout(&info, None)
        .expect("failed to create cull desc layout")
}

fn desc_write(
    set: vk::DescriptorSet,
    binding: u32,
    ty: vk::DescriptorType,
    buffer: vk::Buffer,
    range: u64,
) -> (
    [vk::DescriptorBufferInfo; 1],
    vk::WriteDescriptorSet<'static>,
) {
    let info = [vk::DescriptorBufferInfo {
        buffer,
        offset: 0,
        range,
    }];

    let write = vk::WriteDescriptorSet {
        dst_set: set,
        dst_binding: binding,
        descriptor_count: 1,
        descriptor_type: ty,
        ..Default::default()
    };

    (info, write)
}

#[cfg(test)]
mod staging_tests {
    use super::*;

    fn section(si: i32, pos: BlockPos, open: bool, epoch: u64) -> SectionAlloc {
        SectionAlloc {
            section_index: si,
            aabb: ChunkAABB {
                min: [0.0; 4],
                max: [1.0; 4],
            },
            origin: [0; 3],
            first_index: 0,
            index_count: 6,
            solid_index_count: 6,
            water_first_index: 6,
            water_index_count: 0,
            idx_len: 6,
            vertex_offset: 0,
            vtx_len: 4,
            uploaded_at: std::time::Instant::now(),
            epoch,
            emitted_chests: vec![EmittedChest { pos, open }],
        }
    }

    #[test]
    fn first_chest_section_skips_fade_but_other_first_sections_keep_it() {
        let now = std::time::Instant::now();
        let chest = BlockPos::new(-1, 4, 2);
        let mut alloc = section(0, chest, false, 1);
        alloc.uploaded_at = section_upload_time(now, false, true);
        assert_eq!(
            ChunkBufferStore::section_visibility(false, &alloc, now),
            1.0
        );
        alloc.uploaded_at = section_upload_time(now, false, false);
        assert_eq!(
            ChunkBufferStore::section_visibility(false, &alloc, now),
            0.0
        );
        alloc.uploaded_at = section_upload_time(now, true, false);
        assert_eq!(
            ChunkBufferStore::section_visibility(false, &alloc, now),
            1.0
        );
    }

    #[test]
    fn camera_section_requires_matching_resident_or_accepted_empty() {
        let pos = ChunkPos::new(-1, 2);
        let chunks = HashMap::from([(
            pos,
            ChunkAlloc {
                sections: vec![section(3, BlockPos::new(-1, 48, 32), false, 1)],
            },
        )]);
        let mut empty = HashMap::new();
        assert!(section_resident(&chunks, &empty, &pos, 3, false));
        assert!(!section_resident(&chunks, &empty, &pos, 4, true));
        empty.insert((pos, 4), 1);
        // A tombstone may also mark pool exhaustion: a nonempty section needs a draw.
        assert!(!section_resident(&chunks, &empty, &pos, 4, false));
        assert!(section_resident(&chunks, &empty, &pos, 4, true));
        assert!(!section_resident(
            &chunks,
            &empty,
            &ChunkPos::new(0, 2),
            3,
            true
        ));
    }

    #[test]
    fn resident_chests_follow_accepted_draws_only() {
        let chunk = ChunkPos::new(-1, 0);
        let pos = BlockPos::new(-1, 4, 2);
        let mut chunks = HashMap::from([(
            chunk,
            ChunkAlloc {
                sections: vec![section(0, pos, false, 8)],
            },
        )]);
        let mut empty = HashMap::new();
        let query = |chunks: &HashMap<_, ChunkAlloc>| {
            resident_chest_open(chunks.get(&chunk).unwrap().sections.iter(), &pos)
        };
        assert_eq!(query(&chunks), Some(false));
        chunks.get_mut(&chunk).unwrap().sections[0].index_count = 0;
        assert_eq!(query(&chunks), None); // no indexed draw, no chest claim
        chunks.get_mut(&chunk).unwrap().sections[0].index_count = 6;
        assert!(!epoch_accepts(Some(8), 7)); // stale upload: no swap
        assert_eq!(query(&chunks), Some(false));

        let mut pool = FreeList::new(4);
        assert!(pool.alloc(4).is_some());
        assert!(pool.alloc(4).is_none()); // full pool: no swap
        assert_eq!(query(&chunks), Some(false));

        let accepted = std::collections::HashSet::from([0]);
        let planned = std::collections::HashSet::from([0]);
        assert_eq!(
            swap_accepted(&mut chunks, &mut empty, chunk, &accepted, &planned, 9).len(),
            1
        );
        chunks
            .entry(chunk)
            .or_insert_with(|| ChunkAlloc { sections: vec![] })
            .sections
            .push(section(0, pos, true, 9));
        assert_eq!(query(&chunks), Some(true));
        // Duplicate claims are ambiguous, not a positive residency result.
        chunks
            .get_mut(&chunk)
            .unwrap()
            .sections
            .push(section(1, pos, false, 9));
        assert_eq!(query(&chunks), None);
        chunks.get_mut(&chunk).unwrap().sections.pop();
        chunks.get_mut(&chunk).unwrap().sections[0]
            .emitted_chests
            .push(EmittedChest { pos, open: false });
        assert_eq!(query(&chunks), None); // duplicate within one section too
        chunks.get_mut(&chunk).unwrap().sections[0]
            .emitted_chests
            .pop();

        // Empty acceptance retires the draw and stamps the empty section.
        assert_eq!(
            swap_accepted(
                &mut chunks,
                &mut empty,
                chunk,
                &accepted,
                &Default::default(),
                10
            )
            .len(),
            1
        );
        assert!(!chunks.contains_key(&chunk));
        assert_eq!(empty.get(&(chunk, 0)), Some(&10));
        assert!(!epoch_accepts(empty.get(&(chunk, 0)).copied(), 9));
        forget_column(&mut chunks, &mut empty, &chunk);
        assert!(!empty.contains_key(&(chunk, 0))); // unload

        chunks.insert(
            chunk,
            ChunkAlloc {
                sections: vec![section(0, pos, false, 11)],
            },
        );
        empty.insert((chunk, 1), 11);
        clear_columns(&mut chunks, &mut empty);
        assert!(chunks.is_empty() && empty.is_empty());
    }

    #[test]
    fn double_halves_claim_residency_independently_across_out_of_order_uploads() {
        let (left_col, right_col) = (ChunkPos::new(-1, 0), ChunkPos::new(0, 0));
        let (left, right) = (BlockPos::new(-1, 4, 2), BlockPos::new(0, 4, 2));
        let mut chunks = HashMap::from([
            (
                left_col,
                ChunkAlloc {
                    sections: vec![section(0, left, false, 8)],
                },
            ),
            (
                right_col,
                ChunkAlloc {
                    sections: vec![section(0, right, false, 8)],
                },
            ),
        ]);
        let mut empty = HashMap::new();
        let query = |chunks: &HashMap<ChunkPos, ChunkAlloc>, col, pos| {
            chunks
                .get(&col)
                .and_then(|c| resident_chest_open(c.sections.iter(), &pos))
        };
        let accepted = std::collections::HashSet::from([0]);
        // Only the left's newer upload lands; the right is still its old pose.
        swap_accepted(&mut chunks, &mut empty, left_col, &accepted, &accepted, 10);
        chunks
            .entry(left_col)
            .or_insert_with(|| ChunkAlloc { sections: vec![] })
            .sections
            .push(section(0, left, true, 10));
        assert_eq!(query(&chunks, left_col, left), Some(true));
        assert_eq!(query(&chunks, right_col, right), Some(false));
        assert!(!epoch_accepts(Some(10), 9)); // stale left upload cannot roll pose back
        // Pool refusal does not claim the waiting right half (or suppress its BE).
        let mut pool = FreeList::new(4);
        assert!(pool.alloc(4).is_some());
        assert!(pool.alloc(1).is_none());
        swap_accepted(
            &mut chunks,
            &mut empty,
            right_col,
            &accepted,
            &Default::default(),
            11,
        );
        assert_eq!(query(&chunks, right_col, right), None);
        assert_eq!(query(&chunks, left_col, left), Some(true));
        swap_accepted(&mut chunks, &mut empty, right_col, &accepted, &accepted, 12);
        chunks
            .entry(right_col)
            .or_insert_with(|| ChunkAlloc { sections: vec![] })
            .sections
            .push(section(0, right, true, 12));
        assert_eq!(query(&chunks, right_col, right), Some(true));
        forget_column(&mut chunks, &mut empty, &left_col);
        assert_eq!(query(&chunks, left_col, left), None);
        assert_eq!(query(&chunks, right_col, right), Some(true));
    }

    #[test]
    fn full_pools_replace_after_retired_section_passes_frame_fences() {
        let chunk = ChunkPos::new(0, 0);
        let chest = BlockPos::new(1, 4, 1);
        let mut vtx = FreeList::new(8);
        let mut idx = FreeList::new(12);
        let (a_v, a_i) = (vtx.alloc(4).unwrap(), idx.alloc(6).unwrap());
        let (b_v, b_i) = (vtx.alloc(4).unwrap(), idx.alloc(6).unwrap());
        let mut a = section(0, chest, false, 8);
        a.vertex_offset = a_v as i32;
        a.first_index = a_i;
        let mut b = section(1, chest, true, 8);
        b.vertex_offset = b_v as i32;
        b.first_index = b_i;
        b.emitted_chests.clear();
        let mut chunks = HashMap::from([(
            chunk,
            ChunkAlloc {
                sections: vec![a, b],
            },
        )]);
        let mut empty = HashMap::new();
        let accepted = std::collections::HashSet::from([0]);
        // The same-size replacement cannot fit while A and B occupy both pools.
        assert_eq!((vtx.alloc(4), idx.alloc(6)), (None, None));
        let freed = swap_accepted(
            &mut chunks,
            &mut empty,
            chunk,
            &accepted,
            &Default::default(),
            9,
        );
        assert_eq!(freed, vec![(a_v, 4, a_i, 6)]);
        assert_eq!(chunks[&chunk].sections.len(), 1); // B remains resident
        assert_eq!(
            resident_chest_open(chunks[&chunk].sections.iter(), &chest),
            None
        );
        assert!(!epoch_accepts(empty.get(&(chunk, 0)).copied(), 8));
        let mut pending = VecDeque::from([(MAX_FRAMES_IN_FLIGHT as u64, freed[0])]);
        for frame in 1..MAX_FRAMES_IN_FLIGHT as u64 {
            assert!(pending.front().unwrap().0 > frame);
            assert_eq!((vtx.alloc(4), idx.alloc(6)), (None, None));
        }
        let (_, (vo, vl, io, il)) = pending.pop_front().unwrap();
        vtx.free_region(vo, vl);
        idx.free_region(io, il);
        assert_eq!((vtx.alloc(4), idx.alloc(6)), (Some(a_v), Some(a_i)));
        assert!(pending.is_empty());
        // An accepted retry replaces the tombstone, never resurrecting an
        // older result or the retired chest pose.
        let planned = std::collections::HashSet::from([0]);
        swap_accepted(&mut chunks, &mut empty, chunk, &accepted, &planned, 10);
        assert!(!empty.contains_key(&(chunk, 0)));
    }

    #[test]
    fn pool_exhaustion_before_oversized_section_keeps_all_old_draws() {
        let chunk = ChunkPos::new(0, 0);
        let chest = BlockPos::new(1, 4, 1);
        let mut vtx = FreeList::new(10);
        let mut idx = FreeList::new(15);
        let mut old = section(0, chest, false, 8);
        old.vertex_offset = vtx.alloc(4).unwrap() as i32;
        old.first_index = idx.alloc(6).unwrap();
        let mut other = section(1, chest, true, 8);
        other.vertex_offset = vtx.alloc(4).unwrap() as i32;
        other.first_index = idx.alloc(6).unwrap();
        other.emitted_chests.clear();
        let mut chunks = HashMap::from([(
            chunk,
            ChunkAlloc {
                sections: vec![old, other],
            },
        )]);
        let mut empty = HashMap::new();
        let accepted = std::collections::HashSet::from([0, 1]);
        let incoming = [(2u32, 3u32), (7, 6)]; // second section is too large for staging
        let half = 6 * VERTEX_SIZE as usize;
        let staging_too_large = incoming
            .iter()
            .any(|&(v, i)| staging_upload_section_too_large(v as usize, i as usize, half));
        assert!(staging_too_large);
        let mut taken = Vec::new();
        let mut pool_full = false;
        for (v, i) in incoming {
            let Some(vo) = vtx.alloc(v) else {
                pool_full = true;
                break;
            };
            let Some(io) = idx.alloc(i) else {
                vtx.free_region(vo, v);
                pool_full = true;
                break;
            };
            taken.push((vo, v, io, i));
        }
        assert!(pool_full);
        assert_eq!(taken.len(), 1); // planning stopped before inspecting section 1
        if retire_on_failed_upload(pool_full, staging_too_large) {
            swap_accepted(
                &mut chunks,
                &mut empty,
                chunk,
                &accepted,
                &Default::default(),
                9,
            );
        } else {
            for (vo, vl, io, il) in taken {
                vtx.free_region(vo, vl);
                idx.free_region(io, il);
            }
        }
        assert_eq!(chunks[&chunk].sections.len(), 2);
        assert!(chunks[&chunk].sections.iter().all(|s| s.epoch == 8));
        assert_eq!(
            resident_chest_open(chunks[&chunk].sections.iter(), &chest),
            Some(false)
        );
        assert!(empty.is_empty());
        assert_eq!((vtx.alloc(2), idx.alloc(3)), (Some(8), Some(12)));
        assert!(retire_on_failed_upload(true, false)); // pool-only retry still retires
    }

    #[test]
    fn tall_column_larger_than_staging_is_uploaded_in_section_batches() {
        let half = 100;
        let sections = [(60, 60), (60, 60), (60, 60)];
        assert!(
            sections
                .iter()
                .all(|&(v, i)| staging_section_fits(v, i, half))
        );
        assert!(sections.iter().map(|s| s.0).sum::<usize>() > half);
        assert!(sections.iter().map(|s| s.1).sum::<usize>() > half);

        let (mut vertex_used, mut index_used, mut flushes) = (0, 0, 0);
        for (vertex_bytes, index_bytes) in sections {
            if staging_needs_flush(vertex_used, index_used, vertex_bytes, index_bytes, half) {
                flushes += 1;
                vertex_used = 0;
                index_used = 0;
            }
            vertex_used += vertex_bytes;
            index_used += index_bytes;
        }
        assert_eq!(flushes, 2);
        assert_eq!((vertex_used, index_used), (60, 60));
    }
}
