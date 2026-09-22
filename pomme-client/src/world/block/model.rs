use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use glam::{Mat4, Quat, Vec3};
use serde::Deserialize;

use super::registry::{FaceTextures, Tint};
use crate::assets::{AssetId, AssetIndex, resolve_asset_path_with_packs};

#[derive(Deserialize)]
struct BlockstateFile {
    variants: Option<HashMap<String, VariantEntry>>,
    multipart: Option<Vec<MultipartCase>>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum VariantEntry {
    Single(ModelRef),
    Array(Vec<ModelRef>),
}

impl VariantEntry {
    fn refs(&self) -> &[ModelRef] {
        match self {
            VariantEntry::Single(r) => std::slice::from_ref(r),
            VariantEntry::Array(arr) => arr,
        }
    }

    fn first(&self) -> Option<&ModelRef> {
        self.refs().first()
    }
}

#[derive(Deserialize)]
struct MultipartCase {
    apply: MultipartApply,
    when: Option<serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum MultipartApply {
    Single(ModelRef),
    Array(Vec<ModelRef>),
}

impl MultipartApply {
    fn refs(&self) -> &[ModelRef] {
        match self {
            MultipartApply::Single(r) => std::slice::from_ref(r),
            MultipartApply::Array(arr) => arr,
        }
    }

    fn first(&self) -> Option<&ModelRef> {
        self.refs().first()
    }
}

#[derive(Deserialize)]
struct ModelRef {
    model: String,
    #[serde(default)]
    x: i32,
    #[serde(default)]
    y: i32,
    #[serde(default)]
    uvlock: bool,
    #[serde(default = "default_model_weight")]
    weight: u32,
}

fn default_model_weight() -> u32 {
    1
}

const MAX_MODEL_WEIGHT: u64 = i32::MAX as u64;

fn validate_weight_values(weights: impl IntoIterator<Item = u64>) -> Result<u32, &'static str> {
    let mut total = 0u64;
    for weight in weights {
        if weight == 0 {
            return Err("weight must be a positive int");
        }
        if weight > MAX_MODEL_WEIGHT {
            return Err("weight exceeds POSITIVE_INT/i32::MAX");
        }
        total = total
            .checked_add(weight)
            .ok_or("weight sum overflow")?;
        if total > MAX_MODEL_WEIGHT {
            return Err("weight sum exceeds i32::MAX");
        }
    }
    if total == 0 {
        return Err("weighted model list must not be empty");
    }
    u32::try_from(total).map_err(|_| "weight sum does not fit Java nextInt bound")
}

#[derive(Deserialize, Default, Clone)]
struct ModelFile {
    parent: Option<String>,
    #[serde(default, deserialize_with = "deserialize_texture_map")]
    textures: HashMap<String, String>,
    #[serde(default)]
    elements: Vec<ElementDef>,
    #[serde(default)]
    display: HashMap<String, serde_json::Value>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct DisplayTransform {
    pub rotation: Vec3,
    pub translation: Vec3,
    pub scale: Vec3,
}

impl DisplayTransform {
    pub fn to_matrix(self) -> Mat4 {
        Mat4::from_translation(self.translation)
            * Mat4::from_rotation_x(self.rotation.x.to_radians())
            * Mat4::from_rotation_y(self.rotation.y.to_radians())
            * Mat4::from_rotation_z(self.rotation.z.to_radians())
            * Mat4::from_scale(self.scale)
    }
}

fn parse_display_transform(json: &serde_json::Value) -> Option<DisplayTransform> {
    let obj = json.as_object()?;
    let rotation = obj
        .get("rotation")
        .map(|value| parse_vec3(value, Vec3::ZERO))
        .unwrap_or(Vec3::ZERO);
    let translation = obj
        .get("translation")
        .map(|value| parse_vec3(value, Vec3::ZERO))
        .unwrap_or(Vec3::ZERO)
        * (1.0 / 16.0);
    let scale = obj
        .get("scale")
        .map(|value| parse_vec3(value, Vec3::ONE))
        .unwrap_or(Vec3::ONE);
    Some(DisplayTransform {
        rotation,
        translation: translation.clamp(Vec3::splat(-5.0), Vec3::splat(5.0)),
        scale: scale.clamp(Vec3::splat(-4.0), Vec3::splat(4.0)),
    })
}

pub(crate) fn default_block_ground_transform() -> Mat4 {
    DisplayTransform {
        rotation: Vec3::ZERO,
        translation: Vec3::new(0.0, 3.0 / 16.0, 0.0),
        scale: Vec3::splat(0.25),
    }
    .to_matrix()
}

fn deserialize_texture_map<'de, D>(de: D) -> Result<HashMap<String, String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    let raw: HashMap<String, serde_json::Value> = HashMap::deserialize(de)?;
    let mut out = HashMap::new();
    for (k, v) in raw {
        if let Some(s) = v.as_str() {
            out.insert(k, s.to_string());
        } else if let Some(sprite) = v.get("sprite").and_then(serde_json::Value::as_str) {
            out.insert(k, sprite.to_string());
        }
    }
    Ok(out)
}

#[derive(Deserialize, Clone)]
struct ElementDef {
    from: [f32; 3],
    to: [f32; 3],
    #[serde(default)]
    rotation: Option<ElementRotation>,
    #[serde(default)]
    faces: HashMap<String, FaceDef>,
    #[serde(default = "default_true")]
    shade: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Deserialize, Clone)]
struct ElementRotation {
    origin: [f32; 3],
    axis: String,
    angle: f32,
    #[serde(default)]
    rescale: bool,
}

#[derive(Deserialize, Clone)]
struct FaceDef {
    uv: Option<[f32; 4]>,
    texture: String,
    cullface: Option<String>,
    #[serde(default)]
    rotation: Option<i32>,
    #[serde(rename = "tintindex")]
    tint_index: Option<i32>,
}

#[derive(Clone, Debug, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Direction {
    Down,
    Up,
    North,
    South,
    West,
    East,
}

impl Direction {
    pub fn offset(&self) -> [i32; 3] {
        match self {
            Direction::Down => [0, -1, 0],
            Direction::Up => [0, 1, 0],
            Direction::North => [0, 0, -1],
            Direction::South => [0, 0, 1],
            Direction::West => [-1, 0, 0],
            Direction::East => [1, 0, 0],
        }
    }

    fn from_str(s: &str) -> Option<Self> {
        match s {
            "down" => Some(Direction::Down),
            "up" => Some(Direction::Up),
            "north" => Some(Direction::North),
            "south" => Some(Direction::South),
            "west" => Some(Direction::West),
            "east" => Some(Direction::East),
            _ => None,
        }
    }

    fn rotate_y(self, degrees: i32) -> Self {
        let steps = degrees.rem_euclid(360) / 90;
        let mut d = self;
        for _ in 0..steps {
            d = match d {
                Direction::North => Direction::East,
                Direction::East => Direction::South,
                Direction::South => Direction::West,
                Direction::West => Direction::North,
                other => other,
            };
        }
        d
    }

    fn rotate_x(self, degrees: i32) -> Self {
        let steps = degrees.rem_euclid(360) / 90;
        let mut d = self;
        for _ in 0..steps {
            d = match d {
                Direction::North => Direction::Down,
                Direction::Down => Direction::South,
                Direction::South => Direction::Up,
                Direction::Up => Direction::North,
                other => other,
            };
        }
        d
    }

    /// The default table's shade, for the item paths that bake it in; terrain
    /// looks its dimension's table up at mesh time instead.
    pub(crate) fn shade_light(&self) -> f32 {
        CardinalLighting::DEFAULT.by_face(*self)
    }
}

/// Vanilla `CardinalLighting`: the per-face brightness a dimension shades
/// block faces with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CardinalLighting {
    pub down: f32,
    pub up: f32,
    pub north: f32,
    pub south: f32,
    pub west: f32,
    pub east: f32,
}

impl CardinalLighting {
    pub const DEFAULT: Self = Self {
        down: 0.5,
        up: 1.0,
        north: 0.8,
        south: 0.8,
        west: 0.6,
        east: 0.6,
    };
    pub const NETHER: Self = Self {
        down: 0.9,
        up: 0.9,
        north: 0.8,
        south: 0.8,
        west: 0.6,
        east: 0.6,
    };

    pub fn by_face(&self, dir: Direction) -> f32 {
        match dir {
            Direction::Down => self.down,
            Direction::Up => self.up,
            Direction::North => self.north,
            Direction::South => self.south,
            Direction::West => self.west,
            Direction::East => self.east,
        }
    }
}

/// A dimension type's `cardinal_light`, vanilla `CardinalLighting.Type`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CardinalLightType {
    #[default]
    Default,
    Nether,
}

