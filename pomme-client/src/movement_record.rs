//! Opt-in client observations, not proof of server acceptance/internal state.
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use azalea_protocol::packets::ProtocolPacket;
use azalea_protocol::packets::game::{ClientboundGamePacket as C, ServerboundGamePacket as S};
use parking_lot::Mutex;
use serde_json::{Value, json};
const QUEUE: usize = 16;
const LIMIT: u64 = 1024 * 1024 * 1024;
const MAX_RAW: usize = 8 * 1024 * 1024;
const MAX_ROW: usize = 12 * 1024 * 1024;
static NEXT_PACKET_TRACE_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_CONNECTION_EPOCH: AtomicU64 = AtomicU64::new(1);
static NEXT_RECORDING_ID: AtomicU64 = AtomicU64::new(1);
pub struct Recorder {
    active: AtomicBool,
    epoch: u64,
    context: Mutex<(Option<u64>, Option<u64>)>,
    state: Mutex<State>,
}
impl Default for Recorder {
    fn default() -> Self {
        Self {
            active: AtomicBool::new(false),
            epoch: NEXT_CONNECTION_EPOCH.fetch_add(1, Ordering::Relaxed),
            context: Mutex::new((None, None)),
            state: Mutex::new(State::default()),
        }
    }
}
#[derive(Default)]
struct State {
    session: Option<Session>,
    status: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PacketTraceId {
    session: u64,
    id: u64,
}
struct Session {
    tx: Option<crossbeam_channel::Sender<Value>>,
    start: Instant,
    seq: u64,
    dropped: u64,
    reason: &'static str,
    path: PathBuf,
    wall: u64,
    id: u64,
}
impl Recorder {
    pub fn active(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }
    pub fn status(&self) -> String {
        self.state.lock().status.clone()
    }
    pub fn start(self: &Arc<Self>, rt: &tokio::runtime::Runtime, dir: &Path) {
        self.start_path(
            rt,
            dir.join(format!("movement-{}.jsonl", uuid::Uuid::new_v4())),
        );
    }
    fn start_path(self: &Arc<Self>, rt: &tokio::runtime::Runtime, path: PathBuf) {
        let mut state = self.state.lock();
        if state.session.is_some() {
            return;
        }
        let (tx, rx) = crossbeam_channel::bounded(QUEUE);
        let wall = wall_ms();
        state.status = format!("Recording start UTC unix_ms={wall}: {}", path.display());
        state.session = Some(Session {
            tx: Some(tx),
            start: Instant::now(),
            seq: 0,
            dropped: 0,
            reason: "stop",
            path: path.clone(),
            wall,
            id: NEXT_RECORDING_ID.fetch_add(1, Ordering::Relaxed),
        });
        self.active.store(true, Ordering::Relaxed);
        let recorder = self.clone();
        let protocol = crate::version::session_protocol();
        rt.spawn_blocking(move || {
            let result = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .and_then(|file| recorder.write_log(BufWriter::new(file), rx, protocol, LIMIT));
            recorder.finish(result);
        });
    }
    fn finish(&self, result: io::Result<()>) {
        self.stop(if result.is_ok() {
            "finished"
        } else {
            "writer_error"
        });
        let mut state = self.state.lock();
        let session = state.session.take().expect("writer owns session");
        state.status = match result {
            Ok(()) => format!(
                "Ended UTC unix_ms={}; start={}; {} (reason={}, dropped={})",
                wall_ms(),
                session.wall,
                session.path.display(),
                session.reason,
                session.dropped
            ),
            Err(error) => {
                tracing::error!(%error, "Movement recording write/flush failed");
                format!(
                    "Recording FAILED: {error}; {} (reason={}, dropped={})",
                    session.path.display(),
                    session.reason,
                    session.dropped
                )
            }
        };
    }
    pub fn stop(&self, reason: &'static str) {
        self.active.store(false, Ordering::Relaxed);
        let mut state = self.state.lock();
        if let Some(s) = &mut state.session {
            if s.tx.take().is_some() {
                s.reason = reason;
            }
            state.status = format!("Draining recording ({}); {}", s.reason, s.path.display());
        }
    }
    // ponytail: short session mutex orders producers; split only if measured
    // contention matters.
    pub fn record(
        &self,
        direction: &'static str,
        stage: &'static str,
        make: impl FnOnce() -> Option<Value>,
    ) {
        self.record_in_session(None, direction, stage, make);
    }

