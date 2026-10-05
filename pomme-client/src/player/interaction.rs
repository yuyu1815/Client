use std::collections::{BTreeMap, HashMap, HashSet};

use azalea_block::BlockState;
use azalea_core::attribute_modifier_operation::AttributeModifierOperation;
use azalea_core::direction::Direction;
use azalea_core::position::BlockPos;
use azalea_entity::dimensions::EntityDimensions;
use azalea_inventory::ItemStackData;
use azalea_inventory::components::{
    AttributeModifiers, BlocksAttacks, ChargedProjectiles, Consumable, EquipmentSlotGroup, Food,
    ItemUseAnimation, KineticWeapon, MinimumAttackCharge, Tool, ToolRule, UseEffects,
};
use azalea_protocol::packets::game::ServerboundGamePacket;
use azalea_protocol::packets::game::s_interact::InteractionHand;
use azalea_protocol::packets::game::s_player_action::{Action, ServerboundPlayerAction};
use azalea_protocol::packets::game::s_set_carried_item::ServerboundSetCarriedItem;
use azalea_protocol::packets::game::s_use_item::ServerboundUseItem;
use azalea_protocol::packets::game::s_use_item_on::{BlockHit, ServerboundUseItemOn};
use azalea_registry::builtin::{Attribute, BlockKind, EntityKind, ItemKind};
use glam::{DVec3, Vec3, dvec3};
use pomme_protocol::wire;

use crate::app::input::{self, InputState};
use crate::audio::{AudioEngine, CATEGORY_BLOCKS, CATEGORY_PLAYERS, SoundRef};
use crate::entity::EntityStore;
use crate::entity::components::{LookDirection, Position};
use crate::net::sender::PacketSender;
use crate::particle::ParticleStore;
use crate::physics::aabb::{self, Aabb, Axis, Face};
use crate::physics::block_shape::{self, LocalBox};
use crate::player::inventory::item_resource_name;
use crate::renderer::pipelines::held_item::UseAnim;
use crate::world::block::registry::BlockRegistry;
use crate::world::block::sound::block_sounds;
use crate::world::block::{has_collision, is_air};
use crate::world::chunk::ChunkStore;

const REACH: f32 = 4.5;
const ENTITY_REACH: f64 = 3.0;
const CREATIVE_ENTITY_REACH_BONUS: f64 = 2.0;
const DESTROY_COOLDOWN: u32 = 5;
const MISS_COOLDOWN: u32 = 10;
const USE_DELAY: u32 = 4;
const SWING_DURATION: i32 = 6;
/// Vanilla `Consumable`: no bite effects during the first ~22% of the use,
/// then a burst every 4 ticks.
const CONSUME_EFFECTS_START_FRACTION: f32 = 0.21875;
const CONSUME_EFFECTS_INTERVAL: i32 = 4;
const MAX_FOOD_LEVEL: u32 = 20;

fn brush_dust_tick(elapsed: i32) -> bool {
    elapsed % 10 == 5
}

fn brush_arm_flip(hand: InteractionHand, main_hand_right: bool) -> f64 {
    if (hand == InteractionHand::MainHand) == main_hand_right {
        1.0
    } else {
        -1.0
    }
}

fn brush_dust_direction(face: Direction, view: Vec3) -> (f32, f32) {
    match face {
        Direction::Down | Direction::Up => (view.z, -view.x),
        Direction::North => (1.0, -0.1),
        Direction::South => (-1.0, 0.1),
        Direction::West => (-0.1, -1.0),
        Direction::East => (0.1, 1.0),
    }
}

fn brush_block_particle(
    state: BlockState,
) -> (
    crate::particle::ServerParticleKind,
    crate::particle::ServerParticleOptions,
) {
    (
        crate::particle::ServerParticleKind::Block,
        crate::particle::ServerParticleOptions::Block(state),
    )
}

fn water_bucket_destination(
    hit: BlockHitResult,
    chunks: &ChunkStore,
    creative: bool,
    sneaking: bool,
) -> Option<BlockPos> {
    use crate::world::block::{
        can_be_replaced_by_water, can_place_water, is_air, liquid_block_container_type,
    };

    if hit.world_border {
        return None;
    }
    let loaded_state = |pos: BlockPos| {
        chunks
            .get_chunk(&azalea_core::position::ChunkPos::new(
                pos.x.div_euclid(16),
                pos.z.div_euclid(16),
            ))
            .map(|_| chunks.get_block_state(pos.x, pos.y, pos.z))
    };
    let adjacent = hit.block_pos.offset_with_direction(hit.face);
    let clicked = loaded_state(hit.block_pos)?;
    // BucketItem.use chooses the first location from the block's Java type,
    // before asking whether this particular state can accept water.
    let is_container = liquid_block_container_type(clicked)
        // Older per-protocol state tables predate `k`; preserve their historical
        // candidate rule until those tables can carry Java interface metadata.
        .unwrap_or_else(|| can_place_water(clicked, creative));
    let initial = if is_container {
        hit.block_pos
    } else {
        adjacent
    };
    let accepts = |state: BlockState, hit_result_present: bool| {
        let can_place_inside = match liquid_block_container_type(state) {
            Some(is_container) => is_container && can_place_water(state, creative),
            // Preserve the old selector/acceptance contract on historical
            // version tables that do not carry exact Java container metadata.
            None => can_place_water(state, creative),
        };
        is_air(state)
            || ((can_be_replaced_by_water(state) || can_place_inside)
                && (!sneaking || !hit_result_present))
    };
    if let Some(state) = loaded_state(initial)
        && accepts(state, true)
    {
        Some(initial)
    } else if let Some(state) = loaded_state(adjacent)
        && accepts(state, false)
    {
        // emptyContents retries relative to the original hit with a null hit
        // result; sneak restrictions therefore do not apply to the fallback.
        Some(adjacent)
    } else {
        None
    }
}

fn bucket_evaporation_particle() -> (
    crate::particle::ServerParticleKind,
    crate::particle::ServerParticleOptions,
) {
    (
        crate::particle::ServerParticleKind::LargeSmoke,
        crate::particle::ServerParticleOptions::Simple,
    )
}

fn candle_particle_offsets(state: BlockState) -> Option<&'static [(f64, f64, f64)]> {
    use crate::world::block::{block_id, block_properties};
    match block_id(state) {
        "candle" | "white_candle" | "light_gray_candle" | "gray_candle" | "black_candle"
        | "brown_candle" | "red_candle" | "orange_candle" | "yellow_candle" | "lime_candle"
        | "green_candle" | "cyan_candle" | "light_blue_candle" | "blue_candle"
        | "purple_candle" | "magenta_candle" | "pink_candle" => {
            const OFFSETS: [&[(f64, f64, f64)]; 4] = [
                &[(0.5, 0.5, 0.5)],
                &[(0.375, 0.4375, 0.5), (0.625, 0.5, 0.4375)],
                &[
                    (0.5, 0.3125, 0.625),
                    (0.375, 0.4375, 0.5),
                    (0.5625, 0.5, 0.4375),
                ],
                &[
                    (0.4375, 0.3125, 0.5625),
                    (0.625, 0.4375, 0.5625),
                    (0.375, 0.4375, 0.375),
                    (0.5625, 0.5, 0.375),
                ],
            ];
            let count = block_properties(state)
                .get("candles")
                .and_then(|value| value.parse::<usize>().ok())?
                .clamp(1, 4);
            Some(OFFSETS[count - 1])
        }
        "candle_cake"
        | "white_candle_cake"
        | "light_gray_candle_cake"
        | "gray_candle_cake"
        | "black_candle_cake"
        | "brown_candle_cake"
        | "red_candle_cake"
        | "orange_candle_cake"
        | "yellow_candle_cake"
        | "lime_candle_cake"
        | "green_candle_cake"
        | "cyan_candle_cake"
        | "light_blue_candle_cake"
        | "blue_candle_cake"
        | "purple_candle_cake"
        | "magenta_candle_cake"
        | "pink_candle_cake" => Some(&[(0.5, 1.0, 0.5)]),
        _ => None,
    }
}

fn candle_extinguish_requests(
    state: BlockState,
    pos: BlockPos,
    hit_y: f64,
    hand_empty: bool,
    may_build: bool,
) -> Vec<crate::world::particle_tick::ParticleSpawnRequest> {
    use crate::particle::{ServerParticleKind as Kind, ServerParticleOptions as Options};
    use crate::world::block::{block_id, block_properties};
    let Some(offsets) = candle_particle_offsets(state) else {
        return Vec::new();
    };
    let id = block_id(state);
    let candle_cake = id == "candle_cake" || id.ends_with("_candle_cake");
    if !hand_empty
        || !may_build
        || block_properties(state).get("lit") != Some("true")
        || (candle_cake && hit_y - f64::from(pos.y) <= 0.5)
    {
        return Vec::new();
    }
    offsets
        .iter()
        .map(
            |&(x, y, z)| crate::world::particle_tick::ParticleSpawnRequest {
                kind: Kind::Smoke,
                options: Options::Simple,
                position: dvec3(
                    f64::from(pos.x) + x,
                    f64::from(pos.y) + y,
                    f64::from(pos.z) + z,
                ),
                velocity: dvec3(0.0, 0.1, 0.0),
                always_visible: false,
            },
        )
        .collect()
}

fn dragon_egg_teleport_requests(
    pos: BlockPos,
    chunks: &ChunkStore,
    border: &crate::world::border::WorldBorder,
    seed: u64,
) -> Vec<crate::world::particle_tick::ParticleSpawnRequest> {
    use crate::particle::{ServerParticleKind as Kind, ServerParticleOptions as Options};
    let mut rng = fastrand::Rng::with_seed(seed);
    let min_y = chunks.min_y();
    let max_y = min_y + chunks.height() as i32;
    for _ in 0..1000 {
        let candidate = BlockPos::new(
            pos.x + rng.i32(0..16) - rng.i32(0..16),
            pos.y + rng.i32(0..8) - rng.i32(0..8),
            pos.z + rng.i32(0..16) - rng.i32(0..16),
        );
        if candidate.y < min_y
            || candidate.y >= max_y
            || !border.contains(f64::from(candidate.x), f64::from(candidate.z))
            || chunks
                .get_chunk(&azalea_core::position::ChunkPos::new(
                    candidate.x.div_euclid(16),
                    candidate.z.div_euclid(16),
                ))
                .is_none()
            || !is_air(chunks.get_block_state(candidate.x, candidate.y, candidate.z))
            || is_air(chunks.get_block_state(candidate.x, candidate.y - 1, candidate.z))
        {
            continue;
        }
        return (0..128)
            .map(|_| {
                let t = rng.f64();
                let velocity = dvec3(
                    f64::from((rng.f32() - 0.5) * 0.2),
                    f64::from((rng.f32() - 0.5) * 0.2),
                    f64::from((rng.f32() - 0.5) * 0.2),
                );
                let x = (f64::from(candidate.x) + t * (f64::from(pos.x) - f64::from(candidate.x)))
                    + (rng.f64() - 0.5)
                    + 0.5;
                let y = f64::from(candidate.y)
                    + t * (f64::from(pos.y) - f64::from(candidate.y))
                    + rng.f64()
                    - 0.5;
                let z = (f64::from(candidate.z) + t * (f64::from(pos.z) - f64::from(candidate.z)))
                    + (rng.f64() - 0.5)
                    + 0.5;
                crate::world::particle_tick::ParticleSpawnRequest {
                    kind: Kind::Portal,
                    options: Options::Simple,
                    position: dvec3(x, y, z),
                    velocity,
                    always_visible: false,
                }
            })
            .collect();
    }
    Vec::new()
}

