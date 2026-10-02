# batch-02 追加原因調査：ladder / kelp / honey

調査対象は梯子の `MOVE-ladder-walk_approach` と `MOVE-ladder-jump_approach`、`PICK-0199`、`RENDER-honey` のみ。decorated pot / enchanting-table world book / bell 本体は別担当範囲として調べていない。現コードと既存リファレンス・記録を読むだけの調査で、修正・build・test・commitはしていない。

## 結論の要約

| 対象 | 判定 | 最小差分案 |
|---|---|---|
| 梯子W-only上昇なし | 強い原因候補。水平衝突そのものは検出・保持されるが、梯子に対する vanilla の「水平衝突またはjump中ならY速度を0.2にする」後処理が欠けている。報告の反復実測とコード位置も一致。 | `apply_collision_with_context` の衝突後、更新済み位置で climbing 判定し、`(horizontal_collision || jump)` かつ climbable なら `velocity.y = 0.2`。既存の粉雪分岐を壊さず条件を共有する最小変更が候補。 |
| kelp PICK-0199 | front-vs-none比較としては**無効な証拠／Pomme側未確定**。公式raw snapshotがMISSを明記し、Pommeはhit snapshotなし。Pomme固有の実機差とは扱わない。現コードにfluidを選択対象から除外するpredicateはなく、kelpはempty-outlineにもならず、形状未定義時はfull-cube fallback。 | コードを直さず、試験をやり直す。直近の公式・Pomme各々の `InteractionState.target` / `BlockHitResult` を記録してから、破壊結果を補助証拠として比較。 |
| honey透過 | 強い原因候補。26.2のhoney textureは全texelがpartial alpha。chunk mesherはこれをcutoutへ送り、chunk solid/cutout pipelineはblend無効。alpha>=0.5のtextureはdiscardされず、出力色がblendなしで書かれ不透明に見える経路と一致。 | model quadをalpha-partial時に汎用translucent geometryへ分離し、alpha blend / depth-test / depth-write-offのpassへ送る。既存water passはfluid専用なのでそのまま流用しない。 |

---

## 1. 梯子：水平衝突は失われていない。climb impulseの消費漏れ

### 報告データ

`records/MOVE-ladder-walk_approach.json` は有効な開始点 `(300.5,64,-1.5)` からWを3秒保持した比較を3回ずつ記録。公式はYが64から `69.1223–69.6575` に上昇、Pommeは全3回Y=64のまま。梯子は `ladder[facing=north,waterlogged=false]`、z=0、当たり面のz範囲は `.8125–1.0`。Pommeの終了zは `.5125` でプレイヤーAABBが梯子のcollision boxへ達している。`MOVE-ladder-jump_approach` も同系列の差だが、W+Spaceの過上昇を含み、W-onlyとは原因を分けて再確認する必要がある。

### 現コードの流れ

1. `pomme-client/src/physics/movement.rs::is_on_climbable` はプレイヤー足位置をfloorしてblock idを調べ、`ladder` をtrueとしている。
2. `tick_land` はtravel前にこの関数を呼ぶ。climbing時はX/Zを `[-0.15,0.15]` にclamp、Yの落下を最低`-0.15`へ制限し、sneak中の下降を0にする。これはvanilla `LivingEntity.handleOnClimbable` と対応する。
3. 接近初tickはまだ梯子セルに入っていない。`apply_collision_with_context` は `resolve_collision_for_player` の結果から `collided_x/z` を算出し、`horizontal_collision = collided_x || collided_z` を作り、`player.horizontal_collision` に保存する。したがってこの経路でflagが失われたのではない。
4. 衝突後、位置を `player.position += resolved` で更新してから後処理する。現状の `horizontal_collision` を使う垂直impulseは粉雪+革靴だけで、`player.velocity.y = 0.2` を設定する。梯子用の同条件分岐がない。梯子に対しては「flagはtrueだが未消費」が最も直接的な差分。
5. tick後半の `tick_land` は重力を引くので、衝突後に梯子impulseを入れなければ上昇せず、次tickには落下clampのみが働く。Spaceはtick前半の `jump_held && is_on_climbable` で0.2を与えるため、W-onlyだけ抜ける現象を説明し得る。

