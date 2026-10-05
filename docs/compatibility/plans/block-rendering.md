# 地形block描画 — 公式26.2互換計画

- **共通base:** `a12e38d9ea09d290a0b1736fe48a1cdafe49325f`（調査対象Client worktreeも同一HEAD）。
- **SteelMC参照pin:** `0b1f87c36a664f08d81397e942fb1e19f6eb282c`（読み取り専用。今回の計画では実行・比較していない）。
- **参照版:** decompiled Java Edition 26.2、`minecraft-26.2-decompiled/src/net/minecraft`。調査時点で完全な公式26.2 client resource pack/assetsは手元のcheckoutに存在せず、Javaソース参照と完全asset参照を区別する。
- **比較環境:** A=Pomme client→公式26.2 server（受信したblock/light/biomeをclientがどうterrain描画するか）。B=SteelMC singleplayer→公式integrated server（権威側block/light生成・送信と同一terrain consumerを分けて照合）。A/Bとも計画上の将来比較で、未実行。authority実装はserver-world/server-gameplay、通信運搬はconnection-and-protocol、pack読込はresources、sky/weather/fogはatmosphere、block entity描画はblock-entities、item model描画はitem-renderingに属する。
- **母集合出所:** 公式 `minecraft-26.2-decompiled/src/net/minecraft/world/level/block/Blocks.java:329-1234` (`public static final Block` declarations; extraction filter and names at inventory end)、`minecraft-26.2-decompiled/src/net/minecraft/core/registries/BuiltInRegistries.java:192,194,303-329` (`FLUID`, `BLOCK`, `internalRegister`, bootstrap/freeze) とblock state definition/bootstrap、`minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/dispatch/` のselector/model codecs、`minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/` のfluid model/renderer、`minecraft-26.2-decompiled/src/net/minecraft/client/renderer/chunk/` と `minecraft-26.2-decompiled/src/net/minecraft/world/level/lighting/` のlayer/light/visibility実装。資産ファイル集合は欠落しているため、blockstate/model/texture全ファイルの分母は未確定。

## 対象と責任境界

対象はclient terrainのblock stateからの面/quad生成、block/fluid material/layer、section mesh、terrain用光/AO採取、section内およびsection間visibility、frustumとの合成、GPU mesh更新・破棄まで。16³ sectionごとの地形として描かれる通常block・fluidの全状態とその隣接条件が対象。描画shapeとcollision shapeは別の仕様値であり、物理collisionをterrain側で決めない。

- **状態所有と入力:** connectionはpacketを運ぶだけ。chunk/world consumerが受け取ったblockstate/biome/lightをworld snapshotとして保持し、client light propagationは描画予測・補間値として公式から届くlight dataとの適用順/上書き境界を明記する。server-worldはauthoritative block/light propagation ownerであり、client rendererはその権威を代行しない。resourcesはpack優先順位を解決し、model graph・texture/sprite・material・色入力を一つのimmutable resource generationとして公開する。
- **mesh所有:** terrain mesherは `(world/dimension epoch, resource generation, section revision, neighbor revisions, mesh request generation)` が対応するsnapshotからworker resultを作る。render threadは全stampが現行である場合だけGPU mesh/layerをtransactionalに置換する。古いworker出力で新world/reloadを上書きしない。dimension switch、resource reload、unloadではcancel→queue drain/invalidate→GPU release→visibility reset→新generation再compileの順を契約化する。
- **機能境界:** block model decode/bakeとasset lifetimeはresources側、stateごとの選択・neighbor culling/light・section geometryは本owner、GPU submit/camera frustumはrenderer consumer。atmosphere owns sky/weather/cloud/fog/dimension atmosphere/worldborder draw; block-entities owns separate sign/banner/beacon/etc draw; item-rendering owns item models. Block entitiesをterrain quadに吸収せず、block model rendererにitem-only wrapperを転用しない。`BlockStateModelWrapper`等がterrain selectorから参照可能かは全producer/consumerで判定し、item-only pathなら対象外として記録する。
- **失敗条件:** 不正JSON、missing/unknown model/material、parent cycle/depth超過、missing texture、unsupported selector/layer、worker failure、GPU allocation failureはresource/path/selector/section/generationを伴うdiagnosticにする。正式な意図的 missing-model fallbackと未対応/parse失敗を区別し、黙ったskip/cube fallbackをcoverage達成扱いしない。allocation失敗時は同じworld/resource generationの旧meshを維持し、失敗と再試行予算/terminal failureを見える形で扱う。

## 公式の機能母集合

### 登録・blockstate/model/material

1. **Block宣言inventory (抽出済み宣言、active registry確定ではない):** `minecraft-26.2-decompiled/src/net/minecraft/world/level/block/Blocks.java:329-1234` の `public static final Block NAME = Blocks.register(...)` という行形式に一致する名前を機械抽出し、787件・787 distinctを得た。全名は末尾の4列リストに掲載。`minecraft-26.2-decompiled/src/net/minecraft/world/level/block/Blocks.java:329-1234`にはこの形式外を含めregister呼出しが計794あり、`minecraft-26.2-decompiled/src/net/minecraft/core/registries/BuiltInRegistries.java:194,303-329`のBLOCK registry初期化/bootstrap/freeze、bootstrap完了後の実registry key/stateを突合するまで787をactive key数とみなさない。blockごとの`StateDefinition` property/value直積とpossible statesも完全分母に含める。追加登録・alias・data-driven登録・特殊/air/fluid-only経路は未照合。
2. **blockstate selector有限形:** `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/dispatch/BlockStateModelDispatcher.java:29-40,42-57,60-77` (`CODEC`, `instantiate`, `MultiPartDefinition`, `SimpleModelSelectors`) は `variants`、`multipart`、双方を許し、両方なしを拒否。simple selector重複は例外、multipartは既存simple割当を保ち未割当possible stateを補う。variant key全件、state key/value、selector overlap/unmatched、multipart selector全件を照合する。
3. **weighted model有限形:** `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/dispatch/BlockStateModel.java:61-85` (`ELEMENT_CODEC`,`HARDCODED_WEIGHTED_CODEC`,`Unbaked.CODEC`) は単独 `Variant` または空でないweighted list、各weightはpositive intでdefault 1、weighted itemはsingle variantのみ。`minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/dispatch/Variant.java:12-17,61-71` (`Variant.MAP_CODEC`, `SimpleModelState`) のmodel id/x/y/z/uvlockを全組合せ含める。乱数選択seed/state/position依存を固定seed比較と全候補選択範囲で検証。
4. **multipart condition有限形と否定の表現:** `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/dispatch/multipart/Selector.java:12-20` (`Selector`), `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/dispatch/multipart/Condition.java:16-34` (`Condition`), `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/dispatch/multipart/KeyValueCondition.java:22-80` (`KeyValueCondition`, `Term`, `Terms`), `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/dispatch/multipart/CombinedCondition.java:12-42` (`CombinedCondition`) のrecursive key/value、AND/OR、複数property/value alternatives、空/不正/未知property、selector順序/重複を対象にする。公式 `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/dispatch/multipart/KeyValueCondition.java:45-61` (`Term.parse`) はproperty値の各term先頭 `!` を否定として解釈し、同ファイル `:64-91` (`Terms.parse`) は `|` alternativesを処理する。これは値レベル否定であり、公式condition objectには `NOT`/`!`/`XOR` operator shapeはない。Pomme `Client/pomme-client/src/world/block/model.rs:2114-2129` (`parse_condition`) がobject-level `NOT`/`!`/`XOR`をrejectするのは公式shape外のため互換gapではない。一方、同ファイル `:2143-2157` (`parse_condition` value terms) は値の `!`/`|` を表現し、`:4194-4200` は `!false` のmatching経路を確認できる。object operator rejectionとproperty value negationを分けて全てのselectorで比較する。
5. **model graphとspecial有限母集合:** 公式 `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/dispatch/BlockStateModel.java:21-26,60-93` の `BlockStateModel`/`Unbaked`/`SimpleCachedUnbakedRoot` と単独/weighted variant codec、`minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/model/BlockModel.java:13-25` (`BlockModel`, `Unbaked`), `BlockStateModelWrapper.java:14-44` (`update`, `Unbaked`), `SpecialBlockModelWrapper.java:10-35` (`update`, typed `Unbaked`), `EmptyBlockModel.java` (`EmptyBlockModel` symbol/line未照合), `ConditionalBlockModel.java` (`ConditionalBlockModel` symbol/line未照合), `CompositeBlockModel.java` (`CompositeBlockModel` symbol/line未照合)、`minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/dispatch/BlockStateModelPart.java:12-21` (`BlockStateModelPart.Unbaked`) と `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/SelectBlockModel.java:15-52` (`SelectBlockModel`, `UnbakedSwitch`) のproducer/selectorを別々の型群として追う。未照合型の正確なline/symbolはasset/model-graph調査時に確認し、file-only参照だけでcomplete coverageを主張しない。`BlockStateModel.java` に `BlockStateModel.SpecialWrapper` という型・nested codecは無く、specialは `BlockModel` graphの `SpecialBlockModelWrapper.Unbaked` (`minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/model/SpecialBlockModelWrapper.java:10-35`) を通る。`minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/BuiltInBlockModels.java:74-110,122-130,136-265` は `createBlockModels` のbuilt-in terrain defaultsからspecial wrapperを返す経路。呼び出し連鎖は `minecraft-26.2-decompiled/src/net/minecraft/client/resources/model/ModelManager.java:109` → `BuiltInBlockModels.createBlockModels` → `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/chunk/SectionCompiler.java:94-105` → `ModelBlockRenderer.tesselateBlock`。wrapperは `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/BlockModelRenderState.java:54-85,115-124` でsubmitされる。対して `SpecialModelRenderers.CODEC` (`minecraft-26.2-decompiled/src/net/minecraft/client/renderer/special/SpecialModelRenderers.java:16`) の実consumerはJSON `special` item model `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/item/ItemModels.java:22` → `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/item/SpecialModelWrapper.java:70-80` → `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/item/ItemStackRenderState.java:201-228`。terrain wrapperはregistry id codecではなくtyped `Unbaked`をbuiltinから直接渡す別経路。

   `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/special/SpecialModelRenderers.java:18-32` のbootstrap登録13全件をinventory化し、vanilla sourceのbuilt-in terrain consumer / item-only consumerを分類する。terrain consumersは `BuiltInBlockModels.addDefaults` のtyped `Unbaked` construction、item consumersは `SpecialModelRenderers.CODEC` を使う。

   | 登録名 | scope / terrain consumerの根拠（行番号は直前記載 `BuiltInBlockModels.java`） |
   |---|---|
   | `bell` | terrain: `BuiltInBlockModels.addDefaults:103` → `Blocks.BELL` |
   | `banner` | terrain: `addDefaults:86-87` ground/wall banner colorごとのdispatch |
   | `book` | terrain: `BuiltInBlockModels.addDefaults:106` → `createEnchantingTable:305-309` (enchanting table)を確認。item利用は未確認（汎用codecの登録は利用実例を証明しない） |
   | `conduit` | terrain: `addDefaults:104` → `Blocks.CONDUIT` |
   | `chest` | terrain: `addDefaults:90-100` ender/regular/trapped/copper chest variants |
   | `copper_golem_statue` | terrain: `addDefaults:100` weathering/waxed variants |
   | `head` | terrain: `addDefaults:78-83` mob skull/head states; `createMobHeads` dispatchは`:154-157` |
   | `player_head` | terrain: `addDefaults:84-85` player head/wall head |
   | `shulker_box` | terrain: `addDefaults:88-89` ordinary/dyed shulker boxes |
   | `shield` | item-only: `minecraft-26.2-decompiled/src/net/minecraft/client/data/models/ItemModelGenerators.java:421-422` のnormal/blocking item models; `BuiltInBlockModels.addDefaults` にconsumerなし |
   | `trident` | item-only: `minecraft-26.2-decompiled/src/net/minecraft/client/data/models/ItemModelGenerators.java:451-453` のin-hand/throwing item models; `BuiltInBlockModels.addDefaults` にconsumerなし |
   | `decorated_pot` | terrain: `addDefaults:105` → `Blocks.DECORATED_POT` |
   | `end_cube` | terrain: `addDefaults:107-108` → end gateway/portal |

   13件が公式special registry分母。11 keyにbuilt-in terrain consumer、2 key (`shield`, `trident`) はitem-only consumer。`book` はterrainのenchanting-table consumerを確認済み、item利用は未確認。`SpecialModelRenderers.CODEC` の実consumerはitem model wrapper、terrainでは `SpecialBlockModelWrapper.Unbaked` をbuiltin defaultsからtypedで生成する。汎用codecの登録だけでは特定special keyの利用実例を証明しない。block entity rendererはこのspecial model経路と同一視せず `block-entities` owner。asset不足で全model asset inventoryは未確定だが、このregistry-ID codecのterrain callerはsource上に無い。
