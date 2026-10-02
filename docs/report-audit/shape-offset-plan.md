# 26.2 shape offset audit: `offset_dependent_states=110`

Investigation only; no runtime/data/code changes. Scope is `shapes-26.2.json` (`state_count=32366`, `verified_states=32211`, `skipped_states=155`, comprising 45 context-dependent + 110 offset-dependent; `oracle_disagreement_states=0`). The 110 are exactly the six groups below. Do not mix these with the 45 context-dependent states.

## 110 skipped states (26.2 IDs, exact state enumeration)

All property combinations listed are actual 26.2 block table properties and use the per-block state ordering in `blocks-26.2.json` / `blockgen::state_properties`: first listed property varies slowest, last varies fastest. Ranges are inclusive state IDs.

| IDs | Block and property assignments | Count | Offset data channel | Pomme vs comparison report |
|---|---|---:|---|---|
| 45–84 | `mangrove_propagule`: `age=0..4 × hanging=true,false × stage=0,1 × waterlogged=true,false` (ID order is lexicographic in that listed product; each age contributes 8 IDs) | 40 | outline only | Collision equal (`[]`); outline differs all 40: Pomme fallback full cube vs report's single small propagule box. |
| 2321 | `dandelion {}` | 1 | outline only | Collision equal (`[]`); outline differs: full cube vs report box. |
| 2322 | `golden_dandelion {}` | 1 | outline only | Same. |
| 2323 | `torchflower {}` | 1 | outline only | Same. |
| 2324 | `poppy {}` | 1 | outline only | Same. |
| 2325 | `blue_orchid {}` | 1 | outline only | Same. |
| 2326 | `allium {}` | 1 | outline only | Same. |
| 2327 | `azure_bluet {}` | 1 | outline only | Same. |
| 2328 | `red_tulip {}` | 1 | outline only | Same. |
| 2329 | `orange_tulip {}` | 1 | outline only | Same. |
| 2330 | `white_tulip {}` | 1 | outline only | Same. |
| 2331 | `pink_tulip {}` | 1 | outline only | Same. |
| 2332 | `oxeye_daisy {}` | 1 | outline only | Same. |
| 2333 | `cornflower {}` | 1 | outline only | Same. |
| 2334 | `wither_rose {}` | 1 | outline only | Same. |
| 2335 | `lily_of_the_valley {}` | 1 | outline only | Same. |
| 15278 | `bamboo_sapling {}` | 1 | outline only | Collision equal (`[]`); outline differs: full cube vs report's 0.5 × 0.75 × 0.5 box. |
| 15279–15290 | `bamboo`: `age=0,1 × leaves=none,small,large × stage=0,1` (12 IDs; last property varies fastest) | 12 | collision + outline | Both differ for all 12; Pomme is full cube in both channels, report has narrow stalk collision and stalk/leaves outline. Outline varies by `leaves=large`; `age` and `stage` do not affect reported shape. |
| 30209–30228 | `pointed_dripstone`: `thickness=tip_merge,tip,frustum,middle,base × vertical_direction=up,down × waterlogged=true,false` (20 IDs) | 20 | collision + outline | Both differ for all 20; Pomme full cube, report uses thickness/direction-dependent spike boxes. Waterlogged does not affect shapes. |
| 30229–30248 | `sulfur_spike`: same property product/order as pointed dripstone | 20 | collision + outline | Both differ for all 20; same reported geometry by corresponding thickness/direction as pointed dripstone. |
| 32361 | `open_eyeblossom {}` | 1 | outline only | Collision equal (`[]`); outline differs: full cube vs flower box. |
| 32362 | `closed_eyeblossom {}` | 1 | outline only | Same. |
| **Total** | 22 block records (including the 15 one-state flower/eyeblossom records) | **110** | | |

For an unambiguous individual ID-to-property expansion, e.g. propagule ID `45 + age*8 + hanging_index*4 + stage*2 + waterlogged_index`, with enum/bool indices in the order shown above. Bamboo ID `15279 + age_index*6 + leaves_index*2 + stage_index`; spike-family ID `first + thickness_index*4 + direction_index*2 + waterlogged_index`. This enumerates every ID in the table without conflating property layouts from another version.

### Reported comparison boxes and shape differences

Values below are `[min_x,min_y,min_z,max_x,max_y,max_z]` from `evidence/all-state-shape-comparison.json`; the Pomme side is identical for each state within the group unless stated. They are report-observed boxes, not yet asserted to be canonical, position-independent data.

