use std::collections::HashMap;
use std::path::Path;
use std::slice;
use std::sync::{Arc, Mutex};

use azalea_core::position::BlockPos;
use azalea_registry::builtin::BlockEntityKind;
use pomme_gpu_allocator::vulkan::{Allocation, Allocator};
use pyronyx::vk;

use crate::assets::{AssetIndex, resolve_asset_path};
use crate::renderer::camera::CameraUniform;
use crate::renderer::chunk::mesher::ChunkVertex;
use crate::renderer::entity_model::{BakedEntityModel, ModelConvention, PartAnim};
use crate::renderer::pipelines::entity_renderer::{
    BlendMode, ModelInput, WHITE_TINT, create_pipeline, fallback_texture,
};
use crate::renderer::{MAX_FRAMES_IN_FLIGHT, block_entity_model, shader, util};
use crate::ui::font::{GLYPH_ATLAS_SIZE, GlyphMap};

const MAX_SIGN_VERTICES: usize = 65536;
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SignVertex {
    position: [f32; 3],
    uv_layer: [f32; 3],
    color: [f32; 4],
    colored: f32,
}

pub struct BlockEntityRenderInfo {
    pub pos: BlockPos,
    pub kind: BlockEntityKind,
    /// Copper golem statue body-layer index (standing, running, sitting, star).
    pub statue_pose: Option<u8>,
    pub yaw: f32,
    /// Texture-variant index; the model index is `variant % models.len()`, so
    /// chest variants (material-major, [single, left, right] per material)
    /// fold to their type's model and single-model kinds always use model 0.
    pub variant: u32,
    /// Lid openness for chest/shulker, 0.0=closed to 1.0=open. Raw (un-eased);
    /// the pipeline applies a cubic ease at draw time.
    pub lid_open: f32,
    /// Plain-text sign faces extracted from the block entity's render messages.
    pub sign_front: Option<[String; 4]>,
    pub sign_back: Option<[String; 4]>,
    pub sign_front_color: [f32; 3],
    pub sign_front_glowing: bool,
    pub sign_back_color: [f32; 3],
    pub sign_back_glowing: bool,
    pub sign_wall: bool,
    /// Approximate local lightmap brightness; glowing text bypasses it.
    pub sign_light: f32,
}

struct TextureSlot {
    image: vk::Image,
    view: vk::ImageView,
    allocation: Allocation,
    set: vk::DescriptorSet,
}

struct KindEntry {
    /// Model variants sharing one vertex buffer; their `part_ranges` are
    /// rebased to be buffer-absolute at build time.
    models: Vec<BakedEntityModel>,
    vertex_buffer: vk::Buffer,
    vertex_allocation: Allocation,
    textures: Vec<TextureSlot>,
}

