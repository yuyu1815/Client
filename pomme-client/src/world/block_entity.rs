use std::collections::HashMap;

use azalea_block::BlockState;
use azalea_core::position::BlockPos;
use azalea_registry::builtin::BlockEntityKind;
use simdnbt::owned::NbtCompound;

#[derive(Clone)]
pub struct StoredBlockEntity {
    #[allow(dead_code)]
    pub kind: BlockEntityKind,
    #[allow(dead_code)]
    pub nbt: NbtCompound,
}

/// Extract the four vanilla-rendered text lines for one sign face. The NBT
/// stores each line as a JSON component string; malformed components fall back
/// to their raw text rather than preventing editing.
pub fn sign_lines(nbt: &NbtCompound, is_front_text: bool) -> [String; 4] {
    let face = if is_front_text {
        "front_text"
    } else {
        "back_text"
    };
    let Some(messages) = nbt
        .get(face)
        .and_then(|tag| tag.compound())
        .and_then(|face| face.list("messages"))
    else {
        return std::array::from_fn(|_| String::new());
    };
    let simdnbt::owned::NbtList::String(messages) = messages else {
        return std::array::from_fn(|_| String::new());
    };
    std::array::from_fn(|i| {
        messages.get(i).map_or_else(String::new, |json| {
            let json = json.to_str();
            serde_json::from_str::<serde_json::Value>(&json)
                .map(|component| component_plain_text(&component))
                .unwrap_or_else(|_| json.into_owned())
        })
    })
}

fn component_plain_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Object(object) => {
            let mut text = object
                .get("text")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned();
            if let Some(extra) = object.get("extra").and_then(serde_json::Value::as_array) {
                for child in extra {
                    text.push_str(&component_plain_text(child));
                }
            }
            text
        }
        serde_json::Value::Array(items) => items.iter().map(component_plain_text).collect(),
        _ => String::new(),
    }
}

// TODO: Sign world text is still not rendered. `sign_lines` currently flattens
// component JSON to plain text, so correct vanilla styling, wrapping, dye/glow
// colors, and fullbright glow require the world-text renderer path.
/// Blocks the block-entity pipeline draws in place of chunk geometry. The
/// chunk mesher skips these (their block models are particle-texture-only,
/// which would otherwise fall back to a full cube of that texture); other
/// block-entity blocks either have real block models or placeholder cubes.
pub fn rendered_kind(name: &str) -> Option<BlockEntityKind> {
    match name {
        "chest" => Some(BlockEntityKind::Chest),
        "trapped_chest" => Some(BlockEntityKind::TrappedChest),
        "ender_chest" => Some(BlockEntityKind::EnderChest),
        // Copper chests share vanilla's `chest` block entity type; the
        // weathering stage only picks the texture.
        s if s.ends_with("copper_chest") => Some(BlockEntityKind::Chest),
        s if s == "shulker_box" || s.ends_with("_shulker_box") => Some(BlockEntityKind::ShulkerBox),
        s if (s.ends_with("_sign") || s.ends_with("_wall_sign"))
            && !s.ends_with("_hanging_sign") =>
        {
            Some(BlockEntityKind::Sign)
        }
        _ => None,
    }
}

/// Kinds [`rendered_kind`] synthesizes entries for; used to detect entries
/// gone stale when the block at their position stops mapping to them.
fn is_rendered(kind: BlockEntityKind) -> bool {
    matches!(
        kind,
        BlockEntityKind::Chest
            | BlockEntityKind::TrappedChest
            | BlockEntityKind::EnderChest
            | BlockEntityKind::ShulkerBox
            | BlockEntityKind::Sign
    )
}

/// Sync the client-side entry for `pos` after a block update. The server sends
/// no block-entity data for e.g. a freshly placed chest, so blocks the BE
/// pipeline renders get an entry synthesized from the block state (vanilla
/// creates the client `BlockEntity` from the state the same way); a position
/// whose block is no longer a block entity drops its stale entry.
pub fn sync_block_entity(
    map: &mut HashMap<BlockPos, StoredBlockEntity>,
    pos: BlockPos,
    state: BlockState,
) {
    let id = crate::world::block::block_id(state);
    if let Some(kind) = rendered_kind(id) {
        if map.get(&pos).is_none_or(|e| e.kind != kind) {
            map.insert(
                pos,
                StoredBlockEntity {
                    kind,
                    nbt: NbtCompound::default(),
                },
            );
        }
    } else if !is_block_entity_block(id) || map.get(&pos).is_some_and(|e| is_rendered(e.kind)) {
        // A synthesized entry is also stale when the block swaps directly to a
        // different block-entity block (e.g. /setblock chest -> sign).
        map.remove(&pos);
    }
}

