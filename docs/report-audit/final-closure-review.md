# 全writer完了後・最終独立確認（read-only）

対象repo: `C:/Users/yuzum/Desktop/mine_rust/Client`
確認HEAD: `c7dc8c3f9c6c52ddbd143dc7c599293458677d77` (`Implement 26.2 offset block shapes`)
確認方法: Git履歴/status、Rustソース検索・読取、既存JSONのread-only解析、26.2 decompiled source読取。source/build/test/check/format/commitは実施していない。

## Findings

### P1（最終test前に直す）— offset raycast追加テストのray座標がshape外で、テスト実行時に失敗する

- `pomme-client/src/physics/block_shape.rs:1387-1403` `world_offset_translates_outline_raycast_and_collision_boxes` のlarge-leaf bamboo境界テストで、`edge_ray` のxを `block_pos + offset.x + 0.95` としている。raycast側 `interaction.rs:2435-2440` はcanonical outlineに同じoffsetを別途適用するため、ray相対shape座標は0.95となる。
- 同じソースの`canonical_plant_shape`（`block_shape.rs:130-145`付近）とテスト自身のshape assertion（1320付近）より、large bamboo outlineはx `[0.1875, 0.8125]`。0.95は上限0.8125を超え、`raycast(...).unwrap()`はhitせずpanicする。位置`(-8,64,3)`で公式Mth演算を再現すると`offset.x=0.21666666865`、現テストrayの相対xは1.16666666865、offset後shape上限は1.02916666865で、差は0.1375。
- Mojang 26.2 `BambooStalkBlock.java:41-43,73-85` でもlarge outline幅10/16、collision幅3/16、`state.getOffset(pos)`をmoveする。実際に隣接セルrayで確認するなら、同じseed条件（offset.x > 0.19）の下で `x = edge_pos.x + 1.0`（隣接セル側・shape内）を試す。現行テストは実行していないが、座標と箱境界から失敗が静的に導ける。
- 必要最小fix: テストの`edge_ray.x`から`offset.x + 0.95`をやめ、隣接セルかつtranslated-box内となる`edge_pos.x as f64 + 1.0`等へ修正し、後続の正式testで実行確認する。ここでは修正しない。

### P2 — offsetがraycast/collisionへ入ったが、描画メッシュには入っていない

- `world/block/mod.rs:882-907` `block_offset(state,pos)` は26.2固有のoffsetを返す。`physics/collision.rs:11,143-152` はoffset込みworld AABB、`player/interaction.rs:2218-2220,2435-2440` はoffset込みoutlineを使い、neighbor ray候補も探索する。公式 `BlockBehaviour.java:678-694` と `Mth.java:356-359` のseed/式に沿い、`x*3129871` のint overflow後cast、z/長式のlong wrapping、符号付き`>>16`、float division、H clampの順もコードと一致する。負座標・y非依存・H境界テストがある。
- ただし `renderer/chunk/mesher.rs:2028-2054` のblock位置はsection-local originのみで、通常mesh生成に`block_offset`を足していない（renderer側に`block_offset(...)`のcallなし）。該当22 block groupsの葉/花/bamboo/dripstone/spikeは、ray/collision位置と描画位置がずれる。
- 必要最小fix: メッシュのblock-position→model-vertex配置境界で`block_offset(state, actual BlockPos)`を適用し、section bounds/culling・remesh/edit経路の影響を確認する。今回の順序指定どおり最適化はせず、まず同一位置の正しさを検証する。

### P2 — living `flying` moduleもMobDef描画callerがなく、nonliving 3 modulesは未登録

- `renderer/mod.rs:6-11` は`aquatic/flying/humanoid/terrestrial`だけを登録する。`vehicles.rs`、`projectiles.rs`、`nonliving_special.rs`はmodule宣言もproduction callerもなく、`entity_renderer.rs::mob_definitions`からのbake呼出もない。ファイル内geometry/testはcrate経路から除外される。
- `flying.rs`は登録されコンパイル対象だが、全repo検索で`entity_models::flying::` callerは0件。従ってmodule内で書いた飛行mob geometryは実描画されない。現状`mob_definitions()`には78個の`MobDef` kind entryがある（read-onlyソースカウント）。表の存在やnonempty bake testだけでは、全モデルが公式geometryで実描画される根拠にならない。aquatic/humanoid/terrestrialは現行MobDef内に実callerがあり、参照元はそれぞれ8/23/27か所。
- 前reviewの指摘は**3 unregistered moduleについて有効のまま**。加えて、前reviewの「flying actual caller」記述は現在のソース検索結果と一致しないため訂正する。
- 必要最小fix: runtime対応を主張するmobのみ、対応entity kind/variant/texture/metadataと実bakeを`MobDef`へ結線し、production caller確認を追加する。順次対応を採らないモデルは未対応と数える。

