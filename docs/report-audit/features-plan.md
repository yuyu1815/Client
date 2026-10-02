# Feature report audit: remaining findings and minimal fixes

調査対象は report の対象コミット `0a1aee9c6be911a3081cf454b0bae6d694ec9d55` と、現在の `Client/` checkout。これはコード変更案の調査メモで、実装・build・test・run は行っていない。証拠とコードの確度を分け、見た目の候補を確定bug扱いしない。

## 判定サマリー

| 対象 | 判定 | 要旨 |
|---|---|---|
| Command syntax error context | **表示側の実装経路はある。reportの実行後コマンドエラーは未修正の実動差** | `CommandParse::usage`/`SyntaxError` は入力中のローカルparseに関する。実行後のserver errorは `net/chat.rs` がSystemChat Componentを受けてspan化する別経路。少なくとも報告された実行後エラーとローカルusageを一つの原因にまとめない。 |
| 夜間の過剰な明るさ | **実動差確定、原因は未確定。光量式が最有力** | 18,000 tick・同じ光源fixtureで差あり。`world_brightness` がsky/block lightのmaxへ固定tableを当てるが、測定された原因を特定するlightmap traceは証拠中にない。 |
| 空の水平色帯 | **実動差確定、コード候補ありだが原因未確定** | reportの比較条件はtime 6000。空ドームは単色 `sky_color_linear`、clear colorはFog/Sky混色で別式。この境界が候補。GPU/画像で境界由来を切り分けた証拠なし。 |
| Shadow planes | **候補のまま** | 記録上、雪ゴーレム/TNT/Vex等の本体が見えない個体の足元に白い平面。本体欠落と同じ原因か、shadow/materialか未特定。別バグとして計上しない。 |
| 8個のdisabled設定 | **既知のUI無効化。項目ごとに backend 完成度が違う** | 8項目をまとめてenableするだけでは誤表示。表のscopeを満たすまで無効のままにする。 |
| auto-FPS mtime | **確定コードbug（環境のmtime分解能依存で表面化）** | 保存成功判定がfilesystem mtimeと直前のwall clockの厳密順序比較。報告環境では新規ファイルmtimeが比較時刻より古いケース20/20、retry 3/3 fail。ゲーム内FPS計測失敗とは言えない。 |
| integrated server stack overflow | **実クラッシュ確定、コード上はstack設定不足が有力。再現原因の確証なし** | ログは名前付き `integrated-server` threadのoverflow→SIGABRT。launch側threadが標準stack、Steelのdebug deep-call pathは8 MiBを明示する。cyclic recursion/巨大stack配列は調査範囲の単純検索では特定できず、build/run禁止条件のため確認不能。 |

## 詳細

### 1. Command error context

**原記録** `records/COMMAND-error-context.json`（report内ID。探索した `batch-09.json` というファイル名自体はreport内で見つからず、上記recordが該当証拠）。`/give @s stone -1` はInteger error一行のみ、存在しないitem IDもcontextなし。公式は `command.context.parse_error` と `<--[HERE]` を伴う。再現はen_us・OP権限。記録自体も「実行後のserver error」と「入力中local parse」は別評価と注意している。

**現在コードの経路**
- 入力編集中: `pomme-client/src/net/commands.rs::CommandParse`、`CommandTree::usage`、`SyntaxError`; `pomme-client/src/ui/chat.rs::syntax_error_component` と `parse_error_component`。`CommandTree::usage` はcompletionなしのときparse errorを `UsageLine::Error` に渡し、`parse_error_component` は位置・前後文字・`<--[HERE]` を作る。つまりこのシンボル群は既にcontext機能を持ち、serverから受けた実行後error recordを直接説明しない。
- Serverからの通常system chat: `pomme-client/src/net/chat.rs::parse_system_chat` → `read_component` → `send_chat` → `format_chat_component_spans`。コードは受信Componentを明示的にテキスト一行へ変換していない。Componentのtranslate/args/extra解釈、または実際にwireで届いたcomponent形状を疑うべきだが、recordにはraw packet/component dumpがなく、ここを確定原因とはしない。

