//! Per-block-state collision shapes for the handful of blocks whose hitbox
//! isn't a full cube. Ported from the vanilla block classes (`SlabBlock`,
//! `StairBlock`, etc.). Boxes are block-local (0..1); the caller offsets them
//! to the block position.
//!
//! TODO: chests and many smaller shapes still fall back to a full cube.
//! Fence gates are represented from their block state; neighbor-derived
//! connections and entity-context shapes still need runtime context.
//!
//! TODO: blocks with no collision but a small outline (torches, flowers,
//! buttons, plants, redstone dust) fall back to a full cube too, so the
//! crosshair still reaches them from a block away.

use azalea_block::BlockState;

use crate::world::block::PropMap;

/// A block-local axis-aligned box: `[min_x, min_y, min_z, max_x, max_y,
/// max_z]`.
pub type LocalBox = [f64; 6];

/// Cached collision boxes for `state`: `None` for a full cube, `Some(&[])` for
/// no collision, `Some(boxes)` for a partial shape.
pub fn partial_shape(state: BlockState) -> Option<&'static [LocalBox]> {
    // Explicit shapes can override hasCollision (WallHangingSignBlock does).
    crate::world::block::block_shape(state)
        .or_else(|| (!crate::world::block::has_collision(state)).then_some(&[][..]))
}

const FULL_CUBE_SHAPE: &[LocalBox] = &[[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]];

/// Boxes the interaction raycast clips against (vanilla `getShape`). Unlike
/// `partial_shape` the full-cube case is already resolved, so an empty slice
/// means "not targetable" rather than "no collision".
pub fn outline_shape(state: BlockState) -> &'static [LocalBox] {
    crate::world::block::block_outline(state).unwrap_or(FULL_CUBE_SHAPE)
}

/// Computes one state's shape. Takes id/props rather than a `BlockState` so
/// the block-table build can call it without re-entering the table.
pub(crate) fn compute_shape(id: &str, props: &PropMap) -> Option<Vec<LocalBox>> {
    if id.ends_with("_slab") {
        return Some(match props.get("type") {
            Some("top") => vec![[0.0, 0.5, 0.0, 1.0, 1.0, 1.0]],
            Some("double") => return None,             // full cube
            _ => vec![[0.0, 0.0, 0.0, 1.0, 0.5, 1.0]], // bottom
        });
    }

    if id.ends_with("_stairs") {
        return Some(stair_boxes(
            props.get("half").unwrap_or("bottom"),
            props.get("facing").unwrap_or("north"),
            props.get("shape").unwrap_or("straight"),
        ));
    }

    if matches!(
        id,
        "oak_shelf"
            | "spruce_shelf"
            | "birch_shelf"
            | "jungle_shelf"
            | "acacia_shelf"
            | "dark_oak_shelf"
            | "mangrove_shelf"
            | "cherry_shelf"
            | "bamboo_shelf"
            | "crimson_shelf"
            | "warped_shelf"
            | "pale_oak_shelf"
    ) {
        return Some(shelf_shape(props.get("facing").unwrap_or("north")));
    }

    match id {
        _ if id.ends_with("_wall_hanging_sign") => Some(wall_hanging_sign_shape(props)),
        _ if id.ends_with("_sign") => Some(Vec::new()),
        "hopper" => Some(hopper_shape(props.get("facing").unwrap_or("down"))),
        "chain" => Some(chain_shape(props)),
        _ if id.ends_with("_chain") => Some(chain_shape(props)),
        "dirt_path" | "farmland" => Some(vec![[0.0, 0.0, 0.0, 1.0, 0.9375, 1.0]]),
        _ if id.ends_with("_carpet") => Some(vec![[0.0, 0.0, 0.0, 1.0, 0.0625, 1.0]]),
        _ if id.ends_with("_door") => Some(door_shape(props)),
        _ if id.ends_with("_trapdoor") => Some(trapdoor_shape(props)),
        _ if id.ends_with("_bed") => Some(bed_shape(props)),
        _ if id.ends_with("_fence") => Some(cross_collision_shape(props, 4.0 / 16.0, 24.0 / 16.0)),
        _ if id.ends_with("_fence_gate") => Some(fence_gate_shape(props)),
        "iron_bars" => Some(cross_collision_shape(props, 2.0 / 16.0, 1.0)),
        _ if id.ends_with("_pane") => Some(cross_collision_shape(props, 2.0 / 16.0, 1.0)),
        _ if id.ends_with("_wall") => Some(wall_shape(props)),
        "cactus" => Some(vec![[
            1.0 / 16.0,
            0.0,
            1.0 / 16.0,
            15.0 / 16.0,
            15.0 / 16.0,
            15.0 / 16.0,
        ]]),
        "ladder" => Some(vec![ladder_shape(props.get("facing").unwrap_or("north"))]),
        "anvil" | "chipped_anvil" | "damaged_anvil" => {
            Some(anvil_shape(props.get("facing").unwrap_or("north")))
        }
        "cake" => {
            let bites = props
                .get("bites")
                .and_then(|v| v.parse::<i32>().ok())
                .unwrap_or(0)
                .clamp(0, 6);
            Some(vec![[
                (1 + bites * 2) as f64 / 16.0,
                0.0,
                1.0 / 16.0,
                15.0 / 16.0,
                0.5,
                15.0 / 16.0,
            ]])
        }
        // `SnowLayerBlock.getCollisionShape` is one layer shorter than its
        // outline, so a single layer has no collision at all.
        "snow" => Some(snow_shape(snow_layers(props) - 1)),
        _ => None,
    }
}