### P2相当の未完了UI（server順・候補button orderは現実装でまだ扱わない）

- **Crafter:** `ui/special_container.rs:35-45,98-103,155-166,206-217`と`player/menu_click.rs:39-45,68-83`は46 slot、入力0..8、player9..44、readonly出力45、hotbar36..44、結果クリック抑止、disabled mask/button id 0..8に一致。46枠のunit assertionsあり。`in_game.rs:4555+`共通経路は`result.button`をcontainer id付き`ContainerButtonClick`として送る。静的確認では前指摘のslot-count不整合は解消。
- **Loom:** `special_container.rs:125-128`はslot0-2 input/slot3 result + player slotsのlayout、ただしUIには候補pattern listも候補クリック処理もない（button生成はCrafter専用、206-208）。`recipe_id: None`。したがって候補のbutton orderや26.2 selectable-pattern listとの一致は未実装であり、順番が正しいとは確認できない。loom pattern selectionを完成扱いしない。
- **Stonecutter:** `special_container.rs:124`はslot0/1だけを描画し、recipe candidate/result list・recipe id selectionは作らない（`recipe_id: None`）。server-provided slot stateは表示できるが、server recipe order/button mappingは実装されていない。Loom同様、完成扱いしない。recipe-selection追加時は26.2 server list順を入力item/tagごとに保持し、そのlist indexを送る必要がある。
- これらは「現行スロット描画経路の即時バグ」ではなく、候補選択機能の未実装/未source確認として残す。現在のSpecialMenuコメントも「selectable recipe orderを合成しない」と明記する。

### P2 — resource-pack UIで対応済み表示に見えるcape/skin設定は実動作していない

- 現行`net/mod.rs:791-804`はClientInformationのskin customizationを全true、main handをRight、particle statusをAllとして送る。`ui/menu/options.rs:1004+`はcape/jacket/sleeve/pants/hat controlsをdisabledにしている。`skin_main_hand_right`の保存値はHUD攻撃indicator位置用で、protocol main_hand切替には未接続。particle optionsもstub/constant。
- 以前のfinding有効。これはresource-pack apply/restore/order修正とは別の未実装settings。表示/保存だけで機能完成と数えない。最小対応までは未実装として明記。

## 現HEADで配線を確認した実装（コード経路のみ。実行・実機検証ではない）