/// Blocks vanilla backs with a block entity. Used to suppress missing-model
/// warnings and to detect stale block-entity map entries; the subset the BE
/// pipeline actually draws is [`rendered_kind`].
pub fn is_block_entity_block(name: &str) -> bool {
    rendered_kind(name).is_some() // chests, copper chests, shulker boxes
        || matches!(
        name,
        // Signs
        | "oak_sign" | "spruce_sign" | "birch_sign" | "jungle_sign" | "acacia_sign" | "dark_oak_sign"
        | "mangrove_sign" | "cherry_sign" | "pale_oak_sign" | "bamboo_sign"
        | "crimson_sign" | "warped_sign"
        | "oak_wall_sign" | "spruce_wall_sign" | "birch_wall_sign" | "jungle_wall_sign"
        | "acacia_wall_sign" | "dark_oak_wall_sign" | "mangrove_wall_sign" | "cherry_wall_sign"
        | "pale_oak_wall_sign" | "bamboo_wall_sign"
        | "crimson_wall_sign" | "warped_wall_sign"
        | "oak_hanging_sign" | "spruce_hanging_sign" | "birch_hanging_sign" | "jungle_hanging_sign"
        | "acacia_hanging_sign" | "dark_oak_hanging_sign" | "mangrove_hanging_sign"
        | "cherry_hanging_sign" | "pale_oak_hanging_sign" | "bamboo_hanging_sign"
        | "crimson_hanging_sign" | "warped_hanging_sign"
        | "oak_wall_hanging_sign" | "spruce_wall_hanging_sign" | "birch_wall_hanging_sign"
        | "jungle_wall_hanging_sign" | "acacia_wall_hanging_sign" | "dark_oak_wall_hanging_sign"
        | "mangrove_wall_hanging_sign" | "cherry_wall_hanging_sign" | "pale_oak_wall_hanging_sign"
        | "bamboo_wall_hanging_sign" | "crimson_wall_hanging_sign" | "warped_wall_hanging_sign"
        // Banners
        | "white_banner" | "orange_banner" | "magenta_banner" | "light_blue_banner"
        | "yellow_banner" | "lime_banner" | "pink_banner" | "gray_banner"
        | "light_gray_banner" | "cyan_banner" | "purple_banner" | "blue_banner"
        | "brown_banner" | "green_banner" | "red_banner" | "black_banner"
        | "white_wall_banner" | "orange_wall_banner" | "magenta_wall_banner" | "light_blue_wall_banner"
        | "yellow_wall_banner" | "lime_wall_banner" | "pink_wall_banner" | "gray_wall_banner"
        | "light_gray_wall_banner" | "cyan_wall_banner" | "purple_wall_banner" | "blue_wall_banner"
        | "brown_wall_banner" | "green_wall_banner" | "red_wall_banner" | "black_wall_banner"
        // Beds
        | "white_bed" | "orange_bed" | "magenta_bed" | "light_blue_bed"
        | "yellow_bed" | "lime_bed" | "pink_bed" | "gray_bed"
        | "light_gray_bed" | "cyan_bed" | "purple_bed" | "blue_bed"
        | "brown_bed" | "green_bed" | "red_bed" | "black_bed"
        // Skulls / heads
        | "skeleton_skull" | "skeleton_wall_skull"
        | "wither_skeleton_skull" | "wither_skeleton_wall_skull"
        | "zombie_head" | "zombie_wall_head"
        | "player_head" | "player_wall_head"
        | "creeper_head" | "creeper_wall_head"
        | "dragon_head" | "dragon_wall_head"
        | "piglin_head" | "piglin_wall_head"
        // Misc block entities
        | "conduit" | "decorated_pot" | "end_portal" | "end_gateway"
        | "beacon" | "spawner" | "trial_spawner" | "vault"
        | "brewing_stand" | "lectern" | "campfire" | "soul_campfire"
        | "beehive" | "bee_nest" | "bell" | "suspicious_sand" | "suspicious_gravel"
        | "crafter"
    )
}

