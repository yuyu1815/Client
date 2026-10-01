//! Static terrestrial model bakes for mob-family integration.
//!
//! Coordinates and UV offsets are model pixels, as in `entity_model`. These
//! are intentionally geometry-only: per-entity pose/state and renderer layers
//! are supplied by the integration that registers these bakes.
use glam::Vec3;

use crate::renderer::entity_model::{BakedEntityModel, EntityPart, ModelCube, bake_model};

fn cube(origin: [f32; 3], size: [f32; 3], uv: (i32, i32)) -> ModelCube {
    ModelCube {
        origin: Vec3::from_array(origin),
        size: Vec3::from_array(size),
        tex_offset: uv,
        deformation: 0.0,
        mirror: false,
    }
}
fn part(name: &str, pivot: [f32; 3], boxes: Vec<ModelCube>) -> EntityPart {
    EntityPart {
        name: name.into(),
        offset: Vec3::from_array(pivot),
        default_rotation: Vec3::ZERO,
        cubes: boxes,
        parent: None,
    }
}
fn model(parts: Vec<EntityPart>, w: u32, h: u32) -> BakedEntityModel {
    bake_model(parts, w, h)
}
fn legs(width: f32, length: f32, y: f32, front: f32, back: f32, uv: (i32, i32)) -> Vec<EntityPart> {
    [
        (-width, back, "right_hind_leg"),
        (width, back, "left_hind_leg"),
        (-width, front, "right_front_leg"),
        (width, front, "left_front_leg"),
    ]
    .into_iter()
    .map(|(x, z, n)| {
        part(
            n,
            [x, y, z],
            vec![cube([-2.0, 0.0, -2.0], [4.0, length, 4.0], uv)],
        )
    })
    .collect()
}

pub fn bake_armadillo_model() -> BakedEntityModel {
    let mut p = vec![
        part(
            "body",
            [0.0, 14.0, 0.0],
            vec![
                cube([-5.0, -5.0, -7.0], [10.0, 10.0, 14.0], (0, 0)),
                cube([-4.0, -6.0, -6.0], [8.0, 2.0, 12.0], (0, 24)),
            ],
        ),
        part(
            "head",
            [0.0, 12.0, -6.0],
            vec![
                cube([-3.0, -3.0, -5.0], [6.0, 5.0, 6.0], (0, 16)),
                cube([-1.0, 0.0, -7.0], [2.0, 2.0, 2.0], (24, 16)),
            ],
        ),
    ];
    p.extend(legs(3.0, 4.0, 18.0, -4.0, 5.0, (24, 0)));
    model(p, 64, 64)
}

pub fn bake_camel_model() -> BakedEntityModel {
    let mut p = vec![
        part(
            "body",
            [0.0, 14.0, 0.0],
            vec![
                cube([-7.0, -5.0, -12.0], [14.0, 10.0, 24.0], (0, 25)),
                cube([-5.0, -11.0, -5.0], [10.0, 6.0, 10.0], (0, 0)),
            ],
        ),
        part(
            "neck",
            [0.0, 9.0, -9.0],
            vec![cube([-4.0, -14.0, -4.0], [8.0, 16.0, 8.0], (60, 0))],
        ),
        part(
            "head",
            [0.0, -5.0, -4.0],
            vec![cube([-3.5, -4.0, -7.0], [7.0, 8.0, 7.0], (60, 0))],
        ),
    ];
    p.extend(legs(5.0, 12.0, 12.0, -8.0, 8.0, (0, 0)));
    model(p, 128, 128)
}

pub fn bake_camel_husk_model() -> BakedEntityModel {
    bake_camel_model()
}

pub fn bake_fox_model() -> BakedEntityModel {
    let mut p = vec![
        part(
            "head",
            [-1.0, 16.5, -3.0],
            vec![
                cube([-3.0, -2.0, -5.0], [8.0, 6.0, 6.0], (1, 5)),
                cube([-3.0, -4.0, -4.0], [2.0, 2.0, 1.0], (8, 1)),
                cube([3.0, -4.0, -4.0], [2.0, 2.0, 1.0], (15, 1)),
                cube([-1.0, 2.01, -8.0], [4.0, 2.0, 3.0], (6, 18)),
            ],
        ),
        part(
            "body",
            [0.0, 16.0, -6.0],
            vec![cube([-3.0, 4.0, -3.5], [6.0, 11.0, 6.0], (24, 15))],
        ),
        part(
            "tail",
            [-4.0, 15.0, -1.0],
            vec![cube([2.0, 0.0, -1.0], [4.0, 9.0, 5.0], (30, 0))],
        ),
    ];
    p.extend(legs(3.0, 6.0, 17.5, 0.0, 7.0, (4, 24)));
    model(p, 48, 32)
}

