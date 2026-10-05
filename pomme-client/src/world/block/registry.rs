use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::OnceLock;

use azalea_block::BlockState;
use serde::{Deserialize, Serialize};

// v10 invalidates v9 after item tint semantics became part of baked-model
// provenance; the cache still stores face textures only and old files remain.
pub const BLOCK_CACHE_FILE: &str = "block_cache_v10.json";

use super::model;
use super::model::{BakedModel, MultipartEntry, WeightedBakedModel};
use crate::assets::AssetIndex;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tint {
    None,
    Grass,
    Foliage,
    DryFoliage,
    /// Fixed vanilla block color, independent of biome.
    Fixed([u8; 3]),
    /// Power-level color, resolved at mesh time from the state's `power`.
    Redstone,
    /// Growth-age color, resolved at mesh time from the state's `age`.
    Stem,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct FaceTextures {
    pub top: String,
    pub bottom: String,
    pub north: String,
    pub south: String,
    pub east: String,
    pub west: String,
    pub side_overlay: Option<String>,
    pub tint: Tint,
    /// The model's `particle` texture slot (vanilla `getParticleMaterial`),
    /// used for block-break particles.
    #[serde(default)]
    pub particle: Option<String>,
}

impl FaceTextures {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        top: &str,
        bottom: &str,
        north: &str,
        south: &str,
        east: &str,
        west: &str,
        side_overlay: Option<&str>,
        tint: Tint,
    ) -> Self {
        Self {
            top: top.into(),
            bottom: bottom.into(),
            north: north.into(),
            south: south.into(),
            east: east.into(),
            west: west.into(),
            side_overlay: side_overlay.map(Into::into),
            tint,
            particle: None,
        }
    }

    pub fn uniform(name: &str, tint: Tint) -> Self {
        Self::new(name, name, name, name, name, name, None, tint)
    }
}

#[derive(Clone)]
pub struct BlockRegistry {
    textures: HashMap<String, FaceTextures>,
    baked: HashMap<String, HashMap<String, Vec<WeightedBakedModel>>>,
    multipart: HashMap<String, Vec<MultipartEntry>>,
    item_models: HashMap<String, BakedModel>,
    flat_item_textures: std::collections::HashSet<String>,
    flat_item_texture_keys: HashMap<String, String>,
    item_particle_icons: HashMap<String, String>,
    item_definitions: HashMap<String, serde_json::Value>,
    model_particle_icons: HashMap<String, Vec<Option<String>>>,
    flat_item_tints: HashMap<String, model::ItemTint>,
    item_ground_transforms: HashMap<String, glam::Mat4>,
    item_fixed_transforms: HashMap<String, glam::Mat4>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ItemParticleModelContext<'a> {
    /// The ClientLevel dimension passed by BreakingItemParticle's provider.
    pub dimension: Option<&'a str>,
    pub registries: Option<&'a azalea_core::registry_holder::RegistryHolder>,
}

#[derive(Clone, Copy)]
enum RawComponent<'a> {
    Missing,
    Removed,
    Set(&'a simdnbt::owned::NbtTag),
}

fn raw_component<'a>(raw: Option<&'a simdnbt::owned::NbtCompound>, name: &str) -> RawComponent<'a> {
    let Some(raw) = raw else {
        return RawComponent::Missing;
    };
    if raw.get(format!("!minecraft:{name}").as_str()).is_some()
        || raw.get(format!("!{name}").as_str()).is_some()
    {
        return RawComponent::Removed;
    }
    raw.get(format!("minecraft:{name}").as_str())
        .or_else(|| raw.get(name))
        .map_or(RawComponent::Missing, RawComponent::Set)
}

fn typed_or_default<T>(item: &azalea_inventory::ItemStackData) -> Option<T>
where
    T: azalea_inventory::components::DataComponentTrait
        + azalea_inventory::default_components::DefaultableComponent,
{
    if item.component_patch.has_kind(T::KIND) {
        item.component_patch.get::<T>().cloned()
    } else {
        azalea_inventory::default_components::get_default_component::<T>(item.kind)
    }
}

fn item_particle_component_present(
    item: &azalea_inventory::ItemStackData,
    raw: Option<&simdnbt::owned::NbtCompound>,
    name: &str,
) -> bool {
    if !matches!(raw_component(raw, name), RawComponent::Missing) {
        return matches!(raw_component(raw, name), RawComponent::Set(_));
    }
    let kind = match name {
        "lodestone_tracker" => azalea_registry::builtin::DataComponentKind::LodestoneTracker,
        "dyed_color" => azalea_registry::builtin::DataComponentKind::DyedColor,
        "trim" => azalea_registry::builtin::DataComponentKind::Trim,
        "block_state" => azalea_registry::builtin::DataComponentKind::BlockState,
        "charged_projectiles" => azalea_registry::builtin::DataComponentKind::ChargedProjectiles,
        "custom_model_data" => azalea_registry::builtin::DataComponentKind::CustomModelData,
        "damage" => azalea_registry::builtin::DataComponentKind::Damage,
        "max_damage" => azalea_registry::builtin::DataComponentKind::MaxDamage,
        "max_stack_size" => azalea_registry::builtin::DataComponentKind::MaxStackSize,
        _ => return false,
    };
    if item.component_patch.has_kind(kind) {
        return item
            .component_patch
            .iter()
            .any(|(entry_kind, value)| entry_kind == kind && value.is_some());
    }
    match name {
        "damage" => azalea_inventory::default_components::get_default_component::<
            azalea_inventory::components::Damage,
        >(item.kind)
        .is_some(),
        "max_damage" => azalea_inventory::default_components::get_default_component::<
            azalea_inventory::components::MaxDamage,
        >(item.kind)
        .is_some(),
        "max_stack_size" => azalea_inventory::default_components::get_default_component::<
            azalea_inventory::components::MaxStackSize,
        >(item.kind)
        .is_some(),
        "block_state" => azalea_inventory::default_components::get_default_component::<
            azalea_inventory::components::BlockState,
        >(item.kind)
        .is_some(),
        "charged_projectiles" => azalea_inventory::default_components::get_default_component::<
            azalea_inventory::components::ChargedProjectiles,
        >(item.kind)
        .is_some(),
        "custom_model_data" => azalea_inventory::default_components::get_default_component::<
            azalea_inventory::components::CustomModelData,
        >(item.kind)
        .is_some(),
        "lodestone_tracker" => azalea_inventory::default_components::get_default_component::<
            azalea_inventory::components::LodestoneTracker,
        >(item.kind)
        .is_some(),
        "trim" => azalea_inventory::default_components::get_default_component::<
            azalea_inventory::components::Trim,
        >(item.kind)
        .is_some(),
        "dyed_color" => azalea_inventory::default_components::get_default_component::<
            azalea_inventory::components::DyedColor,
        >(item.kind)
        .is_some(),
        _ => false,
    }
}

fn item_model_component_value<'a>(
    item: &azalea_inventory::ItemStackData,
    raw: Option<&'a simdnbt::owned::NbtCompound>,
    name: &str,
) -> Option<serde_json::Value> {
    match raw_component(raw, name) {
        RawComponent::Removed => return None,
        RawComponent::Set(value) => return nbt_json_value(value),
        RawComponent::Missing => {}
    }
    let component = match name {
        "custom_model_data" => serde_json::to_value(custom_model_data(item, raw)?).ok()?,
        "damage" => serde_json::json!(
            typed_or_default::<azalea_inventory::components::Damage>(item)?.amount
        ),
        "max_damage" => serde_json::json!(
            typed_or_default::<azalea_inventory::components::MaxDamage>(item)?.amount
        ),
        "max_stack_size" => serde_json::json!(
            typed_or_default::<azalea_inventory::components::MaxStackSize>(item)?.count
        ),
        "block_state" => serde_json::to_value(
            typed_or_default::<azalea_inventory::components::BlockState>(item)?.properties,
        )
        .ok()?,
        "dyed_color" => serde_json::to_value(
            item.component_patch
                .get::<azalea_inventory::components::DyedColor>()?,
        )
        .ok()?,
        _ => return None,
    };
    Some(component)
}