struct KindDef {
    kind: BlockEntityKind,
    models: Vec<BakedEntityModel>,
    tex_variants: &'static [&'static [&'static str]],
    tex_size: u32,
}

/// 16 dye colors in vanilla `DyeColor` ordinal order. Used both to build
/// texture-variant arrays and to map block names back to variant indices.
const DYE_COLOR_NAMES: [&str; 16] = [
    "white",
    "orange",
    "magenta",
    "light_blue",
    "yellow",
    "lime",
    "pink",
    "gray",
    "light_gray",
    "cyan",
    "purple",
    "blue",
    "brown",
    "green",
    "red",
    "black",
];

/// Sign wood order used by the block-entity variant mapping.
const SIGN_WOOD_NAMES: [&str; 12] = [
    "oak", "spruce", "birch", "jungle", "acacia", "dark_oak", "mangrove", "cherry", "pale_oak",
    "bamboo", "crimson", "warped",
];

/// Chest textures mirroring vanilla `Sheets.chooseSprite`: material-major with
/// [single, double-left, double-right] per material, so
/// `variant = material * 3 + type` and `variant % 3` is the model index.
/// Material order matches [`variant_for_block`]. Christmas only reskins the
/// normal material (vanilla `getChestMaterial` checks copper first), so copper
/// stages keep their look.
macro_rules! chest_textures {
    ($base:literal, copper) => {
        chest_textures!($base, "copper", "copper_exposed", "copper_weathered", "copper_oxidized")
    };
    ($($mat:literal),+ $(,)?) => {
        &[$(
            &[concat!("minecraft/textures/entity/chest/", $mat, ".png")],
            &[concat!("minecraft/textures/entity/chest/", $mat, "_left.png")],
            &[concat!("minecraft/textures/entity/chest/", $mat, "_right.png")],
        )+]
    };
}

const CHEST_TEXTURES: &[&[&str]] = chest_textures!("normal", copper);

const CHEST_XMAS_TEXTURES: &[&[&str]] = chest_textures!("christmas", copper);

const TRAPPED_CHEST_TEXTURES: &[&[&str]] = chest_textures!("trapped");

const ENDER_CHEST_TEXTURES: &[&[&str]] = &[&["minecraft/textures/entity/chest/ender.png"]];

const COPPER_GOLEM_STATUE_TEXTURES: &[&[&str]] = &[
    &["minecraft/textures/entity/copper_golem/copper_golem.png"],
    &["minecraft/textures/entity/copper_golem/copper_golem_exposed.png"],
    &["minecraft/textures/entity/copper_golem/copper_golem_weathered.png"],
    &["minecraft/textures/entity/copper_golem/copper_golem_oxidized.png"],
];

const SHULKER_TEXTURES: &[&[&str]] = &[
    &["minecraft/textures/entity/shulker/shulker_white.png"],
    &["minecraft/textures/entity/shulker/shulker_orange.png"],
    &["minecraft/textures/entity/shulker/shulker_magenta.png"],
    &["minecraft/textures/entity/shulker/shulker_light_blue.png"],
    &["minecraft/textures/entity/shulker/shulker_yellow.png"],
    &["minecraft/textures/entity/shulker/shulker_lime.png"],
    &["minecraft/textures/entity/shulker/shulker_pink.png"],
    &["minecraft/textures/entity/shulker/shulker_gray.png"],
    &["minecraft/textures/entity/shulker/shulker_light_gray.png"],
    &["minecraft/textures/entity/shulker/shulker_cyan.png"],
    &["minecraft/textures/entity/shulker/shulker_purple.png"],
    &["minecraft/textures/entity/shulker/shulker_blue.png"],
    &["minecraft/textures/entity/shulker/shulker_brown.png"],
    &["minecraft/textures/entity/shulker/shulker_green.png"],
    &["minecraft/textures/entity/shulker/shulker_red.png"],
    &["minecraft/textures/entity/shulker/shulker_black.png"],
    &["minecraft/textures/entity/shulker/shulker.png"],
];

fn name_index(table: &[&str], name: &str) -> Option<u32> {
    table.iter().position(|&n| n == name).map(|i| i as u32)
}

/// Build a [`PartAnim`] applying chest/shulker lid motion. `openness` is the
/// raw [0, 1] value; vanilla applies cubic easing so the lid decelerates as it
/// approaches the open or closed extreme.
fn lid_anim(kind: BlockEntityKind, openness: f32) -> PartAnim {
    if openness <= 0.0 {
        return PartAnim::default();
    }
    let inv = 1.0 - openness;
    let eased = 1.0 - inv * inv * inv;
    match kind {
        BlockEntityKind::Chest | BlockEntityKind::TrappedChest | BlockEntityKind::EnderChest => {
            // Parts are [bottom, lid, lock]; lid and lock swing together.
            let rot = glam::Vec3::new(-eased * std::f32::consts::FRAC_PI_2, 0.0, 0.0);
            PartAnim {
                rotation: vec![(1, rot), (2, rot)],
                ..Default::default()
            }
        }
        BlockEntityKind::ShulkerBox => PartAnim {
            rotation: vec![(0, glam::Vec3::new(0.0, eased * 270.0f32.to_radians(), 0.0))],
            translation: vec![(0, glam::Vec3::new(0.0, -eased * 8.0, 0.0))],
        },
        _ => PartAnim::default(),
    }
}

pub fn variant_for_block(
    kind: BlockEntityKind,
    name: &str,
    props: &crate::world::block::PropMap,
) -> u32 {
    match kind {
        // Ender chests have no `type` property and fall through to 0.
        BlockEntityKind::Chest | BlockEntityKind::TrappedChest => {
            let ty = match props.get("type") {
                Some("left") => 1,
                Some("right") => 2,
                _ => 0,
            };
            // Copper weathering stage selects the material row (waxing keeps
            // the stage's texture); trapped chests have no copper form.
            let material = match name.strip_prefix("waxed_").unwrap_or(name) {
                "copper_chest" => 1,
                "exposed_copper_chest" => 2,
                "weathered_copper_chest" => 3,
                "oxidized_copper_chest" => 4,
                _ => 0,
            };
            material * 3 + ty
        }
        BlockEntityKind::ShulkerBox => name
            .strip_suffix("_shulker_box")
            .and_then(|s| name_index(&DYE_COLOR_NAMES, s))
            .unwrap_or(16),
        BlockEntityKind::Sign => name
            .strip_suffix("_wall_sign")
            .or_else(|| name.strip_suffix("_sign"))
            .and_then(|s| name_index(&SIGN_WOOD_NAMES, s))
            .unwrap_or(0),
        _ => 0,
    }
}

/// XZ offset from a double-chest half to its partner. `type=left` connects at
/// `facing.getClockWise()`, `type=right` at `getCounterClockWise()` (vanilla
/// `ChestBlock.getConnectedDirection`).
pub fn chest_partner_offset(facing: &str, chest_type: &str) -> Option<(i32, i32)> {
    let clockwise = match facing {
        "north" => (1, 0),
        "east" => (0, 1),
        "south" => (-1, 0),
        "west" => (0, -1),
        _ => return None,
    };
    match chest_type {
        "left" => Some(clockwise),
        "right" => Some((-clockwise.0, -clockwise.1)),
        _ => None,
    }
}

/// Values mirror vanilla's `direction.toYRot()`; the draw code applies
/// `rotY(180 - yaw)` for y-down models and `rotY(-yaw)` for y-up ones.
pub fn yaw_for_block(kind: BlockEntityKind, props: &crate::world::block::PropMap) -> f32 {
    match kind {
        BlockEntityKind::Chest
        | BlockEntityKind::TrappedChest
        | BlockEntityKind::EnderChest
        | BlockEntityKind::ShulkerBox
        | BlockEntityKind::CopperGolemStatue => match props.get("facing") {
            Some("south") => 0.0,
            Some("west") => 90.0,
            Some("north") => 180.0,
            Some("east") => 270.0,
            _ => 0.0,
        },
        // Standing signs use a 0..15 rotation; wall signs have no rotation
        // property and face one of the four horizontal directions instead.
        BlockEntityKind::Sign => props
            .get("rotation")
            .and_then(|s| s.parse::<f32>().ok())
            .map(|r| r * 22.5)
            .or_else(|| match props.get("facing") {
                Some("south") => Some(0.0),
                Some("west") => Some(90.0),
                Some("north") => Some(180.0),
                Some("east") => Some(270.0),
                _ => None,
            })
            .unwrap_or(0.0),
        _ => 0.0,
    }
}

/// Vanilla swaps chest textures for the christmas set on Dec 24-26 (local
/// date), decided once at renderer construction. The check runs before the
/// trapped-chest one there, so trapped chests turn christmas too; ender and
/// copper chests never do.
fn is_christmas() -> bool {
    let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    now.month() == time::Month::December && (24..=26).contains(&now.day())
}

fn kind_definitions() -> Vec<KindDef> {
    let xmas = is_christmas();
    let chest_models = block_entity_model::bake_chest_models();
    // Ender chests have no double form; only the single model applies.
    let ender_models = vec![chest_models[0].clone()];
    // In 26.2, standing and hanging sign boards are blockstate models; the
    // vanilla sign renderers submit text only.
    vec![
        KindDef {
            kind: BlockEntityKind::Chest,
            models: chest_models.clone(),
            tex_variants: if xmas {
                CHEST_XMAS_TEXTURES
            } else {
                CHEST_TEXTURES
            },
            tex_size: 64,
        },
        KindDef {
            kind: BlockEntityKind::TrappedChest,
            models: chest_models,
            tex_variants: if xmas {
                CHEST_XMAS_TEXTURES
            } else {
                TRAPPED_CHEST_TEXTURES
            },
            tex_size: 64,
        },
        KindDef {
            kind: BlockEntityKind::EnderChest,
            models: ender_models,
            tex_variants: ENDER_CHEST_TEXTURES,
            tex_size: 64,
        },
        KindDef {
            kind: BlockEntityKind::ShulkerBox,
            models: vec![block_entity_model::bake_shulker_box_model()],
            tex_variants: SHULKER_TEXTURES,
            tex_size: 64,
        },
    ]
}

pub struct BlockEntityPipeline {
    pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    camera_layout: vk::DescriptorSetLayout,
    texture_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    camera_sets: Vec<vk::DescriptorSet>,
    camera_buffers: Vec<vk::Buffer>,
    camera_allocations: Vec<Allocation>,
    texture_sampler: vk::Sampler,
    entries: HashMap<BlockEntityKind, KindEntry>,
    copper_golem_statue: KindEntry,
    text_pipeline: vk::Pipeline,
    text_layout: vk::PipelineLayout,
    text_set_layout: vk::DescriptorSetLayout,
    text_pool: vk::DescriptorPool,
    text_sets: Vec<vk::DescriptorSet>,
    text_buffers: Vec<vk::Buffer>,
    text_allocations: Vec<Allocation>,
}

impl BlockEntityPipeline {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        device: &vk::Device,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
        render_pass: vk::RenderPass,
        allocator: &Arc<Mutex<Allocator>>,
        jar_assets_dir: &Path,
        asset_index: &Option<AssetIndex>,
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

