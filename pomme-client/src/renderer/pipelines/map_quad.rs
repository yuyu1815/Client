use std::slice;
use std::sync::{Arc, Mutex};

use glam::{Mat4, Quat, Vec3};
use pomme_gpu_allocator::vulkan::{Allocation, Allocator};
use pyronyx::vk;

use crate::renderer::camera::CameraUniform;
use crate::renderer::{MAX_FRAMES_IN_FLIGHT, shader, util};

const MAPS_PER_DESCRIPTOR_POOL: u32 = 256;
const MAP_PLANE_OFFSET: f32 = 1.0 / 1024.0;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    uv: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ModelPush {
    model: [[f32; 4]; 4],
}

/// Draws a one-block-square map plane into the current world render pass.
/// Positions are relative to `CameraUniform`'s render anchor; `rotation` maps
/// the local XY plane (normal +Z) into the item-frame orientation.
pub struct MapQuadPipeline {
    pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    camera_sets: Vec<vk::DescriptorSet>,
    camera_buffers: Vec<vk::Buffer>,
    camera_allocations: Vec<Allocation>,
    camera_pool: vk::DescriptorPool,
    camera_layout: vk::DescriptorSetLayout,
    texture_layout: vk::DescriptorSetLayout,
    descriptor_pools: Vec<Vec<vk::DescriptorPool>>,
    maps_drawn: Vec<usize>,
    vertex_buffer: vk::Buffer,
    vertex_allocation: Allocation,
}

impl MapQuadPipeline {
    pub fn new(
        device: &vk::Device,
        allocator: &Arc<Mutex<Allocator>>,
        render_pass: vk::RenderPass,
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
        let push_range = vk::PushConstantRange {
            stage_flags: vk::ShaderStageFlags::Vertex,
            offset: 0,
            size: size_of::<ModelPush>() as u32,
        };
        let layouts = [camera_layout, texture_layout];
        let layout_info = vk::PipelineLayoutCreateInfo {
            set_layout_count: layouts.len() as u32,
            set_layouts: layouts.as_ptr(),
            push_constant_range_count: 1,
            push_constant_ranges: &push_range,
            ..Default::default()
        };
        let pipeline_layout = device
            .create_pipeline_layout(&layout_info, None)
            .expect("failed to create map quad pipeline layout");

        let camera_layouts = vec![camera_layout; MAX_FRAMES_IN_FLIGHT];
        let camera_pool_sizes = [vk::DescriptorPoolSize {
            ty: vk::DescriptorType::UniformBuffer,
            descriptor_count: MAX_FRAMES_IN_FLIGHT as u32,
        }];
        let camera_pool_info = vk::DescriptorPoolCreateInfo {
            max_sets: MAX_FRAMES_IN_FLIGHT as u32,
            pool_size_count: camera_pool_sizes.len() as u32,
            pool_sizes: camera_pool_sizes.as_ptr(),
            ..Default::default()
        };
        let camera_pool = device
            .create_descriptor_pool(&camera_pool_info, None)
            .expect("failed to create map quad camera descriptor pool");
        let camera_alloc_info = vk::DescriptorSetAllocateInfo {
            descriptor_pool: camera_pool,
            descriptor_set_count: camera_layouts.len() as u32,
            set_layouts: camera_layouts.as_ptr(),
            ..Default::default()
        };
        let mut camera_sets = vec![vk::DescriptorSet::null(); camera_layouts.len()];
        device
            .allocate_descriptor_sets(&camera_alloc_info, &mut camera_sets)
            .expect("failed to allocate map quad camera sets");

        let mut camera_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        let mut camera_allocations = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        for &set in &camera_sets {
            let (buffer, allocation) = util::create_uniform_buffer(
                device,
                allocator,
                size_of::<CameraUniform>() as u64,
                "map_quad_camera",
            );
            let info = vk::DescriptorBufferInfo {
                buffer,
                offset: 0,
                range: size_of::<CameraUniform>() as u64,
            };
            let write = vk::WriteDescriptorSet {
                dst_set: set,
                dst_binding: 0,
                descriptor_type: vk::DescriptorType::UniformBuffer,
                descriptor_count: 1,
                buffer_info: &info,
                ..Default::default()
            };
            device.update_descriptor_sets(&[write], &[]);
            camera_buffers.push(buffer);
            camera_allocations.push(allocation);
        }

        // Separate per-frame pools let callers reset only after that frame's
        // fence, while keeping sampled views stable for all recorded draws.
        let descriptor_pools = (0..MAX_FRAMES_IN_FLIGHT)
            .map(|_| vec![create_texture_pool(device)])
            .collect();

        let vertices = [
            Vertex {
                position: [-0.5, -0.5, 0.0],
                uv: [0.0, 1.0],
            },
            Vertex {
                position: [0.5, -0.5, 0.0],
                uv: [1.0, 1.0],
            },
            Vertex {
                position: [0.5, 0.5, 0.0],
                uv: [1.0, 0.0],
            },
            Vertex {
                position: [-0.5, -0.5, 0.0],
                uv: [0.0, 1.0],
            },
            Vertex {
                position: [0.5, 0.5, 0.0],
                uv: [1.0, 0.0],
            },
            Vertex {
                position: [-0.5, 0.5, 0.0],
                uv: [0.0, 0.0],
            },
        ];
        let (vertex_buffer, vertex_allocation) = util::create_mapped_buffer(
            device,
            allocator,
            bytemuck::cast_slice(&vertices),
            vk::BufferUsageFlags::VertexBuffer,
            "map_quad_vertices",
        );
        let pipeline = create_pipeline(device, render_pass, pipeline_layout);

        Self {
            pipeline,
            pipeline_layout,
            camera_sets,
            camera_buffers,
            camera_allocations,
            camera_pool,
            camera_layout,
            texture_layout,
            descriptor_pools,
            maps_drawn: vec![0; MAX_FRAMES_IN_FLIGHT],
            vertex_buffer,
            vertex_allocation,
        }
    }

