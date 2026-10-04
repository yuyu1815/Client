use std::sync::{Arc, Mutex};

use pomme_gpu_allocator::MemoryLocation;
use pomme_gpu_allocator::vulkan::{Allocation, AllocationCreateDesc, AllocationScheme, Allocator};
use pyronyx::khr::surface::SurfacePhysicalDevice;
use pyronyx::khr::swapchain::SwapchainDevice;
use pyronyx::vk;

use super::context::ContextError;
use super::util;
use crate::renderer::context::VulkanContext;

#[allow(dead_code)]
pub struct Swapchain {
    pub handle: vk::SwapchainKHR,
    pub images: Vec<vk::Image>,
    pub image_views: Vec<vk::ImageView>,
    pub format: vk::SurfaceFormatKHR,
    pub extent: vk::Extent2D,
    pub depth_image: vk::Image,
    pub depth_view: vk::ImageView,
    pub depth_allocation: Option<Allocation>,
    pub render_pass: vk::RenderPass,
    pub render_pass_scene: vk::RenderPass,
    pub render_pass_load: vk::RenderPass,
    pub framebuffers: Vec<vk::Framebuffer>,
    pub framebuffers_scene: Vec<vk::Framebuffer>,
    pub framebuffers_load: Vec<vk::Framebuffer>,
    pub retired: bool,
}

struct SwapchainBuild<'a> {
    ctx: &'a VulkanContext,
    handle: vk::SwapchainKHR,
    image_views: Vec<vk::ImageView>,
    depth_image: vk::Image,
    depth_view: vk::ImageView,
    depth_allocation: Option<Allocation>,
    render_passes: Vec<vk::RenderPass>,
    framebuffers: Vec<vk::Framebuffer>,
    armed: bool,
}

impl<'a> SwapchainBuild<'a> {
    fn new(ctx: &'a VulkanContext, handle: vk::SwapchainKHR) -> Self {
        Self {
            ctx,
            handle,
            image_views: Vec::new(),
            depth_image: vk::Image::null(),
            depth_view: vk::ImageView::null(),
            depth_allocation: None,
            render_passes: Vec::new(),
            framebuffers: Vec::new(),
            armed: true,
        }
    }
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for SwapchainBuild<'_> {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let d = &self.ctx.device;
        for &fb in &self.framebuffers {
            d.destroy_framebuffer(fb, None);
        }
        for &rp in &self.render_passes {
            d.destroy_render_pass(rp, None);
        }
        if self.depth_view != vk::ImageView::null() {
            d.destroy_image_view(self.depth_view, None);
        }
        if self.depth_image != vk::Image::null() {
            d.destroy_image(self.depth_image, None);
        }
        if let Some(a) = self.depth_allocation.take() {
            self.ctx.allocator.lock().unwrap().free(a).ok();
        }
        for &view in &self.image_views {
            d.destroy_image_view(view, None);
        }
        d.destroy_swapchain(self.handle, None);
    }
}