        let push_constant_range = vk::PushConstantRange {
            stage_flags: vk::ShaderStageFlags::Vertex,
            offset: 0,
            size: 112,
        };
        let layouts = [camera_layout, texture_layout];
        let layout_info = vk::PipelineLayoutCreateInfo {
            set_layout_count: layouts.len() as u32,
            set_layouts: layouts.as_ptr(),
            push_constant_range_count: 1,
            push_constant_ranges: &push_constant_range,
            ..Default::default()
        };
        let pipeline_layout = device
            .create_pipeline_layout(&layout_info, None)
            .expect("failed to create block-entity pipeline layout");

        let pipeline = create_pipeline(
            device,
            render_pass,
            pipeline_layout,
            BlendMode::Opaque,
            ModelInput::PushConstant,
        );

        let defs = kind_definitions();
        let tex_count = defs
            .iter()
            .map(|d| d.tex_variants.len() as u32)
            .sum::<u32>()
            + COPPER_GOLEM_STATUE_TEXTURES.len() as u32;

        let pool_sizes = [
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::UniformBuffer,
                descriptor_count: MAX_FRAMES_IN_FLIGHT as u32,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::CombinedImageSampler,
                descriptor_count: tex_count.max(1),
            },
        ];
        let pool_info = vk::DescriptorPoolCreateInfo {
            max_sets: MAX_FRAMES_IN_FLIGHT as u32 + tex_count.max(1),
            pool_size_count: pool_sizes.len() as u32,
            pool_sizes: pool_sizes.as_ptr(),
            ..Default::default()
        };
        let descriptor_pool = device
            .create_descriptor_pool(&pool_info, None)
            .expect("failed to create block-entity descriptor pool");

        let camera_layouts_vec: Vec<_> = (0..MAX_FRAMES_IN_FLIGHT).map(|_| camera_layout).collect();
        let camera_alloc_info = vk::DescriptorSetAllocateInfo {
            descriptor_pool,
            descriptor_set_count: camera_layouts_vec.len() as u32,
            set_layouts: camera_layouts_vec.as_ptr(),
            ..Default::default()
        };
        let mut camera_sets = vec![vk::DescriptorSet::null(); camera_layouts_vec.len()];
        device
            .allocate_descriptor_sets(&camera_alloc_info, &mut camera_sets)
            .expect("failed to allocate block-entity camera sets");