    fn record_in_session(
        &self,
        expected_session: Option<u64>,
        direction: &'static str,
        stage: &'static str,
        make: impl FnOnce() -> Option<Value>,
    ) {
        if !self.active() {
            return;
        }
        let mut state = self.state.lock();
        let Some(s) = state.session.as_mut().filter(|s| s.tx.is_some()) else {
            return;
        };
        if expected_session.is_some_and(|id| id != s.id) {
            return;
        }
        let Some(data) = make() else {
            return;
        };
        s.seq += 1;
        let (frame_id, tick) = *self.context.lock();
        let row = json!({"seq":s.seq,"offset_us":s.start.elapsed().as_micros() as u64,"frame_id":frame_id,"tick":tick,"direction":direction,"stage":stage,"data":data});
        if s.tx.as_ref().unwrap().try_send(row).is_err() {
            s.dropped += 1;
        }
    }
    fn write_log(
        &self,
        mut out: impl Write,
        rx: crossbeam_channel::Receiver<Value>,
        protocol: i32,
        limit: u64,
    ) -> io::Result<()> {
        let start = self.state.lock().session.as_ref().unwrap().wall;
        write_row(
            &mut out,
            &json!({"type":"header","schema":1,"utc_start_unix_ms":start,"connection_epoch":self.epoch,"wire_protocol":protocol,"executable":executable_identity(),"package_version":env!("CARGO_PKG_VERSION"),"source_build_revision":null,"queue_capacity":QUEUE,"size_limit_bytes":limit,"max_row_bytes":MAX_ROW,"semantics":"client observations only; queued != transport_write_success != server acceptance; raw PLAY/CONFIG payloads include cookies/chat/custom payloads and are private local diagnostics; LOGIN/auth secrets excluded; normalized packet IDs are native; seq gaps/dropped mean incomplete evidence"}),
        )?;
        let mut bytes = 0;
        let mut omitted = 0;
        let mut written = 0;
        for row in rx {
            let mut data = serde_json::to_vec(&row)?;
            data.push(b'\n');
            if data.len() > MAX_ROW {
                omitted += 1;
                self.state.lock().session.as_mut().unwrap().dropped += 1;
                continue;
            }
            out.write_all(&data)?;
            written += 1;
            bytes += data.len() as u64;
            // ponytail: soft limit then drain; hard ceiling LIMIT + QUEUE*MAX_ROW (<1.19
            // GiB).
            if bytes >= limit {
                self.stop("size_limit");
            }
        }
        let footer = {
            let state = self.state.lock();
            let s = state.session.as_ref().unwrap();
            json!({"type":"footer","utc_end_unix_ms":wall_ms(),"offset_us":s.start.elapsed().as_micros() as u64,"last_seq":s.seq,"written":written,"dropped":s.dropped,"oversize_omitted":omitted,"reason":s.reason,"complete":s.dropped==0 && omitted==0})
        };
        write_row(&mut out, &footer)?;
        out.flush()
    }
    pub fn local_prediction(
        &self,
        pos: azalea_core::position::BlockPos,
        before: azalea_block::BlockState,
        predicted: azalea_block::BlockState,
        sequence: u32,
    ) {
        self.record("local", "local_prediction", || Some(json!({
            "pos":block(&pos),"before":before.id(),"predicted":predicted.id(),"action_sequence":sequence
        })));
    }

    pub fn outbound(&self, stage: &'static str, packet: &S) {
        self.record("outbound", stage, || outbound(packet));
    }
    pub fn inbound(&self, packet: &C, wire_id: Option<u32>) {
        self.record("inbound", "received", || {
            inbound(packet).map(|mut data| {
                data["wire_id"] = json!(wire_id);
                data
            })
        });
    }

    /// Record the exact uncompressed plaintext frame (VarInt id + payload).
    /// LOGIN/authentication frames and transport encryption keys are never
    /// passed here.
    fn record_omission(
        &self,
        expected_session: u64,
        direction: &'static str,
        stage: &'static str,
        data: Value,
    ) {
        if !self.active() {
            return;
        }
        let mut state = self.state.lock();
        let Some(s) = state.session.as_mut().filter(|s| s.tx.is_some()) else {
            return;
        };
        if s.id != expected_session {
            return;
        }
        s.dropped += 1;
        s.seq += 1;
        let (frame_id, tick) = *self.context.lock();
        let row = json!({"seq":s.seq,"offset_us":s.start.elapsed().as_micros() as u64,"frame_id":frame_id,"tick":tick,"direction":direction,"stage":stage,"data":data});
        if s.tx.as_ref().unwrap().try_send(row).is_err() {
            s.dropped += 1;
        }
    }

    pub fn set_context(&self, frame_id: u64, tick: u64) {
        *self.context.lock() = (Some(frame_id), Some(tick));
    }

    pub fn packet_trace_id(&self) -> Option<PacketTraceId> {
        if !self.active() {
            return None;
        }
        let mut state = self.state.lock();
        let session = state.session.as_mut().filter(|s| s.tx.is_some())?;
        let id = NEXT_PACKET_TRACE_ID.fetch_add(1, Ordering::Relaxed);
        Some(PacketTraceId {
            session: session.id,
            id,
        })
    }

    pub fn packet_stage(
        &self,
        trace: PacketTraceId,
        stage: &'static str,
        reason: Option<&'static str>,
    ) {
        let current_session = self
            .state
            .lock()
            .session
            .as_ref()
            .filter(|s| s.tx.is_some())
            .map(|s| s.id);
        if current_session != Some(trace.session) {
            return;
        }
        self.record_in_session(Some(trace.session), "outbound", stage, || {
            Some(json!({"packet_trace_id":trace.id,"reason":reason}))
        });
    }

    pub fn raw_packet(&self, direction: &'static str, protocol_state: &'static str, frame: &[u8]) {
        self.raw_packet_traced(direction, protocol_state, frame, None);
    }

