//! Client-local world particle sampling (Java `ClientLevel` animate/weather
//! ticks).
//!
//! This module describes spawn requests only; provider initialization and live
//! particle ownership remain in `ParticleStore`.
mod blocks;
use std::collections::HashMap;

use azalea_block::BlockState;
use azalea_core::position::BlockPos;
use glam::{DVec3, dvec3};

use crate::particle::{ParticleMode, ServerParticleKind, ServerParticleOptions};
use crate::renderer::chunk::mesher::BiomeClimate;
use crate::renderer::pipelines::weather::{Precip, precipitation_for};
use crate::world::block::{self, FluidKind};
use crate::world::chunk::ChunkStore;

#[derive(Clone, Debug)]
pub struct ParticleSpawnRequest {
    pub kind: ServerParticleKind,
    pub options: ServerParticleOptions,
    pub position: DVec3,
    pub velocity: DVec3,
    pub always_visible: bool,
}

/// Java `RedStoneOreBlock.spawnParticles`, shared by attack/use and entity
/// step-on actions. Periodic lit-ore animateTick remains a separate source.
pub fn redstone_ore_interaction_requests(
    chunks: &ChunkStore,
    pos: BlockPos,
    seed: u64,
) -> Vec<ParticleSpawnRequest> {
    let state = chunks.get_block_state(pos.x, pos.y, pos.z);
    if !matches!(
        block::block_id(state),
        "redstone_ore" | "deepslate_redstone_ore"
    ) {
        return Vec::new();
    }
    let mut rng = fastrand::Rng::with_seed(seed);
    let mut requests = Vec::new();
    for (dx, dy, dz, axis) in [
        (0, -1, 0, 1),
        (0, 1, 0, 1),
        (0, 0, -1, 2),
        (0, 0, 1, 2),
        (-1, 0, 0, 0),
        (1, 0, 0, 0),
    ] {
        let neighbor = chunks.get_block_state(pos.x + dx, pos.y + dy, pos.z + dz);
        if block::is_solid_render(neighbor).unwrap_or(false) {
            continue;
        }
        let random = |rng: &mut fastrand::Rng| f64::from(rng.f32());
        let (x, y, z) = match axis {
            0 => (
                0.5 + 0.5625 * f64::from(dx),
                random(&mut rng),
                random(&mut rng),
            ),
            1 => (
                random(&mut rng),
                0.5 + 0.5625 * f64::from(dy),
                random(&mut rng),
            ),
            _ => (
                random(&mut rng),
                random(&mut rng),
                0.5 + 0.5625 * f64::from(dz),
            ),
        };
        requests.push(ParticleSpawnRequest {
            kind: ServerParticleKind::Dust,
            options: ServerParticleOptions::Dust {
                packed_color: 0xff0000,
                scale: 1.0,
            },
            position: dvec3(pos.x as f64 + x, pos.y as f64 + y, pos.z as f64 + z),
            velocity: DVec3::ZERO,
            always_visible: false,
        });
    }
    requests
}

impl ParticleSpawnRequest {
    fn simple(kind: ServerParticleKind, position: DVec3) -> Self {
        Self {
            kind,
            options: ServerParticleOptions::Simple,
            position,
            velocity: DVec3::ZERO,
            always_visible: false,
        }
    }
}

