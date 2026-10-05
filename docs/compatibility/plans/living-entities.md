# 生体entity描画 — 公式26.2互換計画

- 調査base: `a12e38d9ea09d290a0b1736fe48a1cdafe49325f`（Client tree）
- Steel pin: `0b1f87c36a664f08d81397e942fb1e19f6eb282b`
- 比較対象: Minecraft Java Edition 26.2 decompiled source (`C:/Users/yuzum/Desktop/mine_rust/minecraft-26.2-decompiled`)
- 比較経路: **A=Pomme client → 公式 dedicated server**、**B=Steel singleplayer client → 公式 integrated server**。この計画は両clientの生体entity描画を扱い、server AI・server authorityは別owner。
- 母集合の出所: `minecraft-26.2-decompiled/src/net/minecraft/world/entity/EntityTypes.java:165–322` の各登録についてgeneric classを読み、同source treeのclass継承を `LivingEntity` まで辿った。カテゴリ分類ではなく継承関係で数え、登録型93件を得た。`Player` は `EntityTypes.java:321` 登録だが `Player` → `Avatar` → `LivingEntity` 継承なので含む。`ArmorStand` (`:170`) と `Mannequin` (`:248`) は `MobCategory.MISC` でもliving subclassであり含む。`Bee` (`:176`) も含む。分類は静的ソースから機械的に再現可能。

## 対象と責任境界

対象は公式に登録されたLivingEntity subclass全件のclient-side model、pose、animation、texture variant、overlay/layer、equipment attachment、special render path。entity metadataが意味づけられて所有状態へ届き、render extractionで一貫したframe snapshotを作り、そのsnapshotをrenderer/profileが描画するまでを範囲とする。

- **Protocol/metadata境界**: wire値をentity種別・registry variant込みの意味ある型付き `LivingState`（owner entity）へ写す。未登録variant/未知enum値は定義済みsafe fallbackと診断を使い、入力不正だけでpanicしない。serverが送る戦闘・AI・死亡authorityはこの層で新規模倣せず、受信したclient描画状態を反映する。
- **Render extraction境界**: owner stateから描画に必要な情報を欠落なく一度にsnapshot化する。公式Pose全18値、baby/adultとage scale、registry keyを保持したvariant、前/現tickのanimation clock、hurt/death/invisible/glow状態、head/body yaw、equipmentとbody/model attachment transformを含む。extractionで種別ごとのデータを落としrenderer側に再推定させない。
- **Render profile/dispatch境界**: `LivingRenderProfile(type, model adult/baby, texture variant, animation strategy, ordered layer graph)`がgeneric living rendererへ接続する。model/texture variant未解決とlayer欠落を区別して診断する。EnderDragonのmultipart/history/beam/death rays等は通常profileへ擬装せず、明示的なspecial pathとして登録しgenericとの責任分担を決める。
- **描画実行境界**: 公式tick/frame補間、角度単位、head/body yaw差、hurt/death/invisible/glowing、pose対species animation優先順位、baby scaleとlayer/equipment attachment補正を公式実装値で定義する。各条件の合成順序も互換契約とする。まだソース比較していない係数・順序は本計画で推測せず「要確定」とする。
- **資源・隣接責務**: species body/pose/layerの所有はliving rendering。player skin/cape、equipment item asset/item meshはitem-rendering側との契約を作り、living側がitem assetを複製しない。asset load/reloadとregistry世代管理はresources側。passengerのvehicle transformはnonliving/physics側がownerで、living renderingは契約されたtransform/attachmentのみ受け取る。equipment socket/model transform、baby時のsocket補正、player slim/wide model選択の所有元・適用順を曖昧にしない。

## 公式の機能母集合

EntityTypes登録型をclass inheritanceで数えると**93 living types**。`MobCategory`で切るとMISCのArmorStand/Mannequin/Playerほかを誤って除外するため採用しない。`EntityRenderers.java:47–54,61`を独立に照合した。93件の内訳は通常のEntityType renderer provider 91件と、`createAvatarRenderers` が提供するPlayer/MannequinのAvatar special path 2件である（coverageは93/93だが、93件すべてが通常factory登録という意味ではない）。Model/layerの詳細は全93件のrender class/model referenceを拾った「構造確認」に留まり、各classの`addLayer`、model `setupAnim`、state抽出とPomme値の全数値差分比較は**未実施**。従って一覧のrenderer名は機能存在の記録であって互換pass判定ではない。