6. **layer/material:** 公式 `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/chunk/ChunkSectionLayer.java:9-51` のSOLID/CUTOUT/TRANSLUCENT全3層、transparencyによる分類、material flags・sprite透明度・tint・shade・AO・emissive・cullfaceの組合せを対象とする。材料の宣言と実sprite opacity、force-translucent/material flagsは個別に追跡。
7. **fluid:** `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/FluidStateModelSet.java:12-39` には4 registered Fluid keys: WATER, FLOWING_WATER→water model、LAVA, FLOWING_LAVA→lava model。missing-model fallbackも別経路。`minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/FluidModel.java:11-33` (`FluidModel.Unbaked`, `bake`, `overlayMaterial`, transparency/layer) のstill/flow/optional overlay/material layer/force-translucent/tintの有限形を追う。registry全Fluid（empty含む）とrenderer mapping4を混同しない。water materials = still/flow/overlay+water tint、lava = still/flow・overlay/tintなし。`minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/FluidRenderer.java:29-65`のMAX_FLUID_HEIGHT=0.8888889Fを含む全surface predicates/height/flow/lightingを母集合にする。
8. **light/AO:** `minecraft-26.2-decompiled/src/net/minecraft/world/level/LightLayer.java:6-10`全2値 BLOCK/SKY（dimension capabilityで有無）、level 0..15全16値、`Direction.values()` 6方向、all sections/section edge/corner update states。公式 `minecraft-26.2-decompiled/src/net/minecraft/world/level/lighting/LightEngine.java:32-101,125-161` はdampening/emission/voxel face occlusion、decrease→storage transition→increaseを含む。state block declarationsとの対応、sky-source/height/dimension no-sky cases、section enabled/empty/packet-provided/update/removal lifecycle全てを対象にする。
9. **within-section visibility:** `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/chunk/VisGraph.java:12-127` は16×16×16=4096 voxel、6-direction connectivity、対称6×6=36 face pair、opaque count 0..4096を使い、opaque<256ならall-connected、4096ならnone、その他はedge flood-fill。境界255/256/4095/4096、すべてのseed face、all block solid-render classificationsを含む。
10. **section graph/culling:** `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/SectionOcclusionGraph.java:45-367` (`update`, `waitAndReset`, graph traversal/`GraphState`) のcamera-start/reset/async graph publication, loaded/unloaded/late sections, entry-face cones, view distance, 60-block distant raymarch, smart-cull/frustum/FOV/camera-cell updates、frustum planes、world vertical bounds、visibility absent/failureを全状態として列挙。draw consumerはsolid/cutout/translucent/water/LOD/overlayごとにfalse-hideが無いことを確認。
11. **lifecycle/task/GPU:** official `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/LevelRenderer.java:782-818` (`invalidateCompiledGeometry`), `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/chunk/SectionCompiler.java:53-130`, `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/chunk/SectionRenderDispatcher.java:83-105,416-559` のcompile/cancel/requeue/failure/reload/view-area reset、translucent `SortState` とcamera変更時async resortを含む。Mesh requests/results/upload/disposal、worker panic, task cancellation, device allocation failureの状態遷移を完全列挙。
12. **欠落している資産母集合:** `minecraft-26.2-decompiled`には `assets/`、`assets/minecraft/blockstates`、`assets/minecraft/models`が無く、`Client/pomme-client/assets`はiconだけ。`cache/26.2.json`やdata generator、Steel asset、old auditをvanilla client assetsの代理にしない。正確な公式26.2 game JAR/resource packとasset indexをversion/hash固定で取得して初めて、blockstate JSON全件、全namespace/selector/state/model graph/parent/texture/material/animation/biome tint inputsの有限分母を確定できる。したがって以下787は宣言在庫であって、現時点で全active blockや全assetを網羅したという主張ではない。

### Blocks.java 宣言名一覧（全787 distinct宣言名）

抽出は上記のpublic static final Block + direct `Blocks.register(`行形式。これは宣言名inventoryであり、各nameのregistry keyやactive registry membershipを検証したものではない。行番号範囲329–1234、ファイル全体794 callとの対応・他登録経路/state数は未解決。

