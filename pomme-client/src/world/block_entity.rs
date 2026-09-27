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
    pub sign_front: Option<[String; 4]>,
    pub sign_back: Option<[String; 4]>,
}

impl StoredBlockEntity {
    pub fn new(kind: BlockEntityKind, nbt: NbtCompound) -> Self {
        let is_sign = kind == BlockEntityKind::Sign;
        Self {
            kind,
            sign_front: is_sign.then(|| sign_lines(&nbt, true)),
            sign_back: is_sign.then(|| sign_lines(&nbt, false)),
            nbt,
        }
    }

    pub fn update_nbt(&mut self, nbt: NbtCompound) {
        self.sign_front = (self.kind == BlockEntityKind::Sign).then(|| sign_lines(&nbt, true));
        self.sign_back = (self.kind == BlockEntityKind::Sign).then(|| sign_lines(&nbt, false));
        self.nbt = nbt;
    }
}

/// Stable, non-URL identity for a player-head texture source.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PlayerHeadProfileSource {
    ResourceTexture(String),
    EmbeddedTexturesProperty(String),
    Profile(String),
    Default,
}

/// Parse only the 26.2 `profile` field; never interpret NBT as a URL or path.
pub fn player_head_profile_source(nbt: &NbtCompound) -> PlayerHeadProfileSource {
    use simdnbt::owned::{NbtList, NbtTag};
    let Some(profile) = nbt.get("profile") else {
        return PlayerHeadProfileSource::Default;
    };
    if let NbtTag::String(name) = profile {
        let name = name.to_str();
        return if name.is_empty() {
            PlayerHeadProfileSource::Default
        } else {
            PlayerHeadProfileSource::Profile(name.into_owned())
        };
    }
    let Some(profile) = profile.compound() else {
        return PlayerHeadProfileSource::Default;
    };
    if let Some(NbtTag::String(texture)) = profile.get("texture") {
        let texture = texture.to_str();
        if valid_player_head_resource_texture(&texture) {
            return PlayerHeadProfileSource::ResourceTexture(texture.into_owned());
        }
    }
    let property = profile
        .get("properties")
        .and_then(|tag| tag.compound())
        .and_then(|props| props.get("textures"))
        .and_then(|tag| match tag {
            NbtTag::List(NbtList::Compound(values)) => values.iter().find_map(|value| {
                if value.get("name")?.string()?.to_str() == "textures" {
                    Some(value.get("value")?.string()?.to_str().into_owned())
                } else {
                    None
                }
            }),
            NbtTag::Compound(props) => props.get("textures").and_then(|tag| match tag {
                NbtTag::List(NbtList::String(values)) => {
                    values.first().map(|v| v.to_str().into_owned())
                }
                _ => None,
            }),
            _ => None,
        });
    if let Some(value) = property {
        return PlayerHeadProfileSource::EmbeddedTexturesProperty(value);
    }
    let id = profile
        .get("id")
        .and_then(|tag| tag.int_array())
        .and_then(|v| {
            (v.len() == 4).then(|| {
                v.iter()
                    .flat_map(|n| n.to_be_bytes())
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            })
        })
        .filter(|id| valid_profile_id(id));
    let name = profile
        .get("name")
        .and_then(|tag| tag.string())
        .map(|v| v.to_str().into_owned())
        .filter(|name| valid_profile_name(name));
    id.or(name).map_or(
        PlayerHeadProfileSource::Default,
        PlayerHeadProfileSource::Profile,
    )
}

pub fn valid_player_head_resource_texture(value: &str) -> bool {
    let Some((namespace, path)) = value.split_once(':') else {
        return false;
    };
    fn component(s: &str) -> bool {
        !s.is_empty()
            && s.bytes().all(|b| {
                b.is_ascii_lowercase()
                    || b.is_ascii_digit()
                    || matches!(b, b'_' | b'-' | b'.' | b'/')
            })
            && !s.starts_with('/')
            && !s
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
    }
    component(namespace) && !namespace.contains('/') && component(path) && !path.contains('%')
}

fn valid_profile_id(id: &str) -> bool {
    (id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()))
        || (id.len() == 36
            && id.bytes().enumerate().all(|(i, b)| {
                if [8, 13, 18, 23].contains(&i) {
                    b == b'-'
                } else {
                    b.is_ascii_hexdigit()
                }
            }))
}

fn valid_profile_name(name: &str) -> bool {
    (1..=16).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
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
        "conduit" => Some(BlockEntityKind::Conduit),
        "player_head" | "player_wall_head" => Some(BlockEntityKind::Skull),
        s if s.ends_with("copper_golem_statue") => Some(BlockEntityKind::CopperGolemStatue),
        s if s == "shulker_box" || s.ends_with("_shulker_box") => Some(BlockEntityKind::ShulkerBox),
        s if (s.ends_with("_sign") || s.ends_with("_wall_sign"))
            && !s.ends_with("_hanging_sign") =>
        {
            Some(BlockEntityKind::Sign)
        }
        _ => None,
    }
}

