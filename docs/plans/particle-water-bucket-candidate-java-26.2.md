# Water bucket candidate selection — Java 26.2

## Verified Java contract

Reference: `minecraft-26.2-decompiled/src/net/minecraft/world/item/BucketItem.java`.

- `BucketItem.use` selects `hit.pos` iff the clicked **block type** implements `LiquidBlockContainer` and the bucket content is `WATER`; otherwise it selects `hit.pos.relative(face)`. No sneak condition participates in this initial choice.
- `emptyContents` at that candidate evaluates `mayReplace = state.canBeReplaced(WATER)`, then `placeLiquid = mayReplace || (block instanceof LiquidBlockContainer && canPlaceLiquid(...))`, and accepts `air || (placeLiquid && (!sneaking || hitResult == null))`.
- On rejection with a hit result, it recursively retries the original hit's adjacent position with `hitResult == null`.
- `WATER_EVAPORATES` is queried only after this succeeds, at the accepted candidate; then Java emits 8 `LARGE_SMOKE` particles and returns before `placeLiquid`.

`canPlaceLiquid` is not equivalent to “the waterlogged property is false.” `SimpleWaterloggedBlock` returns true for WATER even when wet; `SlabBlock` rejects double slabs; `BarrierBlock` additionally requires a creative Player; Seagrass/TallSeagrass/Kelp/KelpPlant override it to false. Java `SignBlock` implements `SimpleWaterloggedBlock` and does not override this method.

## Rust implementation

- `tools/stategen/StateDump.java` now dumps both exact runtime class membership (`block instanceof LiquidBlockContainer`) and the dynamic-dispatch result of `canPlaceLiquid(null, EmptyBlockGetter, ZERO, state, WATER)`.
- `tools/blockgen/src/main.rs` stores those native fields as `k` (container type) and `n` (null-user `canPlaceLiquid` result); existing `w` remains the final Java `state.canBeReplaced(WATER)` result.
- `world::block::liquid_block_container_type` exposes exact `k` identity; `can_place_water` consumes Java-dumped acceptance and applies the documented creative-Player exception for BarrierBlock.
- On native 26.2, `player::interaction::water_bucket_destination` chooses the initial candidate from `k`, then models `emptyContents` and recursive fallback. Older protocol tables have no `k/n` yet; the selector retains its pre-existing state-based candidate fallback for those tables instead of applying a fabricated 26.2 type table. Java parity for those older versions remains unclaimed.
- The caller resolves evaporation at this returned candidate and emits the existing 8-particle `LARGE_SMOKE` burst.
- Sneaking applies to the initial `emptyContents` call, not `BucketItem.use` selection; adjacent retry has no hit result, matching Java. Existing Cauldron and RedstoneOre block-use consumption branches were left intact.

## Important correction to the requested Sign expectation

The stated “wet Sign with `w=false` must fall back” does **not** match Java 26.2. The official-source `SignBlock` inherits `SimpleWaterloggedBlock.canPlaceLiquid`, which returns true for WATER without checking `WATERLOGGED`; `emptyContents` reaches the evaporation branch before calling `placeLiquid`. Consequently wet Sign (`w=false`, `n=true`) is still accepted at the clicked position and emits there when evaporation is enabled. Tests encode this Java behavior rather than the contradictory expected result.

A genuine typed-candidate rejection/fallback is covered with a double slab (`k=true`, `w=false`, and Java `canPlaceLiquid` false for `type=double`). `SeagrassBlock`, `TallSeagrassBlock`, `KelpBlock`, and `KelpPlantBlock` also verify interface membership without a `waterlogged` property; their `canPlaceLiquid` override is false, while their current water state can still make `w=true`.

## Generated metadata preservation

The temporary audit directory is retained at `Client/tmp/water-bucket-metadata-before/` until review, including the actual pre-edit state JSON and the Java raw dumps from before/after the dump extension. `verify_metadata.py` compares all prior raw fields and every prior native key/value, checks all 1,196 block rows and the 32,366-state product, and records hashes in `comparison.json`.

- Native blocks/states: **1,196 / 32,366**.
- Java `LiquidBlockContainer` membership: **433 blocks / 23,486 states**.
- Previous raw fields: unchanged; only `liquid_block_container` and `can_place_water` were added.
- Previous native per-state metadata is semantically unchanged, including every `w`; only `k` and `n` were added. This is a value-level comparison, not a claim that the JSON lines or file bytes remained identical.
- `blocks-26.2.json` before/after SHA-256 values differ (`9cf6c454512d5ebe4a2004a666277160c2fda56d54de663575ac1696b137c7aa` / `3e24da164097aa0b02f74b6f77fce44cc6eefe35a3533f5b9f32042dcd2d7b68`), so the files were not byte-identical. The verification compared generated and checked-in semantic values and confirmed the prior per-state metadata values (including `w`) were preserved; it did not establish byte identity.
- Before native JSON SHA-256: `49bd4a0d2dd5e4332b6f2fbb7698ae92ffeb67652efc45a81c76eb166bb601c5`.
- `blockgen` semantic output SHA-256: `7a9c528b0d57dc4bd27ce3b6b7e254d8dfa77fe33be845edd8a8ba7e66038112`.
- Checked-in final native JSON SHA-256: `38a70c2ca3b5288083dc64b53d2f25f03cce16b854a59aabf046329e07216d6c`. The checked-in file was patched from its before-snapshot without reformatting existing lines; the comparison script proves semantic equality with blockgen output and exact preservation of all prior fields.
- Java raw dump before SHA-256: `d38a9fabdcfb6e0245a0f2283d11c469ae1a11977f6c013756838e49ff1e9804`.
- Java raw dump after SHA-256: `8a77176be8d2ede274d79a4678bb7736ce540fa2b8f18bcd35418dec25b6831b`.

