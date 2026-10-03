use azalea_buf::{AzBuf, AzBufVar};
use azalea_core::bitset::FixedBitSet;
use azalea_core::position::{BlockPos, ChunkPos};
use azalea_core::registry_holder::RegistryHolder;
use azalea_core::sound::CustomSound;
use azalea_protocol::packets::game::{ClientboundGamePacket, ServerboundGamePacket};
use azalea_registry::builtin::{EntityKind, SoundEvent};
use azalea_registry::identifier::Identifier;
use azalea_registry::{DataRegistry, Holder, Registry};
use crossbeam_channel::{SendError, Sender};

use super::NetworkEvent;
use super::chat_security::ProfileKeyServices;
use super::commands::{CommandTree, SharedCommandTree};
use super::connection::send_event;
use super::sender::PacketSender;
use crate::entity::MetaValue;
use crate::entity::components::Position;
use crate::net::chunk_batch::ChunkBatchSizeCalculator;
use crate::player::inventory::item_resource_name;
use crate::renderer::pipelines::entity_renderer::{
    CAT_VARIANT_ORDER, CHICKEN_VARIANT_ORDER, COW_VARIANT_ORDER, WOLF_VARIANT_ORDER,
};
use crate::ui::server_dialog::DialogReference;
use crate::ui::text::format_text_spans;
use crate::world::block::model::CardinalLightType;

pub(crate) fn attribute_event(
    entity_id: i32,
    snapshot: azalea_protocol::packets::game::c_update_attributes::AttributeSnapshot,
) -> NetworkEvent {
    NetworkEvent::EntityAttributeUpdate {
        entity_id,
        snapshot,
    }
}

fn dialog_holder_reference(
    holder: &azalea_registry::Holder<azalea_registry::data::Dialog, simdnbt::owned::Nbt>,
) -> DialogReference {
    match holder {
        azalea_registry::Holder::Reference(dialog) => DialogReference::ProtocolId(dialog.to_u32()),
        azalea_registry::Holder::Direct(nbt) => DialogReference::inline(nbt),
    }
}

/// Dimension info from a login/respawn registry entry. Fields that Azalea does
/// not model directly live in its flattened extras. Missing `has_skylight`
/// defaults to true; missing `cardinal_light` defaults to vanilla's `default`.
fn dimension_clock_id(
    world_clock_ids: Option<&[Identifier]>,
    dim: &azalea_core::registry_holder::dimension_type::DimensionKindElement,
) -> Option<u32> {
    let Some(clock_name) = dim
        ._extra
        .get("default_clock")
        .and_then(|tag| tag.string())
        .map(|value| value.to_string())
    else {
        tracing::debug!("Dimension has no default_clock; world clock is unknown");
        return None;
    };
    // 26.2's official registry key is minecraft:world_clock. Resolve the
    // server-provided entry order; never infer an ID from a dimension name.
    let Some(world_clock_ids) = world_clock_ids else {
        tracing::warn!("Server omitted minecraft:world_clock registry; world clock is unknown");
        return None;
    };
    let Some(id) = world_clock_ids
        .iter()
        .position(|key| key.to_string() == clock_name)
    else {
        tracing::warn!(clock = %clock_name, "Dimension default_clock is absent from minecraft:world_clock");
        return None;
    };
    Some(id as u32)
}

fn dimension_info(
    dim: &azalea_core::registry_holder::dimension_type::DimensionKindElement,
    is_debug: bool,
    clock_id: Option<u32>,
    world_key: &str,
    timeline_entries: &super::environment::TimelineEntries,
    timeline_entries_error: Option<&str>,
    world_clock_ids: Option<&[Identifier]>,
) -> NetworkEvent {
    NetworkEvent::DimensionInfo {
        is_debug,
        height: dim.height,
        min_y: dim.min_y,
        clock_id,
        world_clock_ids: world_clock_ids.map(|ids| {
            ids.iter()
                .enumerate()
                .map(|(id, key)| (key.to_string(), id as u32))
                .collect()
        }),
        has_skylight: dim
            ._extra
            .get("has_skylight")
            .and_then(|tag| tag.byte())
            .map(|b| b != 0)
            .unwrap_or(true),
        environment_input: super::environment::DimensionEnvironmentInput {
            has_sky_light: dim
                ._extra
                .get("has_skylight")
                .and_then(|tag| tag.byte())
                .map(|b| b != 0)
                .unwrap_or(true),
            has_ceiling: dim
                ._extra
                .get("has_ceiling")
                .and_then(|tag| tag.byte())
                .is_some_and(|b| b != 0),
            is_end_world: world_key == "minecraft:the_end",
            ambient_light: dim._extra.get("ambient_light").and_then(|tag| tag.float()),
            sky_light_level: dim
                ._extra
                .get("attributes")
                .and_then(|tag| tag.compound())
                .and_then(|attrs| attrs.float(super::environment::SKY_LIGHT_LEVEL_ATTRIBUTE)),
            timeline_refs: dim
                ._extra
                .get("timelines")
                .and_then(|tag| tag.list())
                .and_then(|list| list.strings())
                .map(|items| {
                    items
                        .iter()
                        .map(|item| item.to_str().into_owned())
                        .collect()
                })
                .unwrap_or_default(),
            timeline_entries: timeline_entries.clone(),
            timeline_entries_error: timeline_entries_error.map(str::to_owned),
        },
        cardinal_light: match dim
            ._extra
            .get("cardinal_light")
            .and_then(|tag| tag.string())
            .map(|value| value.to_str())
            .as_deref()
        {
            Some("nether") => CardinalLightType::Nether,
            _ => CardinalLightType::Default,
        },
    }
}

pub async fn handle_game_packet(
    packet: &ClientboundGamePacket,
    sender: &PacketSender,
    event_tx: &Sender<NetworkEvent>,
    registry_holder: &RegistryHolder,
    shared_tree: &SharedCommandTree,
    batch_size_calculator: &mut ChunkBatchSizeCalculator,
    current_dimension: &mut (u32, i32),
    server_cookies: &mut std::collections::HashMap<
        azalea_registry::identifier::Identifier,
        Vec<u8>,
    >,
) -> Result<(), SendError<NetworkEvent>> {
    handle_game_packet_with_display_text(
        packet,
        sender,
        event_tx,
        registry_holder,
        &std::sync::Arc::new(Vec::new()),
        Some("timeline registry snapshot unavailable"),
        None,
        shared_tree,
        batch_size_calculator,
        current_dimension,
        server_cookies,
        &mut std::collections::VecDeque::new(),
    )
    .await
}

