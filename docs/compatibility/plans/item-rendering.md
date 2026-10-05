# アイテム・装備描画 — 公式26.2互換計画

- **比較基準:** 共通base `a12e38d9ea09d290a0b1736fe48a1cdafe49325f`。対象実装はこのcommitの `Client/`。公式仕様・参照実装は Java Edition 26.2、`minecraft-26.2-decompiled/src/net/minecraft/`。SteelMC参照pinは `0b1f87c36a664f08d81397e942fb1e19f6eb282b`（submodule状態でpin/cleanを確認、render source比較は未実施）。
- **比較環境:** A=Pomme client → 公式26.2 server。B=SteelMC singleplayer → 公式integrated server。Bのauthority・接続・保存比較は各ownerの責任であり、本planはsingleplayer時にも同じ描画consumerが状態を正しく投影する条件だけを扱う。いずれも将来の比較環境であり、今回の実行・挙動一致を示すものではない。
- **母集合の出所:** 公式decompileのenum/codec/property/special/tint bootstrap全登録とrenderer consumer、ならびに固定した26.2 client resource/JAR・data pack・builtin registryの全item/model/equipment corpus。今回、Java宣言は調査したがdecompile treeに資源定義一式はなく、後者のcorpus/hashは未確定。

## 対象と責任境界

対象は既にロード済み資産を消費する item visual の解決と描画: player skin/cape/first-person arm、装備・trim/dye/glint、held item/use/shield、GUI icon、world drop、map、特殊item renderer、item activation。10 display contextすべてを分母とする。stack自体、profile認証、asset parsing/reload、entity poseをrendererが所有しない。

| 状態・責務 | owner / 契約 |
|---|---|
| item stack/components/count/damage/selected slot・container同期 | `inventory` が正本。描画へ一貫したstack snapshotを渡す。 |
| use intent・cooldown・server許可と適用 | `interaction` / `server-gameplay`。描画はuse stateの視覚表現を消費し、権威状態を決めない。 |
| profile/session/token・skin取得の認証/transport | `connection-and-protocol`。item-renderingはprofile/appearance入力とロード済みvisual assetを受け取る。 |
| model/texture/equipment asset parsing、pack優先度、reload、resource generation | `resources-and-launch`。immutableなasset snapshot/generationをconsumer契約として公開。 |
| 生体のpose/visibility/baby/spectator/model-part状態抽出、非生体のmodel pose | `living-entities` / `nonliving-entities`。item-renderingはフレーム単位のappearance/pose snapshotを消費。 |
| stack等からcontext別model/tint/special/equipment visualへ解決し、描画順・変換・layerを適用 | 本機能。held/GUI/drop/map/armor/skin consumers間で解決意味を複製しない。 |
| map data/decoration state、world/gameplay状態 | upstreamのworld/gameplay owner。map rendererは受け取ったmap snapshotを表示する。 |

描画入力は少なくとも `ItemVisualState`（stack components/count/damage/use duration/use action/selected/charged、display context、必要なworld time/view entity等）と `PlayerAppearanceState`（profile由来skin/model type/default fallback、7 part masks、cape、equipped wingsを含む）に分離する。これらは契約上のsnapshotであり、現行コードに同名型があるという主張ではない。数値・profile identityを含むcache keyは、評価に影響するstack/state/context/asset epochを漏らしてstale visualを再利用してはならない。profile秘密情報やtokenを描画cacheへ複製しない。

## 公式の機能母集合

公式Java宣言から全件列挙できる有限語彙は次のとおり。登録数はdecompile source中の全enum値またはbootstrap `ID_MAPPER.put` を数えたもの。登録の列挙完了と各値のPomme consumer coverageは別で、後者は未完了。全item/resource-pack由来の定義は有限登録数に含まれず、resource corpus固定後に追加する。

