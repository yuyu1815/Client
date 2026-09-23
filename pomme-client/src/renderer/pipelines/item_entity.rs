use std::collections::HashMap;
use std::slice;
use std::sync::{Arc, Mutex};

use glam::Mat4;
use pomme_gpu_allocator::vulkan::{Allocation, Allocator};
use pyronyx::vk;

use crate::renderer::camera::CameraUniform;
use crate::renderer::chunk::atlas::{AtlasRegion, AtlasUVMap, SpriteAlphaMask, TextureAtlas};
use crate::renderer::{MAX_FRAMES_IN_FLIGHT, shader, util, world_shadow};
use crate::world::block::model::{BakedModel, Direction, ItemTint, direction_from_positions};

/// Item-only vertex format. Vanilla's ENTITY item format keeps UV0 as floats
/// and carries a baked face normal; both matter here because generated sprite
/// texel boundaries must line up with extrusion geometry and dropped items use
/// the normal for two-direction diffuse lighting.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct ItemVertex {
    position: [f32; 3],
    tex_coords: [f32; 2],
    light_tint: u32,
    normal: [i8; 4],
}

impl ItemVertex {
    const STRIDE: u32 = size_of::<Self>() as u32;

    fn binding_description() -> vk::VertexInputBindingDescription {
        vk::VertexInputBindingDescription {
            binding: 0,
            stride: Self::STRIDE,
            input_rate: vk::VertexInputRate::Vertex,
        }
    }

    fn attribute_descriptions() -> [vk::VertexInputAttributeDescription; 4] {
        [
            vk::VertexInputAttributeDescription {
                location: 0,
                binding: 0,
                format: vk::Format::R32G32B32Sfloat,
                offset: 0,
            },
            vk::VertexInputAttributeDescription {
                location: 1,
                binding: 0,
                format: vk::Format::R32G32Sfloat,
                offset: 12,
            },
            vk::VertexInputAttributeDescription {
                location: 2,
                binding: 0,
                format: vk::Format::R8G8B8A8Unorm,
                offset: 20,
            },
            vk::VertexInputAttributeDescription {
                location: 3,
                binding: 0,
                format: vk::Format::R8G8B8A8Snorm,
                offset: 24,
            },
        ]
    }
}

fn pack_normal(normal: glam::Vec3) -> [i8; 4] {
    let normal = normal.normalize_or_zero();
    [
        (normal.x.clamp(-1.0, 1.0) * 127.0).round() as i8,
        (normal.y.clamp(-1.0, 1.0) * 127.0).round() as i8,
        (normal.z.clamp(-1.0, 1.0) * 127.0).round() as i8,
        0,
    ]
}

pub struct ItemRenderInfo {
    pub item_name: String,
    pub model_matrix: Mat4,
    pub light: f32,
    pub nether_lighting: bool,
    pub entity_uuid: Option<uuid::Uuid>,
    pub invisible: bool,
    pub actual_age: Option<u32>,
    pub actual_render_age: f32,
    pub age_f: f32,
    pub actual_spin: f32,
    pub spin: f32,
    pub bob_offset: f32,
    pub actual_bob_offset: f32,
    pub controlled_phase: bool,
    pub bob_controlled: bool,
    pub position: [f64; 3],
    pub stack_count: i32,
}

/// What the item-entity renderer needs to place a mesh: whether it baked from a
/// 3D (block) model, plus its local-space bounding box used for the hover
/// height and the 3D-vs-flat copy layout (vanilla reads these off
/// `getModelBoundingBox`).
#[derive(Clone, Copy)]
pub struct ItemMeshInfo {
    pub is_block_model: bool,
    pub bounds_min: glam::Vec3,
    pub bounds_max: glam::Vec3,
}

struct MeshEntry {
    buffer: vk::Buffer,
    allocation: Allocation,
    gui_buffer: Option<(vk::Buffer, Allocation)>,
    vertex_count: u32,
    is_3d_model: bool,
    /// Vanilla `BakedQuad.MaterialInfo`: a sprite with partial alpha draws on
    /// the translucent item sheet, everything else on the cutout one.
    translucent: bool,
    bounds_min: glam::Vec3,
    bounds_max: glam::Vec3,
    tint_rgbs: Vec<[u8; 3]>,
}

/// Descriptor layouts, per-frame camera UBOs, and atlas set shared by the
/// pipelines that draw item meshes with the item_entity shaders.
pub(super) struct ItemPipelineShared {
    pub pipeline_layout: vk::PipelineLayout,
    camera_layout: vk::DescriptorSetLayout,
    atlas_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    camera_sets: Vec<vk::DescriptorSet>,
    atlas_set: vk::DescriptorSet,
    atlas_sampler: vk::Sampler,
    camera_buffers: Vec<vk::Buffer>,
    camera_allocations: Vec<Option<Allocation>>,
    last_view_projection: [[f32; 4]; 4],
    last_camera_position: [f32; 3],
}

fn write_texture_descriptor(
    device: &vk::Device,
    set: vk::DescriptorSet,
    view: vk::ImageView,
    sampler: vk::Sampler,
) {
    let image_info = vk::DescriptorImageInfo {
        sampler,
        image_view: view,
        image_layout: vk::ImageLayout::ShaderReadOnlyOptimal,
    };
    let write = vk::WriteDescriptorSet {
        dst_set: set,
        dst_binding: 0,
        descriptor_type: vk::DescriptorType::CombinedImageSampler,
        descriptor_count: 1,
        image_info: &image_info,
        ..Default::default()
    };
    device.update_descriptor_sets(&[write], &[]);
}

impl ItemPipelineShared {
    pub(super) fn new(
        device: &vk::Device,
        allocator: &Arc<Mutex<Allocator>>,
        atlas: &TextureAtlas,
        label: &str,
    ) -> Self {
        let camera_layout = util::create_descriptor_set_layout(
            device,
            vk::DescriptorType::UniformBuffer,
            vk::ShaderStageFlags::Vertex,
        );
        let atlas_layout = util::create_descriptor_set_layout(
            device,
            vk::DescriptorType::CombinedImageSampler,
            vk::ShaderStageFlags::Fragment,
        );

        let push_range = vk::PushConstantRange {
            stage_flags: vk::ShaderStageFlags::Vertex | vk::ShaderStageFlags::Fragment,
            offset: 0,
            // 64-byte model + fragment light + world-light selector + padded
            // mat3 normal matrix. Vulkan guarantees at least 128 push bytes.
            size: 128,
        };
        let layouts = [camera_layout, atlas_layout];
        let layout_info = vk::PipelineLayoutCreateInfo {
            set_layout_count: layouts.len() as u32,
            set_layouts: layouts.as_ptr(),
            push_constant_range_count: 1,
            push_constant_ranges: &push_range,
            ..Default::default()
        };
        let pipeline_layout = device
            .create_pipeline_layout(&layout_info, None)
            .unwrap_or_else(|_| panic!("failed to create {label} pipeline layout"));

        let pool_sizes = [
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::UniformBuffer,
                descriptor_count: MAX_FRAMES_IN_FLIGHT as u32,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::CombinedImageSampler,
                descriptor_count: 2,
            },
        ];
        let pool_info = vk::DescriptorPoolCreateInfo {
            max_sets: MAX_FRAMES_IN_FLIGHT as u32 + 2,
            pool_size_count: pool_sizes.len() as u32,
            pool_sizes: pool_sizes.as_ptr(),
            ..Default::default()
        };
        let descriptor_pool = device
            .create_descriptor_pool(&pool_info, None)
            .unwrap_or_else(|_| panic!("failed to create {label} descriptor pool"));

        let camera_layouts: Vec<_> = (0..MAX_FRAMES_IN_FLIGHT).map(|_| camera_layout).collect();
        let camera_alloc_info = vk::DescriptorSetAllocateInfo {
            descriptor_pool,
            descriptor_set_count: camera_layouts.len() as u32,
            set_layouts: camera_layouts.as_ptr(),
            ..Default::default()
        };
        let mut camera_sets = vec![vk::DescriptorSet::null(); camera_layouts.len()];
        device
            .allocate_descriptor_sets(&camera_alloc_info, &mut camera_sets)
            .unwrap_or_else(|_| panic!("failed to allocate {label} camera sets"));