/// Vanilla `getShape` where it differs from `getCollisionShape`. `None` means
/// the two agree, so `compute_shape`'s result doubles as the outline.
pub(crate) fn compute_outline(id: &str, props: &PropMap) -> Option<Vec<LocalBox>> {
    match id {
        // Most specific suffix first: all four families also end in _sign.
        _ if id.ends_with("_wall_hanging_sign") => {
            let facing = props.get("facing").unwrap_or("north");
            let mut boxes = wall_hanging_sign_shape(props);
            boxes.insert(
                0,
                rotate_horizontal_box([0.0625, 0.0, 0.4375, 0.9375, 0.625, 0.5625], facing),
            );
            Some(boxes)
        }
        _ if id.ends_with("_hanging_sign") => Some(vec![match props.get("rotation") {
            Some("0" | "8") => [0.0625, 0.0, 0.4375, 0.9375, 0.625, 0.5625],
            Some("4" | "12") => [0.4375, 0.0, 0.0625, 0.5625, 0.625, 0.9375],
            _ => [0.1875, 0.0, 0.1875, 0.8125, 1.0, 0.8125],
        }]),
        _ if id.ends_with("_wall_sign") => Some(vec![rotate_horizontal_box(
            [0.0, 0.28125, 0.875, 1.0, 0.78125, 1.0],
            props.get("facing").unwrap_or("north"),
        )]),
        _ if id.ends_with("_sign") => Some(vec![[0.25, 0.0, 0.25, 0.75, 1.0, 0.75]]),
        _ if id.ends_with("_fence") => Some(cross_collision_shape(props, 4.0 / 16.0, 1.0)),
        "snow" => Some(snow_shape(snow_layers(props))),
        // `LiquidBlock.getShape` and `BubbleColumnBlock.getShape` are
        // `Shapes.empty()`: the pick ray clips straight through them.
        "water" | "lava" | "bubble_column" => Some(Vec::new()),
        _ => None,
    }
}

fn hopper_shape(facing: &str) -> Vec<LocalBox> {
    // 26.2 HopperBlock.makeShapes: bowl minus its 12px-wide inside, plus
    // the body and spout. ENABLED does not change either shape.
    let mut boxes = vec![
        [0.25, 0.25, 0.25, 0.75, 0.625, 0.75],
        [0.0, 0.625, 0.0, 1.0, 0.6875, 1.0],
        [0.0, 0.6875, 0.0, 0.125, 1.0, 1.0],
        [0.875, 0.6875, 0.0, 1.0, 1.0, 1.0],
        [0.125, 0.6875, 0.0, 0.875, 1.0, 0.125],
        [0.125, 0.6875, 0.875, 0.875, 1.0, 1.0],
    ];
    boxes.push(if facing == "down" {
        [0.375, 0.0, 0.375, 0.625, 0.25, 0.625]
    } else {
        rotate_horizontal_box([0.375, 0.25, 0.0, 0.625, 0.5, 0.25], facing)
    });
    boxes
}