| Scope | 現HEADの主な呼出・データ経路 | 判定/残り |
|---|---|---|
| Resource pack | `ui/menu/options.rs` selection保存・reload request → `resource_pack.rs` local/server active order (`active_pack_dirs`, `apply_server_pack`, `remove_server_pack`) → `world/block/registry.rs` / `world/block/model.rs` models, item definitions, textures with packs → atlas/renderer reload. | local selection restore/order and server/local pack update path present. Filesystem/zip/network pack reload実行・競合優先順位の実機結果未確認。 |
| Spyglass/invert mouse | `interaction.rs::ActiveUseKind::Spyglass/is_using_spyglass` → `core.rs::compute_fov_modifier` (spyglass multiplier 0.1) / `in_game.rs` `hud::build_camera_overlays` scope; camera reads saved invert-mouse option; option toggle calls settings save. | source path present. player/renderer runtime-frame test未実施。speed FOV still TODO attribute path. |
| Command editor | interaction ordinary UseItemOn/pending position → `app/core.rs` matching timely same-position command-block NBT response/block id → `ui/command_block.rs`; save via `app/phases/in_game.rs` → `net/sender.rs::set_command_block` native packet encoder. | native only protocol guard is wired; actual server permission/roundtrip未確認。 |
| Shield | `world/block/model.rs::bake_shield_item_model` special mesh + `pipelines/item_display.rs` pack-resolved normal/blocking item bases + held-item blocking pose gated by active-use/hand state. | mesh/call path present; patterned/banner component and GUI/first-person GPU display not verified/unsupported. |
| Campfire | lit campfire block-entity data/model/particle branch from world block entity/render path; prior review confirmed lit check/emission link. | source only; particles/GPU/display not run. |
| Banner | block-entity classifier recognizes color standing/wall banners (`world/block_entity.rs:386-433,486-495`); `mesher.rs` handles standing/wall banner bases; color classifier test exists. | base routing present; runtime image comparison unrun. |
| Night/lighting | `renderer/pipelines/sky.rs` day/night keyframes/cloud color; block light/state/light engine tables in `world/light/*` and block light properties. | path source reviewed; screenshots/time-dependent visual and lighting simulation not run. |
| Item/use effects | `interaction.rs` active-use state, short button edge latch, UseEffects speed/sprint and packet flow; `app/phases/in_game.rs` ticks state and `physics/movement.rs` consumes movement effect. | relevant tick/unit tests exist but not executed. |
| Physics/offset | `physics/movement.rs` tick resolver; `physics/collision.rs` offset-world AABB; `interaction.rs::raycast` translated outline and neighboring X/Z cells; offset seed in `world/block/mod.rs`. | offset translation logic conforms to 26.2 inspected formula, but P1 test ray is wrong and renderer model offset is absent. Actual movement/collision runtime and GPU alignment remain unverified. |
| Translucency/edit trace | `mesher.rs::edit_state_supported/edit_model_quads_supported` rejects partial-alpha or non-unit faces for fast edit; `edit.rs::EditOverlay::admit` drops rejected stale overlay; `app/core.rs` enqueues dirty remesh. Mesh list trace order regular(solid+cutout) → water → translucent, starts adjusted by `trace_index_start`; buffer upload trace/hash follows list concatenation order in `buffer.rs::ChunkBufferStore::upload_batch`. | source/data path matches previous review; hash/upload offsets not GPU validated; tests not run. |

## Shape data, generator/runtime validation, official-source comparison

- Existing embedded `shapes-26.2.json` parses: version `26.2`, **1,196 blocks / 32,366 states / 799 dictionary shapes / 16,980 overrides; 32,211 verified + 155 skipped (45 context + 110 offset) + 0 oracle disagreement**. This only reports pre-existing generated-oracle status; the 110 Java-source ported shapes are not newly oracle/report-position verified. `shape-offset-plan.md` explicitly says report sample BlockPos unresolved and position-included report comparison is 0/110.
- Runtime `world/block/mod.rs::validate_shape_boxes` checks finite coordinates and min<=max; `build_table` verifies protocol/version, state count, block count/order/name/first ID/props, override index order/range, dictionary shape index existence, shape coverage/skipped counts. It intentionally does not enforce all boxes inside [0,1], because official voxel shapes such as walls extend beyond a block cell. Runtime `None`/empty semantics remain distinct.
- `tools/blockgen/src/main.rs::gen_shapes` gates 26.2, checks comparison length, block count/identity/density, property names/defaults/order/state identities/status, `usesOffset` is required boolean, overwrite offsets unique/in-range, dictionaries finite/3D/non-inverted, plus comparison agreement. Shape JSON is read-only parsed here; no generator roundtrip executed.
- Mojang source checked directly: `BlockBehaviour.java:678-694` (offset source, XZ and XYZ), `Mth.java:356-359` (seed and x int arithmetic), `BambooStalkBlock.java:41-43,73-85` (canonical shape and runtime offset application), plus `Blocks.java` offsetType registrations. `block_offset` matches the inspected XZ formula, including **no multiplication by H** in X/Z expression; clamp is to ±H. 26.2 jar was present per task, but this review used source rather than running javap. Comparison report and oracle reference position not found inside Client checkout; exact sample-position normalization therefore remains unverified.

## Ledger, Git, scope and verification limits