fn spawn_interaction_particle_requests(
    requests: Vec<crate::world::particle_tick::ParticleSpawnRequest>,
    camera_pos: DVec3,
    chunks: &ChunkStore,
    effects: &mut BreakEffects,
) {
    for request in requests {
        effects.particles.add_particle_spawn_request(
            request,
            camera_pos,
            effects.registry,
            chunks,
            effects.biome_climate,
        );
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ItemUseResult {
    Pass,
    Success,
    Fail,
}

/// Handles the predicted-break effects need (vanilla level event 2001 spawns
/// break particles alongside the sound).
pub struct BreakEffects<'a> {
    pub particles: &'a mut ParticleStore,
    pub registry: &'a BlockRegistry,
    pub biome_climate: &'a HashMap<u32, crate::renderer::chunk::mesher::BiomeClimate>,
}

#[derive(Debug, Clone, Copy)]
pub struct BlockHitResult {
    pub block_pos: BlockPos,
    pub face: Direction,
    pub hit_point: DVec3,
    pub inside: bool,
    pub world_border: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct EntityHitResult {
    pub entity_id: i32,
    pub location: DVec3,
    pub entity_pos: DVec3,
}

#[derive(Debug, Clone, Copy)]
pub enum HitResult {
    Block(BlockHitResult),
    Entity(EntityHitResult),
}

/// Last server-known state for a locally-predicted block change, matching
/// vanilla `BlockStatePredictionHandler.ServerVerifiedState`.
struct ServerVerifiedState {
    seq: u32,
    state: BlockState,
    player_pos: DVec3,
}

/// Submitted placement awaiting its server-owned inventory count/ACK.
#[derive(Clone, Copy)]
struct PlacementUse {
    seq: u32,
    kind: ItemKind,
    count: i32,
    pos: BlockPos,
    previous: BlockState,
    prediction: Option<BlockState>,
    rejected: bool,
}

/// A locally tracked active use or physical-button latch. Consumable effects
/// and the special hold-use lifecycles remain server-authoritative.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActiveUseKind {
    Consumable,
    Bow,
    CrossbowCharge,
    CrossbowFire,
    Trident,
    Spyglass,
    Shield,
}

struct ActiveUse {
    hand: InteractionHand,
    kind: ItemKind,
    use_kind: ActiveUseKind,
    anim: ItemUseAnimation,
    bow: bool,
    sound: SoundRef,
    has_particles: bool,
    use_effects: UseEffects,
    duration: i32,
    /// Counts down from `duration`; vanilla lets it run negative until the
    /// server completes the use.
    remaining: i32,
}

#[derive(Clone, Copy)]
struct MiningContext {
    mining_efficiency: f32,
    haste_amplifier: Option<u8>,
    fatigue_amplifier: Option<u8>,
    block_break_speed: f32,
    submerged_mining_speed: f32,
    eyes_in_water: bool,
}

impl Default for MiningContext {
    fn default() -> Self {
        Self {
            mining_efficiency: 0.0,
            haste_amplifier: None,
            fatigue_amplifier: None,
            block_break_speed: 1.0,
            submerged_mining_speed: 0.2,
            eyes_in_water: false,
        }
    }
}

impl MiningContext {
    fn from_player(player: &crate::player::LocalPlayer) -> Self {
        let mut result = Self {
            mining_efficiency: player.attribute_value("minecraft:mining_efficiency", 0.0) as f32,
            block_break_speed: player.attribute_value("minecraft:block_break_speed", 1.0) as f32,
            submerged_mining_speed: player.attribute_value("minecraft:submerged_mining_speed", 0.2)
                as f32,
            eyes_in_water: player.eyes_in_water,
            ..Self::default()
        };
        for effect in player.effects.sorted_desc() {
            match crate::mob_effect::info(effect.effect_id).map(|info| info.name) {
                Some("haste" | "conduit_power") => {
                    result.haste_amplifier = Some(
                        result
                            .haste_amplifier
                            .map_or(effect.amplifier, |amp| amp.max(effect.amplifier)),
                    );
                }
                Some("mining_fatigue") => result.fatigue_amplifier = Some(effect.amplifier),
                _ => {}
            }
        }
        result
    }
}

pub struct InteractionState {
    pub target: Option<HitResult>,
    world_border: crate::world::border::WorldBorder,
    may_build: bool,
    seq: u32,
    carried_slot: u8,
    last_teleport_seq: u32,
    pending_predictions: HashMap<BlockPos, ServerVerifiedState>,
    /// Pre-write states; drained after interaction/ACK, before priority remesh.
    visual_edits: Vec<(BlockPos, BlockState)>,
    is_destroying: bool,
    destroy_pos: BlockPos,
    /// Held stack captured when the break started, vanilla `destroyingItem`;
    /// a mid-mine change of item or components restarts the break.
    destroying_item: Option<ItemStackData>,
    destroy_progress: f32,
    destroy_ticks: f32,
    destroy_delay: u32,
    miss_time: u32,
    use_delay: u32,
    pending_writable_book: Option<InteractionHand>,
    pub pending_command_block: Option<(BlockPos, std::time::Instant)>,
    using_item: Option<ActiveUse>,
    brush_use: Option<(InteractionHand, i32)>,
    water_evaporates: bool,
    main_hand_right: bool,
    /// Keeps charge/one-shot items from retriggering while the use button stays
    /// down, even if the server clears its using-item metadata mid-hold.
    use_latch: Option<(InteractionHand, ItemKind)>,
    using_bow: bool,
    swinging: bool,
    swinging_hand: InteractionHand,
    swing_time: i32,
    /// ItemInHandRenderer heights, indexed main/off hand.
    hand_height: [f32; 2],
    o_hand_height: [f32; 2],
    pending_placement_uses: [Option<PlacementUse>; 2],
    /// LocalPlayer xBob/yBob (pitch/yaw), independent of world-camera bob.
    hand_view_bob: glam::Vec2,
    o_hand_view_bob: glam::Vec2,
    attack_anim: f32,
    o_attack_anim: f32,
    /// Vanilla `Player.attackStrengthTicker`. The companion `itemSwapTicker`
    /// (hand-raise animation) is not tracked.
    attack_strength_ticker: u32,
    /// Vanilla `Player.lastItemInMainHand`; `None` is the empty hand.
    last_item_in_main_hand: Option<ItemStackData>,
    mining_context: MiningContext,
    item_tags: BTreeMap<String, Vec<ItemKind>>,
    thundering: bool,
    animal_particle_interactions: Vec<AnimalParticleInteraction>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AnimalParticleInteraction {
    ForcedAge(i32),
    AgeLock(i32, bool),
    Immediate(i32, crate::entity::particle_animals::InteractionParticle),
    MooshroomStew(i32, ItemKind),
}

impl InteractionState {
    pub fn new() -> Self {
        Self {
            target: None,
            world_border: crate::world::border::WorldBorder::default(),
            may_build: true,
            seq: 0,
            // Vanilla `MultiPlayerGameMode.carriedIndex` starts at 0, the slot
            // a fresh inventory selects, so a join sends nothing until it
            // changes.
            carried_slot: 0,
            last_teleport_seq: 0,
            pending_predictions: HashMap::new(),
            visual_edits: Vec::new(),
            is_destroying: false,
            destroy_pos: BlockPos {
                x: -1,
                y: -1,
                z: -1,
            },
            destroying_item: None,
            destroy_progress: 0.0,
            destroy_ticks: 0.0,
            destroy_delay: 0,
            miss_time: 0,
            use_delay: 0,
            pending_writable_book: None,
            pending_command_block: None,
            using_item: None,
            brush_use: None,
            water_evaporates: false,
            main_hand_right: true,
            use_latch: None,
            using_bow: false,
            swinging: false,
            swinging_hand: InteractionHand::MainHand,
            swing_time: 0,
            hand_height: [0.0; 2],
            o_hand_height: [0.0; 2],
            pending_placement_uses: [None; 2],
            hand_view_bob: glam::Vec2::ZERO,
            o_hand_view_bob: glam::Vec2::ZERO,
            attack_anim: 0.0,
            o_attack_anim: 0.0,
            attack_strength_ticker: 0,
            last_item_in_main_hand: None,
            mining_context: MiningContext::default(),
            item_tags: BTreeMap::new(),
            thundering: false,
            animal_particle_interactions: Vec::new(),
        }
    }

    pub fn take_visual_edits(&mut self) -> Vec<(BlockPos, BlockState)> {
        std::mem::take(&mut self.visual_edits)
    }

    pub(crate) fn set_item_tags(&mut self, tags: &BTreeMap<String, Vec<ItemKind>>) {
        if self.item_tags != *tags {
            self.item_tags.clone_from(tags);
        }
    }

    pub(crate) fn set_thundering(&mut self, thundering: bool) {
        self.thundering = thundering;
    }

    pub(crate) fn take_animal_particle_interactions(&mut self) -> Vec<AnimalParticleInteraction> {
        std::mem::take(&mut self.animal_particle_interactions)
    }

    /// Vanilla `retainKnownServerState`: an existing entry only gets its
    /// sequence bumped, since its stored state is already the server's.
    fn retain_known_server_state(&mut self, pos: BlockPos, state: BlockState, player_pos: DVec3) {
        self.pending_predictions
            .entry(pos)
            .and_modify(|v| v.seq = self.seq)
            .or_insert(ServerVerifiedState {
                seq: self.seq,
                state,
                player_pos,
            });
    }

    /// Vanilla `updateKnownServerState`: a server block update for a predicted
    /// position only refreshes the stored state. Returns true if absorbed, in
    /// which case the caller must not apply the update to the world.
    pub fn update_known_server_state(&mut self, pos: &BlockPos, state: BlockState) -> bool {
        if let Some(v) = self.pending_predictions.get_mut(pos) {
            v.state = state;
            true
        } else {
            false
        }
    }

    pub fn on_teleport(&mut self) {
        self.last_teleport_seq = self.seq;
    }

    /// Applies a predicted break locally: remembers the server state for
    /// rollback, clears the block, and plays the break effects.
    #[allow(clippy::too_many_arguments)]
    fn predict_destroy(
        &mut self,
        pos: BlockPos,
        state: BlockState,
        player_pos: DVec3,
        chunks: &ChunkStore,
        audio: &mut AudioEngine,
        effects: &mut BreakEffects,
        dirty_chunks: &mut Vec<BlockPos>,
        recorder: &crate::movement_record::Recorder,
    ) {
        self.retain_known_server_state(pos, state, player_pos);
        self.visual_edits.push((pos, state));
        chunks.set_block_state(pos.x, pos.y, pos.z, BlockState::AIR);
        recorder.local_prediction(pos, state, BlockState::AIR, self.seq);
        mark_dirty(&pos, dirty_chunks);
        play_break_sound(audio, state, pos);
        effects.particles.add_destroy_block_effect(
            pos,
            state,
            effects.registry,
            chunks,
            effects.biome_climate,
        );
    }

    /// Vanilla `endPredictionsUpTo` + `ClientLevel.syncBlockState`: resolves
    /// every prediction up to `seq` to the server-verified state, so rejected
    /// breaks pop back instead of desyncing the world. Returns the position to
    /// snap the player back to when a restored block overlaps them.
    pub fn acknowledge(
        &mut self,
        seq: u32,
        chunks: &ChunkStore,
        player: Aabb,
        dirty_chunks: &mut Vec<BlockPos>,
    ) -> Option<DVec3> {
        let snap_allowed = self.last_teleport_seq < seq;
        // Keep the lowest block pos among overlapping reverts so the chosen snap
        // is deterministic (HashMap iteration order is not).
        let mut snap_to: Option<((i32, i32, i32), DVec3)> = None;
        let visual_edits = &mut self.visual_edits;
        self.pending_predictions.retain(|pos, verified| {
            if verified.seq > seq {
                return true;
            }
            let current = chunks.get_block_state(pos.x, pos.y, pos.z);
            if current != verified.state {
                tracing::debug!(
                    "Server did not confirm block change at {pos:?}, reverting to {:?}",
                    verified.state
                );
                visual_edits.push((*pos, current));
                chunks.set_block_state(pos.x, pos.y, pos.z, verified.state);
                mark_dirty(pos, dirty_chunks);
                // Full-cube collision, as the engine has no per-shape voxels.
                if snap_allowed && has_collision(verified.state) {
                    let block = Aabb::block(pos.x, pos.y, pos.z);
                    let key = (pos.x, pos.y, pos.z);
                    if block.intersects(&player) && snap_to.is_none_or(|(best, _)| key < best) {
                        snap_to = Some((key, verified.player_pos));
                    }
                }
            }
            false
        });
        // ACK and inventory updates can straddle client ticks. Keep a confirmed
        // placement waiting for its count update; discard rejections after the
        // next count check (a falling block can consume an item then disappear).
        for pending in self.pending_placement_uses.iter_mut().flatten() {
            if pending.seq <= seq {
                let current = chunks.get_block_state(pending.pos.x, pending.pos.y, pending.pos.z);
                pending.rejected = pending
                    .prediction
                    .map_or(current == pending.previous, |state| current != state);
            }
        }
        snap_to.map(|(_, pos)| pos)
    }

    pub fn destroy_stage(&self) -> Option<(BlockPos, u32)> {
        if !self.is_destroying || self.destroy_progress <= 0.0 {
            return None;
        }
        let stage = (self.destroy_progress * 10.0) as u32;
        Some((self.destroy_pos, stage.min(9)))
    }

    pub fn get_swing_progress(&self, partial_tick: f32) -> f32 {
        let mut diff = self.attack_anim - self.o_attack_anim;
        if diff < 0.0 {
            diff += 1.0;
        }
        self.o_attack_anim + diff * partial_tick
    }

    pub fn hand_animation(
        &self,
        partial_tick: f32,
        view: LookDirection,
    ) -> crate::renderer::pipelines::hand::HandAnimation {
        let mut swing_progress = [0.0; 2];
        swing_progress[hand_index(self.swinging_hand)] = self.get_swing_progress(partial_tick);
        let inverse_height = std::array::from_fn(|i| {
            1.0 - (self.o_hand_height[i]
                + (self.hand_height[i] - self.o_hand_height[i]) * partial_tick)
        });
        let bob = self.o_hand_view_bob.lerp(self.hand_view_bob, partial_tick);
        crate::renderer::pipelines::hand::HandAnimation {
            swing_progress,
            inverse_height,
            view_follow: glam::Mat4::from_rotation_x(
                ((view.x_rot_deg() - bob.x) * 0.1).to_radians(),
            ) * glam::Mat4::from_rotation_y(
                ((view.y_rot_deg() - bob.y) * 0.1).to_radians(),
            ),
        }
    }

    /// Vanilla ItemInHandRenderer.itemUsed: only the successfully used hand
    /// lowers.
    pub fn item_used(&mut self, hand: InteractionHand) {
        self.hand_height[hand_index(hand)] = 0.0;
    }

    fn placement_used(
        &mut self,
        hand: InteractionHand,
        stack: &ItemStackData,
        creative: bool,
        pos: BlockPos,
        previous: BlockState,
        prediction: Option<BlockState>,
    ) {
        if creative {
            self.item_used(hand);
        } else {
            // Inventory counts are server-owned here (prediction does not shrink
            // stacks). Lower on the resulting count update, never on a rejected ACK.
            self.pending_placement_uses[hand_index(hand)] = Some(PlacementUse {
                seq: self.seq,
                kind: stack.kind,
                count: stack.count,
                pos,
                previous,
                prediction,
                rejected: false,
            });
        }
    }

    fn update_placement_heights(
        &mut self,
        main: Option<&ItemStackData>,
        off: Option<&ItemStackData>,
    ) {
        for hand in [InteractionHand::MainHand, InteractionHand::OffHand] {
            let index = hand_index(hand);
            let Some(pending) = self.pending_placement_uses[index] else {
                continue;
            };
            let stack = stack_for_hand(hand, main, off);
            if stack.is_none_or(|stack| stack.kind == pending.kind)
                && stack.map_or(0, |stack| stack.count) != pending.count
            {
                self.item_used(hand);
                self.pending_placement_uses[index] = None;
            } else if pending.rejected || stack.is_some_and(|stack| stack.kind != pending.kind) {
                self.pending_placement_uses[index] = None;
            }
        }
    }

    fn tick_hand_animation(&mut self, look: LookDirection) {
        self.o_hand_height = self.hand_height;
        for height in &mut self.hand_height {
            *height += (1.0 - *height).clamp(-0.4, 0.4);
        }
        self.o_hand_view_bob = self.hand_view_bob;
        self.hand_view_bob +=
            (glam::Vec2::new(look.x_rot_deg(), look.y_rot_deg()) - self.hand_view_bob) * 0.5;
    }

    fn start_swing(&mut self, hand: InteractionHand) {
        if !self.swinging || self.swing_time >= SWING_DURATION / 2 || self.swing_time < 0 {
            self.swing_time = -1;
            self.swinging = true;
            self.swinging_hand = hand;
        }
    }

    /// An attack or mining swing, always reported to the server.
    fn swing(&mut self, sender: &PacketSender) {
        self.start_swing(InteractionHand::MainHand);
        send_swing(sender);
    }

    /// A swing from using an item or entity; see [`send_use_swing`].
    fn swing_use(&mut self, sender: &PacketSender, hand: InteractionHand) {
        self.start_swing(hand);
        send_use_swing(sender, hand);
    }

    fn update_swing(&mut self) {
        self.o_attack_anim = self.attack_anim;
        if self.swinging {
            self.swing_time += 1;
            if self.swing_time >= SWING_DURATION {
                self.swing_time = 0;
                self.swinging = false;
            }
        } else {
            self.swing_time = 0;
        }
        self.attack_anim = self.swing_time as f32 / SWING_DURATION as f32;
    }

    /// Ports vanilla `LocalPlayer.pick`: block raycast first, entity ray
    /// truncated at the block hit, the entity wins only if strictly closer
    /// and within entity reach; otherwise the block hit remains available.
    pub fn update_target(
        &mut self,
        eye_pos: Position,
        look_dir: LookDirection,
        chunks: &ChunkStore,
        entities: &EntityStore,
        creative: bool,
        world_border: &crate::world::border::WorldBorder,
    ) {
        self.world_border = *world_border;
        let entity_reach = ENTITY_REACH
            + if creative {
                CREATIVE_ENTITY_REACH_BONUS
            } else {
                0.0
            };
        let max_dist = (REACH as f64).max(entity_reach);

        let from: DVec3 = eye_pos.into();
        let dir = look_dir.as_vec();
        let block_hit = raycast(from, dir, REACH, chunks, world_border);

        let block_dist_sq = block_hit
            .map(|h| h.hit_point.distance_squared(from))
            .unwrap_or(max_dist * max_dist);
        let to = from + dir.as_dvec3() * block_dist_sq.sqrt();

        if let Some(hit) = nearest_entity_hit(from, to, entities) {
            let dist_sq = hit.location.distance_squared(from);
            if entity_hit_wins(dist_sq, block_dist_sq, entity_reach) {
                self.target = Some(HitResult::Entity(hit));
                return;
            }
        }

        self.target = block_hit.map(HitResult::Block);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn tick(
        &mut self,
        input: &InputState,
        chunks: &ChunkStore,
        sender: &PacketSender,
        audio: &mut AudioEngine,
        player_pos: DVec3,
        player_aabb: Aabb,
        eye_pos: DVec3,
        look: LookDirection,
        on_ground: bool,
        creative: bool,
        spectator: bool,
        entities: &EntityStore,
        hand: InteractionHand,
        hand_on_cooldown: bool,
        food: u32,
        selected_slot: u8,
        held_stack: Option<&ItemStackData>,
        offhand_stack: Option<&ItemStackData>,
        has_projectile: bool,
        place_block: Option<BlockState>,
        offhand_place_block: Option<BlockState>,
        offhand_on_cooldown: bool,
        hands_empty: bool,
        player: &crate::player::LocalPlayer,
        effects: &mut BreakEffects,
    ) -> Vec<BlockPos> {
        self.mining_context = MiningContext::from_player(player);
        self.may_build = player.may_build;
        let mut dirty_chunks = Vec::new();
        self.tick_hand_animation(look);
        self.update_placement_heights(held_stack, offhand_stack);

        self.ensure_has_sent_carried_item(sender, selected_slot);
        let mut cancelled_use = false;
        if let Some((latched_hand, latched_kind)) = self.use_latch
            && stack_for_hand(latched_hand, held_stack, offhand_stack).map(|stack| stack.kind)
                != Some(latched_kind)
        {
            if self.using_item.is_some() || self.using_bow {
                self.release_using_item(sender);
                self.use_latch = Some((latched_hand, latched_kind));
            }
            // Keep the old-hand latch until the button comes up; otherwise the
            // replacement stack could be used by the held-input retry next tick.
            cancelled_use = true;
        }
        if spectator {
            self.using_item = None;
            self.use_latch = None;
            self.using_bow = false;
            self.clear_destroying_state();
        }

        // Vanilla `Minecraft.tick` order: attack/use input (which triggers the
        // swing) runs first, then `--missTime`, then the player entity advances
        // `updateSwingTime` and `updatingUsingItem`. Running `update_swing`
        // last keeps the swing animation cadence in lockstep with vanilla.
        if !input.is_cursor_captured() {
            if spectator {
                self.clear_destroying_state();
            } else {
                self.stop_destroying(sender);
            }
            // No screen-open release in vanilla either: an in-flight use keeps
            // ticking (and completing) while a menu is up.
            self.update_using_item(
                held_stack,
                offhand_stack,
                audio,
                chunks,
                player_pos,
                eye_pos,
                look,
                effects,
            );
            self.tick_brush_use(
                input,
                held_stack,
                offhand_stack,
                look,
                eye_pos,
                chunks,
                effects,
            );
            self.tick_attack_cooldown(held_stack);
            self.update_swing();
            return dirty_chunks;
        }

        // Vanilla `handleKeybinds` drains attack clicks while an item is in
        // use, and `continueAttack` early-returns on `isUsingItem`.
        let using = self.using_item.is_some() || self.using_bow || self.brush_use.is_some();
        let use_latched = self.use_latch.is_some();

        if !using && !use_latched && input.action_just_pressed(input::Action::Destroy) {
            self.start_attack(
                chunks,
                sender,
                audio,
                input,
                player_pos,
                on_ground,
                creative,
                spectator,
                held_stack,
                effects,
                &mut dirty_chunks,
            );
        }

        // Vanilla `handleKeybinds`: while an item is in use, holding the use
        // key continues it and releasing sends RELEASE_USE_ITEM (an early
        // cancel; consumables finish on the server's own timer, never on
        // release).
        if using {
            if !input.performing_action(input::Action::Use) {
                self.release_using_item(sender);
            }
        } else if use_latched {
            // Server metadata can stop an item use before the physical button
            // is released. Keep the edge latch until that release, but don't
            // send a second release packet for a server-stopped use.
            if !input.performing_action(input::Action::Use) {
                self.use_latch = None;
            }
        } else if !cancelled_use
            && (input.action_just_pressed(input::Action::Use)
                || (input.performing_action(input::Action::Use) && self.use_delay == 0))
        {
            let sneaking = input.performing_action(input::Action::Sneak);
            let suppress_block_use = sneaking && !hands_empty;
            let success = self.start_use_item(
                sender,
                audio,
                chunks,
                player_pos,
                player_aabb,
                eye_pos,
                look,
                place_block,
                offhand_place_block,
                held_stack,
                food,
                creative,
                spectator,
                entities,
                hand,
                hand_on_cooldown,
                offhand_stack,
                offhand_on_cooldown,
                has_projectile,
                sneaking,
                suppress_block_use,
                effects,
                &mut dirty_chunks,
            );
            if success {
                let _ = input.weak_rumble_for_instant();
            }
        }

        // Vanilla checks `isUsingItem` once before the attack/use/pick loops,
        // so a use started above does not suppress a pick from the same tick.
        if !using && !use_latched && input.middle_just_pressed() {
            self.pick_block_or_entity(sender, input.ctrl_held());
        }

        let attack_down = input.performing_action(input::Action::Destroy);
        if !attack_down {
            self.miss_time = 0;
        }
        if spectator {
            // Mode changes must not leave an ordinary mining session running;
            // spectator policy does not emit an ordinary abort-dig packet.
            self.clear_destroying_state();
        } else if self.using_item.is_none() && self.use_latch.is_none() {
            if attack_down {
                self.continue_attack(
                    chunks,
                    sender,
                    audio,
                    player_pos,
                    on_ground,
                    creative,
                    held_stack,
                    effects,
                    &mut dirty_chunks,
                );
            } else {
                self.stop_destroying(sender);
            }
        }

        if self.is_destroying {
            let _ = input.strong_rumble_for_tick();
        }

        if self.miss_time > 0 {
            self.miss_time -= 1;
        }
        if self.use_delay > 0 {
            self.use_delay -= 1;
        }
        self.update_using_item(
            held_stack,
            offhand_stack,
            audio,
            chunks,
            player_pos,
            eye_pos,
            look,
            effects,
        );
        self.tick_brush_use(
            input,
            held_stack,
            offhand_stack,
            look,
            eye_pos,
            chunks,
            effects,
        );
        self.tick_attack_cooldown(held_stack);
        self.update_swing();

        dirty_chunks
    }

    fn tick_brush_use(
        &mut self,
        input: &InputState,
        held_stack: Option<&ItemStackData>,
        offhand_stack: Option<&ItemStackData>,
        look: LookDirection,
        eye_pos: DVec3,
        chunks: &ChunkStore,
        effects: &mut BreakEffects,
    ) {
        let Some((hand, elapsed)) = self.brush_use else {
            return;
        };
        if !input.performing_action(input::Action::Use)
            || stack_for_hand(hand, held_stack, offhand_stack)
                .is_none_or(|stack| stack.kind != ItemKind::Brush)
        {
            self.brush_use = None;
            return;
        }
        let elapsed = elapsed + 1;
        self.brush_use = Some((hand, elapsed));
        if !brush_dust_tick(elapsed) {
            return;
        }
        let Some(HitResult::Block(hit)) = self.target else {
            self.brush_use = None;
            return;
        };
        let state = chunks.get_block_state(hit.block_pos.x, hit.block_pos.y, hit.block_pos.z);
        if !crate::world::block::should_spawn_terrain_particles(state)
            || crate::world::block::has_invisible_render_shape(state)
        {
            return;
        }
        let dir = look.as_vec();
        let flip = brush_arm_flip(hand, self.main_hand_right);
        let (dx, dz) = brush_dust_direction(hit.face, dir);
        let mut pos = hit.hit_point;
        if hit.face == Direction::West {
            pos.x -= 1.0e-6;
        } else if hit.face == Direction::North {
            pos.z -= 1.0e-6;
        }
        let count = fastrand::usize(7..12);
        for _ in 0..count {
            self.spawn_interaction_block_particle(
                state,
                pos,
                dvec3(
                    f64::from(dx) * flip * 3.0 * fastrand::f64(),
                    0.0,
                    f64::from(dz) * flip * 3.0 * fastrand::f64(),
                ),
                eye_pos,
                chunks,
                effects,
            );
        }
    }

    fn spawn_dragon_egg_particles(
        &self,
        pos: BlockPos,
        camera_pos: DVec3,
        chunks: &ChunkStore,
        effects: &mut BreakEffects,
    ) {
        let requests =
            dragon_egg_teleport_requests(pos, chunks, &self.world_border, fastrand::u64(..));
        spawn_interaction_particle_requests(requests, camera_pos, chunks, effects);
    }

    fn spawn_interaction_block_particle(
        &self,
        state: BlockState,
        pos: DVec3,
        velocity: DVec3,
        camera_pos: DVec3,
        chunks: &ChunkStore,
        effects: &mut BreakEffects,
    ) {
        let (kind, options) = brush_block_particle(state);
        effects.particles.add_particles_from_packet(
            kind,
            options,
            false,
            false,
            pos,
            velocity,
            1.0,
            0,
            camera_pos,
            effects.registry,
            chunks,
            effects.biome_climate,
        );
    }

    fn pick_block_or_entity(&self, sender: &PacketSender, include_data: bool) {
        match self.target {
            Some(HitResult::Block(hit)) => sender.send_raw(wire::encode_pick_item_from_block(
                hit.block_pos.x,
                hit.block_pos.y,
                hit.block_pos.z,
                include_data,
            )),
            Some(HitResult::Entity(hit)) => {
                sender.send_raw(wire::encode_pick_item_from_entity(
                    hit.entity_id,
                    include_data,
                ));
            }
            None => {}
        }
    }

    /// Vanilla `Player.tick`: advance the attack cooldown, and reset it when
    /// the main-hand item *type* changes; component or count changes only
    /// refresh the cache.
    fn tick_attack_cooldown(&mut self, held_stack: Option<&ItemStackData>) {
        self.attack_strength_ticker = self.attack_strength_ticker.saturating_add(1);
        // Vanilla `ItemStack.matches`: same item, components, and count.
        let matches = same_item_same_components(held_stack, self.last_item_in_main_hand.as_ref())
            && held_stack.map(|s| s.count) == self.last_item_in_main_hand.as_ref().map(|s| s.count);
        if !matches {
            let same_type = match (held_stack, &self.last_item_in_main_hand) {
                (None, None) => true,
                (Some(a), Some(b)) => a.kind == b.kind,
                _ => false,
            };
            if !same_type {
                self.attack_strength_ticker = 0;
            }
            self.last_item_in_main_hand = held_stack.cloned();
        }
    }

    /// Vanilla `Player.getAttackStrengthScale(0.0)` (the HUD passes no
    /// partial tick).
    pub fn attack_strength_scale(&self, delay: f32) -> f32 {
        (self.attack_strength_ticker as f32 / delay).clamp(0.0, 1.0)
    }

    /// Vanilla `Player.cannotAttackWithItem(stack, 0)` (the one call site
    /// passes no tolerance); the ratio is unclamped, unlike the scale.
    fn cannot_attack_with_item(&self, held: Option<&ItemStackData>) -> bool {
        let required = held
            .and_then(crate::player::menu_click::component::<MinimumAttackCharge>)
            .map_or(0.0, |c| c.value);
        required > 0.0
            && (self.attack_strength_ticker as f32 / attack_strength_delay(held)) < required
    }

    #[allow(clippy::too_many_arguments)]
    fn start_attack(
        &mut self,
        chunks: &ChunkStore,
        sender: &PacketSender,
        audio: &mut AudioEngine,
        input: &InputState,
        player_pos: DVec3,
        on_ground: bool,
        creative: bool,
        spectator: bool,
        held_stack: Option<&ItemStackData>,
        effects: &mut BreakEffects,
        dirty_chunks: &mut Vec<BlockPos>,
    ) {
        if spectator || self.miss_time > 0 {
            return;
        }

        // TODO: full-charge spears take vanilla's PIERCING_WEAPON branch
        // instead of the plain entity/block dispatch.
        if self.cannot_attack_with_item(held_stack) {
            return;
        }

        let hit = match self.target {
            None => {
                // Vanilla `Minecraft.startAttack` MISS branch.
                self.miss_time = MISS_COOLDOWN;
                self.attack_strength_ticker = 0;
                self.swing(sender);
                return;
            }
            Some(HitResult::Entity(hit)) => {
                sender.send_raw(wire::encode_attack(hit.entity_id));
                self.attack_strength_ticker = 0;
                self.swing(sender);
                let _ = input.weak_rumble_for_instant();
                return;
            }
            Some(HitResult::Block(hit)) => hit,
        };

        let state = chunks.get_block_state(hit.block_pos.x, hit.block_pos.y, hit.block_pos.z);
        if is_air(state) {
            self.miss_time = MISS_COOLDOWN;
            self.attack_strength_ticker = 0;
            self.swing(sender);
            return;
        }

        if crate::world::block::block_id(state) == "dragon_egg" {
            self.spawn_dragon_egg_particles(hit.block_pos, player_pos, chunks, effects);
        }
        if !hit.world_border && self.may_build {
            spawn_interaction_particle_requests(
                crate::world::particle_tick::redstone_ore_interaction_requests(
                    chunks,
                    hit.block_pos,
                    (self.seq as u64).wrapping_add(0x7265_6473_746f_6e65),
                ),
                player_pos,
                chunks,
                effects,
            );
        }

        self.start_destroy_block(
            hit,
            chunks,
            sender,
            audio,
            player_pos,
            on_ground,
            creative,
            held_stack,
            effects,
            dirty_chunks,
        );
        self.swing(sender);
    }

    #[allow(clippy::too_many_arguments)]
    fn continue_attack(
        &mut self,
        chunks: &ChunkStore,
        sender: &PacketSender,
        audio: &mut AudioEngine,
        player_pos: DVec3,
        on_ground: bool,
        creative: bool,
        held_stack: Option<&ItemStackData>,
        effects: &mut BreakEffects,
        dirty_chunks: &mut Vec<BlockPos>,
    ) {
        if self.miss_time > 0 {
            return;
        }

        // Vanilla `continueAttack` only mines blocks; holding the button over
        // an entity does not re-attack it.
        let Some(HitResult::Block(hit)) = self.target else {
            self.stop_destroying(sender);
            return;
        };

        let state = chunks.get_block_state(hit.block_pos.x, hit.block_pos.y, hit.block_pos.z);
        if is_air(state) {
            self.stop_destroying(sender);
            return;
        }

        if self.continue_destroy_block(
            hit,
            chunks,
            sender,
            audio,
            player_pos,
            on_ground,
            creative,
            held_stack,
            effects,
            dirty_chunks,
        ) {
            let current_state =
                chunks.get_block_state(hit.block_pos.x, hit.block_pos.y, hit.block_pos.z);
            effects.particles.add_breaking_block_effect(
                hit.block_pos,
                current_state,
                hit.face,
                effects.registry,
                chunks,
                effects.biome_climate,
            );
        }
        self.swing(sender);
    }

    /// Vanilla `Minecraft.startUseItem`: the block interaction goes first,
    /// falling through to `use_item` when nothing on the block consumed the
    /// click. Returns `true` if a use interaction was sent.
    #[allow(clippy::too_many_arguments)]
    fn start_use_item(
        &mut self,
        sender: &PacketSender,
        audio: &mut AudioEngine,
        chunks: &ChunkStore,
        player_pos: DVec3,
        _player_aabb: Aabb,
        eye_pos: DVec3,
        look: LookDirection,
        place_block: Option<BlockState>,
        offhand_place_block: Option<BlockState>,
        held_stack: Option<&ItemStackData>,
        food: u32,
        creative: bool,
        spectator: bool,
        entities: &EntityStore,
        hand: InteractionHand,
        hand_on_cooldown: bool,
        offhand_stack: Option<&ItemStackData>,
        offhand_on_cooldown: bool,
        has_projectile: bool,
        sneaking: bool,
        suppress_block_use: bool,
        effects: &mut BreakEffects,
        dirty_chunks: &mut Vec<BlockPos>,
    ) -> bool {
        if self.is_destroying {
            return false;
        }

        self.use_delay = USE_DELAY;

        // The server result is asynchronous; only known ordinary cows are
        // classified as PASS. Unknown and interactive entities stay conservative.
        if let Some(HitResult::Entity(hit)) = self.target {
            sender.send_raw(wire::encode_interact(
                hit.entity_id,
                protocol_hand(hand),
                hit.location - hit.entity_pos,
                sneaking,
            ));
            // Vanilla runs mobInteract locally before the server result arrives.
            // Mirror only the exact client-side particle branch; never consume or
            // mutate the player's inventory here.
            if !spectator
                && let Some(stack) = held_stack.filter(|stack| stack.count > 0)
                && let Some(action) = animal_particle_interaction(
                    entities,
                    hit.entity_id,
                    stack,
                    &self.item_tags,
                    self.thundering,
                )
            {
                self.animal_particle_interactions.push(action);
            }
            // Spectator entity interaction is packet-only; no local item use.
            if spectator || !entity_interaction_passes(entities, hit.entity_id, held_stack) {
                if !spectator {
                    self.swing_use(sender, hand);
                }
                return true;
            }
        }

        let hit_block = if let Some(HitResult::Block(hit)) = self.target {
            let target_state =
                chunks.get_block_state(hit.block_pos.x, hit.block_pos.y, hit.block_pos.z);
            let target_id = crate::world::block::block_id(target_state);
            let target_id = target_id.strip_prefix("minecraft:").unwrap_or(target_id);
            let command_block = matches!(
                target_id,
                "command_block" | "chain_command_block" | "repeating_command_block"
            );
            if command_block && held_stack.is_none() {
                if self.pending_command_block.is_none_or(|(pos, started)| {
                    pos != hit.block_pos || started.elapsed() > std::time::Duration::from_secs(2)
                }) {
                    self.pending_command_block = Some((hit.block_pos, std::time::Instant::now()));
                    self.seq += 1;
                    sender.send(ServerboundGamePacket::UseItemOn(ServerboundUseItemOn {
                        hand,
                        block_hit: BlockHit {
                            block_pos: hit.block_pos,
                            direction: hit.face,
                            location: azalea_vec3(hit.hit_point),
                            inside: hit.inside,
                            world_border: hit.world_border,
                        },
                        seq: self.seq,
                    }));
                }
                return true;
            }
            if command_block
                && self.pending_command_block.is_none_or(|(pos, started)| {
                    pos != hit.block_pos || started.elapsed() > std::time::Duration::from_secs(2)
                })
            {
                self.pending_command_block = Some((hit.block_pos, std::time::Instant::now()));
            } else if !command_block {
                self.pending_command_block = None;
            }
            self.seq += 1;
            sender.send(ServerboundGamePacket::UseItemOn(ServerboundUseItemOn {
                hand,
                block_hit: BlockHit {
                    block_pos: hit.block_pos,
                    direction: hit.face,
                    location: azalea_vec3(hit.hit_point),
                    inside: hit.inside,
                    world_border: hit.world_border,
                },
                seq: self.seq,
            }));
            // Blocks whose vanilla useWithoutItem succeeds consume the click
            // unless sneaking with something in hand.
            // Spectator block use is packet-only so the server can open/observe
            // containers; never predict local placement or item use.
            if spectator {
                return true;
            }
            let target = chunks.get_block_state(hit.block_pos.x, hit.block_pos.y, hit.block_pos.z);
            let target_id = crate::world::block::block_id(target);
            if !suppress_block_use
                && held_stack.is_some_and(|stack| stack.kind == ItemKind::WaterBucket)
                && matches!(
                    target_id,
                    "cauldron" | "water_cauldron" | "lava_cauldron" | "powder_snow_cauldron"
                )
            {
                // CauldronInteractions maps WATER_BUCKET to a successful
                // block use on each cauldron variant; BucketItem.use is skipped.
                return true;
            }
            if !suppress_block_use && hand == InteractionHand::MainHand && target_id == "dragon_egg"
            {
                self.spawn_dragon_egg_particles(hit.block_pos, eye_pos, chunks, effects);
                return true;
            }
            if !suppress_block_use
                && !hit.world_border
                && matches!(target_id, "redstone_ore" | "deepslate_redstone_ore")
            {
                spawn_interaction_particle_requests(
                    crate::world::particle_tick::redstone_ore_interaction_requests(
                        chunks,
                        hit.block_pos,
                        (self.seq as u64).wrapping_add(0x7265_6473_746f_6e65),
                    ),
                    eye_pos,
                    chunks,
                    effects,
                );
                // RedStoneOreBlock.useItemOn consumes non-BlockItems, including
                // buckets, but passes a BlockItem through when placement is valid.
                let placement_possible = place_block.is_some_and(|predicted| {
                    placement_target(hit, crate::world::block::block_id(predicted), chunks)
                        .is_some()
                });
                if !placement_possible {
                    return true;
                }
            }
            if !suppress_block_use {
                let requests = candle_extinguish_requests(
                    target,
                    hit.block_pos,
                    hit.hit_point.y,
                    held_stack.is_none_or(|stack| stack.count <= 0),
                    self.may_build,
                );
                if !requests.is_empty() {
                    let mut properties: Vec<_> = crate::world::block::block_properties(target)
                        .entries()
                        .map(|(key, value)| (key.to_owned(), value.to_owned()))
                        .collect();
                    if let Some((_, value)) = properties.iter_mut().find(|(key, _)| key == "lit") {
                        *value = "false".to_owned();
                    }
                    let predicted =
                        crate::world::block::state_with_properties(target_id, &properties)
                            .unwrap_or(target);
                    self.retain_known_server_state(hit.block_pos, target, player_pos);
                    self.visual_edits.push((hit.block_pos, target));
                    chunks.set_block_state(
                        hit.block_pos.x,
                        hit.block_pos.y,
                        hit.block_pos.z,
                        predicted,
                    );
                    mark_dirty(&hit.block_pos, dirty_chunks);
                    spawn_interaction_particle_requests(requests, eye_pos, chunks, effects);
                    return true;
                }
                if opens_menu(target) {
                    return true;
                }
            }
            if let Some(result) = self.try_place_block(
                hit,
                place_block,
                held_stack,
                hand,
                creative,
                sender,
                chunks,
                player_pos,
                look,
                dirty_chunks,
            ) {
                return result;
            }
            let destination = water_bucket_destination(hit, chunks, creative, sneaking);
            if held_stack.is_some_and(|stack| stack.kind == ItemKind::WaterBucket)
                && self.water_evaporates
                && let Some(destination) = destination
            {
                let pos = dvec3(
                    destination.x as f64,
                    destination.y as f64,
                    destination.z as f64,
                );
                let (kind, options) = bucket_evaporation_particle();
                for _ in 0..8 {
                    effects.particles.add_particles_from_packet(
                        kind,
                        options.clone(),
                        false,
                        false,
                        dvec3(
                            pos.x + fastrand::f64(),
                            pos.y + fastrand::f64(),
                            pos.z + fastrand::f64(),
                        ),
                        dvec3(0.0, 0.0, 0.0),
                        1.0,
                        0,
                        eye_pos,
                        effects.registry,
                        chunks,
                        effects.biome_climate,
                    );
                }
            }
            if held_stack.is_some_and(|stack| stack.kind == ItemKind::Brush) {
                self.brush_use = Some((hand, 0));
            }
            held_stack.is_some()
        } else {
            false
        };

        // A spectator's air/entity interaction never predicts item use.
        if spectator {
            return false;
        }

        // A non-block item passes the block interaction, so vanilla falls
        // through to `useItem` (this is how eating at the ground works).
        let used = self.use_item(
            sender,
            audio,
            chunks,
            player_pos,
            eye_pos,
            look,
            held_stack,
            food,
            creative,
            hand,
            hand_on_cooldown,
            has_projectile,
            effects,
        );
        // `use_item` may send a packet and still PASS locally (e.g. a stick);
        // Air item-use FAIL also falls through; only SUCCESS stops the loop.
        if used == ItemUseResult::Success {
            if matches!(self.target, Some(HitResult::Entity(_))) {
                self.swing_use(sender, hand);
            }
            return true;
        }

        // Vanilla checks the offhand after the main-hand interaction passes.
        // Entity interactions stay conservative because their PASS result is
        // server-authoritative here; block PASS is approximated above.
        if should_try_offhand(
            self.target.is_none(),
            !hit_block || matches!(self.target, Some(HitResult::Block(_))),
            used != ItemUseResult::Success,
            offhand_stack.is_some(),
        ) {
            if let Some(HitResult::Block(hit)) = self.target {
                self.seq += 1;
                sender.send(ServerboundGamePacket::UseItemOn(ServerboundUseItemOn {
                    hand: InteractionHand::OffHand,
                    block_hit: BlockHit {
                        block_pos: hit.block_pos,
                        direction: hit.face,
                        location: azalea_vec3(hit.hit_point),
                        inside: hit.inside,
                        world_border: hit.world_border,
                    },
                    seq: self.seq,
                }));
                if let Some(result) = self.try_place_block(
                    hit,
                    offhand_place_block,
                    offhand_stack,
                    InteractionHand::OffHand,
                    creative,
                    sender,
                    chunks,
                    player_pos,
                    look,
                    dirty_chunks,
                ) {
                    return result;
                }
            }
            return self.use_item(
                sender,
                audio,
                chunks,
                player_pos,
                eye_pos,
                look,
                offhand_stack,
                food,
                creative,
                InteractionHand::OffHand,
                offhand_on_cooldown,
                has_projectile,
                effects,
            ) == ItemUseResult::Success;
        }
        hit_block
    }

    pub fn water_bucket_destination(
        &self,
        chunks: &ChunkStore,
        creative: bool,
        sneaking: bool,
    ) -> Option<BlockPos> {
        let HitResult::Block(hit) = self.target? else {
            return None;
        };
        water_bucket_destination(hit, chunks, creative, sneaking)
    }

    pub fn set_water_evaporates(&mut self, water_evaporates: bool) {
        self.water_evaporates = water_evaporates;
    }

    pub fn set_main_hand_right(&mut self, main_hand_right: bool) {
        self.main_hand_right = main_hand_right;
    }

    pub fn take_writable_book_open(&mut self) -> Option<InteractionHand> {
        self.pending_writable_book.take()
    }

    /// Vanilla `MultiPlayerGameMode.useItem` + `Consumable.startConsuming`:
    /// sends `ServerboundUseItem` for any held item (the server decides what
    /// it does; pearls and snowballs work through this too) and begins the
    /// local use timer when the item is consumable and edible right now.
    #[allow(clippy::too_many_arguments)]
    fn use_item(
        &mut self,
        sender: &PacketSender,
        audio: &mut AudioEngine,
        chunks: &ChunkStore,
        player_pos: DVec3,
        eye_pos: DVec3,
        look: LookDirection,
        held_stack: Option<&ItemStackData>,
        food: u32,
        creative: bool,
        hand: InteractionHand,
        hand_on_cooldown: bool,
        has_projectile: bool,
        effects: &mut BreakEffects,
    ) -> ItemUseResult {
        let Some(stack) = held_stack else {
            return ItemUseResult::Pass;
        };

        self.seq += 1;
        sender.send(ServerboundGamePacket::UseItem(ServerboundUseItem {
            hand,
            seq: self.seq,
            y_rot: look.y_rot_deg(),
            x_rot: look.x_rot_deg(),
        }));

        if hand_on_cooldown {
            return ItemUseResult::Pass;
        }
        // LocalPlayer.openItemGui opens writable books immediately; the server
        // sends OpenBook only for written books with content.
        if stack.kind == ItemKind::WritableBook
            && crate::player::menu_click::component::<
                azalea_inventory::components::WritableBookContent,
            >(stack)
            .is_some()
        {
            self.pending_writable_book = Some(hand);
        }
        if stack.kind == ItemKind::Bow && !creative && !has_projectile {
            return ItemUseResult::Fail;
        }

        let special_use = match stack.kind {
            ItemKind::Bow => Some((ActiveUseKind::Bow, ItemUseAnimation::Bow, 72_000)),
            ItemKind::Crossbow => {
                let charged = crate::player::menu_click::component::<ChargedProjectiles>(stack)
                    .is_some_and(|projectiles| !projectiles.items.is_empty());
                Some((
                    if charged {
                        ActiveUseKind::CrossbowFire
                    } else {
                        ActiveUseKind::CrossbowCharge
                    },
                    ItemUseAnimation::Crossbow,
                    if charged { 0 } else { 25 },
                ))
            }
            ItemKind::Trident => Some((ActiveUseKind::Trident, ItemUseAnimation::Spear, 72_000)),
            ItemKind::Spyglass => {
                Some((ActiveUseKind::Spyglass, ItemUseAnimation::Spyglass, 72_000))
            }
            ItemKind::Shield => Some((ActiveUseKind::Shield, ItemUseAnimation::BlockKind, 72_000)),
            _ => None,
        };
        if let Some((use_kind, anim, duration)) = special_use {
            self.using_bow = use_kind == ActiveUseKind::Bow;
            self.use_latch = Some((hand, stack.kind));
            self.using_item = Some(ActiveUse {
                hand,
                kind: stack.kind,
                use_kind,
                anim,
                bow: use_kind == ActiveUseKind::Bow,
                sound: SoundRef::event("entity.generic.eat"),
                has_particles: false,
                use_effects: crate::player::menu_click::component::<UseEffects>(stack)
                    .unwrap_or_default(),
                duration,
                remaining: duration,
            });
            return ItemUseResult::Success;
        }

        let Some(consumable) = crate::player::menu_click::component::<Consumable>(stack) else {
            return if main_hand_use_succeeds(stack) {
                if stack.kind == ItemKind::EnderPearl {
                    self.swing_use(sender, hand);
                }
                ItemUseResult::Success
            } else {
                ItemUseResult::Pass
            };
        };
        // Vanilla `Consumable.canConsume` → `Player.canEat`: food needs
        // hunger unless it can always be eaten; creative players (vanilla
        // invulnerable) always can. Non-food consumables have no gate.
        if let Some(f) = crate::player::menu_click::component::<Food>(stack)
            && !(creative || f.can_always_eat || food < MAX_FOOD_LEVEL)
        {
            return ItemUseResult::Fail;
        }

        let duration = (consumable.consume_seconds * 20.0) as i32;
        let active = ActiveUse {
            hand,
            kind: stack.kind,
            use_kind: ActiveUseKind::Consumable,
            anim: consumable.animation,
            bow: false,
            sound: SoundRef::resolve(&consumable.sound),
            has_particles: consumable.has_consume_particles,
            use_effects: crate::player::menu_click::component::<UseEffects>(stack)
                .unwrap_or_default(),
            duration,
            remaining: duration,
        };
        if duration > 0 {
            self.using_item = Some(active);
        } else {
            // Vanilla `Consumable.startConsuming`: a zero-duration consumable
            // skips the use timer and consumes on the spot (`onConsume`).
            emit_consume_effects(
                &active,
                16,
                audio,
                effects.particles,
                effects.registry,
                chunks,
                player_pos,
                eye_pos,
                look,
            );
        }
        ItemUseResult::Success
    }

    /// A respawn constructs a fresh LocalPlayer in vanilla. Reset only the
    /// transient player-owned animation/use state that Pomme keeps inside the
    /// longer-lived interaction controller; block prediction/sequences remain.
    pub fn reset_player_transients_for_respawn(&mut self) {
        self.using_item = None;
        self.brush_use = None;
        self.use_latch = None;
        self.using_bow = false;
        self.swinging = false;
        self.swing_time = 0;
        self.attack_anim = 0.0;
        self.o_attack_anim = 0.0;
        self.swinging_hand = InteractionHand::MainHand;
        self.hand_height = [0.0; 2];
        self.o_hand_height = [0.0; 2];
        self.hand_view_bob = glam::Vec2::ZERO;
        self.o_hand_view_bob = glam::Vec2::ZERO;
        self.pending_placement_uses = [None; 2];
        self.attack_strength_ticker = 0;
        self.last_item_in_main_hand = None;
    }

    /// Client `LivingEntity.onSyncedDataUpdated(DATA_LIVING_ENTITY_FLAGS)`:
    /// when the server clears the using-item bit, discard the local use state
    /// immediately.
    pub fn sync_using_item_flag(&mut self, is_using: bool) {
        // TODO: vanilla also starts a use when the bit turns on with none
        // active (a server-initiated use); pomme only starts uses from its own
        // UseItem send.
        if !is_using {
            self.using_item = None;
            self.brush_use = None;
            self.using_bow = false;
        }
    }

    /// Dead-player `LivingEntity.tick` heartbeat that still runs before the
    /// removed check around `aiStep`. This deliberately excludes keybind and
    /// block-interaction handling: only an already-active item use advances.
    #[allow(clippy::too_many_arguments)]
    pub fn tick_dead_living_state(
        &mut self,
        held_stack: Option<&ItemStackData>,
        offhand_stack: Option<&ItemStackData>,
        audio: &mut AudioEngine,
        chunks: &ChunkStore,
        player_pos: DVec3,
        eye_pos: DVec3,
        look: LookDirection,
        effects: &mut BreakEffects,
    ) {
        self.update_using_item(
            held_stack,
            offhand_stack,
            audio,
            chunks,
            player_pos,
            eye_pos,
            look,
            effects,
        );
    }

    /// Remaining dead-player `Player.tick` state. Swing animation belongs to
    /// `Player.aiStep` and therefore stops on the tick-20 removal tick, while
    /// attack-strength ticking happens after `super.tick` and still advances
    /// once on that final local-player tick.
    pub fn tick_dead_player_state(
        &mut self,
        held_stack: Option<&ItemStackData>,
        advance_swing: bool,
    ) {
        if advance_swing {
            self.update_swing();
        }
        self.tick_attack_cooldown(held_stack);
    }

    /// Per-tick item-use heartbeat, vanilla `LivingEntity.updatingUsingItem`
    /// / `updateUsingItem`: stop silently if the held stack changed, emit the
    /// periodic bite sound/particles, count the timer down. Completion is
    /// server-authoritative (entity event 9 → `complete_using`); the timer
    /// just runs negative until the server acts.
    #[allow(clippy::too_many_arguments)]
    fn update_using_item(
        &mut self,
        held_stack: Option<&ItemStackData>,
        offhand_stack: Option<&ItemStackData>,
        audio: &mut AudioEngine,
        chunks: &ChunkStore,
        player_pos: DVec3,
        eye_pos: DVec3,
        look: LookDirection,
        effects: &mut BreakEffects,
    ) {
        let Some(active) = &self.using_item else {
            return;
        };
        let active_stack = stack_for_hand(active.hand, held_stack, offhand_stack);
        if active_stack.map(|s| s.kind) != Some(active.kind) {
            self.using_item = None;
            return;
        }
        // `Consumable.shouldEmitParticlesAndSounds`.
        let elapsed = active.duration - active.remaining;
        let wait = (active.duration as f32 * CONSUME_EFFECTS_START_FRACTION) as i32;
        if active.use_kind == ActiveUseKind::Consumable
            && elapsed > wait
            && active.remaining % CONSUME_EFFECTS_INTERVAL == 0
        {
            emit_consume_effects(
                active,
                5,
                audio,
                effects.particles,
                effects.registry,
                chunks,
                player_pos,
                eye_pos,
                look,
            );
        }
        if let Some(active) = &mut self.using_item {
            active.remaining -= 1;
        }
    }

    /// Vanilla `MultiPlayerGameMode.releaseUsingItem`: an early release just
    /// cancels a consume; nothing finishes on release for food.
    fn release_using_item(&mut self, sender: &PacketSender) {
        // A loaded crossbow fires immediately on UseItem and never enters the
        // server's item-use lifecycle, so only release the local hold latch.
        if !self
            .using_item
            .as_ref()
            .is_some_and(|active| active.use_kind == ActiveUseKind::CrossbowFire)
        {
            send_action(
                sender,
                Action::ReleaseUseItem,
                BlockPos { x: 0, y: 0, z: 0 },
                Direction::Down,
                0,
            );
        }
        self.using_item = None;
        self.use_latch = None;
        self.using_bow = false;
    }

    /// Client `LivingEntity.completeUsingItem` (entity event 9) →
    /// `Consumable.onConsume`: the final 16-crumb burst plus one more consume
    /// sound. Food, saturation, the burp, and the shrunk stack all arrive as
    /// separate server packets.
    pub fn complete_using(
        &mut self,
        audio: &mut AudioEngine,
        particles: &mut ParticleStore,
        registry: &BlockRegistry,
        chunks: &ChunkStore,
        player_pos: DVec3,
        eye_pos: DVec3,
        look: LookDirection,
    ) {
        let Some(active) = self.using_item.take() else {
            return;
        };
        if active.use_kind != ActiveUseKind::Consumable {
            return;
        }
        emit_consume_effects(
            &active, 16, audio, particles, registry, chunks, player_pos, eye_pos, look,
        );
    }

    /// Vanilla `LocalPlayer.itemUseSpeedMultiplier`: the in-use item's
    /// `UseEffects` movement-input scale (1.0 when nothing is in use).
    pub fn use_speed_multiplier(&self) -> f32 {
        self.using_item
            .as_ref()
            .map_or(1.0, |a| a.use_effects.speed_multiplier)
    }

    /// Vanilla `LocalPlayer.isSlowDueToUsingItem`, which gates sprinting.
    pub fn slow_due_to_using_item(&self) -> bool {
        self.using_item
            .as_ref()
            .is_some_and(|a| !a.use_effects.can_sprint)
    }

    /// Whether a spyglass use is active. FOV/HUD can consume this without
    /// depending on item packet or inventory prediction details.
    pub fn is_using_spyglass(&self) -> bool {
        self.using_item
            .as_ref()
            .is_some_and(|active| active.use_kind == ActiveUseKind::Spyglass)
    }

    /// First-person use-animation state for the held-item renderer, vanilla
    /// `ItemInHandRenderer.applyEatTransform` inputs. `None` unless an
    /// eat/drink use is active with ticks remaining; the hand is preserved.
    pub fn use_animation(&self, partial_tick: f32) -> Option<UseAnim> {
        let active = self.using_item.as_ref()?;
        if active.remaining <= 0
            || (!active.bow
                && active.use_kind != ActiveUseKind::Shield
                && !matches!(active.anim, ItemUseAnimation::Eat | ItemUseAnimation::Drink))
        {
            return None;
        }
        Some(UseAnim {
            curr_usage_time: active.remaining as f32 - partial_tick + 1.0,
            duration: active.duration as f32,
            left_hand: active.hand == InteractionHand::OffHand,
            bow: active.bow,
            shield_blocking: active.use_kind == ActiveUseKind::Shield,
        })
    }

    /// BlockItems optimistically succeed; only the server validates placement.
    /// None means an empty/non-block item, which may PASS.
    #[allow(clippy::too_many_arguments)]
    fn try_place_block(
        &mut self,
        hit: BlockHitResult,
        prediction: Option<BlockState>,
        stack: Option<&ItemStackData>,
        hand: InteractionHand,
        creative: bool,
        sender: &PacketSender,
        chunks: &ChunkStore,
        player_pos: DVec3,
        look: LookDirection,
        dirty_chunks: &mut Vec<BlockPos>,
    ) -> Option<bool> {
        let stack = stack.filter(|stack| stack.count > 0)?;
        let item_name = item_resource_name(stack.kind);
        let block_name = crate::world::block::registry::block_for_item(&item_name)?;
        if let Some((pos, previous)) = placement_target(hit, block_name, chunks) {
            let prediction = prediction
                .or_else(|| crate::world::block::default_state_of(block_name))
                .map(|state| placement_state(state, hit.face, look));
            if let Some(state) = prediction {
                self.retain_known_server_state(pos, previous, player_pos);
                self.visual_edits.push((pos, previous));
                chunks.set_block_state(pos.x, pos.y, pos.z, state);
                sender
                    .recorder
                    .local_prediction(pos, previous, state, self.seq);
                mark_dirty(&pos, dirty_chunks);
            }
            self.placement_used(hand, stack, creative, pos, previous, prediction);
        } else if creative {
            // Unloaded/out-of-height cells have no local state to retain or invent.
            self.item_used(hand);
        }
        self.swing_use(sender, hand);
        Some(true)
    }

    #[allow(clippy::too_many_arguments)]
    fn start_destroy_block(
        &mut self,
        hit: BlockHitResult,
        chunks: &ChunkStore,
        sender: &PacketSender,
        audio: &mut AudioEngine,
        player_pos: DVec3,
        on_ground: bool,
        creative: bool,
        held_stack: Option<&ItemStackData>,
        effects: &mut BreakEffects,
        dirty_chunks: &mut Vec<BlockPos>,
    ) {
        let state = chunks.get_block_state(hit.block_pos.x, hit.block_pos.y, hit.block_pos.z);

        if is_air(state) {
            return;
        }

        let progress =
            destroy_progress(state, on_ground, creative, held_stack, self.mining_context);

        if progress >= 1.0 {
            if self.is_destroying {
                send_action(
                    sender,
                    Action::AbortDestroyBlock,
                    self.destroy_pos,
                    Direction::Down,
                    0,
                );
                self.is_destroying = false;
            }
            self.seq += 1;
            let seq = self.seq;
            send_action(
                sender,
                Action::StartDestroyBlock,
                hit.block_pos,
                hit.face,
                seq,
            );
            self.predict_destroy(
                hit.block_pos,
                state,
                player_pos,
                chunks,
                audio,
                effects,
                dirty_chunks,
                &sender.recorder,
            );
            if creative {
                self.destroy_delay = DESTROY_COOLDOWN;
            }
            return;
        }

        if self.is_destroying && self.same_destroy_target(hit.block_pos, held_stack) {
            return;
        }

        if self.is_destroying {
            send_action(
                sender,
                Action::AbortDestroyBlock,
                self.destroy_pos,
                hit.face,
                0,
            );
        }

        self.seq += 1;
        let seq = self.seq;
        send_action(
            sender,
            Action::StartDestroyBlock,
            hit.block_pos,
            hit.face,
            seq,
        );

        self.is_destroying = true;
        self.destroy_pos = hit.block_pos;
        self.destroying_item = held_stack.cloned();
        self.destroy_progress = 0.0;
        self.destroy_ticks = 0.0;
    }

    #[allow(clippy::too_many_arguments)]
    fn continue_destroy_block(
        &mut self,
        hit: BlockHitResult,
        chunks: &ChunkStore,
        sender: &PacketSender,
        audio: &mut AudioEngine,
        player_pos: DVec3,
        on_ground: bool,
        creative: bool,
        held_stack: Option<&ItemStackData>,
        effects: &mut BreakEffects,
        dirty_chunks: &mut Vec<BlockPos>,
    ) -> bool {
        if self.destroy_delay > 0 {
            self.destroy_delay -= 1;
            return true;
        }

        if !self.same_destroy_target(hit.block_pos, held_stack) {
            self.start_destroy_block(
                hit,
                chunks,
                sender,
                audio,
                player_pos,
                on_ground,
                creative,
                held_stack,
                effects,
                dirty_chunks,
            );
            return true;
        }

        let state = chunks.get_block_state(hit.block_pos.x, hit.block_pos.y, hit.block_pos.z);
        if is_air(state) {
            self.is_destroying = false;
            return false;
        }

        self.destroy_progress +=
            destroy_progress(state, on_ground, creative, held_stack, self.mining_context);
        if self.destroy_ticks % 4.0 == 0.0 {
            play_hit_sound(audio, state, hit.block_pos);
        }
        self.destroy_ticks += 1.0;

        if self.destroy_progress >= 1.0 {
            self.seq += 1;
            let seq = self.seq;
            send_action(
                sender,
                Action::StopDestroyBlock,
                hit.block_pos,
                hit.face,
                seq,
            );
            self.predict_destroy(
                hit.block_pos,
                state,
                player_pos,
                chunks,
                audio,
                effects,
                dirty_chunks,
                &sender.recorder,
            );
            self.destroy_delay = DESTROY_COOLDOWN;
            self.is_destroying = false;
            self.destroy_progress = 0.0;
            self.destroy_ticks = 0.0;
        }
        true
    }

    /// Ports vanilla `MultiPlayerGameMode.ensureHasSentCarriedItem`: tell the
    /// server which hotbar slot is selected whenever it changes, so it resolves
    /// interactions against the item we're actually holding.
    fn ensure_has_sent_carried_item(&mut self, sender: &PacketSender, selected_slot: u8) {
        if selected_slot != self.carried_slot {
            self.carried_slot = selected_slot;
            sender.send(ServerboundGamePacket::SetCarriedItem(
                ServerboundSetCarriedItem {
                    slot: selected_slot as u16,
                },
            ));
        }
    }

    /// Vanilla `MultiPlayerGameMode.sameDestroyTarget`: still mining the same
    /// block with the same item.
    fn same_destroy_target(&self, pos: BlockPos, held: Option<&ItemStackData>) -> bool {
        self.destroy_pos == pos && same_item_same_components(held, self.destroying_item.as_ref())
    }

    pub fn stop_destroying_for_screen(&mut self, sender: &PacketSender) {
        self.miss_time = 0;
        self.stop_destroying(sender);
    }

    fn clear_destroying_state(&mut self) {
        self.is_destroying = false;
        self.destroy_progress = 0.0;
        self.destroy_ticks = 0.0;
        self.destroying_item = None;
    }

    fn stop_destroying(&mut self, sender: &PacketSender) {
        if self.is_destroying {
            send_action(
                sender,
                Action::AbortDestroyBlock,
                self.destroy_pos,
                Direction::Down,
                0,
            );
            self.is_destroying = false;
            self.destroy_progress = 0.0;
            // Vanilla `MultiPlayerGameMode.stopDestroyBlock`.
            self.attack_strength_ticker = 0;
        }
    }
}

/// The player's attack speed with the given main-hand item: base 4.0 plus the
/// item's `AttributeModifiers` component, folded like vanilla
/// `AttributeInstance.calculateValue`. Computed locally like vanilla's client;
/// the server's `UpdateAttributes` snapshot is deliberately not used (it
/// already bakes in the held item's modifier and lags item switches).
pub fn attack_speed(held: Option<&ItemStackData>) -> f64 {
    let base = 4.0f64;
    let mut add = 0.0f64;
    let mut mul_base = 0.0f64;
    let mut mul_total = 1.0f64;
    if let Some(stack) = held
        && let Some(mods) = crate::player::menu_click::component::<AttributeModifiers>(stack)
    {
        for entry in &mods.modifiers {
            if entry.kind != Attribute::AttackSpeed
                || !matches!(
                    entry.slot,
                    EquipmentSlotGroup::Mainhand
                        | EquipmentSlotGroup::Hand
                        | EquipmentSlotGroup::Any
                )
            {
                continue;
            }
            match entry.modifier.operation {
                AttributeModifierOperation::AddValue => add += entry.modifier.amount,
                AttributeModifierOperation::AddMultipliedBase => mul_base += entry.modifier.amount,
                AttributeModifierOperation::AddMultipliedTotal => {
                    mul_total *= 1.0 + entry.modifier.amount
                }
            }
        }
    }
    // Vanilla `RangedAttribute` ATTACK_SPEED bounds.
    ((base + add) * (1.0 + mul_base) * mul_total).clamp(0.0, 1024.0)
}

/// Vanilla `Player.getCurrentItemAttackStrengthDelay`, in ticks.
pub fn attack_strength_delay(held: Option<&ItemStackData>) -> f32 {
    let speed = attack_speed(held);
    if speed <= 0.0 {
        f32::INFINITY
    } else {
        (1.0 / speed * 20.0) as f32
    }
}

/// Vanilla `ItemStack.isSameItemSameComponents`: item type and components,
/// never the count. `None` is the empty hand.
fn same_item_same_components(a: Option<&ItemStackData>, b: Option<&ItemStackData>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.is_same_item_and_components(b),
        _ => false,
    }
}

/// Whether known vanilla block-use behavior consumes the click, so we do not
/// locally fall through to block placement or item use.
fn opens_menu(state: BlockState) -> bool {
    let id = crate::world::block::block_id(state);
    matches!(
        id,
        "crafting_table"
            | "brewing_stand"
            | "furnace"
            | "blast_furnace"
            | "smoker"
            | "chest"
            | "trapped_chest"
            | "ender_chest"
            | "barrel"
            | "hopper"
    ) || id.ends_with("shulker_box")
        || id.ends_with("anvil")
}

/// Vanilla `BlockBehaviour.getDestroyProgress` with `Player.getDestroySpeed`
/// as the numerator: the held tool's mining speed over hardness, divided by 30
/// with the correct tool for drops and 100 without.
fn destroy_progress(
    state: BlockState,
    on_ground: bool,
    creative: bool,
    held_stack: Option<&ItemStackData>,
    mining: MiningContext,
) -> f32 {
    if creative {
        return 1.0;
    }
    let behavior = crate::world::block::block_behavior(state);
    let hardness = behavior.destroy_time;

    if hardness < 0.0 {
        return 0.0;
    }
    if hardness == 0.0 {
        return 1.0;
    }

    let tool = held_stack.and_then(crate::player::menu_click::component::<Tool>);
    let tool = tool.as_ref();
    // BlockState is a data-free shim in pomme-block; resolve the native block
    // name from the client registry instead of treating every state as Air.
    let kind = crate::world::block::block_id(state)
        .parse::<BlockKind>()
        .ok();

    let mut speed = tool.map_or(1.0, |t| {
        kind.map_or(t.default_mining_speed, |kind| tool_mining_speed(t, kind))
    });
    if speed > 1.0 {
        speed += mining.mining_efficiency;
    }
    if let Some(amplifier) = mining.haste_amplifier {
        speed *= 1.0 + (f32::from(amplifier) + 1.0) * 0.2;
    }
    if let Some(amplifier) = mining.fatigue_amplifier {
        speed *= match amplifier {
            0 => 0.3,
            1 => 0.09,
            2 => 0.0027,
            _ => 0.00081,
        };
    }
    speed *= mining.block_break_speed;
    if mining.eyes_in_water {
        speed *= mining.submerged_mining_speed;
    }
    if !on_ground {
        speed /= 5.0;
    }

    let correct_tool = !behavior.requires_correct_tool_for_drops
        || tool.is_some_and(|t| kind.is_some_and(|kind| tool_correct_for_drops(t, kind)));
    let divisor = if correct_tool { 30.0 } else { 100.0 };
    speed / hardness / divisor
}

/// Vanilla `Tool.getMiningSpeed`: first rule with a speed that covers the
/// block wins, else the default.
fn tool_mining_speed(tool: &Tool, kind: BlockKind) -> f32 {
    first_rule_value(tool, kind, |r| r.speed).unwrap_or(tool.default_mining_speed)
}

/// Vanilla `Tool.isCorrectForDrops`: first rule with a verdict that covers
/// the block wins, else false.
fn tool_correct_for_drops(tool: &Tool, kind: BlockKind) -> bool {
    first_rule_value(tool, kind, |r| r.correct_for_drops).unwrap_or(false)
}

fn first_rule_value<T: Copy>(
    tool: &Tool,
    kind: BlockKind,
    field: impl Fn(&ToolRule) -> Option<T>,
) -> Option<T> {
    tool.rules
        .iter()
        .find_map(|rule| field(rule).filter(|_| rule.blocks.contains(kind)))
}

/// Plays a block's mining hit sound, matching vanilla
/// `MultiPlayerGameMode.continueDestroyBlock`: volume `(volume + 1) / 8`, pitch
/// `pitch * 0.5`.
fn play_hit_sound(audio: &mut AudioEngine, state: BlockState, pos: BlockPos) {
    let s = block_sounds(state);
    play_block_sound(
        audio,
        &s.hit_event,
        pos,
        (s.volume + 1.0) / 8.0,
        s.pitch * 0.5,
    );
}

/// Plays a block's break sound, matching vanilla `LevelEventHandler` event
/// 2001: volume `(volume + 1) / 2`, pitch `pitch * 0.8`.
pub fn play_break_sound(audio: &mut AudioEngine, state: BlockState, pos: BlockPos) {
    let s = block_sounds(state);
    play_block_sound(
        audio,
        &s.break_event,
        pos,
        (s.volume + 1.0) / 2.0,
        s.pitch * 0.8,
    );
}

/// Vanilla `Consumable.emitParticlesAndSounds`: the shared bite / final-gulp
/// burst of item crumbs plus the consume sound. The sound plays locally here
/// and again from the server's broadcast, doubling up for the eater exactly
/// like vanilla (MC-98310).
#[allow(clippy::too_many_arguments)]
fn emit_consume_effects(
    active: &ActiveUse,
    particle_count: u32,
    audio: &mut AudioEngine,
    particles: &mut ParticleStore,
    registry: &BlockRegistry,
    chunks: &ChunkStore,
    player_pos: DVec3,
    eye_pos: DVec3,
    look: LookDirection,
) {
    if active.has_particles {
        particles.add_item_use_particles(
            particle_count,
            active.kind,
            registry,
            eye_pos,
            look.x_rot_deg(),
            look.y_rot_deg(),
            chunks,
        );
    }
    let (volume, pitch) = if matches!(active.anim, ItemUseAnimation::Drink) {
        (0.5, 0.9 + fastrand::f32() * 0.1)
    } else {
        (
            if fastrand::bool() { 0.5 } else { 1.0 },
            1.0 + 0.2 * (fastrand::f32() - fastrand::f32()),
        )
    };
    // Observe requests before the silent test engine's empty sound index drops
    // them.
    #[cfg(test)]
    tests::CONSUME_SOUND_REQUESTS.with(|count| count.set(count.get() + 1));
    audio.play_world_sound(
        &active.sound,
        CATEGORY_PLAYERS,
        Position::new(player_pos.x, player_pos.y, player_pos.z),
        volume,
        pitch,
        fastrand::u64(..),
    );
}

/// Plays a block sound event at the block centre in the BLOCKS category with a
/// random variant. No-op for an empty event (a silent `SoundType` slot).
fn play_block_sound(audio: &mut AudioEngine, event: &str, pos: BlockPos, volume: f32, pitch: f32) {
    if event.is_empty() {
        return;
    }
    audio.play_world_sound(
        &SoundRef::event(event),
        CATEGORY_BLOCKS,
        Position::new(pos.x as f64 + 0.5, pos.y as f64 + 0.5, pos.z as f64 + 0.5),
        volume,
        pitch,
        fastrand::u64(..),
    );
}

fn animal_particle_interaction(
    entities: &EntityStore,
    entity_id: i32,
    stack: &ItemStackData,
    item_tags: &BTreeMap<String, Vec<ItemKind>>,
    thundering: bool,
) -> Option<AnimalParticleInteraction> {
    let entity = entities.living.get(&entity_id)?;
    if entity.health <= 0.0 {
        return None;
    }
    let kind = entity.entity_type;
    let item = stack.kind;
    let has_tag = |name: &str| {
        item_tags
            .get(&format!("minecraft:{name}"))
            .is_some_and(|items| items.contains(&item))
    };
    let baby_can_age = entity.is_baby && !entity.age_locked;

    if item == ItemKind::GoldenDandelion
        && entity.is_baby
        && crate::entity::is_ageable_mob(kind)
        && !matches!(
            kind,
            EntityKind::Villager | EntityKind::ZombieHorse | EntityKind::SkeletonHorse
        )
    {
        return Some(AnimalParticleInteraction::AgeLock(
            entity_id,
            !entity.age_locked,
        ));
    }
    if kind == EntityKind::Mooshroom
        && entity.variant == 1
        && !entity.is_baby
        && suspicious_stew_effect_item(item)
    {
        return Some(AnimalParticleInteraction::MooshroomStew(entity_id, item));
    }
    if matches!(kind, EntityKind::Camel | EntityKind::CamelHusk)
        && baby_can_age
        && has_tag(if kind == EntityKind::Camel {
            "camel_food"
        } else {
            "camel_husk_food"
        })
    {
        return Some(AnimalParticleInteraction::Immediate(
            entity_id,
            crate::entity::particle_animals::InteractionParticle::AgeUpHappyVillager,
        ));
    }
    if matches!(kind, EntityKind::Llama | EntityKind::TraderLlama)
        && baby_can_age
        && has_tag("llama_food")
    {
        return Some(AnimalParticleInteraction::Immediate(
            entity_id,
            crate::entity::particle_animals::InteractionParticle::AgeUpHappyVillager,
        ));
    }
    if kind == EntityKind::Dolphin && baby_can_age && has_tag("fishes") {
        return Some(AnimalParticleInteraction::ForcedAge(entity_id));
    }
    if kind == EntityKind::Tadpole && !entity.age_locked && has_tag("frog_food") {
        return Some(AnimalParticleInteraction::Immediate(
            entity_id,
            crate::entity::particle_animals::InteractionParticle::AgeUpHappyVillager,
        ));
    }
    if kind == EntityKind::Panda
        && baby_can_age
        && entity.panda_flags & 0x10 == 0
        && !(entity.variant == 2 && thundering)
        && has_tag("panda_food")
    {
        return Some(AnimalParticleInteraction::ForcedAge(entity_id));
    }
    let max_health = entity
        .attributes
        .get("minecraft:max_health")
        .map(|&health| health as f32)
        .unwrap_or(match kind {
            EntityKind::Wolf => 40.0,
            EntityKind::Cat | EntityKind::Ocelot => 10.0,
            _ => f32::INFINITY,
        });
    if entity.is_tame
        && entity.health < max_health
        && matches!(
            kind,
            EntityKind::Wolf | EntityKind::Cat | EntityKind::Ocelot
        )
    {
        return None;
    }
    if crate::entity::is_equine(&kind)
        && baby_can_age
        && matches!(
            item,
            ItemKind::Wheat
                | ItemKind::Sugar
                | ItemKind::HayBlock
                | ItemKind::Apple
                | ItemKind::Carrot
                | ItemKind::GoldenCarrot
                | ItemKind::GoldenApple
                | ItemKind::EnchantedGoldenApple
        )
    {
        return Some(AnimalParticleInteraction::Immediate(
            entity_id,
            crate::entity::particle_animals::InteractionParticle::AgeUpHappyVillager,
        ));
    }
    if baby_can_age
        && animal_food_tag(kind).is_some_and(has_tag)
        && crate::entity::is_ageable_mob(kind)
    {
        return Some(AnimalParticleInteraction::ForcedAge(entity_id));
    }
    None
}

fn animal_food_tag(kind: EntityKind) -> Option<&'static str> {
    Some(match kind {
        EntityKind::Pig => "pig_food",
        EntityKind::Cow | EntityKind::Mooshroom => "cow_food",
        EntityKind::Sheep => "sheep_food",
        EntityKind::Chicken => "chicken_food",
        EntityKind::Rabbit => "rabbit_food",
        EntityKind::Goat => "goat_food",
        EntityKind::Turtle => "turtle_food",
        EntityKind::Wolf => "wolf_food",
        EntityKind::Cat => "cat_food",
        EntityKind::Ocelot => "ocelot_food",
        EntityKind::Fox => "fox_food",
        EntityKind::Sniffer => "sniffer_food",
        EntityKind::Armadillo => "armadillo_food",
        EntityKind::Axolotl => "axolotl_food",
        _ => return None,
    })
}

fn suspicious_stew_effect_item(item: ItemKind) -> bool {
    matches!(
        item,
        ItemKind::Dandelion
            | ItemKind::GoldenDandelion
            | ItemKind::Torchflower
            | ItemKind::Poppy
            | ItemKind::BlueOrchid
            | ItemKind::Allium
            | ItemKind::AzureBluet
            | ItemKind::RedTulip
            | ItemKind::OrangeTulip
            | ItemKind::WhiteTulip
            | ItemKind::PinkTulip
            | ItemKind::OxeyeDaisy
            | ItemKind::Cornflower
            | ItemKind::WitherRose
            | ItemKind::LilyOfTheValley
    )
}

fn entity_interaction_passes(
    entities: &EntityStore,
    entity_id: i32,
    stack: Option<&ItemStackData>,
) -> bool {
    entity_passes_for_item(
        entities
            .living
            .get(&entity_id)
            .map(|entity| entity.entity_type),
        stack,
    )
}

fn entity_passes_for_item(entity: Option<EntityKind>, stack: Option<&ItemStackData>) -> bool {
    // Bow use follows Entity.interact's PASS for these ordinary entities.
    // Interactive entities (villagers, horses, armor stands, frames, etc.) and
    // unclassified entities remain conservative; stateful overrides aren't modeled.
    stack.is_some_and(|stack| stack.kind == ItemKind::Bow)
        && matches!(
            entity,
            Some(
                EntityKind::Cow
                    | EntityKind::Sheep
                    | EntityKind::Chicken
                    | EntityKind::Zombie
                    | EntityKind::Skeleton
                    | EntityKind::Player
            )
        )
}

fn should_try_offhand(
    target_is_air: bool,
    block_interaction_passed: bool,
    main_hand_passed: bool,
    offhand_nonempty: bool,
) -> bool {
    (target_is_air || block_interaction_passed) && main_hand_passed && offhand_nonempty
}

/// Local approximation of vanilla's client-side `useItem` result.
/// `UseEffects` is present on every item and `Tool` only controls mining;
/// neither starts a use in vanilla `Item.use` (a stick must PASS).
fn main_hand_use_succeeds(stack: &ItemStackData) -> bool {
    matches!(
        stack.kind,
        ItemKind::Bow | ItemKind::WritableBook | ItemKind::WrittenBook | ItemKind::EnderPearl
    ) || crate::player::menu_click::component::<Consumable>(stack).is_some()
        || crate::player::menu_click::component::<BlocksAttacks>(stack).is_some()
        || crate::player::menu_click::component::<KineticWeapon>(stack).is_some()
}

fn stack_for_hand<'a>(
    hand: InteractionHand,
    main_hand: Option<&'a ItemStackData>,
    off_hand: Option<&'a ItemStackData>,
) -> Option<&'a ItemStackData> {
    match hand {
        InteractionHand::MainHand => main_hand,
        InteractionHand::OffHand => off_hand,
    }
}