| EntityType | 公式登録class | 公式renderer / model起点 | 公式source evidence (line + symbol) | 照合状態 | Pomme `mob_definitions` | Pomme MobDef source | Variant domain | Adult/baby model | Layer graph family | Species animation/events |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| allay | Allay | AllayRenderer | `EntityTypes.java:167 Allay` / `EntityRenderers.java:73 AllayRenderer` | 登録・構造のみ。model/layer詳細未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2494 MobDef.kind=EntityKind::Allay` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `AllayRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| armadillo | Armadillo | ArmadilloRenderer | `EntityTypes.java:169 Armadillo` / `EntityRenderers.java:75 ArmadilloRenderer` | 登録・構造のみ。model/layer詳細未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2014 MobDef.kind=EntityKind::Armadillo` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `ArmadilloRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| armor_stand | ArmorStand | ArmorStandRenderer | `EntityTypes.java:170 ArmorStand` / `EntityRenderers.java:76 ArmorStandRenderer` | 登録・構造のみ。pose/equipment layers詳細未比較 | 定義あり（nonliving_special bakerも要照合） | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1299 MobDef.kind=EntityKind::ArmorStand` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `ArmorStandRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| axolotl | Axolotl | AxolotlRenderer | `EntityTypes.java:172 Axolotl` / `EntityRenderers.java:78 AxolotlRenderer` | 登録・構造のみ。variant/model/layer未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1942 MobDef.kind=EntityKind::Axolotl` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `AxolotlRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| bat | Bat | BatRenderer | `EntityTypes.java:175 Bat` / `EntityRenderers.java:81 BatRenderer` | 登録・構造のみ。model/layer詳細未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1842 MobDef.kind=EntityKind::Bat` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `BatRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| bee | Bee | BeeRenderer | `EntityTypes.java:176 Bee` / `EntityRenderers.java:82 BeeRenderer` | 登録・構造のみ。model/layer詳細未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2506 MobDef.kind=EntityKind::Bee` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `BeeRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| blaze | Blaze | BlazeRenderer | `EntityTypes.java:179 Blaze` / `EntityRenderers.java:85 BlazeRenderer` | 登録・構造のみ。model/layer詳細未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2522 MobDef.kind=EntityKind::Blaze` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `BlazeRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| bogged | Bogged | BoggedRenderer | `EntityTypes.java:181 Bogged` / `EntityRenderers.java:87 BoggedRenderer` | 登録・構造のみ。sheared/layer詳細未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1509 MobDef.kind=EntityKind::Bogged` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `BoggedRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| breeze | Breeze | BreezeRenderer | `EntityTypes.java:182 Breeze` / `EntityRenderers.java:88 BreezeRenderer` | 登録・構造のみ。model/layer詳細未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2534 MobDef.kind=EntityKind::Breeze` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `BreezeRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| camel | Camel | CamelRenderer | `EntityTypes.java:184 Camel` / `EntityRenderers.java:90 CamelRenderer` | 登録・構造のみ。baby/pose/layer未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2030 MobDef.kind=EntityKind::Camel` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `CamelRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| camel_husk | CamelHusk | CamelHuskRenderer | `EntityTypes.java:185 CamelHusk` / `EntityRenderers.java:91 CamelHuskRenderer` | 登録・構造のみ。baby/pose/layer未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2046 MobDef.kind=EntityKind::CamelHusk` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `CamelHuskRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| cat | Cat | CatRenderer | `EntityTypes.java:186 Cat` / `EntityRenderers.java:92 CatRenderer` | 登録・構造のみ。pose/layer詳細未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1672 MobDef.kind=EntityKind::Cat` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `CatRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| cave_spider | CaveSpider | CaveSpiderRenderer | `EntityTypes.java:187 CaveSpider` / `EntityRenderers.java:93 CaveSpiderRenderer` | 登録・構造のみ。layer詳細未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1591 MobDef.kind=EntityKind::CaveSpider` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `CaveSpiderRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| chicken | Chicken | ChickenRenderer | `EntityTypes.java:191 Chicken` / `EntityRenderers.java:97 ChickenRenderer` | 登録・構造のみ。baby/wing layer詳細未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1343 MobDef.kind=EntityKind::Chicken` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `ChickenRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| cod | Cod | CodRenderer | `EntityTypes.java:192 Cod` / `EntityRenderers.java:98 CodRenderer` | 登録・構造のみ。model/animation未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1857 MobDef.kind=EntityKind::Cod` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `CodRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| copper_golem | CopperGolem | CopperGolemRenderer | `EntityTypes.java:193 CopperGolem` / `EntityRenderers.java:100 CopperGolemRenderer` | 登録・構造のみ。pose/layer詳細未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2410 MobDef.kind=EntityKind::CopperGolem` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `CopperGolemRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| cow | Cow | CowRenderer | `EntityTypes.java:195 Cow` / `EntityRenderers.java:101 CowRenderer` | 登録・構造のみ。baby/variant/layer未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1331 MobDef.kind=EntityKind::Cow` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `CowRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| creaking | Creaking | CreakingRenderer | `EntityTypes.java:196 Creaking` / `EntityRenderers.java:102 CreakingRenderer` | 登録・構造のみ。model/layer詳細未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2422 MobDef.kind=EntityKind::Creaking` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `CreakingRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| creeper | Creeper | CreeperRenderer | `EntityTypes.java:197 Creeper` / `EntityRenderers.java:103 CreeperRenderer` | 登録・構造のみ。charged layer/pose未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1528 MobDef.kind=EntityKind::Creeper` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `CreeperRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| dolphin | Dolphin | DolphinRenderer | `EntityTypes.java:200 Dolphin` / `EntityRenderers.java:106 DolphinRenderer` | 登録・構造のみ。model/animation未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1958 MobDef.kind=EntityKind::Dolphin` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `DolphinRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| donkey | Donkey | DonkeyRenderer | `EntityTypes.java:201 Donkey` / `EntityRenderers.java:109 DonkeyRenderer` | 登録・構造のみ。baby/saddle/equipment layers未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1752 MobDef.kind=EntityKind::Donkey` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `DonkeyRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| drowned | Drowned | DrownedRenderer | `EntityTypes.java:203 Drowned` / `EntityRenderers.java:114 DrownedRenderer` | 登録・構造のみ。clothing layer未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1437 MobDef.kind=EntityKind::Drowned` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `DrownedRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| elder_guardian | ElderGuardian | ElderGuardianRenderer | `EntityTypes.java:205 ElderGuardian` / `EntityRenderers.java:116 ElderGuardianRenderer` | 登録・構造のみ。layer/animation未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1986 MobDef.kind=EntityKind::ElderGuardian` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `ElderGuardianRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| enderman | EnderMan | EndermanRenderer | `EntityTypes.java:206 EnderMan` / `EntityRenderers.java:117 EndermanRenderer` | 登録・構造のみ。carried block/eyes layer未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1608 MobDef.kind=EntityKind::Enderman` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `EndermanRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| endermite | Endermite | EndermiteRenderer | `EntityTypes.java:207 Endermite` / `EntityRenderers.java:118 EndermiteRenderer` | 登録・構造のみ。model/animation未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2242 MobDef.kind=EntityKind::Endermite` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `EndermiteRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| ender_dragon | EnderDragon | EnderDragonRenderer | `EntityTypes.java:208 EnderDragon` / `EntityRenderers.java:119 EnderDragonRenderer` | special multipart/history/beam/death path構造確認。Pomme接続未確認 | **定義なし** | なし（MobDef未定義） | 公式候補未確認（closure blocker） | 該当なし（未定義）。公式adult/baby model未照合 | `EnderDragonRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| evoker | Evoker | EvokerRenderer | `EntityTypes.java:211 Evoker` / `EntityRenderers.java:122 EvokerRenderer` | 登録・構造のみ。spell/held item layers未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2326 MobDef.kind=EntityKind::Evoker` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `EvokerRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| fox | Fox | FoxRenderer | `EntityTypes.java:219 Fox` / `EntityRenderers.java:131 FoxRenderer` | 登録・構造のみ。pose/layers未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2058 MobDef.kind=EntityKind::Fox` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `FoxRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| frog | Frog | FrogRenderer | `EntityTypes.java:220 Frog` / `EntityRenderers.java:132 FrogRenderer` | 登録・構造のみ。tongue/pose animation未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2074 MobDef.kind=EntityKind::Frog` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `FrogRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| ghast | Ghast | GhastRenderer | `EntityTypes.java:222 Ghast` / `EntityRenderers.java:134 GhastRenderer` | 登録・構造のみ。charging variants未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2558 MobDef.kind=EntityKind::Ghast` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `GhastRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| happy_ghast | HappyGhast | HappyGhastRenderer | `EntityTypes.java:223 HappyGhast` / `EntityRenderers.java:135 HappyGhastRenderer` | 登録・構造のみ。harness/equipment layers未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2570 MobDef.kind=EntityKind::HappyGhast` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `HappyGhastRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| giant | Giant | GiantMobRenderer | `EntityTypes.java:224 Giant` / `EntityRenderers.java:136 GiantMobRenderer` | 登録・構造のみ。scaleとlayer補正未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1569 MobDef.kind=EntityKind::Giant` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `GiantMobRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| glow_squid | GlowSquid | GlowSquidRenderer | `EntityTypes.java:226 GlowSquid` / `EntityRenderers.java:140 GlowSquidRenderer` | 登録・構造のみ。baby model/light layer未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1830 MobDef.kind=EntityKind::GlowSquid` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `GlowSquidRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| goat | Goat | GoatRenderer | `EntityTypes.java:227 Goat` / `EntityRenderers.java:144 GoatRenderer` | 登録・構造のみ. ram pose/layers未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2086 MobDef.kind=EntityKind::Goat` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `GoatRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| guardian | Guardian | GuardianRenderer | `EntityTypes.java:228 Guardian` / `EntityRenderers.java:145 GuardianRenderer` | 登録・構造のみ。beam/eyes layer未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1974 MobDef.kind=EntityKind::Guardian` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `GuardianRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| hoglin | Hoglin | HoglinRenderer | `EntityTypes.java:229 Hoglin` / `EntityRenderers.java:146 HoglinRenderer` | 登録・構造のみ。baby/pose/layers未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2102 MobDef.kind=EntityKind::Hoglin` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `HoglinRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| horse | Horse | HorseRenderer | `EntityTypes.java:231 Horse` / `EntityRenderers.java:148 HorseRenderer` | 登録・構造のみ。marking/saddle/armor layers未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1718 MobDef.kind=EntityKind::Horse` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `HorseRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| husk | Husk | HuskRenderer | `EntityTypes.java:232 Husk` / `EntityRenderers.java:149 HuskRenderer` | 登録・構造のみ。humanoid layers未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1425 MobDef.kind=EntityKind::Husk` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `HuskRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| illusioner | Illusioner | IllusionerRenderer | `EntityTypes.java:233 Illusioner` / `EntityRenderers.java:150 IllusionerRenderer` | 登録・構造のみ。spell/held layers未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2338 MobDef.kind=EntityKind::Illusioner` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `IllusionerRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| iron_golem | IronGolem | IronGolemRenderer | `EntityTypes.java:235 IronGolem` / `EntityRenderers.java:152 IronGolemRenderer` | 登録・構造のみ。crack layer/attack pose未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1925 MobDef.kind=EntityKind::IronGolem` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `IronGolemRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| llama | Llama | LlamaRenderer | `EntityTypes.java:243 Llama` / `EntityRenderers.java:161 LlamaRenderer` | 登録・構造のみ。baby/decor/saddle layers未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2210 MobDef.kind=EntityKind::Llama` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `LlamaRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| magma_cube | MagmaCube | MagmaCubeRenderer | `EntityTypes.java:245 MagmaCube` / `EntityRenderers.java:163 MagmaCubeRenderer` | 登録・構造のみ。cube geometry/size未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2586 MobDef.kind=EntityKind::MagmaCube` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `MagmaCubeRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| mannequin | Mannequin | AvatarRenderer (special; `EntityRenderers.java:61`) | `EntityTypes.java:248 Mannequin` / `EntityRenderers.java:47 createAvatarRenderers` | special registration確認。skin/model/layers未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1389 MobDef.kind=EntityKind::Mannequin` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `AvatarRenderer (special; `EntityRenderers.java:61`)`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| mooshroom | MushroomCow | MushroomCowRenderer | `EntityTypes.java:251 MushroomCow` / `EntityRenderers.java:168 MushroomCowRenderer` | 登録・構造のみ。mushroom layer/variant未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2598 MobDef.kind=EntityKind::Mooshroom` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `MushroomCowRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| mule | Mule | DonkeyRenderer (MULE/MULE_BABY types) | `EntityTypes.java:252 Mule` / `EntityRenderers.java:171 DonkeyRenderer` | 登録・構造のみ。baby/saddle layers未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1769 MobDef.kind=EntityKind::Mule` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `DonkeyRenderer (MULE/MULE_BABY types)`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| nautilus | Nautilus | NautilusRenderer | `EntityTypes.java:253 Nautilus` / `EntityRenderers.java:175 NautilusRenderer` | 登録・構造のみ。baby/equipment layers未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2458 MobDef.kind=EntityKind::Nautilus` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `NautilusRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| ocelot | Ocelot | OcelotRenderer | `EntityTypes.java:256 Ocelot` / `EntityRenderers.java:178 OcelotRenderer` | 登録・構造のみ。pose/variant未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1694 MobDef.kind=EntityKind::Ocelot` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `OcelotRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| panda | Panda | PandaRenderer | `EntityTypes.java:261 Panda` / `EntityRenderers.java:183 PandaRenderer` | 登録・構造のみ。gene/pose layers未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2134 MobDef.kind=EntityKind::Panda` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `PandaRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| parched | Parched | ParchedRenderer | `EntityTypes.java:262 Parched` / `EntityRenderers.java:184 ParchedRenderer` | 登録・構造のみ。model/layers未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2302 MobDef.kind=EntityKind::Parched` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `ParchedRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| parrot | Parrot | ParrotRenderer | `EntityTypes.java:263 Parrot` / `EntityRenderers.java:185 ParrotRenderer` | 登録・構造のみ。pose/shoulder layers未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2434 MobDef.kind=EntityKind::Parrot` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `ParrotRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| phantom | Phantom | PhantomRenderer | `EntityTypes.java:264 Phantom` / `EntityRenderers.java:186 PhantomRenderer` | 登録・構造のみ。size/animation未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2614 MobDef.kind=EntityKind::Phantom` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `PhantomRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| pig | Pig | PigRenderer | `EntityTypes.java:265 Pig` / `EntityRenderers.java:187 PigRenderer` | 登録・構造のみ。baby/saddle/variant未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1311 MobDef.kind=EntityKind::Pig` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `PigRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| piglin | Piglin | PiglinRenderer | `EntityTypes.java:266 Piglin` / `EntityRenderers.java:190 PiglinRenderer` | 登録・構造のみ。baby/armor/held item layers未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2374 MobDef.kind=EntityKind::Piglin` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `PiglinRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| piglin_brute | PiglinBrute | PiglinRenderer (brute models) | `EntityTypes.java:267 PiglinBrute` / `EntityRenderers.java:194 PiglinRenderer` | 登録・構造のみ。armor/held item layers未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2386 MobDef.kind=EntityKind::PiglinBrute` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `PiglinRenderer (brute models)`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| pillager | Pillager | PillagerRenderer | `EntityTypes.java:268 Pillager` / `EntityRenderers.java:196 PillagerRenderer` | 登録・構造のみ。crossbow pose/layers未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2350 MobDef.kind=EntityKind::Pillager` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `PillagerRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| polar_bear | PolarBear | PolarBearRenderer | `EntityTypes.java:269 PolarBear` / `EntityRenderers.java:197 PolarBearRenderer` | 登録・構造のみ。baby/attack pose未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2150 MobDef.kind=EntityKind::PolarBear` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `PolarBearRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| pufferfish | Pufferfish | PufferfishRenderer | `EntityTypes.java:272 Pufferfish` / `EntityRenderers.java:198 PufferfishRenderer` | 登録・構造のみ。size variants/animation未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1911 MobDef.kind=EntityKind::Pufferfish` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `PufferfishRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| rabbit | Rabbit | RabbitRenderer | `EntityTypes.java:273 Rabbit` / `EntityRenderers.java:199 RabbitRenderer` | 登録・構造のみ。type/Toast/baby未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1706 MobDef.kind=EntityKind::Rabbit` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `RabbitRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| ravager | Ravager | RavagerRenderer | `EntityTypes.java:274 Ravager` / `EntityRenderers.java:200 RavagerRenderer` | 登録・構造のみ。attack/stun layers未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2166 MobDef.kind=EntityKind::Ravager` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `RavagerRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| salmon | Salmon | SalmonRenderer | `EntityTypes.java:275 Salmon` / `EntityRenderers.java:201 SalmonRenderer` | 登録・構造のみ。size/animation未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1866 MobDef.kind=EntityKind::Salmon` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `SalmonRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| sheep | Sheep | SheepRenderer | `EntityTypes.java:276 Sheep` / `EntityRenderers.java:202 SheepRenderer` | 登録・構造のみ。wool/sheared layers未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1362 MobDef.kind=EntityKind::Sheep` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `SheepRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| shulker | Shulker | ShulkerRenderer | `EntityTypes.java:277 Shulker` / `EntityRenderers.java:203 ShulkerRenderer` | 登録・構造のみ。peek/lid animation未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2631 MobDef.kind=EntityKind::Shulker` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `ShulkerRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| silverfish | Silverfish | SilverfishRenderer | `EntityTypes.java:279 Silverfish` / `EntityRenderers.java:205 SilverfishRenderer` | 登録・構造のみ。model/animation未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2254 MobDef.kind=EntityKind::Silverfish` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `SilverfishRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| skeleton | Skeleton | SkeletonRenderer | `EntityTypes.java:280 Skeleton` / `EntityRenderers.java:206 SkeletonRenderer` | 登録・構造のみ。armor/held item layers未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1485 MobDef.kind=EntityKind::Skeleton` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `SkeletonRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| skeleton_horse | SkeletonHorse | UndeadHorseRenderer (SKELETON/SKELETON_BABY) | `EntityTypes.java:281 SkeletonHorse` / `EntityRenderers.java:209 UndeadHorseRenderer` | 登録・構造のみ。baby/saddle layers未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1784 MobDef.kind=EntityKind::SkeletonHorse` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `UndeadHorseRenderer (SKELETON/SKELETON_BABY)`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| slime | Slime | SlimeRenderer | `EntityTypes.java:282 Slime` / `EntityRenderers.java:217 SlimeRenderer` | 登録・構造のみ。size/squish未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1625 MobDef.kind=EntityKind::Slime` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `SlimeRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| sniffer | Sniffer | SnifferRenderer | `EntityTypes.java:284 Sniffer` / `EntityRenderers.java:219 SnifferRenderer` | 登録・構造のみ。sniff/dig/pose animation未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2178 MobDef.kind=EntityKind::Sniffer` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `SnifferRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| snow_golem | SnowGolem | SnowGolemRenderer | `EntityTypes.java:286 SnowGolem` / `EntityRenderers.java:221 SnowGolemRenderer` | 登録・構造のみ。pumpkin layer未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2266 MobDef.kind=EntityKind::SnowGolem` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `SnowGolemRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| spider | Spider | SpiderRenderer | `EntityTypes.java:289 Spider` / `EntityRenderers.java:224 SpiderRenderer` | 登録・構造のみ。eyes layer未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1577 MobDef.kind=EntityKind::Spider` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `SpiderRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| squid | Squid | SquidRenderer | `EntityTypes.java:292 Squid` / `EntityRenderers.java:230 SquidRenderer` | 登録・構造のみ。baby/model/animation未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1816 MobDef.kind=EntityKind::Squid` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `SquidRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| stray | Stray | StrayRenderer | `EntityTypes.java:293 Stray` / `EntityRenderers.java:232 StrayRenderer` | 登録・構造のみ。clothing layer未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1497 MobDef.kind=EntityKind::Stray` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `StrayRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| strider | Strider | StriderRenderer | `EntityTypes.java:294 Strider` / `EntityRenderers.java:233 StriderRenderer` | 登録・構造のみ。saddle/pose未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2194 MobDef.kind=EntityKind::Strider` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `StriderRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| sulfur_cube | SulfurCube | SulfurCubeRenderer | `EntityTypes.java:295 SulfurCube` / `EntityRenderers.java:234 SulfurCubeRenderer` | 登録・構造のみ。size/squish未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2643 MobDef.kind=EntityKind::SulfurCube` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `SulfurCubeRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| tadpole | Tadpole | TadpoleRenderer | `EntityTypes.java:296 Tadpole` / `EntityRenderers.java:235 TadpoleRenderer` | 登録・構造のみ。model/animation未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2278 MobDef.kind=EntityKind::Tadpole` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `TadpoleRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| trader_llama | TraderLlama | LlamaRenderer (TRADER_LLAMA/BABY) | `EntityTypes.java:300 TraderLlama` / `EntityRenderers.java:239 LlamaRenderer` | 登録・構造のみ。baby/decor/saddle layers未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2226 MobDef.kind=EntityKind::TraderLlama` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `LlamaRenderer (TRADER_LLAMA/BABY)`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| tropical_fish | TropicalFish | TropicalFishRenderer | `EntityTypes.java:302 TropicalFish` / `EntityRenderers.java:241 TropicalFishRenderer` | 登録・構造のみ。packed variants/layers未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1880 MobDef.kind=EntityKind::TropicalFish` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `TropicalFishRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| turtle | Turtle | TurtleRenderer | `EntityTypes.java:303 Turtle` / `EntityRenderers.java:242 TurtleRenderer` | 登録・構造のみ。baby/egg/pose animation未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1998 MobDef.kind=EntityKind::Turtle` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `TurtleRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| vex | Vex | VexRenderer | `EntityTypes.java:304 Vex` / `EntityRenderers.java:243 VexRenderer` | 登録・構造のみ。charging/arms pose未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2677 MobDef.kind=EntityKind::Vex` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `VexRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| villager | Villager | VillagerRenderer | `EntityTypes.java:305 Villager` / `EntityRenderers.java:244 VillagerRenderer` | 登録・構造のみ。profession/level/baby layers未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1543 MobDef.kind=EntityKind::Villager` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `VillagerRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| vindicator | Vindicator | VindicatorRenderer | `EntityTypes.java:306 Vindicator` / `EntityRenderers.java:245 VindicatorRenderer` | 登録・構造のみ。held item pose未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2362 MobDef.kind=EntityKind::Vindicator` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `VindicatorRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| wandering_trader | WanderingTrader | WanderingTraderRenderer | `EntityTypes.java:307 WanderingTrader` / `EntityRenderers.java:246 WanderingTraderRenderer` | 登録・構造のみ。llama-spawn/held layers未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2290 MobDef.kind=EntityKind::WanderingTrader` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `WanderingTraderRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| warden | Warden | WardenRenderer | `EntityTypes.java:308 Warden` / `EntityRenderers.java:247 WardenRenderer` | 登録・構造のみ。heart/tendril/biome layers未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2446 MobDef.kind=EntityKind::Warden` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `WardenRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| witch | Witch | WitchRenderer | `EntityTypes.java:310 Witch` / `EntityRenderers.java:249 WitchRenderer` | 登録・構造のみ。held item pose未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1642 MobDef.kind=EntityKind::Witch` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `WitchRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| wither | WitherBoss | WitherBossRenderer | `EntityTypes.java:311 WitherBoss` / `EntityRenderers.java:250 WitherBossRenderer` | 登録・構造のみ。armor/charging layers未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2689 MobDef.kind=EntityKind::Wither` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `WitherBossRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| wither_skeleton | WitherSkeleton | WitherSkeletonRenderer | `EntityTypes.java:312 WitherSkeleton` / `EntityRenderers.java:251 WitherSkeletonRenderer` | 登録・構造のみ。armor/held item layers未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2314 MobDef.kind=EntityKind::WitherSkeleton` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `WitherSkeletonRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| wolf | Wolf | WolfRenderer | `EntityTypes.java:314 Wolf` / `EntityRenderers.java:253 WolfRenderer` | 登録・構造のみ。collar/variant/pose layers未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1651 MobDef.kind=EntityKind::Wolf` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `WolfRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| zoglin | Zoglin | ZoglinRenderer | `EntityTypes.java:315 Zoglin` / `EntityRenderers.java:254 ZoglinRenderer` | 登録・構造のみ。baby/attack pose未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2118 MobDef.kind=EntityKind::Zoglin` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `ZoglinRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| zombie | Zombie | ZombieRenderer | `EntityTypes.java:316 Zombie` / `EntityRenderers.java:255 ZombieRenderer` | 登録・構造のみ。baby/armor/held item layers未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1413 MobDef.kind=EntityKind::Zombie` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `ZombieRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| zombie_horse | ZombieHorse | UndeadHorseRenderer (ZOMBIE/ZOMBIE_BABY) | `EntityTypes.java:317 ZombieHorse` / `EntityRenderers.java:258 UndeadHorseRenderer` | 登録・構造のみ。baby/saddle layers未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1800 MobDef.kind=EntityKind::ZombieHorse` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `UndeadHorseRenderer (ZOMBIE/ZOMBIE_BABY)`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| zombie_nautilus | ZombieNautilus | ZombieNautilusRenderer | `EntityTypes.java:318 ZombieNautilus` / `EntityRenderers.java:266 ZombieNautilusRenderer` | 登録・構造のみ。baby/equipment layers未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2474 MobDef.kind=EntityKind::ZombieNautilus` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `ZombieNautilusRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| zombie_villager | ZombieVillager | ZombieVillagerRenderer | `EntityTypes.java:319 ZombieVillager` / `EntityRenderers.java:267 ZombieVillagerRenderer` | 登録・構造のみ。profession/armor layers未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1461 MobDef.kind=EntityKind::ZombieVillager` | 公式候補未確認（closure blocker） | Pomme `baby: Some`。公式adult/baby model未照合 | `ZombieVillagerRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| zombified_piglin | ZombifiedPiglin | ZombifiedPiglinRenderer | `EntityTypes.java:320 ZombifiedPiglin` / `EntityRenderers.java:270 ZombifiedPiglinRenderer` | 登録・構造のみ。baby/armor/held item layers未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:2398 MobDef.kind=EntityKind::ZombifiedPiglin` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `ZombifiedPiglinRenderer`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |
| player | Player | AvatarRenderer (special; `EntityRenderers.java:61`) | `EntityTypes.java:321 Player` / `EntityRenderers.java:47 createAvatarRenderers` | special registration確認。slim/wide/skin/cape/layers未比較 | 定義あり | `pomme-client/src/renderer/pipelines/entity_renderer.rs:1400 MobDef.kind=EntityKind::Player` | 公式候補未確認（closure blocker） | Pomme `baby: None`。公式adult/baby model未照合 | `AvatarRenderer (special; `EntityRenderers.java:61`)`（直接/継承graph全体は未確定） | 公式state/event集合未照合 |

