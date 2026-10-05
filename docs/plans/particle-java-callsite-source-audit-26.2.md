# Java 26.2 particle source audit — merged snapshot

## Scope and audit result

This document supersedes the earlier “progress/extraction only” summary. The eight latest reviewed shards were merged into `particle-java-callsite-inventory-26.2.csv`; the CSV keeps the original **146 source rows**, zero-based index, path, and order. The merged per-callsite JSON is stored in each row's `merged_callsite_audits_json` column.

Counts use two deliberately separate denominators:

| Measure | Count | Meaning |
|---|---:|---|
| Mechanical callsite candidates | **303** | Extracted Java contexts across all 146 source rows. This includes method/API declarations, forwarding/delegation, helpers, and server no-op cases. Not particle-production count. |
| Java particle-generation operations | **254** | Syntactic particle-sink operations after excluding declarations, helper/API-only records, forwarding/delegation, command wrappers, and two current no-client sources. Not unique particle types/events and not proof of Rust parity. |
| Structured semantic records complete | **303 / 303** | Has a per-callsite Java method, guard/state, option origin (or justified `not_applicable`), Rust production entry, and evidence/status. |
| Structured semantic records incomplete | **0 / 303** | The prior **109** incomplete count was a merge/schema recognition failure, not 109 missing Java/Rust audits: shard 0 had 39 records under `semantic_audit`, shard 1 had 39 under `audit`, and shard 5 had 31 nested under `semantic_audit.callsite_audits`. All 109 are now mapped to one canonical per-callsite `semantic_review` shape. |

The 254 operations divide by audited Java-side classification into **221 client/event/particle-internal operations** (entity callbacks/delegations 84, block/fluid/interaction 84, client packet/world callbacks 21, LevelEvent 18, particle-engine internal children 14) and **33 server `sendParticles` source operations** that are delivered to clients by packet. The 33 server operations are not a request to recreate those emissions locally. The count excludes the `ServerLevel` overload declarations and forwarding calls.

The remaining **49** candidates have an explicit non-source disposition in the merged JSON: 19 helper/unused utility records, 12 non-sink helper delegations, 9 command-wrapper/delegation records, 4 `ServerLevel` API declarations, 3 `ServerLevel` forwarding overloads, and **2 server/no-client no-ops** (`SquidFleeGoal` and the unsynchronized Animal.inLove tick). `candidate_disposition` distinguishes these from the 254 source operations; `candidate_disposition_counts` aggregates them per original CSV row.

The CSV classifier counts invocation-site operations, not emitted particle quantity: e.g. a loop that emits five particles is one Java operation. It also does not certify the accuracy or completeness of implementation.

## Fields and mechanical checks

The original `particle-source-audit-reviewed-0..7.json` files are preserved as the reviewers' source evidence and may contain their earlier `audit_status` at the time of review. The **current** per-callsite overlay is the CSV's `merged_callsite_audits_json` / `current_recheck_status` / `current_connection_or_gap_summary`; do not read an archived shard's original status as the final snapshot.

Each merged callsite record carries:

- Java line, enclosing method line/signature, and invocation expression;
- Java guard/state context and particle kind/options provenance; fixed `ParticleTypes.*` calls identify the constant/default option, while dynamic/helper calls identify their argument, metadata, registry, or caller source. API declarations, server no-ops, and non-emitting declarations use a reasoned `not_applicable` rather than an invented option;
- Rust production entry, Java-side category, evidence, and audit status;
- a `machine_semantic_fields_complete` boolean requiring nonempty method, guard, options, Rust-entry, classification evidence, and status fields;
- a `candidate_disposition` distinguishing source operation, API declaration, helper, delegation, or server no-op; and
- an `is_generation_callsite` boolean for the 254-operation count; and
- current recheck notes where a previously reported discrepancy has since been reconciled.