impl CardinalLightType {
    pub fn table(self) -> CardinalLighting {
        match self {
            Self::Default => CardinalLighting::DEFAULT,
            Self::Nether => CardinalLighting::NETHER,
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ItemTint {
    /// No item JSON tint source, or an un-tinted model face.
    Untinted,
    /// Vanilla `Constant` source, decoded from opaque ARGB/RGB input.
    Constant([u8; 3]),
    /// Vanilla `GrassColorSource`; the RGB is the source's level-independent result.
    Grass {
        temperature: f32,
        downfall: f32,
        rgb: [u8; 3],
    },
    /// A known JSON source that this client does not yet evaluate.
    Unknown { kind: String },
}

impl Default for ItemTint {
    fn default() -> Self {
        Self::Untinted
    }
}

impl ItemTint {
    pub fn rgb(&self) -> [u8; 3] {
        match self {
            Self::Untinted => [255, 255, 255],
            Self::Constant(rgb) => *rgb,
            Self::Grass { rgb, .. } => *rgb,
            Self::Unknown { .. } => [255, 255, 255],
        }
    }

    pub(crate) fn debug_json(&self) -> serde_json::Value {
        serde_json::json!({
            "source": self.debug_kind(),
            "rgb": self.rgb(),
            "definition": self,
        })
    }

    fn debug_kind(&self) -> &'static str {
        match self {
            Self::Untinted => "untinted",
            Self::Constant(_) => "constant",
            Self::Grass { .. } => "grass",
            Self::Unknown { .. } => "unknown",
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BakedQuad {
    pub positions: [[f32; 3]; 4],
    pub uvs: [[f32; 2]; 4],
    pub texture: String,
    pub cullface: Option<Direction>,
    /// Original vanilla face tint index; retained for probe parity with Java quads.
    #[serde(default)]
    pub tint_index: Option<i32>,
    pub tint: super::registry::Tint,
    /// Item-model tint resolved from that item's JSON `tints` list. This is
    /// separate from block/terrain Tint so a block tint cannot leak into items.
    #[serde(default)]
    pub item_tint: ItemTint,
    /// GUI's precomputed ITEMS_3D shade. Held/drop world shaders compute
    /// their context light from the packed normal instead.
    pub shade_light: f32,
    /// The face terrain shades this quad as, `None` for `shade: false`.
    pub shade_face: Option<Direction>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BakedModel {
    pub quads: Vec<BakedQuad>,
    pub is_full_cube: bool,
    /// Vanilla `canOcclude`: full cubes occlude, but cutout blocks like leaves
    /// don't, so neighbor faces against them still render.
    pub occludes: bool,
}

#[derive(Clone)]
pub struct WeightedBakedModel {
    pub weight: u32,
    pub model: BakedModel,
}

#[derive(Clone)]
pub struct MultipartEntry {
    pub(crate) when: WhenCondition,
    pub(crate) models: Vec<WeightedBakedModel>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum WhenCondition {
    Always,
    Never,
    Property {
        key: String,
        values: Vec<(String, bool)>,
    },
    All(Vec<WhenCondition>),
    Any(Vec<WhenCondition>),
}

impl WhenCondition {
    pub(crate) fn matches(&self, props: &super::PropMap) -> bool {
        match self {
            Self::Always => true,
            Self::Never => false,
            Self::Property { key, values } => props.get(key).is_some_and(|actual| {
                values.iter().any(|(expected, negated)| {
                    (*negated && actual != expected) || (!*negated && actual == expected)
                })
            }),
            Self::All(terms) => terms.iter().all(|term| term.matches(props)),
            Self::Any(terms) => terms.iter().any(|term| term.matches(props)),
        }
    }
}

fn choose_weighted<'a>(models: &'a [WeightedBakedModel], seed: i64) -> Option<&'a BakedModel> {
    let total = validate_weight_values(models.iter().map(|model| model.weight as u64)).ok()? as u64;
    let pick = legacy_next_int(seed, total as u32) as u64;
    let mut cursor = 0;
    models.iter().find_map(|model| {
        cursor += model.weight as u64;
        (pick < cursor).then_some(&model.model)
    })
}

/// Vanilla `Mth.getSeed(BlockPos)`, followed by the `LegacyRandomSource`
/// bounded integer used by `WeightedList`. This keeps model alternatives
/// position-dependent instead of silently selecting the first JSON entry.
pub(crate) fn model_seed_for_position(x: i32, y: i32, z: i32) -> i64 {
    let mut seed = (x as i64).wrapping_mul(3_129_871)
        ^ (z as i64).wrapping_mul(116_129_781)
        ^ y as i64;
    seed = seed
        .wrapping_mul(seed)
        .wrapping_mul(42_317_861)
        .wrapping_add(seed.wrapping_mul(11));
    seed >> 16
}

fn legacy_next(seed: &mut u64, bits: u32) -> u32 {
    *seed = seed
        .wrapping_mul(25_214_903_917)
        .wrapping_add(11)
        & ((1u64 << 48) - 1);
    (*seed >> (48 - bits)) as u32
}

fn legacy_next_int(seed: i64, bound: u32) -> u32 {
    debug_assert!(bound > 0);
    let mut state = ((seed as u64) ^ 25_214_903_917) & ((1u64 << 48) - 1);
    if bound.is_power_of_two() {
        return ((bound as u64 * legacy_next(&mut state, 31) as u64) >> 31) as u32;
    }
    loop {
        let bits = legacy_next(&mut state, 31);
        let value = bits % bound;
        if bits.wrapping_sub(value).wrapping_add(bound - 1) < (1 << 31) {
            return value;
        }
    }
}

pub(crate) fn multipart_seed_for_position(x: i32, y: i32, z: i32) -> i64 {
    let seed = model_seed_for_position(x, y, z);
    let mut state = ((seed as u64) ^ 25_214_903_917) & ((1u64 << 48) - 1);
    (((legacy_next(&mut state, 32) as u64) << 32) | legacy_next(&mut state, 32) as u64) as i64
}

pub(crate) fn choose_baked_model<'a>(
    models: &'a [WeightedBakedModel],
    seed: i64,
) -> Option<&'a BakedModel> {
    choose_weighted(models, seed)
}

const FOLIAGE_TINTED: &[&str] = &[
    "oak_leaves",
    "dark_oak_leaves",
    "jungle_leaves",
    "acacia_leaves",
    "mangrove_leaves",
    "vine",
];

const DRY_FOLIAGE_TINTED: &[&str] = &["leaf_litter"];

const GRASS_TINTED: &[&str] = &[
    "grass_block",
    "grass",
    "short_grass",
    "tall_grass",
    "fern",
    "large_fern",
];

pub fn load_all_block_textures(
    jar_assets_dir: &Path,
    asset_index: &Option<AssetIndex>,
    packs: Option<&crate::resource_pack::ResourcePackManager>,
) -> HashMap<String, FaceTextures> {
    let mut results = HashMap::new();
    let mut model_cache = HashMap::new();

    for_each_blockstate(
        jar_assets_dir,
        asset_index,
        packs,
        |block_name, blockstate| {
            let model_ref = extract_default_model_ref(blockstate)?;
            let resolved = resolve_model(
                &model_ref.model,
                jar_assets_dir,
                asset_index,
                &mut model_cache,
                packs,
            );
            let face_textures = build_face_textures(block_name, &resolved.textures)?;
            results.insert(block_name.to_string(), face_textures);
            Some(())
        },
    );

    tracing::info!(
        "Loaded {} block texture mappings from vanilla assets",
        results.len()
    );
    results
}

type BakedModelMap = HashMap<String, HashMap<String, Vec<WeightedBakedModel>>>;
type MultipartMap = HashMap<String, Vec<MultipartEntry>>;

pub fn bake_all_models(
    jar_assets_dir: &Path,
    asset_index: &Option<AssetIndex>,
    packs: Option<&crate::resource_pack::ResourcePackManager>,
) -> (BakedModelMap, MultipartMap) {
    let mut results: BakedModelMap = HashMap::new();
    let mut multipart_results: HashMap<String, Vec<MultipartEntry>> = HashMap::new();
    let mut model_cache = HashMap::new();
    let mut total = 0u32;

    for_each_blockstate(
        jar_assets_dir,
        asset_index,
        packs,
        |block_name, blockstate| {
            total += 1;
            let mut variants_map: HashMap<String, Vec<WeightedBakedModel>> = HashMap::new();

            if let Some(variants) = &blockstate.variants {
                for (variant_key, variant_entry) in variants {
                    if let Err(error) = validate_weight_values(
                        variant_entry.refs().iter().map(|model_ref| model_ref.weight as u64),
                    ) {
                        tracing::warn!(
                            "Skipping invalid weighted blockstate model {block_name} variant {variant_key}: {error}"
                        );
                        continue;
                    }
                    let mut models = Vec::new();
                    for model_ref in variant_entry.refs() {
                        let resolved = resolve_model(
                            &model_ref.model,
                            jar_assets_dir,
                            asset_index,
                            &mut model_cache,
                            packs,
                        );
                        if let Some(mut baked) = bake_resolved_model(
                            &resolved,
                            model_ref.x,
                            model_ref.y,
                            model_ref.uvlock,
                            |tint_index| determine_tint_for_index(block_name, tint_index),
                        ) {
                            if is_non_occluding(block_name) {
                                baked.occludes = false;
                            }
                            models.push(WeightedBakedModel {
                                weight: model_ref.weight,
                                model: baked,
                            });
                        }
                    }
                    if !models.is_empty() {
                        variants_map.insert(variant_key.clone(), models);
                    }
                }
            } else if let Some(multipart) = &blockstate.multipart {
                let mut entries = Vec::new();
                for case in multipart {
                    if let Err(error) = validate_weight_values(
                        case.apply.refs().iter().map(|model_ref| model_ref.weight as u64),
                    ) {
                        tracing::warn!(
                            "Skipping invalid weighted multipart models for {block_name}: {error}"
                        );
                        continue;
                    }
                    let mut models = Vec::new();
                    for model_ref in case.apply.refs() {
                        let resolved = resolve_model(
                            &model_ref.model,
                            jar_assets_dir,
                            asset_index,
                            &mut model_cache,
                            packs,
                        );
                        if let Some(baked) = bake_resolved_model(
                            &resolved,
                            model_ref.x,
                            model_ref.y,
                            model_ref.uvlock,
                            |tint_index| determine_tint_for_index(block_name, tint_index),
                        ) {
                            models.push(WeightedBakedModel {
                                weight: model_ref.weight,
                                model: baked,
                            });
                        }
                    }
                    if !models.is_empty() {
                        entries.push(MultipartEntry {
                            when: parse_when_condition(&case.when),
                            models,
                        });
                    }
                }
                if !entries.is_empty() {
                    multipart_results.insert(block_name.to_string(), entries);
                }
            }

            if !variants_map.is_empty() {
                results.insert(block_name.to_string(), variants_map);
            }
            Some(())
        },
    );

    let mut missing_names: Vec<String> = Vec::new();
    for_each_blockstate(jar_assets_dir, asset_index, packs, |block_name, _| {
        if !results.contains_key(block_name)
            && !multipart_results.contains_key(block_name)
            && !crate::world::block_entity::is_block_entity_block(block_name)
            && !crate::world::block_entity::is_fluid_block(block_name)
            && !crate::world::block_entity::is_invisible_block(block_name)
        {
            missing_names.push(block_name.to_string());
        }
        Some(())
    });
    missing_names.sort();
    let baked_count = results.len() + multipart_results.len();
    tracing::info!(
        "Baked models for {}/{} blocks ({} unhandled)",
        baked_count,
        total,
        missing_names.len()
    );
    if !missing_names.is_empty() {
        tracing::warn!("Unhandled baked models: {}", missing_names.join(", "));
    }
    (results, multipart_results)
}

pub struct BakedItemModels {
    pub models: HashMap<String, BakedModel>,
    pub generated_textures: HashSet<String>,
    pub flat_texture_keys: HashMap<String, String>,
    pub flat_tints: HashMap<String, ItemTint>,
    pub ground_transforms: HashMap<String, Mat4>,
}

/// Every `minecraft/items/*.json` name across the jar and the active packs,
/// so a pack can add an item, not only replace one. Vanilla walks every pack
/// in `FallbackResourceManager`.
fn item_definition_names(
    jar_assets_dir: &Path,
    packs: Option<&crate::resource_pack::ResourcePackManager>,
) -> std::collections::BTreeSet<String> {
    let pack_dirs = packs
        .into_iter()
        .flat_map(|packs| packs.active_pack_dirs())
        .map(|dir| dir.join("assets"));
    std::iter::once(jar_assets_dir.to_path_buf())
        .chain(pack_dirs)
        .filter_map(|root| std::fs::read_dir(root.join("minecraft").join("items")).ok())
        .flatten()
        .flatten()
        .filter_map(|entry| {
            entry
                .file_name()
                .to_str()?
                .strip_suffix(".json")
                .map(str::to_owned)
        })
        .collect()
}

pub fn bake_item_models(
    jar_assets_dir: &Path,
    asset_index: &Option<AssetIndex>,
    packs: Option<&crate::resource_pack::ResourcePackManager>,
) -> BakedItemModels {
    let mut item_models: HashMap<String, BakedModel> = HashMap::new();
    let mut flat_item_textures: HashSet<String> = HashSet::new();
    let mut flat_keys: HashMap<String, String> = HashMap::new();
    let mut flat_tints: HashMap<String, ItemTint> = HashMap::new();
    let mut ground_transforms: HashMap<String, Mat4> = HashMap::new();
    let mut model_cache: HashMap<String, ModelFile> = HashMap::new();

    for item_name in item_definition_names(jar_assets_dir, packs) {
        let item_name = item_name.as_str();
        let item_asset_key = format!("minecraft/items/{item_name}.json");
        let item_path =
            resolve_asset_path_with_packs(jar_assets_dir, asset_index, &item_asset_key, packs);
        let Ok(contents) = std::fs::read_to_string(item_path) else {
            continue;
        };
        let Ok(json): Result<serde_json::Value, _> = serde_json::from_str(&contents) else {
            continue;
        };

        let parts = collect_model_parts(&json);
        if parts.is_empty() {
            continue;
        }

        // Item models do not carry the terrain block state; in particular a
        // stem block's item is a seed and must not inherit the age tint.
        let mut merged: Option<BakedModel> = None;
        // Vanilla applies each composite part's own GROUND transform. Pomme
        // merges the parts into one mesh, so it can apply only one; no vanilla
        // composite disagrees (beds share `block/template_bed`), so the first
        // part's wins and a disagreement is logged rather than modelled.
        let mut ground_transform: Option<Mat4> = None;
        for part in &parts {
            let resolved = resolve_model(
                &part.path,
                jar_assets_dir,
                asset_index,
                &mut model_cache,
                packs,
            );
            match ground_transform {
                None => ground_transform = Some(resolved.ground_transform),
                Some(existing) if existing.abs_diff_eq(resolved.ground_transform, 1.0e-6) => {}
                Some(_) => tracing::warn!(
                    "{item_name}: composite part {} has a different ground transform; using the first part's",
                    part.path
                ),
            }
            // A flat sprite (layer0, no elements) only makes sense as the
            // sole part; `merged` stays empty so no 3D model is inserted.
            if parts.len() == 1 && resolved.elements.is_empty() {
                if let Some(value) = resolved.textures.get("layer0")
                    && let Some(key) = texture_to_name(value)
                {
                    flat_item_textures.insert(key.clone());
                    flat_keys.insert(item_name.to_string(), key);
                    flat_tints.insert(
                        item_name.to_string(),
                        resolve_item_tint(&part.tints, Some(0)),
                    );
                }
                break;
            }
            let Some(mut baked) = bake_resolved_model_with_item_tints(
                &resolved,
                0,
                0,
                false,
                |_| Tint::None,
                |tint_index| resolve_item_tint(&part.tints, tint_index),
            ) else {
                continue;
            };
            if let Some(m) = part.transform {
                for quad in &mut baked.quads {
                    for p in &mut quad.positions {
                        *p = m.transform_point3(Vec3::from_array(*p)).to_array();
                    }
                }
            }
            merged = Some(match merged.take() {
                None => baked,
                Some(mut model) => {
                    model.quads.extend(baked.quads);
                    model.is_full_cube = false;
                    model.occludes = false;
                    model
                }
            });
        }
        if let Some(transform) = ground_transform {
            ground_transforms.insert(item_name.to_string(), transform);
        }
        if let Some(mut baked) = merged {
            apply_gui_lambert(&mut baked.quads, BLOCK_GUI_ROTATION_DEG);
            item_models.insert(item_name.to_string(), baked);
        }
    }

    item_models.insert("chest".to_string(), bake_chest_item_model());
    ground_transforms.insert("chest".to_string(), default_block_ground_transform());
    flat_keys.remove("chest");

    tracing::info!(
        "Baked {} item models, {} flat items, and registered {} generated-item textures",
        item_models.len(),
        flat_keys.len(),
        flat_item_textures.len()
    );
    BakedItemModels {
        models: item_models,
        generated_textures: flat_item_textures,
        flat_texture_keys: flat_keys,
        flat_tints,
        ground_transforms,
    }
}

pub fn bake_chest_item_model() -> BakedModel {
    let tex = "entity/chest/normal";
    let mut quads = Vec::new();
    let shades = vanilla_gui_face_shades(CHEST_GUI_ROTATION_DEG);
    add_chest_cube(
        &mut quads,
        1.0 / 16.0,
        0.0,
        1.0 / 16.0,
        15.0 / 16.0,
        10.0 / 16.0,
        15.0 / 16.0,
        0.0,
        19.0,
        14.0,
        10.0,
        14.0,
        tex,
        shades,
    );
    add_chest_cube(
        &mut quads,
        1.0 / 16.0,
        9.0 / 16.0,
        1.0 / 16.0,
        15.0 / 16.0,
        14.0 / 16.0,
        15.0 / 16.0,
        0.0,
        0.0,
        14.0,
        5.0,
        14.0,
        tex,
        shades,
    );
    add_chest_cube(
        &mut quads,
        7.0 / 16.0,
        7.0 / 16.0,
        15.0 / 16.0,
        9.0 / 16.0,
        11.0 / 16.0,
        1.0,
        0.0,
        0.0,
        2.0,
        4.0,
        1.0,
        tex,
        shades,
    );
    BakedModel {
        quads,
        is_full_cube: false,
        occludes: false,
    }
}

const CHEST_GUI_ROTATION_DEG: [f32; 3] = [30.0, 45.0, 0.0];
const BLOCK_GUI_ROTATION_DEG: [f32; 3] = [30.0, 225.0, 0.0];

fn rotate_y(v: [f32; 3], angle: f32) -> [f32; 3] {
    let (s, c) = angle.sin_cos();
    [c * v[0] + s * v[2], v[1], -s * v[0] + c * v[2]]
}

fn rotate_x(v: [f32; 3], angle: f32) -> [f32; 3] {
    let (s, c) = angle.sin_cos();
    [v[0], c * v[1] - s * v[2], s * v[1] + c * v[2]]
}

fn items_3d_lights() -> ([f32; 3], [f32; 3]) {
    let base = |x: f32, y: f32, z: f32| {
        let len = (x * x + y * y + z * z).sqrt();
        [x / len, y / len, z / len]
    };
    let transform = |v: [f32; 3]| {
        // Match Lighting's ITEMS_3D matrix: the rightmost pose transform is
        // applied first, then the Y flip is part of that matrix, not a final
        // post-rotation. This is the Java source order in Lighting.java.
        let v = rotate_x(v, 2.3561945);
        let v = rotate_y(v, -std::f32::consts::PI / 8.0);
        let v = rotate_x(v, 3.2375858);
        let v = rotate_y(v, 1.0821041);
        [v[0], -v[1], v[2]]
    };
    (
        transform(base(0.2, 1.0, -0.7)),
        transform(base(-0.2, 1.0, 0.7)),
    )
}

fn lambert_shade(world_normal: [f32; 3], l0: [f32; 3], l1: [f32; 3]) -> f32 {
    let d0 = (l0[0] * world_normal[0] + l0[1] * world_normal[1] + l0[2] * world_normal[2]).max(0.0);
    let d1 = (l1[0] * world_normal[0] + l1[1] * world_normal[1] + l1[2] * world_normal[2]).max(0.0);
    ((d0 + d1) * 0.6 + 0.4).min(1.0)
}

fn rotate_mesh_normal(n_mesh: [f32; 3], rotation_deg: [f32; 3]) -> [f32; 3] {
    let after_y = rotate_y(n_mesh, rotation_deg[1].to_radians());
    let rotated = rotate_x(after_y, rotation_deg[0].to_radians());
    // GuiItemAtlas applies PoseStack.scale(slot, -slot, slot) before the
    // item transform. VertexConsumer.putBakedQuad transforms normals with
    // that pose's inverse-transpose, so the GUI Y flip is part of Java's
    // actual normal submitted to item.vsh (not a projection-only flip).
    [rotated[0], -rotated[1], rotated[2]]
}

fn vanilla_gui_face_shades(rotation_deg: [f32; 3]) -> [f32; 6] {
    let (l0, l1) = items_3d_lights();
    let normals = [
        [0.0, 1.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, 0.0, -1.0],
        [0.0, 0.0, 1.0],
        [-1.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
    ];
    let mut shades = [0.0; 6];
    for (i, &n) in normals.iter().enumerate() {
        shades[i] = lambert_shade(rotate_mesh_normal(n, rotation_deg), l0, l1);
    }
    shades
}

fn apply_gui_lambert(quads: &mut [BakedQuad], rotation_deg: [f32; 3]) {
    let (l0, l1) = items_3d_lights();
    for quad in quads {
        let p = &quad.positions;
        let e1 = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
        let e2 = [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]];
        let nx = e1[1] * e2[2] - e1[2] * e2[1];
        let ny = e1[2] * e2[0] - e1[0] * e2[2];
        let nz = e1[0] * e2[1] - e1[1] * e2[0];
        let len = (nx * nx + ny * ny + nz * nz).sqrt();
        if len < 1e-6 {
            continue;
        }
        let n_mesh = [nx / len, ny / len, nz / len];
        let n_world = rotate_mesh_normal(n_mesh, rotation_deg);
        // ItemFeatureRenderer's item.vsh applies minecraft_mix_light once to
        // every GUI quad normal. Do not retain the terrain cardinal shade here;
        // that would apply Direction::shade_light a second time in the GUI
        // item path. Held/drop paths ignore this byte and light the packed
        // normal in their context-specific vertex shader.
        quad.shade_light = lambert_shade(n_world, l0, l1);
    }
}

#[allow(clippy::too_many_arguments)]
fn add_chest_cube(
    quads: &mut Vec<BakedQuad>,
    x0: f32,
    y0: f32,
    z0: f32,
    x1: f32,
    y1: f32,
    z1: f32,
    u: f32,
    v: f32,
    w: f32,
    h: f32,
    d: f32,
    texture: &str,
    shades: [f32; 6],
) {
    const TEX_SIZE: f32 = 64.0;
    let face_specs: [FaceSpec; 6] = [
        FaceSpec {
            positions: [[x0, y1, z1], [x1, y1, z1], [x1, y1, z0], [x0, y1, z0]],
            uv_pixels: (u + d + w, v, u + d + w + w, v + d),
            uv_pattern: UvPattern::Up,
            shade: shades[0],
        },
        FaceSpec {
            positions: [[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]],
            uv_pixels: (u + d, v, u + d + w, v + d),
            uv_pattern: UvPattern::Down,
            shade: shades[1],
        },
        FaceSpec {
            positions: [[x0, y0, z0], [x0, y1, z0], [x1, y1, z0], [x1, y0, z0]],
            uv_pixels: (u + d, v + d, u + d + w, v + d + h),
            uv_pattern: UvPattern::North,
            shade: shades[2],
        },
        FaceSpec {
            positions: [[x1, y0, z1], [x1, y1, z1], [x0, y1, z1], [x0, y0, z1]],
            uv_pixels: (u + d + w + d, v + d, u + d + w + d + w, v + d + h),
            uv_pattern: UvPattern::SouthWestEast,
            shade: shades[3],
        },
        FaceSpec {
            positions: [[x0, y0, z1], [x0, y1, z1], [x0, y1, z0], [x0, y0, z0]],
            uv_pixels: (u, v + d, u + d, v + d + h),
            uv_pattern: UvPattern::SouthWestEast,
            shade: shades[4],
        },
        FaceSpec {
            positions: [[x1, y0, z0], [x1, y1, z0], [x1, y1, z1], [x1, y0, z1]],
            uv_pixels: (u + d + w, v + d, u + d + w + d, v + d + h),
            uv_pattern: UvPattern::SouthWestEast,
            shade: shades[5],
        },
    ];
    for spec in face_specs {
        let (u_min_px, v_min_px, u_max_px, v_max_px) = spec.uv_pixels;
        let u1 = (u_min_px + 0.5) / TEX_SIZE;
        let v1 = (v_min_px + 0.5) / TEX_SIZE;
        let u2 = (u_max_px - 0.5) / TEX_SIZE;
        let v2 = (v_max_px - 0.5) / TEX_SIZE;
        let uvs = match spec.uv_pattern {
            UvPattern::Up => [[u1, v2], [u2, v2], [u2, v1], [u1, v1]],
            UvPattern::Down => [[u1, v1], [u2, v1], [u2, v2], [u1, v2]],
            UvPattern::North => [[u1, v2], [u1, v1], [u2, v1], [u2, v2]],
            UvPattern::SouthWestEast => [[u2, v2], [u2, v1], [u1, v1], [u1, v2]],
        };
        quads.push(BakedQuad {
            positions: spec.positions,
            uvs,
            texture: texture.to_string(),
            cullface: None,
            tint_index: None,
            tint: super::registry::Tint::None,
            item_tint: ItemTint::Untinted,
            shade_light: spec.shade,
            shade_face: None,
        });
    }
}

struct FaceSpec {
    positions: [[f32; 3]; 4],
    uv_pixels: (f32, f32, f32, f32),
    uv_pattern: UvPattern,
    shade: f32,
}

enum UvPattern {
    Up,
    Down,
    North,
    SouthWestEast,
}

struct ModelPart {
    path: String,
    transform: Option<Mat4>,
    tints: Vec<ItemTint>,
}

/// Model references to bake for one item. `minecraft:composite` contributes
/// every child, with the children's `transformation`s composed parent-to-child
/// (vanilla `CompositeModel.Unbaked.bake`); anything else keeps the old
/// first-model-string heuristic, which for select/condition trees picks one
/// representative state.
fn collect_model_parts(json: &serde_json::Value) -> Vec<ModelPart> {
    let mut parts = Vec::new();
    if let Some(node) = json.get("model") {
        collect_parts_from_node(node, None, &mut parts);
    }
    if parts.is_empty()
        && let Some(path) = first_item_model_ref(json)
    {
        parts.push(ModelPart {
            path,
            transform: None,
            tints: find_first_model_tints(json),
        });
    }
    parts
}

/// First `model` reference in an item definition, falling back to a special
/// renderer's `base`.
pub fn first_item_model_ref(json: &serde_json::Value) -> Option<String> {
    find_first_model_string(json)
        .or_else(|| find_first_string_for_key(json, "base"))
        .map(|path| strip_mc_prefix(&path).to_string())
}

fn find_first_model_tints(json: &serde_json::Value) -> Vec<ItemTint> {
    fn find(node: &serde_json::Value) -> Option<Vec<ItemTint>> {
        if node.get("model").and_then(serde_json::Value::as_str).is_some() {
            return Some(parse_item_tints(node));
        }
        match node {
            serde_json::Value::Object(map) => map.values().find_map(find),
            serde_json::Value::Array(values) => values.iter().find_map(find),
            _ => None,
        }
    }
    find(json).unwrap_or_default()
}

fn parse_item_tints(node: &serde_json::Value) -> Vec<ItemTint> {
    node.get("tints")
        .and_then(serde_json::Value::as_array)
        .map(|values| values.iter().map(parse_item_tint).collect())
        .unwrap_or_default()
}

fn parse_item_tint(value: &serde_json::Value) -> ItemTint {
    let Some(kind) = value
        .get("type")
        .and_then(serde_json::Value::as_str)
        .map(strip_mc_prefix)
    else {
        return ItemTint::Unknown {
            kind: "missing_type".to_string(),
        };
    };
    match kind {
        "constant" => value
            .get("value")
            .and_then(serde_json::Value::as_i64)
            .map(|value| ItemTint::Constant([
                (value as u32 >> 16) as u8,
                (value as u32 >> 8) as u8,
                value as u8,
            ]))
            .unwrap_or_else(|| ItemTint::Unknown {
                kind: "constant".to_string(),
            }),
        "grass" => {
            let temperature = value
                .get("temperature")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.5) as f32;
            let downfall = value
                .get("downfall")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(1.0) as f32;
            // 26.2's built-in item source uses the default grass colormap
            // for the only vanilla item grass parameters currently shipped.
            let rgb = if (temperature - 0.5).abs() < f32::EPSILON
                && (downfall - 1.0).abs() < f32::EPSILON
            {
                [124, 189, 107]
            } else {
                // Source identity remains visible; unsupported parameterized
                // grass is not silently treated as a world/biome tint.
                [255, 255, 255]
            };
            ItemTint::Grass {
                temperature,
                downfall,
                rgb,
            }
        }
        other => ItemTint::Unknown {
            kind: other.to_string(),
        },
    }
}

fn resolve_item_tint(tints: &[ItemTint], index: Option<i32>) -> ItemTint {
    if tints.is_empty() {
        return ItemTint::Untinted;
    }
    let Some(index) = index else {
        return ItemTint::Untinted;
    };
    if index < 0 {
        return ItemTint::Untinted;
    }
    tints
        .get(index as usize)
        .cloned()
        .unwrap_or_else(|| ItemTint::Unknown {
            kind: format!("missing_index_{index}"),
        })
}

fn collect_parts_from_node(
    node: &serde_json::Value,
    parent_transform: Option<Mat4>,
    parts: &mut Vec<ModelPart>,
) {
    let own_transform = match node.get("transformation") {
        Some(value) => match parse_item_transformation(value) {
            Some(transform) => Some(transform),
            None => {
                tracing::warn!("Skipping item-model part with unsupported transformation encoding");
                return;
            }
        },
        None => None,
    };
    let transform = match (parent_transform, own_transform) {
        (Some(parent), Some(own)) => Some(parent * own),
        (parent, own) => parent.or(own),
    };
    let node_type = node
        .get("type")
        .and_then(|t| t.as_str())
        .map(strip_mc_prefix);
    match node_type {
        Some("composite") => {
            if let Some(models) = node.get("models").and_then(|m| m.as_array()) {
                for child in models {
                    collect_parts_from_node(child, transform, parts);
                }
            }
        }
        Some("model") => {
            if let Some(path) = node.get("model").and_then(|m| m.as_str()) {
                parts.push(ModelPart {
                    path: strip_mc_prefix(path).to_string(),
                    transform,
                    tints: parse_item_tints(node),
                });
            }
        }
        // Other node types are left for the caller's whole-file fallback.
        _ => {}
    }
}

/// Vanilla `Transformation.compose` (Transformation.java:103): `translation ·
/// leftRotation · scale · rightRotation`, translation in block units. Pomme
/// currently supports the stock quaternion-array encoding. Other valid codec
/// forms are rejected by the caller instead of silently becoming identity.
fn parse_item_transformation(json: &serde_json::Value) -> Option<Mat4> {
    let object = json.as_object()?;
    let quat = |key: &str| -> Option<Quat> {
        let Some(value) = object.get(key) else {
            return Some(Quat::IDENTITY);
        };
        let arr = value.as_array()?;
        if arr.len() != 4 {
            return None;
        }
        let get = |i: usize| arr.get(i)?.as_f64().map(|v| v as f32);
        Some(Quat::from_xyzw(get(0)?, get(1)?, get(2)?, get(3)?))
    };
    let vec3 = |key: &str, default: Vec3| {
        object.get(key).map_or(Some(default), |v| {
            v.as_array().and_then(|arr| {
                if arr.len() != 3 {
                    return None;
                }
                Some(Vec3::new(
                    arr[0].as_f64()? as f32,
                    arr[1].as_f64()? as f32,
                    arr[2].as_f64()? as f32,
                ))
            })
        })
    };
    Some(
        Mat4::from_translation(vec3("translation", Vec3::ZERO)?)
            * Mat4::from_quat(quat("left_rotation")?)
            * Mat4::from_scale(vec3("scale", Vec3::ONE)?)
            * Mat4::from_quat(quat("right_rotation")?),
    )
}

pub fn parse_vec3(value: &serde_json::Value, default: Vec3) -> Vec3 {
    let Some(arr) = value.as_array() else {
        return default;
    };
    let get = |i: usize| arr.get(i).and_then(|v| v.as_f64()).map(|v| v as f32);
    Vec3::new(
        get(0).unwrap_or(default.x),
        get(1).unwrap_or(default.y),
        get(2).unwrap_or(default.z),
    )
}

pub fn strip_mc_prefix(s: &str) -> &str {
    s.strip_prefix("minecraft:").unwrap_or(s)
}

pub fn find_first_model_string(json: &serde_json::Value) -> Option<String> {
    find_first_string_for_key(json, "model")
}

pub fn find_first_string_for_key(json: &serde_json::Value, key: &str) -> Option<String> {
    match json {
        serde_json::Value::Object(map) => {
            if let Some(serde_json::Value::String(s)) = map.get(key) {
                return Some(s.clone());
            }
            for v in map.values() {
                if let Some(r) = find_first_string_for_key(v, key) {
                    return Some(r);
                }
            }
            None
        }
        serde_json::Value::Array(arr) => arr.iter().find_map(|v| find_first_string_for_key(v, key)),
        _ => None,
    }
}

fn parse_when_condition(when: &Option<serde_json::Value>) -> WhenCondition {
    let Some(value) = when else {
        return WhenCondition::Always;
    };
    match parse_condition(value) {
        Ok(condition) => condition,
        Err(error) => {
            tracing::warn!("Ignoring malformed blockstate when condition: {error}");
            WhenCondition::Never
        }
    }
}

fn parse_condition(value: &serde_json::Value) -> Result<WhenCondition, String> {
    let serde_json::Value::Object(map) = value else {
        return Err("condition must be an object".into());
    };
    if map.contains_key("NOT") || map.contains_key("!") || map.contains_key("XOR") {
        return Err("unsupported condition operator; only AND/OR are vanilla-compatible".into());
    }
    if map.len() != 1 && (map.contains_key("OR") || map.contains_key("AND")) {
        return Err("combined condition must contain exactly one operator".into());
    }
    if let Some(terms) = map.get("OR") {
        return Ok(WhenCondition::Any(parse_condition_list(terms)?));
    }
    if let Some(terms) = map.get("AND") {
        return Ok(WhenCondition::All(parse_condition_list(terms)?));
    }
    let mut properties = Vec::with_capacity(map.len());
    for (key, value) in map {
        let raw = match value {
            serde_json::Value::String(value) => value.clone(),
            serde_json::Value::Bool(value) => value.to_string(),
            serde_json::Value::Number(value) => value.to_string(),
            _ => return Err(format!("property {key} has a non-scalar value")),
        };
        let mut values = Vec::new();
        for item in raw.split('|') {
            if item.is_empty() {
                return Err(format!("property {key} has an empty alternative"));
            }
            let (negated, expected) = item.strip_prefix('!').map_or((false, item), |v| (true, v));
            if expected.is_empty() {
                return Err(format!("property {key} has an empty negation"));
            }
            values.push((expected.to_string(), negated));
        }
        properties.push(WhenCondition::Property {
            key: key.clone(),
            values,
        });
    }
    Ok(match properties.len() {
        0 => WhenCondition::Always,
        1 => properties.pop().unwrap(),
        _ => WhenCondition::All(properties),
    })
}

fn parse_condition_list(value: &serde_json::Value) -> Result<Vec<WhenCondition>, String> {
    let serde_json::Value::Array(terms) = value else {
        return Err("combined condition must contain an array".into());
    };
    terms.iter().map(parse_condition).collect()
}

fn for_each_blockstate(
    jar_assets_dir: &Path,
    asset_index: &Option<AssetIndex>,
    packs: Option<&crate::resource_pack::ResourcePackManager>,
    mut callback: impl FnMut(&str, &BlockstateFile) -> Option<()>,
) {
    let Some(blockstates_dir) = resolve_blockstates_dir(jar_assets_dir, asset_index, packs) else {
        tracing::warn!("Blockstates directory not found");
        return;
    };

    let entries = match std::fs::read_dir(&blockstates_dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("Failed to read blockstates dir: {e}");
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }

        let Ok(contents) = std::fs::read_to_string(&path) else {
            continue;
        };
        let blockstate = match serde_json::from_str::<BlockstateFile>(&contents) {
            Ok(blockstate) => blockstate,
            Err(error) => {
                tracing::warn!(
                    "Skipping malformed blockstate {} ({}): {error}",
                    name,
                    path.display()
                );
                continue;
            }
        };

        callback(name, &blockstate);
    }
}

fn resolve_blockstates_dir(
    jar_assets_dir: &Path,
    asset_index: &Option<AssetIndex>,
    packs: Option<&crate::resource_pack::ResourcePackManager>,
) -> Option<PathBuf> {
    let candidates = [
        jar_assets_dir.join("assets/minecraft/blockstates"),
        jar_assets_dir.join("jar/assets/minecraft/blockstates"),
        PathBuf::from("reference/assets/assets/minecraft/blockstates"),
    ];

    for c in &candidates {
        if c.is_dir() {
            return Some(c.clone());
        }
    }

    // Also check the original simple path
    let path = jar_assets_dir.join("minecraft/blockstates");
    if path.is_dir() {
        return Some(path);
    }

    if asset_index.is_some() {
        let test_path = resolve_asset_path_with_packs(
            jar_assets_dir,
            asset_index,
            "minecraft/blockstates/stone.json",
            packs,
        );
        if test_path.exists() {
            return test_path.parent().map(|p| p.to_path_buf());
        }
    }

    None
}

fn extract_default_model_ref(blockstate: &BlockstateFile) -> Option<ModelRef> {
    if let Some(variants) = &blockstate.variants {
        let entry = variants.get("").or_else(|| variants.values().next())?;
        let r = entry.first()?;
        Some(ModelRef {
            model: r.model.clone(),
            x: r.x,
            y: r.y,
            uvlock: r.uvlock,
            weight: r.weight,
        })
    } else if let Some(multipart) = &blockstate.multipart {
        let r = multipart.first()?.apply.first()?;
        Some(ModelRef {
            model: r.model.clone(),
            x: r.x,
            y: r.y,
            uvlock: r.uvlock,
            weight: r.weight,
        })
    } else {
        None
    }
}

struct ResolvedModel {
    textures: HashMap<String, String>,
    elements: Vec<ElementDef>,
    ground_transform: Mat4,
}

fn resolve_model(
    model_id: &str,
    jar_assets_dir: &Path,
    asset_index: &Option<AssetIndex>,
    cache: &mut HashMap<String, ModelFile>,
    packs: Option<&crate::resource_pack::ResourcePackManager>,
) -> ResolvedModel {
    let mut texture_map: HashMap<String, String> = HashMap::new();
    let mut elements: Option<Vec<ElementDef>> = None;
    let mut ground_transform: Option<Mat4> = None;
    let mut current_id = model_id.to_string();

    for _ in 0..20 {
        let Some(model) = load_model(&current_id, jar_assets_dir, asset_index, cache, packs) else {
            break;
        };

        for (key, value) in &model.textures {
            texture_map
                .entry(key.clone())
                .or_insert_with(|| value.clone());
        }

        if elements.is_none() && !model.elements.is_empty() {
            elements = Some(model.elements.clone());
        }
        if ground_transform.is_none()
            && let Some(transform) = model
                .display
                .get("ground")
                .and_then(parse_display_transform)
        {
            ground_transform = Some(transform.to_matrix());
        }

        match &model.parent {
            Some(parent) => current_id = parent.clone(),
            None => break,
        }
    }

    let mut resolved_textures = HashMap::new();
    for (key, value) in &texture_map {
        resolved_textures.insert(key.clone(), resolve_ref(value, &texture_map, 0));
    }

    ResolvedModel {
        textures: resolved_textures,
        elements: elements.unwrap_or_default(),
        ground_transform: ground_transform.unwrap_or(Mat4::IDENTITY),
    }
}

fn resolve_ref(value: &str, map: &HashMap<String, String>, depth: u32) -> String {
    if depth > 10 {
        return value.to_string();
    }
    if let Some(ref_name) = value.strip_prefix('#')
        && let Some(target) = map.get(ref_name)
    {
        return resolve_ref(target, map, depth + 1);
    }
    value.to_string()
}

fn load_model<'a>(
    model_id: &str,
    jar_assets_dir: &Path,
    asset_index: &Option<AssetIndex>,
    cache: &'a mut HashMap<String, ModelFile>,
    packs: Option<&crate::resource_pack::ResourcePackManager>,
) -> Option<&'a ModelFile> {
    if cache.contains_key(model_id) {
        return cache.get(model_id);
    }

    let asset_key = model_id_to_asset_key(model_id);
    let file_path = resolve_model_path(jar_assets_dir, asset_index, &asset_key, packs)?;

    let contents = std::fs::read_to_string(&file_path).ok()?;
    let model: ModelFile = serde_json::from_str(&contents).ok()?;
    cache.insert(model_id.to_string(), model);
    cache.get(model_id)
}

