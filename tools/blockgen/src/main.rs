//! Generates pomme's per-version block data from Mojang's data-generator
//! reports (`java -DbundlerMainClass=net.minecraft.data.Main -jar server.jar
//! --reports`).
//!
//! Usage:
//!   blockgen blocks <reports/blocks.json> <version> <out.json>
//!   blockgen behavior <azalea generated.rs> <out.json>
//!   blockgen state <generated/state.json> <blocks-<v>.json> <out.json>
//!   blockgen shapes <blocks-<v>.json> <oracle.json> <verified-comparison.json>
//! <out.json>
//!
//! `blocks` flattens the report into a compact per-block table (name, first
//! state id, default state id, ordered property lists). Every explicit state
//! id + property set in the report is cross-checked against the cartesian
//! reconstruction the client uses, and the id space is verified dense — the
//! tool hard-fails rather than emit silently-wrong data.
//!
//! `behavior` seeds the name-keyed destroy-time table from an azalea
//! `generated.rs` (e.g. `~/.cargo/git/checkouts/azalea-*/<rev>/azalea-block/
//! src/generated.rs`); new blocks the seed doesn't know must be appended by
//! hand from the decompiled `Blocks.java`. Hand-added entries survive a
//! regen (existing keys the seed doesn't produce are carried over and
//! listed), but the seed wins for keys it does produce — a hand-correction
//! to a seeded value does not survive.
//!
//! `state` compacts the raw per-state property dump produced by running
//! vanilla (`tools/stategen/StateDump.java`, see `just stategen`) into the
//! per-block table the client embeds: each field is a scalar when uniform
//! across the block's states, else a per-state array, and face-occlusion
//! masks are deduped into a dictionary. State counts and value ranges are
//! cross-checked against the version's blocks table.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.as_slice() {
        [cmd, report, version, out] if cmd == "blocks" => gen_blocks(report, version, out),
        [cmd, generated, out] if cmd == "behavior" => gen_behavior(generated, out),
        [cmd, dump, blocks, out] if cmd == "state" => gen_state(dump, blocks, out),
        [cmd, blocks, oracle, comparison, out] if cmd == "shapes" => {
            gen_shapes(blocks, oracle, comparison, out)
        }
        _ => Err("usage: blockgen blocks <blocks.json> <version> <out.json>\n       blockgen behavior <generated.rs> <out.json>\n       blockgen state <state.json> <blocks-<v>.json> <out.json>\n       blockgen shapes <blocks-<v>.json> <vanilla-shape-oracle.json> <verified-comparison.json> <out.json>".into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("blockgen: {e}");
            ExitCode::FAILURE
        }
    }
}

type Error = Box<dyn std::error::Error>;

struct Block {
    name: String,
    first_id: u32,
    default_id: u32,
    /// Property (key, values) pairs in the report's listed order; the last
    /// property varies fastest in the state-id cartesian product.
    props: Vec<(String, Vec<String>)>,
}

fn gen_blocks(report_path: &str, version: &str, out_path: &str) -> Result<(), Error> {
    let report: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string(report_path)?)?;

    let mut blocks = Vec::new();
    for (name, entry) in &report {
        blocks.push(parse_block(name, entry)?);
    }
    blocks.sort_by_key(|b| b.first_id);

    // The id space must tile densely from 0 with no gaps or overlaps.
    let mut expected_id = 0u32;
    for block in &blocks {
        if block.first_id != expected_id {
            return Err(format!(
                "id space not dense: block '{}' starts at {} but expected {}",
                block.name, block.first_id, expected_id
            )
            .into());
        }
        expected_id += state_count(block);
    }

    let mut out = String::new();
    writeln!(out, "{{")?;
    writeln!(out, "  \"version\": {},", serde_json::to_string(version)?)?;
    writeln!(out, "  \"state_count\": {expected_id},")?;
    writeln!(out, "  \"blocks\": [")?;
    for (i, block) in blocks.iter().enumerate() {
        let comma = if i + 1 < blocks.len() { "," } else { "" };
        let mut line = format!(
            "    {{\"name\": {}, \"first_id\": {}, \"default_id\": {}",
            serde_json::to_string(&block.name)?,
            block.first_id,
            block.default_id
        );
        if !block.props.is_empty() {
            let props: Vec<serde_json::Value> = block
                .props
                .iter()
                .map(|(k, vs)| serde_json::json!([k, vs]))
                .collect();
            write!(line, ", \"props\": {}", serde_json::to_string(&props)?)?;
        }
        writeln!(out, "{line}}}{comma}")?;
    }
    writeln!(out, "  ]")?;
    writeln!(out, "}}")?;

    std::fs::write(out_path, &out)?;
    println!(
        "wrote {} blocks / {} states for {} to {}",
        blocks.len(),
        expected_id,
        version,
        out_path
    );
    Ok(())
}

