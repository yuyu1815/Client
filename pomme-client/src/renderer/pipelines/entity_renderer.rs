use std::collections::HashMap;
use std::path::Path;
use std::slice;
use std::sync::{Arc, Mutex};

use azalea_registry::builtin::EntityKind;
use pomme_gpu_allocator::vulkan::{Allocation, Allocator};
use pyronyx::vk;

use crate::assets::{AssetIndex, resolve_asset_path};
use crate::entity::components::Position;
use crate::renderer::camera::CameraUniform;
use crate::renderer::chunk::mesher::ChunkVertex;
use crate::renderer::entity_model::BakedEntityModel;
use crate::renderer::{MAX_FRAMES_IN_FLIGHT, entity_model, entity_models, shader, util};

pub const MAX_OVERLAYS: usize = 4;

fn flip_degrees(kind: EntityKind) -> f32 {
    match kind {
        EntityKind::Spider
        | EntityKind::CaveSpider
        | EntityKind::Endermite
        | EntityKind::Silverfish => 180.0,
        _ => 90.0,
    }
}

fn mob_scale(kind: EntityKind) -> f32 {
    match kind {
        EntityKind::Giant => 6.0,
        EntityKind::CaveSpider => 0.7,
        _ => 1.0,
    }
}

fn death_fall_degrees(death_time: f32, kind: EntityKind) -> f32 {
    if death_time <= 0.0 || matches!(kind, EntityKind::Squid | EntityKind::GlowSquid) {
        return 0.0;
    }
    (((death_time - 1.0) / 20.0 * 1.6).sqrt()).min(1.0) * flip_degrees(kind)
}

fn end_crystal_y(age: f32) -> f32 {
    let h = (age * 0.2).sin() * 0.5 + 0.5;
    (h * h + h) * 0.4 - 1.4
}

/// Per-frame instance buffer capacity, in (entity, part) draws. Far above any
/// realistic on-screen entity count; excess is dropped with a warning.
const MAX_INSTANCES: usize = 16384;
const MAX_PLAYER_SKINS: usize = 128;

/// Per-instance data for one (entity, part) draw, fed as instance-rate vertex
/// attributes (binding 1) — the four model-matrix columns, tint, overlay, uv.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct EntityInstance {
    model: [[f32; 4]; 4],
    tint: [f32; 4],
    overlay_color: [f32; 4],
    uv_params: [f32; 4],
}

pub struct EntityRenderInfo {
    /// Interpolated/visually offset position for the model and frustum.
    pub position: Position,
    /// Current simulation position for vanilla's strict distance check.
    pub simulation_position: Position,
    pub head_x_rot_deg: f32,
    pub head_y_rot_deg: f32,
    pub body_y_rot_deg: f32,
    pub is_baby: bool,
    pub is_crouching: bool,
    pub shulker_peek: f32,
    pub shulker_attach_face: azalea_core::direction::Direction,
    pub is_sleeping: bool,
    /// Vanilla `sleepDirectionToRotation` result; absent when bed facing is
    /// unavailable.
    pub sleeping_yaw_deg: Option<f32>,
    pub walk_anim_pos: f32,
    pub walk_anim_speed: f32,
    pub entity_kind: EntityKind,
    pub player_uuid: Option<uuid::Uuid>,
    pub is_invisible: bool,
    /// Effective vanilla player-model visibility bits; non-players use native
    /// defaults.
    pub skin_parts_mask: u8,
    pub variant_index: u32,
    pub armor_stand_flags: u8,
    /// head, body, left/right arm, left/right leg; native Euler degrees.
    pub armor_stand_pose: [[f32; 3]; 6],
    /// Retained stacks for a later armor/hand equipment layer.
    pub armor_stand_equipment: std::collections::HashMap<
        azalea_inventory::components::EquipmentSlot,
        azalea_inventory::ItemStack,
    >,
    /// BODY equipment snapshot for Happy Ghast's native equipment layer.
    pub body_equipment: azalea_inventory::ItemStack,
    /// Happy Ghast is ridden when it has a passenger (not when saddle is
    /// present).
    pub is_ridden: bool,
    pub overlay_tints: [Option<[f32; 4]>; MAX_OVERLAYS],
    /// Per-slot overlay texture variant (villager type/profession/level).
    pub overlay_variants: [u32; MAX_OVERLAYS],
    /// Villager head-shake (unhappy counter > 0).
    pub is_unhappy: bool,
    pub head_y_offset: f32,
    pub head_x_rot_deg_override: Option<f32>,
    pub has_red_overlay: bool,
    pub death_time: f32,
    /// Mob is targeting/attacking — raises zombie/skeleton arms.
    pub aggressive: bool,
    /// Chicken wing-flap phase and 0..1 amplitude, interpolated.
    pub flap: f32,
    pub flap_speed: f32,
    /// Enderman screaming state — raises the head.
    pub is_creepy: bool,
    /// Zombie-family conversion — shakes the whole body.
    pub is_converting: bool,
    /// Witch drinking. Driven by the using-item metadata flag rather than
    /// vanilla's `isHoldingItem` (main-hand item check) — pomme tracks no
    /// mob equipment; the two only diverge for command-equipped witches.
    pub is_holding_item: bool,
    /// Witch per-entity nose-wobble rate, resolved from the entity id.
    pub nose_wobble_speed: f32,
    /// Tamable sitting pose (wolf/cat).
    pub is_sitting: bool,
    pub is_sprinting: bool,
    /// Wolf anger — angry face texture is picked upstream; this pins the tail.
    pub is_angry: bool,
    /// Wolf tail pitch (vanilla `getTailAngle`), radians.
    pub tail_angle: f32,
    /// Wolf beg head tilt, radians, interpolated.
    pub head_roll_angle: f32,
    /// Wolf wet-shake progress 0..2, interpolated.
    pub shake_anim: f32,
    /// Cat lie-down / relax springs, interpolated.
    pub lie_down_amount: f32,
    pub lie_down_amount_tail: f32,
    pub relax_state_one_amount: f32,
    /// Rabbit hop keyframe clock, seconds since the hop started.
    pub hop_elapsed_secs: Option<f32>,
    /// Equine grass-eat / rear-up / feeding springs, interpolated.
    pub eat_anim: f32,
    pub stand_anim: f32,
    pub feeding_anim: f32,
    /// Equine tail swish (client-local RNG counter).
    pub animate_tail: bool,
    /// Fish flop pose / squid body branch.
    pub is_in_water: bool,
    pub is_on_ground: bool,
    pub boat_rowing_time: [f32; 2],
    /// Native AbstractBoat status: bounding-box top is submerged in water.
    pub boat_underwater: bool,
    /// Squid tentacle stroke angle, interpolated.
    pub tentacle_angle: f32,
    /// Bat pose flag + its fly/rest animation clock.
    pub bat_resting: bool,
    pub bat_elapsed_secs: Option<f32>,
    /// Iron golem countdowns; the punch one is partial-tick adjusted.
    pub golem_attack_ticks: f32,
    pub golem_offer_flower_ticks: u32,
    /// Base-model tint (wolf wet-shade grayscale); white for everyone else.
    pub base_tint: [f32; 4],
    /// Extra scale applied after the entity rotation (slime size + squish),
    /// shared by base and overlay draws.
    pub body_transform: Option<glam::Mat4>,
    /// Camera orientation for native camera-facing projectile quads.
    pub camera_orientation: Option<glam::Quat>,
    /// Native Trident foil flag; only this projectile uses the entity-glint
    /// pass.
    pub projectile_foil: bool,
    /// Interpolated entity age in ticks; drives entity animations/projectiles.
    pub age_in_ticks: f32,
    /// Per-entity deterministic phase used by vanilla's Phantom flap clock.
    pub animation_phase: f32,
    /// Arm-swing progress 0..1; drives the zombie attack swing.
    pub attack_time: f32,
    pub vex_charging: bool,
    /// Skip frustum/distance culling (the 3rd-person self entity, which sits at
    /// the camera and must never blink out).
    pub skip_cull: bool,
}

/// Everything inert: mob-family animation inputs zeroed, no overlays, white
/// tint. Construction sites spell out only the fields that apply to them.
impl Default for EntityRenderInfo {
    fn default() -> Self {
        Self {
            position: Position::new(0.0, 0.0, 0.0),
            simulation_position: Position::new(0.0, 0.0, 0.0),
            head_x_rot_deg: 0.0,
            head_y_rot_deg: 0.0,
            body_y_rot_deg: 0.0,
            is_baby: false,
            is_crouching: false,
            shulker_peek: 0.0,
            shulker_attach_face: azalea_core::direction::Direction::Down,
            is_sleeping: false,
            sleeping_yaw_deg: None,
            walk_anim_pos: 0.0,
            walk_anim_speed: 0.0,
            entity_kind: EntityKind::Player,
            player_uuid: None,
            is_invisible: false,
            skin_parts_mask: 0x7f,
            variant_index: 0,
            armor_stand_flags: 0,
            armor_stand_pose: [[0.0; 3]; 6],
            armor_stand_equipment: std::collections::HashMap::new(),
            body_equipment: azalea_inventory::ItemStack::Empty,
            is_ridden: false,
            overlay_tints: [None; MAX_OVERLAYS],
            overlay_variants: [0; MAX_OVERLAYS],
            is_unhappy: false,
            head_y_offset: 0.0,
            head_x_rot_deg_override: None,
            has_red_overlay: false,
            death_time: 0.0,
            aggressive: false,
            flap: 0.0,
            flap_speed: 0.0,
            is_creepy: false,
            is_converting: false,
            is_holding_item: false,
            nose_wobble_speed: 0.0,
            is_sitting: false,
            is_sprinting: false,
            is_angry: false,
            tail_angle: 0.0,
            head_roll_angle: 0.0,
            shake_anim: 0.0,
            lie_down_amount: 0.0,
            lie_down_amount_tail: 0.0,
            relax_state_one_amount: 0.0,
            hop_elapsed_secs: None,
            eat_anim: 0.0,
            stand_anim: 0.0,
            feeding_anim: 0.0,
            animate_tail: false,
            is_in_water: false,
            is_on_ground: false,
            boat_rowing_time: [0.0; 2],
            boat_underwater: false,
            tentacle_angle: 0.0,
            bat_resting: false,
            bat_elapsed_secs: None,
            golem_attack_ticks: 0.0,
            golem_offer_flower_ticks: 0,
            base_tint: WHITE_TINT,
            body_transform: None,
            camera_orientation: None,
            projectile_foil: false,
            age_in_ticks: 0.0,
            animation_phase: 0.0,
            attack_time: 0.0,
            vex_charging: false,
            skip_cull: false,
        }
    }
}

/// How an overlay layer is blended. Base/baby variants are always `Opaque`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OverlayKind {
    /// Cutout, depth-writing — sheep wool and all base models.
    Opaque,
    /// `Opaque` with backface culling (vanilla `entityCutoutCull`) — meshes
    /// with coplanar zero-depth quads (bat wings).
    OpaqueCulled,
    /// Translucent, depth-writing — the slime shell (vanilla
    /// `entityTranslucent`; the alpha lives in the texture).
    BodyTranslucent,
    /// Breeze wind texture scrolls horizontally at 0.02 UV/tick.
    WindScroll,
    /// Translucent, full-bright, depth-write off — spider glowing eyes.
    EyesTranslucent,
    /// ShulkerBullet's enlarged alpha-0.15 copy.
    ShulkerBulletOverlay,
    /// Additive, full-bright, depth-writing, scrolling UV — charged creeper
    /// swirl.
    SwirlAdditive,
    /// Vanilla entity glint material for the foiled Trident second pass.
    TridentGlint,
}

struct MobVariant {
    model: BakedEntityModel,
    vertex_buffer: vk::Buffer,
    vertex_allocation: Allocation,
    texture_image: vk::Image,
    texture_view: vk::ImageView,
    texture_allocation: Allocation,
    texture_set: vk::DescriptorSet,
    overlay_kind: OverlayKind,
    /// Overlay whose part poses (pivots/rotations/scales) differ from the
    /// base model's, so its part transforms can't be shared with the base
    /// (stray/bogged clothing: humanoid ±1.9 legs over skeleton ±2.0).
    own_pivots: bool,
}

struct MobEntry {
    adult_variants: Vec<MobVariant>,
    baby_variants: Option<Vec<MobVariant>>,
    /// Overlay slots, each with its own texture variants
    /// (`overlay_variants[slot]` picks one).
    adult_overlays: Vec<Vec<MobVariant>>,
    baby_overlays: Vec<Vec<MobVariant>>,
    anim: AnimationType,
}

struct PlayerSkinTexture {
    image: vk::Image,
    view: vk::ImageView,
    allocation: Allocation,
    set: vk::DescriptorSet,
    cape: Option<PlayerCapeTexture>,
    cape_set: Option<vk::DescriptorSet>,
    slim: bool,
}

struct PlayerCapeTexture {
    image: vk::Image,
    view: vk::ImageView,
    allocation: Allocation,
}

impl MobEntry {
    fn base_variant(&self, is_baby: bool, variant_index: u32) -> &MobVariant {
        let pool = if is_baby {
            self.baby_variants.as_ref().unwrap_or(&self.adult_variants)
        } else {
            &self.adult_variants
        };
        let idx = (variant_index as usize).min(pool.len().saturating_sub(1));
        &pool[idx]
    }

    fn overlays(&self, is_baby: bool) -> &[Vec<MobVariant>] {
        if is_baby {
            &self.baby_overlays
        } else {
            &self.adult_overlays
        }
    }

    fn overlay_variant(&self, is_baby: bool, slot: usize, variant_index: u32) -> &MobVariant {
        let pool = &self.overlays(is_baby)[slot];
        let idx = (variant_index as usize).min(pool.len().saturating_sub(1));
        &pool[idx]
    }
}

pub const WHITE_TINT: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

/// Each mob's flattened variant pool by registry path; the net handler
/// resolves synced registry entries by name against these same slices (so
/// their order is pomme's, not the protocol id's), and the renderer
/// constructor asserts the pools line up.
pub const CHICKEN_VARIANT_ORDER: &[&str] = &["temperate", "warm", "cold"];
pub const COW_VARIANT_ORDER: &[&str] = &["temperate", "cold", "warm"];
pub const PIG_VARIANT_ORDER: &[&str] = &["temperate", "cold", "warm"];
/// Wolf pool interleaves 3 state textures (wild/tame/angry) per variant.
pub const WOLF_VARIANT_ORDER: &[&str] = &[
    "pale", "spotted", "snowy", "black", "ashen", "rusty", "woods", "chestnut", "striped",
];
pub const CAT_VARIANT_ORDER: &[&str] = &[
    "all_black",
    "black",
    "british_shorthair",
    "calico",
    "jellie",
    "persian",
    "ragdoll",
    "red",
    "siamese",
    "tabby",
    "white",
];

/// Pool length the `*_VARIANT_ORDER` slice implies for mobs whose variant
/// index comes from a synced registry.
fn expected_variant_count(kind: EntityKind) -> Option<usize> {
    match kind {
        EntityKind::Chicken => Some(CHICKEN_VARIANT_ORDER.len()),
        EntityKind::Cow => Some(COW_VARIANT_ORDER.len()),
        EntityKind::Pig => Some(PIG_VARIANT_ORDER.len()),
        EntityKind::Wolf => Some(WOLF_VARIANT_ORDER.len() * 3),
        EntityKind::Cat => Some(CAT_VARIANT_ORDER.len()),
        _ => None,
    }
}

/// Vanilla `OverlayTexture` hurt pixel (ARGB 0xB2FF0000): rgb is the overlay
/// color, `a` is how much of the base color survives the mix.
const HURT_OVERLAY: [f32; 4] = [1.0, 0.0, 0.0, 178.0 / 255.0];
const NO_OVERLAY: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

pub const WOOL_COLOR_RGBA: [[f32; 4]; 16] = [
    rgb(0xF0F0F0), // 0 white
    rgb(0xEB8844), // 1 orange
    rgb(0xC354CD), // 2 magenta
    rgb(0x6689D3), // 3 light_blue
    rgb(0xDECF2A), // 4 yellow
    rgb(0x41CD34), // 5 lime
    rgb(0xD88198), // 6 pink
    rgb(0x434343), // 7 gray
    rgb(0xABABAB), // 8 light_gray
    rgb(0x287697), // 9 cyan
    rgb(0x7B2FBE), // 10 purple
    rgb(0x253192), // 11 blue
    rgb(0x51301A), // 12 brown
    rgb(0x3B511A), // 13 green
    rgb(0xB3312C), // 14 red
    rgb(0x1E1B1B), // 15 black
];

const fn rgb(hex: u32) -> [f32; 4] {
    let r = ((hex >> 16) & 0xFF) as f32 / 255.0;
    let g = ((hex >> 8) & 0xFF) as f32 / 255.0;
    let b = (hex & 0xFF) as f32 / 255.0;
    [r, g, b, 1.0]
}

pub fn wool_color_tint(color: u8) -> [f32; 4] {
    WOOL_COLOR_RGBA[(color & 0x0F) as usize]
}

/// Vanilla `DyeColor.getTextureDiffuseColor` — the modern dye table used by
/// collar layers (`WOOL_COLOR_RGBA` above is the legacy wool table).
pub const DYE_COLOR_RGBA: [[f32; 4]; 16] = [
    rgb(0xF9FFFE), // 0 white
    rgb(0xF9801D), // 1 orange
    rgb(0xC74EBD), // 2 magenta
    rgb(0x3AB3DA), // 3 light_blue
    rgb(0xFED83D), // 4 yellow
    rgb(0x80C71F), // 5 lime
    rgb(0xF38BAA), // 6 pink
    rgb(0x474F52), // 7 gray
    rgb(0x9D9D97), // 8 light_gray
    rgb(0x169C9C), // 9 cyan
    rgb(0x8932B8), // 10 purple
    rgb(0x3C44AA), // 11 blue
    rgb(0x835432), // 12 brown
    rgb(0x5E7C16), // 13 green
    rgb(0xB02E26), // 14 red
    rgb(0x1D1D21), // 15 black
];

/// Out-of-range ids are white (vanilla `DyeColor.byId`).
pub fn dye_color_tint(color: u8) -> [f32; 4] {
    DYE_COLOR_RGBA
        .get(color as usize)
        .copied()
        .unwrap_or(DYE_COLOR_RGBA[0])
}

pub fn jeb_sheep_tint(entity_id: i32, age_in_ticks: u32) -> [f32; 4] {
    let base = (age_in_ticks / 25).wrapping_add(entity_id as u32);
    let c1 = (base % 16) as usize;
    let c2 = ((base + 1) % 16) as usize;
    let t = (age_in_ticks % 25) as f32 / 25.0;
    let a = WOOL_COLOR_RGBA[c1];
    let b = WOOL_COLOR_RGBA[c2];
    [
        a[0] * (1.0 - t) + b[0] * t,
        a[1] * (1.0 - t) + b[1] * t,
        a[2] * (1.0 - t) + b[2] * t,
        1.0,
    ]
}

