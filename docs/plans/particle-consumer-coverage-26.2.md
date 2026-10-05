# Native particle CPU-consumer coverage (Java 26.2)

`every_native_particle_kind_spawns_from_a_valid_typed_packet_fixture` in `pomme-client/src/particle.rs` isolates every native ID 0–124, decodes a valid typed option, ticks `ParticleStore`, and reaches the CPU consumer: quad extraction plus `build_particle_vertices`, or Elder Guardian's model request. The CPU vertex helper is the one called by `ParticlePipeline::update_and_draw` before host-buffer upload; this is a CPU-boundary test, not a Vulkan or visual test.

Fixture descriptor inventory is grounded in Java 26.2 `ParticleResources.java::registerProviders` and the installed 26.2 client jar: the registrations contain 125 native kinds; the jar contains 112 `assets/minecraft/particles/*.json` files, of which 111 intersect native kinds (the additional `ambient_entity_effect.json` is not a native registry kind). Only those 111 registered kinds receive synthetic ordered descriptor frames. The other 14 are asserted descriptorless and must reach their appropriate non-descriptor consumer: block/terrain material, item model/material, Elder Guardian model, or a child's real descriptor. The fixture supplies no PNGs; its synthetic atlas regions have UVs checked to remain strictly within `[0,1]`. This inventory was verified from `C:\Users\yuzum\AppData\Roaming\ModrinthApp\meta\versions\26.2-0.19.5\26.2-0.19.5.jar` and `minecraft-26.2-decompiled/src/net/minecraft/client/particle/ParticleResources.java`.

The descriptor fixture preserves Java's per-descriptor frame counts, verified against the same jar: 62×1, 1×3, 4×4, 3×5, 2×7, 28×8, 2×11, 6×12, 2×16, and 1×26. `poof`/`smoke` synthetic descriptor entries point to the same synthetic `generic_7..generic_0` regions as the Java JSON. Emitter checks distinguish the Java large and small gust providers: the large emitter creates `GUST` children using the `gust` descriptor, while the small emitter creates `SMALL_GUST` children using the `small_gust` descriptor. The directly spawnable `small_gust` provider remains independently represented by its registry kind.

The focused fixture passed on the recorded integration snapshot: `cd Client && mise exec -- cargo test -p pomme-client --locked --profile dev-fast every_native_particle_kind_spawns_from_a_valid_typed_packet_fixture -- --nocapture` → 1 passed, 0 failed, exit 0. The latest shared worktree was subsequently blocked by an incomplete untracked `pomme-client/src/world/particle_tick/blocks.rs`; see `particle-integration-verification.md` and do not treat the earlier fixture result as a current whole-workspace pass. No production fallback was added to accommodate this fixture. GUI/GPU execution and actual image parity remain untested.

## Kind-to-consumer assertion

For every ID 0–124, the test resolves the native name and typed `ServerParticleOptions`, spawns, ticks, and asserts the following. The exact name/ID source is `RegistryTable::native()` / `registries-26.2.json`; IDs therefore are not copied into a second handwritten table.