/// Vanilla's separate interaction shape only overrides the hit direction,
/// never the outline hit location or whether the block was hit at all.
pub(crate) fn interaction_shape(state: BlockState) -> &'static [LocalBox] {
    const INSIDE: LocalBox = [0.125, 0.6875, 0.125, 0.875, 1.0, 0.875];
    if crate::world::block::block_id(state) != "hopper" {
        return &[];
    }
    match crate::world::block::block_properties(state).get("facing") {
        Some("north") => &[INSIDE, [0.375, 0.5, 0.0, 0.625, 0.625, 0.25]],
        Some("south") => &[INSIDE, [0.375, 0.5, 0.75, 0.625, 0.625, 1.0]],
        Some("west") => &[INSIDE, [0.0, 0.5, 0.375, 0.25, 0.625, 0.625]],
        Some("east") => &[INSIDE, [0.75, 0.5, 0.375, 1.0, 0.625, 0.625]],
        _ => &[INSIDE],
    }
}

fn chain_shape(props: &PropMap) -> Vec<LocalBox> {
    // ChainBlock: three-pixel-wide rod, shared by iron and all copper variants.
    vec![match props.get("axis") {
        Some("x") => [0.0, 0.40625, 0.40625, 1.0, 0.59375, 0.59375],
        Some("z") => [0.40625, 0.40625, 0.0, 0.59375, 0.59375, 1.0],
        _ => [0.40625, 0.0, 0.40625, 0.59375, 1.0, 0.59375],
    }]
}

fn wall_hanging_sign_shape(props: &PropMap) -> Vec<LocalBox> {
    // WallHangingSignBlock.getCollisionShape: crossbar only, not the board.
    vec![rotate_horizontal_box(
        [0.0, 0.875, 0.375, 1.0, 1.0, 0.625],
        props.get("facing").unwrap_or("north"),
    )]
}

fn shelf_shape(facing: &str) -> Vec<LocalBox> {
    [
        [0.0, 0.75, 0.6875, 1.0, 1.0, 0.8125],
        [0.0, 0.0, 0.8125, 1.0, 1.0, 1.0],
        [0.0, 0.0, 0.6875, 1.0, 0.25, 0.8125],
    ]
    .into_iter()
    .map(|b| rotate_horizontal_box(b, facing))
    .collect()
}

fn fence_gate_shape(props: &PropMap) -> Vec<LocalBox> {
    if props.get("open") == Some("true") {
        return Vec::new();
    }
    let bar: LocalBox = [0.0, 0.0, 6.0 / 16.0, 1.0, 1.5, 10.0 / 16.0];
    let facing = props.get("facing").unwrap_or("north");
    if matches!(facing, "east" | "west") {
        vec![rot_y90(bar)]
    } else {
        vec![bar]
    }
}

fn cross_collision_shape(props: &PropMap, width: f64, height: f64) -> Vec<LocalBox> {
    let half = (1.0 - width) / 2.0;
    let mut boxes = vec![[half, 0.0, half, 1.0 - half, height, 1.0 - half]];
    for (direction, connected) in [
        ("north", "north"),
        ("east", "east"),
        ("south", "south"),
        ("west", "west"),
    ] {
        if props.get(connected) == Some("true") {
            boxes.push(match direction {
                "north" => [half, 0.0, 0.0, 1.0 - half, height, 0.5],
                "south" => [half, 0.0, 0.5, 1.0 - half, height, 1.0],
                "east" => [0.5, 0.0, half, 1.0, height, 1.0 - half],
                _ => [0.0, 0.0, half, 0.5, height, 1.0 - half],
            });
        }
    }
    boxes
}

fn wall_shape(props: &PropMap) -> Vec<LocalBox> {
    let mut boxes = Vec::new();
    if props.get("up") != Some("false") {
        boxes.push([0.25, 0.0, 0.25, 0.75, 1.5, 0.75]);
    }
    for (direction, key) in [
        ("north", "north"),
        ("east", "east"),
        ("south", "south"),
        ("west", "west"),
    ] {
        if matches!(props.get(key), Some("low" | "tall")) {
            boxes.push(match direction {
                "north" => [0.3125, 0.0, 0.0, 0.6875, 1.5, 0.6875],
                "south" => [0.3125, 0.0, 0.3125, 0.6875, 1.5, 1.0],
                "east" => [0.3125, 0.0, 0.3125, 1.0, 1.5, 0.6875],
                _ => [0.0, 0.0, 0.3125, 0.6875, 1.5, 0.6875],
            });
        }
    }
    boxes
}

fn ladder_shape(facing: &str) -> LocalBox {
    rotate_horizontal_box([0.0, 0.0, 13.0 / 16.0, 1.0, 1.0, 1.0], facing)
}