fn state_count(block: &Block) -> u32 {
    block.props.iter().map(|(_, vs)| vs.len() as u32).product()
}

fn parse_block(name: &str, entry: &serde_json::Value) -> Result<Block, Error> {
    let name = name.strip_prefix("minecraft:").unwrap_or(name).to_string();

    // Value arrays are in variant order, but the report's property KEY order
    // is the builder order, not the state-enumeration order (vanilla sorts
    // properties by name for the state definition). Rather than assume the
    // sort rule, each property's stride is derived from the explicit state
    // ids below and the properties reordered to match.
    let mut props: Vec<(String, Vec<String>)> = Vec::new();
    if let Some(properties) = entry.get("properties") {
        let map = properties
            .as_object()
            .ok_or_else(|| format!("{name}: properties is not an object"))?;
        for (key, values) in map {
            let values = values
                .as_array()
                .ok_or_else(|| format!("{name}: property {key} values not an array"))?
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(String::from)
                        .ok_or_else(|| format!("{name}: property {key} has non-string value"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            props.push((key.clone(), values));
        }
    }

    let states = entry
        .get("states")
        .and_then(|s| s.as_array())
        .ok_or_else(|| format!("{name}: missing states array"))?;

    let expected: u32 = props.iter().map(|(_, vs)| vs.len() as u32).product();
    if states.len() as u32 != expected {
        return Err(format!(
            "{name}: {} states but property product is {expected}",
            states.len()
        )
        .into());
    }

    let first_id = states
        .iter()
        .filter_map(|s| s.get("id").and_then(|i| i.as_u64()))
        .min()
        .ok_or_else(|| format!("{name}: states missing ids"))? as u32;

    // Index the explicit states by their full property assignment.
    let mut by_props: std::collections::HashMap<BTreeMap<&str, &str>, u32> =
        std::collections::HashMap::new();
    for state in states {
        let id = state
            .get("id")
            .and_then(|i| i.as_u64())
            .ok_or_else(|| format!("{name}: state missing id"))? as u32;
        let mut key = BTreeMap::new();
        if let Some(map) = state.get("properties").and_then(|p| p.as_object()) {
            for (k, v) in map {
                key.insert(
                    k.as_str(),
                    v.as_str()
                        .ok_or_else(|| format!("{name}: non-string property value"))?,
                );
            }
        }
        by_props.insert(key, id);
    }

    // The base state (first id) must sit at every property's first value,
    // and flipping one property to its second value reveals that property's
    // stride in the enumeration.
    let base: BTreeMap<&str, &str> = props
        .iter()
        .map(|(k, vs)| (k.as_str(), vs[0].as_str()))
        .collect();
    if by_props.get(&base) != Some(&first_id) {
        return Err(format!(
            "{name}: base state (all first values) is not the first id — enumeration isn't a plain cartesian product"
        )
        .into());
    }
    let mut strides: Vec<u32> = Vec::with_capacity(props.len());
    for (key, values) in &props {
        if values.len() == 1 {
            strides.push(0);
            continue;
        }
        let mut flipped = base.clone();
        flipped.insert(key.as_str(), values[1].as_str());
        let id = by_props
            .get(&flipped)
            .ok_or_else(|| format!("{name}: no state for {key}={}", values[1]))?;
        strides.push(id - first_id);
    }

    // Reorder to enumeration order: largest stride first (single-value
    // properties contribute factor 1 and can go last).
    let mut order: Vec<usize> = (0..props.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(strides[i]));
    let props: Vec<(String, Vec<String>)> = order.into_iter().map(|i| props[i].clone()).collect();

    let mut default_id = None;

    // Cross-check every explicit state against the cartesian reconstruction
    // (derived property order, last property varying fastest).
    for state in states {
        let id = state
            .get("id")
            .and_then(|i| i.as_u64())
            .ok_or_else(|| format!("{name}: state missing id"))? as u32;
        let offset = id
            .checked_sub(first_id)
            .ok_or_else(|| format!("{name}: state id {id} below first id {first_id}"))?;

        let mut stride: u32 = props.iter().map(|(_, vs)| vs.len() as u32).product();
        for (key, values) in &props {
            stride /= values.len() as u32;
            let index = (offset / stride) as usize % values.len();
            let reconstructed = &values[index];
            let reported = state
                .get("properties")
                .and_then(|p| p.get(key))
                .and_then(|v| v.as_str())
                .ok_or_else(|| format!("{name}: state {id} missing property {key}"))?;
            if reconstructed != reported {
                return Err(format!(
                    "{name}: state {id} property {key} reconstructs to '{reconstructed}' but report says '{reported}' — enumeration order changed, extend the format"
                )
                .into());
            }
        }

        if state.get("default").and_then(|d| d.as_bool()) == Some(true) {
            default_id = Some(id);
        }
    }

    let default_id = default_id.ok_or_else(|| format!("{name}: no default state"))?;
    Ok(Block {
        name,
        first_id,
        default_id,
        props,
    })
}

/// Extracts `destroy_time` + `requires_correct_tool_for_drops` per block from
/// azalea's machine-generated `generated.rs` block list (uniform shape:
/// `name => BlockBehavior::new().strength(a, b)..., {`).
fn gen_behavior(generated_path: &str, out_path: &str) -> Result<(), Error> {
    let source = std::fs::read_to_string(generated_path)?;
    let mut entries: BTreeMap<String, (f32, bool)> = BTreeMap::new();

    for line in source.lines() {
        let trimmed = line.trim_start();
        let Some((name, rest)) = trimmed.split_once(" => BlockBehavior::new()") else {
            continue;
        };
        if !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            continue;
        }
        let destroy_time = extract_float_arg(rest, ".strength(")
            .or_else(|| extract_float_arg(rest, ".destroy_time("))
            .unwrap_or(0.0);
        let requires_tool = rest.contains(".requires_correct_tool_for_drops()");
        entries.insert(name.to_string(), (destroy_time, requires_tool));
    }

    if entries.is_empty() {
        return Err("no block behavior entries found — wrong input file?".into());
    }

    // Carry over hand-appended keys the seed doesn't know.
    let mut carried: Vec<String> = Vec::new();
    match std::fs::read_to_string(out_path) {
        Ok(existing) => {
            let existing: BTreeMap<String, BehaviorJson> =
                serde_json::from_str(&existing).map_err(|e| format!("{out_path}: {e}"))?;
            for (name, b) in existing {
                if !entries.contains_key(&name) {
                    entries.insert(name.clone(), (b.destroy_time, b.requires_correct_tool));
                    carried.push(name);
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("{out_path}: {e}").into()),
    }

    let mut out = String::new();
    writeln!(out, "{{")?;
    let len = entries.len();
    for (i, (name, (destroy_time, requires_tool))) in entries.iter().enumerate() {
        let comma = if i + 1 < len { "," } else { "" };
        writeln!(
            out,
            "  {}: {{\"destroy_time\": {destroy_time}, \"requires_correct_tool\": {requires_tool}}}{comma}",
            serde_json::to_string(name)?
        )?;
    }
    writeln!(out, "}}")?;

    std::fs::write(out_path, &out)?;
    println!("wrote {len} behavior entries to {out_path}");
    if !carried.is_empty() {
        println!(
            "carried over {} hand-added entries: {}",
            carried.len(),
            carried.join(", ")
        );
    }
    Ok(())
}

/// An entry of the emitted `block_behavior.json`, read back on regen.
#[derive(serde::Deserialize)]
struct BehaviorJson {
    destroy_time: f32,
    requires_correct_tool: bool,
}

fn extract_float_arg(text: &str, method: &str) -> Option<f32> {
    let start = text.find(method)? + method.len();
    let rest = &text[start..];
    let end = rest.find([',', ')'])?;
    rest[..end].trim().parse().ok()
}

/// Raw per-state dump written by `tools/stategen/StateDump.java`.
#[derive(serde::Deserialize)]
struct StateDumpFile {
    version: String,
    state_count: u32,
    emission: Vec<u8>,
    dampening: Vec<u8>,
    propagates_skylight_down: Vec<u8>,
    can_occlude: Vec<u8>,
    use_shape_for_light_occlusion: Vec<u8>,
    /// `BlockBehaviour.hasCollision`; block-level, so uniform across each
    /// block's states.
    has_collision: Vec<u8>,
    /// State id (as string) -> 6 face masks, 64 hex chars each, present
    /// exactly for states with `can_occlude && use_shape_for_light_occlusion`.
    face_masks: std::collections::HashMap<String, [String; 6]>,
}

#[derive(serde::Deserialize)]
struct BlocksFile {
    version: String,
    state_count: u32,
    blocks: Vec<BlocksEntry>,
}

#[derive(serde::Deserialize)]
struct BlocksEntry {
    name: String,
    first_id: u32,
    #[serde(default)]
    props: Vec<(String, Vec<String>)>,
}

#[derive(serde::Deserialize)]
struct ShapeComparison {
    block: String,
    props: BTreeMap<String, String>,
    collision_equal: bool,
    outline_equal: bool,
    official_collision: Vec<[f64; 6]>,
    official_outline: Vec<[f64; 6]>,
    status: String,
}

#[derive(serde::Serialize)]
struct GeneratedShapes {
    version: String,
    state_count: u32,
    verified_states: u32,
    skipped_states: u32,
    context_dependent_states: u32,
    offset_dependent_states: u32,
    oracle_disagreement_states: u32,
    oracle: &'static str,
    shapes: Vec<Vec<[f64; 6]>>,
    blocks: Vec<GeneratedShapeBlock>,
}

#[derive(serde::Serialize)]
struct GeneratedShapeBlock {
    name: String,
    first_id: u32,
    props: Vec<(String, Vec<String>)>,
    /// `(state offset, collision shape index, outline shape index)`.
    overrides: Vec<(u32, u32, u32)>,
}

/// Converts Steel's existing shape oracle into state-local overrides only
/// where both the report layout and the report's independent vanilla shape
/// comparison prove the oracle data matches this exact version/state.
fn gen_shapes(
    blocks_path: &str,
    oracle_path: &str,
    comparison_path: &str,
    out_path: &str,
) -> Result<(), Error> {
    let blocks: BlocksFile = serde_json::from_str(&std::fs::read_to_string(blocks_path)?)?;
    if blocks.version != "26.2" {
        return Err(format!(
            "shape oracle is verified only for 26.2, not '{}'",
            blocks.version
        )
        .into());
    }
    let oracle: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(oracle_path)?)?;
    let oracle_blocks = oracle["blocks"]
        .as_array()
        .ok_or("oracle has no blocks array")?;
    let oracle_shapes = oracle["shapes"]
        .as_array()
        .ok_or("oracle has no shapes array")?;
    let comparisons: Vec<ShapeComparison> =
        serde_json::from_str(&std::fs::read_to_string(comparison_path)?)?;
    if comparisons.len() != blocks.state_count as usize {
        return Err(format!(
            "comparison has {} states, blocks table has {}",
            comparisons.len(),
            blocks.state_count
        )
        .into());
    }
    if oracle_blocks.len() != blocks.blocks.len() {
        return Err(format!(
            "oracle has {} blocks, 26.2 table has {}",
            oracle_blocks.len(),
            blocks.blocks.len()
        )
        .into());
    }

    let mut generated = Vec::with_capacity(blocks.blocks.len());
    let mut shape_dictionary = Vec::new();
    let mut shape_indices = HashMap::new();
    let mut state_id = 0u32;
    let mut verified_states = 0u32;
    let mut skipped_states = 0u32;
    let mut context_dependent_states = 0u32;
    let mut offset_dependent_states = 0u32;
    let mut oracle_disagreement_states = 0u32;
    let mut overrides = 0u32;

    for (block_index, block) in blocks.blocks.iter().enumerate() {
        if block.first_id != state_id {
            return Err(format!(
                "blocks table not dense at '{}': starts at {}, expected {state_id}",
                block.name, block.first_id
            )
            .into());
        }
        let oracle_block = &oracle_blocks[block_index];
        if oracle_block["id"].as_u64() != Some(block_index as u64)
            || oracle_block["name"].as_str() != Some(block.name.as_str())
        {
            return Err(format!(
                "oracle block identity mismatch at {}: expected {}",
                block_index, block.name
            )
            .into());
        }

        let oracle_properties = oracle_block["properties"]
            .as_array()
            .ok_or_else(|| format!("{}: oracle properties missing", block.name))?;
        let oracle_defaults = oracle_block["default_properties"]
            .as_array()
            .ok_or_else(|| format!("{}: oracle default_properties missing", block.name))?;
        if oracle_properties.len() != block.props.len()
            || oracle_defaults.len() != block.props.len()
        {
            return Err(format!(
                "{}: oracle has {} properties/defaults, version table has {}",
                block.name,
                oracle_properties.len(),
                block.props.len()
            )
            .into());
        }
        let mut seen = HashMap::new();
        for (axis, ((key, values), oracle_property)) in
            block.props.iter().zip(oracle_properties).enumerate()
        {
            let raw = oracle_property
                .as_str()
                .ok_or_else(|| format!("{}: invalid oracle property", block.name))?;
            let normalized = shape_property_name(raw, axis, &mut seen);
            if normalized != *key {
                return Err(format!(
                    "{}: oracle property {raw} maps to '{normalized}', version table says '{key}'",
                    block.name
                )
                .into());
            }
            let default = oracle_defaults[axis]
                .as_str()
                .ok_or_else(|| format!("{}: invalid oracle default property", block.name))?;
            let default_value = shape_property_value(default).ok_or_else(|| {
                format!(
                    "{}: cannot decode oracle default property '{default}'",
                    block.name
                )
            })?;
            if !values.iter().any(|value| value == &default_value) {
                return Err(format!(
                    "{}: oracle default {key}={default_value} is not in version table",
                    block.name
                )
                .into());
            }
        }

        let count: u32 = block
            .props
            .iter()
            .map(|(_, values)| values.len() as u32)
            .product();
        if count == 0 {
            return Err(format!("{}: zero block states", block.name).into());
        }
        let mut block_overrides = Vec::new();
        for offset in 0..count {
            let comparison = &comparisons[(state_id + offset) as usize];
            let properties = state_properties(&block.props, offset);
            if comparison.block != block.name || comparison.props != properties {
                return Err(format!(
                    "state {} identity mismatch: expected {} {:?}, comparison has {} {:?}",
                    state_id + offset,
                    block.name,
                    properties,
                    comparison.block,
                    comparison.props
                )
                .into());
            }
            if comparison.status == "context_dependent" {
                skipped_states += 1;
                context_dependent_states += 1;
                continue;
            }
            if comparison.status != "compared" {
                return Err(format!(
                    "state {} has unknown comparison status '{}'",
                    state_id + offset,
                    comparison.status
                )
                .into());
            }

            let collision = oracle_shape(oracle_block, oracle_shapes, "collision_shapes", offset)?;
            let outline = oracle_shape(oracle_block, oracle_shapes, "outline_shapes", offset)?;
            let has_offset = oracle_block["collision_shapes"]["usesOffset"] == true
                || oracle_block["outline_shapes"]["usesOffset"] == true;
            if has_offset {
                // Randomized boxes need the block position at their call site.
                skipped_states += 1;
                offset_dependent_states += 1;
                continue;
            }
            if !same_boxes(&collision, &comparison.official_collision)
                || !same_boxes(&outline, &comparison.official_outline)
            {
                // The oracle has a small number of cross-version changes.
                skipped_states += 1;
                oracle_disagreement_states += 1;
                continue;
            }
            verified_states += 1;
            if !comparison.collision_equal || !comparison.outline_equal {
                block_overrides.push((
                    offset,
                    intern_shape(collision, &mut shape_dictionary, &mut shape_indices),
                    intern_shape(outline, &mut shape_dictionary, &mut shape_indices),
                ));
                overrides += 1;
            }
        }
        state_id += count;
        generated.push(GeneratedShapeBlock {
            name: block.name.clone(),
            first_id: block.first_id,
            props: block.props.clone(),
            overrides: block_overrides,
        });
    }
    if state_id != blocks.state_count {
        return Err(format!(
            "blocks cover {state_id} states, table declares {}",
            blocks.state_count
        )
        .into());
    }
    if verified_states + skipped_states != state_id {
        return Err(format!(
            "shape coverage is {} verified + {} skipped, expected {state_id}",
            verified_states, skipped_states
        )
        .into());
    }

    let output = GeneratedShapes {
        version: blocks.version,
        state_count: state_id,
        verified_states,
        skipped_states,
        context_dependent_states,
        offset_dependent_states,
        oracle_disagreement_states,
        oracle: "third_party/SteelMC/steel-registry/build_assets/blocks.json",
        shapes: shape_dictionary,
        blocks: generated,
    };
    std::fs::write(out_path, format!("{}\n", serde_json::to_string(&output)?))?;
    println!(
        "wrote shape overrides for {} states ({} verified, {} skipped, {} overrides, {} shared shapes) to {out_path}",
        state_id,
        verified_states,
        skipped_states,
        overrides,
        output.shapes.len()
    );
    Ok(())
}

fn intern_shape(
    boxes: Vec<[f64; 6]>,
    dictionary: &mut Vec<Vec<[f64; 6]>>,
    indices: &mut HashMap<Vec<[u64; 6]>, u32>,
) -> u32 {
    let key: Vec<[u64; 6]> = boxes
        .iter()
        .map(|bounds| bounds.map(f64::to_bits))
        .collect();
    if let Some(&index) = indices.get(&key) {
        return index;
    }
    let index = dictionary.len() as u32;
    indices.insert(key, index);
    dictionary.push(boxes);
    index
}

fn shape_property_name(raw: &str, _axis: usize, seen: &mut HashMap<String, usize>) -> String {
    let name = raw.to_ascii_lowercase();
    if name == "has_bottle" {
        let index = seen.entry(name).or_default();
        let result = format!("has_bottle_{index}");
        *index += 1;
        return result;
    }
    if name.starts_with("has_bottle_") {
        return name;
    }
    match name.as_str() {
        "horizontal_facing" | "facing_hopper" => "facing".into(),
        "slab_type" | "chest_type" | "piston_type" => "type".into(),
        "stairs_shape" | "rail_shape" | "rail_shape_straight" => "shape".into(),
        "bed_part" => "part".into(),
        "double_block_half" => "half".into(),
        "side_chain_part" => "side_chain".into(),
        "noteblock_instrument" => "instrument".into(),
        "east_wall" | "east_redstone" => "east".into(),
        "north_wall" | "north_redstone" => "north".into(),
        "south_wall" | "south_redstone" => "south".into(),
        "west_wall" | "west_redstone" => "west".into(),
        "door_hinge" => "hinge".into(),
        "attach_face" => "face".into(),
        "horizontal_axis" => "axis".into(),
        "level_cauldron" | "level_composter" => "level".into(),
        "dried_ghast_hydration_levels" => "hydration".into(),
        "bamboo_leaves" => "leaves".into(),
        "stability_distance" => "distance".into(),
        "bell_attachment" => "attachment".into(),
        "structureblock_mode" | "test_block_mode" | "mode_comparator" => "mode".into(),
        "level_honey" => "honey_level".into(),
        "respawn_anchor_charges" => "charges".into(),
        "speleothem_thickness" => "thickness".into(),
        _ => name
            .rsplit_once('_')
            .filter(|(_, suffix)| suffix.parse::<u32>().is_ok())
            .map_or(name.clone(), |(base, _)| base.to_string()),
    }
}

fn shape_property_value(value: &str) -> Option<String> {
    if let Some(value) = value.strip_prefix("bool_") {
        return Some(value.into());
    }
    if let Some(value) = value.strip_prefix("int_") {
        return Some(value.into());
    }
    if let Some(value) = value.strip_prefix("enum_") {
        return value.split_once('_').map(|(_, value)| value.to_string());
    }
    None
}

fn state_properties(props: &[(String, Vec<String>)], offset: u32) -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();
    let mut stride: u32 = props
        .iter()
        .map(|(_, values)| values.len() as u32)
        .product();
    for (key, choices) in props {
        stride /= choices.len() as u32;
        values.insert(
            key.clone(),
            choices[(offset / stride) as usize % choices.len()].clone(),
        );
    }
    values
}

fn oracle_shape(
    block: &serde_json::Value,
    dictionary: &[serde_json::Value],
    key: &str,
    offset: u32,
) -> Result<Vec<[f64; 6]>, Error> {
    let shape = &block[key];
    if shape["usesOffset"] != false {
        return Ok(Vec::new());
    }
    let indices = shape["overwrites"]
        .as_array()
        .ok_or_else(|| format!("{}: {key} overwrites missing", block["name"]))?
        .iter()
        .find(|entry| entry["offset"].as_u64() == Some(offset as u64))
        .map_or(&shape["default"], |entry| &entry["shapes"]);
    let indices = indices
        .as_array()
        .ok_or_else(|| format!("{}: {key} default missing", block["name"]))?;
    indices
        .iter()
        .map(|index| {
            let id = index
                .as_u64()
                .ok_or_else(|| format!("{}: invalid shape index", block["name"]))?
                as usize;
            let aabb = dictionary
                .get(id)
                .ok_or_else(|| format!("{}: shape index {id} out of range", block["name"]))?;
            let min = aabb["min"]
                .as_array()
                .ok_or_else(|| format!("{}: shape {id} has no min", block["name"]))?;
            let max = aabb["max"]
                .as_array()
                .ok_or_else(|| format!("{}: shape {id} has no max", block["name"]))?;
            if min.len() != 3 || max.len() != 3 {
                return Err(format!("{}: shape {id} is not 3D", block["name"]).into());
            }
            let mut result = [0.0; 6];
            for axis in 0..3 {
                result[axis] = min[axis]
                    .as_f64()
                    .ok_or_else(|| format!("{}: shape {id} invalid min", block["name"]))?;
                result[axis + 3] = max[axis]
                    .as_f64()
                    .ok_or_else(|| format!("{}: shape {id} invalid max", block["name"]))?;
                if result[axis] > result[axis + 3] {
                    return Err(format!("{}: shape {id} has inverted bounds", block["name"]).into());
                }
            }
            Ok(result)
        })
        .collect()
}

fn same_boxes(a: &[[f64; 6]], b: &[[f64; 6]]) -> bool {
    let mut a = a.to_vec();
    let mut b = b.to_vec();
    let order = |x: &[f64; 6], y: &[f64; 6]| {
        for axis in 0..6 {
            let compare = x[axis].total_cmp(&y[axis]);
            if compare != std::cmp::Ordering::Equal {
                return compare;
            }
        }
        std::cmp::Ordering::Equal
    };
    a.sort_by(order);
    b.sort_by(order);
    a == b
}

fn gen_state(dump_path: &str, blocks_path: &str, out_path: &str) -> Result<(), Error> {
    let dump: StateDumpFile = serde_json::from_str(&std::fs::read_to_string(dump_path)?)?;
    let blocks: BlocksFile = serde_json::from_str(&std::fs::read_to_string(blocks_path)?)?;

    if dump.version != blocks.version {
        return Err(format!(
            "version mismatch: state dump is '{}', blocks table is '{}'",
            dump.version, blocks.version
        )
        .into());
    }
    if dump.state_count != blocks.state_count {
        return Err(format!(
            "state count mismatch: state dump has {}, blocks table has {}",
            dump.state_count, blocks.state_count
        )
        .into());
    }
    let n = dump.state_count as usize;
    for (key, len) in [
        ("emission", dump.emission.len()),
        ("dampening", dump.dampening.len()),
        (
            "propagates_skylight_down",
            dump.propagates_skylight_down.len(),
        ),
        ("can_occlude", dump.can_occlude.len()),
        (
            "use_shape_for_light_occlusion",
            dump.use_shape_for_light_occlusion.len(),
        ),
        ("has_collision", dump.has_collision.len()),
    ] {
        if len != n {
            return Err(format!("{key} has {len} entries, expected {n}").into());
        }
    }
    for i in 0..n {
        if dump.emission[i] > 15 || dump.dampening[i] > 15 {
            return Err(format!("state {i}: light value out of 0..=15 range").into());
        }
        for (key, v) in [
            ("propagates_skylight_down", dump.propagates_skylight_down[i]),
            ("can_occlude", dump.can_occlude[i]),
            (
                "use_shape_for_light_occlusion",
                dump.use_shape_for_light_occlusion[i],
            ),
            ("has_collision", dump.has_collision[i]),
        ] {
            if v > 1 {
                return Err(format!("state {i}: {key} is {v}, expected 0/1").into());
            }
        }
    }

    // Dedupe face masks into a dictionary, iterating in ascending state-id
    // order so the output is deterministic.
    let mut masks_by_state: BTreeMap<u32, &[String; 6]> = BTreeMap::new();
    for (key, masks) in &dump.face_masks {
        let id: u32 = key
            .parse()
            .map_err(|_| format!("face_masks key '{key}' is not a state id"))?;
        if id as usize >= n {
            return Err(format!("face_masks state id {id} out of range").into());
        }
        for mask in masks {
            if mask.len() != 64 || !mask.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(format!("state {id}: face mask '{mask}' is not 64 hex chars").into());
            }
        }
        masks_by_state.insert(id, masks);
    }
    for i in 0..n {
        let shaped = dump.can_occlude[i] == 1 && dump.use_shape_for_light_occlusion[i] == 1;
        if shaped != masks_by_state.contains_key(&(i as u32)) {
            return Err(format!(
                "state {i}: face masks {} but can_occlude && use_shape is {shaped}",
                if shaped { "missing" } else { "present" }
            )
            .into());
        }
    }
    let mut dict: Vec<&str> = Vec::new();
    let mut dict_index: std::collections::HashMap<&str, u32> = std::collections::HashMap::new();
    let mut state_masks: Vec<Option<[u32; 6]>> = vec![None; n];
    for (&id, masks) in &masks_by_state {
        let mut indices = [0u32; 6];
        for (slot, mask) in indices.iter_mut().zip(masks.iter()) {
            *slot = *dict_index.entry(mask).or_insert_with(|| {
                dict.push(mask);
                dict.len() as u32 - 1
            });
        }
        state_masks[id as usize] = Some(indices);
    }

    let mut out = String::new();
    writeln!(out, "{{")?;
    writeln!(
        out,
        "  \"version\": {},",
        serde_json::to_string(&dump.version)?
    )?;
    writeln!(out, "  \"state_count\": {n},")?;
    writeln!(out, "  \"masks\": {},", serde_json::to_string(&dict)?)?;
    writeln!(out, "  \"blocks\": [")?;
    let mut expected_id = 0u32;
    for (i, block) in blocks.blocks.iter().enumerate() {
        if block.first_id != expected_id {
            return Err(format!(
                "blocks table not dense at '{}': starts at {} expected {expected_id}",
                block.name, block.first_id
            )
            .into());
        }
        let count: u32 = block.props.iter().map(|(_, vs)| vs.len() as u32).product();
        let range = block.first_id as usize..(block.first_id + count) as usize;
        expected_id += count;

        let mut line = format!("    {{\"name\": {}", serde_json::to_string(&block.name)?);
        for (key, values) in [
            ("e", &dump.emission[range.clone()]),
            ("d", &dump.dampening[range.clone()]),
            ("p", &dump.propagates_skylight_down[range.clone()]),
            ("o", &dump.can_occlude[range.clone()]),
            ("u", &dump.use_shape_for_light_occlusion[range.clone()]),
        ] {
            write!(line, ", \"{key}\": {}", scalar_or_array(values)?)?;
        }
        let collision = &dump.has_collision[range.clone()];
        if collision.iter().any(|&c| c != collision[0]) {
            return Err(format!("{}: has_collision varies across states", block.name).into());
        }
        write!(line, ", \"c\": {}", collision[0])?;
        let masks = &state_masks[range];
        if masks.iter().any(Option::is_some) {
            if masks.iter().all(|m| *m == masks[0]) {
                // Uniform across the block: a single 6-index tuple.
                write!(
                    line,
                    ", \"f\": {}",
                    serde_json::to_string(&masks[0].unwrap())?
                )?;
            } else {
                // Per state: 6-index tuple or null.
                write!(line, ", \"f\": {}", serde_json::to_string(masks)?)?;
            }
        }
        let comma = if i + 1 < blocks.blocks.len() { "," } else { "" };
        writeln!(out, "{line}}}{comma}")?;
    }
    if expected_id as usize != n {
        return Err(format!("blocks cover {expected_id} states, dump has {n}").into());
    }
    writeln!(out, "  ]")?;
    writeln!(out, "}}")?;

    std::fs::write(out_path, &out)?;
    println!(
        "wrote state data for {} states ({} shaped, {} distinct masks) to {out_path}",
        n,
        masks_by_state.len(),
        dict.len()
    );
    Ok(())
}

/// A single JSON value when every entry is equal, else the full array.
fn scalar_or_array(values: &[u8]) -> Result<String, Error> {
    let first = *values.first().ok_or("block with zero states")?;
    if values.iter().all(|&v| v == first) {
        Ok(first.to_string())
    } else {
        Ok(serde_json::to_string(values)?)
    }
}