| Native kind | ID | Expected consumer assertion |
|---|---:|---|
| `angry_villager` | 0 | `extract` → CPU quad vertices (child quad allowed) |
| `block` | 1 | terrain block material (`fixture/block_particle`) → `extract` → CPU quad vertices |
| `block_marker` | 2 | terrain block material (`fixture/block_particle`) → `extract` → CPU quad vertices |
| `bubble` | 3 | `extract` → CPU quad vertices (child quad allowed) |
| `sulfur_bubbles` | 4 | `extract` → CPU quad vertices (child quad allowed) |
| `noxious_gas` | 5 | `extract` → CPU quad vertices (child quad allowed) |
| `noxious_gas_cloud` | 6 | descriptorless emitter → `noxious_gas` child descriptor → `extract` → CPU quad vertices |
| `geyser` | 7 | descriptorless emitter → `geyser_plume` child descriptor → `extract` → CPU quad vertices |
| `geyser_base` | 8 | `extract` → CPU quad vertices (child quad allowed) |
| `geyser_poof` | 9 | `extract` → CPU quad vertices (child quad allowed) |
| `geyser_plume` | 10 | `extract` → CPU quad vertices (child quad allowed) |
| `cloud` | 11 | `extract` → CPU quad vertices (child quad allowed) |
| `copper_fire_flame` | 12 | `extract` → CPU quad vertices (child quad allowed) |
| `crit` | 13 | `extract` → CPU quad vertices (child quad allowed) |
| `damage_indicator` | 14 | `extract` → CPU quad vertices (child quad allowed) |
| `dragon_breath` | 15 | `extract` → CPU quad vertices (child quad allowed) |
| `dripping_lava` | 16 | `extract` → CPU quad vertices (child quad allowed) |
| `falling_lava` | 17 | `extract` → CPU quad vertices (child quad allowed) |
| `landing_lava` | 18 | `extract` → CPU quad vertices (child quad allowed) |
| `dripping_water` | 19 | `extract` → CPU quad vertices (child quad allowed) |
| `falling_water` | 20 | `extract` → CPU quad vertices (child quad allowed) |
| `dust` | 21 | `extract` → CPU quad vertices (child quad allowed) |
| `dust_color_transition` | 22 | `extract` → CPU quad vertices (child quad allowed) |
| `effect` | 23 | `extract` → CPU quad vertices (child quad allowed) |
| `elder_guardian` | 24 | descriptorless particle → `model_render_requests` → `ElderGuardianModel` |
| `enchanted_hit` | 25 | `extract` → CPU quad vertices (child quad allowed) |
| `enchant` | 26 | `extract` → CPU quad vertices (child quad allowed) |
| `end_rod` | 27 | `extract` → CPU quad vertices (child quad allowed) |
| `entity_effect` | 28 | `extract` → CPU quad vertices (child quad allowed) |
| `explosion_emitter` | 29 | descriptorless emitter → `explosion` child descriptor → `extract` → CPU quad vertices |
| `explosion` | 30 | `extract` → CPU quad vertices (child quad allowed) |
| `gust` | 31 | `extract` → CPU quad vertices (child quad allowed) |
| `small_gust` | 32 | `extract` → CPU quad vertices (child quad allowed) |
| `gust_emitter_large` | 33 | descriptorless emitter → `gust` child descriptor → `extract` → CPU quad vertices |
| `gust_emitter_small` | 34 | descriptorless emitter → Java `GUST` child (`gust` descriptor) → `extract` → CPU quad vertices |
| `sonic_boom` | 35 | `extract` → CPU quad vertices (child quad allowed) |
| `falling_dust` | 36 | `extract` → CPU quad vertices (child quad allowed) |
| `firework` | 37 | `extract` → CPU quad vertices (child quad allowed) |
| `fishing` | 38 | `extract` → CPU quad vertices (child quad allowed) |
| `flame` | 39 | `extract` → CPU quad vertices (child quad allowed) |
| `infested` | 40 | `extract` → CPU quad vertices (child quad allowed) |
| `cherry_leaves` | 41 | `extract` → CPU quad vertices (child quad allowed) |
| `pale_oak_leaves` | 42 | `extract` → CPU quad vertices (child quad allowed) |
| `tinted_leaves` | 43 | `extract` → CPU quad vertices (child quad allowed) |
| `sculk_soul` | 44 | `extract` → CPU quad vertices (child quad allowed) |
| `sculk_charge` | 45 | `extract` → CPU quad vertices (child quad allowed) |
| `sculk_charge_pop` | 46 | `extract` → CPU quad vertices (child quad allowed) |
| `soul_fire_flame` | 47 | `extract` → CPU quad vertices (child quad allowed) |
| `soul` | 48 | `extract` → CPU quad vertices (child quad allowed) |
| `flash` | 49 | `extract` → CPU quad vertices (child quad allowed) |
| `happy_villager` | 50 | `extract` → CPU quad vertices (child quad allowed) |
| `composter` | 51 | `extract` → CPU quad vertices (child quad allowed) |
| `heart` | 52 | `extract` → CPU quad vertices (child quad allowed) |
| `instant_effect` | 53 | `extract` → CPU quad vertices (child quad allowed) |
| `item` | 54 | item model material (`fixture/item_particle`) → `extract` → CPU quad vertices |
| `vibration` | 55 | `extract` → CPU quad vertices (child quad allowed) |
| `trail` | 56 | `extract` → CPU quad vertices (child quad allowed) |
| `pause_mob_growth` | 57 | `extract` → CPU quad vertices (child quad allowed) |
| `reset_mob_growth` | 58 | `extract` → CPU quad vertices (child quad allowed) |
| `item_slime` | 59 | item model material (`fixture/item_particle`) → `extract` → CPU quad vertices |
| `item_cobweb` | 60 | item model material (`fixture/item_particle`) → `extract` → CPU quad vertices |
| `item_snowball` | 61 | item model material (`fixture/item_particle`) → `extract` → CPU quad vertices |
| `large_smoke` | 62 | `extract` → CPU quad vertices (child quad allowed) |
| `lava` | 63 | `extract` → CPU quad vertices (child quad allowed) |
| `mycelium` | 64 | `extract` → CPU quad vertices (child quad allowed) |
| `note` | 65 | `extract` → CPU quad vertices (child quad allowed) |
| `poof` | 66 | `extract` → CPU quad vertices (child quad allowed) |
| `portal` | 67 | `extract` → CPU quad vertices (child quad allowed) |
| `rain` | 68 | `extract` → CPU quad vertices (child quad allowed) |
| `smoke` | 69 | `extract` → CPU quad vertices (child quad allowed) |
| `white_smoke` | 70 | `extract` → CPU quad vertices (child quad allowed) |
| `sneeze` | 71 | `extract` → CPU quad vertices (child quad allowed) |
| `spit` | 72 | `extract` → CPU quad vertices (child quad allowed) |
| `squid_ink` | 73 | `extract` → CPU quad vertices (child quad allowed) |
| `sweep_attack` | 74 | `extract` → CPU quad vertices (child quad allowed) |
| `totem_of_undying` | 75 | `extract` → CPU quad vertices (child quad allowed) |
| `underwater` | 76 | `extract` → CPU quad vertices (child quad allowed) |
| `splash` | 77 | `extract` → CPU quad vertices (child quad allowed) |
| `witch` | 78 | `extract` → CPU quad vertices (child quad allowed) |
| `bubble_pop` | 79 | `extract` → CPU quad vertices (child quad allowed) |
| `current_down` | 80 | `extract` → CPU quad vertices (child quad allowed) |
| `bubble_column_up` | 81 | `extract` → CPU quad vertices (child quad allowed) |
| `nautilus` | 82 | `extract` → CPU quad vertices (child quad allowed) |
| `dolphin` | 83 | `extract` → CPU quad vertices (child quad allowed) |
| `campfire_cosy_smoke` | 84 | `extract` → CPU quad vertices (child quad allowed) |
| `campfire_signal_smoke` | 85 | `extract` → CPU quad vertices (child quad allowed) |
| `dripping_honey` | 86 | `extract` → CPU quad vertices (child quad allowed) |
| `falling_honey` | 87 | `extract` → CPU quad vertices (child quad allowed) |
| `landing_honey` | 88 | `extract` → CPU quad vertices (child quad allowed) |
| `falling_nectar` | 89 | `extract` → CPU quad vertices (child quad allowed) |
| `falling_spore_blossom` | 90 | `extract` → CPU quad vertices (child quad allowed) |
| `ash` | 91 | `extract` → CPU quad vertices (child quad allowed) |
| `crimson_spore` | 92 | `extract` → CPU quad vertices (child quad allowed) |
| `warped_spore` | 93 | `extract` → CPU quad vertices (child quad allowed) |
| `spore_blossom_air` | 94 | `extract` → CPU quad vertices (child quad allowed) |
| `dripping_obsidian_tear` | 95 | `extract` → CPU quad vertices (child quad allowed) |
| `falling_obsidian_tear` | 96 | `extract` → CPU quad vertices (child quad allowed) |
| `landing_obsidian_tear` | 97 | `extract` → CPU quad vertices (child quad allowed) |
| `reverse_portal` | 98 | `extract` → CPU quad vertices (child quad allowed) |
| `white_ash` | 99 | `extract` → CPU quad vertices (child quad allowed) |
| `small_flame` | 100 | `extract` → CPU quad vertices (child quad allowed) |
| `snowflake` | 101 | `extract` → CPU quad vertices (child quad allowed) |
| `dripping_dripstone_lava` | 102 | `extract` → CPU quad vertices (child quad allowed) |
| `falling_dripstone_lava` | 103 | `extract` → CPU quad vertices (child quad allowed) |
| `dripping_dripstone_water` | 104 | `extract` → CPU quad vertices (child quad allowed) |
| `falling_dripstone_water` | 105 | `extract` → CPU quad vertices (child quad allowed) |
| `glow_squid_ink` | 106 | `extract` → CPU quad vertices (child quad allowed) |
| `glow` | 107 | `extract` → CPU quad vertices (child quad allowed) |
| `wax_on` | 108 | `extract` → CPU quad vertices (child quad allowed) |
| `wax_off` | 109 | `extract` → CPU quad vertices (child quad allowed) |
| `electric_spark` | 110 | `extract` → CPU quad vertices (child quad allowed) |
| `scrape` | 111 | `extract` → CPU quad vertices (child quad allowed) |
| `shriek` | 112 | `extract` → CPU quad vertices (child quad allowed) |
| `egg_crack` | 113 | `extract` → CPU quad vertices (child quad allowed) |
| `dust_plume` | 114 | `extract` → CPU quad vertices (child quad allowed) |
| `trial_spawner_detection` | 115 | `extract` → CPU quad vertices (child quad allowed) |
| `trial_spawner_detection_ominous` | 116 | `extract` → CPU quad vertices (child quad allowed) |
| `vault_connection` | 117 | `extract` → CPU quad vertices (child quad allowed) |
| `dust_pillar` | 118 | terrain block material (`fixture/block_particle`) → `extract` → CPU quad vertices |
| `ominous_spawning` | 119 | `extract` → CPU quad vertices (child quad allowed) |
| `raid_omen` | 120 | `extract` → CPU quad vertices (child quad allowed) |
| `trial_omen` | 121 | `extract` → CPU quad vertices (child quad allowed) |
| `block_crumble` | 122 | terrain block material (`fixture/block_particle`) → `extract` → CPU quad vertices |
| `firefly` | 123 | `extract` → CPU quad vertices (child quad allowed) |
| `sulfur_cube_goo` | 124 | `extract` → CPU quad vertices (child quad allowed) |

