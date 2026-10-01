pub(super) mod sign_text;

use std::collections::HashMap;
use std::path::Path;
use std::slice;
use std::sync::{Arc, Mutex};

use azalea_core::position::BlockPos;
use azalea_registry::builtin::BlockEntityKind;
use pomme_gpu_allocator::vulkan::{Allocation, Allocator};
use pyronyx::vk;
use sign_text::{MAX_SIGN_VERTICES, SignVertex};

use super::text_display::TextDisplayDraw;
use crate::assets::{AssetIndex, resolve_asset_path};
use crate::renderer::camera::CameraUniform;
use crate::renderer::chunk::mesher::ChunkVertex;
use crate::renderer::entity_model::{BakedEntityModel, ModelConvention, PartAnim};
use crate::renderer::pipelines::entity_renderer::{
    BlendMode, ModelInput, WHITE_TINT, create_pipeline, fallback_texture,
};
use crate::renderer::placed_head_skin::{MAX_ENTRIES, PlacedHeadSkinCache};
use crate::renderer::{
    BlockEntityModelDrawCounts, MAX_FRAMES_IN_FLIGHT, block_entity_model, shader, util,
};
use crate::ui::font::GlyphMap;
use crate::world::block_entity::PlayerHeadProfileSource;

// One extra slot for the pack-reloadable default sheet.
const MAX_HEAD_TEXTURES: usize = MAX_ENTRIES + 1;