- Mangrove propagule: Pomme collision `[]`, report collision `[]`; Pomme outline `[[0,0,0,1,1,1]]`. Report outline by age (same for both `stage` and both `waterlogged` values): age 0 hanging `[[.1875,.8125,.1875,.3125,1,.3125]]`, standing `[[.1875,0,.1875,.3125,1,.3125]]`; age 1 hanging minY `.625`; age 2 hanging minY `.4375`; age 3 hanging minY `.1875`; age 4 hanging minY `0`. Standing minY is `0` at every age. In all cases minX=minZ `.1875`, maxX=maxZ `.3125`, maxY=1.
- The 15 single-state flowers and both eyeblossoms: Pomme collision/report collision `[]`; Pomme outline full cube; report outline `[[.0625,0,.0625,.4375,.625,.4375]]`.
- Bamboo sapling: collision `[]`; Pomme outline full cube; report outline `[[0,0,0,.5,.75,.5]]`.
- Bamboo: Pomme collision and outline full cube. Report collision for every state `[[.15625,0,.15625,.34375,1,.34375]]`. Report outline for `leaves=none|small` `[[.0625,0,.0625,.4375,1,.4375]]`; for `leaves=large` `[[-.0625,0,-.0625,.5625,1,.5625]]`.
- Pointed dripstone / sulfur spike (both channels agree with each other for each state): `tip_merge` up/down `[[.1875,0,.1875,.5625,1,.5625]]`; `tip` up `[[.1875,0,.1875,.5625,.6875,.5625]]`, down `[[.1875,.3125,.1875,.5625,1,.5625]]`; `frustum` either direction `[[.125,0,.125,.625,1,.625]]`; `middle` either direction `[[.0625,0,.0625,.6875,1,.6875]]`; `base` either direction `[[0,0,0,.75,1,.75]]`. `waterlogged` does not change geometry.

Thus the current skip discards real state-specific geometry; recording those translated report boxes in a state table would also bake an unknown sample position into the static state record and still be wrong at other coordinates.

## Vanilla 26.2 source bytecode findings

Jar inspected: `C:/Users/yuzum/AppData/Roaming/.minecraft/versions/26.2/26.2.jar`; executable was `C:/Program Files/Java/jdk-21/bin/javap.exe`. Commands were `javap -classpath <jar> -c -p net.minecraft.world.level.block.state.BlockBehaviour`, `... 'BlockBehaviour$Properties'`, `... 'BlockBehaviour$OffsetType'`, and focused `Mth` / block class inspections.