        let atlas_alloc_info = vk::DescriptorSetAllocateInfo {
            descriptor_pool,
            descriptor_set_count: 1,
            set_layouts: &atlas_layout,
            ..Default::default()
        };
        let mut atlas_set = vk::DescriptorSet::null();
        device
            .allocate_descriptor_sets(&atlas_alloc_info, slice::from_mut(&mut atlas_set))
            .unwrap_or_else(|_| panic!("failed to allocate {label} atlas set"));
        // Vanilla's items atlas has no mip chain, so generated items sample
        // level 0. TODO: block-model items draw from the mipmapped blocks atlas
        // in vanilla; pomme samples those at level 0 too.
        let atlas_sampler = unsafe { util::create_nearest_sampler(device) };

        let mut camera_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        let mut camera_allocations: Vec<Option<Allocation>> =
            Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);

        for &set in &camera_sets {
            let (buf, alloc) = util::create_uniform_buffer(
                device,
                allocator,
                size_of::<CameraUniform>() as u64,
                &format!("{label}_camera"),
            );
            let buffer_info = vk::DescriptorBufferInfo {
                buffer: buf,
                offset: 0,
                range: size_of::<CameraUniform>() as u64,
            };
            let write = vk::WriteDescriptorSet {
                dst_set: set,
                dst_binding: 0,
                descriptor_type: vk::DescriptorType::UniformBuffer,
                descriptor_count: 1,
                buffer_info: &buffer_info,
                ..Default::default()
            };
            device.update_descriptor_sets(&[write], &[]);
            camera_buffers.push(buf);
            camera_allocations.push(Some(alloc));
        }

        let this = Self {
            pipeline_layout,
            camera_layout,
            atlas_layout,
            descriptor_pool,
            camera_sets,
            atlas_set,
            atlas_sampler,
            camera_buffers,
            camera_allocations,
            last_view_projection: glam::Mat4::IDENTITY.to_cols_array_2d(),
            last_camera_position: [0.0; 3],
        };
        this.rebind_atlas(device, atlas);
        this
    }

    pub(super) fn rebind_atlas(&self, device: &vk::Device, atlas: &TextureAtlas) {
        write_texture_descriptor(device, self.atlas_set, atlas.view, self.atlas_sampler);
    }

    pub(super) fn allocate_texture_set(
        &self,
        device: &vk::Device,
        view: vk::ImageView,
        sampler: vk::Sampler,
    ) -> vk::DescriptorSet {
        let info = vk::DescriptorSetAllocateInfo {
            descriptor_pool: self.descriptor_pool,
            descriptor_set_count: 1,
            set_layouts: &self.atlas_layout,
            ..Default::default()
        };
        let mut set = vk::DescriptorSet::null();
        device.allocate_descriptor_sets(&info, slice::from_mut(&mut set))
            .expect("failed to allocate shadow texture descriptor");
        write_texture_descriptor(device, set, view, sampler);
        set
    }

    pub(super) fn update_camera(&mut self, frame: usize, uniform: &CameraUniform) {
        self.last_view_projection = uniform.view_projection();
        self.last_camera_position = uniform.camera_position();
        let bytes = bytemuck::bytes_of(uniform);
        if let Some(alloc) = self.camera_allocations[frame].as_mut() {
            alloc.mapped_slice_mut().unwrap()[..bytes.len()].copy_from_slice(bytes);
        }
    }

    pub(super) fn bind(&self, cmd: vk::CommandBuffer, frame: usize, pipeline: vk::Pipeline) {
        self.bind_texture(cmd, frame, pipeline, self.atlas_set);
    }

    pub(super) fn bind_texture(
        &self,
        cmd: vk::CommandBuffer,
        frame: usize,
        pipeline: vk::Pipeline,
        texture_set: vk::DescriptorSet,
    ) {
        cmd.bind_pipeline(vk::PipelineBindPoint::Graphics, pipeline);
        cmd.bind_descriptor_sets(
            vk::PipelineBindPoint::Graphics,
            self.pipeline_layout,
            0,
            &[self.camera_sets[frame], texture_set],
            &[],
        );
    }

    pub(super) fn destroy(&mut self, device: &vk::Device, allocator: &Arc<Mutex<Allocator>>) {
        for i in 0..MAX_FRAMES_IN_FLIGHT {
            device.destroy_buffer(self.camera_buffers[i], None);
            if let Some(alloc) = self.camera_allocations[i].take() {
                allocator.lock().unwrap().free(alloc).ok();
            }
        }

        device.destroy_sampler(self.atlas_sampler, None);
        device.destroy_pipeline_layout(self.pipeline_layout, None);
        device.destroy_descriptor_pool(self.descriptor_pool, None);
        device.destroy_descriptor_set_layout(self.camera_layout, None);
        device.destroy_descriptor_set_layout(self.atlas_layout, None);
    }
}

pub(super) fn push_model_light(
    cmd: vk::CommandBuffer,
    layout: vk::PipelineLayout,
    model: &Mat4,
    light: f32,
) {
    let mvp_data = model.to_cols_array();
    cmd.push_constants(
        layout,
        vk::ShaderStageFlags::Vertex | vk::ShaderStageFlags::Fragment,
        0,
        bytemuck::bytes_of(&mvp_data),
    );
    cmd.push_constants(
        layout,
        vk::ShaderStageFlags::Vertex | vk::ShaderStageFlags::Fragment,
        64,
        bytemuck::bytes_of(&light),
    );
}

