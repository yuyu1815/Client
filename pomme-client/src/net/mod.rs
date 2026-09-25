#[cfg(test)]
mod azalea_compat;
pub(crate) mod chat;
pub(crate) mod chat_security;
pub mod chunk_batch;
pub mod commands;
pub mod conn;
pub mod connection;
mod cooldown;
mod dialog;
pub mod handler;
pub mod known_packs;
mod native_codecs;
pub mod resolve;
pub mod sender;
pub mod stream;
pub mod translate;

use std::sync::Arc;

use azalea_block::BlockState;
use azalea_core::heightmap_kind::HeightmapKind;
use azalea_core::position::{BlockPos, ChunkPos};
use azalea_inventory::ItemStack;
use azalea_registry::builtin::{BlockEntityKind, EntityKind};
use glam::DVec3;
use simdnbt::owned::NbtCompound;

use crate::entity::MetaValue;
use crate::entity::components::Position;
use crate::entity::villager::{VillagerKind, VillagerProfession};

/// Lossless client-owned Explosion payload; sound is decoded as a native
/// holder.
#[derive(Clone)]
pub struct ExplosionPayload {
    pub center: azalea_core::position::Vec3,
    pub radius: f32,
    pub block_count: i32,
    pub player_knockback: Option<azalea_core::position::Vec3>,
    pub explosion_particle: azalea_entity::particle::Particle,
    pub explosion_sound: crate::audio::SoundRef,
    pub block_particles: Vec<
        azalea_protocol::packets::game::c_explode::Weighted<
            azalea_protocol::packets::game::c_explode::ExplosionParticleInfo,
        >,
    >,
}

/// A packet's per-column light payload (chunk load or standalone update):
/// present sections listed in `*_updates`, selected by `*_y_mask`, with
/// `empty_*_y_mask` marking explicitly-zero sections.
pub struct PacketLightData {
    pub sky_updates: Arc<Box<[Box<[u8]>]>>,
    pub block_updates: Arc<Box<[Box<[u8]>]>>,
    pub sky_y_mask: azalea_core::bitset::BitSet,
    pub block_y_mask: azalea_core::bitset::BitSet,
    pub empty_sky_y_mask: azalea_core::bitset::BitSet,
    pub empty_block_y_mask: azalea_core::bitset::BitSet,
}

impl From<&azalea_protocol::packets::game::c_light_update::ClientboundLightUpdatePacketData>
    for PacketLightData
{
    fn from(
        data: &azalea_protocol::packets::game::c_light_update::ClientboundLightUpdatePacketData,
    ) -> Self {
        Self {
            sky_updates: data.sky_updates.clone(),
            block_updates: data.block_updates.clone(),
            sky_y_mask: data.sky_y_mask.clone(),
            block_y_mask: data.block_y_mask.clone(),
            empty_sky_y_mask: data.empty_sky_y_mask.clone(),
            empty_block_y_mask: data.empty_block_y_mask.clone(),
        }
    }
}