The quad route means `ParticleStore::extract` followed by `renderer::pipelines::particle::build_particle_vertices`; per-kind fixture assertion checks six CPU vertices per extracted quad and the synthetic descriptor UV set if selected. Elder Guardian goes through `ParticleStore::model_render_requests` and the `ElderGuardianModel` request instead of the quad path. Particles which create children are checked after the same live tick, so extracted quads can be child output; ordinary no-child providers are expected to produce their own quad.

## Related existing checks and limits

- `particle.rs::particle_asset_reload_replaces_transient_state_and_uses_new_ordered_frames`: transient state reset and ordered sprite frame rebinding after asset reload.
- `particle.rs::firework_shapes_create_java_spark_counts_and_velocity_families`, `firework_starter_sequences_multiple_explosions_and_survives_entity_removal`, and `firework_spark_trail_inherits_fade_twinkle_and_color`: firework children, sound requests, temporal behavior.
- `particle/water.rs::landing_sounds_match_java_events_and_fire_once_only_after_collision` and `particle_mode_and_distance_culling_cannot_enqueue_landing_sounds`: landing sound requests and false-positive suppression.
- Runtime consumer wiring: `app/phases/in_game.rs` drains `ParticleStore::drain_sound_requests` into `AudioEngine::play_world_sound`, obtains `model_render_requests`, and passes quad/model requests to the renderer. These are source callsite checks, not end-to-end audio/GPU tests.
- Existing provider tests cover child lifetimes/removal; no Vulkan device is constructed here. A GUI/A-B comparison and original screenshot particle identification remain separate, unverified work.
