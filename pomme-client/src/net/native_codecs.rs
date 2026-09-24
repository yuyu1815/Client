//! Native 26.2 wire corrections for packet layouts that the pinned Azalea
//! codecs still serialize differently. These helpers are only used when the
//! connection has no version translation active.

use std::io;

use azalea_buf::AzBuf;
use azalea_core::sound::CustomSound;
use azalea_inventory::ItemStack;
use azalea_protocol::packets::game::c_explode::{ExplosionParticleInfo, Weighted};
use azalea_protocol::packets::game::s_set_creative_mode_slot::ServerboundSetCreativeModeSlot;
use azalea_registry::Holder;
use azalea_registry::builtin::SoundEvent;
use pomme_protocol::{Direction, PacketTable, Phase, wire};

/// Decode official 26.2 ClientboundExplode before Azalea misreads the sound
/// Holder. A recognized malformed frame returns Err and must not fall back to
/// Azalea.
pub(crate) fn decode_native_explosion(
    raw: &[u8],
) -> Result<Option<super::ExplosionPayload>, String> {
    let packet_id = pomme_protocol::PacketTable::native()
        .id(
            pomme_protocol::Phase::Game,
            pomme_protocol::Direction::Clientbound,
            "explode",
        )
        .ok_or_else(|| "native explode id missing".to_owned())?;
    let mut pos = 0;
    let Some(id) = pomme_protocol::wire::read_varint(raw, &mut pos) else {
        return Ok(None);
    };
    if id != packet_id {
        return Ok(None);
    }
    let mut input = std::io::Cursor::new(&raw[pos..]);
    macro_rules! read {
        ($ty:ty, $field:literal) => {
            <$ty as AzBuf>::azalea_read(&mut input)
                .map_err(|e| format!("malformed native explode {}: {e}", $field))?
        };
    }
    // STREAM_CODEC order (javap): Vec3, FLOAT, INT, Optional<Vec3>, Particle,
    // Holder<SoundEvent>, weighted list.
    let center = read!(azalea_core::position::Vec3, "center");
    let radius = read!(f32, "radius");
    let block_count = read!(i32, "block_count");
    let player_knockback = read!(Option<azalea_core::position::Vec3>, "player_knockback");
    let explosion_particle = read!(azalea_entity::particle::Particle, "explosion_particle");
    let holder = read!(Holder<SoundEvent, CustomSound>, "explosion_sound");
    let explosion_sound = crate::audio::SoundRef::resolve(&holder);
    let block_particles = read!(Vec<Weighted<ExplosionParticleInfo>>, "block_particles");
    if input.position() as usize != input.get_ref().len() {
        return Err(format!(
            "native explode has {} trailing bytes",
            input.get_ref().len() - input.position() as usize
        ));
    }
    Ok(Some(super::ExplosionPayload {
        center,
        radius,
        block_count,
        player_knockback,
        explosion_particle,
        explosion_sound,
        block_particles,
    }))
}

