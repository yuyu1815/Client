use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use pomme_gpu_allocator::vulkan::{Allocation, Allocator};
use pyronyx::vk;

use crate::renderer::util;
use crate::world::maps::{self, MapData};

pub const MAP_TEXTURE_SIZE: u32 = 128;

/// GPU resources for one map. Bind `view` with `sampler` as a combined image
/// sampler.
pub struct MapTexture {
    pub image: vk::Image,
    pub view: vk::ImageView,
    pub sampler: vk::Sampler,
    allocation: Allocation,
}

/// Owns nearest-filtered RGBA textures keyed by the protocol map ID.
pub struct MapTextureStore {
    textures: HashMap<u32, MapTexture>,
    sampler: vk::Sampler,
}

impl MapTextureStore {
    pub fn new(device: &vk::Device) -> Self {
        Self {
            textures: HashMap::new(),
            sampler: unsafe { util::create_nearest_sampler(device) },
        }
    }

    /// Convert a MapStore entry to RGBA and create or replace its 128x128 GPU
    /// image.
    pub fn update(
        &mut self,
        id: u32,
        map: &MapData,
        device: &vk::Device,
        allocator: &Arc<Mutex<Allocator>>,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
    ) -> &MapTexture {
        let pixels = map_rgba(map);
        let texture = if let Some(texture) = self.textures.get_mut(&id) {
            let (staging, staging_allocation) =
                util::create_staging_buffer(device, allocator, &pixels, "map_texture_staging");
            let region = vk::BufferImageCopy {
                image_subresource: vk::ImageSubresourceLayers {
                    aspect_mask: vk::ImageAspectFlags::Color,
                    mip_level: 0,
                    base_array_layer: 0,
                    layer_count: 1,
                },
                image_extent: vk::Extent3D {
                    width: MAP_TEXTURE_SIZE,
                    height: MAP_TEXTURE_SIZE,
                    depth: 1,
                },
                ..Default::default()
            };
            util::submit_one_time(device, queue, command_pool, |cmd| {
                util::record_image_regions(cmd, staging, texture.image, 1, &[region]);
            });
            device.destroy_buffer(staging, None);
            let _ = allocator.lock().unwrap().free(staging_allocation);
            texture
        } else {
            let (image, view, allocation) = util::create_gpu_image(
                device,
                allocator,
                MAP_TEXTURE_SIZE,
                MAP_TEXTURE_SIZE,
                "map_texture",
            );
            let (staging, staging_allocation) =
                util::create_staging_buffer(device, allocator, &pixels, "map_texture_staging");
            util::upload_image(
                device,
                queue,
                command_pool,
                staging,
                image,
                MAP_TEXTURE_SIZE,
                MAP_TEXTURE_SIZE,
            );
            device.destroy_buffer(staging, None);
            let _ = allocator.lock().unwrap().free(staging_allocation);
            self.textures.entry(id).or_insert(MapTexture {
                image,
                view,
                sampler: self.sampler,
                allocation,
            })
        };
        texture
    }

    pub fn get(&self, id: u32) -> Option<&MapTexture> {
        self.textures.get(&id)
    }

    /// Release all map GPU resources. Call before destroying the Vulkan device.
    pub fn destroy(&mut self, device: &vk::Device, allocator: &Arc<Mutex<Allocator>>) {
        for (_, texture) in self.textures.drain() {
            device.destroy_image_view(texture.view, None);
            device.destroy_image(texture.image, None);
            let _ = allocator.lock().unwrap().free(texture.allocation);
        }
        device.destroy_sampler(self.sampler, None);
    }
}

/// Convert map palette indices to tightly packed RGBA8 pixels. Index 0 is the
/// transparent map color; Minecraft's 64 palette colors each have four shades.
pub fn map_rgba(map: &MapData) -> Vec<u8> {
    let mut rgba = vec![0; (MAP_TEXTURE_SIZE * MAP_TEXTURE_SIZE * 4) as usize];
    for (pixel, &index) in rgba.chunks_exact_mut(4).zip(&map.colors) {
        if index != 0 {
            let color = maps::palette(index);
            pixel.copy_from_slice(&color.map(|channel| (channel * 255.0).round() as u8));
        }
    }
    rgba
}
