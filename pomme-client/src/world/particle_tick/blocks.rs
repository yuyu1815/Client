//! Java 26.2 block-local sampled particle callbacks.
use azalea_block::BlockState;
use azalea_core::position::BlockPos;
use glam::{DVec3, dvec3};

use super::ParticleSpawnRequest;
use crate::particle::{ServerParticleKind as Kind, ServerParticleOptions as Options};
use crate::world::block::{self, FluidKind};
use crate::world::chunk::ChunkStore;

/// Run the block portion of `ClientLevel.doAnimateTick` on loaded samples.
pub(super) fn sample_positions(
    chunks: &ChunkStore,
    climates: &std::collections::HashMap<u32, crate::renderer::chunk::mesher::BiomeClimate>,
    foliage: &crate::renderer::chunk::mesher::Colormap,
    positions: impl IntoIterator<Item = BlockPos>,
    seed: u64,
    raining: bool,
    thundering: bool,
    game_time: i64,
    sky_darken: u8,
    mut dripstone_attributes: impl FnMut(BlockPos) -> (bool, Kind, Options),
) -> Vec<ParticleSpawnRequest> {
    let mut out = Vec::new();
    for pos in positions {
        if pos.y < chunks.min_y() || pos.y >= chunks.min_y() + chunks.height() as i32 {
            continue;
        }
        let state = chunks.get_block_state(pos.x, pos.y, pos.z);
        let id = block::block_id(state);
        let props = block::block_properties(state);
        let mut rng = fastrand::Rng::with_seed(seed ^ packed_pos(pos));
        let mut emit = |kind, options, position, velocity, always_visible| {
            out.push(ParticleSpawnRequest {
                kind,
                options,
                position,
                velocity,
                always_visible,
            });
        };
        match id {
            // BrewingStandBlock: one smoke particle at each animate sample.
            "brewing_stand" => emit(
                Kind::Smoke,
                Options::Simple,
                dvec3(
                    f(pos.x, 0.4 + rng.f32() as f64 * 0.2),
                    f(pos.y, 0.7 + rng.f32() as f64 * 0.3),
                    f(pos.z, 0.4 + rng.f32() as f64 * 0.2),
                ),
                DVec3::ZERO,
                false,
            ),
            // FurnaceBlock / BlastFurnaceBlock / SmokerBlock.
            "furnace" | "blast_furnace" | "smoker" if props.get("lit") == Some("true") => {
                let _sound_roll = rng.f64();
                let facing = horizontal(props.get("facing").unwrap_or("north"));
                let side = rng.f64() * 0.6 - 0.3;
                let yoff = rng.f64()
                    * if id == "furnace" {
                        6.0 / 16.0
                    } else {
                        9.0 / 16.0
                    };
                let (dx, dz) = if facing.0 != 0 {
                    (f64::from(facing.0) * 0.52, side)
                } else {
                    (side, f64::from(facing.1) * 0.52)
                };
                let p = dvec3(f(pos.x, 0.5 + dx), pos.y as f64 + yoff, f(pos.z, 0.5 + dz));
                emit(Kind::Smoke, Options::Simple, p, DVec3::ZERO, false);
                if id == "furnace" {
                    emit(Kind::Flame, Options::Simple, p, DVec3::ZERO, false);
                }
            }
            // TorchBlock / WallTorchBlock: fixed smoke + flame pair.
            "torch" | "soul_torch" => {
                let p = dvec3(pos.x as f64 + 0.5, pos.y as f64 + 0.7, pos.z as f64 + 0.5);
                emit(Kind::Smoke, Options::Simple, p, DVec3::ZERO, false);
                emit(
                    if id == "soul_torch" {
                        Kind::SoulFireFlame
                    } else {
                        Kind::Flame
                    },
                    Options::Simple,
                    p,
                    DVec3::ZERO,
                    false,
                );
            }
            "wall_torch" | "soul_wall_torch" => {
                let (fx, fz) = opposite_horizontal(props.get("facing").unwrap_or("north"));
                let p = dvec3(
                    f(pos.x, 0.5 + 0.27 * fx),
                    pos.y as f64 + 0.92,
                    f(pos.z, 0.5 + 0.27 * fz),
                );
                emit(Kind::Smoke, Options::Simple, p, DVec3::ZERO, false);
                emit(
                    if id == "soul_wall_torch" {
                        Kind::SoulFireFlame
                    } else {
                        Kind::Flame
                    },
                    Options::Simple,
                    p,
                    DVec3::ZERO,
                    false,
                );
            }
            // RedstoneTorchBlock / RedstoneWallTorchBlock.
            "redstone_torch" | "redstone_wall_torch" if props.get("lit") == Some("true") => {
                let wall = id == "redstone_wall_torch";
                let (fx, fz) = opposite_horizontal(props.get("facing").unwrap_or("north"));
                let p = if wall {
                    dvec3(
                        f(pos.x, 0.5 + (rng.f64() - 0.5) * 0.2 + 0.27 * fx),
                        f(pos.y, 0.92 + (rng.f64() - 0.5) * 0.2),
                        f(pos.z, 0.5 + (rng.f64() - 0.5) * 0.2 + 0.27 * fz),
                    )
                } else {
                    dvec3(
                        f(pos.x, 0.5 + (rng.f64() - 0.5) * 0.2),
                        f(pos.y, 0.7 + (rng.f64() - 0.5) * 0.2),
                        f(pos.z, 0.5 + (rng.f64() - 0.5) * 0.2),
                    )
                };
                emit(
                    Kind::Dust,
                    Options::Dust {
                        packed_color: 0xff0000,
                        scale: 1.0,
                    },
                    p,
                    DVec3::ZERO,
                    false,
                );
            }
            "repeater" if props.get("powered") == Some("true") => {
                let d = horizontal(props.get("facing").unwrap_or("north"));
                let offset = (if rng.bool() {
                    props
                        .get("delay")
                        .and_then(|x| x.parse::<f64>().ok())
                        .unwrap_or(1.0)
                        * 2.0
                        - 1.0
                } else {
                    -5.0
                }) / 16.0;
                let p = dvec3(
                    f(
                        pos.x,
                        0.5 + (rng.f64() - 0.5) * 0.2 + offset * f64::from(d.0),
                    ),
                    f(pos.y, 0.4 + (rng.f64() - 0.5) * 0.2),
                    f(
                        pos.z,
                        0.5 + (rng.f64() - 0.5) * 0.2 + offset * f64::from(d.1),
                    ),
                );
                emit(
                    Kind::Dust,
                    Options::Dust {
                        packed_color: 0xff0000,
                        scale: 1.0,
                    },
                    p,
                    DVec3::ZERO,
                    false,
                );
            }
            "mycelium" if rng.u32(0..10) == 0 => emit(
                Kind::Mycelium,
                Options::Simple,
                dvec3(f(pos.x, rng.f64()), pos.y as f64 + 1.1, f(pos.z, rng.f64())),
                DVec3::ZERO,
                false,
            ),
            "ender_chest" => {
                for _ in 0..3 {
                    let fx = sign(&mut rng);
                    let fz = sign(&mut rng);
                    emit(
                        Kind::Portal,
                        Options::Simple,
                        dvec3(
                            f(pos.x, 0.5 + 0.25 * fx),
                            f(pos.y, rng.f32() as f64),
                            f(pos.z, 0.5 + 0.25 * fz),
                        ),
                        dvec3(
                            rng.f32() as f64 * fx,
                            (rng.f32() as f64 - 0.5) * 0.125,
                            rng.f32() as f64 * fz,
                        ),
                        false,
                    );
                }
            }
            "nether_portal" => {
                for _ in 0..4 {
                    let (mut x, y, mut z) = (rng.f64(), rng.f64(), rng.f64());
                    let (mut vx, vy, mut vz) = (
                        (rng.f32() as f64 - 0.5) * 0.5,
                        (rng.f32() as f64 - 0.5) * 0.5,
                        (rng.f32() as f64 - 0.5) * 0.5,
                    );
                    let flip = sign(&mut rng);
                    if chunks.get_block_state(pos.x - 1, pos.y, pos.z) == state
                        || chunks.get_block_state(pos.x + 1, pos.y, pos.z) == state
                    {
                        z = 0.5 + 0.25 * flip;
                        vz = rng.f32() as f64 * 2.0 * flip;
                    } else {
                        x = 0.5 + 0.25 * flip;
                        vx = rng.f32() as f64 * 2.0 * flip;
                    }
                    emit(
                        Kind::Portal,
                        Options::Simple,
                        dvec3(f(pos.x, x), f(pos.y, y), f(pos.z, z)),
                        dvec3(vx, vy, vz),
                        false,
                    );
                }
            }
            "end_portal" => emit(
                Kind::Smoke,
                Options::Simple,
                dvec3(f(pos.x, rng.f64()), pos.y as f64 + 0.8, f(pos.z, rng.f64())),
                DVec3::ZERO,
                false,
            ),
            "end_rod" if rng.u32(0..5) == 0 => {
                let d = direction(props.get("facing").unwrap_or("up"));
                let r = 0.4 - (rng.f32() as f64 + rng.f32() as f64) * 0.4;
                let p = dvec3(
                    f(pos.x, 0.55 - rng.f32() as f64 * 0.1 + d.0 * r),
                    f(pos.y, 0.55 - rng.f32() as f64 * 0.1 + d.1 * r),
                    f(pos.z, 0.55 - rng.f32() as f64 * 0.1 + d.2 * r),
                );
                emit(
                    Kind::EndRod,
                    Options::Simple,
                    p,
                    dvec3(
                        gaussian(&mut rng) * 0.005,
                        gaussian(&mut rng) * 0.005,
                        gaussian(&mut rng) * 0.005,
                    ),
                    false,
                );
            }
            "respawn_anchor"
                if props
                    .get("charges")
                    .and_then(|x| x.parse::<u8>().ok())
                    .unwrap_or(0)
                    > 0 =>
            {
                emit(
                    Kind::ReversePortal,
                    Options::Simple,
                    dvec3(
                        f(pos.x, 1.0 - rng.f64()),
                        pos.y as f64 + 1.0,
                        f(pos.z, 1.0 - rng.f64()),
                    ),
                    dvec3(0.0, rng.f32() as f64 * 0.04, 0.0),
                    false,
                )
            }
            "sculk_sensor"
                if props.get("sculk_sensor_phase") == Some("active")
                    || props.get("phase") == Some("active") =>
            {
                let dir = rng.u32(0..6) as usize;
                if dir < 2 {
                    continue;
                }
                let d = [(0, 0), (0, 0), (0, -1), (0, 1), (-1, 0), (1, 0)][dir];
                let x = if d.0 == 0 {
                    1.0 - rng.f64()
                } else {
                    0.5 + f64::from(d.0) * 0.6
                };
                let z = if d.1 == 0 {
                    1.0 - rng.f64()
                } else {
                    0.5 + f64::from(d.1) * 0.6
                };
                emit(
                    Kind::DustColorTransition,
                    Options::DustColorTransition {
                        from_color: 3790560,
                        to_color: 0xff0000,
                        scale: 1.0,
                    },
                    dvec3(f(pos.x, x), pos.y as f64 + 0.25, f(pos.z, z)),
                    dvec3(0.0, rng.f32() as f64 * 0.04, 0.0),
                    false,
                );
            }
            "wet_sponge" => wet_sponge(chunks, pos, state, &mut rng, &mut emit),
            "redstone_wire" => redstone_wire(pos, state, &mut rng, &mut emit),
            "redstone_ore" | "deepslate_redstone_ore"
                if props.get("lit") == Some("true") =>
            {
                redstone_ore(chunks, pos, &mut rng, &mut emit)
            }
            "lever" if props.get("powered") == Some("true") && rng.f32() < 0.25 => {
                let face = props.get("face").unwrap_or("wall");
                let facing = direction(props.get("facing").unwrap_or("north"));
                let connected = if face == "floor" {
                    (0.0, 1.0, 0.0)
                } else if face == "ceiling" {
                    (0.0, -1.0, 0.0)
                } else {
                    facing
                };
                let connect = (-connected.0, -connected.1, -connected.2);
                let p = dvec3(
                    pos.x as f64 + 0.5 + 0.1 * (-facing.0) + 0.2 * connect.0,
                    pos.y as f64 + 0.5 + 0.1 * (-facing.1) + 0.2 * connect.1,
                    pos.z as f64 + 0.5 + 0.1 * (-facing.2) + 0.2 * connect.2,
                );
                emit(
                    Kind::Dust,
                    Options::Dust {
                        packed_color: 0xff0000,
                        scale: 0.5,
                    },
                    p,
                    DVec3::ZERO,
                    false,
                );
            }
            "oak_leaves"
            | "spruce_leaves"
            | "birch_leaves"
            | "jungle_leaves"
            | "acacia_leaves"
            | "dark_oak_leaves"
            | "cherry_leaves"
            | "pale_oak_leaves"
            | "mangrove_leaves"
            | "azalea_leaves"
            | "flowering_azalea_leaves" => sample_leaves(
                chunks, climates, foliage, pos, id, raining, &mut rng, &mut emit,
            ),
            _ if is_candle(id) && props.get("lit") == Some("true") => {
                sample_candles(pos, id, props, &mut rng, &mut emit)
            }
            "bubble_column" => {
                if props.get("drag") == Some("true") {
                    emit(
                        Kind::CurrentDown,
                        Options::Simple,
                        dvec3(pos.x as f64 + 0.5, pos.y as f64 + 0.8, pos.z as f64),
                        DVec3::ZERO,
                        true,
                    );
                } else {
                    emit(
                        Kind::BubbleColumnUp,
                        Options::Simple,
                        dvec3(pos.x as f64 + 0.5, pos.y as f64, pos.z as f64 + 0.5),
                        dvec3(0.0, 0.04, 0.0),
                        true,
                    );
                    emit(
                        Kind::BubbleColumnUp,
                        Options::Simple,
                        dvec3(
                            f(pos.x, rng.f32() as f64),
                            f(pos.y, rng.f32() as f64),
                            f(pos.z, rng.f32() as f64),
                        ),
                        dvec3(0.0, 0.04, 0.0),
                        true,
                    );
                }
            }
            "beehive" | "bee_nest"
                if props
                    .get("honey_level")
                    .and_then(|v| v.parse::<u8>().ok())
                    .unwrap_or(0)
                    >= 5 =>
            {
                if block::fluid(state).kind == FluidKind::Empty && rng.f32() >= 0.3 {
                    if let Some(p) = drip_position(chunks, pos, state, &mut rng, true) {
                        emit(Kind::DrippingHoney, Options::Simple, p, DVec3::ZERO, false);
                    }
                }
            }
            "brushable_block"
            | "suspicious_sand"
            | "suspicious_gravel"
            | "sand"
            | "red_sand"
            | "gravel"
            | "white_concrete_powder"
            | "orange_concrete_powder"
            | "magenta_concrete_powder"
            | "light_blue_concrete_powder"
            | "yellow_concrete_powder"
            | "lime_concrete_powder"
            | "pink_concrete_powder"
            | "gray_concrete_powder"
            | "light_gray_concrete_powder"
            | "cyan_concrete_powder"
            | "purple_concrete_powder"
            | "blue_concrete_powder"
            | "brown_concrete_powder"
            | "green_concrete_powder"
            | "red_concrete_powder"
            | "black_concrete_powder"
                if rng.u32(0..16) == 0
                    && is_free(chunks.get_block_state(pos.x, pos.y - 1, pos.z)) =>
            {
                emit(
                    Kind::FallingDust,
                    Options::Block(state),
                    dvec3(
                        f(pos.x, rng.f64()),
                        pos.y as f64 - 0.05,
                        f(pos.z, rng.f64()),
                    ),
                    DVec3::ZERO,
                    false,
                )
            }
            "enchanting_table" => enchanting_table(chunks, pos, &mut rng, &mut emit),
            "end_gateway"
                if chunks.block_entities.get(&pos).is_some_and(|be| {
                    be.kind == azalea_registry::builtin::BlockEntityKind::EndGateway
                }) =>
            {
                end_gateway(chunks, pos, &mut rng, &mut emit)
            }
            "crying_obsidian" if rng.u32(0..5) == 0 => {
                crying_obsidian(chunks, pos, &mut rng, &mut emit)
            }
            "pointed_dripstone" => pointed_dripstone(
                chunks,
                pos,
                state,
                &mut dripstone_attributes,
                &mut rng,
                &mut emit,
            ),
            "potent_sulfur"
                if props.get("potent_sulfur_state") != Some("dry") && {
                    let s = chunks.get_block_state(pos.x, pos.y + 1, pos.z);
                    let f = block::fluid(s);
                    f.kind == FluidKind::Water && f.amount == 8 && !f.falling
                } =>
            {
                for _ in 0..2 {
                    emit(
                        Kind::SulfurBubbles,
                        Options::Simple,
                        dvec3(
                            f(pos.x, rng.f32() as f64),
                            pos.y as f64 + 1.0 + rng.f32() as f64,
                            f(pos.z, rng.f32() as f64),
                        ),
                        DVec3::ZERO,
                        true,
                    );
                }
            }
            "lightning_rod"
                if thundering
                    && rng.u32(0..200) as i64 <= game_time.rem_euclid(200)
                    && is_world_surface_rod(
                        chunks.heightmap_height(
                            azalea_core::heightmap_kind::HeightmapKind::WorldSurface,
                            pos.x,
                            pos.z,
                        ),
                        pos.y,
                    ) =>
            {
                lightning_rod(
                    pos,
                    props.get("facing").unwrap_or("up"),
                    &mut rng,
                    &mut emit,
                )
            }
            "fire" | "soul_fire" => base_fire(chunks, pos, &mut rng, &mut emit),

            "wither_rose" => {
                for _ in 0..3 {
                    if rng.bool() {
                        emit(
                            Kind::Smoke,
                            Options::Simple,
                            dvec3(
                                f(pos.x, 0.5 + rng.f64() / 5.0),
                                pos.y as f64 + 0.5 - rng.f64(),
                                f(pos.z, 0.5 + rng.f64() / 5.0),
                            ),
                            DVec3::ZERO,
                            false,
                        );
                    }
                }
            }
            "firefly_bush"
                if is_firefly_dark(
                    chunks.get_sky_light(pos.x, pos.y, pos.z),
                    chunks.get_block_light(pos.x, pos.y, pos.z),
                    sky_darken,
                ) && rng.f64() <= 0.7 =>
            {
                emit(
                    Kind::Firefly,
                    Options::Simple,
                    dvec3(
                        f(pos.x, rng.f64() * 10.0 - 5.0),
                        f(pos.y, rng.f64() * 5.0),
                        f(pos.z, rng.f64() * 10.0 - 5.0),
                    ),
                    DVec3::ZERO,
                    false,
                )
            }
            "campfire" if props.get("lit") == Some("true") && rng.u32(0..5) == 0 => {
                let p = dvec3(pos.x as f64 + 0.5, pos.y as f64 + 0.5, pos.z as f64 + 0.5);
                for _ in 0..(rng.u32(0..1) + 1) {
                    emit(
                        Kind::Lava,
                        Options::Simple,
                        p,
                        dvec3(rng.f32() as f64 / 2.0, 5.0e-5, rng.f32() as f64 / 2.0),
                        false,
                    );
                }
            }
            "soul_campfire" => {}
            "dried_ghast" => dried_ghast(pos, props, &mut rng, &mut emit),
            "spore_blossom" => spore_blossom(chunks, pos, &mut rng, &mut emit),
            _ => {}
        }
        // Handled sound-only or data-dependent class callbacks have no generic
        // fallback.
        let _ = (raining, thundering, game_time);
    }
    out
}

