# Held equipment rendering plan (ArmorStand first)

## Scope and source of truth

確定している未実装は、ArmorStand の `Mainhand` / `Offhand` item mesh を world draw に接続すること。頭装備の欠落原因は別調査であり、本計画の根因扱いにしない。公式 `minecraft-26.2-decompiled` の `ItemInHandLayer` / `ArmedModel` / `ArmorStandRenderer` / Humanoid model を基準にし、対象は ArmorStand 両手を最優先、同じ Humanoid arm attachment と少量の分岐で成立する通常 Humanoid (`Player`, `Mannequin`, Zombie/Skeleton 等、公式 armed layer 対象の既存 `EntityKind`) も共通経路に含める。公式の layer 対象外の mob は追加しない。

非対象: Villager 系 `CrossedArmsItemLayer` は腕交差専用 transform なので別計画。CustomHeadLayer は既に `renderer/mod.rs` の専用 global item/skull 経路がある。古い `entity_renderer.rs` の TODO (頭/held layer 未実装) は頭経路については stale。頭防具全欠落の根因分析は独立残件。

## 既存経路とデータ

- `app/phases/in_game.rs::armor_stand_render_infos` (7151) は `armor_stand_equipment`, `armor_stand_pose`, `armor_stand_flags`, invisibility、small stand 用 `body_transform` を既に保持。`EntityStore` は `EquipmentSlot` map を管理。
- `renderer/mod.rs` の `Renderer::render_world` 準備時に `frame_entities` の armor layers と HEAD 装備を解決し、`ItemRenderInfo` を `item_entities` に積む。頭は `EntityRenderer::custom_head_attachment` + `DisplayResolver("head")` を使用し skull は dedicated draw にして item mesh の二重提出を避ける。world draw は `ItemEntityPipeline::draw` を再利用する。
- `renderer/pipelines/entity_renderer.rs::EntityRenderer::custom_head_transform` が既存の `entity_matrix`, `compute_anim`, `compute_part_transforms` を利用する。ArmorStand animation は `armor_stand_pose` を反映し、part transforms には Humanoid model convention の座標変換が入る。
- `renderer/pipelines/item_display.rs::DisplayResolver` は model/item JSON と親 chain を辿って `display.<key>` を解決、translation は 1/16 block 換算、結果は `DisplayTransform::to_matrix()`。`renderer/pipelines/item_entity.rs::ItemRenderInfo` は model matrix を受け、既存 pipeline が mesh/texture, lighting, overlay/glint の対応済み機能を描画する。

## 実装案

1. `entity_renderer.rs` に小さな arm attachment API (例 `held_item_attachment(info, anchor, logical_slot, main_hand_right) -> Option<(Mat4, display_key)>`) を追加。公式 `ArmedModel` の左右arm part を見つけ、ArmorStand は show-arms flag `0x04` がなければ hand draw を省く。公式 `ArmorStandRenderer` の invisibility/small handling と同じ条件にする。既存 entity root + 計算済み arm part transform から attachment を得る。通常 Humanoid は同じ左右arm part path を利用。
2. attachment は item mesh が中心原点に bake 済みの現行規約で返す。公式 `ItemInHandLayer` の arm-local translate/rotate (including arm-side sign and vanilla item orientation) をここで合成し、display transform はその後。Final: `entity_root * arm_part_pose * vanilla_hand_item_pose * display.to_matrix()`; root includes yaw and `body_transform` exactly once. Arm/pose transforms use model-space block units converted consistently to block coordinates by model convention. Small ArmorStand scale/translation stays in `entity_root`/`body_transform` once; never rescale attachment separately.
3. Y-up world/block conventionを保ち、独自Y反転は禁止。既存 `CustomHead` の skull-only flip が ItemEntityPipeline でさらにYflipされた過去の二重反転を回帰させないため、held attachmentはarm model convention由来の行列のみとし、head/skull helperを流用しない。display transformは既に local display semantics を適用するので別途 item matrix flip を加えない。
4. `renderer/mod.rs` の entity preparation loop に held slots を抽出する局所関数を追加し `EquipmentSlot::Mainhand/Offhand` を空でない場合だけ item name/profile/dye 等既存 item resolverで解決、`ensure_item_mesh` 後 `ItemRenderInfo` を既存 `item_entities` vectorへ push。`display` resolver key は物理 arm sideに応じ `thirdperson_righthand` / `thirdperson_lefthand` (logical Mainhand は `main_hand_right` と offhand で反転) を用いる。同じ手の slot/item pairを二度追加しない。既存 head processingの後段でも同じ equipment stack全体を scan しないよう slotを二つに限定する。Invisibleは ArmorStandRenderer の invisibility policyに従い equipment rendering可否を決め、ordinary invisible entityの既存装備方針を壊さない。
5. Item pipelineの texture lighting/white overlay/glintは通常 `ItemRenderInfo` + `ItemEntityPipeline::draw` に委譲する。追加の Vulkan pipeline/renderer は作らない。unsupported special model/context (shield blocking/use pose, trident/bow use animation, profile等) の能力は既存 resolver/pipelineの実装範囲に限定し、held item generic layerが第一人称 `HeldItemPipeline` 全能力を得るとは主張しない。profileは item draw path既存データを使える場合だけ維持する。

### Portal変更競合の回避

`renderer/mod.rs` がportal担当と競合しうる。主要な変更は `entity_renderer.rs` に attachment helperを閉じ、world entity list生成の短い箇所だけ `mod.rs` に追記する。共通の render pass / portal traversal / `render_frame` / global pipeline所有権を触らず、既存 `item_entities` に追加して `ItemEntityPipeline` を共用する。競合時は変更直前にファイルを再読し、portal変更と同一regionの広い置換を避ける。

## 最小 production tests

`entity_renderer.rs` 内 testsで生産用 attachment helperを直接検証: (a) default ArmorStand right/left arms が左右逆にならず slot + handednessの対応確認、(b) `0x04` false はNone/skip、trueは描画可能、(c) arm pose rotationがattachment matrixへ反映、(d) small root scaleが一度だけ乗り scale成分が通常の1/2、(e) entity位置/yaw/body poseとの合成順が期待点/軸へ変換、(f) 行列finiteかつY-up (単位Yを不意に反転しない)。world extraction testはMainhand/Offhand各1件、empty slot skip、armor/head stack非重複を確認。数値期待値は公式part pivotsと既存 `custom_head_attachment_matrix` test作法に基づく。

## 実装順・検証

1. 公式上の attachment local constants / renderer small+invisible ruleを再確認し、test付き pure transform helperを追加。
2. ArmorStand両slotを world `ItemRenderInfo` 抽出へ接続、mesh warmingと既存 display resolveを再利用。
3. 表示確認後、通常Humanoidの公式 armed targetを同じ helperへ通す。Villager系を混ぜない。
4. `mise run check`、関連する `entity_renderer` / render extractionテスト、最後に `mise run test`。ゲーム内では通常/小型/pose付き/ShowArmsなし/Invisible/left-handed standの両手、通常player/mob、頭装備を比較し、座標/二重描画/glint/textureを確認する。

## 別件・非保証

頭防具全欠落は別途 trace: CustomHeadLayer item-vs-armor predicate、armor resolver/equippable texture layers、item mesh/texture/model rootを切り分ける。special item modelの全条件、shield/trident/bowの使用中ポーズ、custom profileなどは今回の一般hand attachment実装が自動的にサポートするとはしない。全itemを独自rendererで再実装しない。