        let mut camera_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        let mut camera_allocations = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
        for &set in &camera_sets {
            let (buf, alloc) = util::create_uniform_buffer(
                device,
                allocator,
                size_of::<CameraUniform>() as u64,
                "block_entity_camera_uniform",
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
            camera_allocations.push(alloc);
        }

        let texture_sampler = unsafe { util::create_nearest_sampler(device) };

        let mut entries = HashMap::new();
        let mut pending_uploads: Vec<util::PendingImageUpload> = Vec::new();
        let mut staging_to_free: Vec<(vk::Buffer, Allocation)> = Vec::new();
        for def in defs {
            let entry = build_entry(
                device,
                allocator,
                descriptor_pool,
                texture_layout,
                texture_sampler,
                jar_assets_dir,
                asset_index,
                def.models,
                def.tex_variants,
                def.tex_size,
                &mut pending_uploads,
                &mut staging_to_free,
            );
            entries.insert(def.kind, entry);
        }

        let copper_golem_statue = build_entry(
            device,
            allocator,
            descriptor_pool,
            texture_layout,
            texture_sampler,
            jar_assets_dir,
            asset_index,
            crate::renderer::entity_model::bake_copper_golem_statue_models(),
            COPPER_GOLEM_STATUE_TEXTURES,
            64,
            &mut pending_uploads,
            &mut staging_to_free,
        );

        util::upload_images_batched(device, queue, command_pool, &pending_uploads);

        {
            let mut alloc = allocator.lock().unwrap();
            for (buf, a) in staging_to_free {
                device.destroy_buffer(buf, None);
                alloc.free(a).ok();
            }
        }

        let bindings = [0, 1].map(|binding| vk::DescriptorSetLayoutBinding {
            binding,
            descriptor_type: vk::DescriptorType::CombinedImageSampler,
            descriptor_count: 1,
            stage_flags: vk::ShaderStageFlags::Fragment,
            ..Default::default()
        });
        let text_set_layout = device
            .create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo {
                    binding_count: 2,
                    bindings: bindings.as_ptr(),
                    ..Default::default()
                },
                None,
            )
            .expect("sign atlas layout");
        let text_layouts = [camera_layout, text_set_layout];
        let text_layout = device
            .create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo {
                    set_layout_count: 2,
                    set_layouts: text_layouts.as_ptr(),
                    ..Default::default()
                },
                None,
            )
            .expect("sign text layout");
        let text_pipeline = create_sign_pipeline(device, render_pass, text_layout);
        let text_pool = device
            .create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo {
                    max_sets: MAX_FRAMES_IN_FLIGHT as u32,
                    pool_size_count: 1,
                    pool_sizes: &vk::DescriptorPoolSize {
                        ty: vk::DescriptorType::CombinedImageSampler,
                        descriptor_count: (2 * MAX_FRAMES_IN_FLIGHT) as u32,
                    },
                    ..Default::default()
                },
                None,
            )
            .expect("sign atlas descriptor pool");
        let text_layouts = vec![text_set_layout; MAX_FRAMES_IN_FLIGHT];
        let mut text_sets = vec![vk::DescriptorSet::null(); MAX_FRAMES_IN_FLIGHT];
        device
            .allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo {
                    descriptor_pool: text_pool,
                    descriptor_set_count: MAX_FRAMES_IN_FLIGHT as u32,
                    set_layouts: text_layouts.as_ptr(),
                    ..Default::default()
                },
                &mut text_sets,
            )
            .expect("sign atlas sets");
        let mut text_buffers = Vec::new();
        let mut text_allocations = Vec::new();
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            let (buffer, alloc) = util::create_mapped_buffer(
                device,
                allocator,
                &vec![0u8; MAX_SIGN_VERTICES * size_of::<SignVertex>()],
                vk::BufferUsageFlags::VertexBuffer,
                "sign_text_vertices",
            );
            text_buffers.push(buffer);
            text_allocations.push(alloc);
        }

        Self {
            text_pipeline,
            text_layout,
            text_set_layout,
            text_pool,
            text_sets,
            text_buffers,
            text_allocations,
            pipeline,
            pipeline_layout,
            camera_layout,
            texture_layout,
            descriptor_pool,
            camera_sets,
            camera_buffers,
            camera_allocations,
            texture_sampler,
            entries,
            copper_golem_statue,
        }
    }

    pub fn update_camera(&mut self, frame: usize, uniform: &CameraUniform) {
        let bytes = bytemuck::bytes_of(uniform);
        self.camera_allocations[frame].mapped_slice_mut().unwrap()[..bytes.len()]
            .copy_from_slice(bytes);
    }

    pub fn draw(
        &mut self,
        device: &vk::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        anchor: glam::DVec3,
        eye: glam::DVec3,
        items: &[BlockEntityRenderInfo],
        font: Option<(&GlyphMap, [vk::DescriptorImageInfo; 2])>,
    ) {
        if items.is_empty() {
            return;
        }

        cmd.bind_pipeline(vk::PipelineBindPoint::Graphics, self.pipeline);

        let mut bound_entry: *const KindEntry = std::ptr::null();
        let mut bound_set: vk::DescriptorSet = vk::DescriptorSet::null();

        for info in items {
            let is_statue = info.kind == BlockEntityKind::CopperGolemStatue;
            let entry = if is_statue {
                &self.copper_golem_statue
            } else if let Some(entry) = self.entries.get(&info.kind) {
                entry
            } else {
                continue;
            };
            let variant_idx = (info.variant as usize).min(entry.textures.len().saturating_sub(1));
            let slot = &entry.textures[variant_idx];

            let entry_ptr: *const KindEntry = entry;
            if bound_entry != entry_ptr {
                cmd.bind_vertex_buffers(0, &[entry.vertex_buffer], &[0]);
                bound_entry = entry_ptr;
                bound_set = vk::DescriptorSet::null();
            }
            if bound_set != slot.set {
                cmd.bind_descriptor_sets(
                    vk::PipelineBindPoint::Graphics,
                    self.pipeline_layout,
                    0,
                    &[self.camera_sets[frame], slot.set],
                    &[],
                );
                bound_set = slot.set;
            }

            let model = if is_statue {
                let pose = info.statue_pose.unwrap_or(0);
                &entry.models[(pose as usize).min(entry.models.len() - 1)]
            } else {
                &entry.models[info.variant as usize % entry.models.len()]
            };

            let block_center = (glam::DVec3::new(
                info.pos.x as f64 + 0.5,
                info.pos.y as f64,
                info.pos.z as f64 + 0.5,
            ) - anchor)
                .as_vec3();
            let model_mat = match model.convention {
                // With the 180 yaw offset and the convention's baked-in flip
                // this reproduces vanilla's block-entity `scale(1,-1,-1)`.
                ModelConvention::EntityYDown => {
                    glam::Mat4::from_translation(block_center)
                        * glam::Mat4::from_rotation_y((180.0f32 - info.yaw).to_radians())
                }
                // Vanilla `ChestRenderer`: rotate by -facing.toYRot() about the
                // block center; coords are relative to the block's min corner.
                ModelConvention::BlockYUp if is_statue => {
                    // CopperGolemStatueBlockRenderer translates to block
                    // center and rotates by -opposite(facing).toYRot().
                    glam::Mat4::from_translation(block_center)
                        * glam::Mat4::from_rotation_y((-info.yaw - 180.0).to_radians())
                }
                ModelConvention::BlockYUp => {
                    glam::Mat4::from_translation(block_center)
                        * glam::Mat4::from_rotation_y((-info.yaw).to_radians())
                        * glam::Mat4::from_translation(glam::Vec3::new(-0.5, 0.0, -0.5))
                }
            };

            let mut model_mat = model_mat;
            if is_statue {
                // CopperGolemStatueModel.setupAnim sets root.zRot = PI.
                model_mat *= glam::Mat4::from_rotation_z(std::f32::consts::PI);
            }
            let anim = if is_statue {
                PartAnim::default()
            } else {
                lid_anim(info.kind, info.lid_open)
            };
            let part_transforms = model.compute_part_transforms(&anim);
            for (i, (start, count)) in model.part_ranges.iter().enumerate() {
                if *count == 0 {
                    continue;
                }
                let part_mat = model_mat * part_transforms[i];
                let cols = part_mat.to_cols_array();
                // Shared entity shader push block: mat, tint, overlay_color, uv_params.
                // Block entities are opaque with no hurt flash or UV scroll.
                let no_overlay = [0.0f32, 0.0, 0.0, 1.0];
                let uv_params = [0.0f32; 4];
                let mut bytes = [0u8; 112];
                bytes[..64].copy_from_slice(bytemuck::cast_slice(&cols));
                bytes[64..80].copy_from_slice(bytemuck::cast_slice(&WHITE_TINT));
                bytes[80..96].copy_from_slice(bytemuck::cast_slice(&no_overlay));
                bytes[96..112].copy_from_slice(bytemuck::cast_slice(&uv_params));
                cmd.push_constants(
                    self.pipeline_layout,
                    vk::ShaderStageFlags::Vertex,
                    0,
                    &bytes,
                );
                cmd.draw(*count, 1, *start, 0);
            }
        }
        if let Some((glyphs, textures)) = font {
            self.draw_sign_text(device, cmd, frame, anchor, eye, items, glyphs, textures);
        }
    }

    fn draw_sign_text(
        &mut self,
        device: &vk::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        anchor: glam::DVec3,
        eye: glam::DVec3,
        items: &[BlockEntityRenderInfo],
        glyphs: &GlyphMap,
        textures: [vk::DescriptorImageInfo; 2],
    ) {
        let mut vertices = Vec::new();
        for info in items.iter().filter(|i| i.kind == BlockEntityKind::Sign) {
            for (front, lines, dye, glowing) in [
                (
                    true,
                    &info.sign_front,
                    info.sign_front_color,
                    info.sign_front_glowing,
                ),
                (
                    false,
                    &info.sign_back,
                    info.sign_back_color,
                    info.sign_back_glowing,
                ),
            ] {
                let Some(lines) = lines else {
                    continue;
                };
                let base =
                    (glam::DVec3::new(info.pos.x as f64, info.pos.y as f64, info.pos.z as f64)
                        - anchor)
                        .as_vec3();
                // StandingSignRenderer.textTransformation, including wall offset,
                // back-face rotation and the inverted Y of Font coordinates.
                let matrix = glam::Mat4::from_translation(base + glam::Vec3::splat(0.5))
                    * glam::Mat4::from_rotation_y((-info.yaw).to_radians())
                    * glam::Mat4::from_translation(if info.sign_wall {
                        glam::Vec3::new(0.0, -0.3125, -0.4375)
                    } else {
                        glam::Vec3::ZERO
                    })
                    * glam::Mat4::from_rotation_y(if front { 0.0 } else { std::f32::consts::PI })
                    * glam::Mat4::from_translation(glam::Vec3::new(0.0, 1.0 / 3.0, 0.046666667))
                    * glam::Mat4::from_scale(glam::Vec3::new(1.0 / 96.0, -1.0 / 96.0, 1.0 / 96.0));
                let black = dye == [29.0 / 255.0, 29.0 / 255.0, 33.0 / 255.0];
                let dark = if black && glowing {
                    [0.941, 0.922, 0.922]
                } else {
                    dye.map(|c| c * 0.4)
                };
                let color = if glowing {
                    dye
                } else {
                    dark.map(|c| c * info.sign_light)
                };
                let near = (glam::DVec3::new(
                    info.pos.x as f64 + 0.5,
                    info.pos.y as f64 + 0.5,
                    info.pos.z as f64 + 0.5,
                ) - eye)
                    .length_squared()
                    < 256.0;
                let outline = glowing && (black || near);
                for (row, line) in lines.iter().enumerate() {
                    // Vanilla SignBlockEntity: 90 px line width, 10 px height.
                    let chars: Vec<_> = line
                        .chars()
                        .take(256)
                        .scan(0.0f32, |width, ch| {
                            let gi = glyphs.glyph(ch, None);
                            if *width + gi.advance > 90.0 {
                                return None;
                            }
                            let x = *width;
                            *width += gi.advance;
                            Some((x, gi))
                        })
                        .collect();
                    let width: f32 = chars.last().map_or(0.0, |(x, gi)| x + gi.advance);
                    let y = row as f32 * 10.0 - 20.0;
                    if outline {
                        for (x, gi) in &chars {
                            for dy in -1..=1 {
                                for dx in -1..=1 {
                                    if dx != 0 || dy != 0 {
                                        push_sign_glyph(
                                            &mut vertices,
                                            matrix,
                                            gi,
                                            *x - width / 2.0 + dx as f32 * 0.5,
                                            y + dy as f32 * 0.5,
                                            dark,
                                        );
                                    }
                                }
                            }
                        }
                    }
                    for (x, gi) in &chars {
                        push_sign_glyph(&mut vertices, matrix, gi, *x - width / 2.0, y, color);
                    }
                }
            }
        }
        if vertices.is_empty() {
            return;
        }
        let len = vertices.len().min(MAX_SIGN_VERTICES);
        let len = len - len % 6;
        let bytes = bytemuck::cast_slice(&vertices[..len]);
        self.text_allocations[frame].mapped_slice_mut().unwrap()[..bytes.len()]
            .copy_from_slice(bytes);
        let writes: Vec<_> = textures
            .iter()
            .enumerate()
            .map(|(binding, image)| vk::WriteDescriptorSet {
                dst_set: self.text_sets[frame],
                dst_binding: binding as u32,
                descriptor_type: vk::DescriptorType::CombinedImageSampler,
                descriptor_count: 1,
                image_info: image,
                ..Default::default()
            })
            .collect();
        device.update_descriptor_sets(&writes, &[]);
        cmd.bind_pipeline(vk::PipelineBindPoint::Graphics, self.text_pipeline);
        cmd.bind_descriptor_sets(
            vk::PipelineBindPoint::Graphics,
            self.text_layout,
            0,
            &[self.camera_sets[frame], self.text_sets[frame]],
            &[],
        );
        cmd.bind_vertex_buffers(0, &[self.text_buffers[frame]], &[0]);
        cmd.draw(len as u32, 1, 0, 0);
    }

    pub fn recreate_pipeline(&mut self, device: &vk::Device, render_pass: vk::RenderPass) {
        device.destroy_pipeline(self.pipeline, None);
        device.destroy_pipeline(self.text_pipeline, None);
        self.text_pipeline = create_sign_pipeline(device, render_pass, self.text_layout);
        self.pipeline = create_pipeline(
            device,
            render_pass,
            self.pipeline_layout,
            BlendMode::Opaque,
            ModelInput::PushConstant,
        );
    }

    pub fn destroy(&mut self, device: &vk::Device, allocator: &Arc<Mutex<Allocator>>) {
        let mut alloc = allocator.lock().unwrap();
        for i in 0..MAX_FRAMES_IN_FLIGHT {
            device.destroy_buffer(self.text_buffers[i], None);
            alloc
                .free(std::mem::replace(&mut self.text_allocations[i], unsafe {
                    std::mem::zeroed()
                }))
                .ok();
            device.destroy_buffer(self.camera_buffers[i], None);
            alloc
                .free(std::mem::replace(&mut self.camera_allocations[i], unsafe {
                    std::mem::zeroed()
                }))
                .ok();
        }
        device.destroy_sampler(self.texture_sampler, None);
        for entry in self
            .entries
            .values_mut()
            .chain(std::iter::once(&mut self.copper_golem_statue))
        {
            device.destroy_buffer(entry.vertex_buffer, None);
            alloc
                .free(std::mem::replace(&mut entry.vertex_allocation, unsafe {
                    std::mem::zeroed()
                }))
                .ok();
            for slot in entry.textures.iter_mut() {
                device.destroy_image_view(slot.view, None);
                alloc
                    .free(std::mem::replace(&mut slot.allocation, unsafe {
                        std::mem::zeroed()
                    }))
                    .ok();
                device.destroy_image(slot.image, None);
            }
        }
        drop(alloc);

        device.destroy_pipeline(self.pipeline, None);
        device.destroy_pipeline(self.text_pipeline, None);
        device.destroy_pipeline_layout(self.text_layout, None);
        device.destroy_descriptor_pool(self.text_pool, None);
        device.destroy_descriptor_set_layout(self.text_set_layout, None);
        device.destroy_pipeline_layout(self.pipeline_layout, None);
        device.destroy_descriptor_pool(self.descriptor_pool, None);
        device.destroy_descriptor_set_layout(self.camera_layout, None);
        device.destroy_descriptor_set_layout(self.texture_layout, None);
    }
}

