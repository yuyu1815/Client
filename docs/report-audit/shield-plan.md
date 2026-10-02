# ITEM-shield 描画差: 原因確定メモ

調査のみ。対象 `Client`。共有コード変更なし（settings agent の `held_item/hand/left-hand` 作業と競合させない）。build/test/check/commit/runtime repro は実施していない。

## 結論

観測差（実在 shield がhotbar iconと一人称で描画されない）の直接原因は、26.2の `minecraft:shield` が `minecraft:special` renderer modelを使うのに、現在の item bake がその special rendererを一般的に解決せず、さらに本アイテムは fallback として期待される `item/shield` flat spriteも持たないこと。

- 26.2 `assets/minecraft/items/shield.json` のrootは `minecraft:condition`（property `minecraft:using_item`）。false/true両方のleafが `minecraft:special`、`model.type=minecraft:shield`。baseは `minecraft:item/shield` / `minecraft:item/shield_blocking`。
- 同名 `assets/minecraft/models/item/shield.json` と `shield_blocking.json` はparticle texture `block/dark_oak_planks` とdisplay transformsだけを定義し、`elements`も `textures.layer0` も持たない。これらは盾のplate/handle meshやアイコン画像ではない。
- `BlockRegistry::load` → `model::bake_item_models` (`pomme-client/src/world/block/{registry,model}.rs`) は `collect_model_parts` が `composite` と `model` nodeのみ実装し、condition/special leafを含むfallbackは `first_item_model_ref`の最初のmodel string/baseを一つだけ取る。special renderer type=`shield`はbakeされない。`item_definition_is_static`もmodel/composite以外はfalseだが、false時にも代表leafのmeshをbakeする。Shield用condition選択/描画は実装していない。
- 選ばれたbase modelにelementsがないため、通常のflat modelにもならず `flat_item_texture_keys`にshield icon keyが登録されない。`Renderer::ensure_item_mesh` (`renderer/mod.rs`) はregistry 3D modelがなければ `item/{name}` をflat key fallbackにするが、jarに `assets/minecraft/textures/item/shield.png` はなく、使用可能なshield artworkはspecial rendererが使う `textures/entity/shield/...`。従ってmeshが実質空になり、GUI atlas bakeもheld drawも頂点を描けない。
- GUI経路は inventory slotで `item_resource_name(ItemKind::Shield)`→`shield` の `MenuElement::ItemIcon` を発行し、`Renderer::render_frame`で `ensure_item_mesh`後、`GuiItemPipeline::bake_to_slot` がatlasへbakeする。`SpriteId::EmptyShield` は別途 `ui/inventory.rs`のoffhand empty slotに渡す背景で、実itemのiconには使われない。
- Held経路は `app/phases/in_game.rs`でmain/offhand stackが `Renderer::render_world`へ渡され、`ensure_item_mesh`、`HeldItemPipeline::update_and_draw` (`renderer/pipelines/held_item.rs`) がitem name/meshでdrawする。現在mesh選択special caseはbowのみ。盾blocking用mesh/pose分岐なし。ただしbase `models/item/shield*.json`のdisplay transform自体は `DisplayResolver`が読める。meshがないため transformでは症状を直せない。

ゆえに確定点は**atlas/GPU描画器が shield を途中で偶発的に取り落とすのではなく、その上流でspecial shield modelをbakeするmesh/textureがそもそもregistryに生成されない**こと。これは両症状に共通する原因。atlas slot allocation/frame GPU結果はruntimeで確認していないが、コード経路に実meshがないことで両pipelineが出力できないのは静的コードから確定する。

## 実jarの確認

対象 `C:/Users/yuzum/AppData/Roaming/.minecraft/versions/26.2/26.2.jar`。

| Jar path | 内容 | 重要な寸法/値 |
|---|---|---|
| `assets/minecraft/items/shield.json` | `condition(using_item)`→special shield (normal/blocking) | base refs: `item/shield`, `item/shield_blocking`; `scale=[1,-1,-1]` |
| `assets/minecraft/models/item/shield.json` | regular display/particle data only | FP right rotation `[0,180,5]`, translation `[-10,1.75,-10]`, scale `[1.25,1.25,1.25]`; FP left `[0,180,5]`, `[10,0,-10]`, same scale. GUI `[15,-25,-5]`, translation `[2,3,0]`, scale `.65` |
| `assets/minecraft/models/item/shield_blocking.json` | same pattern, blocking display transforms | FP right `[0,180,-5]`, `[-15,3.25,-11]`, scale `1.25`; left `[0,180,-5]`, `[5,5,-11]`, scale `1.25` |
| `assets/minecraft/textures/entity/shield/shield_base.png` | patterned/default shield entity-layer texture | PNG 64x64 (paletted) |
| `assets/minecraft/textures/entity/shield/shield_base_nopattern.png` | no-pattern shield entity-layer texture | PNG 64x64 (paletted) |
| `assets/minecraft/textures/gui/sprites/container/slot/shield.png` | empty offhand placeholder | separate GUI background, not item icon |

