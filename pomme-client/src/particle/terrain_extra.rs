//! Java block/item-backed particle providers from ParticleResources.java.
use std::collections::HashMap;

use azalea_block::BlockState;
use glam::{DVec3, Quat, dvec3};

use super::{
    Appearance, Kind, Particle, ParticleStore, ServerParticleKind as K, ServerParticleOptions as O,
};
use crate::renderer::chunk::atlas::AtlasUVMap;
use crate::renderer::chunk::mesher::{BiomeClimate, world_brightness};
use crate::world::block::registry::{BlockRegistry, Tint};
use crate::world::block::{block_id, is_air};
use crate::world::chunk::ChunkStore;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct State {
    pub(super) kind: K,
    pub(super) translucent: bool,
}

pub(super) fn translucent(state: State) -> bool {
    state.kind != K::FallingDust && state.translucent
}

pub(super) fn supports(kind: K) -> bool {
    matches!(
        kind,
        K::BlockMarker
            | K::FallingDust
            | K::DustPillar
            | K::BlockCrumble
            | K::ItemSlime
            | K::ItemCobweb
            | K::ItemSnowball
    )
}

fn options_match(kind: K, options: &O) -> bool {
    if matches!(
        kind,
        K::BlockMarker | K::FallingDust | K::DustPillar | K::BlockCrumble
    ) {
        matches!(options, O::Block(_))
    } else {
        matches!(kind, K::ItemSlime | K::ItemCobweb | K::ItemSnowball)
            && matches!(options, O::Simple)
    }
}

pub(super) fn spawn(
    store: &mut ParticleStore,
    kind: K,
    options: O,
    pos: DVec3,
    vel: DVec3,
    registry: &BlockRegistry,
    chunks: &ChunkStore,
    climate: &HashMap<u32, BiomeClimate>,
) -> bool {
    if !options_match(kind, &options) {
        return false;
    }
    if matches!(
        kind,
        K::BlockMarker | K::FallingDust | K::DustPillar | K::BlockCrumble
    ) {
        let O::Block(state) = options else {
            return false;
        };
        let Some(p) = block_particle(store, kind, state, pos, vel, registry, chunks, climate)
        else {
            return false;
        };
        store.push(p);
        return true;
    }
    let name = match kind {
        K::ItemSlime => "slime_ball",
        K::ItemCobweb => "cobweb",
        K::ItemSnowball => "snowball",
        _ => return false,
    };
    let Some(id) = pomme_protocol::registries::RegistryTable::native()
        .id_of(pomme_protocol::registries::ClientRegistry::Item, name)
    else {
        return false;
    };
    let Some(item) =
        <azalea_registry::builtin::ItemKind as azalea_registry::Registry>::from_u32(id)
    else {
        return false;
    };
    let stack = azalea_inventory::ItemStack::Present(azalea_inventory::ItemStackData {
        kind: item,
        count: 1,
        component_patch: Default::default(),
    });
    let region = store.item_particle_region(registry, &stack, None);
    let pos_block = azalea_core::position::BlockPos::new(
        pos.x.floor() as i32,
        pos.y.floor() as i32,
        pos.z.floor() as i32,
    );
    let mut particle = Particle::breaking_item(
        pos,
        vel,
        region,
        world_brightness(chunks, pos_block.x, pos_block.y, pos_block.z),
    );
    particle.kind = Kind::TerrainExtra(State {
        kind,
        translucent: region.translucent,
    });
    store.push(particle);
    true
}

fn block_state_supported(kind: K, state: BlockState) -> bool {
    match kind {
        K::BlockMarker => true,
        K::FallingDust => is_air(state) || !crate::world::block::has_invisible_render_shape(state),
        K::DustPillar | K::BlockCrumble => {
            !is_air(state)
                && block_id(state) != "moving_piston"
                && crate::world::block::should_spawn_terrain_particles(state)
        }
        _ => false,
    }
}