#[allow(clippy::too_many_arguments)]
fn build_entry(
    device: &vk::Device,
    allocator: &Arc<Mutex<Allocator>>,
    descriptor_pool: vk::DescriptorPool,
    texture_layout: vk::DescriptorSetLayout,
    texture_sampler: vk::Sampler,
    jar_assets_dir: &Path,
    asset_index: &Option<AssetIndex>,
    mut models: Vec<BakedEntityModel>,
    tex_variants: &[&[&str]],
    fallback_tex_size: u32,
    pending_uploads: &mut Vec<util::PendingImageUpload>,
    staging_to_free: &mut Vec<(vk::Buffer, Allocation)>,
) -> KindEntry {
    let mut all_vertices: Vec<ChunkVertex> = Vec::new();
    for model in &mut models {
        let base = all_vertices.len() as u32;
        all_vertices.append(&mut model.vertices);
        for range in &mut model.part_ranges {
            range.0 += base;
        }
    }
    let vert_bytes = bytemuck::cast_slice::<ChunkVertex, u8>(&all_vertices);
    let (vertex_buffer, vertex_allocation) = util::create_mapped_buffer(
        device,
        allocator,
        vert_bytes,
        vk::BufferUsageFlags::VertexBuffer,
        "block_entity_vertices",
    );

    let textures = tex_variants
        .iter()
        .map(|keys| {
            build_texture_slot(
                device,
                allocator,
                descriptor_pool,
                texture_layout,
                texture_sampler,
                jar_assets_dir,
                asset_index,
                keys,
                fallback_tex_size,
                pending_uploads,
                staging_to_free,
            )
        })
        .collect();

    KindEntry {
        models,
        vertex_buffer,
        vertex_allocation,
        textures,
    }
}