**数え方と差分の読み方**: `EntityTypes`の登録generic型と`world/entity/**/*.java`のextends chainを照合する母集合は93 living types。公式側は通常provider 91 + Player/Mannequin Avatar special 2という経路で93登録型を説明する（下のroute台帳で再照合する）。Pomme側は`entity_renderer.rs:1272–2895`付近に通常/追加MobDef群があり、Dragon bakerは`renderer/entity_models/flying.rs:731`にある。これらを数えただけではmacro展開後のtype集合やproduction routeの網羅性を証明しないため、回収draftに記したMobDef initializerの総数および92 living/3 nonlivingという集計はこの計画ではcoverage根拠として採用せず、M1でmacro-expanded type setとofficial living setの差分を閉じる。現時点でEnderDragonの通常MobDef entryは確認できず、bakerからspecial production drawへの接続も未確認。Beeは公式登録（`EntityTypes.java:176`）およびPommeの表記載定義（`:2506`）に存在するため、古い一覧からの欠落をimplementation gapとしない。「definitionあり」は描画一致を意味せず、全layer graphと各`setupAnim`/state値の比較は未完了。

### M3有限候補・renderer graph inventory（調査時点）

上表は93 official living registrationを行ごとにinventoryする。variant domainの「公式候補未確認」、official adult/baby modelの未照合、species animation/eventの未照合は未確定集合であり、完了分母を閉じない。Pomme baby欄は表のMobDef `baby: Some/None` inventoryであって、official baby model有無や同一性ではない。Pomme側全MobDef集合とmacro/shared definitionsをexpanded type単位で数えた差分は未完了。M1でliving membershipとnonliving extra typeを独立台帳にして、production routeへの対応を閉じる。未調査speciesは互換でも未実装でもなく未判定とする。