impl Swapchain {
    pub fn new(
        ctx: &VulkanContext,
        width: u32,
        height: u32,
        vsync: bool,
        mut old_swapchain: Option<&mut Swapchain>,
    ) -> Result<Self, ContextError> {
        let caps = ctx.physical_device.get_surface_capabilities(ctx.surface)?;
        let formats = ctx.physical_device.get_surface_formats(ctx.surface)?;
        let present_modes = ctx.physical_device.get_surface_present_modes(ctx.surface)?;

        let format = formats
            .iter()
            .find(|f| {
                f.format == vk::Format::B8G8R8A8Srgb
                    && f.color_space == vk::ColorSpaceKHR::SrgbNonlinear
            })
            .copied()
            .unwrap_or(formats[0]);

        let present_mode = if vsync {
            vk::PresentModeKHR::Fifo
        } else {
            // MoltenVK's Mailbox stays synced to the display refresh, so on macOS
            // only Immediate (displaySyncEnabled = NO) truly uncaps; elsewhere
            // Mailbox is preferred (uncapped and tear-free).
            let prefer = if cfg!(target_os = "macos") {
                [vk::PresentModeKHR::Immediate, vk::PresentModeKHR::Mailbox]
            } else {
                [vk::PresentModeKHR::Mailbox, vk::PresentModeKHR::Immediate]
            };
            prefer
                .into_iter()
                .find(|m| present_modes.contains(m))
                .unwrap_or(vk::PresentModeKHR::Fifo)
        };

        // Prefer the surface's reported drawable size: when `current_extent` is
        // defined (not u32::MAX, as on macOS/MoltenVK), the Vulkan spec requires
        // using it. Falling back to the window size here is what letterboxed the
        // image in native fullscreen.
        let extent = if caps.current_extent.width != u32::MAX {
            caps.current_extent
        } else {
            vk::Extent2D {
                width: width.clamp(caps.min_image_extent.width, caps.max_image_extent.width),
                height: height.clamp(caps.min_image_extent.height, caps.max_image_extent.height),
            }
        };
        // Guard against a degenerate (0,0) extent (e.g. a minimized window
        // reporting current_extent = 0) so the aspect ratio stays finite.
        let extent = vk::Extent2D {
            width: extent.width.max(1),
            height: extent.height.max(1),
        };

        let image_count = (caps.min_image_count + 1).min(if caps.max_image_count == 0 {
            u32::MAX
        } else {
            caps.max_image_count
        });

        let (sharing_mode, queue_families): (vk::SharingMode, Vec<u32>) =
            if ctx.graphics_family != ctx.present_family {
                (
                    vk::SharingMode::Concurrent,
                    vec![ctx.graphics_family, ctx.present_family],
                )
            } else {
                (vk::SharingMode::Exclusive, vec![])
            };

        // All fallible surface queries and create-info construction happen before
        // retirement. Vulkan retires oldSwapchain as soon as native creation is
        // reached, even when vkCreateSwapchainKHR itself fails.
        let old_handle = old_swapchain
            .as_deref()
            .filter(|old| !old.retired)
            .map_or(vk::SwapchainKHR::null(), |old| old.handle);
        let swapchain_info = vk::SwapchainCreateInfoKHR {
            surface: ctx.surface,
            min_image_count: image_count,
            image_format: format.format,
            image_color_space: format.color_space,
            image_extent: extent,
            image_array_layers: 1,
            image_usage: vk::ImageUsageFlags::ColorAttachment | vk::ImageUsageFlags::TransferSrc,
            image_sharing_mode: sharing_mode,
            queue_family_index_count: queue_families.len() as u32,
            queue_family_indices: queue_families.as_ptr(),
            pre_transform: caps.current_transform,
            composite_alpha: vk::CompositeAlphaFlagsKHR::Opaque,
            present_mode,
            clipped: vk::TRUE,
            old_swapchain: old_handle,
            ..Default::default()
        };

        if let Some(old) = old_swapchain.as_deref_mut() {
            if old_handle != vk::SwapchainKHR::null() {
                old.retired = true;
            }
        }
        let handle = ctx.device.create_swapchain(&swapchain_info, None)?;
        let mut build = SwapchainBuild::new(ctx, handle);
        let images = ctx.device.get_swapchain_images(handle)?;

        for &img in &images {
            let view_info = vk::ImageViewCreateInfo {
                image: img,
                view_type: vk::ImageViewType::Type2D,
                format: format.format,
                subresource_range: util::COLOR_SUBRESOURCE_RANGE,
                ..Default::default()
            };
            build
                .image_views
                .push(ctx.device.create_image_view(&view_info, None)?);
        }

        let (depth_image, depth_view, depth_allocation) =
            create_depth_resources(&ctx.device, extent, &ctx.allocator)?;
        build.depth_image = depth_image;
        build.depth_view = depth_view;
        build.depth_allocation = Some(depth_allocation);

        let render_pass = create_render_pass(&ctx.device, format.format)?;
        build.render_passes.push(render_pass);
        let render_pass_scene = create_render_pass_scene(&ctx.device, format.format)?;
        build.render_passes.push(render_pass_scene);
        let render_pass_load = create_render_pass_load(&ctx.device, format.format)?;
        build.render_passes.push(render_pass_load);

        let mut make_fbs = |rp: vk::RenderPass| -> Result<Vec<vk::Framebuffer>, vk::Error> {
            let mut fbs = Vec::with_capacity(build.image_views.len());
            for &view in &build.image_views {
                let attachments = [view, build.depth_view];
                let fb_info = vk::FramebufferCreateInfo {
                    render_pass: rp,
                    attachment_count: attachments.len() as u32,
                    attachments: attachments.as_ptr(),
                    width: extent.width,
                    height: extent.height,
                    layers: 1,
                    ..Default::default()
                };
                let fb = ctx.device.create_framebuffer(&fb_info, None)?;
                build.framebuffers.push(fb);
                fbs.push(fb);
            }
            Ok(fbs)
        };

        let framebuffers = make_fbs(render_pass)?;
        let framebuffers_scene = make_fbs(render_pass_scene)?;
        let framebuffers_load = make_fbs(render_pass_load)?;

        let image_views = std::mem::take(&mut build.image_views);
        build.depth_image = vk::Image::null();
        build.depth_view = vk::ImageView::null();
        let depth_allocation = build.depth_allocation.take().unwrap();
        build.render_passes.clear();
        build.framebuffers.clear();
        build.disarm();
        Ok(Self {
            handle,
            images,
            image_views,
            format,
            extent,
            depth_image,
            depth_view,
            depth_allocation: Some(depth_allocation),
            render_pass,
            render_pass_scene,
            render_pass_load,
            framebuffers,
            framebuffers_scene,
            framebuffers_load,
            retired: false,
        })
    }