fn nbt_json_value(tag: &simdnbt::owned::NbtTag) -> Option<serde_json::Value> {
    use simdnbt::owned::NbtTag as T;
    Some(match tag {
        T::Byte(v) => serde_json::json!(v),
        T::Short(v) => serde_json::json!(v),
        T::Int(v) => serde_json::json!(v),
        T::Long(v) => serde_json::json!(v),
        T::Float(v) if v.is_finite() => serde_json::json!(v),
        T::Double(v) if v.is_finite() => serde_json::json!(v),
        T::String(v) => serde_json::Value::String(v.to_string()),
        T::Compound(c) => serde_json::Value::Object(
            c.iter()
                .map(|(k, v)| Some((k.to_string(), nbt_json_value(v)?)))
                .collect::<Option<_>>()?,
        ),
        T::List(list) => serde_json::Value::Array(
            list.as_nbt_tags()
                .iter()
                .map(nbt_json_value)
                .collect::<Option<_>>()?,
        ),
        _ => return None,
    })
}

fn custom_model_data(
    item: &azalea_inventory::ItemStackData,
    raw: Option<&simdnbt::owned::NbtCompound>,
) -> Option<azalea_inventory::components::CustomModelData> {
    use simdnbt::owned::{NbtList, NbtTag};
    match raw_component(raw, "custom_model_data") {
        RawComponent::Removed => None,
        RawComponent::Set(NbtTag::Compound(data)) => {
            let floats = match data.get("floats") {
                None => Vec::new(),
                Some(NbtTag::List(NbtList::Float(v))) if v.iter().all(|x| x.is_finite()) => {
                    v.clone()
                }
                _ => return None,
            };
            let flags = match data.get("flags") {
                None => Vec::new(),
                Some(NbtTag::List(NbtList::Byte(v))) => v.iter().map(|x| *x != 0).collect(),
                _ => return None,
            };
            let strings = match data.get("strings") {
                None => Vec::new(),
                Some(NbtTag::List(NbtList::String(v))) => v
                    .iter()
                    .map(|value| value.to_string_lossy().into_owned())
                    .collect(),
                _ => return None,
            };
            let colors = match data.get("colors") {
                None => Vec::new(),
                Some(NbtTag::List(NbtList::Int(v))) => v.clone(),
                _ => return None,
            };
            Some(azalea_inventory::components::CustomModelData {
                floats,
                flags,
                strings,
                colors,
            })
        }
        RawComponent::Set(_) => None,
        RawComponent::Missing => item
            .component_patch
            .get::<azalea_inventory::components::CustomModelData>()
            .cloned()
            .or_else(|| azalea_inventory::default_components::get_default_component(item.kind)),
    }
}

fn item_model_particle_value(
    property: &str,
    node: &serde_json::Value,
    item: &azalea_inventory::ItemStackData,
    raw: Option<&simdnbt::owned::NbtCompound>,
    context: ItemParticleModelContext<'_>,
) -> Option<serde_json::Value> {
    use azalea_registry::DataRegistry;
    use simdnbt::owned::NbtTag;
    let property = property.strip_prefix("minecraft:").unwrap_or(property);
    Some(match property {
        "custom_model_data" => serde_json::to_value(custom_model_data(item, raw)?).ok()?,
        "display_context" => serde_json::json!("ground"),
        "context_dimension" => serde_json::json!(context.dimension?),
        "context_entity_type" | "main_hand" => return None,
        "block_state" => {
            let key = node.get("block_state_property")?.as_str()?;
            let value = match raw_component(raw, property) {
                RawComponent::Removed => None,
                RawComponent::Set(NbtTag::Compound(values)) => {
                    values.get(key).and_then(|v| match v {
                        NbtTag::String(s) => Some(s.to_string()),
                        _ => None,
                    })
                }
                RawComponent::Set(_) => return None,
                RawComponent::Missing => {
                    typed_or_default::<azalea_inventory::components::BlockState>(item)?
                        .properties
                        .get(key)
                        .cloned()
                }
            }?;
            serde_json::Value::String(value)
        }
        "charge_type" => {
            let projectiles = match raw_component(raw, "charged_projectiles") {
                RawComponent::Removed => None,
                RawComponent::Set(NbtTag::List(list)) => Some(
                    list.as_nbt_tags()
                        .iter()
                        .filter_map(|v| match v {
                            NbtTag::Compound(c) => c.string("id").map(|s| s.to_string()),
                            _ => None,
                        })
                        .collect::<Vec<_>>(),
                ),
                RawComponent::Set(_) => return None,
                RawComponent::Missing => typed_or_default::<
                    azalea_inventory::components::ChargedProjectiles,
                >(item)
                .map(|p| {
                    p.items
                        .into_iter()
                        .filter_map(|s| match s {
                            azalea_inventory::ItemStack::Present(d) => {
                                Some(crate::player::inventory::item_resource_name(d.kind))
                            }
                            _ => None,
                        })
                        .collect()
                }),
            };
            let value = match projectiles.as_deref().unwrap_or_default() {
                [] => "none",
                names if names.iter().any(|n| n.ends_with("firework_rocket")) => "rocket",
                _ => "arrow",
            };
            serde_json::Value::String(value.into())
        }
        "trim_material" => {
            let name = match raw_component(raw, "trim") {
                RawComponent::Removed => None,
                RawComponent::Set(NbtTag::Compound(trim)) => {
                    trim.get("material").and_then(|v| match v {
                        NbtTag::String(s) => Some(s.to_string()),
                        NbtTag::Compound(c) => c.string("id").map(|s| s.to_string()),
                        _ => None,
                    })
                }
                RawComponent::Set(_) => return None,
                RawComponent::Missing => {
                    typed_or_default::<azalea_inventory::components::Trim>(item).and_then(|trim| {
                        let id = trim.material.protocol_id();
                        context
                            .registries?
                            .protocol_id_to_identifier("minecraft:trim_material".into(), id)
                            .map(ToString::to_string)
                    })
                }
            }?;
            let resource = if name.contains(':') {
                name
            } else {
                format!("minecraft:{name}")
            };
            serde_json::Value::String(resource)
        }
        "local_time" => {
            let pattern = node.get("pattern")?.as_str()?;
            serde_json::Value::String(java_local_time_pattern(pattern, chrono::Local::now()))
        }
        _ => return None,
    })
}

fn java_local_time_pattern(pattern: &str, now: chrono::DateTime<chrono::Local>) -> String {
    use chrono::{Datelike, Timelike};
    let mut result = String::new();
    let mut chars = pattern.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\'' {
            continue;
        }
        let count = 1 + chars.clone().take_while(|next| *next == ch).count();
        for _ in 1..count {
            chars.next();
        }
        let value = match ch {
            'y' => format!(
                "{:0width$}",
                now.format("%Y").to_string().parse::<i32>().unwrap_or(0),
                width = count
            ),
            'M' => format!("{:0width$}", now.month(), width = count),
            'd' => format!("{:0width$}", now.day(), width = count),
            'H' => format!("{:0width$}", now.hour(), width = count),
            'm' => format!("{:0width$}", now.minute(), width = count),
            's' => format!("{:0width$}", now.second(), width = count),
            _ => std::iter::repeat_n(ch, count).collect(),
        };
        result.push_str(&value);
    }
    result
}

