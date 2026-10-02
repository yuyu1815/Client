use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use azalea_core::position::{BlockPos, ChunkPos};
use azalea_protocol::packets::game::{
    ServerboundClientInformation, ServerboundCommandSuggestion, ServerboundGamePacket,
};
use azalea_registry::builtin::{BlockEntityKind, EntityKind};
use glam::{DVec3, FloatExt as _};

use crate::app::core::{AppCore, PlayerInputState};
use crate::app::level_load::LevelLoadTracker;
use crate::app::phases::Gfx;
use crate::app::{TICK_RATE, input};
use crate::audio::{CATEGORY_AMBIENT, CATEGORY_PLAYERS, SoundRef};
use crate::benchmark::{
    Benchmark, BenchmarkResult, ChunkLoadBench, ChunkLoadResult, ChunkLoadStep, UploadHandle,
    UploadStatus, upload_result,
};
use crate::entity::components::{LookDirection, Position};
use crate::entity::{EntityStore, ItemEntityStore, lerp_angle};
use crate::net::connection::ConnectionHandle;
use crate::player::LocalPlayer;
use crate::player::interaction::{HitResult, InteractionState};
use crate::player::menu_click::ContainerKind;
use crate::player::tab_list::TabList;
use crate::renderer::chunk::buffer::column_is_near;
use crate::renderer::chunk::mesher::{BiomeClimate, ChunkMeshData, MeshDispatcher};
use crate::renderer::chunk::occlusion_graph::{self, VisibilitySet};
use crate::renderer::entity_model::triangle_wave;
use crate::renderer::pipelines::block_entity;
use crate::renderer::pipelines::entity_renderer::{
    EntityRenderInfo, MAX_OVERLAYS, WHITE_TINT, dye_color_tint, jeb_sheep_tint, wool_color_tint,
};
use crate::renderer::pipelines::menu_overlay::MenuElement;
use crate::renderer::{MapQuadDraw, Renderer, SkyState};
use crate::resource_pack::ResourcePackManager;
use crate::ui::chat::{ChatState, ChatUiAction};
use crate::ui::death::{self, DeathAction};
use crate::ui::pause::{self, PauseAction, PauseScreen};
use crate::ui::{common, hud};
use crate::world::block::model::CardinalLightType;
use crate::world::block_entity_anim::BlockEntityAnimStore;
use crate::world::chunk::ChunkStore;

/// Which screen a server-opened container renders as.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ContainerScreen {
    CraftingTable,
    Furnace(crate::ui::furnace::FurnaceVariant),
    Chest { rows: u8 },
    ShulkerBox,
    Hopper,
    Anvil,
    Enchantment,
    Beacon,
    Merchant,
    Horse { columns: u8, entity_id: i32 },
    Special(crate::ui::special_container::SpecialMenu),
}

impl ContainerScreen {
    /// The click-prediction menu kind backing this screen.
    pub fn click_kind(self) -> ContainerKind {
        match self {
            Self::CraftingTable => ContainerKind::CraftingTable,
            Self::Furnace(_) => ContainerKind::Furnace,
            Self::Chest { rows } => ContainerKind::Chest { rows },
            Self::ShulkerBox => ContainerKind::ShulkerBox,
            Self::Hopper => ContainerKind::Hopper,
            Self::Anvil => ContainerKind::Anvil,
            Self::Enchantment => ContainerKind::Enchantment,
            Self::Beacon => ContainerKind::Beacon,
            Self::Merchant => ContainerKind::Merchant,
            Self::Horse { columns, .. } => ContainerKind::Horse { columns },
            Self::Special(menu) => match menu {
                crate::ui::special_container::SpecialMenu::Dispenser => ContainerKind::Dispenser,
                crate::ui::special_container::SpecialMenu::BrewingStand => {
                    ContainerKind::BrewingStand
                }
                crate::ui::special_container::SpecialMenu::Cartography => {
                    ContainerKind::Cartography
                }
                crate::ui::special_container::SpecialMenu::Grindstone => ContainerKind::Grindstone,
                crate::ui::special_container::SpecialMenu::Smithing => ContainerKind::Smithing,
                crate::ui::special_container::SpecialMenu::Crafter => ContainerKind::Crafter,
                crate::ui::special_container::SpecialMenu::Stonecutter => {
                    ContainerKind::Stonecutter
                }
                crate::ui::special_container::SpecialMenu::Loom => ContainerKind::Loom,
            },
        }
    }
}

/// A server-opened container screen.
pub struct OpenContainer {
    pub id: i32,
    pub title: String,
    pub screen: ContainerScreen,
    /// Menu slots in container indices; slots from `inv_start()` on are backed
    /// by the player inventory.
    pub slots: Vec<azalea_inventory::ItemStack>,
    /// The menu's data values (`ClientboundContainerSetData`), e.g. furnace
    /// lit/cook progress or the anvil repair cost. Vanilla data slots are
    /// shorts; the enchanting table uses all 10 (costs, seed, clues) with -1
    /// sentinels, so values are kept sign-extended.
    pub data: [i16; 10],
    /// Which menu data fields have actually arrived from the server.
    pub data_received: [bool; 10],
    /// The anvil rename field's state; Some only for the anvil screen.
    pub anvil: Option<crate::ui::anvil::AnvilState>,
    /// The book animation's state; Some only for the enchantment screen.
    pub enchant: Option<crate::ui::enchantment::EnchantState>,
    pub merchant: Option<crate::ui::merchant::MerchantModel>,
    /// This menu's latest server state id, echoed in container clicks.
    pub state_id: u32,
    pub loom_scroll: usize,
    pub stonecutter_scroll: usize,
}

impl OpenContainer {
    /// First container slot backed by the player inventory; container slot `i`
    /// maps to player inventory slot `i - inv_start() + 9` from here on.
    fn inv_start(&self) -> usize {
        self.screen.click_kind().inv_start()
    }
}

/// Vanilla sign dye RGB values; the renderer applies the 0.4 darkening for
/// non-glowing text and keeps this full color for glowing text.
pub(crate) fn sign_render_style(
    nbt: &simdnbt::owned::NbtCompound,
) -> (([f32; 3], bool), ([f32; 3], bool)) {
    const DYES: [(&str, [f32; 3]); 16] = [
        ("white", [249.0 / 255.0, 1.0, 254.0 / 255.0]),
        ("orange", [249.0 / 255.0, 128.0 / 255.0, 29.0 / 255.0]),
        ("magenta", [199.0 / 255.0, 78.0 / 255.0, 189.0 / 255.0]),
        ("light_blue", [58.0 / 255.0, 179.0 / 255.0, 218.0 / 255.0]),
        ("yellow", [254.0 / 255.0, 216.0 / 255.0, 61.0 / 255.0]),
        ("lime", [128.0 / 255.0, 199.0 / 255.0, 31.0 / 255.0]),
        ("pink", [243.0 / 255.0, 139.0 / 255.0, 170.0 / 255.0]),
        ("gray", [71.0 / 255.0, 79.0 / 255.0, 82.0 / 255.0]),
        ("light_gray", [157.0 / 255.0, 157.0 / 255.0, 151.0 / 255.0]),
        ("cyan", [22.0 / 255.0, 156.0 / 255.0, 156.0 / 255.0]),
        ("purple", [137.0 / 255.0, 50.0 / 255.0, 184.0 / 255.0]),
        ("blue", [60.0 / 255.0, 68.0 / 255.0, 170.0 / 255.0]),
        ("brown", [131.0 / 255.0, 84.0 / 255.0, 50.0 / 255.0]),
        ("green", [94.0 / 255.0, 124.0 / 255.0, 22.0 / 255.0]),
        ("red", [176.0 / 255.0, 46.0 / 255.0, 38.0 / 255.0]),
        ("black", [29.0 / 255.0, 29.0 / 255.0, 33.0 / 255.0]),
    ];
    let face_style = |face: &str| {
        let Some(compound) = nbt.get(face).and_then(|tag| tag.compound()) else {
            return (DYES[15].1, false);
        };
        let color = compound
            .string("color")
            .and_then(|value| {
                let name = value.to_str();
                DYES.iter()
                    .find(|(candidate, _)| *candidate == name.as_ref())
                    .map(|(_, rgb)| *rgb)
            })
            .unwrap_or(DYES[15].1);
        (color, compound.byte("has_glowing_text").unwrap_or(0) != 0)
    };
    (face_style("front_text"), face_style("back_text"))
}

fn show_death_screen_param(param: f32) -> bool {
    param == 0.0
}

fn limited_crafting_param(param: f32) -> bool {
    param == 1.0
}

fn is_win_game_event(
    event: &azalea_protocol::packets::game::c_game_event::EventType,
    _param: f32,
) -> bool {
    matches!(
        event,
        azalea_protocol::packets::game::c_game_event::EventType::WinGame
    )
}

fn credits_may_advance(server_modal_pending: bool) -> bool {
    !server_modal_pending
}

pub(crate) fn death_confirm_escape_allowed(death_confirm: bool, credits_active: bool) -> bool {
    death_confirm && !credits_active
}

fn finish_win_credits(
    state: &mut Option<crate::ui::menu::CreditsRollState>,
    complete: bool,
) -> bool {
    complete && state.take().is_some()
}

fn finish_win_credits_if_allowed(
    state: &mut Option<crate::ui::menu::CreditsRollState>,
    complete: bool,
    server_modal_pending: bool,
) -> bool {
    credits_may_advance(server_modal_pending) && finish_win_credits(state, complete)
}

fn tab_list_overlay_visible(
    requested: bool,
    hide_gui: bool,
    paused: bool,
    gui_open: bool,
    options_open: bool,
    chat_open: bool,
    dead: bool,
    death_screen_open: bool,
) -> bool {
    requested
        && !hide_gui
        && !paused
        && !gui_open
        && !options_open
        && !chat_open
        && !dead
        && !death_screen_open
}

pub struct GameState {
    pub chunk_store: ChunkStore,
    /// Client-side light engine (vanilla `LevelLightEngine`); recreated with
    /// the chunk store on dimension changes, drained once per tick.
    pub light_engine: crate::world::light::LevelLightEngine,
    /// Set by [`Self::update_light`] when chunk-load light marked columns
    /// dirty; consumed by the visibility refresh as its new-loads signal.
    pub pending_load_rescan: bool,
    pub entity_store: EntityStore,
    /// Last server position for every spawned entity, including entity kinds
    /// Pomme does not otherwise render. Used by packet-driven entity-bound
    /// sounds.
    pub entity_positions: HashMap<i32, Position>,
    /// Entity ids whose shared `DATA_SILENT` flag is currently true.
    pub silent_entities: HashSet<i32>,
    pub position_set: bool,
    /// Vanilla `ClientPacketListener.levelLoadTracker`: present from login or
    /// respawn until the level is ready and `player_loaded` has been sent.
    pub level_load: Option<LevelLoadTracker>,
    /// Vanilla `ClientPacketListener.clientLoaded`. While false, the local
    /// player doesn't tick and sends no movement.
    pub client_loaded: bool,
    pub player: LocalPlayer,
    pub item_cooldowns: crate::player::cooldown::CooldownTracker,
    pub world_border: crate::world::border::WorldBorder,
    /// Bubble index the pop sound last played for, so each pop fires once.
    pub last_bubble_pop_sound_played: i32,
    pub biome_climate: Arc<HashMap<u32, BiomeClimate>>,
    pub player_walk_pos: f32,
    pub player_walk_speed: f32,
    pub player_prev_walk_speed: f32,
    pub mesh_dispatcher: MeshDispatcher,
    /// Whether the server is this process, which the pause menu labels.
    pub singleplayer: bool,
    pub paused: bool,
    pub dead: bool,
    pub death_screen_open: bool,
    pub show_death_screen: bool,
    /// Server-side LimitedCrafting flag; vanilla client stores it but does not
    /// use it to reject local crafting operations.
    pub do_limited_crafting: bool,
    /// Game-winning credits roll, independent of the death-screen state.
    pub win_credits: Option<crate::ui::menu::CreditsRollState>,
    pub hardcore: bool,
    pub death_message: String,
    pub death_screen_ticks: u32,
    pub death_confirm: bool,
    pub death_confirm_ticks: u32,
    pub respawn_sent: bool,
    pub inventory_open: bool,
    pub creative_inventory_open: bool,
    pub creative_state: crate::ui::creative_inventory::CreativeState,
    /// The inventory menu's (container 0) latest server state id, echoed in
    /// container clicks; an open container keeps its own.
    pub inventory_state_id: u32,
    /// Carried (cursor) stack for container screens, driven by the server.
    pub cursor_item: azalea_inventory::ItemStack,
    /// The server-opened container screen (crafting table), if any.
    pub open_container: Option<OpenContainer>,
    /// Writable-book editor entered through the local use flow.
    pub book_edit: Option<crate::ui::book::BookEditState>,
    /// Read-only view opened by the server's OpenBook packet.
    pub book_view: Option<crate::ui::book::BookViewState>,
    pub sign_edit: Option<crate::ui::sign::SignEditState>,
    pub command_block_edit: Option<crate::ui::command_block::CommandBlockEditState>,
    /// Which container menu was open last frame (0 = player inventory,
    /// including the creative inventory), to detect close transitions.
    pub container_was_open: Option<i32>,
    /// Active survival click-drag (button + slots covered), if any.
    pub inv_drag: Option<(azalea_inventory::operations::QuickCraftKind, Vec<u16>)>,
    /// Last survival left click (slot, time) for double-click detection.
    pub inv_last_click: Option<(u16, Instant)>,
    /// Server registries, for hashing predicted container clicks.
    pub registries: Arc<azalea_core::registry_holder::RegistryHolder>,
    /// Timeline tags with server numeric IDs resolved to timeline identifiers.
    pub timeline_tags: std::collections::HashMap<
        azalea_registry::identifier::Identifier,
        Vec<azalea_registry::identifier::Identifier>,
    >,
    pub chat: ChatState,
    pub server_dialog: Option<crate::ui::server_dialog::ServerDialogState>,
    pub server_links: Vec<crate::ui::server_dialog::ServerLink>,
    pub code_of_conduct: Option<String>,
    pub code_of_conduct_scroll: usize,
    pub pending_server_transfer: Option<crate::net::ServerTransfer>,
    pub dialog_registry: Arc<crate::ui::server_dialog::DialogRegistry>,
    pub loom_patterns: Arc<crate::ui::loom::PatternData>,
    /// The connection is in the configuration phase (the join, or a
    /// reconfiguration), where dialogs can't run commands.
    pub configuring: bool,
    pub command_tree: Option<Arc<crate::net::commands::CommandTree>>,
    pub tab_list: TabList,
    pub tab_score_state: crate::ui::player_tab::TabScoreState,
    pub server_enforces_secure_chat: bool,
    /// Locator bar waypoints tracked by the server.
    pub waypoints: crate::world::waypoints::WaypointMap,
    pub maps: crate::world::maps::MapStore,
    /// Vanilla `Hud.toolHighlightTimer` / `lastToolHighlight` (see
    /// `tick_tool_highlight`).
    pub tool_highlight_timer: u32,
    pub last_tool_highlight: azalea_inventory::ItemStack,
    pub action_bar: Option<(Vec<crate::ui::text::TextSpan>, u64)>,
    pub title: crate::ui::title::TitleState,
    pub scoreboard: crate::ui::hud::Scoreboard,
    pub local_scoreboard_name: Option<String>,
    pub boss_bars: crate::ui::boss_bar::BossBarState,
    pub toasts: crate::ui::toast::ToastState,
    pub recipe_book: crate::ui::recipe_book::RecipeBookState,
    pub subtitles: crate::ui::subtitles::SubtitleOverlayState,
    /// Client tick counter (vanilla `player.tickCount`).
    pub tick_count: u64,
    probe_actor_diag_tick: u64,
    /// Vanilla `Hud.autosaveIndicatorValue` / `lastAutosaveIndicatorValue`;
    /// driven by in-flight screenshot writes (pomme saves no worlds).
    pub saving_indicator_value: f32,
    pub last_saving_indicator_value: f32,
    /// Tick of the last XP progress change; the XP bar outprioritizes the
    /// locator bar for 100 ticks after it (vanilla
    /// `experienceDisplayStartTick`; `i64::MIN` = untouched since (re)spawn,
    /// so the first change after joining never takes priority).
    pub xp_display_start_tick: i64,
    /// Vehicle we are the controlling (first) passenger of, from
    /// `SetPassengers` (vanilla `getControlledVehicle`).
    pub controlled_vehicle_id: Option<i32>,
    /// Vehicle we are any passenger of (vanilla `getVehicle`).
    pub riding_vehicle_id: Option<i32>,
    /// Smoothed vignette darkness (vanilla `Hud.vignetteBrightness`).
    pub vignette_brightness: f32,
    pub interaction: InteractionState,
    pub sky_state: crate::renderer::SkyState,
    pub dimension_environment: crate::net::environment::DimensionEnvironment,
    pub pending_dimension_environment: Option<crate::net::environment::DimensionEnvironmentInput>,
    /// Latest authoritative samples for every server clock, independent of the
    /// clock currently selected by the dimension's visual sky.
    pub world_clocks: HashMap<u32, crate::net::environment::ClockSample>,
    pub world_clock_ids: HashMap<String, u32>,
    pub world_clock_game_time: i64,
    pub show_debug: bool,
    pub show_chunk_borders: bool,
    pub advanced_item_tooltips: bool,
    /// F1 (vanilla `hideGui`): the HUD, chat, and overlays don't render.
    pub hide_gui: bool,
    /// A chord fired while F3 was held, so releasing F3 must not toggle the
    /// overlay (vanilla `usedDebugKeyAsModifier`).
    pub f3_chord_consumed: bool,
    /// Set by F3+A; consumed by `update_game` to re-mesh every loaded chunk.
    pub pending_chunk_reload: bool,
    /// Game mode before the last change (vanilla `previousLocalPlayerMode`),
    /// the F3+N return target.
    pub previous_game_mode: Option<u8>,
    /// Current dimension identifier (e.g. "minecraft:overworld"), for F3+C.
    pub dimension: String,
    /// Dimension-type `cardinal_light`, which picks the terrain shade table
    /// and the item entity light directions.
    pub cardinal_light: CardinalLightType,
    /// F3+F4 game-mode switcher overlay, while open.
    pub game_mode_switcher: Option<crate::ui::game_mode_switcher::GameModeSwitcherState>,
    /// Spectator hotbar menu (vanilla `SpectatorGui`). Not a GUI screen: the
    /// cursor stays grabbed and mouse look stays live while it is open.
    pub spectator: crate::ui::spectator_menu::SpectatorGuiState,
    /// Last frame's switcher presence, to re-apply the cursor grab on change.
    switcher_was_open: bool,
    pub last_sent_input: PlayerInputState,
    pub last_sent_pos: Position,
    pub last_sent_look_dir: LookDirection,
    pub last_sent_on_ground: bool,
    pub last_sent_horizontal_collision: bool,
    pub was_sprinting: bool,
    pub position_send_counter: u32,
    pub options_from_game: bool,
    pub last_render_distance: u32,
    pub last_chat_visibility: crate::ui::chat::ChatVisibilitySetting,
    pub last_chat_colors: bool,
    pub last_main_hand_right: bool,
    pub last_particle_mode: crate::particle::ParticleMode,
    /// Local desired model parts; remote Avatar masks live on their entities.
    pub local_skin_parts_mask: u8,
    pub last_skin_parts_mask: u8,
    pub last_locale: &'static str,
    pub server_render_distance: u32,
    pub server_simulation_distance: u32,
    pub item_entity_store: ItemEntityStore,
    pub particle_store: crate::particle::ParticleStore,
    pub item_activation: Option<crate::item_activation::ItemActivation>,
    pub block_entity_anim: BlockEntityAnimStore,
    pub benchmark: Option<Benchmark>,
    pub benchmark_result: Option<BenchmarkResult>,
    pub benchmark_saved: bool,
    /// In-flight/finished upload of the FPS result, while its overlay is shown.
    pub benchmark_upload: Option<UploadHandle>,
    /// Which pause screen is showing (main / benchmark submenu / chunk loader).
    pub pause_screen: PauseScreen,
    pub chunk_load_bench: Option<ChunkLoadBench>,
    pub chunk_load_result: Option<ChunkLoadResult>,
    /// Set by Esc while a chunk-load benchmark runs; consumed next frame to
    /// cancel it.
    pub chunk_load_abort: bool,
    /// In-flight/finished upload of the chunk-load result, while its overlay is
    /// shown.
    pub chunk_load_upload: Option<UploadHandle>,
    /// Last frame's `update_game` CPU phase timings, for the chunk-load
    /// benchmark's worst-frame breakdown.
    pub last_update_phases: crate::benchmark::UpdatePhases,
    pub movement_frame_id: u64,
    /// Monotonic content generation per column, bumped on every edit (and chunk
    /// load). This is the dirty marker: a column needs (re)meshing whenever its
    /// `content_gen` outruns what was last enqueued, regardless of visibility,
    /// so an edit to a deferred/hidden column can never be lost.
    pub content_gen: HashMap<ChunkPos, u64>,
    /// Global ticket; unlike unloaded column entries, never recycled.
    pub next_content_gen: u64,
    /// What was most recently meshed for each column: the LOD, the column
    /// `content_gen`, and the bitmask of section indices already meshed. The
    /// re-scan meshes only sections newly made visible (or re-meshes all on a
    /// lod/content change), so hidden sections never mesh.
    pub meshed: HashMap<ChunkPos, MeshedCol>,
    /// Per-column bitmask of currently-visible section indices (bit `si` set =
    /// section is in-frustum and not occluded). Computed in
    /// `update_visibility`.
    pub vis_mask: HashMap<ChunkPos, u32>,
    /// Per-section generation for edits only (bulk uses the column
    /// `content_gen` above). Bumped per edited section so a result is
    /// dropped only when *that* section was edited again — editing one
    /// section never invalidates a sibling section's in-flight result.
    /// Sections meshed together as one edit span share one gen value.
    pub section_gen: HashMap<(ChunkPos, i32), u64>,
    pub next_section_gen: u64,
    /// Per-section cave-cull visibility (vanilla `VisibilitySet`), keyed like
    /// `section_gen`. Fed by mesh results; consumed by the occlusion walk.
    pub section_vis: HashMap<(ChunkPos, i32), VisibilitySet>,
    /// Per-column bitmask of sections whose mesh is finished and, if it had
    /// any geometry, uploaded — vanilla's "not `UNCOMPILED`", where an empty
    /// mesh counts too. The level load gate waits on the camera's bit.
    pub compiled: HashMap<ChunkPos, u32>,
    /// Highest upload epoch each `section_vis` entry was set from; mirrors the
    /// buffer's per-section geometry gate so a stale bulk can't re-stale an
    /// edited section's visibility.
    pub section_vis_epoch: HashMap<(ChunkPos, i32), u64>,
    /// Cached per-column frustum tier (0 in view, 1 margin, 2 behind),
    /// recomputed each time an occlusion walk completes. Only the F3
    /// overlay reads it now.
    pub vis_tiers: HashMap<ChunkPos, u8>,
    pub vis_valid: bool,
    /// Camera 8-block bucket that last triggered an occlusion walk — movement,
    /// not rotation, drives recomputes (vanilla's cadence).
    pub last_vis_cam: (i32, i32, i32),
    pub last_vis_rd: u32,
    /// In-flight async occlusion walk; its result is applied a few frames
    /// later.
    pub vis_task: Option<crossbeam_channel::Receiver<HashMap<ChunkPos, u32>>>,
    /// Runtime toggle for graph-driven chunk occlusion culling (F3+O). When
    /// off, only frustum culling applies (full masks pushed to the
    /// renderer).
    pub chunk_occlusion_enabled: bool,
}

fn bump_loaded_content_generations(
    content_gen: &mut HashMap<ChunkPos, u64>,
    next_content_gen: &mut u64,
    centers: impl IntoIterator<Item = ChunkPos>,
    loaded: &HashSet<ChunkPos>,
) -> HashSet<ChunkPos> {
    let mut affected = HashSet::new();
    for center in centers {
        for pos in crate::world::chunk::mesh_neighborhood(center) {
            if loaded.contains(&pos) {
                affected.insert(pos);
            }
        }
    }
    for pos in &affected {
        *next_content_gen += 1;
        content_gen.insert(*pos, *next_content_gen);
    }
    affected
}

/// Only adjacent dirty sections in the same column can share a snapshot/job.
fn consecutive_section_runs(
    sections: impl IntoIterator<Item = (ChunkPos, i32)>,
) -> Vec<(ChunkPos, std::ops::Range<i32>)> {
    let mut sections: Vec<_> = sections.into_iter().collect();
    sections.sort_unstable_by_key(|(col, si)| (col.x, col.z, *si));
    sections.dedup();
    let mut runs: Vec<(ChunkPos, std::ops::Range<i32>)> = Vec::new();
    for (col, si) in sections {
        if let Some((last_col, range)) = runs.last_mut()
            && *last_col == col
            && range.end == si
        {
            range.end = si + 1;
        } else {
            runs.push((col, si..si + 1));
        }
    }
    runs
}

fn bump_section_generations(
    generations: &mut HashMap<(ChunkPos, i32), u64>,
    next: &mut u64,
    col: ChunkPos,
    sections: std::ops::Range<i32>,
) -> u64 {
    *next += 1;
    for si in sections {
        generations.insert((col, si), *next);
    }
    *next
}

/// Reject unloaded columns even while a worker snapshot keeps the chunk Arc
/// alive; the weak-backed chunk storage alone cannot answer membership.
fn mesh_result_is_stale(
    chunks: &ChunkStore,
    content_gen: &HashMap<ChunkPos, u64>,
    section_gen: &HashMap<(ChunkPos, i32), u64>,
    pos: ChunkPos,
    generation: u64,
    mut replaced: std::ops::Range<i32>,
    edit: bool,
    column_revision: u64,
) -> bool {
    !chunks.light_data.contains_key(&(pos.x, pos.z))
        || if edit {
            content_gen.get(&pos).copied().unwrap_or(0) != column_revision
                || replaced.any(|si| section_gen.get(&(pos, si)).copied() != Some(generation))
        } else {
            content_gen.get(&pos).copied() != Some(generation)
        }
}

/// A rejected grouped edit can still contain sections with no newer job.
/// Re-mesh those from a fresh snapshot; uploading the old group would also
/// replace sections whose generation has moved on.
fn current_edit_section_runs(
    chunks: &ChunkStore,
    section_gen: &HashMap<(ChunkPos, i32), u64>,
    pos: ChunkPos,
    generation: u64,
    replaced: std::ops::Range<i32>,
) -> Vec<(ChunkPos, std::ops::Range<i32>)> {
    if !chunks.light_data.contains_key(&(pos.x, pos.z)) {
        return Vec::new();
    }
    consecutive_section_runs(
        replaced
            .filter(|&si| section_gen.get(&(pos, si)).copied() == Some(generation))
            .map(|si| (pos, si)),
    )
}

/// What a column was last meshed as: LOD, content generation, and the set of
/// section indices (bitmask) that have been meshed so far.
#[derive(Clone, Copy)]
pub struct MeshedCol {
    pub lod: u32,
    pub content_gen: u64,
    pub mask: u32,
}

impl GameState {
    /// Return identifiers in the server's native order for a timeline tag.
    pub fn timeline_tag(
        &self,
        tag: &azalea_registry::identifier::Identifier,
    ) -> Option<&[azalea_registry::identifier::Identifier]> {
        self.timeline_tags.get(tag).map(Vec::as_slice)
    }

    /// Resolve a timeline's named world clock against the server registry and
    /// return the sampled tick position for pure evaluators.
    pub fn timeline_clock_sample(
        &self,
        timeline: &crate::net::environment::TimelineInput,
    ) -> Result<i64, String> {
        let id = self.world_clock_ids.get(&timeline.clock).ok_or_else(|| {
            format!(
                "timeline {} references unknown world clock {}",
                timeline.id, timeline.clock
            )
        })?;
        let sample = self.world_clocks.get(id).ok_or_else(|| {
            format!(
                "timeline {} world clock {} (id {id}) has no sample",
                timeline.id, timeline.clock
            )
        })?;
        let period = timeline
            .tracks
            .iter()
            .find_map(|(_, track)| track.period_ticks);
        Ok(sample.timeline_ticks(period))
    }

    /// Resolve a spawned entity's attachment from the dimensions available to
    /// existing raycast/player state. Pose/attribute-scale mutations beyond
    /// these sources are not represented by the current shared entity state.
    pub(crate) fn tracking_attachment_for(
        player: &LocalPlayer,
        entities: &EntityStore,
        items: &ItemEntityStore,
        id: i32,
    ) -> Option<crate::particle::TrackingAttachment> {
        let (position, width, height) = if id == player.entity_id {
            let bounds = player.bounding_box();
            (
                player.position.into(),
                bounds.max.x - bounds.min.x,
                player.height(),
            )
        } else if let Some(entity) = entities.living.get(&id) {
            let mut dims = azalea_entity::dimensions::EntityDimensions::from(entity.entity_type);
            if entity.is_baby {
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
            (
                entity.position.into(),
                f64::from(dims.width),
                f64::from(dims.height),
            )
        } else if let Some(position) = items.position(id) {
            let dims = azalea_entity::dimensions::EntityDimensions::from(EntityKind::Item);
            (
                position.into(),
                f64::from(dims.width),
                f64::from(dims.height),
            )
        } else {
            let vehicle = entities.vehicles.get(&id)?;
            let kind = vehicle.kind?;
            let dims = azalea_entity::dimensions::EntityDimensions::from(kind);
            (
                vehicle.position.into(),
                f64::from(dims.width),
                f64::from(dims.height),
            )
        };
        Some(crate::particle::TrackingAttachment {
            position,
            width,
            height,
        })
    }

    pub(crate) fn probe_lightmap_brightness(&self) -> f32 {
        eye_lightmap_brightness(self)
    }

    pub fn new(
        renderer: &Renderer,
        resource_packs: &ResourcePackManager,
        render_distance: u32,
        singleplayer: bool,
        chat_options: crate::ui::chat::ChatOptions,
        main_hand_right: bool,
        particle_mode: crate::particle::ParticleMode,
        skin_parts_mask: u8,
    ) -> Self {
        let biome_climate = Arc::new(HashMap::new());
        // The dimension's shade table arrives with `DimensionInfo`, which
        // builds a fresh dispatcher.
        let mesh_dispatcher = renderer.create_mesh_dispatcher(
            biome_climate,
            Some(resource_packs),
            Default::default(),
        );

        let chunk_store = ChunkStore::new(render_distance);
        Self {
            light_engine: crate::world::light::LevelLightEngine::new(
                chunk_store.height(),
                chunk_store.min_y(),
                true,
            ),
            pending_load_rescan: false,
            chunk_store,
            entity_store: EntityStore::new(),
            entity_positions: HashMap::new(),
            silent_entities: HashSet::new(),
            position_set: false,
            level_load: None,
            client_loaded: false,
            options_from_game: false,
            last_render_distance: render_distance,
            last_chat_visibility: chat_options.visibility,
            last_chat_colors: chat_options.colors,
            last_main_hand_right: main_hand_right,
            last_particle_mode: particle_mode,
            local_skin_parts_mask: skin_parts_mask & 0x7f,
            last_skin_parts_mask: skin_parts_mask & 0x7f,
            last_locale: crate::lang::locale(),
            server_render_distance: 0,
            server_simulation_distance: 0,
            item_entity_store: ItemEntityStore::new(),
            particle_store: {
                let (grass, foliage, dry_foliage) = mesh_dispatcher.colormaps();
                let mut store = crate::particle::ParticleStore::new(
                    renderer.atlas_uv_map().clone(),
                    grass,
                    foliage,
                    dry_foliage,
                );
                store.set_mode(particle_mode);
                store
            },
            item_activation: None,
            block_entity_anim: BlockEntityAnimStore::default(),
            player: LocalPlayer::new(),
            item_cooldowns: crate::player::cooldown::CooldownTracker::default(),
            world_border: crate::world::border::WorldBorder::default(),
            last_bubble_pop_sound_played: 0,
            biome_climate: Arc::new(HashMap::new()),
            player_walk_pos: 0.0,
            player_walk_speed: 0.0,
            player_prev_walk_speed: 0.0,
            mesh_dispatcher,
            singleplayer,
            paused: false,
            dead: false,
            death_screen_open: false,
            show_death_screen: true,
            do_limited_crafting: false,
            win_credits: None,
            hardcore: false,
            death_message: String::new(),
            death_screen_ticks: 0,
            death_confirm: false,
            death_confirm_ticks: 0,
            respawn_sent: false,
            inventory_open: false,
            creative_inventory_open: false,
            creative_state: crate::ui::creative_inventory::CreativeState::new(),
            inventory_state_id: 0,
            cursor_item: azalea_inventory::ItemStack::Empty,
            open_container: None,
            book_edit: None,
            book_view: None,
            sign_edit: None,
            command_block_edit: None,
            container_was_open: None,
            inv_drag: None,
            inv_last_click: None,
            registries: Arc::new(azalea_core::registry_holder::RegistryHolder::default()),
            timeline_tags: Default::default(),
            chat: {
                let mut chat = ChatState::new();
                chat.set_options(chat_options);
                chat
            },
            server_dialog: None,
            server_links: Vec::new(),
            code_of_conduct: None,
            code_of_conduct_scroll: 0,
            pending_server_transfer: None,
            dialog_registry: Arc::default(),
            loom_patterns: Arc::default(),
            configuring: true,
            command_tree: None,
            tab_list: TabList::new(),
            tab_score_state: crate::ui::player_tab::TabScoreState::default(),
            server_enforces_secure_chat: false,
            waypoints: crate::world::waypoints::WaypointMap::default(),
            maps: crate::world::maps::MapStore::default(),
            tool_highlight_timer: 0,
            last_tool_highlight: azalea_inventory::ItemStack::Empty,
            action_bar: None,
            title: crate::ui::title::TitleState::default(),
            scoreboard: crate::ui::hud::Scoreboard::default(),
            local_scoreboard_name: None,
            boss_bars: crate::ui::boss_bar::BossBarState::default(),
            toasts: crate::ui::toast::ToastState::default(),
            recipe_book: crate::ui::recipe_book::RecipeBookState::default(),
            subtitles: crate::ui::subtitles::SubtitleOverlayState::default(),
            tick_count: 0,
            probe_actor_diag_tick: u64::MAX,
            saving_indicator_value: 0.0,
            last_saving_indicator_value: 0.0,
            xp_display_start_tick: i64::MIN,
            controlled_vehicle_id: None,
            riding_vehicle_id: None,
            vignette_brightness: 1.0,
            interaction: InteractionState::new(),
            sky_state: SkyState::default_day(),
            dimension_environment: Default::default(),
            pending_dimension_environment: None,
            world_clocks: HashMap::new(),
            world_clock_ids: HashMap::new(),
            world_clock_game_time: 0,
            show_debug: false,
            show_chunk_borders: false,
            advanced_item_tooltips: false,
            hide_gui: false,
            f3_chord_consumed: false,
            pending_chunk_reload: false,
            previous_game_mode: None,
            dimension: String::new(),
            cardinal_light: Default::default(),
            game_mode_switcher: None,
            spectator: Default::default(),
            switcher_was_open: false,
            last_sent_input: PlayerInputState::default(),
            last_sent_pos: Position::default(),
            last_sent_look_dir: LookDirection::default(),
            last_sent_on_ground: false,
            last_sent_horizontal_collision: false,
            was_sprinting: false,
            position_send_counter: 0,
            benchmark: None,
            benchmark_result: None,
            benchmark_saved: false,
            benchmark_upload: None,
            pause_screen: PauseScreen::Main,
            chunk_load_bench: None,
            chunk_load_result: None,
            chunk_load_abort: false,
            chunk_load_upload: None,
            last_update_phases: crate::benchmark::UpdatePhases::default(),
            movement_frame_id: 0,
            content_gen: HashMap::new(),
            next_content_gen: 0,
            meshed: HashMap::new(),
            vis_mask: HashMap::new(),
            section_gen: HashMap::new(),
            next_section_gen: 0,
            section_vis: HashMap::new(),
            compiled: HashMap::new(),
            section_vis_epoch: HashMap::new(),
            vis_tiers: HashMap::new(),
            vis_valid: false,
            last_vis_cam: (i32::MIN, i32::MIN, i32::MIN),
            last_vis_rd: u32::MAX,
            vis_task: None,
            chunk_occlusion_enabled: true,
        }
    }

    /// Vanilla `LocalPlayer.jumpableVehicle() != null`: controlling a saddled
    /// equine. Equine `getJumpCooldown()` is always 0; camels/nautilus (dash
    /// cooldown) aren't tracked by the entity store yet.
    pub fn riding_jumpable_vehicle(&self) -> bool {
        self.controlled_vehicle_id
            .and_then(|id| self.entity_store.living.get(&id))
            .is_some_and(|e| crate::entity::is_equine(&e.entity_type) && e.saddled)
    }

    /// Vanilla `Hud.getPlayerVehicleWithHealth`: (health, max health) of the
    /// ridden vehicle when it is living (`Entity.showVehicleHealth`); the
    /// living-store lookup is the `instanceof LivingEntity` gate.
    pub fn vehicle_health(&self) -> Option<(f32, f32)> {
        self.riding_vehicle_id
            .and_then(|id| self.entity_store.living.get(&id))
            .map(|e| (e.health, e.max_health))
    }

    /// A server dialog, or the confirm screen one raised, is the top screen.
    /// Vanilla runs no key mapping while a screen is up, and the screens under
    /// it neither draw nor take input.
    pub fn dialog_open(&self) -> bool {
        self.server_dialog.is_some() || self.chat.has_pending_modal_prompt()
    }

    fn server_modal_blocks_credits(&self) -> bool {
        self.code_of_conduct.is_some() || self.dialog_open()
    }

    /// Topmost screen wins, even if chat is still marked focused beneath it.
    pub(crate) fn text_owner(&self) -> input::TextOwner {
        use input::TextOwner;
        if self.level_load.is_some() {
            TextOwner::Other
        } else if self.command_block_edit.is_some() {
            TextOwner::Sign
        } else if self.sign_edit.is_some() {
            TextOwner::Sign
        } else if self.book_edit.is_some() {
            TextOwner::Book
        } else if self.dialog_open() {
            TextOwner::Dialog
        } else if self.creative_inventory_open
            && self.creative_state.tab.captures_typing()
            && !self.chat.is_open()
            && !self.options_from_game
            && !self.paused
            && !self.inventory_open
            && self.open_container.is_none()
            && self.book_view.is_none()
            && self.win_credits.is_none()
            && self.code_of_conduct.is_none()
            && self.game_mode_switcher.is_none()
        {
            TextOwner::CreativeSearch
        } else if self.gui_open() || self.options_from_game || self.paused {
            TextOwner::Other
        } else if self.chat.is_focused() {
            TextOwner::Chat
        } else {
            TextOwner::None
        }
    }

    pub fn gui_open(&self) -> bool {
        self.win_credits.is_some()
            || self.code_of_conduct.is_some()
            || self.inventory_open
            || self.creative_inventory_open
            || self.open_container.is_some()
            || self.book_edit.is_some()
            || self.book_view.is_some()
            || self.sign_edit.is_some()
            || self.command_block_edit.is_some()
            || self.dialog_open()
            || self.game_mode_switcher.is_some()
    }

    /// The container menu the player currently has open (0 = survival
    /// inventory), if any.
    pub fn open_menu_id(&self) -> Option<i32> {
        if let Some(c) = &self.open_container {
            Some(c.id)
        } else if self.inventory_open || self.creative_inventory_open {
            Some(0)
        } else {
            None
        }
    }

    /// The currently open menu's slots: the open container's, else the player
    /// inventory's.
    pub fn menu_slots(&self) -> &[azalea_inventory::ItemStack] {
        match &self.open_container {
            Some(c) => &c.slots,
            None => self.player.inventory.slots(),
        }
    }

    /// Set a slot of the currently open menu. Container slots backing the
    /// player inventory mirror into it, so the hotbar and a reopened
    /// inventory stay in sync.
    pub fn set_menu_slot(&mut self, index: usize, item: azalea_inventory::ItemStack) {
        match &mut self.open_container {
            Some(c) => {
                let Some(s) = c.slots.get_mut(index) else {
                    return;
                };
                *s = item.clone();
                let inv_start = c.inv_start();
                if index >= inv_start {
                    self.player.inventory.set_slot(index - inv_start + 9, item);
                }
            }
            None => self.player.inventory.set_slot(index, item),
        }
    }

    /// Optimistic writable-page preview only; server slot packets replace it.
    fn preview_book_pages(&mut self, slot: u32, pages: Vec<String>) {
        use azalea_core::filterable::Filterable;
        use azalea_inventory::ItemStack;
        use azalea_inventory::components::WritableBookContent;
        use azalea_registry::builtin::ItemKind;
        let index = match slot {
            0..=8 => crate::player::inventory::HOTBAR_START + slot as usize,
            40 => 45,
            _ => return,
        };
        let stack = self.player.inventory.slot(index);
        if !matches!(stack, ItemStack::Present(data) if data.kind == ItemKind::WritableBook) {
            return;
        }
        let preview = stack.clone().with_component(WritableBookContent {
            pages: pages
                .into_iter()
                .map(|raw| Filterable {
                    raw,
                    filtered: None,
                })
                .collect(),
        });
        self.player.inventory.set_slot(index, preview);
        self.sync_container_from_inventory();
    }

    /// Re-mirror the inventory-backed slots into the open container after a
    /// direct player-inventory update.
    pub fn sync_container_from_inventory(&mut self) {
        let Some(c) = &mut self.open_container else {
            return;
        };
        let inv_start = c.inv_start();
        for (i, slot) in c.slots.iter_mut().enumerate().skip(inv_start) {
            *slot = self.player.inventory.slot(i - inv_start + 9).clone();
        }
    }

    /// Record the open container's latest server state id.
    pub fn set_container_state_id(&mut self, state_id: u32) {
        if let Some(c) = &mut self.open_container {
            c.state_id = state_id;
        }
    }

    pub fn close_creative_inventory(&mut self) {
        self.creative_inventory_open = false;
        self.creative_state.reset_interaction();
    }

    /// Close whichever container menu is open. Clears the carried stack
    /// (vanilla switches to the inventory menu, whose carried stack is empty;
    /// the server returns the items via inventory sync) and any in-flight
    /// gesture so a stale drag can't commit on reopen.
    pub fn close_menu(&mut self) {
        self.inventory_open = false;
        self.open_container = None;
        self.recipe_book.reset_menu();
        self.cursor_item = azalea_inventory::ItemStack::Empty;
        self.inv_drag = None;
        self.inv_last_click = None;
    }

    /// Replaces any open server dialog; false (logged) when `reference`
    /// doesn't resolve to a dialog.
    pub fn open_server_dialog(
        &mut self,
        reference: crate::ui::server_dialog::DialogReference,
    ) -> bool {
        match crate::ui::server_dialog::ServerDialogState::open(
            reference,
            &self.dialog_registry,
            &self.server_links,
        ) {
            Ok(dialog) => {
                self.server_dialog = Some(dialog);
                true
            }
            Err(error) => {
                tracing::warn!("Could not open server dialog: {error}");
                false
            }
        }
    }

    /// A focused text field (anvil rename, creative search) is capturing
    /// keyboard input: letter/digit keys must type instead of acting as
    /// hotkeys. The anvil field is editable only while its input slot is
    /// filled, matching vanilla.
    pub fn wants_text_input(&self) -> bool {
        if (self.inventory_open
            || self.open_container.as_ref().is_some_and(|c| {
                matches!(
                    c.screen,
                    ContainerScreen::CraftingTable | ContainerScreen::Furnace(_)
                )
            }))
            && self.recipe_book.wants_text_input()
        {
            return true;
        }
        if self
            .server_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.wants_text_input())
        {
            return true;
        }
        // Read-only books still need the ordered PageUp/PageDown key stream.
        if self.book_edit.is_some()
            || self.book_view.is_some()
            || self.sign_edit.is_some()
            || self.command_block_edit.is_some()
        {
            return true;
        }
        if self.creative_inventory_open {
            return self.creative_state.tab.captures_typing();
        }
        matches!(
            &self.open_container,
            Some(c) if c.screen == ContainerScreen::Anvil
                && c.slots.first().is_some_and(|s| s.is_present())
        )
    }

    /// Applies the client-owned state carried by vanilla GameEvents. WinGame's
    /// parameter is intentionally ignored; the other flags use official 26.2
    /// float comparisons verbatim.
    pub(crate) fn apply_game_event(
        &mut self,
        event: azalea_protocol::packets::game::c_game_event::EventType,
        param: f32,
    ) {
        use azalea_protocol::packets::game::c_game_event::EventType;
        self.chat.apply_game_event_notice(event.clone());
        if is_win_game_event(&event, param) {
            if !self.respawn_sent && self.win_credits.is_none() {
                self.death_confirm = false;
                self.death_confirm_ticks = 0;
                self.win_credits = Some(crate::ui::menu::CreditsRollState::default());
            }
            return;
        }
        match event {
            EventType::ImmediateRespawn => self.show_death_screen = show_death_screen_param(param),
            EventType::LimitedCrafting => self.do_limited_crafting = limited_crafting_param(param),
            _ => {}
        }
    }

    /// Closes the death screen and its confirm, and re-arms the respawn send.
    pub fn reset_death_screen(&mut self) {
        self.death_screen_open = false;
        self.death_screen_ticks = 0;
        self.death_confirm = false;
        self.death_confirm_ticks = 0;
        self.respawn_sent = false;
    }

    /// No menu (pause, inventory, chat) is capturing input.
    pub fn input_live(&self) -> bool {
        !self.paused
            && self.win_credits.is_none()
            && !self.death_screen_open
            && !self.gui_open()
            && !self.chat.is_open()
            && self.benchmark_result.is_none()
            && self.chunk_load_result.is_none()
    }

    /// F3-family debug chords; these fire even while a menu is open, matching
    /// vanilla KeyboardHandler. Returns true if handled. The overlay itself
    /// toggles in [`Self::handle_f3_release`], not here.
    // TODO: vanilla gates hitbox/border/copy chords on the server's
    // reducedDebugInfo flag, which pomme doesn't track yet.
    pub fn handle_debug_key(
        &mut self,
        code: winit::keyboard::KeyCode,
        f3_held: bool,
        connection: &ConnectionHandle,
    ) -> bool {
        use winit::keyboard::KeyCode;
        if code == KeyCode::F3 {
            // Consumed, but acts on release so chords can suppress it.
            return true;
        }
        if !f3_held {
            return false;
        }
        let handled = match code {
            KeyCode::KeyA => {
                self.pending_chunk_reload = true;
                self.debug_feedback("Reloading all chunks");
                true
            }
            // TODO: F3+B show hitboxes (no entity hitbox renderer yet)
            KeyCode::KeyC => {
                // Vanilla also crashes the game when held for 10s; not ported.
                let p = &self.player;
                let cmd = format!(
                    "/execute in {} run tp @s {:.2} {:.2} {:.2} {:.2} {:.2}",
                    self.dimension,
                    p.position.x,
                    p.position.y,
                    p.position.z,
                    p.look_dir.y_rot_deg(),
                    p.look_dir.x_rot_deg(),
                );
                if common::set_clipboard(&cmd) {
                    self.debug_feedback("Copied location to clipboard");
                }
                true
            }
            KeyCode::KeyD => {
                self.chat.clear_messages();
                true
            }
            KeyCode::KeyG => {
                self.show_chunk_borders = !self.show_chunk_borders;
                self.debug_feedback(if self.show_chunk_borders {
                    "Chunk borders: shown"
                } else {
                    "Chunk borders: hidden"
                });
                true
            }
            KeyCode::KeyH => {
                self.advanced_item_tooltips = !self.advanced_item_tooltips;
                self.debug_feedback(if self.advanced_item_tooltips {
                    "Advanced tooltips: shown"
                } else {
                    "Advanced tooltips: hidden"
                });
                true
            }
            KeyCode::KeyI => {
                // TODO: entity variant and server-side NBT query (vanilla
                // copyRecreateCommand with addNbt/pullFromServer)
                if let Some(HitResult::Block(t)) = self.interaction.target {
                    let state = self.chunk_store.get_block_state(
                        t.block_pos.x,
                        t.block_pos.y,
                        t.block_pos.z,
                    );
                    let props = crate::world::block::block_properties(state)
                        .entries()
                        .map(|(k, v)| format!("{k}={v}"))
                        .collect::<Vec<_>>()
                        .join(",");
                    let block = crate::world::block::block_id(state);
                    let desc = if props.is_empty() {
                        block.to_string()
                    } else {
                        format!("{block}[{props}]")
                    };
                    let cmd = format!(
                        "/setblock {} {} {} {desc}",
                        t.block_pos.x, t.block_pos.y, t.block_pos.z
                    );
                    if common::set_clipboard(&cmd) {
                        self.debug_feedback("Copied client-side block data to clipboard");
                    }
                }
                true
            }
            KeyCode::F4 => {
                if let Some(switcher) = &mut self.game_mode_switcher {
                    switcher.cycle();
                } else if self.input_live() {
                    // Sent unconditionally on apply; the server refuses
                    // without permission.
                    // TODO: gate on permission level once pomme tracks it
                    // (vanilla canSwitchGameMode / debug.gamemodes.error).
                    self.game_mode_switcher =
                        Some(crate::ui::game_mode_switcher::GameModeSwitcherState::open(
                            self.player.game_mode,
                            self.previous_game_mode,
                        ));
                }
                true
            }
            KeyCode::KeyN => {
                // Sent unconditionally; the server refuses without permission.
                use azalea_core::game_type::GameMode;
                let target = if self.player.game_mode != 3 {
                    GameMode::Spectator
                } else {
                    self.previous_game_mode
                        .and_then(GameMode::from_id)
                        .unwrap_or(GameMode::Creative)
                };
                connection
                    .packet_tx
                    .send(ServerboundGamePacket::ChangeGameMode(
                    azalea_protocol::packets::game::s_change_game_mode::ServerboundChangeGameMode {
                        mode: target,
                    },
                ));
                true
            }
            KeyCode::KeyO => {
                // Pomme-specific: chunk occlusion culling toggle.
                self.chunk_occlusion_enabled = !self.chunk_occlusion_enabled;
                // Force the throttled recompute to run next frame so the
                // toggle takes effect.
                self.vis_valid = false;
                tracing::info!("Chunk occlusion: {}", self.chunk_occlusion_enabled);
                true
            }
            KeyCode::KeyV => {
                self.debug_feedback("Client version info:");
                self.chat.push_message(vec![crate::ui::text::TextSpan::new(
                    format!("Pomme Client {}", env!("CARGO_PKG_VERSION")),
                    [1.0, 1.0, 1.0, 1.0],
                )]);
                true
            }
            // TODO: F3+F6 debug options screen, F3+P pause-on-lost-focus,
            // F3+S dump dynamic textures, F3+T resource pack reload,
            // F3+L profiler, F3+1..4 debug charts (no backing features)
            _ => false,
        };
        self.f3_chord_consumed |= handled;
        handled
    }

    /// Vanilla toggles the debug overlay when F3 is released, unless a chord
    /// key consumed it as a modifier while held (KeyboardHandler.keyPress).
    /// An open game-mode switcher applies its selection instead.
    pub fn handle_f3_release(&mut self, connection: &ConnectionHandle) {
        if let Some(switcher) = self.game_mode_switcher.take() {
            use azalea_core::game_type::GameMode;
            if switcher.selected != self.player.game_mode
                && let Some(mode) = GameMode::from_id(switcher.selected)
            {
                connection
                    .packet_tx
                    .send(ServerboundGamePacket::ChangeGameMode(
                    azalea_protocol::packets::game::s_change_game_mode::ServerboundChangeGameMode {
                        mode,
                    },
                ));
            }
            self.f3_chord_consumed = false;
            return;
        }
        if self.f3_chord_consumed {
            self.f3_chord_consumed = false;
        } else {
            self.show_debug = !self.show_debug;
        }
    }

    /// Yellow bold "[Debug]:" prefix plus a plain message, vanilla
    /// `debugFeedback`.
    fn debug_feedback(&mut self, message: &str) {
        use crate::ui::text::TextSpan;
        let yellow = [1.0, 1.0, 85.0 / 255.0, 1.0];
        let mut prefix = TextSpan::new("[Debug]:".into(), yellow);
        prefix.bold = true;
        self.chat.push_message(vec![
            prefix,
            TextSpan::new(" ".into(), [1.0, 1.0, 1.0, 1.0]),
            TextSpan::new(message.into(), [1.0, 1.0, 1.0, 1.0]),
        ]);
    }

    /// Whether any field sent in `ClientInformation` differs from the last
    /// packet. The initial connection sends its own packet before this state
    /// is created.
    fn client_information_changed(
        &self,
        render_distance: u32,
        chat_options: crate::ui::chat::ChatOptions,
        main_hand_right: bool,
        particle_mode: crate::particle::ParticleMode,
        skin_parts_mask: u8,
    ) -> bool {
        self.last_skin_parts_mask != skin_parts_mask & 0x7f
            || client_information_changed(
                (
                    self.last_render_distance,
                    self.last_chat_visibility,
                    self.last_chat_colors,
                    self.last_main_hand_right,
                    self.last_particle_mode,
                    self.last_locale,
                ),
                (
                    render_distance,
                    chat_options.visibility,
                    chat_options.colors,
                    main_hand_right,
                    particle_mode,
                    crate::lang::locale(),
                ),
            )
    }

    pub fn sync_client_information(
        &mut self,
        connection: &ConnectionHandle,
        render_distance: u32,
        chat_options: crate::ui::chat::ChatOptions,
        main_hand_right: bool,
        particle_mode: crate::particle::ParticleMode,
        skin_parts_mask: u8,
    ) {
        let render_changed = self.last_render_distance != render_distance;
        let chat_changed = self.last_chat_visibility != chat_options.visibility
            || self.last_chat_colors != chat_options.colors;
        self.last_render_distance = render_distance;
        self.last_chat_visibility = chat_options.visibility;
        self.last_chat_colors = chat_options.colors;
        self.last_main_hand_right = main_hand_right;
        self.last_particle_mode = particle_mode;
        self.local_skin_parts_mask = skin_parts_mask & 0x7f;
        self.last_skin_parts_mask = skin_parts_mask & 0x7f;
        self.particle_store.set_mode(particle_mode);
        self.last_locale = crate::lang::locale();
        if render_changed {
            tracing::info!("Render distance changed to {render_distance}");
        }
        if chat_changed {
            tracing::info!(
                visibility = ?chat_options.visibility,
                colors = chat_options.colors,
                "Chat client information changed"
            );
        }

        connection
            .packet_tx
            .send(ServerboundGamePacket::ClientInformation(
                ServerboundClientInformation {
                    client_information: crate::net::client_information(
                        render_distance as u8,
                        chat_options,
                        main_hand_right,
                        particle_mode,
                        skin_parts_mask,
                    ),
                },
            ));
    }

    /// Mark every loaded column whose 5x5 biome/tint snapshot can observe a
    /// change at `centers`. The snapshot is intentionally 3x3 columns, so a
    /// load, edit, light update, or unload invalidates the same dependency set.
    pub fn bump_loaded_mesh_neighborhoods(
        &mut self,
        centers: impl IntoIterator<Item = ChunkPos>,
    ) -> HashSet<ChunkPos> {
        let loaded: HashSet<_> = self.chunk_store.loaded_positions().collect();
        let affected = bump_loaded_content_generations(
            &mut self.content_gen,
            &mut self.next_content_gen,
            centers,
            &loaded,
        );
        if !affected.is_empty() {
            self.pending_load_rescan = true;
        }
        affected
    }

    /// The chunk column the player stands in.
    pub fn player_chunk(&self) -> ChunkPos {
        ChunkPos::new(
            (self.player.position.x as i32).div_euclid(16),
            (self.player.position.z as i32).div_euclid(16),
        )
    }

    /// Runs one light update (vanilla `ClientLevel.update`, called per frame
    /// from `Minecraft.runTick`: drain queued light tasks, then
    /// `runLightUpdates`) and turns the resulting dirty scope into remesh
    /// work: columns whose chunk-load light applied go through the
    /// content-gen path like chunk loads (the visibility rescan enqueues
    /// them tier-gated), individual lit sections remesh on the priority lane.
    pub fn update_light(&mut self, renderer: &mut Renderer, chunk_detail: u32) {
        let measuring = self.benchmark.is_some();
        let pending_before = measuring.then(|| self.light_engine.pending_light_tasks());
        let mut dirty = crate::world::light::LightDirty::default();
        let engine_start = measuring.then(std::time::Instant::now);
        self.light_engine
            .poll_and_run(&mut self.chunk_store, &mut dirty);
        self.last_update_phases.light_engine_ms = engine_start
            .map(|start| start.elapsed().as_secs_f32() * 1000.0)
            .unwrap_or_default();

        if !dirty.columns.is_empty() || !dirty.sections.is_empty() {
            let mesh_start = measuring.then(std::time::Instant::now);
            let bumped = self.bump_loaded_mesh_neighborhoods(
                dirty
                    .columns
                    .iter()
                    .copied()
                    .map(|(x, z)| ChunkPos::new(x, z)),
            );
            let player_chunk = self.player_chunk();
            let min_section_y = self.chunk_store.min_y() >> 4;
            let section_count = self.chunk_store.section_count();
            let sections = dirty.sections.iter().filter_map(|key| {
                let si = key.y - min_section_y;
                let col = ChunkPos::new(key.x, key.z);
                // Padding/out-of-range sections have no mesh; columns already
                // bumped above remesh wholesale anyway.
                (si >= 0
                    && si < section_count
                    && !bumped.contains(&col)
                    && self.chunk_store.get_chunk(&col).is_some())
                .then_some((col, si))
            });
            for (col, range) in consecutive_section_runs(sections) {
                self.enqueue_section_edit(
                    renderer,
                    col,
                    range,
                    crate::app::core::chunk_lod(col, player_chunk, chunk_detail),
                );
            }
            self.last_update_phases.light_mesh_ms = mesh_start
                .map(|start| start.elapsed().as_secs_f32() * 1000.0)
                .unwrap_or_default();
        } else {
            self.last_update_phases.light_mesh_ms = 0.0;
        }

        // `update_light` is synchronous and neither poll-and-run nor remesh enqueue
        // inserts light tasks, so the pending-count delta is the processed count.
        if let Some(before) = pending_before {
            let after = self.light_engine.pending_light_tasks();
            self.last_update_phases.light_tasks_processed = before.saturating_sub(after);
            self.last_update_phases.light_tasks_pending = after;
        } else {
            self.last_update_phases.light_tasks_processed = 0;
            self.last_update_phases.light_tasks_pending = 0;
        }
    }

    /// Mesh edited sections on the priority lane, ungated by visibility.
    /// Every section in the span gets the same generation for stale rejection.
    pub fn enqueue_section_edit(
        &mut self,
        renderer: &mut Renderer,
        col: ChunkPos,
        sections: std::ops::Range<i32>,
        lod: u32,
    ) {
        let g = self.bump_section_gen(col, sections.clone());
        renderer.expect_block_edit_mesh(col, sections.clone(), g);
        self.mesh_dispatcher.enqueue(
            &self.chunk_store,
            &self.block_entity_anim,
            col,
            lod,
            true,
            g,
            self.content_gen.get(&col).copied().unwrap_or(0),
            sections,
        );
    }

    /// One gen for the whole span: the drain stale-check compares every
    /// section in a mesh's `replaced` range against its single
    /// `content_gen`, so grouped sections must share a value.
    fn bump_section_gen(&mut self, col: ChunkPos, sections: std::ops::Range<i32>) -> u64 {
        bump_section_generations(
            &mut self.section_gen,
            &mut self.next_section_gen,
            col,
            sections,
        )
    }

    /// Collect the frame's ready meshes, apply their CPU-side bookkeeping, then
    /// upload them in one coalesced GPU transfer (one fence wait, not one per
    /// mesh) to avoid the streaming stutter from per-mesh `queue.wait_idle`.
    /// Shared with the loading phase, which streams the spawn chunks in before
    /// the game phase takes over.
    pub fn drain_and_upload_meshes(&mut self, renderer: &mut Renderer) {
        let drain_start = std::time::Instant::now();
        let results: Vec<_> = self.mesh_dispatcher.drain_results().collect();
        let mut batch = Vec::with_capacity(results.len());
        for mut mesh in results {
            // Stale meshes count too: worker time spent is worker time spent.
            if let Some(bench) = &mut self.chunk_load_bench {
                bench.record_mesh(mesh.queue_ms, mesh.mesh_ms);
            }
            // Drop a mesh built from an out-of-date snapshot. A mesh for a chunk
            // that has since unloaded is always stale (uploading it would resurrect
            // a column nothing cleans up). Grouped edits can overlap newer jobs
            // with different queue keys; bulk loads keep the column key.
            let stale = mesh_result_is_stale(
                &self.chunk_store,
                &self.content_gen,
                &self.section_gen,
                mesh.pos,
                mesh.content_gen,
                mesh.replaced.clone(),
                mesh.timing.is_some(),
                mesh.column_revision,
            );
            if mesh.timing.is_some() {
                renderer.record_edit_mesh_result(
                    &mesh,
                    stale,
                    self.content_gen.get(&mesh.pos).copied().unwrap_or(0),
                );
            }
            if stale {
                if mesh.timing.is_some() {
                    // Rebuild only sections not covered by a newer edit. A fresh
                    // snapshot also covers intervening chunk/neighbor changes;
                    // do not enqueue anything for an unloaded column.
                    let runs = current_edit_section_runs(
                        &self.chunk_store,
                        &self.section_gen,
                        mesh.pos,
                        mesh.content_gen,
                        mesh.replaced.clone(),
                    );
                    let lod = self.meshed.get(&mesh.pos).map_or(0, |m| m.lod);
                    for (_, range) in runs {
                        self.enqueue_section_edit(renderer, mesh.pos, range, lod);
                    }
                }
                self.mesh_dispatcher.recycle(mesh);
                continue;
            }
            if let Some(t) = &mesh.timing {
                let ms = |d: std::time::Duration| d.as_secs_f32() * 1000.0;
                tracing::debug!(
                    "edit remesh [{}, {}]: queue {:.1}ms + mesh {:.1}ms + drain {:.1}ms = {:.1}ms",
                    mesh.pos.x,
                    mesh.pos.z,
                    ms(t.started_at - t.enqueued_at),
                    ms(t.meshed_at - t.started_at),
                    ms(t.meshed_at.elapsed()),
                    ms(t.enqueued_at.elapsed()),
                );
            }
            // Taken before the upload so the mesh can move into the batch; the
            // upload reports back what it had to drop.
            self.apply_mesh_bookkeeping(&mut mesh);
            batch.push(mesh);
        }
        self.last_update_phases.mesh_drain_ms = drain_start.elapsed().as_secs_f32() * 1000.0;
        let upload_start = std::time::Instant::now();
        let dropped = renderer.upload_chunk_meshes(&batch);
        self.last_update_phases.upload_ms = upload_start.elapsed().as_secs_f32() * 1000.0;
        self.clear_dropped_meshed(dropped);
        // Return the uploaded meshes' buffers to the worker pool for reuse.
        for mesh in batch {
            self.mesh_dispatcher.recycle(mesh);
        }
    }

    /// Vanilla `SectionUpdateTracker.hasAllNeighbors` plus
    /// `LevelRenderer.isSectionCompiledAndVisible`: the section holding
    /// `camera_block` may only have compiled once its column's whole 3x3
    /// neighbourhood was loaded and lit, and it must have a mesh — an empty one
    /// counts, as vanilla's empty `CompiledSectionMesh` does.
    pub fn camera_section_ready(&self, camera_block: glam::IVec3) -> bool {
        let column = ChunkPos::new(camera_block.x >> 4, camera_block.z >> 4);
        let neighbourhood_lit = crate::world::chunk::column_neighborhood(column).all(|p| {
            self.chunk_store.get_chunk(&p).is_some()
                && self.light_engine.light_on_in_column((p.x, p.z))
        });
        let section = (camera_block.y - self.chunk_store.min_y()) >> 4;
        let compiled = self
            .compiled
            .get(&column)
            .is_some_and(|mask| mask & section_bit(section) != 0);
        neighbourhood_lit && compiled
    }

    /// Vanilla `ClientPacketListener.handleLogin`/`handleRespawn`: clear the
    /// loaded flag and start waiting for the new level.
    pub fn start_level_load(&mut self) {
        self.client_loaded = false;
        // TODO: vanilla gives a newly created singleplayer world a 500ms close
        // delay (`Minecraft.doWorldLoad`); Pomme can't tell a fresh world from
        // an opened one yet.
        let mut tracker = LevelLoadTracker::start_client_load(
            std::time::Duration::ZERO,
            std::time::Instant::now(),
        );
        // 1.20.1 and 1.20.2 have no LEVEL_CHUNKS_LOAD_START game event (1.20.4
        // added it), so nothing would ever move the tracker on; those clients
        // had the level from the login packet.
        if crate::version::session_protocol() < 765 {
            tracker.loading_packets_received();
        }
        self.level_load = Some(tracker);
    }

    /// Adopt a finished mesh's CPU-side state: its per-section visibility sets,
    /// epoch-guarded so a stale result can't overwrite a newer edit's
    /// visibility, and the sections it compiled. The upload can still drop a
    /// section afterwards, which `clear_dropped_meshed` takes back out.
    fn apply_mesh_bookkeeping(&mut self, mesh: &mut ChunkMeshData) {
        let pos = mesh.pos;
        for (si, vis) in std::mem::take(&mut mesh.visibility) {
            let e = self.section_vis_epoch.entry((pos, si)).or_insert(0);
            if mesh.upload_epoch >= *e {
                *e = mesh.upload_epoch;
                self.section_vis.insert((pos, si), vis);
            }
        }
        *self.compiled.entry(pos).or_default() |= section_bits(mesh.replaced.clone());
    }

    /// Sections dropped on pool exhaustion were retired from the buffer; clear
    /// their meshed bit so the next rescan re-enqueues them, and their compiled
    /// bit, since nothing of them reached the GPU.
    fn clear_dropped_meshed(&mut self, dropped: Vec<(ChunkPos, Vec<i32>)>) {
        for (pos, sections) in dropped {
            let retired = section_bits(sections);
            if let Some(m) = self.meshed.get_mut(&pos) {
                m.mask &= !retired;
            }
            if let Some(mask) = self.compiled.get_mut(&pos) {
                *mask &= !retired;
            }
        }
    }

    /// Synchronously remesh diagnostic samples; gameplay edits are queued.
    pub fn remesh_probe_targets(
        &mut self,
        renderer: &mut Renderer,
        samples: &[(i32, i32, i32, azalea_block::BlockState)],
    ) {
        let mut sections = HashSet::new();
        for &(x, y, z, _) in samples.iter().take(3) {
            let col = ChunkPos::new(x.div_euclid(16), z.div_euclid(16));
            let si = (y - self.chunk_store.min_y()).div_euclid(16);
            if self.chunk_store.get_chunk(&col).is_some() {
                sections.insert((col, si));
            }
        }
        for ((col, si), _) in sections.into_iter().map(|key| (key, ())) {
            let g = self.bump_section_gen(col, si..si + 1);
            let mesh = self.mesh_dispatcher.mesh_sections_now(
                &self.chunk_store,
                &self.block_entity_anim,
                col,
                si..si + 1,
                g,
            );
            self.apply_mesh_upload(renderer, mesh);
        }
    }

    fn apply_mesh_upload(&mut self, renderer: &mut Renderer, mut mesh: ChunkMeshData) {
        self.apply_mesh_bookkeeping(&mut mesh);
        let dropped = renderer.upload_chunk_meshes(std::slice::from_ref(&mesh));
        self.clear_dropped_meshed(dropped);
        self.mesh_dispatcher.recycle(mesh);
    }

    fn effective_render_distance(&self) -> u32 {
        if self.server_render_distance > 0 {
            self.last_render_distance.min(self.server_render_distance)
        } else {
            self.last_render_distance
        }
    }

    /// Drive the cave-cull occlusion walk: apply a finished async walk to the
    /// per-column draw masks, then schedule the next one on 8-block camera
    /// movement or chunk loads (one at a time, off the main thread — vanilla's
    /// async, movement-gated cadence). The walk is rotation-independent;
    /// frustum culling runs per-frame on the GPU.
    pub fn update_visibility(
        &mut self,
        renderer: &mut Renderer,
        player_chunk: ChunkPos,
        loads_happened: bool,
    ) {
        // Before the camera is placed the frustum is meaningless, so trust
        // nothing and let the queue mesh everything nearest-first.
        if !self.position_set {
            if self.vis_valid {
                self.vis_valid = false;
                self.vis_tiers.clear();
            }
            return;
        }

        // Apply a finished walk (its result lags a few frames, like vanilla's).
        let finished = self.vis_task.as_ref().and_then(|rx| rx.try_recv().ok());
        if let Some(bfs) = finished {
            self.vis_task = None;
            self.apply_visibility(renderer, &bfs);
        }

        // Schedule the next walk on 8-block movement, chunk loads, or an
        // invalidated result (`!vis_valid`, e.g. the F3+O toggle forcing a
        // recompute while stationary), one in flight.
        let eye = renderer.camera_render_position();
        let cam_bucket = (
            (eye.x / 8.0).floor() as i32,
            (eye.y / 8.0).floor() as i32,
            (eye.z / 8.0).floor() as i32,
        );
        if self.vis_task.is_none()
            && (!self.vis_valid
                || cam_bucket != self.last_vis_cam
                || loads_happened
                || self.last_vis_rd != self.effective_render_distance())
        {
            self.last_vis_cam = cam_bucket;
            self.last_vis_rd = self.effective_render_distance();
            let section_vis = self.section_vis.clone();
            let min_y = self.chunk_store.min_y();
            let n = self.chunk_store.section_count();
            let cam_si = ((eye.y - min_y as f64) / 16.0).floor() as i32;
            // Servers may stream beyond our view; never walk all received chunks.
            let rd = self.effective_render_distance() as i32;
            let (tx, rx) = crossbeam_channel::bounded(1);
            std::thread::spawn(move || {
                let bfs = occlusion_graph::compute_visible_mask(
                    &section_vis,
                    player_chunk,
                    cam_si,
                    eye,
                    min_y,
                    n,
                    rd,
                );
                let _ = tx.send(bfs);
            });
            self.vis_task = Some(rx);
        }
    }

    /// Combine a finished walk with the current camera frustum into per-column
    /// draw masks (occluded sections omitted) and tiers, and push them to the
    /// GPU cull.
    fn apply_visibility(&mut self, renderer: &mut Renderer, bfs: &HashMap<ChunkPos, u32>) {
        let planes = renderer.frustum_planes();
        let planes_wide = renderer.frustum_planes_dilated(VIS_MARGIN_RADIANS);
        let eye_f = renderer.camera_render_position();
        let min_y = self.chunk_store.min_y() as f32;
        let max_y = min_y + self.chunk_store.height() as f32;
        let full = section_mask(self.chunk_store.section_count());

        let mut tiers = HashMap::new();
        let mut masks = HashMap::new();
        let player_chunk = self.player_chunk();
        let rd = self.effective_render_distance();
        for pos in self.chunk_store.loaded_positions() {
            if !column_in_render_distance(pos, player_chunk, rd) {
                masks.insert(pos, 0);
                continue;
            }
            let near = column_is_near(pos, eye_f);
            let tier = if near {
                0
            } else {
                column_frustum_tier(pos, eye_f, &planes, &planes_wide, min_y, max_y)
            };
            // Near columns always draw fully; otherwise a column draws only the
            // sections the graph proved occlusion-visible (none => fully hidden).
            let mask = if near {
                full
            } else {
                bfs.get(&pos).copied().unwrap_or(0)
            };
            // A fully-occluded column (no visible section) drops to the hidden tier.
            let tier = if tier == 0 && mask == 0 { 2 } else { tier };
            tiers.insert(pos, tier);
            masks.insert(pos, mask);
        }
        self.vis_tiers = tiers;
        self.vis_valid = true;

        // With occlusion off, push full masks inside render distance only.
        if !self.chunk_occlusion_enabled {
            for (pos, m) in &mut masks {
                if column_in_render_distance(*pos, player_chunk, rd) {
                    *m = full;
                }
            }
        }
        // This copy must match the draw mask, including the occlusion-off
        // override and out-of-render-distance zeroes.
        self.vis_mask = masks.clone();
        renderer.set_chunk_visibility(masks);
    }

    /// Enqueue all not-yet-meshed sections within render distance. Visibility
    /// and frustum culling affect drawing only, never whether a section meshes.
    pub fn rescan_mesh_jobs(
        &mut self,
        _renderer: &Renderer,
        player_chunk: ChunkPos,
        chunk_detail: u32,
    ) {
        let n = self.chunk_store.section_count();
        let full = section_mask(n);
        let rd = self.effective_render_distance();
        for pos in self.chunk_store.loaded_positions() {
            if !column_in_render_distance(pos, player_chunk, rd) {
                continue;
            }
            let target = mesh_target_mask(pos, player_chunk, rd, full);
            if target == 0 {
                continue;
            }
            let lod = crate::app::core::chunk_lod(pos, player_chunk, chunk_detail);
            let content_gen = self.content_gen.get(&pos).copied().unwrap_or(0);
            // Only mark sections actually queued; a later visibility walk can
            // reveal more sections without a content or LOD change.
            // TODO: vanilla won't schedule a section's first compile until its
            // 3x3 column neighbourhood is loaded and lit
            // (`LevelExtractor.java:155` / `SectionUpdateTracker.hasAllNeighbors`);
            // we mesh against missing neighbours as air and repair the borders
            // when their light bumps `content_gen`.
            let already_meshed = self
                .meshed
                .get(&pos)
                .filter(|m| m.lod == lod && m.content_gen == content_gen)
                .map_or(0, |m| m.mask);
            let to_mesh = target & !already_meshed;
            if to_mesh != 0 {
                for (start, end) in contiguous_runs(to_mesh) {
                    self.mesh_dispatcher.enqueue(
                        &self.chunk_store,
                        &self.block_entity_anim,
                        pos,
                        lod,
                        false,
                        content_gen,
                        content_gen,
                        start..end,
                    );
                }
            }
            self.meshed.insert(
                pos,
                MeshedCol {
                    lod,
                    content_gen,
                    mask: target | already_meshed,
                },
            );
        }
    }
}

fn column_in_render_distance(pos: ChunkPos, center: ChunkPos, rd: u32) -> bool {
    pos.x.abs_diff(center.x).max(pos.z.abs_diff(center.z)) <= rd
}

fn mesh_target_mask(pos: ChunkPos, center: ChunkPos, rd: u32, full: u32) -> u32 {
    if column_in_render_distance(pos, center, rd) {
        full
    } else {
        0
    }
}

/// Extra FOV (radians) for the tier-1 "about to be seen" margin frustum, so
/// small camera turns reveal already-meshed terrain instead of a meshing
/// curtain.
const VIS_MARGIN_RADIANS: f32 = 0.6;

/// Frustum tier for a column: 0 in view, 1 in the dilated margin, 2 behind the
/// camera. (Nearby columns are forced to 0 by the caller.)
fn column_frustum_tier(
    pos: ChunkPos,
    eye: glam::DVec3,
    planes: &[[f32; 4]; 6],
    planes_wide: &[[f32; 4]; 6],
    min_y: f32,
    max_y: f32,
) -> u8 {
    // Camera-relative full-height column box, matching how the GPU cull
    // subtracts the eye before its plane test (cull.comp); f64 first for
    // precision at extreme coordinates.
    let dx = (pos.x as f64 * 16.0 - eye.x) as f32;
    let dz = (pos.z as f64 * 16.0 - eye.z) as f32;
    let mn = [dx, (min_y as f64 - eye.y) as f32, dz];
    let mx = [dx + 16.0, (max_y as f64 - eye.y) as f32, dz + 16.0];
    if aabb_in_frustum(&mn, &mx, planes) {
        0
    } else if aabb_in_frustum(&mn, &mx, planes_wide) {
        1
    } else {
        2
    }
}

pub(crate) const fn server_tick_runs(frozen: bool, steps: u32) -> bool {
    !frozen || steps > 0
}

pub(crate) fn server_time_tick_period(rate: f32) -> f32 {
    if rate.is_finite() && rate >= 1.0 {
        1.0 / rate
    } else {
        TICK_RATE
    }
}

fn tick_cloaks(player: &mut crate::player::LocalPlayer, entities: &mut crate::entity::EntityStore) {
    player.cloak.tick(*player.position);
    for (&id, entity) in entities.living.iter_mut() {
        entity.cloak.tick(*entity.position);
        if id != player.entity_id && entity.entity_type == EntityKind::Player {
            entity.cape_motion.tick_remote(
                *entity.position,
                entity.velocity,
                entity.on_ground,
                entity.health <= 0.0 || entity.death_time > 0,
                entity.flags.swimming,
                entity.flags.fall_flying,
            );
        }
    }
    for vehicle in entities.vehicles.values_mut() {
        if vehicle.kind == Some(azalea_registry::builtin::EntityKind::Mannequin) {
            vehicle.cloak.tick(*vehicle.position);
            vehicle
                .cape_motion
                .tick_mannequin(vehicle.shared_flags & 0x80 != 0);
        }
    }
}

fn advance_world_clock_tick(
    game_time: &mut i64,
    clocks: &mut HashMap<u32, crate::net::environment::ClockSample>,
) {
    *game_time = game_time.wrapping_add(1);
    for clock in clocks.values_mut() {
        clock.advance_game_time(1);
    }
}

pub(crate) fn advance_server_time(
    accumulator: &mut f32,
    dt: f32,
    tick_rate: f32,
    frozen: bool,
    steps: &mut u32,
    sky: &mut SkyState,
    _clocks: &mut HashMap<u32, crate::net::environment::ClockSample>,
) -> u32 {
    let period = server_time_tick_period(tick_rate);
    *accumulator = (*accumulator + dt.max(0.0)).min(1.0);
    let mut ticks = 0;
    while *accumulator + 1e-6 >= period {
        if server_tick_runs(frozen, *steps) {
            sky.advance_clock_tick();
            sky.game_time = sky.game_time.wrapping_add(1);
            if frozen {
                *steps -= 1;
            }
            ticks += 1;
        }
        *accumulator = (*accumulator - period).max(0.0);
    }
    ticks
}

/// Bit for one section index, 0 outside a column's 32 addressable sections (a
/// camera outside build height resolves to such an index).
fn section_bit(si: i32) -> u32 {
    if (0..32).contains(&si) { 1u32 << si } else { 0 }
}

/// The bits for several section indices.
fn section_bits(indices: impl IntoIterator<Item = i32>) -> u32 {
    indices
        .into_iter()
        .fold(0u32, |mask, si| mask | section_bit(si))
}

/// Full mask for an `n`-section column (bits `0..n` set).
fn section_mask(n: i32) -> u32 {
    if n >= 32 { u32::MAX } else { (1u32 << n) - 1 }
}

/// Contiguous `(start, end)` index runs of set bits in `mask`, so a (usually
/// contiguous) visible set enqueues as a few range jobs — one gather per run.
fn contiguous_runs(mask: u32) -> Vec<(i32, i32)> {
    let mut runs = Vec::new();
    let mut i = 0i32;
    while i < 32 {
        if mask & (1u32 << i) != 0 {
            let start = i;
            while i < 32 && mask & (1u32 << i) != 0 {
                i += 1;
            }
            runs.push((start, i));
        } else {
            i += 1;
        }
    }
    runs
}

/// Conservative AABB-vs-frustum test (the dominant-corner max-dot used by
/// `cull.comp`): true unless the box is fully behind some plane.
fn aabb_in_frustum(mn: &[f32; 3], mx: &[f32; 3], planes: &[[f32; 4]]) -> bool {
    for p in planes {
        let d = p[0] * if p[0] >= 0.0 { mx[0] } else { mn[0] }
            + p[1] * if p[1] >= 0.0 { mx[1] } else { mn[1] }
            + p[2] * if p[2] >= 0.0 { mx[2] } else { mn[2] }
            + p[3];
        if d < 0.0 {
            return false;
        }
    }
    true
}

/// A deliberately loose world-space box for the BE models we own. The 8-block
/// radius encloses all four statue poses, rotated/double/open chest lids and
/// locks, rotated/lifted shulker lids, heads (including the hat/wall offset),
/// conduit and empty sign boards. Half a block is spare for FP plane tests.
/// Text glyph metrics come from replaceable font assets: without a proven
/// bound for those metrics, never reject a sign with text (including glow).
fn sign_has_text(front: Option<&[String; 4]>, back: Option<&[String; 4]>) -> bool {
    [front, back]
        .into_iter()
        .flatten()
        .flatten()
        .any(|line| !line.is_empty())
}

fn sign_text_in_range(pos: &BlockPos, player_eye: glam::DVec3) -> bool {
    glam::DVec3::new(pos.x as f64 + 0.5, pos.y as f64 + 0.5, pos.z as f64 + 0.5)
        .distance_squared(player_eye)
        <= 256.0
}

// Use the same section mask as the chunk draw list. Missing/invalid visibility
// is not proof of a visible chunk: retain the BE until the next graph update.
fn chest_section_visible(si: i32, valid: bool, mask: Option<u32>) -> bool {
    valid && (0..32).contains(&si) && mask.is_some_and(|mask| mask & (1u32 << si) != 0)
}

fn block_entity_in_frustum(
    kind: BlockEntityKind,
    pos: &BlockPos,
    has_sign_text: bool,
    eye: glam::DVec3,
    planes: &[[f32; 4]; 6],
) -> bool {
    // Camera::planes_from_view_projection orders left/right, bottom/top,
    // near, far. The far plane can still belong to the previous frame's RD:
    // render_world updates it only after BE extraction. GPU projection clips
    // beyond the current far plane, so only reject against the first five.
    let planes = &planes[..5];
    if !matches!(
        kind,
        BlockEntityKind::Chest
            | BlockEntityKind::TrappedChest
            | BlockEntityKind::EnderChest
            | BlockEntityKind::ShulkerBox
            | BlockEntityKind::Conduit
            | BlockEntityKind::Skull
            | BlockEntityKind::CopperGolemStatue
            | BlockEntityKind::Sign
    ) || kind == BlockEntityKind::Sign && has_sign_text
        || !eye.is_finite()
        || planes
            .iter()
            .any(|p| !p.iter().all(|v| v.is_finite()) || p[..3].iter().all(|v| *v == 0.0))
    {
        return true;
    }
    let center = glam::DVec3::new(pos.x as f64 + 0.5, pos.y as f64 + 0.5, pos.z as f64 + 0.5);
    let delta = (center - eye).as_vec3();
    let mn = (delta - glam::Vec3::splat(8.5)).to_array();
    let mx = (delta + glam::Vec3::splat(8.5)).to_array();
    aabb_in_frustum(&mn, &mx, planes)
}

pub enum GameUpdateResult {
    None,
    ManualDisconnect,
    Disconnected { reason: String },
    Transfer(crate::net::ServerTransfer),
}

enum ResultKind {
    Fps,
    ChunkLoad,
}

fn handle_chat_ui_action(
    action: ChatUiAction,
    core: &mut AppCore,
    connection: &ConnectionHandle,
    game: &mut GameState,
) {
    match action {
        ChatUiAction::OpenUrl(url) => {
            let Ok(url) = crate::chat_component::parse_untrusted_url(url) else {
                return;
            };
            if let Err(e) = open::that(&url) {
                tracing::warn!("Could not open chat link {url:?}: {e}");
            }
        }
        ChatUiAction::OpenChatSettings => {
            game.chat.close_for_settings();
            core.menu.open_chat_settings();
            game.options_from_game = true;
            game.paused = true;
        }
        ChatUiAction::RunCommand(command) => {
            handle_unattended_command(&command, connection, game);
        }
        ChatUiAction::RunCommandUnsigned(command) => {
            connection
                .packet_tx
                .send_raw(crate::net::chat::encode_outbound_command(&command));
        }
        ChatUiAction::Custom { id, payload } => connection.packet_tx.send_custom_click(id, payload),
        // The chat screen stays as the dialog's `previousScreen`.
        ChatUiAction::ShowDialog(dialog) => {
            game.open_server_dialog(crate::ui::server_dialog::DialogReference::Holder(dialog));
        }
    }
}

fn handle_unattended_command(command: &str, connection: &ConnectionHandle, game: &mut GameState) {
    use crate::net::commands::UnattendedCommandCheck;
    use crate::ui::chat::CommandConfirmationKind;

    let command = command.strip_prefix('/').unwrap_or(command);
    let check = game
        .command_tree
        .as_ref()
        .map_or(UnattendedCommandCheck::ParseErrors, |tree| {
            tree.verify_unattended(command)
        });
    match CommandConfirmationKind::for_check(check) {
        Some(kind) => game
            .chat
            .request_command_confirmation(command.to_owned(), kind),
        None => {
            connection
                .packet_tx
                .send_raw(crate::net::chat::encode_outbound_command(command));
            // `setScreen(screenAfterCommand)` re-adds ChatScreen, whose
            // `removed` resets the scroll.
            game.chat.reset_chat_scroll();
        }
    }
}

/// Drops the dialog if the input just handled finished it, then carries out
/// the action it reported.
pub(crate) fn settle_server_dialog(
    action: Option<crate::ui::server_dialog::ServerDialogAction>,
    core: &mut AppCore,
    connection: &ConnectionHandle,
    game: &mut GameState,
) {
    use crate::ui::server_dialog::ServerDialogAction;

    // Vanilla swaps in the after-action screen only where the click event
    // reaches `setScreen` (`DialogScreen.runAction`).
    let (activate, follow_up) = match action {
        None => (false, None),
        // `Screen.clickUrlAction`: the confirm screen replaces the dialog,
        // while opening the link straight away (or chat links being off)
        // leaves the screen alone.
        Some(ServerDialogAction::OpenUrl(url)) => match game.chat.request_open_url(url) {
            Some(action) => (false, Some(action)),
            None => (game.chat.has_pending_modal_prompt(), None),
        },
        // `ClientConfigurationPacketListenerImpl.createDialogAccess`.
        Some(ServerDialogAction::RunCommand(command)) if game.configuring => {
            tracing::warn!(
                "Commands are not supported in configuration phase, trying to run '{command}'"
            );
            (false, None)
        }
        Some(ServerDialogAction::RunCommand(command)) => {
            (true, Some(ChatUiAction::RunCommand(command)))
        }
        Some(ServerDialogAction::Custom { id, payload }) => {
            (true, Some(ChatUiAction::Custom { id, payload }))
        }
        // `showDialog` only warns when the dialog doesn't resolve, leaving the
        // current one up.
        Some(ServerDialogAction::ShowDialog(reference)) => {
            game.open_server_dialog(reference);
            (false, None)
        }
    };
    if activate && let Some(dialog) = game.server_dialog.as_mut() {
        dialog.activate();
    }
    if game
        .server_dialog
        .as_ref()
        .is_some_and(|dialog| dialog.is_finished())
    {
        game.server_dialog = None;
    }
    if let Some(action) = follow_up {
        handle_chat_ui_action(action, core, connection, game);
    }
}

/// The server dialog and the confirm screen a chat or dialog link opens over
/// it, with their clicks settled. The connecting screen shares it for
/// configuration-phase dialogs.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_server_screens(
    elements: &mut Vec<MenuElement>,
    sw: f32,
    sh: f32,
    gs: f32,
    core: &mut AppCore,
    gfx: &Gfx,
    connection: &ConnectionHandle,
    game: &mut GameState,
    // The client tick count, or `None` where the phase runs no game ticks.
    tick: Option<u64>,
    text_events: &[crate::ui::text_edit::TextInputEvent],
) {
    if game.command_block_edit.is_some() {
        let cursor = core.input.cursor_pos();
        let pressed = core.input.left_just_pressed();
        let done = pressed
            && crate::ui::common::hit_test(cursor, crate::ui::command_block::done_rect(sw, sh, gs));
        let cancel = (pressed
            && crate::ui::common::hit_test(
                cursor,
                crate::ui::command_block::cancel_rect(sw, sh, gs),
            ))
            || core.input.escape_pressed();
        if let Some(edit) = &mut game.command_block_edit {
            edit.input(text_events, &|s| {
                gfx.renderer.menu_text_width(s, common::FONT_SIZE)
            });
            if pressed {
                for i in 0..4 {
                    if crate::ui::common::hit_test(
                        cursor,
                        crate::ui::command_block::toggle_rect(sw, sh, gs, i),
                    ) {
                        edit.toggle(i);
                        break;
                    }
                }
            }
            edit.draw(elements, sw, sh, gs, &|s| {
                gfx.renderer.menu_text_width(s, common::FONT_SIZE)
            });
            if done {
                let (command, mode, flags) = edit.packet();
                if connection
                    .packet_tx
                    .set_command_block(edit.pos, &command, mode, flags)
                {
                    game.command_block_edit = None;
                    game.interaction.pending_command_block = None;
                    core.apply_cursor_grab(gfx.window.as_ref(), Some(game));
                }
            } else if cancel {
                game.command_block_edit = None;
                game.interaction.pending_command_block = None;
                core.apply_cursor_grab(gfx.window.as_ref(), Some(game));
            }
        }
        core.input.consume_left_just_pressed();
        core.input.clear_just_pressed_actions();
        return;
    }
    if game.sign_edit.is_some() {
        let cursor = core.input.cursor_pos();
        let done = core.input.left_just_pressed()
            && crate::ui::common::hit_test(cursor, crate::ui::sign::done_rect(sw, sh, gs));
        let close = done || core.input.escape_pressed();
        if let Some(sign) = &mut game.sign_edit {
            sign.input(text_events, &|s| {
                gfx.renderer.menu_text_width(s, common::FONT_SIZE)
            });
            sign.draw(elements, sw, sh, gs, &|s| {
                gfx.renderer.menu_text_width(s, common::FONT_SIZE)
            });
            if close {
                connection
                    .packet_tx
                    .sign_update(sign.pos, sign.is_front_text, sign.lines());
                game.sign_edit = None;
                core.apply_cursor_grab(gfx.window.as_ref(), Some(game));
            }
        }
        core.input.consume_left_just_pressed();
        core.input.clear_just_pressed_actions();
        return;
    }
    if game.book_view.is_some() {
        let cursor = core.input.cursor_pos();
        // The confirmation screen owns clicks and Escape, not the book below.
        if game.chat.has_pending_modal_prompt() {
            if let Some(book) = &mut game.book_view {
                book.draw(elements, sw, sh, gs, cursor, &|spans, scale| {
                    gfx.renderer.menu_spans_width(spans, scale)
                });
            }
            if let Some(action) = game.chat.build_modal_prompt(
                elements,
                sw,
                sh,
                gs,
                cursor,
                core.input.left_just_pressed(),
                &|spans, scale| gfx.renderer.menu_spans_width(spans, scale),
            ) {
                handle_chat_ui_action(action, core, connection, game);
            }
            if core.input.escape_pressed() {
                game.chat.handle_escape();
            }
            core.input.consume_left_just_pressed();
            core.input.clear_just_pressed_actions();
            return;
        }
        let (page, pages) = game
            .book_view
            .as_ref()
            .map_or((0, 1), |book| (book.page, book.pages.len()));
        let pressed = core.input.left_just_pressed();
        let done = pressed && crate::ui::book::view_done_hit(cursor, sw, sh, gs);
        let action = pressed
            .then(|| crate::ui::book::view_hit_action(cursor, sw, sh, gs, page, pages))
            .flatten();
        let mut clicked_style = None;
        if let Some(book) = &mut game.book_view {
            for event in text_events {
                if let crate::ui::text_edit::TextInputEvent::Key { code, .. } = event {
                    book.key(*code);
                }
            }
            if let Some(index @ 0..=1) = action {
                book.navigate(index);
            } else if pressed && !done {
                clicked_style = book
                    .style_at(cursor)
                    .and_then(|style| style.click_event.clone());
                if let Some(crate::chat_component::ClickEvent::ChangePage(target)) = clicked_style {
                    if target > 0 && !book.pages.is_empty() {
                        book.set_page((target as usize) - 1);
                    }
                    clicked_style = None;
                }
            }
            core.input.consume_left_just_pressed();
            book.draw(elements, sw, sh, gs, cursor, &|spans, scale| {
                gfx.renderer.menu_spans_width(spans, scale)
            });
            book.draw_hover(
                elements,
                cursor,
                sw,
                sh,
                gs,
                game.advanced_item_tooltips,
                &|spans| gfx.renderer.menu_spans_width(spans, common::FONT_SIZE),
            );
        }
        if let Some(click) = clicked_style {
            use crate::chat_component::ClickEvent;
            let action = match click {
                ClickEvent::OpenUrl(url) => game.chat.request_open_url(url),
                ClickEvent::RunCommand(command) => Some(ChatUiAction::RunCommand(command)),
                ClickEvent::CopyToClipboard(value) => {
                    common::set_clipboard(&value);
                    None
                }
                ClickEvent::ShowDialog(dialog) => Some(ChatUiAction::ShowDialog(dialog)),
                ClickEvent::Custom { id, payload } => Some(ChatUiAction::Custom { id, payload }),
                // A read-only book has no command-input box.
                ClickEvent::SuggestCommand(_) | ClickEvent::ChangePage(_) => None,
            };
            if let Some(action) = action {
                if matches!(action, ChatUiAction::ShowDialog(_)) {
                    game.book_view = None;
                }
                handle_chat_ui_action(action, core, connection, game);
            }
        }
        if game.chat.has_pending_modal_prompt() {
            // Do not let the link's opening click accept the new prompt.
            let clicked = false;
            if let Some(action) = game.chat.build_modal_prompt(
                elements,
                sw,
                sh,
                gs,
                cursor,
                clicked,
                &|spans, scale| gfx.renderer.menu_spans_width(spans, scale),
            ) {
                handle_chat_ui_action(action, core, connection, game);
            }
            if core.input.escape_pressed() {
                game.chat.handle_escape();
            }
        } else if done || core.input.escape_pressed() {
            game.book_view = None;
            core.apply_cursor_grab(gfx.window.as_ref(), Some(game));
        }
        core.input.clear_just_pressed_actions();
        return;
    }
    if game.book_edit.is_some() {
        let cursor = core.input.cursor_pos();
        let pressed = core.input.left_just_pressed();
        let action = pressed
            .then(|| crate::ui::book::edit_hit_action(cursor, sw, sh, gs))
            .flatten();
        let width = |s: &str| gfx.renderer.menu_text_width(s, common::FONT_SIZE);
        let mut result = None;
        let mut close = false;
        if let Some(book) = &mut game.book_edit {
            if book.author.is_empty() {
                book.author = game
                    .local_scoreboard_name
                    .clone()
                    .unwrap_or_else(|| core.user.username.clone());
            }
            let was_signing = book.is_signing();
            book.mouse(
                cursor,
                sw,
                sh,
                gs,
                pressed,
                core.input.left_held(),
                core.input.shift_held(),
                &width,
            );
            if was_signing && (action == Some(2) || core.input.escape_pressed()) {
                book.back();
            } else {
                if let Some(index @ 0..=1) = action {
                    book.navigate_measured(index, &width);
                }
                let enter_sign = was_signing
                    && text_events.iter().any(|event| {
                        matches!(
                            event,
                            crate::ui::text_edit::TextInputEvent::Key {
                                code: winit::keyboard::KeyCode::Enter
                                    | winit::keyboard::KeyCode::NumpadEnter,
                                ..
                            }
                        )
                    });
                result = book.input_measured(
                    text_events,
                    !was_signing && action == Some(2),
                    action == Some(3) || enter_sign,
                    &width,
                );
                close = book.should_close() || (!was_signing && core.input.escape_pressed());
            }
            book.draw_measured(elements, sw, sh, gs, cursor, &width);
        }
        if let Some((slot, pages, title)) = result {
            if connection.packet_tx.edit_book(slot, pages.clone(), title) {
                game.preview_book_pages(slot, pages);
            }
        }
        if close {
            game.book_edit = None;
            core.apply_cursor_grab(gfx.window.as_ref(), Some(game));
        }
        core.input.consume_left_just_pressed();
        core.input.clear_just_pressed_actions();
        return;
    }
    if let Some(text) = game.code_of_conduct.clone() {
        let fs = common::FONT_SIZE * gs;
        let x = sw * 0.1;
        let y = sh * 0.1;
        let w = sw * 0.8;
        let h = sh * 0.8;
        elements.push(MenuElement::Rect {
            x,
            y,
            w,
            h,
            corner_radius: 5.0,
            color: [0.04, 0.04, 0.06, 0.97],
        });
        elements.push(MenuElement::Text {
            x: sw / 2.0,
            y: y + 12.0,
            text: crate::lang::ui("Code of Conduct", "行動規範").into(),
            scale: fs * 1.5,
            color: common::WHITE,
            centered: true,
        });
        elements.push(MenuElement::ScissorPush {
            x: x + 12.0,
            y: y + 38.0,
            w: w - 24.0,
            h: h - 88.0,
        });
        let mut lines = Vec::new();
        let mut line = String::new();
        for word in text.split_whitespace() {
            let candidate = if line.is_empty() {
                word.to_owned()
            } else {
                format!("{line} {word}")
            };
            if !line.is_empty() && gfx.renderer.menu_text_width(&candidate, fs) > w - 28.0 {
                lines.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        if !line.is_empty() {
            lines.push(line);
        }
        let visible = ((h - 88.0) / (fs + 3.0)).floor().max(1.0) as usize;
        let scroll = core.input.consume_menu_scroll().round() as isize;
        game.code_of_conduct_scroll = if scroll < 0 {
            game.code_of_conduct_scroll
                .saturating_add((-scroll) as usize)
        } else {
            game.code_of_conduct_scroll.saturating_sub(scroll as usize)
        }
        .min(lines.len().saturating_sub(visible));
        for (i, line) in lines
            .iter()
            .skip(game.code_of_conduct_scroll)
            .take(visible)
            .enumerate()
        {
            elements.push(MenuElement::Text {
                x: x + 14.0,
                y: y + 42.0 + i as f32 * (fs + 3.0),
                text: line.clone(),
                scale: fs,
                color: common::WHITE,
                centered: false,
            });
        }
        elements.push(MenuElement::ScissorPop);
        let buttons = [
            [sw / 2.0 - 108.0, y + h - 38.0, 96.0, 24.0],
            [sw / 2.0 + 12.0, y + h - 38.0, 96.0, 24.0],
        ];
        for (rect, label) in buttons.into_iter().zip([
            crate::lang::ui("Accept", "同意する"),
            crate::lang::ui("Decline", "拒否する"),
        ]) {
            elements.push(MenuElement::Rect {
                x: rect[0],
                y: rect[1],
                w: rect[2],
                h: rect[3],
                corner_radius: 3.0,
                color: [0.2, 0.3, 0.2, 1.0],
            });
            elements.push(MenuElement::Text {
                x: rect[0] + rect[2] / 2.0,
                y: rect[1] + 7.0,
                text: label.into(),
                scale: fs,
                color: common::WHITE,
                centered: true,
            });
        }
        let cursor = core.input.cursor_pos();
        let clicked = core.input.left_just_pressed();
        let hit = |r: [f32; 4]| {
            cursor.0 >= r[0]
                && cursor.0 <= r[0] + r[2]
                && cursor.1 >= r[1]
                && cursor.1 <= r[1] + r[3]
        };
        let decision = if core.input.escape_pressed() || (clicked && hit(buttons[1])) {
            Some(false)
        } else if core.input.enter_pressed() || (clicked && hit(buttons[0])) {
            Some(true)
        } else {
            None
        };
        if let Some(accept) = decision {
            connection.packet_tx.decide_code_of_conduct(accept);
            game.code_of_conduct = None;
            core.input.clear_just_pressed_actions();
        }
        return;
    }
    let modal_open = game.chat.has_pending_modal_prompt();
    if let Some(dialog) = game.server_dialog.as_mut() {
        // The dialog types while it is the top screen; a confirm screen over
        // it takes the keyboard instead.
        if !modal_open {
            let fs = common::FONT_SIZE * gs;
            dialog.handle_text_input(text_events, gs, &|s| gfx.renderer.menu_text_width(s, fs));
        }
        let scroll = core.input.consume_menu_scroll();
        if scroll != 0.0 && !modal_open {
            dialog.handle_scroll(scroll);
        }
        let action = dialog.build(
            elements,
            sw,
            sh,
            gs,
            crate::ui::server_dialog::WidgetInput {
                cursor: core.input.cursor_pos(),
                clicked: core.input.left_just_pressed() && !modal_open,
                held: core.input.left_held() && !modal_open,
                shift: core.input.shift_held(),
                activate: !modal_open
                    && (core.input.enter_pressed()
                        || core.input.key_just_pressed(winit::keyboard::KeyCode::Space)),
                arrow_steps: i32::from(
                    core.input
                        .key_just_pressed(winit::keyboard::KeyCode::ArrowRight),
                ) - i32::from(
                    core.input
                        .key_just_pressed(winit::keyboard::KeyCode::ArrowLeft),
                ),
                tick,
                advanced_tooltips: game.advanced_item_tooltips,
            },
            &|t, s| gfx.renderer.menu_text_width(t, s),
            &|spans, s| gfx.renderer.menu_spans_width(spans, s),
        );
        if dialog.take_click_sound() {
            core.audio.play_ui_click();
        }
        settle_server_dialog(action, core, connection, game);
        core.input.clear_just_pressed_actions();
        core.apply_cursor_grab(&gfx.window, Some(game));
    }

    if game.chat.has_pending_modal_prompt() {
        let cursor = core.input.cursor_pos();
        let clicked = core.input.left_just_pressed();
        // `AbstractButton.onClick` plays the click; this is the same
        // last-frame hit test the modal presses with.
        if clicked && game.chat.hovering_clickable(cursor, false) {
            core.audio.play_ui_click();
        }
        if let Some(action) =
            game.chat
                .build_modal_prompt(elements, sw, sh, gs, cursor, clicked, &|spans, s| {
                    gfx.renderer.menu_spans_width(spans, s)
                })
        {
            handle_chat_ui_action(action, core, connection, game);
        }
        core.input.clear_just_pressed_actions();
        core.apply_cursor_grab(&gfx.window, Some(game));
    }
}

/// A key press while a server dialog is the top screen: Escape cancels it,
/// Tab cycles its text fields, and anything else types. A confirm screen the
/// dialog raised sits above it and answers Escape first.
pub(crate) fn server_dialog_key(
    code: winit::keyboard::KeyCode,
    event: &winit::event::KeyEvent,
    core: &mut AppCore,
    window: &winit::window::Window,
    connection: &ConnectionHandle,
    game: &mut GameState,
) {
    use winit::keyboard::KeyCode;

    if game.chat.has_pending_modal_prompt() {
        // `ConfirmScreen` answers Escape with `accept(false)`, which returns
        // to the screen under it.
        if code == KeyCode::Escape {
            game.chat.handle_escape();
            core.input.clear_action(input::Action::OpenMenu);
            core.apply_cursor_grab(window, Some(game));
        } else {
            core.input.on_menu_key_event(event);
        }
        return;
    }
    match code {
        KeyCode::Escape => {
            let action = game
                .server_dialog
                .as_mut()
                .and_then(|dialog| dialog.handle_escape());
            settle_server_dialog(action, core, connection, game);
            core.input.clear_action(input::Action::OpenMenu);
            core.apply_cursor_grab(window, Some(game));
        }
        KeyCode::Tab => {
            if let Some(dialog) = game.server_dialog.as_mut() {
                dialog.handle_tab(core.input.shift_held());
            }
        }
        _ => core.input.on_menu_key_event(event),
    }
}

/// Carry out the button/dismiss action a benchmark result overlay reported,
/// targeting the matching benchmark's result/upload fields.
fn apply_result_action(
    action: common::ResultAction,
    kind: ResultKind,
    status: Option<UploadStatus>,
    json: String,
    core: &mut AppCore,
    gfx: &Gfx,
    game: &mut GameState,
) {
    match action {
        common::ResultAction::StartUpload => {
            let handle = Some(upload_result(&core.tokio_rt, json));
            match kind {
                ResultKind::Fps => game.benchmark_upload = handle,
                ResultKind::ChunkLoad => game.chunk_load_upload = handle,
            }
        }
        common::ResultAction::Recopy => {
            if let Some(UploadStatus::Done { url, .. }) = status {
                common::set_clipboard(&url);
            }
        }
        common::ResultAction::Dismiss => {
            match kind {
                ResultKind::Fps => {
                    game.benchmark_result = None;
                    game.benchmark_upload = None;
                }
                ResultKind::ChunkLoad => {
                    game.chunk_load_result = None;
                    game.chunk_load_upload = None;
                }
            }
            core.apply_cursor_grab(&gfx.window, Some(game));
        }
        common::ResultAction::None => {}
    }
}

/// Set the active render distance (the persisted menu value) and push it to the
/// server — used by the chunk-load benchmark as it ramps the distance up and
/// down.
fn apply_render_distance(
    core: &mut AppCore,
    game: &mut GameState,
    connection: &ConnectionHandle,
    rd: u32,
) {
    core.menu.render_distance = rd;
    game.sync_client_information(
        connection,
        rd,
        core.menu.chat_options,
        core.menu.main_hand_right(),
        core.menu.particle_status(),
        core.menu.skin_parts_mask(),
    );
}

/// Vanilla HopperScreen: 176x133 texture, five inputs and 36 player slots.
#[allow(clippy::too_many_arguments)]
fn build_hopper(
    elements: &mut Vec<MenuElement>,
    screen_w: f32,
    screen_h: f32,
    cursor: (f32, f32),
    input: &crate::ui::container::ContainerInput,
    slots: &[azalea_inventory::ItemStack],
    title: &str,
    cursor_item: &azalea_inventory::ItemStack,
    drag: &mut Option<crate::ui::container::DragState>,
    last_click: &mut Option<(u16, Instant)>,
    gs: f32,
    advanced_tooltips: bool,
) -> crate::ui::container::ContainerResult {
    use azalea_inventory::ItemStack;

    use crate::renderer::pipelines::menu_overlay::SpriteId;
    use crate::ui::container::{
        ContainerResult, SlotCtx, push_cursor_stack, push_panel, resolve_gesture,
    };
    let panel = push_panel(
        elements,
        screen_w,
        screen_h,
        gs,
        133.0,
        SpriteId::HopperBackground,
    );
    panel.label(elements, 8.0, 6.0, title);
    panel.label(elements, 8.0, 39.0, "Inventory");
    let kind = ContainerKind::Hopper;
    let mut ctx = SlotCtx::new(elements, &panel, cursor, kind, slots, cursor_item, drag);
    for slot in 0..5u16 {
        ctx.slot(
            44.0 + slot as f32 * common::SLOT_STRIDE,
            20.0,
            slots.get(slot as usize).unwrap_or(&ItemStack::Empty),
            None,
            slot,
        );
    }
    ctx.player_rows(slots, 5, 32, 51.0);
    let (hovered, shown_cursor) = ctx.finish(cursor_item);
    push_cursor_stack(elements, cursor, panel.scale, &shown_cursor);
    if cursor_item.is_empty()
        && let Some(item) = hovered.and_then(|slot| slots.get(slot as usize))
        && let Some(item) = item.as_present()
        && let Ok(value) = serde_json::to_value(item)
    {
        let lines = crate::ui::chat::item_tooltip_lines(&value, None, advanced_tooltips);
        if !lines.is_empty() {
            common::push_tooltip_lines(elements, cursor, screen_w, screen_h, panel.scale, lines);
        }
    }
    let (ops, clicked_outside) = resolve_gesture(
        input,
        hovered,
        &panel,
        cursor,
        kind,
        slots,
        cursor_item,
        drag,
        last_click,
    );
    ContainerResult {
        clicked_outside,
        ops,
        button: None,
        recipe_id: None,
    }
}

/// Predict each container click locally (instant UI + drag preview), then send
/// the predicted diff as `HashedStack`es so the server suppresses corrections
/// when the prediction is right (vanilla lockstep).
fn send_container_clicks(
    game: &mut GameState,
    connection: &ConnectionHandle,
    ops: Vec<azalea_inventory::operations::ClickOperation>,
) {
    use azalea_inventory::ItemStack;
    use azalea_inventory::operations::{
        ClickOperation, QuickCraftClick, QuickCraftKind, QuickCraftStatus,
    };
    use azalea_protocol::packets::game::s_container_click::{
        HashedStack, ServerboundContainerClick,
    };

    use crate::player::menu_click;

    let (container_id, kind, state_id) = match &game.open_container {
        Some(c) => (c.id, c.screen.click_kind(), c.state_id),
        None => (0, ContainerKind::Player, game.inventory_state_id),
    };

    let mut drag_kind = QuickCraftKind::Left;
    let mut drag_slots: Vec<u16> = Vec::new();
    for op in &ops {
        let (changed, carried): (Vec<(u16, ItemStack)>, ItemStack) = match op {
            ClickOperation::QuickCraft(QuickCraftClick {
                kind: qc_kind,
                status,
            }) => match status {
                QuickCraftStatus::Start => {
                    drag_kind = qc_kind.clone();
                    drag_slots.clear();
                    (Vec::new(), game.cursor_item.clone())
                }
                QuickCraftStatus::Add { slot } => {
                    drag_slots.push(*slot);
                    (Vec::new(), game.cursor_item.clone())
                }
                QuickCraftStatus::End => {
                    let (changed, remainder) = menu_click::drag_distribution(
                        kind,
                        game.menu_slots(),
                        &game.cursor_item,
                        &drag_kind,
                        &drag_slots,
                    );
                    for (s, item) in &changed {
                        game.set_menu_slot(*s as usize, item.clone());
                    }
                    game.cursor_item = remainder.clone();
                    (changed, remainder)
                }
            },
            other => {
                let mut cursor = std::mem::take(&mut game.cursor_item);
                let changed = menu_click::apply_click(
                    kind,
                    game.menu_slots(),
                    &mut cursor,
                    other,
                    crate::player::is_creative(game.player.game_mode),
                );
                game.cursor_item = cursor;
                for (s, item) in &changed {
                    game.set_menu_slot(*s as usize, item.clone());
                }
                (changed, game.cursor_item.clone())
            }
        };

        let mut click = ServerboundContainerClick {
            container_id,
            state_id,
            slot_num: op.slot_num().map(|s| s as i16).unwrap_or(-999),
            button_num: op.button_num(),
            click_type: op.click_type(),
            changed_slots: Default::default(),
            carried_item: HashedStack::from_item_stack(&carried, &game.registries),
        };
        for (s, item) in &changed {
            click
                .changed_slots
                .insert(*s, HashedStack::from_item_stack(item, &game.registries));
        }
        connection
            .packet_tx
            .send(ServerboundGamePacket::ContainerClick(click));
    }
}

/// Vanilla `Lightmap.getBrightness` at a block position, with
/// `getMaxLocalRawBrightness` = max(skyLight - skyDarken, blockLight).
/// TODO: skyDarken (26.2: 15 - the SKY_LIGHT_LEVEL environment attribute) is
/// untracked; 0 assumed, so the outdoor night-time vignette stays weak.
fn lightmap_brightness(chunks: &ChunkStore, dimension: &str, x: i32, y: i32, z: i32) -> f32 {
    let level = chunks
        .get_sky_light(x, y, z)
        .max(chunks.get_block_light(x, y, z)) as f32;
    // Dimension-type ambient light, matched by id since the dimension-type
    // registry isn't tracked; custom dimensions fall back to 0.
    let ambient = if dimension == "minecraft:the_nether" {
        0.1
    } else {
        0.0
    };
    let v = level / 15.0;
    let curved = v / (4.0 - 3.0 * v);
    // Mth.lerp(ambientLight, curved, 1.0)
    curved + (1.0 - curved) * ambient
}

fn tnt_render_effect(fuse: f32) -> (f32, f32) {
    let swell = if fuse < 10.0 && fuse >= 0.0 {
        (1.0 - fuse / 10.0).clamp(0.0, 1.0).powi(4) * 0.3 + 1.0
    } else {
        1.0
    };
    let white = if fuse >= 0.0 && (fuse / 5.0).floor() as i32 % 2 == 0 {
        1.0
    } else {
        0.0
    };
    (swell, white)
}

fn eye_lightmap_brightness(game: &GameState) -> f32 {
    let eye = game.player.eye_pos();
    lightmap_brightness(
        &game.chunk_store,
        &game.dimension,
        eye.x.floor() as i32,
        eye.y.floor() as i32,
        eye.z.floor() as i32,
    )
}

/// Approximates vanilla's data-driven `equippable.camera_overlay` component
/// check (item components aren't tracked): a carved pumpkin in the head slot.
fn head_is_carved_pumpkin(player: &LocalPlayer) -> bool {
    match player.inventory.slot(crate::player::inventory::ARMOR_START) {
        azalea_inventory::ItemStack::Present(d) => {
            crate::player::inventory::item_resource_name(d.kind) == "carved_pumpkin"
        }
        _ => false,
    }
}

/// Vanilla `LivingEntityRenderer`: hurt or dying entities take the red overlay.
fn has_red_overlay(hurt_time: u8, death_time: u32) -> bool {
    hurt_time > 0 || death_time > 0
}

/// Vanilla `LivingEntityRenderState.deathTime`: the clock plus the partial
/// tick, or zero while alive.
fn render_death_time(death_time: u32, partial_tick: f32) -> f32 {
    if death_time > 0 {
        death_time as f32 + partial_tick
    } else {
        0.0
    }
}

/// Vanilla `Hud.tick`: the held-item tooltip timer resets to 40 when the
/// selected item's type or hover name changes, clears when the slot empties,
/// and otherwise counts down.
fn tick_tool_highlight(core: &AppCore, game: &mut GameState) {
    use azalea_inventory::ItemStack;
    let selected = game
        .player
        .inventory
        .hotbar_slots()
        .get(core.input.selected_slot() as usize)
        .cloned()
        .unwrap_or(ItemStack::Empty);
    match (&selected, &game.last_tool_highlight) {
        (ItemStack::Empty, _) => game.tool_highlight_timer = 0,
        (ItemStack::Present(new), ItemStack::Present(old))
            if new.kind == old.kind
                && crate::ui::common::item_display_name(new)
                    == crate::ui::common::item_display_name(old) =>
        {
            game.tool_highlight_timer = game.tool_highlight_timer.saturating_sub(1);
        }
        _ => game.tool_highlight_timer = 40,
    }
    game.last_tool_highlight = selected;
}

pub fn update_game(
    core: &mut AppCore,
    dt: f32,
    raw_dt: f32,
    gfx: &mut Gfx,
    connection: &ConnectionHandle,
    game: &mut GameState,
    benchmark_fps_limit: Option<u32>,
    window_occluded: bool,
) -> GameUpdateResult {
    if core
        .probe
        .as_ref()
        .is_some_and(|probe| !probe.probe_hud_visible())
    {
        game.hide_gui = true;
    }
    // Snapshot last frame's phase timings before this frame overwrites them: they
    // align with `raw_dt`, which measures the previous frame's full duration.
    let frame_start = std::time::Instant::now();
    let prev_phases = game.last_update_phases;
    game.particle_store.set_mode(core.menu.particle_status());

    // Position the audio listener at the player's head and push current
    // volumes before draining sound packets this frame.
    let listener_pos = game.player.eye_pos();
    core.audio.set_listener(
        listener_pos,
        game.player.look_dir.y_rot_deg(),
        game.player.look_dir.x_rot_deg(),
    );
    core.audio
        .update_entity_sound_position(game.player.entity_id, game.player.position);
    core.audio.set_volumes(core.menu.category_volumes());
    core.audio.set_subtitles_enabled(core.menu.show_subtitles);

    gfx.renderer.set_vsync(core.menu.vsync);
    core.apply_pending_pack_changes(&mut gfx.renderer);
    game.chat.set_options(core.menu.chat_options);

    // Vanilla pauseIfInactive: losing OS focus for more than half a second
    // with no screen open pauses the game, which also releases the cursor
    // (otherwise a system overlay like Win-key search opens over a still
    // captured cursor). TODO: F3+P toggle (options.pauseOnLostFocus).
    if core.probe.is_none()
        && core.auto_fps_result_path.is_none()
        && core
            .unfocused_since
            .is_some_and(|t| t.elapsed().as_millis() > 500)
        && game.input_live()
        && !game.dead
    {
        game.paused = true;
        game.pause_screen = PauseScreen::Main;
        core.apply_cursor_grab(&gfx.window, Some(game));
    }

    let disconnect_reason =
        core.drain_network_events(connection, None, &mut gfx.renderer, &gfx.window, game);
    core.input.set_text_owner(game.text_owner());
    if let Some(transfer) = game.pending_server_transfer.take() {
        game.tab_score_state.set_visible(false);
        return GameUpdateResult::Transfer(transfer);
    }
    if let Some(reason) = disconnect_reason {
        game.tab_score_state.set_visible(false);
        return GameUpdateResult::Disconnected { reason };
    }

    game.chat.tick();
    for mark in game.chat.take_chat_marks() {
        connection.packet_tx.mark_chat(mark);
    }

    game.drain_and_upload_meshes(&mut gfx.renderer);

    game.mesh_dispatcher
        .set_camera_position(*game.player.position);

    // Predict the selected sky clock between SetTime packets. Other clocks
    // advance from authoritative game-time deltas to avoid double prediction.
    // This cadence is separate from the 20 Hz player/input loop below.
    let simulation_ticks = advance_server_time(
        &mut core.time_tick_accumulator,
        dt,
        core.server_tick_rate,
        core.server_tick_frozen,
        &mut core.server_tick_steps,
        &mut game.sky_state,
        &mut game.world_clocks,
    );
    game.item_entity_store.advance_age(simulation_ticks);

    if game.input_live() && game.chunk_load_bench.is_none() {
        gfx.renderer.update_camera(
            &mut core.input,
            dt,
            core.menu.sensitivity,
            core.menu.invert_mouse,
        );
    }

    // Menus never pause the simulation; tick_physics substitutes neutral input.
    game.movement_frame_id = game.movement_frame_id.wrapping_add(1);
    connection
        .packet_tx
        .recorder
        .set_context(game.movement_frame_id, game.tick_count);
    let accumulator_before = core.tick_accumulator;
    core.tick_accumulator += dt;
    let fixed_tick_start = game.benchmark.is_some().then(std::time::Instant::now);
    let mut fixed_tick_count = 0;
    while core.tick_accumulator >= TICK_RATE {
        fixed_tick_count += 1;
        game.tick_count = game.tick_count.wrapping_add(1);
        connection
            .packet_tx
            .recorder
            .set_context(game.movement_frame_id, game.tick_count);
        if game
            .item_activation
            .as_mut()
            .is_some_and(|activation| !activation.tick())
        {
            game.item_activation = None;
        }
        game.item_cooldowns.tick();
        // ClientLevel.tickTime advances once per eligible fixed world tick;
        // never use the server tick-rate/interpolation cadence for this clock.
        if (!core.server_tick_frozen || simulation_ticks > 0) && !(game.singleplayer && game.paused)
        {
            advance_world_clock_tick(&mut game.world_clock_game_time, &mut game.world_clocks);
        }
        // ClientLevel ticks the border on each fixed client/world tick, even
        // when the world clock is frozen or running at a modified rate.
        game.world_border.tick();
        // Vanilla `Minecraft.tick` order: `gameMode.tick` drives the connection
        // tick (and so the level load tracker) before the level's entities,
        // i.e. before the local player moves or sends anything.
        AppCore::tick_level_load(&gfx.renderer, connection, game);
        // Vanilla Gui.tick falls back from dead health alone when no screen is
        // open, so death UI/auto-respawn must not depend on PlayerCombatKill.
        let has_screen = game.win_credits.is_some()
            || game.death_screen_open
            || game.paused
            || game.options_from_game
            || game.gui_open()
            || game.chat.is_open();
        if game.dead && !has_screen {
            match crate::app::core::death_route(game.show_death_screen) {
                crate::app::core::DeathRoute::ShowDeathScreen => {
                    core.open_death_screen(connection, &gfx.window, game, None);
                }
                crate::app::core::DeathRoute::Respawn => core.send_respawn(connection, game),
            }
        }
        let local_player_was_removed = game.dead && game.player.death_animation_finished();
        core.tick_physics(&mut gfx.renderer, connection, &gfx.window, game);
        // Cloak motion is a level tick (20 Hz), not physics/interpolation cadence.
        tick_cloaks(&mut game.player, &mut game.entity_store);
        // `LocalPlayer.tick` returns before `super.tick()` until the client has
        // loaded, so the player's own baseTick state waits with it.
        if game.client_loaded && !local_player_was_removed {
            // LivingEntity.baseTick hurt/effects and Player.tick sleep state still
            // run on the tick-20 removal tick, then stop with future entity ticks.
            game.player.tick_hurt();
            game.player.effects.tick();
            game.player.tick_sleep();
        }
        game.item_entity_store.tick(&game.chunk_store);
        game.entity_store
            .tick_projectile_displays(&game.chunk_store);
        let book_players: Vec<_> = std::iter::once(*game.player.position)
            .chain(
                game.entity_store
                    .living
                    .values()
                    .filter_map(|entity| entity.player_uuid.map(|_| *entity.position)),
            )
            .collect();
        for (pos, entity) in &mut game.chunk_store.block_entities {
            entity.tick_bell_swing();
            if let Some(book) = entity.book.as_mut() {
                let center =
                    glam::DVec3::new(pos.x as f64 + 0.5, pos.y as f64 + 0.5, pos.z as f64 + 0.5);
                let nearest = book_players
                    .iter()
                    .copied()
                    .filter(|player| player.distance_squared(center) <= 9.0)
                    .min_by(|a, b| {
                        a.distance_squared(center)
                            .total_cmp(&b.distance_squared(center))
                    });
                book.tick(*pos, nearest);
            }
        }
        // Client CampfireBlockEntity::particleTick: 11% chance per fixed tick,
        // then 2-3 native cosy/signal particles (server packet path stays separate).
        let mut campfire_rng = fastrand::Rng::new();
        let mut campfire_food_rng = fastrand::Rng::new();
        let mut campfire_food_smoke = Vec::new();
        let campfire_smoke: Vec<_> = game
            .chunk_store
            .block_entities
            .iter()
            .filter_map(|(pos, entity)| {
                let state = game.chunk_store.get_block_state(pos.x, pos.y, pos.z);
                let id = crate::world::block::block_id(state);
                if !matches!(id, "campfire" | "soul_campfire") {
                    return None;
                }
                let props = crate::world::block::block_properties(state);
                let lit = props.get("lit") == Some("true");
                let waterlogged = props.get("waterlogged") == Some("true");
                if !crate::particle::campfire_smoke_enabled(lit, waterlogged) {
                    return None;
                }
                let signal = props.get("signal_fire") == Some("true");
                let facing = props.get("facing").unwrap_or("north");
                for (slot, occupied) in entity.campfire_slots.iter().copied().enumerate() {
                    if occupied
                        && crate::particle::campfire_food_smoke_enabled(campfire_food_rng.f32())
                    {
                        campfire_food_smoke
                            .push(crate::particle::campfire_slot_smoke_pos(*pos, facing, slot));
                    }
                }
                let chance = campfire_rng.f32();
                if chance >= 0.11 {
                    return None;
                }
                let count = crate::particle::campfire_smoke_count(chance, campfire_rng.u32(0..2))?;
                Some((*pos, signal, count))
            })
            .collect();
        let particle_camera = gfx.renderer.camera_render_position();
        for pos in campfire_food_smoke {
            for _ in 0..4 {
                game.particle_store
                    .add_campfire_food_smoke(pos, particle_camera);
            }
        }
        for (pos, signal, count) in campfire_smoke {
            for _ in 0..count {
                let signed_offset =
                    || fastrand::f64() / 3.0 * if fastrand::bool() { 1.0 } else { -1.0 };
                let spawn = glam::dvec3(
                    pos.x as f64 + 0.5 + signed_offset(),
                    pos.y as f64 + fastrand::f64() + fastrand::f64(),
                    pos.z as f64 + 0.5 + signed_offset(),
                );
                game.particle_store.add_campfire_smoke(spawn, signal);
            }
        }
        let chunks = &game.chunk_store;
        let player = &game.player;
        let entities = &game.entity_store;
        let items = &game.item_entity_store;
        game.particle_store.tick_with_entity_lookup(chunks, |id| {
            GameState::tracking_attachment_for(player, entities, items, id)
        });
        game.block_entity_anim.tick();
        game.title.tick();
        tick_tool_highlight(core, game);
        // LocalPlayer.handlePortalTransitionEffect belongs to aiStep, which is
        // skipped once tickDeath removes the player at exactly death tick 20.
        let local_player_ai_step_ran = !game.dead || !game.player.death_animation_finished();
        let inside_portal = game.player.is_inside_nether_portal(&game.chunk_store);
        if local_player_ai_step_ran && game.player.tick_portal_effect(inside_portal) {
            // Vanilla forLocalAmbience: AMBIENT category at the listener,
            // volume 0.25, pitch 0.8..1.2.
            core.audio.play_world_sound(
                &SoundRef::event("block.portal.trigger"),
                CATEGORY_AMBIENT,
                game.player.position,
                0.25,
                fastrand::f32() * 0.4 + 0.8,
                fastrand::u64(..),
            );
        }
        // Vanilla Hud.updateVignetteBrightness: 1%-per-tick smoothing toward
        // the darkness of the eye block's light level.
        let target = (1.0 - eye_lightmap_brightness(game)).clamp(0.0, 1.0);
        game.vignette_brightness += (target - game.vignette_brightness) * 0.01;
        // Vanilla `Hud.tickAutosaveIndicator`.
        game.last_saving_indicator_value = game.saving_indicator_value;
        let target = if gfx.renderer.screenshot_saving() {
            1.0
        } else {
            0.0
        };
        game.saving_indicator_value = game.saving_indicator_value.lerp(target, 0.2);
        if let Some(c) = &mut game.open_container
            && let Some(state) = &mut c.enchant
        {
            state.tick(&c.slots, &c.data);
            // Vanilla `EnchantmentScreen.containerTick` keeps the XP bar
            // prioritized while the screen is open.
            game.xp_display_start_tick = game.tick_count as i64;
        }
        connection.packet_tx.recorder.record("local", "fixed_tick", || {
            Some(serde_json::json!({"frame_id":game.movement_frame_id,"tick_in_frame":fixed_tick_count,"player":crate::movement_record::player(game),"client_loaded":game.client_loaded,"dead":game.dead}))
        });
        AppCore::send_client_tick_end(connection);
        core.tick_accumulator -= TICK_RATE;
    }
    connection.packet_tx.recorder.record("local", "movement_frame", || {
        Some(serde_json::json!({"frame_id":game.movement_frame_id,"raw_dt":raw_dt,"simulation_dt":dt,"accumulator_before":accumulator_before,"accumulator_after":core.tick_accumulator,"ticks":fixed_tick_count}))
    });
    game.last_update_phases.fixed_tick_count = fixed_tick_count;
    game.last_update_phases.fixed_tick_ms = fixed_tick_start
        .map(|start| start.elapsed().as_secs_f32() * 1000.0)
        .unwrap_or_default();

    // Once per frame after the frame's ticks, where vanilla `Minecraft.runTick`
    // calls `level.update()`.
    let light_update_start = game.benchmark.is_some().then(std::time::Instant::now);
    game.update_light(&mut gfx.renderer, core.menu.chunk_detail);
    game.last_update_phases.light_update_ms = light_update_start
        .map(|start| start.elapsed().as_secs_f32() * 1000.0)
        .unwrap_or_default();
    let pre_ui_start = game.benchmark.is_some().then(std::time::Instant::now);

    // F1 (vanilla keyToggleGui); only while no screen or chat is open.
    if core.input.key_just_pressed(winit::keyboard::KeyCode::F1) && game.input_live() {
        game.hide_gui = !game.hide_gui;
    }
    // Vanilla leaves bed via InBedChatScreen's ESC / "Leave bed" button; no
    // bed screen yet, so the jump key wakes. TODO: InBedChatScreen.
    if game.input_live()
        && game.player.is_sleeping()
        && core.input.action_just_pressed(input::Action::Jump)
    {
        core.send_stop_sleeping(connection, game.player.entity_id);
    }
    // TODO: remaining vanilla keybinds with no backing feature yet:
    // L advancements, P social interactions, O friends overlay (in-game),
    // G quick actions, F4 spectator shader effects, C/X creative saved
    // hotbars, spectator hotbar select.

    // Finished F2 captures announce in chat (vanilla screenshot.success: bare
    // filename, underlined).
    // TODO: vanilla makes the filename a clickable open-file link; pomme chat
    // has no click handling yet.
    use crate::ui::text::TextSpan;
    for result in gfx.renderer.take_screenshot_messages() {
        let spans = match result {
            Ok(name) => {
                let mut file = TextSpan::new(name, common::WHITE);
                file.underline = true;
                vec![
                    TextSpan::new("Saved screenshot as ".into(), common::WHITE),
                    file,
                ]
            }
            Err(err) => vec![TextSpan::new(
                format!("Couldn't save screenshot: {err}"),
                common::WHITE,
            )],
        };
        game.chat.push_message(spans);
    }

    // F3+A: drop every mesh and re-enqueue all loaded columns.
    if game.pending_chunk_reload {
        game.pending_chunk_reload = false;
        game.meshed.clear();
        gfx.renderer.clear_chunk_meshes();
        game.vis_valid = false;
        game.pending_load_rescan = true;
    }

    let partial_tick = core.tick_accumulator / TICK_RATE;

    let enter = core.input.enter_pressed();
    let tab = core.input.tab_pressed();
    let shift = core.input.shift_held();
    let up = core.input.up_pressed();
    let down = core.input.down_pressed();
    let page_up = core.input.page_up_pressed();
    let page_down = core.input.page_down_pressed();
    // The ordered key/char stream goes to whichever text consumer owns this
    // frame; menus (in-game options) drain it themselves in build_menu_input.
    let text_events = if game.chat.is_open() || game.wants_text_input() {
        core.input.drain_text_events()
    } else {
        Vec::new()
    };
    let text_sw = gfx.renderer.screen_width() as f32;
    let text_gs = hud::gui_scale(
        text_sw,
        gfx.renderer.screen_height() as f32,
        core.menu.gui_scale_setting,
    );
    let text_fs = common::FONT_SIZE * text_gs;
    if game.text_owner() == input::TextOwner::Other && game.wants_text_input() {
        game.recipe_book.handle_text_events(&text_events, &|t| {
            gfx.renderer.menu_text_width(t, common::FONT_SIZE)
        });
    }
    let chat_was_open = game.chat.is_open();
    if game.text_owner() != input::TextOwner::Chat {
        // The top screen replaces ChatScreen; server screens consume their
        // text below, not through the still-open chat field.
    } else if let Some(msg) = game.chat.handle_key_input(
        &text_events,
        enter,
        tab,
        shift,
        up,
        down,
        page_up,
        page_down,
        text_sw - 4.0 * text_gs,
        &|s| gfx.renderer.menu_text_width(s, text_fs),
        game.command_tree.as_deref(),
    ) {
        core.send_chat_message(connection, msg);
    }
    // Enter closes chat even when there was nothing to send.
    if chat_was_open && !game.chat.is_open() {
        core.apply_cursor_grab(&gfx.window, Some(game));
    }
    if game.server_dialog.is_none() && game.chat.is_open() {
        let scroll = core.input.consume_menu_scroll();
        if scroll != 0.0 {
            game.chat
                .handle_scroll(core.input.cursor_pos(), scroll, shift);
        }
    }
    if let Some((id, command)) = game.chat.take_suggestion_request() {
        connection
            .packet_tx
            .send(ServerboundGamePacket::CommandSuggestion(
                ServerboundCommandSuggestion { id, command },
            ));
    }

    // Chat counts as text capture too, so digits/E/Q/F type instead of acting
    // as game keys (vanilla suppresses KeyMappings while any screen is open).
    core.input.text_capture = game.wants_text_input() || game.chat.is_open();
    core.input.menu_capture = game.gui_open() || game.death_screen_open;
    core.input.spectator = crate::player::is_spectator(game.player.game_mode);
    let dynamic_atlas_start = game.benchmark.is_some().then(std::time::Instant::now);
    let atlas_stats = core.sync_game_dynamic_atlas(
        game,
        &mut gfx.renderer,
        core.input.spectator && game.spectator.is_menu_active(),
        dynamic_atlas_start.is_some(),
    );
    game.last_update_phases.dynamic_atlas_ms = dynamic_atlas_start
        .map(|start| start.elapsed().as_secs_f32() * 1000.0)
        .unwrap_or_default();
    game.last_update_phases.dynamic_atlas_added_keys = atlas_stats.added_keys;
    game.last_update_phases.dynamic_atlas_removed_keys = atlas_stats.removed_keys;
    game.last_update_phases.dynamic_atlas_dirty = atlas_stats.dirty;
    game.last_update_phases.dynamic_atlas_face_dirty = atlas_stats.face_dirty;
    game.last_update_phases.dynamic_atlas_spectator_changed = atlas_stats.spectator_changed;
    game.last_update_phases.dynamic_atlas_renderer_ms = atlas_stats.renderer_ms;
    game.last_update_phases.dynamic_atlas_pack_cpu_ms = atlas_stats.pack_cpu_ms;
    game.last_update_phases.dynamic_atlas_retire_wait_ms = atlas_stats.retire_wait_ms;
    game.last_update_phases.dynamic_atlas_upload_submit_wait_ms = atlas_stats.upload_submit_wait_ms;
    game.last_update_phases.dynamic_atlas_descriptor_ms = atlas_stats.descriptor_ms;

    // The F3+F4 switcher shows the mouse cursor while open.
    let switcher_open = game.game_mode_switcher.is_some();
    if switcher_open != game.switcher_was_open {
        game.switcher_was_open = switcher_open;
        core.apply_cursor_grab(&gfx.window, Some(game));
    }

    let mut close_inventory = false;
    let mut pause_action = PauseAction::None;
    let mut death_action = DeathAction::None;

    gfx.renderer.sync_camera_pos(
        game.player
            .prev_eye_pos()
            .lerp(game.player.eye_pos(), partial_tick as f64),
    );
    // Per-frame FOV interpolation; set before the frustum/view-projection reads.
    gfx.renderer.set_render_partial_tick(partial_tick);
    gfx.renderer.set_death_time(if game.dead {
        game.player.death_time as f32 + partial_tick
    } else {
        0.0
    });
    gfx.renderer.set_hurt(
        game.player.hurt_time,
        game.player.hurt_dir,
        core.menu.damage_tilt_strength,
    );
    // Plain lerp (vanilla getInterpolatedWalkDistance); the forward-extrapolating
    // camera variant judders across tick boundaries when per-tick speed varies.
    let bob_walk = game
        .player
        .prev_walk_dist
        .lerp(game.player.walk_dist, partial_tick);
    let bob_amount = game.player.prev_bob.lerp(game.player.bob, partial_tick);
    gfx.renderer
        .set_view_bob(bob_walk, bob_amount, core.menu.view_bobbing);
    gfx.renderer.update_third_person_distance(
        game.player
            .prev_eye_pos()
            .lerp(game.player.eye_pos(), partial_tick as f64),
        &game.chunk_store,
    );
    // Esc cancels a running benchmark: restore the render distance it changed.
    if std::mem::take(&mut game.chunk_load_abort)
        && let Some(bench) = game.chunk_load_bench.take()
    {
        apply_render_distance(core, game, connection, bench.original_rd());
    }
    // Watch the chunk-load benchmark from straight above, framed to its load
    // radius.
    match &game.chunk_load_bench {
        Some(bench) => {
            let radius = bench.effective_rd().max(1) as f32 * 16.0;
            gfx.renderer.set_top_down_radius(radius);
        }
        None => gfx.renderer.clear_top_down(),
    }

    let sw = gfx.renderer.screen_width() as f32;
    let sh = gfx.renderer.screen_height() as f32;
    let gs = hud::gui_scale(sw, sh, core.menu.gui_scale_setting);
    game.last_update_phases.pre_ui_other_ms = pre_ui_start
        .map(|start| start.elapsed().as_secs_f32() * 1000.0)
        .unwrap_or_default();
    let ui_extract_start = game.benchmark.is_some().then(std::time::Instant::now);

    let mut elements: Vec<MenuElement> = Vec::new();

    let debug = if game.show_debug {
        Some(hud::DebugInfo {
            fps: gfx.fps_counter.display_fps(),
            position: *game.player.position,
            y_rot_deg: gfx.renderer.camera_look_dir().y_rot_deg(),
            x_rot_deg: gfx.renderer.camera_look_dir().x_rot_deg(),
            target_block: game.interaction.target.and_then(|t| {
                let HitResult::Block(t) = t else {
                    return None;
                };
                let state =
                    game.chunk_store
                        .get_block_state(t.block_pos.x, t.block_pos.y, t.block_pos.z);
                let props = crate::world::block::block_properties(state)
                    .entries()
                    .map(|(k, v)| format!("{k}: {v}"))
                    .collect();
                Some((
                    t.block_pos,
                    t.face,
                    crate::world::block::block_id(state).to_string(),
                    props,
                ))
            }),
            chunk_count: gfx.renderer.loaded_chunk_count(),
            sections_drawn: gfx.renderer.sections_drawn(),
            occlusion_on: game.chunk_occlusion_enabled,
            mesh_gate: game.vis_valid.then(|| {
                // Among in-frustum columns: sections we mesh vs sections skipped as
                // occluded (the per-section occlusion win). Middle slot unused.
                let n = game.chunk_store.section_count() as u32;
                let mut visible = 0u32;
                let mut hidden = 0u32;
                for (pos, &mask) in &game.vis_mask {
                    if game.vis_tiers.get(pos).copied().unwrap_or(0) == 0 {
                        let v = mask.count_ones();
                        visible += v;
                        hidden += n.saturating_sub(v);
                    }
                }
                (visible, 0, hidden)
            }),
            gpu_name: gfx.renderer.gpu_name(),
            vulkan_version: gfx.renderer.vulkan_version(),
            screen_w: gfx.renderer.screen_width(),
            screen_h: gfx.renderer.screen_height(),
            timings: Some(hud::FrameTimings {
                frame_ms: gfx.renderer.last_timings().frame_ms,
                fence_ms: gfx.renderer.last_timings().fence_ms,
                acquire_ms: gfx.renderer.last_timings().acquire_ms,
                cull_ms: gfx.renderer.last_timings().cull_ms,
                draw_ms: gfx.renderer.last_timings().draw_ms,
                present_ms: gfx.renderer.last_timings().present_ms,
            }),
        })
    } else {
        None
    };
    // The chunk-load benchmark renders a clean top-down view: only terrain, no HUD,
    // entities/player, held item, clouds, or weather — and skipping them also keeps
    // the measured frame times honest.
    let benchmark_running = game.chunk_load_bench.is_some();
    // Underwater overlay (vanilla ScreenEffectRenderer.submitWater): part of
    // the 3D pass in vanilla, so it shows even with the GUI hidden, but not
    // while sleeping.
    if !benchmark_running
        && gfx.renderer.is_first_person()
        && !crate::player::is_spectator(game.player.game_mode)
        && !game.player.is_sleeping()
        && game.player.eyes_in_water
    {
        hud::build_underwater_overlay(
            &mut elements,
            sw,
            sh,
            eye_lightmap_brightness(game),
            game.player.look_dir.y_rot_deg(),
            game.player.look_dir.x_rot_deg(),
        );
    }
    if !benchmark_running && game.hide_gui {
        // F1: vanilla still renders the debug overlay with the GUI hidden.
        if let Some(info) = debug.as_ref() {
            hud::build_debug_overlay(&mut elements, info, gs, &|t, s| {
                gfx.renderer.menu_text_width(t, s)
            });
        }
    } else if !benchmark_running {
        // Vanilla Hud.extractCameraOverlays: vignette, pumpkin, and portal
        // draw under everything else in the HUD.
        let portal_intensity = game
            .player
            .prev_portal_effect_intensity
            .lerp(game.player.portal_effect_intensity, partial_tick);
        hud::build_camera_overlays(
            &mut elements,
            sw,
            sh,
            core.menu.vignette.then_some(game.vignette_brightness),
            gfx.renderer.is_first_person() && head_is_carved_pumpkin(&game.player),
            gfx.renderer.is_first_person() && game.interaction.is_using_spyglass(),
            portal_intensity,
        );
        let is_survival = crate::player::is_survival(game.player.game_mode);
        let air_bubbles = hud::air_bubbles(game.player.air_supply, game.player.eyes_in_water)
            .filter(|_| is_survival);
        // The pop sound only plays while the bubbles render (HUD visible).
        if let Some(bubbles) = &air_bubbles {
            if !game.player.eyes_in_water {
                game.last_bubble_pop_sound_played = 0;
            } else if bubbles.is_popping && game.last_bubble_pop_sound_played != bubbles.popping_pos
            {
                let volume = 0.5 + 0.1 * (bubbles.empty - 3 + 1).max(0) as f32;
                let pitch = 1.0 + 0.1 * (bubbles.empty - 5 + 1).max(0) as f32;
                core.audio.play_world_sound(
                    &SoundRef::event("ui.hud.bubble_pop"),
                    CATEGORY_PLAYERS,
                    game.player.position,
                    volume,
                    pitch,
                    fastrand::u64(..),
                );
                game.last_bubble_pop_sound_played = bubbles.popping_pos;
            }
        }
        // Contextual bar choice (vanilla Hud.nextContextualInfoState): the
        // jump bar takes the slot while controlling a saddled mount, the
        // locator bar while waypoints are tracked; an active jump charge or
        // an XP change within 100 ticks outprioritizes the locator.
        enum BarChoice {
            Jump,
            Xp,
            Locator,
            Empty,
        }
        let can_jump_bar = game.riding_jumpable_vehicle();
        let jump_charge = game.player.jump_riding_scale;
        let xp_prioritized =
            is_survival && game.xp_display_start_tick + 100 > game.tick_count as i64;
        let bar_choice = if game.waypoints.has_waypoints() {
            if can_jump_bar && jump_charge > 0.0 {
                BarChoice::Jump
            } else if xp_prioritized {
                BarChoice::Xp
            } else {
                BarChoice::Locator
            }
        } else if can_jump_bar {
            BarChoice::Jump
        } else if is_survival {
            BarChoice::Xp
        } else {
            BarChoice::Empty
        };
        let show_locator = matches!(bar_choice, BarChoice::Locator);
        let locator_dots = if show_locator {
            let (yaw_deg, pitch_deg) = gfx.renderer.camera_effective_look_deg();
            let cam = crate::world::waypoints::WaypointCamera {
                position: gfx.renderer.camera_render_position(),
                yaw_deg,
                pitch_deg,
                view_rot_proj: gfx.renderer.locator_projection(),
                fov_y_deg: gfx.renderer.camera_fov_degrees(),
            };
            let store = &game.entity_store;
            let entity_eye_pos = |uuid: &uuid::Uuid| {
                store.player_by_uuid(uuid).map(|e| {
                    let feet = e.prev_position.lerp(e.position, partial_tick as f64);
                    // TODO: swimming/gliding eye height needs entity pose data.
                    let eye_height = if e.is_crouching {
                        crate::player::CROUCH_EYE_HEIGHT
                    } else {
                        crate::player::STANDING_EYE_HEIGHT
                    };
                    let block_pos = glam::IVec3::new(
                        e.position.x.floor() as i32,
                        e.position.y.floor() as i32,
                        e.position.z.floor() as i32,
                    );
                    (
                        block_pos,
                        *feet + glam::DVec3::new(0.0, f64::from(eye_height), 0.0),
                    )
                })
            };
            game.waypoints.extract_dots(
                &cam,
                *game.player.position,
                core.user.uuid,
                &entity_eye_pos,
            )
        } else {
            Vec::new()
        };
        let bar = match bar_choice {
            BarChoice::Jump => hud::ContextualBarKind::JumpableVehicle {
                charge: jump_charge,
            },
            BarChoice::Xp => hud::ContextualBarKind::Experience,
            BarChoice::Locator => hud::ContextualBarKind::Locator {
                dots: &locator_dots,
                arrow_frame_1: game.tick_count % 14 >= 10,
            },
            BarChoice::Empty => hud::ContextualBarKind::Empty,
        };
        // Vanilla `renderMaxAttackIndicator`: the picked entity is living
        // (implicit: pomme entity hits only come from `living`), alive, at
        // full charge, and the weapon is slow enough to matter (delay > 5).
        // TODO: vanilla also skips it when the active item's ATTACK_RANGE
        // component says the hit is out of range (spears).
        let held = game.player.inventory.held_stack(core.input.selected_slot());
        let delay = crate::player::interaction::attack_strength_delay(held);
        let scale = game.interaction.attack_strength_scale(delay);
        let show_full = scale >= 1.0
            && delay > 5.0
            && matches!(game.interaction.target, Some(HitResult::Entity(hit))
                if game
                    .entity_store
                    .living
                    .get(&hit.entity_id)
                    .is_some_and(|e| e.health > 0.0));
        let attack = hud::AttackIndicatorState {
            mode: core.menu.attack_indicator,
            scale,
            show_full,
            main_hand_right: core.menu.main_hand_right(),
        };
        if crate::player::is_spectator(game.player.game_mode) {
            crate::ui::spectator_menu::build_spectator_menu(
                &mut elements,
                &mut game.spectator,
                &game.tab_list,
                sw,
                sh,
                gs,
                &|t, s| gfx.renderer.menu_text_width(t, s),
            );
        }
        hud::build_hud(
            &mut elements,
            sw,
            sh,
            core.input.selected_slot(),
            game.player.health,
            game.player.absorption,
            game.player.max_health,
            game.player.food,
            game.player.armor,
            air_bubbles,
            game.player.eyes_in_water,
            game.vehicle_health(),
            game.tick_count,
            game.player.experience_level,
            game.player.experience_progress,
            bar,
            game.player.game_mode,
            game.player.inventory.hotbar_slots(),
            &game.item_cooldowns,
            partial_tick,
            game.tool_highlight_timer,
            game.action_bar
                .as_ref()
                .map(|(spans, tick)| (spans.as_slice(), game.tick_count.wrapping_sub(*tick))),
            &|spans, s| gfx.renderer.menu_spans_width(spans, s),
            &game.scoreboard,
            game.local_scoreboard_name.as_deref(),
            &game.player.effects,
            &game.boss_bars,
            gfx.renderer.is_first_person(),
            debug.as_ref(),
            core.menu.gui_scale_setting,
            &attack,
            &|t, s| gfx.renderer.menu_text_width(t, s),
        );
        if let Some(held) = game.player.inventory.held_stack(core.input.selected_slot()) {
            if let Some(map_id) = held.get_component::<azalea_inventory::components::MapId>() {
                if map_id.id >= 0 {
                    if let Some(map) = game.maps.0.get(&(map_id.id as u32)) {
                        let size = 128.0;
                        let x = sw - size - 12.0;
                        let y = 12.0;
                        elements.push(MenuElement::Rect {
                            x: x - 3.0,
                            y: y - 3.0,
                            w: size + 6.0,
                            h: size + 6.0,
                            corner_radius: 0.0,
                            color: [0.12, 0.09, 0.06, 1.0],
                        });
                        for (i, color) in map.colors.iter().copied().enumerate() {
                            if color != 0 {
                                elements.push(MenuElement::Rect {
                                    x: x + (i % 128) as f32,
                                    y: y + (i / 128) as f32,
                                    w: 1.0,
                                    h: 1.0,
                                    corner_radius: 0.0,
                                    color: crate::world::maps::palette(color),
                                });
                            }
                        }
                        for marker in &map.decorations {
                            let rotation = f32::from(marker.rotation).rem_euclid(16.0)
                                * std::f32::consts::TAU
                                / 16.0;
                            let (sin, cos) = rotation.sin_cos();
                            let mx = x + f32::from(marker.x) / 2.0 + 64.0;
                            let my = y + f32::from(marker.y) / 2.0 + 64.0;
                            // Vanilla MapRenderer scales its [-1, 1] quad by four,
                            // after translating it by (-0.125, 0.125) in local space.
                            let sprite_cx = mx - 0.5 * cos - 0.5 * sin;
                            let sprite_cy = my - 0.5 * sin + 0.5 * cos;
                            elements.push(MenuElement::RotatedImage {
                                cx: sprite_cx,
                                cy: sprite_cy,
                                w: 8.0,
                                h: 8.0,
                                radians: rotation,
                                sprite: crate::renderer::pipelines::menu_overlay::SpriteId::MapDecoration(
                                    marker.asset,
                                ),
                                tint: [1.0; 4],
                            });
                            if let Some(name) = &marker.name {
                                let width = gfx
                                    .renderer
                                    .menu_text_width(name, crate::ui::common::FONT_SIZE);
                                let text_scale = if width > 0.0 {
                                    crate::ui::common::FONT_SIZE * (25.0 / width).min(6.0 / 9.0)
                                } else {
                                    crate::ui::common::FONT_SIZE * (6.0 / 9.0)
                                };
                                elements.push(MenuElement::Text {
                                    x: mx,
                                    y: my + 4.0,
                                    text: name.clone(),
                                    scale: text_scale,
                                    color: [1.0; 4],
                                    centered: true,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    let effective_rd = if game.server_render_distance > 0 {
        core.menu.render_distance.min(game.server_render_distance)
    } else {
        core.menu.render_distance
    };
    let entity_view_scale =
        crate::renderer::entity_view_scale(effective_rd, core.menu.entity_distance_percent);

    let mut map_quads = Vec::new();
    for frame in game.entity_store.vehicles.values().filter(|entity| {
        matches!(
            entity.kind,
            Some(azalea_registry::builtin::EntityKind::ItemFrame)
                | Some(azalea_registry::builtin::EntityKind::GlowItemFrame)
        )
    }) {
        if !crate::renderer::entity_distance_visible(
            *frame.position,
            gfx.renderer.camera_render_position(),
            1024.0,
            entity_view_scale,
        ) {
            continue;
        }
        let azalea_inventory::ItemStack::Present(stack) = &frame.item_frame_item else {
            continue;
        };
        let Some(map_id) = stack.get_component::<azalea_inventory::components::MapId>() else {
            continue;
        };
        if map_id.id < 0 {
            continue;
        }
        let map_id = map_id.id as u32;
        let Some(map) = game.maps.0.get(&map_id) else {
            continue;
        };
        let direction = frame
            .item_frame_direction
            .unwrap_or(azalea_core::direction::Direction::South);
        let base_matrix =
            item_frame_base_matrix(*frame.position, direction, gfx.renderer.camera_anchor());
        map_quads.push(MapQuadDraw {
            map_id,
            map_data: map.clone(),
            position: base_matrix.w_axis.truncate(),
            rotation: crate::renderer::pipelines::map_quad::frame_map_rotation(
                glam::Quat::from_mat4(&base_matrix),
                frame.item_frame_rotation,
            ),
        });
    }

    // Vanilla Hud.extractSleepOverlay sits outside the isHidden gate: above
    // the hotbar/effects/boss bar, below chat and the tab list.
    // TODO: vanilla draws the scoreboard sidebar, action bar, and nameplates
    // above the fade; here they dim under it (build_hud bundles the first two).
    if !benchmark_running {
        hud::build_sleep_overlay(&mut elements, sw, sh, game.player.sleep_counter);
    }

    let tab_list_visible = tab_list_overlay_visible(
        core.input.performing_action(input::Action::ViewPlayerList),
        game.hide_gui,
        game.paused,
        game.gui_open(),
        game.options_from_game,
        game.chat.is_open(),
        game.dead,
        game.death_screen_open,
    );
    game.tab_score_state.set_visible(tab_list_visible);
    if tab_list_visible {
        let r = &gfx.renderer;
        crate::ui::player_tab::build_player_tab_overlay(
            &mut elements,
            sw,
            &game.tab_list,
            &game.scoreboard,
            gs,
            &|t, s| r.menu_text_width(t, s),
            &|spans, s| r.menu_spans_width(spans, s),
            &mut game.tab_score_state,
            game.tick_count,
        );
    }

    if !benchmark_running {
        let renderer = &gfx.renderer;
        if !game.hide_gui {
            crate::ui::player_tab::build_player_nameplates(
                &mut elements,
                crate::ui::player_tab::PlayerNameplates {
                    entity_store: &game.entity_store,
                    tab_list: &game.tab_list,
                    scoreboard: &game.scoreboard,
                    local_uuid: core.user.uuid,
                    partial_tick,
                    screen_height: renderer.screen_height(),
                    fov_degrees: renderer.camera_fov_degrees(),
                    camera_pos: renderer.camera_render_position(),
                    entity_view_scale,
                    project: &|position| renderer.project_world_to_screen_with_depth(position),
                },
            );
        }
    }

    if let Some(switcher) = &mut game.game_mode_switcher {
        crate::ui::game_mode_switcher::build_game_mode_switcher(
            &mut elements,
            switcher,
            sw,
            sh,
            core.input.cursor_pos(),
            gs,
        );
    }

    if let Some(ref mut bench) = game.benchmark {
        let entity_count = game.entity_store.living.len() as u32;
        let done = bench.record_frame(
            raw_dt * 1000.0,
            gfx.renderer.last_timings(),
            prev_phases.cpu_update_ms,
            prev_phases.render_wall_ms,
            prev_phases,
            benchmark_fps_limit,
            window_occluded,
            core.menu.vsync,
            gfx.renderer.loaded_chunk_count(),
            entity_count,
        );
        let progress = bench.progress();
        elements.push(MenuElement::Rect {
            x: sw * 0.25,
            y: 16.0,
            w: sw * 0.5,
            h: 8.0,
            corner_radius: 4.0,
            color: [1.0, 1.0, 1.0, 0.1],
        });
        elements.push(MenuElement::Rect {
            x: sw * 0.25,
            y: 16.0,
            w: sw * 0.5 * progress,
            h: 8.0,
            corner_radius: 4.0,
            color: [0.294, 0.871, 0.498, 0.8],
        });
        elements.push(MenuElement::Text {
            x: sw / 2.0,
            y: 28.0,
            text: format!(
                "{} {:.0}%",
                crate::lang::ui("Benchmarking...", "計測中..."),
                progress * 100.0
            ),
            scale: 8.0 * gs,
            color: [1.0, 1.0, 1.0, 1.0],
            centered: true,
        });
        if done {
            let bench = game.benchmark.take().unwrap();
            let (result, saved) = if let Some(path) = &core.auto_fps_result_path {
                bench.finish_with_save_at(path)
            } else {
                bench.finish_with_save(&core.data_dirs.game_dir)
            };
            game.benchmark_saved = saved.is_ok();
            if let Err(error) = saved {
                tracing::error!("Failed to save benchmark: {error}");
            }
            game.benchmark_result = Some(result);
            game.benchmark_upload = None;
            core.apply_cursor_grab(&gfx.window, Some(game));
        }
    }

    if let Some(ref result) = game.benchmark_result {
        let mut lines = vec![
            format!("GPU: {}", result.gpu),
            format!(
                "{}x{} / {} {} / {} {} / {} {}",
                result.resolution[0],
                result.resolution[1],
                crate::lang::ui("RD", "描画距離"),
                result.render_distance,
                result.peak_chunk_count,
                crate::lang::ui("chunks", "チャンク"),
                result.peak_entity_count,
                crate::lang::ui("entities", "エンティティ"),
            ),
            format!(
                "{}: {:.0}",
                crate::lang::ui("Avg FPS", "平均 FPS"),
                result.avg_fps
            ),
            format!(
                "{}: {:.0} / {}: {:.0}",
                crate::lang::ui("Min", "最小"),
                result.min_fps,
                crate::lang::ui("Max", "最大"),
                result.max_fps
            ),
            format!(
                "{}: {:.2}ms / P1: {:.2}ms / P99: {:.2}ms",
                crate::lang::ui("Frame", "フレーム"),
                result.avg_frame_ms,
                result.p1_frame_ms,
                result.p99_frame_ms
            ),
            format!(
                "{}: {:.2}ms / {}: {:.2}ms / {}: {:.2}ms",
                crate::lang::ui("Fence", "フェンス"),
                result.avg_fence_ms,
                crate::lang::ui("Cull", "カリング"),
                result.avg_cull_ms,
                crate::lang::ui("Draw", "描画"),
                result.avg_draw_ms
            ),
            format!(
                "{} {} (>{:.0}ms) - {} {}",
                result.spike_count,
                crate::lang::ui("spikes", "スパイク"),
                8.0,
                crate::lang::ui("Saved to", "保存先:"),
                core.auto_fps_result_path
                    .as_deref()
                    .unwrap_or(&core.data_dirs.game_dir.join("benchmark.json"))
                    .display()
            ),
        ];
        if crate::benchmark::is_debug_build() {
            lines.push(
                crate::lang::ui(
                    "Debug build - frame times are not representative",
                    "デバッグビルド - フレーム時間は参考値です",
                )
                .to_string(),
            );
        }
        let json = serde_json::to_string_pretty(result).unwrap_or_default();
        let status = game
            .benchmark_upload
            .as_ref()
            .map(|h| h.lock().unwrap().clone());
        let action = common::push_results_overlay(
            &mut elements,
            sw,
            sh,
            gs,
            sh / 2.0 - 90.0,
            crate::lang::ui("Benchmark Complete", "ベンチマーク完了"),
            &lines,
            status.as_ref(),
            core.input.cursor_pos(),
            core.input.left_just_pressed(),
            core.input.escape_pressed(),
        );
        apply_result_action(action, ResultKind::Fps, status, json, core, gfx, game);
    }

    if let Some(mut bench) = game.chunk_load_bench.take() {
        let count = gfx.renderer.loaded_chunk_count();
        match bench.update(
            count,
            raw_dt * 1000.0,
            gfx.renderer.last_timings(),
            prev_phases,
        ) {
            ChunkLoadStep::Wait => {
                game.chunk_load_bench = Some(bench);
            }
            ChunkLoadStep::Load(rd) => {
                apply_render_distance(core, game, connection, rd);
                game.chunk_load_bench = Some(bench);
            }
            ChunkLoadStep::Done(result) => {
                apply_render_distance(core, game, connection, bench.original_rd());
                tracing::info!(
                    "Chunk load RD {} (effective {}): {} chunks in {:.2}s ({:.0} chunks/s), \
                     first chunk {:.2}s, frame avg {:.1}ms / worst {:.1}ms",
                    result.target_rd,
                    result.effective_rd,
                    result.chunk_count,
                    result.load_secs,
                    result.chunks_per_sec,
                    result.time_to_first_secs,
                    result.avg_frame_ms,
                    result.worst_frame_ms,
                );
                result.save(&core.data_dirs.game_dir);
                game.chunk_load_result = Some(*result);
                game.chunk_load_upload = None;
                core.apply_cursor_grab(&gfx.window, Some(game));
            }
        }
    }

    if let Some(ref bench) = game.chunk_load_bench {
        let progress = format!(
            "{} {}/{}",
            crate::lang::ui("run", "実行"),
            bench.current_run(),
            bench.total_runs()
        );
        let label = if bench.resetting() {
            format!(
                "{} ({progress})",
                crate::lang::ui("Resetting world...", "ワールドをリセット中...")
            )
        } else {
            format!(
                "{} {}... {} {} ({progress})",
                crate::lang::ui("Loading RD", "描画距離を読み込み中"),
                bench.target_rd(),
                bench.loaded(),
                crate::lang::ui("chunks", "チャンク")
            )
        };
        elements.push(MenuElement::Text {
            x: sw / 2.0,
            y: 28.0,
            text: label,
            scale: 8.0 * gs,
            color: [1.0, 1.0, 1.0, 1.0],
            centered: true,
        });
    }

    if let Some(ref result) = game.chunk_load_result {
        let rd_line = if result.effective_rd != result.target_rd {
            format!(
                "{}: {} ({} {})",
                crate::lang::ui("Render Distance", "描画距離"),
                result.target_rd,
                crate::lang::ui("server-capped to", "サーバーの上限"),
                result.effective_rd
            )
        } else if result.achieved_rd < result.target_rd {
            format!(
                "{}: {} ({} ~{})",
                crate::lang::ui("Render Distance", "描画距離"),
                result.target_rd,
                crate::lang::ui("server loaded", "サーバーの読み込み距離"),
                result.achieved_rd
            )
        } else {
            format!(
                "{}: {}",
                crate::lang::ui("Render Distance", "描画距離"),
                result.target_rd
            )
        };
        let mut lines = vec![
            rd_line,
            format!(
                "{} {} {} {:.2}s ({} {} {})",
                crate::lang::ui("Loaded", "読み込み完了:"),
                result.chunk_count,
                crate::lang::ui("chunks in", "チャンク /"),
                result.load_secs,
                crate::lang::ui("avg of", "平均"),
                result.runs,
                crate::lang::ui("runs", "回")
            ),
            format!(
                "{:.0} {} - {} {:.2}s",
                result.chunks_per_sec,
                crate::lang::ui("chunks/sec", "チャンク/秒"),
                crate::lang::ui("first chunk in", "最初のチャンクまで"),
                result.time_to_first_secs
            ),
            format!(
                "{}: {} {:.1}ms / {} {:.1}ms",
                crate::lang::ui("Frame while loading", "読み込み中のフレーム"),
                crate::lang::ui("avg", "平均"),
                result.avg_frame_ms,
                crate::lang::ui("worst", "最悪"),
                result.worst_frame_ms
            ),
            format!("GPU: {} / Vulkan {}", result.gpu, result.vulkan),
            format!(
                "{} {} / {} threads / v{} / {}x{}",
                result.os,
                result.arch,
                result.cpu_threads,
                result.version,
                result.resolution[0],
                result.resolution[1],
            ),
            format!(
                "{} {}",
                crate::lang::ui("Saved to", "保存先:"),
                core.data_dirs.game_dir.join("chunk_load.json").display()
            ),
        ];
        if crate::benchmark::is_debug_build() {
            lines.push(
                crate::lang::ui(
                    "Debug build - frame times are not representative",
                    "デバッグビルド - フレーム時間は参考値です",
                )
                .to_string(),
            );
        }
        let json = serde_json::to_string_pretty(result).unwrap_or_default();
        let status = game
            .chunk_load_upload
            .as_ref()
            .map(|h| h.lock().unwrap().clone());
        let action = common::push_results_overlay(
            &mut elements,
            sw,
            sh,
            gs,
            sh / 2.0 - 100.0,
            crate::lang::ui("Chunk Load Complete", "チャンク読み込み完了"),
            &lines,
            status.as_ref(),
            core.input.cursor_pos(),
            core.input.left_just_pressed(),
            core.input.escape_pressed(),
        );
        apply_result_action(action, ResultKind::ChunkLoad, status, json, core, gfx, game);
    }

    // A dialog is the top screen: the screens under it keep their state
    // (vanilla's `previousScreen`) but neither draw nor take input. The Hud
    // still draws, so chat keeps its unfocused backlog.
    let dialog_open = game.dialog_open();
    let mut win_credits_complete = false;
    let server_modal_blocks_credits = game.server_modal_blocks_credits();
    if let Some(state) = game.win_credits.as_mut()
        && credits_may_advance(server_modal_blocks_credits)
    {
        let menu_input = core.build_menu_input(dt);
        let r = &gfx.renderer;
        let (result, complete) =
            core.menu
                .build_win_credits_roll(state, sw, sh, &menu_input, &|t, s| {
                    r.menu_text_width(t, s)
                });
        elements.extend(result.elements);
        core.input.clear_just_pressed_actions();
        win_credits_complete = complete;
    } else if game.options_from_game && !dialog_open {
        core.menu.server_render_distance = game.server_render_distance;
        let mut menu_input = core.build_menu_input(dt);
        // Chat consumed the enter/tab latches earlier this frame; hand them on.
        menu_input.enter = enter;
        menu_input.tab = tab;
        let r = &gfx.renderer;
        let result = core
            .menu
            .build(sw, sh, &menu_input, |t, s| r.menu_text_width(t, s));
        elements.extend(result.elements);
        core.input.clear_just_pressed_actions();
        core.sync_display_mode(&gfx.window);
    } else if game.death_screen_open && !dialog_open {
        let cursor = core.input.cursor_pos();
        let clicked = core.input.left_just_pressed() && !game.respawn_sent;
        death_action = if game.death_confirm {
            death::build_death_confirm(
                &mut elements,
                sw,
                sh,
                cursor,
                clicked,
                gs,
                death::buttons_ready(game.death_confirm_ticks),
            )
        } else {
            let buttons_enabled =
                !game.respawn_sent && death::buttons_ready(game.death_screen_ticks);
            let r = &gfx.renderer;
            death::build_death_screen(
                &mut elements,
                sw,
                sh,
                cursor,
                clicked,
                gs,
                &game.death_message,
                game.player.score,
                game.hardcore,
                buttons_enabled,
                &|t, s| r.menu_text_width(t, s),
            )
        };
        core.input.clear_just_pressed_actions();
    } else if game.paused && !matches!(game.pause_screen, PauseScreen::Hidden) && !dialog_open {
        let cursor = core.input.cursor_pos();
        let clicked = core.input.left_just_pressed();
        pause_action = pause::build_pause_menu(
            &mut elements,
            sw,
            sh,
            cursor,
            clicked,
            gs,
            game.pause_screen,
            game.server_render_distance,
            game.singleplayer,
            connection.packet_tx.recorder.active(),
        );
        if game.pause_screen == PauseScreen::Benchmark {
            core.menu
                .build_movement_recording_status(&mut elements, sw, sh, &|t, s| {
                    gfx.renderer.menu_text_width(t, s)
                });
        }
        core.input.clear_just_pressed_actions();
    }

    if finish_win_credits_if_allowed(
        &mut game.win_credits,
        win_credits_complete,
        server_modal_blocks_credits,
    ) {
        // Taking the state makes this callback edge-triggered; send_respawn is
        // independently guarded by GameState::respawn_sent.
        core.send_respawn(connection, game);
    }

    let mut player_preview = None;
    let mut book_preview = None;
    if (game.inventory_open || game.open_container.is_some()) && !dialog_open {
        // Key shortcuts stay quiet while a text field (anvil rename) types.
        let keys_live = !game.wants_text_input();
        let input = crate::ui::container::ContainerInput {
            left_pressed: core.input.left_just_pressed(),
            right_pressed: core.input.right_just_pressed(),
            middle_pressed: core.input.middle_just_pressed(),
            left_held: core.input.left_held(),
            right_held: core.input.right_held(),
            shift: core.input.shift_held(),
            hotbar_swap: keys_live
                .then(|| core.input.hotbar_key_just_pressed())
                .flatten(),
            swap_offhand: keys_live && core.input.key_just_pressed(winit::keyboard::KeyCode::KeyF),
            throw: keys_live && core.input.key_just_pressed(winit::keyboard::KeyCode::KeyQ),
            throw_all: core.input.ctrl_held(),
        };
        // The anvil rename field consumes this frame's typing; a changed
        // accepted name goes to the server (vanilla `onNameChanged`).
        if let Some(c) = &mut game.open_container
            && let Some(state) = &mut c.anvil
            && let Some(name) =
                crate::ui::anvil::update_rename(state, &c.slots, &text_events, &|s| {
                    gfx.renderer.menu_text_width(s, common::FONT_SIZE)
                })
        {
            use azalea_protocol::packets::game::s_rename_item::ServerboundRenameItem;
            connection
                .packet_tx
                .send(ServerboundGamePacket::RenameItem(ServerboundRenameItem {
                    name,
                }));
        }
        let mut select_trade = None;
        let mut beacon_effect_selection = None;
        let place_recipe;
        let native_recipes =
            crate::version::session_protocol() == pomme_protocol::version::NATIVE.protocol;
        let (clicked_outside, ops) = if let Some(container) = &mut game.open_container {
            let result = match container.screen {
                ContainerScreen::Merchant => {
                    let model = container
                        .merchant
                        .as_mut()
                        .expect("merchant screen has model");
                    let scroll = core.input.consume_menu_scroll();
                    model.scroll_by(scroll.round() as i32);
                    let result = crate::ui::merchant::build_merchant(
                        &mut elements,
                        sw,
                        sh,
                        core.input.cursor_pos(),
                        &input,
                        model,
                        &container.slots,
                        &container.title,
                        &game.cursor_item,
                        &mut game.inv_drag,
                        &mut game.inv_last_click,
                        gs,
                    );
                    select_trade = result.select_trade;
                    result.container
                }
                ContainerScreen::Horse { columns, .. } => crate::ui::horse::build_horse(
                    &mut elements,
                    sw,
                    sh,
                    core.input.cursor_pos(),
                    &input,
                    crate::ui::horse::HorseLayout { columns },
                    &container.slots,
                    &container.title,
                    &game.cursor_item,
                    &mut game.inv_drag,
                    &mut game.inv_last_click,
                    gs,
                ),
                ContainerScreen::CraftingTable => crate::ui::crafting_table::build_crafting_table(
                    &mut elements,
                    sw,
                    sh,
                    core.input.cursor_pos(),
                    &input,
                    &container.slots,
                    &container.title,
                    &game.cursor_item,
                    &mut game.inv_drag,
                    &mut game.inv_last_click,
                    gs,
                    &mut game.recipe_book,
                    native_recipes,
                    &|t, s| gfx.renderer.menu_text_width(t, s),
                    game.advanced_item_tooltips,
                ),
                ContainerScreen::Furnace(variant) => crate::ui::furnace::build_furnace(
                    &mut elements,
                    sw,
                    sh,
                    core.input.cursor_pos(),
                    &input,
                    variant,
                    &container.slots,
                    &container.data,
                    &container.title,
                    &game.cursor_item,
                    &mut game.inv_drag,
                    &mut game.inv_last_click,
                    gs,
                    &|t, s| gfx.renderer.menu_text_width(t, s),
                    &mut game.recipe_book,
                    native_recipes,
                    game.advanced_item_tooltips,
                ),
                ContainerScreen::Chest { rows } => crate::ui::chest::build_chest(
                    &mut elements,
                    sw,
                    sh,
                    core.input.cursor_pos(),
                    &input,
                    rows,
                    &container.slots,
                    &container.title,
                    &game.cursor_item,
                    &mut game.inv_drag,
                    &mut game.inv_last_click,
                    gs,
                    game.advanced_item_tooltips,
                ),
                ContainerScreen::Hopper => build_hopper(
                    &mut elements,
                    sw,
                    sh,
                    core.input.cursor_pos(),
                    &input,
                    &container.slots,
                    &container.title,
                    &game.cursor_item,
                    &mut game.inv_drag,
                    &mut game.inv_last_click,
                    gs,
                    game.advanced_item_tooltips,
                ),
                ContainerScreen::ShulkerBox => crate::ui::chest::build_shulker_box(
                    &mut elements,
                    sw,
                    sh,
                    core.input.cursor_pos(),
                    &input,
                    &container.slots,
                    &container.title,
                    &game.cursor_item,
                    &mut game.inv_drag,
                    &mut game.inv_last_click,
                    gs,
                    game.advanced_item_tooltips,
                ),
                ContainerScreen::Anvil => crate::ui::anvil::build_anvil(
                    &mut elements,
                    sw,
                    sh,
                    core.input.cursor_pos(),
                    &input,
                    &container.slots,
                    &container.data,
                    &container.title,
                    container.anvil.as_ref().expect("anvil screen has state"),
                    game.player.experience_level,
                    crate::player::is_creative(game.player.game_mode),
                    &game.cursor_item,
                    &mut game.inv_drag,
                    &mut game.inv_last_click,
                    gs,
                    &|t, s| gfx.renderer.menu_text_width(t, s),
                ),
                ContainerScreen::Beacon => {
                    let result = crate::ui::beacon::build_beacon(
                        &mut elements,
                        sw,
                        sh,
                        core.input.cursor_pos(),
                        &input,
                        &container.slots,
                        &container.data,
                        &container.data_received,
                        &container.title,
                        &game.cursor_item,
                        &mut game.inv_drag,
                        &mut game.inv_last_click,
                        gs,
                    );
                    beacon_effect_selection = result.effects;
                    result.container
                }
                ContainerScreen::Special(
                    crate::ui::special_container::SpecialMenu::Stonecutter,
                ) => crate::ui::stonecutter::build(
                    &mut elements,
                    sw,
                    sh,
                    core.input.cursor_pos(),
                    &input,
                    &container.slots,
                    &container.data,
                    &container.data_received,
                    game.recipe_book.updates.as_ref(),
                    &game.recipe_book.item_tags,
                    &mut container.stonecutter_scroll,
                    &container.title,
                    &game.cursor_item,
                    &mut game.inv_drag,
                    &mut game.inv_last_click,
                    gs,
                    game.advanced_item_tooltips,
                ),
                ContainerScreen::Special(crate::ui::special_container::SpecialMenu::Loom) => {
                    crate::ui::loom::build(
                        &mut elements,
                        sw,
                        sh,
                        core.input.cursor_pos(),
                        &input,
                        &container.slots,
                        &container.data,
                        &container.data_received,
                        &game.loom_patterns,
                        &mut container.loom_scroll,
                        &container.title,
                        &game.cursor_item,
                        &mut game.inv_drag,
                        &mut game.inv_last_click,
                        gs,
                        game.advanced_item_tooltips,
                    )
                }
                ContainerScreen::Special(menu) => crate::ui::special_container::build(
                    &mut elements,
                    sw,
                    sh,
                    core.input.cursor_pos(),
                    &input,
                    menu,
                    &container.slots,
                    &container.data,
                    &container.title,
                    &game.cursor_item,
                    &mut game.inv_drag,
                    &mut game.inv_last_click,
                    gs,
                    game.advanced_item_tooltips,
                ),
                ContainerScreen::Enchantment => {
                    let result = crate::ui::enchantment::build_enchantment(
                        &mut elements,
                        sw,
                        sh,
                        core.input.cursor_pos(),
                        &input,
                        &container.slots,
                        &container.data,
                        &container.title,
                        container
                            .enchant
                            .as_ref()
                            .expect("enchantment screen has state"),
                        partial_tick,
                        &game.registries,
                        game.player.experience_level,
                        crate::player::is_creative(game.player.game_mode),
                        &game.cursor_item,
                        &mut game.inv_drag,
                        &mut game.inv_last_click,
                        gs,
                        &|t, s| gfx.renderer.menu_text_width(t, s),
                        &|t, s| gfx.renderer.menu_text_width_sga(t, s),
                    );
                    book_preview = Some(result.book);
                    result.container
                }
            };
            place_recipe = result
                .recipe_id
                .map(|(display_id, use_max_items)| (container.id, display_id, use_max_items));
            if let Some(button_id) = result.button {
                use azalea_protocol::packets::game::s_container_button_click::ServerboundContainerButtonClick;
                connection
                    .packet_tx
                    .send(ServerboundGamePacket::ContainerButtonClick(
                        ServerboundContainerButtonClick {
                            container_id: container.id,
                            button_id,
                        },
                    ));
            }
            (result.clicked_outside, result.ops)
        } else {
            let result = crate::ui::inventory::build_inventory(
                &mut elements,
                sw,
                sh,
                core.input.cursor_pos(),
                &input,
                &game.player.inventory,
                &game.cursor_item,
                &mut game.inv_drag,
                &mut game.inv_last_click,
                gs,
                &mut game.recipe_book,
                native_recipes,
                game.advanced_item_tooltips,
                &|t, s| gfx.renderer.menu_text_width(t, s),
            );
            place_recipe = result
                .recipe_id
                .map(|(display_id, use_max_items)| (0, display_id, use_max_items));
            player_preview = result.player_preview;
            (result.clicked_outside, result.ops)
        };
        close_inventory = clicked_outside;
        if game.recipe_book.settings_dirty {
            let book_type = match game.open_container.as_ref().map(|c| &c.screen) {
                Some(ContainerScreen::Furnace(crate::ui::furnace::FurnaceVariant::Furnace)) => 1,
                Some(ContainerScreen::Furnace(
                    crate::ui::furnace::FurnaceVariant::BlastFurnace,
                )) => 2,
                Some(ContainerScreen::Furnace(crate::ui::furnace::FurnaceVariant::Smoker)) => 3,
                _ => 0,
            };
            game.recipe_book.store_settings(book_type);
            connection.packet_tx.recipe_book_settings(
                book_type,
                game.recipe_book.open,
                game.recipe_book.craftable_only,
            );
            game.recipe_book.settings_dirty = false;
        }
        if let Some(index) = select_trade {
            connection.packet_tx.select_trade(index);
        }
        if let Some((primary, secondary)) = beacon_effect_selection {
            connection.packet_tx.set_beacon(primary, secondary);
        }
        if let Some((container_id, display_id, use_max_items)) = place_recipe {
            connection
                .packet_tx
                .place_recipe(container_id, display_id, use_max_items);
        }
        send_container_clicks(game, connection, ops);
        core.input.clear_just_pressed_actions();
    }

    if game.creative_inventory_open && !dialog_open {
        let cursor = core.input.cursor_pos();
        let clicked = core.input.left_just_pressed();
        let middle_clicked = core.input.middle_just_pressed();
        let right_clicked = core.input.right_just_pressed();
        let scroll_delta = core.input.consume_menu_scroll();
        // `typed`/`backspace` come from the frame's single drain up top; a
        // second drain here would always read empty.
        let creative_has_focus = game.text_owner() == input::TextOwner::CreativeSearch;
        let creative_preedit = core.input.chat_preedit().filter(|_| creative_has_focus);
        let action = crate::ui::creative_inventory::build_creative_inventory(
            &mut elements,
            &mut game.creative_state,
            sw,
            sh,
            cursor,
            clicked,
            middle_clicked,
            right_clicked,
            scroll_delta,
            if creative_has_focus {
                &text_events
            } else {
                &[]
            },
            creative_preedit,
            core.input.key_just_pressed(winit::keyboard::KeyCode::KeyT),
            core.input.hotbar_key_just_pressed(),
            core.input.key_just_pressed(winit::keyboard::KeyCode::KeyF),
            &game.player.inventory,
            gs,
            game.advanced_item_tooltips,
            core.input.left_held(),
            core.input.right_held(),
            &|t, s| gfx.renderer.menu_text_width(t, s),
        );
        use azalea_protocol::packets::game::s_set_creative_mode_slot::ServerboundSetCreativeModeSlot;
        let mut set_creative_slot = |slot_num: u16, item: azalea_inventory::ItemStack| {
            if crate::player::is_creative(game.player.game_mode) {
                connection
                    .packet_tx
                    .send(ServerboundGamePacket::SetCreativeModeSlot(
                        ServerboundSetCreativeModeSlot {
                            slot_num,
                            item_stack: item.clone(),
                        },
                    ));
                // Optimistic local update; the server echoes via ContainerSetSlot.
                game.player.inventory.set_slot(slot_num as usize, item);
            }
        };
        match action {
            crate::ui::creative_inventory::CreativeAction::Close => {
                close_inventory = true;
            }
            crate::ui::creative_inventory::CreativeAction::SetSlot(slot_num, item) => {
                set_creative_slot(slot_num, item);
            }
            crate::ui::creative_inventory::CreativeAction::SetSlots(items) => {
                for (slot_num, item) in items {
                    set_creative_slot(slot_num, item);
                }
            }
            crate::ui::creative_inventory::CreativeAction::None => {}
        }
        if game.text_owner() == input::TextOwner::CreativeSearch {
            let (x, y, fs) = crate::ui::creative_inventory::search_ime_cursor_area(
                &game.creative_state.search,
                sw,
                sh,
                gs,
                core.input.chat_preedit(),
                &|t, s| gfx.renderer.menu_text_width(t, s),
            );
            gfx.window.set_ime_cursor_area(
                winit::dpi::PhysicalPosition::new(x as i32, y as i32),
                winit::dpi::PhysicalSize::new(1, fs.max(1.0) as u32),
            );
        }
        core.input.clear_just_pressed_actions();
    }

    // Before chat so chat draws over it (vanilla extract order).
    if !benchmark_running && !game.hide_gui {
        game.title.build(&mut elements, sw, sh, gs, partial_tick);
    }

    // F1 hides the closed-chat overlay; an open chat is a screen and renders
    // regardless (vanilla Hud.extractChat vs ChatScreen).
    if !game.hide_gui || game.chat.is_focused() {
        let command_tree = game.command_tree.clone();
        if game.chat.is_focused() && !dialog_open {
            let fs = common::FONT_SIZE * gs;
            let x = 4.0 * gs
                + game
                    .chat
                    .ime_caret_x(sw - 4.0 * gs, &|s| gfx.renderer.menu_text_width(s, fs));
            gfx.window.set_ime_cursor_area(
                winit::dpi::PhysicalPosition::new(x as i32, (sh - 14.0 * gs) as i32),
                winit::dpi::PhysicalSize::new(1, fs.max(1.0) as u32),
            );
        }
        let chat_action = game.chat.build(
            &mut elements,
            crate::ui::chat::ChatBuildContext {
                screen_w: sw,
                screen_h: sh,
                gui_scale: gs,
                cursor: core.input.cursor_pos(),
                preedit: core.input.chat_preedit(),
                covered: dialog_open,
                clicked: core.input.left_just_pressed(),
                shift: core.input.shift_held(),
                command_tree: command_tree.as_deref(),
                advanced_item_tooltips: game.advanced_item_tooltips,
                text_width_fn: &|t, s| gfx.renderer.menu_text_width(t, s),
                spans_width_fn: &|spans, s| gfx.renderer.menu_spans_width(spans, s),
            },
        );
        if let Some(action) = chat_action {
            handle_chat_ui_action(action, core, connection, game);
        }
    }

    // Subtitles draw above chat and the tab list; toasts stay on top
    // (vanilla extract order). The queue is empty while the option is off.
    let subtitle_now = std::time::Instant::now();
    for ev in core.audio.take_subtitle_events() {
        game.subtitles
            .on_play_sound(&ev.key, *ev.pos, ev.range, subtitle_now);
    }
    if core.menu.show_subtitles && !benchmark_running && !game.hide_gui {
        let (yaw_deg, pitch_deg) = gfx.renderer.camera_effective_look_deg();
        game.subtitles.build(
            &mut elements,
            sw,
            sh,
            gs,
            gfx.renderer.camera_render_position(),
            yaw_deg,
            pitch_deg,
            subtitle_now,
            &|t, s| gfx.renderer.menu_text_width(t, s),
        );
    }

    // Vanilla Gui.update() runs the toast manager every frame regardless of
    // screens or F1; only rendering is gated (ToastManager.extractRenderState).
    for event in game.toasts.update() {
        core.audio.play_ui_sound(event, 1.0, 1.0);
    }
    if !benchmark_running && !game.hide_gui {
        game.toasts.build(&mut elements, sw, gs, &|spans, s| {
            gfx.renderer.menu_spans_width(spans, s)
        });
    }

    build_server_screens(
        &mut elements,
        sw,
        sh,
        gs,
        core,
        gfx,
        connection,
        game,
        Some(game.tick_count),
        &text_events,
    );

    core.input.set_text_owner(game.text_owner());
    if game.chat.is_open() && !dialog_open && core.input.cursor_moved_this_frame() {
        let icon = if game
            .chat
            .hovering_clickable(core.input.cursor_pos(), core.input.shift_held())
        {
            winit::window::CursorIcon::Pointer
        } else {
            winit::window::CursorIcon::Default
        };
        gfx.window.set_cursor(icon);
    }

    // Chat consumes keys, not clicks; nothing else clears them while only chat
    // is open, so drop them here to keep stray clicks out of the live sim.
    if game.chat.is_open() {
        core.input.clear_just_pressed_actions();
    }

    let hand_animation = game
        .interaction
        .hand_animation(partial_tick, gfx.renderer.camera_look_dir());
    let use_anim = game.interaction.use_animation(partial_tick);
    let destroy_info = game.interaction.destroy_stage().map(|(pos, stage)| {
        let state = game.chunk_store.get_block_state(pos.x, pos.y, pos.z);
        (pos, stage, state)
    });

    game.last_update_phases.ui_extract_ms = ui_extract_start
        .map(|start| start.elapsed().as_secs_f32() * 1000.0)
        .unwrap_or_default();

    let probe_peer_filter = core
        .probe
        .as_ref()
        .and_then(|probe| probe.peer_filter())
        .cloned();
    let scene_extract_start = game.benchmark.is_some().then(std::time::Instant::now);
    let mut entity_renders: Vec<EntityRenderInfo> = if benchmark_running {
        Vec::new()
    } else {
        game.entity_store
            .living
            .iter()
            .filter_map(|(&entity_id, e)| {
                let entity_name = e.player_uuid.and_then(|uuid| {
                    game.tab_list
                        .players
                        .get(&uuid)
                        .map(|player| player.name.as_str())
                });
                if crate::app::probe::should_exclude_peer(
                    probe_peer_filter.as_ref(),
                    entity_id,
                    game.player.entity_id,
                    e.player_uuid,
                    entity_name,
                ) {
                    return None;
                }
                let interp_pos = e.prev_position.lerp(e.position, partial_tick as f64);
                let sleep_orientation = e.sleeping_pos.and_then(|bed| {
                    let state = game.chunk_store.get_block_state(bed.x, bed.y, bed.z);
                    match crate::world::block::block_properties(state).get("facing")? {
                        "south" => Some((90.0, 0.0, 1.0)),
                        "west" => Some((0.0, -1.0, 0.0)),
                        "north" => Some((270.0, 0.0, -1.0)),
                        "east" => Some((180.0, 1.0, 0.0)),
                        _ => None,
                    }
                });
                let render_pos = sleep_orientation.map_or(interp_pos, |(_, step_x, step_z)| {
                    let dimensions =
                        azalea_entity::dimensions::EntityDimensions::from(e.entity_type);
                    let eye_height = dimensions.eye_height * if e.is_baby { 0.5 } else { 1.0 };
                    let head_offset = f64::from(eye_height - 0.1);
                    Position::new(
                        interp_pos.x - step_x * head_offset,
                        interp_pos.y,
                        interp_pos.z - step_z * head_offset,
                    )
                });
                let extras =
                    entity_extras(entity_id, e, partial_tick, game.sky_state.game_time as i64);
                let body_yaw = lerp_angle(e.prev_body_y_rot_deg, e.body_y_rot_deg, partial_tick);
                let (walk_dist, bob) = if entity_id == game.player.entity_id {
                    (
                        game.player
                            .prev_walk_dist
                            .lerp(game.player.walk_dist, partial_tick),
                        game.player.prev_bob.lerp(game.player.bob, partial_tick),
                    )
                } else if e.entity_type == EntityKind::Player {
                    (
                        e.cape_motion
                            .prev_walk_dist
                            .lerp(e.cape_motion.walk_dist, partial_tick),
                        e.cape_motion.prev_bob.lerp(e.cape_motion.bob, partial_tick),
                    )
                } else {
                    (e.walk_pos(partial_tick), e.walk_speed(partial_tick))
                };
                let fall_flying_ticks = if entity_id == game.player.entity_id {
                    if game.player.fall_flying {
                        game.player.fall_flying_ticks as f32
                    } else {
                        0.0
                    }
                } else {
                    e.cape_motion.fall_flying_ticks as f32
                };
                let cape_pose = native_cape_pose(
                    e.cloak.render_lerp(f64::from(partial_tick)),
                    *interp_pos,
                    body_yaw,
                    walk_dist,
                    bob,
                    fall_flying_ticks,
                    partial_tick,
                );

                Some(EntityRenderInfo {
                    happy_ghast_equipment_layers: Vec::new(),
                    position: render_pos + extras.render_offset,
                    simulation_position: e.position,
                    head_y_rot_deg: lerp_angle(
                        e.prev_head_y_rot_deg,
                        e.head_y_rot_deg,
                        partial_tick,
                    ),
                    head_x_rot_deg: e
                        .prev_look_dir
                        .x_rot_deg()
                        .lerp(e.look_dir.x_rot_deg(), partial_tick),
                    body_y_rot_deg: body_yaw,
                    cape_pose,
                    is_baby: e.is_baby,
                    is_crouching: e.is_crouching,
                    shulker_peek: e.prev_shulker_peek_amount
                        + (e.shulker_peek_amount - e.prev_shulker_peek_amount) * partial_tick,
                    shulker_attach_face: e.shulker_attach_face,
                    is_sleeping: e.sleeping_pos.is_some(),
                    sleeping_yaw_deg: sleep_orientation.map(|(angle, _, _)| angle),
                    walk_anim_pos: e.walk_pos(partial_tick),
                    walk_anim_speed: e.walk_speed(partial_tick),
                    entity_kind: e.entity_type,
                    player_uuid: e.player_uuid,
                    is_invisible: e.flags.invisible,
                    skin_parts_mask: match e.entity_type {
                        EntityKind::Player if entity_id == game.player.entity_id => {
                            game.local_skin_parts_mask
                        }
                        EntityKind::Player | EntityKind::Mannequin => {
                            e.skin_parts_mask.unwrap_or(0x7f)
                        }
                        _ => 0x7f,
                    },
                    variant_index: extras.variant_index,
                    armor_stand_flags: 0,
                    armor_stand_pose: [[0.0; 3]; 6],
                    armor_stand_equipment: e.equipment.clone(),
                    body_equipment: e
                        .equipment
                        .get(&azalea_inventory::components::EquipmentSlot::Body)
                        .cloned()
                        .unwrap_or(azalea_inventory::ItemStack::Empty),
                    chest_equipment: e
                        .equipment
                        .get(&azalea_inventory::components::EquipmentSlot::Chest)
                        .cloned()
                        .unwrap_or(azalea_inventory::ItemStack::Empty),
                    is_ridden: game
                        .entity_store
                        .vehicles
                        .get(&entity_id)
                        .is_some_and(|v| !v.passengers.is_empty()),
                    overlay_tints: extras.overlay_tints,
                    overlay_variants: extras.overlay_variants,
                    is_unhappy: e.unhappy_counter > 0,
                    head_y_offset: extras.head_y_offset,
                    head_x_rot_deg_override: extras.head_x_rot_deg_override,
                    has_red_overlay: has_red_overlay(e.hurt_time, e.death_time),
                    death_time: render_death_time(e.death_time, partial_tick),
                    aggressive: e.aggressive,
                    flap: extras.flap,
                    flap_speed: extras.flap_speed,
                    is_creepy: e.is_creepy,
                    is_converting: e.is_converting,
                    // TODO: derive from the main-hand item (vanilla
                    // `isHoldingItem`) once mob equipment tracking lands.
                    is_holding_item: e.witch_drinking,
                    nose_wobble_speed: extras.nose_wobble_speed,
                    is_sitting: e.is_sitting,
                    is_sprinting: e.is_sprinting,
                    is_angry: extras.is_angry,
                    tail_angle: extras.tail_angle,
                    head_roll_angle: extras.head_roll_angle,
                    shake_anim: extras.shake_anim,
                    lie_down_amount: extras.lie_down_amount,
                    lie_down_amount_tail: extras.lie_down_amount_tail,
                    relax_state_one_amount: extras.relax_state_one_amount,
                    hop_elapsed_secs: extras.hop_elapsed_secs,
                    base_tint: extras.base_tint.unwrap_or(WHITE_TINT),
                    eat_anim: extras.eat_anim,
                    stand_anim: extras.stand_anim,
                    feeding_anim: extras.feeding_anim,
                    animate_tail: extras.animate_tail,
                    is_in_water: e.is_in_water,
                    is_on_ground: e.on_ground,
                    tentacle_angle: extras.tentacle_angle,
                    bat_resting: e.bat_resting,
                    bat_elapsed_secs: extras.bat_elapsed_secs,
                    golem_attack_ticks: extras.golem_attack_ticks,
                    golem_offer_flower_ticks: extras.golem_offer_flower_ticks,
                    body_transform: extras.body_transform,
                    age_in_ticks: e.age_in_ticks as f32 + partial_tick,
                    animation_phase: extras.animation_phase,
                    attack_time: e.swing_progress(partial_tick),
                    vex_charging: extras.vex_charging,
                    skip_cull: false,
                    ..Default::default()
                })
            })
            .collect()
    };

    if core.probe.is_some()
        && game.tick_count % 20 == 0
        && game.probe_actor_diag_tick != game.tick_count
    {
        game.probe_actor_diag_tick = game.tick_count;
        let camera = gfx.renderer.camera_render_position();
        let actors: Vec<String> = game
            .entity_store
            .living
            .iter()
            .map(|(&entity_id, entity)| {
                let player = entity.player_uuid.and_then(|uuid| game.tab_list.players.get(&uuid));
                let name = player.map(|p| p.name.as_str()).unwrap_or("<non-player>");
                let game_mode = player.map(|p| p.game_mode.to_string()).unwrap_or_else(|| "na".into());
                let spectator = player.is_some_and(|p| p.game_mode == 3);
                let skin_sprite = player.is_some_and(|p| p.textures.is_some());
                let distance = entity.position.distance(camera);
                let draw_eligible = !crate::app::probe::should_exclude_peer(
                    probe_peer_filter.as_ref(),
                    entity_id,
                    game.player.entity_id,
                    entity.player_uuid,
                    player.map(|p| p.name.as_str()),
                );
                format!(
                    "id={entity_id} name={name} kind={:?} uuid={:?} gameMode={game_mode} spectator={spectator} pos=({:.3},{:.3},{:.3}) distance={distance:.3} bodyBounds=({:.3},{:.3},{:.3})..({:.3},{:.3},{:.3}) headBounds=({:.3},{:.3},{:.3})..({:.3},{:.3},{:.3}) skinSprite={skin_sprite} drawEligible={draw_eligible}",
                    entity.entity_type,
                    entity.player_uuid,
                    entity.position.x,
                    entity.position.y,
                    entity.position.z,
                    entity.position.x - 0.3,
                    entity.position.y,
                    entity.position.z - 0.3,
                    entity.position.x + 0.3,
                    entity.position.y + 1.5,
                    entity.position.z + 0.3,
                    entity.position.x - 0.3,
                    entity.position.y + 1.5,
                    entity.position.z - 0.3,
                    entity.position.x + 0.3,
                    entity.position.y + 1.8,
                    entity.position.z + 0.3,
                )
            })
            .collect();
        tracing::info!(
            target = "renderprobe",
            camera = ?camera,
            localEntityId = game.player.entity_id,
            excludedPeer = ?probe_peer_filter,
            actorCount = actors.len(),
            actors = ?actors,
            "actual render actor diagnostic"
        );
    }

    if !benchmark_running
        && !gfx.renderer.is_first_person()
        && !game.player.death_animation_finished()
    {
        let interp_pos = game
            .player
            .prev_position
            .lerp(game.player.position, partial_tick as f64);

        let interp_y_rot_deg = lerp_angle(
            game.player.prev_look_dir.y_rot_deg(),
            game.player.look_dir.y_rot_deg(),
            partial_tick,
        );

        entity_renders.push(EntityRenderInfo {
            happy_ghast_equipment_layers: Vec::new(),
            chest_equipment: game
                .player
                .inventory
                .slot(crate::player::inventory::ARMOR_START + 1)
                .clone(),
            position: interp_pos,
            simulation_position: game.player.position,
            head_y_rot_deg: interp_y_rot_deg,
            head_x_rot_deg: gfx.renderer.camera_look_dir().x_rot_deg(),
            body_y_rot_deg: interp_y_rot_deg, // TODO: proper body rotation affected by collisions
            cape_pose: native_cape_pose(
                game.player.cloak.render_lerp(f64::from(partial_tick)),
                *interp_pos,
                interp_y_rot_deg,
                game.player
                    .prev_walk_dist
                    .lerp(game.player.walk_dist, partial_tick),
                game.player.prev_bob.lerp(game.player.bob, partial_tick),
                if game.player.fall_flying {
                    game.player.fall_flying_ticks as f32
                } else {
                    0.0
                },
                partial_tick,
            ),
            is_crouching: game.player.crouching && (!game.dead || game.player.death_time > 0),
            walk_anim_pos: game.player_walk_pos - game.player_walk_speed * (1.0 - partial_tick),
            walk_anim_speed: (game.player_prev_walk_speed
                + (game.player_walk_speed - game.player_prev_walk_speed) * partial_tick)
                .min(1.0),
            entity_kind: EntityKind::Player,
            player_uuid: Some(core.user.uuid),
            skin_parts_mask: game.local_skin_parts_mask,
            has_red_overlay: has_red_overlay(game.player.hurt_time, game.player.death_time),
            death_time: render_death_time(game.player.death_time, partial_tick),
            skip_cull: true,
            ..Default::default()
        });
    }

    if !benchmark_running {
        entity_renders.extend(arrow_render_infos(&game.entity_store, partial_tick));
        let camera_look = gfx.renderer.camera_look_dir();
        entity_renders.extend(projectile_render_infos(
            &game.entity_store,
            partial_tick,
            camera_look.y_rot_deg(),
            camera_look.x_rot_deg(),
            gfx.renderer.camera_orientation(),
        ));
        entity_renders.extend(boat_render_infos(
            &game.entity_store,
            &game.chunk_store,
            partial_tick,
        ));
        entity_renders.extend(minecart_render_infos(&game.entity_store, partial_tick));
        entity_renders.extend(end_crystal_render_infos(&game.entity_store, partial_tick));
        entity_renders.extend(armor_stand_render_infos(&game.entity_store, partial_tick));
        entity_renders.extend(mannequin_render_infos(&game.entity_store, partial_tick));
        entity_renders.extend(experience_orb_render_infos(
            &game.entity_store,
            &game.chunk_store,
            &game.dimension,
            partial_tick,
            camera_look.y_rot_deg(),
            camera_look.x_rot_deg(),
        ));
    }

    let sky_partial_tick = if core.server_tick_frozen {
        0.0
    } else {
        (core.time_tick_accumulator / server_time_tick_period(core.server_tick_rate))
            .clamp(0.0, 1.0)
    };
    let sky = crate::renderer::SkyState {
        day_time: game.sky_state.day_time,
        game_time: game.sky_state.game_time,
        rain_level: game.sky_state.rain_level,
        thunder_level: game.sky_state.thunder_level,
        clock_id: game.sky_state.clock_id,
        clock_partial_tick: game.sky_state.clock_partial_tick,
        clock_rate: game.sky_state.clock_rate,
        last_network_clock: game.sky_state.last_network_clock,
        partial_tick: sky_partial_tick,
    };
    if game.show_chunk_borders {
        gfx.renderer.update_chunk_borders(
            game.chunk_store.min_y(),
            game.chunk_store.min_y() + game.chunk_store.height() as i32,
        );
    }

    let item_age_partial_tick = if core.server_tick_frozen {
        1.0
    } else {
        sky_partial_tick
    };
    let mut item_renders = if benchmark_running {
        Vec::new()
    } else {
        build_item_render_infos(
            &game.item_entity_store,
            &game.entity_store,
            &game.chunk_store,
            &gfx.renderer,
            game.cardinal_light,
            gfx.renderer.camera_render_position(),
            gfx.renderer.camera_anchor(),
            partial_tick,
            item_age_partial_tick,
            entity_view_scale,
        )
    };
    if !benchmark_running {
        let camera_anchor = gfx.renderer.camera_anchor();
        item_renders.extend(minecart_cargo_render_infos(
            &game.entity_store,
            &mut gfx.renderer,
            &game.chunk_store,
            &game.dimension,
            game.cardinal_light,
            camera_anchor,
            partial_tick,
        ));
        for vehicle in game.entity_store.vehicles.values() {
            if vehicle.kind != Some(azalea_registry::builtin::EntityKind::BlockDisplay) {
                continue;
            }
            let Some(state) = vehicle
                .display
                .block_state
                .and_then(crate::world::block::try_state)
            else {
                continue;
            };
            let pos = vehicle
                .prev_position
                .lerp(vehicle.position, f64::from(partial_tick));
            let block_pos = [
                pos.x.floor() as i32,
                pos.y.floor() as i32,
                pos.z.floor() as i32,
            ];
            let Some(item_name) = gfx.renderer.ensure_block_mesh(state, block_pos) else {
                continue;
            };
            let d = vehicle.display;
            let quat = |q: [f32; 4]| {
                let q = glam::Quat::from_xyzw(q[0], q[1], q[2], q[3]);
                if q.is_finite() && q.length_squared() > 1.0e-12 {
                    q.normalize()
                } else {
                    glam::Quat::IDENTITY
                }
            };
            let (entity_yaw, entity_pitch) = vehicle
                .look_dir
                .map_or((0.0, 0.0), |look| (look.y_rot_deg(), look.x_rot_deg()));
            let (camera_yaw, camera_pitch) = gfx.renderer.camera_effective_look_deg();
            let (yaw, pitch) = match d.billboard {
                1 => (180.0 - camera_yaw, entity_pitch),
                2 => (entity_yaw, -camera_pitch),
                3 => (180.0 - camera_yaw, -camera_pitch),
                _ => (entity_yaw, entity_pitch),
            };
            let orientation = glam::Quat::from_euler(
                glam::EulerRot::YXZ,
                -yaw.to_radians(),
                pitch.to_radians(),
                0.0,
            );
            let trs = glam::Mat4::from_quat(orientation)
                * glam::Mat4::from_translation(glam::Vec3::from_array(d.translation))
                * glam::Mat4::from_quat(quat(d.left_rotation))
                * glam::Mat4::from_scale(glam::Vec3::from_array(d.scale))
                * glam::Mat4::from_quat(quat(d.right_rotation));
            let relative = glam::DVec3::new(pos.x, pos.y, pos.z) - camera_anchor;
            item_renders.push(crate::renderer::pipelines::item_entity::ItemRenderInfo {
                item_name,
                raw_dye_rgb: None,
                player_head_profile_source: None,
                model_matrix: glam::Mat4::from_translation(
                    relative.as_vec3() + glam::Vec3::splat(0.5),
                ) * trs,
                light: lightmap_brightness(
                    &game.chunk_store,
                    &game.dimension,
                    block_pos[0],
                    block_pos[1],
                    block_pos[2],
                ),
                white_overlay: 0.0,
                nether_lighting: game.cardinal_light == CardinalLightType::Nether,
                entity_uuid: None,
                invisible: vehicle.shared_flags & 0x20 != 0,
                actual_age: None,
                actual_render_age: 0.0,
                age_f: 0.0,
                actual_spin: 0.0,
                spin: 0.0,
                bob_offset: 0.0,
                actual_bob_offset: 0.0,
                controlled_phase: false,
                bob_controlled: false,
                position: pos.to_array(),
                stack_count: 1,
            });
        }
        for vehicle in game.entity_store.vehicles.values() {
            if vehicle.kind != Some(azalea_registry::builtin::EntityKind::FallingBlock) {
                continue;
            }
            let Some(state) = vehicle.falling_block.state else {
                continue;
            };
            let pos = vehicle
                .prev_position
                .lerp(vehicle.position, f64::from(partial_tick));
            let block_pos = crate::entity::falling_block_sample_pos(pos);
            if crate::world::block::is_air(state)
                || game
                    .chunk_store
                    .get_block_state(block_pos.x, block_pos.y, block_pos.z)
                    == state
                || crate::world::block_entity::is_invisible_block(crate::world::block::block_id(
                    state,
                ))
            {
                continue;
            }
            let start = vehicle.falling_block.start_pos;
            let Some(item_name) = gfx
                .renderer
                .ensure_block_mesh(state, [start.x, start.y, start.z])
            else {
                continue;
            };
            item_renders.push(crate::renderer::pipelines::item_entity::ItemRenderInfo {
                item_name,
                raw_dye_rgb: None,
                player_head_profile_source: None,
                // Block meshes are centered by -0.5 per vertex; restore that half-block
                // while matching FallingBlockRenderer's root (-0.5, 0, -0.5).
                model_matrix: crate::entity::falling_block_model_matrix(
                    pos,
                    gfx.renderer.camera_anchor(),
                ),
                light: lightmap_brightness(
                    &game.chunk_store,
                    &game.dimension,
                    block_pos.x,
                    block_pos.y,
                    block_pos.z,
                ),
                white_overlay: 0.0,
                nether_lighting: game.cardinal_light == CardinalLightType::Nether,
                entity_uuid: None,
                invisible: false,
                actual_age: None,
                actual_render_age: 0.0,
                age_f: 0.0,
                actual_spin: 0.0,
                spin: 0.0,
                bob_offset: 0.0,
                actual_bob_offset: 0.0,
                controlled_phase: false,
                bob_controlled: false,
                position: pos.to_array(),
                stack_count: 1,
            });
        }
        for vehicle in game.entity_store.vehicles.values() {
            if vehicle.kind != Some(azalea_registry::builtin::EntityKind::ItemDisplay) {
                continue;
            }
            let azalea_inventory::ItemStack::Present(stack) = &vehicle.item_display_stack else {
                continue;
            };
            let item_name = crate::player::inventory::item_resource_name(stack.kind);
            gfx.renderer.ensure_item_mesh(&item_name);
            let pos = vehicle
                .prev_position
                .lerp(vehicle.position, f64::from(partial_tick));
            let block_pos = [
                pos.x.floor() as i32,
                pos.y.floor() as i32,
                pos.z.floor() as i32,
            ];
            let d = vehicle.display;
            let quat = |q: [f32; 4]| {
                let q = glam::Quat::from_xyzw(q[0], q[1], q[2], q[3]);
                if q.is_finite() && q.length_squared() > 1.0e-12 {
                    q.normalize()
                } else {
                    glam::Quat::IDENTITY
                }
            };
            let (entity_yaw, entity_pitch) = vehicle
                .look_dir
                .map_or((0.0, 0.0), |look| (look.y_rot_deg(), look.x_rot_deg()));
            let (camera_yaw, camera_pitch) = gfx.renderer.camera_effective_look_deg();
            let (yaw, pitch) = match d.billboard {
                1 => (180.0 - camera_yaw, entity_pitch),
                2 => (entity_yaw, -camera_pitch),
                3 => (180.0 - camera_yaw, -camera_pitch),
                _ => (entity_yaw, entity_pitch),
            };
            let orientation = glam::Quat::from_euler(
                glam::EulerRot::YXZ,
                -yaw.to_radians(),
                pitch.to_radians(),
                0.0,
            );
            let trs = glam::Mat4::from_quat(orientation)
                * glam::Mat4::from_translation(glam::Vec3::from_array(d.translation))
                * glam::Mat4::from_quat(quat(d.left_rotation))
                * glam::Mat4::from_scale(glam::Vec3::from_array(d.scale))
                * glam::Mat4::from_quat(quat(d.right_rotation));
            let relative = glam::DVec3::new(pos.x, pos.y, pos.z) - gfx.renderer.camera_anchor();
            item_renders.push(crate::renderer::pipelines::item_entity::ItemRenderInfo {
                item_name,
                raw_dye_rgb: None,
                player_head_profile_source: None,
                model_matrix: glam::Mat4::from_translation(
                    relative.as_vec3() + glam::Vec3::splat(0.5),
                ) * trs
                    * glam::Mat4::from_rotation_y(std::f32::consts::PI),
                light: if d.brightness >= 0 {
                    (((d.brightness >> 4) & 0xf).max((d.brightness >> 20) & 0xf)) as f32 / 15.0
                } else {
                    lightmap_brightness(
                        &game.chunk_store,
                        &game.dimension,
                        block_pos[0],
                        block_pos[1],
                        block_pos[2],
                    )
                },
                white_overlay: 0.0,
                nether_lighting: game.cardinal_light == CardinalLightType::Nether,
                entity_uuid: None,
                invisible: vehicle.shared_flags & 0x20 != 0,
                actual_age: None,
                actual_render_age: 0.0,
                age_f: 0.0,
                actual_spin: 0.0,
                spin: 0.0,
                bob_offset: 0.0,
                actual_bob_offset: 0.0,
                controlled_phase: false,
                bob_controlled: false,
                position: pos.to_array(),
                stack_count: 1,
            });
        }
        for vehicle in game.entity_store.vehicles.values() {
            if vehicle.kind != Some(azalea_registry::builtin::EntityKind::Tnt) {
                continue;
            }
            let state = match vehicle.tnt_block_state {
                Some(id) => crate::world::block::try_state(id),
                None => crate::world::block::default_state_of("tnt"),
            };
            let Some(state) = state else { continue };
            let pos = vehicle
                .prev_position
                .lerp(vehicle.position, f64::from(partial_tick));
            let block_pos = [
                pos.x.floor() as i32,
                pos.y.floor() as i32,
                pos.z.floor() as i32,
            ];
            let Some(item_name) = gfx.renderer.ensure_block_mesh(state, block_pos) else {
                continue;
            };
            let fuse = vehicle.tnt_prev_fuse as f32
                + (vehicle.tnt_fuse - vehicle.tnt_prev_fuse) as f32 * partial_tick;
            let (swell, white_overlay) = tnt_render_effect(fuse);
            let relative = glam::DVec3::new(pos.x, pos.y, pos.z) - camera_anchor;
            item_renders.push(crate::renderer::pipelines::item_entity::ItemRenderInfo {
                item_name,
                raw_dye_rgb: None,
                player_head_profile_source: None,
                model_matrix: glam::Mat4::from_translation(
                    relative.as_vec3() + glam::Vec3::Y * 0.5,
                ) * glam::Mat4::from_scale(glam::Vec3::splat(swell)),
                light: lightmap_brightness(
                    &game.chunk_store,
                    &game.dimension,
                    block_pos[0],
                    block_pos[1],
                    block_pos[2],
                ),
                white_overlay,
                nether_lighting: game.cardinal_light == CardinalLightType::Nether,
                entity_uuid: None,
                invisible: false,
                actual_age: None,
                actual_render_age: 0.0,
                age_f: 0.0,
                actual_spin: 0.0,
                spin: 0.0,
                bob_offset: 0.0,
                actual_bob_offset: 0.0,
                controlled_phase: false,
                bob_controlled: false,
                position: pos.to_array(),
                stack_count: 1,
            });
        }
    }

    let scene_extract_first_ms = scene_extract_start
        .map(|start| start.elapsed().as_secs_f32() * 1000.0)
        .unwrap_or_default();

    let be_extract_start = game.benchmark.is_some().then(std::time::Instant::now);
    let block_entity_renders: Vec<crate::renderer::BlockEntityRenderInfo> = if benchmark_running {
        Vec::new()
    } else {
        let be_frustum = game
            .position_set
            .then(|| gfx.renderer.block_entity_frustum_planes());
        let be_eye = gfx.renderer.camera_render_position();
        let sign_text_eye = *gfx.renderer.camera_pivot_position();
        game.chunk_store
            .block_entities
            .iter()
            .filter_map(|(pos, be)| {
                // This existing loose box needs only the BE kind/text, not the
                // block state. Unknown kinds and written signs are never culled.
                if let Some(planes) = &be_frustum {
                    let has_sign_text = crate::world::block_entity::is_sign_kind(be.kind)
                        && (sign_has_text(be.sign_front.as_ref(), be.sign_back.as_ref())
                            || game.sign_edit.as_ref().is_some_and(|edit| edit.pos == *pos));
                    if !block_entity_in_frustum(be.kind, pos, has_sign_text, be_eye, planes) {
                        return None;
                    }
                }
                let state = game.chunk_store.get_block_state(pos.x, pos.y, pos.z);
                let id = crate::world::block::block_id(state);
                // A predicted break leaves a stale entry until the server
                // confirms; don't render entries whose block is gone.
                if !crate::world::block_entity::is_block_entity_block(id) {
                    return None;
                }
                // Claim only accepted, still-visible resident geometry. An in-flight
                // edit keeps the old mesh/pose until its replacement lands; an
                // upload failure/unload/LOD mesh without chest quads falls back
                // to the BE. A double half is suppressed only when its own
                // indexed geometry is resident, regardless of its partner.
                let column = ChunkPos::new(pos.x.div_euclid(16), pos.z.div_euclid(16));
                let si = (pos.y - game.chunk_store.min_y()).div_euclid(16);
                if be.kind == BlockEntityKind::Chest
                    && chest_section_visible(
                        si,
                        game.vis_valid,
                        game.vis_mask.get(&column).copied(),
                    )
                    && gfx.renderer.resident_chunk_chest_open(pos).is_some()
                {
                    return None;
                }
                let props = crate::world::block::block_properties(state);
                let statue =
                    crate::world::block_entity::copper_golem_statue_render_state(id, props);
                let variant = statue.map_or_else(
                    || block_entity::variant_for_block(be.kind, id, props),
                    |(_, oxidation)| oxidation,
                );
                let yaw = block_entity::yaw_for_block(be.kind, props);
                let openness_at = |p: &BlockPos| {
                    game.block_entity_anim
                        .container(p)
                        .map(|a| a.openness(partial_tick))
                        .unwrap_or(0.0)
                };
                let mut lid_open = openness_at(pos);
                // A double chest's lids follow the max openness of both halves
                // (vanilla opennessCombiner); the open block event only arrives
                // at the interacted half's position.
                if matches!(
                    be.kind,
                    BlockEntityKind::Chest | BlockEntityKind::TrappedChest
                ) && let Some((dx, dz)) = block_entity::chest_partner_offset(
                    props.get("facing").unwrap_or("north"),
                    props.get("type").unwrap_or("single"),
                ) {
                    let partner = BlockPos::new(pos.x + dx, pos.y, pos.z + dz);
                    lid_open = lid_open.max(openness_at(&partner));
                }
                let is_sign = crate::world::block_entity::is_sign_kind(be.kind);
                let copy_sign_text = is_sign && sign_text_in_range(pos, sign_text_eye);
                // Client-only live preview; never mutate server NBT or its cached faces.
                let face_lines = |front, stored: Option<&[String; 4]>| {
                    game.sign_edit.as_ref().map_or_else(
                        || stored.cloned().unwrap_or_default(),
                        |edit| edit.preview_lines(*pos, front, stored),
                    )
                };
                let sign_front = copy_sign_text.then(|| face_lines(true, be.sign_front.as_ref()));
                let sign_back = copy_sign_text.then(|| face_lines(false, be.sign_back.as_ref()));
                let ((sign_front_color, sign_front_glowing), (sign_back_color, sign_back_glowing)) =
                    if is_sign {
                        sign_render_style(&be.nbt)
                    } else {
                        (([0.0; 3], false), ([0.0; 3], false))
                    };
                let player_head_profile_source = matches!(id, "player_head" | "player_wall_head")
                    .then(|| be.player_head_profile_source.clone())
                    .flatten();
                let pot_wobble = be.pot_wobble.and_then(|(started, positive)| {
                    let duration = crate::world::block_entity::pot_wobble_duration(positive) as f32;
                    let progress =
                        (game.tick_count.saturating_sub(started) as f32 + partial_tick) / duration;
                    (progress <= 1.0).then_some((progress, positive))
                });
                Some(crate::renderer::BlockEntityRenderInfo {
                    pos: *pos,
                    player_head_profile_source,
                    bell_swing: be.bell_swing,
                    decorated_pot_sherds: be.decorated_pot_sherds.clone(),
                    pot_wobble,
                    kind: be.kind,
                    statue_pose: statue.map(|(pose, _)| pose),
                    book: be.book.as_ref().map(|book| book.interpolated(partial_tick)),
                    banner_phase: (pos.x as i64 * 7
                        + pos.y as i64 * 9
                        + pos.z as i64 * 13
                        + game.tick_count as i64)
                        .rem_euclid(100) as f32
                        + partial_tick,
                    bell_partial: partial_tick,
                    yaw,
                    variant,
                    lid_open,
                    sign_front,
                    sign_back,
                    sign_front_color,
                    sign_front_glowing,
                    sign_back_color,
                    sign_back_glowing,
                    sign_wall: props.get("facing").is_some(),
                    sign_light: if is_sign {
                        lightmap_brightness(&game.chunk_store, &game.dimension, pos.x, pos.y, pos.z)
                    } else {
                        1.0
                    },
                })
            })
            .collect()
    };
    game.last_update_phases.be_extract_ms = be_extract_start
        .map(|start| start.elapsed().as_secs_f32() * 1000.0)
        .unwrap_or_default();

    let scene_extract_second_start = game.benchmark.is_some().then(std::time::Instant::now);
    let weather_columns = if benchmark_running {
        Vec::new()
    } else {
        build_weather_columns(
            &game.chunk_store,
            &game.biome_climate,
            gfx.renderer.camera_render_position(),
            sky.rain(),
        )
    };

    let particle_quads = if benchmark_running {
        Vec::new()
    } else {
        game.particle_store
            .extract(partial_tick, gfx.renderer.camera_anchor())
    };

    let held_item = if benchmark_running {
        (None, None)
    } else {
        let held_item = |held_stack: &azalea_inventory::ItemStack| match held_stack {
            azalea_inventory::ItemStack::Present(data) if data.count > 0 => {
                let name = crate::player::inventory::item_resource_name(data.kind);
                (name != "air").then(|| {
                    let light =
                        get_entity_light(&game.chunk_store, gfx.renderer.camera_pivot_position());
                    let raw_dye_rgb = data
                        .get_component::<azalea_inventory::components::DyedColor>()
                        .map(|color| {
                            let rgb = color.rgb;
                            [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8]
                        });
                    (
                        name,
                        light,
                        raw_dye_rgb,
                        crate::world::block_entity::player_head_profile_source_from_item(
                            held_stack,
                        ),
                    )
                })
            }
            _ => None,
        };
        (
            held_item(&game.player.inventory.hotbar_slots()[core.input.selected_slot() as usize]),
            held_item(game.player.inventory.offhand()),
        )
    };
    let main_hand_right = core.menu.main_hand_right();
    game.last_update_phases.scene_extract_ms = scene_extract_first_ms
        + scene_extract_second_start
            .map(|start| start.elapsed().as_secs_f32() * 1000.0)
            .unwrap_or_default();

    // Last element pushed: vanilla draws the saving indicator on its own
    // stratum above screens, with the GUI hidden (F1) included.
    if !benchmark_running && core.menu.show_autosave_indicator {
        let alpha = game
            .last_saving_indicator_value
            .lerp(game.saving_indicator_value, partial_tick)
            .clamp(0.0, 1.0);
        if (alpha * 255.0).floor() > 0.0 {
            hud::build_saving_indicator(&mut elements, sw, sh, gs, alpha, &|t, s| {
                gfx.renderer.menu_text_width(t, s)
            });
        }
    }

    if let Some(mut probe) = core.probe.take() {
        probe.poll(core, &mut gfx.renderer, game, &sky);
        elements.extend(probe.item_overlay_elements());
        core.probe = Some(probe);
    }

    // Recompute after this frame's state changes (a finished benchmark releases
    // the cursor mid-frame), so the renderer doesn't re-hide it from a stale value.
    let hide_cursor = game.input_live() && !game.dead && core.input.is_cursor_captured();
    gfx.renderer.set_world_border(
        game.client_loaded.then_some(&game.world_border),
        partial_tick,
    );
    let show_hand = !game.hide_gui
        && core
            .probe
            .as_ref()
            .is_none_or(|probe| probe.held_item_draw_enabled());
    let activation_profile = game
        .item_activation
        .as_ref()
        .and_then(|activation| activation.draw(partial_tick))
        .and_then(|draw| {
            crate::world::block_entity::player_head_profile_source_from_item(draw.stack)
        });
    gfx.renderer.update_head_skins(
        block_entity_renders
            .iter()
            .filter_map(|head| head.player_head_profile_source.as_ref())
            .chain(
                item_renders
                    .iter()
                    .filter_map(|item| item.player_head_profile_source.as_ref()),
            )
            .chain(elements.iter().filter_map(|element| match element {
                crate::renderer::pipelines::menu_overlay::MenuElement::ItemIcon {
                    player_head_profile_source,
                    ..
                } => player_head_profile_source.as_ref(),
                _ => None,
            }))
            .chain(held_item.0.as_ref().and_then(|item| item.3.as_ref()))
            .chain(held_item.1.as_ref().and_then(|item| item.3.as_ref()))
            .chain(activation_profile.as_ref()),
        &core.tokio_rt,
    );
    game.last_update_phases.cpu_update_ms = frame_start.elapsed().as_secs_f32() * 1000.0;
    let render_start = std::time::Instant::now();
    if let Err(e) = gfx.renderer.render_world(
        &gfx.window,
        hide_cursor,
        show_hand,
        main_hand_right,
        elements,
        hand_animation,
        use_anim,
        held_item,
        destroy_info,
        game.show_chunk_borders,
        game.dimension.as_str(),
        sky,
        &entity_renders,
        // World text is independent of F1, but suppressed during benchmarks as before.
        (!benchmark_running).then_some(&game.entity_store),
        &item_renders,
        &block_entity_renders,
        &particle_quads,
        &weather_columns,
        if benchmark_running {
            crate::renderer::CloudMode::Off
        } else {
            core.menu.cloud_mode
        },
        effective_rd,
        core.menu.entity_distance_percent,
        &game.chunk_store,
        player_preview,
        book_preview,
        &map_quads,
        game.player.eyes_in_water,
        game.item_activation
            .as_ref()
            .and_then(|activation| activation.draw(partial_tick)),
        game.benchmark.is_some(),
    ) {
        tracing::error!("Render error: {e}");
    }
    game.last_update_phases.render_wall_ms = render_start.elapsed().as_secs_f32() * 1000.0;
    // Whole-frame wall time (incl. render), read next frame to align with `raw_dt`.
    game.last_update_phases.update_ms = frame_start.elapsed().as_secs_f32() * 1000.0;

    if close_inventory {
        game.close_menu();
        game.close_creative_inventory();
        core.apply_cursor_grab(&gfx.window, Some(game));
    }

    // Tell the server when a container menu closes so it returns/drops the
    // cursor stack (and a crafting grid's contents).
    let open_menu = game.open_menu_id();
    if let Some(prev) = game.container_was_open
        && open_menu != Some(prev)
    {
        game.recipe_book.reset_menu();
        use azalea_protocol::packets::game::s_container_close::ServerboundContainerClose;
        connection
            .packet_tx
            .send(ServerboundGamePacket::ContainerClose(
                ServerboundContainerClose { container_id: prev },
            ));
    }
    game.container_was_open = open_menu;

    match death_action {
        DeathAction::Respawn => {
            game.death_confirm = false;
            core.send_respawn(connection, game);
        }
        DeathAction::TitleScreen => {
            game.tab_score_state.set_visible(false);
            return GameUpdateResult::ManualDisconnect;
        }
        DeathAction::ShowConfirm => {
            game.death_confirm = true;
            game.death_confirm_ticks = 0;
        }
        DeathAction::None => {}
    }

    match pause_action {
        PauseAction::Resume => {
            game.paused = false;
            core.apply_cursor_grab(&gfx.window, Some(game));
        }
        PauseAction::Options => {
            core.menu.open_options();
            game.options_from_game = true;
            core.apply_cursor_grab(&gfx.window, Some(game));
        }
        PauseAction::Disconnect => {
            game.tab_score_state.set_visible(false);
            return GameUpdateResult::ManualDisconnect;
        }
        PauseAction::OpenBenchmark => {
            game.pause_screen = PauseScreen::Benchmark;
        }
        PauseAction::OpenChunkLoader => {
            game.pause_screen = PauseScreen::ChunkLoader;
        }
        PauseAction::Back => {
            game.pause_screen = match game.pause_screen {
                PauseScreen::ChunkLoader => PauseScreen::Benchmark,
                _ => PauseScreen::Main,
            };
        }
        PauseAction::StartMovementRecording => {
            connection
                .packet_tx
                .recorder
                .start(&core.tokio_rt, &core.data_dirs.game_dir);
            core.menu.movement_recording = Some(connection.packet_tx.recorder.clone());
            game.paused = false;
            core.apply_cursor_grab(&gfx.window, Some(game));
        }
        PauseAction::StopMovementRecording => {
            connection.packet_tx.recorder.stop("user_stop");
        }
        PauseAction::StartFpsBenchmark => {
            game.benchmark = Some(Benchmark::new(
                gfx.renderer.gpu_name(),
                gfx.renderer.screen_width(),
                gfx.renderer.screen_height(),
                core.menu.render_distance,
            ));
            game.benchmark_result = None;
            game.pause_screen = PauseScreen::Main;
            game.paused = false;
            core.apply_cursor_grab(&gfx.window, Some(game));
        }
        PauseAction::StartChunkLoad(rd) => {
            game.chunk_load_bench = Some(ChunkLoadBench::new(
                rd,
                core.menu.render_distance,
                game.server_render_distance,
                gfx.renderer.gpu_name(),
                gfx.renderer.vulkan_version(),
                gfx.renderer.screen_width(),
                gfx.renderer.screen_height(),
                [
                    game.player.position.x,
                    game.player.position.y,
                    game.player.position.z,
                ],
            ));
            game.chunk_load_result = None;
            game.pause_screen = PauseScreen::Main;
            game.paused = false;
            // Drop to the minimum render distance so the server unloads the far
            // chunks; the driver raises it to the target once the reset settles.
            apply_render_distance(core, game, connection, crate::benchmark::CHUNK_LOAD_MIN_RD);
            core.apply_cursor_grab(&gfx.window, Some(game));
        }
        PauseAction::ReportBugs => {
            let _ = open::that("https://github.com/PommeMC/Client/issues");
        }
        PauseAction::None => {}
    }

    if game.options_from_game {
        if game.client_information_changed(
            core.menu.render_distance,
            core.menu.chat_options,
            core.menu.main_hand_right(),
            core.menu.particle_status(),
            core.menu.skin_parts_mask(),
        ) {
            game.sync_client_information(
                connection,
                core.menu.render_distance,
                core.menu.chat_options,
                core.menu.main_hand_right(),
                core.menu.particle_status(),
                core.menu.skin_parts_mask(),
            );
        }
        if !core.menu.is_options_screen() {
            game.options_from_game = false;
            // Chat Settings opened from chat return to it, unpaused.
            game.paused = !game.chat.return_from_settings(game.command_tree.as_deref());
            core.apply_cursor_grab(&gfx.window, Some(game));
        }
    }

    GameUpdateResult::None
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BoatUnderwaterStatus {
    UnderWater,
    UnderFlowingWater,
}

/// Native AbstractBoat.getUnderwaterStatus: scan the box's top plane (+0.001)
/// and distinguish a source-water result from the first flowing-water result.
fn boat_underwater_status(
    min_x: f64,
    max_x: f64,
    max_y: f64,
    min_z: f64,
    max_z: f64,
    mut fluid_at: impl FnMut(i32, i32, i32) -> crate::world::block::Fluid,
) -> Option<BoatUnderwaterStatus> {
    use crate::world::block::FluidKind;

    let max_y = max_y + 0.001;
    let (x0, x1) = (min_x.floor() as i32, max_x.ceil() as i32);
    let (y0, y1) = (max_y.floor() as i32, max_y.ceil() as i32);
    let (z0, z1) = (min_z.floor() as i32, max_z.ceil() as i32);
    let mut still_water = false;
    for x in x0..x1 {
        for y in y0..y1 {
            for z in z0..z1 {
                let water = fluid_at(x, y, z);
                if water.kind != FluidKind::Water {
                    continue;
                }
                let height = if fluid_at(x, y + 1, z).kind == FluidKind::Water {
                    1.0
                } else {
                    water.height()
                };
                if !(max_y < f64::from(y) + f64::from(height)) {
                    continue;
                }
                if water.is_source() {
                    still_water = true;
                } else {
                    return Some(BoatUnderwaterStatus::UnderFlowingWater);
                }
            }
        }
    }
    still_water.then_some(BoatUnderwaterStatus::UnderWater)
}

/// Extract visual interpolation independently from packet movement baselines.
fn boat_render_infos(
    store: &crate::entity::EntityStore,
    chunks: &crate::world::chunk::ChunkStore,
    partial_tick: f32,
) -> Vec<EntityRenderInfo> {
    use azalea_registry::builtin::EntityKind;

    store
        .vehicles
        .values()
        .filter_map(|vehicle| {
            let kind = vehicle.kind?;
            if !matches!(
                kind,
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
            ) {
                return None;
            }
            if vehicle.shared_flags & 0x20 != 0 {
                return None;
            }
            let yaw = match (vehicle.prev_look_dir, vehicle.look_dir) {
                (Some(prev), Some(current)) => {
                    lerp_angle(prev.y_rot_deg(), current.y_rot_deg(), partial_tick)
                }
                (_, Some(current)) => current.y_rot_deg(),
                _ => 0.0,
            };
            let position = vehicle
                .prev_position
                .lerp(vehicle.position, partial_tick as f64);
            let dimensions = azalea_entity::dimensions::EntityDimensions::from(kind);
            let half_width = f64::from(dimensions.width) * 0.5;
            let underwater = boat_underwater_status(
                position.x - half_width,
                position.x + half_width,
                position.y + f64::from(dimensions.height),
                position.z - half_width,
                position.z + half_width,
                |x, y, z| crate::world::block::fluid(chunks.get_block_state(x, y, z)),
            )
            .is_some();
            let rowing_time = std::array::from_fn(|i| {
                vehicle.boat_prev_rowing_time[i]
                    + (vehicle.boat_rowing_time[i] - vehicle.boat_prev_rowing_time[i])
                        * partial_tick
            });
            let hurt_time = vehicle.boat_prev_hurt_time as f32
                + (vehicle.boat_hurt_time - vehicle.boat_prev_hurt_time) as f32 * partial_tick;
            let damage = vehicle.boat_prev_damage
                + (vehicle.boat_damage - vehicle.boat_prev_damage) * partial_tick;
            let hurt_angle = if hurt_time > 0.0 {
                (hurt_time.sin() * hurt_time * damage.max(0.0) / 10.0
                    * vehicle.boat_hurt_direction as f32)
                    .to_radians()
            } else {
                0.0
            };
            let bubble_angle = vehicle.boat_prev_bubble_angle
                + (vehicle.boat_bubble_angle - vehicle.boat_prev_bubble_angle) * partial_tick;
            Some(EntityRenderInfo {
                happy_ghast_equipment_layers: Vec::new(),
                position,
                simulation_position: vehicle.position,
                body_y_rot_deg: yaw,
                entity_kind: kind,
                boat_rowing_time: rowing_time,
                boat_underwater: underwater,
                body_transform: Some(
                    glam::Mat4::from_translation(glam::Vec3::Y * 0.375)
                        * glam::Mat4::from_rotation_x(hurt_angle)
                        * glam::Mat4::from_axis_angle(
                            glam::Vec3::new(1.0, 0.0, 1.0).normalize(),
                            bubble_angle.to_radians(),
                        ),
                ),
                ..Default::default()
            })
        })
        .collect()
}

fn minecart_cargo_state(vehicle: &crate::entity::VehicleState) -> Option<azalea_block::BlockState> {
    minecart_cargo_state_for(
        vehicle.kind?,
        vehicle.minecart_display_state,
        vehicle.minecart_furnace_has_fuel,
    )
}

fn minecart_cargo_state_for(
    kind: azalea_registry::builtin::EntityKind,
    custom_state: Option<u32>,
    furnace_has_fuel: bool,
) -> Option<azalea_block::BlockState> {
    use azalea_registry::builtin::EntityKind;

    use crate::world::block::{block_properties, default_state_of, state_with_properties};

    if let Some(id) = custom_state {
        return crate::world::block::try_state(id)
            .filter(|state| !crate::world::block::is_air(*state));
    }
    let name = match kind {
        EntityKind::ChestMinecart => "chest",
        EntityKind::FurnaceMinecart => "furnace",
        EntityKind::TntMinecart => "tnt",
        EntityKind::HopperMinecart => "hopper",
        EntityKind::CommandBlockMinecart => "command_block",
        EntityKind::SpawnerMinecart => "spawner",
        EntityKind::Minecart => return None,
        _ => return None,
    };
    let mut state = default_state_of(name)?;
    let mut properties: Vec<_> = block_properties(state)
        .entries()
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect();
    if name == "chest" || name == "furnace" {
        if let Some((_, value)) = properties.iter_mut().find(|(key, _)| key == "facing") {
            *value = "north".to_owned();
        }
    }
    if name == "furnace"
        && let Some((_, value)) = properties.iter_mut().find(|(key, _)| key == "lit")
    {
        *value = furnace_has_fuel.to_string();
    }
    if !properties.is_empty() {
        state = state_with_properties(name, &properties)?;
    }
    Some(state)
}

fn minecart_cargo_render_infos(
    store: &crate::entity::EntityStore,
    renderer: &mut crate::renderer::Renderer,
    chunks: &crate::world::chunk::ChunkStore,
    dimension: &str,
    cardinal_light: CardinalLightType,
    camera_anchor: glam::DVec3,
    partial_tick: f32,
) -> Vec<crate::renderer::pipelines::item_entity::ItemRenderInfo> {
    use azalea_registry::builtin::EntityKind;
    store
        .vehicles
        .values()
        .filter_map(|vehicle| {
            let kind = vehicle.kind?;
            if !matches!(
                kind,
                EntityKind::Minecart
                    | EntityKind::ChestMinecart
                    | EntityKind::FurnaceMinecart
                    | EntityKind::TntMinecart
                    | EntityKind::HopperMinecart
                    | EntityKind::CommandBlockMinecart
                    | EntityKind::SpawnerMinecart
            ) || vehicle.shared_flags & 0x20 != 0
            {
                return None;
            }
            let state = minecart_cargo_state(vehicle)?;
            let position = vehicle
                .prev_position
                .lerp(vehicle.position, f64::from(partial_tick));
            let block_pos = [
                position.x.floor() as i32,
                position.y.floor() as i32,
                position.z.floor() as i32,
            ];
            let item_name = renderer.ensure_block_mesh(state, block_pos)?;
            let current = vehicle.look_dir?;
            let (yaw, pitch) = match vehicle.prev_look_dir {
                Some(prev) => (
                    lerp_angle(prev.y_rot_deg(), current.y_rot_deg(), partial_tick),
                    prev.x_rot_deg() + (current.x_rot_deg() - prev.x_rot_deg()) * partial_tick,
                ),
                None => (current.y_rot_deg(), current.x_rot_deg()),
            };
            let hurt = (vehicle.boat_hurt_time as f32 - partial_tick).max(0.0);
            let damage = (vehicle.boat_damage - partial_tick).max(0.0);
            let rocking = (hurt.sin() * hurt * damage / 10.0 * vehicle.boat_hurt_direction as f32)
                .to_radians();
            let offset = vehicle.minecart_display_offset;
            let relative = glam::DVec3::new(position.x, position.y, position.z) - camera_anchor;
            let matrix = glam::Mat4::from_translation(relative.as_vec3())
                * glam::Mat4::from_rotation_y((180.0 - yaw).to_radians())
                * glam::Mat4::from_rotation_z(-pitch.to_radians())
                * glam::Mat4::from_translation(glam::Vec3::Y * 0.375)
                * glam::Mat4::from_rotation_x(rocking)
                * glam::Mat4::from_scale(glam::Vec3::splat(0.75))
                * glam::Mat4::from_translation(glam::vec3(-0.5, (offset - 8) as f32 / 16.0, 0.5))
                * glam::Mat4::from_rotation_y(std::f32::consts::FRAC_PI_2);
            Some(crate::renderer::pipelines::item_entity::ItemRenderInfo {
                item_name,
                raw_dye_rgb: None,
                player_head_profile_source: None,
                model_matrix: matrix,
                light: lightmap_brightness(
                    chunks,
                    dimension,
                    block_pos[0],
                    block_pos[1],
                    block_pos[2],
                ),
                white_overlay: 0.0,
                nether_lighting: cardinal_light == CardinalLightType::Nether,
                entity_uuid: None,
                invisible: false,
                actual_age: None,
                actual_render_age: 0.0,
                age_f: 0.0,
                actual_spin: 0.0,
                spin: 0.0,
                bob_offset: 0.0,
                actual_bob_offset: 0.0,
                controlled_phase: false,
                bob_controlled: false,
                position: position.to_array(),
                stack_count: 1,
            })
        })
        .collect()
}

fn end_crystal_render_infos(
    store: &crate::entity::EntityStore,
    partial_tick: f32,
) -> Vec<EntityRenderInfo> {
    store
        .vehicles
        .values()
        .filter_map(|entity| {
            if entity.kind != Some(azalea_registry::builtin::EntityKind::EndCrystal)
                || entity.shared_flags & 0x20 != 0
            {
                return None;
            }
            let position = entity
                .prev_position
                .lerp(entity.position, partial_tick as f64);
            Some(EntityRenderInfo {
                happy_ghast_equipment_layers: Vec::new(),
                position,
                simulation_position: entity.position,
                entity_kind: azalea_registry::builtin::EntityKind::EndCrystal,
                age_in_ticks: entity.crystal_age as f32 + partial_tick,
                crystal_beam: entity.crystal_beam_target.map(|target| {
                    let target_center = glam::DVec3::new(
                        f64::from(target.x) + 0.5,
                        f64::from(target.y) + 0.5,
                        f64::from(target.z) + 0.5,
                    );
                    crate::renderer::pipelines::entity_renderer::CrystalBeamRenderInfo {
                        target_offset: target_center - *position,
                        age_in_ticks: entity.crystal_age as f32 + partial_tick,
                        light_coords: 0,
                    }
                }),
                variant_index: u32::from(!entity.crystal_show_bottom),
                ..Default::default()
            })
        })
        .collect()
}

fn armor_stand_render_infos(
    store: &crate::entity::EntityStore,
    partial_tick: f32,
) -> Vec<EntityRenderInfo> {
    store
        .vehicles
        .values()
        .filter_map(|stand| {
            if stand.kind != Some(EntityKind::ArmorStand) || stand.shared_flags & 0x20 != 0 {
                return None;
            }
            let position = stand
                .prev_position
                .lerp(stand.position, f64::from(partial_tick));
            let yaw = match (stand.prev_look_dir, stand.look_dir) {
                (Some(prev), Some(now)) => {
                    lerp_angle(prev.y_rot_deg(), now.y_rot_deg(), partial_tick)
                }
                (_, Some(now)) => now.y_rot_deg(),
                _ => 0.0,
            };
            Some(EntityRenderInfo {
                happy_ghast_equipment_layers: Vec::new(),
                position,
                simulation_position: stand.position,
                entity_kind: EntityKind::ArmorStand,
                body_y_rot_deg: yaw,
                armor_stand_flags: stand.armor_stand_flags,
                armor_stand_pose: stand.armor_stand_pose,
                armor_stand_equipment: stand.armor_stand_equipment.clone(),
                body_transform: (stand.armor_stand_flags & 0x01 != 0)
                    .then(|| glam::Mat4::from_scale(glam::Vec3::splat(0.5))),
                ..Default::default()
            })
        })
        .collect()
}

fn mannequin_profile_id(profile: &azalea_inventory::components::Profile) -> Option<uuid::Uuid> {
    use azalea_inventory::components::PartialOrFullProfile;
    match profile.unpack.as_ref() {
        PartialOrFullProfile::Full(profile) => Some(profile.uuid),
        PartialOrFullProfile::Partial(profile) => profile.id,
    }
}

/// Port of AvatarRenderer.extractCapeState plus PlayerCapeModel.setupAnim.
fn native_cape_pose(
    cloak: glam::DVec3,
    position: glam::DVec3,
    body_yaw_deg: f32,
    walk_distance: f32,
    bob: f32,
    fall_flying_ticks: f32,
    partial_tick: f32,
) -> glam::Quat {
    let delta = cloak - position;
    let yaw = body_yaw_deg.to_radians();
    let forward_x = yaw.sin() as f64;
    let forward_z = -(yaw.cos() as f64);
    let flap = (delta.y as f32 * 10.0).clamp(-6.0, 32.0) + (walk_distance * 6.0).sin() * 32.0 * bob;
    let fall_flying_scale = ((fall_flying_ticks + partial_tick).powi(2) / 100.0).clamp(0.0, 1.0);
    let lean =
        (((delta.x * forward_x + delta.z * forward_z) as f32) * 100.0 * (1.0 - fall_flying_scale))
            .clamp(0.0, 150.0);
    let lean2 = (((delta.x * forward_z - delta.z * forward_x) as f32) * 100.0).clamp(-20.0, 20.0);
    glam::Quat::from_rotation_y(-std::f32::consts::PI)
        * glam::Quat::from_rotation_x((6.0 + lean / 2.0 + flap).to_radians())
        * glam::Quat::from_rotation_z((lean2 / 2.0).to_radians())
        * glam::Quat::from_rotation_y((180.0 - lean2 / 2.0).to_radians())
}

fn mannequin_render_infos(
    store: &crate::entity::EntityStore,
    partial_tick: f32,
) -> Vec<EntityRenderInfo> {
    store
        .vehicles
        .values()
        .filter_map(|mannequin| {
            if mannequin.kind != Some(EntityKind::Mannequin) || mannequin.shared_flags & 0x20 != 0 {
                return None;
            }
            let yaw = match (mannequin.prev_look_dir, mannequin.look_dir) {
                (Some(prev), Some(now)) => {
                    lerp_angle(prev.y_rot_deg(), now.y_rot_deg(), partial_tick)
                }
                (_, Some(now)) => now.y_rot_deg(),
                _ => 0.0,
            };
            let profile_uuid = mannequin
                .mannequin_profile
                .as_ref()
                .and_then(mannequin_profile_id);
            // Native fallback variant follows the real profile UUID (or NIL);
            // the skin cache is independently keyed by this entity instance.
            let fallback_uuid = profile_uuid.unwrap_or_else(uuid::Uuid::nil);
            let skin_uuid = mannequin.uuid.unwrap_or(fallback_uuid);
            let position = mannequin
                .prev_position
                .lerp(mannequin.position, f64::from(partial_tick));
            let cape_pose = native_cape_pose(
                mannequin.cloak.render_lerp(f64::from(partial_tick)),
                *position,
                yaw,
                mannequin
                    .cape_motion
                    .prev_walk_dist
                    .lerp(mannequin.cape_motion.walk_dist, partial_tick),
                mannequin
                    .cape_motion
                    .prev_bob
                    .lerp(mannequin.cape_motion.bob, partial_tick),
                mannequin.cape_motion.fall_flying_ticks as f32,
                partial_tick,
            );
            Some(EntityRenderInfo {
                happy_ghast_equipment_layers: Vec::new(),
                cape_pose,
                chest_equipment: mannequin
                    .armor_stand_equipment
                    .get(&azalea_inventory::components::EquipmentSlot::Chest)
                    .cloned()
                    .unwrap_or(azalea_inventory::ItemStack::Empty),
                position,
                simulation_position: mannequin.position,
                entity_kind: EntityKind::Mannequin,
                body_y_rot_deg: yaw,
                player_uuid: Some(skin_uuid),
                variant_index:
                    crate::renderer::pipelines::entity_renderer::default_player_skin_index(
                        fallback_uuid,
                    ) as u32,
                skin_parts_mask: mannequin.mannequin_skin_parts_mask,
                is_crouching: mannequin.mannequin_pose == crate::entity::EntityPose::Crouching,
                is_sleeping: mannequin.mannequin_pose == crate::entity::EntityPose::Sleeping,
                ..Default::default()
            })
        })
        .collect()
}

fn experience_orb_icon(value: i32) -> u32 {
    [3, 7, 17, 37, 73, 149, 307, 617, 1237, 2477]
        .iter()
        .take_while(|&&threshold| value >= threshold)
        .count() as u32
}

fn experience_orb_light(block_light: u8, sky_light: u8, dimension: &str) -> f32 {
    let level = sky_light.max(block_light.saturating_add(7).min(15)) as f32 / 15.0;
    let curved = level / (4.0 - 3.0 * level);
    if dimension == "minecraft:the_nether" {
        curved + (1.0 - curved) * 0.1
    } else {
        curved
    }
}

fn experience_orb_color(age: f32, light: f32) -> [f32; 4] {
    let phase = age / 2.0;
    let red = ((phase.sin() + 1.0) * 0.5 * 255.0) as u8;
    let blue = ((phase + std::f32::consts::PI * 4.0 / 3.0).sin() + 1.0) * 0.1 * 255.0;
    [
        red as f32 / 255.0 * light,
        light,
        blue as u8 as f32 / 255.0 * light,
        128.0 / 255.0,
    ]
}

fn experience_orb_render_infos(
    store: &crate::entity::EntityStore,
    chunks: &ChunkStore,
    dimension: &str,
    partial_tick: f32,
    camera_yaw: f32,
    camera_pitch: f32,
) -> Vec<EntityRenderInfo> {
    store
        .vehicles
        .values()
        .filter_map(|orb| {
            if orb.kind != Some(EntityKind::ExperienceOrb) || orb.shared_flags & 0x20 != 0 {
                return None;
            }
            let age = orb.experience_orb_age as f32 + partial_tick;
            let x = orb.position.x.floor() as i32;
            let y = orb.position.y.floor() as i32;
            let z = orb.position.z.floor() as i32;
            let light = experience_orb_light(
                chunks.get_block_light(x, y, z),
                chunks.get_sky_light(x, y, z),
                dimension,
            );
            let mut overlay_tints = [None; MAX_OVERLAYS];
            overlay_tints[0] = Some(experience_orb_color(age, light));
            Some(EntityRenderInfo {
                happy_ghast_equipment_layers: Vec::new(),
                position: orb.prev_position.lerp(orb.position, partial_tick as f64),
                simulation_position: orb.position,
                entity_kind: EntityKind::ExperienceOrb,
                body_y_rot_deg: camera_yaw,
                head_x_rot_deg: camera_pitch,
                variant_index: experience_orb_icon(orb.experience_orb_value),
                overlay_tints,
                age_in_ticks: age,
                ..Default::default()
            })
        })
        .collect()
}

fn minecart_render_infos(
    store: &crate::entity::EntityStore,
    partial_tick: f32,
) -> Vec<EntityRenderInfo> {
    use azalea_registry::builtin::EntityKind;

    store
        .vehicles
        .values()
        .filter_map(|vehicle| {
            let kind = vehicle.kind?;
            if !matches!(
                kind,
                EntityKind::Minecart
                    | EntityKind::ChestMinecart
                    | EntityKind::FurnaceMinecart
                    | EntityKind::TntMinecart
                    | EntityKind::HopperMinecart
                    | EntityKind::CommandBlockMinecart
                    | EntityKind::SpawnerMinecart
            ) || vehicle.shared_flags & 0x20 != 0
            {
                return None;
            }
            let yaw = match (vehicle.prev_look_dir, vehicle.look_dir) {
                (Some(prev), Some(current)) => {
                    lerp_angle(prev.y_rot_deg(), current.y_rot_deg(), partial_tick)
                }
                (_, Some(current)) => current.y_rot_deg(),
                _ => 0.0,
            };
            let pitch = match (vehicle.prev_look_dir, vehicle.look_dir) {
                (Some(prev), Some(current)) => {
                    prev.x_rot_deg() + (current.x_rot_deg() - prev.x_rot_deg()) * partial_tick
                }
                (_, Some(current)) => current.x_rot_deg(),
                _ => 0.0,
            };
            let position = vehicle
                .prev_position
                .lerp(vehicle.position, partial_tick as f64);
            let hurt = (vehicle.boat_hurt_time as f32 - partial_tick).max(0.0);
            let rocking = if hurt > 0.0 {
                (hurt.sin() * hurt * vehicle.boat_damage.max(0.0) / 10.0
                    * vehicle.boat_hurt_direction as f32)
                    .to_radians()
            } else {
                0.0
            };
            Some(EntityRenderInfo {
                happy_ghast_equipment_layers: Vec::new(),
                position,
                simulation_position: vehicle.position,
                body_y_rot_deg: yaw,
                entity_kind: kind,
                head_x_rot_deg: pitch,
                body_transform: Some(
                    glam::Mat4::from_translation(glam::Vec3::Y * 0.375)
                        * glam::Mat4::from_rotation_z(-pitch.to_radians())
                        * glam::Mat4::from_rotation_x(rocking),
                ),
                ..Default::default()
            })
        })
        .collect()
}

fn arrow_render_infos(
    store: &crate::entity::EntityStore,
    partial_tick: f32,
) -> Vec<EntityRenderInfo> {
    store
        .vehicles
        .values()
        .filter_map(|entity| {
            let kind = entity.kind?;
            let variant_index = match kind {
                EntityKind::Arrow if entity.arrow_effect_color > 0 => 2,
                EntityKind::Arrow => 0,
                EntityKind::SpectralArrow => 1,
                _ => return None,
            };
            let look = entity.look_dir?;
            let pos = entity
                .projectile
                .as_ref()
                .map_or(entity.position, |p| p.position(partial_tick));
            let velocity = entity.projectile.as_ref().map_or(entity.velocity, |p| {
                if p.stopped { DVec3::ZERO } else { p.velocity }
            });
            let (yaw, pitch) = if velocity.length_squared() > 1.0e-8 {
                (
                    (velocity.x.atan2(velocity.z).to_degrees()) as f32,
                    (velocity.y.atan2(velocity.x.hypot(velocity.z)).to_degrees()) as f32,
                )
            } else {
                (look.y_rot_deg(), look.x_rot_deg())
            };
            Some(EntityRenderInfo {
                happy_ghast_equipment_layers: Vec::new(),
                position: pos,
                simulation_position: pos,
                body_y_rot_deg: yaw,
                head_x_rot_deg: pitch,
                entity_kind: EntityKind::Arrow,
                variant_index,
                base_tint: if kind == EntityKind::Arrow && entity.arrow_effect_color > 0 {
                    let rgb = entity.arrow_effect_color as u32;
                    [
                        ((rgb >> 16) & 0xff) as f32 / 255.0,
                        ((rgb >> 8) & 0xff) as f32 / 255.0,
                        (rgb & 0xff) as f32 / 255.0,
                        1.0,
                    ]
                } else {
                    [1.0; 4]
                },
                ..Default::default()
            })
        })
        .collect()
}

fn stack_render_count(count: i32) -> usize {
    if count <= 1 {
        1
    } else if count <= 16 {
        2
    } else if count <= 32 {
        3
    } else if count <= 48 {
        4
    } else {
        5
    }
}

fn get_entity_light(chunk_store: &ChunkStore, pos: Position) -> f32 {
    crate::renderer::chunk::mesher::world_brightness(
        chunk_store,
        pos.x.floor() as i32,
        pos.y.floor() as i32,
        pos.z.floor() as i32,
    )
}

/// Builds the rain/snow columns in a square around the camera (vanilla
/// WeatherEffectRenderer.extractRenderState). Returns empty when it is not
/// raining or when no precipitation biomes are nearby.
fn build_weather_columns(
    chunk_store: &ChunkStore,
    biome_climate: &HashMap<u32, BiomeClimate>,
    cam: glam::DVec3,
    rain: f32,
) -> Vec<crate::renderer::WeatherColumn> {
    use crate::renderer::WeatherColumn;
    use crate::renderer::pipelines::weather::{Precip, WEATHER_RADIUS, precipitation_for};

    if rain <= 0.0 {
        return Vec::new();
    }

    let cam_x = cam.x.floor() as i32;
    let cam_y = cam.y.floor() as i32;
    let cam_z = cam.z.floor() as i32;

    let mut columns = Vec::new();
    for dz in -WEATHER_RADIUS..=WEATHER_RADIUS {
        for dx in -WEATHER_RADIUS..=WEATHER_RADIUS {
            let wx = cam_x + dx;
            let wz = cam_z + dz;
            let terrain = chunk_store.motion_blocking_height(wx, wz);
            let y0 = (cam_y - WEATHER_RADIUS).max(terrain);
            let y1 = (cam_y + WEATHER_RADIUS).max(terrain);
            if y1 - y0 == 0 {
                continue;
            }
            let Some(biome_id) = chunk_store.biome_id_checked(wx, cam_y, wz) else {
                // Weather is diagnostic/visual only; do not invent biome 0 at
                // the view-distance edge while the chunk is still absent.
                continue;
            };
            let Some(climate) = biome_climate.get(&biome_id).copied() else {
                continue;
            };
            let precip = precipitation_for(&climate, cam_y);
            if precip == Precip::None {
                continue;
            }
            let light_y = cam_y.max(terrain);
            let light = get_entity_light(
                chunk_store,
                Position::new(wx as f64, light_y as f64, wz as f64),
            );
            columns.push(WeatherColumn {
                x: wx,
                z: wz,
                bottom_y: y0 as f32,
                top_y: y1 as f32,
                precip,
                light,
            });
        }
    }
    columns
}

fn item_stack_seed(item_id: u32, damage: i32) -> i64 {
    (item_id as i32).wrapping_add(damage) as i64
}

fn transform_item_bounds(
    min: glam::Vec3,
    max: glam::Vec3,
    transform: glam::Mat4,
) -> (glam::Vec3, glam::Vec3) {
    let mut out_min = glam::Vec3::splat(f32::INFINITY);
    let mut out_max = glam::Vec3::splat(f32::NEG_INFINITY);
    for x in [min.x, max.x] {
        for y in [min.y, max.y] {
            for z in [min.z, max.z] {
                let point = transform.transform_point3(glam::Vec3::new(x, y, z));
                out_min = out_min.min(point);
                out_max = out_max.max(point);
            }
        }
    }
    (out_min, out_max)
}

#[cfg(test)]
mod dropped_item_tests {
    use super::{
        item_frame_base_matrix, item_frame_item_matrix, item_stack_seed, transform_item_bounds,
    };

    #[test]
    fn dropped_item_copies_preserve_player_head_profile() {
        use azalea_inventory::components::{PartialOrFullProfile, PartialProfile, Profile};
        use azalea_inventory::{ItemStack, ItemStackData};
        use azalea_registry::builtin::{DataComponentKind, ItemKind};

        use crate::world::block_entity::{
            PlayerHeadProfileSource, PlayerHeadSkinPatch, player_head_profile_source_from_item,
        };

        let profile = Profile {
            unpack: Box::new(PartialOrFullProfile::Partial(PartialProfile {
                name: Some("Alex".into()),
                id: None,
                properties: Default::default(),
            })),
            skin_patch: Box::default(),
        };
        let mut data = ItemStackData::new(ItemKind::PlayerHead, 64);
        // SAFETY: Profile is inserted under its matching component kind.
        unsafe {
            data.component_patch
                .unchecked_insert_component(DataComponentKind::Profile, Some(profile.into()));
        }
        let source = player_head_profile_source_from_item(&ItemStack::Present(data));
        let expected = Some(PlayerHeadProfileSource::DynamicName {
            name: "Alex".into(),
            patch: PlayerHeadSkinPatch::default(),
        });
        assert_eq!(source, expected);
        let mut infos = Vec::new();
        super::emit_item_copies(
            &mut infos,
            "player_head",
            None,
            source,
            1,
            0,
            64,
            glam::Vec3::ZERO,
            0.0,
            0.0,
            0.0,
            false,
            false,
            glam::Mat4::IDENTITY,
            0.0,
            1.0,
            1.0,
            false,
            None,
            false,
            None,
            0.0,
            glam::DVec3::ZERO,
            64,
        );
        assert_eq!(infos.len(), super::stack_render_count(64) as usize);
        assert!(
            infos
                .iter()
                .all(|info| info.player_head_profile_source == expected)
        );
    }

    #[test]
    fn dropped_item_scatter_seed_includes_damage() {
        assert_eq!(item_stack_seed(42, 0), 42);
        assert_eq!(item_stack_seed(42, 7), 49);
        assert_eq!(item_stack_seed(u32::MAX, 2), 1);
    }

    #[test]
    fn item_frame_item_matrix_halves_fixed_scale_and_translation_but_not_body_or_map() {
        use azalea_core::direction::Direction as D;
        use glam::{DVec3, Mat4, Quat, Vec3};

        let attachment = DVec3::new(-16.5, 64.5, 31.5);
        let anchor = DVec3::new(-16.0, 64.0, 32.0);
        let translation = Vec3::new(2.0, -4.0, 6.0) / 16.0;
        let scale = Vec3::new(0.5, 0.75, 0.25);
        let fixed = Mat4::from_translation(translation)
            * Mat4::from_rotation_y(90_f32.to_radians())
            * Mat4::from_scale(scale);
        for (direction, normal) in [
            (D::Down, DVec3::NEG_Y),
            (D::Up, DVec3::Y),
            (D::North, DVec3::NEG_Z),
            (D::South, DVec3::Z),
            (D::West, DVec3::NEG_X),
            (D::East, DVec3::X),
        ] {
            let position = attachment - normal * 0.46875;
            // This is the production base matrix shared by body and map draws.
            let base = item_frame_base_matrix(position, direction, anchor);
            assert!(
                base.transform_point3(Vec3::ZERO)
                    .abs_diff_eq((attachment - anchor).as_vec3(), 1e-6)
            );
            assert!(
                base.transform_vector3(Vec3::Z)
                    .abs_diff_eq(-normal.as_vec3(), 1e-6)
            );
            for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
                assert!((base.transform_vector3(axis).length() - 1.0).abs() < 1e-6);
            }
            for rotation in 0..8 {
                let item = item_frame_item_matrix(base, rotation, fixed);
                let (sin, cos) = (rotation as f32 * 45.0).to_radians().sin_cos();
                let local_center = Vec3::new(
                    (translation.x * cos - translation.y * sin) * 0.5,
                    (translation.x * sin + translation.y * cos) * 0.5,
                    0.4375 + translation.z * 0.5,
                );
                assert!(
                    item.transform_point3(Vec3::ZERO)
                        .abs_diff_eq(base.transform_point3(local_center), 1e-6)
                );
                for (axis, length) in [(Vec3::X, scale.x), (Vec3::Y, scale.y), (Vec3::Z, scale.z)] {
                    assert!((item.transform_vector3(axis).length() - length * 0.5).abs() < 1e-6);
                }
                let plain = item_frame_item_matrix(base, rotation, Mat4::IDENTITY);
                assert!(
                    plain
                        .transform_point3(Vec3::ZERO)
                        .abs_diff_eq(base.transform_point3(Vec3::Z * 0.4375), 1e-6)
                );
                assert!((plain.transform_vector3(Vec3::X).length() - 0.5).abs() < 1e-6);
                let map_rotation = crate::renderer::pipelines::map_quad::frame_map_rotation(
                    Quat::from_mat4(&base),
                    rotation,
                );
                assert!(((map_rotation * Vec3::X).length() - 1.0).abs() < 1e-6);
                assert!(((map_rotation * Vec3::Y).length() - 1.0).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn transformed_bounds_follow_ground_display_transform() {
        let transform = glam::Mat4::from_translation(glam::Vec3::new(0.0, 3.0 / 16.0, 0.0))
            * glam::Mat4::from_scale(glam::Vec3::splat(0.5));
        let (min, max) =
            transform_item_bounds(glam::Vec3::splat(-0.5), glam::Vec3::splat(0.5), transform);

        assert!((min - glam::Vec3::new(-0.25, -0.0625, -0.25)).length() < 1.0e-6);
        assert!((max - glam::Vec3::new(0.25, 0.4375, 0.25)).length() < 1.0e-6);
    }
}

fn dropped_item_geometry(renderer: &Renderer, item_name: &str) -> (glam::Mat4, f32, f32) {
    let mesh = renderer.item_mesh_info(item_name);
    let is_block_model = mesh
        .map(|mesh| mesh.is_block_model)
        .unwrap_or_else(|| renderer.registry().get_item_model(item_name).is_some());
    let fallback_transform = if is_block_model {
        crate::world::block::model::default_block_ground_transform()
    } else {
        glam::Mat4::from_scale(glam::Vec3::splat(0.5))
    };
    let ground_transform = renderer
        .registry()
        .get_item_ground_transform(item_name)
        .unwrap_or(fallback_transform);
    let (bounds_min, bounds_max) = mesh
        .map(|mesh| (mesh.bounds_min, mesh.bounds_max))
        .unwrap_or_else(|| {
            if is_block_model {
                (glam::Vec3::splat(-0.5), glam::Vec3::splat(0.5))
            } else {
                (
                    glam::Vec3::new(-0.5, -0.5, -1.0 / 32.0),
                    glam::Vec3::new(0.5, 0.5, 1.0 / 32.0),
                )
            }
        });
    let (min, max) = transform_item_bounds(bounds_min, bounds_max, ground_transform);
    (ground_transform, min.y, max.z - min.z)
}

/// Emits the hovering, spinning, multi-copy cluster for one dropped item,
/// shared by resting items and the pickup fly-animation. `ground_transform`
/// is the model's resolved GROUND display transform; `min_y` and `z_size` are
/// the bounds after that transform, matching `ItemStackRenderState`.
#[allow(clippy::too_many_arguments)]
fn emit_item_copies(
    infos: &mut Vec<crate::renderer::pipelines::item_entity::ItemRenderInfo>,
    item_name: &str,
    raw_dye_rgb: Option<[u8; 3]>,
    player_head_profile_source: Option<crate::world::block_entity::PlayerHeadProfileSource>,
    item_id: u32,
    damage: i32,
    count: i32,
    anchor_rel_pos: glam::Vec3,
    age_f: f32,
    bob_offset: f32,
    actual_bob_offset: f32,
    controlled_phase: bool,
    bob_controlled: bool,
    ground_transform: glam::Mat4,
    min_y: f32,
    z_size: f32,
    light: f32,
    nether_lighting: bool,
    entity_uuid: Option<uuid::Uuid>,
    invisible: bool,
    actual_age: Option<u32>,
    actual_render_age: f32,
    world_position: glam::DVec3,
    stack_count: i32,
) {
    use crate::renderer::pipelines::item_entity::ItemRenderInfo;
    use crate::util::JavaRandom;

    let bob = (age_f / 10.0 + bob_offset).sin() * 0.1 + 0.1;
    let spin = age_f / 20.0 + bob_offset;
    let copies = stack_render_count(count);
    // hover = bob + (-modelBoundingBox.minY) + 0.0625
    let hover_y = bob - min_y + 0.0625;

    let base = glam::Mat4::from_translation(anchor_rel_pos + glam::Vec3::new(0.0, hover_y, 0.0))
        * glam::Mat4::from_rotation_y(spin);
    let mut push = |copy_offset: glam::Mat4| {
        infos.push(ItemRenderInfo {
            item_name: item_name.to_string(),
            raw_dye_rgb,
            player_head_profile_source: player_head_profile_source.clone(),
            model_matrix: base * copy_offset * ground_transform,
            light,
            white_overlay: 0.0,
            nether_lighting,
            entity_uuid,
            invisible,
            actual_age,
            actual_render_age,
            age_f,
            actual_spin: actual_render_age / 20.0 + actual_bob_offset,
            spin,
            bob_offset,
            actual_bob_offset,
            controlled_phase,
            bob_controlled,
            position: world_position.to_array(),
            stack_count,
        });
    };

    // ItemClusterRenderState.getSeedForItemStack: registry id + damage value.
    let mut rng = JavaRandom::new(item_stack_seed(item_id, damage));
    let mut jitter = |spread: f32| (rng.next_float() * 2.0 - 1.0) * spread;

    if z_size > 0.0625 {
        push(glam::Mat4::IDENTITY);
        for _ in 1..copies {
            let off = glam::Vec3::new(jitter(0.15), jitter(0.15), jitter(0.15));
            push(glam::Mat4::from_translation(off));
        }
    } else {
        let z_step = z_size * 1.5;
        let z_start = -(z_step * (copies - 1) as f32 / 2.0);
        push(glam::Mat4::from_translation(glam::Vec3::new(
            0.0, 0.0, z_start,
        )));
        for i in 1..copies {
            let z = z_start + z_step * i as f32;
            let off = glam::Vec3::new(jitter(0.15 * 0.5), jitter(0.15 * 0.5), z);
            push(glam::Mat4::from_translation(off));
        }
    }
}

fn item_frame_base_rotation(direction: azalea_core::direction::Direction) -> glam::Mat4 {
    use azalea_core::direction::Direction as D;
    match direction {
        D::North => glam::Mat4::IDENTITY,
        D::South => glam::Mat4::from_rotation_y(180_f32.to_radians()),
        D::East => glam::Mat4::from_rotation_y(-90_f32.to_radians()),
        D::West => glam::Mat4::from_rotation_y(90_f32.to_radians()),
        D::Up => {
            glam::Mat4::from_rotation_x(-90_f32.to_radians())
                * glam::Mat4::from_rotation_y(180_f32.to_radians())
        }
        D::Down => {
            glam::Mat4::from_rotation_x(90_f32.to_radians())
                * glam::Mat4::from_rotation_y(180_f32.to_radians())
        }
    }
}

fn item_frame_base_position(
    position: glam::DVec3,
    direction: azalea_core::direction::Direction,
) -> glam::DVec3 {
    let normal = match direction {
        azalea_core::direction::Direction::Down => glam::DVec3::NEG_Y,
        azalea_core::direction::Direction::Up => glam::DVec3::Y,
        azalea_core::direction::Direction::North => glam::DVec3::NEG_Z,
        azalea_core::direction::Direction::South => glam::DVec3::Z,
        azalea_core::direction::Direction::West => glam::DVec3::NEG_X,
        azalea_core::direction::Direction::East => glam::DVec3::X,
    };
    position + normal * 0.46875
}

fn item_frame_base_matrix(
    position: glam::DVec3,
    direction: azalea_core::direction::Direction,
    anchor: glam::DVec3,
) -> glam::Mat4 {
    glam::Mat4::from_translation((item_frame_base_position(position, direction) - anchor).as_vec3())
        * item_frame_base_rotation(direction)
}

fn item_frame_item_matrix(base: glam::Mat4, rotation: i32, fixed: glam::Mat4) -> glam::Mat4 {
    base * glam::Mat4::from_rotation_z((rotation as f32 * 45.0).to_radians())
        * glam::Mat4::from_translation(glam::Vec3::new(0.0, 0.0, 0.4375))
        // Vanilla's outer half-scale also scales the FIXED display translation.
        // Meshes are already centered; do not add another T(-0.5).
        * glam::Mat4::from_scale(glam::Vec3::splat(0.5))
        * fixed
}

fn projectile_render_infos(
    store: &crate::entity::EntityStore,
    partial_tick: f32,
    camera_yaw: f32,
    camera_pitch: f32,
    camera_orientation: glam::Quat,
) -> Vec<EntityRenderInfo> {
    use azalea_registry::builtin::EntityKind as K;
    store
        .vehicles
        .values()
        .filter_map(|vehicle| {
            let kind = vehicle.kind?;
            if vehicle.shared_flags & 0x20 != 0
                || !matches!(
                    kind,
                    K::DragonFireball
                        | K::Trident
                        | K::ShulkerBullet
                        | K::WitherSkull
                        | K::LlamaSpit
                )
            {
                return None;
            }
            let look = vehicle.look_dir?;
            let position = vehicle
                .prev_position
                .lerp(vehicle.position, partial_tick as f64);
            let (yaw, pitch) = if kind == K::DragonFireball {
                (camera_yaw, camera_pitch)
            } else if vehicle.velocity.length_squared() > 1.0e-8 {
                (
                    vehicle.velocity.x.atan2(vehicle.velocity.z).to_degrees() as f32,
                    vehicle
                        .velocity
                        .y
                        .atan2(vehicle.velocity.x.hypot(vehicle.velocity.z))
                        .to_degrees() as f32,
                )
            } else {
                (look.y_rot_deg(), look.x_rot_deg())
            };
            Some(EntityRenderInfo {
                happy_ghast_equipment_layers: Vec::new(),
                position,
                simulation_position: vehicle.position,
                entity_kind: kind,
                body_y_rot_deg: yaw,
                head_x_rot_deg: pitch,
                variant_index: u32::from(kind == K::WitherSkull && vehicle.projectile_dangerous),
                camera_orientation: (kind == K::DragonFireball).then_some(camera_orientation),
                projectile_foil: kind == K::Trident && vehicle.projectile_foil,
                overlay_tints: if kind == K::ShulkerBullet {
                    [Some([1.0, 1.0, 1.0, 0.15]), None, None, None]
                } else {
                    [None; crate::renderer::pipelines::entity_renderer::MAX_OVERLAYS]
                },
                age_in_ticks: vehicle.projectile_prev_age as f32
                    + (vehicle.projectile_age as f32 - vehicle.projectile_prev_age as f32)
                        * partial_tick,
                ..Default::default()
            })
        })
        .collect()
}

/// 26.2 ThrownItemRenderer and SnowballRenderer: camera-facing GROUND item
/// models at the interpolated display position, not dropped-item clusters.
#[allow(clippy::too_many_arguments)]
fn snowball_render_infos(
    entities: &crate::entity::EntityStore,
    partial_tick: f32,
    camera_pos: glam::DVec3,
    camera_pivot: glam::DVec3,
    anchor: glam::DVec3,
    view_scale: f32,
    camera_look: (f32, f32),
    frustum: &[[f32; 4]; 6],
    ground_transform: impl Fn(&str) -> Option<glam::Mat4>,
    nether_lighting: bool,
    light_at: impl Fn(Position) -> f32,
) -> Vec<crate::renderer::pipelines::item_entity::ItemRenderInfo> {
    use crate::renderer::pipelines::item_entity::ItemRenderInfo;

    let (yaw, pitch) = camera_look;
    let billboard = glam::Mat4::from_rotation_y((180.0 - yaw).to_radians())
        * glam::Mat4::from_rotation_x((-pitch).to_radians());
    entities
        .vehicles
        .values()
        .filter_map(|vehicle| {
            if vehicle.shared_flags & 0x20 != 0 {
                return None;
            }
            let (default_item_name, scale, full_bright) = match vehicle.kind? {
                azalea_registry::builtin::EntityKind::Snowball => ("snowball", 1.0, false),
                azalea_registry::builtin::EntityKind::Egg => ("egg", 1.0, false),
                azalea_registry::builtin::EntityKind::EnderPearl => ("ender_pearl", 1.0, false),
                azalea_registry::builtin::EntityKind::ExperienceBottle => {
                    ("experience_bottle", 1.0, false)
                }
                azalea_registry::builtin::EntityKind::Fireball => ("fire_charge", 3.0, true),
                azalea_registry::builtin::EntityKind::LingeringPotion => {
                    ("lingering_potion", 1.0, false)
                }
                azalea_registry::builtin::EntityKind::SmallFireball => ("fire_charge", 0.75, true),
                azalea_registry::builtin::EntityKind::SplashPotion => ("splash_potion", 1.0, false),
                _ => return None,
            };
            let item_name = match &vehicle.projectile_item {
                azalea_inventory::ItemStack::Present(stack) if !stack.is_empty() => {
                    crate::player::inventory::item_resource_name(stack.kind)
                }
                _ => default_item_name.to_owned(),
            };
            let ground_transform = ground_transform(&item_name)?;
            let raw_dye_rgb = match &vehicle.projectile_item {
                azalea_inventory::ItemStack::Present(stack) => stack
                    .get_component::<azalea_inventory::components::DyedColor>()
                    .map(|color| {
                        let rgb = color.rgb;
                        [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8]
                    }),
                azalea_inventory::ItemStack::Empty => None,
            };
            let pos = *vehicle
                .projectile
                .as_ref()
                .map_or(vehicle.position, |p| p.position(partial_tick));
            // Snowball is 0.25 cubed. EntityRenderDispatcher inflates the
            // culling AABB by 0.5 on each side; distance uses getSize * 64.
            if !crate::renderer::entity_distance_visible(pos, camera_pos, 0.25 * 64.0, view_scale) {
                return None;
            }
            let center = (pos + glam::DVec3::Y * 0.125 - camera_pivot).as_vec3();
            if frustum.iter().any(|p| {
                p[0] * center.x + p[1] * center.y + p[2] * center.z + p[3]
                    < -0.625 * (p[0].abs() + p[1].abs() + p[2].abs())
            }) {
                return None;
            }
            Some(ItemRenderInfo {
                item_name,
                raw_dye_rgb,
                player_head_profile_source: None,
                model_matrix: glam::Mat4::from_translation((pos - anchor).as_vec3())
                    * billboard
                    * glam::Mat4::from_scale(glam::Vec3::splat(scale))
                    * ground_transform,
                light: if full_bright {
                    1.0
                } else {
                    light_at(pos.into())
                },
                white_overlay: 0.0,
                nether_lighting,
                entity_uuid: None,
                invisible: false,
                actual_age: None,
                actual_render_age: 0.0,
                age_f: 0.0,
                actual_spin: 0.0,
                spin: 0.0,
                bob_offset: 0.0,
                actual_bob_offset: 0.0,
                controlled_phase: false,
                bob_controlled: false,
                position: pos.to_array(),
                stack_count: 1,
            })
        })
        .collect()
}

fn build_item_render_infos(
    entity_store: &crate::entity::ItemEntityStore,
    entities: &crate::entity::EntityStore,
    chunk_store: &ChunkStore,
    renderer: &Renderer,
    cardinal_light: CardinalLightType,
    camera_pos: glam::DVec3,
    anchor: glam::DVec3,
    partial_tick: f32,
    age_partial_tick: f32,
    entity_view_scale: f32,
) -> Vec<crate::renderer::pipelines::item_entity::ItemRenderInfo> {
    let mut infos = Vec::new();
    let nether_lighting = cardinal_light == CardinalLightType::Nether;
    // ItemEntity is 0.25 x 0.25: AABB getSize = 0.25, not 64 blocks.
    // Keep the existing store filter only as a broad phase at high settings.
    for item in
        entity_store.visible_items(camera_pos, (16.0 * f64::from(entity_view_scale)).max(64.0))
    {
        if !crate::renderer::entity_distance_visible(
            *item.position,
            camera_pos,
            16.0,
            entity_view_scale,
        ) {
            continue;
        }
        let actual_age_f = item.age as f32 + age_partial_tick;
        let actual_bob_offset = item.bob_offset;
        let target_trace = std::env::var_os("POMME_ITEM_ENTITY_TRACE").is_some()
            && std::env::var("POMME_DROP_TARGET_UUID")
                .is_ok_and(|target| target == item.uuid.to_string());
        let phase_age = std::env::var("POMME_DROP_PHASE_AGE")
            .ok()
            .and_then(|v| v.parse::<f32>().ok());
        let phase_bob = std::env::var("POMME_DROP_PHASE_BOB_OFFSET")
            .ok()
            .and_then(|v| v.parse::<f32>().ok());
        let bob_input = std::env::var("POMME_DROP_BOB_OFFSET")
            .ok()
            .and_then(|v| v.parse::<f32>().ok());
        let controlled_phase = target_trace && phase_age.is_some() && phase_bob.is_some();
        let bob_controlled = target_trace && bob_input.is_some();
        let age_f = if controlled_phase {
            phase_age.unwrap_or(actual_age_f)
        } else {
            actual_age_f
        };
        let bob_offset = if bob_controlled {
            bob_input.unwrap_or(actual_bob_offset)
        } else if controlled_phase {
            phase_bob.unwrap_or(actual_bob_offset)
        } else {
            actual_bob_offset
        };
        let lerped = item.prev_position.lerp(item.position, partial_tick as f64);
        let light = get_entity_light(chunk_store, lerped);
        let (ground_transform, min_y, z_size) = dropped_item_geometry(renderer, &item.item_name);
        emit_item_copies(
            &mut infos,
            &item.item_name,
            item.stack
                .as_ref()
                .and_then(|stack| stack.get_component::<azalea_inventory::components::DyedColor>())
                .map(|color| {
                    let rgb = color.rgb;
                    [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8]
                }),
            item.stack.as_ref().and_then(|stack| {
                crate::world::block_entity::player_head_profile_source_from_item(
                    &azalea_inventory::ItemStack::Present(stack.clone()),
                )
            }),
            item.item_id,
            item.damage,
            item.count,
            (*lerped - anchor).as_vec3(),
            age_f,
            bob_offset,
            actual_bob_offset,
            controlled_phase,
            bob_controlled,
            ground_transform,
            min_y,
            z_size,
            light,
            nether_lighting,
            Some(item.uuid),
            item.invisible,
            Some(item.age),
            actual_age_f,
            *lerped,
            item.count,
        );
    }

    // Pickup fly-animation: the cluster at the lerped position, age frozen at
    // pickup.
    for pickup in entity_store.active_pickups(partial_tick) {
        let age_f = pickup.age as f32 + partial_tick;
        let light = get_entity_light(chunk_store, pickup.position);
        let (ground_transform, min_y, z_size) = dropped_item_geometry(renderer, &pickup.item_name);
        emit_item_copies(
            &mut infos,
            &pickup.item_name,
            pickup
                .stack
                .as_ref()
                .and_then(|stack| stack.get_component::<azalea_inventory::components::DyedColor>())
                .map(|color| {
                    let rgb = color.rgb;
                    [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8]
                }),
            pickup.stack.as_ref().and_then(|stack| {
                crate::world::block_entity::player_head_profile_source_from_item(
                    &azalea_inventory::ItemStack::Present(stack.clone()),
                )
            }),
            pickup.item_id,
            pickup.damage,
            pickup.count,
            (*pickup.position - anchor).as_vec3(),
            age_f,
            pickup.bob_offset,
            pickup.bob_offset,
            false,
            false,
            ground_transform,
            min_y,
            z_size,
            light,
            nether_lighting,
            None,
            false,
            None,
            age_f,
            *pickup.position,
            pickup.count,
        );
    }

    // Thrown item billboards use the existing item mesh/atlas path, but not
    // dropped-item bobbing, stack copies, or spin.
    infos.extend(snowball_render_infos(
        entities,
        partial_tick,
        camera_pos,
        *renderer.camera_pivot_position(),
        anchor,
        entity_view_scale,
        renderer.camera_effective_look_deg(),
        &renderer.frustum_planes(),
        |name| {
            renderer
                .item_mesh_info(name)
                .map(|_| dropped_item_geometry(renderer, name).0)
        },
        nether_lighting,
        |pos| get_entity_light(chunk_store, pos),
    ));

    // Frame bodies use the baked block/item_frame or block/glow_item_frame
    // model. The held stack is sent through the existing item mesh path; map
    // contents still need the dedicated dynamic-map renderer.
    for frame in entities.vehicles.values().filter(|entity| {
        matches!(
            entity.kind,
            Some(azalea_registry::builtin::EntityKind::ItemFrame)
                | Some(azalea_registry::builtin::EntityKind::GlowItemFrame)
        )
    }) {
        if !crate::renderer::entity_distance_visible(
            *frame.position,
            camera_pos,
            1024.0,
            entity_view_scale,
        ) {
            continue;
        }
        let Some(kind) = frame.kind else { continue };
        let has_map = matches!(
            &frame.item_frame_item,
            azalea_inventory::ItemStack::Present(stack)
                if !stack.is_empty()
                    && stack.get_component::<azalea_inventory::components::MapId>().is_some()
        );
        let item_name = match (
            kind == azalea_registry::builtin::EntityKind::GlowItemFrame,
            has_map,
        ) {
            (false, false) => "pomme:item_frame_body",
            (true, false) => "pomme:glow_item_frame_body",
            (false, true) => "pomme:item_frame_map_body",
            (true, true) => "pomme:glow_item_frame_map_body",
        };
        let position = *frame.position;
        // ItemFrame metadata direction is authoritative; entity yaw doesn't
        // represent floor/ceiling mounting and is not used to infer facing.
        let direction = frame
            .item_frame_direction
            .unwrap_or(azalea_core::direction::Direction::South);
        let base_matrix = item_frame_base_matrix(position, direction, anchor);
        // build_item_mesh already centers baked model vertices by subtracting 0.5.
        infos.push(crate::renderer::pipelines::item_entity::ItemRenderInfo {
            item_name: item_name.to_owned(),
            raw_dye_rgb: None,
            player_head_profile_source: None,
            model_matrix: base_matrix,
            light: get_entity_light(
                chunk_store,
                Position::new(position.x, position.y, position.z),
            ),
            white_overlay: 0.0,
            nether_lighting,
            entity_uuid: None,
            invisible: false,
            actual_age: None,
            actual_render_age: 0.0,
            age_f: 0.0,
            actual_spin: 0.0,
            spin: 0.0,
            bob_offset: 0.0,
            actual_bob_offset: 0.0,
            controlled_phase: false,
            bob_controlled: false,
            position: position.to_array(),
            stack_count: 1,
        });

        if let azalea_inventory::ItemStack::Present(stack) = &frame.item_frame_item
            && !stack.is_empty()
            && stack
                .get_component::<azalea_inventory::components::MapId>()
                .is_none()
        {
            let name = crate::player::inventory::item_resource_name(stack.kind);
            let frame_item_transform = item_frame_item_matrix(
                base_matrix,
                frame.item_frame_rotation,
                renderer
                    .registry()
                    .get_item_fixed_transform(&name)
                    .unwrap_or(glam::Mat4::IDENTITY),
            );
            let raw_dye_rgb = stack
                .get_component::<azalea_inventory::components::DyedColor>()
                .map(|color| {
                    let rgb = color.rgb;
                    [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8]
                });
            infos.push(crate::renderer::pipelines::item_entity::ItemRenderInfo {
                item_name: name,
                raw_dye_rgb,
                player_head_profile_source:
                    crate::world::block_entity::player_head_profile_source_from_item(
                        &frame.item_frame_item,
                    ),
                model_matrix: frame_item_transform,
                light: get_entity_light(
                    chunk_store,
                    Position::new(position.x, position.y, position.z),
                ),
                white_overlay: 0.0,
                nether_lighting,
                entity_uuid: None,
                invisible: false,
                actual_age: None,
                actual_render_age: 0.0,
                age_f: 0.0,
                actual_spin: 0.0,
                spin: 0.0,
                bob_offset: 0.0,
                actual_bob_offset: 0.0,
                controlled_phase: false,
                bob_controlled: false,
                position: position.to_array(),
                stack_count: stack.count,
            });
        }
    }

    infos
}

#[derive(Default)]
struct EntityExtras {
    variant_index: u32,
    overlay_tints: [Option<[f32; 4]>; MAX_OVERLAYS],
    overlay_variants: [u32; MAX_OVERLAYS],
    head_y_offset: f32,
    head_x_rot_deg_override: Option<f32>,
    flap: f32,
    flap_speed: f32,
    body_transform: Option<glam::Mat4>,
    render_offset: glam::DVec3,
    nose_wobble_speed: f32,
    is_angry: bool,
    tail_angle: f32,
    head_roll_angle: f32,
    shake_anim: f32,
    lie_down_amount: f32,
    lie_down_amount_tail: f32,
    relax_state_one_amount: f32,
    hop_elapsed_secs: Option<f32>,
    /// Base-model tint (wolf wet shade, glow squid dimming, tropical fish
    /// base dye); `None` = white.
    base_tint: Option<[f32; 4]>,
    eat_anim: f32,
    stand_anim: f32,
    feeding_anim: f32,
    animate_tail: bool,
    tentacle_angle: f32,
    bat_elapsed_secs: Option<f32>,
    golem_attack_ticks: f32,
    golem_offer_flower_ticks: u32,
    vex_charging: bool,
    animation_phase: f32,
}

/// Only the first overlay slot visible, untinted.
const SLOT0_TINTS: [Option<[f32; 4]>; MAX_OVERLAYS] = {
    let mut tints = [None; MAX_OVERLAYS];
    tints[0] = Some(WHITE_TINT);
    tints
};

/// Slot-0 overlay picked by a 1-based id (0 draws nothing), as
/// (`overlay_tints`, `overlay_variants`).
fn slot0_overlay(id: u32) -> ([Option<[f32; 4]>; MAX_OVERLAYS], [u32; MAX_OVERLAYS]) {
    let tints = if id != 0 {
        SLOT0_TINTS
    } else {
        [None; MAX_OVERLAYS]
    };
    (tints, [id.saturating_sub(1), 0, 0, 0])
}

fn entity_extras(
    entity_id: i32,
    e: &crate::entity::LivingEntity,
    alpha: f32,
    game_time: i64,
) -> EntityExtras {
    match e.entity_type {
        EntityKind::Cow | EntityKind::Pig | EntityKind::Axolotl => EntityExtras {
            variant_index: e.variant,
            ..Default::default()
        },
        EntityKind::Bee => EntityExtras {
            variant_index: (if e.anger_end_time > game_time { 1 } else { 0 })
                + (if e.bee_flags & 0x08 != 0 { 2 } else { 0 }),
            ..Default::default()
        },
        EntityKind::Ghast => EntityExtras {
            variant_index: e.ghast_charging as u32,
            ..Default::default()
        },
        EntityKind::Vex => EntityExtras {
            variant_index: e.vex_charging as u32,
            vex_charging: e.vex_charging,
            ..Default::default()
        },
        EntityKind::Mooshroom | EntityKind::Shulker => EntityExtras {
            variant_index: e.variant,
            ..Default::default()
        },
        EntityKind::MagmaCube => EntityExtras {
            body_transform: Some(slime_body_transform(e, alpha)),
            ..Default::default()
        },
        EntityKind::Phantom => EntityExtras {
            overlay_tints: SLOT0_TINTS,
            animation_phase: (entity_id * 3) as f32,
            body_transform: Some(glam::Mat4::from_scale(glam::Vec3::splat(
                1.0 + e.phantom_size as f32 * 0.15,
            ))),
            ..Default::default()
        },
        EntityKind::Breeze => EntityExtras {
            overlay_tints: [Some(WHITE_TINT), Some(WHITE_TINT), None, None],
            ..Default::default()
        },
        EntityKind::SulfurCube => {
            let small = e.sulfur_cube_size <= 1;
            let mut overlay_tints = [None; MAX_OVERLAYS];
            overlay_tints[usize::from(small)] = Some(WHITE_TINT);
            EntityExtras {
                variant_index: u32::from(small),
                overlay_tints,
                body_transform: Some(slime_body_transform(e, alpha)),
                ..Default::default()
            }
        }
        EntityKind::Wither => {
            let invulnerable = (e.wither_invulnerability as f32 - alpha).max(0.0);
            let tick = invulnerable.floor() as i32;
            let flashing = tick > 0 && (tick > 80 || tick / 5 % 2 != 1);
            let scale = 2.0 - invulnerable / 220.0 * 0.5;
            EntityExtras {
                variant_index: u32::from(flashing),
                overlay_tints: if e.wither_invulnerability > 0 {
                    SLOT0_TINTS
                } else {
                    [None; MAX_OVERLAYS]
                },
                body_transform: Some(glam::Mat4::from_scale(glam::Vec3::splat(scale))),
                ..Default::default()
            }
        }
        EntityKind::Chicken => EntityExtras {
            variant_index: e.variant,
            flap: e.prev_flap.lerp(e.flap, alpha),
            flap_speed: e.prev_flap_speed.lerp(e.flap_speed, alpha),
            ..Default::default()
        },
        EntityKind::Sheep => sheep_extras(entity_id, e, alpha),
        EntityKind::Villager => villager_like_extras(e, &VILLAGER_TYPE_HAT),
        EntityKind::ZombieVillager => villager_like_extras(e, &ZOMBIE_VILLAGER_TYPE_HAT),
        EntityKind::Bogged => EntityExtras {
            overlay_tints: SLOT0_TINTS,
            variant_index: e.is_sheared as u32,
            ..Default::default()
        },
        // Always-visible slot-0 overlay (spider eyes, drowned/stray clothing).
        EntityKind::Spider | EntityKind::Drowned | EntityKind::Stray => EntityExtras {
            overlay_tints: SLOT0_TINTS,
            ..Default::default()
        },
        EntityKind::Enderman => EntityExtras {
            overlay_tints: SLOT0_TINTS,
            // Vanilla `EndermanRenderer.getRenderOffset`: per-frame gaussian
            // x/z shake while screaming.
            render_offset: if e.is_creepy {
                glam::DVec3::new(
                    crate::particle::next_gaussian() * 0.02,
                    0.0,
                    crate::particle::next_gaussian() * 0.02,
                )
            } else {
                glam::DVec3::ZERO
            },
            ..Default::default()
        },
        EntityKind::Slime => EntityExtras {
            overlay_tints: SLOT0_TINTS,
            body_transform: Some(slime_body_transform(e, alpha)),
            ..Default::default()
        },
        EntityKind::Witch => EntityExtras {
            nose_wobble_speed: 0.01 * (entity_id % 10) as f32,
            ..Default::default()
        },
        EntityKind::Wolf => wolf_extras(e, alpha, game_time),
        EntityKind::Cat => cat_extras(e, alpha),
        EntityKind::Horse => {
            // Markings overlay; id 0 = NONE. Slot 1 is the saddle equipment layer.
            let (mut overlay_tints, overlay_variants) = slot0_overlay((e.variant >> 8) & 0xFF);
            overlay_tints[1] = e.saddled.then_some(WHITE_TINT);
            EntityExtras {
                variant_index: e.variant & 0xFF,
                overlay_tints,
                overlay_variants,
                ..equine_extras(e, alpha)
            }
        }
        EntityKind::Donkey | EntityKind::Mule => EntityExtras {
            variant_index: e.has_chest as u32,
            ..equine_extras(e, alpha)
        },
        EntityKind::SkeletonHorse | EntityKind::ZombieHorse => equine_extras(e, alpha),
        EntityKind::Squid | EntityKind::GlowSquid => squid_extras(e, alpha),
        EntityKind::Bat => EntityExtras {
            bat_elapsed_secs: e.bat_anim_start.map(|s| anim_clock_secs(e, s, alpha)),
            ..Default::default()
        },
        EntityKind::Cod
        | EntityKind::Salmon
        | EntityKind::TropicalFish
        | EntityKind::Pufferfish => fish_extras(e, alpha),
        EntityKind::IronGolem => golem_extras(e, alpha),
        EntityKind::Rabbit => EntityExtras {
            // "Toast" overrides the variant texture (slot 7).
            variant_index: if e.custom_name.as_deref() == Some("Toast") {
                7
            } else {
                e.variant
            },
            hop_elapsed_secs: e.hop_anim_start.map(|s| anim_clock_secs(e, s, alpha)),
            ..Default::default()
        },
        // Charged-creeper aura overlay (slot 0) only when powered.
        EntityKind::Creeper if e.powered => EntityExtras {
            overlay_tints: SLOT0_TINTS,
            ..Default::default()
        },
        _ => EntityExtras::default(),
    }
}

/// Seconds on a vanilla `AnimationState` clock started at tick `start`
/// (clocks start one tick ahead of the current age, so clamp at 0).
fn anim_clock_secs(e: &crate::entity::LivingEntity, start: u32, alpha: f32) -> f32 {
    (e.age_in_ticks as f32 - start as f32 + alpha).max(0.0) * 0.05
}

/// Vanilla `AbstractCubeMobRenderer.applySizeAndSquish` plus the slime-only
/// `downscaleSlightly` (0.999 shrink + a 0.001 drop that tucks the inner body
/// under the shell surface; vanilla's +0.001 is in flipped space = down).
fn slime_body_transform(e: &crate::entity::LivingEntity, alpha: f32) -> glam::Mat4 {
    let squish = e.prev_squish + (e.squish - e.prev_squish) * alpha;
    let size = e.slime_size as f32;
    let ss = squish / (size * 0.5 + 1.0);
    let w = 1.0 / (ss + 1.0);
    glam::Mat4::from_scale(glam::Vec3::splat(0.999))
        * glam::Mat4::from_translation(glam::Vec3::new(0.0, -0.001, 0.0))
        * glam::Mat4::from_scale(glam::Vec3::new(w * size, size / w, w * size))
}

/// Iron golem: `IronGolemRenderer.setupRotations` body sway, the punch /
/// flower countdowns, and the `IronGolemCrackinessLayer` health overlay.
fn golem_extras(e: &crate::entity::LivingEntity, alpha: f32) -> EntityExtras {
    // `Crackiness.GOLEM` thresholds over max health 100 (attributes aren't
    // parsed; vanilla never modifies the golem's).
    let crack_level = match e.health / 100.0 {
        f if f < 0.25 => 3,
        f if f < 0.5 => 2,
        f if f < 0.75 => 1,
        _ => 0,
    };
    let (overlay_tints, overlay_variants) = slot0_overlay(crack_level);
    EntityExtras {
        golem_attack_ticks: if e.golem_attack_ticks > 0 {
            e.golem_attack_ticks as f32 - alpha
        } else {
            0.0
        },
        golem_offer_flower_ticks: e.golem_offer_flower_ticks as u32,
        // +-6.5 degree roll in step with the walk cycle.
        body_transform: (e.walk_speed(alpha) >= 0.01).then(|| {
            let sway = 6.5 * triangle_wave(e.walk_pos(alpha) + 6.0, 13.0);
            glam::Mat4::from_rotation_z(sway.to_radians())
        }),
        overlay_tints,
        overlay_variants,
        ..Default::default()
    }
}

/// Squid tentacle stroke + the `SquidRenderer.setupRotations` body pitch and
/// axial spin; glow squid adds the post-hurt dimming.
fn squid_extras(e: &crate::entity::LivingEntity, alpha: f32) -> EntityExtras {
    let x_rot = e.prev_x_body_rot + (e.x_body_rot - e.prev_x_body_rot) * alpha;
    // z_body_rot grows without bound; wrap only here, after the lerp.
    let z_rot = (e.prev_z_body_rot + (e.z_body_rot - e.prev_z_body_rot) * alpha).rem_euclid(360.0);
    let (up, down) = if e.is_baby { (0.25, -0.6) } else { (0.5, -1.2) };
    // Approximation: vanilla drops the glow light level while dark and lets
    // ambient light take over; pomme's entity pipeline is unlit, so darken
    // the tint instead.
    let base_tint = (e.entity_type == EntityKind::GlowSquid).then(|| {
        let k = (1.0 - e.dark_ticks as f32 / 10.0).clamp(0.0, 1.0);
        [k, k, k, 1.0]
    });
    EntityExtras {
        tentacle_angle: e.prev_tentacle_angle + (e.tentacle_angle - e.prev_tentacle_angle) * alpha,
        // The axial spin is applied about Y, after the pitch (vanilla).
        body_transform: Some(
            glam::Mat4::from_translation(glam::Vec3::new(0.0, up, 0.0))
                * glam::Mat4::from_rotation_x(x_rot.to_radians())
                * glam::Mat4::from_rotation_y(z_rot.to_radians())
                * glam::Mat4::from_translation(glam::Vec3::new(0.0, down, 0.0)),
        ),
        base_tint,
        ..Default::default()
    }
}

/// The four fish renderers' `setupRotations`: body wobble about Y, the
/// on-land 90 degree flop roll, and the pufferfish bob; plus per-kind variant
/// and tint selection.
fn fish_extras(e: &crate::entity::LivingEntity, alpha: f32) -> EntityExtras {
    use std::f32::consts::FRAC_PI_2;
    let age = e.age_in_ticks as f32 + alpha;
    if e.entity_type == EntityKind::Pufferfish {
        return EntityExtras {
            variant_index: e.puff_state as u32,
            render_offset: glam::DVec3::new(0.0, ((age * 0.05).cos() * 0.08) as f64, 0.0),
            ..Default::default()
        };
    }
    // Only the salmon scales its wobble when out of water.
    let (amp, ang) = if e.entity_type == EntityKind::Salmon && !e.is_in_water {
        (1.3, 1.7)
    } else {
        (1.0, 1.0)
    };
    let wobble = (amp * 4.3 * (ang * 0.6 * age).sin()).to_radians();
    let mut m = glam::Mat4::from_rotation_y(wobble);
    if !e.is_in_water {
        let t = if e.entity_type == EntityKind::Cod {
            glam::Vec3::new(0.1, 0.1, -0.1)
        } else {
            glam::Vec3::new(0.2, 0.1, 0.0)
        };
        m *= glam::Mat4::from_translation(t) * glam::Mat4::from_rotation_z(FRAC_PI_2);
    }
    let mut extras = EntityExtras {
        body_transform: Some(m),
        ..Default::default()
    };
    match e.entity_type {
        EntityKind::Salmon => extras.variant_index = e.variant,
        EntityKind::TropicalFish => {
            // Packed variant: b0 shape, b1 pattern, b2 base dye, b3 pattern
            // dye. An unknown shape/pattern pair falls back to KOB (small,
            // pattern 0) like vanilla's sparse id map.
            let v = e.variant as i32;
            let (shape, pattern) = match ((v & 0xFF) as usize, ((v >> 8) & 0xFF) as u32) {
                (shape @ 0..=1, pattern @ 0..=5) => (shape, pattern),
                _ => (0, 0),
            };
            extras.variant_index = shape as u32;
            extras.base_tint = Some(dye_color_tint(((v >> 16) & 0xFF) as u8));
            extras.overlay_tints[shape] = Some(dye_color_tint(((v >> 24) & 0xFF) as u8));
            extras.overlay_variants = [pattern, pattern, 0, 0];
        }
        _ => {}
    }
    extras
}

fn equine_extras(e: &crate::entity::LivingEntity, alpha: f32) -> EntityExtras {
    EntityExtras {
        eat_anim: e.prev_eat_anim + (e.eat_anim - e.prev_eat_anim) * alpha,
        stand_anim: e.prev_stand_anim + (e.stand_anim - e.prev_stand_anim) * alpha,
        feeding_anim: e.prev_mouth_anim + (e.mouth_anim - e.prev_mouth_anim) * alpha,
        animate_tail: e.tail_swishing(),
        ..Default::default()
    }
}

/// Wolf texture state (`variant_index = variant * 3 + state`, tame > angry >
/// wild priority), collar tint, tail angle, and the beg/shake/wet values.
fn wolf_extras(e: &crate::entity::LivingEntity, alpha: f32, game_time: i64) -> EntityExtras {
    use std::f32::consts::PI;
    let is_angry = e.anger_end_time > 0 && e.anger_end_time > game_time;
    let state = if e.is_tame {
        1
    } else if is_angry {
        2
    } else {
        0
    };
    let tail_angle = if is_angry {
        1.5393804
    } else if e.is_tame {
        // Tame wolves carry their health in the tail; tame max health is a
        // fixed 40 (`applyTamingSideEffects`), attributes aren't parsed.
        let max_health = 40.0;
        (0.55 - (max_health - e.health) / max_health * 0.4) * PI
    } else {
        0.62831855
    };
    let mut overlay_tints = [None; MAX_OVERLAYS];
    if e.is_tame {
        overlay_tints[0] = Some(dye_color_tint(e.collar_color));
    }
    let wet = e.wet_shade(alpha);
    EntityExtras {
        variant_index: e.variant * 3 + state,
        overlay_tints,
        is_angry,
        tail_angle,
        head_roll_angle: (e.prev_interested_angle
            + (e.interested_angle - e.prev_interested_angle) * alpha)
            * 0.15
            * PI,
        shake_anim: e.prev_shake_anim + (e.shake_anim - e.prev_shake_anim) * alpha,
        base_tint: Some([wet, wet, wet, 1.0]),
        ..Default::default()
    }
}

/// Cat collar, pose springs, and the lie-down whole-body roll (vanilla
/// `CatRenderer.setupRotations`).
// TODO: the extra 0.15 offset while lying on a sleeping player.
fn cat_extras(e: &crate::entity::LivingEntity, alpha: f32) -> EntityExtras {
    let mut overlay_tints = [None; MAX_OVERLAYS];
    if e.is_tame {
        overlay_tints[0] = Some(dye_color_tint(e.collar_color));
    }
    let lie = e.prev_lie_down_amount + (e.lie_down_amount - e.prev_lie_down_amount) * alpha;
    let body_transform = (lie > 0.0).then(|| {
        glam::Mat4::from_translation(glam::Vec3::new(0.4 * lie, 0.15 * lie, 0.1 * lie))
            * glam::Mat4::from_rotation_z((90.0 * lie).to_radians())
    });
    EntityExtras {
        variant_index: e.variant,
        overlay_tints,
        lie_down_amount: lie,
        lie_down_amount_tail: e.prev_lie_down_amount_tail
            + (e.lie_down_amount_tail - e.prev_lie_down_amount_tail) * alpha,
        relax_state_one_amount: e.prev_relax_state_one_amount
            + (e.relax_state_one_amount - e.prev_relax_state_one_amount) * alpha,
        body_transform,
        ..Default::default()
    }
}

fn sheep_extras(entity_id: i32, e: &crate::entity::LivingEntity, alpha: f32) -> EntityExtras {
    let is_jeb = e.custom_name.as_deref() == Some("jeb_");
    let tint = if is_jeb {
        jeb_sheep_tint(entity_id, e.age_in_ticks)
    } else if let Some(c) = e.wool_color {
        wool_color_tint(c)
    } else {
        WHITE_TINT
    };

    let mut overlay_tints = [None; MAX_OVERLAYS];
    if !e.is_sheared {
        if e.is_baby {
            overlay_tints[0] = Some(tint);
        } else {
            let undercoat_visible = is_jeb || e.wool_color.is_some_and(|c| c != 0);
            overlay_tints[0] = if undercoat_visible { Some(tint) } else { None };
            overlay_tints[1] = Some(tint);
        }
    }

    let (pos_scale, angle_scale) = sheep_eat_scales(e.eat_anim_tick, e.prev_eat_anim_tick, alpha);
    let age_scale = if e.is_baby { 0.5 } else { 1.0 };
    let head_y_offset = pos_scale * 9.0 * age_scale;
    let head_x_rot_deg_override = if e.eat_anim_tick > 0 || e.prev_eat_anim_tick > 0 {
        Some(angle_scale)
    } else {
        None
    };

    EntityExtras {
        overlay_tints,
        head_y_offset,
        head_x_rot_deg_override,
        ..Default::default()
    }
}

/// Whether the type texture's built-in hat is fully or partially covered by
/// the profession texture's own hat, per the `villager` sections of the
/// `.png.mcmeta` files under `textures/entity/villager/` (hardcoded — no
/// resource-pack support). 0 = none, 1 = partial, 2 = full.
const VILLAGER_TYPE_HAT: [u8; 7] = [2, 0, 0, 0, 2, 0, 0]; // desert, snow = full
// `zombie_villager/type/` ships no `.mcmeta` files at all.
const ZOMBIE_VILLAGER_TYPE_HAT: [u8; 7] = [0; 7];
const VILLAGER_PROFESSION_HAT: [u8; 15] = [
    0, // none
    0, // armorer
    1, // butcher (partial)
    0, // cartographer
    0, // cleric
    2, // farmer
    2, // fisherman
    2, // fletcher
    0, // leatherworker
    2, // librarian
    0, // mason
    0, // nitwit
    2, // shepherd
    0, // toolsmith
    0, // weaponsmith
];

/// Overlay slots: 0 = biome type (full model), 1 = biome type (no-hat model),
/// 2 = profession, 3 = profession level. Mirrors vanilla
/// `VillagerProfessionLayer.submit`, shared by villager and zombie villager
/// (which differ only in their type-hat `.mcmeta` tables).
fn villager_like_extras(e: &crate::entity::LivingEntity, type_hat_table: &[u8; 7]) -> EntityExtras {
    use crate::entity::villager::VillagerProfession;

    let kind = e.villager_kind as usize;
    let profession = e.villager_profession as usize;

    let type_hat = type_hat_table[kind];
    let prof_hat = VILLAGER_PROFESSION_HAT[profession];
    let type_hat_visible = prof_hat == 0 || (prof_hat == 1 && type_hat != 2);

    let mut overlay_tints = [None; MAX_OVERLAYS];
    overlay_tints[if type_hat_visible { 0 } else { 1 }] = Some(WHITE_TINT);
    // Profession and level layers are adult-only; nitwits have no level badge.
    if !e.is_baby && e.villager_profession != VillagerProfession::None {
        overlay_tints[2] = Some(WHITE_TINT);
        if e.villager_profession != VillagerProfession::Nitwit {
            overlay_tints[3] = Some(WHITE_TINT);
        }
    }

    EntityExtras {
        overlay_tints,
        overlay_variants: [
            kind as u32,
            kind as u32,
            (profession as u32).saturating_sub(1),
            e.villager_level.clamp(1, 5) - 1,
        ],
        ..Default::default()
    }
}

fn sheep_eat_scales(eat_tick: u8, prev_eat_tick: u8, alpha: f32) -> (f32, f32) {
    use std::f32::consts::PI;

    // Mirrors vanilla Sheep.java:127-149. Linear-blend previous and current tick
    // first so the head dip is smooth between server ticks.
    let interp = prev_eat_tick as f32 + (eat_tick as f32 - prev_eat_tick as f32) * alpha;
    let pos_scale = if interp <= 0.0 {
        0.0
    } else if (4.0..=36.0).contains(&interp) {
        1.0
    } else if interp < 4.0 {
        interp / 4.0
    } else {
        -(interp - 40.0) / 4.0
    };

    let angle_scale = if (4.0..36.0).contains(&interp) {
        let s = (interp - 4.0) / 32.0;
        PI / 5.0 + (PI * 7.0 / 100.0) * (s * 28.7).sin()
    } else if interp > 0.0 {
        PI / 5.0
    } else {
        0.0
    };

    (pos_scale, angle_scale)
}

fn client_information_changed(
    last: (
        u32,
        crate::ui::chat::ChatVisibilitySetting,
        bool,
        bool,
        crate::particle::ParticleMode,
        &str,
    ),
    current: (
        u32,
        crate::ui::chat::ChatVisibilitySetting,
        bool,
        bool,
        crate::particle::ParticleMode,
        &str,
    ),
) -> bool {
    last != current
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{
        advance_server_time, advance_world_clock_tick, armor_stand_render_infos,
        arrow_render_infos, block_entity_in_frustum, boat_render_infos, boat_underwater_status,
        bump_loaded_content_generations, bump_section_generations, consecutive_section_runs,
        credits_may_advance, current_edit_section_runs, death_confirm_escape_allowed,
        experience_orb_color, experience_orb_icon, experience_orb_light,
        experience_orb_render_infos, finish_win_credits, finish_win_credits_if_allowed,
        has_red_overlay, is_win_game_event, item_frame_base_position, item_frame_base_rotation,
        limited_crafting_param, mannequin_render_infos, mesh_result_is_stale, mesh_target_mask,
        minecart_render_infos, native_cape_pose, section_bit, section_bits, server_tick_runs,
        show_death_screen_param, sign_has_text, sign_text_in_range, tick_cloaks, tnt_render_effect,
    };
    use crate::renderer::SkyState;

    #[test]
    fn native_cape_pose_matches_standing_and_forward_lean_goldens() {
        use glam::{DVec3, Vec3};

        let stand = native_cape_pose(DVec3::ZERO, DVec3::ZERO, 0.0, 0.0, 0.0, 0.0, 0.5);
        let up = stand * Vec3::Y;
        assert!((up.y - 6.0_f32.to_radians().cos()).abs() < 1e-5);
        assert!((up.z + 6.0_f32.to_radians().sin()).abs() < 1e-5);

        let lean = native_cape_pose(
            DVec3::new(0.0, 0.0, -1.0),
            DVec3::ZERO,
            0.0,
            0.0,
            0.0,
            0.0,
            0.5,
        );
        let up = lean * Vec3::Y;
        assert!((up.y - 56.0_f32.to_radians().cos()).abs() < 1e-5);
        assert!((up.z + 56.0_f32.to_radians().sin()).abs() < 1e-5);

        let moving = glam::Mat4::from_quat(native_cape_pose(
            DVec3::new(0.0, 0.0, -1.0),
            DVec3::ZERO,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
        ));
        let partial_one = glam::Mat4::from_quat(native_cape_pose(
            DVec3::new(0.0, 0.0, -1.0),
            DVec3::ZERO,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
        ));
        let flying_done = glam::Mat4::from_quat(native_cape_pose(
            DVec3::new(0.0, 0.0, -1.0),
            DVec3::ZERO,
            0.0,
            0.0,
            0.0,
            10.0,
            0.0,
        ));
        assert!(!moving.abs_diff_eq(partial_one, 1e-5));
        assert!(partial_one.abs_diff_eq(flying_done, 1e-5));
        let turned = native_cape_pose(
            DVec3::new(0.0, 0.0, -1.0),
            DVec3::ZERO,
            90.0,
            0.0,
            0.0,
            0.0,
            0.0,
        );
        assert!(!moving.abs_diff_eq(glam::Mat4::from_quat(turned), 1e-4));
    }

    #[test]
    fn tick_cloaks_snapshots_local_living_and_mannequin_only() {
        use azalea_registry::builtin::EntityKind;

        use crate::entity::EntityStore;
        use crate::entity::components::{LookDirection, Position};
        let mut player = crate::player::LocalPlayer::new();
        let mut entities = EntityStore::new();
        entities.spawn_living(
            1,
            EntityKind::Zombie,
            Position::new(1.0, 0.0, 0.0),
            LookDirection::default(),
            0.0,
            None,
        );
        entities.set_vehicle_spawn_transform(
            2,
            Position::new(1.0, 0.0, 0.0),
            glam::DVec3::ZERO,
            crate::entity::components::LookDirection::default(),
        );
        entities.set_vehicle_kind(2, EntityKind::Mannequin);
        entities.set_vehicle_spawn_transform(
            3,
            Position::new(1.0, 0.0, 0.0),
            glam::DVec3::ZERO,
            crate::entity::components::LookDirection::default(),
        );
        entities.set_vehicle_kind(3, EntityKind::AcaciaBoat);

        tick_cloaks(&mut player, &mut entities);
        player.position = Position::new(4.0, 0.0, 0.0);
        entities.vehicles.get_mut(&2).unwrap().position = Position::new(5.0, 0.0, 0.0);
        entities.vehicles.get_mut(&3).unwrap().position = Position::new(9.0, 0.0, 0.0);
        entities.living.get_mut(&1).unwrap().position = Position::new(3.0, 0.0, 0.0);
        tick_cloaks(&mut player, &mut entities);
        assert_eq!(player.cloak.current.x, 1.0);
        assert_eq!(entities.living[&1].cloak.current.x, 1.5);
        assert_eq!(entities.vehicles[&2].cloak.current.x, 2.0);
        assert!(!entities.vehicles[&3].cloak.initialized);
        entities.remove_entity(2);
        assert!(!entities.vehicles.contains_key(&2));
    }

    #[test]
    fn experience_orb_icon_thresholds_and_source_tint() {
        for (value, icon) in [
            (-1, 0),
            (0, 0),
            (2, 0),
            (3, 1),
            (6, 1),
            (7, 2),
            (17, 3),
            (37, 4),
            (73, 5),
            (149, 6),
            (307, 7),
            (617, 8),
            (1237, 9),
            (2476, 9),
            (2477, 10),
        ] {
            assert_eq!(experience_orb_icon(value), icon, "value {value}");
        }
        assert_eq!(
            experience_orb_color(0.0, 1.0),
            [127.0 / 255.0, 1.0, 3.0 / 255.0, 128.0 / 255.0]
        );
        assert!((experience_orb_light(0, 0, "minecraft:overworld") - 7.0 / 39.0).abs() < 1e-6);
        assert_eq!(experience_orb_light(15, 0, "minecraft:overworld"), 1.0);
        assert_eq!(experience_orb_light(0, 15, "minecraft:overworld"), 1.0);
        assert!(
            (experience_orb_light(0, 0, "minecraft:the_nether") - (0.1 + 0.9 * (7.0 / 39.0))).abs()
                < 1e-6
        );
    }

    #[test]
    fn armor_stand_render_extraction_keeps_marker_visible_and_filters_entity_invisible() {
        use azalea_registry::builtin::EntityKind;

        use crate::entity::components::Position;
        use crate::entity::{EntityStore, MetaValue};

        let mut store = EntityStore::new();
        store.set_vehicle_transform(1, Position::new(1.0, 2.0, 3.0), glam::DVec3::ZERO);
        store.set_vehicle_kind(1, EntityKind::ArmorStand);
        store.apply_vehicle_metadata(1, 15, MetaValue::Byte(0x11));
        store.set_armor_stand_rotation(1, 16, [0.0, 45.0, 0.0]);
        store.set_armor_stand_equipment(
            1,
            vec![(
                azalea_inventory::components::EquipmentSlot::Mainhand,
                azalea_inventory::ItemStack::Empty,
            )],
        );
        let renders = armor_stand_render_infos(&store, 0.5);
        assert_eq!(renders.len(), 1);
        assert_eq!(renders[0].armor_stand_flags, 0x11);
        assert_eq!(renders[0].armor_stand_pose[0], [0.0, 45.0, 0.0]);
        assert_eq!(renders[0].armor_stand_equipment.len(), 1);
        store.apply_vehicle_metadata(1, 15, MetaValue::Byte(0x01));
        let renders = armor_stand_render_infos(&store, 0.5);
        assert_eq!(renders[0].body_transform.unwrap().x_axis.x, 0.5);
        store.apply_vehicle_metadata(1, 0, MetaValue::Byte(0x20));
        assert!(armor_stand_render_infos(&store, 0.5).is_empty());
    }

    #[test]
    fn mannequin_skin_keys_use_instance_uuid_and_nil_fallback_stays_native() {
        use azalea_registry::builtin::EntityKind;

        use crate::entity::EntityStore;
        use crate::entity::components::Position;

        let mut store = EntityStore::new();
        let first = uuid::Uuid::from_u128(1);
        let second = uuid::Uuid::from_u128(2);
        for (id, uuid) in [(1, first), (2, second)] {
            store.set_vehicle_transform(id, Position::default(), glam::DVec3::ZERO);
            store.set_vehicle_kind(id, EntityKind::Mannequin);
            store.set_vehicle_uuid(id, uuid);
            store.set_mannequin_profile(id, azalea_inventory::components::Profile::default());
        }
        let infos = mannequin_render_infos(&store, 0.0);
        assert_eq!(infos.len(), 2);
        assert_ne!(infos[0].player_uuid, infos[1].player_uuid);
        assert!(infos.iter().all(|info| {
            info.variant_index
                == crate::renderer::pipelines::entity_renderer::default_player_skin_index(
                    uuid::Uuid::nil(),
                ) as u32
        }));
    }

    #[test]
    fn mannequin_render_extraction_forwards_and_resets_chest_stack() {
        use azalea_registry::builtin::{EntityKind, ItemKind};

        use crate::entity::EntityStore;
        use crate::entity::components::Position;

        let mut store = EntityStore::new();
        store.set_vehicle_kind(3, EntityKind::Mannequin);
        store.set_vehicle_transform(3, Position::new(1.0, 2.0, 3.0), glam::DVec3::ZERO);
        let chest = azalea_inventory::ItemStack::Present(azalea_inventory::ItemStackData::new(
            ItemKind::Elytra,
            1,
        ));
        store.set_armor_stand_equipment(
            3,
            vec![(
                azalea_inventory::components::EquipmentSlot::Chest,
                chest.clone(),
            )],
        );
        assert_eq!(
            mannequin_render_infos(&store, 0.5)[0].chest_equipment,
            chest
        );
        store.set_armor_stand_equipment(
            3,
            vec![(
                azalea_inventory::components::EquipmentSlot::Chest,
                azalea_inventory::ItemStack::Empty,
            )],
        );
        assert_eq!(
            mannequin_render_infos(&store, 0.5)[0].chest_equipment,
            azalea_inventory::ItemStack::Empty,
        );
    }

    #[test]
    fn experience_orb_render_extraction_interpolates_fixed_tick_age_and_filters_invisible() {
        use azalea_registry::builtin::EntityKind;

        use crate::entity::components::Position;
        use crate::entity::{EntityStore, MetaValue};
        use crate::world::chunk::ChunkStore;

        let mut store = EntityStore::new();
        store.set_passengers(1, &[]);
        store.set_vehicle_kind(1, EntityKind::ExperienceOrb);
        store.set_vehicle_transform(1, Position::new(2.0, 3.0, 4.0), glam::DVec3::ZERO);
        store.apply_vehicle_metadata(1, 8, MetaValue::Int(617));
        store.tick_projectile_displays(&ChunkStore::new(2));
        let chunks = ChunkStore::new(2);
        let info =
            experience_orb_render_infos(&store, &chunks, "minecraft:overworld", 0.5, 30.0, -15.0);
        assert_eq!(info.len(), 1);
        assert_eq!(info[0].age_in_ticks, 1.5);
        assert_eq!(info[0].variant_index, 8);
        assert_eq!(
            (info[0].body_y_rot_deg, info[0].head_x_rot_deg),
            (30.0, -15.0)
        );
        store.apply_vehicle_metadata(1, 0, MetaValue::Byte(0x20));
        assert!(
            experience_orb_render_infos(&store, &chunks, "minecraft:overworld", 0.5, 30.0, -15.0,)
                .is_empty()
        );
    }

    #[test]
    fn tnt_fuse_scale_flash_and_partial_tick_boundaries() {
        assert_eq!(tnt_render_effect(80.0), (1.0, 1.0));
        assert_eq!(tnt_render_effect(10.0), (1.0, 1.0));
        assert_eq!(tnt_render_effect(5.0).1, 0.0);
        assert_eq!(tnt_render_effect(9.0).1, 1.0);
        assert!((tnt_render_effect(9.0).0 - 1.00003).abs() < 1.0e-6);
        assert_eq!(tnt_render_effect(0.0), (1.3, 1.0));
        assert_eq!(tnt_render_effect(-1.0), (1.0, 0.0));
        let partial = 10.0 + (9.0 - 10.0) * 0.5;
        assert_eq!(tnt_render_effect(partial).1, 1.0);
    }

    #[test]
    fn boat_underwater_status_matches_native_top_plane_fluid_scan() {
        use crate::world::block::{Fluid, FluidKind};
        let empty = Fluid {
            kind: FluidKind::Empty,
            amount: 0,
            falling: false,
        };
        let source = Fluid {
            kind: FluidKind::Water,
            amount: 8,
            falling: false,
        };
        let flowing = Fluid {
            kind: FluidKind::Water,
            amount: 4,
            falling: false,
        };
        let lava = Fluid {
            kind: FluidKind::Lava,
            amount: 8,
            falling: false,
        };
        assert_eq!(
            boat_underwater_status(0.0, 1.0, 0.0, 0.0, 1.0, |_, _, _| empty),
            None
        );
        // Water contacting only the boat's bottom is not an underwater status.
        assert_eq!(
            boat_underwater_status(0.0, 1.0, 0.0, 0.0, 1.0, |_, y, _| if y == -1 {
                source
            } else {
                empty
            }),
            None
        );
        assert_eq!(
            boat_underwater_status(0.0, 1.0, 0.0, 0.0, 1.0, |_, y, _| if y == 0 {
                source
            } else {
                empty
            }),
            Some(super::BoatUnderwaterStatus::UnderWater)
        );
        assert_eq!(
            boat_underwater_status(0.0, 1.0, 0.0, 0.0, 1.0, |_, y, _| if y == 0 {
                flowing
            } else {
                empty
            }),
            Some(super::BoatUnderwaterStatus::UnderFlowingWater)
        );
        // Native getHeight returns a full block when the same fluid continues above.
        assert_eq!(
            boat_underwater_status(0.0, 1.0, 0.5, 0.0, 1.0, |_, y, _| {
                if y == 0 || y == 1 { flowing } else { empty }
            }),
            Some(super::BoatUnderwaterStatus::UnderFlowingWater)
        );
        assert_eq!(
            boat_underwater_status(0.0, 1.0, 0.0, 0.0, 1.0, |_, _, _| lava),
            None
        );
        // A waterlogged block exposes source water to FluidState just like an ordinary
        // source.
        assert_eq!(
            boat_underwater_status(0.0, 1.0, 0.0, 0.0, 1.0, |_, _, _| source),
            Some(super::BoatUnderwaterStatus::UnderWater)
        );
        // Integer floor/ceil scan is Euclidean in effect for negative coordinates.
        assert_eq!(
            boat_underwater_status(-1.8, -0.2, 0.0, -1.8, -0.2, |x, _, z| {
                if x == -1 && z == -1 { source } else { empty }
            }),
            Some(super::BoatUnderwaterStatus::UnderWater)
        );
        let surface = f64::from(source.height());
        assert_eq!(
            boat_underwater_status(0.0, 1.0, surface - 0.001, 0.0, 1.0, |_, y, _| if y == 0 {
                source
            } else {
                empty
            }),
            None
        );
    }

    #[test]
    fn all_twenty_boat_kinds_extract_from_nonliving_vehicle_store() {
        use azalea_registry::builtin::EntityKind as K;

        use crate::entity::EntityStore;
        use crate::entity::components::{LookDirection, Position};

        let kinds = [
            K::AcaciaBoat,
            K::AcaciaChestBoat,
            K::BambooRaft,
            K::BambooChestRaft,
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
        let mut store = EntityStore::new();
        for (id, kind) in kinds.into_iter().enumerate() {
            store.set_vehicle_spawn_transform(
                id as i32,
                Position::new(id as f64, 64.0, -2.0),
                glam::DVec3::ZERO,
                LookDirection::new(135.0, 0.0),
            );
            store.set_vehicle_kind(id as i32, kind);
        }
        let chunks = crate::world::chunk::ChunkStore::new(1);
        let draws = boat_render_infos(&store, &chunks, 1.0);
        assert_eq!(draws.len(), 20);
        assert!(store.living.is_empty(), "boats remain nonliving");
        assert!(draws.iter().all(|info| info.body_y_rot_deg == 135.0));
        assert!(draws.iter().all(|info| info.body_transform.is_some()));
        assert!(draws.iter().all(|info| info.boat_rowing_time == [0.0; 2]));
        assert!(draws.iter().all(|info| !info.boat_underwater));
    }

    #[test]
    fn boat_pose_uses_interpolated_native_tick_state() {
        use azalea_registry::builtin::EntityKind as K;

        use crate::entity::EntityStore;
        use crate::entity::components::{LookDirection, Position};
        use crate::world::chunk::ChunkStore;

        let mut store = EntityStore::new();
        store.set_vehicle_spawn_transform(
            1,
            Position::default(),
            glam::DVec3::ZERO,
            LookDirection::default(),
        );
        store.set_vehicle_kind(1, K::OakBoat);
        store.apply_vehicle_metadata(1, 8, crate::entity::MetaValue::Int(3));
        store.apply_vehicle_metadata(1, 10, crate::entity::MetaValue::Float(8.0));
        store.apply_vehicle_metadata(1, 11, crate::entity::MetaValue::Bool(true));
        store.apply_vehicle_metadata(1, 12, crate::entity::MetaValue::Bool(true));
        store.apply_vehicle_metadata(1, 13, crate::entity::MetaValue::Int(10));
        store.tick_projectile_displays(&ChunkStore::new(1));

        let chunks = ChunkStore::new(1);
        let info = &boat_render_infos(&store, &chunks, 0.5)[0];
        assert_eq!(info.boat_rowing_time, [0.19634955; 2]);
        assert_eq!(store.vehicles[&1].boat_prev_hurt_time, 3);
        assert_eq!(store.vehicles[&1].boat_hurt_time, 2);
        assert_eq!(store.vehicles[&1].boat_prev_damage, 8.0);
        assert_eq!(store.vehicles[&1].boat_damage, 7.0);
        assert!((store.vehicles[&1].boat_bubble_angle - 10.0 * 0.5_f32.sin() * 0.05).abs() < 1e-6);
        assert!(info.body_transform.is_some());

        store.apply_vehicle_metadata(1, 11, crate::entity::MetaValue::Bool(false));
        store.apply_vehicle_metadata(1, 13, crate::entity::MetaValue::Int(0));
        store.tick_projectile_displays(&ChunkStore::new(1));
        let reset = &boat_render_infos(&store, &chunks, 1.0)[0];
        assert_eq!(reset.boat_rowing_time[0], 0.0);
        assert_eq!(reset.boat_rowing_time[1], 2.0 * 0.3926991);
        assert_eq!(store.vehicles[&1].boat_bubble_multiplier, 0.0);
    }

    #[test]
    fn seven_minecart_kinds_extract_interpolated_draw_infos_from_nonliving_store() {
        use azalea_registry::builtin::EntityKind as K;

        use crate::entity::EntityStore;
        use crate::entity::components::{LookDirection, Position};

        let kinds = [
            K::Minecart,
            K::ChestMinecart,
            K::FurnaceMinecart,
            K::TntMinecart,
            K::HopperMinecart,
            K::CommandBlockMinecart,
            K::SpawnerMinecart,
        ];
        let mut store = EntityStore::new();
        for (id, kind) in kinds.into_iter().enumerate() {
            store.set_vehicle_spawn_transform(
                id as i32,
                Position::new(id as f64, 64.0, -4.0),
                glam::DVec3::ZERO,
                LookDirection::new(170.0, 12.0),
            );
            store.set_vehicle_kind(id as i32, kind);
        }
        let draws = minecart_render_infos(&store, 0.5);
        assert_eq!(draws.len(), 7);
        assert!(store.living.is_empty());
        assert!(draws.iter().all(|info| info.body_y_rot_deg == 170.0));
        assert!(draws.iter().all(|info| info.position.y == 64.0));
        assert!(draws.iter().all(|info| info.body_transform.is_some()));
    }

    #[test]
    fn pig_variant_store_value_reaches_renderer_extras() {
        use azalea_registry::builtin::EntityKind;

        use crate::entity::EntityStore;
        use crate::entity::components::{LookDirection, Position};

        let mut store = EntityStore::new();
        store.spawn_living(
            7,
            EntityKind::Pig,
            Position::default(),
            LookDirection::default(),
            0.0,
            None,
        );
        store.set_variant(7, EntityKind::Pig, 1); // cold, pool order temperate/cold/warm
        let extras = super::entity_extras(7, &store.living[&7], 1.0, 0);
        assert_eq!(extras.variant_index, 1);
    }

    #[test]
    fn hopper_ui_targets_all_41_native_slots() {
        use azalea_inventory::ItemStack;
        use azalea_inventory::operations::{ClickOperation, QuickMoveClick};

        use crate::ui::container::ContainerInput;
        let input = ContainerInput {
            left_pressed: true,
            right_pressed: false,
            middle_pressed: false,
            left_held: false,
            right_held: false,
            shift: true,
            hotbar_swap: None,
            swap_offhand: false,
            throw: false,
            throw_all: false,
        };
        let slots = vec![ItemStack::Empty; 41];
        for scale in [1.0, 2.0, 3.0] {
            for slot in 0..41u16 {
                let (x, y) = match slot {
                    0..=4 => (44.0 + slot as f32 * 18.0, 20.0),
                    5..=31 => (
                        8.0 + ((slot - 5) % 9) as f32 * 18.0,
                        51.0 + ((slot - 5) / 9) as f32 * 18.0,
                    ),
                    _ => (8.0 + (slot - 32) as f32 * 18.0, 109.0),
                };
                let mut elements = Vec::new();
                let result = super::build_hopper(
                    &mut elements,
                    176.0 * scale,
                    133.0 * scale,
                    ((x + 8.0) * scale, (y + 8.0) * scale),
                    &input,
                    &slots,
                    "Hopper",
                    &ItemStack::Empty,
                    &mut None,
                    &mut None,
                    scale,
                    false,
                );
                use crate::renderer::pipelines::menu_overlay::{MenuElement, SpriteId};
                assert!(elements.iter().any(|element| matches!(element,MenuElement::Image {x,y,w,h,sprite:SpriteId::HopperBackground,..}
                if *x==0.0 && *y==0.0 && *w==176.0*scale && *h==133.0*scale)));
                assert!(elements.iter().any(
                    |element| matches!(element,MenuElement::TextFlat {x,y,text,..}
                if *x==8.0*scale && *y==39.0*scale && text=="Inventory")
                ));
                assert!(!elements.iter().any(|element| matches!(
                    element,
                    MenuElement::Image {
                        sprite: SpriteId::Generic54Bottom,
                        ..
                    }
                )));
                assert!(!result.clicked_outside);
                assert!(matches!(
                    result.ops.as_slice(),
                    [ClickOperation::QuickMove(QuickMoveClick::Left { slot: actual })]
                        if *actual == slot
                ));
            }
        }
        // Empty space beside the five hopper slots is not a phantom sixth slot.
        let result = super::build_hopper(
            &mut Vec::new(),
            176.0,
            133.0,
            (8.0, 28.0),
            &input,
            &slots,
            "Hopper",
            &ItemStack::Empty,
            &mut None,
            &mut None,
            1.0,
            false,
        );
        assert!(result.ops.is_empty());
    }

    #[test]
    fn client_information_diff_includes_locale_without_repeat_sends() {
        use crate::ui::chat::ChatVisibilitySetting;
        let last = (
            12,
            ChatVisibilitySetting::Full,
            true,
            true,
            crate::particle::ParticleMode::All,
            "en_us",
        );
        assert!(!super::client_information_changed(last, last));
        assert!(super::client_information_changed(
            last,
            (
                12,
                ChatVisibilitySetting::Full,
                true,
                true,
                crate::particle::ParticleMode::All,
                "ja_jp",
            )
        ));
        let updated = (
            12,
            ChatVisibilitySetting::Full,
            true,
            true,
            crate::particle::ParticleMode::All,
            "ja_jp",
        );
        assert!(!super::client_information_changed(updated, updated));
        assert!(super::client_information_changed(
            updated,
            (
                13,
                ChatVisibilitySetting::Full,
                true,
                true,
                crate::particle::ParticleMode::All,
                "ja_jp",
            )
        ));
        assert!(super::client_information_changed(
            updated,
            (
                12,
                ChatVisibilitySetting::Full,
                false,
                true,
                crate::particle::ParticleMode::All,
                "ja_jp",
            )
        ));
        assert!(super::client_information_changed(
            updated,
            (
                12,
                ChatVisibilitySetting::Full,
                true,
                false,
                crate::particle::ParticleMode::All,
                "ja_jp",
            )
        ));
        assert!(super::client_information_changed(
            updated,
            (
                12,
                ChatVisibilitySetting::Full,
                true,
                true,
                crate::particle::ParticleMode::Minimal,
                "ja_jp",
            )
        ));
    }

    #[test]
    fn sign_text_copy_range_matches_draw_distance() {
        use azalea_core::position::BlockPos;
        use glam::DVec3;

        assert!(sign_text_in_range(&BlockPos::new(15, 0, 0), DVec3::ZERO));
        assert!(sign_text_in_range(
            &BlockPos::new(16, 0, 0),
            DVec3::new(0.5, 0.5, 0.5)
        ));
        assert!(!sign_text_in_range(
            &BlockPos::new(17, 0, 0),
            DVec3::new(0.5, 0.5, 0.5)
        ));
    }

    #[test]
    fn chest_suppression_requires_visible_resident_section() {
        use azalea_core::position::{BlockPos, ChunkPos};

        use super::chest_section_visible;
        let col = ChunkPos::new(-2, 1);
        let pos = BlockPos::new(-31, -49, 16);
        assert_eq!(col.x, pos.x.div_euclid(16));
        let si = (pos.y + 64).div_euclid(16);
        assert_eq!(si, 0);
        assert!(chest_section_visible(si, true, Some(1)));
        assert!(!chest_section_visible(si, true, Some(0))); // outside RD / occluded
        assert!(!chest_section_visible(si, false, Some(1)));
        assert!(!chest_section_visible(si, true, None));
        assert!(!chest_section_visible(32, true, Some(u32::MAX)));
    }

    #[test]
    fn block_entity_cull_only_rejects_proven_outside_boxes() {
        use azalea_core::position::BlockPos;
        use azalea_registry::builtin::BlockEntityKind as Kind;
        use glam::DVec3;

        // A half-space x >= 0; an 8.5-radius box touching the boundary draws.
        let mut planes = [[1.0, 0.0, 0.0, 100_000.0]; 6];
        planes[0][3] = 0.0;
        let eye = DVec3::ZERO;
        let visible = |kind, x, text, planes: &[[f32; 4]; 6]| {
            block_entity_in_frustum(kind, &BlockPos::new(x, 0, 0), text, eye, planes)
        };
        for kind in [
            Kind::Chest,
            Kind::TrappedChest,
            Kind::EnderChest,
            Kind::ShulkerBox,
            Kind::CopperGolemStatue,
            Kind::Skull,
            Kind::Conduit,
            Kind::Sign,
        ] {
            assert!(!visible(kind, -20, false, &planes), "{kind:?}");
            assert!(visible(kind, -9, false, &planes), "{kind:?}"); // box touches
            assert!(visible(kind, 0, false, &planes), "{kind:?}");
        }
        let blank = std::array::from_fn(|_| String::new());
        let mut written = blank.clone();
        written[0] = "glowing text".into();
        assert!(!sign_has_text(Some(&blank), Some(&blank)));
        for (front, back) in [
            (Some(&written), Some(&blank)),
            (Some(&blank), Some(&written)),
        ] {
            assert!(sign_has_text(front, back)); // either face, including glow
            assert!(visible(
                Kind::Sign,
                -20,
                sign_has_text(front, back),
                &planes
            ));
        }
        assert!(visible(Kind::Beacon, -20, false, &planes)); // unknown renderer
        assert!(visible(Kind::Chest, -20, false, &[[0.0; 4]; 6])); // no frustum
        assert!(!block_entity_in_frustum(
            Kind::Chest,
            &BlockPos::new(-20, 0, 0),
            false,
            DVec3::new(-10.0, 0.0, 0.0),
            &planes
        ));
        // Negative positions are camera-relative, not converted to unsigned.
        assert!(block_entity_in_frustum(
            Kind::Chest,
            &BlockPos::new(-20, 0, 0),
            false,
            DVec3::new(-30.0, 0.0, 0.0),
            &planes
        ));
    }

    #[test]
    fn block_entity_state_lookup_reorder_keeps_candidates() {
        use azalea_core::position::BlockPos;
        use azalea_registry::builtin::BlockEntityKind as Kind;
        use glam::DVec3;

        // x >= 0, with the existing 8.5-block model margin. None stands
        // for an unloaded/unknown state (get_block_state returns air).
        let mut planes = [[1.0, 0.0, 0.0, 100_000.0]; 6];
        planes[0][3] = 0.0;
        let eye = DVec3::ZERO;
        for kind in [Kind::Chest, Kind::Sign, Kind::Beacon, Kind::HangingSign] {
            for x in [-20, -10, -9, 0] {
                for text in [false, true] {
                    for state in [None, Some("air"), Some("chest"), Some("oak_hanging_sign")] {
                        let pos = BlockPos::new(x, 0, 0);
                        let in_view = || block_entity_in_frustum(kind, &pos, text, eye, &planes);
                        let valid_state = || {
                            crate::world::block_entity::is_block_entity_block(
                                state.unwrap_or("air"),
                            )
                        };
                        let original = valid_state() && in_view();
                        let mut reads = 0;
                        let reordered = in_view() && {
                            reads += 1;
                            valid_state()
                        };
                        assert_eq!(
                            original, reordered,
                            "{kind:?} x={x} text={text} state={state:?}"
                        );
                        assert_eq!(reads, usize::from(in_view()));
                    }
                }
            }
        }
        assert!(!block_entity_in_frustum(
            Kind::Chest,
            &BlockPos::new(-10, 0, 0),
            false,
            eye,
            &planes
        ));
        assert!(block_entity_in_frustum(
            Kind::Chest,
            &BlockPos::new(-9, 0, 0),
            false,
            eye,
            &planes
        ));
        assert!(block_entity_in_frustum(
            Kind::Sign,
            &BlockPos::new(-20, 0, 0),
            true,
            eye,
            &planes
        ));
    }

    #[test]
    fn block_entity_cull_ignores_stale_far_but_rejects_behind_camera() {
        use azalea_core::position::BlockPos;
        use azalea_registry::builtin::BlockEntityKind as Kind;
        use glam::DVec3;

        use crate::renderer::camera::Camera;

        let mut camera = Camera::new(16.0 / 9.0); // +Z; initial far is 1000
        camera.set_render_distance(1);
        let old_planes = camera.block_entity_frustum_planes();
        let eye = DVec3::ZERO;
        let visible = |pos, planes: &[[f32; 4]; 6]| {
            block_entity_in_frustum(Kind::Chest, &pos, false, eye, planes)
        };
        let near = BlockPos::new(0, 0, 300);
        let distant = BlockPos::new(0, 0, 1500);
        assert!(visible(near, &old_planes));
        // Without dropping plane 5, the old RD would reject this before
        // render_world sets the new RD (32 chunks => far 2048).
        assert!(!super::aabb_in_frustum(
            &[-8.0, -8.0, 1492.0],
            &[9.0, 9.0, 1509.0],
            &old_planes
        ));
        assert!(visible(distant, &old_planes));
        assert!(!visible(BlockPos::new(0, 0, -300), &old_planes));
        assert!(!visible(BlockPos::new(1500, 0, 300), &old_planes));
        camera.set_render_distance(32);
        assert!(visible(distant, &camera.block_entity_frustum_planes()));
    }

    #[test]
    fn block_entity_radius_encloses_every_baked_pose_and_lid() {
        use azalea_registry::builtin::BlockEntityKind as Kind;
        use glam::Vec3;

        use crate::renderer::block_entity_model as models;
        use crate::renderer::entity_model::{BakedEntityModel, PartAnim};

        let check = |model: &BakedEntityModel, anim: PartAnim| {
            let transforms = model.compute_part_transforms(&anim);
            for (i, (start, len)) in model.part_ranges.iter().enumerate() {
                for v in &model.vertices[*start as usize..(*start + *len) as usize] {
                    let p = transforms[i].transform_point3(Vec3::from_array(v.position));
                    // At most two more blocks for the block-centre translation,
                    // wall-head offset and arbitrary facing/root rotations.
                    assert!(p.is_finite() && p.length() + 2.0 < 8.5, "part {i}: {p:?}");
                }
            }
        };
        for model in models::bake_chest_models() {
            for openness in [0.0, 0.5, 1.0] {
                check(&model, super::block_entity::lid_anim(Kind::Chest, openness));
            }
        }
        let shulker = models::bake_shulker_box_model();
        for openness in [0.0, 0.5, 1.0] {
            check(
                &shulker,
                super::block_entity::lid_anim(Kind::ShulkerBox, openness),
            );
        }
        for model in crate::renderer::entity_model::bake_copper_golem_statue_models() {
            check(&model, PartAnim::default());
        }
        for model in [
            models::bake_conduit_model(),
            models::bake_player_head_model(),
            models::bake_sign_model(),
        ] {
            check(&model, PartAnim::default());
        }
    }

    #[test]
    fn projectile_display_interpolates_without_changing_packet_position() {
        crate::world::block::init("26.2");
        use azalea_registry::builtin::EntityKind;
        use glam::DVec3;

        use crate::entity::EntityStore;
        use crate::entity::components::{LookDirection, Position};
        let mut chunks = crate::world::chunk::ChunkStore::new(1);
        let _chunk = chunks.chunk_storage.upsert(
            azalea_core::position::ChunkPos::new(0, 0),
            azalea_world::chunk::Chunk::default(),
        );
        let mut store = EntityStore::new();
        store.set_vehicle_spawn_transform(
            4,
            Position::new(2.0, 70.0, 2.0),
            DVec3::X,
            LookDirection::new(-90.0, 0.0),
        );
        store.set_vehicle_kind(4, EntityKind::Arrow);
        store.tick_projectile_displays(&chunks);
        assert_eq!(store.vehicles[&4].position, Position::new(2.0, 70.0, 2.0));
        assert_eq!(
            arrow_render_infos(&store, 0.5)[0].position,
            Position::new(2.5, 70.0, 2.0)
        );
        // Multiple render extractions in one tick cannot advance simulation.
        for _ in 0..10 {
            assert_eq!(
                arrow_render_infos(&store, 1.0)[0].position,
                Position::new(3.0, 70.0, 2.0)
            );
        }
        store.set_vehicle_transform(4, Position::new(3.25, 71.0, 2.0), DVec3::Y);
        assert_eq!(
            arrow_render_infos(&store, 0.5)[0].position,
            Position::new(3.25, 71.0, 2.0)
        );
        store.remove_entity(4);
        store.set_vehicle_spawn_transform(
            4,
            Position::new(1.0, 70.0, 1.0),
            DVec3::X,
            LookDirection::new(0.0, 0.0),
        );
        store.set_vehicle_kind(4, EntityKind::Snowball);
        store.tick_projectile_displays(&chunks);
        let snowball = super::snowball_render_infos(
            &store,
            0.5,
            DVec3::new(0.0, 70.0, 0.0),
            DVec3::ZERO,
            DVec3::ZERO,
            1.0,
            (0.0, 0.0),
            &[[0.0; 4]; 6],
            |_| Some(glam::Mat4::IDENTITY),
            false,
            |_| 1.0,
        );
        assert!((snowball[0].position[1] - 69.98514999985695).abs() < 1e-10);
        assert_eq!(
            store.vehicles[&4].projectile.as_ref().unwrap().velocity.y,
            -0.03 * (0.99_f32 as f64)
        );
        // Rendering multiple frames between two 20 Hz ticks must not move it.
        for _ in 0..10 {
            let frame = super::snowball_render_infos(
                &store,
                0.5,
                DVec3::new(0.0, 70.0, 0.0),
                DVec3::ZERO,
                DVec3::ZERO,
                1.0,
                (0.0, 0.0),
                &[[0.0; 4]; 6],
                |_| Some(glam::Mat4::IDENTITY),
                false,
                |_| 1.0,
            );
            assert_eq!(frame[0].position, snowball[0].position);
        }
        for _ in 0..12 {
            store.tick_projectile_displays(&chunks);
        }
        let display = store.vehicles[&4].projectile.as_ref().unwrap();
        assert!(
            display.current.x > 10.0,
            "snowball still moves past tick ten"
        );
        assert_ne!(display.prev, display.current);
        assert_eq!(store.vehicles[&4].position, Position::new(1.0, 70.0, 1.0));
    }

    #[test]
    fn arrow_visual_angles_match_packet_look_and_zero_motion_falls_back() {
        use crate::entity::components::{LookDirection, Position};
        let mut store = crate::entity::EntityStore::new();
        store.set_vehicle_spawn_transform(
            1,
            Position::default(),
            glam::DVec3::new(1.0, 1.0, 0.0),
            LookDirection::new(12.0, 13.0),
        );
        store.set_vehicle_kind(1, azalea_registry::builtin::EntityKind::Arrow);
        let arrows = arrow_render_infos(&store, 1.0);
        let arrow = &arrows[0];
        assert_eq!((arrow.body_y_rot_deg, arrow.head_x_rot_deg), (90.0, 45.0));
        store.set_vehicle_motion(1, glam::DVec3::ZERO);
        let arrows = arrow_render_infos(&store, 1.0);
        let arrow = &arrows[0];
        assert_eq!((arrow.body_y_rot_deg, arrow.head_x_rot_deg), (12.0, 13.0));
        // Grounding packets win over a nonzero flight velocity in rendering.
        store.set_vehicle_motion(1, glam::DVec3::Y);
        store.set_projectile_grounded(1, true);
        let arrow = &arrow_render_infos(&store, 1.0)[0];
        assert_eq!((arrow.body_y_rot_deg, arrow.head_x_rot_deg), (12.0, 13.0));
    }

    #[test]
    fn spawned_arrows_render_and_follow_packet_transforms() {
        use azalea_registry::builtin::EntityKind;
        use glam::DVec3;

        use crate::entity::EntityStore;
        use crate::entity::components::{LookDirection, Position};

        let mut store = EntityStore::new();
        for (id, kind) in [(1, EntityKind::Arrow), (2, EntityKind::SpectralArrow)] {
            store.set_vehicle_spawn_transform(
                id,
                Position::new(1.0, 2.0, 3.0),
                DVec3::ZERO,
                LookDirection::new(45.0, -20.0),
            );
            store.set_vehicle_kind(id, kind);
        }
        let renders = arrow_render_infos(&store, 1.0);
        assert_eq!(renders.len(), 2);
        assert!(renders.iter().any(|r| r.variant_index == 0));
        assert!(renders.iter().any(|r| r.variant_index == 1));
        store.set_vehicle_transform(1, Position::new(4.0, 5.0, 6.0), DVec3::ZERO);
        store.set_vehicle_rotation(1, LookDirection::new(90.0, 30.0));
        let moved = arrow_render_infos(&store, 1.0)
            .into_iter()
            .find(|r| r.variant_index == 0)
            .unwrap();
        assert_eq!(moved.position, Position::new(4.0, 5.0, 6.0));
        assert_eq!((moved.body_y_rot_deg, moved.head_x_rot_deg), (90.0, 30.0));
        store.remove_entity(1);
        assert_eq!(arrow_render_infos(&store, 1.0).len(), 1);
    }

    #[test]
    fn snowball_vehicle_extracts_once_moves_and_disappears_without_item_store() {
        use azalea_registry::builtin::EntityKind;
        use glam::{DVec3, Mat4, Vec3};

        use crate::entity::EntityStore;
        use crate::entity::components::{LookDirection, Position};

        let mut store = EntityStore::new();
        for (id, kind) in [
            (1, EntityKind::Snowball),
            (2, EntityKind::Arrow),
            (3, EntityKind::Egg),
        ] {
            store.set_vehicle_spawn_transform(
                id,
                Position::new(0.0, 0.0, 4.0),
                DVec3::ZERO,
                LookDirection::new(0.0, 0.0),
            );
            store.set_vehicle_kind(id, kind);
        }
        // Wide planes, then a narrow right boundary to exercise frustum cull.
        let wide = [[0.0; 4]; 6];
        let ground = Mat4::from_scale(Vec3::splat(0.5));
        let extract =
            |store: &EntityStore, camera: DVec3, scale: f32, planes: &[[f32; 4]; 6], mesh| {
                super::snowball_render_infos(
                    store,
                    1.0,
                    camera,
                    DVec3::ZERO,
                    DVec3::new(1.0, 0.0, 0.0),
                    scale,
                    (0.0, 0.0),
                    planes,
                    |_| mesh,
                    false,
                    |_| 0.7,
                )
            };
        let renders = extract(&store, DVec3::ZERO, 1.0, &wide, Some(ground));
        assert_eq!(renders.len(), 1);
        assert_eq!(renders[0].item_name, "snowball");
        assert_eq!(renders[0].stack_count, 1);
        assert_eq!(renders[0].position, [0.0, 0.0, 4.0]);
        assert_eq!(renders[0].light, 0.7);
        assert_eq!(
            renders[0].model_matrix.transform_point3(Vec3::ZERO),
            Vec3::new(-1.0, 0.0, 4.0),
        );
        assert_eq!(renders[0].model_matrix.x_axis.truncate().length(), 0.5);
        assert_eq!(super::arrow_render_infos(&store, 1.0).len(), 1);
        assert!(extract(&store, DVec3::ZERO, 1.0, &wide, None).is_empty());
        assert!(extract(&store, DVec3::ZERO, 0.0, &wide, Some(ground)).is_empty());
        assert!(extract(&store, DVec3::new(17.0, 0.0, 4.0), 1.0, &wide, Some(ground)).is_empty());
        let mut narrow = wide;
        narrow[0] = [-1.0, 0.0, 0.0, -1.0];
        assert!(extract(&store, DVec3::ZERO, 1.0, &narrow, Some(ground)).is_empty());

        store.set_vehicle_transform(1, Position::new(2.0, 3.0, 5.0), DVec3::ZERO);
        let moved = extract(&store, DVec3::ZERO, 1.0, &wide, Some(ground));
        assert_eq!(moved.len(), 1);
        assert_eq!(moved[0].position, [2.0, 3.0, 5.0]);
        assert_eq!(
            moved[0].model_matrix.transform_point3(Vec3::ZERO),
            Vec3::new(1.0, 3.0, 5.0)
        );
        assert!(!store.remove_impacted_snowball(2)); // Event 3 on another kind
        assert!(store.remove_impacted_snowball(1)); // Event 3 on snowball
        assert!(extract(&store, DVec3::ZERO, 1.0, &wide, Some(ground)).is_empty());
        assert_eq!(super::arrow_render_infos(&store, 1.0).len(), 1);
        assert!(store.vehicles.contains_key(&3));
        store.remove_entity(1); // later RemoveEntities is idempotent
        assert!(!store.remove_impacted_snowball(1));
        store.set_vehicle_spawn_transform(
            1,
            Position::new(0.0, 0.0, 4.0),
            DVec3::ZERO,
            LookDirection::default(),
        );
        store.set_vehicle_kind(1, EntityKind::Snowball); // fresh id reuse is visible
        assert_eq!(
            extract(&store, DVec3::ZERO, 1.0, &wide, Some(ground)).len(),
            1
        );
    }

    #[test]
    fn snowball_draw_key_matches_warmed_registry_item_name() {
        use azalea_registry::builtin::EntityKind;
        use glam::DVec3;

        use crate::entity::EntityStore;
        use crate::entity::components::{LookDirection, Position};
        use crate::world::block::registry::BlockRegistry;

        let root = crate::test_util::test_temp_dir("snowball_item_key");
        let jar = root.join("jar");
        let items = jar.join("minecraft/items");
        let models = jar.join("minecraft/models/item");
        std::fs::create_dir_all(&items).unwrap();
        std::fs::create_dir_all(&models).unwrap();
        std::fs::write(
            items.join("snowball.json"),
            r#"{"model":{"type":"minecraft:model","model":"minecraft:item/snowball"}}"#,
        )
        .unwrap();
        std::fs::write(
            models.join("snowball.json"),
            r#"{"textures":{"layer0":"minecraft:item/snowball"}}"#,
        )
        .unwrap();
        crate::world::block::prewarm_protocol(pomme_protocol::version::NATIVE.protocol);
        let registry = BlockRegistry::load(&jar, &None, &root, None);
        assert_eq!(
            registry.get_flat_item_texture_key("snowball"),
            Some("item/snowball")
        );
        // warm_item_meshes registers these exact names; mesh_info and draw use exact
        // keys.
        let warmed_keys: std::collections::HashSet<_> = registry.item_names().collect();
        assert!(warmed_keys.contains("snowball"));
        assert!(!warmed_keys.contains("minecraft:snowball"));

        let mut store = EntityStore::new();
        store.set_vehicle_spawn_transform(
            1,
            Position::new(0.0, 0.0, 4.0),
            DVec3::ZERO,
            LookDirection::new(0.0, 0.0),
        );
        store.set_vehicle_kind(1, EntityKind::Snowball);
        let renders = super::snowball_render_infos(
            &store,
            1.0,
            DVec3::ZERO,
            DVec3::ZERO,
            DVec3::ZERO,
            1.0,
            (0.0, 0.0),
            &[[0.0; 4]; 6],
            |_| Some(glam::Mat4::IDENTITY),
            false,
            |_| 1.0,
        );
        assert_eq!(renders.len(), 1);
        assert!(warmed_keys.contains(renders[0].item_name.as_str()));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn mesh_rescan_targets_all_sections_inside_render_distance_only() {
        use azalea_core::position::ChunkPos;
        let center = ChunkPos::new(0, 0);
        let inside = ChunkPos::new(4, 0);
        let outside = ChunkPos::new(5, 0);
        // Even a zero visibility mask must not suppress meshing inside RD.
        assert_eq!(mesh_target_mask(inside, center, 4, 0xff), 0xff);
        assert_eq!(mesh_target_mask(outside, center, 4, 0xff), 0);
    }

    #[test]
    fn item_frame_base_and_face_normal_match_all_six_directions() {
        use azalea_core::direction::Direction as D;
        use glam::DVec3;

        for (direction, normal) in [
            (D::North, DVec3::NEG_Z),
            (D::South, DVec3::Z),
            (D::East, DVec3::X),
            (D::West, DVec3::NEG_X),
            (D::Up, DVec3::Y),
            (D::Down, DVec3::NEG_Y),
        ] {
            assert_eq!(
                item_frame_base_position(DVec3::ZERO, direction),
                normal * 0.46875
            );
        }
        // North frame: body mesh is already centered; adding another -0.5
        // would move its intended [-0.03125, 0.03125] depth interval away.
        let north_base = item_frame_base_position(DVec3::ZERO, D::North).z;
        assert!((north_base - (-0.46875)).abs() < 1e-6);
        assert!(((-0.46875_f32 + 0.4375) - (-0.03125)).abs() < 1e-6);
    }

    #[test]
    fn item_frame_item_center_matches_official_horizontal_rotations() {
        use azalea_core::direction::Direction as D;
        use glam::{DVec3, Vec3};

        for (direction, expected) in [
            (D::North, Vec3::new(0.0, 0.0, -0.03125)),
            (D::South, Vec3::new(0.0, 0.0, 0.03125)),
            (D::East, Vec3::new(0.03125, 0.0, 0.0)),
            (D::West, Vec3::new(-0.03125, 0.0, 0.0)),
        ] {
            let position = item_frame_base_position(DVec3::ZERO, direction).as_vec3();
            let matrix =
                glam::Mat4::from_translation(position) * item_frame_base_rotation(direction);
            let center = matrix.transform_point3(Vec3::new(0.0, 0.0, 0.4375));
            assert!(
                center.abs_diff_eq(expected, 1e-6),
                "{direction:?}: {center:?}"
            );
        }
    }

    #[test]
    fn tab_overlay_visibility_requires_every_existing_gate() {
        use super::tab_list_overlay_visible as visible;
        assert!(visible(
            true, false, false, false, false, false, false, false
        ));
        for gate in 1..8 {
            let mut args = [true, false, false, false, false, false, false, false];
            args[gate] = true;
            assert!(!visible(
                args[0], args[1], args[2], args[3], args[4], args[5], args[6], args[7]
            ));
        }
    }

    #[test]
    fn tracking_attachment_uses_local_living_and_spawned_nonliving_dimensions() {
        use azalea_registry::builtin::EntityKind;
        use glam::DVec3;

        use crate::entity::EntityStore;
        use crate::entity::components::{LookDirection, Position};
        use crate::player::LocalPlayer;

        let player = LocalPlayer::new();
        let mut entities = EntityStore::new();
        let mut items = crate::entity::ItemEntityStore::new();
        let local =
            super::GameState::tracking_attachment_for(&player, &entities, &items, player.entity_id)
                .unwrap();
        assert_eq!(local.position, DVec3::from(player.position));
        assert_eq!(
            local.width,
            player.bounding_box().max.x - player.bounding_box().min.x
        );
        assert_eq!(local.height, player.height());

        entities.spawn_living(
            41,
            EntityKind::Zombie,
            Position::new(2.0, 3.0, 4.0),
            LookDirection::new(0.0, 0.0),
            0.0,
            None,
        );
        let living =
            super::GameState::tracking_attachment_for(&player, &entities, &items, 41).unwrap();
        let living_dimensions =
            azalea_entity::dimensions::EntityDimensions::from(EntityKind::Zombie);
        assert_eq!(living.position, DVec3::new(2.0, 3.0, 4.0));
        assert_eq!(living.width, f64::from(living_dimensions.width));
        assert_eq!(living.height, f64::from(living_dimensions.height));

        entities.set_vehicle_spawn_transform(
            42,
            Position::new(5.0, 6.0, 7.0),
            DVec3::ZERO,
            LookDirection::new(0.0, 0.0),
        );
        entities.set_vehicle_kind(42, EntityKind::Minecart);
        let nonliving =
            super::GameState::tracking_attachment_for(&player, &entities, &items, 42).unwrap();
        let vehicle_dimensions =
            azalea_entity::dimensions::EntityDimensions::from(EntityKind::Minecart);
        assert_eq!(nonliving.position, DVec3::new(5.0, 6.0, 7.0));
        assert_eq!(nonliving.width, f64::from(vehicle_dimensions.width));
        assert_eq!(nonliving.height, f64::from(vehicle_dimensions.height));

        // Item-only lookup uses authoritative base position, never render bob.
        items.spawn_item(
            45,
            uuid::Uuid::nil(),
            Position::new(8.0, 9.0, 10.0),
            DVec3::ZERO,
        );
        let item =
            super::GameState::tracking_attachment_for(&player, &entities, &items, 45).unwrap();
        let item_dimensions = azalea_entity::dimensions::EntityDimensions::from(EntityKind::Item);
        assert_eq!(item.position, DVec3::new(8.0, 9.0, 10.0));
        assert_eq!(item.width, f64::from(item_dimensions.width));
        assert_eq!(item.height, f64::from(item_dimensions.height));
        items.teleport(45, Position::new(11.0, 12.0, 13.0), None, false);
        assert_eq!(
            super::GameState::tracking_attachment_for(&player, &entities, &items, 45)
                .unwrap()
                .position,
            DVec3::new(11.0, 12.0, 13.0)
        );
        assert!(
            super::GameState::tracking_attachment_for(&player, &entities, &items, 46).is_none()
        );

        entities.set_passengers(43, &[]);
        entities.set_vehicle_transform(44, Position::default(), DVec3::ZERO);
        assert!(
            super::GameState::tracking_attachment_for(&player, &entities, &items, 43).is_none()
        );
        assert!(
            super::GameState::tracking_attachment_for(&player, &entities, &items, 44).is_none()
        );
        assert!(
            super::GameState::tracking_attachment_for(&player, &entities, &items, 999).is_none()
        );
    }

    #[test]
    fn fixed_world_tick_advances_cursor_and_independent_clock_samples_once() {
        let mut game_time = 100;
        let mut clocks = HashMap::from([
            (
                1,
                crate::net::environment::ClockSample {
                    total_ticks: 100,
                    partial_tick: 0.0,
                    rate: 1.0,
                },
            ),
            (
                2,
                crate::net::environment::ClockSample {
                    total_ticks: 7,
                    partial_tick: 0.75,
                    rate: 0.0,
                },
            ),
            (
                3,
                crate::net::environment::ClockSample {
                    total_ticks: 0,
                    partial_tick: 0.75,
                    rate: -0.25,
                },
            ),
            (
                4,
                crate::net::environment::ClockSample {
                    total_ticks: 0,
                    partial_tick: 0.0,
                    rate: 0.25,
                },
            ),
        ]);
        for _ in 0..20 {
            advance_world_clock_tick(&mut game_time, &mut clocks);
        }
        assert_eq!(game_time, 120);
        assert_eq!(
            (clocks[&1].total_ticks, clocks[&1].partial_tick),
            (120, 0.0)
        );
        assert_eq!((clocks[&2].total_ticks, clocks[&2].partial_tick), (7, 0.75));
        assert_eq!(
            (clocks[&3].total_ticks, clocks[&3].partial_tick),
            (-5, 0.75)
        );
        assert_eq!((clocks[&4].total_ticks, clocks[&4].partial_tick), (5, 0.0));

        let source = include_str!("in_game.rs");
        assert_eq!(
            source
                .matches("advance_world_clock_tick(&mut game.world_clock_game_time")
                .count(),
            1
        );
    }

    #[test]
    fn fixed_world_time_obeys_pause_and_freeze_gates() {
        let mut game_time = 100;
        let mut clocks = HashMap::from([(
            1,
            crate::net::environment::ClockSample {
                total_ticks: 100,
                partial_tick: 0.0,
                rate: 1.0,
            },
        )]);
        for (frozen, simulation_ticks, singleplayer_paused) in [(true, 0, false), (false, 1, true)]
        {
            if (!frozen || simulation_ticks > 0) && !singleplayer_paused {
                advance_world_clock_tick(&mut game_time, &mut clocks);
            }
        }
        assert_eq!(game_time, 100);
        assert_eq!(clocks[&1].total_ticks, 100);
    }

    #[test]
    fn frozen_ticks_only_run_when_step_budget_exists() {
        assert!(server_tick_runs(false, 0));
        assert!(server_tick_runs(false, 3));
        assert!(!server_tick_runs(true, 0));
        assert!(server_tick_runs(true, 1));
    }

    #[test]
    fn server_clock_uses_10_20_and_40_tick_cadences() {
        for (rate, expected) in [(10.0, 10u32), (20.0, 20), (40.0, 40)] {
            let mut sky = SkyState::default_day();
            sky.apply_clock_update(0, 0, 0.0, 1.0);
            let mut accumulator = 0.0;
            let mut steps = 0;
            assert_eq!(
                advance_server_time(
                    &mut accumulator,
                    1.0,
                    rate,
                    false,
                    &mut steps,
                    &mut sky,
                    &mut HashMap::new(),
                ),
                expected
            );
            assert_eq!(sky.day_time, i64::from(expected));
        }
    }

    #[test]
    fn server_tick_prediction_does_not_double_advance_world_clock_samples() {
        let mut sky = SkyState::default_day();
        let mut clocks = HashMap::from([
            (
                1,
                crate::net::environment::ClockSample {
                    total_ticks: 20,
                    partial_tick: 0.0,
                    rate: 0.0,
                },
            ),
            (
                2,
                crate::net::environment::ClockSample {
                    total_ticks: 30,
                    partial_tick: 0.0,
                    rate: 0.5,
                },
            ),
        ]);
        let mut accumulator = 0.0;
        let mut steps = 0;
        assert_eq!(
            advance_server_time(
                &mut accumulator,
                1.0,
                20.0,
                false,
                &mut steps,
                &mut sky,
                &mut clocks
            ),
            20
        );
        assert_eq!(clocks[&1].total_ticks, 20);
        assert_eq!(clocks[&2].total_ticks, 30);
        assert_eq!(clocks[&2].partial_tick, 0.0);
    }

    #[test]
    fn frozen_clock_stops_and_step_budget_is_consumed_at_server_cadence() {
        let mut sky = SkyState::default_day();
        sky.apply_clock_update(0, 0, 0.0, 1.0);
        let mut accumulator = 0.0;
        let mut steps = 0;
        advance_server_time(
            &mut accumulator,
            1.0,
            20.0,
            true,
            &mut steps,
            &mut sky,
            &mut HashMap::new(),
        );
        assert_eq!(sky.day_time, 0);
        steps = 2;
        assert_eq!(
            advance_server_time(
                &mut accumulator,
                0.1,
                20.0,
                true,
                &mut steps,
                &mut sky,
                &mut HashMap::new(),
            ),
            2
        );
        assert_eq!(sky.day_time, 2);
        assert_eq!(steps, 0);
    }

    #[test]
    fn item_age_follows_simulation_ticks_not_client_ticks_or_daytime() {
        use glam::DVec3;
        use uuid::Uuid;

        use crate::entity::ItemEntityStore;
        use crate::entity::components::Position;

        let mut sky = SkyState::default_day();
        let mut items = ItemEntityStore::new();
        items.spawn_item(1, Uuid::nil(), Position::new(0.0, 64.0, 0.0), DVec3::ZERO);
        items.set_item_data(1, "minecraft:stone".into(), 1, 0, 1, None);
        let age = |items: &ItemEntityStore| items.visible_items(DVec3::ZERO, 100.0)[0].age;
        assert_eq!(age(&items), 0);

        let mut accumulator = 0.0;
        let mut steps = 0;
        let ticks = advance_server_time(
            &mut accumulator,
            1.0,
            20.0,
            true,
            &mut steps,
            &mut sky,
            &mut HashMap::new(),
        );
        items.advance_age(ticks);
        assert_eq!(age(&items), 0);
        sky.day_time = 6000;
        assert_eq!(age(&items), 0);

        steps = 3;
        let ticks = advance_server_time(
            &mut accumulator,
            0.15,
            20.0,
            true,
            &mut steps,
            &mut sky,
            &mut HashMap::new(),
        );
        items.advance_age(ticks);
        assert_eq!(ticks, 3);
        assert_eq!(age(&items), 3);
        assert_eq!(steps, 0);
    }

    #[test]
    fn section_bits_cover_the_indices_and_ignore_the_rest() {
        assert_eq!(section_bits(0..3), 0b111);
        assert_eq!(section_bits(0..0), 0);
        assert_eq!(section_bits([2, 5]), 0b100100);
        // A camera outside build height resolves to a section index no column
        // has; it must read as "not compiled", not shift out of range.
        assert_eq!(section_bit(-1), 0);
        assert_eq!(section_bit(32), 0);
        assert_eq!(section_bits(-3..-2), 0);
    }

    #[test]
    fn light_edits_group_only_adjacent_sections_in_one_column() {
        use azalea_core::position::ChunkPos;

        let a = ChunkPos::new(-2, 3);
        let b = ChunkPos::new(-2, 4);
        let c = ChunkPos::new(1, 3);
        // Shuffled keys, duplicate, separate columns and negative world Y
        // normalized against min_y = -64 (min_section_y = -4).
        let min_section_y = -64 >> 4;
        assert_eq!(
            consecutive_section_runs([
                (c, -2 - min_section_y),
                (a, 2),
                (b, 1),
                (a, 0),
                (c, -3 - min_section_y),
                (a, 1),
                (a, 0),
                (a, 5)
            ]),
            vec![(a, 0..3), (a, 5..6), (b, 1..2), (c, 1..3)]
        );
        assert!(consecutive_section_runs([]).is_empty());
    }

    #[test]
    fn grouped_edit_generation_rejects_old_results_per_section() {
        use std::collections::HashMap;
        use std::sync::Arc;

        use azalea_core::position::ChunkPos;

        use crate::world::chunk::{ChunkLightData, ChunkStore};

        let col = ChunkPos::new(-2, 3);
        let mut generations = HashMap::new();
        let mut next = 0;
        let first = bump_section_generations(&mut generations, &mut next, col, 0..3);
        assert!((0..3).all(|si| generations[&(col, si)] == first));
        assert!(!generations.contains_key(&(col, 3)));
        let mut chunks = ChunkStore::new_with_dimension(2, 16, -64);
        chunks.light_data.insert(
            (col.x, col.z),
            Arc::new(ChunkLightData {
                sky_sections: vec![],
                block_sections: vec![],
                min_y: -64,
                has_sky: false,
                sky_top_section: None,
            }),
        );
        assert!(!mesh_result_is_stale(
            &chunks,
            &HashMap::new(),
            &generations,
            col,
            first,
            0..3,
            true,
            0
        ));
        let second = bump_section_generations(&mut generations, &mut next, col, 1..2);
        assert!(second > first);
        assert!(mesh_result_is_stale(
            &chunks,
            &HashMap::new(),
            &generations,
            col,
            first,
            0..3,
            true,
            0
        ));
        assert!(!mesh_result_is_stale(
            &chunks,
            &HashMap::new(),
            &generations,
            col,
            first,
            0..1,
            true,
            0
        ));
        assert!(!mesh_result_is_stale(
            &chunks,
            &HashMap::new(),
            &generations,
            col,
            second,
            1..2,
            true,
            0
        ));
    }

    #[test]
    fn overlapping_grouped_edit_reschedules_only_still_current_sections() {
        use std::collections::HashMap;
        use std::sync::Arc;

        use azalea_core::position::ChunkPos;

        use crate::world::chunk::{ChunkLightData, ChunkStore};

        let col = ChunkPos::new(-2, 3);
        let mut chunks = ChunkStore::new_with_dimension(2, 16, -64);
        chunks.light_data.insert(
            (col.x, col.z),
            Arc::new(ChunkLightData {
                sky_sections: vec![],
                block_sections: vec![],
                min_y: -64,
                has_sky: false,
                sky_top_section: None,
            }),
        );
        let mut generations = HashMap::new();
        let mut next = 0;
        let old = bump_section_generations(&mut generations, &mut next, col, 0..2);
        let new = bump_section_generations(&mut generations, &mut next, col, 1..2);
        assert!(mesh_result_is_stale(
            &chunks,
            &HashMap::new(),
            &generations,
            col,
            old,
            0..2,
            true,
            0
        )); // The old group cannot upload section 1.
        let runs = current_edit_section_runs(&chunks, &generations, col, old, 0..2);
        assert_eq!(runs, vec![(col, 0..1)]);
        // The drain enqueues each run with a fresh generation and snapshot.
        for (_, range) in runs {
            let fresh = bump_section_generations(&mut generations, &mut next, col, range.clone());
            assert!(!mesh_result_is_stale(
                &chunks,
                &HashMap::new(),
                &generations,
                col,
                fresh,
                range,
                true,
                0
            )); // Section 0 can now upload.
        }
        assert_eq!(generations[&(col, 1)], new); // No old geometry or requeue for 1.
        assert!(!mesh_result_is_stale(
            &chunks,
            &HashMap::new(),
            &generations,
            col,
            new,
            1..2,
            true,
            0
        )); // The newer section 1 job remains valid.
        assert!(current_edit_section_runs(&chunks, &generations, col, old, 0..2).is_empty());

        // Disjoint survivors, including a negative section, must not requeue
        // the overlapping stale section or an unloaded column.
        let old = bump_section_generations(&mut generations, &mut next, col, -2..2);
        bump_section_generations(&mut generations, &mut next, col, -1..1);
        assert_eq!(
            current_edit_section_runs(&chunks, &generations, col, old, -2..2),
            vec![(col, -2..-1), (col, 1..2)]
        );
        chunks.light_data.remove(&(col.x, col.z));
        assert!(current_edit_section_runs(&chunks, &generations, col, old, -2..2).is_empty());
    }

    #[test]
    fn reload_keeps_bulk_generation_above_an_in_flight_old_snapshot() {
        use azalea_core::position::ChunkPos;
        let col = ChunkPos::new(-2, 1);
        let mut generations = std::collections::HashMap::new();
        let mut next = 0;
        let loaded = std::collections::HashSet::from([col]);
        for _ in 0..4 {
            bump_loaded_content_generations(&mut generations, &mut next, [col], &loaded);
        }
        let stale_mesh_gen = generations[&col];
        generations.remove(&col); // Unload releases the column entry.
        assert!(!generations.contains_key(&col));
        bump_loaded_content_generations(&mut generations, &mut next, [col], &loaded);
        assert!(stale_mesh_gen < generations[&col]);
        let mut chunks = crate::world::chunk::ChunkStore::new_with_dimension(2, 16, -64);
        chunks.light_data.insert(
            (col.x, col.z),
            std::sync::Arc::new(crate::world::chunk::ChunkLightData {
                sky_sections: vec![],
                block_sections: vec![],
                min_y: -64,
                has_sky: false,
                sky_top_section: None,
            }),
        );
        assert!(mesh_result_is_stale(
            &chunks,
            &generations,
            &std::collections::HashMap::new(),
            col,
            stale_mesh_gen,
            0..1,
            false,
            stale_mesh_gen,
        ));
    }

    #[test]
    fn unloaded_snapshot_arc_cannot_resurrect_bulk_mesh() {
        use std::collections::HashMap;
        use std::sync::Arc;

        use azalea_buf::AzBuf;
        use azalea_core::position::ChunkPos;

        use crate::world::chunk::{ChunkLightData, ChunkStore};

        let pos = ChunkPos::new(0, 0);
        let mut chunks = ChunkStore::new_with_dimension(2, 16, -64);
        let mut data = Vec::new();
        azalea_world::chunk::Section::default()
            .azalea_write(&mut data)
            .unwrap();
        chunks.load_chunk(pos, &data, &[]).unwrap();
        chunks.light_data.insert(
            (0, 0),
            Arc::new(ChunkLightData {
                sky_sections: vec![],
                block_sections: vec![],
                min_y: -64,
                has_sky: false,
                sky_top_section: None,
            }),
        );
        let snapshot = chunks.get_chunk(&pos).unwrap();
        let mut generations = HashMap::from([(pos, 1)]);
        let sections = HashMap::new();
        let edit_sections = HashMap::from([((pos, 0), 7)]);
        assert!(!mesh_result_is_stale(
            &chunks,
            &generations,
            &edit_sections,
            pos,
            7,
            0..1,
            true,
            1
        ));
        // ChunkLoaded replaces the contents through the SAME Arc, not a new identity.
        chunks.load_decoded_chunk(pos, azalea_world::Chunk::default());
        assert!(Arc::ptr_eq(&snapshot, &chunks.get_chunk(&pos).unwrap()));
        let mut next = 1;
        bump_loaded_content_generations(
            &mut generations,
            &mut next,
            [pos],
            &std::collections::HashSet::from([pos]),
        );
        assert!(mesh_result_is_stale(
            &chunks,
            &generations,
            &edit_sections,
            pos,
            7,
            0..1,
            true,
            1
        ));
        assert_eq!(
            current_edit_section_runs(&chunks, &edit_sections, pos, 7, 0..1),
            vec![(pos, 0..1)]
        );
        let mut edit_sections = edit_sections;
        let mut ticket = 7;
        let fresh = bump_section_generations(&mut edit_sections, &mut ticket, pos, 0..1);
        assert!(!mesh_result_is_stale(
            &chunks,
            &generations,
            &edit_sections,
            pos,
            fresh,
            0..1,
            true,
            next
        ));
        generations.insert(pos, 1); // Continue the bulk/unload assertions below.
        assert!(!mesh_result_is_stale(
            &chunks,
            &generations,
            &sections,
            pos,
            1,
            0..1,
            false,
            1
        ));
        chunks.unload_chunk(&pos);
        generations.remove(&pos);
        assert!(
            chunks.get_chunk(&pos).is_some(),
            "snapshot keeps weak chunk alive"
        );
        assert!(mesh_result_is_stale(
            &chunks,
            &generations,
            &sections,
            pos,
            1,
            0..1,
            false,
            1
        ));
        drop(snapshot);
    }

    #[test]
    fn unload_and_reload_dirty_the_remaining_3x3_dependency_set() {
        use std::collections::{HashMap, HashSet};

        use azalea_core::position::ChunkPos;

        let center = ChunkPos::new(0, 0);
        let loaded: HashSet<_> = crate::world::chunk::mesh_neighborhood(center)
            .into_iter()
            .collect();
        let mut generations = HashMap::new();
        let mut next = 0;
        assert_eq!(
            bump_loaded_content_generations(&mut generations, &mut next, [center], &loaded).len(),
            9
        );
        assert_eq!(generations.len(), 9);

        let mut after_unload = loaded.clone();
        after_unload.remove(&center);
        let before = generations.clone();
        generations.remove(&center);
        let dirty =
            bump_loaded_content_generations(&mut generations, &mut next, [center], &after_unload);
        assert_eq!(dirty.len(), 8);
        assert!(!generations.contains_key(&center));
        assert!(dirty.iter().all(|pos| generations[pos] > before[pos]));

        let before = generations.clone();
        let dirty = bump_loaded_content_generations(&mut generations, &mut next, [center], &loaded);
        assert_eq!(dirty.len(), 9);
        assert!(
            dirty
                .iter()
                .all(|pos| generations[pos] > before.get(pos).copied().unwrap_or(0))
        );
    }

    #[test]
    fn red_overlay_matches_vanilla_hurt_and_death_timers() {
        assert!(has_red_overlay(1, 0));
        assert!(has_red_overlay(0, 1));
        assert!(!has_red_overlay(0, 0));
    }

    #[test]
    fn immediate_respawn_flag_uses_official_zero_polarity() {
        assert!(show_death_screen_param(0.0));
        assert!(!show_death_screen_param(1.0));
        assert!(!show_death_screen_param(-1.0));
    }

    #[test]
    fn limited_crafting_flag_is_true_only_at_one() {
        assert!(limited_crafting_param(1.0));
        assert!(!limited_crafting_param(0.0));
        assert!(!limited_crafting_param(2.0));
    }

    #[test]
    fn win_game_ignores_its_parameter() {
        use azalea_protocol::packets::game::c_game_event::EventType;
        for param in [0.0, 1.0, -1.0, 99.0] {
            assert!(is_win_game_event(&EventType::WinGame, param));
        }
        assert!(!is_win_game_event(&EventType::ImmediateRespawn, 1.0));
    }

    #[test]
    fn completed_credits_closes_and_dispatches_callback_once() {
        let mut state = Some(crate::ui::menu::CreditsRollState::default());
        assert!(!finish_win_credits(&mut state, false));
        assert!(finish_win_credits(&mut state, true));
        assert!(!finish_win_credits(&mut state, true));
    }

    #[test]
    fn server_modal_owns_input_over_pending_credits() {
        let mut credits = Some(crate::ui::menu::CreditsRollState::default());
        assert!(!credits_may_advance(true));
        assert!(!finish_win_credits_if_allowed(&mut credits, true, true));
        assert!(credits.is_some());
        assert!(finish_win_credits_if_allowed(&mut credits, true, false));
        assert!(credits.is_none());
        assert!(!death_confirm_escape_allowed(true, true));
        assert!(death_confirm_escape_allowed(true, false));
    }

    #[test]
    fn minecart_cargo_uses_native_default_states_and_optional_override() {
        use azalea_registry::builtin::EntityKind as K;

        use super::minecart_cargo_state_for;
        crate::world::block::init("26.2");
        for (kind, block, facing) in [
            (K::ChestMinecart, "chest", Some("north")),
            (K::FurnaceMinecart, "furnace", Some("north")),
            (K::TntMinecart, "tnt", None),
            (K::HopperMinecart, "hopper", None),
            (K::CommandBlockMinecart, "command_block", None),
            (K::SpawnerMinecart, "spawner", None),
        ] {
            let state = minecart_cargo_state_for(kind, None, false).unwrap();
            assert_eq!(crate::world::block::block_id(state), block);
            if let Some(facing) = facing {
                assert_eq!(
                    crate::world::block::block_properties(state).get("facing"),
                    Some(facing)
                );
            }
        }
        assert!(minecart_cargo_state_for(K::Minecart, None, false).is_none());
        let furnace = minecart_cargo_state_for(K::FurnaceMinecart, None, true).unwrap();
        assert_eq!(
            crate::world::block::block_properties(furnace).get("lit"),
            Some("true")
        );
        let custom = crate::world::block::default_state_of("stone").unwrap();
        let override_state =
            minecart_cargo_state_for(K::ChestMinecart, Some(u32::from(custom)), false).unwrap();
        assert_eq!(override_state, custom);
        assert!(minecart_cargo_state_for(K::ChestMinecart, Some(u32::MAX), false).is_none());
    }
}
