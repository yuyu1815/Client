# Mob-local particle source evidence (Java 26.2)

Scope here is only `entity::particle_mobs::tick`; the aggregate call from
`EntityStore::client_particle_requests` is intentionally deferred to the parent
integration pass. Java source of truth is `minecraft-26.2-decompiled/src/`.
`particle-entity-tick-followup-gaps-26.2.md` is a gap list, not behavioral
specification.

## Implemented in `particle_mobs.rs`

| Java source / source lines | Actual Java client behavior | Rust behavior |
|---|---|---|
| `world/entity/monster/Enderman.java:219-224` | `aiStep`, only on client, emits two PORTAL each tick with random x/z over 0.5×bbWidth, y over bbHeight minus 0.25, velocity `(rand-.5)*2, -rand, (rand-.5)*2`. No teleport timer/condition. | Two typed PORTAL requests per tick; per-entity/tick seeded visual RNG. |
| `world/entity/monster/Endermite.java:111-117` | `aiStep`, only on client, emits two PORTAL every tick with random body position and the same velocity distribution. | Two typed PORTAL requests per tick; no invented teleport gate. |
| `world/entity/monster/Guardian.java:208-212` | When synced `DATA_ID_MOVING` is true and in water, two BUBBLE requests behind its view vector at offsets `-1.5`; independent from attack-beam particles. | Two BUBBLE requests gated by synced moving metadata (index 16) and common current water state. Uses synchronized look rotation and scaled dimensions. |

The requests use existing `ParticleSpawnRequest` and `ServerParticleOptions`;
there is no provider physics duplication. Guardian `DATA_ID_ATTACK_TARGET` is
native index 17 / INT; metadata is handled in `EntityStore::apply_entity_data`
after existing ageable/player index normalization. Target changes reset the
client attack clock; a missing/dead target clears it next tick. Beam phase is
`clientSideAttackTime / getAttackDuration()`; Guardian duration is 80 and
`ElderGuardian.java:47-49` overrides it to 60. Bubbles follow Java's normalized
source-eye to target bbox-midpoint line with Java's per-step distance formula.

The indices are derived from Java's `defineId` inheritance layout, not guessed:
`Entity` owns 0-7, `LivingEntity` 8-14, `Mob` 15. Guardian declares moving
then attack-target at 16/17 (`Guardian.java:58-59,103-106`). `Raider` declares
celebrating at 16, and Spellcaster declares its next slot at 17
(`Raider.java:58,81-83`; `SpellcasterIllager.java:32,42-45`). Neither kind is
in the existing Ageable index normalization, so these native IDs pass unchanged
through `EntityStore::apply_entity_data`; generic SetEntityData decoding already
preserves INT/BYTE as `MetaValue::Int/Byte` in `net/handler.rs:1735-1765`.

Spellcaster metadata is `DATA_SPELL_CASTING_ID` native index 17 / BYTE in
`SpellcasterIllager.java:32,42-45`. Evoker and Illusioner retain spell enum IDs
0..5 (unknown IDs map to NONE), with colors NONE, (0.7,0.7,0.8),
(0.4,0.3,0.35), (0.7,0.5,0.2), (0.3,0.3,0.8), (0.1,0.1,0.2). Java's
`ARGB.as8BitChannel` floors `float*255`; generated requests use the resulting
opaque ARGB. Hand locations mirror `SpellcasterIllager.java:100-112` using
body yaw plus `cos(tickCount*0.6662)*0.25`, scaled ±0.6 horizontal and 1.8 up.

Tests exercise the actual store metadata path (`apply_entity_data`) for target
clock start/advance/change/remove and spell-on/two hands/spell-none, plus native
entity removal cleanup, ElderGuardian duration, and the independent two
swimming bubbles.

## Additional phase owner integrated after this module

`Warden`, `Breeze`, `Illusioner`, `Ravager`, and Slime/MagmaCube/SulfurCube phase state is owned by `entity/particle_mob_phases.rs`. Its real aggregate entry is `EntityStore::tick_mob_phase_particles`, called once from `EntityStore::client_particle_requests`; the old ambiguous production `tick(..., None)` entry point was removed. Pose/size metadata and removal cleanup are connected. Ravager events 4/39/69 dispatch through the entity-event path; ID 4 also preserves the existing GolemPunch signal, and ID 69 requests are enqueued immediately.

## Remaining confirmed source gaps

- `Witch.java:170-176`: event 15 owns the random 10–44 WITCH burst through the existing client event handler. It remains event-owned; do not duplicate it with a client tick.
- `LivingEntity.breakItem`: entity event IDs **47–52, 65, 68** still lack an ITEM particle consumer with the correct slot/components snapshot.
- Animal interaction helpers for Camel/Horse/Llama/Tadpole age-up and Brown Mooshroom stew exist but are not connected to actual successful interaction branches. Do not mark them integrated until caller logic checks Java item, age-lock, variant/effect, and success predicates.
- `particle-java-callsite-inventory-26.2.csv` remains incomplete; this file closes only the callsites listed in its evidence.

Server AI and explicit `sendParticles` are not resampled locally; they remain on the native `LevelParticles` route. Do not infer extra local phases from server-only callers.