pub struct BlockEntityRenderInfo {
    pub pos: BlockPos,
    pub player_head_profile_source: Option<PlayerHeadProfileSource>,
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
pub(crate) const SIGN_WOOD_NAMES: [&str; 12] = [
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

// Skull variants are stored as adjacent [standing, wall] pairs. The pair bit
// preserves the wall-head transform independently of the texture/model type.
const SKULL_TEXTURES: &[&[&str]] = &[
    &["minecraft/textures/entity/skeleton/skeleton.png"],
    &["minecraft/textures/entity/skeleton/skeleton.png"],
    &["minecraft/textures/entity/skeleton/wither_skeleton.png"],
    &["minecraft/textures/entity/skeleton/wither_skeleton.png"],
    &["minecraft/textures/entity/zombie/zombie.png"],
    &["minecraft/textures/entity/zombie/zombie.png"],
    &["minecraft/textures/entity/creeper/creeper.png"],
    &["minecraft/textures/entity/creeper/creeper.png"],
    &["minecraft/textures/entity/player/slim/steve.png"],
    &["minecraft/textures/entity/player/slim/steve.png"],
    // Dragon and piglin heads have unique geometry; their intentionally empty
    // models below prevent a misleading Steve/standard-skull substitute.
    &["minecraft/textures/entity/skeleton/skeleton.png"],
    &["minecraft/textures/entity/skeleton/skeleton.png"],
];

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
pub(crate) fn lid_anim(kind: BlockEntityKind, openness: f32) -> PartAnim {
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
        BlockEntityKind::Skull => skull_variant(name),
        BlockEntityKind::Sign | BlockEntityKind::HangingSign => name
            .strip_suffix("_wall_hanging_sign")
            .or_else(|| name.strip_suffix("_hanging_sign"))
            .or_else(|| name.strip_suffix("_wall_sign"))
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
        BlockEntityKind::Skull => props
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
        BlockEntityKind::Sign | BlockEntityKind::HangingSign => props
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
pub(crate) fn is_christmas() -> bool {
    let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    christmas_on(now.month(), now.day())
}

pub(crate) fn christmas_on(month: time::Month, day: u8) -> bool {
    month == time::Month::December && (24..=26).contains(&day)
}

fn kind_definitions(xmas: bool) -> Vec<KindDef> {
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
            kind: BlockEntityKind::Conduit,
            models: vec![block_entity_model::bake_conduit_model()],
            tex_variants: &[&["minecraft/textures/entity/conduit/base.png"]],
            tex_size: 64,
        },
        KindDef {
            kind: BlockEntityKind::Skull,
            models: skull_models(),
            tex_variants: SKULL_TEXTURES,
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

// An exhausted slot falls back to push-constant draws.
const MAX_CHEST_INSTANCES: usize = 16384;
// Bound the pairwise disjointness checks, including chest_run, independently
// of frame capacity. Separate windows retain their original draw order.
const MAX_CHEST_WINDOW: usize = 64;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ChestInstance {
    model: [[f32; 4]; 4],
    tint: [f32; 4],
    overlay_color: [f32; 4],
    uv_params: [f32; 4],
}

fn skull_variant(name: &str) -> u32 {
    let (kind, wall) = match name {
        "skeleton_skull" => (0, false),
        "skeleton_wall_skull" => (0, true),
        "wither_skeleton_skull" => (1, false),
        "wither_skeleton_wall_skull" => (1, true),
        "zombie_head" => (2, false),
        "zombie_wall_head" => (2, true),
        "creeper_head" => (3, false),
        "creeper_wall_head" => (3, true),
        "player_head" => (4, false),
        "player_wall_head" => (4, true),
        // Dragon, piglin, and unknown skull meshes are intentionally not
        // approximated with the standard cube or a player skin.
        _ => (
            5,
            name.ends_with("_wall_head") || name.ends_with("_wall_skull"),
        ),
    };
    kind * 2 + u32::from(wall)
}

fn skull_models() -> Vec<BakedEntityModel> {
    let standard = block_entity_model::bake_skull_model(32);
    let zombie = block_entity_model::bake_skull_model(64);
    let player = block_entity_model::bake_player_head_model();
    let unsupported = block_entity_model::bake_unsupported_skull_model();
    vec![
        standard.clone(), // skeleton
        standard,
        zombie,                                   // zombie heads use the 64x64 skin layout
        block_entity_model::bake_skull_model(32), // creeper
        player,
        unsupported,
    ]
}

fn is_wall_skull_variant(variant: u32) -> bool {
    variant % 2 == 1
}

fn skull_wall_model_matrix(model: glam::Mat4, yaw: f32) -> glam::Mat4 {
    let facing = glam::Mat4::from_rotation_y((-yaw).to_radians());
    model
        * facing
        * glam::Mat4::from_translation(glam::Vec3::new(0.0, 0.25, -0.25))
        * facing.inverse()
}

fn chest_matrix(info: &BlockEntityRenderInfo, anchor: glam::DVec3) -> glam::Mat4 {
    let center = (glam::DVec3::new(
        info.pos.x as f64 + 0.5,
        info.pos.y as f64,
        info.pos.z as f64 + 0.5,
    ) - anchor)
        .as_vec3();
    glam::Mat4::from_translation(center)
        * glam::Mat4::from_rotation_y((-info.yaw).to_radians())
        * glam::Mat4::from_translation(glam::Vec3::new(-0.5, 0.0, -0.5))
}

fn chest_instance(matrix: glam::Mat4) -> ChestInstance {
    ChestInstance {
        model: matrix.to_cols_array_2d(),
        tint: WHITE_TINT,
        overlay_color: [0.0, 0.0, 0.0, 1.0],
        uv_params: [0.0; 4],
    }
}

fn chest_instances(
    items: &[&BlockEntityRenderInfo],
    model: &BakedEntityModel,
    anchor: glam::DVec3,
) -> Vec<ChestInstance> {
    let poses = model.compute_part_transforms(&PartAnim::default());
    let mut data = Vec::with_capacity(items.len() * model.part_ranges.len());
    for (part, (_, count)) in model.part_ranges.iter().enumerate() {
        if *count == 0 {
            continue;
        }
        for chest in items {
            data.push(chest_instance(chest_matrix(chest, anchor) * poses[part]));
        }
    }
    data
}

fn closed_chest(info: &BlockEntityRenderInfo) -> bool {
    info.kind == BlockEntityKind::Chest && info.lid_open == 0.0
}

// Baked closed single chest: body/lid x,z=[1,15]/16; lock x=[7,9]/16,
// z=[15,16]/16; y=[0,14]/16. Only cardinal yaw has these exact bounds.
// Double halves may touch at a seam and are deliberately not relaxed.
#[cfg(test)]
thread_local! {
    static CHEST_COMPARISONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn chests_disjoint(a: &BlockEntityRenderInfo, b: &BlockEntityRenderInfo) -> bool {
    #[cfg(test)]
    CHEST_COMPARISONS.with(|count| count.set(count.get() + 1));
    let dx = i64::from(a.pos.x) - i64::from(b.pos.x);
    let dy = i64::from(a.pos.y) - i64::from(b.pos.y);
    let dz = i64::from(a.pos.z) - i64::from(b.pos.z);
    if dx.abs() > 1 || dy.abs() > 1 || dz.abs() > 1 {
        return true;
    }
    if dy != 0 {
        return true; // closed height 14/16, so different block levels have a gap
    }
    if a.variant % 3 != 0 || b.variant % 3 != 0 {
        return false;
    }
    let bounds = |yaw: f32| match yaw {
        0.0 => Some(((1, 15), (1, 16))),
        90.0 => Some(((0, 15), (1, 15))),
        180.0 => Some(((1, 15), (0, 15))),
        270.0 => Some(((1, 16), (1, 15))),
        _ => None,
    };
    let (Some((ax, az)), Some((bx, bz))) = (bounds(a.yaw), bounds(b.yaw)) else {
        return false;
    };
    let apart =
        |a: (i64, i64), b: (i64, i64), delta: i64| a.1 < b.0 - delta * 16 || b.1 - delta * 16 < a.0;
    apart(ax, bx, dx) || apart(az, bz, dz)
}

// Every window is pairwise disjoint. Never sort across an intersecting chest,
// animated lid or other BE (notably translucent heads).
struct ChestOrder<'a> {
    items: Vec<&'a BlockEntityRenderInfo>,
    windows: Vec<usize>,
}

impl<'a> std::ops::Deref for ChestOrder<'a> {
    type Target = [&'a BlockEntityRenderInfo];
    fn deref(&self) -> &Self::Target {
        &self.items
    }
}

fn chest_order<'a>(items: &'a [BlockEntityRenderInfo]) -> ChestOrder<'a> {
    let mut order = ChestOrder {
        items: Vec::with_capacity(items.len()),
        windows: Vec::with_capacity(items.len()),
    };
    let mut start = 0;
    while start < items.len() {
        if !closed_chest(&items[start]) {
            order.items.push(&items[start]);
            order.windows.push(start);
            start += 1;
            continue;
        }
        let mut end = start + 1;
        while end < items.len()
            && end - start < MAX_CHEST_WINDOW
            && closed_chest(&items[end])
            && items[start..end]
                .iter()
                .all(|other| chests_disjoint(other, &items[end]))
        {
            end += 1;
        }
        let mut window: Vec<_> = items[start..end].iter().collect();
        window.sort_by_key(|info| info.variant); // stable within a variant
        order
            .windows
            .extend(std::iter::repeat_n(start, window.len()));
        order.items.extend(window);
        start = end;
    }
    order
}

// Only reorder opaque parts across disjoint blocks in a consecutive,
// identical texture/geometry run. Otherwise use the old draw path.
fn chest_run(items: &ChestOrder<'_>, start: usize, max_items: usize) -> usize {
    let first = items[start];
    if !closed_chest(first) || max_items < 2 {
        return 0;
    }
    let mut end = start + 1;
    while end < items.len() && end - start < max_items {
        let next = items[end];
        if items.windows[end] != items.windows[start]
            || !closed_chest(next)
            || next.variant != first.variant
            || items[start..end]
                .iter()
                .any(|other| !chests_disjoint(other, next))
        {
            break;
        }
        end += 1;
    }
    if end - start >= 2 { end - start } else { 0 }
}

#[derive(Default)]
struct ChestFrame {
    buffer: Option<(vk::Buffer, Allocation)>,
    used: usize,
    unavailable: bool,
}

/// Mapped storage belongs to one fence slot, never to an extraction worker.
#[derive(Default)]
struct TextDisplayFrame {
    buffer: Option<(vk::Buffer, Allocation)>,
    capacity: usize,
    used: usize,
    // Growth may happen after an earlier draw was recorded in the same command buffer.
    retired: Vec<(vk::Buffer, Allocation)>,
}

pub struct BlockEntityPipeline {
    pipeline: vk::Pipeline,
    chest_pipeline: vk::Pipeline,
    chest_frames: [ChestFrame; MAX_FRAMES_IN_FLIGHT],
    player_head_pipeline: vk::Pipeline,
    player_head_pool: vk::DescriptorPool,
    player_head_textures: HashMap<PlayerHeadProfileSource, TextureSlot>,
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
    display_pipelines: [vk::Pipeline; 2],
    display_frames: [TextDisplayFrame; MAX_FRAMES_IN_FLIGHT],
    text_sets_ready: [bool; MAX_FRAMES_IN_FLIGHT],
    text_layout: vk::PipelineLayout,
    text_set_layout: vk::DescriptorSetLayout,
    text_pool: vk::DescriptorPool,
    text_sets: Vec<vk::DescriptorSet>,
    text_buffers: Vec<vk::Buffer>,
    text_allocations: Vec<Allocation>,
    sign_vertices: Vec<SignVertex>,
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
        christmas_chests: bool,
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

        let chest_pipeline = create_pipeline(
            device,
            render_pass,
            pipeline_layout,
            BlendMode::Opaque,
            ModelInput::Instanced,
        );

        let player_head_pipeline = create_pipeline(
            device,
            render_pass,
            pipeline_layout,
            BlendMode::TranslucentDepthWrite,
            ModelInput::PushConstant,
        );
        // Separate from the static BE pool: at most one descriptor per CPU key.
        let player_head_pool = device
            .create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo {
                    flags: vk::DescriptorPoolCreateFlags::FreeDescriptorSet,
                    max_sets: MAX_HEAD_TEXTURES as u32,
                    pool_size_count: 1,
                    pool_sizes: &vk::DescriptorPoolSize {
                        ty: vk::DescriptorType::CombinedImageSampler,
                        descriptor_count: MAX_HEAD_TEXTURES as u32,
                    },
                    ..Default::default()
                },
                None,
            )
            .expect("placed head texture pool");

        let defs = kind_definitions(christmas_chests);
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
        let text_pipeline =
            create_sign_pipeline(device, render_pass, text_layout, WorldTextMode::Sign);
        let display_pipelines = [WorldTextMode::Display, WorldTextMode::SeeThrough]
            .map(|mode| create_sign_pipeline(device, render_pass, text_layout, mode));
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
            display_pipelines,
            display_frames: std::array::from_fn(|_| TextDisplayFrame::default()),
            text_sets_ready: [false; MAX_FRAMES_IN_FLIGHT],
            text_pipeline,
            text_layout,
            text_set_layout,
            text_pool,
            text_sets,
            text_buffers,
            text_allocations,
            sign_vertices: Vec::new(),
            pipeline,
            chest_pipeline,
            chest_frames: std::array::from_fn(|_| ChestFrame::default()),
            player_head_pipeline,
            player_head_pool,
            player_head_textures: HashMap::new(),
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

    /// Before frame command recording: publish this frame's CPU-ready sheets.
    /// Cache hits neither upload nor wait. Descriptors are immutable until
    /// freed.
    pub(in crate::renderer) fn update_player_head_textures(
        &mut self,
        device: &vk::Device,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
        allocator: &Arc<Mutex<Allocator>>,
        skins: &PlacedHeadSkinCache,
    ) {
        let expired: Vec<_> = self
            .player_head_textures
            .keys()
            .filter(|source| skins.skin(source).0 != *source)
            .cloned()
            .collect();
        if !expired.is_empty() {
            // ponytail: rare eviction waits for all in-flight users. Use fence-
            // retired slots only if measured churn warrants the extra state.
            device
                .wait_idle()
                .expect("wait before retiring placed head textures");
            for source in expired {
                let slot = self.player_head_textures.remove(&source).unwrap();
                destroy_head_texture(device, allocator, self.player_head_pool, slot);
            }
        }

        let mut uploads = Vec::new();
        let mut staging = Vec::new();
        for (source, skin) in skins.ready().chain(std::iter::once(
            skins.skin(&PlayerHeadProfileSource::Default),
        )) {
            if self.player_head_textures.contains_key(source) {
                continue;
            }
            // CPU admission and this dedicated pool share the same hard bound.
            if self.player_head_textures.len() >= MAX_HEAD_TEXTURES {
                break;
            }
            let slot = build_rgba_texture_slot(
                device,
                allocator,
                self.player_head_pool,
                self.texture_layout,
                self.texture_sampler,
                &skin.pixels,
                skin.width,
                skin.height,
                &mut uploads,
                &mut staging,
            );
            self.player_head_textures.insert(source.clone(), slot);
        }
        if uploads.is_empty() {
            return;
        }
        // ponytail: first-ready batches use the existing synchronous upload.
        // No steady-frame wait; switch to fence-retired staging if this stalls.
        util::upload_images_batched(device, queue, command_pool, &uploads);
        let mut alloc = allocator.lock().unwrap();
        for (buffer, allocation) in staging {
            device.destroy_buffer(buffer, None);
            alloc.free(allocation).ok();
        }
    }

    /// Caller must have waited for all in-flight frames, and must not have an
    /// unsubmitted command buffer referencing these slots (reload/teardown).
    pub(crate) fn invalidate_player_head_textures(
        &mut self,
        device: &vk::Device,
        allocator: &Arc<Mutex<Allocator>>,
    ) {
        for (_, slot) in self.player_head_textures.drain() {
            destroy_head_texture(device, allocator, self.player_head_pool, slot);
        }
    }

    pub fn update_camera(&mut self, frame: usize, uniform: &CameraUniform) {
        let bytes = bytemuck::bytes_of(uniform);
        self.camera_allocations[frame].mapped_slice_mut().unwrap()[..bytes.len()]
            .copy_from_slice(bytes);
    }

    pub(in crate::renderer) fn draw(
        &mut self,
        device: &vk::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        anchor: glam::DVec3,
        eye: glam::DVec3,
        player_eye: glam::DVec3,
        items: &[BlockEntityRenderInfo],
        head_skins: &PlacedHeadSkinCache,
        font: Option<(&GlyphMap, [vk::DescriptorImageInfo; 2])>,
        benchmark_timing: bool,
        allocator: &Arc<Mutex<Allocator>>,
    ) -> (f32, f32, u32, BlockEntityModelDrawCounts, u32) {
        if items.is_empty() {
            return (0.0, 0.0, 0, BlockEntityModelDrawCounts::default(), 0);
        }

        cmd.bind_pipeline(vk::PipelineBindPoint::Graphics, self.pipeline);
        // Model interval: per-item model/part processing through the final model
        // cmd.draw.
        let model_start = benchmark_timing.then(std::time::Instant::now);
        let mut model_draws = 0;
        let mut draws_by_kind = BlockEntityModelDrawCounts::default();
        let mut bound_pipeline = self.pipeline;

        let mut bound_entry: *const KindEntry = std::ptr::null();
        let mut bound_set: vk::DescriptorSet = vk::DescriptorSet::null();

        let order = chest_order(items);
        let mut iter = order.iter().copied().enumerate();
        while let Some((index, info)) = iter.next() {
            if benchmark_timing && info.kind == BlockEntityKind::Chest {
                draws_by_kind.chest_instances += 1;
            }
            let free = MAX_CHEST_INSTANCES.saturating_sub(self.chest_frames[frame].used);
            let run = if info.kind == BlockEntityKind::Chest
                && info.lid_open == 0.0
                && !self.chest_frames[frame].unavailable
            {
                let entry = &self.entries[&BlockEntityKind::Chest];
                let model = &entry.models[info.variant as usize % entry.models.len()];
                let parts = model
                    .part_ranges
                    .iter()
                    .filter(|(_, count)| *count > 0)
                    .count();
                chest_run(&order, index, if parts == 0 { 0 } else { free / parts })
            } else {
                0
            };
            if run > 0 {
                // Allocation is fallible. Never consume a run until its entire
                // instance payload has been safely written to this fence slot.
                if self.chest_frames[frame].buffer.is_none() {
                    match util::try_create_mapped_buffer(
                        device,
                        allocator,
                        &vec![0; MAX_CHEST_INSTANCES * size_of::<ChestInstance>()],
                        vk::BufferUsageFlags::VertexBuffer,
                        "closed_chest_instances",
                    ) {
                        Ok(buffer) => self.chest_frames[frame].buffer = Some(buffer),
                        Err(_) => self.chest_frames[frame].unavailable = true,
                    }
                }
                if let Some((buffer, allocation)) = &mut self.chest_frames[frame].buffer {
                    let entry = &self.entries[&BlockEntityKind::Chest];
                    let model = &entry.models[info.variant as usize % entry.models.len()];
                    let tex =
                        &entry.textures[(info.variant as usize).min(entry.textures.len() - 1)];
                    let first = self.chest_frames[frame].used;
                    let data = chest_instances(&order[index..index + run], model, anchor);
                    let bytes = bytemuck::cast_slice::<ChestInstance, u8>(&data);
                    let offset = first * size_of::<ChestInstance>();
                    allocation.mapped_slice_mut().unwrap()[offset..offset + bytes.len()]
                        .copy_from_slice(bytes);
                    self.chest_frames[frame].used += data.len();
                    if benchmark_timing {
                        draws_by_kind.chest_instances += (run - 1) as u32;
                        draws_by_kind.chest_batched += run as u32;
                    }
                    cmd.bind_pipeline(vk::PipelineBindPoint::Graphics, self.chest_pipeline);
                    cmd.bind_vertex_buffers(0, &[entry.vertex_buffer, *buffer], &[0, 0]);
                    cmd.bind_descriptor_sets(
                        vk::PipelineBindPoint::Graphics,
                        self.pipeline_layout,
                        0,
                        &[self.camera_sets[frame], tex.set],
                        &[],
                    );
                    let mut part_first = first as u32;
                    for &(start, count) in &model.part_ranges {
                        if count == 0 {
                            continue;
                        }
                        cmd.draw(count, run as u32, start, part_first);
                        part_first += run as u32;
                        if benchmark_timing {
                            model_draws += 1;
                            draws_by_kind.chest += 1;
                            draws_by_kind.closed_chest_candidate += 1;
                            draws_by_kind.chest_batch_draws += 1;
                        }
                    }
                    bound_pipeline = self.chest_pipeline;
                    bound_entry = std::ptr::null();
                    bound_set = vk::DescriptorSet::null();
                    for _ in 1..run {
                        iter.next();
                    }
                    continue;
                }
            }
            if benchmark_timing && closed_chest(info) {
                let next = order.get(index + 1).copied();
                // Three baked parts per chest; a batch needs at least two.
                if self.chest_frames[frame].unavailable || free < 6 {
                    draws_by_kind.chest_capacity_rejected += 1;
                } else if next.is_some_and(|other| {
                    closed_chest(other) && order.windows[index] != order.windows[index + 1]
                }) {
                    draws_by_kind.chest_overlap_rejected += 1;
                } else {
                    draws_by_kind.chest_run_boundary += 1;
                }
            }
            let is_statue = info.kind == BlockEntityKind::CopperGolemStatue;
            let entry = if is_statue {
                &self.copper_golem_statue
            } else if let Some(entry) = self.entries.get(&info.kind) {
                entry
            } else {
                continue;
            };
            let variant_idx = (info.variant as usize).min(entry.textures.len().saturating_sub(1));
            let fallback = &entry.textures[variant_idx];
            // The extractor marks only player heads with Some(Default/profile).
            // Resolve on every draw: A/B and a changed source cannot inherit
            // the preceding head's descriptor, including pending/failed keys.
            let is_player_head =
                info.kind == BlockEntityKind::Skull && info.player_head_profile_source.is_some();
            let slot = if is_player_head {
                let fallback = self
                    .player_head_textures
                    .get(&PlayerHeadProfileSource::Default)
                    .unwrap_or(fallback);
                head_skins.texture(
                    info.player_head_profile_source.as_ref(),
                    &self.player_head_textures,
                    fallback,
                )
            } else {
                fallback
            };
            let pipeline = if is_player_head {
                self.player_head_pipeline
            } else {
                self.pipeline
            };
            if bound_pipeline != pipeline {
                cmd.bind_pipeline(vk::PipelineBindPoint::Graphics, pipeline);
                bound_pipeline = pipeline;
            }

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
            } else if info.kind == BlockEntityKind::Skull {
                &entry.models[(info.variant as usize / 2).min(entry.models.len() - 1)]
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
                ModelConvention::BlockYUp => chest_matrix(info, anchor),
            };

            let mut model_mat = model_mat;
            if info.kind == BlockEntityKind::Skull && is_wall_skull_variant(info.variant) {
                model_mat = skull_wall_model_matrix(model_mat, info.yaw);
            }
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
                // No hurt flash or UV scroll. Player heads retain texture alpha
                // for both head and hat (shared shader discard + alpha blending).
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
                if benchmark_timing {
                    model_draws += 1;
                    let count = match info.kind {
                        BlockEntityKind::Chest => &mut draws_by_kind.chest,
                        BlockEntityKind::TrappedChest => &mut draws_by_kind.trapped_chest,
                        BlockEntityKind::EnderChest => &mut draws_by_kind.ender_chest,
                        BlockEntityKind::ShulkerBox => &mut draws_by_kind.shulker,
                        BlockEntityKind::Conduit => &mut draws_by_kind.conduit,
                        BlockEntityKind::CopperGolemStatue => {
                            &mut draws_by_kind.copper_golem_statue
                        }
                        BlockEntityKind::Skull => &mut draws_by_kind.skull,
                        _ => &mut draws_by_kind.other,
                    };
                    *count += 1;
                    if info.lid_open == 0.0
                        && matches!(
                            info.kind,
                            BlockEntityKind::Chest
                                | BlockEntityKind::TrappedChest
                                | BlockEntityKind::EnderChest
                        )
                    {
                        draws_by_kind.closed_chest_candidate += 1;
                    }
                }
            }
        }
        let model_ms = model_start.map_or(0.0, |start| start.elapsed().as_secs_f32() * 1000.0);
        // Sign interval: glyph generation, mapped-buffer copy, bindings and sign
        // cmd.draw.
        let sign_start = benchmark_timing.then(std::time::Instant::now);
        let mut sign_vertices = 0;
        if let Some((glyphs, textures)) = font {
            sign_vertices = sign_text::draw_sign_text(
                device,
                cmd,
                anchor,
                eye,
                player_eye,
                items,
                glyphs,
                textures,
                self.camera_sets[frame],
                self.text_pipeline,
                self.text_layout,
                self.text_sets[frame],
                self.text_buffers[frame],
                &mut self.text_allocations[frame],
                &mut self.text_sets_ready[frame],
                &mut self.sign_vertices,
            );
        }
        let sign_ms = sign_start.map_or(0.0, |start| start.elapsed().as_secs_f32() * 1000.0);
        (model_ms, sign_ms, model_draws, draws_by_kind, sign_vertices)
    }

