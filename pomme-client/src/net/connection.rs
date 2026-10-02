use azalea_protocol::address::ServerAddr;
use azalea_protocol::packets::ClientIntention;
use azalea_protocol::packets::config::{ClientboundConfigPacket, ServerboundConfigPacket};
use azalea_protocol::packets::game::{ClientboundGamePacket, ServerboundGamePacket};
use azalea_protocol::packets::login::c_hello::ClientboundHello;
use azalea_protocol::packets::login::s_hello::ServerboundHello;
use azalea_protocol::packets::login::s_key::ServerboundKey;
use azalea_protocol::packets::login::s_login_acknowledged::ServerboundLoginAcknowledged;
use azalea_protocol::packets::login::{ClientboundLoginPacket, ServerboundLoginPacket};
use azalea_protocol::read::{ReadPacketError, deserialize_packet};
use crossbeam_channel::Sender;
use pomme_protocol::{Direction, PacketTable, Phase};
use thiserror::Error;
use tokio::sync::mpsc;

use super::NetworkEvent;
use super::chat::ChatPacketError;
use super::chat_security::{ChatSender, ProfileKeyPair};
use super::conn::{Conn, MemoryEnd, RawWriter};
use super::handler::{handle_game_packet_with_display_text, handle_raw_game_packet};
use super::sender::{Outbound, PacketSender};
use crate::ui::server_dialog::DialogRegistry;

#[derive(Error, Debug)]
pub enum ConnectionError {
    #[error("invalid server address: {0}")]
    InvalidAddress(String),

    #[error("connection failed: {0}")]
    Connect(std::io::Error),