fn anvil_shape(facing: &str) -> Vec<LocalBox> {
    let axis_x = matches!(facing, "east" | "west");
    let boxes = vec![
        [
            2.0 / 16.0,
            0.0,
            2.0 / 16.0,
            14.0 / 16.0,
            4.0 / 16.0,
            14.0 / 16.0,
        ],
        [
            4.0 / 16.0,
            4.0 / 16.0,
            3.0 / 16.0,
            12.0 / 16.0,
            5.0 / 16.0,
            13.0 / 16.0,
        ],
        [
            6.0 / 16.0,
            5.0 / 16.0,
            4.0 / 16.0,
            10.0 / 16.0,
            10.0 / 16.0,
            12.0 / 16.0,
        ],
        [3.0 / 16.0, 10.0 / 16.0, 0.0, 13.0 / 16.0, 1.0, 1.0],
    ];
    if axis_x {
        boxes.into_iter().map(|b| rot_y90(b)).collect()
    } else {
        boxes
    }
}

fn door_shape(props: &PropMap) -> Vec<LocalBox> {
    let facing = props.get("facing").unwrap_or("north");
    let direction = if props.get("open") == Some("true") {
        match (facing, props.get("hinge").unwrap_or("left")) {
            ("north", "left") | ("south", "right") => "east",
            ("north", "right") | ("south", "left") => "west",
            ("east", "left") | ("west", "right") => "south",
            ("east", "right") | ("west", "left") => "north",
            _ => facing,
        }
    } else {
        facing
    };
    let north = [0.0, 0.0, 0.8125, 1.0, 1.0, 1.0];
    vec![rotate_horizontal_box(north, direction)]
}

fn bed_shape(props: &PropMap) -> Vec<LocalBox> {
    let facing = props.get("facing").unwrap_or("north");
    let direction = match (props.get("part").unwrap_or("foot"), facing) {
        ("head", "north") | ("foot", "south") => "north",
        ("head", "south") | ("foot", "north") => "south",
        ("head", "east") | ("foot", "west") => "east",
        _ => "west",
    };
    let mut boxes = vec![
        [0.0, 0.1875, 0.0, 1.0, 0.5625, 1.0],
        [0.0, 0.0, 0.0, 0.1875, 0.1875, 0.1875],
        [0.8125, 0.0, 0.0, 1.0, 0.1875, 0.1875],
    ];
    for box_ in &mut boxes[1..] {
        *box_ = rotate_horizontal_box(*box_, direction);
    }
    boxes
}

fn trapdoor_shape(props: &PropMap) -> Vec<LocalBox> {
    let half = props.get("half").unwrap_or("bottom");
    let shape = if props.get("open") == Some("true") {
        let north = [0.0, 0.0, 0.8125, 1.0, 1.0, 1.0];
        rotate_horizontal_box(north, props.get("facing").unwrap_or("north"))
    } else if half == "top" {
        [0.0, 0.8125, 0.0, 1.0, 1.0, 1.0]
    } else {
        [0.0, 0.0, 0.0, 1.0, 0.1875, 1.0]
    };
    vec![shape]
}

fn rotate_horizontal_box(mut b: LocalBox, facing: &str) -> LocalBox {
    for _ in 0..dir_steps(facing) {
        b = rot_y90(b);
    }
    b
}

fn snow_layers(props: &PropMap) -> i32 {
    props
        .get("layers")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1)
}

/// Vanilla `SnowLayerBlock.SHAPES[layers]`, two pixels per layer; index 0 is
/// empty.
fn snow_shape(layers: i32) -> Vec<LocalBox> {
    if layers <= 0 {
        return Vec::new();
    }
    vec![[0.0, 0.0, 0.0, 1.0, layers as f64 * 2.0 / 16.0, 1.0]]
}

/// Vanilla `StairBlock` shape: a half-slab plus 1–3 upper corner pillars,
/// rotated to `facing`/`shape` and Y-flipped for the top half.
fn stair_boxes(half: &str, facing: &str, shape: &str) -> Vec<LocalBox> {
    // Base shape faces north, bottom half. SHAPE_OUTER is the half-slab plus one
    // corner; STRAIGHT adds its 90° rotation; INNER adds a third corner.
    let mut boxes = vec![[0.0, 0.0, 0.0, 1.0, 0.5, 1.0]];
    let corner: LocalBox = [0.0, 0.5, 0.0, 0.5, 1.0, 0.5];
    match shape {
        "inner_left" | "inner_right" => {
            boxes.push(corner);
            boxes.push(rot_y90(corner));
            boxes.push(rot_y90(rot_y90(corner)));
        }
        "outer_left" | "outer_right" => boxes.push(corner),
        _ => {
            boxes.push(corner);
            boxes.push(rot_y90(corner));
        }
    }

    if half == "top" {
        for b in &mut boxes {
            *b = invert_y(*b);
        }
    }

    // Vanilla derives the lookup direction from facing and shape.
    let dir = match shape {
        "inner_left" => ccw(facing),
        "outer_right" => cw(facing),
        _ => facing,
    };
    for _ in 0..dir_steps(dir) {
        for b in &mut boxes {
            *b = rot_y90(*b);
        }
    }

    boxes
}