fn select_particle_models<'a>(
    node: &'a serde_json::Value,
    item: &azalea_inventory::ItemStackData,
    raw: Option<&simdnbt::owned::NbtCompound>,
    context: ItemParticleModelContext<'_>,
    model_icons: &'a HashMap<String, Vec<Option<String>>>,
    item_icons: &'a HashMap<String, String>,
) -> Option<Vec<Option<&'a str>>> {
    use simdnbt::owned::NbtTag;
    let kind = node
        .get("type")?
        .as_str()?
        .strip_prefix("minecraft:")
        .unwrap_or(node.get("type")?.as_str()?);
    match kind {
        "bundle/selected_item" => Some(Vec::new()),
        "model" => {
            let name = crate::assets::AssetId::parse(node.get("model")?.as_str()?).canonical();
            Some(
                model_icons
                    .get(&name)
                    .map(|icons| icons.iter().map(|icon| icon.as_deref()).collect())
                    .or_else(|| item_icons.get(&name).map(|icon| vec![Some(icon.as_str())]))
                    .unwrap_or_else(|| vec![None]),
            )
        }
        // Java SpecialModelWrapper adds a layer with particleMaterial=null.
        "special" => Some(vec![None]),
        "composite" => {
            let mut icons = Vec::new();
            for part in node.get("models")?.as_array()? {
                if let Some(part_icons) =
                    select_particle_models(part, item, raw, context, model_icons, item_icons)
                {
                    icons.extend(part_icons);
                }
            }
            (!icons.is_empty()).then_some(icons)
        }
        "condition" => {
            let prop = node.get("property")?.as_str()?;
            let value = if prop.ends_with("custom_model_data") {
                let idx = node
                    .get("index")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0) as usize;
                let flags = custom_model_data(item, raw).map(|v| v.flags);
                flags.and_then(|v| v.get(idx).copied()).unwrap_or(false)
            } else if prop.ends_with("has_component") {
                item_particle_component_present(
                    item,
                    raw,
                    node.get("component")?
                        .as_str()?
                        .strip_prefix("minecraft:")
                        .unwrap_or(node.get("component")?.as_str()?),
                )
            } else if prop.ends_with("using_item") || prop.ends_with("fishing_rod/cast") {
                false // Java provider supplies owner = null.
            } else if prop.ends_with("broken") {
                let damage = match raw_component(raw, "damage") {
                    RawComponent::Set(NbtTag::Int(v)) => *v,
                    RawComponent::Removed => 0,
                    RawComponent::Missing => {
                        typed_or_default::<azalea_inventory::components::Damage>(item)
                            .map_or(0, |v| v.amount)
                    }
                    _ => return None,
                };
                let max = match raw_component(raw, "max_damage") {
                    RawComponent::Set(NbtTag::Int(v)) => *v,
                    RawComponent::Removed => 0,
                    RawComponent::Missing => {
                        typed_or_default::<azalea_inventory::components::MaxDamage>(item)
                            .map_or(0, |v| v.amount)
                    }
                    _ => return None,
                };
                max > 0 && damage + 1 >= max
            } else if prop.ends_with("bundle/has_selected_item") {
                false // ItemDisplayContext.GROUND doesn't expose bundle selection UI state.
            } else {
                return None;
            };
            let branch = if value {
                node.get("on_true")?
            } else {
                node.get("on_false")?
            };
            select_particle_models(branch, item, raw, context, model_icons, item_icons)
        }
        "select" => {
            let prop = node.get("property")?.as_str()?;
            let value = if prop.ends_with("custom_model_data") {
                let index = node
                    .get("index")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0) as usize;
                custom_model_data(item, raw)
                    .and_then(|data| data.strings.get(index).cloned())
                    .map(serde_json::Value::String)
            } else if prop.ends_with("component") {
                let component = node
                    .get("component")?
                    .as_str()?
                    .strip_prefix("minecraft:")
                    .unwrap_or(node.get("component")?.as_str()?);
                item_model_component_value(item, raw, component)
            } else {
                item_model_particle_value(prop, node, item, raw, context)
            };
            let cases = node.get("cases")?.as_array()?;
            let chosen = value
                .as_ref()
                .and_then(|value| {
                    cases.iter().find(|case| {
                        let when = &case["when"];
                        when == value
                            || when
                                .as_array()
                                .is_some_and(|values| values.iter().any(|v| v == value))
                    })
                })
                .and_then(|case| case.get("model"));
            select_particle_models(
                chosen.unwrap_or(node.get("fallback")?),
                item,
                raw,
                context,
                model_icons,
                item_icons,
            )
        }
        "range_dispatch" => {
            let property = node.get("property")?.as_str()?;
            let prop = property.strip_prefix("minecraft:").unwrap_or(property);
            let value = match prop {
                "custom_model_data" => {
                    let index = node
                        .get("index")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0) as usize;
                    let data = custom_model_data(item, raw)?;
                    *data.floats.get(index)?
                }
                "damage" => {
                    let damage = match raw_component(raw, "damage") {
                        RawComponent::Set(NbtTag::Int(v)) => *v,
                        RawComponent::Removed => 0,
                        RawComponent::Missing => {
                            typed_or_default::<azalea_inventory::components::Damage>(item)
                                .map_or(0, |v| v.amount)
                        }
                        _ => return None,
                    } as f32;
                    let max = match raw_component(raw, "max_damage") {
                        RawComponent::Set(NbtTag::Int(v)) => *v,
                        RawComponent::Removed => 0,
                        RawComponent::Missing => {
                            typed_or_default::<azalea_inventory::components::MaxDamage>(item)
                                .map_or(0, |v| v.amount)
                        }
                        _ => return None,
                    } as f32;
                    if max <= 0.0 {
                        f32::NAN
                    } else if node
                        .get("normalize")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(true)
                    {
                        (damage / max).clamp(0.0, 1.0)
                    } else {
                        damage.clamp(0.0, max)
                    }
                }
                "count" => {
                    let max = match raw_component(raw, "max_stack_size") {
                        RawComponent::Set(NbtTag::Int(v)) => *v,
                        RawComponent::Removed => 64,
                        RawComponent::Missing => {
                            typed_or_default::<azalea_inventory::components::MaxStackSize>(item)
                                .map_or(64, |v| v.count)
                        }
                        _ => return None,
                    } as f32;
                    let count = item.count as f32;
                    if node
                        .get("normalize")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(true)
                    {
                        (count / max).clamp(0.0, 1.0)
                    } else {
                        count.clamp(0.0, max)
                    }
                }
                // NeedleDirectionHelper, UseDuration, UseCycle and CrossbowPull all return 0 when
                // owner is null.
                "compass" | "time" | "use_duration" | "use_cycle" | "crossbow/pull" => 0.0,
                "bundle/fullness" => 0.0,
                _ => return None,
            } * node
                .get("scale")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(1.0) as f32;
            let entries = node.get("entries")?.as_array()?;
            let selected = entries
                .iter()
                .filter_map(|entry| {
                    let threshold = entry.get("threshold")?.as_f64()? as f32;
                    threshold
                        .is_finite()
                        .then_some((threshold, entry.get("model")?))
                })
                .filter(|(threshold, _)| *threshold <= value)
                .max_by(|a, b| a.0.total_cmp(&b.0))
                .map(|(_, model)| model);
            let branch = selected.or_else(|| node.get("fallback"))?;
            select_particle_models(branch, item, raw, context, model_icons, item_icons)
        }
        _ => None,
    }
}

fn baked_choices_satisfy(
    choices: &[WeightedBakedModel],
    predicate: impl Fn(&BakedModel) -> bool,
) -> bool {
    !choices.is_empty() && choices.iter().all(|choice| predicate(&choice.model))
}