    pub fn destroy(&mut self, device: &vk::Device, allocator: &Arc<Mutex<Allocator>>) {
        let _ = device.wait_idle();

        for fbs in [
            &mut self.framebuffers,
            &mut self.framebuffers_scene,
            &mut self.framebuffers_load,
        ] {
            for &fb in fbs.iter() {
                device.destroy_framebuffer(fb, None);
            }
            fbs.clear();
        }

        for &rp in &[
            self.render_pass,
            self.render_pass_scene,
            self.render_pass_load,
        ] {
            device.destroy_render_pass(rp, None);
        }

        device.destroy_image_view(self.depth_view, None);
        device.destroy_image(self.depth_image, None);
        if let Some(alloc) = self.depth_allocation.take() {
            allocator.lock().unwrap().free(alloc).ok();
        }

        for &view in &self.image_views {
            device.destroy_image_view(view, None);
        }
        self.image_views.clear();

        device.destroy_swapchain(self.handle, None);
    }

    pub fn aspect_ratio(&self) -> f32 {
        self.extent.width as f32 / self.extent.height.max(1) as f32
    }
}

fn create_depth_resources(
    device: &vk::Device,
    extent: vk::Extent2D,
    allocator: &Arc<Mutex<Allocator>>,
) -> Result<(vk::Image, vk::ImageView, Allocation), ContextError> {
    let depth_format = vk::Format::D32Sfloat;

    let image_info = vk::ImageCreateInfo {
        image_type: vk::ImageType::Type2D,
        format: depth_format,
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

    let image = device.create_image(&image_info, None)?;
    let mem_reqs = device.get_image_memory_requirements(image);

    let allocation = match allocator.lock().unwrap().allocate(&AllocationCreateDesc {
        name: "depth_image",
        requirements: mem_reqs,
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
        device.destroy_image(image, None);
        allocator.lock().unwrap().free(allocation).ok();
        return Err(error.into());
    }

    let view_info = vk::ImageViewCreateInfo {
        image,
        view_type: vk::ImageViewType::Type2D,
        format: depth_format,
        subresource_range: util::DEPTH_SUBRESOURCE_RANGE,
        ..Default::default()
    };
    let view = match device.create_image_view(&view_info, None) {
        Ok(view) => view,
        Err(error) => {
            device.destroy_image(image, None);
            allocator.lock().unwrap().free(allocation).ok();
            return Err(error.into());
        }
    };

    Ok((image, view, allocation))
}

fn create_render_pass(
    device: &vk::Device,
    color_format: vk::Format,
) -> Result<vk::RenderPass, vk::Error> {
    let attachments = [
        vk::AttachmentDescription {
            format: color_format,
            samples: vk::SampleCountFlags::Type1,
            load_op: vk::AttachmentLoadOp::Clear,
            store_op: vk::AttachmentStoreOp::Store,
            stencil_load_op: vk::AttachmentLoadOp::DontCare,
            stencil_store_op: vk::AttachmentStoreOp::DontCare,
            initial_layout: vk::ImageLayout::Undefined,
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

    let color_ref = [vk::AttachmentReference {
        attachment: 0,
        layout: vk::ImageLayout::ColorAttachmentOptimal,
    }];
    let depth_ref = vk::AttachmentReference {
        attachment: 1,
        layout: vk::ImageLayout::DepthStencilAttachmentOptimal,
    };

    let subpass = [vk::SubpassDescription {
        pipeline_bind_point: vk::PipelineBindPoint::Graphics,
        color_attachment_count: color_ref.len() as u32,
        color_attachments: color_ref.as_ptr(),
        depth_stencil_attachment: &depth_ref,
        ..Default::default()
    }];

    let dependency = [vk::SubpassDependency {
        src_subpass: vk::SUBPASS_EXTERNAL,
        dst_subpass: 0,
        src_stage_mask: vk::PipelineStageFlags::ColorAttachmentOutput
            | vk::PipelineStageFlags::EarlyFragmentTests,
        src_access_mask: vk::AccessFlags::empty(),
        dst_stage_mask: vk::PipelineStageFlags::ColorAttachmentOutput
            | vk::PipelineStageFlags::EarlyFragmentTests,
        dst_access_mask: vk::AccessFlags::ColorAttachmentWrite
            | vk::AccessFlags::DepthStencilAttachmentWrite,
        ..Default::default()
    }];

    let render_pass_info = vk::RenderPassCreateInfo {
        attachment_count: attachments.len() as u32,
        attachments: attachments.as_ptr(),
        subpass_count: subpass.len() as u32,
        subpasses: subpass.as_ptr(),
        dependency_count: dependency.len() as u32,
        dependencies: dependency.as_ptr(),
        ..Default::default()
    };

    device.create_render_pass(&render_pass_info, None)
}

fn create_render_pass_scene(
    device: &vk::Device,
    color_format: vk::Format,
) -> Result<vk::RenderPass, vk::Error> {
    create_render_pass_variant(
        device,
        color_format,
        vk::AttachmentLoadOp::Clear,
        vk::ImageLayout::Undefined,
        vk::ImageLayout::ColorAttachmentOptimal,
    )
}

fn create_render_pass_load(
    device: &vk::Device,
    color_format: vk::Format,
) -> Result<vk::RenderPass, vk::Error> {
    create_render_pass_variant(
        device,
        color_format,
        vk::AttachmentLoadOp::Load,
        vk::ImageLayout::ColorAttachmentOptimal,
        vk::ImageLayout::PresentSrcKHR,
    )
}

fn create_render_pass_variant(
    device: &vk::Device,
    color_format: vk::Format,
    load_op: vk::AttachmentLoadOp,
    initial_layout: vk::ImageLayout,
    final_layout: vk::ImageLayout,
) -> Result<vk::RenderPass, vk::Error> {
    let attachments = [
        vk::AttachmentDescription {
            format: color_format,
            samples: vk::SampleCountFlags::Type1,
            load_op,
            store_op: vk::AttachmentStoreOp::Store,
            stencil_load_op: vk::AttachmentLoadOp::DontCare,
            stencil_store_op: vk::AttachmentStoreOp::DontCare,
            initial_layout,
            final_layout,
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

    let color_ref = [vk::AttachmentReference {
        attachment: 0,
        layout: vk::ImageLayout::ColorAttachmentOptimal,
    }];
    let depth_ref = vk::AttachmentReference {
        attachment: 1,
        layout: vk::ImageLayout::DepthStencilAttachmentOptimal,
    };

    let subpass = [vk::SubpassDescription {
        pipeline_bind_point: vk::PipelineBindPoint::Graphics,
        color_attachment_count: color_ref.len() as u32,
        color_attachments: color_ref.as_ptr(),
        depth_stencil_attachment: &depth_ref,
        ..Default::default()
    }];

    let dependency = [vk::SubpassDependency {
        src_subpass: vk::SUBPASS_EXTERNAL,
        dst_subpass: 0,
        src_stage_mask: vk::PipelineStageFlags::ColorAttachmentOutput
            | vk::PipelineStageFlags::EarlyFragmentTests,
        src_access_mask: vk::AccessFlags::empty(),
        dst_stage_mask: vk::PipelineStageFlags::ColorAttachmentOutput
            | vk::PipelineStageFlags::EarlyFragmentTests,
        dst_access_mask: vk::AccessFlags::ColorAttachmentWrite
            | vk::AccessFlags::DepthStencilAttachmentWrite,
        ..Default::default()
    }];

    let info = vk::RenderPassCreateInfo {
        attachment_count: attachments.len() as u32,
        attachments: attachments.as_ptr(),
        subpass_count: subpass.len() as u32,
        subpasses: subpass.as_ptr(),
        dependency_count: dependency.len() as u32,
        dependencies: dependency.as_ptr(),
        ..Default::default()
    };

    device.create_render_pass(&info, None)
}
