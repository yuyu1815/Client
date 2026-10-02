# Block entity 未描画: 最小実装案（調査のみ）

調査日: 2026-10-14（作業環境の日時）
対象コード: `C:/Users/yuzum/Desktop/mine_rust/Client`
根拠レポート: `C:/Users/yuzum/Downloads/Pomme_0a1aee9_全文と対象別詳細報告/Pomme_0a1aee9_full_report`
対象: batch-01 の bell 本体・campfire 煙、batch-02 の decorated_pot 本体・enchanting_table 上の本。

> 制約: 調査・提案のみ。Skull担当が共有ファイルを編集中のため、Rustコードは変更していない。campfire煙の原因は特定扱いしない。

## Findings

### 記録上の症状と境界

原 `records/BLOCK-*.json` は全て `classification=observed_runtime_difference`、`independent_root_cause_confirmed=false`。通常描画（render-probeなし）、晴れ・時刻6000・床stone、固定カメラ `(0.5,64,10.5)`, yaw 180 / 1280x720 の静止画比較であり、機能・アニメーション全体の検証ではない。

| Batch / 原記録 | 記録の観測 | 現状コードから言えること |
|---|---|---|
| batch-01 `records/BLOCK-bell.json` | `bell[attachment=floor,facing=south]` の支持フレームは見えるが本体が見えない | `rendered_kind` は Bell を含まない。`is_block_entity_block` は Bell を含み、チャンクメッシュが欠落モデル警告/particle cube fallback を避けているため、BE描画にもチャンク描画にも本体の責任主体がない可能性が高い。根本原因は未確定。 |
| batch-01 `records/BLOCK-campfire.json` | `campfire[lit=true]` と `soul_campfire[lit=true]` で炎/本体は見え、煙柱がこの撮影で見えない | 本体の新規BEモデル化はこの記録の症状には不要。一般particle処理は既にあるが、campfireのambient生成が別途あるかは切り分けが必要。 |
| batch-02 `records/BLOCK-decorated_pot.json` | decorated_pot本体がPommeで見えない | `is_block_entity_block("decorated_pot")` はtrue、`rendered_kind` はNone。通常のブロックモデル経路は特殊ブロックとして抑制される一方、BE pipeline登録もないのが最小の不整合候補。 |
| batch-02 `records/BLOCK-enchanting_table.json` | enchanting_table本体は見え、本のアニメーションが見えない | enchanting_table本体はchunk blockstate model経路に残る。`BookPreviewPipeline` はenchanting GUI用の別描画であり、world BEとしての本を描画しない。 |

batch JSONの確認: `docs/report-audit/batch-01.json` は80 objects/128 records、描画recordsに bell を含む。`docs/report-audit/batch-02.json` の `BLOCK-ENTITY-RENDER` clusterはdecorated_pot/enchanting_table（およびdragon_head）を原因未確定候補として列挙。batch-02の表示グループにもench-tableとdecorated_pot、campfire組がある。バッチ名は今回対象の対応付けに用い、根本原因確定の根拠にはしていない。

### コード経路

- `pomme-client/src/world/block_entity.rs::rendered_kind` がBE-only描画・合成エントリの対象を決める。現状 Bell/DecoratedPot/EnchantingTable は未登録。
- 同ファイル `is_block_entity_block` はこれらを既に含む。chunk mesherに対する「BEブロックなのでfallback cubeにしない」判定であって、実際に描画することを保証しない。
- `sync_block_entity` は `rendered_kind` 対象ならブロック更新時に空NBTでエントリを合成。`is_rendered` は合成エントリのstale判定を担うため、種類追加時は同じKindを追加する必要がある。
- `pomme-client/src/renderer/pipelines/block_entity.rs::kind_definitions` はChest/Conduit/Skull/Shulkerだけを通常 `KindEntry` として構築。`variant_for_block`, `yaw_for_block`, `BlockEntityRenderInfo` 抽出（`app/phases/in_game.rs`）, pipelineのmodel/texture bind/drawが別々の接続点。
- 通常modelは `BlockEntityRenderInfo` を抽出し `BlockEntityPipeline::draw` がblock-relative model matrix、`BakedEntityModel::compute_part_transforms`、`KindEntry` texture descriptorで描く。model conventionは `block_entity_model.rs::ModelConvention::{EntityYDown,BlockYUp}`。block-only BE geometryには `BlockYUp` と `chest_matrix` 相当（center位置 + `rotY(-yaw)` + `(-0.5,0,-0.5)`）を再利用するのが一番近い既存経路。
- `pomme-client/src/renderer/pipelines/book_preview.rs::BookPreviewPipeline` はGUI用PIP pipeline。画像は `minecraft/textures/entity/enchantment/enchanting_table_book.png`。7パーツのmeshはprivate `build_parts()`、各描画時に `BookPreview { open, flip }` から `BookModel.setupAnim`相当をCPU pose。すでに存在するGUI機能をworld本の実装済みと見なさない。
- NBT経路は既存: `net/handler.rs` の `ClientboundGamePacket::BlockEntityData` → `NetworkEvent::BlockEntityUpdate`、`app/core.rs::update_block_entity` は同種のStoredBlockEntityへNBTを保存/更新。chunk snapshotのBE NBTも `StoredBlockEntity::new` に入る。Potに必要なのはまず `StoredBlockEntity`から描画用4面装飾を安全に読むこと。特別なpacket種は不要。
- 一般server particleも既存: `net/handler.rs`でLevelParticlesをdecode、`app/core.rs`で`ParticleStore::add_particles_from_packet`、`particle.rs::ServerParticleKind::Smoke` (registry id 69 in 26.2) → `Particle::smoke` → `ParticleStore::tick_with_entity_lookup` / `extract`、`renderer/pipelines/particle.rs`で描画。これはserver送信粒子処理であって、client-onlyのBlock.animateTickを置き換えない。

