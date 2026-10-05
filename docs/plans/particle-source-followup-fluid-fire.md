# Fluid / fire source-condition follow-up

Reference version: Java 26.2, `minecraft-26.2-decompiled/src/net/minecraft`.

## DripParticle provider fluid types

`DripParticle.tick` removes a particle for being inside a matching fluid only when `type != Fluids.EMPTY`; the provider-owned Java types are:

| Particle provider | Java fluid type |
|---|---|
| `DrippingWater`, `FallingWater` | `Fluids.WATER` |
| `DrippingDripstoneWater`, `FallingDripstoneWater` | `Fluids.WATER` |
| `DrippingLava`, `FallingLava`, `LandingLava` | `Fluids.LAVA` |
| `DrippingDripstoneLava`, `FallingDripstoneLava` | `Fluids.LAVA` |
| Honey hanging/falling/landing, `FallingNectar`, Obsidian Tear hanging/falling/landing | `Fluids.EMPTY` (no in-fluid removal test) |

Evidence: `DripParticle.java` provider constructors, especially `LavaHangProvider`, `LavaFallProvider`, `LavaLandProvider`, `WaterHangProvider`, `WaterFallProvider`, the four dripstone providers, and `Honey*`, `NectarFallProvider`, `ObsidianTear*`; registrations are in `ParticleResources.java:81–85,138–149`.

Implemented in `pomme-client/src/particle/water.rs`: `drip_fluid_type` now expresses Water/Lava/None, and both hanging and falling collision checks use it. This fixes honey and obsidian tear being implicitly classified as Water and also covers their landing/falling phases. Hanging expiry still transitions to the matching falling kind; existing landing child and sound paths were not changed. Added provider-map and real-block-state tick tests: matching Water/Lava must remove, cross-fluid particles remain, and Honey/Tear remain in both Water and Lava.

## WaterFluid underwater condition: audited false positive

Java `WaterFluid.animateTick` (`WaterFluid.java:60–67`) emits `UNDERWATER` for `fluidState.isSource() || fluidState.FALLING`; ordinary flowing water only takes the ambient sound branch. Rust block fluid metadata in `world/block/mod.rs::state_fluid` maps `water[level=0]` to source, levels `1..=7` to non-falling amounts `7..=1`, and levels `8..=15` to amount 8 with `falling=true`; waterlogged states are also full, non-falling sources. Therefore the previous `amount == 8` test is extensionally equivalent on the actual 26.2 water block states, including falling columns: **this audit item is a false positive, not a missing emission condition**.

For clarity the production condition in `world/particle_tick.rs` now names the Java semantics (`water_fluid_emits_underwater`: source OR falling) rather than testing the incidental amount encoding. Added a test against real registry block states for all 16 water levels, waterlogged stairs, and air. Ordinary flowing levels remain false; no additional particles are introduced there.

## BaseFireBlock condition: fixed in the recovered sampler

Java `BaseFireBlock.java:82–89` emits the lower three `LARGE_SMOKE` particles and exits the remaining side/top branch when `canBurn(below) || sturdyTop(below)`; it proceeds to side/top checks only when both are false. The restored `world/particle_tick/blocks.rs::sample_positions` now uses that exact predicate. Its real-state test checks burnable oak planks and sturdy non-burning stone each take the lower-smoke branch, while a block with neither property does not. This code is in the complete sampler, not a reduced stub.

## Final verification snapshot

- `mise run check` from `Client/` — exit **0** (`cargo check -p pomme-client --locked --profile dev-fast`).
- `mise exec -- cargo test -p pomme-client --locked --profile dev-fast drip_removal_uses_real_world_fluid_type_and_skips_empty_type -- --test-threads=1` — exit **0**, **1 passed**.
- `mise exec -- cargo test -p pomme-client --locked world::particle_tick::blocks::tests -- --test-threads=1` — exit **0**, **6 passed**, including real-state BaseFire, sulfur and both redstone ores.
- The final workspace run, including the new fixed block sampler, is in `particle-integration-verification.md`.
