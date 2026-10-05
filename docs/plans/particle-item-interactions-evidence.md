# Brush / Water Bucket / ItemEntity particle interaction evidence

Scope: close the remaining interaction-side parity gaps without expanding pickup rendering beyond `ItemEntity`.

## Implemented

- **Water Bucket evaporation:** `Core::tick` no longer infers evaporation from a dimension-name suffix. For a held water bucket, it resolves `WATER_EVAPORATES` at the candidate placement block using `net::environment::dripstone_attributes_at`, the biome positional layer, and current timeline clocks. A waterlogged property set to `false` (and the Java liquid-container plant types `seagrass`, `tall_seagrass`, `kelp`, `kelp_plant`) selects the clicked block; otherwise the hit-face-adjacent block is selected. Eligible local evaporation emits the existing eight `LARGE_SMOKE` particles with zero velocity and independent uniform `[0,1)` offsets in all three coordinates. The server packet path is unchanged.
- **Brush:** `BrushItem` dust direction still follows the hit face and view vector. The flip now derives from the used hand plus the existing `last_main_hand_right` client-information setting: main/off hand and left/right main-arm combinations map to the corresponding physical brushing arm. There is no particle-only preference.
- **Pickup:** `ItemEntityStore::active_pickups` uses a shared target calculation for remote living entities: interpolated entity position plus half the entity eye height. Player eye height follows sleeping/crouching/swimming/fall-flying/spin-attack pose constants; other living kinds use registry `EntityDimensions::eye_height`. Existing three-tick lifetime, squared interpolation, tick decrement timing, and local-player fallback are unchanged.

Java references (26.2 decompilation):
- `minecraft-26.2-decompiled/src/net/minecraft/world/item/BrushItem.java`: `onUseTick`, `spawnDustParticles`, `DustParticlesDelta.fromDirection`; brush arm is `used hand == MAIN_HAND ? main arm : opposite`, and particles use `random.nextInt(7,12)`, face-dependent direction, arm flip, and `* 3` velocity.
- `minecraft-26.2-decompiled/src/net/minecraft/world/item/BucketItem.java`: `use` chooses clicked position for water `LiquidBlockContainer`, otherwise hit-face adjacent; `emptyContents` queries positional `WATER_EVAPORATES` and adds 8 `LARGE_SMOKE` samples in the selected block.
- Java `EntityDimensions.eyeHeight` plus player pose constants inform the pickup target calculation. No non-ItemEntity pickup model was added.

## Tests added / extended

- `player::interaction::tests::item_interaction_particles_use_native_options_and_brush_cadence`: both main-arm settings × both interaction hands, and all six hit faces (up/down use view direction).
- `player::interaction::tests::water_bucket_destination_uses_open_liquid_container_before_adjacent_block`: waterlogged-capable block vs adjacent placement selection.
- Existing `water_bucket_evaporation_is_a_client_local_eight_large_smoke_burst` remains and checks false attribute, non-water bucket, and exactly eight particles.
- `net::environment::tests::water_evaporation_uses_attribute_over_dimension_name_and_tracks_timeline_changes`: explicit false positional override over true base, non-Nether-default timeline on/off, and period wrap.
- `entity::tests::item_pickup_target_uses_remote_entity_pose_eye_height`: remote crouching and swimming eye height.
- Existing `entity::tests::item_take_keeps_pre_shrink_model_follows_collector_and_expires_after_three_ticks` remains for snapshot model, moving target, three-tick expiry, and stack shrink.

## Validation status

The test code is present, but focused tests could not execute because the concurrent broader particle integration currently fails compilation before the test binary is built. See the agent handoff/report for exact commands and unrelated compile errors. `mise run check` likewise reaches `pomme-client` and fails on unrelated concurrent `entity/particle_misc.rs`, `entity/particle_mobs.rs`, `renderer/lightmap.rs`, and `world/block/registry.rs` errors; no errors were reported in the interaction, entity pickup, or core changes in that check output.

The original Brush/Water Bucket task left non-`ItemEntity` pickup animation out of scope at that time. The later Java-callsite-backed extension is tracked separately in [`particle-item-pickup-entity-followup.md`](particle-item-pickup-entity-followup.md); this does not add entries to the 125-kind particle registry.