    pub fn raw_packet_traced(
        &self,
        direction: &'static str,
        protocol_state: &'static str,
        frame: &[u8],
        trace: Option<PacketTraceId>,
    ) {
        if !self.active() {
            return;
        }
        let current_session = self
            .state
            .lock()
            .session
            .as_ref()
            .filter(|s| s.tx.is_some())
            .map(|s| s.id);
        let Some(session_id) = current_session else {
            return;
        };
        if trace.is_some_and(|trace| trace.session != session_id) {
            return;
        }
        let packet_trace_id = trace
            .map(|trace| trace.id)
            .unwrap_or_else(|| NEXT_PACKET_TRACE_ID.fetch_add(1, Ordering::Relaxed));
        let trace_origin = "observed";
        if frame.len() > MAX_RAW {
            self.record_omission(session_id, direction, "packet_raw_omitted_oversize", json!({"packet_trace_id":packet_trace_id,"packet_trace_origin":trace_origin,"raw_length":frame.len(),"limit":MAX_RAW}));
            return;
        }
        let mut cursor = std::io::Cursor::new(frame);
        let native_id = <u32 as azalea_buf::AzBufVar>::azalea_read_var(&mut cursor).ok();
        let protocol = crate::version::session_protocol();
        let direction_enum = if direction == "inbound" {
            pomme_protocol::Direction::Clientbound
        } else {
            pomme_protocol::Direction::Serverbound
        };
        let phase = if protocol_state == "configuration" {
            pomme_protocol::Phase::Configuration
        } else {
            pomme_protocol::Phase::Game
        };
        let packet_type = native_id.and_then(|id| {
            pomme_protocol::PacketTable::for_protocol(protocol)?
                .name_of(phase, direction_enum, id)
                .map(str::to_owned)
        });
        self.record_in_session(Some(session_id), direction, "packet_raw", || Some(json!({
            "protocol_state":protocol_state,"wire_protocol":protocol,"connection_epoch":self.epoch,
            "packet_trace_id":packet_trace_id,"packet_trace_origin":trace_origin,"native_id":native_id,"packet_type":packet_type,
            "raw_length":frame.len(),"payload_base64":base64::Engine::encode(&base64::engine::general_purpose::STANDARD, frame),
            "capture_point":"wire_plaintext","payload_layout":"id_plus_payload"
        })));
    }
}
// Called only by the blocking writer, never by a UI/game-thread producer.
fn executable_identity() -> Value {
    match std::env::current_exe() {
        Ok(path) => file_identity(&path),
        Err(error) => json!({"error":error.to_string()}),
    }
}
fn file_identity(path: &Path) -> Value {
    use sha2::{Digest, Sha256};
    let result = (|| -> io::Result<Value> {
        let mut file = std::fs::File::open(path)?;
        let before = file.metadata()?;
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 64 * 1024];
        let mut read_bytes = 0u64;
        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
            read_bytes += n as u64;
        }
        let after = file.metadata()?;
        let unchanged = before.len() == after.len()
            && before.modified().ok() == after.modified().ok()
            && read_bytes == before.len();
        Ok(json!({"path":path,"size_bytes":before.len(),
            "mtime_unix_ms":before.modified().ok().and_then(|t|t.duration_since(UNIX_EPOCH).ok()).map(|t|t.as_millis() as u64),
            "sha256":hash.finalize().iter().map(|byte|format!("{byte:02x}")).collect::<String>(),"metadata_unchanged_during_read":unchanged,
            "semantics":"current_exe file at writer start, not an attestation of mapped memory or source revision"}))
    })();
    result.unwrap_or_else(|error| json!({"path":path,"error":error.to_string()}))
}
fn wall_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn write_row(out: &mut impl Write, row: &Value) -> io::Result<()> {
    serde_json::to_writer(&mut *out, row)?;
    out.write_all(b"\n")
}
fn vec3(p: &azalea_core::position::Vec3) -> Value {
    json!([p.x, p.y, p.z])
}
fn block(p: &azalea_core::position::BlockPos) -> Value {
    json!([p.x, p.y, p.z])
}
fn look(p: &azalea_entity::LookDirection) -> Value {
    json!([p.y_rot(), p.x_rot()])
}
fn change(p: &azalea_protocol::common::movements::PositionMoveRotation) -> Value {
    json!({"position":vec3(&p.pos),"velocity":vec3(&p.delta),"yaw_pitch":look(&p.look_direction)})
}
fn relative(p: &azalea_protocol::common::movements::RelativeMovements) -> Value {
    json!({"position":[p.x,p.y,p.z],"yaw_pitch":[p.y_rot,p.x_rot],"velocity":[p.delta_x,p.delta_y,p.delta_z],"rotate_delta":p.rotate_delta})
}
fn named(p: &impl ProtocolPacket, fields: Value) -> Value {
    json!({"packet":p.name(),"native_id":p.id(),"fields":fields})
}
pub(crate) fn outbound(p: &S) -> Option<Value> {
    let fields = match p {
        S::MovePlayerPos(p) => {
            json!({"position":vec3(&p.pos),"on_ground":p.flags.on_ground,"horizontal_collision":p.flags.horizontal_collision})
        }
        S::MovePlayerPosRot(p) => {
            json!({"position":vec3(&p.pos),"yaw_pitch":look(&p.look_direction),"on_ground":p.flags.on_ground,"horizontal_collision":p.flags.horizontal_collision})
        }
        S::MovePlayerRot(p) => {
            json!({"yaw_pitch":look(&p.look_direction),"on_ground":p.flags.on_ground,"horizontal_collision":p.flags.horizontal_collision})
        }
        S::MovePlayerStatusOnly(p) => {
            json!({"on_ground":p.flags.on_ground,"horizontal_collision":p.flags.horizontal_collision})
        }
        S::MoveVehicle(p) => json!({"position":vec3(&p.pos),"yaw_pitch":look(&p.look_direction)}),
        S::PlayerInput(p) => {
            json!({"forward":p.forward,"backward":p.backward,"left":p.left,"right":p.right,"jump":p.jump,"shift":p.shift,"sprint":p.sprint})
        }
        S::PlayerCommand(p) => json!({"entity_id":p.id.0,"action":p.action as u32,"data":p.data}),
        S::PlayerAction(p) => {
            json!({"action":p.action as u32,"block":block(&p.pos),"face":p.direction as u32,"sequence":p.seq})
        }
        S::UseItemOn(p) => {
            json!({"hand":p.hand as u32,"block":block(&p.block_hit.block_pos),"face":p.block_hit.direction as u32,"hit":vec3(&p.block_hit.location),"inside":p.block_hit.inside,"world_border":p.block_hit.world_border,"sequence":p.seq})
        }
        S::UseItem(p) => {
            json!({"hand":p.hand as u32,"sequence":p.seq,"yaw_pitch":[p.y_rot,p.x_rot]})
        }
        S::Swing(p) => json!({"hand":p.hand as u32}),
        S::AcceptTeleportation(p) => json!({"teleport_id":p.id}),
        _ => return None,
    };
    Some(named(p, fields))
}
/// Gate by packet ID before decoding: no arbitrary NBT/chat/raw packet copies.
pub fn outbound_frame(frame: &[u8]) -> Option<Value> {
    use azalea_buf::AzBufVar;
    let mut cursor = std::io::Cursor::new(frame);
    let id = u32::azalea_read_var(&mut cursor).ok()?;
    let table =
        pomme_protocol::PacketTable::for_protocol(pomme_protocol::version::NATIVE.protocol)?;
    if ![
        "move_player_pos",
        "move_player_pos_rot",
        "move_player_rot",
        "move_player_status_only",
        "move_vehicle",
        "player_input",
        "player_command",
        "player_action",
        "use_item_on",
        "use_item",
        "swing",
        "accept_teleportation",
    ]
    .iter()
    .any(|name| {
        table.id(
            pomme_protocol::Phase::Game,
            pomme_protocol::Direction::Serverbound,
            name,
        ) == Some(id)
    }) {
        return None;
    }
    outbound(&S::read(id, &mut cursor).ok()?)
}
fn inbound(p: &C) -> Option<Value> {
    let fields = match p {
        C::PlayerPosition(p) => {
            json!({"teleport_id":p.id,"change":change(&p.change),"relative":relative(&p.relative)})
        }
        C::PlayerRotation(p) => {
            json!({"yaw_pitch":[p.y_rot,p.x_rot],"relative":[p.relative_y,p.relative_x]})
        }
        C::MoveVehicle(p) => json!({"position":vec3(&p.pos),"yaw_pitch":look(&p.look_direction)}),
        C::TeleportEntity(p) => {
            json!({"entity_id":p.id.0,"change":change(&p.change),"relative":relative(&p.relative),"on_ground":p.on_ground})
        }
        C::EntityPositionSync(p) => {
            json!({"entity_id":p.id.0,"change":change(&p.values),"on_ground":p.on_ground})
        }
        C::SetEntityMotion(p) => {
            let v: azalea_core::position::Vec3 = p.delta.into();
            json!({"entity_id":p.id.0,"velocity":vec3(&v)})
        }
        C::BlockUpdate(p) => json!({"block":block(&p.pos),"state":p.block_state.id()}),
        C::BlockChangedAck(p) => json!({"sequence":p.seq}),
        C::SectionBlocksUpdate(p) => {
            json!({"section":[p.section_pos.x,p.section_pos.y,p.section_pos.z],"count":p.states.len(),"truncated":p.states.len()>4096,"updates":p.states.iter().take(4096).map(|s| json!({"local":[s.pos.x,s.pos.y,s.pos.z],"state":s.state.id()})).collect::<Vec<_>>()})
        }
        _ => return None,
    };
    Some(named(p, fields))
}
pub(crate) fn own_attributes(p: &crate::player::LocalPlayer) -> Value {
    Value::Object(p.attributes.iter().map(|(id, data)| {
        (id.clone(), json!({"base":data.base,"modifiers":data.modifiers,"effective":p.attribute_value(id, data.base)}))
    }).collect())
}