各行の次作業は、その行に記載された正確なofficial renderer classを起点に、同classが参照する`ModelLayers`/model bakeとmodel `setupAnim`、直接・superclass constructor由来layer、type固有entityのmetadata/default/event sourceを追い、同じ条件をPomme `MobDef.kind` source lineからowner field→`entity_extras`/snapshot initializer→production resolver/consumerまで対応付けること。shared renderer family（donkey/mule、horse variants、avatar等）は共通実装とtype固有model key/predicateを別列で閉じる。variant/baby/animation/layer欄が「未確認」の行はそのrendererだけの確認で完了にせず、上記全source handoffを調べてmatrixのevidence/status/pending欄を更新する。

variant候補の最終分類は各typeごとに「固定enum / dynamic registry key集合（registry generation付き）/ metadata numeric domain / なし / 未確認」のいずれかで記録し、現在この表では公式sourceを全数突合していないため全typeを未確認とした。M3では各候補のsource file:line:symbol、合法key/value集合またはpack/registry generation、unknown fallbackを埋める。baby/adultは各official model/layer baking locationとstate predicateを記録する。species event/poseは公式state extractor・model `setupAnim`・Pomme counterpart双方を記録し、数値比較状態を「構造のみ / 差分済 / 未比較」に分離する。現時点のinventoryは構造のみで、animation数値は未比較。

#### 公式renderer constructorsの直接`addLayer`呼出し（read-only棚卸し）

以下は`client/renderer/entity/**/*.java`からの文字列`.addLayer(`の直接呼出しの所在を列挙する機械棚卸し。名前と行位置は原本で再確認可能だが、**literal direct-call countは完全なlayer graphではない**。親classからの継承layer、constructor delegation、条件付きlayer、Player/Mannequin special pathを各登録typeへ展開する継承解決は未完であり、graph closure blockerとして残す。上表のLayer graph familyは対応する登録renderer（shared rendererは同じfamily）を示し、Pomme graphとの差分が済んだ意味ではない。

