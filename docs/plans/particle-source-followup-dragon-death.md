# Ender Dragon death particle paths (Java 26.2)

## Reference behavior

The two `EXPLOSION_EMITTER` sources are independent:

- `minecraft/world/entity/boss/enderdragon/phases/DragonDeathPhase.java:29-36` runs `doClientTick`; its private phase timer emits when `time++ % 10 == 0`. This is driven by the synchronized dragon phase and may run while the dragon still has health.
- `minecraft/world/entity/boss/enderdragon/EnderDragon.java:468-476` overrides `tickDeath`; it increments `dragonDeathTime` and emits one emitter every tick when the resulting value is in inclusive range 180..=200. Its position is `(x + (nextFloat()-0.5)*8, y + 2 + (nextFloat()-0.5)*4, z + (nextFloat()-0.5)*8)` with zero velocity.
- The same Rust aggregate already emits one `EXPLOSION` each health-zero client tick. It remains separate from both emitter sources.

Dragon `tickDeath` removes the server entity once `dragonDeathTime >= 200` (`EnderDragon.java:497-514`); it does not implement the ordinary 20-tick living-death cleanup. In this client, living entities are removed by authoritative entity-removal packets; the 20-tick cleanup found in the client belongs to `LocalPlayer`, not remote mobs or dragons. No Dragon-specific local removal was added.

## Rust implementation

`pomme-client/src/entity/particle_bosses.rs` stores a dedicated `dragon_death_time` alongside the existing phase timer and phase-burst counter. `tick_boss_particles`, called from `EntityStore::client_particle_requests`, advances this clock once per distinct client game tick while health is non-positive, resets it when health is positive, and adds the inclusive 180..=200 emitter. The existing phase cadence and health-zero Explosion are unchanged. Phase metadata resets only phase-local state; positive health metadata resets the death clock. Existing spawn/remove lifecycle clears the whole boss state, protecting entity-ID reuse.

The Java server-only XP award and Dragon flight/phase AI are intentionally not reproduced.

## Tests and latest verification

Tests in `entity/particle_bosses.rs` exercise the real aggregate path, including 179/180/199/200/201 boundaries, health-zero Explosion before and during the late window, simultaneous phase and late emitters, duplicate calls for the same game tick, the unchanged 10-tick phase emitter cadence, health recovery, removal and ID reuse. The aggregate tests initialize the native protocol/block registry while holding the test protocol guard.

Serial commands from `Client/`:
- `mise exec -- cargo test -p pomme-client --locked --profile dev-fast dragon_death -- --test-threads=1` — exit **0**, **1 passed** (health recovery/removal/ID reuse).
- `mise exec -- cargo test -p pomme-client --locked --profile dev-fast dragon_late_death_burst_uses_its_own_clock_and_exact_window -- --test-threads=1` — exit **0**, **1 passed** (179/180/199/200/201 boundaries).
- `mise exec -- cargo test -p pomme-client --locked --profile dev-fast dragon_phase_burst_and_late_death_burst_remain_independent -- --test-threads=1` — exit **0**, **1 passed**.

Existing Dragon model/history and Wither tests remain in the workspace suite.
