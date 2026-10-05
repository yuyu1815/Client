# FireworkRocket particle parity — Java 26.2

## Scope and Java sources

References are `minecraft-26.2-decompiled/src/net/minecraft/world/entity/projectile/FireworkRocketEntity.java` and `minecraft-26.2-decompiled/src/net/minecraft/client/particle/FireworkParticles.java`.

- `FireworkRocketEntity.DATA_ID_FIREWORKS_ITEM` is synchronized as an item stack; its index-8 item metadata is already translated by the network handler to `NetworkEvent::EntityProjectileItem` and retained as `VehicleState::projectile_item`.
- `FireworkRocketEntity.tick` emits one `FIREWORK` particle per client tick with Gaussian X/Z (`sigma=.05`) and Y velocity `-deltaMovement.y * .5`; launch sound is `entity.firework_rocket.launch`, ambient category, volume `3`, pitch `1`.
- entity event 17 snapshots the rocket position/motion and ordered `Fireworks.explosions`; empty explosions retain the existing 2–3 Poof path. The nonempty path queues a typed `FireworkStarterRequest`, not a generic explosion and not a server `LevelParticles` packet.
- `Starter` is a no-render schedule: explosion `i` is emitted at life `2*i`; lifetime is `explosionCount*2-1`, plus 15 ticks if any explosion twinkles. It creates one flash per explosion and the Java large/small local/remote blast sound at life 0; delayed twinkle sound follows the extra delay.
- Java spark counts and geometry: small/large ball are 98/386 directions (`steps=2/4`, base speed `.25/.5`); star is 121 particles; creeper is 265; burst is 70. Shape/fade color are selected per spark; empty colors use `DyeColor.BLACK` firework RGB `0x1E1B1B`. Star/creeper paths match Java coordinate arrays and rotations. Burst uses rocket motion plus Gaussian offsets.
- `SparkParticle` carries `trail`, `twinkle`, and per-spark fade color into trail children. Trail cadence is the Java even-age condition before half-life; child age starts at half its own lifetime. `OverlayParticle` is a four-tick translucent flash with Java size/alpha curves.

## Rust ownership and flow

- `EntityStore` retains the synchronized item stack and one-shot event-17 state. Rocket tick creates local `Firework` flight-particle requests and launch sound is consumed once.
- `core.rs` validates event-17 against an existing FireworkRocket, obtains its component from the retained Azalea item stack, snapshots position/motion, and queues `FireworkStarterRequest`. Existing empty-fireworks Poof handling remains.
- `ParticleStore` owns pending/live firework starters and advances them with the existing particle tick. It creates shape sparks through `particle/magic.rs::firework_spark`, creates flash particles through a dedicated helper, and routes blast/twinkle audio through existing particle sound requests. The native server `/particle firework` provider remains a plain spark with trail/twinkle/fade disabled.
- The starter retains its own position/motion/explosion values; later entity removal cannot invalidate child emission. Normal `LevelParticles` handling is not replayed by the entity-event path.

## Verification status

Added focused unit tests for retained `Fireworks` metadata/event-once behavior, rocket flight-particle motion and launch-audio once semantics, all five shape child counts/speed families, multi-explosion two-tick spacing, blast/twinkle timing and snapshot position, child trail inheritance of color/fade/twinkle, and separation from plain server firework particles.

Verification was affected by concurrent edits in shared renderer/entity files. The final `mise run check` exited **0** after the entity-mob-phase integration was temporarily completed. The focused `cargo test -p pomme-client --locked particle::tests::firework_shapes_create_java_spark_counts_and_velocity_families -- --exact` still exits **101** before running tests, due to three unrelated current test-compile errors: missing `ParticleQuad::light_uv` initializer in `particle.rs:3411`, an ambiguous `[f32; N]` for `block_tint` in `renderer/lightmap.rs:64`, and a test passing `vision`'s `[[f32;4];256]` where a `Settings` value is expected in `renderer/lightmap.rs:129`. Firework-specific compile errors were corrected; the new tests remain unexecuted. `git diff --check` exited **0**, and `rustfmt --edition 2024 --check pomme-client/src/particle/magic.rs` exited **0**. A broader rustfmt check over shared modified files reports formatting differences in unrelated concurrent edits; no whole-file formatting was applied. No game GUI, Java game client, or visual A/B session was started.
