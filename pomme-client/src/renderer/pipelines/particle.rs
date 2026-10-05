use std::slice;
use std::sync::{Arc, Mutex};

use glam::{Quat, Vec3};
use pomme_gpu_allocator::vulkan::{Allocation, Allocator};
use pyronyx::vk;

use crate::renderer::camera::{Camera, CameraUniform};
use crate::renderer::chunk::atlas::TextureAtlas;
use crate::renderer::{MAX_FRAMES_IN_FLIGHT, shader, util};

/// Vanilla `ParticleGroup.MAX_PARTICLES`: the particle store's hard cap,
/// and thus the per-frame vertex buffer size.
pub const MAX_PARTICLE_QUADS: usize = 16384;
const MAX_VERTS: usize = MAX_PARTICLE_QUADS * 6;

/// One particle quad, extracted per frame from the particle store. Most use a
/// full-camera billboard; Java LOOKAT_Y quads keep a yaw-only orientation.
pub struct ParticleQuad {
    /// Partial-tick-lerped world-space position (quad center).
    pub pos: [f32; 3],
    /// Vanilla `quadSize`; the quad spans twice this.
    pub size: f32,
    pub u0: f32,
    pub u1: f32,
    pub v0: f32,
    pub v1: f32,
    /// Java `ARGB.colorFromFloat` packed RGBA8, before lightmap multiplication.
    pub color: u32,
    /// Packed byte UV2 (block in low byte, sky in next byte); regular nibbles
    /// are level*16 and smooth emission retains its sub-nibble values.
    pub light_uv: u32,
    /// Vanilla `SingleQuadParticle.Layer`: false = opaque/cutout terrain
    /// layer, true = alpha-blended translucent layer.
    pub translucent: bool,
    /// Java `SingleQuadParticle.FacingCameraMode.LOOKAT_Y`; all other quads
    /// use the normal full-camera billboard.
    pub look_at_y: bool,
    pub rotation: [f32; 4],
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct ParticleVertex {
    position: [f32; 3],
    uv: [f32; 2],
    color: u32,
    light_uv: u32,
}

fn particle_billboard_axes(right: Vec3, up: Vec3, rotation: [f32; 4]) -> (Vec3, Vec3) {
    // Most particle kinds carry the identity rotation. Avoid two quaternion-vector
    // products per quad in that case; rotated particles keep the original path.
    if rotation == Quat::IDENTITY.to_array() {
        (right, up)
    } else {
        let rotation = Quat::from_array(rotation);
        (rotation * right, rotation * up)
    }
}

fn particle_camera_axes(camera: &Camera, look_at_y: bool) -> (Vec3, Vec3) {
    if look_at_y {
        let orientation = camera.orientation();
        // Java LOOKAT_Y uses Quaternion.set(0, camera.rotation().y, 0,
        // camera.rotation().w), intentionally discarding camera pitch.
        let yaw = Quat::from_xyzw(0.0, orientation.y, 0.0, orientation.w).normalize();
        (yaw * Vec3::X, yaw * Vec3::Y)
    } else {
        camera.billboard_axes()
    }
}

fn particle_corner_position(
    center: Vec3,
    right: Vec3,
    up: Vec3,
    nx: f32,
    ny: f32,
    quad_size: f32,
) -> Vec3 {
    center + (right * nx + up * ny) * quad_size
}

/// The CPU-side vertex conversion used immediately before the GPU upload.
pub(crate) fn build_particle_vertices(
    camera: &Camera,
    quads: &[ParticleQuad],
) -> (Vec<ParticleVertex>, usize) {
    let (xyz_right, xyz_up) = camera.billboard_axes();
    let (look_right, look_up) = particle_camera_axes(camera, true);
    let mut verts = Vec::with_capacity(quads.len().min(MAX_PARTICLE_QUADS) * 6);
    let emit = |verts: &mut Vec<ParticleVertex>, translucent: bool| {
        for quad in quads.iter().filter(|q| q.translucent == translucent) {
            if verts.len() >= MAX_VERTS {
                return;
            }
            let center = Vec3::from(quad.pos);
            let (base_right, base_up) = if quad.look_at_y {
                (look_right, look_up)
            } else {
                (xyz_right, xyz_up)
            };
            let (rotated_right, rotated_up) =
                particle_billboard_axes(base_right, base_up, quad.rotation);
            let corner = |nx: f32, ny: f32, u: f32, v: f32| ParticleVertex {
                position: particle_corner_position(
                    center,
                    rotated_right,
                    rotated_up,
                    nx,
                    ny,
                    quad.size,
                )
                .into(),
                uv: [u, v],
                color: quad.color,
                light_uv: quad.light_uv,
            };
            let corners = [
                corner(1.0, -1.0, quad.u1, quad.v1),
                corner(1.0, 1.0, quad.u1, quad.v0),
                corner(-1.0, 1.0, quad.u0, quad.v0),
                corner(-1.0, -1.0, quad.u0, quad.v1),
            ];
            for &i in &[0usize, 1, 2, 0, 2, 3] {
                verts.push(corners[i]);
            }
        }
    };
    emit(&mut verts, false);
    let opaque_verts = verts.len();
    emit(&mut verts, true);
    (verts, opaque_verts)
}

pub struct ParticlePipeline {
    pipeline: vk::Pipeline,
    translucent_pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    camera_layout: vk::DescriptorSetLayout,
    atlas_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    camera_sets: Vec<vk::DescriptorSet>,
    atlas_set: vk::DescriptorSet,
    atlas_sampler: vk::Sampler,
    camera_buffers: Vec<vk::Buffer>,
    camera_allocations: Vec<Option<Allocation>>,
    vertex_buffers: Vec<vk::Buffer>,
    vertex_allocations: Vec<Option<Allocation>>,
    vertices: Vec<ParticleVertex>,
}

impl ParticlePipeline {
    pub fn new(
        device: &vk::Device,
        render_pass: vk::RenderPass,
        allocator: &Arc<Mutex<Allocator>>,
        atlas: &TextureAtlas,
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

        let layouts = [camera_layout, atlas_layout];
        let layout_info = vk::PipelineLayoutCreateInfo {
            set_layout_count: layouts.len() as u32,
            set_layouts: layouts.as_ptr(),
            ..Default::default()
        };
        let pipeline_layout = device
            .create_pipeline_layout(&layout_info, None)
            .expect("failed to create particle pipeline layout");

        let pipeline = create_pipeline(device, render_pass, pipeline_layout, false);
        let translucent_pipeline = create_pipeline(device, render_pass, pipeline_layout, true);

        let pool_sizes = [
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::UniformBuffer,
                descriptor_count: MAX_FRAMES_IN_FLIGHT as u32,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::CombinedImageSampler,
                descriptor_count: 1,
            },
        ];
        let pool_info = vk::DescriptorPoolCreateInfo {
            max_sets: (MAX_FRAMES_IN_FLIGHT + 1) as u32,
            pool_size_count: pool_sizes.len() as u32,
            pool_sizes: pool_sizes.as_ptr(),
            ..Default::default()
        };
        let descriptor_pool = device
            .create_descriptor_pool(&pool_info, None)
            .expect("failed to create particle descriptor pool");

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
            .expect("failed to allocate particle camera sets");

