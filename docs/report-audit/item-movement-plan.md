# アイテム使用・移動差分の調査と最小修正案

調査対象: `Client` 現在 HEAD `914ebf2`。根拠資料は `docs/report-audit/batch-09.json` と外部の `Pomme_0a1aee9_full_report/{records,reports}`。本メモは調査のみ。コード変更、build/test/runtime replayはしていない。

## 結論一覧

| 症状 | 根本原因（現在のコードから確定できる範囲） | 修正対象 |
|---|---|---|
| Crossbow保持で連射 | `InteractionState::tick` は長押し中、using state が無いと `use_delay==0` のたびに `start_use_item` を呼ぶ。`USE_DELAY=4`。crossbowは `use_item` が通常の非consumable経路を通り `ActiveUse` に入らず、4tickごとにUseItem packetを再送する。この繰り返し使用がサーバーのcrossbow操作を再実行させる。 | `start_use_item`/`use_item`/`ActiveUse` にcrossbow charging stateを追加し、holdは一回だけ開始、charging中は再送せず、releaseで一度だけ終了。loaded crossbowの発射は押下一回につき最大一発で、チャージ開始とは別のone-shotとして扱う。 |
| Trident投擲されない | 現行`use_item`はTridentをcharge useにしない。したがって`using_item`/`using_bow`とも立たず、release経路が発火しない。 | Tridentをcharge可能なActiveUseにし、pressで一度UseItem、hold中timer、releaseでReleaseUseItem一回。投擲/消費はサーバー結果（inventory/投射体）に委ねる。 |
| Shield GUI/FPS差 | GUIの空offhand placeholder `EmptyShield` は空スロット背景で、実item iconの代替ではない。通常アイコンは `item_resource_name`→item model/texture atlas。FPS held itemは汎用meshとgeneric transform。shield専用block pose/use-animationはない。画像上でGUIアイコンと手持ち姿が双方ないことは記録されているが、根本原因がatlas解決かは現状未確定。ダメージ防御は未検証。 | `world/block/model.rs` item model bake + `renderer/pipelines/held_item.rs`/renderer draw selectionを確認し、minecraft:shieldのresolved model/textureを診断。欠損時だけ既存item-model pathへ修正。Use state中のshield blocking poseを別途追加。防御機能の実装・主張はserver damage testが別途必要。 |
| Spyglass FOV/overlay | `compute_fov_modifier(&LocalPlayer, effect_scale)`はsprint/flyingのみ。TODOにspyglass scope明記。InteractionState active use情報はFOV計算へ渡らない。`hud.rs` camera overlaysにもspyglass TODO。spyglassは現在ActiveUseにならず、hold中のローカル use signalもない。 | ActiveUseにspyglass使用を表す情報を保持し、FOV計算で専用倍率を既存`fovEffectScale`へ反映。release/cancelで戻す。`hud.rs::build_camera_overlays`へuse中だけscope mask。画面/FOV変更はクライアント処理でありサーバー側使用だけでは完結しない。 |
| Left-hand option | optionはpersist/reloadされ、`main_hand_right()`がHUD attack indicatorへ渡る。一方`net::client_information()`は`HumanoidArm::Right`固定。held item/hand rendererはmain=`false`, offhand=`true`をhandedness設定なしで扱う。optionはUI文字だけではないが、第一人称全体に反映されない。 | `client_information(view_distance, chat_options, main_hand_right)`へ設定を渡しHumanoidArm反映。第一人称側は主副handの論理役割を維持して左右表示を選ぶ（main-hand right=false時にmain item/armを左へ、offhandを右へ）。HeldItemPipelineとHandPipelineに一貫してside mappingし、attack-indicatorの現行位置決定を壊さない。 |
| Levitation stationary | `physics/movement.rs`にはLevitationのfall-distance resetはあるが、速度/移動適用は見当たらない。`tick_with_context`→`travel`→`tick_land`では通常gravityが作用するのみ。サーバー側effect metadata受信と表示があっても、client movement predictionには不十分。 | `movement::tick_with_context`のaiStep/travel該当順序でlevitation amplifierに応じて上向き目標速度へ近づける処理を加える（vanilla同等の `vy += (0.05*(amplifier+1)-vy)*0.2` 相当、正確なfloat順序は修正時に照合）。gravityとの適用位置をvanilla順で決め、collision・water/flying・effect除去も既存tick内で扱う。 |

