# Remaining Java 26.2 entity/projectile particle follow-up

> **Historical handoff, not current gap status:** the rows below predate the later animal, mob, phase, boss, misc, metadata, interaction and projectile integrations. Do not read their old `not yet owned` descriptions as a current implementation report. Latest source rechecks and final automated results are in [`particle-java-callsite-source-audit-26.2.md`](particle-java-callsite-source-audit-26.2.md), the CSV current-recheck columns, and [`particle-integration-verification.md`](particle-integration-verification.md).

This is the handoff after common `Entity` / `LivingEntity` tick helpers. Per-callsite Java lines remain in `particle-java-callsite-inventory-26.2.csv` and the source classification in `particle-entity-tick-evidence.md`. The common helper is implemented by `EntityStore::client_particle_requests` + `entity::particle_tick::tick_common`; see `particle-entity-common-evidence.md` for its contract/tests.

## Concrete remaining source/class gaps

| Java class/source | Remaining particle trigger | Required state/phase not yet owned (or correct existing owner) |
|---|---|---|
| `AgeableMob` | `forcedAgeTimer` every 4 ticks; age-lock cooldown every 2 ticks for up to 40 ticks | `isBaby` and `isAgeLocked` are synced, but client `forcedAgeTimer` / `ageLockParticleTimer` are not entity metadata. Java sets these on age-up / golden-dandelion interaction; need an interaction result/timer event, not infer a 40-tick burst from persistent `isAgeLocked`. |
| `AreaEffectCloud` | per-tick ring/idle particles | Cloud type is not retained as a dedicated entity model; requires `radius`, `waitTime`, `duration/age`, particle option/color, and shrinking/expansion phase from metadata/NBT. |
| `OminousItemSpawner` | every 5 game-time ticks, 1–3 `OMINOUS_SPAWNING` with flight vectors | Item payload may arrive in metadata; client age/phase and destination `flyTowards` target/source are not retained. Requires exact tracker/player/position target rule, not a center substitute. |
| `Animal` | love hearts every 10 ticks while `inLove > 0` | `inLove` countdown is server-only (no entity metadata); breeding event 18 already owns the discrete 7-heart burst. Do not synthesize a prolonged timer from event 18. |
| `TamableAnimal` | taming smoke/heart burst | Events 6/7 are event-owned by `EntityParticleEvent`; no tick timer required. Keep that dispatch, do not add duplicate client tick output. |
| `Allay` | heart particles in its local visual behavior | Need synced/local `DANCING`/duplication phase and the exact time gate; current entity state has no Allay dance/duplication clock. |
| `Bee` | nectar drip client tick | `bee_flags` already retains the nectar bit; remaining gap is exact pollination/nectar visual timer and fluid/block/top attachment calculation that selects drip kind/position. |
| `Camel` | happy-villager during dash/eligible local animation | Need dash transition/tick phase and client random gate; keep dash state distinct from unrelated pose. |
| `MushroomCow` | local smoke / variant particle loop | `variant` metadata exists; missing local cooldown/random phase and exact use/shear trigger. Its `sendParticles(EXPLOSION)` line is native packet-only, not a second local request. |
| `Dolphin` | paired `DOLPHIN` trail and happy-villager around self | Need swim/look vector at the particle source and the treasure/fish/boost animation phase (`gotFish`/goal state is not retained as synced metadata). |
| `AbstractHorse` | happy-villager / item particles for tame/feed interactions | Equipment and tame/eat flags are retained; remaining source calls are interaction/event-side (taming events 6/7 already event-owned) and need exact interaction result/timer, not AI goals. |
| `Llama` | happy-villager on tame interaction | Taming event owns the success/failure burst; do not create an extra tick timer. |
| `Ocelot` | tame/feeding heart/smoke visuals | Events 40/41 are routed to `EntityParticleEvent`; remaining needs are exact interaction result only if another client-local trigger is proven. |
| `Fox` | item particles when dropping/ejecting held item | Entity event 45 already carries the interaction trigger; held equipment and look direction are retained. Preserve the event path; no continuous tick phase. |
| `Tadpole` | happy-villager at growth transition | Baby state is synced; the growth/transform edge is not a repeating client state. Needs a transition event/age timer if Java confirms a separate local-only callsite. |
| `AbstractNautilus` | mouth bubble | Need actual mouth/attachment vector from its synced pose/yaw and current movement; no dedicated mouth pose is kept. |
| `Panda` | sneeze particle at sneeze phase | Need the sneeze timer/pose transition and body-yaw direction; current generic entity pose does not retain sneeze progress. |
| `Sniffer` | block particles at digging | Pose is synced, but its `AnimationState`/dig timer and exact block-under-bounding-box state at the call phase are not owned. |
| `Squid` | local movement bubble | `is_in_water` and body/tentacle motion exist; remaining gap is Java `Squid.aiStep` phase/tick gate and exact client position/velocity interpolation. `SQUID_INK` `sendParticles` stays packet-owned. |
| `Wolf` | local water splash animation | Need exact transition out of wet-shake / body bbox samples if not covered by common water entry. Its scute `sendParticles` call is packet-owned. |
| `EnderDragon` | body explosion particles / phase trail | Need client phase enum, head/body/segment positions and their rotations; these are not represented by one entity center. |
| `DragonDeathPhase` | explosion-emitter burst | Need the active dragon death phase, phase clock, and exact segment/body sample positions. |
| `DragonLandingPhase` | dragon-breath particles | Need active landing phase, current look/movement vector, and dragon body/segment attachment positions. |
| `DragonSittingFlamingPhase` | dragon-breath fan | Need active sitting-flaming phase, phase timer, head direction and mouth attachment position. |
| `WitherBoss` | smoke and colored head auras | `wither_invulnerability` exists; missing all three heads' current yaw/pitch/position and powered/phase interpolation needed to compute head origins. |
| `EnderMan` | portal during teleport | `is_creepy` is synced, but a teleport start/finish/client transition timer is not retained; use actual teleport event/phase and current AABB. |
| `Endermite` | portal trail | Needs the Java tick/teleport/random gate and tick-owned deterministic random phase; not infer from generic entity age if its local random gate differs. |
| `Guardian` | bubble trail toward target | Need synced target entity id/beam attack progress plus exact eye/look origin. Target/beam is currently absent from `LivingEntity`. |
| `Ravager` | colored head effect and poof on attack/stun | Need attack/stun/roar phase and head attachment position; do not approximate at entity center. |
| `Witch` | witch particles | This is event 15, not a repeating drink particle; preserve the event owner. `witch_drinking` is already synced for rendering, but does not replace event 15. |
| `Breeze` | jump-trail block particles and ground block burst | Need jump-trail tick count (5-tick lifetime), movement phase/ground contact and exact `getInBlockState`/supporting block. Attack/long-jump pose alone is insufficient. |
| `AbstractCubeMob` → `Slime`, `MagmaCube`, `SulfurCube` | 1-in-20 splash on size metadata change while already in water | Size and current water are retained/reconstructed; the `onSyncedDataUpdated` size-change edge and its one-shot RNG sample are not. Add a metadata transition flag in the cube owner, not another base water-entry splash. |
| `Illusioner` | cloud during illusion/teleport | Need illusion/teleport phase transition and local timer; server AI goal state is not synced. |
| `SpellcasterIllager` | paired colored hand effect | Need spell enum/casting tick state and exact hand offset. Generic `aggressive`/pose is not the spell id. |
| `Warden` | block particles while digging | Synced pose exists; missing Warden `AnimationState` start tick/elapsed phase and accurate support-block state. Do not emit during roar/attack unless Java's digging helper is active. |
| `AbstractVillager` / `Villager` | heart/angry/happy/splash around-self helpers | Discrete server interaction/breeding/anger events are already event-owned where IDs 12–14/42 apply. Any remaining timer requires exact `unhappyCounter`/interaction phase; do not port village AI. |
| `EvokerFangs` | crit burst during attack tick | Need synchronized fangs warmup/attack age and attack-side position offsets; no living entity tick state exists for this nonliving entity. |
| `EyeOfEnder` | bubble + portal trail | Need target block position/flight vector plus EyeOfEnder age/random tick phase; current nonliving state has position/velocity only. |
| `FireworkRocketEntity` | exhaust/firework/explosion | Explicitly deferred as requested. Requires `Fireworks` component shape/colors/fade/trail/twinkle parsing, rocket lifetime/flight state, and passing exact explosion initial state to `particle/magic.rs` provider. |
| `LlamaSpit` | 7 SPIT particles at packet spawn | This is spawn-time, not tick-time. Spawn velocity is available; needs a one-shot spawn visual request owned at AddEntity ingress (not every tick). |
| `AbstractBoat` | splash when above bubble column | Requires loaded-world bubble-column crossing/direction/100-tick random gate; boat bubble metadata is retained, but the exact local `onAboveBubbleColumn` trigger is not. |
| `MinecartFurnace` | `LARGE_SMOKE` 1-in-4 while fueled | `minecart_furnace_has_fuel` is retained; missing per-entity tick RNG phase. Keep its distinct condition from generic TNT smoke. |
| `MinecartTNT` | `SMOKE` every tick while its fuse > 0 | `tnt_fuse` is tracked for `PrimedTnt`, not minecart TNT. Need minecart-TNT ignition/fuse sync/event source and separate client countdown. |
| `ThrowableProjectile`, `AbstractHurtingProjectile`, `AbstractArrow`, `Arrow`, `SpectralArrow`, `Snowball`, `ThrownEgg`, `ShulkerBullet` | Remaining traces after common work | `AbstractArrow` crit burst is impact/event-only; `AbstractHurtingProjectile` trail kind is subclass-dependent; `ThrownEnderpearl` (separate row below) needs age/random portal phase. Existing underwater bubbles, tipped/spectral arrow, ShulkerBullet END_ROD and Snowball/Egg impact event are already in earlier implementation; don't duplicate. |
| `ThrownEnderpearl` | PORTAL every Java client tick | Need exact tick parity/random sample and entity age lifecycle; existing position/velocity alone does not encode its client random state. |

Server-only `sendParticles` sites remain native `LevelParticles`: `Entity`, `LivingEntity`, `MushroomCow`, `Squid`, `Wolf`, `ShulkerBullet`, command/AI helpers. They must not be resampled in these owners.