        let atlas_alloc_info = vk::DescriptorSetAllocateInfo {
            descriptor_pool,
            descriptor_set_count: 1,
            set_layouts: &atlas_layout,
            ..Default::default()
        };
        let mut atlas_set = vk::DescriptorSet::null();
        device
            .allocate_descriptor_sets(&atlas_alloc_info, slice::from_mut(&mut atlas_set))
            .expect("failed to allocate particle atlas set");
        // Vanilla's particles atlas has no mip chain. Pomme stores particle
        // sprites in the combined atlas image, so clamp this consumer to LOD 0.
        let atlas_sampler = unsafe { util::create_nearest_sampler(device) };

        let mut camera_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        let mut camera_allocations: Vec<Option<Allocation>> =
            Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        for &set in &camera_sets {
            let (buf, alloc) = util::create_uniform_buffer(
                device,
                allocator,
                std::mem::size_of::<CameraUniform>() as u64,
                "particle_camera",
            );
            let buffer_info = vk::DescriptorBufferInfo {
                buffer: buf,
                offset: 0,
                range: std::mem::size_of::<CameraUniform>() as u64,
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

        let mut vertex_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        let mut vertex_allocations: Vec<Option<Allocation>> =
            Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        let vertex_bytes = (MAX_VERTS * std::mem::size_of::<ParticleVertex>()) as u64;
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            let (buf, alloc) = util::create_host_buffer(
                device,
                allocator,
                vertex_bytes,
                vk::BufferUsageFlags::VertexBuffer,
                "particle_vertices",
            );
            vertex_buffers.push(buf);
            vertex_allocations.push(Some(alloc));
        }

        let this = Self {
            pipeline,
            translucent_pipeline,
            pipeline_layout,
            camera_layout,
            atlas_layout,
            descriptor_pool,
            camera_sets,
            atlas_set,
            atlas_sampler,
            camera_buffers,
            camera_allocations,
            vertex_buffers,
            vertex_allocations,
            vertices: Vec::new(),
        };
        this.rebind_atlas(device, atlas);
        this
    }