fn block_particle(
    store: &ParticleStore,
    kind: K,
    state: BlockState,
    pos: DVec3,
    velocity: DVec3,
    registry: &BlockRegistry,
    chunks: &ChunkStore,
    climate: &HashMap<u32, BiomeClimate>,
) -> Option<Particle> {
    let marker = kind == K::BlockMarker;
    let falling = kind == K::FallingDust;
    if !block_state_supported(kind, state) {
        return None;
    }
    let faces = registry.get_textures(state);
    if !falling && faces.is_none() {
        return None;
    }
    let faces = faces;
    let bp = azalea_core::position::BlockPos::new(
        pos.x.floor() as i32,
        pos.y.floor() as i32,
        pos.z.floor() as i32,
    );
    let light = world_brightness(chunks, bp.x, bp.y, bp.z);
    let color = if marker {
        [1.0; 3]
    } else if falling {
        falling_dust_color(
            store,
            faces.map_or(Tint::None, |f| f.tint),
            state,
            bp,
            chunks,
            climate,
        )
    } else {
        block_tint(store, faces.unwrap().tint, state, bp, chunks, climate)
    };
    let region = if falling {
        store
            .uv_map
            .particle_sprite_names("minecraft:falling_dust")
            .and_then(|f| f.first())
            .filter(|n| store.uv_map.has_region(n))
            .map(|n| store.uv_map.get_region(n))
            .unwrap_or_else(|| store.uv_map.get_region("particle/falling_dust"))
    } else {
        // BlockMarker uses the block model's particle material, not its own JSON
        // sprite.
        let faces = faces.unwrap();
        store
            .uv_map
            .get_region(faces.particle.as_deref().unwrap_or(&faces.top))
    };
    let provider_state = State {
        kind,
        translucent: !falling && region.translucent,
    };
    if falling {
        let base = (32.0 / (fastrand::f32() * 0.8 + 0.2)) as i32;
        let lifetime = ((base as f32 * 0.9) as i32).max(1);
        let mut p = Particle::special(
            Kind::TerrainExtra(provider_state),
            pos,
            velocity,
            None,
            lifetime,
            0.2 * 0.675,
            region,
        );
        p.color = color;
        p.light = light;
        p.friction = 0.98;
        p.pitch = (fastrand::f32() - 0.5) * 0.1;
        p.rot = fastrand::f32() * std::f32::consts::TAU;
        p.rot_o = p.rot;
        Some(p)
    } else if marker {
        let mut p = Particle::special(
            Kind::TerrainExtra(provider_state),
            pos,
            DVec3::ZERO,
            None,
            80,
            0.5,
            region,
        );
        p.color = [1.0; 3];
        p.light = light;
        Some(p)
    } else {
        let mut p = Particle::terrain(pos, velocity, region, color, light);
        p.kind = Kind::TerrainExtra(provider_state);
        match kind {
            K::BlockCrumble => {
                p.vel = DVec3::ZERO;
                p.lifetime = fastrand::i32(1..=10);
            }
            K::DustPillar => {
                p.vel = dvec3(
                    super::next_gaussian() / 30.0,
                    velocity.y + super::next_gaussian() / 2.0,
                    super::next_gaussian() / 30.0,
                );
                p.lifetime = fastrand::i32(20..=39);
            }
            _ => {}
        }
        Some(p)
    }
}

fn falling_dust_color(
    store: &ParticleStore,
    tint: Tint,
    state: BlockState,
    pos: azalea_core::position::BlockPos,
    chunks: &ChunkStore,
    climate: &HashMap<u32, BiomeClimate>,
) -> [f32; 3] {
    let rgb = crate::world::block::falling_block_dust_color(state)
        .or_else(|| {
            if tint == Tint::None {
                crate::world::block::map_color_rgb(state)
            } else {
                Some(tint_color(store, tint, state, pos, chunks, climate))
            }
        })
        .unwrap_or([0, 0, 0]);
    rgb.map(|channel| f32::from(channel) / 255.0)
}