fn protocol_hand(hand: InteractionHand) -> wire::InteractionHand {
    match hand {
        InteractionHand::MainHand => wire::InteractionHand::MainHand,
        InteractionHand::OffHand => wire::InteractionHand::OffHand,
    }
}

/// Record an edited block. The caller (`core::dirty_sections_for_block`)
/// expands it into the affected 16³ sections, including neighbour
/// sections/columns when the block is on a boundary.
fn mark_dirty(pos: &BlockPos, dirty: &mut Vec<BlockPos>) {
    if !dirty.contains(pos) {
        dirty.push(*pos);
    }
}

/// Base canBeReplaced: do not merge slabs/piles or replace a block with its
/// own item. Snow's other-item override only permits a single layer (26.2).
fn can_replace_for_item(state: BlockState, block_name: &str) -> bool {
    let name = crate::world::block::block_id(state);
    if name == "snow" {
        return block_name != "snow"
            && crate::world::block::block_properties(state).get("layers") == Some("1");
    }
    crate::world::block::is_replaceable(state) && name != block_name
}

fn placement_target(
    hit: BlockHitResult,
    block_name: &str,
    chunks: &ChunkStore,
) -> Option<(BlockPos, BlockState)> {
    let clicked = chunks.get_block_state(hit.block_pos.x, hit.block_pos.y, hit.block_pos.z);
    let pos = if can_replace_for_item(clicked, block_name) {
        hit.block_pos
    } else {
        hit.block_pos.offset_with_direction(hit.face)
    };
    if pos.y < chunks.min_y()
        || pos.y >= chunks.min_y() + chunks.height() as i32
        || chunks
            .get_chunk(&azalea_core::position::ChunkPos::new(
                pos.x.div_euclid(16),
                pos.z.div_euclid(16),
            ))
            .is_none()
    {
        return None;
    }
    let previous = chunks.get_block_state(pos.x, pos.y, pos.z);
    Some((pos, previous))
}

fn placement_state(state: BlockState, face: Direction, look: LookDirection) -> BlockState {
    use crate::world::block::{
        block_id, block_properties, default_state_of, state_with_properties,
    };
    let name = block_id(state);
    let wall = match name {
        "torch" => Some("wall_torch".to_owned()),
        "soul_torch" => Some("soul_wall_torch".to_owned()),
        "redstone_torch" => Some("redstone_wall_torch".to_owned()),
        name if name.ends_with("_sign") && !name.contains("hanging") => {
            Some(name.replace("_sign", "_wall_sign"))
        }
        _ => None,
    };
    let horizontal_face = match face {
        Direction::North => Some("north"),
        Direction::South => Some("south"),
        Direction::West => Some("west"),
        Direction::East => Some("east"),
        _ => None,
    };
    let wall_state = horizontal_face.and_then(|_| wall.as_deref().and_then(default_state_of));
    let state = wall_state.unwrap_or(state);
    let mut props: Vec<_> = block_properties(state)
        .entries()
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect();
    let facing = ["north", "east", "south", "west"]
        [((look.y_rot_deg() / 90.0 + 0.5).floor() as i32).rem_euclid(4) as usize];
    for (key, value) in &mut props {
        match key.as_str() {
            "axis" => {
                *value = match face {
                    Direction::West | Direction::East => "x",
                    Direction::North | Direction::South => "z",
                    _ => "y",
                }
                .to_owned()
            }
            "facing" => {
                *value = if wall_state.is_some() {
                    horizontal_face.unwrap()
                } else {
                    facing
                }
                .to_owned()
            }
            "rotation" => {
                *value = (((look.y_rot_deg() + 180.0) * 16.0 / 360.0 + 0.5).floor() as i32)
                    .rem_euclid(16)
                    .to_string()
            }
            _ => {}
        }
    }
    // ponytail: default + axis/facing/rotation only; support, merging,
    // waterlogging, multi-cell and other context-dependent states rely on
    // server reconciliation. Missing wall variants likewise keep the default
    // until server correction.
    state_with_properties(block_id(state), &props).unwrap_or(state)
}