    pub fn rebind_atlas(&self, device: &vk::Device, atlas: &TextureAtlas) {
        let image_info = vk::DescriptorImageInfo {
            sampler: self.atlas_sampler,
            image_view: atlas.view,
            image_layout: vk::ImageLayout::ShaderReadOnlyOptimal,
        };
        let write = vk::WriteDescriptorSet {
            dst_set: self.atlas_set,
            dst_binding: 0,
            descriptor_type: vk::DescriptorType::CombinedImageSampler,
            descriptor_count: 1,
            image_info: &image_info,
            ..Default::default()
        };
        device.update_descriptor_sets(&[write], &[]);
    }

    pub fn update_camera(&mut self, frame: usize, uniform: &CameraUniform) {
        let bytes = bytemuck::bytes_of(uniform);
        if let Some(alloc) = self.camera_allocations[frame].as_mut() {
            alloc.mapped_slice_mut().unwrap()[..bytes.len()].copy_from_slice(bytes);
        }
    }

    pub fn update_and_draw(
        &mut self,
        cmd: vk::CommandBuffer,
        frame: usize,
        camera: &Camera,
        quads: &[ParticleQuad],
    ) {
        if quads.is_empty() {
            return;
        }

        // Opaque quads first, then translucent (Vanilla's separate layers).
        let (vertices, opaque_verts) = build_particle_vertices(camera, quads);
        self.vertices = vertices;
        let verts = &mut self.vertices;

        let bytes = bytemuck::cast_slice::<ParticleVertex, u8>(&verts);
        if let Some(alloc) = self.vertex_allocations[frame].as_mut() {
            alloc.mapped_slice_mut().unwrap()[..bytes.len()].copy_from_slice(bytes);
        }

        cmd.bind_vertex_buffers(0, &[self.vertex_buffers[frame]], &[0]);
        cmd.bind_descriptor_sets(
            vk::PipelineBindPoint::Graphics,
            self.pipeline_layout,
            0,
            &[self.camera_sets[frame], self.atlas_set],
            &[],
        );
        if opaque_verts > 0 {
            cmd.bind_pipeline(vk::PipelineBindPoint::Graphics, self.pipeline);
            cmd.draw(opaque_verts as u32, 1, 0, 0);
        }
        if verts.len() > opaque_verts {
            cmd.bind_pipeline(vk::PipelineBindPoint::Graphics, self.translucent_pipeline);
            cmd.draw(
                (verts.len() - opaque_verts) as u32,
                1,
                opaque_verts as u32,
                0,
            );
        }
    }

    pub fn recreate_pipeline(&mut self, device: &vk::Device, render_pass: vk::RenderPass) {
        device.destroy_pipeline(self.pipeline, None);
        device.destroy_pipeline(self.translucent_pipeline, None);
        self.pipeline = create_pipeline(device, render_pass, self.pipeline_layout, false);
        self.translucent_pipeline =
            create_pipeline(device, render_pass, self.pipeline_layout, true);
    }

    pub fn destroy(&mut self, device: &vk::Device, allocator: &Arc<Mutex<Allocator>>) {
        let mut alloc = allocator.lock().unwrap();
        for i in 0..MAX_FRAMES_IN_FLIGHT {
            device.destroy_buffer(self.camera_buffers[i], None);
            if let Some(a) = self.camera_allocations[i].take() {
                alloc.free(a).ok();
            }
            device.destroy_buffer(self.vertex_buffers[i], None);
            if let Some(a) = self.vertex_allocations[i].take() {
                alloc.free(a).ok();
            }
        }
        drop(alloc);

        device.destroy_pipeline(self.pipeline, None);
        device.destroy_pipeline(self.translucent_pipeline, None);
        device.destroy_sampler(self.atlas_sampler, None);
        device.destroy_pipeline_layout(self.pipeline_layout, None);
        device.destroy_descriptor_pool(self.descriptor_pool, None);
        device.destroy_descriptor_set_layout(self.camera_layout, None);
        device.destroy_descriptor_set_layout(self.atlas_layout, None);
    }
}

#[cfg(test)]
mod tests {
    use glam::{Quat, Vec3};

    use super::{
        ParticleVertex, build_particle_vertices, particle_billboard_axes, particle_camera_axes,
        particle_corner_position,
    };

