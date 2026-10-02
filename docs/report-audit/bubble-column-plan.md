# Bubble-column movement investigation (26.2)

## Scope and result

Investigation only. No source changes, commit, build, or tests were made. This plan covers the two recorded cases `MOVE-bubble_up` and `MOVE-bubble_down` in report commit `0a1aee9c6be911a3081cf454b0bae6d694ec9d55`.

The likely minimal implementation is **not** a special travel mode. Bubble columns have water fluid state and use the normal water travel path; 26.2 invokes a separate bubble-column entity-inside callback during entity movement. Pomme already recognizes bubble columns as water but has no corresponding callback in the player movement pipeline. Also, the current eye-water flag treats a bubble-column block as water, so air is depleted where vanilla exempts the eye-in-column case.

## Record facts

- `records/MOVE-bubble_up.json`: soul sand at `(250,63,0)`, source water from Y64–77, no input for four seconds, three repeats per client. All Pomme repeats remained at Y63.875; reference starts at Y79.1445 and reaches a max Y80.7698 during the sampled path. Valid fixture in all six raw runs.
- `records/MOVE-bubble_down.json`: magma at `(250,63,0)`, water Y64–77, start around Y72.9, no input for four seconds, three repeats per client. Reference reached Y64.0 with minimum vertical velocity `-0.3`; Pomme ended around Y70.525–70.600 with minimum `-0.025`. All six runs marked valid.
- Both are reported `observed_runtime_difference`; neither record confirms an independent root cause. The actual report observations support missing bubble response but do not by themselves establish exact per-tick constants or tick ordering.

## Code traced in Pomme

- `pomme-client/src/physics/movement.rs::tick_with_context` (around lines 107–125): updates water state, applies fluid currents, resets fall distance, then does input and `travel`.
- `travel` (around 217–245) selects `tick_water` whenever `player.in_water` is true.
- `tick_water` (around 475–537) applies input/swimming movement, calls `apply_collision_with_context`, then applies horizontal drag and `WATER_VERTICAL_DRAG = 0.8`, followed by `fluid_falling_adjusted`. This is the ordinary-water travel branch bubble columns should continue to use.
- `apply_collision_with_context` (around 700–870): resolves collision and updates player position/state. It handles cobweb, powder snow and honey block callbacks/effects, but has no bubble-column effect. Its call occurs inside `tick_water` before that function applies water drag, which is the appropriate insertion point for an Entity.move-style bubble callback.
- `pomme-client/src/player/mod.rs::update_water_state` (around 623–683) sets `in_water` from fluid height and `eyes_in_water` using `is_water_block`. `is_water_block` (around line 60) accepts every state whose `fluid(state).kind` is water. The `tick` method then calls `tick_air_supply` (currently around line 611) after `travel`; the value being consumed was computed before travel. `tick_air_supply` decrements whenever `eyes_in_water` is true and otherwise refills by `AIR_RECOVERY_RATE` (four, subject to `MAX_AIR_SUPPLY`). There is no bubble-column exception.
- `pomme-client/src/world/block/mod.rs::state_fluid` maps `bubble_column` to `FULL_WATER`; `BlockData.properties` stores parsed state properties in a `PropMap`. The shipped `pomme-client/src/world/block/data/blocks-26.2.json` entry has `bubble_column` property `drag` with values `true` and `false`. Thus the per-state direction property is already available through `block_properties(state).get("drag")`; no new registry field or hand-maintained block metadata appears necessary.
- `pomme-client/src/physics/movement.rs::block_movement_factor` mentions `bubble_column` only in its normal-water movement-factor special case. It is not a vertical bubble-force implementation.

## Reference/source evidence

No Mojang Java `Entity.java` / `BubbleColumnBlock.java` source file was found in this checkout. The available direct implementation reference is the repository's SteelMC 26.2-compatible port; it explicitly labels these methods as Vanilla behavior:

- `third_party/SteelMC/steel-core/src/entity/mod.rs` constants around lines 138–144:
  - inside upward max `0.7`
  - inside downward min `-0.3`
  - open-above upward max `1.8`
  - open-above downward min `-0.9`
  - downward acceleration `0.03`
  - inside upward acceleration `0.06`
  - open-above upward acceleration `0.1`
- `third_party/SteelMC/steel-core/src/entity/entity/mod.rs::default_on_inside_bubble_column` (around lines 437–450): downward `max(vy - 0.03, -0.3)`, upward `min(vy + 0.06, 0.7)`; preserves X/Z and resets fall distance. Flying players return without effect.
- `default_on_above_bubble_column` (around lines 408–424): downward `max(vy - 0.03, -0.9)`, upward `min(vy + 0.1, 1.8)`; preserves X/Z, does not reset fall distance; flying players return without effect.
- `third_party/SteelMC/steel-core/src/behavior/blocks/fluid/bubble_column_block.rs::is_open_above` (around lines 122–130): open above exactly when the block above has an empty collision shape **and** an empty fluid state. `apply_entity_effect` reads the bubble state's `DRAG` and chooses the open-above or inside callback; it does nothing when the entity-inside hit is not precise.
- `third_party/SteelMC/steel-core/src/entity/mod.rs::apply_block_effect_segment` (around lines 505–567): after movement, checks block/fluid intersection on the swept path; block effects are checked against the entity-inside shape, and precision uses movement distance or an end AABB intersection. This is useful for matching the callback timing and avoiding a check based only on the player's eye or block coordinate.
- `third_party/SteelMC/steel-core/src/entity/tests/fall_and_fluids.rs` (around lines 156–217) asserts inside up `0.68 -> 0.7`, inside down `-0.28 -> -0.3`, above up `1.75 -> 1.8`, above down `-0.88 -> -0.9`, inside fall-distance reset and flying-player immunity.
- Its `living_entity.rs::is_eye_in_bubble_column` and `tick_living_air_supply` (around lines 1392 and 1433–1473) show the air rule: water drains air only when the eye is in water **and not** in a bubble column; otherwise supply recovers by four up to max. This keeps swimming/water detection distinct from drowning exemption.