The 109-record correction recovers existing evidence from schema variants without restarting the 146-row review: shard 0's `semantic_audit` (39), shard 1's `audit` (39), and shard 5's nested `semantic_audit.callsite_audits` (31) are retained in their source JSON and exposed uniformly as `semantic_review`. **Additional Java method/caller or Rust production-source rereads required for these 109 records: 0**; the per-callsite details were already present in those differently shaped fields. **Remaining semantic-field gaps: 0.** The shard-5 server-API cases use the recorded per-callsite signature/category and explicitly mark option provenance not applicable where the API merely forwards caller options. Fixed `ParticleTypes.*` option claims are grounded against `minecraft-26.2-decompiled/src/net/minecraft/core/particles/ParticleTypes.java` declarations; dynamic options retain their existing caller/condition/metadata/registry provenance.

The check leaves genuinely absent evidence incomplete instead of promoting a generic candidate/excerpt to a semantic audit. `semantic_audit_complete` means the record has a structured source audit; it does **not** mean that the Rust particle is Java-identical, tested, or visually verified. The separate `is_generation_callsite` count is 254; schema normalization and the later current-recheck overlay are distinct from the generation-operation count.

## Current implementation recheck (latest verified snapshot)

The following findings were valid in older intermediate snapshots and are now **closed in the current worktree**. The original Java source identities and owner boundaries are unchanged; the table below separates production paths from the focused evidence.

| Java source | Rust owner / path now connected | Automated evidence in the final snapshot |
|---|---|---|
| `ClientExplosionTracker` / `ClientPacketListener.handleExplosion` primary and weighted options | `net/handler.rs` preserves typed `ParticleOptions`; `app/core.rs` queues explosion packet; `ParticleStore::add_explosion_packet_particle` decodes through the existing native typed-options codec and common provider endpoint. No five-kind whitelist remains. | `particle::tests::explosion_packet_options_use_shared_typed_particle_spawn_boundary` and `net::handler::tests::explosion_ingress_preserves_the_complete_native_payload` pass. Test atlas includes the GUST descriptor required by the actual provider; weighted packet options use the same provider boundary. |
| `Minecraft.continueAttack` → `MultiPlayerGameMode.continueDestroyBlock` → `ClientLevel.addBreakingBlockEffect` | `player/interaction.rs::continue_attack` emits once when continuation succeeds; `ParticleStore::add_breaking_block_effect` owns Java shape-bounds sampling, hit-face offset and terrain material/physics. The final destroy volume burst remains separate. | `continued_mining_routes_one_face_particle_and_never_duplicates_final_break_burst` passes. |
| `DragonEggBlock.useWithoutItem/attack/teleport` | `InteractionState::start_use_item` and attack input route through the existing store. Java `BlockBehaviour.useItemOn` returns `TRY_WITH_EMPTY_HAND` by default regardless of held stack; `MultiPlayerGameMode` invokes `useWithoutItem` only on MAIN_HAND and only when `suppressUsingBlock` is false. Rust therefore permits a nonempty main-hand stack, gates offhand and preserves sneak-with-held-item suppression. | `dragon_egg_use_and_attack_emit_exactly_128_portals_only_for_valid_candidate` passes with both nonempty Dirt main hand and empty main hand, plus attack and no-candidate/border/build-height conditions. |
| `AbstractArrow` / `Arrow` | `EntityStore` retains AbstractArrow critical flag bit 0 at metadata index 8 and emits four CRIT particles while flying. Arrow colored trail is 2 per flying tick (including zero motion), 1 per fifth in-ground tick; Arrow event 0 separately emits the 20-particle color pickup/despawn burst. | `critical_arrow_metadata_produces_four_crit_particles_per_flying_tick`, `tipped_arrow_trail_matches_flight_ground_cadence_and_resets_on_reuse`, `tipped_arrow_entity_event_zero_emits_twenty_color_particles_only_when_colored` pass. |
| `ClientLevel.animateTick` / block/fluid/weather callbacks | `world/particle_tick/blocks.rs::sample_positions` uses the shared 667-pair probe stream and retains the recovery-backed Java attribute/tag/shape/heightmap behavior. `potent_sulfur_state` and BaseFire's `canBurn(below) || sturdyTop(below)` are verified. | Block sampler tests cover actual registry states and passed as part of focused and full-suite runs. |
| `BellBlockEntity`, `PowderSnowBlock`, candle/egg, RedstoneOre and water-bucket actions | Each action/contact caller connects once to typed `ParticleSpawnRequest` then `ParticleStore`; Bell action is packet-owned, PowderSnow uses actual client-caller movement/contact, Candle has its explicit empty-hand/build/lit/hit gates, Egg follows default interaction fallthrough, Ore/bucket are input-owned. | Bell 9, PowderSnow 5, candle 2, Dragon Egg 1, Redstone Ore 7, water bucket 3, mining 1 focused tests pass. |
| `ClientLevel.createFireworks` empty explosion list | Event 17's one-shot snapshot selects Java's 2–4 POOF fallback for absent/empty lists; no false starter/sound/flash. | `firework_event_uses_poof_for_absent_or_empty_explosions_and_claims_once` passes. |
| `LevelEventHandler` 3020 and Guardian Elder GameEvent 10 | Core event branch emits the correct 30 ominous detector + 20 `TRIAL_OMEN`/`SOUL_FIRE_FLAME` pairs (70 total) and the Elder Guardian model request at player position. The Elder root uses camera-relative overlay transform, not spawn-position placement. | `level_event_3020_emits_exact_java_particle_pairs`, `guardian_elder_game_event_uses_player_position_and_param_one_for_sound`, and CPU real-model/camera-matrix test pass. |
| Ender Dragon late death | Independent death counter emits `EXPLOSION_EMITTER` inclusively at 180–200 while preserving phase burst and health-zero Explosion; not removed by LocalPlayer's 20-tick cleanup. | Late-death boundary, independence and lifecycle focused tests pass. |
| `Explosion` typed option whitelist | GUST, both Gust emitters and all typed native Explosion options now use the common provider endpoint; audio/knockback and one-remap boundaries remain separate. | Both typed provider and packet-ingress tests pass. |
| Animal/Tamable/Ocelot/Allay event handlers | EntityEvent 6/7 and Ocelot 40/41 route the seven-particle HEART/SMOKE taming/trust bursts; event 18 routes seven Animal HEART particles or Allay's override of three. Animal.inLove's periodic tick has no synchronized client timer and its server-side `Level.addParticle` is a no-op, so that source is not locally replayed. | Animal/Allay event 18 and Tamable/Ocelot 6/7/40/41 source tests pass. |
| Dolphin and Fox event handlers | Dolphin event 38 creates seven HAPPY_VILLAGER particles; Fox event 45 creates eight ITEM particles from the synchronized mouth stack and rotated look vector. | Dolphin event and Fox event payload tests pass. |
| Thrown Snowball/Egg impact | EntityEvent 3 routes Snowball's eight item/snowball particles and ThrownEgg's eight item particles with velocity spread; typed item payload/components are retained. | `thrown_snowball_and_egg_entity_event_three_keep_item_payloads` passes; the handler event-dispatch test covers event 3 forwarding/death order. |
| `LivingEntity.handleEntityEvent(46)` | Packet-owned random-teleport event now emits 128 PORTAL requests from previous/current positions and true entity dimensions; local player uses its own fixed-tick positions. | Deterministic geometry/count test; existing handler test covers event 46 dispatch. |

