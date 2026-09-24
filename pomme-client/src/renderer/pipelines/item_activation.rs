use std::sync::{Arc, Mutex};

use pomme_gpu_allocator::MemoryLocation;
use pomme_gpu_allocator::vulkan::{Allocation, AllocationCreateDesc, AllocationScheme, Allocator};
use pyronyx::vk;

use crate::renderer::context::{ContextError, VulkanContext};
use crate::renderer::util;

struct DepthTarget {
    image: vk::Image,
    view: vk::ImageView,
    allocation: Allocation,
}

/// Owns the activation-compatible render pass and one private D32 target per
/// swapchain image. It never aliases the swapchain's world depth image.
pub struct ActivationTargets {
    pub render_pass: vk::RenderPass,
    pub framebuffers: Vec<vk::Framebuffer>,
    depth_targets: Vec<DepthTarget>,
    extent: vk::Extent2D,
}

impl ActivationTargets {
    pub fn new(
        ctx: &VulkanContext,
        swapchain: &crate::renderer::swapchain::Swapchain,
    ) -> Result<Self, ContextError> {
        let render_pass = create_render_pass(&ctx.device, swapchain.format.format)?;
        let mut this = Self {
            render_pass,
            framebuffers: Vec::new(),
            depth_targets: Vec::new(),
            extent: swapchain.extent,
        };
        let result = (|| {
            for _ in &swapchain.images {
                this.depth_targets.push(create_depth_target(
                    &ctx.device,
                    &ctx.allocator,
                    swapchain.extent,
                )?);
            }
            for (color, depth) in swapchain.image_views.iter().zip(&this.depth_targets) {
                let attachments = [*color, depth.view];
                let info = vk::FramebufferCreateInfo {
                    render_pass,
                    attachment_count: attachments.len() as u32,
                    attachments: attachments.as_ptr(),
                    width: swapchain.extent.width,
                    height: swapchain.extent.height,
                    layers: 1,
                    ..Default::default()
                };
                this.framebuffers
                    .push(ctx.device.create_framebuffer(&info, None)?);
            }
            Ok(())
        })();
        if let Err(error) = result {
            this.destroy(&ctx.device, &ctx.allocator);
            return Err(error);
        }
        Ok(this)
    }

    /// Record after the previous render pass has ended. `color_image` must be
    /// this frame's acquired swapchain image; caller ends this pass after UI.
    pub fn begin_activation(
        &self,
        ctx: &VulkanContext,
        cmd: vk::CommandBuffer,
        image_index: usize,
        color_image: vk::Image,
    ) {
        assert!(
            image_index < self.framebuffers.len(),
            "activation image index out of range"
        );
        let to_color = vk::ImageMemoryBarrier {
            src_access_mask: vk::AccessFlags::ColorAttachmentWrite,
            dst_access_mask: vk::AccessFlags::ColorAttachmentRead
                | vk::AccessFlags::ColorAttachmentWrite,
            old_layout: vk::ImageLayout::PresentSrcKHR,
            new_layout: vk::ImageLayout::ColorAttachmentOptimal,
            src_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
            dst_queue_family_index: vk::QUEUE_FAMILY_IGNORED,
            image: color_image,
            subresource_range: util::COLOR_SUBRESOURCE_RANGE,
            ..Default::default()
        };
        cmd.pipeline_barrier(
            vk::PipelineStageFlags::ColorAttachmentOutput,
            vk::PipelineStageFlags::ColorAttachmentOutput,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[to_color],
        );
        let clear_values = [
            vk::ClearValue {
                color: vk::ClearColorValue { float32: [0.0; 4] },
            },
            vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 1.0,
                    stencil: 0,
                },
            },
        ];
        let info = vk::RenderPassBeginInfo {
            render_pass: self.render_pass,
            framebuffer: self.framebuffers[image_index],
            render_area: vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: self.extent,
            },
            clear_value_count: clear_values.len() as u32,
            clear_values: clear_values.as_ptr(),
            ..Default::default()
        };
        let _ = ctx;
        cmd.begin_render_pass(&info, vk::SubpassContents::Inline);
        cmd.set_viewport(
            0,
            &[vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: self.extent.width as f32,
                height: self.extent.height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            }],
        );
        cmd.set_scissor(
            0,
            &[vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: self.extent,
            }],
        );
    }

    pub fn destroy(&mut self, device: &vk::Device, allocator: &Arc<Mutex<Allocator>>) {
        for fb in self.framebuffers.drain(..) {
            device.destroy_framebuffer(fb, None);
        }
        for target in self.depth_targets.drain(..) {
            device.destroy_image_view(target.view, None);
            device.destroy_image(target.image, None);
            allocator.lock().unwrap().free(target.allocation).ok();
        }
        if self.render_pass != vk::RenderPass::null() {
            device.destroy_render_pass(self.render_pass, None);
            self.render_pass = vk::RenderPass::null();
        }
    }
}