## Minimum implementation instructions for main

### Bell body (本体のみ。初回は静止meshまで)

1. `world/block_entity.rs`: `rendered_kind("bell") = Some(BlockEntityKind::Bell)`、`is_rendered`へBell追加。`is_block_entity_block`に既にあるのでリスト重複は不要。block update時に合成エントリが作られることをテスト。
2. `renderer/pipelines/block_entity.rs::kind_definitions`: Bell `KindDef`を追加し、`block_entity_model.rs`に `bake_bell_model()`。公式の`BellModel`寸法/UVを26.2 client assets/model sourceに照合してから実装する。候補textureは `minecraft:textures/entity/bell/bell_body.png`（chunk atlasの`entity/bell/bell_body` spriteも `renderer/chunk/atlas.rs` が認識）。縦の支持フレームはchunk/blockstate側に既に見えるのでBE meshは本体だけにし、重複させない。
3. block stateの`attachment` (`floor|ceiling|single_wall|double_wall`)と`facing`で向きを作る。追加の `yaw_for_block` 分岐か、Bell専用transform関数をrender extractionで呼ぶ。Bellの傾きは固定モデル変形ではなくtick animation入力と分離する。
4. 初回テストはfloor/southの本体geometry、texture参照、block updateからdraw itemまで。wall/ceiling向きは状態表で変換単体テスト。

### Bell swing animation (本体mesh後の小さな別段階)

- 専用 `BellAnimStore` を `BlockEntityAnimStore` にまとめるか同ファイルの既存container mapに増築せず、小さな position keyed storeを作る。`app/phases/in_game.rs`のtickでprev/currentを進め、render extractionでpartial-tick値を `BlockEntityRenderInfo` に渡す。
- `ClientboundBlockEvent` は既に `NetworkEvent::BlockEvent {pos, action_id, action_parameter}` として届くが `app/core.rs`ではaction_id=1をchest/shulkerだけに消費。Bellのblock event ID / parameterと vanilla trigger semanticsは実装前に26.2参照実装で確認し、`app/core.rs`にblock stateがBellの時だけanimation triggerを追加する。専用packet追加は不要。
- テスト: bell eventでanimation開始、prev/current部分tick補間、別座標/他kindのイベント非干渉、四つのattachment/facing姿勢。

### Decorated pot body + NBT sherds

1. `rendered_kind("decorated_pot") = Some(BlockEntityKind::DecoratedPot)`、`is_rendered`にも追加。decorated_potは既に `is_block_entity_block` true。sync_block_entityで設置直後は空NBT=全側Brickとなる挙動を保つ。
2. `StoredBlockEntity`に4つのdecoration identifier（または描画時にNBTから一度抽出した固定長配列）をキャッシュし、`new`と`update_nbt`双方で更新する。26.2のDecoratedPotBlockEntityのNBTキー/型を参照実装で確認する。過去の形式は`sherds`の4要素item-id listだが、これを無検証で現行プロトコルへ決め打ちしない。空/欠損/不正/長さ不足はBrick扱いにフォールバック。
3. `kind_definitions`にDecoratedPot entry、`block_entity_model.rs`に公式geometry/UV bake関数を追加。向きはblockstate `facing` (stateの回転)を使う。画像を1枚の不透明pot色に置換せず、各面がbrickまたはshard motifとなる公式テクスチャ/UVを再現する。
4. texture候補のrootは `minecraft:textures/entity/decorated_pot/`。base/side とシェルド柄texture（`angler`, `archer`, `arms_up`, `blade`, `brewer`, `burn`, `danger`, `explorer`, `friend`, `heart`, `heartbreak`, `howl`, `miner`, `mourner`, `plenty`, `prize`, `scrape`, `sheaf`, `shelter`, `skull`, `snort`）を26.2 jarの実ファイル名・UVに照合。ここはtexture一覧を報告のみから確定できていないので、ファイル名をハードコードする前の資産照合が必須。
5. 最小renderer拡張: 既存`KindEntry`の単一variant descriptorだけでは4面ごとのNBT textureを選べない。全pot textureをrenderer生成時に一括ロードし、4つのpot part rangeごとに対応slotをbind/drawする専用処理を追加する（21^4 texture組合せや配置ごとのGPU texture生成はしない）。`BlockEntityRenderInfo`には既に抽出した4面texture indicesを渡し、NBTをdraw loopで都度parseしない。
6. packet/NBT: 新packet不要。通常設置時空壺は合成エントリで描画できる。装飾済み/更新されたpotは既存chunk BE tagまたは既存`BlockEntityData` packetで装飾が送られることを確認する。既存 `update_block_entity` はNBT Noneを無視するので表示更新はNBT付き更新に限られる。wire packet追加は不要。
7. テスト: rendered_kind/is_rendered/syncの対応、NBT 4面の順序→identifier mapping（4種混合、Brick、欠損/不正/余分要素）、update_nbt後のrefresh、facing回転、各texture slotとpart range対応。画像差分は空potと4種の柄を同じカメラで見る。