impl BlockRegistry {
    #[cfg(test)]
    pub(crate) fn test_empty() -> Self {
        Self {
            textures: HashMap::new(),
            baked: HashMap::new(),
            multipart: HashMap::new(),
            item_models: HashMap::new(),
            flat_item_textures: Default::default(),
            flat_item_texture_keys: HashMap::new(),
            item_particle_icons: HashMap::new(),
            item_definitions: HashMap::new(),
            model_particle_icons: HashMap::new(),
            flat_item_tints: HashMap::new(),
            item_ground_transforms: HashMap::new(),
            item_fixed_transforms: HashMap::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn test_set_item_particle_model(
        &mut self,
        item: &str,
        definition: serde_json::Value,
        leaves: &[(&str, &str)],
    ) {
        self.item_definitions
            .insert(item.into(), serde_json::json!({"model":definition}));
        for (model, texture) in leaves {
            self.model_particle_icons
                .insert((*model).into(), vec![Some((*texture).into())]);
        }
    }

    #[cfg(test)]
    pub(crate) fn test_add_particle_fixture(&mut self) {
        self.textures.insert(
            "stone".into(),
            FaceTextures::uniform("fixture/stone", Tint::None),
        );
        let mut textures = self.textures.get("stone").unwrap().clone();
        textures.particle = Some("fixture/block_particle".into());
        self.textures.insert("stone".into(), textures);
        for item in ["stone", "slime_ball", "cobweb", "snowball"] {
            self.item_particle_icons
                .insert(item.into(), "fixture/item_particle".into());
        }
    }

    pub fn load(
        jar_assets_dir: &Path,
        asset_index: &Option<AssetIndex>,
        game_dir: &Path,
        packs: Option<&crate::resource_pack::ResourcePackManager>,
    ) -> Self {
        let cache_path = game_dir.join(BLOCK_CACHE_FILE);

        let textures = if packs.is_none() {
            if let Some(cached) = load_cache(&cache_path) {
                tracing::info!("Block registry: {} blocks (cached textures)", cached.len());
                Some(cached)
            } else {
                None
            }
        } else {
            None
        };

        let textures = textures.unwrap_or_else(|| {
            let mut textures = model::load_all_block_textures(jar_assets_dir, asset_index, packs);

            textures
                .entry("water".into())
                .or_insert_with(|| FaceTextures::uniform("water_still", Tint::None));
            textures
                .entry("lava".into())
                .or_insert_with(|| FaceTextures::uniform("lava_still", Tint::None));

            save_cache(&cache_path, &textures);
            tracing::info!(
                "Block registry: {} blocks (built and cached)",
                textures.len()
            );
            textures
        });

        let (baked, multipart) = model::bake_all_models(jar_assets_dir, asset_index, packs);
        let baked_items = model::bake_item_models(jar_assets_dir, asset_index, packs);
        let item_models = baked_items.models;
        let flat_item_textures = baked_items.generated_textures;
        let flat_item_texture_keys = baked_items.flat_texture_keys;
        let item_particle_icons = baked_items.particle_icons;
        let item_definitions = baked_items.item_definitions;
        let model_particle_icons = baked_items.model_particle_icons;
        let flat_item_tints = baked_items.flat_tints;
        let item_ground_transforms = baked_items.ground_transforms;
        let item_fixed_transforms = baked_items.fixed_transforms;

        Self {
            textures,
            baked,
            multipart,
            item_models,
            flat_item_textures,
            flat_item_texture_keys,
            item_particle_icons,
            item_definitions,
            model_particle_icons,
            flat_item_tints,
            item_ground_transforms,
            item_fixed_transforms,
        }
    }

    /// All mapped BlockItems start from the active protocol's vanilla default.
    pub fn placeable_block_for_item(&self, item_name: &str) -> Option<BlockState> {
        super::default_state_of(block_for_item(item_name)?)
    }

    pub fn get_item_model(&self, name: &str) -> Option<&BakedModel> {
        self.item_models.get(name)
    }

    /// Every item with a baked 3D model or a generated flat sprite.
    pub fn item_names(&self) -> impl Iterator<Item = &str> + '_ {
        self.item_models
            .keys()
            .chain(self.flat_item_texture_keys.keys())
            .map(String::as_str)
    }

    pub fn flat_item_textures(&self) -> impl Iterator<Item = &str> + '_ {
        self.flat_item_textures.iter().map(String::as_str)
    }

    pub fn get_flat_item_texture_key(&self, name: &str) -> Option<&str> {
        self.flat_item_texture_keys.get(name).map(String::as_str)
    }

    /// Canonical item-definition key selected for this stack. Explicit item
    /// models are preserved even when no corresponding asset was baked.
    pub fn item_model_name(stack: &azalea_inventory::ItemStack) -> Option<String> {
        let item = stack.as_present().filter(|item| !item.is_empty())?;
        Some(
            item.component_patch
                .get::<azalea_inventory::components::ItemModel>()
                .map(|model| {
                    crate::assets::AssetId::parse(&model.resource_location.to_string()).canonical()
                })
                .unwrap_or_else(|| crate::player::inventory::item_resource_name(item.kind)),
        )
    }

