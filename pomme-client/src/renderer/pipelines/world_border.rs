use std::path::Path;
use std::sync::{Arc, Mutex};

use pomme_gpu_allocator::vulkan::{Allocation, Allocator};
use pyronyx::vk;

use crate::assets::{AssetIndex, resolve_asset_path};
use crate::renderer::camera::CameraUniform;
use crate::renderer::{MAX_FRAMES_IN_FLIGHT, shader, util};
use crate::world::border::{BorderStatus, WorldBorder};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    uv: [f32; 2],
    brightness: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BorderBounds {
    pub min_x: f64,
    pub max_x: f64,
    pub min_z: f64,
    pub max_z: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BorderDraw {
    pub bounds: BorderBounds,
    pub alpha: f32,
    pub offset: f32,
    pub sides: [bool; 4],
    pub status: BorderStatus,
}

/// Vanilla's extract rule: only borders whose nearest edge is within render
/// distance are visible.
pub fn extract_border(
    border: &WorldBorder,
    partial_tick: f32,
    camera: [f64; 3],
    render_distance: f64,
    offset: f32,
) -> Option<BorderDraw> {
    if !render_distance.is_finite() || render_distance <= 0.0 {
        return None;
    }
    let [min_x, max_x, min_z, max_z] = border.bounds_at(partial_tick);
    if camera[0] < min_x - render_distance
        || camera[0] > max_x + render_distance
        || camera[2] < min_z - render_distance
        || camera[2] > max_z + render_distance
    {
        return None;
    }
    // Official 26.2 extract rejects the central region farther than the render
    // distance from all four edges; getDistanceToBorder is signed outside.
    if camera[0] < max_x - render_distance
        && camera[0] > min_x + render_distance
        && camera[2] < max_z - render_distance
        && camera[2] > min_z + render_distance
    {
        return None;
    }
    let d = (camera[0] - min_x)
        .min(max_x - camera[0])
        .min(camera[2] - min_z)
        .min(max_z - camera[2]);
    let alpha = (1.0 - d / render_distance).powi(4).clamp(0.0, 1.0) as f32;
    if alpha <= 0.0 {
        return None;
    }
    let distances = [
        (camera[2] - max_z).abs(),
        (camera[0] - min_x).abs(),
        (camera[2] - min_z).abs(),
        (camera[0] - max_x).abs(),
    ];
    Some(BorderDraw {
        bounds: BorderBounds {
            min_x,
            max_x,
            min_z,
            max_z,
        },
        alpha,
        offset,
        sides: distances.map(|distance| distance < render_distance),
        status: border.status(),
    })
}

pub struct WorldBorderPipeline {
    pipeline: vk::Pipeline,
    layout: vk::PipelineLayout,
    pool: vk::DescriptorPool,
    camera_layout: vk::DescriptorSetLayout,
    texture_layout: vk::DescriptorSetLayout,
    camera_sets: Vec<vk::DescriptorSet>,
    texture_sets: [vk::DescriptorSet; 3],
    camera_buffers: Vec<vk::Buffer>,
    camera_allocs: Vec<Option<Allocation>>,
    vertex_buffers: Vec<vk::Buffer>,
    vertex_allocs: Vec<Option<Allocation>>,
    sampler: vk::Sampler,
    images: [vk::Image; 3],
    views: [vk::ImageView; 3],
    image_allocs: Vec<Option<Allocation>>,
}

impl WorldBorderPipeline {
    pub fn new(
        device: &vk::Device,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
        render_pass: vk::RenderPass,
        allocator: &Arc<Mutex<Allocator>>,
        assets: &Path,
        index: &Option<AssetIndex>,
    ) -> Self {
        let camera_layout = util::create_descriptor_set_layout(
            device,
            vk::DescriptorType::UniformBuffer,
            vk::ShaderStageFlags::Vertex,
        );
        let texture_layout = util::create_descriptor_set_layout(
            device,
            vk::DescriptorType::CombinedImageSampler,
            vk::ShaderStageFlags::Fragment,
        );
        let layouts = [camera_layout, texture_layout];
        let li = vk::PipelineLayoutCreateInfo {
            set_layout_count: 2,
            set_layouts: layouts.as_ptr(),
            ..Default::default()
        };
        let layout = device.create_pipeline_layout(&li, None).unwrap();
        let pipeline = create_pipeline(device, render_pass, layout);
        let sizes = [
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::UniformBuffer,
                descriptor_count: MAX_FRAMES_IN_FLIGHT as u32,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::CombinedImageSampler,
                descriptor_count: 3,
            },
        ];
        let pi = vk::DescriptorPoolCreateInfo {
            max_sets: (MAX_FRAMES_IN_FLIGHT + 3) as u32,
            pool_size_count: 2,
            pool_sizes: sizes.as_ptr(),
            ..Default::default()
        };
        let pool = device.create_descriptor_pool(&pi, None).unwrap();
        let set_layouts: Vec<_> = (0..MAX_FRAMES_IN_FLIGHT).map(|_| camera_layout).collect();
        let ai = vk::DescriptorSetAllocateInfo {
            descriptor_pool: pool,
            descriptor_set_count: set_layouts.len() as u32,
            set_layouts: set_layouts.as_ptr(),
            ..Default::default()
        };
        let mut camera_sets = vec![vk::DescriptorSet::null(); MAX_FRAMES_IN_FLIGHT];
        device
            .allocate_descriptor_sets(&ai, &mut camera_sets)
            .unwrap();
        let texture_layouts = [texture_layout; 3];
        let ai = vk::DescriptorSetAllocateInfo {
            descriptor_pool: pool,
            descriptor_set_count: 3,
            set_layouts: texture_layouts.as_ptr(),
            ..Default::default()
        };
        let mut texture_sets = [vk::DescriptorSet::null(); 3];
        device
            .allocate_descriptor_sets(&ai, &mut texture_sets)
            .unwrap();
        let mut camera_buffers = Vec::new();
        let mut camera_allocs = Vec::new();
        for &set in &camera_sets {
            let (buf, alloc) = util::create_uniform_buffer(
                device,
                allocator,
                size_of::<CameraUniform>() as u64,
                "border_camera",
            );
            let bi = vk::DescriptorBufferInfo {
                buffer: buf,
                offset: 0,
                range: size_of::<CameraUniform>() as u64,
            };
            device.update_descriptor_sets(
                &[vk::WriteDescriptorSet {
                    dst_set: set,
                    dst_binding: 0,
                    descriptor_type: vk::DescriptorType::UniformBuffer,
                    descriptor_count: 1,
                    buffer_info: &bi,
                    ..Default::default()
                }],
                &[],
            );
            camera_buffers.push(buf);
            camera_allocs.push(Some(alloc));
        }
        let mut vertex_buffers = Vec::new();
        let mut vertex_allocs = Vec::new();
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            let (b, a) = util::create_host_buffer(
                device,
                allocator,
                (24 * size_of::<Vertex>()) as u64,
                vk::BufferUsageFlags::VertexBuffer,
                "border_vertices",
            );
            vertex_buffers.push(b);
            vertex_allocs.push(Some(a));
        }
        let path = resolve_asset_path(assets, index, "minecraft/textures/misc/forcefield.png");
        let (pixels, w, h) = util::load_png(&path).unwrap_or_else(|| {
            tracing::warn!("Failed to load forcefield texture, using fallback");
            (vec![255; 16 * 16 * 4], 16, 16)
        });
        // The reused weather fragment shader has no tint input, so keep one
        // forcefield image per official BorderStatus color.
        let mut images = [vk::Image::null(); 3];
        let mut views = [vk::ImageView::null(); 3];
        let mut image_allocs = Vec::with_capacity(3);
        for (i, status) in BorderStatus::ALL.into_iter().enumerate() {
            let color = status.color();
            let mut tinted = pixels.clone();
            for px in tinted.chunks_exact_mut(4) {
                for channel in 0..3 {
                    px[channel] = (u16::from(px[channel]) * u16::from(color[channel]) / 255) as u8;
                }
            }
            let (image, view, image_alloc) =
                util::create_gpu_image(device, allocator, w, h, "world_border_forcefield");
            let (staging, staging_alloc) =
                util::create_staging_buffer(device, allocator, &tinted, "world_border_staging");
            util::upload_image(device, queue, command_pool, staging, image, w, h);
            device.destroy_buffer(staging, None);
            allocator.lock().unwrap().free(staging_alloc).ok();
            images[i] = image;
            views[i] = view;
            image_allocs.push(Some(image_alloc));
        }
        let si = vk::SamplerCreateInfo {
            mag_filter: vk::Filter::Nearest,
            min_filter: vk::Filter::Nearest,
            address_mode_u: vk::SamplerAddressMode::Repeat,
            address_mode_v: vk::SamplerAddressMode::Repeat,
            address_mode_w: vk::SamplerAddressMode::Repeat,
            ..Default::default()
        };
        let sampler = device.create_sampler(&si, None).unwrap();
        for i in 0..3 {
            let ii = vk::DescriptorImageInfo {
                sampler,
                image_view: views[i],
                image_layout: vk::ImageLayout::ShaderReadOnlyOptimal,
            };
            device.update_descriptor_sets(
                &[vk::WriteDescriptorSet {
                    dst_set: texture_sets[i],
                    dst_binding: 0,
                    descriptor_type: vk::DescriptorType::CombinedImageSampler,
                    descriptor_count: 1,
                    image_info: &ii,
                    ..Default::default()
                }],
                &[],
            );
        }
        Self {
            pipeline,
            layout,
            pool,
            camera_layout,
            texture_layout,
            camera_sets,
            texture_sets,
            camera_buffers,
            camera_allocs,
            vertex_buffers,
            vertex_allocs,
            sampler,
            images,
            views,
            image_allocs,
        }
    }
    pub fn update_camera(&mut self, frame: usize, uniform: &CameraUniform) {
        let b = bytemuck::bytes_of(uniform);
        self.camera_allocs[frame]
            .as_mut()
            .unwrap()
            .mapped_slice_mut()
            .unwrap()[..b.len()]
            .copy_from_slice(b);
    }
    pub fn draw(
        &mut self,
        cmd: vk::CommandBuffer,
        frame: usize,
        draw: Option<BorderDraw>,
        camera: [f64; 3],
        anchor: [f64; 3],
        far: f32,
    ) {
        let Some(d) = draw else { return };
        let clip_x0 = d.bounds.min_x.max(camera[0] - f64::from(far));
        let clip_x1 = d.bounds.max_x.min(camera[0] + f64::from(far));
        let clip_z0 = d.bounds.min_z.max(camera[2] - f64::from(far));
        let clip_z1 = d.bounds.max_z.min(camera[2] + f64::from(far));
        let x0 = (clip_x0 - anchor[0]) as f32;
        let x1 = (clip_x1 - anchor[0]) as f32;
        let z0 = (clip_z0 - anchor[2]) as f32;
        let z1 = (clip_z1 - anchor[2]) as f32;
        let bx0 = (d.bounds.min_x - anchor[0]) as f32;
        let bx1 = (d.bounds.max_x - anchor[0]) as f32;
        let bz0 = (d.bounds.min_z - anchor[2]) as f32;
        let bz1 = (d.bounds.max_z - anchor[2]) as f32;
        let y0 = (camera[1] - f64::from(far) - anchor[1]) as f32;
        let y1 = (camera[1] + f64::from(far) - anchor[1]) as f32;
        let camx = (camera[0] - anchor[0]) as f32;
        let camz = (camera[2] - anchor[2]) as f32;
        let u = 0.5 * ((camera[0] as i64) & 1) as f32;
        let v = -(camera[1] as f32 * 0.5).fract() + d.offset;
        let sides = [
            (
                [x0, y0, bz1],
                [x1, y0, bz1],
                [x1, y1, bz1],
                [x0, y1, bz1],
                ((x1 - x0) * 0.5),
                bz1 - camz,
            ),
            (
                [bx0, y0, z0],
                [bx0, y0, z1],
                [bx0, y1, z1],
                [bx0, y1, z0],
                ((z1 - z0) * 0.5),
                bx0 - camx,
            ),
            (
                [x1, y0, bz0],
                [x0, y0, bz0],
                [x0, y1, bz0],
                [x1, y1, bz0],
                ((x1 - x0) * 0.5),
                bz0 - camz,
            ),
            (
                [bx1, y0, z1],
                [bx1, y0, z0],
                [bx1, y1, z0],
                [bx1, y1, z1],
                ((z1 - z0) * 0.5),
                bx1 - camx,
            ),
        ];
        let mut verts = Vec::with_capacity(24);
        let mut active = 0u32;
        for (side, (a, b, c, e, span, _dist)) in sides.into_iter().enumerate() {
            if !d.sides[side] {
                continue;
            }
            active |= 1 << side;
            let u0 = u;
            let u1 = u + span;
            let v0 = v;
            let v1 = v + 2.0 * far;
            for (p, t) in [
                (a, [u0, v1]),
                (b, [u1, v1]),
                (c, [u1, v0]),
                (a, [u0, v1]),
                (c, [u1, v0]),
                (e, [u0, v0]),
            ] {
                verts.push(Vertex {
                    position: p,
                    uv: t,
                    brightness: d.alpha,
                });
            }
        }
        if verts.is_empty() {
            return;
        }
        let bytes = bytemuck::cast_slice(&verts);
        self.vertex_allocs[frame]
            .as_mut()
            .unwrap()
            .mapped_slice_mut()
            .unwrap()[..bytes.len()]
            .copy_from_slice(bytes);
        let texture_index = match d.status {
            BorderStatus::Stationary => 0,
            BorderStatus::Growing => 1,
            BorderStatus::Shrinking => 2,
        };
        cmd.bind_pipeline(vk::PipelineBindPoint::Graphics, self.pipeline);
        cmd.bind_descriptor_sets(
            vk::PipelineBindPoint::Graphics,
            self.layout,
            0,
            &[self.camera_sets[frame], self.texture_sets[texture_index]],
            &[],
        );
        cmd.bind_vertex_buffers(0, &[self.vertex_buffers[frame]], &[0]);
        cmd.draw(verts.len() as u32, 1, 0, 0);
        let _ = active;
    }
    pub fn recreate(&mut self, device: &vk::Device, pass: vk::RenderPass) {
        device.destroy_pipeline(self.pipeline, None);
        self.pipeline = create_pipeline(device, pass, self.layout);
    }
    pub fn destroy(&mut self, device: &vk::Device, allocator: &Arc<Mutex<Allocator>>) {
        let mut a = allocator.lock().unwrap();
        for i in 0..MAX_FRAMES_IN_FLIGHT {
            device.destroy_buffer(self.camera_buffers[i], None);
            if let Some(x) = self.camera_allocs[i].take() {
                a.free(x).ok();
            }
            device.destroy_buffer(self.vertex_buffers[i], None);
            if let Some(x) = self.vertex_allocs[i].take() {
                a.free(x).ok();
            }
        }
        for i in 0..3 {
            device.destroy_image_view(self.views[i], None);
            device.destroy_image(self.images[i], None);
            if let Some(x) = self.image_allocs[i].take() {
                a.free(x).ok();
            }
        }
        drop(a);
        device.destroy_sampler(self.sampler, None);
        device.destroy_pipeline(self.pipeline, None);
        device.destroy_pipeline_layout(self.layout, None);
        device.destroy_descriptor_pool(self.pool, None);
        device.destroy_descriptor_set_layout(self.camera_layout, None);
        device.destroy_descriptor_set_layout(self.texture_layout, None);
    }
}