## Item-use tick / packetの具体的な流れ

1. `app/core.rs::tick_physics` は毎tick movement処理を先行する。`movement::tick_with_context`に渡す`use_speed_multiplier`/`slow_due_to_using_item`はInteractionStateがその時点で持つ値なので、同tickに新しく開始した使用がmovementへ反映されるのは基本的に次tickから。
2. 同tick後半にheld stackをinventoryから読み、`game.interaction.tick(...)`を呼ぶ。`InteractionState::tick`は、カーソル未capture時はitem use tickだけ進める。通常captured時は開始時点の`using = using_item.is_some() || using_bow`を計算。
3. `using=true`なら、Use inputを離していた場合だけ`release_using_item`が`PlayerAction::ReleaseUseItem`を送り、local stateを消す。保持中は再UseItemしない。
4. `using=false`ならjust-pressed、またはperforming actionかつ`use_delay==0`で`start_use_item`。同関数冒頭で`use_delay=4`。tick末尾で同delayを1減らすため、非-latched itemを保持すると概ね4tickごと（~0.2秒ごと）に再試行される。start_use_itemはblock useを先に送り、pass時にair `UseItem`を送る。
5. `use_item`は全アイテムに`ServerboundUseItem {hand,seq,y_rot,x_rot}`を送るが、local active timerはBowまたは`Consumable`限定。Crossbow、Trident、Spyglass、Shieldはいずれもここに独自hold処理がない（shield component判定はlocal successを返すだけでuse duration/stateを作らない）。
6. `ReleaseUseItem`は`using`分岐でのみ送る。したがってTrident/Spyglass/Crossbowを単純にpacketだけ送ってもlocal active stateを作らない限り、release packetを送る保証はない。

### Crossbow: auto-fire / held re-fireの問いへの回答

- ソース上確定できる欠陥は「hold中の`UseItem`再送」である。Bowだけは`using_item`と`using_bow`を設定し、その後holdでpacketを再送しない。Crossbowには該当latchがなく4tickごとに`UseItem`送信へ戻る。
- reportは2.125–2.161秒保持×3回で矢16→11、Crossbow damage=4、charged projectileが最後に1本、Arrow entity 4体を記録。Referenceは2.284–2.302秒保持×3回で矢16→15、charged projectile 1本、damageなし。これはholdで複数回UseItemされた実害の証拠。
- 「チャージ完了イベントがクライアントのauto-fireを直接起こす」処理は現行ソースでは見つからない。より正確には、client timerで自動射撃しているのではなく、再送したUseItem packetによりサーバーのuseが再実行され、loaded時の発射等が複数回起きうる。実際どのpacketが各4発を起こしたかはreportにpacket traceがないので1:1対応までは断言しない。
- 目標挙動はpress→charge開始1回→hold中 packetなし→release `ReleaseUseItem` 1回でcharge完了。fully charged crossbowを次のpressで発射した場合はuseがone-shotで済み、押し続けて再発射しない。UseItemの押下種別を charge-start と loaded-fire に分けて回帰確認する。

### Trident release packetは届くか

- 現行コードでTridentをholdしても`using_item`をセットしないため、`tick`のrelease branch（`if using { if !performing_action(Use) { release_using_item } }`）に入らない。従ってTridentを押して離したことを契機とする`ReleaseUseItem`は送られない。入力buttonのreleaseはInputState上で認識されてもpacket生成へ結びついていない。
- `ITEM-trident` reportでPommeは2.123–2.131秒保持後も所持品tridentが1、投射体無しを3/3回確認。Referenceは3/3回trident消費とTrident entity確認。packet captureがないため「絶対にwireに一切release actionが出ていない」までは既存runtime証拠なし。ただしコード経路から通常のrelease packetが出ないことは確定。
- 修正はbowの分岐にTridentを単に足すだけでは不十分。Trident用active use durationと10tick投擲閾値、hold時のanimation/use pose、release時だけ終了packet、早すぎるreleaseのcancel、item swap/death/metadata停止時のcleanupが必要。耐久/consumption/projectile spawnはサーバーauthoritativeとする。Riptide/Loyaltyはreportで未検証なので範囲外と明記。