const JAVA_26_2_FLAMMABLE_BLOCKS: [&str; 175] = [
    "oak_planks",
    "spruce_planks",
    "birch_planks",
    "jungle_planks",
    "acacia_planks",
    "cherry_planks",
    "dark_oak_planks",
    "pale_oak_planks",
    "mangrove_planks",
    "bamboo_planks",
    "bamboo_mosaic",
    "oak_slab",
    "spruce_slab",
    "birch_slab",
    "jungle_slab",
    "acacia_slab",
    "cherry_slab",
    "dark_oak_slab",
    "pale_oak_slab",
    "mangrove_slab",
    "bamboo_slab",
    "bamboo_mosaic_slab",
    "oak_fence_gate",
    "spruce_fence_gate",
    "birch_fence_gate",
    "jungle_fence_gate",
    "acacia_fence_gate",
    "cherry_fence_gate",
    "dark_oak_fence_gate",
    "pale_oak_fence_gate",
    "mangrove_fence_gate",
    "bamboo_fence_gate",
    "oak_fence",
    "spruce_fence",
    "birch_fence",
    "jungle_fence",
    "acacia_fence",
    "cherry_fence",
    "dark_oak_fence",
    "pale_oak_fence",
    "mangrove_fence",
    "bamboo_fence",
    "oak_stairs",
    "birch_stairs",
    "spruce_stairs",
    "jungle_stairs",
    "acacia_stairs",
    "cherry_stairs",
    "dark_oak_stairs",
    "pale_oak_stairs",
    "mangrove_stairs",
    "bamboo_stairs",
    "bamboo_mosaic_stairs",
    "oak_log",
    "spruce_log",
    "birch_log",
    "jungle_log",
    "acacia_log",
    "cherry_log",
    "pale_oak_log",
    "dark_oak_log",
    "mangrove_log",
    "bamboo_block",
    "stripped_oak_log",
    "stripped_spruce_log",
    "stripped_birch_log",
    "stripped_jungle_log",
    "stripped_acacia_log",
    "stripped_cherry_log",
    "stripped_dark_oak_log",
    "stripped_pale_oak_log",
    "stripped_mangrove_log",
    "stripped_bamboo_block",
    "stripped_oak_wood",
    "stripped_spruce_wood",
    "stripped_birch_wood",
    "stripped_jungle_wood",
    "stripped_acacia_wood",
    "stripped_cherry_wood",
    "stripped_dark_oak_wood",
    "stripped_pale_oak_wood",
    "stripped_mangrove_wood",
    "oak_wood",
    "spruce_wood",
    "birch_wood",
    "jungle_wood",
    "acacia_wood",
    "cherry_wood",
    "pale_oak_wood",
    "dark_oak_wood",
    "mangrove_wood",
    "mangrove_roots",
    "oak_leaves",
    "spruce_leaves",
    "birch_leaves",
    "jungle_leaves",
    "acacia_leaves",
    "cherry_leaves",
    "dark_oak_leaves",
    "pale_oak_leaves",
    "mangrove_leaves",
    "bookshelf",
    "tnt",
    "short_grass",
    "fern",
    "dead_bush",
    "short_dry_grass",
    "tall_dry_grass",
    "sunflower",
    "lilac",
    "rose_bush",
    "peony",
    "tall_grass",
    "large_fern",
    "dandelion",
    "golden_dandelion",
    "poppy",
    "open_eyeblossom",
    "closed_eyeblossom",
    "blue_orchid",
    "allium",
    "azure_bluet",
    "red_tulip",
    "orange_tulip",
    "white_tulip",
    "pink_tulip",
    "oxeye_daisy",
    "cornflower",
    "lily_of_the_valley",
    "torchflower",
    "pitcher_plant",
    "wither_rose",
    "pink_petals",
    "wildflowers",
    "leaf_litter",
    "cactus_flower",
    "vine",
    "coal_block",
    "hay_block",
    "target",
    "pale_moss_block",
    "pale_moss_carpet",
    "pale_hanging_moss",
    "dried_kelp_block",
    "bamboo",
    "scaffolding",
    "lectern",
    "composter",
    "sweet_berry_bush",
    "beehive",
    "bee_nest",
    "azalea_leaves",
    "flowering_azalea_leaves",
    "cave_vines",
    "cave_vines_plant",
    "spore_blossom",
    "azalea",
    "flowering_azalea",
    "big_dripleaf",
    "big_dripleaf_stem",
    "small_dripleaf",
    "hanging_roots",
    "glow_lichen",
    "firefly_bush",
    "bush",
    "acacia_shelf",
    "bamboo_shelf",
    "birch_shelf",
    "cherry_shelf",
    "dark_oak_shelf",
    "jungle_shelf",
    "mangrove_shelf",
    "oak_shelf",
    "pale_oak_shelf",
    "spruce_shelf",
];