| 母集合 / 全登録値 | 公式出所（path:line:symbol） | 調査状態 |
|---|---|---|
| Display context 10: `NONE`, `THIRD_PERSON_LEFT_HAND`, `THIRD_PERSON_RIGHT_HAND`, `FIRST_PERSON_LEFT_HAND`, `FIRST_PERSON_RIGHT_HAND`, `HEAD`, `GUI`, `GROUND`, `FIXED`, `ON_SHELF` | `minecraft-26.2-decompiled/src/net/minecraft/world/item/ItemDisplayContext.java:14-25:enum`。first-person/left-hand分類 `:35-40` | 登録全件済。全consumer適用表は未完了 |
| Equipment slot 8: `MAINHAND`, `OFFHAND`, `FEET`, `LEGS`, `CHEST`, `HEAD`, `BODY`, `SADDLE`; type 4: `HAND`, `HUMANOID_ARMOR`, `ANIMAL_ARMOR`, `SADDLE` | `minecraft-26.2-decompiled/src/net/minecraft/world/entity/EquipmentSlot.java:18-27:enum`, `:106-110:getType` | enum全件済。全entity consumerとの直積は未完了 |
| Player model part 7: `CAPE`, `JACKET`, `LEFT_SLEEVE`, `RIGHT_SLEEVE`, `LEFT_PANTS_LEG`, `RIGHT_PANTS_LEG`, `HAT` | `minecraft-26.2-decompiled/src/net/minecraft/world/entity/player/PlayerModelPart.java:17-25:enum` | 全件済。texture/geometry差のruntime比較は未実施 |
| Item use animation 12: `NONE`, `EAT`, `DRINK`, `BLOCK`, `BOW`, `TRIDENT`, `CROSSBOW`, `SPYGLASS`, `TOOT_HORN`, `BRUSH`, `BUNDLE`, `SPEAR` | `minecraft-26.2-decompiled/src/net/minecraft/world/item/ItemUseAnimation.java:18-31:enum` | 全件済。各arm/item/transform consumer網羅は未完了 |
| Item model node codec 8: `empty`, `model`, `range_dispatch`, `special`, `composite`, `bundle/selected_item`, `select`, `condition` | `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/item/ItemModels.java:19-26:bootstrap` | 全登録済。固定resourceでの定義/使用inventoryなし |
| Select property 10: `custom_model_data`, `main_hand`, `charge_type`, `trim_material`, `block_state`, `display_context`, `local_time`, `context_entity_type`, `context_dimension`, `component` | `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/item/properties/select/SelectItemModelProperties.java:18-27:bootstrap` | 全登録済。境界値/全使用assetは未確認 |
| Numeric property 10: `custom_model_data`, `bundle/fullness`, `damage`, `cooldown`, `time`, `compass`, `crossbow/pull`, `use_cycle`, `use_duration`, `count` | `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/item/properties/numeric/RangeSelectItemModelProperties.java:19-28:bootstrap` | 全登録済。単位・丸め・範囲全件runtime条件は未確定 |
| Conditional property 13: `custom_model_data`, `using_item`, `broken`, `damaged`, `fishing_rod/cast`, `has_component`, `bundle/has_selected_item`, `selected`, `carried`, `extended_view`, `keybind_down`, `view_entity`, `component` | `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/item/properties/conditional/ConditionalItemModelProperties.java:19-31:bootstrap` | 全登録済。caller/value semantics別coverage未完了 |
| Special renderer 13: `bell`, `banner`, `book`, `conduit`, `chest`, `copper_golem_statue`, `head`, `player_head`, `shulker_box`, `shield`, `trident`, `decorated_pot`, `end_cube` | `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/special/SpecialModelRenderers.java:19-31:bootstrap` | 全登録済。Pomme固有specialは一部確認、同等性未確認 |
| Tint source 8: `custom_model_data`, `constant`, `dye`, `grass`, `firework`, `potion`, `map_color`, `team` | `minecraft-26.2-decompiled/src/net/minecraft/client/color/item/ItemTintSources.java:18-27:bootstrap` | 全登録済。色入力とfallbackのasset別coverage未完了 |
| Equipment layer type 19: `HUMANOID`, `HUMANOID_LEGGINGS`, `HUMANOID_BABY`, `WINGS`, `WOLF_BODY`, `HORSE_BODY`, `LLAMA_BODY`, `PIG_SADDLE`, `STRIDER_SADDLE`, `CAMEL_SADDLE`, `CAMEL_HUSK_SADDLE`, `HORSE_SADDLE`, `DONKEY_SADDLE`, `MULE_SADDLE`, `ZOMBIE_HORSE_SADDLE`, `SKELETON_HORSE_SADDLE`, `HAPPY_GHAST_BODY`, `NAUTILUS_SADDLE`, `NAUTILUS_BODY` | `minecraft-26.2-decompiled/src/net/minecraft/client/resources/model/EquipmentClientInfo.java:102-123:LayerType` | enum全件済。各typeの全resource/consumer未確認 |
| Client item properties 3: `hand_animation_on_swap` (default true), `oversized_in_gui` (false), `swap_animation_scale` (1.0) | `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/item/ClientItem.java:25-32:Properties` | 宣言/default確認。境界・効果未比較 |

**データ駆動母集合の凍結ゲート:** Java enumやcodec登録はitem個別定義を列挙しない。100%の母集合は版/hashを固定したJE 26.2 client JAR/resources + data pack + builtin registryから、全namespaceのitem定義、参照model graph/node/property/special/tint、stack component、equipment assets/layers/textures、trim materials、map decoration/sprite、profile関連assetを列挙して作る。decompile treeには `assets/**/items/*.json`, `assets/**/models/item/*.json`, `data/**/items/*.json` がなく、生成器宣言を成果resourceと同一視しない。resource corpus未取得・hash未固定の間はasset分母が未知で完了不可。各固定entryを `調査済み / 未調査 / unsupported / runtime未比較` の状態で管理し、未確定entryを網羅済みに数えない。

## 現行実装と差分

以下はbase sourceの静的経路。存在は実出力一致を意味しない。現行実装参照はrepository root相対の `Client/` 配下、公式参照はdecompile root相対の `minecraft-26.2-decompiled/src/net/minecraft/` 配下で示す。