**最小案**: 現象を一度だけ同じ `/give` 入力で切り分ける診断を追加（またはpacketを採取）し、受信 `system_chat` のComponentをkey/args/extra/styles付きで保存する。wireに既に `command.context.parse_error` があるなら `read_component` → `format_chat_component_spans` の間で情報を落としている箇所を直す。wireがerror keyのみならPommeの画面が入力履歴を保持しているか確認して、原文errorの再組み立てを検討するが、serverが使うunknown componentを推測しない。既存の `parse_error_component` はlocal parse用に維持し、server errorへ誤って適用しない。

### 2. Sky band and night lighting

**Records** `records/RENDER-sky-band.json`、`records/RENDER-night.json`。両方 `observed_runtime_difference`、`independent_root_cause_confirmed: false`。報告上はsoftware Vulkan (llvmpipe); real GPU/Mac未確認。

- `RENDER-night`: clear, time 18000, 7 blocks filled、glowstone/torch、同じfeet `[0.5,66,10.5]`, yaw 180, pitch 20。Pommeは周辺が広く明るく光の減衰が異なる。
- `RENDER-sky-band`: time 6000、feet `[0.5,64,8.5]`, yaw 180, pitch 8、同じ条件で水平の色境界。

**コード候補**
- `pomme-client/src/renderer/chunk/mesher.rs::world_brightness` は `max(sky_light, block_light)` を `LIGHT_TABLE` (`0.05..1.0`) に対応させる。これは低levelに0ではない最低brightness 0.05を与え、sky/block channelも一つに潰す。夜光の明るさ差を説明しうる実コード候補だが、光源範囲/観測ピクセルの数値がなく確定原因ではない。関連処理は同ファイル `model_quad_lights` と block AO helpers。
- `pomme-client/src/renderer/pipelines/sky.rs::SkyState::clear_color_linear` はOverworldのfog-colorとsky-colorをrender distance依存で混ぜ、`SkyState::sky_color_linear` はsky-colorのみ返す。`SkyPipeline::update_and_draw` は単色 `sky_color` をUBOへ設定する。`pomme-client/src/renderer/shaders/sky.frag` mode 0はその色を全面出力する。clear colorとtop-discの境界に水平線が出る仮説はコードに基づくが、投影/深度/clear色がどのpixel境界へ出たかのtraceがなく原因確定ではない。

**最小案**: 夜は同じfixtureで複数点（光源から距離/高度別）のsky/block levelと最終vertex multiplierを記録してVanillaと比較し、差を出す光量式だけを直す。まず `max(sky,block)` と最低0.05をVanilla lightmap相当の別channel式へ置き換える必要性を数値で確かめる。Bandは時間6000・通常カメラでclear-colorとtop-disc色を記録。差が隣接面だけならclear/fog境界の色式を共通化するか、vanilla相当の連続sky gradientを追加。単一画像に合わせた定数補正は避ける。

### 3. Shadow planes candidate

**Record** `records/CANDIDATE-shadow-planes.json`; report explicitly says snow golem, TNT, vex等が本体なしで白い矩形状surfaceを持つ可能性、本体不在はentity個別記録に記載、shadow/material原因は未確定、重複バグ計上禁止。

現在 `pomme-client/src/renderer/world_shadow.rs::item_shadow_pieces` は可視entity、radius/strength、既知full-collision surfaceとlight gateから影pieceを作る。`renderer/pipelines/item_entity.rs` にshadow trace出力がある。原因が実際のshadowか、missing entity/modelの足元に残った別render passかを示す入力別traceはまだrecordにない。shadow settingやthresholdを変更する根拠はない。

**最小案**: 同じ1体でentity existence/visibility、`item_shadow_pieces`結果（piece数・bounds・alpha）、最終shadow pipeline draw数を同frameで採取。entity本体なし/ありのペアで判定し、影生成されていなければshadow codeは触らず該当model/render defectへ戻す。現時点は候補のまま。

### 4. Disabled settings: existing fields and complete scope