    #[test]
    fn unrotated_particle_axes_match_the_original_quaternion_path() {
        for (right, up) in [
            (Vec3::X, Vec3::Y),
            (Vec3::new(0.25, -0.8, 0.54), Vec3::new(-0.9, 0.1, 0.3)),
        ] {
            for rotation in [Quat::IDENTITY, Quat::from_rotation_z(0.6)] {
                let (actual_right, actual_up) =
                    particle_billboard_axes(right, up, rotation.to_array());
                assert_eq!(actual_right, rotation * right);
                assert_eq!(actual_up, rotation * up);
                for (nx, ny) in [(1.0, -1.0), (1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0)] {
                    assert_eq!(
                        particle_corner_position(
                            Vec3::new(2.0, 3.0, -4.0),
                            actual_right,
                            actual_up,
                            nx,
                            ny,
                            0.5
                        ),
                        particle_corner_position(
                            Vec3::new(2.0, 3.0, -4.0),
                            rotation * right,
                            rotation * up,
                            nx,
                            ny,
                            0.5
                        )
                    );
                }
            }
        }
    }

    #[test]
    fn lookat_y_discards_camera_pitch_while_xyz_tracks_it() {
        use crate::renderer::camera::Camera;

        let mut camera = Camera::new(16.0 / 9.0);
        camera.look_dir = crate::entity::components::LookDirection::new(35.0, 0.0);
        let (yaw_right, yaw_up) = particle_camera_axes(&camera, true);
        let (xyz_right, xyz_up) = particle_camera_axes(&camera, false);
        camera.look_dir = crate::entity::components::LookDirection::new(35.0, 55.0);
        let (pitched_yaw_right, pitched_yaw_up) = particle_camera_axes(&camera, true);
        let (pitched_xyz_right, pitched_xyz_up) = particle_camera_axes(&camera, false);

        assert!((yaw_right - pitched_yaw_right).length() < 1e-5);
        assert!((yaw_up - pitched_yaw_up).length() < 1e-5);
        assert!((xyz_right - pitched_xyz_right).length() < 1e-5);
        assert!((xyz_up - pitched_xyz_up).length() > 0.1);
        assert!(yaw_up.y > 0.999);
        assert!(yaw_right.y.abs() < 1e-5);
    }

    #[test]
    fn particle_vertex_stride_matches_shader_attribute_offsets() {
        assert_eq!(std::mem::size_of::<ParticleVertex>(), 28);
        assert_eq!(std::mem::offset_of!(ParticleVertex, position), 0);
        assert_eq!(std::mem::offset_of!(ParticleVertex, uv), 12);
        assert_eq!(std::mem::offset_of!(ParticleVertex, color), 20);
        assert_eq!(std::mem::offset_of!(ParticleVertex, light_uv), 24);
    }

    #[test]
    fn pure_vertex_builder_preserves_material_uv_light_and_layer_order() {
        use crate::renderer::ParticleQuad;
        use crate::renderer::camera::Camera;

        let quad = |u0, u1, translucent| ParticleQuad {
            pos: [2.0, 3.0, 4.0],
            size: 0.25,
            u0,
            u1,
            v0: 0.2,
            v1: 0.4,
            color: 0x1234_5678,
            light_uv: 0x00f0_00a0,
            translucent,
            look_at_y: false,
            rotation: Quat::IDENTITY.to_array(),
        };
        let camera = Camera::new(16.0 / 9.0);
        let (vertices, opaque_count) =
            build_particle_vertices(&camera, &[quad(0.11, 0.19, true), quad(0.31, 0.39, false)]);
        assert_eq!(vertices.len(), 12);
        assert_eq!(opaque_count, 6);
        assert_eq!(vertices[0].uv, [0.39, 0.4]);
        assert_eq!(vertices[2].uv, [0.31, 0.2]);
        assert_eq!(vertices[0].color, 0x1234_5678);
        assert_eq!(vertices[0].light_uv, 0x00f0_00a0);
        assert_eq!(vertices[6].uv, [0.19, 0.4]);
        assert_eq!(vertices[8].uv, [0.11, 0.2]);
    }

    #[test]
    fn cpu_vertex_builder_applies_lookat_y_before_output() {
        use crate::renderer::ParticleQuad;
        use crate::renderer::camera::Camera;

        let quad = |look_at_y| ParticleQuad {
            pos: [0.0; 3],
            size: 1.0,
            u0: 0.1,
            u1: 0.2,
            v0: 0.3,
            v1: 0.4,
            color: u32::MAX,
            light_uv: 0,
            translucent: false,
            look_at_y,
            rotation: Quat::IDENTITY.to_array(),
        };
        let mut camera = Camera::new(16.0 / 9.0);
        camera.look_dir = crate::entity::components::LookDirection::new(35.0, 0.0);
        let (flat_look, _) = build_particle_vertices(&camera, &[quad(true)]);
        let (flat_billboard, _) = build_particle_vertices(&camera, &[quad(false)]);
        camera.look_dir = crate::entity::components::LookDirection::new(35.0, 55.0);
        let (pitched_look, _) = build_particle_vertices(&camera, &[quad(true)]);
        let (pitched_billboard, _) = build_particle_vertices(&camera, &[quad(false)]);
        assert!(
            (Vec3::from(flat_look[0].position) - Vec3::from(pitched_look[0].position)).length()
                < 1e-5
        );
        assert!(
            (Vec3::from(flat_billboard[0].position) - Vec3::from(pitched_billboard[0].position))
                .length()
                > 0.1
        );
    }

