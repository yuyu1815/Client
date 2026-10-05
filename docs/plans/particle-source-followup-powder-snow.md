# Powder-snow SNOWFLAKE source follow-up

Implemented the client-local `PowderSnowBlock.entityInside` SNOWFLAKE source against the Java 26.2 callsite.

## Java behavior consulted

- `minecraft-26.2-decompiled/src/net/minecraft/world/level/block/PowderSnowBlock.java`, `entityInside` (full method, lines 73–97 in this decompile): particles are inside the entity-inside branch; require a client level, horizontal movement (`xOld != x || zOld != z`), then a separate `nextBoolean()` for each callback; particle is `SNOWFLAKE`, at `(entity.x, callbackBlock.y + 1, entity.z)`, velocity `(randomBetween(-1,1)*0.083333336f, 0.05f, randomBetween(-1,1)*0.083333336f)`.
- `minecraft-26.2-decompiled/src/net/minecraft/world/entity/Entity.java`, `checkInsideBlocks` and its callback: per-movement intersected cells are visited once, cell callback only follows collision-volume contact; `getInBlockState()` uses `blockPosition()` (`Entity.java` around 3534), not any neighboring block.

Java's exact entity predicate is `!(entity instanceof LivingEntity) || entity.getInBlockState().is(this)`; the particle method contains no spectator check. The general caller gate is `Entity.isAffectedByBlocks() == !removed && !noPhysics`. `LivingEntity.tick` additionally runs client-side block effects only for `isLocalInstanceAuthoritative()`: local players and living mounts whose actual controlling passenger is local qualify; remote players/mobs do not. Powder snow slowdown remains in `physics/movement.rs` and was not changed or duplicated.

## Rust wiring

- `pomme-client/src/entity/particle_tick.rs`: `powder_snow_requests` matches the Java OR predicate: only living entities require `getInBlockState() == powder_snow`; non-living entities emit for actual intersected powder-snow callbacks. Both branches require horizontal movement and the caller's `affected_by_blocks` eligibility. For contact-producing nonliving owners, `entity_affected_by_blocks` represents Java's `isAffectedByBlocks` contract: AbstractArrow's `ID_FLAGS & 0x02` gates Arrow/SpectralArrow/Trident (bit `0x01` remains critical; Entity shared flag `0x04` is unrelated), while ShulkerBullet's Java override returns `!isRemoved()` and therefore allows contact despite constructor `noPhysics=true`. Removal is handled by owner-store lifecycle. Living sources share the Java client-authority gate: local player or living mount controlled by the local first Player passenger. Horse-family/Camel/Nautilus require their synced saddle; Pig requires saddle + local carrot-on-a-stick; Strider requires saddle + local warped-fungus-on-a-stick. Second-seat local and remote first-seat passengers do not qualify. The local player's current main/offhand held items feed steering checks.
- `app/phases/in_game.rs`: local-player source runs after its fixed physics tick through one `local_block_effect_particle_requests` gate shared by PowderSnow and step-on RedstoneOre. Remote living entities do not run Java's client-side `applyEffectsFromBlocks`; locally controlled living mounts do. After actual item/projectile owner ticks, the nonliving adapter samples `VehicleState`/projectile transforms and `ItemEntityStore` positions. Pickup animation snapshots and display-only projectile interpolation are not consulted. Requests use the existing typed particle path.
- The local/remote living paths use fixed-tick positions. Nonliving paths use dedicated owner movement history (`particle_prev_position`) rather than render interpolation or pickup snapshots. Vehicle movement history is consumed once per fixed tick; vehicle spawn/teleport and item spawn/teleport reset it, while item physics captures the prior position at tick start. Removal follows owner-store deletion.

RNG is injected into the shared request function for deterministic tests. Production calls use tick/entity-derived seeds, preserving independent 1/2 callback chances and uniform `[-1,1]` velocity samples without depending on render interpolation.

## Focused validation

Tests in `entity/particle_tick.rs` cover seeded option/kind/position/velocity, per-cell random branch, multiple touched cells, edge vs non-overlap, living adjacent contact with `getInBlockState` air, nonliving adjacent contact under the OR branch, stationary, caller-level noPhysics rejection, remote living suppression, local-player and locally controlled horse output, unsaddled horse/Pig without steering/remote first passenger/second-seat negatives, Arrow/SpectralArrow/Trident ID_FLAGS 0x00/0x01/0x02/0x03 with unrelated shared flag 0x04, Happy Ghast synced still-state, and vehicle/item spawn-teleport-remove lifecycle.

Verification from `Client/` on the final implementation snapshot:

- `mise exec -- cargo test -p pomme-client --locked --profile dev-fast powder_snow -- --test-threads=1` — exit **0**, **5 passed**. An initial run failed one spectator-condition assertion because the test used a seed that selected Java's `nextBoolean() == false` emission branch; the fixture was corrected to reuse the already-selected seed known to emit. Production conditions were not weakened.
- `mise run test` on the contact-authority snapshot — workspace client **1,764 passed / 0 failed**, protocol **52 passed / 0 failed**, singleplayer **1 passed / 1 ignored** (includes the authority/flag fixtures).