| Official renderer source | Direct `.addLayer` positions (layer symbol) |
|---|---|
| `AbstractSkeletonRenderer.java` | 21 HumanoidArmorLayer |
| `AbstractZombieRenderer.java` | 22 HumanoidArmorLayer |
| `AllayRenderer.java` | 17 ItemInHandLayer |
| `ArmorStandRenderer.java` | 30 HumanoidArmorLayer, 38 ItemInHandLayer, 39 WingsLayer, 40 CustomHeadLayer |
| `BoggedRenderer.java` | 16 SkeletonClothingLayer |
| `BreezeRenderer.java` | 16 BreezeWindLayer, 17 BreezeEyesLayer |
| `CamelHuskRenderer.java` | 16 createCamelSaddleLayer |
| `CamelRenderer.java` | 22 createCamelSaddleLayer |
| `CatRenderer.java` | 19 CatCollarLayer |
| `CopperGolemRenderer.java` | 31 LivingEntityEmissiveLayer, 41 ItemInHandLayer, 42 BlockDecorationLayer, 43 CustomHeadLayer |
| `CreakingRenderer.java` | 17 LivingEntityEmissiveLayer |
| `CreeperRenderer.java` | 17 CreeperPowerLayer |
| `DolphinRenderer.java` | 17 DolphinCarryingItemLayer |
| `DonkeyRenderer.java` | 28 SimpleEquipmentLayer |
| `DrownedRenderer.java` | 30 DrownedOuterLayer |
| `EndermanRenderer.java` | 25 EnderEyesLayer, 26 CarriedBlockLayer |
| `EvokerRenderer.java` | 17 ItemInHandLayer |
| `FoxRenderer.java` | 45 FoxHeldItemLayer |
| `GiantMobRenderer.java` | 17 ItemInHandLayer, 18 HumanoidArmorLayer |
| `HappyGhastRenderer.java` | 22 SimpleEquipmentLayer, 32 RopesLayer |
| `HorseRenderer.java` | 55 HorseMarkingLayer, 56 SimpleEquipmentLayer, 67 SimpleEquipmentLayer |
| `HumanoidMobRenderer.java` | 36 CustomHeadLayer, 37 WingsLayer, 38 ItemInHandLayer |
| `IllagerRenderer.java` | 14 CustomHeadLayer |
| `IllusionerRenderer.java` | 22 ItemInHandLayer |
| `IronGolemRenderer.java` | 24 IronGolemCrackinessLayer, 25 IronGolemFlowerLayer |
| `LlamaRenderer.java` | 42 LlamaDecorLayer |
| `MushroomCowRenderer.java` | 42 MushroomCowMushroomLayer |
| `NautilusRenderer.java` | 20 SimpleEquipmentLayer, 30 SimpleEquipmentLayer |
| `PandaRenderer.java` | 58 PandaHoldsItemLayer |
| `PhantomRenderer.java` | 17 PhantomEyesLayer |
| `PiglinRenderer.java` | 29 HumanoidArmorLayer |
| `PigRenderer.java` | 29 SimpleEquipmentLayer |
| `PillagerRenderer.java` | 15 ItemInHandLayer |
| `player/AvatarRenderer.java` | 51 HumanoidArmorLayer, 58 PlayerItemInHandLayer, 59 ArrowLayer, 60 Deadmau5EarsLayer, 61 CapeLayer, 62 CustomHeadLayer, 63 WingsLayer, 64 ParrotOnShoulderLayer, 65 SpinAttackEffectLayer, 66 BeeStingerLayer |
| `SheepRenderer.java` | 18 SheepWoolUndercoatLayer, 19 SheepWoolLayer |
| `SlimeRenderer.java` | 16 SlimeOuterLayer |
| `SnowGolemRenderer.java` | 21 SnowGolemHeadLayer |
| `SpiderRenderer.java` | 20 SpiderEyesLayer |
| `StrayRenderer.java` | 15 SkeletonClothingLayer |
| `StriderRenderer.java` | 23 SimpleEquipmentLayer |
| `SulfurCubeRenderer.java` | 35 SulfurCubeInnerLayer |
| `TropicalFishRenderer.java` | 26 TropicalFishPatternLayer |
| `UndeadHorseRenderer.java` | 29 SimpleEquipmentLayer, 39 SimpleEquipmentLayer |
| `VexRenderer.java` | 18 ItemInHandLayer |
| `VillagerRenderer.java` | 21 CustomHeadLayer, 22 VillagerProfessionLayer, 31 CrossedArmsItemLayer |
| `VindicatorRenderer.java` | 17 ItemInHandLayer |
| `WanderingTraderRenderer.java` | 17 CustomHeadLayer, 18 CrossedArmsItemLayer |
| `WardenRenderer.java` | 25 LivingEntityEmissiveLayer, 30 LivingEntityEmissiveLayer, 40 LivingEntityEmissiveLayer, 50 LivingEntityEmissiveLayer, 55 LivingEntityEmissiveLayer |
| `WitchRenderer.java` | 18 WitchItemLayer |
| `WitherBossRenderer.java` | 19 WitherArmorLayer |
| `WolfRenderer.java` | 17 WolfArmorLayer, 18 WolfCollarLayer |
| `ZombieNautilusRenderer.java` | 28 SimpleEquipmentLayer, 38 SimpleEquipmentLayer |
| `ZombieVillagerRenderer.java` | 24 HumanoidArmorLayer, 32 VillagerProfessionLayer |
| `ZombifiedPiglinRenderer.java` | 30 HumanoidArmorLayer |

継承graphは`LivingEntityRenderer`のlayer storage/submitを含めconstructor・superclass chainごとに解決し、conditional branchも閉じたときにのみcompleteとする。renderer source enum/registry variantのfinite candidate closure、全typeのbaby/adult mapping、pose/event source positions、Pomme counterpartの差分状態、およびPNG asset presenceもM3未完了項目であり、layer呼出し棚卸しだけで置換しない。

## 現行実装と差分

### 読み取ったPomme経路

- `Client/pomme-client/src/entity/mod.rs:184–225` はEntityPose 18値とwire ID mappingを持つ（unknown ID → Standing）。`:282–360`のLivingEntityはprev/current positions、head/body yaw、age/baby、pose、registry/variant slot、種別状態を保持し、`:3254–3505`付近でmetadataを反映する。
- `Client/pomme-client/src/app/phases/in_game.rs:6040–6242` はliving storeからEntityRenderInfoを抽出し、位置/yaw/歩行、baby、variant、不可視・hurt/death、equipment、複数species clocks/transformsを渡す。Pose全値はEntityRenderInfo fieldとして確認できず、少なくともこの共通snapshot initializerからrendererへ未伝達。Pose保持だけでは描画互換を満たさない。一方、`entity_extras` `:9265–9425`及びspecies helper `:9427–9624`にspecies単位の状態処理があり、個別経路を調べず「未animation」とは判定しない（例: Sniffer専用stateはLivingEntityにあり、generic extras matchに見えない箇所も含め接続調査が必要）。
- `Client/pomme-client/src/renderer/pipelines/entity_renderer.rs:394–...`の`EntityRenderInfo`と`:952–1020`のAnimationType/MobDef、`:1273–2895`の定義群、`:2914`付近の`EntityRenderer::new`と`:2954`でのdefinition load、`:3586–3625`のplayer skin/effective variant/animation dispatch、`:3972`付近のArmorStand held-item variant選択（一般profileのvariant resolverではない）、`:4166–4183`の一般draw経路（`:4178`でvariantを選択）を確認。`Static` animation tagを「その種にアニメーションがない」根拠にしない。公式モデル`setupAnim`とstate入力の比較対象をまだ網羅していないからである。
- `Client/pomme-client/src/renderer/entity_models/flying.rs:731–...`には`bake_ender_dragon_model()`があり、dragon形状baker自体は存在する。しかし`entity_renderer.rs`のprofile一覧には`EntityKind::EnderDragon`定義が見当たらず、grepした当該production extraction/renderer経路にもEnderDragon profile/draw接続は確認できない。よって現時点の正確な結論は**「baker存在、production接続未確認」**である。「dragon model不在」と断定しない一方、generic draw・特殊draw・別dispatchの不存在も網羅的に証明できていないため「全経路で不在確認」とも書かない。Dragon special pathのproducerからsubmissionまで追跡が必要。
- `Client/pomme-client/src/renderer/entity_models/humanoid.rs:4–5`はHumanoid baker自体がintegration未登録と明記。少なくともhumanoid special modelはrenderer/profile定義だけで本番描画を保証できない。

### 未達差分

1. 共通Poseとspecies pose/stateをひとつの抽出契約で伝えるか、species専用snapshotへ責任分担するかが未決で、現在のsnapshotにはPose enum全体の伝達が見えない。
2. variantは複数種でinteger/index化される。registry key、registry reload generation、unknown fallback/diagnostic、asset lookupの一貫性を比較し切れていない。
3. Baby model/scale適用はspeciesごとに違い、`is_baby` flagだけではofficial baby layer/model/attachmentを満たした証明にならない。Baby判定対象、model geometry、transformとsocketのどちらで縮尺を適用するか、equipment offsetを一度だけ補正する条件が要調査。
4. `EntityRenderInfo`にはoverlay/equipment/animation等の情報が多数あるが、公式`addLayer`の順序、各layer visibility、shader/pass、texture依存世代まで比較済みではない。
5. PlayerとMannequinのspecial avatar、ArmorStandの非典型カテゴリ、Dragonの別drawはgeneric mobの追加だけでは自動的に接続されない。
6. 公式asset treeの実PNG有無を今回の調査で確認していない。公式source内のresource ID/texture pathで母集合を追い、decompiled treeにassetがなければPNG有無をunknownとする。欠落assetを未存在と推定しない。

## 互換にする設計

### 状態からframe snapshotへ

Network/registry境界でprotocol固有indexを無造作にrendererへ流さず、owner entityの型付きsemantic stateへ解決する。registry variantは解決済みresource keyまたはgeneration付きhandleとraw unknown情報を必要に応じて持ち、registry reload後に古いIDを別variantとして誤用しない。未知type/variant/poseはpanicせず、安全なspecies/default modelへfallbackし診断可能とする。不正payloadのfallbackが見えない描画を正常扱いにしないよう診断と比較結果を分離する。

Render extractionは同じpartial tickの一貫したimmutable snapshotを作る。公式必要項目は全Pose(18)、baby/age、variant key、各animation stateのprevious/current ticksまたはstart tick、walk/flap/attack/sleep/fall-flying等種別入力、head/body yaw、hurt/death/invisibility/glowing、light/outline関連状態、equipment stacks/slot、各body/model attachment transform。どの時計がentity tickで進むか、frame補間で導出するかを公式state extractorごとに決定し、renderingがmutable network stateを直接参照しない。

### Profileとdraw graph

全living typeに一意の`LivingRenderProfile`を対応付け、model variant（adult/baby含む）、texture/overlay variant resolver、animation strategy、ordered layer graph、attachment map、render passを持たせる。定義の存在だけでなく、本番renderer registryから全profile到達可能であることを条件にする。renderer/model/layer baking失敗時にはそのtypeを記録し、全透明/未drawをsilently fallbackしない。