fn create_depth_target(
    device: &vk::Device,
    allocator: &Arc<Mutex<Allocator>>,
    extent: vk::Extent2D,
) -> Result<DepthTarget, ContextError> {
    let info = vk::ImageCreateInfo {
        image_type: vk::ImageType::Type2D,
        format: vk::Format::D32Sfloat,
        extent: vk::Extent3D {
            width: extent.width,
            height: extent.height,
            depth: 1,
        },
        mip_levels: 1,
        array_layers: 1,
        samples: vk::SampleCountFlags::Type1,
        tiling: vk::ImageTiling::Optimal,
        usage: vk::ImageUsageFlags::DepthStencilAttachment,
        ..Default::default()
    };
    let image = device.create_image(&info, None)?;
    let requirements = device.get_image_memory_requirements(image);
    let allocation = match allocator.lock().unwrap().allocate(&AllocationCreateDesc {
        name: "totem_activation_depth",
        requirements,
        location: MemoryLocation::GpuOnly,
        linear: false,
        allocation_scheme: AllocationScheme::GpuAllocatorManaged,
    }) {
        Ok(allocation) => allocation,
        Err(error) => {
            device.destroy_image(image, None);
            return Err(error.into());
        }
    };
    if let Err(error) =
        unsafe { device.bind_image_memory(image, allocation.memory(), allocation.offset()) }
    {
        allocator.lock().unwrap().free(allocation).ok();
        device.destroy_image(image, None);
        return Err(error.into());
    }
    let view_info = vk::ImageViewCreateInfo {
        image,
        view_type: vk::ImageViewType::Type2D,
        format: vk::Format::D32Sfloat,
        subresource_range: util::DEPTH_SUBRESOURCE_RANGE,
        ..Default::default()
    };
    match device.create_image_view(&view_info, None) {
        Ok(view) => Ok(DepthTarget {
            image,
            view,
            allocation,
        }),
        Err(error) => {
            device.destroy_image(image, None);
            allocator.lock().unwrap().free(allocation).ok();
            Err(error.into())
        }
    }
}

fn create_render_pass(
    device: &vk::Device,
    color_format: vk::Format,
) -> Result<vk::RenderPass, vk::Error> {
    let attachments = [
        vk::AttachmentDescription {
            format: color_format,
            samples: vk::SampleCountFlags::Type1,
            load_op: vk::AttachmentLoadOp::Load,
            store_op: vk::AttachmentStoreOp::Store,
            stencil_load_op: vk::AttachmentLoadOp::DontCare,
            stencil_store_op: vk::AttachmentStoreOp::DontCare,
            initial_layout: vk::ImageLayout::ColorAttachmentOptimal,
            final_layout: vk::ImageLayout::PresentSrcKHR,
            ..Default::default()
        },
        vk::AttachmentDescription {
            format: vk::Format::D32Sfloat,
            samples: vk::SampleCountFlags::Type1,
            load_op: vk::AttachmentLoadOp::Clear,
            store_op: vk::AttachmentStoreOp::DontCare,
            stencil_load_op: vk::AttachmentLoadOp::DontCare,
            stencil_store_op: vk::AttachmentStoreOp::DontCare,
            initial_layout: vk::ImageLayout::Undefined,
            final_layout: vk::ImageLayout::DepthStencilAttachmentOptimal,
            ..Default::default()
        },
    ];
    let color = [vk::AttachmentReference {
        attachment: 0,
        layout: vk::ImageLayout::ColorAttachmentOptimal,
    }];
    let depth = vk::AttachmentReference {
        attachment: 1,
        layout: vk::ImageLayout::DepthStencilAttachmentOptimal,
    };
    let subpass = [vk::SubpassDescription {
        pipeline_bind_point: vk::PipelineBindPoint::Graphics,
        color_attachment_count: 1,
        color_attachments: color.as_ptr(),
        depth_stencil_attachment: &depth,
        ..Default::default()
    }];
    let deps = external_dependencies();
    let info = vk::RenderPassCreateInfo {
        attachment_count: attachments.len() as u32,
        attachments: attachments.as_ptr(),
        subpass_count: 1,
        subpasses: subpass.as_ptr(),
        dependency_count: deps.len() as u32,
        dependencies: deps.as_ptr(),
        ..Default::default()
    };
    device.create_render_pass(&info, None)
}

