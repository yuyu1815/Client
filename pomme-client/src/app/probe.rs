//! Opt-in, fixed-wait capture of the live client inputs, not a second world
//! reader.
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use azalea_core::position::ChunkPos;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::core::AppCore;
use super::phases::in_game::GameState;
use super::render_debug;
use crate::renderer::{Renderer, SkyState};
use crate::util::write_atomic;
use crate::world::block;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Region {
    min_x: i32,
    min_y: i32,
    min_z: i32,
    max_x: i32,
    max_y: i32,
    max_z: i32,
}

impl Region {
    fn parse(v: &Value, min_y: i32, height: u32) -> Result<Self> {
        let coordinate = |name: &str, lo: i64, hi: i64| -> Result<i32> {
            let n = v[name]
                .as_f64()
                .ok_or("Region coordinate must be numeric")?;
            if !n.is_finite() || n.fract() != 0.0 || n < lo as f64 || n > hi as f64 {
                return Err(format!("Invalid region coordinate {name}: {n}").into());
            }
            Ok(n as i32)
        };
        let r = Self {
            min_x: coordinate("minX", -29999999, 29999999)?,
            max_x: coordinate("maxX", -29999999, 29999999)?,
            min_z: coordinate("minZ", -29999999, 29999999)?,
            max_z: coordinate("maxZ", -29999999, 29999999)?,
            min_y: coordinate(
                "minY",
                i64::from(min_y).max(-2032),
                (i64::from(min_y) + i64::from(height) - 1).min(2031),
            )?,
            max_y: coordinate(
                "maxY",
                i64::from(min_y).max(-2032),
                (i64::from(min_y) + i64::from(height) - 1).min(2031),
            )?,
        };
        let mut volume = 1i64;
        for (min, max) in [(r.min_x, r.max_x), (r.min_y, r.max_y), (r.min_z, r.max_z)] {
            let length = i64::from(max) - i64::from(min) + 1;
            if !(1..=32768).contains(&length) || volume * length > 32768 {
                return Err("Region order/volume invalid (maximum 32768)".into());
            }
            volume *= length;
        }
        Ok(r)
    }
}

fn canonical_id(id: &str) -> String {
    if id.contains(':') {
        id.to_owned()
    } else {
        format!("minecraft:{id}")
    }
}

fn state_record(state: azalea_block::BlockState) -> Value {
    let id = canonical_id(block::block_id(state));
    let properties: BTreeMap<_, _> = block::block_properties(state).entries().collect();
    let pairs = properties
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(",");
    json!({"block": id, "properties": properties, "stateKey": format!("{id}[{pairs}]")})
}

fn biome_name(holder: &azalea_core::registry_holder::RegistryHolder, id: u32) -> Result<String> {
    let key = "minecraft:worldgen/biome".into();
    let registry = holder
        .extra
        .get(&key)
        .ok_or("Missing server biome registry")?;
    let (name, _) = registry
        .map
        .get_index(id as usize)
        .ok_or_else(|| format!("Missing server biome ID {id}"))?;
    Ok(name.to_string())
}