/// Rotate a box 90° about the block's vertical center axis: `(x, z)` -> `(1-z,
/// x)`.
fn rot_y90([x0, y0, z0, x1, y1, z1]: LocalBox) -> LocalBox {
    [1.0 - z1, y0, x0, 1.0 - z0, y1, x1]
}

fn invert_y([x0, y0, z0, x1, y1, z1]: LocalBox) -> LocalBox {
    [x0, 1.0 - y1, z0, x1, 1.0 - y0, z1]
}

fn dir_steps(facing: &str) -> u32 {
    match facing {
        "east" => 1,
        "south" => 2,
        "west" => 3,
        _ => 0, // north
    }
}

fn cw(facing: &str) -> &'static str {
    match facing {
        "north" => "east",
        "east" => "south",
        "south" => "west",
        _ => "north",
    }
}

fn ccw(facing: &str) -> &'static str {
    match facing {
        "north" => "west",
        "west" => "south",
        "south" => "east",
        _ => "north",
    }
}

#[cfg(test)]
mod tests {
    use super::{LocalBox, outline_shape, partial_shape};

    const SIGN_WOODS: [&str; 12] = [
        "oak", "spruce", "birch", "jungle", "acacia", "dark_oak", "mangrove", "cherry", "bamboo",
        "crimson", "warped", "pale_oak",
    ];

    #[test]
    fn sign_shapes_match_vanilla_for_all_directions_and_rotations() {
        crate::world::block::init("26.2");
        for wood in SIGN_WOODS {
            for waterlogged in ["false", "true"] {
                for rotation in 0..16 {
                    let rotation = rotation.to_string();
                    let props = [
                        ("rotation", rotation.as_str()),
                        ("waterlogged", waterlogged),
                    ];
                    let standing = crate::world::block::find_state(&format!("{wood}_sign"), &props);
                    assert_eq!(partial_shape(standing), Some(&[][..]));
                    assert_eq!(
                        outline_shape(standing),
                        [[0.25, 0.0, 0.25, 0.75, 1.0, 0.75]]
                    );
                    for attached in ["false", "true"] {
                        let hanging = crate::world::block::find_state(
                            &format!("{wood}_hanging_sign"),
                            &[
                                ("rotation", rotation.as_str()),
                                ("attached", attached),
                                ("waterlogged", waterlogged),
                            ],
                        );
                        let expected = match rotation.as_str() {
                            "0" | "8" => [0.0625, 0.0, 0.4375, 0.9375, 0.625, 0.5625],
                            "4" | "12" => [0.4375, 0.0, 0.0625, 0.5625, 0.625, 0.9375],
                            _ => [0.1875, 0.0, 0.1875, 0.8125, 1.0, 0.8125],
                        };
                        assert_eq!(partial_shape(hanging), Some(&[][..]));
                        assert_eq!(
                            outline_shape(hanging),
                            [expected],
                            "{wood} {rotation} {attached}"
                        );
                    }
                }
                for (facing, wall, board, bar) in [
                    (
                        "north",
                        [0.0, 0.28125, 0.875, 1.0, 0.78125, 1.0],
                        [0.0625, 0.0, 0.4375, 0.9375, 0.625, 0.5625],
                        [0.0, 0.875, 0.375, 1.0, 1.0, 0.625],
                    ),
                    (
                        "south",
                        [0.0, 0.28125, 0.0, 1.0, 0.78125, 0.125],
                        [0.0625, 0.0, 0.4375, 0.9375, 0.625, 0.5625],
                        [0.0, 0.875, 0.375, 1.0, 1.0, 0.625],
                    ),
                    (
                        "east",
                        [0.0, 0.28125, 0.0, 0.125, 0.78125, 1.0],
                        [0.4375, 0.0, 0.0625, 0.5625, 0.625, 0.9375],
                        [0.375, 0.875, 0.0, 0.625, 1.0, 1.0],
                    ),
                    (
                        "west",
                        [0.875, 0.28125, 0.0, 1.0, 0.78125, 1.0],
                        [0.4375, 0.0, 0.0625, 0.5625, 0.625, 0.9375],
                        [0.375, 0.875, 0.0, 0.625, 1.0, 1.0],
                    ),
                ] {
                    let props = [("facing", facing), ("waterlogged", waterlogged)];
                    let wall_sign =
                        crate::world::block::find_state(&format!("{wood}_wall_sign"), &props);
                    assert_eq!(partial_shape(wall_sign), Some(&[][..]));
                    assert_eq!(outline_shape(wall_sign), [wall], "{wood} {facing}");
                    let hanging = crate::world::block::find_state(
                        &format!("{wood}_wall_hanging_sign"),
                        &props,
                    );
                    assert!(!crate::world::block::has_collision(hanging));
                    assert_eq!(partial_shape(hanging), Some(&[bar][..]), "{wood} {facing}");
                    assert_eq!(outline_shape(hanging), [board, bar], "{wood} {facing}");
                }
            }
        }
    }