    /// Call after the selected frame's fence has completed and before recording
    /// its map draws. Recycles only that frame's sampled-texture descriptors.
    pub fn begin_frame(&mut self, device: &vk::Device, frame: usize) {
        for &pool in &self.descriptor_pools[frame] {
            device
                .reset_descriptor_pool(pool, vk::DescriptorPoolResetFlags::default())
                .expect("failed to reset map quad descriptor pool");
        }
        self.maps_drawn[frame] = 0;
    }

    pub fn update_camera(&mut self, frame: usize, camera: &CameraUniform) {
        let bytes = bytemuck::bytes_of(camera);
        self.camera_allocations[frame].mapped_slice_mut().unwrap()[..bytes.len()]
            .copy_from_slice(bytes);
    }

    pub fn recreate_pipeline(&mut self, device: &vk::Device, render_pass: vk::RenderPass) {
        device.destroy_pipeline(self.pipeline, None);
        self.pipeline = create_pipeline(device, render_pass, self.pipeline_layout);
    }

    pub fn destroy(&mut self, device: &vk::Device, allocator: &Arc<Mutex<Allocator>>) {
        device.destroy_pipeline(self.pipeline, None);
        device.destroy_pipeline_layout(self.pipeline_layout, None);
        device.destroy_descriptor_pool(self.camera_pool, None);
        for pools in self.descriptor_pools.drain(..) {
            for pool in pools {
                device.destroy_descriptor_pool(pool, None);
            }
        }
        device.destroy_descriptor_set_layout(self.texture_layout, None);
        device.destroy_descriptor_set_layout(self.camera_layout, None);
        for buffer in self.camera_buffers.drain(..) {
            device.destroy_buffer(buffer, None);
        }
        for allocation in self.camera_allocations.drain(..) {
            let _ = allocator.lock().unwrap().free(allocation);
        }
        device.destroy_buffer(self.vertex_buffer, None);
        let _ = allocator
            .lock()
            .unwrap()
            .free(std::mem::replace(&mut self.vertex_allocation, unsafe {
                std::mem::zeroed()
            }));
    }

