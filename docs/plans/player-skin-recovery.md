# Player skin 失敗回復・renderer容量計画

## 確定欠陥と未確定の原因

### 現コードで確定している欠陥

- `pomme-client/src/app/core.rs::queue_player_skin_for` は同じ cache UUID・profile UUID・source の要求を `requested_player_skins` に残したまま重複抑止する。`drain_player_skin_results` はfetch失敗時に警告するだけで要求を消さないため、同sourceの次イベントでも再fetchされず、source変更・削除・切断まで回復しない。
- fetch成功後も `Renderer::update_player_entity_skin` / `EntityRenderer::update_player_skin` は容量拒否を呼出側へ通知しない。`MAX_PLAYER_SKINS` 到達時はwarnしてfallbackを維持するだけで、Coreは成功扱いのまま要求を保持し、同source再通知を抑止する。
- stale generation/sourceの結果は `PlayerSkinRequest::accepts` で拒否される既存防御がある。回復処理はこの判定を先に行い、古い失敗/成功が現行要求を解除・上書きしないことを維持する。

### まだ根因と断定しない事項

- 撮影対象 `2026-10-04-1.log.gz` の `Player skin cache full` 60件と `load success` 1891件は容量拒否が実発生した証拠だが、撮影対象NPCのUUIDやSteve表示との対応は未確認。Steve全員の根因とは断定しない。
- Remove-before-spawnの順序仮説だけを公式との差として直さない。公式もPlayerInfoなしspawnを拒否するため、今回の計画は確認済み失敗状態の回復に限定する。

## 最小の推奨設計

### 1. 世代付き要求の状態と失敗遷移

`requested_player_skins` の各entryは現行要求のgeneration/sourceを表す。結果処理はまず現entryが結果と完全一致するか検査し、一致時のみ処理する。

- **fetch失敗**: 一致する現要求だけを解除し、次の同source通知を新generationとして受け付ける。
- **fetch成功・GPU登録成功**: 現行source/generationを成功済みとして保持し、従来どおり同source通知をdedupする。
- **fetch成功・GPU登録失敗**: 一致する現要求だけを解除する。次の既存通知で再要求できるようにし、成功済みとして記録しない。
- **stale結果**: 無視。特に結果処理時のremoveは `accepts(result)` と同じ現entry確認下に限り、古い要求が新要求を消せないようにする。
- **UUID v3でfetchしない既存ケース**は現在の挙動を維持し、無意味な自動retryを追加しない。

毎フレームretryは導入しない。失敗後抑止を解除するため、retry契機は既存PlayerInfo/Mannequin等の同source再通知だけとする。失敗通知が毎イベント反復して連打される場合に限り、次通知から適用する短いbounded cooldownを採用するかを実装時に検討する。採用するなら単調時計によるUUID単位の期限を持ち、generation/source変更・削除・切断で無効化する。HTTP 429/timeout等のerror classごとの待機値に根拠がない段階では数値を捏造せず、まず「次eventのみ・自動retryなし」を採用案とする。

### 2. Renderer結果契約

`EntityRenderer::update_player_skin` と `Renderer::update_player_entity_skin` を `Result<(), PlayerSkinUploadError>` 相当に変更し、最低限 `CapacityFull { current, limit }` と回復可能な upload/descriptor allocation error を区別する。現行のskin/cape画像uploadやdescriptor allocation経路を確認し、部分確保後の失敗では追加済みGPU資源を解放して既存skinを壊さないこと。現在の新規UUID上限確認をアップロード前に保ち、既存UUID更新は枠を消費しない契約を維持する。

Coreはrenderer成功を受けて初めてskin登録を成功扱いにする。失敗は要求解除と匿名化した警告/カウンターへ反映する。ログにUUID、player name、source URL、textures tokenを出さず、error class、generation、`current/limit` の数だけを記録する。fetch既存ログも今回の変更で秘匿情報を増やさない。

### 3. 容量・資源寿命の扱い

現状 `entity_renderer.rs` は `MAX_PLAYER_SKINS = 128`、`player_skins: HashMap<Uuid, PlayerSkinTexture>` を持つ。descriptor poolのtexture descriptor数は通常mob等の合計に `(MAX_PLAYER_SKINS * 2) + 1 + 1024` を加算し、player skinとoptional cape用に最大2 descriptor/skinを予約している。skin/cape GPU image、view、allocator allocation、descriptor setをEntityRendererが所有する。