#[derive(Clone, Copy)]
pub struct MovingBlockRender {
    pub state: BlockState,
    pub offset: glam::DVec3,
    pub source: bool,
    pub extending: bool,
    pub progress: f32,
    pub direction: glam::DVec3,
}

/// Resolve the moved state and vanilla's piston translation from the 26.2
/// update-tag fields. Invalid or stale payloads intentionally remain invisible.
pub fn moving_block_render_details(nbt: &NbtCompound) -> Option<MovingBlockRender> {
    use simdnbt::owned::NbtTag;

    let number = |key: &str| -> Option<f32> {
        match nbt.get(key)? {
            NbtTag::Float(value) => Some(*value),
            _ => None,
        }
    };
    let boolean = |key: &str| -> Option<bool> {
        match nbt.get(key)? {
            NbtTag::Byte(value) => Some(*value != 0),
            _ => None,
        }
    };
    let progress = number("progress")?;
    if !(0.0..=1.0).contains(&progress) {
        return None;
    }
    let extending = boolean("extending")?;
    let source = boolean("source")?;
    let direction = match nbt.get("facing")? {
        NbtTag::String(value) => match value.to_str().as_ref() {
            "down" => glam::DVec3::NEG_Y,
            "up" => glam::DVec3::Y,
            "north" => glam::DVec3::NEG_Z,
            "south" => glam::DVec3::Z,
            "west" => glam::DVec3::NEG_X,
            "east" => glam::DVec3::X,
            _ => return None,
        },
        _ => return None,
    };
    let moved = nbt.get("blockState")?.compound()?;
    let name = match moved.get("Name")? {
        NbtTag::String(value) => {
            let value = value.to_str();
            value
                .strip_prefix("minecraft:")
                .unwrap_or(value.as_ref())
                .to_owned()
        }
        _ => return None,
    };
    let properties = match moved.get("Properties") {
        None => Vec::new(),
        Some(tag) => tag
            .compound()?
            .iter()
            .map(|(key, value)| {
                Some((
                    key.to_str().into_owned(),
                    value.string()?.to_str().into_owned(),
                ))
            })
            .collect::<Option<Vec<_>>>()?,
    };
    let state = crate::world::block::state_with_properties(&name, &properties)?;
    let offset = direction
        * f64::from(if extending {
            progress - 1.0
        } else {
            1.0 - progress
        });
    Some(MovingBlockRender {
        state,
        offset,
        source,
        extending,
        progress,
        direction,
    })
}

pub fn moving_block_render(nbt: &NbtCompound) -> Option<(BlockState, glam::DVec3)> {
    moving_block_render_details(nbt).map(|render| (render.state, render.offset))
}

pub fn moving_block_collision(nbt: &NbtCompound) -> Option<(BlockState, glam::DVec3)> {
    if matches!(nbt.get("source")?, simdnbt::owned::NbtTag::Byte(value) if *value != 0) {
        return None;
    }
    moving_block_render(nbt)
}

pub fn is_invisible_block(name: &str) -> bool {
    matches!(
        name,
        "air" | "cave_air" | "void_air" | "barrier" | "light" | "structure_void" | "moving_piston"
    )
}

pub fn is_fluid_block(name: &str) -> bool {
    matches!(name, "water" | "lava" | "bubble_column")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standing_and_wall_signs_use_the_sign_renderer_but_hanging_signs_do_not() {
        assert_eq!(rendered_kind("oak_sign"), Some(BlockEntityKind::Sign));
        assert_eq!(
            rendered_kind("spruce_wall_sign"),
            Some(BlockEntityKind::Sign)
        );
        assert_eq!(rendered_kind("oak_hanging_sign"), None);
        assert!(is_block_entity_block("oak_sign"));
    }

    #[test]
    fn heavy_core_is_not_hidden_as_an_invisible_block() {
        assert!(!is_invisible_block("heavy_core"));
    }
}