pub(super) async fn handle_game_packet_with_display_text(
    packet: &ClientboundGamePacket,
    sender: &PacketSender,
    event_tx: &Sender<NetworkEvent>,
    registry_holder: &RegistryHolder,
    timeline_entries: &super::environment::TimelineEntries,
    timeline_entries_error: Option<&str>,
    world_clock_ids: Option<&[Identifier]>,
    shared_tree: &SharedCommandTree,
    batch_size_calculator: &mut ChunkBatchSizeCalculator,
    current_dimension: &mut (u32, i32),
    server_cookies: &mut std::collections::HashMap<Identifier, Vec<u8>>,
    display_text: &mut std::collections::VecDeque<NetworkEvent>,
) -> Result<(), SendError<NetworkEvent>> {
    // All events stay FIFO: a full UI queue waits without losing world state.
    match packet {
        ClientboundGamePacket::Login(p) => {
            // With no registry entry the main-thread store keeps its current
            // height (initially overworld); decode with that same height.
            if let Some((_, dim)) = p.common.dimension_type(registry_holder) {
                *current_dimension = (dim.height, dim.min_y);
                send_event(
                    event_tx,
                    dimension_info(
                        dim,
                        p.common.is_debug,
                        dimension_clock_id(world_clock_ids, dim),
                        &p.common.dimension.to_string(),
                        timeline_entries,
                        timeline_entries_error,
                        world_clock_ids,
                    ),
                )
                .await?;
            }
            send_event(
                event_tx,
                NetworkEvent::DimensionName {
                    name: p.common.dimension.to_string(),
                },
            )
            .await?;
            send_event(
                event_tx,
                NetworkEvent::GameModeChanged {
                    game_mode: p.common.game_type as u8,
                    previous: Some(p.common.previous_game_type.0.map(|m| m.to_id())),
                },
            )
            .await?;
            send_event(
                event_tx,
                NetworkEvent::ServerViewDistance {
                    distance: p.chunk_radius,
                },
            )
            .await?;
            send_event(
                event_tx,
                NetworkEvent::ServerSimulationDistance {
                    distance: p.simulation_distance,
                },
            )
            .await?;
            send_event(
                event_tx,
                NetworkEvent::PlayerLogin {
                    entity_id: p.player_id.0,
                    hardcore: p.hardcore,
                    show_death_screen: p.show_death_screen,
                    online_mode: p.online_mode,
                },
            )
            .await?;
            send_event(
                event_tx,
                NetworkEvent::SecureChatEnforced {
                    enforced: ProfileKeyServices::get().is_some() && p.enforces_secure_chat,
                },
            )
            .await?;
        }
        ClientboundGamePacket::ChunksBiomes(p) => {
            for chunk in &p.chunk_biome_data {
                send_event(
                    event_tx,
                    NetworkEvent::ChunkBiomes {
                        pos: chunk.pos,
                        data: chunk.buffer.clone(),
                    },
                )
                .await?;
            }
        }
        ClientboundGamePacket::LevelChunkWithLight(p) => {
            tracing::trace!(
                "Chunk [{}, {}] ({} block entities)",
                p.x,
                p.z,
                p.chunk_data.block_entities.len()
            );
            let chunk_pos = ChunkPos::new(p.x, p.z);
            let block_entities = p
                .chunk_data
                .block_entities
                .iter()
                .map(|be| {
                    let local_x = ((be.packed_xz >> 4) & 0x0F) as i32;
                    let local_z = (be.packed_xz & 0x0F) as i32;
                    let block_pos = azalea_core::position::BlockPos {
                        x: chunk_pos.x * 16 + local_x,
                        y: be.y as i16 as i32,
                        z: chunk_pos.z * 16 + local_z,
                    };
                    let compound = match &be.data {
                        simdnbt::owned::Nbt::Some(base) => base.clone().as_compound(),
                        // Vanilla omits empty update tags (e.g. copper golem statues).
                        simdnbt::owned::Nbt::None => simdnbt::owned::NbtCompound::new(),
                    };
                    (block_pos, be.kind, compound)
                })
                .collect();
            let (height, min_y) = *current_dimension;
            let data = p.chunk_data.data.clone();
            let heightmaps = p.chunk_data.heightmaps.clone();
            // ponytail: one decode in flight per connection (no unbounded workers).
            // Measure p99 decode/keepalive lag; if inbound keepalive waits >1s,
            // use a bounded ordered decoder queue (never reorder block updates).
            let chunk = match tokio::task::spawn_blocking(move || {
                azalea_world::chunk::Chunk::read_with_dimension_height(
                    &mut std::io::Cursor::new(data.as_ref().as_ref()),
                    height,
                    min_y,
                    &heightmaps,
                )
            })
            .await
            {
                Ok(Ok(chunk)) => chunk,
                Ok(Err(error)) => {
                    tracing::warn!(chunk = ?chunk_pos, %error, "Skipping malformed chunk");
                    return Ok(());
                }
                Err(error) => {
                    tracing::warn!(chunk = ?chunk_pos, %error, "Skipping failed chunk decode");
                    return Ok(());
                }
            };
            send_event(
                event_tx,
                NetworkEvent::ChunkLoaded {
                    pos: chunk_pos,
                    chunk: Box::new(chunk),
                    light: (&p.light_data).into(),
                    block_entities,
                },
            )
            .await?;
        }
        ClientboundGamePacket::BlockEvent(p) => {
            send_event(
                event_tx,
                NetworkEvent::BlockEvent {
                    pos: p.pos,
                    action_id: p.action_id,
                    action_parameter: p.action_parameter,
                },
            )
            .await?;
        }
        ClientboundGamePacket::Explode(p) => {
            send_event(
                event_tx,
                NetworkEvent::Explosion(super::ExplosionPayload {
                    center: p.center,
                    radius: p.radius,
                    block_count: p.block_count,
                    player_knockback: p.player_knockback,
                    explosion_particle: p.explosion_particle.clone(),
                    explosion_sound: crate::audio::SoundRef::event(p.explosion_sound.to_str()),
                    block_particles: p.block_particles.clone(),
                }),
            )
            .await?;
        }
        ClientboundGamePacket::Sound(p) => {
            // Coordinates are fixed-point: block position times 8.
            send_event(
                event_tx,
                NetworkEvent::PlaySound {
                    sound: crate::audio::SoundRef::resolve(&p.sound),
                    category: p.source as u8,
                    pos: Position::new(p.x as f64 / 8.0, p.y as f64 / 8.0, p.z as f64 / 8.0),
                    volume: p.volume,
                    pitch: p.pitch,
                    seed: p.seed,
                },
            )
            .await?;
        }
        ClientboundGamePacket::SoundEntity(p) => {
            send_event(
                event_tx,
                NetworkEvent::PlayEntitySound {
                    sound: crate::audio::SoundRef::resolve(&p.sound),
                    category: p.source as u8,
                    entity_id: p.id.0,
                    volume: p.volume,
                    pitch: p.pitch,
                    seed: p.seed,
                },
            )
            .await?;
        }
        ClientboundGamePacket::StopSound(p) => {
            send_event(
                event_tx,
                NetworkEvent::StopSound {
                    sound_id: p.name.as_ref().map(ToString::to_string),
                    category: p.source.map(|source| source as u8),
                },
            )
            .await?;
        }
        ClientboundGamePacket::BlockEntityData(p) => {
            let nbt = match &p.tag {
                simdnbt::owned::Nbt::Some(base) => Some(base.clone().as_compound()),
                simdnbt::owned::Nbt::None => None,
            };
            send_event(
                event_tx,
                NetworkEvent::BlockEntityUpdate {
                    pos: p.pos,
                    kind: p.block_entity_type,
                    nbt,
                },
            )
            .await?;
        }
        ClientboundGamePacket::LightUpdate(p) => {
            send_event(
                event_tx,
                NetworkEvent::LightUpdate {
                    pos: ChunkPos::new(p.x, p.z),
                    light: (&p.light_data).into(),
                },
            )
            .await?;
        }
        ClientboundGamePacket::ForgetLevelChunk(p) => {
            send_event(event_tx, NetworkEvent::ChunkUnloaded { pos: p.pos }).await?;
        }
        ClientboundGamePacket::SetChunkCacheCenter(p) => {
            send_event(event_tx, NetworkEvent::ChunkCacheCenter { x: p.x, z: p.z }).await?;
        }
        ClientboundGamePacket::PlayerPosition(p) => {
            send_event(
                event_tx,
                NetworkEvent::PlayerPosition {
                    id: p.id,
                    change: p.change.clone(),
                    relative: p.relative.clone(),
                },
            )
            .await?;
        }
        ClientboundGamePacket::MoveVehicle(p) => {
            send_event(
                event_tx,
                NetworkEvent::MoveVehicle {
                    pos: glam::dvec3(p.pos.x, p.pos.y, p.pos.z),
                    yaw: p.look_direction.y_rot(),
                    pitch: p.look_direction.x_rot(),
                },
            )
            .await?;
        }
        ClientboundGamePacket::PlayerRotation(p) => {
            send_event(
                event_tx,
                NetworkEvent::PlayerRotation {
                    y_rot: p.y_rot,
                    x_rot: p.x_rot,
                    relative_y: p.relative_y,
                    relative_x: p.relative_x,
                },
            )
            .await?;
        }
        ClientboundGamePacket::KeepAlive(p) => {
            sender.send(ServerboundGamePacket::KeepAlive(
                azalea_protocol::packets::game::s_keep_alive::ServerboundKeepAlive { id: p.id },
            ));
        }
        ClientboundGamePacket::Ping(p) => {
            send_event(event_tx, NetworkEvent::Ping { id: p.id as i32 }).await?;
        }
        ClientboundGamePacket::StoreCookie(p) => {
            server_cookies.insert(p.key.clone(), p.payload.clone());
        }
        ClientboundGamePacket::CookieRequest(p) => {
            sender.send(ServerboundGamePacket::CookieResponse(
                azalea_protocol::packets::game::s_cookie_response::ServerboundCookieResponse {
                    payload: server_cookies.get(&p.key).cloned(),
                    key: p.key.clone(),
                },
            ));
        }
        ClientboundGamePacket::ChunkBatchStart(_) => {
            batch_size_calculator.on_batch_start();
        }
        ClientboundGamePacket::ChunkBatchFinished(p) => {
            // Answered on the network thread: vanilla's
            // `handleChunkBatchFinished` is one of the few handlers it doesn't
            // defer to the main thread.
            batch_size_calculator.on_batch_finished(p.batch_size);
            sender.send(ServerboundGamePacket::ChunkBatchReceived(
                azalea_protocol::packets::game::s_chunk_batch_received::ServerboundChunkBatchReceived {
                    desired_chunks_per_tick: batch_size_calculator.desired_chunks_per_tick(),
                },
            ));
        }
        ClientboundGamePacket::ContainerSetContent(p) => {
            send_event(
                event_tx,
                NetworkEvent::ContainerContent {
                    container_id: p.container_id,
                    items: p.items.clone(),
                    carried: p.carried_item.clone(),
                    state_id: p.state_id,
                },
            )
            .await?;
        }
        ClientboundGamePacket::SetCursorItem(p) => {
            send_event(
                event_tx,
                NetworkEvent::CursorItem {
                    item: p.contents.clone(),
                },
            )
            .await?;
        }
        ClientboundGamePacket::ContainerSetSlot(p) => {
            send_event(
                event_tx,
                NetworkEvent::ContainerSlot {
                    container_id: p.container_id,
                    index: p.slot,
                    item: p.item_stack.clone(),
                    state_id: p.state_id,
                },
            )
            .await?;
        }
        ClientboundGamePacket::SetHeldSlot(p) if (0..9).contains(&p.slot) => {
            send_event(event_tx, NetworkEvent::HeldSlot { slot: p.slot as u8 }).await?;
        }
        ClientboundGamePacket::ContainerSetData(p) => {
            send_event(
                event_tx,
                NetworkEvent::ContainerData {
                    container_id: p.container_id,
                    id: p.id,
                    value: p.value,
                },
            )
            .await?;
        }
        ClientboundGamePacket::MerchantOffers(p) => {
            send_event(
                event_tx,
                NetworkEvent::MerchantOffers {
                    container_id: p.container_id,
                    offers: p.offers.clone(),
                    villager_level: p.villager_level,
                    villager_xp: p.villager_xp,
                    show_progress: p.show_progress,
                    can_restock: p.can_restock,
                },
            )
            .await?;
        }
        ClientboundGamePacket::MountScreenOpen(p) => {
            send_event(
                event_tx,
                NetworkEvent::MountScreenOpen {
                    container_id: p.container_id,
                    inventory_columns: p.inventory_columns,
                    entity_id: p.entity_id.0,
                },
            )
            .await?;
        }
        ClientboundGamePacket::InitializeBorder(p) => {
            send_event(
                event_tx,
                NetworkEvent::WorldBorderInitialize {
                    center_x: p.new_center_x,
                    center_z: p.new_center_z,
                    old_size: p.old_size,
                    new_size: p.new_size,
                    lerp_time: i64::try_from(p.lerp_time).unwrap_or(i64::MAX),
                    absolute_max_size: p.new_absolute_max_size as i32,
                    warning_blocks: p.warning_blocks as i32,
                    warning_time: p.warning_time as i32,
                },
            )
            .await?;
        }
        ClientboundGamePacket::SetBorderCenter(p) => {
            send_event(
                event_tx,
                NetworkEvent::WorldBorderCenter {
                    x: p.new_center_x,
                    z: p.new_center_z,
                },
            )
            .await?;
        }
        ClientboundGamePacket::SetBorderSize(p) => {
            send_event(event_tx, NetworkEvent::WorldBorderSize { size: p.size }).await?;
        }
        ClientboundGamePacket::SetBorderLerpSize(p) => {
            send_event(
                event_tx,
                NetworkEvent::WorldBorderLerpSize {
                    old_size: p.old_size,
                    new_size: p.new_size,
                    lerp_time: i64::try_from(p.lerp_time).unwrap_or(i64::MAX),
                },
            )
            .await?;
        }
        ClientboundGamePacket::SetBorderWarningDistance(p) => {
            send_event(
                event_tx,
                NetworkEvent::WorldBorderWarningBlocks {
                    warning_blocks: p.warning_blocks as i32,
                },
            )
            .await?;
        }
        ClientboundGamePacket::SetBorderWarningDelay(p) => {
            send_event(
                event_tx,
                NetworkEvent::WorldBorderWarningTime {
                    warning_time: p.warning_delay as i32,
                },
            )
            .await?;
        }
        ClientboundGamePacket::OpenScreen(p) => {
            send_event(
                event_tx,
                NetworkEvent::OpenScreen {
                    container_id: p.container_id,
                    menu_type: p.menu_type,
                    title: p.title.to_string(),
                },
            )
            .await?;
        }
        ClientboundGamePacket::OpenBook(p) => {
            send_event(event_tx, NetworkEvent::OpenBook { hand: p.hand }).await?;
        }
        ClientboundGamePacket::ContainerClose(_) => {
            send_event(event_tx, NetworkEvent::ContainerClosed).await?;
        }
        ClientboundGamePacket::SetHealth(p) => {
            send_event(
                event_tx,
                NetworkEvent::PlayerHealth {
                    health: p.health,
                    food: p.food,
                    saturation: p.saturation,
                },
            )
            .await?;
        }
        ClientboundGamePacket::SetExperience(p) => {
            send_event(
                event_tx,
                NetworkEvent::PlayerExperience {
                    progress: p.experience_progress,
                    level: p.experience_level as i32,
                    total_experience: p.total_experience,
                },
            )
            .await?;
        }
        ClientboundGamePacket::SetPlayerInventory(p) => {
            send_event(
                event_tx,
                NetworkEvent::SetPlayerInventory {
                    slot: p.slot,
                    item: p.contents.clone(),
                },
            )
            .await?;
        }
        ClientboundGamePacket::UpdateMobEffect(p) => {
            send_event(
                event_tx,
                NetworkEvent::UpdateMobEffect {
                    entity_id: p.entity_id.0,
                    effect: crate::mob_effect::MobEffectInstance {
                        effect_id: p.mob_effect.to_u32(),
                        amplifier: p.data.amplifier.clamp(0, u8::MAX as i32) as u8,
                        duration: p.data.duration,
                        ambient: p.data.flags.ambient,
                        show_particles: p.data.flags.show_particles,
                        show_icon: p.data.flags.show_icon,
                    },
                },
            )
            .await?;
        }
        ClientboundGamePacket::RemoveMobEffect(p) => {
            send_event(
                event_tx,
                NetworkEvent::RemoveMobEffect {
                    entity_id: p.entity_id.0,
                    effect_id: p.effect.to_u32(),
                },
            )
            .await?;
        }
        ClientboundGamePacket::Waypoint(p) => {
            send_event(
                event_tx,
                NetworkEvent::Waypoint {
                    operation: p.operation,
                    waypoint: p.waypoint.clone(),
                },
            )
            .await?;
        }
        ClientboundGamePacket::MapItemData(p) => {
            let decorations = p.decorations.as_ref().map(|items| {
                items
                    .iter()
                    .map(|d| {
                        let asset = crate::world::maps::MapDecorationAsset::from_registry_id(
                            d.decoration_type as u32,
                        );
                        crate::world::maps::MapDecoration {
                            asset,
                            x: d.x,
                            y: d.y,
                            rotation: d.rot,
                            name: d.name.as_ref().map(ToString::to_string),
                            show_on_item_frame: asset.show_on_item_frame(),
                        }
                    })
                    .collect()
            });
            let patch = p.color_patch.0.as_ref().and_then(|patch| {
                (patch.width != 0).then(|| {
                    (
                        patch.width,
                        patch.height,
                        patch.start_x,
                        patch.start_y,
                        patch.map_colors.clone(),
                    )
                })
            });
            send_event(
                event_tx,
                NetworkEvent::MapItemData {
                    map_id: p.map_id,
                    scale: p.scale,
                    locked: p.locked,
                    patch,
                    decorations,
                },
            )
            .await?;
        }
        ClientboundGamePacket::UpdateAttributes(p) => {
            for snapshot in &p.values {
                send_event(event_tx, attribute_event(p.entity_id.0, snapshot.clone())).await?;
            }
        }
        ClientboundGamePacket::PlayerAbilities(p) => {
            send_event(
                event_tx,
                NetworkEvent::PlayerAbilitiesChanged {
                    invulnerable: p.flags.invulnerable,
                    flying: p.flags.flying,
                    can_fly: p.flags.can_fly,
                    instant_break: p.flags.instant_break,
                    flying_speed: p.flying_speed,
                    walking_speed: p.walking_speed,
                },
            )
            .await?;
        }
        ClientboundGamePacket::BossEvent(p) => {
            use azalea_protocol::packets::game::c_boss_event::Operation;

            use crate::ui::boss_bar::BossBarOp;
            let op = match &p.operation {
                Operation::Add(add) => BossBarOp::Add {
                    name: format_text_spans(&add.name, [1.0; 4]),
                    progress: add.progress,
                    color: add.style.color as u8,
                    overlay: add.style.overlay as u8,
                    darken_screen: add.properties.darken_screen,
                    play_music: add.properties.play_music,
                    create_world_fog: add.properties.create_world_fog,
                },
                Operation::Remove => BossBarOp::Remove,
                Operation::UpdateProgress(progress) => BossBarOp::UpdateProgress(*progress),
                Operation::UpdateName(name) => {
                    BossBarOp::UpdateName(format_text_spans(name, [1.0; 4]))
                }
                Operation::UpdateStyle(style) => BossBarOp::UpdateStyle {
                    color: style.color as u8,
                    overlay: style.overlay as u8,
                },
                Operation::UpdateProperties(props) => BossBarOp::UpdateProperties {
                    darken_screen: props.darken_screen,
                    play_music: props.play_music,
                    create_world_fog: props.create_world_fog,
                },
            };
            send_event(event_tx, NetworkEvent::BossBarUpdate { id: p.id, op }).await?;
        }
        ClientboundGamePacket::UpdateAdvancements(p) => {
            use azalea_protocol::packets::game::c_update_advancements::FrameType;

            use crate::ui::toast;
            let added = p
                .added
                .iter()
                .map(|holder| {
                    (
                        holder.id.to_string(),
                        toast::AdvancementData {
                            display: holder.value.display.as_deref().map(|d| {
                                toast::AdvancementDisplay {
                                    title: format_text_spans(&d.title, [1.0; 4]),
                                    frame: match d.frame {
                                        FrameType::Task => toast::AdvancementFrame::Task,
                                        FrameType::Challenge => toast::AdvancementFrame::Challenge,
                                        FrameType::Goal => toast::AdvancementFrame::Goal,
                                    },
                                    show_toast: d.show_toast,
                                    icon_item: match &d.icon {
                                        azalea_inventory::ItemStack::Present(data) => {
                                            Some(item_resource_name(data.kind))
                                        }
                                        azalea_inventory::ItemStack::Empty => None,
                                    },
                                }
                            }),
                            requirements: holder.value.requirements.clone(),
                        },
                    )
                })
                .collect();
            let progress = p
                .progress
                .iter()
                .map(|(id, criteria)| {
                    (
                        id.to_string(),
                        criteria
                            .iter()
                            .map(|(name, c)| (name.clone(), c.date.is_some()))
                            .collect(),
                    )
                })
                .collect();
            send_event(
                event_tx,
                NetworkEvent::AdvancementsUpdate(Box::new(toast::AdvancementsUpdate {
                    reset: p.reset,
                    added,
                    removed: p.removed.iter().map(|id| id.to_string()).collect(),
                    progress,
                    show_advancements: p.show_advancements,
                })),
            )
            .await?;
        }
        ClientboundGamePacket::RecipeBookAdd(p) => {
            // Entry.FLAG_NOTIFICATION = 1 (ClientboundRecipeBookAddPacket).
            let entries: Vec<_> = p
                .entries
                .iter()
                .filter(|e| e.flags & 1 != 0)
                .map(|e| recipe_toast_entry(&e.contents.display))
                .collect();
            if !entries.is_empty() {
                send_event(event_tx, NetworkEvent::RecipeToastAdd { entries }).await?;
            }
            send_event(event_tx, NetworkEvent::RecipeBookAdd(p.clone())).await?;
        }
        ClientboundGamePacket::PlaceGhostRecipe(p) => {
            send_event(event_tx, NetworkEvent::PlaceGhostRecipe(p.clone())).await?;
        }
        ClientboundGamePacket::RecipeBookRemove(p) => {
            send_event(event_tx, NetworkEvent::RecipeBookRemove(p.recipes.clone())).await?;
        }
        ClientboundGamePacket::RecipeBookSettings(p) => {
            send_event(
                event_tx,
                NetworkEvent::RecipeBookSettings(p.book_settings.clone()),
            )
            .await?;
        }
        ClientboundGamePacket::UpdateRecipes(p) => {
            send_event(event_tx, NetworkEvent::UpdateRecipes(p.clone())).await?;
        }
        ClientboundGamePacket::UpdateTags(p) => {
            send_event(event_tx, NetworkEvent::RecipeItemTags(p.tags.clone())).await?;
            let (patterns, items) = super::connection::loom_pattern_tags(&p.tags);
            send_event(event_tx, NetworkEvent::LoomPatternTags(patterns, items)).await?;
        }
        ClientboundGamePacket::SetTitleText(p) => {
            send_event(
                event_tx,
                NetworkEvent::TitleText {
                    spans: format_text_spans(&p.text, [1.0; 4]),
                },
            )
            .await?;
        }
        ClientboundGamePacket::SetSubtitleText(p) => {
            send_event(
                event_tx,
                NetworkEvent::SubtitleText {
                    spans: format_text_spans(&p.text, [1.0; 4]),
                },
            )
            .await?;
        }
        ClientboundGamePacket::SetTitlesAnimation(p) => {
            // azalea decodes the fields as u32; vanilla reads signed ints and
            // ignores negatives, so restore the sign before forwarding.
            send_event(
                event_tx,
                NetworkEvent::TitlesAnimation {
                    fade_in: p.fade_in as i32,
                    stay: p.stay as i32,
                    fade_out: p.fade_out as i32,
                },
            )
            .await?;
        }
        ClientboundGamePacket::ClearTitles(p) => {
            send_event(
                event_tx,
                NetworkEvent::ClearTitles {
                    reset_times: p.reset_times,
                },
            )
            .await?;
        }
        ClientboundGamePacket::SetObjective(p) => {
            use azalea_protocol::packets::game::c_set_objective::Method;
            let (display, number_format, render_type) = match &p.method {
                Method::Add {
                    display_name,
                    number_format,
                    render_type,
                    ..
                }
                | Method::Change {
                    display_name,
                    number_format,
                    render_type,
                    ..
                } => {
                    let number_format = match objective_number_format(number_format) {
                        Ok(number_format) => number_format,
                        Err(error) => {
                            tracing::warn!(
                                objective = %p.objective_name,
                                %error,
                                "Skipping malformed scoreboard objective number format"
                            );
                            return Ok(());
                        }
                    };
                    (
                        Some(format_text_spans(display_name, [1.0; 4])),
                        number_format,
                        Some(render_type.clone()),
                    )
                }
                Method::Remove => (None, None, None),
            };
            send_event(
                event_tx,
                NetworkEvent::ScoreboardObjective {
                    name: p.objective_name.clone(),
                    display,
                    number_format,
                    render_type,
                },
            )
            .await?;
        }
        ClientboundGamePacket::SetDisplayObjective(p) => {
            send_event(
                event_tx,
                NetworkEvent::ScoreboardDisplay {
                    slot: p.slot,
                    name: (!p.objective_name.is_empty()).then(|| p.objective_name.clone()),
                },
            )
            .await?;
        }
        ClientboundGamePacket::SetScore(p) => {
            let number_format = match p
                .number_format
                .as_ref()
                .map(score_number_format)
                .transpose()
            {
                Ok(number_format) => number_format,
                Err(error) => {
                    tracing::warn!(
                        owner = %p.owner,
                        objective = %p.objective_name,
                        %error,
                        "Skipping malformed scoreboard score number format"
                    );
                    return Ok(());
                }
            };
            send_event(
                event_tx,
                NetworkEvent::ScoreboardScore {
                    owner: p.owner.clone(),
                    objective: p.objective_name.clone(),
                    // Wire scores are signed varints; azalea models the field
                    // unsigned.
                    score: p.score as i32,
                    display: p
                        .display
                        .as_ref()
                        .map(|text| format_text_spans(text, [1.0; 4])),
                    number_format,
                },
            )
            .await?;
        }
        ClientboundGamePacket::ResetScore(p) => {
            send_event(
                event_tx,
                NetworkEvent::ScoreboardReset {
                    owner: p.owner.clone(),
                    objective: p.objective_name.clone(),
                },
            )
            .await?;
        }
        ClientboundGamePacket::SetPlayerTeam(p) => {
            use azalea_protocol::packets::game::c_set_player_team::Method;
            match &p.method {
                Method::Add((parameters, members)) => {
                    send_scoreboard_team(event_tx, &p.name, parameters, Some(members.clone()))
                        .await?
                }
                Method::Change(parameters) => {
                    send_scoreboard_team(event_tx, &p.name, parameters, None).await?
                }
                Method::Join(members) | Method::Leave(members) => {
                    send_event(
                        event_tx,
                        NetworkEvent::ScoreboardTeamMembers {
                            name: p.name.clone(),
                            members: members.clone(),
                            join: matches!(p.method, Method::Join(_)),
                        },
                    )
                    .await?;
                }
                Method::Remove => {
                    send_event(
                        event_tx,
                        NetworkEvent::ScoreboardTeamRemoved {
                            name: p.name.clone(),
                        },
                    )
                    .await?;
                }
            }
        }
        ClientboundGamePacket::BlockUpdate(p) => {
            send_event(
                event_tx,
                NetworkEvent::BlockUpdate {
                    pos: p.pos,
                    state: p.block_state,
                },
            )
            .await?;
        }
        ClientboundGamePacket::SectionBlocksUpdate(p) => {
            let updates: Vec<_> = p
                .states
                .iter()
                .map(|s| {
                    let block_pos = azalea_core::position::BlockPos {
                        x: p.section_pos.x * 16 + s.pos.x as i32,
                        y: p.section_pos.y * 16 + s.pos.y as i32,
                        z: p.section_pos.z * 16 + s.pos.z as i32,
                    };
                    (block_pos, s.state)
                })
                .collect();
            send_event(event_tx, NetworkEvent::SectionBlocksUpdate { updates }).await?;
        }
        ClientboundGamePacket::BlockChangedAck(p) => {
            send_event(event_tx, NetworkEvent::BlockChangedAck { seq: p.seq }).await?;
        }
        ClientboundGamePacket::TickingState(p) => {
            send_event(
                event_tx,
                NetworkEvent::TickingState {
                    tick_rate: p.tick_rate,
                    is_frozen: p.is_frozen,
                },
            )
            .await?;
        }
        ClientboundGamePacket::TickingStep(p) => {
            send_event(
                event_tx,
                NetworkEvent::TickingStep {
                    tick_steps: p.tick_steps,
                },
            )
            .await?;
        }
        ClientboundGamePacket::SetTime(p) => {
            let clock_updates = p
                .clock_updates
                .iter()
                .map(|(clock, state)| {
                    (
                        clock.protocol_id(),
                        state.total_ticks as i64,
                        state.partial_tick,
                        state.rate,
                    )
                })
                .collect();
            send_event(
                event_tx,
                NetworkEvent::TimeUpdate {
                    game_time: p.game_time as i64,
                    clock_updates,
                    legacy: crate::version::session_protocol()
                        < pomme_protocol::version::NATIVE.protocol,
                },
            )
            .await?;
        }
        ClientboundGamePacket::SetChunkCacheRadius(p) => {
            send_event(
                event_tx,
                NetworkEvent::ServerViewDistance { distance: p.radius },
            )
            .await?;
        }
        ClientboundGamePacket::SetSimulationDistance(p) => {
            send_event(
                event_tx,
                NetworkEvent::ServerSimulationDistance {
                    distance: p.simulation_distance,
                },
            )
            .await?;
        }
        ClientboundGamePacket::GameEvent(p) => {
            use azalea_protocol::packets::game::c_game_event::EventType;
            match p.event {
                EventType::ChangeGameMode => {
                    send_event(
                        event_tx,
                        NetworkEvent::GameModeChanged {
                            game_mode: p.param as u8,
                            previous: None,
                        },
                    )
                    .await?;
                }
                EventType::WaitForLevelChunks => {
                    send_event(event_tx, NetworkEvent::LevelChunksLoadStart).await?;
                }
                EventType::StartRaining
                | EventType::StopRaining
                | EventType::RainLevelChange
                | EventType::ThunderLevelChange => {
                    send_event(
                        event_tx,
                        NetworkEvent::WeatherUpdate {
                            event: p.event,
                            param: p.param,
                        },
                    )
                    .await?;
                }
                _ => {
                    send_event(
                        event_tx,
                        NetworkEvent::GameEvent {
                            event: p.event,
                            param: p.param,
                        },
                    )
                    .await?;
                }
            }
        }
        ClientboundGamePacket::Disconnect(p) => {
            tracing::warn!("Disconnected: {}", p.reason);
            send_event(
                event_tx,
                NetworkEvent::Disconnected {
                    reason: format!("{}", p.reason),
                },
            )
            .await?;
        }
        ClientboundGamePacket::AddEntity(p) => {
            let y_rot_deg = (p.y_rot as f32) * 360.0 / 256.0;
            let x_rot_deg = (p.x_rot as f32) * 360.0 / 256.0;
            let head_y_rot_deg = (p.y_head_rot as f32) * 360.0 / 256.0;
            let item_frame_direction = matches!(
                p.entity_type,
                EntityKind::ItemFrame | EntityKind::GlowItemFrame
            )
            .then(|| {
                use azalea_core::direction::Direction as D;
                match p.data {
                    0 => D::Down,
                    1 => D::Up,
                    2 => D::North,
                    3 => D::South,
                    4 => D::West,
                    5 => D::East,
                    _ => D::South,
                }
            });
            let mut position: Position = p.position.into();
            if let Some(direction) = item_frame_direction {
                // ItemFrame AddEntity sends the attachment BlockPos, not its center.
                position += glam::DVec3::splat(0.5)
                    - glam::DVec3::from(Position::from(direction.normal_vec3())) * 0.46875;
            }
            // Keep the center and facing in one event, including when metadata
            // omits the default South direction.
            send_event(
                event_tx,
                NetworkEvent::EntitySpawned {
                    id: p.id.0,
                    uuid: p.uuid,
                    entity_type: p.entity_type,
                    position,
                    spawn_data: p.data,
                    item_frame_direction,
                    velocity: lp_to_dvec3(&p.movement),
                    y_rot_deg,
                    x_rot_deg,
                    head_y_rot_deg,
                },
            )
            .await?;
        }
        ClientboundGamePacket::DamageEvent(p) => {
            send_event(event_tx, NetworkEvent::EntityDamaged { id: p.entity_id.0 }).await?;
        }
        ClientboundGamePacket::HurtAnimation(p) => {
            send_event(
                event_tx,
                NetworkEvent::HurtAnimation {
                    id: p.id.0,
                    yaw: p.yaw,
                },
            )
            .await?;
        }
        ClientboundGamePacket::RotateHead(p) => {
            let head_y_rot_deg = (p.y_head_rot as f32) * 360.0 / 256.0;
            send_event(
                event_tx,
                NetworkEvent::EntityHeadRotation {
                    id: p.entity_id.0,
                    head_y_rot_deg,
                },
            )
            .await?;
        }
        ClientboundGamePacket::MoveEntityPos(p) => {
            send_entity_moved(event_tx, p.entity_id.0, &p.delta, p.on_ground).await?;
        }
        ClientboundGamePacket::MoveEntityPosRot(p) => {
            use azalea_core::delta::PositionDeltaTrait;
            let look: azalea_entity::LookDirection = p.look_direction.into();
            send_event(
                event_tx,
                NetworkEvent::EntityMovedRotated {
                    id: p.entity_id.0,
                    dx: p.delta.x(),
                    dy: p.delta.y(),
                    dz: p.delta.z(),
                    y_rot_deg: look.y_rot(),
                    x_rot_deg: look.x_rot(),
                    on_ground: p.on_ground,
                },
            )
            .await?;
        }
        ClientboundGamePacket::MoveEntityRot(p) => {
            let look: azalea_entity::LookDirection = p.look_direction.into();
            send_event(
                event_tx,
                NetworkEvent::EntityRotated {
                    id: p.entity_id.0,
                    y_rot_deg: look.y_rot(),
                    x_rot_deg: look.x_rot(),
                    on_ground: p.on_ground,
                },
            )
            .await?;
        }
        ClientboundGamePacket::TeleportEntity(p) => {
            let delta = p.change.delta;
            send_event(
                event_tx,
                NetworkEvent::EntityTeleported {
                    id: p.id.0,
                    position: p.change.pos.into(),
                    relative: Some(p.relative.clone()),
                    velocity: Some(glam::DVec3::new(delta.x, delta.y, delta.z)),
                    y_rot_deg: p.change.look_direction.y_rot(),
                    x_rot_deg: p.change.look_direction.x_rot(),
                    on_ground: p.on_ground,
                },
            )
            .await?;
        }
        ClientboundGamePacket::EntityPositionSync(p) => {
            send_event(
                event_tx,
                NetworkEvent::EntityTeleported {
                    id: p.id.0,
                    position: p.values.pos.into(),
                    relative: None,
                    velocity: None,
                    y_rot_deg: p.values.look_direction.y_rot(),
                    x_rot_deg: p.values.look_direction.x_rot(),
                    on_ground: p.on_ground,
                },
            )
            .await?;
        }
        ClientboundGamePacket::SetEntityMotion(p) => {
            send_event(
                event_tx,
                NetworkEvent::EntityMotion {
                    id: p.id.0,
                    velocity: lp_to_dvec3(&p.delta),
                },
            )
            .await?;
        }
        ClientboundGamePacket::LevelEvent(p) => {
            send_event(
                event_tx,
                NetworkEvent::LevelEvent {
                    event_type: p.event_type,
                    pos: p.pos,
                    data: p.data,
                },
            )
            .await?;
        }
        ClientboundGamePacket::RemoveEntities(p) => {
            let ids: Vec<i32> = p.entity_ids.iter().map(|id| id.0).collect();
            send_event(event_tx, NetworkEvent::EntitiesRemoved { ids }).await?;
        }
        ClientboundGamePacket::SetPassengers(p) => {
            send_event(
                event_tx,
                NetworkEvent::SetPassengers {
                    vehicle: p.vehicle.0,
                    passengers: p.passengers.iter().map(|id| id.0).collect(),
                },
            )
            .await?;
        }
        ClientboundGamePacket::SetEquipment(p) => {
            let slots: Vec<_> = p
                .slots
                .slots
                .iter()
                .map(|(slot, item)| (*slot, item.clone()))
                .collect();
            send_event(
                event_tx,
                NetworkEvent::ArmorStandEquipment {
                    id: p.entity_id.0,
                    slots,
                },
            )
            .await?;
            // Mount saddle state remains on the existing living-entity route.
            for (slot, item) in &p.slots.slots {
                if *slot == azalea_inventory::components::EquipmentSlot::Saddle {
                    send_event(
                        event_tx,
                        NetworkEvent::EntitySaddle {
                            entity_id: p.entity_id.0,
                            saddled: item.is_present(),
                        },
                    )
                    .await?;
                }
            }
        }
        ClientboundGamePacket::SetEntityData(p) => {
            // Avatar's absorption/score sit at 17/18 since 1.21.9 (773);
            // 15/16 on older wire versions (main hand moved to 15, pushing
            // them up).
            let (absorption_idx, score_idx) = if crate::version::session_protocol() <= 772 {
                (15, 16)
            } else {
                (17, 18)
            };
            for item in p.packed_items.iter() {
                // ItemFrame metadata index 8 is ItemFrameDirection in 26.2.
                if item.index == 8
                    && let azalea_entity::EntityDataValue::Direction(direction) = &item.value
                {
                    send_event(
                        event_tx,
                        NetworkEvent::ItemFrameDirection {
                            id: p.id.0,
                            direction: *direction,
                        },
                    )
                    .await?;
                }
                // Pinned azalea 26.2 metadata: ItemFrame index 9 is an
                // ItemStack and index 10 is an Int rotation.
                if item.index == 9
                    && let azalea_entity::EntityDataValue::ItemStack(stack) = &item.value
                {
                    send_event(
                        event_tx,
                        NetworkEvent::ItemFrameItem {
                            id: p.id.0,
                            item: stack.clone(),
                        },
                    )
                    .await?;
                }
                if item.index == 10
                    && let azalea_entity::EntityDataValue::Int(rotation) = &item.value
                {
                    send_event(
                        event_tx,
                        NetworkEvent::ItemFrameRotation {
                            id: p.id.0,
                            rotation: *rotation,
                        },
                    )
                    .await?;
                }
                // index 8 = item stack data for item entities
                if item.index == 8
                    && let azalea_entity::EntityDataValue::ItemStack(stack) = &item.value
                {
                    let data = match stack {
                        azalea_inventory::ItemStack::Present(data) => Some(data),
                        azalea_inventory::ItemStack::Empty => None,
                    };
                    let name = data.map_or_else(String::new, |data| {
                        crate::player::inventory::item_resource_name(data.kind)
                    });
                    let item_id = data.map_or(0, |data| data.kind.to_u32());
                    let damage = data
                        .and_then(|data| {
                            crate::player::menu_click::component::<
                                azalea_inventory::components::Damage,
                            >(data)
                        })
                        .map_or(0, |component| component.amount);
                    let count = data.map_or(0, |data| data.count);
                    send_event(
                        event_tx,
                        NetworkEvent::EntityItemData {
                            id: p.id.0,
                            item_name: name,
                            item_id,
                            damage,
                            count,
                            stack: data.cloned(),
                        },
                    )
                    .await?;
                }
                // Mannequin DATA_PROFILE follows Avatar's main-arm and
                // customization metadata (indices 15 and 16).
                if item.index == 17
                    && let azalea_entity::EntityDataValue::ResolvableProfile(profile) = &item.value
                {
                    send_event(
                        event_tx,
                        NetworkEvent::MannequinProfile {
                            id: p.id.0,
                            profile: profile.clone(),
                        },
                    )
                    .await?;
                }
                // Index 6 = entity pose
                if item.index == 6
                    && let azalea_entity::EntityDataValue::Pose(pose) = &item.value
                {
                    send_event(
                        event_tx,
                        NetworkEvent::EntityPose {
                            id: p.id.0,
                            pose: crate::entity::EntityPose::from_vanilla_id(*pose as i32),
                        },
                    )
                    .await?;
                }
                // Index 14 = LivingEntity SLEEPING_POS (OptionalBlockPos).
                if item.index == 14
                    && let azalea_entity::EntityDataValue::OptionalBlockPos(pos) = &item.value
                {
                    send_event(
                        event_tx,
                        NetworkEvent::EntitySleepingPos {
                            id: p.id.0,
                            pos: *pos,
                        },
                    )
                    .await?;
                }
                if (16..=21).contains(&item.index)
                    && let azalea_entity::EntityDataValue::Rotations(rotation) = &item.value
                {
                    send_event(
                        event_tx,
                        NetworkEvent::ArmorStandData {
                            id: p.id.0,
                            index: item.index,
                            value: crate::net::ArmorStandMetaValue::Rotation([
                                rotation.x, rotation.y, rotation.z,
                            ]),
                        },
                    )
                    .await?;
                }
                let item_display_value = match (&item.value, item.index) {
                    (azalea_entity::EntityDataValue::ItemStack(stack), 23) => {
                        Some(crate::net::ItemDisplayMetaValue::Stack(stack.clone()))
                    }
                    (azalea_entity::EntityDataValue::Byte(value), 24) => {
                        Some(crate::net::ItemDisplayMetaValue::Context(*value))
                    }
                    _ => None,
                };
                if let Some(value) = item_display_value {
                    send_event(
                        event_tx,
                        NetworkEvent::ItemDisplayData {
                            id: p.id.0,
                            index: item.index,
                            value,
                        },
                    )
                    .await?;
                }
                let display_value = match &item.value {
                    azalea_entity::EntityDataValue::Int(v)
                        if matches!(item.index, 8..=10 | 16 | 22) =>
                    {
                        Some(crate::net::DisplayMetaValue::Int(*v))
                    }
                    azalea_entity::EntityDataValue::Float(v) if (17..=21).contains(&item.index) => {
                        Some(crate::net::DisplayMetaValue::Float(*v))
                    }
                    azalea_entity::EntityDataValue::Byte(v) if item.index == 15 => {
                        Some(crate::net::DisplayMetaValue::Byte(*v))
                    }
                    azalea_entity::EntityDataValue::Vector3(v) if matches!(item.index, 11 | 12) => {
                        Some(crate::net::DisplayMetaValue::Vector([v.x, v.y, v.z]))
                    }
                    azalea_entity::EntityDataValue::Quaternion(q)
                        if matches!(item.index, 13 | 14) =>
                    {
                        Some(crate::net::DisplayMetaValue::Quaternion([
                            q.x, q.y, q.z, q.w,
                        ]))
                    }
                    azalea_entity::EntityDataValue::BlockState(s) if item.index == 23 => {
                        Some(crate::net::DisplayMetaValue::BlockState(u32::from(s.id())))
                    }
                    _ => None,
                };
                if let Some(value) = display_value {
                    send_event(
                        event_tx,
                        NetworkEvent::DisplayData {
                            id: p.id.0,
                            index: item.index,
                            value,
                        },
                    )
                    .await?;
                }
                let text_display_transform = match (&item.value, item.index) {
                    (azalea_entity::EntityDataValue::Vector3(v), 11 | 12) => {
                        Some(crate::net::TextDisplayTransformValue::Vector([
                            v.x, v.y, v.z,
                        ]))
                    }
                    (azalea_entity::EntityDataValue::Quaternion(q), 13 | 14) => {
                        Some(crate::net::TextDisplayTransformValue::Quaternion([
                            q.x, q.y, q.z, q.w,
                        ]))
                    }
                    (azalea_entity::EntityDataValue::Byte(v), 15) => {
                        Some(crate::net::TextDisplayTransformValue::Billboard(*v))
                    }
                    _ => None,
                };
                if let Some(value) = text_display_transform {
                    send_event(
                        event_tx,
                        NetworkEvent::TextDisplayTransform {
                            id: p.id.0,
                            index: item.index,
                            value,
                        },
                    )
                    .await?;
                }
                // 26.2 Display.TextDisplay.DATA_TEXT_ID follows the 15
                // Display metadata fields and Entity's 8 shared fields (index 23).
                if item.index == 23
                    && let azalea_entity::EntityDataValue::FormattedText(text) = &item.value
                {
                    send_event(
                        event_tx,
                        display_text
                            .pop_front()
                            .unwrap_or_else(|| NetworkEvent::TextDisplayText {
                                id: p.id.0,
                                text: format_text_spans(text, [1.0; 4]),
                            }),
                    )
                    .await?;
                }
                // 26.2 Avatar.DATA_PLAYER_MAIN_HAND is the HumanoidArm at
                // index 15; legacy protocols are normalized by translate.rs.
                if item.index == 15
                    && let azalea_entity::EntityDataValue::HumanoidArm(arm) = &item.value
                {
                    send_event(
                        event_tx,
                        NetworkEvent::EntityMainArm {
                            id: p.id.0,
                            right: *arm == azalea_entity::HumanoidArm::Right,
                        },
                    )
                    .await?;
                }
                if item.index == 8
                    && let azalea_entity::EntityDataValue::ItemStack(
                        azalea_inventory::ItemStack::Present(stack),
                    ) = &item.value
                {
                    send_event(
                        event_tx,
                        NetworkEvent::EntityProjectileItem {
                            id: p.id.0,
                            stack: stack.clone(),
                        },
                    )
                    .await?;
                }
                if item.index == 10
                    && let azalea_entity::EntityDataValue::Particles(particles) = &item.value
                {
                    send_event(
                        event_tx,
                        NetworkEvent::EntityEffectParticles {
                            id: p.id.0,
                            particles: Some(particles.to_vec()),
                            ambient: None,
                        },
                    )
                    .await?;
                }
                if item.index == 11
                    && let azalea_entity::EntityDataValue::Boolean(ambient) = &item.value
                {
                    send_event(
                        event_tx,
                        NetworkEvent::EntityEffectParticles {
                            id: p.id.0,
                            particles: None,
                            ambient: Some(*ambient),
                        },
                    )
                    .await?;
                }
                // Scalar values are forwarded raw; the store resolves their
                // meaning per (kind, index) like vanilla `onSyncedDataUpdated`
                // (`EntityStore::apply_entity_data`).
                let scalar = match &item.value {
                    azalea_entity::EntityDataValue::Boolean(v) => Some(MetaValue::Bool(*v)),
                    azalea_entity::EntityDataValue::Int(v) => Some(MetaValue::Int(*v)),
                    azalea_entity::EntityDataValue::Byte(v) => Some(MetaValue::Byte(*v)),
                    azalea_entity::EntityDataValue::Float(v) => Some(MetaValue::Float(*v)),
                    azalea_entity::EntityDataValue::Long(v) => Some(MetaValue::Long(*v)),
                    azalea_entity::EntityDataValue::BlockState(state) => {
                        Some(MetaValue::BlockState(u32::from(state.id())))
                    }
                    azalea_entity::EntityDataValue::OptionalBlockState(state) => {
                        Some(MetaValue::OptionalBlockState(
                            (!state.is_air()).then(|| u32::from(state.id())),
                        ))
                    }
                    azalea_entity::EntityDataValue::OptionalBlockPos(pos) => {
                        Some(MetaValue::OptionalBlockPos(*pos))
                    }
                    azalea_entity::EntityDataValue::BlockPos(pos) => {
                        Some(MetaValue::BlockPos(*pos))
                    }
                    // Serializer 16 on Shulker only. The consumer gates this
                    // semantic value by (kind, index); other index-16 fields
                    // remain their original Bool/Byte/Int variants.
                    azalea_entity::EntityDataValue::Direction(direction) => {
                        Some(MetaValue::Direction(*direction))
                    }
                    _ => None,
                };
                if let Some(value) = scalar {
                    send_event(
                        event_tx,
                        NetworkEvent::EntityData {
                            id: p.id.0,
                            index: item.index,
                            value,
                        },
                    )
                    .await?;
                }
                // Player score (Int; index gated per wire version above).
                // Kind-blind; the consumer applies it only to the local
                // player.
                if item.index == score_idx
                    && let azalea_entity::EntityDataValue::Int(score) = &item.value
                {
                    send_event(
                        event_tx,
                        NetworkEvent::PlayerScore {
                            entity_id: p.id.0,
                            score: *score,
                        },
                    )
                    .await?;
                }
                // Player absorption (Float, Player.DATA_PLAYER_ABSORPTION_ID;
                // index gated per wire version above). Kind-blind; the
                // consumer applies it only to the local player.
                if item.index == absorption_idx
                    && let azalea_entity::EntityDataValue::Float(absorption) = &item.value
                {
                    send_event(
                        event_tx,
                        NetworkEvent::PlayerAbsorption {
                            entity_id: p.id.0,
                            absorption: *absorption,
                        },
                    )
                    .await?;
                }
                // Index 2 = custom name (Optional<Component>); needed for jeb_ sheep detection.
                if item.index == 2
                    && let azalea_entity::EntityDataValue::OptionalFormattedText(opt) = &item.value
                {
                    let name = opt.as_ref().map(|c| c.to_string());
                    send_event(
                        event_tx,
                        NetworkEvent::EntityCustomName { id: p.id.0, name },
                    )
                    .await?;
                }
                // Index 18 on cows = CowVariant Holder.
                if item.index == 18
                    && let azalea_entity::EntityDataValue::CowVariant(variant) = &item.value
                {
                    send_event(
                        event_tx,
                        variant_event(registry_holder, p.id.0, EntityKind::Cow, variant),
                    )
                    .await?;
                }
                // Index 18 on chickens = ChickenVariant Holder.
                if item.index == 18
                    && let azalea_entity::EntityDataValue::ChickenVariant(variant) = &item.value
                {
                    send_event(
                        event_tx,
                        variant_event(registry_holder, p.id.0, EntityKind::Chicken, variant),
                    )
                    .await?;
                }
                // Pig DATA_VARIANT_ID is index 19 in 26.2: pig boost time
                // occupies 18, then the PigVariant holder uses serializer 28.
                if item.index == 19
                    && let azalea_entity::EntityDataValue::PigVariant(variant) = &item.value
                {
                    send_event(
                        event_tx,
                        variant_event(registry_holder, p.id.0, EntityKind::Pig, variant),
                    )
                    .await?;
                }
                // Cat / wolf variant Holders: 20 / 23 on 26.x, one lower on
                // 1.21.9-1.21.11 (no AgeableMob age-locked slot).
                if (item.index == 19 || item.index == 20)
                    && let azalea_entity::EntityDataValue::CatVariant(variant) = &item.value
                {
                    send_event(
                        event_tx,
                        variant_event(registry_holder, p.id.0, EntityKind::Cat, variant),
                    )
                    .await?;
                }
                if (item.index == 22 || item.index == 23)
                    && let azalea_entity::EntityDataValue::WolfVariant(variant) = &item.value
                {
                    send_event(
                        event_tx,
                        variant_event(registry_holder, p.id.0, EntityKind::Wolf, variant),
                    )
                    .await?;
                }
                // VillagerData (type/profession/level): villagers at 19 (18
                // on 1.21.9-1.21.11), zombie villagers at 20.
                if (18..=20).contains(&item.index)
                    && let azalea_entity::EntityDataValue::VillagerData(data) = &item.value
                {
                    send_event(
                        event_tx,
                        NetworkEvent::VillagerData {
                            id: p.id.0,
                            kind: data.kind.into(),
                            profession: data.profession.into(),
                            level: data.level,
                        },
                    )
                    .await?;
                }
            }
        }
        // Event id 3 = living entity death or snowball impact.
        // TODO: event 60 (`makePoofParticles`) when a mob's death clock hits 20.
        ClientboundGamePacket::EntityEvent(p) if p.event_id == 3 => {
            send_event(event_tx, NetworkEvent::EntityDied { id: p.entity_id.0 }).await?;
        }
        // Event id 9 = finished using an item (vanilla `completeUsingItem`).
        ClientboundGamePacket::EntityEvent(p) if p.event_id == 9 => {
            send_event(event_tx, NetworkEvent::FinishUseItem { id: p.entity_id.0 }).await?;
        }
        // Event id 10 = sheep eat-grass animation start (40-tick head-dip).
        ClientboundGamePacket::EntityEvent(p) if p.event_id == 10 => {
            send_event(event_tx, NetworkEvent::SheepEatStart { id: p.entity_id.0 }).await?;
        }
        // Event id 1 = rabbit jump start (15-tick hop).
        ClientboundGamePacket::EntityEvent(p) if p.event_id == 1 => {
            send_event(event_tx, NetworkEvent::RabbitJump { id: p.entity_id.0 }).await?;
        }
        // Event id 19 = squid tentacle-clock rollover.
        ClientboundGamePacket::EntityEvent(p) if p.event_id == 19 => {
            send_event(
                event_tx,
                NetworkEvent::SquidTentacleReset { id: p.entity_id.0 },
            )
            .await?;
        }
        // Event id 4 = iron golem punch (10-tick swing).
        ClientboundGamePacket::EntityEvent(p) if p.event_id == 4 => {
            send_event(event_tx, NetworkEvent::GolemPunch { id: p.entity_id.0 }).await?;
        }
        // Event id 35 = Totem activation, emitted by the entity that used it.
        ClientboundGamePacket::EntityEvent(p) if p.event_id == 35 => {
            send_event(
                event_tx,
                NetworkEvent::TotemUsed {
                    entity_id: p.entity_id.0,
                },
            )
            .await?;
        }
        // Events 11 / 34 = iron golem flower offer start / stop.
        ClientboundGamePacket::EntityEvent(p) if p.event_id == 11 => {
            send_event(
                event_tx,
                NetworkEvent::GolemOfferFlower {
                    id: p.entity_id.0,
                    offering: true,
                },
            )
            .await?;
        }
        ClientboundGamePacket::EntityEvent(p) if p.event_id == 34 => {
            send_event(
                event_tx,
                NetworkEvent::GolemOfferFlower {
                    id: p.entity_id.0,
                    offering: false,
                },
            )
            .await?;
        }
        // Events 8 / 56 = wolf wet-shake start / cancel.
        ClientboundGamePacket::EntityEvent(p) if p.event_id == 8 => {
            send_event(
                event_tx,
                NetworkEvent::WolfShaking {
                    id: p.entity_id.0,
                    shaking: true,
                },
            )
            .await?;
        }
        ClientboundGamePacket::EntityEvent(p) if p.event_id == 56 => {
            send_event(
                event_tx,
                NetworkEvent::WolfShaking {
                    id: p.entity_id.0,
                    shaking: false,
                },
            )
            .await?;
        }
        // Arm-swing animation drives the zombie attack swing (skeleton aim uses the
        // aggressive flag instead). Both hands trigger the same swing timer.
        ClientboundGamePacket::Animate(p)
            if matches!(
                p.action,
                azalea_protocol::packets::game::c_animate::AnimationAction::SwingMainHand
                    | azalea_protocol::packets::game::c_animate::AnimationAction::SwingOffHand
            ) =>
        {
            send_event(event_tx, NetworkEvent::EntitySwing { id: p.id.0 }).await?;
        }
        // Critical-hit particle emitters (vanilla Animate actions 4/5).
        ClientboundGamePacket::Animate(p)
            if matches!(
                p.action,
                azalea_protocol::packets::game::c_animate::AnimationAction::CriticalHit
                    | azalea_protocol::packets::game::c_animate::AnimationAction::MagicCriticalHit
            ) =>
        {
            let kind = if p.action
                == azalea_protocol::packets::game::c_animate::AnimationAction::CriticalHit
            {
                crate::net::CriticalHitKind::Critical
            } else {
                crate::net::CriticalHitKind::Enchanted
            };
            send_event(event_tx, NetworkEvent::CriticalHit { id: p.id.0, kind }).await?;
        }
        // Vanilla handleAnimate action 2 -> stopSleepInBed(false, false).
        ClientboundGamePacket::Animate(p)
            if matches!(
                p.action,
                azalea_protocol::packets::game::c_animate::AnimationAction::WakeUp
            ) =>
        {
            send_event(event_tx, NetworkEvent::EntityWakeUp { id: p.id.0 }).await?;
        }
        ClientboundGamePacket::TakeItemEntity(p) => {
            send_event(
                event_tx,
                NetworkEvent::ItemPickedUp {
                    item_id: p.item_id as i32,
                    collector_id: p.player_id.0,
                    amount: p.amount as i32,
                },
            )
            .await?;
        }
        ClientboundGamePacket::Respawn(p) => {
            send_event(
                event_tx,
                NetworkEvent::PlayerRespawned {
                    keep_entity_data: p.data_to_keep & 2 != 0,
                    keep_attribute_modifiers: p.data_to_keep & 1 != 0,
                },
            )
            .await?;
            if let Some((_, dim)) = p.common.dimension_type(registry_holder) {
                *current_dimension = (dim.height, dim.min_y);
                send_event(
                    event_tx,
                    dimension_info(
                        dim,
                        p.common.is_debug,
                        dimension_clock_id(world_clock_ids, dim),
                        &p.common.dimension.to_string(),
                        timeline_entries,
                        timeline_entries_error,
                        world_clock_ids,
                    ),
                )
                .await?;
            }
            send_event(
                event_tx,
                NetworkEvent::DimensionName {
                    name: p.common.dimension.to_string(),
                },
            )
            .await?;
            send_event(
                event_tx,
                NetworkEvent::GameModeChanged {
                    game_mode: p.common.game_type as u8,
                    previous: Some(p.common.previous_game_type.0.map(|m| m.to_id())),
                },
            )
            .await?;
            // Vanilla recreates the player on respawn; the server re-sends any
            // effects kept across it.
            send_event(event_tx, NetworkEvent::ClearMobEffects).await?;
        }
        ClientboundGamePacket::PlayerCombatKill(p) => {
            tracing::info!("Player died: {}", p.message);
            send_event(
                event_tx,
                NetworkEvent::PlayerDied {
                    player_id: p.player_id.0,
                    message: p.message.to_string(),
                },
            )
            .await?;
        }
        ClientboundGamePacket::ResourcePackPush(p) => {
            tracing::info!(
                "Server pushing resource pack {} (required: {})",
                p.id,
                p.required
            );
            send_event(
                event_tx,
                NetworkEvent::ResourcePackPush {
                    id: p.id,
                    url: p.url.clone(),
                    hash: p.hash.clone(),
                    required: p.required,
                    prompt: p.prompt.clone(),
                },
            )
            .await?;
        }
        ClientboundGamePacket::ResourcePackPop(p) => {
            tracing::info!("Server popping resource pack {:?}", p.id);
            send_event(event_tx, NetworkEvent::ResourcePackPop { id: p.id }).await?;
        }
        ClientboundGamePacket::PlayerInfoUpdate(p) => {
            use crate::player::tab_list::{PlayerInfoActions, PlayerInfoEntry};
            tracing::info!(
                target = "renderprobe",
                packet = "PlayerInfoUpdate",
                entries = p.entries.len(),
                addPlayer = p.actions.add_player,
                updateGameMode = p.actions.update_game_mode,
                "player info packet reached client tab-list pipeline"
            );
            let actions = PlayerInfoActions {
                add_player: p.actions.add_player,
                initialize_chat: p.actions.initialize_chat,
                update_game_mode: p.actions.update_game_mode,
                update_listed: p.actions.update_listed,
                update_latency: p.actions.update_latency,
                update_display_name: p.actions.update_display_name,
                update_list_order: p.actions.update_list_order,
                update_hat: p.actions.update_hat,
            };
            let entries = p
                .entries
                .iter()
                .map(|e| PlayerInfoEntry {
                    uuid: e.profile.uuid,
                    name: e.profile.name.clone(),
                    textures: e
                        .profile
                        .properties
                        .map
                        .get("textures")
                        .map(|p| p.value.clone()),
                    game_mode: e.game_mode.to_id(),
                    listed: e.listed,
                    latency: e.latency,
                    display_name: e
                        .display_name
                        .as_ref()
                        .map(|c| crate::ui::text::format_text_spans(c, [1.0, 1.0, 1.0, 1.0])),
                    list_order: e.list_order,
                    show_hat: e.update_hat,
                    chat_session: if p.actions.initialize_chat {
                        match (ProfileKeyServices::get(), e.chat_session.as_ref()) {
                            (Some(services), Some(session)) => match services
                                .validate_session(e.profile.uuid, session)
                            {
                                Ok(session) => Some(session),
                                Err(error) => {
                                    tracing::error!(
                                        player = %e.profile.name,
                                        "Failed to validate profile key: {error}"
                                    );
                                    None
                                }
                            },
                            (None, Some(_)) => {
                                tracing::warn!(
                                    player = %e.profile.name,
                                    "Ignoring chat session due to missing Mojang Services public key"
                                );
                                None
                            }
                            (_, None) => None,
                        }
                    } else {
                        None
                    },
                })
                .collect();
            send_event(
                event_tx,
                NetworkEvent::PlayerInfoUpdate { actions, entries },
            )
            .await?;
        }
        ClientboundGamePacket::PlayerInfoRemove(p) => {
            send_event(
                event_tx,
                NetworkEvent::PlayerInfoRemove {
                    uuids: p.profile_ids.clone(),
                },
            )
            .await?;
        }
        ClientboundGamePacket::TabList(p) => {
            send_event(
                event_tx,
                NetworkEvent::TabListHeaderFooter {
                    header: crate::ui::text::format_text_spans(&p.header, [1.0, 1.0, 1.0, 1.0]),
                    footer: crate::ui::text::format_text_spans(&p.footer, [1.0, 1.0, 1.0, 1.0]),
                },
            )
            .await?;
        }
        ClientboundGamePacket::Commands(p) => {
            let tree = std::sync::Arc::new(CommandTree::from_packet(p));
            tracing::info!(
                "Command tree received: {} nodes, root commands = {:?}",
                p.entries.len(),
                tree.root_child_names()
            );
            *shared_tree.lock() = Some(tree.clone());
            send_event(event_tx, NetworkEvent::CommandTree { tree }).await?;
        }
        ClientboundGamePacket::ShowDialog(p) => {
            send_event(
                event_tx,
                NetworkEvent::ShowDialog {
                    dialog: dialog_holder_reference(&p.dialog),
                },
            )
            .await?;
        }
        ClientboundGamePacket::ClearDialog(_) => {
            send_event(event_tx, NetworkEvent::ClearDialog).await?;
        }
        ClientboundGamePacket::CustomChatCompletions(p) => {
            let action = match p.action {
                azalea_protocol::packets::game::c_custom_chat_completions::Action::Add => {
                    super::CustomChatCompletionsAction::Add
                }
                azalea_protocol::packets::game::c_custom_chat_completions::Action::Remove => {
                    super::CustomChatCompletionsAction::Remove
                }
                azalea_protocol::packets::game::c_custom_chat_completions::Action::Set => {
                    super::CustomChatCompletionsAction::Set
                }
            };
            send_event(
                event_tx,
                NetworkEvent::CustomChatCompletions {
                    action,
                    entries: p.entries.clone(),
                },
            )
            .await?;
        }
        _other => {}
    }
    Ok(())
}

