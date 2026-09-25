use azalea_buf::{AzBuf, AzBufVar};
use azalea_core::bitset::FixedBitSet;
use azalea_core::position::{BlockPos, ChunkPos};
use azalea_core::registry_holder::RegistryHolder;
use azalea_core::sound::CustomSound;
use azalea_protocol::packets::game::{ClientboundGamePacket, ServerboundGamePacket};
use azalea_registry::builtin::{EntityKind, SoundEvent};
use azalea_registry::identifier::Identifier;
use azalea_registry::{DataRegistry, Holder, Registry};
use crossbeam_channel::Sender;

use super::NetworkEvent;
use super::chat_security::ProfileKeyServices;
use super::commands::{CommandTree, SharedCommandTree};
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
    registries: &RegistryHolder,
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
    let Some(registry) = registries.extra.get(&Identifier::new("world_clock")) else {
        tracing::warn!("Server omitted minecraft:world_clock registry; world clock is unknown");
        return None;
    };
    let Some(id) = registry
        .map
        .iter()
        .position(|(key, _)| key.to_string() == clock_name)
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
) -> NetworkEvent {
    NetworkEvent::DimensionInfo {
        is_debug,
        height: dim.height,
        min_y: dim.min_y,
        clock_id,
        has_skylight: dim
            ._extra
            .get("has_skylight")
            .and_then(|tag| tag.byte())
            .map(|b| b != 0)
            .unwrap_or(true),
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

pub fn handle_game_packet(
    packet: &ClientboundGamePacket,
    sender: &PacketSender,
    event_tx: &Sender<NetworkEvent>,
    registry_holder: &RegistryHolder,
    shared_tree: &SharedCommandTree,
    batch_size_calculator: &mut ChunkBatchSizeCalculator,
    server_cookies: &mut std::collections::HashMap<
        azalea_registry::identifier::Identifier,
        Vec<u8>,
    >,
) {
    match packet {
        ClientboundGamePacket::Login(p) => {
            if let Some((_, dim)) = p.common.dimension_type(registry_holder) {
                let _ = event_tx.try_send(dimension_info(
                    dim,
                    p.common.is_debug,
                    dimension_clock_id(registry_holder, dim),
                ));
            }
            let _ = event_tx.try_send(NetworkEvent::DimensionName {
                name: p.common.dimension.to_string(),
            });
            let _ = event_tx.try_send(NetworkEvent::GameModeChanged {
                game_mode: p.common.game_type as u8,
                previous: Some(p.common.previous_game_type.0.map(|m| m.to_id())),
            });
            let _ = event_tx.try_send(NetworkEvent::ServerViewDistance {
                distance: p.chunk_radius,
            });
            let _ = event_tx.try_send(NetworkEvent::ServerSimulationDistance {
                distance: p.simulation_distance,
            });
            let _ = event_tx.try_send(NetworkEvent::PlayerLogin {
                entity_id: p.player_id.0,
                hardcore: p.hardcore,
                show_death_screen: p.show_death_screen,
                online_mode: p.online_mode,
            });
            let _ = event_tx.try_send(NetworkEvent::SecureChatEnforced {
                enforced: ProfileKeyServices::get().is_some() && p.enforces_secure_chat,
            });
        }
        ClientboundGamePacket::ChunksBiomes(p) => {
            for chunk in &p.chunk_biome_data {
                let _ = event_tx.try_send(NetworkEvent::ChunkBiomes {
                    pos: chunk.pos,
                    data: chunk.buffer.clone(),
                });
            }
        }
        ClientboundGamePacket::LevelChunkWithLight(p) => {
            tracing::trace!(
                "Chunk [{}, {}] ({} block entities)",
                p.x,
                p.z,
                p.chunk_data.block_entities.len()
            );
            let _ = event_tx.try_send(NetworkEvent::ChunkLoaded {
                pos: ChunkPos::new(p.x, p.z),
                data: p.chunk_data.data.clone(),
                heightmaps: p.chunk_data.heightmaps.clone(),
                light: (&p.light_data).into(),
            });
            let chunk_pos = ChunkPos::new(p.x, p.z);
            let entries: Vec<_> = p
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
                        simdnbt::owned::Nbt::None => simdnbt::owned::NbtCompound::default(),
                    };
                    (block_pos, be.kind, compound)
                })
                .collect();
            let _ = event_tx.try_send(NetworkEvent::BlockEntitySync { chunk_pos, entries });
        }
        ClientboundGamePacket::BlockEvent(p) => {
            let _ = event_tx.try_send(NetworkEvent::BlockEvent {
                pos: p.pos,
                action_id: p.action_id,
                action_parameter: p.action_parameter,
            });
        }
        ClientboundGamePacket::Explode(p) => {
            let _ = event_tx.try_send(NetworkEvent::Explosion(super::ExplosionPayload {
                center: p.center,
                radius: p.radius,
                block_count: p.block_count,
                player_knockback: p.player_knockback,
                explosion_particle: p.explosion_particle.clone(),
                explosion_sound: crate::audio::SoundRef::event(p.explosion_sound.to_str()),
                block_particles: p.block_particles.clone(),
            }));
        }
        ClientboundGamePacket::Sound(p) => {
            // Coordinates are fixed-point: block position times 8.
            let _ = event_tx.try_send(NetworkEvent::PlaySound {
                sound: crate::audio::SoundRef::resolve(&p.sound),
                category: p.source as u8,
                pos: Position::new(p.x as f64 / 8.0, p.y as f64 / 8.0, p.z as f64 / 8.0),
                volume: p.volume,
                pitch: p.pitch,
                seed: p.seed,
            });
        }
        ClientboundGamePacket::SoundEntity(p) => {
            let _ = event_tx.try_send(NetworkEvent::PlayEntitySound {
                sound: crate::audio::SoundRef::resolve(&p.sound),
                category: p.source as u8,
                entity_id: p.id.0,
                volume: p.volume,
                pitch: p.pitch,
                seed: p.seed,
            });
        }
        ClientboundGamePacket::StopSound(p) => {
            let _ = event_tx.try_send(NetworkEvent::StopSound {
                sound_id: p.name.as_ref().map(ToString::to_string),
                category: p.source.map(|source| source as u8),
            });
        }
        ClientboundGamePacket::BlockEntityData(p) => {
            let nbt = match &p.tag {
                simdnbt::owned::Nbt::Some(base) => Some(base.clone().as_compound()),
                simdnbt::owned::Nbt::None => None,
            };
            let _ = event_tx.try_send(NetworkEvent::BlockEntityUpdate {
                pos: p.pos,
                kind: p.block_entity_type,
                nbt,
            });
        }
        ClientboundGamePacket::LightUpdate(p) => {
            let _ = event_tx.try_send(NetworkEvent::LightUpdate {
                pos: ChunkPos::new(p.x, p.z),
                light: (&p.light_data).into(),
            });
        }
        ClientboundGamePacket::ForgetLevelChunk(p) => {
            let _ = event_tx.try_send(NetworkEvent::ChunkUnloaded { pos: p.pos });
        }
        ClientboundGamePacket::SetChunkCacheCenter(p) => {
            let _ = event_tx.try_send(NetworkEvent::ChunkCacheCenter { x: p.x, z: p.z });
        }
        ClientboundGamePacket::PlayerPosition(p) => {
            let _ = event_tx.try_send(NetworkEvent::PlayerPosition {
                id: p.id,
                change: p.change.clone(),
                relative: p.relative.clone(),
            });
        }
        ClientboundGamePacket::PlayerRotation(p) => {
            let _ = event_tx.try_send(NetworkEvent::PlayerRotation {
                y_rot: p.y_rot,
                x_rot: p.x_rot,
                relative_y: p.relative_y,
                relative_x: p.relative_x,
            });
        }
        ClientboundGamePacket::KeepAlive(p) => {
            sender.send(ServerboundGamePacket::KeepAlive(
                azalea_protocol::packets::game::s_keep_alive::ServerboundKeepAlive { id: p.id },
            ));
        }
        ClientboundGamePacket::Ping(p) => {
            sender.send(ServerboundGamePacket::Pong(
                azalea_protocol::packets::game::s_pong::ServerboundPong { id: p.id },
            ));
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
            let _ = event_tx.try_send(NetworkEvent::ContainerContent {
                container_id: p.container_id,
                items: p.items.clone(),
                carried: p.carried_item.clone(),
                state_id: p.state_id,
            });
        }
        ClientboundGamePacket::SetCursorItem(p) => {
            let _ = event_tx.try_send(NetworkEvent::CursorItem {
                item: p.contents.clone(),
            });
        }
        ClientboundGamePacket::ContainerSetSlot(p) => {
            let _ = event_tx.try_send(NetworkEvent::ContainerSlot {
                container_id: p.container_id,
                index: p.slot,
                item: p.item_stack.clone(),
                state_id: p.state_id,
            });
        }
        ClientboundGamePacket::SetHeldSlot(p) if (0..9).contains(&p.slot) => {
            let _ = event_tx.try_send(NetworkEvent::HeldSlot { slot: p.slot as u8 });
        }
        ClientboundGamePacket::ContainerSetData(p) => {
            let _ = event_tx.try_send(NetworkEvent::ContainerData {
                container_id: p.container_id,
                id: p.id,
                value: p.value,
            });
        }
        ClientboundGamePacket::MerchantOffers(p) => {
            let _ = event_tx.try_send(NetworkEvent::MerchantOffers {
                container_id: p.container_id,
                offers: p.offers.clone(),
                villager_level: p.villager_level,
                villager_xp: p.villager_xp,
                show_progress: p.show_progress,
                can_restock: p.can_restock,
            });
        }
        ClientboundGamePacket::MountScreenOpen(p) => {
            let _ = event_tx.try_send(NetworkEvent::MountScreenOpen {
                container_id: p.container_id,
                inventory_columns: p.inventory_columns,
                entity_id: p.entity_id.0,
            });
        }
        ClientboundGamePacket::InitializeBorder(p) => {
            let _ = event_tx.try_send(NetworkEvent::WorldBorderInitialize {
                center_x: p.new_center_x,
                center_z: p.new_center_z,
                old_size: p.old_size,
                new_size: p.new_size,
                lerp_time: i64::try_from(p.lerp_time).unwrap_or(i64::MAX),
                absolute_max_size: p.new_absolute_max_size as i32,
                warning_blocks: p.warning_blocks as i32,
                warning_time: p.warning_time as i32,
            });
        }
        ClientboundGamePacket::SetBorderCenter(p) => {
            let _ = event_tx.try_send(NetworkEvent::WorldBorderCenter {
                x: p.new_center_x,
                z: p.new_center_z,
            });
        }
        ClientboundGamePacket::SetBorderSize(p) => {
            let _ = event_tx.try_send(NetworkEvent::WorldBorderSize { size: p.size });
        }
        ClientboundGamePacket::SetBorderLerpSize(p) => {
            let _ = event_tx.try_send(NetworkEvent::WorldBorderLerpSize {
                old_size: p.old_size,
                new_size: p.new_size,
                lerp_time: i64::try_from(p.lerp_time).unwrap_or(i64::MAX),
            });
        }
        ClientboundGamePacket::SetBorderWarningDistance(p) => {
            let _ = event_tx.try_send(NetworkEvent::WorldBorderWarningBlocks {
                warning_blocks: p.warning_blocks as i32,
            });
        }
        ClientboundGamePacket::SetBorderWarningDelay(p) => {
            let _ = event_tx.try_send(NetworkEvent::WorldBorderWarningTime {
                warning_time: p.warning_delay as i32,
            });
        }
        ClientboundGamePacket::OpenScreen(p) => {
            let _ = event_tx.try_send(NetworkEvent::OpenScreen {
                container_id: p.container_id,
                menu_type: p.menu_type,
                title: p.title.to_string(),
            });
        }
        ClientboundGamePacket::OpenBook(p) => {
            let _ = event_tx.try_send(NetworkEvent::OpenBook { hand: p.hand });
        }
        ClientboundGamePacket::ContainerClose(_) => {
            let _ = event_tx.try_send(NetworkEvent::ContainerClosed);
        }
        ClientboundGamePacket::SetHealth(p) => {
            let _ = event_tx.try_send(NetworkEvent::PlayerHealth {
                health: p.health,
                food: p.food,
                saturation: p.saturation,
            });
        }
        ClientboundGamePacket::SetExperience(p) => {
            let _ = event_tx.try_send(NetworkEvent::PlayerExperience {
                progress: p.experience_progress,
                level: p.experience_level as i32,
                total_experience: p.total_experience,
            });
        }
        ClientboundGamePacket::SetPlayerInventory(p) => {
            let _ = event_tx.try_send(NetworkEvent::SetPlayerInventory {
                slot: p.slot,
                item: p.contents.clone(),
            });
        }
        ClientboundGamePacket::UpdateMobEffect(p) => {
            let _ = event_tx.try_send(NetworkEvent::UpdateMobEffect {
                entity_id: p.entity_id.0,
                effect: crate::mob_effect::MobEffectInstance {
                    effect_id: p.mob_effect.to_u32(),
                    amplifier: p.data.amplifier.clamp(0, u8::MAX as i32) as u8,
                    duration: p.data.duration,
                    ambient: p.data.flags.ambient,
                    show_particles: p.data.flags.show_particles,
                    show_icon: p.data.flags.show_icon,
                },
            });
        }
        ClientboundGamePacket::RemoveMobEffect(p) => {
            let _ = event_tx.try_send(NetworkEvent::RemoveMobEffect {
                entity_id: p.entity_id.0,
                effect_id: p.effect.to_u32(),
            });
        }
        ClientboundGamePacket::Waypoint(p) => {
            let _ = event_tx.try_send(NetworkEvent::Waypoint {
                operation: p.operation,
                waypoint: p.waypoint.clone(),
            });
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
            let _ = event_tx.try_send(NetworkEvent::MapItemData {
                map_id: p.map_id,
                scale: p.scale,
                locked: p.locked,
                patch,
                decorations,
            });
        }
        ClientboundGamePacket::UpdateAttributes(p) => {
            use azalea_core::attribute_modifier_operation::AttributeModifierOperation;
            use azalea_registry::builtin::Attribute;
            for snapshot in &p.values {
                let base = snapshot.base;
                let mut add = 0.0f64;
                let mut mul_base = 0.0f64;
                let mut mul_total = 1.0f64;
                for m in &snapshot.modifiers {
                    match m.operation {
                        AttributeModifierOperation::AddValue => add += m.amount,
                        AttributeModifierOperation::AddMultipliedBase => mul_base += m.amount,
                        AttributeModifierOperation::AddMultipliedTotal => {
                            mul_total *= 1.0 + m.amount
                        }
                    }
                }
                let value = (base + add) * (1.0 + mul_base) * mul_total;
                let event = match snapshot.attribute {
                    Attribute::Armor => NetworkEvent::EntityArmorUpdate {
                        entity_id: p.entity_id.0,
                        armor: value.clamp(0.0, 30.0).round() as u32,
                    },
                    // Vanilla RangedAttribute MAX_HEALTH clamps to 1..1024.
                    Attribute::MaxHealth => NetworkEvent::EntityMaxHealthUpdate {
                        entity_id: p.entity_id.0,
                        max_health: value.clamp(1.0, 1024.0) as f32,
                    },
                    _ => continue,
                };
                let _ = event_tx.try_send(event);
            }
        }
        ClientboundGamePacket::PlayerAbilities(p) => {
            let _ = event_tx.try_send(NetworkEvent::PlayerAbilitiesChanged {
                invulnerable: p.flags.invulnerable,
                flying: p.flags.flying,
                can_fly: p.flags.can_fly,
                instant_break: p.flags.instant_break,
                flying_speed: p.flying_speed,
                walking_speed: p.walking_speed,
            });
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
            let _ = event_tx.try_send(NetworkEvent::BossBarUpdate { id: p.id, op });
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
            let _ = event_tx.try_send(NetworkEvent::AdvancementsUpdate(Box::new(
                toast::AdvancementsUpdate {
                    reset: p.reset,
                    added,
                    removed: p.removed.iter().map(|id| id.to_string()).collect(),
                    progress,
                    show_advancements: p.show_advancements,
                },
            )));
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
                let _ = event_tx.try_send(NetworkEvent::RecipeToastAdd { entries });
            }
            let _ = event_tx.try_send(NetworkEvent::RecipeBookAdd(p.clone()));
        }
        ClientboundGamePacket::RecipeBookRemove(p) => {
            let _ = event_tx.try_send(NetworkEvent::RecipeBookRemove(p.recipes.clone()));
        }
        ClientboundGamePacket::RecipeBookSettings(p) => {
            let _ = event_tx.try_send(NetworkEvent::RecipeBookSettings(p.book_settings.clone()));
        }
        ClientboundGamePacket::UpdateRecipes(p) => {
            let _ = event_tx.try_send(NetworkEvent::UpdateRecipes(p.clone()));
        }
        ClientboundGamePacket::SetTitleText(p) => {
            let _ = event_tx.try_send(NetworkEvent::TitleText {
                spans: format_text_spans(&p.text, [1.0; 4]),
            });
        }
        ClientboundGamePacket::SetSubtitleText(p) => {
            let _ = event_tx.try_send(NetworkEvent::SubtitleText {
                spans: format_text_spans(&p.text, [1.0; 4]),
            });
        }
        ClientboundGamePacket::SetTitlesAnimation(p) => {
            // azalea decodes the fields as u32; vanilla reads signed ints and
            // ignores negatives, so restore the sign before forwarding.
            let _ = event_tx.try_send(NetworkEvent::TitlesAnimation {
                fade_in: p.fade_in as i32,
                stay: p.stay as i32,
                fade_out: p.fade_out as i32,
            });
        }
        ClientboundGamePacket::ClearTitles(p) => {
            let _ = event_tx.try_send(NetworkEvent::ClearTitles {
                reset_times: p.reset_times,
            });
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
                            return;
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
            let _ = event_tx.try_send(NetworkEvent::ScoreboardObjective {
                name: p.objective_name.clone(),
                display,
                number_format,
                render_type,
            });
        }
        ClientboundGamePacket::SetDisplayObjective(p) => {
            let _ = event_tx.try_send(NetworkEvent::ScoreboardDisplay {
                slot: p.slot,
                name: (!p.objective_name.is_empty()).then(|| p.objective_name.clone()),
            });
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
                    return;
                }
            };
            let _ = event_tx.try_send(NetworkEvent::ScoreboardScore {
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
            });
        }
        ClientboundGamePacket::ResetScore(p) => {
            let _ = event_tx.try_send(NetworkEvent::ScoreboardReset {
                owner: p.owner.clone(),
                objective: p.objective_name.clone(),
            });
        }
        ClientboundGamePacket::SetPlayerTeam(p) => {
            use azalea_protocol::packets::game::c_set_player_team::Method;
            match &p.method {
                Method::Add((parameters, members)) => {
                    send_scoreboard_team(event_tx, &p.name, parameters, Some(members.clone()))
                }
                Method::Change(parameters) => {
                    send_scoreboard_team(event_tx, &p.name, parameters, None)
                }
                Method::Join(members) | Method::Leave(members) => {
                    let _ = event_tx.try_send(NetworkEvent::ScoreboardTeamMembers {
                        name: p.name.clone(),
                        members: members.clone(),
                        join: matches!(p.method, Method::Join(_)),
                    });
                }
                Method::Remove => {
                    let _ = event_tx.try_send(NetworkEvent::ScoreboardTeamRemoved {
                        name: p.name.clone(),
                    });
                }
            }
        }
        ClientboundGamePacket::BlockUpdate(p) => {
            let _ = event_tx.try_send(NetworkEvent::BlockUpdate {
                pos: p.pos,
                state: p.block_state,
            });
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
            let _ = event_tx.try_send(NetworkEvent::SectionBlocksUpdate { updates });
        }
        ClientboundGamePacket::BlockChangedAck(p) => {
            let _ = event_tx.try_send(NetworkEvent::BlockChangedAck { seq: p.seq });
        }
        ClientboundGamePacket::TickingState(p) => {
            let _ = event_tx.try_send(NetworkEvent::TickingState {
                tick_rate: p.tick_rate,
                is_frozen: p.is_frozen,
            });
        }
        ClientboundGamePacket::TickingStep(p) => {
            let _ = event_tx.try_send(NetworkEvent::TickingStep {
                tick_steps: p.tick_steps,
            });
        }
        ClientboundGamePacket::SetTime(p) => {
            let clock_updates = p
                .clock_updates
                .iter()
                .map(|(clock, state)| {
                    (
                        clock.protocol_id(),
                        state.total_ticks,
                        state.partial_tick,
                        state.rate,
                    )
                })
                .collect();
            let _ = event_tx.try_send(NetworkEvent::TimeUpdate {
                game_time: p.game_time,
                clock_updates,
                legacy: crate::version::session_protocol()
                    < pomme_protocol::version::NATIVE.protocol,
            });
        }
        ClientboundGamePacket::SetChunkCacheRadius(p) => {
            let _ = event_tx.try_send(NetworkEvent::ServerViewDistance { distance: p.radius });
        }
        ClientboundGamePacket::SetSimulationDistance(p) => {
            let _ = event_tx.try_send(NetworkEvent::ServerSimulationDistance {
                distance: p.simulation_distance,
            });
        }
        ClientboundGamePacket::GameEvent(p) => {
            use azalea_protocol::packets::game::c_game_event::EventType;
            match p.event {
                EventType::ChangeGameMode => {
                    let _ = event_tx.try_send(NetworkEvent::GameModeChanged {
                        game_mode: p.param as u8,
                        previous: None,
                    });
                }
                EventType::WaitForLevelChunks => {
                    let _ = event_tx.try_send(NetworkEvent::LevelChunksLoadStart);
                }
                EventType::StartRaining
                | EventType::StopRaining
                | EventType::RainLevelChange
                | EventType::ThunderLevelChange => {
                    let _ = event_tx.try_send(NetworkEvent::WeatherUpdate {
                        event: p.event,
                        param: p.param,
                    });
                }
                _ => {
                    let _ = event_tx.try_send(NetworkEvent::GameEvent {
                        event: p.event,
                        param: p.param,
                    });
                }
            }
        }
        ClientboundGamePacket::Disconnect(p) => {
            tracing::warn!("Disconnected: {}", p.reason);
            let _ = event_tx.try_send(NetworkEvent::Disconnected {
                reason: format!("{}", p.reason),
            });
        }
        ClientboundGamePacket::AddEntity(p) => {
            let y_rot_deg = (p.y_rot as f32) * 360.0 / 256.0;
            let x_rot_deg = (p.x_rot as f32) * 360.0 / 256.0;
            let head_y_rot_deg = (p.y_head_rot as f32) * 360.0 / 256.0;
            let _ = event_tx.try_send(NetworkEvent::EntitySpawned {
                id: p.id.0,
                uuid: p.uuid,
                entity_type: p.entity_type,
                position: p.position.into(),
                velocity: lp_to_dvec3(&p.movement),
                y_rot_deg,
                x_rot_deg,
                head_y_rot_deg,
            });
        }
        ClientboundGamePacket::DamageEvent(p) => {
            let _ = event_tx.try_send(NetworkEvent::EntityDamaged { id: p.entity_id.0 });
        }
        ClientboundGamePacket::HurtAnimation(p) => {
            let _ = event_tx.try_send(NetworkEvent::HurtAnimation {
                id: p.id.0,
                yaw: p.yaw,
            });
        }
        ClientboundGamePacket::RotateHead(p) => {
            let head_y_rot_deg = (p.y_head_rot as f32) * 360.0 / 256.0;
            let _ = event_tx.try_send(NetworkEvent::EntityHeadRotation {
                id: p.entity_id.0,
                head_y_rot_deg,
            });
        }
        ClientboundGamePacket::MoveEntityPos(p) => {
            send_entity_moved(event_tx, p.entity_id.0, &p.delta, p.on_ground);
        }
        ClientboundGamePacket::MoveEntityPosRot(p) => {
            use azalea_core::delta::PositionDeltaTrait;
            let look: azalea_entity::LookDirection = p.look_direction.into();
            let _ = event_tx.try_send(NetworkEvent::EntityMovedRotated {
                id: p.entity_id.0,
                dx: p.delta.x(),
                dy: p.delta.y(),
                dz: p.delta.z(),
                y_rot_deg: look.y_rot(),
                x_rot_deg: look.x_rot(),
                on_ground: p.on_ground,
            });
        }
        ClientboundGamePacket::MoveEntityRot(p) => {
            let look: azalea_entity::LookDirection = p.look_direction.into();
            let _ = event_tx.try_send(NetworkEvent::EntityRotated {
                id: p.entity_id.0,
                y_rot_deg: look.y_rot(),
                x_rot_deg: look.x_rot(),
                on_ground: p.on_ground,
            });
        }
        ClientboundGamePacket::TeleportEntity(p) => {
            let delta = p.change.delta;
            let _ = event_tx.try_send(NetworkEvent::EntityTeleported {
                id: p.id.0,
                position: p.change.pos.into(),
                relative: Some(p.relative.clone()),
                velocity: Some(glam::DVec3::new(delta.x, delta.y, delta.z)),
                y_rot_deg: p.change.look_direction.y_rot(),
                x_rot_deg: p.change.look_direction.x_rot(),
                on_ground: p.on_ground,
            });
        }
        ClientboundGamePacket::EntityPositionSync(p) => {
            let _ = event_tx.try_send(NetworkEvent::EntityTeleported {
                id: p.id.0,
                position: p.values.pos.into(),
                relative: None,
                velocity: None,
                y_rot_deg: p.values.look_direction.y_rot(),
                x_rot_deg: p.values.look_direction.x_rot(),
                on_ground: p.on_ground,
            });
        }
        ClientboundGamePacket::SetEntityMotion(p) => {
            let _ = event_tx.try_send(NetworkEvent::EntityMotion {
                id: p.id.0,
                velocity: lp_to_dvec3(&p.delta),
            });
        }
        ClientboundGamePacket::LevelEvent(p) => {
            let _ = event_tx.try_send(NetworkEvent::LevelEvent {
                event_type: p.event_type,
                pos: p.pos,
                data: p.data,
            });
        }
        ClientboundGamePacket::RemoveEntities(p) => {
            let ids: Vec<i32> = p.entity_ids.iter().map(|id| id.0).collect();
            let _ = event_tx.try_send(NetworkEvent::EntitiesRemoved { ids });
        }
        ClientboundGamePacket::SetPassengers(p) => {
            let _ = event_tx.try_send(NetworkEvent::SetPassengers {
                vehicle: p.vehicle.0,
                passengers: p.passengers.iter().map(|id| id.0).collect(),
            });
        }
        ClientboundGamePacket::SetEquipment(p) => {
            // Only the saddle slot is tracked; equipment rendering is a TODO.
            for (slot, item) in &p.slots.slots {
                if *slot == azalea_inventory::components::EquipmentSlot::Saddle {
                    let _ = event_tx.try_send(NetworkEvent::EntitySaddle {
                        entity_id: p.entity_id.0,
                        saddled: item.is_present(),
                    });
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
                // index 8 = item stack data for item entities
                if item.index == 8
                    && let azalea_entity::EntityDataValue::ItemStack(
                        azalea_inventory::ItemStack::Present(data),
                    ) = &item.value
                {
                    let name = crate::player::inventory::item_resource_name(data.kind);
                    let damage = data
                        .get_component::<azalea_inventory::components::Damage>()
                        .map(|component| component.amount)
                        .unwrap_or(0);
                    let _ = event_tx.try_send(NetworkEvent::EntityItemData {
                        id: p.id.0,
                        item_name: name,
                        item_id: data.kind.to_u32(),
                        damage,
                        count: data.count,
                    });
                }
                // Index 6 = entity pose
                if item.index == 6
                    && let azalea_entity::EntityDataValue::Pose(pose) = &item.value
                {
                    let _ = event_tx.try_send(NetworkEvent::EntityPose {
                        id: p.id.0,
                        pose: crate::entity::EntityPose::from_vanilla_id(*pose as i32),
                    });
                }
                // Index 14 = LivingEntity SLEEPING_POS (OptionalBlockPos).
                if item.index == 14
                    && let azalea_entity::EntityDataValue::OptionalBlockPos(pos) = &item.value
                {
                    let _ = event_tx.try_send(NetworkEvent::EntitySleepingPos {
                        id: p.id.0,
                        pos: *pos,
                    });
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
                    _ => None,
                };
                if let Some(value) = scalar {
                    let _ = event_tx.try_send(NetworkEvent::EntityData {
                        id: p.id.0,
                        index: item.index,
                        value,
                    });
                }
                // Player score (Int; index gated per wire version above).
                // Kind-blind; the consumer applies it only to the local
                // player.
                if item.index == score_idx
                    && let azalea_entity::EntityDataValue::Int(score) = &item.value
                {
                    let _ = event_tx.try_send(NetworkEvent::PlayerScore {
                        entity_id: p.id.0,
                        score: *score,
                    });
                }
                // Player absorption (Float, Player.DATA_PLAYER_ABSORPTION_ID;
                // index gated per wire version above). Kind-blind; the
                // consumer applies it only to the local player.
                if item.index == absorption_idx
                    && let azalea_entity::EntityDataValue::Float(absorption) = &item.value
                {
                    let _ = event_tx.try_send(NetworkEvent::PlayerAbsorption {
                        entity_id: p.id.0,
                        absorption: *absorption,
                    });
                }
                // Index 2 = custom name (Optional<Component>); needed for jeb_ sheep detection.
                if item.index == 2
                    && let azalea_entity::EntityDataValue::OptionalFormattedText(opt) = &item.value
                {
                    let name = opt.as_ref().map(|c| c.to_string());
                    let _ = event_tx.try_send(NetworkEvent::EntityCustomName { id: p.id.0, name });
                }
                // Index 18 on cows = CowVariant Holder.
                if item.index == 18
                    && let azalea_entity::EntityDataValue::CowVariant(variant) = &item.value
                {
                    let _ = event_tx.try_send(variant_event(
                        registry_holder,
                        p.id.0,
                        EntityKind::Cow,
                        variant,
                    ));
                }
                // Index 18 on chickens = ChickenVariant Holder.
                if item.index == 18
                    && let azalea_entity::EntityDataValue::ChickenVariant(variant) = &item.value
                {
                    let _ = event_tx.try_send(variant_event(
                        registry_holder,
                        p.id.0,
                        EntityKind::Chicken,
                        variant,
                    ));
                }
                // Cat / wolf variant Holders: 20 / 23 on 26.x, one lower on
                // 1.21.9-1.21.11 (no AgeableMob age-locked slot).
                if (item.index == 19 || item.index == 20)
                    && let azalea_entity::EntityDataValue::CatVariant(variant) = &item.value
                {
                    let _ = event_tx.try_send(variant_event(
                        registry_holder,
                        p.id.0,
                        EntityKind::Cat,
                        variant,
                    ));
                }
                if (item.index == 22 || item.index == 23)
                    && let azalea_entity::EntityDataValue::WolfVariant(variant) = &item.value
                {
                    let _ = event_tx.try_send(variant_event(
                        registry_holder,
                        p.id.0,
                        EntityKind::Wolf,
                        variant,
                    ));
                }
                // VillagerData (type/profession/level): villagers at 19 (18
                // on 1.21.9-1.21.11), zombie villagers at 20.
                if (18..=20).contains(&item.index)
                    && let azalea_entity::EntityDataValue::VillagerData(data) = &item.value
                {
                    let _ = event_tx.try_send(NetworkEvent::VillagerData {
                        id: p.id.0,
                        kind: data.kind.into(),
                        profession: data.profession.into(),
                        level: data.level,
                    });
                }
            }
        }
        // Event id 3 = living entity death.
        // TODO: event 60 (`makePoofParticles`) when a mob's death clock hits 20.
        ClientboundGamePacket::EntityEvent(p) if p.event_id == 3 => {
            let _ = event_tx.try_send(NetworkEvent::EntityDied { id: p.entity_id.0 });
        }
        // Event id 9 = finished using an item (vanilla `completeUsingItem`).
        ClientboundGamePacket::EntityEvent(p) if p.event_id == 9 => {
            let _ = event_tx.try_send(NetworkEvent::FinishUseItem { id: p.entity_id.0 });
        }
        // Event id 10 = sheep eat-grass animation start (40-tick head-dip).
        ClientboundGamePacket::EntityEvent(p) if p.event_id == 10 => {
            let _ = event_tx.try_send(NetworkEvent::SheepEatStart { id: p.entity_id.0 });
        }
        // Event id 1 = rabbit jump start (15-tick hop).
        ClientboundGamePacket::EntityEvent(p) if p.event_id == 1 => {
            let _ = event_tx.try_send(NetworkEvent::RabbitJump { id: p.entity_id.0 });
        }
        // Event id 19 = squid tentacle-clock rollover.
        ClientboundGamePacket::EntityEvent(p) if p.event_id == 19 => {
            let _ = event_tx.try_send(NetworkEvent::SquidTentacleReset { id: p.entity_id.0 });
        }
        // Event id 4 = iron golem punch (10-tick swing).
        ClientboundGamePacket::EntityEvent(p) if p.event_id == 4 => {
            let _ = event_tx.try_send(NetworkEvent::GolemPunch { id: p.entity_id.0 });
        }
        // Event id 35 = Totem activation, emitted by the entity that used it.
        ClientboundGamePacket::EntityEvent(p) if p.event_id == 35 => {
            let _ = event_tx.try_send(NetworkEvent::TotemUsed {
                entity_id: p.entity_id.0,
            });
        }
        // Events 11 / 34 = iron golem flower offer start / stop.
        ClientboundGamePacket::EntityEvent(p) if p.event_id == 11 => {
            let _ = event_tx.try_send(NetworkEvent::GolemOfferFlower {
                id: p.entity_id.0,
                offering: true,
            });
        }
        ClientboundGamePacket::EntityEvent(p) if p.event_id == 34 => {
            let _ = event_tx.try_send(NetworkEvent::GolemOfferFlower {
                id: p.entity_id.0,
                offering: false,
            });
        }
        // Events 8 / 56 = wolf wet-shake start / cancel.
        ClientboundGamePacket::EntityEvent(p) if p.event_id == 8 => {
            let _ = event_tx.try_send(NetworkEvent::WolfShaking {
                id: p.entity_id.0,
                shaking: true,
            });
        }
        ClientboundGamePacket::EntityEvent(p) if p.event_id == 56 => {
            let _ = event_tx.try_send(NetworkEvent::WolfShaking {
                id: p.entity_id.0,
                shaking: false,
            });
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
            let _ = event_tx.try_send(NetworkEvent::EntitySwing { id: p.id.0 });
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
            let _ = event_tx.try_send(NetworkEvent::CriticalHit { id: p.id.0, kind });
        }
        // Vanilla handleAnimate action 2 -> stopSleepInBed(false, false).
        ClientboundGamePacket::Animate(p)
            if matches!(
                p.action,
                azalea_protocol::packets::game::c_animate::AnimationAction::WakeUp
            ) =>
        {
            let _ = event_tx.try_send(NetworkEvent::EntityWakeUp { id: p.id.0 });
        }
        ClientboundGamePacket::TakeItemEntity(p) => {
            let _ = event_tx.try_send(NetworkEvent::ItemPickedUp {
                item_id: p.item_id as i32,
                collector_id: p.player_id.0,
                amount: p.amount as i32,
            });
        }
        ClientboundGamePacket::Respawn(p) => {
            let _ = event_tx.try_send(NetworkEvent::PlayerRespawned {
                keep_entity_data: p.data_to_keep & 2 != 0,
                keep_attribute_modifiers: p.data_to_keep & 1 != 0,
            });
            if let Some((_, dim)) = p.common.dimension_type(registry_holder) {
                let _ = event_tx.try_send(dimension_info(
                    dim,
                    p.common.is_debug,
                    dimension_clock_id(registry_holder, dim),
                ));
            }
            let _ = event_tx.try_send(NetworkEvent::DimensionName {
                name: p.common.dimension.to_string(),
            });
            let _ = event_tx.try_send(NetworkEvent::GameModeChanged {
                game_mode: p.common.game_type as u8,
                previous: Some(p.common.previous_game_type.0.map(|m| m.to_id())),
            });
            // Vanilla recreates the player on respawn; the server re-sends any
            // effects kept across it.
            let _ = event_tx.try_send(NetworkEvent::ClearMobEffects);
        }
        ClientboundGamePacket::PlayerCombatKill(p) => {
            tracing::info!("Player died: {}", p.message);
            let _ = event_tx.try_send(NetworkEvent::PlayerDied {
                player_id: p.player_id.0,
                message: p.message.to_string(),
            });
        }
        ClientboundGamePacket::ResourcePackPush(p) => {
            tracing::info!(
                "Server pushing resource pack {} (required: {})",
                p.id,
                p.required
            );
            let _ = event_tx.try_send(NetworkEvent::ResourcePackPush {
                id: p.id,
                url: p.url.clone(),
                hash: p.hash.clone(),
                required: p.required,
            });
            sender.send(ServerboundGamePacket::ResourcePack(
                azalea_protocol::packets::game::s_resource_pack::ServerboundResourcePack {
                    id: p.id,
                    action: azalea_protocol::packets::game::s_resource_pack::Action::Accepted,
                },
            ));
        }
        ClientboundGamePacket::ResourcePackPop(p) => {
            tracing::info!("Server popping resource pack {:?}", p.id);
            let _ = event_tx.try_send(NetworkEvent::ResourcePackPop { id: p.id });
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
            let _ = event_tx.try_send(NetworkEvent::PlayerInfoUpdate { actions, entries });
        }
        ClientboundGamePacket::PlayerInfoRemove(p) => {
            let _ = event_tx.try_send(NetworkEvent::PlayerInfoRemove {
                uuids: p.profile_ids.clone(),
            });
        }
        ClientboundGamePacket::TabList(p) => {
            let _ = event_tx.try_send(NetworkEvent::TabListHeaderFooter {
                header: crate::ui::text::format_text_spans(&p.header, [1.0, 1.0, 1.0, 1.0]),
                footer: crate::ui::text::format_text_spans(&p.footer, [1.0, 1.0, 1.0, 1.0]),
            });
        }
        ClientboundGamePacket::Commands(p) => {
            let tree = std::sync::Arc::new(CommandTree::from_packet(p));
            tracing::info!(
                "Command tree received: {} nodes, root commands = {:?}",
                p.entries.len(),
                tree.root_child_names()
            );
            *shared_tree.lock() = Some(tree.clone());
            let _ = event_tx.try_send(NetworkEvent::CommandTree { tree });
        }
        ClientboundGamePacket::ShowDialog(p) => {
            let _ = event_tx.try_send(NetworkEvent::ShowDialog {
                dialog: dialog_holder_reference(&p.dialog),
            });
        }
        ClientboundGamePacket::ClearDialog(_) => {
            let _ = event_tx.try_send(NetworkEvent::ClearDialog);
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
            let _ = event_tx.try_send(NetworkEvent::CustomChatCompletions {
                action,
                entries: p.entries.clone(),
            });
        }
        _other => {}
    }
}