    /// `position` is the center of the frame's map plane in anchor-relative
    /// world coordinates. A 1/1024-block camera-ward offset avoids coplanar
    /// fighting with the item-frame backing. Each draw consumes one descriptor
    /// from that frame's pools; additional pools are retained between frames.
    pub fn draw(
        &mut self,
        device: &vk::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        view: vk::ImageView,
        sampler: vk::Sampler,
        position: Vec3,
        rotation: Quat,
    ) {
        let pool_index = self.maps_drawn[frame] / MAPS_PER_DESCRIPTOR_POOL as usize;
        if pool_index == self.descriptor_pools[frame].len() {
            self.descriptor_pools[frame].push(create_texture_pool(device));
        }
        let texture_layouts = [self.texture_layout];
        let alloc_info = vk::DescriptorSetAllocateInfo {
            descriptor_pool: self.descriptor_pools[frame][pool_index],
            descriptor_set_count: 1,
            set_layouts: texture_layouts.as_ptr(),
            ..Default::default()
        };
        let mut texture_set = vk::DescriptorSet::null();
        device
            .allocate_descriptor_sets(&alloc_info, slice::from_mut(&mut texture_set))
            .expect("failed to allocate map quad texture descriptor");
        self.maps_drawn[frame] += 1;
        let image_info = vk::DescriptorImageInfo {
            sampler,
            image_view: view,
            image_layout: vk::ImageLayout::ShaderReadOnlyOptimal,
        };
        let write = vk::WriteDescriptorSet {
            dst_set: texture_set,
            dst_binding: 0,
            descriptor_type: vk::DescriptorType::CombinedImageSampler,
            descriptor_count: 1,
            image_info: &image_info,
            ..Default::default()
        };
        device.update_descriptor_sets(&[write], &[]);

        let model = Mat4::from_translation(position)
            * Mat4::from_quat(rotation)
            * Mat4::from_translation(Vec3::Z * MAP_PLANE_OFFSET);
        let push = ModelPush {
            model: model.to_cols_array_2d(),
        };
        cmd.bind_pipeline(vk::PipelineBindPoint::Graphics, self.pipeline);
        cmd.push_constants(
            self.pipeline_layout,
            vk::ShaderStageFlags::Vertex,
            0,
            bytemuck::bytes_of(&push),
        );
        cmd.bind_descriptor_sets(
            vk::PipelineBindPoint::Graphics,
            self.pipeline_layout,
            0,
            &[self.camera_sets[frame], texture_set],
            &[],
        );
        cmd.bind_vertex_buffers(0, &[self.vertex_buffer], &[0]);
        cmd.draw(6, 1, 0, 0);
    }
}

fn create_texture_pool(device: &vk::Device) -> vk::DescriptorPool {
    let pool_sizes = [vk::DescriptorPoolSize {
        ty: vk::DescriptorType::CombinedImageSampler,
        descriptor_count: MAPS_PER_DESCRIPTOR_POOL,
    }];
    let pool_info = vk::DescriptorPoolCreateInfo {
        max_sets: MAPS_PER_DESCRIPTOR_POOL,
        pool_size_count: pool_sizes.len() as u32,
        pool_sizes: pool_sizes.as_ptr(),
        ..Default::default()
    };
    device
        .create_descriptor_pool(&pool_info, None)
        .expect("failed to create map quad texture descriptor pool")
}