画面は `pomme-client/src/ui/menu/options.rs::build_options_video` / `build_options_controls` とgeneric `option_enabled` が明示prefixで無効化する。`pomme-client/src/ui/menu/mod.rs` は持続設定をロード/保存するが、Listed backend settingsの多くは対応field自体がない。enable時はlabelだけでなくinput handling、save/reload、renderer/packet side effectまで必要。

| 項目 | Backend既存値 | 有効化に必要な最小scope |
|---|---|---|
| Auto-Jump | **なし** | `Options`設定(default/serde/load/save)、control行label/click、gameplayで地面接地時に前方障害物を越える条件とjump impulseを接続。`physics/movement.rs`のswept collision/jump判定を基礎に、hold-jumpとの差も守る。単に既存jump keyを連続させるのではなくVanilla autojump条件を移植。 |
| Brightness | **なし**。`LIGHT_TABLE`は値lookupで、gamma optionではない | gamma scalarの保存・video slider、`world_brightness`から最終shader色までのgamma/lightmap適用。GUI/texture等を不用意に明るくしない。夜の光量問題と同一原因か未確定のため共同fixtureで先に式を確定する。 |
| Graphics | **なし**。画面は固定 `Graphics: Fancy` | 最小の切替値（Fast/Fancy等）の保存/selection。どの描画機能が設定を読むか決める。現状render distance、cloud mode、AO等は別field/別optionであり、graphics modeをUIだけ切替てもbackend効果はない。scopeを定められないうちはdisabled維持が妥当。 |
| Invert Mouse | **なし** | boolの保存、controls row/click、camera mouse-look deltaのpitch符号（yawは変えない）に結線。menu/GUI cursorを反転しない。 |
| Key Bindings | **操作設定としてはなし**。Options ControlsのSensitivityのみfield、`OptionsKeybinds` は `build_options_stub("Keybinds")` | 最低限、既存input actionsを表すbinding mapを保存し、key-down/up dispatchとrebind capture/UIを結線。hardcoded key checks全部を同map経由へ移行しないと画面で変更してもゲーム操作に反映しない。単なるstub page遷移開放不可。 |
| Particles | **画面設定fieldなし、pipeline/storeはある**。`net::client_information` は `ParticleStatus::All` 固定。`ParticleStore::add_server_particle` コメント上Minimal/Decreasedは到達不能 | enum All/Decreased/Minimalを保存、options row cycle、initial/re-sync `ClientInformation`へ値を渡して `client_information_changed` 比較にも含める。併せてlocal particles (`ParticleStore` spawn path) がserver-provided statusと同様のAll/Reduced/Minimal behaviorになるようfilter/limit。server packetへ値を送るだけではclient-local break/ambient particlesが切り替わらない。 |
| Skin Layers | **一部あり**。`MenuSettings`は `skin_cape/jacket/left/right_sleeve/left/right_pants/hat` boolをSerde保存。ただしcontrols全部disabled、`client_information`は全てtrue固定、`renderer/entity_model.rs`にはoverlay layers未実装コメント | UI操作をpacketの`ModelCustomization`へ供給し、変更時`client_information_changed`/`sync_client_information`から送信（initial connectも含む）。自分/remoteのskin renderingにはcape/jacket/sleeve/pants/hat visibilityとlayer model/texture geometryを結線。hatのみ設定fieldがあることは全layerが動作する根拠にならない。少なくともserver側で使うcustomizationとclient-sideモデル表示を両方確認。 |
| Smooth Lighting | **部分backendあり、独立設定なし**。block model AOにper-model `ambient_occlusion`、`mesher`にsmooth vertex light/AO実装、現在常時適用 | `smooth_lighting` boolを保存し、chunk mesh build jobsへ渡し、AO/smooth vertex-light分岐とmesh cache invalidation/remeshを実装。設定切替で既存chunkを更新。各model JSONの `ambientocclusion:false` は尊重し、global OFFで個別モデル指定を壊さない。別途brightness tableと切り離して比較する。 |

**関連existing wiring**: `pomme-client/src/net/mod.rs::client_information` はmodel customization/particle statusを固定している。Initial packetは`net/connection.rs`、in-game updateは`app/phases/in_game.rs::sync_client_information`。Skin fieldsは`ui/menu/mod.rs`設定にあるが、`net/mod.rs`から参照されていない。`menu/options.rs` skin row commentsも「client_information hardcodes them」としている。Skin Layerを無効のままにしているのは現時点で一貫した判断。