fn entity_hit_wins(entity_dist_sq: f64, block_dist_sq: f64, reach: f64) -> bool {
    entity_dist_sq < block_dist_sq && entity_dist_sq < reach * reach
}

pub fn raycast(
    origin: DVec3,
    dir: Vec3,
    max_dist: f32,
    chunks: &ChunkStore,
    world_border: &crate::world::border::WorldBorder,
) -> Option<BlockHitResult> {
    raycast_with_shape(
        origin,
        dir,
        max_dist,
        chunks,
        world_border,
        BlockRayShape::Outline,
    )
}

/// Projectile/block collision ray. This is deliberately separate from the
/// player selection ray: Java BlockClipContext uses COLLIDER and Fluid.NONE.
pub fn raycast_collision(
    origin: DVec3,
    dir: Vec3,
    max_dist: f32,
    chunks: &ChunkStore,
    world_border: &crate::world::border::WorldBorder,
    entity_bottom: f64,
    descending: bool,
) -> Option<BlockHitResult> {
    raycast_with_shape(
        origin,
        dir,
        max_dist,
        chunks,
        world_border,
        BlockRayShape::Collision {
            entity_bottom,
            descending,
        },
    )
}

#[derive(Clone, Copy)]
enum BlockRayShape {
    Outline,
    Collision {
        entity_bottom: f64,
        descending: bool,
    },
}

fn raycast_with_shape(
    origin: DVec3,
    dir: Vec3,
    max_dist: f32,
    chunks: &ChunkStore,
    world_border: &crate::world::border::WorldBorder,
    shape: BlockRayShape,
) -> Option<BlockHitResult> {
    let dir = dir.as_dvec3();
    let mut bx = origin.x.floor() as i32;
    let mut by = origin.y.floor() as i32;
    let mut bz = origin.z.floor() as i32;

    let step_x = if dir.x > 0.0 { 1 } else { -1 };
    let step_y = if dir.y > 0.0 { 1 } else { -1 };
    let step_z = if dir.z > 0.0 { 1 } else { -1 };

    let t_delta_x = if dir.x != 0.0 {
        (1.0 / dir.x).abs()
    } else {
        f64::INFINITY
    };
    let t_delta_y = if dir.y != 0.0 {
        (1.0 / dir.y).abs()
    } else {
        f64::INFINITY
    };
    let t_delta_z = if dir.z != 0.0 {
        (1.0 / dir.z).abs()
    } else {
        f64::INFINITY
    };

    let mut t_max_x = if dir.x > 0.0 {
        (bx as f64 + 1.0 - origin.x) * t_delta_x
    } else {
        (origin.x - bx as f64) * t_delta_x
    };
    let mut t_max_y = if dir.y > 0.0 {
        (by as f64 + 1.0 - origin.y) * t_delta_y
    } else {
        (origin.y - by as f64) * t_delta_y
    };
    let mut t_max_z = if dir.z > 0.0 {
        (bz as f64 + 1.0 - origin.z) * t_delta_z
    } else {
        (origin.z - bz as f64) * t_delta_z
    };

    let reach_end = origin + dir * max_dist as f64;
    let mut t = 0.0_f64;
    let mut checked = HashSet::new();
    let mut nearest: Option<BlockHitResult> = None;
    while t <= max_dist as f64 {
        // A 26.2 offset shape can protrude up to 1/16 block across its cell.
        // Check horizontal neighbors as well; state-only shape tables stay canonical.
        let min_block_y = if matches!(shape, BlockRayShape::Collision { .. }) {
            by - 1
        } else {
            by
        };
        for x in bx - 1..=bx + 1 {
            for y in min_block_y..=by {
                for z in bz - 1..=bz + 1 {
                    let block_pos = BlockPos { x, y, z };
                    if !checked.insert(block_pos) {
                        continue;
                    }
                    let state = chunks.get_block_state(x, y, z);
                    if is_air(state) {
                        continue;
                    }
                    let clipped = match shape {
                        BlockRayShape::Outline => {
                            let outline = block_shape::outline_shape(state);
                            clip_with_interaction_override(
                                origin, reach_end, block_pos, outline, state,
                            )
                        }
                        BlockRayShape::Collision {
                            entity_bottom,
                            descending,
                        } => clip_collision_shape(
                            origin,
                            reach_end,
                            block_pos,
                            state,
                            chunks,
                            entity_bottom,
                            descending,
                        ),
                    };
                    if let Some((hit_point, face, inside)) = clipped {
                        let hit =
                            border_hit(origin, hit_point, block_pos, face, inside, world_border);
                        if nearest.is_none_or(|old| {
                            hit.hit_point.distance_squared(origin)
                                < old.hit_point.distance_squared(origin)
                        }) {
                            nearest = Some(hit);
                        }
                    }
                }
            }
        }
        if t_max_x < t_max_y && t_max_x < t_max_z {
            t = t_max_x;
            t_max_x += t_delta_x;
            bx += step_x;
        } else if t_max_y < t_max_z {
            t = t_max_y;
            t_max_y += t_delta_y;
            by += step_y;
        } else {
            t = t_max_z;
            t_max_z += t_delta_z;
            bz += step_z;
        }
    }
    let block_pos = BlockPos::new(
        reach_end.x.floor() as i32,
        reach_end.y.floor() as i32,
        reach_end.z.floor() as i32,
    );
    let border = Some(border_hit(
        origin,
        reach_end,
        block_pos,
        Direction::nearest(azalea_vec3(dir)).opposite(),
        false,
        world_border,
    ))
    .filter(|hit| hit.world_border);
    match (nearest, border) {
        (Some(block), Some(border))
            if border.hit_point.distance_squared(origin)
                < block.hit_point.distance_squared(origin) =>
        {
            Some(border)
        }
        (Some(block), _) => Some(block),
        (None, border) => border,
    }
}

fn border_hit(
    origin: DVec3,
    raw_location: DVec3,
    original_block_pos: BlockPos,
    original_face: Direction,
    inside: bool,
    world_border: &crate::world::border::WorldBorder,
) -> BlockHitResult {
    let world_border_hit = world_border.contains(origin.x, origin.z)
        && !world_border.contains(raw_location.x, raw_location.z);
    let hit_point = if world_border_hit {
        world_border.clamp_location(raw_location)
    } else {
        raw_location
    };
    BlockHitResult {
        block_pos: if world_border_hit {
            BlockPos::new(
                hit_point.x.floor() as i32,
                hit_point.y.floor() as i32,
                hit_point.z.floor() as i32,
            )
        } else {
            original_block_pos
        },
        // Minecraft 26.2 CollisionGetter.approximateNearestDirection uses
        // hit.location - start; a ray crossing +X/+Z faces East/South.
        face: if world_border_hit {
            approximate_nearest_direction(raw_location - origin)
        } else {
            original_face
        },
        hit_point,
        inside: inside && !world_border_hit,
        world_border: world_border_hit,
    }
}

/// Minecraft 26.2 `CollisionGetter.approximateNearestDirection`: choose the
/// cardinal direction with the greatest positive dot product, preserving the
/// vanilla tie order. The caller passes `hit.location - start` (not its
/// inverse).
fn approximate_nearest_direction(delta: DVec3) -> Direction {
    let (x, y, z) = (delta.x as f32, delta.y as f32, delta.z as f32);
    let mut result = Direction::North;
    let mut highest_dot = f32::from_bits(1);
    for (direction, dot) in [
        (Direction::Down, -y),
        (Direction::Up, y),
        (Direction::North, -z),
        (Direction::South, z),
        (Direction::West, -x),
        (Direction::East, x),
    ] {
        if dot > highest_dot {
            highest_dot = dot;
            result = direction;
        }
    }
    result
}

/// Ports vanilla `ProjectileUtil.getEntityHitResult`: clips the ray against
/// each entity's bounding box and keeps the nearest hit. A box containing the
/// ray origin counts as distance zero.
fn nearest_entity_hit(from: DVec3, to: DVec3, entities: &EntityStore) -> Option<EntityHitResult> {
    let from_v = azalea_vec3(from);
    let to_v = azalea_vec3(to);

    let mut nearest_dist_sq = f64::MAX;
    let mut nearest = None;
    for (&entity_id, entity) in &entities.living {
        let mut dims = EntityDimensions::from(entity.entity_type);
        if entity.is_baby {
            // `Squid.BABY_DIMENSIONS` is an explicit 0.5x0.5, not the
            // generic half scale.
            if matches!(
                entity.entity_type,
                EntityKind::Squid | EntityKind::GlowSquid
            ) {
                dims.width = 0.5;
                dims.height = 0.5;
            } else {
                dims.width *= 0.5;
                dims.height *= 0.5;
            }
        }
        let aabb = dims.make_bounding_box(entity.position.into());

        let (location, dist_sq) = if aabb.contains(from_v) {
            (from, 0.0)
        } else if let Some(clip) = aabb.clip(from_v, to_v) {
            let clip = DVec3::new(clip.x, clip.y, clip.z);
            (clip, clip.distance_squared(from))
        } else {
            continue;
        };

        if dist_sq < nearest_dist_sq {
            nearest_dist_sq = dist_sq;
            nearest = Some(EntityHitResult {
                entity_id,
                location,
                entity_pos: entity.position.into(),
            });
        }
    }
    nearest
}

fn azalea_vec3(v: DVec3) -> azalea_core::position::Vec3 {
    azalea_core::position::Vec3::new(v.x, v.y, v.z)
}

/// How far along the ray vanilla `VoxelShape.clip` probes to decide whether it
/// started inside the shape.
const INSIDE_PROBE_FRACTION: f64 = 0.001;

/// Ports vanilla `VoxelShape.clip`: a ray starting inside the shape hits it at
/// the probe point, otherwise the nearest box entry wins. An empty shape is
/// never hit, so the caller walks on to the next block. Vanilla's
/// degenerate-ray guard is dropped; `raycast` always passes a scaled unit
/// direction.
fn clip_shape(
    from: DVec3,
    to: DVec3,
    block_pos: BlockPos,
    boxes: &[LocalBox],
) -> Option<(DVec3, Direction, bool)> {
    clip_shape_with_offset(from, to, block_pos, boxes, DVec3::ZERO)
}

fn clip_shape_with_offset(
    from: DVec3,
    to: DVec3,
    block_pos: BlockPos,
    boxes: &[LocalBox],
    shape_offset: DVec3,
) -> Option<(DVec3, Direction, bool)> {
    if boxes.is_empty() {
        return None;
    }
    let offset = dvec3(block_pos.x as f64, block_pos.y as f64, block_pos.z as f64) + shape_offset;
    let ray = to - from;
    let probe = from + ray * INSIDE_PROBE_FRACTION;

    let starts_inside = boxes
        .iter()
        .any(|&b| Aabb::from_local(b, offset).contains(probe));
    if starts_inside {
        return Some((probe, Direction::nearest(azalea_vec3(ray)).opposite(), true));
    }

    let (t, face) = aabb::clip_boxes(boxes, offset, from, to)?;
    Some((from + ray * t, face_direction(face), false))
}

/// 26.2 BlockGetter.clipWithInteractionOverride: a closer interaction hit
/// changes only the outline hit's face (including when the ray starts inside).
fn clip_with_interaction_override(
    from: DVec3,
    to: DVec3,
    block_pos: BlockPos,
    outline: &[LocalBox],
    state: BlockState,
) -> Option<(DVec3, Direction, bool)> {
    let shape_offset = crate::world::block::block_offset(state, block_pos);
    let (point, mut face, inside) =
        clip_shape_with_offset(from, to, block_pos, outline, shape_offset)?;
    if let Some((override_point, override_face, _)) =
        clip_shape(from, to, block_pos, block_shape::interaction_shape(state))
        && override_point.distance_squared(from) < point.distance_squared(from)
    {
        face = override_face;
    }
    Some((point, face, inside))
}

const COLLISION_FULL_CUBE: &[LocalBox] = &[[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]];
const SCAFFOLD_STABLE: &[LocalBox] = &[
    [0.0, 0.875, 0.0, 1.0, 1.0, 1.0],
    [0.0, 0.0, 0.0, 0.125, 1.0, 0.125],
    [0.0, 0.0, 0.875, 0.125, 1.0, 1.0],
    [0.875, 0.0, 0.0, 1.0, 1.0, 0.125],
    [0.875, 0.0, 0.875, 1.0, 1.0, 1.0],
];
const SCAFFOLD_UNSTABLE_BOTTOM: &[LocalBox] = &[[0.0, 0.0, 0.0, 1.0, 0.125, 1.0]];
const EMPTY_COLLISION: &[LocalBox] = &[];

fn clip_collision_shape(
    from: DVec3,
    to: DVec3,
    pos: BlockPos,
    state: BlockState,
    chunks: &ChunkStore,
    entity_bottom: f64,
    descending: bool,
) -> Option<(DVec3, Direction, bool)> {
    let block_origin = dvec3(pos.x as f64, pos.y as f64, pos.z as f64);
    let (boxes, offset) = match crate::world::block::block_id(state) {
        "moving_piston" => {
            let moved = chunks
                .block_entities
                .get(&pos)
                .and_then(|entity| crate::world::block_entity::moving_block_collision(&entity.nbt));
            if let Some((moved_state, progress)) = moved {
                (
                    block_shape::partial_shape(moved_state).unwrap_or(COLLISION_FULL_CUBE),
                    crate::world::block::block_offset(moved_state, pos) + block_origin + progress,
                )
            } else {
                (
                    block_shape::partial_shape(state).unwrap_or(COLLISION_FULL_CUBE),
                    block_origin,
                )
            }
        }
        "powder_snow" => (EMPTY_COLLISION, block_origin),
        "scaffolding" => {
            let props = crate::world::block::block_properties(state);
            let is_above_block = entity_bottom > pos.y as f64 + 1.0 - f64::from(1.0e-5_f32);
            let boxes = if is_above_block && !descending {
                SCAFFOLD_STABLE
            } else if props.get("distance") != Some("0")
                && props.get("bottom") == Some("true")
                && entity_bottom > pos.y as f64 - f64::from(1.0e-5_f32)
            {
                SCAFFOLD_UNSTABLE_BOTTOM
            } else {
                EMPTY_COLLISION
            };
            (boxes, block_origin)
        }
        _ => (
            block_shape::partial_shape(state).unwrap_or(COLLISION_FULL_CUBE),
            block_origin + crate::world::block::block_offset(state, pos),
        ),
    };
    clip_shape_with_offset(from, to, pos, boxes, offset - block_origin)
}

/// Vanilla `AABB.getDirection`: a ray entering a box's min face on an axis is
/// travelling positive along it, so the face it hit points back the other way.
fn face_direction(face: Face) -> Direction {
    match (face.axis, face.max) {
        (Axis::X, false) => Direction::West,
        (Axis::X, true) => Direction::East,
        (Axis::Y, false) => Direction::Down,
        (Axis::Y, true) => Direction::Up,
        (Axis::Z, false) => Direction::North,
        (Axis::Z, true) => Direction::South,
    }
}

fn send_action(
    sender: &PacketSender,
    action: Action,
    pos: BlockPos,
    direction: Direction,
    seq: u32,
) {
    sender.send(ServerboundGamePacket::PlayerAction(
        ServerboundPlayerAction {
            action,
            pos,
            direction,
            seq,
        },
    ));
}

/// Reports a swing from using an item, block or entity where the wire
/// version does (`Translation::reports_use_swings`).
pub(crate) fn send_use_swing(sender: &PacketSender, hand: InteractionHand) {
    if crate::net::translate::active().is_none_or(|t| t.reports_use_swings()) {
        send_swing_hand(sender, hand);
    }
}

pub(crate) fn send_swing(sender: &PacketSender) {
    send_swing_hand(sender, InteractionHand::MainHand);
}

fn send_swing_hand(sender: &PacketSender, hand: InteractionHand) {
    use azalea_protocol::packets::game::s_swing::ServerboundSwing;
    sender.send(ServerboundGamePacket::Swing(ServerboundSwing { hand }));
}

/// Q / Ctrl+Q, vanilla `LocalPlayer.drop`'s player-action packet.
pub(crate) fn send_drop(sender: &PacketSender, whole_stack: bool) {
    let action = if whole_stack {
        Action::DropAllItems
    } else {
        Action::DropItem
    };
    send_action(sender, action, BlockPos::default(), Direction::Down, 0);
}

/// F, vanilla `Minecraft.handleKeybinds`' offhand swap.
pub(crate) fn send_swap_offhand(sender: &PacketSender) {
    send_action(
        sender,
        Action::SwapItemWithOffhand,
        BlockPos::default(),
        Direction::Down,
        0,
    );
}

fn hand_index(hand: InteractionHand) -> usize {
    match hand {
        InteractionHand::MainHand => 0,
        InteractionHand::OffHand => 1,
    }
}

#[cfg(test)]
mod tests {
    use azalea_registry::HolderSet;
    use azalea_registry::identifier::Identifier;

    use super::*;

    std::thread_local! {
        pub(super) static CONSUME_SOUND_REQUESTS: std::cell::Cell<usize> = const {
            std::cell::Cell::new(0)
        };
    }

    #[test]
    fn camel_feed_input_routes_only_valid_baby_food_to_the_particle_store() {
        let _protocol = crate::world::block::test_protocol_guard();
        let mut entities = EntityStore::new();
        entities.spawn_living(
            42,
            EntityKind::Camel,
            dvec3(1.0, 2.0, 3.0).into(),
            LookDirection::default(),
            0.0,
            None,
        );
        entities.living.get_mut(&42).unwrap().is_baby = true;

        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let sender = PacketSender::new(tx);
        let mut audio = AudioEngine::silent_for_test();
        let chunks = ChunkStore::new(1);
        let registry = BlockRegistry::test_empty();
        let mut particle_atlas = crate::renderer::chunk::atlas::AtlasUVMap::test_empty();
        for kind in ["happy_villager", "pause_mob_growth", "reset_mob_growth"] {
            particle_atlas.test_insert_particle_sprites(kind, vec!["particle/animal_test".into()]);
        }
        let mut particles = ParticleStore::new(
            particle_atlas,
            std::sync::Arc::new(crate::renderer::chunk::mesher::Colormap::test_empty()),
            std::sync::Arc::new(crate::renderer::chunk::mesher::Colormap::test_empty()),
            std::sync::Arc::new(crate::renderer::chunk::mesher::Colormap::test_empty()),
        );
        let climate = HashMap::new();
        let mut state = InteractionState::new();
        state
            .item_tags
            .insert("minecraft:camel_food".into(), vec![ItemKind::Cactus]);
        state.target = Some(HitResult::Entity(EntityHitResult {
            entity_id: 42,
            location: dvec3(1.0, 2.5, 3.0),
            entity_pos: dvec3(1.0, 2.0, 3.0),
        }));

        macro_rules! interact {
            ($stack:expr, $offhand:expr) => {{
                let mut effects = BreakEffects {
                    particles: &mut particles,
                    registry: &registry,
                    biome_climate: &climate,
                };
                let success = state.start_use_item(
                    &sender,
                    &mut audio,
                    &chunks,
                    dvec3(0.0, 0.0, 0.0),
                    Aabb::from_center(DVec3::ZERO, 0.6, 1.8),
                    DVec3::ZERO,
                    LookDirection::default(),
                    None,
                    None,
                    $stack,
                    20,
                    false,
                    false,
                    &entities,
                    InteractionHand::MainHand,
                    false,
                    $offhand,
                    false,
                    false,
                    false,
                    false,
                    &mut effects,
                    &mut Vec::new(),
                );
                (
                    success,
                    std::mem::take(&mut state.animal_particle_interactions),
                )
            }};
        }

        let wrong_food = ItemStackData::new(ItemKind::Wheat, 1);
        let (success, actions) = interact!(Some(&wrong_food), None);
        assert!(success);
        assert!(actions.is_empty());
        let cactus = ItemStackData::new(ItemKind::Cactus, 1);
        let (success, actions) = interact!(None, Some(&cactus));
        assert!(success);
        assert!(actions.is_empty());
        entities.living.get_mut(&42).unwrap().is_baby = false;
        let (success, actions) = interact!(Some(&cactus), None);
        assert!(success);
        assert!(actions.is_empty());
        entities.living.get_mut(&42).unwrap().is_baby = true;
        entities.living.get_mut(&42).unwrap().age_locked = true;
        let (success, actions) = interact!(Some(&cactus), None);
        assert!(success);
        assert!(actions.is_empty());

        entities.living.get_mut(&42).unwrap().age_locked = false;
        let (success, actions) = interact!(Some(&cactus), None);
        assert!(success);
        assert!(matches!(
            actions.as_slice(),
            [AnimalParticleInteraction::Immediate(42, _)]
        ));
        let request = entities
            .interaction_particle_requests(
                42,
                crate::entity::particle_animals::InteractionParticle::AgeUpHappyVillager,
                7,
            )
            .pop()
            .unwrap();
        particles.add_particle_spawn_request(request, DVec3::ZERO, &registry, &chunks, &climate);
        assert_eq!(particles.test_particle_count(), 1);

        entities.living.get_mut(&42).unwrap().entity_type = EntityKind::Mooshroom;
        entities.living.get_mut(&42).unwrap().variant = 1;
        entities.living.get_mut(&42).unwrap().is_baby = true;
        let flower = ItemStackData::new(ItemKind::Poppy, 1);
        let (success, actions) = interact!(Some(&flower), None);
        assert!(success);
        assert!(actions.is_empty());
        entities.living.get_mut(&42).unwrap().is_baby = false;
        entities.living.get_mut(&42).unwrap().variant = 0;
        let (success, actions) = interact!(Some(&flower), None);
        assert!(success);
        assert!(actions.is_empty());
        entities.living.get_mut(&42).unwrap().variant = 1;
        entities.living.get_mut(&42).unwrap().health = 0.0;
        let (success, actions) = interact!(Some(&flower), None);
        assert!(success);
        assert!(actions.is_empty());
        entities.living.get_mut(&42).unwrap().health = 20.0;
        for excluded in [ItemKind::Bowl, ItemKind::Shears, ItemKind::Cactus] {
            let excluded = ItemStackData::new(excluded, 1);
            let (success, actions) = interact!(Some(&excluded), None);
            assert!(success);
            assert!(actions.is_empty());
        }
        let (success, actions) = interact!(Some(&flower), None);
        assert!(success);
        assert!(matches!(
            actions.as_slice(),
            [AnimalParticleInteraction::MooshroomStew(
                42,
                ItemKind::Poppy
            )]
        ));
        let first = entities.mooshroom_stew_item_requests(42, flower.kind, 8);
        assert_eq!(first.len(), 4);
        assert!(
            first
                .iter()
                .all(|p| p.kind == crate::particle::ServerParticleKind::Effect)
        );
        for request in first {
            particles.add_particle_spawn_request(
                request,
                DVec3::ZERO,
                &registry,
                &chunks,
                &climate,
            );
        }
        let repeated = entities.mooshroom_stew_item_requests(42, flower.kind, 9);
        assert_eq!(repeated.len(), 2);
        assert!(
            repeated
                .iter()
                .all(|p| p.kind == crate::particle::ServerParticleKind::Smoke)
        );
        for request in repeated {
            particles.add_particle_spawn_request(
                request,
                DVec3::ZERO,
                &registry,
                &chunks,
                &climate,
            );
        }
        assert_eq!(particles.test_particle_count(), 7);

        entities.living.get_mut(&42).unwrap().entity_type = EntityKind::Cow;
        entities.living.get_mut(&42).unwrap().is_baby = true;
        let golden_dandelion = ItemStackData::new(ItemKind::GoldenDandelion, 1);
        let (success, actions) = interact!(Some(&golden_dandelion), None);
        assert!(success);
        assert!(matches!(
            actions.as_slice(),
            [AnimalParticleInteraction::AgeLock(42, true)]
        ));
        assert!(entities.start_age_lock_particle_timer(42, true));
        let lock_request = entities
            .tick_animal_particles(&chunks, 0)
            .pop()
            .expect("golden dandelion starts the age-lock burst");
        assert_eq!(
            lock_request.kind,
            crate::particle::ServerParticleKind::PauseMobGrowth
        );
        particles.add_particle_spawn_request(
            lock_request,
            DVec3::ZERO,
            &registry,
            &chunks,
            &climate,
        );
        assert_eq!(particles.test_particle_count(), 8);
        for tick in 1..40 {
            let _ = entities.tick_animal_particles(&chunks, tick);
        }
        assert!(entities.living[&42].age_locked);
        let (success, actions) = interact!(Some(&golden_dandelion), None);
        assert!(success);
        assert!(matches!(
            actions.as_slice(),
            [AnimalParticleInteraction::AgeLock(42, false)]
        ));
        assert!(entities.start_age_lock_particle_timer(42, false));
        let reset_request = entities
            .tick_animal_particles(&chunks, 40)
            .pop()
            .expect("second golden dandelion emits reset particles");
        assert_eq!(
            reset_request.kind,
            crate::particle::ServerParticleKind::ResetMobGrowth
        );
        particles.add_particle_spawn_request(
            reset_request,
            DVec3::ZERO,
            &registry,
            &chunks,
            &climate,
        );
        assert_eq!(particles.test_particle_count(), 9);

        state
            .item_tags
            .insert("minecraft:cow_food".into(), vec![ItemKind::Wheat]);
        entities.living.get_mut(&42).unwrap().age_locked = false;
        let (success, actions) = interact!(Some(&wrong_food), None);
        assert!(success);
        assert!(matches!(
            actions.as_slice(),
            [AnimalParticleInteraction::ForcedAge(42)]
        ));
        assert!(entities.start_age_up_particle_timer(42));
        let age_request = entities
            .tick_animal_particles(&chunks, 41)
            .pop()
            .expect("forced age call emits its first four-tick particle");
        assert_eq!(
            age_request.kind,
            crate::particle::ServerParticleKind::HappyVillager
        );
        particles.add_particle_spawn_request(
            age_request,
            DVec3::ZERO,
            &registry,
            &chunks,
            &climate,
        );
        assert_eq!(particles.test_particle_count(), 10);
    }