/// Normalizes a native SetPlayerTeam optional color for pinned Azalea.
/// `Ok(None)` means the frame is not a team packet or its action has no
/// Parameters. A recognized team packet with malformed Parameters is Err so
/// the caller can skip it rather than decoding it with Azalea's wrong codec.
pub(crate) fn normalize_native_team_color(raw: &[u8]) -> Result<Option<Vec<u8>>, String> {
    let team_id = PacketTable::native()
        .id(Phase::Game, Direction::Clientbound, "set_player_team")
        .ok_or_else(|| "native set_player_team id missing".to_owned())?;
    let mut pos = 0;
    let Some(id) = wire::read_varint(raw, &mut pos) else {
        return Ok(None);
    };
    if id != team_id {
        return Ok(None);
    }

    // A recognized packet's header must be valid before deciding whether its
    // action carries Parameters.
    let name_len = read_team_varint(raw, &mut pos, "team name length")? as usize;
    let name_end = pos
        .checked_add(name_len)
        .filter(|&end| end <= raw.len())
        .ok_or_else(|| "truncated set_player_team name".to_owned())?;
    std::str::from_utf8(&raw[pos..name_end])
        .map_err(|_| "invalid set_player_team name UTF-8".to_owned())?;
    pos = name_end;
    let method = *raw
        .get(pos)
        .ok_or_else(|| "missing set_player_team method".to_owned())?;
    pos += 1;
    match method {
        0 | 2 => {}
        1 | 3 | 4 => return Ok(None), // no Parameters: same native/Azalea layout
        _ => return Err(format!("unknown set_player_team method {method}")),
    }

    // Official Parameters start with three network-NBT Components. Reuse the
    // existing NBT reader; never guess their encoded length.
    for _ in 0..3 {
        super::chat::read_nbt_tag(raw, &mut pos)
            .map_err(|error| format!("malformed set_player_team component: {error}"))?;
    }
    read_team_varint(raw, &mut pos, "name-tag visibility")?;
    read_team_varint(raw, &mut pos, "collision rule")?;

    // Optional<TeamColor> uses the official readBoolean contract: any nonzero
    // byte is true. Capture the full field boundary for a precise replacement.
    let field_start = pos;
    let present = *raw
        .get(pos)
        .ok_or_else(|| "missing set_player_team color presence".to_owned())?
        != 0;
    pos += 1;
    let color = if present {
        let id = read_team_varint(raw, &mut pos, "team color id")?;
        // The official id mapper uses OutOfBoundsStrategy.ZERO, so negative
        // signed VarInts (u32 representation > i32::MAX) and ids outside 0..16
        // decode to BLACK/0 rather than failing or becoming RESET.
        if id < 16 { id } else { 0 }
    } else {
        21 // ChatFormatting::Reset for Optional::None
    };

    // Preserve the rest exactly: this includes options and any method-0 list.
    let mut normalized = Vec::with_capacity(raw.len());
    normalized.extend_from_slice(&raw[..field_start]);
    wire::write_varint(&mut normalized, color);
    normalized.extend_from_slice(&raw[pos..]);
    Ok(Some(normalized))
}

fn read_team_varint(raw: &[u8], pos: &mut usize, field: &str) -> Result<u32, String> {
    wire::read_varint(raw, pos).ok_or_else(|| format!("truncated set_player_team {field}"))
}