| 序 | Blocks field names (source order) |
|---:|---|
| 1–4 | `AIR` · `STONE` · `GRANITE` · `POLISHED_GRANITE` |
| 5–8 | `DIORITE` · `POLISHED_DIORITE` · `ANDESITE` · `POLISHED_ANDESITE` |
| 9–12 | `GRASS_BLOCK` · `DIRT` · `COARSE_DIRT` · `PODZOL` |
| 13–16 | `COBBLESTONE` · `OAK_PLANKS` · `SPRUCE_PLANKS` · `BIRCH_PLANKS` |
| 17–20 | `JUNGLE_PLANKS` · `ACACIA_PLANKS` · `CHERRY_PLANKS` · `DARK_OAK_PLANKS` |
| 21–24 | `PALE_OAK_WOOD` · `PALE_OAK_PLANKS` · `MANGROVE_PLANKS` · `BAMBOO_PLANKS` |
| 25–28 | `BAMBOO_MOSAIC` · `OAK_SAPLING` · `SPRUCE_SAPLING` · `BIRCH_SAPLING` |
| 29–32 | `JUNGLE_SAPLING` · `ACACIA_SAPLING` · `CHERRY_SAPLING` · `DARK_OAK_SAPLING` |
| 33–36 | `PALE_OAK_SAPLING` · `MANGROVE_PROPAGULE` · `BEDROCK` · `WATER` |
| 37–40 | `LAVA` · `SAND` · `SUSPICIOUS_SAND` · `RED_SAND` |
| 41–44 | `GRAVEL` · `SUSPICIOUS_GRAVEL` · `GOLD_ORE` · `DEEPSLATE_GOLD_ORE` |
| 45–48 | `IRON_ORE` · `DEEPSLATE_IRON_ORE` · `COAL_ORE` · `DEEPSLATE_COAL_ORE` |
| 49–52 | `NETHER_GOLD_ORE` · `OAK_LOG` · `SPRUCE_LOG` · `BIRCH_LOG` |
| 53–56 | `JUNGLE_LOG` · `ACACIA_LOG` · `CHERRY_LOG` · `DARK_OAK_LOG` |
| 57–60 | `PALE_OAK_LOG` · `MANGROVE_LOG` · `MANGROVE_ROOTS` · `MUDDY_MANGROVE_ROOTS` |
| 61–64 | `BAMBOO_BLOCK` · `STRIPPED_SPRUCE_LOG` · `STRIPPED_BIRCH_LOG` · `STRIPPED_JUNGLE_LOG` |
| 65–68 | `STRIPPED_ACACIA_LOG` · `STRIPPED_CHERRY_LOG` · `STRIPPED_DARK_OAK_LOG` · `STRIPPED_PALE_OAK_LOG` |
| 69–72 | `STRIPPED_OAK_LOG` · `STRIPPED_MANGROVE_LOG` · `STRIPPED_BAMBOO_BLOCK` · `OAK_WOOD` |
| 73–76 | `SPRUCE_WOOD` · `BIRCH_WOOD` · `JUNGLE_WOOD` · `ACACIA_WOOD` |
| 77–80 | `CHERRY_WOOD` · `DARK_OAK_WOOD` · `MANGROVE_WOOD` · `STRIPPED_OAK_WOOD` |
| 81–84 | `STRIPPED_SPRUCE_WOOD` · `STRIPPED_BIRCH_WOOD` · `STRIPPED_JUNGLE_WOOD` · `STRIPPED_ACACIA_WOOD` |
| 85–88 | `STRIPPED_CHERRY_WOOD` · `STRIPPED_DARK_OAK_WOOD` · `STRIPPED_PALE_OAK_WOOD` · `STRIPPED_MANGROVE_WOOD` |
| 89–92 | `OAK_LEAVES` · `SPRUCE_LEAVES` · `BIRCH_LEAVES` · `JUNGLE_LEAVES` |
| 93–96 | `ACACIA_LEAVES` · `CHERRY_LEAVES` · `DARK_OAK_LEAVES` · `PALE_OAK_LEAVES` |
| 97–100 | `MANGROVE_LEAVES` · `AZALEA_LEAVES` · `FLOWERING_AZALEA_LEAVES` · `SPONGE` |
| 101–104 | `WET_SPONGE` · `GLASS` · `LAPIS_ORE` · `DEEPSLATE_LAPIS_ORE` |
| 105–108 | `LAPIS_BLOCK` · `DISPENSER` · `SANDSTONE` · `CHISELED_SANDSTONE` |
| 109–112 | `CUT_SANDSTONE` · `NOTE_BLOCK` · `POWERED_RAIL` · `DETECTOR_RAIL` |
| 113–116 | `STICKY_PISTON` · `COBWEB` · `SHORT_GRASS` · `FERN` |
| 117–120 | `DEAD_BUSH` · `BUSH` · `SHORT_DRY_GRASS` · `TALL_DRY_GRASS` |
| 121–124 | `SEAGRASS` · `TALL_SEAGRASS` · `PISTON` · `PISTON_HEAD` |
| 125–128 | `MOVING_PISTON` · `DANDELION` · `GOLDEN_DANDELION` · `TORCHFLOWER` |
| 129–132 | `POPPY` · `BLUE_ORCHID` · `ALLIUM` · `AZURE_BLUET` |
| 133–136 | `RED_TULIP` · `ORANGE_TULIP` · `WHITE_TULIP` · `PINK_TULIP` |
| 137–140 | `OXEYE_DAISY` · `CORNFLOWER` · `WITHER_ROSE` · `LILY_OF_THE_VALLEY` |
| 141–144 | `BROWN_MUSHROOM` · `RED_MUSHROOM` · `GOLD_BLOCK` · `IRON_BLOCK` |
| 145–148 | `BRICKS` · `TNT` · `BOOKSHELF` · `CHISELED_BOOKSHELF` |
| 149–152 | `ACACIA_SHELF` · `BAMBOO_SHELF` · `BIRCH_SHELF` · `CHERRY_SHELF` |
| 153–156 | `CRIMSON_SHELF` · `DARK_OAK_SHELF` · `JUNGLE_SHELF` · `MANGROVE_SHELF` |
| 157–160 | `OAK_SHELF` · `PALE_OAK_SHELF` · `SPRUCE_SHELF` · `WARPED_SHELF` |
| 161–164 | `MOSSY_COBBLESTONE` · `OBSIDIAN` · `TORCH` · `WALL_TORCH` |
| 165–168 | `FIRE` · `SOUL_FIRE` · `SPAWNER` · `CREAKING_HEART` |
| 169–172 | `CHEST` · `REDSTONE_WIRE` · `DIAMOND_ORE` · `DEEPSLATE_DIAMOND_ORE` |
| 173–176 | `DIAMOND_BLOCK` · `CRAFTING_TABLE` · `WHEAT` · `FARMLAND` |
| 177–180 | `FURNACE` · `OAK_SIGN` · `SPRUCE_SIGN` · `BIRCH_SIGN` |
| 181–184 | `ACACIA_SIGN` · `CHERRY_SIGN` · `JUNGLE_SIGN` · `DARK_OAK_SIGN` |
| 185–188 | `PALE_OAK_SIGN` · `MANGROVE_SIGN` · `BAMBOO_SIGN` · `OAK_DOOR` |
| 189–192 | `LADDER` · `RAIL` · `OAK_WALL_SIGN` · `SPRUCE_WALL_SIGN` |
| 193–196 | `BIRCH_WALL_SIGN` · `ACACIA_WALL_SIGN` · `CHERRY_WALL_SIGN` · `JUNGLE_WALL_SIGN` |
| 197–200 | `DARK_OAK_WALL_SIGN` · `PALE_OAK_WALL_SIGN` · `MANGROVE_WALL_SIGN` · `BAMBOO_WALL_SIGN` |
| 201–204 | `OAK_HANGING_SIGN` · `SPRUCE_HANGING_SIGN` · `BIRCH_HANGING_SIGN` · `ACACIA_HANGING_SIGN` |
| 205–208 | `CHERRY_HANGING_SIGN` · `JUNGLE_HANGING_SIGN` · `DARK_OAK_HANGING_SIGN` · `PALE_OAK_HANGING_SIGN` |
| 209–212 | `CRIMSON_HANGING_SIGN` · `WARPED_HANGING_SIGN` · `MANGROVE_HANGING_SIGN` · `BAMBOO_HANGING_SIGN` |
| 213–216 | `OAK_WALL_HANGING_SIGN` · `SPRUCE_WALL_HANGING_SIGN` · `BIRCH_WALL_HANGING_SIGN` · `ACACIA_WALL_HANGING_SIGN` |
| 217–220 | `CHERRY_WALL_HANGING_SIGN` · `JUNGLE_WALL_HANGING_SIGN` · `DARK_OAK_WALL_HANGING_SIGN` · `PALE_OAK_WALL_HANGING_SIGN` |
| 221–224 | `MANGROVE_WALL_HANGING_SIGN` · `CRIMSON_WALL_HANGING_SIGN` · `WARPED_WALL_HANGING_SIGN` · `BAMBOO_WALL_HANGING_SIGN` |
| 225–228 | `LEVER` · `STONE_PRESSURE_PLATE` · `IRON_DOOR` · `OAK_PRESSURE_PLATE` |
| 229–232 | `SPRUCE_PRESSURE_PLATE` · `BIRCH_PRESSURE_PLATE` · `JUNGLE_PRESSURE_PLATE` · `ACACIA_PRESSURE_PLATE` |
| 233–236 | `CHERRY_PRESSURE_PLATE` · `DARK_OAK_PRESSURE_PLATE` · `PALE_OAK_PRESSURE_PLATE` · `MANGROVE_PRESSURE_PLATE` |
| 237–240 | `BAMBOO_PRESSURE_PLATE` · `REDSTONE_ORE` · `DEEPSLATE_REDSTONE_ORE` · `REDSTONE_TORCH` |
| 241–244 | `REDSTONE_WALL_TORCH` · `STONE_BUTTON` · `SNOW` · `ICE` |
| 245–248 | `SNOW_BLOCK` · `CACTUS` · `CACTUS_FLOWER` · `CLAY` |
| 249–252 | `SUGAR_CANE` · `JUKEBOX` · `OAK_FENCE` · `NETHERRACK` |
| 253–256 | `SOUL_SAND` · `SOUL_SOIL` · `BASALT` · `POLISHED_BASALT` |
| 257–260 | `SOUL_TORCH` · `SOUL_WALL_TORCH` · `COPPER_TORCH` · `COPPER_WALL_TORCH` |
| 261–264 | `GLOWSTONE` · `NETHER_PORTAL` · `CARVED_PUMPKIN` · `JACK_O_LANTERN` |
| 265–268 | `CAKE` · `REPEATER` · `OAK_TRAPDOOR` · `SPRUCE_TRAPDOOR` |
| 269–272 | `BIRCH_TRAPDOOR` · `JUNGLE_TRAPDOOR` · `ACACIA_TRAPDOOR` · `CHERRY_TRAPDOOR` |
| 273–276 | `DARK_OAK_TRAPDOOR` · `PALE_OAK_TRAPDOOR` · `MANGROVE_TRAPDOOR` · `BAMBOO_TRAPDOOR` |
| 277–280 | `STONE_BRICKS` · `MOSSY_STONE_BRICKS` · `CRACKED_STONE_BRICKS` · `CHISELED_STONE_BRICKS` |
| 281–284 | `PACKED_MUD` · `MUD_BRICKS` · `INFESTED_STONE` · `INFESTED_COBBLESTONE` |
| 285–288 | `INFESTED_STONE_BRICKS` · `INFESTED_MOSSY_STONE_BRICKS` · `INFESTED_CRACKED_STONE_BRICKS` · `INFESTED_CHISELED_STONE_BRICKS` |
| 289–292 | `BROWN_MUSHROOM_BLOCK` · `RED_MUSHROOM_BLOCK` · `MUSHROOM_STEM` · `IRON_BARS` |
| 293–296 | `IRON_CHAIN` · `GLASS_PANE` · `PUMPKIN` · `MELON` |
| 297–300 | `ATTACHED_PUMPKIN_STEM` · `ATTACHED_MELON_STEM` · `PUMPKIN_STEM` · `MELON_STEM` |
| 301–304 | `VINE` · `GLOW_LICHEN` · `RESIN_CLUMP` · `OAK_FENCE_GATE` |
| 305–308 | `MYCELIUM` · `LILY_PAD` · `RESIN_BLOCK` · `RESIN_BRICKS` |
| 309–312 | `RESIN_BRICK_SLAB` · `RESIN_BRICK_WALL` · `CHISELED_RESIN_BRICKS` · `NETHER_BRICKS` |
| 313–316 | `NETHER_BRICK_FENCE` · `NETHER_WART` · `ENCHANTING_TABLE` · `BREWING_STAND` |
| 317–320 | `CAULDRON` · `WATER_CAULDRON` · `LAVA_CAULDRON` · `POWDER_SNOW_CAULDRON` |
| 321–324 | `END_PORTAL` · `END_PORTAL_FRAME` · `END_STONE` · `DRAGON_EGG` |
| 325–328 | `REDSTONE_LAMP` · `COCOA` · `EMERALD_ORE` · `DEEPSLATE_EMERALD_ORE` |
| 329–332 | `ENDER_CHEST` · `TRIPWIRE_HOOK` · `TRIPWIRE` · `EMERALD_BLOCK` |
| 333–336 | `COMMAND_BLOCK` · `BEACON` · `COBBLESTONE_WALL` · `MOSSY_COBBLESTONE_WALL` |
| 337–340 | `FLOWER_POT` · `POTTED_TORCHFLOWER` · `POTTED_OAK_SAPLING` · `POTTED_SPRUCE_SAPLING` |
| 341–344 | `POTTED_BIRCH_SAPLING` · `POTTED_JUNGLE_SAPLING` · `POTTED_ACACIA_SAPLING` · `POTTED_CHERRY_SAPLING` |
| 345–348 | `POTTED_DARK_OAK_SAPLING` · `POTTED_PALE_OAK_SAPLING` · `POTTED_MANGROVE_PROPAGULE` · `POTTED_FERN` |
| 349–352 | `POTTED_DANDELION` · `POTTED_GOLDEN_DANDELION` · `POTTED_POPPY` · `POTTED_BLUE_ORCHID` |
| 353–356 | `POTTED_ALLIUM` · `POTTED_AZURE_BLUET` · `POTTED_RED_TULIP` · `POTTED_ORANGE_TULIP` |
| 357–360 | `POTTED_WHITE_TULIP` · `POTTED_PINK_TULIP` · `POTTED_OXEYE_DAISY` · `POTTED_CORNFLOWER` |
| 361–364 | `POTTED_LILY_OF_THE_VALLEY` · `POTTED_WITHER_ROSE` · `POTTED_RED_MUSHROOM` · `POTTED_BROWN_MUSHROOM` |
| 365–368 | `POTTED_DEAD_BUSH` · `POTTED_CACTUS` · `CARROTS` · `POTATOES` |
| 369–372 | `OAK_BUTTON` · `SPRUCE_BUTTON` · `BIRCH_BUTTON` · `JUNGLE_BUTTON` |
| 373–376 | `ACACIA_BUTTON` · `CHERRY_BUTTON` · `DARK_OAK_BUTTON` · `PALE_OAK_BUTTON` |
| 377–380 | `MANGROVE_BUTTON` · `BAMBOO_BUTTON` · `SKELETON_SKULL` · `SKELETON_WALL_SKULL` |
| 381–384 | `WITHER_SKELETON_SKULL` · `WITHER_SKELETON_WALL_SKULL` · `ZOMBIE_HEAD` · `ZOMBIE_WALL_HEAD` |
| 385–388 | `PLAYER_HEAD` · `PLAYER_WALL_HEAD` · `CREEPER_HEAD` · `CREEPER_WALL_HEAD` |
| 389–392 | `DRAGON_HEAD` · `DRAGON_WALL_HEAD` · `PIGLIN_HEAD` · `PIGLIN_WALL_HEAD` |
| 393–396 | `ANVIL` · `CHIPPED_ANVIL` · `DAMAGED_ANVIL` · `TRAPPED_CHEST` |
| 397–400 | `LIGHT_WEIGHTED_PRESSURE_PLATE` · `HEAVY_WEIGHTED_PRESSURE_PLATE` · `COMPARATOR` · `DAYLIGHT_DETECTOR` |
| 401–404 | `REDSTONE_BLOCK` · `NETHER_QUARTZ_ORE` · `HOPPER` · `QUARTZ_BLOCK` |
| 405–408 | `CHISELED_QUARTZ_BLOCK` · `QUARTZ_PILLAR` · `ACTIVATOR_RAIL` · `DROPPER` |
| 409–412 | `SLIME_BLOCK` · `BARRIER` · `LIGHT` · `IRON_TRAPDOOR` |
| 413–416 | `PRISMARINE` · `PRISMARINE_BRICKS` · `DARK_PRISMARINE` · `PRISMARINE_SLAB` |
| 417–420 | `PRISMARINE_BRICK_SLAB` · `DARK_PRISMARINE_SLAB` · `SEA_LANTERN` · `HAY_BLOCK` |
| 421–424 | `TERRACOTTA` · `COAL_BLOCK` · `PACKED_ICE` · `SUNFLOWER` |
| 425–428 | `LILAC` · `ROSE_BUSH` · `PEONY` · `TALL_GRASS` |
| 429–432 | `LARGE_FERN` · `RED_SANDSTONE` · `CHISELED_RED_SANDSTONE` · `CUT_RED_SANDSTONE` |
| 433–436 | `OAK_SLAB` · `SPRUCE_SLAB` · `BIRCH_SLAB` · `JUNGLE_SLAB` |
| 437–440 | `ACACIA_SLAB` · `CHERRY_SLAB` · `DARK_OAK_SLAB` · `PALE_OAK_SLAB` |
| 441–444 | `MANGROVE_SLAB` · `BAMBOO_SLAB` · `BAMBOO_MOSAIC_SLAB` · `STONE_SLAB` |
| 445–448 | `SMOOTH_STONE_SLAB` · `SANDSTONE_SLAB` · `CUT_SANDSTONE_SLAB` · `PETRIFIED_OAK_SLAB` |
| 449–452 | `COBBLESTONE_SLAB` · `BRICK_SLAB` · `STONE_BRICK_SLAB` · `MUD_BRICK_SLAB` |
| 453–456 | `NETHER_BRICK_SLAB` · `QUARTZ_SLAB` · `RED_SANDSTONE_SLAB` · `CUT_RED_SANDSTONE_SLAB` |
| 457–460 | `PURPUR_SLAB` · `SMOOTH_STONE` · `SMOOTH_SANDSTONE` · `SMOOTH_QUARTZ` |
| 461–464 | `SMOOTH_RED_SANDSTONE` · `SPRUCE_FENCE_GATE` · `BIRCH_FENCE_GATE` · `JUNGLE_FENCE_GATE` |
| 465–468 | `ACACIA_FENCE_GATE` · `CHERRY_FENCE_GATE` · `DARK_OAK_FENCE_GATE` · `PALE_OAK_FENCE_GATE` |
| 469–472 | `MANGROVE_FENCE_GATE` · `BAMBOO_FENCE_GATE` · `SPRUCE_FENCE` · `BIRCH_FENCE` |
| 473–476 | `JUNGLE_FENCE` · `ACACIA_FENCE` · `CHERRY_FENCE` · `DARK_OAK_FENCE` |
| 477–480 | `PALE_OAK_FENCE` · `MANGROVE_FENCE` · `BAMBOO_FENCE` · `SPRUCE_DOOR` |
| 481–484 | `BIRCH_DOOR` · `JUNGLE_DOOR` · `ACACIA_DOOR` · `CHERRY_DOOR` |
| 485–488 | `DARK_OAK_DOOR` · `PALE_OAK_DOOR` · `MANGROVE_DOOR` · `BAMBOO_DOOR` |
| 489–492 | `END_ROD` · `CHORUS_PLANT` · `CHORUS_FLOWER` · `PURPUR_BLOCK` |
| 493–496 | `PURPUR_PILLAR` · `END_STONE_BRICKS` · `TORCHFLOWER_CROP` · `PITCHER_CROP` |
| 497–500 | `PITCHER_PLANT` · `BEETROOTS` · `DIRT_PATH` · `END_GATEWAY` |
| 501–504 | `REPEATING_COMMAND_BLOCK` · `CHAIN_COMMAND_BLOCK` · `FROSTED_ICE` · `MAGMA_BLOCK` |
| 505–508 | `NETHER_WART_BLOCK` · `RED_NETHER_BRICKS` · `BONE_BLOCK` · `STRUCTURE_VOID` |
| 509–512 | `OBSERVER` · `SHULKER_BOX` · `KELP` · `KELP_PLANT` |
| 513–516 | `DRIED_KELP_BLOCK` · `TURTLE_EGG` · `SNIFFER_EGG` · `DRIED_GHAST` |
| 517–520 | `DEAD_TUBE_CORAL_BLOCK` · `DEAD_BRAIN_CORAL_BLOCK` · `DEAD_BUBBLE_CORAL_BLOCK` · `DEAD_FIRE_CORAL_BLOCK` |
| 521–524 | `DEAD_HORN_CORAL_BLOCK` · `TUBE_CORAL_BLOCK` · `BRAIN_CORAL_BLOCK` · `BUBBLE_CORAL_BLOCK` |
| 525–528 | `FIRE_CORAL_BLOCK` · `HORN_CORAL_BLOCK` · `DEAD_TUBE_CORAL` · `DEAD_BRAIN_CORAL` |
| 529–532 | `DEAD_BUBBLE_CORAL` · `DEAD_FIRE_CORAL` · `DEAD_HORN_CORAL` · `TUBE_CORAL` |
| 533–536 | `BRAIN_CORAL` · `BUBBLE_CORAL` · `FIRE_CORAL` · `HORN_CORAL` |
| 537–540 | `DEAD_TUBE_CORAL_FAN` · `DEAD_BRAIN_CORAL_FAN` · `DEAD_BUBBLE_CORAL_FAN` · `DEAD_FIRE_CORAL_FAN` |
| 541–544 | `DEAD_HORN_CORAL_FAN` · `TUBE_CORAL_FAN` · `BRAIN_CORAL_FAN` · `BUBBLE_CORAL_FAN` |
| 545–548 | `FIRE_CORAL_FAN` · `HORN_CORAL_FAN` · `DEAD_TUBE_CORAL_WALL_FAN` · `DEAD_BRAIN_CORAL_WALL_FAN` |
| 549–552 | `DEAD_BUBBLE_CORAL_WALL_FAN` · `DEAD_FIRE_CORAL_WALL_FAN` · `DEAD_HORN_CORAL_WALL_FAN` · `TUBE_CORAL_WALL_FAN` |
| 553–556 | `BRAIN_CORAL_WALL_FAN` · `BUBBLE_CORAL_WALL_FAN` · `FIRE_CORAL_WALL_FAN` · `HORN_CORAL_WALL_FAN` |
| 557–560 | `SEA_PICKLE` · `BLUE_ICE` · `CONDUIT` · `BAMBOO_SAPLING` |
| 561–564 | `BAMBOO` · `POTTED_BAMBOO` · `VOID_AIR` · `CAVE_AIR` |
| 565–568 | `BUBBLE_COLUMN` · `POLISHED_GRANITE_SLAB` · `SMOOTH_RED_SANDSTONE_SLAB` · `MOSSY_STONE_BRICK_SLAB` |
| 569–572 | `POLISHED_DIORITE_SLAB` · `MOSSY_COBBLESTONE_SLAB` · `END_STONE_BRICK_SLAB` · `SMOOTH_SANDSTONE_SLAB` |
| 573–576 | `SMOOTH_QUARTZ_SLAB` · `GRANITE_SLAB` · `ANDESITE_SLAB` · `RED_NETHER_BRICK_SLAB` |
| 577–580 | `POLISHED_ANDESITE_SLAB` · `DIORITE_SLAB` · `BRICK_WALL` · `PRISMARINE_WALL` |
| 581–584 | `RED_SANDSTONE_WALL` · `MOSSY_STONE_BRICK_WALL` · `GRANITE_WALL` · `STONE_BRICK_WALL` |
| 585–588 | `MUD_BRICK_WALL` · `NETHER_BRICK_WALL` · `ANDESITE_WALL` · `RED_NETHER_BRICK_WALL` |
| 589–592 | `SANDSTONE_WALL` · `END_STONE_BRICK_WALL` · `DIORITE_WALL` · `SCAFFOLDING` |
| 593–596 | `LOOM` · `BARREL` · `SMOKER` · `BLAST_FURNACE` |
| 597–600 | `CARTOGRAPHY_TABLE` · `FLETCHING_TABLE` · `GRINDSTONE` · `LECTERN` |
| 601–604 | `SMITHING_TABLE` · `STONECUTTER` · `BELL` · `LANTERN` |
| 605–608 | `SOUL_LANTERN` · `CAMPFIRE` · `SOUL_CAMPFIRE` · `SWEET_BERRY_BUSH` |
| 609–612 | `WARPED_STEM` · `STRIPPED_WARPED_STEM` · `WARPED_HYPHAE` · `STRIPPED_WARPED_HYPHAE` |
| 613–616 | `WARPED_NYLIUM` · `WARPED_FUNGUS` · `WARPED_WART_BLOCK` · `WARPED_ROOTS` |
| 617–620 | `NETHER_SPROUTS` · `CRIMSON_STEM` · `STRIPPED_CRIMSON_STEM` · `CRIMSON_HYPHAE` |
| 621–624 | `STRIPPED_CRIMSON_HYPHAE` · `CRIMSON_NYLIUM` · `CRIMSON_FUNGUS` · `SHROOMLIGHT` |
| 625–628 | `WEEPING_VINES` · `WEEPING_VINES_PLANT` · `TWISTING_VINES` · `TWISTING_VINES_PLANT` |
| 629–632 | `CRIMSON_ROOTS` · `CRIMSON_PLANKS` · `WARPED_PLANKS` · `CRIMSON_SLAB` |
| 633–636 | `WARPED_SLAB` · `CRIMSON_PRESSURE_PLATE` · `WARPED_PRESSURE_PLATE` · `CRIMSON_FENCE` |
| 637–640 | `WARPED_FENCE` · `CRIMSON_TRAPDOOR` · `WARPED_TRAPDOOR` · `CRIMSON_FENCE_GATE` |
| 641–644 | `WARPED_FENCE_GATE` · `CRIMSON_BUTTON` · `WARPED_BUTTON` · `CRIMSON_DOOR` |
| 645–648 | `WARPED_DOOR` · `CRIMSON_SIGN` · `WARPED_SIGN` · `CRIMSON_WALL_SIGN` |
| 649–652 | `WARPED_WALL_SIGN` · `STRUCTURE_BLOCK` · `JIGSAW` · `TEST_BLOCK` |
| 653–656 | `TEST_INSTANCE_BLOCK` · `COMPOSTER` · `TARGET` · `BEE_NEST` |
| 657–660 | `BEEHIVE` · `HONEY_BLOCK` · `HONEYCOMB_BLOCK` · `NETHERITE_BLOCK` |
| 661–664 | `ANCIENT_DEBRIS` · `CRYING_OBSIDIAN` · `RESPAWN_ANCHOR` · `POTTED_CRIMSON_FUNGUS` |
| 665–668 | `POTTED_WARPED_FUNGUS` · `POTTED_CRIMSON_ROOTS` · `POTTED_WARPED_ROOTS` · `LODESTONE` |
| 669–672 | `BLACKSTONE` · `BLACKSTONE_WALL` · `BLACKSTONE_SLAB` · `POLISHED_BLACKSTONE` |
| 673–676 | `POLISHED_BLACKSTONE_BRICKS` · `CRACKED_POLISHED_BLACKSTONE_BRICKS` · `CHISELED_POLISHED_BLACKSTONE` · `POLISHED_BLACKSTONE_BRICK_SLAB` |
| 677–680 | `POLISHED_BLACKSTONE_BRICK_WALL` · `GILDED_BLACKSTONE` · `POLISHED_BLACKSTONE_SLAB` · `POLISHED_BLACKSTONE_PRESSURE_PLATE` |
| 681–684 | `POLISHED_BLACKSTONE_BUTTON` · `POLISHED_BLACKSTONE_WALL` · `CHISELED_NETHER_BRICKS` · `CRACKED_NETHER_BRICKS` |
| 685–688 | `QUARTZ_BRICKS` · `CANDLE` · `CANDLE_CAKE` · `AMETHYST_BLOCK` |
| 689–692 | `BUDDING_AMETHYST` · `AMETHYST_CLUSTER` · `LARGE_AMETHYST_BUD` · `MEDIUM_AMETHYST_BUD` |
| 693–696 | `SMALL_AMETHYST_BUD` · `TUFF` · `TUFF_SLAB` · `TUFF_STAIRS` |
| 697–700 | `TUFF_WALL` · `POLISHED_TUFF` · `POLISHED_TUFF_SLAB` · `POLISHED_TUFF_STAIRS` |
| 701–704 | `POLISHED_TUFF_WALL` · `CHISELED_TUFF` · `TUFF_BRICKS` · `TUFF_BRICK_SLAB` |
| 705–708 | `TUFF_BRICK_STAIRS` · `TUFF_BRICK_WALL` · `CHISELED_TUFF_BRICKS` · `SULFUR` |
| 709–712 | `POTENT_SULFUR` · `POLISHED_SULFUR` · `SULFUR_BRICKS` · `CHISELED_SULFUR` |
| 713–716 | `CINNABAR` · `POLISHED_CINNABAR` · `CINNABAR_BRICKS` · `CHISELED_CINNABAR` |
| 717–720 | `CALCITE` · `TINTED_GLASS` · `POWDER_SNOW` · `SCULK_SENSOR` |
| 721–724 | `CALIBRATED_SCULK_SENSOR` · `SCULK` · `SCULK_VEIN` · `SCULK_CATALYST` |
| 725–728 | `SCULK_SHRIEKER` · `COPPER_ORE` · `DEEPSLATE_COPPER_ORE` · `DRIPSTONE_BLOCK` |
| 729–732 | `POINTED_DRIPSTONE` · `SULFUR_SPIKE` · `CAVE_VINES` · `CAVE_VINES_PLANT` |
| 733–736 | `SPORE_BLOSSOM` · `AZALEA` · `FLOWERING_AZALEA` · `MOSS_CARPET` |
| 737–740 | `PINK_PETALS` · `WILDFLOWERS` · `LEAF_LITTER` · `MOSS_BLOCK` |
| 741–744 | `BIG_DRIPLEAF` · `BIG_DRIPLEAF_STEM` · `SMALL_DRIPLEAF` · `HANGING_ROOTS` |
| 745–748 | `ROOTED_DIRT` · `MUD` · `DEEPSLATE` · `COBBLED_DEEPSLATE` |
| 749–752 | `COBBLED_DEEPSLATE_SLAB` · `COBBLED_DEEPSLATE_WALL` · `POLISHED_DEEPSLATE` · `POLISHED_DEEPSLATE_SLAB` |
| 753–756 | `POLISHED_DEEPSLATE_WALL` · `DEEPSLATE_TILES` · `DEEPSLATE_TILE_SLAB` · `DEEPSLATE_TILE_WALL` |
| 757–760 | `DEEPSLATE_BRICKS` · `DEEPSLATE_BRICK_SLAB` · `DEEPSLATE_BRICK_WALL` · `CHISELED_DEEPSLATE` |
| 761–764 | `CRACKED_DEEPSLATE_BRICKS` · `CRACKED_DEEPSLATE_TILES` · `INFESTED_DEEPSLATE` · `SMOOTH_BASALT` |
| 765–768 | `RAW_IRON_BLOCK` · `RAW_COPPER_BLOCK` · `RAW_GOLD_BLOCK` · `POTTED_AZALEA` |
| 769–772 | `POTTED_FLOWERING_AZALEA` · `OCHRE_FROGLIGHT` · `VERDANT_FROGLIGHT` · `PEARLESCENT_FROGLIGHT` |
| 773–776 | `FROGSPAWN` · `REINFORCED_DEEPSLATE` · `DECORATED_POT` · `CRAFTER` |
| 777–780 | `TRIAL_SPAWNER` · `VAULT` · `HEAVY_CORE` · `PALE_MOSS_BLOCK` |
| 781–784 | `PALE_MOSS_CARPET` · `PALE_HANGING_MOSS` · `OPEN_EYEBLOSSOM` · `CLOSED_EYEBLOSSOM` |
| 785–787 | `POTTED_OPEN_EYEBLOSSOM` · `POTTED_CLOSED_EYEBLOSSOM` · `FIREFLY_BUSH` |