公式 `addLayer`の挿入順と提出順、base model/eyes/armor/clothing/saddle/held item/cape等layerの重なり・visibility・passをtypeごとに記録し再現する。texture IDはregistry keyから導出し、resource reload世代をprofile/model/texture bindingでそろえる。reload前のasset handleで新generationのentityを描かない。Skin/cape/item modelそのものはowner側renderer/resource contractから受け、living layerはsocket transform・visible判定を提供する。babyスケールは公式経路に合わせてbase model/layer/socketのどこへ適用するか決め、equipment位置を二重scaleしない。Player/Mannequin wide/slim selectionはskin metadataの所有側を一つにし、model geometry、cape/skin-parts、armor/held attachmentsで同じeffective variantを使う。

ArmorStandは6-part pose/flagsとequipment attachmentを保持し、Living mob generic poseと混同しない。Dragonはhistory samples/phase/partial tickからmultipart transformsを作るspecial renderer契約を追加し、body model、eyes, death dissolve/rays、crystal beamを明示順序でsubmitする。generic culling/one-mesh drawへ押し込まない。Dragon pathについては公式state/extractionとPomme production connectionが確認できるまでprofile完成としない。

### 公式数値・優先順を決めるルール

各公式EntityRenderer/modelの`extractRenderState` / `setupAnim` / `setupRotations` / `scale` / `addLayer`と対応するentity stateを照合し、数値根拠（公式file:line:symbol）を添える。現状静的ソースで必要な項目は次の通りだが、speciesごとの値を未比較のまま統一値に置き換えない。

- 前tick/current tick補間は公式partial tickを用い、角度wrap/shortest-pathを適用する対象を定める。degrees↔radians変換位置と掛けるtransform順序を公式source根拠で記録する。
- head yaw/body yawのrelative clamp、head pitch clamp、head/body rotationとsleep/crouch/swim/fall-flying transformsの適用順を種別profileに決める。
- hurt/red overlay、death animation/alpha、invisible時のbase body対equipment/eyes等の表示、glowing outlineの適用対象とpassを別条件として公式のpriorityに合わせる。
- poseが種別animation・attack/use-item・baby modelと競合する場合の選択順を公式実装から種別ごとに定める。全18 poseをすべてのtypeへ無効なcross-productとして要求せず、公式に到達可能なpose/state行列だけを作る。
- babyはgeneric scaleと専用baby modelの二重適用を避ける。attachment/equipment offsetにも公式同じ座標系で適用する。
- 公式数値根拠が未確認のscale、角度、clock倍率、layer順、alpha threshold、socket補正は**要確定**として残し、このplanの調査段階で仮数値を置かない。

## 実装順序

### M1 — 安全なmetadata境界と欠落drawをなくす

- **目的**: 全93 typeをproduction draw graphに到達可能にし、registry/metadata不正によるpanicとsilent invisible fallbackを避ける。まずDragonの既存bakerから特殊rendererへの接続要否をproducer→extract→submitまで証明する。
- **依存**: inventory表とofficial registration 93件照合（本planで実施）。Dragon接続のowner境界、Player/Mannequin特殊registration確認。
- **scope**: type registry/profile coverage、metadata unknown variantのsafe fallback+diagnostic、profile/model/layer resource binding、EnderDragon special pathの明示登録。server AI/authorityは対象外。
- **source/check**: EntityTypesとEntityRenderers全登録の差分ゼロ、Pomme living setとprofile setの差分ゼロを静的check。invalid metadata/unknown registry key/no rendererを描画panicなく診断するunit-level checkは将来実装時に追加。Dragonが非接続ならbakerが存在してもproduction-visibleとは扱わない。
- **公式比較条件**: A=Pomme/dedicated, B=Steel singleplayer/integratedを別captureで同一entity type/position/visibilityに揃え、全93 typeでbase draw/expected special drawがあることを確認。PNGが無いときはvisual parity判定保留。
- **残り100%条件**: 93/93 profile-to-production registration、special path明示、metadata fallback/diagnostic、base drawが全て成立。

### M2 — common姿勢と状態

- **目的**: common Pose18、tick/frame補間、yaw/pitch、hurt/death/invisible/glow、base/equipment visibilityを公式と同じsnapshot/transformで扱う。
- **依存**: M1のmetadata/snapshot契約。
- **scope**: 18値Pose decode→LivingState→snapshot→animation/transform/discard/overlayへ一貫伝搬。common fieldsの責任はrender extraction、species-specific poseの責任はprofile animation input。
- **source/check**: `EntityPose`全18値とofficial `Pose` ID/state usageを対応表にする。per-type official `setupRotations`, `setupAnim`, overlay/layer priorityとPomme render codeを差分表にし、未調査数値を明示。PommeをPoseで誤って分類しないことと、unknown ID fallbackを検証。
- **公式比較条件**: standing/crouching/sleeping/swimming/crawling/fall-flying/spin-attackのうちそのtypeで合法なpose、hurt/death/invisible/glowingをA/Bで同一stateにし、body/equipment/eyes/outline/passを比較。
- **残り100%条件**: 全合法type×pose/state組合せで値・優先順に根拠があり、snapshot項目のproducerからconsumerまで説明できる。無効cross-productは分母外。

### M3 — 全species / baby / variant / special layer

- **目的**: 全93 typeのbody、baby、registry variant、species animation/event、ordered layers、equipment attachmentを閉じる。
- **依存**: M1 profile registry、M2 common snapshot/pose contract。
- **scope**: tableの各renderer `addLayer`, model layer/model `setupAnim`, `extractRenderState`, baby/adult bakingとresource IDsを照合し、species ownerで必要情報を補う。Player/Mannequin, ArmorStand, Dragon special pathを明示的に含める。
- **source/check**: 93件すべてにofficial model/layer graphとPomme counterpartの比較状態（詳細照合済み/構造のみ/未比較）を付ける。静的モデルをunanimatedの証拠にしない。registry/texture resourcesはreload generationと未知variant fallbackをcheck。decompiled sourceにasset treeがない場合PNG存在をunknownとしてclosure blockerにする。
- **公式比較条件**: variantがあるtypeは全legal registry entries、baby対象はbaby/adult、equipment/layer combinationsは公式に成立する組合せを比較。player slim/wide、ArmorStand部位pose/slots、Mannequin profile/skin parts、Dragon parts/beam/death、happy ghast harness等固有layerを含める。
- **残り100%条件**: 母集合の各typeでmodel/texture asset mapping、baby適用、variant resolver、species event/animation/layers/attachmentが全てclosed。registry variantやasset候補が未確定なら100%宣言不可。

### M4 — 数値・見た目fidelity

- **目的**: profile登録後に残る変換順、角度/clock/scale/texture/layer描画値を公式実装に一致させ、比較可能な完成判定を成立させる。
- **依存**: M1–M3全体。
- **scope**: speciesごとの公式数値根拠を集め、同一state capture、render-pass/texture generationの比較、差分修正。A/B両接続経路を分けて実行し、server authority差とrenderer差を混同しない。
- **source/check**: 公式Java source:file:line:symbolを各数値/優先順に添える。code/static checksに加え、規定比較fixtureの画像・state traceを保管する。数値を一致させたことだけでruntime parityとは主張しない。
- **公式比較条件**: 下記の必須比較ケースをそれぞれ合法状態で複数frame採取し、pose/model bounds、variant texture、layer ordering、attachment transforms、frame interpolationを比較。
- **残り100%条件**: legal matrix全行に比較結果と根拠があり、未解決visual diff、未確認asset/variant集合、未確認special draw path、未確認接続がゼロ。結果を採取していない行はpassではない。

## 完了条件と比較ケース

100%の分母は「公式登録93 type × そのtypeが公式で到達可能なvariant/baby/pose/animation/layer条件」の合法行列。93は固定母集合だが各typeの合法variant/state組合せはregistry/state source照合後にclosureする。pose全18との単純積や存在しないbaby/armor/poseのcross productは分母にしない。variant/asset候補集合が未確定なら対象行列のclosure blockerであり、仮の有限集合にして完了宣言しない。

必須A/B比較ケース（各ケースで同一server state・partial tick・camera・resource generationを記録する）:

- speciesごとのneutral/adult base bodyと代表的なspecies animation cycle/event（wing/flap、walk、attack、sleep/roar/sniff/dig/croak/charge、water/land animation等、公式が持つもの）。Static profileであっても公式animation sourceを確認してから対象外にする。
- baby/adult対象全typeとbaby mount上passenger equipment/held item/attachment。baby model、body scale、socket、riding seat transformの二重補正を確認。
- registry-driven variantsとunknown/removed variant fallback、variant lookupをまたぐdynamic registry reload/resource reload世代切替。
- hurt→death animation、invisible時のbase対equipment/layers、glowing outlineを独立・同時に試し、各pass/visibility priorityを比較。
- ArmorStand:各6部位pose、arms/baseplate flags、各equipment slot attachment。Mannequin: profile/skin/slim-wide/skin-parts/pose。Player: slim/wide skin, cape, equipment, poseとspectator/invisible経路。
- EnderDragon: history-driven multipart movement/turn, phase/landing/takeoff/sitting, hurt/death dissolve+rays, nearest-crystal beam, culling and body/eyes draw sequence。
- 各typeのequipment/layer（saddle/armor/clothing/eyes/collar/wool/held item等公式登録されたもの）の層順・asset key・socket。passenger transformsそのものはphysics/nonliving ownerと共有contractで比較。