    /// Update a shared set only before its first bind this frame. Updating even
    /// identical descriptors after binding would invalidate recorded commands.
    fn prepare_world_font(
        &mut self,
        device: &vk::Device,
        frame: usize,
        textures: [vk::DescriptorImageInfo; 2],
    ) {
        if self.text_sets_ready[frame] {
            return;
        }
        let writes = world_font_writes(self.text_sets[frame], &textures);
        device.update_descriptor_sets(&writes, &[]);
        self.text_sets_ready[frame] = true;
    }

    /// Exactly once after this slot's fence signals, before any draws. All
    /// atlas handles are fetched anew on the first draw, including after
    /// reload.
    pub(crate) fn begin_frame(
        &mut self,
        frame: usize,
        device: &vk::Device,
        allocator: &Arc<Mutex<Allocator>>,
    ) {
        let slot = &mut self.display_frames[frame];
        let mut alloc = allocator.lock().unwrap();
        for (buffer, allocation) in slot.retired.drain(..) {
            device.destroy_buffer(buffer, None);
            alloc.free(allocation).ok();
        }
        slot.used = 0;
        self.chest_frames[frame].used = 0;
        self.chest_frames[frame].unavailable = false;
        self.text_sets_ready[frame] = false;
    }

    /// Record ONE already-extracted display in the world render pass (B1).
    /// Call after begin_frame + update_camera, with current world_font()
    /// images; the atlas must stay unchanged throughout
    /// recording/submission. Renderer reload already waits idle. No atlas
    /// ownership or worker-thread GPU state. Repeated calls append, never
    /// overwrite sign text or earlier displays.
    pub(crate) fn draw_text_display(
        &mut self,
        device: &vk::Device,
        allocator: &Arc<Mutex<Allocator>>,
        cmd: vk::CommandBuffer,
        frame: usize,
        draw: &TextDisplayDraw,
        textures: [vk::DescriptorImageInfo; 2],
    ) -> Result<(), String> {
        let vertices = draw.gpu_vertices();
        if vertices.is_empty() {
            return Ok(());
        }
        let count =
            u32::try_from(vertices.len()).map_err(|_| "TextDisplay vertex count overflow")?;
        let bytes = bytemuck::cast_slice(&vertices);
        let slot = &mut self.display_frames[frame];
        let end = slot
            .used
            .checked_add(bytes.len())
            .ok_or("TextDisplay buffer size overflow")?;
        if end > slot.capacity {
            let capacity = end
                .checked_next_power_of_two()
                .ok_or("TextDisplay capacity overflow")?;
            let replacement = util::try_create_mapped_buffer(
                device,
                allocator,
                &vec![0; capacity],
                vk::BufferUsageFlags::VertexBuffer,
                "text_display_vertices",
            )?;
            if let Some(old) = slot.buffer.replace(replacement) {
                slot.retired.push(old);
            }
            slot.capacity = capacity;
            slot.used = 0;
        }
        let offset = slot.used;
        let (buffer, allocation) = slot.buffer.as_mut().unwrap();
        allocation.mapped_slice_mut().unwrap()[offset..offset + bytes.len()].copy_from_slice(bytes);
        let buffer = *buffer;
        slot.used += bytes.len();

        self.prepare_world_font(device, frame, textures);
        cmd.bind_pipeline(
            vk::PipelineBindPoint::Graphics,
            self.display_pipelines[usize::from(draw.see_through)],
        );
        cmd.bind_descriptor_sets(
            vk::PipelineBindPoint::Graphics,
            self.text_layout,
            0,
            &[self.camera_sets[frame], self.text_sets[frame]],
            &[],
        );
        cmd.bind_vertex_buffers(0, &[buffer], &[offset as u64]);
        // gpu_vertices orders background, shadow, glyphs for premultiplied blending.
        cmd.draw(count, 1, 0, 0);
        Ok(())
    }