fn dried_ghast(
    pos: BlockPos,
    props: &crate::world::block::PropMap,
    r: &mut fastrand::Rng,
    emit: &mut impl FnMut(Kind, Options, DVec3, DVec3, bool),
) {
    let p = dvec3(pos.x as f64 + 0.5, pos.y as f64 + 0.5, pos.z as f64 + 0.5);
    if props.get("waterlogged") == Some("true") {
        if r.u32(0..6) == 0 {
            emit(
                Kind::HappyVillager,
                Options::Simple,
                dvec3(
                    pos.x as f64 + 0.5 + (r.f32() as f64 * 2.0 - 1.0) / 3.0,
                    pos.y as f64 + 0.9,
                    pos.z as f64 + 0.5 + (r.f32() as f64 * 2.0 - 1.0) / 3.0,
                ),
                dvec3(0.0, r.f32() as f64, 0.0),
                false,
            );
        }
    } else if r.u32(0..6) == 0 {
        emit(
            Kind::WhiteSmoke,
            Options::Simple,
            p,
            dvec3(0.0, 0.02, 0.0),
            false,
        );
    }
}

fn spore_blossom(
    chunks: &ChunkStore,
    pos: BlockPos,
    r: &mut fastrand::Rng,
    emit: &mut impl FnMut(Kind, Options, DVec3, DVec3, bool),
) {
    emit(
        Kind::FallingSporeBlossom,
        Options::Simple,
        dvec3(
            pos.x as f64 + r.f64(),
            pos.y as f64 + 0.7,
            pos.z as f64 + r.f64(),
        ),
        DVec3::ZERO,
        false,
    );
    for _ in 0..14 {
        let p = BlockPos::new(
            pos.x + r.i32(-10..=10),
            pos.y - r.i32(0..=9),
            pos.z + r.i32(-10..=10),
        );
        if !full_collision(chunks.get_block_state(p.x, p.y, p.z)) {
            emit(
                Kind::SporeBlossomAir,
                Options::Simple,
                dvec3(
                    p.x as f64 + r.f64(),
                    p.y as f64 + r.f64(),
                    p.z as f64 + r.f64(),
                ),
                DVec3::ZERO,
                false,
            );
        }
    }
}