pub fn bake_frog_model() -> BakedEntityModel {
    let mut p = vec![
        part(
            "body",
            [0.0, 18.0, 0.0],
            vec![cube([-5.0, -4.0, -7.0], [10.0, 8.0, 14.0], (0, 0))],
        ),
        part(
            "head",
            [0.0, 16.0, -7.0],
            vec![
                cube([-5.0, -3.0, -5.0], [10.0, 6.0, 8.0], (0, 0)),
                cube([-6.0, -5.0, -4.0], [4.0, 2.0, 4.0], (0, 0)),
                cube([2.0, -5.0, -4.0], [4.0, 2.0, 4.0], (0, 0)),
            ],
        ),
    ];
    p.extend(legs(4.0, 4.0, 20.0, -6.0, 6.0, (0, 0)));
    model(p, 48, 48)
}

pub fn bake_goat_model() -> BakedEntityModel {
    let mut p = vec![
        part(
            "head",
            [0.0, 4.0, -8.0],
            vec![
                cube([-4.0, -4.0, -6.0], [8.0, 8.0, 6.0], (0, 0)),
                cube([-3.0, 1.0, -7.0], [6.0, 3.0, 1.0], (1, 33)),
                cube([-5.0, -5.0, -5.0], [1.0, 3.0, 1.0], (22, 0)),
                cube([4.0, -5.0, -5.0], [1.0, 3.0, 1.0], (22, 0)),
                cube([-6.0, -3.0, -2.0], [2.0, 8.0, 2.0], (12, 0)),
                cube([4.0, -3.0, -2.0], [2.0, 8.0, 2.0], (12, 0)),
            ],
        ),
        part(
            "body",
            [0.0, 5.0, 2.0],
            vec![cube([-6.0, -10.0, -7.0], [12.0, 18.0, 10.0], (18, 4))],
        ),
    ];
    p.extend(legs(4.0, 12.0, 12.0, -5.0, 7.0, (0, 16)));
    model(p, 64, 64)
}

fn hoglin_model(zoglin: bool) -> BakedEntityModel {
    let mut p = vec![
        part(
            "head",
            [0.0, 6.0, -8.0],
            vec![
                cube([-6.0, -5.0, -8.0], [12.0, 10.0, 10.0], (1, 1)),
                cube([-5.0, 1.0, -10.0], [10.0, 4.0, 2.0], (1, 21)),
                cube([-7.0, -8.0, -6.0], [3.0, 4.0, 3.0], (61, 1)),
                cube([4.0, -8.0, -6.0], [3.0, 4.0, 3.0], (61, 1)),
                cube([-7.0, 2.0, -11.0], [3.0, 5.0, 2.0], (1, 1)),
                cube([4.0, 2.0, -11.0], [3.0, 5.0, 2.0], (1, 1)),
            ],
        ),
        part(
            "body",
            [0.0, 11.0, 2.0],
            vec![cube([-8.0, -8.0, -10.0], [16.0, 14.0, 20.0], (1, 1))],
        ),
    ];
    p.extend(legs(6.0, 10.0, 13.0, -7.0, 7.0, (1, 1)));
    model(p, 128, 64)
}
pub fn bake_hoglin_model() -> BakedEntityModel {
    hoglin_model(false)
}
pub fn bake_zoglin_model() -> BakedEntityModel {
    hoglin_model(true)
}

/// Cow-shaped base overlay with the three actual mushroom caps/stems; must be
/// rendered over a cow body texture and is not a substitute for that base.
pub fn bake_mooshroom_mushrooms_model() -> BakedEntityModel {
    let cap = cube([-2.0, -2.0, -2.0], [4.0, 2.0, 4.0], (0, 0));
    model(
        vec![
            part("mushroom_back_left", [-5.0, 5.0, 2.0], vec![cap]),
            part("mushroom_back_right", [5.0, 5.0, 2.0], vec![cap]),
            part(
                "mushroom_head",
                [0.0, 4.0, -8.0],
                vec![cube([-3.0, -7.0, -6.0], [6.0, 4.0, 6.0], (0, 0))],
            ),
        ],
        64,
        32,
    )
}

pub fn bake_panda_model() -> BakedEntityModel {
    let mut p = vec![
        part(
            "head",
            [0.0, 10.0, -6.0],
            vec![
                cube([-6.0, -5.0, -6.0], [12.0, 10.0, 9.0], (0, 0)),
                cube([-7.0, -7.0, -4.0], [4.0, 3.0, 4.0], (0, 0)),
                cube([3.0, -7.0, -4.0], [4.0, 3.0, 4.0], (0, 0)),
            ],
        ),
        part(
            "body",
            [0.0, 11.0, 2.0],
            vec![cube([-7.0, -9.0, -8.0], [14.0, 18.0, 16.0], (0, 0))],
        ),
    ];
    p.extend(legs(5.0, 8.0, 17.0, -6.0, 7.0, (0, 0)));
    model(p, 64, 64)
}

