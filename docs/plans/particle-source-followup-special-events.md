# Special event source follow-up (Java 26.2)

## Guardian Elder Effect (GameEvent id 10)

Java reference: `minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java`, `handleGameEvent`, `GUARDIAN_ELDER_EFFECT` branch around line 1543.

- Always calls `level.addParticle(ParticleTypes.ELDER_GUARDIAN, player.getX(), player.getY(), player.getZ(), 0, 0, 0)`.
- Plays `SoundEvents.ELDER_GUARDIAN_CURSE` at the same player coordinates only when `param == 1`.
- Sound category/volume/pitch are `HOSTILE`, `1.0`, `1.0`.
- `net/handler.rs` already forwards this event as `NetworkEvent::GameEvent`. `app/core.rs` now consumes it without changing other GameEvent behavior, spawns `ServerParticleKind::ElderGuardian` with simple options, zero velocity, and normal limiter flags, then uses the existing `ParticleStore::model_render_requests` -> Elder Guardian model renderer path. No billboard or synthetic entity was introduced.
- `app/core.rs` plays the conditional curse sound at player coordinates through the existing positional audio API.

## Trial spawner ominous activation (LevelEvent 3020)

Java references:

- `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/LevelEventHandler.java`, case 3020 around lines 537–548.
- `minecraft-26.2-decompiled/src/net/minecraft/world/level/block/entity/trialspawner/TrialSpawner.java`, `addDetectPlayerParticles` around 320–328 and `addBecomeOminousParticles` around 307–318.

Case 3020 calls the ominous detector helper with data `0`, so it emits exactly 30 `TRIAL_SPAWNER_DETECTED_PLAYER_OMINOUS` particles regardless of packet data. It then emits 20 pairs: one `TRIAL_OMEN` and one `SOUL_FIRE_FLAME` at the same sampled location, sharing Gaussian velocity components (`nextGaussian() * 0.02`). Detector position uses the Java float sampling ranges; paired particles use block-center ±1 double sampling. All are simple particles with normal limiter behavior.

`build_level_event_particle_requests` now emits 70 requests total for id 3020: 30 detector particles plus 20 each of the two paired kinds. It no longer emits `OminousSpawning`. Other IDs 3011–3021 retain their existing cases; the replacement does not call block-entity pulse generation.

## Verification

- `mise exec -- cargo test -p pomme-client --locked --profile dev-fast app::core::tests::level_event_3020_emits_exact_java_particle_pairs -- --exact` — exit 0; 1 passed. Checks 70 total, 30 ominous detection, 20 same-position/same-velocity `TrialOmen` + `SoulFireFlame` pairs, Simple options, normal flags and Java bounds.
- `mise exec -- cargo test -p pomme-client --locked --profile dev-fast app::core::tests::guardian_elder_game_event_uses_player_position_and_param_one_for_sound -- --exact` — exit 0; 1 passed. Checks only event id 10 is accepted, event position is retained, and sound condition is exact `param == 1.0` (including non-1 and NaN).
- Both commands were run from `Client`; first compile completed, and subsequent focused run reused it. No workspace/full-suite run was requested here.
