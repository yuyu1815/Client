//! Biome/dimension `EnvironmentAttributes.AMBIENT_PARTICLES` and animate-tick
//! sampling.
use std::collections::HashMap;

use azalea_core::position::BlockPos;
use glam::{DVec3, dvec3};

use crate::particle::{ParticleMode, ServerParticleKind, ServerParticleOptions};
use crate::world::chunk::ChunkStore;
use crate::world::particle_tick::ParticleSpawnRequest;

#[derive(Clone, Debug)]
pub struct AmbientParticle {
    pub kind: ServerParticleKind,
    pub options: ServerParticleOptions,
    pub probability: f32,
}

pub type AmbientSpawn = ParticleSpawnRequest;

/// Sample Java animate-tick probes. Biome attributes override the dimension's
/// constant map; absent biome entries inherit that dimension value.
pub fn sample(
    chunks: &ChunkStore,
    ambient: &HashMap<u32, Vec<AmbientParticle>>,
    dimension: &[AmbientParticle],
    timeline_override: Option<&[AmbientParticle]>,
    camera: DVec3,
    seed: u64,
    mode: ParticleMode,
) -> Vec<AmbientSpawn> {
    let mut probes = Vec::new();
    super::particle_tick::sample_animate_positions(
        camera,
        |x, z| {
            chunks
                .get_chunk(&azalea_core::position::ChunkPos::new(x, z))
                .is_some()
        },
        |pos, _| probes.push(pos),
        seed,
    );
    sample_positions(
        chunks,
        ambient,
        dimension,
        timeline_override,
        probes.into_iter(),
        seed,
        mode,
    )
}

/// Consume an existing animate-tick probe stream without generating another
/// scan. This is the join point for sharing probes with block/fluid sampling.
pub fn sample_positions(
    chunks: &ChunkStore,
    ambient: &HashMap<u32, Vec<AmbientParticle>>,
    dimension: &[AmbientParticle],
    timeline_override: Option<&[AmbientParticle]>,
    positions: impl IntoIterator<Item = BlockPos>,
    seed: u64,
    mode: ParticleMode,
) -> Vec<AmbientSpawn> {
    let loaded = positions.into_iter().filter(|pos| {
        chunks
            .get_chunk(&azalea_core::position::ChunkPos::new(
                pos.x.div_euclid(16),
                pos.z.div_euclid(16),
            ))
            .is_some()
    });
    sample_loaded_positions(
        chunks,
        ambient,
        dimension,
        timeline_override,
        loaded,
        seed,
        mode,
    )
}

/// As `sample_positions`, but the caller has already filtered the shared
/// animate-tick probe stream for loaded columns.
pub fn sample_loaded_positions(
    chunks: &ChunkStore,
    ambient: &HashMap<u32, Vec<AmbientParticle>>,
    dimension: &[AmbientParticle],
    timeline_override: Option<&[AmbientParticle]>,
    positions: impl IntoIterator<Item = BlockPos>,
    seed: u64,
    mode: ParticleMode,
) -> Vec<AmbientSpawn> {
    if mode == ParticleMode::Minimal
        || (timeline_override.is_none() && ambient.is_empty() && dimension.is_empty())
    {
        return Vec::new();
    }
    let mut result = Vec::new();
    for (probe_index, pos) in positions.into_iter().enumerate() {
        if pos.y < chunks.min_y() || pos.y >= chunks.min_y() + chunks.height() as i32 {
            continue;
        }
        let Some(biome_id) = chunks.biome_id_checked(pos.x, pos.y, pos.z) else {
            continue;
        };
        let particles = timeline_override.unwrap_or_else(|| {
            ambient
                .get(&u32::from(biome_id))
                .map_or(dimension, Vec::as_slice)
        });
        let state = chunks.get_block_state(pos.x, pos.y, pos.z);
        let full_collision = crate::world::block::collision_shape_is_full_block(state);
        if full_collision {
            continue;
        }
        let mut rng = fastrand::Rng::with_seed(
            seed ^ (probe_index as u64).rotate_left(7)
                ^ (pos.x as u64).rotate_left(13)
                ^ (pos.y as u64).rotate_left(29)
                ^ pos.z as u64,
        );
        for particle in particles {
            if probability_passes(rng.f32(), particle.probability) {
                result.push(AmbientSpawn {
                    kind: particle.kind,
                    options: particle.options.clone(),
                    position: dvec3(
                        pos.x as f64 + rng.f64(),
                        pos.y as f64 + rng.f64(),
                        pos.z as f64 + rng.f64(),
                    ),
                    velocity: DVec3::ZERO,
                    always_visible: false,
                });
            }
        }
    }
    result
}

#[inline]
fn probability_passes(draw: f32, probability: f32) -> bool {
    draw <= probability
}

#[cfg(test)]
mod tests {
    use azalea_core::position::{BlockPos, ChunkPos, ChunkSectionBiomePos};
    use azalea_registry::data::Biome;
    use azalea_world::chunk::Chunk;
    use azalea_world::palette::PalettedContainer;

    use super::*;