fn resolve_model_path(
    jar_assets_dir: &Path,
    asset_index: &Option<AssetIndex>,
    asset_key: &str,
    packs: Option<&crate::resource_pack::ResourcePackManager>,
) -> Option<PathBuf> {
    let primary = resolve_asset_path_with_packs(jar_assets_dir, asset_index, asset_key, packs);
    if primary.exists() {
        return Some(primary);
    }

    let ref_path = Path::new("reference/assets/assets")
        .join(asset_key.strip_prefix("minecraft/").unwrap_or(asset_key));
    if ref_path.exists() {
        return Some(ref_path);
    }

    None
}

fn model_id_to_asset_key(model_id: &str) -> String {
    AssetId::parse(model_id).asset_key("models", ".json")
}

fn texture_to_name(texture_ref: &str) -> Option<String> {
    if texture_ref.starts_with('#') {
        return None;
    }
    let id = AssetId::parse(texture_ref);
    if id.namespace == "minecraft" {
        if let Some(block_path) = id.path.strip_prefix("block/") {
            Some(block_path.to_string())
        } else if id.path.starts_with("item/")
            || id.path.starts_with("entity/")
            || id.path.starts_with("particle/")
        {
            Some(id.path.to_string())
        } else {
            Some(format!("minecraft:{}", id.path))
        }
    } else {
        Some(id.canonical())
    }
}