## 現行実装と差分

静的source traceのみ。表の「静的経路あり」は一致や実動作を証明せず、「比較候補」は現時点の確定visual mismatchではない。現時点で確認したものはsource上の処理経路/宣言であり、互換動作の実測ではない。更新契約やowner handoffは設計目標、selector全件・A/B・frame比較は未実施として区別する。比較表は内部実装差と表示出力差を分ける。mesh構築方法、worker/同期方式、内部algorithmやbuffer layoutの差だけは表示非互換として数えない。保守的under-cull/overdrawは性能・最適化候補として記録し、必要面false-cullは欠落候補として区別する。表示非互換を確定するのは、同じ入力条件で最終形状・色・layer・depth/blend/順序・frame outputの差を再現できた場合だけである。false-cullまたはvisible-outputの再現なしに表示差と断定しない。

### Overlay境界と比較条件

terrain対象はfluidのside-overlay material/textureとfluid neighbor-face predicate、block model side-overlay material/textureと隣接predicateなど、section terrain meshへ実際に合成される素材・面の組合せ。現行経路の確認点は `pomme-client/src/world/block/registry.rs:38-65,1091` (`BlockTextures.side_overlay`, atlas material collection)、`pomme-client/src/world/block/model.rs:2874-2886` (grass side-overlay texture assignment)、`pomme-client/src/renderer/chunk/mesher.rs:1764,2795-2821` (terrain side quad/overlay+tint)、fluid側 `pomme-client/src/renderer/chunk/mesher.rs:3111-3375` (`emit_fluid`; flow sprite, `:3371-3372` explicitly says flow sprites have no separate water-overlay material and reuse ordinary two-sided sides) と公式 `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/FluidModel.java:13-28` (`Unbaked.overlayMaterial`, transparency→layer)、`FluidRenderer.java:43-68,85-95` (`isFaceOccludedByState`, `shouldRenderFace`, `tesselate`)。fluid overlay materialは公式比較対象で、現行に同等の独立material経路があるとは確認していない。条件ごとにneighbor state/fluid, face shape, model/fluid layer/material alpha, depth state, draw orderを固定し、terrain geometry/material結果を比較する。