    #[test]
    fn particle_quad_size_is_the_vertex_half_extent() {
        let corner = particle_corner_position(Vec3::ZERO, Vec3::X, Vec3::Y, -1.0, -1.0, 1.5);
        assert_eq!(corner, Vec3::new(-1.5, -1.5, 0.0));
        assert_eq!(
            particle_corner_position(Vec3::ZERO, Vec3::X, Vec3::Y, 1.0, 1.0, 1.5),
            Vec3::new(1.5, 1.5, 0.0)
        );
        let rotated = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2) * Vec3::X;
        assert!((rotated - Vec3::Y).length() < 1e-6);
    }
}

fn create_pipeline(
    device: &vk::Device,
    render_pass: vk::RenderPass,
    layout: vk::PipelineLayout,
    translucent: bool,
) -> vk::Pipeline {
    let vert_spv = shader::include_spirv!("particle.vert.spv");
    let frag_spv = shader::include_spirv!("particle.frag.spv");
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

    let binding_descs = [vk::VertexInputBindingDescription {
        binding: 0,
        stride: std::mem::size_of::<ParticleVertex>() as u32,
        input_rate: vk::VertexInputRate::Vertex,
    }];
    let attr_descs = [
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
            format: vk::Format::R32Uint,
            offset: 24,
        },
    ];
    let vertex_input = vk::PipelineVertexInputStateCreateInfo {
        vertex_binding_description_count: binding_descs.len() as u32,
        vertex_binding_descriptions: binding_descs.as_ptr(),
        vertex_attribute_description_count: attr_descs.len() as u32,
        vertex_attribute_descriptions: attr_descs.as_ptr(),
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
    // Billboards always face the camera, so culling is a no-op; vanilla's
    // back-face cull is skipped rather than fighting winding conventions.
    let rasterizer = vk::PipelineRasterizationStateCreateInfo {
        polygon_mode: vk::PolygonMode::Fill,
        cull_mode: vk::CullModeFlags::None,
        front_face: vk::FrontFace::CounterClockwise,
        line_width: 1.0,
        ..Default::default()
    };
    let multisampling = vk::PipelineMultisampleStateCreateInfo {
        rasterization_samples: vk::SampleCountFlags::Type1,
        ..Default::default()
    };
    // Vanilla PARTICLE_SNIPPET: depth test AND write for both layers.
    // OPAQUE_PARTICLE has no blending (alpha is handled by the fragment
    // discard); TRANSLUCENT_PARTICLE adds standard alpha blending.
    let depth_stencil = vk::PipelineDepthStencilStateCreateInfo {
        depth_test_enable: vk::TRUE,
        depth_write_enable: vk::TRUE,
        depth_compare_op: vk::CompareOp::Less,
        ..Default::default()
    };
    let blend_attachment = if translucent {
        vk::PipelineColorBlendAttachmentState {
            blend_enable: vk::TRUE,
            src_color_blend_factor: vk::BlendFactor::SrcAlpha,
            dst_color_blend_factor: vk::BlendFactor::OneMinusSrcAlpha,
            color_blend_op: vk::BlendOp::Add,
            src_alpha_blend_factor: vk::BlendFactor::One,
            dst_alpha_blend_factor: vk::BlendFactor::OneMinusSrcAlpha,
            alpha_blend_op: vk::BlendOp::Add,
            color_write_mask: vk::ColorComponentFlags::RGBA,
        }
    } else {
        vk::PipelineColorBlendAttachmentState {
            blend_enable: vk::FALSE,
            color_write_mask: vk::ColorComponentFlags::RGBA,
            ..Default::default()
        }
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

    let mut pipeline = [vk::Pipeline::null()];
    device
        .create_graphics_pipelines(vk::PipelineCache::null(), &info, None, &mut pipeline)
        .expect("failed to create particle pipeline");

    device.destroy_shader_module(vert_mod, None);
    device.destroy_shader_module(frag_mod, None);

    pipeline[0]
}
