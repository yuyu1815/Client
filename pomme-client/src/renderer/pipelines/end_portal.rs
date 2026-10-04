use std::path::Path;
use std::slice;
use std::sync::{Arc, Mutex};

use azalea_core::position::BlockPos;
use pomme_gpu_allocator::vulkan::{Allocation, Allocator};
use pyronyx::vk;

use crate::assets::{AssetIndex, resolve_asset_path_with_pack_dirs};
use crate::renderer::camera::CameraUniform;
use crate::renderer::{MAX_FRAMES_IN_FLIGHT, shader, util};

const DOWN: u8 = 1;
const UP: u8 = 2;
const NORTH: u8 = 4;
const SOUTH: u8 = 8;
const WEST: u8 = 16;
const EAST: u8 = 32;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EndPortalDraw {
    pub gateway: bool,
    pub face_mask: u8,
    pub age: Option<i32>,
}
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
}
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Push {
    game_time: f32,
    layers: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AssetLoadMode {
    Startup,
    Reload,
}
struct Texture {
    image: vk::Image,
    view: vk::ImageView,
    sampler: vk::Sampler,
    allocation: Allocation,
}
pub(crate) struct PendingPortalUpload {
    command: vk::CommandBuffer,
    staging: vk::Buffer,
    staging_allocation: Allocation,
    image: vk::Image,
    view: vk::ImageView,
    allocation: Allocation,
}
pub(crate) fn reclaim_pending_uploads(
    device: &vk::Device,
    pool: vk::CommandPool,
    allocator: &Arc<Mutex<Allocator>>,
    pending: &mut Vec<PendingPortalUpload>,
    _idle: super::super::context::TeardownWait,
) {
    if pending.is_empty() {
        return;
    }
    for p in pending.drain(..) {
        device.free_command_buffers(pool, &[p.command.handle()]);
        device.destroy_buffer(p.staging, None);
        device.destroy_image_view(p.view, None);
        device.destroy_image(p.image, None);
        let mut a = util::lock_allocator(allocator);
        let _ = a.free(p.staging_allocation);
        let _ = a.free(p.allocation);
    }
}
pub struct EndPortalPipeline {
    pipeline: vk::Pipeline,
    layout: vk::PipelineLayout,
    camera_layout: vk::DescriptorSetLayout,
    texture_layout: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    camera_sets: Vec<vk::DescriptorSet>,
    texture_set: vk::DescriptorSet,
    camera_buffers: Vec<vk::Buffer>,
    camera_allocations: Vec<Allocation>,
    vertex_buffers: Vec<vk::Buffer>,
    vertex_allocations: Vec<Allocation>,
    vertex_capacities: Vec<usize>,
    textures: [Texture; 2],
}
struct Build<'a> {
    d: &'a vk::Device,
    a: &'a Arc<Mutex<Allocator>>,
    pipeline: vk::Pipeline,
    layout: vk::PipelineLayout,
    cl: vk::DescriptorSetLayout,
    tl: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    camera: Vec<(vk::Buffer, Allocation)>,
    vertices: Vec<(vk::Buffer, Allocation)>,
    textures: Vec<Texture>,
    done: bool,
}
impl Drop for Build<'_> {
    fn drop(&mut self) {
        if self.done {
            return;
        }
        let d = self.d;
        d.destroy_pipeline(self.pipeline, None);
        d.destroy_pipeline_layout(self.layout, None);
        d.destroy_descriptor_pool(self.pool, None);
        d.destroy_descriptor_set_layout(self.cl, None);
        d.destroy_descriptor_set_layout(self.tl, None);
        let mut a = util::lock_allocator(self.a);
        for (b, x) in self.camera.drain(..).chain(self.vertices.drain(..)) {
            d.destroy_buffer(b, None);
            let _ = a.free(x);
        }
        for t in self.textures.drain(..) {
            d.destroy_sampler(t.sampler, None);
            d.destroy_image_view(t.view, None);
            d.destroy_image(t.image, None);
            let _ = a.free(t.allocation);
        }
    }
}
impl<'a> Build<'a> {
    fn new(d: &'a vk::Device, a: &'a Arc<Mutex<Allocator>>) -> Self {
        Self {
            d,
            a,
            pipeline: vk::Pipeline::null(),
            layout: vk::PipelineLayout::null(),
            cl: vk::DescriptorSetLayout::null(),
            tl: vk::DescriptorSetLayout::null(),
            pool: vk::DescriptorPool::null(),
            camera: vec![],
            vertices: vec![],
            textures: vec![],
            done: false,
        }
    }
    fn finish(mut self, sets: Vec<vk::DescriptorSet>, tex: vk::DescriptorSet) -> EndPortalPipeline {
        self.done = true;
        let c = std::mem::take(&mut self.camera);
        let v = std::mem::take(&mut self.vertices);
        let t: [Texture; 2] = std::mem::take(&mut self.textures).try_into().ok().unwrap();
        EndPortalPipeline {
            pipeline: self.pipeline,
            layout: self.layout,
            camera_layout: self.cl,
            texture_layout: self.tl,
            pool: self.pool,
            camera_sets: sets,
            texture_set: tex,
            camera_buffers: c.iter().map(|x| x.0).collect(),
            camera_allocations: c.into_iter().map(|x| x.1).collect(),
            vertex_buffers: v.iter().map(|x| x.0).collect(),
            vertex_allocations: v.into_iter().map(|x| x.1).collect(),
            vertex_capacities: vec![36; MAX_FRAMES_IN_FLIGHT],
            textures: t,
        }
    }
}
impl EndPortalPipeline {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        d: &vk::Device,
        q: vk::Queue,
        cp: vk::CommandPool,
        rp: vk::RenderPass,
        a: &Arc<Mutex<Allocator>>,
        jar: &Path,
        index: &Option<AssetIndex>,
        packs: &[std::path::PathBuf],
        mode: AssetLoadMode,
        pending: &mut Vec<PendingPortalUpload>,
        idle: Option<super::super::context::TeardownWait>,
    ) -> Result<Self, String> {
        if !pending.is_empty() {
            let outcome = match idle {
                Some(super::super::context::TeardownWait::Idle) => {
                    super::super::context::TeardownWait::Idle
                }
                Some(super::super::context::TeardownWait::DeviceLost) => {
                    return Err("device lost while portal uploads were pending".into());
                }
                None => {
                    d.wait_idle()
                        .map_err(|e| format!("portal upload idle wait failed: {e}"))?;
                    super::super::context::TeardownWait::Idle
                }
            };
            reclaim_pending_uploads(d, cp, a, pending, outcome)
        }
        let keys = [
            "minecraft/textures/environment/end_sky.png",
            "minecraft/textures/entity/end_portal/end_portal.png",
        ];
        let decoded = prepare_textures(&keys, jar, index, packs, mode)?;
        let mut b = Build::new(d, a);
        b.cl = d
            .create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo {
                    binding_count: 1,
                    bindings: [vk::DescriptorSetLayoutBinding {
                        binding: 0,
                        descriptor_type: vk::DescriptorType::UniformBuffer,
                        descriptor_count: 1,
                        stage_flags: vk::ShaderStageFlags::Vertex | vk::ShaderStageFlags::Fragment,
                        ..Default::default()
                    }]
                    .as_ptr(),
                    ..Default::default()
                },
                None,
            )
            .map_err(|e| e.to_string())?;
        let binds = [0, 1].map(|binding| vk::DescriptorSetLayoutBinding {
            binding,
            descriptor_type: vk::DescriptorType::CombinedImageSampler,
            descriptor_count: 1,
            stage_flags: vk::ShaderStageFlags::Fragment,
            ..Default::default()
        });
        b.tl = d
            .create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo {
                    binding_count: 2,
                    bindings: binds.as_ptr(),
                    ..Default::default()
                },
                None,
            )
            .map_err(|e| e.to_string())?;
        let pr = vk::PushConstantRange {
            stage_flags: vk::ShaderStageFlags::Fragment,
            offset: 0,
            size: size_of::<Push>() as u32,
        };
        let ls = [b.cl, b.tl];
        b.layout = d
            .create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo {
                    set_layout_count: 2,
                    set_layouts: ls.as_ptr(),
                    push_constant_range_count: 1,
                    push_constant_ranges: &pr,
                    ..Default::default()
                },
                None,
            )
            .map_err(|e| e.to_string())?;
        b.pipeline = create_pipeline(d, rp, b.layout)?;
        let sizes = [
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::UniformBuffer,
                descriptor_count: MAX_FRAMES_IN_FLIGHT as u32,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::CombinedImageSampler,
                descriptor_count: 2,
            },
        ];
        b.pool = d
            .create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo {
                    max_sets: (MAX_FRAMES_IN_FLIGHT + 1) as u32,
                    pool_size_count: 2,
                    pool_sizes: sizes.as_ptr(),
                    ..Default::default()
                },
                None,
            )
            .map_err(|e| e.to_string())?;
        let cl = vec![b.cl; MAX_FRAMES_IN_FLIGHT];
        let mut sets = vec![vk::DescriptorSet::null(); MAX_FRAMES_IN_FLIGHT];
        d.allocate_descriptor_sets(
            &vk::DescriptorSetAllocateInfo {
                descriptor_pool: b.pool,
                descriptor_set_count: cl.len() as u32,
                set_layouts: cl.as_ptr(),
                ..Default::default()
            },
            &mut sets,
        )
        .map_err(|e| e.to_string())?;
        let mut ts = vk::DescriptorSet::null();
        d.allocate_descriptor_sets(
            &vk::DescriptorSetAllocateInfo {
                descriptor_pool: b.pool,
                descriptor_set_count: 1,
                set_layouts: &b.tl,
                ..Default::default()
            },
            slice::from_mut(&mut ts),
        )
        .map_err(|e| e.to_string())?;
        for &s in &sets {
            let (buf, al) = util::try_create_mapped_buffer(
                d,
                a,
                &vec![0u8; size_of::<CameraUniform>()],
                vk::BufferUsageFlags::UniformBuffer,
                "end_portal_camera",
            )?;
            b.camera.push((buf, al));
            let info = vk::DescriptorBufferInfo {
                buffer: buf,
                offset: 0,
                range: size_of::<CameraUniform>() as u64,
            };
            d.update_descriptor_sets(
                &[vk::WriteDescriptorSet {
                    dst_set: s,
                    dst_binding: 0,
                    descriptor_type: vk::DescriptorType::UniformBuffer,
                    descriptor_count: 1,
                    buffer_info: &info,
                    ..Default::default()
                }],
                &[],
            )
        }
        for data in decoded {
            b.textures.push(load_texture(d, q, cp, a, data, pending)?)
        }
        let infos = b
            .textures
            .iter()
            .map(|t| vk::DescriptorImageInfo {
                sampler: t.sampler,
                image_view: t.view,
                image_layout: vk::ImageLayout::ShaderReadOnlyOptimal,
            })
            .collect::<Vec<_>>();
        let writes = [0, 1].map(|i| vk::WriteDescriptorSet {
            dst_set: ts,
            dst_binding: i,
            descriptor_type: vk::DescriptorType::CombinedImageSampler,
            descriptor_count: 1,
            image_info: &infos[i as usize],
            ..Default::default()
        });
        d.update_descriptor_sets(&writes, &[]);
        let init = [Vertex { position: [0.; 3] }; 36];
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            b.vertices.push(util::try_create_mapped_buffer(
                d,
                a,
                bytemuck::cast_slice(&init),
                vk::BufferUsageFlags::VertexBuffer,
                "end_portal_vertices",
            )?)
        }
        Ok(b.finish(sets, ts))
    }
    pub fn update_camera(&mut self, f: usize, u: &CameraUniform) {
        let bytes = bytemuck::bytes_of(u);
        self.camera_allocations[f].mapped_slice_mut().unwrap()[..bytes.len()].copy_from_slice(bytes)
    }
    pub fn draw(
        &mut self,
        d: &vk::Device,
        a: &Arc<Mutex<Allocator>>,
        cmd: vk::CommandBuffer,
        f: usize,
        anchor: glam::DVec3,
        draws: &[(BlockPos, EndPortalDraw)],
        time: f32,
    ) {
        cmd.bind_pipeline(vk::PipelineBindPoint::Graphics, self.pipeline);
        cmd.bind_descriptor_sets(
            vk::PipelineBindPoint::Graphics,
            self.layout,
            0,
            &[self.camera_sets[f], self.texture_set],
            &[],
        );
        let batches = build_batches(draws, anchor);
        let v: Vec<Vertex> = batches
            .iter()
            .flat_map(|b| b.vertices.iter().copied())
            .collect();
        if v.is_empty() {
            return;
        }
        if v.len() > self.vertex_capacities[f] {
            let cap = v.len().next_power_of_two();
            let zero = vec![Vertex { position: [0.; 3] }; cap];
            let Ok((buf, al)) = util::try_create_mapped_buffer(
                d,
                a,
                bytemuck::cast_slice(&zero),
                vk::BufferUsageFlags::VertexBuffer,
                "end_portal_vertices",
            ) else {
                return;
            };
            d.destroy_buffer(self.vertex_buffers[f], None);
            let mut ag = util::lock_allocator(a);
            let _ = ag.free(std::mem::replace(&mut self.vertex_allocations[f], al));
            self.vertex_buffers[f] = buf;
            self.vertex_capacities[f] = cap
        }
        let bytes = bytemuck::cast_slice(&v);
        self.vertex_allocations[f].mapped_slice_mut().unwrap()[..bytes.len()]
            .copy_from_slice(bytes);
        cmd.bind_vertex_buffers(0, &[self.vertex_buffers[f]], &[0]);
        for b in &batches {
            let p = Push {
                game_time: time,
                layers: b.layers,
            };
            cmd.push_constants(
                self.layout,
                vk::ShaderStageFlags::Fragment,
                0,
                bytemuck::bytes_of(&p),
            );
            cmd.draw(b.vertices.len() as u32, 1, b.first_vertex, 0)
        }
    }
    pub fn recreate_pipeline(&mut self, d: &vk::Device, rp: vk::RenderPass) -> Result<(), String> {
        let n = create_pipeline(d, rp, self.layout)?;
        let old = std::mem::replace(&mut self.pipeline, n);
        d.destroy_pipeline(old, None);
        Ok(())
    }
    pub fn destroy(&mut self, d: &vk::Device, a: &Arc<Mutex<Allocator>>) {
        d.destroy_pipeline(self.pipeline, None);
        d.destroy_pipeline_layout(self.layout, None);
        d.destroy_descriptor_pool(self.pool, None);
        d.destroy_descriptor_set_layout(self.camera_layout, None);
        d.destroy_descriptor_set_layout(self.texture_layout, None);
        let mut ag = util::lock_allocator(a);
        for i in 0..MAX_FRAMES_IN_FLIGHT {
            d.destroy_buffer(self.camera_buffers[i], None);
            let _ = ag.free(std::mem::replace(&mut self.camera_allocations[i], unsafe {
                std::mem::zeroed()
            }));
            d.destroy_buffer(self.vertex_buffers[i], None);
            let _ = ag.free(std::mem::replace(&mut self.vertex_allocations[i], unsafe {
                std::mem::zeroed()
            }));
        }
        for t in &mut self.textures {
            d.destroy_sampler(t.sampler, None);
            d.destroy_image_view(t.view, None);
            d.destroy_image(t.image, None);
            let _ = ag.free(std::mem::replace(&mut t.allocation, unsafe {
                std::mem::zeroed()
            }));
        }
    }
}
pub(crate) fn portal_transaction<T, E>(
    prepare: impl FnOnce() -> Result<T, E>,
    commit: impl FnOnce(T),
) -> Result<(), E> {
    let x = prepare()?;
    commit(x);
    Ok(())
}
fn prepare_textures(
    keys: &[&str; 2],
    jar: &Path,
    index: &Option<AssetIndex>,
    packs: &[std::path::PathBuf],
    mode: AssetLoadMode,
) -> Result<[(Vec<u8>, u32, u32); 2], String> {
    let decode = |key: &str| {
        let selected = resolve_asset_path_with_pack_dirs(jar, index, key, packs);
        decode_texture(&selected).or_else(|e| {
            if mode == AssetLoadMode::Startup {
                let builtin = resolve_asset_path_with_pack_dirs(jar, index, key, &[]);
                decode_texture(&builtin)
                    .map_err(|be| format!("{e}; invalid bundled asset {}: {be}", builtin.display()))
            } else {
                Err(e)
            }
        })
    };
    Ok([decode(keys[0])?, decode(keys[1])?])
}
fn decode_texture(p: &Path) -> Result<(Vec<u8>, u32, u32), String> {
    let (x, w, h) = util::load_png(p).ok_or_else(|| format!("failed to decode {}", p.display()))?;
    (w > 0 && h > 0 && x.len() == w as usize * h as usize * 4)
        .then_some((x, w, h))
        .ok_or_else(|| format!("invalid RGBA dimensions in {}", p.display()))
}
fn load_texture(
    d: &vk::Device,
    q: vk::Queue,
    cp: vk::CommandPool,
    a: &Arc<Mutex<Allocator>>,
    (pixels, w, h): (Vec<u8>, u32, u32),
    pending: &mut Vec<PendingPortalUpload>,
) -> Result<Texture, String> {
    let (image, view, allocation) =
        util::try_create_gpu_image_2d(d, a, w, h, vk::Format::R8G8B8A8Unorm, "end_portal_texture")?;
    let (staging, staging_allocation) = match util::try_create_mapped_buffer(
        d,
        a,
        &pixels,
        vk::BufferUsageFlags::TransferSrc,
        "end_portal_texture_staging",
    ) {
        Ok(x) => x,
        Err(e) => {
            d.destroy_image_view(view, None);
            d.destroy_image(image, None);
            let _ = util::lock_allocator(a).free(allocation);
            return Err(e);
        }
    };
    match util::try_upload_image(d, q, cp, staging, image, w, h) {
        Ok(()) => {
            d.destroy_buffer(staging, None);
            let _ = util::lock_allocator(a).free(staging_allocation);
        }
        Err(e) => {
            if let Some(command) = e.submitted_command_buffer {
                pending.push(PendingPortalUpload {
                    command,
                    staging,
                    staging_allocation,
                    image,
                    view,
                    allocation,
                });
                return Err(e.message);
            } else {
                d.destroy_buffer(staging, None);
                d.destroy_image_view(view, None);
                d.destroy_image(image, None);
                let mut ag = util::lock_allocator(a);
                let _ = ag.free(staging_allocation);
                let _ = ag.free(allocation);
                return Err(e.message);
            }
        }
    }
    let sampler = match util::try_create_nearest_sampler(d, 1) {
        Ok(s) => s,
        Err(e) => {
            d.destroy_image_view(view, None);
            d.destroy_image(image, None);
            let _ = util::lock_allocator(a).free(allocation);
            return Err(e);
        }
    };
    Ok(Texture {
        image,
        view,
        sampler,
        allocation,
    })
}
struct Batch {
    vertices: Vec<Vertex>,
    first_vertex: u32,
    layers: u32,
}
fn build_batches(draws: &[(BlockPos, EndPortalDraw)], anchor: glam::DVec3) -> Vec<Batch> {
    let mut out = vec![];
    for &(pos, draw) in draws {
        let mut v = vec![];
        build_vertices(&mut v, pos, draw, anchor);
        if !v.is_empty() {
            let first_vertex = out.iter().map(|b: &Batch| b.vertices.len() as u32).sum();
            out.push(Batch {
                vertices: v,
                first_vertex,
                layers: if draw.gateway { 16 } else { 15 },
            })
        }
    }
    out
}
fn build_vertices(out: &mut Vec<Vertex>, p: BlockPos, d: EndPortalDraw, anchor: glam::DVec3) {
    out.clear();
    let o = (glam::DVec3::new(p.x as f64, p.y as f64, p.z as f64) - anchor).as_vec3();
    if d.gateway {
        for (bit, c) in faces() {
            if d.face_mask & bit != 0 {
                push_face(out, o, c)
            }
        }
    } else {
        if d.face_mask & DOWN != 0 {
            push_face(
                out,
                o,
                [
                    [0., 0.375, 0.],
                    [1., 0.375, 0.],
                    [1., 0.375, 1.],
                    [0., 0.375, 1.],
                ],
            )
        }
        if d.face_mask & UP != 0 {
            push_face(
                out,
                o,
                [
                    [0., 0.75, 0.],
                    [0., 0.75, 1.],
                    [1., 0.75, 1.],
                    [1., 0.75, 0.],
                ],
            )
        }
    }
}
fn push_face(out: &mut Vec<Vertex>, o: glam::Vec3, q: [[f32; 3]; 4]) {
    for i in [0usize, 1, 2, 0, 2, 3] {
        out.push(Vertex {
            position: (o + glam::Vec3::from_array(q[i])).to_array(),
        })
    }
}
fn faces() -> [(u8, [[f32; 3]; 4]); 6] {
    [
        (
            DOWN,
            [[0., 0., 0.], [1., 0., 0.], [1., 0., 1.], [0., 0., 1.]],
        ),
        (UP, [[0., 1., 0.], [0., 1., 1.], [1., 1., 1.], [1., 1., 0.]]),
        (
            NORTH,
            [[0., 0., 0.], [0., 1., 0.], [1., 1., 0.], [1., 0., 0.]],
        ),
        (
            SOUTH,
            [[0., 0., 1.], [1., 0., 1.], [1., 1., 1.], [0., 1., 1.]],
        ),
        (
            WEST,
            [[0., 0., 0.], [0., 0., 1.], [0., 1., 1.], [0., 1., 0.]],
        ),
        (
            EAST,
            [[1., 0., 0.], [1., 1., 0.], [1., 1., 1.], [1., 0., 1.]],
        ),
    ]
}
fn rasterization_state() -> vk::PipelineRasterizationStateCreateInfo<'static> {
    vk::PipelineRasterizationStateCreateInfo {
        polygon_mode: vk::PolygonMode::Fill,
        cull_mode: vk::CullModeFlags::Back,
        front_face: vk::FrontFace::CounterClockwise,
        line_width: 1.,
        ..Default::default()
    }
}
fn depth_stencil_state() -> vk::PipelineDepthStencilStateCreateInfo<'static> {
    vk::PipelineDepthStencilStateCreateInfo {
        depth_test_enable: vk::TRUE,
        depth_write_enable: vk::TRUE,
        depth_compare_op: vk::CompareOp::LessOrEqual,
        ..Default::default()
    }
}
fn create_pipeline(
    d: &vk::Device,
    rp: vk::RenderPass,
    layout: vk::PipelineLayout,
) -> Result<vk::Pipeline, String> {
    let vc = util::read_spv(&mut std::io::Cursor::new(shader::include_spirv!(
        "end_portal.vert.spv"
    )))
    .map_err(|e| e.to_string())?;
    let fc = util::read_spv(&mut std::io::Cursor::new(shader::include_spirv!(
        "end_portal.frag.spv"
    )))
    .map_err(|e| e.to_string())?;
    let module = |c: &[u32]| {
        d.create_shader_module(
            &vk::ShaderModuleCreateInfo {
                code_size: c.len() * 4,
                code: c.as_ptr(),
                ..Default::default()
            },
            None,
        )
        .map_err(|e| e.to_string())
    };
    let v = module(&vc)?;
    let f = match module(&fc) {
        Ok(f) => f,
        Err(e) => {
            d.destroy_shader_module(v, None);
            return Err(e);
        }
    };
    let stages = [
        vk::PipelineShaderStageCreateInfo {
            stage: vk::ShaderStageFlags::Vertex,
            module: v,
            name: c"main".as_ptr(),
            ..Default::default()
        },
        vk::PipelineShaderStageCreateInfo {
            stage: vk::ShaderStageFlags::Fragment,
            module: f,
            name: c"main".as_ptr(),
            ..Default::default()
        },
    ];
    let bind = vk::VertexInputBindingDescription {
        binding: 0,
        stride: size_of::<Vertex>() as u32,
        input_rate: vk::VertexInputRate::Vertex,
    };
    let attr = vk::VertexInputAttributeDescription {
        location: 0,
        binding: 0,
        format: vk::Format::R32G32B32Sfloat,
        offset: 0,
    };
    let vi = vk::PipelineVertexInputStateCreateInfo {
        vertex_binding_description_count: 1,
        vertex_binding_descriptions: &bind,
        vertex_attribute_description_count: 1,
        vertex_attribute_descriptions: &attr,
        ..Default::default()
    };
    let ia = vk::PipelineInputAssemblyStateCreateInfo {
        topology: vk::PrimitiveTopology::TriangleList,
        ..Default::default()
    };
    let vp = vk::PipelineViewportStateCreateInfo {
        viewport_count: 1,
        scissor_count: 1,
        ..Default::default()
    };
    let ra = rasterization_state();
    let ms = vk::PipelineMultisampleStateCreateInfo {
        rasterization_samples: vk::SampleCountFlags::Type1,
        ..Default::default()
    };
    let ds = depth_stencil_state();
    let blend = vk::PipelineColorBlendAttachmentState {
        color_write_mask: vk::ColorComponentFlags::RGBA,
        ..Default::default()
    };
    let bs = vk::PipelineColorBlendStateCreateInfo {
        attachment_count: 1,
        attachments: &blend,
        ..Default::default()
    };
    let dyns = [vk::DynamicState::Viewport, vk::DynamicState::Scissor];
    let dy = vk::PipelineDynamicStateCreateInfo {
        dynamic_state_count: 2,
        dynamic_states: dyns.as_ptr(),
        ..Default::default()
    };
    let info = [vk::GraphicsPipelineCreateInfo {
        stage_count: 2,
        stages: stages.as_ptr(),
        vertex_input_state: &vi,
        input_assembly_state: &ia,
        viewport_state: &vp,
        rasterization_state: &ra,
        multisample_state: &ms,
        depth_stencil_state: &ds,
        color_blend_state: &bs,
        dynamic_state: &dy,
        layout,
        render_pass: rp,
        subpass: 0,
        ..Default::default()
    }];
    let mut p = vk::Pipeline::null();
    let res = d.create_graphics_pipelines(
        vk::PipelineCache::null(),
        &info,
        None,
        slice::from_mut(&mut p),
    );
    d.destroy_shader_module(v, None);
    d.destroy_shader_module(f, None);
    res.map(|_| p).map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn geometry_count_height_and_anchor() {
        let mut v = vec![];
        let p = BlockPos::new(12, 64, -3);
        let a = glam::DVec3::new(10., 60., -5.);
        build_vertices(
            &mut v,
            p,
            EndPortalDraw {
                gateway: false,
                face_mask: DOWN | UP,
                age: None,
            },
            a,
        );
        assert_eq!(v.len(), 12);
        assert_eq!(v[0].position, [2., 4.375, 2.]);
        assert_eq!(v[6].position[1], 4.75);
        build_vertices(
            &mut v,
            p,
            EndPortalDraw {
                gateway: true,
                face_mask: 0x3f,
                age: None,
            },
            a,
        );
        assert_eq!(v.len(), 36);
        build_vertices(
            &mut v,
            p,
            EndPortalDraw {
                gateway: true,
                face_mask: UP | WEST,
                age: None,
            },
            a,
        );
        assert_eq!(v.len(), 12)
    }
    #[test]
    fn batch_ranges_keep_3x3_portal_geometry_and_gateway_layers() {
        let mut draws: Vec<_> = (0..3)
            .flat_map(|z| (0..3).map(move |x| (x, z)))
            .map(|(x, z)| {
                (
                    BlockPos::new(x * 3, 64, z * 3),
                    EndPortalDraw {
                        gateway: false,
                        face_mask: DOWN | UP,
                        age: None,
                    },
                )
            })
            .collect();
        draws.push((
            BlockPos::new(27, 64, 0),
            EndPortalDraw {
                gateway: true,
                face_mask: UP,
                age: None,
            },
        ));
        draws.push((
            BlockPos::new(30, 64, 0),
            EndPortalDraw {
                gateway: true,
                face_mask: DOWN | UP | NORTH,
                age: None,
            },
        ));
        let batches = build_batches(&draws, glam::DVec3::ZERO);
        assert_eq!(
            batches
                .iter()
                .map(|batch| batch.vertices.len())
                .collect::<Vec<_>>(),
            [12, 12, 12, 12, 12, 12, 12, 12, 12, 6, 18]
        );
        assert_eq!(
            batches
                .iter()
                .map(|batch| batch.first_vertex)
                .collect::<Vec<_>>(),
            [0, 12, 24, 36, 48, 60, 72, 84, 96, 108, 114]
        );
        assert_eq!(
            batches.iter().map(|batch| batch.layers).collect::<Vec<_>>(),
            [15, 15, 15, 15, 15, 15, 15, 15, 15, 16, 16]
        );
        let joined: Vec<_> = batches.iter().flat_map(|batch| &batch.vertices).collect();
        for z in 0..3 {
            for x in 0..3 {
                assert_eq!(
                    joined[(z * 3 + x) * 12].position,
                    [x as f32 * 3., 64.375, z as f32 * 3.]
                );
            }
        }
        assert_eq!(joined[108].position, [27., 65., 0.]);
        assert_eq!(joined[114].position, [30., 64., 0.]);
        for pair in batches.windows(2) {
            assert_eq!(
                pair[0].first_vertex + pair[0].vertices.len() as u32,
                pair[1].first_vertex
            );
        }
    }

    #[test]
    fn generated_portal_and_gateway_faces_wind_outward() {
        for (mask, normal) in [(DOWN, glam::Vec3::NEG_Y), (UP, glam::Vec3::Y)] {
            let mut vertices = Vec::new();
            build_vertices(
                &mut vertices,
                BlockPos::new(0, 0, 0),
                EndPortalDraw {
                    gateway: false,
                    face_mask: mask,
                    age: None,
                },
                glam::DVec3::ZERO,
            );
            assert_eq!(vertices.len(), 6);
            for tri in vertices.chunks_exact(3) {
                let a = glam::Vec3::from_array(tri[0].position);
                let b = glam::Vec3::from_array(tri[1].position);
                let c = glam::Vec3::from_array(tri[2].position);
                assert!(
                    (b - a).cross(c - a).dot(normal) > 0.0,
                    "portal face {mask:#x}"
                );
            }
        }
        for (mask, normal) in [
            (DOWN, glam::Vec3::NEG_Y),
            (UP, glam::Vec3::Y),
            (NORTH, glam::Vec3::NEG_Z),
            (SOUTH, glam::Vec3::Z),
            (WEST, glam::Vec3::NEG_X),
            (EAST, glam::Vec3::X),
        ] {
            let mut vertices = Vec::new();
            build_vertices(
                &mut vertices,
                BlockPos::new(0, 0, 0),
                EndPortalDraw {
                    gateway: true,
                    face_mask: mask,
                    age: None,
                },
                glam::DVec3::ZERO,
            );
            assert_eq!(vertices.len(), 6);
            for tri in vertices.chunks_exact(3) {
                let a = glam::Vec3::from_array(tri[0].position);
                let b = glam::Vec3::from_array(tri[1].position);
                let c = glam::Vec3::from_array(tri[2].position);
                assert!(
                    (b - a).cross(c - a).dot(normal) > 0.0,
                    "gateway face {mask:#x}"
                );
            }
        }
        let raster = rasterization_state();
        let depth = depth_stencil_state();
        assert_eq!(raster.cull_mode, vk::CullModeFlags::Back);
        assert_eq!(raster.front_face, vk::FrontFace::CounterClockwise);
        assert_eq!(depth.depth_test_enable, vk::TRUE);
        assert_eq!(depth.depth_write_enable, vk::TRUE);
    }

    #[test]
    fn projective_uv_contract_and_fog_color_path() {
        let vertex = include_str!("../shaders/end_portal.vert");
        let fragment = include_str!("../shaders/end_portal.frag");
        assert!(vertex.contains("gl_Position = view_proj * vec4(view_position, 1.0)"));
        assert!(vertex.contains("projection.y = -projection.y"));
        assert!(fragment.contains("textureProj(Sampler0,tex_proj)"));
        assert!(fragment.contains("textureProj(Sampler1,tex_proj*end_portal_layer"));
        assert!(fragment.contains("fog_env.x, fog_env.y"));
        assert!(fragment.contains("camera_pos.w, fog_color.w"));
        assert!(fragment.contains("linear_to_srgb(fog_color.r)"));
        assert!(fragment.contains("srgb_to_linear(encoded.r)"));

        let mut camera = crate::renderer::camera::Camera::new(1.0);
        camera.set_render_distance(16);
        let uniform = CameraUniform::new(&camera, [0.2, 0.3, 0.4], 16, false);
        assert_eq!(uniform.end_portal_fog_factor(0.0, 0.0), 0.0);
        assert_eq!(uniform.end_portal_fog_factor(128.0, 128.0), 0.125);
        assert_eq!(uniform.end_portal_fog_factor(256.0, 256.0), 1.0);
    }

    #[test]
    fn startup_fallback_reload_rejection_and_unknown_builtin_are_transactional() {
        let root = std::env::temp_dir().join(format!("portal-assets-{}", std::process::id()));
        let builtin = root.join("builtin");
        let pack = root.join("pack/assets");
        let keys = ["test/sky.png", "test/portal.png"];
        for (key, color) in keys.iter().zip([[10, 20, 30, 255], [40, 50, 60, 255]]) {
            let path = builtin.join(key);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let file = std::fs::File::create(path).unwrap();
            let mut encoder = png::Encoder::new(file, 1, 1);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&color)
                .unwrap();
        }
        let invalid_override = pack.join(keys[0]);
        std::fs::create_dir_all(invalid_override.parent().unwrap()).unwrap();
        std::fs::write(&invalid_override, b"broken override").unwrap();
        let packs = vec![root.join("pack")];
        let startup = prepare_textures(&keys, &builtin, &None, &packs, AssetLoadMode::Startup);
        assert_eq!(startup.as_ref().unwrap()[0].0, [10, 20, 30, 255]);
        assert!(prepare_textures(&keys, &builtin, &None, &packs, AssetLoadMode::Reload).is_err());

        let mut committed = false;
        let failed = portal_transaction(|| Err::<(), _>("prepare failed"), |_| committed = true);
        assert_eq!(failed, Err("prepare failed"));
        assert!(!committed);
        assert_eq!(
            portal_transaction(
                || Ok::<_, &str>(17),
                |value| {
                    assert_eq!(value, 17);
                    committed = true;
                }
            ),
            Ok(())
        );
        assert!(committed);

        std::fs::remove_file(builtin.join(keys[0])).unwrap();
        assert!(prepare_textures(&keys, &builtin, &None, &packs, AssetLoadMode::Startup).is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_broken_png_and_bad_dimensions_before_upload() {
        let dir = std::env::temp_dir().join(format!("portal-png-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let invalid = dir.join("invalid.png");
        std::fs::write(&invalid, b"not a png").unwrap();
        assert!(decode_texture(&invalid).is_err());
        let truncated = dir.join("truncated.png");
        std::fs::write(&truncated, [137, 80, 78, 71, 13, 10, 26, 10]).unwrap();
        assert!(decode_texture(&truncated).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