現在の確認範囲では `remove_player_skin` / `clear_player_skins` があり、削除は `device.wait_idle()` 後に資源をfreeする（wait失敗時は保持）。通常のactive NPC離脱・PlayerInfo削除でこれらが確実に呼ばれるかは、実装時にCoreのイベント経路を追って確認する。明確なinactive lifetimeと呼出しが既にあればそれを活かす。なければ、まずその既存削除契約を接続する最小案を優先する。

上限到達時の選択肢:

1. **採用案: 固定128を維持し、拒否をResult化して観測可能にする。** 容量再発の件数を匿名カウンターで確認し、active lifetime/実需要が判明してから必要な容量変更を別判断する。
2. **調査後の候補: inactive skinの回収またはdescriptor set再利用。** active描画・GPU in-flightが参照するtextureを即時破棄しない。現在の`wait_idle` retirementを再利用する最小実装を検討し、描画中active NPC skinのevictionは禁止。連続退場・再登場による毎回の再downloadも避けるため、retirementとCPU側成功cacheの要否を評価する。
3. **不採用: 定数だけ増やす/上限を無制限化する。** poolのdescriptor上限・GPU image消費も合わせて変える必要があり、撮影ログの数だけでは必要サイズが分からない。

この計画のminimumは容量拒否を失敗として通知し、次の既存通知で再要求可能にするところまで。容量そのものの全撤去・active skin eviction・無制限再downloadは含めない。容量拒否の匿名件数と `current/limit` を観測し、実需要または再発状況に応じて上記2の設計を別途決定する。

## 変更境界・並行作業

実装時の局所変更候補:

- `pomme-client/src/app/core.rs`: `queue_player_skin_for`, `drain_player_skin_results`, request状態/結果テスト。
- `pomme-client/src/renderer/mod.rs`: `update_player_entity_skin` の結果転送のみ。
- `pomme-client/src/renderer/pipelines/entity_renderer.rs`: `MAX_PLAYER_SKINS`拒否、 fallible upload契約、GPU資源rollback、容量数のログ、テスト。

`core.rs` / shared renderer / `entity_renderer.rs` はportal・held-equipment担当と共有予定箇所。編集は上記関数/API境界に限定し、隣接描画処理や無関係な書式変更をしない。統合時にAPI変更と並行差分を合わせる。allowlist、redirect拒否、timeout、画像サイズ制限を含むfetch securityは緩めない。公式 `SkinManager` の15秒cacheを丸ごと移植せず、既存の同source成功dedupを再利用する。

## テスト・確認ゲート

1. Coreの状態遷移テスト: fetch失敗→entry解除→同source再通知で新generationを発行する。
2. Stale結果テスト: generation/source/profileが古い成功・失敗のどちらも現entryを変更しない。
3. Renderer契約テスト: capacity errorが呼出側へ伝わり、要求が成功dedup状態に残らない。空きを確保後の既存通知で登録成功する。
4. 成功dedupテスト: 登録成功後の同source再通知は新fetchを作らない。
5. Renderer更新テスト: 既存UUIDのskin置換は上限時も可能。新規UUIDは `current < limit` で受理し、満杯で拒否する。部分GPU確保失敗で旧textureが保持される。retirementを変更する場合はGPU idle前にimage/descriptorをfreeしないテスト可能な状態判定も確認する。
6. ログ確認: UUID/name/URL/tokenがなくerror class・generation・容量件数のみ。
7. security回帰: URL allowlist/redirect拒否/timeout/画像サイズ既存テストを維持し、緩和しない。通常の `mise run check` と関連test、続いて `mise run test` をClient/AGENTS.mdの手順で確認する。

GPU実環境でのupload失敗・descriptor枯渇・in-flight寿命の実証は、CPU状態テストだけで確認済みとしない。

## 実施範囲