Selection/debug表示とblock-breaking/damage表示はterrain meshへ混ぜず、実際のsource consumerとterrainとの合成順/depth境界を確認してから比較対象/除外を決める。現時点でblock selection/debug overlayの描画consumer、damage-stage terrain overlay consumerとその正確な合成順は未確認であり、これらをoverlay一語の分母へ算入しない。handoff内の `interaction` ownerへ選択・破壊状態から表示consumerへのhandoff確認を依頼し、`hud` ownerへdebug presentationのconsumer確認を依頼する。既知の別ownerである `block-entities` / `item-rendering` 本体の描画は対象外だが、terrain meshとの合成境界/depth/順序だけ本planで比較する。未確認consumerは依存/coverage gapとして明示し、架空ownerへ割り当てない。

| 経路/観察 | Pomme現行sourceと公式比較 | 評価 |
|---|---|---|
| 入力からmesh | `Client/pomme-client/src/renderer/chunk/mesher.rs:962-1078` (`MeshDispatcher::enqueue`, `build_snapshot_inner`) はsection+3×3水平neighbor、light/biome等snapshotを作り、`:1167-1204` (`PendingJob::run`) workerが消費。公式 `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/chunk/SectionCompiler.java:53-130` は16³ sectionをcompileしlayer/visibilityを記録。 | 静的経路あり。完全なworld/resource generationの結合、neighbor縁の全case、official invalidationと一致する保証は未確認。 |
| model input/discovery | `Client/pomme-client/src/world/block/model.rs:20-113` (`BlockstateFile`, `ModelRef`, `ModelFile`) はvariants/multipart, model parent/textures/elements/display等をdeserialize。`:604-745` (`bake_all_models`) はasset-index/packから見つけたblockstateを走査しvariant/multipartをbake、空/失敗/unsupportedをskip/warn。`:2168-2303` (`for_each_blockstate`/asset candidate path) は候補列挙・representative model texture抽出。公式 `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/dispatch/BlockStateModelDispatcher.java:29-77` (`CODEC`, `instantiate`), `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/dispatch/BlockStateModel.java:61-85` (`Unbaked.CODEC`), `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/dispatch/multipart/Condition.java:16-34` はnonempty selectors, strict codecs, single/weighted-only variant, recursive AND/OR. | 公式 `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/dispatch/multipart/KeyValueCondition.java:45-61` (`Term.parse`) とPomme `Client/pomme-client/src/world/block/model.rs:2143-2157,4194-4200` の双方にproperty値 `!false` 否定経路がある。Pomme `Client/pomme-client/src/world/block/model.rs:2118-2120` がJSON object-level `NOT`/`!`/`XOR`を拒否するのも、公式codec形にそのobject operatorsが無いため互換gapではない。値prefix否定とobject shapeの差を混同しない。公式 model graph/generated/special全経路と現行representationの全coverageは未確認。 |
| discovery/parent/texture | `Client/pomme-client/src/world/block/model.rs:567-599` (`texture discovery`) はfirst/default model ref中心、`:2228-2240` scanは候補ファイルstem flatten。`:2313-2378` (`resolve_model`) parent chain 20回・texture variable depth10・missing/cycle経路はNone/diagnosticsが限定的。`:2392-2429` missing model→None。公式 `minecraft-26.2-decompiled/src/net/minecraft/client/resources/model/BlockStateModelLoader.java:33-83` (`loadBlockStates`, blockstate stack)、`ModelDiscovery.java:73-93` (`addRoot`, `addSpecialModel`, `resolve`)、`ModelBakery.java:68-86` (`ModelBakery`, `bakeModels`)、`ModelBaker.java:9-27` (`ModelBaker` / operations) dependency discovery/bakeとmodel resource identifiersを全producerから照合する必要がある。 | 比較候補/asset未確定。resource location namespace/path、全weighted/multipart model texture dependency、cycle/depth、diagnostic/fallbackの明示的 parityは未確認。 |
| cuboid bake/quad | `Client/pomme-client/src/world/block/model.rs:2465-2563` (`bake_resolved_model`) face geometry/UV/rotation/uvlock/cullface/tint/shade/AOを生成し、`:403-447` baked representation AO etc。公式 `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/model/BlockModel.java:13-25`, `BlockStateModelWrapper.java:14-44`, `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/dispatch/BlockStateModelPart.java:12-21`, and `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/ModelBlockRenderer.java:50-75,99-201` (`tesselateBlock`, culling) define graph/quad/material consumers. | 基本経路あり。element 16/22.5 rotation/rescale, UV defaults/rotation lock, face winding, inherited AO, cull shape, every tint and flagsを数値比較するまで一致未確認。Block model graph finite formsがrepresentationに到達するか未確認。 |
| model resolution/error | `Client/pomme-client/src/world/block/model.rs:2313-2429` parent and missing resolution、`:604-745` invalid weight/errors can warn/skip. Official codecs enforce positive weights, non-empty selectors, overlap behavior and explicitly model missing fallbacks through model baker. | 静的差候補: unsupported/missing/error/valid-emptyの区別とper-source diagnostic/readiness契約不足。公式意図的missing modelと無言skipは同一扱いにしない。specialの13登録と全builtin terrain/item consumersは母集合§5に個別分類済み。vanilla assets欠落により一般model graphの全asset instanceは未確定だが、special registry-ID codecにterrain consumerはsource上で確認されない。 |
| solid/shape/greedy | `Client/pomme-client/src/renderer/chunk/mesher.rs:1950-2320` main section traversal/emit, `:3386-3455` (`emit_multipart`), `Client/pomme-client/src/renderer/chunk/greedy.rs:99-108,162-371` face extraction/merge gates. `Client/pomme-client/src/renderer/chunk/mesher.rs:280-320` MeshSink layer binning; `:210-215` region-level alpha behavior. Official `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/chunk/SectionCompiler.java:53-130`, `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/ModelBlockRenderer.java:99-201` state/render-shape culling and `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/chunk/ChunkSectionLayer.java:9-51`. | Static path exists, but arbitrary partial render shapes and graph geometry need full consumer proof. greedy merge allowed only when face/material/tint/UV/light/AO/shape equivalence preserves vanilla output. collision shape must not substitute render occlusion. |
| fluids | `Client/pomme-client/src/renderer/chunk/mesher.rs:2872-3001` (`fluid_height_with_above`, corner height, flow), `:3018-3075` (`emit fluid top/UV predicates`), `:3111-3375` (`emit_fluid`) handles hardcoded water/lava paths, water tint, neighbor same-fluid/boolean registry occlusion, separate water indices. Official `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/FluidStateModelSet.java:12-39` (`bake`, `get`), `FluidModel.java:13-28` (`Unbaked`, overlay and layer), `FluidRenderer.java:29-68,70-95` (`isFaceOccludedByState`, `shouldRenderFace`, `tesselate`) use mapped materials/layers, face shapes, overlay, tint, heights. | Static paths for fluid geometry/tint/flow exist. Comparison candidates: mapping registry5 incl empty vs renderer map4; waterlogged promotion in `Client/pomme-client/src/renderer/chunk/mesher.rs:3123-3130` may flatten fluid amount; voxel face occlusion vs boolean `registry.occludes_neighbor`; overlay/material-controlled layer; exact 0.8888889 height, weighted corners, UV/epsilon, directional light. `ModelBlockRenderer.shouldRenderFace → Block.shouldRenderFace` and fluid shape predicates are official comparison candidates, not proof of current visible mismatch. No mismatch declared without reproducing false-cull or final rendered output difference under fixed inputs. |
| light propagation + render lighting | `Client/pomme-client/src/world/light/mod.rs:158-207,240-287,289-353,355-405,407-570` (`LevelLightEngine` mutation/load/unload/packet/apply/publish) plus `Client/pomme-client/src/world/light/engine.rs:30-68` (`begin_updates`, `finish_updates`, `propagate_decreases`), `Client/pomme-client/src/world/light/storage.rs:125-320` (`StorageCore`), `Client/pomme-client/src/world/light/block.rs:23-154` (`BlockLightEngine`), `Client/pomme-client/src/world/light/sky.rs:37-101,149-220,280-420` (`SkyStorage`, `SkyLightEngine`), `Client/pomme-client/src/world/light/world.rs:35-77` (`StoreWorld`); `Client/pomme-client/src/renderer/chunk/mesher.rs:1687-1723,1832-1905,3715-3900` sample raw block/sky light, AO and face/cardinal shade. Official `minecraft-26.2-decompiled/src/net/minecraft/world/level/lighting/LightEngine.java:32-101,125-161`, `minecraft-26.2-decompiled/src/net/minecraft/world/level/lighting/LevelLightEngine.java:32-177`, `minecraft-26.2-decompiled/src/net/minecraft/world/level/LightLayer.java:6+`; official has separate block/sky levels 0..15 and state shape/emission/dampening rules. | Static pathways exist; phase ordering, official packet-vs-local prediction authority, all boundary/shape/sky-source values and rounding remain comparison candidates. Do not infer mismatch merely because current work is queued/asynchronous. |
| local visibility/frustum/culling | `Client/pomme-client/src/renderer/chunk/occlusion_graph.rs:131-205,214-356` 4096 cell flood and section BFS; `Client/pomme-client/src/renderer/chunk/mesher.rs:1920-1924,1975-1999` visibility mask; `Client/pomme-client/src/renderer/chunk/buffer.rs:273-300,1629-1724` AABB frustum+mask, `:1847-1942` water/translucent draw filters. Official `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/chunk/VisGraph.java:12-127`, `VisibilitySet.java:8-30`, `SectionOcclusionGraph.java:45-367`, and `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/ModelBlockRenderer.java:194-200` (`shouldRenderFace` delegates to `Block.shouldRenderFace`). Fluid face shape is separately tested by `FluidRenderer.java:43-68,85-91` (`isFaceOccludedByState`, `shouldRenderFace`); current Pomme `mesher.rs:3401-3407` uses `registry.occludes_neighbor` in a fluid-face path. | Boolean registry occlusion vs official state/face-shape predicates are comparison candidates, not demonstrated visible differences. Internal graph/mesh algorithm differences do not count as visual gaps by themselves. LOD>0/all-visible or absent visibility may be conservative under-cull/overdraw (performance candidate); necessary-face false-cull is a possible visible gap. Compare rendered geometry and final frame for same state/neighbors/layer/camera, reproducing any missing visible face before asserting a display incompatibility. Existing evidence does not yet establish false-cull or visible-output difference. |
| translucent sorting | `Client/pomme-client/src/renderer/chunk/buffer.rs:1894-1942` orders section draw by center; `:1832-1847` explicitly notes water depth unsorted; individual intersecting quads not sorted per supplemental source. Official `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/chunk/SectionCompiler.java:53-55,150-154` (`compile`, `Results.transparencyState`) stores `MeshData.SortState`; `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/chunk/SectionRenderDispatcher.java:510-559` schedules cancellable resort on camera movement; `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/LevelRenderer.java:600-635,826-860` updates visible sections with budget. | Static mismatch candidate: per-quad dynamic sorting and water depth sort. Confirm official trigger/order/indices and compare separately; don't treat section center sorting as per-quad parity. |
| reload/task/GPU lifetime | `Client/pomme-client/src/renderer/chunk/mesher.rs:401-417,941-1023,1028-1078,1164-1204,1325-1395` stamps/snapshot/queue/worker; `Client/pomme-client/src/renderer/chunk/buffer.rs:378-443,1016-1082,1158-1203` upload epoch/tombstone/stale reject/old buffer retention. Official `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/LevelRenderer.java:782-818` invalidates compiled geometry and resets view area/occlusion; `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/chunk/SectionRenderDispatcher.java:83-105,416-559` errors/cancel/translucent sort. | Static current epoch safeguards exist. Full coupling across resource gen/dimension/light/visibility and terminal worker failure diagnostics/retry/disposal/GPU rollback not established. Reload can expose stale mesh unless every consumer epoch participates. |

