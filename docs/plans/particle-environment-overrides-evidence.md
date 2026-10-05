# Particle environment override evidence (Java 26.2)

## Java behavior

Sources are the checked-in `minecraft-26.2-decompiled` tree.

- `src/net/minecraft/world/attribute/EnvironmentAttributes.java`: `WATER_EVAPORATES` is boolean, default `false`, syncable, positional by default (the builder default is positional). `DEFAULT_DRIPSTONE_PARTICLE` is `ParticleOptions`, default `ParticleTypes.DRIPPING_DRIPSTONE_WATER`, syncable, positional by default. `SKY_LIGHT_LEVEL` is explicitly `.notPositional()`; these two attributes are not.
- `src/net/minecraft/world/attribute/AttributeTypes.java`: BOOLEAN allows `and`, `nand`, `or`, `nor`, `xor`, `xnor`; PARTICLE uses `ofNotInterpolated` without a modifier library (override only). Both values use step rather than value interpolation.
- `src/net/minecraft/world/attribute/EnvironmentAttribute.java`: builder defaults `isPositional=true`; `notPositional()` is an explicit opt-out.
- `src/net/minecraft/world/attribute/EnvironmentAttributeSystem.java`: layers are added dimension constants, biome positional layers, then dimension timelines. Positional attributes query the biome at the supplied position; each layer acts on the preceding result and the resulting value is sanitized.
- `src/net/minecraft/world/level/block/PointedDripstoneBlock.java`: `getFluidAboveStalactite` resolves `WATER_EVAPORATES` at `abovePos`. `getDripParticle` resolves `DEFAULT_DRIPSTONE_PARTICLE` at that same position only when the fluid is empty; water/lava use the fixed drip particle. `animateTick` draws one float, returns when `> 0.12`, then accepts the source-fluid branch or the `randomValue < 0.02` default-particle branch.
- `src/net/minecraft/world/level/block/BeehiveBlock.java` and `client/multiplayer/ClientLevel.java`: `BlockTags.IMPERMEABLE` is read for drip positioning. `data/tags/VanillaBlockTagsProvider.java` defines the vanilla membership.

## Rust ownership and flow

- `net/environment.rs` owns typed attribute input, boolean modifier evaluation, typed dripstone option decoding, timeline tracks and the shared `dripstone_attributes_at(ChunkStore, BlockPos, ...)` query. It uses the existing connection `ambient_particle_options` codec rather than duplicating option decoding.
- `net/connection.rs` extracts the dimension and biome values, and resolves the `UpdateTags` impermeable entries by the **source protocol block-registry ID** to the generated protocol's block identity before converting to the currently active block-state identity once.
- `ChunkStore` owns `impermeable_blocks: Option<HashSet<String>>`. `None` means no server block-tag replacement was received and uses vanilla fallback membership; `Some(empty)` is an authoritative empty tag. Each UpdateTags registry replacement replaces the stored tag set; packets without the block registry do not clear it.
- `world/particle_tick/blocks.rs` queries attributes at the root's above-block position and uses the resolved typed kind/options only for the empty-fluid `< 0.02` branch. `core.rs` LevelEvent 1504 calls the same positional query.
- A dimension reset recreates chunks but carries the connection's current tag snapshot into the new `ChunkStore`.
- Heightmap LEAVES behavior remains class metadata-based and is not changed by the tag implementation.

## Fixtures

- `net/environment.rs` verifies boolean modifier truth table, biome-before-timeline composition, exact timeline frame boundary/period wrap, and a typed Dust option payload.
- `net/connection.rs` verifies UpdateTags explicit-empty and no-registry distinctions, plus protocol 763 block ID 94 and protocol 776 ID 101 both map to `glass`.
- `world/block/mod.rs` verifies old/new registry IDs resolve to the same block name.
- `world/particle_tick/blocks.rs` verifies empty-fluid strict `< 0.02`, mud/water-evaporation selection, typed default kind/options and server tag empty/replace behavior.

## Validation status

`mise run check` passed once with exit code 0 after the main implementation and before later concurrent edits; a subsequent check currently stops on an unrelated `renderer/lightmap.rs:64` array-length inference error (`E0284`). The focused command `mise exec -- cargo test -p pomme-client --locked net::environment::tests::dripstone_attributes_apply_positional_then_timeline_and_keep_particle_options -- --exact` passed: 1 passed, 0 failed, 1,661 filtered, exit code 0. A later focused test attempt and the tag/block fixture attempts are currently blocked at test compilation by concurrent errors in `renderer/lightmap.rs`, `particle.rs` (`ParticleQuad.light_uv` / `extract` signature), plus unrelated existing entity test wiring. No task-owned compile errors remained in the successful non-test check. The generated block tables expose per-protocol block order; runtime UpdateTags behavior remains unverified against a live server.
