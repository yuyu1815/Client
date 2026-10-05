# Bell resonance particle source follow-up

## Java reference (26.2)

`minecraft-26.2-decompiled/src/net/minecraft/world/level/block/entity/BellBlockEntity.java`:

- `triggerEvent(1, direction)` refreshes `nearbyEntities` when no cache exists or `gameTime > lastRingTimestamp + 60`, resets resonance ticks, and starts the swing.
- Each client tick increments swing ticks; at `ticks >= 5`, a live/nonremoved Raider strictly within 32 blocks starts resonance if `resonationTicks == 0`.
- Resonance increments to 40; the following tick ends it and calls `showBellParticles`.
- `EntityGetter.getEntitiesOfClass(Class, AABB)` defaults to `EntitySelector.NO_SPECTATORS`; this query does not add an alive/removed predicate. The cached candidates intersect `AABB(blockPos).inflate(48)`. For the emitted count, Java counts cached entities strictly within the 48-block sphere (including dead-but-not-removed entities and non-raiders); local player is part of that list. Raider emission additionally requires alive, not removed, tag membership and strict 48-block sphere distance.
- Each raider gets `clamp((nearbyCount - 21) / -2, 3, 15)` ENTITY_EFFECT particles. The shared color starts at `16700985`, is incremented by 5 for each particle, with zero velocity and position `((x + .5) + (entityX-x)/xzDistance, y + .5, (z + .5) + (entityZ-z)/xzDistance)`.
- Client-side glow and bell sound are not recreated locally; the server/client packet and sound paths own those effects.

## Rust implementation

- `StoredBlockEntity` owns the bell swing plus cached entity IDs, last ring time, and resonance timer/state.
- `Core`'s `BlockEvent` branch calls `on_bell_block_event` only after that function verifies packet block identity, actual world block identity, action 1, and Bell BE identity.
- `tick_block_entity_particles` advances bell resonance on the fixed tick and yields typed `ServerParticleKind::EntityEffect` requests to the existing `ParticleStore` path.
- Configuration- and play-phase `UpdateTags` resolve `minecraft:raiders` source IDs through the packet protocol entity registry to native `EntityKind`; a received empty entity-type tag replaces native fallback with empty membership, while absent registry data leaves fallback untouched. Configuration state retains the override through reconfiguration. Native membership fallback is the existing 26.2 `EntityTypeTag.RAIDERS` data (`evoker`, `pillager`, `ravager`, `vindicator`, `illusioner`, `witch`), not a mob-class heuristic.
- BE replacement/removal clears the owner state through the existing block-entity sync path; dimension reset replaces the chunk store.

## Checks

From `Client/`, serial `mise exec -- cargo test -p pomme-client --locked --profile dev-fast bell_ -- --test-threads=1` — exit **0**, **9 passed / 0 failed** on the final integration snapshot. This includes event identity, exact 32/48-block thresholds, dead/spectator filtering, current tag membership, cached entities and emitter output. The full workspace test result is recorded in `particle-integration-verification.md`.