The independent gate's remaining water-bucket replaceability concern is closed by generated state metadata, not by another block-name list: `tools/stategen/StateDump.java` invokes the public, world/position-independent `BlockState.canBeReplaced(Fluids.WATER)` and `tools/blockgen` compacts it as `w`. Native Java results demonstrate both wrong directions in the old collision-shape estimate: `moving_piston` was guessed replaceable because its shape is dynamic, but Java returns false (`forceSolidOn`); `big_dripleaf` was guessed not replaceable from its bounds, but Java returns true (`forceSolidOff`). The full bucket input test also verifies that the unwaterlogged sign/ladder `LiquidBlockContainer.canPlaceLiquid` branch can mask `canBeReplaced` when selected, while waterlogged containers deny that route and fall back to the adjacent candidate. See `particle-water-bucket-native-replaceability.md` for Java query, generated-table comparison and final serial test evidence.

No prior **confirmed** explosion-option, mining-hit, Arrow (critical/color/event 0), Dragon Egg, Bell, PowderSnow, firework, Elder/3020, Dragon late-death, Animal/Allay breeding event, taming/Ocelot event, Dolphin/Fox event, thrown Snowball/Egg impact, or living teleport-event source finding remains open in the current source snapshot. The unsynchronized Animal.inLove periodic tick is a documented no-client-source case, not a missing Rust simulation. This is not a claim that all 254 Java operations have individually passed Java-vs-Rust visual comparison.