pub fn bake_polar_bear_model() -> BakedEntityModel {
    let mut p = vec![
        part(
            "head",
            [0.0, 10.0, -10.0],
            vec![
                cube([-5.0, -4.0, -6.0], [10.0, 8.0, 8.0], (0, 0)),
                cube([-2.0, 0.0, -8.0], [4.0, 3.0, 2.0], (0, 0)),
                cube([-4.0, -6.0, -3.0], [3.0, 3.0, 3.0], (0, 0)),
                cube([1.0, -6.0, -3.0], [3.0, 3.0, 3.0], (0, 0)),
            ],
        ),
        part(
            "body",
            [0.0, 11.0, 2.0],
            vec![cube([-6.0, -9.0, -10.0], [12.0, 18.0, 20.0], (0, 0))],
        ),
    ];
    p.extend(legs(4.0, 10.0, 17.0, -8.0, 8.0, (0, 0)));
    model(p, 128, 64)
}

pub fn bake_ravager_model() -> BakedEntityModel {
    let mut p = vec![
        part(
            "head",
            [0.0, 6.0, -10.0],
            vec![
                cube([-8.0, -7.0, -8.0], [16.0, 14.0, 10.0], (0, 0)),
                cube([-5.0, 2.0, -12.0], [10.0, 5.0, 4.0], (0, 0)),
                cube([-10.0, -2.0, -5.0], [2.0, 8.0, 3.0], (0, 0)),
                cube([8.0, -2.0, -5.0], [2.0, 8.0, 3.0], (0, 0)),
            ],
        ),
        part(
            "body",
            [0.0, 11.0, 2.0],
            vec![cube([-9.0, -10.0, -12.0], [18.0, 20.0, 24.0], (0, 0))],
        ),
    ];
    p.extend(legs(6.0, 12.0, 13.0, -8.0, 8.0, (0, 0)));
    model(p, 128, 128)
}

pub fn bake_sniffer_model() -> BakedEntityModel {
    let mut p = vec![
        part(
            "head",
            [0.0, 11.0, -13.0],
            vec![
                cube([-6.0, -5.0, -10.0], [12.0, 10.0, 11.0], (0, 0)),
                cube([-5.0, -2.0, -17.0], [10.0, 6.0, 7.0], (0, 0)),
            ],
        ),
        part(
            "body",
            [0.0, 12.0, 0.0],
            vec![cube([-11.0, -10.0, -13.0], [22.0, 20.0, 26.0], (0, 0))],
        ),
    ];
    p.extend(legs(8.0, 10.0, 14.0, -9.0, 9.0, (0, 0)));
    model(p, 192, 192)
}

pub fn bake_strider_model() -> BakedEntityModel {
    let mut p = vec![
        part(
            "body",
            [0.0, 18.0, 0.0],
            vec![cube([-8.0, -5.0, -8.0], [16.0, 10.0, 16.0], (0, 0))],
        ),
        part(
            "head",
            [0.0, 16.0, -6.0],
            vec![
                cube([-4.0, -3.0, -7.0], [8.0, 6.0, 7.0], (0, 0)),
                cube([-6.0, -4.0, -4.0], [2.0, 3.0, 2.0], (0, 0)),
                cube([4.0, -4.0, -4.0], [2.0, 3.0, 2.0], (0, 0)),
            ],
        ),
    ];
    p.extend(legs(5.0, 12.0, 20.0, -5.0, 5.0, (0, 0)));
    model(p, 64, 128)
}

pub fn bake_llama_model(trader: bool) -> BakedEntityModel {
    let mut p = vec![
        part(
            "head",
            [0.0, 7.0, -6.0],
            vec![
                cube([-2.0, -3.0, -8.0], [4.0, 4.0, 9.0], (0, 0)),
                cube([-3.0, -6.0, -5.0], [2.0, 3.0, 2.0], (0, 0)),
                cube([1.0, -6.0, -5.0], [2.0, 3.0, 2.0], (0, 0)),
            ],
        ),
        part(
            "body",
            [0.0, 12.0, 0.0],
            vec![cube([-4.0, -9.0, -6.0], [8.0, 18.0, 12.0], (0, 0))],
        ),
    ];
    p.extend(legs(3.0, 14.0, 19.0, -4.0, 4.0, (0, 0)));
    let _ = trader; // Trader status changes texture/carpeting, not body geometry.
    model(p, 128, 64)
}

#[path = "terrestrial_tests.rs"]
#[cfg(test)]
mod tests;