fn save_json(path: &Path, value: &Value) -> Result<()> {
    write_atomic(path, &serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn save_raw_capture(root: &Path, id: &str, mut raw: RawCapture) -> std::result::Result<(), String> {
    let world_path = root.join("results").join(format!("{id}.world-input.jsonl"));
    let debug_path = root.join("results").join(format!("{id}.render-debug.json"));
    let metadata_path = root.join("results").join(format!("{id}.json"));
    let mut text = Vec::new();
    for cell in raw.cells.drain(..) {
        let mut row = cell.state;
        row["x"] = json!(cell.x);
        row["y"] = json!(cell.y);
        row["z"] = json!(cell.z);
        row["dimension"] = raw.metadata["dimension"].clone();
        row["biome"] = json!(cell.biome);
        row["light"] = json!({"sky": cell.sky, "block": cell.block, "emission": cell.emission});
        serde_json::to_writer(&mut text, &row).map_err(|e| e.to_string())?;
        text.push(b'\n');
    }
    write_atomic(&world_path, &text).map_err(|e| e.to_string())?;
    write_atomic(
        &debug_path,
        &serde_json::to_vec_pretty(&raw.debug).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    raw.metadata["worldInput"] = json!(world_path);
    raw.metadata["renderDebug"] = json!(debug_path);
    save_json(&metadata_path, &raw.metadata).map_err(|e| e.to_string())
}

fn safe_case(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}

struct RawCell {
    x: i32,
    y: i32,
    z: i32,
    state: Value,
    biome: String,
    sky: u8,
    block: u8,
    emission: u8,
}

struct RawCapture {
    metadata: Value,
    cells: Vec<RawCell>,
    debug: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PeerFilter {
    pub mode: String,
    pub peer_name: String,
    #[serde(default)]
    pub peer_uuid: Option<uuid::Uuid>,
    pub provenance: String,
}

pub(crate) fn should_exclude_peer(
    filter: Option<&PeerFilter>,
    entity_id: i32,
    local_entity_id: i32,
    entity_uuid: Option<uuid::Uuid>,
    entity_name: Option<&str>,
) -> bool {
    let Some(filter) = filter else { return false };
    filter.mode == "paired"
        && entity_id != local_entity_id
        && entity_name == Some(filter.peer_name.as_str())
        && filter
            .peer_uuid
            .is_none_or(|expected| entity_uuid == Some(expected))
}

#[derive(Clone)]
struct ItemOverlayLayout {
    metadata: Value,
    panel: [f32; 4],
    background: [f32; 4],
    items: Vec<(String, [f32; 4])>,
}

fn fixed_rect(value: &Value, name: &str) -> Result<[f32; 4]> {
    let values = value
        .get(name)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("itemOverlay missing {name}"))?;
    if values.len() != 4 {
        return Err(format!("itemOverlay {name} must have four values").into());
    }
    let mut rect = [0.0; 4];
    for (out, value) in rect.iter_mut().zip(values) {
        let n = value.as_f64().ok_or("itemOverlay rect must be numeric")?;
        if !n.is_finite() || n.fract() != 0.0 || n < 0.0 || n > 4096.0 {
            return Err("itemOverlay rect must contain bounded integers".into());
        }
        *out = n as f32;
    }
    if rect[2] <= 0.0 || rect[3] <= 0.0 {
        return Err(format!("itemOverlay {name} must be non-empty").into());
    }
    Ok(rect)
}

fn parse_item_overlay(request: &Value) -> Result<Option<ItemOverlayLayout>> {
    let Some(value) = request.get("itemOverlay") else {
        return Ok(None);
    };
    let object = value.as_object().ok_or("itemOverlay must be an object")?;
    if object.get("mode").and_then(Value::as_str) != Some("gui") {
        return Err("itemOverlay mode must be gui".into());
    }
    let gui_scale = object
        .get("guiScale")
        .and_then(Value::as_u64)
        .filter(|scale| (1..=8).contains(scale))
        .ok_or("itemOverlay guiScale must be 1..8")?;
    let panel = fixed_rect(value, "panelPhysicalRect")?;
    let background_rgb = value
        .get("backgroundRGB")
        .and_then(Value::as_array)
        .ok_or("itemOverlay missing backgroundRGB")?;
    if background_rgb.len() != 3 {
        return Err("itemOverlay backgroundRGB must have three values".into());
    }
    let mut background = [0.0; 4];
    for (out, value) in background[..3].iter_mut().zip(background_rgb) {
        let n = value.as_u64().filter(|n| *n <= 255).ok_or("invalid backgroundRGB")?;
        let encoded = n as f32 / 255.0;
        // MenuOverlayPipeline writes to the SRGB swapchain; feed it the exact
        // linear value for the requested neutral byte so Java/Rust panel pixels
        // are compared before item texture differences.
        *out = if encoded <= 0.04045 {
            encoded / 12.92
        } else {
            ((encoded + 0.055) / 1.055).powf(2.4)
        };
    }
    background[3] = 1.0;
    let items = value
        .get("items")
        .and_then(Value::as_array)
        .ok_or("itemOverlay missing items")?;
    if !(1..=12).contains(&items.len()) {
        return Err("itemOverlay requires 1..=12 items".into());
    }
    let allowed = [
        "fern", "bush", "lily_pad", "sugar_cane", "pink_petals", "wildflowers",
        "ice", "honey_block", "stone",
    ];
    let mut parsed = Vec::with_capacity(items.len());
    for item in items {
        let id = item
            .get("id")
            .and_then(Value::as_str)
            .ok_or("itemOverlay item missing id")?;
        if !allowed.contains(&id) || parsed.iter().any(|(seen, _)| seen == id) {
            return Err(format!("unsupported or duplicate itemOverlay id {id}").into());
        }
        let rect = fixed_rect(item, "physicalRect")?;
        if rect[2] != 16.0 * gui_scale as f32 || rect[3] != 16.0 * gui_scale as f32 {
            return Err(format!("itemOverlay {id} physical rect is not 16x16 at guiScale {gui_scale}").into());
        }
        parsed.push((id.to_owned(), rect));
    }
    Ok(Some(ItemOverlayLayout {
        metadata: value.clone(),
        panel,
        background,
        items: parsed,
    }))
}

enum PendingCapture {
    Screenshot {
        id: String,
        rx: Receiver<std::result::Result<crate::renderer::ProbeScreenshotReply, String>>,
        raw: Option<RawCapture>,
    },
    Save {
        id: String,
        rx: Receiver<std::result::Result<(), String>>,
    },
}

pub struct Probe {
    root: PathBuf,
    server: Option<String>,
    next_poll: Instant,
    seen: HashSet<String>,
    states: Vec<Value>,
    pending: Option<PendingCapture>,
    paired_prepared: Option<(String, Value)>,
    paired_prepared_debug: Option<Value>,
    paired_prepared_halo_token: Option<String>,
    paired_target: Option<(String, Instant)>,
    paired_seen: HashSet<String>,
    peer_filter: Option<PeerFilter>,
    item_overlay: Option<ItemOverlayLayout>,
}

impl Probe {
    pub fn new(root: PathBuf, server: Option<String>) -> Self {
        let peer_filter = std::fs::read(root.join("peer-filter.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<PeerFilter>(&bytes).ok());
        if peer_filter.is_none() && root.join("peer-filter.json").is_file() {
            tracing::warn!(target = "renderprobe", "Ignoring invalid peer-filter.json");
        }
        Self {
            root,
            server,
            next_poll: Instant::now(),
            seen: HashSet::new(),
            states: Vec::new(),
            pending: None,
            paired_prepared: None,
            paired_prepared_debug: None,
            paired_prepared_halo_token: None,
            paired_target: None,
            paired_seen: HashSet::new(),
            peer_filter,
            item_overlay: None,
        }
    }

    pub(crate) fn peer_filter(&self) -> Option<&PeerFilter> {
        self.peer_filter.as_ref()
    }

    pub(crate) fn peer_filter_metadata(&self) -> Value {
        self.peer_filter
            .as_ref()
            .map_or(Value::Null, |filter| json!(filter))
    }

    pub fn exit_requested(&self) -> bool {
        self.root.join("exit-request.json").is_file()
    }

    pub(crate) fn item_overlay_elements(&self) -> Vec<crate::renderer::pipelines::menu_overlay::MenuElement> {
        let Some(layout) = &self.item_overlay else { return Vec::new() };
        let mut elements = Vec::with_capacity(layout.items.len() + 1);
        elements.push(crate::renderer::pipelines::menu_overlay::MenuElement::Rect {
            x: layout.panel[0],
            y: layout.panel[1],
            w: layout.panel[2],
            h: layout.panel[3],
            corner_radius: 0.0,
            color: layout.background,
        });
        for (item_name, rect) in &layout.items {
            elements.push(crate::renderer::pipelines::menu_overlay::MenuElement::ItemIcon {
                x: rect[0],
                y: rect[1],
                w: rect[2],
                h: rect[3],
                item_name: item_name.clone(),
                tint: [1.0, 1.0, 1.0, 1.0],
            });
        }
        elements
    }

    pub(crate) fn item_overlay_metadata(&self) -> Value {
        self.item_overlay
            .as_ref()
            .map_or(Value::Null, |layout| layout.metadata.clone())
    }

    pub(crate) fn item_overlay_active(&self) -> bool {
        self.item_overlay.is_some()
    }

    fn path(&self, id: &str, suffix: &str) -> PathBuf {
        self.root.join("results").join(format!("{id}{suffix}"))
    }

    fn fail_paired(&self, request: &Value, id: &str, error: impl std::fmt::Display) {
        tracing::error!("Probe {id}: {error}");
        let value = json!({
            "schema": 1, "status": "failed", "runId": request["runId"], "caseId": request["caseId"],
            "attemptId": request["attemptId"], "resultId": request["resultId"], "error": error.to_string()
        });
        if let Err(e) = save_json(&self.path(id, ".failed.json"), &value) {
            tracing::error!("Cannot publish paired probe failure: {e}");
        }
    }

    fn fail(&self, id: &str, error: impl std::fmt::Display) {
        tracing::error!("Probe {id}: {error}");
        if let Err(e) = save_json(
            &self.path(id, ".failed.json"),
            &json!({"caseId": id, "error": error.to_string()}),
        ) {
            tracing::error!("Cannot publish probe failure: {e}");
        }
    }

    pub fn poll(
        &mut self,
        core: &AppCore,
        renderer: &mut Renderer,
        game: &mut GameState,
        sky: &SkyState,
    ) {
        let request_path = self.root.join("capture-request.json");
        let paired_request = std::fs::read(&request_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .is_some_and(|request| request["schema"].as_i64() == Some(3));
        if !paired_request && Instant::now() < self.next_poll {
            return;
        }
        self.next_poll = Instant::now() + Duration::from_millis(500);
        if let Some(pending) = self.pending.take() {
            match pending {
                PendingCapture::Screenshot { id, rx, raw } => match rx.try_recv() {
                    Err(TryRecvError::Empty) => {
                        self.pending = Some(PendingCapture::Screenshot { id, rx, raw });
                        return;
                    }
                    Err(TryRecvError::Disconnected) => {
                        self.fail(&id, "Screenshot worker disconnected")
                    }
                    Ok(Err(e)) => self.fail(&id, e),
                    Ok(Ok(reply)) => {
                        if let Some(mut raw) = raw {
                            raw.metadata["actualFrameCapturedAt"] =
                                json!(reply.actual_frame_captured_at);
                            raw.metadata["frameReadbackCompletedAt"] =
                                json!(reply.frame_readback_completed_at);
                            raw.metadata["captureFrame"] = json!(reply.frame);
                            let (tx, save_rx) = std::sync::mpsc::channel();
                            let root = self.root.clone();
                            let save_id = id.clone();
                            std::thread::spawn(move || {
                                let result = save_raw_capture(&root, &save_id, raw);
                                let _ = tx.send(result);
                            });
                            self.pending = Some(PendingCapture::Save { id, rx: save_rx });
                        } else {
                            self.finish_saved(&id);
                        }
                    }
                },
                PendingCapture::Save { id, rx } => match rx.try_recv() {
                    Err(TryRecvError::Empty) => {
                        self.pending = Some(PendingCapture::Save { id, rx });
                        return;
                    }
                    Err(TryRecvError::Disconnected) => {
                        self.fail(&id, "Probe save worker disconnected")
                    }
                    Ok(Err(e)) => self.fail(&id, e),
                    Ok(Ok(())) => self.finish_saved(&id),
                },
            }
        }
        if !request_path.is_file() {
            return;
        }
        let request: Value = match std::fs::read(&request_path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| serde_json::from_slice(&bytes).map_err(|e| e.to_string()))
        {
            Ok(request) => request,
            Err(e) => {
                self.fail("request", e);
                return;
            }
        };
        if request["schema"].as_i64() == Some(3) {
            self.poll_paired(&request, core, renderer, game, sky);
            return;
        }
        let mut id = "request".to_owned();
        let result = (|| -> Result<()> {
            let case = request["caseId"].as_str().ok_or("Missing caseId")?;
            if !safe_case(case) {
                return Err("Unsafe caseId".into());
            }
            id = case.to_owned();
            if !self.seen.insert(id.clone()) {
                return Ok(());
            }
            std::fs::create_dir_all(self.root.join("results"))?;
            if request["schema"] != 2 {
                return Err("Expected request schema 2".into());
            }
            self.capture(&id, &request, core, renderer, game, sky)
        })();
        if let Err(e) = result {
            self.fail(&id, e);
        }
    }

    fn paired_debug_samples(
        region: &Region,
        chunks: &crate::world::chunk::ChunkStore,
    ) -> Vec<(i32, i32, i32, azalea_block::BlockState)> {
        let names = [
            "stone",
            "oak_stairs",
            "glass_pane",
            "cobblestone_wall",
            "stripped_oak_log",
            "stripped_acacia_log",
            "stripped_cherry_log",
            "stripped_dark_oak_log",
            "stripped_mangrove_log",
            "stripped_pale_oak_log",
            "pumpkin_stem",
            "melon_stem",
            "attached_pumpkin_stem",
            "attached_melon_stem",
            "potted_fern",
            "bush",
            "sugar_cane",
            "lily_pad",
            "pink_petals",
            "wildflowers",
            "cherry_leaves",
            "mangrove_propagule",
            "azalea_leaves",
            "flowering_azalea_leaves",
            "spruce_leaves",
            "birch_leaves",
            "water",
            "lava",
            "bubble_column",
        ];
        let mut seen = HashSet::new();
        let mut samples = Vec::new();
        for x in region.min_x..=region.max_x {
            for y in region.min_y..=region.max_y {
                for z in region.min_z..=region.max_z {
                    let state = chunks.get_block_state(x, y, z);
                    let key = state_record(state)["stateKey"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned();
                    if names.contains(&block::block_id(state))
                        && seen.insert(key)
                        && samples.len() < 16
                    {
                        samples.push((x, y, z, state));
                    }
                }
            }
        }
        samples
    }

    fn world_halo_token(
        chunks: &crate::world::chunk::ChunkStore,
        region: &Region,
    ) -> Result<String> {
        const PROVENANCE: &[u8] = b"world-halo-token-v2/state-u32le/raw-sky-u8/raw-block-u8/biome-u32le/order=x-asc,y-asc,z-asc/halo=xz+-2/no-fallback";
        let min_x = region
            .min_x
            .checked_sub(2)
            .ok_or("World halo coordinate overflow")?;
        let max_x = region
            .max_x
            .checked_add(2)
            .ok_or("World halo coordinate overflow")?;
        let min_z = region
            .min_z
            .checked_sub(2)
            .ok_or("World halo coordinate overflow")?;
        let max_z = region
            .max_z
            .checked_add(2)
            .ok_or("World halo coordinate overflow")?;
        let mut digest = Sha256::new();
        digest.update(PROVENANCE);
        for value in [
            region.min_x,
            region.min_y,
            region.min_z,
            region.max_x,
            region.max_y,
            region.max_z,
        ] {
            digest.update(value.to_le_bytes());
        }
        for x in min_x..=max_x {
            for z in min_z..=max_z {
                let chunk_pos = ChunkPos::new(x.div_euclid(16), z.div_euclid(16));
                if chunks.get_chunk(&chunk_pos).is_none()
                    || !chunks.light_data.contains_key(&(chunk_pos.x, chunk_pos.z))
                {
                    return Err(format!(
                        "World halo input is unloaded at chunk {},{}",
                        chunk_pos.x, chunk_pos.z
                    )
                    .into());
                }
            }
        }
        for x in min_x..=max_x {
            for y in region.min_y..=region.max_y {
                for z in min_z..=max_z {
                    let state = chunks.get_block_state(x, y, z);
                    let biome = chunks
                        .biome_id_checked(x, y, z)
                        .ok_or_else(|| format!("World halo biome is unloaded at {x},{y},{z}"))?;
                    digest.update(u32::from(state).to_le_bytes());
                    digest.update([chunks.get_sky_light(x, y, z)]);
                    digest.update([chunks.get_block_light(x, y, z)]);
                    digest.update(biome.to_le_bytes());
                }
            }
        }
        Ok(digest
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect())
    }

    fn poll_paired(
        &mut self,
        request: &Value,
        core: &AppCore,
        renderer: &mut Renderer,
        game: &mut GameState,
        sky: &SkyState,
    ) {
        let result_id = request["resultId"].as_str().unwrap_or("request").to_owned();
        let phase = request["phase"].as_str().unwrap_or("");
        for field in ["runId", "caseId", "attemptId", "resultId", "phase"] {
            if !request[field].is_string() {
                self.fail_paired(
                    request,
                    &result_id,
                    format!("Missing paired identity field {field}"),
                );
                return;
            }
        }
        if !safe_case(&result_id) || !safe_case(request["caseId"].as_str().unwrap_or("")) {
            self.fail_paired(request, &result_id, "Unsafe paired case/result ID");
            return;
        }
        if self.paired_seen.contains(&result_id)
            || self.path(&result_id, ".complete.json").is_file()
            || self.path(&result_id, ".failed.json").is_file()
        {
            return;
        }
        if phase == "prepare" {
            if self
                .paired_prepared
                .as_ref()
                .is_some_and(|(id, _)| id == &result_id)
            {
                return;
            }
            let requested_item_overlay = match parse_item_overlay(request) {
                Ok(layout) => layout,
                Err(e) => {
                    self.fail_paired(request, &result_id, e);
                    return;
                }
            };
            match self.validate_capture(request, renderer, game) {
                Ok(region) => {
                    if let Some(layout) = requested_item_overlay {
                        game.hide_gui = false;
                        renderer.arm_gui_item_draw_trace(layout.metadata.clone());
                        self.item_overlay = Some(layout);
                    }
                    if let Err(e) = self.prepare_state_inventory() {
                        self.fail_paired(request, &result_id, e);
                        return;
                    }
                    let samples = Self::paired_debug_samples(&region, &game.chunk_store);
                    let mut prepared_debug =
                        render_debug::prepare(renderer, &game.chunk_store, &samples);
                    let halo_token = match Self::world_halo_token(&game.chunk_store, &region) {
                        Ok(token) => token,
                        Err(e) => {
                            self.fail_paired(request, &result_id, e);
                            return;
                        }
                    };
                    if let Some(sampling) = prepared_debug
                        .get_mut("diagnosticSampling")
                        .and_then(Value::as_object_mut)
                    {
                        sampling.insert("worldHaloTokenProvenance".into(), json!("world-halo-token-v2: target region plus x/z +/-2 for vanilla 5x5 biome tint blend; fixed x/y/z order; raw state ID, raw sky light, raw block light, biome ID; unloaded input rejects token"));
                        sampling.insert("fluidNeighborWorldHaloToken".into(), json!(halo_token));
                        sampling.insert("preparedWorldHaloToken".into(), json!(halo_token));
                        sampling.insert("actualDrawDiagnostics".into(), json!("armed: mesher/upload payload, no GPU readback"));
                    }
                    let trace_samples = Self::paired_debug_samples(&region, &game.chunk_store);
                    renderer.arm_actual_draw_trace(&result_id, &halo_token, &trace_samples);
                    game.remesh_probe_targets(renderer, &trace_samples);
                    let ready = json!({
                        "schema": 1,
                        "runId": request["runId"],
                        "caseId": request["caseId"],
                        "attemptId": request["attemptId"],
                        "resultId": result_id,
                        "client": "rust",
                        "readyAt": chrono::Utc::now().to_rfc3339(),
                    });
                    if let Err(e) = save_json(&self.path(&result_id, ".ready.json"), &ready) {
                        self.fail_paired(request, &result_id, e);
                    } else {
                        self.paired_prepared = Some((result_id, request.clone()));
                        self.paired_prepared_debug = Some(prepared_debug);
                        self.paired_prepared_halo_token = Some(halo_token);
                    }
                }
                Err(e) => tracing::debug!("Paired prepare waiting: {e}"),
            }
            return;
        }
        if phase != "go" {
            self.fail_paired(request, &result_id, "Unknown paired phase");
            return;
        }
        let Some((prepared_id, prepared)) = self.paired_prepared.as_ref() else {
            self.fail_paired(request, &result_id, "GO received without matching prepare");
            return;
        };
        if prepared_id != &result_id
            || ["runId", "caseId", "attemptId", "resultId"]
                .iter()
                .any(|field| prepared[*field] != request[*field])
        {
            self.fail_paired(request, &result_id, "GO does not match prepared identity");
            return;
        }
        let target_text = request["targetCaptureAt"].as_str().unwrap_or("").to_owned();
        let target = match chrono::DateTime::parse_from_rfc3339(&target_text) {
            Ok(value) => value.with_timezone(&chrono::Utc),
            Err(e) => {
                self.fail_paired(request, &result_id, e);
                return;
            }
        };
        let deadline = if let Some((id, deadline)) = &self.paired_target {
            if id == &result_id {
                *deadline
            } else {
                self.paired_target = None;
                Instant::now()
            }
        } else {
            let wait = (target - chrono::Utc::now()).to_std().unwrap_or_default();
            let deadline = Instant::now() + wait;
            self.paired_target = Some((result_id.clone(), deadline));
            deadline
        };
        if Instant::now() < deadline {
            return;
        }
        self.paired_seen.insert(result_id.clone());
        if let Err(e) = self.capture_paired(&result_id, request, core, renderer, game, sky) {
            self.fail_paired(request, &result_id, e);
        }
        self.paired_prepared = None;
        self.paired_prepared_debug = None;
        self.paired_prepared_halo_token = None;
        self.paired_target = None;
    }

    fn validate_capture(
        &self,
        request: &Value,
        renderer: &Renderer,
        game: &GameState,
    ) -> Result<Region> {
        if request["dimension"].as_str() != Some(game.dimension.as_str()) {
            return Err("Requested/actual dimension mismatch".into());
        }
        if game.paused
            || game.gui_open()
            || game.chat.is_open()
            || game.dead
            || game.options_from_game
            || game.dialog_open()
        {
            return Err(format!("Close client screen before capture: paused={}, gui={}, chat={}, dead={}, options={}, dialog={}",
                game.paused, game.gui_open(), game.chat.is_open(), game.dead, game.options_from_game, game.dialog_open()).into());
        }
        let item_overlay = parse_item_overlay(request)?;
        if !renderer.is_first_person() || (!game.hide_gui && item_overlay.is_none()) {
            return Err("Probe requires first person and hidden HUD unless paired itemOverlay mode is active".into());
        }
        let p = game.player.position;
        let (yaw, pitch) = renderer.camera_effective_look_deg();
        let expected_yaw = request["yaw"].as_f64().ok_or("Missing paired yaw")?;
        let yaw_delta = ((f64::from(yaw) - expected_yaw + 540.0) % 360.0) - 180.0;
        if (p.x - request["x"].as_f64().ok_or("Missing paired x")?).abs() > 0.01
            || (p.y - request["y"].as_f64().ok_or("Missing paired y")?).abs() > 0.01
            || (p.z - request["z"].as_f64().ok_or("Missing paired z")?).abs() > 0.01
            || yaw_delta.abs() > 0.1
            || (f64::from(pitch) - request["pitch"].as_f64().ok_or("Missing paired pitch")?).abs()
                > 0.01
        {
            return Err("Camera/input state is not ready".into());
        }
        let chunks = &game.chunk_store;
        let region = Region::parse(&request["region"], chunks.min_y(), chunks.height())?;
        for x in (region.min_x - 4) >> 4..=(region.max_x + 4) >> 4 {
            for z in (region.min_z - 4) >> 4..=(region.max_z + 4) >> 4 {
                if chunks.get_chunk(&ChunkPos::new(x, z)).is_none() {
                    return Err(format!(
                        "Missing client chunk {x},{z}; increase waitSeconds or move region closer"
                    )
                    .into());
                }
                if !chunks.light_data.contains_key(&(x, z))
                    || !game.light_engine.light_on_in_column((x, z))
                {
                    return Err(
                        format!("Missing light column {x},{z}; increase waitSeconds").into(),
                    );
                }
            }
        }
        Ok(region)
    }

    fn finish_saved(&self, id: &str) {
        let metadata_path = self.path(id, ".json");
        let loaded = std::fs::read(&metadata_path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).map_err(|e| e.to_string()));
        match loaded {
            Ok(mut metadata) => {
                metadata["saveCompletedAt"] = json!(chrono::Utc::now().to_rfc3339());
                if let Err(e) = save_json(&metadata_path, &metadata) {
                    self.fail(id, e);
                    return;
                }
                let notice = json!({
                    "schema": 1, "status": "complete", "caseId": metadata["caseId"].as_str().unwrap_or(id),
                    "runId": metadata["runId"], "attemptId": metadata["attemptId"], "resultId": id,
                    "png": self.path(id, ".png"), "metadata": metadata_path,
                    "completedAt": chrono::Utc::now().to_rfc3339()
                });
                if let Err(e) = save_json(&self.path(id, ".complete.json"), &notice) {
                    self.fail(id, e);
                } else {
                    tracing::info!("Probe capture complete: {id}");
                }
            }
            Err(e) => self.fail(id, e),
        }
    }

    fn prepare_state_inventory(&mut self) -> Result<()> {
        if !self.states.is_empty() {
            return Ok(());
        }
        let states: Vec<_> = (0..)
            .map_while(block::try_state)
            .map(state_record)
            .collect();
        let mut sorted: Vec<_> = states.iter().collect();
        sorted.sort_by_key(|s| s["stateKey"].as_str().unwrap());
        if sorted
            .windows(2)
            .any(|s| s[0]["stateKey"] == s[1]["stateKey"])
        {
            return Err("Duplicate canonical state".into());
        }
        let mut text = Vec::new();
        for row in sorted {
            serde_json::to_writer(&mut text, row)?;
            text.push(b'\n');
        }
        write_atomic(&self.root.join("results/states.jsonl"), &text)?;
        self.states = states;
        Ok(())
    }

    fn capture_paired(
        &mut self,
        id: &str,
        request: &Value,
        core: &AppCore,
        renderer: &mut Renderer,
        game: &mut GameState,
        sky: &SkyState,
    ) -> Result<()> {
        let region = self.validate_capture(request, renderer, game)?;
        self.prepare_state_inventory()?;
        let prepared_debug = self
            .paired_prepared_debug
            .clone()
            .ok_or("Paired GO has no prepared diagnostics")?;
        let prepared_halo_token = self
            .paired_prepared_halo_token
            .as_deref()
            .ok_or("Paired GO has no prepared world halo token")?;
        let actual_halo_token = Self::world_halo_token(&game.chunk_store, &region)?;
        if actual_halo_token != prepared_halo_token {
            return Err("Prepared target-region state/light/biome halo changed before GO".into());
        }
        let actual_snapshot_at = chrono::Utc::now().to_rfc3339();
        let chunks = &game.chunk_store;
        let clock_token = (
            game.sky_state.clock_id,
            game.sky_state.day_time,
            game.sky_state.clock_partial_tick,
            game.sky_state.clock_rate,
        );
        let rx = renderer.request_probe_screenshot(self.path(id, ".png"))?;
        let mut cells = Vec::new();
        for x in region.min_x..=region.max_x {
            for y in region.min_y..=region.max_y {
                for z in region.min_z..=region.max_z {
                    let state = chunks.get_block_state(x, y, z);
                    let state_json = self
                        .states
                        .get(u32::from(state) as usize)
                        .ok_or("Unknown block state ID")?
                        .clone();
                    let biome = biome_name(
                        &game.registries,
                        chunks
                            .biome_id_checked(x, y, z)
                            .ok_or("Missing biome cell")?,
                    )?;
                    cells.push(RawCell {
                        x,
                        y,
                        z,
                        state: state_json,
                        biome,
                        sky: chunks.get_sky_light(x, y, z),
                        block: chunks.get_block_light(x, y, z),
                        emission: block::light_props(state).emission,
                    });
                }
            }
        }
        let after_token = (
            game.sky_state.clock_id,
            game.sky_state.day_time,
            game.sky_state.clock_partial_tick,
            game.sky_state.clock_rate,
        );
        if clock_token != after_token {
            return Err("Clock/region snapshot changed during paired capture; reprepare".into());
        }
        let actual_halo_token = Self::world_halo_token(chunks, &region)?;
        if actual_halo_token != prepared_halo_token {
            return Err(
                "Target-region state/light/biome halo changed during raw capture; reprepare".into(),
            );
        }
        let mut debug = render_debug::snapshot_with_prepared(
            renderer,
            sky,
            &game.dimension,
            if game.server_render_distance > 0 {
                core.menu.render_distance.min(game.server_render_distance)
            } else {
                core.menu.render_distance
            },
            game.probe_lightmap_brightness(),
            &prepared_debug,
        );
        if let Some(sampling) = debug
            .get_mut("diagnosticSampling")
            .and_then(Value::as_object_mut)
        {
            sampling.insert("actualWorldRawSampledAt".into(), json!(actual_snapshot_at));
            sampling.insert("actualWorldHaloToken".into(), json!(actual_halo_token));
            sampling.insert("preparedWorldHaloToken".into(), json!(prepared_halo_token));
            sampling.insert(
                "worldInputChanged".into(),
                json!(actual_halo_token != prepared_halo_token),
            );
            sampling.insert("actualDrawDiagnostics".into(), json!("captured emitted/upload payload; GPU fragment/readback not captured"));
        }
        let p = game.player.position;
        let c = renderer.camera_render_position();
        let (yaw, pitch) = renderer.camera_effective_look_deg();
        let (vendor, driver) = renderer.probe_gpu_info();
        let excluded_actor_count = game
            .entity_store
            .living
            .iter()
            .filter(|entry| {
                let entity_id = *entry.0;
                let entity = entry.1;
                let name = entity.player_uuid.and_then(|uuid| {
                    game.tab_list.players.get(&uuid).map(|player| player.name.as_str())
                });
                should_exclude_peer(
                    self.peer_filter.as_ref(),
                    entity_id,
                    game.player.entity_id,
                    entity.player_uuid,
                    name,
                )
            })
            .count();
        let mut excluded_peer = self.peer_filter_metadata();
        if let Some(peer) = excluded_peer.as_object_mut() {
            let resolved_uuid = game.entity_store.living.values().find_map(|entity| {
                let uuid = entity.player_uuid?;
                let name = game.tab_list.players.get(&uuid)?.name.as_str();
                (name == peer.get("peerName").and_then(Value::as_str).unwrap_or_default()).then_some(uuid)
            });
            peer.insert("resolvedPeerUuid".into(), json!(resolved_uuid));
        }
        let metadata = json!({
            "schema": 3, "gameVersion": core.version, "launchVersion": "Pomme", "server": self.server,
            "runId": request["runId"], "caseId": request["caseId"], "attemptId": request["attemptId"], "resultId": id,
            "requestedAt": request["requestedAt"], "targetCaptureAt": request["targetCaptureAt"], "expectedTime": request["expectedTime"], "actualSnapshotAt": actual_snapshot_at,
            "dimension": game.dimension, "playerX": p.x, "playerY": p.y, "playerZ": p.z,
            "playerYaw": game.player.look_dir.y_rot_deg(), "playerPitch": game.player.look_dir.x_rot_deg(),
            "cameraX": c.x, "cameraY": c.y, "cameraZ": c.z, "cameraYaw": yaw, "cameraPitch": pitch,
            "fov": renderer.camera_fov_degrees(), "width": renderer.screen_width(), "height": renderer.screen_height(),
            "gpuName": renderer.gpu_name(), "gpuVendor": format!("PCI 0x{vendor:04x}"), "backend": "Vulkan", "driver": format!("Vulkan driverVersion {driver}"), "vulkanApi": renderer.vulkan_version(),
            "capturedAt": actual_snapshot_at, "cameraMode": "FIRST_PERSON", "hudHidden": game.hide_gui, "showHand": !game.hide_gui, "captureSource": "Vulkan swapchain PresentSrcKHR -> TransferSrcOptimal -> host readback PNG", "viewBobbing": core.menu.view_bobbing,
            "clock": {"id": game.sky_state.clock_id, "totalTicks": game.sky_state.day_time, "partialTick": game.sky_state.clock_partial_tick, "rate": game.sky_state.clock_rate},
            "clockPhase": game.sky_state.day_tick().rem_euclid(24000.0), "serverTickRate": core.server_tick_rate, "serverFrozen": core.server_tick_frozen,
            "serverFrozenTicksToRun": core.server_tick_steps, "clientGameTime": game.sky_state.game_time, "clientTime": game.sky_state.day_time as f64 + f64::from(game.sky_state.clock_partial_tick),
            "clientDaytime": game.sky_state.day_tick(), "rendererSkyTime": sky.day_time as f64 + f64::from(sky.clock_partial_tick + sky.partial_tick * sky.clock_rate),
            "region": region, "recordCount": cells.len(), "stateInventory": self.root.join("results/states.jsonl"), "stateCount": self.states.len(),
            "excludedProbePeer": excluded_peer,
            "actualDrawEntityList": {"actorCount": game.entity_store.living.len(), "excludedProbePeerActorCount": excluded_actor_count, "drawEligibleActorCount": game.entity_store.living.len().saturating_sub(excluded_actor_count), "provenance": "GameState EntityStore at paired capture; filter is exact PlayerInfo name plus optional UUID; no all-entity suppression"},
            "snapshotTiming": "actualSnapshotAt is the target CPU raw snapshot; actualFrameCapturedAt is Vulkan copy recording; frameReadbackCompletedAt is readback completion",
            "diagnosticMode": if self.item_overlay.is_some() { "itemOverlay" } else { "normal" },
            "itemOverlay": self.item_overlay_metadata(),
            "itemOverlayColorSpace": "Rust UI color floats -> B8G8R8A8_SRGB framebuffer; no post-capture correction"
        });
        self.pending = Some(PendingCapture::Screenshot {
            id: id.to_owned(),
            rx,
            raw: Some(RawCapture {
                metadata,
                cells,
                debug,
            }),
        });
        Ok(())
    }

    fn capture(
        &mut self,
        id: &str,
        request: &Value,
        core: &AppCore,
        renderer: &mut Renderer,
        game: &GameState,
        sky: &SkyState,
    ) -> Result<()> {
        let region = self.validate_capture(request, renderer, game)?;
        self.prepare_state_inventory()?;
        let chunks = &game.chunk_store;
        // ponytail: bounded 32768-cell snapshot/write on game thread; shrink regions if
        // tick latency matters.
        let mut text = Vec::new();
        let mut count = 0;
        let debug_names = [
            "stone",
            "oak_stairs",
            "glass_pane",
            "cobblestone_wall",
            "stripped_oak_log",
            "stripped_acacia_log",
            "stripped_cherry_log",
            "stripped_dark_oak_log",
            "stripped_mangrove_log",
            "stripped_pale_oak_log",
            "pumpkin_stem",
            "melon_stem",
            "attached_pumpkin_stem",
            "attached_melon_stem",
            "potted_fern",
            "bush",
            "sugar_cane",
            "lily_pad",
            "pink_petals",
            "wildflowers",
            "cherry_leaves",
            "mangrove_propagule",
            "azalea_leaves",
            "flowering_azalea_leaves",
            "spruce_leaves",
            "birch_leaves",
            "water",
            "lava",
            "bubble_column",
        ];
        let mut debug_keys = HashSet::new();
        let mut debug_samples = Vec::new();
        for x in region.min_x..=region.max_x {
            for y in region.min_y..=region.max_y {
                for z in region.min_z..=region.max_z {
                    let state = chunks.get_block_state(x, y, z);
                    let mut row = self
                        .states
                        .get(u32::from(state) as usize)
                        .ok_or("Unknown block state ID")?
                        .clone();
                    if debug_names.contains(&block::block_id(state))
                        && debug_keys
                            .insert(row["stateKey"].as_str().unwrap_or_default().to_owned())
                        && debug_samples.len() < 16
                    {
                        debug_samples.push((x, y, z, state));
                    }
                    row["dimension"] = json!(game.dimension);
                    row["x"] = json!(x);
                    row["y"] = json!(y);
                    row["z"] = json!(z);
                    let biome = chunks
                        .biome_id_checked(x, y, z)
                        .ok_or("Missing biome cell")?;
                    // Actual renderer quart-cell input, not a fabricated plains fallback.
                    row["biome"] = json!(biome_name(&game.registries, biome)?);
                    row["light"] = json!({"sky": chunks.get_sky_light(x,y,z), "block": chunks.get_block_light(x,y,z), "emission": block::light_props(state).emission});
                    serde_json::to_writer(&mut text, &row)?;
                    text.push(b'\n');
                    count += 1;
                }
            }
        }
        let p = game.player.position;
        let c = renderer.camera_render_position();
        let (yaw, pitch) = renderer.camera_effective_look_deg();
        let (vendor, driver) = renderer.probe_gpu_info();
        let received_clock = game.sky_state.last_network_clock.map(
            |(id, total_ticks, partial_tick, rate)| {
                json!({"id": id, "totalTicks": total_ticks, "partialTick": partial_tick, "rate": rate})
            },
        );
        let debug_path = self.path(id, ".render-debug.json");
        let mut metadata = json!({
            "gameVersion": core.version, "launchVersion": "Pomme", "server": self.server,
            "dimension": game.dimension, "debugWorld": chunks.debug_world.is_some(), "playerX": p.x, "playerY": p.y, "playerZ": p.z,
            "playerYaw": game.player.look_dir.y_rot_deg(), "playerPitch": game.player.look_dir.x_rot_deg(),
            "cameraX": c.x, "cameraY": c.y, "cameraZ": c.z, "cameraYaw": yaw, "cameraPitch": pitch,
            "fov": renderer.camera_fov_degrees(), "width": renderer.screen_width(), "height": renderer.screen_height(),
            "gpuName": renderer.gpu_name(), "gpuVendor": format!("PCI 0x{vendor:04x}"), "backend": "Vulkan",
            "driver": format!("Vulkan driverVersion {driver}"), "vulkanApi": renderer.vulkan_version(),
            "capturedAt": chrono::Utc::now().to_rfc3339(), "cameraMode": "FIRST_PERSON", "hudHidden": game.hide_gui, "showHand": !game.hide_gui, "captureSource": "Vulkan swapchain PresentSrcKHR -> TransferSrcOptimal -> host readback PNG",
            "viewBobbing": core.menu.view_bobbing, "biomeSampling": "renderer quart cell (no vanilla fuzzy zoom)",
            "receivedClock": received_clock,
            "clock": {"id": game.sky_state.clock_id, "totalTicks": game.sky_state.day_time, "partialTick": game.sky_state.clock_partial_tick, "rate": game.sky_state.clock_rate},
            "serverTickRate": core.server_tick_rate, "serverFrozen": core.server_tick_frozen, "serverFrozenTicksToRun": core.server_tick_steps,
            "clientGameTime": game.sky_state.game_time,
            "clientTime": game.sky_state.day_time as f64 + f64::from(game.sky_state.clock_partial_tick),
            "clientDaytime": game.sky_state.day_tick(),
            "clockPhase": game.sky_state.day_tick().rem_euclid(24000.0),
            "rendererSkyTime": sky.day_time as f64
                + f64::from(sky.clock_partial_tick + sky.partial_tick * sky.clock_rate),
            "region": region, "recordCount": count, "worldInput": self.path(id, ".world-input.jsonl"),
            "renderDebug": debug_path, "stateInventory": self.root.join("results/states.jsonl"), "stateCount": self.states.len()
        });
        if request["runId"].is_string() {
            metadata["runId"] = request["runId"].clone();
            metadata["caseId"] = request["caseId"].clone();
            metadata["attemptId"] = request["attemptId"].clone();
            metadata["resultId"] = json!(id);
            metadata["requestedAt"] = request["requestedAt"].clone();
            metadata["targetCaptureAt"] = request["targetCaptureAt"].clone();
            metadata["snapshotTiming"] = json!(
                "actualSnapshotAt is the CPU raw snapshot; actualFrameCapturedAt is the Vulkan copy command recording time; frameReadbackCompletedAt is fence/readback completion"
            );
        }
        write_atomic(&self.path(id, ".world-input.jsonl"), &text)?;
        save_json(
            &debug_path,
            &render_debug::snapshot(
                renderer,
                sky,
                &game.dimension,
                if game.server_render_distance > 0 {
                    core.menu.render_distance.min(game.server_render_distance)
                } else {
                    core.menu.render_distance
                },
                game.probe_lightmap_brightness(),
                chunks,
                &debug_samples,
            ),
        )?;
        // request_probe_screenshot arms the next presented frame; do not call save
        // completion the capture time.
        let actual_snapshot_at = chrono::Utc::now().to_rfc3339();
        let rx = renderer.request_probe_screenshot(self.path(id, ".png"))?;
        metadata["actualSnapshotAt"] = json!(actual_snapshot_at);
        save_json(&self.path(id, ".json"), &metadata)?;
        self.pending = Some(PendingCapture::Screenshot {
            id: id.to_owned(),
            rx,
            raw: None,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn item_overlay_validation_accepts_bounded_variable_16px_slots() {
        let ids = ["fern", "bush", "lily_pad", "sugar_cane", "pink_petals", "wildflowers"];
        let items: Vec<_> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| {
                json!({
                    "id": id,
                    "logicalRect": [16 + i * 20, 12, 16, 16],
                    "physicalRect": [48 + i * 60, 36, 48, 48]
                })
            })
            .collect();
        let value = json!({
            "mode": "gui",
            "guiScale": 3,
            "panelLogicalRect": [8, 8, 132, 24],
            "panelPhysicalRect": [24, 24, 396, 72],
            "backgroundRGB": [32, 32, 32],
            "items": items
        });
        let request = json!({"itemOverlay": value});
        assert!(parse_item_overlay(&request).unwrap().is_some());
        let mut bad = request;
        bad["itemOverlay"]["items"] = json!([]);
        assert!(parse_item_overlay(&bad).is_err());
    }

    #[test]
    fn probe_boundaries_and_canonical_states() {
        block::init("26.2");
        assert_eq!(canonical_id("stone"), "minecraft:stone");
        assert_eq!(canonical_id("test:stone"), "test:stone");
        let row = state_record(block::find_state("oak_stairs", &[("facing", "north")]));
        assert!(
            row["stateKey"]
                .as_str()
                .unwrap()
                .starts_with("minecraft:oak_stairs[facing=north,half=")
        );
        assert!(biome_name(&Default::default(), 0).is_err());
        assert_eq!(
            crate::world::chunk::ChunkStore::new(2).biome_id_checked(0, 70, 0),
            None
        );
        assert!(safe_case("grid-diagonal"));
        assert!(!safe_case("../java"));
        let r = json!({"minX":0,"minY":69,"minZ":0,"maxX":10.0,"maxY":71,"maxZ":10});
        assert!(Region::parse(&r, -64, 384).is_ok());
        for bad in [
            json!("1"),
            json!(0.5),
            json!(-1),
            json!(32768),
            json!(1e100),
            Value::Null,
        ] {
            let mut v = r.clone();
            v["maxX"] = bad;
            assert!(Region::parse(&v, -64, 384).is_err());
        }
        assert!(Region::parse(&r, 80, 256).is_err());
        let mut v = r;
        v["maxX"] = json!(29999999);
        v["maxZ"] = json!(29999999);
        assert!(Region::parse(&v, -64, 384).is_err());
        let a = crate::entity::components::LookDirection::new(-180.0, 32.0).as_vec();
        let b = crate::entity::components::LookDirection::new(180.0, 32.0).as_vec();
        assert!((a - b).length() < 1e-6);
    }

    #[test]
    fn peer_filter_matches_uuid_and_name_only_without_touching_normal_mode() {
        let uuid = uuid::Uuid::from_u128(1);
        let paired = PeerFilter {
            mode: "paired".into(),
            peer_name: "PeerJava".into(),
            peer_uuid: Some(uuid),
            provenance: "test".into(),
        };
        assert!(should_exclude_peer(
            Some(&paired),
            2,
            1,
            Some(uuid),
            Some("PeerJava")
        ));
        assert!(!should_exclude_peer(
            Some(&paired),
            2,
            1,
            Some(uuid),
            Some("Other")
        ));
        assert!(!should_exclude_peer(
            Some(&paired),
            2,
            1,
            Some(uuid::Uuid::from_u128(2)),
            Some("PeerJava")
        ));
        let name_only = PeerFilter {
            peer_uuid: None,
            ..paired.clone()
        };
        assert!(should_exclude_peer(
            Some(&name_only),
            2,
            1,
            Some(uuid::Uuid::from_u128(2)),
            Some("PeerJava")
        ));
        let normal = PeerFilter {
            mode: "normal".into(),
            ..name_only
        };
        assert!(!should_exclude_peer(
            Some(&normal),
            2,
            1,
            Some(uuid),
            Some("PeerJava")
        ));
        assert!(!should_exclude_peer(
            None,
            2,
            1,
            Some(uuid),
            Some("PeerJava")
        ));
        assert!(!should_exclude_peer(
            Some(&paired),
            1,
            1,
            Some(uuid),
            Some("PeerJava")
        ));
    }
}
