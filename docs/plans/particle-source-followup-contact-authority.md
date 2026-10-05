# Contact-particle block-effect authority follow-up

## Java 26.2 basis

The contact sources share the actual `Entity.applyEffectsFromBlocks` caller contract rather than approximating authority separately for each particle:

- `world/entity/LivingEntity.java:2923-2924`: client-side `LivingEntity.tick` calls `applyEffectsFromBlocks()` only when `isLocalInstanceAuthoritative()`.
- `world/entity/Entity.java:960-970,3412-3415`: the same block-effects method calls `stepOn` after the `isAffectedByBlocks()` gate; entity local authority delegates to its controlling passenger's local authority. `isAffectedByBlocks()` rejects removed and no-physics entities.
- `world/entity/player/Player.java:1162-1164`: the local client Player is locally authoritative.
- Controller policies were checked at `Entity.java:3330-3332`, `Mob.java:257-264`, `vehicle/boat/AbstractBoat.java:684-688`, `animal/equine/AbstractHorse.java:923-929`, `animal/pig/Pig.java:108-115`, `monster/Strider.java:209-216`, `animal/nautilus/AbstractNautilus.java:170-176`, and `animal/happyghast/HappyGhast.java:336-343`.
  - Base Entity has no controller. Mob accepts a first passenger that is a Mob (not a Player), so that override does not make player-ridden mobs locally authoritative.
  - Boat returns a first LivingEntity, but AbstractBoat calls `applyEffectsFromBlocks()` unconditionally after its locally-authoritative movement branch; this caller itself is not guarded by the authority test.
  - AbstractHorse family, Camel, and Nautilus require a saddled mount and first passenger Player.
  - Pig requires saddle + first passenger Player holding a Carrot on a Stick; Strider requires saddle + first passenger Player holding a Warped Fungus on a Stick.
  - Happy Ghast requires Body armor, first passenger Player, and not staying still. Its synchronized `STAYS_STILL` field is metadata index 19 in 26.2.
- `world/level/block/RedStoneOreBlock.java`: `stepOn` is reached from that same block-effects traversal, then skips careful stepping. PowderSnow's `entityInside` has its own LivingEntity-vs-nonliving state predicate, but only runs when the caller reaches it.
- `world/entity/projectile/arrow/AbstractArrow.java:77-81,707-716`: AbstractArrow's independent `ID_FLAGS` byte uses bit `0x01` for critical and bit `0x02` for noPhysics; this is not `Entity.DATA_SHARED_FLAGS_ID`.

## Rust boundary and behavior

- `entity/particle_tick.rs::local_authoritative_mounts` derives control from the exact `SetPassengers` first-seat order, the current `LocalPlayerProjectileView.entity_id`, synced saddle/body equipment, Happy Ghast `STAYS_STILL`, and the current local player's main/offhand steering-item booleans. It accepts the local player view even when the local player is not duplicated in `EntityStore::living`; it rejects remote first passengers and local second passengers.
- `entity/particle_tick.rs::tick_common` passes the resulting block-effect eligibility only to the RedstoneOre step-on branch and PowderSnow living callback. It does not return early from `tick_living`; sprint, water-entry and other common visual behavior remains unchanged. Remote living players and mobs do not emit these two client block-effect contacts, while a locally controlled living mount can.
- `entity/particle_tick.rs::tick_vehicle` applies the nonliving Java caller and `isAffectedByBlocks` contract only to block-contact Ore; other vehicle particle paths remain independent. The nonliving PowderSnow owner path uses the same `invokes_client_block_effects` and `entity_affected_by_blocks` decision. Boats and minecarts remain eligible because their Java contact callers are not inside the local-authority-only movement branch.
- Arrow, SpectralArrow, and Trident `set_projectile_metadata` handling retains critical from bit `0x01`, stores noPhysics from bit `0x02` on `ProjectileDisplay`, and gates both PowderSnow and Ore contact through `entity_affected_by_blocks`. Shared flags bit `0x04` is ignored for noPhysics. Java `ShulkerBullet.isAffectedByBlocks()` overrides the default `!removed && !noPhysics` rule with `!removed`, so it remains contact-eligible despite its constructor setting `noPhysics=true`; owner removal handles the removed case. Java's other same-named override in `Vex` does not apply because Vex is not in the current client `applyEffectsFromBlocks` contact-source set. Existing in-ground, tipped-color, pickup snapshots, and unrelated sprint/water sources are not changed by this gate.
- `app/phases/in_game.rs` routes the local player's post-physics contact through `entity::local_block_effect_particle_requests`, using the same outer eligibility for PowderSnow and Ore. A local removed player is skipped before that call, and spectator/noPhysics state is passed as ineligibility. Vehicle spawn/teleport movement history, entity removal, and ID reuse continue to use existing `EntityStore` lifecycle methods.

## Validation

All commands were run serially from `Client/` with the repository-pinned mise toolchain:

- `mise run check` — exit **0**. It completed after the production changes; only existing repository warning classes were printed.
- The prior 18-test contact run had an incorrect ShulkerBullet suppression assertion; that test is now `ore_contact_uses_java_is_affected_by_blocks_override_and_arrow_flags` and expects its Java override. Added `moving_shulker_bullet_gets_powder_snow_contact_despite_no_physics` for actual owner movement, no-movement suppression, Snowflake output, and removal lifecycle.
- Corrected serial verification from `Client/`: `mise run check` — exit **0**; `mise exec -- cargo test -p pomme-client --locked --profile dev-fast entity::particle_tick::tests -- --test-threads=1` — exit **0**, **19 passed**; `every_native_particle_kind_spawns_from_a_valid_typed_packet_fixture` — exit **0**, **1 passed**; `particle_asset_reload_replaces_transient_state_and_uses_new_ordered_frames` — exit **0**, **1 passed**; `item_world_context_matches_packet_and_local_requests_across_switch_and_reload` — exit **0**, **1 passed**; final `mise run test` — exit **0**, client **1,767 passed / 0 failed / 0 ignored**, protocol **52 passed / 0 failed**, singleplayer **1 passed / 1 ignored**, doc-tests **0 failures**.
- No broad formatting was applied. The full build/test set compiles the changed Rust file; no GUI/GPU visual comparison was performed.
- No GUI/GPU visual comparison was performed.

The independent gate also reported a separate water-bucket `forceSolidOn/forceSolidOff` candidate-state difference. That does not concern contact authority or PowderSnow/Ore and was deliberately left unchanged in this scoped fix.