    #[error("packet read error: {0}")]
    Read(#[from] Box<ReadPacketError>),

    #[error("packet write error: {0}")]
    Write(#[from] std::io::Error),

    #[error("authentication failed: {0}")]
    Auth(String),

    #[error("disconnected by server: {0}")]
    Disconnected(String),

    #[error("server requested transfer")]
    Transferred,

    #[error("network event receiver closed")]
    EventReceiverGone,

    #[error("encryption failed: {0}")]
    Encryption(String),

    /// A client-side disconnect with a vanilla translation key.
    #[error("{}", crate::lang::translate(.0).unwrap_or(.0))]
    ClientDisconnect(&'static str),

    #[error("joining {0} servers is not supported yet")]
    Unjoinable(String),
}

impl From<super::resolve::ConnectError> for ConnectionError {
    fn from(e: super::resolve::ConnectError) -> Self {
        use super::resolve::ConnectError;
        match e {
            ConnectError::Resolve(e) => Self::InvalidAddress(e.to_string()),
            ConnectError::Io(e) => Self::Connect(e),
        }
    }
}

pub enum Transport {
    Remote {
        server: String,
        /// The server's protocol from an earlier server-list ping, when joining
        /// from the list; saves `negotiate_wire_version` its status probe.
        protocol: Option<i32>,
    },
    /// An integrated server in this process, reached over an in-memory pipe.
    #[allow(dead_code, reason = "only a singleplayer build opens a world")]
    Memory(MemoryEnd),
}

pub struct ConnectArgs {
    pub transport: Transport,
    /// Transfer is explicit even when no cookies were supplied.
    pub is_transfer: bool,
    pub username: String,
    pub uuid: uuid::Uuid,
    pub access_token: Option<String>,
    pub view_distance: u8,
    pub chat_options: crate::ui::chat::ChatOptions,
    pub main_hand_right: bool,
    pub particle_mode: crate::particle::ParticleMode,
    pub skin_parts_mask: u8,
    pub server_cookies: std::collections::HashMap<azalea_registry::identifier::Identifier, Vec<u8>>,
}

pub struct ConnectionHandle {
    pub event_rx: crossbeam_channel::Receiver<NetworkEvent>,
    pub packet_tx: PacketSender,
    pub task: tokio::task::JoinHandle<()>,
}

impl Drop for ConnectionHandle {
    fn drop(&mut self) {
        self.packet_tx.recorder.stop("disconnect_or_app_exit");
        self.task.abort();
        // The session is over: restore the launched version's wire protocol
        // and block table so nothing stale leaks into the next one.
        crate::version::clear_session_protocol();
        crate::world::block::set_active_protocol(crate::version::selected_protocol());
    }
}

pub fn spawn_connection(rt: &tokio::runtime::Runtime, args: ConnectArgs) -> ConnectionHandle {
    let (event_tx, event_rx) = crossbeam_channel::bounded(4096);
    let (packet_tx, packet_rx) = mpsc::unbounded_channel::<Outbound>();
    let game_packet_tx = packet_tx.clone();
    let recorder = std::sync::Arc::new(crate::movement_record::Recorder::default());
    let packet_tx = PacketSender::with_recorder(packet_tx, recorder.clone());
    let task = rt.spawn(async move {
        let result = connect_recorded(
            args,
            event_tx.clone(),
            game_packet_tx,
            packet_rx,
            recorder.clone(),
        )
        .await;
        recorder.stop("disconnect");
        if let Err(e) = result {
            if matches!(
                e,
                ConnectionError::Transferred | ConnectionError::EventReceiverGone
            ) {
                return;
            }
            tracing::error!("Network error: {e}");
            let reason = friendly_error_reason(&e);
            let _ = send_event(&event_tx, NetworkEvent::Disconnected { reason }).await;
        }
    });
    ConnectionHandle {
        event_rx,
        packet_tx,
        task,
    }
}

pub async fn connect_to_server(
    args: ConnectArgs,
    event_tx: Sender<NetworkEvent>,
    game_packet_tx: mpsc::UnboundedSender<Outbound>,
    game_packet_rx: mpsc::UnboundedReceiver<Outbound>,
) -> Result<(), ConnectionError> {
    connect_recorded(
        args,
        event_tx,
        game_packet_tx,
        game_packet_rx,
        Default::default(),
    )
    .await
    .or_else(|error| match error {
        ConnectionError::EventReceiverGone => Ok(()),
        error => Err(error),
    })
}

async fn connect_recorded(
    args: ConnectArgs,
    event_tx: Sender<NetworkEvent>,
    game_packet_tx: mpsc::UnboundedSender<Outbound>,
    mut game_packet_rx: mpsc::UnboundedReceiver<Outbound>,
    recorder: std::sync::Arc<crate::movement_record::Recorder>,
) -> Result<(), ConnectionError> {
    let ConnectArgs {
        transport,
        is_transfer,
        username,
        uuid,
        access_token,
        view_distance,
        chat_options,
        main_hand_right,
        particle_mode,
        skin_parts_mask,
        mut server_cookies,
    } = args;

    let intention = if is_transfer {
        ClientIntention::Transfer
    } else {
        ClientIntention::Login
    };
    let mut conn = match transport {
        Transport::Remote { server, protocol } => {
            let server_addr: ServerAddr = server
                .as_str()
                .try_into()
                .map_err(|_| ConnectionError::InvalidAddress(server.clone()))?;
            negotiate_wire_version(&server_addr, protocol).await?;
            super::resolve::connect(&server_addr, intention).await?
        }
        Transport::Memory(end) => {
            // The integrated server speaks the native protocol, so there is
            // nothing to probe and translation stays inert for the session.
            #[cfg(feature = "singleplayer")]
            const _: () =
                assert!(pomme_singleplayer::PROTOCOL == pomme_protocol::version::NATIVE.protocol);
            adopt_wire_protocol(pomme_protocol::version::NATIVE.protocol);
            let mut conn = Conn::from_memory(end);
            super::resolve::send_intention(&mut conn, "localhost", 0, intention).await?;
            conn
        }
    };
    conn.recorder = Some(recorder.clone());

    let hello = ServerboundLoginPacket::Hello(ServerboundHello {
        name: username.clone(),
        profile_id: uuid,
    });
    let frame = serialize_frame(&hello)?;
    let frame = match super::translate::active() {
        Some(t) => t.translate_outbound_login_frame(frame),
        None => frame,
    };
    conn.writer.write(&frame).await?;

    tracing::info!("Sent login hello as {username} ({uuid})");
    if access_token.is_none() {
        tracing::warn!(
            "Connecting offline (no access token). The server keys op/permissions to the \
             authenticated account, so op-only commands like /time may return \"Unknown command\" \
             under this offline identity."
        );
    }

    let (profile_id, profile_name) = login_sequence(
        &mut conn,
        &uuid,
        access_token.as_deref(),
        &mut server_cookies,
    )
    .await?;

    // 1.20.1 and older have no configuration phase: the server enters play as
    // soon as it has sent the profile, and the registries ride in the game
    // login packet rather than in registry_data packets.
    let no_config = super::translate::active().is_some_and(|t| t.no_config_phase());
    if !no_config {
        conn.write_packet(ServerboundLoginAcknowledged {}).await?;
    }

    let joined = if no_config {
        tracing::info!("Skipping configuration phase");
        read_inline_registries(&mut conn).await?
    } else {
        tracing::info!("Entering configuration phase");
        Joined {
            configured: config_sequence(
                &mut conn,
                view_distance,
                chat_options,
                main_hand_right,
                particle_mode,
                skin_parts_mask,
                &recorder,
                &event_tx,
                &mut game_packet_rx,
                &mut server_cookies,
                None,
            )
            .await?,
            deferred_login: None,
        }
    };

    tracing::info!("Entering game state");
    let (key_pair_tx, key_pair_rx) = mpsc::unbounded_channel();
    game_loop(
        conn,
        &event_tx,
        GameLoopArgs {
            connected_profile: Some(profile_name),
            recorder,
            outbound_tx: game_packet_tx,
            outbound_rx: game_packet_rx,
            joined,
            view_distance,
            chat_options,
            main_hand_right,
            particle_mode,
            skin_parts_mask,
            chat: ChatSender::new(profile_id, uuid, access_token, key_pair_tx),
            key_pair_rx,
            server_cookies,
        },
    )
    .await
}

/// What the phases before the game loop produced.
struct Joined {
    configured: Configured,
    /// The game `login` frame, when it was already read off the wire: a server
    /// with no configuration phase sends it before the game loop starts, which
    /// then replays it as its first packet.
    deferred_login: Option<Box<[u8]>>,
}

/// What a configuration phase leaves the session with.
struct Configured {
    registries: std::sync::Arc<azalea_core::registry_holder::RegistryHolder>,
    timeline_ids: Option<Vec<azalea_registry::identifier::Identifier>>,
    timeline_tags: std::collections::HashMap<
        azalea_registry::identifier::Identifier,
        Vec<azalea_registry::identifier::Identifier>,
    >,
    dialogs: std::sync::Arc<DialogRegistry>,
    loom_patterns: std::sync::Arc<crate::ui::loom::PatternData>,
}

fn timeline_ids_from_entries<T>(
    entries: &[(azalea_registry::identifier::Identifier, Option<T>)],
) -> Result<Vec<azalea_registry::identifier::Identifier>, ConnectionError> {
    validate_timeline_ids(entries.iter().map(|(key, _)| key.clone()).collect())
}

fn validate_timeline_ids(
    ids: Vec<azalea_registry::identifier::Identifier>,
) -> Result<Vec<azalea_registry::identifier::Identifier>, ConnectionError> {
    use std::collections::HashSet;
    if ids.len() > i32::MAX as usize {
        return Err(ConnectionError::Disconnected(
            "minecraft:timeline registry exceeds numeric ID limit".into(),
        ));
    }
    let mut seen = HashSet::with_capacity(ids.len());
    if ids.iter().any(|id| !seen.insert(id)) {
        return Err(ConnectionError::Disconnected(
            "minecraft:timeline registry contains duplicate IDs".into(),
        ));
    }
    Ok(ids)
}

fn resolve_timeline_tags(
    tags: &azalea_protocol::common::tags::TagMap,
    ids: Option<&[azalea_registry::identifier::Identifier]>,
) -> Result<
    std::collections::HashMap<
        azalea_registry::identifier::Identifier,
        Vec<azalea_registry::identifier::Identifier>,
    >,
    ConnectionError,
> {
    let registry: azalea_registry::identifier::Identifier = "minecraft:timeline".into();
    let Some(tags) = tags.0.get(&registry) else {
        return Ok(Default::default());
    };
    let ids = ids.ok_or_else(|| {
        ConnectionError::Disconnected(
            "minecraft:timeline tags received without minecraft:timeline registry".into(),
        )
    })?;
    let entries: Vec<_> = tags
        .iter()
        .map(|tag| (tag.name.clone(), tag.elements.clone()))
        .collect();
    resolve_timeline_tag_entries(&entries, Some(ids))
}

fn resolve_timeline_tag_entries(
    tags: &[(azalea_registry::identifier::Identifier, Vec<i32>)],
    ids: Option<&[azalea_registry::identifier::Identifier]>,
) -> Result<
    std::collections::HashMap<
        azalea_registry::identifier::Identifier,
        Vec<azalea_registry::identifier::Identifier>,
    >,
    ConnectionError,
> {
    if tags.is_empty() {
        return Ok(Default::default());
    }
    let ids = ids.ok_or_else(|| {
        ConnectionError::Disconnected(
            "minecraft:timeline tags received without minecraft:timeline registry".into(),
        )
    })?;
    let mut resolved = std::collections::HashMap::with_capacity(tags.len());
    for (name, tag_elements) in tags {
        let mut elements = Vec::with_capacity(tag_elements.len());
        for &id in tag_elements {
            let index = usize::try_from(id).map_err(|_| {
                ConnectionError::Disconnected(format!(
                    "minecraft:timeline tag {name} has negative numeric ID {id}"
                ))
            })?;
            elements.push(ids.get(index).cloned().ok_or_else(|| {
                ConnectionError::Disconnected(format!(
                    "minecraft:timeline tag {name} numeric ID {id} is out of range ({} entries)",
                    ids.len()
                ))
            })?);
        }
        resolved.insert(name.clone(), elements);
    }
    Ok(resolved)
}

/// Reads the registries a pre-configuration-phase server ships inside its game
/// `login` packet, returning them with the untranslated login frame for the
/// game loop to replay. That frame is the first the server sends after the
/// profile (`PlayerList.placeNewPlayer`), with no acknowledgement in between.
async fn read_inline_registries(conn: &mut Conn) -> Result<Joined, ConnectionError> {
    use azalea_core::registry_holder::RegistryHolder;

    let login = tokio::time::timeout(PHASE_READ_TIMEOUT, conn.reader.read())
        .await
        .map_err(|_| phase_read_timeout())??;
    let translation = super::translate::active().expect("translation for a config-less version");
    let Some(frames) = translation.split_login_registries(&login) else {
        // A server that turns the join away here does it with a play-phase
        // disconnect, the login phase having already ended.
        if let Some(raw) = translation.translate_game_frame(login)
            && let Ok(ClientboundGamePacket::Disconnect(p)) =
                deserialize_packet::<ClientboundGamePacket>(&mut std::io::Cursor::new(&raw))
        {
            return Err(ConnectionError::Disconnected(format!("{}", p.reason)));
        }
        return Err(ConnectionError::Disconnected(
            "could not read the registries from the login packet".into(),
        ));
    };

    let mut registry_holder = RegistryHolder::default();
    let mut timeline_ids = None;
    let mut loom_patterns = crate::ui::loom::PatternData::default();
    translation.clear_dynamic_registries();
    for frame in frames {
        match deserialize_packet::<ClientboundConfigPacket>(&mut std::io::Cursor::new(&frame)) {
            Ok(ClientboundConfigPacket::RegistryData(p)) => {
                translation.replace_dynamic_registry(
                    &p.registry_id.to_string(),
                    p.entries.iter().map(|(name, _)| name.to_string()).collect(),
                );
                if p.registry_id.to_string() == "minecraft:banner_pattern" {
                    loom_patterns.replace_registry(
                        p.entries
                            .iter()
                            .map(|(key, data)| (key.to_string(), data.clone()))
                            .collect(),
                    );
                }
                if p.registry_id.to_string() == "minecraft:timeline" {
                    timeline_ids = Some(timeline_ids_from_entries(&p.entries)?);
                }
                registry_holder.append(p.registry_id, p.entries);
            }
            Ok(_) => {}
            Err(e) => skip_malformed_packet(e)?,
        }
    }
    Ok(Joined {
        configured: Configured {
            registries: std::sync::Arc::new(registry_holder),
            timeline_ids,
            timeline_tags: Default::default(),
            dialogs: Default::default(),
            loom_patterns: std::sync::Arc::new(loom_patterns),
        },
        deferred_login: Some(login),
    })
}

/// Adopts the server's protocol as the wire version when translation data
/// for it exists, so one client joins any supported server version;
/// otherwise the launched version is kept (and the server shows its own
/// mismatch message, as before). A launched version without translation
/// data (listed for pings, not yet joinable) can never complete a join —
/// the handshake either gets the server's mismatch rejection or succeeds
/// and breaks mid-connect on untranslated packets — so it is refused up
/// front on every path, a failed probe or a stale server-list `known`
/// protocol included. The protocol comes from `known` (a server-list ping)
/// or a status probe. Sets the session protocol and the matching
/// block-state table, so it must run before the login handshake and before
/// any world state loads.
async fn negotiate_wire_version(
    server_addr: &ServerAddr,
    known: Option<i32>,
) -> Result<(), ConnectionError> {
    let selected = crate::version::selected_protocol();
    let probed = match known {
        Some(p) => Some(p),
        None => {
            let probe = async {
                let (status, _) = super::resolve::request_status(server_addr).await.ok()?;
                Some(status.version.protocol)
            };
            tokio::time::timeout(std::time::Duration::from_secs(5), probe)
                .await
                .ok()
                .flatten()
        }
    };
    let wire = resolve_wire(probed, selected).map_err(|p| {
        let name = pomme_protocol::ProtocolVersion::from_protocol(p)
            .map(|v| v.name.to_string())
            .unwrap_or_else(|| format!("protocol {p}"));
        ConnectionError::Unjoinable(name)
    })?;
    tracing::info!("Negotiated wire protocol {wire}");
    adopt_wire_protocol(wire);
    Ok(())
}

/// Speaks `wire` for the rest of the session. The translation layer and the
/// block-state tables both key off it, so they always move together.
fn adopt_wire_protocol(wire: i32) {
    crate::version::set_session_protocol(wire);
    crate::world::block::set_active_protocol(wire);
}

/// The wire protocol to speak given the probed server protocol and the
/// launched (`selected`) one; `Err` carries an unjoinable outcome (see
/// [`negotiate_wire_version`]). Inert while `selected` is joinable — every
/// arm then yields a joinable wire.
fn resolve_wire(probed: Option<i32>, selected: i32) -> Result<i32, i32> {
    let wire = match probed {
        Some(p) if super::translate::joinable(p) => p,
        Some(p) => {
            tracing::warn!("Server speaks unsupported protocol {p}; falling back to {selected}");
            selected
        }
        None => {
            tracing::warn!("Server protocol probe failed; falling back to {selected}");
            selected
        }
    };
    if super::translate::joinable(wire) {
        Ok(wire)
    } else {
        Err(wire)
    }
}

/// Returns the accepted game profile id and exact scoreboard name.
const PHASE_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

async fn login_sequence(
    conn: &mut Conn,
    uuid: &uuid::Uuid,
    access_token: Option<&str>,
    server_cookies: &mut std::collections::HashMap<
        azalea_registry::identifier::Identifier,
        Vec<u8>,
    >,
) -> Result<(uuid::Uuid, String), ConnectionError> {
    loop {
        // Read the raw frame ourselves so older-version layouts can be
        // rewritten before the typed decode (26.1's login_finished lacks the
        // trailing session id).
        let raw = tokio::time::timeout(PHASE_READ_TIMEOUT, conn.reader.read())
            .await
            .map_err(|_| phase_read_timeout())??;
        let raw = match super::translate::active() {
            Some(t) => t.translate_login_frame(raw),
            None => raw,
        };
        let packet: ClientboundLoginPacket = deserialize_packet(&mut std::io::Cursor::new(&raw))?;
        tracing::info!("Login packet: {:?}", std::mem::discriminant(&packet));
        match packet {
            ClientboundLoginPacket::Hello(p) => {
                handle_encryption(conn, &p, uuid, access_token).await?;
            }
            ClientboundLoginPacket::LoginCompression(p) => {
                conn.set_compression_threshold(p.compression_threshold);
                tracing::info!(
                    "Compression enabled (threshold: {})",
                    p.compression_threshold
                );
            }
            ClientboundLoginPacket::LoginFinished(p) => {
                tracing::info!(
                    "Login success: {} ({})",
                    p.game_profile.name,
                    p.game_profile.uuid
                );
                return Ok((p.game_profile.uuid, p.game_profile.name));
            }
            ClientboundLoginPacket::LoginDisconnect(p) => {
                return Err(ConnectionError::Disconnected(format!("{}", p.reason)));
            }
            ClientboundLoginPacket::CookieRequest(p) => {
                conn.write_packet(
                    azalea_protocol::packets::login::s_cookie_response::ServerboundCookieResponse {
                        payload: server_cookies.get(&p.key).cloned(),
                        key: p.key,
                    },
                )
                .await?;
            }
            ClientboundLoginPacket::CustomQuery(p) => {
                conn.write_packet(
                    azalea_protocol::packets::login::s_custom_query_answer::ServerboundCustomQueryAnswer {
                        transaction_id: p.transaction_id,
                        data: None,
                    },
                )
                .await?;
            }
        }
    }
}

async fn handle_encryption(
    conn: &mut Conn,
    hello: &ClientboundHello,
    uuid: &uuid::Uuid,
    access_token: Option<&str>,
) -> Result<(), ConnectionError> {
    let e = azalea_crypto::encrypt(&hello.public_key, &hello.challenge)
        .map_err(ConnectionError::Encryption)?;

    if hello.should_authenticate {
        let access_token = access_token.ok_or_else(|| {
            ConnectionError::Auth(
                "server requires authentication but no access token provided".into(),
            )
        })?;

        tracing::info!("Authenticating with session server (uuid: {uuid})");
        azalea_auth::sessionserver::join(azalea_auth::sessionserver::SessionServerJoinOpts {
            access_token,
            public_key: &hello.public_key,
            private_key: &e.secret_key,
            uuid,
            server_id: &hello.server_id,
            proxy: None,
        })
        .await
        .map_err(|e| ConnectionError::Auth(e.to_string()))?;
        tracing::info!("Session server authentication successful");
    } else {
        tracing::info!("Server does not require authentication");
    }

    conn.write_packet(ServerboundKey {
        key_bytes: e.encrypted_public_key,
        encrypted_challenge: e.encrypted_challenge,
    })
    .await?;

    conn.set_encryption_key(e.secret_key);
    tracing::info!("Encryption enabled");
    Ok(())
}

async fn config_sequence(
    conn: &mut Conn,
    view_distance: u8,
    chat_options: crate::ui::chat::ChatOptions,
    main_hand_right: bool,
    particle_mode: crate::particle::ParticleMode,
    skin_parts_mask: u8,
    recorder: &crate::movement_record::Recorder,
    event_tx: &Sender<NetworkEvent>,
    outbound_rx: &mut mpsc::UnboundedReceiver<Outbound>,
    server_cookies: &mut std::collections::HashMap<
        azalea_registry::identifier::Identifier,
        Vec<u8>,
    >,
    // `Some` on a mid-session reconfiguration: the previous registries,
    // kept when the server re-sends nothing (vanilla's RegistryDataCollector
    // returns the original registries unchanged in that case).
    previous: Option<&Configured>,
) -> Result<Configured, ConnectionError> {
    use azalea_core::registry_holder::RegistryHolder;
    use azalea_protocol::packets::config::*;

    let mut registry_holder = RegistryHolder::default();
    let mut timeline_ids = previous.and_then(|p| p.timeline_ids.clone());
    let mut timeline_tags = previous
        .map(|p| p.timeline_tags.clone())
        .unwrap_or_default();
    let mut loom_patterns = previous
        .map(|p| (*p.loom_patterns).clone())
        .unwrap_or_default();
    if previous.is_none() {
        if let Some(translation) = super::translate::active() {
            translation.clear_dynamic_registries();
        }
    }
    let mut received_registry_data = false;
    let mut received_loom_tags = false;
    let mut selected_known_packs = false;
    let mut received_dialog_tags = None;
    let mut code_of_conduct_seen = false;
    let mut code_of_conduct_accepted = false;
    let mut finish_configuration_pending = false;

    // Vanilla sends brand and client information once, from the login
    // listener; a reconfiguration sends neither.
    if previous.is_none() {
        // Some servers key off the brand.
        write_config_packet(
            conn,
            ServerboundConfigPacket::CustomPayload(s_custom_payload::ServerboundCustomPayload {
                identifier: "minecraft:brand".into(),
                data: super::brand_payload().into(),
            }),
        )
        .await?;

        write_config_packet(
            conn,
            ServerboundConfigPacket::ClientInformation(
                s_client_information::ServerboundClientInformation {
                    information: super::client_information(
                        view_distance,
                        chat_options,
                        main_hand_right,
                        particle_mode,
                        skin_parts_mask & 0x7f,
                    ),
                },
            ),
        )
        .await?;
    }

    // Config frames are read raw so older wire versions translate (765's
    // registry_data fans out into several frames, hence the queue).
    let mut pending = std::collections::VecDeque::new();
    loop {
        if finish_configuration_pending && (!code_of_conduct_seen || code_of_conduct_accepted) {
            write_config_packet(
                conn,
                ServerboundConfigPacket::FinishConfiguration(
                    s_finish_configuration::ServerboundFinishConfiguration {},
                ),
            )
            .await?;
            return Ok(match previous {
                Some(previous) if !received_registry_data => Configured {
                    registries: previous.registries.clone(),
                    timeline_ids,
                    timeline_tags,
                    loom_patterns: std::sync::Arc::new(loom_patterns),
                    dialogs: match received_dialog_tags {
                        Some(tags) => std::sync::Arc::new(previous.dialogs.with_tags(tags)),
                        None => previous.dialogs.clone(),
                    },
                },
                _ => Configured {
                    dialogs: std::sync::Arc::new(dialog_registry(
                        &registry_holder,
                        received_dialog_tags.unwrap_or_default(),
                    )),
                    registries: std::sync::Arc::new(registry_holder),
                    timeline_ids,
                    timeline_tags,
                    loom_patterns: std::sync::Arc::new(loom_patterns),
                },
            });
        }
        let packet = if let Some(packet) = pending.pop_front() {
            packet
        } else {
            tokio::select! {
                raw = tokio::time::timeout(PHASE_READ_TIMEOUT, conn.reader.read()) => {
                    let raw = match raw {
                        Err(_) => return Err(phase_read_timeout()),
                        Ok(Ok(raw)) => raw,
                        Ok(Err(e)) => {
                            skip_malformed_packet(e)?;
                            continue;
                        }
                    };
                    recorder.raw_packet("inbound", "configuration", &raw);
                    let frames = match super::translate::active() {
                        Some(t) => t.translate_config_frame(raw),
                        None => vec![raw],
                    };
                    for frame in frames {
                        if pump_config_future(
                            super::dialog::handle_raw_dialog_packet(Phase::Configuration, &frame, event_tx),
                            conn, outbound_rx, code_of_conduct_seen, &mut code_of_conduct_accepted,
                        ).await?? {
                            continue;
                        }
                        match deserialize_packet::<ClientboundConfigPacket>(
                            &mut std::io::Cursor::new(&frame),
                        ) {
                            Ok(packet) => pending.push_back(packet),
                            Err(e) => skip_malformed_packet(e)?,
                        }
                    }
                    continue;
                }
                // `Some(..)` disables the branch when the channel closes instead
                // of busy-looping on a closed receiver.
                Some(outbound) = outbound_rx.recv() => {
                    write_config_outbound(conn, outbound, code_of_conduct_seen, &mut code_of_conduct_accepted).await?;
                    // Anything else is discarded: vanilla defers its outbound
                    // queue, but pomme's game keeps ticking through a
                    // reconfiguration, so stale movement/actions are best dropped.
                    continue;
                }
            }
        };
        match packet {
            ClientboundConfigPacket::RegistryData(p) => {
                if !received_registry_data && previous.is_some() {
                    // A replacement holder is built from this config's data;
                    // don't keep ids from registries the new holder omits.
                    let new_tags = received_loom_tags.then(|| {
                        (
                            std::mem::take(&mut loom_patterns.tags),
                            std::mem::take(&mut loom_patterns.item_tags),
                        )
                    });
                    loom_patterns = crate::ui::loom::PatternData::default();
                    timeline_ids = None;
                    timeline_tags.clear();
                    if let Some((tags, item_tags)) = new_tags {
                        loom_patterns.replace_tags(tags, item_tags);
                    }
                    if let Some(translation) = super::translate::active() {
                        translation.clear_dynamic_registries();
                    }
                }
                received_registry_data = true;
                // The server omits the data of every entry the pack we claimed
                // carries, so fill those in before azalea drops them.
                let entries = if selected_known_packs {
                    super::known_packs::fill_known_entries(&p.registry_id, p.entries)
                        .map_err(ConnectionError::Disconnected)?
                } else {
                    p.entries
                };
                if let Some(translation) = super::translate::active() {
                    translation.replace_dynamic_registry(
                        &p.registry_id.to_string(),
                        entries.iter().map(|(name, _)| name.to_string()).collect(),
                    );
                }
                if p.registry_id.to_string() == "minecraft:timeline" {
                    timeline_ids = Some(timeline_ids_from_entries(&entries)?);
                    timeline_tags.clear();
                }
                if p.registry_id.to_string() == "minecraft:banner_pattern" {
                    loom_patterns.replace_registry(
                        entries
                            .iter()
                            .map(|(key, data)| (key.to_string(), data.clone()))
                            .collect(),
                    );
                }
                registry_holder.append(p.registry_id, entries);
            }
            ClientboundConfigPacket::UpdateTags(p) => {
                // A later packet replaces an earlier one's tags per registry.
                if let Some(tags) = dialog_tags(&p.tags) {
                    received_dialog_tags = Some(tags);
                }
                timeline_tags = resolve_timeline_tags(&p.tags, timeline_ids.as_deref())?;
                let (patterns, items) = loom_pattern_tags(&p.tags);
                loom_patterns.replace_tags(patterns, items);
                received_loom_tags = true;
            }
            ClientboundConfigPacket::SelectKnownPacks(p) => {
                // Vanilla `handleSelectKnownPacks`: claim the offered packs we
                // have ourselves, so the server can skip their registry data.
                let known_packs = super::known_packs::select_packs(&p.known_packs);
                selected_known_packs = !known_packs.is_empty();
                write_config_packet(
                    conn,
                    ServerboundConfigPacket::SelectKnownPacks(
                        s_select_known_packs::ServerboundSelectKnownPacks { known_packs },
                    ),
                )
                .await?;
            }
            ClientboundConfigPacket::KeepAlive(p) => {
                write_config_packet(
                    conn,
                    ServerboundConfigPacket::KeepAlive(s_keep_alive::ServerboundKeepAlive {
                        id: p.id,
                    }),
                )
                .await?;
            }
            ClientboundConfigPacket::FinishConfiguration(_) => {
                finish_configuration_pending = true;
            }

            ClientboundConfigPacket::Disconnect(p) => {
                return Err(ConnectionError::Disconnected(format!("{}", p.reason)));
            }
            ClientboundConfigPacket::CookieRequest(p) => {
                let payload = server_cookies.get(&p.key).cloned();
                write_config_packet(
                    conn,
                    ServerboundConfigPacket::CookieResponse(
                        s_cookie_response::ServerboundCookieResponse {
                            key: p.key,
                            payload,
                        },
                    ),
                )
                .await?;
            }
            ClientboundConfigPacket::StoreCookie(p) => {
                server_cookies.insert(p.key, p.payload);
            }
            ClientboundConfigPacket::Ping(p) => {
                write_config_packet(
                    conn,
                    ServerboundConfigPacket::Pong(s_pong::ServerboundPong { id: p.id }),
                )
                .await?;
            }
            ClientboundConfigPacket::CodeOfConduct(p) => {
                if code_of_conduct_seen {
                    return Err(ConnectionError::Disconnected(
                        "Server sent duplicate code-of-conduct notice".into(),
                    ));
                }
                code_of_conduct_seen = true;
                pump_config_future(
                    send_event(
                        event_tx,
                        NetworkEvent::CodeOfConduct {
                            text: p.code_of_conduct,
                        },
                    ),
                    conn,
                    outbound_rx,
                    code_of_conduct_seen,
                    &mut code_of_conduct_accepted,
                )
                .await??;
                // Return to the outer select so inbound configuration packets
                // remain live while the user considers the notice.
            }
            ClientboundConfigPacket::Transfer(p) => {
                pump_config_future(
                    send_event(
                        event_tx,
                        NetworkEvent::ServerTransfer(super::ServerTransfer {
                            host: p.host,
                            port: p.port,
                            cookies: server_cookies.clone(),
                        }),
                    ),
                    conn,
                    outbound_rx,
                    code_of_conduct_seen,
                    &mut code_of_conduct_accepted,
                )
                .await??;
                return Err(ConnectionError::Transferred);
            }
            ClientboundConfigPacket::ResourcePackPush(p) => {
                tracing::info!(
                    "Server pushing resource pack {} (required: {})",
                    p.id,
                    p.required
                );
                pump_config_future(
                    send_event(
                        event_tx,
                        NetworkEvent::ResourcePackPush {
                            id: p.id,
                            url: p.url.clone(),
                            hash: p.hash.clone(),
                            required: p.required,
                        },
                    ),
                    conn,
                    outbound_rx,
                    code_of_conduct_seen,
                    &mut code_of_conduct_accepted,
                )
                .await??;
                write_config_packet(
                    conn,
                    ServerboundConfigPacket::ResourcePack(
                        s_resource_pack::ServerboundResourcePack {
                            id: p.id,
                            action: s_resource_pack::Action::Accepted,
                        },
                    ),
                )
                .await?;
            }
            ClientboundConfigPacket::ResourcePackPop(p) => {
                tracing::info!("Server popping resource pack {:?}", p.id);
                pump_config_future(
                    send_event(event_tx, NetworkEvent::ResourcePackPop { id: p.id }),
                    conn,
                    outbound_rx,
                    code_of_conduct_seen,
                    &mut code_of_conduct_accepted,
                )
                .await??;
            }
            _ => {
                tracing::debug!("Config packet: {:?}", std::mem::discriminant(&packet));
            }
        }
    }
}

async fn pump_config_future<F: std::future::Future>(
    future: F,
    conn: &mut Conn,
    outbound_rx: &mut mpsc::UnboundedReceiver<Outbound>,
    conduct_seen: bool,
    conduct_accepted: &mut bool,
) -> Result<F::Output, ConnectionError> {
    tokio::pin!(future);
    loop {
        tokio::select! {
            result = &mut future => return Ok(result),
            Some(out) = outbound_rx.recv() => {
                write_config_outbound(conn, out, conduct_seen, conduct_accepted).await?;
            }
        }
    }
}

async fn write_config_outbound(
    conn: &mut Conn,
    outbound: Outbound,
    conduct_seen: bool,
    conduct_accepted: &mut bool,
) -> Result<(), ConnectionError> {
    let (outbound, trace) = match outbound {
        Outbound::Traced { trace, packet } => (*packet, Some(trace)),
        packet => (packet, None),
    };
    if let (Some(recorder), Some(trace)) = (&conn.recorder, trace) {
        recorder.packet_stage(trace, "dequeued", None);
    }
    conn.pending_trace = trace;
    let result = write_config_outbound_inner(conn, outbound, conduct_seen, conduct_accepted).await;
    if let Some(trace) = conn.pending_trace.take() {
        if let Some(recorder) = &conn.recorder {
            recorder.packet_stage(
                trace,
                if result.is_err() {
                    "outbound_failed"
                } else {
                    "outbound_skipped"
                },
                Some(if result.is_err() {
                    "configuration_dispatch_error"
                } else {
                    "not_written_in_configuration"
                }),
            );
        }
    }
    result
}

async fn write_config_outbound_inner(
    conn: &mut Conn,
    outbound: Outbound,
    conduct_seen: bool,
    conduct_accepted: &mut bool,
) -> Result<(), ConnectionError> {
    use azalea_protocol::packets::config::*;
    match outbound {
        Outbound::CustomClick { id, payload } => {
            if let Some(frame) = custom_click_frame(Phase::Configuration, &id, payload.as_ref()) {
                write_config_frame(conn, frame).await?;
            }
        }
        Outbound::CodeOfConductDecision(accepted) => {
            if !conduct_seen || *conduct_accepted {
                return Err(ConnectionError::Disconnected(
                    "Code-of-conduct decision arrived without a pending notice".into(),
                ));
            }
            if !accepted {
                return Err(ConnectionError::Disconnected(
                    "Code of conduct declined by user".into(),
                ));
            }
            write_config_packet(
                conn,
                ServerboundConfigPacket::AcceptCodeOfConduct(
                    s_accept_code_of_conduct::ServerboundAcceptCodeOfConduct {},
                ),
            )
            .await?;
            *conduct_accepted = true;
        }
        Outbound::Packet(packet) => {
            if let ServerboundGamePacket::ResourcePack(p) = *packet {
                use azalea_protocol::packets::game::s_resource_pack::Action as GameAction;
                use s_resource_pack::Action;
                let action = match p.action {
                    GameAction::SuccessfullyLoaded => Action::SuccessfullyLoaded,
                    GameAction::Declined => Action::Declined,
                    GameAction::FailedDownload => Action::FailedDownload,
                    GameAction::Accepted => Action::Accepted,
                    GameAction::InvalidUrl => Action::InvalidUrl,
                    GameAction::FailedReload => Action::FailedReload,
                    GameAction::Discarded => Action::Discarded,
                };
                write_config_packet(
                    conn,
                    ServerboundConfigPacket::ResourcePack(
                        s_resource_pack::ServerboundResourcePack { id: p.id, action },
                    ),
                )
                .await?;
            }
        }
        // Movement/actions queued during reconfiguration are stale.
        _ => {}
    }
    Ok(())
}

fn phase_read_timeout() -> ConnectionError {
    ConnectionError::Disconnected(format!(
        "server stopped responding during login/configuration for {} seconds",
        PHASE_READ_TIMEOUT.as_secs()
    ))
}

/// One owned pending event, retried inline: dropping this future cancels
/// delivery. No detached workers; callers await in wire order.
pub(super) async fn send_event(
    event_tx: &Sender<NetworkEvent>,
    mut event: NetworkEvent,
) -> Result<(), crossbeam_channel::SendError<NetworkEvent>> {
    loop {
        match event_tx.try_send(event) {
            Ok(()) => return Ok(()),
            Err(crossbeam_channel::TrySendError::Disconnected(event)) => {
                return Err(crossbeam_channel::SendError(event));
            }
            Err(crossbeam_channel::TrySendError::Full(returned)) => {
                event = returned;
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
        }
    }
}

impl From<crossbeam_channel::SendError<NetworkEvent>> for ConnectionError {
    fn from(_: crossbeam_channel::SendError<NetworkEvent>) -> Self {
        Self::EventReceiverGone
    }
}

fn extract_biome_climate(
    holder: &azalea_core::registry_holder::RegistryHolder,
) -> std::collections::HashMap<u32, crate::renderer::chunk::mesher::BiomeClimate> {
    use crate::renderer::chunk::mesher::{BiomeClimate, GrassColorModifier, int_to_rgb};

    let mut result = std::collections::HashMap::new();
    let biome_key: azalea_registry::identifier::Identifier = "minecraft:worldgen/biome".into();
    if let Some(registry) = holder.extra.get(&biome_key) {
        for (id, (_, nbt)) in registry.map.iter().enumerate() {
            let temp = nbt_float(nbt, "temperature").unwrap_or(0.8);
            let downfall = nbt_float(nbt, "downfall").unwrap_or(0.4);
            let has_precipitation = nbt_bool(nbt, "has_precipitation").unwrap_or(true);

            let effects = nbt.get("effects").and_then(|v| match v {
                simdnbt::owned::NbtTag::Compound(c) => Some(c),
                _ => None,
            });

            let grass_color_override = effects
                .and_then(|e| nbt_color_from_compound(e, "grass_color"))
                .map(int_to_rgb);

            let foliage_color_override = effects
                .and_then(|e| nbt_color_from_compound(e, "foliage_color"))
                .map(int_to_rgb);

            let dry_foliage_color_override = effects
                .and_then(|e| nbt_color_from_compound(e, "dry_foliage_color"))
                .map(int_to_rgb);

            let water_color_override = effects
                .and_then(|e| nbt_color_from_compound(e, "water_color"))
                .map(int_to_rgb);

            let grass_color_modifier = effects
                .and_then(|e| nbt_string_from_compound(e, "grass_color_modifier"))
                .map(|s| match s.as_str() {
                    "dark_forest" => GrassColorModifier::DarkForest,
                    "swamp" => GrassColorModifier::Swamp,
                    _ => GrassColorModifier::None,
                })
                .unwrap_or(GrassColorModifier::None);

            result.insert(
                id as u32,
                BiomeClimate {
                    temperature: temp,
                    downfall,
                    has_precipitation,
                    grass_color_override,
                    grass_color_modifier,
                    foliage_color_override,
                    dry_foliage_color_override,
                    water_color_override,
                },
            );
        }
    }
    tracing::info!("Extracted {} biome climate entries", result.len());
    result
}

fn nbt_bool(nbt: &simdnbt::owned::NbtCompound, key: &str) -> Option<bool> {
    nbt.get(key).and_then(|v| match v {
        simdnbt::owned::NbtTag::Byte(b) => Some(*b != 0),
        _ => None,
    })
}

fn nbt_float(nbt: &simdnbt::owned::NbtCompound, key: &str) -> Option<f32> {
    nbt.get(key).and_then(|v| match v {
        simdnbt::owned::NbtTag::Float(f) => Some(*f),
        simdnbt::owned::NbtTag::Double(d) => Some(*d as f32),
        _ => None,
    })
}

fn nbt_color_from_compound(compound: &simdnbt::owned::NbtCompound, key: &str) -> Option<i32> {
    compound.get(key).and_then(|v| match v {
        simdnbt::owned::NbtTag::Int(i) => Some(*i),
        simdnbt::owned::NbtTag::Long(l) => Some(*l as i32),
        simdnbt::owned::NbtTag::String(s) => {
            let s = s.to_string();
            let hex = s.strip_prefix('#').unwrap_or(&s);
            i32::from_str_radix(hex, 16).ok()
        }
        _ => None,
    })
}

fn chat_types_from_registry_holder(
    holder: &azalea_core::registry_holder::RegistryHolder,
) -> super::chat::ChatTypeRegistry {
    let key: azalea_registry::identifier::Identifier = "minecraft:chat_type".into();
    let entries = holder
        .extra
        .get(&key)
        .map(|registry| registry.map.values().cloned().collect())
        .unwrap_or_default();
    super::chat::ChatTypeRegistry::from_entries(entries)
}

/// The registry the dialog glyphs and holders resolve against.
fn dialog_registry_key() -> azalea_registry::identifier::Identifier {
    "minecraft:dialog".into()
}

fn dialog_registry(
    holder: &azalea_core::registry_holder::RegistryHolder,
    tags: std::collections::HashMap<String, Vec<usize>>,
) -> DialogRegistry {
    let entries = holder
        .extra
        .get(&dialog_registry_key())
        .map(|registry| {
            registry
                .map
                .iter()
                .map(|(id, nbt)| (id.to_string(), nbt.clone()))
                .collect()
        })
        .unwrap_or_default();
    DialogRegistry::new(entries, tags)
}

/// The `minecraft:dialog` tags of an `update_tags` packet, if it has any.
pub(super) fn loom_pattern_tags(
    tags: &azalea_protocol::common::tags::TagMap,
) -> (
    std::collections::HashMap<String, Vec<usize>>,
    std::collections::HashMap<String, std::collections::HashSet<u32>>,
) {
    let patterns: azalea_registry::identifier::Identifier = "minecraft:banner_pattern".into();
    let pattern_tags = tags
        .0
        .get(&patterns)
        .into_iter()
        .flatten()
        .map(|tag| {
            (
                tag.name.to_string(),
                tag.elements
                    .iter()
                    .filter_map(|&id| usize::try_from(id).ok())
                    .collect(),
            )
        })
        .collect();
    let items: azalea_registry::identifier::Identifier = "minecraft:item".into();
    let item_tags = tags
        .0
        .get(&items)
        .into_iter()
        .flatten()
        .filter(|tag| {
            matches!(
                tag.name.to_string().as_str(),
                "minecraft:loom_dyes" | "minecraft:loom_patterns"
            )
        })
        .map(|tag| {
            (
                tag.name.to_string(),
                tag.elements
                    .iter()
                    .filter_map(|&id| u32::try_from(id).ok())
                    .collect(),
            )
        })
        .collect();
    (pattern_tags, item_tags)
}

fn dialog_tags(
    tags: &azalea_protocol::common::tags::TagMap,
) -> Option<std::collections::HashMap<String, Vec<usize>>> {
    let tags = tags.0.get(&dialog_registry_key())?;
    Some(
        tags.iter()
            .map(|tag| {
                let entries = tag
                    .elements
                    .iter()
                    .filter_map(|&id| usize::try_from(id).ok())
                    .collect();
                (tag.name.to_string(), entries)
            })
            .collect(),
    )
}

fn nbt_string_from_compound(compound: &simdnbt::owned::NbtCompound, key: &str) -> Option<String> {
    compound.get(key).and_then(|v| match v {
        simdnbt::owned::NbtTag::String(s) => Some(s.to_string()),
        _ => None,
    })
}

struct GameLoopArgs {
    connected_profile: Option<String>,
    recorder: std::sync::Arc<crate::movement_record::Recorder>,
    outbound_tx: mpsc::UnboundedSender<Outbound>,
    outbound_rx: mpsc::UnboundedReceiver<Outbound>,
    joined: Joined,
    view_distance: u8,
    chat_options: crate::ui::chat::ChatOptions,
    main_hand_right: bool,
    particle_mode: crate::particle::ParticleMode,
    skin_parts_mask: u8,
    chat: ChatSender,
    key_pair_rx: mpsc::UnboundedReceiver<Option<std::sync::Arc<ProfileKeyPair>>>,
    server_cookies: std::collections::HashMap<azalea_registry::identifier::Identifier, Vec<u8>>,
}

async fn game_loop(
    mut conn: Conn,
    event_tx: &Sender<NetworkEvent>,
    args: GameLoopArgs,
) -> Result<(), ConnectionError> {
    let GameLoopArgs {
        connected_profile,
        recorder,
        outbound_tx,
        mut outbound_rx,
        joined,
        view_distance,
        chat_options,
        main_hand_right,
        particle_mode,
        skin_parts_mask,
        mut chat,
        mut key_pair_rx,
        mut server_cookies,
    } = args;
    let Joined {
        mut configured,
        mut deferred_login,
    } = joined;
    let mut chat_types = chat_types_from_registry_holder(&configured.registries);
    let sender = PacketSender::with_recorder(outbound_tx, recorder.clone());
    let mut batch_size_calculator = super::chunk_batch::ChunkBatchSizeCalculator::default();
    // ChunkStore also starts with overworld defaults before any DimensionInfo.
    let mut current_dimension = (384, -64);
    let shared_tree: crate::net::commands::SharedCommandTree =
        std::sync::Arc::new(parking_lot::Mutex::new(None));

    let translation = super::translate::active();
    let mut inbound_chat = super::chat::InboundChat::new(crate::version::session_protocol() >= 770);
    let mut chat_tick = tokio::time::interval(std::time::Duration::from_millis(50));
    chat_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    macro_rules! pump {
        ($future:expr) => {
            pump_game_future(
                $future,
                &mut conn.writer,
                &mut outbound_rx,
                &mut key_pair_rx,
                &mut chat,
                &mut chat_tick,
                translation,
                &shared_tree,
                &recorder,
            )
            .await?
        };
    }
    if let Some(profile_name) = connected_profile {
        pump!(send_event(
            event_tx,
            NetworkEvent::BiomeColors {
                colors: extract_biome_climate(&configured.registries),
            }
        ))?;
        pump!(send_event(
            event_tx,
            NetworkEvent::Connected { profile_name }
        ))?;
    }
    // Registries must arrive before any login or predicted container clicks.
    pump!(send_event(
        event_tx,
        NetworkEvent::Registries(configured.registries.clone())
    ))?;
    pump!(send_event(
        event_tx,
        NetworkEvent::TimelineTags(configured.timeline_tags.clone())
    ))?;
    pump!(send_event(
        event_tx,
        NetworkEvent::DialogRegistry(configured.dialogs.clone())
    ))?;
    pump!(send_event(
        event_tx,
        NetworkEvent::LoomPatterns(configured.loom_patterns.clone())
    ))?;
    if deferred_login.is_some() {
        // 1.20.1 sends both from its login handler, where later versions send
        // them in the configuration phase (ClientPacketListener.handleLogin).
        let info = ServerboundGamePacket::ClientInformation(
            azalea_protocol::packets::game::s_client_information::ServerboundClientInformation {
                client_information: super::client_information(
                    view_distance,
                    chat_options,
                    main_hand_right,
                    particle_mode,
                    skin_parts_mask & 0x7f,
                ),
            },
        );
        write_game_frame(
            &mut conn.writer,
            translation,
            serialize_frame(&info)?,
            &recorder,
        )
        .await?;

        let brand = ServerboundGamePacket::CustomPayload(
            azalea_protocol::packets::game::s_custom_payload::ServerboundCustomPayload {
                identifier: "minecraft:brand".into(),
                data: super::brand_payload().into(),
            },
        );
        write_game_frame(
            &mut conn.writer,
            translation,
            serialize_frame(&brand)?,
            &recorder,
        )
        .await?;
    }
    loop {
        let raw = if let Some(raw) = deferred_login.take() {
            Ok(raw)
        } else {
            // TODO: reads and writes share this task, so a blocked write stops
            // the client reading. Harmless against a socket, but an integrated
            // server on a bounded pipe can deadlock; split the writer out.
            pump!(conn.reader.read())
        };
        let raw = match raw {
            Ok(raw) => raw,
            Err(e) => {
                skip_malformed_packet(e)?;
                continue;
            }
        };
        recorder.raw_packet("inbound", "play", &raw);
        if translation.is_none()
            && crate::version::session_protocol() == pomme_protocol::version::NATIVE.protocol
        {
            match super::native_codecs::decode_native_explosion(&raw) {
                Ok(Some(explosion)) => {
                    pump!(send_event(event_tx, NetworkEvent::Explosion(explosion)))?;
                    continue;
                }
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(%error, "Skipping malformed native ClientboundExplode packet");
                    continue;
                }
            }
        }
        let wire_id = if recorder.active() {
            use azalea_buf::AzBufVar;
            u32::azalea_read_var(&mut std::io::Cursor::new(raw.as_ref())).ok()
        } else {
            None
        };
        let raw = match translation {
            Some(t) => match t.translate_game_frame(raw) {
                Some(raw) => raw,
                None => continue,
            },
            None if crate::version::session_protocol()
                == pomme_protocol::version::NATIVE.protocol =>
            {
                match super::native_codecs::normalize_native_team_color(&raw) {
                    Ok(Some(normalized)) => normalized.into_boxed_slice(),
                    Ok(None) => raw,
                    Err(error) => {
                        tracing::warn!(%error, "Skipping malformed native set_player_team packet");
                        continue;
                    }
                }
            }
            None => raw,
        };
        match pump!(super::chat::handle_raw_chat_packet(
            &raw,
            event_tx,
            &chat_types,
            &mut inbound_chat
        )) {
            Some(Err(ChatPacketError::Malformed(error))) => {
                tracing::warn!("Skipping malformed chat packet: {error}");
                continue;
            }
            Some(Err(ChatPacketError::Disconnect(key))) => {
                return Err(ConnectionError::ClientDisconnect(key));
            }
            Some(Err(ChatPacketError::ReceiverGone)) => {
                return Err(ConnectionError::EventReceiverGone);
            }
            Some(Ok(())) => continue,
            None => {}
        }
        if pump!(super::dialog::handle_raw_dialog_packet(
            Phase::Game,
            &raw,
            event_tx
        ))? || pump!(handle_raw_game_packet(&raw, event_tx))?
        {
            continue;
        }
        let mut recorded_metadata = None;
        let mut display_text = std::collections::VecDeque::new();
        let raw = match super::handler::preserve_legacy_display_text(&raw) {
            Ok(Some((remaining, events))) => {
                if recorder.active() {
                    // Keep recording the original metadata packet, not the UI-only
                    // remainder. Extra typed decode is restricted to active recording.
                    recorded_metadata = deserialize_packet::<ClientboundGamePacket>(
                        &mut std::io::Cursor::new(raw.as_ref()),
                    )
                    .ok();
                }
                display_text = events.into();
                remaining.into_boxed_slice()
            }
            Ok(None) => raw,
            Err(error) => {
                tracing::warn!(%error, "Skipping malformed legacy text display metadata");
                continue;
            }
        };
        match deserialize_packet::<ClientboundGamePacket>(&mut std::io::Cursor::new(&raw)) {
            Ok(mut packet) => {
                recorder.inbound(recorded_metadata.as_ref().unwrap_or(&packet), wire_id);
                if matches!(packet, ClientboundGamePacket::StartConfiguration(_)) {
                    // Vanilla clears the client level before acknowledging
                    // (ClientPacketListener.handleConfigurationStart); chat
                    // survives the transition. Whatever the game queued goes
                    // first, then the pending chat acknowledgement.
                    // TODO: chat events still in flight to the game thread
                    // miss this ack; the next login resets the tracker anyway.
                    pump!(send_event(event_tx, NetworkEvent::Reconfiguring))?;
                    while let Ok(out) = outbound_rx.try_recv() {
                        if let Some((frame, trace)) = recorded_outbound_frame(
                            out,
                            translation,
                            &mut chat,
                            &shared_tree,
                            &recorder,
                        )? {
                            write_game_frame_traced(
                                &mut conn.writer,
                                translation,
                                frame,
                                &recorder,
                                trace,
                            )
                            .await?;
                        }
                    }
                    if let Some(frame) = chat.flush_ack() {
                        write_game_frame(&mut conn.writer, translation, frame, &recorder).await?;
                    }
                    let ack = ServerboundGamePacket::ConfigurationAcknowledged(
                        azalea_protocol::packets::game::s_configuration_acknowledged::ServerboundConfigurationAcknowledged,
                    );
                    write_game_frame(
                        &mut conn.writer,
                        translation,
                        serialize_frame(&ack)?,
                        &recorder,
                    )
                    .await?;
                    let next = config_sequence(
                        &mut conn,
                        view_distance,
                        chat_options,
                        main_hand_right,
                        particle_mode,
                        skin_parts_mask,
                        &recorder,
                        event_tx,
                        &mut outbound_rx,
                        &mut server_cookies,
                        Some(&configured),
                    )
                    .await?;
                    if !std::sync::Arc::ptr_eq(&next.registries, &configured.registries) {
                        chat_types = chat_types_from_registry_holder(&next.registries);
                        pump!(send_event(
                            event_tx,
                            NetworkEvent::Registries(next.registries.clone())
                        ))?;
                        pump!(send_event(
                            event_tx,
                            NetworkEvent::BiomeColors {
                                colors: extract_biome_climate(&next.registries),
                            }
                        ))?;
                    }
                    pump!(send_event(
                        event_tx,
                        NetworkEvent::TimelineTags(next.timeline_tags.clone())
                    ))?;
                    if !std::sync::Arc::ptr_eq(&next.dialogs, &configured.dialogs) {
                        pump!(send_event(
                            event_tx,
                            NetworkEvent::DialogRegistry(next.dialogs.clone())
                        ))?;
                    }
                    pump!(send_event(
                        event_tx,
                        NetworkEvent::LoomPatterns(next.loom_patterns.clone())
                    ))?;
                    configured = next;
                    continue;
                }
                if let Some(t) = translation
                    && !t.remap_inbound(&mut packet)
                {
                    continue;
                }
                if let ClientboundGamePacket::UpdateTags(p) = &packet {
                    let timeline_tags =
                        resolve_timeline_tags(&p.tags, configured.timeline_ids.as_deref())?;
                    pump!(send_event(
                        event_tx,
                        NetworkEvent::TimelineTags(timeline_tags)
                    ))?;
                }
                if let ClientboundGamePacket::UpdateTags(p) = &packet
                    && let Some(tags) = dialog_tags(&p.tags)
                {
                    configured.dialogs = std::sync::Arc::new(configured.dialogs.with_tags(tags));
                    pump!(send_event(
                        event_tx,
                        NetworkEvent::DialogRegistry(configured.dialogs.clone())
                    ))?;
                }
                if let ClientboundGamePacket::Login(login) = &mut packet {
                    inbound_chat.reset();
                    if crate::version::session_protocol() < 776 {
                        login.online_mode = conn.is_encrypted();
                    }
                }
                if let ClientboundGamePacket::Transfer(p) = &packet {
                    pump!(send_event(
                        event_tx,
                        NetworkEvent::ServerTransfer(super::ServerTransfer {
                            host: p.host.clone(),
                            port: p.port,
                            cookies: server_cookies.clone(),
                        })
                    ))?;
                    return Err(ConnectionError::Transferred);
                }
                let handler = handle_game_packet_with_display_text(
                    &packet,
                    &sender,
                    event_tx,
                    &configured.registries,
                    &shared_tree,
                    &mut batch_size_calculator,
                    &mut current_dimension,
                    &mut server_cookies,
                    &mut display_text,
                );
                pump!(handler)?;
                if matches!(packet, ClientboundGamePacket::Disconnect(_)) {
                    return Ok(());
                }
            }
            Err(e) => skip_malformed_packet(e)?,
        }
    }
}

/// Poll the ordered inbound work without starving already-queued outbound
/// traffic. ponytail: inbound stays serialized; an unread socket keepalive has
/// no deadline guarantee while the UI stalls. Use a bounded ordered reader if
/// that matters.
async fn pump_game_future<F: std::future::Future>(
    future: F,
    writer: &mut RawWriter,
    outbound_rx: &mut mpsc::UnboundedReceiver<Outbound>,
    key_pair_rx: &mut mpsc::UnboundedReceiver<Option<std::sync::Arc<ProfileKeyPair>>>,
    chat: &mut ChatSender,
    chat_tick: &mut tokio::time::Interval,
    translation: Option<&super::translate::Translation>,
    tree: &crate::net::commands::SharedCommandTree,
    recorder: &crate::movement_record::Recorder,
) -> Result<F::Output, ConnectionError> {
    tokio::pin!(future);
    loop {
        tokio::select! {
            result = &mut future => return Ok(result),
            Some(out) = outbound_rx.recv() => {
                if let Some((frame, trace)) = recorded_outbound_frame(out, translation, chat, tree, recorder)? {
                    write_game_frame_traced(writer, translation, frame, recorder, trace).await?;
                }
            }
            Some(key_pair) = key_pair_rx.recv() => {
                if let Some(frame) = chat.key_pair_ready(key_pair) {
                    write_game_frame(writer, translation, frame, recorder).await?;
                }
            }
            _ = chat_tick.tick() => chat.tick(),
        }
    }
}

fn recorded_outbound_frame(
    out: Outbound,
    translation: Option<&super::translate::Translation>,
    chat: &mut ChatSender,
    tree: &crate::net::commands::SharedCommandTree,
    recorder: &crate::movement_record::Recorder,
) -> Result<Option<(Vec<u8>, Option<crate::movement_record::PacketTraceId>)>, ConnectionError> {
    let (out, trace) = match out {
        Outbound::Traced { trace, packet } => (*packet, Some(trace)),
        packet => (packet, None),
    };
    let observation = if recorder.active() {
        match &out {
            Outbound::Packet(p) => crate::movement_record::outbound(p),
            Outbound::Raw(frame) => crate::movement_record::outbound_frame(frame),
            _ => None,
        }
    } else {
        None
    };
    if let Some(trace) = trace {
        recorder.packet_stage(trace, "dequeued", None);
    }
    let result = outbound_frame(out, translation, chat, tree);
    match &result {
        Err(_) => {
            if let Some(trace) = trace {
                recorder.packet_stage(trace, "encode_failed", Some("encoder_error"));
            } else {
                recorder.record("outbound", "encode_failed", || observation);
            }
        }
        Ok(None) => {
            if let Some(trace) = trace {
                recorder.packet_stage(
                    trace,
                    "outbound_skipped",
                    Some("no_frame_for_current_phase"),
                );
            }
        }
        _ => {}
    }
    result.map(|frame| frame.map(|frame| (frame, trace)))
}

/// The frame one queued outbound item writes, if any.
fn outbound_frame(
    out: Outbound,
    translation: Option<&super::translate::Translation>,
    chat: &mut ChatSender,
    tree: &crate::net::commands::SharedCommandTree,
) -> Result<Option<Vec<u8>>, ConnectionError> {
    Ok(match out {
        Outbound::Packet(mut packet) => match translation {
            Some(t) => {
                t.remap_outbound(&mut packet);
                match &*packet {
                    ServerboundGamePacket::SetCreativeModeSlot(p)
                        if t.creative_slot_delimited() =>
                    {
                        Some(super::native_codecs::encode_native_creative_slot(p)?)
                    }
                    _ => Some(serialize_frame(&*packet)?),
                }
            }
            None if crate::version::session_protocol()
                == pomme_protocol::version::NATIVE.protocol =>
            {
                match &*packet {
                    ServerboundGamePacket::SetCreativeModeSlot(p) => {
                        Some(super::native_codecs::encode_native_creative_slot(p)?)
                    }
                    _ => Some(serialize_frame(&*packet)?),
                }
            }
            None => Some(serialize_frame(&*packet)?),
        },
        Outbound::Raw(bytes) => Some(bytes),
        Outbound::ChatInput(input) => match chat.encode_input(&input, tree.lock().as_deref()) {
            Ok(frame) => Some(frame),
            Err(error) => {
                tracing::warn!("Not sending invalid chat input: {error}");
                None
            }
        },
        Outbound::ChatLogin { online_mode } => {
            chat.login(online_mode);
            None
        }
        Outbound::ChatMark(mark) => chat.mark(*mark),
        Outbound::CustomClick { id, payload } => {
            custom_click_frame(Phase::Game, &id, payload.as_ref())
        }
        Outbound::CodeOfConductDecision(_) => None,
        Outbound::Traced { .. } => unreachable!("trace envelope is unwrapped before encoding"),
    })
}

/// A custom click action for `phase`, unless the wire version predates the
/// packet (the id translation passes appended packets through unmapped) or
/// the action doesn't encode.
fn custom_click_frame(
    phase: Phase,
    id: &str,
    payload: Option<&simdnbt::owned::NbtTag>,
) -> Option<Vec<u8>> {
    let wire = PacketTable::for_protocol(crate::version::session_protocol());
    if wire.is_none_or(|t| {
        t.id(phase, Direction::Serverbound, "custom_click_action")
            .is_none()
    }) {
        tracing::debug!("Not sending custom click action {id:?}: the wire version lacks it");
        return None;
    }
    super::chat::encode_outbound_custom_click_action(phase, id, payload)
        .map_err(|e| tracing::warn!("Could not encode custom click action {id:?}: {e}"))
        .ok()
}

fn serialize_frame<P: azalea_protocol::packets::ProtocolPacket + std::fmt::Debug>(
    packet: &P,
) -> Result<Vec<u8>, ConnectionError> {
    azalea_protocol::write::serialize_packet(packet)
        .map(Vec::from)
        .map_err(|e| ConnectionError::Write(std::io::Error::other(e)))
}

/// Writes one native-layout frame, translating it for other wire versions.
async fn write_game_frame(
    writer: &mut RawWriter,
    translation: Option<&super::translate::Translation>,
    frame: Vec<u8>,
    recorder: &crate::movement_record::Recorder,
) -> Result<(), ConnectionError> {
    write_game_frame_traced(writer, translation, frame, recorder, None).await
}

async fn write_game_frame_traced(
    writer: &mut RawWriter,
    translation: Option<&super::translate::Translation>,
    frame: Vec<u8>,
    recorder: &crate::movement_record::Recorder,
    trace: Option<crate::movement_record::PacketTraceId>,
) -> Result<(), ConnectionError> {
    let observation = if recorder.active() {
        crate::movement_record::outbound_frame(&frame)
    } else {
        None
    };
    let frames = match translation {
        Some(t) if t.translates_outbound() => t.translate_outbound_game_frame(frame),
        _ => vec![frame],
    };
    if frames.is_empty() {
        if let Some(trace) = trace {
            recorder.packet_stage(
                trace,
                "translation_suppressed",
                Some("no_translated_frames"),
            );
        } else {
            recorder.record("outbound", "translation_suppressed", || observation.clone());
        }
    }
    for frame in frames {
        recorder.raw_packet_traced("outbound", "play", &frame, trace);
        let result = writer.write(&frame).await;
        if let Some(trace) = trace {
            recorder.packet_stage(
                trace,
                if result.is_ok() {
                    "transport_write_success"
                } else {
                    "transport_write_failed"
                },
                result.as_ref().err().map(|_| "io_error"),
            );
        } else {
            recorder.record(
                "outbound",
                if result.is_ok() {
                    "transport_write_success"
                } else {
                    "transport_write_failed"
                },
                || {
                    observation.clone().map(|mut data| {
                        use azalea_buf::AzBufVar;
                        data["wire_id"] = serde_json::json!(
                            u32::azalea_read_var(&mut std::io::Cursor::new(frame.as_slice())).ok()
                        );
                        data["error_kind"] = serde_json::json!(
                            result.as_ref().err().map(|e| format!("{:?}", e.kind()))
                        );
                        data
                    })
                },
            );
        }
        result?;
    }
    Ok(())
}

/// Writes one native-layout configuration packet, translating it for other
/// wire versions (765 down: id remap plus suppression of packets the wire
/// version lacks).
async fn write_config_packet(
    conn: &mut Conn,
    packet: ServerboundConfigPacket,
) -> Result<(), ConnectionError> {
    write_config_frame(conn, serialize_frame(&packet)?).await
}

/// [`write_config_packet`] for an already-encoded native-layout frame.
async fn write_config_frame(conn: &mut Conn, frame: Vec<u8>) -> Result<(), ConnectionError> {
    let frame = match super::translate::active().filter(|t| t.translates_config()) {
        Some(t) => match t.translate_outbound_config_frame(frame) {
            Some(frame) => frame,
            None => {
                if let Some(recorder) = &conn.recorder {
                    if let Some(trace) = conn.pending_trace.take() {
                        recorder.packet_stage(
                            trace,
                            "translation_suppressed",
                            Some("configuration_packet_unsupported"),
                        );
                    } else {
                        recorder.record("outbound", "translation_suppressed", || Some(serde_json::json!({"protocol_state":"configuration","reason":"configuration_packet_unsupported"})));
                    }
                }
                return Ok(());
            }
        },
        None => frame,
    };
    // LOGIN frames never pass through this helper; this remains CONFIG-only.
    if let Some(recorder) = &conn.recorder {
        let trace = conn.pending_trace.take();
        recorder.raw_packet_traced("outbound", "configuration", &frame, trace);
        let result = conn.writer.write(&frame).await;
        if let Some(trace) = trace {
            recorder.packet_stage(
                trace,
                if result.is_ok() {
                    "transport_write_success"
                } else {
                    "transport_write_failed"
                },
                result.as_ref().err().map(|_| "io_error"),
            );
        }
        return Ok(result?);
    }
    Ok(conn.writer.write(&frame).await?)
}

/// Recoverable decode errors skip the packet; anything else tears down the
/// connection.
fn skip_malformed_packet(err: Box<ReadPacketError>) -> Result<(), ConnectionError> {
    match &*err {
        ReadPacketError::Parse { .. }
        | ReadPacketError::UnknownPacketId { .. }
        | ReadPacketError::LeftoverData { .. } => {
            tracing::warn!("Skipping malformed packet: {err}");
            Ok(())
        }
        _ => Err(err.into()),
    }
}

fn friendly_error_reason(err: &ConnectionError) -> String {
    let msg = err.to_string();
    if msg.contains("connection refused") || msg.contains("Connection refused") {
        "Connection refused".to_string()
    } else if msg.contains("Connection closed")
        || msg.contains("connection reset")
        || msg.contains("broken pipe")
    {
        "Server closed".to_string()
    } else if msg.contains("timed out") || msg.contains("Timed out") {
        "Connection timed out".to_string()
    } else if msg.contains("no addresses found") || msg.contains("failed to lookup") {
        "Unknown host".to_string()
    } else {
        msg
    }
}

#[cfg(test)]
mod tests {
    use pomme_protocol::version::NATIVE;

    use super::*;

    #[test]
    fn timeline_wire_ids_and_numeric_tags_preserve_protocol_order() {
        let entries = vec![
            ("minecraft:day".into(), Some(())),
            ("minecraft:moon".into(), None),
            ("minecraft:early_game".into(), Some(())),
        ];
        let ids = timeline_ids_from_entries(&entries).unwrap();
        assert_eq!(
            ids,
            [
                "minecraft:day".into(),
                "minecraft:moon".into(),
                "minecraft:early_game".into()
            ]
        );
        let tags = vec![("custom:cycle".into(), vec![2, 0, 1])];
        let resolved = resolve_timeline_tag_entries(&tags, Some(&ids)).unwrap();
        assert_eq!(
            resolved[&"custom:cycle".into()],
            [ids[2].clone(), ids[0].clone(), ids[1].clone()]
        );
        let replaced =
            resolve_timeline_tag_entries(&[("custom:cycle".into(), vec![1])], Some(&ids)).unwrap();
        assert_eq!(replaced[&"custom:cycle".into()], [ids[1].clone()]);
    }

    #[test]
    fn timeline_ids_and_tags_reject_invalid_wire_references() {
        assert!(
            validate_timeline_ids(vec!["minecraft:day".into(), "minecraft:day".into()]).is_err()
        );
        let ids = vec!["minecraft:day".into()];
        assert!(
            resolve_timeline_tag_entries(&[("custom:tag".into(), vec![-1])], Some(&ids)).is_err()
        );
        assert!(
            resolve_timeline_tag_entries(&[("custom:tag".into(), vec![1])], Some(&ids)).is_err()
        );
        assert!(resolve_timeline_tag_entries(&[("custom:tag".into(), vec![0])], None).is_err());
    }

    #[test]
    fn configuration_cookie_response_records_the_exact_written_frame() {
        use azalea_protocol::packets::config::s_cookie_response::ServerboundCookieResponse;
        use base64::Engine;
        let rt = tokio::runtime::Runtime::new().unwrap();
        let dir = crate::test_util::test_temp_dir("config-cookie-raw");
        std::fs::create_dir_all(&dir).unwrap();
        let recorder = std::sync::Arc::new(crate::movement_record::Recorder::default());
        recorder.start(&rt, &dir);
        rt.block_on(async {
            let (client, server) = super::super::conn::memory_pipes();
            let mut conn = Conn::from_memory(client);
            conn.recorder = Some(recorder.clone());
            let mut peer = Conn::from_memory(server);
            write_config_packet(
                &mut conn,
                ServerboundConfigPacket::CookieResponse(ServerboundCookieResponse {
                    key: "minecraft:private_diagnostic_cookie".into(),
                    payload: Some(vec![0, 0xff, 7, 8]),
                }),
            )
            .await
            .unwrap();
            let wire_frame = peer.reader.read().await.unwrap();
            recorder.stop("test");
            let started = std::time::Instant::now();
            while recorder.status().starts_with("Draining") {
                assert!(started.elapsed().as_secs() < 5);
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            let path = std::fs::read_dir(&dir)
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .path();
            let rows: Vec<serde_json::Value> = std::fs::read_to_string(path)
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect();
            let raw = rows
                .iter()
                .find(|row| row["stage"] == "packet_raw" && row["direction"] == "outbound")
                .unwrap();
            assert_eq!(
                base64::engine::general_purpose::STANDARD
                    .decode(raw["data"]["payload_base64"].as_str().unwrap())
                    .unwrap()
                    .as_slice(),
                wire_frame.as_ref()
            );
            assert_eq!(raw["data"]["protocol_state"], "configuration");
            assert_eq!(raw["data"]["packet_type"], "cookie_response");
            assert_eq!(raw["data"]["connection_epoch"], rows[0]["connection_epoch"]);
        });
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn movement_capture_uses_real_queue_and_shared_transport_dispatcher() {
        use azalea_protocol::common::movements::{PositionMoveRotation, RelativeMovements};
        use azalea_protocol::packets::game::c_player_position::ClientboundPlayerPosition;
        use azalea_protocol::packets::game::s_player_input::ServerboundPlayerInput;
        let rt = tokio::runtime::Runtime::new().unwrap();
        let dir = crate::test_util::test_temp_dir("movement-dispatch");
        std::fs::create_dir_all(&dir).unwrap();
        let recorder = std::sync::Arc::new(crate::movement_record::Recorder::default());
        recorder.start(&rt, &dir);
        rt.block_on(async {
            let (tx, mut rx) = mpsc::unbounded_channel();
            let sender = PacketSender::with_recorder(tx, recorder.clone());
            let packet = ServerboundGamePacket::PlayerInput(ServerboundPlayerInput {
                forward: true,
                ..Default::default()
            });
            sender.send(packet);
            let (client, peer) = super::super::conn::memory_pipes();
            let mut conn = Conn::from_memory(client);
            let mut chat = ChatSender::new(
                uuid::Uuid::nil(),
                uuid::Uuid::nil(),
                None,
                mpsc::unbounded_channel().0,
            );
            let tree = Default::default();
            let frame = recorded_outbound_frame(
                rx.recv().await.unwrap(),
                None,
                &mut chat,
                &tree,
                &recorder,
            )
            .unwrap()
            .unwrap();
            let (frame, trace) = frame;
            assert!(crate::movement_record::outbound_frame(&frame).is_some());
            write_game_frame_traced(&mut conn.writer, None, frame.clone(), &recorder, trace)
                .await
                .unwrap();
            drop(peer);
            let (mut failed_end, failed_peer) = super::super::conn::memory_pipes();
            tokio::io::AsyncWriteExt::shutdown(&mut failed_end.tx)
                .await
                .unwrap();
            let mut failed_conn = Conn::from_memory(failed_end);
            let failed =
                write_game_frame_traced(&mut failed_conn.writer, None, frame, &recorder, trace)
                    .await;
            drop(failed_peer);
            if failed.is_ok() {
                recorder.stop("test_failure");
            }
            assert!(failed.is_err());
            recorder.inbound(
                &ClientboundGamePacket::PlayerPosition(ClientboundPlayerPosition {
                    id: 3,
                    change: PositionMoveRotation {
                        pos: azalea_core::position::Vec3::new(0.0, 64.0, 0.0),
                        delta: azalea_core::position::Vec3::ZERO,
                        look_direction: azalea_entity::LookDirection::default(),
                    },
                    relative: RelativeMovements::default(),
                }),
                Some(1),
            );
            recorder.record("local", "applied", || {
                Some(serde_json::json!({"teleport_id":3,"before":[1,64,0],"after":[0,64,0]}))
            });
            recorder.stop("disconnect");
        });
        let start = std::time::Instant::now();
        while recorder.status().starts_with("Draining") {
            assert!(start.elapsed().as_secs() < 5);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(recorder.status().starts_with("Ended"));
        let path = std::fs::read_dir(&dir)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let text = std::fs::read_to_string(path).unwrap();
        let rows: Vec<serde_json::Value> = text
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect();
        let stages: Vec<_> = rows.iter().filter_map(|v| v["stage"].as_str()).collect();
        assert_eq!(
            stages,
            [
                "queue_attempt",
                "queued",
                "dequeued",
                "packet_raw",
                "transport_write_success",
                "packet_raw",
                "transport_write_failed",
                "received",
                "applied"
            ]
        );
        let trace_ids: Vec<_> = rows
            .iter()
            .filter_map(|row| row["data"]["packet_trace_id"].as_u64())
            .collect();
        assert!(trace_ids.len() >= 7);
        assert!(trace_ids.iter().all(|id| *id == trace_ids[0]));
        assert!(rows[0]["connection_epoch"].is_u64());
        assert_eq!(rows.last().unwrap()["reason"], "disconnect");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn full_world_queue_waits_and_pumps_outbound_without_partial_chunk() {
        use std::sync::Arc;
        use std::time::Duration;

        use azalea_buf::AzBuf;
        use azalea_protocol::packets::game::c_level_chunk_with_light::{
            BlockEntity, ClientboundLevelChunkPacketData, ClientboundLevelChunkWithLight,
        };
        use uuid::Uuid;

        let (client_end, server_end) = super::super::conn::memory_pipes();
        let mut peer = Conn::from_memory(server_end);
        let (event_tx, event_rx) = crossbeam_channel::bounded(1);
        event_tx
            .try_send(NetworkEvent::LevelChunksLoadStart)
            .unwrap();
        let mut sections = Vec::new();
        for _ in 0..24 {
            azalea_world::chunk::Section::default()
                .azalea_write(&mut sections)
                .unwrap();
        }
        peer.write_packet(ClientboundLevelChunkWithLight {
            x: 0,
            z: 0,
            chunk_data: ClientboundLevelChunkPacketData {
                heightmaps: Vec::new(),
                data: Arc::new(sections.into_boxed_slice()),
                block_entities: vec![BlockEntity {
                    packed_xz: 0,
                    y: 0,
                    kind: azalea_registry::builtin::BlockEntityKind::CopperGolemStatue,
                    data: simdnbt::owned::Nbt::None,
                }],
            },
            light_data: Default::default(),
        })
        .await
        .unwrap();
        let (outbound_tx, outbound_rx) = mpsc::unbounded_channel();
        let (key_tx, key_pair_rx) = mpsc::unbounded_channel();
        let outbound = outbound_tx.clone();
        let fill = event_tx.clone();
        let client = tokio::spawn(async move {
            game_loop(
                Conn::from_memory(client_end),
                &event_tx,
                GameLoopArgs {
                    connected_profile: None,
                    recorder: Default::default(),
                    outbound_tx,
                    outbound_rx,
                    joined: Joined {
                        configured: Configured {
                            registries: Arc::default(),
                            timeline_ids: None,
                            timeline_tags: Default::default(),
                            dialogs: Arc::default(),
                            loom_patterns: Arc::default(),
                        },
                        deferred_login: None,
                    },
                    view_distance: 8,
                    chat_options: Default::default(),
                    main_hand_right: true,
                    particle_mode: crate::particle::ParticleMode::All,
                    skin_parts_mask: 127,
                    chat: ChatSender::new(Uuid::nil(), Uuid::nil(), None, key_tx),
                    key_pair_rx,
                    server_cookies: Default::default(),
                },
            )
            .await
        });
        outbound
            .send(Outbound::Packet(Box::new(
                ServerboundGamePacket::KeepAlive(
                    azalea_protocol::packets::game::s_keep_alive::ServerboundKeepAlive { id: 123 },
                ),
            )))
            .unwrap();
        assert!(
            matches!(read_test_packet::<ServerboundGamePacket>(&mut peer).await,
            ServerboundGamePacket::KeepAlive(p) if p.id == 123)
        );
        assert!(!client.is_finished());
        assert_eq!(event_rx.len(), 1);
        assert!(matches!(
            recv_event(&event_rx).await,
            NetworkEvent::LevelChunksLoadStart
        ));
        assert!(matches!(
            recv_event(&event_rx).await,
            NetworkEvent::Registries(_)
        ));
        assert!(matches!(
            recv_event(&event_rx).await,
            NetworkEvent::DialogRegistry(_)
        ));
        let NetworkEvent::ChunkLoaded {
            chunk,
            block_entities,
            ..
        } = recv_event(&event_rx).await
        else {
            panic!("expected one atomic chunk snapshot");
        };
        assert_eq!(chunk.sections.len(), 24);
        assert_eq!(block_entities.len(), 1);
        assert!(block_entities[0].2.is_empty());
        assert!(event_rx.is_empty());
        // Both raw chat and raw dialog delivery use the same live writer pump.
        for name in ["system_chat", "server_links"] {
            fill.try_send(NetworkEvent::LevelChunksLoadStart).unwrap();
            let mut raw = Vec::new();
            pomme_protocol::wire::write_varint(
                &mut raw,
                PacketTable::native()
                    .id(Phase::Game, Direction::Clientbound, name)
                    .unwrap(),
            );
            if name == "system_chat" {
                simdnbt::owned::NbtTag::String("waiting".into()).write(&mut raw);
                raw.push(0);
            } else {
                pomme_protocol::wire::write_varint(&mut raw, 0);
            }
            peer.writer.write(&raw).await.unwrap();
            tokio::time::sleep(Duration::from_millis(10)).await;
            outbound
                .send(Outbound::Raw(super::super::chat::encode_chat_ack(37)))
                .unwrap();
            let frame = tokio::time::timeout(Duration::from_secs(2), peer.reader.read())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(
                frame.as_ref(),
                super::super::chat::encode_chat_ack(37).as_slice()
            );
            assert!(!client.is_finished());
            assert_eq!(event_rx.len(), 1);
            assert!(matches!(
                recv_event(&event_rx).await,
                NetworkEvent::LevelChunksLoadStart
            ));
            match (name, recv_event(&event_rx).await) {
                ("system_chat", NetworkEvent::ChatMessage { .. })
                | ("server_links", NetworkEvent::ServerLinks { .. }) => {}
                _ => panic!("raw delivery missing or reordered"),
            }
            assert!(event_rx.is_empty());
        }
        drop(event_rx);
        peer.write_packet(
            azalea_protocol::packets::game::c_set_held_slot::ClientboundSetHeldSlot { slot: 2 },
        )
        .await
        .unwrap();
        assert!(matches!(
            tokio::time::timeout(Duration::from_secs(2), client)
                .await
                .unwrap()
                .unwrap(),
            Err(ConnectionError::EventReceiverGone)
        ));
    }

    #[tokio::test]
    async fn terminal_event_yields_until_queue_space_is_available() {
        let (tx, rx) = crossbeam_channel::bounded(1);
        tx.try_send(NetworkEvent::LevelChunksLoadStart).unwrap();
        let terminal = tokio::spawn(async move {
            send_event(
                &tx,
                NetworkEvent::Disconnected {
                    reason: "full".into(),
                },
            )
            .await
            .unwrap();
        });
        tokio::task::yield_now().await;
        assert!(!terminal.is_finished());
        assert!(matches!(
            rx.try_recv().unwrap(),
            NetworkEvent::LevelChunksLoadStart
        ));
        tokio::time::timeout(std::time::Duration::from_secs(2), terminal)
            .await
            .unwrap()
            .unwrap();
        assert!(
            matches!(rx.try_recv().unwrap(), NetworkEvent::Disconnected { reason } if reason == "full")
        );
    }

    #[tokio::test]
    async fn pending_delivery_is_fifo_exactly_once_and_cancelled_inline() {
        let (tx, rx) = crossbeam_channel::bounded(1);
        tx.try_send(NetworkEvent::LevelChunksLoadStart).unwrap();
        let send = async {
            send_event(&tx, NetworkEvent::HeldSlot { slot: 1 })
                .await
                .unwrap();
            send_event(&tx, NetworkEvent::HeldSlot { slot: 2 })
                .await
                .unwrap();
        };
        tokio::pin!(send);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(10), &mut send)
                .await
                .is_err()
        );
        assert!(matches!(
            rx.try_recv().unwrap(),
            NetworkEvent::LevelChunksLoadStart
        ));
        tokio::join!(send, async {
            assert!(matches!(
                recv_event(&rx).await,
                NetworkEvent::HeldSlot { slot: 1 }
            ));
            assert!(matches!(
                recv_event(&rx).await,
                NetworkEvent::HeldSlot { slot: 2 }
            ));
        });
        assert!(rx.is_empty());
        tx.try_send(NetworkEvent::LevelChunksLoadStart).unwrap();
        let pending_tx = tx.clone();
        let pending =
            tokio::spawn(async move { send_event(&pending_tx, NetworkEvent::ClearDialog).await });
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        assert!(!pending.is_finished());
        pending.abort();
        assert!(pending.await.unwrap_err().is_cancelled());
        rx.try_recv().unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        assert!(
            rx.is_empty(),
            "cancelled delivery must not leave a detached worker"
        );
        drop(rx);
        assert!(matches!(
            send_event(&tx, NetworkEvent::ClearDialog).await,
            Err(crossbeam_channel::SendError(NetworkEvent::ClearDialog))
        ));
    }

    #[tokio::test]
    async fn full_config_delivery_pumps_pack_status_conduct_and_custom_click() {
        use azalea_protocol::packets::config::c_code_of_conduct::ClientboundCodeOfConduct;
        use azalea_protocol::packets::config::c_finish_configuration::ClientboundFinishConfiguration;
        use azalea_protocol::packets::game::s_resource_pack::{Action, ServerboundResourcePack};
        let (client_end, peer_end) = super::super::conn::memory_pipes();
        let mut peer = Conn::from_memory(peer_end);
        let (tx, rx) = crossbeam_channel::bounded(1);
        tx.try_send(NetworkEvent::LevelChunksLoadStart).unwrap();
        let (out_tx, mut out_rx) = mpsc::unbounded_channel();
        let client = tokio::spawn(async move {
            config_sequence(
                &mut Conn::from_memory(client_end),
                8,
                Default::default(),
                true,
                crate::particle::ParticleMode::All,
                127,
                &crate::movement_record::Recorder::default(),
                &tx,
                &mut out_rx,
                &mut Default::default(),
                None,
            )
            .await
        });
        let _: ServerboundConfigPacket = read_test_packet(&mut peer).await;
        let _: ServerboundConfigPacket = read_test_packet(&mut peer).await;
        peer.write_packet(ClientboundCodeOfConduct {
            code_of_conduct: "Rules".into(),
        })
        .await
        .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        assert!(!client.is_finished());
        let id = uuid::Uuid::nil();
        out_tx
            .send(Outbound::Packet(Box::new(
                ServerboundGamePacket::ResourcePack(ServerboundResourcePack {
                    id,
                    action: Action::SuccessfullyLoaded,
                }),
            )))
            .unwrap();
        assert!(
            matches!(read_test_packet::<ServerboundConfigPacket>(&mut peer).await, ServerboundConfigPacket::ResourcePack(p) if p.id == id && matches!(p.action, azalea_protocol::packets::config::s_resource_pack::Action::SuccessfullyLoaded))
        );
        out_tx
            .send(Outbound::CustomClick {
                id: "test:click".into(),
                payload: None,
            })
            .unwrap();
        let raw = tokio::time::timeout(std::time::Duration::from_secs(2), peer.reader.read())
            .await
            .unwrap()
            .unwrap();
        let mut pos = 0;
        assert_eq!(
            pomme_protocol::wire::read_varint(&raw, &mut pos),
            PacketTable::native().id(
                Phase::Configuration,
                Direction::Serverbound,
                "custom_click_action"
            )
        );
        out_tx.send(Outbound::CodeOfConductDecision(true)).unwrap();
        assert!(matches!(
            read_test_packet::<ServerboundConfigPacket>(&mut peer).await,
            ServerboundConfigPacket::AcceptCodeOfConduct(_)
        ));
        assert_eq!(rx.len(), 1);
        assert!(matches!(
            recv_event(&rx).await,
            NetworkEvent::LevelChunksLoadStart
        ));
        assert!(
            matches!(recv_event(&rx).await, NetworkEvent::CodeOfConduct { text } if text == "Rules")
        );
        peer.write_packet(ClientboundFinishConfiguration)
            .await
            .unwrap();
        assert!(matches!(
            read_test_packet::<ServerboundConfigPacket>(&mut peer).await,
            ServerboundConfigPacket::FinishConfiguration(_)
        ));
        tokio::time::timeout(std::time::Duration::from_secs(2), client)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(rx.is_empty());
    }

    #[test]
    fn translated_creative_slot_uses_delimited_components_since_1_21_5() {
        use azalea_inventory::components::Damage;
        use azalea_inventory::{DataComponentPatch, ItemStack, ItemStackData};
        use azalea_protocol::packets::game::s_set_creative_mode_slot::ServerboundSetCreativeModeSlot;
        use azalea_registry::builtin::{DataComponentKind, ItemKind};
        use uuid::Uuid;

        let mut patch = DataComponentPatch::default();
        unsafe {
            patch.unchecked_insert_component(
                DataComponentKind::Damage,
                Some(Damage { amount: 7 }.into()),
            );
        }
        let mut stack = ItemStackData::new(ItemKind::Stone, 1);
        stack.component_patch = patch;
        let packet = ServerboundGamePacket::SetCreativeModeSlot(ServerboundSetCreativeModeSlot {
            slot_num: 36,
            item_stack: ItemStack::Present(stack),
        });
        let translation = super::super::translate::Translation::for_protocol(770).unwrap();

        let mut remapped = packet.clone();
        translation.remap_outbound(&mut remapped);
        let ServerboundGamePacket::SetCreativeModeSlot(expected_packet) = &remapped else {
            unreachable!();
        };
        let expected_native =
            super::super::native_codecs::encode_native_creative_slot(expected_packet).unwrap();
        let mut expected_native_pos = 0;
        let native_id =
            pomme_protocol::wire::read_varint(&expected_native, &mut expected_native_pos).unwrap();
        let wire_id = pomme_protocol::PacketTable::for_protocol(770)
            .unwrap()
            .id(
                Phase::Game,
                Direction::Serverbound,
                "set_creative_mode_slot",
            )
            .unwrap();
        let mut expected = Vec::new();
        pomme_protocol::wire::write_varint(&mut expected, wire_id);
        expected.extend_from_slice(&expected_native[expected_native_pos..]);
        assert_ne!(native_id, wire_id);
        assert!(expected.ends_with(&[1, 7])); // value length=1, damage=7

        let (key_tx, _key_rx) = mpsc::unbounded_channel();
        let mut chat = ChatSender::new(Uuid::nil(), Uuid::nil(), None, key_tx);
        let tree = crate::net::commands::SharedCommandTree::default();
        let frame = outbound_frame(
            Outbound::Packet(Box::new(packet)),
            Some(&translation),
            &mut chat,
            &tree,
        )
        .unwrap()
        .unwrap();
        assert_eq!(translation.translate_outbound_game_frame(frame), [expected]);
    }

    /// A server that accepts pomme's known-pack claim sends the biomes as ids
    /// alone; the climate the mesher colours with then comes entirely from the
    /// embedded elements.
    #[test]
    fn filled_biome_entries_carry_their_climate() {
        use azalea_registry::identifier::Identifier;

        let holder = crate::net::known_packs::filled_holder("worldgen/biome");
        let plains_id = holder.extra[&Identifier::new("minecraft:worldgen/biome")]
            .map
            .get_index_of(&Identifier::new("minecraft:plains"))
            .expect("plains biome") as u32;

        let plains = &extract_biome_climate(&holder)[&plains_id];
        assert_eq!(plains.temperature, 0.8);
        assert_eq!(plains.downfall, 0.4);
    }

    /// 762 (1.19.4) is not a supported version at all, so it never gains a
    /// wire translation; 775 has one.
    #[test]
    fn resolve_wire_gates_unjoinable_versions() {
        let native = NATIVE.protocol;
        // Launched as the native version.
        assert_eq!(resolve_wire(Some(775), native), Ok(775));
        assert_eq!(resolve_wire(Some(762), native), Ok(native));
        assert_eq!(resolve_wire(None, native), Ok(native));
        // Launched as the newest listed version, which need not be native.
        let latest = pomme_protocol::version::LATEST.protocol;
        assert_eq!(resolve_wire(Some(native), latest), Ok(native));
        if crate::net::translate::joinable(latest) {
            assert_eq!(resolve_wire(None, latest), Ok(latest));
        }
        // An untranslated launched version is refused whatever the probe
        // yielded, unless the server itself speaks a joinable protocol.
        assert_eq!(resolve_wire(None, 762), Err(762));
        assert_eq!(resolve_wire(Some(762), 762), Err(762));
        assert_eq!(resolve_wire(Some(native), 762), Ok(native));
        // A staged version (tables embedded, not yet in TRANSLATED) is
        // refused when launched and adopted around when the server is
        // joinable.
        for v in pomme_protocol::version::VERSIONS {
            if pomme_protocol::PacketTable::for_protocol(v.protocol).is_some()
                && !crate::net::translate::joinable(v.protocol)
            {
                assert_eq!(
                    resolve_wire(Some(v.protocol), native),
                    Ok(native),
                    "{}",
                    v.name
                );
                assert_eq!(
                    resolve_wire(None, v.protocol),
                    Err(v.protocol),
                    "{}",
                    v.name
                );
            }
        }
    }

    async fn recv_event(receiver: &crossbeam_channel::Receiver<NetworkEvent>) -> NetworkEvent {
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                match receiver.try_recv() {
                    Ok(event) => return event,
                    Err(crossbeam_channel::TryRecvError::Disconnected) => {
                        panic!("event receiver closed")
                    }
                    Err(crossbeam_channel::TryRecvError::Empty) => {
                        tokio::time::sleep(std::time::Duration::from_millis(1)).await
                    }
                }
            }
        })
        .await
        .expect("timed out waiting for network event")
    }

    async fn read_test_packet<P: azalea_protocol::packets::ProtocolPacket + std::fmt::Debug>(
        peer: &mut Conn,
    ) -> P {
        tokio::time::timeout(std::time::Duration::from_secs(2), peer.read_packet())
            .await
            .expect("peer packet read timed out")
            .unwrap()
    }

    #[tokio::test]
    async fn explicit_transfer_intention_is_independent_of_cookies() {
        use azalea_protocol::packets::handshake::ServerboundHandshakePacket;

        use crate::net::conn::memory_pipes;

        for is_transfer in [false, true] {
            let (client_end, server_end) = memory_pipes();
            let mut peer = Conn::from_memory(server_end);
            let (event_tx, _event_rx) = crossbeam_channel::bounded(64);
            let (packet_tx, packet_rx) = mpsc::unbounded_channel();
            let cookies = if is_transfer {
                Default::default()
            } else {
                std::collections::HashMap::from([("minecraft:session".into(), vec![1])])
            };
            let client = tokio::spawn(connect_to_server(
                ConnectArgs {
                    transport: Transport::Memory(client_end),
                    is_transfer,
                    username: "Steve".into(),
                    uuid: uuid::Uuid::nil(),
                    access_token: None,
                    view_distance: 8,
                    chat_options: crate::ui::chat::ChatOptions::default(),
                    main_hand_right: true,
                    particle_mode: crate::particle::ParticleMode::All,
                    skin_parts_mask: 127,
                    server_cookies: cookies,
                },
                event_tx,
                packet_tx,
                packet_rx,
            ));
            let ServerboundHandshakePacket::Intention(packet) = read_test_packet(&mut peer).await;
            assert_eq!(packet.intention as i32, if is_transfer { 3 } else { 2 });
            client.abort();
        }
    }

    #[tokio::test]
    async fn login_custom_query_is_answered_with_same_transaction_id_and_no_payload() {
        use azalea_protocol::packets::handshake::ServerboundHandshakePacket;
        use azalea_protocol::packets::login::c_custom_query::ClientboundCustomQuery;
        use azalea_protocol::packets::login::s_custom_query_answer::ServerboundCustomQueryAnswer;
        use uuid::Uuid;

        use crate::net::conn::memory_pipes;

        let (client_end, server_end) = memory_pipes();
        let mut peer = Conn::from_memory(server_end);
        let (event_tx, _event_rx) = crossbeam_channel::bounded(64);
        let (packet_tx, packet_rx) = mpsc::unbounded_channel();
        let client = tokio::spawn(connect_to_server(
            ConnectArgs {
                transport: Transport::Memory(client_end),
                is_transfer: false,
                username: "Steve".into(),
                uuid: Uuid::nil(),
                access_token: None,
                view_distance: 8,
                chat_options: crate::ui::chat::ChatOptions::default(),
                main_hand_right: true,
                particle_mode: crate::particle::ParticleMode::All,
                skin_parts_mask: 127,
                server_cookies: Default::default(),
            },
            event_tx,
            packet_tx,
            packet_rx,
        ));

        let _: ServerboundHandshakePacket = read_test_packet(&mut peer).await;
        let _: ServerboundLoginPacket = read_test_packet(&mut peer).await;
        peer.write_packet(ClientboundCustomQuery {
            transaction_id: 0x1234,
            identifier: "example:unknown".into(),
            data: azalea_buf::UnsizedByteArray(vec![]),
        })
        .await
        .unwrap();
        let response: ServerboundLoginPacket = read_test_packet(&mut peer).await;
        assert!(matches!(
            response,
            ServerboundLoginPacket::CustomQueryAnswer(ServerboundCustomQueryAnswer {
                transaction_id: 0x1234,
                data: None
            })
        ));
        client.abort();
    }

    async fn code_of_conduct_peer() -> (
        Conn,
        tokio::task::JoinHandle<Result<(), ConnectionError>>,
        crossbeam_channel::Receiver<NetworkEvent>,
        mpsc::UnboundedSender<Outbound>,
    ) {
        use azalea_auth::game_profile::GameProfile;
        use azalea_protocol::packets::handshake::ServerboundHandshakePacket;
        use azalea_protocol::packets::login::c_login_finished::ClientboundLoginFinished;
        use uuid::Uuid;

        use crate::net::conn::memory_pipes;

        let (client_end, server_end) = memory_pipes();
        let mut peer = Conn::from_memory(server_end);
        let (event_tx, event_rx) = crossbeam_channel::bounded(64);
        let (packet_tx, packet_rx) = mpsc::unbounded_channel();
        let client = tokio::spawn(connect_to_server(
            ConnectArgs {
                transport: Transport::Memory(client_end),
                is_transfer: false,
                username: "Steve".into(),
                uuid: Uuid::nil(),
                access_token: None,
                view_distance: 8,
                chat_options: crate::ui::chat::ChatOptions::default(),
                main_hand_right: true,
                particle_mode: crate::particle::ParticleMode::All,
                skin_parts_mask: 127,
                server_cookies: Default::default(),
            },
            event_tx,
            packet_tx.clone(),
            packet_rx,
        ));
        let _: ServerboundHandshakePacket =
            tokio::time::timeout(std::time::Duration::from_secs(2), peer.read_packet())
                .await
                .expect("peer packet read timed out")
                .unwrap();
        let _: ServerboundLoginPacket = read_test_packet(&mut peer).await;
        peer.write_packet(ClientboundLoginFinished {
            game_profile: GameProfile::new(Uuid::nil(), "Steve".into()),
            session_id: Uuid::nil(),
        })
        .await
        .unwrap();
        let _: ServerboundLoginPacket = read_test_packet(&mut peer).await;
        let _: ServerboundConfigPacket = read_test_packet(&mut peer).await;
        let _: ServerboundConfigPacket = read_test_packet(&mut peer).await;
        (peer, client, event_rx, packet_tx)
    }

    /// The whole join over an in-memory pipe, against a peer that sends only
    /// the two frames the client actually requires: login finished, then
    /// finish configuration. No registry data, no compression, no encryption.
    #[tokio::test]
    async fn code_of_conduct_waits_for_explicit_accept_before_finish_configuration() {
        use azalea_auth::game_profile::GameProfile;
        use azalea_protocol::packets::config::c_code_of_conduct::ClientboundCodeOfConduct;
        use azalea_protocol::packets::config::c_finish_configuration::ClientboundFinishConfiguration;
        use azalea_protocol::packets::handshake::ServerboundHandshakePacket;
        use azalea_protocol::packets::login::c_login_finished::ClientboundLoginFinished;
        use uuid::Uuid;

        use crate::net::conn::memory_pipes;

        async fn sent<P: azalea_protocol::packets::ProtocolPacket + std::fmt::Debug>(
            peer: &mut Conn,
        ) -> P {
            read_test_packet(peer).await
        }

        let (client_end, server_end) = memory_pipes();
        let mut peer = Conn::from_memory(server_end);
        let (event_tx, event_rx) = crossbeam_channel::bounded(64);
        let (packet_tx, packet_rx) = mpsc::unbounded_channel();
        let client = tokio::spawn(connect_to_server(
            ConnectArgs {
                transport: Transport::Memory(client_end),
                is_transfer: false,
                username: "Steve".into(),
                uuid: Uuid::nil(),
                access_token: None,
                view_distance: 8,
                chat_options: crate::ui::chat::ChatOptions::default(),
                main_hand_right: true,
                particle_mode: crate::particle::ParticleMode::All,
                skin_parts_mask: 127,
                server_cookies: Default::default(),
            },
            event_tx,
            packet_tx.clone(),
            packet_rx,
        ));
        assert!(matches!(
            sent::<ServerboundHandshakePacket>(&mut peer).await,
            ServerboundHandshakePacket::Intention(_)
        ));
        let _: ServerboundLoginPacket = sent(&mut peer).await;
        peer.write_packet(ClientboundLoginFinished {
            game_profile: GameProfile::new(Uuid::nil(), "Steve".into()),
            session_id: Uuid::nil(),
        })
        .await
        .unwrap();
        let _: ServerboundLoginPacket = sent(&mut peer).await;
        let _: ServerboundConfigPacket = sent(&mut peer).await;
        let _: ServerboundConfigPacket = sent(&mut peer).await;
        peer.write_packet(ClientboundCodeOfConduct {
            code_of_conduct: "Read this".into(),
        })
        .await
        .unwrap();
        peer.write_packet(ClientboundFinishConfiguration)
            .await
            .unwrap();
        assert!(
            matches!(recv_event(&event_rx).await, NetworkEvent::CodeOfConduct { text } if text == "Read this")
        );
        assert!(
            tokio::time::timeout(
                std::time::Duration::from_millis(30),
                peer.read_packet::<ServerboundConfigPacket>()
            )
            .await
            .is_err()
        );

        packet_tx
            .send(Outbound::CodeOfConductDecision(true))
            .unwrap();
        assert!(matches!(
            sent::<ServerboundConfigPacket>(&mut peer).await,
            ServerboundConfigPacket::AcceptCodeOfConduct(_)
        ));
        assert!(matches!(
            sent::<ServerboundConfigPacket>(&mut peer).await,
            ServerboundConfigPacket::FinishConfiguration(_)
        ));
        client.abort();
    }

    #[tokio::test]
    async fn code_of_conduct_rejection_never_finishes_configuration() {
        use azalea_protocol::packets::config::c_code_of_conduct::ClientboundCodeOfConduct;
        use azalea_protocol::packets::config::c_finish_configuration::ClientboundFinishConfiguration;

        let (mut peer, client, event_rx, packet_tx) = code_of_conduct_peer().await;
        peer.write_packet(ClientboundCodeOfConduct {
            code_of_conduct: "Read this".into(),
        })
        .await
        .unwrap();
        peer.write_packet(ClientboundFinishConfiguration)
            .await
            .unwrap();
        assert!(matches!(
            recv_event(&event_rx).await,
            NetworkEvent::CodeOfConduct { .. }
        ));
        packet_tx
            .send(Outbound::CodeOfConductDecision(false))
            .unwrap();
        assert!(matches!(
            tokio::time::timeout(
                std::time::Duration::from_secs(1),
                peer.read_packet::<ServerboundConfigPacket>()
            )
            .await,
            Err(_) | Ok(Err(_))
        ));
        let result = tokio::time::timeout(std::time::Duration::from_secs(2), client)
            .await
            .expect("client timed out")
            .unwrap();
        assert!(
            matches!(result, Err(ConnectionError::Disconnected(reason)) if reason.contains("declined"))
        );
    }

    #[tokio::test]
    async fn duplicate_code_of_conduct_after_accept_never_finishes_configuration() {
        use azalea_protocol::packets::config::c_code_of_conduct::ClientboundCodeOfConduct;
        use azalea_protocol::packets::config::c_finish_configuration::ClientboundFinishConfiguration;

        let (mut peer, client, event_rx, packet_tx) = code_of_conduct_peer().await;
        let notice = || ClientboundCodeOfConduct {
            code_of_conduct: "Read this".into(),
        };
        peer.write_packet(notice()).await.unwrap();
        assert!(matches!(
            recv_event(&event_rx).await,
            NetworkEvent::CodeOfConduct { .. }
        ));
        packet_tx
            .send(Outbound::CodeOfConductDecision(true))
            .unwrap();
        assert!(matches!(
            read_test_packet::<ServerboundConfigPacket>(&mut peer).await,
            ServerboundConfigPacket::AcceptCodeOfConduct(_)
        ));
        peer.write_packet(notice()).await.unwrap();
        peer.write_packet(ClientboundFinishConfiguration)
            .await
            .unwrap();
        let result = tokio::time::timeout(std::time::Duration::from_secs(1), client)
            .await
            .unwrap()
            .unwrap();
        assert!(
            matches!(result, Err(ConnectionError::Disconnected(reason)) if reason.contains("duplicate"))
        );
        assert!(
            tokio::time::timeout(
                std::time::Duration::from_millis(30),
                peer.read_packet::<ServerboundConfigPacket>()
            )
            .await
            .is_err()
        );
    }

    #[tokio::test]
    async fn code_of_conduct_keeps_ping_live_before_accept() {
        use azalea_protocol::packets::config::c_code_of_conduct::ClientboundCodeOfConduct;
        use azalea_protocol::packets::config::c_ping::ClientboundPing;

        let (mut peer, client, event_rx, packet_tx) = code_of_conduct_peer().await;
        peer.write_packet(ClientboundCodeOfConduct {
            code_of_conduct: "Read this".into(),
        })
        .await
        .expect("send notice");
        assert!(
            matches!(
                recv_event(&event_rx).await,
                NetworkEvent::CodeOfConduct { .. }
            ),
            "notice event"
        );
        peer.write_packet(ClientboundPing { id: 0x1234 })
            .await
            .expect("send config ping");
        assert!(
            matches!(read_test_packet::<ServerboundConfigPacket>(&mut peer).await, ServerboundConfigPacket::Pong(p) if p.id == 0x1234),
            "pong before consent"
        );
        packet_tx
            .send(Outbound::CodeOfConductDecision(true))
            .expect("send consent");
        assert!(
            matches!(
                read_test_packet::<ServerboundConfigPacket>(&mut peer).await,
                ServerboundConfigPacket::AcceptCodeOfConduct(_)
            ),
            "accept after pong"
        );
        client.abort();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), client).await;
    }