fn resolve_face_texture<'a>(
    reference: &str,
    textures: &'a HashMap<String, String>,
) -> Option<&'a str> {
    let slot = reference.strip_prefix('#').unwrap_or(reference);
    textures.get(slot).map(String::as_str)
}

fn bake_resolved_model(
    resolved: &ResolvedModel,
    rot_x: i32,
    rot_y: i32,
    uvlock: bool,
    tint_for_index: impl Fn(Option<i32>) -> super::registry::Tint,
) -> Option<BakedModel> {
    bake_resolved_model_with_item_tints(
        resolved,
        rot_x,
        rot_y,
        uvlock,
        tint_for_index,
        |_| ItemTint::Untinted,
    )
}

fn bake_resolved_model_with_item_tints(
    resolved: &ResolvedModel,
    rot_x: i32,
    rot_y: i32,
    uvlock: bool,
    tint_for_index: impl Fn(Option<i32>) -> super::registry::Tint,
    item_tint_for_index: impl Fn(Option<i32>) -> ItemTint,
) -> Option<BakedModel> {
    if resolved.elements.is_empty() {
        return None;
    }

    let mut quads = Vec::new();

    for element in &resolved.elements {
        let from = [
            element.from[0] / 16.0,
            element.from[1] / 16.0,
            element.from[2] / 16.0,
        ];
        let to = [
            element.to[0] / 16.0,
            element.to[1] / 16.0,
            element.to[2] / 16.0,
        ];

        for (face_name, face_def) in &element.faces {
            let Some(dir) = Direction::from_str(face_name) else {
                continue;
            };

            let Some(texture_ref) = resolve_face_texture(&face_def.texture, &resolved.textures)
            else {
                continue;
            };
            let Some(texture_name) = texture_to_name(texture_ref) else {
                continue;
            };

            let positions = face_positions(dir, from, to);
            let mut uvs = face_uvs(
                dir,
                from,
                to,
                face_def.uv.as_ref(),
                face_def.rotation,
                uvlock,
                rot_x,
                rot_y,
            );

            let mut positions = apply_element_rotation(positions, &element.rotation);

            let mut cullface = face_def.cullface.as_deref().and_then(Direction::from_str);
            let quad_tint = tint_for_index(face_def.tint_index);
            let item_tint = item_tint_for_index(face_def.tint_index);

            if rot_x != 0 || rot_y != 0 {
                positions = rotate_positions(positions, rot_x, rot_y);
                cullface = cullface.map(|d| d.rotate_x(rot_x).rotate_y(rot_y));
            }
            // FaceBakery recalculates the canonical FaceInfo winding after a
            // model rotation (and swaps UVs with the matching vertices).
            if element.rotation.is_none() {
                (positions, uvs) = recalculate_winding(positions, uvs);
            }

            // Vanilla `FaceBakery.bakeQuad`: the shade direction is the
            // rotated quad's nearest cardinal, `UP` for a degenerate quad.
            let shade_face = element
                .shade
                .then(|| direction_from_positions(&positions).unwrap_or(Direction::Up));

            quads.push(BakedQuad {
                positions,
                uvs,
                texture: texture_name,
                cullface,
                tint_index: face_def.tint_index,
                tint: quad_tint,
                item_tint,
                shade_light: shade_face.map_or(1.0, |face| face.shade_light()),
                shade_face,
            });
        }
    }

    if quads.is_empty() {
        return None;
    }

    let is_full_cube = check_full_cube(&quads);
    Some(BakedModel {
        quads,
        is_full_cube,
        occludes: is_full_cube,
    })
}