async fn send_scoreboard_team(
    event_tx: &Sender<NetworkEvent>,
    name: &str,
    parameters: &azalea_protocol::packets::game::c_set_player_team::Parameters,
    members: Option<Vec<String>>,
) -> Result<(), SendError<NetworkEvent>> {
    let color = team_color(parameters.color);
    let sidebar_slot = team_sidebar_slot(parameters.color);
    send_event(
        event_tx,
        NetworkEvent::ScoreboardTeam {
            name: name.into(),
            display_name: format_text_spans(&parameters.display_name, [1.0; 4]),
            prefix: format_text_spans(&parameters.player_prefix, color),
            suffix: format_text_spans(&parameters.player_suffix, color),
            color,
            fill_color: parameters.color.color().map(crate::ui::common::rgb),
            sidebar_slot,
            nametag_visibility: parameters.nametag_visibility,
            collision_rule: parameters.collision_rule,
            friendly_fire: parameters.options & 0x01 != 0,
            see_friendly_invisibles: parameters.options & 0x02 != 0,
            members,
        },
    )
    .await?;
    Ok(())
}

fn score_number_format(
    format: &azalea_chat::numbers::NumberFormat,
) -> Result<crate::ui::hud::ScoreNumberFormat, String> {
    use azalea_chat::numbers::NumberFormat;

    use crate::ui::hud::ScoreNumberFormat as F;
    match format {
        NumberFormat::Blank => Ok(F::Blank),
        NumberFormat::Styled { style } => {
            if style.is_none() {
                return Err("Styled number format has no NBT style compound".into());
            }
            let tag: simdnbt::owned::NbtTag = style.clone().into();
            crate::chat_component::Style::from_nbt_tag(&tag)
                .map(F::Styled)
                .map_err(|error| format!("invalid Styled number format style: {error}"))
        }
        NumberFormat::Fixed { value } => Ok(F::Fixed(format_text_spans(value, [1.0; 4]))),
    }
}