fn create_pipeline(
    device: &vk::Device,
    pass: vk::RenderPass,
    layout: vk::PipelineLayout,
) -> vk::Pipeline {
    let vs = shader::create_shader_module(device, shader::include_spirv!("weather.vert.spv"));
    let fs = shader::create_shader_module(device, shader::include_spirv!("weather.frag.spv"));
    let stages = [
        vk::PipelineShaderStageCreateInfo {
            stage: vk::ShaderStageFlags::Vertex,
            module: vs,
            name: c"main".as_ptr(),
            ..Default::default()
        },
        vk::PipelineShaderStageCreateInfo {
            stage: vk::ShaderStageFlags::Fragment,
            module: fs,
            name: c"main".as_ptr(),
            ..Default::default()
        },
    ];
    let bind = vk::VertexInputBindingDescription {
        binding: 0,
        stride: size_of::<Vertex>() as u32,
        input_rate: vk::VertexInputRate::Vertex,
    };
    let attrs = [
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
            format: vk::Format::R32Sfloat,
            offset: 20,
        },
    ];
    let vi = vk::PipelineVertexInputStateCreateInfo {
        vertex_binding_description_count: 1,
        vertex_binding_descriptions: &bind,
        vertex_attribute_description_count: 3,
        vertex_attribute_descriptions: attrs.as_ptr(),
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
    let rs = vk::PipelineRasterizationStateCreateInfo {
        polygon_mode: vk::PolygonMode::Fill,
        cull_mode: vk::CullModeFlags::None,
        front_face: vk::FrontFace::CounterClockwise,
        line_width: 1.0,
        ..Default::default()
    };
    let ms = vk::PipelineMultisampleStateCreateInfo {
        rasterization_samples: vk::SampleCountFlags::Type1,
        ..Default::default()
    };
    let ds = vk::PipelineDepthStencilStateCreateInfo {
        depth_test_enable: vk::TRUE,
        depth_write_enable: vk::FALSE,
        depth_compare_op: vk::CompareOp::LessOrEqual,
        ..Default::default()
    };
    let ba = vk::PipelineColorBlendAttachmentState {
        blend_enable: vk::TRUE,
        src_color_blend_factor: vk::BlendFactor::SrcAlpha,
        dst_color_blend_factor: vk::BlendFactor::OneMinusSrcAlpha,
        color_blend_op: vk::BlendOp::Add,
        src_alpha_blend_factor: vk::BlendFactor::One,
        dst_alpha_blend_factor: vk::BlendFactor::Zero,
        alpha_blend_op: vk::BlendOp::Add,
        color_write_mask: vk::ColorComponentFlags::RGBA,
    };
    let cb = vk::PipelineColorBlendStateCreateInfo {
        attachment_count: 1,
        attachments: &ba,
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
        rasterization_state: &rs,
        multisample_state: &ms,
        depth_stencil_state: &ds,
        color_blend_state: &cb,
        dynamic_state: &dy,
        layout,
        render_pass: pass,
        subpass: 0,
        ..Default::default()
    }];
    let mut p = vk::Pipeline::null();
    device
        .create_graphics_pipelines(
            vk::PipelineCache::null(),
            &info,
            None,
            std::slice::from_mut(&mut p),
        )
        .unwrap();
    device.destroy_shader_module(vs, None);
    device.destroy_shader_module(fs, None);
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extracts_border_status_for_color_variant_selection() {
        let mut border = WorldBorder::default();
        border.set_size(20.0);
        border.lerp_size_between(20.0, 30.0, 3);
        let draw = extract_border(&border, 0.0, [10.0, 0.0, 0.0], 32.0, 0.0).unwrap();
        assert_eq!(draw.status, BorderStatus::Growing);
    }

    #[test]
    fn culls_and_fades_by_nearest_edge() {
        let b = WorldBorder::default();
        assert!(extract_border(&b, 0.0, [0.0, 64.0, 0.0], 100.0, 0.0).is_none());
        let mut b = b;
        b.set_size(20.0);
        let d = extract_border(&b, 0.0, [10.0, 0.0, 0.0], 20.0, 0.0).unwrap();
        assert_eq!(d.alpha, 1.0);
        assert_eq!(d.bounds.min_x, -10.0);
        assert!(extract_border(&b, 0.0, [30.0, 0.0, 30.0], 10.0, 0.0).is_none());
        assert!(extract_border(&b, 0.0, [10.0, 0.0, 30.0], 10.0, 0.0).is_none());
    }
}