## Active-use metadata同期

- server metadataは`app/core.rs` index 8 Byteを見て`flags & 1 != 0`のみ`InteractionState::sync_using_item_flag`へ渡す。これはusing-item bitのclear時、local active stateを消す実装。set/rising edgeはTODOで新しいActiveUseを作らない。offhand bit (`0x02`)もこの呼び出しでは破棄。
- 一方`entity/mod.rs`側の一般entity metadata parserはindex 8をbit0|bit1で`using_item`にし、bit1を`using_offhand`へ保存するが、そのentity dataがローカルInteractionStateの手・item/timerへ接続されていない。
- したがってactive use metadataの「clear」は動作、開始/使用hand/remaining durationの同期は未実装。server metadataだけで投擲/一人称poseを成立させる設計ではない。今回の修正はlocal input stateで開始・releaseを駆動しつつ、metadata clearをauthoritative cancellationとして尊重する。勝手にmetadata rising edgeからunknown timer/itemを推測しない。

## Shield描画: 確認できたこと・未確定点

- `ui/inventory.rs`の`EmptyShield` spriteはoffhand empty slotの背景絵であり、実在shieldアイテムのアイコン処理ではない。
- 実item icons/HUDは`item_resource_name`で`minecraft:shield`からname `shield`を作り、block registry item model bake/texture atlas経由（`world/block/model.rs::bake_item_models`、`renderer/mod.rs::ensure_item_mesh`等）。GUI item iconを渡すcall siteもitem nameベース。
- 第一人称`held_item.rs::update_and_draw`はmain/offhandについて通常display transformとeat/bow special caseだけ。shield用blocking matrix/poseがない。InteractionStateにshield use animationもない。
- **未確定:** source grepではshieldの具体的texture/model lookup failureを実行していない。従って「atlasにshield imageがない」等の断定はしない。修正者は既存registryの`item_models`/flat texture key、atlas rect、mesh handleを実機なしの既存unit fixtureまたはdebug traceで切り分け、実在textureが正常ならfirst-person poseだけを追加する。報告のホットバーアイコン不在は描画観測として残し、inventory state自体はserver NBTで1個確認済み。

## SpyglassとFOV / overlay設計

- `core.rs::compute_fov_modifier(player, effect_scale)`の現在の効果はflight 1.1とsprint 1.3からのmodifierのみで、最終値を`1.0.lerp(modifier,effect_scale)`。base FOVは`renderer.set_base_fov`、modifierは`renderer.update_fov_mod`、water factorも同位置で更新。
- 関数はLocalPlayerしか受けずInteractionStateを参照しない。active spyglassを受ける引数を一つ増やすのが最小の接続案。scope倍率は通常FOVを縮める倍率を既存FOV effect settingに適用し、slider=0で無効にする。base FOV値そのものを書き換えず、解除時に自然に元へ戻す。
- `ui/hud.rs::build_camera_overlays`にspyglass overlay TODO。use中にだけ円形scope/周辺暗転を重ねる既存overlay pipelineを調べて追加。overlayはitem model/GUI iconとは独立。release、item swap、metadata clear、screen/cursor stateでFOVとoverlayが残留しないことを確認。
- `ITEM-spyglass`: hold 2.124–2.164秒×3、Pomme通常視野・枠なし、Referenceはズーム・黒いscope frame。使用中server inventoryは両者1個。client visual gapの記録で、item消費は問題とされていない。

## Left-hand 設定の接続案