    fn world() -> ChunkStore {
        crate::world::block::init("26.2");
        let mut chunks = ChunkStore::new_with_dimension(2, 16, 0);
        let pos = ChunkPos::new(0, 0);
        let mut chunk = Chunk::default();
        chunk.sections = vec![Default::default(); chunks.section_count() as usize].into();
        chunks
            .partial_storage
            .set(&pos, Some(chunk), &mut chunks.chunk_storage);
        let mut biome = PalettedContainer::<Biome>::new();
        biome.set(ChunkSectionBiomePos { x: 0, y: 0, z: 0 }, Biome::from(0));
        let mut payload = Vec::new();
        for _ in 0..chunks.section_count() {
            biome.write(&mut payload).unwrap();
        }
        assert!(chunks.replace_biomes(pos, &payload).unwrap());
        chunks
    }

    fn particle(probability: f32) -> AmbientParticle {
        AmbientParticle {
            kind: ServerParticleKind::Ash,
            options: ServerParticleOptions::Simple,
            probability,
        }
    }

    #[test]
    fn ambient_probe_requires_loaded_chunk_and_non_full_collision() {
        let _protocol = crate::world::block::test_protocol_guard();
        let chunks = world();
        let particles = HashMap::from([(0, vec![particle(1.0)])]);
        assert!(
            sample_positions(
                &chunks,
                &particles,
                &[],
                None,
                [BlockPos::new(16, 0, 0)],
                4,
                ParticleMode::All
            )
            .is_empty()
        );
        chunks.set_block_state(
            0,
            0,
            0,
            crate::world::block::default_state_of("stone").unwrap(),
        );
        assert!(
            sample_positions(
                &chunks,
                &particles,
                &[],
                None,
                [BlockPos::new(0, 0, 0)],
                4,
                ParticleMode::All
            )
            .is_empty()
        );
        chunks.set_block_state(0, 0, 0, azalea_block::BlockState::AIR);
        assert_eq!(
            sample_positions(
                &chunks,
                &particles,
                &[],
                None,
                [BlockPos::new(0, 0, 0)],
                4,
                ParticleMode::All
            )
            .len(),
            1
        );
    }

    #[test]
    fn ambient_probability_defaults_overrides_and_positions_follow_codec_semantics() {
        let _protocol = crate::world::block::test_protocol_guard();
        assert!(!probability_passes(0.25, 0.0));
        assert!(
            probability_passes(0.0, 0.0),
            "Java uses <=, including the exact zero draw"
        );
        assert!(
            sample_positions(
                &world(),
                &HashMap::from([(0, vec![particle(0.0)])]),
                &[],
                None,
                [BlockPos::new(0, 0, 0)],
                1,
                ParticleMode::All
            )
            .is_empty()
        );
        assert!(probability_passes(0.25, 1.0));
        assert!(
            probability_passes(0.25, 0.25),
            "Java uses <= at the probability boundary"
        );
        let chunks = world();
        let pos = BlockPos::new(0, 0, 0);
        let dimension = [particle(1.0)];
        // No biome value inherits the dimension layer; an explicit empty biome
        // list replaces it; the attribute default with no dimension value is empty.
        assert_eq!(
            sample_positions(
                &chunks,
                &HashMap::new(),
                &dimension,
                None,
                [pos],
                17,
                ParticleMode::All
            )
            .len(),
            1
        );
        assert!(
            sample_positions(
                &chunks,
                &HashMap::from([(0, vec![])]),
                &dimension,
                None,
                [pos],
                17,
                ParticleMode::All
            )
            .is_empty()
        );
        assert!(
            sample_positions(
                &chunks,
                &HashMap::new(),
                &[],
                None,
                [pos],
                17,
                ParticleMode::All
            )
            .is_empty()
        );
        let timeline_value = [particle(1.0)];
        assert_eq!(
            sample_positions(
                &chunks,
                &HashMap::from([(0, vec![])]),
                &dimension,
                Some(&timeline_value),
                [pos],
                17,
                ParticleMode::All,
            )
            .len(),
            1,
            "the later timeline override wins over dimension and biome values"
        );
        assert!(
            sample_positions(
                &chunks,
                &HashMap::from([(0, vec![particle(1.0)])]),
                &dimension,
                Some(&[]),
                [pos],
                17,
                ParticleMode::All,
            )
            .is_empty(),
            "an explicit empty timeline value overrides biome and dimension"
        );
        let requests = sample_positions(
            &chunks,
            &HashMap::from([(0, vec![particle(1.0)])]),
            &[],
            None,
            std::iter::repeat_n(pos, 32),
            17,
            ParticleMode::All,
        );
        assert_eq!(requests.len(), 32);
        let mut varied = [false; 3];
        for spawn in requests {
            for (axis, value) in [spawn.position.x, spawn.position.y, spawn.position.z]
                .into_iter()
                .enumerate()
            {
                assert!((0.0..1.0).contains(&value));
                varied[axis] |= value != 0.0;
            }
        }
        assert!(varied.into_iter().all(|v| v));
    }
}