fn external_dependencies() -> [vk::SubpassDependency; 2] {
    [
        vk::SubpassDependency {
            src_subpass: vk::SUBPASS_EXTERNAL,
            dst_subpass: 0,
            src_stage_mask: vk::PipelineStageFlags::ColorAttachmentOutput
                | vk::PipelineStageFlags::EarlyFragmentTests
                | vk::PipelineStageFlags::LateFragmentTests,
            src_access_mask: vk::AccessFlags::ColorAttachmentWrite
                | vk::AccessFlags::DepthStencilAttachmentWrite,
            dst_stage_mask: vk::PipelineStageFlags::ColorAttachmentOutput
                | vk::PipelineStageFlags::EarlyFragmentTests,
            dst_access_mask: vk::AccessFlags::ColorAttachmentRead
                | vk::AccessFlags::ColorAttachmentWrite
                | vk::AccessFlags::DepthStencilAttachmentWrite,
            dependency_flags: vk::DependencyFlags::ByRegion,
        },
        vk::SubpassDependency {
            src_subpass: 0,
            dst_subpass: vk::SUBPASS_EXTERNAL,
            src_stage_mask: vk::PipelineStageFlags::ColorAttachmentOutput
                | vk::PipelineStageFlags::EarlyFragmentTests
                | vk::PipelineStageFlags::LateFragmentTests,
            src_access_mask: vk::AccessFlags::ColorAttachmentWrite
                | vk::AccessFlags::DepthStencilAttachmentWrite,
            dst_stage_mask: vk::PipelineStageFlags::ColorAttachmentOutput
                | vk::PipelineStageFlags::BottomOfPipe,
            dst_access_mask: vk::AccessFlags::ColorAttachmentRead | vk::AccessFlags::MemoryRead,
            dependency_flags: vk::DependencyFlags::ByRegion,
        },
    ]
}

#[cfg(test)]
mod tests {
    use pyronyx::vk;

    use super::external_dependencies;

    #[test]
    fn external_dependencies_cover_early_and_late_depth_writes() {
        let [incoming, outgoing] = external_dependencies();
        let depth_stages =
            vk::PipelineStageFlags::EarlyFragmentTests | vk::PipelineStageFlags::LateFragmentTests;
        let depth_write = vk::AccessFlags::DepthStencilAttachmentWrite;
        assert_eq!(incoming.src_stage_mask & depth_stages, depth_stages);
        assert_eq!(outgoing.src_stage_mask & depth_stages, depth_stages);
        assert!(incoming.src_access_mask.contains(depth_write));
        assert!(outgoing.src_access_mask.contains(depth_write));
        assert!(
            incoming
                .src_stage_mask
                .contains(vk::PipelineStageFlags::ColorAttachmentOutput)
        );
        assert!(
            incoming
                .src_access_mask
                .contains(vk::AccessFlags::ColorAttachmentWrite)
        );
        assert!(
            outgoing
                .src_stage_mask
                .contains(vk::PipelineStageFlags::ColorAttachmentOutput)
        );
        assert!(
            outgoing
                .src_access_mask
                .contains(vk::AccessFlags::ColorAttachmentWrite)
        );
    }
}