pub(super) fn push_world_lighting(
    cmd: vk::CommandBuffer,
    layout: vk::PipelineLayout,
    model: &Mat4,
    nether: bool,
) {
    let nether = if nether { 1.0_f32 } else { 0.0_f32 };
    cmd.push_constants(
        layout,
        vk::ShaderStageFlags::Vertex,
        68,
        bytemuck::bytes_of(&nether),
    );

    let normal = glam::Mat3::from_mat4(*model).inverse().transpose();
    let cols = normal.to_cols_array();
    let padded_cols = [
        cols[0], cols[1], cols[2], 0.0, cols[3], cols[4], cols[5], 0.0, cols[6], cols[7], cols[8],
        0.0,
    ];
    cmd.push_constants(
        layout,
        vk::ShaderStageFlags::Vertex,
        80,
        bytemuck::bytes_of(&padded_cols),
    );
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ShadowVertex {
    position: [f32; 3],
    uv: [f32; 2],
    color: [u8; 4],
}

struct ShadowPipeline {
    pipeline: vk::Pipeline,
    image: vk::Image,
    view: vk::ImageView,
    allocation: Option<Allocation>,
    sampler: vk::Sampler,
    texture_set: vk::DescriptorSet,
    buffers: Vec<(vk::Buffer, Allocation)>,
    last_trace: Option<serde_json::Value>,
}

impl ShadowPipeline {
    // ponytail: fixed 16-quad per-frame ceiling; grow when non-item entities use this pass.
    const MAX_QUADS: usize = 16;
    const VERTICES_PER_QUAD: usize = 6;

    fn new(
        device: &vk::Device,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
        render_pass: vk::RenderPass,
        allocator: &Arc<Mutex<Allocator>>,
        shared: &ItemPipelineShared,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Self {
        let (image, view, allocation) = util::create_gpu_image_with_format(
            device, allocator, width, height, vk::Format::R8G8B8A8Unorm, "entity_shadow",
        );
        let (staging, staging_alloc) = util::create_staging_buffer(device, allocator, rgba, "entity_shadow_staging");
        util::upload_image(device, queue, command_pool, staging, image, width, height);
        device.destroy_buffer(staging, None);
        allocator.lock().unwrap().free(staging_alloc).ok();
        let sampler = unsafe { util::create_linear_sampler(device) };
        let texture_set = shared.allocate_texture_set(device, view, sampler);
        let pipeline = create_shadow_pipeline(device, render_pass, shared.pipeline_layout);
        let zeroes = vec![0; Self::MAX_QUADS * Self::VERTICES_PER_QUAD * size_of::<ShadowVertex>()];
        let buffers = (0..MAX_FRAMES_IN_FLIGHT).map(|_| {
            util::create_mapped_buffer(device, allocator, &zeroes, vk::BufferUsageFlags::VertexBuffer, "entity_shadow_vertices")
        }).collect();
        Self { pipeline, image, view, allocation: Some(allocation), sampler, texture_set, buffers, last_trace: None }
    }

    fn draw(
        &mut self,
        cmd: vk::CommandBuffer,
        frame: usize,
        shared: &ItemPipelineShared,
        chunks: &crate::world::chunk::ChunkStore,
        items: &[ItemRenderInfo],
        camera: [f64; 3],
        anchor: glam::DVec3,
        dimension: &str,
    ) {
        self.last_trace = None;
        let ambient = if dimension == "minecraft:the_nether" { 0.1 } else { 0.0 };
        let mut vertices = Vec::new();
        let mut target_trace = None;
        for item in items {
            // The drop pipeline receives only currently renderable items; Rust option/invisibility metadata are not tracked yet.
            let trace_target = std::env::var_os("POMME_ITEM_ENTITY_TRACE").is_some()
                && item.entity_uuid.is_some_and(|uuid| std::env::var("POMME_DROP_TARGET_UUID").is_ok_and(|s| s == uuid.to_string()));
            let diagnostic_disabled = trace_target
                && std::env::var("POMME_DROP_SHADOWS_DISABLED").is_ok_and(|v| v == "1");
            let pieces = if diagnostic_disabled { Vec::new() } else {
                world_shadow::item_shadow_pieces(
                    chunks, item.position, camera, 0.15, 0.75, ambient, true, !item.invisible,
                )
            };
            for piece in &pieces {
                append_shadow_quad(&mut vertices, *piece, item.position, anchor);
            }
            if trace_target {
                target_trace = Some(serde_json::json!({
                    "targetEntityUUID": item.entity_uuid,
                    "radius": 0.15, "strength": 0.75, "shadowsOption": true,
                    "visible": !item.invisible, "distanceCamera": camera, "position": item.position,
                    "textureAsset": "minecraft:textures/misc/shadow.png",
                    "textureFormat": "R8G8B8A8_UNORM; dedicated descriptor, not an atlas entry",
                    "sampler": "LINEAR, CLAMP_TO_EDGE; source PNG mcmeta clamp=true",
                    "depthState": "LESS_OR_EQUAL, depth writes off; Java uses GREATER_OR_EQUAL under reversed-Z",
                    "projectionOffset": "perspective ModelView scale 1-1/4096 per VIEW_OFFSET_Z_LAYERING",
                    "scope": "item entities only; default shadows-on behavior, shared invisibility metadata gates this entity",
                    "diagnosticDisabled": diagnostic_disabled,
                    "pieces": pieces.iter().map(|p| serde_json::json!({
                        "relative": p.relative, "bounds": p.bounds, "alpha": p.alpha,
                        "brightness": p.brightness, "powerAtDepth": p.power_at_depth, "uv": p.uv,
                    })).collect::<Vec<_>>(),
                    "emittedVertexCount": vertices.len(),
                    "emittedVertices": vertices.iter().map(|v| serde_json::json!({
                        "position": v.position, "uv": v.uv, "rgba": v.color,
                    })).collect::<Vec<_>>(),
                    "provenance": "actual Rust world-shadow payload immediately before mapped VBO upload and cmd.draw"
                }));
            }
        }
        if vertices.is_empty() {
            self.last_trace = target_trace;
            return;
        }
        let count = vertices.len().min(Self::MAX_QUADS * Self::VERTICES_PER_QUAD);
        let (_, allocation) = &mut self.buffers[frame];
        let mapped = allocation.mapped_slice_mut().expect("shadow vertex buffer is mapped");
        let bytes = bytemuck::cast_slice(&vertices[..count]);
        mapped[..bytes.len()].copy_from_slice(bytes);
        shared.bind_texture(cmd, frame, self.pipeline, self.texture_set);
        cmd.push_constants(
            shared.pipeline_layout,
            vk::ShaderStageFlags::Vertex,
            0,
            bytemuck::bytes_of(&glam::Mat4::IDENTITY.to_cols_array()),
        );
        cmd.bind_vertex_buffers(0, &[self.buffers[frame].0], &[0]);
        cmd.draw(count as u32, 1, 0, 0);
        if let Some(trace) = target_trace.as_mut() { trace["draw"] = serde_json::json!("submitted"); }
        self.last_trace = target_trace;
    }

    fn recreate(&mut self, device: &vk::Device, render_pass: vk::RenderPass, layout: vk::PipelineLayout) {
        device.destroy_pipeline(self.pipeline, None);
        self.pipeline = create_shadow_pipeline(device, render_pass, layout);
    }

    fn destroy(&mut self, device: &vk::Device, allocator: &Arc<Mutex<Allocator>>) {
        device.destroy_pipeline(self.pipeline, None);
        device.destroy_image_view(self.view, None);
        device.destroy_image(self.image, None);
        if let Some(allocation) = self.allocation.take() {
            allocator.lock().unwrap().free(allocation).ok();
        }
        device.destroy_sampler(self.sampler, None);
        for (buffer, allocation) in self.buffers.drain(..) {
            device.destroy_buffer(buffer, None);
            allocator.lock().unwrap().free(allocation).ok();
        }
    }
}

fn append_shadow_quad(
    out: &mut Vec<ShadowVertex>,
    piece: world_shadow::ShadowPiece,
    entity: [f64; 3],
    anchor: glam::DVec3,
) {
    let [min_x, min_y, min_z, max_x, _, max_z] = piece.bounds;
    let [u0, v0, u1, v1] = piece.uv;
    let y = entity[1] as f32 + piece.relative[1] + min_y as f32 - anchor.y as f32;
    let x0 = entity[0] as f32 + piece.relative[0] + min_x as f32 - anchor.x as f32;
    let x1 = entity[0] as f32 + piece.relative[0] + max_x as f32 - anchor.x as f32;
    let z0 = entity[2] as f32 + piece.relative[2] + min_z as f32 - anchor.z as f32;
    let z1 = entity[2] as f32 + piece.relative[2] + max_z as f32 - anchor.z as f32;
    let alpha = (piece.alpha * 255.0).round().clamp(0.0, 255.0) as u8;
    let color = [255, 255, 255, alpha];
    let corners = [
        ([x0, y, z0], [u0, v0]),
        ([x0, y, z1], [u0, v1]),
        ([x1, y, z1], [u1, v1]),
        ([x1, y, z0], [u1, v0]),
    ];
    for i in [0, 1, 2, 0, 2, 3] {
        out.push(ShadowVertex { position: corners[i].0, uv: corners[i].1, color });
    }
}

pub struct ItemEntityPipeline {
    shadow: Option<ShadowPipeline>,
    /// Vanilla `ITEM_CUTOUT`: alpha-tested, no blending.
    cutout: vk::Pipeline,
    /// Vanilla `ITEM_TRANSLUCENT`: alpha-tested and blended.
    translucent: vk::Pipeline,
    shared: ItemPipelineShared,
    meshes: HashMap<String, MeshEntry>,
    last_draw_trace: Option<serde_json::Value>,
}

impl ItemEntityPipeline {
    pub fn new(
        device: &vk::Device,
        render_pass: vk::RenderPass,
        allocator: &Arc<Mutex<Allocator>>,
        atlas: &TextureAtlas,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
        shadow_texture: Option<(u32, u32, Vec<u8>)>,
    ) -> Self {
        let shared = ItemPipelineShared::new(device, allocator, atlas, "item_entity");
        let shadow = shadow_texture.map(|(width, height, rgba)| ShadowPipeline::new(
            device, queue, command_pool, render_pass, allocator, &shared, width, height, &rgba,
        ));
        let (cutout, translucent) =
            create_world_pipelines(device, render_pass, shared.pipeline_layout);

        Self {
            shadow,
            cutout,
            translucent,
            shared,
            meshes: HashMap::new(),
            last_draw_trace: None,
        }
    }

    pub fn update_camera(&mut self, frame: usize, uniform: &CameraUniform) {
        self.shared.update_camera(frame, uniform);
    }

    pub fn draw_shadows(
        &mut self,
        cmd: vk::CommandBuffer,
        frame: usize,
        chunks: &crate::world::chunk::ChunkStore,
        items: &[ItemRenderInfo],
        camera: [f64; 3],
        anchor: glam::DVec3,
        dimension: &str,
    ) {
        let Some(shadow) = self.shadow.as_mut() else { return; };
        shadow.draw(cmd, frame, &self.shared, chunks, items, camera, anchor, dimension);
    }

    fn insert_mesh(
        &mut self,
        device: &vk::Device,
        allocator: &Arc<Mutex<Allocator>>,
        name: &str,
        vertices: &[ItemVertex],
        gui_vertices: Option<&[ItemVertex]>,
        is_3d_model: bool,
        translucent: bool,
    ) {
        let bytes = bytemuck::cast_slice(vertices);
        let (buffer, allocation) = util::create_mapped_buffer(
            device,
            allocator,
            bytes,
            vk::BufferUsageFlags::VertexBuffer,
            &format!("item_{name}"),
        );
        let gui_buffer = gui_vertices.map(|gui_vertices| {
            let (buffer, allocation) = util::create_mapped_buffer(
                device,
                allocator,
                bytemuck::cast_slice(gui_vertices),
                vk::BufferUsageFlags::VertexBuffer,
                &format!("item_{name}_gui_order"),
            );
            (buffer, allocation)
        });
        let (bounds_min, bounds_max) = mesh_bounds(vertices);
        let mut tint_rgbs = Vec::new();
        for vertex in vertices {
            let rgb = [
                ((vertex.light_tint >> 8) & 0xff) as u8,
                ((vertex.light_tint >> 16) & 0xff) as u8,
                ((vertex.light_tint >> 24) & 0xff) as u8,
            ];
            if !tint_rgbs.contains(&rgb) {
                tint_rgbs.push(rgb);
            }
        }
        self.meshes.insert(
            name.to_string(),
            MeshEntry {
                buffer,
                allocation,
                gui_buffer,
                vertex_count: vertices.len() as u32,
                is_3d_model,
                translucent,
                bounds_min,
                bounds_max,
                tint_rgbs,
            },
        );
    }

    /// `None` if no mesh is built yet, else its 3D-model flag and local bounds.
    pub fn mesh_info(&self, name: &str) -> Option<ItemMeshInfo> {
        self.meshes.get(name).map(|m| ItemMeshInfo {
            is_block_model: m.is_3d_model,
            bounds_min: m.bounds_min,
            bounds_max: m.bounds_max,
        })
    }

    pub fn mesh_handle(&self, name: &str) -> Option<(vk::Buffer, u32)> {
        self.meshes.get(name).map(|m| (m.buffer, m.vertex_count))
    }

    pub fn debug_held_draw_payload(&self, name: &str) -> Option<serde_json::Value> {
        let mesh = self.meshes.get(name)?;
        let bytes = mesh.allocation.mapped_slice()?;
        let vertices: &[ItemVertex] = bytemuck::try_cast_slice(bytes).ok()?;
        Some(serde_json::json!({
            "buffer": format!("{:?}", mesh.buffer),
            "boundByteOffset": 0,
            "vertexCount": mesh.vertex_count,
            "strideBytes": ItemVertex::STRIDE,
            "mappedAllocationOffset": mesh.allocation.offset(),
            "mappedAllocationBytes": bytes.len(),
            "vertices": vertices.iter().take(mesh.vertex_count as usize).map(|v| serde_json::json!({
                "position": v.position,
                "uv": v.tex_coords,
                "lightTintBytes": v.light_tint.to_le_bytes(),
                "normalBytes": v.normal,
            })).collect::<Vec<_>>(),
            "provenance": "CPU readback of host-mapped allocation bound by the actual held vkCmdBindVertexBuffers; not GPU readback",
        }))
    }

    pub fn gui_mesh_handle(&self, name: &str) -> Option<(vk::Buffer, u32)> {
        self.meshes.get(name).map(|mesh| {
            let buffer = mesh.gui_buffer.as_ref().map_or(mesh.buffer, |(buffer, _)| *buffer);
            (buffer, mesh.vertex_count)
        })
    }

    pub fn mesh_is_translucent(&self, name: &str) -> bool {
        self.meshes.get(name).is_some_and(|mesh| mesh.translucent)
    }

    pub(crate) fn debug_mesh(&self, name: &str) -> Option<serde_json::Value> {
        self.meshes.get(name).map(|mesh| {
            serde_json::json!({
                "vertexCount": mesh.vertex_count,
                "packedRgb": mesh.tint_rgbs,
                "translucent": mesh.translucent,
                "renderPath": if mesh.translucent { "ItemEntityPipeline::translucent" } else { "ItemEntityPipeline::cutout" },
                "blend": if mesh.translucent { "src-alpha,one-minus-src-alpha" } else { "disabled" },
                "guiDepthWrite": !mesh.translucent,
                "guiAlphaBlend": if mesh.translucent { "one,one-minus-src-alpha" } else { "one,zero" },
                "guiBakePath": "GuiItemPipeline::bake_to_slot -> item_entity.vert; light_tint.r is precomputed Lighting.ITEMS_3D GUI light; color attachment R8G8B8A8Unorm; translucent mesh depth-write=false, cutout depth-write=true",
                "heldPath": "HeldItemPipeline -> item_entity_world.vert; model inverse-transpose normal + Lighting.LEVEL/NETHER vectors; light_tint.r ignored",
                "worldDropPath": "ItemEntityPipeline -> item_entity_world.vert; model inverse-transpose normal + Lighting.LEVEL vectors; light_tint.r ignored; color attachment B8G8R8A8_SRGB",
                "vertexTintFormat": "R8G8B8A8Unorm",
                "normalFormat": "R8G8B8A8Snorm",
                "atlasFormat": "R8G8B8A8_SRGB",
                "alphaCutout": "fragment discard when alpha < 0.1",
                "provenance": "CPU-built item vertex payload retained by ItemEntityPipeline before mapped Vulkan upload; no GPU readback",
            })
        })
    }

    pub fn ensure_mesh(
        &mut self,
        device: &vk::Device,
        allocator: &Arc<Mutex<Allocator>>,
        name: &str,
        model: &BakedModel,
        uv_map: &AtlasUVMap,
    ) {
        if self.meshes.contains_key(name) {
            return;
        }
        let translucent = model
            .quads
            .iter()
            .any(|quad| uv_map.get_region(&quad.texture).translucent);
        let vertices = build_item_mesh(model, uv_map, false);
        if !vertices.is_empty() {
            let gui_vertices = translucent.then(|| build_item_mesh(model, uv_map, true));
            self.insert_mesh(
                device,
                allocator,
                name,
                &vertices,
                gui_vertices.as_deref(),
                true,
                translucent,
            );
        }
    }

    pub fn ensure_flat_mesh(
        &mut self,
        device: &vk::Device,
        allocator: &Arc<Mutex<Allocator>>,
        name: &str,
        texture_key: &str,
        item_tint: ItemTint,
        uv_map: &AtlasUVMap,
    ) {
        if self.meshes.contains_key(name) {
            return;
        }
        if !uv_map.has_region(texture_key) {
            return;
        }
        let region = uv_map.get_region(texture_key);
        let vertices = uv_map
            .sprite_alpha_mask(texture_key)
            .map(|mask| build_extruded_item_mask(mask, region, item_tint.rgb()))
            .unwrap_or_else(|| build_flat_quad(region, item_tint.rgb()));
        if !vertices.is_empty() {
            self.insert_mesh(
                device,
                allocator,
                name,
                &vertices,
                None,
                false,
                region.translucent,
            );
        }
    }

    /// Cutout meshes first, then translucent ones, as vanilla's item sheets
    /// are ordered.
    pub fn draw(&mut self, cmd: vk::CommandBuffer, frame: usize, items: &[ItemRenderInfo]) {
        self.last_draw_trace = None;
        if items.is_empty() {
            return;
        }

        for (pipeline, translucent) in [(self.cutout, false), (self.translucent, true)] {
            let mut bound = false;
            for item in items {
                let Some(mesh) = self.meshes.get(&item.item_name) else {
                    continue;
                };
                if mesh.translucent != translucent {
                    continue;
                }
                if !bound {
                    self.shared.bind(cmd, frame, pipeline);
                    bound = true;
                }
                cmd.bind_vertex_buffers(0, &[mesh.buffer], &[0]);
                push_model_light(
                    cmd,
                    self.shared.pipeline_layout,
                    &item.model_matrix,
                    item.light,
                );
                push_world_lighting(
                    cmd,
                    self.shared.pipeline_layout,
                    &item.model_matrix,
                    item.nether_lighting,
                );
                cmd.draw(mesh.vertex_count, 1, 0, 0);
                if std::env::var_os("POMME_ITEM_ENTITY_TRACE").is_some()
                    && let Some(uuid) = item.entity_uuid
                    && std::env::var("POMME_DROP_TARGET_UUID").is_ok_and(|target| target == uuid.to_string())
                {
                    self.last_draw_trace = Some(serde_json::json!({
                        "status": "submitted",
                        "targetEntityUUID": uuid,
                        "itemId": item.item_name,
                        "stackCount": item.stack_count,
                        "position": item.position,
                        "actualAge": item.actual_age,
                        "actualRenderAge": item.actual_render_age,
                        "renderAge": item.age_f,
                        "actualBobOffset": item.actual_bob_offset,
                        "actualSpin": item.actual_spin,
                        "bobOffset": item.bob_offset,
                        "spin": item.spin,
                        "controlledPhase": item.controlled_phase,
                        "bobControlled": item.bob_controlled,
                        "controlledBobInput": item.bob_controlled.then_some(item.bob_offset),
                        "controlledPhaseInputs": item.controlled_phase.then_some(serde_json::json!({"age": item.age_f, "bobOffset": item.bob_offset, "spin": item.spin})),
                        "displayContext": "GROUND",
                        "light": item.light,
                        "lightMode": if item.nether_lighting { "NETHER" } else { "LEVEL" },
                        "modelMatrixColumnMajor": item.model_matrix.to_cols_array(),
                        "viewProjectionMatrixColumnMajor": self.shared.last_view_projection.into_iter().flatten().collect::<Vec<_>>(),
                        "cameraPositionRelativeToAnchor": self.shared.last_camera_position,
                        "normalMatrixColumnMajor": glam::Mat3::from_mat4(item.model_matrix).inverse().transpose().to_cols_array(),
                        "vertexCount": mesh.vertex_count,
                        "vertexPayload": self.debug_held_draw_payload(&item.item_name),
                        "source": "actual Rust ItemEntityPipeline::draw bound mesh and submitted cmd.draw; payload is mapped VBO CPU bytes"
                    }));
                }
            }
        }
    }

    pub fn probe_draw_trace(&self) -> serde_json::Value {
        self.last_draw_trace.clone().unwrap_or_else(|| serde_json::json!({"status": "no-target-submission"}))
    }

    pub fn recreate_pipeline(&mut self, device: &vk::Device, render_pass: vk::RenderPass) {
        self.destroy_pipelines(device);
        (self.cutout, self.translucent) =
            create_world_pipelines(device, render_pass, self.shared.pipeline_layout);
        if let Some(shadow) = self.shadow.as_mut() {
            shadow.recreate(device, render_pass, self.shared.pipeline_layout);
        }
    }

    fn destroy_pipelines(&self, device: &vk::Device) {
        device.destroy_pipeline(self.cutout, None);
        device.destroy_pipeline(self.translucent, None);
    }

    pub fn rebind_atlas(&self, device: &vk::Device, atlas: &TextureAtlas) {
        self.shared.rebind_atlas(device, atlas);
    }

    pub fn clear_meshes(&mut self, device: &vk::Device, allocator: &Arc<Mutex<Allocator>>) {
        for (_, entry) in self.meshes.drain() {
            device.destroy_buffer(entry.buffer, None);
            allocator.lock().unwrap().free(entry.allocation).ok();
            if let Some((buffer, allocation)) = entry.gui_buffer {
                device.destroy_buffer(buffer, None);
                allocator.lock().unwrap().free(allocation).ok();
            }
        }
    }

    pub fn probe_shadow_trace(&self) -> serde_json::Value {
        self.shadow.as_ref().and_then(|shadow| shadow.last_trace.clone())
            .unwrap_or_else(|| serde_json::json!({"status": "no-target-shadow-submission"}))
    }

    pub fn destroy(&mut self, device: &vk::Device, allocator: &Arc<Mutex<Allocator>>) {
        self.clear_meshes(device, allocator);
        self.destroy_pipelines(device);
        if let Some(shadow) = self.shadow.as_mut() { shadow.destroy(device, allocator); }
        self.shared.destroy(device, allocator);
    }
}

/// Local-space bounds of a baked item mesh before its display transform.
/// Empty meshes report a degenerate box at the origin.
fn mesh_bounds(vertices: &[ItemVertex]) -> (glam::Vec3, glam::Vec3) {
    if vertices.is_empty() {
        return (glam::Vec3::ZERO, glam::Vec3::ZERO);
    }

    let mut min = glam::Vec3::splat(f32::INFINITY);
    let mut max = glam::Vec3::splat(f32::NEG_INFINITY);
    for vertex in vertices {
        let position = glam::Vec3::from_array(vertex.position);
        min = min.min(position);
        max = max.max(position);
    }
    (min, max)
}

/// Vanilla stores a cardinal `BakedQuad.direction()` even when an element
/// rotation makes the geometric normal oblique, `UP` for a degenerate quad.
fn cardinal_normal(positions: &[[f32; 3]; 4]) -> glam::Vec3 {
    let direction = direction_from_positions(positions).unwrap_or(Direction::Up);
    glam::Vec3::from_array(direction.offset().map(|v| v as f32))
}

// Java item submits general quads first, then cullface buckets in its baked order.
fn gui_item_quad_order_key(quad: &crate::world::block::model::BakedQuad) -> (u8, u8) {
    use Direction::{Down, East, North, South, Up, West};
    let direction = quad.cullface.or(quad.shade_face).unwrap_or(Up);
    let rank = if quad.cullface.is_some() {
        match direction {
            North => 0,
            South => 1,
            East => 2,
            West => 3,
            Up => 4,
            Down => 5,
        }
    } else {
        match direction {
            Down => 0,
            Up => 1,
            North => 2,
            South => 3,
            West => 4,
            East => 5,
        }
    };
    (u8::from(quad.cullface.is_some()), rank)
}

fn build_item_mesh(
    model: &BakedModel,
    uv_map: &AtlasUVMap,
    vanilla_gui_order: bool,
) -> Vec<ItemVertex> {
    let mut quads: Vec<_> = model.quads.iter().collect();
    if vanilla_gui_order {
        quads.sort_by_key(|quad| gui_item_quad_order_key(quad));
    }
    let mut vertices = Vec::new();
    for quad in quads {
        let region = uv_map.get_region(&quad.texture);
        let u_span = region.u_max - region.u_min;
        let v_span = region.v_max - region.v_min;
        let rgb = quad.item_tint.rgb();
        let tint = crate::renderer::chunk::mesher::pack_tint_shifted([
            rgb[0] as f32 / 255.0,
            rgb[1] as f32 / 255.0,
            rgb[2] as f32 / 255.0,
        ]);
        let normal = pack_normal(cardinal_normal(&quad.positions));

        for i in [0, 1, 2, 2, 3, 0] {
            let p = quad.positions[i];
            vertices.push(ItemVertex {
                position: [p[0] - 0.5, p[1] - 0.5, p[2] - 0.5],
                tex_coords: [
                    region.u_min + quad.uvs[i][0] * u_span,
                    region.v_min + quad.uvs[i][1] * v_span,
                ],
                // GUI consumes the baked ITEMS_3D shade byte. Held and
                // dropped-item world shaders ignore light_tint.r and compute
                // context lighting from this packed cardinal normal instead.
                light_tint: crate::renderer::chunk::mesher::pack_light_tint(quad.shade_light, tint),
                normal,
            });
        }
    }
    vertices
}

#[cfg(test)]
fn build_extruded_item(img: &image::RgbaImage, region: AtlasRegion) -> Vec<ItemVertex> {
    let w = img.width();
    let h = img.height();
    let mask = SpriteAlphaMask {
        width: w,
        height: h,
        frames: vec![img.pixels().map(|pixel| pixel[3] != 0).collect()],
    };
    build_extruded_item_mask(&mask, region, [255, 255, 255])
}

fn build_extruded_item_mask(
    mask: &SpriteAlphaMask,
    region: AtlasRegion,
    rgb: [u8; 3],
) -> Vec<ItemVertex> {
    let w = mask.width as i32;
    let h = mask.height as i32;
    let mut vertices = Vec::new();

    let px = 1.0 / w as f32;
    let py = 1.0 / h as f32;
    let u_span = region.u_max - region.u_min;
    let v_span = region.v_max - region.v_min;
    let z_min = 7.5 / 16.0 - 0.5;
    let z_max = 8.5 / 16.0 - 0.5;

    let front = [
        [-0.5, -0.5, z_max],
        [0.5, -0.5, z_max],
        [0.5, 0.5, z_max],
        [-0.5, -0.5, z_max],
        [0.5, 0.5, z_max],
        [-0.5, 0.5, z_max],
    ];
    let front_uvs = [
        [region.u_min, region.v_max],
        [region.u_max, region.v_max],
        [region.u_max, region.v_min],
        [region.u_min, region.v_max],
        [region.u_max, region.v_min],
        [region.u_min, region.v_min],
    ];
    for i in 0..6 {
        vertices.push(ItemVertex {
            position: front[i],
            tex_coords: front_uvs[i],
            light_tint: crate::renderer::chunk::mesher::pack_light_tint(
                1.0,
                crate::renderer::chunk::mesher::pack_tint_shifted(
                    rgb.map(|channel| channel as f32 / 255.0),
                ),
            ),
            normal: pack_normal(glam::Vec3::Z),
        });
    }

    let back = [
        [0.5, -0.5, z_min],
        [-0.5, -0.5, z_min],
        [-0.5, 0.5, z_min],
        [0.5, -0.5, z_min],
        [-0.5, 0.5, z_min],
        [0.5, 0.5, z_min],
    ];
    // Vanilla's generated-item NORTH face uses UVs [16, 0, 0, 16].
    // With the north-face winding below that preserves the sprite's model-space
    // orientation: x=+0.5 samples the right side of the sprite, just like the
    // SOUTH face. Mirroring these UVs makes an asymmetric back silhouette no
    // longer line up with the generated side faces.
    let back_uvs = [
        [region.u_max, region.v_max],
        [region.u_min, region.v_max],
        [region.u_min, region.v_min],
        [region.u_max, region.v_max],
        [region.u_min, region.v_min],
        [region.u_max, region.v_min],
    ];
    for i in 0..6 {
        vertices.push(ItemVertex {
            position: back[i],
            tex_coords: back_uvs[i],
            light_tint: crate::renderer::chunk::mesher::pack_light_tint(
                1.0,
                crate::renderer::chunk::mesher::pack_tint_shifted(
                    rgb.map(|channel| channel as f32 / 255.0),
                ),
            ),
            normal: pack_normal(glam::Vec3::NEG_Z),
        });
    }

    for y in 0..h {
        for x in 0..w {
            let top_exposed = mask.has_exposed_edge(x, y, x, y - 1);
            let bottom_exposed = mask.has_exposed_edge(x, y, x, y + 1);
            let left_exposed = mask.has_exposed_edge(x, y, x - 1, y);
            let right_exposed = mask.has_exposed_edge(x, y, x + 1, y);
            if !(top_exposed || bottom_exposed || left_exposed || right_exposed) {
                continue;
            }
            let fx = x as f32 * px - 0.5;
            let fy = 0.5 - (y + 1) as f32 * py;
            let fx1 = fx + px;
            let fy1 = fy + py;
            // Vanilla ItemModelGenerator maps each side face from 0.1 to 0.9
            // inside its source pixel, measured across the exact sprite bounds.
            let u0 = region.u_min + (x as f32 + 0.1) * px * u_span;
            let u1 = region.u_min + (x as f32 + 0.9) * px * u_span;
            let v0 = region.v_min + (y as f32 + 0.1) * py * v_span;
            let v1 = region.v_min + (y as f32 + 0.9) * py * v_span;

            if top_exposed {
                push_side_quad(
                    &mut vertices,
                    fx,
                    fy1,
                    fx1,
                    fy1,
                    z_min,
                    z_max,
                    [[u0, v0], [u0, v1], [u1, v1], [u1, v0]],
                    0.8,
                    rgb,
                );
            }
            if bottom_exposed {
                push_side_quad(
                    &mut vertices,
                    fx1,
                    fy,
                    fx,
                    fy,
                    z_min,
                    z_max,
                    [[u1, v1], [u1, v0], [u0, v0], [u0, v1]],
                    0.8,
                    rgb,
                );
            }
            if left_exposed {
                push_side_quad(
                    &mut vertices,
                    fx,
                    fy,
                    fx,
                    fy1,
                    z_min,
                    z_max,
                    [[u1, v1], [u0, v1], [u0, v0], [u1, v0]],
                    0.8,
                    rgb,
                );
            }
            if right_exposed {
                push_side_quad(
                    &mut vertices,
                    fx1,
                    fy1,
                    fx1,
                    fy,
                    z_min,
                    z_max,
                    [[u0, v0], [u1, v0], [u1, v1], [u0, v1]],
                    0.8,
                    rgb,
                );
            }
        }
    }

    vertices
}

#[allow(clippy::too_many_arguments)]
fn push_side_quad(
    vertices: &mut Vec<ItemVertex>,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    z0: f32,
    z1: f32,
    uvs: [[f32; 2]; 4],
    light: f32,
    rgb: [u8; 3],
) {
    let positions = [[x0, y0, z0], [x0, y0, z1], [x1, y1, z1], [x1, y1, z0]];
    let normal = pack_normal(cardinal_normal(&positions));
    for i in [0, 1, 2, 0, 2, 3] {
        vertices.push(ItemVertex {
            position: positions[i],
            tex_coords: uvs[i],
            light_tint: crate::renderer::chunk::mesher::pack_light_tint(
                light,
                crate::renderer::chunk::mesher::pack_tint_shifted(
                    rgb.map(|channel| channel as f32 / 255.0),
                ),
            ),
            normal,
        });
    }
}

fn build_flat_quad(region: AtlasRegion, rgb: [u8; 3]) -> Vec<ItemVertex> {
    let h = 0.5;
    let positions = [
        [-h, -h, 0.0],
        [h, -h, 0.0],
        [h, h, 0.0],
        [-h, -h, 0.0],
        [h, h, 0.0],
        [-h, h, 0.0],
    ];
    let uvs = [
        [region.u_min, region.v_max],
        [region.u_max, region.v_max],
        [region.u_max, region.v_min],
        [region.u_min, region.v_max],
        [region.u_max, region.v_min],
        [region.u_min, region.v_min],
    ];
    positions
        .iter()
        .zip(uvs.iter())
        .map(|(p, uv)| ItemVertex {
            position: *p,
            tex_coords: *uv,
            light_tint: crate::renderer::chunk::mesher::pack_light_tint(
                1.0,
                crate::renderer::chunk::mesher::pack_tint_shifted(
                    rgb.map(|channel| channel as f32 / 255.0),
                ),
            ),
            normal: pack_normal(glam::Vec3::Z),
        })
        .collect()
}

pub(super) fn create_held_pipeline(
    device: &vk::Device,
    render_pass: vk::RenderPass,
    layout: vk::PipelineLayout,
) -> vk::Pipeline {
    create_pipeline_impl(
        device,
        render_pass,
        layout,
        vk::FrontFace::CounterClockwise,
        true,
        true,
        true,
        false,
    )
}

/// The dropped-item pipelines: `(cutout, translucent)`.
fn create_world_pipelines(
    device: &vk::Device,
    render_pass: vk::RenderPass,
    layout: vk::PipelineLayout,
) -> (vk::Pipeline, vk::Pipeline) {
    let world = |blend| {
        create_pipeline_impl(
            device,
            render_pass,
            layout,
            vk::FrontFace::CounterClockwise,
            true,
            blend,
            true,
            false,
        )
    };
    (world(false), world(true))
}

pub(super) fn create_pipeline_with_front_face(
    device: &vk::Device,
    render_pass: vk::RenderPass,
    layout: vk::PipelineLayout,
    front_face: vk::FrontFace,
) -> vk::Pipeline {
    create_pipeline_impl(device, render_pass, layout, front_face, false, true, true, false)
}

pub(super) fn create_gui_translucent_pipeline(
    device: &vk::Device,
    render_pass: vk::RenderPass,
    layout: vk::PipelineLayout,
    front_face: vk::FrontFace,
) -> vk::Pipeline {
    create_pipeline_impl(device, render_pass, layout, front_face, false, true, false, true)
}

fn create_shadow_pipeline(device: &vk::Device, render_pass: vk::RenderPass, layout: vk::PipelineLayout) -> vk::Pipeline {
    let vert = shader::create_shader_module(device, &shader::include_spirv!("world_shadow.vert.spv")[..]);
    let frag = shader::create_shader_module(device, &shader::include_spirv!("world_shadow.frag.spv")[..]);
    let stages = [
        vk::PipelineShaderStageCreateInfo { stage: vk::ShaderStageFlags::Vertex, module: vert, name: c"main".as_ptr(), ..Default::default() },
        vk::PipelineShaderStageCreateInfo { stage: vk::ShaderStageFlags::Fragment, module: frag, name: c"main".as_ptr(), ..Default::default() },
    ];
    let binding = vk::VertexInputBindingDescription { binding: 0, stride: size_of::<ShadowVertex>() as u32, input_rate: vk::VertexInputRate::Vertex };
    let attrs = [
        vk::VertexInputAttributeDescription { location: 0, binding: 0, format: vk::Format::R32G32B32Sfloat, offset: 0 },
        vk::VertexInputAttributeDescription { location: 1, binding: 0, format: vk::Format::R32G32Sfloat, offset: 12 },
        vk::VertexInputAttributeDescription { location: 2, binding: 0, format: vk::Format::R8G8B8A8Unorm, offset: 20 },
    ];
    let vertex = vk::PipelineVertexInputStateCreateInfo { vertex_binding_description_count: 1, vertex_binding_descriptions: &binding, vertex_attribute_description_count: 3, vertex_attribute_descriptions: attrs.as_ptr(), ..Default::default() };
    let assembly = vk::PipelineInputAssemblyStateCreateInfo { topology: vk::PrimitiveTopology::TriangleList, ..Default::default() };
    let viewport = vk::PipelineViewportStateCreateInfo { viewport_count: 1, scissor_count: 1, ..Default::default() };
    let raster = vk::PipelineRasterizationStateCreateInfo { polygon_mode: vk::PolygonMode::Fill, cull_mode: vk::CullModeFlags::None, front_face: vk::FrontFace::CounterClockwise, line_width: 1.0, ..Default::default() };
    let multisample = vk::PipelineMultisampleStateCreateInfo { rasterization_samples: vk::SampleCountFlags::Type1, ..Default::default() };
    let depth = vk::PipelineDepthStencilStateCreateInfo { depth_test_enable: vk::TRUE, depth_write_enable: vk::FALSE, depth_compare_op: vk::CompareOp::LessOrEqual, ..Default::default() };
    let blend = vk::PipelineColorBlendAttachmentState {
        blend_enable: vk::TRUE, src_color_blend_factor: vk::BlendFactor::SrcAlpha,
        dst_color_blend_factor: vk::BlendFactor::OneMinusSrcAlpha, color_blend_op: vk::BlendOp::Add,
        src_alpha_blend_factor: vk::BlendFactor::One, dst_alpha_blend_factor: vk::BlendFactor::OneMinusSrcAlpha,
        alpha_blend_op: vk::BlendOp::Add, color_write_mask: vk::ColorComponentFlags::RGBA,
    };
    let blending = vk::PipelineColorBlendStateCreateInfo { attachment_count: 1, attachments: &blend, ..Default::default() };
    let dynamic_states = [vk::DynamicState::Viewport, vk::DynamicState::Scissor];
    let dynamic = vk::PipelineDynamicStateCreateInfo { dynamic_state_count: 2, dynamic_states: dynamic_states.as_ptr(), ..Default::default() };
    let info = [vk::GraphicsPipelineCreateInfo {
        stage_count: 2, stages: stages.as_ptr(), vertex_input_state: &vertex, input_assembly_state: &assembly,
        viewport_state: &viewport, rasterization_state: &raster, multisample_state: &multisample,
        depth_stencil_state: &depth, color_blend_state: &blending, dynamic_state: &dynamic,
        layout, render_pass, subpass: 0, ..Default::default()
    }];
    let mut pipeline = vk::Pipeline::null();
    device.create_graphics_pipelines(vk::PipelineCache::null(), &info, None, slice::from_mut(&mut pipeline))
        .expect("failed to create world shadow pipeline");
    device.destroy_shader_module(vert, None);
    device.destroy_shader_module(frag, None);
    pipeline
}

fn create_pipeline_impl(
    device: &vk::Device,
    render_pass: vk::RenderPass,
    layout: vk::PipelineLayout,
    front_face: vk::FrontFace,
    world_lighting: bool,
    blend: bool,
    depth_write: bool,
    accumulate_alpha: bool,
) -> vk::Pipeline {
    let vert_spv: &[u8] = if world_lighting {
        &shader::include_spirv!("item_entity_world.vert.spv")[..]
    } else {
        &shader::include_spirv!("item_entity.vert.spv")[..]
    };
    let frag_spv = shader::include_spirv!("item_entity.frag.spv");
    let vert_mod = shader::create_shader_module(device, vert_spv);
    let frag_mod = shader::create_shader_module(device, frag_spv);

    let stages = [
        vk::PipelineShaderStageCreateInfo {
            stage: vk::ShaderStageFlags::Vertex,
            module: vert_mod,
            name: c"main".as_ptr(),
            ..Default::default()
        },
        vk::PipelineShaderStageCreateInfo {
            stage: vk::ShaderStageFlags::Fragment,
            module: frag_mod,
            name: c"main".as_ptr(),
            ..Default::default()
        },
    ];

    let binding = ItemVertex::binding_description();
    let attrs = ItemVertex::attribute_descriptions();

    let vertex_input = vk::PipelineVertexInputStateCreateInfo {
        vertex_binding_description_count: 1,
        vertex_binding_descriptions: &binding,
        vertex_attribute_description_count: attrs.len() as u32,
        vertex_attribute_descriptions: attrs.as_ptr(),
        ..Default::default()
    };
    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo {
        topology: vk::PrimitiveTopology::TriangleList,
        ..Default::default()
    };
    let viewport_state = vk::PipelineViewportStateCreateInfo {
        viewport_count: 1,
        scissor_count: 1,
        ..Default::default()
    };
    let rasterizer = vk::PipelineRasterizationStateCreateInfo {
        polygon_mode: vk::PolygonMode::Fill,
        cull_mode: vk::CullModeFlags::Back,
        front_face,
        line_width: 1.0,
        ..Default::default()
    };
    let multisampling = vk::PipelineMultisampleStateCreateInfo {
        rasterization_samples: vk::SampleCountFlags::Type1,
        ..Default::default()
    };
    let depth_stencil = vk::PipelineDepthStencilStateCreateInfo {
        depth_test_enable: vk::TRUE,
        depth_write_enable: if depth_write { vk::TRUE } else { vk::FALSE },
        // Vanilla 26.2's DepthStencilState.DEFAULT is inclusive
        // GREATER_THAN_OR_EQUAL under reversed-Z. Pomme uses conventional
        // depth here, so the equivalent comparison is LESS_OR_EQUAL. The
        // equality case matters where generated front/back planes meet their
        // per-pixel extrusion walls at the exact same depth.
        depth_compare_op: vk::CompareOp::LessOrEqual,
        ..Default::default()
    };
    let blend_attachment = vk::PipelineColorBlendAttachmentState {
        blend_enable: if blend { vk::TRUE } else { vk::FALSE },
        src_color_blend_factor: vk::BlendFactor::SrcAlpha,
        dst_color_blend_factor: vk::BlendFactor::OneMinusSrcAlpha,
        color_blend_op: vk::BlendOp::Add,
        src_alpha_blend_factor: vk::BlendFactor::One,
        dst_alpha_blend_factor: if accumulate_alpha {
            vk::BlendFactor::OneMinusSrcAlpha
        } else {
            vk::BlendFactor::Zero
        },
        alpha_blend_op: vk::BlendOp::Add,
        color_write_mask: vk::ColorComponentFlags::RGBA,
    };
    let color_blending = vk::PipelineColorBlendStateCreateInfo {
        attachment_count: 1,
        attachments: &blend_attachment,
        ..Default::default()
    };
    let dynamic_states = [vk::DynamicState::Viewport, vk::DynamicState::Scissor];
    let dynamic_state = vk::PipelineDynamicStateCreateInfo {
        dynamic_state_count: dynamic_states.len() as u32,
        dynamic_states: dynamic_states.as_ptr(),
        ..Default::default()
    };

    let info = [vk::GraphicsPipelineCreateInfo {
        stage_count: stages.len() as u32,
        stages: stages.as_ptr(),
        vertex_input_state: &vertex_input,
        input_assembly_state: &input_assembly,
        viewport_state: &viewport_state,
        rasterization_state: &rasterizer,
        multisample_state: &multisampling,
        depth_stencil_state: &depth_stencil,
        color_blend_state: &color_blending,
        dynamic_state: &dynamic_state,
        layout,
        render_pass,
        subpass: 0,
        ..Default::default()
    }];

    let mut pipeline = vk::Pipeline::null();
    device
        .create_graphics_pipelines(
            vk::PipelineCache::null(),
            &info,
            None,
            slice::from_mut(&mut pipeline),
        )
        .expect("failed to create item entity pipeline");

    device.destroy_shader_module(vert_mod, None);
    device.destroy_shader_module(frag_mod, None);

    pipeline
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit_region() -> AtlasRegion {
        AtlasRegion {
            u_min: 0.0,
            v_min: 0.0,
            u_max: 1.0,
            v_max: 1.0,
            pixel_rect: [0, 0, 1, 1],
            sprite: 0,
            opaque: true,
            translucent: false,
            alpha_counts: [0, 0, 1],
        }
    }

    fn unpack_uv(vertex: &ItemVertex) -> [f32; 2] {
        vertex.tex_coords
    }

    fn assert_uvs_near(vertices: &[ItemVertex], expected: &[[f32; 2]]) {
        assert_eq!(vertices.len(), expected.len());
        for (vertex, expected) in vertices.iter().zip(expected) {
            let actual = unpack_uv(vertex);
            assert!(
                (actual[0] - expected[0]).abs() <= f32::EPSILON
                    && (actual[1] - expected[1]).abs() <= f32::EPSILON,
                "UV {actual:?} differs from expected {expected:?}"
            );
        }
    }

    fn unpack_normal(vertex: &ItemVertex) -> glam::Vec3 {
        glam::Vec3::new(
            vertex.normal[0] as f32 / 127.0,
            vertex.normal[1] as f32 / 127.0,
            vertex.normal[2] as f32 / 127.0,
        )
    }

    #[test]
    fn gui_translucent_quad_order_matches_vanilla_buckets() {
        let quad = |direction, cullface| crate::world::block::model::BakedQuad {
            positions: [[0.0; 3]; 4],
            uvs: [[0.0; 2]; 4],
            texture: String::new(),
            cullface,
            tint_index: None,
            tint: crate::world::block::registry::Tint::None,
            item_tint: ItemTint::Untinted,
            shade_light: 1.0,
            shade_face: Some(direction),
        };
        use Direction::{Down, East, North, South, Up, West};
        let mut quads = vec![
            quad(Down, Some(Down)),
            quad(East, None),
            quad(South, Some(South)),
            quad(Up, None),
            quad(North, Some(North)),
            quad(West, None),
        ];
        quads.sort_by_key(gui_item_quad_order_key);
        let order: Vec<_> = quads
            .iter()
            .map(|quad| (quad.cullface.is_some(), quad.shade_face.unwrap()))
            .collect();
        assert_eq!(
            order,
            vec![
                (false, Up),
                (false, West),
                (false, East),
                (true, North),
                (true, South),
                (true, Down),
            ]
        );
    }

    #[test]
    fn measured_item_defaults_reach_the_raw_light_tint_field() {
        let cases = [
            (ItemTint::Grass { temperature: 0.5, downfall: 1.0, rgb: [124, 189, 107] }, [124, 189, 107]),
            (ItemTint::Untinted, [255, 255, 255]),
            (ItemTint::Constant([113, 195, 92]), [113, 195, 92]),
        ];
        for (tint, expected) in cases {
            let rgb = tint.rgb();
            let packed = crate::renderer::chunk::mesher::pack_tint_shifted(
                rgb.map(|channel| channel as f32 / 255.0),
            );
            assert_eq!(
                [
                    ((packed >> 8) & 0xff) as u8,
                    ((packed >> 16) & 0xff) as u8,
                    ((packed >> 24) & 0xff) as u8,
                ],
                expected
            );
        }
    }

    #[test]
    fn extruded_item_uses_item_rgb_for_front_back_and_mask_faces() {
        let mask = SpriteAlphaMask {
            width: 1,
            height: 1,
            frames: vec![vec![true]],
        };
        let vertices = build_extruded_item_mask(&mask, unit_region(), [124, 189, 107]);
        assert_eq!(vertices.len(), 36);
        for vertex in vertices {
            assert_eq!(
                [
                    ((vertex.light_tint >> 8) & 0xff) as u8,
                    ((vertex.light_tint >> 16) & 0xff) as u8,
                    ((vertex.light_tint >> 24) & 0xff) as u8,
                ],
                [124, 189, 107]
            );
        }
    }

    #[test]
    fn extruded_item_side_faces_wind_outward() {
        let image = image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 255, 255, 255]));
        let vertices = build_extruded_item(&image, unit_region());
        let expected_normals = [
            glam::Vec3::Y,
            glam::Vec3::NEG_Y,
            glam::Vec3::NEG_X,
            glam::Vec3::X,
        ];

        assert_eq!(vertices.len(), 36);
        for (face, expected) in vertices[12..]
            .as_chunks::<6>()
            .0
            .iter()
            .zip(expected_normals)
        {
            let p0 = glam::Vec3::from_array(face[0].position);
            let p1 = glam::Vec3::from_array(face[1].position);
            let p2 = glam::Vec3::from_array(face[2].position);
            let normal = (p1 - p0).cross(p2 - p0);
            assert!(
                normal.dot(expected) > 0.0,
                "side face normal {normal:?} points away from {expected:?}"
            );
        }
    }

    #[test]
    fn extruded_item_carries_vanilla_cardinal_normals() {
        let image = image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 255, 255, 255]));
        let vertices = build_extruded_item(&image, unit_region());
        let expected = [
            glam::Vec3::Z,
            glam::Vec3::NEG_Z,
            glam::Vec3::Y,
            glam::Vec3::NEG_Y,
            glam::Vec3::NEG_X,
            glam::Vec3::X,
        ];

        for (face, expected) in vertices.as_chunks::<6>().0.iter().zip(expected) {
            for vertex in face {
                assert_eq!(unpack_normal(vertex), expected);
            }
        }
    }

    #[test]
    fn baked_quad_normal_matches_vanilla_closest_cardinal_direction() {
        let angle = 22.5_f32.to_radians();
        let normal = glam::Vec3::new(angle.sin(), 0.0, angle.cos());
        let tangent = glam::Vec3::X * angle.cos() - glam::Vec3::Z * angle.sin();
        let up = glam::Vec3::Y;
        let p0 = -tangent * 0.5 - up * 0.5;
        let p1 = tangent * 0.5 - up * 0.5;
        let p2 = tangent * 0.5 + up * 0.5;
        let p3 = -tangent * 0.5 + up * 0.5;
        let mut positions = [p0.to_array(), p1.to_array(), p2.to_array(), p3.to_array()];

        // Ensure the synthetic winding points along the intended oblique normal.
        if crate::world::block::model::quad_normal(&positions)
            .unwrap()
            .dot(normal)
            < 0.0
        {
            positions.reverse();
        }
        assert_eq!(cardinal_normal(&positions), glam::Vec3::Z);
    }

    #[test]
    fn animated_extrusion_unions_exposed_edges_per_frame() {
        let mask = SpriteAlphaMask {
            width: 2,
            height: 1,
            frames: vec![vec![true, false], vec![false, true]],
        };
        let vertices = build_extruded_item_mask(&mask, unit_region(), [255, 255, 255]);

        // Each frame exposes the shared edge from one side, so vanilla emits
        // eight side quads total. Collapsing the frames into one opacity bitmap
        // would incorrectly remove both shared-edge quads and yield 48 vertices.
        assert_eq!(vertices.len(), 12 + 8 * 6);
    }

    #[test]
    fn extruded_item_back_face_preserves_sprite_orientation() {
        let image = image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 255, 255, 255]));
        let vertices = build_extruded_item(&image, unit_region());

        assert_uvs_near(
            &vertices[6..12],
            &[
                [1.0, 1.0],
                [0.0, 1.0],
                [0.0, 0.0],
                [1.0, 1.0],
                [0.0, 0.0],
                [1.0, 0.0],
            ],
        );
    }

    #[test]
    fn extruded_item_front_and_back_use_true_sprite_bounds() {
        let image = image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 255, 255, 255]));
        let region = AtlasRegion {
            u_min: 16.0 / 64.0,
            v_min: 8.0 / 64.0,
            u_max: 32.0 / 64.0,
            v_max: 24.0 / 64.0,
            pixel_rect: [16, 8, 16, 16],
            sprite: 1,
            opaque: true,
            translucent: false,
            alpha_counts: [0, 0, 256],
        };
        let vertices = build_extruded_item(&image, region);

        assert_uvs_near(
            &vertices[..12],
            &[
                [region.u_min, region.v_max],
                [region.u_max, region.v_max],
                [region.u_max, region.v_min],
                [region.u_min, region.v_max],
                [region.u_max, region.v_min],
                [region.u_min, region.v_min],
                [region.u_max, region.v_max],
                [region.u_min, region.v_max],
                [region.u_min, region.v_min],
                [region.u_max, region.v_max],
                [region.u_min, region.v_min],
                [region.u_max, region.v_min],
            ],
        );
    }

    #[test]
    fn extruded_item_side_faces_match_vanilla_pixel_inset_uvs() {
        let image = image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 255, 255, 255]));
        let vertices = build_extruded_item(&image, unit_region());
        let expected = [
            // UP
            [0.1, 0.1],
            [0.1, 0.9],
            [0.9, 0.9],
            [0.1, 0.1],
            [0.9, 0.9],
            [0.9, 0.1],
            // DOWN
            [0.9, 0.9],
            [0.9, 0.1],
            [0.1, 0.1],
            [0.9, 0.9],
            [0.1, 0.1],
            [0.1, 0.9],
            // LEFT / west-facing geometry
            [0.9, 0.9],
            [0.1, 0.9],
            [0.1, 0.1],
            [0.9, 0.9],
            [0.1, 0.1],
            [0.9, 0.1],
            // RIGHT / east-facing geometry
            [0.1, 0.1],
            [0.9, 0.1],
            [0.9, 0.9],
            [0.1, 0.1],
            [0.9, 0.9],
            [0.1, 0.9],
        ];

        assert_uvs_near(&vertices[12..], &expected);
    }
}