## 互換にする設計

terrain selector/model inventoryは `BuiltInRegistries.java:191-194` (`FLUID`, `BLOCK` defaulted registry declarations) と `:303-329` (`internalRegister`, bootstrap loaders, registry freeze) を入口として、`Blocks.java:329-1234` の宣言在庫をbootstrap後のactive registry keys/state definitionに突合する。possible statesは公式 `minecraft-26.2-decompiled/src/net/minecraft/world/level/block/state/StateDefinition.java:52-82,131` (`StateDefinition` construction, `getPossibleStates`) と `:254-255` (`Builder.create`) から各block property/value直積を列挙し、dispatcher `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/dispatch/BlockStateModelDispatcher.java:40-56` (`instantiate`, possible-state fill) へ結び付ける。`Blocks.java`の787 direct declarationsは宣言inventoryに限り、active registry/state/asset denominatorではない。

Special terrain/BE boundary: `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/BuiltInBlockModels.java:74-110,122-130,337-340` (`addDefaults`, `special`, `createBlockModels`) がtyped special block-model defaultsを作り、`minecraft-26.2-decompiled/src/net/minecraft/client/resources/model/ModelManager.java:109` → `BuiltInBlockModels.createBlockModels` → `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/chunk/SectionCompiler.java:53-130` (`compile`; 82-105 block entity and terrain block model are separately visited, `tesselateBlock` at 95) → `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/ModelBlockRenderer.java:50-68` (`tesselateBlock`) がterrain mesh経路。`minecraft-26.2-decompiled/src/net/minecraft/client/renderer/chunk/SectionCompiler.java:146-148` (`handleBlockEntity`) は別BE results経路で、`BuiltInBlockModels`のterrain wrapper存在はBE consumer網羅の証明ではない。今後、各Block ID×possible stateについてstatic terrain special faces/material/depthとdynamic BE faces/draw mode/状態を別記録し、重複描画/欠落を可視化する。special inventoryは `SpecialModelRenderers.java:18-32` の13 keys、`BuiltInBlockModels.addDefaults:78-108` に対応する11 terrain keys、`shield`/`trident`の2 item-only keysの分類を維持する。後者はterrain coverage分母から除外する。item consumerは `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/item/ItemModels.java:22` (`special` codecを`SpecialModelWrapper.Unbaked.MAP_CODEC`へ登録)、`SpecialModelWrapper.java:73-80` (`MAP_CODEC` with `SpecialModelRenderers.CODEC`)、`ItemStackRenderState.java:201-232` (`setupSpecialModel`, submit) のitem-only経路。`SpecialModelRenderers.CODEC`のgeneric登録は個別keyが実際に使われる証拠ではなく、key別使用例は母集合§5のcaller分類を根拠にする。BEの具体runtime consumer inventoryは `block-entities` owner dependencyとして未確認に残す。