    pub fn recreate_pipeline(&mut self, device: &vk::Device, render_pass: vk::RenderPass) {
        device.destroy_pipeline(self.pipeline, None);
        device.destroy_pipeline(self.chest_pipeline, None);
        self.chest_pipeline = create_pipeline(
            device,
            render_pass,
            self.pipeline_layout,
            BlendMode::Opaque,
            ModelInput::Instanced,
        );
        device.destroy_pipeline(self.player_head_pipeline, None);
        self.player_head_pipeline = create_pipeline(
            device,
            render_pass,
            self.pipeline_layout,
            BlendMode::TranslucentDepthWrite,
            ModelInput::PushConstant,
        );
        device.destroy_pipeline(self.text_pipeline, None);
        self.text_pipeline =
            create_sign_pipeline(device, render_pass, self.text_layout, WorldTextMode::Sign);
        for (pipeline, mode) in self
            .display_pipelines
            .iter_mut()
            .zip([WorldTextMode::Display, WorldTextMode::SeeThrough])
        {
            device.destroy_pipeline(*pipeline, None);
            *pipeline = create_sign_pipeline(device, render_pass, self.text_layout, mode);
        }
        self.pipeline = create_pipeline(
            device,
            render_pass,
            self.pipeline_layout,
            BlendMode::Opaque,
            ModelInput::PushConstant,
        );
    }