pub struct ServerTransfer {
    pub host: String,
    pub port: u32,
    pub cookies: std::collections::HashMap<azalea_registry::identifier::Identifier, Vec<u8>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CriticalHitKind {
    Critical,
    Enchanted,
}

#[derive(Clone, Copy)]
pub enum CustomChatCompletionsAction {
    Add,
    Remove,
    Set,
}

pub enum NetworkEvent {
    Connected {
        profile_name: String,
    },
    Registries(Arc<azalea_core::registry_holder::RegistryHolder>),
    /// The `minecraft:dialog` registry with its tags, sent with `Registries`
    /// and again whenever a tag update replaces the dialog tags.
    DialogRegistry(Arc<crate::ui::server_dialog::DialogRegistry>),
    BiomeColors {
        colors: std::collections::HashMap<u32, crate::renderer::chunk::mesher::BiomeClimate>,
    },
    /// `LEVEL_CHUNKS_LOAD_START`: the server has started sending the level.
    LevelChunksLoadStart,
    DimensionInfo {
        is_debug: bool,
        height: u32,
        min_y: i32,
        has_skylight: bool,
        cardinal_light: crate::world::block::model::CardinalLightType,
        /// Server registry ID selected from this dimension type's
        /// `default_clock`.
        clock_id: Option<u32>,
    },
    ChunkLoaded {
        pos: ChunkPos,
        data: Arc<Box<[u8]>>,
        heightmaps: Vec<(HeightmapKind, Box<[u64]>)>,
        light: PacketLightData,
    },
    ChunkBiomes {
        pos: ChunkPos,
        data: Vec<u8>,
    },
    /// Standalone server light correction (`ClientboundLightUpdate`).
    LightUpdate {
        pos: ChunkPos,
        light: PacketLightData,
    },
    ChunkUnloaded {
        pos: ChunkPos,
    },
    ChunkCacheCenter {
        x: i32,
        z: i32,
    },
    PlayerPosition {
        /// Teleport id to acknowledge.
        id: u32,
        change: azalea_protocol::common::movements::PositionMoveRotation,
        relative: azalea_protocol::common::movements::RelativeMovements,
    },
    PlayerRotation {
        y_rot: f32,
        x_rot: f32,
        relative_y: bool,
        relative_x: bool,
    },
    /// Lossless native explosion payload; consumed once by the game core.
    Explosion(ExplosionPayload),
    PlayerHealth {
        health: f32,
        food: u32,
        saturation: f32,
    },
    PlayerExperience {
        progress: f32,
        level: i32,
        total_experience: u32,
    },
    SetPlayerInventory {
        slot: u32,
        item: ItemStack,
    },
    UpdateMobEffect {
        entity_id: i32,
        effect: crate::mob_effect::MobEffectInstance,
    },
    RemoveMobEffect {
        entity_id: i32,
        effect_id: u32,
    },
    ClearMobEffects,
    SetPassengers {
        vehicle: i32,
        passengers: Vec<i32>,
    },
    EntitySaddle {
        entity_id: i32,
        saddled: bool,
    },
    Waypoint {
        operation: azalea_protocol::packets::game::c_waypoint::WaypointOperation,
        waypoint: azalea_protocol::packets::game::c_waypoint::TrackedWaypoint,
    },
    MapItemData {
        map_id: u32,
        scale: u8,
        locked: bool,
        patch: Option<(u8, u8, u8, u8, Vec<u8>)>,
        decorations: Option<Vec<crate::world::maps::MapDecoration>>,
    },
    EntityArmorUpdate {
        entity_id: i32,
        armor: u32,
    },
    EntityMaxHealthUpdate {
        entity_id: i32,
        max_health: f32,
    },
    ContainerContent {
        container_id: i32,
        items: Vec<ItemStack>,
        carried: ItemStack,
        state_id: u32,
    },
    ContainerSlot {
        container_id: i32,
        index: u16,
        item: ItemStack,
        state_id: u32,
    },
    HeldSlot {
        slot: u8,
    },
    ItemCooldown {
        group: azalea_registry::identifier::Identifier,
        duration: i32,
    },
    /// A menu data value (furnace lit/cook progress, etc.).
    ContainerData {
        container_id: i32,
        id: u16,
        value: u16,
    },
    MerchantOffers {
        container_id: i32,
        offers: Vec<azalea_protocol::packets::game::c_merchant_offers::MerchantOffer>,
        villager_level: u32,
        villager_xp: u32,
        show_progress: bool,
        can_restock: bool,
    },
    MountScreenOpen {
        container_id: i32,
        inventory_columns: u32,
        entity_id: i32,
    },
    WorldBorderInitialize {
        center_x: f64,
        center_z: f64,
        old_size: f64,
        new_size: f64,
        lerp_time: i64,
        absolute_max_size: i32,
        warning_blocks: i32,
        warning_time: i32,
    },
    WorldBorderCenter {
        x: f64,
        z: f64,
    },
    WorldBorderSize {
        size: f64,
    },
    WorldBorderLerpSize {
        old_size: f64,
        new_size: f64,
        lerp_time: i64,
    },
    WorldBorderWarningBlocks {
        warning_blocks: i32,
    },
    WorldBorderWarningTime {
        warning_time: i32,
    },
    OpenScreen {
        container_id: i32,
        menu_type: azalea_registry::builtin::MenuKind,
        title: String,
    },
    OpenBook {
        hand: azalea_protocol::packets::game::s_interact::InteractionHand,
    },
    ContainerClosed,
    CursorItem {
        item: ItemStack,
    },
    ChatMessage {
        spans: Vec<crate::ui::text::TextSpan>,
        /// Same bound chat type rendered from the signed body with unsigned
        /// content removed, for Vanilla's `onlyShowSecureChat` path.
        secure_spans: Option<Vec<crate::ui::text::TextSpan>>,
        missing_profile_spans: Option<Vec<crate::ui::text::TextSpan>>,
        signature: Option<[u8; 256]>,
        sender_uuid: Option<uuid::Uuid>,
        signed_body: Option<crate::net::chat_security::SignedChatBody>,
        source: crate::ui::chat::ChatMessageSource,
        tag: Option<crate::ui::chat::ChatMessageTag>,
    },
    DeleteChatMessage {
        signature: [u8; 256],
    },
    ActionBar {
        spans: Vec<crate::ui::text::TextSpan>,
    },
    ServerLinks {
        links: Vec<crate::ui::server_dialog::ServerLink>,
    },
    ShowDialog {
        dialog: crate::ui::server_dialog::DialogReference,
    },
    ClearDialog,
    BossBarUpdate {
        id: uuid::Uuid,
        op: crate::ui::boss_bar::BossBarOp,
    },
    AdvancementsUpdate(Box<crate::ui::toast::AdvancementsUpdate>),
    RecipeToastAdd {
        entries: Vec<crate::ui::toast::RecipeToastEntry>,
    },
    RecipeBookAdd(azalea_protocol::packets::game::c_recipe_book_add::ClientboundRecipeBookAdd),
    RecipeBookRemove(Vec<u32>),
    RecipeBookSettings(azalea_protocol::packets::game::c_recipe_book_settings::RecipeBookSettings),
    UpdateRecipes(azalea_protocol::packets::game::c_update_recipes::ClientboundUpdateRecipes),
    TitleText {
        spans: Vec<crate::ui::text::TextSpan>,
    },
    SubtitleText {
        spans: Vec<crate::ui::text::TextSpan>,
    },
    TitlesAnimation {
        fade_in: i32,
        stay: i32,
        fade_out: i32,
    },
    ClearTitles {
        reset_times: bool,
    },
    ScoreboardObjective {
        name: String,
        display: Option<Vec<crate::ui::text::TextSpan>>,
        number_format: Option<crate::ui::hud::ScoreNumberFormat>,
        render_type: Option<azalea_core::objectives::ObjectiveCriteria>,
    },
    ScoreboardDisplay {
        slot: azalea_protocol::packets::game::c_set_display_objective::DisplaySlot,
        name: Option<String>,
    },
    ScoreboardScore {
        owner: String,
        objective: String,
        score: i32,
        display: Option<Vec<crate::ui::text::TextSpan>>,
        number_format: Option<crate::ui::hud::ScoreNumberFormat>,
    },
    ScoreboardReset {
        owner: String,
        objective: Option<String>,
    },
    ScoreboardTeam {
        name: String,
        display_name: Vec<crate::ui::text::TextSpan>,
        nametag_visibility: azalea_protocol::packets::game::c_set_player_team::NameTagVisibility,
        collision_rule: azalea_protocol::packets::game::c_set_player_team::CollisionRule,
        friendly_fire: bool,
        see_friendly_invisibles: bool,
        prefix: Vec<crate::ui::text::TextSpan>,
        suffix: Vec<crate::ui::text::TextSpan>,
        color: [f32; 4],
        fill_color: Option<[f32; 4]>,
        sidebar_slot: Option<azalea_protocol::packets::game::c_set_display_objective::DisplaySlot>,
        members: Option<Vec<String>>,
    },
    ScoreboardTeamMembers {
        name: String,
        members: Vec<String>,
        join: bool,
    },
    ScoreboardTeamRemoved {
        name: String,
    },
    CommandTree {
        tree: Arc<crate::net::commands::CommandTree>,
    },
    CustomChatCompletions {
        action: CustomChatCompletionsAction,
        entries: Vec<String>,
    },
    CommandSuggestions {
        id: u32,
        /// Offset into the command string (as sent, including the leading `/`)
        /// where the completed range begins.
        start: usize,
        options: Vec<crate::ui::chat::ChatSuggestion>,
    },
    BlockUpdate {
        pos: BlockPos,
        state: BlockState,
    },
    BlockChangedAck {
        seq: u32,
    },
    SectionBlocksUpdate {
        updates: Vec<(BlockPos, BlockState)>,
    },
    BlockEntitySync {
        chunk_pos: ChunkPos,
        entries: Vec<(BlockPos, BlockEntityKind, NbtCompound)>,
    },
    BlockEntityUpdate {
        pos: BlockPos,
        kind: BlockEntityKind,
        nbt: Option<NbtCompound>,
    },
    BlockEvent {
        pos: BlockPos,
        action_id: u8,
        action_parameter: u8,
    },
    PlaySound {
        sound: crate::audio::SoundRef,
        category: u8,
        pos: Position,
        volume: f32,
        pitch: f32,
        seed: u64,
    },
    PlayEntitySound {
        sound: crate::audio::SoundRef,
        category: u8,
        entity_id: i32,
        volume: f32,
        pitch: f32,
        seed: u64,
    },
    StopSound {
        sound_id: Option<String>,
        category: Option<u8>,
    },
    TimeUpdate {
        game_time: u64,
        /// `(world-clock registry id, total ticks, partial tick, rate)`.
        clock_updates: Vec<(u32, u64, f32, f32)>,
        /// Pre-26.2 `set_time` was translated to synthetic clock id 0.
        legacy: bool,
    },
    /// Authoritative `/tick` state. Vanilla applies this to TickRateManager;
    /// it is separate from a world clock's own rate.
    TickingState {
        tick_rate: f32,
        is_frozen: bool,
    },
    /// Authoritative number of ticks to execute while frozen (`/tick step`).
    TickingStep {
        tick_steps: u32,
    },
    WeatherUpdate {
        event: azalea_protocol::packets::game::c_game_event::EventType,
        param: f32,
    },
    /// Game events not consumed by the packet layer (e.g. WinGame and player
    /// flags).
    GameEvent {
        event: azalea_protocol::packets::game::c_game_event::EventType,
        param: f32,
    },
    GameModeChanged {
        game_mode: u8,
        /// `Some` = authoritative previous mode from login/respawn (which may
        /// itself be absent); `None` = derive from the mode being replaced
        /// (the GameEvent packet carries no previous mode).
        previous: Option<Option<u8>>,
    },
    DimensionName {
        name: String,
    },
    PlayerAbilitiesChanged {
        invulnerable: bool,
        flying: bool,
        can_fly: bool,
        instant_break: bool,
        flying_speed: f32,
        walking_speed: f32,
    },
    ServerViewDistance {
        distance: u32,
    },
    ServerSimulationDistance {
        distance: u32,
    },
    EntitySpawned {
        id: i32,
        uuid: uuid::Uuid,
        entity_type: EntityKind,
        position: Position,
        velocity: DVec3,
        y_rot_deg: f32,
        x_rot_deg: f32,
        head_y_rot_deg: f32,
    },
    EntityMoved {
        id: i32,
        dx: f64,
        dy: f64,
        dz: f64,
        on_ground: bool,
    },
    EntityMovedRotated {
        id: i32,
        dx: f64,
        dy: f64,
        dz: f64,
        y_rot_deg: f32,
        x_rot_deg: f32,
        on_ground: bool,
    },
    EntityRotated {
        id: i32,
        y_rot_deg: f32,
        x_rot_deg: f32,
        on_ground: bool,
    },
    EntityMotion {
        id: i32,
        velocity: DVec3,
    },
    EntityTeleported {
        id: i32,
        position: Position,
        relative: Option<azalea_protocol::common::movements::RelativeMovements>,
        /// `TeleportEntity` applies the packet's velocity; `EntityPositionSync`
        /// doesn't (vanilla `setValuesFromPositionPacket` vs
        /// `handleEntityPositionSync`).
        velocity: Option<DVec3>,
        y_rot_deg: f32,
        x_rot_deg: f32,
        on_ground: bool,
    },
    LevelEvent {
        event_type: u32,
        pos: BlockPos,
        data: u32,
    },
    /// Supported `ClientboundLevelParticles` kinds and their decoded options.
    /// Unsupported payload types are dropped without guessing their framing.
    LevelParticles {
        kind: crate::particle::ServerParticleKind,
        options: crate::particle::ServerParticleOptions,
        override_limiter: bool,
        always_show: bool,
        pos: DVec3,
        x_dist: f32,
        y_dist: f32,
        z_dist: f32,
        max_speed: f32,
        count: i32,
    },
    EntitiesRemoved {
        ids: Vec<i32>,
    },
    EntityItemData {
        id: i32,
        item_name: String,
        item_id: u32,
        damage: i32,
        count: i32,
    },
    EntityHeadRotation {
        id: i32,
        head_y_rot_deg: f32,
    },
    /// A raw scalar entity-data value; `EntityStore::apply_entity_data`
    /// resolves its meaning per (kind, index) like vanilla's
    /// `onSyncedDataUpdated`.
    EntityData {
        id: i32,
        index: u8,
        value: MetaValue,
    },
    EntityPose {
        id: i32,
        pose: crate::entity::EntityPose,
    },
    /// LivingEntity metadata index 14 (SLEEPING_POS): Some while in a bed.
    /// Vanilla `isSleeping()` is `getSleepingPos().isPresent()`.
    EntitySleepingPos {
        id: i32,
        pos: Option<BlockPos>,
    },
    /// `ClientboundAnimate` action 2: vanilla `handleAnimate` calls
    /// `stopSleepInBed(false, false)`, forcing the sleep counter to 100.
    EntityWakeUp {
        id: i32,
    },
    /// Entity event 35: a Totem of Undying was used by this entity.
    TotemUsed {
        entity_id: i32,
    },
    /// Animate actions 4/5: spawn a tracked critical-hit particle emitter.
    CriticalHit {
        id: i32,
        kind: CriticalHitKind,
    },
    SheepEatStart {
        id: i32,
    },
    /// Entity event 9: the entity finished using its item (eating complete).
    FinishUseItem {
        id: i32,
    },
    /// Registry/wire variant slot; meaning is per-kind. `kind` is the mob
    /// the emitting arm resolved for, guarding overloaded metadata indices.
    EntityVariant {
        id: i32,
        kind: EntityKind,
        variant: u32,
    },
    WolfShaking {
        id: i32,
        shaking: bool,
    },
    RabbitJump {
        id: i32,
    },
    SquidTentacleReset {
        id: i32,
    },
    /// Entity event 4: iron golem punch.
    GolemPunch {
        id: i32,
    },
    /// Entity events 11 / 34: iron golem flower offer start / stop.
    GolemOfferFlower {
        id: i32,
        offering: bool,
    },
    VillagerData {
        id: i32,
        kind: VillagerKind,
        profession: VillagerProfession,
        level: u32,
    },
    EntityCustomName {
        id: i32,
        name: Option<String>,
    },
    EntitySwing {
        id: i32,
    },
    EntityDamaged {
        id: i32,
    },
    EntityDied {
        id: i32,
    },
    HurtAnimation {
        id: i32,
        yaw: f32,
    },
    ItemPickedUp {
        item_id: i32,
        collector_id: i32,
        amount: i32,
    },
    PlayerLogin {
        entity_id: i32,
        hardcore: bool,
        show_death_screen: bool,
        online_mode: bool,
    },
    SecureChatEnforced {
        enforced: bool,
    },
    PlayerScore {
        entity_id: i32,
        score: i32,
    },
    PlayerAbsorption {
        entity_id: i32,
        absorption: f32,
    },
    /// Vanilla `handleRespawn`: a fresh `LocalPlayer` is built, restoring old
    /// entity data / attribute modifiers only per the packet's keep flags.
    PlayerRespawned {
        keep_entity_data: bool,
        keep_attribute_modifiers: bool,
    },
    PlayerDied {
        player_id: i32,
        message: String,
    },
    ResourcePackPush {
        id: uuid::Uuid,
        url: String,
        hash: String,
        required: bool,
    },
    ResourcePackPop {
        id: Option<uuid::Uuid>,
    },
    /// The server re-entered the configuration phase (a proxy transfer);
    /// world-scoped state resets like vanilla's `clearClientLevel`.
    Reconfiguring,
    Disconnected {
        reason: String,
    },
    /// A server's code of conduct must be shown and accepted by the user before
    /// replying.
    CodeOfConduct {
        text: String,
    },
    /// Server-directed transfer; only server cookies cross to the next
    /// connection. Authentication credentials remain owned by UserData.
    ServerTransfer(ServerTransfer),
    PlayerInfoUpdate {
        actions: crate::player::tab_list::PlayerInfoActions,
        entries: Vec<crate::player::tab_list::PlayerInfoEntry>,
    },
    PlayerInfoRemove {
        uuids: Vec<uuid::Uuid>,
    },
    TabListHeaderFooter {
        header: Vec<crate::ui::text::TextSpan>,
        footer: Vec<crate::ui::text::TextSpan>,
    },
}

/// The client information pomme reports. The configuration and game phase
/// packets wrap the same struct under different field names, and 1.20.1 sends
/// it in the game phase only, so all three sites share this.
pub fn client_information(
    view_distance: u8,
    chat_options: crate::ui::chat::ChatOptions,
) -> azalea_protocol::common::client_information::ClientInformation {
    use azalea_entity::HumanoidArm;
    use azalea_protocol::common::client_information::*;
    let chat_visibility = match chat_options.visibility {
        crate::ui::chat::ChatVisibilitySetting::Full => ChatVisibility::Full,
        crate::ui::chat::ChatVisibilitySetting::System => ChatVisibility::System,
        crate::ui::chat::ChatVisibilitySetting::Hidden => ChatVisibility::Hidden,
    };
    ClientInformation {
        language: "en_us".into(),
        view_distance,
        chat_visibility,
        chat_colors: chat_options.colors,
        model_customization: ModelCustomization {
            cape: true,
            jacket: true,
            left_sleeve: true,
            right_sleeve: true,
            left_pants: true,
            right_pants: true,
            hat: true,
        },
        main_hand: HumanoidArm::Right,
        text_filtering_enabled: false,
        allows_listing: true,
        particle_status: ParticleStatus::All,
    }
}

/// The `minecraft:brand` payload pomme announces itself with. Sent from the
/// configuration phase, or the game phase on versions without one.
pub fn brand_payload() -> Vec<u8> {
    let mut out = Vec::new();
    azalea_core::delta::AzBuf::azalea_write(&String::from("pomme"), &mut out).unwrap();
    out
}