/// azalea's `SetObjective` reads the number format without vanilla's
/// `Optional` bool, so the decode lands shifted: vanilla `None` (the common
/// case) arrives as `Blank`, vanilla `Blank` arrives as `Styled` with empty
/// NBT, and real styled/fixed formats fail to decode the whole packet. Undo
/// the shift for the two recoverable cases.
fn objective_number_format(
    format: &azalea_chat::numbers::NumberFormat,
) -> Result<Option<crate::ui::hud::ScoreNumberFormat>, String> {
    use azalea_chat::numbers::NumberFormat;
    match format {
        NumberFormat::Blank => Ok(None),
        // Azalea's typed objective decoder shifts vanilla Blank into this
        // empty Styled value; preserve the established recovery for that path.
        NumberFormat::Styled { style } if style.is_none() => {
            Ok(Some(crate::ui::hud::ScoreNumberFormat::Blank))
        }
        other => score_number_format(other).map(Some),
    }
}

fn team_sidebar_slot(
    color: azalea_chat::style::ChatFormatting,
) -> Option<azalea_protocol::packets::game::c_set_display_objective::DisplaySlot> {
    use azalea_chat::style::ChatFormatting as C;
    use azalea_protocol::packets::game::c_set_display_objective::DisplaySlot as S;
    Some(match color {
        C::Black => S::TeamBlack,
        C::DarkBlue => S::TeamDarkBlue,
        C::DarkGreen => S::TeamDarkGreen,
        C::DarkAqua => S::TeamDarkAqua,
        C::DarkRed => S::TeamDarkRed,
        C::DarkPurple => S::TeamDarkPurple,
        C::Gold => S::TeamGold,
        C::Gray => S::TeamGray,
        C::DarkGray => S::TeamDarkGray,
        C::Blue => S::TeamBlue,
        C::Green => S::TeamGreen,
        C::Aqua => S::TeamAqua,
        C::Red => S::TeamRed,
        C::LightPurple => S::TeamLightPurple,
        C::Yellow => S::TeamYellow,
        C::White => S::TeamWhite,
        _ => return None,
    })
}

fn team_color(color: azalea_chat::style::ChatFormatting) -> [f32; 4] {
    crate::ui::common::rgb(color.color().unwrap_or(0xffffff))
}

/// Resolves a variant registry holder id to the mob's renderer pool slot.
/// Matches vanilla, which reads the entry's synced NBT and never the registry
/// id: a known `asset_id` picks the exact slot, else the `model` field picks
/// the mesh (its values name slots; absent means "normal" = slot 0), else the
/// registry path as a last resort for entries synced without NBT.
fn variant_index(registry_holder: &RegistryHolder, kind: EntityKind, protocol_id: u32) -> u32 {
    // (registry, pool order, asset prefix, vanilla's default texture variant).
    let (registry, order, asset_prefix, default) = match kind {
        EntityKind::Cow => (
            "minecraft:cow_variant",
            COW_VARIANT_ORDER,
            "entity/cow/cow_",
            "temperate",
        ),
        EntityKind::Chicken => (
            "minecraft:chicken_variant",
            CHICKEN_VARIANT_ORDER,
            "entity/chicken/chicken_",
            "temperate",
        ),
        EntityKind::Pig => (
            "minecraft:pig_variant",
            crate::renderer::pipelines::entity_renderer::PIG_VARIANT_ORDER,
            "entity/pig/pig_",
            "temperate",
        ),
        EntityKind::Cat => (
            "minecraft:cat_variant",
            CAT_VARIANT_ORDER,
            "entity/cat/cat_",
            "tabby",
        ),
        // TODO: wolf NBT nests its textures (assets.wild, no flat
        // asset_id/model), so datapack wolves only resolve by registry path.
        EntityKind::Wolf => (
            "minecraft:wolf_variant",
            WOLF_VARIANT_ORDER,
            "entity/wolf/wolf_",
            "pale",
        ),
        _ => return 0,
    };
    let order_pos = |name: &str| order.iter().position(|p| *p == name).map(|i| i as u32);
    let fallback = order_pos(default).unwrap_or(0);
    // Position == protocol id only holds while every entry carries NBT
    // (azalea shift_removes NBT-less ones). Entries the server skips for a
    // pack pomme claimed are filled in first (`net::known_packs`).
    let Some((ident, nbt)) = registry_holder
        .extra
        .get(&azalea_registry::identifier::Identifier::new(registry))
        .and_then(|r| r.map.get_index(protocol_id as usize))
    else {
        return fallback;
    };
    if let Some(asset) = nbt.string("asset_id").map(|s| s.to_str())
        && let Some(suffix) = asset
            .strip_prefix("minecraft:")
            .unwrap_or(&asset)
            .strip_prefix(asset_prefix)
        && let Some(i) = order_pos(suffix)
    {
        return i;
    }
    if let Some(model) = nbt.string("model").map(|s| s.to_str())
        && let Some(i) = order_pos(&model)
    {
        return i;
    }
    order_pos(ident.path()).unwrap_or(fallback)
}

/// The kind-tagged variant event for a synced-registry holder value.
fn variant_event(
    registry_holder: &RegistryHolder,
    id: i32,
    kind: EntityKind,
    holder: &impl azalea_registry::DataRegistry,
) -> NetworkEvent {
    NetworkEvent::EntityVariant {
        id,
        kind,
        variant: variant_index(registry_holder, kind, holder.protocol_id()),
    }
}

fn lp_to_dvec3(v: &azalea_core::delta::LpVec3) -> glam::DVec3 {
    let v = v.to_vec3();
    glam::DVec3::new(v.x, v.y, v.z)
}

async fn send_entity_moved(
    event_tx: &Sender<NetworkEvent>,
    id: i32,
    delta: &azalea_core::delta::PositionDelta8,
    on_ground: bool,
) -> Result<(), SendError<NetworkEvent>> {
    send_event(
        event_tx,
        NetworkEvent::EntityMoved {
            id,
            dx: delta.xa as f64 / 4096.0,
            dy: delta.ya as f64 / 4096.0,
            dz: delta.za as f64 / 4096.0,
            on_ground,
        },
    )
    .await?;
    Ok(())
}

/// Consume packets that azalea's 26.2 codecs cannot represent correctly
/// before the typed decode runs. Returns whether the packet was consumed.
pub async fn handle_raw_game_packet(
    raw: &[u8],
    event_tx: &Sender<NetworkEvent>,
) -> Result<bool, SendError<NetworkEvent>> {
    let mut cur = std::io::Cursor::new(raw);
    let Ok(packet_id) = u32::azalea_read_var(&mut cur) else {
        return Ok(false);
    };
    let name = pomme_protocol::PacketTable::native().name_of(
        pomme_protocol::Phase::Game,
        pomme_protocol::Direction::Clientbound,
        packet_id,
    );
    let event: Result<Option<NetworkEvent>, String> = match name {
        Some("open_sign_editor") => (|| {
            let pos = BlockPos::azalea_read(&mut cur)?;
            let is_front_text = bool::azalea_read(&mut cur)?;
            Ok::<_, azalea_buf::BufReadError>(Some(NetworkEvent::OpenSignEditor {
                pos,
                is_front_text,
            }))
        })()
        .map_err(|e| e.to_string()),
        Some("cooldown") => super::cooldown::decode_payload(&raw[cur.position() as usize..])
            .map(|cooldown| {
                Some(NetworkEvent::ItemCooldown {
                    group: cooldown.group,
                    duration: cooldown.duration,
                })
            })
            .map_err(|e| e.to_string()),
        Some("set_objective") => parse_set_objective(&mut cur).map(Some),
        Some("set_score") if raw.windows(2).any(|bytes| bytes == [0xc2, 0xa7]) => {
            parse_legacy_score(&mut cur).map(Some)
        }
        Some("set_player_team") if raw.windows(2).any(|bytes| bytes == [0xc2, 0xa7]) => {
            // Parameters already use Azalea's layout after native color normalization.
            let method_pos = {
                let mut header = cur.clone();
                read_raw_string(&mut header, 32767)
                    .ok()
                    .map(|_| header.position() as usize)
            };
            if !method_pos
                .and_then(|pos| raw.get(pos))
                .is_some_and(|method| matches!(method, 0 | 2))
            {
                return Ok(false);
            }
            parse_legacy_team(&mut cur).map(Some)
        }
        Some("level_particles") => parse_level_particles(&mut cur).map_err(|e| e.to_string()),
        Some("sound" | "sound_entity" | "stop_sound") => {
            let result = match name {
                Some("sound") => handle_raw_ui_sound(&mut cur),
                Some("sound_entity") => handle_raw_ui_entity_sound(&mut cur),
                _ => handle_raw_ui_stop_sound(&mut cur),
            };
            match result {
                Ok((false, _)) => return Ok(false),
                Ok((true, event)) => Ok(event),
                Err(e) => Err(e.to_string()),
            }
        }
        _ => return Ok(false),
    };
    match event {
        Ok(Some(event)) => {
            if matches!(
                name,
                Some("set_objective" | "set_score" | "set_player_team")
            ) && cur.position() as usize != raw.len()
            {
                tracing::warn!(packet = ?name, "Skipping raw scoreboard packet with trailing bytes");
            } else {
                send_event(event_tx, event).await?;
            }
        }
        Ok(None) => {}
        Err(error) => tracing::warn!(packet = ?name, %error, "Skipping malformed raw game packet"),
    }
    Ok(true)
}

/// Azalea's pinned 26.2 `SoundSource` omits Vanilla's ordinal-10 `UI` value
/// and decodes unknown ordinals as `Master`. Read just that valid ordinal here;
/// ordinals 0..=9 fall through to Azalea's normal typed decoder.
fn handle_raw_ui_sound(
    cur: &mut std::io::Cursor<&[u8]>,
) -> Result<(bool, Option<NetworkEvent>), azalea_buf::BufReadError> {
    let mut sound = Holder::<SoundEvent, CustomSound>::azalea_read(cur)?;
    if u32::azalea_read_var(cur)? != UI_SOUND_SOURCE {
        return Ok((false, None));
    }
    if let Some(translation) = super::translate::active()
        && !translation.remap_sound(&mut sound)
    {
        return Ok((true, None));
    }
    let x = i32::azalea_read(cur)?;
    let y = i32::azalea_read(cur)?;
    let z = i32::azalea_read(cur)?;
    let volume = f32::azalea_read(cur)?;
    let pitch = f32::azalea_read(cur)?;
    let seed = u64::azalea_read(cur)?;
    Ok((
        true,
        Some(NetworkEvent::PlaySound {
            sound: crate::audio::SoundRef::resolve(&sound),
            category: UI_SOUND_SOURCE as u8,
            pos: Position::new(x as f64 / 8.0, y as f64 / 8.0, z as f64 / 8.0),
            volume,
            pitch,
            seed,
        }),
    ))
}

fn handle_raw_ui_entity_sound(
    cur: &mut std::io::Cursor<&[u8]>,
) -> Result<(bool, Option<NetworkEvent>), azalea_buf::BufReadError> {
    let mut sound = Holder::<SoundEvent, CustomSound>::azalea_read(cur)?;
    if u32::azalea_read_var(cur)? != UI_SOUND_SOURCE {
        return Ok((false, None));
    }
    if let Some(translation) = super::translate::active()
        && !translation.remap_sound(&mut sound)
    {
        return Ok((true, None));
    }
    let entity_id = i32::azalea_read_var(cur)?;
    let volume = f32::azalea_read(cur)?;
    let pitch = f32::azalea_read(cur)?;
    let seed = u64::azalea_read(cur)?;
    Ok((
        true,
        Some(NetworkEvent::PlayEntitySound {
            sound: crate::audio::SoundRef::resolve(&sound),
            category: UI_SOUND_SOURCE as u8,
            entity_id,
            volume,
            pitch,
            seed,
        }),
    ))
}