fn enchanting_table(
    chunks: &ChunkStore,
    pos: BlockPos,
    r: &mut fastrand::Rng,
    emit: &mut impl FnMut(Kind, Options, DVec3, DVec3, bool),
) {
    for y in 0..=1 {
        for x in -2i32..=2 {
            for z in -2i32..=2 {
                if x.abs() != 2 && z.abs() != 2 {
                    continue;
                }
                if r.u32(0..16) != 0 {
                    continue;
                }
                let shelf = chunks.get_block_state(pos.x + x, pos.y + y, pos.z + z);
                if block::block_id(shelf) != "bookshelf" {
                    continue;
                }
                let through = chunks.get_block_state(pos.x + x / 2, pos.y + y, pos.z + z / 2);
                if !block::is_replaceable(through) {
                    continue;
                }
                emit(
                    Kind::Enchant,
                    Options::Simple,
                    dvec3(pos.x as f64 + 0.5, pos.y as f64 + 2.0, pos.z as f64 + 0.5),
                    dvec3(
                        x as f64 + r.f32() as f64 - 0.5,
                        y as f64 - r.f32() as f64 - 1.0,
                        z as f64 + r.f32() as f64 - 0.5,
                    ),
                    false,
                );
            }
        }
    }
}

fn end_gateway(
    chunks: &ChunkStore,
    pos: BlockPos,
    r: &mut fastrand::Rng,
    emit: &mut impl FnMut(Kind, Options, DVec3, DVec3, bool),
) {
    let dirs = [
        (0, -1, 0, 0),
        (0, 1, 0, 1),
        (0, 0, -1, 2),
        (0, 0, 1, 3),
        (-1, 0, 0, 4),
        (1, 0, 0, 5),
    ];
    let state = chunks.get_block_state(pos.x, pos.y, pos.z);
    for (dx, dy, dz, face) in dirs {
        let n = chunks.get_block_state(pos.x + dx, pos.y + dy, pos.z + dz);
        if block::shape_occludes(state, n, face) {
            continue;
        }
        let (mut x, y, mut z) = (r.f64(), r.f64(), r.f64());
        let (mut vx, vy, mut vz) = (
            (r.f64() - 0.5) * 0.5,
            (r.f64() - 0.5) * 0.5,
            (r.f64() - 0.5) * 0.5,
        );
        let flip = sign(r);
        if r.bool() {
            z = 0.5 + 0.25 * flip;
            vz = r.f32() as f64 * 2.0 * flip;
        } else {
            x = 0.5 + 0.25 * flip;
            vx = r.f32() as f64 * 2.0 * flip;
        }
        emit(
            Kind::Portal,
            Options::Simple,
            dvec3(pos.x as f64 + x, pos.y as f64 + y, pos.z as f64 + z),
            dvec3(vx, vy, vz),
            false,
        );
    }
}

fn pointed_dripstone_particle(
    fluid: FluidKind,
    above_is_mud: bool,
    water_evaporates: bool,
    roll: f32,
    default_particle: (Kind, Options),
) -> Option<(Kind, Options)> {
    match if above_is_mud && !water_evaporates {
        FluidKind::Water
    } else {
        fluid
    } {
        FluidKind::Water => Some((Kind::DrippingDripstoneWater, Options::Simple)),
        FluidKind::Lava => Some((Kind::DrippingDripstoneLava, Options::Simple)),
        FluidKind::Empty if roll < 0.02 => Some(default_particle),
        FluidKind::Empty => None,
    }
}

fn pointed_dripstone(
    chunks: &ChunkStore,
    pos: BlockPos,
    state: BlockState,
    dripstone_attributes: &mut impl FnMut(BlockPos) -> (bool, Kind, Options),
    r: &mut fastrand::Rng,
    emit: &mut impl FnMut(Kind, Options, DVec3, DVec3, bool),
) {
    let p = block::block_properties(state);
    if p.get("tip_direction") != Some("down")
        || p.get("thickness") != Some("tip")
        || p.get("waterlogged") == Some("true")
    {
        return;
    }
    let (water_evaporates, default_kind, default_options) = dripstone_attributes(pos);
    let roll = r.f32();
    if roll > 0.12 {
        return;
    }
    let mut root = pos;
    for _ in 0..11 {
        let up = BlockPos::new(root.x, root.y + 1, root.z);
        let s = chunks.get_block_state(up.x, up.y, up.z);
        if block::block_id(s) == "pointed_dripstone"
            && block::block_properties(s).get("tip_direction") == Some("down")
        {
            root = up;
        } else {
            break;
        }
    }
    let above = chunks.get_block_state(root.x, root.y + 1, root.z);
    let above_id = block::block_id(above);
    let fluid = block::fluid(above);
    let Some((kind, options)) = pointed_dripstone_particle(
        fluid.kind,
        above_id == "mud",
        water_evaporates,
        roll,
        (default_kind, default_options),
    ) else {
        return;
    };
    let offset = block::block_offset(state, pos);
    emit(
        kind,
        options,
        dvec3(
            pos.x as f64 + 0.5 + offset.x,
            pos.y as f64 + 4.0 / 16.0,
            pos.z as f64 + 0.5 + offset.z,
        ),
        DVec3::ZERO,
        false,
    );
}

fn lightning_rod(
    pos: BlockPos,
    facing: &str,
    r: &mut fastrand::Rng,
    emit: &mut impl FnMut(Kind, Options, DVec3, DVec3, bool),
) {
    let axis = direction(facing);
    let n = r.u32(1..=2);
    for _ in 0..n {
        let x = r.f64() * 2.0 - 1.0;
        let y = r.f64() * 2.0 - 1.0;
        let z = r.f64() * 2.0 - 1.0;
        let (pvx, pvy, pvz) = if axis.0 != 0.0 {
            (x * 0.5, y * 0.125, z * 0.125)
        } else if axis.1 != 0.0 {
            (x * 0.125, y * 0.5, z * 0.125)
        } else {
            (x * 0.125, y * 0.125, z * 0.5)
        };
        let vx = if axis.0 != 0.0 {
            r.f64() * 2.0 - 1.0
        } else {
            0.0
        };
        let vy = if axis.1 != 0.0 {
            r.f64() * 2.0 - 1.0
        } else {
            0.0
        };
        let vz = if axis.2 != 0.0 {
            r.f64() * 2.0 - 1.0
        } else {
            0.0
        };
        emit(
            Kind::ElectricSpark,
            Options::Simple,
            dvec3(
                pos.x as f64 + 0.5 + pvx,
                pos.y as f64 + 0.5 + pvy,
                pos.z as f64 + 0.5 + pvz,
            ),
            dvec3(vx, vy, vz),
            false,
        );
    }
}