    pub fn destroy(&mut self, device: &vk::Device, allocator: &Arc<Mutex<Allocator>>) {
        self.invalidate_player_head_textures(device, allocator);
        device.destroy_descriptor_pool(self.player_head_pool, None);
        device.destroy_pipeline(self.player_head_pipeline, None);
        let mut alloc = allocator.lock().unwrap();
        for slot in &mut self.chest_frames {
            if let Some((buffer, allocation)) = slot.buffer.take() {
                device.destroy_buffer(buffer, None);
                alloc.free(allocation).ok();
            }
        }
        for slot in &mut self.display_frames {
            for (buffer, allocation) in slot.buffer.take().into_iter().chain(slot.retired.drain(..))
            {
                device.destroy_buffer(buffer, None);
                alloc.free(allocation).ok();
            }
        }
        for pipeline in self.display_pipelines {
            device.destroy_pipeline(pipeline, None);
        }
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
        device.destroy_pipeline(self.chest_pipeline, None);
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

    build_rgba_texture_slot(
        device,
        allocator,
        descriptor_pool,
        texture_layout,
        texture_sampler,
        &pixels,
        width,
        height,
        pending_uploads,
        staging_to_free,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_rgba_texture_slot(
    device: &vk::Device,
    allocator: &Arc<Mutex<Allocator>>,
    descriptor_pool: vk::DescriptorPool,
    texture_layout: vk::DescriptorSetLayout,
    texture_sampler: vk::Sampler,
    pixels: &[u8],
    width: u32,
    height: u32,
    pending_uploads: &mut Vec<util::PendingImageUpload>,
    staging_to_free: &mut Vec<(vk::Buffer, Allocation)>,
) -> TextureSlot {
    // Upload the full sheet unchanged, including transparent hat texels.
    let (image, view, allocation) =
        util::create_gpu_image(device, allocator, width, height, "block_entity_texture");
    let (staging_buf, staging_alloc) =
        util::create_staging_buffer(device, allocator, pixels, "block_entity_texture_staging");
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

fn destroy_head_texture(
    device: &vk::Device,
    allocator: &Arc<Mutex<Allocator>>,
    pool: vk::DescriptorPool,
    slot: TextureSlot,
) {
    device
        .free_descriptor_sets(pool, &[slot.set])
        .expect("free retired placed head descriptor");
    device.destroy_image_view(slot.view, None);
    device.destroy_image(slot.image, None);
    allocator.lock().unwrap().free(slot.allocation).ok();
}

#[derive(Clone, Copy)]
enum WorldTextMode {
    Sign,
    Display,
    SeeThrough,
}

impl WorldTextMode {
    fn depth(self) -> vk::PipelineDepthStencilStateCreateInfo<'static> {
        vk::PipelineDepthStencilStateCreateInfo {
            depth_test_enable: if matches!(self, Self::SeeThrough) {
                vk::FALSE
            } else {
                vk::TRUE
            },
            depth_write_enable: if matches!(self, Self::Sign) {
                vk::TRUE
            } else {
                vk::FALSE
            },
            depth_compare_op: vk::CompareOp::LessOrEqual,
            ..Default::default()
        }
    }
}

fn create_sign_pipeline(
    device: &vk::Device,
    render_pass: vk::RenderPass,
    layout: vk::PipelineLayout,
    mode: WorldTextMode,
) -> vk::Pipeline {
    let vs = shader::create_shader_module(device, shader::include_spirv!("sign_text.vert.spv"));
    let fs = shader::create_shader_module(
        device,
        match mode {
            WorldTextMode::Sign => shader::include_spirv!("sign_text.frag.spv"),
            _ => shader::include_spirv!("text_display.frag.spv"),
        },
    );
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
    let depth = mode.depth();
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

// Two atlas bindings are fixed; keep their writes on the stack per frame.
fn world_font_writes(
    set: vk::DescriptorSet,
    textures: &[vk::DescriptorImageInfo; 2],
) -> [vk::WriteDescriptorSet<'_>; 2] {
    std::array::from_fn(|binding| vk::WriteDescriptorSet {
        dst_set: set,
        dst_binding: binding as u32,
        descriptor_type: vk::DescriptorType::CombinedImageSampler,
        descriptor_count: 1,
        image_info: &textures[binding],
        ..Default::default()
    })
}

#[cfg(test)]
mod sign_text_tests {
    use super::*;

    fn chest(x: i32, variant: u32) -> BlockEntityRenderInfo {
        BlockEntityRenderInfo {
            pos: BlockPos::new(x, 64, -9),
            player_head_profile_source: None,
            kind: BlockEntityKind::Chest,
            statue_pose: None,
            yaw: 90.0,
            variant,
            lid_open: 0.0,
            sign_front: None,
            sign_back: None,
            sign_front_color: [0.0; 3],
            sign_front_glowing: false,
            sign_back_color: [0.0; 3],
            sign_back_glowing: false,
            sign_wall: false,
            sign_light: 0.0,
        }
    }

    #[test]
    fn closed_chest_instances_match_push_constants_and_part_draws() {
        let items = [chest(2, 0), chest(5, 0), chest(8, 0)];
        let anchor = glam::DVec3::new(1.25, 60.0, -12.5);
        let model = &block_entity_model::bake_chest_models()[0];
        let poses = model.compute_part_transforms(&PartAnim::default());
        let data = chest_instances(&items.iter().collect::<Vec<_>>(), model, anchor);
        let parts = model.part_ranges.iter().filter(|(_, n)| *n > 0).count();
        assert_eq!(parts, 3);
        assert_eq!(data.len(), parts * items.len());
        assert_eq!(
            chest_run(&chest_order(&items), 0, MAX_CHEST_INSTANCES / 3),
            3
        );
        // Old path: translation(center) * rotation(-yaw) * translation(-half)
        // then the static per-part pivot. Its 112-byte push block must match
        // each instanced vertex attribute exactly (including tint/overlay/UV).
        for (part, pose) in poses.iter().enumerate() {
            for (i, info) in items.iter().enumerate() {
                let center = (glam::DVec3::new(
                    info.pos.x as f64 + 0.5,
                    info.pos.y as f64,
                    info.pos.z as f64 + 0.5,
                ) - anchor)
                    .as_vec3();
                let old = glam::Mat4::from_translation(center)
                    * glam::Mat4::from_rotation_y((-info.yaw).to_radians())
                    * glam::Mat4::from_translation(glam::Vec3::new(-0.5, 0.0, -0.5))
                    * *pose;
                let instance = &data[part * items.len() + i];
                let mut push = [0u8; 112];
                push[..64].copy_from_slice(bytemuck::cast_slice(&old.to_cols_array()));
                push[64..80].copy_from_slice(bytemuck::cast_slice(&WHITE_TINT));
                push[80..96].copy_from_slice(bytemuck::cast_slice(&[0.0f32, 0.0, 0.0, 1.0]));
                assert_eq!(bytemuck::bytes_of(instance), &push);
            }
        }
        // Per-part vertex counts are unchanged; only the number of calls drops.
        let old_vertices: u32 = model
            .part_ranges
            .iter()
            .map(|(_, n)| n * items.len() as u32)
            .sum();
        let instanced_vertices: u32 = model.part_ranges.iter().map(|(_, n)| n * 3).sum();
        assert_eq!(old_vertices, instanced_vertices);
        assert_eq!(parts * items.len(), 9);
        assert_eq!(parts, 3);
    }

    #[test]
    fn double_chest_instances_match_old_draws_with_rebased_ranges() {
        let anchor = glam::DVec3::new(128.25, 60.0, -12.5);
        let baked = block_entity_model::bake_chest_models();
        for (variant, model_index) in [(4, 1), (5, 2)] {
            let mut model = baked[model_index].clone();
            // KindEntry rebases each model's part ranges into the shared vertex buffer.
            for (start, _) in &mut model.part_ranges {
                *start += 10_000;
            }
            assert!(model.part_ranges.iter().all(|(start, _)| *start >= 10_000));
            for yaw in [0.0, 180.0, 270.0] {
                let mut item = chest(7, variant);
                item.yaw = yaw;
                let pos = item.pos;
                let data = chest_instances(&[&item], &model, anchor);
                let poses = model.compute_part_transforms(&PartAnim::default());
                let mut instance_index = 0;
                for (part, (_, count)) in model.part_ranges.iter().enumerate() {
                    if *count == 0 {
                        continue;
                    }
                    let center =
                        (glam::DVec3::new(pos.x as f64 + 0.5, pos.y as f64, pos.z as f64 + 0.5)
                            - anchor)
                            .as_vec3();
                    let old = glam::Mat4::from_translation(center)
                        * glam::Mat4::from_rotation_y((-yaw).to_radians())
                        * glam::Mat4::from_translation(glam::Vec3::new(-0.5, 0.0, -0.5))
                        * poses[part];
                    let mut push = [0u8; 112];
                    push[..64].copy_from_slice(bytemuck::cast_slice(&old.to_cols_array()));
                    push[64..80].copy_from_slice(bytemuck::cast_slice(&WHITE_TINT));
                    push[80..96].copy_from_slice(bytemuck::cast_slice(&[0.0f32, 0.0, 0.0, 1.0]));
                    // Remaining 16 bytes are the old zero UV parameters.
                    assert_eq!(bytemuck::bytes_of(&data[instance_index]), &push);
                    instance_index += 1;
                }
                assert_eq!(instance_index, data.len());
            }
        }
    }

    #[test]
    fn chest_batch_first_instances_address_their_own_payload() {
        let model = &block_entity_model::bake_chest_models()[0];
        let items = [chest(0, 0), chest(3, 0), chest(6, 0), chest(9, 0)];
        let anchor = glam::DVec3::ZERO;
        let batch_len = 2;
        let first_batch = chest_instances(
            &items[..batch_len].iter().collect::<Vec<_>>(),
            model,
            anchor,
        );
        let second_batch_start = first_batch.len();
        let second_batch = chest_instances(
            &items[batch_len..].iter().collect::<Vec<_>>(),
            model,
            anchor,
        );
        let combined = [&first_batch[..], &second_batch[..]].concat();
        let mut part_first = 0;
        for (part, (_, count)) in model.part_ranges.iter().enumerate() {
            if *count == 0 {
                continue;
            }
            // Mirrors the draw loop's `firstInstance += run` for each part.
            let second_batch_first_instance = second_batch_start + part * batch_len;
            assert_eq!(second_batch_first_instance, second_batch_start + part_first);
            assert_eq!(
                bytemuck::cast_slice::<_, u8>(
                    &combined[second_batch_first_instance..second_batch_first_instance + batch_len]
                ),
                bytemuck::cast_slice::<_, u8>(
                    &second_batch[part * batch_len..(part + 1) * batch_len]
                )
            );
            part_first += batch_len;
        }
    }

    #[test]
    fn chest_run_fallback_predicate_preserves_items_when_unavailable_or_full() {
        let items = [chest(0, 0), chest(3, 0), chest(6, 0)];
        let parts = block_entity_model::bake_chest_models()[0]
            .part_ranges
            .iter()
            .filter(|(_, count)| *count > 0)
            .count();
        for (free, unavailable) in [(parts, false), (MAX_CHEST_INSTANCES, true)] {
            let mut iter = items.iter().enumerate();
            let (index, info) = iter.next().unwrap();
            let run = if !unavailable {
                chest_run(&chest_order(&items), index, free / parts)
            } else {
                0
            };
            assert_eq!(run, 0); // Caller takes the old per-part path without skipping.
            assert_eq!(iter.next().unwrap().1.pos, items[1].pos);
            assert_eq!(info.pos, items[0].pos);
        }
        assert_eq!(
            chest_run(&chest_order(&items), 0, (parts * 2 - 1) / parts),
            0
        );
    }

    #[test]
    fn chest_run_preserves_animated_mixed_and_capacity_fallbacks() {
        let mut items = [chest(0, 0), chest(3, 0), chest(6, 1), chest(9, 1)];
        assert_eq!(chest_run(&chest_order(&items), 0, 2), 2);
        assert_eq!(chest_run(&chest_order(&items), 2, 2), 2);
        assert_eq!(chest_run(&chest_order(&items), 0, 1), 0); // capacity: draw old path
        items[1].lid_open = 0.5;
        assert_eq!(chest_run(&chest_order(&items), 0, 4), 0);
        assert_eq!(chest_run(&chest_order(&items), 1, 4), 0);
        items[1].lid_open = 0.0;
        items[1].pos = BlockPos::new(1, 64, -9); // single has a gap: safe
        assert_eq!(chest_run(&chest_order(&items), 0, 4), 2);
        items[1].pos = BlockPos::new(3, 64, -9);
        items[1].kind = BlockEntityKind::TrappedChest;
        assert_eq!(chest_run(&chest_order(&items), 0, 4), 0);
    }

    #[test]
    fn single_chest_baked_bounds_support_adjacent_gap_rule() {
        let model = &block_entity_model::bake_chest_models()[0];
        let poses = model.compute_part_transforms(&PartAnim::default());
        for (yaw, expected) in [
            (0.0, [1.0, 15.0, 1.0, 16.0]),
            (90.0, [0.0, 15.0, 1.0, 15.0]),
            (180.0, [1.0, 15.0, 0.0, 15.0]),
            (270.0, [1.0, 16.0, 1.0, 15.0]),
        ] {
            let mut info = chest(0, 0);
            info.pos = BlockPos::new(0, 0, 0);
            info.yaw = yaw;
            let mut lo = glam::Vec3::splat(f32::INFINITY);
            let mut hi = glam::Vec3::splat(f32::NEG_INFINITY);
            for (part, &(start, count)) in model.part_ranges.iter().enumerate() {
                let transform = chest_matrix(&info, glam::DVec3::ZERO) * poses[part];
                for vertex in &model.vertices[start as usize..(start + count) as usize] {
                    let point = transform.transform_point3(glam::Vec3::from_array(vertex.position));
                    lo = lo.min(point);
                    hi = hi.max(point);
                }
            }
            for (actual, bound) in [lo.x, hi.x, lo.z, hi.z].into_iter().zip(expected) {
                assert!(
                    (actual * 16.0 - bound).abs() < 0.0001,
                    "{yaw}: {actual} vs {bound}"
                );
            }
            assert!(lo.y.abs() < 0.0001);
            assert!((hi.y * 16.0 - 14.0).abs() < 0.0001);
        }
    }

    #[test]
    fn chest_buckets_only_cross_proven_disjoint_closed_chests() {
        let items = [chest(0, 0), chest(3, 3), chest(6, 0), chest(9, 3)];
        let order = chest_order(&items);
        assert_eq!(
            order.iter().map(|c| c.variant).collect::<Vec<_>>(),
            [0, 0, 3, 3]
        );
        assert_eq!(chest_run(&order, 0, 2), 2);
        assert_eq!(chest_run(&order, 2, 2), 2);
        assert_eq!(chest_run(&order, 0, 1), 0); // full frame slot: no skip

        let mut adjacent = [chest(0, 0), chest(1, 3), chest(2, 0)];
        // Facing west: x extents [0,15]/16 for the first; facing west
        // on the next cell starts at 16/16, strictly separated.
        assert!(chests_disjoint(&adjacent[0], &adjacent[1]));
        assert_eq!(chest_run(&chest_order(&adjacent), 0, 3), 2);
        adjacent[0].yaw = 270.0; // lock reaches the shared x boundary
        adjacent[1].yaw = 90.0; // the other body starts at that boundary
        assert!(!chests_disjoint(&adjacent[0], &adjacent[1]));
        assert_eq!(chest_order(&adjacent)[0].variant, 0);
        assert_eq!(chest_order(&adjacent)[1].variant, 0);
        assert_eq!(chest_run(&chest_order(&adjacent), 0, 3), 0);
        adjacent[1].pos = adjacent[0].pos; // duplicate must not be batched
        adjacent[1].variant = 0;
        assert!(!chests_disjoint(&adjacent[0], &adjacent[1]));
        assert_eq!(chest_run(&chest_order(&adjacent), 0, 3), 0);
        adjacent[1].pos = BlockPos::new(1, 64, -9);
        adjacent[0].variant = 1; // double-left seam: never relax
        adjacent[1].variant = 2; // double-right still drawn separately
        assert!(!chests_disjoint(&adjacent[0], &adjacent[1]));
        let order = chest_order(&adjacent);
        assert_eq!(order.iter().map(|c| c.pos.x).collect::<Vec<_>>(), [0, 1, 2]);
        assert_eq!(order.len(), 3);
        adjacent[1].variant = 1;
        assert_eq!(chest_run(&chest_order(&adjacent), 0, 3), 0);

        let mut mixed = [chest(0, 0), chest(3, 3), chest(6, 0)];
        mixed[1].kind = BlockEntityKind::Skull;
        assert_eq!(
            chest_order(&mixed)
                .iter()
                .map(|c| c.pos.x)
                .collect::<Vec<_>>(),
            [0, 3, 6]
        );
        mixed[1].kind = BlockEntityKind::Chest;
        mixed[1].lid_open = 0.5;
        assert_eq!(
            chest_order(&mixed)
                .iter()
                .map(|c| c.pos.x)
                .collect::<Vec<_>>(),
            [0, 3, 6]
        );
    }

    #[test]
    fn chest_windows_bound_comparisons_and_keep_every_item() {
        // Alternating textures used to form one 5k-item window (12.5M
        // pair checks before bucketing). Include a larger 10k case too.
        for count in [5_000, 10_000] {
            let items: Vec<_> = (0..count)
                .map(|i| chest(i as i32 * 3, (i % 2) as u32 * 3))
                .collect();
            CHEST_COMPARISONS.with(|counter| counter.set(0));
            let order = chest_order(&items);
            assert_eq!(order.len(), count);
            let mut drawn = Vec::with_capacity(count);
            let mut index = 0;
            let mut batches = 0;
            while index < order.len() {
                // Simulate the caller: only skip items after a successful batch.
                let run = chest_run(&order, index, MAX_CHEST_INSTANCES / 3);
                let consumed = run.max(1); // run=0 uses the old draw path
                batches += usize::from(run > 0);
                drawn.extend(order[index..index + consumed].iter().map(|c| c.pos.x));
                index += consumed;
            }
            drawn.sort_unstable();
            assert_eq!(drawn, (0..count).map(|i| i as i32 * 3).collect::<Vec<_>>());
            assert_eq!(batches, count.div_ceil(MAX_CHEST_WINDOW) * 2);
            assert!(
                order
                    .windows
                    .chunks(MAX_CHEST_WINDOW)
                    .all(|chunk| { chunk.iter().all(|&window| window == chunk[0]) })
            );
            let comparisons = CHEST_COMPARISONS.with(|counter| counter.get());
            assert!(
                comparisons <= count * (MAX_CHEST_WINDOW - 1),
                "{count} chests: {comparisons} comparisons"
            );
        }
    }

    #[test]
    fn chest_window_boundary_preserves_order_and_fallbacks() {
        let mut items: Vec<_> = (0..MAX_CHEST_WINDOW + 5)
            .map(|i| chest(i as i32 * 3, 0))
            .collect();
        // Even across the cap, no run may consume a chest from the next window.
        let order = chest_order(&items);
        assert_eq!(order.windows[MAX_CHEST_WINDOW - 1], 0);
        assert_eq!(order.windows[MAX_CHEST_WINDOW], MAX_CHEST_WINDOW);
        assert_eq!(chest_run(&order, MAX_CHEST_WINDOW - 1, 10), 0);
        assert_eq!(chest_run(&order, MAX_CHEST_WINDOW, 1), 0);
        // Duplicates, doubles, open chests, and heads must remain at their
        // original barriers, even when a preceding window fills the cap.
        items[MAX_CHEST_WINDOW].pos = items[MAX_CHEST_WINDOW - 1].pos;
        items[MAX_CHEST_WINDOW + 1].variant = 1;
        items[MAX_CHEST_WINDOW + 1].pos = items[MAX_CHEST_WINDOW].pos;
        items[MAX_CHEST_WINDOW + 2].lid_open = 0.5;
        items[MAX_CHEST_WINDOW + 3].kind = BlockEntityKind::Skull;
        let order = chest_order(&items);
        for i in MAX_CHEST_WINDOW - 1..=MAX_CHEST_WINDOW + 3 {
            assert!(std::ptr::eq(order[i], &items[i]));
        }
        for i in MAX_CHEST_WINDOW..=MAX_CHEST_WINDOW + 3 {
            assert_eq!(chest_run(&order, i, 10), 0);
        }
        assert_eq!(order.len(), items.len());
    }

    #[test]
    fn world_font_bindings_match_previous_descriptor_writes() {
        let textures = [
            vk::DescriptorImageInfo {
                image_layout: vk::ImageLayout::General,
                ..Default::default()
            },
            vk::DescriptorImageInfo {
                image_layout: vk::ImageLayout::ShaderReadOnlyOptimal,
                ..Default::default()
            },
        ];
        let set = vk::DescriptorSet::null();
        let writes = world_font_writes(set, &textures);
        let previous: Vec<_> = textures
            .iter()
            .enumerate()
            .map(|(binding, image)| vk::WriteDescriptorSet {
                dst_set: set,
                dst_binding: binding as u32,
                descriptor_type: vk::DescriptorType::CombinedImageSampler,
                descriptor_count: 1,
                image_info: image,
                ..Default::default()
            })
            .collect();
        for (write, old) in writes.iter().zip(&previous) {
            assert_eq!(write.dst_set, old.dst_set);
            assert_eq!(write.dst_binding, old.dst_binding);
            assert_eq!(write.descriptor_type, old.descriptor_type);
            assert_eq!(write.descriptor_count, old.descriptor_count);
            assert_eq!(write.image_info, old.image_info);
            assert_eq!(write.image_info, &textures[write.dst_binding as usize]);
        }
    }

    #[test]
    fn text_display_depth_and_shader_policy_does_not_change_signs() {
        for (mode, test, write) in [
            (WorldTextMode::Sign, vk::TRUE, vk::TRUE),
            (WorldTextMode::Display, vk::TRUE, vk::FALSE),
            (WorldTextMode::SeeThrough, vk::FALSE, vk::FALSE),
        ] {
            let depth = mode.depth();
            assert_eq!(depth.depth_test_enable, test);
            assert_eq!(depth.depth_write_enable, write);
            assert_eq!(depth.depth_compare_op, vk::CompareOp::LessOrEqual);
        }
        assert_eq!(size_of::<SignVertex>(), 44);
        let shader = include_str!("../shaders/text_display.frag");
        let solid = shader.find("if (v_colored < 0)").unwrap();
        let returned = shader.find("return;").unwrap();
        let sampled = shader.find("texture(").unwrap();
        assert!(solid < returned && returned < sampled);
        assert!(shader.contains("vec4(v_color.rgb * v_color.a, v_color.a)"));
        assert!(shader.contains("float alpha = tex.a * v_color.a;"));
        assert!(shader.contains("vec4(tex.rgb * v_color.rgb * alpha, alpha)"));
        // Signs intentionally retain their existing shader and depth writes.
        assert!(
            include_str!("../shaders/sign_text.frag")
                .contains("vec4(tex.rgb * v_color.rgb * tex.a, tex.a * v_color.a)")
        );
    }

    #[test]
    fn chest_texture_policy_is_frozen_for_renderer_session() {
        for (day, seasonal) in [(23, false), (24, true), (26, true), (27, false)] {
            assert_eq!(christmas_on(time::Month::December, day), seasonal);
            let defs = kind_definitions(seasonal);
            for kind in [BlockEntityKind::Chest, BlockEntityKind::TrappedChest] {
                let textures = defs
                    .iter()
                    .find(|def| def.kind == kind)
                    .unwrap()
                    .tex_variants;
                assert_eq!(
                    textures,
                    if seasonal {
                        CHEST_XMAS_TEXTURES
                    } else if kind == BlockEntityKind::Chest {
                        CHEST_TEXTURES
                    } else {
                        TRAPPED_CHEST_TEXTURES
                    }
                );
            }
        }
        assert!(!christmas_on(time::Month::November, 25));
        // Midnight doesn't reselect the already-built BE textures in either direction.
        let before_christmas = christmas_on(time::Month::December, 23);
        assert!(christmas_on(time::Month::December, 24));
        assert_eq!(
            kind_definitions(before_christmas)[0].tex_variants,
            CHEST_TEXTURES
        );
        let session = christmas_on(time::Month::December, 26);
        assert!(!christmas_on(time::Month::December, 27));
        assert_eq!(
            kind_definitions(session)[0].tex_variants,
            CHEST_XMAS_TEXTURES
        );
    }

    #[test]
    fn conduit_and_player_head_have_idle_geometry_and_textures() {
        for kind in [BlockEntityKind::Conduit, BlockEntityKind::Skull] {
            let definition = kind_definitions(false)
                .into_iter()
                .find(|d| d.kind == kind)
                .unwrap();
            assert!(!definition.models[0].vertices.is_empty());
            assert!(!definition.tex_variants.is_empty());
        }
    }

    #[test]
    fn skull_variants_pair_textures_models_and_wall_forms() {
        let cases = [
            (
                "skeleton_skull",
                0,
                "minecraft/textures/entity/skeleton/skeleton.png",
            ),
            (
                "skeleton_wall_skull",
                1,
                "minecraft/textures/entity/skeleton/skeleton.png",
            ),
            (
                "wither_skeleton_skull",
                2,
                "minecraft/textures/entity/skeleton/wither_skeleton.png",
            ),
            (
                "wither_skeleton_wall_skull",
                3,
                "minecraft/textures/entity/skeleton/wither_skeleton.png",
            ),
            (
                "zombie_head",
                4,
                "minecraft/textures/entity/zombie/zombie.png",
            ),
            (
                "zombie_wall_head",
                5,
                "minecraft/textures/entity/zombie/zombie.png",
            ),
            (
                "creeper_head",
                6,
                "minecraft/textures/entity/creeper/creeper.png",
            ),
            (
                "creeper_wall_head",
                7,
                "minecraft/textures/entity/creeper/creeper.png",
            ),
            (
                "player_head",
                8,
                "minecraft/textures/entity/player/slim/steve.png",
            ),
            (
                "player_wall_head",
                9,
                "minecraft/textures/entity/player/slim/steve.png",
            ),
        ];
        crate::world::block::init("26.2");
        let props =
            crate::world::block::block_properties(crate::world::block::find_state("stone", &[]));
        let def = kind_definitions(false)
            .into_iter()
            .find(|d| d.kind == BlockEntityKind::Skull)
            .unwrap();
        assert_eq!(def.models.len(), 6);
        assert_eq!(def.tex_variants.len(), 12);
        for (name, expected, texture) in cases {
            let variant = variant_for_block(BlockEntityKind::Skull, name, props) as usize;
            assert_eq!(variant, expected, "{name}");
            assert_eq!(def.tex_variants[variant][0], texture, "{name}");
            assert_eq!(is_wall_skull_variant(variant as u32), expected % 2 == 1);
            let model_index = variant / 2;
            if expected < 8 {
                assert!(!def.models[model_index].vertices.is_empty(), "{name}");
            }
        }
        assert_eq!(def.models[4].vertices.len(), 72); // player head retains its hat
        assert_eq!(def.models[5].vertices.len(), 0); // dragon/piglin not faked
        assert_eq!(skull_variant("dragon_head"), 10);
        assert_eq!(skull_variant("piglin_wall_head"), 11);
    }

    #[test]
    fn skull_wall_transform_keeps_facing_offset_and_rotation_conventions() {
        use glam::Vec3;
        let north = skull_wall_model_matrix(glam::Mat4::IDENTITY, 0.0).transform_point3(Vec3::ZERO);
        assert!((north - Vec3::new(0.0, 0.25, -0.25)).length() < 1e-6);
        let east = skull_wall_model_matrix(glam::Mat4::IDENTITY, 90.0).transform_point3(Vec3::ZERO);
        assert!((east - Vec3::new(-0.25, 0.25, 0.0)).length() < 1e-6);

        crate::world::block::init("26.2");
        let standing = crate::world::block::find_state("skeleton_skull", &[("rotation", "8")]);
        assert_eq!(
            yaw_for_block(
                BlockEntityKind::Skull,
                crate::world::block::block_properties(standing)
            ),
            180.0
        );
        let wall = crate::world::block::find_state("skeleton_wall_skull", &[("facing", "north")]);
        assert_eq!(
            yaw_for_block(
                BlockEntityKind::Skull,
                crate::world::block::block_properties(wall)
            ),
            180.0
        );
        assert!(is_wall_skull_variant(skull_variant("skeleton_wall_skull")));
        assert!(!is_wall_skull_variant(skull_variant("skeleton_skull")));
    }

    #[test]
    fn sign_board_geometry_is_not_drawn_as_block_entity_geometry() {
        assert!(
            kind_definitions(false)
                .iter()
                .all(|definition| definition.kind != BlockEntityKind::Sign)
        );
    }
}
