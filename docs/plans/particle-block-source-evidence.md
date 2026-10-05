# Java 26.2 block `animateTick` source coverage

Reference: `minecraft-26.2-decompiled/src/net/minecraft/world/level/block` and `world/level/material`. Search was expanded from the previous `addParticle` call-site inventory to `animateTick\(|particleTick\(` across the Java tree (**53 matched files**, including inherited/helper call paths). The focused block/material search returns 50 files (46 block classes and 4 fluid/base API classes).

`world::particle_tick::blocks::sample` dispatches exact block registry names and state properties from the decoded 26.2 block-state table. It produces typed `ServerParticleKind`/`ServerParticleOptions` requests for the existing `ParticleStore` path. It does not implement Java sounds, server random ticks, or entity AI. Of the 46 classes, 8 have no independent particle output (or inherit the separately listed falling-block source), 33 particle-producing classes are implemented against vanilla state, and 5 are explicitly partial due missing runtime inputs. `BaseFireBlock` uses its Java 26.2 bootstrap list of 175 exact flammable registry keys; it does not approximate flammability from destroy time.

| Java source class | Rust dispatch / result | Evidence / remaining dependency |
|---|---|---|
| `AbstractCandleBlock` | Exact registered candle/candle-cake keys; candle-count offsets; SMALL_FLAME always and SMOKE at `<0.3` | `blocks.rs::sample_candles`, `candle_offsets` |
| `BaseFireBlock` | Java fire/soul-fire conditions, burn-neighbor checks and LARGE_SMOKE positions | `blocks.rs::base_fire`; `JAVA_26_2_FLAMMABLE_BLOCKS` is derived from all 175 `FireBlock.bootStrap` entries and mapped by exact native block key; a test verifies every key exists in the native 26.2 state table. |
| `BeehiveBlock` | honey level ≥5; Java `nextInt(1)+1` produces one attempt; non-fluid state and 70% secondary roll; collision-shape drip | `blocks.rs::drip_position(honey=true)`; vanilla `IMPERMEABLE` set is listed explicitly because runtime tag membership is not retained. Server datapack tag overrides are not represented. |
| `BlastFurnaceBlock` | Lit smoke with facing-dependent mouth coordinates | shared furnace branch in `blocks.rs::sample` |
| `Block` | No-op callback | No particle output in Java. |
| `BrewingStandBlock` | Sampled smoke position | `blocks.rs::sample` |
| `BrushableBlock` | Suspicious sand/gravel Falling Dust with state option, 1/16 and `isFree`-equivalent lower-state check | `blocks.rs::sample`, `is_free` |
| `BubbleColumnBlock` | Drag-down current or two upward bubbles, always-visible path and Java velocities | `blocks.rs::sample` |
| `CampfireBlock` | Lit normal campfire LAVA particle, 1/5 | Campfire smoke/food particles remain owned by existing `CampfireBlockEntity::particleTick` loop; this sampler does not duplicate them. Soul campfire correctly omits this LAVA callback. |
| `CreakingHeartBlock` | No particle output | Its `animateTick` is sound-only and depends on `EnvironmentAttributes.CREAKING_ACTIVE`; sound dispatch is outside this particle sampler. |
| `CryingObsidianBlock` | 1/5, random non-UP face, face-occlusion condition, dripping obsidian tear | `blocks.rs::crying_obsidian` |
| `DriedGhastBlock` | Dry white smoke / waterlogged happy-villager particle at 1/6 | `blocks.rs::dried_ghast` |
| `DryVegetationBlock` | No particle output | Java delegates to ambient desert sound helper only. |
| `EnchantingTableBlock` | Valid bookshelf/transmitter geometry and 1/16 per offset; ENCHANT option/velocity | `blocks.rs::enchanting_table`; uses exact bookshelf key and Rust replaceable state. |
| `EndGatewayBlock` | PORTAL particle for each visible face; face-dependent position and velocity | `blocks.rs::end_gateway`; counts visible faces from the received block shapes. |
| `EndPortalBlock` | SMOKE above portal | `blocks.rs::sample` |
| `EndRodBlock` | 1/5 END_ROD, facing, roll, and Gaussian velocity | `blocks.rs::sample` |
| `EnderChestBlock` | Three PORTAL particles with Java coordinate/velocity distribution | `blocks.rs::sample` |
| `EyeblossomBlock` | No particle output | Java callback is sound-only. |
| `FallingBlock` | Falling Dust with typed block-state option, 1/16 on free lower state | Exact vanilla sand/gravel/red-sand/concrete-powder/anvil IDs in `blocks.rs::sample`; no suffix matching. |
| `FireflyBushBlock` | FIREFLY at Java `getMaxLocalRawBrightness` ≤13 and 70% roll | `blocks.rs::sample`; uses loaded sky/block nibble light and the current dimension/time/weather `SkyLightEvaluation.sky_darken` passed at fixed tick. The comparison is `max(sky.saturating_sub(sky_darken), block)`. |
| `FurnaceBlock` | Lit smoke + flame; facing and furnace height | shared furnace branch |
| `HangingMossBlock` | No particle output | Java callback is sound-only. |
| `LeavesBlock` | Rain-drip and species-specific falling leaves; 0.01/0.1/0.02 chance; tinted leaf color uses biome foliage climate/colormap blend | `blocks.rs::sample_leaves`; rain exposure uses `MOTION_BLOCKING_NO_LEAVES`. `ClientboundLevelChunkPacketData` sends this `Usage.CLIENT` map when present; if missing, `ChunkStore::heightmap_height` primes and caches that exact map from loaded block states, then recomputes the affected column on local writes. Predicate uses Java 26.2 `blocksMotion` (`v`), fluid presence, and generated `instanceof LeavesBlock` metadata (`l`), not the `LEAVES` tag. |
| `LeverBlock` | Powered 1/4 red dust at attachment/facing-derived position | `blocks.rs::sample`; uses exact attachment state properties. |
| `LightningRodBlock` | Thunder/game-time condition and electric sparks at `WORLD_SURFACE` top | `blocks.rs::sample` queries `ChunkStore::heightmap_height(WorldSurface, ...)`; `WORLD_SURFACE` is `Usage.CLIENT` and is normally sent in chunk data. If it is absent, `ChunkStore` primes and caches that exact map using Java `NOT_AIR`; it never substitutes `MOTION_BLOCKING`. The converter/parser retain native ID 1 / `HeightmapKind::WorldSurface`. |
| `MyceliumBlock` | 1/10 MYCELIUM particle above block | `blocks.rs::sample` |
| `NetherPortalBlock` | Four PORTAL particles with adjacent-axis placement and Java velocity ranges | `blocks.rs::sample` |
| `PointedDripstoneBlock` | 12% free-hanging trial; water/lava drops; empty-above default-water drop on the 2% subrange; mud is treated as water when `WATER_EVAPORATES=false` | `blocks.rs::pointed_dripstone` consumes the typed fixed-tick dimension boolean from `net::environment::DimensionEnvironment`; dimension NBT attribute ingress supports `minecraft:gameplay/water_evaporates`, defaulting to Java's `false`. Remaining: positional/biome/timeline resolution for `WATER_EVAPORATES` and typed `DEFAULT_DRIPSTONE_PARTICLE` overrides; default-water behavior is vanilla's default only. |
| `PotentSulfurBlock` | Non-dry state and water source above emits two always-visible SULFUR_BUBBLES | `blocks.rs::sample` |
| `RedStoneOreBlock` | Lit ore dust on neighboring faces whose block state is not Java `isSolidRender` | `blocks.rs::redstone_ore` uses exact generated `BlockData` `t` metadata via `block::is_solid_render`; no full-collision proxy. Unknown metadata is conservatively not treated as non-solid. |
| `RedstoneTorchBlock` | Lit REDSTONE dust with torch offsets | `blocks.rs::sample` |
| `RedstoneWallTorchBlock` | Lit REDSTONE dust with wall attachment offsets | `blocks.rs::sample` |
| `RedStoneWireBlock` | Powered wire with side/up connection sampling, density, line positions, and per-power color | `blocks.rs::redstone_wire`, `wire_line` |
| `RepeaterBlock` | Powered REDSTONE dust with facing, delay, and randomized offset | `blocks.rs::sample` |
| `RespawnAnchorBlock` | Charged anchor REVERSE_PORTAL particle and upward speed | `blocks.rs::sample` |
| `SandBlock` | No particle output | Inherits `FallingBlock` particle callback (mapped above); own override delegates to audio helper only. |
| `SculkSensorBlock` | Active phase SCULK_TO_REDSTONE transition dust, horizontal face, upward speed | `blocks.rs::sample` |
| `ShortDryGrassBlock` | No particle output | Ambient desert sound helper only. |
| `SmokerBlock` | Lit smoke with smoker-specific height | shared furnace branch |
| `SporeBlossomBlock` | One falling spore plus 14 ambient probes rejecting full collision blocks | `blocks.rs::spore_blossom` |
| `TallDryGrassBlock` | No particle output | Ambient desert sound helper only. |
| `TorchBlock` | Smoke + normal/soul flame | `blocks.rs::sample` |
| `WallTorchBlock` | Smoke + normal/soul flame at wall offset | `blocks.rs::sample` |
| `WetSpongeBlock` | Random non-UP face, occlusion rejection, DRIPPING_WATER location | `blocks.rs::wet_sponge` |
| `WitherRoseBlock` | Three half-chance smoke attempts at Java coordinates | `blocks.rs::sample` |

