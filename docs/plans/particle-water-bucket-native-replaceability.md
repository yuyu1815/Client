# Water-bucket native replaceability — Java 26.2

## Decision and Java contract

`BucketItem` asks a block state's fluid-specific replacement query; it does not infer it from a rendering/collision shape:

- `BlockBehaviour.java:261–262`: default `canBeReplaced(BlockState, Fluid)` is `state.canBeReplaced() || !state.isSolid()`.
- `BlockBehaviour.java:815–821`: `calculateSolid()` applies `forceSolidOn` and `forceSolidOff` before considering cached collision bounds.
- `BlockBehaviour.java:1147–1149`: `BlockState.canBeReplaced(Fluid)` delegates to its owning `Block`, so native block overrides are included.
- `BucketItem.java:76–77`: initial target is clicked position only for a `LiquidBlockContainer` with water; otherwise it is the hit-face offset.
- `BucketItem.java:120–135`: `emptyContents` checks `canBeReplaced(content)`, then `LiquidBlockContainer.canPlaceLiquid`, then air/sneak rules, and on failure recurses to the original hit's adjacent position with a null hit result.
- `BucketItem.java:137`: `WATER_EVAPORATES` is resolved at the accepted `emptyContents` candidate, before emitting eight `LARGE_SMOKE` particles in that world/environment.

The inherited `BlockState.canBeReplaced(Fluid)` query is world/position-independent: its only argument is `Fluid`; it evaluates the already-registered state and its owning block. `StateDump.java` now invokes the **final public query** `state.canBeReplaced(Fluids.WATER)` for each native state, rather than copying raw force flags and reimplementing Java. Block-specific overrides are therefore represented too.

## Data flow and Rust behavior

`tools/stategen/StateDump.java` → raw `can_be_replaced_by_water` 0/1 state array → `tools/blockgen/src/main.rs` compact per-block JSON key **`w`** → `StateEntry::w` / `BlockData::can_be_replaced_by_water` → `world::block::can_be_replaced_by_water` → `player::interaction::water_bucket_destination` → local bucket input output.

Native 26.2 uses the exact generated `w` value. Older version tables which predate `w` retain their prior compatibility fallback. The later candidate-selection fix adds separate exact Java `k` (LiquidBlockContainer type) and `n` (null-user `canPlaceLiquid(WATER)` result) metadata; `w`, `k`, and `n` are not interchangeable.

## Reproduced states and container masking

The former `replaceable || !legacy_solid(shape)` estimate demonstrably disagreed in both directions:

| Native state | Java `canBeReplaced(WATER)` | Former shape estimate | Actual route significance |
|---|---:|---:|---|
| `moving_piston` (`forceSolidOn`, dynamic shape) | false | true | Not a liquid container; old estimate could select it as an evaporation destination. It now yields no destination/no smoke. |
| `big_dripleaf` (`forceSolidOff`) | true | false | Not a liquid container; old estimate could reject it. It now reaches this position and emits the eight-particle Nether burst when evaporation is enabled. |
| `cobweb` (`forceSolidOn`, no collision) | false | true | Not a liquid container; actual full input path rejects it as the adjacent destination. |
| `oak_sign` (`forceSolidOn`) | false | true | Dry and wet SignBlock implement SimpleWaterloggedBlock; its Java `canPlaceLiquid` accepts WATER even when wet, masking `w=false` before the evaporation branch. |
| `ladder` (`forceSolidOff`) | true | true | Dry and wet Ladder states select clicked by block type. The exact `w` query is independently true for both. |
| `oak_stairs`, waterlogged false/true | false / false | — | `SimpleWaterloggedBlock.canPlaceLiquid(WATER)` accepts both states; both can be the clicked evaporation candidate. A double slab is the `SlabBlock` rejection case. |
| `end_portal`, `end_gateway` | false | — | Native block overrides are carried by the final Java result, not special Rust name checks. |

The final bucket-candidate review preserves type selection and state acceptance as separate Java decisions. Java `BucketItem.use` selects clicked by `LiquidBlockContainer` type; `emptyContents` then considers `w`, `canPlaceLiquid`, air, sneak/hit-result state, and adjacent recursion. The wet-Sign fallback expectation was checked against source and is not correct: SignBlock inherits SimpleWaterloggedBlock.canPlaceLiquid, which accepts WATER independent of WATERLOGGED. Double slabs cover the true typed-candidate rejection/fallback case. See `particle-water-bucket-candidate-java-26.2.md` for raw dumps, hashes, and actual-input tests.