pub struct EntityRenderer {
    pipeline: vk::Pipeline,
    /// Opaque with backface culling — bat wings.
    culled_pipeline: vk::Pipeline,
    /// Translucent, depth-writing — slime shell.
    body_translucent_pipeline: vk::Pipeline,
    /// Translucent, depth-write off — spider eyes.
    eyes_pipeline: vk::Pipeline,
    /// Additive, depth-writing — charged-creeper energy swirl.
    swirl_pipeline: vk::Pipeline,
    glint_pipeline: vk::Pipeline,
    water_patch_depth_pipeline: vk::Pipeline,
    water_patch_vertex_buffer: vk::Buffer,
    water_patch_vertex_allocation: Allocation,
    water_patch_range: (u32, u32),
    water_patch_part_transform: glam::Mat4,
    pipeline_layout: vk::PipelineLayout,
    camera_layout: vk::DescriptorSetLayout,
    texture_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    camera_sets: Vec<vk::DescriptorSet>,
    camera_buffers: Vec<vk::Buffer>,
    camera_allocations: Vec<Allocation>,
    /// Per-instance vertex buffer (bound at binding 1), one per frame in
    /// flight.
    instance_buffers: Vec<vk::Buffer>,
    instance_allocations: Vec<Allocation>,
    texture_sampler: vk::Sampler,
    /// REPEAT-wrap sampler for the scrolling swirl overlay.
    texture_sampler_repeat: vk::Sampler,
    mobs: HashMap<EntityKind, MobEntry>,
    player_skins: HashMap<uuid::Uuid, PlayerSkinTexture>,
    cape_model: BakedEntityModel,
    cape_vertex_buffer: vk::Buffer,
    cape_vertex_allocation: Allocation,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum BlendMode {
    Opaque,
    /// Opaque with backface culling (vanilla `entityCutoutCull`) — used by
    /// meshes with coplanar zero-depth quads (bat wings).
    OpaqueCulled,
    Translucent,
    /// Same blend as `Translucent` but keeps depth writes (vanilla
    /// `entityTranslucent` vs `EYES`).
    TranslucentDepthWrite,
    Additive,
    /// Vanilla `RenderPipelines.GLINT`: depth-equal, no depth writes.
    Glint,
    /// WATER_MASK: depth-tested/writing patch, no color attachment writes.
    DepthOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AnimationType {
    Quadruped,
    Chicken,
    Humanoid,
    Enderman,
    Zombie,
    Skeleton,
    Spider,
    Villager,
    Witch,
    Wolf,
    /// Cat and ocelot (ocelots only drive the crouch/sprint inputs).
    Feline,
    Rabbit,
    /// Horse family; the hook set is derived from (entity kind, is_baby).
    Equine,
    Squid,
    Bat,
    /// Cod, salmon, tropical fish, pufferfish.
    Fish,
    Golem,
    Blaze,
    Vex,
    Allay,
    Bee,
    Breeze,
    Ghast,
    Phantom,
    Wither,
    /// Shulker lid and exposed head follow the synced peek/facing state.
    Shulker,
    /// No part animation (slime — size/squish live in the body transform).
    Static,
    Boat,
    EndCrystal,
    ArmorStand,
}

struct VariantDef {
    model: BakedEntityModel,
    /// Outer slice: one entry per texture variant (variant_index). Inner slice:
    /// fallback chain of asset keys.
    tex_variants: &'static [&'static [&'static str]],
    tex_size: u32,
    overlay_kind: OverlayKind,
}

struct MobDef {
    kind: EntityKind,
    anim: AnimationType,
    adult: Vec<VariantDef>,
    baby: Option<VariantDef>,
    adult_overlays: Vec<VariantDef>,
    baby_overlays: Vec<VariantDef>,
}

fn mob_definitions() -> Vec<MobDef> {
    // One single-fallback texture entry per name under an entity texture dir.
    macro_rules! tex_table {
        ($dir:expr => $($name:literal),+ $(,)?) => {
            &[$(&[concat!("minecraft/textures/entity/", $dir, "/", $name, ".png")]),+]
        };
    }
    // The villager and zombie-villager overlay dirs ship identical
    // registry-ordered file names; each list is written once here and both
    // mobs' tables expand from it. Types index by the builtin VillagerKind
    // registry order, professions by VillagerProfession order minus "none"
    // (which has no texture), levels by profession level 1-5 minus one.
    macro_rules! villager_type_table {
        ($dir:expr) => {
            tex_table!($dir => "desert", "jungle", "plains", "savanna", "snow", "swamp", "taiga")
        };
    }
    macro_rules! villager_profession_table {
        ($dir:expr) => {
            tex_table!($dir => "armorer", "butcher", "cartographer", "cleric", "farmer",
                "fisherman", "fletcher", "leatherworker", "librarian", "mason", "nitwit",
                "shepherd", "toolsmith", "weaponsmith")
        };
    }
    macro_rules! villager_level_table {
        ($dir:expr) => {
            tex_table!($dir => "stone", "iron", "gold", "emerald", "diamond")
        };
    }

    const PIG_ADULT_TEX: &[&[&str]] = &[
        &["minecraft/textures/entity/pig/pig_temperate.png"],
        &["minecraft/textures/entity/pig/pig_cold.png"],
        &["minecraft/textures/entity/pig/pig_warm.png"],
    ];
    const PIG_BABY_TEX: &[&[&str]] = &[
        &["minecraft/textures/entity/pig/pig_temperate_baby.png"],
        &["minecraft/textures/entity/pig/pig_cold_baby.png"],
        &["minecraft/textures/entity/pig/pig_warm_baby.png"],
    ];
    const AXOLOTL_ADULT_TEX: &[&[&str]] = &[
        &["minecraft/textures/entity/axolotl/axolotl_lucy.png"],
        &["minecraft/textures/entity/axolotl/axolotl_wild.png"],
        &["minecraft/textures/entity/axolotl/axolotl_gold.png"],
        &["minecraft/textures/entity/axolotl/axolotl_cyan.png"],
        &["minecraft/textures/entity/axolotl/axolotl_blue.png"],
    ];
    const AXOLOTL_BABY_TEX: &[&[&str]] = &[
        &["minecraft/textures/entity/axolotl/axolotl_lucy_baby.png"],
        &["minecraft/textures/entity/axolotl/axolotl_wild_baby.png"],
        &["minecraft/textures/entity/axolotl/axolotl_gold_baby.png"],
        &["minecraft/textures/entity/axolotl/axolotl_cyan_baby.png"],
        &["minecraft/textures/entity/axolotl/axolotl_blue_baby.png"],
    ];
    const COW_ADULT_TEX: &[&[&str]] = &[
        &[
            "minecraft/textures/entity/cow/cow_temperate.png",
            "minecraft/textures/entity/cow/cow.png",
        ],
        &["minecraft/textures/entity/cow/cow_cold.png"],
        &["minecraft/textures/entity/cow/cow_warm.png"],
    ];
    const COW_BABY_TEX: &[&[&str]] = &[
        &["minecraft/textures/entity/cow/cow_temperate_baby.png"],
        &["minecraft/textures/entity/cow/cow_cold_baby.png"],
        &["minecraft/textures/entity/cow/cow_warm_baby.png"],
    ];
    // The two normal-mesh variants share one VariantDef, the cold mesh gets
    // its own; the flattened pool follows CHICKEN_VARIANT_ORDER.
    const CHICKEN_NORMAL_TEX: &[&[&str]] = &[
        &[
            "minecraft/textures/entity/chicken/chicken_temperate.png",
            "minecraft/textures/entity/chicken.png",
        ],
        &["minecraft/textures/entity/chicken/chicken_warm.png"],
    ];
    const CHICKEN_COLD_TEX: &[&[&str]] = &[&["minecraft/textures/entity/chicken/chicken_cold.png"]];
    const CHICKEN_BABY_TEX: &[&[&str]] = &[
        &["minecraft/textures/entity/chicken/chicken_temperate_baby.png"],
        &["minecraft/textures/entity/chicken/chicken_warm_baby.png"],
        &["minecraft/textures/entity/chicken/chicken_cold_baby.png"],
    ];
    const SHEEP_ADULT_TEX: &[&[&str]] = tex_table!("sheep" => "sheep");
    const SHEEP_BABY_TEX: &[&[&str]] = tex_table!("sheep" => "sheep_baby");
    const SHEEP_WOOL_UNDERCOAT_TEX: &[&[&str]] = tex_table!("sheep" => "sheep_wool_undercoat");
    const SHEEP_WOOL_TEX: &[&[&str]] = tex_table!("sheep" => "sheep_wool");
    const SHEEP_BABY_WOOL_TEX: &[&[&str]] = tex_table!("sheep" => "sheep_wool_baby");
    const PLAYER_TEX: &[&[&str]] = tex_table!("player/wide" => "steve");
    const ZOMBIE_TEX: &[&[&str]] = tex_table!("zombie" => "zombie");
    const ZOMBIE_BABY_TEX: &[&[&str]] = tex_table!("zombie" => "zombie_baby");
    const HUSK_TEX: &[&[&str]] = tex_table!("zombie" => "husk");
    const HUSK_BABY_TEX: &[&[&str]] = tex_table!("zombie" => "husk_baby");
    const DROWNED_TEX: &[&[&str]] = tex_table!("zombie" => "drowned");
    const DROWNED_BABY_TEX: &[&[&str]] = tex_table!("zombie" => "drowned_baby");
    const DROWNED_OUTER_TEX: &[&[&str]] = tex_table!("zombie" => "drowned_outer_layer");
    const DROWNED_OUTER_BABY_TEX: &[&[&str]] = tex_table!("zombie" => "drowned_outer_layer_baby");
    const ZOMBIE_VILLAGER_TEX: &[&[&str]] = tex_table!("zombie_villager" => "zombie_villager");
    const ZOMBIE_VILLAGER_BABY_TEX: &[&[&str]] =
        tex_table!("zombie_villager" => "zombie_villager_baby");
    const ZOMBIE_VILLAGER_TYPE_TEX: &[&[&str]] = villager_type_table!("zombie_villager/type");
    const ZOMBIE_VILLAGER_BABY_TYPE_TEX: &[&[&str]] = villager_type_table!("zombie_villager/baby");
    const ZOMBIE_VILLAGER_PROFESSION_TEX: &[&[&str]] =
        villager_profession_table!("zombie_villager/profession");
    const ZOMBIE_VILLAGER_LEVEL_TEX: &[&[&str]] =
        villager_level_table!("zombie_villager/profession_level");
    // Wolf pool: variant_index = variant * 3 + state (0 wild, 1 tame,
    // 2 angry); variants follow WOLF_VARIANT_ORDER.
    const WOLF_TEX: &[&[&str]] = tex_table!("wolf" =>
        "wolf", "wolf_tame", "wolf_angry",
        "wolf_spotted", "wolf_spotted_tame", "wolf_spotted_angry",
        "wolf_snowy", "wolf_snowy_tame", "wolf_snowy_angry",
        "wolf_black", "wolf_black_tame", "wolf_black_angry",
        "wolf_ashen", "wolf_ashen_tame", "wolf_ashen_angry",
        "wolf_rusty", "wolf_rusty_tame", "wolf_rusty_angry",
        "wolf_woods", "wolf_woods_tame", "wolf_woods_angry",
        "wolf_chestnut", "wolf_chestnut_tame", "wolf_chestnut_angry",
        "wolf_striped", "wolf_striped_tame", "wolf_striped_angry");
    const WOLF_BABY_TEX: &[&[&str]] = tex_table!("wolf" =>
        "wolf_baby", "wolf_tame_baby", "wolf_angry_baby",
        "wolf_spotted_baby", "wolf_spotted_tame_baby", "wolf_spotted_angry_baby",
        "wolf_snowy_baby", "wolf_snowy_tame_baby", "wolf_snowy_angry_baby",
        "wolf_black_baby", "wolf_black_tame_baby", "wolf_black_angry_baby",
        "wolf_ashen_baby", "wolf_ashen_tame_baby", "wolf_ashen_angry_baby",
        "wolf_rusty_baby", "wolf_rusty_tame_baby", "wolf_rusty_angry_baby",
        "wolf_woods_baby", "wolf_woods_tame_baby", "wolf_woods_angry_baby",
        "wolf_chestnut_baby", "wolf_chestnut_tame_baby", "wolf_chestnut_angry_baby",
        "wolf_striped_baby", "wolf_striped_tame_baby", "wolf_striped_angry_baby");
    const WOLF_COLLAR_TEX: &[&[&str]] = tex_table!("wolf" => "wolf_collar");
    const WOLF_COLLAR_BABY_TEX: &[&[&str]] = tex_table!("wolf" => "wolf_collar_baby");
    // Cat pool follows CAT_VARIANT_ORDER.
    const CAT_TEX: &[&[&str]] = tex_table!("cat" =>
        "cat_all_black", "cat_black", "cat_british_shorthair", "cat_calico", "cat_jellie",
        "cat_persian", "cat_ragdoll", "cat_red", "cat_siamese", "cat_tabby", "cat_white");
    const CAT_BABY_TEX: &[&[&str]] = tex_table!("cat" =>
        "cat_all_black_baby", "cat_black_baby", "cat_british_shorthair_baby", "cat_calico_baby",
        "cat_jellie_baby", "cat_persian_baby", "cat_ragdoll_baby", "cat_red_baby",
        "cat_siamese_baby", "cat_tabby_baby", "cat_white_baby");
    const CAT_COLLAR_TEX: &[&[&str]] = tex_table!("cat" => "cat_collar");
    const CAT_COLLAR_BABY_TEX: &[&[&str]] = tex_table!("cat" => "cat_collar_baby");
    const OCELOT_TEX: &[&[&str]] = tex_table!("cat" => "ocelot");
    const OCELOT_BABY_TEX: &[&[&str]] = tex_table!("cat" => "ocelot_baby");
    // Rabbit: variant ids 0-6 in vanilla id order, slot 7 = the "Toast"
    // custom-name override.
    const RABBIT_TEX: &[&[&str]] = tex_table!("rabbit" =>
        "rabbit_brown", "rabbit_white", "rabbit_black", "rabbit_white_splotched",
        "rabbit_gold", "rabbit_salt", "rabbit_caerbannog", "rabbit_toast");
    const RABBIT_BABY_TEX: &[&[&str]] = tex_table!("rabbit" =>
        "rabbit_brown_baby", "rabbit_white_baby", "rabbit_black_baby",
        "rabbit_white_splotched_baby", "rabbit_gold_baby", "rabbit_salt_baby",
        "rabbit_caerbannog_baby", "rabbit_toast_baby");
    // Horse variant_index = color id 0-6; markings overlay variant = id - 1.
    const HORSE_TEX: &[&[&str]] = tex_table!("horse" =>
        "horse_white", "horse_creamy", "horse_chestnut", "horse_brown", "horse_black",
        "horse_gray", "horse_darkbrown");
    const HORSE_BABY_TEX: &[&[&str]] = tex_table!("horse" =>
        "horse_white_baby", "horse_creamy_baby", "horse_chestnut_baby", "horse_brown_baby",
        "horse_black_baby", "horse_gray_baby", "horse_darkbrown_baby");
    const HORSE_MARKINGS_TEX: &[&[&str]] = tex_table!("horse" =>
        "horse_markings_white", "horse_markings_whitefield", "horse_markings_whitedots",
        "horse_markings_blackdots");
    const HORSE_MARKINGS_BABY_TEX: &[&[&str]] = tex_table!("horse" =>
        "horse_markings_white_baby", "horse_markings_whitefield_baby",
        "horse_markings_whitedots_baby", "horse_markings_blackdots_baby");
    const DONKEY_TEX: &[&[&str]] = tex_table!("horse" => "donkey");
    const DONKEY_BABY_TEX: &[&[&str]] = tex_table!("horse" => "donkey_baby");
    const MULE_TEX: &[&[&str]] = tex_table!("horse" => "mule");
    const MULE_BABY_TEX: &[&[&str]] = tex_table!("horse" => "mule_baby");
    const SKELETON_HORSE_TEX: &[&[&str]] = tex_table!("horse" => "horse_skeleton");
    const SKELETON_HORSE_BABY_TEX: &[&[&str]] = tex_table!("horse" => "horse_skeleton_baby");
    const ZOMBIE_HORSE_TEX: &[&[&str]] = tex_table!("horse" => "horse_zombie");
    const ZOMBIE_HORSE_BABY_TEX: &[&[&str]] = tex_table!("horse" => "horse_zombie_baby");
    const SQUID_TEX: &[&[&str]] = tex_table!("squid" => "squid");
    const SQUID_BABY_TEX: &[&[&str]] = tex_table!("squid" => "squid_baby");
    const GLOW_SQUID_TEX: &[&[&str]] = tex_table!("squid" => "glow_squid");
    const GLOW_SQUID_BABY_TEX: &[&[&str]] = tex_table!("squid" => "glow_squid_baby");
    const BAT_TEX: &[&[&str]] = tex_table!("bat" => "bat");
    const COD_TEX: &[&[&str]] = tex_table!("fish" => "cod");
    const SALMON_TEX: &[&[&str]] = tex_table!("fish" => "salmon");
    const PUFFERFISH_TEX: &[&[&str]] = tex_table!("fish" => "pufferfish");
    const IRON_GOLEM_TEX: &[&[&str]] = tex_table!("iron_golem" => "iron_golem");
    // Indexed by crackiness level minus one (low, medium, high).
    const IRON_GOLEM_CRACKINESS_TEX: &[&[&str]] = tex_table!("iron_golem" =>
        "iron_golem_crackiness_low", "iron_golem_crackiness_medium",
        "iron_golem_crackiness_high");
    const TROPICAL_A_TEX: &[&[&str]] = tex_table!("fish" => "tropical_a");
    const TROPICAL_B_TEX: &[&[&str]] = tex_table!("fish" => "tropical_b");
    const TROPICAL_A_PATTERN_TEX: &[&[&str]] = tex_table!("fish" =>
        "tropical_a_pattern_1", "tropical_a_pattern_2", "tropical_a_pattern_3",
        "tropical_a_pattern_4", "tropical_a_pattern_5", "tropical_a_pattern_6");
    const TROPICAL_B_PATTERN_TEX: &[&[&str]] = tex_table!("fish" =>
        "tropical_b_pattern_1", "tropical_b_pattern_2", "tropical_b_pattern_3",
        "tropical_b_pattern_4", "tropical_b_pattern_5", "tropical_b_pattern_6");
    const SKELETON_TEX: &[&[&str]] = tex_table!("skeleton" => "skeleton");
    const STRAY_TEX: &[&[&str]] = tex_table!("skeleton" => "stray");
    const STRAY_OVERLAY_TEX: &[&[&str]] = tex_table!("skeleton" => "stray_overlay");
    const BOGGED_TEX: &[&[&str]] = tex_table!("skeleton" => "bogged");
    const BOGGED_OVERLAY_TEX: &[&[&str]] = tex_table!("skeleton" => "bogged_overlay");
    const CREEPER_TEX: &[&[&str]] = tex_table!("creeper" => "creeper");
    const CREEPER_ARMOR_TEX: &[&[&str]] = tex_table!("creeper" => "creeper_armor");
    const SPIDER_TEX: &[&[&str]] = tex_table!("spider" => "spider");
    const CAVE_SPIDER_TEX: &[&[&str]] = tex_table!("spider" => "cave_spider");
    const SPIDER_EYES_TEX: &[&[&str]] = tex_table!("spider" => "spider_eyes");
    const ENDERMAN_TEX: &[&[&str]] = tex_table!("enderman" => "enderman");
    const ENDERMAN_EYES_TEX: &[&[&str]] = tex_table!("enderman" => "enderman_eyes");
    const SLIME_TEX: &[&[&str]] = tex_table!("slime" => "slime");
    const WITCH_TEX: &[&[&str]] = &[&[
        "minecraft/textures/entity/witch/witch.png",
        "minecraft/textures/entity/witch.png",
    ]];
    const VILLAGER_TEX: &[&[&str]] = tex_table!("villager" => "villager");
    const VILLAGER_BABY_TEX: &[&[&str]] = tex_table!("villager" => "villager_baby");
    const VILLAGER_TYPE_TEX: &[&[&str]] = villager_type_table!("villager/type");
    const VILLAGER_BABY_TYPE_TEX: &[&[&str]] = villager_type_table!("villager/baby");
    const VILLAGER_PROFESSION_TEX: &[&[&str]] = villager_profession_table!("villager/profession");
    const VILLAGER_LEVEL_TEX: &[&[&str]] = villager_level_table!("villager/profession_level");

    // Base and baby models, plus opaque overlays (sheep wool), are all Opaque.
    fn opaque(
        model: BakedEntityModel,
        tex_variants: &'static [&'static [&'static str]],
        tex_size: u32,
    ) -> VariantDef {
        VariantDef {
            model,
            tex_variants,
            tex_size,
            overlay_kind: OverlayKind::Opaque,
        }
    }

    // Cutout layers over a villager-like base skin (vanilla
    // `VillagerProfessionLayer`, shared by villager and zombie villager):
    // slot 0 = biome type, slot 1 = biome type on the no-hat model (used when
    // the profession texture brings its own hat), slot 2 = profession, slot 3
    // = profession level badge. entity_extras gates slot 0 xor 1 and picks
    // each slot's texture variant. The `bake` parameter takes `no_hat`.
    fn villager_like_overlays(
        bake: fn(bool) -> BakedEntityModel,
        type_tex: &'static [&'static [&'static str]],
        profession_tex: &'static [&'static [&'static str]],
        level_tex: &'static [&'static [&'static str]],
    ) -> Vec<VariantDef> {
        // Slots 0/2/3 share one bake of the hatted model.
        let hatted = bake(false);
        vec![
            opaque(hatted.clone(), type_tex, 64),
            opaque(bake(true), type_tex, 64),
            opaque(hatted.clone(), profession_tex, 64),
            opaque(hatted, level_tex, 64),
        ]
    }

    fn villager_like_baby_overlays(
        bake: fn(bool) -> BakedEntityModel,
        type_tex: &'static [&'static [&'static str]],
    ) -> Vec<VariantDef> {
        vec![
            opaque(bake(false), type_tex, 64),
            opaque(bake(true), type_tex, 64),
        ]
    }

    let mut defs = vec![
        MobDef {
            kind: EntityKind::Arrow,
            anim: AnimationType::Static,
            adult: vec![
                VariantDef {
                    model: entity_model::bake_arrow_model(),
                    tex_variants: &[&["minecraft/textures/entity/projectiles/arrow.png"]],
                    tex_size: 32,
                    overlay_kind: OverlayKind::OpaqueCulled,
                },
                VariantDef {
                    model: entity_model::bake_arrow_model(),
                    tex_variants: &[&["minecraft/textures/entity/projectiles/arrow_spectral.png"]],
                    tex_size: 32,
                    overlay_kind: OverlayKind::OpaqueCulled,
                },
                VariantDef {
                    model: entity_model::bake_arrow_model(),
                    tex_variants: &[&["minecraft/textures/entity/projectiles/arrow_tipped.png"]],
                    tex_size: 32,
                    overlay_kind: OverlayKind::OpaqueCulled,
                },
            ],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::ArmorStand,
            anim: AnimationType::ArmorStand,
            adult: vec![opaque(
                entity_models::nonliving_special::bake_armor_stand_model(),
                &[&["minecraft/textures/entity/armorstand/armorstand.png"]],
                64,
            )],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Pig,
            anim: AnimationType::Quadruped,
            adult: vec![
                opaque(entity_model::bake_pig_model(), &PIG_ADULT_TEX[..1], 64),
                opaque(
                    entity_model::bake_cold_pig_model(),
                    &PIG_ADULT_TEX[1..2],
                    64,
                ),
                opaque(entity_model::bake_pig_model(), &PIG_ADULT_TEX[2..], 64),
            ],
            baby: Some(opaque(
                entity_model::bake_baby_pig_model(),
                PIG_BABY_TEX,
                64,
            )),
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Cow,
            anim: AnimationType::Quadruped,
            adult: vec![opaque(entity_model::bake_cow_model(), COW_ADULT_TEX, 64)],
            baby: Some(opaque(
                entity_model::bake_baby_cow_model(),
                COW_BABY_TEX,
                64,
            )),
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Chicken,
            anim: AnimationType::Chicken,
            adult: vec![
                opaque(entity_model::bake_chicken_model(), CHICKEN_NORMAL_TEX, 64),
                opaque(
                    entity_model::bake_cold_chicken_model(),
                    CHICKEN_COLD_TEX,
                    64,
                ),
            ],
            baby: Some(opaque(
                entity_model::bake_baby_chicken_model(),
                CHICKEN_BABY_TEX,
                16,
            )),
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Sheep,
            anim: AnimationType::Quadruped,
            adult: vec![opaque(
                entity_model::bake_sheep_model(),
                SHEEP_ADULT_TEX,
                64,
            )],
            baby: Some(opaque(
                entity_model::bake_baby_sheep_model(),
                SHEEP_BABY_TEX,
                64,
            )),
            adult_overlays: vec![
                opaque(
                    entity_model::bake_sheep_wool_undercoat_model(),
                    SHEEP_WOOL_UNDERCOAT_TEX,
                    64,
                ),
                opaque(entity_model::bake_sheep_wool_model(), SHEEP_WOOL_TEX, 64),
            ],
            baby_overlays: vec![opaque(
                entity_model::bake_baby_sheep_wool_model(),
                SHEEP_BABY_WOOL_TEX,
                64,
            )],
        },
        MobDef {
            kind: EntityKind::Mannequin,
            anim: AnimationType::Humanoid,
            adult: vec![
                opaque(entity_model::bake_player_model(false), PLAYER_TEX, 64),
                opaque(entity_model::bake_player_model(true), PLAYER_TEX, 64),
            ],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Player,
            anim: AnimationType::Humanoid,
            // Variant 0 = classic (wide) arms, 1 = slim; picked per player from
            // the skin's model metadata (effective_variant_index).
            adult: vec![
                opaque(entity_model::bake_player_model(false), PLAYER_TEX, 64),
                opaque(entity_model::bake_player_model(true), PLAYER_TEX, 64),
            ],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Zombie,
            anim: AnimationType::Zombie,
            adult: vec![opaque(entity_model::bake_zombie_model(), ZOMBIE_TEX, 64)],
            baby: Some(opaque(
                entity_model::bake_baby_zombie_model(),
                ZOMBIE_BABY_TEX,
                64,
            )),
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Husk,
            anim: AnimationType::Zombie,
            adult: vec![opaque(entity_model::bake_husk_model(), HUSK_TEX, 64)],
            baby: Some(opaque(
                entity_model::bake_baby_zombie_model(),
                HUSK_BABY_TEX,
                64,
            )),
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Drowned,
            anim: AnimationType::Zombie,
            adult: vec![opaque(
                entity_model::bake_drowned_model(0.0),
                DROWNED_TEX,
                64,
            )],
            baby: Some(opaque(
                entity_model::bake_baby_zombie_model(),
                DROWNED_BABY_TEX,
                64,
            )),
            adult_overlays: vec![opaque(
                entity_model::bake_drowned_model(0.25),
                DROWNED_OUTER_TEX,
                64,
            )],
            baby_overlays: vec![opaque(
                entity_model::bake_baby_drowned_outer_model(),
                DROWNED_OUTER_BABY_TEX,
                64,
            )],
        },
        MobDef {
            kind: EntityKind::ZombieVillager,
            anim: AnimationType::Zombie,
            adult: vec![opaque(
                entity_model::bake_zombie_villager_model(false),
                ZOMBIE_VILLAGER_TEX,
                64,
            )],
            baby: Some(opaque(
                entity_model::bake_baby_zombie_villager_model(false),
                ZOMBIE_VILLAGER_BABY_TEX,
                64,
            )),
            adult_overlays: villager_like_overlays(
                entity_model::bake_zombie_villager_model,
                ZOMBIE_VILLAGER_TYPE_TEX,
                ZOMBIE_VILLAGER_PROFESSION_TEX,
                ZOMBIE_VILLAGER_LEVEL_TEX,
            ),
            baby_overlays: villager_like_baby_overlays(
                entity_model::bake_baby_zombie_villager_model,
                ZOMBIE_VILLAGER_BABY_TYPE_TEX,
            ),
        },
        MobDef {
            kind: EntityKind::Skeleton,
            anim: AnimationType::Skeleton,
            adult: vec![opaque(
                entity_model::bake_skeleton_model(),
                SKELETON_TEX,
                64,
            )],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Stray,
            anim: AnimationType::Skeleton,
            adult: vec![opaque(entity_model::bake_skeleton_model(), STRAY_TEX, 64)],
            baby: None,
            adult_overlays: vec![opaque(
                entity_model::bake_skeleton_clothing_model(0.25),
                STRAY_OVERLAY_TEX,
                64,
            )],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Bogged,
            anim: AnimationType::Skeleton,
            // Variant 0 = mushrooms, 1 = sheared (empty mushroom parts).
            // TODO: replace with a per-part visibility mask (vanilla
            // `mushrooms.visible = !isSheared`) instead of a second baked
            // model; would also drop the cubeless overlay padding.
            adult: vec![
                opaque(entity_model::bake_bogged_model(false), BOGGED_TEX, 64),
                opaque(entity_model::bake_bogged_model(true), BOGGED_TEX, 64),
            ],
            baby: None,
            adult_overlays: vec![opaque(
                entity_model::bake_bogged_clothing_model(),
                BOGGED_OVERLAY_TEX,
                64,
            )],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Creeper,
            anim: AnimationType::Quadruped,
            adult: vec![opaque(entity_model::bake_creeper_model(), CREEPER_TEX, 64)],
            baby: None,
            // Slot 0: charged-creeper energy swirl (additive, scrolling), shown only
            // when `powered` (gated via overlay_tints in entity_extras).
            adult_overlays: vec![VariantDef {
                model: entity_model::bake_creeper_model(),
                tex_variants: CREEPER_ARMOR_TEX,
                tex_size: 64,
                overlay_kind: OverlayKind::SwirlAdditive,
            }],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Villager,
            anim: AnimationType::Villager,
            adult: vec![opaque(
                entity_model::bake_villager_model(false),
                VILLAGER_TEX,
                64,
            )],
            baby: Some(opaque(
                entity_model::bake_baby_villager_model(false),
                VILLAGER_BABY_TEX,
                64,
            )),
            // TODO: CustomHeadLayer (worn head items) and CrossedArmsItemLayer
            // (held item) need a held-item layer first.
            adult_overlays: villager_like_overlays(
                entity_model::bake_villager_model,
                VILLAGER_TYPE_TEX,
                VILLAGER_PROFESSION_TEX,
                VILLAGER_LEVEL_TEX,
            ),
            baby_overlays: villager_like_baby_overlays(
                entity_model::bake_baby_villager_model,
                VILLAGER_BABY_TYPE_TEX,
            ),
        },
        MobDef {
            kind: EntityKind::Giant,
            anim: AnimationType::Zombie,
            adult: vec![opaque(entity_model::bake_zombie_model(), ZOMBIE_TEX, 64)],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Spider,
            anim: AnimationType::Spider,
            adult: vec![opaque(entity_model::bake_spider_model(), SPIDER_TEX, 64)],
            baby: None,
            // Slot 0: glowing eyes (translucent, full-bright), always visible.
            adult_overlays: vec![VariantDef {
                model: entity_model::bake_spider_model(),
                tex_variants: SPIDER_EYES_TEX,
                tex_size: 64,
                overlay_kind: OverlayKind::EyesTranslucent,
            }],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::CaveSpider,
            anim: AnimationType::Spider,
            adult: vec![opaque(
                entity_model::bake_spider_model(),
                CAVE_SPIDER_TEX,
                64,
            )],
            baby: None,
            adult_overlays: vec![VariantDef {
                model: entity_model::bake_spider_model(),
                tex_variants: SPIDER_EYES_TEX,
                tex_size: 64,
                overlay_kind: OverlayKind::EyesTranslucent,
            }],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Enderman,
            anim: AnimationType::Enderman,
            adult: vec![opaque(
                entity_model::bake_enderman_model(),
                ENDERMAN_TEX,
                64,
            )],
            baby: None,
            adult_overlays: vec![VariantDef {
                model: entity_model::bake_enderman_model(),
                tex_variants: ENDERMAN_EYES_TEX,
                tex_size: 64,
                overlay_kind: OverlayKind::EyesTranslucent,
            }],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Slime,
            anim: AnimationType::Static,
            adult: vec![opaque(
                entity_model::bake_slime_inner_model(),
                SLIME_TEX,
                64,
            )],
            baby: None,
            adult_overlays: vec![VariantDef {
                model: entity_model::bake_slime_outer_model(),
                tex_variants: SLIME_TEX,
                tex_size: 64,
                overlay_kind: OverlayKind::BodyTranslucent,
            }],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Witch,
            anim: AnimationType::Witch,
            adult: vec![opaque(entity_model::bake_witch_model(), WITCH_TEX, 64)],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        // TODO: wolf armor layer (needs the equipment-asset pipeline).
        MobDef {
            kind: EntityKind::Wolf,
            anim: AnimationType::Wolf,
            adult: vec![opaque(entity_model::bake_wolf_model(), WOLF_TEX, 64)],
            baby: Some(opaque(
                entity_model::bake_baby_wolf_model(),
                WOLF_BABY_TEX,
                32,
            )),
            // Slot 0: dye-tinted collar, tame only.
            adult_overlays: vec![opaque(
                entity_model::bake_wolf_collar_model(),
                WOLF_COLLAR_TEX,
                64,
            )],
            baby_overlays: vec![opaque(
                entity_model::bake_baby_wolf_model(),
                WOLF_COLLAR_BABY_TEX,
                32,
            )],
        },
        MobDef {
            kind: EntityKind::Cat,
            anim: AnimationType::Feline,
            adult: vec![opaque(entity_model::bake_cat_model(), CAT_TEX, 64)],
            baby: Some(opaque(
                entity_model::bake_baby_cat_model(),
                CAT_BABY_TEX,
                32,
            )),
            // Slot 0: dye-tinted collar, tame only (its bake is inflated /
            // rescaled per vanilla's collar layers).
            adult_overlays: vec![opaque(
                entity_model::bake_cat_collar_model(),
                CAT_COLLAR_TEX,
                64,
            )],
            baby_overlays: vec![opaque(
                entity_model::bake_baby_cat_collar_model(),
                CAT_COLLAR_BABY_TEX,
                32,
            )],
        },
        MobDef {
            kind: EntityKind::Ocelot,
            anim: AnimationType::Feline,
            adult: vec![opaque(entity_model::bake_ocelot_model(), OCELOT_TEX, 64)],
            baby: Some(opaque(
                entity_model::bake_baby_ocelot_model(),
                OCELOT_BABY_TEX,
                32,
            )),
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Rabbit,
            anim: AnimationType::Rabbit,
            adult: vec![opaque(entity_model::bake_rabbit_model(), RABBIT_TEX, 64)],
            baby: Some(opaque(
                entity_model::bake_baby_rabbit_model(),
                RABBIT_BABY_TEX,
                32,
            )),
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Horse,
            anim: AnimationType::Equine,
            adult: vec![opaque(entity_model::bake_horse_model(), HORSE_TEX, 64)],
            baby: Some(opaque(
                entity_model::bake_baby_horse_model(),
                HORSE_BABY_TEX,
                64,
            )),
            // Slot 0: markings (vanilla `entityTranslucent`), gated on
            // markings != NONE.
            adult_overlays: vec![
                VariantDef {
                    model: entity_model::bake_horse_model(),
                    tex_variants: HORSE_MARKINGS_TEX,
                    tex_size: 64,
                    overlay_kind: OverlayKind::BodyTranslucent,
                },
                VariantDef {
                    model: entity_model::bake_horse_saddle_model(),
                    tex_variants: &[&[
                        "minecraft/textures/entity/equipment/horse_saddle/saddle.png",
                    ]],
                    tex_size: 64,
                    overlay_kind: OverlayKind::Opaque,
                },
            ],
            baby_overlays: vec![VariantDef {
                model: entity_model::bake_baby_horse_model(),
                tex_variants: HORSE_MARKINGS_BABY_TEX,
                tex_size: 64,
                overlay_kind: OverlayKind::BodyTranslucent,
            }],
        },
        MobDef {
            kind: EntityKind::Donkey,
            anim: AnimationType::Equine,
            // Variant 0 = no chest, 1 = chest; the single baby bake absorbs
            // both through `base_variant`'s pool clamp.
            adult: vec![
                opaque(entity_model::bake_donkey_model(0.87, false), DONKEY_TEX, 64),
                opaque(entity_model::bake_donkey_model(0.87, true), DONKEY_TEX, 64),
            ],
            baby: Some(opaque(
                entity_model::bake_baby_donkey_model(),
                DONKEY_BABY_TEX,
                64,
            )),
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Mule,
            anim: AnimationType::Equine,
            adult: vec![
                opaque(entity_model::bake_donkey_model(0.92, false), MULE_TEX, 64),
                opaque(entity_model::bake_donkey_model(0.92, true), MULE_TEX, 64),
            ],
            baby: Some(opaque(
                entity_model::bake_baby_donkey_model(),
                MULE_BABY_TEX,
                64,
            )),
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::SkeletonHorse,
            anim: AnimationType::Equine,
            adult: vec![opaque(
                entity_model::bake_undead_horse_model(),
                SKELETON_HORSE_TEX,
                64,
            )],
            baby: Some(opaque(
                entity_model::bake_baby_horse_model(),
                SKELETON_HORSE_BABY_TEX,
                64,
            )),
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::ZombieHorse,
            anim: AnimationType::Equine,
            adult: vec![opaque(
                entity_model::bake_undead_horse_model(),
                ZOMBIE_HORSE_TEX,
                64,
            )],
            baby: Some(opaque(
                entity_model::bake_baby_horse_model(),
                ZOMBIE_HORSE_BABY_TEX,
                64,
            )),
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Squid,
            anim: AnimationType::Squid,
            adult: vec![opaque(entity_model::bake_squid_model(), SQUID_TEX, 64)],
            baby: Some(opaque(
                entity_model::bake_baby_squid_model(),
                SQUID_BABY_TEX,
                32,
            )),
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        // The glow itself is free (the entity pipeline is unlit/fullbright);
        // the post-hurt dimming rides base_tint.
        MobDef {
            kind: EntityKind::GlowSquid,
            anim: AnimationType::Squid,
            adult: vec![opaque(entity_model::bake_squid_model(), GLOW_SQUID_TEX, 64)],
            baby: Some(opaque(
                entity_model::bake_baby_squid_model(),
                GLOW_SQUID_BABY_TEX,
                32,
            )),
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Bat,
            anim: AnimationType::Bat,
            // Backface-culled: the bat's zero-depth quads are coplanar
            // front/back pairs (vanilla `entityCutoutCull`).
            adult: vec![VariantDef {
                model: entity_model::bake_bat_model(),
                tex_variants: BAT_TEX,
                tex_size: 32,
                overlay_kind: OverlayKind::OpaqueCulled,
            }],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Cod,
            anim: AnimationType::Fish,
            adult: vec![opaque(entity_model::bake_cod_model(), COD_TEX, 32)],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        // Variant = size (small/medium/large), three root-scaled bakes.
        MobDef {
            kind: EntityKind::Salmon,
            anim: AnimationType::Fish,
            adult: vec![
                opaque(entity_model::bake_salmon_model(0.5), SALMON_TEX, 32),
                opaque(entity_model::bake_salmon_model(1.0), SALMON_TEX, 32),
                opaque(entity_model::bake_salmon_model(1.5), SALMON_TEX, 32),
            ],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        // Variant = shape; the dye-tinted pattern layer picks the matching
        // shape slot (0 small / 1 large, xor-gated in entity_extras).
        MobDef {
            kind: EntityKind::TropicalFish,
            anim: AnimationType::Fish,
            adult: vec![
                opaque(
                    entity_model::bake_tropical_fish_model(false, 0.0),
                    TROPICAL_A_TEX,
                    32,
                ),
                opaque(
                    entity_model::bake_tropical_fish_model(true, 0.0),
                    TROPICAL_B_TEX,
                    32,
                ),
            ],
            baby: None,
            adult_overlays: vec![
                opaque(
                    entity_model::bake_tropical_fish_model(false, 0.008),
                    TROPICAL_A_PATTERN_TEX,
                    32,
                ),
                opaque(
                    entity_model::bake_tropical_fish_model(true, 0.008),
                    TROPICAL_B_PATTERN_TEX,
                    32,
                ),
            ],
            baby_overlays: vec![],
        },
        // Variant = puff state (three meshes).
        MobDef {
            kind: EntityKind::Pufferfish,
            anim: AnimationType::Fish,
            adult: vec![
                opaque(entity_model::bake_pufferfish_model(0), PUFFERFISH_TEX, 32),
                opaque(entity_model::bake_pufferfish_model(1), PUFFERFISH_TEX, 32),
                opaque(entity_model::bake_pufferfish_model(2), PUFFERFISH_TEX, 32),
            ],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        // TODO: `IronGolemFlowerLayer` (the offered poppy) needs block models
        // rendered inside an entity pose.
        MobDef {
            kind: EntityKind::IronGolem,
            anim: AnimationType::Golem,
            adult: vec![opaque(
                entity_model::bake_iron_golem_model(),
                IRON_GOLEM_TEX,
                128,
            )],
            baby: None,
            // Slot 0: crack overlay, gated on health in entity_extras.
            adult_overlays: vec![opaque(
                entity_model::bake_iron_golem_model(),
                IRON_GOLEM_CRACKINESS_TEX,
                128,
            )],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Axolotl,
            anim: AnimationType::Static,
            adult: vec![opaque(
                entity_models::aquatic::bake_axolotl_model(),
                AXOLOTL_ADULT_TEX,
                64,
            )],
            baby: Some(opaque(
                entity_models::aquatic::bake_baby_axolotl_model(),
                AXOLOTL_BABY_TEX,
                32,
            )),
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Dolphin,
            anim: AnimationType::Static,
            adult: vec![opaque(
                entity_models::aquatic::bake_dolphin_model(),
                tex_table!("dolphin" => "dolphin"),
                64,
            )],
            baby: Some(opaque(
                entity_models::aquatic::bake_baby_dolphin_model(),
                tex_table!("dolphin" => "dolphin_baby"),
                64,
            )),
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Guardian,
            anim: AnimationType::Static,
            adult: vec![opaque(
                entity_models::aquatic::bake_guardian_model(false),
                tex_table!("guardian" => "guardian"),
                64,
            )],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::ElderGuardian,
            anim: AnimationType::Static,
            adult: vec![opaque(
                entity_models::aquatic::bake_guardian_model(true),
                tex_table!("guardian" => "guardian_elder"),
                64,
            )],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Turtle,
            anim: AnimationType::Static,
            adult: vec![opaque(
                entity_models::aquatic::bake_turtle_model(),
                tex_table!("turtle" => "turtle"),
                128,
            )],
            baby: Some(opaque(
                entity_models::aquatic::bake_baby_turtle_model(),
                tex_table!("turtle" => "turtle_baby"),
                16,
            )),
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Armadillo,
            anim: AnimationType::Static,
            adult: vec![opaque(
                entity_models::terrestrial::bake_armadillo_model(),
                tex_table!("armadillo" => "armadillo"),
                64,
            )],
            baby: Some(opaque(
                entity_models::terrestrial::bake_baby_armadillo_model(),
                tex_table!("armadillo" => "armadillo_baby"),
                64,
            )),
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Camel,
            anim: AnimationType::Static,
            adult: vec![opaque(
                entity_models::terrestrial::bake_camel_model(),
                tex_table!("camel" => "camel"),
                128,
            )],
            baby: Some(opaque(
                entity_models::terrestrial::bake_baby_camel_model(),
                tex_table!("camel" => "camel_baby"),
                64,
            )),
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::CamelHusk,
            anim: AnimationType::Static,
            adult: vec![opaque(
                entity_models::terrestrial::bake_camel_husk_model(),
                tex_table!("camel" => "camel_husk"),
                128,
            )],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Fox,
            anim: AnimationType::Static,
            adult: vec![opaque(
                entity_models::terrestrial::bake_fox_model(),
                tex_table!("fox" => "fox", "fox_sleep", "fox_snow", "fox_snow_sleep"),
                48,
            )],
            baby: Some(opaque(
                entity_models::terrestrial::bake_baby_fox_model(),
                tex_table!("fox" => "fox_baby", "fox_sleep_baby", "fox_snow_baby", "fox_snow_sleep_baby"),
                48,
            )),
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Frog,
            anim: AnimationType::Static,
            adult: vec![opaque(
                entity_models::terrestrial::bake_frog_model(),
                tex_table!("frog" => "frog_temperate", "frog_warm", "frog_cold"),
                48,
            )],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Goat,
            anim: AnimationType::Static,
            adult: vec![opaque(
                entity_models::terrestrial::bake_goat_model(),
                tex_table!("goat" => "goat"),
                64,
            )],
            baby: Some(opaque(
                entity_models::terrestrial::bake_baby_goat_model(),
                tex_table!("goat" => "goat_baby"),
                64,
            )),
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Hoglin,
            anim: AnimationType::Static,
            adult: vec![opaque(
                entity_models::terrestrial::bake_hoglin_model(),
                tex_table!("hoglin" => "hoglin"),
                128,
            )],
            baby: Some(opaque(
                entity_models::terrestrial::bake_baby_hoglin_model(),
                tex_table!("hoglin" => "hoglin_baby"),
                64,
            )),
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Zoglin,
            anim: AnimationType::Static,
            adult: vec![opaque(
                entity_models::terrestrial::bake_zoglin_model(),
                tex_table!("hoglin" => "zoglin"),
                64,
            )],
            baby: Some(opaque(
                entity_models::terrestrial::bake_baby_zoglin_model(),
                tex_table!("hoglin" => "zoglin_baby"),
                64,
            )),
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Panda,
            anim: AnimationType::Static,
            adult: vec![opaque(
                entity_models::terrestrial::bake_panda_model(),
                tex_table!("panda" => "panda"),
                64,
            )],
            baby: Some(opaque(
                entity_models::terrestrial::bake_baby_panda_model(),
                tex_table!("panda" => "panda_baby"),
                64,
            )),
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::PolarBear,
            anim: AnimationType::Static,
            adult: vec![opaque(
                entity_models::terrestrial::bake_polar_bear_model(),
                tex_table!("bear" => "polarbear"),
                128,
            )],
            baby: Some(opaque(
                entity_models::terrestrial::bake_baby_polar_bear_model(),
                tex_table!("bear" => "polarbear_baby"),
                64,
            )),
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Ravager,
            anim: AnimationType::Static,
            adult: vec![opaque(
                entity_models::terrestrial::bake_ravager_model(),
                tex_table!("illager" => "ravager"),
                64,
            )],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Sniffer,
            anim: AnimationType::Static,
            adult: vec![opaque(
                entity_models::terrestrial::bake_sniffer_model(),
                tex_table!("sniffer" => "sniffer"),
                192,
            )],
            baby: Some(opaque(
                entity_models::terrestrial::bake_baby_sniffer_model(),
                tex_table!("sniffer" => "snifflet"),
                128,
            )),
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Strider,
            anim: AnimationType::Static,
            adult: vec![opaque(
                entity_models::terrestrial::bake_strider_model(),
                tex_table!("strider" => "strider", "strider_cold"),
                64,
            )],
            baby: Some(opaque(
                entity_models::terrestrial::bake_baby_strider_model(),
                tex_table!("strider" => "strider_baby", "strider_cold_baby"),
                32,
            )),
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Llama,
            anim: AnimationType::Static,
            adult: vec![opaque(
                entity_models::terrestrial::bake_llama_model(),
                tex_table!("llama" => "llama_brown", "llama_creamy", "llama_gray", "llama_white"),
                128,
            )],
            baby: Some(opaque(
                entity_models::terrestrial::bake_baby_llama_model(),
                tex_table!("llama" => "llama_brown_baby", "llama_creamy_baby", "llama_gray_baby", "llama_white_baby"),
                64,
            )),
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::TraderLlama,
            anim: AnimationType::Static,
            adult: vec![opaque(
                entity_models::terrestrial::bake_llama_model(),
                tex_table!("llama" => "llama_brown", "llama_creamy", "llama_gray", "llama_white"),
                128,
            )],
            baby: Some(opaque(
                entity_models::terrestrial::bake_baby_llama_model(),
                tex_table!("llama" => "llama_brown_baby", "llama_creamy_baby", "llama_gray_baby", "llama_white_baby"),
                64,
            )),
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Endermite,
            anim: AnimationType::Static,
            adult: vec![opaque(
                entity_models::humanoid::bake_endermite_model(),
                tex_table!("endermite" => "endermite"),
                64,
            )],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Silverfish,
            anim: AnimationType::Static,
            adult: vec![opaque(
                entity_models::humanoid::bake_silverfish_model(),
                tex_table!("silverfish" => "silverfish"),
                64,
            )],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::SnowGolem,
            anim: AnimationType::Static,
            adult: vec![opaque(
                entity_models::humanoid::bake_snow_golem_model(),
                tex_table!("snow_golem" => "snow_golem"),
                64,
            )],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Tadpole,
            anim: AnimationType::Static,
            adult: vec![opaque(
                entity_models::humanoid::bake_tadpole_model(),
                tex_table!("tadpole" => "tadpole"),
                16,
            )],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::WanderingTrader,
            anim: AnimationType::Villager,
            adult: vec![opaque(
                entity_models::humanoid::bake_wandering_trader_model(),
                tex_table!("wandering_trader" => "wandering_trader"),
                64,
            )],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Parched,
            anim: AnimationType::Skeleton,
            adult: vec![opaque(
                entity_models::humanoid::bake_parched_model(),
                tex_table!("skeleton" => "parched"),
                64,
            )],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::WitherSkeleton,
            anim: AnimationType::Skeleton,
            adult: vec![opaque(
                entity_models::humanoid::bake_wither_skeleton_model(),
                tex_table!("skeleton" => "wither_skeleton"),
                64,
            )],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Evoker,
            anim: AnimationType::Humanoid,
            adult: vec![opaque(
                entity_models::humanoid::bake_illager_model(),
                tex_table!("illager" => "evoker"),
                64,
            )],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Illusioner,
            anim: AnimationType::Humanoid,
            adult: vec![opaque(
                entity_models::humanoid::bake_illager_model(),
                tex_table!("illager" => "illusioner"),
                64,
            )],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Pillager,
            anim: AnimationType::Humanoid,
            adult: vec![opaque(
                entity_models::humanoid::bake_illager_model(),
                tex_table!("illager" => "pillager"),
                64,
            )],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Vindicator,
            anim: AnimationType::Humanoid,
            adult: vec![opaque(
                entity_models::humanoid::bake_illager_model(),
                tex_table!("illager" => "vindicator"),
                64,
            )],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Piglin,
            anim: AnimationType::Humanoid,
            adult: vec![opaque(
                entity_models::humanoid::bake_piglin_model(),
                tex_table!("piglin" => "piglin"),
                64,
            )],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::PiglinBrute,
            anim: AnimationType::Humanoid,
            adult: vec![opaque(
                entity_models::humanoid::bake_piglin_model(),
                tex_table!("piglin" => "piglin_brute"),
                64,
            )],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::ZombifiedPiglin,
            anim: AnimationType::Humanoid,
            adult: vec![opaque(
                entity_models::humanoid::bake_piglin_model(),
                tex_table!("piglin" => "zombified_piglin"),
                64,
            )],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::CopperGolem,
            anim: AnimationType::Golem,
            adult: vec![opaque(
                entity_models::humanoid::bake_copper_golem_model(),
                tex_table!("copper_golem" => "copper_golem"),
                64,
            )],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Creaking,
            anim: AnimationType::Static,
            adult: vec![opaque(
                entity_models::humanoid::bake_creaking_model(),
                tex_table!("creaking" => "creaking"),
                64,
            )],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Parrot,
            anim: AnimationType::Static,
            adult: vec![opaque(
                entity_models::humanoid::bake_parrot_model(),
                tex_table!("parrot" => "parrot_red_blue", "parrot_blue", "parrot_green", "parrot_yellow_blue", "parrot_grey"),
                32,
            )],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Warden,
            anim: AnimationType::Static,
            adult: vec![opaque(
                entity_models::humanoid::bake_warden_model(),
                tex_table!("warden" => "warden"),
                128,
            )],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Nautilus,
            anim: AnimationType::Static,
            adult: vec![opaque(
                entity_models::humanoid::bake_nautilus_model(),
                tex_table!("nautilus" => "nautilus"),
                128,
            )],
            baby: Some(opaque(
                entity_models::humanoid::bake_baby_nautilus_model(),
                tex_table!("nautilus" => "nautilus_baby"),
                64,
            )),
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::ZombieNautilus,
            anim: AnimationType::Static,
            adult: vec![opaque(
                entity_models::humanoid::bake_zombie_nautilus_model(),
                tex_table!("nautilus" => "zombie_nautilus"),
                128,
            )],
            baby: Some(opaque(
                entity_models::humanoid::bake_baby_nautilus_model(),
                tex_table!("nautilus" => "zombie_nautilus"),
                64,
            )),
            adult_overlays: vec![opaque(
                entity_models::humanoid::bake_zombie_nautilus_coral_model(),
                tex_table!("nautilus" => "zombie_nautilus_coral"),
                128,
            )],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Allay,
            anim: AnimationType::Allay,
            adult: vec![opaque(
                entity_models::flying::bake_allay_model(),
                tex_table!("allay" => "allay"),
                32,
            )],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Bee,
            anim: AnimationType::Bee,
            adult: vec![opaque(
                entity_models::flying::bake_bee_model(),
                tex_table!("bee" => "bee", "bee_angry", "bee_nectar", "bee_angry_nectar"),
                64,
            )],
            baby: Some(opaque(
                entity_models::flying::bake_baby_bee_model(),
                tex_table!("bee" => "bee_baby", "bee_angry_baby", "bee_nectar_baby", "bee_angry_nectar_baby"),
                32,
            )),
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Blaze,
            anim: AnimationType::Blaze,
            adult: vec![opaque(
                entity_models::flying::bake_blaze_model(),
                tex_table!("blaze" => "blaze"),
                64,
            )],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Breeze,
            anim: AnimationType::Breeze,
            adult: vec![opaque(
                entity_models::flying::bake_breeze_model(),
                tex_table!("breeze" => "breeze"),
                32,
            )],
            baby: None,
            adult_overlays: vec![
                opaque(
                    entity_models::flying::bake_breeze_eyes_model(),
                    tex_table!("breeze" => "breeze_eyes"),
                    32,
                ),
                VariantDef {
                    model: entity_models::flying::bake_breeze_wind_model(),
                    tex_variants: tex_table!("breeze" => "breeze_wind"),
                    tex_size: 128,
                    overlay_kind: OverlayKind::WindScroll,
                },
            ],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Ghast,
            anim: AnimationType::Ghast,
            adult: vec![opaque(
                entity_models::flying::bake_ghast_model(),
                tex_table!("ghast" => "ghast", "ghast_shooting"),
                64,
            )],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::HappyGhast,
            anim: AnimationType::Ghast,
            adult: vec![opaque(
                entity_models::flying::bake_happy_ghast_model(),
                tex_table!("ghast" => "happy_ghast"),
                128,
            )],
            baby: Some(opaque(
                entity_models::flying::bake_baby_happy_ghast_model(),
                tex_table!("ghast" => "happy_ghast_baby"),
                64,
            )),
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::MagmaCube,
            anim: AnimationType::Static,
            adult: vec![opaque(
                entity_models::humanoid::bake_magma_cube_model(),
                tex_table!("slime" => "magmacube"),
                64,
            )],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Mooshroom,
            anim: AnimationType::Quadruped,
            adult: vec![opaque(
                entity_model::bake_cow_model(),
                tex_table!("cow" => "mooshroom_red", "mooshroom_brown"),
                64,
            )],
            baby: Some(opaque(
                entity_model::bake_baby_cow_model(),
                tex_table!("cow" => "mooshroom_red_baby", "mooshroom_brown_baby"),
                64,
            )),
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Phantom,
            anim: AnimationType::Phantom,
            adult: vec![opaque(
                entity_models::flying::bake_phantom_model(),
                tex_table!("phantom" => "phantom"),
                64,
            )],
            baby: None,
            adult_overlays: vec![VariantDef {
                model: entity_models::flying::bake_phantom_model(),
                tex_variants: tex_table!("phantom" => "phantom_eyes"),
                tex_size: 64,
                overlay_kind: OverlayKind::EyesTranslucent,
            }],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Shulker,
            anim: AnimationType::Shulker,
            adult: vec![opaque(
                entity_models::humanoid::bake_shulker_model(),
                tex_table!("shulker" => "shulker_white", "shulker_orange", "shulker_magenta", "shulker_light_blue", "shulker_yellow", "shulker_lime", "shulker_pink", "shulker_gray", "shulker_light_gray", "shulker_cyan", "shulker_purple", "shulker_blue", "shulker_brown", "shulker_green", "shulker_red", "shulker_black", "shulker"),
                64,
            )],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::SulfurCube,
            anim: AnimationType::Static,
            adult: vec![
                VariantDef {
                    model: entity_models::humanoid::bake_sulfur_cube_outer_model(),
                    tex_variants: tex_table!("sulfur_cube" => "sulfur_cube_outer"),
                    tex_size: 128,
                    overlay_kind: OverlayKind::BodyTranslucent,
                },
                VariantDef {
                    model: entity_models::humanoid::bake_sulfur_cube_small_outer_model(),
                    tex_variants: tex_table!("sulfur_cube" => "sulfur_cube_outer_small"),
                    tex_size: 64,
                    overlay_kind: OverlayKind::BodyTranslucent,
                },
            ],
            baby: None,
            adult_overlays: vec![
                VariantDef {
                    model: entity_models::humanoid::bake_sulfur_cube_inner_model(),
                    tex_variants: tex_table!("sulfur_cube" => "sulfur_cube_inner"),
                    tex_size: 128,
                    overlay_kind: OverlayKind::BodyTranslucent,
                },
                VariantDef {
                    model: entity_models::humanoid::bake_sulfur_cube_small_inner_model(),
                    tex_variants: tex_table!("sulfur_cube" => "sulfur_cube_inner_small"),
                    tex_size: 64,
                    overlay_kind: OverlayKind::BodyTranslucent,
                },
            ],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Vex,
            anim: AnimationType::Vex,
            adult: vec![opaque(
                entity_models::flying::bake_vex_model(),
                tex_table!("illager" => "vex", "vex_charging"),
                32,
            )],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        },
        MobDef {
            kind: EntityKind::Wither,
            anim: AnimationType::Wither,
            adult: vec![opaque(
                entity_models::flying::bake_wither_model(),
                tex_table!("wither" => "wither", "wither_invulnerable"),
                64,
            )],
            baby: None,
            adult_overlays: vec![VariantDef {
                model: entity_models::flying::bake_wither_model(),
                tex_variants: tex_table!("wither" => "wither_armor"),
                tex_size: 64,
                overlay_kind: OverlayKind::BodyTranslucent,
            }],
            baby_overlays: vec![],
        },
    ];
    macro_rules! boat_def {
        ($kind:ident, $wood:literal, $chest:expr, $raft:expr) => {
            MobDef {
                kind: EntityKind::$kind,
                anim: AnimationType::Boat,
                adult: vec![opaque(
                    if $raft {
                        entity_models::vehicles::bake_raft_model($chest)
                    } else {
                        entity_models::vehicles::bake_boat_model($chest)
                    },
                    if $chest {
                        &[&[concat!(
                            "minecraft/textures/entity/chest_boat/",
                            $wood,
                            ".png"
                        )]]
                    } else {
                        &[&[concat!("minecraft/textures/entity/boat/", $wood, ".png")]]
                    },
                    if $chest { 128 } else { 64 },
                )],
                baby: None,
                adult_overlays: vec![],
                baby_overlays: vec![],
            }
        };
    }
    defs.extend([
        boat_def!(AcaciaBoat, "acacia", false, false),
        boat_def!(AcaciaChestBoat, "acacia", true, false),
        boat_def!(BambooRaft, "bamboo", false, true),
        boat_def!(BambooChestRaft, "bamboo", true, true),
        boat_def!(BirchBoat, "birch", false, false),
        boat_def!(BirchChestBoat, "birch", true, false),
        boat_def!(CherryBoat, "cherry", false, false),
        boat_def!(CherryChestBoat, "cherry", true, false),
        boat_def!(DarkOakBoat, "dark_oak", false, false),
        boat_def!(DarkOakChestBoat, "dark_oak", true, false),
        boat_def!(JungleBoat, "jungle", false, false),
        boat_def!(JungleChestBoat, "jungle", true, false),
        boat_def!(MangroveBoat, "mangrove", false, false),
        boat_def!(MangroveChestBoat, "mangrove", true, false),
        boat_def!(OakBoat, "oak", false, false),
        boat_def!(OakChestBoat, "oak", true, false),
        boat_def!(PaleOakBoat, "pale_oak", false, false),
        boat_def!(PaleOakChestBoat, "pale_oak", true, false),
        boat_def!(SpruceBoat, "spruce", false, false),
        boat_def!(SpruceChestBoat, "spruce", true, false),
    ]);
    for kind in [
        EntityKind::Minecart,
        EntityKind::ChestMinecart,
        EntityKind::FurnaceMinecart,
        EntityKind::TntMinecart,
        EntityKind::HopperMinecart,
        EntityKind::CommandBlockMinecart,
        EntityKind::SpawnerMinecart,
    ] {
        defs.push(MobDef {
            kind,
            anim: AnimationType::Static,
            adult: vec![opaque(
                entity_models::vehicles::bake_minecart_model(),
                tex_table!("minecart" => "minecart"),
                64,
            )],
            baby: None,
            adult_overlays: vec![],
            baby_overlays: vec![],
        });
    }
    for (kind, model, _texture, size, overlay_kind) in [
        (
            // Native getBlockLightLevel is 15. This entity pipeline's existing
            // lighting contract is fixed white vertex light (no sampled sky
            // term), so don't multiply it by world sky at night or add cube-face
            // AO to the native single quad.
            EntityKind::DragonFireball,
            entity_models::projectiles::bake_dragon_fireball_model(),
            "minecraft/textures/entity/enderdragon/dragon_fireball.png",
            16,
            OverlayKind::Opaque,
        ),
        (
            EntityKind::Trident,
            entity_models::projectiles::bake_trident_model(),
            "minecraft/textures/entity/trident/trident.png",
            32,
            OverlayKind::Opaque,
        ),
        (
            EntityKind::ShulkerBullet,
            entity_models::projectiles::bake_shulker_bullet_model(),
            "minecraft/textures/entity/shulker/spark.png",
            64,
            OverlayKind::Opaque,
        ),
        (
            EntityKind::WitherSkull,
            entity_models::projectiles::bake_wither_skull_model(),
            "minecraft/textures/entity/wither/wither.png",
            64,
            OverlayKind::Opaque,
        ),
        (
            EntityKind::LlamaSpit,
            entity_models::projectiles::bake_llama_spit_model(),
            "minecraft/textures/entity/llama/llama_spit.png",
            64,
            OverlayKind::Opaque,
        ),
    ] {
        let tex_variants: &'static [&'static [&'static str]] = match kind {
            EntityKind::DragonFireball => {
                &[&["minecraft/textures/entity/enderdragon/dragon_fireball.png"]]
            }
            EntityKind::Trident => &[&["minecraft/textures/entity/trident/trident.png"]],
            EntityKind::ShulkerBullet => &[&["minecraft/textures/entity/shulker/spark.png"]],
            EntityKind::WitherSkull => &[
                &["minecraft/textures/entity/wither/wither.png"],
                &["minecraft/textures/entity/wither/wither_invulnerable.png"],
            ],
            EntityKind::LlamaSpit => &[&["minecraft/textures/entity/llama/llama_spit.png"]],
            _ => unreachable!(),
        };
        defs.push(MobDef {
            kind,
            anim: AnimationType::Static,
            adult: vec![VariantDef {
                model,
                tex_variants,
                tex_size: size,
                overlay_kind,
            }],
            baby: None,
            adult_overlays: if kind == EntityKind::Trident {
                vec![VariantDef {
                    model: entity_models::projectiles::bake_trident_model(),
                    tex_variants: &[&["minecraft/textures/misc/enchanted_glint_item.png"]],
                    tex_size: 64,
                    overlay_kind: OverlayKind::TridentGlint,
                }]
            } else if kind == EntityKind::ShulkerBullet {
                vec![VariantDef {
                    model: entity_models::projectiles::bake_shulker_bullet_model(),
                    tex_variants: &[&["minecraft/textures/entity/shulker/spark.png"]],
                    tex_size: 64,
                    overlay_kind: OverlayKind::ShulkerBulletOverlay,
                }]
            } else {
                vec![]
            },
            baby_overlays: vec![],
        });
    }
    defs.push(MobDef {
        kind: EntityKind::EndCrystal,
        anim: AnimationType::EndCrystal,
        adult: {
            let model = entity_models::nonliving_special::bake_end_crystal_model();
            let mut no_base = model.clone();
            no_base.part_ranges[3] = (0, 0);
            vec![
                opaque(
                    model,
                    &[&["minecraft/textures/entity/end_crystal/end_crystal.png"]],
                    64,
                ),
                opaque(
                    no_base,
                    &[&["minecraft/textures/entity/end_crystal/end_crystal.png"]],
                    64,
                ),
            ]
        },
        baby: None,
        adult_overlays: vec![],
        baby_overlays: vec![],
    });
    let orb_texture: &'static [&'static [&'static str]] =
        &[&["minecraft/textures/entity/experience/experience_orb.png"]];
    let mut orb_base = entity_models::projectiles::bake_dragon_fireball_model();
    orb_base.vertices.clear();
    orb_base.part_ranges[0].1 = 0;
    defs.push(MobDef {
        kind: EntityKind::ExperienceOrb,
        anim: AnimationType::Static,
        adult: vec![VariantDef {
            model: orb_base,
            tex_variants: orb_texture,
            tex_size: 64,
            overlay_kind: OverlayKind::Opaque,
        }],
        baby: None,
        adult_overlays: vec![VariantDef {
            model: entity_models::projectiles::bake_dragon_fireball_model(),
            tex_variants: orb_texture,
            tex_size: 64,
            overlay_kind: OverlayKind::BodyTranslucent,
        }],
        baby_overlays: vec![],
    });
    defs
}

impl EntityRenderer {
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
        let layouts = [camera_layout, texture_layout];
        let layout_info = vk::PipelineLayoutCreateInfo {
            set_layout_count: layouts.len() as u32,
            set_layouts: layouts.as_ptr(),
            ..Default::default()
        };
        let pipeline_layout = device
            .create_pipeline_layout(&layout_info, None)
            .expect("failed to create entity pipeline layout");

        let [
            pipeline,
            culled_pipeline,
            body_translucent_pipeline,
            eyes_pipeline,
            swirl_pipeline,
            glint_pipeline,
            water_patch_depth_pipeline,
        ] = create_pipelines(device, render_pass, pipeline_layout);

        let defs = mob_definitions();
        let tex_count: u32 = defs
            .iter()
            .map(|d| {
                let mut n: u32 = d.adult.iter().map(|v| v.tex_variants.len() as u32).sum();
                if let Some(b) = &d.baby {
                    n += b.tex_variants.len() as u32;
                }
                for o in &d.adult_overlays {
                    n += o.tex_variants.len() as u32;
                }
                for o in &d.baby_overlays {
                    n += o.tex_variants.len() as u32;
                }
                n
            })
            .sum();
        let tex_count = tex_count + (MAX_PLAYER_SKINS * 2) as u32;

        let pool_sizes = [
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::UniformBuffer,
                descriptor_count: MAX_FRAMES_IN_FLIGHT as u32,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::CombinedImageSampler,
                descriptor_count: tex_count,
            },
        ];
        let pool_info = vk::DescriptorPoolCreateInfo {
            flags: vk::DescriptorPoolCreateFlags::FreeDescriptorSet,
            max_sets: MAX_FRAMES_IN_FLIGHT as u32 + tex_count,
            pool_size_count: pool_sizes.len() as u32,
            pool_sizes: pool_sizes.as_ptr(),
            ..Default::default()
        };
        let descriptor_pool = device
            .create_descriptor_pool(&pool_info, None)
            .expect("failed to create entity descriptor pool");

        let (camera_sets, camera_buffers, camera_allocations) =
            create_camera_sets(device, allocator, descriptor_pool, camera_layout);
        // Per-instance data is a vertex buffer (bound at binding 1), not an SSBO:
        // MoltenVK can't translate a storage-buffer read in a vertex shader.
        let (instance_buffers, instance_allocations) = create_per_frame_host_buffers(
            device,
            allocator,
            (MAX_INSTANCES * size_of::<EntityInstance>()) as u64,
            vk::BufferUsageFlags::VertexBuffer,
            "entity_instances",
        );

        let texture_sampler = unsafe { util::create_nearest_sampler(device) };
        let texture_sampler_repeat = unsafe { util::create_nearest_repeat_sampler(device) };

        let water_patch_model = entity_models::vehicles::bake_boat_water_patch_model();
        let (water_patch_vertex_buffer, water_patch_vertex_allocation) = util::create_mapped_buffer(
            device,
            allocator,
            bytemuck::cast_slice(&water_patch_model.vertices),
            vk::BufferUsageFlags::VertexBuffer,
            "boat_water_patch_vertices",
        );
        let water_patch_range = water_patch_model.part_ranges[0];
        let water_patch_part_transform =
            water_patch_model.compute_part_transforms(&entity_model::PartAnim::default())[0];
        let cape_model = entity_model::bake_player_cape_model();
        let (cape_vertex_buffer, cape_vertex_allocation) = util::create_mapped_buffer(
            device,
            allocator,
            bytemuck::cast_slice(&cape_model.vertices),
            vk::BufferUsageFlags::VertexBuffer,
            "player_cape_vertices",
        );
        let mut mobs = HashMap::new();

        for def in defs {
            let mut build = |v: VariantDef| {
                build_variants(
                    device,
                    queue,
                    command_pool,
                    allocator,
                    descriptor_pool,
                    texture_layout,
                    texture_sampler,
                    texture_sampler_repeat,
                    jar_assets_dir,
                    asset_index,
                    v,
                )
            };
            let adult_variants: Vec<MobVariant> =
                def.adult.into_iter().flat_map(&mut build).collect();
            let baby_variants = def.baby.map(&mut build);
            let mut adult_overlays: Vec<Vec<MobVariant>> =
                def.adult_overlays.into_iter().map(&mut build).collect();
            let mut baby_overlays: Vec<Vec<MobVariant>> =
                def.baby_overlays.into_iter().map(&mut build).collect();

            link_overlays(&adult_variants, &mut adult_overlays);
            if let Some(baby) = &baby_variants {
                link_overlays(baby, &mut baby_overlays);
            }

            if let Some(n) = expected_variant_count(def.kind) {
                assert_eq!(
                    adult_variants.len(),
                    n,
                    "{:?} adult variant pool != variant order length",
                    def.kind
                );
                if let Some(baby) = &baby_variants {
                    assert_eq!(
                        baby.len(),
                        n,
                        "{:?} baby variant pool != variant order length",
                        def.kind
                    );
                }
            }

            mobs.insert(
                def.kind,
                MobEntry {
                    adult_variants,
                    baby_variants,
                    adult_overlays,
                    baby_overlays,
                    anim: def.anim,
                },
            );
        }

        Self {
            pipeline,
            culled_pipeline,
            body_translucent_pipeline,
            eyes_pipeline,
            swirl_pipeline,
            glint_pipeline,
            water_patch_depth_pipeline,
            water_patch_vertex_buffer,
            water_patch_vertex_allocation,
            water_patch_range,
            water_patch_part_transform,
            pipeline_layout,
            camera_layout,
            texture_layout,
            descriptor_pool,
            camera_sets,
            camera_buffers,
            camera_allocations,
            instance_buffers,
            instance_allocations,
            texture_sampler,
            texture_sampler_repeat,
            mobs,
            player_skins: HashMap::new(),
            cape_model,
            cape_vertex_buffer,
            cape_vertex_allocation,
        }
    }

    pub fn update_camera(&mut self, frame: usize, uniform: &CameraUniform) {
        let bytes = bytemuck::bytes_of(uniform);
        self.camera_allocations[frame].mapped_slice_mut().unwrap()[..bytes.len()]
            .copy_from_slice(bytes);
    }

    pub fn update_player_skin(
        &mut self,
        device: &vk::Device,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
        allocator: &Arc<Mutex<Allocator>>,
        uuid: &uuid::Uuid,
        skin: &crate::renderer::SkinData,
    ) {
        if !self.player_skins.contains_key(uuid) && self.player_skins.len() >= MAX_PLAYER_SKINS {
            tracing::warn!("Player skin cache full; keeping fallback texture for {uuid}");
            return;
        }

        let (image, view, allocation) = upload_texture_pixels(
            device,
            queue,
            command_pool,
            allocator,
            &skin.pixels,
            skin.width,
            skin.height,
        );
        let set = if let Some(old) = self.player_skins.get(uuid) {
            old.set
        } else {
            let tex_alloc_info = vk::DescriptorSetAllocateInfo {
                descriptor_pool: self.descriptor_pool,
                descriptor_set_count: 1,
                set_layouts: &self.texture_layout,
                ..Default::default()
            };
            let mut texture_set = vk::DescriptorSet::null();
            device
                .allocate_descriptor_sets(&tex_alloc_info, slice::from_mut(&mut texture_set))
                .expect("failed to allocate player skin texture descriptor set");
            texture_set
        };

        let image_info = vk::DescriptorImageInfo {
            sampler: self.texture_sampler,
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

        let old = self.player_skins.remove(uuid);
        let mut cape_set = old.as_ref().and_then(|old| old.cape_set);
        let cape = skin.cape.as_ref().map(|cape| {
            let (image, view, allocation) = upload_texture_pixels(
                device,
                queue,
                command_pool,
                allocator,
                &cape.pixels,
                cape.width,
                cape.height,
            );
            let descriptor = *cape_set.get_or_insert_with(|| {
                let alloc_info = vk::DescriptorSetAllocateInfo {
                    descriptor_pool: self.descriptor_pool,
                    descriptor_set_count: 1,
                    set_layouts: &self.texture_layout,
                    ..Default::default()
                };
                let mut descriptor = vk::DescriptorSet::null();
                device
                    .allocate_descriptor_sets(&alloc_info, slice::from_mut(&mut descriptor))
                    .expect("failed to allocate player cape texture descriptor set");
                descriptor
            });
            let image_info = vk::DescriptorImageInfo {
                sampler: self.texture_sampler,
                image_view: view,
                image_layout: vk::ImageLayout::ShaderReadOnlyOptimal,
            };
            let write = vk::WriteDescriptorSet {
                dst_set: descriptor,
                dst_binding: 0,
                descriptor_type: vk::DescriptorType::CombinedImageSampler,
                descriptor_count: 1,
                image_info: &image_info,
                ..Default::default()
            };
            device.update_descriptor_sets(&[write], &[]);
            PlayerCapeTexture {
                image,
                view,
                allocation,
            }
        });
        if let Some(old) = old {
            if let Some(cape) = old.cape {
                free_player_cape_texture(device, allocator, cape);
            }
            device.destroy_image_view(old.view, None);
            device.destroy_image(old.image, None);
            allocator.lock().unwrap().free(old.allocation).ok();
        }
        self.player_skins.insert(
            uuid.to_owned(),
            PlayerSkinTexture {
                image,
                view,
                allocation,
                set,
                cape,
                cape_set,
                slim: skin.slim,
            },
        );

        tracing::debug!(
            "Player skin loaded for {uuid}: {}x{}",
            skin.width,
            skin.height
        );
    }

    pub fn remove_player_skin(
        &mut self,
        device: &vk::Device,
        allocator: &Arc<Mutex<Allocator>>,
        uuid: &uuid::Uuid,
    ) {
        if let Some(skin) = self.player_skins.remove(uuid) {
            free_player_skin_texture(device, allocator, self.descriptor_pool, skin);
        }
    }

    pub fn clear_player_skins(&mut self, device: &vk::Device, allocator: &Arc<Mutex<Allocator>>) {
        let descriptor_pool = self.descriptor_pool;
        for (_, skin) in self.player_skins.drain() {
            free_player_skin_texture(device, allocator, descriptor_pool, skin);
        }
    }

    fn player_skin(&self, info: &EntityRenderInfo) -> Option<&PlayerSkinTexture> {
        if !matches!(info.entity_kind, EntityKind::Player | EntityKind::Mannequin) {
            return None;
        }
        self.player_skins.get(info.player_uuid.as_ref()?)
    }

    fn player_texture_set(
        &self,
        info: &EntityRenderInfo,
        fallback: vk::DescriptorSet,
    ) -> vk::DescriptorSet {
        self.player_skin(info).map_or(fallback, |skin| skin.set)
    }

    /// Players pick their model variant (0 = wide, 1 = slim) from the fetched
    /// skin's metadata rather than the caller-supplied index.
    fn effective_variant_index(&self, info: &EntityRenderInfo) -> u32 {
        self.player_skin(info)
            .map_or(info.variant_index, |skin| skin.slim as u32)
    }

    fn compute_anim(
        &self,
        anim_type: AnimationType,
        model: &BakedEntityModel,
        info: &EntityRenderInfo,
    ) -> entity_model::PartAnim {
        // Vanilla `wrapDegrees(headRot - bodyRot)`; matters once a model
        // clamps it (equine +-20).
        let local_head_y = crate::entity::wrap_degrees(info.head_y_rot_deg - info.body_y_rot_deg);
        match anim_type {
            AnimationType::Quadruped => entity_model::compute_quadruped_anim(
                model,
                info.head_x_rot_deg,
                local_head_y,
                info.walk_anim_pos,
                info.walk_anim_speed,
                info.head_y_offset,
                info.head_x_rot_deg_override,
            ),
            AnimationType::Chicken => entity_model::compute_chicken_anim(
                model,
                info.head_x_rot_deg,
                local_head_y,
                info.walk_anim_pos,
                info.walk_anim_speed,
                info.flap,
                info.flap_speed,
            ),
            AnimationType::Humanoid => entity_model::compute_humanoid_anim(
                model,
                info.head_x_rot_deg,
                local_head_y,
                info.walk_anim_pos,
                info.walk_anim_speed,
                info.is_crouching,
            ),
            AnimationType::Enderman => entity_model::compute_enderman_anim(
                model,
                info.head_x_rot_deg,
                local_head_y,
                info.walk_anim_pos,
                info.walk_anim_speed,
                info.age_in_ticks,
                info.is_creepy,
            ),
            AnimationType::Zombie => entity_model::compute_zombie_anim(
                model,
                info.head_x_rot_deg,
                local_head_y,
                info.walk_anim_pos,
                info.walk_anim_speed,
                info.aggressive,
                info.age_in_ticks,
                info.attack_time,
            ),
            AnimationType::Skeleton => entity_model::compute_skeleton_anim(
                model,
                info.head_x_rot_deg,
                local_head_y,
                info.walk_anim_pos,
                info.walk_anim_speed,
                info.aggressive,
                info.age_in_ticks,
            ),
            AnimationType::Spider => entity_model::compute_spider_anim(
                model,
                info.head_x_rot_deg,
                local_head_y,
                info.walk_anim_pos,
                info.walk_anim_speed,
            ),
            AnimationType::Villager => entity_model::compute_villager_anim(
                model,
                info.head_x_rot_deg,
                local_head_y,
                info.walk_anim_pos,
                info.walk_anim_speed,
                info.is_unhappy,
                info.age_in_ticks,
            ),
            AnimationType::Witch => entity_model::compute_witch_anim(
                model,
                info.head_x_rot_deg,
                local_head_y,
                info.walk_anim_pos,
                info.walk_anim_speed,
                info.age_in_ticks,
                info.nose_wobble_speed,
                info.is_holding_item,
            ),
            AnimationType::Wolf => entity_model::compute_wolf_anim(
                model,
                info.head_x_rot_deg,
                local_head_y,
                info.walk_anim_pos,
                info.walk_anim_speed,
                &entity_model::WolfAnimInputs {
                    is_sitting: info.is_sitting,
                    is_angry: info.is_angry,
                    is_baby: info.is_baby,
                    tail_angle: info.tail_angle,
                    head_roll_angle: info.head_roll_angle,
                    shake_anim: info.shake_anim,
                },
            ),
            AnimationType::Feline => entity_model::compute_feline_anim(
                model,
                info.head_x_rot_deg,
                local_head_y,
                info.walk_anim_pos,
                info.walk_anim_speed,
                &entity_model::FelineAnimInputs {
                    is_crouching: info.is_crouching,
                    is_sprinting: info.is_sprinting,
                    is_sitting: info.is_sitting,
                    lie_down_amount: info.lie_down_amount,
                    lie_down_amount_tail: info.lie_down_amount_tail,
                    relax_state_one_amount: info.relax_state_one_amount,
                    is_baby: info.is_baby,
                },
            ),
            AnimationType::Rabbit => entity_model::compute_rabbit_anim(
                model,
                info.head_x_rot_deg,
                local_head_y,
                info.hop_elapsed_secs,
                info.is_baby,
            ),
            AnimationType::Equine => entity_model::compute_equine_anim(
                model,
                info.head_x_rot_deg,
                local_head_y,
                info.walk_anim_pos,
                info.walk_anim_speed,
                info.age_in_ticks,
                &entity_model::EquineAnimInputs {
                    kind: if !info.is_baby {
                        entity_model::EquineKind::Adult
                    } else if matches!(info.entity_kind, EntityKind::Donkey | EntityKind::Mule) {
                        entity_model::EquineKind::BabyDonkey
                    } else {
                        entity_model::EquineKind::BabyHorse
                    },
                    eat_anim: info.eat_anim,
                    stand_anim: info.stand_anim,
                    feeding_anim: info.feeding_anim,
                    animate_tail: info.animate_tail,
                },
            ),
            AnimationType::Squid => entity_model::compute_squid_anim(model, info.tentacle_angle),
            AnimationType::Bat => entity_model::compute_bat_anim(
                model,
                local_head_y,
                info.bat_elapsed_secs,
                info.bat_resting,
            ),
            AnimationType::Fish => entity_model::compute_fish_anim(
                model,
                info.age_in_ticks,
                info.is_in_water,
                info.entity_kind == EntityKind::Pufferfish,
            ),
            AnimationType::Golem => entity_model::compute_golem_anim(
                model,
                info.head_x_rot_deg,
                local_head_y,
                info.walk_anim_pos,
                info.walk_anim_speed,
                info.golem_attack_ticks,
                info.golem_offer_flower_ticks,
            ),
            AnimationType::Vex => {
                let wave = (info.age_in_ticks * 5.5_f32.to_radians()).cos() * 0.1;
                let mut pose = entity_model::PartAnim::default();
                let head_x = info.head_x_rot_deg.to_radians();
                let head_y = local_head_y.to_radians();
                pose.rotation
                    .push((1, glam::Vec3::new(head_x, head_y, 0.0)));
                pose.rotation.push((
                    2,
                    glam::Vec3::new(if info.vex_charging { 0.0 } else { 0.15707964 }, 0.0, 0.0),
                ));
                let wing_y = 1.0995574
                    + (info.age_in_ticks * 45.836624_f32.to_radians()).cos()
                        * 16.2_f32.to_radians();
                pose.rotation
                    .push((5, glam::Vec3::new(0.47123888, wing_y, -0.47123888)));
                pose.rotation
                    .push((6, glam::Vec3::new(0.47123888, -wing_y, 0.47123888)));
                if info.vex_charging {
                    pose.rotation.push((
                        3,
                        glam::Vec3::new(-1.2217305, 0.2617994, -0.47123888 - wave),
                    ));
                    pose.rotation.push((
                        4,
                        glam::Vec3::new(-1.2217305, -0.2617994, 0.47123888 + wave),
                    ));
                }
                pose
            }
            AnimationType::Blaze => {
                let mut pose = entity_model::PartAnim::default();
                let age = info.age_in_ticks;
                for i in 0..12 {
                    let (radius, y, phase) = if i < 4 {
                        (
                            9.0,
                            -2.0 + ((age + (i * 2) as f32) * 0.25).cos(),
                            -age * std::f32::consts::PI * 0.1
                                + i as f32 * std::f32::consts::FRAC_PI_2,
                        )
                    } else if i < 8 {
                        let j = i - 4;
                        (
                            7.0,
                            2.0 + ((age + (i * 2) as f32) * 0.25).cos(),
                            0.7853982
                                + age * std::f32::consts::PI * 0.03
                                + j as f32 * std::f32::consts::FRAC_PI_2,
                        )
                    } else {
                        let j = i - 8;
                        (
                            5.0,
                            11.0 + ((age + i as f32 * 1.5) * 0.5).cos(),
                            0.47123894 - age * std::f32::consts::PI * 0.05
                                + j as f32 * std::f32::consts::FRAC_PI_2,
                        )
                    };
                    let part = &model.parts[i + 1];
                    let target = glam::Vec3::new(radius * phase.cos(), y, radius * phase.sin());
                    pose.translation.push((i + 1, target - part.offset));
                }
                pose.rotation.push((
                    0,
                    glam::Vec3::new(
                        info.head_x_rot_deg.to_radians(),
                        local_head_y.to_radians(),
                        0.0,
                    ),
                ));
                pose
            }
            AnimationType::Allay => entity_model::compute_allay_anim(
                model,
                info.age_in_ticks,
                info.walk_anim_pos,
                info.walk_anim_speed,
            ),
            AnimationType::Bee => {
                entity_model::compute_bee_anim(model, info.age_in_ticks, info.is_on_ground)
            }
            AnimationType::Breeze => entity_model::compute_breeze_anim(
                model,
                info.age_in_ticks,
                info.head_x_rot_deg,
                local_head_y,
            ),
            AnimationType::Ghast => entity_model::compute_ghast_anim(model, info.age_in_ticks),
            AnimationType::Phantom => {
                entity_model::compute_phantom_anim(model, info.age_in_ticks + info.animation_phase)
            }
            AnimationType::Wither => entity_model::compute_wither_anim(
                model,
                info.age_in_ticks,
                info.head_x_rot_deg,
                local_head_y,
            ),
            AnimationType::Shulker => {
                let peek = info.shulker_peek.max(0.0);
                let phase = (0.5 + peek) * std::f32::consts::PI;
                let mut pose = entity_model::PartAnim::default();
                let bob = if phase > std::f32::consts::PI {
                    (info.age_in_ticks * 0.1).sin() * 0.7
                } else {
                    0.0
                };
                // Part translation is in source model pixels; the vanilla lid
                // pivot moves from y=24 to 16 + 8*sin(phase) + bob.
                pose.translation
                    .push((0, glam::Vec3::Y * (16.0 + phase.sin() * 8.0 + bob - 24.0)));
                let lid_y = if peek > 0.3 {
                    (1.0 - phase.sin()).powi(4) * std::f32::consts::PI * 0.125
                } else {
                    0.0
                };
                pose.rotation.push((0, glam::Vec3::new(0.0, lid_y, 0.0)));
                pose.rotation.push((
                    2,
                    glam::Vec3::new(
                        info.head_x_rot_deg.to_radians(),
                        (info.head_y_rot_deg - 180.0 - info.body_y_rot_deg).to_radians(),
                        0.0,
                    ),
                ));
                pose
            }
            AnimationType::Static => entity_model::PartAnim::default(),
            AnimationType::ArmorStand => armor_stand_pose(info),
            AnimationType::EndCrystal => {
                let turn = (info.age_in_ticks * 3.0).to_radians();
                let tilt = std::f32::consts::FRAC_PI_3 / std::f32::consts::SQRT_2;
                entity_model::PartAnim {
                    rotation: vec![
                        (0, glam::Vec3::new(tilt, turn, tilt)),
                        (1, glam::Vec3::new(tilt, turn, tilt)),
                        (2, glam::Vec3::new(tilt, turn, tilt)),
                    ],
                    translation: vec![(
                        0,
                        glam::Vec3::Y * (-end_crystal_y(info.age_in_ticks) * 0.5),
                    )],
                }
            }
            AnimationType::Boat => {
                let paddle = |time: f32, right: bool| {
                    let x_t = ((-time).sin() + 1.0) * 0.5;
                    let y_t = ((-time + 1.0).sin() + 1.0) * 0.5;
                    let x = -std::f32::consts::FRAC_PI_3
                        + (std::f32::consts::FRAC_PI_3 - (std::f32::consts::PI / 12.0)) * x_t;
                    let mut y = -std::f32::consts::FRAC_PI_4 + (std::f32::consts::FRAC_PI_2) * y_t;
                    if right {
                        y = std::f32::consts::PI - y;
                    }
                    (x, y)
                };
                let mut pose = entity_model::PartAnim::default();
                if model.parts.len() >= 3 {
                    let (left_index, right_index) = if model.parts.len() >= 7 {
                        (5, 6)
                    } else {
                        (1, 2)
                    };
                    let (x, y) = paddle(info.boat_rowing_time[0], false);
                    pose.rotation
                        .push((left_index, glam::Vec3::new(x, y, 0.19634955)));
                    let (x, y) = paddle(info.boat_rowing_time[1], true);
                    pose.rotation
                        .push((right_index, glam::Vec3::new(x, y, 0.19634955)));
                }
                pose
            }
        }
    }

    /// The translation is anchor-relative, subtracted in f64 (see
    /// `Camera::anchor`).
    fn entity_matrix(info: &EntityRenderInfo, anchor: glam::DVec3) -> glam::Mat4 {
        if info.entity_kind == EntityKind::ExperienceOrb {
            return glam::Mat4::from_translation((*info.position - anchor).as_vec3())
                * glam::Mat4::from_translation(glam::Vec3::new(0.0, 0.1, 0.0))
                * glam::Mat4::from_rotation_y((180.0 - info.body_y_rot_deg).to_radians())
                * glam::Mat4::from_rotation_x((-info.head_x_rot_deg).to_radians())
                * glam::Mat4::from_scale(glam::Vec3::splat(0.3));
        }
        if info.entity_kind == EntityKind::EndCrystal {
            return glam::Mat4::from_translation((*info.position - anchor).as_vec3())
                * glam::Mat4::from_scale(glam::Vec3::splat(2.0))
                * glam::Mat4::from_translation(glam::Vec3::new(0.0, -0.5, 0.0));
        }
        if matches!(
            info.entity_kind,
            EntityKind::DragonFireball
                | EntityKind::Trident
                | EntityKind::ShulkerBullet
                | EntityKind::WitherSkull
                | EntityKind::LlamaSpit
        ) {
            let position = glam::Mat4::from_translation((*info.position - anchor).as_vec3());
            let (yaw, pitch) = (info.body_y_rot_deg, info.head_x_rot_deg);
            let rotation = match info.entity_kind {
                EntityKind::DragonFireball => {
                    glam::Mat4::from_quat(info.camera_orientation.unwrap_or(glam::Quat::IDENTITY))
                }
                EntityKind::Trident => {
                    glam::Mat4::from_rotation_y((yaw - 90.0).to_radians())
                        * glam::Mat4::from_rotation_z((pitch + 90.0).to_radians())
                }
                EntityKind::WitherSkull => {
                    glam::Mat4::from_rotation_y((180.0 - yaw).to_radians())
                        * glam::Mat4::from_rotation_x(std::f32::consts::PI)
                        * glam::Mat4::from_rotation_y(std::f32::consts::PI)
                }
                EntityKind::LlamaSpit => {
                    glam::Mat4::from_translation(glam::Vec3::Y * 0.15)
                        * glam::Mat4::from_rotation_y((180.0 - yaw).to_radians())
                        * glam::Mat4::from_rotation_x(pitch.to_radians())
                }
                EntityKind::ShulkerBullet => {
                    let age = info.age_in_ticks;
                    glam::Mat4::from_translation(glam::Vec3::Y * 0.15)
                        * glam::Mat4::from_rotation_y((age * 0.1).sin() * std::f32::consts::PI)
                        * glam::Mat4::from_rotation_x((age * 0.1).cos() * std::f32::consts::PI)
                        * glam::Mat4::from_rotation_z((age * 0.15).sin() * std::f32::consts::TAU)
                }
                _ => glam::Mat4::IDENTITY,
            };
            let scale = match info.entity_kind {
                EntityKind::DragonFireball => glam::Vec3::splat(2.0),
                EntityKind::Trident => glam::Vec3::splat(1.0 / 16.0),
                EntityKind::ShulkerBullet => glam::Vec3::new(-0.5, -0.5, 0.5),
                EntityKind::WitherSkull => glam::Vec3::splat(1.0 / 16.0),
                EntityKind::LlamaSpit => glam::Vec3::splat(1.0 / 16.0),
                _ => glam::Vec3::ONE,
            };
            return position * rotation * glam::Mat4::from_scale(scale);
        }
        if info.entity_kind == EntityKind::Arrow {
            // ArrowModel's arrowhead is at x=-12; the entity-model root flip
            // points it along +X. Match vanilla ArrowRenderer's yRot - 90.
            return glam::Mat4::from_translation((*info.position - anchor).as_vec3())
                * glam::Mat4::from_rotation_y((info.body_y_rot_deg - 90.0).to_radians())
                * glam::Mat4::from_rotation_z(info.head_x_rot_deg.to_radians())
                * glam::Mat4::from_scale(glam::Vec3::splat(0.9));
        }
        let mut body_y_rot_deg = info.body_y_rot_deg;
        if info.is_converting {
            // Vanilla `setupRotations` isShaking: a per-tick body-yaw jitter.
            // The addend is a radians-magnitude value applied to degrees —
            // vanilla's own unit mixing, ported literally (~±1.26 degrees).
            // Applied here, after the head-vs-body split, so the head shakes
            // with the body like vanilla.
            body_y_rot_deg += (info.age_in_ticks.floor() * 3.25).cos() * std::f32::consts::PI * 0.4;
        }
        let mut base = glam::Mat4::from_translation((*info.position - anchor).as_vec3())
            * glam::Mat4::from_rotation_y(if info.is_sleeping {
                info.sleeping_yaw_deg.unwrap_or(body_y_rot_deg).to_radians()
            } else {
                (180.0 - body_y_rot_deg).to_radians()
            });
        if info.is_sleeping {
            base *= glam::Mat4::from_rotation_z(std::f32::consts::FRAC_PI_2)
                * glam::Mat4::from_rotation_y(270.0_f32.to_radians());
        }
        if info.death_time > 0.0 {
            base *= glam::Mat4::from_rotation_z(
                death_fall_degrees(info.death_time, info.entity_kind).to_radians(),
            );
        }
        // body_transform sits before the parts (whose root transforms carry
        // the convention's X flip), matching vanilla's setupRotations order.
        let mob_scale = mob_scale(info.entity_kind);
        let mut base = if mob_scale == 1.0 {
            base
        } else {
            base * glam::Mat4::from_scale(glam::Vec3::splat(mob_scale))
        };
        if info.entity_kind == EntityKind::Shulker {
            use azalea_core::direction::Direction as D;
            let outward = match info.shulker_attach_face {
                D::Down => glam::Vec3::Y,
                D::Up => glam::Vec3::NEG_Y,
                D::North => glam::Vec3::Z,
                D::South => glam::Vec3::NEG_Z,
                D::West => glam::Vec3::X,
                D::East => glam::Vec3::NEG_X,
            };
            let center = glam::Vec3::new(0.0, 0.5, 0.0);
            let orient = glam::Mat4::from_translation(center)
                * glam::Mat4::from_quat(glam::Quat::from_rotation_arc(glam::Vec3::Y, outward))
                * glam::Mat4::from_translation(-center);
            base *= orient;
        }
        info.body_transform.map_or(base, |m| base * m)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        cmd: vk::CommandBuffer,
        frame: usize,
        entities: &[EntityRenderInfo],
        frustum: &[[f32; 4]; 6],
        anchor: glam::DVec3,
        eye: glam::DVec3,
        entity_view_scale: f32,
        benchmark_timing: bool,
    ) -> (f32, u32) {
        if entities.is_empty() {
            return (0.0, 0);
        }
        let mut entity_pose_ms = 0.0;
        let mut entity_pose_count = 0;

        // Build the per-frame instance buffer + draw records on the CPU
        // (immutable reads of self.mobs), grouped by variant so each (variant,
        // part) becomes a single instanced draw. `vis`/`groups` borrow self.mobs
        // and are dropped at the end of this block, before the buffer write below.
        let mut instances: Vec<EntityInstance> = Vec::new();
        let (opaque, culled, body, eyes, swirl, glint, water_patch) = {
            let mut vis: Vec<VisEntity> = Vec::new();
            for info in entities {
                let Some(entry) = self.mobs.get(&info.entity_kind) else {
                    continue;
                };
                if !info.skip_cull && !entity_visible(info, frustum, eye, entity_view_scale) {
                    continue;
                }
                let variant = entry.base_variant(info.is_baby, self.effective_variant_index(info));
                let pose_start = benchmark_timing.then(std::time::Instant::now);
                let entity_mat = Self::entity_matrix(info, anchor);
                let anim = self.compute_anim(entry.anim, &variant.model, info);
                // Shared with every overlay that isn't `own_pivots`.
                let part_transforms = variant.model.compute_part_transforms(&anim);
                vis.push(VisEntity {
                    info,
                    entry,
                    entity_mat,
                    anim,
                    part_transforms,
                });
                if let Some(start) = pose_start {
                    entity_pose_ms += start.elapsed().as_secs_f32() * 1000.0;
                    entity_pose_count += 1;
                }
            }
            if vis.is_empty() {
                return (entity_pose_ms, entity_pose_count);
            }

            // Opaque pass: base model + opaque overlays (sheep wool, villager
            // clothing). Overlay layers are exactly coplanar with the base and
            // rely on LessOrEqual depth + draw order to win, so emit in layer
            // phases (all bases, then slot 0 across all entities, then slot
            // 1, ...) — interleaving per entity would let a shared group
            // created by an earlier entity draw a later entity's lower layer
            // after its upper one.
            let mut opaque = VariantGroups::default();
            let mut culled = VariantGroups::default();
            for (vi, v) in vis.iter().enumerate() {
                let base = v
                    .entry
                    .base_variant(v.info.is_baby, self.effective_variant_index(v.info));
                let texture_set = self.player_texture_set(v.info, base.texture_set);
                let group = if base.overlay_kind == OverlayKind::OpaqueCulled {
                    &mut culled
                } else {
                    &mut opaque
                };
                group.add(
                    base,
                    texture_set,
                    (
                        vi,
                        v.info.base_tint,
                        hurt_color(v.info),
                        [0.0, 0.0, 1.0, 1.0],
                    ),
                );
            }
            for slot in 0..MAX_OVERLAYS {
                for (vi, v) in vis.iter().enumerate() {
                    if slot >= v.entry.overlays(v.info.is_baby).len() {
                        continue;
                    }
                    let overlay = v.entry.overlay_variant(
                        v.info.is_baby,
                        slot,
                        v.info.overlay_variants[slot],
                    );
                    let group = match overlay.overlay_kind {
                        OverlayKind::Opaque => &mut opaque,
                        OverlayKind::OpaqueCulled => &mut culled,
                        _ => continue,
                    };
                    if let Some(tint) = v.info.overlay_tints[slot] {
                        group.add(
                            overlay,
                            overlay.texture_set,
                            (vi, tint, hurt_color(v.info), [0.0, 0.0, 1.0, 1.0]),
                        );
                    }
                }
            }

            let mut opaque_records = opaque.emit(&vis, &mut instances);
            opaque_records.extend(collect_player_capes(
                &vis,
                &self.cape_model,
                self.cape_vertex_buffer,
                &self.player_skins,
                &mut instances,
            ));
            let body = collect_overlays(&vis, OverlayKind::BodyTranslucent);
            let eyes = collect_overlays(&vis, OverlayKind::EyesTranslucent);
            let swirl = collect_overlays(&vis, OverlayKind::SwirlAdditive);
            let glint = collect_overlays(&vis, OverlayKind::TridentGlint);
            let mut water_patch = Vec::new();
            for v in &vis {
                if !boat_water_patch_visible(
                    v.info.entity_kind,
                    v.info.boat_underwater,
                    v.info.is_invisible,
                ) {
                    continue;
                }
                let first_instance = instances.len() as u32;
                instances.push(EntityInstance {
                    model: boat_water_patch_matrix(v.entity_mat, self.water_patch_part_transform)
                        .to_cols_array_2d(),
                    tint: [1.0; 4],
                    overlay_color: [0.0; 4],
                    uv_params: [0.0, 0.0, 1.0, 1.0],
                });
                let base = v
                    .entry
                    .base_variant(v.info.is_baby, self.effective_variant_index(v.info));
                let (part_start, part_count) = self.water_patch_range;
                water_patch.push(DrawRecord {
                    texture_set: self.player_texture_set(v.info, base.texture_set),
                    vertex_buffer: self.water_patch_vertex_buffer,
                    part_start,
                    part_count,
                    first_instance,
                    instance_count: 1,
                });
            }

            (
                opaque_records,
                culled.emit(&vis, &mut instances),
                body.emit(&vis, &mut instances),
                eyes.emit(&vis, &mut instances),
                swirl.emit(&vis, &mut instances),
                glint.emit(&vis, &mut instances),
                water_patch,
            )
        };

        // Write the instance buffer (clamped to capacity; the cap is far above any
        // realistic entity count, so overflow only drops the tail with a warning).
        let count = instances.len().min(MAX_INSTANCES);
        if instances.len() > MAX_INSTANCES {
            tracing::warn!(
                "Entity instances ({}) exceed cap {}, dropping excess",
                instances.len(),
                MAX_INSTANCES
            );
        }
        let bytes = bytemuck::cast_slice(&instances[..count]);
        self.instance_allocations[frame].mapped_slice_mut().unwrap()[..bytes.len()]
            .copy_from_slice(bytes);

        self.record_pass(cmd, frame, self.pipeline, &opaque, count);
        self.record_pass(cmd, frame, self.culled_pipeline, &culled, count);
        self.record_pass(cmd, frame, self.body_translucent_pipeline, &body, count);
        self.record_pass(cmd, frame, self.eyes_pipeline, &eyes, count);
        self.record_pass(cmd, frame, self.swirl_pipeline, &swirl, count);
        self.record_pass(cmd, frame, self.glint_pipeline, &glint, count);
        self.record_pass(
            cmd,
            frame,
            self.water_patch_depth_pipeline,
            &water_patch,
            count,
        );
        (entity_pose_ms, entity_pose_count)
    }

    fn record_pass(
        &self,
        cmd: vk::CommandBuffer,
        frame: usize,
        pipeline: vk::Pipeline,
        records: &[DrawRecord],
        count: usize,
    ) {
        if records.is_empty() {
            return;
        }
        cmd.bind_pipeline(vk::PipelineBindPoint::Graphics, pipeline);
        // Per-instance data (binding 1) is the same buffer for the whole pass;
        // gl_InstanceIndex (incl. firstInstance) indexes into it.
        cmd.bind_vertex_buffers(1, &[self.instance_buffers[frame]], &[0]);
        let mut last_vb = vk::Buffer::null();
        let mut last_texture_set = vk::DescriptorSet::null();
        for r in records {
            if r.first_instance as usize + r.instance_count as usize > count {
                continue; // dropped by the capacity clamp above
            }
            if r.vertex_buffer != last_vb || r.texture_set != last_texture_set {
                cmd.bind_descriptor_sets(
                    vk::PipelineBindPoint::Graphics,
                    self.pipeline_layout,
                    0,
                    &[self.camera_sets[frame], r.texture_set],
                    &[],
                );
                cmd.bind_vertex_buffers(0, &[r.vertex_buffer], &[0]);
                last_vb = r.vertex_buffer;
                last_texture_set = r.texture_set;
            }
            cmd.draw(
                r.part_count,
                r.instance_count,
                r.part_start,
                r.first_instance,
            );
        }
    }

    pub fn recreate_pipeline(&mut self, device: &vk::Device, render_pass: vk::RenderPass) {
        device.destroy_pipeline(self.pipeline, None);
        device.destroy_pipeline(self.culled_pipeline, None);
        device.destroy_pipeline(self.body_translucent_pipeline, None);
        device.destroy_pipeline(self.eyes_pipeline, None);
        device.destroy_pipeline(self.swirl_pipeline, None);
        device.destroy_pipeline(self.glint_pipeline, None);
        device.destroy_pipeline(self.water_patch_depth_pipeline, None);
        [
            self.pipeline,
            self.culled_pipeline,
            self.body_translucent_pipeline,
            self.eyes_pipeline,
            self.swirl_pipeline,
            self.glint_pipeline,
            self.water_patch_depth_pipeline,
        ] = create_pipelines(device, render_pass, self.pipeline_layout);
    }

    pub fn destroy(&mut self, device: &vk::Device, allocator: &Arc<Mutex<Allocator>>) {
        let mut alloc = allocator.lock().unwrap();
        for i in 0..MAX_FRAMES_IN_FLIGHT {
            device.destroy_buffer(self.camera_buffers[i], None);
            alloc
                .free(std::mem::replace(&mut self.camera_allocations[i], unsafe {
                    std::mem::zeroed()
                }))
                .ok();
            device.destroy_buffer(self.instance_buffers[i], None);
            alloc
                .free(std::mem::replace(
                    &mut self.instance_allocations[i],
                    unsafe { std::mem::zeroed() },
                ))
                .ok();
        }

        device.destroy_sampler(self.texture_sampler, None);
        device.destroy_sampler(self.texture_sampler_repeat, None);

        for entry in self.mobs.values_mut() {
            let variants: Vec<&mut MobVariant> = entry
                .adult_variants
                .iter_mut()
                .chain(entry.baby_variants.iter_mut().flatten())
                .chain(entry.adult_overlays.iter_mut().flatten())
                .chain(entry.baby_overlays.iter_mut().flatten())
                .collect();
            for v in variants {
                device.destroy_buffer(v.vertex_buffer, None);
                alloc
                    .free(std::mem::replace(&mut v.vertex_allocation, unsafe {
                        std::mem::zeroed()
                    }))
                    .ok();
                device.destroy_image_view(v.texture_view, None);
                alloc
                    .free(std::mem::replace(&mut v.texture_allocation, unsafe {
                        std::mem::zeroed()
                    }))
                    .ok();
                device.destroy_image(v.texture_image, None);
            }
        }
        for (_, skin) in self.player_skins.drain() {
            for set in [Some(skin.set), skin.cape_set].into_iter().flatten() {
                device
                    .free_descriptor_sets(self.descriptor_pool, &[set])
                    .ok();
            }
            destroy_player_skin_texture(device, &mut alloc, skin);
        }
        device.destroy_buffer(self.water_patch_vertex_buffer, None);
        alloc
            .free(std::mem::replace(
                &mut self.water_patch_vertex_allocation,
                unsafe { std::mem::zeroed() },
            ))
            .ok();
        device.destroy_buffer(self.cape_vertex_buffer, None);
        alloc
            .free(std::mem::replace(
                &mut self.cape_vertex_allocation,
                unsafe { std::mem::zeroed() },
            ))
            .ok();

        drop(alloc);

        device.destroy_pipeline(self.pipeline, None);
        device.destroy_pipeline(self.culled_pipeline, None);
        device.destroy_pipeline(self.body_translucent_pipeline, None);
        device.destroy_pipeline(self.eyes_pipeline, None);
        device.destroy_pipeline(self.swirl_pipeline, None);
        device.destroy_pipeline(self.glint_pipeline, None);
        device.destroy_pipeline(self.water_patch_depth_pipeline, None);
        device.destroy_pipeline_layout(self.pipeline_layout, None);
        device.destroy_descriptor_pool(self.descriptor_pool, None);
        device.destroy_descriptor_set_layout(self.camera_layout, None);
        device.destroy_descriptor_set_layout(self.texture_layout, None);
    }
}

/// One host-visible buffer per frame in flight.
fn create_per_frame_host_buffers(
    device: &vk::Device,
    allocator: &Arc<Mutex<Allocator>>,
    size: u64,
    usage: vk::BufferUsageFlags,
    name: &str,
) -> (Vec<vk::Buffer>, Vec<Allocation>) {
    let mut buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
    let mut allocations = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
    for _ in 0..MAX_FRAMES_IN_FLIGHT {
        let (buf, alloc) = util::create_host_buffer(device, allocator, size, usage, name);
        buffers.push(buf);
        allocations.push(alloc);
    }
    (buffers, allocations)
}

/// Per-frame camera UBOs, each bound to its own descriptor set at binding 0.
fn create_camera_sets(
    device: &vk::Device,
    allocator: &Arc<Mutex<Allocator>>,
    pool: vk::DescriptorPool,
    layout: vk::DescriptorSetLayout,
) -> (Vec<vk::DescriptorSet>, Vec<vk::Buffer>, Vec<Allocation>) {
    let layouts: Vec<_> = (0..MAX_FRAMES_IN_FLIGHT).map(|_| layout).collect();
    let alloc_info = vk::DescriptorSetAllocateInfo {
        descriptor_pool: pool,
        descriptor_set_count: layouts.len() as u32,
        set_layouts: layouts.as_ptr(),
        ..Default::default()
    };
    let mut sets = vec![vk::DescriptorSet::null(); layouts.len()];
    device
        .allocate_descriptor_sets(&alloc_info, &mut sets)
        .expect("failed to allocate entity camera descriptor sets");

    let size = size_of::<CameraUniform>() as u64;
    let (buffers, allocations) = create_per_frame_host_buffers(
        device,
        allocator,
        size,
        vk::BufferUsageFlags::UniformBuffer,
        "entity_camera_uniform",
    );
    for (&set, &buffer) in sets.iter().zip(&buffers) {
        let buffer_info = vk::DescriptorBufferInfo {
            buffer,
            offset: 0,
            range: size,
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
    }
    (sets, buffers, allocations)
}

/// A culled, drawable entity with its world transform, animation, and the
/// base model's per-part matrices precomputed.
struct VisEntity<'a> {
    info: &'a EntityRenderInfo,
    entry: &'a MobEntry,
    entity_mat: glam::Mat4,
    anim: entity_model::PartAnim,
    part_transforms: Vec<glam::Mat4>,
}

/// One instanced (variant, part) draw: a run of `instance_count` instances from
/// `first_instance` in the per-frame instance buffer.
struct DrawRecord {
    texture_set: vk::DescriptorSet,
    vertex_buffer: vk::Buffer,
    part_start: u32,
    part_count: u32,
    first_instance: u32,
    instance_count: u32,
}

/// (visible-entity index, tint, overlay color, uv offset+scale) for one
/// instance.
type Member = (usize, [f32; 4], [f32; 4], [f32; 4]);

fn armor_stand_pose(info: &EntityRenderInfo) -> entity_model::PartAnim {
    let mut pose = entity_model::PartAnim::default();
    for (part, rotation) in info.armor_stand_pose.iter().enumerate() {
        pose.rotation.push((
            [0, 1, 3, 2, 5, 4][part],
            glam::Vec3::new(
                rotation[0].to_radians(),
                rotation[1].to_radians(),
                rotation[2].to_radians(),
            ),
        ));
    }
    let body_pose = info.armor_stand_pose[1];
    let body = glam::Vec3::new(
        body_pose[0].to_radians(),
        body_pose[1].to_radians(),
        body_pose[2].to_radians(),
    );
    for part in 6..=8 {
        pose.rotation.push((part, body));
    }
    pose.rotation.push((
        9,
        glam::Vec3::new(0.0, -info.body_y_rot_deg.to_radians(), 0.0),
    ));
    pose
}

fn armor_stand_part_visible(kind: EntityKind, name: &str, flags: u8) -> bool {
    kind != EntityKind::ArmorStand
        || match name {
            "right_arm" | "left_arm" => flags & 0x04 != 0,
            "base_plate" => flags & 0x08 == 0,
            _ => true,
        }
}

pub(crate) fn player_model_part_visible(kind: EntityKind, name: &str, mask: u8) -> bool {
    if !matches!(kind, EntityKind::Player | EntityKind::Mannequin) {
        return true;
    }
    let bit = match name {
        "hat" => 1 << 6,
        "jacket" => 1 << 1,
        "right_sleeve" => 1 << 3,
        "left_sleeve" => 1 << 2,
        "right_pants" => 1 << 5,
        "left_pants" => 1 << 4,
        _ => return true,
    };
    mask & bit != 0
}

/// Visible entities grouped by variant (geometry), so each variant's parts emit
/// one instanced draw covering all its entities.
#[derive(Default)]
struct VariantGroups<'a> {
    groups: Vec<(&'a MobVariant, vk::DescriptorSet, Vec<Member>)>,
}

impl<'a> VariantGroups<'a> {
    fn add(&mut self, variant: &'a MobVariant, texture_set: vk::DescriptorSet, member: Member) {
        let key = variant as *const MobVariant as usize;
        let gi =
            match self.groups.iter().position(|(v, set, _)| {
                *v as *const MobVariant as usize == key && *set == texture_set
            }) {
                Some(gi) => gi,
                None => {
                    self.groups.push((variant, texture_set, Vec::new()));
                    self.groups.len() - 1
                }
            };
        self.groups[gi].2.push(member);
    }

    fn emit(&self, vis: &[VisEntity], instances: &mut Vec<EntityInstance>) -> Vec<DrawRecord> {
        let mut records = Vec::new();
        for (variant, texture_set, members) in &self.groups {
            let own: Option<Vec<Vec<glam::Mat4>>> = variant.own_pivots.then(|| {
                members
                    .iter()
                    .map(|(vi, ..)| variant.model.compute_part_transforms(&vis[*vi].anim))
                    .collect()
            });
            for (p, (start, part_count)) in variant.model.part_ranges.iter().enumerate() {
                if *part_count == 0 {
                    continue;
                }
                let first_instance = instances.len() as u32;
                let mut instance_count = 0;
                for (k, (vi, tint, overlay, uv)) in members.iter().enumerate() {
                    if !player_model_part_visible(
                        vis[*vi].info.entity_kind,
                        &variant.model.parts[p].name,
                        vis[*vi].info.skin_parts_mask,
                    ) || !armor_stand_part_visible(
                        vis[*vi].info.entity_kind,
                        &variant.model.parts[p].name,
                        vis[*vi].info.armor_stand_flags,
                    ) {
                        continue;
                    }
                    let part = match &own {
                        Some(own) => own[k][p],
                        None => vis[*vi].part_transforms[p],
                    };
                    let mut model = vis[*vi].entity_mat * part;
                    if variant.overlay_kind == OverlayKind::ShulkerBulletOverlay {
                        model *= glam::Mat4::from_scale(glam::Vec3::splat(1.5));
                    }
                    instances.push(EntityInstance {
                        model: model.to_cols_array_2d(),
                        tint: *tint,
                        overlay_color: *overlay,
                        uv_params: *uv,
                    });
                    instance_count += 1;
                }
                if instance_count == 0 {
                    continue;
                }
                records.push(DrawRecord {
                    texture_set: *texture_set,
                    vertex_buffer: variant.vertex_buffer,
                    part_start: *start,
                    part_count: *part_count,
                    first_instance,
                    instance_count,
                });
            }
        }
        records
    }
}

fn player_cape_visible(kind: EntityKind, invisible: bool, mask: u8, has_cape: bool) -> bool {
    matches!(kind, EntityKind::Player | EntityKind::Mannequin)
        && !invisible
        && mask & 1 != 0
        && has_cape
}

// WIP: static native idle pose only; moving cloak state is owned by the next
// task.
fn collect_player_capes(
    vis: &[VisEntity<'_>],
    model: &BakedEntityModel,
    vertex_buffer: vk::Buffer,
    player_skins: &HashMap<uuid::Uuid, PlayerSkinTexture>,
    instances: &mut Vec<EntityInstance>,
) -> Vec<DrawRecord> {
    let mut records = Vec::new();
    for entity in vis {
        let Some(skin) = entity
            .info
            .player_uuid
            .as_ref()
            .and_then(|uuid| player_skins.get(uuid))
        else {
            continue;
        };
        if !player_cape_visible(
            entity.info.entity_kind,
            entity.info.is_invisible,
            entity.info.skin_parts_mask,
            skin.cape.is_some(),
        ) {
            continue;
        }
        let Some(texture_set) = skin.cape_set else {
            continue;
        };
        let Some(body_index) = entity
            .entry
            .base_variant(entity.info.is_baby, 0)
            .model
            .parts
            .iter()
            .position(|part| part.name == "body")
        else {
            continue;
        };
        let model_matrix = entity_model::player_cape_attachment_matrix(
            model,
            entity.entity_mat * entity.part_transforms[body_index],
            &entity_model::PartAnim::default(),
        );
        let first_instance = instances.len() as u32;
        instances.push(EntityInstance {
            model: model_matrix.to_cols_array_2d(),
            tint: WHITE_TINT,
            overlay_color: NO_OVERLAY,
            uv_params: [0.0, 0.0, 1.0, 1.0],
        });
        records.push(DrawRecord {
            texture_set,
            vertex_buffer,
            part_start: 0,
            part_count: model.vertices.len() as u32,
            first_instance,
            instance_count: 1,
        });
    }
    records
}

/// Group translucent/emissive overlays by pipeline-compatible material.
fn collect_overlays<'a>(vis: &[VisEntity<'a>], kind: OverlayKind) -> VariantGroups<'a> {
    let mut groups = VariantGroups::default();
    for (vi, v) in vis.iter().enumerate() {
        for slot in 0..v.entry.overlays(v.info.is_baby).len() {
            let overlay =
                v.entry
                    .overlay_variant(v.info.is_baby, slot, v.info.overlay_variants[slot]);
            let wind_in_body_pass = kind == OverlayKind::BodyTranslucent
                && overlay.overlay_kind == OverlayKind::WindScroll;
            let bullet_in_body_pass = kind == OverlayKind::BodyTranslucent
                && overlay.overlay_kind == OverlayKind::ShulkerBulletOverlay;
            let trident_glint = trident_glint_pass_count(
                v.info.entity_kind,
                overlay.overlay_kind,
                v.info.projectile_foil,
            ) == 1;
            if overlay.overlay_kind != kind && !wind_in_body_pass && !bullet_in_body_pass {
                continue;
            }
            if overlay.overlay_kind == OverlayKind::TridentGlint && !trident_glint {
                continue;
            }
            let uv = if v.info.entity_kind == EntityKind::ExperienceOrb {
                experience_orb_uv(v.info.variant_index)
            } else {
                match overlay.overlay_kind {
                    OverlayKind::SwirlAdditive => {
                        let o = (v.info.age_in_ticks * 0.01).rem_euclid(1.0);
                        [o, o, 1.0, 1.0]
                    }
                    OverlayKind::WindScroll => {
                        [(v.info.age_in_ticks * 0.02).rem_euclid(1.0), 0.0, 1.0, 1.0]
                    }
                    OverlayKind::TridentGlint => entity_glint_uv(),
                    _ => [0.0, 0.0, 1.0, 1.0],
                }
            };
            let overlay_color = if overlay.overlay_kind == OverlayKind::BodyTranslucent {
                hurt_color(v.info)
            } else {
                NO_OVERLAY
            };
            let tint = if trident_glint {
                Some(WHITE_TINT)
            } else {
                v.info.overlay_tints[slot]
            };
            if let Some(tint) = tint {
                groups.add(overlay, overlay.texture_set, (vi, tint, overlay_color, uv));
            }
        }
    }
    groups
}

fn trident_glint_pass_count(kind: EntityKind, overlay: OverlayKind, foil: bool) -> u32 {
    u32::from(kind == EntityKind::Trident && overlay == OverlayKind::TridentGlint && foil)
}

/// Vanilla `TextureTransform.ENTITY_GLINT_TEXTURING`: 0.5 scale, 10°
/// rotation, and the native two-period UV scroll (`glintSpeed` defaults to 1).
fn entity_glint_uv() -> [f32; 4] {
    let millis = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64)
        .wrapping_mul(8);
    [
        -((millis % 110_000) as f32 / 110_000.0),
        (millis % 30_000) as f32 / 30_000.0,
        1.0,
        1.0,
    ]
}

fn experience_orb_uv(icon: u32) -> [f32; 4] {
    [
        (icon % 4) as f32 * 0.25,
        (icon / 4) as f32 * 0.25,
        0.25,
        0.25,
    ]
}

fn hurt_color(info: &EntityRenderInfo) -> [f32; 4] {
    if info.has_red_overlay {
        HURT_OVERLAY
    } else {
        NO_OVERLAY
    }
}

const ANIM_MARGIN: f32 = 0.5;

/// Vanilla (width, height) hitbox per supported mob, scaled for babies; used to
/// build the cull bounding sphere.
fn entity_bounds(kind: EntityKind, is_baby: bool) -> (f32, f32) {
    // Vanilla babies declare explicit BABY_DIMENSIONS rather than a scale;
    // list kinds whose constant isn't the half-scale the fallback below
    // assumes. Every new baby mob must be checked against its class.
    if is_baby {
        match kind {
            EntityKind::Chicken => return (0.3, 0.4),
            EntityKind::Rabbit => return (0.24, 0.4),
            EntityKind::Zombie
            | EntityKind::Husk
            | EntityKind::Drowned
            | EntityKind::ZombieVillager
            | EntityKind::Villager => return (0.49, 0.98),
            _ => {}
        }
    }
    let (w, h) = match kind {
        EntityKind::Pig => (0.9, 0.9),
        EntityKind::Cow => (0.9, 1.4),
        EntityKind::Chicken => (0.4, 0.7),
        EntityKind::Sheep => (0.9, 1.3),
        EntityKind::Zombie
        | EntityKind::Husk
        | EntityKind::Drowned
        | EntityKind::ZombieVillager
        | EntityKind::Villager
        | EntityKind::Witch => (0.6, 1.95),
        EntityKind::Skeleton | EntityKind::Stray | EntityKind::Bogged => (0.6, 1.99),
        EntityKind::Creeper => (0.6, 1.7),
        EntityKind::Spider => (1.4, 0.9),
        EntityKind::CaveSpider => (0.98, 0.63),
        EntityKind::Giant => (3.6, 11.7),
        EntityKind::Enderman => (0.6, 2.9),
        EntityKind::Slime => (0.52, 0.52),
        EntityKind::Wolf => (0.6, 0.85),
        EntityKind::Cat | EntityKind::Ocelot => (0.6, 0.7),
        EntityKind::Rabbit => (0.49, 0.6),
        // Horse babies scale 0.7 since 26.1 (`Horse.BABY_DIMENSIONS`; 1.21.x
        // halved, harmless for the cull sphere); donkey/mule babies are the
        // generic half scale.
        EntityKind::Horse | EntityKind::SkeletonHorse | EntityKind::ZombieHorse if is_baby => {
            return (1.3964844 * 0.7, 1.6 * 0.7);
        }
        EntityKind::Horse
        | EntityKind::Mule
        | EntityKind::SkeletonHorse
        | EntityKind::ZombieHorse => (1.3964844, 1.6),
        EntityKind::Donkey => (1.3964844, 1.5),
        // Baby squid dimensions are an explicit 0.5x0.5 in vanilla, not the
        // generic half scale.
        EntityKind::Squid | EntityKind::GlowSquid if is_baby => return (0.5, 0.5),
        EntityKind::Squid | EntityKind::GlowSquid => (0.8, 0.8),
        EntityKind::Bat => (0.5, 0.9),
        EntityKind::Cod => (0.5, 0.3),
        // Salmon/pufferfish scale with their variant; use the largest.
        EntityKind::Salmon => (1.05, 0.6),
        EntityKind::TropicalFish => (0.5, 0.4),
        EntityKind::Pufferfish => (0.7, 0.7),
        EntityKind::IronGolem => (1.4, 2.7),
        EntityKind::Player | EntityKind::Mannequin => (0.6, 1.8),
        _ => (1.0, 1.0),
    };
    let s = if is_baby { 0.5 } else { 1.0 };
    (w * s, h * s)
}

/// Bounding-sphere frustum + distance cull. The frustum planes operate on
/// camera-relative coords (like chunk cull), so the entity position is
/// rebased against the eye in f64 first.
fn entity_visible(
    info: &EntityRenderInfo,
    frustum: &[[f32; 4]; 6],
    eye: glam::DVec3,
    entity_view_scale: f32,
) -> bool {
    let (w, h) = entity_bounds(info.entity_kind, info.is_baby);
    // A body transform (slime size/squish) can grow the entity well past its
    // base bounds: scale the sphere and its center by the largest axis scale,
    // and pad the radius by the translation (pure-rotation transforms still
    // displace pivots — squid pitch, cat lie-down).
    let (scale, shift) = info.body_transform.map_or((1.0, 0.0), |m| {
        let s = m
            .x_axis
            .length_squared()
            .max(m.y_axis.length_squared())
            .max(m.z_axis.length_squared())
            .sqrt()
            .max(1.0);
        (s, m.w_axis.truncate().length())
    });
    let radius = (0.5 * (2.0 * w * w + h * h).sqrt() + ANIM_MARGIN) * scale + shift;
    // Slime's body transform includes its size and an inverse X/Y squish;
    // the cube root of its determinant recovers size without the squish.
    let distance_scale = if info.entity_kind == EntityKind::Slime {
        info.body_transform
            .map_or(1.0, |m| m.determinant().abs().cbrt() / 0.999)
    } else {
        1.0
    };
    let base_range = if info.entity_kind == EntityKind::Arrow {
        // AbstractArrow.shouldRenderAtSqrDistance multiplies AABB getSize by 10.
        // Arrow dimensions are 0.5 x 0.5; spectral arrows use the same model.
        0.5 * 10.0 * 64.0
    } else {
        f64::from((2.0 * w + h) / 3.0 * distance_scale * 64.0)
    };
    if !crate::renderer::entity_distance_visible(
        *info.simulation_position,
        eye,
        base_range,
        entity_view_scale,
    ) {
        return false;
    }
    let mut q = (*info.position - eye).as_vec3();
    q.y += h * 0.5 * scale;
    for pl in frustum {
        if pl[0] * q.x + pl[1] * q.y + pl[2] * q.z + pl[3] < -radius {
            return false;
        }
    }
    true
}

/// Anim part-name indices are computed against the base variant's model and
/// reused for each overlay draw, so overlay part order must match the base
/// (asserted at construction rather than rendering wrong limbs). Overlays
/// whose part poses also match share the base's transforms; the rest are
/// flagged `own_pivots` and get their own.
fn link_overlays(base: &[MobVariant], overlays: &mut [Vec<MobVariant>]) {
    let Some(base_first) = base.first() else {
        return;
    };
    let base_names: Vec<&str> = base_first
        .model
        .parts
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    for overlay in overlays.iter_mut().flatten() {
        let overlay_names: Vec<&str> = overlay
            .model
            .parts
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(
            base_names, overlay_names,
            "overlay part order must match base; anim indices are shared across both"
        );
        overlay.own_pivots = !base_first.model.same_part_poses(&overlay.model);
    }
}

// TODO: share one vertex buffer + model per distinct mesh across texture
// variants (a zombie villager's 33 texture variants clone 2 meshes), and
// batch the per-texture one-time upload submits into one fence wait.
#[allow(clippy::too_many_arguments)]
fn build_variants(
    device: &vk::Device,
    queue: vk::Queue,
    command_pool: vk::CommandPool,
    allocator: &Arc<Mutex<Allocator>>,
    descriptor_pool: vk::DescriptorPool,
    texture_layout: vk::DescriptorSetLayout,
    texture_sampler: vk::Sampler,
    texture_sampler_repeat: vk::Sampler,
    jar_assets_dir: &Path,
    asset_index: &Option<AssetIndex>,
    variant: VariantDef,
) -> Vec<MobVariant> {
    let VariantDef {
        model,
        tex_variants,
        tex_size,
        overlay_kind,
    } = variant;
    // The scrolling swirl needs REPEAT wrapping; everything else clamps.
    let sampler = match overlay_kind {
        OverlayKind::SwirlAdditive | OverlayKind::WindScroll | OverlayKind::TridentGlint => {
            texture_sampler_repeat
        }
        _ => texture_sampler,
    };
    let vert_bytes = bytemuck::cast_slice::<ChunkVertex, u8>(&model.vertices);

    tex_variants
        .iter()
        .map(|tex_keys| {
            let (vertex_buffer, vertex_allocation) = util::create_mapped_buffer(
                device,
                allocator,
                vert_bytes,
                vk::BufferUsageFlags::VertexBuffer,
                "entity_vertices",
            );

            let (texture_image, texture_view, texture_allocation) = load_entity_texture(
                device,
                queue,
                command_pool,
                allocator,
                jar_assets_dir,
                asset_index,
                tex_keys,
                tex_size,
            );

            let tex_alloc_info = vk::DescriptorSetAllocateInfo {
                descriptor_pool,
                descriptor_set_count: 1,
                set_layouts: &texture_layout,
                ..Default::default()
            };
            let mut texture_set = vk::DescriptorSet::null();
            device
                .allocate_descriptor_sets(&tex_alloc_info, slice::from_mut(&mut texture_set))
                .expect("failed to allocate entity texture descriptor set");

            let image_info = vk::DescriptorImageInfo {
                sampler,
                image_view: texture_view,
                image_layout: vk::ImageLayout::ShaderReadOnlyOptimal,
            };
            let tex_write = vk::WriteDescriptorSet {
                dst_set: texture_set,
                dst_binding: 0,
                descriptor_type: vk::DescriptorType::CombinedImageSampler,
                descriptor_count: 1,
                image_info: &image_info,
                ..Default::default()
            };
            device.update_descriptor_sets(&[tex_write], &[]);

            MobVariant {
                model: model.clone(),
                vertex_buffer,
                vertex_allocation,
                texture_image,
                texture_view,
                texture_allocation,
                texture_set,
                overlay_kind,
                own_pivots: false,
            }
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn load_entity_texture(
    device: &vk::Device,
    queue: vk::Queue,
    command_pool: vk::CommandPool,
    allocator: &Arc<Mutex<Allocator>>,
    jar_assets_dir: &Path,
    asset_index: &Option<AssetIndex>,
    asset_keys: &[&str],
    fallback_size: u32,
) -> (vk::Image, vk::ImageView, Allocation) {
    let (pixels, width, height) = asset_keys
        .iter()
        .find_map(|key| {
            let path = resolve_asset_path(jar_assets_dir, asset_index, key);
            util::load_png(&path)
        })
        .unwrap_or_else(|| {
            tracing::warn!(
                "Failed to load entity texture {:?}, using fallback",
                asset_keys
            );
            fallback_texture(fallback_size)
        });

    let (image, view, allocation) =
        util::create_gpu_image(device, allocator, width, height, "entity_texture");
    let (staging_buf, staging_alloc) =
        util::create_staging_buffer(device, allocator, &pixels, "entity_texture_staging");
    util::upload_image(
        device,
        queue,
        command_pool,
        staging_buf,
        image,
        width,
        height,
    );
    device.destroy_buffer(staging_buf, None);
    allocator.lock().unwrap().free(staging_alloc).ok();
    (image, view, allocation)
}

fn upload_texture_pixels(
    device: &vk::Device,
    queue: vk::Queue,
    command_pool: vk::CommandPool,
    allocator: &Arc<Mutex<Allocator>>,
    pixels: &[u8],
    width: u32,
    height: u32,
) -> (vk::Image, vk::ImageView, Allocation) {
    let (image, view, allocation) =
        util::create_gpu_image(device, allocator, width, height, "player_skin_texture");
    let (staging_buf, staging_alloc) =
        util::create_staging_buffer(device, allocator, pixels, "player_skin_texture_staging");
    util::upload_image(
        device,
        queue,
        command_pool,
        staging_buf,
        image,
        width,
        height,
    );
    device.destroy_buffer(staging_buf, None);
    allocator.lock().unwrap().free(staging_alloc).ok();
    (image, view, allocation)
}

fn free_player_skin_texture(
    device: &vk::Device,
    allocator: &Arc<Mutex<Allocator>>,
    descriptor_pool: vk::DescriptorPool,
    skin: PlayerSkinTexture,
) {
    for set in [Some(skin.set), skin.cape_set].into_iter().flatten() {
        device.free_descriptor_sets(descriptor_pool, &[set]).ok();
    }
    let mut alloc = allocator.lock().unwrap();
    destroy_player_skin_texture(device, &mut alloc, skin);
}

fn destroy_player_skin_texture(
    device: &vk::Device,
    allocator: &mut Allocator,
    skin: PlayerSkinTexture,
) {
    if let Some(cape) = skin.cape {
        destroy_player_cape_texture(device, allocator, cape);
    }
    device.destroy_image_view(skin.view, None);
    allocator.free(skin.allocation).ok();
    device.destroy_image(skin.image, None);
}

fn free_player_cape_texture(
    device: &vk::Device,
    allocator: &Arc<Mutex<Allocator>>,
    cape: PlayerCapeTexture,
) {
    let mut alloc = allocator.lock().unwrap();
    destroy_player_cape_texture(device, &mut alloc, cape);
}

fn destroy_player_cape_texture(
    device: &vk::Device,
    allocator: &mut Allocator,
    cape: PlayerCapeTexture,
) {
    device.destroy_image_view(cape.view, None);
    allocator.free(cape.allocation).ok();
    device.destroy_image(cape.image, None);
}

pub(super) fn fallback_texture(size: u32) -> (Vec<u8>, u32, u32) {
    let pixels = [219u8, 148, 148, 255].repeat((size * size) as usize);
    (pixels, size, size)
}

/// The entity render pipelines, in draw order: opaque base, translucent eyes,
/// additive swirl.
fn boat_water_patch_matrix(body_matrix: glam::Mat4, patch_root: glam::Mat4) -> glam::Mat4 {
    body_matrix * patch_root
}

fn boat_water_patch_visible(kind: EntityKind, underwater: bool, invisible: bool) -> bool {
    underwater && !invisible && boat_has_water_patch(kind)
}

fn boat_has_water_patch(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::AcaciaBoat
            | EntityKind::AcaciaChestBoat
            | EntityKind::BirchBoat
            | EntityKind::BirchChestBoat
            | EntityKind::CherryBoat
            | EntityKind::CherryChestBoat
            | EntityKind::DarkOakBoat
            | EntityKind::DarkOakChestBoat
            | EntityKind::JungleBoat
            | EntityKind::JungleChestBoat
            | EntityKind::MangroveBoat
            | EntityKind::MangroveChestBoat
            | EntityKind::OakBoat
            | EntityKind::OakChestBoat
            | EntityKind::PaleOakBoat
            | EntityKind::PaleOakChestBoat
            | EntityKind::SpruceBoat
            | EntityKind::SpruceChestBoat
    )
}

fn create_pipelines(
    device: &vk::Device,
    render_pass: vk::RenderPass,
    layout: vk::PipelineLayout,
) -> [vk::Pipeline; 7] {
    [
        create_pipeline(
            device,
            render_pass,
            layout,
            BlendMode::Opaque,
            ModelInput::Instanced,
        ),
        create_pipeline(
            device,
            render_pass,
            layout,
            BlendMode::OpaqueCulled,
            ModelInput::Instanced,
        ),
        create_pipeline(
            device,
            render_pass,
            layout,
            BlendMode::TranslucentDepthWrite,
            ModelInput::Instanced,
        ),
        create_pipeline(
            device,
            render_pass,
            layout,
            BlendMode::Translucent,
            ModelInput::Instanced,
        ),
        create_pipeline(
            device,
            render_pass,
            layout,
            BlendMode::Additive,
            ModelInput::Instanced,
        ),
        create_pipeline(
            device,
            render_pass,
            layout,
            BlendMode::Glint,
            ModelInput::Instanced,
        ),
        create_pipeline(
            device,
            render_pass,
            layout,
            BlendMode::DepthOnly,
            ModelInput::Instanced,
        ),
    ]
}

/// Source of a draw's model matrix: mobs and closed normal chests can use
/// instance attributes (binding 1); other block entities use per-draw push
/// constants (binding 0 only).
pub(super) enum ModelInput {
    Instanced,
    PushConstant,
}

pub(super) fn create_pipeline(
    device: &vk::Device,
    render_pass: vk::RenderPass,
    layout: vk::PipelineLayout,
    blend: BlendMode,
    model_input: ModelInput,
) -> vk::Pipeline {
    let vert_spv: &[u8] = if blend == BlendMode::Glint {
        shader::include_spirv!("entity_glint.vert.spv").as_slice()
    } else {
        match model_input {
            ModelInput::Instanced => shader::include_spirv!("entity.vert.spv").as_slice(),
            ModelInput::PushConstant => shader::include_spirv!("block_entity.vert.spv").as_slice(),
        }
    };
    let frag_spv: &[u8] = if blend == BlendMode::Glint {
        shader::include_spirv!("entity_glint.frag.spv").as_slice()
    } else {
        shader::include_spirv!("entity.frag.spv").as_slice()
    };

    let vert_module = shader::create_shader_module(device, vert_spv);
    let frag_module = shader::create_shader_module(device, frag_spv);

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

    // Binding 0: per-vertex mesh data. Instanced pipelines add binding 1 with
    // per-instance data (model columns + tint + overlay + uv), one EntityInstance
    // per (entity, part); push-constant ones bind only the mesh.
    let mut bindings = vec![ChunkVertex::binding_description()];
    let mut attrs = ChunkVertex::attribute_descriptions().to_vec();
    if let ModelInput::Instanced = model_input {
        bindings.push(vk::VertexInputBindingDescription {
            binding: 1,
            stride: size_of::<EntityInstance>() as u32,
            input_rate: vk::VertexInputRate::Instance,
        });
        for i in 0..7u32 {
            attrs.push(vk::VertexInputAttributeDescription {
                location: 3 + i,
                binding: 1,
                format: vk::Format::R32G32B32A32Sfloat,
                offset: i * 16,
            });
        }
    }

    let vertex_input = vk::PipelineVertexInputStateCreateInfo {
        vertex_binding_description_count: bindings.len() as u32,
        vertex_binding_descriptions: bindings.as_ptr(),
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
        cull_mode: if matches!(blend, BlendMode::OpaqueCulled | BlendMode::DepthOnly) {
            vk::CullModeFlags::Back
        } else {
            vk::CullModeFlags::None
        },
        front_face: vk::FrontFace::CounterClockwise,
        line_width: 1.0,
        ..Default::default()
    };

    let multisampling = vk::PipelineMultisampleStateCreateInfo {
        rasterization_samples: vk::SampleCountFlags::Type1,
        ..Default::default()
    };

    // Only the translucent eyes overlay skips depth-write (vanilla `EYES`); the
    // opaque base, slime shell, and additive swirl write depth.
    let depth_write = match blend {
        BlendMode::Translucent | BlendMode::Glint => vk::FALSE,
        _ => vk::TRUE,
    };
    let depth_stencil = vk::PipelineDepthStencilStateCreateInfo {
        depth_test_enable: vk::TRUE,
        depth_write_enable: depth_write,
        // Vanilla WATER_MASK uses GREATER_THAN_OR_EQUAL under reversed-Z;
        // pomme's shared scene depth is conventional-Z, so use its equivalent.
        depth_compare_op: if blend == BlendMode::Glint {
            vk::CompareOp::Equal
        } else {
            vk::CompareOp::LessOrEqual
        },
        ..Default::default()
    };

    let blend_attachment = match blend {
        BlendMode::DepthOnly => vk::PipelineColorBlendAttachmentState {
            color_write_mask: vk::ColorComponentFlags::empty(),
            ..Default::default()
        },
        BlendMode::Opaque | BlendMode::OpaqueCulled => vk::PipelineColorBlendAttachmentState {
            blend_enable: vk::FALSE,
            color_write_mask: vk::ColorComponentFlags::RGBA,
            ..Default::default()
        },
        // Standard src-alpha over (glowing eyes, slime shell).
        BlendMode::Translucent | BlendMode::TranslucentDepthWrite => {
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
        }
        // Vanilla glint multiplies destination and source colors.
        BlendMode::Glint => vk::PipelineColorBlendAttachmentState {
            blend_enable: vk::TRUE,
            src_color_blend_factor: vk::BlendFactor::DstColor,
            dst_color_blend_factor: vk::BlendFactor::SrcColor,
            color_blend_op: vk::BlendOp::Add,
            src_alpha_blend_factor: vk::BlendFactor::One,
            dst_alpha_blend_factor: vk::BlendFactor::Zero,
            alpha_blend_op: vk::BlendOp::Add,
            color_write_mask: vk::ColorComponentFlags::RGBA,
        },
        // Additive (energy swirl glow).
        BlendMode::Additive => vk::PipelineColorBlendAttachmentState {
            blend_enable: vk::TRUE,
            src_color_blend_factor: vk::BlendFactor::SrcAlpha,
            dst_color_blend_factor: vk::BlendFactor::One,
            color_blend_op: vk::BlendOp::Add,
            src_alpha_blend_factor: vk::BlendFactor::One,
            dst_alpha_blend_factor: vk::BlendFactor::One,
            alpha_blend_op: vk::BlendOp::Add,
            color_write_mask: vk::ColorComponentFlags::RGBA,
        },
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

    let pipeline_info = [vk::GraphicsPipelineCreateInfo {
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
            &pipeline_info,
            None,
            slice::from_mut(&mut pipeline),
        )
        .expect("failed to create entity pipeline");

    device.destroy_shader_module(vert_module, None);
    device.destroy_shader_module(frag_module, None);

    pipeline
}

#[cfg(test)]
mod tests {
    #[test]
    fn water_patch_scope_geometry_transform_and_visibility_match_native_boats() {
        use azalea_registry::builtin::EntityKind as K;
        use glam::{Mat4, Vec3};

        let boats = [
            K::AcaciaBoat,
            K::AcaciaChestBoat,
            K::BirchBoat,
            K::BirchChestBoat,
            K::CherryBoat,
            K::CherryChestBoat,
            K::DarkOakBoat,
            K::DarkOakChestBoat,
            K::JungleBoat,
            K::JungleChestBoat,
            K::MangroveBoat,
            K::MangroveChestBoat,
            K::OakBoat,
            K::OakChestBoat,
            K::PaleOakBoat,
            K::PaleOakChestBoat,
            K::SpruceBoat,
            K::SpruceChestBoat,
        ];
        assert_eq!(
            boats
                .iter()
                .filter(|&&kind| super::boat_has_water_patch(kind))
                .count(),
            18
        );
        assert!(!super::boat_has_water_patch(K::BambooRaft));
        assert!(!super::boat_has_water_patch(K::BambooChestRaft));
        assert!(!super::boat_water_patch_visible(K::OakBoat, false, false));
        assert!(!super::boat_water_patch_visible(K::OakBoat, true, true));
        assert!(super::boat_water_patch_visible(K::OakBoat, true, false));
        assert!(!super::boat_water_patch_visible(K::BambooRaft, true, false));

        let patch = super::entity_models::vehicles::bake_boat_water_patch_model();
        assert_eq!(patch.vertices.len(), 36);
        assert_eq!(patch.parts[0].offset, Vec3::new(0.0, -3.0, 1.0));
        assert_eq!(patch.parts[0].cubes[0].origin, Vec3::new(-14.0, -9.0, -3.0));
        assert_eq!(patch.parts[0].cubes[0].size, Vec3::new(28.0, 16.0, 3.0));
        let root = patch.compute_part_transforms(&super::entity_model::PartAnim::default())[0];
        let entity = Mat4::from_translation(Vec3::new(5.0, 6.0, 7.0));
        let patch_matrix = super::boat_water_patch_matrix(entity, root);
        assert_eq!(patch_matrix, entity * root);
        assert_eq!(
            patch_matrix.w_axis.truncate(),
            entity.transform_point3(root.w_axis.truncate())
        );
        assert!(patch.part_ranges.iter().all(|&(_, count)| count > 0));
    }

    #[test]
    fn water_patch_pipeline_and_frame_submission_remain_depth_only_and_owned() {
        let source = include_str!("entity_renderer.rs");
        assert!(source.contains("BlendMode::DepthOnly => vk::PipelineColorBlendAttachmentState"));
        assert!(source.contains("color_write_mask: vk::ColorComponentFlags::empty()"));
        assert!(source.contains("depth_write_enable: depth_write"));
        assert!(source.contains("BlendMode::OpaqueCulled | BlendMode::DepthOnly"));
        assert!(source.contains("GREATER_THAN_OR_EQUAL under reversed-Z"));
        assert!(source.contains("depth_compare_op: if blend == BlendMode::Glint"));
        assert!(source.contains("vk::CompareOp::Equal"));
        assert!(source.contains("vk::CompareOp::LessOrEqual"));
        let body_end = source
            .find("self.record_pass(cmd, frame, self.glint_pipeline, &glint, count)")
            .unwrap();
        let patch_pass = source
            .find("self.water_patch_depth_pipeline,\n            &water_patch")
            .unwrap();
        assert!(body_end < patch_pass);
        assert!(source.contains("self.water_patch_vertex_allocation"));
        assert!(source.contains("device.destroy_buffer(self.water_patch_vertex_buffer, None)"));
        assert!(source.contains("device.destroy_pipeline(self.water_patch_depth_pipeline, None)"));
        assert!(source.contains("bytemuck::cast_slice(&water_patch_model.vertices)"));
    }

    #[test]
    fn cape_geometry_attachment_and_draw_predicate_match_native_idle_layer() {
        use azalea_registry::builtin::EntityKind;
        use glam::{Mat4, Vec3};

        let model = super::entity_model::bake_player_cape_model();
        assert_eq!(model.parts.len(), 1);
        assert_eq!(model.vertices.len(), 36);
        assert_eq!(model.parts[0].parent, None); // external player-body parent
        assert_eq!(super::entity_model::PLAYER_CAPE_TEXTURE_SIZE, (64, 64));
        assert_eq!(model.parts[0].offset, Vec3::new(0.0, 0.0, 2.0));
        let cube = model.parts[0].cubes[0];
        assert_eq!(cube.origin, Vec3::new(-5.0, 0.0, -1.0));
        assert_eq!(cube.size, Vec3::new(10.0, 16.0, 1.0));
        assert_eq!(cube.tex_offset, (0, 0));
        let body = Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0));
        let attached = super::entity_model::player_cape_attachment_matrix(
            &model,
            body,
            &super::entity_model::PartAnim::default(),
        );
        assert_eq!(attached.w_axis.truncate(), Vec3::new(1.0, 2.0, 3.125));
        assert!(super::player_cape_visible(
            EntityKind::Player,
            false,
            1,
            true
        ));
        assert!(super::player_cape_visible(
            EntityKind::Mannequin,
            false,
            1,
            true
        ));
        assert!(!super::player_cape_visible(
            EntityKind::Player,
            false,
            0,
            true
        ));
        assert!(!super::player_cape_visible(
            EntityKind::Player,
            true,
            1,
            true
        ));
        assert!(!super::player_cape_visible(
            EntityKind::Player,
            false,
            1,
            false
        ));
        assert!(!super::player_cape_visible(
            EntityKind::Zombie,
            false,
            1,
            true
        ));
    }

    #[test]
    fn trident_foil_has_exactly_one_second_pass_and_reuses_the_base_mesh_pose() {
        use azalea_registry::builtin::EntityKind;
        use pyronyx::vk;

        use crate::renderer::chunk::mesher::ChunkVertex;

        let def = super::mob_definitions()
            .into_iter()
            .find(|def| def.kind == EntityKind::Trident)
            .unwrap();
        let base = &def.adult[0].model;
        let glint = &def.adult_overlays[0];
        assert_eq!(glint.overlay_kind, super::OverlayKind::TridentGlint);
        assert_eq!(
            glint.tex_variants[0][0],
            "minecraft/textures/misc/enchanted_glint_item.png"
        );
        assert!(!base.vertices.is_empty());
        assert_eq!(base.vertices, glint.model.vertices);
        assert_eq!(base.part_ranges, glint.model.part_ranges);
        assert_eq!(
            super::trident_glint_pass_count(EntityKind::Trident, glint.overlay_kind, false),
            0
        );
        assert_eq!(
            super::trident_glint_pass_count(EntityKind::Trident, glint.overlay_kind, true),
            1
        );
        assert_eq!(
            super::trident_glint_pass_count(EntityKind::Arrow, glint.overlay_kind, true),
            0
        );
        assert_eq!(
            base.compute_part_transforms(&super::entity_model::PartAnim::default()),
            glint
                .model
                .compute_part_transforms(&super::entity_model::PartAnim::default())
        );

        // The packed entity vertex stream exposes float3 position + UNORM16x2 UV;
        // item meshes instead carry float UV + tint + packed normal (28-byte stride).
        assert_eq!(ChunkVertex::STRIDE, 20);
        let attrs = ChunkVertex::attribute_descriptions();
        assert_eq!(attrs[0].format, vk::Format::R32G32B32Sfloat);
        assert_eq!(attrs[0].offset, 0);
        assert_eq!(attrs[1].format, vk::Format::R16G16Unorm);
        assert_eq!(attrs[1].offset, 12);
        assert_eq!(attrs[2].format, vk::Format::R8G8B8A8Unorm);
        assert_eq!(attrs[2].offset, 16);
        let item_vertex_source = include_str!("item_entity.rs");
        assert!(item_vertex_source.contains("tex_coords: [f32; 2]"));
        assert!(item_vertex_source.contains("offset: 20"));
        assert!(item_vertex_source.contains("offset: 24"));
    }

    #[test]
    fn entity_glint_scroll_uses_native_two_periods_and_entity_scale() {
        let uv = super::entity_glint_uv();
        assert!((-1.0..=0.0).contains(&uv[0]));
        assert!((0.0..=1.0).contains(&uv[1]));
        assert_eq!(&uv[2..], &[1.0, 1.0]);
        let source = include_str!("../shaders/entity_glint.vert");
        assert!(source.contains("tex_coords * 0.5"));
        assert!(source.contains("0.1745329252"));
        let pipeline_source = include_str!("entity_renderer.rs");
        assert!(pipeline_source.contains("millis % 110_000"));
        assert!(pipeline_source.contains("millis % 30_000"));
        assert!(pipeline_source.contains(".wrapping_mul(8)"));
        assert!(pipeline_source.contains("vk::CompareOp::Equal"));
        assert!(pipeline_source.contains("BlendMode::Glint"));
        assert!(pipeline_source.contains("device.destroy_pipeline(self.glint_pipeline, None)"));
        assert!(pipeline_source.contains("glint_pipeline,\n        ] = create_pipelines"));
    }

    #[test]
    fn dragon_fireball_uses_camera_quaternion_and_full_native_scale() {
        use azalea_registry::builtin::EntityKind;
        use glam::{DVec3, Quat, Vec3};

        let orientation = Quat::from_euler(glam::EulerRot::XYZ, 0.3, -0.7, 0.8);
        let info = super::EntityRenderInfo {
            entity_kind: EntityKind::DragonFireball,
            camera_orientation: Some(orientation),
            ..Default::default()
        };
        let matrix = super::EntityRenderer::entity_matrix(&info, DVec3::ZERO);
        let expected =
            glam::Mat4::from_quat(orientation) * glam::Mat4::from_scale(Vec3::splat(2.0));
        assert!(matrix.abs_diff_eq(expected, 1.0e-6));
        assert!(
            matrix.abs_diff_eq(
                glam::Mat4::from_rotation_y(-0.7)
                    * glam::Mat4::from_rotation_x(0.3)
                    * glam::Mat4::from_scale(Vec3::splat(2.0)),
                1.0e-3
            ) == false,
            "camera roll must not be replaced by yaw/pitch"
        );
    }

    #[test]
    fn shulker_bullet_native_pose_and_overlay_share_the_body_mesh() {
        use azalea_registry::builtin::EntityKind;
        use glam::{DVec3, Vec3};

        let age = 7.25;
        let info = super::EntityRenderInfo {
            entity_kind: EntityKind::ShulkerBullet,
            age_in_ticks: age,
            ..Default::default()
        };
        let matrix = super::EntityRenderer::entity_matrix(&info, DVec3::ZERO);
        let expected = glam::Mat4::from_translation(Vec3::Y * 0.15)
            * glam::Mat4::from_rotation_y((age * 0.1).sin() * std::f32::consts::PI)
            * glam::Mat4::from_rotation_x((age * 0.1).cos() * std::f32::consts::PI)
            * glam::Mat4::from_rotation_z((age * 0.15).sin() * std::f32::consts::TAU)
            * glam::Mat4::from_scale(Vec3::new(-0.5, -0.5, 0.5));
        assert!(matrix.abs_diff_eq(expected, 1.0e-6));
        let defs = super::mob_definitions();
        let bullet = defs
            .iter()
            .find(|d| d.kind == EntityKind::ShulkerBullet)
            .unwrap();
        assert_eq!(bullet.adult[0].overlay_kind, super::OverlayKind::Opaque);
        assert_eq!(bullet.adult_overlays.len(), 1);
        let base = &bullet.adult[0].model;
        let overlay = &bullet.adult_overlays[0].model;
        assert_eq!(base.vertices, overlay.vertices);
        assert_eq!(
            bullet.adult_overlays[0].overlay_kind,
            super::OverlayKind::ShulkerBulletOverlay
        );
        assert_eq!(
            bullet.adult_overlays[0].tex_variants[0][0],
            "minecraft/textures/entity/shulker/spark.png"
        );
    }

    #[test]
    fn experience_orb_uv_stays_within_its_16_pixel_sheet_cell() {
        use super::experience_orb_uv;

        for icon in 0..=10 {
            let [u, v, du, dv] = experience_orb_uv(icon);
            assert_eq!((du, dv), (0.25, 0.25));
            assert!((0.0..=0.75).contains(&u));
            assert!((0.0..=0.5).contains(&v));
            assert!(u + du <= 1.0 && v + dv <= 1.0);
        }
        assert_eq!(experience_orb_uv(10), [0.5, 0.5, 0.25, 0.25]);
    }

    #[test]
    fn experience_orb_uses_native_billboard_transform_scale_and_height_offset() {
        use azalea_registry::builtin::EntityKind;
        use glam::{DVec3, Vec3};

        use super::{EntityRenderInfo, EntityRenderer};
        use crate::entity::components::Position;

        let info = EntityRenderInfo {
            position: Position::new(2.0, 3.0, 4.0),
            entity_kind: EntityKind::ExperienceOrb,
            body_y_rot_deg: 0.0,
            head_x_rot_deg: 0.0,
            ..Default::default()
        };
        let model = EntityRenderer::entity_matrix(&info, DVec3::ZERO);
        assert!((model.x_axis.truncate().length() - 0.3).abs() < 1.0e-6);
        assert!((model.transform_point3(Vec3::ZERO).y - 3.1).abs() < 1.0e-6);
        let lower_left = model.transform_point3(Vec3::new(-0.5, -0.25, 0.0));
        let upper_right = model.transform_point3(Vec3::new(0.5, 0.75, 0.0));
        assert!(((upper_right.x - lower_left.x).abs() - 0.3).abs() < 1.0e-6);
        assert!((upper_right.y - lower_left.y - 0.3).abs() < 1.0e-6);
    }

    #[test]
    fn arrow_tip_points_along_protocol_yaw() {
        use azalea_registry::builtin::EntityKind;
        use glam::{DVec3, Vec3};

        use super::{EntityRenderInfo, EntityRenderer};
        use crate::renderer::entity_model::{PartAnim, bake_arrow_model};

        let model = bake_arrow_model();
        assert_eq!(model.part_ranges.len(), 3);
        assert!(model.part_ranges.iter().all(|&(_, count)| count > 0));
        let cross = model.compute_part_transforms(&PartAnim::default())[1];
        // On the +Z quad, U=0 maps to x=+4: the arrowhead is at this end.
        let tip = Vec3::new(4.0 / 16.0, 0.0, 0.0);
        for (yaw, axis) in [(0.0, -Vec3::Z), (90.0, -Vec3::X)] {
            let info = EntityRenderInfo {
                entity_kind: EntityKind::Arrow,
                body_y_rot_deg: yaw,
                ..Default::default()
            };
            let point =
                (EntityRenderer::entity_matrix(&info, DVec3::ZERO) * cross).transform_point3(tip);
            assert!(
                point.dot(axis) > 0.1,
                "yaw {yaw} must point along {axis:?}: {point:?}"
            );
        }
        let info = EntityRenderInfo {
            entity_kind: EntityKind::Arrow,
            head_x_rot_deg: 30.0,
            ..Default::default()
        };
        let point =
            (EntityRenderer::entity_matrix(&info, DVec3::ZERO) * cross).transform_point3(tip);
        assert!(
            point.y < 0.0,
            "positive vanilla pitch points downward: {point:?}"
        );

        let arrows = super::mob_definitions();
        let arrow = arrows.iter().find(|d| d.kind == EntityKind::Arrow).unwrap();
        assert_eq!(arrow.adult.len(), 2);
        assert!(arrow.adult[1].tex_variants[0][0].ends_with("arrow_spectral.png"));
    }

    #[test]
    fn death_fall_matches_vanilla_boundaries_and_flip_overrides() {
        use azalea_registry::builtin::EntityKind;

        assert_eq!(super::death_fall_degrees(0.0, EntityKind::Zombie), 0.0);
        assert_eq!(super::death_fall_degrees(1.0, EntityKind::Zombie), 0.0);
        assert!(
            (super::death_fall_degrees(6.0, EntityKind::Zombie) - (0.4_f32.sqrt() * 90.0)).abs()
                < 1e-5
        );
        assert_eq!(super::death_fall_degrees(20.0, EntityKind::Zombie), 90.0);
        assert_eq!(super::death_fall_degrees(200.0, EntityKind::Zombie), 90.0);

        for kind in [
            EntityKind::Spider,
            EntityKind::CaveSpider,
            EntityKind::Endermite,
            EntityKind::Silverfish,
        ] {
            assert_eq!(
                super::death_fall_degrees(20.0, kind),
                180.0,
                "vanilla renderer override must use a 180-degree death flip for {kind:?}"
            );
        }
        for kind in [EntityKind::Squid, EntityKind::GlowSquid] {
            assert_eq!(
                super::death_fall_degrees(20.0, kind),
                0.0,
                "Vanilla SquidRenderer bypasses LivingEntityRenderer.setupRotations for {kind:?}"
            );
        }
    }

    #[test]
    fn slime_distance_uses_size_even_when_squished() {
        use azalea_registry::builtin::EntityKind;

        use super::{EntityRenderInfo, entity_visible};
        use crate::entity::components::Position;

        let frustum = [[0.0, 0.0, 0.0, 1000.0]; 6];
        let mut slime = EntityRenderInfo {
            entity_kind: EntityKind::Slime,
            position: Position::new(100.0, 0.0, 0.0),
            simulation_position: Position::new(100.0, 0.0, 0.0),
            ..Default::default()
        };
        assert!(!entity_visible(&slime, &frustum, glam::DVec3::ZERO, 1.0));
        // Size 4, with X/Z halved and Y doubled by squish (0.999 shell shrink).
        slime.body_transform = Some(glam::Mat4::from_scale(
            glam::Vec3::new(2.0, 8.0, 2.0) * 0.999,
        ));
        assert!(entity_visible(&slime, &frustum, glam::DVec3::ZERO, 1.0));
        slime.position = Position::new(145.0, 0.0, 0.0);
        slime.simulation_position = slime.position;
        assert!(!entity_visible(&slime, &frustum, glam::DVec3::ZERO, 1.0));
    }

    #[test]
    fn official_distance_boundaries_for_players_arrows_items_and_frames() {
        use azalea_registry::builtin::EntityKind;
        use glam::DVec3;

        use super::{EntityRenderInfo, entity_visible};
        use crate::entity::components::Position;
        use crate::renderer::{entity_distance_visible, entity_view_scale};

        let frustum = [[0.0, 0.0, 0.0, 1000.0]; 6];
        let visible = |kind, x, percent| {
            entity_visible(
                &EntityRenderInfo {
                    entity_kind: kind,
                    position: Position::new(x, 0.0, 0.0),
                    simulation_position: Position::new(x, 0.0, 0.0),
                    ..Default::default()
                },
                &frustum,
                DVec3::ZERO,
                entity_view_scale(8, percent),
            )
        };
        // Strict position distance (not the padded frustum sphere): player
        // AABB mean = 1, arrow AABB mean = 0.5 times 10.
        assert!(visible(EntityKind::Player, 31.99, 50));
        assert!(!visible(EntityKind::Player, 32.0, 50));
        assert!(visible(EntityKind::Player, 63.99, 100));
        assert!(!visible(EntityKind::Player, 64.0, 100));
        assert!(visible(EntityKind::Arrow, 159.99, 50));
        assert!(!visible(EntityKind::Arrow, 160.0, 50));
        for (percent, cutoff) in [(50, 8.0), (100, 16.0), (500, 80.0)] {
            let scale = entity_view_scale(8, percent);
            assert!(entity_distance_visible(
                DVec3::new(cutoff - 0.01, 0.0, 0.0),
                DVec3::ZERO,
                16.0,
                scale
            ));
            assert!(!entity_distance_visible(
                DVec3::new(cutoff, 0.0, 0.0),
                DVec3::ZERO,
                16.0,
                scale
            ));
            assert_eq!(
                entity_distance_visible(DVec3::X * 30.0, DVec3::ZERO, 16.0, scale),
                percent == 500
            );
            assert_eq!(
                entity_distance_visible(DVec3::X * 70.0, DVec3::ZERO, 16.0, scale),
                percent == 500
            );
        }
        assert!(entity_distance_visible(
            DVec3::X * 511.0,
            DVec3::ZERO,
            1024.0,
            entity_view_scale(8, 50)
        ));
        assert!(!entity_distance_visible(
            DVec3::X * 512.0,
            DVec3::ZERO,
            1024.0,
            entity_view_scale(8, 50)
        ));
        // At high world coordinates the subtraction is still performed in f64.
        let camera = DVec3::new(30_000_000.0, 75.0, 0.0);
        assert!(!entity_distance_visible(
            camera + DVec3::X * 8.0,
            camera,
            16.0,
            entity_view_scale(8, 50)
        ));
    }

    #[test]
    fn player_distance_uses_current_position_not_interpolated_or_visual_offset() {
        use azalea_registry::builtin::EntityKind;
        use glam::DVec3;

        use super::{EntityRenderInfo, entity_visible};
        use crate::entity::components::Position;
        use crate::renderer::entity_view_scale;

        let prev = Position::new(64.7, 0.0, 0.0);
        let now = Position::new(63.7, 0.0, 0.0);
        let frustum = [[0.0, 0.0, 0.0, 1000.0]; 6];
        let scale = entity_view_scale(8, 100);
        let mut player = EntityRenderInfo {
            entity_kind: EntityKind::Player,
            position: prev.lerp(now, 0.0) + DVec3::new(0.5, 0.0, 0.0),
            simulation_position: now,
            ..Default::default()
        };
        assert!(entity_visible(&player, &frustum, DVec3::ZERO, scale));
        player.position = prev.lerp(now, 0.5);
        assert!(entity_visible(&player, &frustum, DVec3::ZERO, scale));
        player.position = prev.lerp(now, 1.0);
        assert!(entity_visible(&player, &frustum, DVec3::ZERO, scale));
        player.simulation_position = Position::new(64.0, 0.0, 0.0);
        assert!(!entity_visible(&player, &frustum, DVec3::ZERO, scale)); // strict edge
        player.simulation_position = now;
        player.position = Position::new(100.0, 0.0, 0.0);
        let frustum = [[-1.0, 0.0, 0.0, 80.0]; 6];
        assert!(!entity_visible(&player, &frustum, DVec3::ZERO, scale));
        player.position = now;
        assert!(entity_visible(&player, &frustum, DVec3::ZERO, scale));
    }

    #[test]
    fn entity_distance_slider_changes_mob_visibility_at_rd8_and_12() {
        use azalea_registry::builtin::EntityKind;

        use super::{EntityRenderInfo, entity_visible};
        use crate::entity::components::Position;
        use crate::renderer::entity_view_scale;

        let frustum = [[0.0, 0.0, 0.0, 1000.0]; 6];
        let mut zombie = EntityRenderInfo {
            entity_kind: EntityKind::Zombie,
            position: Position::new(80.0, 0.0, 0.0),
            simulation_position: Position::new(80.0, 0.0, 0.0),
            ..Default::default()
        };
        let visible = |info: &EntityRenderInfo, rd, percent| {
            entity_visible(
                info,
                &frustum,
                glam::DVec3::ZERO,
                entity_view_scale(rd, percent),
            )
        };
        assert_eq!(entity_view_scale(8, 100), 1.0);
        assert_eq!(entity_view_scale(12, 100), 1.5);
        assert_eq!(entity_view_scale(8, 50), 0.5);
        assert_eq!(entity_view_scale(12, 500), 7.5);
        assert!(!visible(&zombie, 8, 50));
        assert!(!visible(&zombie, 8, 100));
        assert!(visible(&zombie, 8, 500));
        assert!(!visible(&zombie, 12, 50));
        assert!(visible(&zombie, 12, 100));
        assert!(visible(&zombie, 12, 500));
        zombie.position = Position::new(40.0, 0.0, 0.0);
        zombie.simulation_position = zombie.position;
        assert!(!visible(&zombie, 8, 50));
        assert!(visible(&zombie, 12, 50));
    }

    #[test]
    fn giant_and_cave_spider_have_distinct_vanilla_mesh_entries_and_scales() {
        use azalea_registry::builtin::EntityKind;

        let defs = super::mob_definitions();
        for (kind, texture, scale) in [
            (EntityKind::Giant, "zombie/zombie.png", 6.0),
            (EntityKind::CaveSpider, "spider/cave_spider.png", 0.7),
        ] {
            let matches: Vec<_> = defs.iter().filter(|def| def.kind == kind).collect();
            assert_eq!(matches.len(), 1, "{kind:?} must have one MobDef");
            assert!(!matches[0].adult[0].model.vertices.is_empty());
            assert!(matches[0].adult[0].tex_variants[0][0].ends_with(texture));
            assert_eq!(super::mob_scale(kind), scale);
        }
        let cave_spider = defs
            .iter()
            .find(|def| def.kind == EntityKind::CaveSpider)
            .unwrap();
        assert_eq!(cave_spider.adult_overlays.len(), 1);
        assert!(
            cave_spider.adult_overlays[0].tex_variants[0][0].ends_with("spider/spider_eyes.png")
        );
    }

    /// Bakes every mob model; `generate_cube_vertices`' UV seam
    /// `debug_assert!` fires for any mesh that straddles its sheet.
    #[test]
    fn end_crystal_body_uses_registered_bake_variants_and_native_pose() {
        use azalea_registry::builtin::EntityKind;

        let defs = super::mob_definitions();
        let def = defs
            .iter()
            .find(|def| def.kind == EntityKind::EndCrystal)
            .unwrap();
        assert_eq!(def.adult.len(), 2);
        assert_eq!(def.adult[0].model.parts.len(), 4);
        assert!(!def.adult[0].model.vertices.is_empty());
        assert_ne!(
            def.adult[0].model.part_ranges[3],
            def.adult[1].model.part_ranges[3]
        );
        assert_eq!(def.anim, super::AnimationType::EndCrystal);
        assert!((super::end_crystal_y(0.0) + 1.1).abs() < 1.0e-6);
    }

    #[test]
    fn armor_stand_has_real_body_mesh_native_poses_and_visibility_flags() {
        use azalea_registry::builtin::EntityKind;

        use super::EntityRenderInfo;

        let def = super::mob_definitions()
            .into_iter()
            .find(|def| def.kind == EntityKind::ArmorStand)
            .unwrap();
        assert_eq!(def.anim, super::AnimationType::ArmorStand);
        let model = &def.adult[0].model;
        assert_eq!(model.parts.len(), 10);
        assert!(!model.vertices.is_empty());
        assert_eq!(def.adult[0].tex_size, 64);
        assert_eq!(
            def.adult[0].tex_variants[0][0],
            "minecraft/textures/entity/armorstand/armorstand.png"
        );
        assert!(super::armor_stand_part_visible(
            EntityKind::ArmorStand,
            "head",
            0
        ));
        assert!(!super::armor_stand_part_visible(
            EntityKind::ArmorStand,
            "left_arm",
            0
        ));
        assert!(super::armor_stand_part_visible(
            EntityKind::ArmorStand,
            "left_arm",
            0x04
        ));
        assert!(!super::armor_stand_part_visible(
            EntityKind::ArmorStand,
            "base_plate",
            0x08
        ));
        assert!(super::armor_stand_part_visible(
            EntityKind::ArmorStand,
            "head",
            0x10
        ));

        let info = EntityRenderInfo {
            entity_kind: EntityKind::ArmorStand,
            body_y_rot_deg: 90.0,
            armor_stand_pose: [
                [10.0, 20.0, 30.0],
                [40.0, 50.0, 60.0],
                [70.0, 80.0, 90.0],
                [100.0, 110.0, 120.0],
                [130.0, 140.0, 150.0],
                [160.0, 170.0, 180.0],
            ],
            ..Default::default()
        };
        let pose = super::armor_stand_pose(&info);
        assert_eq!(pose.rotation.len(), 10);
        let radians = |xyz: [f32; 3]| {
            glam::Vec3::new(
                xyz[0].to_radians(),
                xyz[1].to_radians(),
                xyz[2].to_radians(),
            )
        };
        assert_eq!(pose.rotation[0], (0, radians([10.0, 20.0, 30.0])));
        assert_eq!(pose.rotation[2], (3, radians([70.0, 80.0, 90.0])));
        assert_eq!(pose.rotation[3], (2, radians([100.0, 110.0, 120.0])));
        assert_eq!(pose.rotation[6], (6, radians([40.0, 50.0, 60.0])));
        assert_eq!(pose.rotation[9].1.y, -std::f32::consts::FRAC_PI_2);
    }

    #[test]
    fn experience_orb_definition_reuses_the_single_dragonfireball_quad_in_translucent_pass() {
        use azalea_registry::builtin::EntityKind;

        let defs = super::mob_definitions();
        let def = defs
            .iter()
            .find(|def| def.kind == EntityKind::ExperienceOrb)
            .unwrap();
        assert!(def.adult[0].model.vertices.is_empty());
        assert_eq!(def.adult_overlays.len(), 1);
        let orb = &def.adult_overlays[0];
        assert!(orb.overlay_kind == super::OverlayKind::BodyTranslucent);
        assert_eq!(orb.model.vertices.len(), 6);
        assert_eq!(orb.tex_size, 64);
        assert_eq!(
            orb.tex_variants[0][0],
            "minecraft/textures/entity/experience/experience_orb.png"
        );
    }

    #[test]
    fn all_mob_meshes_bake_and_definitions_are_unique() {
        let defs = super::mob_definitions();
        let mut kinds = std::collections::HashSet::new();
        for def in &defs {
            assert!(
                kinds.insert(def.kind),
                "duplicate MobDef for {:?}",
                def.kind
            );
            for variant in def
                .adult
                .iter()
                .chain(def.baby.iter())
                .chain(&def.adult_overlays)
                .chain(&def.baby_overlays)
            {
                assert!(
                    !variant.model.vertices.is_empty(),
                    "empty model for {:?}",
                    def.kind
                );
                assert!(
                    !variant.tex_variants.is_empty(),
                    "empty texture pool for {:?}",
                    def.kind
                );
            }
        }
    }

    #[test]
    fn remaining_thirteen_living_bodies_are_registered_with_nonempty_geometry() {
        use azalea_registry::builtin::EntityKind as K;

        let defs = super::mob_definitions();
        for kind in [
            K::Allay,
            K::Bee,
            K::Blaze,
            K::Breeze,
            K::Ghast,
            K::HappyGhast,
            K::MagmaCube,
            K::Mooshroom,
            K::Phantom,
            K::Shulker,
            K::SulfurCube,
            K::Vex,
            K::Wither,
        ] {
            let def = defs
                .iter()
                .find(|def| def.kind == kind)
                .unwrap_or_else(|| panic!("missing {kind:?}"));
            assert!(!def.adult.is_empty());
            assert!(
                def.adult
                    .iter()
                    .all(|variant| !variant.model.vertices.is_empty())
            );
            assert!(
                def.adult
                    .iter()
                    .all(|variant| !variant.tex_variants.is_empty())
            );
        }
        let ghast = defs.iter().find(|def| def.kind == K::Ghast).unwrap();
        assert_eq!(ghast.adult[0].model.part_scales[0], 4.5);
        let happy = defs.iter().find(|def| def.kind == K::HappyGhast).unwrap();
        assert_eq!(happy.adult[0].model.part_scales[0], 4.0);
        assert_eq!(happy.baby.as_ref().unwrap().model.part_scales[0], 0.95);
        let shulker = defs.iter().find(|def| def.kind == K::Shulker).unwrap();
        assert_eq!(shulker.adult[0].tex_variants.len(), 17);
        let sulfur = defs.iter().find(|def| def.kind == K::SulfurCube).unwrap();
        assert_eq!(sulfur.adult.len(), 2);
        assert_eq!(sulfur.adult_overlays.len(), 2);
        let mooshroom = defs.iter().find(|def| def.kind == K::Mooshroom).unwrap();
        assert!(mooshroom.baby.is_some());
        assert!(mooshroom.adult[0].tex_variants[0][0].ends_with("cow/mooshroom_red.png"));
        assert!(mooshroom.adult[0].tex_variants[1][0].ends_with("cow/mooshroom_brown.png"));
    }

    #[test]
    fn twenty_boat_forms_have_dedicated_geometry_and_exact_texture_paths() {
        use azalea_registry::builtin::EntityKind as K;

        let expected = [
            (K::AcaciaBoat, "boat/acacia.png", false),
            (K::AcaciaChestBoat, "chest_boat/acacia.png", true),
            (K::BambooRaft, "boat/bamboo.png", false),
            (K::BambooChestRaft, "chest_boat/bamboo.png", true),
            (K::BirchBoat, "boat/birch.png", false),
            (K::BirchChestBoat, "chest_boat/birch.png", true),
            (K::CherryBoat, "boat/cherry.png", false),
            (K::CherryChestBoat, "chest_boat/cherry.png", true),
            (K::DarkOakBoat, "boat/dark_oak.png", false),
            (K::DarkOakChestBoat, "chest_boat/dark_oak.png", true),
            (K::JungleBoat, "boat/jungle.png", false),
            (K::JungleChestBoat, "chest_boat/jungle.png", true),
            (K::MangroveBoat, "boat/mangrove.png", false),
            (K::MangroveChestBoat, "chest_boat/mangrove.png", true),
            (K::OakBoat, "boat/oak.png", false),
            (K::OakChestBoat, "chest_boat/oak.png", true),
            (K::PaleOakBoat, "boat/pale_oak.png", false),
            (K::PaleOakChestBoat, "chest_boat/pale_oak.png", true),
            (K::SpruceBoat, "boat/spruce.png", false),
            (K::SpruceChestBoat, "chest_boat/spruce.png", true),
        ];
        let defs = super::mob_definitions();
        for (kind, texture, chest) in expected {
            let matches: Vec<_> = defs.iter().filter(|def| def.kind == kind).collect();
            assert_eq!(matches.len(), 1, "{kind:?} has one dedicated renderer def");
            let def = matches[0];
            let variant = &def.adult[0];
            assert_eq!(def.anim, super::AnimationType::Boat);
            assert!(!variant.model.vertices.is_empty());
            let raft = matches!(kind, K::BambooRaft | K::BambooChestRaft);
            assert_eq!(
                variant.model.parts.len(),
                match (raft, chest) {
                    (false, false) => 7,
                    (false, true) => 10,
                    (true, false) => 3,
                    (true, true) => 6,
                }
            );
            assert!(variant.tex_variants[0][0].ends_with(texture));
            assert!(def.baby.is_none());
        }
    }

    #[test]
    fn pig_variant_pool_uses_cold_geometry_and_matching_baby_textures() {
        use azalea_registry::builtin::EntityKind;

        let defs = super::mob_definitions();
        let pig = defs.iter().find(|d| d.kind == EntityKind::Pig).unwrap();
        assert_eq!(pig.adult.len(), 3);
        assert_eq!(pig.baby.as_ref().unwrap().tex_variants.len(), 3);
        assert!(pig.adult[1].model.vertices.len() > pig.adult[0].model.vertices.len());
        assert!(pig.adult[1].tex_variants[0][0].ends_with("pig/pig_cold.png"));
        assert!(pig.baby.as_ref().unwrap().tex_variants[1][0].ends_with("pig/pig_cold_baby.png"));
        assert_eq!(pig.baby.as_ref().unwrap().tex_size, 32);
    }
}