### Enchanting table book (table blockモデルは触らない)

1. `rendered_kind`/`is_rendered`にEnchantingTableを追加して合成BE entryを用意するか、既存block model経路を保持したままbook-only world draw itemを専用抽出する。main推奨は後者相当: 本だけ描き、chunkにあるtable本体をBE modelで覆わない。`is_block_entity_block`は追加済み。
2. `book_preview.rs::build_parts()`の7部位geometryと同textureを再利用できるようmesh/pose計算を共有する。GUIの画面射影/scissorは再利用せず、worldのcamera pipelineでdraw。table centerに配置するpitch/yaw/scale/translationを公式`EnchantingTableBlockEntityRenderer` pose chainと照合し、`block_entity_model.rs`のmodel convention/anchor-relative matrixへ適用。
3. `BookPreviewPipeline`の`open`/`flip`はGUI state依存でありworld animation stateではない。client-only animation stateをtable positionごとに保持し、tickでplayerが近くにいるとき公式algorithm通りrotation/open/pageFlipを更新してprev/current補間をdrawへ渡す。table IDLEの固定本だけから始めるならstate/storeを追加せず、一段階として本体の静止meshを出し、その後animationを加える。
4. packet/NBT不要: enchanted tableのbook poseは通常のplayer proximityに応じたclient-side animation。NBT値をServerBlockEntityUpdateから待たない。block eventは不要。画面メニューの既存`BookPreview`は別スコープ。
5. テスト: mesh頂点/texture load、table中心とyaw/pitch変換、tick近接・遠隔時のstate update、page flip/open interpolationの決定論的pose test。通常描画でtable bodyが一つ・world bookが一つ、GUI enchantment screenも従来どおり描画される確認。

### Campfire smoke: 別経路として切り分ける（原因未確定）

1. 描画対象のcampfire/soul_campfire本体は原記録で見える。`rendered_kind`へ追加したりentity meshを足したりせず、煙だけを追う。
2. 既存のserver送信煙は`LevelParticles`経路で処理されている。server/client captureまたはpacket recorderで観測時間帯にCampfireSmoke particle packetが来ているか、ID mapping/limiter/ParticleStoreの粒子生成数・extract数・sprite region・drawを順に確認する。
3. 別途client ambientは一般LevelParticlesとは異なり、Block.animateTick等からローカル生成する経路。調査時点で`pomme-client/src`にcampfire-specific ambient smoke producerの明示実装は見つからなかったが、静止画だけではserver送信/ambientのどちらが対象だったかを決められない。公式のcampfire/soul campfire animateTick条件（`lit`, `signal_fire`）、確率、位置、particle type（cosy/signal smoke）とPommeのambient tick呼び出しを照合してから不足を判断する。
4. もしpacketが来ているのに消えるなら既存LevelParticles/ParticleStore描画側を修正。もし公式と同じくclient ambientなのにPomme producerがないと確認できた場合のみ、chunk内のlit campfireをtickする軽量ambient producerを追加し既存`ParticleStore`へspawn。dedicated campfire-smoke protocol packetは作らない。
5. 原記録のlimitationsは「煙なしは一時点の画像で、全粒子処理欠落とは断定しない」。したがってこの候補の状態は要観測・未確定のままにする。

## 最小テスト・確認計画（実装担当向け）

- unit: `rendered_kind`/`is_rendered`/`sync_block_entity`新Kind整合、asset-index/texture names、model ranges・UV・facing変換、book/bell tick animation、pot NBT extraction。
- integration: chunk loadからBE entry synthesis → block entity data update → render extraction → expected drawsまで。NBTなしpot、NBTありpot、NBT updateを分離。
- runtime: 元レコードの同配置/視点を再現してPommeと公式静止比較。bellは静止と叩いた後、potはempty/4 sherd faces、table bookはplayer近接/遠隔とページ変化を追加。campfireはpacket受信とambient生成を別々に記録。
- この依頼の調査作業ではbuild/test/checkや実ランタイム検証は実施していない。記録は全て「観測症状/実装候補」であり、修正済み・原因確定とは言わない。