fn handle_raw_ui_stop_sound(
    cur: &mut std::io::Cursor<&[u8]>,
) -> Result<(bool, Option<NetworkEvent>), azalea_buf::BufReadError> {
    let set = FixedBitSet::<2>::azalea_read(cur)?;
    if !set.index(0) || u32::azalea_read_var(cur)? != UI_SOUND_SOURCE {
        return Ok((false, None));
    }
    let name = if set.index(1) {
        Some(Identifier::azalea_read(cur)?.to_string())
    } else {
        None
    };
    Ok((
        true,
        Some(NetworkEvent::StopSound {
            sound_id: name,
            category: Some(UI_SOUND_SOURCE as u8),
        }),
    ))
}

const UI_SOUND_SOURCE: u32 = 10;

#[cfg(test)]
#[derive(Clone, Copy)]
struct SoundPacketIds {
    sound: u32,
    sound_entity: u32,
    stop_sound: u32,
}

#[cfg(test)]
fn sound_packet_ids() -> SoundPacketIds {
    use pomme_protocol::{Direction, PacketTable, Phase};

    static IDS: std::sync::OnceLock<SoundPacketIds> = std::sync::OnceLock::new();
    *IDS.get_or_init(|| {
        let table = PacketTable::native();
        SoundPacketIds {
            sound: table
                .id(Phase::Game, Direction::Clientbound, "sound")
                .expect("sound in packet table"),
            sound_entity: table
                .id(Phase::Game, Direction::Clientbound, "sound_entity")
                .expect("sound_entity in packet table"),
            stop_sound: table
                .id(Phase::Game, Direction::Clientbound, "stop_sound")
                .expect("stop_sound in packet table"),
        }
    })
}

/// The wire layout of vanilla `ClientboundLevelParticlesPacket.write`, up to
/// the particle type id.
fn parse_level_particles(
    cur: &mut std::io::Cursor<&[u8]>,
) -> Result<Option<NetworkEvent>, azalea_buf::BufReadError> {
    let override_limiter = bool::azalea_read(cur)?;
    let always_show = bool::azalea_read(cur)?;
    let pos = glam::dvec3(
        f64::azalea_read(cur)?,
        f64::azalea_read(cur)?,
        f64::azalea_read(cur)?,
    );
    let x_dist = f32::azalea_read(cur)?;
    let y_dist = f32::azalea_read(cur)?;
    let z_dist = f32::azalea_read(cur)?;
    let max_speed = f32::azalea_read(cur)?;
    // Signed on the wire; Java's `i < count` loop no-ops on negative counts.
    let count = i32::azalea_read(cur)?;
    let type_id = u32::azalea_read_var(cur)?;
    // Particle ids shift between versions; translate into the native id
    // space (`ServerParticleKind`'s) when speaking an older protocol.
    let type_id = match super::translate::active() {
        Some(t) => match t.remap_particle(type_id) {
            Some(id) => id,
            None => return Ok(None),
        },
        None => type_id,
    };
    let Some(kind) = crate::particle::ServerParticleKind::from_id(type_id) else {
        // The packet is already length-framed by the transport. Do not guess
        // this type's payload size or reinterpret payload bytes as simple.
        return Ok(None);
    };
    let options = match kind {
        crate::particle::ServerParticleKind::EntityEffect => {
            crate::particle::ServerParticleOptions::EntityEffect {
                color: i32::azalea_read(cur)? as u32,
            }
        }
        crate::particle::ServerParticleKind::Dust => crate::particle::ServerParticleOptions::Dust {
            packed_color: i32::azalea_read(cur)?,
            scale: f32::azalea_read(cur)?,
        },
        crate::particle::ServerParticleKind::Block => {
            let id = u32::azalea_read_var(cur)?;
            let Some(state) = crate::world::block::try_state(id) else {
                return Ok(None);
            };
            crate::particle::ServerParticleOptions::Block(state)
        }
        crate::particle::ServerParticleKind::Item => {
            let (item_id, count) = if super::translate::active().is_some() {
                // Translated payloads use Azalea ItemStack's count/id ordering.
                let count = i32::azalea_read_var(cur)?;
                (u32::azalea_read_var(cur)?, count)
            } else {
                // 26.2 ItemStackTemplate writes item id, then count.
                let item_id = u32::azalea_read_var(cur)?;
                (item_id, i32::azalea_read_var(cur)?)
            };
            let components = azalea_inventory::DataComponentPatch::azalea_read(cur)?;
            crate::particle::ServerParticleOptions::Item {
                item_id,
                count,
                components,
            }
        }
        crate::particle::ServerParticleKind::Shriek => {
            crate::particle::ServerParticleOptions::Shriek {
                delay: i32::azalea_read_var(cur)?,
            }
        }
        crate::particle::ServerParticleKind::Trail => {
            crate::particle::ServerParticleOptions::Trail {
                target: glam::dvec3(
                    f64::azalea_read(cur)?,
                    f64::azalea_read(cur)?,
                    f64::azalea_read(cur)?,
                ),
                color: i32::azalea_read(cur)?,
                duration: i32::azalea_read_var(cur)?,
            }
        }
        crate::particle::ServerParticleKind::Vibration => {
            let source_type = u32::azalea_read_var(cur)?;
            match source_type {
                0 => crate::particle::ServerParticleOptions::VibrationBlock {
                    target: BlockPos::azalea_read(cur)?,
                    arrival_ticks: i32::azalea_read_var(cur)?,
                },
                1 => crate::particle::ServerParticleOptions::VibrationEntity {
                    entity_id: i32::azalea_read_var(cur)?,
                    y_offset: f32::azalea_read(cur)?,
                    arrival_ticks: i32::azalea_read_var(cur)?,
                },
                _ => return Ok(None),
            }
        }
        _ => crate::particle::ServerParticleOptions::Simple,
    };
    Ok(Some(NetworkEvent::LevelParticles {
        kind,
        options,
        override_limiter,
        always_show,
        pos,
        x_dist,
        y_dist,
        z_dist,
        max_speed,
        count,
    }))
}

fn read_raw_component(
    cur: &mut std::io::Cursor<&[u8]>,
) -> Result<crate::chat_component::Component, String> {
    let mut pos = cur.position() as usize;
    let component = super::chat::read_component(cur.get_ref(), &mut pos)?;
    cur.set_position(pos as u64);
    Ok(component)
}

fn read_raw_string(cur: &mut std::io::Cursor<&[u8]>, max_chars: usize) -> Result<String, String> {
    let mut pos = cur.position() as usize;
    let value = super::chat::read_string(cur.get_ref(), &mut pos, max_chars, "scoreboard string")?;
    cur.set_position(pos as u64);
    Ok(value)
}

fn read_raw_number_format(
    cur: &mut std::io::Cursor<&[u8]>,
) -> Result<Option<crate::ui::hud::ScoreNumberFormat>, String> {
    use crate::ui::hud::ScoreNumberFormat as F;
    if !bool::azalea_read(cur).map_err(|e| e.to_string())? {
        return Ok(None);
    }
    Ok(Some(
        match u32::azalea_read_var(cur).map_err(|e| e.to_string())? {
            0 => F::Blank,
            1 => {
                let mut pos = cur.position() as usize;
                let tag = super::chat::read_nbt_tag(cur.get_ref(), &mut pos)?;
                cur.set_position(pos as u64);
                F::Styled(
                    crate::chat_component::Style::from_nbt_tag(&tag).map_err(|e| e.to_string())?,
                )
            }
            2 => F::Fixed(crate::ui::text::format_component_spans(
                &read_raw_component(cur)?,
                [1.0; 4],
            )),
            id => return Err(format!("unknown score number format {id}")),
        },
    ))
}

fn parse_legacy_score(cur: &mut std::io::Cursor<&[u8]>) -> Result<NetworkEvent, String> {
    let owner = read_raw_string(cur, 32767)?;
    let objective = read_raw_string(cur, 32767)?;
    let score = i32::azalea_read_var(cur).map_err(|e| e.to_string())?;
    let display = if bool::azalea_read(cur).map_err(|e| e.to_string())? {
        Some(crate::ui::text::format_component_spans(
            &read_raw_component(cur)?,
            [1.0; 4],
        ))
    } else {
        None
    };
    Ok(NetworkEvent::ScoreboardScore {
        owner,
        objective,
        score,
        display,
        number_format: read_raw_number_format(cur)?,
    })
}

fn parse_legacy_team(cur: &mut std::io::Cursor<&[u8]>) -> Result<NetworkEvent, String> {
    use azalea_protocol::packets::game::c_set_player_team::{CollisionRule, NameTagVisibility};
    let name = read_raw_string(cur, 32767)?;
    let method = u8::azalea_read(cur).map_err(|e| e.to_string())?;
    let display = read_raw_component(cur)?;
    let prefix = read_raw_component(cur)?;
    let suffix = read_raw_component(cur)?;
    let nametag_visibility = NameTagVisibility::azalea_read(cur).map_err(|e| e.to_string())?;
    let collision_rule = CollisionRule::azalea_read(cur).map_err(|e| e.to_string())?;
    let formatting =
        azalea_chat::style::ChatFormatting::azalea_read(cur).map_err(|e| e.to_string())?;
    let options = u8::azalea_read(cur).map_err(|e| e.to_string())?;
    let members = if method == 0 {
        let count = u32::azalea_read_var(cur).map_err(|e| e.to_string())? as usize;
        if count > cur.get_ref().len().saturating_sub(cur.position() as usize) {
            return Err("truncated team members".into());
        }
        let mut members = Vec::new();
        for _ in 0..count {
            members.push(read_raw_string(cur, 32767)?);
        }
        Some(members)
    } else {
        None
    };
    let color = team_color(formatting);
    Ok(NetworkEvent::ScoreboardTeam {
        name,
        display_name: crate::ui::text::format_component_spans(&display, [1.0; 4]),
        prefix: crate::ui::text::format_component_spans(&prefix, color),
        suffix: crate::ui::text::format_component_spans(&suffix, color),
        color,
        fill_color: formatting.color().map(crate::ui::common::rgb),
        sidebar_slot: team_sidebar_slot(formatting),
        nametag_visibility,
        collision_rule,
        friendly_fire: options & 1 != 0,
        see_friendly_invisibles: options & 2 != 0,
        members,
    })
}

/// Preserve only affected TextDisplay text before Azalea consumes its legacy
/// codes. Text placeholders keep each field's position for ordered dispatch;
/// other metadata bytes remain exact. No walk/allocation without a legacy
/// marker.
pub(crate) fn preserve_legacy_display_text(
    raw: &[u8],
) -> Result<Option<(Vec<u8>, Vec<NetworkEvent>)>, String> {
    if !raw.windows(2).any(|bytes| bytes == [0xc2, 0xa7]) {
        return Ok(None);
    }
    let mut cur = std::io::Cursor::new(raw);
    let packet_id = u32::azalea_read_var(&mut cur).map_err(|e| e.to_string())?;
    if pomme_protocol::PacketTable::native().name_of(
        pomme_protocol::Phase::Game,
        pomme_protocol::Direction::Clientbound,
        packet_id,
    ) != Some("set_entity_data")
    {
        return Ok(None);
    }
    let id = i32::azalea_read_var(&mut cur).map_err(|e| e.to_string())?;
    let mut remaining = raw[..cur.position() as usize].to_vec();
    let mut events = Vec::new();
    let mut has_legacy_text = false;
    loop {
        let start = cur.position() as usize;
        let index = u8::azalea_read(&mut cur).map_err(|e| e.to_string())?;
        if index == 255 {
            remaining.push(255);
            break;
        }
        let value_start = cur.position();
        let ty = u32::azalea_read_var(&mut cur).map_err(|e| e.to_string())?;
        if index == 23 && ty == 5 {
            let component = read_raw_component(&mut cur)?;
            component.visit_text(
                &crate::chat_component::ResolvedStyle::default(),
                &mut |text, _| has_legacy_text |= text.contains('§'),
            );
            // Keep duplicate text fields in wire order, even if only one has codes.
            events.push(NetworkEvent::TextDisplayText {
                id,
                text: crate::ui::text::format_component_spans(&component, [1.0; 4]),
            });
            remaining.extend_from_slice(&raw[start..value_start as usize]);
            pomme_protocol::wire::write_varint(&mut remaining, ty);
            simdnbt::owned::NbtTag::String("".into()).write(&mut remaining);
        } else {
            cur.set_position(value_start);
            azalea_entity::EntityDataValue::azalea_read(&mut cur).map_err(|e| e.to_string())?;
            remaining.extend_from_slice(&raw[start..cur.position() as usize]);
        }
    }
    if cur.position() as usize != raw.len() {
        return Err("trailing entity metadata bytes".into());
    }
    Ok(has_legacy_text.then_some((remaining, events)))
}

fn parse_set_objective(cur: &mut std::io::Cursor<&[u8]>) -> Result<NetworkEvent, String> {
    use azalea_buf::AzBuf;
    use azalea_protocol::packets::game::c_set_objective::MethodKind;

    let name = read_raw_string(cur, 32767)?;
    let method = MethodKind::azalea_read(cur).map_err(|error| error.to_string())?;
    let (display, number_format, render_type) = match method {
        MethodKind::Remove => (None, None, None),
        MethodKind::Add | MethodKind::Change => {
            let text = read_raw_component(cur)?;
            let render_type = azalea_core::objectives::ObjectiveCriteria::azalea_read(cur)
                .map_err(|error| error.to_string())?;
            let number_format = read_raw_number_format(cur)?;
            let display = crate::ui::text::format_component_spans(&text, [1.0; 4]);
            (Some(display), number_format, Some(render_type))
        }
    };
    Ok(NetworkEvent::ScoreboardObjective {
        name,
        display,
        number_format,
        render_type,
    })
}

#[cfg(test)]
pub(crate) fn normalize_team_wire_fixture(raw: &[u8]) -> Vec<u8> {
    super::native_codecs::normalize_native_team_color(raw)
        .unwrap()
        .unwrap()
}

#[cfg(test)]
pub(crate) fn legacy_text_nbt(text: &str) -> simdnbt::owned::NbtTag {
    use simdnbt::owned::{NbtCompound, NbtTag};
    let mut nbt = NbtCompound::new();
    nbt.insert("text", NbtTag::String(text.into()));
    nbt.insert("color", NbtTag::String("blue".into()));
    nbt.insert("bold", NbtTag::Byte(1));
    NbtTag::Compound(nbt)
}

#[cfg(test)]
pub(crate) fn legacy_display_wire_fixture() -> Vec<u8> {
    use pomme_protocol::{Direction, PacketTable, Phase};
    let mut raw = Vec::new();
    pomme_protocol::wire::write_varint(
        &mut raw,
        PacketTable::native()
            .id(Phase::Game, Direction::Clientbound, "set_entity_data")
            .unwrap(),
    );
    pomme_protocol::wire::write_varint(&mut raw, 7);
    raw.extend_from_slice(&[24, 1, 8]); // line width: Int(8)
    raw.extend_from_slice(&[23, 5]);
    legacy_text_nbt("§cA§rA\n§aA§BA").write(&mut raw);
    raw.extend_from_slice(&[27, 0, 0, 255]); // flags: Byte(0), end
    raw
}

#[cfg(test)]
fn cooldown_packet_id() -> u32 {
    use pomme_protocol::{Direction, PacketTable, Phase};

    static ID: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *ID.get_or_init(|| {
        PacketTable::native()
            .id(Phase::Game, Direction::Clientbound, "cooldown")
            .expect("cooldown in packet table")
    })
}

/// Icon items for a recipe toast entry (vanilla `RecipeToast.addOrUpdate`:
/// `craftingStation()` and `result()` resolved for their first stack).
fn recipe_toast_entry(
    display: &azalea_protocol::common::recipe::RecipeDisplayData,
) -> crate::ui::toast::RecipeToastEntry {
    use azalea_protocol::common::recipe::RecipeDisplayData;

    let (station, result) = match display {
        RecipeDisplayData::Shapeless(d) => (&d.crafting_station, &d.result),
        RecipeDisplayData::Shaped(d) => (&d.crafting_station, &d.result),
        RecipeDisplayData::Furnace(d) => (&d.crafting_station, &d.result),
        RecipeDisplayData::Stonecutter(d) => (&d.crafting_station, &d.result),
        RecipeDisplayData::Smithing(d) => (&d.crafting_station, &d.result),
    };
    crate::ui::toast::RecipeToastEntry {
        category_item: slot_display_first_item(station),
        unlocked_item: slot_display_first_item(result),
    }
}