| 機能経路 | 現行source evidence | 公式側の比較点と確認済み差分/unknown |
|---|---|---|
| asset definition→mesh | `Client/pomme-client/src/world/block/model.rs:837-877:bake_item_models` は `item_definition_names` からitems JSONを集める。`:1769-1826:item_definition_is_static/collect_item_model_ids`, `:1979-2021:collect_parts_from_node` はmodelとmodel-only compositeをbakeし、selection系は代表leafへ落とす。 | `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/item/ItemModels.java:19-26:bootstrap` の8 codec全体、各property評価とfull stack model resolveをRust側同経路で行うことは未確認。代表mesh/flat fallbackはselection意味の互換を示さない。 |
| stack→held/drop model | `Client/pomme-client/src/world/block/registry.rs:817-829:BlockRegistry::item_model_name`; `Client/pomme-client/src/app/phases/in_game.rs:8485-8494:item_render_model_name`; `Client/pomme-client/src/renderer/mod.rs:1889-1906:item mesh/display context preparation`。 | `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/ItemInHandRenderer.java:128-147:renderItem` は全stack/context/entity等から `ItemModelResolver.updateForTopItem`→`ItemStackRenderState` を解決。Pommeはmodel key/meshを選ぶ経路あり、property graphのstack-resolved全評価は確認されていない。 |
| held draw/use/shield | `Client/pomme-client/src/renderer/mod.rs:3419-3435:render_world held draw`; `Client/pomme-client/src/renderer/pipelines/held_item.rs:104-176:HeldItemPipeline::update_and_draw`, `Client/pomme-client/src/renderer/pipelines/held_item.rs:137-140:shield-blocking key`, `Client/pomme-client/src/renderer/pipelines/held_item.rs:198-246:transforms`。 | 公式 `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/ItemInHandRenderer.java:149-248:renderMapHand/renderOneHandedMap/renderTwoHandedMap/renderMap`, `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/ItemInHandRenderer.java:252-341:renderPlayerArm/applyEatTransform/applyBrushTransform/applyItemArmAttackTransform/applyItemArmTransform`, `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/ItemInHandRenderer.java:343-476:submitHandsWithItems/evaluateWhichHandsToRender/submitArmWithItem` はmap layouts/arms/hand routing/use transformsを含む。全use animation 12種、左右手・各contextの対応は未確認。 |
| GUI/drop/map | `Client/pomme-client/src/renderer/pipelines/gui_item.rs:265-324:bake_to_slot`, `Client/pomme-client/src/renderer/mod.rs:730-763:GUI atlas`; drop `Client/pomme-client/src/renderer/pipelines/item_entity.rs:1124-1173:ItemEntityPipeline::draw`; map `Client/pomme-client/src/renderer/pipelines/map_quad.rs:217-277:MapQuadPipeline::draw`, map texture update `Client/pomme-client/src/renderer/mod.rs:1865-1882:map texture update`。 | GUIはslot atlas bake経路、dropはmesh cutout/translucent、map ID/color quadがある。GUI dynamic stack/component差、`GROUND`/`FIXED`/`ON_SHELF`全context、map hand layout/arms/background/missing-data/decorations/name等は調査未完了。公式 `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/MapRenderer.java:30-104` はcanvas/decorations/labels/state extractionを示す。 |
| equipment/player | `Client/pomme-client/src/renderer/pipelines/equipment.rs:180-236:resolve_humanoid_equipment_layers`; `Client/pomme-client/src/renderer/mod.rs:1930-2014:chest predicates/body path`; `Client/pomme-client/src/renderer/pipelines/entity_renderer.rs:402-445,707-727,921-929,5066-5080,5176-5248:appearance/cache/masks/cape`; `Client/pomme-client/src/renderer/mod.rs:2503-2554,2592-2618:skin lifecycle`。Wolf armor TODO `Client/pomme-client/src/renderer/entity_model.rs:1138-1144` は明示例にすぎない。 | 公式 `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/layers/HumanoidArmorLayer.java:51-90` slot/asset/model choice、`minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/layers/EquipmentLayerRenderer.java:41-110:EquipmentLayerRenderer` ordered layers/dye/foil/trim、`minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/layers/CapeLayer.java:33-64:CapeLayer`, `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/layers/WingsLayer.java:38-76`, `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/player/AvatarRenderer.java:48-62,142-178` が比較基準。Pommeのhumanoid slot mapping、Happy Ghast body、player masks/cape pathはあるが、全8 slot×entity consumers、baby/armor stand、trim/glint、全19 layer typeの同等性は未確認。 |
| special/tint/activation | Pomme `Client/pomme-client/src/world/block/model.rs:1006:bake special handling` にchest/player head/shield/conduit等個別bake special、held shield path、player-head profile texture `Client/pomme-client/src/renderer/pipelines/item_entity.rs:1124-1173`; activation entry `Client/pomme-client/src/renderer/mod.rs:823-853,1832-1859`, `Client/pomme-client/src/renderer/pipelines/held_item.rs:67-82,198-246`。 | 公式には13 special/8 tint bootstrapとactivation/use consumers。個別specialがあるため「対応ゼロ」とはしないが、13件全argument/context/state/tint/animationは網羅されていない。activation全phases/lifetimeも未確認。 |

Pommeにはmeshがない場合のflat texture fallback (`Client/pomme-client/src/renderer/mod.rs:2677-2725:Renderer::ensure_item_mesh`) とfirst-model/special-base参照 (`Client/pomme-client/src/world/block/model.rs:1846-1852:first_item_model_ref`) がある。これはmissing meshを描画可能にする経路であり、unsupported property/special/componentを公式出力として扱う互換fallbackではない。Unknown/invalid nodeやassetは明確な diagnostic/missing behaviorとし、代表leafを正常解決と偽装しない。

## 互換にする設計

1. **Snapshot境界:** frame/frame-independent item inputをinventory/world/profile/entity ownersから一度snapshotし、renderer内で途中更新を混ぜない。`ItemVisualState` と `PlayerAppearanceState` は意味上分離。body pose/visibility/masksはliving owner、item componentsはinventory、asset epochはresourcesから来る。rendererがserver authorityやprofile credentialsを持たない。
2. **共通resolverと順序:** consumers間で同じitem semanticを再実装せず、`snapshot → context/selector predicate評価 → range/select/condition選択 → composite/empty/special/model解決 → tint/foil/equipment layer → context-specific transform/layout/light → draw` の順を固定する。model graphは子順・fallback・condition入力・数値境界を公式定義どおり保持する。特殊rendererはtype固有stateを解決するが、同じsnapshot/generation/context契約を使う。GUI/held/drop/map/shield/activationは最終layout/drawだけcontext固有とする。
3. **contextとanimation:** 10 contextそれぞれのmodel selection、hand/left-hand transform、arm visibility、GUI sizing, drop transform, ground/fixed/shelf, map, headを公式規則に従い解決。use animation 12種をuse stateとitem definitionから選び、duration/elapsed/charge/hand/selectedの更新順・境界を仕様源で確定して視覚化する。serverが否定/中断したuseは権威状態が届いた時に予測表示を収束させるが、本featureはuse actionを適用しない。
4. **appearance/equipment:** ordered equipment layer列、slot適合、entity model/baby/armor-stand例外、dye/default tint、trim、foil/glint、cape/wings/humanoid胸装備優先関係をsnapshotから一貫して解決。capeはvisible/showCape/skin cape等公式predicate、wings競合と胸layer offsetを保つ。全19 layer typeを共通humanoid扱いせず、狼/馬/ラマ/鞍等は対応entity model consumerとの契約を個別に定める。player skin model slim/wide、7 part maskはappearance側の選択に適用する。
5. **資産世代・failure:** resourcesが新generationの定義/model/textureを用意し、item rendererは完全に利用可能なimmutable snapshotへ切替える。1 draw/frameが旧新generationを混ぜず、参照中GPU資産を解放しない。未解決/不正asset・unsupported nodeは診断可能にし、公式missing behaviorに合う表示か対象layerだけ省略する。profile取得失敗・optional cape欠損は明示したfallback（固定corpus/公式挙動を確認後確定）へ移り、古いupload/descriptorが後から新appearanceを上書きしない。失敗一件で無関係なitem/player描画を停止しない。
6. **cache正当性:** cache keyは当該node/property/specialが読むstack component/count/damage/use/selection/context/entity/world/local time/seed/appearance/asset epochを含む。値を読む軸の欠落によるstale reuseを防ぐ。共有できるasset bakeと、frameごとに変わるresolved stateを混ぜない。最適化はこの正しさを保ってから行う。