1. **Asset generationを一括publish:** version/pack precedenceでresource identifiersを解決し、全blockstate selector→model dependency→parent/texture/material/sprite/colorをvalidateしてimmutable generationとして公開する。selectorは`BlockState`の公式possible-state inventoryへ全件割当し、重複/未割当/意図的missing specialを分類する。model selectionは公式random contractを使い、生成済みmeshには選ばれたmodel/material generationを刻む。parse/resolve/unsupported failuresはsource付diagnosticを持つ。fallbackはpolicyとして明示し、coverageとは別に数える。
2. **mesh snapshotと更新順:** packet semantic update→world block/light/neighbor-border revision invalidate→consistent world+resource snapshot→visibility/layer/material/mesh worker→generation/revision validation→GPU bufferのatomic replace→frame draw。旧world/resource/dimension resultをpublishしない。reload/unload/dimensionはcancel→queue drain/dispose→GPU old-generation release→visibility reset→new-generation compile。未ロード隣接chunkは暫定fallbackの値とdirty/remesh条件を固定し、未知をsolid occluderと仮定しない。workerの一過性失敗はbudget付き再試行、枯渇時はterminal diagnostic。GPU allocation failureは同世代旧meshを維持してfailureを公開する。
3. **geometry/material representation:** render shapeとphysics collision shapeを独立した根拠から得る。baked quadはposition, UV, winding, cullface/render-occlusion shape, material/layer, tint source/index, shade, AO flags等、消費時に損なわれない情報を持つ。greedy mergeはface/material/layer/tint/UV/light/AO/shape境界が公式出力同値の隣接に限定し、異なる条件はquad経路へ残す。official selector graph/special/built-in modelが通常cuboidに縮退不能なら専用経路か明示unsupportedとし、fallback cubeを完全対応と数えない。
4. **fluidをblock asset pipelineと整合:** fluid registry identity/state/level/source-flowing/height/flow/neighbor face shape/material/layer/overlay/tintを保ち、still/flow/overlay materialを同じ資産generationで参照する。公式mapping4とempty/missing fallbackは別扱い。waterlogged blockは実FluidStateを保持し、sourceに強制しない。fluid geometry predicatesはsame-fluid上面、底面、各側面、face shape occlusion、corner height、overlay adjacencyごとに公式数値を使い、通常block cullingを流用しない。
5. **light/AO/visibility:** `minecraft-26.2-decompiled/src/net/minecraft/world/level/LightLayer.java:6-10`×level全値とdimension capabilities、6方向・section境界を扱う。authoritative received light updateとclient predicted recomputeの適用順/rollbackを明示し、0..15 raw valuesを色変換前に比較可能に保つ。AO/faceshadeは公式 per-model flat/smooth/emissive/shape pathを基準に sample位置・補間・丸め・level clamp・corner/face shadingを固定する。section内visgraphとsection間loaded-chunk graphを分ける。false cullより保守的visibleを優先し、visibility data欠落/graph未完了/未ロードでは描画を隠さない。
6. **translucency:** materialごとに公式layerを決定し、translucent section sortとsection内quad index sortを分ける。camera movement/sort pending中の表示とcancel/stale resultの採否を定義する。waterのsort policyは通常translucentと別契約にし、intersecting translucent surfacesとnear/far viewpointをケース化する。
7. **境界契約:** resourcesはgeneration+resolved material/model identitiesをrendererへ、world/chunk/networkはversioned state/light snapshotをterrainへ供給。server-world owns authoritative propagation; terrain owns client render prediction only. RendererはGPU resource lifetime/draw maskをowns。atmosphere, block-entities, item-renderingの専用consumerを地形側へ吸収せず、共有material/resource generationのinterfaceだけ定義する。

## 実装順序