- `PlayerSkinUploadError` として `CapacityFull { current, limit }`、`UploadFailed`、`DescriptorAllocationFailed` を転送し、Skin/Cape両方のfallible GPU確保・upload・descriptor失敗では旧登録をcommit前に保持するよう変更。
- Coreは現generation/profile/source一致時のみfailure entryを解除。成功dedupを保ち、fetch失敗後は次の同source通知で新generation、capacity preflight拒否ではfetchを開始せず次通知待ち。v3 UUID no-fetch、128上限、security判定は維持。
- CPU状態テストはCore 2件、capacity契約1件。check成功。全mise testはClient 1500件中1件失敗（`held_attachment_uses_arm_pose_root_yaw_and_small_scale_once`、held-equipment並行変更内のtransform assertion）で、protocol 52件成功、singleplayer 1件成功/1 ignored。Skin GPU実機rollback/descriptor枯渇検証は未実施。

## Follow-up: teardown と local skin の残P1（ownership契約）

### A. bounded shutdown と保存順

- 実所有順: `AppPhase::InGame { gfx, connection, game, world }` は`gfx`を先にdropし、`Renderer::drop` (`renderer/mod.rs`)→`VulkanContext::prepare_teardown`/`VulkanContext::drop` (`renderer/context.rs`) がdevice・command pool・`ManuallyDrop<Arc<Mutex<Allocator>>>`を破棄する前に`wait_idle_for_teardown`をnon-DeviceLostで100ms無限retryする。`StateSlot::transition` (`app/state_slot.rs:19`)のpanicはabortでunwind cleanupなし。
- 採用案: waitを有限回（100ms間隔、回数を定数化）にし結果を`Idle | DeviceLost | NonLostFailure`で保持。Idleは通常cleanup、DeviceLostは仕様上child free可として通常cleanup（Idleとは別enum）、NonLostFailureはnative child/allocator/contextをfree/dropせず、保存後にプロセスをnonzero終了するterminal path。上限後のfree・unwrap/panic・helper Dropへの移譲は禁止。RendererはApp内の単一terminal ownerであり、renderer再生成ループでretain資源を増やさない。
- 保存順を成立させる変更: `app/phases/mod.rs::AppPhase::{Connecting,InGame}`で`world`を`gfx`より前に宣言し、異常なphase/App dropでも`WorldHandle::drop` (`pomme-singleplayer/src/lib.rs:159`)のcancel→join→`serve`内`server.save_and_shutdown` (`lib.rs:263-266`)をRenderer dropより先にする。settingsは既存`App::exiting` (`app/mod.rs:310-312`)の`core.menu.flush_settings()`が`options.json` (`ui/menu/mod.rs:286,919`)へflush。通常window exitは既存`leave_world` (`app/mod.rs:205-224`)→`SavingWorld`/`update_saving` (`app/phases/saving.rs:10`)で保存完了を待ち、event loop終了後にcleanupする契約を維持。NonLostFailureのprocess exitはworld join/saveとsettings flushの後だけにする。
- 比較: bounded wait後のfreeはcompletion不明なので不可。ManuallyDropでRenderer各資源を保護する案は大変更。保存後のprocess exitならOSがnative資源を回収し、無制限リークはプロセス内に残らず最小。注意: `StateSlot`のabort panicはflush/WorldHandle Dropを呼ばないため、その場合の保存保証はしない。panic policy変更が別途必要。

### B. local hand/preview だけfallible transactional upload

- 変更境界: `renderer/mod.rs::update_local_player_skin`とlocal caller (`app/core.rs`のskin結果適用、`renderer/mod.rs::load_player_skin`)、`renderer/pipelines/hand.rs::{reload_skin,upload_skin_to_gpu}`、`renderer/pipelines/skin_preview.rs`。遠隔entity全体はrewriteしない。`renderer/util.rs`とportal prepareは7adf53d3担当中につき編集せず、担当側fallible upload/pending-retain APIおよび`entity_renderer.rs`既存`pending_skin_uploads`/drainを利用する。
- 順序: まずlocal hand image/view/allocationとarm vertexを一時作成し、upload/wait失敗をResult化。submit後completion不明ならstaging/cmd/image/allocationを実pending collectionへ保持してfreeしない。失敗中は旧hand/preview維持。preview更新/構築もfallible stagingし、失敗は旧slot維持。全local staging成功後にentity登録し、entityも成功した時だけhand/previewを一括swap、旧textureはidle確定後のみ解放。entity拒否ならstaged local資源をrollbackまたはpending-retain。CoreへOkを返すのは3領域commit後だけなので部分commit成功dedupを防ぐ。失敗時は同generationを成功記録しない。
- 制約: 同一local slotの再更新でpendingが残る場合は新規upload拒否し旧表示を保つ。無制限retain/背景threadは導入しない。retention上限超過時のprocess終端条件はAと同じ安全策を使う（資源をfreeしない）。