Reproduction was run from `Client/`: compile/run `tools/stategen/StateDump.java` against `../minecraft-debug-server/versions/26.2/server-26.2.jar` plus its `libraries/*.jar`; `cmp` against `state-26.2.raw.after.json` exited 0; run `mise exec -- cargo run -p blockgen --locked -- state <raw.json> pomme-client/src/world/block/data/blocks-26.2.json <out.json>`; and `python tmp/water-bucket-metadata-before/verify_metadata.py` exited 0, checking exact old fields and semantic equality of generated and checked-in JSON without broad reformatting. Java emitted 32,366 states; blockgen emitted 1,196 blocks / 32,366 states, 6,762 shaped states / 37 masks.

## Verification (historical snapshot before Brewing Stand fallthrough fix)

These commands were run serially from `Client/` before the later Brewing Stand regression test and input-boundary fix:

| Command / fixture | Result |
|---|---|
| `mise run check` | exit 0. |
| `mise exec -- cargo test -p blockgen --locked -- --test-threads=1` | exit 0; 6 passed. |
| `mise exec -- cargo test -p pomme-client --locked --profile dev-fast water_bucket -- --test-threads=1` | exit 0; 4 passed, including real `start_use_item` output. |
| `mise exec -- cargo test -p pomme-client --locked --profile dev-fast world::block::tests::water_replacement_uses_vanillas_final_per_state_query -- --exact --test-threads=1` | exit 0; 1 passed. |
| `mise exec -- cargo test -p pomme-client --locked --profile dev-fast player::interaction::tests::redstone_ore_use_consumes_bucket_before_bucket_use_and_enqueues_visible_dust -- --exact --test-threads=1` | exit 0; 1 passed. |
| 125 typed-particle fixture / atlas reload / item-world-context fixture | Each exit 0; 1 passed. |
| Historical workspace snapshot (before the Brewing Stand fix) | One `mise run test` had the then-investigated `bubble` consumer fixture failure (1,767 passed / 1 failed); its immediate full retry exited 0: client **1,768 passed**, protocol **52 passed**, singleplayer **1 passed / 1 ignored**, doc tests 0. This is historical evidence, not a test result for the current source snapshot. No assertion was weakened or provider behavior altered. |
| `mise exec -- rustfmt --edition 2024 --check ...` and `git diff --check` | exit 0. |
| Java raw regeneration + blockgen semantic verification | exit 0; `cmp` raw dumps equal; verification script proves generated native values and existing metadata equal. |

`player::interaction::tests::water_bucket_empty_contents_uses_exact_java_replaceability_and_candidate_position` checks the actual `InteractionState::start_use_item` path, particle count and real coordinates, dry/wet clicked Ladder, wet Sign, double-slab fallback, propertyless Java containers, sneak, and distinct clicked/adjacent evaporation values. `water_bucket_unloaded_fallback_does_not_emit_evaporation_particles` checks an unloaded adjacent candidate through the same input path. The Cauldron and RedstoneOre consumption branches remain tested/preserved.

No source-audit candidate totals changed (254/49 unchanged). No GUI/GPU comparison was performed.

### Current snapshot after the Brewing Stand fallthrough fix

These are focused results for the current edited source snapshot, not a full-workspace gate:

| Command | Result |
|---|---|
| `mise run check` | exit 0. |
| `mise exec -- cargo test -p pomme-client --locked --profile dev-fast water_bucket -- --test-threads=1` | exit 0; 5 passed, 0 failed. Includes the `start_use_item` → `ParticleStore` regression: Brewing Stand non-sneak/attribute on = 0; sneak bypass = 8; ordinary stone/attribute on = 8; attribute off = 0. |
| `mise exec -- cargo test -p pomme-client --locked --profile dev-fast player::interaction::tests::hopper_block_use_is_consumed_by_menu -- --exact --test-threads=1` | exit 0; 1 passed. |
| `mise exec -- cargo test -p pomme-client --locked --profile dev-fast player::interaction::tests::redstone_ore_use_consumes_bucket_before_bucket_use_and_enqueues_visible_dust -- --exact --test-threads=1` | exit 0; 1 passed. |

The final full-workspace gate remains separate and is deferred until the Bubble diagnostic conclusion, as requested.
