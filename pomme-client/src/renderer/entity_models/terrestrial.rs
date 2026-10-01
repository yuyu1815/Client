//! Static 26.2 terrestrial model layer bakes, transcribed from mapped client
//! bytecode.
//!
//! Rest geometry only. Dynamic poses/animation and renderer state selection
//! remain integration responsibilities.
use glam::Vec3;

use crate::renderer::entity_model::{BakedEntityModel, EntityPart, ModelCube, bake_model};

pub fn bake_armadillo_model() -> BakedEntityModel {
    let parts = vec![
        EntityPart {
            name: "body".into(),
            offset: Vec3::new(0.0, 21.0, 4.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![
                ModelCube {
                    origin: Vec3::new(-4.0, -7.0, -10.0),
                    size: Vec3::new(8.0, 8.0, 12.0),
                    tex_offset: (0, 20),
                    deformation: 0.3,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(-4.0, -7.0, -10.0),
                    size: Vec3::new(8.0, 8.0, 12.0),
                    tex_offset: (0, 40),
                    deformation: 0.0,
                    mirror: false,
                },
            ],
            parent: None,
        },
        EntityPart {
            name: "tail".into(),
            offset: Vec3::new(0.0, -3.0, 1.0),
            default_rotation: Vec3::new(0.5061, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-0.5, -0.0865, 0.0933),
                size: Vec3::new(1.0, 6.0, 1.0),
                tex_offset: (44, 53),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(0),
        },
        EntityPart {
            name: "head".into(),
            offset: Vec3::new(0.0, -2.0, -11.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![],
            parent: Some(0),
        },
        EntityPart {
            name: "head_cube".into(),
            offset: Vec3::new(0.0, 0.0, 0.0),
            default_rotation: Vec3::new(-0.3927, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.5, -1.0, -1.0),
                size: Vec3::new(3.0, 5.0, 2.0),
                tex_offset: (43, 15),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(2),
        },
        EntityPart {
            name: "right_ear".into(),
            offset: Vec3::new(-1.0, -1.0, 0.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![],
            parent: Some(2),
        },
        EntityPart {
            name: "right_ear_cube".into(),
            offset: Vec3::new(-0.5, 0.0, -0.6),
            default_rotation: Vec3::new(0.1886, -0.3864, -0.0718),
            cubes: vec![ModelCube {
                origin: Vec3::new(-2.0, -3.0, 0.0),
                size: Vec3::new(2.0, 5.0, 0.0),
                tex_offset: (43, 10),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(4),
        },
        EntityPart {
            name: "left_ear".into(),
            offset: Vec3::new(1.0, -2.0, 0.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![],
            parent: Some(2),
        },
        EntityPart {
            name: "left_ear_cube".into(),
            offset: Vec3::new(0.5, 1.0, -0.6),
            default_rotation: Vec3::new(0.1886, 0.3864, 0.0718),
            cubes: vec![ModelCube {
                origin: Vec3::new(0.0, -3.0, 0.0),
                size: Vec3::new(2.0, 5.0, 0.0),
                tex_offset: (47, 10),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(6),
        },
        EntityPart {
            name: "right_hind_leg".into(),
            offset: Vec3::new(-2.0, 21.0, 4.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.0, 0.0, -1.0),
                size: Vec3::new(2.0, 3.0, 2.0),
                tex_offset: (51, 31),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "left_hind_leg".into(),
            offset: Vec3::new(2.0, 21.0, 4.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.0, 0.0, -1.0),
                size: Vec3::new(2.0, 3.0, 2.0),
                tex_offset: (42, 31),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "right_front_leg".into(),
            offset: Vec3::new(-2.0, 21.0, -4.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.0, 0.0, -1.0),
                size: Vec3::new(2.0, 3.0, 2.0),
                tex_offset: (51, 43),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "left_front_leg".into(),
            offset: Vec3::new(2.0, 21.0, -4.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.0, 0.0, -1.0),
                size: Vec3::new(2.0, 3.0, 2.0),
                tex_offset: (42, 43),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "cube".into(),
            offset: Vec3::new(0.0, 24.0, 0.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-5.0, -10.0, -6.0),
                size: Vec3::new(10.0, 10.0, 10.0),
                tex_offset: (0, 0),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
    ];
    let mut model = bake_model(parts, 64, 64);
    model
}

pub fn bake_baby_armadillo_model() -> BakedEntityModel {
    let parts = vec![
        EntityPart {
            name: "body".into(),
            offset: Vec3::new(0.0, 20.0, 0.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![
                ModelCube {
                    origin: Vec3::new(-2.5, -2.0, -3.5),
                    size: Vec3::new(5.0, 4.0, 7.0),
                    tex_offset: (0, 0),
                    deformation: 0.3,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(-2.5, -2.0, -3.0),
                    size: Vec3::new(5.0, 4.0, 6.0),
                    tex_offset: (0, 11),
                    deformation: 0.0,
                    mirror: false,
                },
            ],
            parent: None,
        },
        EntityPart {
            name: "tail".into(),
            offset: Vec3::new(0.0, 0.0, 3.4),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![],
            parent: Some(0),
        },
        EntityPart {
            name: "right_ear_cube".into(),
            offset: Vec3::new(0.0, 1.5, 1.0),
            default_rotation: Vec3::new(-1.0472, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-0.5, -0.5, -2.0),
                size: Vec3::new(1.0, 1.0, 4.0),
                tex_offset: (22, 11),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(1),
        },
        EntityPart {
            name: "head".into(),
            offset: Vec3::new(0.0, 0.0, -3.2),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![],
            parent: Some(0),
        },
        EntityPart {
            name: "head_cube".into(),
            offset: Vec3::new(0.0, 0.0, 0.0),
            default_rotation: Vec3::new(0.7417649, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.0, -2.0, -4.0),
                size: Vec3::new(2.0, 2.0, 4.0),
                tex_offset: (20, 17),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(3),
        },
        EntityPart {
            name: "right_ear".into(),
            offset: Vec3::new(-1.0, -2.0, -0.3),
            default_rotation: Vec3::new(-0.4363, -0.1134, 0.0524),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.8, -2.0, 0.0),
                size: Vec3::new(2.0, 3.0, 0.0),
                tex_offset: (28, 8),
                deformation: 0.0,
                mirror: true,
            }],
            parent: Some(4),
        },
        EntityPart {
            name: "left_ear".into(),
            offset: Vec3::new(1.0, -2.0, -0.3),
            default_rotation: Vec3::new(-0.4363, 0.1134, -0.0524),
            cubes: vec![ModelCube {
                origin: Vec3::new(-0.2, -2.0, 0.0),
                size: Vec3::new(2.0, 3.0, 0.0),
                tex_offset: (28, 8),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(4),
        },
        EntityPart {
            name: "right_hind_leg".into(),
            offset: Vec3::new(-1.5, 22.0, 2.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.0, 0.0, -1.0),
                size: Vec3::new(2.0, 2.0, 2.0),
                tex_offset: (20, 27),
                deformation: 0.0,
                mirror: true,
            }],
            parent: None,
        },
        EntityPart {
            name: "left_hind_leg".into(),
            offset: Vec3::new(1.5, 22.0, 2.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.0, 0.0, -1.0),
                size: Vec3::new(2.0, 2.0, 2.0),
                tex_offset: (20, 27),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "right_front_leg".into(),
            offset: Vec3::new(1.5, 22.0, -1.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.0, 0.0, -1.0),
                size: Vec3::new(2.0, 2.0, 2.0),
                tex_offset: (20, 23),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "left_front_leg".into(),
            offset: Vec3::new(-1.5, 22.0, -1.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.0, 0.0, -1.0),
                size: Vec3::new(2.0, 2.0, 2.0),
                tex_offset: (24, 0),
                deformation: 0.0,
                mirror: true,
            }],
            parent: None,
        },
        EntityPart {
            name: "cube".into(),
            offset: Vec3::new(0.0, 20.7, 0.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-3.0, -3.0, -3.0),
                size: Vec3::new(6.0, 6.0, 6.0),
                tex_offset: (0, 25),
                deformation: 0.3,
                mirror: false,
            }],
            parent: None,
        },
    ];
    let mut model = bake_model(parts, 64, 64);
    model
}

pub fn bake_camel_model() -> BakedEntityModel {
    let parts = vec![
        EntityPart {
            name: "body".into(),
            offset: Vec3::new(0.0, 4.0, 9.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-7.5, -12.0, -23.5),
                size: Vec3::new(15.0, 12.0, 27.0),
                tex_offset: (0, 25),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "hump".into(),
            offset: Vec3::new(0.0, -12.0, -10.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-4.5, -5.0, -5.5),
                size: Vec3::new(9.0, 5.0, 11.0),
                tex_offset: (74, 0),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(0),
        },
        EntityPart {
            name: "tail".into(),
            offset: Vec3::new(0.0, -9.0, 3.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.5, 0.0, 0.0),
                size: Vec3::new(3.0, 14.0, 0.0),
                tex_offset: (122, 0),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(0),
        },
        EntityPart {
            name: "head".into(),
            offset: Vec3::new(0.0, -3.0, -19.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![
                ModelCube {
                    origin: Vec3::new(-3.5, -7.0, -15.0),
                    size: Vec3::new(7.0, 8.0, 19.0),
                    tex_offset: (60, 24),
                    deformation: 0.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(-3.5, -21.0, -15.0),
                    size: Vec3::new(7.0, 14.0, 7.0),
                    tex_offset: (21, 0),
                    deformation: 0.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(-2.5, -21.0, -21.0),
                    size: Vec3::new(5.0, 5.0, 6.0),
                    tex_offset: (50, 0),
                    deformation: 0.0,
                    mirror: false,
                },
            ],
            parent: Some(0),
        },
        EntityPart {
            name: "left_ear".into(),
            offset: Vec3::new(2.5, -21.0, -9.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-0.5, 0.5, -1.0),
                size: Vec3::new(3.0, 1.0, 2.0),
                tex_offset: (45, 0),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(3),
        },
        EntityPart {
            name: "right_ear".into(),
            offset: Vec3::new(-2.5, -21.0, -9.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-2.5, 0.5, -1.0),
                size: Vec3::new(3.0, 1.0, 2.0),
                tex_offset: (67, 0),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(3),
        },
        EntityPart {
            name: "left_hind_leg".into(),
            offset: Vec3::new(4.9, 1.0, 9.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-2.5, 2.0, -2.5),
                size: Vec3::new(5.0, 21.0, 5.0),
                tex_offset: (58, 16),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "right_hind_leg".into(),
            offset: Vec3::new(-4.9, 1.0, 9.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-2.5, 2.0, -2.5),
                size: Vec3::new(5.0, 21.0, 5.0),
                tex_offset: (94, 16),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "left_front_leg".into(),
            offset: Vec3::new(4.9, 1.0, -10.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-2.5, 2.0, -2.5),
                size: Vec3::new(5.0, 21.0, 5.0),
                tex_offset: (0, 0),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "right_front_leg".into(),
            offset: Vec3::new(-4.9, 1.0, -10.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-2.5, 2.0, -2.5),
                size: Vec3::new(5.0, 21.0, 5.0),
                tex_offset: (0, 26),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
    ];
    let mut model = bake_model(parts, 128, 128);
    model
}

pub fn bake_baby_camel_model() -> BakedEntityModel {
    let parts = vec![
        EntityPart {
            name: "body".into(),
            offset: Vec3::new(0.0, 7.0, 0.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-4.5, -4.0, -8.0),
                size: Vec3::new(9.0, 8.0, 16.0),
                tex_offset: (0, 14),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "tail".into(),
            offset: Vec3::new(0.0, -1.5, 8.05),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.5, -0.5, 0.0),
                size: Vec3::new(3.0, 9.0, 0.0),
                tex_offset: (50, 38),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(0),
        },
        EntityPart {
            name: "head".into(),
            offset: Vec3::new(0.0, 1.0, -7.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![
                ModelCube {
                    origin: Vec3::new(-2.5, -3.0, -7.5),
                    size: Vec3::new(5.0, 5.0, 7.0),
                    tex_offset: (20, 0),
                    deformation: 0.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(-2.5, -12.0, -7.5),
                    size: Vec3::new(5.0, 9.0, 5.0),
                    tex_offset: (0, 0),
                    deformation: 0.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(-2.5, -12.0, -10.5),
                    size: Vec3::new(5.0, 4.0, 3.0),
                    tex_offset: (0, 14),
                    deformation: 0.0,
                    mirror: false,
                },
            ],
            parent: Some(0),
        },
        EntityPart {
            name: "right_ear".into(),
            offset: Vec3::new(-2.5, -11.0, -4.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-3.0, -0.5, -1.0),
                size: Vec3::new(3.0, 1.0, 2.0),
                tex_offset: (37, 0),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(2),
        },
        EntityPart {
            name: "left_ear".into(),
            offset: Vec3::new(2.5, -11.0, -4.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(0.0, -0.5, -1.0),
                size: Vec3::new(3.0, 1.0, 2.0),
                tex_offset: (47, 0),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(2),
        },
        EntityPart {
            name: "right_front_leg".into(),
            offset: Vec3::new(-3.0, 11.5, -5.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.5, -0.5, -1.5),
                size: Vec3::new(3.0, 13.0, 3.0),
                tex_offset: (36, 14),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "left_front_leg".into(),
            offset: Vec3::new(3.0, 11.5, -5.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.5, -0.5, -1.5),
                size: Vec3::new(3.0, 13.0, 3.0),
                tex_offset: (48, 14),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "left_hind_leg".into(),
            offset: Vec3::new(3.0, 11.5, 5.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.5, -0.5, -1.5),
                size: Vec3::new(3.0, 13.0, 3.0),
                tex_offset: (12, 38),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "right_hind_leg".into(),
            offset: Vec3::new(-3.0, 11.5, 5.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.5, -0.5, -1.5),
                size: Vec3::new(3.0, 13.0, 3.0),
                tex_offset: (0, 38),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
    ];
    let mut model = bake_model(parts, 64, 64);
    model
}

pub fn bake_fox_model() -> BakedEntityModel {
    let parts = vec![
        EntityPart {
            name: "head".into(),
            offset: Vec3::new(-1.0, 16.5, -3.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-3.0, -2.0, -5.0),
                size: Vec3::new(8.0, 6.0, 6.0),
                tex_offset: (1, 5),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "right_ear".into(),
            offset: Vec3::new(0.0, 0.0, 0.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-3.0, -4.0, -4.0),
                size: Vec3::new(2.0, 2.0, 1.0),
                tex_offset: (8, 1),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(0),
        },
        EntityPart {
            name: "left_ear".into(),
            offset: Vec3::new(0.0, 0.0, 0.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(3.0, -4.0, -4.0),
                size: Vec3::new(2.0, 2.0, 1.0),
                tex_offset: (15, 1),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(0),
        },
        EntityPart {
            name: "nose".into(),
            offset: Vec3::new(0.0, 0.0, 0.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.0, 2.01, -8.0),
                size: Vec3::new(4.0, 2.0, 3.0),
                tex_offset: (6, 18),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(0),
        },
        EntityPart {
            name: "body".into(),
            offset: Vec3::new(0.0, 16.0, -6.0),
            default_rotation: Vec3::new(1.5707964, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-3.0, 3.999, -3.5),
                size: Vec3::new(6.0, 11.0, 6.0),
                tex_offset: (24, 15),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "tail".into(),
            offset: Vec3::new(-4.0, 15.0, -1.0),
            default_rotation: Vec3::new(-0.05235988, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(2.0, 0.0, -1.0),
                size: Vec3::new(4.0, 9.0, 5.0),
                tex_offset: (30, 0),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(4),
        },
        EntityPart {
            name: "right_hind_leg".into(),
            offset: Vec3::new(-5.0, 17.5, 7.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(2.0, 0.5, -1.0),
                size: Vec3::new(2.0, 6.0, 2.0),
                tex_offset: (13, 24),
                deformation: 0.001,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "left_hind_leg".into(),
            offset: Vec3::new(-1.0, 17.5, 7.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(2.0, 0.5, -1.0),
                size: Vec3::new(2.0, 6.0, 2.0),
                tex_offset: (4, 24),
                deformation: 0.001,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "right_front_leg".into(),
            offset: Vec3::new(-5.0, 17.5, 0.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(2.0, 0.5, -1.0),
                size: Vec3::new(2.0, 6.0, 2.0),
                tex_offset: (13, 24),
                deformation: 0.001,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "left_front_leg".into(),
            offset: Vec3::new(-1.0, 17.5, 0.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(2.0, 0.5, -1.0),
                size: Vec3::new(2.0, 6.0, 2.0),
                tex_offset: (4, 24),
                deformation: 0.001,
                mirror: false,
            }],
            parent: None,
        },
    ];
    let mut model = bake_model(parts, 48, 32);
    model
}

pub fn bake_baby_fox_model() -> BakedEntityModel {
    let parts = vec![
        EntityPart {
            name: "head".into(),
            offset: Vec3::new(0.0, 18.125, 0.125),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![
                ModelCube {
                    origin: Vec3::new(-3.0, -2.125, -5.125),
                    size: Vec3::new(6.0, 5.0, 5.0),
                    tex_offset: (0, 0),
                    deformation: 0.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(-1.0, 0.875, -7.125),
                    size: Vec3::new(2.0, 2.0, 2.0),
                    tex_offset: (18, 20),
                    deformation: 0.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(-3.0, -4.125, -4.125),
                    size: Vec3::new(2.0, 2.0, 1.0),
                    tex_offset: (22, 8),
                    deformation: 0.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(1.0, -4.125, -4.125),
                    size: Vec3::new(2.0, 2.0, 1.0),
                    tex_offset: (22, 11),
                    deformation: 0.0,
                    mirror: false,
                },
            ],
            parent: None,
        },
        EntityPart {
            name: "right_hind_leg".into(),
            offset: Vec3::new(-1.5, 22.0, 4.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.0, 0.0, -1.0),
                size: Vec3::new(2.0, 2.0, 2.0),
                tex_offset: (22, 4),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "left_hind_leg".into(),
            offset: Vec3::new(1.5, 22.0, 4.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.0, 0.0, -1.0),
                size: Vec3::new(2.0, 2.0, 2.0),
                tex_offset: (22, 0),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "right_front_leg".into(),
            offset: Vec3::new(-1.5, 22.0, 0.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.0, 0.0, -1.0),
                size: Vec3::new(2.0, 2.0, 2.0),
                tex_offset: (22, 4),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "left_front_leg".into(),
            offset: Vec3::new(1.5, 22.0, 0.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.0, 0.0, -1.0),
                size: Vec3::new(2.0, 2.0, 2.0),
                tex_offset: (22, 0),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "body".into(),
            offset: Vec3::new(0.0, 20.0, 2.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-2.5, -2.0, -3.0),
                size: Vec3::new(5.0, 4.0, 6.0),
                tex_offset: (0, 10),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "tail".into(),
            offset: Vec3::new(0.0, -0.5, 3.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.5, -1.48, -1.0),
                size: Vec3::new(3.0, 3.0, 6.0),
                tex_offset: (0, 20),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(5),
        },
    ];
    let mut model = bake_model(parts, 32, 32);
    model
}

pub fn bake_frog_model() -> BakedEntityModel {
    let parts = vec![
        EntityPart {
            name: "root".into(),
            offset: Vec3::new(0.0, 24.0, 0.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![],
            parent: None,
        },
        EntityPart {
            name: "body".into(),
            offset: Vec3::new(0.0, -2.0, 4.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![
                ModelCube {
                    origin: Vec3::new(-3.5, -2.0, -8.0),
                    size: Vec3::new(7.0, 3.0, 9.0),
                    tex_offset: (3, 1),
                    deformation: 0.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(-3.5, -1.0, -8.0),
                    size: Vec3::new(7.0, 0.0, 9.0),
                    tex_offset: (23, 22),
                    deformation: 0.0,
                    mirror: false,
                },
            ],
            parent: Some(0),
        },
        EntityPart {
            name: "head".into(),
            offset: Vec3::new(0.0, -2.0, -1.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![
                ModelCube {
                    origin: Vec3::new(-3.5, -1.0, -7.0),
                    size: Vec3::new(7.0, 0.0, 9.0),
                    tex_offset: (23, 13),
                    deformation: 0.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(-3.5, -2.0, -7.0),
                    size: Vec3::new(7.0, 3.0, 9.0),
                    tex_offset: (0, 13),
                    deformation: 0.0,
                    mirror: false,
                },
            ],
            parent: Some(1),
        },
        EntityPart {
            name: "eyes".into(),
            offset: Vec3::new(-0.5, 0.0, 2.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![],
            parent: Some(2),
        },
        EntityPart {
            name: "right_eye".into(),
            offset: Vec3::new(-1.5, -3.0, -6.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.5, -1.0, -1.5),
                size: Vec3::new(3.0, 2.0, 3.0),
                tex_offset: (0, 0),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(3),
        },
        EntityPart {
            name: "left_eye".into(),
            offset: Vec3::new(2.5, -3.0, -6.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.5, -1.0, -1.5),
                size: Vec3::new(3.0, 2.0, 3.0),
                tex_offset: (0, 5),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(3),
        },
        EntityPart {
            name: "croaking_body".into(),
            offset: Vec3::new(0.0, -1.0, -5.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-3.5, -0.1, -2.9),
                size: Vec3::new(7.0, 2.0, 3.0),
                tex_offset: (26, 5),
                deformation: -0.1,
                mirror: false,
            }],
            parent: Some(1),
        },
        EntityPart {
            name: "tongue".into(),
            offset: Vec3::new(0.0, -1.01, 1.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-2.0, 0.0, -7.1),
                size: Vec3::new(4.0, 0.0, 7.0),
                tex_offset: (17, 13),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(1),
        },
        EntityPart {
            name: "left_arm".into(),
            offset: Vec3::new(4.0, -1.0, -6.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.0, 0.0, -1.0),
                size: Vec3::new(2.0, 3.0, 3.0),
                tex_offset: (0, 32),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(1),
        },
        EntityPart {
            name: "left_hand".into(),
            offset: Vec3::new(0.0, 3.0, -1.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-4.0, 0.01, -4.0),
                size: Vec3::new(8.0, 0.0, 8.0),
                tex_offset: (18, 40),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(8),
        },
        EntityPart {
            name: "right_arm".into(),
            offset: Vec3::new(-4.0, -1.0, -6.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.0, 0.0, -1.0),
                size: Vec3::new(2.0, 3.0, 3.0),
                tex_offset: (0, 38),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(1),
        },
        EntityPart {
            name: "right_hand".into(),
            offset: Vec3::new(0.0, 3.0, 0.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-4.0, 0.01, -5.0),
                size: Vec3::new(8.0, 0.0, 8.0),
                tex_offset: (2, 40),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(10),
        },
        EntityPart {
            name: "left_leg".into(),
            offset: Vec3::new(3.5, -3.0, 4.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.0, 0.0, -2.0),
                size: Vec3::new(3.0, 3.0, 4.0),
                tex_offset: (14, 25),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(0),
        },
        EntityPart {
            name: "left_foot".into(),
            offset: Vec3::new(2.0, 3.0, 0.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-4.0, 0.01, -4.0),
                size: Vec3::new(8.0, 0.0, 8.0),
                tex_offset: (2, 32),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(12),
        },
        EntityPart {
            name: "right_leg".into(),
            offset: Vec3::new(-3.5, -3.0, 4.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-2.0, 0.0, -2.0),
                size: Vec3::new(3.0, 3.0, 4.0),
                tex_offset: (0, 25),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(0),
        },
        EntityPart {
            name: "right_foot".into(),
            offset: Vec3::new(-2.0, 3.0, 0.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-4.0, 0.01, -4.0),
                size: Vec3::new(8.0, 0.0, 8.0),
                tex_offset: (18, 32),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(14),
        },
    ];
    let mut model = bake_model(parts, 48, 48);
    model
}

pub fn bake_goat_model() -> BakedEntityModel {
    let parts = vec![
        EntityPart {
            name: "head".into(),
            offset: Vec3::new(1.0, 14.0, 0.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![
                ModelCube {
                    origin: Vec3::new(-6.0, -11.0, -10.0),
                    size: Vec3::new(3.0, 2.0, 1.0),
                    tex_offset: (2, 61),
                    deformation: 0.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(2.0, -11.0, -10.0),
                    size: Vec3::new(3.0, 2.0, 1.0),
                    tex_offset: (2, 61),
                    deformation: 0.0,
                    mirror: true,
                },
                ModelCube {
                    origin: Vec3::new(-0.5, -3.0, -14.0),
                    size: Vec3::new(0.0, 7.0, 5.0),
                    tex_offset: (23, 52),
                    deformation: 0.0,
                    mirror: true,
                },
            ],
            parent: None,
        },
        EntityPart {
            name: "left_horn".into(),
            offset: Vec3::new(0.0, 0.0, 0.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-0.01, -16.0, -10.0),
                size: Vec3::new(2.0, 7.0, 2.0),
                tex_offset: (12, 55),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(0),
        },
        EntityPart {
            name: "right_horn".into(),
            offset: Vec3::new(0.0, 0.0, 0.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-2.99, -16.0, -10.0),
                size: Vec3::new(2.0, 7.0, 2.0),
                tex_offset: (12, 55),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(0),
        },
        EntityPart {
            name: "nose".into(),
            offset: Vec3::new(0.0, -8.0, -8.0),
            default_rotation: Vec3::new(0.9599, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-3.0, -4.0, -8.0),
                size: Vec3::new(5.0, 7.0, 10.0),
                tex_offset: (34, 46),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(0),
        },
        EntityPart {
            name: "body".into(),
            offset: Vec3::new(0.0, 24.0, 0.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![
                ModelCube {
                    origin: Vec3::new(-4.0, -17.0, -7.0),
                    size: Vec3::new(9.0, 11.0, 16.0),
                    tex_offset: (1, 1),
                    deformation: 0.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(-5.0, -18.0, -8.0),
                    size: Vec3::new(11.0, 14.0, 11.0),
                    tex_offset: (0, 28),
                    deformation: 0.0,
                    mirror: false,
                },
            ],
            parent: None,
        },
        EntityPart {
            name: "left_hind_leg".into(),
            offset: Vec3::new(1.0, 14.0, 4.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(0.0, 4.0, 0.0),
                size: Vec3::new(3.0, 6.0, 3.0),
                tex_offset: (36, 29),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "right_hind_leg".into(),
            offset: Vec3::new(-3.0, 14.0, 4.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(0.0, 4.0, 0.0),
                size: Vec3::new(3.0, 6.0, 3.0),
                tex_offset: (49, 29),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "left_front_leg".into(),
            offset: Vec3::new(1.0, 14.0, -6.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(0.0, 0.0, 0.0),
                size: Vec3::new(3.0, 10.0, 3.0),
                tex_offset: (49, 2),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "right_front_leg".into(),
            offset: Vec3::new(-3.0, 14.0, -6.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(0.0, 0.0, 0.0),
                size: Vec3::new(3.0, 10.0, 3.0),
                tex_offset: (35, 2),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
    ];
    let mut model = bake_model(parts, 64, 64);
    model
}

pub fn bake_baby_goat_model() -> BakedEntityModel {
    let parts = vec![
        EntityPart {
            name: "left_hind_leg".into(),
            offset: Vec3::new(1.5, 19.5, 3.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.0, -0.5, -1.0),
                size: Vec3::new(2.0, 5.0, 2.0),
                tex_offset: (29, 12),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "right_hind_leg".into(),
            offset: Vec3::new(-1.5, 19.5, 3.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.0, -0.5, -1.0),
                size: Vec3::new(2.0, 5.0, 2.0),
                tex_offset: (21, 12),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "right_front_leg".into(),
            offset: Vec3::new(-1.5, 19.5, -2.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.0, -0.5, -1.0),
                size: Vec3::new(2.0, 5.0, 2.0),
                tex_offset: (21, 5),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "left_front_leg".into(),
            offset: Vec3::new(1.5, 19.5, -2.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.0, -0.5, -1.0),
                size: Vec3::new(2.0, 5.0, 2.0),
                tex_offset: (29, 5),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "body".into(),
            offset: Vec3::new(0.0, 17.8, 0.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![
                ModelCube {
                    origin: Vec3::new(-3.0, -2.3, -4.5),
                    size: Vec3::new(6.0, 5.0, 9.0),
                    tex_offset: (0, 10),
                    deformation: 0.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(-2.5, -2.2, -4.0),
                    size: Vec3::new(5.0, 4.0, 8.0),
                    tex_offset: (0, 24),
                    deformation: 0.0,
                    mirror: false,
                },
            ],
            parent: None,
        },
        EntityPart {
            name: "head".into(),
            offset: Vec3::new(0.0, 15.5, -3.0),
            default_rotation: Vec3::new(0.4363, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-2.0, -3.8126, -5.1548),
                size: Vec3::new(4.0, 4.0, 6.0),
                tex_offset: (0, 0),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "right_horn".into(),
            offset: Vec3::new(-1.5, -1.5, -1.0),
            default_rotation: Vec3::new(-0.3926991, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(0.0, -4.5, 0.0),
                size: Vec3::new(1.0, 2.0, 1.0),
                tex_offset: (24, 0),
                deformation: 0.0,
                mirror: true,
            }],
            parent: Some(5),
        },
        EntityPart {
            name: "left_horn".into(),
            offset: Vec3::new(-1.5, -1.5, -1.0),
            default_rotation: Vec3::new(-0.3926991, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(2.0, -4.5, 0.0),
                size: Vec3::new(1.0, 2.0, 1.0),
                tex_offset: (24, 0),
                deformation: 0.0,
                mirror: true,
            }],
            parent: Some(5),
        },
        EntityPart {
            name: "right_ear".into(),
            offset: Vec3::new(-1.7, -2.3126, 0.1452),
            default_rotation: Vec3::new(0.0, -0.5236, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-2.0, -0.5, -0.5),
                size: Vec3::new(2.0, 1.0, 1.0),
                tex_offset: (0, 12),
                deformation: 0.0,
                mirror: true,
            }],
            parent: Some(5),
        },
        EntityPart {
            name: "left_ear".into(),
            offset: Vec3::new(1.7, -2.3126, 0.1452),
            default_rotation: Vec3::new(0.0, 0.5236, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(0.0, -0.5, -0.5),
                size: Vec3::new(2.0, 1.0, 1.0),
                tex_offset: (0, 12),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(5),
        },
        EntityPart {
            name: "HeadMain".into(),
            offset: Vec3::new(0.0, -1.3126, -1.1548),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-2.0, -2.5, -4.0),
                size: Vec3::new(4.0, 4.0, 6.0),
                tex_offset: (0, 0),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(5),
        },
    ];
    let mut model = bake_model(parts, 64, 64);
    model
}

pub fn bake_hoglin_model() -> BakedEntityModel {
    let parts = vec![
        EntityPart {
            name: "body".into(),
            offset: Vec3::new(0.0, 7.0, 0.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-8.0, -7.0, -13.0),
                size: Vec3::new(16.0, 14.0, 26.0),
                tex_offset: (1, 1),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "mane".into(),
            offset: Vec3::new(0.0, -14.0, -7.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(0.0, 0.0, -9.0),
                size: Vec3::new(0.0, 10.0, 19.0),
                tex_offset: (90, 33),
                deformation: 0.001,
                mirror: false,
            }],
            parent: Some(0),
        },
        EntityPart {
            name: "head".into(),
            offset: Vec3::new(0.0, 2.0, -12.0),
            default_rotation: Vec3::new(0.87266463, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-7.0, -3.0, -19.0),
                size: Vec3::new(14.0, 6.0, 19.0),
                tex_offset: (61, 1),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "right_ear".into(),
            offset: Vec3::new(-6.0, -2.0, -3.0),
            default_rotation: Vec3::new(0.0, 0.0, -0.6981317),
            cubes: vec![ModelCube {
                origin: Vec3::new(-6.0, -1.0, -2.0),
                size: Vec3::new(6.0, 1.0, 4.0),
                tex_offset: (1, 1),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(2),
        },
        EntityPart {
            name: "left_ear".into(),
            offset: Vec3::new(6.0, -2.0, -3.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.6981317),
            cubes: vec![ModelCube {
                origin: Vec3::new(0.0, -1.0, -2.0),
                size: Vec3::new(6.0, 1.0, 4.0),
                tex_offset: (1, 6),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(2),
        },
        EntityPart {
            name: "right_horn".into(),
            offset: Vec3::new(-7.0, 2.0, -12.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.0, -11.0, -1.0),
                size: Vec3::new(2.0, 11.0, 2.0),
                tex_offset: (10, 13),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(2),
        },
        EntityPart {
            name: "left_horn".into(),
            offset: Vec3::new(7.0, 2.0, -12.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.0, -11.0, -1.0),
                size: Vec3::new(2.0, 11.0, 2.0),
                tex_offset: (1, 13),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(2),
        },
        EntityPart {
            name: "right_front_leg".into(),
            offset: Vec3::new(-4.0, 10.0, -8.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-3.0, 0.0, -3.0),
                size: Vec3::new(6.0, 14.0, 6.0),
                tex_offset: (66, 42),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "left_front_leg".into(),
            offset: Vec3::new(4.0, 10.0, -8.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-3.0, 0.0, -3.0),
                size: Vec3::new(6.0, 14.0, 6.0),
                tex_offset: (41, 42),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "right_hind_leg".into(),
            offset: Vec3::new(-5.0, 13.0, 10.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-2.5, 0.0, -2.5),
                size: Vec3::new(5.0, 11.0, 5.0),
                tex_offset: (21, 45),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "left_hind_leg".into(),
            offset: Vec3::new(5.0, 13.0, 10.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-2.5, 0.0, -2.5),
                size: Vec3::new(5.0, 11.0, 5.0),
                tex_offset: (0, 45),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
    ];
    let mut model = bake_model(parts, 128, 64);
    model
}

pub fn bake_baby_hoglin_model() -> BakedEntityModel {
    let parts = vec![
        EntityPart {
            name: "head".into(),
            offset: Vec3::new(0.0, 13.0, -7.0),
            default_rotation: Vec3::new(0.8727, 0.0, 0.0),
            cubes: vec![
                ModelCube {
                    origin: Vec3::new(-5.0, -2.2605, -10.547),
                    size: Vec3::new(10.0, 4.0, 12.0),
                    tex_offset: (0, 0),
                    deformation: 0.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(-7.0, -4.0981, -8.4879),
                    size: Vec3::new(2.0, 5.0, 2.0),
                    tex_offset: (44, 29),
                    deformation: 0.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(5.0, -4.0981, -8.4879),
                    size: Vec3::new(2.0, 5.0, 2.0),
                    tex_offset: (52, 29),
                    deformation: 0.0,
                    mirror: false,
                },
            ],
            parent: None,
        },
        EntityPart {
            name: "right_ear".into(),
            offset: Vec3::new(-5.0, -1.0, -1.5),
            default_rotation: Vec3::new(0.0, 0.0, -0.8727),
            cubes: vec![ModelCube {
                origin: Vec3::new(-5.1, -0.5, -2.0),
                size: Vec3::new(6.0, 1.0, 4.0),
                tex_offset: (32, 5),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(0),
        },
        EntityPart {
            name: "left_ear".into(),
            offset: Vec3::new(5.0, -1.0, -1.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.8727),
            cubes: vec![ModelCube {
                origin: Vec3::new(-0.9, -0.5, -2.0),
                size: Vec3::new(6.0, 1.0, 4.0),
                tex_offset: (32, 0),
                deformation: 0.0,
                mirror: true,
            }],
            parent: Some(0),
        },
        EntityPart {
            name: "body".into(),
            offset: Vec3::new(0.0, 24.0, 0.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![
                ModelCube {
                    origin: Vec3::new(-4.0, -14.0, -7.0),
                    size: Vec3::new(8.0, 8.0, 14.0),
                    tex_offset: (0, 16),
                    deformation: 0.02,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(0.0, -18.0, -8.0),
                    size: Vec3::new(0.0, 6.0, 11.0),
                    tex_offset: (24, 39),
                    deformation: 0.02,
                    mirror: false,
                },
            ],
            parent: None,
        },
        EntityPart {
            name: "right_hind_leg".into(),
            offset: Vec3::new(-2.5, 18.0, 4.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.5, 0.0, -1.5),
                size: Vec3::new(3.0, 6.0, 3.0),
                tex_offset: (0, 47),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "left_hind_leg".into(),
            offset: Vec3::new(2.5, 18.0, 4.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.5, 0.0, -1.5),
                size: Vec3::new(3.0, 6.0, 3.0),
                tex_offset: (12, 47),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "right_front_leg".into(),
            offset: Vec3::new(-2.5, 18.0, -4.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.5, 0.0, -1.5),
                size: Vec3::new(3.0, 6.0, 3.0),
                tex_offset: (0, 38),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "left_front_leg".into(),
            offset: Vec3::new(2.5, 18.0, -4.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.5, 0.0, -1.5),
                size: Vec3::new(3.0, 6.0, 3.0),
                tex_offset: (12, 38),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
    ];
    let mut model = bake_model(parts, 64, 64);
    model
}

pub fn bake_panda_model() -> BakedEntityModel {
    let parts = vec![
        EntityPart {
            name: "head".into(),
            offset: Vec3::new(0.0, 11.5, -17.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![
                ModelCube {
                    origin: Vec3::new(-6.5, -5.0, -4.0),
                    size: Vec3::new(13.0, 10.0, 9.0),
                    tex_offset: (0, 6),
                    deformation: 0.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(-3.5, 0.0, -6.0),
                    size: Vec3::new(7.0, 5.0, 2.0),
                    tex_offset: (45, 16),
                    deformation: 0.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(3.5, -8.0, -1.0),
                    size: Vec3::new(5.0, 4.0, 1.0),
                    tex_offset: (52, 25),
                    deformation: 0.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(-8.5, -8.0, -1.0),
                    size: Vec3::new(5.0, 4.0, 1.0),
                    tex_offset: (52, 25),
                    deformation: 0.0,
                    mirror: false,
                },
            ],
            parent: None,
        },
        EntityPart {
            name: "body".into(),
            offset: Vec3::new(0.0, 10.0, 0.0),
            default_rotation: Vec3::new(1.5707964, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-9.5, -13.0, -6.5),
                size: Vec3::new(19.0, 26.0, 13.0),
                tex_offset: (0, 25),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "right_hind_leg".into(),
            offset: Vec3::new(-5.5, 15.0, 9.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-3.0, 0.0, -3.0),
                size: Vec3::new(6.0, 9.0, 6.0),
                tex_offset: (40, 0),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "left_hind_leg".into(),
            offset: Vec3::new(5.5, 15.0, 9.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-3.0, 0.0, -3.0),
                size: Vec3::new(6.0, 9.0, 6.0),
                tex_offset: (40, 0),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "right_front_leg".into(),
            offset: Vec3::new(-5.5, 15.0, -9.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-3.0, 0.0, -3.0),
                size: Vec3::new(6.0, 9.0, 6.0),
                tex_offset: (40, 0),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "left_front_leg".into(),
            offset: Vec3::new(5.5, 15.0, -9.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-3.0, 0.0, -3.0),
                size: Vec3::new(6.0, 9.0, 6.0),
                tex_offset: (40, 0),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
    ];
    let mut model = bake_model(parts, 64, 64);
    model
}

pub fn bake_baby_panda_model() -> BakedEntityModel {
    let parts = vec![
        EntityPart {
            name: "body".into(),
            offset: Vec3::new(0.0, 18.5, 2.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-4.5, -3.5, -5.5),
                size: Vec3::new(9.0, 7.0, 11.0),
                tex_offset: (0, 11),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "head".into(),
            offset: Vec3::new(0.0, 19.0, -3.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![
                ModelCube {
                    origin: Vec3::new(-3.5, -3.0, -5.0),
                    size: Vec3::new(7.0, 6.0, 5.0),
                    tex_offset: (0, 0),
                    deformation: 0.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(-2.0, 1.0, -6.0),
                    size: Vec3::new(4.0, 2.0, 1.0),
                    tex_offset: (24, 6),
                    deformation: 0.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(-4.5, -4.0, -3.5),
                    size: Vec3::new(3.0, 3.0, 1.0),
                    tex_offset: (24, 0),
                    deformation: 0.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(1.5, -4.0, -3.5),
                    size: Vec3::new(3.0, 3.0, 1.0),
                    tex_offset: (33, 0),
                    deformation: 0.0,
                    mirror: false,
                },
            ],
            parent: None,
        },
        EntityPart {
            name: "right_hind_leg".into(),
            offset: Vec3::new(-3.0, 22.0, 6.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.5, 0.0, -1.5),
                size: Vec3::new(3.0, 2.0, 3.0),
                tex_offset: (0, 34),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "left_hind_leg".into(),
            offset: Vec3::new(3.0, 22.0, 6.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.5, 0.0, -1.5),
                size: Vec3::new(3.0, 2.0, 3.0),
                tex_offset: (12, 34),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "right_front_leg".into(),
            offset: Vec3::new(-3.0, 22.0, -1.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.5, 0.0, -1.5),
                size: Vec3::new(3.0, 2.0, 3.0),
                tex_offset: (0, 29),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "left_front_leg".into(),
            offset: Vec3::new(3.0, 22.0, -1.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.5, 0.0, -1.5),
                size: Vec3::new(3.0, 2.0, 3.0),
                tex_offset: (12, 29),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
    ];
    let mut model = bake_model(parts, 64, 64);
    model
}

pub fn bake_polar_bear_model() -> BakedEntityModel {
    let parts = vec![
        EntityPart {
            name: "head".into(),
            offset: Vec3::new(0.0, 10.0, -16.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![
                ModelCube {
                    origin: Vec3::new(-3.5, -3.0, -3.0),
                    size: Vec3::new(7.0, 7.0, 7.0),
                    tex_offset: (0, 0),
                    deformation: 0.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(-2.5, 1.0, -6.0),
                    size: Vec3::new(5.0, 3.0, 3.0),
                    tex_offset: (0, 44),
                    deformation: 0.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(-4.5, -4.0, -1.0),
                    size: Vec3::new(2.0, 2.0, 1.0),
                    tex_offset: (26, 0),
                    deformation: 0.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(2.5, -4.0, -1.0),
                    size: Vec3::new(2.0, 2.0, 1.0),
                    tex_offset: (26, 0),
                    deformation: 0.0,
                    mirror: true,
                },
            ],
            parent: None,
        },
        EntityPart {
            name: "body".into(),
            offset: Vec3::new(-2.0, 9.0, 12.0),
            default_rotation: Vec3::new(1.5707964, 0.0, 0.0),
            cubes: vec![
                ModelCube {
                    origin: Vec3::new(-5.0, -13.0, -7.0),
                    size: Vec3::new(14.0, 14.0, 11.0),
                    tex_offset: (0, 19),
                    deformation: 0.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(-4.0, -25.0, -7.0),
                    size: Vec3::new(12.0, 12.0, 10.0),
                    tex_offset: (39, 0),
                    deformation: 0.0,
                    mirror: false,
                },
            ],
            parent: None,
        },
        EntityPart {
            name: "right_hind_leg".into(),
            offset: Vec3::new(-4.5, 14.0, 6.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-2.0, 0.0, -2.0),
                size: Vec3::new(4.0, 10.0, 8.0),
                tex_offset: (50, 22),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "left_hind_leg".into(),
            offset: Vec3::new(4.5, 14.0, 6.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-2.0, 0.0, -2.0),
                size: Vec3::new(4.0, 10.0, 8.0),
                tex_offset: (50, 22),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "right_front_leg".into(),
            offset: Vec3::new(-3.5, 14.0, -8.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-2.0, 0.0, -2.0),
                size: Vec3::new(4.0, 10.0, 6.0),
                tex_offset: (50, 40),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "left_front_leg".into(),
            offset: Vec3::new(3.5, 14.0, -8.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-2.0, 0.0, -2.0),
                size: Vec3::new(4.0, 10.0, 6.0),
                tex_offset: (50, 40),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
    ];
    let mut model = bake_model(parts, 128, 64);
    for p in &mut model.parts {
        p.offset *= 1.2;
        if p.parent.is_none() {
            p.offset.y -= 4.8032;
        }
        for c in &mut p.cubes {
            c.origin *= 1.2;
            c.size *= 1.2;
            c.deformation *= 1.2;
        }
    }
    model
}

pub fn bake_baby_polar_bear_model() -> BakedEntityModel {
    let parts = vec![
        EntityPart {
            name: "body".into(),
            offset: Vec3::new(0.0, 17.5, 0.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-4.0, -3.5, -6.0),
                size: Vec3::new(8.0, 7.0, 12.0),
                tex_offset: (0, 9),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "head".into(),
            offset: Vec3::new(0.0, 18.625, -5.75),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![
                ModelCube {
                    origin: Vec3::new(-3.0, -2.625, -4.25),
                    size: Vec3::new(6.0, 5.0, 4.0),
                    tex_offset: (0, 0),
                    deformation: 0.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(-2.0, 0.375, -6.25),
                    size: Vec3::new(4.0, 2.0, 2.0),
                    tex_offset: (20, 3),
                    deformation: 0.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(-4.0, -3.625, -2.75),
                    size: Vec3::new(2.0, 2.0, 1.0),
                    tex_offset: (20, 0),
                    deformation: 0.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(2.0, -3.625, -2.75),
                    size: Vec3::new(2.0, 2.0, 1.0),
                    tex_offset: (26, 0),
                    deformation: 0.0,
                    mirror: false,
                },
            ],
            parent: None,
        },
        EntityPart {
            name: "right_hind_leg".into(),
            offset: Vec3::new(-2.5, 21.5, 4.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.5, -0.5, -1.5),
                size: Vec3::new(3.0, 3.0, 3.0),
                tex_offset: (0, 34),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "left_hind_leg".into(),
            offset: Vec3::new(2.5, 21.5, 4.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.5, -0.5, -1.5),
                size: Vec3::new(3.0, 3.0, 3.0),
                tex_offset: (12, 34),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "right_front_leg".into(),
            offset: Vec3::new(-2.5, 21.5, -4.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.5, -0.5, -1.5),
                size: Vec3::new(3.0, 3.0, 3.0),
                tex_offset: (0, 28),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "left_front_leg".into(),
            offset: Vec3::new(2.5, 21.5, -4.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.5, -0.5, -1.5),
                size: Vec3::new(3.0, 3.0, 3.0),
                tex_offset: (12, 28),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
    ];
    let mut model = bake_model(parts, 64, 64);
    model
}

pub fn bake_ravager_model() -> BakedEntityModel {
    let parts = vec![
        EntityPart {
            name: "neck".into(),
            offset: Vec3::new(0.0, -7.0, 5.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-5.0, -1.0, -18.0),
                size: Vec3::new(10.0, 10.0, 18.0),
                tex_offset: (68, 73),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "head".into(),
            offset: Vec3::new(0.0, 16.0, -17.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![
                ModelCube {
                    origin: Vec3::new(-8.0, -20.0, -14.0),
                    size: Vec3::new(16.0, 20.0, 16.0),
                    tex_offset: (0, 0),
                    deformation: 0.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(-2.0, -6.0, -18.0),
                    size: Vec3::new(4.0, 8.0, 4.0),
                    tex_offset: (0, 0),
                    deformation: 0.0,
                    mirror: false,
                },
            ],
            parent: Some(0),
        },
        EntityPart {
            name: "right_horn".into(),
            offset: Vec3::new(-10.0, -14.0, -8.0),
            default_rotation: Vec3::new(1.0995574, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(0.0, -14.0, -2.0),
                size: Vec3::new(2.0, 14.0, 4.0),
                tex_offset: (74, 55),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(1),
        },
        EntityPart {
            name: "left_horn".into(),
            offset: Vec3::new(8.0, -14.0, -8.0),
            default_rotation: Vec3::new(1.0995574, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(0.0, -14.0, -2.0),
                size: Vec3::new(2.0, 14.0, 4.0),
                tex_offset: (74, 55),
                deformation: 0.0,
                mirror: true,
            }],
            parent: Some(1),
        },
        EntityPart {
            name: "mouth".into(),
            offset: Vec3::new(0.0, -2.0, 2.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-8.0, 0.0, -16.0),
                size: Vec3::new(16.0, 3.0, 16.0),
                tex_offset: (0, 36),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(1),
        },
        EntityPart {
            name: "body".into(),
            offset: Vec3::new(0.0, 1.0, 2.0),
            default_rotation: Vec3::new(1.5707964, 0.0, 0.0),
            cubes: vec![
                ModelCube {
                    origin: Vec3::new(-7.0, -10.0, -7.0),
                    size: Vec3::new(14.0, 16.0, 20.0),
                    tex_offset: (0, 55),
                    deformation: 0.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(-6.0, 6.0, -7.0),
                    size: Vec3::new(12.0, 13.0, 18.0),
                    tex_offset: (0, 91),
                    deformation: 0.0,
                    mirror: false,
                },
            ],
            parent: None,
        },
        EntityPart {
            name: "right_hind_leg".into(),
            offset: Vec3::new(-8.0, -13.0, 18.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-4.0, 0.0, -4.0),
                size: Vec3::new(8.0, 37.0, 8.0),
                tex_offset: (96, 0),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "left_hind_leg".into(),
            offset: Vec3::new(8.0, -13.0, 18.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-4.0, 0.0, -4.0),
                size: Vec3::new(8.0, 37.0, 8.0),
                tex_offset: (96, 0),
                deformation: 0.0,
                mirror: true,
            }],
            parent: None,
        },
        EntityPart {
            name: "right_front_leg".into(),
            offset: Vec3::new(-8.0, -13.0, -5.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-4.0, 0.0, -4.0),
                size: Vec3::new(8.0, 37.0, 8.0),
                tex_offset: (64, 0),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "left_front_leg".into(),
            offset: Vec3::new(8.0, -13.0, -5.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-4.0, 0.0, -4.0),
                size: Vec3::new(8.0, 37.0, 8.0),
                tex_offset: (64, 0),
                deformation: 0.0,
                mirror: true,
            }],
            parent: None,
        },
    ];
    let mut model = bake_model(parts, 128, 128);
    model
}

pub fn bake_sniffer_model() -> BakedEntityModel {
    let parts = vec![
        EntityPart {
            name: "bone".into(),
            offset: Vec3::new(0.0, 5.0, 0.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![],
            parent: None,
        },
        EntityPart {
            name: "body".into(),
            offset: Vec3::new(0.0, 0.0, 0.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![
                ModelCube {
                    origin: Vec3::new(-12.5, -14.0, -20.0),
                    size: Vec3::new(25.0, 29.0, 40.0),
                    tex_offset: (62, 68),
                    deformation: 0.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(-12.5, -14.0, -20.0),
                    size: Vec3::new(25.0, 24.0, 40.0),
                    tex_offset: (62, 0),
                    deformation: 0.5,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(-12.5, 12.0, -20.0),
                    size: Vec3::new(25.0, 0.0, 40.0),
                    tex_offset: (87, 68),
                    deformation: 0.0,
                    mirror: false,
                },
            ],
            parent: Some(0),
        },
        EntityPart {
            name: "head".into(),
            offset: Vec3::new(0.0, 6.5, -19.48),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![
                ModelCube {
                    origin: Vec3::new(-6.5, -7.5, -11.5),
                    size: Vec3::new(13.0, 18.0, 11.0),
                    tex_offset: (8, 15),
                    deformation: 0.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(-6.5, 7.5, -11.5),
                    size: Vec3::new(13.0, 0.0, 11.0),
                    tex_offset: (8, 4),
                    deformation: 0.0,
                    mirror: false,
                },
            ],
            parent: Some(1),
        },
        EntityPart {
            name: "left_ear".into(),
            offset: Vec3::new(6.51, -7.5, -4.51),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(0.0, 0.0, -3.0),
                size: Vec3::new(1.0, 19.0, 7.0),
                tex_offset: (2, 0),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(2),
        },
        EntityPart {
            name: "right_ear".into(),
            offset: Vec3::new(-6.51, -7.5, -4.51),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.0, 0.0, -3.0),
                size: Vec3::new(1.0, 19.0, 7.0),
                tex_offset: (48, 0),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(2),
        },
        EntityPart {
            name: "nose".into(),
            offset: Vec3::new(0.0, -4.5, -11.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-6.5, -2.0, -9.0),
                size: Vec3::new(13.0, 2.0, 9.0),
                tex_offset: (10, 45),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(2),
        },
        EntityPart {
            name: "lower_beak".into(),
            offset: Vec3::new(0.0, 2.5, -12.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-6.5, -7.0, -8.0),
                size: Vec3::new(13.0, 12.0, 9.0),
                tex_offset: (10, 57),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(2),
        },
        EntityPart {
            name: "right_front_leg".into(),
            offset: Vec3::new(-7.5, 10.0, -15.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-3.5, -1.0, -4.0),
                size: Vec3::new(7.0, 10.0, 8.0),
                tex_offset: (32, 87),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(0),
        },
        EntityPart {
            name: "right_mid_leg".into(),
            offset: Vec3::new(-7.5, 10.0, 0.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-3.5, -1.0, -4.0),
                size: Vec3::new(7.0, 10.0, 8.0),
                tex_offset: (32, 105),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(0),
        },
        EntityPart {
            name: "right_hind_leg".into(),
            offset: Vec3::new(-7.5, 10.0, 15.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-3.5, -1.0, -4.0),
                size: Vec3::new(7.0, 10.0, 8.0),
                tex_offset: (32, 123),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(0),
        },
        EntityPart {
            name: "left_front_leg".into(),
            offset: Vec3::new(7.5, 10.0, -15.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-3.5, -1.0, -4.0),
                size: Vec3::new(7.0, 10.0, 8.0),
                tex_offset: (0, 87),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(0),
        },
        EntityPart {
            name: "left_mid_leg".into(),
            offset: Vec3::new(7.5, 10.0, 0.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-3.5, -1.0, -4.0),
                size: Vec3::new(7.0, 10.0, 8.0),
                tex_offset: (0, 105),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(0),
        },
        EntityPart {
            name: "left_hind_leg".into(),
            offset: Vec3::new(7.5, 10.0, 15.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-3.5, -1.0, -4.0),
                size: Vec3::new(7.0, 10.0, 8.0),
                tex_offset: (0, 123),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(0),
        },
    ];
    let mut model = bake_model(parts, 192, 192);
    model
}

pub fn bake_baby_sniffer_model() -> BakedEntityModel {
    let parts = vec![
        EntityPart {
            name: "bone".into(),
            offset: Vec3::new(0.0, 24.0, 0.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![],
            parent: None,
        },
        EntityPart {
            name: "body".into(),
            offset: Vec3::new(6.0, -3.0, -9.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![
                ModelCube {
                    origin: Vec3::new(-13.0, -14.0, -0.5),
                    size: Vec3::new(14.0, 14.0, 20.0),
                    tex_offset: (0, 35),
                    deformation: 0.25,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(-13.0, -14.0, -0.5),
                    size: Vec3::new(14.0, 15.0, 20.0),
                    tex_offset: (0, 0),
                    deformation: 0.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(-13.0, 0.0, -0.5),
                    size: Vec3::new(14.0, 0.0, 20.0),
                    tex_offset: (68, 0),
                    deformation: 0.0,
                    mirror: false,
                },
            ],
            parent: Some(0),
        },
        EntityPart {
            name: "head".into(),
            offset: Vec3::new(-6.0, -4.75, 0.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![
                ModelCube {
                    origin: Vec3::new(-5.0, -4.25, -7.5),
                    size: Vec3::new(10.0, 9.0, 9.0),
                    tex_offset: (68, 20),
                    deformation: 0.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(-5.0, 3.75, -7.5),
                    size: Vec3::new(10.0, 0.0, 9.0),
                    tex_offset: (88, 20),
                    deformation: 0.0,
                    mirror: false,
                },
            ],
            parent: Some(1),
        },
        EntityPart {
            name: "left_ear".into(),
            offset: Vec3::new(5.0, -4.25, -1.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(0.0, 0.0, -2.0),
                size: Vec3::new(1.0, 11.0, 3.0),
                tex_offset: (104, 38),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(2),
        },
        EntityPart {
            name: "right_ear".into(),
            offset: Vec3::new(-5.0, -4.25, -1.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.0, 0.0, -2.0),
                size: Vec3::new(1.0, 11.0, 3.0),
                tex_offset: (96, 38),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(2),
        },
        EntityPart {
            name: "nose".into(),
            offset: Vec3::new(0.0, -1.25, -9.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-5.0, -3.0, -2.0),
                size: Vec3::new(10.0, 3.0, 4.0),
                tex_offset: (68, 47),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(2),
        },
        EntityPart {
            name: "lower_beak".into(),
            offset: Vec3::new(0.0, 1.25, -9.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-5.0, -2.5, -2.0),
                size: Vec3::new(10.0, 5.0, 4.0),
                tex_offset: (68, 38),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(2),
        },
        EntityPart {
            name: "right_front_leg".into(),
            offset: Vec3::new(-4.0, -4.0, -7.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-2.0, -1.0, -2.0),
                size: Vec3::new(4.0, 5.0, 4.0),
                tex_offset: (0, 69),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(0),
        },
        EntityPart {
            name: "right_mid_leg".into(),
            offset: Vec3::new(-4.0, -4.0, 0.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-2.0, -1.0, -2.0),
                size: Vec3::new(4.0, 5.0, 4.0),
                tex_offset: (0, 78),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(0),
        },
        EntityPart {
            name: "right_hind_leg".into(),
            offset: Vec3::new(-4.0, -4.0, 7.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-2.0, -1.0, -2.0),
                size: Vec3::new(4.0, 5.0, 4.0),
                tex_offset: (0, 87),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(0),
        },
        EntityPart {
            name: "left_front_leg".into(),
            offset: Vec3::new(4.0, -4.0, -7.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-2.0, -1.0, -2.0),
                size: Vec3::new(4.0, 5.0, 4.0),
                tex_offset: (16, 69),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(0),
        },
        EntityPart {
            name: "left_mid_leg".into(),
            offset: Vec3::new(4.0, -4.0, 0.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-2.0, -1.0, -2.0),
                size: Vec3::new(4.0, 5.0, 4.0),
                tex_offset: (16, 78),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(0),
        },
        EntityPart {
            name: "left_hind_leg".into(),
            offset: Vec3::new(4.0, -4.0, 7.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-2.0, -1.0, -2.0),
                size: Vec3::new(4.0, 5.0, 4.0),
                tex_offset: (16, 87),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(0),
        },
    ];
    let mut model = bake_model(parts, 128, 128);
    model
}

pub fn bake_strider_model() -> BakedEntityModel {
    let parts = vec![
        EntityPart {
            name: "right_leg".into(),
            offset: Vec3::new(-4.0, 8.0, 0.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-2.0, 0.0, -2.0),
                size: Vec3::new(4.0, 16.0, 4.0),
                tex_offset: (0, 32),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "left_leg".into(),
            offset: Vec3::new(4.0, 8.0, 0.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-2.0, 0.0, -2.0),
                size: Vec3::new(4.0, 16.0, 4.0),
                tex_offset: (0, 55),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "body".into(),
            offset: Vec3::new(0.0, 1.0, 0.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-8.0, -6.0, -8.0),
                size: Vec3::new(16.0, 14.0, 16.0),
                tex_offset: (0, 0),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "right_bottom_bristle".into(),
            offset: Vec3::new(-8.0, 4.0, -8.0),
            default_rotation: Vec3::new(0.0, 0.0, -1.2217305),
            cubes: vec![ModelCube {
                origin: Vec3::new(-12.0, 0.0, 0.0),
                size: Vec3::new(12.0, 0.0, 16.0),
                tex_offset: (16, 65),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(2),
        },
        EntityPart {
            name: "right_middle_bristle".into(),
            offset: Vec3::new(-8.0, -1.0, -8.0),
            default_rotation: Vec3::new(0.0, 0.0, -1.134464),
            cubes: vec![ModelCube {
                origin: Vec3::new(-12.0, 0.0, 0.0),
                size: Vec3::new(12.0, 0.0, 16.0),
                tex_offset: (16, 49),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(2),
        },
        EntityPart {
            name: "right_top_bristle".into(),
            offset: Vec3::new(-8.0, -5.0, -8.0),
            default_rotation: Vec3::new(0.0, 0.0, -0.87266463),
            cubes: vec![ModelCube {
                origin: Vec3::new(-12.0, 0.0, 0.0),
                size: Vec3::new(12.0, 0.0, 16.0),
                tex_offset: (16, 33),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(2),
        },
        EntityPart {
            name: "left_top_bristle".into(),
            offset: Vec3::new(8.0, -6.0, -8.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.87266463),
            cubes: vec![ModelCube {
                origin: Vec3::new(0.0, 0.0, 0.0),
                size: Vec3::new(12.0, 0.0, 16.0),
                tex_offset: (16, 33),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(2),
        },
        EntityPart {
            name: "left_middle_bristle".into(),
            offset: Vec3::new(8.0, -2.0, -8.0),
            default_rotation: Vec3::new(0.0, 0.0, 1.134464),
            cubes: vec![ModelCube {
                origin: Vec3::new(0.0, 0.0, 0.0),
                size: Vec3::new(12.0, 0.0, 16.0),
                tex_offset: (16, 49),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(2),
        },
        EntityPart {
            name: "left_bottom_bristle".into(),
            offset: Vec3::new(8.0, 3.0, -8.0),
            default_rotation: Vec3::new(0.0, 0.0, 1.2217305),
            cubes: vec![ModelCube {
                origin: Vec3::new(0.0, 0.0, 0.0),
                size: Vec3::new(12.0, 0.0, 16.0),
                tex_offset: (16, 65),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(2),
        },
    ];
    let mut model = bake_model(parts, 64, 128);
    model
}

pub fn bake_baby_strider_model() -> BakedEntityModel {
    let parts = vec![
        EntityPart {
            name: "body".into(),
            offset: Vec3::new(0.0, 16.75, 0.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-3.5, -3.75, -4.0),
                size: Vec3::new(7.0, 7.0, 8.0),
                tex_offset: (0, 0),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "bristle0".into(),
            offset: Vec3::new(0.0, -4.25, 2.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-3.5, -2.5, 0.0),
                size: Vec3::new(7.0, 3.0, 0.0),
                tex_offset: (0, 21),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(0),
        },
        EntityPart {
            name: "bristle1".into(),
            offset: Vec3::new(0.0, -4.25, 0.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-3.5, -2.5, 0.0),
                size: Vec3::new(7.0, 3.0, 0.0),
                tex_offset: (0, 18),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(0),
        },
        EntityPart {
            name: "bristle2".into(),
            offset: Vec3::new(0.0, -4.25, -2.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-3.5, -2.5, 0.0),
                size: Vec3::new(7.0, 3.0, 0.0),
                tex_offset: (0, 15),
                deformation: 0.0,
                mirror: false,
            }],
            parent: Some(0),
        },
        EntityPart {
            name: "right_leg".into(),
            offset: Vec3::new(-1.5, 20.0, 0.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.0, 0.0, -1.0),
                size: Vec3::new(2.0, 4.0, 2.0),
                tex_offset: (0, 24),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "left_leg".into(),
            offset: Vec3::new(1.5, 20.0, 0.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.0, 0.0, -1.0),
                size: Vec3::new(2.0, 4.0, 2.0),
                tex_offset: (8, 24),
                deformation: 0.0,
                mirror: false,
            }],
            parent: None,
        },
    ];
    let mut model = bake_model(parts, 32, 32);
    model
}

fn bake_llama_model_with_deformation(deformation: f32) -> BakedEntityModel {
    let parts = vec![
        EntityPart {
            name: "head".into(),
            offset: Vec3::new(0.0, 7.0, -6.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![
                ModelCube {
                    origin: Vec3::new(-2.0, -14.0, -10.0),
                    size: Vec3::new(4.0, 4.0, 9.0),
                    tex_offset: (0, 0),
                    deformation: -1.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(-4.0, -16.0, -6.0),
                    size: Vec3::new(8.0, 18.0, 6.0),
                    tex_offset: (0, 14),
                    deformation: -1.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(-4.0, -19.0, -4.0),
                    size: Vec3::new(3.0, 3.0, 2.0),
                    tex_offset: (17, 0),
                    deformation: -1.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(1.0, -19.0, -4.0),
                    size: Vec3::new(3.0, 3.0, 2.0),
                    tex_offset: (17, 0),
                    deformation: -1.0,
                    mirror: false,
                },
            ],
            parent: None,
        },
        EntityPart {
            name: "body".into(),
            offset: Vec3::new(0.0, 5.0, 2.0),
            default_rotation: Vec3::new(1.5707964, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-6.0, -10.0, -7.0),
                size: Vec3::new(12.0, 18.0, 10.0),
                tex_offset: (29, 0),
                deformation: -1.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "right_chest".into(),
            offset: Vec3::new(-8.5, 3.0, 3.0),
            default_rotation: Vec3::new(0.0, 1.5707964, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-3.0, 0.0, 0.0),
                size: Vec3::new(8.0, 8.0, 3.0),
                tex_offset: (45, 28),
                deformation: -1.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "left_chest".into(),
            offset: Vec3::new(5.5, 3.0, 3.0),
            default_rotation: Vec3::new(0.0, 1.5707964, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-3.0, 0.0, 0.0),
                size: Vec3::new(8.0, 8.0, 3.0),
                tex_offset: (45, 41),
                deformation: -1.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "right_hind_leg".into(),
            offset: Vec3::new(-3.5, 10.0, 6.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-2.0, 0.0, -2.0),
                size: Vec3::new(4.0, 14.0, 4.0),
                tex_offset: (29, 29),
                deformation: -1.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "left_hind_leg".into(),
            offset: Vec3::new(3.5, 10.0, 6.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-2.0, 0.0, -2.0),
                size: Vec3::new(4.0, 14.0, 4.0),
                tex_offset: (29, 29),
                deformation: -1.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "right_front_leg".into(),
            offset: Vec3::new(-3.5, 10.0, -5.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-2.0, 0.0, -2.0),
                size: Vec3::new(4.0, 14.0, 4.0),
                tex_offset: (29, 29),
                deformation: -1.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "left_front_leg".into(),
            offset: Vec3::new(3.5, 10.0, -5.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-2.0, 0.0, -2.0),
                size: Vec3::new(4.0, 14.0, 4.0),
                tex_offset: (29, 29),
                deformation: -1.0,
                mirror: false,
            }],
            parent: None,
        },
    ];
    let mut model = bake_model(parts, 128, 64);
    for p in &mut model.parts {
        for c in &mut p.cubes {
            if c.deformation == -1.0 {
                c.deformation = deformation;
            }
        }
    }
    model
}

fn bake_baby_llama_model_with_deformation(deformation: f32) -> BakedEntityModel {
    let parts = vec![
        EntityPart {
            name: "head".into(),
            offset: Vec3::new(0.0, 12.0, -4.0),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![
                ModelCube {
                    origin: Vec3::new(-3.0, -9.0, -4.0),
                    size: Vec3::new(6.0, 11.0, 4.0),
                    tex_offset: (0, 0),
                    deformation: -1.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(-1.5, -7.0, -7.0),
                    size: Vec3::new(3.0, 3.0, 3.0),
                    tex_offset: (0, 15),
                    deformation: -1.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(0.5, -11.0, -3.0),
                    size: Vec3::new(2.0, 2.0, 2.0),
                    tex_offset: (20, 4),
                    deformation: -1.0,
                    mirror: false,
                },
                ModelCube {
                    origin: Vec3::new(-2.5, -11.0, -3.0),
                    size: Vec3::new(2.0, 2.0, 2.0),
                    tex_offset: (20, 0),
                    deformation: -1.0,
                    mirror: false,
                },
            ],
            parent: None,
        },
        EntityPart {
            name: "right_hind_leg".into(),
            offset: Vec3::new(-2.5, 16.5, 4.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.4, -0.5, -1.5),
                size: Vec3::new(3.0, 8.0, 3.0),
                tex_offset: (0, 45),
                deformation: -1.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "left_hind_leg".into(),
            offset: Vec3::new(2.5, 16.5, 4.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.6, -0.5, -1.5),
                size: Vec3::new(3.0, 8.0, 3.0),
                tex_offset: (12, 45),
                deformation: -1.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "right_front_leg".into(),
            offset: Vec3::new(-2.5, 16.5, -3.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.4, -0.5, -1.5),
                size: Vec3::new(3.0, 8.0, 3.0),
                tex_offset: (0, 34),
                deformation: -1.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "left_front_leg".into(),
            offset: Vec3::new(2.5, 16.5, -3.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-1.6, -0.5, -1.5),
                size: Vec3::new(3.0, 8.0, 3.0),
                tex_offset: (12, 34),
                deformation: -1.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "body".into(),
            offset: Vec3::new(0.0, 14.0, 2.5),
            default_rotation: Vec3::new(0.0, 0.0, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-4.0, -3.0, -8.5),
                size: Vec3::new(8.0, 6.0, 13.0),
                tex_offset: (0, 15),
                deformation: -1.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "right_chest".into(),
            offset: Vec3::new(-8.5, 4.0, 3.0),
            default_rotation: Vec3::new(0.0, 1.5707964, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-3.0, 0.0, 0.0),
                size: Vec3::new(8.0, 8.0, 3.0),
                tex_offset: (45, 28),
                deformation: -1.0,
                mirror: false,
            }],
            parent: None,
        },
        EntityPart {
            name: "left_chest".into(),
            offset: Vec3::new(5.5, 4.0, 3.0),
            default_rotation: Vec3::new(0.0, 1.5707964, 0.0),
            cubes: vec![ModelCube {
                origin: Vec3::new(-3.0, 0.0, 0.0),
                size: Vec3::new(8.0, 8.0, 3.0),
                tex_offset: (45, 41),
                deformation: -1.0,
                mirror: false,
            }],
            parent: None,
        },
    ];
    let mut model = bake_model(parts, 64, 64);
    for p in &mut model.parts {
        for c in &mut p.cubes {
            if c.deformation == -1.0 {
                c.deformation = deformation;
            }
        }
    }
    model
}

pub fn bake_llama_model() -> BakedEntityModel {
    bake_llama_model_with_deformation(0.0)
}
pub fn bake_llama_decor_model() -> BakedEntityModel {
    bake_llama_model_with_deformation(0.5)
}
pub fn bake_baby_llama_model() -> BakedEntityModel {
    bake_baby_llama_model_with_deformation(0.0)
}
pub fn bake_baby_llama_decor_model() -> BakedEntityModel {
    bake_baby_llama_model_with_deformation(0.2)
}

pub fn bake_camel_husk_model() -> BakedEntityModel {
    bake_camel_model()
}
pub fn bake_zoglin_model() -> BakedEntityModel {
    bake_hoglin_model()
}
pub fn bake_baby_zoglin_model() -> BakedEntityModel {
    bake_baby_hoglin_model()
}
