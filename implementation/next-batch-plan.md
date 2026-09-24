# 次回 fix batch 候補（読み取り調査のみ）

調査時点: 2026-09-24 13:23 JST。`audit-filter/implementation/status-ledger.csv` の3行はこの時刻に読んだスナップショットであり、復元中のCSV注釈や以後の修復状態は未確認。コード・ledgerは変更せず、build/testも実行していない。候補は3 rootのみ。statusと原判定はCSV記載をそのまま保持。原auditの優先suffixはMが1件、Lが2件であり、Hへ格上げしていない。対象除外後、現コードで未実装を再確認でき、source methodまで直接読めたこの3件を提案する。

## 選定root

| 原ID | 原判定 / 現status（CSV） | valid根拠・公式26.2 | 最小の本番接続・テスト |
|---|---|---|---|
| physics-entities `#14-M01`（重複根#7-M07） | 妥当 / 未対応/一次確認待ち | 監査は`EntityEvent` 35の未処理を指摘。現`net/handler.rs`は3,9,10,1,19,4,11,34,8,56等のみ明示し、35なし。公式sourcejar `net/minecraft/client/multiplayer/ClientPacketListener.java:1173-1194` の`handleEntityEvent`は35でTotem emitter(30 tick)、TOTEM_USE音、player本人なら`displayItemActivation(findTotem(...))`。コード上のevent欠落と一致。 | `handler.rs`→`NetworkEvent`（entity id）→`AppCore`で存在entityだけ処理。再利用可能なのはEntityStore lookup、`AudioEngine::play_world_sound`、所持品/slot読み出し。不足はTotem tracking emitter/粒子種とitem-activation UI state+描画（stub不可）。テスト: event 35だけ発火、音/30tick emitter、local player時だけ正しいheld/offhand TotemアイコンをUI stateへ、他entityではoverlayなし。`net/handler.rs`に既存packet event tests、particle/UIの小さな状態テストを追加。 |
| physics-entities `#14-L06` | 妥当 / 未対応/一次確認待ち | `handler.rs`の`Animate`はSwingMain/OffとWakeUpのみ。公式sourcejar同method `ClientPacketListener.java:1080-1097` はaction 4=`ParticleTypes.CRIT`、5=`ENCHANTED_HIT`のtracking emitterを対象entityへ作成。26.2経路の限定欠落。26.3→26.2 action remapは既存`net/translate.rs:4518-4534`なので変更対象にしない。 | handler→event（id+critical種別）→AppCore/entity/particle tracking→renderer。EntityStoreと`ParticleStore`を再利用、現粒子は`particle.rs`の種類が狭くtracking emitter APIが不足。全particle対応ではなくCRIT/ENCHANTED_HITの2種だけ。テスト: action4/5が正しいevent/kindへなり、unknown actionは無視、存在しないentityへemitterを残さない；寿命・追従位置のmodel test。 |
| physics-entities `#14-L07` | 妥当 / 未対応/一次確認待ち | 公式sourcejar `ClientPacketListener.java:1488-1496` は`NO_RESPAWN_BLOCK_AVAILABLE`で`block.minecraft.spawn.not_valid`のtranslatable system messageをplayerに送る。汎用GameEventは現在`handler.rs`からcoreへ届き、`core.rs`で`GameState::apply_game_event`を呼ぶが、`in_game.rs`同関数はWin/ImmediateRespawn/LimitedCrafting以外を無視。よってpacket運搬済みでも表示動作は欠ける。 | generic event契約を維持し、core consumerから既存chat/system-message modelへtranslation keyを追加、通常chat HUDで描画。`ChatState`/既存message renderingを再利用。不足はsystem translatable spanを安全に挿入するhelperが現core matchにない点。テスト: event 0で該当keyが一度だけchat stateへ入り、他GameEventやweatherでは追加なし；画面描画stubは作らない。 |

## 所有範囲 / 次batch案

3件は同時並行の独立patchには分けにくい。共通変更点が`net/handler.rs`、`net/mod.rs`、`app/core.rs`のevent dispatch/consumer。repo外の候補worktree/staged integration案として、**1 integration owner**が小さなevent contractとcore armsをまとめ、粒子 (`particle.rs`/entity renderer) とchat UI (`ui/chat.rs`等) のownerは先にAPIだけ合意してから別stageで実装する。Totem UIはさらにin-game presentation接続が要る。強引な常に成功/画面だけのstubは不可。先にCRIT/ENCHANTED_HITとGameEvent 0を接続し、Totemはemitter+item activationの双方を同じbatchで完結できる場合のみ含める。

## 確認記録・未確認

- 読取: `status-ledger.csv` 652データ項目・7列（これに見出し1行）。抽出した原判定/statusは上表の通り。ledger修復agentによる同時更新後の状態は未確認。
- 公式26.2 sourcejar: `C:/Users/yuzum/Desktop/mine_rust/fabric-render-probe/.gradle/loom-cache/minecraftMaven/net/minecraft/minecraft-clientOnly-201bed67e3/26.2/minecraft-clientOnly-201bed67e3-26.2-sources.jar`。`py -3` zipfileで2207 entries、対象`ClientPacketListener.java`の指定3 method/bodyを抽出。いずれも読取コマンド終了0。
- 実コード照合は上記handler/event/core/state/particle経路のみ。公式のentity activation icon選択API、particle sprite/resource mapping、chat translation assetの実描画、GPU/live server E2Eは未確認。試験実行・buildなし。`javap`はPATHになく終了127（`#14-L10`を候補にせず、公式packet behaviorを推測しない）。