fn is_candle(id: &str) -> bool {
    matches!(
        id,
        "candle"
            | "white_candle"
            | "orange_candle"
            | "magenta_candle"
            | "light_blue_candle"
            | "yellow_candle"
            | "lime_candle"
            | "pink_candle"
            | "gray_candle"
            | "light_gray_candle"
            | "cyan_candle"
            | "purple_candle"
            | "blue_candle"
            | "brown_candle"
            | "green_candle"
            | "red_candle"
            | "black_candle"
            | "candle_cake"
            | "white_candle_cake"
            | "orange_candle_cake"
            | "magenta_candle_cake"
            | "light_blue_candle_cake"
            | "yellow_candle_cake"
            | "lime_candle_cake"
            | "pink_candle_cake"
            | "gray_candle_cake"
            | "light_gray_candle_cake"
            | "cyan_candle_cake"
            | "purple_candle_cake"
            | "blue_candle_cake"
            | "brown_candle_cake"
            | "green_candle_cake"
            | "red_candle_cake"
            | "black_candle_cake"
    )
}

fn candle_offsets(count: usize) -> &'static [(f64, f64, f64)] {
    const A: &[(f64, f64, f64)] = &[(0.5, 0.5, 0.5)];
    const B: &[(f64, f64, f64)] = &[
        (6.0 / 16.0, 7.0 / 16.0, 8.0 / 16.0),
        (10.0 / 16.0, 8.0 / 16.0, 7.0 / 16.0),
    ];
    const C: &[(f64, f64, f64)] = &[
        (8.0 / 16.0, 5.0 / 16.0, 10.0 / 16.0),
        (6.0 / 16.0, 7.0 / 16.0, 8.0 / 16.0),
        (9.0 / 16.0, 8.0 / 16.0, 7.0 / 16.0),
    ];
    const D: &[(f64, f64, f64)] = &[
        (7.0 / 16.0, 5.0 / 16.0, 9.0 / 16.0),
        (10.0 / 16.0, 7.0 / 16.0, 9.0 / 16.0),
        (6.0 / 16.0, 7.0 / 16.0, 6.0 / 16.0),
        (9.0 / 16.0, 8.0 / 16.0, 6.0 / 16.0),
    ];
    [A, B, C, D][count.clamp(1, 4) - 1]
}

fn candle_smoke_roll(chance: f32) -> bool {
    chance < 0.3
}

fn sample_candles(
    pos: BlockPos,
    id: &str,
    props: &crate::world::block::PropMap,
    r: &mut fastrand::Rng,
    emit: &mut impl FnMut(Kind, Options, DVec3, DVec3, bool),
) {
    let cake = matches!(
        id,
        "candle_cake"
            | "white_candle_cake"
            | "orange_candle_cake"
            | "magenta_candle_cake"
            | "light_blue_candle_cake"
            | "yellow_candle_cake"
            | "lime_candle_cake"
            | "pink_candle_cake"
            | "gray_candle_cake"
            | "light_gray_candle_cake"
            | "cyan_candle_cake"
            | "purple_candle_cake"
            | "blue_candle_cake"
            | "brown_candle_cake"
            | "green_candle_cake"
            | "red_candle_cake"
            | "black_candle_cake"
    );
    let count = if cake {
        1
    } else {
        props
            .get("candles")
            .and_then(|x| x.parse::<usize>().ok())
            .unwrap_or(1)
    };
    for (x, y, z) in candle_offsets(count) {
        let chance = r.f32();
        let p = dvec3(pos.x as f64 + x, pos.y as f64 + y, pos.z as f64 + z);
        if candle_smoke_roll(chance) {
            emit(Kind::Smoke, Options::Simple, p, DVec3::ZERO, false);
            if chance < 0.17 {
                let _sound_pitch = r.f32();
                let _sound_volume = r.f32();
            }
        }
        emit(Kind::SmallFlame, Options::Simple, p, DVec3::ZERO, false);
    }
}

fn sample_leaves(
    chunks: &ChunkStore,
    climates: &std::collections::HashMap<u32, crate::renderer::chunk::mesher::BiomeClimate>,
    foliage: &crate::renderer::chunk::mesher::Colormap,
    pos: BlockPos,
    id: &str,
    raining: bool,
    r: &mut fastrand::Rng,
    emit: &mut impl FnMut(Kind, Options, DVec3, DVec3, bool),
) {
    let below = BlockPos::new(pos.x, pos.y - 1, pos.z);
    let state = chunks.get_block_state(below.x, below.y, below.z);
    let rain_biome = chunks
        .biome_id_checked(pos.x, pos.y + 1, pos.z)
        .and_then(|id| climates.get(&id))
        .is_some_and(|c| {
            crate::renderer::pipelines::weather::precipitation_for(c, pos.y + 1)
                == crate::renderer::pipelines::weather::Precip::Rain
        });
    let can_see_rain = rain_exposed(
        chunks.heightmap_height(
            azalea_core::heightmap_kind::HeightmapKind::MotionBlockingNoLeaves,
            pos.x,
            pos.z,
        ),
        pos.y,
    );
    if raining
        && rain_biome
        && can_see_rain
        && r.u32(0..15) == 1
        && !(block::light_props(state).can_occlude && face_sturdy(state, 1))
    {
        emit(
            Kind::DrippingWater,
            Options::Simple,
            drip_particle_below(pos, r),
            DVec3::ZERO,
            false,
        );
    }
    let chance = match id {
        "cherry_leaves" => 0.1,
        "pale_oak_leaves" => 0.02,
        _ => 0.01,
    };
    if r.f32() >= chance || face_full(state, 1) {
        return;
    }
    let kind = match id {
        "cherry_leaves" => Kind::CherryLeaves,
        "pale_oak_leaves" => Kind::PaleOakLeaves,
        _ => Kind::TintedLeaves,
    };
    let options = if kind == Kind::TintedLeaves {
        let rgb = crate::renderer::chunk::mesher::blend_color(pos.x, pos.z, |x, z| {
            chunks
                .biome_id_checked(x, pos.y, z)
                .and_then(|id| climates.get(&id))
                .map(|c| crate::renderer::chunk::mesher::foliage_color(c, foliage))
                .unwrap_or([0.0; 3])
        });
        let packed = 0xff00_0000u32
            | (((rgb[0] * 255.0) as u32) << 16)
            | (((rgb[1] * 255.0) as u32) << 8)
            | (rgb[2] * 255.0) as u32;
        Options::Color {
            color: packed as i32,
        }
    } else {
        Options::Simple
    };
    emit(
        kind,
        options,
        drip_particle_below(pos, r),
        DVec3::ZERO,
        false,
    );
}

fn redstone_wire(
    pos: BlockPos,
    state: BlockState,
    r: &mut fastrand::Rng,
    emit: &mut impl FnMut(Kind, Options, DVec3, DVec3, bool),
) {
    let power = block::block_properties(state)
        .get("power")
        .and_then(|x| x.parse::<u8>().ok())
        .unwrap_or(0);
    if power == 0 {
        return;
    }
    let c = block::redstone_wire_rgb(state);
    let rgb = ((c[0] * 255.0) as i32) << 16 | ((c[1] * 255.0) as i32) << 8 | (c[2] * 255.0) as i32;
    for (name, dx, dz) in [
        ("north", 0, -1),
        ("east", 1, 0),
        ("south", 0, 1),
        ("west", -1, 0),
    ] {
        let conn = block::block_properties(state).get(name).unwrap_or("none");
        if conn == "up" {
            wire_line(pos, (dx, 0, dz), (0, 1, 0), -0.5, 0.5, rgb, r, emit);
        }
        if conn == "up" || conn == "side" {
            wire_line(pos, (0, -1, 0), (dx, 0, dz), 0.0, 0.5, rgb, r, emit);
        } else {
            wire_line(pos, (0, -1, 0), (dx, 0, dz), 0.0, 0.3, rgb, r, emit);
        }
    }
}