fn bbox(b: crate::physics::aabb::Aabb) -> Value {
    json!([b.min.to_array(), b.max.to_array()])
}
pub fn player(game: &crate::app::phases::in_game::GameState) -> Value {
    player_observation(game, false)
}
pub fn event_player(game: &crate::app::phases::in_game::GameState, event: &Value) -> Value {
    player_observation(
        game,
        matches!(
            event["event"].as_str(),
            Some("player_position" | "player_rotation" | "entity_teleport")
        ),
    )
}
fn player_observation(game: &crate::app::phases::in_game::GameState, correction: bool) -> Value {
    let p = &game.player;
    let attributes = own_attributes(p);
    let floor = game.chunk_store.get_block_state(
        p.prev_position.x.floor() as i32,
        (p.prev_position.y - f64::from(0.500_001_f32)).floor() as i32,
        p.prev_position.z.floor() as i32,
    );
    let o = &p.last_travel_observation;
    json!({"pose":format!("{:?}",p.pose),"main_supporting_block_pos":p.main_supporting_block_pos.as_ref().map(block),
        "legacy_floor_semantics":"prev-position center fallback, NOT used travel friction",
        "shape_snapshot":if correction {
            Some(shape_snapshot(game,p.bounding_box(),(p.bounding_box().min.y,p.crouching,crate::physics::movement::has_leather_boots(p),p.fall_distance),"correction current-bbox requery; NOT actual collision inputs"))
        } else if game.riding_vehicle_id.is_none() && p.collision_delta[0] != p.collision_delta[1] {
            o.bbox_before.map(|bb|shape_snapshot(game,bb.expand(p.collision_delta[0]).expand(glam::dvec3(0.0,o.step_height.unwrap_or(0.0),0.0)),(bb.min.y,o.descending,o.leather_boots,o.fall_distance),"post-tick requested sweep + step-height envelope requery; NOT actual collision inputs"))
        } else {None},
        "travel_observation":if game.riding_vehicle_id.is_none() {Some(json!({
            "used_friction_f32":o.friction,"friction_source_pos":o.friction_pos.as_ref().map(block),
            "used_ground_drag_f32":o.ground_drag,"on_ground_at_land_start":o.friction.map(|_|o.on_ground_at_start),
            "used_block_speed_factor_f32":o.block_speed_factor,"used_block_jump_factor_f32":o.block_jump_factor,"jump_power_f32":o.jump_power,
            "used_step_height":o.step_height,"pose_at_move":o.pose_at_move.map(|v|format!("{v:?}")),
            "bbox_before":o.bbox_before.map(bbox),"bbox_after":o.bbox_after.map(bbox),
            "support_before":o.support_before.as_ref().map(block),"support_after":o.support_after.as_ref().map(block),
            "requested_delta":o.bbox_before.map(|_|p.collision_delta[0].to_array()),"clipped_delta":o.bbox_before.map(|_|p.collision_delta[1].to_array()),
            "original_requested_y_negative":o.bbox_before.map(|_|p.collision_delta[0].y < 0.0),
            "final_y_clipped":o.bbox_before.map(|_|p.collision_delta[0].y != p.collision_delta[1].y),"ground_decision":o.ground_decision,
            "entity_shapes":o.entity_shapes_captured.then(||o.entity_shapes.iter().flatten().copied().map(bbox).collect::<Vec<_>>()),
            "entity_shapes_max":8,"entity_shapes_truncated":o.entity_shapes_captured && o.entity_collider_count>8,
            "entity_shapes_omitted":o.entity_shapes_captured.then(||o.entity_collider_count.saturating_sub(8)),
            "context":{"descending":o.descending,"leather_boots":o.leather_boots,"fall_distance":o.fall_distance,"entity_collider_count":o.entity_collider_count,"border_bounds":o.border_bounds}
        }))} else {None},"own_attributes":attributes,
        "effective_movement_speed_f32":crate::physics::movement::movement_speed(p),
        "effective_jump_strength_f32":p.attribute_value("minecraft:jump_strength", f64::from(0.42_f32)) as f32,
        "effective_gravity":crate::physics::movement::effective_gravity(p),
        "sprinting":p.sprinting,"crouching":p.crouching,"swimming":p.swimming,
        "in_water":p.in_water,"in_lava":p.in_lava,"fall_flying":p.fall_flying,
        "effects":p.effects.sorted_desc().iter().map(|e|json!({"id":e.effect_id,"amplifier":e.amplifier,"duration":e.duration})).collect::<Vec<_>>(),
        "abilities":{"flying":p.flying,"may_fly":p.may_fly,"fly_speed":p.fly_speed,"walk_speed":p.walk_speed,"invulnerable":p.invulnerable,"instabuild":p.instabuild},
        "floor_state":floor.id(),"block_friction_f32":crate::physics::movement::block_friction(&game.chunk_store,p.prev_position),
        "pre_collision_delta":game.riding_vehicle_id.is_none().then(||p.collision_delta[0].to_array()),"post_collision_delta":game.riding_vehicle_id.is_none().then(||p.collision_delta[1].to_array()),
        "tick_position_delta":[p.position.x-p.prev_position.x,p.position.y-p.prev_position.y,p.position.z-p.prev_position.z],
        "tick":game.tick_count,"entity_id":p.entity_id,"position":[p.position.x,p.position.y,p.position.z],"velocity":[p.velocity.x,p.velocity.y,p.velocity.z],"on_ground":p.on_ground,"yaw_pitch":[p.look_dir.y_rot_deg(),p.look_dir.x_rot_deg()],"riding":game.riding_vehicle_id,"vehicle":game.controlled_vehicle_id.and_then(|id| game.entity_store.living.get(&id)).map(|v|json!({"position":[v.position.x,v.position.y,v.position.z],"velocity":[v.velocity.x,v.velocity.y,v.velocity.z]}))})
}
fn shape_snapshot(
    game: &crate::app::phases::in_game::GameState,
    region: crate::physics::aabb::Aabb,
    context: (f64, bool, bool, f64),
    semantics: &str,
) -> Value {
    let (cells, boxes, omitted_cells, omitted_boxes) =
        crate::physics::collision::diagnostic_block_shapes(&game.chunk_store, &region, context);
    json!({"semantics":semantics,"region":bbox(region),"player_context":{"feet_y":context.0,"descending":context.1,"leather_boots":context.2,"fall_distance":context.3},
        "block_cells":cells.into_iter().map(|(pos,state)|json!({"pos":block(&pos),"state_id":state.id(),"name":crate::world::block::block_id(state),"properties":crate::world::block::block_properties(state).entries().collect::<std::collections::BTreeMap<_,_>>()})).collect::<Vec<_>>(),
        "shape_aabbs":boxes.into_iter().map(|(pos,bb)|json!({"block":block(&pos),"aabb":bbox(bb)})).collect::<Vec<_>>(),
        "max_block_cells":32,"max_boxes":128,"truncated":omitted_cells>0 || omitted_boxes>0,"omitted_block_cells":omitted_cells,"omitted_boxes_in_visited_cells":omitted_boxes,"boxes_in_unvisited_cells":null,
        "entity_shapes":null,"entity_shapes_semantics":"not requeried; bounded actual travel inputs are in travel_observation.entity_shapes", "current_border_bounds":game.world_border.bounds_at(0.0)})
}
pub fn applied(mut data: Value, game: &crate::app::phases::in_game::GameState) -> Value {
    fn after(data: &mut Value, game: &crate::app::phases::in_game::GameState) {
        if let Some(pos) = data["block"].as_array().filter(|p| p.len() == 3) {
            let p = pos
                .iter()
                .map(|v| v.as_i64().unwrap_or_default() as i32)
                .collect::<Vec<_>>();
            data["after_state"] = json!(game.chunk_store.get_block_state(p[0], p[1], p[2]).id());
        }
    }
    after(&mut data, game);
    if let Some(updates) = data["updates"].as_array_mut() {
        for update in updates {
            after(update, game);
        }
    }
    json!({"event":data,"player":event_player(game,&data)})
}
pub fn event(
    event: &crate::net::NetworkEvent,
    game: &crate::app::phases::in_game::GameState,
) -> Option<Value> {
    use crate::net::NetworkEvent as E;
    let data = match event {
        E::PlayerPosition { id, .. } => json!({"event":"player_position","teleport_id":id}),
        E::PlayerRotation { .. } => json!({"event":"player_rotation"}),
        E::MoveVehicle { .. } => json!({"event":"move_vehicle"}),
        E::EntityTeleported { id, .. }
            if *id == game.player.entity_id || Some(*id) == game.controlled_vehicle_id =>
        {
            json!({"event":"entity_teleport","entity_id":id})
        }
        E::EntityMotion { id, .. }
            if *id == game.player.entity_id || Some(*id) == game.controlled_vehicle_id =>
        {
            json!({"event":"entity_motion","entity_id":id})
        }
        E::BlockUpdate { pos, state } => {
            json!({"event":"block_update","block":block(pos),"before_state":game.chunk_store.get_block_state(pos.x,pos.y,pos.z).id(),"server_state":state.id()})
        }
        E::SectionBlocksUpdate { updates } => {
            json!({"event":"section_blocks_update","count":updates.len(),"truncated":updates.len()>4096,"updates":updates.iter().take(4096).map(|(p,s)|json!({"block":block(p),"before_state":game.chunk_store.get_block_state(p.x,p.y,p.z).id(),"server_state":s.id()})).collect::<Vec<_>>()})
        }
        E::EntityAttributeUpdate {
            entity_id,
            snapshot,
        } if *entity_id == game.player.entity_id => {
            json!({"event":"own_attribute","entity_id":entity_id,"attribute":snapshot.attribute.to_string(),"base":snapshot.base,"modifiers":snapshot.modifiers})
        }
        E::BlockChangedAck { seq } => json!({"event":"block_ack","sequence":seq}),
        _ => return None,
    };
    Some(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ready(cap: usize) -> (Arc<Recorder>, crossbeam_channel::Receiver<Value>) {
        let r = Arc::new(Recorder::default());
        let (tx, rx) = crossbeam_channel::bounded(cap);
        r.state.lock().session = Some(Session {
            tx: Some(tx),
            start: Instant::now(),
            seq: 0,
            dropped: 0,
            reason: "stop",
            path: PathBuf::new(),
            wall: wall_ms(),
            id: NEXT_RECORDING_ID.fetch_add(1, Ordering::Relaxed),
        });
        r.active.store(true, Ordering::Relaxed);
        (r, rx)
    }
    fn rows(bytes: &[u8]) -> Vec<Value> {
        std::str::from_utf8(bytes)
            .unwrap()
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect()
    }
    #[test]
    fn executable_fixture_hash_and_metadata() {
        let dir = crate::test_util::test_temp_dir("movement-exe-identity");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("fixture");
        std::fs::write(&path, b"abc").unwrap();
        let identity = file_identity(&path);
        assert_eq!(identity["size_bytes"], 3);
        assert_eq!(
            identity["sha256"],
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(identity["mtime_unix_ms"].is_u64());
        assert_eq!(identity["metadata_unchanged_during_read"], true);
        assert!(file_identity(&dir.join("missing"))["error"].is_string());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn ordered_stages_drain_gaps_and_limit() {
        use azalea_protocol::packets::game::c_block_changed_ack::ClientboundBlockChangedAck;
        use azalea_protocol::packets::game::s_player_input::ServerboundPlayerInput;
        let (r, rx) = ready(4);
        r.record("local", "fixed_tick", || {
            Some(json!({"tick":1,"position":[0,64,0]}))
        });
        r.outbound(
            "queue_attempt",
            &S::PlayerInput(ServerboundPlayerInput {
                forward: true,
                ..Default::default()
            }),
        );
        r.outbound("queued", &S::PlayerInput(ServerboundPlayerInput::default()));
        r.inbound(
            &C::BlockChangedAck(ClientboundBlockChangedAck { seq: 7 }),
            Some(5),
        );
        r.record("local", "applied", || Some(json!({"sequence":7})));
        assert_eq!(r.state.lock().session.as_ref().unwrap().dropped, 1);
        r.stop("disconnect");
        let mut bytes = Vec::new();
        r.write_log(&mut bytes, rx, 776, LIMIT).unwrap();
        let log = rows(&bytes);
        assert_eq!(log[1]["stage"], "fixed_tick");
        assert_eq!(log[3]["stage"], "queued");
        assert_eq!(log[4]["stage"], "received");
        assert_eq!(log[5]["last_seq"], 5);
        assert_eq!(log[5]["dropped"], 1);
        assert_eq!(log[5]["reason"], "disconnect");
        for pair in log[1..5].windows(2) {
            assert!(pair[0]["seq"].as_u64() < pair[1]["seq"].as_u64());
            assert!(pair[0]["offset_us"].as_u64() <= pair[1]["offset_us"].as_u64());
        }
        assert!(!log.iter().any(|v| v["stage"] == "transport_write_success"));
        let (r, rx) = ready(4);
        r.record("local", "fixed_tick", || Some(json!({"tick":2})));
        let mut bytes = Vec::new();
        r.write_log(&mut bytes, rx, 776, 1).unwrap();
        assert!(!r.active());
        assert_eq!(rows(&bytes).last().unwrap()["reason"], "size_limit");
    }
    #[test]
    fn inactive_and_secret_allowlist() {
        use azalea_protocol::packets::game::s_edit_book::ServerboundEditBook;
        let r = Recorder::default();
        r.record("local", "fixed_tick", || {
            panic!("inactive must not allocate")
        });
        let p = S::EditBook(ServerboundEditBook {
            slot: 0,
            pages: vec!["SECRET".into()],
            title: Some("SECRET".into()),
        });
        assert!(outbound(&p).is_none());
        let bytes = azalea_protocol::write::serialize_packet(&p).unwrap();
        assert!(outbound_frame(&bytes).is_none());
    }
    #[test]
    fn raw_edit_and_rotation_allowlist_and_exact_prediction_sequence() {
        use azalea_protocol::packets::game::s_move_player_rot::ServerboundMovePlayerRot;
        use azalea_protocol::packets::game::s_player_action::{Action, ServerboundPlayerAction};
        let (r, rx) = ready(8);
        crate::world::block::init("26.2");
        let pos = azalea_core::position::BlockPos::new(-1, -48, 2);
        let stone = crate::world::block::first_state_of("stone").unwrap();
        for p in [
            S::PlayerAction(ServerboundPlayerAction {
                action: Action::StartDestroyBlock,
                pos,
                direction: azalea_core::direction::Direction::Up,
                seq: 7,
            }),
            S::MovePlayerRot(ServerboundMovePlayerRot {
                look_direction: azalea_entity::LookDirection::default(),
                flags: Default::default(),
            }),
        ] {
            let frame = azalea_protocol::write::serialize_packet(&p).unwrap();
            assert!(outbound_frame(&frame).is_some());
        }
        r.local_prediction(pos, stone, azalea_block::BlockState::AIR, 7);
        r.local_prediction(pos, azalea_block::BlockState::AIR, stone, 8);
        let first = rx.try_recv().unwrap();
        let second = rx.try_recv().unwrap();
        assert_eq!(first["data"]["before"], stone.id());
        assert_eq!(first["data"]["predicted"], 0);
        assert_eq!(first["data"]["action_sequence"], 7);
        assert_eq!(second["data"]["before"], 0);
        assert_eq!(second["data"]["predicted"], stone.id());
        assert_eq!(second["data"]["action_sequence"], 8);
    }

    #[test]
    fn raw_packet_keeps_cookie_bytes_and_malformed_ids_safely() {
        use base64::Engine;
        let (r, rx) = ready(4);
        r.set_context(42, 9);
        // Native configuration cookie_response ID 5 plus arbitrary cookie bytes.
        let frame = [5, 3, 0xff, 0x00, 0x42];
        r.raw_packet("inbound", "configuration", &frame);
        let row = rx.try_recv().unwrap();
        assert_eq!(row["stage"], "packet_raw");
        assert_eq!(row["data"]["protocol_state"], "configuration");
        assert_eq!(row["data"]["capture_point"], "wire_plaintext");
        assert_eq!(row["data"]["native_id"], 5);
        assert_eq!(row["frame_id"], 42);
        assert_eq!(row["tick"], 9);
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(row["data"]["payload_base64"].as_str().unwrap())
                .unwrap(),
            frame
        );
        r.raw_packet("inbound", "play", &[0x80, 0x80, 0x80, 0x80, 0x80]);
        let malformed = rx.try_recv().unwrap();
        assert!(malformed["data"]["native_id"].is_null());
        assert_eq!(malformed["data"]["raw_length"], 5);
        let (other, _other_rx) = ready(1);
        assert_ne!(r.epoch, other.epoch);
        assert_ne!(
            r.packet_trace_id().unwrap().id,
            other.packet_trace_id().unwrap().id
        );
        let stale_trace = other.packet_trace_id().unwrap();
        r.raw_packet_traced("outbound", "play", &[1, 2, 3], Some(stale_trace));
        assert!(
            rx.is_empty(),
            "a callback from another recording must not contaminate this session"
        );
        r.raw_packet("inbound", "play", &vec![0; MAX_RAW + 1]);
        assert_eq!(
            rx.try_recv().unwrap()["stage"],
            "packet_raw_omitted_oversize"
        );
        r.stop("test");
        let mut log = Vec::new();
        r.write_log(&mut log, rx, crate::version::session_protocol(), LIMIT)
            .unwrap();
        assert_eq!(rows(&log).last().unwrap()["complete"], false);
        assert!(rows(&log).last().unwrap()["dropped"].as_u64().unwrap() >= 1);
    }

    #[test]
    fn writer_write_and_flush_error_are_not_success() {
        struct Fail(bool);
        impl Write for Fail {
            fn write(&mut self, b: &[u8]) -> io::Result<usize> {
                if self.0 {
                    Err(io::Error::other("injected"))
                } else {
                    Ok(b.len())
                }
            }
            fn flush(&mut self) -> io::Result<()> {
                Err(io::Error::other("flush injected"))
            }
        }
        let rt = Arc::new(tokio::runtime::Runtime::new().unwrap());
        let dir = crate::test_util::test_temp_dir("movement-result-ui");
        let mut menu =
            crate::ui::menu::MainMenu::new(&dir, rt.clone(), "test".into(), "test".into(), None);
        for write in [true, false] {
            let (r, rx) = ready(1);
            r.state.lock().session.as_mut().unwrap().path = PathBuf::from("movement-test.jsonl");
            menu.movement_recording = Some(r.clone());
            let (tx, _) = tokio::sync::mpsc::unbounded_channel();
            let (_, event_rx) = crossbeam_channel::unbounded();
            let connection = crate::net::connection::ConnectionHandle {
                event_rx,
                packet_tx: crate::net::sender::PacketSender::with_recorder(tx, r.clone()),
                task: rt.spawn(std::future::pending()),
            };
            for _ in 0..2 {
                r.record("local", "test", || Some(json!({"tick":1})));
            }
            drop(connection); // Actual disconnect stops producers, not the writer.
            assert!(!r.active());
            assert!(r.status().starts_with("Draining"));
            let result = r.write_log(Fail(write), rx, 776, LIMIT);
            assert!(result.is_err());
            r.finish(result); // Production writer completion after disconnect.
            drop(r);
            assert!(
                menu.movement_recording
                    .as_ref()
                    .unwrap()
                    .state
                    .lock()
                    .session
                    .is_none()
            );
            let text = |t: &str, s: f32| t.chars().count() as f32 * s;
            let input = crate::ui::menu::MenuInput::default();
            for disconnected in [false, true] {
                if disconnected {
                    menu.show_disconnect("Connection lost".into());
                }
                let ui = menu.build(854.0, 480.0, &input, text);
                let visible: String = ui
                    .elements
                    .iter()
                    .filter_map(|e| match e {
                        crate::renderer::pipelines::menu_overlay::MenuElement::Text {
                            text,
                            ..
                        } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect();
                assert!(visible.contains("Recording FAILED"));
                assert!(visible.contains("movement-test.jsonl"));
                assert!(visible.contains("dropped=1"));
                assert!(visible.contains("disconnect_or_app_exit"));
            }
            // A new connection's recorder is opt-in, and cannot clear the result.
            let next =
                crate::net::sender::PacketSender::new(tokio::sync::mpsc::unbounded_channel().0);
            assert!(!next.recorder.active());
            assert!(next.recorder.status().is_empty());
            assert!(
                menu.movement_recording
                    .as_ref()
                    .unwrap()
                    .status()
                    .starts_with("Recording FAILED")
            );
        }
    }
    #[test]
    fn start_stop_create_new_and_error_status() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let dir = crate::test_util::test_temp_dir("movement-record");
        std::fs::create_dir_all(&dir).unwrap();
        let r = Arc::new(Recorder::default());
        let path = dir.join("movement-test.jsonl");
        r.start_path(&rt, path.clone());
        r.record("local", "fixed_tick", || Some(json!({"tick":1})));
        r.stop("user_stop");
        fn wait(r: &Recorder) {
            let start = Instant::now();
            while r.state.lock().session.is_some() {
                assert!(start.elapsed().as_secs() < 5);
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }
        wait(&r);
        assert!(r.status().starts_with("Ended"));
        let bytes = std::fs::read(&path).unwrap();
        let footer = rows(&bytes).pop().unwrap();
        assert_eq!(footer["type"], "footer");
        assert_eq!(footer["reason"], "user_stop");
        assert_eq!(footer["complete"], true);
        assert_eq!(footer["dropped"], 0);
        assert!(r.status().contains("dropped=0"));
        assert!(r.status().contains(&path.display().to_string()));
        assert!(!r.active());
        r.start_path(&rt, path.clone());
        wait(&r);
        assert!(r.status().starts_with("Recording FAILED"));
        assert_eq!(std::fs::read(path).unwrap(), bytes);
        r.start_path(&rt, dir.join("missing/record.jsonl"));
        wait(&r);
        assert!(r.status().starts_with("Recording FAILED"));
        assert!(!Arc::new(Recorder::default()).active());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