### vanilla referenceとの照合

既存のSteelMC vanilla port:

- `third_party/SteelMC/steel-core/src/entity/living_entity.rs::default_living_on_climbable`：spectator以外でCLIMBABLE tag、または使用可能なtrapdoor梯子を認識し、最後に接触した梯子位置を保持。
- `LivingEntity::handle_on_climbable`：Y下降を`max(-0.15)`、X/Zを各±0.15に制限。プレイヤーの梯子滑り抑止条件では下降を0。
- `handle_relative_friction_and_calculate_movement`：entityを動かして `MoveResult` を得た**後**、`result.horizontal_collision || self.is_jumping()` かつon-climbable（または条件付きpowder snow）なら`movement.y = 0.2`。

Pommeのclamp値はこのreferenceと一致。一方、`apply_collision_with_context` は後処理に粉雪だけを実装し、climbableを条件に含めない。referenceで確認できた0.2/clamp仕様を、単にtick前にジャンプ時のみ0.2を入れる現在の処理ではカバーできていない。

### 原因確度と最小差分案

- **W-onlyの静的原因確度：高い**。記録は3反復で衝突到達と停止を観測し、実装上もpost-move horizontal collision時のladder impulseが欠落。
- **jump_approachの過上昇の確度：別途要追跡**。報告の初期姿勢・grounded/`jump_from_ground`併発条件とtick毎のY速度を照合しないままW-only修正で解消したと宣言しない。
- 最小候補は`apply_collision_with_context`の、位置・flag更新後、重力後処理前の既存powder-snow impulse付近。vanilla式に合わせ `(horizontal_collision || jump)` と `is_on_climbable(chunks, player.position.into())` の組み合わせで梯子時にY=0.2を与える。現在の粉雪・革靴条件は別条件として残す。
- `horizontal_collision`の定義、軸clipや`resolve_collision_for_player`を変更する根拠は今のところない。

### 修正後に必要な確認

実機手順 `MOVE-ladder-walk_approach` の有効接近開始のみで、W-onlyの3反復とY/velocityのtick軌跡を比較。W+Spaceも独立した3反復を行い、normal jumpとladder 0.2が同tickに二重適用されていないかを検査。埋まり開始の無効配置は再利用しない。今回はテスト/build禁止のため未実行。

---

## 2. Kelp PICK-0199：shape候補、fluid、interaction結果を分離

### 記録の矛盾（差の確度を下げる理由）

`records/PICK-0199.json` はsummaryと `expected_official.hit` で公式frontを主張するが、同じrawの `official_raw.reference_snapshot` は `hitType=MISS`、`hitBlock=BlockPos{x=49,y=64,z=50}` と記録する。試験対象は(50,64,50)。公式側の前景・背景は両方残り、Pomme側も両方残る。さらにPommeのsnapshotはnullで、`actual_pomme.hit_inferred_from_blocks=none` は「どちらも壊れなかった」ことからの推測で、raycast resultの採取値ではない。rawログの公式Posはrequested teleport位置からずれているため、撮影/クリック時の座標同期も確認対象。

したがってこのrecordの分類文字列 `observed_runtime_difference` は、front-vs-noneの差を証明しない。公式のraw hitはMISSであり、「公式front」という比較側の前提を満たしていないため、その比較は当該目的に対して無効扱いが妥当。Pomme側もsnapshotがなく、Pomme自身のhit/miss理由は未確定。現実に選択差がないと断定するのでなく、PICK-0199は無効比較から除外して再試験する。公式snapshotのz=45.4619はtp要求z=47より約1.54 block手前で、water中の位置ずれが候補。通常reach外になった可能性もあるため、次回は操作直前の位置を固定・記録する。

### 現コードでの経路分離

