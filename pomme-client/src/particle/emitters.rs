//! Java 26.2 emitter and special-model particle providers.
use std::collections::HashMap;

use glam::{DVec3, Quat};

use super::{
    Appearance, Kind, Particle, ParticleStore, ServerParticleKind as K, ServerParticleOptions as O,
    descriptor_age_frame, descriptor_frame,
};
use crate::renderer::chunk::atlas::AtlasUVMap;
use crate::renderer::chunk::mesher::BiomeClimate;
use crate::world::block::registry::BlockRegistry;
use crate::world::chunk::ChunkStore;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct State {
    pub(super) kind: K,
    water_blocks: i32,
    impulse_bits: u32,
    seed: u32,
    done: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum RenderRequest {
    ElderGuardianModel {
        position: DVec3,
        age: f32,
        alpha: f32,
        rotation_x_degrees: f32,
        scale: [f32; 3],
        model_translation: [f32; 3],
    },
}

/// The emitted geyser/gust quads use Java's opaque layer; the Elder Guardian
/// uses its own model group and never enters quad extraction.
pub(super) const fn translucent(kind: K) -> bool {
    matches!(kind, K::NoxiousGas)
}

pub(super) fn render_request(p: &Particle, partial: f32) -> Option<RenderRequest> {
    let Kind::Emitters(s) = &p.kind else {
        return None;
    };
    (s.kind == K::ElderGuardian).then(|| {
        let age = p.age as f32 + partial;
        let progress = age / p.lifetime as f32;
        RenderRequest::ElderGuardianModel {
            position: p.prev_pos.lerp(p.pos, f64::from(partial)),
            age,
            alpha: 0.05 + 0.5 * (std::f32::consts::PI * progress).sin(),
            rotation_x_degrees: 60.0 - 150.0 * progress,
            scale: [0.425_531_92, -0.425_531_92, -0.425_531_92],
            model_translation: [0.0, -0.56, 3.5],
        }
    })
}

pub(super) fn supports(k: K) -> bool {
    matches!(
        k,
        K::Geyser
            | K::GeyserBase
            | K::GeyserPoof
            | K::GeyserPlume
            | K::NoxiousGasCloud
            | K::GustEmitterLarge
            | K::GustEmitterSmall
            | K::ElderGuardian
    )
}

fn sprite(k: K) -> &'static str {
    match k {
        K::GeyserBase => "minecraft:geyser_base",
        K::GeyserPoof => "minecraft:geyser_poof",
        K::GeyserPlume => "minecraft:geyser_plume",
        K::NoxiousGas => "minecraft:noxious_gas",
        K::Gust => "minecraft:gust",
        K::SmallGust => "minecraft:small_gust",
        _ => "minecraft:geyser_base",
    }
}
fn quad(k: K, pos: DVec3, vel: DVec3, life: i32, size: f32, atlas: &AtlasUVMap) -> Particle {
    let uv = descriptor_frame(atlas, sprite(k), 0).unwrap_or(atlas.missing_region());
    Particle::special(
        Kind::Emitters(State {
            kind: k,
            water_blocks: 0,
            impulse_bits: 0,
            seed: fastrand::u32(..),
            done: false,
        }),
        pos,
        vel,
        None,
        life,
        size,
        uv,
    )
}

pub(super) fn spawn(
    store: &mut ParticleStore,
    k: K,
    options: O,
    pos: DVec3,
    vel: DVec3,
    _registry: &BlockRegistry,
    _chunks: &ChunkStore,
    _biome: &HashMap<u32, BiomeClimate>,
) -> bool {
    if !supports(k) {
        return false;
    }
    let (water, impulse) = match (k, options) {
        (K::Geyser | K::GeyserPlume, O::Geyser { water_blocks }) => (water_blocks.max(0), 0.0),
        (
            K::GeyserBase | K::GeyserPoof,
            O::GeyserBase {
                water_blocks,
                burst_impulse_base,
            },
        ) => (water_blocks.max(0), burst_impulse_base),
        (K::Geyser | K::GeyserPlume | K::GeyserBase | K::GeyserPoof, _) => return false,
        (
            K::NoxiousGasCloud | K::GustEmitterLarge | K::GustEmitterSmall | K::ElderGuardian,
            O::Simple,
        ) => (0, 0.0),
        _ => return false,
    };
    let (life, size) = match k {
        K::Geyser | K::NoxiousGasCloud => (20, 0.),
        K::GustEmitterLarge => (7, 0.),
        K::GustEmitterSmall => (3, 0.),
        K::ElderGuardian => (30, 0.),
        K::GeyserBase | K::GeyserPoof => (25, 0.),
        K::GeyserPlume => (water.max(1) * 5, 0.),
        _ => return false,
    };
    let uv = descriptor_frame(&store.uv_map, sprite(k), 0).unwrap_or(store.uv_map.missing_region());
    store.pending.push(Particle::special(
        Kind::Emitters(State {
            kind: k,
            water_blocks: water,
            impulse_bits: impulse.to_bits(),
            seed: fastrand::u32(..),
            done: false,
        }),
        pos,
        vel,
        None,
        life,
        size,
        uv,
    ));
    true
}