## 実装順序

| Milestone | 目的・依存・scope | check / 将来の比較条件 | 100%に残る条件 |
|---|---|---|---|
| M0: 分母・責務を凍結 | resourcesと協力してJE26.2 corpus hash、item/stack component/model/equipment/map/profile entries、全公式consumer登録を列挙。10 context・8 slot・7 mask・12 animation・codec/property/special/tint/layer全件をconsumer matrixへ写像。 | source inventoryの全登録/consumerと固定corpusの参照closureを照合し、各entryにowner・調査状態を付ける。resource reload/pack priorityはresources契約を確認。 | corpus/hash未固定、consumer未列挙、適用関係/例外unknownなら100%判定不可。 |
| M1: state解決とmodel graph | M0依存。immutable input snapshot、公式ordered graph・全codec/property選択・special dispatch・tint・invalid/missing fallbackと診断を確立。代表leaf/flat meshを互換判定に使わない。 | 8 node codec、10 select + 10 numeric + 13 conditional properties全登録のdefault/min/max/境界/欠損/型違い、component差、seed/time/view contextを固定asset上で意味比較。 | 全asset graph/node/property useの未調査、unsupported、fallback挙動差がゼロでない限り未完了。 |
| M2: primary consumers | M1依存。両手held/use/shield、GUI slot/oversized/swap、world drop/count/age、map有データの主要表示を同resolverへ移す。inventory/interaction/protocolはinput/authority境界を保持。 | `FIRST_PERSON_[LEFT/RIGHT]_HAND`, `THIRD_PERSON_[LEFT/RIGHT]_HAND`, `GUI`, `GROUND` を含むconsumer比較。stack component/quantity/use change、GUI atlas/cache更新、drop変化を画像とresolved-stateで比較。 | 全contextとconsumer組合せ、special/tint及びGUI dynamic stack差が未確認なら未完了。 |
| M3: appearance・装備・特殊端 | M1依存。skin/cape/arms/7 masks、全slot×該当entity×19 layer types、baby/armor-stand、ordered dye/trim/glint、cape/wings、map layouts/decorations、全13 special、activationを閉じる。living/nonliving model ownerとの契約を確定。 | player slim/wide・fallback・profile変更、全part mask軸、equipment変更/consumer、map missing/data/decorations/label/hand/frame、各special valid/default/malformed、activation start/update/endを対照。 | registry/asset consumer未調査や値/例外unknownを残す場合は対象機能100%ではない。 |
| M4: fidelity/lifecycle closure | M0-M3後。reload/pack priority、resource generation切替、profile/equipment変更、失敗時/再試行/cache retireを全contextで閉じ、静的coverageと見た目一致を別記録。 | fixed corpus×該当consumer/state matrixで公式26.2画像・解決stateと比較し、色/alpha/UV/geometry/transform/layer order/数値許容差を事前固定。missing/invalid/reload回帰も比較。AとBは別実行記録。 | 1件でも未実行/失敗/unknown、許容差未定義、固定外resource追加時は100%でない。 |

実装箇所候補は前掲のmodel bakingおよびheld/GUI/drop/map/equipment consumerとstate producer/owner契約周辺だが、設計契約を固めた後に実際の変更境界を決める。これは今回のRust変更指示ではない。

## 完了条件と比較ケース

**100%の意味:** Java Edition 26.2の固定・hash付きclient resource/data/builtin registry corpus、および公式 source registrations/consumersを分母とする。各固定item/model graph/component/equipment/profile/map assetを、公式の適用context・consumer・state条件と対応付け、全てについて (1) 定義とconsumerの静的coverage、(2) selector/property/special/tint/layer/use numerical behavior、(3) invalid/missing/reload/fallback、(4) runtime visual/state comparisonを別々に記録し、全entryが合格した時だけ100%。未知分母に対する互換率やサンプル数由来の率は出さない。mod/resource packを含む場合は別のhash付きcorpusとして分母を追加し、無制限のmod集合を暗黙に含めない。

必要比較は直積全総当たりではなく、全登録値の個別被覆と相互作用を分離する。

