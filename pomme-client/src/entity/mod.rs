pub mod components;
mod projectile;
pub mod villager;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use projectile::{Flight, Frame, HORIZON, Input, Job, MAX_BATCH, THRESHOLD, Worker};

static WORLD_EPOCH: AtomicU64 = AtomicU64::new(1);
static PROJECTILE_REVISION: AtomicU64 = AtomicU64::new(1);

use azalea_core::position::{BlockPos, ChunkPos};
use azalea_registry::builtin::EntityKind;
use glam::DVec3;

use crate::entity::components::{LookDirection, Position};
use crate::entity::villager::{VillagerKind, VillagerProfession};
use crate::physics::aabb::Aabb;
use crate::physics::collision::resolve_collision;
use crate::world::block::{FluidKind, fluid};
use crate::world::chunk::ChunkStore;

/// A scalar synched-entity-data value, forwarded raw from the wire;
/// [`EntityStore::apply_entity_data`] gives it meaning per (kind, index).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MetaValue {
    Bool(bool),
    Int(i32),
    Byte(u8),
    Float(f32),
    Long(i64),
    OptionalBlockState(Option<u32>),
    OptionalBlockPos(Option<BlockPos>),
    BlockPos(BlockPos),
    BlockState(u32),
    Direction(azalea_core::direction::Direction),
}

/// `AgeableMob` descendants on every supported version (Slime joined only
/// in 26.2, so it's excluded here and special-cased where it matters).
fn is_ageable_mob(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Pig
            | EntityKind::Cow
            | EntityKind::Mooshroom
            | EntityKind::Bee
            | EntityKind::HappyGhast
            | EntityKind::MagmaCube
            | EntityKind::SulfurCube
            | EntityKind::Sheep
            | EntityKind::Chicken
            | EntityKind::Villager
            | EntityKind::Wolf
            | EntityKind::Cat
            | EntityKind::Ocelot
            | EntityKind::Rabbit
            | EntityKind::Squid
            | EntityKind::GlowSquid
    ) || is_equine(&kind)
}

/// Kinds whose entity-data index 16 is the baby flag: `AgeableMob`
/// descendants plus the zombie family (which defines its own baby flag at
/// the same index). NOT baby at 16: Bogged (sheared), Skeleton (stray
/// conversion), Witch (Raider celebrating), fish (from-bucket).
fn is_baby_kind(kind: EntityKind) -> bool {
    is_ageable_mob(kind)
        || matches!(
            kind,
            EntityKind::Slime
                | EntityKind::Zombie
                | EntityKind::Husk
                | EntityKind::Drowned
                | EntityKind::ZombieVillager
        )
}

/// 26.1 (protocol 775) added `AgeableMob`'s age-locked flag at 17, pushing
/// every subclass index up by one; older wire versions are lifted to the
/// 26.x numbering so `apply_entity_data` matches one index per field.
fn normalize_ageable_index(kind: EntityKind, index: u8) -> u8 {
    if is_ageable_mob(kind) && index >= 17 && crate::version::session_protocol() < 775 {
        index + 1
    } else {
        index
    }
}

/// 1.21.9 (protocol 773) reordered `Avatar`'s synched data: main hand moved
/// 18 -> 15, pushing absorption to 17, score to 18, and mode customisation
/// to 16 (the shoulder compounds at 19/20 were dropped; the frame
/// translator strips them). Older wire versions are lifted to the 26.x
/// numbering so `apply_entity_data` matches one index per field.
fn normalize_player_index(kind: EntityKind, index: u8) -> u8 {
    normalize_player_index_at(kind, index, crate::version::session_protocol())
}

fn normalize_player_index_at(kind: EntityKind, index: u8, protocol: i32) -> u8 {
    if kind != EntityKind::Player || protocol > 772 {
        return index;
    }
    match index {
        15 => 17, // absorption
        16 => 18, // score
        17 => 16, // mode customisation
        18 => 15, // main hand
        i => i,
    }
}

const INTERPOLATION_STEPS: i32 = 3;
/// Vanilla `LivingEntity.hurtDuration`, never assigned anything but 10.
pub const HURT_DURATION: u8 = 10;
/// Vanilla default arm-swing duration in ticks
/// (`LivingEntity.getCurrentSwingDuration`).
const SWING_DURATION: u8 = 6;

fn tick_death_time(health: f32, should_tick: bool, death_time: &mut u32) {
    if health <= 0.0 && should_tick {
        *death_time = death_time.wrapping_add(1);
    }
}

fn within_simulation_distance(entity: Position, player: Position, distance: u32) -> bool {
    let entity_x = (entity.x.floor() as i32).div_euclid(16);
    let entity_z = (entity.z.floor() as i32).div_euclid(16);
    let player_x = (player.x.floor() as i32).div_euclid(16);
    let player_z = (player.z.floor() as i32).div_euclid(16);
    entity_x.abs_diff(player_x).max(entity_z.abs_diff(player_z)) <= distance
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EntityPose {
    #[default]
    Standing,
    FallFlying,
    Sleeping,
    Swimming,
    SpinAttack,
    Crouching,
    LongJumping,
    Dying,
    Croaking,
    UsingTongue,
    Sitting,
    Roaring,
    Sniffing,
    Emerging,
    Digging,
    Sliding,
    Shooting,
    Inhaling,
}

impl EntityPose {
    /// 26.2 `Pose.BY_ID`: continuous ordinal map, with unknown ids falling back
    /// to STANDING.
    pub fn from_vanilla_id(id: i32) -> Self {
        match id {
            1 => Self::FallFlying,
            2 => Self::Sleeping,
            3 => Self::Swimming,
            4 => Self::SpinAttack,
            5 => Self::Crouching,
            6 => Self::LongJumping,
            7 => Self::Dying,
            8 => Self::Croaking,
            9 => Self::UsingTongue,
            10 => Self::Sitting,
            11 => Self::Roaring,
            12 => Self::Sniffing,
            13 => Self::Emerging,
            14 => Self::Digging,
            15 => Self::Sliding,
            16 => Self::Shooting,
            17 => Self::Inhaling,
            _ => Self::Standing,
        }
    }
}

/// Vanilla Entity shared-flags byte (bits 0, 3..7); crouching is a pose.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EntityFlags {
    pub on_fire: bool,
    pub sprinting: bool,
    pub swimming: bool,
    pub invisible: bool,
    pub glowing: bool,
    pub fall_flying: bool,
}