- `BlockBehaviour.OffsetType` values are `NONE`, `XZ`, `XYZ`. `Properties.offsetType(NONE)` stores null offset function; `XZ` selects `lambda$offsetType$1`; `XYZ` selects `lambda$offsetType$0`.
- Both functions seed using **`Mth.getSeed(pos.x, 0, pos.z)`** (the block position's y is intentionally discarded for randomized shape offset).
- `Mth.getSeed(Vec3i)` delegates to `getSeed(x,y,z)`. Bytecode arithmetic is Java-long wrapping arithmetic:
  `s0 = ((long)x * 3129871L) ^ ((long)z * 116129781L) ^ (long)y; seed = (s0*s0*42317861L + s0*11L) >> 16`.
- Let `s=Mth.getSeed(x,0,z)` and `H=block.getMaxHorizontalOffset()` (float promoted to double in math):
  `dx = clamp((((s & 15L)/15.0)-0.5)*0.5*H, -H, H)`;
  `dz = clamp(((((s >> 8) & 15L)/15.0)-0.5)*0.5*H, -H, H)`.
  XZ result is `(dx,0,dz)`.
- XYZ additionally sets `dy = ((((s >> 4) & 15L)/15.0)-1.0)*block.getMaxVerticalOffset()`; result `(dx,dy,dz)`. Since nibble/15 is in [0,1], y range is `[-V,0]`; X/Z range is `[-H/4,+H/4]` before the explicit clamp (which is looser for ordinary nonnegative H).
- `BlockBehaviour.getMaxHorizontalOffset()` bytecode default returns `0.25f`; `getMaxVerticalOffset()` returns `0.2f`. Steel's extracted 26.2 block setup confirms `mangrove_propagule`, flowering plants, bamboo sapling/stalk and both eyeblossoms use XZ; `pointed_dripstone` and `sulfur_spike` use XZ with max horizontal `0.125f` (`steel-registry/src/generated/vanilla_blocks.rs`; source evidence, generated file not edited). Therefore all 110 have **zero vertical offset**. H=.25 gives each horizontal shift in [-.0625,+.0625]; H=.125 gives [-.03125,+.03125]. No per-state max value is used.

### Important unresolved comparison/normalization discrepancy

`all-state-shape-comparison.json` does not encode a sample `BlockPos`, and `shape643d385` was not present as a literal in the supplied report tree or repository data when searched. `shapes-26.2.json` identifies only the offset-dependent state count; `blockgen::oracle_shape` deliberately returns `[]` for a channel with `usesOffset=true`, so it does not normalize or verify these boxes. The Steel asset `build_assets/blocks.json` keeps canonical-looking boxes in its shared shape dictionary, marked with `usesOffset=true`; e.g. pointed/sulfur shape 441 has X/Z `[.3125,.6875]`, shape 602 `[.3125,.6875]`, shape 603 `[.3125,.6875]`, bamboo shape 222 `[.40625,.59375]`, and flower shape 54 `[.3125,.6875]`.

The report's pointed/sulfur `tip_merge` observed box X/Z `[.1875,.5625]` is translated by `-.125` relative to Steel shape 441 `[.3125,.6875]`. That is outside the XZ bytecode bound for verified H=.125 (`|dx|,|dz|<=.03125`). Do **not** claim all report values normalize to the Steel canonical shapes yet. Either the report reference used a different position/source/version, or the Steel canonical record or max-offset assumption needs another source check. The report reference's position must be retrieved from the shape report metadata/fixture (the current all-state JSON omits it) and corresponding 26.2 source/runtime parameters checked before stating equivalence. Other offsets are not proven identical simply because values look centered.

## Relevant runtime call sites (change plan only; no code edits)

- Outline raycast: `pomme-client/src/player/interaction.rs`, `raycast` gets concrete cell `(bx,by,bz)` and calls `block_shape::outline_shape(state)` around line 2170; `clip_with_interaction_override` / `clip_shape` accept `block_pos`, where the local boxes become world AABBs. Minimal correct boundary: obtain a canonical shape by state, then apply offset computed from the actual `block_pos` before clipping; do not shift at `outline_shape(state)` without a position.
- Collision AABB collection: `pomme-client/src/physics/collision.rs`, `visit_block_aabbs_bounded` has `(bx,by,bz)` and state before matching `block_shape::partial_shape(state)` around line 143. Translate canonical boxes by block origin **plus** state/block offset. Shared implementation belongs at this world-position shape-to-AABB boundary so `collect_block_aabbs`, player collision collection, `no_collision`, stepping, and visitor-based diagnostics inherit it. Moving-piston and powder-snow/scaffolding bespoke paths are separate context paths; offset applies to any moved vanilla state's own base shape only if vanilla does so.
- Interaction override: `clip_with_interaction_override` clips `interaction_shape(state)` against the same `block_pos`; current supported override is hopper-only and not among these 110. Keep its vanilla semantics separate; shape offset must apply to the relevant outline and any future offset interaction shapes, not blindly to hopper-only hand overrides.
- Other state-only reads found: `physics/movement.rs` uses `partial_shape(above)` only to test emptiness for fluid movement, `particle.rs` scatters particles over `outline_shape(state)`, and renderer lighting/shadow helpers inspect state shapes. They do not currently convert boxes to world AABBs; avoid introducing position-dependent shape APIs into unrelated state-only predicates/rendering without proving those paths need the translated geometry.
- Tests to add with implementation: compare canonical shape and offset-aware world boxes at two distinct `BlockPos` for one XZ plant, bamboo, and one spike block; verify y coordinate does not affect seed, x/z do; cover `leaves=large`, up/down tip, report normalization after its position is known; verify raycast and collision collection use same calculated offset and retain default no-offset behavior. State-table tests should assert only canonical state geometry and preserve the 26.2-specific property layout.

## Verification record / restrictions

Read `Client/AGENTS.md`, `third_party/SteelMC/AGENTS.md`, ponytail skill instructions, `tools/blockgen/src/main.rs`, `shapes-26.2.json`, Steel's extracted `build_assets/blocks.json`, `evidence/all-state-shape-comparison.json`, `evidence/all-state-shape-summary.json`, and relevant collision/raycast callers. Used read/search and Python JSON inspection plus `javap`; no build, test, check, commit, or runtime code modification was performed. Only this requested audit-plan document was written. Exit codes: Python/Javap inspection commands succeeded (0); initial `javap` through PATH failed because javap was not on PATH (127), then absolute JDK javap commands succeeded; an exploratory Windows `for %C` command failed under bash syntax (2) and was replaced by Python subprocess calls. `shape643d385` report identity and all-state report reference `BlockPos` remain unverified.
