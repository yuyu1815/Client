# Particle integration verification — latest snapshot (Java 26.2)

## Final gate after Bubble fixture and Brewing input corrections

This section supersedes the earlier test counts in the historical verification below. The last remaining nondeterministic 125-kind CPU-consumer assertion was a fixture-only mismatch: its real count-1 packet route used nonzero max speed, so a valid Gaussian velocity could move Bubble out of the fixture's one-cell source water before the real tick. The test now keeps `count=1`, sets `max_speed=0` for Bubble only, preserves provider jitter and production water-exit removal, and asserts packet creation → live-after-tick → quad/CPU-vertex output. It also retains the source still-water and stone-support assertions. The earlier seed-1 diagnosis and single-destination-cell control are recorded in `particle-bubble-consumer-stability.md`; test-only environment controls and manual projected-tick diagnostics were removed.

Latest serialized verification from `Client/` via mise, one Cargo process at a time:

| Check | Exit | Result |
|---|---:|---|
| `mise run check` | 0 | Type/borrow check successful; existing warnings. |
| 125-kind fixture, exact filter, 10 separate sequential runs | 0 each | **1 passed / 0 failed** each run; 10/10, 0.07 seconds per run. |
| `cargo test -p pomme-client --locked --profile dev-fast particle::tests -- --test-threads=1` | 0 | **82 passed, 0 failed**. |
| Same `particle::tests` filter with default test parallelism | 0 | **82 passed, 0 failed**. |
| `particle_asset_reload_replaces_transient_state_and_uses_new_ordered_frames` | 0 | **1 passed**. |
| `item_world_context_matches_packet_and_local_requests_across_switch_and_reload` | 0 | **1 passed**. |
| `water_bucket_block_use_consumption_matches_java_before_evaporation_particles` | 0 | **1 passed**; Brewing Stand non-sneak 0 particles, sneak 8; ordinary block 8; evaporation disabled 0. |
| First `mise run test` (normal workspace parallelism) | 0 | Client **1,769 passed**; protocol **52 passed**; singleplayer **1 passed / 1 ignored**; doc tests 0. |
| Second consecutive `mise run test` (normal workspace parallelism) | 0 | Same: client **1,769 passed**; protocol **52 passed**; singleplayer **1 passed / 1 ignored**; doc tests 0. |

The 9,894-file source/config hash manifest (Rust, JSON, TOML, YAML, lockfiles, shaders, CSV and validation files; excludes build output, `.git`, dependencies, and worktree snapshots) is `tmp/particle-final-source-hashes.txt` (ignored local manifest), SHA-256 `981ea62ab1d9da88f34a236e641b317c7462a097d7f4a0c14c91e979140837d5`. Hashes include JSON and build/tool configuration. No commit, reset, broad formatting, or Cargo concurrency was used.

The Brewing test covers the user of the interaction boundary and confirms the menu-opening Brewing Stand consumes non-sneak block use before the evaporating-water particle route; sneaking still follows the bucket path as Java does.

## Historical snapshot details

This document supersedes earlier in-progress blocker/test-failure summaries in this file and the source-follow-up notes. It records the verified shared workspace snapshot; it is **not** a claim of visual Java parity.

## Scope and current state

- Native identity is Java 26.2 / protocol 776: **125** particle kinds, names/IDs from the native registry.
- The actual client resource set has **111 particle descriptors** and **14 descriptorless kinds**. The 125-kind fixture uses those real asset-presence distinctions; descriptorless types must reach terrain/item/emitter/model consumers rather than fabricated JSON. The fixture exercises provider spawn/tick, UV/material selection, CPU `build_particle_vertices`, and Elder Guardian model draw records.
- `world/particle_tick/blocks.rs` recovery is present and is not the temporary test-only replacement: **1,990 lines / 63,928 bytes**, SHA-256 `83ff1b53697cec2f6f44280e2990455fbb382e64701425086270a1fc038f13c1`. It was reconstructed using transcript ranges and Java 26.2 source, not byte-for-byte restored from the 1,813-line predecessor. It retains shared sampler/probe input, block/fluid producers, Java attributes/tags, native shape/light/solid/heightmap queries; `potent_sulfur_state`, BaseFire's `canBurn(below) || sturdyTop(below)`, and both redstone ore variants are covered.
- Client/server resource reload paths rebuild/rebind particle assets while preserving mode and item-world context; live transient particle/emitter/firework state is cleared. GUI reload/resource-pack visual parity is not asserted by the CPU fixture.
- Native water-bucket replaceability now comes from Java's final `BlockState.canBeReplaced(Fluids.WATER)` result, generated as compact state-table key `w`; `BlockState` runtime query in `world/block/mod.rs` consumes it. This retains `forceSolidOn/Off` and Block overrides without another runtime approximation. All 1,196 blocks / 32,366 states have generated `w`; two directionally opposed real misses from the former shape estimate are `moving_piston` (old guess replaceable, Java false) and `big_dripleaf` (old guess not replaceable, Java true). Full `BucketItem.use → emptyContents` input tests show waterlogged sign/ladder container handling is evaluated separately from replaceability.