## Focused tests added

`world::particle_tick::blocks::tests::java_class_identity_candles_and_fire_registry_are_exact` asserts explicit candle identities/offset counts and checks that all **175** Java `FireBlock.bootStrap` flammable keys are unique and present in the native 26.2 block table. `drip_collision_shapes_and_impermeable_tag_are_respected` exercises full cube, bottom slab, vanilla glass tag, and honey drip branches. `world::particle_tick::tests` covers the 667×2 sample count, loaded-chunk gating, weather status counts, and Rain/Snow/None climate classification.

## Common drip rules

`ClientLevel.trySpawnDripParticles` now uses the actual sampled source/target block states, fluids and local collision shapes. It checks source bottom support when the target shape top is below 1; for full-height targets it rejects the vanilla `IMPERMEABLE` block set, checks target shape bottom, then checks the next lower block's fluid and shape-height/full-block condition. Honey drips use their Java-specific full-height / impermeable branch. The 26.2 vanilla impermeable set comes from `tmp/Pumpkin-shallow/assets/datapacks/26_2/data/minecraft/tags/block/impermeable.json` (glass variants, tinted glass, barrier); synced/custom tag overrides remain unavailable.

## `ClientLevel.animateTick` marker particle evidence row

| Java source row | Java 26.2 condition/output | Rust implementation and test basis |
|---|---|---|
| `ClientLevel.animateTick` / `getMarkerParticleTarget` / `doAnimateTick` | Target is resolved once: Creative only, main-hand item in exact `MARKER_PARTICLE_ITEMS = {Items.BARRIER, Items.LIGHT}`, and a `BlockItem`; target is that item's block. Matching sampled state emits `BlockParticleOption(BLOCK_MARKER, state)` at `(x+0.5,y+0.5,z+0.5)` with `(0,0,0)` velocity. | `particle_tick::marker_particle_target` accepts only creative + `barrier`/`light`; `in_game::update_game` resolves current mode and selected hotbar stack each fixed tick. Existing animate-probe loop in `sample_animate_tick_particles` compares the native sampled state, then queues `BlockMarker` + `ServerParticleOptions::Block(state)` at center/zero velocity. Added tests cover both accepted items, survival/adventure/spectator/other modes, empty/other/changed held item, matching/nonmatching state, native option state, center, and zero velocity. The traversal is the existing 667×2 probe set; there is no second sampler or server-packet/provider/atlas change. |

