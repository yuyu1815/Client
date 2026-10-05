# Particle source follow-up: empty fireworks and tipped arrows

## Java reference (26.2)

- `minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientLevel.java:758-764`: `createFireworks` handles an empty explosion list by spawning `random.nextInt(3) + 2` `POOF` particles at the supplied `(x,y,z)` with velocity `(nextGaussian()*0.05, 0.005, nextGaussian()*0.05)`. Count is therefore **2–4**. The empty case returns before the normal explosion starter/sound/flash path.
- `minecraft-26.2-decompiled/src/net/minecraft/world/entity/projectile/FireworkRocketEntity.java:250-254`: event **17** forwards position, current motion, and synchronized explosion list to `ClientLevel.createFireworks`.
- `minecraft-26.2-decompiled/src/net/minecraft/world/entity/projectile/arrow/Arrow.java:83-109`: after `super.tick`, in-ground arrows call `makeParticle(1)` when `inGroundTime % 5 == 0`; flying arrows call `makeParticle(2)` every entity tick. `makeParticle` emits nothing for color `-1`; otherwise each particle is `ENTITY_EFFECT` at independent `getRandomX(0.5), getRandomY(), getRandomZ(0.5)` positions with zero velocity (horizontal offsets are independently within ±`0.5 * arrow width`; Y is within `[0, arrow height)`).
- `minecraft-26.2-decompiled/src/net/minecraft/world/entity/projectile/arrow/AbstractArrow.java:190-208`: in-ground time increments while in ground and resets to zero when the arrow is not in ground.

## Rust changes

- `pomme-client/src/app/core.rs`: `take_firework_event` consumes the existing `EntityStore::take_firework_event` one-shot snapshot and distinguishes nonempty explosions from absent/empty component payloads. Network entity event **17** uses the existing `ParticleStore::add_firework_starter` only for nonempty explosions; absent/empty lists use `firework_empty_poofs`, which now generates **2–4** `POOF`s with Java's position and velocity formula. A duplicate event whose one-shot claim returns `None` is now an explicit no-op rather than accidentally entering the POOF fallback. The regular starter/spark route, snapshot, and one-shot claim are unchanged. Empty lists do not enqueue a firework starter and thus do not create its blast/twinkle sounds or flash.
- `pomme-client/src/entity/mod.rs`: projectile client-tick processing emits **2** color-only Arrow `ENTITY_EFFECT` requests per flying tick, including zero-motion flight, or **1** each fifth in-ground tick. `VehicleState::arrow_in_ground_time` advances on fixed Arrow ticks in-ground and resets while flying; a fresh entity kind/spawn resets the timer and default color. Positions follow `getRandomX(0.5), getRandomY(), getRandomZ(0.5)`; color `< 0` suppresses the trail. AbstractArrow flag bit 0 at metadata index 8 now supplies Java's critical-arrow state and produces four `CRIT` particles along current motion while flying; in-ground arrows return before that branch. Entity event 0 now emits the separate 20-color `ENTITY_EFFECT` pickup/despawn burst for colored tipped arrows. These paths remain distinct from four-per-tick water bubbles and the SpectralArrow trail.

## Verification

Commands are run from `Client/` using the repository-pinned mise toolchain:

- `mise exec -- rustfmt pomme-client/src/entity/mod.rs pomme-client/src/app/core.rs` — exit 0.
- `mise exec -- cargo test -p pomme-client --locked firework_event_uses_poof_for_absent_or_empty_explosions_and_claims_once` — exit 0; **1 passed**, covering missing component, present-empty list, nonempty starter payload, duplicate event suppression, and POOF count/position/velocity.
- `mise exec -- cargo test -p pomme-client --locked tipped_arrow_trail_matches_flight_ground_cadence_and_resets_on_reuse` — exit 0; **1 passed**, exercising production `EntityStore::client_particle_requests`, two-per-flight-tick (including zero velocity), one-per-five-ground-ticks, color removal/re-addition, metadata updates, and entity-ID reuse reset.
- `mise exec -- cargo test -p pomme-client --locked --profile dev-fast critical_arrow_metadata_produces_four_crit_particles_per_flying_tick -- --test-threads=1` — exit 0; **1 passed**, checks metadata index 8 / flag bit 0, four exact CRIT positions and velocities, and in-ground suppression.
- `mise exec -- cargo test -p pomme-client --locked --profile dev-fast tipped_arrow_entity_event_zero_emits_twenty_color_particles_only_when_colored -- --test-threads=1` — exit 0; **1 passed**, checks entity event 0 yields 20 colored particles only for Arrow with a valid color.
- `mise run check` — exit 0 (`cargo check -p pomme-client --locked --profile dev-fast`).
- Those historical integration errors (`ChunkStore` test import, Elder Guardian `Position`/`DVec3`, and options equality) were resolved in the final integration. Latest workspace validation is recorded in `particle-integration-verification.md`; no earlier partial failure is treated as a current failure.

Full workspace tests and visual client verification were not run as part of this focused follow-up. Since the shared worktree is concurrently changing, the focused test successes are snapshot-specific; rerun them after the unrelated compile errors are resolved.