/// Java `ClientLevel.tickWeatherEffects`: bounded random surface sampling
/// around the camera, respecting particle status, climate, loaded chunks and
/// the motion-blocking heightmap. `weather_radius` is the client option
/// (vanilla 10).
pub fn sample_weather(
    chunks: &ChunkStore,
    climates: &HashMap<u32, BiomeClimate>,
    camera: DVec3,
    rain_level: f32,
    weather_radius: i32,
    mode: ParticleMode,
    game_time: i64,
) -> Vec<ParticleSpawnRequest> {
    if rain_level <= 0.0 || weather_radius < 0 {
        return Vec::new();
    }
    let radius = weather_radius;
    let count = weather_sample_count(radius, rain_level, mode);
    let camera_block = BlockPos::new(
        camera.x.floor() as i32,
        camera.y.floor() as i32,
        camera.z.floor() as i32,
    );
    let mut rng = fastrand::Rng::with_seed((game_time as u64).wrapping_mul(312_987_231));
    let mut requests = Vec::new();
    for _ in 0..count.max(0) {
        let x = camera_block.x + rng.i32(-radius..=radius);
        let z = camera_block.z + rng.i32(-radius..=radius);
        let height = chunks.motion_blocking_height(x, z);
        if height <= chunks.min_y() || (height - camera_block.y).abs() > 10 {
            continue;
        }
        let Some(biome) = chunks
            .biome_id_checked(x, height, z)
            .and_then(|id| climates.get(&id))
        else {
            continue;
        };
        if precipitation_for(biome, height) != Precip::Rain {
            continue;
        }
        let pos = BlockPos::new(x, height - 1, z);
        let state = chunks.get_block_state(pos.x, pos.y, pos.z);
        let fluid = block::fluid(state);
        let rx = rng.f64();
        let rz = rng.f64();
        let block_top = shape_height_at(state, rx, rz);
        let fluid_top = match fluid.kind {
            FluidKind::Empty => 0.0,
            _ => f64::from(fluid.height().min(1.0)),
        };
        let y = block_top.max(fluid_top);
        let kind = if fluid.kind == FluidKind::Lava
            || block::block_id(state) == "magma_block"
            || (matches!(block::block_id(state), "campfire" | "soul_campfire")
                && block::block_properties(state).get("lit") == Some("true"))
        {
            ServerParticleKind::Smoke
        } else {
            ServerParticleKind::Rain
        };
        if mode == ParticleMode::Minimal {
            // Vanilla only records this location for rain audio in Minimal mode.
            break;
        }
        requests.push(ParticleSpawnRequest::simple(
            kind,
            dvec3(pos.x as f64 + rx, pos.y as f64 + y, pos.z as f64 + rz),
        ));
    }
    requests
}

/// Samples Java's 667 pairs of radius-16/radius-32 animateTick probes. A
/// callback receives only loaded-world positions; no global chunk scan occurs.
pub fn sample_animate_positions(
    camera: DVec3,
    mut loaded: impl FnMut(i32, i32) -> bool,
    mut visit: impl FnMut(BlockPos, i32),
    seed: u64,
) {
    let origin = BlockPos::new(
        camera.x.floor() as i32,
        camera.y.floor() as i32,
        camera.z.floor() as i32,
    );
    let mut rng = fastrand::Rng::with_seed(seed);
    for _ in 0..667 {
        for radius in [16, 32] {
            let offset = |rng: &mut fastrand::Rng| rng.i32(0..radius) - rng.i32(0..radius);
            let pos = BlockPos::new(
                origin.x + offset(&mut rng),
                origin.y + offset(&mut rng),
                origin.z + offset(&mut rng),
            );
            if loaded(pos.x.div_euclid(16), pos.z.div_euclid(16)) {
                visit(pos, radius);
            }
        }
    }
}

/// Samples Java `WaterFluid` / `LavaFluid` `animateTick` and the shared
/// shape-aware `ClientLevel.trySpawnDripParticles` branch.
pub fn sample_block_particles(
    chunks: &ChunkStore,
    climates: &HashMap<u32, BiomeClimate>,
    foliage: &crate::renderer::chunk::mesher::Colormap,
    camera: DVec3,
    seed: u64,
    raining: bool,
    thundering: bool,
    game_time: i64,
    sky_darken: u8,
    mut dripstone_attributes: impl FnMut(BlockPos) -> (ServerParticleKind, ServerParticleOptions, bool),
) -> Vec<ParticleSpawnRequest> {
    let mut positions = Vec::new();
    sample_animate_positions(
        camera,
        |x, z| {
            chunks
                .get_chunk(&azalea_core::position::ChunkPos::new(x, z))
                .is_some()
        },
        |pos, _| positions.push(pos),
        seed,
    );
    sample_block_particles_at_positions(
        chunks,
        climates,
        foliage,
        positions,
        seed,
        raining,
        thundering,
        game_time,
        sky_darken,
        dripstone_attributes,
    )
}

pub fn sample_block_particles_at_positions(
    chunks: &ChunkStore,
    climates: &HashMap<u32, BiomeClimate>,
    foliage: &crate::renderer::chunk::mesher::Colormap,
    positions: impl IntoIterator<Item = BlockPos>,
    seed: u64,
    raining: bool,
    thundering: bool,
    game_time: i64,
    sky_darken: u8,
    mut dripstone_attributes: impl FnMut(BlockPos) -> (ServerParticleKind, ServerParticleOptions, bool),
) -> Vec<ParticleSpawnRequest> {
    blocks::sample_positions(
        chunks,
        climates,
        foliage,
        positions,
        seed,
        raining,
        thundering,
        game_time,
        sky_darken,
        |pos| {
            let (kind, options, water) = dripstone_attributes(pos);
            (water, kind, options)
        },
    )
}