    #[tokio::test]
    async fn code_of_conduct_defers_finish_and_keeps_ping_live() {
        use azalea_protocol::packets::config::c_code_of_conduct::ClientboundCodeOfConduct;
        use azalea_protocol::packets::config::c_finish_configuration::ClientboundFinishConfiguration;
        use azalea_protocol::packets::config::c_ping::ClientboundPing;

        let (mut peer, client, event_rx, packet_tx) = code_of_conduct_peer().await;
        peer.write_packet(ClientboundCodeOfConduct {
            code_of_conduct: "Read this".into(),
        })
        .await
        .expect("send notice");
        assert!(
            matches!(
                recv_event(&event_rx).await,
                NetworkEvent::CodeOfConduct { .. }
            ),
            "notice event"
        );
        peer.write_packet(ClientboundFinishConfiguration)
            .await
            .expect("send early finish");
        peer.write_packet(ClientboundPing { id: 0x5678 })
            .await
            .expect("send ping while finish pending");
        assert!(
            matches!(read_test_packet::<ServerboundConfigPacket>(&mut peer).await, ServerboundConfigPacket::Pong(p) if p.id == 0x5678),
            "pong while finish pending"
        );
        assert!(
            tokio::time::timeout(
                std::time::Duration::from_millis(50),
                peer.read_packet::<ServerboundConfigPacket>()
            )
            .await
            .is_err(),
            "finish must wait for consent"
        );
        packet_tx
            .send(Outbound::CodeOfConductDecision(true))
            .expect("send consent");
        assert!(
            matches!(
                read_test_packet::<ServerboundConfigPacket>(&mut peer).await,
                ServerboundConfigPacket::AcceptCodeOfConduct(_)
            ),
            "accept must precede finish"
        );
        assert!(
            matches!(
                read_test_packet::<ServerboundConfigPacket>(&mut peer).await,
                ServerboundConfigPacket::FinishConfiguration(_)
            ),
            "finish after accept"
        );
        client.abort();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), client).await;
    }

    #[tokio::test]
    async fn code_of_conduct_keeps_cookie_exchange_live_before_accept() {
        use azalea_protocol::packets::config::c_code_of_conduct::ClientboundCodeOfConduct;
        use azalea_protocol::packets::config::c_cookie_request::ClientboundCookieRequest;
        use azalea_protocol::packets::config::c_store_cookie::ClientboundStoreCookie;

        let (mut peer, client, event_rx, _) = code_of_conduct_peer().await;
        let key: azalea_registry::identifier::Identifier = "minecraft:session".parse().unwrap();
        peer.write_packet(ClientboundCodeOfConduct {
            code_of_conduct: "Read this".into(),
        })
        .await
        .expect("send notice");
        assert!(
            matches!(
                recv_event(&event_rx).await,
                NetworkEvent::CodeOfConduct { .. }
            ),
            "notice event"
        );
        peer.write_packet(ClientboundStoreCookie {
            key: key.clone(),
            payload: vec![1, 2, 3],
        })
        .await
        .expect("store cookie");
        peer.write_packet(ClientboundCookieRequest { key })
            .await
            .expect("request cookie");
        assert!(
            matches!(read_test_packet::<ServerboundConfigPacket>(&mut peer).await, ServerboundConfigPacket::CookieResponse(p) if p.payload.as_deref() == Some(&[1, 2, 3][..])),
            "cookie response before consent"
        );
        client.abort();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), client).await;
    }

    #[tokio::test]
    async fn configuration_transfer_carries_server_cookies() {
        use azalea_protocol::packets::config::c_store_cookie::ClientboundStoreCookie;
        use azalea_protocol::packets::config::c_transfer::ClientboundTransfer;

        let (mut peer, client, event_rx, _) = code_of_conduct_peer().await;
        let key: azalea_registry::identifier::Identifier = "minecraft:session".parse().unwrap();
        peer.write_packet(ClientboundStoreCookie {
            key: key.clone(),
            payload: vec![4, 5, 6],
        })
        .await
        .unwrap();
        peer.write_packet(ClientboundTransfer {
            host: "next.example".into(),
            port: 25570,
        })
        .await
        .unwrap();
        let transfer = recv_event(&event_rx).await;
        assert!(matches!(transfer, NetworkEvent::ServerTransfer(transfer)
            if transfer.host == "next.example"
                && transfer.port == 25570
                && transfer.cookies.get(&key) == Some(&vec![4, 5, 6])));
        assert!(matches!(
            tokio::time::timeout(std::time::Duration::from_secs(2), client)
                .await
                .expect("client timed out")
                .unwrap(),
            Err(ConnectionError::Transferred)
        ));
        assert!(event_rx.try_recv().is_err(), "transfer is not a disconnect");
    }

    #[tokio::test]
    async fn game_transfer_carries_server_cookies() {
        use azalea_protocol::packets::config::ServerboundConfigPacket;
        use azalea_protocol::packets::config::c_finish_configuration::ClientboundFinishConfiguration;
        use azalea_protocol::packets::config::c_store_cookie::ClientboundStoreCookie;
        use azalea_protocol::packets::game::c_transfer::ClientboundTransfer;

        let (mut peer, client, event_rx, _) = code_of_conduct_peer().await;
        let key: azalea_registry::identifier::Identifier = "minecraft:session".parse().unwrap();
        peer.write_packet(ClientboundStoreCookie {
            key: key.clone(),
            payload: vec![7, 8, 9],
        })
        .await
        .unwrap();
        peer.write_packet(ClientboundFinishConfiguration)
            .await
            .unwrap();
        assert!(matches!(
            read_test_packet::<ServerboundConfigPacket>(&mut peer).await,
            ServerboundConfigPacket::FinishConfiguration(_)
        ));
        peer.write_packet(ClientboundTransfer {
            host: "game.example".into(),
            port: 25571,
        })
        .await
        .unwrap();
        let transfer = loop {
            let event = recv_event(&event_rx).await;
            if let NetworkEvent::ServerTransfer(transfer) = event {
                break transfer;
            }
        };
        assert_eq!(transfer.host, "game.example");
        assert_eq!(transfer.port, 25571);
        assert_eq!(transfer.cookies.get(&key), Some(&vec![7, 8, 9]));
        assert!(matches!(
            tokio::time::timeout(std::time::Duration::from_secs(2), client)
                .await
                .expect("client timed out")
                .unwrap(),
            Err(ConnectionError::Transferred)
        ));
        assert!(event_rx.try_recv().is_err(), "transfer is not a disconnect");
    }

    #[tokio::test]
    async fn joins_an_integrated_server_over_the_pipe() {
        use azalea_auth::game_profile::GameProfile;
        use azalea_protocol::packets::config::c_finish_configuration::ClientboundFinishConfiguration;
        use azalea_protocol::packets::handshake::ServerboundHandshakePacket;
        use azalea_protocol::packets::login::c_login_finished::ClientboundLoginFinished;
        use uuid::Uuid;

        use crate::net::conn::memory_pipes;

        /// The next packet the client sent, in whichever phase the caller
        /// names.
        async fn sent<P: azalea_protocol::packets::ProtocolPacket + std::fmt::Debug>(
            peer: &mut Conn,
        ) -> P {
            read_test_packet(peer).await
        }

        let (client_end, server_end) = memory_pipes();
        let mut peer = Conn::from_memory(server_end);

        let (event_tx, event_rx) = crossbeam_channel::bounded(4096);
        let (packet_tx, packet_rx) = mpsc::unbounded_channel();

        let client = tokio::spawn(connect_to_server(
            ConnectArgs {
                transport: Transport::Memory(client_end),
                is_transfer: false,
                username: "ConfiguredName".to_owned(),
                uuid: Uuid::nil(),
                access_token: None,
                view_distance: 8,
                chat_options: crate::ui::chat::ChatOptions::default(),
                main_hand_right: false,
                particle_mode: crate::particle::ParticleMode::Minimal,
                skin_parts_mask: 0b0100101,
                server_cookies: Default::default(),
            },
            event_tx,
            packet_tx,
            packet_rx,
        ));

        assert!(matches!(
            sent(&mut peer).await,
            ServerboundHandshakePacket::Intention(p) if p.protocol_version == NATIVE.protocol
        ));
        assert!(matches!(
            sent(&mut peer).await,
            ServerboundLoginPacket::Hello(p) if p.name == "ConfiguredName"
        ));

        peer.write_packet(ClientboundLoginFinished {
            game_profile: GameProfile::new(Uuid::nil(), "AcceptedProfile".to_owned()),
            session_id: Uuid::nil(),
        })
        .await
        .unwrap();

        assert!(matches!(
            sent(&mut peer).await,
            ServerboundLoginPacket::LoginAcknowledged(_)
        ));
        assert!(matches!(
            sent(&mut peer).await,
            ServerboundConfigPacket::CustomPayload(p) if p.identifier.to_string() == "minecraft:brand"
        ));
        assert!(matches!(
            sent(&mut peer).await,
            ServerboundConfigPacket::ClientInformation(p)
                if p.information.view_distance == 8
                    && p.information.main_hand == azalea_entity::HumanoidArm::Left
                    && p.information.particle_status == azalea_protocol::common::client_information::ParticleStatus::Minimal
                    && p.information.model_customization.cape
                    && !p.information.model_customization.jacket
                    && p.information.model_customization.left_sleeve
                    && !p.information.model_customization.right_sleeve
                    && !p.information.model_customization.left_pants
                    && p.information.model_customization.right_pants
                    && !p.information.model_customization.hat
        ));

        peer.write_packet(ClientboundFinishConfiguration)
            .await
            .unwrap();

        assert!(matches!(
            sent(&mut peer).await,
            ServerboundConfigPacket::FinishConfiguration(_)
        ));

        // Emitted as soon as configuration ends, before any game packet.
        let events = vec![recv_event(&event_rx).await, recv_event(&event_rx).await];
        assert!(matches!(events[0], NetworkEvent::BiomeColors { .. }));
        assert!(
            matches!(&events[1], NetworkEvent::Connected { profile_name } if profile_name == "AcceptedProfile")
        );

        client.abort();
    }
}