fn create_pipeline(
    device: &vk::Device,
    render_pass: vk::RenderPass,
    layout: vk::PipelineLayout,
) -> vk::Pipeline {
    let vert_module =
        shader::create_shader_module(device, shader::include_spirv!("map_quad.vert.spv"));
    let frag_module =
        shader::create_shader_module(device, shader::include_spirv!("map_quad.frag.spv"));
    let stages = [
        vk::PipelineShaderStageCreateInfo {
            stage: vk::ShaderStageFlags::Vertex,
            module: vert_module,
            name: c"main".as_ptr(),
            ..Default::default()
        },
        vk::PipelineShaderStageCreateInfo {
            stage: vk::ShaderStageFlags::Fragment,
            module: frag_module,
            name: c"main".as_ptr(),
            ..Default::default()
        },
    ];
    let binding = vk::VertexInputBindingDescription {
        binding: 0,
        stride: size_of::<Vertex>() as u32,
        input_rate: vk::VertexInputRate::Vertex,
    };
    let attributes = [
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
    ];
    let vertex_input = vk::PipelineVertexInputStateCreateInfo {
        vertex_binding_description_count: 1,
        vertex_binding_descriptions: &binding,
        vertex_attribute_description_count: attributes.len() as u32,
        vertex_attribute_descriptions: attributes.as_ptr(),
        ..Default::default()
    };
    let assembly = vk::PipelineInputAssemblyStateCreateInfo {
        topology: vk::PrimitiveTopology::TriangleList,
        ..Default::default()
    };
    let viewport = vk::PipelineViewportStateCreateInfo {
        viewport_count: 1,
        scissor_count: 1,
        ..Default::default()
    };
    let raster = vk::PipelineRasterizationStateCreateInfo {
        polygon_mode: vk::PolygonMode::Fill,
        cull_mode: vk::CullModeFlags::None,
        front_face: vk::FrontFace::CounterClockwise,
        line_width: 1.0,
        depth_bias_enable: vk::TRUE,
        depth_bias_constant_factor: -1.0,
        depth_bias_slope_factor: -1.0,
        ..Default::default()
    };
    let multisample = vk::PipelineMultisampleStateCreateInfo {
        rasterization_samples: vk::SampleCountFlags::Type1,
        ..Default::default()
    };
    let depth = vk::PipelineDepthStencilStateCreateInfo {
        depth_test_enable: vk::TRUE,
        depth_write_enable: vk::FALSE,
        depth_compare_op: vk::CompareOp::LessOrEqual,
        ..Default::default()
    };
    let blend_attachment = vk::PipelineColorBlendAttachmentState {
        blend_enable: vk::TRUE,
        src_color_blend_factor: vk::BlendFactor::SrcAlpha,
        dst_color_blend_factor: vk::BlendFactor::OneMinusSrcAlpha,
        color_blend_op: vk::BlendOp::Add,
        src_alpha_blend_factor: vk::BlendFactor::One,
        dst_alpha_blend_factor: vk::BlendFactor::OneMinusSrcAlpha,
        alpha_blend_op: vk::BlendOp::Add,
        color_write_mask: vk::ColorComponentFlags::RGBA,
    };
    let blend = vk::PipelineColorBlendStateCreateInfo {
        attachment_count: 1,
        attachments: &blend_attachment,
        ..Default::default()
    };
    let dynamics = [vk::DynamicState::Viewport, vk::DynamicState::Scissor];
    let dynamic = vk::PipelineDynamicStateCreateInfo {
        dynamic_state_count: dynamics.len() as u32,
        dynamic_states: dynamics.as_ptr(),
        ..Default::default()
    };
    let info = [vk::GraphicsPipelineCreateInfo {
        stage_count: stages.len() as u32,
        stages: stages.as_ptr(),
        vertex_input_state: &vertex_input,
        input_assembly_state: &assembly,
        viewport_state: &viewport,
        rasterization_state: &raster,
        multisample_state: &multisample,
        depth_stencil_state: &depth,
        color_blend_state: &blend,
        dynamic_state: &dynamic,
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
        .expect("failed to create map quad graphics pipeline");
    device.destroy_shader_module(vert_module, None);
    device.destroy_shader_module(frag_module, None);
    pipeline
}