## Final serial verification

All Cargo commands ran from `Client/` with the repository-pinned mise toolchain. No parallel Cargo jobs were run.

| Command / focused filter | Exit | Result |
|---|---:|---|
| `mise run check` | 0 | `cargo check -p pomme-client --locked --profile dev-fast`; existing warnings only. Run after the final production changes. |
| `cargo test ... world::particle_tick::blocks::tests -- --test-threads=1` | 0 | **6 passed** on the recovered sampler snapshot (BaseFire, Sulfur, lit redstone stone/deepslate, shared boundary/tag/shape tests). |
| `cargo test ... powder_snow -- --test-threads=1` | 0 | **5 passed**; true swept entity contact and Java caller eligibility. |
| `cargo test ... redstone_ore -- --test-threads=1` | 0 | **7 passed**; input attack/use, living/vehicle/item contact, periodic block sampler and both ore variants. |
| `cargo test ... water_bucket -- --test-threads=1` | 0 | **3 passed**; full input destination/emptyContents particle path, container accepted/denied, neighbor/fallback, unloaded/border, evaporation and force-solid cases.
| `cargo test ... world::block::tests::water_replacement_uses_vanillas_final_per_state_query -- --exact --test-threads=1` | 0 | **1 passed**; generated native Java query, both force-solid outcomes, waterlogged true/false and dynamic block cases.
| `cargo test ... net::environment::tests::water_evaporation_uses_attribute_over_dimension_name_and_tracks_timeline_changes -- --exact --test-threads=1` | 0 | **1 passed**; actual `WATER_EVAPORATES` dimension/biome/timeline resolution. |
| `cargo test ... player::interaction::tests::candle_ -- --test-threads=1` | 0 | **2 passed**; explicit Candle/CandleCake empty-hand, `mayBuild`, lit and cake hit-height gates. |
| `cargo test ... dragon_egg_use_and_attack_emit_exactly_128_portals_only_for_valid_candidate -- --test-threads=1` | 0 | **1 passed**; use and attack, nonempty as well as empty main-hand default interaction, bounds/candidate gates, 128 PORTAL requests. |
| `cargo test ... continued_mining_routes_one_face_particle_and_never_duplicates_final_break_burst -- --test-threads=1` | 0 | **1 passed**; one successful continuation → one face particle; final volume burst remains separate. |
| `cargo test ... projectile_water_bubbles_match_java_families_and_lifecycle -- --test-threads=1` | 0 | **1 passed**; all 15 unique base-family kinds, four bubbles each, exact displacement, water/in-ground/removal cases. |
| `cargo test ... eye_of_ender_keeps_its_single_four_bubble_owner -- --test-threads=1` | 0 | **1 passed**; EyeOfEnder remains a separate 4-bubble owner. |
| `cargo test ... arrow_ -- --test-threads=1` | 0 | **13 passed**; includes Arrow flight/ground cadence, critical-arrow metadata trail, event-0 20-particle pickup burst, existing Arrow/Trident rendering tests. |
| `cargo test ... firework_ -- --test-threads=1` | 0 | **7 passed**; empty/absent list 2–4 POOF and one-shot starter, along with normal firework families. |
| `cargo test ... level_event_3020_emits_exact_java_particle_pairs -- --test-threads=1` | 0 | **1 passed**; 30 detections + 20 TrialOmen/SoulFireFlame pairs = **70**. |
| `cargo test ... guardian_elder_game_event_uses_player_position_and_param_one_for_sound -- --test-threads=1` | 0 | **1 passed**; GameEvent 10. |
| `cargo test ... elder_guardian_particle_maps_camera_matrix_and_real_model_parts_to_draw_records -- --test-threads=1` | 0 | **1 passed**; camera-relative overlay root and existing real model draw records. |
| `cargo test ... dragon_late_death_burst_uses_its_own_clock_and_exact_window -- --test-threads=1` | 0 | **1 passed**; 179/180/199/200/201 boundaries. |
| `cargo test ... dragon_phase_burst_and_late_death_burst_remain_independent -- --test-threads=1` | 0 | **1 passed**; phase burst and late death are distinct sources. |
| `cargo test ... dragon_death_clock_resets_on_heal_and_entity_lifecycle -- --test-threads=1` | 0 | **1 passed**; recovery, removal and ID reuse. |
| `cargo test ... gust_seed_providers_emit_three_gusts_with_java_scale_and_timing -- --test-threads=1` | 0 | **1 passed**; large/small child cadence and values. |
| `cargo test ... drip_removal_uses_real_world_fluid_type_and_skips_empty_type -- --test-threads=1` | 0 | **1 passed**; Honey/Tear EMPTY fluid type is preserved. |
| `cargo test ... bell_ -- --test-threads=1` | 0 | **9 passed**; event ownership, cached Raider identity/tag, count/color/position and strict radii. |
| `cargo test ... entity::particle_animals::tests -- --test-threads=1` | 0 | **13 passed**; Bee, Panda, age-lock, Dolphin tick/event, Allay/Animal breeding event, Tamable/Ocelot event, Fox event, Sniffer and Wolf source routes. |
| `cargo test ... living_teleport_event_emits_128_interpolated_portal_particles -- --test-threads=1` | 0 | **1 passed**; 128 Portal geometry/velocity requests across old/current positions and entity width/height. |
| `cargo test ... thrown_snowball_and_egg_entity_event_three_keep_item_payloads -- --test-threads=1` | 0 | **1 passed**; Event 3 handles default Snowball and typed Snowball/Egg item stacks with Java velocity behavior. |
| `cargo test ... client_entity_and_level_events_dispatch_particle_sources -- --test-threads=1` | 0 | **1 passed**; packet dispatch for relevant IDs including 18, 38, 40/41, 45, 46 and 67. |
| `cargo test ... explosion_packet_options_use_shared_typed_particle_spawn_boundary -- --test-threads=1` | 0 | **1 passed**; primary/weighted typed options through common providers, including GUST. |
| `cargo test ... explosion_ingress_preserves_the_complete_native_payload -- --test-threads=1` | 0 | **1 passed**; handler preserves the typed primary/weighted packet payload. |
| `cargo test ... every_native_particle_kind_spawns_from_a_valid_typed_packet_fixture -- --test-threads=1` | 0 | **1 passed**; all **125** kinds reach valid typed providers and actual material/model/CPU vertex consumers using 111 descriptors/14 descriptorless kinds. Its fixture now allocates the chunk sections it mutates; the former empty-section setup made water/bubble world-state edits no-ops. |
| `cargo test ... particle_asset_reload_replaces_transient_state_and_uses_new_ordered_frames -- --test-threads=1` | 0 | **1 passed**; old transient state discarded and new ordered atlas frames used. |
| `cargo test ... camel_feed_input_routes_only_valid_baby_food_to_the_particle_store -- --test-threads=1` | 0 | **1 passed**; animal interaction input and actual Store consumer. |
| `cargo test ... typed_metadata_particles_use_the_level_particle_option_codec -- --test-threads=1` | 0 | **1 passed**; typed metadata uses the same options codec, preserving the single remap path. |
| `cargo test ... living_equipment_break_events_map_to_java_slots_and_snapshot_inventory -- --test-threads=1` | 0 | **1 passed**; Java slots/events and pre-clear snapshot. |
| `cargo test ... living_item_break_particles_keep_stack_and_java_emission_shape -- --test-threads=1` | 0 | **1 passed**; typed stack/components and five-sample shape. |
| `cargo test ... equipment_break_event_is_dispatched_before_the_following_empty_slot_update -- --test-threads=1` | 0 | **1 passed**; packet ordering retained. |
| `mise run test` | 0 | **Client 1,766 passed / 0 failed / 0 ignored; protocol 52 passed / 0 failed; singleplayer 1 passed / 1 ignored; doc-tests 0.** This run follows the native replaceability change and 125-kind fixture setup correction. |

