# Small-scope implementation queue (snapshot `8ffc5465bf81ffa25cf1867866e8c64f23e88857`)

This document plans work only; no source edits or claims of visual/runtime completion. The 102-row production-route inventory is `current-entity-route-matrix.md`. Its source state is ahead of older plans: all 55 living kinds now have a living store classifier, MobDef/bake, common `EntityRenderInfo` extraction and `EntityRenderer::draw` route, while nonliving families have several distinct current extractors. None of this turns the remaining layers/behavior into ready.

## Ranked tasks, with bounded file/function edges

### 1. Night lighting contract (P0; one render-input-to-shader slice)
**Files:** `pomme-client/src/renderer/mod.rs` (`Renderer::render_world` / sky+terrain pass inputs), the terrain pipeline/shader binding files, `pomme-client/src/renderer/shaders/`.
**Boundary:** preserve raw night/sky-darken values from world state through frame input and shader uniform; do not combine with entity model work. Establish the actual vanilla `SkyDarken`/night-terrain relationship and one uniform source of truth. No render-layer or model changes.

### 2. Typed display completeness (P1; split by entity kind)
**Files:** `net/handler.rs::dispatch_world_packet`, `net/mod.rs::NetworkEvent`, `app/core.rs` metadata event apply, `entity/mod.rs::VehicleState`/metadata setters, `app/phases/in_game.rs` display extraction (BlockDisplay around 5410, ItemDisplay around 5578), `renderer/pipelines/item_entity.rs` only if context/material API blocks reuse.
**Boundary A ItemDisplay:** retain full item stack/components and context transform semantics; then common Display interpolation/view-range/glow fields. Existing transform/billboard extraction is real but not a full Display implementation. Keep separate from TextDisplay's index 23 text.
**Boundary B BlockDisplay:** typed full BlockState and verify its extraction, model/material resolution and final item-entity pipeline submission. Do not count `ensure_block_mesh` or a unit test as draw proof.

### 3. Per-species defining layer/state fixes (P1; one species family per patch)
**Mooshroom:** `entity_renderer.rs::mob_definitions`/overlay construction + `in_game.rs::entity_extras` + block-model extraction/draw. Add actual mushroom block models/material, never cow-only completion.
**Shulker:** `entity/mod.rs` shulker metadata/position bounds + `in_game.rs` render position/culling + `entity_renderer.rs` animation/model transform. Keep attached-face and teleport bounds coherent; do not call generic invisibility a fix.
**Wither:** handler typed/index retention, `LivingEntity` target fields and animation input in `in_game.rs::entity_extras`, then `entity_renderer.rs` three-head transforms. Store target IDs/state; do not add only `bake_wither_model` (already exists).

### 4. Flying animation slice (P2; flags/clock only after state audit)
**Files:** `net/handler.rs` typed metadata, `entity/mod.rs::LivingEntity` fields and fixed-tick update, `in_game.rs::entity_extras`, matching `entity_renderer.rs::AnimationType` arm. Scope each to an exact species (Allay, Bee, Blaze, Breeze, Ghast, HappyGhast, Phantom, Vex are individually distinguishable); the “7 flying animation flags/keyframes” report does not authorize silently including/excluding a species. First reconcile that count against mother-set/batch rows, then land one family at a time. Avoid animation on frame delta if source animation state advances on fixed entity ticks.

### 5. Vehicle geometry/state (P2)
**Files:** `in_game.rs::boat_render_infos` (6219), `minecart_render_infos` (6718), `minecart_cargo_render_infos`, `entity_renderer.rs` vehicle MobDefs/models and draw state, `entity/mod.rs::VehicleState` typed metadata.
**Boundary:** verify old/new rail coordinate/pose through real renderer caller; separately review default chest cargo geometry with reference before claiming complete. Boat paddle/lifecycle/cloth and predicate remain discrete state/layer work, not a universal boat rework. Latest boat water mask predicate and cape lifecycle UV changes are source-present, not runtime verified.

### 6. Independent product/UI/model gaps (P2–P3)
- Banner ordered cloth: block-entity extraction/state → cloth mesh/material and actual frame draw; no `Banner` row should imply done.
- ShieldPatterns: preserve pattern components from inventory to shield layer/material; Loom real preview is separately in `pomme-client/src/ui/loom.rs` and item render path.
- Mannequin: handler profile + `VehicleState.mannequin_profile`, `mannequin_render_infos`, player-skin fetch/cache exact input; stop name-only fetch and retain SkinPatch/texture source.
- Trident/enchantment glint: keep base/foil second-pass geometry; user knobs are a config boundary, no change to forced-foil semantics without an explicit contract.
- Rope/leash and skull jaw/ear: distinct render submission/model animation, not MobDef coverage.
- Shape offset: `block_shape` canonical geometry API plus position-aware collision/raycast callers; current 0/110 means explicitly out of scope for entity tasks.

## Findings / readiness controls

- Snapshot HEAD fixed at `8ffc5465bf81ffa25cf1867866e8c64f23e88857`. Current production caller audit supports the shared living route and several special consumers; the detailed 102 entries are in the matrix.
- Latest harness-panic repair, boat water-mask predicate, cape UV lifecycle and fixed-tick motion persistence are present in source on this snapshot; none establish runtime verification.
- **No additional confirmed new source bug is asserted in this inventory.** Unproven concerns stay `unknown`/remaining rather than being promoted from a warning, registered bake, or unittest. Before filing another bug, read the whole source caller and record exact file:line + concrete violated behavior.
- `allReady=false`; 102 target rows inventoried, zero claimed visually/runtime verified. Explicit known non-completions in the matrix remain blockers, not “hidden complete.”

## Allowed follow-up evidence (not run here)

A downstream implementation task should cite source line ranges and narrow checks authorized by that task. The source inventory did not run build, tests, `--no-run`, runtime/GPU, or optimization checks. Current allowed documentation-only check commands are `node scripts/check-report-audit.cjs` (audit data integrity) and `git diff --check`; these do not validate renderer output.