    #[test]
    fn hopper_shapes_match_registry_oracle_in_all_five_directions_and_enabled_states() {
        crate::world::block::init("26.2");
        let oracle: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../third_party/SteelMC/steel-registry/build_assets/blocks.json"
            ))
            .unwrap(),
        )
        .unwrap();
        let hopper = oracle["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["name"] == "hopper")
            .unwrap();
        let oracle_boxes = |kind: &str, offset: usize| -> Vec<LocalBox> {
            let shape = &hopper[kind];
            let indices = shape["overwrites"]
                .as_array()
                .unwrap()
                .iter()
                .find(|entry| entry["offset"] == offset)
                .map_or(&shape["default"], |entry| &entry["shapes"]);
            indices
                .as_array()
                .unwrap()
                .iter()
                .map(|index| {
                    let b = &oracle["shapes"][index.as_u64().unwrap() as usize];
                    std::array::from_fn(|axis| {
                        b[if axis < 3 { "min" } else { "max" }][axis % 3]
                            .as_f64()
                            .unwrap()
                    })
                })
                .collect()
        };
        let contains = |boxes: &[LocalBox], point: [f64; 3]| {
            boxes
                .iter()
                .any(|b| (0..3).all(|axis| point[axis] >= b[axis] && point[axis] < b[axis + 3]))
        };
        for (direction, facing) in ["down", "north", "south", "west", "east"]
            .into_iter()
            .enumerate()
        {
            for (enabled_index, enabled) in ["true", "false"].into_iter().enumerate() {
                let state = crate::world::block::find_state(
                    "hopper",
                    &[("facing", facing), ("enabled", enabled)],
                );
                let collision = partial_shape(state).unwrap();
                assert_eq!(outline_shape(state), collision);
                assert_eq!(
                    super::compute_outline("hopper", crate::world::block::block_properties(state)),
                    None
                );
                for (kind, actual) in [
                    ("collision_shapes", collision),
                    ("outline_shapes", outline_shape(state)),
                    ("interaction_shapes", super::interaction_shape(state)),
                ] {
                    let expected = oracle_boxes(kind, direction + enabled_index * 5);
                    // Every oracle and local bound is on this 1/16 grid; cell
                    // occupancy checks the union, not a particular box split.
                    for x in 0..16 {
                        for y in 0..16 {
                            for z in 0..16 {
                                let point = [x, y, z].map(|v| (v as f64 + 0.5) / 16.0);
                                assert_eq!(
                                    contains(actual, point),
                                    contains(&expected, point),
                                    "{facing} {enabled} {kind} {point:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn chain_variants_use_axis_collision_as_outline_fallback() {
        crate::world::block::init("26.2");
        for id in [
            "iron_chain",
            "copper_chain",
            "exposed_copper_chain",
            "weathered_copper_chain",
            "oxidized_copper_chain",
            "waxed_copper_chain",
            "waxed_exposed_copper_chain",
            "waxed_weathered_copper_chain",
            "waxed_oxidized_copper_chain",
        ] {
            for waterlogged in ["false", "true"] {
                for (axis, expected) in [
                    ("x", [0.0, 0.40625, 0.40625, 1.0, 0.59375, 0.59375]),
                    ("y", [0.40625, 0.0, 0.40625, 0.59375, 1.0, 0.59375]),
                    ("z", [0.40625, 0.40625, 0.0, 0.59375, 0.59375, 1.0]),
                ] {
                    let state = crate::world::block::find_state(
                        id,
                        &[("axis", axis), ("waterlogged", waterlogged)],
                    );
                    assert_eq!(partial_shape(state), Some(&[expected][..]), "{id} {axis}");
                    assert_eq!(outline_shape(state), [expected], "{id} {axis}");
                    assert_eq!(
                        super::compute_outline(id, crate::world::block::block_properties(state)),
                        None
                    );
                }
            }
        }
        // The legacy id shares the helper, but chain_command_block is not a chain.
        let props = crate::world::block::block_properties(crate::world::block::find_state(
            "iron_chain",
            &[("axis", "y")],
        ));
        assert_eq!(
            super::compute_shape("chain", props),
            super::compute_shape("iron_chain", props)
        );
        assert_eq!(
            partial_shape(crate::world::block::find_state("chain_command_block", &[])),
            None
        );
        assert_eq!(
            partial_shape(crate::world::block::find_state("torch", &[])),
            Some(&[][..])
        );
    }

    #[test]
    fn sign_outline_raycast_hits_board_but_passes_outside_to_chest() {
        use azalea_core::position::{BlockPos, ChunkPos};
        use glam::{Vec3, dvec3};

        use crate::world::border::WorldBorder;
        use crate::world::chunk::ChunkStore;

        crate::world::block::init("26.2");
        let mut chunks = ChunkStore::new(1);
        chunks.partial_storage.set(
            &ChunkPos::new(0, 0),
            Some(azalea_world::chunk::Chunk::default()),
            &mut chunks.chunk_storage,
        );
        let sign_pos = BlockPos::new(2, 64, 2);
        let chest_pos = BlockPos::new(2, 64, 4);
        chunks.set_block_state(2, 64, 4, crate::world::block::find_state("chest", &[]));
        for id in [
            "oak_sign",
            "oak_wall_sign",
            "oak_hanging_sign",
            "oak_wall_hanging_sign",
        ] {
            let props = if matches!(id, "oak_wall_sign" | "oak_wall_hanging_sign") {
                [("facing", "north")]
            } else {
                [("rotation", "0")]
            };
            let state = crate::world::block::find_state(id, &props);
            chunks.set_block_state(2, 64, 2, state);
            let ray = |x, y| {
                crate::player::interaction::raycast(
                    dvec3(x, y, 0.5),
                    Vec3::Z,
                    6.0,
                    &chunks,
                    &WorldBorder::default(),
                )
                .unwrap()
                .block_pos
            };
            assert_eq!(ray(2.5, 64.5), sign_pos, "{id} board");
            assert_eq!(ray(2.1, 64.8), chest_pos, "{id} outside board");
            if id == "oak_wall_hanging_sign" {
                assert_eq!(ray(2.1, 64.9375), sign_pos, "crossbar remains targetable");
                assert_eq!(ray(2.5, 64.75), chest_pos, "gap between board and crossbar");
            }
        }
    }

    #[test]
    fn shelves_match_vanilla_shapes_and_outline_in_all_horizontal_directions() {
        crate::world::block::init("26.2");
        let expected = [
            (
                "north",
                [
                    [0.0, 0.75, 0.6875, 1.0, 1.0, 0.8125],
                    [0.0, 0.0, 0.8125, 1.0, 1.0, 1.0],
                    [0.0, 0.0, 0.6875, 1.0, 0.25, 0.8125],
                ],
            ),
            (
                "east",
                [
                    [0.1875, 0.75, 0.0, 0.3125, 1.0, 1.0],
                    [0.0, 0.0, 0.0, 0.1875, 1.0, 1.0],
                    [0.1875, 0.0, 0.0, 0.3125, 0.25, 1.0],
                ],
            ),
            (
                "south",
                [
                    [0.0, 0.75, 0.1875, 1.0, 1.0, 0.3125],
                    [0.0, 0.0, 0.0, 1.0, 1.0, 0.1875],
                    [0.0, 0.0, 0.1875, 1.0, 0.25, 0.3125],
                ],
            ),
            (
                "west",
                [
                    [0.6875, 0.75, 0.0, 0.8125, 1.0, 1.0],
                    [0.8125, 0.0, 0.0, 1.0, 1.0, 1.0],
                    [0.6875, 0.0, 0.0, 0.8125, 0.25, 1.0],
                ],
            ),
        ];
        for (facing, boxes) in &expected {
            let state = crate::world::block::find_state("oak_shelf", &[("facing", *facing)]);
            assert_eq!(partial_shape(state), Some(&boxes[..]), "{facing}");
            assert_eq!(outline_shape(state), boxes, "{facing}");
        }

        let north = expected[0].1;
        let player = [0.2, 0.0, 0.0, 0.8, 1.8, 0.6];
        let intersects = |a: LocalBox, b: LocalBox| {
            a[0] < b[3] && a[3] > b[0] && a[1] < b[4] && a[4] > b[1] && a[2] < b[5] && a[5] > b[2]
        };
        assert!(!north.iter().any(|&b| intersects(b, player)));
        assert!(intersects(north[1], [0.2, 0.0, 0.85, 0.8, 1.8, 0.95]));
    }

    #[test]
    fn supported_collision_families_follow_state_connections() {
        crate::world::block::init("26.2");
        let shape = |id, props| {
            crate::world::block::block_shape(crate::world::block::find_state(id, props)).unwrap()
        };
        let connected_fence = shape(
            "oak_fence",
            &[
                ("north", "true"),
                ("east", "false"),
                ("south", "false"),
                ("west", "false"),
            ],
        );
        assert_eq!(connected_fence.len(), 2);
        assert_eq!(connected_fence[0], [0.375, 0.0, 0.375, 0.625, 1.5, 0.625]);
        assert_eq!(shape("cactus", &[])[0][4], 15.0 / 16.0);
        assert_eq!(shape("ladder", &[("facing", "north")])[0][2], 13.0 / 16.0);
        assert_eq!(shape("anvil", &[("facing", "north")]).len(), 4);
    }

    #[test]
    fn cross_connections_keep_their_collision_and_outline_heights_in_all_directions() {
        crate::world::block::init("26.2");
        for (id, half, height) in [
            ("oak_fence", 0.375, 1.5),
            ("nether_brick_fence", 0.375, 1.5),
            ("iron_bars", 0.4375, 1.0),
            ("glass_pane", 0.4375, 1.0),
            ("white_stained_glass_pane", 0.4375, 1.0),
        ] {
            for (direction, arm) in [
                ("north", [half, 0.0, 0.0, 1.0 - half, height, 0.5]),
                ("east", [0.5, 0.0, half, 1.0, height, 1.0 - half]),
                ("south", [half, 0.0, 0.5, 1.0 - half, height, 1.0]),
                ("west", [0.0, 0.0, half, 0.5, height, 1.0 - half]),
            ] {
                let props = ["north", "east", "south", "west"]
                    .map(|key| (key, if key == direction { "true" } else { "false" }));
                let state = crate::world::block::find_state(id, &props);
                let collision = [[half, 0.0, half, 1.0 - half, height, 1.0 - half], arm];
                assert_eq!(
                    partial_shape(state),
                    Some(&collision[..]),
                    "{id} {direction}"
                );
                let outline = collision.map(|mut b| {
                    b[4] = 1.0;
                    b
                });
                assert_eq!(outline_shape(state), outline, "{id} {direction}");
            }
        }
    }

    #[test]
    fn collision_shapes_follow_vanilla_state_properties() {
        crate::world::block::init("26.2");
        let shape = |id, props| {
            crate::world::block::block_shape(crate::world::block::find_state(id, props)).unwrap()
        };

        assert_eq!(
            shape(
                "oak_door",
                &[("facing", "north"), ("open", "false"), ("hinge", "left")]
            ),
            &[[0.0, 0.0, 0.8125, 1.0, 1.0, 1.0]]
        );
        assert_eq!(
            shape(
                "oak_door",
                &[("facing", "north"), ("open", "true"), ("hinge", "left")]
            ),
            &[[0.0, 0.0, 0.0, 0.1875, 1.0, 1.0]]
        );
        assert_eq!(
            shape(
                "oak_trapdoor",
                &[("half", "top"), ("open", "false"), ("facing", "north")]
            ),
            &[[0.0, 0.8125, 0.0, 1.0, 1.0, 1.0]]
        );
        assert_eq!(shape("cake", &[("bites", "3")])[0][0], 7.0 / 16.0);
        assert_eq!(
            shape("oak_fence_gate", &[("facing", "north"), ("open", "false")]),
            &[[0.0, 0.0, 6.0 / 16.0, 1.0, 1.5, 10.0 / 16.0]]
        );
        assert!(shape("oak_fence_gate", &[("open", "true")]).is_empty());
        assert_eq!(
            shape("oak_fence_gate", &[("facing", "east"), ("open", "false")]),
            &[[6.0 / 16.0, 0.0, 0.0, 10.0 / 16.0, 1.5, 1.0]]
        );
        assert_eq!(
            shape("white_bed", &[("part", "foot"), ("facing", "north")]).len(),
            3
        );
    }
}