- 全10 display contextを少なくとも一度ずつ全適用rendererで検証。左右・first/third-personと`show hand`/spectator/selected/useは交互作用を追加する。
- 8 model codecs、10 select、10 numeric、13 conditional、13 special、8 tintを、各登録の正常入力に加えて境界/欠損/invalid/component差の代表条件で検証し、固定corpusに出現する全property-use値は網羅する。数値/丸め/タイミングは公式実装と資源定義からケースごとに確定する。
- held/useは12 animationすべて、左右手/charge/use cancelの適用可能な組合せ。盾blockingと通常表示、bow/trident/crossbow等charge系、eat/drink、spyglass/horn/brush/bundle/spearを個別state遷移比較。
- GUIは全固定item entryの通常iconと `oversized_in_gui`, swap animation properties、damage/count/component/model変化、atlas/cache更新。dropはモデル、透明/pass、count/age/transformと`GROUND`。`FIXED`/`ON_SHELF`/`HEAD`を別consumerで検証。
- mapはone-hand/two-hand、arm/skin、map present/missing/background、decoration icon/rotation/frame filter/label、held context。map decoration registry/resource全件を固定母集合で列挙。
- Equipmentは全8 slotと公式各entity consumer、19 layer typeの固定asset usageを対応づけ、humanoid inner/outer、baby/armor-stand例外、ordered multilayer/dye/default color/trim/foil、cape vs wings vs humanoid chest offsetを比較。player skin/cape/elytraの有無/model/fallback、全7 masksも比較。
- lifecycleはasset reload/pack precedence時のgeneration一貫性、古いupload遅延完了、profile replacement/removal、欠損/不正asset、GPU retireと診断/fallbackを比較。これは資源reload実行をこのphaseで行ったという意味ではない。
- **A:** multiplayerのPomme client画像/stateを公式26.2 serverから得た同一状態で公式clientと比較。**B:** SteelMC singleplayer描画状態を公式integrated serverが作る同等状態と比較。server authority/save/protocolの合否は該当ownerに委ね、rendererの静的経路で代替しない。

許容差は現時点未決定。対象解像度、camera/light/weather、frame timing、GPU/backend、色空間、画像diff閾値と意味的state比較条件をruntime phase前に固定する。今回の成果は静的ソース照合と将来planだけで、build/test/アプリ起動/live比較・実画像比較は一切実施していない。

## 依存・未確認事項

**依存:** resources-and-launch（26.2 assets/registry hash・priority/reload generation・texture lifetime契約）、inventory（stack/component snapshot）、interaction/server-gameplay（use/cooldown/authority遷移）、living-entities（player pose/appearance/visibility/masks・humanoid armor/wings/cape target）、nonliving-entities（animal/vehicle equipment consumer/model contract）、world/map owner（map/decorations state）、connection-and-protocol（profile/appearance retrieval boundary）。隣接planの範囲を書き換えず、各ownerとsnapshot/error contractを合意する。protocolはtransport、resourcesはload、各rendererはsemantic consumerを担う。

**調査済み:** 公式上記有限宣言全件、公式held/map/player armor/cape/wingsの主要入口、Pomme dynamic definition→mesh bake、held/GUI/drop/map/equipment/skin/cape/activationの主要静的経路、flat mesh fallbackの性質。調査済みはsource pathの読解であり実挙動ではない。

**未確認・ブロッカー:**

1. 公式26.2 client JAR/resource/data corpusの取得場所・version/hash、全item/model/component/equipment/trim/map/profile asset inventoryと生成resourceとの差。これがないとasset denominator不明。
2. 全official renderer consumerとslot/type/entity対応、全Pomme producer/callerのcoverage。特に全19 equipment layer/全slot非humanoid、GUI動的stack、map first/third-person全経路。
3. 全property/special/tintの使用assetと意味上の入力値、公式数値・端点・丸め・fallback、use 12種の更新/中断条件、activationのphases/lifetime。
4. profile skin/cape/elytra取得失敗・fallback・slim/wide、appearance cache invalidation、reload中のGPU資産寿命と古い結果の置換。
5. consumerごとの実行環境、視覚比較再現条件・許容差。A/B比較は未実施、SteelMC pinは今回読み取り検証していない。

**実施範囲:** docs-only plan作成。公式比較の将来条件を記述したが、Rust/source/launcher/SteelMC/設定/lockfile/保護userdataは変更していない。
## 機能別の実装トレースと確定した未確認点

以下は上の一般設計を具体化する現行経路。`Client/` prefixはcommon baseのClient repository相対、公式参照は `minecraft-26.2-decompiled/src/net/minecraft/` 相対。参照は実ファイルを読み、1-originの行範囲とsymbolを確認したもの。登録/関数の存在は全ての入力・出力の比較済みを意味しない。表中「未確認」は対応済みと推定しない。

### A. skin / cape / player appearance

| 段階 | 現行 Pomme | 公式26.2比較軸 | 実挙動・差分/未確認 |
|---|---|---|---|
| appearance入力/取得 | `Client/pomme-client/src/renderer/mod.rs:2509-2601:Renderer::load_player_skin/update_local_player_skin/update_player_entity_skin`。UUID由来のskin取得後、hand preview/entity rendererを更新し、失敗時はerrorを返す。`Client/pomme-client/src/app/core.rs:1560-1598:apply_player_skin_result` は要求結果を処理。 | `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/player/AvatarRenderer.java:48-62,142-178:AvatarRenderer` と `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/layers/CapeLayer.java:33-64:CapeLayer`。 | profile入力/通信ownerはconnection/protocol。PommeのHTTP取得/cache key、profile変更・skin texture欠損時の既定skin、古い結果の破棄条件、失敗後の再試行は全callerまで未確認。成功uploadのframe lifecycleは上記関数で確認するが見た目一致は未比較。 |
| model/mask/visibility | `Client/pomme-client/src/renderer/pipelines/entity_renderer.rs:402-445,707-727:player appearance/cache and player model draw`; `Client/pomme-client/src/renderer/pipelines/entity_renderer.rs:921-929,5066-5080,5176-5248:model parts/cape paths`。 | `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/player/AvatarRenderer.java:142-178:AvatarRenderer` と `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/layers/CapeLayer.java:33-64:CapeLayer`。公式part/model/cape predicatesを比較。 | 7 part mask（cape/jacket/sleeves/pants/hat）、slim/wide、playerとmannequin、invisibility、cape bit/texture availabilityは個別条件として測る。Pommeの全組合せ・texture欠損時cape抑止と公式一致は未確認。 |
| wings/cape競合 | `Client/pomme-client/src/renderer/mod.rs:1930-2014:chest equipment/body path` と前掲entity-renderer cape ranges。 | `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/layers/WingsLayer.java:38-76:WingsLayer`; `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/layers/CapeLayer.java:33-64:CapeLayer`; `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/layers/EquipmentLayerRenderer.java:41-110:EquipmentLayerRenderer`。 | chest WINGS/HUMANOID asset/type判定、cape suppressionと胸layer offsetを別々に判定する。Pommeが個別分岐を持つことだけを全装備・全appearance条件の一致根拠にしない。 |