fn check_full_cube(quads: &[BakedQuad]) -> bool {
    if quads.len() != 6 {
        return false;
    }
    let mut dirs = [false; 6];
    for q in quads {
        match q.cullface {
            Some(Direction::Down) => dirs[0] = true,
            Some(Direction::Up) => dirs[1] = true,
            Some(Direction::North) => dirs[2] = true,
            Some(Direction::South) => dirs[3] = true,
            Some(Direction::West) => dirs[4] = true,
            Some(Direction::East) => dirs[5] = true,
            None => return false,
        }
    }
    dirs.iter().all(|&d| d)
}

/// Vanilla `FaceInfo`: the per-face vertex order all the UV rules are defined
/// against. Every face winds CCW viewed from outside, which the chunk
/// pipeline's backface culling needs.
pub(crate) fn face_positions(dir: Direction, from: [f32; 3], to: [f32; 3]) -> [[f32; 3]; 4] {
    let [x0, y0, z0] = from;
    let [x1, y1, z1] = to;
    match dir {
        Direction::Down => [[x0, y0, z1], [x0, y0, z0], [x1, y0, z0], [x1, y0, z1]],
        Direction::Up => [[x0, y1, z0], [x0, y1, z1], [x1, y1, z1], [x1, y1, z0]],
        Direction::North => [[x1, y1, z0], [x1, y0, z0], [x0, y0, z0], [x0, y1, z0]],
        Direction::South => [[x0, y1, z1], [x0, y0, z1], [x1, y0, z1], [x1, y1, z1]],
        Direction::West => [[x0, y1, z0], [x0, y0, z0], [x0, y0, z1], [x0, y1, z1]],
        Direction::East => [[x1, y1, z1], [x1, y0, z1], [x1, y0, z0], [x1, y1, z0]],
    }
}

pub(crate) fn face_uvs(
    dir: Direction,
    from: [f32; 3],
    to: [f32; 3],
    explicit_uv: Option<&[f32; 4]>,
    rotation: Option<i32>,
    uvlock: bool,
    rot_x: i32,
    rot_y: i32,
) -> [[f32; 2]; 4] {
    // Vanilla `FaceBakery.defaultFaceUV`, normalized to 0..1: some faces
    // sample a window reflected about the texture center.
    let (u1, v1, u2, v2) = if let Some(uv) = explicit_uv {
        (uv[0] / 16.0, uv[1] / 16.0, uv[2] / 16.0, uv[3] / 16.0)
    } else {
        match dir {
            Direction::Down => (from[0], 1.0 - to[2], to[0], 1.0 - from[2]),
            Direction::Up => (from[0], from[2], to[0], to[2]),
            Direction::North => (1.0 - to[0], 1.0 - to[1], 1.0 - from[0], 1.0 - from[1]),
            Direction::South => (from[0], 1.0 - to[1], to[0], 1.0 - from[1]),
            Direction::West => (from[2], 1.0 - to[1], to[2], 1.0 - from[1]),
            Direction::East => (1.0 - to[2], 1.0 - to[1], 1.0 - from[2], 1.0 - from[1]),
        }
    };

    // Vanilla `CuboidFace.UVs` corner cycle, identical for every face (the
    // per-face variation lives in `face_positions`' vertex order).
    // `Quadrant.rotateVertexIndex` shifts each vertex forward through the
    // cycle, spinning the texture clockwise per 90 degrees viewed from
    // outside the block.
    let cycle = [[u1, v1], [u1, v2], [u2, v2], [u2, v1]];
    let shift = rotation.map_or(0, |r| {
        if r % 90 != 0 {
            // Vanilla `Quadrant.parseJson` rejects the whole model instead.
            tracing::warn!("face uv rotation {r} is not a multiple of 90, truncating");
        }
        r.rem_euclid(360) / 90
    }) as usize;
    let raw = std::array::from_fn(|i| cycle[(i + shift) % 4]);
    if !uvlock || (rot_x == 0 && rot_y == 0) {
        return raw;
    }
    raw.map(|[u, v]| uvlock_uv(dir, [u, v], rot_x, rot_y))
}