The supplied `third_party/SteelMC/steel-registry/build_assets/blocks.json` bubble-column entry also confirms empty collision/outline boxes, water fluid state, and a `DRAG` property with default true. Pomme's 26.2 block data separately confirms property spellings/values `drag=true|false`. The user's candidate constants therefore match the inspected code/test evidence, with one qualification: open-above *downward* cap is `-0.9`, not `-0.3`.

## Concrete proposed changes (do not apply in this investigation)

1. In `pomme-client/src/physics/movement.rs`, add one private bubble-effect helper and call it from `apply_collision_with_context` after the resolved player position/AABB is established and before control returns to `tick_water` / `tick_land` (hence before their drag/gravity tail). Apply it only to bubble-column blocks intersecting the entity-inside region; do not use eye position as the movement trigger. Preserve the existing water fluid detection, `apply_fluid_currents`, `tick_water` selection, and water drag. The callback is an addition to water travel, not a replacement for it.
2. For each intersected column state, read `drag` from `crate::world::block::block_properties(state)` (`true` means downward, `false` upward). Determine `open_above` from the above block's collision shape being empty and its fluid being empty, matching Steel's `BubbleColumnBlock::is_open_above`; a bubble/water block above means the inside limits apply. Use collision shape semantics, not render/outline shape.
3. Apply exact Y-only equations:
   - Inside + drag up: `vy = min(vy + 0.06, 0.7)`.
   - Inside + drag down: `vy = max(vy - 0.03, -0.3)`.
   - Open-above + drag up: `vy = min(vy + 0.1, 1.8)`.
   - Open-above + drag down: `vy = max(vy - 0.03, -0.9)`.
   Do not alter horizontal velocity. Reset `fall_distance` to zero only for the inside-column callback, as Steel does; open-above keeps accumulated fall distance. Skip both callbacks while `player.flying` (the reference's `is_flying_player` guard).
4. In `pomme-client/src/player/mod.rs`, preserve `eyes_in_water` for swimming/water travel; add a distinct eye-in-bubble-column state or equivalent current-tick query for air supply. Change only the drowning condition to `eyes_in_water && !eyes_in_bubble_column`; otherwise use the existing +4 recovery capped at `MAX_AIR_SUPPLY`. Do not make bubble columns globally not-water: `in_water`, `tick_water`, and swimming/fluid behavior still need water semantics. Keep air evaluation based on the pre-travel eye state already computed by `update_water_state`; reordering the whole player's air tick is not needed for this scoped behavior.

## Minimal regression plan

- Pure helper tests in `movement.rs`: all four equations at sub-cap and cap-crossing values; preserve X/Z; inside resets fall distance while open-above does not; flying state is unchanged. Use the exact Steel boundary cases above.
- Movement fixture tests in `movement.rs`: a continuous column with `drag=false` and an air block above must use the open-up branch; `drag=true` with air above must use `-0.03` and `-0.9` cap. Same states with water above must use inside limits (`+0.06/0.7`, `-0.03/-0.3`). Place player AABBs intersecting the column, then just outside it, to detect accidental eye-only or non-intersection triggering.
- Air test in `player/mod.rs`: eye in ordinary water drains one air per tick; eye in bubble column recovers four to max; eye in air recovers four; bubble at feet but eye in ordinary water still drains. This guards against conflating block-volume water contact with the eye drowning exception.
- Tick-order/control regression: compare the same water and bubble setups under neutral input. Confirm normal `tick_water` drag remains in the pipeline and bubble impulse is applied during collision/move before the water drag tail; do not add the bubble impulse both before travel and after movement. Re-run the two report trajectories (four seconds, three repeats each) only after implementation is owned and runtime testing is approved.

## Unresolved / not verified

- No current build/test/runtime replay was run (explicitly prohibited). The existing test evidence is Steel source, not a Pomme runtime test.
- The checkout has no direct Mojang Java source. Steel is a port labeled to mirror vanilla, supported by its explicit tests; confirm against the intended 26.2 decompiled/reference source if it becomes available before implementation.
- Pomme does not expose a generic swept `checkInsideBlocks` callback pass. The helper must choose an AABB/swept-block scan that matches actual bubble-column entity-inside intersections and avoids duplicate acceleration when several column cells overlap one player. Steel's `visited_blocks` set means each block effect is applied at most once per movement segment; a simple per-cell loop must preserve that property.
- For entities spanning multiple vertically stacked bubble blocks with different `drag` states, exact block iteration/order can affect repeated callback application. Normal columns share the state; mixed states require a targeted fixture rather than an arbitrary first-match rule.
- The report's exact movement tick trace and Pomme's local-vs-server authority could not be reconstructed solely from the two summary observations; reproduce with the referenced setup before comparing per-tick values.
