# Held equipment rendering plan (ArmorStand first)

## Scope and official behavior

未実装の中心は ArmorStand の `Mainhand` / `Offhand` を item mesh の world draw に接続すること。公式根拠は `minecraft-26.2-decompiled`（26.2）を固定基準にする。ArmorStand 両手を先に正しく実装し、通常 Humanoid は既存 `EntityKind` が公式 `ItemInHandLayer` 対象で、腕pose・main-arm情報も取れるものだけ同じ対象判定を通す。全 mob に一般化しない。Villager 系 `CrossedArmsItemLayer` は腕交差専用、CustomHeadLayer は別経路。頭防具の欠落根因は本計画外。

`ItemInHandLayer.java:23-60` は RIGHT/LEFT腕の非empty itemだけを提出し、`translateToHand` 後に `Rx(-90°) * Ry(180°) * T((left?-1:+1)*x/16, y/16, z/16)` を適用する。通常値は `(x,y,z)=(1,2,-10)`、baby offset対象では `(0,1,-4.5)`（同ファイル:44-48,63-65）。ArmorStand はbaby offset対象外。`HumanoidModel.java:462-469` のtranslateはroot、次にarmの `translateAndRotate`。ArmorStand は `ArmorStandRenderer.java:38` でもこの同じlayerを使う。`ArmorStandModel.java:58-83` はShowArmsでarm geometryの `visible` を変えるが、`translateToHand` がその値を一時保存して `true` にし、super translate実行後復元する。したがって `ShowArms=0x04` は木腕の可視性だけで、held item skip guardではない。held stackがあれば腕meshが非表示でも描画する。

公式 `LivingEntityRenderer.java:74-108` はbodyがinvisibleでもlayer呼び出しを別途抑止しない。ItemInHandLayerの条件もitemがnonemptyかだけ（39行）。従ってinvisibleを held item skip条件にしない。ArmorStand基本経路にspectatorはない。現在のworld item抽出が他entityで `is_spectator` をskipするならその既存方針は維持する（ArmorStandのguard根拠ではない）。ArmorStand `isSmall` はrendererがsmall modelを選択し (`ArmorStandRenderer.java:68-70`)、Client側の `body_transform = scale(0.5)` は `entity_matrix` 末尾に一度だけ適用されるため、attachment側で再scaleしない。

## 行列・座標と左右

`EntityYDown` modelの `compute_part_transforms` はpivotのY rebase/mirrorと角度共役を済ませ、rootにX flipを含む（`Client/pomme-client/src/renderer/entity_model.rs:171-215`）。そのためVanilla手元操作を単純にそのまま掛けるのも、独自flip禁止も誤り。item meshのbakeは各頂点を `[x-.5,y-.5,z-.5]` にして中心原点化済み（`Client/pomme-client/src/renderer/pipelines/item_entity.rs:1357-1361`）。Display transform側で中心shiftを重ねない。

次の式はcolumn-vector / glamの `A * B` 順（右端から頂点へ）。`E=EntityRenderer::entity_matrix(info,anchor)`、`A=compute_part_transforms(anim)[right_arm|left_arm]`（part transformにモデルroot flip込み）、`F=scale(1,-1,1)` はEntityYDownからitem-local Y-upへ渡すbridge、`V=Rx(-π/2)*Ry(π)*T(sx/16,2/16,-10/16)`、`D=display.to_matrix()`。通常ArmorStandの最終行列は **`M=E*A*F*V*D`**。これで vanillaのhand-local Y-down操作とworld/item Y-upを接続し、head専用flipを再利用せず、根拠のないflip禁止もしない。`E`が yaw・smallの`.5` `body_transform`を含むので重複なし。腕pivotは公式`ArmorStandModel.java:39-44` のright `(-5,2,0)/16`, left `(5,2,0)/16`、腕poseは同ファイル:65-69。default pose・identity display時、`F*Rx(-90°)*Ry(180°)*T`のhand offsetはright `(-1,-10,-2)/16`、left `(1,-10,-2)/16`。よってpose前の腕pivotを含むhand anchorはright `(-6,-8,-2)/16`、left `(6,-8,-2)/16`（`E`によるEntityYDown root rebase/rotationはその後）。この点をmatrix testの期待値にする。