/// First-stack resolution of a slot display, mirroring vanilla
/// `SlotDisplay.resolveForFirstStack` for the context-free variants.
/// Component-modified displays fall back to the bare base item (pomme's icon
/// atlas keys on item name only); `Tag`/`AnyFuel` need registries pomme
/// doesn't track client-side and vanilla never uses them for station/result.
fn slot_display_first_item(
    slot: &azalea_protocol::common::recipe::SlotDisplayData,
) -> Option<String> {
    use azalea_inventory::ItemStack;
    use azalea_protocol::common::recipe::SlotDisplayData;

    match slot {
        SlotDisplayData::Empty | SlotDisplayData::AnyFuel | SlotDisplayData::Tag(_) => None,
        SlotDisplayData::Item(d) => Some(item_resource_name(d.item)),
        SlotDisplayData::ItemStack(d) => match &d.stack {
            ItemStack::Present(data) => Some(item_resource_name(data.kind)),
            ItemStack::Empty => None,
        },
        SlotDisplayData::WithAnyPotion(d) => slot_display_first_item(&d.contents),
        SlotDisplayData::OnlyWithComponent(d) => slot_display_first_item(&d.contents),
        SlotDisplayData::Dyed(d) => slot_display_first_item(&d.target),
        SlotDisplayData::SmithingTrim(d) => slot_display_first_item(&d.base),
        SlotDisplayData::WithRemainder(d) => slot_display_first_item(&d.input),
        SlotDisplayData::Composite(d) => d.contents.iter().find_map(slot_display_first_item),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use azalea_protocol::packets::game::c_set_held_slot::ClientboundSetHeldSlot;
    use parking_lot::Mutex;
    use pomme_protocol::wire;

    use super::{
        handle_game_packet as handle_game_packet_async,
        handle_raw_game_packet as handle_raw_game_packet_async, *,
    };

    #[test]
    fn level_particles_entity_effect_decodes_full_argb_tint() {
        let mut raw = Vec::new();
        false.azalea_write(&mut raw).unwrap();
        true.azalea_write(&mut raw).unwrap();
        for value in [1.0f64, 2.0, 3.0] {
            value.azalea_write(&mut raw).unwrap();
        }
        for value in [0.0f32, 0.0, 0.0, 1.0] {
            value.azalea_write(&mut raw).unwrap();
        }
        1i32.azalea_write(&mut raw).unwrap();
        28u32.azalea_write_var(&mut raw).unwrap();
        raw.extend_from_slice(&0x2612_3456u32.to_be_bytes());
        let event = parse_level_particles(&mut std::io::Cursor::new(raw.as_slice()))
            .unwrap()
            .unwrap();
        assert!(matches!(
            event,
            NetworkEvent::LevelParticles {
                kind: crate::particle::ServerParticleKind::EntityEffect,
                options: crate::particle::ServerParticleOptions::EntityEffect {
                    color: 0x2612_3456
                },
                ..
            }
        ));
    }

    fn handle_raw_game_packet(raw: &[u8], tx: &Sender<NetworkEvent>) -> bool {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(handle_raw_game_packet_async(raw, tx))
            .unwrap()
    }

    async fn handle_game_packet(
        packet: &ClientboundGamePacket,
        sender: &PacketSender,
        event_tx: &Sender<NetworkEvent>,
        registries: &RegistryHolder,
        tree: &SharedCommandTree,
        batches: &mut ChunkBatchSizeCalculator,
        cookies: &mut std::collections::HashMap<Identifier, Vec<u8>>,
    ) -> Result<(), SendError<NetworkEvent>> {
        handle_game_packet_async(
            packet,
            sender,
            event_tx,
            registries,
            tree,
            batches,
            &mut (384, -64),
            cookies,
        )
        .await
    }
    use crate::net::sender::Outbound;

    fn statue_chunk_packet(nbt: simdnbt::owned::Nbt) -> ClientboundGamePacket {
        use azalea_protocol::packets::game::c_level_chunk_with_light::{
            BlockEntity, ClientboundLevelChunkPacketData, ClientboundLevelChunkWithLight,
        };
        ClientboundGamePacket::LevelChunkWithLight(ClientboundLevelChunkWithLight {
            x: -2,
            z: 1,
            chunk_data: ClientboundLevelChunkPacketData {
                heightmaps: Vec::new(),
                data: Arc::new({
                    let mut data = Vec::new();
                    for _ in 0..24 {
                        azalea_world::chunk::Section::default()
                            .azalea_write(&mut data)
                            .unwrap();
                    }
                    data.into_boxed_slice()
                }),
                block_entities: vec![BlockEntity {
                    packed_xz: 0xf1,
                    y: (-64i16) as u16,
                    kind: azalea_registry::builtin::BlockEntityKind::CopperGolemStatue,
                    data: nbt,
                }],
            },
            light_data: Default::default(),
        })
    }

    async fn dispatch_world_packet(
        packet: &ClientboundGamePacket,
        event_tx: &Sender<NetworkEvent>,
    ) -> Result<(), SendError<NetworkEvent>> {
        let (out_tx, _out_rx) = tokio::sync::mpsc::unbounded_channel();
        handle_game_packet(
            packet,
            &PacketSender::new(out_tx),
            event_tx,
            &RegistryHolder::default(),
            &Arc::new(Mutex::new(None)),
            &mut ChunkBatchSizeCalculator::default(),
            &mut std::collections::HashMap::new(),
        )
        .await
    }

    async fn resume_full_dispatch(
        packet: &ClientboundGamePacket,
        tx: &Sender<NetworkEvent>,
        rx: &crossbeam_channel::Receiver<NetworkEvent>,
    ) -> NetworkEvent {
        let dispatch = dispatch_world_packet(packet, tx);
        tokio::pin!(dispatch);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(10), &mut dispatch)
                .await
                .is_err()
        );
        assert!(rx.is_full());
        assert!(
            rx.try_iter()
                .all(|event| matches!(event, NetworkEvent::LevelChunksLoadStart))
        );
        tokio::time::timeout(std::time::Duration::from_secs(2), dispatch)
            .await
            .unwrap()
            .unwrap();
        let event = rx.try_recv().unwrap();
        assert!(rx.is_empty());
        event
    }

    #[tokio::test]
    async fn remove_entities_stays_ordered_and_waits_when_queue_is_full() {
        use azalea_core::entity_id::MinecraftEntityId;
        use azalea_protocol::packets::game::c_remove_entities::ClientboundRemoveEntities;

        let packet = ClientboundGamePacket::RemoveEntities(ClientboundRemoveEntities {
            entity_ids: vec![MinecraftEntityId(12), MinecraftEntityId(34)],
        });
        let (tx, rx) = crossbeam_channel::bounded(4096);
        tx.try_send(NetworkEvent::LevelChunksLoadStart).unwrap();
        dispatch_world_packet(&packet, &tx).await.unwrap();
        assert!(matches!(
            rx.try_recv().unwrap(),
            NetworkEvent::LevelChunksLoadStart
        ));
        assert!(matches!(
            rx.try_recv().unwrap(),
            NetworkEvent::EntitiesRemoved { ids } if ids == [12, 34]
        ));
        assert!(rx.is_empty());

        for _ in 0..4096 {
            tx.try_send(NetworkEvent::LevelChunksLoadStart).unwrap();
        }
        assert!(matches!(
            resume_full_dispatch(&packet, &tx, &rx).await,
            NetworkEvent::EntitiesRemoved { ids } if ids == [12, 34]
        ));
        drop(rx);
        assert!(matches!(
            dispatch_world_packet(&packet, &tx).await,
            Err(SendError(NetworkEvent::EntitiesRemoved { ids })) if ids == [12, 34]
        ));
    }

    #[tokio::test]
    async fn living_effect_particle_metadata_is_forwarded_with_native_types() {
        use azalea_core::entity_id::MinecraftEntityId;
        use azalea_entity::{EntityDataItem, EntityDataValue, EntityMetadataItems};
        use azalea_protocol::packets::game::c_set_entity_data::ClientboundSetEntityData;

        let particles = vec![azalea_entity::particle::Particle::EntityEffect(
            azalea_entity::particle::ColorParticle::default(),
        )];
        let packet = ClientboundGamePacket::SetEntityData(ClientboundSetEntityData {
            id: MinecraftEntityId(12),
            packed_items: EntityMetadataItems(vec![
                EntityDataItem {
                    index: 10,
                    value: EntityDataValue::Particles(particles.clone().into_boxed_slice()),
                },
                EntityDataItem {
                    index: 11,
                    value: EntityDataValue::Boolean(true),
                },
            ]),
        });
        let (tx, rx) = crossbeam_channel::bounded(4);
        dispatch_world_packet(&packet, &tx).await.unwrap();
        assert!(matches!(
            rx.try_recv().unwrap(),
            NetworkEvent::EntityEffectParticles { id: 12, particles: Some(v), ambient: None }
                if v == particles
        ));
        assert!(matches!(
            rx.try_recv().unwrap(),
            NetworkEvent::EntityEffectParticles {
                id: 12,
                particles: None,
                ambient: Some(true)
            }
        ));
        assert!(matches!(
            rx.try_recv().unwrap(),
            NetworkEvent::EntityData {
                id: 12,
                index: 11,
                value: MetaValue::Bool(true)
            }
        ));
        assert!(rx.is_empty());
    }

    #[tokio::test]
    async fn landing_signals_stay_ordered_and_wait_on_full_queue() {
        use azalea_core::entity_id::MinecraftEntityId;
        use azalea_entity::{EntityDataItem, EntityDataValue, EntityMetadataItems};
        use azalea_protocol::packets::game::c_entity_event::ClientboundEntityEvent;
        use azalea_protocol::packets::game::c_set_entity_data::ClientboundSetEntityData;

        let metadata = ClientboundGamePacket::SetEntityData(ClientboundSetEntityData {
            id: MinecraftEntityId(12),
            packed_items: EntityMetadataItems(vec![EntityDataItem {
                index: 10,
                value: EntityDataValue::Boolean(true),
            }]),
        });
        let impact = ClientboundGamePacket::EntityEvent(ClientboundEntityEvent {
            entity_id: MinecraftEntityId(34),
            event_id: 3,
        });
        let (tx, rx) = crossbeam_channel::bounded(2);
        dispatch_world_packet(&metadata, &tx).await.unwrap();
        dispatch_world_packet(&impact, &tx).await.unwrap();
        assert!(matches!(
            rx.try_recv().unwrap(),
            NetworkEvent::EntityData {
                id: 12,
                index: 10,
                value: MetaValue::Bool(true)
            }
        ));
        assert!(matches!(
            rx.try_recv().unwrap(),
            NetworkEvent::EntityDied { id: 34 }
        ));
        for packet in [&metadata, &impact] {
            tx.try_send(NetworkEvent::LevelChunksLoadStart).unwrap();
            tx.try_send(NetworkEvent::LevelChunksLoadStart).unwrap();
            let event = resume_full_dispatch(packet, &tx, &rx).await;
            assert!(matches!(
                event,
                NetworkEvent::EntityData { .. } | NetworkEvent::EntityDied { .. }
            ));
        }
        drop(rx);
        assert!(matches!(
            dispatch_world_packet(&impact, &tx).await,
            Err(SendError(NetworkEvent::EntityDied { id: 34 }))
        ));
    }

    #[tokio::test]
    async fn login_then_respawn_multi_event_delivery_stays_fifo_on_a_single_slot() {
        use azalea_core::entity_id::MinecraftEntityId;
        use azalea_core::game_type::{GameMode, OptionalGameType};
        use azalea_protocol::packets::common::CommonPlayerSpawnInfo;
        use azalea_protocol::packets::game::c_login::ClientboundLogin;
        use azalea_protocol::packets::game::c_respawn::ClientboundRespawn;
        let common = CommonPlayerSpawnInfo {
            dimension_type: azalea_registry::data::DimensionKind::new_raw(0),
            dimension: "minecraft:overworld".into(),
            seed: 42,
            game_type: GameMode::Survival,
            previous_game_type: OptionalGameType(None),
            is_debug: false,
            is_flat: false,
            last_death_location: None,
            portal_cooldown: 0,
            sea_level: 63,
        };
        let packets = [
            ClientboundGamePacket::Login(ClientboundLogin {
                player_id: MinecraftEntityId(7),
                hardcore: false,
                levels: vec!["minecraft:overworld".into()],
                max_players: 20,
                chunk_radius: 12,
                simulation_distance: 10,
                reduced_debug_info: false,
                show_death_screen: true,
                do_limited_crafting: false,
                common: common.clone(),
                online_mode: false,
                enforces_secure_chat: false,
            }),
            ClientboundGamePacket::Respawn(ClientboundRespawn {
                common,
                data_to_keep: 3,
            }),
        ];
        let registries = crate::net::known_packs::filled_holder("dimension_type");
        assert!(!registries.dimension_type.map.is_empty());
        let (tx, rx) = crossbeam_channel::bounded(1);
        tx.try_send(NetworkEvent::LevelChunksLoadStart).unwrap();
        let producer = async {
            let (out_tx, _out_rx) = tokio::sync::mpsc::unbounded_channel();
            let sender = PacketSender::new(out_tx);
            let tree = Arc::new(Mutex::new(None));
            let mut batches = ChunkBatchSizeCalculator::default();
            let mut cookies = Default::default();
            for packet in &packets {
                handle_game_packet(
                    packet,
                    &sender,
                    &tx,
                    &registries,
                    &tree,
                    &mut batches,
                    &mut cookies,
                )
                .await
                .unwrap();
            }
        };
        tokio::pin!(producer);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(10), &mut producer)
                .await
                .is_err()
        );
        let consumer = async {
            let mut order = Vec::new();
            for _ in 0..13 {
                let event = tokio::time::timeout(std::time::Duration::from_secs(2), async {
                    loop {
                        if let Ok(event) = rx.try_recv() {
                            break event;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                    }
                })
                .await
                .unwrap();
                order.push(match event {
                    NetworkEvent::LevelChunksLoadStart => "start",
                    NetworkEvent::DimensionInfo { .. } => "dimension",
                    NetworkEvent::DimensionName { .. } => "name",
                    NetworkEvent::GameModeChanged { .. } => "mode",
                    NetworkEvent::ServerViewDistance { .. } => "view",
                    NetworkEvent::ServerSimulationDistance { .. } => "simulation",
                    NetworkEvent::PlayerLogin { .. } => "login",
                    NetworkEvent::SecureChatEnforced { .. } => "secure",
                    NetworkEvent::PlayerRespawned { .. } => "respawn",
                    NetworkEvent::ClearMobEffects => "clear_effects",
                    _ => panic!("unexpected lifecycle event"),
                });
            }
            assert_eq!(
                order,
                [
                    "start",
                    "dimension",
                    "name",
                    "mode",
                    "view",
                    "simulation",
                    "login",
                    "secure",
                    "respawn",
                    "dimension",
                    "name",
                    "mode",
                    "clear_effects"
                ]
            );
        };
        tokio::join!(producer, consumer);
        assert!(rx.is_empty());
    }

    #[tokio::test]
    async fn raw_game_route_waits_on_full_and_does_not_hide_receiver_drop() {
        let mut raw = Vec::new();
        wire::write_varint(&mut raw, cooldown_packet_id());
        "test:shared".to_string().azalea_write(&mut raw).unwrap();
        (-1_i32).azalea_write_var(&mut raw).unwrap();
        let (tx, rx) = crossbeam_channel::bounded(1);
        tx.try_send(NetworkEvent::LevelChunksLoadStart).unwrap();
        {
            let route = handle_raw_game_packet_async(&raw, &tx);
            tokio::pin!(route);
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(10), &mut route)
                    .await
                    .is_err()
            );
            assert!(matches!(
                rx.try_recv().unwrap(),
                NetworkEvent::LevelChunksLoadStart
            ));
            assert!(
                tokio::time::timeout(std::time::Duration::from_secs(2), route)
                    .await
                    .unwrap()
                    .unwrap()
            );
        }
        assert!(matches!(
            rx.try_recv().unwrap(),
            NetworkEvent::ItemCooldown { duration: -1, .. }
        ));
        assert!(rx.is_empty());
        drop(rx);
        assert!(handle_raw_game_packet_async(&raw, &tx).await.is_err());
        raw.pop();
        assert!(handle_raw_game_packet_async(&raw, &tx).await.unwrap());
    }

    #[tokio::test]
    async fn malformed_chunk_skips_without_consuming_queue_or_reordering_next_load() {
        use azalea_protocol::packets::game::c_forget_level_chunk::ClientboundForgetLevelChunk;
        let (out_tx, _out_rx) = tokio::sync::mpsc::unbounded_channel();
        let sender = PacketSender::new(out_tx);
        let (tx, rx) = crossbeam_channel::bounded(2);
        let registries = RegistryHolder::default();
        let tree = Arc::new(Mutex::new(None));
        let mut batches = ChunkBatchSizeCalculator::default();
        let mut cookies = std::collections::HashMap::new();
        let mut dimension = (16, -64);
        let mut packet = statue_chunk_packet(simdnbt::owned::Nbt::None);
        let ClientboundGamePacket::LevelChunkWithLight(ref mut payload) = packet else {
            unreachable!()
        };
        let valid = payload.chunk_data.data.clone();
        payload.chunk_data.data = Arc::new(Vec::new().into_boxed_slice());
        handle_game_packet_async(
            &packet,
            &sender,
            &tx,
            &registries,
            &tree,
            &mut batches,
            &mut dimension,
            &mut cookies,
        )
        .await
        .unwrap();
        assert!(rx.is_empty());
        let ClientboundGamePacket::LevelChunkWithLight(ref mut payload) = packet else {
            unreachable!()
        };
        payload.chunk_data.data = valid;
        handle_game_packet_async(
            &packet,
            &sender,
            &tx,
            &registries,
            &tree,
            &mut batches,
            &mut dimension,
            &mut cookies,
        )
        .await
        .unwrap();
        handle_game_packet_async(
            &ClientboundGamePacket::ForgetLevelChunk(ClientboundForgetLevelChunk {
                pos: ChunkPos::new(-2, 1),
            }),
            &sender,
            &tx,
            &registries,
            &tree,
            &mut batches,
            &mut dimension,
            &mut cookies,
        )
        .await
        .unwrap();
        let NetworkEvent::ChunkLoaded { chunk, .. } = rx.try_recv().unwrap() else {
            panic!("valid chunk must be published before unload")
        };
        assert_eq!(chunk.sections.len(), 1);
        assert!(matches!(
            rx.try_recv().unwrap(),
            NetworkEvent::ChunkUnloaded { .. }
        ));
    }

    #[tokio::test]
    async fn last_queue_slot_accepts_chunk_and_empty_statue_nbt_together() {
        use azalea_registry::builtin::BlockEntityKind;
        use simdnbt::owned::{Nbt, NbtCompound};

        assert_eq!(BlockEntityKind::CopperGolemStatue.to_u32(), 47);
        for nbt in [Nbt::None, Nbt::new("".into(), NbtCompound::new())] {
            let (tx, rx) = crossbeam_channel::bounded(4096);
            for _ in 0..4095 {
                tx.try_send(NetworkEvent::LevelChunksLoadStart).unwrap();
            }
            dispatch_world_packet(&statue_chunk_packet(nbt), &tx)
                .await
                .unwrap();
            assert_eq!(rx.len(), 4096);
            for _ in 0..4095 {
                assert!(matches!(
                    rx.try_recv().unwrap(),
                    NetworkEvent::LevelChunksLoadStart
                ));
            }
            let NetworkEvent::ChunkLoaded {
                pos,
                block_entities,
                ..
            } = rx.try_recv().unwrap()
            else {
                panic!("expected atomic chunk snapshot");
            };
            assert_eq!(pos, ChunkPos::new(-2, 1));
            assert_eq!(block_entities.len(), 1);
            let (pos, kind, nbt) = &block_entities[0];
            assert_eq!(*pos, BlockPos::new(-17, -64, 17));
            assert_eq!(*kind, BlockEntityKind::CopperGolemStatue);
            assert!(nbt.is_empty());
            assert!(rx.is_empty());
        }
    }

    #[tokio::test]
    async fn mandatory_world_packets_wait_on_full_and_stop_on_disconnected_queue() {
        use azalea_protocol::packets::game::c_block_entity_data::ClientboundBlockEntityData;
        use azalea_protocol::packets::game::c_block_update::ClientboundBlockUpdate;
        use azalea_protocol::packets::game::c_forget_level_chunk::ClientboundForgetLevelChunk;
        use azalea_protocol::packets::game::c_light_update::ClientboundLightUpdate;
        use azalea_protocol::packets::game::c_section_blocks_update::ClientboundSectionBlocksUpdate;
        let pos = BlockPos::new(-17, -64, 17);
        for packet in [
            statue_chunk_packet(simdnbt::owned::Nbt::None),
            ClientboundGamePacket::ForgetLevelChunk(ClientboundForgetLevelChunk {
                pos: ChunkPos::new(-2, 1),
            }),
            ClientboundGamePacket::BlockEntityData(ClientboundBlockEntityData {
                pos,
                block_entity_type: azalea_registry::builtin::BlockEntityKind::CopperGolemStatue,
                tag: simdnbt::owned::Nbt::None,
            }),
            ClientboundGamePacket::BlockUpdate(ClientboundBlockUpdate {
                pos,
                block_state: azalea_block::BlockState::AIR,
            }),
            ClientboundGamePacket::SectionBlocksUpdate(ClientboundSectionBlocksUpdate {
                section_pos: azalea_core::position::ChunkSectionPos::new(-2, -4, 1),
                states: Vec::new(),
            }),
            ClientboundGamePacket::LightUpdate(ClientboundLightUpdate {
                x: -2,
                z: 1,
                light_data: Default::default(),
            }),
        ] {
            let (tx, rx) = crossbeam_channel::bounded(4096);
            for _ in 0..4096 {
                tx.try_send(NetworkEvent::LevelChunksLoadStart).unwrap();
            }
            let event = resume_full_dispatch(&packet, &tx, &rx).await;
            if let NetworkEvent::ChunkLoaded { block_entities, .. } = event {
                assert_eq!(block_entities.len(), 1);
                assert!(block_entities[0].2.is_empty());
            }
            drop(rx);
            assert!(matches!(
                dispatch_world_packet(&packet, &tx).await,
                Err(SendError(_))
            ));
        }
    }

    #[tokio::test]
    async fn chunk_then_standalone_be_and_block_updates_then_unload_stay_fifo() {
        use azalea_protocol::packets::game::c_block_entity_data::ClientboundBlockEntityData;
        use azalea_protocol::packets::game::c_block_update::ClientboundBlockUpdate;
        use azalea_protocol::packets::game::c_forget_level_chunk::ClientboundForgetLevelChunk;
        let (tx, rx) = crossbeam_channel::bounded(4);
        let pos = BlockPos::new(-17, -64, 17);
        dispatch_world_packet(&statue_chunk_packet(simdnbt::owned::Nbt::None), &tx)
            .await
            .unwrap();
        dispatch_world_packet(
            &ClientboundGamePacket::BlockEntityData(ClientboundBlockEntityData {
                pos,
                block_entity_type: azalea_registry::builtin::BlockEntityKind::CopperGolemStatue,
                tag: simdnbt::owned::Nbt::None,
            }),
            &tx,
        )
        .await
        .unwrap();
        dispatch_world_packet(
            &ClientboundGamePacket::BlockUpdate(ClientboundBlockUpdate {
                pos,
                block_state: azalea_block::BlockState::AIR,
            }),
            &tx,
        )
        .await
        .unwrap();
        dispatch_world_packet(
            &ClientboundGamePacket::ForgetLevelChunk(ClientboundForgetLevelChunk {
                pos: ChunkPos::new(-2, 1),
            }),
            &tx,
        )
        .await
        .unwrap();
        assert!(matches!(
            rx.try_recv().unwrap(),
            NetworkEvent::ChunkLoaded { .. }
        ));
        assert!(matches!(
            rx.try_recv().unwrap(),
            NetworkEvent::BlockEntityUpdate { nbt: None, .. }
        ));
        assert!(matches!(
            rx.try_recv().unwrap(),
            NetworkEvent::BlockUpdate { .. }
        ));
        assert!(matches!(
            rx.try_recv().unwrap(),
            NetworkEvent::ChunkUnloaded { .. }
        ));
        assert!(rx.is_empty());
    }

    #[tokio::test]
    async fn animate_critical_actions_dispatch_distinct_events_and_other_actions_do_not() {
        use azalea_core::entity_id::MinecraftEntityId;
        use azalea_protocol::packets::game::c_animate::{AnimationAction, ClientboundAnimate};

        let (out_tx, _out_rx) = tokio::sync::mpsc::unbounded_channel();
        let sender = PacketSender::new(out_tx);
        let (event_tx, event_rx) = crossbeam_channel::bounded(4);
        let registries = RegistryHolder::default();
        let command_tree = Arc::new(Mutex::new(None));
        let mut batches = ChunkBatchSizeCalculator::default();
        let mut cookies = std::collections::HashMap::new();
        for (action, expected) in [
            (
                AnimationAction::CriticalHit,
                crate::net::CriticalHitKind::Critical,
            ),
            (
                AnimationAction::MagicCriticalHit,
                crate::net::CriticalHitKind::Enchanted,
            ),
        ] {
            handle_game_packet(
                &ClientboundGamePacket::Animate(ClientboundAnimate {
                    id: MinecraftEntityId(41),
                    action,
                }),
                &sender,
                &event_tx,
                &registries,
                &command_tree,
                &mut batches,
                &mut cookies,
            )
            .await
            .unwrap();
            assert!(
                matches!(event_rx.recv().unwrap(), NetworkEvent::CriticalHit { id: 41, kind } if kind == expected)
            );
        }
        handle_game_packet(
            &ClientboundGamePacket::Animate(ClientboundAnimate {
                id: MinecraftEntityId(41),
                action: AnimationAction::WakeUp,
            }),
            &sender,
            &event_tx,
            &registries,
            &command_tree,
            &mut batches,
            &mut cookies,
        )
        .await
        .unwrap();
        assert!(matches!(
            event_rx.recv().unwrap(),
            NetworkEvent::EntityWakeUp { id: 41 }
        ));
        for (event_id, expected) in [(35, Some(41)), (255, None)] {
            handle_game_packet(
                &ClientboundGamePacket::EntityEvent(
                    azalea_protocol::packets::game::c_entity_event::ClientboundEntityEvent {
                        entity_id: MinecraftEntityId(41),
                        event_id,
                    },
                ),
                &sender,
                &event_tx,
                &registries,
                &command_tree,
                &mut batches,
                &mut cookies,
            )
            .await
            .unwrap();
            if let Some(id) = expected {
                assert!(
                    matches!(event_rx.recv().unwrap(), NetworkEvent::TotemUsed { entity_id } if entity_id == id)
                );
            } else {
                assert!(event_rx.try_recv().is_err());
            }
        }
    }

    #[tokio::test]
    async fn explosion_ingress_preserves_the_complete_native_payload() {
        use azalea_core::position::Vec3;
        use azalea_entity::particle::Particle;
        use azalea_protocol::packets::game::c_explode::{
            ClientboundExplode, ExplosionParticleInfo, Weighted,
        };

        let (out_tx, _out_rx) = tokio::sync::mpsc::unbounded_channel();
        let sender = PacketSender::new(out_tx);
        let (event_tx, event_rx) = crossbeam_channel::bounded(1);
        let registries = RegistryHolder::default();
        let command_tree = Arc::new(Mutex::new(None));
        let mut batches = ChunkBatchSizeCalculator::default();
        let mut cookies = std::collections::HashMap::new();
        let packet = ClientboundExplode {
            center: Vec3::new(1.0, 2.0, 3.0),
            radius: 4.5,
            block_count: 23,
            player_knockback: Some(Vec3::new(0.25, -0.5, 0.75)),
            explosion_particle: Particle::ExplosionEmitter,
            explosion_sound: SoundEvent::AmbientCave,
            block_particles: vec![Weighted {
                value: ExplosionParticleInfo {
                    particle: Particle::EndRod,
                    scaling: 1.25,
                    speed: 0.75,
                },
                weight: 9,
            }],
        };
        handle_game_packet(
            &ClientboundGamePacket::Explode(packet.clone()),
            &sender,
            &event_tx,
            &registries,
            &command_tree,
            &mut batches,
            &mut cookies,
        )
        .await
        .unwrap();
        let NetworkEvent::Explosion(event) = event_rx.recv().unwrap() else {
            panic!("expected Explosion event");
        };
        assert_eq!(event.center, packet.center);
        assert_eq!(event.radius, packet.radius);
        assert_eq!(event.block_count, packet.block_count);
        assert_eq!(event.player_knockback, packet.player_knockback);
        assert_eq!(event.explosion_particle, packet.explosion_particle);
        assert_eq!(event.block_particles, packet.block_particles);
        assert_eq!(event.explosion_sound.event_name(), "minecraft:ambient.cave");
    }

    #[tokio::test]
    async fn player_rotation_and_entity_teleport_keep_relative_flags_in_events() {
        use azalea_core::entity_id::MinecraftEntityId;
        use azalea_core::position::Vec3;
        use azalea_protocol::common::movements::{PositionMoveRotation, RelativeMovements};
        use azalea_protocol::packets::game::c_entity_position_sync::ClientboundEntityPositionSync;
        use azalea_protocol::packets::game::c_player_rotation::ClientboundPlayerRotation;
        use azalea_protocol::packets::game::c_teleport_entity::ClientboundTeleportEntity;

        let (out_tx, _out_rx) = tokio::sync::mpsc::unbounded_channel();
        let sender = PacketSender::new(out_tx);
        let (event_tx, event_rx) = crossbeam_channel::bounded(8);
        let registries = RegistryHolder::default();
        let command_tree = Arc::new(Mutex::new(None));
        let mut batches = ChunkBatchSizeCalculator::default();
        let mut cookies = std::collections::HashMap::new();
        let mut dispatch = async |packet: ClientboundGamePacket| {
            handle_game_packet(
                &packet,
                &sender,
                &event_tx,
                &registries,
                &command_tree,
                &mut batches,
                &mut cookies,
            )
            .await
            .unwrap();
        };
        dispatch(ClientboundGamePacket::PlayerRotation(
            ClientboundPlayerRotation {
                y_rot: 12.0,
                relative_y: true,
                x_rot: 25.0,
                relative_x: false,
            },
        ))
        .await;
        assert!(matches!(
            event_rx.recv().unwrap(),
            NetworkEvent::PlayerRotation {
                y_rot: 12.0,
                x_rot: 25.0,
                relative_y: true,
                relative_x: false
            }
        ));

        dispatch(ClientboundGamePacket::TeleportEntity(
            ClientboundTeleportEntity {
                id: MinecraftEntityId(42),
                change: PositionMoveRotation {
                    pos: Vec3::new(1.0, 2.0, 3.0),
                    delta: Vec3::new(4.0, 5.0, 6.0),
                    look_direction: azalea_entity::LookDirection::new(7.0, 8.0),
                },
                relative: RelativeMovements {
                    x: true,
                    y: false,
                    z: true,
                    y_rot: true,
                    x_rot: false,
                    delta_x: true,
                    delta_y: false,
                    delta_z: true,
                    rotate_delta: true,
                },
                on_ground: true,
            },
        ))
        .await;
        let NetworkEvent::EntityTeleported {
            relative: Some(relative),
            velocity: Some(velocity),
            ..
        } = event_rx.recv().unwrap()
        else {
            panic!("TeleportEntity must preserve its relative flags and velocity");
        };
        assert!(relative.x && relative.z && relative.y_rot && relative.delta_x && relative.delta_z);
        assert!(relative.rotate_delta);
        assert_eq!(velocity, glam::dvec3(4.0, 5.0, 6.0));

        dispatch(ClientboundGamePacket::EntityPositionSync(
            ClientboundEntityPositionSync {
                id: MinecraftEntityId(42),
                values: PositionMoveRotation {
                    pos: Vec3::new(9.0, 8.0, 7.0),
                    delta: Vec3::new(6.0, 5.0, 4.0),
                    look_direction: azalea_entity::LookDirection::new(3.0, 2.0),
                },
                on_ground: false,
            },
        ))
        .await;
        assert!(matches!(
            event_rx.recv().unwrap(),
            NetworkEvent::EntityTeleported {
                relative: None,
                velocity: None,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn merchant_mount_and_border_packets_keep_native_fields_in_events() {
        use azalea_core::entity_id::MinecraftEntityId;
        use azalea_inventory::ItemStackData;
        use azalea_protocol::packets::game::c_initialize_border::ClientboundInitializeBorder;
        use azalea_protocol::packets::game::c_merchant_offers::{
            ClientboundMerchantOffers, DataComponentExactPredicate, ItemCost, MerchantOffer,
        };
        use azalea_protocol::packets::game::c_mount_screen_open::ClientboundMountScreenOpen;
        use azalea_protocol::packets::game::c_set_border_center::ClientboundSetBorderCenter;
        use azalea_protocol::packets::game::c_set_border_lerp_size::ClientboundSetBorderLerpSize;
        use azalea_protocol::packets::game::c_set_border_size::ClientboundSetBorderSize;
        use azalea_protocol::packets::game::c_set_border_warning_delay::ClientboundSetBorderWarningDelay;
        use azalea_protocol::packets::game::c_set_border_warning_distance::ClientboundSetBorderWarningDistance;
        use azalea_registry::builtin::ItemKind;

        let (out_tx, _out_rx) = tokio::sync::mpsc::unbounded_channel();
        let sender = PacketSender::new(out_tx);
        let (event_tx, event_rx) = crossbeam_channel::bounded(16);
        let registries = RegistryHolder::default();
        let command_tree = Arc::new(Mutex::new(None));
        let mut batches = ChunkBatchSizeCalculator::default();
        let mut cookies = std::collections::HashMap::new();
        let mut dispatch = async |packet: ClientboundGamePacket| {
            handle_game_packet(
                &packet,
                &sender,
                &event_tx,
                &registries,
                &command_tree,
                &mut batches,
                &mut cookies,
            )
            .await
            .unwrap();
        };
        dispatch(ClientboundGamePacket::MerchantOffers(
            ClientboundMerchantOffers {
                container_id: 17,
                offers: vec![MerchantOffer {
                    base_cost_a: ItemCost {
                        item: ItemKind::Emerald,
                        count: 4,
                        components: DataComponentExactPredicate { expected: vec![] },
                    },
                    result: ItemStackData::new(ItemKind::Stone, 2).into(),
                    cost_b: Some(ItemCost {
                        item: ItemKind::Diamond,
                        count: 3,
                        components: DataComponentExactPredicate { expected: vec![] },
                    }),
                    out_of_stock: true,
                    uses: 5,
                    max_uses: 12,
                    xp: 9,
                    special_price_diff: -2,
                    price_multiplier: 0.25,
                    demand: 6,
                }],
                villager_level: 4,
                villager_xp: 81,
                show_progress: true,
                can_restock: false,
            },
        ))
        .await;
        assert!(matches!(
            event_rx.recv().unwrap(),
            NetworkEvent::MerchantOffers {
                container_id: 17,
                offers,
                villager_level: 4,
                villager_xp: 81,
                show_progress: true,
                can_restock: false,
            } if offers.len() == 1
                && offers[0].base_cost_a.count == 4
                && offers[0].result.as_present().is_some_and(|s| s.count == 2)
                && offers[0].cost_b.as_ref().is_some_and(|c| c.count == 3)
                && offers[0].out_of_stock
                && offers[0].uses == 5
                && offers[0].max_uses == 12
                && offers[0].xp == 9
                && offers[0].special_price_diff == -2
                && offers[0].price_multiplier == 0.25
                && offers[0].demand == 6
        ));
        dispatch(ClientboundGamePacket::MountScreenOpen(
            ClientboundMountScreenOpen {
                container_id: 18,
                inventory_columns: 5,
                entity_id: MinecraftEntityId(42),
            },
        ))
        .await;
        assert!(matches!(
            event_rx.recv().unwrap(),
            NetworkEvent::MountScreenOpen {
                container_id: 18,
                inventory_columns: 5,
                entity_id: 42,
            }
        ));
        dispatch(ClientboundGamePacket::InitializeBorder(
            ClientboundInitializeBorder {
                new_center_x: 1.5,
                new_center_z: -2.5,
                old_size: 100.0,
                new_size: 50.0,
                lerp_time: 120,
                new_absolute_max_size: 29_999_984,
                warning_blocks: 7,
                warning_time: 21,
            },
        ))
        .await;
        assert!(matches!(
            event_rx.recv().unwrap(),
            NetworkEvent::WorldBorderInitialize {
                center_x: 1.5,
                center_z: -2.5,
                old_size: 100.0,
                new_size: 50.0,
                lerp_time: 120,
                absolute_max_size: 29_999_984,
                warning_blocks: 7,
                warning_time: 21,
            }
        ));
        dispatch(ClientboundGamePacket::SetBorderCenter(
            ClientboundSetBorderCenter {
                new_center_x: 3.0,
                new_center_z: 4.0,
            },
        ))
        .await;
        assert!(matches!(
            event_rx.recv().unwrap(),
            NetworkEvent::WorldBorderCenter { x: 3.0, z: 4.0 }
        ));
        dispatch(ClientboundGamePacket::SetBorderSize(
            ClientboundSetBorderSize { size: 75.0 },
        ))
        .await;
        assert!(matches!(
            event_rx.recv().unwrap(),
            NetworkEvent::WorldBorderSize { size: 75.0 }
        ));
        dispatch(ClientboundGamePacket::SetBorderLerpSize(
            ClientboundSetBorderLerpSize {
                old_size: 75.0,
                new_size: 25.0,
                lerp_time: 20,
            },
        ))
        .await;
        assert!(matches!(
            event_rx.recv().unwrap(),
            NetworkEvent::WorldBorderLerpSize {
                old_size: 75.0,
                new_size: 25.0,
                lerp_time: 20
            }
        ));
        dispatch(ClientboundGamePacket::SetBorderWarningDistance(
            ClientboundSetBorderWarningDistance { warning_blocks: 11 },
        ))
        .await;
        assert!(matches!(
            event_rx.recv().unwrap(),
            NetworkEvent::WorldBorderWarningBlocks { warning_blocks: 11 }
        ));
        dispatch(ClientboundGamePacket::SetBorderWarningDelay(
            ClientboundSetBorderWarningDelay { warning_delay: 31 },
        ))
        .await;
        assert!(matches!(
            event_rx.recv().unwrap(),
            NetworkEvent::WorldBorderWarningTime { warning_time: 31 }
        ));
    }

    #[test]
    fn raw_cooldown_packet_emits_decoded_group_and_signed_duration() {
        use azalea_buf::{AzBuf, AzBufVar};
        use pomme_protocol::{Direction, PacketTable, Phase};

        let mut raw = Vec::new();
        wire::write_varint(
            &mut raw,
            PacketTable::native()
                .id(Phase::Game, Direction::Clientbound, "cooldown")
                .unwrap(),
        );
        "test:shared".to_string().azalea_write(&mut raw).unwrap();
        (-1_i32).azalea_write_var(&mut raw).unwrap();

        let (event_tx, event_rx) = crossbeam_channel::bounded(1);
        assert!(handle_raw_game_packet(&raw, &event_tx));
        assert!(matches!(
            event_rx.recv().unwrap(),
            NetworkEvent::ItemCooldown { group, duration: -1 }
                if group == azalea_registry::identifier::Identifier::new("test:shared")
        ));
    }

    #[test]
    fn malformed_raw_cooldown_is_consumed_without_event() {
        use pomme_protocol::{Direction, PacketTable, Phase};

        let mut raw = Vec::new();
        wire::write_varint(
            &mut raw,
            PacketTable::native()
                .id(Phase::Game, Direction::Clientbound, "cooldown")
                .unwrap(),
        );
        raw.push(0x80);
        let (event_tx, event_rx) = crossbeam_channel::bounded(1);
        assert!(handle_raw_game_packet(&raw, &event_tx));
        assert!(event_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn set_held_slot_emits_authoritative_hotbar_selection() {
        let (out_tx, _out_rx) = tokio::sync::mpsc::unbounded_channel();
        let sender = PacketSender::new(out_tx);
        let (event_tx, event_rx) = crossbeam_channel::bounded(1);
        let registries = RegistryHolder::default();
        let command_tree = Arc::new(Mutex::new(None));
        let receive = async |slot| {
            handle_game_packet(
                &ClientboundGamePacket::SetHeldSlot(ClientboundSetHeldSlot { slot }),
                &sender,
                &event_tx,
                &registries,
                &command_tree,
                &mut ChunkBatchSizeCalculator::default(),
                &mut std::collections::HashMap::new(),
            )
            .await
            .unwrap();
        };

        receive(5).await;
        assert!(matches!(
            event_rx.recv().unwrap(),
            NetworkEvent::HeldSlot { slot: 5 }
        ));

        for invalid in [9, u32::MAX] {
            receive(invalid).await;
            assert!(matches!(
                event_rx.try_recv(),
                Err(crossbeam_channel::TryRecvError::Empty)
            ));
        }
    }

    #[tokio::test]
    async fn game_ping_is_answered_and_cookies_round_trip() {
        let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel();
        let sender = PacketSender::new(out_tx);
        let (event_tx, event_rx) = crossbeam_channel::bounded(4);
        let registries = RegistryHolder::default();
        let command_tree = Arc::new(Mutex::new(None));
        let mut cookies = std::collections::HashMap::new();
        let mut batches = ChunkBatchSizeCalculator::default();
        let dispatch = async |packet: &ClientboundGamePacket,
                              cookies: &mut std::collections::HashMap<_, _>,
                              batches: &mut ChunkBatchSizeCalculator| {
            handle_game_packet(
                packet,
                &sender,
                &event_tx,
                &registries,
                &command_tree,
                batches,
                cookies,
            )
            .await
            .unwrap();
        };

        dispatch(
            &ClientboundGamePacket::PlayerPosition(
                azalea_protocol::packets::game::c_player_position::ClientboundPlayerPosition {
                    id: 7,
                    change: azalea_protocol::common::movements::PositionMoveRotation {
                        pos: azalea_core::position::Vec3::default(),
                        delta: azalea_core::position::Vec3::default(),
                        look_direction: azalea_entity::LookDirection::default(),
                    },
                    relative: azalea_protocol::common::movements::RelativeMovements::default(),
                },
            ),
            &mut cookies,
            &mut batches,
        )
        .await;
        dispatch(
            &ClientboundGamePacket::Ping(azalea_protocol::packets::game::c_ping::ClientboundPing {
                id: 0x1234,
            }),
            &mut cookies,
            &mut batches,
        )
        .await;
        assert!(matches!(
            event_rx.try_recv().unwrap(),
            NetworkEvent::PlayerPosition { id: 7, .. }
        ));
        assert!(matches!(
            event_rx.try_recv().unwrap(),
            NetworkEvent::Ping { id: 0x1234 }
        ));
        assert!(
            out_rx.try_recv().is_err(),
            "PLAY pong must wait for main-thread FIFO"
        );

        let key = Identifier::new("minecraft:test");
        dispatch(
            &ClientboundGamePacket::StoreCookie(
                azalea_protocol::packets::game::c_store_cookie::ClientboundStoreCookie {
                    key: key.clone(),
                    payload: vec![1, 2, 3],
                },
            ),
            &mut cookies,
            &mut batches,
        )
        .await;
        dispatch(
            &ClientboundGamePacket::CookieRequest(
                azalea_protocol::packets::game::c_cookie_request::ClientboundCookieRequest { key },
            ),
            &mut cookies,
            &mut batches,
        )
        .await;
        let Outbound::Packet(packet) = out_rx.try_recv().unwrap() else {
            panic!("expected cookie response packet");
        };
        assert!(matches!(
            *packet,
            ServerboundGamePacket::CookieResponse(ref p)
                if p.payload.as_deref() == Some(&[1, 2, 3])
        ));

        let pos = ChunkPos::new(-2, 3);
        dispatch(
            &ClientboundGamePacket::ChunksBiomes(
                azalea_protocol::packets::game::c_chunks_biomes::ClientboundChunksBiomes {
                    chunk_biome_data: vec![
                        azalea_protocol::packets::game::c_chunks_biomes::ChunkBiomeData {
                            pos,
                            buffer: vec![4, 5, 6],
                        },
                    ],
                },
            ),
            &mut cookies,
            &mut batches,
        )
        .await;
        assert!(matches!(
            event_rx.try_recv().unwrap(),
            NetworkEvent::ChunkBiomes { pos: event_pos, data }
                if event_pos == pos && data == [4, 5, 6]
        ));
    }

    #[tokio::test]
    async fn legacy_display_wire_preserves_other_metadata_and_bounded_fifo() {
        let raw = legacy_display_wire_fixture();
        let (remaining, events) = preserve_legacy_display_text(&raw).unwrap().unwrap();
        let packet: ClientboundGamePacket = azalea_protocol::read::deserialize_packet(
            &mut std::io::Cursor::new(remaining.as_slice()),
        )
        .unwrap();
        let ClientboundGamePacket::SetEntityData(data) = &packet else {
            panic!("metadata");
        };
        assert_eq!(data.packed_items.len(), 3);
        assert_eq!(data.packed_items[0].index, 24);
        assert!(matches!(
            data.packed_items[0].value,
            azalea_entity::EntityDataValue::Int(8)
        ));
        assert_eq!(data.packed_items[1].index, 23);
        assert_eq!(data.packed_items[2].index, 27);
        let (tx, rx) = crossbeam_channel::bounded(1);
        tx.try_send(NetworkEvent::LevelChunksLoadStart).unwrap();
        let delivery = async {
            let (out_tx, _out_rx) = tokio::sync::mpsc::unbounded_channel();
            let mut events = events.into();
            super::handle_game_packet_with_display_text(
                &packet,
                &PacketSender::new(out_tx),
                &tx,
                &RegistryHolder::default(),
                &Arc::new(Vec::new()),
                Some("timeline registry snapshot unavailable"),
                None,
                &Arc::new(Mutex::new(None)),
                &mut ChunkBatchSizeCalculator::default(),
                &mut (384, -64),
                &mut Default::default(),
                &mut events,
            )
            .await
            .unwrap();
            assert!(events.is_empty());
        };
        tokio::pin!(delivery);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(10), &mut delivery)
                .await
                .is_err()
        );
        assert!(matches!(
            rx.try_recv().unwrap(),
            NetworkEvent::LevelChunksLoadStart
        ));
        let receive = async {
            let next = async || {
                tokio::time::timeout(std::time::Duration::from_secs(2), async {
                    loop {
                        if let Ok(event) = rx.try_recv() {
                            return event;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                    }
                })
                .await
                .unwrap()
            };
            assert!(matches!(
                next().await,
                NetworkEvent::EntityData {
                    id: 7,
                    index: 24,
                    value: MetaValue::Int(8)
                }
            ));
            let NetworkEvent::TextDisplayText { id, text } = next().await else {
                panic!("text");
            };
            assert_eq!(id, 7);
            assert_eq!(text[0].color, crate::ui::common::rgb(0xff5555));
            assert!(!text[0].bold);
            assert_eq!(text[1].color, crate::ui::common::rgb(0x5555ff));
            assert!(text[1].bold);
            assert!(matches!(
                next().await,
                NetworkEvent::EntityData {
                    id: 7,
                    index: 27,
                    value: MetaValue::Byte(0)
                }
            ));
        };
        tokio::join!(delivery, receive);
        assert!(rx.is_empty());
        for end in 0..raw.len() {
            if end >= raw.windows(2).position(|b| b == [0xc2, 0xa7]).unwrap() + 2 {
                assert!(
                    preserve_legacy_display_text(&raw[..end]).is_err(),
                    "truncated at {end}"
                );
            }
        }
        let mut trailing = raw.clone();
        trailing.push(0);
        assert!(preserve_legacy_display_text(&trailing).is_err());
        assert!(preserve_legacy_display_text(&remaining).unwrap().is_none());
    }

    fn direct_sound() -> Holder<SoundEvent, CustomSound> {
        Holder::Direct(CustomSound {
            sound_id: Identifier::new("minecraft:test.ui"),
            range: None,
        })
    }

    #[test]
    fn raw_sound_preserves_ui_source_ordinal() {
        let mut raw = Vec::new();
        wire::write_varint(&mut raw, sound_packet_ids().sound);
        direct_sound().azalea_write(&mut raw).unwrap();
        wire::write_varint(&mut raw, UI_SOUND_SOURCE);
        8_i32.azalea_write(&mut raw).unwrap();
        16_i32.azalea_write(&mut raw).unwrap();
        24_i32.azalea_write(&mut raw).unwrap();
        0.75_f32.azalea_write(&mut raw).unwrap();
        1.25_f32.azalea_write(&mut raw).unwrap();
        7_u64.azalea_write(&mut raw).unwrap();

        let (tx, rx) = crossbeam_channel::bounded(1);
        assert!(handle_raw_game_packet(&raw, &tx));
        match rx.recv().unwrap() {
            NetworkEvent::PlaySound { category, pos, .. } => {
                assert_eq!(category, 10);
                assert_eq!(pos, Position::new(1.0, 2.0, 3.0));
            }
            _ => panic!("expected PlaySound"),
        }
    }

    #[test]
    fn raw_entity_sound_preserves_ui_source_ordinal() {
        let mut raw = Vec::new();
        wire::write_varint(&mut raw, sound_packet_ids().sound_entity);
        direct_sound().azalea_write(&mut raw).unwrap();
        wire::write_varint(&mut raw, UI_SOUND_SOURCE);
        42_i32.azalea_write_var(&mut raw).unwrap();
        1.0_f32.azalea_write(&mut raw).unwrap();
        0.5_f32.azalea_write(&mut raw).unwrap();
        9_u64.azalea_write(&mut raw).unwrap();

        let (tx, rx) = crossbeam_channel::bounded(1);
        assert!(handle_raw_game_packet(&raw, &tx));
        match rx.recv().unwrap() {
            NetworkEvent::PlayEntitySound {
                category,
                entity_id,
                ..
            } => {
                assert_eq!(category, 10);
                assert_eq!(entity_id, 42);
            }
            _ => panic!("expected PlayEntitySound"),
        }
    }

    #[test]
    fn raw_stop_sound_preserves_ui_source_ordinal() {
        let mut raw = Vec::new();
        wire::write_varint(&mut raw, sound_packet_ids().stop_sound);
        let mut flags = FixedBitSet::<2>::new();
        flags.set(0);
        flags.set(1);
        flags.azalea_write(&mut raw).unwrap();
        wire::write_varint(&mut raw, UI_SOUND_SOURCE);
        Identifier::new("minecraft:test.ui")
            .azalea_write(&mut raw)
            .unwrap();

        let (tx, rx) = crossbeam_channel::bounded(1);
        assert!(handle_raw_game_packet(&raw, &tx));
        match rx.recv().unwrap() {
            NetworkEvent::StopSound { sound_id, category } => {
                assert_eq!(category, Some(10));
                assert_eq!(sound_id.as_deref(), Some("minecraft:test.ui"));
            }
            _ => panic!("expected StopSound"),
        }
    }
}

#[cfg(test)]
mod scoreboard_display_event_tests {
    use azalea_protocol::packets::game::c_set_display_objective::DisplaySlot as Slot;

    use crate::net::NetworkEvent;

    #[test]
    fn display_event_retains_representative_slots_and_objective() {
        for slot in [Slot::List, Slot::Sidebar, Slot::BelowName, Slot::TeamRed] {
            let event = NetworkEvent::ScoreboardDisplay {
                slot,
                name: Some("objective".to_owned()),
            };
            assert!(
                matches!(event, NetworkEvent::ScoreboardDisplay { slot: got, name: Some(ref name) } if got as usize == slot as usize && name == "objective")
            );
        }
    }
}

#[cfg(test)]
mod dimension_info_tests {
    use std::collections::HashMap;

    use simdnbt::owned::NbtTag;

    use super::{dimension_clock_id, dimension_info, parse_set_objective};
    use crate::net::NetworkEvent;
    use crate::world::block::model::CardinalLightType;

    #[test]
    fn probe_dimension_info_preserves_debug_and_cardinal_light() {
        let dim = azalea_core::registry_holder::dimension_type::DimensionKindElement {
            height: 384,
            min_y: -64,
            ultrawarm: None,
            _extra: HashMap::from([
                ("has_skylight".to_string(), NbtTag::Byte(1)),
                (
                    "cardinal_light".to_string(),
                    NbtTag::String("nether".into()),
                ),
            ]),
        };

        let NetworkEvent::DimensionInfo {
            height,
            min_y,
            has_skylight,
            cardinal_light,
            is_debug,
            clock_id,
            environment_input,
            ..
        } = dimension_info(
            &dim,
            true,
            None,
            "minecraft:overworld",
            &std::sync::Arc::new(Vec::new()),
            Some("timeline registry snapshot unavailable"),
            None,
        )
        else {
            panic!("dimension_info returned the wrong event variant");
        };
        assert!(is_debug);
        assert_eq!(clock_id, None);
        assert_eq!(height, 384);
        assert_eq!(min_y, -64);
        assert!(has_skylight);
        assert_eq!(cardinal_light, CardinalLightType::Nether);
        assert_eq!(environment_input.ambient_light, None);
    }

    #[test]
    fn end_weather_uses_world_key_not_dimension_type() {
        let dim = azalea_core::registry_holder::dimension_type::DimensionKindElement {
            height: 384,
            min_y: -64,
            ultrawarm: None,
            _extra: HashMap::from([
                ("has_skylight".to_string(), NbtTag::Byte(1)),
                ("has_ceiling".to_string(), NbtTag::Byte(0)),
                ("ambient_light".to_string(), NbtTag::Float(0.25)),
            ]),
        };
        let end = dimension_info(
            &dim,
            false,
            None,
            "minecraft:the_end",
            &std::sync::Arc::new(Vec::new()),
            Some("timeline registry snapshot unavailable"),
            None,
        );
        let custom_world_using_end_type = dimension_info(
            &dim,
            false,
            None,
            "example:custom",
            &std::sync::Arc::new(Vec::new()),
            Some("timeline registry snapshot unavailable"),
            None,
        );
        let NetworkEvent::DimensionInfo {
            environment_input: end,
            ..
        } = end
        else {
            panic!("wrong event");
        };
        let NetworkEvent::DimensionInfo {
            environment_input: custom,
            ..
        } = custom_world_using_end_type
        else {
            panic!("wrong event");
        };
        assert!(end.is_end_world);
        assert!(!custom.is_end_world);
        assert_eq!(end.ambient_light, Some(0.25));
        assert_eq!(custom.ambient_light, Some(0.25));
    }

    #[test]
    fn set_objective_optional_number_formats_decode_without_shifting() {
        use azalea_buf::{AzBuf, AzBufVar};
        use azalea_chat::numbers::NumberFormat;
        use azalea_protocol::packets::game::c_set_objective::MethodKind;
        use pomme_protocol::{Direction, PacketTable, Phase};

        let packet_id = PacketTable::native()
            .id(Phase::Game, Direction::Clientbound, "set_objective")
            .unwrap();
        let formats = [
            None,
            Some(NumberFormat::Blank),
            Some(NumberFormat::Styled {
                style: simdnbt::owned::Nbt::new(
                    "".into(),
                    simdnbt::owned::NbtCompound::from_values(vec![(
                        "color".into(),
                        simdnbt::owned::NbtTag::String("red".into()),
                    )]),
                ),
            }),
            Some(NumberFormat::Fixed {
                value: azalea_chat::FormattedText::from("fixed"),
            }),
        ];

        for (index, format) in formats.into_iter().enumerate() {
            let mut bytes = Vec::new();
            pomme_protocol::wire::write_varint(&mut bytes, packet_id);
            "objective".to_string().azalea_write(&mut bytes).unwrap();
            let method = if index < 2 {
                MethodKind::Add
            } else {
                MethodKind::Change
            };
            method.azalea_write(&mut bytes).unwrap();
            azalea_chat::FormattedText::from("Deaths")
                .azalea_write(&mut bytes)
                .unwrap();
            let render_type = if index % 2 == 0 {
                azalea_core::objectives::ObjectiveCriteria::Integer
            } else {
                azalea_core::objectives::ObjectiveCriteria::Hearts
            };
            render_type.azalea_write(&mut bytes).unwrap();
            let format_at = bytes.len();
            format.is_some().azalea_write(&mut bytes).unwrap();
            if let Some(NumberFormat::Styled { style }) = &format {
                azalea_registry::builtin::NumberFormatKind::Styled
                    .azalea_write(&mut bytes)
                    .unwrap();
                // Style.Serializer.TRUSTED_STREAM_CODEC writes unnamed network NBT.
                style.write_unnamed(&mut bytes);
            } else if let Some(format) = &format {
                format.azalea_write(&mut bytes).unwrap();
            }
            let expected_prefix: &[u8] = match &format {
                None => &[0],
                Some(NumberFormat::Blank) => &[1, 0],
                Some(NumberFormat::Styled { .. }) => &[1, 1],
                Some(NumberFormat::Fixed { .. }) => &[1, 2],
            };
            assert_eq!(
                &bytes[format_at..format_at + expected_prefix.len()],
                expected_prefix,
                "wire fixture {index}"
            );

            let mut cursor = std::io::Cursor::new(bytes.as_slice());
            let _ = u32::azalea_read_var(&mut cursor).unwrap();
            let NetworkEvent::ScoreboardObjective {
                name,
                display,
                number_format,
                render_type: actual_render_type,
            } = parse_set_objective(&mut cursor)
                .unwrap_or_else(|error| panic!("fixture {index}: {error}"))
            else {
                panic!("wrong objective event");
            };
            assert_eq!(name, "objective", "fixture {index}");
            assert_eq!(display.unwrap()[0].text, "Deaths", "fixture {index}");
            assert_eq!(actual_render_type, Some(render_type), "fixture {index}");
            assert!(
                matches!(
                    (&format, &number_format),
                    (None, None)
                        | (
                            Some(NumberFormat::Blank),
                            Some(crate::ui::hud::ScoreNumberFormat::Blank)
                        )
                        | (
                            Some(NumberFormat::Styled { .. }),
                            Some(crate::ui::hud::ScoreNumberFormat::Styled(_))
                        )
                        | (
                            Some(NumberFormat::Fixed { .. }),
                            Some(crate::ui::hud::ScoreNumberFormat::Fixed(_))
                        )
                ),
                "fixture {index}"
            );
            assert_eq!(cursor.position() as usize, bytes.len(), "fixture {index}");
        }
    }

    #[test]
    fn set_objective_remove_has_no_render_type() {
        use azalea_buf::{AzBuf, AzBufVar};
        use azalea_protocol::packets::game::c_set_objective::MethodKind;
        use pomme_protocol::{Direction, PacketTable, Phase};

        let mut bytes = Vec::new();
        let packet_id = PacketTable::native()
            .id(Phase::Game, Direction::Clientbound, "set_objective")
            .unwrap();
        pomme_protocol::wire::write_varint(&mut bytes, packet_id);
        "objective".to_string().azalea_write(&mut bytes).unwrap();
        MethodKind::Remove.azalea_write(&mut bytes).unwrap();
        let mut cursor = std::io::Cursor::new(bytes.as_slice());
        let _ = u32::azalea_read_var(&mut cursor).unwrap();
        let NetworkEvent::ScoreboardObjective {
            display,
            number_format,
            render_type,
            ..
        } = parse_set_objective(&mut cursor).unwrap()
        else {
            panic!("wrong objective event");
        };
        assert!(display.is_none());
        assert!(number_format.is_none());
        assert!(render_type.is_none());
        assert_eq!(cursor.position() as usize, bytes.len());
    }

    #[test]
    fn world_clock_dimension_id_and_wire_map_keep_missing_nbt_ordinal() {
        let ids = [
            "minecraft:overworld".into(),
            "custom:missing".into(),
            "custom:clock".into(),
        ];
        let dim = azalea_core::registry_holder::dimension_type::DimensionKindElement {
            height: 384,
            min_y: -64,
            ultrawarm: None,
            _extra: HashMap::from([(
                "default_clock".to_string(),
                simdnbt::owned::NbtTag::String("custom:clock".into()),
            )]),
        };
        assert_eq!(dimension_clock_id(Some(&ids), &dim), Some(2));
        let NetworkEvent::DimensionInfo {
            world_clock_ids,
            clock_id,
            ..
        } = dimension_info(
            &dim,
            false,
            Some(2),
            "custom:dimension",
            &std::sync::Arc::new(Vec::new()),
            Some("timeline registry snapshot unavailable"),
            Some(&ids),
        )
        else {
            panic!("wrong event");
        };
        let world_clock_ids = world_clock_ids.unwrap();
        assert_eq!(world_clock_ids["custom:clock"], 2);
        assert_eq!(clock_id, Some(2));
    }

    #[test]
    fn probe_dimension_info_defaults_to_normal_world_and_cardinal_light() {
        let dim = azalea_core::registry_holder::dimension_type::DimensionKindElement {
            height: 384,
            min_y: -64,
            ultrawarm: None,
            _extra: HashMap::new(),
        };

        let NetworkEvent::DimensionInfo {
            has_skylight,
            cardinal_light,
            is_debug,
            ..
        } = dimension_info(
            &dim,
            false,
            None,
            "minecraft:overworld",
            &std::sync::Arc::new(Vec::new()),
            Some("timeline registry snapshot unavailable"),
            None,
        )
        else {
            panic!("dimension_info returned the wrong event variant");
        };
        assert!(has_skylight);
        assert!(!is_debug);
        assert_eq!(cardinal_light, CardinalLightType::Default);
    }
}

#[cfg(test)]
mod team_sidebar_slot_tests {
    use azalea_chat::style::ChatFormatting as C;
    use azalea_protocol::packets::game::c_set_display_objective::DisplaySlot as S;

    use super::team_sidebar_slot;

    #[test]
    fn all_team_colors_map_to_their_protocol_slots_and_reset_has_none() {
        let colors = [
            C::Black,
            C::DarkBlue,
            C::DarkGreen,
            C::DarkAqua,
            C::DarkRed,
            C::DarkPurple,
            C::Gold,
            C::Gray,
            C::DarkGray,
            C::Blue,
            C::Green,
            C::Aqua,
            C::Red,
            C::LightPurple,
            C::Yellow,
            C::White,
        ];
        let slots = [
            S::TeamBlack,
            S::TeamDarkBlue,
            S::TeamDarkGreen,
            S::TeamDarkAqua,
            S::TeamDarkRed,
            S::TeamDarkPurple,
            S::TeamGold,
            S::TeamGray,
            S::TeamDarkGray,
            S::TeamBlue,
            S::TeamGreen,
            S::TeamAqua,
            S::TeamRed,
            S::TeamLightPurple,
            S::TeamYellow,
            S::TeamWhite,
        ];
        for (color, slot) in colors.into_iter().zip(slots) {
            assert_eq!(team_sidebar_slot(color), Some(slot));
        }
        assert_eq!(team_sidebar_slot(C::Reset), None);
    }
}
#[cfg(test)]
mod styled_number_format_tests {
    use azalea_buf::AzBuf;
    use azalea_chat::numbers::NumberFormat;
    use simdnbt::owned::{Nbt, NbtCompound, NbtTag};

    use super::score_number_format;

    fn style(fields: Vec<(&str, NbtTag)>) -> Nbt {
        Nbt::new(
            "".into(),
            NbtCompound::from_values(
                fields
                    .into_iter()
                    .map(|(key, value)| (key.to_owned().into(), value))
                    .collect(),
            ),
        )
    }

    #[test]
    fn styled_format_preserves_full_style_on_only_the_numeric_literal() {
        let format = NumberFormat::Styled {
            style: style(vec![
                ("bold", NbtTag::Byte(1)),
                ("italic", NbtTag::Byte(1)),
                ("underlined", NbtTag::Byte(1)),
                ("strikethrough", NbtTag::Byte(1)),
                ("obfuscated", NbtTag::Byte(1)),
                ("font", NbtTag::String("minecraft:uniform".into())),
                ("insertion", NbtTag::String("inserted".into())),
                ("shadow_color", NbtTag::Int(0x80112233u32 as i32)),
                ("text", NbtTag::String("MUST_NOT_RENDER".into())),
                ("extra", NbtTag::String("NOR_THIS".into())),
            ]),
        };
        let parsed = score_number_format(&format).unwrap();
        let spans = crate::ui::hud::format_score_number(73, Some(&parsed), [1.0, 1.0, 0.33, 1.0]);
        assert_eq!(spans.len(), 1);
        let span = &spans[0];
        assert_eq!(span.text, "73");
        assert_eq!(span.color, [1.0; 4]);
        assert!(
            span.bold && span.italic && span.underline && span.strikethrough && span.obfuscated
        );
        assert_eq!(span.font.as_deref(), Some("minecraft:uniform"));
        assert!(span.shadow_color.is_some());
        let component_style = span.component_style.as_ref().unwrap();
        assert!(component_style.bold && component_style.italic);
        assert!(component_style.underlined && component_style.strikethrough);
        assert!(component_style.obfuscated);
        assert!(component_style.font.is_some() && component_style.shadow_color.is_some());
        assert_eq!(component_style.insertion.as_deref(), Some("inserted"));

        let colored = NumberFormat::Styled {
            style: style(vec![("color", NbtTag::String("red".into()))]),
        };
        let parsed = score_number_format(&colored).unwrap();
        let spans = crate::ui::hud::format_score_number(9, Some(&parsed), [1.0, 1.0, 0.33, 1.0]);
        assert_eq!(spans[0].color, crate::ui::common::rgb(0xff5555));
    }

    #[test]
    fn absent_or_malformed_styled_nbt_is_rejected() {
        let absent = NumberFormat::Styled { style: Nbt::None };
        assert!(score_number_format(&absent).is_err());
        // Only the typed objective decoder's documented blank-recovery path
        // treats its shifted empty style as Blank.
        assert!(matches!(
            super::objective_number_format(&absent),
            Ok(Some(crate::ui::hud::ScoreNumberFormat::Blank))
        ));

        let malformed = NumberFormat::Styled {
            style: style(vec![("bold", NbtTag::String("not-a-bool".into()))]),
        };
        assert!(score_number_format(&malformed).is_err());

        let mut payload = Vec::new();
        "objective".to_string().azalea_write(&mut payload).unwrap();
        azalea_protocol::packets::game::c_set_objective::MethodKind::Add
            .azalea_write(&mut payload)
            .unwrap();
        azalea_chat::FormattedText::from("Deaths")
            .azalea_write(&mut payload)
            .unwrap();
        azalea_core::objectives::ObjectiveCriteria::Integer
            .azalea_write(&mut payload)
            .unwrap();
        true.azalea_write(&mut payload).unwrap();
        malformed.azalea_write(&mut payload).unwrap();
        let mut cursor = std::io::Cursor::new(payload.as_slice());
        assert!(super::parse_set_objective(&mut cursor).is_err());
    }
}