fn redstone_ore(
    chunks: &ChunkStore,
    pos: BlockPos,
    r: &mut fastrand::Rng,
    emit: &mut impl FnMut(Kind, Options, DVec3, DVec3, bool),
) {
    let dirs = [
        (0, -1, 0),
        (0, 1, 0),
        (0, 0, -1),
        (0, 0, 1),
        (-1, 0, 0),
        (1, 0, 0),
    ];
    for (dx, dy, dz) in dirs {
        let n = chunks.get_block_state(pos.x + dx, pos.y + dy, pos.z + dz);
        if block::is_solid_render(n) != Some(false) {
            continue;
        }
        let x = if dx != 0 {
            0.5 + 0.5625 * f64::from(dx)
        } else {
            r.f64()
        };
        let y = if dy != 0 {
            0.5 + 0.5625 * f64::from(dy)
        } else {
            r.f64()
        };
        let z = if dz != 0 {
            0.5 + 0.5625 * f64::from(dz)
        } else {
            r.f64()
        };
        emit(
            Kind::Dust,
            Options::Dust {
                packed_color: 0xff0000,
                scale: 1.0,
            },
            dvec3(pos.x as f64 + x, pos.y as f64 + y, pos.z as f64 + z),
            DVec3::ZERO,
            false,
        );
    }
}

fn can_burn(state: BlockState) -> bool {
    JAVA_26_2_FLAMMABLE_BLOCKS.contains(&block::block_id(state))
}

fn base_fire(
    chunks: &ChunkStore,
    pos: BlockPos,
    r: &mut fastrand::Rng,
    emit: &mut impl FnMut(Kind, Options, DVec3, DVec3, bool),
) {
    if r.u32(0..24) == 0 {
        let _volume = r.f32();
        let _pitch = r.f32();
    }
    let below = BlockPos::new(pos.x, pos.y - 1, pos.z);
    let bs = chunks.get_block_state(below.x, below.y, below.z);
    if can_burn(bs) || face_sturdy(bs, 1) {
        for _ in 0..3 {
            emit(
                Kind::LargeSmoke,
                Options::Simple,
                dvec3(
                    pos.x as f64 + r.f64(),
                    pos.y as f64 + r.f64() * 0.5 + 0.5,
                    pos.z as f64 + r.f64(),
                ),
                DVec3::ZERO,
                false,
            );
        }
        return;
    }
    for (dx, dz, side) in [(-1, 0, 0), (1, 0, 1), (0, -1, 2), (0, 1, 3)] {
        let n = chunks.get_block_state(pos.x + dx, pos.y, pos.z + dz);
        if !can_burn(n) {
            continue;
        }
        for _ in 0..2 {
            let x = match side {
                0 => r.f64() * 0.1,
                1 => 1.0 - r.f64() * 0.1,
                _ => r.f64(),
            };
            let y = r.f64();
            let z = match side {
                2 => r.f64() * 0.1,
                3 => 1.0 - r.f64() * 0.1,
                _ => r.f64(),
            };
            emit(
                Kind::LargeSmoke,
                Options::Simple,
                dvec3(pos.x as f64 + x, pos.y as f64 + y, pos.z as f64 + z),
                DVec3::ZERO,
                false,
            );
        }
    }
    if can_burn(chunks.get_block_state(pos.x, pos.y + 1, pos.z)) {
        for _ in 0..2 {
            emit(
                Kind::LargeSmoke,
                Options::Simple,
                dvec3(
                    pos.x as f64 + r.f64(),
                    pos.y as f64 + 1.0 - r.f64() * 0.1,
                    pos.z as f64 + r.f64(),
                ),
                DVec3::ZERO,
                false,
            );
        }
    }
}

fn packed_pos(p: BlockPos) -> u64 {
    (p.x as u64).wrapping_mul(0x9E3779B185EBCA87)
        ^ (p.y as u64).rotate_left(21)
        ^ (p.z as u64).rotate_left(42)
}

fn f(b: i32, d: f64) -> f64 {
    b as f64 + d
}

fn sign(r: &mut fastrand::Rng) -> f64 {
    if r.bool() { 1.0 } else { -1.0 }
}

fn horizontal(f: &str) -> (i32, i32) {
    match f {
        "east" => (1, 0),
        "west" => (-1, 0),
        "south" => (0, 1),
        _ => (0, -1),
    }
}

fn opposite_horizontal(f: &str) -> (f64, f64) {
    let (x, z) = horizontal(f);
    (-f64::from(x), -f64::from(z))
}

fn gaussian(r: &mut fastrand::Rng) -> f64 {
    let u = (r.f64()).max(f64::MIN_POSITIVE);
    let v = r.f64();
    (-2.0 * u.ln()).sqrt() * (std::f64::consts::TAU * v).cos()
}

pub(super) fn full_collision(state: BlockState) -> bool {
    block::has_collision(state)
        && block::block_shape(state).is_none_or(|shape| {
            shape.iter().any(|bounds| {
                bounds[0] <= 0.0
                    && bounds[1] <= 0.0
                    && bounds[2] <= 0.0
                    && bounds[3] >= 1.0
                    && bounds[4] >= 1.0
                    && bounds[5] >= 1.0
            })
        })
}

fn is_firefly_dark(sky: u8, block: u8, sky_darken: u8) -> bool {
    sky.saturating_sub(sky_darken).max(block) <= 13
}

fn rain_exposed(height: Option<i32>, y: i32) -> bool {
    height.is_some_and(|height| height <= y + 1)
}

fn is_free(s: BlockState) -> bool {
    let id = block::block_id(s);
    block::is_air(s)
        || matches!(id, "fire" | "soul_fire")
        || block::fluid(s).kind != FluidKind::Empty
        || block::is_replaceable(s)
}

fn face_full(s: BlockState, face: usize) -> bool {
    face_sturdy(s, face)
}

