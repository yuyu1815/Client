# Java 26.2 common Entity/LivingEntity client particles

Reference source is `minecraft-26.2-decompiled/src/net/minecraft/world/entity/Entity.java` and `LivingEntity.java`. Common tick output is owned by `entity::particle_tick::tick_common`, called once from `EntityStore::client_particle_requests` through the existing `in_game.rs` entity request hook. The particle store still owns provider creation/limits. Remaining non-common classes and their missing fields/phases are listed in [`particle-entity-tick-followup-gaps-26.2.md`](particle-entity-tick-followup-gaps-26.2.md).

## Java helper coverage

| Java helper / call site | Java condition and count | Rust owner / inputs | Status |
|---|---|---|---|
| `Entity.baseTick` → `canSpawnSprintParticle` → `spawnSprintParticle` (`Entity.java:575–576, 1630–1648`) | Every base tick satisfying the inherited sprint predicate: sprinting, alive, not crouching/spectator, not touching water/lava. Emits one native `BLOCK` particle unless the foot state has invisible render shape. | `entity/particle_tick.rs::tick_living`; metadata index 0 (`EntityFlags.sprinting`), metadata pose, health, PlayerInfo game mode, previous sampled water/lava contact, synced motion, dimension AABB, native block state at legacy foot pos. | Implemented. Native block option, exact `position.y + 0.1`, width offset/clamping, and `(-4*vx, 1.5, -4*vz)` velocity are preserved. `on_ground` is not a predicate in Java's helper. |
| `Entity.updateFluidInteraction` → `doWaterSplashEffect` (`Entity.java:1569–1578, 1591–1615`) | On transition from not touching water to touching water, except first tick. For each particle family `N = ceil(1 + width*20)`: N `BUBBLE` and N `SPLASH`, at `floor(y)+1`. | `entity/particle_tick.rs::fluid_contact` + per-entity `ClientEntityParticleState`; full deflated bbox, each overlapped fluid cell's real top, entity eye height, previous/current water transition, entity width and motion. | Implemented. Water-column sampling is bbox-based, not center-block sampling. A one-tick initialization prevents a spawn-in-water splash. `N` is 13 for a 0.6-wide entity (26 total requests). |
| `LivingEntity` water entry overrides (`Player.java:1352–1355`, `ExperienceOrb.java:232–234`) | Spectator Player suppresses inherited splash; ExperienceOrb suppresses it entirely. | Player spectator status comes from synchronized `PlayerInfoUpdate` / `TabList`; ExperienceOrb kind check. | Implemented. |
| `LivingEntity.tickEffects` client path (`LivingEntity.java:840–855`) | Choose one synchronized effect-particle option; emit with probability `1 / ((invisible ? 15 : 4) * (ambient ? 5 : 1))`. | Existing `in_game.rs::living_effect_sources` and `ParticleStore::add_living_effect_particles`; effect-particle/ambient metadata retained by `handler.rs`. | Already equivalent; deliberately not emitted from `tick_common` to avoid duplication. |
| `LivingEntity.checkFallDamage` (`LivingEntity.java:373–401`) | Landing block particles are issued by `ServerLevel.sendParticles`, not local `addParticle`. Generic `Entity.checkFallDamage` calls block `fallOn`; honey's client particles use the entity event path. | Existing LevelParticles receive path and `EntityHoneyParticles` event path. | Packet/event-owned; no local duplicate. Generic fall/landing direct client request count is 0. |
| `LivingEntity.spawnItemParticles` / `breakItem` (`LivingEntity.java:1382–1391, 3310–3324`) | Five item particles on an equipment-break entity event; not a periodic use/eat tick helper. | Item stacks/equipment are retained, but entity event IDs 47–52, 65, 68 are not routed to this helper in the current dispatch. Core event dispatch is outside this change's ownership. | Not implemented here; explicit event-path follow-up (see residual list). `LivingEntity.java` has no per-tick eat/use `addParticle` call to reproduce. The using/offhand metadata and equipment are already retained. |
| `LivingEntity.handleEntityEvent` portal / poof / drown (`LivingEntity.java:1940–1958, 1995–2045`) | Portal event 46 emits 128; poof emits 20; drown emits 8 bubbles. | Existing `EntityParticleEvent` / `EntityPoof` / `EntityDrownParticles` handlers in net/core. | Event-owned and excluded from tick generation. No duplicate requests added. |

## State provenance