impl From<u8> for EntityFlags {
    fn from(flags: u8) -> Self {
        Self {
            on_fire: flags & 0x01 != 0,
            sprinting: flags & 0x08 != 0,
            swimming: flags & 0x10 != 0,
            invisible: flags & 0x20 != 0,
            glowing: flags & 0x40 != 0,
            fall_flying: flags & 0x80 != 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EntityEffect {
    pub effect_id: u32,
    pub duration: i32,
    pub amplifier: u8,
    pub ambient: bool,
    pub show_particles: bool,
    pub show_icon: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntityDimensions {
    pub width: f64,
    pub height: f64,
}

pub struct LivingEntity {
    pub position: Position,
    pub prev_position: Position,
    pub look_dir: LookDirection,
    pub prev_look_dir: LookDirection,
    pub head_y_rot_deg: f32,
    pub prev_head_y_rot_deg: f32,
    pub body_y_rot_deg: f32,
    pub prev_body_y_rot_deg: f32,
    pub entity_type: EntityKind,
    pub player_uuid: Option<uuid::Uuid>,
    pub walk_anim_pos: f32,
    pub walk_anim_speed: f32,
    pub prev_walk_anim_speed: f32,
    pub is_baby: bool,
    pub is_crouching: bool,
    pub pose: EntityPose,
    pub sleeping_pos: Option<BlockPos>,
    pub flags: EntityFlags,
    /// LivingEntity DATA_LIVING_ENTITY_FLAGS: using-item hand bits / riptide
    /// bit.
    pub using_item: bool,
    pub using_offhand: bool,
    /// Avatar DATA_PLAYER_MAIN_HAND; defaults right until its metadata arrives.
    pub main_arm_right: bool,
    /// Avatar DATA_PLAYER_MODE_CUSTOMISATION from remote entity metadata.
    pub skin_parts_mask: Option<u8>,
    pub riptide_spin: bool,
    pub attributes: HashMap<String, f64>,
    pub effects: HashMap<u32, EntityEffect>,
    pub on_ground: bool,
    pub wool_color: Option<u8>,
    /// Sheep wool shorn / bogged mushrooms shorn.
    pub is_sheared: bool,
    /// Registry/wire variant slot; meaning is per-kind. Holder-backed values
    /// are pre-resolved to pool indices by the net handler; raw-int kinds are
    /// normalized in `EntityStore::apply_entity_data`.
    pub variant: u32,
    /// Species metadata, exact field meanings are resolved below from 26.2
    /// indices.
    pub bee_flags: u8,
    pub ghast_charging: bool,
    pub vex_charging: bool,
    pub phantom_size: i32,
    pub shulker_peek: u8,
    pub shulker_peek_amount: f32,
    pub prev_shulker_peek_amount: f32,
    pub shulker_attach_face: azalea_core::direction::Direction,
    pub sulfur_cube_size: i32,
    pub wither_invulnerability: i32,
    /// Chicken wing-flap state (vanilla `Chicken.aiStep`): `flap` is the
    /// unbounded wing-cycle phase, `flap_speed` the 0..1 amplitude.
    pub flap: f32,
    pub prev_flap: f32,
    pub flap_speed: f32,
    pub prev_flap_speed: f32,
    /// Slime squish spring (vanilla `AbstractCubeMob`): negative = squashed
    /// on landing, positive = stretched in the air.
    pub squish: f32,
    pub prev_squish: f32,
    pub slime_size: u8,
    /// Enderman screaming flag — raises the head and jitters the render
    /// position.
    pub is_creepy: bool,
    /// Zombie-family conversion (drowning / villager cure) — body-yaw shake.
    pub is_converting: bool,
    /// Witch drinking flag — swings the nose down toward the potion.
    pub witch_drinking: bool,
    /// Tamable (wolf/cat) state from the flags byte.
    pub is_sitting: bool,
    pub is_tame: bool,
    pub is_sprinting: bool,
    /// Dye id, wolf/cat collars (vanilla default red).
    pub collar_color: u8,
    pub is_interested: bool,
    /// Vanilla persistent-anger end time; angry while > current game time.
    pub anger_end_time: i64,
    /// Metadata health; drives the tame wolf's tail angle.
    pub health: f32,
    /// `max_health` attribute (`UpdateAttributes`); sizes the mount heart row.
    pub max_health: f32,
    pub interested_angle: f32,
    pub prev_interested_angle: f32,
    pub shake_anim: f32,
    pub prev_shake_anim: f32,
    pub is_lying: bool,
    pub relax_state_one: bool,
    pub lie_down_amount: f32,
    pub prev_lie_down_amount: f32,
    pub lie_down_amount_tail: f32,
    pub prev_lie_down_amount_tail: f32,
    pub relax_state_one_amount: f32,
    pub prev_relax_state_one_amount: f32,
    /// Tick the rabbit hop keyframe clock started at, while hopping.
    pub hop_anim_start: Option<u32>,
    /// Equine flag-byte state (grass eating, rearing, open mouth).
    pub is_eating: bool,
    pub is_standing: bool,
    pub is_open_mouth: bool,
    pub eat_anim: f32,
    pub prev_eat_anim: f32,
    pub stand_anim: f32,
    pub prev_stand_anim: f32,
    pub mouth_anim: f32,
    pub prev_mouth_anim: f32,
    pub has_chest: bool,
    /// Saddle equipment slot occupied (`SetEquipment`); gates the jump bar.
    pub saddled: bool,
    /// Latest server equipment updates, including the native body slot.
    pub equipment:
        HashMap<azalea_inventory::components::EquipmentSlot, azalea_inventory::ItemStack>,
    /// Local AbstractHorse.onPlayerJump charge, consumed on the next grounded
    /// tick.
    pub horse_jump_pending_scale: f32,
    /// Packet-driven velocity (vanilla remote entities never integrate their
    /// own); feeds the squid body-rotation sim.
    pub velocity: DVec3,
    /// Vanilla `wasTouchingWater`, probed per tick for aquatic kinds.
    pub is_in_water: bool,
    /// Squid client sim (vanilla `Squid.aiStep`), degrees.
    pub x_body_rot: f32,
    pub prev_x_body_rot: f32,
    pub z_body_rot: f32,
    pub prev_z_body_rot: f32,
    pub tentacle_angle: f32,
    pub prev_tentacle_angle: f32,
    pub bat_resting: bool,
    /// Tick the bat's current fly/rest animation started at.
    pub bat_anim_start: Option<u32>,
    pub puff_state: u8,
    /// Glow squid post-hurt dim timer, synced then decremented client-side.
    pub dark_ticks: i32,
    /// Iron golem punch / flower-offer countdowns (entity events 4, 11, 34).
    pub golem_attack_ticks: u8,
    pub golem_offer_flower_ticks: u16,
    pub villager_kind: VillagerKind,
    pub villager_profession: VillagerProfession,
    pub villager_level: u32,
    /// Villager head-shake timer; shakes while > 0 (vanilla unhappy counter,
    /// synched then decremented client-side each tick like vanilla does).
    pub unhappy_counter: i32,
    pub eat_anim_tick: u8,
    pub prev_eat_anim_tick: u8,
    pub hurt_time: u8,
    pub death_time: u32,
    pub age_in_ticks: u32,
    pub custom_name: Option<String>,
    /// Mob is targeting/attacking (metadata mob-flags bit 0x04). Raises
    /// zombie/skeleton arms.
    pub aggressive: bool,
    /// Creeper charged/powered flag — shows the blue aura overlay.
    pub powered: bool,
    /// Arm-swing animation timer, counts down from `SWING_DURATION` to 0
    /// (driven by the server `Animate` packet). Drives the zombie attack
    /// swing.
    pub swing_time: u8,
    /// Chicken `flapping` decay factor.
    flapping: f32,
    target_squish: f32,
    prev_on_ground: bool,
    is_shaking: bool,
    jump_ticks: i32,
    jump_duration: i32,
    tail_counter: u8,
    tentacle_movement: f32,
    tentacle_speed: f32,
    rotate_speed: f32,
    interp_target: Position,
    interp_look_dir: LookDirection,
    interp_steps: i32,
    interp_head_y_rot_deg: f32,
    interp_head_y_rot_steps: i32,
}

impl LivingEntity {
    pub fn new(
        entity_type: EntityKind,
        position: Position,
        look_dir: LookDirection,
        head_y_rot_deg: f32,
        body_y_rot_deg: f32,
        player_uuid: Option<uuid::Uuid>,
    ) -> Self {
        let default_health = if entity_type == EntityKind::IronGolem {
            100.0
        } else {
            20.0
        };
        Self {
            position,
            prev_position: position,
            look_dir,
            prev_look_dir: look_dir,
            head_y_rot_deg,
            prev_head_y_rot_deg: head_y_rot_deg,
            body_y_rot_deg,
            prev_body_y_rot_deg: body_y_rot_deg,
            entity_type,
            player_uuid,
            walk_anim_pos: 0.0,
            walk_anim_speed: 0.0,
            prev_walk_anim_speed: 0.0,
            is_baby: false,
            is_crouching: false,
            pose: EntityPose::Standing,
            sleeping_pos: None,
            flags: EntityFlags::default(),
            using_item: false,
            using_offhand: false,
            main_arm_right: true,
            skin_parts_mask: None,
            riptide_spin: false,
            attributes: HashMap::new(),
            effects: HashMap::new(),
            // Spawn grounded: on_ground is packet-driven and a stationary
            // entity gets no movement packet for up to 60 ticks.
            on_ground: true,
            wool_color: None,
            is_sheared: false,
            // Vanilla salmon default is MEDIUM (id 1); non-default-only
            // metadata means the size may never be synced.
            bee_flags: 0,
            ghast_charging: false,
            vex_charging: false,
            phantom_size: 0,
            shulker_peek: 0,
            shulker_peek_amount: 0.0,
            prev_shulker_peek_amount: 0.0,
            shulker_attach_face: azalea_core::direction::Direction::Down,
            sulfur_cube_size: 1,
            wither_invulnerability: 0,
            variant: if entity_type == EntityKind::Salmon {
                1
            } else {
                0
            },
            flap: 0.0,
            prev_flap: 0.0,
            flap_speed: 0.0,
            prev_flap_speed: 0.0,
            squish: 0.0,
            prev_squish: 0.0,
            slime_size: 1,
            is_creepy: false,
            is_converting: false,
            witch_drinking: false,
            is_sitting: false,
            is_tame: false,
            is_sprinting: false,
            collar_color: 14,
            is_interested: false,
            anger_end_time: -1,
            // Vanilla constructs at max health; the golem's crack overlay
            // reads it before the metadata arrives.
            health: default_health,
            max_health: default_health,
            interested_angle: 0.0,
            prev_interested_angle: 0.0,
            shake_anim: 0.0,
            prev_shake_anim: 0.0,
            is_lying: false,
            relax_state_one: false,
            lie_down_amount: 0.0,
            prev_lie_down_amount: 0.0,
            lie_down_amount_tail: 0.0,
            prev_lie_down_amount_tail: 0.0,
            relax_state_one_amount: 0.0,
            prev_relax_state_one_amount: 0.0,
            hop_anim_start: None,
            is_eating: false,
            is_standing: false,
            is_open_mouth: false,
            eat_anim: 0.0,
            prev_eat_anim: 0.0,
            stand_anim: 0.0,
            prev_stand_anim: 0.0,
            mouth_anim: 0.0,
            prev_mouth_anim: 0.0,
            has_chest: false,
            saddled: false,
            equipment: HashMap::new(),
            horse_jump_pending_scale: 0.0,
            velocity: DVec3::ZERO,
            is_in_water: false,
            x_body_rot: 0.0,
            prev_x_body_rot: 0.0,
            z_body_rot: 0.0,
            prev_z_body_rot: 0.0,
            tentacle_angle: 0.0,
            prev_tentacle_angle: 0.0,
            bat_resting: false,
            bat_anim_start: None,
            puff_state: 0,
            dark_ticks: 0,
            golem_attack_ticks: 0,
            golem_offer_flower_ticks: 0,
            villager_kind: VillagerKind::default(),
            villager_profession: VillagerProfession::default(),
            villager_level: 0,
            unhappy_counter: 0,
            eat_anim_tick: 0,
            prev_eat_anim_tick: 0,
            hurt_time: 0,
            death_time: 0,
            age_in_ticks: 0,
            custom_name: None,
            aggressive: false,
            powered: false,
            swing_time: 0,
            flapping: 1.0,
            target_squish: 0.0,
            // Vanilla `AbstractCubeMob.wasOnGround` starts false; with the
            // grounded spawn above this reproduces vanilla's first-track
            // landing squash and skips the airborne-spawn stretch.
            prev_on_ground: false,
            is_shaking: false,
            jump_ticks: 0,
            jump_duration: 0,
            tail_counter: 0,
            tentacle_movement: 0.0,
            tentacle_speed: 1.0 / (fastrand::f32() + 1.0) * 0.2,
            rotate_speed: 0.0,
            interp_target: position,
            interp_look_dir: look_dir,
            interp_steps: 0,
            interp_head_y_rot_deg: head_y_rot_deg,
            interp_head_y_rot_steps: 0,
        }
    }

    fn interpolate_to_pos(&mut self, pos: Position) {
        self.interp_target = pos;
        self.interp_steps = INTERPOLATION_STEPS;
    }

    /// Locally controlled mounts simulate instead of following remote lerp
    /// targets.
    pub(crate) fn stop_interpolation(&mut self) {
        self.interp_target = self.position;
        self.interp_steps = 0;
        self.interp_head_y_rot_steps = 0;
    }

    pub fn tick_interpolation(&mut self) {
        self.prev_position = self.position;
        self.prev_look_dir = self.look_dir;

        if self.interp_steps > 0 {
            let alpha = 1.0 / self.interp_steps as f64;
            self.position = self.position.lerp(self.interp_target, alpha);
            let y_rot = lerp_angle(
                self.look_dir.y_rot_deg(),
                self.interp_look_dir.y_rot_deg(),
                1.0 / self.interp_steps as f32,
            );
            let x_rot = self.look_dir.x_rot_deg()
                + (self.interp_look_dir.x_rot_deg() - self.look_dir.x_rot_deg())
                    / self.interp_steps as f32;
            self.look_dir = LookDirection::new(y_rot, x_rot);
            self.interp_steps -= 1;
        }

        self.prev_head_y_rot_deg = self.head_y_rot_deg;
        if self.interp_head_y_rot_steps > 0 {
            self.head_y_rot_deg = lerp_angle(
                self.head_y_rot_deg,
                self.interp_head_y_rot_deg,
                1.0 / self.interp_head_y_rot_steps as f32,
            );
            self.interp_head_y_rot_steps -= 1;
        }

        self.prev_body_y_rot_deg = self.body_y_rot_deg;
    }

    /// Arm-swing progress 0..1 for the current frame. `swing_time` counts down,
    /// so progress rises 0→1 over the swing; idle clamps to 1 (where the
    /// attack pose contribution is zero, like vanilla's attackTime
    /// endpoints).
    pub fn swing_progress(&self, partial: f32) -> f32 {
        ((SWING_DURATION as f32 - self.swing_time as f32 + partial) / SWING_DURATION as f32)
            .clamp(0.0, 1.0)
    }

    /// Vanilla `WalkAnimationState.position(partialTick)` (babies run the
    /// cycle 3x).
    pub fn walk_pos(&self, partial: f32) -> f32 {
        let scale = if self.is_baby { 3.0 } else { 1.0 };
        (self.walk_anim_pos - self.walk_anim_speed * (1.0 - partial)) * scale
    }

    /// Vanilla `WalkAnimationState.speed(partialTick)`.
    pub fn walk_speed(&self, partial: f32) -> f32 {
        (self.prev_walk_anim_speed + (self.walk_anim_speed - self.prev_walk_anim_speed) * partial)
            .min(1.0)
    }

    /// Vanilla `Chicken.aiStep` wing flap; the update order matters.
    fn tick_flap(&mut self) {
        self.prev_flap = self.flap;
        self.prev_flap_speed = self.flap_speed;
        let delta = if self.on_ground { -0.3 } else { 1.2 };
        self.flap_speed = (self.flap_speed + delta).clamp(0.0, 1.0);
        if !self.on_ground && self.flapping < 1.0 {
            self.flapping = 1.0;
        }
        self.flapping *= 0.9;
        self.flap += self.flapping * 2.0;
    }

    /// Client-side springs for the wolf beg tilt / shake ramp, cat lie-down
    /// and relax, and the rabbit hop clock (vanilla ticks these on both
    /// sides; only the driving flags are synced). Inert for other mobs.
    fn tick_tamable_anims(&mut self) {
        let spring = |cur: f32, on: bool, up: f32, down: f32| {
            if on {
                (cur + up).min(1.0)
            } else {
                (cur - down).max(0.0)
            }
        };
        self.prev_interested_angle = self.interested_angle;
        self.interested_angle +=
            (if self.is_interested { 1.0 } else { 0.0 } - self.interested_angle) * 0.4;

        if self.is_shaking {
            self.prev_shake_anim = self.shake_anim;
            self.shake_anim += 0.05;
            if self.prev_shake_anim >= 2.0 {
                self.is_shaking = false;
                self.shake_anim = 0.0;
                self.prev_shake_anim = 0.0;
            }
        }

        self.prev_lie_down_amount = self.lie_down_amount;
        self.lie_down_amount = spring(self.lie_down_amount, self.is_lying, 0.15, 0.22);
        self.prev_lie_down_amount_tail = self.lie_down_amount_tail;
        self.lie_down_amount_tail = spring(self.lie_down_amount_tail, self.is_lying, 0.08, 0.13);
        self.prev_relax_state_one_amount = self.relax_state_one_amount;
        self.relax_state_one_amount =
            spring(self.relax_state_one_amount, self.relax_state_one, 0.1, 0.13);

        // Vanilla `Rabbit.setupAnimationStates` (baseTick) then the
        // `aiStep` jump counter. The clock starts at vanilla's post-increment
        // tickCount; `age_in_ticks` only increments after this tick body.
        if self.jump_ticks > 0 {
            if self.hop_anim_start.is_none() {
                self.hop_anim_start = Some(self.age_in_ticks + 1);
            }
        } else {
            self.hop_anim_start = None;
        }
        if self.jump_ticks != self.jump_duration {
            self.jump_ticks += 1;
        } else if self.jump_duration != 0 {
            self.jump_ticks = 0;
            self.jump_duration = 0;
        }
    }

    /// Vanilla `Wolf.getWetShade` grayscale, with wetness approximated by the
    /// shake run (rain wetness isn't sampled).
    // TODO: true isInWaterOrRain wetness for the pre-shake 0.75 darkening.
    pub fn wet_shade(&self, alpha: f32) -> f32 {
        if !self.is_shaking {
            return 1.0;
        }
        let shake = self.prev_shake_anim + (self.shake_anim - self.prev_shake_anim) * alpha;
        (0.75 + shake / 2.0 * 0.25).min(1.0)
    }

    pub fn tail_swishing(&self) -> bool {
        self.tail_counter > 0
    }

    /// Vanilla `AbstractHorse.tick`/`aiStep` springs for the grass-eat,
    /// rear-up and feeding-mouth animations, plus the client-local tail-swish
    /// counter. Gated on the equine kinds (the tail RNG isn't free).
    fn tick_equine_anims(&mut self) {
        if fastrand::u32(0..200) == 0 {
            self.tail_counter = 1;
        }
        if self.tail_counter > 0 {
            self.tail_counter += 1;
            if self.tail_counter > 8 {
                self.tail_counter = 0;
            }
        }
        self.prev_eat_anim = self.eat_anim;
        if self.is_eating {
            self.eat_anim = (self.eat_anim + (1.0 - self.eat_anim) * 0.4 + 0.05).min(1.0);
        } else {
            self.eat_anim = (self.eat_anim - self.eat_anim * 0.4 - 0.05).max(0.0);
        }
        self.prev_stand_anim = self.stand_anim;
        if self.is_standing {
            self.prev_eat_anim = 0.0;
            self.eat_anim = 0.0;
            self.stand_anim = (self.stand_anim + (1.0 - self.stand_anim) * 0.4 + 0.05).min(1.0);
        } else {
            self.stand_anim = (self.stand_anim
                + (0.8 * self.stand_anim * self.stand_anim * self.stand_anim - self.stand_anim)
                    * 0.6
                - 0.05)
                .max(0.0);
        }
        self.prev_mouth_anim = self.mouth_anim;
        if self.is_open_mouth {
            self.mouth_anim = (self.mouth_anim + (1.0 - self.mouth_anim) * 0.7 + 0.05).min(1.0);
        } else {
            self.mouth_anim = (self.mouth_anim - self.mouth_anim * 0.7 - 0.05).max(0.0);
        }
    }

    /// Vanilla `Entity.updateFluidInteraction`: true when any water column in
    /// the (slightly deflated) AABB's block range reaches above the box
    /// bottom. The AABB matches the entity's actual dimensions, including the
    /// baby-squid override and the salmon/pufferfish variant scale.
    fn probe_water(&self, chunks: &ChunkStore) -> bool {
        let (w, h): (f64, f64) = match self.entity_type {
            // `Squid.BABY_DIMENSIONS` is an explicit 0.5x0.5.
            EntityKind::Squid | EntityKind::GlowSquid if self.is_baby => (0.5, 0.5),
            EntityKind::Squid | EntityKind::GlowSquid => (0.8, 0.8),
            EntityKind::Cod => (0.5, 0.3),
            // `Salmon.getSalmonScale`: small 0.5, medium 1.0, large 1.5.
            EntityKind::Salmon => {
                let scale = match self.variant {
                    0 => 0.5,
                    2 => 1.5,
                    _ => 1.0,
                };
                (0.7 * scale, 0.4 * scale)
            }
            EntityKind::TropicalFish => (0.5, 0.4),
            // `Pufferfish.getScale`: states 0/1/2 = 0.5/0.7/1.0.
            EntityKind::Pufferfish => {
                let scale = match self.puff_state {
                    0 => 0.5,
                    1 => 0.7,
                    _ => 1.0,
                };
                (0.7 * scale, 0.7 * scale)
            }
            _ => (0.6, 0.6),
        };
        let min_x = self.position.x - w / 2.0 + 0.001;
        let min_y = self.position.y + 0.001;
        let min_z = self.position.z - w / 2.0 + 0.001;
        let max_x = self.position.x + w / 2.0 - 0.001;
        let max_y = self.position.y + h - 0.001;
        let max_z = self.position.z + w / 2.0 - 0.001;
        for bx in (min_x.floor() as i32)..=(max_x.ceil() as i32 - 1) {
            for by in (min_y.floor() as i32)..=(max_y.ceil() as i32 - 1) {
                for bz in (min_z.floor() as i32)..=(max_z.ceil() as i32 - 1) {
                    let f = fluid(chunks.get_block_state(bx, by, bz));
                    if f.kind != FluidKind::Water {
                        continue;
                    }
                    // Full height when the block above is also water.
                    let above = fluid(chunks.get_block_state(bx, by + 1, bz));
                    let top = by as f64
                        + if above.kind == FluidKind::Water {
                            1.0
                        } else {
                            f.height() as f64
                        };
                    if top >= min_y {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Vanilla `Squid.aiStep` body/tentacle sim. The tentacle stroke clock
    /// clamps at 2*pi on the client and only entity event 19 resets it; the
    /// body yaw and pitch follow the packet-driven velocity (vanilla steers
    /// remote squids client-side too, overriding the synced yaw).
    fn tick_squid(&mut self) {
        use std::f32::consts::PI;
        self.prev_x_body_rot = self.x_body_rot;
        self.prev_z_body_rot = self.z_body_rot;
        self.prev_tentacle_angle = self.tentacle_angle;
        self.tentacle_movement += self.tentacle_speed;
        if self.tentacle_movement > PI * 2.0 {
            self.tentacle_movement = PI * 2.0;
        }
        if self.is_in_water {
            if self.tentacle_movement < PI {
                let scale = self.tentacle_movement / PI;
                self.tentacle_angle = (scale * scale * PI).sin() * PI * 0.25;
                if scale > 0.75 {
                    self.rotate_speed = 1.0;
                } else {
                    self.rotate_speed *= 0.8;
                }
            } else {
                self.tentacle_angle = 0.0;
                self.rotate_speed *= 0.99;
            }
            let v = self.velocity;
            let horiz = (v.x * v.x + v.z * v.z).sqrt();
            self.body_y_rot_deg +=
                (-(v.x.atan2(v.z) as f32).to_degrees() - self.body_y_rot_deg) * 0.1;
            self.z_body_rot += PI * self.rotate_speed * 1.5;
            self.x_body_rot += (-(horiz.atan2(v.y) as f32).to_degrees() - self.x_body_rot) * 0.1;
        } else {
            self.tentacle_angle = self.tentacle_movement.sin().abs() * PI * 0.25;
            self.x_body_rot += (-90.0 - self.x_body_rot) * 0.02;
        }
    }

    /// Vanilla `AbstractCubeMob.tick` squish spring; the update order matters.
    fn tick_squish(&mut self) {
        self.prev_squish = self.squish;
        self.squish += (self.target_squish - self.squish) * 0.5;
        if self.on_ground && !self.prev_on_ground {
            self.target_squish = -0.5;
        } else if !self.on_ground && self.prev_on_ground {
            self.target_squish = 1.0;
        }
        self.prev_on_ground = self.on_ground;
        self.target_squish *= 0.6;
    }

    /// Per-kind per-tick animation state (the kind-specific tail of vanilla
    /// `aiStep`); arms accrue as mobs land.
    fn tick_kind_anims(&mut self) {
        match self.entity_type {
            EntityKind::Chicken => self.tick_flap(),
            EntityKind::Slime => self.tick_squish(),
            EntityKind::Squid | EntityKind::GlowSquid => {
                self.tick_squid();
                if self.dark_ticks > 0 {
                    self.dark_ticks -= 1;
                }
            }
            // One animation clock, restarted whenever the resting flag
            // flips (the setter restarts it); vanilla starts it at the
            // post-increment tickCount.
            EntityKind::Bat if self.bat_anim_start.is_none() => {
                self.bat_anim_start = Some(self.age_in_ticks + 1);
            }
            k if is_equine(&k) => self.tick_equine_anims(),
            EntityKind::IronGolem => {
                self.golem_attack_ticks = self.golem_attack_ticks.saturating_sub(1);
                self.golem_offer_flower_ticks = self.golem_offer_flower_ticks.saturating_sub(1);
            }
            _ => {}
        }
    }

    pub fn tick_body_rotation(&mut self) {
        let dx = self.position.x - self.prev_position.x;
        let dz = self.position.z - self.prev_position.z;
        let dist_sq = (dx * dx + dz * dz) as f32;

        if dist_sq > 0.0025 {
            let walk_dir = -(dx as f32).atan2(dz as f32).to_degrees();
            let diff_from_look = wrap_degrees(self.look_dir.y_rot_deg() - walk_dir).abs();
            let body_target = if diff_from_look > 95.0 && diff_from_look < 265.0 {
                walk_dir - 180.0
            } else {
                walk_dir
            };
            let diff = wrap_degrees(body_target - self.body_y_rot_deg);
            self.body_y_rot_deg += diff * 0.3;
        }

        let head_diff = wrap_degrees(self.head_y_rot_deg - self.body_y_rot_deg);
        if head_diff.abs() > 50.0 {
            self.body_y_rot_deg += head_diff - head_diff.signum() * 50.0;
        }
    }
}

pub struct ItemEntity {
    pub uuid: uuid::Uuid,
    pub position: Position,
    pub prev_position: Position,
    pub item_name: String,
    /// Registry id (vanilla `Item.getId`) — part of the copy-scatter seed.
    pub item_id: u32,
    /// Vanilla `ItemStack.getDamageValue()` — the other seed component.
    pub damage: i32,
    pub count: i32,
    pub stack: Option<azalea_inventory::ItemStackData>,
    pub age: u32,
    pub bob_offset: f32,
    pub invisible: bool,
    velocity: DVec3,
    on_ground: bool,
    /// Server-authoritative position, tracked from move/teleport packets.
    server_pos: Position,
}

struct PickupAnimation {
    item_name: String,
    stack: Option<azalea_inventory::ItemStackData>,
    item_id: u32,
    damage: i32,
    count: i32,
    start_pos: Position,
    target_pos: Position,
    bob_offset: f32,
    age: u32,
    life: u32,
}

pub struct PickupRenderInfo {
    pub item_name: String,
    pub stack: Option<azalea_inventory::ItemStackData>,
    pub item_id: u32,
    pub damage: i32,
    pub count: i32,
    pub position: Position,
    pub bob_offset: f32,
    pub age: u32,
}

const PICKUP_LIFE: u32 = 3;

pub struct ItemEntityStore {
    items: HashMap<i32, ItemEntity>,
    pickups: Vec<PickupAnimation>,
}

impl ItemEntityStore {
    pub fn new() -> Self {
        Self {
            items: HashMap::new(),
            pickups: Vec::new(),
        }
    }

    pub fn position(&self, id: i32) -> Option<Position> {
        self.items.get(&id).map(|entity| entity.position)
    }

    pub fn spawn_item(&mut self, id: i32, uuid: uuid::Uuid, position: Position, velocity: DVec3) {
        let bob_offset =
            ((id as u32).wrapping_mul(2654435761)) as f32 / u32::MAX as f32 * std::f32::consts::TAU;
        self.items.insert(
            id,
            ItemEntity {
                uuid,
                position,
                prev_position: position,
                item_name: String::new(),
                item_id: 0,
                damage: 0,
                count: 1,
                stack: None,
                age: 0,
                bob_offset,
                invisible: false,
                velocity,
                on_ground: false,
                server_pos: position,
            },
        );
    }

    /// Vanilla `Entity` shared flags byte: bit 0x20 marks an invisible entity.
    pub fn set_shared_flags(&mut self, id: i32, flags: u8) {
        if let Some(entity) = self.items.get_mut(&id) {
            entity.invisible = flags & 0x20 != 0;
        }
    }

    pub fn set_item_data(
        &mut self,
        id: i32,
        item_name: String,
        item_id: u32,
        damage: i32,
        count: i32,
        stack: Option<azalea_inventory::ItemStackData>,
    ) {
        if let Some(entity) = self.items.get_mut(&id) {
            entity.item_name = item_name;
            entity.item_id = item_id;
            entity.damage = damage;
            entity.count = count;
            entity.stack = stack;
        }
    }

    /// Apply a server position delta: advance the authoritative base and
    /// snap to it. Items have no interpolation handler in vanilla
    /// (`moveOrInterpolateTo` -> `setPos`); local physics predicts between
    /// packets and `prev_position` smooths the render lerp.
    pub fn move_delta(&mut self, id: i32, dx: f64, dy: f64, dz: f64, on_ground: bool) {
        if let Some(entity) = self.items.get_mut(&id) {
            entity.server_pos += DVec3::new(dx, dy, dz);
            entity.position = entity.server_pos;
            entity.on_ground = on_ground;
        }
    }

    pub fn teleport(
        &mut self,
        id: i32,
        position: Position,
        velocity: Option<DVec3>,
        on_ground: bool,
    ) {
        if let Some(entity) = self.items.get_mut(&id) {
            // Vanilla suppresses the render lerp on jumps over 64 blocks
            // (`tooBigToInterpolate`).
            if entity.position.distance_squared(*position) > 4096.0 {
                entity.prev_position = position;
            }
            entity.server_pos = position;
            entity.position = position;
            if let Some(velocity) = velocity {
                entity.velocity = velocity;
            }
            entity.on_ground = on_ground;
        }
    }

    /// Vanilla `handleSetEntityMotion`.
    pub fn set_motion(&mut self, id: i32, velocity: DVec3) {
        if let Some(entity) = self.items.get_mut(&id) {
            entity.velocity = velocity;
        }
    }

    /// Handle a take-item packet: animate the (pre-shrink) cluster flying to
    /// the collector, then shrink the stack by `amount`, removing it only
    /// when empty (vanilla `handleTakeItemEntity`). Returns the item's
    /// position for the pickup sound, or `None` if there's nothing to pick
    /// up.
    pub fn pickup(&mut self, item_id: i32, target_pos: Position, amount: i32) -> Option<Position> {
        let entity = self.items.get_mut(&item_id)?;
        if entity.item_name.is_empty() {
            return None;
        }
        let start_pos = entity.position;
        let anim = PickupAnimation {
            item_name: entity.item_name.clone(),
            stack: entity.stack.clone(),
            item_id: entity.item_id,
            damage: entity.damage,
            count: entity.count,
            start_pos,
            target_pos,
            bob_offset: entity.bob_offset,
            age: entity.age,
            life: 0,
        };
        entity.count -= amount;
        let empty = entity.count <= 0;
        self.pickups.push(anim);
        if empty {
            self.items.remove(&item_id);
        }
        Some(start_pos)
    }

    pub fn remove(&mut self, ids: &[i32]) {
        for &id in ids {
            self.items.remove(&id);
        }
    }

    pub fn advance_age(&mut self, simulation_ticks: u32) {
        for entity in self.items.values_mut() {
            entity.age = entity.age.wrapping_add(simulation_ticks);
        }
    }

    pub fn tick(&mut self, chunk_store: &ChunkStore) {
        for (&id, entity) in self.items.iter_mut() {
            entity.prev_position = entity.position;
            tick_item_physics(id, entity, chunk_store);
        }
        for pickup in &mut self.pickups {
            pickup.life += 1;
        }
        self.pickups.retain(|p| p.life < PICKUP_LIFE);
    }

    pub fn visible_items(&self, camera_pos: DVec3, max_dist: f64) -> Vec<&ItemEntity> {
        let max_dist_sq = max_dist * max_dist;
        self.items
            .values()
            .filter(|e| {
                !e.item_name.is_empty() && e.position.distance_squared(camera_pos) < max_dist_sq
            })
            .collect()
    }

    pub fn active_pickups(&self, partial_tick: f32) -> Vec<PickupRenderInfo> {
        self.pickups
            .iter()
            .map(|p| {
                let t = (p.life as f32 + partial_tick) / PICKUP_LIFE as f32;
                let t = t * t;
                let pos = p.start_pos.lerp(p.target_pos, t as f64);
                PickupRenderInfo {
                    item_name: p.item_name.clone(),
                    stack: p.stack.clone(),
                    item_id: p.item_id,
                    damage: p.damage,
                    count: p.count,
                    position: pos,
                    bob_offset: p.bob_offset,
                    age: p.age,
                }
            })
            .collect()
    }
}

/// Vanilla `ItemEntity.getDefaultGravity`.
const ITEM_GRAVITY: f64 = 0.04;
/// Vanilla `Entity.getAirDrag`.
const ITEM_AIR_DRAG: f64 = 0.98;
/// Item hitbox is 0.25 cubed (`EntityType.ITEM` dimensions).
const ITEM_HALF_WIDTH: f64 = 0.125;

/// Client-side port of `ItemEntity.tick` movement: gravity or fluid drift,
/// collide-and-slide, friction, then the half-speed landing bounce.
/// Server-only parts (merging, despawn, pickup delay) are omitted.
fn tick_item_physics(id: i32, entity: &mut ItemEntity, chunk_store: &ChunkStore) {
    let block_x = entity.position.x.floor() as i32;
    let block_y = entity.position.y.floor() as i32;
    let block_z = entity.position.z.floor() as i32;
    let chunk_pos = ChunkPos::new(block_x.div_euclid(16), block_z.div_euclid(16));
    if chunk_store.get_chunk(&chunk_pos).is_none() {
        // Don't simulate (and fall) through unloaded terrain.
        return;
    }

    let state = chunk_store.get_block_state(block_x, block_y, block_z);
    let fluid = fluid(state);
    // Vanilla `getFluidHeight(...) > 0.1`: how far the fluid surface sits
    // above the item's feet, sampled at the position block.
    let fluid_height = block_y as f64 + fluid.height() as f64 - entity.position.y;
    match fluid.kind {
        FluidKind::Water if fluid_height > 0.1 => apply_item_fluid_movement(entity, 0.99),
        FluidKind::Lava if fluid_height > 0.1 => apply_item_fluid_movement(entity, 0.95),
        _ => entity.velocity.y -= ITEM_GRAVITY,
    }

    // Vanilla rest throttle: a settled item only re-runs collision every
    // 4th tick. `age` increments after this runs; vanilla's `tickCount`
    // increments before, hence the +1.
    let horizontal_sq =
        entity.velocity.x * entity.velocity.x + entity.velocity.z * entity.velocity.z;
    if entity.on_ground && horizontal_sq <= 1e-5 && (entity.age as i64 + 1 + id as i64) % 4 != 0 {
        return;
    }

    let aabb = Aabb::from_center(entity.position.into(), ITEM_HALF_WIDTH, ITEM_HALF_WIDTH);
    let (delta, on_ground) = resolve_collision(chunk_store, aabb, entity.velocity.into(), 0.0);
    entity.position += delta;
    entity.on_ground = on_ground;

    // TODO: per-block slipperiness (ice/slime); vanilla multiplies by the
    // friction of the block below, default 0.6.
    let ground_friction = if on_ground {
        ITEM_AIR_DRAG * 0.6
    } else {
        ITEM_AIR_DRAG
    };
    entity.velocity.x *= ground_friction;
    entity.velocity.y *= ITEM_AIR_DRAG;
    entity.velocity.z *= ground_friction;
    if on_ground && entity.velocity.y < 0.0 {
        entity.velocity.y *= -0.5;
    }
}

/// Vanilla `ItemEntity.setFluidMovement`: horizontal drag plus a slow
/// upward drift toward the surface.
fn apply_item_fluid_movement(entity: &mut ItemEntity, multiplier: f64) {
    entity.velocity.x *= multiplier;
    entity.velocity.z *= multiplier;
    if entity.velocity.y < 0.06 {
        entity.velocity.y += 5.0e-4;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RideAuthority {
    Server,
    Client,
}

/// Render-only prediction; never a relative-move packet baseline.
#[derive(Clone, Debug)]
pub struct ProjectileDisplay {
    pub prev: Position,
    pub current: Position,
    pub velocity: DVec3,
    pub stopped: bool,
    pub on_ground: bool,
    pub in_ground: bool,
    pub no_gravity: bool,
    revision: u64,
    drag: f64,
    medium_tick: u64,
    frames: Option<Flight>,
    next: Option<Flight>,
}

impl ProjectileDisplay {
    fn invalidate(&mut self) {
        self.revision = PROJECTILE_REVISION.fetch_add(1, Ordering::Relaxed);
        self.frames = None;
        self.next = None;
        self.medium_tick = 0;
    }

    pub fn position(&self, partial_tick: f32) -> Position {
        self.prev
            .lerp(self.current, f64::from(partial_tick.clamp(0.0, 1.0)))
    }
}

/// Sample available medium at flight start / prefetch. Crossing water inside
/// a 64-tick flight retains the earlier drag (display-only approximation).
/// A visible wall ghost until the server impact packet is deliberate; if
/// observed often, upgrade to a collision-worker snapshot, not a block scan
/// here.
fn projectile_drag(chunks: &ChunkStore, pos: Position, arrow: bool) -> f64 {
    let water = pos.is_finite()
        && pos.abs().max_element() < (i32::MAX - 32) as f64
        && fluid(chunks.get_block_state(
            pos.x.floor() as i32,
            pos.y.floor() as i32,
            pos.z.floor() as i32,
        ))
        .kind
            == FluidKind::Water;
    if water {
        if arrow {
            0.6_f32 as f64
        } else {
            0.8_f32 as f64
        }
    } else {
        0.99_f32 as f64
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisplayState {
    pub interpolation_delay: i32,
    pub transformation_duration: i32,
    pub position_rotation_duration: i32,
    pub translation: [f32; 3],
    pub scale: [f32; 3],
    pub left_rotation: [f32; 4],
    pub right_rotation: [f32; 4],
    pub billboard: u8,
    pub brightness: i32,
    pub view_range: f32,
    pub shadow_radius: f32,
    pub shadow_strength: f32,
    pub width: f32,
    pub height: f32,
    pub glow_override: i32,
    pub block_state: Option<u32>,
}

impl Default for DisplayState {
    fn default() -> Self {
        Self {
            interpolation_delay: 0,
            transformation_duration: 0,
            position_rotation_duration: 0,
            translation: [0.0; 3],
            scale: [1.0; 3],
            left_rotation: [0.0, 0.0, 0.0, 1.0],
            right_rotation: [0.0, 0.0, 0.0, 1.0],
            billboard: 0,
            brightness: -1,
            view_range: 1.0,
            shadow_radius: 0.0,
            shadow_strength: 1.0,
            width: 0.0,
            height: 0.0,
            glow_override: -1,
            block_state: None,
        }
    }
}

fn decode_falling_block_state(
    kind: Option<EntityKind>,
    raw: i32,
    protocol: i32,
) -> Option<azalea_block::BlockState> {
    (kind == Some(EntityKind::FallingBlock) && protocol == pomme_protocol::version::NATIVE.protocol)
        .then(|| {
            u32::try_from(raw)
                .ok()
                .and_then(crate::world::block::try_state)
        })
        .flatten()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FallingBlockRenderState {
    pub state: Option<azalea_block::BlockState>,
    pub start_pos: BlockPos,
}

pub(crate) fn falling_block_sample_pos(position: Position) -> BlockPos {
    BlockPos::new(
        position.x.floor() as i32,
        (position.y + 0.98).floor() as i32,
        position.z.floor() as i32,
    )
}

pub(crate) fn falling_block_model_matrix(position: Position, camera_anchor: DVec3) -> glam::Mat4 {
    let relative = DVec3::new(position.x, position.y, position.z) - camera_anchor;
    glam::Mat4::from_translation(relative.as_vec3() + glam::Vec3::new(0.0, 0.5, 0.0))
}

#[derive(Clone, Debug)]
pub struct VehicleState {
    /// Missing for SetPassengers-only placeholders.
    pub kind: Option<EntityKind>,
    /// Raw AddEntity data, retained without assuming a registry mapping. For a
    /// FallingBlock this is a protocol block-state id, not a metadata index.
    pub spawn_data: Option<i32>,
    /// Native-only interpretation of FallingBlock's AddEntity state and
    /// metadata 8.
    pub falling_block: FallingBlockRenderState,
    pub position: Position,
    pub prev_position: Position,
    pub velocity: DVec3,
    pub prev_look_dir: Option<LookDirection>,
    pub projectile: Option<ProjectileDisplay>,
    /// Full synchronized stack for ThrownItem entities (26.2 metadata 8).
    pub projectile_item: azalea_inventory::ItemStack,
    /// WitherSkull dangerous/invulnerable metadata (index 8); Trident foil
    /// (12).
    pub projectile_dangerous: bool,
    pub projectile_foil: bool,
    /// Arrow metadata index 11: tipped-arrow color, default -1.
    pub arrow_effect_color: i32,
    /// Primed TNT synchronized fuse and block-state metadata.
    pub tnt_fuse: i32,
    pub tnt_prev_fuse: i32,
    pub tnt_block_state: Option<u32>,
    /// None until a real spawn transform arrives; SetPassengers may create
    /// placeholders.
    pub look_dir: Option<LookDirection>,
    /// Shared entity flags (metadata 0); bit 5 is invisible.
    pub shared_flags: u8,
    /// AbstractBoat synchronized state (26.2 metadata 8..13).
    pub boat_hurt_time: i32,
    pub boat_hurt_direction: i32,
    pub boat_damage: f32,
    pub boat_left_paddle: bool,
    pub boat_right_paddle: bool,
    pub boat_bubble_time: i32,
    pub boat_prev_rowing_time: [f32; 2],
    pub boat_rowing_time: [f32; 2],
    pub boat_prev_bubble_multiplier: f32,
    pub boat_bubble_multiplier: f32,
    pub boat_prev_bubble_angle: f32,
    pub boat_bubble_angle: f32,
    pub boat_tick_count: u32,
    pub boat_prev_hurt_time: i32,
    pub boat_prev_damage: f32,
    /// AbstractMinecart metadata 11/12: optional registry state id + offset.
    pub minecart_display_state: Option<u32>,
    pub minecart_display_offset: i32,
    /// MinecartFurnace DATA_ID_FUEL, used by its native default display state.
    pub minecart_furnace_has_fuel: bool,
    /// EndCrystal metadata 8/9; kept separate from other entity metadata.
    pub crystal_beam_target: Option<BlockPos>,
    pub crystal_show_bottom: bool,
    pub crystal_age: u32,
    /// ExperienceOrb metadata 8 and its client-side visual age.
    pub experience_orb_value: i32,
    pub experience_orb_age: u32,
    /// Native projectile-renderer tick clock, interpolated with the frame
    /// partial.
    pub projectile_age: u32,
    pub projectile_prev_age: u32,
    /// Synced Mannequin profile (metadata index 17); its UUID is the
    /// texture-cache identity.
    pub mannequin_profile: Option<azalea_inventory::components::Profile>,
    /// Avatar customization mask defaults to all seven skin layers enabled.
    pub mannequin_skin_parts_mask: u8,
    pub mannequin_main_arm_right: bool,
    pub mannequin_pose: EntityPose,
    /// ArmorStand metadata 15 flags and native six-part pose (degrees).
    pub armor_stand_flags: u8,
    pub armor_stand_pose: [[f32; 3]; 6],
    /// ArmorStand's hands and armor; equipment rendering is separate.
    pub armor_stand_equipment:
        HashMap<azalea_inventory::components::EquipmentSlot, azalea_inventory::ItemStack>,
    /// ItemFrame spawn data / metadata index 8; independent from entity yaw.
    pub item_frame_direction: Option<azalea_core::direction::Direction>,
    /// Full metadata index 9 stack, retained for component-backed item render.
    pub item_frame_item: azalea_inventory::ItemStack,
    /// Metadata index 10, in 45-degree increments.
    pub item_frame_rotation: i32,
    pub passengers: Vec<i32>,
    /// Common Display metadata; ItemDisplay/TextDisplay payloads remain
    /// separate.
    pub display: DisplayState,
    /// ItemDisplay metadata 23/24: full stack and native display-context byte.
    pub item_display_stack: azalea_inventory::ItemStack,
    pub item_display_context: u8,
    /// TextDisplay metadata: component 23, line width 24, background 25,
    /// opacity 26, and style flags 27.
    pub text_display_text: Option<Vec<crate::ui::text::TextSpan>>,
    pub text_display_line_width: i32,
    pub text_display_background: u32,
    pub text_display_opacity: u8,
    pub text_display_flags: u8,
    /// TextDisplay transform metadata indices 11..15, retained verbatim.
    pub text_display_translation: [f32; 3],
    pub text_display_scale: [f32; 3],
    pub text_display_left_rotation: [f32; 4],
    pub text_display_right_rotation: [f32; 4],
    pub text_display_billboard: u8,
    /// Display metadata index 17; vanilla default 1.0 (64 blocks at scale 1).
    pub text_display_view_range: f32,
}

pub struct EntityStore {
    pub living: HashMap<i32, LivingEntity>,
    /// Passenger lists and root transforms for all entity kinds, including
    /// nonliving vehicles.
    pub vehicles: HashMap<i32, VehicleState>,
    pub vehicle_of: HashMap<i32, i32>,
    tick: u64,
    epoch: u64,
    worker: Option<Worker>,
}

impl EntityStore {
    pub fn new() -> Self {
        Self {
            living: HashMap::new(),
            vehicles: HashMap::new(),
            vehicle_of: HashMap::new(),
            tick: 0,
            epoch: WORLD_EPOCH.fetch_add(1, Ordering::Relaxed),
            worker: None,
        }
    }

    /// A new dimension must not reuse old-world flights or projectile vehicles.
    /// Keep the worker: its one pending job is drained and rejected by epoch.
    pub(crate) fn replace_projectile_world(&mut self) -> Vec<i32> {
        self.epoch = WORLD_EPOCH.fetch_add(1, Ordering::Relaxed);
        let ids: Vec<_> = self
            .vehicles
            .iter()
            .filter_map(|(&id, v)| v.projectile.is_some().then_some(id))
            .collect();
        for &id in &ids {
            self.remove_entity(id);
        }
        ids
    }

    /// EntityGetter.getEntityCollisions uses source.canCollideWith(target),
    /// which requires target.canBeCollidedWith(source). Ordinary living mobs
    /// (including players) return false: they push, not block movement.
    /// Shulker overrides that predicate for living shulkers.
    pub fn collision_aabbs(&self, local_id: i32, region: &Aabb) -> Vec<Aabb> {
        self.living
            .iter()
            .filter_map(|(&id, entity)| {
                if id == local_id
                    || entity.health <= 0.0
                    || entity.entity_type != EntityKind::Shulker
                {
                    return None;
                }
                let dimensions =
                    azalea_entity::dimensions::EntityDimensions::from(entity.entity_type);
                let (width, height) = (f64::from(dimensions.width), f64::from(dimensions.height));
                let box_ = Aabb::from_center(entity.position.into(), width * 0.5, height * 0.5);
                (box_.intersects(region)).then_some(box_)
            })
            .collect()
    }

    /// Replace a vehicle's ordered passenger list from SetPassengers; order is
    /// semantically significant.
    pub fn set_passengers(&mut self, vehicle_id: i32, passengers: &[i32]) {
        for &passenger in passengers {
            if let Some(old_vehicle) = self.vehicle_of.get(&passenger).copied()
                && old_vehicle != vehicle_id
                && let Some(old) = self.vehicles.get_mut(&old_vehicle)
            {
                old.passengers.retain(|&id| id != passenger);
            }
        }
        for passenger in self.vehicle_of.keys().copied().collect::<Vec<_>>() {
            if self.vehicle_of.get(&passenger) == Some(&vehicle_id) {
                self.vehicle_of.remove(&passenger);
            }
        }
        let vehicle = self.vehicles.entry(vehicle_id).or_insert(VehicleState {
            position: self
                .living
                .get(&vehicle_id)
                .map_or(Position::default(), |e| e.position),
            prev_position: self
                .living
                .get(&vehicle_id)
                .map_or(Position::default(), |e| e.position),
            kind: None,
            spawn_data: None,
            falling_block: FallingBlockRenderState::default(),
            velocity: DVec3::ZERO,
            prev_look_dir: None,
            projectile: None,
            projectile_item: azalea_inventory::ItemStack::Empty,
            projectile_dangerous: false,
            projectile_foil: false,
            arrow_effect_color: -1,
            tnt_fuse: 80,
            tnt_prev_fuse: 81,
            tnt_block_state: None,
            look_dir: None,
            shared_flags: 0,
            boat_hurt_time: 0,
            boat_hurt_direction: 1,
            boat_damage: 0.0,
            boat_left_paddle: false,
            boat_right_paddle: false,
            boat_bubble_time: 0,
            boat_prev_rowing_time: [0.0; 2],
            boat_rowing_time: [0.0; 2],
            boat_prev_bubble_multiplier: 0.0,
            boat_bubble_multiplier: 0.0,
            boat_prev_bubble_angle: 0.0,
            boat_bubble_angle: 0.0,
            boat_tick_count: 0,
            boat_prev_hurt_time: 0,
            boat_prev_damage: 0.0,
            minecart_display_state: None,
            minecart_display_offset: 6,
            minecart_furnace_has_fuel: false,
            crystal_beam_target: None,
            crystal_show_bottom: true,
            crystal_age: 0,
            experience_orb_value: 0,
            experience_orb_age: 0,
            projectile_age: 0,
            projectile_prev_age: 0,
            mannequin_profile: None,
            mannequin_skin_parts_mask: 0x7f,
            mannequin_main_arm_right: true,
            mannequin_pose: EntityPose::Standing,
            armor_stand_flags: 0,
            armor_stand_pose: [
                [0.0; 3],
                [0.0; 3],
                [-10.0, 0.0, -10.0],
                [-15.0, 0.0, 10.0],
                [-1.0, 0.0, -1.0],
                [1.0, 0.0, 1.0],
            ],
            armor_stand_equipment: HashMap::new(),
            item_frame_direction: None,
            item_frame_item: azalea_inventory::ItemStack::Empty,
            item_frame_rotation: 0,
            passengers: Vec::new(),
            display: DisplayState::default(),
            item_display_stack: azalea_inventory::ItemStack::Empty,
            item_display_context: 0,
            text_display_text: None,
            text_display_line_width: 200,
            text_display_background: 0x4000_0000,
            text_display_opacity: 0xff,
            text_display_flags: 0,
            text_display_translation: [0.0; 3],
            text_display_scale: [1.0; 3],
            text_display_left_rotation: [0.0, 0.0, 0.0, 1.0],
            text_display_right_rotation: [0.0, 0.0, 0.0, 1.0],
            text_display_billboard: 0,
            text_display_view_range: 1.0,
        });
        vehicle.passengers.clear();
        vehicle.passengers.extend_from_slice(passengers);
        for &passenger in passengers {
            self.vehicle_of.insert(passenger, vehicle_id);
        }
    }

    pub fn set_vehicle_transform(&mut self, id: i32, position: Position, velocity: DVec3) {
        let state = self.vehicles.entry(id).or_insert(VehicleState {
            position,
            prev_position: position,
            kind: None,
            spawn_data: None,
            falling_block: FallingBlockRenderState::default(),
            velocity,
            prev_look_dir: None,
            projectile: None,
            projectile_item: azalea_inventory::ItemStack::Empty,
            projectile_dangerous: false,
            projectile_foil: false,
            arrow_effect_color: -1,
            tnt_fuse: 80,
            tnt_prev_fuse: 81,
            tnt_block_state: None,
            look_dir: None,
            shared_flags: 0,
            boat_hurt_time: 0,
            boat_hurt_direction: 1,
            boat_damage: 0.0,
            boat_left_paddle: false,
            boat_right_paddle: false,
            boat_bubble_time: 0,
            boat_prev_rowing_time: [0.0; 2],
            boat_rowing_time: [0.0; 2],
            boat_prev_bubble_multiplier: 0.0,
            boat_bubble_multiplier: 0.0,
            boat_prev_bubble_angle: 0.0,
            boat_bubble_angle: 0.0,
            boat_tick_count: 0,
            boat_prev_hurt_time: 0,
            boat_prev_damage: 0.0,
            minecart_display_state: None,
            minecart_display_offset: 6,
            minecart_furnace_has_fuel: false,
            crystal_beam_target: None,
            crystal_show_bottom: true,
            crystal_age: 0,
            experience_orb_value: 0,
            experience_orb_age: 0,
            projectile_age: 0,
            projectile_prev_age: 0,
            mannequin_profile: None,
            mannequin_skin_parts_mask: 0x7f,
            mannequin_main_arm_right: true,
            mannequin_pose: EntityPose::Standing,
            armor_stand_flags: 0,
            armor_stand_pose: [
                [0.0; 3],
                [0.0; 3],
                [-10.0, 0.0, -10.0],
                [-15.0, 0.0, 10.0],
                [-1.0, 0.0, -1.0],
                [1.0, 0.0, 1.0],
            ],
            armor_stand_equipment: HashMap::new(),
            item_frame_direction: None,
            item_frame_item: azalea_inventory::ItemStack::Empty,
            item_frame_rotation: 0,
            passengers: Vec::new(),
            display: DisplayState::default(),
            item_display_stack: azalea_inventory::ItemStack::Empty,
            item_display_context: 0,
            text_display_text: None,
            text_display_line_width: 200,
            text_display_background: 0x4000_0000,
            text_display_opacity: 0xff,
            text_display_flags: 0,
            text_display_translation: [0.0; 3],
            text_display_scale: [1.0; 3],
            text_display_left_rotation: [0.0, 0.0, 0.0, 1.0],
            text_display_right_rotation: [0.0, 0.0, 0.0, 1.0],
            text_display_billboard: 0,
            text_display_view_range: 1.0,
        });
        state.prev_position = state.position;
        state.position = position;
        state.velocity = velocity;
        if let Some(display) = &mut state.projectile {
            display.prev = position;
            display.current = position;
            display.velocity = velocity;
            display.invalidate();
            display.stopped = display.in_ground || display.on_ground;
        }
    }

    /// Motion packets update flight without rewinding the visual to the packet
    /// baseline.
    pub fn set_vehicle_motion(&mut self, id: i32, velocity: DVec3) {
        if let Some(vehicle) = self.vehicles.get_mut(&id) {
            vehicle.velocity = velocity;
            if let Some(display) = &mut vehicle.projectile {
                display.velocity = velocity;
                display.invalidate();
                if velocity.length_squared() > 1.0e-8 {
                    display.on_ground = false;
                    // AbstractArrow releases inGround on a new nonzero motion.
                    display.in_ground = false;
                }
                display.stopped = display.in_ground || display.on_ground;
            }
        }
    }

    pub fn set_projectile_item(&mut self, id: i32, stack: azalea_inventory::ItemStackData) {
        if let Some(vehicle) = self.vehicles.get_mut(&id) {
            vehicle.projectile_item = azalea_inventory::ItemStack::Present(stack);
        }
    }

    pub fn set_vehicle_spawn_data(&mut self, id: i32, spawn_data: i32) {
        if let Some(vehicle) = self.vehicles.get_mut(&id) {
            vehicle.spawn_data = Some(spawn_data);
            vehicle.falling_block.state = decode_falling_block_state(
                vehicle.kind,
                spawn_data,
                crate::version::session_protocol(),
            );
        }
    }

    pub fn apply_vehicle_metadata(&mut self, id: i32, index: u8, value: MetaValue) {
        let Some(vehicle) = self.vehicles.get_mut(&id) else {
            return;
        };
        match (vehicle.kind, index, value) {
            (Some(EntityKind::WitherSkull), 8, MetaValue::Bool(v)) => {
                vehicle.projectile_dangerous = v
            }
            (Some(EntityKind::Trident), 12, MetaValue::Bool(v)) => vehicle.projectile_foil = v,
            (Some(EntityKind::Arrow), 11, MetaValue::Int(v)) => vehicle.arrow_effect_color = v,
            (Some(EntityKind::Tnt), 8, MetaValue::Int(v)) => {
                vehicle.tnt_prev_fuse = v.saturating_add(1);
                vehicle.tnt_fuse = v;
            }
            (Some(EntityKind::Tnt), 9, MetaValue::BlockState(v)) => {
                vehicle.tnt_block_state = Some(v);
            }
            (Some(EntityKind::FallingBlock), 8, MetaValue::BlockPos(pos)) => {
                vehicle.falling_block.start_pos = pos;
            }
            (Some(EntityKind::EndCrystal), 8, MetaValue::OptionalBlockPos(pos)) => {
                vehicle.crystal_beam_target = pos;
            }
            (Some(EntityKind::EndCrystal), 9, MetaValue::Bool(v)) => {
                vehicle.crystal_show_bottom = v;
            }
            (Some(EntityKind::ExperienceOrb), 8, MetaValue::Int(v)) => {
                vehicle.experience_orb_value = v;
            }
            (Some(EntityKind::ArmorStand), 15, MetaValue::Byte(flags)) => {
                vehicle.armor_stand_flags = flags;
            }
            _ => {}
        }
        if index == 0
            && let MetaValue::Byte(flags) = value
        {
            vehicle.shared_flags = flags;
        }
        if matches!(
            vehicle.kind,
            Some(
                EntityKind::Minecart
                    | EntityKind::ChestMinecart
                    | EntityKind::FurnaceMinecart
                    | EntityKind::TntMinecart
                    | EntityKind::HopperMinecart
                    | EntityKind::CommandBlockMinecart
                    | EntityKind::SpawnerMinecart
            )
        ) {
            match (index, value) {
                (8, MetaValue::Int(value)) => vehicle.boat_hurt_time = value,
                (9, MetaValue::Int(value)) => vehicle.boat_hurt_direction = value,
                (10, MetaValue::Float(value)) => vehicle.boat_damage = value,
                (11, MetaValue::OptionalBlockState(state)) => {
                    vehicle.minecart_display_state = state
                }
                (12, MetaValue::Int(offset)) => vehicle.minecart_display_offset = offset,
                (13, MetaValue::Bool(has_fuel))
                    if vehicle.kind == Some(EntityKind::FurnaceMinecart) =>
                {
                    vehicle.minecart_furnace_has_fuel = has_fuel
                }
                _ => {}
            }
            return;
        }
        if !matches!(
            vehicle.kind,
            Some(
                EntityKind::AcaciaBoat
                    | EntityKind::AcaciaChestBoat
                    | EntityKind::BambooRaft
                    | EntityKind::BambooChestRaft
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
        ) {
            return;
        }
        match (index, value) {
            (8, MetaValue::Int(value)) => vehicle.boat_hurt_time = value,
            (9, MetaValue::Int(value)) => vehicle.boat_hurt_direction = value,
            (10, MetaValue::Float(value)) => vehicle.boat_damage = value,
            (11, MetaValue::Bool(value)) => vehicle.boat_left_paddle = value,
            (12, MetaValue::Bool(value)) => vehicle.boat_right_paddle = value,
            (13, MetaValue::Int(value)) => vehicle.boat_bubble_time = value,
            _ => {}
        }
    }

    pub fn set_item_display_metadata(
        &mut self,
        id: i32,
        index: u8,
        value: crate::net::ItemDisplayMetaValue,
    ) {
        let Some(vehicle) = self.vehicles.get_mut(&id) else {
            return;
        };
        if vehicle.kind != Some(EntityKind::ItemDisplay) {
            return;
        }
        match (index, value) {
            (23, crate::net::ItemDisplayMetaValue::Stack(stack)) => {
                vehicle.item_display_stack = stack
            }
            (24, crate::net::ItemDisplayMetaValue::Context(context)) => {
                vehicle.item_display_context = context
            }
            _ => {}
        }
    }

    pub fn set_display_metadata(
        &mut self,
        id: i32,
        index: u8,
        value: crate::net::DisplayMetaValue,
    ) {
        use crate::net::DisplayMetaValue as V;
        let Some(vehicle) = self.vehicles.get_mut(&id) else {
            return;
        };
        if !matches!(
            vehicle.kind,
            Some(EntityKind::BlockDisplay | EntityKind::ItemDisplay | EntityKind::TextDisplay)
        ) {
            return;
        }
        let d = &mut vehicle.display;
        match (index, value) {
            (8, V::Int(v)) => d.interpolation_delay = v,
            (9, V::Int(v)) => d.transformation_duration = v,
            (10, V::Int(v)) => d.position_rotation_duration = v,
            (11, V::Vector(v)) if v.iter().all(|x| x.is_finite()) => d.translation = v,
            (12, V::Vector(v)) if v.iter().all(|x| x.is_finite()) => d.scale = v,
            (13, V::Quaternion(v)) if v.iter().all(|x| x.is_finite()) => d.left_rotation = v,
            (14, V::Quaternion(v)) if v.iter().all(|x| x.is_finite()) => d.right_rotation = v,
            (15, V::Byte(v)) => d.billboard = v,
            (16, V::Int(v)) => d.brightness = v,
            (17, V::Float(v)) if v.is_finite() => d.view_range = v,
            (18, V::Float(v)) if v.is_finite() => d.shadow_radius = v,
            (19, V::Float(v)) if v.is_finite() => d.shadow_strength = v,
            (20, V::Float(v)) if v.is_finite() => d.width = v,
            (21, V::Float(v)) if v.is_finite() => d.height = v,
            (22, V::Int(v)) => d.glow_override = v,
            (23, V::BlockState(v)) if vehicle.kind == Some(EntityKind::BlockDisplay) => {
                d.block_state = Some(v)
            }
            _ => {}
        }
    }

    pub fn set_vehicle_kind(&mut self, id: i32, kind: EntityKind) {
        if let Some(vehicle) = self.vehicles.get_mut(&id) {
            vehicle.kind = Some(kind);
            vehicle.falling_block = FallingBlockRenderState::default();
            if kind == EntityKind::FallingBlock
                && crate::version::session_protocol() == pomme_protocol::version::NATIVE.protocol
            {
                vehicle.falling_block.state = vehicle.spawn_data.and_then(|raw| {
                    decode_falling_block_state(Some(kind), raw, crate::version::session_protocol())
                });
            }
            vehicle.projectile_age = 0;
            vehicle.projectile_prev_age = 0;
            if kind == EntityKind::ChestMinecart {
                vehicle.minecart_display_offset = 8;
            } else if kind == EntityKind::HopperMinecart {
                vehicle.minecart_display_offset = 1;
            }
            vehicle.projectile = matches!(
                kind,
                EntityKind::Arrow | EntityKind::SpectralArrow | EntityKind::Snowball
            )
            .then_some(ProjectileDisplay {
                prev: vehicle.position,
                current: vehicle.position,
                velocity: vehicle.velocity,
                stopped: false,
                revision: PROJECTILE_REVISION.fetch_add(1, Ordering::Relaxed),
                drag: 0.99_f32 as f64,
                medium_tick: 0,
                frames: None,
                next: None,
                on_ground: false,
                in_ground: false,
                no_gravity: false,
            });
        }
    }

    /// Fixed 20 Hz world tick. Absolute frame indexes prevent lost ticks even
    /// when a worker result arrives late; a miss runs the same cheap pure step.
    pub fn tick_projectile_displays(&mut self, chunks: &ChunkStore) {
        self.tick = self.tick.wrapping_add(1);
        let tick = self.tick;
        for vehicle in self.vehicles.values_mut() {
            if vehicle.kind == Some(EntityKind::EndCrystal) {
                vehicle.crystal_age = vehicle.crystal_age.wrapping_add(1);
            }
            if vehicle.kind == Some(EntityKind::ExperienceOrb) {
                vehicle.experience_orb_age = vehicle.experience_orb_age.wrapping_add(1);
            }
            if vehicle.kind == Some(EntityKind::ShulkerBullet) {
                vehicle.projectile_prev_age = vehicle.projectile_age;
                vehicle.projectile_age = vehicle.projectile_age.wrapping_add(1);
            }
            if matches!(
                vehicle.kind,
                Some(
                    EntityKind::AcaciaBoat
                        | EntityKind::AcaciaChestBoat
                        | EntityKind::BambooRaft
                        | EntityKind::BambooChestRaft
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
            ) {
                vehicle.boat_tick_count = vehicle.boat_tick_count.wrapping_add(1);
                vehicle.boat_prev_rowing_time = vehicle.boat_rowing_time;
                for (i, rowing) in vehicle.boat_rowing_time.iter_mut().enumerate() {
                    *rowing = if [vehicle.boat_left_paddle, vehicle.boat_right_paddle][i] {
                        *rowing + 0.3926991
                    } else {
                        0.0
                    };
                }
                vehicle.boat_prev_bubble_multiplier = vehicle.boat_bubble_multiplier;
                vehicle.boat_bubble_multiplier = (vehicle.boat_bubble_multiplier
                    + if vehicle.boat_bubble_time > 0 {
                        0.05
                    } else {
                        -0.1
                    })
                .clamp(0.0, 1.0);
                vehicle.boat_prev_bubble_angle = vehicle.boat_bubble_angle;
                vehicle.boat_bubble_angle = 10.0
                    * (0.5 * vehicle.boat_tick_count as f32).sin()
                    * vehicle.boat_bubble_multiplier;
                vehicle.boat_prev_hurt_time = vehicle.boat_hurt_time;
                vehicle.boat_prev_damage = vehicle.boat_damage;
                vehicle.boat_hurt_time = vehicle.boat_hurt_time.saturating_sub(1);
                if vehicle.boat_damage > 0.0 {
                    vehicle.boat_damage -= 1.0;
                }
            }
            if vehicle.kind == Some(EntityKind::Tnt) {
                vehicle.tnt_prev_fuse = vehicle.tnt_fuse;
                vehicle.tnt_fuse = vehicle.tnt_fuse.saturating_sub(1);
            }
        }
        if let Some(worker) = &mut self.worker {
            match worker.poll() {
                Ok(Some(result)) if result.epoch == self.epoch => {
                    for flight in result.flights {
                        if let Some(display) = self
                            .vehicles
                            .get_mut(&flight.id)
                            .and_then(|v| v.projectile.as_mut())
                            && display.revision == flight.revision
                            && !display.stopped
                            && tick <= flight.start + HORIZON as u64
                            // A late result is valid only when its predicted pose
                            // still joins the locally simulated pose exactly.
                            && (flight.start >= tick
                                || (flight.frames[(tick - flight.start - 1) as usize].position
                                    == display.current
                                    && flight.frames[(tick - flight.start - 1) as usize].velocity
                                        == display.velocity))
                        {
                            if flight.start < tick {
                                display.drag = flight.drag;
                                display.medium_tick = tick;
                                display.frames = Some(flight);
                            } else {
                                display.next = Some(flight);
                            }
                        }
                    }
                }
                Err(_) => self.worker = None,
                _ => {}
            }
        }
        let active = self
            .vehicles
            .values()
            .filter(|v| v.projectile.as_ref().is_some_and(|p| !p.stopped))
            .count();
        if active >= THRESHOLD && self.worker.is_none() {
            self.worker = Worker::new();
        }
        let mut inputs = Vec::new();
        for (&id, vehicle) in &mut self.vehicles {
            let Some(display) = &mut vehicle.projectile else {
                continue;
            };
            display.prev = display.current;
            if display.stopped {
                continue;
            }
            let arrow = matches!(
                vehicle.kind,
                Some(EntityKind::Arrow | EntityKind::SpectralArrow)
            );
            if let Some(next) = display.next.take() {
                if next.start < tick
                    && next.start + HORIZON as u64 >= tick
                    && next.frames[0].position == display.current
                    && next.frames[0].velocity == display.velocity
                {
                    display.drag = next.drag;
                    display.medium_tick = tick;
                    display.frames = Some(next);
                } else if next.start >= tick {
                    display.next = Some(next);
                }
                // Otherwise a packet, changed medium, or late worker result
                // invalidated the join; continue from the current pose.
            }
            // Re-sample only at the start of a horizon, not per block crossed.
            if display.medium_tick == 0 || tick >= display.medium_tick + HORIZON as u64 {
                display.drag = projectile_drag(chunks, display.current, arrow);
                display.medium_tick = tick;
            }
            let cached = display
                .frames
                .as_ref()
                .is_some_and(|f| f.start < tick && tick <= f.start + HORIZON as u64);
            if cached {
                let f = display.frames.as_ref().unwrap();
                let frame = f.frames[(tick - f.start) as usize];
                display.current = frame.position;
                display.velocity = frame.velocity;
            } else {
                let frame = projectile::step(
                    Frame {
                        position: display.current,
                        velocity: display.velocity,
                    },
                    arrow,
                    display.no_gravity,
                    display.drag,
                );
                if frame.position.is_finite() && frame.velocity.is_finite() {
                    display.current = frame.position;
                    display.velocity = frame.velocity;
                } else {
                    display.prev = display.current;
                } // hold, never permanently ground
            }
            // Refill before expiration. While pending, continue from the last
            // pose, never overwrite it with a stale worker origin.
            if active >= THRESHOLD
                && display.current.is_finite()
                && display.velocity.is_finite()
                && inputs.len() < MAX_BATCH
                && display.next.is_none()
                && display
                    .frames
                    .as_ref()
                    .is_none_or(|f| tick + 16 >= f.start + HORIZON as u64)
            {
                let (start, source) = match &display.frames {
                    Some(f) if tick < f.start + HORIZON as u64 => {
                        (f.start + HORIZON as u64, f.frames[HORIZON])
                    }
                    _ => (
                        tick,
                        Frame {
                            position: display.current,
                            velocity: display.velocity,
                        },
                    ),
                };
                inputs.push(Input {
                    id,
                    revision: display.revision,
                    start,
                    source,
                    arrow,
                    no_gravity: display.no_gravity,
                    // Use the latest available medium for the future horizon;
                    // it may differ from the medium at its exact start tick.
                    drag: if start > tick {
                        projectile_drag(chunks, display.current, arrow)
                    } else {
                        display.drag
                    },
                });
            }
        }
        if let Some(worker) = &mut self.worker
            && !worker.in_flight
            && !inputs.is_empty()
            && !worker.submit(Job {
                epoch: self.epoch,
                inputs,
            })
        {
            self.worker = None; // disconnected -> identical synchronous math
        }
    }

    pub fn set_projectile_grounded(&mut self, id: i32, on_ground: bool) {
        if let Some(vehicle) = self.vehicles.get_mut(&id)
            && let Some(display) = &mut vehicle.projectile
        {
            display.invalidate();
            display.on_ground = on_ground;
            display.stopped = display.in_ground || on_ground;
            if display.stopped {
                display.prev = vehicle.position;
                display.current = vehicle.position;
            }
        }
    }

    pub fn set_projectile_metadata(&mut self, id: i32, index: u8, value: MetaValue) {
        self.set_projectile_metadata_at(id, index, value, crate::version::session_protocol());
    }

    fn set_projectile_metadata_at(&mut self, id: i32, index: u8, value: MetaValue, protocol: i32) {
        if let Some(vehicle) = self.vehicles.get_mut(&id)
            && let Some(display) = &mut vehicle.projectile
        {
            if let (5, MetaValue::Bool(no_gravity)) = (index, value) {
                if display.no_gravity != no_gravity {
                    display.invalidate();
                }
                display.no_gravity = no_gravity;
            }
            // AbstractArrow inGround index 10 is a Boolean in 26.1 and 26.2.
            if matches!(
                vehicle.kind,
                Some(EntityKind::Arrow | EntityKind::SpectralArrow)
            ) && matches!(protocol, 775 | 776)
                && let (10, MetaValue::Bool(in_ground)) = (index, value)
            {
                display.invalidate();
                display.in_ground = in_ground;
                display.stopped = in_ground || display.on_ground;
                if in_ground {
                    display.prev = vehicle.position;
                    display.current = vehicle.position;
                }
            }
        }
    }

    pub fn set_mannequin_profile(
        &mut self,
        id: i32,
        profile: azalea_inventory::components::Profile,
    ) {
        if let Some(vehicle) = self.vehicles.get_mut(&id)
            && vehicle.kind == Some(EntityKind::Mannequin)
        {
            vehicle.mannequin_profile = Some(profile);
        }
    }

    pub fn set_armor_stand_rotation(&mut self, id: i32, index: u8, rotation: [f32; 3]) {
        if let Some(vehicle) = self.vehicles.get_mut(&id)
            && vehicle.kind == Some(EntityKind::ArmorStand)
            && (16..=21).contains(&index)
        {
            let pose = &mut vehicle.armor_stand_pose[usize::from(index - 16)];
            for (dst, src) in pose.iter_mut().zip(rotation) {
                *dst = if src.is_finite() { src % 360.0 } else { 0.0 };
            }
        }
    }

    pub fn set_armor_stand_equipment(
        &mut self,
        id: i32,
        slots: Vec<(
            azalea_inventory::components::EquipmentSlot,
            azalea_inventory::ItemStack,
        )>,
    ) {
        for (slot, item) in slots {
            let equipped = item.is_present();
            if let Some(entity) = self.living.get_mut(&id) {
                if equipped {
                    entity.equipment.insert(slot, item.clone());
                } else {
                    entity.equipment.remove(&slot);
                }
                if slot == azalea_inventory::components::EquipmentSlot::Saddle {
                    entity.saddled = equipped;
                }
            }
            if let Some(vehicle) = self.vehicles.get_mut(&id) {
                if equipped {
                    vehicle.armor_stand_equipment.insert(slot, item);
                } else {
                    vehicle.armor_stand_equipment.remove(&slot);
                }
            }
        }
    }

    pub fn set_item_frame_direction(
        &mut self,
        id: i32,
        direction: azalea_core::direction::Direction,
    ) {
        if let Some(vehicle) = self.vehicles.get_mut(&id)
            && matches!(
                vehicle.kind,
                Some(EntityKind::ItemFrame | EntityKind::GlowItemFrame)
            )
        {
            // The first direction initializes an already-centered spawn. Later
            // updates recover the attachment center from the old face normal.
            if let Some(old) = vehicle.item_frame_direction {
                vehicle.position += (Position::from(old.normal_vec3())
                    - Position::from(direction.normal_vec3()))
                    * 0.46875;
            }
            vehicle.item_frame_direction = Some(direction);
        }
    }

    pub fn set_item_frame_item(&mut self, id: i32, item: azalea_inventory::ItemStack) {
        if let Some(vehicle) = self.vehicles.get_mut(&id)
            && matches!(
                vehicle.kind,
                Some(EntityKind::ItemFrame | EntityKind::GlowItemFrame)
            )
        {
            vehicle.item_frame_item = item;
        }
    }

    pub fn set_item_frame_rotation(&mut self, id: i32, rotation: i32) {
        if let Some(vehicle) = self.vehicles.get_mut(&id)
            && matches!(
                vehicle.kind,
                Some(EntityKind::ItemFrame | EntityKind::GlowItemFrame)
            )
        {
            vehicle.item_frame_rotation = rotation;
        }
    }

    pub fn set_text_display_text(&mut self, id: i32, text: Vec<crate::ui::text::TextSpan>) {
        if let Some(vehicle) = self.vehicles.get_mut(&id)
            && vehicle.kind == Some(EntityKind::TextDisplay)
        {
            vehicle.text_display_text = Some(text);
        }
    }

    pub fn set_text_display_transform(
        &mut self,
        id: i32,
        index: u8,
        value: crate::net::TextDisplayTransformValue,
    ) {
        let Some(vehicle) = self.vehicles.get_mut(&id) else {
            return;
        };
        if vehicle.kind != Some(EntityKind::TextDisplay) {
            return;
        }
        use crate::net::TextDisplayTransformValue as Value;
        match (index, value) {
            (11, Value::Vector(v)) => vehicle.text_display_translation = v,
            (12, Value::Vector(v)) => vehicle.text_display_scale = v,
            (13, Value::Quaternion(q)) => vehicle.text_display_left_rotation = q,
            (14, Value::Quaternion(q)) => vehicle.text_display_right_rotation = q,
            (15, Value::Billboard(b)) => vehicle.text_display_billboard = b,
            _ => {}
        }
    }

    pub fn set_text_display_metadata(&mut self, id: i32, index: u8, value: MetaValue) {
        let Some(vehicle) = self.vehicles.get_mut(&id) else {
            return;
        };
        if vehicle.kind != Some(EntityKind::TextDisplay) {
            return;
        }
        match (index, value) {
            (17, MetaValue::Float(range)) => vehicle.text_display_view_range = range,
            (24, MetaValue::Int(width)) => vehicle.text_display_line_width = width,
            (25, MetaValue::Int(color)) => vehicle.text_display_background = color as u32,
            (26, MetaValue::Byte(opacity)) => vehicle.text_display_opacity = opacity,
            (27, MetaValue::Byte(flags)) => vehicle.text_display_flags = flags,
            _ => {}
        }
    }

    pub fn set_vehicle_rotation(&mut self, id: i32, look_dir: LookDirection) {
        if let Some(vehicle) = self.vehicles.get_mut(&id)
            && vehicle.look_dir.is_some()
        {
            vehicle.prev_look_dir = vehicle.look_dir;
            vehicle.look_dir = Some(look_dir);
        }
    }

    pub fn set_vehicle_spawn_transform(
        &mut self,
        id: i32,
        position: Position,
        velocity: DVec3,
        look_dir: LookDirection,
    ) {
        self.set_vehicle_transform(id, position, velocity);
        if let Some(vehicle) = self.vehicles.get_mut(&id) {
            vehicle.look_dir = Some(look_dir);
            vehicle.prev_look_dir = Some(look_dir);
        }
    }

    /// Resolve nested mount chains to their root; malformed cycles terminate
    /// safely.
    pub fn root_vehicle(&self, entity_id: i32) -> Option<i32> {
        let mut current = *self.vehicle_of.get(&entity_id)?;
        let mut steps = 0;
        while let Some(parent) = self.vehicle_of.get(&current) {
            current = *parent;
            steps += 1;
            if steps > self.vehicle_of.len() {
                return None;
            }
        }
        Some(current)
    }

    /// Vanilla's first-passenger controller rule plus caller-supplied vehicle
    /// policy.
    pub fn ride_authority(&self, passenger_id: i32, can_control_vehicle: bool) -> RideAuthority {
        let Some(vehicle_id) = self.vehicle_of.get(&passenger_id) else {
            return RideAuthority::Server;
        };
        if can_control_vehicle
            && self
                .vehicles
                .get(vehicle_id)
                .is_some_and(|v| v.passengers.first() == Some(&passenger_id))
        {
            RideAuthority::Client
        } else {
            RideAuthority::Server
        }
    }

    /// 26.2 AbstractHorse: a saddled horse is controlled by its first Player
    /// passenger. `is_local_player` must only be true for the logged-in
    /// client's player ID.
    pub fn horse_ride_authority(&self, passenger_id: i32, is_local_player: bool) -> RideAuthority {
        let Some(vehicle_id) = self.vehicle_of.get(&passenger_id) else {
            return RideAuthority::Server;
        };
        let Some(horse) = self.living.get(vehicle_id) else {
            return RideAuthority::Server;
        };
        if is_local_player
            && horse.entity_type == EntityKind::Horse
            && horse.saddled
            && self
                .vehicles
                .get(vehicle_id)
                .is_some_and(|v| v.passengers.first() == Some(&passenger_id))
        {
            RideAuthority::Client
        } else {
            RideAuthority::Server
        }
    }

    /// Apply the caller-resolved vanilla vehicle attachment point and passenger
    /// vehicle offset.
    pub fn passenger_position(
        &self,
        vehicle_id: i32,
        vehicle_attachment: DVec3,
        passenger_offset: DVec3,
    ) -> Option<Position> {
        let vehicle_position = self
            .living
            .get(&vehicle_id)
            .map(|entity| entity.position)
            .or_else(|| {
                self.vehicles
                    .get(&vehicle_id)
                    .map(|vehicle| vehicle.position)
            })?;
        Some((DVec3::from(vehicle_position) + vehicle_attachment - passenger_offset).into())
    }

    pub fn spawn_living(
        &mut self,
        id: i32,
        entity_type: EntityKind,
        position: Position,
        look_dir: LookDirection,
        body_y_rot_deg: f32,
        player_uuid: Option<uuid::Uuid>,
    ) {
        self.living.insert(
            id,
            LivingEntity::new(
                entity_type,
                position,
                look_dir,
                look_dir.y_rot_deg(),
                body_y_rot_deg,
                player_uuid,
            ),
        );
    }

    pub fn move_living_delta(&mut self, id: i32, dx: f64, dy: f64, dz: f64, on_ground: bool) {
        if let Some(entity) = self.living.get_mut(&id) {
            let target = entity.interp_target + DVec3::new(dx, dy, dz);
            entity.interpolate_to_pos(target);
            entity.on_ground = on_ground;
        }
    }

    pub fn teleport_living(&mut self, id: i32, position: Position, on_ground: bool) {
        if let Some(entity) = self.living.get_mut(&id) {
            entity.interpolate_to_pos(position);
            entity.on_ground = on_ground;
        }
    }

    /// Resolves a raw synched-entity-data scalar per (kind, index), the
    /// direct analogue of vanilla's per-class `onSyncedDataUpdated`. Index
    /// arithmetic follows the registration chain: `Entity` 0-7,
    /// `LivingEntity` 8-14, `Mob` 15, `AgeableMob` 16 baby + 17 age-locked,
    /// first subclass field 18, in the 26.x numbering
    /// (`normalize_ageable_index`).
    pub fn apply_entity_data(&mut self, id: i32, index: u8, value: MetaValue) {
        use MetaValue::{Bool, Byte, Float, Int, Long};
        let Some(entity) = self.living.get_mut(&id) else {
            return;
        };
        let kind = entity.entity_type;
        let index = normalize_player_index(kind, normalize_ageable_index(kind, index));
        match (kind, index, value) {
            // Shared entity flags byte, as registered by Entity.DATA_SHARED_FLAGS_ID.
            (_, 0, Byte(f)) => {
                entity.flags = f.into();
                entity.is_sprinting = entity.flags.sprinting;
            }
            (_, 8, Byte(f)) => {
                entity.using_item = f & 0x01 != 0 || f & 0x02 != 0;
                entity.using_offhand = f & 0x02 != 0;
                entity.riptide_spin = f & 0x04 != 0;
            }
            (_, 9, Float(h)) => entity.health = h,
            // Mob flags byte: bit 0x04 = aggressive. Players aren't mobs;
            // their 15 is Avatar's main hand (a byte on 1.21.9-1.21.10).
            (k, 15, Byte(f)) if k != EntityKind::Player => entity.aggressive = f & 0x04 != 0,
            (EntityKind::Player | EntityKind::Mannequin, 16, Byte(mask)) => {
                entity.skin_parts_mask = Some(mask & 0x7f)
            }
            (k, 16, Bool(b)) if is_baby_kind(k) => entity.is_baby = b,
            // Skeleton: powder-snow stray conversion; drives the vanilla
            // `isShaking` body jitter.
            (EntityKind::Skeleton, 16, Bool(b)) => entity.is_converting = b,
            (EntityKind::Bogged, 16, Bool(b)) => entity.is_sheared = b,
            // Slime size: 16 on 1.21.9-26.1.x, 18 since Slime joined
            // AgeableMob in 26.2.
            (EntityKind::Slime | EntityKind::MagmaCube, 16 | 18, Int(s)) => {
                entity.slime_size = s.clamp(1, 127) as u8
            }
            (EntityKind::SulfurCube, 18, Int(s)) => {
                entity.slime_size = s.clamp(1, 127) as u8;
                entity.sulfur_cube_size = s.max(1);
            }
            (EntityKind::Mooshroom, 18, Int(t)) => entity.variant = t.clamp(0, 1) as u32,
            (EntityKind::Bee, 18, Byte(f)) => entity.bee_flags = f,
            (EntityKind::Bee, 19, Long(t)) => entity.anger_end_time = t,
            (EntityKind::Ghast, 16, Bool(b)) => entity.ghast_charging = b,
            (EntityKind::Vex, 16, Byte(f)) => entity.vex_charging = f & 0x01 != 0,
            (EntityKind::Phantom, 16, Int(s)) => entity.phantom_size = s.max(0),
            (EntityKind::Shulker, 16, MetaValue::Direction(face)) => {
                entity.shulker_attach_face = face
            }
            (EntityKind::Shulker, 17, Byte(p)) => entity.shulker_peek = p,
            (EntityKind::Shulker, 18, Byte(c)) => entity.variant = (c & 0xFF) as u32,
            (EntityKind::Wither, 19, Int(t)) => entity.wither_invulnerability = t.max(0),
            // Sheep wool byte: low nibble = DyeColor, bit 0x10 = sheared.
            (EntityKind::Sheep, 18, Byte(w)) => {
                entity.wool_color = Some(w & 0x0F);
                entity.is_sheared = w & 0x10 != 0;
            }
            (EntityKind::Creeper, 17, Bool(b)) => entity.powered = b,
            (EntityKind::Enderman, 17, Bool(b)) => entity.is_creepy = b,
            (EntityKind::Witch, 17, Bool(b)) => entity.witch_drinking = b,
            // Zombie-family underwater conversion / zombie villager curing.
            (EntityKind::Zombie | EntityKind::Husk | EntityKind::Drowned, 18, Bool(b))
            | (EntityKind::ZombieVillager, 19, Bool(b)) => entity.is_converting = b,
            (EntityKind::Villager, 18, Int(c)) => entity.unhappy_counter = c,
            // Vanilla sparse rabbit id map: 99 = evil, unknown ids fall back
            // to brown.
            (EntityKind::Rabbit, 18, Int(v)) => {
                entity.variant = match v {
                    0..=5 => v as u32,
                    99 => 6,
                    _ => 0,
                }
            }
            // Axolotl.DATA_VARIANT: legacy enum order Lucy, Wild, Gold, Cyan, Blue.
            (EntityKind::Axolotl, 18, Int(v)) => {
                entity.variant = if (0..=4).contains(&v) { v as u32 } else { 0 }
            }
            // Equine flags byte: bit 0x10 = eating, 0x20 = standing (rear),
            // 0x40 = open mouth.
            (k, 18, Byte(f)) if is_equine(&k) => {
                entity.is_tame = f & 0x02 != 0;
                entity.is_eating = f & 0x10 != 0;
                entity.is_standing = f & 0x20 != 0;
                entity.is_open_mouth = f & 0x40 != 0;
            }
            (EntityKind::Donkey | EntityKind::Mule, 19, Bool(b)) => entity.has_chest = b,
            // Horse packed variant: `color | markings << 8`, both wrapping
            // their id ranges (vanilla `ByIdMap` WRAP).
            (EntityKind::Horse, 19, Int(v)) => {
                let v = v as u32;
                entity.variant = ((v & 0xFF) % 7) | ((((v >> 8) & 0xFF) % 5) << 8);
            }
            // Tamable flags byte: bit 0x01 = sitting, 0x04 = tame.
            (EntityKind::Wolf | EntityKind::Cat, 18, Byte(f)) => {
                entity.is_sitting = f & 0x01 != 0;
                entity.is_tame = f & 0x04 != 0;
            }
            (EntityKind::Wolf, 20, Bool(b)) => entity.is_interested = b,
            (EntityKind::Wolf, 21, Int(c)) | (EntityKind::Cat, 23, Int(c)) => {
                entity.collar_color = c as u8 & 0x0F
            }
            (EntityKind::Cat, 21, Bool(b)) => entity.is_lying = b,
            // Persistent anger: a game-time end tick since 1.21.11; on
            // 1.21.9-1.21.10 a remaining-tick countdown the server re-syncs
            // as it decrements, so any positive value means angry.
            (EntityKind::Wolf, 22, Long(t)) => entity.anger_end_time = t,
            (EntityKind::Wolf, 22, Int(t)) => {
                entity.anger_end_time = if t > 0 { i64::MAX } else { -1 }
            }
            (EntityKind::Cat, 22, Bool(b)) => entity.relax_state_one = b,
            // Bat flags byte: bit 0x01 = resting (hanging); a flip restarts
            // the fly/rest animation clock.
            (EntityKind::Bat, 16, Byte(f)) => {
                let resting = f & 0x01 != 0;
                if entity.bat_resting != resting {
                    entity.bat_resting = resting;
                    entity.bat_anim_start = Some(entity.age_in_ticks + 1);
                }
            }
            // Salmon size ids clamp to SMALL..LARGE; a pufferfish puff state
            // outside 0/1 renders big (vanilla `switch` default).
            (EntityKind::Salmon, 17, Int(v)) => entity.variant = v.clamp(0, 2) as u32,
            (EntityKind::TropicalFish, 17, Int(v)) => entity.variant = v as u32,
            (EntityKind::Pufferfish, 17, Int(s)) => {
                entity.puff_state = if (0..=1).contains(&s) { s as u8 } else { 2 }
            }
            (EntityKind::GlowSquid, 18, Int(t)) => entity.dark_ticks = t,
            _ => {}
        }
    }

    pub fn set_crouching(&mut self, id: i32, is_crouching: bool) {
        self.set_pose(
            id,
            if is_crouching {
                EntityPose::Crouching
            } else {
                EntityPose::Standing
            },
        );
    }

    /// Stores Avatar DATA_PLAYER_MAIN_HAND metadata for player entities.
    pub fn set_main_arm(&mut self, id: i32, right: bool) {
        if let Some(entity) = self.living.get_mut(&id)
            && entity.entity_type == EntityKind::Player
        {
            entity.main_arm_right = right;
        }
        if let Some(entity) = self.vehicles.get_mut(&id)
            && entity.kind == Some(EntityKind::Mannequin)
        {
            entity.mannequin_main_arm_right = right;
        }
    }

    pub fn set_mannequin_customization(&mut self, id: i32, mask: u8) {
        if let Some(entity) = self.vehicles.get_mut(&id)
            && entity.kind == Some(EntityKind::Mannequin)
        {
            entity.mannequin_skin_parts_mask = mask & 0x7f;
        }
    }

    /// Stores the complete metadata pose without collapsing
    /// swimming/crawling/sleeping.
    pub fn set_pose(&mut self, id: i32, pose: EntityPose) {
        if let Some(entity) = self.living.get_mut(&id) {
            entity.pose = pose;
            entity.is_crouching = pose == EntityPose::Crouching;
        }
        if let Some(entity) = self.vehicles.get_mut(&id)
            && entity.kind == Some(EntityKind::Mannequin)
        {
            entity.mannequin_pose = pose;
        }
    }

    pub fn set_sleeping_pos(&mut self, id: i32, pos: Option<BlockPos>) {
        if let Some(entity) = self.living.get_mut(&id) {
            entity.sleeping_pos = pos;
        }
    }

    /// Resolve dimensions only where 26.2 defines a common pose override.
    /// Other poses retain per-EntityType dimensions, which require ingress type
    /// dimensions.
    pub fn dimensions_for_pose(base: EntityDimensions, pose: EntityPose) -> EntityDimensions {
        match pose {
            EntityPose::Sleeping => EntityDimensions {
                width: 0.2,
                height: 0.2,
            },
            _ => base,
        }
    }

    /// `kind` is the mob the emitting handler arm resolved the value for;
    /// metadata indices are overloaded across kinds, so a mismatched entity
    /// ignores the write.
    pub fn set_variant(&mut self, id: i32, kind: EntityKind, raw: u32) {
        if let Some(entity) = self.living.get_mut(&id)
            && entity.entity_type == kind
        {
            entity.variant = raw;
        }
    }

    pub fn set_villager_data(
        &mut self,
        id: i32,
        kind: VillagerKind,
        profession: VillagerProfession,
        level: u32,
    ) {
        if let Some(entity) = self.living.get_mut(&id)
            && matches!(
                entity.entity_type,
                EntityKind::Villager | EntityKind::ZombieVillager
            )
        {
            entity.villager_kind = kind;
            entity.villager_profession = profession;
            entity.villager_level = level;
        }
    }

    pub fn start_sheep_eat(&mut self, id: i32) {
        if let Some(entity) = self.living.get_mut(&id)
            && entity.entity_type == EntityKind::Sheep
        {
            entity.eat_anim_tick = 40;
            entity.prev_eat_anim_tick = 40;
        }
    }

    pub fn mark_dead(&mut self, id: i32) {
        if let Some(entity) = self.living.get_mut(&id)
            && entity.entity_type != EntityKind::Player
        {
            entity.health = 0.0;
            entity.is_crouching = false;
        }
    }

    /// Mirrors vanilla `LivingEntity.handleDamageEvent`: `hurtTime = 10`.
    pub fn mark_hurt(&mut self, id: i32) {
        if let Some(entity) = self.living.get_mut(&id) {
            entity.hurt_time = HURT_DURATION;
        }
    }

    pub fn set_custom_name(&mut self, id: i32, name: Option<String>) {
        if let Some(entity) = self.living.get_mut(&id) {
            entity.custom_name = name;
        }
    }

    /// Wolf wet-shake start / cancel (entity events 8 / 56).
    pub fn set_wolf_shaking(&mut self, id: i32, shaking: bool) {
        if let Some(entity) = self.living.get_mut(&id)
            && entity.entity_type == EntityKind::Wolf
        {
            entity.is_shaking = shaking;
            entity.shake_anim = 0.0;
            entity.prev_shake_anim = 0.0;
        }
    }

    /// Rabbit hop (entity event 1): vanilla sets a 15-tick jump run.
    pub fn start_rabbit_jump(&mut self, id: i32) {
        if let Some(entity) = self.living.get_mut(&id)
            && entity.entity_type == EntityKind::Rabbit
        {
            entity.jump_duration = 15;
            entity.jump_ticks = 0;
        }
    }

    pub fn set_attribute(&mut self, id: i32, key: impl Into<String>, value: f64) {
        if let Some(entity) = self.living.get_mut(&id) {
            entity.attributes.insert(key.into(), value);
        }
    }

    pub fn set_effect(&mut self, id: i32, effect: EntityEffect) {
        if let Some(entity) = self.living.get_mut(&id) {
            entity.effects.insert(effect.effect_id, effect);
        }
    }

    pub fn remove_effect(&mut self, id: i32, effect_id: u32) {
        if let Some(entity) = self.living.get_mut(&id) {
            entity.effects.remove(&effect_id);
        }
    }

    pub fn set_living_motion(&mut self, id: i32, velocity: DVec3) {
        if let Some(entity) = self.living.get_mut(&id) {
            entity.velocity = velocity;
        }
    }

    /// Entity event 19: the server rolled the tentacle clock over.
    pub fn squid_tentacle_reset(&mut self, id: i32) {
        if let Some(entity) = self.living.get_mut(&id)
            && matches!(
                entity.entity_type,
                EntityKind::Squid | EntityKind::GlowSquid
            )
        {
            entity.tentacle_movement = 0.0;
        }
    }

    /// Iron golem punch (entity event 4): vanilla runs a 10-tick swing.
    pub fn golem_punch(&mut self, id: i32) {
        if let Some(entity) = self.living.get_mut(&id)
            && entity.entity_type == EntityKind::IronGolem
        {
            entity.golem_attack_ticks = 10;
        }
    }

    /// Iron golem flower offer start / stop (entity events 11 / 34): a
    /// 400-tick hold.
    pub fn set_golem_offering_flower(&mut self, id: i32, offering: bool) {
        if let Some(entity) = self.living.get_mut(&id)
            && entity.entity_type == EntityKind::IronGolem
        {
            entity.golem_offer_flower_ticks = if offering { 400 } else { 0 };
        }
    }

    /// Begins an arm swing (server `Animate` packet). Restarts when idle or
    /// past the halfway point (vanilla `LivingEntity.swing`); `swing_time`
    /// counts down, so that is `swing_time <= SWING_DURATION / 2`.
    pub fn start_swing(&mut self, id: i32) {
        if let Some(entity) = self.living.get_mut(&id)
            && entity.swing_time <= SWING_DURATION / 2
        {
            entity.swing_time = SWING_DURATION;
        }
    }

    /// Rotation half of any movement packet: rotation plus onGround. Extends
    /// any in-flight position lerp instead of re-targeting it (vanilla
    /// `moveOrInterpolateTo` rotation overloads).
    pub fn rotate_living(&mut self, id: i32, y_rot_deg: f32, x_rot_deg: f32, on_ground: bool) {
        if let Some(entity) = self.living.get_mut(&id) {
            entity.interp_look_dir = LookDirection::new(y_rot_deg, x_rot_deg);
            entity.interp_steps = entity.interp_steps.max(INTERPOLATION_STEPS);
            entity.on_ground = on_ground;
        }
    }

    pub fn update_head_rotation(&mut self, id: i32, head_y_rot_deg: f32) {
        if let Some(entity) = self.living.get_mut(&id) {
            entity.interp_head_y_rot_deg = head_y_rot_deg;
            entity.interp_head_y_rot_steps = INTERPOLATION_STEPS;
        }
    }

    /// Event 3 removes an impacted snowball visually before RemoveEntities
    /// arrives. Only a tracked snowball may be removed; a later removal is
    /// harmless.
    pub fn remove_impacted_snowball(&mut self, id: i32) -> bool {
        if self.vehicles.get(&id).and_then(|v| v.kind) != Some(EntityKind::Snowball) {
            return false;
        }
        self.remove_entity(id);
        true
    }

    /// Remove one entity and direct graph edges without deleting passenger
    /// subtrees.
    pub fn remove_entity(&mut self, id: i32) -> Option<LivingEntity> {
        for vehicle in self.vehicles.values_mut() {
            vehicle.passengers.retain(|&passenger| passenger != id);
        }
        self.vehicle_of.remove(&id);
        self.vehicle_of.retain(|_, vehicle_id| *vehicle_id != id);
        self.vehicles.remove(&id);
        self.living.remove(&id)
    }

    pub fn remove_living(&mut self, id: i32) -> Option<LivingEntity> {
        self.remove_entity(id)
    }

    pub fn has_player_uuid(&self, uuid: &uuid::Uuid) -> bool {
        self.player_by_uuid(uuid).is_some()
    }

    pub fn player_by_uuid(&self, uuid: &uuid::Uuid) -> Option<&LivingEntity> {
        self.living
            .values()
            .find(|entity| entity.player_uuid == Some(*uuid))
    }

    pub fn tick_living(
        &mut self,
        chunks: &ChunkStore,
        player_position: Position,
        simulation_distance: u32,
    ) {
        for entity in self.living.values_mut() {
            entity.tick_interpolation();
            if entity.entity_type == EntityKind::Shulker {
                entity.prev_shulker_peek_amount = entity.shulker_peek_amount;
                let target = entity.shulker_peek as f32 * 0.01;
                entity.shulker_peek_amount = if entity.shulker_peek_amount > target {
                    (entity.shulker_peek_amount - 0.05).clamp(target, 1.0)
                } else {
                    (entity.shulker_peek_amount + 0.05).clamp(0.0, target)
                };
            }
            entity.tick_body_rotation();
            let dx = entity.position.x - entity.prev_position.x;
            let dz = entity.position.z - entity.prev_position.z;
            if entity.health <= 0.0 {
                stop_walk_animation(
                    &mut entity.walk_anim_pos,
                    &mut entity.walk_anim_speed,
                    &mut entity.prev_walk_anim_speed,
                );
            } else {
                update_walk_animation(
                    dx,
                    dz,
                    &mut entity.walk_anim_pos,
                    &mut entity.walk_anim_speed,
                    &mut entity.prev_walk_anim_speed,
                );
            }
            if probes_water(&entity.entity_type) {
                entity.is_in_water = entity.probe_water(chunks);
            }
            entity.tick_kind_anims();
            entity.tick_tamable_anims();
            entity.prev_eat_anim_tick = entity.eat_anim_tick;
            if entity.eat_anim_tick > 0 {
                entity.eat_anim_tick -= 1;
            }
            if entity.hurt_time > 0 {
                entity.hurt_time -= 1;
            }
            tick_death_time(
                entity.health,
                within_simulation_distance(entity.position, player_position, simulation_distance),
                &mut entity.death_time,
            );
            if entity.swing_time > 0 {
                entity.swing_time -= 1;
            }
            if entity.unhappy_counter > 0 {
                entity.unhappy_counter -= 1;
            }
            entity.age_in_ticks = entity.age_in_ticks.wrapping_add(1);
        }
    }
}

pub fn stop_walk_animation(walk_pos: &mut f32, walk_speed: &mut f32, prev_walk_speed: &mut f32) {
    *prev_walk_speed = 0.0;
    *walk_speed = 0.0;
    *walk_pos = 0.0;
}

pub fn update_walk_animation(
    dx: f64,
    dz: f64,
    walk_pos: &mut f32,
    walk_speed: &mut f32,
    prev_walk_speed: &mut f32,
) {
    let distance = ((dx * dx + dz * dz) as f32).sqrt();
    let target_speed = (distance * 4.0).min(1.0);
    *prev_walk_speed = *walk_speed;
    *walk_speed += (target_speed - *walk_speed) * 0.4;
    *walk_pos += *walk_speed;
}

pub fn wrap_degrees(deg: f32) -> f32 {
    let mut d = deg % 360.0;
    if d >= 180.0 {
        d -= 360.0;
    }
    if d < -180.0 {
        d += 360.0;
    }
    d
}

pub fn lerp_angle(from: f32, to: f32, alpha: f32) -> f32 {
    from + wrap_degrees(to - from) * alpha
}

/// The horse family (vanilla `AbstractHorse` subclasses pomme renders).
pub fn is_equine(kind: &EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Horse
            | EntityKind::Donkey
            | EntityKind::Mule
            | EntityKind::SkeletonHorse
            | EntityKind::ZombieHorse
    )
}

/// Entity kinds accepted by vanilla's `AbstractHorse` inventory screen gate.
pub fn supports_horse_inventory(kind: &EntityKind) -> bool {
    is_equine(kind)
        || matches!(
            kind,
            EntityKind::Llama | EntityKind::TraderLlama | EntityKind::Camel | EntityKind::CamelHusk
        )
}

pub fn is_living_mob(kind: &EntityKind) -> bool {
    is_equine(kind)
        || matches!(
            kind,
            EntityKind::Llama | EntityKind::TraderLlama | EntityKind::Camel | EntityKind::CamelHusk
        )
        || matches!(
            kind,
            EntityKind::Player
                | EntityKind::Pig
                | EntityKind::Cow
                | EntityKind::CaveSpider
                | EntityKind::Sheep
                | EntityKind::Chicken
                | EntityKind::Zombie
                | EntityKind::Skeleton
                | EntityKind::Creeper
                | EntityKind::Spider
                | EntityKind::Villager
                | EntityKind::Enderman
                | EntityKind::Slime
                | EntityKind::Witch
                | EntityKind::Husk
                | EntityKind::Drowned
                | EntityKind::ZombieVillager
                | EntityKind::Stray
                | EntityKind::Bogged
                | EntityKind::Wolf
                | EntityKind::Cat
                | EntityKind::Ocelot
                | EntityKind::Rabbit
                | EntityKind::Squid
                | EntityKind::GlowSquid
                | EntityKind::Bat
                | EntityKind::Cod
                | EntityKind::Salmon
                | EntityKind::TropicalFish
                | EntityKind::Pufferfish
                | EntityKind::IronGolem
                | EntityKind::Allay
                | EntityKind::Armadillo
                | EntityKind::Axolotl
                | EntityKind::Bee
                | EntityKind::Blaze
                | EntityKind::Breeze
                | EntityKind::CopperGolem
                | EntityKind::Creaking
                | EntityKind::Dolphin
                | EntityKind::ElderGuardian
                | EntityKind::Endermite
                | EntityKind::Evoker
                | EntityKind::Fox
                | EntityKind::Frog
                | EntityKind::Ghast
                | EntityKind::Giant
                | EntityKind::Goat
                | EntityKind::Guardian
                | EntityKind::HappyGhast
                | EntityKind::Hoglin
                | EntityKind::Illusioner
                | EntityKind::MagmaCube
                | EntityKind::Mooshroom
                | EntityKind::Nautilus
                | EntityKind::Panda
                | EntityKind::Parched
                | EntityKind::Parrot
                | EntityKind::Phantom
                | EntityKind::Piglin
                | EntityKind::PiglinBrute
                | EntityKind::Pillager
                | EntityKind::PolarBear
                | EntityKind::Ravager
                | EntityKind::Shulker
                | EntityKind::Silverfish
                | EntityKind::Sniffer
                | EntityKind::SnowGolem
                | EntityKind::Strider
                | EntityKind::SulfurCube
                | EntityKind::Tadpole
                | EntityKind::Turtle
                | EntityKind::Vex
                | EntityKind::Vindicator
                | EntityKind::WanderingTrader
                | EntityKind::Warden
                | EntityKind::Wither
                | EntityKind::WitherSkeleton
                | EntityKind::Zoglin
                | EntityKind::ZombieNautilus
                | EntityKind::ZombifiedPiglin
        )
}

/// Kinds whose `wasTouchingWater` matters for rendering (fish flop pose,
/// squid body rotation).
fn probes_water(kind: &EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Squid
            | EntityKind::GlowSquid
            | EntityKind::Cod
            | EntityKind::Salmon
            | EntityKind::TropicalFish
            | EntityKind::Pufferfish
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falling_block_wire_state_gate_retains_raw_data_without_foreign_remapping() {
        let native = pomme_protocol::version::NATIVE.protocol;
        assert!(decode_falling_block_state(Some(EntityKind::FallingBlock), 0, native).is_some());
        assert!(decode_falling_block_state(Some(EntityKind::FallingBlock), -1, native).is_none());
        assert!(
            decode_falling_block_state(Some(EntityKind::FallingBlock), i32::MAX, native).is_none()
        );
        assert!(
            decode_falling_block_state(Some(EntityKind::FallingBlock), 0, native - 1).is_none()
        );
        assert!(decode_falling_block_state(Some(EntityKind::Tnt), 0, native).is_none());

        let mut store = projectile(EntityKind::Tnt, Position::default(), DVec3::ZERO);
        store.set_vehicle_spawn_data(1, 0);
        assert_eq!(store.vehicles[&1].spawn_data, Some(0));
        assert_eq!(store.vehicles[&1].falling_block.state, None);
    }

    #[test]
    fn falling_block_start_pos_metadata_is_exactly_retained() {
        let mut store = projectile(EntityKind::FallingBlock, Position::default(), DVec3::ZERO);
        let start = BlockPos::new(-31, -48, 17);
        store.apply_vehicle_metadata(1, 8, MetaValue::BlockPos(start));
        assert_eq!(store.vehicles[&1].falling_block.start_pos, start);
    }

    #[test]
    fn falling_block_sample_and_root_matrix_match_renderer_coordinates() {
        let position = Position::new(4.25, 8.05, -2.75);
        assert_eq!(falling_block_sample_pos(position), BlockPos::new(4, 9, -3));
        let anchor = DVec3::new(4.0, 8.0, -3.0);
        let matrix = falling_block_model_matrix(position, anchor);
        assert_eq!(matrix.w_axis.truncate(), glam::Vec3::new(0.25, 0.55, 0.25));
    }

    #[test]
    fn shulker_bullet_renderer_age_is_fixed_tick_interpolatable_and_resets_on_reuse() {
        let chunks = ChunkStore::new(1);
        let mut store = projectile(EntityKind::ShulkerBullet, Position::default(), DVec3::ZERO);
        assert_eq!(store.vehicles[&1].projectile_age, 0);
        store.tick_projectile_displays(&chunks);
        assert_eq!(
            (
                store.vehicles[&1].projectile_prev_age,
                store.vehicles[&1].projectile_age
            ),
            (0, 1)
        );
        store.tick_projectile_displays(&chunks);
        assert_eq!(
            (
                store.vehicles[&1].projectile_prev_age,
                store.vehicles[&1].projectile_age
            ),
            (1, 2)
        );
        store.set_vehicle_kind(1, EntityKind::ShulkerBullet);
        assert_eq!(
            (
                store.vehicles[&1].projectile_prev_age,
                store.vehicles[&1].projectile_age
            ),
            (0, 0)
        );
    }

    fn projectile(kind: EntityKind, position: Position, velocity: DVec3) -> EntityStore {
        let mut store = EntityStore::new();
        store.set_vehicle_spawn_transform(1, position, velocity, LookDirection::default());
        store.set_vehicle_kind(1, kind);
        store
    }

    #[test]
    fn primed_tnt_fuse_metadata_resyncs_and_ticks_at_fixed_rate() {
        let mut store = projectile(EntityKind::Tnt, Position::default(), DVec3::ZERO);
        store.apply_vehicle_metadata(1, 8, MetaValue::Int(10));
        assert_eq!(store.vehicles[&1].tnt_fuse, 10);
        assert_eq!(store.vehicles[&1].tnt_prev_fuse, 11);
        let chunks = ChunkStore::new(1);
        store.tick_projectile_displays(&chunks);
        assert_eq!(store.vehicles[&1].tnt_prev_fuse, 10);
        assert_eq!(store.vehicles[&1].tnt_fuse, 9);
        store.apply_vehicle_metadata(1, 8, MetaValue::Int(4));
        assert_eq!(store.vehicles[&1].tnt_prev_fuse, 5);
        assert_eq!(store.vehicles[&1].tnt_fuse, 4);
    }

    #[test]
    fn boat_metadata_uses_versioned_indices_and_never_enters_living_store() {
        let mut store = EntityStore::new();
        store.set_vehicle_spawn_transform(
            7,
            Position::new(1.0, 2.0, 3.0),
            DVec3::ZERO,
            LookDirection::new(90.0, 0.0),
        );
        store.set_vehicle_kind(7, EntityKind::OakChestBoat);
        for (index, value) in [
            (8, MetaValue::Int(4)),
            (9, MetaValue::Int(-1)),
            (10, MetaValue::Float(6.5)),
            (11, MetaValue::Bool(true)),
            (12, MetaValue::Bool(false)),
            (13, MetaValue::Int(3)),
            (0, MetaValue::Byte(0x20)),
        ] {
            store.apply_vehicle_metadata(7, index, value);
        }
        let boat = &store.vehicles[&7];
        assert_eq!((boat.boat_hurt_time, boat.boat_hurt_direction), (4, -1));
        assert_eq!(boat.boat_damage, 6.5);
        assert!(boat.boat_left_paddle);
        assert!(!boat.boat_right_paddle);
        assert_eq!(boat.boat_bubble_time, 3);
        assert_eq!(boat.shared_flags, 0x20);
        assert!(store.living.is_empty());

        store.set_vehicle_spawn_transform(
            8,
            Position::default(),
            DVec3::ZERO,
            LookDirection::default(),
        );
        store.set_vehicle_kind(8, EntityKind::Minecart);
        store.apply_vehicle_metadata(8, 8, MetaValue::Int(99));
        assert_eq!(store.vehicles[&8].boat_hurt_time, 0);
    }

    #[test]
    fn minecart_metadata_keeps_optional_display_state_and_native_offsets() {
        for (kind, default_offset) in [
            (EntityKind::Minecart, 6),
            (EntityKind::ChestMinecart, 8),
            (EntityKind::FurnaceMinecart, 6),
            (EntityKind::TntMinecart, 6),
            (EntityKind::HopperMinecart, 1),
            (EntityKind::CommandBlockMinecart, 6),
            (EntityKind::SpawnerMinecart, 6),
        ] {
            let mut store = projectile(kind, Position::default(), DVec3::ZERO);
            assert_eq!(store.vehicles[&1].minecart_display_offset, default_offset);
            store.apply_vehicle_metadata(1, 11, MetaValue::OptionalBlockState(Some(321)));
            store.apply_vehicle_metadata(1, 12, MetaValue::Int(14));
            let cart = &store.vehicles[&1];
            assert_eq!(cart.minecart_display_state, Some(321));
            assert_eq!(cart.minecart_display_offset, 14);
            assert!(store.living.is_empty());
            store.apply_vehicle_metadata(1, 11, MetaValue::OptionalBlockState(None));
            assert_eq!(store.vehicles[&1].minecart_display_state, None);
        }
        let mut chest = projectile(EntityKind::ChestMinecart, Position::default(), DVec3::ZERO);
        assert_eq!(chest.vehicles[&1].minecart_display_offset, 8);
        let mut hopper = projectile(EntityKind::HopperMinecart, Position::default(), DVec3::ZERO);
        assert_eq!(hopper.vehicles[&1].minecart_display_offset, 1);
    }

    #[test]
    fn sixteen_projectiles_integrate_every_fixed_tick_and_interpolate() {
        crate::world::block::init("26.2");
        let mut chunks = ChunkStore::new(1);
        chunks.load_decoded_chunk(ChunkPos::new(0, 0), azalea_world::chunk::Chunk::default());
        let mut store = EntityStore::new();
        for id in 0..16 {
            store.set_vehicle_spawn_transform(
                id,
                Position::new(2.0, 70.5, 2.5),
                DVec3::X,
                LookDirection::default(),
            );
            store.set_vehicle_kind(id, EntityKind::Arrow);
        }
        store.tick_projectile_displays(&chunks);
        store.tick_projectile_displays(&chunks);
        for vehicle in store.vehicles.values() {
            let display = vehicle.projectile.as_ref().unwrap();
            assert!((display.current.x - (3.0 + 0.99_f32 as f64)).abs() < 1e-12);
            assert_eq!(display.prev.x, 3.0);
            assert!(display.position(0.9).x > display.position(0.1).x);
        }
    }

    #[test]
    fn held_worker_never_loses_ticks_or_rewinds_interpolation() {
        crate::world::block::init("26.2");
        let chunks = ChunkStore::new(1);
        let mut store = EntityStore::new();
        for id in 0..THRESHOLD as i32 {
            store.set_vehicle_spawn_transform(
                id,
                Position::new(2.0, 70.0, 2.0),
                DVec3::X,
                LookDirection::default(),
            );
            store.set_vehicle_kind(id, EntityKind::Arrow);
        }
        let (worker, _jobs, results) = Worker::held();
        store.worker = Some(worker);
        store.tick_projectile_displays(&chunks);
        let source = store.vehicles[&0].projectile.as_ref().unwrap();
        let flight = projectile::flight(Input {
            id: 0,
            revision: source.revision,
            start: store.tick,
            source: Frame {
                position: source.current,
                velocity: source.velocity,
            },
            arrow: true,
            no_gravity: false,
            drag: source.drag,
        });
        store.tick_projectile_displays(&chunks);
        let p = store.vehicles[&0].projectile.as_ref().unwrap();
        assert!((p.current.x - (3.0 + 0.99_f32 as f64)).abs() < 1e-12);
        assert_eq!(p.prev.x, 3.0);
        assert!(p.position(0.1).x > 3.0);
        assert!(p.position(0.9).x > p.position(0.1).x);
        let mut before = p.position(0.9).x;
        for _ in 0..2 {
            store.tick_projectile_displays(&chunks);
            let p = store.vehicles[&0].projectile.as_ref().unwrap();
            assert!(p.position(0.1).x > before, "0.9 -> 0.1 must not rewind");
            before = p.position(0.9).x;
        }
        results
            .send(projectile::Result {
                epoch: store.epoch,
                flights: vec![flight],
            })
            .unwrap();
        store.tick_projectile_displays(&chunks);
        assert!(
            store.vehicles[&0]
                .projectile
                .as_ref()
                .unwrap()
                .frames
                .is_some()
        );
        assert!(
            store.vehicles[&0]
                .projectile
                .as_ref()
                .unwrap()
                .position(0.1)
                .x
                > before
        );
    }

    #[test]
    fn sync_and_worker_match_for_all_three_kinds_across_two_horizons() {
        crate::world::block::init("26.2");
        let chunks = ChunkStore::new(1);
        let mut async_store = EntityStore::new();
        let mut sync_store = EntityStore::new();
        for store in [&mut async_store, &mut sync_store] {
            for id in 0..16 {
                store.set_vehicle_spawn_transform(
                    id,
                    Position::new(2.0, 70.0, 2.0),
                    DVec3::new(1.0, 0.2, 0.0),
                    LookDirection::default(),
                );
                store.set_vehicle_kind(
                    id,
                    match id % 3 {
                        0 => EntityKind::Arrow,
                        1 => EntityKind::SpectralArrow,
                        _ => EntityKind::Snowball,
                    },
                );
            }
        }
        // Suppress spawning in the reference store without altering its math.
        let (mut sync_worker, _jobs, _results) = Worker::held();
        sync_worker.in_flight = true;
        sync_store.worker = Some(sync_worker);
        let (mut worker, jobs, results) = Worker::held();
        worker.in_flight = false;
        async_store.worker = Some(worker);
        for tick in 1..=140 {
            async_store.tick_projectile_displays(&chunks);
            sync_store.tick_projectile_displays(&chunks);
            for id in 0..16 {
                let actual = async_store.vehicles[&id].projectile.as_ref().unwrap();
                let expected = sync_store.vehicles[&id].projectile.as_ref().unwrap();
                assert_eq!(
                    (actual.prev, actual.current, actual.velocity),
                    (expected.prev, expected.current, expected.velocity),
                    "entity {id} tick {tick}"
                );
            }
            if let Ok(job) = jobs.try_recv() {
                results
                    .send(projectile::Result {
                        epoch: async_store.epoch,
                        flights: job.inputs.into_iter().map(projectile::flight).collect(),
                    })
                    .unwrap();
            }
        }
        assert!(
            async_store
                .vehicles
                .values()
                .any(|v| v.projectile.as_ref().unwrap().frames.is_some())
        );
    }

    #[test]
    fn late_worker_and_misordered_packet_cannot_snap_to_an_old_path() {
        crate::world::block::init("26.2");
        let mut chunks = ChunkStore::new(1);
        chunks.load_decoded_chunk(ChunkPos::new(0, 0), azalea_world::chunk::Chunk::default());
        let mut store = EntityStore::new();
        for id in 0..16 {
            store.set_vehicle_spawn_transform(
                id,
                Position::new(2.0, 70.0, 2.0),
                DVec3::X,
                LookDirection::default(),
            );
            store.set_vehicle_kind(id, EntityKind::Arrow);
        }
        let (mut worker, jobs, results) = Worker::held();
        worker.in_flight = false;
        store.worker = Some(worker);
        store.tick_projectile_displays(&chunks);
        let first = jobs.try_recv().unwrap();
        assert_eq!(first.inputs.len(), 16);
        // A water update while the job is held changes the synchronous path.
        chunks.set_block_state(
            3,
            70,
            2,
            crate::world::block::first_state_of("water").unwrap(),
        );
        store
            .vehicles
            .get_mut(&0)
            .unwrap()
            .projectile
            .as_mut()
            .unwrap()
            .medium_tick = 0;
        for _ in 0..3 {
            store.tick_projectile_displays(&chunks);
        }
        let before = store.vehicles[&0]
            .projectile
            .as_ref()
            .unwrap()
            .position(0.9)
            .x;
        results
            .send(projectile::Result {
                epoch: store.epoch,
                flights: first.inputs.into_iter().map(projectile::flight).collect(),
            })
            .unwrap();
        store.tick_projectile_displays(&chunks);
        let p = store.vehicles[&0].projectile.as_ref().unwrap();
        assert!(p.frames.is_none(), "changed drag must reject a late flight");
        assert!(p.position(0.1).x > before);
        assert_eq!(
            store.vehicles[&1]
                .projectile
                .as_ref()
                .unwrap()
                .frames
                .as_ref()
                .unwrap()
                .start,
            1
        );

        // A packet correction invalidates even a future trajectory from the same id.
        let stale = projectile::flight(Input {
            id: 0,
            revision: p.revision,
            start: store.tick + 1,
            source: Frame {
                position: p.current,
                velocity: p.velocity,
            },
            arrow: true,
            no_gravity: false,
            drag: p.drag,
        });
        let previous = p.current.x;
        store.set_vehicle_motion(0, DVec3::new(2.0, 0.0, 0.0));
        results
            .send(projectile::Result {
                epoch: store.epoch,
                flights: vec![stale],
            })
            .unwrap();
        store.tick_projectile_displays(&chunks);
        assert!(
            store.vehicles[&0]
                .projectile
                .as_ref()
                .unwrap()
                .next
                .is_none()
        );
        assert!(store.vehicles[&0].projectile.as_ref().unwrap().current.x > previous);
    }

    #[test]
    fn stale_correction_remove_reuse_world_epoch_and_landing_are_authoritative() {
        crate::world::block::init("26.2");
        let chunks = ChunkStore::new(1);
        let mut store = projectile(EntityKind::Arrow, Position::new(2.0, 70.0, 2.0), DVec3::X);
        store.tick_projectile_displays(&chunks);
        let p = store.vehicles[&1].projectile.as_ref().unwrap();
        let stale = projectile::flight(Input {
            id: 1,
            revision: p.revision,
            start: store.tick,
            source: Frame {
                position: Position::new(999.0, 70.0, 2.0),
                velocity: DVec3::X,
            },
            arrow: true,
            no_gravity: false,
            drag: 0.99_f32 as f64,
        });
        let (worker, _jobs, results) = Worker::held();
        store.worker = Some(worker);
        store.set_vehicle_transform(1, Position::new(10.0, 70.0, 2.0), DVec3::X);
        results
            .send(projectile::Result {
                epoch: store.epoch,
                flights: vec![stale.clone()],
            })
            .unwrap();
        store.tick_projectile_displays(&chunks);
        assert_eq!(
            store.vehicles[&1].projectile.as_ref().unwrap().current.x,
            11.0
        );
        store.remove_entity(1);
        store.set_vehicle_spawn_transform(
            1,
            Position::new(20.0, 70.0, 2.0),
            DVec3::X,
            LookDirection::default(),
        );
        store.set_vehicle_kind(1, EntityKind::Arrow);
        let (worker, _jobs, results) = Worker::held();
        store.worker = Some(worker);
        results
            .send(projectile::Result {
                epoch: store.epoch,
                flights: vec![stale.clone()],
            })
            .unwrap();
        store.tick_projectile_displays(&chunks);
        assert_eq!(
            store.vehicles[&1].projectile.as_ref().unwrap().current.x,
            21.0
        );
        store.set_projectile_metadata_at(1, 10, MetaValue::Bool(true), 776);
        store.tick_projectile_displays(&chunks);
        assert!(store.vehicles[&1].projectile.as_ref().unwrap().stopped);
        assert_eq!(
            store.vehicles[&1].projectile.as_ref().unwrap().current.x,
            20.0
        );
        let mut world = projectile(EntityKind::Arrow, Position::new(5.0, 70.0, 2.0), DVec3::X);
        assert_ne!(world.epoch, store.epoch);
        let (worker, _jobs, results) = Worker::held();
        world.worker = Some(worker);
        results
            .send(projectile::Result {
                epoch: store.epoch,
                flights: vec![stale],
            })
            .unwrap();
        world.tick_projectile_displays(&chunks);
        assert_eq!(
            world.vehicles[&1].projectile.as_ref().unwrap().current.x,
            6.0
        );
    }

    #[test]
    fn dimension_info_rejects_held_world_job_and_reuses_worker_for_new_projectiles() {
        crate::world::block::init("26.2");
        let chunks = ChunkStore::new(1);
        let mut store = EntityStore::new();
        let mut positions = HashMap::new();
        for id in 0..16 {
            let pos = Position::new(2.0, 70.0, 2.0);
            store.set_vehicle_spawn_transform(id, pos, DVec3::X, LookDirection::default());
            store.set_vehicle_kind(id, EntityKind::Arrow);
            positions.insert(id, pos);
        }
        // An unrelated vehicle and living entities are not owned by this lifecycle.
        store.set_vehicle_spawn_transform(
            20,
            Position::default(),
            DVec3::ZERO,
            LookDirection::default(),
        );
        store.set_vehicle_kind(20, EntityKind::Bat);
        positions.insert(20, Position::default());
        store.spawn_living(
            21,
            EntityKind::Cow,
            Position::default(),
            LookDirection::default(),
            0.0,
            None,
        );
        let (mut worker, jobs, results) = Worker::held();
        worker.in_flight = false;
        store.worker = Some(worker);
        store.tick_projectile_displays(&chunks);
        let old_job = jobs.try_recv().unwrap();
        assert_eq!(old_job.inputs.len(), 16);
        let old_epoch = old_job.epoch;

        // This is the same cleanup invoked by AppCore's DimensionInfo event.
        crate::app::core::clear_dimension_projectiles(&mut store, &mut positions);
        assert_ne!(store.epoch, old_epoch);
        assert!(store.worker.as_ref().unwrap().in_flight);
        assert!(store.vehicles.values().all(|v| v.projectile.is_none()));
        assert!(!positions.contains_key(&0));
        assert_eq!(positions.len(), 1);
        assert!(store.vehicles.contains_key(&20));
        assert!(store.living.contains_key(&21));
        // Reused ID must not inherit the old-world packet position baseline.
        let new_pos = Position::new(100.0, 70.0, 2.0);
        for id in 0..16 {
            store.set_vehicle_spawn_transform(id, new_pos, DVec3::X, LookDirection::default());
            store.set_vehicle_kind(id, EntityKind::Arrow);
            positions.insert(id, new_pos);
        }
        let mut stale = projectile::flight(
            old_job
                .inputs
                .into_iter()
                .find(|input| input.id == 0)
                .unwrap(),
        );
        stale.revision = store.vehicles[&0].projectile.as_ref().unwrap().revision;
        stale.start = store.tick + 10; // make revision and future-tick checks pass
        results
            .send(projectile::Result {
                epoch: old_epoch,
                flights: vec![stale],
            })
            .unwrap();
        store.tick_projectile_displays(&chunks);
        let display = store.vehicles[&0].projectile.as_ref().unwrap();
        assert!(display.current.x > 100.0);
        assert!(display.frames.is_none() && display.next.is_none());
        let new_job = jobs.try_recv().unwrap();
        assert_eq!(new_job.epoch, store.epoch);
        assert_eq!(new_job.inputs.len(), 16);
        results
            .send(projectile::Result {
                epoch: new_job.epoch,
                flights: new_job.inputs.into_iter().map(projectile::flight).collect(),
            })
            .unwrap();
        store.tick_projectile_displays(&chunks);
        let display = store.vehicles[&0].projectile.as_ref().unwrap();
        assert!(
            display.frames.is_some(),
            "new-world flight renders via the retained worker"
        );
        assert!(display.current.x > 101.0 && display.current.x < 103.0);
    }

    #[test]
    fn threshold_horizon_and_bounded_batch() {
        crate::world::block::init("26.2");
        let chunks = ChunkStore::new(1);
        let mut store = EntityStore::new();
        for id in 0..(MAX_BATCH + 1) as i32 {
            store.set_vehicle_spawn_transform(
                id,
                Position::new(2.0, 70.0, 2.0),
                DVec3::X,
                LookDirection::default(),
            );
            store.set_vehicle_kind(id, EntityKind::Arrow);
        }
        let (mut worker, jobs, _results) = Worker::held();
        worker.in_flight = false;
        store.worker = Some(worker);
        store.tick_projectile_displays(&chunks);
        let job = jobs.try_recv().unwrap();
        assert_eq!(job.inputs.len(), MAX_BATCH);
        assert_eq!(job.epoch, store.epoch);
        for _ in 1..140 {
            store.tick_projectile_displays(&chunks);
        }
        assert!(
            store
                .vehicles
                .values()
                .all(|v| v.projectile.as_ref().unwrap().current.x > 60.0)
        );
        assert!(jobs.try_recv().is_err(), "only one in-flight job");
        let mut fifteen = EntityStore::new();
        for id in 0..15 {
            fifteen.set_vehicle_spawn_transform(
                id,
                Position::new(2.0, 70.0, 2.0),
                DVec3::X,
                LookDirection::default(),
            );
            fifteen.set_vehicle_kind(id, EntityKind::Arrow);
        }
        fifteen.tick_projectile_displays(&chunks);
        assert!(fifteen.worker.is_none());
        fifteen.set_vehicle_spawn_transform(
            15,
            Position::new(2.0, 70.0, 2.0),
            DVec3::X,
            LookDirection::default(),
        );
        fifteen.set_vehicle_kind(15, EntityKind::Arrow);
        fifteen.tick_projectile_displays(&chunks);
        assert!(
            (fifteen.vehicles[&0].projectile.as_ref().unwrap().current.x - (3.0 + 0.99_f32 as f64))
                .abs()
                < 1e-12
        );
    }

    #[test]
    fn early_refill_keeps_absolute_ticks_across_horizon() {
        crate::world::block::init("26.2");
        let chunks = ChunkStore::new(1);
        let mut store = EntityStore::new();
        for id in 0..16 {
            store.set_vehicle_spawn_transform(
                id,
                Position::new(2.0, 70.0, 2.0),
                DVec3::X,
                LookDirection::default(),
            );
            store.set_vehicle_kind(id, EntityKind::Arrow);
        }
        let (mut worker, jobs, results) = Worker::held();
        worker.in_flight = false;
        store.worker = Some(worker);
        store.tick_projectile_displays(&chunks);
        let initial = jobs.try_recv().unwrap();
        results
            .send(projectile::Result {
                epoch: store.epoch,
                flights: initial.inputs.into_iter().map(projectile::flight).collect(),
            })
            .unwrap();
        for _ in 2..=49 {
            store.tick_projectile_displays(&chunks);
        }
        let refill = jobs.try_recv().unwrap();
        assert_eq!(refill.inputs.len(), 16);
        assert!(refill.inputs.iter().all(|f| f.start == 65));
        results
            .send(projectile::Result {
                epoch: store.epoch,
                flights: refill.inputs.into_iter().map(projectile::flight).collect(),
            })
            .unwrap();
        store.tick_projectile_displays(&chunks);
        assert!(
            store.vehicles[&0]
                .projectile
                .as_ref()
                .unwrap()
                .next
                .is_some()
        );
        let mut previous = store.vehicles[&0]
            .projectile
            .as_ref()
            .unwrap()
            .position(0.9)
            .x;
        for _ in 51..=130 {
            store.tick_projectile_displays(&chunks);
            let p = store.vehicles[&0].projectile.as_ref().unwrap();
            assert!(
                p.position(0.1).x > previous,
                "backwards at tick {}",
                store.tick
            );
            previous = p.position(0.9).x;
        }
    }

    #[test]
    fn water_sample_is_display_only_and_crossing_geometry_never_hides_arrow() {
        crate::world::block::init("26.2");
        let mut chunks = ChunkStore::new(1);
        chunks.load_decoded_chunk(ChunkPos::new(0, 0), azalea_world::chunk::Chunk::default());
        chunks.set_block_state(
            2,
            70,
            2,
            crate::world::block::first_state_of("water").unwrap(),
        );
        chunks.set_block_state(
            3,
            70,
            2,
            crate::world::block::first_state_of("stone").unwrap(),
        );
        let mut arrow = projectile(
            EntityKind::SpectralArrow,
            Position::new(2.0, 70.5, 2.5),
            DVec3::X,
        );
        arrow.tick_projectile_displays(&chunks);
        arrow.tick_projectile_displays(&chunks);
        assert_eq!(
            arrow.vehicles[&1].projectile.as_ref().unwrap().current.x,
            3.0 + 0.6_f32 as f64
        );
        assert!(!arrow.vehicles[&1].projectile.as_ref().unwrap().stopped);
        assert_eq!(arrow.vehicles[&1].position.x, 2.0);
        let mut snow = projectile(
            EntityKind::Snowball,
            Position::new(2.0, 70.5, 2.5),
            DVec3::X,
        );
        snow.tick_projectile_displays(&chunks);
        assert!(
            (snow.vehicles[&1].projectile.as_ref().unwrap().velocity.x - 0.8_f32 as f64).abs()
                < 1e-12
        );
        assert!(snow.remove_impacted_snowball(1));
        assert!(!snow.vehicles.contains_key(&1));
        arrow.set_projectile_grounded(1, true);
        arrow.tick_projectile_displays(&chunks);
        assert_eq!(
            arrow.vehicles[&1].projectile.as_ref().unwrap().current.x,
            2.0
        );
    }

    #[test]
    fn grounded_arrow_metadata_survives_zero_delta_correction_in_both_protocols() {
        for protocol in [775, 776] {
            let mut store = projectile(EntityKind::Arrow, Position::new(2.0, 70.0, 2.0), DVec3::X);
            store.set_projectile_metadata_at(1, 10, MetaValue::Bool(true), protocol);
            let pos = store.vehicles[&1].position;
            store.set_vehicle_transform(1, pos, DVec3::X); // zero-delta PosRot
            store.set_projectile_grounded(1, false);
            assert!(store.vehicles[&1].projectile.as_ref().unwrap().stopped);
            store.set_vehicle_motion(1, DVec3::ZERO);
            assert!(store.vehicles[&1].projectile.as_ref().unwrap().stopped);
            store.set_projectile_metadata_at(1, 10, MetaValue::Bool(false), protocol);
            assert!(!store.vehicles[&1].projectile.as_ref().unwrap().stopped);
        }
        let mut released = projectile(EntityKind::Arrow, Position::default(), DVec3::X);
        released.set_projectile_metadata_at(1, 10, MetaValue::Bool(true), 776);
        released.set_vehicle_motion(1, DVec3::X);
        assert!(!released.vehicles[&1].projectile.as_ref().unwrap().in_ground);
        assert!(!released.vehicles[&1].projectile.as_ref().unwrap().stopped);
        let mut older = projectile(EntityKind::Arrow, Position::default(), DVec3::X);
        older.set_projectile_metadata_at(1, 10, MetaValue::Bool(true), 774);
        assert!(!older.vehicles[&1].projectile.as_ref().unwrap().in_ground);
    }

    #[test]
    fn snowball_gravity_and_drag_precede_movement_and_motion_does_not_rewind() {
        crate::world::block::init("26.2");
        let mut chunks = ChunkStore::new(1);
        let _chunk = chunks
            .chunk_storage
            .upsert(ChunkPos::new(0, 0), azalea_world::chunk::Chunk::default());
        let mut store = projectile(
            EntityKind::Snowball,
            Position::new(2.0, 70.0, 2.0),
            DVec3::X,
        );
        store.tick_projectile_displays(&chunks);
        let p = store.vehicles[&1].projectile.as_ref().unwrap();
        assert!((p.current.x - 2.9900000095367432).abs() < 1e-12);
        assert!((p.current.y - 69.9702999997139).abs() < 1e-12);
        assert!((p.velocity.y - (-0.029700000286102295)).abs() < 1e-12);
        for _ in 1..5 {
            store.tick_projectile_displays(&chunks);
        }
        let before = store.vehicles[&1].projectile.as_ref().unwrap().current;
        assert_eq!(store.vehicles[&1].position, Position::new(2.0, 70.0, 2.0));
        store.set_vehicle_motion(1, DVec3::new(0.0, 1.0, 0.0));
        assert_eq!(
            store.vehicles[&1].projectile.as_ref().unwrap().current,
            before
        );
        assert_eq!(store.vehicles[&1].position, Position::new(2.0, 70.0, 2.0));
        // Fresh flight for the ten-tick vanilla sequence (not a lifespan).
        let mut store = projectile(
            EntityKind::Snowball,
            Position::new(2.0, 70.0, 2.0),
            DVec3::X,
        );
        for _ in 0..10 {
            store.tick_projectile_displays(&chunks);
        }
        let p = store.vehicles[&1].projectile.as_ref().unwrap();
        assert!((p.current.x - 11.466175068104723).abs() < 1e-10);
        assert!((p.current.y - 68.41453842498419).abs() < 1e-10);
        assert!((p.velocity.y - (-0.28398525204314173)).abs() < 1e-10);
        store.tick_projectile_displays(&chunks);
        let p = store.vehicles[&1].projectile.as_ref().unwrap();
        assert!((p.current.x - (11.466175068104723 + p.velocity.x)).abs() < 1e-10);
        assert_eq!(store.vehicles[&1].position, Position::new(2.0, 70.0, 2.0));
    }

    #[test]
    fn mannequin_profile_customization_and_pose_remain_on_nonliving_route() {
        let mut store = projectile(EntityKind::Mannequin, Position::default(), DVec3::ZERO);
        assert!(!is_living_mob(&EntityKind::Mannequin));
        store.set_mannequin_profile(1, azalea_inventory::components::Profile::default());
        store.set_mannequin_customization(1, 0xff);
        store.set_main_arm(1, false);
        store.set_pose(1, EntityPose::Crouching);
        let mannequin = &store.vehicles[&1];
        assert!(mannequin.mannequin_profile.is_some());
        assert_eq!(mannequin.mannequin_skin_parts_mask, 0x7f);
        assert!(!mannequin.mannequin_main_arm_right);
        assert_eq!(mannequin.mannequin_pose, EntityPose::Crouching);
        assert!(!store.living.contains_key(&1));
    }

    #[test]
    fn player_main_arm_metadata_is_stored_and_non_players_ignore_it() {
        let mut store = EntityStore::new();
        for (id, kind) in [(1, EntityKind::Player), (2, EntityKind::Zombie)] {
            store.spawn_living(
                id,
                kind,
                Position::default(),
                LookDirection::default(),
                0.0,
                None,
            );
        }
        assert!(store.living[&1].main_arm_right);
        store.set_main_arm(1, false);
        store.set_main_arm(2, false);
        assert!(!store.living[&1].main_arm_right);
        assert!(store.living[&2].main_arm_right);
    }

    #[test]
    fn player_skin_parts_metadata_is_kept_separate_and_masks_to_seven_bits() {
        let mut store = EntityStore::new();
        store.spawn_living(
            1,
            EntityKind::Player,
            Position::default(),
            LookDirection::default(),
            0.0,
            None,
        );
        store.spawn_living(
            2,
            EntityKind::Mannequin,
            Position::default(),
            LookDirection::default(),
            0.0,
            None,
        );
        store.apply_entity_data(2, 16, MetaValue::Byte(2));
        assert_eq!(store.living[&2].skin_parts_mask, Some(2));
        assert_eq!(store.living[&1].skin_parts_mask, None);
        for mask in 0..=u8::MAX {
            store.apply_entity_data(1, 16, MetaValue::Byte(mask));
            assert_eq!(store.living[&1].skin_parts_mask, Some(mask & 0x7f));
        }
        store.apply_entity_data(1, 16, MetaValue::Bool(true));
        assert_eq!(store.living[&1].skin_parts_mask, Some(127));
    }

    #[test]
    fn remaining_living_species_metadata_uses_26_2_indices() {
        let mut store = EntityStore::new();
        for (id, kind) in [
            EntityKind::Bee,
            EntityKind::Ghast,
            EntityKind::MagmaCube,
            EntityKind::Mooshroom,
            EntityKind::Phantom,
            EntityKind::Shulker,
            EntityKind::SulfurCube,
            EntityKind::Vex,
            EntityKind::Wither,
        ]
        .into_iter()
        .enumerate()
        {
            store.spawn_living(
                id as i32,
                kind,
                Position::default(),
                LookDirection::default(),
                0.0,
                None,
            );
        }
        store.apply_entity_data(0, 18, MetaValue::Byte(0x08));
        store.apply_entity_data(0, 19, MetaValue::Long(120));
        store.apply_entity_data(1, 16, MetaValue::Bool(true));
        store.apply_entity_data(2, 18, MetaValue::Int(4));
        store.apply_entity_data(3, 18, MetaValue::Int(1));
        store.apply_entity_data(4, 16, MetaValue::Int(3));
        store.apply_entity_data(
            5,
            16,
            MetaValue::Direction(azalea_core::direction::Direction::East),
        );
        store.apply_entity_data(5, 16, MetaValue::Bool(true)); // wrong serializer is ignored
        store.apply_entity_data(5, 17, MetaValue::Byte(127));
        store.apply_entity_data(5, 18, MetaValue::Byte(16));
        store.apply_entity_data(6, 18, MetaValue::Int(2));
        store.apply_entity_data(7, 16, MetaValue::Byte(1));
        store.apply_entity_data(8, 19, MetaValue::Int(40));

        assert_eq!(store.living[&0].bee_flags, 0x08);
        assert_eq!(store.living[&0].anger_end_time, 120);
        assert!(store.living[&1].ghast_charging);
        assert_eq!(store.living[&2].slime_size, 4);
        assert_eq!(store.living[&3].variant, 1);
        assert_eq!(store.living[&4].phantom_size, 3);
        assert_eq!(store.living[&5].shulker_peek, 127);
        assert_eq!(
            store.living[&5].shulker_attach_face,
            azalea_core::direction::Direction::East
        );
        assert_eq!(store.living[&5].variant, 16);
        assert_eq!(store.living[&6].sulfur_cube_size, 2);
        assert!(store.living[&7].vex_charging);
        assert_eq!(store.living[&8].wither_invulnerability, 40);
    }

    #[test]
    fn shulker_direction_is_typed_and_peek_interpolates_in_source_rate() {
        use azalea_core::direction::Direction as D;

        let mut store = EntityStore::new();
        store.spawn_living(
            1,
            EntityKind::Shulker,
            Position::default(),
            LookDirection::default(),
            0.0,
            None,
        );
        for face in [D::Down, D::Up, D::North, D::South, D::West, D::East] {
            store.apply_entity_data(1, 16, MetaValue::Direction(face));
            assert_eq!(store.living[&1].shulker_attach_face, face);
        }
        store.apply_entity_data(1, 16, MetaValue::Bool(true));
        assert_eq!(store.living[&1].shulker_attach_face, D::East);
        store.apply_entity_data(1, 17, MetaValue::Byte(50));
        for _ in 0..5 {
            store.tick_living(&ChunkStore::new(2), Position::default(), 10);
        }
        let shulker = &store.living[&1];
        assert!((shulker.prev_shulker_peek_amount - 0.2).abs() < 1e-6);
        assert!((shulker.shulker_peek_amount - 0.25).abs() < 1e-6);
        let interpolated = shulker.prev_shulker_peek_amount
            + (shulker.shulker_peek_amount - shulker.prev_shulker_peek_amount) * 0.5;
        assert!((interpolated - 0.225).abs() < 1e-6);
    }

    #[test]
    fn horse_inventory_family_is_scoped_and_stored_as_living() {
        let horse_family = [
            EntityKind::Horse,
            EntityKind::Donkey,
            EntityKind::Mule,
            EntityKind::SkeletonHorse,
            EntityKind::ZombieHorse,
            EntityKind::Llama,
            EntityKind::TraderLlama,
            EntityKind::Camel,
            EntityKind::CamelHusk,
        ];
        let mut store = EntityStore::new();
        for (id, kind) in horse_family.into_iter().enumerate() {
            assert!(supports_horse_inventory(&kind), "{kind:?}");
            if is_living_mob(&kind) {
                store.spawn_living(
                    id as i32,
                    kind,
                    Position::default(),
                    LookDirection::default(),
                    0.0,
                    None,
                );
            }
        }
        assert!(store.living.contains_key(&5), "Llama enters living store");
        assert!(
            store.living.contains_key(&6),
            "TraderLlama enters living store"
        );
        assert!(store.living.contains_key(&7), "Camel enters living store");
        assert!(
            store.living.contains_key(&8),
            "CamelHusk enters living store"
        );

        for kind in [EntityKind::Pig, EntityKind::Nautilus] {
            assert!(!supports_horse_inventory(&kind), "{kind:?}");
        }
        assert!(is_living_mob(&EntityKind::Nautilus));

        // Inventory acceptance must not expand shared equine animation/metadata
        // or the explicit equine riding-jump predicate.
        for kind in [
            EntityKind::Llama,
            EntityKind::TraderLlama,
            EntityKind::Camel,
            EntityKind::CamelHusk,
        ] {
            assert!(!is_equine(&kind), "{kind:?} remains outside is_equine");
        }
    }

    #[test]
    fn missing_living_mobs_are_classified_away_from_vehicle_storage() {
        let missing_mobs = [
            EntityKind::Allay,
            EntityKind::Armadillo,
            EntityKind::Axolotl,
            EntityKind::Bee,
            EntityKind::Blaze,
            EntityKind::Breeze,
            EntityKind::Camel,
            EntityKind::CamelHusk,
            EntityKind::CaveSpider,
            EntityKind::CopperGolem,
            EntityKind::Creaking,
            EntityKind::Dolphin,
            EntityKind::ElderGuardian,
            EntityKind::Endermite,
            EntityKind::Evoker,
            EntityKind::Fox,
            EntityKind::Frog,
            EntityKind::Ghast,
            EntityKind::Giant,
            EntityKind::Goat,
            EntityKind::Guardian,
            EntityKind::HappyGhast,
            EntityKind::Hoglin,
            EntityKind::Illusioner,
            EntityKind::Llama,
            EntityKind::MagmaCube,
            EntityKind::Mooshroom,
            EntityKind::Nautilus,
            EntityKind::Panda,
            EntityKind::Parched,
            EntityKind::Parrot,
            EntityKind::Phantom,
            EntityKind::Piglin,
            EntityKind::PiglinBrute,
            EntityKind::Pillager,
            EntityKind::PolarBear,
            EntityKind::Ravager,
            EntityKind::Shulker,
            EntityKind::Silverfish,
            EntityKind::Sniffer,
            EntityKind::SnowGolem,
            EntityKind::Strider,
            EntityKind::SulfurCube,
            EntityKind::Tadpole,
            EntityKind::TraderLlama,
            EntityKind::Turtle,
            EntityKind::Vex,
            EntityKind::Vindicator,
            EntityKind::WanderingTrader,
            EntityKind::Warden,
            EntityKind::Wither,
            EntityKind::WitherSkeleton,
            EntityKind::Zoglin,
            EntityKind::ZombieNautilus,
            EntityKind::ZombifiedPiglin,
        ];
        let mut store = EntityStore::new();
        for (id, kind) in missing_mobs.into_iter().enumerate() {
            assert!(is_living_mob(&kind), "{kind:?} must reach living storage");
            store.spawn_living(
                id as i32,
                kind,
                Position::default(),
                LookDirection::default(),
                0.0,
                None,
            );
            assert!(store.living.contains_key(&(id as i32)));
            assert!(!store.vehicles.contains_key(&(id as i32)));
        }
        for non_mob in [
            EntityKind::OakBoat,
            EntityKind::Minecart,
            EntityKind::Arrow,
            EntityKind::Snowball,
            EntityKind::ArmorStand,
            EntityKind::TextDisplay,
            EntityKind::ItemDisplay,
            EntityKind::BlockDisplay,
            EntityKind::Item,
            EntityKind::Mannequin,
        ] {
            assert!(
                !is_living_mob(&non_mob),
                "{non_mob:?} stays on its own route"
            );
        }
    }

    #[test]
    fn same_uuid_stone_shared_invisibility_flag_gates_its_shadow_input() {
        let uuid = uuid::Uuid::parse_str("00000000-0000-4000-8000-000000000001").unwrap();
        let mut store = ItemEntityStore::new();
        store.spawn_item(1, uuid, Position::new(0.5, 64.0, 3.5), DVec3::ZERO);
        store.set_item_data(1, "minecraft:stone".into(), 1, 0, 1, None);

        store.set_shared_flags(1, 0x20);
        let item = store.visible_items(DVec3::new(0.5, 65.0, 1.5), 64.0)[0];
        assert_eq!(item.uuid, uuid);
        assert_eq!(item.item_name, "minecraft:stone");
        assert!(item.invisible, "metadata bit 0x20 must suppress its shadow");

        store.set_shared_flags(1, 0);
        assert!(!store.visible_items(DVec3::new(0.5, 65.0, 1.5), 64.0)[0].invisible);
    }

    #[test]
    fn shared_flags_and_living_use_bits_are_retained() {
        let mut store = EntityStore::new();
        store.spawn_living(
            7,
            EntityKind::Zombie,
            Position::default(),
            LookDirection::default(),
            0.0,
            None,
        );
        store.apply_entity_data(7, 0, MetaValue::Byte(0xF9));
        store.apply_entity_data(7, 8, MetaValue::Byte(0x07));
        let entity = &store.living[&7];
        assert_eq!(
            entity.flags,
            EntityFlags {
                on_fire: true,
                sprinting: true,
                swimming: true,
                invisible: true,
                glowing: true,
                fall_flying: true
            }
        );
        assert!(entity.using_item && entity.using_offhand && entity.riptide_spin);
        assert!(entity.is_sprinting);
    }

    #[test]
    fn pose_ids_match_vanilla_26_2_and_unknown_ids_default_to_standing() {
        assert_eq!(EntityPose::from_vanilla_id(0), EntityPose::Standing);
        assert_eq!(EntityPose::from_vanilla_id(3), EntityPose::Swimming);
        assert_eq!(EntityPose::from_vanilla_id(15), EntityPose::Sliding);
        assert_eq!(EntityPose::from_vanilla_id(17), EntityPose::Inhaling);
        assert_eq!(EntityPose::from_vanilla_id(18), EntityPose::Standing);
        assert_eq!(EntityPose::from_vanilla_id(-1), EntityPose::Standing);
    }

    #[test]
    fn pose_dimensions_only_override_sleeping_in_vanilla_common_entity_logic() {
        let base = EntityDimensions {
            width: 0.6,
            height: 1.8,
        };
        assert_eq!(
            EntityStore::dimensions_for_pose(base, EntityPose::Swimming),
            base
        );
        assert_eq!(
            EntityStore::dimensions_for_pose(base, EntityPose::Crouching),
            base
        );
        assert_eq!(
            EntityStore::dimensions_for_pose(base, EntityPose::Sleeping),
            EntityDimensions {
                width: 0.2,
                height: 0.2
            }
        );
    }

    #[test]
    fn remove_entity_detaches_edges_keeps_subtree_and_allows_id_reuse() {
        let mut s = EntityStore::new();
        s.set_passengers(99, &[]);
        assert_eq!(s.vehicles[&99].kind, None);
        assert_eq!(s.vehicles[&99].look_dir, None);
        s.set_vehicle_rotation(99, LookDirection::new(1.0, 2.0));
        assert_eq!(s.vehicles[&99].look_dir, None);
        s.set_vehicle_spawn_transform(
            10,
            Position::default(),
            DVec3::ZERO,
            LookDirection::new(30.0, 5.0),
        );
        s.set_passengers(10, &[20, 21]);
        s.set_vehicle_transform(20, Position::default(), DVec3::ZERO);
        s.set_vehicle_rotation(20, LookDirection::new(45.0, 10.0));
        s.set_passengers(20, &[30]);
        s.remove_entity(10);
        assert!(!s.vehicles.contains_key(&10));
        assert!(!s.vehicle_of.contains_key(&20));
        assert_eq!(s.vehicles[&20].passengers, [30]);
        assert_eq!(s.vehicle_of[&30], 20);
        assert_eq!(s.root_vehicle(30), Some(20));
        s.remove_entity(30);
        assert!(s.vehicles[&20].passengers.is_empty());
        assert!(!s.vehicle_of.contains_key(&30));
        s.set_vehicle_spawn_transform(
            10,
            Position::new(7.0, 8.0, 9.0),
            DVec3::ZERO,
            LookDirection::new(90.0, 20.0),
        );
        s.set_passengers(10, &[40]);
        assert_eq!(s.root_vehicle(40), Some(10));
        assert_eq!(
            s.vehicles[&10].look_dir,
            Some(LookDirection::new(90.0, 20.0))
        );
    }

    #[test]
    fn end_crystal_metadata_defaults_and_indices_are_kind_specific() {
        let mut store = EntityStore::new();
        store.set_passengers(1, &[]);
        store.set_vehicle_kind(1, EntityKind::EndCrystal);
        assert!(store.vehicles[&1].crystal_beam_target.is_none());
        assert!(store.vehicles[&1].crystal_show_bottom);
        let pos = BlockPos::new(2, 3, 4);
        store.apply_vehicle_metadata(1, 8, MetaValue::OptionalBlockPos(Some(pos)));
        store.apply_vehicle_metadata(1, 9, MetaValue::Bool(false));
        assert_eq!(store.vehicles[&1].crystal_beam_target, Some(pos));
        assert!(!store.vehicles[&1].crystal_show_bottom);
        store.set_vehicle_kind(2, EntityKind::Arrow);
        store.apply_vehicle_metadata(2, 8, MetaValue::OptionalBlockPos(Some(pos)));
        assert!(store.vehicles[&2].crystal_beam_target.is_none());
    }

    #[test]
    fn armor_stand_flags_pose_equipment_and_cleanup_stay_nonliving() {
        let mut store = projectile(EntityKind::ArmorStand, Position::default(), DVec3::ZERO);
        let stand = &store.vehicles[&1];
        assert_eq!(stand.armor_stand_flags, 0);
        assert_eq!(stand.armor_stand_pose[2], [-10.0, 0.0, -10.0]);
        assert_eq!(stand.armor_stand_pose[3], [-15.0, 0.0, 10.0]);
        store.apply_vehicle_metadata(1, 15, MetaValue::Byte(0x1d));
        store.set_armor_stand_rotation(1, 16, [350.0, f32::NAN, -370.0]);
        assert_eq!(store.vehicles[&1].armor_stand_flags, 0x1d);
        assert_eq!(store.vehicles[&1].armor_stand_pose[0], [350.0, 0.0, -10.0]);
        store.set_armor_stand_rotation(1, 21, [1.0, 2.0, 3.0]);
        assert_eq!(store.vehicles[&1].armor_stand_pose[5], [1.0, 2.0, 3.0]);
        use azalea_inventory::components::EquipmentSlot as Slot;
        let slots = [
            Slot::Mainhand,
            Slot::Offhand,
            Slot::Feet,
            Slot::Legs,
            Slot::Chest,
            Slot::Head,
        ]
        .into_iter()
        .map(|slot| (slot, azalea_inventory::ItemStack::Empty))
        .collect::<Vec<_>>();
        let helmet = azalea_inventory::ItemStack::Present(azalea_inventory::ItemStackData::new(
            azalea_registry::builtin::ItemKind::DiamondHelmet,
            1,
        ));
        let mut slots = slots;
        slots[0].1 = helmet.clone();
        store.set_armor_stand_equipment(1, slots);
        assert_eq!(store.vehicles[&1].armor_stand_equipment.len(), 1);
        assert_eq!(
            store.vehicles[&1].armor_stand_equipment
                [&azalea_inventory::components::EquipmentSlot::Mainhand],
            helmet
        );
        use azalea_inventory::components::EquipmentSlot::{Body, Saddle};
        let harness = azalea_inventory::ItemStack::Present(azalea_inventory::ItemStackData::new(
            azalea_registry::builtin::ItemKind::DiamondHelmet,
            1,
        ));
        store
            .set_armor_stand_equipment(1, vec![(Body, harness.clone()), (Saddle, harness.clone())]);
        assert_eq!(store.vehicles[&1].armor_stand_equipment[&Body], harness);
        store.set_armor_stand_equipment(1, vec![(Body, azalea_inventory::ItemStack::Empty)]);
        assert!(!store.vehicles[&1].armor_stand_equipment.contains_key(&Body));
        store.remove_entity(1);
        assert!(!store.vehicles.contains_key(&1));
        assert!(store.living.is_empty());
    }

    #[test]
    fn living_equipment_updates_keep_native_body_and_saddle_slots_and_clear_tombstones() {
        use azalea_inventory::components::EquipmentSlot as Slot;
        let mut store = EntityStore::new();
        store.spawn_living(
            1,
            EntityKind::HappyGhast,
            Position::default(),
            LookDirection::default(),
            0.0,
            None,
        );
        let stack = azalea_inventory::ItemStack::Present(azalea_inventory::ItemStackData::new(
            azalea_registry::builtin::ItemKind::DiamondHelmet,
            1,
        ));
        let slots = [
            Slot::Mainhand,
            Slot::Offhand,
            Slot::Feet,
            Slot::Legs,
            Slot::Chest,
            Slot::Head,
            Slot::Body,
            Slot::Saddle,
        ];
        store.set_armor_stand_equipment(
            1,
            slots
                .into_iter()
                .map(|slot| (slot, azalea_inventory::ItemStack::Empty))
                .collect(),
        );
        store.set_armor_stand_equipment(
            1,
            vec![(Slot::Body, stack.clone()), (Slot::Saddle, stack.clone())],
        );
        assert_eq!(store.living[&1].equipment.len(), 2);
        assert!(store.living[&1].saddled);
        store.set_armor_stand_equipment(
            1,
            vec![
                (Slot::Body, azalea_inventory::ItemStack::Empty),
                (Slot::Saddle, azalea_inventory::ItemStack::Empty),
            ],
        );
        assert!(store.living[&1].equipment.is_empty());
        assert!(!store.living[&1].saddled);
    }

    #[test]
    fn experience_orb_value_and_visual_age_are_nonliving_and_kind_specific() {
        let mut orb = projectile(EntityKind::ExperienceOrb, Position::default(), DVec3::ZERO);
        assert_eq!(orb.vehicles[&1].experience_orb_value, 0);
        assert_eq!(orb.vehicles[&1].experience_orb_age, 0);
        orb.apply_vehicle_metadata(1, 8, MetaValue::Int(149));
        orb.apply_vehicle_metadata(1, 9, MetaValue::Int(42));
        assert_eq!(orb.vehicles[&1].experience_orb_value, 149);
        assert_eq!(orb.vehicles[&1].experience_orb_age, 0);
        orb.tick_projectile_displays(&ChunkStore::new(2));
        assert_eq!(orb.vehicles[&1].experience_orb_age, 1);
        let mut other = projectile(EntityKind::Arrow, Position::default(), DVec3::ZERO);
        other.apply_vehicle_metadata(1, 8, MetaValue::Int(149));
        assert_eq!(other.vehicles[&1].experience_orb_value, 0);
    }

    #[test]
    fn block_display_metadata_is_typed_and_separate_from_text_content() {
        let mut store = EntityStore::new();
        store.set_passengers(42, &[]);
        store.set_vehicle_kind(42, EntityKind::BlockDisplay);
        store.set_display_metadata(
            42,
            11,
            crate::net::DisplayMetaValue::Vector([1.0, 2.0, 3.0]),
        );
        store.set_display_metadata(
            42,
            12,
            crate::net::DisplayMetaValue::Vector([-1.0, 2.0, 3.0]),
        );
        store.set_display_metadata(
            42,
            13,
            crate::net::DisplayMetaValue::Quaternion([0.0, 0.0, 0.0, 1.0]),
        );
        store.set_display_metadata(42, 23, crate::net::DisplayMetaValue::BlockState(1));
        assert_eq!(store.vehicles[&42].display.translation, [1.0, 2.0, 3.0]);
        assert_eq!(store.vehicles[&42].display.scale, [-1.0, 2.0, 3.0]);
        assert_eq!(store.vehicles[&42].display.block_state, Some(1));
        assert!(store.vehicles[&42].text_display_text.is_none());
        store.set_vehicle_kind(43, EntityKind::TextDisplay);
        store.set_display_metadata(43, 23, crate::net::DisplayMetaValue::BlockState(2));
        assert_eq!(store.vehicles[&43].display.block_state, None);
    }

    #[test]
    fn item_display_stack_and_native_context_are_typed_and_separate() {
        let mut store = EntityStore::new();
        store.set_passengers(1, &[]);
        store.set_vehicle_kind(1, EntityKind::ItemDisplay);
        for (id, expected) in [
            (0, 0),
            (1, 1),
            (2, 2),
            (3, 3),
            (4, 4),
            (5, 5),
            (6, 6),
            (7, 7),
            (8, 8),
        ] {
            store.set_item_display_metadata(1, 24, crate::net::ItemDisplayMetaValue::Context(id));
            assert_eq!(store.vehicles[&1].item_display_context, expected);
        }
        store.set_item_display_metadata(
            1,
            23,
            crate::net::ItemDisplayMetaValue::Stack(azalea_inventory::ItemStack::Empty),
        );
        assert!(matches!(
            store.vehicles[&1].item_display_stack,
            azalea_inventory::ItemStack::Empty
        ));
        store.set_vehicle_kind(2, EntityKind::BlockDisplay);
        store.set_item_display_metadata(2, 24, crate::net::ItemDisplayMetaValue::Context(8));
        assert_eq!(store.vehicles[&2].item_display_context, 0);
    }

    #[test]
    fn text_display_transform_metadata_is_partial_and_preserves_text() {
        let mut store = EntityStore::new();
        store.set_passengers(1, &[]);
        store.set_vehicle_kind(1, EntityKind::TextDisplay);
        store.set_text_display_transform(
            1,
            11,
            crate::net::TextDisplayTransformValue::Vector([2.0, 3.0, 4.0]),
        );
        store.set_text_display_transform(
            1,
            12,
            crate::net::TextDisplayTransformValue::Vector([5.0, 6.0, 7.0]),
        );
        store.set_text_display_transform(
            1,
            13,
            crate::net::TextDisplayTransformValue::Quaternion([1.0, 2.0, 3.0, 4.0]),
        );
        store.set_text_display_transform(
            1,
            14,
            crate::net::TextDisplayTransformValue::Quaternion([5.0, 6.0, 7.0, 8.0]),
        );
        store.set_text_display_transform(
            1,
            15,
            crate::net::TextDisplayTransformValue::Billboard(255),
        );
        store.set_text_display_text(1, Vec::new());
        let state = &store.vehicles[&1];
        assert_eq!(state.text_display_translation, [2.0, 3.0, 4.0]);
        assert_eq!(state.text_display_scale, [5.0, 6.0, 7.0]);
        assert_eq!(state.text_display_left_rotation, [1.0, 2.0, 3.0, 4.0]);
        assert_eq!(state.text_display_right_rotation, [5.0, 6.0, 7.0, 8.0]);
        assert_eq!(state.text_display_billboard, 255);
        assert_eq!(state.text_display_text, Some(Vec::new()));
        assert_eq!(state.text_display_background, 0x4000_0000);
    }

    #[test]
    fn vehicle_order_root_authority_and_position_contract() {
        let mut store = EntityStore::new();
        store.set_vehicle_transform(10, Position::new(4.0, 5.0, 6.0), DVec3::new(1.0, 0.0, 0.0));
        store.set_passengers(10, &[20, 21]);
        store.set_passengers(20, &[30]);
        assert_eq!(store.root_vehicle(30), Some(10));
        assert_eq!(store.ride_authority(20, true), RideAuthority::Client);
        assert_eq!(store.ride_authority(21, true), RideAuthority::Server);
        assert_eq!(store.ride_authority(20, false), RideAuthority::Server);
        assert_eq!(
            store.passenger_position(10, DVec3::Y, DVec3::ZERO),
            Some(Position::new(4.0, 6.0, 6.0))
        );
        store.set_passengers(11, &[20]);
        assert!(!store.vehicles[&10].passengers.contains(&20));
        assert_eq!(store.root_vehicle(20), Some(11));
    }

    #[test]
    fn tick_living_advances_remote_interpolation_state() {
        let mut store = EntityStore::new();
        store.spawn_living(
            1,
            EntityKind::Zombie,
            Position::new(0.0, 64.0, 0.0),
            LookDirection::default(),
            0.0,
            None,
        );
        store.move_living_delta(1, 3.0, 0.0, 0.0, true);
        let before = store.living[&1].position;

        store.tick_living(&ChunkStore::new(2), Position::default(), 10);

        let entity = &store.living[&1];
        assert_eq!(
            entity.prev_position, before,
            "each world tick must advance the remote entity interpolation endpoint"
        );
        assert_ne!(
            entity.position, before,
            "a pending remote movement interpolation must keep progressing"
        );
        assert_eq!(
            entity.age_in_ticks, 1,
            "remote living entities must keep receiving client ticks"
        );
    }

    #[test]
    fn stop_walk_animation_matches_vanilla_state_reset() {
        let mut position = 12.5;
        let mut speed = 0.7;
        let mut speed_old = 0.4;

        stop_walk_animation(&mut position, &mut speed, &mut speed_old);

        assert_eq!(
            position, 0.0,
            "vanilla stop() clears walk animation position"
        );
        assert_eq!(speed, 0.0, "vanilla stop() clears current walk speed");
        assert_eq!(speed_old, 0.0, "vanilla stop() clears previous walk speed");
    }

    #[test]
    fn death_clock_matches_health_and_simulation_distance_boundaries() {
        let mut death_time = 7;
        tick_death_time(0.01, true, &mut death_time);
        assert_eq!(
            death_time, 7,
            "vanilla does not rewind deathTime merely because health is positive"
        );

        tick_death_time(0.0, false, &mut death_time);
        assert_eq!(
            death_time, 7,
            "dead entities outside simulation distance must not advance deathTime"
        );
        tick_death_time(0.0, true, &mut death_time);
        assert_eq!(
            death_time, 8,
            "zero health in simulation range advances deathTime"
        );
        tick_death_time(-1.0, true, &mut death_time);
        assert_eq!(
            death_time, 9,
            "non-positive health in simulation range keeps advancing deathTime"
        );
    }

    #[test]
    fn death_simulation_distance_uses_chunk_chessboard_distance() {
        let player = Position::new(15.9, 64.0, -0.1);
        assert!(within_simulation_distance(
            Position::new(16.0 * 10.0, 64.0, -16.0 * 10.0),
            player,
            10,
        ));
        assert!(
            !within_simulation_distance(Position::new(16.0 * 11.0, 64.0, 0.0), player, 10,),
            "chunk 11 must be outside a simulation distance of 10"
        );
    }

    #[test]
    fn death_event_forces_mobs_but_not_players_to_zero_health() {
        let mut store = EntityStore::new();
        store.spawn_living(
            1,
            EntityKind::Zombie,
            Position::default(),
            LookDirection::default(),
            0.0,
            None,
        );
        store.spawn_living(
            2,
            EntityKind::Player,
            Position::default(),
            LookDirection::default(),
            0.0,
            None,
        );

        store.living.get_mut(&1).unwrap().death_time = 6;
        store.living.get_mut(&1).unwrap().is_crouching = true;
        store.mark_dead(1);
        store.mark_dead(2);

        assert_eq!(
            store.living[&1].health, 0.0,
            "vanilla event 3 kills non-player living entities client-side"
        );
        assert_eq!(
            store.living[&1].death_time, 6,
            "event 3 must not restart an already-running vanilla death clock"
        );
        assert!(
            !store.living[&1].is_crouching,
            "non-player event 3 transitions the entity to the DYING pose"
        );
        assert_eq!(
            store.living[&2].health, 20.0,
            "vanilla event 3 does not set player health client-side"
        );
    }

    #[test]
    fn player_index_normalization() {
        let at = |index, protocol| normalize_player_index_at(EntityKind::Player, index, protocol);
        // <= 772 permutes the four Avatar/Player fields into 26.x order.
        assert_eq!(at(15, 772), 17); // absorption
        assert_eq!(at(16, 772), 18); // score
        assert_eq!(at(17, 772), 16); // mode customisation
        assert_eq!(at(18, 772), 15); // main hand
        let mut mapped: Vec<u8> = (15..=18).map(|i| at(i, 764)).collect();
        mapped.sort_unstable();
        assert_eq!(mapped, [15, 16, 17, 18]);
        // 1.21.10 and newer already use the 26.x order; other kinds keep
        // their own subclass indices.
        assert_eq!(at(15, 773), 15);
        assert_eq!(at(17, 776), 17);
        assert_eq!(normalize_player_index_at(EntityKind::Zombie, 15, 772), 15);
    }
}