pub(super) fn tick(
    p: &mut Particle,
    chunks: &ChunkStore,
    atlas: &AtlasUVMap,
    children: &mut Vec<Particle>,
) -> bool {
    let Kind::Emitters(mut s) = p.kind else {
        return false;
    };
    p.prev_pos = p.pos;
    if p.age >= p.lifetime {
        return false;
    }
    p.age += 1;
    match s.kind {
        K::Geyser => {
            if p.age % 2 == 0 {
                for _ in 0..2 {
                    children.push(geyser_child(
                        K::GeyserBase,
                        p.pos,
                        p.vel,
                        s.water_blocks,
                        1.5,
                        atlas,
                    ));
                }
            }
            for _ in 0..s.water_blocks.saturating_add(2) {
                children.push(geyser_child(
                    K::GeyserPlume,
                    p.pos,
                    p.vel,
                    s.water_blocks,
                    0.,
                    atlas,
                ));
            }
            if p.age % 10 == 0 {
                for _ in 0..20 {
                    children.push(geyser_child(
                        K::GeyserPoof,
                        p.pos,
                        p.vel,
                        s.water_blocks,
                        2.,
                        atlas,
                    ));
                }
            }
        }
        K::Gust | K::SmallGust => {
            animate(p, atlas, s.kind);
            p.size = if s.kind == K::SmallGust { 0.15 } else { 1.0 };
            p.light = 1.0;
        }
        K::GeyserBase | K::GeyserPoof => {
            p.pos += p.vel;
            p.vel.y -= 0.04 * p.gravity;
            p.vel *= 0.725;
            if p.age == 1 {
                p.vel.y = p.vel.y.abs() + f64::from(f32::from_bits(s.impulse_bits));
            }
            animate(p, atlas, s.kind);
        }
        K::GeyserPlume => {
            let max_y = s.water_blocks.max(1) as f64 - 1.;
            let start = p.target.unwrap_or(p.pos).y;
            let t = ((p.pos.y - start) / max_y).clamp(0., 1.);
            let t = if t.is_finite() { t } else { 0. };
            let x = ((s.seed as u16 as i16) as f64 / 32768.) * 0.1 * t;
            let z = (((s.seed >> 16) as u16 as i16) as f64 / 32768.) * 0.1 * t;
            p.vel.x = x;
            p.vel.z = z;
            let propulsion =
                1.45 * (if s.water_blocks == 1 { 1.5 } else { 1. }) * s.water_blocks.max(1) as f32;
            p.gravity = f64::from(propulsion * (t as f32).powi(3) * 0.12);
            p.vel.y -= p.gravity;
            p.pos += p.vel;
            p.size = p.base_size
                * (2. + s.water_blocks as f32 / 8. + t as f32 * (1. + s.water_blocks as f32 / 8.));
            if !s.done && (p.vel.y < 0. || p.pos.y > start + max_y) {
                p.lifetime = (p.age + 5).min(p.lifetime);
                p.friction = 0.;
                s.done = true;
            }
            animate(p, atlas, s.kind);
        }
        K::NoxiousGas => {
            p.vel.y -= 0.04 * p.gravity;
            p.pos += p.vel;
            p.vel *= p.friction;
            p.size = p.base_size * (p.age as f32 / p.lifetime as f32 * 32.0).clamp(0.0, 1.0);
            animate(p, atlas, s.kind);
        }
        K::NoxiousGasCloud => {
            if p.age % 2 == 0 {
                // The native picker samples a random direction and radius <= 3 blocks.
                let mut dir = DVec3::new(fastrand::f64() - 0.5, 0., fastrand::f64() - 0.5)
                    .normalize_or_zero();
                if dir == DVec3::ZERO {
                    dir = DVec3::X;
                }
                let radius = f64::from(fastrand::f32() * 3.);
                let point = p.pos + dir * radius - DVec3::Y * 0.25;
                let source = azalea_core::position::BlockPos::new(
                    p.pos.x.floor() as i32,
                    p.pos.y.floor() as i32,
                    p.pos.z.floor() as i32,
                );
                if crate::world::block::noxious_gas_reachable(source, point, |pos| {
                    let chunk_pos = azalea_core::position::ChunkPos::new(
                        pos.x.div_euclid(16),
                        pos.z.div_euclid(16),
                    );
                    chunks.get_chunk(&chunk_pos)?;
                    Some(chunks.get_block_state(pos.x, pos.y, pos.z))
                }) {
                    let lifetime = (3.0 / (fastrand::f32() * 0.8 + 0.2)) as i32;
                    let mut gas = quad(
                        K::NoxiousGas,
                        point,
                        DVec3::ZERO,
                        lifetime.max(1),
                        0.15,
                        atlas,
                    );
                    gas.color = [0.1; 3];
                    gas.gravity = -0.1;
                    gas.friction = 0.96;
                    children.push(gas);
                }
            }
        }
        K::GustEmitterLarge | K::GustEmitterSmall => {
            // Provider timing/scale values are represented as state rather than visible
            // seed quads.
            let (scale, delay) = if s.kind == K::GustEmitterLarge {
                (3., 0)
            } else {
                (1., 2)
            };
            if p.age % (delay + 1) == 0 {
                for _ in 0..3 {
                    let off = DVec3::new(
                        fastrand::f64() - fastrand::f64(),
                        fastrand::f64() - fastrand::f64(),
                        fastrand::f64() - fastrand::f64(),
                    ) * scale;
                    let gust_kind = if s.kind == K::GustEmitterLarge {
                        K::Gust
                    } else {
                        K::SmallGust
                    };
                    let gust = quad(
                        gust_kind,
                        p.pos + off,
                        DVec3::ZERO,
                        12 + fastrand::i32(0..4),
                        if gust_kind == K::SmallGust { 0.15 } else { 1.0 },
                        atlas,
                    );
                    children.push(gust);
                }
            }
        }
        K::ElderGuardian => {}
        _ => return false,
    }
    p.kind = Kind::Emitters(s);
    true
}
fn animate(p: &mut Particle, atlas: &AtlasUVMap, k: K) {
    if let Some(frames) = atlas.particle_sprite_names(sprite(k)) {
        if let Some(frame) = descriptor_frame(
            atlas,
            sprite(k),
            descriptor_age_frame(p.age, p.lifetime, frames.len()),
        ) {
            p.set_sprite(&frame);
        }
    }
    p.size = p.base_size * (p.age as f32 / p.lifetime.max(1) as f32 * 32.).clamp(0., 1.);
}
fn geyser_child(
    k: K,
    pos: DVec3,
    vel: DVec3,
    water: i32,
    impulse: f32,
    atlas: &AtlasUVMap,
) -> Particle {
    let life = if k == K::GeyserPlume {
        water.max(1) * 5
    } else {
        (25.0 * (fastrand::f32() * 0.2 + 0.8)) as i32
    };
    let mut p = quad(k, pos, vel, life, 0.1, atlas);
    let Kind::Emitters(mut s) = p.kind else {
        unreachable!()
    };
    s.water_blocks = water;
    s.impulse_bits = impulse.to_bits();
    if k == K::GeyserPlume {
        p.pos.x += (fastrand::f64() - 0.5) * 0.2;
        p.pos.y += f64::from(fastrand::f32());
        p.pos.z += (fastrand::f64() - 0.5) * 0.2;
        p.target = Some(p.pos);
        p.base_size = 0.075;
        p.size = p.base_size * (2. + water as f32 / 8.);
        let propulsion = 1.45 * (if water == 1 { 1.5 } else { 1. }) * water.max(1) as f32;
        p.vel.y = f64::from(propulsion);
        p.gravity = -f64::from(propulsion);
    } else {
        p.pos.x += (fastrand::f64() - 0.5) * 0.5;
        p.pos.y += 0.2 + (fastrand::f64() - 0.5) * 0.5;
        p.pos.z += (fastrand::f64() - 0.5) * 0.5;
        p.vel.y = p.vel.y.abs();
        p.base_size = 0.25 + 0.125 * water as f32;
        p.size = 3. + 0.125 * water as f32;
        p.gravity = 0.;
        p.friction = 0.725;
    }
    p.kind = Kind::Emitters(s);
    p
}