/// Java 26.2 `ClientLevel.getMarkerParticleTarget`: only creative players
/// holding a barrier or light block item select a local marker target.
pub fn marker_particle_target(game_mode: u8, held_item: Option<&str>) -> Option<&'static str> {
    if game_mode != 1 {
        return None;
    }
    match held_item {
        Some("barrier") => Some("barrier"),
        Some("light") => Some("light"),
        _ => None,
    }
}

pub fn sample_animate_tick_particles(
    chunks: &ChunkStore,
    camera: DVec3,
    seed: u64,
    marker_target: Option<&str>,
) -> Vec<ParticleSpawnRequest> {
    let mut positions = Vec::new();
    sample_animate_positions(
        camera,
        |x, z| {
            chunks
                .get_chunk(&azalea_core::position::ChunkPos::new(x, z))
                .is_some()
        },
        |pos, _| positions.push(pos),
        seed,
    );
    sample_animate_tick_particles_at_positions(chunks, positions, seed, marker_target)
}

pub fn sample_animate_tick_particles_at_positions(
    chunks: &ChunkStore,
    positions: impl IntoIterator<Item = BlockPos>,
    seed: u64,
    marker_target: Option<&str>,
) -> Vec<ParticleSpawnRequest> {
    let mut requests = Vec::new();
    for pos in positions {
        if pos.y < chunks.min_y() || pos.y >= chunks.min_y() + chunks.height() as i32 {
            continue;
        }
        let state = chunks.get_block_state(pos.x, pos.y, pos.z);
        if let Some(request) = marker_particle_request(marker_target, state, pos) {
            requests.push(request);
        }
        let fluid = block::fluid(state);
        if fluid.kind == FluidKind::Empty {
            continue;
        }
        let mut rng = fastrand::Rng::with_seed(
            seed ^ (pos.x as u64).rotate_left(13) ^ (pos.y as u64).rotate_left(29) ^ pos.z as u64,
        );
        match fluid.kind {
            FluidKind::Water if water_fluid_emits_underwater(state) && rng.u32(0..10) == 0 => {
                requests.push(ParticleSpawnRequest::simple(
                    ServerParticleKind::Underwater,
                    dvec3(
                        pos.x as f64 + rng.f64(),
                        pos.y as f64 + rng.f64(),
                        pos.z as f64 + rng.f64(),
                    ),
                ));
            }
            FluidKind::Lava => {
                let above = chunks.get_block_state(pos.x, pos.y + 1, pos.z);
                if (block::is_air(above) || !blocks::full_collision(above)) && rng.u32(0..100) == 0
                {
                    requests.push(ParticleSpawnRequest::simple(
                        ServerParticleKind::Lava,
                        dvec3(
                            pos.x as f64 + rng.f64(),
                            pos.y as f64 + 1.0,
                            pos.z as f64 + rng.f64(),
                        ),
                    ));
                }
            }
            _ => {}
        }
        let sample = rng.u32(0..10);
        if sample == 0 {
            let drip = match fluid.kind {
                FluidKind::Water => ServerParticleKind::DrippingWater,
                FluidKind::Lava => ServerParticleKind::DrippingLava,
                FluidKind::Empty => continue,
            };
            if let Some(position) = blocks::drip_position(chunks, pos, state, &mut rng, false) {
                requests.push(ParticleSpawnRequest::simple(drip, position));
            }
        }
    }
    requests
}

/// Java WaterFluid emits underwater particles for source or falling states;
/// ordinary flowing states only make the ambient sound.
fn water_fluid_emits_underwater(state: BlockState) -> bool {
    let fluid = block::fluid(state);
    fluid.kind == FluidKind::Water && (fluid.is_source() || fluid.falling)
}

