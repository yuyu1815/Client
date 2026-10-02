# Final source/type gate — not complete

- Repository: `C:/Users/yuzum/Desktop/mine_rust/Client`
- Actual HEAD during review: `2f267bc607b771977e4dff0d980b779508a8c0c3`
- Gate type: source review + Rust type-check only. No build/link, test execution, runtime, GPU, perf, optimization, or shape roundtrip.
- Scope is the current working tree after the Happy Ghast equipment writer and other existing writers. Tree was already dirty; this review does not attribute inherited changes to itself except the two small compile-only fixes listed below.

## Result

**Not allReady.** `mise run check` passed; the requested `cargo check --tests` initially found test-code/API compile errors, which were minimally corrected, and its rerun passed. Numerous mandatory source chains remain unreviewed to the row-table level, with specific visible gaps below. A successful typecheck is not semantic/render/runtime completion. Do not mark the coverage ledger completed.

## Current-tree entity/render routing evidence and limitations

- Live source is `pomme-client/src/renderer/pipelines/entity_renderer.rs::mob_definitions`; actual production construction is `EntityRenderer::new`, renderer setup in `pomme-client/src/renderer/mod.rs`, then the per-frame `EntityRenderer::draw` lookup. These are the call sites to count, not function definitions or tests.
- Current initializer visibly has old `entity_model::bake_*` entries, module-qualified aquatic/terrestrial/humanoid/flying registrations, and new vehicle/projectile blocks near lines ~2198–2308. `renderer/mod.rs` includes model-module declarations. This is evidence of real calls, but I did **not** produce the required 55-living + 47-nonliving per-kind row table covering packet native kind → spawn/metadata/store → RenderInfo extraction → model/material/texture/bones → frame submit/draw. Therefore those totals and end-to-end completeness are **not certified here**; no definition/test-only count is claimed.
- The compile warning from current `cargo check` proves at least these compiled bakes currently have no non-test caller: `entity_models::flying::bake_happy_ghast_harness_model` (`flying.rs:519`), `bake_ender_dragon_model` (`flying.rs:733`), `entity_models::humanoid::bake_creaking_eyes_model` (`humanoid.rs:652`), `entity_models::terrestrial::bake_llama_decor_model` and `bake_baby_llama_decor_model` (`terrestrial.rs:3194,3200`); `entity_models::nonliving_special::{bake_armor_stand_model,bake_end_crystal_model}` and helpers also warn unused. These must not be counted as drawn. In particular the unused harness bake is not a Happy Ghast gear layer. Exact intended species scope / whether any are deliberately deferred remains to be resolved by source row audit.
- `projectiles.rs` and `vehicles.rs` have actual registration code in `mob_definitions`; module definitions alone are not used as a count. Per-target state/texture/submission still needs audit.
- Legacy shape/model collision with newly baked models and every remaining dead bake was not enumerated exhaustively. The compiler warnings above are confirmed instances, not a complete dead-bake list.

## Source issues / evidence

### P1: Happy Ghast harness/equipment is still not wired

`bake_happy_ghast_harness_model` has no production caller (confirmed by `mise run check` dead-code diagnostic); the preceding Happy Ghast gear assessment found no full BODY `ItemStack`/equipment-asset resolver/layer or leash-holder draw path. `SetEquipment`/entity/render chain must be followed in this actual tree before claiming the feature. Do not substitute `saddled: bool` or a fixed harness texture for native slot+Equippable asset selection. Confirm whether a later writer changed this area by source-level trace; no such layer/caller appeared in the diagnostics inspected.

### P1: unrelated source/layout review is not finished

The following explicit required checks have **not** been completed as independent source comparisons and block closure:

1. Build actual per-target row tables for 55 living and 47 nonliving kinds through metadata/storage, extraction, model/material/texture/bones, and actual frame submit/draw. Existing reports tied to old HEAD are historical inputs only.
2. World block entities: ordered four pot textures/facing/wobble 7/10; actual 7-piece book including nonempty zero-thickness cover faces/table base; bell event direction/shake; ordered banner cloth-only masks/default/custom tags/assets; campfire cozy/signal smoke and occupied-cook particles. Trace actual event/handler → stored data → tick → renderer/particle consumer. Existing `block_entity.rs` has `Items` NBT lookup and pot `sherds` lookup, but that alone does not establish 26.2 native key/casing/type; verify against the supplied decompiled `ValueInput/ValueOutput` and real handler payloads, not a presumed literal.
3. Review every new fixed tick against the actual `while core.tick_accumulator >= TICK_RATE` call chain. Observed `in_game.rs:3134-3138` uses fixed accumulator; at ~3182-3207 it ticks item entities/projectile displays, bell, book, and later campfire particles inside the tick loop. This is positive evidence for those listed updates being tick-based, but does not certify all updates, server-tick-rate behavior, partial-tick interpolation clock, or no render-FPS coupling.
4. Remote player profile entity UUID vs identity; local skin mask vs NPC; LeftMainHand logical stack/active-hand invariants against physical side 2×2, arm UVs and winding. Not fully source-compared.
5. NightPackedVertex/raw pair/all writers, greedy identity, actual minimum stride, raster layouts and all GLSL input locations/sizes/offsets. The prior `0..4 -> raw location 5` assumption is not accepted: actual `entity_renderer.rs` currently builds `ChunkVertex::attribute_descriptions()` then appends seven instance attributes at locations `3+i` for Instanced (`~3949-3971`). That deserves direct collision review against the chunk attributes and shader interfaces; it is not proof location 5 is valid. Do not validate from comments alone.
6. Scalar skyDarken source → frame uniform → chunk/water/translucent/entities/particle lightmap, day ratio/AO/block-light invariants, skyless dimension and four-sample paths; the prior source-only report explicitly noted `world_shadow.rs` does not track `skyDarken`. Position bias/range, offset origin/effect mesh skip, GLSL layouts/MAX_BONES, shaders, missing sprites, atlas/alpha/depth/cache reload and old-generation disposal/removal cleanup also remain unchecked here.
7. Loom/Stonecutter native 26.2 server button candidate order, property-0 authority, item tags/default components/tombstones/result pickup; classify Preview as unsupported when appropriate. Check five unsupported settings remain disabled. Command editor 2-second matching-position pending request/permissions/native packet; right-use GUI latch/short-release/tick-before-packet; FpsRunId validity; shape-offset and shape dictionary validation regression checks also remain open.

### Confirmed potentially important findings from live source

- `pomme-client/src/world/block_entity.rs:165` extracts campfire slots from exact key `"Items"` via `NbtTag::List(NbtList::Compound(...))`; the test at ~923 also manufactures `"Items"`. I did **not** use this test literal as proof of native 26.2 key/casing. It needs the requested direct `ValueInput/Output` and packet-payload verification.
- `in_game.rs:3184-3207` places projectile/block-entity/book updates inside fixed tick; `core.rs:3003` stamps pot wobble with `game.tick_count`. This is a source call-chain observation only, not verification of all clocks or interpolation.
- Actual sky-review grep found `pomme-client/src/renderer/world_shadow.rs:107-109` explicitly states `skyDarken` is not tracked and uses a daytime fixture with sky level 15. This is a concrete missing scalar-lighting source input, not a completed source chain.
- `cargo check` warned `particle::ParticleStore::tick` is unused in normal client build at `particle.rs:2077`; confirm the intended tick integration/modes and do not infer execution from its definition.

## Minimal owned source changes for type errors

The first test-target check failed with seven compile errors. I made only test/type-support fixes:

- `pomme-client/src/app/phases/in_game.rs`: changed the new `snowball_render_infos` test callback arguments from `Option<Mat4>` to the function's required `Fn(&str) -> Option<Mat4>` closure, adapted a captured test mesh as `|_| mesh`, and compared the warmed item key to the observed exact `"snowball"` key rather than a nonexistent constant.
- `pomme-client/src/renderer/pipelines/entity_renderer.rs`: added `Debug` derive to `AnimationType` because a test `assert_eq!` formats it.

No runtime/semantic fallback implementation was added. These edits were typechecked but **their tests were not run**.

## Checks and exits

1. `cd /c/Users/yuzum/Desktop/mine_rust/Client && mise run check` — exit **0**; `cargo check -p pomme-client --locked --profile dev-fast`; warnings include unused book/living/nonliving bakes and existing unused symbols.
2. `mise exec -- cargo check --locked --profile dev-fast --tests -p pomme-client -p blockgen -p pomme-singleplayer` — first exit **101**, failed only in test compilation (missing `SNOWBALL_ITEM_NAME`, four wrong snowball callback argument types, `AnimationType: Debug`). After the fixes above, exact rerun exit **0**; produced type/check artifacts, **did not execute tests**. Existing warnings remain.
3. `mise exec -- rustfmt --edition 2024 --config skip_children=true --check <all modified Rust files and new ui/loom.rs, ui/stonecutter.rs>` — exit **0**, no output.
4. `git diff --check` — exit **0**, no whitespace errors; emitted expected LF→CRLF working-copy warnings.
5. `mise exec -- node scripts/check-report-audit.cjs` — exit **0**: `batches=793 unique=793 duplicates=0`; `ledger=793 unique=793 duplicates=0 missing=0 extra=0`; `ledger_objects=486`; external object index skipped/unavailable; separate shape scope 1,196 blocks / 32,366 states. This validates record accounting only; does not prove source implementation/coverage and does not update the ledger.
6. Prior unauthorized partial tests per task history: Pot 5 + Book 2 = **7 passed**. These are honestly recorded historical facts only; they do not demonstrate final gate completion. No test command was run during this review.
7. Build/link/runtime/perf/optimization/shape roundtrip: **not run by instruction**.