- `pomme-client/src/world/block/mod.rs::IMPLICIT_WATER` と `state_fluid` は`kelp`/`kelp_plant`をFULL_WATERとしている。これはfluid state/描画/水判定の情報。
- `pomme-client/src/physics/block_shape.rs::compute_outline` はwater/lava/bubble_columnだけに明示的なempty outlineを与える。kelpは列挙されない。
- `outline_shape` は `block_outline(state).unwrap_or(FULL_CUBE_SHAPE)`。`world/block/mod.rs::block_outline` は `outline.as_deref().or(shape.as_deref())`。現コードでkelpに明示shapeがない場合、fluids扱いだけを理由にrayを通さず、default outlineがfull-cubeになる。
- `pomme-client/src/player/interaction.rs::raycast` のblock predicateは `!is_air(state)`。fluid kindを条件にtargetable blockを除外するfilterはここにない。続いてそのblockのoutlineにclipし、hitを返す。
- `InteractionState::update_target` はblock raycast後に近いentityが勝つ場合のみentityへ置換し、それ以外は `block_hit.map(HitResult::Block)` を保持。攻撃側は`self.target`がBlockならそのhitを採用し、NoneならMISS分岐になる。つまり「fluid判定でtarget候補をskip」「empty outlineをclipで通過」「full-cube fallback」「targetが更新されずNone」は別の説明なので混ぜない。
- chunk描画の `classify_block` はblock id `kelp`をwater passへ分類しない。solid modelとしてmesh後、`fluid(state)`がwaterなら水を追加emitする。これは描画の二重構造で、interaction raycastはこの分類関数を呼んでいない。

### 原因確度と最小案

- **記録されたfront/noneの差：低〜未確定**（raw snapshotが矛盾しPomme hitが未採取）。現段階でshapeコードの修正対象と決めない。
- 現コード上「kelpをfluidだからraycastから除外する」誤りは見つからない。empty-outlineによってkelpを通過する経路も確認できない。shape担当が編集中なので、`block_shape.rs`・形状表・共有outlineの編集/変更はこの調査で行っていない。
- 最小次手はコード変更でなく計測修正：同じ状態を再配置し、公式/Pomme双方でteleport後の`Pos`・Rotation・target block position/face/hitTypeを攻撃直前に同時保存。Pommeは`InteractionState::target`直後の`BlockHitResult`を採る。無破壊時もhit snapshotで対象位置を確定し、前景/背景の最終状態だけからhitを逆算しない。

### 再試験判定表

1. 水なしkelp + 背面stone: kelp outline対照。
2. kelp周囲を水で満たし、同一座標に水source/kelp実状態を確認: implicit fluidを加えた条件。
3. その前にplain waterのみを狙う対照: vanilla及びPommeで`LiquidBlock.getShape` emptyによりrayが通過することを確認。
4. ray hit、interaction target、破壊結果を別々に保存。公式の実hitがMiss、または位置が狙いからずれたrunはfront比較に使わない。

---

## 3. Honey透過：partial-alpha textureがcutout passで不透明化

### 26.2 assetの確認

ローカルMinecraft 26.2 client jar `C:/Users/yuzum/AppData/Roaming/.minecraft/versions/26.2/26.2.jar`をread-onlyで調べた。

- `assets/minecraft/models/block/honey_block.json` は境界面を含むouter elementと1/16 insetのinner elementを持ち、top/side/bottom textureを使う。
- PNGはindexed color (`IHDR` color type 3)かつ`tRNS`透明度テーブルを持つ。alpha値の展開結果は各16×16 sprite全256 texelが同一partial alpha：`honey_block_side=189`、`honey_block_top=191`、`honey_block_bottom=129`。すなわちopaque texelは0、fully transparent texelも0。
- 公式がpartial alphaを透過表示する観測は `records/RENDER-honey.json`。比較画像ファイルはこの配布レポート内に見当たらなかったため、pixel差の再計測はしていない。通常起動のログと配置同期は記録あり。

### 現コードのatlas/render layer経路