/// CopperGolemStatueBlockRenderer pose index and oxidation texture index.
pub fn copper_golem_statue_render_state(
    name: &str,
    props: &crate::world::block::PropMap,
) -> Option<(u8, u32)> {
    let pose = match props.get("copper_golem_pose")? {
        "standing" => 0,
        "running" => 1,
        "sitting" => 2,
        "star" => 3,
        _ => return None,
    };
    let material = name.strip_prefix("waxed_").unwrap_or(name);
    let oxidation = match material {
        "copper_golem_statue" => 0,
        "exposed_copper_golem_statue" => 1,
        "weathered_copper_golem_statue" => 2,
        "oxidized_copper_golem_statue" => 3,
        _ => return None,
    };
    Some((pose, oxidation))
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
            | BlockEntityKind::CopperGolemStatue
            | BlockEntityKind::Conduit
            | BlockEntityKind::Skull
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
            map.insert(pos, StoredBlockEntity::new(kind, NbtCompound::default()));
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
        // Vanilla renders these special block entities; their particle-only
        // block models must not fall back to a cube.
        | "copper_golem_statue" | "exposed_copper_golem_statue" | "weathered_copper_golem_statue"
        | "oxidized_copper_golem_statue" | "waxed_copper_golem_statue"
        | "waxed_exposed_copper_golem_statue" | "waxed_weathered_copper_golem_statue"
        | "waxed_oxidized_copper_golem_statue"
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
    fn player_head_resource_identifier_rejects_traversal_and_non_asset_paths() {
        assert!(valid_player_head_resource_texture(
            "minecraft:entity/player/slim/steve"
        ));
        for value in [
            "minecraft:../secret",
            "minecraft:/absolute",
            "C:/x",
            "minecraft:a\\\\b",
            "minecraft:a%2fb",
            "minecraft:a//b",
        ] {
            assert!(!valid_player_head_resource_texture(value), "{value}");
        }
    }

    #[test]
    fn profile_identifiers_are_strictly_validated() {
        assert!(valid_profile_id("0123456789abcdef0123456789abcdef"));
        assert!(!valid_profile_id("not-a-uuid"));
        assert!(valid_profile_name("Player_1"));
        assert!(!valid_profile_name("../player"));
    }

    #[test]
    fn stored_sign_text_matches_parser_and_refreshes_on_nbt_update() {
        use simdnbt::owned::NbtList;

        let mut face = NbtCompound::new();
        face.insert(
            "messages",
            NbtList::String(vec![
                "{\"text\":\"one\"}".into(),
                "raw fallback".into(),
                "\"three\"".into(),
                "\"four\"".into(),
                "\"ignored fifth line\"".into(),
            ]),
        );
        let mut nbt = NbtCompound::new();
        nbt.insert("front_text", face);
        let mut entity = StoredBlockEntity::new(BlockEntityKind::Sign, nbt);
        assert_eq!(
            entity.sign_front.as_ref().unwrap(),
            &[
                "one".to_owned(),
                "raw fallback".to_owned(),
                "three".to_owned(),
                "four".to_owned()
            ]
        );
        assert_eq!(
            entity.sign_front.as_ref().unwrap(),
            &sign_lines(&entity.nbt, true)
        );

        entity.update_nbt(NbtCompound::new());
        assert_eq!(
            entity.sign_front.as_ref().unwrap(),
            &[String::new(), String::new(), String::new(), String::new()]
        );
        assert_eq!(
            entity.sign_front.as_ref().unwrap(),
            &sign_lines(&entity.nbt, true)
        );
        assert_eq!(
            entity.sign_back.as_ref().unwrap(),
            &sign_lines(&entity.nbt, false)
        );
    }

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
    fn copper_golem_statues_use_their_own_render_kind() {
        assert_eq!(
            rendered_kind("copper_golem_statue"),
            Some(BlockEntityKind::CopperGolemStatue)
        );
        assert_eq!(
            rendered_kind("waxed_oxidized_copper_golem_statue"),
            Some(BlockEntityKind::CopperGolemStatue)
        );
    }

    #[test]
    fn copper_golem_statue_pose_and_oxidation_follow_block_state() {
        crate::world::block::init("26.2");
        for (name, pose, expected) in [
            ("copper_golem_statue", "running", (1u8, 0u32)),
            ("waxed_oxidized_copper_golem_statue", "star", (3u8, 3u32)),
        ] {
            let state = crate::world::block::find_state(name, &[("copper_golem_pose", pose)]);
            assert_eq!(
                copper_golem_statue_render_state(
                    name,
                    crate::world::block::block_properties(state),
                ),
                Some(expected),
            );
        }
    }

    #[test]
    fn copper_golem_statues_are_not_fallback_cubes() {
        for name in [
            "copper_golem_statue",
            "exposed_copper_golem_statue",
            "weathered_copper_golem_statue",
            "oxidized_copper_golem_statue",
            "waxed_copper_golem_statue",
            "waxed_exposed_copper_golem_statue",
            "waxed_weathered_copper_golem_statue",
            "waxed_oxidized_copper_golem_statue",
        ] {
            assert!(is_block_entity_block(name), "{name}");
        }
    }

    #[test]
    fn moving_block_progress_changes_render_translation() {
        use simdnbt::owned::NbtTag;

        let mut moved = NbtCompound::new();
        moved.insert("Name", "minecraft:stone");
        let mut payload = NbtCompound::new();
        payload.insert("blockState", NbtTag::Compound(moved));
        payload.insert("progress", NbtTag::Float(0.0));
        payload.insert("extending", NbtTag::Byte(1));
        payload.insert("source", NbtTag::Byte(0));
        payload.insert("facing", "north");
        let start = moving_block_render_details(&payload).unwrap().offset;
        let mut halfway = NbtCompound::new();
        halfway.insert(
            "blockState",
            NbtTag::Compound({
                let mut state = NbtCompound::new();
                state.insert("Name", "minecraft:stone");
                state
            }),
        );
        halfway.insert("progress", NbtTag::Float(0.5));
        halfway.insert("extending", NbtTag::Byte(1));
        halfway.insert("source", NbtTag::Byte(0));
        halfway.insert("facing", "north");
        let middle = moving_block_render_details(&halfway).unwrap().offset;
        assert_ne!(start, middle);
        assert_eq!(middle, glam::DVec3::new(0.0, 0.0, 0.5));
    }

    #[test]
    fn heavy_core_is_not_hidden_as_an_invisible_block() {
        assert!(!is_invisible_block("heavy_core"));
    }
}