## HEAD, tree, hashes, commit state

- HEAD stayed `2f267bc607b771977e4dff0d980b779508a8c0c3`; no commit made.
- Worktree has the inherited 24 modified Rust source files plus untracked report-audit notes, `pomme-client/src/ui/loom.rs`, and `pomme-client/src/ui/stonecutter.rs`; this reviewer additionally modified only the two Rust files above and added this report. Preserve all inherited diffs; do not stage an indiscriminate whole tree.
- SHA-256 after this review:
  - `pomme-client/src/app/phases/in_game.rs`: `542c6fc99f4a5e8227356650a554f7efbb288f6706bd4be0ff8784db1ceb9d1f`
  - `pomme-client/src/renderer/pipelines/entity_renderer.rs`: `52e755f8586c43bb0a2c4266b4d9388b3d8f5886463e60f3989c1ddc36165235`
  - `pomme-client/src/world/block_entity.rs`: `cd1cbce9068849f10613daa8a9f7d2694c465a3b92aeba0c27fb53a38fc40579`
  - `pomme-client/src/entity/mod.rs`: `176292d31ca9e85e624da9a045046c72c0f6a195d3b943344a6b3e7d9df1b741`
  - `pomme-client/src/net/mod.rs`: `50102f40b2a9f465d2fd582adbbc4ddb2fc74fd3c96bd4099dee0fa67f590936`
  - `pomme-client/src/particle.rs`: `01fe9d0ee0c4933ac3675593394d0258588f02f710d72f41044c21f71dc1825f`
  - `pomme-client/src/renderer/chunk/buffer.rs`: `1131a4756c967a1c6b3ebb261ecd7813ed34d665f889eaf8ea8529e94a365152`
  - `pomme-client/src/renderer/chunk/mesher.rs`: `2ce4efa6dc3e08693967bc08a481dc5c46a8fdc7ef0fef07f8028e8847e4cc56`

## Next gate work

1. Finish the source-only rows for 55 living / 47 nonliving and the targeted BE, NBT, rate, hands/skins, GPU bindings/shaders/lighting/reload/menu/command/shape checks above. Report exact per-kind and exact remaining unused bake lists; do not use a name count or test call as proof.
2. If further source changes are necessary, keep changes minimal and owned; re-run only allowed check/type/format/diff/audit validation. Do not run tests/build/runtime until authorized phase.
3. Preserve gate order: final build → tests/runtime later → only then optimization. Shape roundtrip is not authorized now; its checked-in inputs and existing report evidence are sufficient for that later phase, so a missing placeholder oracle is not a blocker.
4. Aggregate inherited diffs carefully. Commit policy is `git commit --only` for reviewed small source-fix files; no commit was created here. Runtime remains unperformed.

## Later source-writer closeout (HEAD `4e0e1c2945b909d79d53fce08c8ea7643b5600a4`)

This addendum preserves the review snapshot above; it does not rewrite its HEAD, test history, or conclusions. After the source writers finished, the repository advanced from the report's recorded `2f267bc...` snapshot to `4e0e1c2`. This is a documentation closeout, not a new source review or a claim that the overall gate passed.

- The source-writer phase is complete, but the repository is still **not allReady**. The audit's `793` records (and matching ledger count) measure report accounting, not implementation readiness or the required row-level production route verification. Keep `coverage-ledger.json` statuses as recorded; do not infer readiness from its count.
- Later targeted source findings to carry forward: Night terrain had a duplicated curve and was fixed; chest handling is guarded (`d0e0`/`cargo4e` noted in the source-writer closeout); client tick source was checked, but `cloak` remains at a low update rate. These findings do not substitute for a whole-tree audit or runtime verification.
- No end-to-end completion claim is made. The earlier unresolved source-chain, graphics/layout, feature, test/runtime, and row-table limitations remain unless separately verified and documented against this HEAD.
- Snapshot files under `.agent-snapshots/` are retained locally as agent before/status logs and are intentionally excluded from commits; they are not source deliverables.