### C. 最小tests

- 本番pending push/drainを通すfake submit/wait test: submit前失敗では安全な資源だけfree、submit後wait errorではcmd/staging/imageをfreeせずpendingに残し、完了確認後のみdrain/free。entity拒否・hand/preview失敗時のold slot保持と、全成功時だけ3 slot同generation commitを観測。
- teardown共通終了判断のtestでIdle/DeviceLostはcleanup許可、NonLost連続失敗は規定回数で停止・native cleanup禁止・fatalを観測。fake world/settings/renderer操作列で`world save/join → options flush → renderer terminal decision`を検証。destroy文字列を返すだけのfake closureは不可。
- CPU本番で確認できるのは状態・所有権遷移・操作順まで。GPU queue完了/device-loss/native free安全性は実GPU確認が別途必要。本追記依頼ではdocsのみ変更、build/testは実行しない。

### B 実施結果（2026-10-04）

- `Renderer::update_local_player_skin` はhand画像/view/allocatorと両arm vertex bufferを fallible prepare し、skin previewのbody/arm bufferも fallible prepare してから `update_player_entity_skin` を呼ぶ。entity登録失敗では両prepared組を破棄し、entity側pending資源の所有権に触れず、旧entity/hand/previewを維持する。entity成功後だけpreview descriptor/meshとhand texture/verticesをinfallible swapし、Coreのskin result callbackが顔データをcommitする。追加のidle `expect`、local skin用 `try_submit_one_time`、panic upload pathは使わない。既存`util::try_upload_image`とentity rendererの`pending_skin_uploads`を利用し、submit後wait errorのcommand/staging/image/view/allocationを実pending collectionへ移動する。submit前失敗はstagingとprepared imageをrollbackする。preview descriptor更新はVulkanのvoid `update_descriptor_sets` をcommit時に行うためfallible操作ではない。
- `Renderer::load_player_skin` は `Result` を返し、`app/mod.rs` の実callerがerrorを記録する。Coreの `apply_player_skin_result` はrenderer成功後だけfaceデータをdedup/commitし、失敗なら要求を解除する既存契約を使用する。capacity/upload/descriptor errorは既存Coreの匿名error stageへ流れる。
- CPU test追加: `skin_upload_pending_collection_owns_submitted_resources_until_idle` はproductionと共有するpending push/drain helperおよび実Vecを使い、pre-submitは未登録、post-submitのcommand/staging/imageデータをcollectionが保持、非idle判定後も保持、idle判定後に同データがdrainされることを検証する。Coreのstale/full/success generation回帰testは既存の`production_result_boundary_rejects_stale_and_commits_only_successful_uploads`を利用。local GPU slotを含む hand/preview/entity 各failureの独立CPU transaction test、およびDeviceLost分岐の実GPU検証は未実施（このため全要求されたCPU failure matrixを完了扱いしない）。
- 実GPU upload、queue/device lost、allocator/descriptor freeのin-flight安全性は未検証。Aのshutdown wait無限retry・強制終了policyは変更せず、解消済みとは主張しない。
- 確認: `mise exec -- cargo test -p pomme-client --locked --profile dev-fast skin_upload_pending_collection_owns_submitted_resources_until_idle` は1 passed、同profileの`production_result_boundary_rejects_stale_and_commits_only_successful_uploads` は1 passed、`mise run check` はexit 0。`mise run test` はexit 101: Client 1532 passed / 1 failed、protocol 52 passed、singleplayer 1 passed / 1 ignored。唯一のClient失敗は並行作業中の`net::azalea_compat::legacy_filled_map_frame_metadata_runs_translation_decode_remap_event_and_store` (`azalea_compat.rs:503`, `translation data`期待値unwrap)。local skinで追加したGPU fixture/testはなく、実GPUは未確認。

