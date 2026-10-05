# Pomme × Minecraft Java Edition — 現行互換性一覧

- 比較基準：**公式 Java Edition 26.2**（ローカルの逆コンパイルソース）。
- Rustスナップショット：`e3d7b9abe47c966051633e63d28bd38abe17f22f`。調査開始時のワーキングツリーはclean。
- 集約日時：2026-10-05T19:38:48+09:00。Luna 19分野の並列静的調査＋3分野群の再確認。
- 対象：主要機能 **194項目 / 19分野**。全機能・全ブロック・全entity・全接続版の網羅表ではない。分野ごとの調査範囲を下に記載。
- 今回はビルド、テスト実行、実サーバー接続、GPU/音声/操作の比較を行っていない。**公式との一致確認済みは0件**。既存テストへの言及はコードの存在を示すだけ。
- 接続対応バージョンの範囲と、公式26.2の挙動比較は別。内蔵シングルプレイはSteelMCを含む別環境として表示。
- 正本データ：[data.json](data.json)。公式ソースへの根拠リンクはこの作業ディレクトリ内のローカル参照（Client単体の配布には含まれない）。

## 判定の読み方

| 軸 | 状態 | 意味 |
|---|---|---|
| 実装 | 実装あり | 行に書いた限定的な処理が本番経路にある。完全互換の保証ではない。 |
| 実装 | 一部実装 | 経路はあるが、行の対象範囲に具体的な欠落・未接続がある。 |
| 実装 | 未実装 | 対象経路の欠落を確認した。単なる未調査とは区別する。 |
| 実装 | 未調査 | 実装状態を判断する根拠が不足している。 |
| 比較 | 差分あり | 公式とRustの現在ソースから具体的な意味的差分を確認した。実機での再現確認ではない。 |
| 比較 | 未検証 | 一致/差分を確定していない。実装ありでもこの状態になり得る。 |
| 検証 | 静的照合 | 公式とRust双方のソースを照合した。 |
| 検証 | Rust静的確認のみ / 未調査 | 双方のソースを揃えた判定には至っていない。 |

## 件数（互換率ではない）

| 実装あり | 一部実装 | 未実装 | 未調査 | 静的な差分あり | 比較未検証 |
|---:|---:|---:|---:|---:|---:|
| 114 | 75 | 4 | 1 | 41 | 153 |

## 分野別索引

| 分野 | 項目数 | 差分あり |
|---|---:|---:|
| [地上移動・衝突](#domain-movement) | 10 | 0 |
| [流体・特殊環境の物理](#domain-fluids) | 10 | 1 |
| [攻撃・破壊・設置・使用](#domain-interaction) | 10 | 2 |
| [在庫・コンテナ操作](#domain-inventory) | 10 | 2 |
| [HUD・情報表示](#domain-hud) | 10 | 0 |
| [チャット・コマンド](#domain-chat) | 11 | 3 |
| [メニュー・設定](#domain-menus) | 11 | 3 |
| [ブロック・照明描画](#domain-blocks) | 10 | 0 |
| [空・天候・ワールド境界](#domain-atmosphere) | 10 | 4 |
| [生物の描画](#domain-mobs) | 9 | 4 |
| [乗り物・非生物エンティティ](#domain-nonliving) | 10 | 1 |
| [ブロックエンティティ](#domain-blockentities) | 9 | 4 |
| [アイテム・装備・プレイヤー描画](#domain-items) | 10 | 1 |
| [パーティクル](#domain-particles) | 10 | 0 |
| [音声・音楽・字幕](#domain-audio) | 10 | 4 |
| [接続・プロトコル基盤](#domain-connection) | 11 | 0 |
| [ワールド・通信同期](#domain-sync) | 12 | 1 |
| [リソースパック](#domain-packs) | 10 | 4 |
| [内蔵シングルプレイ](#domain-singleplayer) | 11 | 7 |

<a id="domain-movement"></a>

## 地上移動・衝突

調査範囲：Pommeのphysics/movement.rs、collision.rs、aabb.rs、block_shape.rsとplayer/mod.rsのプレイヤー移動・姿勢処理を読み、app/core.rsの通常tick呼出しまで確認。公式LivingEntity.java、Player.java、LocalPlayer.javaおよびEntity.javaの対応処理と照合した。流体中の移動物理と乗り物移動の互換性は対象外で、実行比較はしていない。

- 制約：本調査はソース静的照合のみで、Rustビルド・テスト・クライアント起動・公式環境との実測比較は行っていない。全行のcomparisonは未検証で、一致を確認したものではない。
- 制約：流体移動は担当範囲外として行を設けていない。公式サーバーによる移動権威・補正、SteelMC内蔵シングルプレイの移動経路、全block shape・全entityの網羅性もこの調査では判定していない。
- 制約：通常プレイヤーの主要な移動呼出しはapp/core.rs内で確認した。app/phases/in_game.rsはtick駆動等の周辺実装を確認したが、移動関数の直接呼出し箇所は見つけていない。

| ID / 機能 | 公式の挙動 | Rustの現状・差分 | 実装 | 比較 / 検証 |
|---|---|---|---|---|
| [movement-01](#movement-01)<br>地上移動 › 入力 › 斜め移動と入力減衰 | LocalPlayerは入力を0.98倍し、使用中アイテムの速度係数としゃがみ係数を適用してから、単位正方形の範囲に方向・大きさを調整する。 | Rustはキー入力を正規化し、0.98、アイテム係数、しゃがみ時のsneaking_speedを順に掛けてsquare_movementし、移動tickに渡す。 | 実装あり | 未検証<br>静的照合 |
| [movement-02](#movement-02)<br>地上移動 › 加速・減速 › 接地と空中の移動 | LivingEntity.travelInAirは接地時に足元ブロック摩擦を使って相対加速し、移動後にブロック摩擦と空気抵抗を適用する。 | Rustは接地時にブロック摩擦の三乗に応じた加速を計算し、空中では通常0.02・走行時0.026の加速を用いて、衝突後に垂直・水平抗力と重力を適用する。 | 実装あり | 未検証<br>静的照合 |
| [movement-03](#movement-03)<br>ジャンプ › 通常・走行ジャンプ › 高さと前方加速 | LivingEntity.jumpFromGroundはジャンプ力属性、ブロックのjump factor、Jump Boostを合成し、接地走行ジャンプでは視線yaw方向へ水平速度0.2を加える。 | Rustはjump_strength属性、足元ブロック係数、Jump Boostを合成し、走行中にyaw方向へ0.2を加算する。grounded状態か水面条件でjump入力を受け、10 tickの再ジャンプdelayを管理する。 | 実装あり | 未検証<br>静的照合 |
| [movement-04](#movement-04)<br>走行 › 開始・停止 › 入力・空腹・衝突条件 | LocalPlayerは前進impulse、十分な食料、アイテム使用・低速移動状態、水平衝突などを考慮して走行開始・停止を判定し、走行切替の入力時間窓を管理する。 | Rustは前進入力、food &gt; 6、しゃがみ・アイテム使用を開始条件に含め、後退方向・空腹・使用中アイテム・重大な水平衝突で停止する。double-tap窓は7 tick。 | 実装あり | 未検証<br>静的照合 |
| [movement-05](#movement-05)<br>しゃがみ › 姿勢・当たり判定 › 天井下の縮小 | Player.updatePlayerPoseは立位のfitを確認し、desired poseが塞がれているときはしゃがみ、それも塞がれていればswimming poseへ退避する。しゃがみ姿勢の当たり判定は高さが低い。 | Rustはしゃがみ・立位姿勢の衝突可否を確認してcrouching状態を決め、tick後にdesired poseを設定。該当姿勢が入らない場合、しゃがみ、最後にswimming姿勢へ退避し、姿勢に応じたbounding boxを使う。 | 実装あり | 未検証<br>静的照合 |
| [movement-06](#movement-06)<br>衝突 › ブロック・entity › 軸別解決と段差越え | Entity.moveは移動先のblock/entity colliderを集めて軸ごとに衝突を解き、水平移動が塞がれた接地条件では候補高を試し、水平距離が改善したstep結果を選ぶ。 | Rustは部分block shape、entity AABB、world borderを集め、Y・水平二軸の順に移動量をclipする。接地中または前tick接地で水平方向が塞がれた場合、max step height以下の候補を試して水平距離の改善を採用する。 | 実装あり | 未検証<br>静的照合 |
| [movement-07](#movement-07)<br>しゃがみ移動 › 足場端 › 落下を抑制 | Entity.moveはしゃがみ入力中、接地またはstep height相当以内に足場がある状況で、支持を失う水平移動を小刻みに縮めて端からの落下を避ける。 | RustはSneak入力があり飛行中でなく、上向き移動でないとき、足場判定をx方向・z方向・斜め方向に反復して水平移動を最大0.05ずつ縮める。 | 実装あり | 未検証<br>静的照合 |
| [movement-08](#movement-08)<br>地上移動 › はしご等 › 上昇・落下制限 | LivingEntityのclimbable移動は水平速度を±0.15に制限し、下降速度を最低-0.15に抑える。プレイヤーは条件によりはしご下降を抑止し、接触衝突またはジャンプ時に上向き速度0.2を得る。 | Rustはladder・vine・scaffolding等をclimbableとして扱い、水平・下降速度制限を適用するほか、ジャンプ時または水平衝突時にy速度を0.2にする。 | 実装あり | 未検証<br>静的照合 |
| [movement-09](#movement-09)<br>飛行 › Elytra滑空 › 揚力・姿勢制御 | LivingEntity.updateFallFlyingMovementは有効重力、pitchとlook方向、水平速度から降下・揚力と方向追従を計算し、速度に(0.99, 0.98, 0.99)の係数を掛けてから移動する。 | Rustはfall_flying状態のtravel分岐で同じ揚力・速度方向補正の式と軸別dragを計算し、その速度で通常衝突解決を行う。 | 実装あり | 未検証<br>静的照合 |
| [movement-10](#movement-10)<br>飛行 › クリエイティブ飛行 › jump二度押しと鉛直入力 | LocalPlayerはmayfly時のjump新規押下を7 tickのwindowで二度検出して飛行状態を切り替え、飛行中はjump/sneak入力とfly speedから鉛直移動を加える。通常tickの接地でcreative以外の飛行を停止する。 | Rustはmay_fly時のjump押下遷移を7 tickで処理し、飛行中のjump/sneakで鉛直速度をfly_speedの3倍加算する。travel後に接地した非creativeの飛行を解除する。 | 実装あり | 未検証<br>静的照合 |

### 条件・残件・根拠

<a id="movement-01"></a>

<details>
<summary>movement-01 — 地上移動 › 入力 › 斜め移動と入力減衰</summary>

- 比較環境：クライアント共通
- 対象条件：地上で前進と横移動を同時入力し、アイテム使用・しゃがみ係数を変える
- 残件 / 比較すべき点：キーボードおよびアナログ入力の正規化、斜め移動量、丸め順を同じ状態で比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/player/LocalPlayer.java:669](../../../minecraft-26.2-decompiled/src/net/minecraft/client/player/LocalPlayer.java#L669) — modifyInput：入力の0.98倍、アイテム使用・低速移動係数、square調整。
  - Rust：[Client/pomme-client/src/physics/movement.rs:1616](../../pomme-client/src/physics/movement.rs#L1616) — movement_input_with_sneaking_speed：同様の入力係数とsquare_movementを計算。
  - Rust：[Client/pomme-client/src/physics/movement.rs:107](../../pomme-client/src/physics/movement.rs#L107) — tick_with_context：入力値の計算から移動状態更新を実行。
  - Rust：[Client/pomme-client/src/app/core.rs:7811](../../pomme-client/src/app/core.rs#L7811) — movement::tick_with_context：通常の非乗馬tickから本番移動処理を呼ぶ。

</details>

<a id="movement-02"></a>

<details>
<summary>movement-02 — 地上移動 › 加速・減速 › 接地と空中の移動</summary>

- 比較環境：クライアント共通
- 対象条件：非流体・非滑空状態で、平坦地上と空中の水平速度を比較する
- 残件 / 比較すべき点：各摩擦ブロック、属性modifier、接地開始tickの加速と速度減衰を実測比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/entity/LivingEntity.java:2347](../../../minecraft-26.2-decompiled/src/net/minecraft/world/entity/LivingEntity.java#L2347) — travelInAir：摩擦位置取得と接地/空中のtravel処理。
  - Rust：[Client/pomme-client/src/physics/movement.rs:413](../../pomme-client/src/physics/movement.rs#L413) — tick_land：加速、衝突、重力および水平・垂直dragの適用。
  - Rust：[Client/pomme-client/src/physics/movement.rs:1469](../../pomme-client/src/physics/movement.rs#L1469) — block_movement_factor：ブロック移動係数の計算。
  - Rust：[Client/pomme-client/src/app/core.rs:7811](../../pomme-client/src/app/core.rs#L7811) — movement::tick_with_context：通常プレイヤー移動の本番呼出し。

</details>

<a id="movement-03"></a>

<details>
<summary>movement-03 — ジャンプ › 通常・走行ジャンプ › 高さと前方加速</summary>

- 比較環境：クライアント共通
- 対象条件：平地で通常ジャンプと走行ジャンプをし、効果・jump_strength属性を変える
- 残件 / 比較すべき点：ジャンプ初速、上昇高さ、再入力可能tick、ブロック係数を公式と比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/entity/LivingEntity.java:2255](../../../minecraft-26.2-decompiled/src/net/minecraft/world/entity/LivingEntity.java#L2255) — getJumpPower：属性・ブロック係数・Jump Boostの合算。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/entity/LivingEntity.java:2264](../../../minecraft-26.2-decompiled/src/net/minecraft/world/entity/LivingEntity.java#L2264) — jumpFromGround：鉛直速度の設定と走行ジャンプ水平boost。
  - Rust：[Client/pomme-client/src/physics/movement.rs:387](../../pomme-client/src/physics/movement.rs#L387) — jump_from_ground：ジャンプ力合成および走行boost。
  - Rust：[Client/pomme-client/src/physics/movement.rs:1469](../../pomme-client/src/physics/movement.rs#L1469) — block_movement_factor：ジャンプ時のブロック係数選択。

</details>

<a id="movement-04"></a>

<details>
<summary>movement-04 — 走行 › 開始・停止 › 入力・空腹・衝突条件</summary>

- 比較環境：クライアント共通
- 対象条件：前進のdouble-tap、走行キー、後退、空腹値6以下、水平衝突を個別に試す
- 残件 / 比較すべき点：LocalPlayerの前進impulse・sprintingPossible条件および走行窓の設定値を同条件で比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/player/LocalPlayer.java:768](../../../minecraft-26.2-decompiled/src/net/minecraft/client/player/LocalPlayer.java#L768) — aiStep：走行窓を使った開始と走行停止判定。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/player/LocalPlayer.java:1095](../../../minecraft-26.2-decompiled/src/net/minecraft/client/player/LocalPlayer.java#L1095) — canStartSprinting：走行開始に必要な状態。
  - Rust：[Client/pomme-client/src/physics/movement.rs:1026](../../pomme-client/src/physics/movement.rs#L1026) — update_sprint_state：開始窓、開始条件、停止条件を更新。
  - Rust：[Client/pomme-client/src/app/core.rs:7811](../../pomme-client/src/app/core.rs#L7811) — movement::tick_with_context：入力状態から走行状態を更新する呼出し経路。

</details>

<a id="movement-05"></a>

<details>
<summary>movement-05 — しゃがみ › 姿勢・当たり判定 › 天井下の縮小</summary>

- 比較環境：クライアント共通
- 対象条件：立位では低い天井に当たるが、しゃがみ姿勢なら収まる場所で移動する
- 残件 / 比較すべき点：姿勢更新のtick順、隙間のepsilon、entity・world borderを含むfit判定を比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/entity/player/Player.java:335](../../../minecraft-26.2-decompiled/src/net/minecraft/world/entity/player/Player.java#L335) — updatePlayerPose：姿勢に応じて衝突可否を確認し、poseを選択。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/entity/player/Player.java:363](../../../minecraft-26.2-decompiled/src/net/minecraft/world/entity/player/Player.java#L363) — canPlayerFitWithinBlocksAndEntitiesWhen：姿勢別boxをepsilon分deflateして衝突判定。
  - Rust：[Client/pomme-client/src/physics/movement.rs:1090](../../pomme-client/src/physics/movement.rs#L1090) — update_crouch_state：しゃがみ状態の判定。
  - Rust：[Client/pomme-client/src/physics/movement.rs:1120](../../pomme-client/src/physics/movement.rs#L1120) — update_player_pose：pose候補と収容不能時の退避先を設定。
  - Rust：[Client/pomme-client/src/player/mod.rs:609](../../pomme-client/src/player/mod.rs#L609) — bounding_box_for_pose：poseごとの衝突boxを取得。

</details>

<a id="movement-06"></a>

<details>
<summary>movement-06 — 衝突 › ブロック・entity › 軸別解決と段差越え</summary>

- 比較環境：クライアント共通
- 対象条件：段差・壁・部分形状ブロックに向かって接地移動し、entity colliderも置く
- 残件 / 比較すべき点：複数形状の重なり、衝突軸順序、段差候補選択、境界条件を公式と比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/entity/Entity.java:1130](../../../minecraft-26.2-decompiled/src/net/minecraft/world/entity/Entity.java#L1130) — collide：entityとblock形状を含む衝突解決およびstep候補を扱う。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/entity/Entity.java:1158](../../../minecraft-26.2-decompiled/src/net/minecraft/world/entity/Entity.java#L1158) — collectCandidateStepUpHeights：上り候補高さを収集。
  - Rust：[Client/pomme-client/src/physics/collision.rs:462](../../pomme-client/src/physics/collision.rs#L462) — resolve_collision_for_player：block/entity/border colliderを軸別に解決し、step候補を選ぶ。
  - Rust：[Client/pomme-client/src/physics/movement.rs:703](../../pomme-client/src/physics/movement.rs#L703) — apply_collision_with_context：travel中の移動をcollision resolverへ接続。

</details>

<a id="movement-07"></a>

<details>
<summary>movement-07 — しゃがみ移動 › 足場端 › 落下を抑制</summary>

- 比較環境：クライアント共通
- 対象条件：地上足場の端でしゃがみながら水平・斜め移動する
- 残件 / 比較すべき点：足場の途切れ、entity支持物、world border、落下距離による適用条件を比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/entity/Entity.java:783](../../../minecraft-26.2-decompiled/src/net/minecraft/world/entity/Entity.java#L783) — move：collide前にmaybeBackOffFromEdgeを適用。
  - Rust：[Client/pomme-client/src/physics/movement.rs:1200](../../pomme-client/src/physics/movement.rs#L1200) — back_off_from_edge：足場のない水平移動を刻み幅で縮める。
  - Rust：[Client/pomme-client/src/physics/movement.rs:703](../../pomme-client/src/physics/movement.rs#L703) — apply_collision_with_context：collision解決前にしゃがみ足場補正を適用。
  - Rust：[Client/pomme-client/src/physics/collision.rs:218](../../pomme-client/src/physics/collision.rs#L218) — no_collision_for_player：足場候補のblock/entity/border衝突を判定。

</details>

<a id="movement-08"></a>

<details>
<summary>movement-08 — 地上移動 › はしご等 › 上昇・落下制限</summary>

- 比較環境：クライアント共通
- 対象条件：はしご・ツタ・足場に接触して上昇、下降、横衝突、ジャンプする
- 残件 / 比較すべき点：全climbable blockの認識、flying・scaffolding時の下降抑止差、およびpowder snowとの条件を比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/entity/LivingEntity.java:2530](../../../minecraft-26.2-decompiled/src/net/minecraft/world/entity/LivingEntity.java#L2530) — handleRelativeFrictionAndCalculateMovement：climbable状態の速度補正後、衝突・jump条件で上向き速度を設定。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/entity/LivingEntity.java:2556](../../../minecraft-26.2-decompiled/src/net/minecraft/world/entity/LivingEntity.java#L2556) — handleOnClimbable：水平・落下速度制限およびプレイヤーの下降抑止。
  - Rust：[Client/pomme-client/src/physics/movement.rs:1254](../../pomme-client/src/physics/movement.rs#L1254) — is_on_climbable：対象block識別。
  - Rust：[Client/pomme-client/src/physics/movement.rs:413](../../pomme-client/src/physics/movement.rs#L413) — tick_land：climbable上の移動速度補正。
  - Rust：[Client/pomme-client/src/physics/movement.rs:703](../../pomme-client/src/physics/movement.rs#L703) — apply_collision_with_context：衝突後の登攀速度補正。

</details>

<a id="movement-09"></a>

<details>
<summary>movement-09 — 飛行 › Elytra滑空 › 揚力・姿勢制御</summary>

- 比較環境：クライアント共通
- 対象条件：Elytra滑空中に水平・下向き・上向き視線で速度変化を観察する
- 残件 / 比較すべき点：入力角度、属性gravity、速度ベクトルおよび衝突時の着地条件を同じ初期値で数値比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/entity/LivingEntity.java:2462](../../../minecraft-26.2-decompiled/src/net/minecraft/world/entity/LivingEntity.java#L2462) — updateFallFlyingMovement：滑空の重力・揚力・方向追従と軸別drag。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/entity/LivingEntity.java:2441](../../../minecraft-26.2-decompiled/src/net/minecraft/world/entity/LivingEntity.java#L2441) — travelFallFlying：計算した速度をmoveへ渡す。
  - Rust：[Client/pomme-client/src/physics/movement.rs:549](../../pomme-client/src/physics/movement.rs#L549) — tick_fall_flying：滑空速度の更新式と衝突処理。
  - Rust：[Client/pomme-client/src/physics/movement.rs:218](../../pomme-client/src/physics/movement.rs#L218) — travel：fall_flying状態を滑空travelへ振り分ける。

</details>

<a id="movement-10"></a>

<details>
<summary>movement-10 — 飛行 › クリエイティブ飛行 › jump二度押しと鉛直入力</summary>

- 比較環境：クライアント共通
- 対象条件：飛行可能状態でjumpを二度押し、上昇・下降入力と接地停止を確認する
- 残件 / 比較すべき点：能力packet更新、飛行中のtravel水平制御と鉛直drag、spectator noclipの実装範囲を比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/player/LocalPlayer.java:800](../../../minecraft-26.2-decompiled/src/net/minecraft/client/player/LocalPlayer.java#L800) — aiStep：jump edge検出と7 tick windowによるflying toggle。
  - Rust：[Client/pomme-client/src/physics/movement.rs:341](../../pomme-client/src/physics/movement.rs#L341) — update_fly_state：may_fly・jump押下windowによるflight toggle。
  - Rust：[Client/pomme-client/src/physics/movement.rs:107](../../pomme-client/src/physics/movement.rs#L107) — tick_with_context：jump/sneakの鉛直入力と通常travel経路。
  - Rust：[Client/pomme-client/src/physics/movement.rs:281](../../pomme-client/src/physics/movement.rs#L281) — stop_flying_on_ground：creative以外の接地飛行を停止。
  - Rust：[Client/pomme-client/src/app/core.rs:7811](../../pomme-client/src/app/core.rs#L7811) — movement::tick_with_context：通常プレイヤーtickからflight処理に接続。

</details>


<a id="domain-fluids"></a>

## 流体・特殊環境の物理

調査範囲：RustのLocalPlayer状態更新、physics::movementの水/溶岩移動・衝突時の特殊ブロック処理と、app/core.rsからの通常ティック接続を確認し、公式LivingEntity/LocalPlayerと指定ブロックを照合。流体流速計算の全分岐、サーバー上の実ダメージ・効果の最終権威、全登攀可能ブロックは網羅していません。

- 制約：水移動で公式のDolphin's Graceによる0.96減衰が確認したRust経路にないため、差分ありと判定。
- 制約：公式の溺れダメージはサーバー権威だがRust tick_air_supplyはローカルhealthを予測更新し、水中呼吸/無敵免除を適用していない。実際のhealth同期との関係は未確認。
- 制約：build/test/アプリ起動・実機比較は指示により未実施。全てソース静的照合。
- 制約：公式のCLIMBABLEタグ集合とRustの列挙ブロック、粉雪の動的衝突形状、流体流速の全パターンは網羅していない。

| ID / 機能 | 公式の挙動 | Rustの現状・差分 | 実装 | 比較 / 検証 |
|---|---|---|---|---|
| [fluids-01](#fluids-01)<br>水泳・溺れ › 水中判定と水中移動 | 水中では水平入力速度0.02を基礎に移動し、通常の水平減衰は0.8（スプリント時0.9）。Water Movement Efficiencyで速度と減衰を補正し、イルカの grace 効果があれば減衰は0.96になります。 | LocalPlayer状態からin_waterを更新し、通常ティックはtick_waterへ接続しています。速度・水効率補正はある一方、確認した水移動にはイルカのgrace補正がありません。 | 一部実装 | 差分あり<br>静的照合 |
| [fluids-02](#fluids-02)<br>水泳・溺れ › 水面下での上昇/下降入力 | 水中ジャンプで垂直速度に0.04を加算し、下入力でも0.04を減算します。水深がジャンプ閾値を超える場合は地上ジャンプではなく液体ジャンプを行います。 | 水中判定とfluid_heightに基づきジャンプ閾値を選び、ジャンプ/スニークで±0.04を反映してから水中移動を行います。 | 実装あり | 未検証<br>静的照合 |
| [fluids-03](#fluids-03)<br>水泳・溺れ › 水中呼吸ゲージと溺れダメージ | 水中で目が水に入り、水中呼吸不可なら空気を減らし、溺れダメージ条件成立時にサーバー側で2ダメージを与えます。泡柱内の目は空気減少を免除し、水外では空気を回復します。 | Rustのローカルtickは目が水中かつ泡柱外なら空気を減らし、閾値以下でローカルhealthも2減らします。公式の溺れダメージはサーバー権威で、水中呼吸等の免除もサーバー側にあります。Rustのhealth予測がサーバー同期でどう補正されるか、免除状態での表示差が生じるかは未確認です。 | 一部実装 | 未検証<br>静的照合 |
| [fluids-04](#fluids-04)<br>流体物理 › 部分水位・溶岩接触判定 | エンティティ境界箱の流体接触から水/溶岩の流体高さを求め、接触状態と目の水中状態を使い移動・呼吸を判断します。 | LocalPlayerの幅/高さの境界箱を走査し、水位と溶岩位を別々に保持、目と足の水判定からin_water/in_lava/swimmingを更新します。 | 実装あり | 未検証<br>静的照合 |
| [fluids-05](#fluids-05)<br>溶岩 › 浅い/深い溶岩の移動 | 溶岩中は水平入力0.02で移動し、浅い場合は速度を(0.5,0.8,0.5)で減衰後に流体落下補正、深い場合は全速度を0.5倍し、重力の1/4を適用します。 | 溶岩高さと0.4閾値で浅/深を分け、同じ水平基礎入力・減衰・落下補正・重力1/4を適用します。 | 実装あり | 未検証<br>静的照合 |
| [fluids-06](#fluids-06)<br>泡柱 › 上昇/下降と天井側の速度上限 | 泡柱内部では上昇(+0.06、上限0.7)または下降(-0.03、下限-0.3)し、上が空いている柱上端では+0.1/上限1.8または-0.03/下限-0.9を適用します。 | 移動経路が通過した泡柱を走査し、drag状態と上部の空き状態に応じて上記速度更新を行います。creative flight中は無効化します。 | 実装あり | 未検証<br>静的照合 |
| [fluids-07](#fluids-07)<br>はしご/ツタ › 登攀中の落下抑制とジャンプ | 登攀可能ブロック上では水平速度を±0.15、落下速度を最低-0.15に制限し、落下距離をリセットします。ジャンプまたは水平衝突時に垂直速度0.2を与えます。 | 一部の登攀ブロックIDと開いた梯子隣接トラップドアを認識し、移動速度制限・落下距離リセット・ジャンプ/衝突時の0.2上昇を適用します。公式のCLIMBABLEタグ全体との網羅性は未確認です。 | 一部実装 | 未検証<br>静的照合 |
| [fluids-08](#fluids-08)<br>特殊環境 › クモの巣による移動阻害 | クモの巣内では通常速度成分に(0.25,0.05,0.25)を乗算し、Weaving効果中のLivingEntityは(0.5,0.25,0.5)を使います。 | 衝突移動の直前にクモの巣との交差を調べ、標準/Weaving倍率を移動量に適用し、速度と落下距離をリセットします。 | 実装あり | 未検証<br>静的照合 |
| [fluids-09](#fluids-09)<br>特殊環境 › 粉雪の沈み込みと革ブーツ登攀 | 粉雪内では移動量に(0.9,1.5,0.9)を適用します。革ブーツ装備者は歩行可能で、ジャンプまたは水平衝突条件で垂直速度0.2を得ます。 | 粉雪との交差に同倍率を適用し、速度/落下距離をリセットします。革ブーツ装備時はジャンプまたは水平衝突で上昇を設定します。歩行可能形状は別途衝突判定側に依存します。 | 一部実装 | 未検証<br>静的照合 |
| [fluids-10](#fluids-10)<br>特殊環境 › 蜂蜜ブロック側面の滑り | 落下中に蜂蜜ブロック側面を滑ると垂直速度を-0.1274に制限し、旧落下速度が-0.13未満の場合は水平速度も比例して減衰し、落下距離をリセットします。 | 水平衝突かつ下降移動時にプレイヤー境界箱が蜂蜜ブロックに触れていれば、同じ旧速度閾値で水平倍率と垂直速度を更新し、落下距離をリセットします。 | 実装あり | 未検証<br>静的照合 |

### 条件・残件・根拠

<a id="fluids-01"></a>

<details>
<summary>fluids-01 — 水泳・溺れ › 水中判定と水中移動</summary>

- 比較環境：クライアント共通
- 対象条件：水中で前進し、通常/スプリント/水中移動効率/イルカのgraceを個別に適用する
- 残件 / 比較すべき点：イルカのgrace有無それぞれで水平減衰と移動距離を比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/entity/LivingEntity.java:2382](../../../minecraft-26.2-decompiled/src/net/minecraft/world/entity/LivingEntity.java#L2382) — travelInWater：速度効率、sprint減衰、Dolphin's Grace=0.96および水中移動を定義。
  - Rust：[Client/pomme-client/src/physics/movement.rs:483](../../pomme-client/src/physics/movement.rs#L483) — tick_water：0.02基礎速度、水効率、0.8/0.9基礎減衰はあるがgrace分岐なし。
  - Rust：[Client/pomme-client/src/app/core.rs:7811](../../pomme-client/src/app/core.rs#L7811) — movement::tick_with_context 呼出：非搭乗時の本番プレイヤーティックから移動処理に接続。

</details>

<a id="fluids-02"></a>

<details>
<summary>fluids-02 — 水泳・溺れ › 水面下での上昇/下降入力</summary>

- 比較環境：クライアント共通
- 対象条件：水に接触中、深さ0.4以下/超過、地上/空中でジャンプまたはスニークする
- 残件 / 比較すべき点：閾値境界と入力保持時の速度を同条件で比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/entity/LivingEntity.java:2278](../../../minecraft-26.2-decompiled/src/net/minecraft/world/entity/LivingEntity.java#L2278) — goDownInWater / jumpInLiquid：水中上下入力の±0.04。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/entity/LivingEntity.java:2891](../../../minecraft-26.2-decompiled/src/net/minecraft/world/entity/LivingEntity.java#L2891) — aiStep の jump 分岐：流体深さと閾値に基づき水中ジャンプ/地上ジャンプを選択。
  - Rust：[Client/pomme-client/src/physics/movement.rs:48](../../pomme-client/src/physics/movement.rs#L48) — LIQUID_JUMP_ACCELERATION：水中上下入力値0.04。
  - Rust：[Client/pomme-client/src/physics/movement.rs:165](../../pomme-client/src/physics/movement.rs#L165) — tick_with_context：入力、in_water、fluid_heightで液体/地上ジャンプを選び通常移動へ接続。

</details>

<a id="fluids-03"></a>

<details>
<summary>fluids-03 — 水泳・溺れ › 水中呼吸ゲージと溺れダメージ</summary>

- 比較環境：クライアント共通
- 対象条件：目が水中/泡柱内、水中呼吸効果あり/なし、空気が-20以下になる条件
- 残件 / 比較すべき点：水中呼吸・無敵条件のローカル抑止と、サーバーからのhealth更新に対する予測の扱いを確認する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/entity/LivingEntity.java:440](../../../minecraft-26.2-decompiled/src/net/minecraft/world/entity/LivingEntity.java#L440) — baseTick：水中呼吸等の免除判定、空気低下、サーバーでの2ダメージ、泡柱除外を定義。
  - Rust：[Client/pomme-client/src/player/mod.rs:705](../../pomme-client/src/player/mod.rs#L705) — tick_air_supply：泡柱除外と空気回復は実装されるが、目が水中の場合の空気/health更新に効果免除判定がない。
  - Rust：[Client/pomme-client/src/physics/movement.rs:210](../../pomme-client/src/physics/movement.rs#L210) — tick_with_context：移動ティック終端で空気状態更新を呼び出す。

</details>

<a id="fluids-04"></a>

<details>
<summary>fluids-04 — 流体物理 › 部分水位・溶岩接触判定</summary>

- 比較環境：クライアント共通
- 対象条件：源流・段階水位・水面直上の流体・水と溶岩の境界にプレイヤーを置く
- 残件 / 比較すべき点：境界箱の端、視点水位、流体量の丸め境界を公式と比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/entity/LivingEntity.java:2323](../../../minecraft-26.2-decompiled/src/net/minecraft/world/entity/LivingEntity.java#L2323) — shouldTravelInFluid：水/溶岩接触状態を流体移動へ振り分け。
  - Rust：[Client/pomme-client/src/player/mod.rs:723](../../pomme-client/src/player/mod.rs#L723) — update_water_state：AABB走査と水/溶岩高さ、目・足状態、泳ぎ状態を更新。
  - Rust：[Client/pomme-client/src/physics/movement.rs:119](../../pomme-client/src/physics/movement.rs#L119) — tick_with_context：毎移動ティックで水状態を更新してから移動分岐に使用。

</details>

<a id="fluids-05"></a>

<details>
<summary>fluids-05 — 溶岩 › 浅い/深い溶岩の移動</summary>

- 比較環境：クライアント共通
- 対象条件：浅い溶岩/深い溶岩で水平移動および上昇/下降する
- 残件 / 比較すべき点：溶岩高さの閾値と重力適用順序を同条件で比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/entity/LivingEntity.java:2411](../../../minecraft-26.2-decompiled/src/net/minecraft/world/entity/LivingEntity.java#L2411) — travelInLava：浅深判定、移動減衰、落下補正、重力1/4。
  - Rust：[Client/pomme-client/src/physics/movement.rs:610](../../pomme-client/src/physics/movement.rs#L610) — tick_lava：lava_height閾値別の減衰と重力処理。
  - Rust：[Client/pomme-client/src/app/core.rs:7811](../../pomme-client/src/app/core.rs#L7811) — movement::tick_with_context 呼出：溶岩移動を含む通常移動ティックの本番入口。

</details>

<a id="fluids-06"></a>

<details>
<summary>fluids-06 — 泡柱 › 上昇/下降と天井側の速度上限</summary>

- 比較環境：クライアント共通
- 対象条件：上昇/下降泡柱の内部と上端を通過し、通常/飛行状態で確認する
- 残件 / 比較すべき点：移動経路で複数泡柱を横切るケース、上部の衝突形状・流体の組合せを比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/level/block/BubbleColumnBlock.java:57](../../../minecraft-26.2-decompiled/src/net/minecraft/world/level/block/BubbleColumnBlock.java#L57) — entityInside：上部に空気があるかで柱上端/内部効果を選ぶ。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/entity/Entity.java:2738](../../../minecraft-26.2-decompiled/src/net/minecraft/world/entity/Entity.java#L2738) — handleOnAboveBubbleColumn / handleOnInsideBubbleColumn：上端と内部それぞれの速度増減・上限下限。
  - Rust：[Client/pomme-client/src/physics/movement.rs:890](../../pomme-client/src/physics/movement.rs#L890) — apply_bubble_column_effect：通過セルとdrag/上部空き判定を走査し速度に反映。
  - Rust：[Client/pomme-client/src/physics/movement.rs:968](../../pomme-client/src/physics/movement.rs#L968) — bubble_column_velocity：速度の4条件別増減とclamp。

</details>

<a id="fluids-07"></a>

<details>
<summary>fluids-07 — はしご/ツタ › 登攀中の落下抑制とジャンプ</summary>

- 比較環境：クライアント共通
- 対象条件：梯子/ツタ等で登る、降りる、スニークし、壁に衝突する
- 残件 / 比較すべき点：26.2のCLIMBABLEタグ構成およびスニーク時の梯子下降抑制条件を照合する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/entity/LivingEntity.java:1610](../../../minecraft-26.2-decompiled/src/net/minecraft/world/entity/LivingEntity.java#L1610) — onClimbable：公式はCLIMBABLEタグ等から登攀状態を決定。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/entity/LivingEntity.java:2549](../../../minecraft-26.2-decompiled/src/net/minecraft/world/entity/LivingEntity.java#L2549) — handleOnClimbable：落下距離、速度制限、プレイヤー梯子下降抑制。
  - Rust：[Client/pomme-client/src/physics/movement.rs:1254](../../pomme-client/src/physics/movement.rs#L1254) — is_on_climbable：IDの列挙およびトラップドア条件を認識。
  - Rust：[Client/pomme-client/src/physics/movement.rs:703](../../pomme-client/src/physics/movement.rs#L703) — apply_collision_with_context：クライム速度制限とジャンプ/衝突時上昇の利用経路。

</details>

<a id="fluids-08"></a>

<details>
<summary>fluids-08 — 特殊環境 › クモの巣による移動阻害</summary>

- 比較環境：クライアント共通
- 対象条件：クモの巣に侵入し、通常時とWeaving効果中に移動する
- 残件 / 比較すべき点：ブロック内侵入時の適用回数、侵入前速度の保存/消去とWeaving条件を比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/level/block/WebBlock.java:34](../../../minecraft-26.2-decompiled/src/net/minecraft/world/level/block/WebBlock.java#L34) — entityInside：標準倍率とWeaving時倍率をmakeStuckInBlockに設定。
  - Rust：[Client/pomme-client/src/physics/movement.rs:737](../../pomme-client/src/physics/movement.rs#L737) — apply_collision_with_context：クモの巣交差時の倍率・速度消去・落下距離リセット。
  - Rust：[Client/pomme-client/src/app/core.rs:7811](../../pomme-client/src/app/core.rs#L7811) — movement::tick_with_context 呼出：特殊ブロック処理に到達する通常移動の接続。

</details>

<a id="fluids-09"></a>

<details>
<summary>fluids-09 — 特殊環境 › 粉雪の沈み込みと革ブーツ登攀</summary>

- 比較環境：クライアント共通
- 対象条件：粉雪内で歩く/落下する、革ブーツ有無、ジャンプと壁衝突を試す
- 残件 / 比較すべき点：深い落下時の粉雪衝突形状と革ブーツ歩行可能形状を確認する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/level/block/PowderSnowBlock.java:52](../../../minecraft-26.2-decompiled/src/net/minecraft/world/level/block/PowderSnowBlock.java#L52) — entityInside：0.9/1.5/0.9の内部移動倍率。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/level/block/PowderSnowBlock.java:118](../../../minecraft-26.2-decompiled/src/net/minecraft/world/level/block/PowderSnowBlock.java#L118) — canEntityWalkOnPowderSnow：革ブーツ装備者の歩行可能条件。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/entity/LivingEntity.java:2535](../../../minecraft-26.2-decompiled/src/net/minecraft/world/entity/LivingEntity.java#L2535) — handleRelativeFrictionAndCalculateMovement：水平衝突/ジャンプと登攀・粉雪歩行条件でvy=0.2。
  - Rust：[Client/pomme-client/src/physics/movement.rs:746](../../pomme-client/src/physics/movement.rs#L746) — apply_collision_with_context：粉雪の内部倍率と革ブーツ時上昇を適用。

</details>

<a id="fluids-10"></a>

<details>
<summary>fluids-10 — 特殊環境 › 蜂蜜ブロック側面の滑り</summary>

- 比較環境：クライアント共通
- 対象条件：蜂蜜ブロック側面を落下し、速度-0.13境界の上下で確認する
- 残件 / 比較すべき点：蜂蜜面への接触位置・高さ境界と速度閾値を同条件で比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/level/block/HoneyBlock.java:67](../../../minecraft-26.2-decompiled/src/net/minecraft/world/level/block/HoneyBlock.java#L67) — entityInside / isSlidingDown：落下中・ブロック上端高さ・旧vy境界で側面滑りを判定。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/level/block/HoneyBlock.java:91](../../../minecraft-26.2-decompiled/src/net/minecraft/world/level/block/HoneyBlock.java#L91) — doSlideMovement：旧vy&lt;-0.13時の水平減衰、滑り速度設定、落下距離リセット。
  - Rust：[Client/pomme-client/src/physics/movement.rs:853](../../pomme-client/src/physics/movement.rs#L853) — apply_collision_with_context：蜂蜜側面接触時の滑り適用箇所。
  - Rust：[Client/pomme-client/src/physics/movement.rs:985](../../pomme-client/src/physics/movement.rs#L985) — honey_slide_movement：旧速度の復元、水平倍率、垂直滑り速度を算出。

</details>


<a id="domain-interaction"></a>

## 攻撃・破壊・設置・使用

調査範囲：公式 MultiPlayerGameMode.java、LocalPlayer.java、Minecraft.java、Player.java、ItemCooldowns.java と Rust interaction.rs、cooldown.rs、input.rs、app/core.rs の対象選択・入力から処理/送信までを静的に照合。ブロック使用の個別Block実装全種、全EntityのmobInteract、全アイテム固有効果の網羅はしていません。

- 制約：調査は指定版の静的ソース照合のみで、サーバー接続・実機比較・build/testは行っていません。
- 制約：サーバー権威のブロック/Entity/アイテム効果はRustクライアントの未実装と扱わず、クライアント予測・packet送信と明確に区別しました。
- 制約：公式と意味的一致を実行確認していないため、明示したPIERCING_WEAPONおよび対象範囲以外の比較は未検証です。

| ID / 機能 | 公式の挙動 | Rustの現状・差分 | 実装 | 比較 / 検証 |
|---|---|---|---|---|
| [interaction-01](#interaction-01)<br>対象選択 › ブロック/Entity › 視線による選択 | 公式はブロックを先にレイ判定し、その位置までEntityを判定して近い方を選び、ブロック/Entityの相互作用範囲で結果を絞ります。 | Rustはcore tickからupdate_targetを呼び、ブロック4.5、Entityは通常3.0/クリエイティブ5.0の固定距離で判定します。公式の能力由来の範囲やアイテムAttackRangeコンポーネントはこの経路に反映されていません。 | 実装あり | 差分あり<br>静的照合 |
| [interaction-02](#interaction-02)<br>ブロック破壊 › 通常採掘 › 押下継続と破壊予測 | 公式は開始時にSTART_DESTROY_BLOCKを送り、破壊可能量をtickごとに加算し、完了時にSTOP_DESTROY_BLOCKとブロック破壊予測を行います。対象/手持ちが変われば採掘を切り替え、押下終了時はABORT_DESTROY_BLOCKを送ります。 | RustのDestroy入力からInteractionState.tick、start/continue_destroy_blockへ繋がり、同種コンポーネントの手持ちと対象位置を採掘セッションとして追跡します。完了時は予測ブロック消去とACK時の復元があり、進捗はMiningContextを使います。 | 実装あり | 未検証<br>静的照合 |
| [interaction-03](#interaction-03)<br>ブロック設置 › ブロックアイテム › 使用packetとローカル予測 | 公式は対象ブロック使用を先に試し、PASSなら手持ちアイテムのuseOnを実行します。設置可否や最終状態はアイテム/ブロック処理とサーバー応答に従います。 | RustはUseItemOnを送り、placeable blockについて候補位置・予測状態をローカル反映し、シーケンスACKで拒否時に戻します。個々のBlockState.useItemOn/useOn動作は一般的にはローカル再現せず、既知ブロックの分岐とサーバー権威に依存します。 | 一部実装 | 未検証<br>静的照合 |
| [interaction-04](#interaction-04)<br>Entity攻撃 › 通常攻撃入力 › Attack packet | 公式は攻撃入力時、対象Entityに攻撃packetを送り、ローカル攻撃処理後に攻撃強度tickerをリセットします。PIERCING_WEAPONコンポーネントがあれば通常攻撃でなくSTAB処理を選びます。 | RustはEntity hitにAttack packetを送り、tickerを0にしてメインハンドSwingを送ります。ただしstart_attackにfull-charge spear/PIERCING_WEAPON分岐がTODOとして明記され、通常攻撃経路に入ります。 | 一部実装 | 差分あり<br>静的照合 |
| [interaction-05](#interaction-05)<br>Entity使用 › 右クリック › 相互作用packetと手 | 公式は各手についてEntityの相互作用範囲を確認してinteractを送り、ローカルinteractOnの結果が成功なら相互作用を終え、成功種別に応じて手を振ります。 | RustはEntity対象でInteract packet（Entity相対hit位置、hand、sneaking）を送り、known ordinary cow以外は結果を保守的に扱います。成功/失敗結果を待つサーバー権威のためローカルで任意EntityのinteractOn効果は実行しません。 | 一部実装 | 未検証<br>静的照合 |
| [interaction-06](#interaction-06)<br>アイテム使用 › Use入力 › MainHand/OffHandと使用packet | 公式はMainHandからOffHandの順に使用を試し、ブロック相互作用が成功しなければ空中/アイテム使用へ進みます。item cooldown中はローカルitem.useを抑止し、UseItem packet自体は送ります。 | Rustはblock→use_itemの順に処理し、手ごとのUseItem/UseItemOnとcooldown状態を受け取ります。クールダウン時もUseItem packetを送ってからローカル使用を止めますが、block-use成功/PASS分類は一部の既知動作を近似しています。 | 一部実装 | 未検証<br>静的照合 |
| [interaction-07](#interaction-07)<br>アイテム使用 › 食料/長押しアイテム › 使用継続と解放 | 公式LocalPlayerは使用手を記録し、同期された使用フラグで開始/停止します。プレイヤーが使用を解放するとMultiPlayerGameModeはRELEASE_USE_ITEMを送り、ローカルの使用を解放します。 | RustはConsumable/Bow/Crossbow/Trident/Spyglass/Shield等の局所使用状態を記録し、使用ボタン解放でReleaseUseItemを送り、同期フラグ解除で状態を消します。消費完了や食料・所持数変更はサーバー応答に委ねます。 | 実装あり | 未検証<br>静的照合 |
| [interaction-08](#interaction-08)<br>クールダウン › アイテム使用グループ › 表示/使用抑止 | 公式ItemCooldownsはUseCooldownのcooldownGroup、なければitem IDをキーにし、tick数とdurationから残存率を求めます。duration 0は削除され、isOnCooldownは残存率が正ならtrueです。 | Rust CooldownTrackerも明示groupまたはitem IDを使い、ネットワークItemCooldownイベントで適用し、coreが両手の使用抑止に利用します。 | 実装あり | 未検証<br>静的照合 |
| [interaction-09](#interaction-09)<br>攻撃クールダウン › MainHandアイテム変更 › 攻撃強度 | 公式は毎tick攻撃強度tickerを進め、手持ちItemStackが変わった際、item type変更ならtickerをresetします。強度はticker/現在の攻撃遅延で算出し、MinimumAttackCharge条件を満たさない攻撃を抑止します。 | RustはInteractionState.tickでtickerを進め、item種別変更時にresetし、AttackSpeed attribute modifiersから遅延を算出します。MinimumAttackChargeもstart_attackで確認します。 | 実装あり | 未検証<br>静的照合 |
| [interaction-10](#interaction-10)<br>手の操作 › Hotbar選択 › server carried slot同期 | 公式は選択中のhotbar indexがcarriedIndexと異なる場合にServerboundSetCarriedItemを送信し、操作対象アイテムをサーバーと同期します。 | Rust inputが数字キー/scrollでselected_slotを変え、core tickからInteractionState.tickへ渡し、ensure_has_sent_carried_itemが変更時のみpacketを送ります。初期slot 0は既に選択済みとして送信を省きます。 | 実装あり | 未検証<br>静的照合 |

### 条件・残件・根拠

<a id="interaction-01"></a>

<details>
<summary>interaction-01 — 対象選択 › ブロック/Entity › 視線による選択</summary>

- 比較環境：クライアント共通
- 対象条件：既定範囲以外の相互作用範囲またはAttackRange付きアイテムで対象を選ぶ
- 残件 / 比較すべき点：属性で範囲を変更したプレイヤー、およびAttackRangeコンポーネント装備時の選択範囲・対象を確認する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/player/LocalPlayer.java:1209](../../../minecraft-26.2-decompiled/src/net/minecraft/client/player/LocalPlayer.java#L1209) — pick：blockInteractionRange/entityInteractionRangeを受け、先にブロックhitを取りEntity距離を比較しrangeでfilterする。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/player/LocalPlayer.java:1191](../../../minecraft-26.2-decompiled/src/net/minecraft/client/player/LocalPlayer.java#L1191) — raycastHitResult：アイテムAttackRangeを先に試し、通常pickにfallbackする。
  - Rust：[Client/pomme-client/src/player/interaction.rs:840](../../pomme-client/src/player/interaction.rs#L840) — InteractionState::update_target：REACH定数とENTITY_REACH/CREATIVE_ENTITY_REACH_BONUSで選択する。
  - Rust：[Client/pomme-client/src/app/core.rs:7634](../../pomme-client/src/app/core.rs#L7634) — core tick：プレイヤー視点・ChunkStore・EntityStoreを渡して本番選択を更新する。

</details>

<a id="interaction-02"></a>

<details>
<summary>interaction-02 — ブロック破壊 › 通常採掘 › 押下継続と破壊予測</summary>

- 比較環境：公式サーバー接続
- 対象条件：サバイバルで読み込み済みの通常ブロックを押し続けて破壊し、途中で視点/手持ちを変える
- 残件 / 比較すべき点：各ブロック・採掘速度要因で完了tick、開始/中断/完了packet順序、および拒否ACKでの復元を公式と比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/MultiPlayerGameMode.java:151](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/MultiPlayerGameMode.java#L151) — startDestroyBlock：ワールド境界・ゲームモードを見て開始packetと予測を行う。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/MultiPlayerGameMode.java:227](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/MultiPlayerGameMode.java#L227) — continueDestroyBlock：破壊進捗を加算し、完了時STOP_DESTROY_BLOCK予測を行う。
  - Rust：[Client/pomme-client/src/player/interaction.rs:2174](../../pomme-client/src/player/interaction.rs#L2174) — start_destroy_block：開始/即時破壊packetと破壊予測を管理する。
  - Rust：[Client/pomme-client/src/player/interaction.rs:2264](../../pomme-client/src/player/interaction.rs#L2264) — continue_destroy_block：遅延、対象/アイテム継続性、進捗加算、STOPと予測を処理する。
  - Rust：[Client/pomme-client/src/player/interaction.rs:879](../../pomme-client/src/player/interaction.rs#L879) — InteractionState::tick：入力Destroyをstart_attack/continue_attackに接続する。

</details>

<a id="interaction-03"></a>

<details>
<summary>interaction-03 — ブロック設置 › ブロックアイテム › 使用packetとローカル予測</summary>

- 比較環境：公式サーバー接続
- 対象条件：対象面または置換可能セルに通常のplaceable block itemを使用する
- 残件 / 比較すべき点：置換規則、向き・状態、手持ち減少、失敗ACK、および個別ブロックがPASS/CONSUMEする場合を比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/MultiPlayerGameMode.java:325](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/MultiPlayerGameMode.java#L325) — useItemOn：境界検査後performUseItemOnを予測処理で実行しUseItemOn packetを送る。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/MultiPlayerGameMode.java:354](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/MultiPlayerGameMode.java#L354) — performUseItemOn：ブロックuseItemOnが消費しなければ手持ちitem.useOnへ進む。
  - Rust：[Client/pomme-client/src/player/interaction.rs:1509](../../pomme-client/src/player/interaction.rs#L1509) — start_use_item：ブロックhitでUseItemOnを送り、既知の相互作用分岐後にtry_place_blockへ進む。
  - Rust：[Client/pomme-client/src/player/interaction.rs:2135](../../pomme-client/src/player/interaction.rs#L2135) — try_place_block：設置候補を予測し、server ACK用の予測状態を登録する。

</details>

<a id="interaction-04"></a>

<details>
<summary>interaction-04 — Entity攻撃 › 通常攻撃入力 › Attack packet</summary>

- 比較環境：公式サーバー接続
- 対象条件：PIERCING_WEAPON付き手持ちアイテムでEntityを攻撃する
- 残件 / 比較すべき点：この条件でRustが通常Attack packetを出すのに対し、公式はSTAB player-actionと固有処理を行う差分を実機packet/結果で確認する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/Minecraft.java:1736](../../../minecraft-26.2-decompiled/src/net/minecraft/client/Minecraft.java#L1736) — startAttack：PIERCING_WEAPON時はpiercingAttackを選び、ENTITY通常分岐より先にreturnする。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/MultiPlayerGameMode.java:427](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/MultiPlayerGameMode.java#L427) — attack：通常攻撃packet送信後にPlayer攻撃とticker resetを行う。
  - Rust：[Client/pomme-client/src/player/interaction.rs:1264](../../pomme-client/src/player/interaction.rs#L1264) — start_attack：Entity対象ではencode_attackとticker resetを行い、先頭コメントでPIERCING_WEAPON未対応を示す。

</details>

<a id="interaction-05"></a>

<details>
<summary>interaction-05 — Entity使用 › 右クリック › 相互作用packetと手</summary>

- 比較環境：公式サーバー接続
- 対象条件：通常Entityと相互作用可能EntityをMainHand/OffHandで使用する
- 残件 / 比較すべき点：EntityごとのPASS/CONSUME、手の試行順、相互作用成功時のSwing、範囲外時のpacket有無を確認する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/Minecraft.java:1774](../../../minecraft-26.2-decompiled/src/net/minecraft/client/Minecraft.java#L1774) — startUseItem：各InteractionHandでEntity hitを処理し、相互作用成功ならreturnする。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/MultiPlayerGameMode.java:442](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/MultiPlayerGameMode.java#L442) — interact：相対hit位置付きpacketを送り、spectator以外ではinteractOnを実行する。
  - Rust：[Client/pomme-client/src/player/interaction.rs:1405](../../pomme-client/src/player/interaction.rs#L1405) — start_use_item：Entity hitをInteract packetへ変換し、限定した動物particle分岐以外はサーバー結果を保守的に扱う。
  - Rust：[Client/pomme-client/src/player/interaction.rs:1700](../../pomme-client/src/player/interaction.rs#L1700) — start_use_item：block useがPASSした場合のoffhand packetと使用経路を繋ぐ。

</details>

<a id="interaction-06"></a>

<details>
<summary>interaction-06 — アイテム使用 › Use入力 › MainHand/OffHandと使用packet</summary>

- 比較環境：公式サーバー接続
- 対象条件：空中使用およびブロック上でMainHandがPASSしOffHandが使用可能な場合
- 残件 / 比較すべき点：全ブロック結果のPASS/CONSUME分類、両手のpacket順、cooldown中のUseItem送信とローカル効果抑止を比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/Minecraft.java:1774](../../../minecraft-26.2-decompiled/src/net/minecraft/client/Minecraft.java#L1774) — startUseItem：InteractionHand列を走査し、ブロック使用結果を先に扱う。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/MultiPlayerGameMode.java:384](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/MultiPlayerGameMode.java#L384) — useItem：UseItem packetを構成し、cooldownならitem.useを実行せずPASSにする。
  - Rust：[Client/pomme-client/src/player/interaction.rs:1405](../../pomme-client/src/player/interaction.rs#L1405) — start_use_item：block interactionの後にuse_item、PASS時にoffhandを試す。
  - Rust：[Client/pomme-client/src/player/interaction.rs:1774](../../pomme-client/src/player/interaction.rs#L1774) — use_item：UseItem送信の後にhand_on_cooldownを見てローカル使用を中断する。
  - Rust：[Client/pomme-client/src/app/core.rs:7649](../../pomme-client/src/app/core.rs#L7649) — core tick：両手のcooldown状態をInteractionState.tickへ渡す。

</details>

<a id="interaction-07"></a>

<details>
<summary>interaction-07 — アイテム使用 › 食料/長押しアイテム › 使用継続と解放</summary>

- 比較環境：公式サーバー接続
- 対象条件：食料を使用中にボタンを保持/解放し、サーバー完了イベントを受ける
- 残件 / 比較すべき点：使用開始tick、アニメ/移動抑制、解放packet、早期解放時の消費中止、完了イベントによる所持数変化を比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/player/LocalPlayer.java:532](../../../minecraft-26.2-decompiled/src/net/minecraft/client/player/LocalPlayer.java#L532) — startUsingItem：使用手とstartedUsingItemを設定し、同期フラグ更新でも使用状態を同期する。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/MultiPlayerGameMode.java:509](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/MultiPlayerGameMode.java#L509) — releaseUsingItem：RELEASE_USE_ITEMを送ってPlayer.releaseUsingItemを呼ぶ。
  - Rust：[Client/pomme-client/src/player/interaction.rs:1774](../../pomme-client/src/player/interaction.rs#L1774) — use_item：Consumableと特殊アイテムの使用状態を生成する。
  - Rust：[Client/pomme-client/src/player/interaction.rs:2042](../../pomme-client/src/player/interaction.rs#L2042) — release_using_item：使用解放packetを送りローカルactive use/latchを解除する。
  - Rust：[Client/pomme-client/src/player/interaction.rs:1938](../../pomme-client/src/player/interaction.rs#L1938) — sync_using_item_flag：サーバー使用フラグ解除でローカル使用を終了する。

</details>

<a id="interaction-08"></a>

<details>
<summary>interaction-08 — クールダウン › アイテム使用グループ › 表示/使用抑止</summary>

- 比較環境：クライアント共通
- 対象条件：明示group共有、既定item別group、duration 0削除、tick満了を含むcooldown
- 残件 / 比較すべき点：共有group複数item、負duration、partial tick表示、終了境界と再適用時刻を比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/item/ItemCooldowns.java:23](../../../minecraft-26.2-decompiled/src/net/minecraft/world/item/ItemCooldowns.java#L23) — isOnCooldown/getCooldownPercent：残存率の正値をcooldownとしtick/durationで進捗を計算する。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/item/ItemCooldowns.java:51](../../../minecraft-26.2-decompiled/src/net/minecraft/world/item/ItemCooldowns.java#L51) — getCooldownGroup：UseCooldown groupかitem registry IDを用いる。
  - Rust：[Client/pomme-client/src/player/cooldown.rs:20](../../pomme-client/src/player/cooldown.rs#L20) — CooldownTracker::apply：duration 0削除、それ以外はtick基準でendを保存する。
  - Rust：[Client/pomme-client/src/player/cooldown.rs:40](../../pomme-client/src/player/cooldown.rs#L40) — CooldownTracker::fraction：残存率とis_on_cooldownを計算する。
  - Rust：[Client/pomme-client/src/app/core.rs:4635](../../pomme-client/src/app/core.rs#L4635) — NetworkEvent::ItemCooldown：受信イベントからtrackerへcooldownを適用する。

</details>

<a id="interaction-09"></a>

<details>
<summary>interaction-09 — 攻撃クールダウン › MainHandアイテム変更 › 攻撃強度</summary>

- 比較環境：クライアント共通
- 対象条件：速度modifierの異なる武器への持ち替えとMinimumAttackCharge付きアイテム
- 残件 / 比較すべき点：modifier操作/slot適用、武器変更tick、攻撃入力直前の強度境界を公式値と照合する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/entity/player/Player.java:262](../../../minecraft-26.2-decompiled/src/net/minecraft/world/entity/player/Player.java#L262) — Player.tick：attackStrengthTickerを進め、ItemStack変更時はitem type差でresetする。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/entity/player/Player.java:1703](../../../minecraft-26.2-decompiled/src/net/minecraft/world/entity/player/Player.java#L1703) — getCurrentItemAttackStrengthDelay/cannotAttackWithItem：攻撃遅延を攻撃速度から算出しMinimumAttackChargeを判定する。
  - Rust：[Client/pomme-client/src/player/interaction.rs:1229](../../pomme-client/src/player/interaction.rs#L1229) — tick_attack_cooldown：ticker更新とitem type変更時のリセットを行う。
  - Rust：[Client/pomme-client/src/player/interaction.rs:1249](../../pomme-client/src/player/interaction.rs#L1249) — attack_strength_scale/attack_strength_delay：AttackSpeed modifier由来の遅延とscaleを提供する。
  - Rust：[Client/pomme-client/src/player/interaction.rs:1255](../../pomme-client/src/player/interaction.rs#L1255) — cannot_attack_with_item：MinimumAttackChargeに届かない攻撃を拒否する。

</details>

<a id="interaction-10"></a>

<details>
<summary>interaction-10 — 手の操作 › Hotbar選択 › server carried slot同期</summary>

- 比較環境：公式サーバー接続
- 対象条件：slot 0から別hotbar slotへ切替後、そのslotでブロック/アイテム操作を行う
- 残件 / 比較すべき点：数字/scroll入力、screen capture時抑止、複数slot連続変更とinteraction packet順を比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/MultiPlayerGameMode.java:317](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/MultiPlayerGameMode.java#L317) — ensureHasSentCarriedItem：selected index変更時にSetCarriedItemを送信する。
  - Rust：[Client/pomme-client/src/app/input.rs:713](../../pomme-client/src/app/input.rs#L713) — InputState::on_key_event：入力イベントでhotbar slot選択を更新する。
  - Rust：[Client/pomme-client/src/player/interaction.rs:2342](../../pomme-client/src/player/interaction.rs#L2342) — ensure_has_sent_carried_item：選択slot変更時にtyped SetCarriedItem packetを送る。
  - Rust：[Client/pomme-client/src/app/core.rs:7715](../../pomme-client/src/app/core.rs#L7715) — core tick：selected_slotを渡してinteraction入力処理を実行する。

</details>


<a id="domain-inventory"></a>

## 在庫・コンテナ操作

調査範囲：指定されたmenu_click.rs、inventory.rs、container.rs、crafting_table.rs、recipe_book.rs、merchant.rs、creative_inventory.rsと本番経路のapp/phases/in_game.rsを確認し、公式26.2のAbstractContainerMenu、InventoryMenu、CraftingMenu、RecipeBookComponent、MerchantScreen/MerchantMenu、CreativeModeInventoryScreenを照合。全コンテナ・全アイテムの互換性は調査しておらず、個別に記したクリック経路とUI動作に限定する。

- 制約：ビルド・cargo test・アプリ起動は行っていない。テスト記述を根拠にしている箇所も、テスト実行済みではない。
- 制約：差分ありは公式とRustの現在ソースに明示された保存hotbar tab未搭載と、クリエイティブ画面外dropの欠落に限定した。
- 制約：container clickはサーバー権威で、Rust側のローカル予測がないmenu種別・結果枠がある。全menuに共通する予測対応とは判断していない。

| ID / 機能 | 公式の挙動 | Rustの現状・差分 | 実装 | 比較 / 検証 |
|---|---|---|---|---|
| [inventory-01](#inventory-01)<br>プレイヤー在庫 › 通常スロット › 左右クリックで取得・配置 | 空カーソルの左クリックはスタック全数、右クリックは端数切り上げの半数を取り、配置時は左で可能数、右で1個を置く。 | Player inventory menuについてこの分岐を予測し、変更スロットとカーソルを更新してcontainer click packetに載せる本番経路がある。 | 実装あり | 未検証<br>静的照合 |
| [inventory-02](#inventory-02)<br>ホッパー › シフトクリック › ホッパーとプレイヤー在庫間の移動 | ホッパー内のアイテムはプレイヤー側へ、プレイヤー側のアイテムはホッパー側へquick-moveし、移動先の空き・既存スタックに従う。 | ホッパー固有のスロット範囲を使って両方向のquick-moveを予測し、満杯なら移動しない。範囲と移動結果を確認するunit test記述もあるが、今回は実行していない。 | 実装あり | 未検証<br>静的照合 |
| [inventory-03](#inventory-03)<br>プレイヤークラフト › 結果枠 › 取得時のサーバー権威処理 | クラフト結果の取得はレシピ材料消費などを伴うため、結果枠クリックはメニュー/サーバー側のクラフト処理を通る。 | プレイヤーおよびCraftingTableの結果slotを予測対象外にし、変更をローカル適用せずクリック操作をサーバーへ送る。 | 実装あり | 未検証<br>静的照合 |
| [inventory-04](#inventory-04)<br>コンテナ操作 › クリックドラッグ › 複数枠への分配 | ドラッグを開始・継続・終了し、対象slotへ均等/1個ずつ（クリエイティブではclone分配）を配置し、残量をカーソルに残す。 | 通常対応コンテナではドラッグ対象枠を収集し、終了時に分配予測・slot更新をして、quick-craft操作を送信する。いくつかの複雑menu種別は分配予測を無効化している。 | 一部実装 | 未検証<br>静的照合 |
| [inventory-05](#inventory-05)<br>クラフトレシピブック › 検索・作成可能フィルター | 検索文字列でレシピ表示を絞り、作成可能フィルターを切り替えると、材料在庫に応じた作成可能レシピだけを表示する。 | 受信したrecipe display/category/groupを対象に検索し、要求材料を割り当てて作成可能なentryを絞り込む。要求材料やタグ情報が不明なentryは作成可能として表示しない。 | 一部実装 | 未検証<br>静的照合 |
| [inventory-06](#inventory-06)<br>クラフトレシピブック › レシピ選択 › 材料配置要求 | 選んだレシピとcontainer IDをplace-recipe要求としてサーバーへ送り、サーバーが材料配置を処理する。 | 画面から選択recipe ID/use_max_itemsを受け、native PlaceRecipeのpacketを送る経路がある。ゴーストrecipeはcontainer ID一致時に限り状態へ反映する。 | 実装あり | 未検証<br>静的照合 |
| [inventory-07](#inventory-07)<br>村人取引 › 取引一覧 › オファー選択 | 表示された取引行を選ぶと、選択したglobal offer indexをServerboundSelectTradeとして送る。 | 表示オファーを7件単位でスクロールし、行クリックで全体indexを選択、別packet送信経路へ渡す。 | 実装あり | 未検証<br>静的照合 |
| [inventory-08](#inventory-08)<br>クリエイティブ在庫 › セーブ済みホットバーtab | Hotbar tabで9本の保存hotbarを参照し、選択中の保存内容を表示・呼び出せる。 | Hotbar variantのmetadataはあるが、画面に列挙するTABSから除外され、itemsもEmptyなので保存hotbar画面を利用できない。 | 未実装 | 差分あり<br>静的照合 |
| [inventory-09](#inventory-09)<br>クリエイティブ在庫 › アイテム一覧 › middle-clickで最大stack複製 | creative item picker上のアイテムをmiddle-clickすると最大stackをカーソルに複製する。 | カーソルが空で一覧枠にmiddle-clickした場合、アイテム種のmax stack countでcursor itemを作る。 | 実装あり | 未検証<br>静的照合 |
| [inventory-10](#inventory-10)<br>クリエイティブ在庫 › アイテム保持中の画面外クリック › アイテムdrop | アイテム保持中に画面外を左クリックするとアイテムをworldへdropし、クリエイティブdrop packetを送る。 | 画面外クリックでcursor itemをEmptyにするだけで、drop送信処理はTODOとして未実装。 | 一部実装 | 差分あり<br>静的照合 |

### 条件・残件・根拠

<a id="inventory-01"></a>

<details>
<summary>inventory-01 — プレイヤー在庫 › 通常スロット › 左右クリックで取得・配置</summary>

- 比較環境：公式サーバー接続
- 対象条件：プレイヤー在庫の通常スロットで、結果スロット以外を左/右クリックする。
- 残件 / 比較すべき点：サーバー応答後のスロット・カーソル同期を、左右クリックとスタック上限で比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/inventory/AbstractContainerMenu.java:447](../../../minecraft-26.2-decompiled/src/net/minecraft/world/inventory/AbstractContainerMenu.java#L447) — doClick：通常pickupのクリック種別を処理し、左右で取得量を分ける。
  - Rust：[Client/pomme-client/src/player/menu_click.rs:485](../../pomme-client/src/player/menu_click.rs#L485) — pickup_click：左右クリックの取得・配置量とカーソルを計算する。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:3398](../../pomme-client/src/app/phases/in_game.rs#L3398) — send_container_click：予測結果を適用し、変更スロットとcarried itemをpacketに含める。

</details>

<a id="inventory-02"></a>

<details>
<summary>inventory-02 — ホッパー › シフトクリック › ホッパーとプレイヤー在庫間の移動</summary>

- 比較環境：公式サーバー接続
- 対象条件：ホッパーmenuで通常スタックをホッパー枠またはプレイヤー枠からシフトクリックする。
- 残件 / 比較すべき点：ホッパーの部分空き、満杯、ホットバー優先順を公式サーバーで比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/inventory/HopperMenu.java:41](../../../minecraft-26.2-decompiled/src/net/minecraft/world/inventory/HopperMenu.java#L41) — HopperMenu.quickMoveStack：ホッパーmenuのquick-moveを実装する公式入口。
  - Rust：[Client/pomme-client/src/player/menu_click.rs:568](../../pomme-client/src/player/menu_click.rs#L568) — quick_move：Hopperの両方向をinv_startで分ける。
  - Rust：[Client/pomme-client/src/player/menu_click.rs:802](../../pomme-client/src/player/menu_click.rs#L802) — hopper_layout_and_shift_transfer_both_directions：スロット範囲と両方向移動のテスト記述（未実行）。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:3413](../../pomme-client/src/app/phases/in_game.rs#L3413) — send_container_click：予測した変更をcontainer click packetとして送る。

</details>

<a id="inventory-03"></a>

<details>
<summary>inventory-03 — プレイヤークラフト › 結果枠 › 取得時のサーバー権威処理</summary>

- 比較環境：公式サーバー接続
- 対象条件：プレイヤー2x2クラフトの結果slot 0をクリックする。
- 残件 / 比較すべき点：レシピ材料、結果の連続取得、材料不足時にサーバー確定後のslotとカーソルを比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/inventory/InventoryMenu.java:55](../../../minecraft-26.2-decompiled/src/net/minecraft/world/inventory/InventoryMenu.java#L55) — InventoryMenu.constructor：結果枠とクラフトグリッドをmenu slotとして登録する。
  - Rust：[Client/pomme-client/src/player/menu_click.rs:87](../../pomme-client/src/player/menu_click.rs#L87) — ContainerKind::crafting_result_slot：Player/CraftingTableの結果slotを0として定義する。
  - Rust：[Client/pomme-client/src/player/menu_click.rs:266](../../pomme-client/src/player/menu_click.rs#L266) — apply_click：結果slot操作は予測せずサーバー権威に委ねる。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:3413](../../pomme-client/src/app/phases/in_game.rs#L3413) — send_container_click：非予測操作もサーバーへ送るクリックpacketを構築する。

</details>

<a id="inventory-04"></a>

<details>
<summary>inventory-04 — コンテナ操作 › クリックドラッグ › 複数枠への分配</summary>

- 比較環境：公式サーバー接続
- 対象条件：Playerまたはホッパーmenuで、カーソルにスタックを持って複数の有効枠をドラッグする。
- 残件 / 比較すべき点：左/右/中ドラッグそれぞれの個数配分、枠制限、非対応menuでのサーバー再同期を比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/inventory/AbstractContainerMenu.java:384](../../../minecraft-26.2-decompiled/src/net/minecraft/world/inventory/AbstractContainerMenu.java#L384) — doClick：QUICK_CRAFT状態で対象slotを収集する公式処理。
  - Rust：[Client/pomme-client/src/player/menu_click.rs:298](../../pomme-client/src/player/menu_click.rs#L298) — drag_distribution：対象slot群にkind別分配を計算する。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:3382](../../pomme-client/src/app/phases/in_game.rs#L3382) — send_container_click：End時に変更slotと残りcursorを適用する。
  - Rust：[Client/pomme-client/src/ui/container.rs:1742](../../pomme-client/src/ui/container.rs#L1742) — resolve_gesture：画面入力から開始/追加/終了のgestureを作る。

</details>

<a id="inventory-05"></a>

<details>
<summary>inventory-05 — クラフトレシピブック › 検索・作成可能フィルター</summary>

- 比較環境：クライアント共通
- 対象条件：クラフト台レシピブックで、既知のrecipe display/requirementsと在庫を使い検索またはfilterを操作する。
- 残件 / 比較すべき点：named ingredient、重複材料、複合displayおよび翻訳名検索を公式と比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/recipebook/RecipeBookComponent.java:234](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/recipebook/RecipeBookComponent.java#L234) — updateCollections：公式の検索文字列でrecipe collectionを絞り込む。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/recipebook/RecipeBookComponent.java:134](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/recipebook/RecipeBookComponent.java#L134) — init：作成可能フィルターbuttonを構築する。
  - Rust：[Client/pomme-client/src/ui/recipe_book.rs:212](../../pomme-client/src/ui/recipe_book.rs#L212) — RecipeBookState::collections：カテゴリ・grid適合・検索・craftable_onlyによる表示選別。
  - Rust：[Client/pomme-client/src/ui/recipe_book.rs:429](../../pomme-client/src/ui/recipe_book.rs#L429) — RecipeBookState::can_craft：既知のrequirementsとtag・available item数で作成可能性を判定する。

</details>

<a id="inventory-06"></a>

<details>
<summary>inventory-06 — クラフトレシピブック › レシピ選択 › 材料配置要求</summary>

- 比較環境：公式サーバー接続
- 対象条件：開いているクラフト台で受信済みrecipeを選択する。
- 残件 / 比較すべき点：材料を一部だけ所持する場合の配置、use-max指定、別menuの古いghost packet拒否を照合する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/recipebook/RecipeBookComponent.java:383](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/recipebook/RecipeBookComponent.java#L383) — tryPlaceRecipe：公式でrecipe選択を処理しcontainer ID/recipe/useMaxを送る。
  - Rust：[Client/pomme-client/src/ui/crafting_table.rs:94](../../pomme-client/src/ui/crafting_table.rs#L94) — build_crafting_table：レシピ選択IDをrecipe book UIから取得する。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:5826](../../pomme-client/src/app/phases/in_game.rs#L5826) — render_container_screen：container ID・recipe display ID・use_max指定をplace_recipe送信へ渡す。
  - Rust：[Client/pomme-client/src/ui/recipe_book.rs:199](../../pomme-client/src/ui/recipe_book.rs#L199) — RecipeBookState::receive_ghost：active container ID一致時だけゴーストrecipeを受け入れる。

</details>

<a id="inventory-07"></a>

<details>
<summary>inventory-07 — 村人取引 › 取引一覧 › オファー選択</summary>

- 比較環境：公式サーバー接続
- 対象条件：8件以上ある村人オファーの、スクロール後に表示された行を選択する。
- 残件 / 比較すべき点：スクロール端・更新後の選択index維持、およびサーバーのpayment slot更新を比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/inventory/MerchantScreen.java:62](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/inventory/MerchantScreen.java#L62) — selectTrade：選択オファーをmenu hintへ設定する。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/inventory/MerchantScreen.java:64](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/inventory/MerchantScreen.java#L64) — selectTrade：ServerboundSelectTradePacketを送信する。
  - Rust：[Client/pomme-client/src/ui/merchant.rs:147](../../pomme-client/src/ui/merchant.rs#L147) — build_merchant：可視行クリックをスクロール込みのoffer indexへ変換する。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:5817](../../pomme-client/src/app/phases/in_game.rs#L5817) — render_container_screen：選択indexをselect_trade packet送信へ渡す。

</details>

<a id="inventory-08"></a>

<details>
<summary>inventory-08 — クリエイティブ在庫 › セーブ済みホットバーtab</summary>

- 比較環境：クライアント共通
- 対象条件：クリエイティブモードで保存済みhotbarを呼び出す。
- 残件 / 比較すべき点：保存・読込・tab表示・ホットバーへの適用は未実装範囲として別途確認する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/inventory/CreativeModeInventoryScreen.java:540](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/inventory/CreativeModeInventoryScreen.java#L540) — selectTab：HOTBAR tab選択時にHotbarManagerを使って保存内容を表示する。
  - Rust：[Client/pomme-client/src/ui/creative_inventory.rs:144](../../pomme-client/src/ui/creative_inventory.rs#L144) — CreativeTab::meta：HotbarはEmpty sourceと定義されている。
  - Rust：[Client/pomme-client/src/ui/creative_inventory.rs:239](../../pomme-client/src/ui/creative_inventory.rs#L239) — TABS：表示対象12tabの配列にHotbarが含まれない。

</details>

<a id="inventory-09"></a>

<details>
<summary>inventory-09 — クリエイティブ在庫 › アイテム一覧 › middle-clickで最大stack複製</summary>

- 比較環境：クライアント共通
- 対象条件：クリエイティブのアイテム一覧タブで、カーソルを空にしてitem上をmiddle-clickする。
- 残件 / 比較すべき点：スタック上限コンポーネントを持つアイテムと、既にcursor itemがある場合の公式挙動を比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/inventory/AbstractContainerMenu.java:552](../../../minecraft-26.2-decompiled/src/net/minecraft/world/inventory/AbstractContainerMenu.java#L552) — doClick：CLONEはinfinite materialsかつ空カーソル時にslot.safeCloneを行う。
  - Rust：[Client/pomme-client/src/ui/creative_inventory.rs:490](../../pomme-client/src/ui/creative_inventory.rs#L490) — build_creative_inventory：middle clickかつ空cursorで表示itemの最大stackを設定する。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:5842](../../pomme-client/src/app/phases/in_game.rs#L5842) — render_creative_inventory：build_creative_inventoryをフレーム描画経路から呼ぶ。

</details>

<a id="inventory-10"></a>

<details>
<summary>inventory-10 — クリエイティブ在庫 › アイテム保持中の画面外クリック › アイテムdrop</summary>

- 比較環境：公式サーバー接続
- 対象条件：クリエイティブ在庫画面でitemをcursorに持ち、tabではない画面外を左クリックする。
- 残件 / 比較すべき点：カーソル内スタック全数dropとサーバー生成item entityの有無を確認する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/inventory/CreativeModeInventoryScreen.java:204](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/inventory/CreativeModeInventoryScreen.java#L204) — slotClicked：通常inventory tab以外の画面外クリックにdrop処理がある。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/inventory/CreativeModeInventoryScreen.java:210](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/inventory/CreativeModeInventoryScreen.java#L210) — slotClicked：左クリック時にcursor全数をdropしcreative drop handlerへ渡す。
  - Rust：[Client/pomme-client/src/ui/creative_inventory.rs:656](../../pomme-client/src/ui/creative_inventory.rs#L656) — build_creative_inventory：outside click時にcursor itemを消去するがdropせず、TODOで明示。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:5868](../../pomme-client/src/app/phases/in_game.rs#L5868) — render_creative_inventory：実装済みSetSlot/SetSlotsのみcreative slot packetに変換する。

</details>


<a id="domain-hud"></a>

## HUD・情報表示

調査範囲：指定された hud/boss_bar/title/player_tab/toast/spectator_menu/waypoints の状態・描画処理と、app/phases/in_game.rs の描画呼出し、app/core.rs の関連ネットワークイベント適用を照合。公式26.2の Hud、BossHealthOverlay、PlayerTabOverlay、ToastManager、SpectatorGui、LocatorBar と ContextualBar 系を確認。音字幕、inventory、チャットは対象外。

- 制約：静的コード照合のみで、実機表示・サーバー接続・ビジュアル比較は行っていないため、全行comparisonは未検証。
- 制約：HUD表示実装の存在は確認したが、実機での画像/配置/アニメーション一致を意味しない。
- 制約：bossbarのdarken-screen/play-music/create-world-fog効果はboss_bar.rs内に未実装コメントがあり、表示以外の互換性は限定的。
- 制約：toastはadvancement/recipe/tutorialを確認した範囲であり、公式now-playing toastとの対応は確認できない。
- 制約：spectator menuの描画経路は確認したが、入力経路全体や通信後の遷移までは照合範囲外。
- 制約：HUD以外の音字幕・inventory・チャットは意図的に除外。

| ID / 機能 | 公式の挙動 | Rustの現状・差分 | 実装 | 比較 / 検証 |
|---|---|---|---|---|
| [hud-01](#hud-01)<br>HUD › 体力・空腹・空気 › サバイバル状態表示 | 体力・吸収・防具・空腹をHUDに描き、空気が満タンでなくなるか水中なら泡を表示する。 | build_hud はサバイバル時に体力/吸収、防具、騎乗時以外の空腹を描画し、air_bubbles と泡描画で空気を表示する。in_game の build_hud 呼出しで本番描画経路に接続されている。 | 実装あり | 未検証<br>静的照合 |
| [hud-02](#hud-02)<br>HUD › hotbar › 選択枠と最大9スロットのアイテム表示 | 通常モードでは画面下部にhotbar背景・選択枠・最大9枠のアイテムアイコンを表示し、個数も重ねて表示する。 | build_hud が非spectator時に選択枠と先頭9スロットのアイコン/個数を描画する。描画経路はin_gameから接続されている。 | 実装あり | 未検証<br>静的照合 |
| [hud-03](#hud-03)<br>HUD › 経験値 › 経験値バーとレベル数 | 経験値が選択される場合は経験値バーを表示し、経験値レベルが正ならバー上にレベル数を表示する。 | build_hud はContextualBarKind::Experienceでバー背景/進捗を描き、サバイバルの正の経験値レベルをバー上に描く。bar種別などは呼出し側から受け取る。 | 実装あり | 未検証<br>静的照合 |
| [hud-04](#hud-04)<br>HUD › bossbar › 進捗・色・名前 | サーバーから受け取ったboss eventを順に表示し、名前・色/ノッチ形式・進捗バーを更新する。 | BossBarStateはイベントを保持し、進捗を補間してbuild_hudでバー/名前を描画します。現行呼出経路ではdarken-screenがtick時のlightmap darkeningに使われ、create-world-fogは環境/lightmap属性評価に渡されます。play-musicの効果接続と各効果の公式同等性は未確認です。 | 実装あり | 未検証<br>静的照合 |
| [hud-05](#hud-05)<br>HUD › title/subtitle › フェード付き中央表示 | titleのfade-in/stay/fade-out時間に応じて中央にtitleを表示し、設定済みsubtitleも併せて表示する。 | TitleStateはタイトル設定時にカウントダウンを開始し、タイトル有効時だけsubtitleを表示する。tickで減算し、buildが部分tickを用いてalphaを算出する。 | 実装あり | 未検証<br>静的照合 |
| [hud-06](#hud-06)<br>HUD › tab一覧 › プレイヤー、header/footer、ping、LIST score | 要求中にlisted playerを一覧し、名前/顔/pingを描画する。LIST objectiveがあればspectator以外にスコアまたはheart表示を追加し、header/footerも表示する。 | player_tabはlisted一覧、チーム装飾名、顔、ping、header/footerおよびLIST score/heart表示を構築し、in_gameが表示条件を満たす場合に呼び出す。 | 実装あり | 未検証<br>静的照合 |
| [hud-07](#hud-07)<br>HUD › scoreboard › sidebar objectiveと最大15行 | 選択されたsidebar objectiveのscoreを表示し、非表示entryを除きスコア降順等で最大15行を描画する。 | Scoreboardがobjective/score/team/display-slotを保持し、build_scoreboardはteam別sidebar選択、# owner除外、score降順・名前順、最大15行、番号書式を使って描画する。 | 実装あり | 未検証<br>静的照合 |
| [hud-08](#hud-08)<br>HUD › toast › advancement/recipe通知のキュー表示 | toastを空きslotに割り当て、表示/退場アニメーションを更新してHUDに描画する。公式はtoast種別ごとの通知に加え、設定が有効ならnow-playing toastも扱う。 | Rustはadvancement完了/表示条件に応じた通知、recipe通知、tutorial toastをキュー/表示し、フレーム更新とHUD表示に接続する。now-playing toastはこの担当実装で確認できない。 | 一部実装 | 未検証<br>静的照合 |
| [hud-09](#hud-09)<br>HUD › spectator menu › command hotbarと選択項目名 | spectator中、開いたcommand hotbarをfade/slide付きで表示し、選択項目名またはcategory promptをバー上に表示する。 | spectator_menu builderはalphaに応じたhotbar/selection/icon/番号と項目名/promptを描画し、in_gameからspectator時に呼ばれる。機能メソッドはあるが、本調査ではkey/mouse入力経路全体の互換確認はしていない。 | 一部実装 | 未検証<br>静的照合 |
| [hud-10](#hud-10)<br>HUD › waypoint locator › 方角/距離ドット表示 | 追跡中waypointを距離順に処理し、自分自身を除いて視野内方角にlocator barのdot/距離style/pitch markerを表示する。 | WaypointMapはTrack/Untrack/同variant Updateを反映し、カメラに対する角度・距離・pitchを使ってdotを抽出する。in_gameが抽出dotをHUD contextual barへ渡す。 | 実装あり | 未検証<br>静的照合 |

### 条件・残件・根拠

<a id="hud-01"></a>

<details>
<summary>hud-01 — HUD › 体力・空腹・空気 › サバイバル状態表示</summary>

- 比較環境：クライアント共通
- 対象条件：サバイバルで通常時・水中・空気減少時、および体力/食料/防具変化を確認する。
- 残件 / 比較すべき点：公式と同じゲーム状態で各アイコン数、吸収心臓、泡の点滅/破裂、騎乗時の食料置換と位置を比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/Hud.java:761](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/Hud.java#L761) — extractPlayerHealth：体力行を算出し、空気泡処理へ進む公式HUD経路。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/Hud.java:899](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/Hud.java#L899) — extractAirBubbles：水中または空気低下時の空気泡条件と泡数計算。
  - Rust：[Client/pomme-client/src/ui/hud.rs:626](../../pomme-client/src/ui/hud.rs#L626) — build_hud：サバイバルの体力・空腹・防具、車両心臓およびHUD各要素を構築。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:4878](../../pomme-client/src/app/phases/in_game.rs#L4878) — build_hud呼出し：ゲーム描画フレームからHUDビルダーを呼ぶ。

</details>

<a id="hud-02"></a>

<details>
<summary>hud-02 — HUD › hotbar › 選択枠と最大9スロットのアイテム表示</summary>

- 比較環境：クライアント共通
- 対象条件：非spectatorでhotbarの空枠・アイテム・複数個スタック・選択位置を表示する。
- 残件 / 比較すべき点：GUIスケールと複数個数、選択位置、クールダウン表示を公式と同じ条件で比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/Hud.java:535](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/Hud.java#L535) — extractHotbarAndDecorations：spectatorと通常のアイテムhotbarを切り替える公式経路。
  - Rust：[Client/pomme-client/src/ui/hud.rs:626](../../pomme-client/src/ui/hud.rs#L626) — build_hud：非spectator時にHotbar/HotbarSelectionおよび最大9スロットのアイテム描画を構築。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:4878](../../pomme-client/src/app/phases/in_game.rs#L4878) — build_hud呼出し：実ゲーム描画からhotbarを含むHUDを呼び出す。

</details>

<a id="hud-03"></a>

<details>
<summary>hud-03 — HUD › 経験値 › 経験値バーとレベル数</summary>

- 比較環境：クライアント共通
- 対象条件：経験値バーが選択され、経験値進捗とレベルが0/正のケース。
- 残件 / 比較すべき点：経験値バーの選択優先度、進捗端点、レベル表示位置/色を公式と比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/Hud.java:547](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/Hud.java#L547) — extractHotbarAndDecorations：選択中のcontextual barを更新し、経験値レベルを描画する公式経路。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/contextualbar/ContextualBar.java:35](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/contextualbar/ContextualBar.java#L35) — extractExperienceLevel：経験値レベルの公式テキスト配置。
  - Rust：[Client/pomme-client/src/ui/hud.rs:931](../../pomme-client/src/ui/hud.rs#L931) — ContextualBarKind::Experience：経験値バー背景と進捗フィルを選択し、後続でレベル数を描画。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:4878](../../pomme-client/src/app/phases/in_game.rs#L4878) — build_hud呼出し：経験値情報を含む状態をHUD描画へ渡す。

</details>

<a id="hud-04"></a>

<details>
<summary>hud-04 — HUD › bossbar › 進捗・色・名前</summary>

- 比較環境：公式サーバー接続
- 対象条件：サーバーがbossbar追加・進捗/名前/スタイル更新・削除を送る。
- 残件 / 比較すべき点：描画する複数バーの順序/配置/補間を比較し、darken-screen・boss music・world fogの公式効果との差を確認する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/components/BossHealthOverlay.java:60](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/components/BossHealthOverlay.java#L60) — extractRenderState：boss barと名前を表示する。
  - Rust：[Client/pomme-client/src/ui/boss_bar.rs:79](../../pomme-client/src/ui/boss_bar.rs#L79) — BossBarState::apply：eventの追加・削除・進捗・名前・スタイル・プロパティ更新を適用。
  - Rust：[Client/pomme-client/src/ui/hud.rs:1133](../../pomme-client/src/ui/hud.rs#L1133) — build_boss_bars：保持されたbossbarを描画。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:3830](../../pomme-client/src/app/phases/in_game.rs#L3830) — boss darkening tick update：darken-screenがlightmap darkeningへ接続。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:3494](../../pomme-client/src/app/phases/in_game.rs#L3494) — evaluate_lightmap_attributes：create-world-fogを環境属性評価へ渡す。

</details>

<a id="hud-05"></a>

<details>
<summary>hud-05 — HUD › title/subtitle › フェード付き中央表示</summary>

- 比較環境：公式サーバー接続
- 対象条件：title/subtitle/timing packetを受け、フェード各区間を通過する。
- 残件 / 比較すべき点：ゼロ/負の時間指定、途中の時間更新、部分tickを含めalpha曲線と位置を比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/Hud.java:358](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/Hud.java#L358) — extractTitle：公式のタイトル有効条件とfade-in/fade-out alpha計算。
  - Rust：[Client/pomme-client/src/ui/title.rs:37](../../pomme-client/src/ui/title.rs#L37) — TitleState::set_title：title設定で合計表示時間のカウントダウンを始める。
  - Rust：[Client/pomme-client/src/ui/title.rs:84](../../pomme-client/src/ui/title.rs#L84) — TitleState::build：有効時にalphaを計算しtitle/subtitleを描画。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:5916](../../pomme-client/src/app/phases/in_game.rs#L5916) — title.build呼出し：ゲームHUD描画列でtitleを描く。

</details>

<a id="hud-06"></a>

<details>
<summary>hud-06 — HUD › tab一覧 › プレイヤー、header/footer、ping、LIST score</summary>

- 比較環境：公式サーバー接続
- 対象条件：tabキーを要求し、listed player数、header/footer、LIST objective、spectatorを変える。
- 残件 / 比較すべき点：列折返し、skin/hat、score/heartの幅と点滅、tab表示条件を公式と同条件で比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/components/PlayerTabOverlay.java:96](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/components/PlayerTabOverlay.java#L96) — extractRenderState：プレイヤー一覧のslot幅/表示情報を構成する公式経路。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/components/PlayerTabOverlay.java:207](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/components/PlayerTabOverlay.java#L207) — extractRenderState：プレイヤー顔、spectator名装飾、LIST score、ping描画。
  - Rust：[Client/pomme-client/src/ui/player_tab.rs:84](../../pomme-client/src/ui/player_tab.rs#L84) — build_player_tab_overlay：listedプレイヤーの複数列一覧、header/footer、顔、score/heart、pingを描画。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:5027](../../pomme-client/src/app/phases/in_game.rs#L5027) — build_player_tab_overlay呼出し：表示条件成立時に一覧overlayを構築。

</details>

<a id="hud-07"></a>

<details>
<summary>hud-07 — HUD › scoreboard › sidebar objectiveと最大15行</summary>

- 比較環境：公式サーバー接続
- 対象条件：サーバーがsidebar objective、entry score、team sidebar slotを設定する。
- 残件 / 比較すべき点：objective/team切替、score tie-break、非表示entry、書式と描画位置を公式と比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/Hud.java:671](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/Hud.java#L671) — displayScoreboardSidebar：公式sidebarのscore並べ替え、非表示entry除外、15行制限。
  - Rust：[Client/pomme-client/src/ui/hud.rs:1200](../../pomme-client/src/ui/hud.rs#L1200) — build_scoreboard：表示objectiveを選び、entry並べ替え/15行制限/書式/背景を処理。
  - Rust：[Client/pomme-client/src/app/core.rs:5018](../../pomme-client/src/app/core.rs#L5018) — ScoreboardObjectiveイベント：サーバーobjective更新をScoreboard状態へ反映。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:4878](../../pomme-client/src/app/phases/in_game.rs#L4878) — build_hud呼出し：HUD中でscoreboard builderを呼ぶ。

</details>

<a id="hud-08"></a>

<details>
<summary>hud-08 — HUD › toast › advancement/recipe通知のキュー表示</summary>

- 比較環境：公式サーバー接続
- 対象条件：advancement/recipe通知を受け、HUD表示の有無とtoast slot占有を確認する。
- 残件 / 比較すべき点：同時通知、5 slot待ち行列、時間/slide曲線を比較する。now-playing toastの実装・設定連携の有無も確認対象。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/components/toasts/ToastManager.java:38](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/components/toasts/ToastManager.java#L38) — ToastManager.update：toastの更新、slot解放と表示切替音の公式処理。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/components/toasts/ToastManager.java:80](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/components/toasts/ToastManager.java#L80) — ToastManager.extractRenderState：表示中toastをHUD状態へ描画し、now-playing通知も条件付きで描く。
  - Rust：[Client/pomme-client/src/ui/toast.rs:386](../../pomme-client/src/ui/toast.rs#L386) — ToastState::apply_advancements：advancement更新からtoastキューを更新。
  - Rust：[Client/pomme-client/src/ui/toast.rs:540](../../pomme-client/src/ui/toast.rs#L540) — ToastState::build：表示中toastを描画。in_game.rsの5984行から呼ばれる。
  - Rust：[Client/pomme-client/src/app/core.rs:4973](../../pomme-client/src/app/core.rs#L4973) — AdvancementsUpdateイベント：サーバーadvancement更新をtoast状態に適用。

</details>

<a id="hud-09"></a>

<details>
<summary>hud-09 — HUD › spectator menu › command hotbarと選択項目名</summary>

- 比較環境：公式サーバー接続
- 対象条件：spectatorモードでmenuを開き、空/無効/選択済み項目と5秒後のfadeを確認する。
- 残件 / 比較すべき点：入力イベントから選択・ページ移動・teleport packet送信までの呼出し接続、項目順とfade時間を公式と比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/components/spectator/SpectatorGui.java:44](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/components/spectator/SpectatorGui.java#L44) — extractHotbar：有効なmenuをalphaに応じて下端に表示し、消失時に閉じる。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/components/spectator/SpectatorGui.java:84](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/components/spectator/SpectatorGui.java#L84) — extractAction：選択項目またはcategory promptをhotbar上に表示。
  - Rust：[Client/pomme-client/src/ui/spectator_menu.rs:354](../../pomme-client/src/ui/spectator_menu.rs#L354) — build_spectator_menu：ホットバー、項目、選択番号、action/promptの描画を構築。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:4868](../../pomme-client/src/app/phases/in_game.rs#L4868) — build_spectator_menu呼出し：spectator時にゲームHUD描画から呼ぶ。

</details>

<a id="hud-10"></a>

<details>
<summary>hud-10 — HUD › waypoint locator › 方角/距離ドット表示</summary>

- 比較環境：公式サーバー接続
- 対象条件：server waypointのposition/chunk/azimuth形式、視野角、距離style境界、自身UUID除外を確認する。
- 残件 / 比較すべき点：yaw/pitch投影、境界角、距離sprite、並び順、未対応styleの表示を公式locator barと同条件で比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/contextualbar/LocatorBar.java:40](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/contextualbar/LocatorBar.java#L40) — LocatorBar.extractRenderState：カメラに基づくwaypoint走査からlocator barを描画する公式入口。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/waypoints/ClientWaypointManager.java:20](../../../minecraft-26.2-decompiled/src/net/minecraft/client/waypoints/ClientWaypointManager.java#L20) — updateWaypoint：追跡済みwaypointの更新と一覧走査の公式管理。
  - Rust：[Client/pomme-client/src/world/waypoints.rs:95](../../pomme-client/src/world/waypoints.rs#L95) — WaypointMap::apply：サーバー操作のTrack/Untrack/Updateを反映。
  - Rust：[Client/pomme-client/src/world/waypoints.rs:125](../../pomme-client/src/world/waypoints.rs#L125) — WaypointMap::extract_dots：距離順、自己除外、角度範囲、style indexとpitchを抽出。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:4825](../../pomme-client/src/app/phases/in_game.rs#L4825) — extract_dots呼出し：フレームのカメラ状態からdotを作りHUDへ渡す。
  - Rust：[Client/pomme-client/src/app/core.rs:4508](../../pomme-client/src/app/core.rs#L4508) — WaypointUpdateイベント：ネットワークwaypoint操作をWaypointMapに適用。

</details>


<a id="domain-chat"></a>

## チャット・コマンド

調査範囲：指定されたRustのui/chat.rs、chat_component.rs、net/chat.rs、net/chat_security.rs、net/commands.rs、app/core.rsを読み、in_game.rs・input.rs・net/connection.rsの接続経路も確認しました。公式26.2のChatComponent.java、ChatScreen.java、CommandSuggestions.java、ClientPacketListener.java、ChatListener.java、Screen.javaを照合しました。実行確認はしておらず、実機通信・全チャット装飾形式・全Brigadier argument parserは未調査です。

- 制約：指定どおり読み取り専用の静的照合のみ実施し、cargo/build/test・アプリ起動・公式サーバー接続はしていません。
- 制約：chat delay、key取得失敗、server-enforces-secure-chat、custom click packetのwire上の相互運用は今回実測していません。
- 制約：CommandTreeは未移植Brigadier parserをUnknownとして扱う箇所があり、候補・usage・安全判定の完全一致は保証できません。
- 制約：Rich textの全NBT/JSON component variant、show_item/show_entity tooltip、翻訳資産の表示は網羅していません。

| ID / 機能 | 公式の挙動 | Rustの現状・差分 | 実装 | 比較 / 検証 |
|---|---|---|---|---|
| [chat-01](#chat-01)<br>チャット入力 › 送信・履歴・下書き | Enterで空でない入力を正規化し、履歴へ重複連続登録を避けて記録した後、/始まりはコマンド、それ以外はチャットとして送信します。上下キーで履歴をたどり、戻ると編集中の下書きを復元します。 | ChatStateがEnter処理・最大100件の履歴・下書きのclose理由別保存を持ち、in_gameのキー入力処理が返された文字列をAppCore経由で接続へ渡します。ChatState.openとChatMethodにより通常/コマンドのprefixも切り替わります。 | 実装あり | 未検証<br>静的照合 |
| [chat-02](#chat-02)<br>チャット表示 › 可視性・折返し・スクロール中の新着 | チャット行を幅に合わせて折り返し最大100行分表示し、可視性設定に応じてプレイヤー/サーバー系メッセージを隠します。スクロール中に新着が来ると閲覧位置を保ち、新着表示状態を更新します。 | RustはPlayer/SystemServer/SystemClient別に可視性を判定し、最大100メッセージを遅延wrapして描画、スクロール中の新着で位置を補正します。描画経路はChatState.build、メッセージ受信はcoreのNetworkEvent::ChatMessageから接続されています。 | 実装あり | 未検証<br>静的照合 |
| [chat-03](#chat-03)<br>チャット表示 › 署名付き削除メッセージ | 署名で対象メッセージを特定し、追加から60ゲームtick未満なら削除を遅らせ、経過後に削除マーカーへ置換します。 | Rustは署名一致行を削除マーカーに置換し、新しすぎる場合は3秒後のwall-clock時刻まで保留します。公式はHUD tickで60 tickを判定するため、ゲーム停止・tick停滞時にタイミングが異なる既知差分です。 | 一部実装 | 差分あり<br>静的照合 |
| [chat-04](#chat-04)<br>コマンド補完 › 入力中のBrigadier補完とサーバー候補 | 入力のparse位置に対してBrigadierの候補を要求し、ローカルliteral候補とargument parser側の候補を統合します。サーバー応答のrequest idが一致した候補だけを該当rangeへ適用します。 | Rustは編集ごとにコマンドツリーをparseし、literalはローカル提示、argumentがあればServerboundCommandSuggestionを送信します。応答では最新idを確認し、UTF-16 rangeをbyte offsetへ変換して入力候補へ適用します。ただしRustはargument候補をparser固有のローカルsuggestionではなくサーバーへ一律委譲すると明記しています。 | 一部実装 | 差分あり<br>静的照合 |
| [chat-05](#chat-05)<br>コマンド補完 › 通常チャットのcustom tab候補 | 通常メッセージ入力時はサーバー配布のcustom tab候補を、caret直前の単語prefixに基づいて候補表示し、候補適用時に対象単語を置換します。 | Rustはcustom completionsのAdd/Remove/Setを保持し、case-insensitive prefixで一致候補を作り、現在語開始から入力末尾までを置換rangeにします。NetworkEventによる受信・ChatStateへの配送については関連event variantとcore handlerを追跡しました。 | 実装あり | 未検証<br>静的照合 |
| [chat-06](#chat-06)<br>リッチテキスト › 受信componentの装飾・chat type・filter mask | system/player/disguised chat componentをchat type decorationで整形し、filter maskの全隠し・部分隠しを反映します。overlay指定のsystem chatは履歴でなくaction barへ送ります。 | Rustはazaleaのcomponent decodeより先にraw chat packetをdecodeし、chat registry/direct decoration、overlay、player chatの部分filter（UTF-16位置を#化）を処理し、styleを保持するspanへ変換します。UIへ渡すNetworkEventはcoreでChatStateまたはaction barに接続されています。 | 実装あり | 未検証<br>静的照合 |
| [chat-07](#chat-07)<br>セキュアチャット › profile key/sessionとplayer chat署名検証 | Online-modeのprofile key/sessionを使い、content・timestamp・salt・last-seen署名群等で通常chatを署名します。受信player chatはglobal index、signature cache、profile session/署名を検証し、無効/欠落時はエラー扱いにします。 | Rustは証明書を取得しsession updateを送信、sessionがある同一accountのchatをRSA署名します。受信はglobal index/cacheを検証し、TabListのplayer sessionで署名と連鎖を検証してからcoreが署名状態・only-secure表示をChatStateに伝えます。 | 実装あり | 未検証<br>静的照合 |
| [chat-08](#chat-08)<br>セキュアチャット › only show secureと処理済みacknowledgement | Only Show Secure Chat有効時は未信頼メッセージを表示しません（unsigned decorationも隠す）。表示/非表示の結果をlast-seen trackerへ記録し、送信時またはoffset閾値でacknowledgementを送ります。 | Rustはonly_secure時に署名本文から描画用spansを選び、未署名/modified判定を経て不安全行を隠す設計です。ChatStateが署名付き行のprocessed/shown markを出し、ChatSenderのring trackerがlast-seen updateとack packetを作ります。 | 実装あり | 未検証<br>静的照合 |
| [chat-09](#chat-09)<br>リッチテキスト › styled span hover tooltip | componentの継承styleを文字範囲に適用し、hover eventが付いた範囲上でshow_text等のtooltipを表示します。 | RustはcomponentをResolvedStyle付きTextSpanへ分割し、描画したspanごとにhit regionを記録してstyle_atでhover styleを引き、HoverEventに応じtooltipを構築します。Item hoverはJSON shapeからの再構成に制約があり、entity hoverはadvanced tooltip設定が必要です。 | 一部実装 | 差分あり<br>静的照合 |
| [chat-10](#chat-10)<br>リッチテキスト › click eventとuntrusted URL確認 | クリックしたtext styleのclick eventを実行します。Open URLは未信頼URIを検証し確認画面経由、RunCommandはコマンド検証/必要な確認、SuggestCommandは入力へ挿入、clipboard/custom/dialog actionは各固有処理へ分岐します。 | Rustはspan hit regionからClickEventを取得しRunCommand/SuggestCommand/CopyToClipboard/Custom/ShowDialogを処理します。OpenUrlはhttp(s) URIを検証しlinks設定に従って確認modalまたはopen actionへ渡し、通常クリックとshift insertionを分けています。 | 実装あり | 未検証<br>静的照合 |
| [chat-11](#chat-11)<br>コマンド送信 › message argument署名とクリック起動の安全確認 | 通常コマンドはmessage型のsignable argumentが無ければunsigned command packet、あれば共通timestamp/salt/last-seenで各argumentを署名して送ります。クリック等のunattended commandはparse・署名要否・権限を検査し、問題に応じて確認します。 | RustはCommandTree::signable_argumentsでMessage parser範囲を抽出し、ChatSender::encode_inputからunsigned/signed packetを選択します。RunCommand clickはverify_unattendedの結果に応じ即時実行またはsignature/permission/parse confirmationへ送られます。ローカルparser未実装branchは安全側に不確実扱いです。 | 実装あり | 未検証<br>静的照合 |

### 条件・残件・根拠

<a id="chat-01"></a>

<details>
<summary>chat-01 — チャット入力 › 送信・履歴・下書き</summary>

- 比較環境：クライアント共通
- 対象条件：チャットを開き、文字を入力してEnter送信後に履歴を上下する。
- 残件 / 比較すべき点：正規化のUnicode空白境界、履歴保存の永続性、および割り込み・Escape・設定画面復帰時の下書き挙動を同条件で比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/ChatScreen.java:247](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/ChatScreen.java#L247) — moveInHistory：上下履歴と編集中historyBufferの復元。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/ChatScreen.java:312](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/ChatScreen.java#L312) — handleChatInput：正規化後、/有無でsendCommand/sendChatを分岐。
  - Rust：[Client/pomme-client/src/ui/chat.rs:1118](../../pomme-client/src/ui/chat.rs#L1118) — move_in_history：履歴選択とhistory_buffer復元。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:4511](../../pomme-client/src/app/phases/in_game.rs#L4511) — handle_key_input call site：チャット入力を処理し送信メッセージをcoreへ渡す。
  - Rust：[Client/pomme-client/src/app/core.rs:3544](../../pomme-client/src/app/core.rs#L3544) — send_chat_message：チャット送信をpacket_txへ接続。

</details>

<a id="chat-02"></a>

<details>
<summary>chat-02 — チャット表示 › 可視性・折返し・スクロール中の新着</summary>

- 比較環境：クライアント共通
- 対象条件：複数行メッセージ受信後に古い行へスクロールし、Full/System/Hiddenを切り替える。
- 残件 / 比較すべき点：幅・scale・line spacing時の折返しとscrollbar位置、ならびにRust設定変更時に既存履歴が公式同様再フィルタされるかを比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/components/ChatComponent.java:278](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/components/ChatComponent.java#L278) — addMessageToDisplayQueue：折返し表示キュー上限とスクロール中の新着アンカー。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/components/ChatComponent.java:382](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/components/ChatComponent.java#L382) — scrollChat：スクロール上限と先頭到達時のnewMessageSinceScroll解除。
  - Rust：[Client/pomme-client/src/ui/chat.rs:780](../../pomme-client/src/ui/chat.rs#L780) — source_visible：3種類のメッセージソースに対する可視性条件。
  - Rust：[Client/pomme-client/src/ui/chat.rs:799](../../pomme-client/src/ui/chat.rs#L799) — accept_pending_message：メッセージ保管上限とスクロール中の新着位置補正。
  - Rust：[Client/pomme-client/src/app/core.rs:4881](../../pomme-client/src/app/core.rs#L4881) — NetworkEvent::ChatMessage：ネットワーク受信イベントをChatStateへ配送。

</details>

<a id="chat-03"></a>

<details>
<summary>chat-03 — チャット表示 › 署名付き削除メッセージ</summary>

- 比較環境：クライアント共通
- 対象条件：受信直後の署名付き行を削除し、ゲームtickを停止または遅延させて削除時刻を見る。
- 残件 / 比較すべき点：ポーズ/低TPS中にwall timeとHUD tickのどちらを基準に削除するか、期限境界で実機比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/components/ChatComponent.java:319](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/components/ChatComponent.java#L319) — deleteMessageOrDelay：addedTime+60のHUD tickまで削除を遅延。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java:1067](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java#L1067) — handleDeleteChat：署名を展開して削除処理へ接続。
  - Rust：[Client/pomme-client/src/ui/chat.rs:901](../../pomme-client/src/ui/chat.rs#L901) — delete_message_or_delay：受信Instant+3秒を削除期限にする。
  - Rust：[Client/pomme-client/src/app/core.rs:4939](../../pomme-client/src/app/core.rs#L4939) — NetworkEvent::DeleteChatMessage：削除packetイベントからChatState削除へ接続。

</details>

<a id="chat-04"></a>

<details>
<summary>chat-04 — コマンド補完 › 入力中のBrigadier補完とサーバー候補</summary>

- 比較環境：公式サーバー接続
- 対象条件：コマンドliteral補完とargument補完（名前・enum等）を含む入力をTabおよび候補クリックで適用する。
- 残件 / 比較すべき点：各parserのローカル候補/ask-server指定、空応答時のliteral fallback、Unicodeを含むrangeおよび候補の並び順を公式と比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/components/CommandSuggestions.java:210](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/components/CommandSuggestions.java#L210) — updateCommandInfo：CommandDispatcher.getCompletionSuggestionsをparse位置で実行。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java:1651](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java#L1651) — handleCommandSuggestions：server suggestions応答をsuggestion providerへ配送。
  - Rust：[Client/pomme-client/src/net/commands.rs:497](../../pomme-client/src/net/commands.rs#L497) — CommandTree::completions：literal候補をローカル生成しargument候補をserver-required扱い。
  - Rust：[Client/pomme-client/src/ui/chat.rs:1206](../../pomme-client/src/ui/chat.rs#L1206) — update_command_info：編集時parse、request発行およびsuggestion state生成。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:4537](../../pomme-client/src/app/phases/in_game.rs#L4537) — take_suggestion_request call site：保留requestをServerboundCommandSuggestion packetとして送る。
  - Rust：[Client/pomme-client/src/ui/chat.rs:1433](../../pomme-client/src/ui/chat.rs#L1433) — apply_server_suggestions：id一致とUTF-16 offset変換を経て候補を適用。

</details>

<a id="chat-05"></a>

<details>
<summary>chat-05 — コマンド補完 › 通常チャットのcustom tab候補</summary>

- 比較環境：公式サーバー接続
- 対象条件：サーバーがcustom chat completionを設定した後、通常チャットでprefixを入力してTabする。
- 残件 / 比較すべき点：case-insensitive照合、caretが入力途中にある場合の後続文字保持、Add/Remove/Set更新時の表示条件を比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/components/CommandSuggestions.java:210](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/components/CommandSuggestions.java#L210) — updateCommandInfo：非コマンド入力ではcustomTabSuggestionsをprefixで候補化。
  - Rust：[Client/pomme-client/src/ui/chat.rs:1267](../../pomme-client/src/ui/chat.rs#L1267) — update_custom_completions：Add/Remove/Setと開いている入力候補の更新。
  - Rust：[Client/pomme-client/src/ui/chat.rs:1291](../../pomme-client/src/ui/chat.rs#L1291) — update_custom_suggestions：caret直前tokenとのcase-insensitive prefix一致と置換range。
  - Rust：[Client/pomme-client/src/app/core.rs:5089](../../pomme-client/src/app/core.rs#L5089) — NetworkEvent::CustomChatCompletions：custom completion packetのAdd/Remove/SetをChatStateへ配送。

</details>

<a id="chat-06"></a>

<details>
<summary>chat-06 — リッチテキスト › 受信componentの装飾・chat type・filter mask</summary>

- 比較環境：公式サーバー接続
- 対象条件：翻訳parameter/style付きchat type、hover/clickを持つcomponent、overlayおよび部分filter maskを含む受信packetを表示する。
- 残件 / 比較すべき点：複合component形式・翻訳引数・部分filterのUTF-16 surrogate境界を公式と比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java:1014](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java#L1014) — handleSystemChat：overlayをaction bar、それ以外をsystem chat listenerへ分岐。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java:1023](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java#L1023) — handlePlayerChat：player chat packetを検証後ChatListenerへ配送。
  - Rust：[Client/pomme-client/src/net/chat.rs:277](../../pomme-client/src/net/chat.rs#L277) — handle_raw_chat_packet：raw packetをchat専用decoderで処理。
  - Rust：[Client/pomme-client/src/net/chat.rs:692](../../pomme-client/src/net/chat.rs#L692) — filtered_component：部分filter bitをUTF-16単位で#文字へ置換。
  - Rust：[Client/pomme-client/src/app/core.rs:4881](../../pomme-client/src/app/core.rs#L4881) — NetworkEvent::ChatMessage：受信spanと署名状態をUIへ接続。

</details>

<a id="chat-07"></a>

<details>
<summary>chat-07 — セキュアチャット › profile key/sessionとplayer chat署名検証</summary>

- 比較環境：公式サーバー接続
- 対象条件：Online-modeでkey取得可能な同一profileの通常chatを送受信し、無効署名・session期限切れ・index不整合も確認する。
- 残件 / 比較すべき点：Mojang証明書更新/期限猶予・署名chain/repeat境界・サーバーenforce設定の全分岐を実接続で比較する。今回、HTTP資格情報やサーバー側検証結果は未確認。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java:1023](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java#L1023) — handlePlayerChat：global index、cache unpack、player validator経由の受信。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java:2525](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java#L2525) — sendChat：timestamp/salt/last-seenとencoder署名をpacketへ格納。
  - Rust：[Client/pomme-client/src/net/chat_security.rs:478](../../pomme-client/src/net/chat_security.rs#L478) — ChatSender::encode_input：通常chat内容の長さ検査・last-seen更新・session署名。
  - Rust：[Client/pomme-client/src/net/chat_security.rs:431](../../pomme-client/src/net/chat_security.rs#L431) — key_pair_ready：session UUIDを発行しchat_session_update frameを生成。
  - Rust：[Client/pomme-client/src/player/tab_list.rs:66](../../pomme-client/src/player/tab_list.rs#L66) — validate_chat_message：期限猶予、署名検証、署名chainをチェック。
  - Rust：[Client/pomme-client/src/app/core.rs:4881](../../pomme-client/src/app/core.rs#L4881) — NetworkEvent::ChatMessage：player validation後のsecure spans/tagをChatStateへ渡す。

</details>

<a id="chat-08"></a>

<details>
<summary>chat-08 — セキュアチャット › only show secureと処理済みacknowledgement</summary>

- 比較環境：公式サーバー接続
- 対象条件：only secureのON/OFFでunsigned・modified・fully filtered・署名なしmessageを受信し、送信前後のackを観察する。
- 残件 / 比較すべき点：不安全判定、fully filteredと非表示のack bit、deletion後のpending除去、最大20件ring順序/standalone ack閾値を公式と比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/chat/ChatListener.java:95](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/chat/ChatListener.java#L95) — handlePlayerChatMessage：onlyShowSecureに応じunsigned contentを除いて描画。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java:2512](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java#L2512) — markMessageAsProcessed：shown状態をlast-seenへ登録しoffset&gt;64でack。
  - Rust：[Client/pomme-client/src/app/core.rs:4881](../../pomme-client/src/app/core.rs#L4881) — NetworkEvent::ChatMessage：secure spans選択、validation後tagと可視判定への接続。
  - Rust：[Client/pomme-client/src/ui/chat.rs:718](../../pomme-client/src/ui/chat.rs#L718) — push_message_with_source：only_secureとNotSecure tagに基づき表示抑制、処理markを追跡。
  - Rust：[Client/pomme-client/src/net/chat_security.rs:458](../../pomme-client/src/net/chat_security.rs#L458) — ChatSender::mark：processed/deleted markをtrackerへ反映しackを生成。

</details>

<a id="chat-09"></a>

<details>
<summary>chat-09 — リッチテキスト › styled span hover tooltip</summary>

- 比較環境：クライアント共通
- 対象条件：show_text、show_item、show_entityを持つcomponentの範囲へhoverする。
- 残件 / 比較すべき点：show_textの継承・改行・tooltip幅と、item/entity tooltipのpayload・advanced設定を公式で比較する。show_itemはRustコメント上もpayload型情報等を失う既知制約。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/components/ChatComponent.java:109](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/components/ChatComponent.java#L109) — extractRenderState：chat foreground/background描画とhover可能なtext経路。
  - Rust：[Client/pomme-client/src/ui/text.rs:152](../../pomme-client/src/ui/text.rs#L152) — format_component_spans：componentをResolvedStyle付きspanへ展開。
  - Rust：[Client/pomme-client/src/ui/chat.rs:237](../../pomme-client/src/ui/chat.rs#L237) — style_at：描画spanのhit regionからcursor位置styleを特定。
  - Rust：[Client/pomme-client/src/ui/chat.rs:2110](../../pomme-client/src/ui/chat.rs#L2110) — ChatState::build hover handling：範囲上のhover eventをtooltip builderへ接続。
  - Rust：[Client/pomme-client/src/ui/chat.rs:2504](../../pomme-client/src/ui/chat.rs#L2504) — push_hover_tooltip：Text/Item/Entityごとにtooltip linesを生成。

</details>

<a id="chat-10"></a>

<details>
<summary>chat-10 — リッチテキスト › click eventとuntrusted URL確認</summary>

- 比較環境：公式サーバー接続
- 対象条件：未信頼URL・run_command・suggest_command・copy_to_clipboard・custom clickを含むchat componentをクリックする。
- 残件 / 比較すべき点：URL scheme/host/character検証、確認modalのYes/Copy/Noとscreen復帰、コマンド署名/権限/parse-error confirmationを公式と比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/Screen.java:253](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/Screen.java#L253) — defaultHandleGameClickEvent：run command、dialog、custom actionのゲームclick処理。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/Screen.java:283](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/Screen.java#L283) — defaultHandleClickEvent：URL、suggest command、clipboard等の標準click処理。
  - Rust：[Client/pomme-client/src/ui/chat.rs:1648](../../pomme-client/src/ui/chat.rs#L1648) — handle_click：hit styleのeventとshift insertionを処理。
  - Rust：[Client/pomme-client/src/ui/chat.rs:685](../../pomme-client/src/ui/chat.rs#L685) — request_open_url：URL検証、links toggle、prompt有無を処理。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:2355](../../pomme-client/src/app/phases/in_game.rs#L2355) — handle_chat_ui_action：URL open、unattended command、custom packet等の実利用先。

</details>

<a id="chat-11"></a>

<details>
<summary>chat-11 — コマンド送信 › message argument署名とクリック起動の安全確認</summary>

- 比較環境：公式サーバー接続
- 対象条件：literal-only command、message argument command、権限限定command、未知parserを含むrun_command clickを実行する。
- 残件 / 比較すべき点：Brigadier parse結果・redirect・restricted node・署名対象rangeを公式と比較し、未知parserを含むbranchごとの確認画面と実際の送信packetを接続テストする。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java:2533](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java#L2533) — sendCommand：signable argumentsの有無でunsigned/signed commandを分岐。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java:2549](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java#L2549) — sendUnattendedCommand：クリック等からのcommandをverifyCommandで判定しconfirmationを選ぶ。
  - Rust：[Client/pomme-client/src/net/commands.rs:333](../../pomme-client/src/net/commands.rs#L333) — CommandTree::signable_arguments：Message型argument名と値rangeをparse treeから抽出。
  - Rust：[Client/pomme-client/src/net/chat_security.rs:478](../../pomme-client/src/net/chat_security.rs#L478) — ChatSender::encode_input：signable argumentなし/ありで異なるcommand encoderへ送る。
  - Rust：[Client/pomme-client/src/net/commands.rs:298](../../pomme-client/src/net/commands.rs#L298) — verify_unattended：parse validity・署名要否・restrictionを分類。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:2376](../../pomme-client/src/app/phases/in_game.rs#L2376) — handle_chat_ui_action RunCommand：click commandをunattended verification/confirmation処理に接続。

</details>


<a id="domain-menus"></a>

## メニュー・設定

調査範囲：タイトル画面、設定メニュー（言語・映像倍率・操作・アクセシビリティ）、ポーズ/死後画面、サーバー一覧とサーバー発のダイアログについて、Rustの画面構築から入力・状態遷移/利用側までを静的に照合。world管理、シングルプレイの実動作、launcher認証は対象外で、見た目の一致も判定していない。

- 制約：Rust・公式とも実機やビルドで検証しておらず、結論は静的コード照合に限定。
- 制約：UIの配置/色/アニメーション等、見た目の一致は判定していない。
- 制約：world管理、シングルプレイの実動作、launcher認証は別担当。
- 制約：公式マルチプレイ画面のLAN発見/警告/サーバー状態の詳細や、アクセシビリティ全設定、サーバーダイアログの全入力・本文形式は網羅確認していない。

| ID / 機能 | 公式の挙動 | Rustの現状・差分 | 実装 | 比較 / 検証 |
|---|---|---|---|---|
| [menus-01](#menus-01)<br>タイトル › メイン画面 › マルチプレイの一覧画面へ移動 | 公式はマルチプレイ利用可の場合、警告省略設定が有効ならJoinMultiplayerScreenへ進み、無効ならSafetyScreenへ進む。この行では警告省略が有効な条件の一覧への遷移を扱う。 | RustはタイトルのMultiplayerボタンのクリック/選択確定からScreen::ServerListへ遷移し、一覧の選択状態をリセットする。シングルプレイのライフサイクルは別分野で扱う。 | 実装あり | 未検証<br>静的照合 |
| [menus-02](#menus-02)<br>タイトル › 設定ショートカット › 言語/アクセシビリティ | 公式タイトル画面から言語選択とアクセシビリティ設定を直接開ける。 | Rustタイトル画面の言語・アクセシビリティアイコンはそれぞれ対応画面へ遷移し、設定側は英語/日本語切替および一部アクセシビリティ項目を持つ。 | 実装あり | 未検証<br>静的照合 |
| [menus-03](#menus-03)<br>設定 › 言語 › 利用可能な言語を検索・選択 | 公式の言語画面は利用可能な言語一覧を表示し、検索語で絞り込み、選択した言語を適用して戻る。 | Rustの選択肢はEnglish (US)と日本語 (日本)の2つだけ。選択時にlocaleを変更して設定を保存するが、公式の言語一覧検索や他言語選択は提供しない。 | 一部実装 | 差分あり<br>静的照合 |
| [menus-04](#menus-04)<br>設定 › 映像 › GUI倍率を切り替える | 公式映像設定にはGUI Scaleオプションがあり、GUIの表示倍率を変更できる。 | RustはGUI Scaleの値を表示し、クリックでAuto相当の0から画面サイズに基づく最大値まで循環して保存する。画面構築でも同じ倍率設定を利用する。 | 実装あり | 未検証<br>静的照合 |
| [menus-05](#menus-05)<br>設定 › キー設定 › キー割当の変更・リセット | 公式のキー設定画面では個別割当を選択してキーを再割当でき、各割当/全割当のリセットも可能。 | Rustにはキー設定への項目表示があるが無効化されており、OptionsControls画面から割当変更へ進めない。InputStateは一部Actionの既定キーを定義するにとどまる。 | 未実装 | 差分あり<br>静的照合 |
| [menus-06](#menus-06)<br>設定 › アクセシビリティ › 字幕と視点の揺れ | 公式アクセシビリティ設定には字幕表示と視点の揺れの切替がある。 | Rustは字幕/視点の揺れを切り替えて設定保存する一方、ナレーター、ハイコントラスト等の複数項目は固定表示か無効化されている。 | 一部実装 | 未検証<br>静的照合 |
| [menus-07](#menus-07)<br>マルチプレイ › サーバー一覧 › 選択/追加/編集/削除/更新 | 公式の接続前画面は保存済みサーバーを一覧表示し、選択・直接接続・追加・編集・削除確認・更新・戻るの操作を提供する。 | Rustはservers.jsonから一覧を読み書きし、選択/ダブルクリック接続、直接接続、追加、編集、削除確認、更新、戻るを提供する。 | 実装あり | 未検証<br>静的照合 |
| [menus-08](#menus-08)<br>マルチプレイ › サーバー行 › 状態問い合わせと接続 | 公式一覧は登録サーバーをpingして状態を表示し、行選択後の接続操作から接続を開始できる。 | Rustは画面内の可視行をpingし、MOTD/人数/遅延/バージョン等を状態へ格納する。行のダブルクリック、アイコン接続操作、または選択後のJoin Serverが接続アクションを発行する。 | 実装あり | 未検証<br>静的照合 |
| [menus-09](#menus-09)<br>ポーズ › メニュー › 設定/ゲーム復帰/切断 | 公式ポーズ画面はゲーム復帰、設定、切断を提供し、単一プレイ時の切断ボタン名は保存してタイトルへ戻る意味を示す。 | Rustは復帰・設定・切断の操作を持つが、Advancements/Statistics/フィードバック等は無効化され、独自のBenchmark項目がある。singleplayer時の切断表示もありますが、その保存・終了意味論は今回の対象外。 | 一部実装 | 差分あり<br>静的照合 |
| [menus-10](#menus-10)<br>死後 › リスポーン/タイトル移動 › 20 tick遅延と確認 | 公式は通常時にRespawn、死亡後タイトル移動に確認画面を提供し、hardcore時はSpectate Worldと確認なしのタイトル移動を使う。ボタンは20 tick後に有効化される。 | Rustは通常/hardcoreで同様のラベルと確認分岐を構築し、20 tick後に有効化する。Respawnアクションはin-game処理からrespawn送信へ接続される。 | 実装あり | 未検証<br>静的照合 |
| [menus-11](#menus-11)<br>サーバーダイアログ › ボディ/入力/アクション/閉じる | 公式サーバーダイアログは本文と入力欄、種類別ボタン、Escape可否、アクション後の閉じる/待機等を扱い、アクションはURL/コマンド/別ダイアログ等へ接続される。 | Rustはサーバー/inline dialogを解決・解析してダイアログ状態を生成し、種類別操作や入力値、Escape/after_actionを処理する。コード上にnotice/confirmation/multi_action/dialog_list/server_links等の分岐があるが、公式との網羅的一致は未確認。 | 一部実装 | 未検証<br>静的照合 |

### 条件・残件・根拠

<a id="menus-01"></a>

<details>
<summary>menus-01 — タイトル › メイン画面 › マルチプレイの一覧画面へ移動</summary>

- 比較環境：接続前
- 対象条件：マルチプレイ利用可、公式の警告省略設定が有効な状態で、タイトル画面からマルチプレイを選択する。
- 残件 / 比較すべき点：一覧への遷移を実操作で比較する。警告画面とアカウント由来の無効化条件は、この行の一致判定には含めない。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/TitleScreen.java:184](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/TitleScreen.java#L184) — multiplayer button action：multiplayer利用可判定とskipMultiplayerWarningによる画面選択。
  - Rust：[Client/pomme-client/src/ui/menu/main_screen.rs:287](../../pomme-client/src/ui/menu/main_screen.rs#L287) — MainMenu::build_main_pomme：ボタンid 1からServerListへ遷移する。

</details>

<a id="menus-02"></a>

<details>
<summary>menus-02 — タイトル › 設定ショートカット › 言語/アクセシビリティ</summary>

- 比較環境：接続前
- 対象条件：タイトル画面から各アイコンを選択する
- 残件 / 比較すべき点：ショートカットの経路は確認済み。一致未確認。Rustが提供する言語は英語/日本語の2つに限定される（menus-03）。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/TitleScreen.java:132](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/TitleScreen.java#L132) — TitleScreen.init：言語選択ボタン。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/TitleScreen.java:138](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/TitleScreen.java#L138) — TitleScreen.init：アクセシビリティ設定ボタン。
  - Rust：[Client/pomme-client/src/ui/menu/main_screen.rs:378](../../pomme-client/src/ui/menu/main_screen.rs#L378) — MainMenu::build_main_pomme：言語アイコンとクリック処理。
  - Rust：[Client/pomme-client/src/ui/menu/main_screen.rs:385](../../pomme-client/src/ui/menu/main_screen.rs#L385) — MainMenu::build_main_pomme：アクセシビリティ設定アイコンとクリック処理。

</details>

<a id="menus-03"></a>

<details>
<summary>menus-03 — 設定 › 言語 › 利用可能な言語を検索・選択</summary>

- 比較環境：クライアント共通
- 対象条件：言語画面で日本語または英語を選択し、他言語を探す
- 残件 / 比較すべき点：対応言語数の差はコード上で明確。翻訳適用範囲や言語資産を実機比較していない。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/options/LanguageSelectScreen.java:39](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/options/LanguageSelectScreen.java#L39) — LanguageSelectScreen.addTitle：検索欄を設置し入力で一覧をfilterEntriesへ渡す。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/options/LanguageSelectScreen.java:91](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/options/LanguageSelectScreen.java#L91) — LanguageSelectScreen.onDone：選択された言語をLanguageManagerとOptionsへ適用。
  - Rust：[Client/pomme-client/src/ui/menu/options.rs:471](../../pomme-client/src/ui/menu/options.rs#L471) — MainMenu::build_options_language：英語と日本語のみ選択肢として構築。
  - Rust：[Client/pomme-client/src/ui/menu/options.rs:1373](../../pomme-client/src/ui/menu/options.rs#L1373) — MainMenu::build_options_grid：言語選択時にlocaleを切替・保存。

</details>

<a id="menus-04"></a>

<details>
<summary>menus-04 — 設定 › 映像 › GUI倍率を切り替える</summary>

- 比較環境：クライアント共通
- 対象条件：映像設定でGUI Scaleを複数回切り替える
- 残件 / 比較すべき点：Rust側の変更・保存・UI利用は確認。一致未確認で、公式の倍率値/自動計算と同じかは数値比較が必要。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/options/VideoSettingsScreen.java:69](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/options/VideoSettingsScreen.java#L69) — VideoSettingsScreen.displayOptions：映像設定のdisplayOptionsにoptions.guiScale()を含む。
  - Rust：[Client/pomme-client/src/ui/menu/options.rs:573](../../pomme-client/src/ui/menu/options.rs#L573) — MainMenu::build_options_video：GUI倍率をAutoまたは数値として表示。
  - Rust：[Client/pomme-client/src/ui/menu/options.rs:1388](../../pomme-client/src/ui/menu/options.rs#L1388) — MainMenu::build_options_grid：クリックで上限まで循環して設定保存。
  - Rust：[Client/pomme-client/src/ui/menu/servers.rs:11](../../pomme-client/src/ui/menu/servers.rs#L11) — MainMenu::build_server_list：画面倍率設定が画面レイアウトに利用される例。

</details>

<a id="menus-05"></a>

<details>
<summary>menus-05 — 設定 › キー設定 › キー割当の変更・リセット</summary>

- 比較環境：クライアント共通
- 対象条件：操作設定からキー割当変更またはリセットを試みる
- 残件 / 比較すべき点：再割当UIと割当状態の更新/保存がRustの確認範囲に見当たらず、公式のキー設定機能との差がある。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/options/controls/ControlsScreen.java:35](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/options/controls/ControlsScreen.java#L35) — ControlsScreen.addOptions：KeyBindsScreenを開く操作を提供。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/options/controls/KeyBindsScreen.java:40](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/options/controls/KeyBindsScreen.java#L40) — KeyBindsScreen.addFooter：resetAllでkeyMappingsを既定値に戻す。
  - Rust：[Client/pomme-client/src/ui/menu/options.rs:701](../../pomme-client/src/ui/menu/options.rs#L701) — MainMenu::build_options_controls：Key Binds項目は表示される。
  - Rust：[Client/pomme-client/src/ui/menu/options.rs:706](../../pomme-client/src/ui/menu/options.rs#L706) — MainMenu::build_options_controls：Key Binds...がdisabled一覧に含まれる。
  - Rust：[Client/pomme-client/src/app/input.rs:49](../../pomme-client/src/app/input.rs#L49) — Action::default_key：実装対象Actionの既定キーを返すが、ここに再割当処理はない。

</details>

<a id="menus-06"></a>

<details>
<summary>menus-06 — 設定 › アクセシビリティ › 字幕と視点の揺れ</summary>

- 比較環境：クライアント共通
- 対象条件：字幕と視点の揺れを個別に切り替える
- 残件 / 比較すべき点：この2設定の実使用時の効果はRust側利用箇所と合わせて別途比較が必要。一致未確認であり、アクセシビリティ全項目の実装を意味しない。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/options/AccessibilityOptionsScreen.java:22](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/options/AccessibilityOptionsScreen.java#L22) — AccessibilityOptionsScreen.options：narrator/showSubtitlesなどの公式設定を一覧化。
  - Rust：[Client/pomme-client/src/ui/menu/options.rs:835](../../pomme-client/src/ui/menu/options.rs#L835) — MainMenu::build_options_accessibility：字幕と視点の揺れの現在値を表示。
  - Rust：[Client/pomme-client/src/ui/menu/options.rs:857](../../pomme-client/src/ui/menu/options.rs#L857) — MainMenu::build_options_accessibility：一部項目をdisabledに指定。
  - Rust：[Client/pomme-client/src/ui/menu/options.rs:1433](../../pomme-client/src/ui/menu/options.rs#L1433) — MainMenu::build_options_grid：字幕と視点の揺れはクリックで切替・保存。

</details>

<a id="menus-07"></a>

<details>
<summary>menus-07 — マルチプレイ › サーバー一覧 › 選択/追加/編集/削除/更新</summary>

- 比較環境：接続前
- 対象条件：サーバーを追加・編集・削除し、一覧を更新する
- 残件 / 比較すべき点：操作の存在とRustの保存呼出は確認。一致未確認で、ファイル互換性や各フォーム検証の細部は比較していない。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/multiplayer/JoinMultiplayerScreen.java:48](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/multiplayer/JoinMultiplayerScreen.java#L48) — JoinMultiplayerScreen.init：サーバー一覧を読み込み画面初期化。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/multiplayer/JoinMultiplayerScreen.java:74](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/multiplayer/JoinMultiplayerScreen.java#L74) — JoinMultiplayerScreen.init：直接接続とサーバー追加の操作を登録。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/multiplayer/JoinMultiplayerScreen.java:104](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/multiplayer/JoinMultiplayerScreen.java#L104) — JoinMultiplayerScreen.init：更新と戻るの操作を登録。
  - Rust：[Client/pomme-client/src/ui/menu/servers.rs:389](../../pomme-client/src/ui/menu/servers.rs#L389) — MainMenu::build_server_list：直接接続/追加、編集/削除、更新ボタンの構築と処理。
  - Rust：[Client/pomme-client/src/ui/server_list.rs:78](../../pomme-client/src/ui/server_list.rs#L78) — ServerList::save：サーバー一覧をJSON保存し、追加/更新/削除操作から呼ばれる。

</details>

<a id="menus-08"></a>

<details>
<summary>menus-08 — マルチプレイ › サーバー行 › 状態問い合わせと接続</summary>

- 比較環境：接続前
- 対象条件：サーバー一覧を開き、応答後にサーバーへ接続する
- 残件 / 比較すべき点：ping結果と接続アクションの経路を確認。一致未確認で、ping失敗/遅延/互換プロトコル表示・接続条件を公式と比較していない。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/multiplayer/JoinMultiplayerScreen.java:50](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/multiplayer/JoinMultiplayerScreen.java#L50) — JoinMultiplayerScreen.init：サーバー選択一覧を作成しオンラインサーバーを反映。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/multiplayer/JoinMultiplayerScreen.java:68](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/multiplayer/JoinMultiplayerScreen.java#L68) — JoinMultiplayerScreen.init：選択行のjoinを呼ぶボタン。
  - Rust：[Client/pomme-client/src/ui/menu/servers.rs:112](../../pomme-client/src/ui/menu/servers.rs#L112) — MainMenu::build_server_list：可視行の未問い合わせサーバーをping対象にする。
  - Rust：[Client/pomme-client/src/ui/server_list.rs:115](../../pomme-client/src/ui/server_list.rs#L115) — ping_all_servers：サーバーごとに状態問い合わせタスクを開始。
  - Rust：[Client/pomme-client/src/ui/menu/servers.rs:372](../../pomme-client/src/ui/menu/servers.rs#L372) — MainMenu::build_server_list：選択後のJoin Serverから接続アクションを生成。

</details>

<a id="menus-09"></a>

<details>
<summary>menus-09 — ポーズ › メニュー › 設定/ゲーム復帰/切断</summary>

- 比較環境：クライアント共通
- 対象条件：マルチプレイ中にポーズし、復帰・設定・切断を選ぶ
- 残件 / 比較すべき点：メニュー構成には公式との差がある。オンライン切断アクションの終了先/報告処理の意味論は追加比較が必要。シングルプレイ切断処理は別担当範囲。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/PauseScreen.java:94](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/PauseScreen.java#L94) — PauseScreen.createPauseMenu：公式ポーズ画面にAdvancements/Statistics等を登録。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/PauseScreen.java:157](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/PauseScreen.java#L157) — PauseScreen.createPauseMenu：シングルプレイ時にオプション/マルチプレイオプションを表示。
  - Rust：[Client/pomme-client/src/ui/pause.rs:208](../../pomme-client/src/ui/pause.rs#L208) — build_main：AdvancementsとStatisticsを無効ボタンとして描画。
  - Rust：[Client/pomme-client/src/ui/pause.rs:261](../../pomme-client/src/ui/pause.rs#L261) — build_main：設定アクション。
  - Rust：[Client/pomme-client/src/ui/pause.rs:285](../../pomme-client/src/ui/pause.rs#L285) — build_main：singleplayer判定でSave and Quit/Disconnect表示を選択。

</details>

<a id="menus-10"></a>

<details>
<summary>menus-10 — 死後 › リスポーン/タイトル移動 › 20 tick遅延と確認</summary>

- 比較環境：クライアント共通
- 対象条件：通常死亡とhardcore死亡で各ボタンを押し、20 tick前後を確認する
- 残件 / 比較すべき点：文言・分岐・遅延値は静的に対応するが、サーバー応答や実際の画面遷移は未比較。一致未確認。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/DeathScreen.java:46](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/DeathScreen.java#L46) — DeathScreen.init：通常Respawn/hardcore Spectateのボタンとrespawn処理。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/DeathScreen.java:68](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/DeathScreen.java#L68) — DeathScreen.handleExitToTitleScreen：hardcore直帰、通常は確認画面。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/DeathScreen.java:161](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/DeathScreen.java#L161) — DeathScreen.tick：20 tickでボタン有効化。
  - Rust：[Client/pomme-client/src/ui/death.rs:5](../../pomme-client/src/ui/death.rs#L5) — BUTTON_DELAY_TICKS：遅延閾値を20 tickと定義。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:7193](../../pomme-client/src/app/phases/in_game.rs#L7193) — update_game death_action dispatch：死亡画面のRespawnアクションをrespawn送信へつなぐ。

</details>

<a id="menus-11"></a>

<details>
<summary>menus-11 — サーバーダイアログ › ボディ/入力/アクション/閉じる</summary>

- 比較環境：公式サーバー接続
- 対象条件：サーバーから確認ダイアログを受け取り、入力/ボタン/Escape後の遷移を行う
- 残件 / 比較すべき点：基本経路と複数ダイアログ種別は確認したが、全codec/body/input/action形式と応答待機の同等性は確かめていない。一致未確認。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/dialog/DialogScreen.java:65](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/dialog/DialogScreen.java#L65) — DialogScreen.init：bodyとinputsを組み立て、本文のスクロール領域を設置。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/dialog/DialogScreen.java:149](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/dialog/DialogScreen.java#L149) — DialogScreen.shouldCloseOnEsc：ダイアログ設定に従いEscape閉鎖可否を決定。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/dialog/DialogScreen.java:161](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/dialog/DialogScreen.java#L161) — DialogScreen.runAction：afterActionに応じて画面遷移しクリックアクションを処理。
  - Rust：[Client/pomme-client/src/ui/server_dialog.rs:575](../../pomme-client/src/ui/server_dialog.rs#L575) — ServerDialogState::open：参照を解決・parse_dialogしてダイアログ状態を生成。
  - Rust：[Client/pomme-client/src/ui/server_dialog.rs:1322](../../pomme-client/src/ui/server_dialog.rs#L1322) — parse_dialog：Escape可否、after_action、ダイアログ種別を解釈。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:1204](../../pomme-client/src/app/phases/in_game.rs#L1204) — GameState dialog opening path：受信した参照をServerDialogState::openへ接続。

</details>


<a id="domain-blocks"></a>

## ブロック・照明描画

調査範囲：Rustの Section meshing/model selection、block/fluid geometry、透過pass、AO/lighting、visibility culling、destroy overlayの構築とRenderer/InGame接続を読みました。公式SectionCompiler、ModelBlockRenderer、BlockStateModel(Set)、FluidRenderer、破壊stage生成を参照し、実機画像・数値比較は未実施です。block entity本体やitem/weather/entityは対象外です。

- 制約：今回の確認はソース静的照合のみであり、ゲーム起動・cargo test・同一ワールドでの描画比較は行っていません。
- 制約：ブロックモデル資産の全形式・全state対応や全blockの見た目を網羅したとは判定していません。
- 制約：LOD&gt;0のvisibilityは意図的に全visibleであり、カリング機能の差分として記録しました。

| ID / 機能 | 公式の挙動 | Rustの現状・差分 | 実装 | 比較 / 検証 |
|---|---|---|---|---|
| [blocks-01](#blocks-01)<br>ブロック描画 › 状態別モデル › 配置に基づくweighted variant選択 | 公式はBlockStateModelSetから状態のモデルを取得し、BlockModelRendererが位置seedのRandomでmodel partsを選んでtessellateします。 | Rustはblockstate定義をbakeし、BlockRegistry::get_baked_model_atが位置seedでweighted choiceを選び、mesherが選択モデルのquadを出力します。 | 実装あり | 未検証<br>静的照合 |
| [blocks-02](#blocks-02)<br>ブロック描画 › multipart model › state条件を満たすpartの合成 | 公式SectionCompilerは各BlockStateのmodelを取得してblock rendererへ渡し、modelのparts/quadを出力します。 | Rustはmultipartのwhen条件に合うentryから位置seedでmodelを選んでquadを合成し、通常のmesher経路で出力します。 | 実装あり | 未検証<br>静的照合 |
| [blocks-03](#blocks-03)<br>ブロック描画 › 面カリング › 隣接opaque blockに隠れるcullface省略 | 公式ModelBlockRendererは方向別quadを隣接stateについてBlock.shouldRenderFaceで判定し、通過したquadだけ描画します。 | Rustはquadのcullface隣接blockがBlockRegistry::occludes_neighborを満たす場合にそのquadを省略します。 | 実装あり | 未検証<br>静的照合 |
| [blocks-04](#blocks-04)<br>ブロック描画 › AOと面陰影 › モデル頂点ごとの陰影 | 公式はambient occlusionが有効でblock発光がなくmodel partもAO対応の場合AO tessellationを使い、そうでなければflat lightingへ分岐します。 | RustはモデルquadのAOフラグと隣接遮蔽サンプルから頂点明度を算出し、block/sky lightと面方向shadeを組み合わせてmesh頂点に格納します。 | 実装あり | 未検証<br>静的照合 |
| [blocks-05](#blocks-05)<br>ブロック描画 › solid/cutout/partial alpha › material別pass | 公式SectionCompilerはquad materialのlayerでbufferを振り分け、透過layer meshはtransparencyStateでquad sortします。 | Rustはatlas alpha特性でopaqueをsolid、binary alphaをcutout、partial alphaをtranslucent indexへ分類し、translucent block modelを別passでdrawします。 | 実装あり | 未検証<br>静的照合 |
| [blocks-06](#blocks-06)<br>液体描画 › 水・溶岩面 › 同流体面の省略と高さ形状 | 公式FluidRendererは近傍流体・block occlusionを用いて面ごとに可視性を決め、周辺fluid heightからtop形状を構成します。 | Rustは隣接同流体または遮蔽block面を省略し、隣接高さを平均してtop/sidesを作り、水と溶岩を別のsprite/index経路に出力します。 | 実装あり | 未検証<br>静的照合 |
| [blocks-07](#blocks-07)<br>液体描画 › waterlogged block › block modelと水の同時描画 | 公式SectionCompilerはblockStateのfluidStateがemptyでなければfluid tessellationを行い、その後MODEL形状もtessellateします。 | Rustは通常block modelの出力後、Solid分類かつWater fluid stateを持つblockに水geometryを追加します。 | 実装あり | 未検証<br>静的照合 |
| [blocks-08](#blocks-08)<br>光源 › sky/block light › light更新後のblock明度 | 公式SectionCompilerはregionを介してblock model/fluid lightingを取得し、BlockModelLighterを有効化してsection meshに反映します。 | RustはChunkLightDataのsky/block nibbleを読み、max(sky, block)をlight tableへ変換しmesh sampleに格納します。light engineのdirty sectionはInGameからremeshに接続されています。 | 実装あり | 未検証<br>静的照合 |
| [blocks-09](#blocks-09)<br>カリング › cave visibility › 遮蔽されたsectionのdraw抑制 | 公式SectionCompilerはsolid-render blockをVisGraphに登録し、全sectionでresolveしたface connectivityをcompile結果へ返します。 | RustのLOD0はfull opaque voxelからface connectivityを計算しsection visibility maskで描画を抑制する。LOD&gt;0は全方向visibleとしてcave cullingを省略する。これは内部カリング・過剰描画の差であり、表示結果の非互換は未確認。 | 一部実装 | 未検証<br>静的照合 |
| [blocks-10](#blocks-10)<br>破壊overlay › 対象block › 10段階crack texture表示 | 公式の破壊progressは0より大きい場合int(progress*10)のstageを算出し、stage別destroy textureを用います。 | Rustはdestroy stage texture 10枚のatlasをロードし、対象stateのmodel quadを投影UV付きoverlayとして描画します。Rendererのdestroy_infoが存在する場合のみdrawを呼びます。 | 実装あり | 未検証<br>静的照合 |

### 条件・残件・根拠

<a id="blocks-01"></a>

<details>
<summary>blocks-01 — ブロック描画 › 状態別モデル › 配置に基づくweighted variant選択</summary>

- 比較環境：クライアント共通
- 対象条件：weighted variantsを持つ通常ブロックを異なる座標に設置する
- 残件 / 比較すべき点：同一stateの複数座標で公式とRustの選択variant・表示quadを比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/BlockStateModelSet.java:18](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/BlockStateModelSet.java#L18) — BlockStateModelSet.get：BlockStateからモデルを取得（未登録時はmissingModel）。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/ModelBlockRenderer.java:61](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/ModelBlockRenderer.java#L61) — ModelBlockRenderer.tesselateBlock：位置seedをRandomへ設定しmodel partsを収集。
  - Rust：[Client/pomme-client/src/world/block/registry.rs:992](../../pomme-client/src/world/block/registry.rs#L992) — BlockRegistry::get_baked_model_at：位置seedを用いたweighted model選択。
  - Rust：[Client/pomme-client/src/renderer/chunk/mesher.rs:2062](../../pomme-client/src/renderer/chunk/mesher.rs#L2062) — compile_section mesh path：選択modelをemit_baked_modelへ渡す。

</details>

<a id="blocks-02"></a>

<details>
<summary>blocks-02 — ブロック描画 › multipart model › state条件を満たすpartの合成</summary>

- 比較環境：クライアント共通
- 対象条件：複数のstate条件に応じてpartが切り替わるblockstateを表示する
- 残件 / 比較すべき点：multipart各条件・weighted partが公式と同じ組合せ、回転、可視面になるか比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/chunk/SectionCompiler.java:94](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/chunk/SectionCompiler.java#L94) — SectionCompiler.compile：状態に対応するモデルをBlockModelRendererへ渡す。
  - Rust：[Client/pomme-client/src/world/block/registry.rs:1040](../../pomme-client/src/world/block/registry.rs#L1040) — BlockRegistry::get_multipart_quads_at：条件を評価し、seedで選択した各entryのquadを連結。
  - Rust：[Client/pomme-client/src/renderer/chunk/mesher.rs:2087](../../pomme-client/src/renderer/chunk/mesher.rs#L2087) — compile_section mesh path：multipart quadをemit_multipartへ渡す。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:1717](../../pomme-client/src/app/phases/in_game.rs#L1717) — InGame::drain_and_upload_meshes：生成meshを本番のupload経路でdrainする。

</details>

<a id="blocks-03"></a>

<details>
<summary>blocks-03 — ブロック描画 › 面カリング › 隣接opaque blockに隠れるcullface省略</summary>

- 比較環境：クライアント共通
- 対象条件：隣接するfull opaque block間の境界面を確認する
- 残件 / 比較すべき点：透明・部分形状・同一block境界を含め、Block.shouldRenderFaceとの条件差を比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/ModelBlockRenderer.java:121](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/ModelBlockRenderer.java#L121) — tesselateAmbientOcclusion：方向quadをshouldRenderFace判定し、可視なら出力。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/ModelBlockRenderer.java:194](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/ModelBlockRenderer.java#L194) — shouldRenderFace：Block.shouldRenderFaceへ隣接state判定を委譲。
  - Rust：[Client/pomme-client/src/renderer/chunk/mesher.rs:2662](../../pomme-client/src/renderer/chunk/mesher.rs#L2662) — emit_baked_model：cullface隣接stateがoccludeする場合quadをcontinue。
  - Rust：[Client/pomme-client/src/world/block/registry.rs:1077](../../pomme-client/src/world/block/registry.rs#L1077) — BlockRegistry::occludes_neighbor：Rust側の隣接遮蔽判定。

</details>

<a id="blocks-04"></a>

<details>
<summary>blocks-04 — ブロック描画 › AOと面陰影 › モデル頂点ごとの陰影</summary>

- 比較環境：クライアント共通
- 対象条件：発光しないAO有効modelをopaque blockに隣接させる
- 残件 / 比較すべき点：公式・Rust双方の4頂点AO値、light sample位置、発光block時のflat分岐を同配置で比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/ModelBlockRenderer.java:66](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/ModelBlockRenderer.java#L66) — ModelBlockRenderer.tesselateBlock：設定・発光・model AOに応じてAO/flat tessellationを選択。
  - Rust：[Client/pomme-client/src/renderer/chunk/mesher.rs:3719](../../pomme-client/src/renderer/chunk/mesher.rs#L3719) — compute_face_ao_samples：隣接shade/lightを頂点別にサンプルしAOを計算。
  - Rust：[Client/pomme-client/src/renderer/chunk/block_ao.rs:24](../../pomme-client/src/renderer/chunk/block_ao.rs#L24) — vertex_ao_level：遮蔽サンプルからAO levelとbrightnessを決める。
  - Rust：[Client/pomme-client/src/renderer/chunk/mesher.rs:2685](../../pomme-client/src/renderer/chunk/mesher.rs#L2685) — emit_baked_model：モデルquadのAO属性をlighting計算へ渡して出力。

</details>

<a id="blocks-05"></a>

<details>
<summary>blocks-05 — ブロック描画 › solid/cutout/partial alpha › material別pass</summary>

- 比較環境：クライアント共通
- 対象条件：solid、cutout texture、partial-alpha block modelを同一視野で描画する
- 残件 / 比較すべき点：公式material/layer判定との対応、partial-alpha quad sorting順と複数遮蔽物の見え方を比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/chunk/SectionCompiler.java:64](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/chunk/SectionCompiler.java#L64) — SectionCompiler.compile quadOutput：quad material layerから出力bufferを選択。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/chunk/SectionCompiler.java:120](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/chunk/SectionCompiler.java#L120) — SectionCompiler.compile：TRANSLUCENT meshをvertex sortingでsort。
  - Rust：[Client/pomme-client/src/renderer/chunk/mesher.rs:312](../../pomme-client/src/renderer/chunk/mesher.rs#L312) — MeshSink::indices_for：region transparency/opacity別にindex listへ振分け。
  - Rust：[Client/pomme-client/src/renderer/mod.rs:3287](../../pomme-client/src/renderer/mod.rs#L3287) — Renderer world translucent passes：partial-alpha block modelを別translucent passで描画。

</details>

<a id="blocks-06"></a>

<details>
<summary>blocks-06 — 液体描画 › 水・溶岩面 › 同流体面の省略と高さ形状</summary>

- 比較環境：クライアント共通
- 対象条件：水源・流れる水・溶岩が隣接する地形を表示する
- 残件 / 比較すべき点：斜面・角のfluid height、side face culling、flow UVと公式FluidRendererを比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/FluidRenderer.java:64](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/FluidRenderer.java#L64) — FluidRenderer.shouldRenderFace：同一fluid隣接と自己遮蔽で面を判定。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/FluidRenderer.java:85](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/block/FluidRenderer.java#L85) — FluidRenderer.tesselate：6方向の面判定およびfluid height計算。
  - Rust：[Client/pomme-client/src/renderer/chunk/mesher.rs:3111](../../pomme-client/src/renderer/chunk/mesher.rs#L3111) — emit_fluid：同流体・opaque隣接面を落とし、高さと面geometryを生成。
  - Rust：[Client/pomme-client/src/renderer/mod.rs:3298](../../pomme-client/src/renderer/mod.rs#L3298) — Renderer water pass：water専用pipeline/indexでopaque後に描画。

</details>

<a id="blocks-07"></a>

<details>
<summary>blocks-07 — 液体描画 › waterlogged block › block modelと水の同時描画</summary>

- 比較環境：クライアント共通
- 対象条件：waterlogged stair/slab等のblock modelを配置する
- 残件 / 比較すべき点：水面の有無・高さと形状遮蔽を公式と比較し、全fluid-bearing block分類を確認する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/chunk/SectionCompiler.java:89](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/chunk/SectionCompiler.java#L89) — SectionCompiler.compile：blockStateのfluidStateが非emptyならfluidRendererを呼ぶ。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/chunk/SectionCompiler.java:94](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/chunk/SectionCompiler.java#L94) — SectionCompiler.compile：fluidとは別にMODEL形状をtessellate。
  - Rust：[Client/pomme-client/src/renderer/chunk/mesher.rs:2131](../../pomme-client/src/renderer/chunk/mesher.rs#L2131) — compile_section waterlogged branch：Solid blockのwater fluid stateを判定し追加emit_fluid。
  - Rust：[Client/pomme-client/src/renderer/chunk/mesher.rs:2062](../../pomme-client/src/renderer/chunk/mesher.rs#L2062) — compile_section model path：waterlogged fluid追加より先に通常modelを出力。

</details>

<a id="blocks-08"></a>

<details>
<summary>blocks-08 — 光源 › sky/block light › light更新後のblock明度</summary>

- 比較環境：クライアント共通
- 対象条件：松明またはsky lightがあるblock周囲で光源追加・削除を反映する
- 残件 / 比較すべき点：公式block/sky light値、dimension sky darkening、境界伝播と再mesh後の明るさを比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/chunk/SectionCompiler.java:60](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/chunk/SectionCompiler.java#L60) — SectionCompiler.compile：BlockModelLighter section compile cacheを有効化。
  - Rust：[Client/pomme-client/src/renderer/chunk/mesher.rs:1690](../../pomme-client/src/renderer/chunk/mesher.rs#L1690) — ChunkStoreSnapshot::get_light_raw：sky/block light nibbleを取得し欠損列はsky=15/block=0。
  - Rust：[Client/pomme-client/src/renderer/chunk/mesher.rs:1706](../../pomme-client/src/renderer/chunk/mesher.rs#L1706) — ChunkStoreSnapshot::get_light：max(sky,block)をLIGHT_TABLEで明度に変換。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:1616](../../pomme-client/src/app/phases/in_game.rs#L1616) — InGame::update_light：light engine更新・dirty sectionの再meshを開始。

</details>

<a id="blocks-09"></a>

<details>
<summary>blocks-09 — カリング › cave visibility › 遮蔽されたsectionのdraw抑制</summary>

- 比較環境：クライアント共通
- 対象条件：洞窟内外から遮蔽壁越しのsectionを、LOD0とLOD&gt;0の距離で確認する
- 残件 / 比較すべき点：LOD0/遠距離LOD境界で同じ視点の表示結果を比較する。過剰描画・性能差と、表示欠落等の互換性差を別に評価する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/chunk/SectionCompiler.java:59](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/chunk/SectionCompiler.java#L59) — SectionCompiler.compile：各section compileでVisGraphを作成。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/chunk/SectionCompiler.java:78](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/chunk/SectionCompiler.java#L78) — SectionCompiler.compile：solid-render blockをopaque graphへ登録。
  - Rust：[Client/pomme-client/src/renderer/chunk/mesher.rs:1993](../../pomme-client/src/renderer/chunk/mesher.rs#L1993) — compile_sections visibility branch：LOD&gt;0 sectionはVisibilitySet::allとしてcave cullingを延期。
  - Rust：[Client/pomme-client/src/renderer/chunk/occlusion_graph.rs:134](../../pomme-client/src/renderer/chunk/occlusion_graph.rs#L134) — compute_visibility：LOD0向けにsection内の非opaque領域をflood fill。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:1978](../../pomme-client/src/app/phases/in_game.rs#L1978) — InGame::apply_visibility：可視section maskをrendererへ接続。

</details>

<a id="blocks-10"></a>

<details>
<summary>blocks-10 — 破壊overlay › 対象block › 10段階crack texture表示</summary>

- 比較環境：クライアント共通
- 対象条件：破壊可能blockを長押しし、progress stageが変化する
- 残件 / 比較すべき点：stage境界値、対象state/model geometry、投影方向とalpha/depth挙動を公式と比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/MultiPlayerGameMode.java:551](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/MultiPlayerGameMode.java#L551) — MultiPlayerGameMode.getDestroyStage：progressに応じて0..9のstage値を計算し、progressなしは-1。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/chunk/SectionCompiler.java:94](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/chunk/SectionCompiler.java#L94) — SectionCompiler.compile：通常のblock modelのquad構成を公式側で使用する根拠。
  - Rust：[Client/pomme-client/src/renderer/pipelines/block_overlay.rs:439](../../pomme-client/src/renderer/pipelines/block_overlay.rs#L439) — load_destroy_atlas：destroy_stage_0..9をロード。
  - Rust：[Client/pomme-client/src/renderer/pipelines/block_overlay.rs:246](../../pomme-client/src/renderer/pipelines/block_overlay.rs#L246) — BlockOverlayPipeline::draw：block state/position/stageからoverlay vertexを作り描画。
  - Rust：[Client/pomme-client/src/renderer/mod.rs:3155](../../pomme-client/src/renderer/mod.rs#L3155) — Renderer world draw destroy_info：destroy_infoがあるとoverlay pipelineへstage付きで接続。

</details>


<a id="domain-atmosphere"></a>

## 空・天候・ワールド境界

調査範囲：Rust側はSkyStateとsky/weather/cloud/world_borderパイプライン、world/border.rs、net/environment.rs、in_game.rsでの入力・描画接続を確認。公式側はSkyRenderer、WeatherEffectRenderer、CloudRenderer、AtmosphericFogEnvironment、WorldBorderRendererおよびWorldBorderを確認。独立した全属性トラックのdecode/evaluateや全dimensionの最終描画、サーバー権威のborder移動制約は網羅していない。

- 制約：描画距離・光量・色の実機/スクリーンショット比較は行っていない。静的照合のみ。
- 制約：SkyStateおよびdimension環境属性のネットワーク入力元・全timeline codecを網羅していない。
- 制約：WorldBorderのサーバー側移動制約・ダメージ・警告HUDは担当範囲外として未調査。

| ID / 機能 | 公式の挙動 | Rustの現状・差分 | 実装 | 比較 / 検証 |
|---|---|---|---|---|
| [atmosphere-01](#atmosphere-01)<br>空 › 太陽・月・星 › 時刻と天候に応じた描画 | 26.2はdimensionのskyboxがNONEでない場合、EnvironmentAttributeProbeから太陽/月/星の角度、星の明るさ、月相、空色などを取得し、雨量で太陽・月を暗くする。 | RustはSkyStateのday_timeから固定の時刻キーフレームと太陽/月の角度・月相を算出し、雨・雷で星を暗くして太陽/月を薄くする。本番のSkyPipelineはworld描画中に呼ばれる。 | 実装あり | 未検証<br>静的照合 |
| [atmosphere-02](#atmosphere-02)<br>空 › 月 › 月相テクスチャ選択 | 月相に応じた8種類の月画像を選び、その月相を天体描画に使う。 | Rustはday_timeの日数を8相に分け、8枚のmoon画像をロードして対応する画像descriptorで描画する。 | 実装あり | 未検証<br>静的照合 |
| [atmosphere-03](#atmosphere-03)<br>天候 › biome降水 › 雨・雪・降水なしの判定 | 公式は各列のbiome降水種別を問い合わせ、雨/雪のみの列をカメラ周辺に抽出し描画する。 | Rustはbiome climateの降水有無と高さ補正後の温度（0.15未満なら雪）から種別を決め、周辺11×11列の天候描画へ渡す。 | 実装あり | 未検証<br>静的照合 |
| [atmosphere-04](#atmosphere-04)<br>天候 › 雪 › 降雪列の明るさ | 公式の雪列はblock lightとsky lightをそれぞれ3/4に弱めた明るさで描画する。 | Rustは雨雪共通で列位置のworld_brightnessをそのまま頂点brightnessに使い、雪用の3/4補正は見当たらない。 | 一部実装 | 差分あり<br>静的照合 |
| [atmosphere-05](#atmosphere-05)<br>雲 › 高さ・スクロール・表示距離 | 公式はdimensionのcloud heightを基準に4ブロック厚で、game timeとテクスチャ寸法に基づくX方向スクロールを行い、cloud range設定から描画半径を決める。 | Rustは高さ192.33、厚さ4、同じスクロール係数・周期を使うが、描画距離はcloud rangeではなくMIN_FAR由来の固定上限で制限される。 | 一部実装 | 差分あり<br>静的照合 |
| [atmosphere-06](#atmosphere-06)<br>霧 › overworld大気 › 空色と天候を反映した背景色 | AtmosphericFogEnvironmentはFOG_COLORと天候補正済みSKY_COLORを、sky fog end距離とrender distanceに応じた係数で混合する。 | Rustのclear_color_linearはoverworldに限り固定FOG_COLOR・固定天候補正とrender distanceから空色を混ぜる。確認した範囲では霧開始/終了距離やbiome・視線方向の霧色補正をこの関数で扱わない。 | 一部実装 | 差分あり<br>静的照合 |
| [atmosphere-07](#atmosphere-07)<br>dimension環境 › Nether等 › 空色・霧属性の適用 | 公式AtmosphericFogEnvironmentはdimensionの環境属性probeからFOG_COLOR、SKY_COLOR、霧距離などを取得し、dimension環境ごとの値で処理する。 | Rustのclear_color_linearはminecraft:overworld以外をsky_colorのlinear変換だけで返し、属性由来のfog colorやfog distanceを適用しない。 | 一部実装 | 差分あり<br>静的照合 |
| [atmosphere-08](#atmosphere-08)<br>world border › サーバー更新 › 中心・サイズ・警告情報の反映 | 公式クライアントは初期border状態や中心・サイズ変更・サイズ補間・警告距離/時間を受け、状態と描画を更新する。 | Rustのnetwork handlerは各border packetをNetworkEvent化し、app側がWorldBorderの中心・サイズ補間・warning情報へ反映する。borderは固定world tickで進行し、in_gameから描画状態がRendererへ渡る。 | 実装あり | 未検証<br>静的照合 |
| [atmosphere-09](#atmosphere-09)<br>world border › 表示 › 接近時の枠・alpha・状態色 | 公式は枠からrender distance以内の側面を表示し、最近接border距離に基づいてalphaを4乗で減衰し、border statusの色を適用する。 | Rustは部分tick boundsを抽出し、最近接距離の4乗alphaと各側面の可視判定を作成してpipelineで描画する。Growing/Shrinking/Stationary別の色付きforcefield textureを使う。 | 実装あり | 未検証<br>静的照合 |
| [atmosphere-10](#atmosphere-10)<br>world border › 制約 › block操作rayのborder越境 | 公式WorldBorderはXZの半開区間で内外判定し、border越境位置をbounds内へclampする機能を持つ。サーバー側の移動・ダメージ権威とは別のクライアント処理である。 | Rustは同様のcontains/clamp_locationを実装し、block interactionのrayが内側から外へ出る場合にhit pointをborderへclampする。確認範囲ではWorldBorderを使ったクライアント移動制限は接続されていない。 | 一部実装 | 未検証<br>静的照合 |

### 条件・残件・根拠

<a id="atmosphere-01"></a>

<details>
<summary>atmosphere-01 — 空 › 太陽・月・星 › 時刻と天候に応じた描画</summary>

- 比較環境：クライアント共通
- 対象条件：通常の空が表示されるdimensionで時刻を進め、雨量を変える
- 残件 / 比較すべき点：dimension側の角度・星明るさ・月相属性をRustの固定キーフレーム経路がどこまで再現するか、昼夜・雨天で比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/SkyRenderer.java:265](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/SkyRenderer.java#L265) — extractRenderState：skybox条件および角度、雨輝度、星輝度、月相、空色を環境属性から取得。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/SkyRenderer.java:316](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/SkyRenderer.java#L316) — renderSunMoonAndStars：抽出した角度・月相・雨輝度・星輝度で天体を描画。
  - Rust：[Client/pomme-client/src/renderer/pipelines/sky.rs:517](../../pomme-client/src/renderer/pipelines/sky.rs#L517) — SkyPipeline::update_and_draw：昼時刻と固定キーフレーム、天候から描画uniformを構成。
  - Rust：[Client/pomme-client/src/renderer/mod.rs:3118](../../pomme-client/src/renderer/mod.rs#L3118) — Renderer::render_world：ゲームワールド描画経路からSkyPipelineを呼び出す。

</details>

<a id="atmosphere-02"></a>

<details>
<summary>atmosphere-02 — 空 › 月 › 月相テクスチャ選択</summary>

- 比較環境：クライアント共通
- 対象条件：day_timeを8日相当進めて各月相を表示する
- 残件 / 比較すべき点：各日境界の月相インデックスと公式MoonPhaseのテクスチャ順・切替時刻を照合する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/SkyRenderer.java:372](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/SkyRenderer.java#L372) — renderMoon：MoonPhase.index()を使い月相の頂点/画像領域を選択。
  - Rust：[Client/pomme-client/src/renderer/pipelines/sky.rs:20](../../pomme-client/src/renderer/pipelines/sky.rs#L20) — moon_phase：8日周期のday_timeから相indexを算出。
  - Rust：[Client/pomme-client/src/renderer/pipelines/sky.rs:365](../../pomme-client/src/renderer/pipelines/sky.rs#L365) — SkyPipeline::new：8相の月画像をロードしmoon_setsへ結び付ける。
  - Rust：[Client/pomme-client/src/renderer/pipelines/sky.rs:553](../../pomme-client/src/renderer/pipelines/sky.rs#L553) — SkyPipeline::update_and_draw：計算したphase descriptorを月描画に使用。

</details>

<a id="atmosphere-03"></a>

<details>
<summary>atmosphere-03 — 天候 › biome降水 › 雨・雪・降水なしの判定</summary>

- 比較環境：クライアント共通
- 対象条件：降水あり/なしbiomeおよび雪線上下で雨雪を確認する
- 残件 / 比較すべき点：公式getPrecipitationAtの高さ・biome温度境界、降水列の半径とカメラ高さを同じ地形で比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/WeatherEffectRenderer.java:62](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/WeatherEffectRenderer.java#L62) — extractRenderState：カメラ周辺を走査しgetPrecipitationAtの結果別に雨雪列を抽出。
  - Rust：[Client/pomme-client/src/renderer/pipelines/weather.rs:33](../../pomme-client/src/renderer/pipelines/weather.rs#L33) — precipitation_for：降水無効判定と高さ補正温度による雪/雨分類。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:8198](../../pomme-client/src/app/phases/in_game.rs#L8198) — build_weather_columns：chunk biome・高さを参照し天候列を組み立てる。
  - Rust：[Client/pomme-client/src/renderer/mod.rs:3320](../../pomme-client/src/renderer/mod.rs#L3320) — Renderer::render_world：生成済み列をweather pipelineへ渡す。

</details>

<a id="atmosphere-04"></a>

<details>
<summary>atmosphere-04 — 天候 › 雪 › 降雪列の明るさ</summary>

- 比較環境：クライアント共通
- 対象条件：明るさの異なる場所で雨列と雪列の同じ相対距離・強度を表示する
- 残件 / 比較すべき点：雪の列を公式同様にblock/sky各light値の3/4補正した場合との見た目差を確認する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/WeatherEffectRenderer.java:180](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/WeatherEffectRenderer.java#L180) — createSnowColumnInstance：雪列のblock/sky lightを各3/4に補正。
  - Rust：[Client/pomme-client/src/renderer/pipelines/weather.rs:325](../../pomme-client/src/renderer/pipelines/weather.rs#L325) — WeatherPipeline::update_and_draw：rain/snowどちらもcol.lightを補正せず輝度へ乗算。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:8236](../../pomme-client/src/app/phases/in_game.rs#L8236) — build_weather_columns：共通world_brightnessをlightとして列へ保存。

</details>

<a id="atmosphere-05"></a>

<details>
<summary>atmosphere-05 — 雲 › 高さ・スクロール・表示距離</summary>

- 比較環境：クライアント共通
- 対象条件：雲の高さと水平スクロールを見ながら遠距離設定を上げる
- 残件 / 比較すべき点：公式CloudRendererのrange=128 chunksまで伸びるケースに対し、Rustのfar-plane制限で雲の終端・フェードが見えないか比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/CloudRenderer.java:125](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/CloudRenderer.java#L125) — render：雲レイヤーと描画距離rangeから半径を求め、時刻ベースでスクロール。
  - Rust：[Client/pomme-client/src/renderer/pipelines/clouds.rs:13](../../pomme-client/src/renderer/pipelines/clouds.rs#L13) — CLOUD_HEIGHT：雲の固定レイヤー高さ。
  - Rust：[Client/pomme-client/src/renderer/pipelines/clouds.rs:30](../../pomme-client/src/renderer/pipelines/clouds.rs#L30) — CLOUD_FADE_END：far-planeのMIN_FARでフェード終端を固定。
  - Rust：[Client/pomme-client/src/renderer/pipelines/clouds.rs:264](../../pomme-client/src/renderer/pipelines/clouds.rs#L264) — CloudPipeline::update_and_draw：同じ処理で雲グリッドを更新・描画。

</details>

<a id="atmosphere-06"></a>

<details>
<summary>atmosphere-06 — 霧 › overworld大気 › 空色と天候を反映した背景色</summary>

- 比較環境：クライアント共通
- 対象条件：overworldでrender distance、雨/雷、日の出方向を変える
- 残件 / 比較すべき点：AtmosphericFogEnvironment.getBaseColor/setupFogの属性入力、日の出方向補正および霧距離を含む実画面の色・距離を比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/fog/environment/AtmosphericFogEnvironment.java:26](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/fog/environment/AtmosphericFogEnvironment.java#L26) — getBaseColor：FOG_COLOR、日出色、天候補正済み空色を取得。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/fog/environment/AtmosphericFogEnvironment.java:44](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/fog/environment/AtmosphericFogEnvironment.java#L44) — getBaseColor：sky fog endと描画距離から混合率を求める。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/fog/environment/AtmosphericFogEnvironment.java:68](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/fog/environment/AtmosphericFogEnvironment.java#L68) — setupFog：属性由来の環境霧開始/終了、sky/cloud終端および雨霧を設定。
  - Rust：[Client/pomme-client/src/renderer/pipelines/sky.rs:208](../../pomme-client/src/renderer/pipelines/sky.rs#L208) — SkyState::clear_color_linear：overworld向け固定FOG_COLOR・固定距離式を適用。

</details>

<a id="atmosphere-07"></a>

<details>
<summary>atmosphere-07 — dimension環境 › Nether等 › 空色・霧属性の適用</summary>

- 比較環境：クライアント共通
- 対象条件：Nether等のoverworld以外で空色・背景色・霧距離を確認する
- 残件 / 比較すべき点：各dimensionのskybox種類およびFOG_COLOR/FOG_START_DISTANCE/FOG_END_DISTANCEを変え、Rustの非overworld分岐と比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/fog/environment/AtmosphericFogEnvironment.java:26](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/fog/environment/AtmosphericFogEnvironment.java#L26) — getBaseColor：dimensionのattributeProbeから霧色を取得。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/fog/environment/AtmosphericFogEnvironment.java:68](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/fog/environment/AtmosphericFogEnvironment.java#L68) — setupFog：dimension属性から霧開始・終了距離を取得。
  - Rust：[Client/pomme-client/src/renderer/pipelines/sky.rs:208](../../pomme-client/src/renderer/pipelines/sky.rs#L208) — SkyState::clear_color_linear：overworld以外は天候・fog attribute処理を通らずsky colorを返す。
  - Rust：[Client/pomme-client/src/net/environment.rs:48](../../pomme-client/src/net/environment.rs#L48) — DimensionEnvironmentInput：dimension環境入力の一部属性を保持するが、clear_color_linearでは参照されない。

</details>

<a id="atmosphere-08"></a>

<details>
<summary>atmosphere-08 — world border › サーバー更新 › 中心・サイズ・警告情報の反映</summary>

- 比較環境：公式サーバー接続
- 対象条件：初期化後にborder中心・サイズ・補間時間・警告設定を変更する
- 残件 / 比較すべき点：packet受信から描画までの値・補間期間を公式クライアントと同じ更新回数で比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/level/border/WorldBorder.java:137](../../../minecraft-26.2-decompiled/src/net/minecraft/world/level/border/WorldBorder.java#L137) — setCenter：中心変更をborder状態へ反映。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/level/border/WorldBorder.java:188](../../../minecraft-26.2-decompiled/src/net/minecraft/world/level/border/WorldBorder.java#L188) — lerpSizeBetween：サイズ補間を開始し、同値なら静的extentにする。
  - Rust：[Client/pomme-client/src/net/handler.rs:783](../../pomme-client/src/net/handler.rs#L783) — handle_game_packet：InitializeBorderおよび個別border packetをイベント化。
  - Rust：[Client/pomme-client/src/app/core.rs:4670](../../pomme-client/src/app/core.rs#L4670) — NetworkEvent::WorldBorderInitialize：初期packetをWorldBorder::initializeへ適用。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:3858](../../pomme-client/src/app/phases/in_game.rs#L3858) — tick_game：world tickごとにborder lerpを進行。

</details>

<a id="atmosphere-09"></a>

<details>
<summary>atmosphere-09 — world border › 表示 › 接近時の枠・alpha・状態色</summary>

- 比較環境：クライアント共通
- 対象条件：borderの内側・外側および四辺のrender-distance範囲で表示する
- 残件 / 比較すべき点：部分tick補間中、各辺の距離・alpha・色とforcefieldのアニメーションを同条件で確認する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/WorldBorderRenderer.java:105](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/WorldBorderRenderer.java#L105) — extract：border bounds、距離条件、4乗alphaとstatus tintを抽出。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/WorldBorderRenderer.java:131](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/WorldBorderRenderer.java#L131) — render：距離内の側面をforcefield textureで描画。
  - Rust：[Client/pomme-client/src/renderer/pipelines/world_border.rs:39](../../pomme-client/src/renderer/pipelines/world_border.rs#L39) — extract_border：境界距離・alpha・side・statusからBorderDrawを生成。
  - Rust：[Client/pomme-client/src/renderer/pipelines/world_border.rs:303](../../pomme-client/src/renderer/pipelines/world_border.rs#L303) — WorldBorderPipeline::draw：status別textureで抽出結果の側面を描画。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:7077](../../pomme-client/src/app/phases/in_game.rs#L7077) — render_frame：world border stateをRendererへ設定。

</details>

<a id="atmosphere-10"></a>

<details>
<summary>atmosphere-10 — world border › 制約 › block操作rayのborder越境</summary>

- 比較環境：クライアント共通
- 対象条件：border内から外側のblockへ視線・rayを向ける。移動制約はサーバー接続で別途確認する
- 残件 / 比較すべき点：ray境界hitの端点・半開境界を比較する。移動拒否やborder外ダメージはサーバー権威処理なので本調査では未確認。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/level/border/WorldBorder.java:48](../../../minecraft-26.2-decompiled/src/net/minecraft/world/level/border/WorldBorder.java#L48) — isWithinBounds：XZ境界を半開区間で判定。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/level/border/WorldBorder.java:65](../../../minecraft-26.2-decompiled/src/net/minecraft/world/level/border/WorldBorder.java#L65) — clampVec3ToBound：位置を境界内へclamp。
  - Rust：[Client/pomme-client/src/world/border.rs:193](../../pomme-client/src/world/border.rs#L193) — WorldBorder::contains：同様にXZ境界を半開区間で判定。
  - Rust：[Client/pomme-client/src/world/border.rs:200](../../pomme-client/src/world/border.rs#L200) — WorldBorder::clamp_location：ray位置をbounds内へclamp。
  - Rust：[Client/pomme-client/src/player/interaction.rs:3193](../../pomme-client/src/player/interaction.rs#L3193) — synthesize_block_hit_result：ray越境時にborder clampを利用。

</details>


<a id="domain-mobs"></a>

## 生物の描画

調査範囲：Client/pomme-client/src/entity/mod.rs の同期metadata・mob state、renderer/entity_model.rs と entity_models、pipelines/entity_renderer.rs のMobDef/animation/draw、app/phases/in_game.rs のliving抽出を、公式26.2の個別 mob renderer と照合。具体的な9種の挙動のみを扱い、player skin/equipment、非living、他mob全般の網羅・実機画像比較はしていない。

- 制約：今回の確認はコード静的照合であり、アプリ起動・cargo test・実機接続・画像比較は実施していない。
- 制約：current-entity-route-matrix.mdは別commit 8ffc5465bf81ffa25cf1867866e8c64f23e88857のsnapshotなので、手掛かりとしてのみ使用し、判定根拠は現在ソースから取り直した。
- 制約：entity rendererへ入るgeneric本番呼出経路は確認したが、このJSONは全mob・全metadata・全特殊layerの網羅ではない。

| ID / 機能 | 公式の挙動 | Rustの現状・差分 | 実装 | 比較 / 検証 |
|---|---|---|---|---|
| [mobs-01](#mobs-01)<br>living mob描画 › axolotl › variantと成体/子供テクスチャ | 公式は5 variantごとに成体・子供テクスチャを選び、variant固有の水中/地上/移動/死んだふり等のアニメーション状態をRenderStateへ渡す。 | Rustはvariant metadataを保存し、成体/子供のvariant別テクスチャとモデルを選ぶ本番経路がある。一方MobDefのanimationはStaticで、状態別アニメーションは接続されていない。 | 一部実装 | 差分あり<br>静的照合 |
| [mobs-02](#mobs-02)<br>living mob描画 › mooshroom › variant色と成体/子供 | 公式はbrown/red variantに対応する成体/子供textureを選び、さらにvariantに応じたblock-stateのキノコモデルを専用layerで描画する。 | Rustはmetadata variantを保存し、赤/茶の成体/子供textureとcow bakeを本番MobDefから描画するが、専用キノコblock-model layerはMobDefにない。 | 一部実装 | 差分あり<br>静的照合 |
| [mobs-03](#mobs-03)<br>living mob描画 › wolf › tame時の首輪と鎧layer | 公式WolfRendererは成体/子供モデルに加えて鎧layerと首輪layerを登録し、tame状態に応じて首輪を描画する。 | Rustはwolfの成体/子供bakeとtame・collar color metadataを取り込み、tame時の染色首輪overlayを描画する。wolf armor layerは未実装と明記されている。 | 一部実装 | 差分あり<br>静的照合 |
| [mobs-04](#mobs-04)<br>living mob描画 › sheep › 毛刈り・羊毛色・jeb_名 | 公式SheepRendererは成体/子供モデルと羊毛undercoat/wool layerを用い、毛刈り状態・羊毛色とjeb_名による虹色羊毛をrender stateへ反映する。 | Rustは羊metadata byteから色とsheared bitを保持し、成体/子供の羊毛overlayを登録する。sheep_extrasで毛刈り時にoverlayを隠し、jeb_名では時間依存tintを作る。 | 実装あり | 未検証<br>静的照合 |
| [mobs-05](#mobs-05)<br>living mob描画 › creeper › powered auraと爆発前膨張 | 公式はpowered時のenergy-swirl layerを描き、爆発前のswellingに応じて胴体を拡縮・点滅させる。 | Rustはpowered metadataからcharged overlay tintを有効化してauraを描くが、MobDefはQuadruped animationで、爆発swelling値の状態/scale/white-overlay経路がない。 | 一部実装 | 差分あり<br>静的照合 |
| [mobs-06](#mobs-06)<br>living mob描画 › slime › sizeと着地/空中squish | 公式Slime renderer/modelはentity sizeに応じて大きさを変え、着地・空中のsquishを反映し、透明outer shell layerを描く。 | Rustはsize metadataを保存し、fixed tickで着地/離地squishを更新する。entity_extrasが補間したsize/squishからbody transformを作り、inner bodyとtranslucent outer shellをproduction drawへ渡す。 | 実装あり | 未検証<br>静的照合 |
| [mobs-07](#mobs-07)<br>living mob描画 › shulker › 開閉peekと壁面向き | 公式はpeek量、attach face、染色textureをstateへ渡し、開閉shell姿勢と取り付け面に合わせた回転を描画する。 | Rustはface/peek/color metadataを保持し、peek量をtick補間してEntityRenderInfoへ渡す。Shulker animationとfaceに基づく本体transform、variant texture poolが本番MobDefにある。 | 実装あり | 未検証<br>静的照合 |
| [mobs-08](#mobs-08)<br>living mob描画 › bee › angry/nectar textureと飛行animation | 公式は怒り・花粉(nectar)状態で4種のtextureを選び、ground/flight状態やroll等をrender state/model animationに反映する。 | Rustはbee flagsとanger timeから怒り/nectarの組合せtexture indexを選び、Bee animation modelを使う。ただし描画animation関数の入力はageとon-groundに限られ、公式の状態animation一式との一致は確認できない。 | 一部実装 | 未検証<br>静的照合 |
| [mobs-09](#mobs-09)<br>living mob描画 › ghast › charging時texture切替 | 公式GhastRendererはisChargingに応じて通常textureとshooting textureを切り替える。 | RustはGhast metadata booleanを保存し、entity_extrasでvariant indexへ変換、MobDefに通常/ shooting texture poolとGhast animationを接続する。 | 実装あり | 未検証<br>静的照合 |

### 条件・残件・根拠

<a id="mobs-01"></a>

<details>
<summary>mobs-01 — living mob描画 › axolotl › variantと成体/子供テクスチャ</summary>

- 比較環境：クライアント共通
- 対象条件：成体/子供の5色variantおよび水中・地上・playing-dead状態
- 残件 / 比較すべき点：同期variantごとのテクスチャ選択を確認し、状態遷移時のモデル姿勢を公式と比較する。Rustにaxolotl状態animationの入力/計算がなく、公式の各状態animationとの差がある。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/AxolotlRenderer.java:34](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/AxolotlRenderer.java#L34) — getTextureLocation：state.variantとstate.isBabyからvariant別adult/baby textureを選択。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/AxolotlRenderer.java:43](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/AxolotlRenderer.java#L43) — extractRenderState：variantおよびplaying-dead/water/ground/moving animation stateを抽出。
  - Rust：[Client/pomme-client/src/entity/mod.rs:3400](../../pomme-client/src/entity/mod.rs#L3400) — EntityStore::apply_entity_data：Axolotl variant metadataを保存。
  - Rust：[Client/pomme-client/src/renderer/pipelines/entity_renderer.rs:1942](../../pomme-client/src/renderer/pipelines/entity_renderer.rs#L1942) — MobDef(Axolotl)：本番model定義でanimationがStatic。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:6123](../../pomme-client/src/app/phases/in_game.rs#L6123) — EntityRenderInfo extraction：living entityの本番描画情報を構築。

</details>

<a id="mobs-02"></a>

<details>
<summary>mobs-02 — living mob描画 › mooshroom › variant色と成体/子供</summary>

- 比較環境：クライアント共通
- 対象条件：赤/茶variantの成体および子供の背中・頭部キノコ
- 残件 / 比較すべき点：variantに結び付くblock-stateモデルのlayerを実装し、位置・姿勢・子供時の表示を公式と比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/MushroomCowRenderer.java:40](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/MushroomCowRenderer.java#L40) — MushroomCowRenderer constructor/extractRenderState：成体/子供cow modelおよびMushroomCowMushroomLayerを登録し、variant block stateをresolverへ渡す。
  - Rust：[Client/pomme-client/src/entity/mod.rs:3343](../../pomme-client/src/entity/mod.rs#L3343) — EntityStore::apply_entity_data：Mooshroom metadata variantを保持。
  - Rust：[Client/pomme-client/src/renderer/pipelines/entity_renderer.rs:2598](../../pomme-client/src/renderer/pipelines/entity_renderer.rs#L2598) — MobDef(Mooshroom)：赤/茶・成体/子供textureは設定されるがadult_overlaysとbaby_overlaysは空。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:6123](../../pomme-client/src/app/phases/in_game.rs#L6123) — EntityRenderInfo extraction：MobDefへ接続するliving本番描画データの生成箇所。

</details>

<a id="mobs-03"></a>

<details>
<summary>mobs-03 — living mob描画 › wolf › tame時の首輪と鎧layer</summary>

- 比較環境：クライアント共通
- 対象条件：tame wolfの成体/子供、collar dye、および装備中wolf armor
- 残件 / 比較すべき点：首輪色/子供textureの視覚比較と、equipment assetを用いた公式armor layerとの差を確認する。鎧表示はRust側TODO。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/WolfRenderer.java:16](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/WolfRenderer.java#L16) — WolfRenderer constructor：成体/子供モデルにWolfArmorLayerとWolfCollarLayerを登録。
  - Rust：[Client/pomme-client/src/entity/mod.rs:3419](../../pomme-client/src/entity/mod.rs#L3419) — EntityStore::apply_entity_data：wolfのtame/sitting flags、collar colorを保持。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:9590](../../pomme-client/src/app/phases/in_game.rs#L9590) — wolf_extras：tame時にdye collar tintを設定。
  - Rust：[Client/pomme-client/src/renderer/pipelines/entity_renderer.rs:1649](../../pomme-client/src/renderer/pipelines/entity_renderer.rs#L1649) — MobDef(Wolf)：成体/子供モデルと首輪overlayはあるがarmor layer TODO。

</details>

<a id="mobs-04"></a>

<details>
<summary>mobs-04 — living mob描画 › sheep › 毛刈り・羊毛色・jeb_名</summary>

- 比較環境：クライアント共通
- 対象条件：白以外の羊毛、sheared状態、成体/子供、およびjeb_名
- 残件 / 比較すべき点：羊毛layerの色・undercoat境界・虹色周期・毛刈り時の表示を同条件で視覚比較する。一致は未確認。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/SheepRenderer.java:16](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/SheepRenderer.java#L16) — SheepRenderer constructor/extractRenderState：成体/子供モデルとundercoat/wool layerを登録し、色とjeb_状態を抽出。
  - Rust：[Client/pomme-client/src/entity/mod.rs:3379](../../pomme-client/src/entity/mod.rs#L3379) — EntityStore::apply_entity_data：Sheep metadataから色nibbleとsheared bitを取り込む。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:9636](../../pomme-client/src/app/phases/in_game.rs#L9636) — sheep_extras：jeb_の時間色とsheared/babyに応じたoverlay tintを設定。
  - Rust：[Client/pomme-client/src/renderer/pipelines/entity_renderer.rs:1361](../../pomme-client/src/renderer/pipelines/entity_renderer.rs#L1361) — MobDef(Sheep)：成体/子供本体およびwool undercoat/wool overlayのproduction定義。

</details>

<a id="mobs-05"></a>

<details>
<summary>mobs-05 — living mob描画 › creeper › powered auraと爆発前膨張</summary>

- 比較環境：クライアント共通
- 対象条件：charged creeper、および爆発直前のswelling進行中
- 残件 / 比較すべき点：charged overlayの合成を比較し、爆発直前のswelling scale/点滅がRust側で欠落する差を確認・対応する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/CreeperRenderer.java:20](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/CreeperRenderer.java#L20) — CreeperRenderer.scale/getWhiteOverlayProgress/extractRenderState：swellingによるscaleと白点滅、powered状態を抽出。
  - Rust：[Client/pomme-client/src/entity/mod.rs:3383](../../pomme-client/src/entity/mod.rs#L3383) — EntityStore::apply_entity_data：powered flagは保持されるが該当部にswelling状態はない。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:9500](../../pomme-client/src/app/phases/in_game.rs#L9500) — entity_extras Creeper branch：powered時のみslot-0 aura tintを有効化。
  - Rust：[Client/pomme-client/src/renderer/pipelines/entity_renderer.rs:1527](../../pomme-client/src/renderer/pipelines/entity_renderer.rs#L1527) — MobDef(Creeper)：charged aura overlayは定義されるがanimation種別はQuadruped。

</details>

<a id="mobs-06"></a>

<details>
<summary>mobs-06 — living mob描画 › slime › sizeと着地/空中squish</summary>

- 比較環境：クライアント共通
- 対象条件：size 1以上のslimeが着地・跳躍する状態
- 残件 / 比較すべき点：サイズごとの比率、着地/離地squish位相、shellの透明度と位置を公式画像と比較する。一致は未確認。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/SlimeRenderer.java:14](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/SlimeRenderer.java#L14) — SlimeRenderer constructor/scale：slime modelとSlimeOuterLayerを登録しrenderer scaleを適用。
  - Rust：[Client/pomme-client/src/entity/mod.rs:3334](../../pomme-client/src/entity/mod.rs#L3334) — EntityStore::apply_entity_data/tick_squish：size metadataおよびcube mobのsquish spring更新。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:9500](../../pomme-client/src/app/phases/in_game.rs#L9500) — entity_extras/slime_body_transform：補間squishとsizeから本体transformを生成。
  - Rust：[Client/pomme-client/src/renderer/pipelines/entity_renderer.rs:1624](../../pomme-client/src/renderer/pipelines/entity_renderer.rs#L1624) — MobDef(Slime)：inner bodyとBodyTranslucent outer overlayを登録。

</details>

<a id="mobs-07"></a>

<details>
<summary>mobs-07 — living mob描画 › shulker › 開閉peekと壁面向き</summary>

- 比較環境：クライアント共通
- 対象条件：peek 0〜最大値、6方向のattach face、染色shulker
- 残件 / 比較すべき点：各面の座標系・shell開閉量・染色textureを画像比較する。公式の移動中render offset/culling boundsまで等価かは今回未照合。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/ShulkerRenderer.java:29](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/ShulkerRenderer.java#L29) — ShulkerRenderer.extractRenderState/setupRotations：render offset、color、peek量、attach faceを抽出し、attach faceに応じて回転。
  - Rust：[Client/pomme-client/src/entity/mod.rs:3357](../../pomme-client/src/entity/mod.rs#L3357) — EntityStore::apply_entity_data/tick_living：face、peek、color metadataとpeek interpolationを保持。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:6140](../../pomme-client/src/app/phases/in_game.rs#L6140) — EntityRenderInfo extraction：補間peek量とattach faceをproduction render infoへ渡す。
  - Rust：[Client/pomme-client/src/renderer/pipelines/entity_renderer.rs:2631](../../pomme-client/src/renderer/pipelines/entity_renderer.rs#L2631) — MobDef(Shulker)/entity_matrix：variant textureとShulker animation、本体face transformの描画経路。

</details>

<a id="mobs-08"></a>

<details>
<summary>mobs-08 — living mob描画 › bee › angry/nectar textureと飛行animation</summary>

- 比較環境：クライアント共通
- 対象条件：地上/飛行 × calm/angry × nectar有無
- 残件 / 比較すべき点：4状態texture選択とroll・wing/body poseを条件ごとに公式と比較する。特に怒り/nectar状態がRustのpose入力へ必要かを確認する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/BeeRenderer.java:16](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/BeeRenderer.java#L16) — BeeRenderer.extractRenderState/getTextureLocation：状態を抽出しnormal/angry/nectar組合せtextureを選択。
  - Rust：[Client/pomme-client/src/entity/mod.rs:3349](../../pomme-client/src/entity/mod.rs#L3349) — EntityStore::apply_entity_data：bee flagsおよびanger end timeを保存。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:9275](../../pomme-client/src/app/phases/in_game.rs#L9275) — entity_extras Bee branch：anger timeとnectar flagから4通りのvariant indexを生成。
  - Rust：[Client/pomme-client/src/renderer/pipelines/entity_renderer.rs:2506](../../pomme-client/src/renderer/pipelines/entity_renderer.rs#L2506) — MobDef(Bee)/compute_anim：Bee texture poolとBee model animationをproduction drawで使用。

</details>

<a id="mobs-09"></a>

<details>
<summary>mobs-09 — living mob描画 › ghast › charging時texture切替</summary>

- 比較環境：クライアント共通
- 対象条件：ghast charging false/trueの遷移中
- 残件 / 比較すべき点：同期charging値が切替タイミングと合うか、選ばれる両textureおよびtentacle poseを同時刻で比較する。一致は未確認。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/GhastRenderer.java:10](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/GhastRenderer.java#L10) — GhastRenderer.getTextureLocation/extractRenderState：charging時だけshooting textureを選択。
  - Rust：[Client/pomme-client/src/entity/mod.rs:3354](../../pomme-client/src/entity/mod.rs#L3354) — EntityStore::apply_entity_data：Ghast index 16 booleanをghast_chargingへ保存。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:9270](../../pomme-client/src/app/phases/in_game.rs#L9270) — entity_extras Ghast branch：charging flagをvariant indexに変換。
  - Rust：[Client/pomme-client/src/renderer/pipelines/entity_renderer.rs:2557](../../pomme-client/src/renderer/pipelines/entity_renderer.rs#L2557) — MobDef(Ghast)/compute_anim：通常/shooting texture poolとGhast animationを登録。

</details>


<a id="domain-nonliving"></a>

## 乗り物・非生物エンティティ

調査範囲：EntityStoreの非living車両状態・metadata・乗客更新、projectile予測、in_gameの車両/非living描画抽出、および指定renderer群を照合。公式26.2のentity/rendererソースも照合した。非living全種類・全metadataや全描画モデルの網羅はしていない。

- 制約：指定された作業は静的照合のみ。ビルド・cargo test・アプリ実行・公式サーバー接続・実機比較はしていない。
- 制約：乗り物の物理移動・権威的なboat/minecart挙動はサーバー側にあるため、Rust未実装と判定していない。
- 制約：projectile予測は明示的に表示専用で、壁衝突/水中横断の近似差がある。Dropped itemとplayer skinは調査対象外。

| ID / 機能 | 公式の挙動 | Rustの現状・差分 | 実装 | 比較 / 検証 |
|---|---|---|---|---|
| [nonliving-01](#nonliving-01)<br>projectile › 飛翔体 › 表示軌道予測 | 公式クライアントの投射物表示・位置補間は未照合。参考に読んだ矢のentity tickは衝突・減衰・重力を処理するが、表示経路の比較根拠とは区別する。 | Rustは矢等の表示用軌道を最大64 tick予測するが、衝突を解かない表示専用計算であり、サーバー位置は権威情報として別保持する。水中を飛行中に横断しても開始時のdragを維持する。 | 一部実装 | 未検証<br>Rust静的確認のみ |
| [nonliving-02](#nonliving-02)<br>乗り物 › Boat/raft › 損傷・櫂・水中状態の表示 | 公式BoatRendererは補間済みyaw、hurt time/direction、damage、bubble angle、underwater状態、左右のrowing timeをrender stateへ渡す。 | Rustはboat各種stateをvehicleに保持し、前後位置/yawとrowing timeを補間し、水面下のfluidを走査してboat描画情報を生成する。公式と同じrenderer state全項目・演算であることまでは確認していない。 | 実装あり | 未検証<br>静的照合 |
| [nonliving-03](#nonliving-03)<br>乗り物 › Minecart › 傷み・表示ブロック付き描画 | 公式AbstractMinecartRendererはcartの移動behaviorに応じた補間位置/回転を抽出し、hurt stateとdisplay block state/offsetも描画状態に設定する。 | Rustはminecart種別の前後位置・yaw/pitchを補間し、損傷揺れ等をEntityRenderInfoに含める。minecart display state/offsetとfurnace状態も保持するが、公式の新旧rail behavior由来の軌道補間との同等性は確認できない。 | 一部実装 | 未検証<br>静的照合 |
| [nonliving-04](#nonliving-04)<br>乗り物 › SetPassengers › 乗客関係と追従位置 | 公式のSetPassengersでvehicle/ordered passenger関係を更新し、車両と乗客は乗車attachment位置に従う。 | RustはSetPassengersから順序付きpassengersと逆引きvehicle_ofを更新し、移動/描画側からpassenger_positionでvehicle attachment位置を求める。接続中LocalPlayerの制御対象は先頭passengerとして別判定する。 | 一部実装 | 未検証<br>Rust静的確認のみ |
| [nonliving-05](#nonliving-05)<br>爆発物 › Primed TNT › fuse・点滅・膨張表示 | 公式PrimedTntは初期fuse 80とblock stateを同期し、tickごとにfuseを減らし、TntRendererで残りfuseに応じた膨張/白点滅を描画する。 | RustはTNT metadataのfuse/block stateを保持し、tick間のfuseを補間して膨張・白overlayを計算し、TNT block modelを描画する。サーバーの爆発/権威処理は担当外。 | 実装あり | 未検証<br>静的照合 |
| [nonliving-06](#nonliving-06)<br>落下物 › FallingBlock › 移動block state描画 | 公式FallingBlockRendererはentityのstart positionと現在block stateをrender stateに渡し、移動blockを現在の位置/環境で描画する。 | Rust native protocolではspawn-data block stateを解釈し、metadata start positionも保持する。描画時は補間位置のblock sampleが同じstateまたは不可視/airなら描画を抑止する。 | 一部実装 | 未検証<br>静的照合 |
| [nonliving-07](#nonliving-07)<br>pickup entity › Experience Orb › 値別テクスチャ・発光色 | 公式ExperienceOrbは同期値を持ち、ExperienceOrbRendererが値からsprite、年齢と明るさに応じた色、camera-facing姿勢を描画する。 | Rustはorb値とclient visual ageを保持し、値からsprite variant、age/lightからtintを作り、位置を補間して描画する。 | 実装あり | 未検証<br>静的照合 |
| [nonliving-08](#nonliving-08)<br>entity › End Crystal › 回転・beam target・台座 | 公式EndCrystalはbeam targetとshow-bottom metadataを持ち、tickで年齢を進め、rendererがbeam targetを描画状態へ渡す。 | Rustはbeam target・show-bottom・visual ageを保持し、ageとtarget offsetを補間してEndCrystal描画情報を作る。 | 実装あり | 未検証<br>静的照合 |
| [nonliving-09](#nonliving-09)<br>Display entity › ItemDisplay › item/context/transform表示 | 公式Displayはtranslation/scale/quaternion/billboard等を同期し、client tickでtransform interpolationを更新する。ItemDisplayはitem stackとdisplay contextに従って描画する。 | RustはItemDisplay stack/contextと共通Display transformをmetadataから保持し、world位置を補間してitem meshを描画する。確認した抽出箇所ではtransform値の時間補間ではなく保持値を適用している。 | 一部実装 | 差分あり<br>静的照合 |
| [nonliving-10](#nonliving-10)<br>Display entity › BlockDisplay/TextDisplay › block・文字表示 | 公式Displayは同期transform/billboardと補間を扱い、各display subclassがblockまたはtextをrender stateに反映する。 | RustはBlockDisplay stateとTextDisplay text/style/transformを保持し、block displayをitem/block pipelineに、text displayをcamera-aware world-space glyph/background geometryに接続する。 | 一部実装 | 未検証<br>静的照合 |

### 条件・残件・根拠

<a id="nonliving-01"></a>

<details>
<summary>nonliving-01 — projectile › 飛翔体 › 表示軌道予測</summary>

- 比較環境：クライアント共通
- 対象条件：矢/投擲物が壁または水中を通る場合の表示軌道
- 残件 / 比較すべき点：公式クライアントの投射物表示・位置補間経路を特定し、packet位置更新間の表示を壁・水入り・着弾条件で比較する。サーバー権威の衝突処理をクライアント未実装とみなさない。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/entity/projectile/arrow/AbstractArrow.java:223](../../../minecraft-26.2-decompiled/src/net/minecraft/world/entity/projectile/arrow/AbstractArrow.java#L223) — AbstractArrow.tick：ブロックclipの後に移動し、air dragと重力を適用する。
  - Rust：[Client/pomme-client/src/entity/projectile.rs:1](../../pomme-client/src/entity/projectile.rs#L1) — projectile module / step：表示専用・衝突なし予測と予測順序を明記。
  - Rust：[Client/pomme-client/src/entity/mod.rs:2600](../../pomme-client/src/entity/mod.rs#L2600) — EntityStore::tick_projectile_displays：固定tickで予測trajectoryを更新する。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:6404](../../pomme-client/src/app/phases/in_game.rs#L6404) — in-game entity render extraction：projectile表示情報を本番描画リストへ加える。

</details>

<a id="nonliving-02"></a>

<details>
<summary>nonliving-02 — 乗り物 › Boat/raft › 損傷・櫂・水中状態の表示</summary>

- 比較環境：クライアント共通
- 対象条件：各種boat/raftで損傷中または櫂を漕ぎ、水中にも入る
- 残件 / 比較すべき点：hurt rocking・bubble angle・underwater判定・櫂周期を同じ状態で公式と比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/AbstractBoatRenderer.java:56](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/AbstractBoatRenderer.java#L56) — AbstractBoatRenderer.extractRenderState：公式boat描画が収集する補間/状態値。
  - Rust：[Client/pomme-client/src/entity/mod.rs:1643](../../pomme-client/src/entity/mod.rs#L1643) — VehicleState boat fields：damage/paddle/bubble/rowingの状態を保持する。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:7365](../../pomme-client/src/app/phases/in_game.rs#L7365) — boat_render_infos：boat種別を抽出し、位置/yaw補間とfluid状態を求める。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:6411](../../pomme-client/src/app/phases/in_game.rs#L6411) — in-game entity render extraction：boat render infosを本番描画へ接続する。

</details>

<a id="nonliving-03"></a>

<details>
<summary>nonliving-03 — 乗り物 › Minecart › 傷み・表示ブロック付き描画</summary>

- 比較環境：クライアント共通
- 対象条件：通常/貨物/TNT等のminecartがrail上を曲がるか、display blockを持つ
- 残件 / 比較すべき点：rail沿いの新旧behavior補間、display block/offset及び各派生cartの専用描画を公式と比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/AbstractMinecartRenderer.java:100](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/AbstractMinecartRenderer.java#L100) — AbstractMinecartRenderer.extractRenderState：behavior別の状態抽出とdamage/display block処理。
  - Rust：[Client/pomme-client/src/entity/mod.rs:1659](../../pomme-client/src/entity/mod.rs#L1659) — VehicleState minecart fields：display state/offset/furnace fuel状態を保持。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:8044](../../pomme-client/src/app/phases/in_game.rs#L8044) — minecart_render_infos：minecart各派生と補間姿勢を描画情報化する。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:6416](../../pomme-client/src/app/phases/in_game.rs#L6416) — in-game entity render extraction：minecart render infosを描画経路へ加える。

</details>

<a id="nonliving-04"></a>

<details>
<summary>nonliving-04 — 乗り物 › SetPassengers › 乗客関係と追従位置</summary>

- 比較環境：公式サーバー接続
- 対象条件：複数passengerの乗車・乗り換え・降車と非living vehicle追従
- 残件 / 比較すべき点：公式packet/entity attachment offsets、boat/minecart固有乗車位置及び乗客順序変更時の描画/制御対象を比較する。
- 根拠：
  - Rust：[Client/pomme-client/src/entity/mod.rs:1897](../../pomme-client/src/entity/mod.rs#L1897) — EntityStore::set_passengers：順序を保ち古い車両から関係を除去して逆引きを更新。
  - Rust：[Client/pomme-client/src/entity/mod.rs:3140](../../pomme-client/src/entity/mod.rs#L3140) — EntityStore::passenger_position：vehicle位置+attachment-offsetで乗客位置を計算。
  - Rust：[Client/pomme-client/src/app/core.rs:3061](../../pomme-client/src/app/core.rs#L3061) — apply_passengers：本番SetPassengers eventからEntityStoreへ反映。
  - Rust：[Client/pomme-client/src/app/core.rs:4430](../../pomme-client/src/app/core.rs#L4430) — NetworkEvent::SetPassengers handler：受信イベントで乗車/制御IDを更新。

</details>

<a id="nonliving-05"></a>

<details>
<summary>nonliving-05 — 爆発物 › Primed TNT › fuse・点滅・膨張表示</summary>

- 比較環境：クライアント共通
- 対象条件：primed TNTのfuse残り10 tick以下から爆発packetまで
- 残件 / 比較すべき点：公式のframe partial fuse、点滅周期、scale及びblock state差し替えを比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/entity/item/PrimedTnt.java:91](../../../minecraft-26.2-decompiled/src/net/minecraft/world/entity/item/PrimedTnt.java#L91) — PrimedTnt.defineSynchedData / tick：fuse/block state初期値とtick減算・爆発条件。
  - Rust：[Client/pomme-client/src/entity/mod.rs:2200](../../pomme-client/src/entity/mod.rs#L2200) — EntityStore::apply_entity_data vehicle metadata：fuseとblock state metadataを保持する。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:3609](../../pomme-client/src/app/phases/in_game.rs#L3609) — tnt_render_effect：fuseから膨張と点滅overlayを算出。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:6727](../../pomme-client/src/app/phases/in_game.rs#L6727) — TNT render extraction：補間fuseを用いてblock modelを描画する。

</details>

<a id="nonliving-06"></a>

<details>
<summary>nonliving-06 — 落下物 › FallingBlock › 移動block state描画</summary>

- 比較環境：クライアント共通
- 対象条件：異なるblock stateの上を落下するfalling block
- 残件 / 比較すべき点：biome/cardinal lighting、bounding-box上面でのsample位置、同blockに着地した時の非表示条件を比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/FallingBlockRenderer.java:41](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/FallingBlockRenderer.java#L41) — FallingBlockRenderer.extractRenderState：start position/current block stateと描画位置を取得。
  - Rust：[Client/pomme-client/src/entity/mod.rs:1564](../../pomme-client/src/entity/mod.rs#L1564) — decode_falling_block_state：native protocolのAddEntity stateを限定してblock state化。
  - Rust：[Client/pomme-client/src/entity/mod.rs:2207](../../pomme-client/src/entity/mod.rs#L2207) — FallingBlock metadata handling：開始位置metadataを保持する。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:6577](../../pomme-client/src/app/phases/in_game.rs#L6577) — FallingBlock render extraction：位置補間、状態条件、block model描画に接続。

</details>

<a id="nonliving-07"></a>

<details>
<summary>nonliving-07 — pickup entity › Experience Orb › 値別テクスチャ・発光色</summary>

- 比較環境：クライアント共通
- 対象条件：異なる経験値を持つorbを複数dimensionの光量で表示
- 残件 / 比較すべき点：値境界のsprite選択、age tint/light計算、billboard姿勢を公式rendererと比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/entity/ExperienceOrb.java:98](../../../minecraft-26.2-decompiled/src/net/minecraft/world/entity/ExperienceOrb.java#L98) — ExperienceOrb.defineSynchedData / getValue：orb value metadataの保持と読み出し。
  - Rust：[Client/pomme-client/src/entity/mod.rs:2216](../../pomme-client/src/entity/mod.rs#L2216) — ExperienceOrb metadata handling：metadata index 8のvalueをvehicle stateへ保存。
  - Rust：[Client/pomme-client/src/entity/mod.rs:2607](../../pomme-client/src/entity/mod.rs#L2607) — EntityStore::tick_projectile_displays：orb visual ageをtick更新。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:7934](../../pomme-client/src/app/phases/in_game.rs#L7934) — experience_orb_render_infos：sprite/color/lightと補間位置を本番描画情報にする。

</details>

<a id="nonliving-08"></a>

<details>
<summary>nonliving-08 — entity › End Crystal › 回転・beam target・台座</summary>

- 比較環境：クライアント共通
- 対象条件：beam targetの有無および台座表示を切り替えたcrystal
- 残件 / 比較すべき点：beam geometry/animation、台座フラグ反映、公式age-based rotationを描画で比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/entity/boss/enderdragon/EndCrystal.java:54](../../../minecraft-26.2-decompiled/src/net/minecraft/world/entity/boss/enderdragon/EndCrystal.java#L54) — EndCrystal.defineSynchedData / tick：同期beam target/show bottomとtick年齢。
  - Rust：[Client/pomme-client/src/entity/mod.rs:2210](../../pomme-client/src/entity/mod.rs#L2210) — EndCrystal metadata handling：beam targetと台座表示metadataを保持する。
  - Rust：[Client/pomme-client/src/entity/mod.rs:2604](../../pomme-client/src/entity/mod.rs#L2604) — EntityStore::tick_projectile_displays：crystal visual ageをtick更新。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:7701](../../pomme-client/src/app/phases/in_game.rs#L7701) — end_crystal_render_infos：位置・age・beam targetを描画情報化する。

</details>

<a id="nonliving-09"></a>

<details>
<summary>nonliving-09 — Display entity › ItemDisplay › item/context/transform表示</summary>

- 比較環境：クライアント共通
- 対象条件：item displayのstack/context/transformをtick interpolation付きで変更
- 残件 / 比較すべき点：公式のtransform interpolation delay/durationに対しRustが値変更を即時反映するか、item display contextごとの姿勢とpackモデルを比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/entity/Display.java:154](../../../minecraft-26.2-decompiled/src/net/minecraft/world/entity/Display.java#L154) — Display.tick：同期transform更新時にclient側interpolation/render stateを更新。
  - Rust：[Client/pomme-client/src/entity/mod.rs:1695](../../pomme-client/src/entity/mod.rs#L1695) — VehicleState display fields：共通Display値とItemDisplay stack/contextを状態保持。
  - Rust：[Client/pomme-client/src/entity/mod.rs:2300](../../pomme-client/src/entity/mod.rs#L2300) — EntityStore::set_item_display_metadata：stack/context metadataを種類別に適用。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:6641](../../pomme-client/src/app/phases/in_game.rs#L6641) — ItemDisplay render extraction：stackとtransformを用いて描画に接続。

</details>

<a id="nonliving-10"></a>

<details>
<summary>nonliving-10 — Display entity › BlockDisplay/TextDisplay › block・文字表示</summary>

- 比較環境：クライアント共通
- 対象条件：block displayの変形、およびtext displayのbillboard/alignment/background/opacity
- 残件 / 比較すべき点：公式Display interpolation、block lighting/brightness override、text wrapping・line width・背景とsee-through/shadowの細部を比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/entity/Display.java:187](../../../minecraft-26.2-decompiled/src/net/minecraft/world/entity/Display.java#L187) — Display.defineSynchedData：公式共通transform/billboard/view range等の同期項目。
  - Rust：[Client/pomme-client/src/entity/mod.rs:1701](../../pomme-client/src/entity/mod.rs#L1701) — VehicleState TextDisplay fields：text, style, transform, billboard/view-range状態を保存。
  - Rust：[Client/pomme-client/src/renderer/pipelines/text_display.rs:92](../../pomme-client/src/renderer/pipelines/text_display.rs#L92) — text_matrix：billboardとtranslation/rotation/scaleをworld-space matrixへ適用。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:6492](../../pomme-client/src/app/phases/in_game.rs#L6492) — BlockDisplay render extraction：BlockDisplay metadataをblock model描画へ接続する。

</details>


<a id="domain-blockentities"></a>

## ブロックエンティティ

調査範囲：Pommeのworld/block_entity.rs、block_entity_anim.rs、renderer/block_entity_model.rs、placed_head_skin.rs、renderer/pipelines/block_entity と sign_text、in_game.rsの抽出・呼出経路を調査し、公式26.2の対応BlockEntityRendererと関連block entityを照合。container UIとparticle処理、一覧にないblock entityの全面調査は対象外。

- 制約：静的ソース照合のみで、公式クライアントとの実機比較・Rustテスト実行はしていない。
- 制約：Container UIとparticle描画は別担当のため、今回の挙動・判定に含めない。Conduitのtargetを用いる攻撃/particle処理もこの描画調査では扱わない。
- 制約：Rustはblock entity NBTのparseを行っていても、Banner patternsのように実際の描画利用へ接続されていない状態がある。

| ID / 機能 | 公式の挙動 | Rustの現状・差分 | 実装 | 比較 / 検証 |
|---|---|---|---|---|
| [blockentities-01](#blockentities-01)<br>block entity › chest/trapped/ender/copper chest › stateに応じた外観と開閉 | 公式はblock stateの向き・single/left/rightとchest種別からモデル・素材を選び、開閉状態を隣接半分と合成して補間し、ふたをイージングして描画する。 | Rustはblock stateからvariant/yawを選び、viewer数由来の開閉値を補間し、double chestでは相方の最大値を用いてふた・錠を回転する。公式はdouble chestの明るさも合成するが、Rust側の同等処理は今回確認できず。 | 実装あり | 未検証<br>静的照合 |
| [blockentities-02](#blockentities-02)<br>block entity › sign/hanging sign › 表裏の文字描画 | 公式はfront/backそれぞれのSignTextを整形テキストとして行幅に合わせ、染料色・発光・暗色アウトライン・照明を適用して両面に描画する。 | RustはNBTからfront/backの4行を抽出し、両面の位置・色・発光・明るさを反映し描画する。ただし表示データはplain text化され、公式の装飾付きFormattedCharSequenceやtext filtering相当はこの描画経路に渡っていない。 | 一部実装 | 差分あり<br>静的照合 |
| [blockentities-03](#blockentities-03)<br>block entity › banner/wall banner › 基底色・文様レイヤーの描画 | 公式はblock entityからbase color・patternsを取り出し、attachment別モデルと旗の揺れに加え、base色と最大16個の文様レイヤーを描く。 | RustはpatternsをNBTから読み取るが最大6個であり、その保存値は描画情報やblock_entity pipelineで利用されない。描画経路はstanding/wallの旗モデル・基底banner texture・揺れまでで、NBT文様は描かれない。 | 一部実装 | 差分あり<br>静的照合 |
| [blockentities-04](#blockentities-04)<br>block entity › skull/head › 種類・向き・プレイヤースキン | 公式はblock stateのstanding rotation/wall facingとhead typeから専用モデル・textureを選び、block entityのanimation stateとprofileを反映して描画する。 | Rustはskull種別ごとのmodel/texture、standing/wall variantと回転を描画し、player head profileからskin取得・resource patchを非同期cache経由で解決する。Rust経路のprofile取得・skin network失敗時表示は公式と同条件確認していない。 | 実装あり | 未検証<br>静的照合 |
| [blockentities-05](#blockentities-05)<br>block entity › conduit › 非稼働/稼働状態のモデル・アニメーション | 公式はisActive等のblock entity runtime stateから非稼働shell、または稼働時の回転cage・wind・eye（hunting stateによるeye texture）を選び、時間位相を反映する。 | RustはNBTのTarget UUIDをvisual stateにparseするが、block entity pipelineのConduit定義はbase textureのbaked model一つで、公式のactive/inactive状態、cage/wind/eye描画の接続は確認できない。 | 一部実装 | 差分あり<br>静的照合 |
| [blockentities-06](#blockentities-06)<br>block entity › end portal/gateway › 面選択とportal描画 | 公式はEnd Portal/End Gateway rendererでblock周囲の可視面だけをportal/gateway render typeで描く。 | RustはEndPortalに固定面mask、EndGatewayに隣接block遮蔽から求めたface maskを作り、gatewayはNBT Ageも描画情報に渡す。end portal pipelineの専用描画経路がある。 | 実装あり | 未検証<br>静的照合 |
| [blockentities-07](#blockentities-07)<br>block entity › decorated pot › decorationsとwobble | 公式はblock entity decorations・direction・wobble style/timeを抽出し、側面ごとの装飾を描画し、正/負スタイルごとの揺れを適用する。 | RustはNBTのsherdsを4面分へparseし面別textureを選び、イベント由来のwobble progressを描画matrixに適用する。 | 実装あり | 未検証<br>静的照合 |
| [blockentities-08](#blockentities-08)<br>block entity › enchanting table › 本の開閉・ページ・向きアニメーション | 公式は補間したflip/open/time/yawからBookModelのページめくり・開閉・回転状態を作り、浮遊する本を描画する。 | RustはEnchantingBookStateをblock entityに保持してtick/interpolateし、pipelineがbook stateのflip/openをPartAnimへ反映し本modelを描画する。 | 実装あり | 未検証<br>静的照合 |
| [blockentities-09](#blockentities-09)<br>block entity › beacon › beam section描画 | 公式はBeaconBeamOwnerのbeam sectionsから色・高さを抽出し、アニメーション時刻を用いて各区間の不透明beam/glowを描画する。 | Rustのrendered_kindはbeaconを描画対象に含めず、is_renderedの登録種別にもBeaconがないため、このblock entity pipelineではbeacon beam描画へ到達しない。 | 未実装 | 差分あり<br>静的照合 |

### 条件・残件・根拠

<a id="blockentities-01"></a>

<details>
<summary>blockentities-01 — block entity › chest/trapped/ender/copper chest › stateに応じた外観と開閉</summary>

- 比較環境：公式サーバー接続
- 対象条件：各chest種別とsingle/doubleを開閉し、向き・テクスチャ・アニメーションを比較する
- 残件 / 比較すべき点：相方側だけを開いた場合の両ふた、double chestの光量、銅の酸化段階・季節テクスチャを公式と同条件で比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/blockentity/ChestRenderer.java:73](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/blockentity/ChestRenderer.java#L73) — extractRenderState：block stateからtype/facing/materialを決め、opennessを抽出。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/blockentity/ChestRenderer.java:89](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/blockentity/ChestRenderer.java#L89) — submit：向き・イージング・素材/型別モデルで描画。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:6851](../../pomme-client/src/app/phases/in_game.rs#L6851) — block entity render extraction：相方chest opennessとのmax合成とvariant/yawを描画情報へ伝搬。
  - Rust：[Client/pomme-client/src/renderer/pipelines/block_entity.rs:390](../../pomme-client/src/renderer/pipelines/block_entity.rs#L390) — lid_anim：chestふたと錠の回転、chest種別別モデル・素材の定義。

</details>

<a id="blockentities-02"></a>

<details>
<summary>blockentities-02 — block entity › sign/hanging sign › 表裏の文字描画</summary>

- 比較環境：クライアント共通
- 対象条件：通常/吊り看板の表裏に装飾付き・複数行テキストを設定し、発光色と照明を比較する
- 残件 / 比較すべき点：テキスト装飾・filtering、長文の折り返し/行数制限、glowing outlineの適用距離を比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/blockentity/AbstractSignRenderer.java:32](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/blockentity/AbstractSignRenderer.java#L32) — submit：front/back各面を別transformで描画。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/blockentity/AbstractSignRenderer.java:48](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/blockentity/AbstractSignRenderer.java#L48) — submitSignText：整形テキスト、text filtering、染料色・発光・照明を用いる。
  - Rust：[Client/pomme-client/src/world/block_entity.rs:334](../../pomme-client/src/world/block_entity.rs#L334) — StoredBlockEntity::new/update_nbt：看板NBTからfront/backの行を文字列として保持。
  - Rust：[Client/pomme-client/src/renderer/pipelines/block_entity/sign_text.rs:126](../../pomme-client/src/renderer/pipelines/block_entity/sign_text.rs#L126) — draw_sign_text：看板文字列をglyph vertex化して描画する。

</details>

<a id="blockentities-03"></a>

<details>
<summary>blockentities-03 — block entity › banner/wall banner › 基底色・文様レイヤーの描画</summary>

- 比較環境：クライアント共通
- 対象条件：文様なし/あり（7層以上を含む）の地上旗と壁掛け旗を比較する
- 残件 / 比較すべき点：NBT由来の色・文様が見た目へ反映されるか、通常/壁モデルの姿勢と揺れを比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/blockentity/BannerRenderer.java:81](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/blockentity/BannerRenderer.java#L81) — extractRenderState：baseColor・patterns・attachment・phaseをrender stateへ抽出する。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/blockentity/BannerRenderer.java:174](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/blockentity/BannerRenderer.java#L174) — submitBanner/submitPatterns：旗・揺れる布とbase色を描き、最大16層のpatternをsubmitする。
  - Rust：[Client/pomme-client/src/world/block_entity.rs:470](../../pomme-client/src/world/block_entity.rs#L470) — banner_patterns：pattern registry key/asset idと色をparseし、6層で打ち切る。
  - Rust：[Client/pomme-client/src/renderer/pipelines/block_entity.rs:609](../../pomme-client/src/renderer/pipelines/block_entity.rs#L609) — kind_definitions：旗モデルと基底banner textureは設定するがpatternsを使う描画項目は存在しない。

</details>

<a id="blockentities-04"></a>

<details>
<summary>blockentities-04 — block entity › skull/head › 種類・向き・プレイヤースキン</summary>

- 比較環境：クライアント共通
- 対象条件：各skull/head種別のstanding/wall向きと、skin property付き/なしplayer headを比較する
- 残件 / 比較すべき点：公式でのprofile解決、Alex/Steve形状、animation、未解決時fallbackを実機で比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/blockentity/SkullBlockRenderer.java:91](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/blockentity/SkullBlockRenderer.java#L91) — extractRenderState：block stateからstanding/wall transform、skull type、render typeを抽出する。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/blockentity/SkullBlockRenderer.java:105](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/blockentity/SkullBlockRenderer.java#L105) — submit：type別modelでanimation値を使いskullを描画する。
  - Rust：[Client/pomme-client/src/renderer/pipelines/block_entity.rs:619](../../pomme-client/src/renderer/pipelines/block_entity.rs#L619) — kind_definitions：skull model群・texture variantsを描画定義へ登録する。
  - Rust：[Client/pomme-client/src/renderer/placed_head_skin.rs:199](../../pomme-client/src/renderer/placed_head_skin.rs#L199) — PlacedHeadSkinCache::resolve：profile/propertyまたはdynamic name/idをskin dataへ解決する。

</details>

<a id="blockentities-05"></a>

<details>
<summary>blockentities-05 — block entity › conduit › 非稼働/稼働状態のモデル・アニメーション</summary>

- 比較環境：クライアント共通
- 対象条件：稼働条件を満たさないconduitと稼働中/hunting中を比較する
- 残件 / 比較すべき点：稼働時の各可動部・eye texture・回転/浮遊、非稼働時のshellを描画するか比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/blockentity/ConduitRenderer.java:94](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/blockentity/ConduitRenderer.java#L94) — extractRenderState：active、rotation、animation phase、hunting状態を抽出する。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/blockentity/ConduitRenderer.java:102](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/blockentity/ConduitRenderer.java#L102) — submit：状態に応じshellまたはcage/wind/eyeを描き分ける。
  - Rust：[Client/pomme-client/src/renderer/pipelines/block_entity.rs:585](../../pomme-client/src/renderer/pipelines/block_entity.rs#L585) — kind_definitions：Conduitに単一baked modelとconduit/base textureを割り当てる。
  - Rust：[Client/pomme-client/src/world/block_entity.rs:360](../../pomme-client/src/world/block_entity.rs#L360) — StoredBlockEntity::new：NBT Target UUIDのみconduit visual stateへ設定する。

</details>

<a id="blockentities-06"></a>

<details>
<summary>blockentities-06 — block entity › end portal/gateway › 面選択とportal描画</summary>

- 比較環境：クライアント共通
- 対象条件：portal/gatewayの露出面・遮蔽面およびAge条件を比較する
- 残件 / 比較すべき点：公式と面の可視性、gatewayのAgeに応じる見た目、dimension/textureを同条件で比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/blockentity/TheEndPortalRenderer.java:21](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/blockentity/TheEndPortalRenderer.java#L21) — submit：facesToShowを用いend portal cubeを描画する。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/blockentity/AbstractEndPortalRenderer.java:45](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/blockentity/AbstractEndPortalRenderer.java#L45) — extractRenderState：遮られていないblock面をfacesToShowへ集める。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:6891](../../pomme-client/src/app/phases/in_game.rs#L6891) — block entity render extraction：end portal/gatewayのface maskを作りgateway Ageを取得する。
  - Rust：[Client/pomme-client/src/renderer/pipelines/end_portal.rs:19](../../pomme-client/src/renderer/pipelines/end_portal.rs#L19) — EndPortalDraw：gateway flag・face mask・ageをportal rendererへ渡す状態を定義する。

</details>

<a id="blockentities-07"></a>

<details>
<summary>blockentities-07 — block entity › decorated pot › decorationsとwobble</summary>

- 比較環境：公式サーバー接続
- 対象条件：4面に異なるsherdがあるpotとpositive/negative hit wobbleを比較する
- 残件 / 比較すべき点：公式と4面の向き・texture対応、wobble期間・振幅・回転軌跡を同一イベントで比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/blockentity/DecoratedPotRenderer.java:151](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/blockentity/DecoratedPotRenderer.java#L151) — extractRenderState：decorations・direction・wobble progressを抽出する。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/blockentity/DecoratedPotRenderer.java:162](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/blockentity/DecoratedPotRenderer.java#L162) — submit：方向transform、wobble styleに応じた回転と装飾を描画する。
  - Rust：[Client/pomme-client/src/world/block_entity.rs:345](../../pomme-client/src/world/block_entity.rs#L345) — StoredBlockEntity::new/update_nbt：NBT sherdsを保持し、pot wobble stateを更新する。
  - Rust：[Client/pomme-client/src/renderer/pipelines/block_entity.rs:306](../../pomme-client/src/renderer/pipelines/block_entity.rs#L306) — decorated_pot_part_texture：各面ごとのsherd texture variantを選択する。

</details>

<a id="blockentities-08"></a>

<details>
<summary>blockentities-08 — block entity › enchanting table › 本の開閉・ページ・向きアニメーション</summary>

- 比較環境：クライアント共通
- 対象条件：近くにplayerがいる/いない時の本の開閉、ページ移動、向きの変化を比較する
- 残件 / 比較すべき点：公式の乱数更新・player距離/向き応答・補間を一定時間観察し、本の動きを比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/blockentity/EnchantTableRenderer.java:41](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/blockentity/EnchantTableRenderer.java#L41) — extractRenderState：flip/open/time/yawをpartial tickで補間する。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/blockentity/EnchantTableRenderer.java:61](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/blockentity/EnchantTableRenderer.java#L61) — submit：本transformとBookModel.Stateで本を描画する。
  - Rust：[Client/pomme-client/src/world/block_entity.rs:355](../../pomme-client/src/world/block_entity.rs#L355) — StoredBlockEntity::new：enchanting table用book animation stateを所有する。
  - Rust：[Client/pomme-client/src/renderer/pipelines/block_entity.rs:1666](../../pomme-client/src/renderer/pipelines/block_entity.rs#L1666) — block entity draw loop：book stateからanimationを作って描画modelへ適用する。

</details>

<a id="blockentities-09"></a>

<details>
<summary>blockentities-09 — block entity › beacon › beam section描画</summary>

- 比較環境：クライアント共通
- 対象条件：有効なbeam sectionがあるbeaconを視界内で見る
- 残件 / 比較すべき点：beam sectionの有無・複数色・高さ・距離によるradius変化がRust画面に描かれないことを実機で確認する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/blockentity/BeaconRenderer.java:44](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/blockentity/BeaconRenderer.java#L44) — extract：beam sectionの色/高さとanimation/radius情報を抽出する。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/blockentity/BeaconRenderer.java:54](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/blockentity/BeaconRenderer.java#L54) — submit：各sectionのbeamと最終区間を描画する。
  - Rust：[Client/pomme-client/src/world/block_entity.rs:812](../../pomme-client/src/world/block_entity.rs#L812) — rendered_kind：描画対象block entity種別の明示matchにbeaconがない。
  - Rust：[Client/pomme-client/src/world/block_entity.rs:862](../../pomme-client/src/world/block_entity.rs#L862) — is_rendered：合成render entry対象の列挙にもBeaconがない。

</details>


<a id="domain-items"></a>

## アイテム・装備・プレイヤー描画

調査範囲：照合対象はプレイヤースキン/ケープ/装備、一人称の手と手持ちアイテム、GUIアイテム、ドロップアイテム、地図、盾、アイテム発動演出です。指定されたパイプライン群・entity_model.rs・player_skin_parts_tests.rs・item_activation.rsと、描画呼出経路および公式26.2 rendererを確認しました。Mob本体、block entity本体、inventory操作は対象外です。

- 制約：ビルド・cargo test・アプリ起動・実機比較は行っていない。player_skin_parts_tests.rs等の既存テストは読み取りのみで、テスト実行済みとは扱っていない。
- 制約：mob本体/block entity/inventory操作は担当外。mobの装備layer全般やmap GUI表示等は本一覧で包括的に断定しない。
- 制約：公式との意味的差分を明示できたのは今回確認したitem frame mapのdecoration描画経路。その他は一致を確認しておらず未検証。

| ID / 機能 | 公式の挙動 | Rustの現状・差分 | 実装 | 比較 / 検証 |
|---|---|---|---|---|
| [items-01](#items-01)<br>プレイヤースキン › モデルパーツ › slim腕とオーバーレイ表示 | 公式プレイヤーモデルはslim設定で腕幅を変え、帽子・ジャケット・袖・ズボンのオーバーレイを各表示フラグに応じて表示する。 | Rustはslim別モデルを焼き、7ビットのskin_parts_maskで6種のオーバーレイのみを切り替える。本番entity描画へmaskが渡される経路を確認した。 | 実装あり | 未検証<br>静的照合 |
| [items-02](#items-02)<br>プレイヤースキン › ケープ › 可視条件と装備との排他 | 公式は不可視でなくshowCapeが有効かつcape textureがある場合に表示し、胸装備のWINGS layerがある場合は通常capeを抑止、HUMANOID layerがあればcape attachmentを調整する。 | Rustはplayer/mannequin・可視・mask bit 0・cape textureの条件を確認し、WINGS layerで抑止、HUMANOID layer情報をattachmentに使う。SkinDataからcape textureをGPU uploadし描画する経路を確認した。 | 実装あり | 未検証<br>静的照合 |
| [items-03](#items-03)<br>装備 › humanoid防具 › Equipment asset layerのtexture/tint | 公式EquipmentLayerRendererは装備assetのlayerを順に描画し、usePlayerTexture時はプレイヤーtextureを使い、dye色をlayer色へ反映する。 | Rustは装備stackのEquippable slot/asset_idとDyedColorを読み、humanoid/humanoid_leggings asset JSONのlayer順・texture・染色色を解決し、active packのtextureを描画入力へ接続する。 | 一部実装 | 未検証<br>静的照合 |
| [items-04](#items-04)<br>一人称手 › skin arm › 左右とskin texture | 公式一人称rendererはplayer skin textureを使い、左右腕をプレイヤーモデルにより描画する。 | Rust HandPipelineは左右の腕meshをskin imageへ結び付けてdrawする。profile skin取得前はwide armで初期化し、後続skin準備時にはskin.slimに応じたmeshを生成する処理がある。 | 一部実装 | 未検証<br>静的照合 |
| [items-05](#items-05)<br>手持ちアイテム › first-person displayと使用動作 | 公式はItemDisplayContext.FIRST_PERSON系の左右別display transformを用い、食べる/飲む時は残り使用時間に応じたapplyEatTransformを適用する。 | Rustは左右別DisplayResolverを選び、通常・bow・shield block・eat/drinkのarm matrixを分岐してitem meshをdrawする。 | 一部実装 | 未検証<br>静的照合 |
| [items-06](#items-06)<br>盾 › 使用中の一人称表示 › blocking model | 公式は使用中のitemをuse動作に応じて一人称に描画し、盾は使用状態に応じたblocking表示姿勢を取る。 | Rustは現在の手がshieldをblockingしていると判定した場合minecraft:item/shield_blocking modelを解決し、shield_blocking arm matrixを適用する。 | 実装あり | 未検証<br>静的照合 |
| [items-07](#items-07)<br>GUI item › inventory/menu icon › item modelのatlas bake | 公式GUI item表示はitem modelのGUI display contextを解決し、GUI上でitem iconを描画する。 | RustはMenuElement::ItemIconを介してmeshをGUI item atlasへbakeし、atlas slotをmenu overlayから表示する本番経路がある。対応するGUI display transformと透明mesh pipelineも存在する。 | 一部実装 | 未検証<br>静的照合 |
| [items-08](#items-08)<br>ドロップitem › hover/spin/stack countによる重なり | 公式ItemEntityRendererはbounding box下端と0.0625最小浮上量を使い、ageでbob/spinし、model depthに応じてstack copiesを3D散開またはflat積層する。 | Rust item extractionはbbox由来hover補正、bob/spin、stack countに基づくcopy数・3D/flat配置を作り、ItemEntityPipelineへdraw入力を渡す。 | 実装あり | 未検証<br>静的照合 |
| [items-09](#items-09)<br>地図 › item frame › map textureの表示 | 公式MapRendererはmap textureを描画し、map dataのdecorationを抽出して条件に応じてmap面上にmarker/iconも重ねる。 | Rustはmap decorationをMapStoreに受信・保存し、item frameのmapを平面textureとして描く。主rendererのmap更新とmap_rgbaはcolorsのみを扱い、確認したframe描画経路にはdecorationのicon合成がない。 | 一部実装 | 差分あり<br>静的照合 |
| [items-10](#items-10)<br>アイテム発動 › Totem activation overlay › 寿命と表示 | 公式はアイテム発動を40 tick保持し、partial tickを含めてFIXED itemを拡大・回転・移動させてGUI表示する。 | RustはTotem使用イベントをlocal playerに限定してactivation stateを作り、手持ちmain/offhandのDeathProtection itemまたはTotemを選択、40 simulation tick保持してdraw payloadをrendererへ渡す。 | 一部実装 | 未検証<br>静的照合 |

### 条件・残件・根拠

<a id="items-01"></a>

<details>
<summary>items-01 — プレイヤースキン › モデルパーツ › slim腕とオーバーレイ表示</summary>

- 比較環境：クライアント共通
- 対象条件：プレイヤーのslim/standard skinと各skin part表示フラグを切り替える
- 残件 / 比較すべき点：実機で各skin partの可視性とslim腕の輪郭を公式26.2と比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/player/AvatarRenderer.java:49](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/player/AvatarRenderer.java#L49) — AvatarRenderer constructor：slimSteveに応じPLAYER_SLIM/PLAYER model layerとPlayerModel.slimを選択。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/model/player/PlayerModel.java:47](../../../minecraft-26.2-decompiled/src/net/minecraft/client/model/player/PlayerModel.java#L47) — PlayerModel.createMesh：slim時は腕幅3のmeshを構築。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/model/player/PlayerModel.java:112](../../../minecraft-26.2-decompiled/src/net/minecraft/client/model/player/PlayerModel.java#L112) — PlayerModel.setupAnim：showHat/showJacket/showLeftPants/showRightPants等でoverlay可視性を制御。
  - Rust：[Client/pomme-client/src/renderer/entity_model.rs:494](../../pomme-client/src/renderer/entity_model.rs#L494) — bake_player_model：slim設定を受けてplayer modelを生成。
  - Rust：[Client/pomme-client/src/renderer/pipelines/entity_renderer.rs:5066](../../pomme-client/src/renderer/pipelines/entity_renderer.rs#L5066) — player_model_part_visible：skin part maskでoverlay部位を判定。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:6166](../../pomme-client/src/app/phases/in_game.rs#L6166) — EntityRenderInfo construction：maskを本番renderer入力へ渡す。

</details>

<a id="items-02"></a>

<details>
<summary>items-02 — プレイヤースキン › ケープ › 可視条件と装備との排他</summary>

- 比較環境：クライアント共通
- 対象条件：cape textureあり/なし、cape表示mask、WINGS/HUMANOID胸装備を変える
- 残件 / 比較すべき点：showCape相当の表示設定がRust側mask bit 0へ反映される条件と、attachment姿勢を実機比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/layers/CapeLayer.java:51](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/layers/CapeLayer.java#L51) — CapeLayer.submit：invisible/showCape/cape texture条件とWINGS/HUMANOID分岐。
  - Rust：[Client/pomme-client/src/renderer/pipelines/entity_renderer.rs:5183](../../pomme-client/src/renderer/pipelines/entity_renderer.rs#L5183) — native_cape_draw_visible：capeの可視・WINGS排他predicate。
  - Rust：[Client/pomme-client/src/renderer/pipelines/entity_renderer.rs:5197](../../pomme-client/src/renderer/pipelines/entity_renderer.rs#L5197) — collect_player_capes：cape textureとattachmentを使って描画recordを作る。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:6166](../../pomme-client/src/app/phases/in_game.rs#L6166) — EntityRenderInfo construction：プレイヤー表示maskなどを描画入力に転送。

</details>

<a id="items-03"></a>

<details>
<summary>items-03 — 装備 › humanoid防具 › Equipment asset layerのtexture/tint</summary>

- 比較環境：クライアント共通
- 対象条件：head/chest/legs/feetのasset layer、染色およびresource pack上書き
- 残件 / 比較すべき点：公式にはlayer別player texture overrideやtrim描画もあるため、use_player_texture/trim/glintの適用範囲と見た目を確認する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/layers/EquipmentLayerRenderer.java:55](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/layers/EquipmentLayerRenderer.java#L55) — renderLayers：equipment layer順にtextureを選択してmodel描画する。
  - Rust：[Client/pomme-client/src/renderer/pipelines/equipment.rs:180](../../pomme-client/src/renderer/pipelines/equipment.rs#L180) — resolve_humanoid_equipment_layers：slot/component/asset定義からhumanoid layer群を解決する。
  - Rust：[Client/pomme-client/src/renderer/mod.rs:1978](../../pomme-client/src/renderer/mod.rs#L1978) — Renderer::render equipment preparation：装備stackに対してresolverを呼びrenderer入力を作る。
  - Rust：[Client/pomme-client/src/renderer/pipelines/entity_renderer.rs:4296](../../pomme-client/src/renderer/pipelines/entity_renderer.rs#L4296) — entity draw submission：準備済みarmor layerをentity描画側へ渡す。

</details>

<a id="items-04"></a>

<details>
<summary>items-04 — 一人称手 › skin arm › 左右とskin texture</summary>

- 比較環境：クライアント共通
- 対象条件：一人称でメイン/オフハンドそれぞれの腕をskin取得前後に表示する
- 残件 / 比較すべき点：AvatarRendererの袖(hasSleeve)表示とRustの一人称腕skin-part mask連携、およびprofile切替後の左右/skin modelを比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/player/AvatarRenderer.java:235](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/player/AvatarRenderer.java#L235) — renderRightHand：公式の右手描画はplayer skin textureとsleeve条件を受け取る。
  - Rust：[Client/pomme-client/src/renderer/pipelines/hand.rs:230](../../pomme-client/src/renderer/pipelines/hand.rs#L230) — HandPipeline::update_and_draw：左右を選んでskin付きarm vertex bufferを描画する。
  - Rust：[Client/pomme-client/src/renderer/pipelines/hand.rs:175](../../pomme-client/src/renderer/pipelines/hand.rs#L175) — HandPipeline::new：skin texture初期化と暫定wide arm mesh作成。
  - Rust：[Client/pomme-client/src/renderer/mod.rs:3451](../../pomme-client/src/renderer/mod.rs#L3451) — hand draw call：空手の一人称側でHandPipelineを呼び出す。

</details>

<a id="items-05"></a>

<details>
<summary>items-05 — 手持ちアイテム › first-person displayと使用動作</summary>

- 比較環境：クライアント共通
- 対象条件：左右手の通常item、食べる/飲むitem、bow使用の各状態
- 残件 / 比較すべき点：ItemInHandRendererの全arm transform合成順・数式とRust matrixの振幅/軸/表示条件を各使用アニメーションで比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/ItemInHandRenderer.java:287](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/ItemInHandRenderer.java#L287) — applyEatTransform：残り使用時間をもとに食べる/飲むtransformを作る公式処理。
  - Rust：[Client/pomme-client/src/renderer/pipelines/held_item.rs:104](../../pomme-client/src/renderer/pipelines/held_item.rs#L104) — HeldItemPipeline::update_and_draw：左右display transformとuse animation種別に応じたmatrix選択。
  - Rust：[Client/pomme-client/src/renderer/mod.rs:3419](../../pomme-client/src/renderer/mod.rs#L3419) — held item draw call：held item入力に対し本番drawを呼ぶ。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:6021](../../pomme-client/src/app/phases/in_game.rs#L6021) — hand/use animation extraction：interactionから手振りと使用状態を抽出して描画入力へ渡す。

</details>

<a id="items-06"></a>

<details>
<summary>items-06 — 盾 › 使用中の一人称表示 › blocking model</summary>

- 比較環境：クライアント共通
- 対象条件：main/offhandの盾について、その手でuse中と非use時を切り替える
- 残件 / 比較すべき点：公式のuseItemHand/main-arm条件およびblocking poseとRustのphysical hand判定・display json適用結果を比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/ItemInHandRenderer.java:475](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/ItemInHandRenderer.java#L475) — renderItem call path：各手のitem stackを公式rendererで処理する入口。
  - Rust：[Client/pomme-client/src/renderer/pipelines/held_item.rs:136](../../pomme-client/src/renderer/pipelines/held_item.rs#L136) — HeldItemPipeline::update_and_draw：block中の盾にshield_blocking model keyを選ぶ。
  - Rust：[Client/pomme-client/src/renderer/pipelines/held_item.rs:155](../../pomme-client/src/renderer/pipelines/held_item.rs#L155) — HeldItemPipeline::update_and_draw：shield blocking時に専用arm matrixへ分岐する。
  - Rust：[Client/pomme-client/src/renderer/pipelines/held_item.rs:274](../../pomme-client/src/renderer/pipelines/held_item.rs#L274) — shield_blocking_for_hand：use animationが対象handと一致する条件を判定する。

</details>

<a id="items-07"></a>

<details>
<summary>items-07 — GUI item › inventory/menu icon › item modelのatlas bake</summary>

- 比較環境：クライアント共通
- 対象条件：通常item iconと半透明item iconをGUI/menu上で表示する
- 残件 / 比較すべき点：公式のcount/durability/foil/tooltip/oversized item表示までこの範囲で再現されるか、GUI実機比較で確認する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/state/gui/GuiItemRenderState.java:1](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/state/gui/GuiItemRenderState.java#L1) — GuiItemRenderState：GUI item用render stateの公式型。
  - Rust：[Client/pomme-client/src/renderer/pipelines/gui_item.rs:158](../../pomme-client/src/renderer/pipelines/gui_item.rs#L158) — GuiItemPipeline::new：GUI display resolverと通常/透過item pipelineを構築。
  - Rust：[Client/pomme-client/src/renderer/pipelines/gui_item.rs:260](../../pomme-client/src/renderer/pipelines/gui_item.rs#L260) — GuiItemPipeline::bake_to_slot：item meshをGUI slot上に描画/bakeする。
  - Rust：[Client/pomme-client/src/renderer/pipelines/menu_overlay.rs:1414](../../pomme-client/src/renderer/pipelines/menu_overlay.rs#L1414) — MenuElement::ItemIcon draw：GUI menu elementのitem iconを描画経路へ接続する。

</details>

<a id="items-08"></a>

<details>
<summary>items-08 — ドロップitem › hover/spin/stack countによる重なり</summary>

- 比較環境：クライアント共通
- 対象条件：地上にある1個/複数個のflat itemと3D itemのdropを観察する
- 残件 / 比較すべき点：random seed、copy count境界、3D閾値0.0625および位置/回転を同一ageで比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/ItemEntityRenderer.java:41](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/ItemEntityRenderer.java#L41) — ItemEntityRenderer.submit：bbox hover補正、bob、spin後にcount描画へ進む。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/ItemEntityRenderer.java:72](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/ItemEntityRenderer.java#L72) — submitMultipleFromCount：model depthに応じた複数item配置。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:8524](../../pomme-client/src/app/phases/in_game.rs#L8524) — emit_item_copies：bob/spin/copy配置を含むitem render inputsを生成。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:8899](../../pomme-client/src/app/phases/in_game.rs#L8899) — build_item_render_infos：item entity storeから描画対象itemを抽出する。

</details>

<a id="items-09"></a>

<details>
<summary>items-09 — 地図 › item frame › map textureの表示</summary>

- 比較環境：クライアント共通
- 対象条件：marker/decorationを含むmapをitem frameに設置して正面から見る
- 残件 / 比較すべき点：Rust側にmap decorationの抽出・描画が接続されていないため、公式marker/iconの欠落を実機で確認する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/MapRenderer.java:85](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/MapRenderer.java#L85) — extractRenderState：textureとdecorationをrender stateに抽出。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/MapRenderer.java:48](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/MapRenderer.java#L48) — render：表示条件に応じdecorationをmap面上に描画。
  - Rust：[Client/pomme-client/src/world/maps.rs:198](../../pomme-client/src/world/maps.rs#L198) — MapStore::apply：受信decorationを保持。受信非対応ではなく描画未接続。
  - Rust：[Client/pomme-client/src/renderer/map_texture.rs:118](../../pomme-client/src/renderer/map_texture.rs#L118) — map_rgba：colorsのpalette indexからRGBAを作り、decorationは合成しない。
  - Rust：[Client/pomme-client/src/renderer/mod.rs:1865](../../pomme-client/src/renderer/mod.rs#L1865) — Renderer::render_world map update：map texture更新判定とuploadはcolorsのみ。
  - Rust：[Client/pomme-client/src/renderer/pipelines/map_quad.rs:281](../../pomme-client/src/renderer/pipelines/map_quad.rs#L281) — map_quad_vertices：map textureの平面を作る。

</details>

<a id="items-10"></a>

<details>
<summary>items-10 — アイテム発動 › Totem activation overlay › 寿命と表示</summary>

- 比較環境：クライアント共通
- 対象条件：local playerでTotem発動し、40 tickのoverlayと表示itemを観察する
- 残件 / 比較すべき点：公式ScreenEffectRendererのsmoothScale、移動/回転数式、ランダムoffset範囲とRust animation matrixおよびPRNGを同条件比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/ScreenEffectRenderer.java:36](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/ScreenEffectRenderer.java#L36) — ITEM_ACTIVATION_ANIMATION_LENGTH：公式activation durationは40 tick。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/ScreenEffectRenderer.java:86](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/ScreenEffectRenderer.java#L86) — renderItemActivationAnimation：partial tickを含むanimationとFIXED item描画。
  - Rust：[Client/pomme-client/src/item_activation.rs:204](../../pomme-client/src/item_activation.rs#L204) — ItemActivation：40 tick state、tickと非進行draw payloadを管理。
  - Rust：[Client/pomme-client/src/app/core.rs:6004](../../pomme-client/src/app/core.rs#L6004) — Totem event handler：Totemイベントからlocal-player activationを設定する。
  - Rust：[Client/pomme-client/src/renderer/mod.rs:1854](../../pomme-client/src/renderer/mod.rs#L1854) — Renderer activation preparation：activation payloadをrenderer側で消費する。

</details>


<a id="domain-particles"></a>

## パーティクル

調査範囲：Level-particle packetの wire/options decodeからspawnまで、ParticleStoreの一部型・寿命/運動・描画、worldのブロック/環境/ブロックエンティティparticle tickを調査。mobごとの全発生挙動・全Particle providerは網羅しておらず、一致確認や実機比較はしていない。

- 制約：対象は限定した挙動のみで、全particle kind/provider・全mob発生源を確認しておらず、全particle互換を断定しない。
- 制約：調査は静的ソース照合であり、build/test/application起動・公式クライアントとの実機比較は未実施。
- 制約：天候本体と音は担当範囲外。

| ID / 機能 | 公式の挙動 | Rustの現状・差分 | 実装 | 比較 / 検証 |
|---|---|---|---|---|
| [particles-01](#particles-01)<br>ネットワーク › LevelParticles › typed particle packet decodeとspawn接続 | 公式packetはoverrideLimiter/alwaysShow、位置、spread、速度、count、ParticleOptionsをregistry-aware stream codecで読み、ClientPacketListenerからparticle engineへ渡す。 | RustのLevelParticles parserはpacketのフラグ・位置・分散・countとparticle種別に応じたoptionsをNetworkEvent化し、coreのイベント処理からParticleStoreへ渡す。 | 実装あり | 未検証<br>静的照合 |
| [particles-02](#particles-02)<br>ネットワーク › LevelParticles › count 0の方向指定とcount正数の散布 | 公式count=0はspread×maxSpeedを単一particleの速度として渡し、count&gt;0は各particleの位置と速度を独立したGaussian乱数で生成する。 | Rustのpacket spawn関数もcount=0で単一方向particleを作り、正数countではGaussian散布とvelocityをcount回作る。負countはspawnしない。 | 実装あり | 未検証<br>静的照合 |
| [particles-03](#particles-03)<br>particle lifecycle › 共通tick › 寿命・重力・摩擦・衝突 | 公式Particle.tickはageがlifetime到達後に除去し、それまでは重力を適用して移動、摩擦を掛け、地面接触時は水平速度を追加減衰する。移動は有効時にblock collisionで制限する。 | RustのParticle tickはage/lifetimeで終了し、重力と種類別移動を適用する。terrain/item/smoke/dust等はcollision付き移動へ分岐し、種類ごとの差分がある。 | 一部実装 | 未検証<br>静的照合 |
| [particles-04](#particles-04)<br>描画 › 粒子quad › billboard・opaque/translucent分類 | 公式engineはrender group別にparticle stateを抽出し、particle provider/layerに応じた描画を行う。SingleQuad系ではカメラ向きなどのfacing modeと不透明/半透明layerが描画形状を決める。 | RustはParticleStoreからParticleQuadを抽出し、renderer pipelineで通常billboardまたはLOOKAT_Y向きを選択、opaque/translucent別に頂点を生成する。 | 一部実装 | 未検証<br>静的照合 |
| [particles-05](#particles-05)<br>ブロック発生 › repeater › animate tick dust | 公式RepeaterBlock.animateTickは状態の向き・遅延等に基づき、位置を選んでREDSTONE dustをspawnする。 | Rustのblock particle samplerは複数block種別を現在の状態からparticle requestへ変換し、in-game tickでanimate probeを共有してspawnする。ただし今回の確認範囲ではRepeaterの具体的な発生分岐を特定できず、実装有無を断定しない。 | 未調査 | 未検証<br>未調査 |
| [particles-06](#particles-06)<br>流体発生 › 水中水泡/溶岩滴 › animate tick | 公式WaterFluidは条件を満たす水でUNDERWATERを、LavaFluidは確率条件を満たす溶岩でLAVAをanimate tick発生させる。 | Rustのsample_block_particlesはWaterFluid/LavaFluid相当と形状対応drip branchをサンプリングし、requestをgame tickのparticle storeへ送る。 | 実装あり | 未検証<br>静的照合 |
| [particles-07](#particles-07)<br>環境発生 › biome/dimension ambient particle probability | 公式ambient particle属性はbiome値があれば適用し、なければdimension値を使い、probabilityに応じてanimate tick位置へ粒子を発生させる。 | Rustは既存animate probe位置ごとにbiome idとparticle属性を解決し、なければdimension設定へfallbackし、確率判定後requestを返す。Minimal modeは生成を止める。 | 一部実装 | 未検証<br>静的照合 |
| [particles-08](#particles-08)<br>ブロックエンティティ発生 › lit campfire smoke | 公式CampfireBlockEntity.particleTickは一定確率でCampfireBlock.makeParticlesを呼び、cosy/signalの状態に応じた煙などを発生させる。 | Rustはblock entity粒子tickで読み込み済みの対象状態からparticle requestを構成し、in-game tickがrequestをstoreへ送る。Campfireを含む複数のblock entity向け実装だが、回数・乱数・煙挙動の一致は未確認。 | 一部実装 | 未検証<br>静的照合 |
| [particles-09](#particles-09)<br>typed particle › Dust › packet option color/scaleから描画生成 | 公式dust optionは色とscaleを保持し、DustParticle providerがその値を用いてparticleを初期化する。 | Rust packet parserはDust optionsをServerParticleOptions::Dustとして保持し、ParticleStoreのDust分岐で構築する。typed data経路はあるがproviderごとの色・サイズ・寿命は未照合。 | 一部実装 | 未検証<br>静的照合 |
| [particles-10](#particles-10)<br>mob発生 › Blaze › tickごとのLARGE_SMOKE | 公式Blaze.aiStepのclient-side経路は毎tick、entity周辺にLARGE_SMOKEを2個spawnする。 | RustのEntityStore::client_particle_requestsはBlazeごとに位置乱数を使ってLargeSmokeを2個request化し、in-game tickでparticle storeへ渡す。 | 実装あり | 未検証<br>静的照合 |

### 条件・残件・根拠

<a id="particles-01"></a>

<details>
<summary>particles-01 — ネットワーク › LevelParticles › typed particle packet decodeとspawn接続</summary>

- 比較環境：公式サーバー接続
- 対象条件：サーバーからtyped LevelParticles packetを受け取る
- 残件 / 比較すべき点：各particle optionのprotocol固有codecとregistry IDを26.2サーバーpacketで比較する。全kind対応とは断定しない。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/network/protocol/game/ClientboundLevelParticlesPacket.java:36](../../../minecraft-26.2-decompiled/src/net/minecraft/network/protocol/game/ClientboundLevelParticlesPacket.java#L36) — ClientboundLevelParticlesPacket(RegistryFriendlyByteBuf)：wire項目を順に読みParticleTypes.STREAM_CODECでoptionsをdecodeする。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java:2203](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java#L2203) — handleParticleEvent：decoded packetをClientLevel.addParticle経由でengineへ渡す。
  - Rust：[Client/pomme-client/src/net/handler.rs:2810](../../pomme-client/src/net/handler.rs#L2810) — parse_level_particles_impl：typed optionsを伴うLevelParticlesのdecode実装。
  - Rust：[Client/pomme-client/src/app/core.rs:5942](../../pomme-client/src/app/core.rs#L5942) — NetworkEvent::LevelParticles：実イベント経路でParticleStore::add_particles_from_packetを呼ぶ。

</details>

<a id="particles-02"></a>

<details>
<summary>particles-02 — ネットワーク › LevelParticles › count 0の方向指定とcount正数の散布</summary>

- 比較環境：公式サーバー接続
- 対象条件：同一のcount=0/正数packetを受信する
- 残件 / 比較すべき点：乱数generator・seedが異なるため、同一seedでの座標一致ではなく分布と件数を実行比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java:2205](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java#L2205) — handleParticleEvent：0件分岐の速度計算および正数件のGaussian scatter loop。
  - Rust：[Client/pomme-client/src/particle.rs:3066](../../pomme-client/src/particle.rs#L3066) — ParticleStore::add_particles_from_packet：count 0/正数を分けて単一spawnまたはGaussian spawnを行う。
  - Rust：[Client/pomme-client/src/app/core.rs:5942](../../pomme-client/src/app/core.rs#L5942) — NetworkEvent::LevelParticles：packet fieldsをspawn関数へ接続。

</details>

<a id="particles-03"></a>

<details>
<summary>particles-03 — particle lifecycle › 共通tick › 寿命・重力・摩擦・衝突</summary>

- 比較環境：クライアント共通
- 対象条件：寿命が設定された通常のterrain/smoke/dust particle
- 残件 / 比較すべき点：複数providerのlifetime・衝突境界・接地摩擦を別々に比較する。全particleで共通挙動とは扱わない。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/particle/Particle.java:91](../../../minecraft-26.2-decompiled/src/net/minecraft/client/particle/Particle.java#L91) — Particle.tick：age/lifetime判定、重力、移動、摩擦、接地時の水平減衰。
  - Rust：[Client/pomme-client/src/particle.rs:1046](../../pomme-client/src/particle.rs#L1046) — Particle::tick：寿命確認、重力とparticle kind別移動のtick経路。
  - Rust：[Client/pomme-client/src/particle.rs:3623](../../pomme-client/src/particle.rs#L3623) — ParticleStore::tick_with_entity_lookup：live particlesをtickするstore側処理。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:4335](../../pomme-client/src/app/phases/in_game.rs#L4335) — in_game particle tick：ゲームtick本番経路がstore tickを呼ぶ。

</details>

<a id="particles-04"></a>

<details>
<summary>particles-04 — 描画 › 粒子quad › billboard・opaque/translucent分類</summary>

- 比較環境：クライアント共通
- 対象条件：通常quad particleとLOOKAT_Y指定particleを描画する
- 残件 / 比較すべき点：全render layer、depth/blend状態、特定providerの見た目を画像比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/particle/ParticleEngine.java:123](../../../minecraft-26.2-decompiled/src/net/minecraft/client/particle/ParticleEngine.java#L123) — ParticleEngine.extract：render orderのgroupからrender stateを抽出。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/particle/Particle.java:115](../../../minecraft-26.2-decompiled/src/net/minecraft/client/particle/Particle.java#L115) — Particle.getGroup：各particleがrender groupを指定する契約。
  - Rust：[Client/pomme-client/src/particle.rs:3789](../../pomme-client/src/particle.rs#L3789) — ParticleStore::extract：現在のparticleをrender用quadへ変換。
  - Rust：[Client/pomme-client/src/renderer/pipelines/particle.rs:86](../../pomme-client/src/renderer/pipelines/particle.rs#L86) — build_particle_vertices：billboard向きとtranslucentフラグを使いGPU頂点を生成。

</details>

<a id="particles-05"></a>

<details>
<summary>particles-05 — ブロック発生 › repeater › animate tick dust</summary>

- 比較環境：クライアント共通
- 対象条件：稼働中のRepeaterをanimate tick対象として表示する
- 残件 / 比較すべき点：Rust blocks.rsでRepeater固有分岐の有無を確認する。未確認の間は発生有無を断定せず、公式animateTickとの条件・位置分布比較は保留する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/level/block/RepeaterBlock.java:88](../../../minecraft-26.2-decompiled/src/net/minecraft/world/level/block/RepeaterBlock.java#L88) — RepeaterBlock.animateTick：公式のrepeater animate-tick発生処理。
  - Rust：[Client/pomme-client/src/world/particle_tick/blocks.rs:12](../../pomme-client/src/world/particle_tick/blocks.rs#L12) — sample_positions：ブロック状態別particle request samplerの入口。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:4059](../../pomme-client/src/app/phases/in_game.rs#L4059) — sample_animate_tick_particles_at_positions：animate probeを使うworld particle本番処理。

</details>

<a id="particles-06"></a>

<details>
<summary>particles-06 — 流体発生 › 水中水泡/溶岩滴 › animate tick</summary>

- 比較環境：クライアント共通
- 対象条件：読み込み済みchunk内の水/溶岩fluid animate tick
- 残件 / 比較すべき点：fluid source/falling・上面/隣接形状、spawn確率と乱数頻度を同じ地形で比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/level/material/WaterFluid.java:60](../../../minecraft-26.2-decompiled/src/net/minecraft/world/level/material/WaterFluid.java#L60) — WaterFluid.animateTick：水中particle発生と条件の入口。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/level/material/LavaFluid.java:63](../../../minecraft-26.2-decompiled/src/net/minecraft/world/level/material/LavaFluid.java#L63) — LavaFluid.animateTick：溶岩particle発生分岐。
  - Rust：[Client/pomme-client/src/world/particle_tick.rs:202](../../pomme-client/src/world/particle_tick.rs#L202) — sample_block_particles：Water/Lava animate tickとdrip branchの実装入口。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:4110](../../pomme-client/src/app/phases/in_game.rs#L4110) — sample_block_particles_at_positions：world particle requestを本番tickに接続。

</details>

<a id="particles-07"></a>

<details>
<summary>particles-07 — 環境発生 › biome/dimension ambient particle probability</summary>

- 比較環境：クライアント共通
- 対象条件：ambient particle属性のあるbiomeとdimension fallbackを移動する
- 残件 / 比較すべき点：属性取得元、確率判定頻度、乱数列およびparticle modeによる公式差分を同一環境で検証する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientLevel.java:568](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientLevel.java#L568) — ClientLevel.animateTick：animate-tick probeを実施しblock/fluidへ委譲する公式入口。
  - Rust：[Client/pomme-client/src/world/environment_particles.rs:86](../../pomme-client/src/world/environment_particles.rs#L86) — sample_loaded_positions：biome ambientを優先しdimension値へfallback、probabilityでrequest化。
  - Rust：[Client/pomme-client/src/world/environment_particles.rs:32](../../pomme-client/src/world/environment_particles.rs#L32) — sample：共通animate positions samplerを使う。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:4030](../../pomme-client/src/app/phases/in_game.rs#L4030) — sample_loaded_positions：in-gameでambient requestをparticle storeへ流す。

</details>

<a id="particles-08"></a>

<details>
<summary>particles-08 — ブロックエンティティ発生 › lit campfire smoke</summary>

- 比較環境：クライアント共通
- 対象条件：lit campfireのcosy/signal fireを近傍で表示する
- 残件 / 比較すべき点：CampfireBlockEntity/ CampfireBlockの確率・信号火の高さ・cosy smoke provider lifetimeを比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/level/block/entity/CampfireBlockEntity.java:98](../../../minecraft-26.2-decompiled/src/net/minecraft/world/level/block/entity/CampfireBlockEntity.java#L98) — CampfireBlockEntity.particleTick：particle tickと煙発生の条件。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/particle/CampfireSmokeParticle.java:8](../../../minecraft-26.2-decompiled/src/net/minecraft/client/particle/CampfireSmokeParticle.java#L8) — CampfireSmokeParticle：cosy/signal smokeのclient provider。
  - Rust：[Client/pomme-client/src/world/block_entity_particle.rs:129](../../pomme-client/src/world/block_entity_particle.rs#L129) — tick_block_entity_particles：block entity particle発生処理。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:4203](../../pomme-client/src/app/phases/in_game.rs#L4203) — tick_block_entity_particles：ゲームtickから処理を呼びrequestを合流。

</details>

<a id="particles-09"></a>

<details>
<summary>particles-09 — typed particle › Dust › packet option color/scaleから描画生成</summary>

- 比較環境：公式サーバー接続
- 対象条件：サーバーから異なる色/scaleのDust packetを受信する
- 残件 / 比較すべき点：公式DustParticleBaseのscale・color変換・lifetimeとRust Dust constructorを複数入力値で比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/particle/DustParticle.java:8](../../../minecraft-26.2-decompiled/src/net/minecraft/client/particle/DustParticle.java#L8) — DustParticle：公式dust particle provider実装。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/particle/DustParticleBase.java:7](../../../minecraft-26.2-decompiled/src/net/minecraft/client/particle/DustParticleBase.java#L7) — DustParticleBase：scalable optionsを使うdust共通particle。
  - Rust：[Client/pomme-client/src/net/handler.rs:2859](../../pomme-client/src/net/handler.rs#L2859) — parse_level_particles_impl Dust：Dust typed optionsをdecode。
  - Rust：[Client/pomme-client/src/particle.rs:3440](../../pomme-client/src/particle.rs#L3440) — ParticleStore::add_server_particle Dust branch：Dust optionからparticleを生成するcase。

</details>

<a id="particles-10"></a>

<details>
<summary>particles-10 — mob発生 › Blaze › tickごとのLARGE_SMOKE</summary>

- 比較環境：クライアント共通
- 対象条件：表示中Blazeの通常client tick
- 残件 / 比較すべき点：同tick条件でspawn位置分布・粒子の寿命/描画を比較する。mob全種の発生は確認していない。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/entity/monster/Blaze.java:94](../../../minecraft-26.2-decompiled/src/net/minecraft/world/entity/monster/Blaze.java#L94) — Blaze.aiStep：client-side分岐でloop 2回、LARGE_SMOKEを発生。
  - Rust：[Client/pomme-client/src/entity/mod.rs:3776](../../pomme-client/src/entity/mod.rs#L3776) — EntityStore::client_particle_requests Blaze case：BlazeごとにLargeSmokeを2 request作成。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:4090](../../pomme-client/src/app/phases/in_game.rs#L4090) — EntityStore::client_particle_requests：in-game tickのmob particle request収集。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:4219](../../pomme-client/src/app/phases/in_game.rs#L4219) — world_particle_requests spawn loop：mob requestをParticleStore::add_particle_spawn_requestへ渡す。

</details>


<a id="domain-audio"></a>

## 音声・音楽・字幕

調査範囲：RustのAudioEngine/SoundsIndex、OpenALのlistener/source設定、SubtitleOverlayStateと、app/core.rs・app/phases/in_game.rsのサウンド受信/描画接続を確認。公式SoundEngine、SoundManager、MusicManager、SubtitleOverlayを照合した。decoder.rsの音声形式/エラー処理の詳細、全ゲーム内サウンド発生箇所、聴感・実機比較は未調査。

- 制約：調査は静的ソース照合のみ。音声を実際に聴いた、実機比較した、ビルドまたはテストしたとは判定していない。
- 制約：decoder.rsの対応codec/stream decoderの詳細、全sound eventと全環境音源の実装網羅は未調査。
- 制約：公式サーバー権威のsound packet生成側は評価対象外。受信後のclient再生経路のみ調査した。

| ID / 機能 | 公式の挙動 | Rustの現状・差分 | 実装 | 比較 / 検証 |
|---|---|---|---|---|
| [audio-01](#audio-01)<br>サウンドイベント › sounds.json › リソースパックの登録と重み付き選択 | SoundManagerはリソーススタックからsounds.jsonを読み、replace指定で既存イベントを置換し、WeighedSoundEventsは重み付き候補から音源を選ぶ。 | SoundsIndexが内蔵assetsと有効リソースパックを順に適用し、replace以外は候補を追加、再生時にweightを使ってファイル候補を選ぶ。 | 実装あり | 未検証<br>静的照合 |
| [audio-02](#audio-02)<br>カテゴリー › サウンド音量 › カテゴリー別・Master音量 | 公式はインスタンス音量、カテゴリー設定値、追加category gainを掛け、結果を0..1に制限する。 | SoundCategoryのプロトコルordinalを検査してvolume slotへ対応付け、Master×対象カテゴリーのgainをsourceへ設定し、変更時に稼働sourceも更新する。 | 実装あり | 未検証<br>静的照合 |
| [audio-03](#audio-03)<br>距離と定位 › ワールド音 › listener姿勢と距離減衰 | 公式はカメラ位置・前方・上方をlistenerへ設定し、LINEAR attenuationの場合にmax(volume,1)×sound attenuation distanceを使う。 | ゲーム内でeye位置とyaw/pitchからlistener向きを更新し、ワールド音をabsolute位置へ送り、OpenALのlinear-distance sourceに距離上限を設定する。 | 実装あり | 未検証<br>静的照合 |
| [audio-04](#audio-04)<br>距離と定位 › entity-bound sound › entity移動追従と終了 | 公式はtickable sound instanceの位置をtickごとに更新し、sound停止条件に応じて停止する。 | PlayEntitySoundは既知entity位置でsourceを開始し、entity位置更新を該当sourceへ転送する。停止/完了報告で追跡数を解放するが、汎用tickable sound instanceの位置・音量更新モデルではない。 | 一部実装 | 未検証<br>静的照合 |
| [audio-05](#audio-05)<br>停止 › stop sound packet › sound idとcategoryの組合せ | 公式はsound idとsourceの両方があれば一致したインスタンスを止め、片方のみならその条件で止め、両方nullなら全停止する。 | NetworkEvent::StopSoundをstop_soundsへ渡す。sound idとcategoryのAND一致でactive sourceを停止し、両方なしならStopAll、未知categoryは無視する。 | 実装あり | 未検証<br>静的照合 |
| [audio-06](#audio-06)<br>繰返し › sound instance › loopingと遅延再生 | 公式SoundEngineはinstanceのloopingとdelayから自動loopまたは手動loopを決め、手動loopではdelay後に再生を再登録する。 | RustのUI/world/menu各再生経路はlooping=falseを指定し、遅延再生や一般のlooping instanceを表現するAPIは確認できない。 | 一部実装 | 差分あり<br>静的照合 |
| [audio-07](#audio-07)<br>音楽 › メニューBGM › 開始待ち・曲間待ち | 公式MusicManagerはsituational Musicのmin/max delay、音楽頻度設定、現在曲の置換可否に応じて曲を開始し、音量変更時はgainをfadeする。 | Rustはメニューでmusic.menuのみを100 tick後に開始し、終了後20..600 tick待つ。situational music選択、music frequency設定、音量fadeはこのAudioEngine経路にない。 | 一部実装 | 差分あり<br>静的照合 |
| [audio-08](#audio-08)<br>字幕 › sound event subtitle › 再生通知と距離範囲 | 公式は再生sound eventにsubtitleがありlistener登録時、実音量・category cullingより前に通知する。非relativeかつattenuation有効ならattenuation distanceを範囲にし、それ以外は無限範囲とする。 | RustはShow Subtitles有効時に解決済みイベントのsubtitle key・発生位置・max(volume,1)×attenuation distanceをqueueし、描画側で距離判定する。UI再生経路は字幕queueを通らない。 | 一部実装 | 差分あり<br>静的照合 |
| [audio-09](#audio-09)<br>字幕 › subtitle overlay › 表示時間と距離矢印 | 公式SubtitleOverlayはnotification display time設定倍率を掛けた期間で古い位置を削除し、最寄り位置に応じて視野外方向矢印を表示する。 | Rust overlayは再生位置をsubtitle key単位で保持し、最寄りsource・方向矢印・fadeを描くが、表示時間は3000ms固定でnotification display time倍率設定がない。 | 一部実装 | 差分あり<br>静的照合 |
| [audio-10](#audio-10)<br>環境音 › クライアントローカル効果 › portal ambient trigger | 公式LocalPlayerはportalEffectIntensityが0のときblock.portal.triggerをforLocalAmbience経由で再生する。Rust側portal triggerの発生条件・音量・pitch・頻度との実動作一致は未確認。 | Rustはportal内tickでblock.portal.triggerをAmbientカテゴリー、player位置、volume 0.25、pitch 0.8..1.2でローカル発火する接続を持つ。公式の汎用biome/環境音tick全体との互換性は確認していない。 | 一部実装 | 未検証<br>静的照合 |

### 条件・残件・根拠

<a id="audio-01"></a>

<details>
<summary>audio-01 — サウンドイベント › sounds.json › リソースパックの登録と重み付き選択</summary>

- 比較環境：クライアント共通
- 対象条件：sounds.jsonの同一イベントへ複数packが登録され、イベント再生される場合
- 残件 / 比較すべき点：26.2アセットと同じpack優先順位・候補分布・乱数系列で選択されるかを比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/sounds/SoundManager.java:89](../../../minecraft-26.2-decompiled/src/net/minecraft/client/sounds/SoundManager.java#L89) — SoundManager.prepare：リソーススタック内のsounds.jsonを列挙して登録処理へ渡す。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/sounds/SoundManager.java:266](../../../minecraft-26.2-decompiled/src/net/minecraft/client/sounds/SoundManager.java#L266) — Preparations.handleRegistration：replace時に登録を置換し、そうでなければ既存登録を保持して音源を追加する。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/sounds/WeighedSoundEvents.java:42](../../../minecraft-26.2-decompiled/src/net/minecraft/client/sounds/WeighedSoundEvents.java#L42) — WeighedSoundEvents.getSound：weight合計から乱数で候補を選ぶ。
  - Rust：[Client/pomme-client/src/audio/sounds.rs:186](../../pomme-client/src/audio/sounds.rs#L186) — SoundsIndex::load：内蔵音声定義とpackをロードし、優先順に適用する。
  - Rust：[Client/pomme-client/src/audio/sounds.rs:298](../../pomme-client/src/audio/sounds.rs#L298) — SoundsIndex::apply_value / choose_inner：replace/追加と重みによる候補選択を実装している。

</details>

<a id="audio-02"></a>

<details>
<summary>audio-02 — カテゴリー › サウンド音量 › カテゴリー別・Master音量</summary>

- 比較環境：クライアント共通
- 対象条件：MasterおよびMusic/Blocks等の各カテゴリー音量を変更して再生中の音がある場合
- 残件 / 比較すべき点：全カテゴリーordinalと設定値の伝播・volume clampを同条件で確認する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/sounds/SoundEngine.java:469](../../../minecraft-26.2-decompiled/src/net/minecraft/client/sounds/SoundEngine.java#L469) — SoundEngine.calculateVolume(float, SoundSource)：インスタンス音量、設定カテゴリー音量、gainを乗算する。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/sounds/SoundSource.java:6](../../../minecraft-26.2-decompiled/src/net/minecraft/sounds/SoundSource.java#L6) — SoundSource：公式のsound sourceカテゴリー定義。
  - Rust：[Client/pomme-client/src/audio/mod.rs:36](../../pomme-client/src/audio/mod.rs#L36) — SoundCategory / try_from_index：カテゴリーをordinalで定義し、未定義値を拒否する。
  - Rust：[Client/pomme-client/src/audio/mod.rs:1261](../../pomme-client/src/audio/mod.rs#L1261) — category_gain / refresh_gains：Masterと個別カテゴリーを合成してOpenAL gainへ反映する。

</details>

<a id="audio-03"></a>

<details>
<summary>audio-03 — 距離と定位 › ワールド音 › listener姿勢と距離減衰</summary>

- 比較環境：クライアント共通
- 対象条件：listenerから異なる方向・距離に置いた通常のLINEAR attenuationワールド音
- 残件 / 比較すべき点：座標軸、基準距離0、距離境界での減衰カーブと公式のlinearAttenuationを比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/sounds/SoundEngine.java:493](../../../minecraft-26.2-decompiled/src/net/minecraft/client/sounds/SoundEngine.java#L493) — SoundEngine.updateSource：カメラの位置・forward・upをlistener transformへ設定する。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/sounds/SoundEngine.java:377](../../../minecraft-26.2-decompiled/src/net/minecraft/client/sounds/SoundEngine.java#L377) — SoundEngine.play：音量からattenuation distanceを計算し、LINEAR時にchannelへ設定する。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:3718](../../pomme-client/src/app/phases/in_game.rs#L3718) — in_game update audio listener：eye位置とlook方向をAudioEngineへ渡し、同frameで音量/subtitle設定を更新する。
  - Rust：[Client/pomme-client/src/audio/openal.rs:623](../../pomme-client/src/audio/openal.rs#L623) — Source::configure：position/relativeとlinear-distance、max distanceを設定する。

</details>

<a id="audio-04"></a>

<details>
<summary>audio-04 — 距離と定位 › entity-bound sound › entity移動追従と終了</summary>

- 比較環境：クライアント共通
- 対象条件：サーバーからentity-bound soundを受け、再生中にentityが移動または消滅する場合
- 残件 / 比較すべき点：entity音のsource追従タイミング、停止通知、未知/消滅entityへの動作を比較する。汎用tickable soundは未確認。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/sounds/SoundEngine.java:245](../../../minecraft-26.2-decompiled/src/net/minecraft/client/sounds/SoundEngine.java#L245) — SoundEngine.tickInGameSound：tickable instanceの位置・音量・pitchを更新する。
  - Rust：[Client/pomme-client/src/app/core.rs:5302](../../pomme-client/src/app/core.rs#L5302) — NetworkEvent::PlayEntitySound：entity soundをAudioEngineへ渡す。
  - Rust：[Client/pomme-client/src/app/core.rs:5664](../../pomme-client/src/app/core.rs#L5664) — entity position update：entity位置変化をAudioEngineへ転送する。
  - Rust：[Client/pomme-client/src/audio/mod.rs:909](../../pomme-client/src/audio/mod.rs#L909) — AudioWorker::handle_command：entity id一致のactive source位置を更新し、StopEntityも処理する。

</details>

<a id="audio-05"></a>

<details>
<summary>audio-05 — 停止 › stop sound packet › sound idとcategoryの組合せ</summary>

- 比較環境：公式サーバー接続
- 対象条件：sound idのみ/categoryのみ/両方/両方なしのstop packetをそれぞれ受ける場合
- 残件 / 比較すべき点：再生中と再生終了直後の停止対象、namespace正規化、停止packetと同frameの再生競合を確認する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/sounds/SoundEngine.java:500](../../../minecraft-26.2-decompiled/src/net/minecraft/client/sounds/SoundEngine.java#L500) — SoundEngine.stop(Identifier, SoundSource)：source別、sound id別、全停止の条件分岐。
  - Rust：[Client/pomme-client/src/app/core.rs:5312](../../pomme-client/src/app/core.rs#L5312) — NetworkEvent::StopSound：ネットワーク停止イベントをAudioEngineへ接続する。
  - Rust：[Client/pomme-client/src/audio/mod.rs:592](../../pomme-client/src/audio/mod.rs#L592) — AudioEngine::stop_sounds：category検査後にID/category filterまたは全停止を発行する。
  - Rust：[Client/pomme-client/src/audio/mod.rs:891](../../pomme-client/src/audio/mod.rs#L891) — AudioWorker::handle_command / sound_matches_stop：active sound IDとcategoryのAND一致で停止する。

</details>

<a id="audio-06"></a>

<details>
<summary>audio-06 — 繰返し › sound instance › loopingと遅延再生</summary>

- 比較環境：クライアント共通
- 対象条件：looping=trueまたはdelay&gt;0のSoundInstance相当を要求するケース。通常の単発packet soundは対象外。
- 残件 / 比較すべき点：該当instance/APIをRust側で扱う必要がある機能範囲と、公式の手動loop delayを比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/sounds/SoundEngine.java:320](../../../minecraft-26.2-decompiled/src/net/minecraft/client/sounds/SoundEngine.java#L320) — shouldLoopManually / shouldLoopAutomatically：loopingとdelayによる再生方式を判定する。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/sounds/SoundEngine.java:428](../../../minecraft-26.2-decompiled/src/net/minecraft/client/sounds/SoundEngine.java#L428) — SoundEngine.play：sourceへautomatic loopingを適用する。
  - Rust：[Client/pomme-client/src/audio/mod.rs:538](../../pomme-client/src/audio/mod.rs#L538) — AudioEngine::play_positioned_sound：world soundのPlayCommandはlooping=false固定。
  - Rust：[Client/pomme-client/src/audio/mod.rs:450](../../pomme-client/src/audio/mod.rs#L450) — AudioEngine::play_ui_sound / play_menu_track：UIとmenu music経路もlooping=falseで、delay付きinstanceの入口はない。

</details>

<a id="audio-07"></a>

<details>
<summary>audio-07 — 音楽 › メニューBGM › 開始待ち・曲間待ち</summary>

- 比較環境：接続前
- 対象条件：タイトル画面のmenu music。ゲーム状況に応じるworld music選択はRustこの範囲で未実装。
- 残件 / 比較すべき点：menu曲の初回/終了後delayと、接続後のsituational music・置換・音量fadeを別々に比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/sounds/MusicManager.java:33](../../../minecraft-26.2-decompiled/src/net/minecraft/client/sounds/MusicManager.java#L33) — MusicManager.tick：situational music、min/max delayとfrequencyを元に曲を管理する。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/sounds/MusicManager.java:107](../../../minecraft-26.2-decompiled/src/net/minecraft/client/sounds/MusicManager.java#L107) — MusicManager.fadePlaying：音楽gain変更時のfade処理。
  - Rust：[Client/pomme-client/src/audio/mod.rs:671](../../pomme-client/src/audio/mod.rs#L671) — AudioEngine::update_menu_music：tick換算でmenu music開始待ちを進める。
  - Rust：[Client/pomme-client/src/audio/mod.rs:689](../../pomme-client/src/audio/mod.rs#L689) — AudioEngine::play_menu_track：music.menuをMusicカテゴリーで再生し、終了報告後に遅延する。
  - Rust：[Client/pomme-client/src/app/phases/in_menu.rs:40](../../pomme-client/src/app/phases/in_menu.rs#L40) — in_menu update：メニュー更新からmenu musicを開始/更新する。

</details>

<a id="audio-08"></a>

<details>
<summary>audio-08 — 字幕 › sound event subtitle › 再生通知と距離範囲</summary>

- 比較環境：クライアント共通
- 対象条件：subtitle付きのpositional event。relative/UI音とattenuation無効音は公式の無限範囲挙動との違いを確認する対象。
- 残件 / 比較すべき点：字幕の通知対象がrelative/attenuation無効/カテゴリ音量0のケースで公式と同じか確認し、UI soundを含む条件差を確定する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/sounds/SoundEngine.java:384](../../../minecraft-26.2-decompiled/src/net/minecraft/client/sounds/SoundEngine.java#L384) — SoundEngine.play：再生listenerへ通知し、relative/attenuation条件でrangeを選ぶ。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/components/SubtitleOverlay.java:117](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/components/SubtitleOverlay.java#L117) — SubtitleOverlay.onPlaySound：字幕keyがある音の位置を記録し、既存字幕をrefreshする。
  - Rust：[Client/pomme-client/src/audio/mod.rs:497](../../pomme-client/src/audio/mod.rs#L497) — AudioEngine::play_positioned_sound：positional event解決後、volume culling前に字幕queueへ範囲を記録する。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:5959](../../pomme-client/src/app/phases/in_game.rs#L5959) — in_game subtitle event drain：queued字幕をSubtitleOverlayStateへ接続する。

</details>

<a id="audio-09"></a>

<details>
<summary>audio-09 — 字幕 › subtitle overlay › 表示時間と距離矢印</summary>

- 比較環境：クライアント共通
- 対象条件：同じ字幕キーが複数位置で再生され、通知表示時間設定を変更する場合
- 残件 / 比較すべき点：設定倍率変更時の字幕寿命・fade・複数位置からの最寄り矢印を比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/components/SubtitleOverlay.java:64](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/components/SubtitleOverlay.java#L64) — SubtitleOverlay.extractRenderState：notificationDisplayTime倍率付きで再生位置をpurgeする。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/components/SubtitleOverlay.java:117](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/components/SubtitleOverlay.java#L117) — SubtitleOverlay.onPlaySound / Subtitle.refresh：subtitle文言単位で位置を集約し、再生位置を更新する。
  - Rust：[Client/pomme-client/src/ui/subtitles.rs:13](../../pomme-client/src/ui/subtitles.rs#L13) — DISPLAY_TIME_MS：表示寿命を3000ms固定している。
  - Rust：[Client/pomme-client/src/ui/subtitles.rs:72](../../pomme-client/src/ui/subtitles.rs#L72) — SubtitleOverlayState::on_play_sound / build：同一keyで位置を集約し、nearest sourceと矢印/fadeを描画する。

</details>

<a id="audio-10"></a>

<details>
<summary>audio-10 — 環境音 › クライアントローカル効果 › portal ambient trigger</summary>

- 比較環境：内蔵シングルプレイ
- 対象条件：ローカルplayerがportal内にいて該当trigger条件を満たす場合。biome ambience全般は対象外。
- 残件 / 比較すべき点：同じ状態でイベント頻度、発生位置、volume/pitch分布、停止/離脱条件を比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/player/LocalPlayer.java:915](../../../minecraft-26.2-decompiled/src/net/minecraft/client/player/LocalPlayer.java#L915) — LocalPlayer portal effect sound：portalEffectIntensityが0の場合にPORTAL_TRIGGERをforLocalAmbienceで再生する。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:4375](../../pomme-client/src/app/phases/in_game.rs#L4375) — portal local ambience trigger：Rust側のportal効果成立時のローカル発火処理。
  - Rust：[Client/pomme-client/src/audio/mod.rs:460](../../pomme-client/src/audio/mod.rs#L460) — AudioEngine::play_world_sound：Rustのローカル位置付きsound再生経路。

</details>


<a id="domain-connection"></a>

## 接続・プロトコル基盤

調査範囲：指定されたClient/pomme-client/src/net/{connection,conn,resolve,known_packs,native_codecs,translate}.rs と pomme-protocol の version/wire/packets/registries を読み、入口から接続・設定処理・利用箇所まで静的に追跡した。公式26.2のclient status/login/configuration、Connection圧縮・暗号化、known packとnetwork registry loadを参照した。実サーバー接続・ビルド・テストは行っておらず、play packet個別処理、packダウンロード、launcher、1.20〜26.3の全版検証は範囲外。

- 制約：静的調査のみ。接続試験、cargo build/test、公式serverとのpacket captureは実施していないため、一致を確認した行はない。
- 制約：26.2以外の版への実装あり判定は、version table/translation経路が存在する範囲の限定判定であり、1.20〜26.3全版での動作保証ではない。
- 制約：Known-pack dataは26.2のみembedded。other versionのselect_known_packs応答やknown-pack registry補完はこの実装から確認できない。
- 制約：play packet具体処理、packダウンロード、launcherは担当外。native_codecs.rsは担当範囲として読み込んだが、今回の行では個別play packet補正を評価していない。

| ID / 機能 | 公式の挙動 | Rustの現状・差分 | 実装 | 比較 / 検証 |
|---|---|---|---|---|
| [connection-01](#connection-01)<br>接続 › status応答 › プロトコル番号取得と接続時のwire版選択 | 公式status応答のversionにname/protocolを含め、クライアントはそのprotocolをサーバー一覧情報として扱う。 | join時は既知protocolまたは最大5秒のstatus probeからwire版を選び、joinableでない場合は起動版へフォールバックし、それもjoin不可なら接続を拒否する。 | 実装あり | 未検証<br>静的照合 |
| [connection-02](#connection-02)<br>接続 › DNS/TCP › 複数候補を試してhandshakeを送る | 公式クライアントは解決したサーバーアドレスへ接続し、handshakeでホスト・ポート・接続目的をサーバーへ送る。 | IP literalはDNSを省略し、hostnameはSRV系とsystem resolverを並行実行、IPv4優先・重複排除して最大5秒ずつ接続を試し、成功時にhandshake intentionを送る。 | 実装あり | 未検証<br>静的照合 |
| [connection-03](#connection-03)<br>login › 暗号化要求 › セッション認証後に共有鍵を有効化 | 公式は暗号鍵を生成し、shouldAuthenticate=trueならsession serverへ認証してから鍵packetを送り、送信完了後に暗号化を有効にする。falseならsession server認証を省略する。 | Rustはhelloのshould_authenticateに従い、必要ならaccess tokenでsessionserver join後にServerboundKeyを書き、AES cipherを接続へ設定する。認証要求があるのにtokenがない場合は失敗する。 | 実装あり | 未検証<br>静的照合 |
| [connection-04](#connection-04)<br>login › 圧縮閾値packet › 送受信frameの圧縮切替 | 公式はlogin compression packetのthresholdを受け取り、remote接続の送受信圧縮器を設定する。threshold未満は非圧縮、threshold以上はzlib圧縮を使う。 | Rustはlogin packetを処理中にreader/writer双方のthresholdを設定し、圧縮frameをzlib decode/encodeする。受信時はthreshold未満、宣言長超過、実際長不一致などを拒否する。 | 実装あり | 未検証<br>静的照合 |
| [connection-05](#connection-05)<br>login › cookie/custom query › 受信要求への応答 | 公式login listenerはcustom queryにtransaction idを保ったnull answerを返し、cookie要求には該当cookieがあればそのpayload、なければ不在を返す。 | Rust login sequenceはcustom queryへdata=None、cookie requestへ保存cookieのOptionを返す。configuration sequenceでもcookie request/store cookieを扱う。 | 実装あり | 未検証<br>静的照合 |
| [connection-06](#connection-06)<br>configuration › login完了後 › acknowledgementと初期client情報送信 | 公式はlogin完了後configuration listenerへ移行し、login acknowledgedを送り、brandとclient informationを送信する。 | Rustはprofile受領後、config phaseありならlogin acknowledgedを送り、config_sequence開始時にbrandとclient informationを一度送信する。 | 実装あり | 未検証<br>静的照合 |
| [connection-07](#connection-07)<br>configuration › finish › registry収集後にgame phaseへ移行 | 公式はregistry data/tagsをcollectorへ蓄積し、finish configuration受信時にregistriesをcollectしてgame listenerへ切替え、finish acknowledgementを返す。 | Rustはconfiguration packetを処理し、FinishConfiguration受信後にfinish packetを返しConfigured stateをgame loopへ渡す。mid-session reconfiguration時は新規registry dataがなければ前のholderを保つ。 | 実装あり | 未検証<br>静的照合 |
| [connection-08](#connection-08)<br>configuration › select known packs › 提供packの選択応答 | 公式KnownPacksManagerは利用可能なknown packとの完全一致をサーバー提示順に選び、選択したpackをrepositoryへ設定して応答する。 | Rustは現在session protocolにknown-pack tableがある場合だけnamespace/id/version一致分を提示順に返す。embedded known-pack dataは26.2だけで、他versionは空選択になる。 | 一部実装 | 未検証<br>静的照合 |
| [connection-09](#connection-09)<br>configuration › known-pack registry entries › データ省略分を補完してregistryへ登録 | 公式NetworkRegistryLoadTaskはnetwork data付きentryをNBT decodeし、dataなしentryは選択済みknown-pack resourceからJSONを読み、registryに登録する。 | Rustはknown packを一つでも選択した場合、registry dataのNone entryをembedded JSONからNBTへ変換してからRegistryHolderへ追加する。対象elementが見つからないと接続を切る。 | 一部実装 | 未検証<br>静的照合 |
| [connection-10](#connection-10)<br>版変換 › version別wire packetとregistry IDをnative空間へ変換 | 公式26.2のpacket/registry IDは26.2のprotocol registration順で解釈され、dataを伴うregistry elementはサーバー提供順で利用される。 | Rustは26.2をnative wireとし、joinableな別protocolではframe/packet変換、静的registry IDの名前ベース相互変換を行う。configuration中のdynamic registryはembedded静的tableから推測せず、server entry順を保持し重複名を未解決にする。 | 一部実装 | 未検証<br>静的照合 |
| [connection-11](#connection-11)<br>login/configuration › 1.20.1系のconfigなしserverをnative処理へ接続 | 公式26.2ではconfiguration phaseに移行しlogin acknowledgedを送り、registry dataをconfigurationで受け取る。 | Rustはwire tableにconfiguration phaseがない場合login acknowledged/config sequenceを飛ばし、game login packet内registryをconfig RegistryData相当へ分割してConfiguredを作り、元login frameをgame loopへ再投入する。 | 一部実装 | 未検証<br>Rust静的確認のみ |

### 条件・残件・根拠

<a id="connection-01"></a>

<details>
<summary>connection-01 — 接続 › status応答 › プロトコル番号取得と接続時のwire版選択</summary>

- 比較環境：接続前
- 対象条件：既知protocolなしでremote serverへjoinする
- 残件 / 比較すべき点：status probe失敗・未知protocol・既知protocol陳腐化の各ケースで、選択wire版とサーバーの拒否/接続結果を比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ServerStatusPinger.java:78](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ServerStatusPinger.java#L78) — handleStatusResponse：status.versionからprotocolを保存する。
  - Rust：[Client/pomme-client/src/net/connection.rs:624](../../pomme-client/src/net/connection.rs#L624) — negotiate_wire_version：known値またはstatus probeをwire版決定に使う。
  - Rust：[Client/pomme-client/src/net/connection.rs:664](../../pomme-client/src/net/connection.rs#L664) — resolve_wire：join可能性判定と起動版fallbackを行う。
  - Rust：[Client/pomme-client/src/net/resolve.rs:93](../../pomme-client/src/net/resolve.rs#L93) — request_status：Status requestを送りstatus応答を読む。

</details>

<a id="connection-02"></a>

<details>
<summary>connection-02 — 接続 › DNS/TCP › 複数候補を試してhandshakeを送る</summary>

- 比較環境：接続前
- 対象条件：IPv4/IPv6両方の候補を返すremote hostnameへ接続する
- 残件 / 比較すべき点：SRV、DNS timeout、複数候補失敗時の接続先と成功/失敗挙動を同じ環境で比較する。
- 根拠：
  - Rust：[Client/pomme-client/src/net/resolve.rs:27](../../pomme-client/src/net/resolve.rs#L27) — resolve_candidates：解決候補を集約しIPv4優先に整列する。
  - Rust：[Client/pomme-client/src/net/resolve.rs:113](../../pomme-client/src/net/resolve.rs#L113) — send_intention：session protocol、host、port、intentionをhandshake packetへ設定する。
  - Rust：[Client/pomme-client/src/net/resolve.rs:140](../../pomme-client/src/net/resolve.rs#L140) — connect：候補ごとにTCP接続しhandshakeを送信する。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/network/protocol/handshake/ClientIntentionPacket.java:14](../../../minecraft-26.2-decompiled/src/net/minecraft/network/protocol/handshake/ClientIntentionPacket.java#L14) — ClientIntentionPacket：公式handshake packetはprotocolVersion、hostName、port、intentionを持つ。

</details>

<a id="connection-03"></a>

<details>
<summary>connection-03 — login › 暗号化要求 › セッション認証後に共有鍵を有効化</summary>

- 比較環境：公式サーバー接続
- 対象条件：暗号化を要求するserverで有効tokenあり/なしを試す
- 残件 / 比較すべき点：online-mode、offline-mode、認証失敗時の切断、暗号鍵packet直後の暗号化切替を公式serverで比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientHandshakePacketListenerImpl.java:110](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientHandshakePacketListenerImpl.java#L110) — handleHello：shouldAuthenticateに応じて認証後または直接鍵送信へ進む。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientHandshakePacketListenerImpl.java:150](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientHandshakePacketListenerImpl.java#L150) — setEncryption：鍵packet送信完了後に暗号鍵を設定する。
  - Rust：[Client/pomme-client/src/net/connection.rs:752](../../pomme-client/src/net/connection.rs#L752) — handle_encryption：challenge暗号化と必要時のsession server認証を実行する。
  - Rust：[Client/pomme-client/src/net/conn.rs:371](../../pomme-client/src/net/conn.rs#L371) — set_encryption_key：送受信のcipherをセットする。

</details>

<a id="connection-04"></a>

<details>
<summary>connection-04 — login › 圧縮閾値packet › 送受信frameの圧縮切替</summary>

- 比較環境：公式サーバー接続
- 対象条件：threshold未満・一致・超過サイズのpacketを受信する
- 残件 / 比較すべき点：閾値境界、閾値未満の不正圧縮、8MiB上限、負thresholdを含むwire挙動を実機比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientHandshakePacketListenerImpl.java:220](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientHandshakePacketListenerImpl.java#L220) — handleCompression：remote接続でthresholdをConnectionへ設定する。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/network/CompressionDecoder.java:36](../../../minecraft-26.2-decompiled/src/net/minecraft/network/CompressionDecoder.java#L36) — decode：先頭長0は非圧縮、それ以外は展開し宣言長を検査する。
  - Rust：[Client/pomme-client/src/net/connection.rs:713](../../pomme-client/src/net/connection.rs#L713) — login_sequence：LoginCompressionを受信しConnの圧縮閾値を設定する。
  - Rust：[Client/pomme-client/src/net/conn.rs:125](../../pomme-client/src/net/conn.rs#L125) — decode_compressed：宣言長・閾値・zlib終端・展開長を検査する。

</details>

<a id="connection-05"></a>

<details>
<summary>connection-05 — login › cookie/custom query › 受信要求への応答</summary>

- 比較環境：公式サーバー接続
- 対象条件：Login phaseでcustom queryと既知/未知cookieを要求する
- 残件 / 比較すべき点：cookie/custom query responseをserver接続で比較し、null answerとcookie payload有無を確認する。
- 根拠：
  - Rust：[Client/pomme-client/src/net/connection.rs:686](../../pomme-client/src/net/connection.rs#L686) — login_sequence：login packet処理loop。
  - Rust：[Client/pomme-client/src/net/connection.rs:730](../../pomme-client/src/net/connection.rs#L730) — ClientboundLoginPacket::CookieRequest：保存cookieをcookie responseに含める。
  - Rust：[Client/pomme-client/src/net/connection.rs:739](../../pomme-client/src/net/connection.rs#L739) — ClientboundLoginPacket::CustomQuery：transaction idを保ちdataなしの応答を送る。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientHandshakePacketListenerImpl.java:226](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientHandshakePacketListenerImpl.java#L226) — handleCustomQuery：transaction idを保ちnull answerを返す。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientCommonPacketListenerImpl.java:207](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientCommonPacketListenerImpl.java#L207) — handleRequestCookie：cookie keyに対応するcookieをresponseへ渡す。

</details>

<a id="connection-06"></a>

<details>
<summary>connection-06 — configuration › login完了後 › acknowledgementと初期client情報送信</summary>

- 比較環境：公式サーバー接続
- 対象条件：通常の26.2 remote loginからconfigurationへ移る
- 残件 / 比較すべき点：packet順序、brand値、view distance等のclient information各値を公式serverの観測packetと比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientHandshakePacketListenerImpl.java:170](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientHandshakePacketListenerImpl.java#L170) — handleLoginFinished：configuration listenerを作りlogin acknowledged、brand、client informationを送る。
  - Rust：[Client/pomme-client/src/net/connection.rs:253](../../pomme-client/src/net/connection.rs#L253) — connect_recorded：config phaseありの場合にlogin acknowledgedを書き、configurationへ遷移する。
  - Rust：[Client/pomme-client/src/net/connection.rs:849](../../pomme-client/src/net/connection.rs#L849) — config_sequence：初回configuration時にbrandとclient informationを送る。

</details>

<a id="connection-07"></a>

<details>
<summary>connection-07 — configuration › finish › registry収集後にgame phaseへ移行</summary>

- 比較環境：公式サーバー接続
- 対象条件：registry data/tagsを含む通常のinitial configurationを完了する
- 残件 / 比較すべき点：空registry、known packで省略されたregistry、再configurationでdata省略のときのholderとtag状態を公式26.2と比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientConfigurationPacketListenerImpl.java:80](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientConfigurationPacketListenerImpl.java#L80) — handleRegistryData：registry entriesをcollectorに追加する。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientConfigurationPacketListenerImpl.java:160](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientConfigurationPacketListenerImpl.java#L160) — handleConfigurationFinished：registriesを収集しgame listenerへ遷移する。
  - Rust：[Client/pomme-client/src/net/connection.rs:795](../../pomme-client/src/net/connection.rs#L795) — config_sequence：configuration packet loopとfinish処理を行う。
  - Rust：[Client/pomme-client/src/net/connection.rs:885](../../pomme-client/src/net/connection.rs#L885) — FinishConfiguration：終了条件成立時にFinishConfigurationを送信してConfiguredを返す。

</details>

<a id="connection-08"></a>

<details>
<summary>connection-08 — configuration › select known packs › 提供packの選択応答</summary>

- 比較環境：公式サーバー接続
- 対象条件：26.2 serverが既知packを一つ以上提示する
- 残件 / 比較すべき点：26.2 pack tableの完全性と、packの組み合わせ・選択順・未知version時に公式が期待する応答をserver接続で比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientConfigurationPacketListenerImpl.java:94](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientConfigurationPacketListenerImpl.java#L94) — handleSelectKnownPacks：KnownPacksManagerに選択を依頼してserverへ応答する。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/KnownPacksManager.java:31](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/KnownPacksManager.java#L31) — trySelectingPacks：提示順に一致packを選びrepositoryへ設定する。
  - Rust：[Client/pomme-client/src/net/known_packs.rs:17](../../pomme-client/src/net/known_packs.rs#L17) — select_packs：埋め込みtableとnamespace/id/versionを照合し順序保持で選択する。
  - Rust：[Client/pomme-protocol/src/version.rs:62](../../pomme-protocol/src/version.rs#L62) — EMBEDDED：known-packs dataを持つ版は26.2のみ。

</details>

<a id="connection-09"></a>

<details>
<summary>connection-09 — configuration › known-pack registry entries › データ省略分を補完してregistryへ登録</summary>

- 比較環境：公式サーバー接続
- 対象条件：serverが選択済みknown packのregistry entry dataを省略する
- 残件 / 比較すべき点：サーバーが提示した全registry/elementが26.2 embedded tableで補完可能か、型変換と失敗時処理を実接続で確認する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/resources/NetworkRegistryLoadTask.java:48](../../../minecraft-26.2-decompiled/src/net/minecraft/resources/NetworkRegistryLoadTask.java#L48) — load：dataありはnetwork NBT、なしはknownDataSource resourceから読み込む。
  - Rust：[Client/pomme-client/src/net/connection.rs:1015](../../pomme-client/src/net/connection.rs#L1015) — ClientboundConfigPacket::RegistryData：known packs選択時にNone entryを補完してからholderに追加する。
  - Rust：[Client/pomme-client/src/net/known_packs.rs:31](../../pomme-client/src/net/known_packs.rs#L31) — fill_known_entries：None entryをknown-pack tableから取得しJSON→NBTへ変換する。
  - Rust：[Client/pomme-client/src/net/known_packs.rs:70](../../pomme-client/src/net/known_packs.rs#L70) — json_to_nbt：JSON値をNBT tagへ変換する。

</details>

<a id="connection-10"></a>

<details>
<summary>connection-10 — 版変換 › version別wire packetとregistry IDをnative空間へ変換</summary>

- 比較環境：公式サーバー接続
- 対象条件：26.2または変換tableを持つwire版でregistry IDを含むpacketを扱う
- 残件 / 比較すべき点：この調査は全変換codec/registry IDを網羅せず、差分ある版の代表packet、未対応registry項目、duplicate/missing ID条件をそれぞれ比較する必要がある。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientConfigurationPacketListenerImpl.java:80](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientConfigurationPacketListenerImpl.java#L80) — handleRegistryData：server registry dataをcollectorに渡す。
  - Rust：[Client/pomme-client/src/net/translate.rs:832](../../pomme-client/src/net/translate.rs#L832) — joinable：翻訳対応protocolを明示し、tableがあるだけではjoin許可しない。
  - Rust：[Client/pomme-client/src/net/translate.rs:1425](../../pomme-client/src/net/translate.rs#L1425) — remap_inbound：受信typed packetのregistry ID変換入口。
  - Rust：[Client/pomme-protocol/src/registries.rs:171](../../pomme-protocol/src/registries.rs#L171) — DynamicRegistries::replace：dynamic registry IDはserver由来のentry順で保持する。

</details>

<a id="connection-11"></a>

<details>
<summary>connection-11 — login/configuration › 1.20.1系のconfigなしserverをnative処理へ接続</summary>

- 比較環境：公式サーバー接続
- 対象条件：protocol 763相当のconfigなしwire serverでjoinする
- 残件 / 比較すべき点：参照版26.2の公式コードはconfig phaseありのみで、旧版との公式wire比較根拠は今回未確認。対象旧版の公式参照と実serverのregistry/login順を別途照合する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientHandshakePacketListenerImpl.java:170](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientHandshakePacketListenerImpl.java#L170) — handleLoginFinished：26.2ではconfigurationへ切り替えてackを送る。
  - Rust：[Client/pomme-client/src/net/connection.rs:250](../../pomme-client/src/net/connection.rs#L250) — connect_recorded：no_config_phaseでackを省きinline registry処理を選ぶ。
  - Rust：[Client/pomme-client/src/net/connection.rs:537](../../pomme-client/src/net/connection.rs#L537) — read_inline_registries：login frameからregistry dataを読みConfiguredを作り元frameを遅延する。
  - Rust：[Client/pomme-client/src/net/translate.rs:986](../../pomme-client/src/net/translate.rs#L986) — no_config_phase：wire packet tableからconfiguration phase不在を判定する。

</details>


<a id="domain-sync"></a>

## ワールド・通信同期

調査範囲：指定されたClientPacketListenerのplay処理とRust handler→NetworkEvent→app/core.rs/world/chunk.rsの確認可能な接続を照合。主にchunk/light、teleport、entity、time/weather、respawn、keepalive、chunk batchを調査し、difficultyのRust受信処理が見つからない点も記録した。実行・実機比較はしていない。

- 制約：静的なソース照合のみで、ビルド・テスト・クライアント起動・実サーバー比較は実施していない。
- 制約：チャット、inventory、resource pack、particle/audioの個別意味、接続段階の調査は対象外。entity metadataは全kind/indexの網羅を主張しない。
- 制約：difficultyは公式handlerの具体処理とRust側の受信分岐欠落を確認したが、Rustのdecoderが該当packetを未知packetとして扱う際の実行時挙動は未確認。

| ID / 機能 | 公式の挙動 | Rustの現状・差分 | 実装 | 比較 / 検証 |
|---|---|---|---|---|
| [sync-01](#sync-01)<br>world/chunk/light › chunk列 › chunk本体・light同期・unload・cache中心 | 公式はchunkデータとblock entityをchunkへ適用し、chunk/standalone light payloadをlight engineへ反映する。chunk forgetはchunkを除去し、cache center通知はchunk sourceの中心を更新する。 | Rustはdimension height/min_yでchunkをdecodeしblock entityと初期lightを渡す。standalone lightもmask付きで反映し、ForgetLevelChunk時はchunk/light/block entity/mesh等を破棄、cache centerをChunkStoreへ反映する。 | 実装あり | 未検証<br>静的照合 |
| [sync-02](#sync-02)<br>world/chunk/light › 既存chunk › standalone light updateを反映 | 公式は単独light packetのsky/block update mask、empty mask、section値をlight engineへ渡し、chunkのlightを有効にする。 | Rust handlerは座標とlight dataをNetworkEventへ送り、coreは同じsection mask変換を使ってenable=falseのlight correction taskをqueueする。 | 実装あり | 未検証<br>静的照合 |
| [sync-03](#sync-03)<br>teleport › 自プレイヤー › server correctionを適用してteleportを確認 | 公式はrelative flagsを現在値に対して解決し、乗車中でなければ位置等を設定し、AcceptTeleportation(id)と現在位置のPosRotを返す。 | RustはPlayerPositionをmain state eventへ渡し、apply_player_correctionでrelative位置・速度・回転を反映する。乗車状態に関わらずAcceptTeleportation(id)と現在位置/回転のmove packetを送る。 | 実装あり | 未検証<br>静的照合 |
| [sync-04](#sync-04)<br>entity › spawn › entity kind・uuid・座標・回転・速度からentity状態を生成 | 公式はspawn packetからentityを生成し、packet値を使って初期化してlevelへ追加する。 | Rustはspawn値を角度・速度・item frame attachment位置等に変換してevent化し、coreは位置を登録し、living/vehicle/item系entity storeへ登録する。 | 実装あり | 未検証<br>静的照合 |
| [sync-05](#sync-05)<br>entity › movement/teleport › tracked entityの位置・回転・速度更新 | 公式は相対移動/回転をposition codecとentity状態へ適用し、absolute position syncやteleportはrelative flags等を解決してground状態も反映する。 | Rustはdelta move、rotation、teleport、position sync、motion packetを個別eventへ変換し、coreで現在位置/回転/速度をentity storeへ反映する。 | 実装あり | 未検証<br>静的照合 |
| [sync-06](#sync-06)<br>entity › metadata › entity metadata差分を対応する表示・状態へ反映 | 公式は対象entityが存在する場合、packed metadata valuesをentity dataへassignする。 | Rust handlerはmetadata index/typeに応じてpose、display、item display、text display等のeventを生成し、coreがentity store/player状態へ設定する。未対応index/typeの完全な適用はこの照合では確認していない。 | 一部実装 | 未検証<br>静的照合 |
| [sync-07](#sync-07)<br>world/time › server clock › game time・dimension clock更新 | 公式はserver game timeをlevel/telemetryへ適用し、clock managerにclock updatesを渡して環境attribute cacheをinvalidateする。 | Rust handlerはgame_timeとclock updateをeventにし、coreはgame timeと選択dimension clockの値をsky stateに適用する。 | 実装あり | 未検証<br>静的照合 |
| [sync-08](#sync-08)<br>world/environment › weather/game event › server通知を天候・game stateへ反映 | 公式はrain start/stop、rain/thunder level、game mode、immediate respawn等のGameEventを対応するlevel/player状態へ適用する。 | Rust handlerは天候イベントをWeatherUpdateにし、coreはrain/thunder値を更新する。他のeventはGameEventへ流して一部credit/guardian等を処理するが、全eventの公式相当動作は確認していない。 | 一部実装 | 未検証<br>静的照合 |
| [sync-09](#sync-09)<br>player/respawn › spawn情報・保持flagsに従うplayer state reset | 公式はrespawn spawn infoからdimension変更を処理し、data_to_keep flagsに沿って新しいLocalPlayerを作成して新levelへのロードを開始する。 | Rust handlerはkeep entity data/attribute modifiers flagsをevent化する。coreはdimension/player transient、死亡状態、送信baseline等をresetし、player reset_for_respawnへ保持flagを渡す。 | 実装あり | 未検証<br>静的照合 |
| [sync-10](#sync-10)<br>difficulty › server difficulty変更 › level difficulty・lock更新 | 公式はChangeDifficulty packetのdifficultyとlocked値をlevel dataへ設定し、対応画面があれば変更を通知する。 | 調査したhandler.rsのplay packet分岐およびcore.rsのevent処理にdifficulty更新経路を確認できなかった。difficulty enumのworld creation設定は存在するが、これはサーバー通知の受信処理ではない。 | 未実装 | 差分あり<br>静的照合 |
| [sync-11](#sync-11)<br>keepalive › 接続維持 › server idを同じ値で応答 | 公式は受信KeepAlive idを変えずServerboundKeepAliveで応答する（poll event freeze中の送信は最大1分待機する）。 | Rust handlerは受信idを同じ値でServerboundKeepAliveに包んでPacketSender経由で送る。凍結時の遅延条件は今回確認範囲のRust経路では見当たらない。 | 実装あり | 未検証<br>静的照合 |
| [sync-12](#sync-12)<br>chunk batch › server pacing › batch処理時間から次回希望chunk数を返す | 公式はbatch start時刻を記録し、非空batchのnanos/chunkを3倍範囲へclamp、最大旧weight 49で平滑化し、7ms/tick予算からdesired chunks/tickを計算して応答する。 | Rustは同じ初期値2ms、clamp係数3、weight cap 49、7ms予算で計算し、ChunkBatchStart/Finishedを処理してServerboundChunkBatchReceivedを送る。数式はソース上公式と同形だが実測時間/挙動一致は未検証。 | 実装あり | 未検証<br>静的照合 |

### 条件・残件・根拠

<a id="sync-01"></a>

<details>
<summary>sync-01 — world/chunk/light › chunk列 › chunk本体・light同期・unload・cache中心</summary>

- 比較環境：公式サーバー接続
- 対象条件：chunk load/light update/unloadとcache center通知を受信する
- 残件 / 比較すべき点：複数dimension・negative section・empty light mask・境界chunk unload後のmesh/light結果を公式と比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java:843](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java#L843) — handleLevelChunkWithLight：chunk packet更新後にlight適用をqueueし、chunk lightingを有効化する。
  - Rust：[Client/pomme-client/src/net/handler.rs:482](../../pomme-client/src/net/handler.rs#L482) — handle_game_packet / LevelChunkWithLight：dimension高さでdecodeし、block entityとlightをChunkLoadedに束ねる。
  - Rust：[Client/pomme-client/src/app/core.rs:2324](../../pomme-client/src/app/core.rs#L2324) — queue_light_apply：sky/block maskとempty maskをlight section entriesにしてApplyLight taskへ渡す。
  - Rust：[Client/pomme-client/src/app/core.rs:4306](../../pomme-client/src/app/core.rs#L4306) — NetworkEvent::ChunkUnloaded：chunk storeと関連light/mesh等を破棄する。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java:2327](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java#L2327) — handleSetChunkCacheCenter：cache centerをchunk sourceへ適用する。

</details>

<a id="sync-02"></a>

<details>
<summary>sync-02 — world/chunk/light › 既存chunk › standalone light updateを反映</summary>

- 比較環境：公式サーバー接続
- 対象条件：ロード済みchunkに対するstandalone LightUpdateを受信する
- 残件 / 比較すべき点：empty section、skyなしdimension、既存chunk外の更新を含めて公式との適用結果を比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java:2281](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java#L2281) — handleLightUpdatePacket：standalone packetのlight dataを適用queueへ送り、light enabledにする。
  - Rust：[Client/pomme-client/src/net/handler.rs:628](../../pomme-client/src/net/handler.rs#L628) — handle_game_packet / LightUpdate：LightUpdateを座標とlight payload付きNetworkEventへ変換する。
  - Rust：[Client/pomme-client/src/app/core.rs:4303](../../pomme-client/src/app/core.rs#L4303) — NetworkEvent::LightUpdate：standalone更新をenable=falseで同じlight apply経路へ渡す。
  - Rust：[Client/pomme-client/src/net/mod.rs:53](../../pomme-client/src/net/mod.rs#L53) — PacketLightData::from：sky/block更新配列と通常/empty masksを保持する。

</details>

<a id="sync-03"></a>

<details>
<summary>sync-03 — teleport › 自プレイヤー › server correctionを適用してteleportを確認</summary>

- 比較環境：公式サーバー接続
- 対象条件：自プレイヤーが徒歩または乗車中にPlayerPosition correctionを受信する
- 残件 / 比較すべき点：乗車時の位置更新、境界値・relative flags、確認応答後のserver echoを公式と比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java:792](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java#L792) — handleMovePlayer：乗車時以外に位置補正し、teleport acceptとPosRotを送信する。
  - Rust：[Client/pomme-client/src/net/handler.rs:644](../../pomme-client/src/net/handler.rs#L644) — handle_game_packet / PlayerPosition：position correctionとrelative flagsをPlayerPosition eventとして配送する。
  - Rust：[Client/pomme-client/src/app/core.rs:4334](../../pomme-client/src/app/core.rs#L4334) — NetworkEvent::PlayerPosition：ゲーム状態側でcorrectionを適用し、teleport後処理とchunk centerを更新する。
  - Rust：[Client/pomme-client/src/app/core.rs:2863](../../pomme-client/src/app/core.rs#L2863) — apply_player_correction：relative補正後にAcceptTeleportationと位置/回転packetを送る。

</details>

<a id="sync-04"></a>

<details>
<summary>sync-04 — entity › spawn › entity kind・uuid・座標・回転・速度からentity状態を生成</summary>

- 比較環境：公式サーバー接続
- 対象条件：通常mobまたはitem frameのAddEntityを受信する
- 残件 / 比較すべき点：対象entity type別の初期metadata/attachment、未登録player info時の挙動、およびspawn直後の表示を比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java:576](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java#L576) — handleAddEntity：entityをpacketから生成しrecreateFromPacket後levelへ追加する。
  - Rust：[Client/pomme-client/src/net/handler.rs:28](../../pomme-client/src/net/handler.rs#L28) — entity_spawn_event：packetのposition、velocity、回転、item frame方向をspawn eventへ変換する。
  - Rust：[Client/pomme-client/src/net/handler.rs:1382](../../pomme-client/src/net/handler.rs#L1382) — handle_game_packet / AddEntity：spawn eventをネットワークevent queueへ送る。
  - Rust：[Client/pomme-client/src/app/core.rs:5545](../../pomme-client/src/app/core.rs#L5545) — NetworkEvent::EntitySpawned：entity位置記録後、livingやvehicle等のstoreへ生成する。

</details>

<a id="sync-05"></a>

<details>
<summary>sync-05 — entity › movement/teleport › tracked entityの位置・回転・速度更新</summary>

- 比較環境：公式サーバー接続
- 対象条件：spawn済みentityがmove、teleport、motion packetを受信する
- 残件 / 比較すべき点：補間距離しきい値、unknown entity、relative teleport、on-ground状態の公式との差と描画時系列を比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java:647](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java#L647) — handleEntityPositionSync：absolute position syncをentityに適用し、状況に応じて補間/snapする。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java:732](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java#L732) — handleMoveEntity：delta位置/回転をentity position codecとentityへ反映する。
  - Rust：[Client/pomme-client/src/net/handler.rs:1409](../../pomme-client/src/net/handler.rs#L1409) — handle_game_packet / MoveEntityPos：相対移動packetをentity moved eventへ変換し、teleport/sync分岐も存在する。
  - Rust：[Client/pomme-client/src/app/core.rs:5652](../../pomme-client/src/app/core.rs#L5652) — NetworkEvent::EntityMoved：entity store/item entity/位置追跡にdeltaを適用する。
  - Rust：[Client/pomme-client/src/app/core.rs:5730](../../pomme-client/src/app/core.rs#L5730) — NetworkEvent::EntityTeleported：known entityの現在値とrelative data等を使いteleportを適用する。

</details>

<a id="sync-06"></a>

<details>
<summary>sync-06 — entity › metadata › entity metadata差分を対応する表示・状態へ反映</summary>

- 比較環境：公式サーバー接続
- 対象条件：spawn済みentityへposeまたはdisplay metadata更新が届く
- 残件 / 比較すべき点：entity kindごとの全metadata index/value type、unknown entity、初回metadataと差分更新を網羅比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java:639](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java#L639) — handleSetEntityData：entityが存在すればpacked metadata valuesを割り当てる。
  - Rust：[Client/pomme-client/src/net/handler.rs:1537](../../pomme-client/src/net/handler.rs#L1537) — handle_game_packet / SetEntityData：metadata indexと値typeを分類してentity metadata event群へ送る。
  - Rust：[Client/pomme-client/src/app/core.rs:6140](../../pomme-client/src/app/core.rs#L6140) — NetworkEvent::EntityData：entity/player metadataを実際の状態へ適用する。

</details>

<a id="sync-07"></a>

<details>
<summary>sync-07 — world/time › server clock › game time・dimension clock更新</summary>

- 比較環境：公式サーバー接続
- 対象条件：dimensionに対応するclock updateを含むSetTimeを受信する
- 残件 / 比較すべき点：clock未登録/欠落時、partial tick・rateの推移、日周表示を同条件で比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java:1109](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java#L1109) — handleSetTime：game timeとclock updatesを適用しenvironment cacheをinvalidateする。
  - Rust：[Client/pomme-client/src/net/handler.rs:1291](../../pomme-client/src/net/handler.rs#L1291) — handle_game_packet / SetTime：game time、clock id、ticks、partial tick、rateをTimeUpdateへ渡す。
  - Rust：[Client/pomme-client/src/app/core.rs:5447](../../pomme-client/src/app/core.rs#L5447) — NetworkEvent::TimeUpdate：sky stateのgame timeと選択されたclock状態を更新する。

</details>

<a id="sync-08"></a>

<details>
<summary>sync-08 — world/environment › weather/game event › server通知を天候・game stateへ反映</summary>

- 比較環境：公式サーバー接続
- 対象条件：rain level変更または通常のGameEventを受信する
- 残件 / 比較すべき点：各event種別の副作用、特にimmediate respawn・limited crafting・special soundsを公式と個別比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java:1488](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java#L1488) — handleGameEvent：weather、game mode、respawn設定等をevent別に反映する。
  - Rust：[Client/pomme-client/src/net/handler.rs:1331](../../pomme-client/src/net/handler.rs#L1331) — handle_game_packet / GameEvent：game mode、chunk loading、weatherを分岐し、残りは汎用event化する。
  - Rust：[Client/pomme-client/src/app/core.rs:5469](../../pomme-client/src/app/core.rs#L5469) — NetworkEvent::WeatherUpdate：rain/thunder levelsをsky stateへ反映する。
  - Rust：[Client/pomme-client/src/app/core.rs:5490](../../pomme-client/src/app/core.rs#L5490) — NetworkEvent::GameEvent：一部固有処理後にgame stateへeventを適用する。

</details>

<a id="sync-09"></a>

<details>
<summary>sync-09 — player/respawn › spawn情報・保持flagsに従うplayer state reset</summary>

- 比較環境：公式サーバー接続
- 対象条件：死亡後またはdimension移動後にRespawn packetを受信する
- 残件 / 比較すべき点：dimension切替、両data_to_keep bitの全組合せ、保持される属性/効果/位置を公式と比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java:1218](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java#L1218) — handleRespawn：spawn infoでlevel/playerを再構成しpacket keep flagを参照する。
  - Rust：[Client/pomme-client/src/net/handler.rs:2126](../../pomme-client/src/net/handler.rs#L2126) — handle_game_packet / Respawn：data_to_keepの各bitをPlayerRespawnedへ変換する。
  - Rust：[Client/pomme-client/src/app/core.rs:7056](../../pomme-client/src/app/core.rs#L7056) — NetworkEvent::PlayerRespawned：dimension/transient stateと送信baselineをresetしplayerをrespawnする。

</details>

<a id="sync-10"></a>

<details>
<summary>sync-10 — difficulty › server difficulty変更 › level difficulty・lock更新</summary>

- 比較環境：公式サーバー接続
- 対象条件：接続中にserverからdifficulty変更packetを受信する
- 残件 / 比較すべき点：Rustのpacket decoderがChangeDifficultyをどのvariantとして扱うかと、未知packet時の実際の接続挙動を確認する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java:1788](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java#L1788) — handleChangeDifficulty：difficultyとlocked flagをlevel dataに設定する。
  - Rust：[Client/pomme-client/src/net/handler.rs:404](../../pomme-client/src/net/handler.rs#L404) — handle_game_packet：play packet dispatchを確認したがdifficulty更新caseがなく、比較範囲の処理で更新eventは生成されない。
  - Rust：[Client/pomme-client/src/app/core.rs:4283](../../pomme-client/src/app/core.rs#L4283) — NetworkEvent dispatch loop：確認した同期event処理群にdifficulty更新eventの利用経路は見つからなかった。

</details>

<a id="sync-11"></a>

<details>
<summary>sync-11 — keepalive › 接続維持 › server idを同じ値で応答</summary>

- 比較環境：公式サーバー接続
- 対象条件：通常通信中およびrender event pollingが停止する状況でKeepAliveを受信する
- 残件 / 比較すべき点：通常時の往復応答に加え、render freeze時に公式同様の遅延/timeout動作が必要か比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientCommonPacketListenerImpl.java:146](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientCommonPacketListenerImpl.java#L146) — handleKeepAlive：idを維持したresponseをfreeze predicateと1分timeout付きで送る。
  - Rust：[Client/pomme-client/src/net/handler.rs:678](../../pomme-client/src/net/handler.rs#L678) — handle_game_packet / KeepAlive：受信idでServerboundKeepAliveを即時queueする。
  - Rust：[Client/pomme-client/src/net/sender.rs:134](../../pomme-client/src/net/sender.rs#L134) — PacketSender::send：serverbound packetをOutbound queueへ渡す。

</details>

<a id="sync-12"></a>

<details>
<summary>sync-12 — chunk batch › server pacing › batch処理時間から次回希望chunk数を返す</summary>

- 比較環境：公式サーバー接続
- 対象条件：複数のchunk batch（空batch含む）を受信する
- 残件 / 比較すべき点：実測所要時間の開始境界、空batch、複数回後の希望値が公式と同じになるか比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java:2352](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java#L2352) — handleChunkBatchStart：batch startを計算器に通知する。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ChunkBatchSizeCalculator.java:7](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ChunkBatchSizeCalculator.java#L7) — ChunkBatchSizeCalculator：公式の初期値、clamp、weight、7ms予算を実装する。
  - Rust：[Client/pomme-client/src/net/chunk_batch.rs:10](../../pomme-client/src/net/chunk_batch.rs#L10) — ChunkBatchSizeCalculator：公式と同値の定数と算定式を実装する。
  - Rust：[Client/pomme-client/src/net/handler.rs:697](../../pomme-client/src/net/handler.rs#L697) — handle_game_packet / ChunkBatchFinished：batch時間を更新してdesired_chunks_per_tickの応答packetを送る。

</details>


<a id="domain-packs"></a>

## リソースパック

調査範囲：Rust側は指定されたresource_pack.rs、assets.rs、lang.rs、ui/font.rs、net/connection.rs、app/core.rsに加え、呼出先のworld/block/model.rs、renderer/chunk/atlas.rs、audio/sounds.rs、app/phases/in_game.rsを照合。公式26.2側はPackRepository/MultiPackResourceManager、サーバーパック管理・通知、model/blockstate、font/language、soundの該当実装を確認。具体的な描画品質と音の再生結果、全pack format・全providerの網羅検証は対象外。

- 制約：Rustのunit testは存在していても本調査では実行していない。ビルド、cargo test、アプリ起動は禁止指示により未実施。
- 制約：描画結果・音声再生の体感比較は担当外。texture animationはRust処理を確認したが、公式TextureAtlas側との行単位照合は未完了のためRust静的確認のみとした。
- 制約：公式とRustの同一packファイルを使った実機比較は行っていない。全model形式、font provider、pack formatの網羅性は主張しない。

| ID / 機能 | 公式の挙動 | Rustの現状・差分 | 実装 | 比較 / 検証 |
|---|---|---|---|---|
| [packs-01](#packs-01)<br>Resource packs › local/server stack › higher-priority asset resolution | 公式は選択されたpackを順序付きでResourceManagerへ積み、resource stackと単一resource取得で上位packを優先する。 | Rustは有効化順のpack rootを保持し、asset解決では後ろから探索するため、後から有効化したlocal/server packが優先される。 | 実装あり | 未検証<br>静的照合 |
| [packs-02](#packs-02)<br>Resource packs › local selection › persisted order and activation | 公式は保存されたresource pack IDを選択順として復元し、存在しないpackを選択結果から除外する。 | Rustはoptions.jsonのresource_packsを読み、無効名・重複を除去して順序を保ち、各packを有効化する。UI上の変更はreload_assetsが立った時に資産再構築へ接続する。 | 実装あり | 未検証<br>静的照合 |
| [packs-03](#packs-03)<br>Server resource packs › consent › accept or decline push | 公式はサーバーpack設定が許可ならpromptなしで受諾し、拒否設定ならdecline、prompt設定なら同意画面を出す。 | Rustはpushをprompt queueへ入れ、ユーザーのaccept/decline操作でAccepted/Declinedを送る。接続先ごとの許可・拒否設定によるprompt省略経路は確認できない。 | 一部実装 | 差分あり<br>静的照合 |
| [packs-04](#packs-04)<br>Server resource packs › download and load failure feedback | 公式はダウンロード失敗・適用失敗をpack単位の最終結果として通知し、適用reload失敗時は復旧処理へ進む。 | RustはHTTP/hash/ZIP/metadata失敗をFailedDownloadで応答し、required packの失敗では切断理由を設定する。reload失敗後に元の有効stackへ戻す回復経路はこの適用処理で確認できない。 | 一部実装 | 差分あり<br>静的照合 |
| [packs-05](#packs-05)<br>Server resource packs › push/pop › apply, remove, and live reload | 公式はUUID単位でpushされたpackをstackへ適用し、ID指定popまたはpop-allで外してreloadする。 | RustはUUID単位でserver packを差し替え・削除し、push成功後はresource reloadを終えてから成功応答を送る。pop/pop-allも削除があった場合にlive assetを再構築する。 | 実装あり | 未検証<br>静的照合 |
| [packs-06](#packs-06)<br>Models and blockstates › resource-pack definitions › load layered blockstate data | 公式は選択packのblockstates resource stackを読み、上位pack定義を後から適用してblock state modelを構築する。モデル定義もResourceManagerから列挙・解決する。 | Rustのblock model loaderは有効packのminecraft/blockstatesを列挙し、優先packのファイルを解決して既存のRustモデル/mesh経路へ渡す。公式の全model codec・blockstate形式と同等であることまでは示せない。 | 一部実装 | 未検証<br>静的照合 |
| [packs-07](#packs-07)<br>Textures and animation › pack override › texture metadata and frames | 公式はtexture画像と同じresource stackのtexture metadataを読み、画像のanimation metadataに応じてframeを処理する。 | Rustはtextureとmcmetaをpack優先度で解決し、frame layout/animation metadataをtexture atlasのSourceへ反映する。アニメーション再生の見た目・全metadata値の一致は未確認。 | 実装あり | 未検証<br>Rust静的確認のみ |
| [packs-08](#packs-08)<br>Fonts › resource-pack font definitions › providers and options | 公式はfont定義のresource stackからproviderを読み、reference依存を解決し、Force Unicode/Japanese glyph variant optionsをprovider filterに適用する。 | Rustはactive packを含むfont IDを発見し、font JSON/provider/referenceを読み込む経路を持つが、FontOptionsはdefault固定でForce Unicode/Japanese Glyph VariantsはTODO。 | 一部実装 | 差分あり<br>静的照合 |
| [packs-09](#packs-09)<br>Language › selected locale › pack translation merging and fallback | 公式は選択言語とen_usをfallback stackとして全namespace・全packから読み、上位resourceのtranslation値で同じkeyを上書きする。 | Rustは英語をjar/indexから、日本語をasset indexから一度だけ読み込み、en_us/ja_jpの二択で英語fallbackする。active resource packのlang JSONと任意localeはカタログへ取り込まない。 | 一部実装 | 差分あり<br>静的照合 |
| [packs-10](#packs-10)<br>Sounds › sounds.json › layered event registration and replace | 公式はnamespaceごとにsounds.jsonのresource stackを読み、イベント登録を順に適用し、replace指定で既存event登録を置換する。 | Rustはbuilt-inとactive packのsounds.jsonを優先順に読み、同名eventへ既定でentriesを追記しreplace=trueでイベント全体を置換する。 | 実装あり | 未検証<br>静的照合 |

### 条件・残件・根拠

<a id="packs-01"></a>

<details>
<summary>packs-01 — Resource packs › local/server stack › higher-priority asset resolution</summary>

- 比較環境：クライアント共通
- 対象条件：同一asset keyを複数の有効packに置き、上位・下位の内容を切り替える
- 残件 / 比較すべき点：同一assetを重複させ、Rustの有効化順と公式の選択順で最終採用packを比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/server/packs/resources/MultiPackResourceManager.java:43](../../../minecraft-26.2-decompiled/src/net/minecraft/server/packs/resources/MultiPackResourceManager.java#L43) — MultiPackResourceManager(PackType,List&lt;PackResources&gt;)：pack列を順にnamespace managerへ登録する。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/server/packs/repository/PackRepository.java:83](../../../minecraft-26.2-decompiled/src/net/minecraft/server/packs/repository/PackRepository.java#L83) — openAllSelected：選択packをselected順で開く。
  - Rust：[Client/pomme-client/src/resource_pack.rs:88](../../pomme-client/src/resource_pack.rs#L88) — active_pack_dirs：active_packsを低優先から高優先の順で公開。
  - Rust：[Client/pomme-client/src/assets.rs:119](../../pomme-client/src/assets.rs#L119) — resolve_asset_path_with_packs：manager側の逆順探索結果を組み込みassetより優先する。

</details>

<a id="packs-02"></a>

<details>
<summary>packs-02 — Resource packs › local selection › persisted order and activation</summary>

- 比較環境：接続前
- 対象条件：保存済みpack順に複数packを起動時復元し、UIから有効化・無効化する
- 残件 / 比較すべき点：公式OptionsのID復元・position制約と、Rustの順序・不在IDの扱いを同じ保存内容で照合する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/server/packs/repository/PackRepository.java:37](../../../minecraft-26.2-decompiled/src/net/minecraft/server/packs/repository/PackRepository.java#L37) — reload：現在の選択名を再発見packから復元する。
  - Rust：[Client/pomme-client/src/resource_pack.rs:42](../../pomme-client/src/resource_pack.rs#L42) — ResourcePackManager::new：options.jsonのresource_packsを読み込み正規化後に有効化する。
  - Rust：[Client/pomme-client/src/resource_pack.rs:367](../../pomme-client/src/resource_pack.rs#L367) — normalize_local_pack_selection：順序を保って重複・安全でない名前を除去する。
  - Rust：[Client/pomme-client/src/app/core.rs:4020](../../pomme-client/src/app/core.rs#L4020) — apply_pending_pack_changes：local toggleを管理器へ適用し、reload_assets要求を実資産reloadへつなぐ。

</details>

<a id="packs-03"></a>

<details>
<summary>packs-03 — Server resource packs › consent › accept or decline push</summary>

- 比較環境：公式サーバー接続
- 対象条件：サーバーがrequired/optional pack pushを送り、保存済み許可設定の有無を変える
- 残件 / 比較すべき点：保存済みserver-pack許可設定に応じたprompt省略・自動拒否の公式挙動をRust接続UIと比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientCommonPacketListenerImpl.java:170](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientCommonPacketListenerImpl.java#L170) — handleResourcePackPush：server statusとrequired条件で自動処理かprompt表示かを分岐する。
  - Rust：[Client/pomme-client/src/net/connection.rs:1170](../../pomme-client/src/net/connection.rs#L1170) — config_sequence ResourcePackPush arm：push内容をResourcePackPush eventとして通知する。
  - Rust：[Client/pomme-client/src/app/core.rs:7107](../../pomme-client/src/app/core.rs#L7107) — drain_network_events ResourcePackPush arm：受信pushをprompt queueへ登録する。
  - Rust：[Client/pomme-client/src/app/core.rs:4044](../../pomme-client/src/app/core.rs#L4044) — accept_server_pack / decline_server_pack：明示的な操作がAcceptedまたはDeclined応答を送る。

</details>

<a id="packs-04"></a>

<details>
<summary>packs-04 — Server resource packs › download and load failure feedback</summary>

- 比較環境：公式サーバー接続
- 対象条件：不正URL/hash/ZIPまたはreload不能なrequired・optional packを返す
- 残件 / 比較すべき点：同じ失敗を起こし、optional/required別の応答、旧pack維持、ユーザー向け通知、切断の差を確認する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/resources/server/ServerPackManager.java:231](../../../minecraft-26.2-decompiled/src/net/minecraft/client/resources/server/ServerPackManager.java#L231) — triggerReloadIfNeeded：reload失敗時にactivation状態を戻し、適用失敗理由を管理する。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/resources/server/ServerPackManager.java:175](../../../minecraft-26.2-decompiled/src/net/minecraft/client/resources/server/ServerPackManager.java#L175) — onDownload：失敗packにDOWNLOAD_FAILED removal reasonを設定する。
  - Rust：[Client/pomme-client/src/app/core.rs:1438](../../pomme-client/src/app/core.rs#L1438) — pack_download_action：download resultをSuccessfullyLoaded/FailedDownloadへ変換する。
  - Rust：[Client/pomme-client/src/app/core.rs:7251](../../pomme-client/src/app/core.rs#L7251) — drain_network_events completed pack downloads：失敗を記録し、required失敗はdisconnect reasonを設定して応答する。

</details>

<a id="packs-05"></a>

<details>
<summary>packs-05 — Server resource packs › push/pop › apply, remove, and live reload</summary>

- 比較環境：公式サーバー接続
- 対象条件：server packを適用後、ID指定popとIDなしpop-allを行う
- 残件 / 比較すべき点：複数packの積み重ね・同一UUID再push・個別pop時に公式同様の残存順と表示切替になるか確認する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientCommonPacketListenerImpl.java:188](../../../minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientCommonPacketListenerImpl.java#L188) — handleResourcePackPop：UUID指定または全packのpopを管理器へ渡す。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/resources/server/ServerPackManager.java:102](../../../minecraft-26.2-decompiled/src/net/minecraft/client/resources/server/ServerPackManager.java#L102) — popPack：該当UUIDの有効packに削除理由を付けreloadを要求する。
  - Rust：[Client/pomme-client/src/resource_pack.rs:209](../../pomme-client/src/resource_pack.rs#L209) — apply_server_pack：同一UUIDのserver packを除去してから新packをstack末尾へ適用する。
  - Rust：[Client/pomme-client/src/app/core.rs:7136](../../pomme-client/src/app/core.rs#L7136) — drain_network_events ResourcePackPop arm：個別/全pop後にreload_live_resource_assetsを呼ぶ。

</details>

<a id="packs-06"></a>

<details>
<summary>packs-06 — Models and blockstates › resource-pack definitions › load layered blockstate data</summary>

- 比較環境：クライアント共通
- 対象条件：blockstate JSONと参照model JSONをlocal packで上書きする
- 残件 / 比較すべき点：代表的なvariants/multipart、親model、override、未知model参照を同じpackで公式と照合する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/resources/model/BlockStateModelLoader.java:32](../../../minecraft-26.2-decompiled/src/net/minecraft/client/resources/model/BlockStateModelLoader.java#L32) — loadBlockStates：blockstates resource stackの全層を列挙して各定義をdecodeする。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/resources/model/BlockStateModelLoader.java:83](../../../minecraft-26.2-decompiled/src/net/minecraft/client/resources/model/BlockStateModelLoader.java#L83) — loadBlockStateDefinitionStack：順にinstantiateし、後続定義のstate mappingを反映する。
  - Rust：[Client/pomme-client/src/world/block/model.rs:2187](../../pomme-client/src/world/block/model.rs#L2187) — blockstate asset discovery：有効packのminecraft/blockstatesを収集する。
  - Rust：[Client/pomme-client/src/world/block/model.rs:2196](../../pomme-client/src/world/block/model.rs#L2196) — blockstate asset resolution：blockstates JSONを優先pack解決経由で取得し既存model loaderへ渡す。

</details>

<a id="packs-07"></a>

<details>
<summary>packs-07 — Textures and animation › pack override › texture metadata and frames</summary>

- 比較環境：クライアント共通
- 対象条件：animated PNGとtexture .mcmetaを複数優先度のpackで上書きする
- 残件 / 比較すべき点：frame時間・順序、interpolation、filtering等を公式resourceと同じ画像/metadataで比較する。
- 根拠：
  - Rust：[Client/pomme-client/src/resource_pack.rs:96](../../pomme-client/src/resource_pack.rs#L96) — resolve_asset_with_metadata：asset供給pack以上の優先度からmcmetaを取得する。
  - Rust：[Client/pomme-client/src/renderer/chunk/atlas.rs:629](../../pomme-client/src/renderer/chunk/atlas.rs#L629) — resolve_texture_resource：texture/metadataをpack、asset index、jarから解決する。
  - Rust：[Client/pomme-client/src/renderer/chunk/atlas.rs:660](../../pomme-client/src/renderer/chunk/atlas.rs#L660) — load_source：metadataからanimation layoutを生成し初期frameとmip sourceへ反映する。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/renderer/texture/SpriteContents.java:113](../../../minecraft-26.2-decompiled/src/net/minecraft/client/renderer/texture/SpriteContents.java#L113) — createAnimatedTexture：animation metadataのframe列・時間情報からanimated textureを生成する。

</details>

<a id="packs-08"></a>

<details>
<summary>packs-08 — Fonts › resource-pack font definitions › providers and options</summary>

- 比較環境：クライアント共通
- 対象条件：font providerのbitmap/space/referenceとfilterを含むpackを有効にし、font optionを切り替える
- 残件 / 比較すべき点：各provider形式とfilter、options切替時のglyph選択・font reloadを公式と照合する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/font/FontManager.java:102](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/font/FontManager.java#L102) — prepare：font definition stackを読みproviderを構築する。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/font/FontManager.java:193](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/font/FontManager.java#L193) — getFontOptions：Force UnicodeとJapanese glyph variantをprovider optionsへ反映する。
  - Rust：[Client/pomme-client/src/ui/font.rs:503](../../pomme-client/src/ui/font.rs#L503) — GlyphMap::load：font optionsをdefaultに固定しUnicode/Japanese variants未対応をTODOとしている。
  - Rust：[Client/pomme-client/src/ui/font.rs:943](../../pomme-client/src/ui/font.rs#L943) — load_unresolved_font：resource stackを読みprovider群を解決する。

</details>

<a id="packs-09"></a>

<details>
<summary>packs-09 — Language › selected locale › pack translation merging and fallback</summary>

- 比較環境：クライアント共通
- 対象条件：local/server packで選択言語の一部keyを上書きし、未収録keyをfallbackする
- 残件 / 比較すべき点：pack提供langファイルの取り込み、namespace横断、優先順、対応localeの選択を公式と比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/resources/language/LanguageManager.java:49](../../../minecraft-26.2-decompiled/src/net/minecraft/client/resources/language/LanguageManager.java#L49) — onResourceManagerReload：en_usに選択localeを重ねてClientLanguageを構築する。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/resources/language/ClientLanguage.java:29](../../../minecraft-26.2-decompiled/src/net/minecraft/client/resources/language/ClientLanguage.java#L29) — loadFrom：各locale・全namespaceのresource stackを走査する。
  - Rust：[Client/pomme-client/src/lang.rs:28](../../pomme-client/src/lang.rs#L28) — catalogs：jar/indexから英語・日本語の二カタログだけをロードする。
  - Rust：[Client/pomme-client/src/lang.rs:61](../../pomme-client/src/lang.rs#L61) — set_locale：選択可能localeをen_us/ja_jpに限定する。

</details>

<a id="packs-10"></a>

<details>
<summary>packs-10 — Sounds › sounds.json › layered event registration and replace</summary>

- 比較環境：クライアント共通
- 対象条件：複数packから同一sound eventを追記・replace指定で上書きする
- 残件 / 比較すべき点：同一eventのsounds/subtitle、replace値、参照eventをstack順に変えて公式と登録結果を比較する。音声の実再生・mixは対象外。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/sounds/SoundManager.java:84](../../../minecraft-26.2-decompiled/src/net/minecraft/client/sounds/SoundManager.java#L84) — prepare：sounds.json resource stackを順に読み登録を適用する。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/sounds/SoundManager.java:266](../../../minecraft-26.2-decompiled/src/net/minecraft/client/sounds/SoundManager.java#L266) — Preparations.handleRegistration：sound registrationをpreparation registryへ適用する。
  - Rust：[Client/pomme-client/src/audio/sounds.rs:182](../../pomme-client/src/audio/sounds.rs#L182) — SoundsIndex::load：built-inと全active packのsounds.jsonを順に読み込む。
  - Rust：[Client/pomme-client/src/audio/sounds.rs:298](../../pomme-client/src/audio/sounds.rs#L298) — SoundsIndex::apply_value：replace=trueならイベント置換、falseならentries追記。

</details>


<a id="domain-singleplayer"></a>

## 内蔵シングルプレイ

調査範囲：指定されたRustのworld list/create/edit/delete、singleplayer起動・終了とSteelMC起動設定・保存入口、および公式WorldOpenFlows、WorldCreationContext、WorldCreationUiState、CreateWorldScreen、IntegratedServer、LevelStorageSourceを静的に照合。SteelMC全コード、個別のworldgen内容、保存データの実読込・実プレイ比較は調査していません。worldデータ操作、ビルド、テスト、アプリ起動はしていません。

- 制約：比較は静的のみで、公式とRustの実動作一致確認はしていません。
- 制約：SteelMC全コード、worldgen全機能、datapack互換、プレイ中のserver権威処理全般は調査対象外です。未調査機能を対応済みとは判定していません。
- 制約：world saveの読み書き、world作成・削除、ビルド、cargo test、アプリ起動は実行していません。

| ID / 機能 | 公式の挙動 | Rustの現状・差分 | 実装 | 比較 / 検証 |
|---|---|---|---|---|
| [singleplayer-01](#singleplayer-01)<br>シングルプレイ › 内蔵接続 › クライアントを内蔵サーバーへ接続 | 公式はワールドを開くとlevel data・datapack等の準備後にクライアントのworld loadへ進み、IntegratedServer上のworldで遊べる。 | PommeはSteelMCを別スレッドで起動し、メモリパイプの反対側を通常の接続処理へ渡す。本番経路の接続配線はあるが、プロトコル・ゲーム動作の実機一致は未確認。 | 実装あり | 未検証<br>静的照合 |
| [singleplayer-02](#singleplayer-02)<br>ワールド一覧 › 保存の発見 › 一覧表示対象 | 公式はLevelStorageSourceからlevel candidatesを列挙し、各worldのlevel summaryを読み、ロック状態なども含めて一覧化する。 | Pommeはsaves直下の各ディレクトリ内にあるpomme_level.jsonだけを読み、欠損・不正なsidecarは一覧から飛ばす。公式Java worldのlevel.datだけのsaveを列挙する実装は確認できない。 | 一部実装 | 差分あり<br>静的照合 |
| [singleplayer-03](#singleplayer-03)<br>ワールド作成 › 作成確定 › worldデータ生成・起動 | 公式のcreate確定はLevelSettingsとWorldOptions等を用意し、WorldOpenFlowsを通してworld data・generation settingsを構成して、そのままworldをロードする。 | Pommeのcreate_worldはsidecarと空のsave directoryを作って一覧画面へ戻るだけで、作成確定時には内蔵serverを起動しない。後からPlay操作でserver起動・spawn area準備に進む。 | 一部実装 | 差分あり<br>静的照合 |
| [singleplayer-04](#singleplayer-04)<br>ワールド作成 › Survival/Creative・難易度・コマンド許可 | 公式はselected game type、difficulty、hardcore、allow-commandsをLevelSettingsに渡し、world側の初期設定にする。 | PommeはSurvival/Creative、4段階のdifficulty、commands設定をsidecarに保存し、起動時にSteelMC configのdefault gamemode/difficultyへ渡す。commands許可はop permission group付与として扱う。 | 実装あり | 未検証<br>静的照合 |
| [singleplayer-05](#singleplayer-05)<br>ワールド作成 › Hardcore › 死亡時のworld制約 | 公式HardcoreはSurvivalにhardcore=trueを付け、difficultyをHardに固定し、commandsを無効にする。 | PommeのUIとsidecarはhardcoreフラグを保持し、Hard/commands offにするが、singleplayer::openのLaunchOptionsにhardcoreがなく、SteelMCのdefault gamemode/difficulty設定にもhardcoreが渡らない。Hardcore死亡制約を適用する経路は確認できない。 | 一部実装 | 差分あり<br>静的照合 |
| [singleplayer-06](#singleplayer-06)<br>ワールド作成 › Seed › generator seedの指定 | 公式は空欄seedをrandomにし、数値文字列をlongとして解析、数値でなければJava String hashを使う。 | Pommeは入力seedをsidecarに保持し、起動時にi64またはJava互換UTF-16 hashへ変換し、空欄ではランダム値を渡す。SteelMC world configにはseedを設定するが、既存worldでseed変更が反映されるかは確認していない。 | 実装あり | 未検証<br>静的照合 |
| [singleplayer-07](#singleplayer-07)<br>ワールド作成 › WorldCreationContext › 生成オプション・構造物設定 | 公式WorldCreationContextはWorldOptions、datapack dimensions、選択dimensions、worldgen registry、datapack resources/configurationを保持し、seedや生成設定をworld loadへ渡す。 | PommeのWorld tabではWorld Type/Customize、Generate Structures、Bonus Chestが非活性または操作不能。SteelMC設定はvanilla generatorのoverworld/nether/endを固定指定し、LaunchOptionsにstructures等の選択値がない。 | 一部実装 | 差分あり<br>静的照合 |
| [singleplayer-08](#singleplayer-08)<br>ゲーム内 › Pause › 内蔵serverのtick停止・pause時save | 公式IntegratedServerはMinecraft側pausedまたは参加player不在でpause状態へ移り、pause開始時にsaveEverythingを呼び、pause tickを行う。 | Pommeのpauseはclient側フラグでclient world clock等を止める。SteelMCは独立server runtime上でrunし続け、pause状態を通知する経路は確認できず、pause開始時のserver saveも行わない。 | 一部実装 | 差分あり<br>静的照合 |
| [singleplayer-09](#singleplayer-09)<br>ワールド再開・終了 › save永続化 › server停止時の保存 | 公式worldを開くとlevel dataを読み、通常の終了やpause時saveでworldを永続化し、次回再開に使う。 | Pommeはsession lockを保持してSteelMCを起動し、WorldHandle drop/shutdown時にpacket処理をdrainした後save_and_shutdownを呼ぶ。SteelMCの保存処理入口は接続されているが、実データを再openして状態維持する確認は未実施。 | 実装あり | 未検証<br>静的照合 |
| [singleplayer-10](#singleplayer-10)<br>ワールド一覧 › 削除 › 確認後のsave削除 | 公式は確認後LevelStorageAccess.deleteLevelを呼び、lockを検査してからworld directoryを削除する。 | 削除確認後、UIは一覧のworld folder名をWorldList::deleteへ渡す。deleteはsaves_dir.join(folder)にremove_dir_allを行うが、削除関数内にsession lockやpath containment検査はない。UI経由のpath escapeやsymlink悪用が可能かは未確認であり、実データ操作も未実施。 | 一部実装 | 差分あり<br>静的照合 |
| [singleplayer-11](#singleplayer-11)<br>ワールド編集 › 名前変更・付随するworld操作 | 公式EditWorldScreenはworld icon reset、folder open、backup、backup folder、optimize操作を提供する。 | Pommeはworld名をsidecarに保存しfolder名は維持する。world folderを開く操作はあるが、backup作成、backup folder、optimizeはdisabledで、icon resetも有効な処理経路は確認できない。 | 一部実装 | 差分あり<br>静的照合 |

### 条件・残件・根拠

<a id="singleplayer-01"></a>

<details>
<summary>singleplayer-01 — シングルプレイ › 内蔵接続 › クライアントを内蔵サーバーへ接続</summary>

- 比較環境：内蔵シングルプレイ
- 対象条件：singleplayer機能有効ビルドで一覧からworldを開く
- 残件 / 比較すべき点：実際の参加・初期化・プレイ中通信を公式IntegratedServerと比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/worldselection/WorldOpenFlows.java:305](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/worldselection/WorldOpenFlows.java#L305) — openWorld：world openからlevel data読込処理を開始する。
  - Rust：[Client/pomme-client/src/app/phases/in_menu.rs:149](../../pomme-client/src/app/phases/in_menu.rs#L149) — MenuAction::PlayWorld：選択したworldからsingleplayer::openを呼び、Memory transportでConnectする。
  - Rust：[Client/pomme-client/src/singleplayer.rs:107](../../pomme-client/src/singleplayer.rs#L107) — open：SteelMC launchとmemory_pipes接続を設定する。
  - Rust：[Client/pomme-singleplayer/src/lib.rs:244](../../pomme-singleplayer/src/lib.rs#L244) — JavaTcpClient::from_transport：SteelのJavaTcpClientをmemory transportに接続する。

</details>

<a id="singleplayer-02"></a>

<details>
<summary>singleplayer-02 — ワールド一覧 › 保存の発見 › 一覧表示対象</summary>

- 比較環境：接続前
- 対象条件：saves配下に公式level.datのみのworld、またはPomme sidecar付きworldがある
- 残件 / 比較すべき点：公式world saveをPommeで列挙・互換読込できるかは未対応に見える。world metadataとlocked表示を含む一覧挙動を比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/worldselection/WorldSelectionList.java:197](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/worldselection/WorldSelectionList.java#L197) — loadLevels：LevelStorageSourceから候補を取りsummaryをロードする。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/level/storage/LevelStorageSource.java:286](../../../minecraft-26.2-decompiled/src/net/minecraft/world/level/storage/LevelStorageSource.java#L286) — loadLevelSummaries：各候補のlock状態を調べlevel summaryを読む。
  - Rust：[Client/pomme-client/src/ui/world_list.rs:146](../../pomme-client/src/ui/world_list.rs#L146) — WorldList::scan：ディレクトリ内のpomme_level.jsonを読めた場合だけsummaryにする。

</details>

<a id="singleplayer-03"></a>

<details>
<summary>singleplayer-03 — ワールド作成 › 作成確定 › worldデータ生成・起動</summary>

- 比較環境：接続前
- 対象条件：Create New World画面で確定した直後
- 残件 / 比較すべき点：作成直後の起動・初期world data生成・エラー回復という公式フローとの差を確認する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/worldselection/CreateWorldScreen.java:347](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/worldselection/CreateWorldScreen.java#L347) — createLevelSettings：UI設定からLevelSettingsを作る。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/worldselection/WorldOpenFlows.java:101](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/worldselection/WorldOpenFlows.java#L101) — createFreshLevel：WorldOptionsとLevelSettingsからworld data/generation settingsを構成しdoWorldLoadへ進む。
  - Rust：[Client/pomme-client/src/ui/menu/worlds.rs:1059](../../pomme-client/src/ui/menu/worlds.rs#L1059) — create_world：summaryを構築してWorldList::create後、WorldListへ戻る。
  - Rust：[Client/pomme-client/src/ui/world_list.rs:219](../../pomme-client/src/ui/world_list.rs#L219) — WorldList::create：ディレクトリとsidecarを書き、summaryを一覧に追加する。

</details>

<a id="singleplayer-04"></a>

<details>
<summary>singleplayer-04 — ワールド作成 › Survival/Creative・難易度・コマンド許可</summary>

- 比較環境：内蔵シングルプレイ
- 対象条件：通常のSurvivalまたはCreative worldを作成し起動する
- 残件 / 比較すべき点：初回参加時のゲームモード、難易度、各コマンドの権限結果を公式と比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/worldselection/CreateWorldScreen.java:351](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/worldselection/CreateWorldScreen.java#L351) — createLevelSettings：ゲームモード、difficulty、hardcore、commandsをLevelSettingsへ渡す。
  - Rust：[Client/pomme-client/src/ui/menu/worlds.rs:1061](../../pomme-client/src/ui/menu/worlds.rs#L1061) — create_world：選択したモード等をWorldSummaryへ記録する。
  - Rust：[Client/pomme-client/src/singleplayer.rs:125](../../pomme-client/src/singleplayer.rs#L125) — open：summaryのgamemode/difficulty/commandsをLaunchOptionsへ写す。
  - Rust：[Client/pomme-singleplayer/src/lib.rs:301](../../pomme-singleplayer/src/lib.rs#L301) — permission_groups：commands許可をop groupのdefault assignmentに変換し、world configへ設定値を渡す。

</details>

<a id="singleplayer-05"></a>

<details>
<summary>singleplayer-05 — ワールド作成 › Hardcore › 死亡時のworld制約</summary>

- 比較環境：内蔵シングルプレイ
- 対象条件：Hardcore選択で作成し、ゲーム内で死亡する
- 残件 / 比較すべき点：Hardcoreフラグの永続化・サーバー権威の死亡時処理・復活不可挙動は実装確認が必要。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/worldselection/CreateWorldScreen.java:351](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/worldselection/CreateWorldScreen.java#L351) — createLevelSettings：difficultyとhardcoreをDifficultySettingsへ渡す。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/worldselection/WorldCreationUiState.java:138](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/worldselection/WorldCreationUiState.java#L138) — getDifficulty/isHardcore：HardcoreはHard難易度にし、hardcore状態を保持する。
  - Rust：[Client/pomme-client/src/ui/menu/worlds.rs:532](../../pomme-client/src/ui/menu/worlds.rs#L532) — SelectedMode::stored：Hardcore選択をSurvivalとhardcore boolへ変換する。
  - Rust：[Client/pomme-client/src/singleplayer.rs:120](../../pomme-client/src/singleplayer.rs#L120) — open：LaunchOptionsへgame_mode/difficulty等を渡すがhardcoreフラグは渡さない。

</details>

<a id="singleplayer-06"></a>

<details>
<summary>singleplayer-06 — ワールド作成 › Seed › generator seedの指定</summary>

- 比較環境：内蔵シングルプレイ
- 対象条件：新規worldで空欄、数値、文字列seedを各指定する
- 残件 / 比較すべき点：各seedで同一生成結果になるかの一致確認は禁止・未実施。既存save再開時に保存済みgenerator seedを維持するかも確認する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/level/levelgen/WorldOptions.java:80](../../../minecraft-26.2-decompiled/src/net/minecraft/world/level/levelgen/WorldOptions.java#L80) — parseSeed：空文字、long parse、Java hashによるseed解釈。
  - Rust：[Client/pomme-client/src/ui/world_list.rs:96](../../pomme-client/src/ui/world_list.rs#L96) — parse_seed：入力文字列をi64またはjava_string_hashへ変換する。
  - Rust：[Client/pomme-client/src/singleplayer.rs:116](../../pomme-client/src/singleplayer.rs#L116) — open：空欄seedはランダム値とし、LaunchOptionsへ渡す。
  - Rust：[Client/pomme-singleplayer/src/lib.rs:342](../../pomme-singleplayer/src/lib.rs#L342) — worlds_config：seed値をSteelMC WorldsConfigへ渡す。

</details>

<a id="singleplayer-07"></a>

<details>
<summary>singleplayer-07 — ワールド作成 › WorldCreationContext › 生成オプション・構造物設定</summary>

- 比較環境：内蔵シングルプレイ
- 対象条件：新規worldのWorldタブでworld type、構造物、bonus chestを変更する
- 残件 / 比較すべき点：公式と同じ生成設定・datapack由来dimensionsを選択または復元できるか確認する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/worldselection/WorldCreationContext.java:18](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/worldselection/WorldCreationContext.java#L18) — WorldCreationContext：worldgen options、dimensions、registries、datapack resources/configurationを保持する。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/worldselection/CreateWorldScreen.java:777](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/worldselection/CreateWorldScreen.java#L777) — WorldTab：Generate StructuresとBonus Chestを設定可能なswitchとして設置する。
  - Rust：[Client/pomme-client/src/ui/menu/worlds.rs:879](../../pomme-client/src/ui/menu/worlds.rs#L879) — build_create_world：world type/customizeとstructures/bonus chestをinert表示として描画する。
  - Rust：[Client/pomme-singleplayer/src/lib.rs:328](../../pomme-singleplayer/src/lib.rs#L328) — worlds_config：固定のoverworld/nether/end generatorを設定し、生成オプションの選択値は含めない。

</details>

<a id="singleplayer-08"></a>

<details>
<summary>singleplayer-08 — ゲーム内 › Pause › 内蔵serverのtick停止・pause時save</summary>

- 比較環境：内蔵シングルプレイ
- 対象条件：内蔵worldでPauseメニューを開き、時間経過させる
- 残件 / 比較すべき点：pause中のMob/block/server tick停止と開始時自動saveを公式と比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/server/IntegratedServer.java:146](../../../minecraft-26.2-decompiled/src/net/minecraft/client/server/IntegratedServer.java#L146) — tickServer：paused判定とpause開始時saveEverythingを行いtickPausedへ移る。
  - Rust：[Client/pomme-client/src/app/phases/in_game.rs:3778](../../pomme-client/src/app/phases/in_game.rs#L3778) — advance_level_time：singleplayerかつpaused時はclient-side level time進行を抑える。
  - Rust：[Client/pomme-singleplayer/src/lib.rs:237](../../pomme-singleplayer/src/lib.rs#L237) — serve：Steel server.runをspawnし、停止要求まで独立して実行する。

</details>

<a id="singleplayer-09"></a>

<details>
<summary>singleplayer-09 — ワールド再開・終了 › save永続化 › server停止時の保存</summary>

- 比較環境：内蔵シングルプレイ
- 対象条件：worldを開いて変更後に終了し、同じ保存先を再度開く
- 残件 / 比較すべき点：block/player/level dataの保存完了と次回load復元を確認する。world操作や実データ検査は今回行っていない。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/worldselection/WorldOpenFlows.java:313](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/worldselection/WorldOpenFlows.java#L313) — openWorldLoadLevelData：既存worldのlevel dataを読みsummary等を取得してopenを継続する。
  - Rust：[Client/pomme-singleplayer/src/lib.rs:260](../../pomme-singleplayer/src/lib.rs#L260) — serve：cancel後にpacket taskを閉じて待機しsave_and_shutdownする。
  - Rust：[Client/third_party/SteelMC/steel-core/src/server/run_loop.rs:88](../../third_party/SteelMC/steel-core/src/server/run_loop.rs#L88) — save_and_shutdown：server終了時のsave/shutdown処理入口。
  - Rust：[Client/pomme-client/src/singleplayer.rs:59](../../pomme-client/src/singleplayer.rs#L59) — World：serverをworld session中保持し、session lockを寿命管理する。

</details>

<a id="singleplayer-10"></a>

<details>
<summary>singleplayer-10 — ワールド一覧 › 削除 › 確認後のsave削除</summary>

- 比較環境：接続前
- 対象条件：削除確認後、対象saveが別processでlock中または通常状態
- 残件 / 比較すべき点：lock中worldの削除可否、削除対象のroot containment、symlinkを含むsaveでの実挙動を、隔離したテスト用コピー上で公式と比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/world/level/storage/LevelStorageSource.java:622](../../../minecraft-26.2-decompiled/src/net/minecraft/world/level/storage/LevelStorageSource.java#L622) — LevelStorageAccess.deleteLevel：削除前にcheckLockし、lock fileを除外してdirectoryを削除する。
  - Rust：[Client/pomme-client/src/ui/menu/worlds.rs:1281](../../pomme-client/src/ui/menu/worlds.rs#L1281) — build_confirm_delete_world：確認が肯定された場合WorldList::deleteを呼ぶ。
  - Rust：[Client/pomme-client/src/ui/world_list.rs:240](../../pomme-client/src/ui/world_list.rs#L240) — WorldList::delete：remove_dir_allで対象folderを削除し、memory上の一覧から除外する。

</details>

<a id="singleplayer-11"></a>

<details>
<summary>singleplayer-11 — ワールド編集 › 名前変更・付随するworld操作</summary>

- 比較環境：接続前
- 対象条件：一覧でworldを選択しEdit画面を操作する
- 残件 / 比較すべき点：公式のicon/backup/optimize各操作と、rename時に保持される全metadataを比較する。
- 根拠：
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/worldselection/EditWorldScreen.java:77](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/worldselection/EditWorldScreen.java#L77) — EditWorldScreen icon reset：icon reset buttonを作成し、icon fileがある場合に有効化する。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/worldselection/EditWorldScreen.java:84](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/worldselection/EditWorldScreen.java#L84) — EditWorldScreen backup button：backup actionをUIに接続する。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/worldselection/EditWorldScreen.java:88](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/worldselection/EditWorldScreen.java#L88) — EditWorldScreen backup folder button：backup folderを開く操作をUIに接続する。
  - 公式：[minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/worldselection/EditWorldScreen.java:103](../../../minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/worldselection/EditWorldScreen.java#L103) — EditWorldScreen optimize button：optimize画面へ進む操作を提供する。
  - Rust：[Client/pomme-client/src/ui/menu/worlds.rs:1190](../../pomme-client/src/ui/menu/worlds.rs#L1190) — build_edit_world：Rust Edit UIのOpen World Folderおよびdisabled操作表示。

</details>