    /// Resolve Java's GROUND item model into its ordered layer particle
    /// materials. The item-model resolver seed is 0; layer selection uses the
    /// particle provider's live RNG after this method returns.
    pub fn get_item_particle_materials<'a>(
        &'a self,
        stack: &azalea_inventory::ItemStack,
        raw_components: Option<&simdnbt::owned::NbtCompound>,
        context: ItemParticleModelContext<'_>,
    ) -> Option<Vec<Option<&'a str>>> {
        let item = stack.as_present().filter(|item| !item.is_empty())?;
        let raw_item_model = raw_component(raw_components, "item_model");
        let typed_item_model_removed = item.component_patch.iter().any(|(kind, value)| {
            kind == azalea_registry::builtin::DataComponentKind::ItemModel && value.is_none()
        });
        let model_name = match raw_item_model {
            RawComponent::Removed => crate::player::inventory::item_resource_name(item.kind),
            RawComponent::Set(simdnbt::owned::NbtTag::String(value)) => {
                crate::assets::AssetId::parse(&value.to_string()).canonical()
            }
            RawComponent::Set(_) => return None,
            RawComponent::Missing if typed_item_model_removed => {
                crate::player::inventory::item_resource_name(item.kind)
            }
            RawComponent::Missing => Self::item_model_name(stack)?,
        };
        if let Some(definition) = self.item_definitions.get(&model_name) {
            return select_particle_models(
                definition.get("model")?,
                item,
                raw_components,
                context,
                &self.model_particle_icons,
                &self.item_particle_icons,
            )
            .filter(|materials| !materials.is_empty());
        }
        self.item_particle_icons
            .get(&model_name)
            .map(|icon| vec![Some(icon.as_str())])
    }

    pub fn get_item_particle_icon(&self, stack: &azalea_inventory::ItemStack) -> Option<&str> {
        self.get_item_particle_icon_with_raw(stack, None)
    }

    pub fn get_item_particle_icon_with_raw(
        &self,
        stack: &azalea_inventory::ItemStack,
        raw_components: Option<&simdnbt::owned::NbtCompound>,
    ) -> Option<&str> {
        self.get_item_particle_materials(
            stack,
            raw_components,
            ItemParticleModelContext::default(),
        )?
        .into_iter()
        .flatten()
        .next()
    }

    pub fn get_flat_item_tint(&self, name: &str) -> model::ItemTint {
        self.flat_item_tints.get(name).cloned().unwrap_or_default()
    }

    pub fn get_item_ground_transform(&self, name: &str) -> Option<glam::Mat4> {
        self.item_ground_transforms.get(name).copied()
    }

    pub fn get_item_fixed_transform(&self, name: &str) -> Option<glam::Mat4> {
        self.item_fixed_transforms.get(name).copied()
    }

    pub(crate) fn debug_item_snapshot(&self, name: &str) -> serde_json::Value {
        if let Some(model) = self.item_models.get(name) {
            return serde_json::json!({
                "item": name,
                "path": "3d_baked",
                "quads": model.quads.iter().map(|quad| serde_json::json!({
                    "texture": quad.texture,
                    "tintIndex": quad.tint_index,
                    "direction": quad.shade_face,
                    "positions": quad.positions,
                    "itemTint": quad.item_tint.debug_json(),
                })).collect::<Vec<_>>(),
                "provenance": "Rust item model bake; no block color source",
            });
        }
        let texture = self.flat_item_texture_keys.get(name);
        let tint = self.flat_item_tints.get(name).cloned().unwrap_or_default();
        serde_json::json!({
            "item": name,
            "path": "flat_generated",
            "texture": texture,
            "tintIndex": 0,
            "itemTint": tint.debug_json(),
            "provenance": "Rust generated item mesh input; no block color source",
        })
    }

    pub fn get_textures(&self, state: BlockState) -> Option<&FaceTextures> {
        self.textures.get(super::block_id(state))
    }

    /// Probe-only summary of the already-baked data used by the renderer.
    pub(crate) fn debug_model_snapshot(
        &self,
        state: BlockState,
        x: i32,
        y: i32,
        z: i32,
    ) -> serde_json::Value {
        let seed = model::model_seed_for_position(x, y, z);
        let baked_selection = self.get_baked_alternatives(state).and_then(|choices| {
            let selected = model::choose_baked_model(choices, seed)?;
            let index = choices
                .iter()
                .position(|choice| std::ptr::eq(&choice.model, selected))?;
            Some((index, selected.clone()))
        });
        let baked = baked_selection.as_ref().map(|(_, model)| {
            serde_json::json!({
                "quadCount": model.quads.len(),
                "tintedQuadCount": model.quads.iter().filter(|quad| quad.tint != Tint::None).count(),
                "quads": model.quads,
                "isFullCube": model.is_full_cube,
                "occludes": model.occludes,
            })
        });
        let multipart = self.get_multipart_quads_at(state, x, y, z).map(|quads| {
            serde_json::json!({
                "quadCount": quads.len(),
                "quads": quads,
            })
        });
        let multipart_quad_count = multipart
            .as_ref()
            .and_then(|value| value.get("quadCount"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        serde_json::json!({
            "block": super::block_id(state),
            "properties": super::block_properties(state).entries().collect::<HashMap<_, _>>(),
            "selection": {
                "seed": seed,
                "targetPos": [x, y, z],
                "selectedModelKey": baked_selection.as_ref().map(|(index, _)| format!("{}#{}", super::block_id(state), index)),
                "provenance": "position-seeded baked-model extraction; not actual GPU draw"
            },
            "faceTextures": self.get_textures(state).and_then(|textures| serde_json::to_value(textures).ok()),
            "baked": baked,
            "multipart": multipart,
            "multipartQuadCount": multipart_quad_count,
        })
    }

    pub fn get_baked_model(&self, state: BlockState) -> Option<&BakedModel> {
        self.get_baked_alternatives(state)?
            .first()
            .map(|choice| &choice.model)
    }

    /// Selects a variant with the same position-seeded weighted lookup as
    /// vanilla `ModelBlockRenderer.tesselateBlock`.
    pub fn get_baked_model_at(
        &self,
        state: BlockState,
        x: i32,
        y: i32,
        z: i32,
    ) -> Option<BakedModel> {
        let choices = self.get_baked_alternatives(state)?;
        let seed = model::model_seed_for_position(x, y, z);
        model::choose_baked_model(choices, seed).cloned()
    }

    fn get_baked_alternatives(&self, state: BlockState) -> Option<&Vec<WeightedBakedModel>> {
        let variants = self.baked.get(super::block_id(state))?;
        if variants.len() == 1 {
            return variants.values().next();
        }

        // Vanilla variant keys only list the properties that affect the model, so
        // match by subset rather than exact string equality (an empty key matches
        // any state, serving as the default variant).
        let props = super::block_properties(state);
        variants
            .iter()
            .find(|(key, _)| {
                constraints_match(props, key.split(',').filter_map(|p| p.split_once('=')))
            })
            .map(|(_, models)| models)
            .or_else(|| variants.values().next())
    }

    pub fn get_multipart_quads(&self, state: BlockState) -> Option<Vec<&model::BakedQuad>> {
        let entries = self.multipart.get(super::block_id(state))?;
        let props = super::block_properties(state);
        let quads: Vec<_> = entries
            .iter()
            .filter(|entry| entry.when.matches(props))
            .flat_map(|entry| {
                entry
                    .models
                    .first()
                    .into_iter()
                    .flat_map(|choice| choice.model.quads.iter())
            })
            .collect();
        if quads.is_empty() { None } else { Some(quads) }
    }

    pub fn get_multipart_quads_at(
        &self,
        state: BlockState,
        x: i32,
        y: i32,
        z: i32,
    ) -> Option<Vec<model::BakedQuad>> {
        let entries = self.multipart.get(super::block_id(state))?;
        let props = super::block_properties(state);
        let seed = model::multipart_seed_for_position(x, y, z);
        let mut quads = Vec::new();
        for entry in entries {
            if entry.when.matches(props)
                && let Some(selected) = model::choose_baked_model(&entry.models, seed)
            {
                quads.extend(selected.quads.iter().cloned());
            }
        }
        if quads.is_empty() { None } else { Some(quads) }
    }

    fn baked_model_flag(&self, state: BlockState, f: impl Fn(&BakedModel) -> bool) -> bool {
        if super::is_air(state) {
            return false;
        }
        self.get_baked_alternatives(state)
            .is_some_and(|choices| baked_choices_satisfy(choices, f))
    }

    pub fn is_opaque_full_cube(&self, state: BlockState) -> bool {
        self.baked_model_flag(state, |m| m.is_full_cube)
    }

    /// Whether `state` culls a neighbor's adjacent face. Callers do not always
    /// have the block position, so only cull when every weighted alternative
    /// occludes; choosing the first model here can erase faces at positions
    /// where `get_baked_model_at` selects a non-occluding alternative.
    pub fn occludes_neighbor(&self, state: BlockState) -> bool {
        if super::is_air(state) {
            return false;
        }
        self.baked_model_flag(state, |model| model.occludes)
    }

    pub fn texture_names(&self) -> impl Iterator<Item = &str> + '_ {
        let face_textures = self.textures.values().flat_map(|ft| {
            let base = [
                &ft.top, &ft.bottom, &ft.north, &ft.south, &ft.east, &ft.west,
            ];
            base.into_iter()
                .map(|s| s.as_str())
                .chain(ft.side_overlay.as_deref())
        });

        let baked_textures = self.baked.values().flat_map(|variants| {
            variants.values().flat_map(|choices| {
                choices
                    .iter()
                    .flat_map(|choice| choice.model.quads.iter().map(|q| q.texture.as_str()))
            })
        });

        let multipart_textures = self.multipart.values().flat_map(|entries| {
            entries.iter().flat_map(|entry| {
                entry
                    .models
                    .iter()
                    .flat_map(|choice| choice.model.quads.iter().map(|q| q.texture.as_str()))
            })
        });

        let item_model_textures = self
            .item_models
            .values()
            .flat_map(|model| model.quads.iter().map(|q| q.texture.as_str()));

        face_textures
            .chain(baked_textures)
            .chain(multipart_textures)
            .chain(item_model_textures)
            .chain(self.item_particle_icons.values().map(String::as_str))
            .chain(
                self.model_particle_icons
                    .values()
                    .flat_map(|icons| icons.iter().filter_map(|icon| icon.as_deref())),
            )
    }
}