## Remaining confidence limits

- `303/303` extracted candidates have structured semantic fields in the 146-row audit; `254` are Java particle-generation operations and `49` are API/helper/no-op/delegation candidates. Two no-client cases are the Squid server-AI base `Level.addParticle` no-op and Animal's unsynchronized love-timer call. These are audit denominators, not behavior-completion counts. The CSV inline per-callsite overlay and current columns mark the rechecked ClientLevel, AbstractArrow, Arrow, Dragon Egg, BaseFireBlock, RedstoneOreBlock, LivingEntity and animal/projectile event routes; archived shard `audit_status` values are historical.
- The full client workspace suite result is documented in `particle-integration-verification.md`; it is rerun after the additional Animal/Dolphin/Fox/throwable-event tests. A passing suite does not prove exact Java visual parity.
- Custom biome-level visual lightmap attribute overrides and some custom/composite item component selector values are still not fully supported/verified. Vanilla biome visual lightmaps have no such override in the 26.2 registry.
- GUI/GPU A/B, Java-client comparison and original screenshot particle identification remain unperformed. The decompile tree contains client classes, but its README documents the official server bundle as the source artifact; client class provenance is not independently verified.

## Requested false-positive and path rechecks

### Villager disagreement: resolved as connected

The shard-6 `AbstractVillager` “missing” record conflicts with shard-7 `Villager` records. Current `app/core.rs` has `12..=14 | 42 if kind_name == "villager"`, maps these IDs to Heart / AngryVillager / HappyVillager / Splash, and emits five particles for each event. This is the client route for Java's `Villager.handleEntityEvent` calls into `AbstractVillager.addParticlesAroundSelf`. Both rows are marked connected in the merged CSV; the older shard-6 missing claim is stale. This checks event-driven production, not all Villager interaction branches.

### Squid line 318 and Animal line 85: no local client producer

Java `Squid.java:318` is in `SquidFleeGoal.tick`, reached through server `serverAiStep` / goal ticking. It calls the base `Level.addParticle` overload whose server implementation is empty; `ServerLevel.sendParticles` is separate.

Java `Animal.java:85` calls the same local `Level.addParticle` overload from `Animal.aiStep`, but `inLove` is a private, unsynchronized field. `setInLove` is reached on the server-player path; the client does not receive the 600-tick counter through entity metadata, while the server's local addParticle call is a no-op. The actually observable breeding source is `Animal.handleEntityEvent(18)` at line 250, now handled as seven hearts; Allay overrides that event with three hearts. Both `Animal.java:85` and `Squid.java:318` are classified as server/no-client no-ops and excluded from the 254 source-operation count. No server AI or unsynchronized love timer is locally resampled.

### Critical and magic-critical local-vs-packet path: no duplicate local spawn

Java `MultiPlayerGameMode.attack` invokes `Player.attack` on the local player after sending the attack packet. `Player.attack` runs `attackVisualEffects` (and therefore `crit` / `magicCrit`) only inside `if (wasHurt)`, where `wasHurt` is `target.hurtOrSimulate(...)`. `Entity.hurtOrSimulate` dispatches to `hurtClient` on a client `Level`; ordinary `LivingEntity` uses the base `hurtClient`, which returns `false`. Critical eligibility also requires a living target. Thus the successful living-target crit/magic-crit particle is **not directly emitted from the local client prediction path**. On the authoritative server, `ServerPlayer.crit` / `magicCrit` call `sendToTrackingPlayersAndSelf` with Animate actions 4/5; Rust `net/handler.rs` routes these to `NetworkEvent::CriticalHit`, and `core.rs` creates the tracking emitter. Do not add a speculative local copy: it would duplicate the self-delivered packet path. This conclusion is based on the caller/guard chain, not merely the existence of `LocalPlayer.crit` methods.

### Other known reports reconciled against current entrypoints

Current source inspection confirms these previous “missing” claims are stale or the owner path is now connected:

- Elder Guardian GameEvent 10 is consumed by the current core path; LevelEvent 3020's `TrialSpawnerDetectionOminous`, `TrialOmen`, and `SoulFireFlame` outputs are present.
- Bell BlockEvent action 1 connects `core.rs` to `world::block_entity_particle::on_bell_block_event`; the block-entity tick emits resonance particles.
- Candle extinguish and Dragon Egg attack emissions are connected through `player/interaction.rs`.
- Powder-snow contact requests are called from the fixed in-game tick for remote living entities and the local player.
- Mooshroom interaction output is consumed by `core.rs`'s `AnimalFx::MooshroomStew` branch. The common animal interaction queue is drained and dispatched in the live input path.
- AreaEffectCloud particle metadata travels from `net/handler.rs` through `NetworkEvent::ParticleMetadata` and `core.rs::apply_particle_metadata` to `misc_particle_cloud_options` / `set_cloud_particle`.
- LivingEntity break events 47–52/65/68 map to equipment slots, snapshot the pre-clear stack, and create ITEM options in `core.rs` before the subsequent equipment update.
- `water.rs` exempts honey and obsidian-tear drips from water/lava collision removal; `world/particle_tick.rs` uses the WaterFluid `source || falling` predicate; the audited honey-drip path checks the block/tag/shape conditions.

These are **static call-path observations**, not test results. “Connected” does not claim complete Java parity.

## Deliberately not classified as bugs

- Rust and Java Gaussian random-number streams are not identical. A seeded sample sequence difference alone is not an implementation bug; a concrete distribution/formula/guard mismatch is required.
- Dragon head positions reconstructed from the transmitted history/state are not inherently a bug. Report a gap only if a specific required history input or Java formula is missing.
- The original particle screenshots still do not identify a particle type, so this audit cannot attribute the original visual symptom to a specific source.

## Audit and verification boundaries

- **No structured semantic fields remain incomplete (303/303).** The 109-record correction recovers existing evidence from schema variants. The 254 operations and 49 non-source records are separate from tests and implementation parity; no assertion is made that all Java source outputs match visually.
- The 146-row master keeps all original indices, paths, and order. Current source rechecks are overlaid in CSV `current_recheck_status`, `current_connection_or_gap_summary`, and per-callsite `current_recheck`; original shard JSON statuses are preserved as history.
- The 109-record schema normalization itself did not reread Java/Rust source. The later targeted integration rechecks did read the concrete Java conditions/Rust production paths listed above.
- The whole workspace test run on the current snapshot is documented in `particle-integration-verification.md`. Automated source-audit completeness and a green suite are not GUI/GPU visual parity. The original screenshot particle remains unidentified.

## Commands / checks for this merge

All commands ran from `C:\Users\yuzum\Desktop\mine_rust` and only read/rewrote audit CSV/Markdown data (no Rust source, Cargo, or GUI actions).

- Initial Python schema comparison: shard 0 = 39 `semantic_audit` records; shard 1 = 39 `audit` records; shard 5 = 31 nested `semantic_audit.callsite_audits` records. This exactly matches the prior 109 incomplete count; it exposed the merger's field-name/record-depth mismatch before any source reread.
- Initial Python normalization check (historical merge): **PASS, exit 0** — retained 146 rows, paths, indices/order, 303 candidates and 303 complete / 0 incomplete semantic records.
- Final current-recheck check: **PASS, exit 0** — candidate JSON + CSV totals now **303 candidates / 254 generation operations / 49 non-source / 303 complete / 0 incomplete**; the inline callsite overlays and source-row summaries match for ClientLevel, AbstractArrow, Arrow, DragonEgg, BaseFire, RedStoneOre and Animal.
- Post-gate CSV correction: **PASS, exit 0** — retained all 146 row identities/indexes/order and the same 303/254/49/303/0 totals; replaced all **22** remaining shard-5 `current_recheck` extraction-only warnings with the current `semantic_review` record's method/category/disposition/status while retaining the distinction between audit completeness and implementation/test/visual completion.
- Final code validation is not docs-only: see the final serial Cargo commands/results in `particle-integration-verification.md`. The visual limitations remain explicitly open.