`mise run test` is the authoritative whole-workspace result. Focused filters above are supplementary source-contract checks; no test assertion was removed or weakened to obtain a pass.

## Corrections made while integrating

- **Dragon Egg:** Java's `BlockBehaviour.useItemOn` default returns `TRY_WITH_EMPTY_HAND` even when the selected stack is nonempty. `MultiPlayerGameMode` then calls `useWithoutItem` only for MAIN_HAND; secondary-use while either hand contains an item suppresses the block call. Rust now allows the nonempty main-hand default-interaction path and separately checks main hand/sneak suppression. Candle/CandleCake remain explicitly empty-hand gated by their `useItemOn` methods.
- **Arrow:** Java's flying color trail does not have a generic stopped/zero-motion guard; Rust no longer suppresses it for zero motion. Arrow event 0's 20 colored particles and AbstractArrow critical bit are also implemented, and Trident now retains the shared projectile metadata state.
- **Projectile coverage count:** Java water tick families are 3 AbstractArrow + 6 AbstractHurtingProjectile + 6 ThrowableProjectile = **15 unique kinds**. EyeOfEnder is a separate owner (**16th distinct kind**). The other “12 projectile/thrown” count is a render inventory, not this water-source denominator.
- **Explosion test setup:** an initial focused test used an empty atlas, so the real Gust provider correctly declined a missing sprite; a test-only Gust descriptor is now supplied. The weighted test also now clears a previous typed request before asserting its one-request result. Provider behavior/assertions were not bypassed.
- **PowderSnow test setup:** an initial test reused a seed whose `nextBoolean()` was false for the assertion that expected a particle; it now uses the already-selected true-emission seed. Production branch conditions were not changed to force a pass.
- During source recheck, additional real event sources were closed: LivingEntity event 46 (128 Portal), Animal/Allay event 18, TamableAnimal 6/7, Dolphin 38, Ocelot 40/41, Fox 45, and Snowball/Egg event 3. `Animal.inLove` periodic hearts are unsynchronized and not locally reachable; the server local-particle call is a no-op, while event 18 is the observable client path.
- Earlier serial workspace runs before the final source/test fixes had one Explosion test-fixture failure; it was corrected by providing the real provider descriptor and clearing unrelated pending test state. The current full workspace result is green as recorded above.