#[allow(clippy::too_many_arguments)]
fn build_texture_slot(
    device: &vk::Device,
    allocator: &Arc<Mutex<Allocator>>,
    descriptor_pool: vk::DescriptorPool,
    texture_layout: vk::DescriptorSetLayout,
    texture_sampler: vk::Sampler,
    jar_assets_dir: &Path,
    asset_index: &Option<AssetIndex>,
    keys: &[&str],
    fallback_tex_size: u32,
    pending_uploads: &mut Vec<util::PendingImageUpload>,
    staging_to_free: &mut Vec<(vk::Buffer, Allocation)>,
) -> TextureSlot {
    let (pixels, width, height) = keys
        .iter()
        .find_map(|key| {
            let path = resolve_asset_path(jar_assets_dir, asset_index, key);
            util::load_png(&path)
        })
        .unwrap_or_else(|| {
            tracing::warn!("Failed to load BE texture {:?}, using fallback", keys);
            fallback_texture(fallback_tex_size)
        });

    let (image, view, allocation) =
        util::create_gpu_image(device, allocator, width, height, "block_entity_texture");
    let (staging_buf, staging_alloc) =
        util::create_staging_buffer(device, allocator, &pixels, "block_entity_texture_staging");
    pending_uploads.push(util::PendingImageUpload {
        staging_buffer: staging_buf,
        staging_size: pixels.len() as u64,
        image,
        width,
        height,
        mip_levels: 1,
    });
    staging_to_free.push((staging_buf, staging_alloc));

    let tex_alloc_info = vk::DescriptorSetAllocateInfo {
        descriptor_pool,
        descriptor_set_count: 1,
        set_layouts: &texture_layout,
        ..Default::default()
    };
    let mut set = vk::DescriptorSet::null();
    device
        .allocate_descriptor_sets(&tex_alloc_info, slice::from_mut(&mut set))
        .expect("failed to allocate BE texture descriptor set");

    let image_info = vk::DescriptorImageInfo {
        sampler: texture_sampler,
        image_view: view,
        image_layout: vk::ImageLayout::ShaderReadOnlyOptimal,
    };
    let tex_write = vk::WriteDescriptorSet {
        dst_set: set,
        dst_binding: 0,
        descriptor_type: vk::DescriptorType::CombinedImageSampler,
        descriptor_count: 1,
        image_info: &image_info,
        ..Default::default()
    };
    device.update_descriptor_sets(&[tex_write], &[]);

    TextureSlot {
        image,
        view,
        allocation,
        set,
    }
}