1. `renderer/chunk/atlas.rs::sprite_transparency` はopaqueを「全alphaが255」、translucentを「partial-alphaが少なくとも1個」と分類する。honey texturesは`opaque=false, translucent=true`になるはず。
2. `renderer/chunk/mesher.rs::emit_face` はこのうち`region.opaque`だけを使い、falseならquadを`MeshSink.cutout`へ格納する。`region.translucent`をgeneric block quadのindex/pass選択には使わない。`SectionMesh`にあるindex群はsolid、cutout、waterの三系統。
3. `renderer/pipelines/chunk.rs::create_pipelines` はsolid/cutout双方 `blend_enable=FALSE`。cutoutだけ `chunk.frag` がalpha `<0.5`をdiscardし、通過したpixelは`out_color=vec4(shaded,color.a)`で出す。honeyのalphaは129/255≈0.506、189/255≈0.741、191/255≈0.749なのでdiscard thresholdを通るが、blend無効の色出力となり背景色と合成されず不透明に見える。
4. 同じ`chunk.rs::create_water_pipeline`はalpha blending on / depth write off、`water.frag`はsampled alphaを保持。ただしこれはwater geometry専用vertex/pipelineで、honey model quadをこのままroutingするものではない。
5. `world/block/model.rs::BakedModel`はgeometry・occlusion情報を保持するがBlock render layerを保持しない。BlockRegistryにも別のper-block translucent layer dispatchは見当たらない。block collision `compute_shape`/`compute_outline`のhoney特例は透過とは無関係。

### 原因確度と最小差分案

- **コード経路の原因確度：高い**。assetの全ピクセルpartial alphaと、honeyを含む一般quadのcutout/非blend経路が対応する。Reportの通常起動差とも一致。別の色/lighting原因が併存しないかは未確認。
- 最小の正しい設計差分候補は`MeshSink`/`SectionMesh`/chunk bufferにgeneric translucent quad index passを追加し、`region.translucent`のmodel quadsをそこへ振り分けること。chunk translucent pipelineでsource-alpha blend、depth test on、depth write off、適切な奥行き順sortを行う。existing water list/passとそのまま併合しない（water専用shader/vertex情報がある）。
- `chunk.frag`のthresholdだけ下げる、またはalphaを強制255にする対応は穴は減ってもpartial alpha合成をしないため誤修正。`cutout`へblendを入れるとleafなどbinary-alpha geometryまで順序/深度が変わるので避ける。

### 修正後の確認

26.2 jarの3 alpha分布を再確認し、普通のalpha cutout（葉等）とopaque stoneを対照に、honeyの前後に赤色blockを置く。Pommeの同じ通常起動条件/同じ視点で、透明度・depth occlusion・隣接opaque geometryへの影響を比較。実GPU/Macは元レポート未確認なので別の検証対象とする。今回は未実行。

---

## 参照した主なファイルとシンボル

- `pomme-client/src/physics/movement.rs`: `tick_land`, `apply_collision_with_context`, `is_on_climbable`
- `third_party/SteelMC/steel-core/src/entity/living_entity.rs`: `default_living_on_climbable`, `handle_on_climbable`, `handle_relative_friction_and_calculate_movement`
- `records/MOVE-ladder-walk_approach.json`, `records/MOVE-ladder-jump_approach.json`
- `records/PICK-0199.json`; `pomme-client/src/player/interaction.rs`: `InteractionState::update_target`, `raycast`; `pomme-client/src/world/block/mod.rs`: `IMPLICIT_WATER`, `state_fluid`; `pomme-client/src/physics/block_shape.rs`: `compute_outline`, `outline_shape`
- `records/RENDER-honey.json`; 26.2 client jar above; `pomme-client/src/renderer/chunk/atlas.rs`: `sprite_transparency`; `pomme-client/src/renderer/chunk/mesher.rs`: `MeshSink`, `classify_block`, `emit_face`; `pomme-client/src/renderer/pipelines/chunk.rs`: `create_pipelines`, `create_water_pipeline`; `pomme-client/src/renderer/shaders/chunk.frag`, `water.frag`

## 実施した確認・制約

- 読み取り: `AGENTS.md`、ponytail skill、現行Rustコード、SteelMC reference、batch-02 record JSON、Minecraft 26.2 client jar内のhoney block model/texture。
- read-only asset scriptでPNG indexed palette + tRNSを展開。実行結果: 16×16各256 texel、alpha counts side `{189:256}` / top `{191:256}` / bottom `{129:256}`。
- build/test/実機再試験/コード変更/commitは実施していない。共有形状担当の編集中ファイルは閲覧のみ。
- 許可されたメモ `docs/report-audit/ladder-kelp-honey-plan.md` のみを変更した。
