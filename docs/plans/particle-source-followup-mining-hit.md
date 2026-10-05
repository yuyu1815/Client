# Java 26.2 continued-mining hit particle

## Java reference

- `Minecraft.java::continueAttack` (26.2 decompile lines 1686–1690) calls `gameMode.continueDestroyBlock(pos, direction)`. Only when it returns `true` does it call `level.addBreakingBlockEffect(pos, direction)`.
- `MultiPlayerGameMode.java::continueDestroyBlock` returns false for an already-air target, but true for the destroy-delay branch, a successful same-target mining tick (including the tick that finishes breaking), and a restart through `startDestroyBlock`.
- `ClientLevel.java::addBreakingBlockEffect` (1084–1119) re-reads the *current* BlockState, requires `renderShape != INVISIBLE` and `shouldSpawnTerrainParticles()`, gets `blockState.getShape(level,pos).bounds()` (the AABB union of the outline VoxelShape, not one randomly selected constituent box), samples x/y/z independently as `pos + random.nextDouble() * (max-min-0.2F) + 0.1F + min`, then moves only the chosen face coordinate to the bound ±0.1F. It creates `TerrainParticle(..., blockState, pos).setPower(0.2F).scale(0.6F)`.
- `TerrainParticle` uses the block state's particle material, base RGB 0.6 multiplied by terrain tint, randomized quarter-sprite UV, and the normal terrain constructor physics. `Particle.setPower` transforms x/z by 0.2 and y by `(y-0.1)*0.2+0.1`; `Particle.scale` sets the collision dimensions to 0.12x0.12. It does not change `SingleQuadParticle.quadSize`.

## Rust implementation

- `pomme-client/src/player/interaction.rs::continue_attack` now uses the boolean result from `continue_destroy_block`. On success it re-reads the block from `ChunkStore` and calls `ParticleStore::add_breaking_block_effect` with the current ray-hit face. It does not change packet order or prediction.
- `pomme-client/src/particle.rs::ParticleStore::add_breaking_block_effect` owns the effect because it owns the terrain constructor, atlas/material, tint, light, and insertion. It filters air, invisible render shapes, and block states that opt out of terrain particles; uses the local `getShape` outline boxes' union bounds; preserves Java's 0.1F inset even for dimensions below 0.2; samples each coordinate, applies the face offset, and reuses the normal terrain constructor.
- The hit particle's `Kind::TerrainScaled` keeps the Java 0.6 collision half-width (0.06) while retaining the existing TerrainParticle rendered quad size. Its velocity is transformed with Java `setPower(0.2)`.
- The final predicted destroy continues to use `add_destroy_block_effect`, the distinct volume burst. Since the predicted current state is air by the time the hit effect is added, the finishing mining tick does not duplicate a face particle.
- The effect is invoked from the input/client tick (`InteractionState::continue_attack`), not rendering cadence.

## Verification added

- `particle.rs` tests cover all six faces, full-block and partial slab bounds, sub-0.2 tiny bounds without clamping, air/invisible/terrain-particle opt-outs, the block particle material/terrain color-light inputs, Java power and collision-scale formulas, and extraction into a `ParticleQuad`.
- `interaction.rs::continued_mining_routes_one_face_particle_and_never_duplicates_final_break_burst` exercises production continue-attack routing, face effects on active mining and target changes, no extra effect on stop or after the 4.5-block ray-pick range, and the separate 64-particle predicted destroy burst without a `TerrainScaled` particle.

## Validation status

- `cd Client && mise run check` — **exit 0** after the shared `blocks.rs` recovery landed; warnings remain in unrelated modules.
- `cd Client && mise exec -- cargo test -p pomme-client --locked --profile dev-fast continued_mining_routes_one_face_particle_and_never_duplicates_final_break_burst -- --test-threads=1` — **exit 0**, 1 passed, 1 filtered, 0.07 s.
- `cd Client && mise exec -- cargo test -p pomme-client --locked --profile dev-fast breaking -- --test-threads=1` — **exit 0**, 5 passed (4 mining-related, 1 unrelated environment test matched by the filter), 0 failed, 0.07 s.
- `cd Client && mise exec -- cargo test -p pomme-client --locked --profile dev-fast continued_mining_routes_one_face_particle_and_never_duplicates_final_break_burst -- --test-threads=1` — **exit 0**, 1 passed, 1,750 filtered, 0.07 s.
- `cd Client && rustfmt --edition 2024 --check pomme-client/src/particle.rs pomme-client/src/player/interaction.rs` — **exit 0**; `git diff --check` on the two code files and this note — **exit 0**.

The first focused check encountered concurrently changing `world/particle_tick/blocks.rs`; that blocker was repaired before the final successful check/tests. A full workspace test suite and in-game visual comparison were not part of this mining-only task.