fn tint_color(
    store: &ParticleStore,
    tint: Tint,
    state: BlockState,
    pos: azalea_core::position::BlockPos,
    chunks: &ChunkStore,
    climate: &HashMap<u32, BiomeClimate>,
) -> [u8; 3] {
    let color = match tint {
        Tint::None => [1.0; 3],
        Tint::Redstone => crate::world::block::redstone_wire_rgb(state),
        Tint::Stem => crate::world::block::stem_rgb(state),
        tint => store.blend_tint(tint, pos, chunks, climate),
    };
    color.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn block_tint(
    store: &ParticleStore,
    tint: Tint,
    state: BlockState,
    pos: azalea_core::position::BlockPos,
    chunks: &ChunkStore,
    climate: &HashMap<u32, BiomeClimate>,
) -> [f32; 3] {
    let base = [0.6; 3];
    if tint == Tint::None || block_id(state) == "grass_block" {
        return base;
    }
    let color = match tint {
        Tint::Redstone => crate::world::block::redstone_wire_rgb(state),
        Tint::Stem => crate::world::block::stem_rgb(state),
        tint => store.blend_tint(tint, pos, chunks, climate),
    };
    [base[0] * color[0], base[1] * color[1], base[2] * color[2]]
}

pub(super) fn tick(
    p: &mut Particle,
    chunks: &ChunkStore,
    atlas: &AtlasUVMap,
    _children: &mut Vec<Particle>,
) -> bool {
    let Kind::TerrainExtra(state) = p.kind else {
        return false;
    };
    p.prev_pos = p.pos;
    if p.age >= p.lifetime {
        return false;
    }
    p.age += 1;
    match state.kind {
        K::BlockMarker => {}
        K::FallingDust => {
            if let Some(frames) = atlas
                .particle_sprite_names("minecraft:falling_dust")
                .filter(|f| !f.is_empty())
            {
                let i = super::animated_frame_index(p.age, p.lifetime, frames.len());
                if let Some(name) = frames.get(i) {
                    p.set_sprite(&atlas.get_region(name));
                }
            }
            p.rot_o = p.rot;
            p.rot += std::f32::consts::PI * p.pitch * 2.0;
            if p.on_ground {
                p.rot_o = 0.0;
                p.rot = 0.0;
            }
            p.move_with_collision(chunks);
            p.vel.y = (p.vel.y - 0.003).max(-0.14);
        }
        K::BlockCrumble | K::DustPillar | K::ItemSlime | K::ItemCobweb | K::ItemSnowball => {
            p.vel.y -= 0.04 * p.gravity;
            p.move_with_collision(chunks);
            p.vel *= p.friction;
            if p.on_ground {
                p.vel.x *= 0.7;
                p.vel.z *= 0.7;
            }
            p.light = world_brightness(
                chunks,
                p.pos.x.floor() as i32,
                p.pos.y.floor() as i32,
                p.pos.z.floor() as i32,
            );
        }
        _ => return false,
    }
    true
}

pub(super) fn appearance(p: &Particle, partial: f32) -> Option<Appearance> {
    let Kind::TerrainExtra(state) = p.kind else {
        return None;
    };
    let (size, rotation) = match state.kind {
        K::BlockMarker => (0.5, Quat::IDENTITY),
        K::FallingDust => {
            let grow = ((p.age as f32 + partial) / p.lifetime as f32 * 32.0).clamp(0.0, 1.0);
            let angle = p.rot_o + (p.rot - p.rot_o) * partial;
            (p.size * grow, Quat::from_rotation_z(angle))
        }
        _ => (p.size, Quat::IDENTITY),
    };
    Some(Appearance {
        size,
        color: p.color,
        alpha: p.alpha,
        rotation,
        second_rotation: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const KINDS: [K; 7] = [
        K::BlockMarker,
        K::FallingDust,
        K::DustPillar,
        K::BlockCrumble,
        K::ItemSlime,
        K::ItemCobweb,
        K::ItemSnowball,
    ];

    #[test]
    fn all_seven_providers_accept_only_their_typed_options() {
        let _protocol = crate::world::block::test_protocol_guard();
        let state = crate::world::block::first_state_of("stone").expect("native stone state");
        for kind in KINDS {
            assert!(supports(kind));
            let block = matches!(
                kind,
                K::BlockMarker | K::FallingDust | K::DustPillar | K::BlockCrumble
            );
            let valid = if block { O::Block(state) } else { O::Simple };
            let invalid = if block { O::Simple } else { O::Block(state) };
            assert!(options_match(kind, &valid));
            assert!(!options_match(kind, &invalid));
        }
    }

    #[test]
    fn block_providers_use_their_java_specific_state_filters() {
        let _protocol = crate::world::block::test_protocol_guard();
        let state = |name| crate::world::block::first_state_of(name).expect(name);
        let air = state("air");
        let stone = state("stone");
        assert!(block_state_supported(K::BlockMarker, air));
        assert!(block_state_supported(K::BlockMarker, state("barrier")));
        assert!(block_state_supported(K::FallingDust, air));
        assert!(block_state_supported(K::FallingDust, stone));
        assert!(!block_state_supported(K::FallingDust, state("water")));
        assert!(!block_state_supported(
            K::FallingDust,
            state("structure_void")
        ));
        assert!(block_state_supported(K::DustPillar, stone));
        assert!(!block_state_supported(K::DustPillar, air));
        assert!(!block_state_supported(
            K::DustPillar,
            state("moving_piston")
        ));
        assert!(!block_state_supported(K::BlockCrumble, state("barrier")));
    }

    #[test]
    fn fixed_items_resolve_by_native_registry_name() {
        let _protocol = crate::world::block::test_protocol_guard();
        let table = pomme_protocol::registries::RegistryTable::native();
        for name in ["slime_ball", "cobweb", "snowball"] {
            let id = table
                .id_of(pomme_protocol::registries::ClientRegistry::Item, name)
                .unwrap();
            assert!(
                <azalea_registry::builtin::ItemKind as azalea_registry::Registry>::from_u32(id)
                    .is_some(),
                "{name} id={id}"
            );
        }
    }

    #[test]
    fn marker_and_falling_dust_keep_distinct_quad_sizes() {
        let atlas = AtlasUVMap::test_empty();
        let marker = Particle::special(
            Kind::TerrainExtra(State {
                kind: K::BlockMarker,
                translucent: false,
            }),
            DVec3::ZERO,
            DVec3::ZERO,
            None,
            80,
            0.5,
            atlas.missing_region(),
        );
        let dust = Particle::special(
            Kind::TerrainExtra(State {
                kind: K::FallingDust,
                translucent: false,
            }),
            DVec3::ZERO,
            DVec3::ZERO,
            None,
            50,
            0.135,
            atlas.missing_region(),
        );
        assert_eq!(appearance(&marker, 0.5).unwrap().size, 0.5);
        assert_eq!(appearance(&dust, 0.0).unwrap().size, 0.0);
        assert_eq!(appearance(&dust, 0.5).unwrap().size, 0.135 * 0.32);
        assert_eq!(appearance(&dust, 1.0).unwrap().size, 0.135 * 0.64);
        assert!(!translucent(State {
            kind: K::FallingDust,
            translucent: true
        }));
        assert!(!translucent(State {
            kind: K::BlockCrumble,
            translucent: false
        }));
        assert!(translucent(State {
            kind: K::BlockCrumble,
            translucent: true
        }));
    }
}