現段階は静的ソース・コード構造の照合であり、**live comparison結果はない**。A/B実機で画像・traceを採取した、全93 typeが実行時に描かれた、または互換完成したという主張はしない。

## 依存・未確認事項

- Official source references: `minecraft-26.2-decompiled/src/net/minecraft/world/entity/EntityTypes.java:165–322` (registered entity generics); `.../world/entity/LivingEntity.java:322` and transitive superclass sources (membership); `.../world/entity/Pose.java:18–37` (Pose18); `.../client/renderer/entity/EntityRenderers.java:45–275` (provider/special factory registration); `.../client/renderer/entity/EntityRenderDispatcher.java:45–46,91–106,221–222` (Player/Mannequin map ownership, entity dispatch and model choice); each named `*Renderer.java` under `.../client/renderer/entity/` plus model classes under `.../client/model/` and layer definitions under `.../client/model/geom/ModelLayers.java` (詳細照合は未完)。Dragon: `.../client/renderer/entity/EnderDragonRenderer.java:35–220`, `extractRenderState` and `submit` (history, phase, beam, eyes, death rays, culling special case)。
- Pomme references (base source; recheck symbols as lines move): `Client/pomme-client/src/app/phases/in_game.rs:6040–6242` (living extraction), `:9265–9624` (`entity_extras` and species helpers); `Client/pomme-client/src/entity/mod.rs:184–225` (Pose18), `:282–360` (LivingEntity-owned values), `:3254–3505` (metadata application); `Client/pomme-client/src/renderer/pipelines/entity_renderer.rs:394` (`EntityRenderInfo`), `:999–1010` (`MobDef`/`mob_definitions` start), `:1272–2895` (definition inventory), `:3586–3625` (player choice/dispatch), `:3972` (ArmorStand held-item variant path, not general profile selection), `:4166–4183` (general draw path); `Client/pomme-client/src/renderer/entity_models/flying.rs:731` (`bake_ender_dragon_model`). Production reachability for any one inventory row remains a separate trace.
- Pomme MobDef coverage count after macro/shared definition expansion is pending; do not use the recovered draft's initializer totals as a denominator. The source has a Dragon model baker, but its production connection is unresolved. Official renderer routing is recorded as 91 ordinary providers plus 2 Avatar special routes; full inherited `addLayer` graph and model-state comparison remain pending.
- Java decompiled tree asset PNG availability has not been established; only official source resource IDs are acceptable for initial membership. Any image file presence/absence remains unknown until asset source is inspected.
- Exact numbers/order for species transforms, layer draw graph/pass, registry variant universe, texture reload lifecycle, attachment/socket/baby offsets and all legal species-specific pose/animation conditions require further official source inspection. Until each is grounded, write `要確定`; do not substitute educated guesses.
- Static source checks cannot establish live render equivalence or A/B client-server parity. No Cargo/build/test/application run and no runtime result is authorized in this work item.

## 登録・母集合・互換判定の分離（改訂追記）

「93」は互換済み数でも通常provider数でもなく、Java Edition 26.2の`EntityTypes`登録generic型から継承をたどって得るliving登録型の母集合である。根拠は`minecraft-26.2-decompiled/src/net/minecraft/world/entity/EntityTypes.java:165–322`の登録群、`world/entity/LivingEntity.java:322`、`world/entity/Mob.java:123–124`、`world/entity/Avatar.java:26–27`の継承鎖。`MobCategory`は判定基準にしない。ArmorStandは`EntityTypes.java:170`のMISC登録でも`decoration/ArmorStand.java:66–67`でLivingEntityを継承する。Mannequinは`:248`のMISC登録から`decoration/Mannequin.java:37–38`→Avatar、Playerは`:321`から`Player.java:143–144`→Avatarで、AvatarはLivingEntityを継承する。3者とも母集合に含める。Beeは`:176`登録のliving種。

登録経路は一種類ではない。`client/renderer/entity/EntityRenderers.java:47–54`の`createAvatarRenderers`はPlayerModelType.WIDE/SLIMの2 rendererを作り、`:61`のregistration validationはPLAYER/MANNEQUINを通常`PROVIDERS`登録から除外する（対象外の意味ではない）。同ファイル`:119`はENDER_DRAGONを`EnderDragonRenderer::new`へ登録する。`client/renderer/entity/EntityRenderDispatcher.java:45–46,91–106,221–222`では通常mapとは別のplayer/mannequin mapsを保持し、entity typeに応じAvatar mapへdispatchしskin modelからWIDE/SLIM rendererを選ぶ。公式静的照合では通常 living provider 91 + Avatar special 2 = 93 entity types。closureでは型ごとにrouteを再照合する。Avatarのwide/slimは2つのliving登録typeを意味しない。Dragonは通常renderer registryに登録されても、描画内容は一般livingの一体model経路と同一ではない。

本planでは以下を混同しない。(1) **membership**:公式登録とLivingEntity継承に基づき表の93 typeに入るか。(2) **Pomme inventory**:MobDef/個別dispatch/special drawに記述があるか（存在は到達性・fidelityの証拠ではない）。(3) **compatibility verdict**:production consumerへ到達し合法条件すべての根拠付き比較が終わったか。現在の全typeの判定は未判定であり、未調査を互換/未実装のどちらにも分類しない。MobDefの`baby`、indexやbakerは在庫情報でしかない。Arrow/EndCrystal/ExperienceOrb等の非living定義は分母外。MobDef macro/共有定義の展開とproduction dispatch差分をM1で閉じるまで、回収draftのinitializer数をcoverage証明にしない。

## 法的条件集合と分母の閉じ方

全93種の合法条件行列はまだ閉じていない。行は`official EntityType × renderer/model route × legal variant × age/model choice × reachable pose/state/event × applicable layer/equipment condition × interpolation/pass`。存在しないbaby/pose/equipmentの直積を作らない。有限か動的か、illegal条件が不明な候補は`unverified`として分母を閉じない。

各speciesで公式metadata/default/state/event producer→renderer factory/special dispatch→`createRenderState`/`extractRenderState`/`submit`→model layer/model `setupAnim`→`setupRotations`/`scale`→constructor/superclass由来layer graphを追う。variantは**enum / registry key (static/dynamicとgeneration記録) / numeric domain / none / unverified**で分類し、合法key/value集合またはrange、unknown behavior、source file:line:symbolを記録する。registry世代をまたぐ値は再解決規則も記す。asset fileの未発見をvariant不在の根拠にしない。

baby/adultは公式predicate・model layer/baker/scaleにより`adult only / baby model / shared model with scale / other rule / unverified`へ分類し、各legal age stateでのmodel/texture choiceとlayer/socket/equipment scaleを記録する。pose/event台帳はmetadata/state transitionから到達可能なposeを絞り、併存条件、優先順、tick clockを記録する。全18 poseとの無効cross-productは禁止。公式に到達可能性を閉じられない候補はunverifiedのままblocker。

Layer台帳は直接`addLayer`呼出しに留まらず、superclass constructorからの継承とsubmit順をtypeごとに展開する。各layerのsymbol、asset/model、predicate/visibility、equipment/effect/invisibility/glow/hurt/deathとの条件、pass/blend、baseとの順序、baby/socket補正を記録。effects layer等をsourceで確認できないとき「適用なし」と推測せずunverifiedとする。`LivingEntityRenderer.java:46,55–56,100–105`はlayer listの保持・登録・順次submitを示す。条件は各`shouldRenderLayers`/`RenderLayer` sourceで確認する。

分母は93行それぞれのvariant domain、age/model、legal pose/state/event、model/texture候補、conditional/inherited layer graph、Pomme production consumerと根拠が埋まり、未解決候補ゼロになって初めて固定する。将来の比較は固定合法集合の全行とし、未実施はpassではない。

## Production経路・引き渡し点

- **公式登録/producer**: 上記`EntityTypes`集合を継承解決し、通常providerとPlayer/Mannequin Avatar dispatchを区別する。公式`LivingEntityRenderer.java:267–270`は`entity.getPose()`をrender stateへ保存する。`Pose.java:18–37`はID 0..17の18値。
- **Pomme state/metadata**: `Client/pomme-client/src/entity/mod.rs:184–225`に18値とwire ID mapping (unknown→Standing)、`:282–360`にLivingEntity-owned state、`:3254–3505`にmetadata applicationがある。`sniffer_state`は`:304,512,3348`にfield/default/metadata armがある。具体的kind/index、spawn default、event、retained/interpolated値をconsumerまでspeciesごとに追う必要があり、全typeで閉じたという意味ではない。
- **extraction**: `Client/pomme-client/src/app/phases/in_game.rs:6040–6242`がliving storeを`EntityRenderInfo`へ変換し、`:6085`で`entity_extras`を参照、`:6123`以下でsnapshotを構築する。`entity_extras`とhelpersは`:9265–9624`。LivingEntity.poseは保持される一方、共通initializerにはPose enum fieldが見えずcrouching/sleepingとspecies情報などが抽出される。これはtrace課題であり、それだけで互換性違反とは断定しない。consumerが必要とするposeの代替情報/別経路と合法pose別drawを調べる。
- **snapshot/consumer**: `Client/pomme-client/src/renderer/pipelines/entity_renderer.rs:394–...`の`EntityRenderInfo`、`:999–1010`のMobDef定義と`mob_definitions()`、`:1272–2895`の定義群、`:2914`近辺の`EntityRenderer::new`/`:2954`のload、`:3586–3625`player dispatch、`:3972`ArmorStand個別処理、`:4166–4183`一般drawは回収draftの調査anchor。lineは編集で移動し得るので実装時はsymbolで再確認する。一般draw・special dispatch・baker・assetをproducerからactual submissionまで追い、MobDef/variant_index/baby flagだけで到達性やfidelityを認定しない。
- **avatarとdragon**: Official `EntityRenderers.java:47–54,61` Player/Mannequin special dispatchと通常provider validation除外を登録・実dispatch両面で確認する。AvatarRenderer wide/slim、skin parts、cape/equipment/layersを両type別に照合する。Pomme extractionのavatar起点は`in_game.rs:7860–7900`付近。Official `EntityRenderers.java:119`はDragon登録。`EnderDragonRenderer.java:35–220`のspecial submit/extractionはhistory/phase/beam/death rays/culling routeを持つ。Pomme `entity/mod.rs`はphase metadata/boss state、`renderer/entity_models/flying.rs:731`はDragon bakerを持つが、producer→retained history→snapshot→special consumer→drawの接続は未確定。model bakerはproduction reachabilityではない。