/// Mirrors vanilla `BlockMath.getFaceTransformation` +
/// `FaceBakery.inverseFaceTransformation`: UV coordinates are transformed in
/// the face's local basis, not by adding the model rotation angle to V.
fn uvlock_uv(dir: Direction, uv: [f32; 2], rot_x: i32, rot_y: i32) -> [f32; 2] {
    // FaceBakery's inverseFaceTransformation works in the actual FaceInfo
    // tangent basis. In particular, V is -Y on every vertical face; using a
    // generic X/Y/Z basis mirrors the rotated wall/pane faces.
    let (u_axis, v_axis) = face_uv_basis(dir);
    let normal = Vec3::from_array(dir.offset().map(|value| value as f32));
    let target = nearest_cardinal_direction(rotate_vector(normal, rot_x, rot_y))
        .unwrap_or(Direction::Up);
    let (target_u, target_v) = face_uv_basis(target);
    let local = u_axis * (uv[0] - 0.5) + v_axis * (uv[1] - 0.5);
    let rotated = rotate_vector(local, rot_x, rot_y);
    [rotated.dot(target_u) + 0.5, rotated.dot(target_v) + 0.5]
}

fn face_uv_basis(dir: Direction) -> (Vec3, Vec3) {
    match dir {
        Direction::Down => (Vec3::X, Vec3::NEG_Z),
        Direction::Up => (Vec3::X, Vec3::Z),
        Direction::North => (Vec3::NEG_X, Vec3::NEG_Y),
        Direction::South => (Vec3::X, Vec3::NEG_Y),
        Direction::West => (Vec3::Z, Vec3::NEG_Y),
        Direction::East => (Vec3::NEG_Z, Vec3::NEG_Y),
    }
}

fn rotate_vector(v: Vec3, rot_x: i32, rot_y: i32) -> Vec3 {
    // Vanilla Quadrant R90 is BLOCK_ROT_*_90, i.e. the negative JOML
    // quarter-turn used by the existing position baker.
    let v = rotate_x_vec(v, -rot_x);
    rotate_y_vec(v, -rot_y)
}

fn rotate_x_vec(v: Vec3, degrees: i32) -> Vec3 {
    let angle = (degrees as f32).to_radians();
    let (sin, cos) = angle.sin_cos();
    Vec3::new(v.x, cos * v.y - sin * v.z, sin * v.y + cos * v.z)
}

fn rotate_y_vec(v: Vec3, degrees: i32) -> Vec3 {
    let angle = (degrees as f32).to_radians();
    let (sin, cos) = angle.sin_cos();
    Vec3::new(cos * v.x + sin * v.z, v.y, -sin * v.x + cos * v.z)
}

fn apply_element_rotation(
    mut positions: [[f32; 3]; 4],
    rotation: &Option<ElementRotation>,
) -> [[f32; 3]; 4] {
    let Some(rot) = rotation else {
        return positions;
    };

    let origin = [
        rot.origin[0] / 16.0,
        rot.origin[1] / 16.0,
        rot.origin[2] / 16.0,
    ];
    let angle_rad = rot.angle.to_radians();
    let cos = angle_rad.cos();
    let sin = angle_rad.sin();

    for pos in &mut positions {
        let dx = pos[0] - origin[0];
        let dy = pos[1] - origin[1];
        let dz = pos[2] - origin[2];

        let (nx, ny, nz) = match rot.axis.as_str() {
            "x" => (dx, cos * dy - sin * dz, sin * dy + cos * dz),
            "y" => (cos * dx + sin * dz, dy, -sin * dx + cos * dz),
            "z" => (cos * dx - sin * dy, sin * dx + cos * dy, dz),
            _ => (dx, dy, dz),
        };

        if rot.rescale {
            let scale = 1.0 / cos.abs();
            pos[0] = origin[0] + nx * scale;
            pos[1] = origin[1] + ny * scale;
            pos[2] = origin[2] + nz * scale;
        } else {
            pos[0] = origin[0] + nx;
            pos[1] = origin[1] + ny;
            pos[2] = origin[2] + nz;
        }
    }

    positions
}

/// The quad's winding normal, or `None` when it is degenerate.
pub(crate) fn quad_normal(positions: &[[f32; 3]; 4]) -> Option<Vec3> {
    let p0 = Vec3::from_array(positions[0]);
    let p1 = Vec3::from_array(positions[1]);
    let p2 = Vec3::from_array(positions[2]);
    (p1 - p0).cross(p2 - p0).try_normalize()
}

/// Vanilla `FaceBakery.findClosestDirection`: the cardinal with the largest
/// positive dot product, first in `Direction` order on a tie; `None` when the
/// normal points nowhere.
pub(crate) fn nearest_cardinal_direction(normal: Vec3) -> Option<Direction> {
    let mut best = None;
    let mut closest_product = 0.0f32;
    for candidate in [
        Direction::Down,
        Direction::Up,
        Direction::North,
        Direction::South,
        Direction::West,
        Direction::East,
    ] {
        let product = normal.dot(Vec3::from_array(candidate.offset().map(|v| v as f32)));
        if product >= 0.0 && product > closest_product {
            closest_product = product;
            best = Some(candidate);
        }
    }
    best
}

pub(crate) fn direction_from_positions(positions: &[[f32; 3]; 4]) -> Option<Direction> {
    nearest_cardinal_direction(quad_normal(positions)?)
}

fn recalculate_winding(
    mut positions: [[f32; 3]; 4],
    mut uvs: [[f32; 2]; 4],
) -> ([[f32; 3]; 4], [[f32; 2]; 4]) {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for position in positions {
        for axis in 0..3 {
            min[axis] = min[axis].min(position[axis]);
            max[axis] = max[axis].max(position[axis]);
        }
    }
    let Some(direction) = direction_from_positions(&positions) else {
        return (positions, uvs);
    };
    let canonical = face_positions(direction, min, max);
    for vertex in 0..4 {
        let Some(source) = (vertex..4).find(|&candidate| {
            positions[candidate]
                .iter()
                .zip(canonical[vertex])
                .all(|(&actual, expected)| (actual - expected).abs() < 1.0e-5)
        }) else {
            return (positions, uvs);
        };
        positions.swap(vertex, source);
        uvs.swap(vertex, source);
    }
    (positions, uvs)
}

fn rotate_positions(mut positions: [[f32; 3]; 4], rot_x: i32, rot_y: i32) -> [[f32; 3]; 4] {
    let center = 0.5f32;

    if rot_x != 0 {
        let angle = (rot_x as f32).to_radians();
        let cos = angle.cos();
        let sin = angle.sin();
        for pos in &mut positions {
            let dy = pos[1] - center;
            let dz = pos[2] - center;
            pos[1] = center + cos * dy + sin * dz;
            pos[2] = center - sin * dy + cos * dz;
        }
    }

    if rot_y != 0 {
        let angle = (rot_y as f32).to_radians();
        let cos = angle.cos();
        let sin = angle.sin();
        for pos in &mut positions {
            let dx = pos[0] - center;
            let dz = pos[2] - center;
            pos[0] = center + cos * dx - sin * dz;
            pos[2] = center + sin * dx + cos * dz;
        }
    }

    positions
}

fn build_face_textures(
    block_name: &str,
    textures: &HashMap<String, String>,
) -> Option<FaceTextures> {
    let mut faces = face_textures_base(block_name, textures)?;
    faces.particle = textures.get("particle").and_then(|v| texture_to_name(v));
    Some(faces)
}

fn face_textures_base(
    block_name: &str,
    textures: &HashMap<String, String>,
) -> Option<FaceTextures> {
    let get = |key: &str| -> Option<String> { textures.get(key).and_then(|v| texture_to_name(v)) };

    let (up, down, north, south, east, west) = (
        get("up"),
        get("down"),
        get("north"),
        get("south"),
        get("east"),
        get("west"),
    );

    let tint = determine_block_tint(block_name);

    if let (Some(up), Some(down), Some(north), Some(south), Some(east), Some(west)) =
        (up, down, north, south, east, west)
    {
        let (side_overlay, tint) = if block_name == "grass_block" {
            (Some("grass_block_side_overlay"), Tint::Grass)
        } else {
            (None, tint)
        };
        return Some(FaceTextures::new(
            &up,
            &down,
            &north,
            &south,
            &east,
            &west,
            side_overlay,
            tint,
        ));
    }

    if let Some(all) = get("all") {
        return Some(FaceTextures::uniform(&all, tint));
    }

    if let (Some(end), Some(side)) = (get("end"), get("side")) {
        return Some(FaceTextures::new(
            &end,
            &end,
            &side,
            &side,
            &side,
            &side,
            None,
            Tint::None,
        ));
    }

    if let (Some(top), Some(side)) = (get("top"), get("side")) {
        let bottom = get("bottom").unwrap_or_else(|| top.clone());
        return Some(FaceTextures::new(
            &top, &bottom, &side, &side, &side, &side, None, tint,
        ));
    }

    if let Some(cross) = get("cross") {
        return Some(FaceTextures::uniform(&cross, tint));
    }

    if let (Some(front), Some(side)) = (get("front"), get("side")) {
        let top = get("top")
            .or_else(|| get("end"))
            .unwrap_or_else(|| side.clone());
        let bottom = get("bottom").unwrap_or_else(|| top.clone());
        return Some(FaceTextures::new(
            &top,
            &bottom,
            &front,
            &side,
            &side,
            &side,
            None,
            Tint::None,
        ));
    }

    if let Some(p) = get("particle") {
        return Some(FaceTextures::uniform(&p, tint));
    }

    None
}

/// Full-cube blocks that mustn't cull adjacent faces (vanilla `noOcclusion`).
/// Solid `packed_ice`/`blue_ice` still occlude, so `ice` is matched exactly.
fn is_non_occluding(block_name: &str) -> bool {
    block_name.ends_with("_leaves")
        || block_name.ends_with("_stained_glass")
        || matches!(block_name, "glass" | "tinted_glass" | "ice" | "frosted_ice")
}

fn determine_block_tint(block_name: &str) -> Tint {
    match block_name {
        "redstone_wire" => Tint::Redstone,
        // BlockColors.createDefault(): constant(-2046180) = 0xFFE0C71C.
        "attached_melon_stem" | "attached_pumpkin_stem" => Tint::Fixed([0xE0, 0xC7, 0x1C]),
        "melon_stem" | "pumpkin_stem" => Tint::Stem,
        "spruce_leaves" => Tint::Fixed([0x61, 0x99, 0x61]),
        "birch_leaves" => Tint::Fixed([0x80, 0xA7, 0x55]),
        "potted_fern" | "bush" | "sugar_cane" => Tint::Grass,
        // BlockColors.constant(colorInHand, colorInWorld): terrain uses world.
        "lily_pad" => Tint::Fixed([0x20, 0x80, 0x30]),
        "pink_petals" | "wildflowers" => Tint::Grass,
        name if GRASS_TINTED.contains(&name) => Tint::Grass,
        name if DRY_FOLIAGE_TINTED.contains(&name) => Tint::DryFoliage,
        name if FOLIAGE_TINTED.contains(&name) => Tint::Foliage,
        _ => Tint::None,
    }
}