#[derive(Deserialize)]
struct BlockItemDefinition {
    block: String,
}

#[derive(Deserialize)]
struct PlacementData {
    block_items: HashMap<String, BlockItemDefinition>,
    replaceable: HashSet<String>,
}

fn placement_data() -> &'static PlacementData {
    static DATA: OnceLock<PlacementData> = OnceLock::new();
    DATA.get_or_init(|| {
        // Extracted from the existing Steel vanilla data by tools/placement-data.mjs.
        serde_json::from_str(include_str!("data/placement-26.2.json"))
            .expect("invalid placement data")
    })
}

/// BlockItem capability is independent of whether we can predict its state.
/// The extracted association also covers aliases such as redstone and string.
pub fn block_for_item(item_name: &str) -> Option<&'static str> {
    placement_data()
        .block_items
        .get(item_name)
        .map(|item| item.block.as_str())
}

pub(super) fn block_is_replaceable(block_name: &str) -> bool {
    placement_data().replaceable.contains(block_name)
}

/// Whether every `key=value` constraint holds for `props`. A value may list
/// alternatives separated by `|`, as vanilla multipart `when` clauses do.
fn constraints_match<'a>(
    props: &super::PropMap,
    mut constraints: impl Iterator<Item = (&'a str, &'a str)>,
) -> bool {
    constraints.all(|(k, v)| {
        props
            .get(k)
            .is_some_and(|pv| v.split('|').any(|opt| opt == pv))
    })
}