    #[test]
    fn hopper_block_use_is_consumed_by_menu() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        assert!(opens_menu(
            crate::world::block::first_state_of("hopper").unwrap()
        ));
        assert!(opens_menu(
            crate::world::block::first_state_of("brewing_stand").unwrap()
        ));
        assert!(!opens_menu(
            crate::world::block::first_state_of("stone").unwrap()
        ));
    }

    #[test]
    fn approximate_nearest_direction_keeps_north_at_smallest_subnormal_dot() {
        let smallest_positive_subnormal = f32::from_bits(1);
        let delta = dvec3(0.0, 0.0, f64::from(smallest_positive_subnormal));
        assert_eq!(delta.z as f32, smallest_positive_subnormal);
        assert_eq!(approximate_nearest_direction(delta), Direction::North);
    }

    #[test]
    fn approximate_nearest_direction_uses_f32_ties_and_vanilla_order() {
        let delta = dvec3(1.0, 0.0, 1.0 + 1e-8);
        assert_eq!(delta.x as f32, delta.z as f32);
        assert_eq!(approximate_nearest_direction(delta), Direction::South);
    }

    fn border_test_world() -> (ChunkStore, crate::world::border::WorldBorder) {
        crate::world::block::init("26.2");
        let mut chunks = ChunkStore::new(1);
        chunks.partial_storage.set(
            &azalea_core::position::ChunkPos::new(0, 0),
            Some(azalea_world::chunk::Chunk::default()),
            &mut chunks.chunk_storage,
        );
        let mut border = crate::world::border::WorldBorder::default();
        border.set_size(10.0);
        (chunks, border)
    }

    #[test]
    fn hopper_raycast_uses_hollow_outline_and_only_overrides_the_face() {
        let _protocol = crate::world::block::test_protocol_guard();
        let (chunks, _) = border_test_world();
        let border = crate::world::border::WorldBorder::default();
        let pos = BlockPos::new(2, 64, 2);
        for (facing, spout, floor) in [
            ("down", [2.5, 64.0, 2.5], 0.0),
            ("north", [2.5, 64.0, 2.1], 0.25),
            ("south", [2.5, 64.0, 2.9], 0.25),
            ("west", [2.1, 64.0, 2.5], 0.25),
            ("east", [2.9, 64.0, 2.5], 0.25),
        ] {
            let state = crate::world::block::find_state("hopper", &[("facing", facing)]);
            chunks.set_block_state(pos.x, pos.y, pos.z, state);
            let hit = raycast(
                DVec3::from_array(spout) - DVec3::Y,
                Vec3::Y,
                3.0,
                &chunks,
                &border,
            )
            .unwrap();
            assert_eq!(hit.block_pos, pos);
            assert_eq!(hit.face, Direction::Down);
            assert_eq!(hit.hit_point.y, 64.0 + floor, "{facing} spout");

            let from = dvec3(2.5, 65.06, 2.1);
            let dir = Vec3::new(0.0, -0.3, 1.0).normalize();
            let to = from + dir.as_dvec3() * 3.0;
            let raw = clip_shape(from, to, pos, block_shape::outline_shape(state)).unwrap();
            assert_eq!(raw.1, Direction::North, "inner wall, not full-cube top");
            let hit = raycast(from, dir, 3.0, &chunks, &border).unwrap();
            assert_eq!(hit.face, Direction::Up, "entry through the top opening");
            assert_eq!(hit.hit_point, raw.0, "override must not move the hit");
            assert_eq!(hit.inside, raw.2);
            assert!((hit.hit_point.z - 2.875).abs() < 1e-6);
            let floor = raycast(dvec3(2.5, 65.2, 2.5), Vec3::NEG_Y, 2.0, &chunks, &border).unwrap();
            assert_eq!(floor.hit_point.y, 64.0 + 11.0 / 16.0);
            assert_eq!(floor.face, Direction::Up);
            // Interaction shape alone must never produce a target.
            assert!(raycast(dvec3(2.5, 65.1, 2.5), Vec3::NEG_Y, 0.2, &chunks, &border).is_none());
        }
        chunks.set_block_state(
            2,
            64,
            2,
            crate::world::block::find_state("hopper", &[("facing", "north")]),
        );
        let hit = raycast(dvec3(2.5, 64.55, 1.5), Vec3::Z, 3.0, &chunks, &border).unwrap();
        assert_eq!(
            hit.hit_point,
            dvec3(2.5, 64.55, 2.25),
            "interaction-only spout space is not the hit location"
        );
        let hit = raycast(dvec3(2.5, 64.8, 2.5), Vec3::NEG_Y, 1.0, &chunks, &border).unwrap();
        assert!(!hit.inside, "inside the hollow is not inside the outline");
        assert_eq!(hit.hit_point.y, 64.6875);
        let from = dvec3(2.05, 64.8, 2.5);
        let hit = raycast(from, Vec3::X, 1.0, &chunks, &border).unwrap();
        assert!(
            hit.inside,
            "starting in the rim retains the outline's inside flag"
        );
        assert_eq!(hit.face, Direction::West);
        assert_eq!(hit.hit_point, from + DVec3::X * INSIDE_PROBE_FRACTION);
    }

    #[test]
    fn hopper_lower_space_does_not_block_entity_selection_or_blocks_behind_it() {
        let _protocol = crate::world::block::test_protocol_guard();
        let (chunks, _) = border_test_world();
        let border = crate::world::border::WorldBorder::default();
        chunks.set_block_state(2, 64, 2, crate::world::block::find_state("hopper", &[]));
        chunks.set_block_state(2, 64, 5, crate::world::block::find_state("stone", &[]));
        let from = Position::new(2.1, 64.1, 1.5);
        let hit = raycast(from.into(), Vec3::Z, REACH, &chunks, &border).unwrap();
        assert_eq!(hit.block_pos, BlockPos::new(2, 64, 5));
        let mut entities = EntityStore::new();
        entities.spawn_living(
            42,
            EntityKind::Pig,
            Position::new(2.1, 64.0, 4.25),
            LookDirection::default(),
            0.0,
            None,
        );
        let mut interaction = InteractionState::new();
        interaction.update_target(
            from,
            LookDirection::default(),
            &chunks,
            &entities,
            false,
            &border,
        );
        assert!(matches!(interaction.target, Some(HitResult::Entity(hit)) if hit.entity_id == 42));
        interaction.update_target(
            Position::new(2.5, 64.4, 1.5),
            LookDirection::default(),
            &chunks,
            &entities,
            false,
            &border,
        );
        assert!(
            matches!(interaction.target, Some(HitResult::Block(hit)) if hit.block_pos == BlockPos::new(2, 64, 2))
        );
    }

    #[test]
    fn raycast_block_hit_crossing_border_synthesizes_vanilla_result() {
        let _protocol = crate::world::block::test_protocol_guard();
        let (mut chunks, border) = border_test_world();
        let stone = crate::world::block::first_state_of("stone").unwrap();
        chunks.set_block_state(6, 64, 0, stone);

        let hit = raycast(dvec3(3.0, 64.5, 0.5), Vec3::X, 5.0, &chunks, &border).unwrap();
        assert_eq!(hit.hit_point, dvec3(5.0 - f64::from(1.0E-5_f32), 64.5, 0.5));
        assert_eq!(hit.block_pos, BlockPos::new(4, 64, 0));
        assert_eq!(hit.face, Direction::East);
        assert!(!hit.inside);
        assert!(hit.world_border);
    }

    #[test]
    fn raycast_miss_end_crossing_positive_z_synthesizes_vanilla_result() {
        let _protocol = crate::world::block::test_protocol_guard();
        let (chunks, border) = border_test_world();
        let hit = raycast(dvec3(0.5, 64.5, 3.0), Vec3::Z, 5.0, &chunks, &border).unwrap();
        assert_eq!(hit.hit_point, dvec3(0.5, 64.5, 5.0 - f64::from(1.0E-5_f32)));
        assert_eq!(hit.block_pos, BlockPos::new(0, 64, 4));
        assert_eq!(hit.face, Direction::South);
        assert!(!hit.inside);
        assert!(hit.world_border);
    }

    #[test]
    fn raycast_in_bounds_block_hit_is_not_a_border_hit() {
        let _protocol = crate::world::block::test_protocol_guard();
        let (mut chunks, border) = border_test_world();
        let stone = crate::world::block::first_state_of("stone").unwrap();
        chunks.set_block_state(2, 64, 0, stone);

        let hit = raycast(dvec3(0.5, 64.5, 0.5), Vec3::X, 4.0, &chunks, &border).unwrap();
        assert_eq!(hit.hit_point, dvec3(2.0, 64.5, 0.5));
        assert_eq!(hit.block_pos, BlockPos::new(2, 64, 0));
        assert_eq!(hit.face, Direction::West);
        assert!(!hit.inside);
        assert!(!hit.world_border);
    }

    #[test]
    fn raycast_starting_outside_does_not_synthesize_border_hit() {
        let _protocol = crate::world::block::test_protocol_guard();
        let (chunks, border) = border_test_world();
        let origin = dvec3(6.0, 64.5, 0.5);
        assert!(raycast(origin, Vec3::X, 2.0, &chunks, &border).is_none());

        let not_a_border_hit = border_hit(
            origin,
            dvec3(8.0, 64.5, 0.5),
            BlockPos::new(8, 64, 0),
            Direction::West,
            false,
            &border,
        );
        assert_eq!(not_a_border_hit.hit_point, dvec3(8.0, 64.5, 0.5));
        assert_eq!(not_a_border_hit.block_pos, BlockPos::new(8, 64, 0));
        assert_eq!(not_a_border_hit.face, Direction::West);
        assert!(!not_a_border_hit.inside);
        assert!(!not_a_border_hit.world_border);
    }

    #[test]
    fn offhand_eat_use_animation_keeps_remaining_time_and_hand() {
        let mut state = InteractionState::new();
        state.using_item = Some(ActiveUse {
            hand: InteractionHand::OffHand,
            kind: ItemKind::Apple,
            use_kind: ActiveUseKind::Consumable,
            anim: ItemUseAnimation::Eat,
            bow: false,
            sound: SoundRef::event("entity.generic.eat"),
            has_particles: true,
            use_effects: UseEffects::default(),
            duration: 32,
            remaining: 12,
        });

        let anim = state.use_animation(0.25).unwrap();
        assert_eq!(anim.curr_usage_time, 12.75);
        assert_eq!(anim.duration, 32.0);
        assert!(anim.left_hand);
    }

    #[test]
    fn respawn_resets_player_owned_interaction_transients() {
        let mut state = InteractionState::new();
        state.swinging = true;
        state.swing_time = 4;
        state.attack_anim = 0.8;
        state.o_attack_anim = 0.6;
        state.attack_strength_ticker = 7;
        state.last_item_in_main_hand = Some(ItemStackData::new(ItemKind::Stone, 1));
        state.using_item = Some(ActiveUse {
            hand: InteractionHand::MainHand,
            kind: ItemKind::Apple,
            use_kind: ActiveUseKind::Consumable,
            anim: ItemUseAnimation::Eat,
            bow: false,
            sound: SoundRef::event("entity.generic.eat"),
            has_particles: true,
            use_effects: UseEffects::default(),
            duration: 32,
            remaining: 12,
        });

        state.reset_player_transients_for_respawn();

        assert!(!state.swinging);
        assert_eq!(state.swing_time, 0);
        assert_eq!(state.attack_anim, 0.0);
        assert_eq!(state.o_attack_anim, 0.0);
        assert_eq!(state.attack_strength_ticker, 0);
        assert!(state.last_item_in_main_hand.is_none());
        assert!(state.using_item.is_none());
    }

    #[test]
    fn spectator_destroy_state_clear_is_local_only() {
        let mut state = InteractionState::new();
        state.is_destroying = true;
        state.destroy_progress = 0.75;
        state.destroy_ticks = 3.0;
        state.destroying_item = Some(ItemStackData::new(ItemKind::Stone, 1));

        state.clear_destroying_state();

        assert!(!state.is_destroying);
        assert_eq!(state.destroy_progress, 0.0);
        assert_eq!(state.destroy_ticks, 0.0);
        assert!(state.destroying_item.is_none());
        assert!(state.destroy_stage().is_none());
    }

    #[test]
    fn synced_using_item_flag_clears_server_stopped_shield_blocking_pose() {
        let mut state = InteractionState::new();
        state.using_item = Some(ActiveUse {
            hand: InteractionHand::MainHand,
            kind: ItemKind::Shield,
            use_kind: ActiveUseKind::Shield,
            anim: ItemUseAnimation::BlockKind,
            bow: false,
            sound: SoundRef::event("item.armor.equip_generic"),
            has_particles: false,
            use_effects: UseEffects::default(),
            duration: 72_000,
            remaining: 12,
        });

        state.sync_using_item_flag(true);
        assert!(state.using_item.is_some());
        assert!(state.use_animation(0.0).unwrap().shield_blocking);
        state.sync_using_item_flag(false);
        assert!(state.using_item.is_none());
        assert!(state.use_animation(0.0).is_none());
    }

    #[test]
    fn hand_animation_routes_only_the_accepted_swing_hand() {
        for hand in [InteractionHand::MainHand, InteractionHand::OffHand] {
            let mut state = InteractionState::new();
            state.start_swing(hand);
            state.update_swing();
            state.update_swing();
            let opposite = if hand == InteractionHand::MainHand {
                InteractionHand::OffHand
            } else {
                InteractionHand::MainHand
            };
            // A rejected early restart must not redirect the in-flight swing.
            state.start_swing(opposite);
            let animation = state.hand_animation(0.5, LookDirection::default());
            assert!(animation.swing_progress[hand_index(hand)] > 0.0);
            assert_eq!(animation.swing_progress[hand_index(opposite)], 0.0);
            state.swing_time = SWING_DURATION / 2;
            state.start_swing(opposite);
            assert_eq!(state.swinging_hand, opposite);
        }
    }

    #[test]
    fn item_used_lowers_only_its_hand_and_restores_point_four_per_tick() {
        for hand in [InteractionHand::MainHand, InteractionHand::OffHand] {
            let mut state = InteractionState::new();
            for _ in 0..3 {
                state.tick_hand_animation(LookDirection::default());
            }
            assert_eq!(state.hand_height, [1.0; 2]);
            state.item_used(hand);
            let i = hand_index(hand);
            let other = 1 - i;
            assert_eq!(state.hand_height[i], 0.0);
            assert_eq!(state.hand_height[other], 1.0);
            // itemUsed leaves the previous height untouched.
            assert!((state.o_hand_height[i] - 0.8).abs() < 1e-6);
            for expected in [0.4, 0.8, 1.0] {
                state.tick_hand_animation(LookDirection::default());
                assert!((state.hand_height[i] - expected).abs() < 1e-6);
                let animation = state.hand_animation(0.5, LookDirection::default());
                let inverse = 1.0 - (state.o_hand_height[i] + expected) * 0.5;
                assert!((animation.inverse_height[i] - inverse).abs() < 1e-6);
                assert_eq!(animation.inverse_height[other], 0.0);
            }
        }
    }

    #[test]
    fn hand_view_follow_smooths_half_and_interpolates_before_tenth_angle_rotation() {
        let mut state = InteractionState::new();
        let look = LookDirection::new(80.0, 40.0);
        state.tick_hand_animation(look);
        assert_eq!(state.hand_view_bob, glam::Vec2::new(20.0, 40.0));
        let animation = state.hand_animation(0.5, look);
        let expected = glam::Mat4::from_rotation_x(3.0_f32.to_radians())
            * glam::Mat4::from_rotation_y(6.0_f32.to_radians());
        assert!(animation.view_follow.abs_diff_eq(expected, 1e-6));
        state.tick_hand_animation(look);
        assert_eq!(state.hand_view_bob, glam::Vec2::new(30.0, 60.0));
    }

    #[test]
    fn dead_player_heartbeat_stops_swing_on_removal_tick_but_not_attack_cooldown() {
        let mut state = InteractionState::new();
        state.swinging = true;
        state.swing_time = 0;
        state.attack_strength_ticker = 0;

        state.tick_dead_player_state(None, true);
        assert_eq!(state.swing_time, 1);
        assert_eq!(state.attack_strength_ticker, 1);

        state.tick_dead_player_state(None, false);
        assert_eq!(
            state.swing_time, 1,
            "Player.aiStep must be skipped on the tick-20 removal tick"
        );
        assert_eq!(
            state.attack_strength_ticker, 2,
            "Player.tick state after super.tick still advances once on the removal tick"
        );
    }

    #[test]
    fn entity_pass_fallback_is_limited_to_known_plain_bow_targets() {
        let bow = ItemStackData::new(ItemKind::Bow, 1);
        for entity in [
            EntityKind::Cow,
            EntityKind::Sheep,
            EntityKind::Chicken,
            EntityKind::Zombie,
            EntityKind::Skeleton,
            EntityKind::Player,
        ] {
            assert!(
                entity_passes_for_item(Some(entity), Some(&bow)),
                "{entity:?}"
            );
        }
        for entity in [
            EntityKind::Villager,
            EntityKind::Horse,
            EntityKind::ArmorStand,
        ] {
            assert!(
                !entity_passes_for_item(Some(entity), Some(&bow)),
                "{entity:?}"
            );
        }
        assert!(!entity_passes_for_item(None, Some(&bow)));
        assert!(!entity_passes_for_item(Some(EntityKind::Cow), None));
        let stick = ItemStackData::new(ItemKind::Stick, 1);
        assert!(!entity_passes_for_item(Some(EntityKind::Cow), Some(&stick)));
    }

    fn headless_use_fixture() -> (
        ChunkStore,
        AudioEngine,
        EntityStore,
        ParticleStore,
        BlockRegistry,
    ) {
        use std::sync::Arc;

        use crate::renderer::chunk::atlas::AtlasUVMap;
        use crate::renderer::chunk::mesher::Colormap;

        let colors = Arc::new(Colormap::test_empty());
        (
            border_test_world().0,
            AudioEngine::silent_for_test(),
            EntityStore::new(),
            ParticleStore::new(
                AtlasUVMap::test_empty(),
                colors.clone(),
                colors.clone(),
                colors,
            ),
            BlockRegistry::test_empty(),
        )
    }

    #[test]
    fn item_interaction_particles_use_native_options_and_brush_cadence() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let state = crate::world::block::first_state_of("stone").unwrap();
        let (brush_kind, brush_options) = brush_block_particle(state);
        assert!(matches!(
            brush_kind,
            crate::particle::ServerParticleKind::Block
        ));
        assert!(
            matches!(brush_options, crate::particle::ServerParticleOptions::Block(s) if s == state)
        );
        let (bucket_kind, bucket_options) = bucket_evaporation_particle();
        assert!(matches!(
            bucket_kind,
            crate::particle::ServerParticleKind::LargeSmoke
        ));
        assert!(matches!(
            bucket_options,
            crate::particle::ServerParticleOptions::Simple
        ));
        assert_eq!(
            (1..=25)
                .filter(|tick| brush_dust_tick(*tick))
                .collect::<Vec<_>>(),
            [5, 15, 25]
        );
        for (main_hand_right, main_flip, off_flip) in [(true, 1.0, -1.0), (false, -1.0, 1.0)] {
            assert_eq!(
                brush_arm_flip(InteractionHand::MainHand, main_hand_right),
                main_flip
            );
            assert_eq!(
                brush_arm_flip(InteractionHand::OffHand, main_hand_right),
                off_flip
            );
        }
        assert_eq!(
            brush_dust_direction(Direction::Up, Vec3::new(0.2, 0.3, 0.4)),
            (0.4, -0.2)
        );
        assert_eq!(
            brush_dust_direction(Direction::Down, Vec3::new(0.2, 0.3, 0.4)),
            (0.4, -0.2)
        );
        assert_eq!(
            brush_dust_direction(Direction::North, Vec3::ZERO),
            (1.0, -0.1)
        );
        assert_eq!(
            brush_dust_direction(Direction::South, Vec3::ZERO),
            (-1.0, 0.1)
        );
        assert_eq!(
            brush_dust_direction(Direction::West, Vec3::ZERO),
            (-0.1, -1.0)
        );
        assert_eq!(
            brush_dust_direction(Direction::East, Vec3::ZERO),
            (0.1, 1.0)
        );
    }

    #[test]
    fn water_bucket_destination_uses_open_liquid_container_before_adjacent_block() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let mut chunks = ChunkStore::new(1);
        let mut column = azalea_world::chunk::Chunk::default();
        column.sections = vec![Default::default(); chunks.section_count() as usize].into();
        chunks.load_decoded_chunk(azalea_core::position::ChunkPos::new(0, 0), column);
        let clicked = BlockPos::new(1, 64, 1);
        let hit = BlockHitResult {
            block_pos: clicked,
            face: Direction::East,
            hit_point: DVec3::ZERO,
            inside: false,
            world_border: false,
        };
        let stairs = crate::world::block::default_state_of("oak_stairs").unwrap();
        chunks.set_block_state(clicked.x, clicked.y, clicked.z, stairs);
        assert_eq!(
            water_bucket_destination(hit, &chunks, false, false),
            Some(clicked)
        );
        assert_eq!(
            water_bucket_destination(hit, &chunks, false, true),
            Some(BlockPos::new(2, 64, 1)),
            "sneak blocks insertion into clicked dry stairs, then vanilla retries adjacent"
        );
        assert_eq!(
            water_bucket_destination(
                BlockHitResult {
                    world_border: true,
                    ..hit
                },
                &chunks,
                false,
                false,
            ),
            None
        );
        assert_eq!(
            water_bucket_destination(hit, &ChunkStore::new(1), false, false),
            None,
            "unloaded clicked block cannot be a water placement candidate"
        );

        // BucketItem.use selects adjacent for replaceable non-container clicks;
        // it does not empty into hit.block_pos.
        let replaceable = crate::world::block::default_state_of("short_grass").unwrap();
        chunks.set_block_state(clicked.x, clicked.y, clicked.z, replaceable);
        assert_eq!(
            water_bucket_destination(hit, &chunks, false, false),
            Some(BlockPos::new(2, 64, 1))
        );

        chunks.set_block_state(
            clicked.x,
            clicked.y,
            clicked.z,
            crate::world::block::default_state_of("stone").unwrap(),
        );
        let stone = crate::world::block::default_state_of("stone").unwrap();
        chunks.set_block_state(2, 64, 1, stone);
        assert_eq!(water_bucket_destination(hit, &chunks, false, false), None);
        for name in ["end_portal", "end_gateway"] {
            chunks.set_block_state(
                2,
                64,
                1,
                crate::world::block::default_state_of(name).unwrap(),
            );
            assert_eq!(
                water_bucket_destination(hit, &chunks, false, false),
                None,
                "{name}"
            );
        }

        let barrier = crate::world::block::default_state_of("barrier").unwrap();
        chunks.set_block_state(clicked.x, clicked.y, clicked.z, barrier);
        chunks.set_block_state(
            2,
            64,
            1,
            crate::world::block::default_state_of("air").unwrap(),
        );
        assert_eq!(
            water_bucket_destination(hit, &chunks, false, false),
            Some(BlockPos::new(2, 64, 1))
        );
        assert_eq!(
            water_bucket_destination(hit, &chunks, true, false),
            Some(clicked),
            "BarrierBlock is a LiquidBlockContainer; only creative state acceptance changes the result"
        );

        let ladder_dry = crate::world::block::find_state(
            "ladder",
            &[("facing", "north"), ("waterlogged", "false")],
        );
        let ladder_wet = crate::world::block::find_state(
            "ladder",
            &[("facing", "north"), ("waterlogged", "true")],
        );
        for ladder in [ladder_dry, ladder_wet] {
            chunks.set_block_state(clicked.x, clicked.y, clicked.z, ladder);
            assert_eq!(
                water_bucket_destination(hit, &chunks, false, false),
                Some(clicked),
                "LiquidBlockContainer type selects clicked before state-specific acceptance"
            );
            assert_eq!(
                water_bucket_destination(hit, &chunks, false, true),
                Some(BlockPos::new(2, 64, 1)),
                "sneaking makes emptyContents retry adjacent with no hit result"
            );
        }

        let wet_sign = crate::world::block::find_state(
            "oak_sign",
            &[("rotation", "0"), ("waterlogged", "true")],
        );
        chunks.set_block_state(clicked.x, clicked.y, clicked.z, wet_sign);
        assert_eq!(
            water_bucket_destination(hit, &chunks, false, false),
            Some(clicked),
            "wet SignBlock has w=false but SimpleWaterloggedBlock.canPlaceLiquid still returns true"
        );
        let double_slab = crate::world::block::find_state(
            "oak_slab",
            &[("type", "double"), ("waterlogged", "false")],
        );
        chunks.set_block_state(clicked.x, clicked.y, clicked.z, double_slab);
        assert_eq!(
            water_bucket_destination(hit, &chunks, false, false),
            Some(BlockPos::new(2, 64, 1)),
            "double slab type is selected first but its Java override rejects liquid"
        );

        for name in ["seagrass", "kelp", "kelp_plant"] {
            let state = crate::world::block::default_state_of(name).unwrap();
            chunks.set_block_state(clicked.x, clicked.y, clicked.z, state);
            assert_eq!(
                water_bucket_destination(hit, &chunks, false, false),
                Some(clicked),
                "propertyless LiquidBlockContainer {name}"
            );
        }

        // Older version tables do not yet have Java class-membership metadata;
        // keep their previous state-based candidate rule rather than pretending
        // the native 26.2 class data describes those older APIs.
        crate::world::block::init("1.21.11");
        let old_ladder_dry = crate::world::block::find_state(
            "ladder",
            &[("facing", "north"), ("waterlogged", "false")],
        );
        let old_ladder_wet = crate::world::block::find_state(
            "ladder",
            &[("facing", "north"), ("waterlogged", "true")],
        );
        assert_eq!(
            crate::world::block::liquid_block_container_type(old_ladder_dry),
            None
        );
        chunks.set_block_state(
            2,
            64,
            1,
            crate::world::block::default_state_of("air").unwrap(),
        );
        chunks.set_block_state(clicked.x, clicked.y, clicked.z, old_ladder_dry);
        assert_eq!(
            water_bucket_destination(hit, &chunks, false, false),
            Some(clicked)
        );
        chunks.set_block_state(clicked.x, clicked.y, clicked.z, old_ladder_wet);
        assert_eq!(
            water_bucket_destination(hit, &chunks, false, false),
            Some(BlockPos::new(2, 64, 1))
        );
    }

    #[test]
    fn redstone_ore_attack_uses_the_same_immediate_visible_dust_source() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let (mut chunks, border) = border_test_world();
        let pos = BlockPos::new(4, 64, 4);
        chunks.set_block_state(
            pos.x,
            pos.y,
            pos.z,
            crate::world::block::find_state("deepslate_redstone_ore", &[("lit", "false")]),
        );
        let mut particles = particle_test_store();
        let registry = BlockRegistry::test_empty();
        let climate = HashMap::new();
        let mut audio = AudioEngine::silent_for_test();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let sender = PacketSender::new(tx);
        let mut interaction = InteractionState::new();
        interaction.world_border = border;
        interaction.target = Some(HitResult::Block(BlockHitResult {
            block_pos: pos,
            face: Direction::Up,
            hit_point: dvec3(4.5, 65.0, 4.5),
            inside: false,
            world_border: false,
        }));
        interaction.start_attack(
            &chunks,
            &sender,
            &mut audio,
            &InputState::new(),
            dvec3(4.5, 64.0, 4.5),
            true,
            false,
            false,
            None,
            &mut BreakEffects {
                particles: &mut particles,
                registry: &registry,
                biome_climate: &climate,
            },
            &mut Vec::new(),
        );
        assert_eq!(particles.test_particle_count(), 6);
    }

    #[test]
    fn redstone_ore_use_consumes_bucket_before_bucket_use_and_enqueues_visible_dust() {
        use winit::event::{ElementState, MouseButton};

        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let (mut chunks, mut audio, entities, mut particles, registry) = headless_use_fixture();
        let pos = BlockPos::new(8, 64, 8);
        chunks.set_block_state(
            pos.x,
            pos.y,
            pos.z,
            crate::world::block::find_state("redstone_ore", &[("lit", "false")]),
        );
        let mut input = InputState::new();
        input.on_mouse_button(MouseButton::Right, ElementState::Pressed);
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let sender = PacketSender::new(tx);
        let mut interaction = InteractionState::new();
        interaction.set_water_evaporates(true);
        interaction.target = Some(HitResult::Block(BlockHitResult {
            block_pos: pos,
            face: Direction::Up,
            hit_point: dvec3(8.5, 65.0, 8.5),
            inside: false,
            world_border: false,
        }));
        let stack = ItemStackData::new(ItemKind::WaterBucket, 1);
        let player_pos = dvec3(8.5, 64.0, 8.5);
        let biome_climate = HashMap::new();
        interaction.tick(
            &input,
            &chunks,
            &sender,
            &mut audio,
            player_pos,
            Aabb::from_center(player_pos, 0.3, 0.9),
            player_pos + DVec3::Y * 1.62,
            LookDirection::default(),
            true,
            false,
            false,
            &entities,
            InteractionHand::MainHand,
            false,
            20,
            0,
            Some(&stack),
            None,
            false,
            None,
            None,
            false,
            false,
            &crate::player::LocalPlayer::new(),
            &mut BreakEffects {
                particles: &mut particles,
                registry: &registry,
                biome_climate: &biome_climate,
            },
        );
        assert_eq!(
            particles.test_particle_count(),
            6,
            "ore Dust is emitted; consumed RedStoneOre use must not add 8 water-bucket smoke particles"
        );
    }

    #[test]
    fn water_bucket_evaporation_is_a_client_local_eight_large_smoke_burst() {
        use winit::event::{ElementState, MouseButton};

        let run = |water_evaporates: bool, stack_kind: ItemKind| {
            let (chunks, mut audio, entities, mut particles, registry) = headless_use_fixture();
            let mut input = InputState::new();
            input.on_mouse_button(MouseButton::Right, ElementState::Pressed);
            let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
            let sender = PacketSender::new(tx);
            let mut state = InteractionState::new();
            state.set_water_evaporates(water_evaporates);
            state.target = Some(HitResult::Block(BlockHitResult {
                block_pos: BlockPos::new(0, 64, 0),
                face: Direction::Up,
                hit_point: dvec3(0.5, 65.0, 0.5),
                inside: false,
                world_border: false,
            }));
            let stack = ItemStackData::new(stack_kind, 1);
            let pos = dvec3(0.5, 64.0, 0.5);
            let biome_climate = HashMap::new();
            state.tick(
                &input,
                &chunks,
                &sender,
                &mut audio,
                pos,
                Aabb::from_center(pos, 0.3, 0.9),
                pos + DVec3::Y * 1.62,
                LookDirection::default(),
                true,
                false,
                false,
                &entities,
                InteractionHand::MainHand,
                false,
                20,
                0,
                Some(&stack),
                None,
                false,
                None,
                None,
                false,
                false,
                &crate::player::LocalPlayer::new(),
                &mut BreakEffects {
                    particles: &mut particles,
                    registry: &registry,
                    biome_climate: &biome_climate,
                },
            );
            particles.test_pending().len()
        };

        assert_eq!(run(false, ItemKind::WaterBucket), 0);
        assert_eq!(run(true, ItemKind::Bucket), 0);
        assert_eq!(run(true, ItemKind::WaterBucket), 8);
    }

    #[test]
    fn water_bucket_block_use_consumption_matches_java_before_evaporation_particles() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");

        let run = |block: &str, sneaking: bool, water_evaporates: bool| {
            let (mut chunks, mut audio, entities, mut particles, registry) = headless_use_fixture();
            let state = crate::world::block::default_state_of(block).unwrap();
            chunks.set_block_state(0, 64, 0, state);
            let hit = BlockHitResult {
                block_pos: BlockPos::new(0, 64, 0),
                face: Direction::Up,
                hit_point: dvec3(0.5, 65.0, 0.5),
                inside: false,
                world_border: false,
            };
            let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
            let sender = PacketSender::new(tx);
            let mut interaction = InteractionState::new();
            interaction.set_water_evaporates(water_evaporates);
            interaction.target = Some(HitResult::Block(hit));
            let player_pos = dvec3(0.5, 64.0, 0.5);
            let biome_climate = HashMap::new();
            interaction.start_use_item(
                &sender,
                &mut audio,
                &chunks,
                player_pos,
                Aabb::from_center(player_pos, 0.3, 0.9),
                player_pos + DVec3::Y * 1.62,
                LookDirection::default(),
                None,
                None,
                Some(&ItemStackData::new(ItemKind::WaterBucket, 1)),
                20,
                false,
                false,
                &entities,
                InteractionHand::MainHand,
                false,
                None,
                false,
                false,
                sneaking,
                sneaking,
                &mut BreakEffects {
                    particles: &mut particles,
                    registry: &registry,
                    biome_climate: &biome_climate,
                },
                &mut Vec::new(),
            );
            particles.test_pending().len()
        };

        assert_eq!(run("brewing_stand", false, true), 0);
        assert_eq!(run("brewing_stand", true, true), 8);
        assert_eq!(run("stone", false, true), 8);
        assert_eq!(run("stone", false, false), 0);
    }

    #[test]
    fn water_bucket_empty_contents_uses_exact_java_replaceability_and_candidate_position() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let run = |clicked_state: BlockState,
                   adjacent_state: BlockState,
                   evaporates_at: (bool, bool),
                   sneaking: bool| {
            let (mut chunks, mut audio, entities, mut particles, registry) = headless_use_fixture();
            let mut column = azalea_world::chunk::Chunk::default();
            column.sections = vec![Default::default(); chunks.section_count() as usize].into();
            chunks.load_decoded_chunk(azalea_core::position::ChunkPos::new(0, 0), column);
            let hit_pos = BlockPos::new(0, 64, 0);
            let adjacent = BlockPos::new(0, 65, 0);
            chunks.set_block_state(hit_pos.x, hit_pos.y, hit_pos.z, clicked_state);
            chunks.set_block_state(adjacent.x, adjacent.y, adjacent.z, adjacent_state);
            assert_eq!(
                chunks.get_block_state(hit_pos.x, hit_pos.y, hit_pos.z),
                clicked_state
            );
            assert_eq!(
                chunks.get_block_state(adjacent.x, adjacent.y, adjacent.z),
                adjacent_state
            );
            let hit = BlockHitResult {
                block_pos: hit_pos,
                face: Direction::Up,
                hit_point: dvec3(0.5, 65.0, 0.5),
                inside: false,
                world_border: false,
            };
            let destination = water_bucket_destination(hit, &chunks, false, sneaking);
            let evaporates = match destination {
                Some(pos) if pos == hit_pos => evaporates_at.0,
                Some(pos) if pos == adjacent => evaporates_at.1,
                _ => false,
            };

            let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
            let sender = PacketSender::new(tx);
            let mut interaction = InteractionState::new();
            interaction.set_water_evaporates(evaporates);
            interaction.target = Some(HitResult::Block(hit));
            let player_pos = dvec3(0.5, 64.0, 0.5);
            let biome_climate = HashMap::new();
            interaction.start_use_item(
                &sender,
                &mut audio,
                &chunks,
                player_pos,
                Aabb::from_center(player_pos, 0.3, 0.9),
                player_pos + DVec3::Y * 1.62,
                LookDirection::default(),
                None,
                None,
                Some(&ItemStackData::new(ItemKind::WaterBucket, 1)),
                20,
                false,
                false,
                &entities,
                InteractionHand::MainHand,
                false,
                None,
                false,
                false,
                sneaking,
                sneaking,
                &mut BreakEffects {
                    particles: &mut particles,
                    registry: &registry,
                    biome_climate: &biome_climate,
                },
                &mut Vec::new(),
            );
            (
                destination,
                particles.test_pending().len(),
                particles.test_pending_positions(),
            )
        };

        let in_block = |positions: &[DVec3], block: BlockPos| {
            positions.iter().all(|p| {
                p.x >= block.x as f64
                    && p.x < block.x as f64 + 1.0
                    && p.y >= block.y as f64
                    && p.y < block.y as f64 + 1.0
                    && p.z >= block.z as f64
                    && p.z < block.z as f64 + 1.0
            })
        };
        let stone = crate::world::block::default_state_of("stone").unwrap();
        let cobweb = crate::world::block::default_state_of("cobweb").unwrap();
        let adjacent = BlockPos::new(0, 65, 0);
        let assert_output = |(destination, count, positions): (
            Option<BlockPos>,
            usize,
            Vec<DVec3>,
        ),
                             expected: Option<BlockPos>,
                             particle_block: Option<BlockPos>| {
            assert_eq!(destination, expected);
            assert_eq!(
                count,
                if particle_block.is_some() { 8 } else { 0 },
                "destination={destination:?}, particle_block={particle_block:?}, positions={positions:?}"
            );
            if let Some(block) = particle_block {
                assert!(
                    in_block(&positions, block),
                    "particles not inside {block:?}: {positions:?}"
                );
            } else {
                assert!(positions.is_empty());
            }
        };
        assert_output(run(stone, cobweb, (true, false), false), None, None);

        let standing_sign = crate::world::block::default_state_of("oak_sign").unwrap();
        // Dry sign accepts water through LiquidBlockContainer even though w=false.
        assert_output(
            run(stone, standing_sign, (false, true), false),
            Some(adjacent),
            Some(adjacent),
        );
        let waterlogged_sign = crate::world::block::find_state(
            "oak_sign",
            &[("rotation", "0"), ("waterlogged", "true")],
        );
        // Java SignBlock inherits SimpleWaterloggedBlock.canPlaceLiquid, which
        // accepts WATER independent of WATERLOGGED. Thus w=false does not force
        // fallback; the environment branch runs before placeLiquid.
        assert_output(
            run(waterlogged_sign, BlockState::AIR, (true, false), false),
            Some(BlockPos::new(0, 64, 0)),
            Some(BlockPos::new(0, 64, 0)),
        );
        let double_slab = crate::world::block::find_state(
            "oak_slab",
            &[("type", "double"), ("waterlogged", "false")],
        );
        // Give the clicked and adjacent coordinates opposite environment
        // values. The result must query only the chosen candidate: a dry ladder
        // is selected at clicked (true there, false adjacent), while a double
        // slab rejects at clicked and retries to adjacent (false clicked, true
        // adjacent).
        assert_output(
            run(double_slab, BlockState::AIR, (false, true), false),
            Some(adjacent),
            Some(adjacent),
        );
        assert_output(
            run(double_slab, BlockState::AIR, (true, false), false),
            Some(adjacent),
            None,
        );

        let air = BlockState::AIR;
        for waterlogged in ["false", "true"] {
            let ladder = crate::world::block::find_state(
                "ladder",
                &[("facing", "north"), ("waterlogged", waterlogged)],
            );
            // Java selects clicked by block type. SimpleWaterloggedBlock's
            // canPlaceLiquid accepts WATER regardless of the wet state, and the
            // Ladder also has exact w=true for the replaceability route.
            assert_output(
                run(ladder, air, (true, false), false),
                Some(BlockPos::new(0, 64, 0)),
                Some(BlockPos::new(0, 64, 0)),
            );
            // Sneak is checked by emptyContents, not BucketItem.use's initial
            // type choice; failure recurses to adjacent with a null hit result.
            assert_output(
                run(ladder, air, (false, true), true),
                Some(adjacent),
                Some(adjacent),
            );
            assert_output(
                run(ladder, air, (false, true), false),
                Some(BlockPos::new(0, 64, 0)),
                None,
            );
        }

        // Non-container hit chooses adjacent. Types without a waterlogged
        // property are taken directly from Java's interface membership.
        assert_output(
            run(stone, air, (false, true), false),
            Some(adjacent),
            Some(adjacent),
        );
        for name in ["seagrass", "kelp", "kelp_plant"] {
            let state = crate::world::block::default_state_of(name).unwrap();
            assert_output(
                run(state, air, (true, false), false),
                Some(BlockPos::new(0, 64, 0)),
                Some(BlockPos::new(0, 64, 0)),
            );
        }

        let moving_piston = crate::world::block::default_state_of("moving_piston").unwrap();
        assert_output(run(stone, moving_piston, (false, true), false), None, None);
        let big_dripleaf = crate::world::block::default_state_of("big_dripleaf").unwrap();
        assert_output(
            run(stone, big_dripleaf, (false, true), false),
            Some(adjacent),
            Some(adjacent),
        );
        assert_output(
            run(stone, big_dripleaf, (true, false), false),
            Some(adjacent),
            None,
        );
    }

    #[test]
    fn water_bucket_unloaded_fallback_does_not_emit_evaporation_particles() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let (mut chunks, mut audio, entities, mut particles, registry) = headless_use_fixture();
        let mut column = azalea_world::chunk::Chunk::default();
        column.sections = vec![Default::default(); chunks.section_count() as usize].into();
        chunks.load_decoded_chunk(azalea_core::position::ChunkPos::new(0, 0), column);
        let hit_pos = BlockPos::new(15, 64, 0);
        let hit = BlockHitResult {
            block_pos: hit_pos,
            face: Direction::East,
            hit_point: dvec3(16.0, 64.5, 0.5),
            inside: false,
            world_border: false,
        };
        chunks.set_block_state(
            hit_pos.x,
            hit_pos.y,
            hit_pos.z,
            crate::world::block::default_state_of("stone").unwrap(),
        );
        assert!(
            chunks
                .get_chunk(&azalea_core::position::ChunkPos::new(1, 0))
                .is_none()
        );
        assert_eq!(water_bucket_destination(hit, &chunks, false, false), None);

        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let sender = PacketSender::new(tx);
        let mut interaction = InteractionState::new();
        interaction.set_water_evaporates(true);
        interaction.target = Some(HitResult::Block(hit));
        let player_pos = dvec3(15.5, 64.0, 0.5);
        let biome_climate = HashMap::new();
        interaction.start_use_item(
            &sender,
            &mut audio,
            &chunks,
            player_pos,
            Aabb::from_center(player_pos, 0.3, 0.9),
            player_pos + DVec3::Y * 1.62,
            LookDirection::default(),
            None,
            None,
            Some(&ItemStackData::new(ItemKind::WaterBucket, 1)),
            20,
            false,
            false,
            &entities,
            InteractionHand::MainHand,
            false,
            None,
            false,
            false,
            false,
            false,
            &mut BreakEffects {
                particles: &mut particles,
                registry: &registry,
                biome_climate: &biome_climate,
            },
            &mut Vec::new(),
        );
        assert!(particles.test_pending().is_empty());

        // CauldronInteractions consumes WATER_BUCKET before BucketItem.use, so
        // the evaporation path must not run even when its environment is true.
        chunks.set_block_state(
            hit_pos.x,
            hit_pos.y,
            hit_pos.z,
            crate::world::block::default_state_of("water_cauldron").unwrap(),
        );
        interaction.target = Some(HitResult::Block(hit));
        assert!(interaction.start_use_item(
            &sender,
            &mut audio,
            &chunks,
            player_pos,
            Aabb::from_center(player_pos, 0.3, 0.9),
            player_pos + DVec3::Y * 1.62,
            LookDirection::default(),
            None,
            None,
            Some(&ItemStackData::new(ItemKind::WaterBucket, 1)),
            20,
            false,
            false,
            &entities,
            InteractionHand::MainHand,
            false,
            None,
            false,
            false,
            false,
            false,
            &mut BreakEffects {
                particles: &mut particles,
                registry: &registry,
                biome_climate: &biome_climate,
            },
            &mut Vec::new(),
        ));
        assert!(particles.test_pending().is_empty());
    }

    #[test]
    fn books_use_item_open_locally_only_for_writable_content() {
        use crate::net::sender::Outbound;
        let (chunks, mut audio, _, mut particles, registry) = headless_use_fixture();
        let biome_climate = HashMap::new();
        let mut effects = BreakEffects {
            particles: &mut particles,
            registry: &registry,
            biome_climate: &biome_climate,
        };
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let sender = PacketSender::new(tx);
        for (kind, cooldown, expected_open) in [
            (
                ItemKind::WritableBook,
                false,
                Some(InteractionHand::MainHand),
            ),
            (ItemKind::WritableBook, true, None),
            (ItemKind::WrittenBook, false, None),
        ] {
            let mut state = InteractionState::new();
            let stack = ItemStackData::new(kind, 1);
            assert!(
                state.use_item(
                    &sender,
                    &mut audio,
                    &chunks,
                    dvec3(0.5, 64.0, 0.5),
                    dvec3(0.5, 65.62, 0.5),
                    LookDirection::default(),
                    Some(&stack),
                    20,
                    false,
                    InteractionHand::MainHand,
                    cooldown,
                    false,
                    &mut effects,
                ) == if cooldown {
                    ItemUseResult::Pass
                } else {
                    ItemUseResult::Success
                },
            );
            assert!(
                matches!(rx.try_recv(), Ok(Outbound::Packet(packet)) if matches!(*packet, ServerboundGamePacket::UseItem(_)))
            );
            assert_eq!(state.take_writable_book_open(), expected_open);
            assert_eq!(state.take_writable_book_open(), None);
        }
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn air_use_cooldown_and_fail_allow_offhand_food_through_real_tick() {
        use crate::net::sender::Outbound;
        for (kind, cooldown, food, expected_use) in [
            (
                ItemKind::EnderPearl,
                true,
                10,
                Some(InteractionHand::OffHand),
            ),
            (ItemKind::Bow, false, 10, Some(InteractionHand::OffHand)),
            (ItemKind::Apple, false, 20, None),
            (ItemKind::EnderPearl, false, 10, None),
        ] {
            let (chunks, mut audio, entities, mut particles, registry) = headless_use_fixture();
            let mut input = InputState::new();
            input.on_mouse_button(
                winit::event::MouseButton::Right,
                winit::event::ElementState::Pressed,
            );
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let sender = PacketSender::new(tx);
            let mut state = InteractionState::new();
            let main = ItemStackData::new(kind, 1);
            let off = ItemStackData::new(ItemKind::Apple, 1);
            let pos = dvec3(0.5, 64.0, 0.5);
            let biome_climate = HashMap::new();
            state.tick(
                &input,
                &chunks,
                &sender,
                &mut audio,
                pos,
                Aabb::from_center(pos, 0.3, 0.9),
                pos + DVec3::Y * 1.62,
                LookDirection::default(),
                true,
                false,
                false,
                &entities,
                InteractionHand::MainHand,
                cooldown,
                food,
                0,
                Some(&main),
                Some(&off),
                false,
                None,
                None,
                false,
                false,
                &crate::player::LocalPlayer::new(),
                &mut BreakEffects {
                    particles: &mut particles,
                    registry: &registry,
                    biome_climate: &biome_climate,
                },
            );
            let mut uses = Vec::new();
            let mut swings = Vec::new();
            while let Ok(outbound) = rx.try_recv() {
                if let Outbound::Packet(p) = outbound {
                    match *p {
                        ServerboundGamePacket::UseItem(p) => uses.push((p.hand, p.seq)),
                        ServerboundGamePacket::Swing(p) => swings.push(p.hand),
                        ServerboundGamePacket::SetCarriedItem(_) => {}
                        p => panic!("unexpected {p:?}"),
                    }
                } else {
                    panic!("unexpected raw packet")
                }
            }
            let fallback = kind != ItemKind::EnderPearl || cooldown;
            let mut expected = vec![(InteractionHand::MainHand, 1)];
            if fallback {
                expected.push((InteractionHand::OffHand, 2));
            }
            assert_eq!(uses, expected, "{kind:?}");
            assert_eq!(
                swings,
                if fallback {
                    vec![]
                } else {
                    vec![InteractionHand::MainHand]
                }
            );
            assert_eq!(state.using_item.as_ref().map(|u| u.hand), expected_use);
        }
    }

    #[test]
    fn start_use_item_orders_main_and_offhand_packets() {
        let _protocol = crate::world::block::test_protocol_guard();
        use InteractionHand::{MainHand, OffHand};

        use crate::net::sender::Outbound;

        #[derive(Debug, PartialEq)]
        enum Sent {
            On(InteractionHand, u32),
            Use(InteractionHand, u32),
            Swing(InteractionHand),
        }

        // Vanilla 26.2 Minecraft.startUseItem tries both hands on PASS;
        // successful placement also reports a client swing in this version.
        let cases = [
            (
                "multistate log consumes main hand",
                ItemKind::OakLog,
                ItemKind::Stone,
                false,
                vec![Sent::On(MainHand, 1), Sent::Swing(MainHand)],
            ),
            (
                "multistate stair consumes main hand",
                ItemKind::OakStairs,
                ItemKind::Stone,
                false,
                vec![Sent::On(MainHand, 1), Sent::Swing(MainHand)],
            ),
            (
                "sign consumes main hand",
                ItemKind::OakSign,
                ItemKind::Stone,
                false,
                vec![Sent::On(MainHand, 1), Sent::Swing(MainHand)],
            ),
            (
                "wall torch predicts its wall variant, not offhand fallback",
                ItemKind::Torch,
                ItemKind::Stone,
                false,
                vec![Sent::On(MainHand, 1), Sent::Swing(MainHand)],
            ),
            (
                "stick + placeable offhand",
                ItemKind::Stick,
                ItemKind::Stone,
                false,
                vec![
                    Sent::On(MainHand, 1),
                    Sent::Use(MainHand, 2),
                    Sent::On(OffHand, 3),
                    Sent::Swing(OffHand),
                ],
            ),
            (
                "stick + apple offhand",
                ItemKind::Stick,
                ItemKind::Apple,
                false,
                vec![
                    Sent::On(MainHand, 1),
                    Sent::Use(MainHand, 2),
                    Sent::On(OffHand, 3),
                    Sent::Use(OffHand, 4),
                ],
            ),
            (
                "shield consumes main-hand use",
                ItemKind::Shield,
                ItemKind::Stone,
                false,
                vec![Sent::On(MainHand, 1), Sent::Use(MainHand, 2)],
            ),
            (
                "spectator stops after main-hand block use",
                ItemKind::Stick,
                ItemKind::Stone,
                true,
                vec![Sent::On(MainHand, 1)],
            ),
        ];

        for (name, main_kind, off_kind, spectator, expected) in cases {
            let (chunks, mut audio, entities, mut particles, registry) = headless_use_fixture();
            let stone = crate::world::block::first_state_of("stone").unwrap();
            let hit = BlockHitResult {
                block_pos: BlockPos::new(2, 64, 2),
                face: if main_kind == ItemKind::Torch {
                    Direction::East
                } else {
                    Direction::Up
                },
                hit_point: if main_kind == ItemKind::Torch {
                    dvec3(3.0, 64.5, 2.5)
                } else {
                    dvec3(2.5, 65.0, 2.5)
                },
                inside: false,
                world_border: false,
            };
            chunks.set_block_state(2, 64, 2, stone);
            assert_eq!(chunks.get_block_state(2, 64, 2), stone, "{name}");
            let placed_pos = hit.block_pos.offset_with_direction(hit.face);
            assert!(is_air(chunks.get_block_state(2, 65, 2)), "{name}");
            let mut state = InteractionState::new();
            state.hand_height = [1.0; 2];
            state.o_hand_height = [1.0; 2];
            state.target = Some(HitResult::Block(hit));
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let sender = PacketSender::new(tx);
            let biome_climate = HashMap::new();
            let mut effects = BreakEffects {
                particles: &mut particles,
                registry: &registry,
                biome_climate: &biome_climate,
            };
            let mut dirty_chunks = Vec::new();
            let main = ItemStackData::new(main_kind, 1);
            let off = ItemStackData::new(off_kind, 1);
            let player_pos = dvec3(0.5, 64.0, 0.5);
            assert!(
                state.start_use_item(
                    &sender,
                    &mut audio,
                    &chunks,
                    player_pos,
                    Aabb::from_center(player_pos, 0.3, 0.9),
                    player_pos + dvec3(0.0, 1.62, 0.0),
                    LookDirection::default(),
                    registry.placeable_block_for_item(&item_resource_name(main_kind)),
                    registry.placeable_block_for_item(&item_resource_name(off_kind)),
                    Some(&main),
                    10, // hungry, so the offhand apple starts consuming
                    false,
                    spectator,
                    &entities,
                    MainHand,
                    false,
                    Some(&off),
                    false,
                    false,
                    false,
                    false,
                    &mut effects,
                    &mut dirty_chunks,
                ),
                "{name}",
            );

            let mut actual = Vec::new();
            while let Ok(outbound) = rx.try_recv() {
                let Outbound::Packet(packet) = outbound else {
                    panic!("{name}: unexpected non-packet outbound");
                };
                actual.push(match *packet {
                    ServerboundGamePacket::UseItemOn(packet) => {
                        assert_eq!(packet.block_hit.block_pos, hit.block_pos, "{name}");
                        assert_eq!(packet.block_hit.direction, hit.face, "{name}");
                        Sent::On(packet.hand, packet.seq)
                    }
                    ServerboundGamePacket::UseItem(packet) => Sent::Use(packet.hand, packet.seq),
                    ServerboundGamePacket::Swing(packet) => Sent::Swing(packet.hand),
                    packet => panic!("{name}: unexpected packet {packet:?}"),
                });
            }
            assert_eq!(actual, expected, "{name}");
            if let Some(swing_hand) = actual.iter().find_map(|sent| match sent {
                Sent::Swing(hand) => Some(*hand),
                _ => None,
            }) {
                assert!(state.swinging, "{name}");
                assert_eq!(state.swinging_hand, swing_hand, "{name}");
            } else {
                assert!(!state.swinging, "{name}");
            }
            let main_prediction = registry.placeable_block_for_item(&item_resource_name(main_kind));
            let placed = !spectator
                && (main_prediction.is_some()
                    || main_kind == ItemKind::Stick && off_kind == ItemKind::Stone);
            let predicted = if main_kind == ItemKind::Torch {
                crate::world::block::find_state("wall_torch", &[("facing", "east")])
            } else {
                main_prediction.unwrap_or(stone)
            };
            assert_eq!(
                chunks.get_block_state(placed_pos.x, placed_pos.y, placed_pos.z),
                if placed { predicted } else { BlockState::AIR },
                "{name}",
            );
            assert_eq!(
                dirty_chunks,
                if placed { vec![placed_pos] } else { vec![] },
                "{name}",
            );
            assert_eq!(
                state.pending_predictions.len(),
                usize::from(placed),
                "{name}"
            );
            if placed {
                assert_eq!(
                    state.pending_predictions[&placed_pos].seq,
                    if main_kind == ItemKind::Stick { 3 } else { 1 },
                    "{name}"
                );
            }
            let expected_use = match main_kind {
                ItemKind::Shield => Some((MainHand, ItemKind::Shield)),
                _ if off_kind == ItemKind::Apple => Some((OffHand, ItemKind::Apple)),
                _ => None,
            };
            assert_eq!(
                state.using_item.as_ref().map(|use_| (use_.hand, use_.kind)),
                expected_use,
                "{name}",
            );
            if let Some(hand) = actual.iter().find_map(|sent| match sent {
                Sent::Swing(hand) => Some(*hand),
                _ => None,
            }) {
                // Survival never shrinks a stack speculatively. Only its server
                // count change lowers that hand, including consuming the last item.
                assert_eq!(state.hand_height, [1.0; 2], "{name}");
                let (main_after, off_after) = if hand == MainHand {
                    (None, Some(&off))
                } else {
                    (Some(&main), None)
                };
                state.update_placement_heights(main_after, off_after);
                let mut expected_height = [1.0; 2];
                expected_height[hand_index(hand)] = 0.0;
                assert_eq!(state.hand_height, expected_height, "{name}");
                state.tick_hand_animation(LookDirection::default());
                expected_height[hand_index(hand)] = 0.4;
                assert_eq!(state.hand_height, expected_height, "{name}");
            }
        }
    }

    #[test]
    fn start_use_item_predicts_even_when_server_may_reject_placement() {
        let _protocol = crate::world::block::test_protocol_guard();
        use InteractionHand::{MainHand, OffHand};

        use crate::net::sender::Outbound;

        for (
            name,
            main_kind,
            blocked,
            overlap,
            menu,
            sneak,
            cooldown,
            off_cooldown,
            expected_hand,
        ) in [
            (
                "occupied",
                ItemKind::Stone,
                true,
                false,
                false,
                false,
                false,
                false,
                Some(MainHand),
            ),
            (
                "player collision",
                ItemKind::Stone,
                false,
                true,
                false,
                false,
                false,
                false,
                Some(MainHand),
            ),
            (
                "cooldown",
                ItemKind::Stone,
                false,
                false,
                false,
                false,
                true,
                false,
                Some(MainHand),
            ),
            (
                "offhand cooldown",
                ItemKind::Stick,
                false,
                false,
                false,
                false,
                false,
                true,
                Some(OffHand),
            ),
            (
                "menu consumes use",
                ItemKind::Stone,
                false,
                false,
                true,
                false,
                false,
                false,
                None,
            ),
            (
                "sneak bypasses menu",
                ItemKind::Stone,
                false,
                false,
                true,
                true,
                false,
                false,
                Some(MainHand),
            ),
            (
                "creative main",
                ItemKind::Stone,
                false,
                false,
                false,
                false,
                false,
                false,
                Some(MainHand),
            ),
            (
                "creative offhand",
                ItemKind::Stick,
                false,
                false,
                false,
                false,
                false,
                false,
                Some(OffHand),
            ),
        ] {
            let (chunks, mut audio, entities, mut particles, registry) = headless_use_fixture();
            let stone = crate::world::block::first_state_of("stone").unwrap();
            chunks.set_block_state(
                2,
                64,
                2,
                if menu {
                    crate::world::block::first_state_of("hopper").unwrap()
                } else {
                    stone
                },
            );
            if blocked {
                chunks.set_block_state(2, 65, 2, stone);
            }
            let hit = BlockHitResult {
                block_pos: BlockPos::new(2, 64, 2),
                face: Direction::Up,
                hit_point: dvec3(2.5, 65.0, 2.5),
                inside: false,
                world_border: false,
            };
            let player_pos = if overlap {
                dvec3(2.5, 65.0, 2.5)
            } else {
                dvec3(0.5, 64.0, 0.5)
            };
            let mut state = InteractionState::new();
            state.hand_height = [1.0; 2];
            state.o_hand_height = [1.0; 2];
            state.target = Some(HitResult::Block(hit));
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let sender = PacketSender::new(tx);
            let biome_climate = HashMap::new();
            let mut effects = BreakEffects {
                particles: &mut particles,
                registry: &registry,
                biome_climate: &biome_climate,
            };
            let main = ItemStackData::new(main_kind, 64);
            let off = ItemStackData::new(ItemKind::Stone, 64);
            let mut dirty = Vec::new();
            let consumed = state.start_use_item(
                &sender,
                &mut audio,
                &chunks,
                player_pos,
                Aabb::from_center(player_pos, 0.3, 0.9),
                player_pos + dvec3(0.0, 1.62, 0.0),
                LookDirection::default(),
                registry.placeable_block_for_item(&item_resource_name(main_kind)),
                Some(stone),
                Some(&main),
                20,
                true,
                false,
                &entities,
                MainHand,
                cooldown,
                Some(&off),
                off_cooldown,
                false,
                sneak,
                sneak,
                &mut effects,
                &mut dirty,
            );
            assert_eq!(
                consumed,
                expected_hand.is_some() || menu && !sneak,
                "{name}"
            );
            let mut swings = Vec::new();
            let mut on_hands = Vec::new();
            while let Ok(out) = rx.try_recv() {
                let Outbound::Packet(packet) = out else {
                    panic!("{name}: unexpected raw packet")
                };
                match *packet {
                    ServerboundGamePacket::Swing(packet) => swings.push(packet.hand),
                    ServerboundGamePacket::UseItemOn(packet) => on_hands.push(packet.hand),
                    ServerboundGamePacket::UseItem(packet) => {
                        assert_eq!(packet.hand, MainHand, "{name}")
                    }
                    packet => panic!("{name}: unexpected {packet:?}"),
                }
            }
            assert_eq!(
                swings,
                expected_hand.into_iter().collect::<Vec<_>>(),
                "{name}"
            );
            assert_eq!(
                on_hands,
                if main_kind == ItemKind::Stick {
                    vec![MainHand, OffHand]
                } else {
                    vec![MainHand]
                },
                "{name}"
            );
            assert_eq!(state.swinging, expected_hand.is_some(), "{name}");
            let mut heights = [1.0; 2];
            if let Some(hand) = expected_hand {
                heights[hand_index(hand)] = 0.0;
                assert_eq!(state.swinging_hand, hand, "{name}");
            }
            assert_eq!(state.hand_height, heights, "{name}");
            assert_eq!(
                dirty,
                if expected_hand.is_some() {
                    vec![BlockPos::new(2, 65, 2)]
                } else {
                    vec![]
                },
                "{name}"
            );
            assert_eq!(state.use_delay, USE_DELAY, "{name}");
            for height in [0.4, 0.8, 1.0] {
                state.tick_hand_animation(LookDirection::default());
                if let Some(hand) = expected_hand {
                    heights[hand_index(hand)] = height;
                }
                assert_eq!(state.hand_height, heights, "{name}");
            }
        }
    }

    #[test]
    fn start_use_item_preserves_inside_and_restores_replaced_state_on_rejected_ack() {
        let _protocol = crate::world::block::test_protocol_guard();
        use crate::net::sender::Outbound;
        for replace_clicked in [true, false] {
            let (chunks, mut audio, entities, mut particles, registry) = headless_use_fixture();
            let stone = crate::world::block::first_state_of("stone").unwrap();
            let previous = if replace_clicked {
                crate::world::block::first_state_of("short_grass").unwrap()
            } else {
                crate::world::block::water_source_state()
            };
            chunks.set_block_state(2, 64, 2, if replace_clicked { previous } else { stone });
            let pos = BlockPos::new(2, if replace_clicked { 64 } else { 65 }, 2);
            if !replace_clicked {
                chunks.set_block_state(pos.x, pos.y, pos.z, previous);
            }
            let border = crate::world::border::WorldBorder::default();
            let hit = raycast(dvec3(2.5, 64.5, 2.5), Vec3::NEG_Y, REACH, &chunks, &border).unwrap();
            assert!(hit.inside);
            assert_eq!(hit.face, Direction::Up);
            let mut state = InteractionState::new();
            state.target = Some(HitResult::Block(hit));
            state.hand_height = [1.0; 2];
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let sender = PacketSender::new(tx);
            let biome_climate = HashMap::new();
            let mut effects = BreakEffects {
                particles: &mut particles,
                registry: &registry,
                biome_climate: &biome_climate,
            };
            let stack = ItemStackData::new(ItemKind::Stone, 64);
            let player_pos = dvec3(0.5, 64.0, 0.5);
            let player = Aabb::from_center(player_pos, 0.3, 0.9);
            let mut dirty = Vec::new();
            assert!(state.start_use_item(
                &sender,
                &mut audio,
                &chunks,
                player_pos,
                player,
                player_pos + dvec3(0.0, 1.62, 0.0),
                LookDirection::default(),
                Some(stone),
                None,
                Some(&stack),
                20,
                false,
                false,
                &entities,
                InteractionHand::MainHand,
                false,
                None,
                false,
                false,
                false,
                false,
                &mut effects,
                &mut dirty,
            ));
            match rx.try_recv().unwrap() {
                Outbound::Packet(packet) => match *packet {
                    ServerboundGamePacket::UseItemOn(packet) => {
                        assert!(packet.block_hit.inside);
                        assert_eq!(packet.block_hit.location, azalea_vec3(hit.hit_point));
                    }
                    packet => panic!("expected UseItemOn, got {packet:?}"),
                },
                _ => panic!("expected typed packet"),
            }
            assert!(
                matches!(rx.try_recv(), Ok(Outbound::Packet(packet)) if matches!(*packet, ServerboundGamePacket::Swing(_)))
            );
            assert!(rx.try_recv().is_err());
            assert_eq!(chunks.get_block_state(pos.x, pos.y, pos.z), stone);
            assert_eq!(dirty, vec![pos]);
            assert_eq!(state.pending_predictions[&pos].state, previous);
            dirty.clear();
            assert_eq!(state.acknowledge(1, &chunks, player, &mut dirty), None);
            assert_eq!(chunks.get_block_state(pos.x, pos.y, pos.z), previous);
            assert_eq!(dirty, vec![pos]);
            state.update_placement_heights(Some(&stack), None);
            assert_eq!(state.hand_height, [1.0; 2]);
            assert!(state.pending_placement_uses.iter().all(Option::is_none));
            // A later unrelated count change must not lower a rejected placement.
            state.update_placement_heights(Some(&ItemStackData::new(ItemKind::Stone, 63)), None);
            assert_eq!(state.hand_height, [1.0; 2]);
        }
    }

    #[test]
    fn immediate_edit_prediction_repick_collision_and_ack_rollback() {
        let _protocol = crate::world::block::test_protocol_guard();
        use crate::net::sender::Outbound;
        use crate::physics::collision::no_collision;

        let (mut chunks, mut audio, entities, mut particles, registry) = headless_use_fixture();
        let col = azalea_core::position::ChunkPos::new(-1, 0);
        let mut column = azalea_world::Chunk::default();
        column.sections = vec![Default::default(); chunks.section_count() as usize].into();
        chunks.load_decoded_chunk(col, column);
        let stone = crate::world::block::first_state_of("stone").unwrap();
        let dirt = crate::world::block::first_state_of("dirt").unwrap();
        let pos = BlockPos::new(-1, -48, 2); // negative column + section boundary
        let behind = BlockPos::new(-1, -48, 3);
        chunks.set_block_state(pos.x, pos.y, pos.z, stone);
        chunks.set_block_state(behind.x, behind.y, behind.z, stone);
        let border = crate::world::border::WorldBorder::default();
        let eye = Position::new(-0.5, -47.5, 0.5);
        let look = LookDirection::default();
        let probe = Aabb::from_center(dvec3(-0.5, -47.5, 2.5), 0.2, 0.2);
        let player_pos = dvec3(-0.5, -49.0, 0.5);
        let player = Aabb::from_center(player_pos, 0.3, 0.9);
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let sender = PacketSender::new(tx);
        let stack = ItemStackData::new(ItemKind::Dirt, 64);
        let biome_climate = HashMap::new();
        let mut effects = BreakEffects {
            particles: &mut particles,
            registry: &registry,
            biome_climate: &biome_climate,
        };
        let mut state = InteractionState::new();
        let mut dirty = Vec::new();
        let mut before = Vec::new();
        for previous in [stone, dirt, dirt] {
            state.update_target(eye, look, &chunks, &entities, true, &border);
            let Some(HitResult::Block(hit)) = state.target else {
                panic!("missing target")
            };
            assert_eq!(hit.block_pos, pos);
            assert!(!no_collision(&chunks, &probe));
            state.start_destroy_block(
                hit,
                &chunks,
                &sender,
                &mut audio,
                player_pos,
                true,
                true,
                None,
                &mut effects,
                &mut dirty,
            );
            assert!(
                matches!(rx.try_recv(), Ok(Outbound::Packet(p)) if matches!(&*p,
                ServerboundGamePacket::PlayerAction(action) if action.pos == pos))
            );
            assert_eq!(chunks.get_block_state(pos.x, pos.y, pos.z), BlockState::AIR);
            assert!(no_collision(&chunks, &probe)); // same world, no physics tick/mesh wait
            state.update_target(eye, look, &chunks, &entities, true, &border);
            let Some(HitResult::Block(hit)) = state.target else {
                panic!("missing exposed target")
            };
            assert_eq!(hit.block_pos, behind);
            assert_eq!(hit.face, Direction::North);
            assert!(state.start_use_item(
                &sender,
                &mut audio,
                &chunks,
                player_pos,
                player,
                eye.into(),
                look,
                Some(dirt),
                None,
                Some(&stack),
                20,
                true,
                false,
                &entities,
                InteractionHand::MainHand,
                false,
                None,
                false,
                false,
                false,
                false,
                &mut effects,
                &mut dirty
            ));
            assert!(
                matches!(rx.try_recv(), Ok(Outbound::Packet(p)) if matches!(&*p,
                ServerboundGamePacket::UseItemOn(packet) if packet.block_hit.block_pos == behind
                    && packet.block_hit.direction == Direction::North))
            );
            assert!(
                matches!(rx.try_recv(), Ok(Outbound::Packet(p)) if matches!(*p,
                ServerboundGamePacket::Swing(_)))
            );
            assert_eq!(chunks.get_block_state(pos.x, pos.y, pos.z), dirt);
            assert!(!no_collision(&chunks, &probe)); // overlap remains accepted
            state.update_target(eye, look, &chunks, &entities, true, &border);
            assert!(matches!(state.target, Some(HitResult::Block(hit)) if hit.block_pos == pos));
            before.extend([(pos, previous), (pos, BlockState::AIR)]);
        }
        assert_eq!(dirty, vec![pos]);
        assert_eq!(state.take_visual_edits(), before);
        assert!(state.take_visual_edits().is_empty());
        dirty.clear();
        state.acknowledge(state.seq - 1, &chunks, player, &mut dirty);
        assert!(dirty.is_empty());
        assert!(state.take_visual_edits().is_empty()); // old ACK does not retire/update delta
        state.acknowledge(state.seq, &chunks, player, &mut dirty);
        assert_eq!(dirty, vec![pos]);
        assert_eq!(state.take_visual_edits(), vec![(pos, dirt)]); // rollback re-enters delta
        assert_eq!(chunks.get_block_state(pos.x, pos.y, pos.z), stone);
        assert!(!no_collision(&chunks, &probe));
        state.update_target(eye, look, &chunks, &entities, true, &border);
        assert!(matches!(state.target, Some(HitResult::Block(hit)) if hit.block_pos == pos));
    }

    #[test]
    fn consecutive_break_then_place_keeps_latest_prediction_until_its_ack() {
        let _protocol = crate::world::block::test_protocol_guard();
        for confirmed in [false, true] {
            let (chunks, mut audio, entities, mut particles, registry) = headless_use_fixture();
            let stone = crate::world::block::first_state_of("stone").unwrap();
            let dirt = crate::world::block::first_state_of("dirt").unwrap();
            let pos = BlockPos::new(2, 64, 2);
            chunks.set_block_state(2, 63, 2, stone);
            chunks.set_block_state(pos.x, pos.y, pos.z, stone);
            let player_pos = dvec3(0.5, 64.0, 0.5);
            let player = Aabb::from_center(player_pos, 0.3, 0.9);
            let biome_climate = HashMap::new();
            let mut effects = BreakEffects {
                particles: &mut particles,
                registry: &registry,
                biome_climate: &biome_climate,
            };
            let mut state = InteractionState::new();
            let mut dirty = Vec::new();
            state.hand_height = [1.0; 2];
            state.seq = 1;
            state.predict_destroy(
                pos,
                stone,
                player_pos,
                &chunks,
                &mut audio,
                &mut effects,
                &mut dirty,
                &crate::movement_record::Recorder::default(),
            );
            assert!(is_air(chunks.get_block_state(pos.x, pos.y, pos.z)));
            assert_eq!(dirty, vec![pos]);
            state.target = Some(HitResult::Block(BlockHitResult {
                block_pos: BlockPos::new(2, 63, 2),
                face: Direction::Up,
                hit_point: dvec3(2.5, 64.0, 2.5),
                inside: false,
                world_border: false,
            }));
            let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
            let sender = PacketSender::new(tx);
            let stack = ItemStackData::new(ItemKind::Dirt, 64);
            assert!(state.start_use_item(
                &sender,
                &mut audio,
                &chunks,
                player_pos,
                player,
                player_pos + dvec3(0.0, 1.62, 0.0),
                LookDirection::default(),
                Some(dirt),
                None,
                Some(&stack),
                20,
                false,
                false,
                &entities,
                InteractionHand::MainHand,
                false,
                None,
                false,
                false,
                false,
                false,
                &mut effects,
                &mut dirty,
            ));
            assert_eq!(chunks.get_block_state(pos.x, pos.y, pos.z), dirt);
            assert_eq!(state.pending_predictions[&pos].seq, 2);
            assert_eq!(state.pending_predictions[&pos].state, stone);
            dirty.clear();
            state.acknowledge(1, &chunks, player, &mut dirty);
            assert!(dirty.is_empty());
            assert_eq!(chunks.get_block_state(pos.x, pos.y, pos.z), dirt);
            assert!(state.pending_predictions.contains_key(&pos));
            if confirmed {
                assert!(state.update_known_server_state(&pos, dirt));
            }
            state.acknowledge(2, &chunks, player, &mut dirty);
            assert_eq!(
                chunks.get_block_state(pos.x, pos.y, pos.z),
                if confirmed { dirt } else { stone }
            );
            assert_eq!(dirty, if confirmed { vec![] } else { vec![pos] });
            assert!(state.pending_predictions.is_empty());
            state.update_placement_heights(Some(&stack), None);
            assert_eq!(state.hand_height, [1.0; 2]);
            assert_eq!(state.pending_placement_uses[0].is_some(), confirmed);
            // A successful ACK before a later inventory packet must still lower
            // the main hand; a rejected edit must not react to unrelated changes.
            state.update_placement_heights(Some(&ItemStackData::new(ItemKind::Dirt, 63)), None);
            assert_eq!(
                state.hand_height,
                if confirmed { [0.0, 1.0] } else { [1.0; 2] }
            );
        }
    }

    #[test]
    fn start_use_item_multistate_reconciles_both_hands_and_ignores_old_ack() {
        let _protocol = crate::world::block::test_protocol_guard();
        use InteractionHand::{MainHand, OffHand};

        use crate::net::sender::Outbound;
        for hand in [MainHand, OffHand] {
            for response in [None, Some("oak_slab"), Some("dirt")] {
                let (chunks, mut audio, entities, mut particles, registry) = headless_use_fixture();
                let pos = BlockPos::new(2, 65, 2);
                let original = crate::world::block::default_state_of("stone").unwrap();
                let predicted = registry.placeable_block_for_item("oak_slab").unwrap();
                assert_ne!(
                    predicted,
                    crate::world::block::first_state_of("oak_slab").unwrap()
                );
                chunks.set_block_state(2, 64, 2, original);
                chunks.set_block_state(pos.x, pos.y, pos.z, original); // occupied
                let player_pos = dvec3(2.5, 65.0, 2.5); // overlaps the target
                let player = Aabb::from_center(player_pos, 0.3, 0.9);
                let mut state = InteractionState::new();
                state.target = Some(HitResult::Block(BlockHitResult {
                    block_pos: BlockPos::new(2, 64, 2),
                    face: Direction::Up,
                    hit_point: dvec3(2.5, 65.0, 2.5),
                    inside: false,
                    world_border: true,
                }));
                let stack = ItemStackData::new(ItemKind::OakSlab, 64);
                let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
                let sender = PacketSender::new(tx);
                let biome_climate = HashMap::new();
                let mut effects = BreakEffects {
                    particles: &mut particles,
                    registry: &registry,
                    biome_climate: &biome_climate,
                };
                let mut dirty = Vec::new();
                for seq in 1..=2 {
                    assert!(state.start_use_item(
                        &sender,
                        &mut audio,
                        &chunks,
                        player_pos,
                        player,
                        player_pos + dvec3(0.0, 1.62, 0.0),
                        LookDirection::default(),
                        None,
                        None, // unknown placement state
                        if hand == MainHand { Some(&stack) } else { None },
                        20,
                        false,
                        false,
                        &entities,
                        MainHand,
                        true,
                        if hand == OffHand { Some(&stack) } else { None },
                        true,
                        false,
                        false,
                        false,
                        &mut effects,
                        &mut dirty
                    ));
                    let latest = if hand == MainHand { seq } else { seq * 2 };
                    assert_eq!(state.pending_predictions[&pos].seq, latest);
                    assert_eq!(state.pending_predictions[&pos].state, original);
                    assert_eq!(chunks.get_block_state(pos.x, pos.y, pos.z), predicted);
                    assert_eq!(stack.count, 64);
                }
                let mut swings = Vec::new();
                let mut on = Vec::new();
                while let Ok(Outbound::Packet(packet)) = rx.try_recv() {
                    match *packet {
                        ServerboundGamePacket::Swing(packet) => swings.push(packet.hand),
                        ServerboundGamePacket::UseItemOn(packet) => {
                            assert!(packet.block_hit.world_border);
                            on.push((packet.hand, packet.seq));
                        }
                        packet => panic!("unexpected {packet:?}"),
                    }
                }
                assert_eq!(swings, vec![hand; 2]);
                assert_eq!(
                    on,
                    if hand == MainHand {
                        vec![(MainHand, 1), (MainHand, 2)]
                    } else {
                        vec![(MainHand, 1), (OffHand, 2), (MainHand, 3), (OffHand, 4)]
                    }
                );
                assert_eq!(dirty, vec![pos]);
                dirty.clear();
                let latest = state.seq;
                state.acknowledge(latest - 1, &chunks, player, &mut dirty);
                assert!(dirty.is_empty());
                assert_eq!(chunks.get_block_state(pos.x, pos.y, pos.z), predicted);
                assert!(state.pending_predictions.contains_key(&pos));
                let authoritative =
                    response.map(|name| crate::world::block::default_state_of(name).unwrap());
                if let Some(server) = authoritative {
                    // Shared absorption path used by both single/section server updates.
                    assert!(state.update_known_server_state(&pos, server));
                    assert_eq!(chunks.get_block_state(pos.x, pos.y, pos.z), predicted);
                }
                state.acknowledge(latest, &chunks, player, &mut dirty);
                let expected = authoritative.unwrap_or(original);
                assert_eq!(chunks.get_block_state(pos.x, pos.y, pos.z), expected);
                assert_eq!(
                    dirty,
                    if expected == predicted {
                        vec![]
                    } else {
                        vec![pos]
                    }
                );
                assert!(state.pending_predictions.is_empty());
            }
        }
    }

    #[test]
    fn placement_state_predicts_basic_orientation_and_wall_variants() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        for (name, face, key, expected) in [
            ("oak_log", Direction::East, "axis", "x"),
            ("oak_log", Direction::North, "axis", "z"),
            ("oak_log", Direction::Up, "axis", "y"),
            ("oak_stairs", Direction::Up, "facing", "north"),
            ("oak_sign", Direction::Up, "rotation", "8"),
            ("torch", Direction::East, "facing", "east"),
            ("soul_torch", Direction::West, "facing", "west"),
            ("redstone_torch", Direction::North, "facing", "north"),
            ("oak_sign", Direction::South, "facing", "south"),
        ] {
            let state = placement_state(
                crate::world::block::default_state_of(name).unwrap(),
                face,
                LookDirection::default(),
            );
            assert_eq!(
                crate::world::block::block_properties(state).get(key),
                Some(expected),
                "{name}"
            );
            if name.ends_with("torch") {
                assert!(crate::world::block::block_id(state).ends_with("wall_torch"));
            } else if name == "oak_sign" && face == Direction::South {
                assert_eq!(crate::world::block::block_id(state), "oak_wall_sign");
            }
        }
    }

    #[test]
    fn start_use_item_unwritable_targets_succeed_without_inventing_predictions() {
        use InteractionHand::{MainHand, OffHand};

        use crate::net::sender::Outbound;
        for hand in [MainHand, OffHand] {
            for unloaded in [false, true] {
                let (chunks, mut audio, entities, mut particles, registry) = headless_use_fixture();
                let hit = BlockHitResult {
                    block_pos: if unloaded {
                        BlockPos::new(15, 64, 2)
                    } else {
                        BlockPos::new(2, chunks.min_y() + chunks.height() as i32 - 1, 2)
                    },
                    face: if unloaded {
                        Direction::East
                    } else {
                        Direction::Up
                    },
                    hit_point: dvec3(2.5, 65.0, 2.5),
                    inside: false,
                    world_border: false,
                };
                let stone = registry.placeable_block_for_item("stone").unwrap();
                chunks.set_block_state(hit.block_pos.x, hit.block_pos.y, hit.block_pos.z, stone);
                let pos = hit.block_pos.offset_with_direction(hit.face);
                let mut state = InteractionState::new();
                state.hand_height = [1.0; 2];
                state.target = Some(HitResult::Block(hit));
                let stack = ItemStackData::new(ItemKind::Stone, 64);
                let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
                let sender = PacketSender::new(tx);
                let biome_climate = HashMap::new();
                let mut effects = BreakEffects {
                    particles: &mut particles,
                    registry: &registry,
                    biome_climate: &biome_climate,
                };
                let player_pos = dvec3(0.5, 64.0, 0.5);
                let mut dirty = Vec::new();
                assert!(state.start_use_item(
                    &sender,
                    &mut audio,
                    &chunks,
                    player_pos,
                    Aabb::from_center(player_pos, 0.3, 0.9),
                    player_pos + dvec3(0.0, 1.62, 0.0),
                    LookDirection::default(),
                    Some(stone),
                    Some(stone),
                    if hand == MainHand { Some(&stack) } else { None },
                    20,
                    true,
                    false,
                    &entities,
                    MainHand,
                    false,
                    if hand == OffHand { Some(&stack) } else { None },
                    false,
                    false,
                    false,
                    false,
                    &mut effects,
                    &mut dirty
                ));
                assert!(dirty.is_empty());
                assert!(state.pending_predictions.is_empty());
                assert!(state.pending_placement_uses.iter().all(Option::is_none));
                assert!(is_air(chunks.get_block_state(pos.x, pos.y, pos.z)));
                assert!(state.swinging);
                assert_eq!(state.swinging_hand, hand);
                assert_eq!(state.hand_height[hand_index(hand)], 0.0);
                let mut swing_packets = 0;
                while let Ok(Outbound::Packet(packet)) = rx.try_recv() {
                    if let ServerboundGamePacket::Swing(packet) = *packet {
                        assert_eq!(packet.hand, hand);
                        swing_packets += 1;
                    }
                }
                assert_eq!(swing_packets, 1);
            }
        }
    }

    #[test]
    fn placement_target_does_not_treat_contextual_blocks_as_replaceable() {
        let _protocol = crate::world::block::test_protocol_guard();
        let (chunks, _) = border_test_world();
        let hit = BlockHitResult {
            block_pos: BlockPos::new(2, 64, 2),
            face: Direction::Up,
            hit_point: dvec3(2.5, 65.0, 2.5),
            inside: false,
            world_border: false,
        };
        let stone = crate::world::block::first_state_of("stone").unwrap();
        for state in [
            crate::world::block::find_state("oak_slab", &[("type", "bottom")]),
            crate::world::block::find_state("snow", &[("layers", "2")]),
        ] {
            chunks.set_block_state(2, 64, 2, state);
            chunks.set_block_state(2, 65, 2, stone);
            assert_eq!(
                placement_target(hit, "stone", &chunks),
                Some((BlockPos::new(2, 65, 2), stone))
            );
        }
        let snow = crate::world::block::find_state("snow", &[("layers", "1")]);
        chunks.set_block_state(2, 64, 2, snow);
        assert_eq!(
            placement_target(hit, "stone", &chunks),
            Some((hit.block_pos, snow))
        );
        assert!(!can_replace_for_item(
            crate::world::block::first_state_of("vine").unwrap(),
            "vine"
        ));
        assert!(crate::world::block::registry::block_for_item("stick").is_none());
        assert_eq!(
            crate::world::block::registry::block_for_item("redstone"),
            Some("redstone_wire")
        );
        let registry = BlockRegistry::test_empty();
        assert_eq!(
            registry.placeable_block_for_item("torch"),
            crate::world::block::default_state_of("torch")
        );
        assert_eq!(
            registry.placeable_block_for_item("oak_slab"),
            crate::world::block::default_state_of("oak_slab")
        );
        assert_ne!(
            registry.placeable_block_for_item("oak_slab"),
            crate::world::block::first_state_of("oak_slab")
        );
    }

    #[test]
    fn bow_press_hold_release_packets_and_renderer_stages() {
        use azalea_inventory::ItemStack;
        use winit::event::{ElementState, MouseButton};

        use crate::net::sender::Outbound;
        use crate::player::inventory::{HOTBAR_START, Inventory, OFFHAND};
        use crate::renderer::pipelines::held_item::selected_item_model_name;

        for (name, arrow_slot, creative) in [
            ("survival arrow slot 40", Some(40), false),
            ("survival arrow slot 0", Some(0), false),
            ("survival arrow offhand", Some(OFFHAND), false),
            ("survival without arrows", None, false),
            ("creative without arrows", None, true),
        ] {
            let (chunks, mut audio, entities, mut particles, registry) = headless_use_fixture();
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let sender = PacketSender::new(tx);
            let biome_climate = HashMap::new();
            let mut effects = BreakEffects {
                particles: &mut particles,
                registry: &registry,
                biome_climate: &biome_climate,
            };
            let mut inventory = Inventory::new();
            inventory.set_slot(
                HOTBAR_START,
                ItemStack::Present(ItemStackData::new(ItemKind::Bow, 1)),
            );
            if let Some(slot) = arrow_slot {
                inventory.set_slot(
                    slot,
                    ItemStack::Present(ItemStackData::new(ItemKind::Arrow, 1)),
                );
            }
            // tick receives this boolean from app/core.rs, not an Inventory.
            // Mirror that boundary's full-slot scan (including 0, 40 and offhand).
            let has_projectile = inventory.slots().iter().any(|stack| {
                matches!(stack, ItemStack::Present(data) if data.count > 0 && matches!(
                    data.kind, ItemKind::Arrow | ItemKind::SpectralArrow | ItemKind::TippedArrow
                ))
            });
            assert_eq!(has_projectile, arrow_slot.is_some(), "{name}");
            let offhand = match inventory.offhand() {
                ItemStack::Present(data) if data.count > 0 => Some(data),
                _ => None,
            };
            let player_pos = dvec3(0.5, 64.0, 0.5);
            let eye_pos = player_pos + dvec3(0.0, 1.62, 0.0);
            assert!(is_air(chunks.get_block_state(0, 65, 0)), "{name}");
            let mut state = InteractionState::new();
            assert!(state.target.is_none(), "air use: {name}");
            // released() skips the input path; new() captures the cursor without
            // a window, but does initialize optional gilrs controller support.
            let mut input = InputState::new();
            assert!(input.is_cursor_captured(), "{name}");
            CONSUME_SOUND_REQUESTS.with(|count| count.set(0));

            let mut tick =
                |state: &mut InteractionState, input: &InputState, audio: &mut AudioEngine| {
                    let dirty = state.tick(
                        input,
                        &chunks,
                        &sender,
                        audio,
                        player_pos,
                        Aabb::from_center(player_pos, 0.3, 0.9),
                        eye_pos,
                        LookDirection::default(),
                        true,
                        creative,
                        false,
                        &entities,
                        InteractionHand::MainHand,
                        false,
                        MAX_FOOD_LEVEL,
                        input.selected_slot(),
                        inventory.held_stack(input.selected_slot()),
                        offhand,
                        has_projectile,
                        None,
                        None,
                        false,
                        false,
                        &crate::player::LocalPlayer::new(),
                        &mut effects,
                    );
                    assert!(dirty.is_empty(), "air use must not dirty blocks: {name}");
                };
            assert_eq!(
                selected_item_model_name("bow", state.use_animation(1.0), false),
                "bow",
            );
            input.on_mouse_button(MouseButton::Right, ElementState::Pressed);
            assert!(input.action_just_pressed(input::Action::Use), "{name}");
            tick(&mut state, &input, &mut audio);
            match rx.try_recv().expect("one bow press packet") {
                Outbound::Packet(packet) => match *packet {
                    ServerboundGamePacket::UseItem(packet) => {
                        assert_eq!(packet.hand, InteractionHand::MainHand, "{name}");
                        assert_eq!(packet.seq, 1, "{name}");
                    }
                    packet => panic!("{name}: unexpected press packet {packet:?}"),
                },
                _ => panic!("{name}: expected typed UseItem packet"),
            }
            assert!(rx.try_recv().is_err(), "extra press packet: {name}");
            input.clear_just_pressed_actions();
            assert!(!input.action_just_pressed(input::Action::Use), "{name}");
            assert!(input.performing_action(input::Action::Use), "{name}");

            let can_use = has_projectile || creative;
            assert_eq!(state.using_item.is_some(), can_use, "{name}");
            assert_eq!(
                state.using_bow, can_use,
                "failed use must not latch: {name}"
            );
            if can_use {
                // The press tick already runs update_using_item once. Use the
                // real timer and renderer bridge at the end of each tick (+1
                // partial tick cancels use_animation's interpolation offset).
                for elapsed in 1..=18 {
                    if elapsed > 1 {
                        tick(&mut state, &input, &mut audio);
                    }
                    let active = state.using_item.as_ref().expect("held bow remains active");
                    assert!(active.bow, "{name}");
                    assert_eq!(active.kind, ItemKind::Bow, "{name}");
                    assert_eq!(active.hand, InteractionHand::MainHand, "{name}");
                    assert_eq!(active.duration - active.remaining, elapsed, "{name}");
                    let anim = state.use_animation(1.0).expect("active bow animation");
                    assert!(anim.bow && !anim.left_hand, "{name}");
                    assert_eq!(
                        anim.duration - anim.curr_usage_time,
                        elapsed as f32,
                        "{name}"
                    );
                    let expected = match elapsed {
                        1..=12 => "bow_pulling_0",
                        13..=17 => "bow_pulling_1",
                        _ => "bow_pulling_2",
                    };
                    // Calls the same selector -> bow_model_name used by update_and_draw.
                    assert_eq!(
                        selected_item_model_name("bow", Some(anim), false),
                        expected,
                        "{name}: tick {elapsed}",
                    );
                    assert_eq!(
                        selected_item_model_name("bow", Some(anim), true),
                        "bow",
                        "inactive offhand: {name}",
                    );
                    assert!(rx.try_recv().is_err(), "held bow resent UseItem: {name}");
                }
            } else {
                assert!(state.use_animation(1.0).is_none(), "{name}");
                assert_eq!(
                    selected_item_model_name("bow", state.use_animation(1.0), false),
                    "bow",
                );
            }

            input.on_mouse_button(MouseButton::Right, ElementState::Released);
            tick(&mut state, &input, &mut audio);
            if can_use {
                match rx.try_recv().expect("one release packet") {
                    Outbound::Packet(packet) => match *packet {
                        ServerboundGamePacket::PlayerAction(packet) => {
                            assert!(matches!(packet.action, Action::ReleaseUseItem), "{name}");
                            assert_eq!(packet.pos, BlockPos::new(0, 0, 0), "{name}");
                            assert_eq!(packet.direction, Direction::Down, "{name}");
                            assert_eq!(packet.seq, 0, "{name}");
                        }
                        packet => panic!("{name}: unexpected release packet {packet:?}"),
                    },
                    _ => panic!("{name}: expected typed ReleaseUseItem packet"),
                }
            }
            assert!(state.using_item.is_none() && !state.using_bow, "{name}");
            assert!(state.use_animation(1.0).is_none(), "{name}");
            assert_eq!(
                selected_item_model_name("bow", state.use_animation(1.0), false),
                "bow",
            );
            input.clear_just_pressed_actions();
            for _ in 0..5 {
                tick(&mut state, &input, &mut audio);
            }
            assert!(rx.try_recv().is_err(), "duplicate/spurious release: {name}");
            CONSUME_SOUND_REQUESTS.with(|count| {
                assert_eq!(
                    count.get(),
                    0,
                    "bow must not request bite/completion sounds: {name}",
                );
            });
        }
    }

    #[test]
    fn charge_and_latched_item_packets_follow_button_edges() {
        use azalea_inventory::ItemStack;
        use azalea_inventory::components::{ChargedProjectiles, DataComponentUnion};
        use azalea_registry::builtin::DataComponentKind;
        use winit::event::{ElementState, MouseButton};

        use crate::net::sender::Outbound;

        let (chunks, mut audio, entities, mut particles, registry) = headless_use_fixture();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let sender = PacketSender::new(tx);
        let biome_climate = HashMap::new();
        let mut effects = BreakEffects {
            particles: &mut particles,
            registry: &registry,
            biome_climate: &biome_climate,
        };
        let pos = dvec3(0.5, 64.0, 0.5);
        let eye = pos + dvec3(0.0, 1.62, 0.0);
        let mut input = InputState::new();
        let mut tick =
            |state: &mut InteractionState, input: &InputState, held: Option<&ItemStackData>| {
                state.tick(
                    input,
                    &chunks,
                    &sender,
                    &mut audio,
                    pos,
                    Aabb::from_center(pos, 0.3, 0.9),
                    eye,
                    LookDirection::default(),
                    true,
                    false,
                    false,
                    &entities,
                    InteractionHand::MainHand,
                    false,
                    MAX_FOOD_LEVEL,
                    input.selected_slot(),
                    held,
                    None,
                    false,
                    None,
                    None,
                    false,
                    true,
                    &crate::player::LocalPlayer::new(),
                    &mut effects,
                );
            };
        macro_rules! expect_use {
            () => {
                match rx.try_recv().expect("one UseItem packet") {
                    Outbound::Packet(packet) => {
                        assert!(matches!(*packet, ServerboundGamePacket::UseItem(_)))
                    }
                    _ => panic!("expected a typed UseItem packet"),
                }
            };
        }
        macro_rules! expect_release {
            () => {
                match rx.try_recv().expect("one release packet") {
                    Outbound::Packet(packet) => match *packet {
                        ServerboundGamePacket::PlayerAction(packet) => {
                            assert!(matches!(packet.action, Action::ReleaseUseItem));
                        }
                        packet => panic!("expected ReleaseUseItem, got {packet:?}"),
                    },
                    _ => panic!("expected a typed ReleaseUseItem packet"),
                }
            };
        }

        // Uncharged crossbow: start once, don't resend while charging, release once.
        let crossbow = ItemStackData::new(ItemKind::Crossbow, 1);
        let mut state = InteractionState::new();
        input.on_mouse_button(MouseButton::Right, ElementState::Pressed);
        tick(&mut state, &input, Some(&crossbow));
        expect_use!();
        assert_eq!(
            state.using_item.as_ref().map(|active| active.use_kind),
            Some(ActiveUseKind::CrossbowCharge)
        );
        input.clear_just_pressed_actions();
        for _ in 0..30 {
            tick(&mut state, &input, Some(&crossbow));
            assert!(rx.try_recv().is_err(), "held crossbow resent UseItem");
        }
        input.on_mouse_button(MouseButton::Right, ElementState::Released);
        tick(&mut state, &input, Some(&crossbow));
        expect_release!();

        // Server completion can clear the using-item bit before physical button-up;
        // the independent latch must still prevent a fire/reload cycle.
        state = InteractionState::new();
        input.on_mouse_button(MouseButton::Right, ElementState::Pressed);
        tick(&mut state, &input, Some(&crossbow));
        expect_use!();
        state.sync_using_item_flag(false);
        input.clear_just_pressed_actions();
        for _ in 0..30 {
            tick(&mut state, &input, Some(&crossbow));
            assert!(
                rx.try_recv().is_err(),
                "server-completed crossbow retriggered"
            );
        }
        input.on_mouse_button(MouseButton::Right, ElementState::Released);
        tick(&mut state, &input, Some(&crossbow));
        assert!(
            rx.try_recv().is_err(),
            "server-completed crossbow sent duplicate release"
        );

        // A server-reported charged projectile makes this press a one-shot fire;
        // holding cannot retrigger and button-up needs no release action packet.
        let mut loaded = ItemStackData::new(ItemKind::Crossbow, 1);
        let charged = ChargedProjectiles {
            items: vec![ItemStack::from(ItemKind::Arrow)],
        };
        // SAFETY: component union value matches ChargedProjectiles.
        unsafe {
            loaded.component_patch.unchecked_insert_component(
                DataComponentKind::ChargedProjectiles,
                Some(DataComponentUnion::from(charged)),
            );
        }
        state = InteractionState::new();
        input.on_mouse_button(MouseButton::Right, ElementState::Pressed);
        tick(&mut state, &input, Some(&loaded));
        expect_use!();
        assert_eq!(
            state.using_item.as_ref().map(|active| active.use_kind),
            Some(ActiveUseKind::CrossbowFire)
        );
        input.clear_just_pressed_actions();
        for _ in 0..12 {
            tick(&mut state, &input, Some(&loaded));
            assert!(rx.try_recv().is_err(), "held charged crossbow fired again");
        }
        input.on_mouse_button(MouseButton::Right, ElementState::Released);
        tick(&mut state, &input, Some(&loaded));
        assert!(
            rx.try_recv().is_err(),
            "instant crossbow fire sent release action"
        );

        // A press and release before the tick still starts use once; the next
        // tick releases it, with custom UseEffects applied to the active item.
        let mut trident = ItemStackData::new(ItemKind::Trident, 1);
        let mut effects = UseEffects::new();
        effects.can_sprint = true;
        effects.interact_vibrations = false;
        effects.speed_multiplier = 0.65;
        // SAFETY: union value matches UseEffects.
        unsafe {
            trident.component_patch.unchecked_insert_component(
                DataComponentKind::UseEffects,
                Some(DataComponentUnion::from(effects)),
            );
        }
        state = InteractionState::new();
        input.on_mouse_button(MouseButton::Right, ElementState::Pressed);
        input.on_mouse_button(MouseButton::Right, ElementState::Released);
        assert!(input.action_just_pressed(input::Action::Use));
        assert!(!input.performing_action(input::Action::Use));
        tick(&mut state, &input, Some(&trident));
        expect_use!();
        assert_eq!(state.use_speed_multiplier(), 0.65);
        assert!(!state.slow_due_to_using_item());
        assert_eq!(
            state
                .using_item
                .as_ref()
                .map(|active| active.use_effects.interact_vibrations),
            Some(false)
        );
        assert!(rx.try_recv().is_err(), "short click sent duplicate use");
        input.clear_just_pressed_actions();
        tick(&mut state, &input, Some(&trident));
        expect_release!();
        assert!(rx.try_recv().is_err(), "short click sent duplicate release");
        tick(&mut state, &input, Some(&trident));
        assert!(
            rx.try_recv().is_err(),
            "short click repeated use after release"
        );

        // Removing UseEffects from a trident must not revive its prototype override.
        let mut removed_effects = trident.clone();
        unsafe {
            removed_effects
                .component_patch
                .unchecked_insert_component(DataComponentKind::UseEffects, None);
        }
        state = InteractionState::new();
        input.on_mouse_button(MouseButton::Right, ElementState::Pressed);
        tick(&mut state, &input, Some(&removed_effects));
        expect_use!();
        assert_eq!(
            state.using_item.as_ref().unwrap().use_effects,
            UseEffects::default()
        );

        // At/above the vanilla threshold the wire sequence is still one start
        // and one release; server-side releaseUsing decides whether to throw.
        state = InteractionState::new();
        input.on_mouse_button(MouseButton::Right, ElementState::Pressed);
        tick(&mut state, &input, Some(&trident));
        expect_use!();
        input.clear_just_pressed_actions();
        for _ in 0..10 {
            tick(&mut state, &input, Some(&trident));
            assert!(rx.try_recv().is_err(), "held trident resent UseItem");
        }
        input.on_mouse_button(MouseButton::Right, ElementState::Released);
        tick(&mut state, &input, Some(&trident));
        expect_release!();

        // Spyglass exposes a readable scope state and releases through the same
        // server-authoritative use packet lifecycle.
        let spyglass = ItemStackData::new(ItemKind::Spyglass, 1);
        state = InteractionState::new();
        input.on_mouse_button(MouseButton::Right, ElementState::Pressed);
        tick(&mut state, &input, Some(&spyglass));
        expect_use!();
        assert!(state.is_using_spyglass());
        input.on_mouse_button(MouseButton::Right, ElementState::Released);
        tick(&mut state, &input, Some(&spyglass));
        expect_release!();
        assert!(!state.is_using_spyglass());

        // Server cancel clears the scope state, but the physical-button latch
        // still prevents a held re-use until actual button-up.
        state = InteractionState::new();
        input.on_mouse_button(MouseButton::Right, ElementState::Pressed);
        tick(&mut state, &input, Some(&spyglass));
        expect_use!();
        state.sync_using_item_flag(false);
        assert!(!state.is_using_spyglass());
        input.clear_just_pressed_actions();
        for _ in 0..12 {
            tick(&mut state, &input, Some(&spyglass));
            assert!(
                rx.try_recv().is_err(),
                "server cancel allowed spyglass re-use while held"
            );
        }
        input.on_mouse_button(MouseButton::Right, ElementState::Released);
        tick(&mut state, &input, Some(&spyglass));
        assert!(
            rx.try_recv().is_err(),
            "server-stopped spyglass sent a duplicate release"
        );

        // Replacing an active item cancels its use once and doesn't use the new stack
        // in the same tick, even though the button remains held.
        let mut state = InteractionState::new();
        input.on_mouse_button(MouseButton::Right, ElementState::Pressed);
        tick(&mut state, &input, Some(&trident));
        expect_use!();
        input.clear_just_pressed_actions();
        let stone = ItemStackData::new(ItemKind::Stone, 1);
        tick(&mut state, &input, Some(&stone));
        expect_release!();
        for _ in 0..12 {
            tick(&mut state, &input, Some(&stone));
            assert!(
                rx.try_recv().is_err(),
                "held input used the replacement stack"
            );
        }
        input.on_mouse_button(MouseButton::Right, ElementState::Released);
        tick(&mut state, &input, Some(&stone));
        assert!(
            rx.try_recv().is_err(),
            "item swap emitted an extra use packet"
        );
    }

    #[test]
    fn offhand_use_falls_through_for_passed_main_hand_only() {
        assert!(should_try_offhand(true, false, true, true)); // stick + apple
        assert!(!should_try_offhand(true, false, false, true)); // shield/food succeeded
        assert!(should_try_offhand(false, true, true, true));
        assert!(!should_try_offhand(false, false, true, true));
        assert!(!should_try_offhand(true, false, true, false));
    }

    #[test]
    fn active_use_checks_the_stack_in_its_own_hand() {
        let main = ItemStackData::new(ItemKind::Stone, 1);
        let off = ItemStackData::new(ItemKind::Apple, 1);
        assert_eq!(
            stack_for_hand(InteractionHand::MainHand, Some(&main), Some(&off))
                .unwrap()
                .kind,
            ItemKind::Stone
        );
        assert_eq!(
            stack_for_hand(InteractionHand::OffHand, Some(&main), Some(&off))
                .unwrap()
                .kind,
            ItemKind::Apple
        );
    }

    /// Vanilla `isSameItemSameComponents`: count never matters, the item type
    /// does, and the empty hand only matches itself.
    #[test]
    fn protocol_hand_preserves_main_and_offhand() {
        assert_eq!(
            protocol_hand(InteractionHand::MainHand),
            wire::InteractionHand::MainHand
        );
        assert_eq!(
            protocol_hand(InteractionHand::OffHand),
            wire::InteractionHand::OffHand
        );
    }

    #[test]
    fn item_comparison_ignores_count() {
        let a = ItemStackData::new(ItemKind::Stone, 1);
        assert!(same_item_same_components(
            Some(&a),
            Some(&ItemStackData::new(ItemKind::Stone, 64))
        ));
        assert!(!same_item_same_components(
            Some(&a),
            Some(&ItemStackData::new(ItemKind::Dirt, 1))
        ));
        assert!(!same_item_same_components(Some(&a), None));
        assert!(same_item_same_components(None, None));
    }

    #[test]
    fn pick_dispatches_current_target_and_ignores_miss() {
        use crate::net::sender::Outbound;

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let sender = PacketSender::new(tx);
        let mut interaction = InteractionState::new();

        interaction.target = Some(HitResult::Block(BlockHitResult {
            block_pos: BlockPos::new(-1, 64, 3),
            face: Direction::North,
            hit_point: DVec3::ZERO,
            inside: false,
            world_border: false,
        }));
        interaction.pick_block_or_entity(&sender, true);
        match rx.try_recv().expect("block pick packet") {
            Outbound::Raw(bytes) => {
                assert_eq!(bytes, wire::encode_pick_item_from_block(-1, 64, 3, true))
            }
            _ => panic!("pick packet must use raw encoding"),
        }

        interaction.target = Some(HitResult::Entity(EntityHitResult {
            entity_id: 300,
            location: DVec3::ZERO,
            entity_pos: DVec3::ZERO,
        }));
        interaction.pick_block_or_entity(&sender, false);
        match rx.try_recv().expect("entity pick packet") {
            Outbound::Raw(bytes) => {
                assert_eq!(bytes, wire::encode_pick_item_from_entity(300, false));
            }
            _ => panic!("pick packet must use raw encoding"),
        }

        interaction.target = None;
        interaction.pick_block_or_entity(&sender, false);
        assert!(matches!(
            rx.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty)
        ));
    }

    #[test]
    fn ray_over_partial_block_misses_but_ray_onto_it_hits() {
        let slab_height = 0.5;
        let block = BlockPos::new(0, 0, 0);
        let bottom_slab: [LocalBox; 1] = [[0.0, 0.0, 0.0, 1.0, slab_height, 1.0]];
        let origin = dvec3(-1.0, 1.5, 0.5);

        let over_the_slab = origin + dvec3(4.0, -1.4, 0.0);
        let slab_hit = clip_shape(origin, over_the_slab, block, &bottom_slab);
        assert!(slab_hit.is_none());

        let onto_the_slab = origin + dvec3(3.0, -2.75, 0.0);
        let (hit_point, face, inside) =
            clip_shape(origin, onto_the_slab, block, &bottom_slab).unwrap();
        assert!(!inside);
        let tolerance = 1e-9;
        let is_on_slab_surface = (hit_point.y - slab_height).abs() < tolerance;
        assert!(is_on_slab_surface, "hit {hit_point:?}");
        assert_eq!(face, Direction::Up);
    }

    #[test]
    fn out_of_reach_entity_does_not_hide_a_closer_block_hit() {
        assert!(!entity_hit_wins(3.1 * 3.1, 4.0 * 4.0, ENTITY_REACH));
        assert!(entity_hit_wins(2.0 * 2.0, 4.0 * 4.0, ENTITY_REACH));
        assert!(!entity_hit_wins(2.0 * 2.0, 1.0 * 1.0, ENTITY_REACH));
    }

    /// Vanilla `VoxelShape.clip` reports the inside case at the probe point,
    /// not at the ray's origin.
    #[test]
    fn ray_starting_inside_partial_block_hits_immediately() {
        let block = BlockPos::new(0, 0, 0);
        let bottom_slab: [LocalBox; 1] = [[0.0, 0.0, 0.0, 1.0, 0.5, 1.0]];
        let inside_the_slab = dvec3(0.5, 0.25, 0.5);
        let ray = dvec3(0.0, -4.0, 0.0);

        let (hit_point, face, inside) =
            clip_shape(inside_the_slab, inside_the_slab + ray, block, &bottom_slab).unwrap();
        assert!(inside);
        assert_eq!(hit_point, inside_the_slab + ray * INSIDE_PROBE_FRACTION);
        assert_eq!(face, Direction::Up);
    }

    /// Vanilla clips straight through an empty shape (`LiquidBlock.getShape`),
    /// so the caller walks on to the block behind it.
    #[test]
    fn ray_passes_through_an_empty_shape() {
        let block = BlockPos::new(0, 0, 0);
        let from = dvec3(0.5, 2.0, 0.5);
        assert!(clip_shape(from, from + dvec3(0.0, -4.0, 0.0), block, &[]).is_none());
    }

    fn rule(blocks: Vec<BlockKind>, speed: Option<f32>, correct: Option<bool>) -> ToolRule {
        ToolRule {
            blocks: HolderSet::Direct { contents: blocks },
            speed,
            correct_for_drops: correct,
        }
    }

    /// Vanilla rule resolution: the first matching rule with the queried
    /// field wins, and each field resolves independently.
    #[test]
    fn tool_rules_first_match_per_field() {
        let tool = Tool {
            rules: vec![
                rule(vec![BlockKind::Obsidian], None, Some(false)),
                rule(
                    vec![BlockKind::Stone, BlockKind::Obsidian],
                    Some(4.0),
                    Some(true),
                ),
            ],
            default_mining_speed: 1.5,
            ..Tool::new()
        };
        assert_eq!(tool_mining_speed(&tool, BlockKind::Stone), 4.0);
        assert!(tool_correct_for_drops(&tool, BlockKind::Stone));
        // The speedless first rule is skipped for speed but wins for drops.
        assert_eq!(tool_mining_speed(&tool, BlockKind::Obsidian), 4.0);
        assert!(!tool_correct_for_drops(&tool, BlockKind::Obsidian));
        // No matching rule: default speed, not correct for drops.
        assert_eq!(tool_mining_speed(&tool, BlockKind::Dirt), 1.5);
        assert!(!tool_correct_for_drops(&tool, BlockKind::Dirt));
    }

    /// Anchor on azalea's `HolderSet::contains`: `Named` sets reference a
    /// block tag whose contents aren't on the wire and are never populated,
    /// so tool rules sent with tags conservatively match nothing (azalea's
    /// item defaults inline every tag as `Direct`).
    #[test]
    fn named_holder_set_matches_nothing() {
        let set: HolderSet<BlockKind, Identifier> = HolderSet::Named {
            key: Identifier::new("minecraft:mineable/pickaxe"),
            contents: vec![],
        };
        assert!(!set.contains(BlockKind::Stone));
    }

    /// The generated iron pickaxe default resolves like vanilla: fast and
    /// correct on stone, default speed on dirt.
    #[test]
    fn iron_pickaxe_default_tool() {
        let pickaxe = ItemStackData::new(ItemKind::IronPickaxe, 1);
        let tool = crate::player::menu_click::component::<Tool>(&pickaxe)
            .expect("iron pickaxe has a tool component");
        assert_eq!(tool_mining_speed(&tool, BlockKind::Stone), 6.0);
        assert!(tool_correct_for_drops(&tool, BlockKind::Stone));
        assert_eq!(tool_mining_speed(&tool, BlockKind::Dirt), 1.0);
        assert!(!tool_correct_for_drops(&tool, BlockKind::Dirt));
    }

    #[test]
    fn survival_instant_break_start_does_not_apply_creative_five_tick_delay() {
        use crate::net::sender::Outbound;

        let (chunks, mut audio, _, mut particles, registry) = headless_use_fixture();
        let pos = BlockPos::new(2, 64, 2);
        chunks.set_block_state(
            pos.x,
            pos.y,
            pos.z,
            crate::world::block::first_state_of("wheat").unwrap(),
        );
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let sender = PacketSender::new(tx);
        let biome_climate = HashMap::new();
        let mut effects = BreakEffects {
            particles: &mut particles,
            registry: &registry,
            biome_climate: &biome_climate,
        };
        let mut interaction = InteractionState::new();
        let mut dirty = Vec::new();
        interaction.start_destroy_block(
            BlockHitResult {
                block_pos: pos,
                face: Direction::Up,
                hit_point: dvec3(2.5, 65.0, 2.5),
                inside: false,
                world_border: false,
            },
            &chunks,
            &sender,
            &mut audio,
            DVec3::ZERO,
            true,
            false,
            None,
            &mut effects,
            &mut dirty,
        );
        assert_eq!(interaction.destroy_delay, 0);
        assert!(!interaction.is_destroying);
        assert_eq!(dirty, vec![pos]);
        assert!(matches!(
            rx.try_recv(),
            Ok(Outbound::Packet(packet))
                if matches!(*packet, ServerboundGamePacket::PlayerAction(ref action)
                    if action.action == Action::StartDestroyBlock)
        ));
    }

    #[test]
    fn catchup_ticks_consume_entity_attack_edge_once() {
        use winit::event::{ElementState, MouseButton};

        use crate::net::sender::Outbound;

        let (chunks, mut audio, entities, mut particles, registry) = headless_use_fixture();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let sender = PacketSender::new(tx);
        let biome_climate = HashMap::new();
        let mut effects = BreakEffects {
            particles: &mut particles,
            registry: &registry,
            biome_climate: &biome_climate,
        };
        let pos = dvec3(0.5, 64.0, 0.5);
        let eye = pos + dvec3(0.0, 1.62, 0.0);
        let mut input = crate::app::input::InputState::new();
        input.on_mouse_button(MouseButton::Left, ElementState::Pressed);
        let mut interaction = InteractionState::new();
        interaction.target = Some(HitResult::Entity(EntityHitResult {
            entity_id: 42,
            location: eye + dvec3(0.0, 0.0, 1.0),
            entity_pos: eye + dvec3(0.0, 0.0, 1.0),
        }));
        for _ in 0..10 {
            interaction.tick(
                &input,
                &chunks,
                &sender,
                &mut audio,
                pos,
                Aabb::from_center(pos, 0.3, 0.9),
                eye,
                LookDirection::default(),
                true,
                false,
                false,
                &entities,
                InteractionHand::MainHand,
                false,
                MAX_FOOD_LEVEL,
                0,
                None,
                None,
                false,
                None,
                None,
                false,
                true,
                &crate::player::LocalPlayer::new(),
                &mut effects,
            );
            input.clear_just_pressed_actions();
        }
        let mut packets = Vec::new();
        while let Ok(packet) = rx.try_recv() {
            packets.push(packet);
        }
        assert_eq!(
            packets
                .iter()
                .filter(|p| matches!(p, Outbound::Raw(_)))
                .count(),
            1
        );
    }

    #[test]
    fn neutral_gui_input_preserves_carried_slot_and_real_slot_changes_sync_once() {
        use crate::net::sender::Outbound;
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let sender = PacketSender::new(tx);
        let mut interaction = InteractionState::new();
        interaction.carried_slot = 8;

        let mut neutral = InputState::released();
        neutral.set_selected_slot(8);
        interaction.ensure_has_sent_carried_item(&sender, neutral.selected_slot());
        assert!(
            rx.try_recv().is_err(),
            "GUI neutral input must not resend slot 8"
        );

        interaction.ensure_has_sent_carried_item(&sender, 4);
        assert!(matches!(
            rx.try_recv(),
            Ok(Outbound::Packet(packet))
                if matches!(*packet, ServerboundGamePacket::SetCarriedItem(ref p) if p.slot == 4)
        ));
        assert!(rx.try_recv().is_err(), "a slot change sends one packet");

        interaction.ensure_has_sent_carried_item(&sender, 0);
        assert!(matches!(
            rx.try_recv(),
            Ok(Outbound::Packet(packet))
                if matches!(*packet, ServerboundGamePacket::SetCarriedItem(ref p) if p.slot == 0)
        ));
    }

    #[test]
    fn mining_uses_native_block_kind_instead_of_data_free_air_shim() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let oak_wood = crate::world::block::default_state_of("oak_wood").unwrap();
        let axe = ItemStackData::new(ItemKind::StoneAxe, 1);
        let tool = crate::player::menu_click::component::<Tool>(&axe).unwrap();
        assert_eq!(crate::world::block::block_id(oak_wood), "oak_wood");
        assert_eq!(oak_wood.as_block_kind(), BlockKind::Air);
        assert_eq!(
            destroy_progress(oak_wood, true, false, Some(&axe), MiningContext::default()),
            1.0 / 15.0
        );
        assert_eq!(
            destroy_progress(oak_wood, true, false, None, MiningContext::default()),
            1.0 / 60.0
        );
        assert_eq!(tool_mining_speed(&tool, "oak_wood".parse().unwrap()), 4.0);
        let modified = MiningContext {
            haste_amplifier: Some(0),
            fatigue_amplifier: Some(u8::MAX),
            ..MiningContext::default()
        };
        assert_eq!(
            destroy_progress(oak_wood, true, false, Some(&axe), modified),
            4.0 * 1.2 * 0.00081 / 2.0 / 30.0
        );

        let mut player = crate::player::LocalPlayer::new();
        let effect = |effect_id, amplifier| crate::mob_effect::MobEffectInstance {
            effect_id,
            amplifier,
            duration: -1,
            ambient: false,
            show_particles: true,
            show_icon: true,
        };
        player.effects.update(effect(2, 0)); // Haste I
        player.effects.update(effect(28, 1)); // Conduit Power II wins by amplifier
        player.effects.update(effect(3, u8::MAX)); // indefinite Fatigue IV+
        let context = MiningContext::from_player(&player);
        assert_eq!(context.haste_amplifier, Some(1));
        assert_eq!(context.fatigue_amplifier, Some(u8::MAX));
        player.effects.remove(28);

        let gold_ore = crate::world::block::default_state_of("gold_ore").unwrap();
        let golden_pickaxe = ItemStackData::new(ItemKind::GoldenPickaxe, 1);
        let gold_tool = crate::player::menu_click::component::<Tool>(&golden_pickaxe).unwrap();
        assert_eq!(
            tool_mining_speed(&gold_tool, "gold_ore".parse().unwrap()),
            12.0
        );
        assert!(!tool_correct_for_drops(
            &gold_tool,
            "gold_ore".parse().unwrap()
        ));
        let gold_context = MiningContext::from_player(&player);
        let increment =
            destroy_progress(gold_ore, true, false, Some(&golden_pickaxe), gold_context);
        assert_eq!(increment, 12.0 * 1.2 * 0.00081 / 3.0 / 100.0);
        assert!(increment * 25.0 < 1.0);

        player.set_attribute_value("minecraft:mining_efficiency", 2.0);
        player.set_attribute_value("minecraft:block_break_speed", 1.5);
        player.set_attribute_value("minecraft:submerged_mining_speed", 0.25);
        player.eyes_in_water = true;
        let context = MiningContext::from_player(&player);
        let water_increment =
            destroy_progress(gold_ore, true, false, Some(&golden_pickaxe), context);
        assert_eq!(
            water_increment,
            14.0 * 1.2 * 0.00081 * 1.5 * 0.25 / 3.0 / 100.0
        );
        assert_eq!(
            destroy_progress(
                oak_wood,
                true,
                false,
                None,
                MiningContext {
                    mining_efficiency: 100.0,
                    ..MiningContext::default()
                }
            ),
            1.0 / 60.0,
            "mining efficiency does not increase a base speed of 1",
        );
    }

    fn particle_test_store() -> ParticleStore {
        use std::sync::Arc;
        let mut atlas = crate::renderer::chunk::atlas::AtlasUVMap::test_empty();
        atlas.test_insert_particle_sprites("smoke", vec!["particle/smoke".into()]);
        atlas.test_insert_particle_sprites("portal", vec!["particle/portal".into()]);
        let colors = Arc::new(crate::renderer::chunk::mesher::Colormap::test_empty());
        ParticleStore::new(atlas, colors.clone(), colors.clone(), colors)
    }

    fn egg_test_chunks(with_floor: bool) -> ChunkStore {
        crate::world::block::init("26.2");
        let mut chunks = ChunkStore::new(1);
        for x in -1..=1 {
            for z in -1..=1 {
                let mut chunk = azalea_world::chunk::Chunk::default();
                chunk.sections = vec![Default::default(); chunks.section_count() as usize].into();
                chunks.load_decoded_chunk(azalea_core::position::ChunkPos::new(x, z), chunk);
            }
        }
        if with_floor {
            let stone = crate::world::block::default_state_of("stone").unwrap();
            for x in -7..=23 {
                for z in -7..=23 {
                    chunks.set_block_state(x, 64, z, stone);
                }
            }
        }
        chunks
    }

    #[test]
    fn candle_block_use_emits_one_smoke_per_lit_candle_then_predicts_unlit() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let pos = BlockPos::new(2, 64, 2);
        let mut chunks = ChunkStore::new(1);
        let mut chunk = azalea_world::chunk::Chunk::default();
        chunk.sections = vec![Default::default(); chunks.section_count() as usize].into();
        chunks.load_decoded_chunk(azalea_core::position::ChunkPos::new(0, 0), chunk);
        let lit =
            crate::world::block::find_state("cyan_candle", &[("candles", "3"), ("lit", "true")]);
        chunks.set_block_state(pos.x, pos.y, pos.z, lit);
        let mut particles = particle_test_store();
        let registry = BlockRegistry::test_empty();
        let climate = HashMap::new();
        let mut audio = AudioEngine::silent_for_test();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let sender = PacketSender::new(tx);
        let mut interaction = InteractionState::new();
        interaction.target = Some(HitResult::Block(BlockHitResult {
            block_pos: pos,
            face: Direction::Up,
            hit_point: dvec3(2.5, 65.0, 2.5),
            inside: false,
            world_border: false,
        }));
        let mut dirty = Vec::new();
        let call = |interaction: &mut InteractionState,
                    particles: &mut ParticleStore,
                    audio: &mut AudioEngine,
                    dirty: &mut Vec<BlockPos>| {
            let mut effects = BreakEffects {
                particles,
                registry: &registry,
                biome_climate: &climate,
            };
            interaction.start_use_item(
                &sender,
                audio,
                &chunks,
                dvec3(0.0, 64.0, 0.0),
                Aabb::from_center(DVec3::ZERO, 0.6, 1.8),
                dvec3(0.0, 65.62, 0.0),
                LookDirection::default(),
                None,
                None,
                None,
                20,
                false,
                false,
                &EntityStore::new(),
                InteractionHand::MainHand,
                false,
                None,
                false,
                false,
                false,
                false,
                &mut effects,
                dirty,
            )
        };
        assert!(call(
            &mut interaction,
            &mut particles,
            &mut audio,
            &mut dirty
        ));
        assert_eq!(particles.test_particle_count(), 3);
        assert!(!dirty.is_empty());
        assert_eq!(
            crate::world::block::block_properties(chunks.get_block_state(pos.x, pos.y, pos.z))
                .get("lit"),
            Some("false")
        );
        let _ = call(&mut interaction, &mut particles, &mut audio, &mut dirty);
        assert_eq!(
            particles.test_particle_count(),
            3,
            "same lit candle only extinguishes once"
        );
    }

    #[test]
    fn candle_offsets_and_gates_match_java_for_counts_cake_hit_hand_and_permission() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let pos = BlockPos::new(0, 64, 0);
        for count in 1..=4 {
            let state = crate::world::block::find_state(
                "red_candle",
                &[
                    (
                        "candles",
                        match count {
                            1 => "1",
                            2 => "2",
                            3 => "3",
                            _ => "4",
                        },
                    ),
                    ("lit", "true"),
                ],
            );
            let requests = candle_extinguish_requests(state, pos, 65.0, true, true);
            assert_eq!(requests.len(), count);
            assert!(
                requests
                    .iter()
                    .all(|r| r.kind == crate::particle::ServerParticleKind::Smoke
                        && r.velocity == dvec3(0.0, 0.1, 0.0))
            );
        }
        let cake = crate::world::block::find_state("blue_candle_cake", &[("lit", "true")]);
        assert!(candle_extinguish_requests(cake, pos, 64.5, true, true).is_empty());
        let top = candle_extinguish_requests(cake, pos, 64.500_001, true, true);
        assert_eq!(top.len(), 1);
        assert_eq!(top[0].position, dvec3(0.5, 65.0, 0.5));
        assert!(candle_extinguish_requests(cake, pos, 65.0, false, true).is_empty());
        assert!(candle_extinguish_requests(cake, pos, 65.0, true, false).is_empty());
        let unlit =
            crate::world::block::find_state("red_candle", &[("candles", "4"), ("lit", "false")]);
        assert!(candle_extinguish_requests(unlit, pos, 65.0, true, true).is_empty());
    }

    #[test]
    fn dragon_egg_use_and_attack_emit_exactly_128_portals_only_for_valid_candidate() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let origin = BlockPos::new(8, 64, 8);
        let chunks = egg_test_chunks(true);
        let egg = crate::world::block::default_state_of("dragon_egg").unwrap();
        chunks.set_block_state(origin.x, origin.y, origin.z, egg);
        let border = crate::world::border::WorldBorder::default();
        let seed = (0..1000)
            .find(|seed| !dragon_egg_teleport_requests(origin, &chunks, &border, *seed).is_empty())
            .unwrap();
        let requests = dragon_egg_teleport_requests(origin, &chunks, &border, seed);
        assert_eq!(requests.len(), 128);
        assert!(
            requests
                .iter()
                .all(|r| r.kind == crate::particle::ServerParticleKind::Portal)
        );
        assert!(
            requests
                .iter()
                .all(|r| r.velocity.abs().cmple(dvec3(0.1, 0.1, 0.1)).all())
        );
        assert!(requests.iter().all(|r| {
            (-7.0..24.0).contains(&r.position.x)
                && (-7.0..24.0).contains(&r.position.z)
                && (63.5..65.5).contains(&r.position.y)
        }));
        let mut particles = particle_test_store();
        let registry = BlockRegistry::test_empty();
        let climate = HashMap::new();
        spawn_interaction_particle_requests(
            requests,
            dvec3(8.5, 64.5, 8.5),
            &chunks,
            &mut BreakEffects {
                particles: &mut particles,
                registry: &registry,
                biome_climate: &climate,
            },
        );
        assert_eq!(particles.test_particle_count(), 128);

        // Exercise both Java input branches through the same interaction owner.
        let run_input =
            |attack: bool, held_stack: Option<&ItemStackData>, hand: InteractionHand| {
                let mut particles = particle_test_store();
                let registry = BlockRegistry::test_empty();
                let climate = HashMap::new();
                let mut audio = AudioEngine::silent_for_test();
                let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
                let sender = PacketSender::new(tx);
                let mut interaction = InteractionState::new();
                interaction.world_border = border;
                interaction.target = Some(HitResult::Block(BlockHitResult {
                    block_pos: origin,
                    face: Direction::Up,
                    hit_point: dvec3(8.5, 65.0, 8.5),
                    inside: false,
                    world_border: false,
                }));
                fastrand::seed(1);
                let mut effects = BreakEffects {
                    particles: &mut particles,
                    registry: &registry,
                    biome_climate: &climate,
                };
                if attack {
                    interaction.start_attack(
                        &chunks,
                        &sender,
                        &mut audio,
                        &InputState::new(),
                        dvec3(8.5, 64.0, 8.5),
                        true,
                        false,
                        false,
                        None,
                        &mut effects,
                        &mut Vec::new(),
                    );
                } else {
                    assert!(interaction.start_use_item(
                        &sender,
                        &mut audio,
                        &chunks,
                        dvec3(8.5, 64.0, 8.5),
                        Aabb::from_center(dvec3(8.5, 64.0, 8.5), 0.6, 1.8),
                        dvec3(8.5, 65.62, 8.5),
                        LookDirection::default(),
                        None,
                        None,
                        held_stack,
                        20,
                        false,
                        false,
                        &EntityStore::new(),
                        hand,
                        false,
                        None,
                        false,
                        false,
                        false,
                        false,
                        &mut effects,
                        &mut Vec::new(),
                    ));
                }
                assert_eq!(effects.particles.test_particle_count(), 128);
                assert_eq!(chunks.get_block_state(origin.x, origin.y, origin.z), egg);
            };
        let held_item = ItemStackData::new(ItemKind::Dirt, 1);
        run_input(false, Some(&held_item), InteractionHand::MainHand);
        run_input(false, None, InteractionHand::MainHand);
        run_input(true, None, InteractionHand::MainHand);

        let no_floor = egg_test_chunks(false);
        assert!(dragon_egg_teleport_requests(origin, &no_floor, &border, seed).is_empty());
        let mut outside = border;
        outside.set_size(0.0);
        assert!(dragon_egg_teleport_requests(origin, &chunks, &outside, seed).is_empty());
        assert!(
            dragon_egg_teleport_requests(
                BlockPos::new(8, chunks.min_y() + chunks.height() as i32 + 20, 8),
                &chunks,
                &border,
                seed
            )
            .is_empty()
        );
    }

    #[test]
    fn continued_mining_routes_one_face_particle_and_never_duplicates_final_break_burst() {
        let _protocol = crate::world::block::test_protocol_guard();
        crate::world::block::init("26.2");
        let (chunks, mut audio, _, mut particles, registry) = headless_use_fixture();
        let climate = HashMap::new();
        let stone = crate::world::block::default_state_of("stone").unwrap();
        let pos = BlockPos::new(2, 64, 2);
        chunks.set_block_state(pos.x, pos.y, pos.z, stone);
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let sender = PacketSender::new(tx);
        let mut interaction = InteractionState::new();
        interaction.target = Some(HitResult::Block(BlockHitResult {
            block_pos: pos,
            face: Direction::East,
            hit_point: dvec3(3.0, 64.5, 2.5),
            inside: false,
            world_border: false,
        }));
        interaction.is_destroying = true;
        interaction.destroy_pos = pos;
        let mut effects = BreakEffects {
            particles: &mut particles,
            registry: &registry,
            biome_climate: &climate,
        };
        interaction.continue_attack(
            &chunks,
            &sender,
            &mut audio,
            dvec3(2.5, 64.0, 2.5),
            true,
            false,
            None,
            &mut effects,
            &mut Vec::new(),
        );
        assert_eq!(effects.particles.pending_terrain_counts_for_test(), (1, 1));

        let next_pos = BlockPos::new(3, 64, 2);
        let dirt = crate::world::block::default_state_of("dirt").unwrap();
        chunks.set_block_state(next_pos.x, next_pos.y, next_pos.z, dirt);
        interaction.target = Some(HitResult::Block(BlockHitResult {
            block_pos: next_pos,
            face: Direction::North,
            hit_point: dvec3(3.5, 64.5, 2.0),
            inside: false,
            world_border: false,
        }));
        interaction.continue_attack(
            &chunks,
            &sender,
            &mut audio,
            dvec3(2.5, 64.0, 2.5),
            true,
            false,
            None,
            &mut effects,
            &mut Vec::new(),
        );
        assert_eq!(effects.particles.pending_terrain_counts_for_test(), (2, 2));

        interaction.target = None;
        interaction.continue_attack(
            &chunks,
            &sender,
            &mut audio,
            dvec3(2.5, 64.0, 2.5),
            true,
            false,
            None,
            &mut effects,
            &mut Vec::new(),
        );
        assert_eq!(effects.particles.pending_terrain_counts_for_test(), (2, 2));

        // The normal ray-pick limit leaves a block beyond 4.5 blocks out of
        // the target; continuing attack then follows the no-hit/stop path.
        chunks.set_block_state(pos.x, pos.y, pos.z, BlockState::AIR);
        chunks.set_block_state(next_pos.x, next_pos.y, next_pos.z, BlockState::AIR);
        let far_pos = BlockPos::new(2, 64, 8);
        chunks.set_block_state(far_pos.x, far_pos.y, far_pos.z, stone);
        let mut far_border = crate::world::border::WorldBorder::default();
        far_border.set_size(1000.0);
        interaction.update_target(
            Position::new(2.5, 64.5, 2.5),
            LookDirection::default(),
            &chunks,
            &EntityStore::new(),
            false,
            &far_border,
        );
        assert!(interaction.target.is_none());
        interaction.continue_attack(
            &chunks,
            &sender,
            &mut audio,
            dvec3(2.5, 64.0, 2.5),
            true,
            false,
            None,
            &mut effects,
            &mut Vec::new(),
        );
        assert_eq!(effects.particles.pending_terrain_counts_for_test(), (2, 2));

        // A final predicted break keeps its separate volume burst and the
        // current block has become air before the hit-face effect is considered.
        chunks.set_block_state(pos.x, pos.y, pos.z, stone);
        interaction.target = Some(HitResult::Block(BlockHitResult {
            block_pos: pos,
            face: Direction::Up,
            hit_point: dvec3(2.5, 65.0, 2.5),
            inside: false,
            world_border: false,
        }));
        interaction.is_destroying = true;
        interaction.destroy_pos = pos;
        effects.particles.clear();
        interaction.continue_attack(
            &chunks,
            &sender,
            &mut audio,
            dvec3(2.5, 64.0, 2.5),
            true,
            true,
            None,
            &mut effects,
            &mut Vec::new(),
        );
        assert!(is_air(chunks.get_block_state(pos.x, pos.y, pos.z)));
        assert_eq!(effects.particles.pending_terrain_counts_for_test(), (64, 0));
    }
}