### B. equipment layers

現行 `Client/pomme-client/src/renderer/pipelines/equipment.rs:180-236:resolve_humanoid_equipment_layers` はslotを `Head/Chest/Feet -> humanoid`, `Legs -> humanoid_leggings` に写し、それ以外は空を返す。equippable componentがない、slot不一致、asset id欠落、asset/layer解決失敗なら空、dye componentがなければassetの `color_when_undyed`、dyeableでないlayerは白tint相当を返す。これは現行humanoid経路であり、全equip slotが対応済みという意味ではない。

| 比較単位 | Pomme側根拠 | 公式側根拠 | 判定/未確認 |
|---|---|---|---|
| humanoid slot/asset/dye | 上記 `Client/pomme-client/src/renderer/pipelines/equipment.rs:180-236:resolve_humanoid_equipment_layers`。texture解決は `Client/pomme-client/src/renderer/pipelines/equipment.rs:160-178:resolve_equipment_texture`。 | `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/layers/HumanoidArmorLayer.java:51-90:HumanoidArmorLayer`; `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/layers/EquipmentLayerRenderer.java:41-110:EquipmentLayerRenderer`。 | head/chest/feet/legsのasset id・slot・dye layer順は実装されているが、trim/foil、layer orderingと公式一致は未確認。 |
| body/chest special case | `Client/pomme-client/src/renderer/mod.rs:1930-2014:chest predicates/body equipment draw`。 | `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/layers/EquipmentLayerRenderer.java:41-110:EquipmentLayerRenderer` と `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/layers/WingsLayer.java:38-76`。 | Happy Ghast Body別経路とchest WINGS/HUMANOID predicateはhumanoid armor resolverとは別consumer。全entity/asset適用は未確認。 |
| 19 layer type × entity consumers | Pomme側は上記humanoid resolverとrenderer body/animal callersのみ一部確認。 | `minecraft-26.2-decompiled/src/net/minecraft/client/resources/model/EquipmentClientInfo.java:102-123:LayerType`; `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/layers/EquipmentLayerRenderer.java:41-110:EquipmentLayerRenderer`。 | layer type 19件（前掲母集合）を登録・enum列挙しただけで対応済みにしない。公式のasset使用とentity consumerの関係、Pommeの消費者/欠落を固定corpusに照合する。 |

### C. first-person hand / use / shield / 12 use animations

| 段階 | 現行 Pomme | 比較軸と未確認 |
|---|---|---|
| caller gate/論理手 | `Client/pomme-client/src/renderer/mod.rs:1832-1860:Renderer::render_world` 入力とgate記録、`Client/pomme-client/src/renderer/mod.rs:3419-3454:held hand submission`。held drawは `show_hand && FirstPerson && !top_down` の場合のみ。論理main/offhandはheld tuple `.0/.1`、physical left/rightは `main_hand_right` と `off_hand` から決める。main hand itemなしの時だけempty-hand armをdrawする。 | 公式 `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/ItemInHandRenderer.java:343-476:submitHandsWithItems/evaluateWhichHandsToRender/submitArmWithItem` と同じshow-hand/first-person/spectator/hand evaluationかを比較。Pomme全spectator/empty-hand combinationsは未比較。 |
| use input/選択/transform | `Client/pomme-client/src/renderer/pipelines/held_item.rs:104-176:HeldItemPipeline::update_and_draw` はuse animation/offhandからmesh keyを選び、physical sideを導出し、left/right display resolverを選択。盾blocking時には `shield_blocking` model pathを選び、bow/shield/eat/default arm transformを適用する。 | 公式 `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/ItemInHandRenderer.java:252-341:renderPlayerArm/applyEatTransform/applyBrushTransform/applyItemArmAttackTransform/applyItemArmTransform`, `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/ItemInHandRenderer.java:343-476:submitHandsWithItems`। hand-specific display transformは `ItemDisplayContext` selectionとも比較。 | 現行入力生成は `Client/pomme-client/src/app/phases/in_game.rs:6022-6024:interaction.use_animation` でframe partial tickとinteraction stateから作られ、`:7130-7134` でrender inputsへ渡る。公式use animation 12件は母集合でありcoverageではない。Pomme `UseAnim`への全値mappingとserver-authoritative state update/cancel order、duration/partial tick/left-right selectionは未照合。 |
| shield blocking draw | `Client/pomme-client/src/renderer/pipelines/held_item.rs:137-140,198-246:HeldItemPipeline::update_and_draw` はblocking key/model selection、transform、actual `cmd.draw`。 | 公式 `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/ItemInHandRenderer.java:343-476:submitHandsWithItems` および `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/special/ShieldSpecialRenderer.java:1-97:ShieldSpecialRenderer`（special rendererの入力/mesh compare）。 | shield blocking meshだけでなく、block state predicate、blocking hand、arm visibility、transform、draw順、ordinary shieldとの切替を別判定する。Pomme shield special renderer内の相互作用は未確認。 |