| Required condition/state | Java source of truth | Rust availability |
|---|---|---|
| Sprinting / crouching / swimming pose | Entity shared-flags metadata index 0; pose metadata index 6 | `handler.rs` forwards `Byte` and pose; `EntityStore::apply_entity_data` stores flags/pose. |
| Grounded state | Relative move / teleport packet on-ground bit | Already applied by entity movement handlers. Not used by `spawnSprintParticle`, but retained for fall/landing semantics. |
| Motion | Entity motion packet or relative/teleport update | `EntityStore::set_living_motion`; existing `LivingEntity.velocity`. |
| Width/height/eye | Entity kind dimensions, pose, baby state, `minecraft:scale`, Salmon/Pufferfish/cube variants, Shulker attachment/peek | Native dimensions plus synced baby/scale/fish/cube metadata; Shulker progress AABB from attachment face and peek; box sampled over all covered fluid blocks. |
| Water/lava membership and previous tick | Local client world fluid state; not a wire metadata field | Reconstructed from loaded `ChunkStore` cells each fixed tick; prior edge stored on its LivingEntity/VehicleState. Missing surrounding chunks suppress a false transition, matching Java's loaded-region guard. |
| Spectator | PlayerInfoUpdate game mode, not entity metadata | Read from `TabList` UUID/game-mode state at the existing request hook. |
| Foot block options | World block state at `getOnPosLegacy` | Native `BlockState` is passed as `ServerParticleOptions::Block`; fluid and invisible render-shape states are rejected. |

## Deterministic checks added

- `sprint_particle_uses_synced_flag_ground_block_native_state_and_velocity`: exact block state, position, and velocity from synced sprint+motion.
- `water_entry_uses_full_bbox_and_emits_java_splash_count_once`: center block is dry while the entity bbox overlaps a water source; validates 13 bubble + 13 splash requests and no repeat next tick.
- `spectator_does_not_emit_sprint_particles_in_common_tick`: synchronized spectator suppression. Event particles have no tick-helper input path.
- Existing `synchronized_entity_client_particle_ticks_emit_vanilla_species_trails_once` and projectile test remain intact.

## Validation in this workspace snapshot

- `cd Client && mise exec -- rustfmt --edition 2024 pomme-client/src/entity/particle_tick.rs`: exit **0**; formatting was limited to the new file (no workspace-wide rustfmt).
- `cd Client && mise run check`: exit **0** (warnings only; includes unrelated concurrent-work warnings).
- `cd Client && mise exec -- cargo test -p pomme-client --locked --profile dev-fast entity::particle_tick::tests -- --nocapture`: exit **0**, **4 passed**, 0 failed (**1,614 filtered out**). Covers scaled/variant dimensions, spectator suppression, native block-state sprint request, full-bbox water entry/count/no-repeat.
- `cd Client && mise run test`: exceeded the 1,800-second tool timeout. The four common-helper tests all passed in the captured partial output. That output also showed failures in unrelated parallel particle work (`world::environment_particles`, `net::handler` particle dispatch, provider ownership, `particle::magic`, `particle::terrain_extra`, `particle::water`) and a handler queue test running >60 seconds; no final full-suite result/exit code was produced.

## Remaining concrete class/phase gaps (separate ownership)

- **`AbstractCubeMob::onSyncedDataUpdated`** (`monster/cubemob/AbstractCubeMob.java:174–182`), inherited by **Slime, MagmaCube, SulfurCube**: on size metadata change, if already touching water, random 1-in-20 call to `doWaterSplashEffect`. This is a metadata-transition particle trigger, not the common baseTick water-entry transition; the size-change edge is not recorded for common particles.
- **`LivingEntity.breakItem`** (`LivingEntity.java:1382–1391`) and `handleEntityEvent` slots 47–52/65/68: need handler event routing plus the existing core `EntityParticleEvent` consumer to read the exact broken slot stack and create five item particles. Equipment is already retained; no new field is missing. Not changed to avoid conflict with the event-dispatch owner.
- **`Rabbit.handleEntityEvent(1)`**: block sprint particle is an event emission, not baseTick. Existing handler/core event route owns it; generic predicate deliberately emits none for Rabbit.
- **`IronGolem.canSpawnSprintParticle`** (`animal/golem/IronGolem.java:130`): differs from base predicate: horizontal velocity squared > `2.500000277905201e-7` and random 1-in-5. Implemented in `tick_living` using retained motion and fixed-tick RNG.
- **`Player.doWaterSplashEffect`** only differs by spectator suppression (implemented from PlayerInfo); **`ExperienceOrb.doWaterSplashEffect`** is empty (implemented by kind check).
- **`LivingEntity.checkFallDamage`** server packet source must remain native LevelParticles. It is intentionally not reconstructed from remote on-ground/position deltas. HoneyBlock event 54 remains separately event-owned.
- **Item use/eating**: no per-tick `addParticle` call exists in these 26.2 `Entity.java` / `LivingEntity.java` sources. Use metadata (hand flags) and entity equipment are retained, but item-break and interaction event work remains outside the common tick hook.