/// Encodes the native 26.2 ServerboundSetCreativeModeSlot frame. The caller
/// must select this only on the native (no-translation) connection path.
pub(crate) fn encode_native_creative_slot(
    packet: &ServerboundSetCreativeModeSlot,
) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    let id = PacketTable::native()
        .id(
            Phase::Game,
            Direction::Serverbound,
            "set_creative_mode_slot",
        )
        .ok_or_else(|| io::Error::other("native set_creative_mode_slot id missing"))?;
    wire::write_varint(&mut out, id);
    out.extend_from_slice(&(packet.slot_num as i16).to_be_bytes());

    match &packet.item_stack {
        ItemStack::Empty => wire::write_varint(&mut out, 0),
        ItemStack::Present(stack) => {
            // Preserve the caller's count/item/components exactly; do not
            // reject or normalize unusual counts or item limits here.
            wire::write_varint(&mut out, stack.count as u32);
            stack.kind.azalea_write(&mut out)?;

            let additions: Vec<_> = stack
                .component_patch
                .iter()
                .filter_map(|(kind, value)| value.map(|value| (kind, value)))
                .collect();
            let removals: Vec<_> = stack
                .component_patch
                .iter()
                .filter_map(|(kind, value)| value.is_none().then_some(kind))
                .collect();
            wire::write_varint(&mut out, additions.len() as u32);
            wire::write_varint(&mut out, removals.len() as u32);

            for (kind, component) in additions {
                kind.azalea_write(&mut out)?;
                let mut value = Vec::new();
                component.encode(&mut value)?;
                wire::write_varint(&mut out, value.len() as u32);
                out.extend_from_slice(&value);
            }
            for kind in removals {
                kind.azalea_write(&mut out)?;
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use azalea_inventory::components::{Damage, Unbreakable};
    use azalea_inventory::{ItemStack, ItemStackData};
    use azalea_registry::Registry;
    use azalea_registry::builtin::{DataComponentKind, ItemKind};

    use super::*;

    fn explosion_frame(sound: &[u8]) -> Vec<u8> {
        use azalea_core::position::Vec3;
        use azalea_entity::particle::Particle;
        use azalea_protocol::packets::game::c_explode::{ExplosionParticleInfo, Weighted};
        let mut b = Vec::new();
        wire::write_varint(
            &mut b,
            PacketTable::native()
                .id(Phase::Game, Direction::Clientbound, "explode")
                .unwrap(),
        );
        Vec3::new(1.0, 2.0, 3.0).azalea_write(&mut b).unwrap();
        4.5f32.azalea_write(&mut b).unwrap();
        23i32.azalea_write(&mut b).unwrap();
        Some(Vec3::new(0.25, -0.5, 0.75))
            .azalea_write(&mut b)
            .unwrap();
        Particle::ExplosionEmitter.azalea_write(&mut b).unwrap();
        b.extend_from_slice(sound); // explicit official holder bytes, not a sound writer roundtrip
        vec![Weighted {
            value: ExplosionParticleInfo {
                particle: Particle::EndRod,
                scaling: 1.25,
                speed: 0.75,
            },
            weight: 9,
        }]
        .azalea_write(&mut b)
        .unwrap();
        b
    }
    #[test]
    fn native_explosion_sound_reference_and_direct_holder_fixtures() {
        use azalea_core::position::Vec3;
        let reference = decode_native_explosion(&explosion_frame(&[1]))
            .unwrap()
            .unwrap();
        assert_eq!(
            reference.explosion_sound.event_name(),
            "entity.allay.ambient_with_item"
        );
        assert_eq!(reference.block_particles.len(), 1);
        assert_eq!(reference.block_particles[0].weight, 9);
        assert_eq!(reference.block_particles[0].value.scaling, 1.25);
        assert_eq!(reference.block_particles[0].value.speed, 0.75);
        assert_eq!(reference.center, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(
            reference.player_knockback,
            Some(Vec3::new(0.25, -0.5, 0.75))
        );
        let mut none = vec![0, 12];
        none.extend_from_slice(b"example:boom");
        none.push(0);
        let direct = decode_native_explosion(&explosion_frame(&none))
            .unwrap()
            .unwrap();
        assert_eq!(direct.explosion_sound.event_name(), "example:boom");
        assert_eq!(direct.block_particles.len(), 1);
        assert_eq!(
            direct.block_particles[0].value.particle,
            azalea_entity::particle::Particle::EndRod
        );
        assert_eq!(direct.block_particles[0].weight, 9);
        assert_eq!(direct.block_particles[0].value.scaling, 1.25);
        assert_eq!(direct.block_particles[0].value.speed, 0.75);
        assert_eq!(direct.center, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(direct.radius, 4.5);
        assert_eq!(direct.block_count, 23);
        assert_eq!(direct.player_knockback, Some(Vec3::new(0.25, -0.5, 0.75)));
        assert_eq!(
            direct.explosion_particle,
            azalea_entity::particle::Particle::ExplosionEmitter
        );
        let mut some = vec![0, 12];
        some.extend_from_slice(b"example:boom");
        some.push(1);
        some.extend_from_slice(&2.5f32.to_be_bytes());
        let direct = decode_native_explosion(&explosion_frame(&some))
            .unwrap()
            .unwrap();
        assert_eq!(direct.explosion_sound.event_name(), "example:boom");
        assert_eq!(direct.block_particles.len(), 1);
        assert_eq!(
            direct.block_particles[0].value.particle,
            azalea_entity::particle::Particle::EndRod
        );
        assert_eq!(direct.block_particles[0].weight, 9);
        assert_eq!(direct.block_particles[0].value.scaling, 1.25);
        assert_eq!(direct.block_particles[0].value.speed, 0.75);
        assert_eq!(direct.center, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(direct.radius, 4.5);
        assert_eq!(direct.block_count, 23);
        assert_eq!(direct.player_knockback, Some(Vec3::new(0.25, -0.5, 0.75)));
        assert_eq!(
            direct.explosion_particle,
            azalea_entity::particle::Particle::ExplosionEmitter
        );
    }
    #[test]
    fn truncated_native_explosion_sound_and_list_are_rejected() {
        assert!(decode_native_explosion(&explosion_frame(&[0x80])).is_err());
        assert!(decode_native_explosion(&explosion_frame(&[0, 12, b'e', b'x'])).is_err());
        let mut direct = vec![0, 12];
        direct.extend_from_slice(b"example:boom");
        direct.push(1);
        assert!(decode_native_explosion(&explosion_frame(&direct)).is_err());
        let mut list = explosion_frame(&[1]);
        list.truncate(list.len() - 2);
        assert!(decode_native_explosion(&list).is_err());
    }
    fn team_header(method: u8) -> Vec<u8> {
        let id = PacketTable::native()
            .id(Phase::Game, Direction::Clientbound, "set_player_team")
            .unwrap();
        let mut frame = Vec::new();
        wire::write_varint(&mut frame, id);
        frame.extend_from_slice(&[1, b't', method]);
        frame
    }

    fn team_frame(method: u8, color: Option<&[u8]>, members: bool) -> Vec<u8> {
        let mut frame = team_header(method);
        if method == 0 || method == 2 {
            // Three valid network-NBT TAG_String Components with payload "x".
            for _ in 0..3 {
                frame.extend_from_slice(&[8, 0, 1, b'x']);
            }
            frame.extend_from_slice(&[0, 0]); // visibility, collision
            match color {
                Some(id) => {
                    frame.push(1);
                    frame.extend_from_slice(id);
                }
                None => frame.push(0),
            }
            frame.push(3); // options
        }
        if members && (method == 0 || method == 3 || method == 4) {
            frame.extend_from_slice(&[1, 1, b'p']); // one member "p"
        }
        frame
    }

    fn normalized_color(frame: &[u8], method: u8) -> (u8, u8) {
        let normalized = normalize_native_team_color(frame)
            .expect("well-formed native team")
            .expect("Parameters action must normalize");
        let start = normalized.len() - if method == 0 { 5 } else { 2 };
        (normalized[start] as u8, normalized[start + 1])
    }

    #[test]
    fn native_team_color_none_and_valid_ids_keep_following_fields() {
        assert_eq!(normalized_color(&team_frame(2, None, false), 2), (21, 3));
        assert_eq!(
            normalized_color(&team_frame(2, Some(&[0]), false), 2),
            (0, 3)
        );
        let mut nonzero_presence = team_frame(2, Some(&[5]), false);
        let presence = nonzero_presence.len() - 3;
        nonzero_presence[presence] = 2; // readBoolean treats any nonzero as true
        assert_eq!(normalized_color(&nonzero_presence, 2), (5, 3));
        assert_eq!(
            normalized_color(&team_frame(0, Some(&[15]), true), 0),
            (15, 3)
        );
        let added = normalize_native_team_color(&team_frame(0, Some(&[5]), true))
            .unwrap()
            .unwrap();
        assert_eq!(&added[added.len() - 3..], &[1, 1, b'p']);
    }

    #[test]
    fn native_team_color_out_of_range_uses_official_zero_strategy() {
        assert_eq!(
            normalized_color(&team_frame(2, Some(&[16]), false), 2),
            (0, 3)
        );
        // Signed VarInt -1, encoded in the u32 wire helper's five bytes.
        assert_eq!(
            normalized_color(
                &team_frame(2, Some(&[0xff, 0xff, 0xff, 0xff, 0x0f]), false),
                2
            ),
            (0, 3)
        );
    }

    #[test]
    fn native_team_nonparameter_actions_are_not_rewritten() {
        for (method, members) in [(1, false), (3, true), (4, true)] {
            let frame = team_frame(method, None, members);
            assert_eq!(normalize_native_team_color(&frame).unwrap(), None);
        }
        let not_team = [0xff];
        assert_eq!(normalize_native_team_color(&not_team).unwrap(), None);
    }

    #[test]
    fn malformed_known_native_team_fails_closed() {
        let mut truncated_header = team_header(2);
        truncated_header.pop(); // method missing
        assert!(normalize_native_team_color(&truncated_header).is_err());

        let mut truncated_nbt = team_header(2);
        truncated_nbt.extend_from_slice(&[8, 0, 0]);
        assert!(normalize_native_team_color(&truncated_nbt).is_err());

        let mut missing_presence = team_frame(2, None, false);
        missing_presence.truncate(missing_presence.len() - 2); // drop presence + options
        assert!(normalize_native_team_color(&missing_presence).is_err());

        let mut missing_color = team_frame(2, Some(&[5]), false);
        missing_color.truncate(missing_color.len() - 2); // retain presence, drop id + options
        assert!(normalize_native_team_color(&missing_color).is_err());
    }

    fn creative(slot: u16, item: ItemStack) -> ServerboundSetCreativeModeSlot {
        ServerboundSetCreativeModeSlot {
            slot_num: slot,
            item_stack: item,
        }
    }

    #[test]
    fn native_creative_empty_and_empty_patch_fixtures() {
        let empty = encode_native_creative_slot(&creative(36, ItemStack::Empty)).unwrap();
        let mut expected = Vec::new();
        wire::write_varint(
            &mut expected,
            PacketTable::native()
                .id(
                    Phase::Game,
                    Direction::Serverbound,
                    "set_creative_mode_slot",
                )
                .unwrap(),
        );
        expected.extend_from_slice(&[0, 36, 0]);
        assert_eq!(empty, expected);

        let stack = ItemStackData::new(ItemKind::Stone, 1);
        let encoded = encode_native_creative_slot(&creative(36, stack.into())).unwrap();
        assert_eq!(&encoded[encoded.len() - 2..], &[0, 0]); // empty patch

        let negative_slot =
            encode_native_creative_slot(&creative(u16::MAX, ItemStack::Empty)).unwrap();
        assert_eq!(&negative_slot[negative_slot.len() - 3..], &[0xff, 0xff, 0]);
    }

    #[test]
    fn native_creative_additions_are_length_prefixed_and_removals_are_not() {
        let mut patch = azalea_inventory::DataComponentPatch::default();
        // The component union values use the existing Azalea component
        // serializer, once each, before their native length prefixes.
        unsafe {
            patch.unchecked_insert_component(
                DataComponentKind::Damage,
                Some(Damage { amount: 7 }.into()),
            );
            patch.unchecked_insert_component(
                DataComponentKind::Unbreakable,
                Some(Unbreakable.into()),
            );
            patch.unchecked_insert_component(DataComponentKind::ItemName, None);
        }
        let mut stack = ItemStackData::new(ItemKind::Stone, 1);
        stack.component_patch = patch;
        let encoded = encode_native_creative_slot(&creative(36, stack.into())).unwrap();

        let damage_kind = DataComponentKind::Damage.to_u32();
        let unbreakable_kind = DataComponentKind::Unbreakable.to_u32();
        let removed_kind = DataComponentKind::ItemName.to_u32();
        let mut expected_patch = Vec::new();
        wire::write_varint(&mut expected_patch, 2);
        wire::write_varint(&mut expected_patch, 1);
        wire::write_varint(&mut expected_patch, damage_kind);
        expected_patch.extend_from_slice(&[1, 7]); // byte length 1, VarInt damage 7
        wire::write_varint(&mut expected_patch, unbreakable_kind);
        expected_patch.push(0); // zero-byte unit value still has a length delimiter
        wire::write_varint(&mut expected_patch, removed_kind); // removal: id only
        let mut expected = Vec::new();
        wire::write_varint(
            &mut expected,
            PacketTable::native()
                .id(
                    Phase::Game,
                    Direction::Serverbound,
                    "set_creative_mode_slot",
                )
                .unwrap(),
        );
        expected.extend_from_slice(&[0, 36]);
        wire::write_varint(&mut expected, 1);
        ItemKind::Stone.azalea_write(&mut expected).unwrap();
        expected.extend_from_slice(&expected_patch);
        assert_eq!(encoded, expected);

        let mut only_removal = azalea_inventory::DataComponentPatch::default();
        unsafe { only_removal.unchecked_insert_component(DataComponentKind::ItemName, None) };
        let mut stack = ItemStackData::new(ItemKind::Stone, 1);
        stack.component_patch = only_removal;
        let encoded = encode_native_creative_slot(&creative(36, stack.into())).unwrap();
        let mut expected_tail = vec![0, 1]; // zero additions, one removal
        wire::write_varint(&mut expected_tail, removed_kind);
        assert!(encoded.ends_with(&expected_tail));
    }
}