12種の公式 `ItemUseAnimation` は `minecraft-26.2-decompiled/src/net/minecraft/world/item/ItemUseAnimation.java:18-31:enum`。各値を現行 callerから `UseAnim`生成・tick更新・cancel/reset・適用hand・consumer drawまで追い、対応なし/異なる/未確認を記録する。enumの全値が存在することをPommeの描画対応に数えない。

### D. GUI item

Pomme `Client/pomme-client/src/renderer/pipelines/gui_item.rs:265-324:GuiItemPipeline::bake_to_slot` は、呼出元が選んだ `mesh_key` でmesh取得、`original_name`からdisplay transformを解決、mesh translucencyでpipeline選択、player head profileがあればhead texture set選択、slot atlasへのdrawという順。従って「mesh key→atlas slot/display transform」は描画配置責務であり、「stack/component/model selection」は上流model/stack resolver責務である。

現状、GUIがstack component/count/damage/selected/model propertyを全て保持してmesh keyを作るか、同一item idの異なるstackを別表示するか、atlas更新時にbakeを無効化するかは未確認。transparent pipeline分岐はsourceにあるが、全transparent geometry/alpha blend/pass orderingが公式と一致するかは未確認。公式側は `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/item/ItemModelResolver.java:38-64:updateForTopItem/appendItemLayers` がstack→`ItemStackRenderState`を解決し、`:68-77:shouldPlaySwapAnimation/swapAnimationScale` がswap propertyを読む。GUI入口は `minecraft-26.2-decompiled/src/net/minecraft/client/gui/GuiGraphicsExtractor.java:882-898:item` で `GUI` context stateを生成する。`oversized_in_gui`/swap propertyの存在だけでGUI挙動を完了扱いしない。

### E. dropped items

Pommeのworld mesh consumerは `Client/pomme-client/src/renderer/pipelines/item_entity.rs:1124-1173:ItemEntityPipeline::draw`。cutout passの後translucent passを走査し、mesh無/透明分類不一致/player-head texture欠損はskip、各entryにmodel matrix/light/white overlayをpushしてdrawする。traceにはstack_count、actual age、bob、spin、GROUND context等が記録される。

現行producerの完全な入力更新順はここで未照合。比較対象は公式 `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/ItemEntityRenderer.java:31-47:extractRenderState/submit` のstack/model extraction、bob/spin、ならびに `:50-110:submitMultipleFromCount` のcount/depth別copy transformsである。固定corpusでstack/countに応じるcopy count、ageによるbob/spin、rotation、light、cutout/translucent sorting、glint/alpha、component-dependent modelをそれぞれ記録する。上記draw functionだけをもって全producer値・透明描画一致と主張しない。

### F. maps — retention と各consumerを分離

**状態保持 (Pomme):** `Client/pomme-client/src/world/maps.rs:4-8:MapData`, `Client/pomme-client/src/world/maps.rs:161-199:MapStore::apply`。新IDの初回insert時に `scale` と `locked` を設定し、既存IDへの後続applyではこれらを更新しない。colors patchは dimensions>0、範囲内、`pixels.len()==width*height` のときだけ適用し、未初期化色面は128×128 zero面を作り、有効な矩形部分だけcopyする。不正/空patchは既存colorsを保持する。decorationsは `Some` の時だけ置換、`None` は保持。ゆえにscale/locked/初回値、colors patch、decorations patchは別々の状態遷移であり、保存データがあることは全consumerが描くことを意味しない。

| 消費者 | Pommeで確認した経路 | 公式比較・未確認 |
|---|---|---|
| map frame texture colors | `Client/pomme-client/src/app/phases/in_game.rs:8628-8675:extract_map_frame_quads` はframe itemのMapIdとMapStore entryを突合し `MapQuadDraw` を作る。`Client/pomme-client/src/renderer/mod.rs:1865-1882:map texture update` はcolors差分時のみ `map_texture_store.update`。`Client/pomme-client/src/renderer/map_texture.rs:38-112:MapTextureStore::update` はmap palette colorsをtextureへ変換/更新する。 | 公式 `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/MapRenderer.java:30-104:render/extractRenderState` はtexture面とdecorations/labels stateを含む。Pomme map texture storeはcolors由来texture更新の根拠であってdecorations生成/drawの根拠ではない。 |
| item frame quad | `Client/pomme-client/src/app/phases/in_game.rs:8628-8675:extract_map_frame_quads` はframe itemのMapIdとMapStore entryを突合し `MapQuadDraw` を作る。`Client/pomme-client/src/renderer/pipelines/map_quad.rs:217-277:MapQuadPipeline::draw` はtexture面を6頂点で描く。 | 公式 `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/MapRenderer.java:30-57:render` はquad canvasをdrawし、その後marker/labelを別submitする。Pomme quad drawがtexture面のみでdecoration drawではないことを分けて記録。 |
| GUI overlay decorations/labels | Pomme sourceでGUI overlay decoration/icon/name consumerを特定できていない。 | 公式 `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/MapRenderer.java:30-80:render` にdecoration sprite、frame filter、rotation、label placement/text。**Pomme consumer未確認**。MapStore decorationsを保持する事実をoverlay draw対応とみなさない。 |
| held one/two-hand layout | Pomme held pipelineは通常held item transform（上記C）を持つが、map special layout/arms/two-hand map consumerは未確認。 | 公式 `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/ItemInHandRenderer.java:149-248:renderMapHand/renderOneHandedMap/renderTwoHandedMap/renderMap` と `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/ItemInHandRenderer.java:343-476:submitHandsWithItems`。one/two hand、arm/skin、partial data/backgroundを別比較する。 |

公式 decoration registry全値、sprite resolution/missing sprite、name/label、item-frame filter、held map missing data/backgroundは未完了。MapStore decorations保持とmap colors texture更新はそれぞれ保持/texture consumer evidenceに限り、全draw consumerのcoverageを意味しない。

### G. activation animation (ordinary held useとは別pipeline)