fn send_scoreboard_team(
    event_tx: &Sender<NetworkEvent>,
    name: &str,
    parameters: &azalea_protocol::packets::game::c_set_player_team::Parameters,
    members: Option<Vec<String>>,
) {
    let color = team_color(parameters.color);
    let sidebar_slot = team_sidebar_slot(parameters.color);
    let _ = event_tx.try_send(NetworkEvent::ScoreboardTeam {
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
    });
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

fn send_entity_moved(
    event_tx: &Sender<NetworkEvent>,
    id: i32,
    delta: &azalea_core::delta::PositionDelta8,
    on_ground: bool,
) {
    let _ = event_tx.try_send(NetworkEvent::EntityMoved {
        id,
        dx: delta.xa as f64 / 4096.0,
        dy: delta.ya as f64 / 4096.0,
        dz: delta.za as f64 / 4096.0,
        on_ground,
    });
}

/// Consume packets that azalea's 26.2 codecs cannot represent correctly
/// before the typed decode runs. Returns whether the packet was consumed.
pub fn handle_raw_game_packet(raw: &[u8], event_tx: &Sender<NetworkEvent>) -> bool {
    let mut cur = std::io::Cursor::new(raw);
    let Ok(packet_id) = u32::azalea_read_var(&mut cur) else {
        return false;
    };

    if Some(packet_id)
        == pomme_protocol::PacketTable::for_protocol(pomme_protocol::version::NATIVE.protocol)
            .and_then(|table| {
                table.id(
                    pomme_protocol::Phase::Game,
                    pomme_protocol::Direction::Clientbound,
                    "open_sign_editor",
                )
            })
    {
        use azalea_buf::AzBuf;
        let result = (|| {
            let pos = BlockPos::azalea_read(&mut cur)?;
            let is_front_text = bool::azalea_read(&mut cur)?;
            Ok::<_, azalea_buf::BufReadError>((pos, is_front_text))
        })();
        match result {
            Ok((pos, is_front_text)) => {
                let _ = event_tx.try_send(NetworkEvent::OpenSignEditor { pos, is_front_text });
            }
            Err(error) => tracing::warn!("Skipping malformed OpenSignEditor packet: {error}"),
        }
        return true;
    }

    if packet_id == cooldown_packet_id() {
        match super::cooldown::decode_payload(&raw[cur.position() as usize..]) {
            Ok(cooldown) => {
                let _ = event_tx.try_send(NetworkEvent::ItemCooldown {
                    group: cooldown.group,
                    duration: cooldown.duration,
                });
            }
            Err(e) => tracing::warn!("Skipping malformed Cooldown packet: {e}"),
        }
        return true;
    }

    if packet_id == set_objective_packet_id() {
        match parse_set_objective(&mut cur) {
            Ok(event) => {
                let _ = event_tx.try_send(event);
            }
            Err(e) => tracing::warn!("Skipping malformed SetObjective packet: {e}"),
        }
        return true;
    }

    let sound_ids = sound_packet_ids();
    let sound_result = if packet_id == sound_ids.sound {
        Some(handle_raw_ui_sound(&mut cur, event_tx))
    } else if packet_id == sound_ids.sound_entity {
        Some(handle_raw_ui_entity_sound(&mut cur, event_tx))
    } else if packet_id == sound_ids.stop_sound {
        Some(handle_raw_ui_stop_sound(&mut cur, event_tx))
    } else {
        None
    };
    if let Some(result) = sound_result {
        return match result {
            Ok(consumed) => consumed,
            Err(e) => {
                tracing::warn!("Skipping malformed sound packet: {e}");
                true
            }
        };
    }

    if packet_id != level_particles_packet_id() {
        return false;
    }
    match parse_level_particles(&mut cur) {
        Ok(Some(event)) => {
            let _ = event_tx.try_send(event);
        }
        Ok(None) => {}
        Err(e) => tracing::warn!("Skipping malformed LevelParticles packet: {e}"),
    }
    true
}

/// Azalea's pinned 26.2 `SoundSource` omits Vanilla's ordinal-10 `UI` value
/// and decodes unknown ordinals as `Master`. Read just that valid ordinal here;
/// ordinals 0..=9 fall through to Azalea's normal typed decoder.
fn handle_raw_ui_sound(
    cur: &mut std::io::Cursor<&[u8]>,
    event_tx: &Sender<NetworkEvent>,
) -> Result<bool, azalea_buf::BufReadError> {
    let mut sound = Holder::<SoundEvent, CustomSound>::azalea_read(cur)?;
    if u32::azalea_read_var(cur)? != UI_SOUND_SOURCE {
        return Ok(false);
    }
    if let Some(translation) = super::translate::active()
        && !translation.remap_sound(&mut sound)
    {
        return Ok(true);
    }
    let x = i32::azalea_read(cur)?;
    let y = i32::azalea_read(cur)?;
    let z = i32::azalea_read(cur)?;
    let volume = f32::azalea_read(cur)?;
    let pitch = f32::azalea_read(cur)?;
    let seed = u64::azalea_read(cur)?;
    let _ = event_tx.try_send(NetworkEvent::PlaySound {
        sound: crate::audio::SoundRef::resolve(&sound),
        category: UI_SOUND_SOURCE as u8,
        pos: Position::new(x as f64 / 8.0, y as f64 / 8.0, z as f64 / 8.0),
        volume,
        pitch,
        seed,
    });
    Ok(true)
}

fn handle_raw_ui_entity_sound(
    cur: &mut std::io::Cursor<&[u8]>,
    event_tx: &Sender<NetworkEvent>,
) -> Result<bool, azalea_buf::BufReadError> {
    let mut sound = Holder::<SoundEvent, CustomSound>::azalea_read(cur)?;
    if u32::azalea_read_var(cur)? != UI_SOUND_SOURCE {
        return Ok(false);
    }
    if let Some(translation) = super::translate::active()
        && !translation.remap_sound(&mut sound)
    {
        return Ok(true);
    }
    let entity_id = i32::azalea_read_var(cur)?;
    let volume = f32::azalea_read(cur)?;
    let pitch = f32::azalea_read(cur)?;
    let seed = u64::azalea_read(cur)?;
    let _ = event_tx.try_send(NetworkEvent::PlayEntitySound {
        sound: crate::audio::SoundRef::resolve(&sound),
        category: UI_SOUND_SOURCE as u8,
        entity_id,
        volume,
        pitch,
        seed,
    });
    Ok(true)
}

fn handle_raw_ui_stop_sound(
    cur: &mut std::io::Cursor<&[u8]>,
    event_tx: &Sender<NetworkEvent>,
) -> Result<bool, azalea_buf::BufReadError> {
    let set = FixedBitSet::<2>::azalea_read(cur)?;
    if !set.index(0) || u32::azalea_read_var(cur)? != UI_SOUND_SOURCE {
        return Ok(false);
    }
    let name = if set.index(1) {
        Some(Identifier::azalea_read(cur)?.to_string())
    } else {
        None
    };
    let _ = event_tx.try_send(NetworkEvent::StopSound {
        sound_id: name,
        category: Some(UI_SOUND_SOURCE as u8),
    });
    Ok(true)
}

const UI_SOUND_SOURCE: u32 = 10;

#[derive(Clone, Copy)]
struct SoundPacketIds {
    sound: u32,
    sound_entity: u32,
    stop_sound: u32,
}

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

fn set_objective_packet_id() -> u32 {
    use pomme_protocol::{Direction, PacketTable, Phase};

    static ID: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *ID.get_or_init(|| {
        PacketTable::native()
            .id(Phase::Game, Direction::Clientbound, "set_objective")
            .expect("set_objective in packet table")
    })
}

fn parse_set_objective(cur: &mut std::io::Cursor<&[u8]>) -> Result<NetworkEvent, String> {
    use azalea_buf::AzBuf;
    use azalea_protocol::packets::game::c_set_objective::MethodKind;

    let name = String::azalea_read(cur).map_err(|error| error.to_string())?;
    let method = MethodKind::azalea_read(cur).map_err(|error| error.to_string())?;
    let (display, number_format, render_type) = match method {
        MethodKind::Remove => (None, None, None),
        MethodKind::Add | MethodKind::Change => {
            let text =
                azalea_chat::FormattedText::azalea_read(cur).map_err(|error| error.to_string())?;
            let render_type = azalea_core::objectives::ObjectiveCriteria::azalea_read(cur)
                .map_err(|error| error.to_string())?;
            let number_format = if bool::azalea_read(cur).map_err(|error| error.to_string())? {
                Some(
                    azalea_chat::numbers::NumberFormat::azalea_read(cur)
                        .map_err(|error| error.to_string())?,
                )
            } else {
                None
            };
            let display = format_text_spans(&text, [1.0; 4]);
            (
                Some(display),
                number_format
                    .as_ref()
                    .map(score_number_format)
                    .transpose()?,
                Some(render_type),
            )
        }
    };
    Ok(NetworkEvent::ScoreboardObjective {
        name,
        display,
        number_format,
        render_type,
    })
}

fn cooldown_packet_id() -> u32 {
    use pomme_protocol::{Direction, PacketTable, Phase};

    static ID: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *ID.get_or_init(|| {
        PacketTable::native()
            .id(Phase::Game, Direction::Clientbound, "cooldown")
            .expect("cooldown in packet table")
    })
}

/// `ClientboundLevelParticles`' packet id from the vanilla-derived table
/// (cross-checked against azalea's dispatch table in `azalea_compat`).
fn level_particles_packet_id() -> u32 {
    use pomme_protocol::{Direction, PacketTable, Phase};

    static ID: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *ID.get_or_init(|| {
        PacketTable::native()
            .id(Phase::Game, Direction::Clientbound, "level_particles")
            .expect("level_particles in packet table")
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

    use super::*;
    use crate::net::sender::Outbound;

    #[test]
    fn animate_critical_actions_dispatch_distinct_events_and_other_actions_do_not() {
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
            );
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
        );
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
            );
            if let Some(id) = expected {
                assert!(
                    matches!(event_rx.recv().unwrap(), NetworkEvent::TotemUsed { entity_id } if entity_id == id)
                );
            } else {
                assert!(event_rx.try_recv().is_err());
            }
        }
    }

    #[test]
    fn explosion_ingress_preserves_the_complete_native_payload() {
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
        );
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

    #[test]
    fn player_rotation_and_entity_teleport_keep_relative_flags_in_events() {
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
        let mut dispatch = |packet: ClientboundGamePacket| {
            handle_game_packet(
                &packet,
                &sender,
                &event_tx,
                &registries,
                &command_tree,
                &mut batches,
                &mut cookies,
            );
        };
        dispatch(ClientboundGamePacket::PlayerRotation(
            ClientboundPlayerRotation {
                y_rot: 12.0,
                relative_y: true,
                x_rot: 25.0,
                relative_x: false,
            },
        ));
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
        ));
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
        ));
        assert!(matches!(
            event_rx.recv().unwrap(),
            NetworkEvent::EntityTeleported {
                relative: None,
                velocity: None,
                ..
            }
        ));
    }

    #[test]
    fn merchant_mount_and_border_packets_keep_native_fields_in_events() {
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
        let mut dispatch = |packet: ClientboundGamePacket| {
            handle_game_packet(
                &packet,
                &sender,
                &event_tx,
                &registries,
                &command_tree,
                &mut batches,
                &mut cookies,
            );
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
        ));
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
        ));
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
        ));
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
        ));
        assert!(matches!(
            event_rx.recv().unwrap(),
            NetworkEvent::WorldBorderCenter { x: 3.0, z: 4.0 }
        ));
        dispatch(ClientboundGamePacket::SetBorderSize(
            ClientboundSetBorderSize { size: 75.0 },
        ));
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
        ));
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
        ));
        assert!(matches!(
            event_rx.recv().unwrap(),
            NetworkEvent::WorldBorderWarningBlocks { warning_blocks: 11 }
        ));
        dispatch(ClientboundGamePacket::SetBorderWarningDelay(
            ClientboundSetBorderWarningDelay { warning_delay: 31 },
        ));
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

    #[test]
    fn set_held_slot_emits_authoritative_hotbar_selection() {
        let (out_tx, _out_rx) = tokio::sync::mpsc::unbounded_channel();
        let sender = PacketSender::new(out_tx);
        let (event_tx, event_rx) = crossbeam_channel::bounded(1);
        let registries = RegistryHolder::default();
        let command_tree = Arc::new(Mutex::new(None));
        let receive = |slot| {
            handle_game_packet(
                &ClientboundGamePacket::SetHeldSlot(ClientboundSetHeldSlot { slot }),
                &sender,
                &event_tx,
                &registries,
                &command_tree,
                &mut ChunkBatchSizeCalculator::default(),
                &mut std::collections::HashMap::new(),
            );
        };

        receive(5);
        assert!(matches!(
            event_rx.recv().unwrap(),
            NetworkEvent::HeldSlot { slot: 5 }
        ));

        for invalid in [9, u32::MAX] {
            receive(invalid);
            assert!(matches!(
                event_rx.try_recv(),
                Err(crossbeam_channel::TryRecvError::Empty)
            ));
        }
    }

    #[test]
    fn game_ping_is_answered_and_cookies_round_trip() {
        let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel();
        let sender = PacketSender::new(out_tx);
        let (event_tx, event_rx) = crossbeam_channel::bounded(4);
        let registries = RegistryHolder::default();
        let command_tree = Arc::new(Mutex::new(None));
        let mut cookies = std::collections::HashMap::new();
        let mut batches = ChunkBatchSizeCalculator::default();
        let dispatch = |packet: &ClientboundGamePacket,
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
            );
        };

        dispatch(
            &ClientboundGamePacket::Ping(azalea_protocol::packets::game::c_ping::ClientboundPing {
                id: 0x1234,
            }),
            &mut cookies,
            &mut batches,
        );
        let Outbound::Packet(packet) = out_rx.try_recv().unwrap() else {
            panic!("expected pong packet");
        };
        assert!(matches!(*packet, ServerboundGamePacket::Pong(ref p) if p.id == 0x1234));

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
        );
        dispatch(
            &ClientboundGamePacket::CookieRequest(
                azalea_protocol::packets::game::c_cookie_request::ClientboundCookieRequest { key },
            ),
            &mut cookies,
            &mut batches,
        );
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
        );
        assert!(matches!(
            event_rx.try_recv().unwrap(),
            NetworkEvent::ChunkBiomes { pos: event_pos, data }
                if event_pos == pos && data == [4, 5, 6]
        ));
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

    use super::{dimension_info, parse_set_objective};
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
        } = dimension_info(&dim, true, None)
        else {
            panic!("dimension_info returned the wrong event variant");
        };
        assert!(is_debug);
        assert_eq!(clock_id, None);
        assert_eq!(height, 384);
        assert_eq!(min_y, -64);
        assert!(has_skylight);
        assert_eq!(cardinal_light, CardinalLightType::Nether);
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
                style.write(&mut bytes);
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
        } = dimension_info(&dim, false, None)
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