fn push_sign_glyph(
    vertices: &mut Vec<SignVertex>,
    matrix: glam::Mat4,
    gi: &crate::ui::font::GlyphInfo,
    x: f32,
    y: f32,
    color: [f32; 3],
) {
    if gi.pixel_w == 0 || gi.pixel_h == 0 || vertices.len() + 6 > MAX_SIGN_VERTICES {
        return;
    }
    let x0 = x + gi.left;
    let y0 = y + gi.top;
    let u0 = gi.atlas_x as f32 / GLYPH_ATLAS_SIZE as f32;
    let v0 = gi.atlas_y as f32 / GLYPH_ATLAS_SIZE as f32;
    let u1 = (gi.atlas_x + gi.pixel_w) as f32 / GLYPH_ATLAS_SIZE as f32;
    let v1 = (gi.atlas_y + gi.pixel_h) as f32 / GLYPH_ATLAS_SIZE as f32;
    let corners = [
        (x0, y0, u0, v0),
        (x0, y0 + gi.draw_h, u0, v1),
        (x0 + gi.draw_w, y0 + gi.draw_h, u1, v1),
        (x0 + gi.draw_w, y0, u1, v0),
    ];
    for index in [0, 1, 2, 0, 2, 3] {
        let (px, py, u, v) = corners[index];
        vertices.push(SignVertex {
            position: matrix
                .transform_point3(glam::Vec3::new(px, py, 0.0))
                .to_array(),
            uv_layer: [u, v, gi.atlas_layer as f32],
            color: [color[0], color[1], color[2], 1.0],
            colored: if gi.colored { 1.0 } else { 0.0 },
        });
    }
}