fn load_cache(path: &Path) -> Option<HashMap<String, FaceTextures>> {
    let data = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

#[cfg(test)]
mod weighted_occlusion_tests {
    use super::*;

    #[test]
    fn mixed_occlusion_alternatives_never_hide_a_neighbor_face() {
        let choices = vec![
            WeightedBakedModel {
                weight: 1,
                model: BakedModel {
                    quads: Vec::new(),
                    ambient_occlusion: true,
                    is_full_cube: true,
                    occludes: true,
                },
            },
            WeightedBakedModel {
                weight: 1,
                model: BakedModel {
                    quads: Vec::new(),
                    ambient_occlusion: true,
                    is_full_cube: false,
                    occludes: false,
                },
            },
        ];
        let non_occluding_position = (0..1000)
            .find(|&x| {
                model::choose_baked_model(&choices, model::model_seed_for_position(x, 0, 0))
                    .is_some_and(|selected| !selected.occludes)
            })
            .expect("weighted selection reaches the non-occluding alternative");
        assert!(
            model::choose_baked_model(
                &choices,
                model::model_seed_for_position(non_occluding_position, 0, 0)
            )
            .is_some_and(|selected| !selected.occludes)
        );
        assert!(!baked_choices_satisfy(&choices, |model| model.occludes));
        assert!(!baked_choices_satisfy(&choices, |model| model.is_full_cube));
        assert!(baked_choices_satisfy(&choices[..1], |model| model.occludes));
    }
}

#[cfg(test)]
mod item_particle_tests {
    use azalea_inventory::ItemStack;
    use azalea_inventory::components::ItemModel;
    use azalea_registry::builtin::ItemKind;
    use azalea_registry::identifier::Identifier;

    use super::*;

    #[test]
    fn every_mapped_block_item_predicts_its_active_default_state() {
        let _protocol = crate::world::block::test_protocol_guard();
        super::super::init("26.2");
        let registry = BlockRegistry::test_empty();
        for (item, definition) in &placement_data().block_items {
            let expected = super::super::default_state_of(&definition.block)
                .unwrap_or_else(|| panic!("missing block {} for {item}", definition.block));
            assert_eq!(
                registry.placeable_block_for_item(item),
                Some(expected),
                "{item}"
            );
        }
        assert_eq!(registry.placeable_block_for_item("stick"), None);
    }

    #[test]
    fn item_particle_materials_use_legacy_icon_only_without_a_definition() {
        let mut registry = BlockRegistry::test_empty();
        registry
            .item_particle_icons
            .insert("stone".into(), "stone_icon".into());
        let stack = ItemStack::new(ItemKind::Stone, 1);
        assert_eq!(
            registry
                .get_item_particle_materials(&stack, None, ItemParticleModelContext::default(),),
            Some(vec![Some("stone_icon")])
        );

        registry.item_definitions.insert(
            "stone".into(),
            serde_json::json!({"model": {
                "type": "minecraft:select",
                "property": "minecraft:unknown_property",
                "fallback": {"type": "minecraft:model", "model": "minecraft:item/stone"}
            }}),
        );
        registry
            .model_particle_icons
            .insert("item/stone".into(), vec![Some("base_icon".into())]);
        assert_eq!(
            registry
                .get_item_particle_materials(&stack, None, ItemParticleModelContext::default(),),
            None
        );
    }

    #[test]
    fn raw_item_components_resolve_item_model_removal_and_custom_model_data_select() {
        use simdnbt::owned::{NbtCompound, NbtList};
        let mut registry = BlockRegistry::test_empty();
        registry
            .item_particle_icons
            .insert("stone".into(), "stone_icon".into());
        registry
            .item_particle_icons
            .insert("alternate".into(), "alternate_icon".into());
        registry.item_definitions.insert("stone".into(), serde_json::json!({"model": {
            "type": "minecraft:select", "property": "minecraft:custom_model_data",
            "cases": [{"when": "alt", "model": {"type": "minecraft:model", "model": "minecraft:item/alternate"}}],
            "fallback": {"type": "minecraft:model", "model": "minecraft:item/default"}
        }}));
        registry.model_particle_icons.insert(
            "item/alternate".into(),
            vec![Some("alternate_branch".into())],
        );
        registry
            .model_particle_icons
            .insert("item/default".into(), vec![Some("default_branch".into())]);
        let stack = ItemStack::new(ItemKind::Stone, 4);
        let mut custom = NbtCompound::new();
        custom.insert("strings", NbtList::String(vec!["alt".into()]));
        let mut raw = NbtCompound::new();
        raw.insert("minecraft:custom_model_data", custom);
        assert_eq!(
            registry.get_item_particle_icon_with_raw(&stack, Some(&raw)),
            Some("alternate_branch")
        );
        let raw_empty = NbtCompound::new();
        assert_eq!(
            registry.get_item_particle_icon_with_raw(&stack, Some(&raw_empty)),
            Some("default_branch")
        );
        registry.item_definitions.insert("stone".into(), serde_json::json!({"model": {
            "type": "minecraft:range_dispatch", "property": "minecraft:custom_model_data",
            "entries": [{"threshold": 2.0, "model": {"type": "minecraft:model", "model": "minecraft:item/alternate"}}],
            "fallback": {"type": "minecraft:model", "model": "minecraft:item/default"}
        }}));
        let mut numbers = NbtCompound::new();
        numbers.insert("floats", NbtList::Float(vec![2.0]));
        let mut raw_range = NbtCompound::new();
        raw_range.insert("minecraft:custom_model_data", numbers);
        assert_eq!(
            registry.get_item_particle_icon_with_raw(&stack, Some(&raw_range)),
            Some("alternate_branch")
        );
        registry.item_definitions.insert("stone".into(), serde_json::json!({"model": {
            "type": "minecraft:condition", "property": "minecraft:custom_model_data", "index": 0,
            "on_true": {"type": "minecraft:model", "model": "minecraft:item/alternate"},
            "on_false": {"type": "minecraft:model", "model": "minecraft:item/default"}
        }}));
        let mut flags = NbtCompound::new();
        flags.insert("flags", NbtList::Byte(vec![1]));
        let mut raw_condition = NbtCompound::new();
        raw_condition.insert("minecraft:custom_model_data", flags);
        assert_eq!(
            registry.get_item_particle_icon_with_raw(&stack, Some(&raw_condition)),
            Some("alternate_branch")
        );
        let mut override_model = NbtCompound::new();
        override_model.insert("minecraft:item_model", "minecraft:alternate");
        assert_eq!(
            registry.get_item_particle_icon_with_raw(&stack, Some(&override_model)),
            Some("alternate_icon")
        );
        let mut removed_model = NbtCompound::new();
        removed_model.insert("!minecraft:item_model", 1i8);
        assert_eq!(
            registry.get_item_particle_icon_with_raw(&stack, Some(&removed_model)),
            Some("default_branch")
        );
    }

    #[test]
    fn every_native_item_id_matches_azalea_kind_resource_name() {
        use azalea_registry::Registry;
        let registry = pomme_protocol::registries::RegistryTable::native();
        for (id, expected) in registry
            .names(pomme_protocol::registries::ClientRegistry::Item)
            .iter()
            .enumerate()
        {
            let kind = azalea_registry::builtin::ItemKind::from_u32(id as u32)
                .unwrap_or_else(|| panic!("native item id {id} ({expected}) absent from Azalea"));
            assert_eq!(
                crate::player::inventory::item_resource_name(kind),
                *expected,
                "item id {id}"
            );
        }
    }

    #[test]
    fn vanilla_item_property_nodes_resolve_legal_ground_context_paths() {
        use simdnbt::owned::{NbtCompound, NbtList};
        let mut registry = BlockRegistry::test_empty();
        let leaf = |id: &str| serde_json::json!({"type":"minecraft:model","model":format!("minecraft:item/{id}")});
        for name in ["ground", "fallback", "branch", "first", "second"] {
            registry
                .model_particle_icons
                .insert(format!("item/{name}"), vec![Some(format!("{name}_icon"))]);
        }
        let stack = ItemStack::new(ItemKind::Stone, 4);
        let mut raw = NbtCompound::new();
        let mut cases = vec![
            (
                "minecraft:display_context",
                serde_json::json!({"type":"minecraft:select","property":"minecraft:display_context","cases":[{"when":"ground","model":leaf("ground")}],"fallback":leaf("fallback")}),
                "ground_icon",
            ),
            (
                "minecraft:context_dimension",
                serde_json::json!({"type":"minecraft:select","property":"minecraft:context_dimension","cases":[{"when":"minecraft:overworld","model":leaf("branch")}],"fallback":leaf("fallback")}),
                "branch_icon",
            ),
            (
                "minecraft:context_entity_type",
                serde_json::json!({"type":"minecraft:select","property":"minecraft:context_entity_type","cases":[{"when":"minecraft:player","model":leaf("branch")}],"fallback":leaf("fallback")}),
                "fallback_icon",
            ),
            (
                "minecraft:main_hand",
                serde_json::json!({"type":"minecraft:select","property":"minecraft:main_hand","cases":[{"when":"right","model":leaf("branch")}],"fallback":leaf("fallback")}),
                "fallback_icon",
            ),
            (
                "minecraft:local_time",
                serde_json::json!({"type":"minecraft:select","property":"minecraft:local_time","pattern":"yyyy-MM-dd","cases":[{"when":java_local_time_pattern("yyyy-MM-dd", chrono::Local::now()),"model":leaf("branch")}],"fallback":leaf("fallback")}),
                "branch_icon",
            ),
            (
                "minecraft:trim_material",
                serde_json::json!({"type":"minecraft:select","property":"minecraft:trim_material","cases":[{"when":"minecraft:quartz","model":leaf("branch")}],"fallback":leaf("fallback")}),
                "branch_icon",
            ),
            (
                "minecraft:charge_type",
                serde_json::json!({"type":"minecraft:select","property":"minecraft:charge_type","cases":[{"when":"rocket","model":leaf("branch")}],"fallback":leaf("fallback")}),
                "branch_icon",
            ),
            (
                "minecraft:component",
                serde_json::json!({"type":"minecraft:select","property":"minecraft:component","component":"minecraft:custom_name","cases":[{"when":"server label","model":leaf("branch")}],"fallback":leaf("fallback")}),
                "branch_icon",
            ),
            (
                "minecraft:block_state",
                serde_json::json!({"type":"minecraft:select","property":"minecraft:block_state","block_state_property":"honey_level","cases":[{"when":"5","model":leaf("branch")}],"fallback":leaf("fallback")}),
                "branch_icon",
            ),
            (
                "minecraft:using_item",
                serde_json::json!({"type":"minecraft:condition","property":"minecraft:using_item","on_true":leaf("branch"),"on_false":leaf("fallback")}),
                "fallback_icon",
            ),
            (
                "minecraft:fishing_rod/cast",
                serde_json::json!({"type":"minecraft:condition","property":"minecraft:fishing_rod/cast","on_true":leaf("branch"),"on_false":leaf("fallback")}),
                "fallback_icon",
            ),
            (
                "minecraft:broken",
                serde_json::json!({"type":"minecraft:condition","property":"minecraft:broken","on_true":leaf("branch"),"on_false":leaf("fallback")}),
                "fallback_icon",
            ),
            (
                "minecraft:has_component",
                serde_json::json!({"type":"minecraft:condition","property":"minecraft:has_component","component":"minecraft:dyed_color","on_true":leaf("branch"),"on_false":leaf("fallback")}),
                "fallback_icon",
            ),
            (
                "minecraft:bundle/has_selected_item",
                serde_json::json!({"type":"minecraft:condition","property":"minecraft:bundle/has_selected_item","on_true":leaf("branch"),"on_false":leaf("fallback")}),
                "fallback_icon",
            ),
            (
                "minecraft:time",
                serde_json::json!({"type":"minecraft:range_dispatch","property":"minecraft:time","entries":[{"threshold":0.0,"model":leaf("first")}],"fallback":leaf("fallback")}),
                "first_icon",
            ),
            (
                "minecraft:compass",
                serde_json::json!({"type":"minecraft:range_dispatch","property":"minecraft:compass","entries":[{"threshold":0.0,"model":leaf("first")}],"fallback":leaf("fallback")}),
                "first_icon",
            ),
            (
                "minecraft:crossbow/pull",
                serde_json::json!({"type":"minecraft:range_dispatch","property":"minecraft:crossbow/pull","entries":[{"threshold":0.0,"model":leaf("first")}],"fallback":leaf("fallback")}),
                "first_icon",
            ),
            (
                "minecraft:use_duration",
                serde_json::json!({"type":"minecraft:range_dispatch","property":"minecraft:use_duration","entries":[{"threshold":0.0,"model":leaf("first")}],"fallback":leaf("fallback")}),
                "first_icon",
            ),
            (
                "minecraft:use_cycle",
                serde_json::json!({"type":"minecraft:range_dispatch","property":"minecraft:use_cycle","entries":[{"threshold":0.0,"model":leaf("first")}],"fallback":leaf("fallback")}),
                "first_icon",
            ),
            (
                "minecraft:damage",
                serde_json::json!({"type":"minecraft:range_dispatch","property":"minecraft:damage","entries":[{"threshold":0.0,"model":leaf("first")}],"fallback":leaf("fallback")}),
                "fallback_icon",
            ),
            (
                "minecraft:count",
                serde_json::json!({"type":"minecraft:range_dispatch","property":"minecraft:count","entries":[{"threshold":0.05,"model":leaf("first")}],"fallback":leaf("fallback")}),
                "first_icon",
            ),
        ];
        let mut block_state = NbtCompound::new();
        block_state.insert("honey_level", "5");
        raw.insert("minecraft:block_state", block_state);
        let mut trim = NbtCompound::new();
        trim.insert("material", "minecraft:quartz");
        raw.insert("minecraft:trim", trim);
        let mut projectile = NbtCompound::new();
        projectile.insert("id", "minecraft:firework_rocket");
        raw.insert(
            "minecraft:charged_projectiles",
            NbtList::Compound(vec![projectile]),
        );
        raw.insert("minecraft:custom_name", "server label");
        for (property, definition, expected) in cases.drain(..) {
            registry
                .item_definitions
                .insert("stone".into(), serde_json::json!({"model":definition}));
            let context = ItemParticleModelContext {
                dimension: Some("minecraft:overworld"),
                registries: None,
            };
            let icons = registry
                .get_item_particle_materials(&stack, Some(&raw), context)
                .unwrap_or_else(|| panic!("{property} failed to produce a GROUND material"));
            assert_eq!(
                icons.first().copied().flatten(),
                Some(expected),
                "{property}"
            );
        }
        let mut composite = serde_json::json!({"type":"minecraft:composite","models":[leaf("first"),{"type":"minecraft:special","base":"minecraft:item/template_chest","model":{"type":"minecraft:chest"}},{"type":"minecraft:bundle/selected_item"},leaf("second")]});
        registry.item_definitions.insert(
            "stone".into(),
            serde_json::json!({"model":composite.take()}),
        );
        let materials = registry
            .get_item_particle_materials(&stack, None, ItemParticleModelContext::default())
            .unwrap();
        assert_eq!(materials, [Some("first_icon"), None, Some("second_icon")]);
    }

    #[test]
    fn vanilla_bow_clock_compass_and_crossbow_resolve_material_with_particle_context() {
        let mut registry = BlockRegistry::test_empty();
        for (model, icon) in [
            ("item/bow", "bow_icon"),
            ("item/clock_00", "clock_icon"),
            ("item/compass_16", "compass_icon"),
            ("item/crossbow", "crossbow_icon"),
        ] {
            registry
                .model_particle_icons
                .insert(model.into(), vec![Some(icon.into())]);
        }
        let bow = serde_json::json!({"model": {"type":"minecraft:condition", "property":"minecraft:using_item",
            "on_true":{"type":"minecraft:model","model":"minecraft:item/bow_pulling_0"},
            "on_false":{"type":"minecraft:model","model":"minecraft:item/bow"}}});
        let clock = serde_json::json!({"model": {"type":"minecraft:select", "property":"minecraft:context_dimension",
            "cases":[{"when":"minecraft:overworld","model":{"type":"minecraft:range_dispatch","property":"minecraft:time","scale":64.0,
                "entries":[{"threshold":0.0,"model":{"type":"minecraft:model","model":"minecraft:item/clock_00"}}],
                "fallback":{"type":"minecraft:model","model":"minecraft:item/clock_01"}}}],
            "fallback":{"type":"minecraft:model","model":"minecraft:item/clock_01"}}});
        let compass = serde_json::json!({"model": {"type":"minecraft:condition", "property":"minecraft:has_component", "component":"minecraft:lodestone_tracker",
            "on_true":{"type":"minecraft:model","model":"minecraft:item/compass_16"},
            "on_false":{"type":"minecraft:range_dispatch","property":"minecraft:compass","scale":32.0,
                "entries":[{"threshold":0.0,"model":{"type":"minecraft:model","model":"minecraft:item/compass_16"}}],
                "fallback":{"type":"minecraft:model","model":"minecraft:item/compass_17"}}}});
        let crossbow = serde_json::json!({"model": {"type":"minecraft:select", "property":"minecraft:charge_type",
            "cases":[{"when":"rocket","model":{"type":"minecraft:model","model":"minecraft:item/crossbow_firework"}},
                     {"when":"arrow","model":{"type":"minecraft:model","model":"minecraft:item/crossbow_arrow"}}],
            "fallback":{"type":"minecraft:condition","property":"minecraft:using_item",
                "on_true":{"type":"minecraft:model","model":"minecraft:item/crossbow_pulling_0"},
                "on_false":{"type":"minecraft:model","model":"minecraft:item/crossbow"}}}});
        for (kind, name, definition, expected) in [
            (ItemKind::Bow, "bow", bow, "bow_icon"),
            (ItemKind::Clock, "clock", clock, "clock_icon"),
            (ItemKind::Compass, "compass", compass, "compass_icon"),
            (ItemKind::Crossbow, "crossbow", crossbow, "crossbow_icon"),
        ] {
            registry.item_definitions.insert(name.into(), definition);
            let stack = ItemStack::new(kind, 1);
            let materials = registry
                .get_item_particle_materials(
                    &stack,
                    None,
                    ItemParticleModelContext {
                        dimension: Some("minecraft:overworld"),
                        registries: None,
                    },
                )
                .unwrap_or_else(|| panic!("{name} item particle had no material"));
            assert_eq!(
                materials.first().copied().flatten(),
                Some(expected),
                "{name}"
            );
        }
    }

    #[test]
    fn item_model_component_selects_mapped_icon_without_guessing_missing_overrides() {
        let registry = BlockRegistry {
            textures: HashMap::new(),
            baked: HashMap::new(),
            multipart: HashMap::new(),
            item_models: HashMap::new(),
            flat_item_textures: Default::default(),
            flat_item_texture_keys: HashMap::new(),
            flat_item_tints: HashMap::new(),
            item_ground_transforms: HashMap::new(),
            item_fixed_transforms: HashMap::new(),
            item_definitions: HashMap::new(),
            model_particle_icons: HashMap::new(),
            item_particle_icons: HashMap::from([
                ("stone".into(), "base_particle".into()),
                ("alternate".into(), "override_particle".into()),
                ("sub/nested".into(), "nested_particle".into()),
                ("other:sub/custom".into(), "custom_particle".into()),
            ]),
        };
        let base = ItemStack::new(ItemKind::Stone, 1);
        assert_eq!(BlockRegistry::item_model_name(&ItemStack::default()), None);
        assert_eq!(
            BlockRegistry::item_model_name(&base).as_deref(),
            Some("stone")
        );
        assert_eq!(
            registry.get_item_particle_icon(&base),
            Some("base_particle")
        );
        let mapped = base.clone().with_component(ItemModel {
            resource_location: Identifier::new("minecraft:alternate"),
        });
        assert_eq!(
            BlockRegistry::item_model_name(&mapped).as_deref(),
            Some("alternate")
        );
        assert_eq!(
            registry.get_item_particle_icon(&mapped),
            Some("override_particle")
        );
        let nested = base.clone().with_component(ItemModel {
            resource_location: Identifier::new("minecraft:sub/nested"),
        });
        assert_eq!(
            BlockRegistry::item_model_name(&nested).as_deref(),
            Some("sub/nested")
        );
        assert_eq!(
            registry.get_item_particle_icon(&nested),
            Some("nested_particle")
        );
        let custom = base.clone().with_component(ItemModel {
            resource_location: Identifier::new("other:sub/custom"),
        });
        assert_eq!(
            BlockRegistry::item_model_name(&custom).as_deref(),
            Some("other:sub/custom")
        );
        assert_eq!(
            registry.get_item_particle_icon(&custom),
            Some("custom_particle")
        );
        let missing = base.with_component(ItemModel {
            resource_location: Identifier::new("minecraft:unknown"),
        });
        assert_eq!(
            BlockRegistry::item_model_name(&missing).as_deref(),
            Some("unknown")
        );
        assert_eq!(registry.get_item_particle_icon(&missing), None);
    }
}

fn save_cache(path: &Path, textures: &HashMap<String, FaceTextures>) {
    if let Ok(json) = serde_json::to_string(textures)
        && let Err(e) = std::fs::write(path, json)
    {
        tracing::warn!("Failed to write block cache: {e}");
    }
}