- persistence: `ui/menu/mod.rs`にserde設定`skin_main_hand_right`、getter `main_hand_right()`、options row toggle + saveあり。
- すでに利用: `app/phases/in_game.rs` HUD inputへ渡し、`ui/hud.rs` attack indicator位置は左右を選ぶ。
- 反映欠落: `net/mod.rs::client_information`ではHumanoidArm::Right固定。configuration/login/game packetの複数送信箇所が共通helperを使うため、settingを同helperに渡す。
- renderer: `interaction.rs::use_animation`の`left_hand`はoffhandか否かというhand identity。`in_game.rs` renderでmain hand itemはHeldItemPipelineへ`left_hand=false`、offhandはtrue。これは“どちらのinventory handか”と“画面上の左右side”を同じboolに押し込んでいる。修正時はlogical hand IDとdisplay sideを分け、main-hand preferenceに応じmain item/armのsideを決めて反対側へoffhandを置く。shader/model transformの鏡像、item-specific left display transform、bow draw handのsignを合わせる。
- `SETTING-main-hand`の報告は左手設定保存済みなのにdiamond swordが右表示という比較。attack indicatorだけは既存UIで選択側に移動するため、完全未対応とはしない。

## Levitation移動処理案

- `movement.rs::tick_with_context`: water state/current、fall distance reset、crouch、input, sprint/fly, jump, then `travel`; land path `tick_land`でcollision move→gravity subtraction→vertical drag。`effective_gravity`はslow_falling等処理だがLevitation target velocityには未使用。
- `has_effect_named(..., "levitation")`は`reset_fall_distance_for_tick`でfall distanceを0にするため、effect名登録と一部効果処理はあるが移動駆動が欠落。
- 追加箇所はmovement common tickのgravity/travel前（vanilla LivingEntity travel levitation adjustment相当）にし、water/fall-flying等の分岐との優先順は公式26.2同等に整理する。ground frictionの後に単純速度加算するだけでは、tick_landのgravityで値が変わるので順序が重要。
- 既存報告値: `MOVE-levitation_stationary` 0 amplifier、4秒・無入力・stone floor。Pomme Y64.0→64.0、範囲64.0–64.0を3/3回。Reference end Y=69.1890–69.2798、max Y=70.1418–70.2325、z移動なし3/3回。movement behaviorが実際に不足する差を支持するが、target trajectory全体やeffect expire後は未検証。

## 次の実装担当に渡すprompt案

> `docs/report-audit/item-movement-plan.md`を読み、そこに指定した現行HEADの原因と制約を維持して修正してください。最初に既存callers/testsを再確認し、`InteractionState::{tick,start_use_item,use_item,update_using_item,release_using_item,sync_using_item_flag}`を中心にcrossbow/trident/spyglass/shieldのactive-use lifecycleを最小差分で実装してください。長押しで4tickごとに再送される通常item-useをなくし、crossbow charged-fire one-shotとuncharged charge/releaseを分け、tridentはthreshold前後releaseを扱ってください。use metadataのbit0/bit1 clearとoffhandを誤用せず、item swap/respawn/server stopでstate/FOV/overlayを残さないでください。Spyglassは`compute_fov_modifier`と`build_camera_overlays`、shieldはitem texture lookupを先に確定してからblocking pose、left-handは`client_information`とfirst-person hand/item mapping（logical handとdisplay sideを分離）、Levitationは`physics/movement.rs`のvanilla順序でvertical velocityへ反映してください。Riptide/Loyaltyやshield damage reductionなど報告未検証機能を黙って範囲拡張しないでください。unit testsはpacket count / release packet / threshold / swap cancellation / metadata clear / FOV restore / levitation zero-input trajectoryを追加し、最後に許可されたmise focused testと変更後checkを実行。local server reproは別途許可がある場合だけ実施。報告記録の各値と比較し、未確認点を修正済みと書かないこと。

## 実施確認と未確認

- 読了: `Client/AGENTS.md`, ponytail `SKILL.md`, `batch-09.json`, report `volumes/items_01.md`, `volumes/features_01.md`, reports/items crossbow/shield/spyglass/trident, records/MOVE-levitation_stationary.json。
- read-only確認: `git -C C:/Users/yuzum/Desktop/mine_rust/Client status --short --branch && git ... rev-parse --short HEAD` → exit 0、`master...origin/master [ahead 14]`, HEAD `914ebf2`, `?? docs/`（report-audit evidence directory already untracked; this memo is the only file written by this task）。
- Build/test/runtime comparison: 実行なし（依頼で禁止）。現在HEADでのゲーム再現、実際のatlas lookup値、wire packet capture、post-914ebf2 gameplay、GPU表示、Riptide/Loyalty、shield damage protectionは未確認。