fn create_sign_pipeline(
    device: &vk::Device,
    render_pass: vk::RenderPass,
    layout: vk::PipelineLayout,
) -> vk::Pipeline {
    let vs = shader::create_shader_module(device, shader::include_spirv!("sign_text.vert.spv"));
    let fs = shader::create_shader_module(device, shader::include_spirv!("sign_text.frag.spv"));
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
    let binding = [vk::VertexInputBindingDescription {
        binding: 0,
        stride: size_of::<SignVertex>() as u32,
        input_rate: vk::VertexInputRate::Vertex,
    }];
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
            format: vk::Format::R32G32B32Sfloat,
            offset: 12,
        },
        vk::VertexInputAttributeDescription {
            location: 2,
            binding: 0,
            format: vk::Format::R32G32B32A32Sfloat,
            offset: 24,
        },
        vk::VertexInputAttributeDescription {
            location: 3,
            binding: 0,
            format: vk::Format::R32Sfloat,
            offset: 40,
        },
    ];
    let vertex_input = vk::PipelineVertexInputStateCreateInfo {
        vertex_binding_description_count: 1,
        vertex_binding_descriptions: binding.as_ptr(),
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
        ..Default::default()
    };
    let samples = vk::PipelineMultisampleStateCreateInfo {
        rasterization_samples: vk::SampleCountFlags::Type1,
        ..Default::default()
    };
    let depth = vk::PipelineDepthStencilStateCreateInfo {
        depth_test_enable: vk::TRUE,
        depth_write_enable: vk::TRUE,
        depth_compare_op: vk::CompareOp::LessOrEqual,
        ..Default::default()
    };
    let attachment = [vk::PipelineColorBlendAttachmentState {
        blend_enable: vk::TRUE,
        src_color_blend_factor: vk::BlendFactor::One,
        dst_color_blend_factor: vk::BlendFactor::OneMinusSrcAlpha,
        color_blend_op: vk::BlendOp::Add,
        src_alpha_blend_factor: vk::BlendFactor::One,
        dst_alpha_blend_factor: vk::BlendFactor::OneMinusSrcAlpha,
        alpha_blend_op: vk::BlendOp::Add,
        color_write_mask: vk::ColorComponentFlags::RGBA,
    }];
    let blending = vk::PipelineColorBlendStateCreateInfo {
        attachment_count: 1,
        attachments: attachment.as_ptr(),
        ..Default::default()
    };
    let dynamic = [vk::DynamicState::Viewport, vk::DynamicState::Scissor];
    let dynamic_state = vk::PipelineDynamicStateCreateInfo {
        dynamic_state_count: 2,
        dynamic_states: dynamic.as_ptr(),
        ..Default::default()
    };
    let infos = [vk::GraphicsPipelineCreateInfo {
        stage_count: 2,
        stages: stages.as_ptr(),
        vertex_input_state: &vertex_input,
        input_assembly_state: &assembly,
        viewport_state: &viewport,
        rasterization_state: &raster,
        multisample_state: &samples,
        depth_stencil_state: &depth,
        color_blend_state: &blending,
        dynamic_state: &dynamic_state,
        layout,
        render_pass,
        subpass: 0,
        ..Default::default()
    }];
    let mut result = vk::Pipeline::null();
    device
        .create_graphics_pipelines(
            vk::PipelineCache::null(),
            &infos,
            None,
            slice::from_mut(&mut result),
        )
        .expect("sign text pipeline");
    device.destroy_shader_module(vs, None);
    device.destroy_shader_module(fs, None);
    result
}

#[cfg(test)]
mod sign_text_tests {
    use super::*;

    #[test]
    fn sign_board_geometry_is_not_drawn_as_block_entity_geometry() {
        assert!(
            kind_definitions()
                .iter()
                .all(|definition| definition.kind != BlockEntityKind::Sign)
        );
    }

    #[test]
    fn glyph_quad_uses_atlas_layer_and_world_matrix() {
        let glyph = crate::ui::font::GlyphInfo {
            atlas_layer: 2,
            colored: false,
            atlas_x: 8,
            atlas_y: 16,
            pixel_w: 4,
            pixel_h: 7,
            draw_w: 4.0,
            draw_h: 7.0,
            left: 1.0,
            top: 2.0,
            advance: 5.0,
            bold_offset: 1.0,
            shadow_offset: 1.0,
        };
        let mut vertices = Vec::new();
        push_sign_glyph(
            &mut vertices,
            glam::Mat4::from_translation(glam::Vec3::new(2.0, 3.0, 4.0)),
            &glyph,
            10.0,
            20.0,
            [1.0, 0.5, 0.0],
        );
        assert_eq!(vertices.len(), 6);
        assert_eq!(vertices[0].position, [13.0, 25.0, 4.0]);
        assert_eq!(vertices[0].uv_layer, [8.0 / 2048.0, 16.0 / 2048.0, 2.0]);
        assert_eq!(vertices[2].position, [17.0, 32.0, 4.0]);
    }
}