fn face_sturdy(s: BlockState, face: usize) -> bool {
    if !block::has_collision(s) {
        return false;
    }
    let cube = [0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
    let shapes = block::block_shape(s).unwrap_or(std::slice::from_ref(&cube));
    shapes.iter().any(|b| match face {
        0 => b[1] <= 0.0 && b[0] <= 0.0 && b[2] <= 0.0 && b[3] >= 1.0 && b[5] >= 1.0,
        1 => b[4] >= 1.0 && b[0] <= 0.0 && b[2] <= 0.0 && b[3] >= 1.0 && b[5] >= 1.0,
        2 => b[2] <= 0.0 && b[0] <= 0.0 && b[1] <= 0.0 && b[3] >= 1.0 && b[4] >= 1.0,
        3 => b[5] >= 1.0 && b[0] <= 0.0 && b[1] <= 0.0 && b[3] >= 1.0 && b[4] >= 1.0,
        4 => b[0] <= 0.0 && b[1] <= 0.0 && b[2] <= 0.0 && b[4] >= 1.0 && b[5] >= 1.0,
        5 => b[3] >= 1.0 && b[1] <= 0.0 && b[2] <= 0.0 && b[4] >= 1.0 && b[5] >= 1.0,
        _ => false,
    })
}

fn drip_particle_below(p: BlockPos, r: &mut fastrand::Rng) -> DVec3 {
    dvec3(
        p.x as f64 + r.f64(),
        p.y as f64 - 0.05,
        p.z as f64 + r.f64(),
    )
}

pub(super) fn drip_position(
    chunks: &ChunkStore,
    pos: BlockPos,
    state: BlockState,
    r: &mut fastrand::Rng,
    honey: bool,
) -> Option<DVec3> {
    if block::fluid(state).kind != FluidKind::Empty {
        return None;
    }
    let shape = block::block_shape(state).unwrap_or(&[[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]]);
    let top = shape_top(state);
    let bottom = shape.iter().map(|b| b[1]).fold(1.0, f64::min);
    if honey {
        if top < 1.0 || impermeable(chunks, state) {
            return None;
        }
        if bottom > 0.0 {
            return Some(shape_random(pos, shape, bottom - 0.05, r));
        }
        let p = BlockPos::new(pos.x, pos.y - 1, pos.z);
        let s = chunks.get_block_state(p.x, p.y, p.z);
        if block::fluid(s).kind == FluidKind::Empty && (!full_collision(s) || shape_top(s) < 1.0) {
            return Some(shape_random(pos, shape, -0.05, r));
        }
        return None;
    }
    let target = BlockPos::new(pos.x, pos.y - 1, pos.z);
    let below = chunks.get_block_state(target.x, target.y, target.z);
    if block::fluid(below).kind != FluidKind::Empty {
        return None;
    }
    let btop = shape_top(below);
    if btop < 1.0 {
        if !face_sturdy(state, 0) {
            return None;
        }
        return Some(dvec3(
            target.x as f64 + r.f64(),
            target.y as f64 + 0.95,
            target.z as f64 + r.f64(),
        ));
    }
    if impermeable(chunks, below) {
        return None;
    }
    let t_shape = block::block_shape(below).unwrap_or(&[[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]]);
    let t_bottom = t_shape.iter().map(|b| b[1]).fold(1.0, f64::min);
    if t_bottom > 0.0 {
        return Some(shape_random(target, t_shape, t_bottom - 0.05, r));
    }
    let below2 = BlockPos::new(target.x, target.y - 1, target.z);
    let b2 = chunks.get_block_state(below2.x, below2.y, below2.z);
    if block::fluid(b2).kind != FluidKind::Empty || (shape_top(b2) >= 1.0 && full_collision(b2)) {
        return None;
    }
    Some(shape_random(target, t_shape, -0.05, r))
}

fn shape_top(s: BlockState) -> f64 {
    if !block::has_collision(s) {
        return 0.0;
    }
    block::block_shape(s)
        .unwrap_or(&[[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]])
        .iter()
        .map(|b| b[4])
        .fold(0.0, f64::max)
}

fn shape_random(p: BlockPos, shape: &[[f64; 6]], y: f64, r: &mut fastrand::Rng) -> DVec3 {
    let minx = shape.iter().map(|b| b[0]).fold(1.0, f64::min);
    let maxx = shape.iter().map(|b| b[3]).fold(0.0, f64::max);
    let minz = shape.iter().map(|b| b[2]).fold(1.0, f64::min);
    let maxz = shape.iter().map(|b| b[5]).fold(0.0, f64::max);
    dvec3(
        p.x as f64 + minx + r.f64() * (maxx - minx),
        p.y as f64 + y,
        p.z as f64 + minz + r.f64() * (maxz - minz),
    )
}

fn impermeable(chunks: &ChunkStore, s: BlockState) -> bool {
    let id = block::block_id(s);
    if let Some(value) = chunks.is_impermeable(id) {
        return value;
    }
    matches!(
        id,
        "glass"
            | "tinted_glass"
            | "barrier"
            | "white_stained_glass"
            | "orange_stained_glass"
            | "magenta_stained_glass"
            | "light_blue_stained_glass"
            | "yellow_stained_glass"
            | "lime_stained_glass"
            | "pink_stained_glass"
            | "gray_stained_glass"
            | "light_gray_stained_glass"
            | "cyan_stained_glass"
            | "purple_stained_glass"
            | "blue_stained_glass"
            | "brown_stained_glass"
            | "green_stained_glass"
            | "red_stained_glass"
            | "black_stained_glass"
    )
}

fn wet_sponge(
    chunks: &ChunkStore,
    pos: BlockPos,
    state: BlockState,
    r: &mut fastrand::Rng,
    emit: &mut impl FnMut(
        crate::particle::ServerParticleKind,
        crate::particle::ServerParticleOptions,
        DVec3,
        DVec3,
        bool,
    ),
) {
    let d = r.u32(0..6) as usize;
    if d == 1 {
        return;
    }
    let dirs = [
        (0, -1, 0),
        (0, 1, 0),
        (0, 0, -1),
        (0, 0, 1),
        (-1, 0, 0),
        (1, 0, 0),
    ];
    let (dx, dy, dz) = dirs[d];
    let n = BlockPos::new(pos.x + dx, pos.y + dy, pos.z + dz);
    let next = chunks.get_block_state(n.x, n.y, n.z);
    if block::light_props(state).can_occlude && face_sturdy(next, opposite_face(d)) {
        return;
    }
    let (x, y, z) = if d == 0 {
        (r.f64(), -0.05, r.f64())
    } else if dx != 0 {
        (if dx > 0 { 1.1 } else { 0.05 }, r.f64() * 0.8, r.f64())
    } else {
        (r.f64(), r.f64() * 0.8, if dz > 0 { 1.1 } else { 0.05 })
    };
    emit(
        crate::particle::ServerParticleKind::DrippingWater,
        crate::particle::ServerParticleOptions::Simple,
        dvec3(pos.x as f64 + x, pos.y as f64 + y, pos.z as f64 + z),
        DVec3::ZERO,
        false,
    );
}

fn wire_line(
    pos: BlockPos,
    side: (i32, i32, i32),
    along: (i32, i32, i32),
    from: f32,
    to: f32,
    color: i32,
    r: &mut fastrand::Rng,
    emit: &mut impl FnMut(Kind, Options, DVec3, DVec3, bool),
) {
    let span = to - from;
    if r.f32() >= 0.2 * span {
        return;
    }
    let t = f64::from(from + span * r.f32());
    let p = dvec3(
        pos.x as f64 + 0.5 + 0.4375 * f64::from(side.0) + t * f64::from(along.0),
        pos.y as f64 + 0.5 + 0.4375 * f64::from(side.1) + t * f64::from(along.1),
        pos.z as f64 + 0.5 + 0.4375 * f64::from(side.2) + t * f64::from(along.2),
    );
    emit(
        Kind::Dust,
        Options::Dust {
            packed_color: color,
            scale: 1.0,
        },
        p,
        DVec3::ZERO,
        false,
    );
}

fn opposite_face(face: usize) -> usize {
    [1, 0, 3, 2, 5, 4][face]
}
fn direction(f: &str) -> (f64, f64, f64) {
    match f {
        "down" => (0.0, -1.0, 0.0),
        "north" => (0.0, 0.0, -1.0),
        "south" => (0.0, 0.0, 1.0),
        "west" => (-1.0, 0.0, 0.0),
        "east" => (1.0, 0.0, 0.0),
        _ => (0.0, 1.0, 0.0),
    }
}
fn is_world_surface_rod(height: Option<i32>, y: i32) -> bool {
    height.is_some_and(|height| y == height - 1)
}

fn crying_obsidian(
    chunks: &ChunkStore,
    pos: BlockPos,
    r: &mut fastrand::Rng,
    emit: &mut impl FnMut(Kind, Options, DVec3, DVec3, bool),
) {
    for (dx, dy, dz) in [
        (0, -1, 0),
        (0, 1, 0),
        (0, 0, -1),
        (0, 0, 1),
        (-1, 0, 0),
        (1, 0, 0),
    ] {
        if full_collision(chunks.get_block_state(pos.x + dx, pos.y + dy, pos.z + dz)) {
            continue;
        }
        let x = if dx == 0 {
            r.f64()
        } else {
            0.5 + 0.6 * f64::from(dx)
        };
        let y = if dy == 0 {
            r.f64()
        } else {
            0.5 + 0.6 * f64::from(dy)
        };
        let z = if dz == 0 {
            r.f64()
        } else {
            0.5 + 0.6 * f64::from(dz)
        };
        emit(
            Kind::DrippingObsidianTear,
            Options::Simple,
            dvec3(pos.x as f64 + x, pos.y as f64 + y, pos.z as f64 + z),
            DVec3::ZERO,
            false,
        );
    }
}

#[cfg(test)]
mod tests {
    use azalea_core::position::ChunkPos;
    use azalea_world::chunk::Chunk;

    use super::*;

    fn world_with_states(pos: BlockPos, sulfur: BlockState, above: BlockState) -> ChunkStore {
        let mut chunks = ChunkStore::new(2);
        let mut chunk = Chunk::default();
        chunk.sections = vec![Default::default(); chunks.section_count() as usize].into();
        chunks.load_decoded_chunk(
            ChunkPos::new(pos.x.div_euclid(16), pos.z.div_euclid(16)),
            chunk,
        );
        chunks.set_block_state(pos.x, pos.y, pos.z, sulfur);
        chunks.set_block_state(pos.x, pos.y + 1, pos.z, above);
        chunks
    }

    fn sulfur_requests(chunks: &ChunkStore, pos: BlockPos) -> Vec<ParticleSpawnRequest> {
        sample_positions(
            chunks,
            &std::collections::HashMap::new(),
            &crate::renderer::chunk::mesher::Colormap::test_empty(),
            [pos],
            1,
            false,
            false,
            0,
            0,
            |_| (false, Kind::DrippingWater, Options::Simple),
        )
    }

    fn test_rng() -> fastrand::Rng {
        fastrand::Rng::with_seed(3)
    }

    fn state(name: &str) -> BlockState {
        block::state_without_properties(name).expect("native block state")
    }

    #[test]
    fn java_class_identity_candles_and_fire_registry_are_exact() {
        let _protocol = block::test_protocol_guard();
        assert_eq!(JAVA_26_2_FLAMMABLE_BLOCKS.len(), 175);
        assert_eq!(
            JAVA_26_2_FLAMMABLE_BLOCKS
                .iter()
                .copied()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            175
        );
        assert!(
            JAVA_26_2_FLAMMABLE_BLOCKS
                .iter()
                .all(|name| block::default_state_of(name).is_some())
        );
        assert!(is_candle("red_candle_cake"));
        assert!(is_candle("blue_candle"));
        assert!(!is_candle("unknown_candle"));
        assert_eq!(candle_offsets(1).len(), 1);
        assert_eq!(candle_offsets(2).len(), 2);
        assert_eq!(candle_offsets(3).len(), 3);
        assert_eq!(candle_offsets(4).len(), 4);
        assert_eq!(candle_offsets(99).len(), 4);
        assert!(candle_smoke_roll(0.0));
        assert!(candle_smoke_roll(0.2999));
        assert!(!candle_smoke_roll(0.3));
    }

    #[test]
    fn lit_redstone_ore_animate_tick_covers_stone_and_deepslate_variants() {
        let _protocol = block::test_protocol_guard();
        block::init("26.2");
        let pos = BlockPos::new(1, 64, 1);
        for name in ["redstone_ore", "deepslate_redstone_ore"] {
            let mut chunks = world_with_states(
                pos,
                block::find_state(name, &[("lit", "true")]),
                state("air"),
            );
            for neighbor in [
                BlockPos::new(1, 63, 1),
                BlockPos::new(1, 65, 1),
                BlockPos::new(1, 64, 0),
                BlockPos::new(1, 64, 2),
                BlockPos::new(0, 64, 1),
                BlockPos::new(2, 64, 1),
            ] {
                chunks.set_block_state(neighbor.x, neighbor.y, neighbor.z, state("air"));
            }
            let requests = sample_positions(
                &chunks,
                &std::collections::HashMap::new(),
                &crate::renderer::chunk::mesher::Colormap::test_empty(),
                [pos],
                19,
                false,
                false,
                0,
                0,
                |_| (false, Kind::DrippingWater, Options::Simple),
            );
            assert_eq!(requests.len(), 6, "{name}");
            assert!(requests.iter().all(|request| {
                request.kind == Kind::Dust
                    && matches!(
                        request.options,
                        Options::Dust {
                            packed_color: 0xff0000,
                            scale: 1.0
                        }
                    )
                    && request.velocity == DVec3::ZERO
            }));
        }
    }

    #[test]
    fn five_block_sampler_inputs_produce_java_boundary_results() {
        assert!(is_firefly_dark(15, 0, 2));
        assert!(!is_firefly_dark(15, 14, 2));
        assert!(rain_exposed(Some(50), 49));
        assert!(!rain_exposed(Some(51), 49));
        assert!(!rain_exposed(None, 49));
        assert!(is_world_surface_rod(Some(80), 79));
        assert!(!is_world_surface_rod(Some(80), 80));
        assert!(!is_world_surface_rod(None, 79));
        assert!(matches!(
            pointed_dripstone_particle(
                FluidKind::Empty,
                false,
                false,
                0.0199,
                (Kind::Ash, Options::Simple)
            ),
            Some((Kind::Ash, Options::Simple))
        ));
        assert!(
            pointed_dripstone_particle(
                FluidKind::Empty,
                false,
                false,
                0.02001,
                (Kind::Ash, Options::Simple)
            )
            .is_none()
        );
        assert!(matches!(
            pointed_dripstone_particle(
                FluidKind::Empty,
                true,
                false,
                0.12,
                (Kind::Ash, Options::Simple)
            ),
            Some((Kind::DrippingDripstoneWater, Options::Simple))
        ));
        assert!(
            pointed_dripstone_particle(
                FluidKind::Empty,
                true,
                true,
                0.12,
                (Kind::Ash, Options::Simple)
            )
            .is_none()
        );
        assert!(matches!(
            pointed_dripstone_particle(
                FluidKind::Lava,
                false,
                true,
                0.12,
                (Kind::Ash, Options::Simple)
            ),
            Some((Kind::DrippingDripstoneLava, Options::Simple))
        ));
    }

    #[test]
    fn base_fire_uses_real_flammable_and_sturdy_block_states() {
        let _protocol = block::test_protocol_guard();
        block::init("26.2");
        let pos = BlockPos::new(1, 64, 1);
        let mut chunks = ChunkStore::new(2);
        let mut chunk = Chunk::default();
        chunk.sections = vec![Default::default(); chunks.section_count() as usize].into();
        chunks.load_decoded_chunk(ChunkPos::new(0, 0), chunk);
        for below in [state("oak_planks"), state("stone")] {
            chunks.set_block_state(pos.x, pos.y - 1, pos.z, below);
            let mut emitted = Vec::new();
            {
                let mut emit = |kind, _, _, _, _| emitted.push(kind);
                base_fire(&chunks, pos, &mut fastrand::Rng::with_seed(7), &mut emit);
            }
            assert_eq!(
                emitted
                    .iter()
                    .filter(|kind| matches!(kind, Kind::LargeSmoke))
                    .count(),
                3,
                "below={} must enter Java's flammable OR sturdy-top branch",
                block::block_id(below),
            );
        }
    }

    #[test]
    fn drip_collision_shapes_and_impermeable_tag_are_respected() {
        let _protocol = block::test_protocol_guard();
        block::init("26.2");
        let mut chunks = ChunkStore::new(2);
        let pos = BlockPos::new(0, 64, 0);
        let stone = state("stone");
        let glass = state("glass");
        let slab = block::state_with_properties(
            "stone_slab",
            &[
                ("type".into(), "bottom".into()),
                ("waterlogged".into(), "false".into()),
            ],
        )
        .expect("bottom slab");
        assert!(full_collision(stone));
        assert!(!full_collision(slab));
        assert!(impermeable(&chunks, glass));
        assert!(!impermeable(&chunks, stone));
        assert!(drip_position(&chunks, pos, stone, &mut test_rng(), true).is_some());
        assert!(drip_position(&chunks, pos, slab, &mut test_rng(), true).is_none());
        assert!(drip_position(&chunks, pos, glass, &mut test_rng(), true).is_none());
        chunks.replace_impermeable_blocks(std::collections::HashSet::new());
        assert!(
            !impermeable(&chunks, glass),
            "explicit empty server tag replaces vanilla defaults"
        );
        chunks.replace_impermeable_blocks(std::collections::HashSet::from(["stone".into()]));
        assert!(
            impermeable(&chunks, stone),
            "server replacement is authoritative"
        );
    }

    #[test]
    fn potent_sulfur_animate_tick_uses_real_state_property_and_source_water_only() {
        let _protocol = block::test_protocol_guard();
        block::init("26.2");
        let pos = BlockPos::new(1, 64, 1);
        let source_water = block::find_state("water", &[("level", "0")]);
        let flowing_water = block::find_state("water", &[("level", "1")]);
        let air = block::find_state("air", &[]);

        for sulfur_state in ["wet", "dormant", "erupting", "continuous"] {
            let sulfur =
                block::find_state("potent_sulfur", &[("potent_sulfur_state", sulfur_state)]);
            assert_eq!(
                block::block_properties(sulfur).get("potent_sulfur_state"),
                Some(sulfur_state)
            );
            let requests = sulfur_requests(&world_with_states(pos, sulfur, source_water), pos);
            assert_eq!(requests.len(), 2, "state={sulfur_state}");
            assert!(requests.iter().all(|request| {
                request.kind == Kind::SulfurBubbles
                    && request.position.y >= pos.y as f64 + 1.0
                    && request.position.y <= pos.y as f64 + 2.0
                    && request.velocity == DVec3::ZERO
            }));
        }

        let dry = block::find_state("potent_sulfur", &[("potent_sulfur_state", "dry")]);
        assert!(sulfur_requests(&world_with_states(pos, dry, source_water), pos).is_empty());

        let wet = block::find_state("potent_sulfur", &[("potent_sulfur_state", "wet")]);
        for above in [flowing_water, air] {
            assert!(
                sulfur_requests(&world_with_states(pos, wet, above), pos).is_empty(),
                "above block {} is not source water",
                block::block_id(above)
            );
        }
    }
}