pub(super) fn appearance(p: &Particle, partial: f32) -> Option<Appearance> {
    let Kind::Emitters(s) = &p.kind else {
        return None;
    };
    matches!(
        s.kind,
        K::GeyserBase | K::GeyserPoof | K::GeyserPlume | K::NoxiousGas | K::Gust | K::SmallGust
    )
    .then(|| {
        let age = p.age as f32 + partial;
        Appearance {
            size: p.size,
            color: p.color,
            alpha: if s.kind == K::NoxiousGas {
                (1. - age / p.lifetime as f32).clamp(0., 1.)
            } else {
                p.alpha
            },
            rotation: Quat::IDENTITY,
            second_rotation: None,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_eight_providers_have_one_owner() {
        for k in [
            K::Geyser,
            K::GeyserBase,
            K::GeyserPoof,
            K::GeyserPlume,
            K::NoxiousGasCloud,
            K::GustEmitterLarge,
            K::GustEmitterSmall,
            K::ElderGuardian,
        ] {
            assert!(supports(k));
        }
    }
    #[test]
    fn geyser_emits_two_base_and_water_plus_two_plumes_every_other_tick() {
        let atlas = AtlasUVMap::test_empty();
        let mut p = Particle::special(
            Kind::Emitters(State {
                kind: K::Geyser,
                water_blocks: 3,
                impulse_bits: 1.5f32.to_bits(),
                seed: 1,
                done: false,
            }),
            DVec3::ZERO,
            DVec3::ZERO,
            None,
            20,
            0.,
            atlas.missing_region(),
        );
        p.age = 1;
        let mut c = Vec::new();
        assert!(tick(&mut p, &ChunkStore::new(2), &atlas, &mut c));
        assert_eq!(c.len(), 7);
        assert_eq!(
            c.iter()
                .filter(|p| matches!(
                    p.kind,
                    Kind::Emitters(State {
                        kind: K::GeyserBase,
                        ..
                    })
                ))
                .count(),
            2
        );
        assert_eq!(
            c.iter()
                .filter(|p| matches!(
                    p.kind,
                    Kind::Emitters(State {
                        kind: K::GeyserPlume,
                        ..
                    })
                ))
                .count(),
            5
        );
    }
    #[test]
    fn geyser_runs_twenty_ticks_with_exact_child_types_and_options() {
        let atlas = AtlasUVMap::test_empty();
        let mut p = Particle::special(
            Kind::Emitters(State {
                kind: K::Geyser,
                water_blocks: 4,
                impulse_bits: 1.5f32.to_bits(),
                seed: 7,
                done: false,
            }),
            DVec3::ZERO,
            DVec3::ZERO,
            None,
            20,
            0.0,
            atlas.missing_region(),
        );
        let mut children = Vec::new();
        for _ in 0..20 {
            assert!(tick(&mut p, &ChunkStore::new(2), &atlas, &mut children));
        }
        assert_eq!(children.len(), 20 * 6 + 10 * 2 + 2 * 20);
        let count = |kind| {
            children
                .iter()
                .filter(|child| matches!(child.kind, Kind::Emitters(State { kind: child_kind, .. }) if child_kind == kind))
                .count()
        };
        assert_eq!(count(K::GeyserBase), 20);
        assert_eq!(count(K::GeyserPlume), 120);
        assert_eq!(count(K::GeyserPoof), 40);
        for child in &children {
            let Kind::Emitters(state) = &child.kind else {
                panic!("unexpected child type")
            };
            assert_eq!(state.water_blocks, 4);
            if state.kind == K::GeyserBase {
                assert_eq!(f32::from_bits(state.impulse_bits), 1.5);
            }
            if state.kind == K::GeyserPoof {
                assert_eq!(f32::from_bits(state.impulse_bits), 2.0);
            }
        }
    }

    #[test]
    fn gust_seed_providers_emit_three_gusts_with_java_scale_and_timing() {
        let atlas = AtlasUVMap::test_empty();
        let mut large = Particle::special(
            Kind::Emitters(State {
                kind: K::GustEmitterLarge,
                water_blocks: 0,
                impulse_bits: 0,
                seed: 0,
                done: false,
            }),
            DVec3::ZERO,
            DVec3::ZERO,
            None,
            7,
            0.0,
            atlas.missing_region(),
        );
        let mut children = Vec::new();
        for _ in 0..7 {
            assert!(tick(&mut large, &ChunkStore::new(2), &atlas, &mut children));
        }
        assert_eq!(children.len(), 21);
        assert!(children.iter().all(|p| {
            matches!(p.kind, Kind::Emitters(State { kind: K::Gust, .. }))
                && p.vel == DVec3::ZERO
                && (12..=15).contains(&p.lifetime)
                && p.size == 1.0
                && p.pos.x.abs() <= 3.0
                && p.pos.y.abs() <= 3.0
                && p.pos.z.abs() <= 3.0
                && p.u0 == atlas.missing_region().u_min
                && p.v0 == atlas.missing_region().v_min
        }));

        let mut small = Particle::special(
            Kind::Emitters(State {
                kind: K::GustEmitterSmall,
                water_blocks: 0,
                impulse_bits: 0,
                seed: 0,
                done: false,
            }),
            DVec3::ZERO,
            DVec3::ZERO,
            None,
            3,
            0.0,
            atlas.missing_region(),
        );
        let mut small_children = Vec::new();
        for _ in 0..3 {
            assert!(tick(
                &mut small,
                &ChunkStore::new(2),
                &atlas,
                &mut small_children
            ));
        }
        assert_eq!(small_children.len(), 3);
        assert!(small_children.iter().all(|p| {
            matches!(
                p.kind,
                Kind::Emitters(State {
                    kind: K::SmallGust,
                    ..
                })
            ) && p.vel == DVec3::ZERO
                && (12..=15).contains(&p.lifetime)
                && p.size == 0.15
                && p.pos.x.abs() <= 1.0
                && p.pos.y.abs() <= 1.0
                && p.pos.z.abs() <= 1.0
                && p.u0 == atlas.missing_region().u_min
                && p.v0 == atlas.missing_region().v_min
        }));
    }

    #[test]
    fn quad_emitters_use_the_java_opaque_layer_and_elder_model_is_not_a_quad() {
        for kind in [K::GeyserBase, K::GeyserPoof, K::GeyserPlume, K::Gust] {
            assert!(!translucent(kind), "{kind:?} uses an opaque Java layer");
        }
        assert!(translucent(K::NoxiousGas));
        assert!(
            !appearance(
                &Particle::special(
                    Kind::Emitters(State {
                        kind: K::ElderGuardian,
                        water_blocks: 0,
                        impulse_bits: 0,
                        seed: 0,
                        done: false,
                    }),
                    DVec3::ZERO,
                    DVec3::ZERO,
                    None,
                    30,
                    1.0,
                    AtlasUVMap::test_empty().missing_region(),
                ),
                0.5,
            )
            .is_some()
        );
    }

    #[test]
    fn elder_request_alpha_and_rotation_match_java_lifecycle_endpoints() {
        let atlas = AtlasUVMap::test_empty();
        let mut p = Particle::special(
            Kind::Emitters(State {
                kind: K::ElderGuardian,
                water_blocks: 0,
                impulse_bits: 0,
                seed: 0,
                done: false,
            }),
            DVec3::ZERO,
            DVec3::ZERO,
            None,
            30,
            0.0,
            atlas.missing_region(),
        );
        for (age, alpha, rotation) in [(0, 0.05, 60.0), (15, 0.55, -15.0), (30, 0.05, -90.0)] {
            p.age = age;
            let Some(RenderRequest::ElderGuardianModel {
                alpha: actual_alpha,
                rotation_x_degrees,
                ..
            }) = render_request(&p, 0.0)
            else {
                panic!("expected guardian model request")
            };
            assert!((actual_alpha - alpha).abs() < 1e-6);
            assert!((rotation_x_degrees - rotation).abs() < 1e-6);
        }
    }

    #[test]
    fn elder_request_is_model_not_quad_and_matches_vanilla_transform_values() {
        let p = Particle::special(
            Kind::Emitters(State {
                kind: K::ElderGuardian,
                water_blocks: 0,
                impulse_bits: 0,
                seed: 0,
                done: false,
            }),
            DVec3::new(2., 3., 4.),
            DVec3::ZERO,
            None,
            30,
            0.,
            AtlasUVMap::test_empty().missing_region(),
        );
        assert!(appearance(&p, 0.5).is_none());
        let Some(RenderRequest::ElderGuardianModel {
            position,
            age,
            alpha,
            rotation_x_degrees,
            scale,
            model_translation,
        }) = render_request(&p, 0.5)
        else {
            panic!("expected guardian model request")
        };
        assert_eq!(position, DVec3::new(2., 3., 4.));
        assert_eq!(age, 0.5);
        assert!((alpha - 0.07616799).abs() < 1e-6);
        assert_eq!(rotation_x_degrees, 57.5);
        assert_eq!(scale, [0.42553192, -0.42553192, -0.42553192]);
        assert_eq!(model_translation, [0.0, -0.56, 3.5]);
    }
}