## Explicit unresolved runtime data

1. **Environment attribute parity remains partial:** dimension `WATER_EVAPORATES` is retained and used, but timeline/biome/positional overrides are not yet evaluated; `DEFAULT_DRIPSTONE_PARTICLE` uses Java's vanilla default (`DRIPPING_DRIPSTONE_WATER`) rather than synced typed overrides. Creaking heart has no particle output; `CREAKING_ACTIVE` controls sound only.
2. **Server block-tag overrides:** existing `IMPERMEABLE` membership still uses a vanilla fixed set. `UpdateTags` is not yet retained in `ChunkStore`, so custom/datapack overrides remain unresolved.
3. **Heightmaps and lighting:** wire heightmaps are retained separately by kind. Java `ChunkAccess.getHeight` calls `Heightmap.primeHeightmaps` when a requested map is absent; the client mirrors that for `WORLD_SURFACE` (`NOT_AIR`) and `MOTION_BLOCKING_NO_LEAVES` (`blocksMotion || fluid present`, excluding `state.getBlock() instanceof LeavesBlock`). The latter class test is generated from all 32,366 Java states (11 block keys), not inferred from a tag. Primed maps are stored in the chunk's normal heightmap table and updated on local state writes. World lighting uses loaded sky/block nibble light plus environment sky-darkening at fixed tick.

Validation additions: StateDump emits vanilla `isSolidRender` and `blocksMotion` for all 32,366 states; blockgen compacts them to `t` and `v`; the 26.2 table has 1,196 blocks and was verified equivalent for all existing fields after excluding `t`/`v`. Added concrete boundary tests cover darkening-adjusted brightness (sky 15/darken 2 = 13; block light 14 rejects), leaves rain exposure via its requested map, lightning rod WORLD_SURFACE from the requested map, pointed-dripstone default-water probability/mud override/lava, representative solid-render metadata, and absent-map priming with negative minY, empty columns, water, LeavesBlock exclusion, same-type received-map priority, block updates, and unloaded-chunk `None`. `mise run check` and `mise exec -- cargo check -p pomme-client --tests --locked --profile dev-fast` both passed after heightmap changes. Focused runtime tests were not run: `ps -W` showed an active `pomme_client-e2601c96198867a4.exe` using the shared test output, so no competing relink was attempted.

These are concrete inputs not represented by the current client model, not claims that those source sites have exact parity.