fn marker_particle_request(
    target: Option<&str>,
    state: BlockState,
    pos: BlockPos,
) -> Option<ParticleSpawnRequest> {
    target
        .filter(|target| block::block_id(state) == *target)
        .map(|_| ParticleSpawnRequest {
            kind: ServerParticleKind::BlockMarker,
            options: ServerParticleOptions::Block(state),
            position: dvec3(pos.x as f64 + 0.5, pos.y as f64 + 0.5, pos.z as f64 + 0.5),
            velocity: DVec3::ZERO,
            always_visible: false,
        })
}

fn weather_sample_count(radius: i32, rain_level: f32, mode: ParticleMode) -> i32 {
    let diameter = radius * 2 + 1;
    let divisor = if mode == ParticleMode::Decreased {
        2
    } else {
        1
    };
    (0.225 * f64::from(diameter * diameter) * f64::from(rain_level.clamp(0.0, 1.0)).powi(2)) as i32
        / divisor
}

fn shape_height_at(state: BlockState, x: f64, z: f64) -> f64 {
    let Some(shape) = block::block_shape(state) else {
        return 1.0;
    };
    shape
        .iter()
        .filter(|bounds| x >= bounds[0] && x <= bounds[3] && z >= bounds[2] && z <= bounds[5])
        .map(|bounds| bounds[4])
        .fold(0.0, f64::max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn animate_tick_samples_exactly_667_pairs_and_never_visits_unloaded_chunks() {
        let mut visits = Vec::new();
        sample_animate_positions(
            dvec3(0.0, 64.0, 0.0),
            |x, z| x == 0 && z == 0,
            |pos, radius| visits.push((pos, radius)),
            7,
        );
        assert!(visits.len() <= 1334);
        assert!(
            visits
                .iter()
                .all(|(p, _)| p.x.div_euclid(16) == 0 && p.z.div_euclid(16) == 0)
        );
    }

    #[test]
    fn water_underwater_gate_matches_source_or_falling_for_real_block_states() {
        let _protocol = block::test_protocol_guard();
        block::init("26.2");
        for level in 0..=15 {
            let state = block::find_state("water", &[("level", &level.to_string())]);
            let fluid = block::fluid(state);
            let java_condition = fluid.is_source() || fluid.falling;
            assert_eq!(
                water_fluid_emits_underwater(state),
                java_condition,
                "water[level={level}] => amount={}, falling={}, source={}",
                fluid.amount,
                fluid.falling,
                fluid.is_source()
            );
            assert_eq!(
                water_fluid_emits_underwater(state),
                level == 0 || level >= 8
            );
        }
        let logged = block::find_state(
            "oak_stairs",
            &[
                ("facing", "north"),
                ("half", "bottom"),
                ("shape", "straight"),
                ("waterlogged", "true"),
            ],
        );
        assert!(water_fluid_emits_underwater(logged));
        assert!(!water_fluid_emits_underwater(block::find_state("air", &[])));
    }

    #[test]
    fn marker_target_matches_java_game_mode_and_held_item_gate() {
        assert_eq!(marker_particle_target(1, Some("barrier")), Some("barrier"));
        assert_eq!(marker_particle_target(1, Some("light")), Some("light"));
        for mode in [0, 2, 3, 4] {
            assert_eq!(marker_particle_target(mode, Some("barrier")), None);
            assert_eq!(marker_particle_target(mode, Some("light")), None);
        }
        for item in [None, Some("stone"), Some("debug_stick"), Some("air")] {
            assert_eq!(marker_particle_target(1, item), None);
        }
        // A held-slot change is resolved afresh; neither previous target persists.
        assert_eq!(marker_particle_target(1, Some("stone")), None);
    }

    #[test]
    fn marker_request_keeps_native_state_center_and_zero_velocity() {
        let _protocol = block::test_protocol_guard();
        block::init("26.2");
        let state = block::default_state_of("barrier").expect("native barrier state");
        let pos = BlockPos::new(-2, 70, 4);
        let request = marker_particle_request(Some("barrier"), state, pos).unwrap();
        assert!(matches!(request.kind, ServerParticleKind::BlockMarker));
        assert!(matches!(request.options, ServerParticleOptions::Block(s) if s == state));
        assert_eq!(request.position, dvec3(-1.5, 70.5, 4.5));
        assert_eq!(request.velocity, DVec3::ZERO);
        assert!(marker_particle_request(Some("light"), state, pos).is_none());
        let light = block::default_state_of("light").expect("native light state");
        let light_request = marker_particle_request(Some("light"), light, pos).unwrap();
        assert!(matches!(
            light_request.kind,
            ServerParticleKind::BlockMarker
        ));
        assert!(matches!(light_request.options, ServerParticleOptions::Block(s) if s == light));
        assert!(marker_particle_request(None, state, pos).is_none());
    }

    #[test]
    fn weather_impact_sampling_selects_rain_but_not_snow_or_dry_biomes() {
        let warm = BiomeClimate {
            temperature: 0.8,
            has_precipitation: true,
            ..BiomeClimate::default()
        };
        let cold = BiomeClimate {
            temperature: 0.0,
            has_precipitation: true,
            ..BiomeClimate::default()
        };
        let dry = BiomeClimate {
            has_precipitation: false,
            ..warm
        };
        assert!(precipitation_for(&warm, 64) == Precip::Rain);
        assert!(precipitation_for(&cold, 64) == Precip::Snow);
        assert!(precipitation_for(&dry, 64) == Precip::None);
    }

    #[test]
    fn weather_samples_scale_with_rain_status_and_default_radius() {
        assert_eq!(weather_sample_count(10, 1.0, ParticleMode::All), 99);
        assert_eq!(weather_sample_count(10, 1.0, ParticleMode::Decreased), 49);
        assert_eq!(weather_sample_count(10, 0.0, ParticleMode::All), 0);
        assert_eq!(weather_sample_count(10, 1.0, ParticleMode::Minimal), 99);
    }

    #[test]
    fn redstone_ore_action_particles_match_visible_faces_and_native_dust_payload() {
        use azalea_core::position::ChunkPos;
        use azalea_world::chunk::Chunk;

        let _protocol = block::test_protocol_guard();
        block::init("26.2");
        let pos = BlockPos::new(1, 64, 1);
        let mut chunks = ChunkStore::new(1);
        let mut chunk = Chunk::default();
        chunk.sections = vec![Default::default(); chunks.section_count() as usize].into();
        chunks.load_decoded_chunk(ChunkPos::new(0, 0), chunk);

        for (ore_id, lit) in [
            ("redstone_ore", "false"),
            ("deepslate_redstone_ore", "true"),
        ] {
            let ore = block::find_state(ore_id, &[("lit", lit)]);
            chunks.set_block_state(pos.x, pos.y, pos.z, ore);
            let requests = redstone_ore_interaction_requests(&chunks, pos, 27);
            assert_eq!(requests.len(), 6, "{ore_id}[lit={lit}]");
            assert!(requests.iter().all(|request| {
                request.kind == ServerParticleKind::Dust
                    && matches!(request.options, ServerParticleOptions::Dust { packed_color: 0xff0000, scale } if scale == 1.0)
                    && request.velocity == DVec3::ZERO
            }));
            assert!(requests.iter().any(|r| r.position.x == 0.9375));
            assert!(requests.iter().any(|r| r.position.x == 2.0625));
            assert!(requests.iter().any(|r| r.position.y == 63.9375));
            assert!(requests.iter().any(|r| r.position.y == 65.0625));
            assert!(requests.iter().any(|r| r.position.z == 0.9375));
            assert!(requests.iter().any(|r| r.position.z == 2.0625));

            let stone = block::default_state_of("stone").unwrap();
            chunks.set_block_state(pos.x - 1, pos.y, pos.z, stone);
            assert_eq!(redstone_ore_interaction_requests(&chunks, pos, 27).len(), 5);
            let glass = block::default_state_of("glass").unwrap();
            chunks.set_block_state(pos.x - 1, pos.y, pos.z, glass);
            assert_eq!(redstone_ore_interaction_requests(&chunks, pos, 27).len(), 6);
        }
        chunks.set_block_state(
            pos.x,
            pos.y,
            pos.z,
            block::default_state_of("stone").unwrap(),
        );
        assert!(redstone_ore_interaction_requests(&chunks, pos, 27).is_empty());
    }

    #[test]
    fn animate_tick_uses_vanilla_two_radii_and_no_unloaded_chunk_callback() {
        let mut radii = Vec::new();
        sample_animate_positions(
            dvec3(0.0, 64.0, 0.0),
            |_, _| true,
            |_, radius| radii.push(radius),
            1,
        );
        assert_eq!(radii.len(), 1334);
        assert_eq!(radii.iter().filter(|&&r| r == 16).count(), 667);
        assert_eq!(radii.iter().filter(|&&r| r == 32).count(), 667);
    }
}