## Generated metadata preservation / oracle comparison

Reference runner: existing Java 25.0.2 setup plus `minecraft-debug-server/versions/26.2/server-26.2.jar` and its bundled libraries. Only temporary output under repository-root `tmp/` was used; no installed profile or world was modified.

- Java oracle run: **32,366 states**, **6,762 face-occlusion-shaped states**.
- `blockgen state` output: **1,196 blocks / 32,366 states**, **6,762 shaped / 37 distinct masks**.
- The later candidate-selection change has a dedicated independent before/after audit in `particle-water-bucket-candidate-java-26.2.md`. It preserves every existing state field/value, especially `w`, while adding exact `k` and `n`; `blocks-26.2.json` has byte-identical before/after SHA-256.
- Expanded `w` values cover all **32,366 states**: **4,546 true / 27,820 false**; only `resin_brick_wall` requires a per-state array.
- Pre-candidate-fix state JSON SHA-256: `49bd4a0d2dd5e4332b6f2fbb7698ae92ffeb67652efc45a81c76eb166bb601c5`.
- Candidate-fix blockgen output SHA-256: `7a9c528b0d57dc4bd27ce3b6b7e254d8dfa77fe33be845edd8a8ba7e66038112`; checked-in byte-preserving metadata addition SHA-256: `38a70c2ca3b5288083dc64b53d2f25f03cce16b854a59aabf046329e07216d6c`.

## Verification on final shared snapshot

Commands were run serially from `Client/`:

| Command | Result |
|---|---|
| `mise run check` | exit 0; only existing warnings. |
| `mise exec -- cargo test -p blockgen --locked -- --test-threads=1` | exit 0; 6 passed. |
| `mise exec -- cargo test -p pomme-client --locked --profile dev-fast world::block::tests::water_replacement_uses_vanillas_final_per_state_query -- --exact --test-threads=1` | exit 0; 1 passed. Includes force-on/off mismatch assertions and both waterlogged values. |
| `mise exec -- cargo test -p pomme-client --locked --profile dev-fast water_bucket -- --test-threads=1` | Latest run exit 0; 4 passed. Exercises full `start_use_item` path, Java typed selection, wet/dry Ladder, wet Sign, double-slab rejection, propertyless containers, sneaking, per-candidate evaporation values and particle positions, plus unloaded fallback. |
| `mise exec -- cargo test -p pomme-client --locked --profile dev-fast net::environment::tests::water_evaporation_uses_attribute_over_dimension_name_and_tracks_timeline_changes -- --exact --test-threads=1` | exit 0; 1 passed. Verifies actual dimension/biome/timeline semantics (including a non-Nether override). |
| `mise exec -- cargo test -p pomme-client --locked --profile dev-fast particle::tests::every_native_particle_kind_spawns_from_a_valid_typed_packet_fixture -- --exact --test-threads=1` | exit 0; 1 passed / all 125 native kinds. |
| `mise exec -- cargo test -p pomme-client --locked --profile dev-fast particle::tests::particle_asset_reload_replaces_transient_state_and_uses_new_ordered_frames -- --exact --test-threads=1` | exit 0; 1 passed. |
| `mise exec -- cargo test -p pomme-client --locked --profile dev-fast particle::tests::item_world_context_matches_packet_and_local_requests_across_switch_and_reload -- --exact --test-threads=1` | exit 0; 1 passed. |
| `mise run test` | Latest run exit 0; client **1,768 passed / 0 failed / 0 ignored**, protocol **52 passed / 0 failed**, singleplayer **1 passed / 1 ignored**, doc tests 0. |
| `mise exec -- rustfmt --edition 2024 --check pomme-client/src/world/block/mod.rs` | exit 0. |
| `git diff --check` | exit 0 (Git emitted existing LF→CRLF working-copy notices). |

The first post-change run of the 125-kind fixture failed at `bubble`: its `ChunkStore` contained a default chunk with no sections, so `set_block_state` silently no-op'd and the provider correctly removed the bubble outside water. The fixture now allocates native chunk sections before mutating the water/stone states; the provider was not weakened. The corrected 125-kind fixture and full workspace run both pass.

## Limits

This establishes exact source query data and the Rust candidate/CPU particle path, not a GPU or visual comparison against a Java client. Original screenshot particle identity, Java/Rust GUI A/B, and in-game verification remain unperformed. The older-version fallback is retained for historical state tables without `w`; this task's exact oracle coverage is native Java 26.2 / protocol 776.