## Source audit and reporting boundaries

- `particle-java-callsite-inventory-26.2.csv`: **146** original rows, **303** extracted candidate contexts, **254** particle-generation operations, **49** non-source candidates, **303** records with structured semantic fields / **0** incomplete. Two no-client operations are Squid server AI and unsynchronized Animal.inLove; 33 `sendParticles` sources remain packet-owned. Latest per-callsite overlay marks the audited client paths as connected.
- 303 structured records mean audited Java methods/options/conditions/Rust entrypoints exist; they do **not** mean each Java operation is individually behavior-identical, tested, or visually compared. Server `sendParticles` producers remain packet-owned and are not duplicated locally.
- Registry, typed codec, provider spawn, CPU render-consumer coverage, source triggers, and GUI/GPU parity are separate evidence levels. The 125 test verifies typed request→provider→consumer output, not all natural trigger behavior or exact trajectories.
- Earlier intermediate failure/blocker notes in individual follow-ups are history, not latest status; each touched follow-up now points to this final verification record.

## Remaining limitations / not verified

- No Java/Rust in-game GUI A/B, GPU-driver validation, or original screenshot particle-name identification/reproduction was performed. The two original screenshots still lack particle identity.
- Vanilla 26.2 biome visual lightmap overrides are absent, but **custom biome-level visual lightmap override behavior remains unsupported/unverified**.
- Some custom/composite item component model-selector values remain outside the verified resolver scope. Do not infer item particle visual parity for those components.
- Java client-class provenance in the local decompile tree is not independently confirmed against the official matching client jar; the decompile README describes a server bundle. Source conclusions use the checked-in 26.2 classes/registry and packet fixtures, but this remains a provenance caveat.
- `git status` shows the shared task worktree has many uncommitted changes. No commit/reset was performed. A concurrent agent reported mistakenly running `cargo fmt -p pomme-client`; the diff is very large and mixed with functional work. No pre-format baseline is available to safely reverse formatter-only hunks, so none were reverted. Whitespace-ignored diff statistics still contain substantial content changes; this is not safely separable from other authors' edits.