| 段階 | 現行経路/挙動 | 比較条件・未確認 |
|---|---|---|
| event start gate/stack決定 | `Client/pomme-client/src/item_activation.rs:185-260:activation_for_event/find_totem_stack` はentity存在かつ対象IDがlocal playerの時だけ開始し、選択main hand→offhand→default totem順にstackを選ぶ。`Client/pomme-client/src/app/core.rs:6004-6010:activation_for_event call` がevent結果をgame stateへセットする。 | 公式activation開始イベントとstack selectionの具体入力・server update順を照合。Pomme gateがremote/unknown eventを除外することは確認済み、runtime比較は未実施。 |
| lifetime/update/payload | `Client/pomme-client/src/item_activation.rs:193-240:ItemActivation::DURATION/tick/draw` は40 tick寿命、simulation tickごとの減算、0到達時終了。drawは `ticks_remaining != 0` 時にborrowed payloadとpartial tickを返し、read-onlyで寿命を進めない。`Client/pomme-client/src/app/phases/in_game.rs:3847-3854:activation tick/end`, `Client/pomme-client/src/app/phases/in_game.rs:7085-7090:activation payload generation`。 | 40 tick、partial tick補間、start/end境界とofficial activation timingの一致は未比較。render frame回数で更新しない契約を維持する。 |
| dedicated draw | `Client/pomme-client/src/renderer/mod.rs:823-853:activation targets/pipeline initialization`, `Client/pomme-client/src/renderer/mod.rs:3464-3620:dedicated activation draw preparation`。`Client/pomme-client/src/renderer/pipelines/held_item.rs:67-82,198-246:update_activation_and_draw` は `FIXED` display transformとanimation transformを合成し専用activation pipelineでdraw。 | 通常held useの `update_and_draw`/hand gateとは別処理。失敗時target/mesh欠損、suspended/resize時のactivation表示は未確認。公式のactivation-specific rendering sourceは固定decompile上で再特定し、通常のItemInHandRenderer use animationと混同しない。 |

## 比較対象のserver境界

比較実行は混ぜない。**A** は Pomme client→公式26.2 dedicated serverの権威状態を入力とするclient rendering比較。**B** はSteelMC singleplayer→公式 integrated serverと同等world/gameplay状態の比較。公式server側のequipmentは `minecraft-26.2-decompiled/src/net/minecraft/server/level/ServerEntity.java:270-284:sendPairingData` がliving entity slotを列挙し、非empty stackを `ClientboundSetEquipmentPacket` として送る。SteelMC側は（以下SteelMC pathはClient repository相対）`third_party/SteelMC/steel-registry/src/equipment.rs:21-74:EquipmentSlot/slot_type` でslot分類、`third_party/SteelMC/steel-core/src/entity/living_base/mod.rs:799-813:queue_equipment_changes/drain_equipment_changes` でstate差分を保持し、`third_party/SteelMC/steel-core/src/entity/tracker/mod.rs:492-500:equipment broadcast` と `third_party/SteelMC/steel-core/src/entity/tracker/mod.rs:908-921:send_to` でpacketを送る。これらはslot/state/send/authority根拠のみで、どちらもclient draw/texture/transformの証拠ではない。SteelMC pin `0b1f87c36a664f08d81397e942fb1e19f6eb282b` はsubmodule状態で確認済み。Bの入力world/gameplay equivalenceはserver-gameplay/singleplayer-lifecycle ownerと別途照合し、このplanでは主張しない。

## 更新後のmilestone gateと未完条件

既存 M0–M4を維持し、次の具体gateを追加する。依存関係は `M0 -> M1 -> (M2 || M3) -> M4`。M2とM3は共通resolver M1後に別trackで進められるが、両方のclosureなしにM4を完了させない。

| Gate | Deliverable / check gate | 100%判定を止める条件 |
|---|---|---|
| M0 corpus + boundaries | JAR/resources/data/builtin registryを版とSHA-256で固定しentry manifest化。Pomme入口→owner/update→consumerと公式登録→consumerのmatrixを作る。A/B別入力契約、隣接ownerとのsnapshot/error boundary確認。 | 固定corpus/hashなし、公式 consumerまたはPomme caller未列挙、map retentionとdraw consumer未分離。 |
| M1 common resolver | model graph/8 codec + 10 select/10 numeric/13 conditional properties、component/context/entity inputs、special/tint/failure fallbackを共通意味モデルで確定。 | enum/codec登録をconsumer testに読み替え、未読property/use assetやstack component差が残る。 |
| M2 primary item consumers | hand/use/shield gate+left/right selection、GUI mesh-key/atlas-placement分離、world drop transforms/count/age/copy/light/transparent、map colors texture updateとitem-frame quadを接続。 | GUI dynamic stack差・transparent pass未測定、12 use-animation consumer未対応、map one/two-handまたはmissing-data未比較。 |
| M3 appearance/special consumers | skin/profile/cache/fallback/masks/cape/wings、equipment slot/type/entity matrix、Happy Ghast, map decoration/label/frame, special renderer, activation lifetime/drawを閉じる。 | 19 layer type登録だけ、appearance欠損/競合条件、map decoration保持のみ、activation start/update/endいずれか未確認。 |
| M4 closure | 固定corpusの各entry×適用 consumer/state条件のstatic coverage・resolved-state差・render output差・failure/reloadを別記録。A/B run resultも独立記録し、許容差/環境を事前固定。 | corpus外の対象追加、未読/未測定/unsupported/unknown、失敗、未定義toleranceが一件でもあれば100%未達。 |

今回のdocs-only checkは文書差分とsource citation範囲を検査するだけであり、runtime behavior parityを証明しない。固定 corpus/hash、全 consumer matrix、実画像/state比較、A/B比較は未取得/未実施のため、この計画に互換率や達成済み判定はない。