slot割当は物理armごとに行う: Vanilla `ArmedEntityRenderState.java:44-52` はRIGHT/LEFT arm item stateを直接用意し、logical MAINHAND/OFFHANDは `mainArm` に対して反対側へ対応。ArmorStandは公式default RIGHT main armなのでMainhand→right_arm/`thirdperson_righthand`、Offhand→left_arm/`thirdperson_lefthand`。通常Humanoidでleft-handedならこのslot対応が逆転するのでmain_arm_rightを情報として保持する。display left-handed処理もside選択だけでは不十分: `ItemTransform.java:20-40` はleft handでtranslation.x, rotation.y/zの符号を反転し、`ItemTransforms.java:51-69` はlefthand指定欠落時righthand transformをfallbackにする。よって左のDは `T(-tx,ty,tz)*Rx(rx)*Ry(-ry)*Rz(-rz)*S`（translationは1/16 block換算）とする。現行`DisplayResolver`は個別keyを読むがこのfallback/mirrorは未実装のため、held経路で公式意味を実装するか、対応データをD構築時に適用する。適用を二重にしない。

## Client接続とguard

既存 `Renderer::render_world` のframe entity preparationでEquipmentSlotをMainhand/Offhandに限定して読む。空slotは追加せず、item name/meshは既存resolverと`ensure_item_mesh`、描画は`ItemRenderInfo`→`ItemEntityPipeline::draw`を使用。物理armごとに該当slotを一度だけ処理。ArmorStandのdraw guardは (1) entity kindがArmorStand、(2)該当slotにPresent/nonempty stack、(3)対応arm partがmodelにある、(4) spectator skipを共通world抽出側ですでに適用する場合その既存条件、に限る。ShowArms false/invisible/smallをskip条件にしない。通常humanoid babyだけ公式offset値を選択、ArmorStandは通常offset固定。

根拠Client: `pomme-client/src/renderer/pipelines/entity_renderer.rs:3887-3906` は既存head pose取得、`:3909-4025` `entity_matrix` がroot配置と `body_transform` を合成。`pomme-client/src/renderer/mod.rs:1945-2043` は既存head item mesh/ItemRenderInfo追加例。general Humanoid側は`ArmedModel` interface (`minecraft-26.2-decompiled/src/net/minecraft/client/model/ArmedModel.java:7-8`)、`HumanoidMobRenderer.java:38`、`AvatarRenderer.java:58` でlayer搭載を確認し、target enumerationはこれらrendererから作る。Villager crossed-armsは対象外。

## 最小テストと実装順

1. Attachment pure helperへ式を実装。ArmorStand ShowArms false/true両方で同じheld attachmentが得られること、左右arm pivot/poseが正しいこと、small scaleが一度だけ、world Yが不意に反転しないことを行列・変換点で検査。
2. Mainhand/Offhand抽出をArmorStandから接続。空slot skip、mesh warming、display key左右、invisible item継続を確認。
3. Displayのleft-handed mirrorとlefthand fallbackを公式式でテスト。通常Humanoidはmain_arm_right情報、baby offset、対象renderer/layerが揃ったentityだけ拡張。
4. `mise run check`、関連focused tests、最後に`mise run test`。ゲーム表示は通常/小型/pose付き/ShowArmsなし/Invisible/両slotで見る。

追加Vulkan pipelineや独自item meshは作らない。特殊item use/attack pose（bow/trident/shield等）はこの一般attachment式だけでは公式再現されないため、`ItemInHandLayer.java:49-56` の状態情報・animation処理を実装するまでは対応保証しない。