26.2 shield item definition's item-selected model leaves refer to **special-renderer type shield**. The texture assets enumerate `entity/shield/base.png`, `border.png` etc. plus `shield_base(_nopattern).png`; resource pack banner patterns are a further dynamic rendering concern, not needed to prove plain shield mesh absent. A minimal plain shield implementation should deliberately state whether banner-pattern layers are out of scope rather than silently claiming patterned shields match vanilla.

### Vanilla plate/handle shape and mesh reuse

Reference geometry verified in public `ShieldModel` mapping/source (Minecraft Java 1.21.8; not asserted byte-for-byte decompile of the local 26.2 jar): model layer texture 64x64, two ModelPart cuboids:

- `plate`: texOffs `(0,0)`, origin `(-6,-11,-2)`, size `(12,22,1)` model pixels.
- `handle`: texOffs `(26,0)`, origin `(-1,-3,-1)`, size `(2,6,6)` model pixels.

This project already has vanilla cuboid/box-unwrapped UV primitives in `renderer/entity_model.rs`: `ModelCube`, `cube_faces`, `generate_cube_vertices` (texOffs and dimensions are in pixel units; entity UV sheet dimensions passed explicitly). These can provide CPU quad geometry, but `ItemEntityPipeline` still needs correct shield atlas texture region and item vertex/mesh upload. Alternative: special-case in `world/block/model.rs` producing `BakedModel` quads directly in the existing item pipeline's model units/UV sheet. Do not bind `textures/gui/sprites/container/slot/shield.png` as the item art. Plain normal/blocking special model has same base geometry; blocking state chiefly selects `shield_blocking` display transforms, and source item definition carries special node transformation scale Y/Z negative.

## Condition/selector, transforms and states

- Current `collect_model_parts` fallback is lossy by design: one representative reference for all non-composite selector trees. For shield `minecraft:condition` leaves special nodes; nested special model's `model.type` is `shield`, not a vanilla block/item model JSON to tessellate.
- `DisplayResolver` (`renderer/pipelines/item_display.rs`) loads `items/<name>.json`, gets `first_item_model_ref`, walks model parent chain and resolves `display.<key>`. `HeldItemPipeline` already maintains separate `firstperson_righthand` and `firstperson_lefthand` resolver; `GuiItemPipeline` already resolves `gui`. The direct shield / shield_blocking model JSON has useful transforms as above. Do not assume the resolver implements item selector choice: it does not evaluate `condition(using_item)` or select `shield_blocking` during blocking.
- `held_item::selected_item_model_name` currently only selects `bow_pulling_0..2`; shield state is not represented in `HeldItemInfo` or the renderer call. This is the one item-specific state connection a shield pose change will need, beyond the shared hand-side mapping being modified by settings agent. Reuse those resolved transform maps where convenient, but pass correct blocking state/model transform explicitly; don't add a competing left-vs-right hand API in this task.
- Existing special-model precedent: `model::bake_item_models` conditionally synthesizes builtin quads only for player_head, conduit, copper_chest, shulker boxes, trapped chest. `BakedModel` -> `ItemEntityPipeline::ensure_mesh/ensure_flat_mesh` -> shared GUI bake / first-person mesh is the closest reuse path for unpatterned geometry. `block_entity_model.rs` / `entity_model.rs` special box emitters establish cuboid UV/quad methods, but their GPU block/entity pipelines are not directly reusable as a held shield mesh; item mesh and atlas registration expected by both draw paths are reusable.

Vanilla 26.2 transforms available for first-person pose:

| state | right-hand transform (rot°, translation in 1/16 units, scale) | left-hand transform |
|---|---|---|
| idle shield | `(0,180,5)`, `(-10,1.75,-10)`, `(1.25,1.25,1.25)` | `(0,180,5)`, `(10,0,-10)`, same |
| blocking shield | `(0,180,-5)`, `(-15,3.25,-11)`, same | `(0,180,-5)`, `(5,5,-11)`, same |

Those rotations/translations are from model display JSON. Root special model also includes `scale (1,-1,-1)` transformation; existing ordinary model mesh centering and display transform path do not currently apply selector-root transformation. Check the intended baked vertex convention against local `build_item_mesh` / `DisplayTransform` before adding it (avoid double/missing flip). Existing root-model transform parsing can potentially apply it at bake time.

## 原記録と範囲境界

- Source record: `records/ITEM-shield.json`, report `reports/items/shield.md` in `C:/Users/yuzum/Downloads/Pomme_0a1aee9_全文と対象別詳細報告/Pomme_0a1aee9_full_report`.
- Pomme shield inventory count 1 on all 3 runs; icons/held render absent by 3 recorded screenshots; held durations 2.129–2.143s. Fabric reference had shield in inventory and `usingItem=true` in captured official samples.
- Record/report classify **render/display difference**. Attack damage blocking, direction coverage, durability damage, active-state packet correctness are explicitly untested there. Do not claim that generated model/pose makes defense work; damage prevention requires separate authoritative server behavior/repro.
- Original UI screenshot was not reopened/reanalyzed by this investigation; conclusions do not assume any server damage result.