- `coverage-ledger.json` is unchanged and still anchored at `auditedClientHead=914ebf2`. Node read-only parse: **793 records**: 636 `in_progress_not_complete`, 153 `not_marked_fixed`, 1 `not_established_not_a_confirmed_fix_target`, 1 old `committed_and_independently_reviewed_at_914ebf2`, 2 `implementation_committed_db06606_check_passed_runtime_test_unverified`. No record was changed to fixed based on this review.
- History: `origin/master..HEAD` = **57 commits**, branch `master...origin/master [ahead 57]`. Complete post-base range `0533a1f^..HEAD` is **58 commits** including `0533a1f`; merge-base resolves `b7a5dcbc9d1ad44544f415d7e76ac89d83dc9692`. `git status --short` shows **no tracked modifications**; 14 pre-existing untracked `docs/report-audit/*.md` plans/reports plus this allowed memo make **15 untracked markdown files**. They need commit by the next final担当, per user instruction; reviewer did not modify source or commit anything.
- `61e8cd9` parent/tree diff was rechecked: exactly four files (adds `aquatic-models.md`, `entity_models/aquatic.rs`; modifies `flying-models.md`, `entity_models/flying.rs`). No hidden extra tree file/content-loss evidence. No history rewrite/amend occurred.
- Commands and results: `git -C ... status --short --branch`, `log`, `rev-list --count`, `merge-base`, `show`, `diff --check origin/master..HEAD` — exit 0; count 57; diff-check no output. `node` JSON parse/counts and one JavaScript seed arithmetic sanity calculation — exit 0. `grep/find/read` searches — successful; shape-comparison report absent in checkout. First `git status/log` without `-C Client` failed exit 128 because bash starts in parent `mine_rust`; corrected commands used explicit `git -C`. No build, compile, test, check, formatter, runtime, GPU, source edit, or commit was run.
- **全体の793件調査を完了/修正済と扱わない。** この確認は現HEAD差分・実経路の静的レビューであり、下記とソース/実機未確認事項を残す。重大解消済/ゼロ残件とは報告しない。

## 最終担当へ渡す検証計画（ここでは実行禁止）

1. P1 test座標をwriterが直した後、`mise exec -- cargo test -p pomme-client --locked --profile dev-fast --no-run` でtest compilation、続けて `mise run test`（`.mise.toml`: pomme-client/protocol/singleplayer tests、`--no-fail-fast`）を実行。`mise run test`はcompileだけでなく実tests実行。
2. `mise run build`でclient dev-fast build（singleplayer enabled）。`pomme-client/build.rs`が各GLSL shaderをshadercでcompileし、`translucent.frag`等shader再生成/compileはこのbuildで確認される。最終testだけでなくshader include dependenciesもfresh buildでcompile確認する。
3. blockgenは上記`mise run test`対象外。`mise exec -- cargo test -p blockgen --locked --profile dev-fast`でunit tests。report/oracle/comparisonが揃う作業コピーで、generator roundtripは次の実引数形（出力はtempにし、source dataを上書きしない）を別途実行し、生成JSONを現行入力/embedded結果と比較する: `mise exec -- cargo run -p blockgen -- blocks <26.2-generated/reports/blocks.json> 26.2 <tmp>/blocks-26.2.json`; shapes: `mise exec -- cargo run -p blockgen -- shapes <blocks-26.2.json> <vanilla-oracle.json> <verified-comparison.json> <tmp>/shapes-26.2.json`. 当Client checkoutにshape comparison JSON/reference reportsは見つからず、これらの入力パスは実在確認後に指定する。比較ファイルなしではshape roundtripを完了扱いしない。
4. Runtime確認: display可能環境でGPU/Vulkan起動、translucent model (部分alpha) render/edit fallback/remesh、shader reload、offset plantsの描画位置 vs crosshair/collision、shield hand pose/spyglass FOV scope、pack reload、special menus、command editor/サーバー同期を個別に確認。特にmenu candidate UIは現状未実装なので、実装前にrecipe-orderテストをpassした扱いにしない。
5. `mise run test-registry`はこの変更範囲の必須網羅を保証しないが、Steel参照補助を検証するなら別途実行。実機/GPU未実施とsource未確認は成功ビルドと分離して報告する。
6. 全未追跡plansをcommitする要否はmain担当へ引き継ぐ（除外不要との指示）。working treeがcommit直前にcleanか再確認し、このレビューmemoもコミット対象とする。