| Milestone | 目的・依存・scope | check / 将来A/B比較条件 | 100%に向けて残る条件 |
|---|---|---|---|
| M0: denominatorと診断 | 公式version/hash固定、asset JAR/index解決、bootstrap後active registry + possible states + fluid states、pack precedence・全resource id inventoryを確定する。M0はM2 selector/model全件比較と再現pixel/frame比較の前提。 | source declaration vs runtime registry/state snapshot とresource pack hashを突合し、全selector→state/model/texture参照を機械照合。A:同一seed/worldの受信chunk全state; B:同一seed generated chunkを入力。asset/hashとactive state集合なしでselector全件比較/pixel比較を完了扱いしない。 | 公式assetsが未入手。exact 26.2 asset/resource packを取得/hash固定できない範囲はblocked/未確定。state/asset denominator未知のまま。 |
| M1: generation・lifecycle・failure | M0依存。まずworld/resource/dimension/section generation consumerを全棚卸しし、現行のproducer→consumer/epoch接続状況を記録する。その上でstale worker/light/GPU reject、unload/reload/reset/cancel/failure/rollbackの更新契約を設計目標として定義する。ここに書かれた契約は実装済みとの主張ではない。 | consumer inventory + 現行契約のsource check後、将来state-transition比較: reloadを処理中meshで挟み旧結果不可視、allocation failureで同世代旧mesh存続、terminal errorを追跡。A: resource reload+chunk border edits/unload; B: dimension/world pause-save-reopen後同じgeneration. | consumer棚卸し/契約設計/動作比較はいずれも未完了。全consumer epoch、failure/retry budget/official task semanticsが未確認。 |
| M2: model registry/asset coverage + block geometry | M0,M1依存。全official block state selector/model graph/special/weighted/random/baked quad/material/tint/cull + complete asset discoveryを揃える。 | 全state coverage table、unmatched/unsupported=0、fixed seed weighted selection; A/B:全state representative sceneではなく全state selector全件を自動入力しmesh topology/material/tint compare、partial shapes/cull neighborsを網羅。 | registryの13 special key、11 built-in terrain consumer、2 item-only consumerをsource callerまで照合済み。asset JAR固定後は一般blockstate/model graphとmodel typeごとの全asset instanceを補完する。 |
| M3: fluidとlayer境界 | M0-M2とresource material契約依存。registry5 vs mapped4, fluid geometry/material/layer/overlay/waterlogged correct. | 2 fluid families×source/flow/levels×top/bottom/6 directions×same/different/fluid/partial occluders/waterlogged/overlay neighbors; heights/flow/UV/tint/layer/indices数値比較。A received states、B authoritative fluid updatesを分ける。 | Fluid registry全件・fluid states、pack-specific material/light casesとpixel parity未比較。 |
| M4: light/AO/visibility | M1 lifecycle+M2 shape/material依存。all 2 layers×16 values×6 dirs, section borders, all block light properties, AO paths, 4096 voxel visgraph and section graph. | propagation raw level arrays, AO sample/interpolation/output, all 36 visibility pairs and thresholds 255/256/4095/4096, unloaded boundaries, FOV/camera bins/late load/frustum each layer. A packet light ordering; B worldgen/time/block edits/lights. | 全state shape/emission/opacity/dimension familiesと公式update schedulingの静的・runtime突合。 |
| M5: translucent ordering/fidelity closure | M2-M4 dependency。公式 layer classification、translucent quad sorting, water sorting, texture UV/animation/shade/color/pixel. | overlapping translucent quads + camera sweep, water crossings, resource animation, deterministic frame capture and geometry/material/light trace on A and B separately. | exact full vanilla assets/hash、全state×全asset×behavior×neighbor/lighting/camera/resource-generation条件を列挙して各条件0 gapか説明済み仕様差になるまで完了しない。 |

## 完了条件と比較ケース

**100%の定義 (将来目標):** 固定Java Edition 26.2 server/client build、正確なvanilla resource pack/asset index hash、bootstrap後block/fluid registry + possible-state snapshot、target settings/render distance/light configurationを記録する。その固定入力から得た (a)全active block keysと各StateDefinition possible states、(b)全blockstate selector・variant weighted alternatives・multipart branches/model graph/built-in/special path、(c)全model parent/texture/material/sprite/animation/color input、(d)all fluid type/state/neighbor/face/material combinations、(e)2 LightLayer×16 levels×6 propagation directions×dimension/section boundaries、(f)AO/render shapes/36 face-connectivity pairs/loaded section graph states、(g)all three render layers/camera sort lifecycleを有限ケース表として生成する。各ケースでexpected official state-selection, mesh topology, material/layer, tint, UV, raw/interpolated light, AO, visibility, sorted indices, generation acceptanceを記録し、未列挙/未検証consumer=0、未対応・欠落・比較待ち=0、再現したvisible output gap=0、failure casesが規定policyを満たす場合のみこのscopeの100%。内部algorithm/mesh構築差だけはvisible gapへ数えない。保守的under-cull/overdrawは性能候補として分離する。固定分母と比較条件に対する将来目標であり、現在の互換率/達成を示さない。公式asset hash、active registry/state corpusが不明な範囲はblocked/未確定、pixel/frame比較は未実施。分母外の推定総合率や旧指摘数ベースの率は出さない。

将来のA/B比較は、各々同一version/settings/seed/camera/frame conditionsを固定し、Pomme client→公式serverとSteelMC singleplayer→公式integratedserverを分離する。Aはpacket semantic state/light、renderer result、公式client referenceを比較。BはSteelMCのauthoritative generation/update/packet lifecycleを公式integratedserverと比較し、terrain consumer差とserver authority差を別ログにする。全ケースでblock identity+state properties, neighbor 3×3/section border, biome, dimension skylight, time/light, resource generation, camera/FOV/frustum, random seed, expected/actual resource idを保存する。pixel/frame capturesはgeometry/material/layer/light/AO/visibility diagnosticsと突合する補助証拠であり、単一画像で分母網羅としない。

具体的将来case: opaque/cutout/translucent/emissive samples; every selector/state and every random alternative fixed seed; model parent alias/missing/cycle/depth and texture variable cycles; element rotations 16°/22.5°+rescale, UV rotations/lock, cullface all 6 directions, tint indices and inherited AO; partial slabs/stairs/fences/leaves/glass against every neighbor face; 4096-cell masks around 255/256/4095/4096 opaque counts and every pair of reachable boundary faces; all 0..15 light transitions at section/chunk edge, shape occlusion/source changes, no-sky and skylight dimensions; water/lava still-flow top/bottom/cardinal, corners, same-fluid adjacency, partial-shape blockers, waterlogged states, overlay neighbors; reload/dimension swap/unload while jobs in flight, worker failure, GPU allocation failure; intersecting glass/leaves/water translucent geometry under camera sweep; LOD/frustum/camera reset and late neighbor-chunk load. これらは future verification planであり今回実行済みではない。

## 依存・未確認事項

- **隣接ownersとの依存:** `resources` ownerがexact 26.2 assets + pack precedence + immutable generationを供給し、`connection-and-protocol`がworld semantic state/light updatesをtransport、`server-world` / `server-gameplay`が権威state/propagationを所有する。`block-entities` は別BE draw、`item-rendering` はitem model draw、`atmosphere` はsky/weather/cloud/fog/dimension/worldborder、`interaction` は選択/破壊状態、`hud` はdebug presentationの所有境界候補としてhandoffする。ただしselection/debug/damage overlayのterrain隣接consumer、正確なdepth/order合成点はsource consumer未確認であり、これらownerとのhandoffを未確認依存として保持する。他owner本体はscope外、terrainとの合成境界/depth/順序だけ本plan比較対象。overlayを一括分母として扱わない。各ownerとのgeneration/semantic handoff、reload invalidation、server authority boundary契約が必要。
- **未確認:** source checkoutにvanilla assetsが無くblockstate/model/texture/material/color/animation inventory未取得。`minecraft-26.2-decompiled/src/net/minecraft/world/level/block/Blocks.java:329-1234` (`Blocks` direct declarations) 787 direct declarationsはactive registry proofでなく、794 total callsとの追加登録経路/aliases/state countsが未確認。special registry 13 keyとbuilt-in terrain/item routesは全件分類したが、asset model graph参照と各typeの全instanceはassets取得後に閉じる。`BlockStateModel.SpecialWrapper`はsource中に存在しない名称で、対応する別graph `SpecialBlockModelWrapper` を追跡した。
- **未確認:** official 26.2でのfull layer/material flags, model graph, random selection, fluids’ exact geometry and light numeric formulasと全current consumerの逐項差分。current waterlogged source-state promotion、boolean occlusion、biome tint input lifecycleを公式state別に比較する。Official layer callerは `SectionCompiler.java:63-72,94-105` (`getOrBeginLayer` by quad material layer; fluid output layer; block tesselation) と `FluidRenderer.java:92-95` (`model.layer()`)。current layer consumer anchorsは `pomme-client/src/renderer/chunk/mesher.rs:280-320` (`MeshSink` layer binning), `pomme-client/src/renderer/chunk/buffer.rs:1847-1942` (layer draw filters)。registry declarations/bootstrapは `BuiltInRegistries.java:192,194,303-329` (`FLUID`, `BLOCK`, `internalRegister`, `bootStrap`/freeze)、宣言inventoryは `Blocks.java:329-1234`、fluid model state mappingは `FluidStateModelSet.java:12-38` (`bake`, `get`)、possible-state enumerateは `StateDefinition.java:52-82,131` (`getPossibleStates`)。787 Blocks declarationsはactive registry/state/asset denominatorではない。
- **未確認:** resource reloadとdimension/world replacementのrenderer/light/model generation同時性、all stale output guards、failure retry/diagnostic parity、section graph async lifecycle、translucent per-quad sort/camera trigger、water depth sortを動作比較していない。
- **現在の範囲:** source照合と計画作成のみ。現行確認済み=列挙した静的source declaration/call paths、設計目標=M0–M5で提案する更新/coverage契約、比較未実施=全asset/state selector、visible false-cull/output/frame parity、overlay合成、BEとのstatic/dynamic二重描画調査。Cargo/build/test/benchmark/application起動/live A/B comparison/visual parityは実施していない。既存実装経路があることは実際の互換性を証明しない。