## 次の実装担当用prompt

> `docs/report-audit/shield-plan.md`と`docs/report-audit/item-movement-plan.md`を読み、settings agentが進めている`held_item/hand/left-hand`の共有変更に触れずにITEM-shieldの描画だけを実装する。根本原因は26.2の`items/shield.json`が`condition(using_item)`→`special(type=shield)`を選択する一方、`bake_item_models`がspecialをbakeせず、special base `models/item/shield*.json`にもelements/layer0がない点。`EmptyShield`は空slot背景のため変更対象にしない。まず`BakedModel`+`ItemEntityPipeline`+既存GUI atlas/held drawの経路を再利用し、盾plain plate/handle geometry (texture 64x64、plate texOffs 0,0 origin -6,-11,-2 size 12,22,1; handle texOffs 26,0 origin -1,-3,-1 size 2,6,6; model source referenceは1.21.8)を通常・blocking共通meshとして追加する。26.2 texture assetsの`entity/shield/shield_base(_nopattern).png`をasset resolver/atlasで実際に引けることを確認し、存在しない`textures/item/shield.png`をfallbackにしない。item selector special rootのscale flipとbaseのdisplay transformsを二重適用しない。`DisplayResolver`の既存gui/right/left transformsを使い、blocking時にはshield-specific blocking transformを選べる最小の state inputを接続する。selectorsによる使用中mesh/pose、右/左hand両方とGUIでの実表示を小さく回帰テストし、plain shield以外（banner pattern、damage reduction）は未対応と明示する。盾の防御挙動はこの描画修正で実装・検証済みと主張しない。

## 推奨最小tests

コード変更時に追加するheadless単体tests（今回は実行しない）:

1. `world/block/model.rs`: 26.2 shield item JSON fixtureをbakeし、idle special shieldのmodelが空でない、少なくともplate/handleの面群が存在し`flat_item_texture_keys["shield"]`に誤った`item/shield`を登録しない。`condition(using_item)` both branchesからmodel type/baseを認識し、normal/blocking transformを取り違えない。
2. `renderer/pipelines/item_display.rs` or held_item tests: `shield` と `shield_blocking`のGUI/right/left display JSON fixtureのtranslation/rotation/scaleを、上表の値で解決する（translationがresolverで16分の1化されることもassert）。
3. `renderer/pipelines/held_item.rs`: shield active=false/trueでblocking resolver/pose selectionが切替わり、right/left logical-hand variants両方で同じ選択規則を保つ。settings agentのhand-side変更と重複したside mappingを作らない。
4. Existing item geometry generation test: plate/handle face count・bounds・UV ranges against 64x64 sheet; tex-offets/dimensions and `texture` key are asserted. This catches unsupported geometry/UV assumptions without runtime/GPU test.
5. Integration smoke test is still needed after code: GUI atlas icon + first-person idle + held-right-button blocking compare against official 26.2. No test should assert damage blocked unless separate server-authoritative protection path is exercised.

## 実施した確認

- Read: `Client/AGENTS.md`, ponytail `SKILL.md`, `docs/report-audit/item-movement-plan.md`, external `records/ITEM-shield.json`, `reports/items/shield.md`.
- Inspected code: `pomme-client/src/world/block/model.rs::{bake_item_models,item_definition_is_static,collect_model_parts,collect_parts_from_node}`, `world/block/registry.rs::{get_item_model,get_flat_item_texture_key}`, `renderer/mod.rs::{ensure_item_mesh,render_frame}`, `renderer/pipelines/{gui_item.rs::bake_to_slot,held_item.rs::update_and_draw,item_display.rs}`, `renderer/entity_model.rs::{ModelCube,cube_faces,generate_cube_vertices}`, `ui/inventory.rs`, `player/inventory.rs`.
- Read actual jar with commands:
  - `unzip -p /c/Users/yuzum/AppData/Roaming/.minecraft/versions/26.2/26.2.jar assets/minecraft/items/shield.json` → condition special shield branches and `using_item` property.
  - `unzip -p ... assets/minecraft/models/item/shield.json` and `shield_blocking.json` → display transforms and only particle texture.
  - `unzip -l ... | grep ...shield...` → no `textures/item/shield.png`; listed the two 64x64 `textures/entity/shield/shield_base*.png`, `gui/.../slot/shield.png`, and shield pattern textures.
  - `unzip -p ... textures/entity/shield/shield_base.png | file -` and nopattern equivalent → both `PNG image data, 64 x 64, 8-bit colormap`.
  - Web search for vanilla `ShieldModel` geometry: mappings.dev Minecraft 1.21.8, plate+handle cuboids cited above. This confirms a vanilla reference but **was not verified against 26.2 class bytecode**.
- No build/test/check/commit/game runtime or image review was run (per request). `python`/`python3` jar listing attempts failed with exit 49 due Windows shim; `tar -tf` rejected the jar as tar (exit 1), then `unzip` was used successfully. `cmd.exe /c jar...` returned without listing entries; no claim based on that attempt.