## Obligation matrix — evidence / consumer / finding / pending

| Obligation | Official evidenceと必要なclosure | Pomme evidence/consumer | 現状 | 未完了の検証・acceptance |
| --- | --- | --- | --- | --- |
| Membership/dispatch | `EntityTypes.java:165–322`; genericとtransitive superclass。`EntityRenderers.java:47–61,119` | `entity_renderer.rs` MobDef及び実production dispatch | 93はmembership。Player/Mannequin special、ArmorStand MISC living、Dragon dedicated route | official living集合とproduction route集合を突合。macro-expanded entries、余剰/欠落ID、special routeも閉じて93/93 |
| Metadata/default/event | 各entityのsynced metadata、constructor default、event transitions | `entity/mod.rs:282–360,3254–3505`; event handlers/species fields | shared pose/stateあり。全species producer/default/event→consumerは未閉鎖 | 全render-relevant inputのkind/index/event/default/owner/update order/unknown handlingをspecies別記録しsnapshotまでtrace |
| Variant | official entity/model stateとenum/registry/numeric source、resolver | `entity_extras` `in_game.rs:9265–9624`; `EntityRenderInfo.variant_index`; resolver/draw | tableの候補は未確認。MobDef/indexはdomain証明でない | 93行をenum/registry/numeric/none/unverified分類。合法集合・fallback・generation・resourceを根拠付きで閉じる |
| Baby/adult | age predicate、official layer bake/model selection/scale | MobDef baby値、baker、snapshot `is_baby` | baby値はofficial eligibility/model証明でない | legal age states、model/texture choice、scale/socket/equipmentを一度だけ適用し両side比較 |
| Pose/common state | `Pose.java:18–37`; `LivingEntityRenderer.java:74–105,155–185,267–272`; species override | `EntityPose` `entity/mod.rs:184–225`; stored pose; snapshot initializer `in_game.rs:6123–6242` | 18値は確認、full enum→snapshot handoff未解決。自動的なincompatibility判定はしない | legal pose by type、mapping/代替抽出/consumer、transform priorityとsource-to-consumer traceを閉じる |
| Animation/state clocks | per species official `extractRenderState`,`setupAnim`,entity tick/event transitions | `LivingEntity` fields、`entity_extras`/helpers、`EntityRenderInfo` clocks | species state/helperあり。完全なconsumer/numeric closureなし。Static tagは証明でない | event、retained prev/current、clock/interpolation、model consumer、formula全件照合 |
| Inherited/conditional layers | renderer constructor/superclass、`LivingEntityRenderer.java:46,55–56,100–105`;各layer submit/predicate | MobDef overlays/equipment inputsと実renderer consumer | draft direct addLayerはgraph全体ではない | typeごとに順序と継承graph、equipment/effect/invisibility/glow/hurt/death predicate/pass/visibilityを閉じる |
| Player/Mannequin | `EntityRenderers.java:47–54,61`; `EntityRenderDispatcher.java:45–46,91–106,221–222`で2 map生成/選択、AvatarRenderer、skin/equipment source | extraction `in_game.rs:7860–7900`; dispatch `entity_renderer.rs:3586–3625` | official special dispatchは確認、Pomme全draw parity unknown | 両type別にdispatcher→wide/slim、skin parts、cape/equipment、pose/visibility→drawを比較 |
| EnderDragon | `EntityTypes` registration; `EntityRenderers.java:119`; `EnderDragonRenderer` history/phase/beam/death/culling | phase/boss state、`flying.rs:731` baker。通常MobDef routeでの接続なし、special production draw未確認 | official専用pathとPomme baker/metadataはある。end-to-end到達性unknown | producer→history/phase→snapshot→special consumer→multipart/eyes/beam/rays/dissolve/cullingを通しで追う |
| Asset/reload | official resource IDs, model/texture refs, layer graph | profile asset paths/resource binding generation | decompiled source asset有無は未確認 | authoritative asset sourceで全候補列挙。未入手はunknown。reload generation/fallbackを追跡 |

## 実装設計と依存milestones

設計責務は分離する。registry completenessはtype/dispatch/resource bindingの責任、pose/common state transportはentity ownerからimmutable frame snapshotまで、species/baby/variant/layer coverageはrender profileとlayer/attachment graph、numerical animation fidelityは公式tick/partial tick・数値・transform順序を適用する責任とする。これを巨大MobDefや`variant_index`の拡張だけに押し込まない。各値のauthoritative ownerと更新窓口を定め、rendererはmutable network stateを再推定せず一貫したpartial-tick snapshotを消費する。typeはgeneric profileか明示special rendererの一つへ到達する。

1. **M1 — registry completeness / production reachability** (先行): 93公式登録/継承台帳とPomme living/extra定義集合を差分化し、macro expansionを含む全profileまたはspecial routeを登録する。Player/Mannequin AvatarとDragon special routeを明示。Deliverable: membership+route map、余剰/欠落一覧、unknown metadata/variant fallback+diagnostic方針、resource/model load failureの可視化。Gate: 公式living setとproduction route setの厳密一致、unresolved dispatch 0。model fidelityは未達のまま別評価。
2. **M2 — common pose/state snapshot** (M1のdispatch境界後): owner state→同一partial tickのimmutable snapshot→consumerを定め、Pose18、spawn defaults、metadata/event、retained previous/current clocks、hurt/death/invisible/glow/equipment共通値を引き渡す。species-only stateはspecies ownerに置き、契約したsemantic inputとしてsnapshotへ渡す。Gate: legal stateごとproducer-consumer traceが全typeで閉鎖、未説明の消失/再推定/二重補正0。pose保持だけでtransform一致とはしない。
3. **M3 — species/baby/variant/layer closure** (M1,M2依存): 93行すべてのofficial model/texture/variant/baby/layer/equipment/event台帳を閉じ、profileかexplicit special routeへ対応。Dragon multipart/history、ArmorStand部位pose/slots、Player/Mannequin Avatarは必須deliverables。Gate: legal condition universeを確定し各行にofficial evidenceとPomme production consumerを記録、未調査0・候補漏れ0。
4. **M4 — numerical animation fidelity + differential verification** (M1–M3依存): official state/model formulas、previous/current interpolation、angles/clocks、transform order、overlay/layer/pass/alpha/light、attachment offsetsを根拠付きで実装。後続の許可されたruntime A/B differentialを行う。Gate: fixed legal matrix全行の結果を記録、未解決差分0。A=Pomme client→official dedicated serverとB=Steel singleplayer client→official integrated serverを分けて比較し、server state/authority差とclient render差を混同しない。許可・fixture・比較基準が未準備ならpendingであり完了としない。

現時点は静的source調査とplan修正のみ。コード実装、Rust check、runtime A/B、timing計測はいずれも未実施。future A/Bは同一条件を記録し、条件不一致は反例または判定不能として保存する。分母から除外してpassとしない。

## 反例・リスク・範囲外

- **現時点の反例/注意**: ArmorStand/Mannequin/PlayerはMISCでも母集合に属する。Beeは公式登録・renderer route・Pomme definition inventoryに存在し、古い表の欠落は実装漏れの根拠ではない。Dragonはofficial dedicated rendererとPomme baker/phase metadataがあるがproduction draw接続は未証明。LivingEntityにPoseが保持されても共通snapshotにenum fieldが見えない一方、crouching/sleeping等へ抽出される。このpose handoffは調査課題で、直ちに非互換と確定しない。
- **リスク**: MobDef/texture row=coverage、enum/index=registry domain、baby flag=official baby model、direct `addLayer`=継承layer graph、Static tag=no animation、PNG未発見=asset不存在、baker=production connection、と誤認すること。行列やsourceにunverifiedが残る限り分母/100%を閉じない。
- **範囲外**: server AI/戦闘/死亡authorityの新規模倣、spawn/physics/passenger transformの所有、item asset/skin/cape/resource loaderそのものは別owner。本機能は境界input/attachment/reload contractを扱う。コード変更と今回の実機比較はしない。
- **比較結果**: static reviewのみ。runtime画像/state trace、timing、A/B parity resultはない。docs planのcheckが成功しても動作一致は証明しない。