/// Resolve only a registered layer. Unknown layers stay white, matching
/// BlockModelRenderer's -1 fallback instead of inventing a biome tint.
fn determine_tint_for_index(block_name: &str, tint_index: Option<i32>) -> Tint {
    let Some(index) = tint_index else {
        return Tint::None;
    };
    if matches!(block_name, "pink_petals" | "wildflowers") {
        return if index == 1 { Tint::Grass } else { Tint::None };
    }
    if index == 0 { determine_block_tint(block_name) } else { Tint::None }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gui_item_lighting_uses_vanilla_items_3d_pose_order() {
        let (light0, light1) = items_3d_lights();
        for (actual, expected) in [
            (light0, [
                -0.9334393_f32,
                -0.26269472_f32,
                -0.24430018_f32,
            ]),
            (light1, [-0.10357136_f32, -0.97660685_f32, 0.18844643_f32]),
        ] {
            for (actual, expected) in actual.into_iter().zip(expected) {
                assert!((actual - expected).abs() < 1.0e-5, "{actual} != {expected}");
            }
        }
    }

    #[test]
    fn gui_item_lighting_replaces_terrain_cardinal_shade() {
        let mut quad = BakedQuad {
            positions: [[0.0, 1.0, 0.0], [1.0, 1.0, 0.0], [1.0, 1.0, 1.0], [0.0, 1.0, 1.0]],
            uvs: [[0.0, 0.0]; 4],
            texture: "stone".to_string(),
            cullface: Some(Direction::Up),
            tint_index: None,
            tint: Tint::None,
            item_tint: ItemTint::Untinted,
            shade_light: Direction::Up.shade_light(),
            shade_face: Some(Direction::Up),
        };
        apply_gui_lambert(std::slice::from_mut(&mut quad), BLOCK_GUI_ROTATION_DEG);
        assert!((quad.shade_light - 0.4).abs() < 1.0e-6);
    }

    const DIRS: [Direction; 6] = [
        Direction::Down,
        Direction::Up,
        Direction::North,
        Direction::South,
        Direction::West,
        Direction::East,
    ];

    /// The (right, up) axes of each face viewed from outside the block:
    /// sky-up for the sides, and vanilla's map orientation for up/down.
    /// `right x up` is the outward normal.
    fn face_axes(dir: Direction) -> (Vec3, Vec3) {
        match dir {
            Direction::Down => (Vec3::X, Vec3::Z),
            Direction::Up => (Vec3::X, Vec3::NEG_Z),
            Direction::North => (Vec3::NEG_X, Vec3::Y),
            Direction::South => (Vec3::X, Vec3::Y),
            Direction::West => (Vec3::Z, Vec3::Y),
            Direction::East => (Vec3::NEG_Z, Vec3::Y),
        }
    }

    /// Quads must stay CCW viewed from outside for backface culling.
    #[test]
    fn determine_tint_matches_vanilla_leaf_table() {
        let cases = [
            ("oak_leaves", Tint::Foliage),
            ("dark_oak_leaves", Tint::Foliage),
            ("jungle_leaves", Tint::Foliage),
            ("acacia_leaves", Tint::Foliage),
            ("mangrove_leaves", Tint::Foliage),
            ("vine", Tint::Foliage),
            ("spruce_leaves", Tint::Fixed([0x61, 0x99, 0x61])),
            ("birch_leaves", Tint::Fixed([0x80, 0xA7, 0x55])),
            ("cherry_leaves", Tint::None),
            ("azalea_leaves", Tint::None),
            ("flowering_azalea_leaves", Tint::None),
            ("pale_oak_leaves", Tint::None),
            ("attached_pumpkin_stem", Tint::Fixed([0xE0, 0xC7, 0x1C])),
            ("attached_melon_stem", Tint::Fixed([0xE0, 0xC7, 0x1C])),
            ("pumpkin_stem", Tint::Stem),
            ("melon_stem", Tint::Stem),
            ("potted_fern", Tint::Grass),
            ("bush", Tint::Grass),
            ("sugar_cane", Tint::Grass),
            ("lily_pad", Tint::Fixed([0x20, 0x80, 0x30])),
            ("pink_petals", Tint::Grass),
            ("wildflowers", Tint::Grass),
        ];
        for (name, expected) in cases {
            assert_eq!(determine_block_tint(name), expected, "{name}");
        }
        assert_eq!(determine_tint_for_index("pink_petals", Some(0)), Tint::None);
        assert_eq!(determine_tint_for_index("pink_petals", Some(1)), Tint::Grass);
        assert_eq!(determine_tint_for_index("pink_petals", Some(2)), Tint::None);
        assert_eq!(determine_tint_for_index("lily_pad", Some(0)), determine_block_tint("lily_pad"));
        assert_eq!(determine_tint_for_index("lily_pad", Some(1)), Tint::None);
        assert_eq!(parse_item_tint(&serde_json::json!({
            "type": "minecraft:grass", "temperature": 0.5, "downfall": 1.0
        })).rgb(), [124, 189, 107]);
        assert_eq!(parse_item_tint(&serde_json::json!({
            "type": "minecraft:constant", "value": -9321636
        })).rgb(), [113, 195, 92]);
        assert_eq!(parse_item_tints(&serde_json::json!({"type": "minecraft:model"})), Vec::<ItemTint>::new());
        assert!(matches!(parse_item_tint(&serde_json::json!({"type": "minecraft:dye"})), ItemTint::Unknown { .. }));
        assert_eq!(resolve_item_tint(&[ItemTint::Constant([1, 2, 3])], Some(0)).rgb(), [1, 2, 3]);
        assert!(matches!(resolve_item_tint(&[ItemTint::Constant([1, 2, 3])], Some(1)), ItemTint::Unknown { .. }));
        assert_eq!(resolve_item_tint(&[ItemTint::Grass { temperature: 0.5, downfall: 1.0, rgb: [1, 2, 3] }], Some(-1)), ItemTint::Untinted);
        assert_eq!(resolve_item_tint(&[], None), ItemTint::Untinted);
        assert_eq!(resolve_item_tint(&[], Some(0)), ItemTint::Untinted);
    }

    #[test]
    fn face_winding_is_ccw_from_outside() {
        for dir in DIRS {
            let p = face_positions(dir, [0.0; 3], [1.0; 3]).map(Vec3::from_array);
            let normal = (p[1] - p[0]).cross(p[2] - p[0]);
            let outward = Vec3::from_array(dir.offset().map(|c| c as f32));
            assert!(normal.dot(outward) > 0.0, "{dir:?} winds the wrong way");
            assert_eq!(
                direction_from_positions(&p.map(|position| position.to_array())),
                Some(dir)
            );
        }
    }

    #[test]
    fn model_rotation_uses_final_face_direction_for_cardinal_shading() {
        let face = FaceDef {
            uv: Some([0.0, 0.0, 16.0, 16.0]),
            texture: "side".to_string(),
            cullface: None,
            rotation: None,
            tint_index: None,
        };
        let resolved = ResolvedModel {
            textures: HashMap::from([("side".to_string(), "block/piston_side".to_string())]),
            elements: vec![ElementDef {
                from: [6.0, 6.0, 4.0],
                to: [10.0, 10.0, 20.0],
                rotation: None,
                faces: HashMap::from([("west".to_string(), face)]),
                shade: true,
            }],
            ground_transform: Mat4::IDENTITY,
        };

        let baked = bake_resolved_model(&resolved, 0, 270, false, |_| Tint::None).unwrap();
        assert_eq!(baked.quads.len(), 1);
        assert_eq!(
            direction_from_positions(&baked.quads[0].positions),
            Some(Direction::South)
        );
        // Model rotation changes geometry, but uvlock=false leaves explicit UVs alone.
        assert_eq!(baked.quads[0].uvs, [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]]);
        assert!((baked.quads[0].shade_light - Direction::South.shade_light()).abs() < 1.0e-6);
    }

    /// Every face must show the full-tile texture upright at rotation 0 and
    /// spin it clockwise per 90 degrees, vanilla's `FaceInfo` +
    /// `CuboidFace.UVs` + `Quadrant` behavior (the piston's side faces use
    /// 90/270 and read 180 degrees off when the direction is inverted).
    #[test]
    fn face_uvs_show_upright_clockwise_rotated_texture() {
        // Image corners in clockwise order; screen corner `s` under a
        // clockwise rotation by `steps` shows image corner `(s - steps) % 4`.
        const IMAGE_CORNERS: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        for dir in DIRS {
            let (right, up) = face_axes(dir);
            let positions = face_positions(dir, [0.0; 3], [1.0; 3]).map(Vec3::from_array);
            for (rot, steps) in [
                (None, 0),
                (Some(0), 0),
                (Some(90), 1),
                (Some(180), 2),
                (Some(270), 3),
            ] {
                let uvs = face_uvs(
                    dir,
                    [0.0; 3],
                    [1.0; 3],
                    Some(&[0.0, 0.0, 16.0, 16.0]),
                    rot,
                    false,
                    0,
                    0,
                );
                let mid = |axis: Vec3| {
                    let coords = positions.map(|p| p.dot(axis));
                    (coords.iter().copied().fold(f32::INFINITY, f32::min)
                        + coords.iter().copied().fold(f32::NEG_INFINITY, f32::max))
                        / 2.0
                };
                let (right_mid, up_mid) = (mid(right), mid(up));
                for (p, uv) in positions.iter().zip(uvs) {
                    let screen_corner = match (p.dot(right) > right_mid, p.dot(up) > up_mid) {
                        (false, true) => 0,
                        (true, true) => 1,
                        (true, false) => 2,
                        (false, false) => 3,
                    };
                    let expected = IMAGE_CORNERS[(screen_corner + 4 - steps) % 4];
                    assert_eq!(uv, expected, "{dir:?} rotation {rot:?} vertex {p:?}");
                }
            }
        }
    }

    /// Default UV windows per face, vanilla `FaceBakery.defaultFaceUV`: the
    /// down/north/east windows reflect about the texture center.
    #[test]
    fn default_uv_windows_match_vanilla() {
        let from = [2.0 / 16.0, 3.0 / 16.0, 4.0 / 16.0];
        let to = [8.0 / 16.0, 9.0 / 16.0, 10.0 / 16.0];
        let expected = [
            (Direction::Down, (0.125, 0.375, 0.5, 0.75)),
            (Direction::Up, (0.125, 0.25, 0.5, 0.625)),
            (Direction::North, (0.5, 0.4375, 0.875, 0.8125)),
            (Direction::South, (0.125, 0.4375, 0.5, 0.8125)),
            (Direction::West, (0.25, 0.4375, 0.625, 0.8125)),
            (Direction::East, (0.375, 0.4375, 0.75, 0.8125)),
        ];
        for (dir, (u1, v1, u2, v2)) in expected {
            let uvs = face_uvs(dir, from, to, None, None, false, 0, 0);
            // The cycle assigns vertex 0 the (u1, v1) corner and vertex 2 the
            // (u2, v2) corner.
            assert_eq!(uvs[0], [u1, v1], "{dir:?} window origin");
            assert_eq!(uvs[2], [u2, v2], "{dir:?} window extent");
        }
    }

    /// The bed shape: a composite item model contributes every child, the
    /// foot carrying its one-block translation.
    #[test]
    fn composite_item_collects_all_parts() {
        let json: serde_json::Value = serde_json::from_str(
            r#"{
                "model": {
                    "type": "minecraft:composite",
                    "models": [
                        {"type": "minecraft:model", "model": "minecraft:block/red_bed_head"},
                        {
                            "type": "minecraft:model",
                            "model": "minecraft:block/red_bed_foot",
                            "transformation": {"translation": [0.0, 0.0, 1.0]}
                        }
                    ]
                }
            }"#,
        )
        .unwrap();
        let parts = collect_model_parts(&json);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].path, "block/red_bed_head");
        assert!(parts[0].transform.is_none());
        assert_eq!(parts[1].path, "block/red_bed_foot");
        let moved = parts[1].transform.unwrap().transform_point3(Vec3::ZERO);
        assert!((moved - Vec3::new(0.0, 0.0, 1.0)).length() < 1e-6);
    }

    #[test]
    fn unsupported_item_transformation_encoding_is_rejected() {
        let raw_matrix = serde_json::json!([
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0
        ]);
        let axis_angle = serde_json::json!({
            "left_rotation": {"axis": [0.0, 1.0, 0.0], "angle": 90.0}
        });
        assert!(parse_item_transformation(&raw_matrix).is_none());
        assert!(parse_item_transformation(&axis_angle).is_none());
    }

    /// Non-composite trees (bundles' select/condition) keep the old
    /// first-model-string behavior.
    #[test]
    fn select_item_falls_back_to_first_model() {
        let json: serde_json::Value = serde_json::from_str(
            r#"{
                "model": {
                    "type": "minecraft:select",
                    "cases": [
                        {"model": {"type": "minecraft:model", "model": "minecraft:item/bundle_open"}}
                    ],
                    "fallback": {"type": "minecraft:model", "model": "minecraft:item/bundle"}
                }
            }"#,
        )
        .unwrap();
        let parts = collect_model_parts(&json);
        assert_eq!(parts.len(), 1);
        assert!(parts[0].transform.is_none());
        let legacy = find_first_model_string(&json).unwrap();
        assert_eq!(parts[0].path, strip_mc_prefix(&legacy));
    }

    use crate::test_util::test_temp_dir;

    #[test]
    fn item_definition_and_ground_transform_follow_resource_pack_override() {
        let root = test_temp_dir("item_model_pack");
        let jar = root.join("jar");
        let instance = root.join("instance");
        let items = jar.join("minecraft/items");
        let models = jar.join("minecraft/models/item");
        let pack_items = instance.join("resourcepacks/test_pack/assets/minecraft/items");
        let pack_models = instance.join("resourcepacks/test_pack/assets/other/models/item");
        std::fs::create_dir_all(&items).unwrap();
        std::fs::create_dir_all(&models).unwrap();
        std::fs::create_dir_all(&pack_items).unwrap();
        std::fs::create_dir_all(&pack_models).unwrap();
        std::fs::write(
            instance.join("resourcepacks/test_pack/pack.mcmeta"),
            r#"{"pack":{"pack_format":84,"description":"test"}}"#,
        )
        .unwrap();
        std::fs::write(
            items.join("test_item.json"),
            r#"{"model":{"type":"minecraft:model","model":"minecraft:item/base"}}"#,
        )
        .unwrap();
        std::fs::write(
            models.join("base.json"),
            r#"{"parent":"minecraft:item/generated","textures":{"layer0":"minecraft:item/base"}}"#,
        )
        .unwrap();
        std::fs::write(
            models.join("generated.json"),
            r#"{"display":{"ground":{"translation":[0,2,0],"scale":[0.5,0.5,0.5]}}}"#,
        )
        .unwrap();
        std::fs::write(
            pack_items.join("test_item.json"),
            r#"{"model":{"type":"minecraft:model","model":"other:item/replacement"}}"#,
        )
        .unwrap();
        std::fs::write(
            pack_models.join("replacement.json"),
            r#"{"parent":"minecraft:item/generated","textures":{"layer0":"other:item/replacement"}}"#,
        )
        .unwrap();
        // An item the jar does not define at all.
        std::fs::write(
            pack_items.join("pack_only.json"),
            r#"{"model":{"type":"minecraft:model","model":"other:item/replacement"}}"#,
        )
        .unwrap();

        let mut packs = crate::resource_pack::ResourcePackManager::new(&instance);
        packs.enable_local_pack("test_pack");
        let baked = bake_item_models(&jar, &None, Some(&packs));
        assert_eq!(
            baked.flat_texture_keys.get("test_item").map(String::as_str),
            Some("other:item/replacement")
        );
        assert_eq!(
            baked.flat_texture_keys.get("pack_only").map(String::as_str),
            Some("other:item/replacement")
        );
        let transform = baked.ground_transforms["test_item"];
        let origin = transform.transform_point3(Vec3::ZERO);
        assert!((origin - Vec3::new(0.0, 2.0 / 16.0, 0.0)).length() < 1.0e-6);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bare_face_texture_slot_matches_vanilla_texture_slots() {
        let root = test_temp_dir("bare_texture_slot");
        let models = root.join("minecraft/models/block");
        std::fs::create_dir_all(&models).unwrap();
        std::fs::write(
            models.join("test_core.json"),
            r#"{
                "textures":{"all":"block/test_core"},
                "elements":[{
                    "from":[4,0,4],"to":[12,8,12],
                    "faces":{
                        "down":{"texture":"all"},"up":{"texture":"all"},
                        "north":{"texture":"all"},"south":{"texture":"all"},
                        "west":{"texture":"all"},"east":{"texture":"all"}
                    }
                }]
            }"#,
        )
        .unwrap();

        let mut cache = HashMap::new();
        let resolved = resolve_model("block/test_core", &root, &None, &mut cache, None);
        let baked = bake_resolved_model(&resolved, 0, 0, false, |_| Tint::None).unwrap();
        assert_eq!(baked.quads.len(), 6);
        assert!(baked.quads.iter().all(|quad| quad.texture == "test_core"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ground_display_transform_matches_vanilla_centered_mesh() {
        let json = serde_json::json!({
            "rotation": [0.0, 0.0, 0.0],
            "translation": [0.0, 3.0, 0.0],
            "scale": [0.5, 0.5, 0.5]
        });
        let transform = parse_display_transform(&json).unwrap().to_matrix();

        // ItemTransform applies translation/rotation/scale and then recenters
        // vanilla's 0..1 model. Pomme stores its item mesh already centered,
        // so the equivalent matrix leaves out only that final -0.5 step.
        let min = transform.transform_point3(Vec3::splat(-0.5));
        let max = transform.transform_point3(Vec3::splat(0.5));
        assert!((min - Vec3::new(-0.25, -0.0625, -0.25)).length() < 1e-6);
        assert!((max - Vec3::new(0.25, 0.4375, 0.25)).length() < 1e-6);
    }

    #[test]
    fn plain_item_is_a_single_part() {
        let json: serde_json::Value = serde_json::from_str(
            r#"{"model": {"type": "minecraft:model", "model": "minecraft:block/oak_stairs"}}"#,
        )
        .unwrap();
        let parts = collect_model_parts(&json);
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].path, "block/oak_stairs");
        assert!(parts[0].transform.is_none());
    }

    #[test]
    fn multipart_conditions_follow_vanilla_truth_table() {
        let props = crate::world::block::PropMap::from_pairs(vec![
            ("north", "true"),
            ("east", "false"),
            ("shape", "inner_left"),
        ]);
        let or = parse_condition(&serde_json::json!({
            "OR": [{"north": "false"}, {"east": "false"}]
        }))
        .unwrap();
        let and = parse_condition(&serde_json::json!({
            "AND": [{"north": "true"}, {"shape": "inner_left|outer_left"}]
        }))
        .unwrap();
        let pipe = parse_condition(&serde_json::json!({"shape": "inner_right|inner_left"})).unwrap();
        let negated = parse_condition(&serde_json::json!({"north": "!false"})).unwrap();
        assert!(or.matches(&props));
        assert!(and.matches(&props));
        assert!(pipe.matches(&props));
        assert!(negated.matches(&props));
        assert!(!parse_condition(&serde_json::json!({"missing": "true"}))
            .unwrap()
            .matches(&props));
        assert_eq!(parse_when_condition(&None), WhenCondition::Always);
        assert_eq!(parse_condition(&serde_json::json!({})).unwrap(), WhenCondition::Always);
        assert!(parse_condition(&serde_json::json!({"AND": []})).unwrap().matches(&props));
        assert!(!parse_condition(&serde_json::json!({"OR": []})).unwrap().matches(&props));
        assert!(parse_condition(&serde_json::json!({"NOT": {"north": "false"}})).is_err());
        assert!(parse_condition(&serde_json::json!({"XOR": []})).is_err());
        assert_eq!(parse_when_condition(&Some(serde_json::json!({"OR": true}))), WhenCondition::Never);
    }

    #[test]
    fn uvlock_uses_inverse_face_basis_and_plain_uv_is_unchanged() {
        let plain = face_uvs(
            Direction::Up,
            [0.0; 3],
            [1.0; 3],
            Some(&[0.0, 0.0, 16.0, 16.0]),
            None,
            false,
            0,
            90,
        );
        let locked = face_uvs(
            Direction::Up,
            [0.0; 3],
            [1.0; 3],
            Some(&[0.0, 0.0, 16.0, 16.0]),
            None,
            true,
            0,
            90,
        );
        // uvlock=false keeps explicit UVs unchanged even when geometry rotates.
        assert_eq!(plain, [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]]);
        // Java 26.2 BlockMath.getFaceTransformation(BLOCK_ROT_Y_90, UP)
        // inverted through FaceBakery gives this cycle.
        let expected = [[1.0, 0.0], [0.0, 0.0], [0.0, 1.0], [1.0, 1.0]];
        for (actual, expected) in locked.into_iter().zip(expected) {
            assert!((actual[0] - expected[0]).abs() < 1.0e-6);
            assert!((actual[1] - expected[1]).abs() < 1.0e-6);
        }
        assert_eq!(
            face_uvs(Direction::South, [0.0; 3], [1.0; 3], None, None, false, 0, 90),
            face_uvs(Direction::South, [0.0; 3], [1.0; 3], None, None, false, 0, 0)
        );
    }

    #[test]
    fn invalid_weights_are_rejected_without_panicking() {
        assert_eq!(validate_weight_values([0]), Err("weight must be a positive int"));
        assert_eq!(validate_weight_values([u64::MAX]), Err("weight exceeds POSITIVE_INT/i32::MAX"));
        assert_eq!(validate_weight_values([1u64 << 32]), Err("weight exceeds POSITIVE_INT/i32::MAX"));
        assert_eq!(validate_weight_values([i32::MAX as u64, 1]), Err("weight sum exceeds i32::MAX"));
        assert_eq!(validate_weight_values([1, 2]), Ok(3));
        assert!(serde_json::from_str::<ModelRef>(
            r#"{"model":"minecraft:block/test","weight":-1}"#
        )
        .is_err());
        assert!(choose_baked_model(&[], 0).is_none());
    }

    #[test]
    fn weighted_selection_matches_legacy_random_anchor_seeds() {
        let a = BakedModel { quads: Vec::new(), is_full_cube: false, occludes: false };
        let b = a.clone();
        let choices = vec![
            WeightedBakedModel { weight: 1, model: a },
            WeightedBakedModel { weight: 1, model: b },
        ];
        let first = &choices[0].model as *const _;
        let second = &choices[1].model as *const _;
        // Java 26.2 LegacyRandomSource.nextInt(2): 0 -> 1, 1 -> 1, -1 -> 0.
        assert_eq!(choose_baked_model(&choices, 0).unwrap() as *const _, second);
        assert_eq!(choose_baked_model(&choices, 1).unwrap() as *const _, second);
        assert_eq!(choose_baked_model(&choices, -1).unwrap() as *const _, first);
    }

    #[test]
    fn vanilla_position_seed_and_weighted_lookup_are_deterministic() {
        assert_eq!(model_seed_for_position(3, 70, 1), -108_665_848_602_893);
        assert_eq!(
            multipart_seed_for_position(3, 70, 1),
            -1_913_033_443_730_608_672
        );
        let a = BakedModel { quads: Vec::new(), is_full_cube: false, occludes: false };
        let b = a.clone();
        let choices = vec![
            WeightedBakedModel { weight: 1, model: a },
            WeightedBakedModel { weight: 1, model: b },
        ];
        let first = &choices[0].model as *const _;
        let second = &choices[1].model as *const _;
        let mut saw_first = false;
        let mut saw_second = false;
        for x in -64..64 {
            let seed = model_seed_for_position(x, 70, x * 3);
            let selected = choose_baked_model(&choices, seed).unwrap() as *const _;
            saw_first |= selected == first;
            saw_second |= selected == second;
        }
        assert!(saw_first && saw_second);
    }
}