### 5. Auto-FPS mtime

**Record evidence**: `evidence/test-status.json`: standard suite 1 fail / 1 ignored, test retries 3/3 fail; 20 new temp files all mtime before pre-write Rust `SystemTime::now` (approx 3ms in that environment), Python writer did not reproduce. Separately `filesystem-mtime-diagnostic.json` has 100 samples, older-than count 0, min delta 14,642 ns, median 16,099 ns. This demonstrates filesystem/writer-specific clock behavior, not a stable universal failure. Full-duration runtime FPS measurement/gameplay was not demonstrated failing.

**Code bug**: `pomme-client/src/app/auto_fps.rs::AutoFps::succeed` compares `metadata(...).modified()? < self.started_at`. A valid result file can have a coarser or skewed timestamp even if newly written after run start. Unique per-run `benchmark_file` already exists, so mtime is unnecessary proof of ownership.

**Minimal fix**: Make run-specific result path existence/content validity the success condition, not strict wall-clock mtime. To retain reported timestamp, read modified time only as metadata and tolerate clock granularity (or serialize the benchmark result's own timestamp/identity). Keep unique run id/path as anti-stale-file guarantee. Test should prove an existing current-run result succeeds when mtime is older than `started_at`; do not sleep or skip test to hide the check.

### 6. Integrated-server stack overflow

**Evidence** `evidence/singleplayer-dev-fast.log`: compile finishes; ignored-inclusive `pomme-singleplayer` tests run 2 tests; `world_settings_reach_the_steel_config ... ok`; then `thread 'integrated-server' ... overflowed its stack`, `fatal runtime error: stack overflow, aborting`, signal 6/SIGABRT, cargo exit 101. This says no frame/function in which it overflowed.

**Code trail**: `pomme-singleplayer/src/lib.rs::launch` creates `thread::Builder::new().name("integrated-server")` with no explicit stack size; `drive` calls Tokio runtime `block_on(serve)` on this outer thread. Steel `third_party/SteelMC/steel-utils/src/threading.rs::DEBUG_STACK_SIZE` is 8 MiB; `steel-core/src/server/mod.rs` uses it for debug Rayon generation threads because density-function call chains overflow default 2 MiB; Steel test runtime/wrapper also uses it in `steel-core/src/server/tests.rs`. This is strong evidence that a standard-stack thread in the new integration path may execute a deep server/worldgen call chain. But current log has no backtrace/function marker and therefore does not prove which call runs out of stack.

A targeted text search of `pomme-singleplayer/src` found no obvious recursive helpers or conspicuously large fixed local arrays. It cannot exclude recursion in Steel/worldgen/dependency code. No confirmed cycle or array root cause is found. Since build/run is expressly forbidden, do not classify a larger stack as proven repair.

**Minimal next fix candidate**: Use Steel's existing `DEBUG_STACK_SIZE` for the named outer thread in `launch` under debug assertions (or establish that thread is only runtime orchestration and move deep work onto existing sized worker pool). The smallest change is adding `.stack_size(DEBUG_STACK_SIZE)` before `.spawn`, but verify visibility/dependency and purpose before implementing. This addresses the evident stack-size mismatch without changing algorithm recursion. To confirm root cause later, obtain a debug stack trace/last stage or compare with the sized outer thread in a separately authorized run. Do not replace with unconditional production stack increase without that evidence.

## Exclusions / unverified

- `batch-09.json` literal name was not located in report directory; matching command evidence available as `records/COMMAND-error-context.json`. If batch-09 is stored elsewhere, it was not inspected.
- Resource packs, short right-click, items, levitation are out of scope per task and intentionally not repeated.
- No files besides this memo were written. No code changed; no commit/build/check/test/run commands issued.
- Search/read operations were inspection only; there is no validation command exit code applicable to code behavior. The report's recorded exits are distinct: stack overflow test command exit 101/SIGABRT, auto-FPS standard suite 1 failed of 1217 with 3 individual retries failed; neither was rerun here.
