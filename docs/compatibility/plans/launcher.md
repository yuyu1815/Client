# 起動準備・引渡し — 公式26.2互換計画

- **Pomme基準:** `Client` checkout の base `a12e38d9ea09d290a0b1736fe48a1cdafe49325f`（本計画を作成する前のHEAD）。
- **SteelMC基準:** `Client/third_party/SteelMC` の pin `0b1f87c36a664f08d81397e942fb1e19f6eb282c`。このworktreeではsubmoduleを初期化しておらず、Steel実装の読み取り・比較は未実施。
- **参照版:** 公式 Minecraft Java Edition `26.2`。参照物はworkspace rootの `minecraft-26.2-decompiled/src/net/minecraft/client/main/Main.java` と `minecraft-26.2-decompiled/cache/26.2.json`。
- **比較環境:** (A) Pomme launcher → 公式26.2 server、(B) SteelMC singleplayer → 公式26.2 integrated server。準備・接続・world開始境界の比較計画であり、今回は静的な計画文書のみ。
- **母集合の出所:** 起動引数は公式 `Main.java:74–112` の `parser.accepts(...)` 宣言と `nonOptions()`、起動構成は `Main.java:202–218` の `GameConfig` 組立、metadataは保存済み `26.2.json` の有限schema inventory。公式launcher実装は参照workspaceに存在しないため、その取得実装・policyを直接比較したとは扱わない。

## 1. 対象と責任境界

対象は **Pomme launcherが選択したMinecraft versionのmetadata・asset・client runtimeを準備し、Rust client processへ一貫した起動条件を引き渡すまで**。公式launcherのUI再現やJava launcher replicaは対象外。計画の責務は次の境界に置く。

- **PreparationService（launcher側の将来設計）** が選択状態/generationの正本を所有する。UIの `LaunchRequest(installationId, explicit version, gameDir, platform, binary policy)` を受理した時点でinstallation設定を一度だけ解決し、generation IDとimmutable `LaunchPlan`を生成する。serviceはsingle-owner command actorまたは同等のmutex serialization domainを持ち、request受理、cancel/invalidate、ready publish、spawn acceptanceをそのdomain内でlinearizeする。network/hash/extract I/Oはlock外で行い、完了結果をactorへ戻す。UIはrequest ID/generation/monotonic revisionを表示し、messageを現selectionと照合して遅延したstale Ready/Error/Progressで表示を上書きしない。downloaded-version cacheは表示・高速化用に限る。新requestまたは選択cancelは以前のgenerationをinvalidateし、古い結果は共有download cacheへの安全なartifact publishには使えてもready表示・spawnには使わない。
- **LaunchPlan / spawn ownership** は一度解決・検証した `game version + metadata/index identity + validated directories + Pomme binary release/platform/digest + launch policy + credential handoff handle` をまとめる不変値。prepare/spawnは必ず同一planを使い、Tauri側で別のinstallation.versionを再解決しない。ready publishもactor内でcurrent generationを確認してlinearizeする。OS spawn acceptanceではactorがcurrent generation照合とspawn reservationを一つのatomic stepとして行い、予約確定後のOS spawn実行はactive-child生成まで責任を持つ。新request/cancelがreservation commitより先なら古いspawnを拒否し、reservation commitが先なら旧generationのspawnは受理済childとして管理し黙ってkillしない。予約後のOS spawn失敗はgeneration付き`SpawnFailed`を返し、安全handoff cleanupを行うが準備済valid cacheは維持する。成否はgeneration equalityだけでなくactor ordering/reservation stateで決める。active childはspawn時generationに束縛して保持し、次の選択で黙って終了させない。spawnが先に受理されたchild statusは選択version情報とは別stateで保持・表示し捨てない。複数childを許すか拒否するか、cancel時のactive child policyは別途明示し、未決のまま暗黙実装しない。
- **resource owner:** Rust runtimeがasset key解決、優先順位、resource reload/loadingを所有する。launcherは必要なmetadata・object・JAR由来assetの取得物完全性と起動前readinessを保証するが、runtime asset利用や再読込を引き受けない。
- **auth/account owner:** 認証、account選択、credentialの生成/更新、およびaccount identityの正本を所有する。launcherは安全なaccount reference/opaque credential handleだけを受け取る境界を持ち、token値を作成・更新・解釈しない。
- **protocol owner:** Rust clientのsupported game-version capability、protocol選択を所有する。auth/account ownerとの合意でcredential handoff format/lifetimeを定める。launcherはhandleを最小限かつ秘匿して子へ渡し、終了・失敗時にcleanupする契約を持つ。
- **singleplayer lifecycle owner:** world data、runtime start/tick/saveはRust/SteelMC側のlifecycle責任。launcherはgame-dir選択、process生成・監視・安全な終了報告までを受け持ち、world save/tickの意味は変更しない。

現状の通常UI entryは `pomme-launcher/src/App.tsx:233–307::handleLaunch` がinstallation/server versionを選び `ensureAssets(currentInstall.version)` を待ってからinstallation ID/overrideを別々に `launchGame` へ渡す。serverVersion + serverIpの場合、ここで `currentInstall.version=serverVersion` を設定するが、Tauri側でもoverrideを別途渡すため、準備versionとspawn versionがずれる可能性がある。Tauri `pomme-launcher/src-tauri/src/commands.rs:350–385::ensure_assets/launch_game` は別commandで準備後にinstallationを再検索し、`:480–580::spawn_game` が `override_version.unwrap_or(install.version)` としてversionを再構成する。これは明確なP0/P1差分であり、override有無双方で同一resolved versionを準備・検証・spawnに使う必要がある。benchmark entry `pomme-launcher/src-tauri/src/auto_benchmark.rs:213–251` も `ensure_assets` 後に共有 `commands::spawn_game` を直接呼ぶ。これは現在の静的source経路確認であり、実benchmark比較ではない。将来は両entryとも同じPreparationService/LaunchPlanを通し、どのTauri commandからも直接spawn bypassを許さない。immutable planを一箇所で所有するのは、準備・表示・spawnに散らばる再解決補正をなくすため。

## 2. 公式の機能母集合

### 2.1 Main.javaの宣言済み引数

公式ソースは未知optionを許容し (`Main.java:74`)、35 named optionsを宣言するほか `nonOptions()` positional declaration (`:112`) も持つ。下表は公式Mainの実consume/条件と、Pomme CLI/settings/runtime経路の有無を照合する。Pomme側「なし/unknown」は適用不要の断定ではない。対象はlauncherのversion/assets/platform/binary準備とprocess handoffで、graphics/auth実装自体は隣接ownerの責任。

| Official option | 公式Main consume path:line:symbol / 条件 | Pomme対応経路 / launcher判断 |
|---|---|---|
| `demo` | `Main.java:195,220::optionSet.has → GameData.isDemo` | CLIなし (`pomme-client/src/args.rs:5–48`)、runtime設定も未確認。owner比較unknown; launcher契約未確定、100%残条件。 |
| `disableMultiplayer` | `Main.java:195,223::optionSet.has → GameData.disableMultiplayer` | CLI/runtime toggleなし。Pomme network capability比較unknown; launcher適用契約未確定、100%残条件。 |
| `disableChat` | `Main.java:196,224::optionSet.has → GameData.disableChat` | CLI/runtime toggleなし。chat owner比較unknown; launcher契約未確定、100%残条件。 |
| `fullscreen` | `Main.java:193,217::optionSet.has → DisplayData.fullscreen` | `pomme-client/src/ui/menu/mod.rs:699,840,980–983`に永続display modeはあるがlauncher handoffなし。既存runtime設定あり/起動引渡し不足を分けてowner確認。 |
| `checkGlErrors` | declaration `Main.java:79`; Main consume/GameConfig転送なし | Pomme同等runtime経路を確認できず。graphics owner比較unknown; launcher契約未確定、100%残条件。 |
| `renderDebugLabels` | `Main.java:80,198,227::has → GameData.renderDebugLabels` | CLI/settings/runtime同等設定なし。renderer owner比較unknown; launcher契約未確定、100%残条件。 |
| `vulkanValidation` | `Main.java:81,200,226::has → GameData.vulkanValidation` | `pomme-client/src/renderer/context.rs:129–150,176–188`でdebug buildかつvalidation layer有なら自動設定。launcher handoffなしだが条件付きruntime既定は存在。releaseで選択不可、renderer比較unknown; launcher契約未確定で100%残条件。 |
| `graphicsBackend` | `Main.java:82–84,201,228::parseArgument → GameData.graphics` | Pomme Vulkan固定 (`pomme-client/src/renderer/context.rs:118–120`)、選択CLI/settingsなし。固定backendの要求同等性はrenderer比較unknown; launcher契約未確定、100%残条件。 |
| `jfrProfile` | `Main.java:85,142–144::has → JvmProfiler.start(CLIENT)` | Rust CLI/runtime JFR handoffなし。profiling ownerで同等要求比較unknown (Java JVM固有と即断しない); launcher契約未確定、100%残条件。 |
| `tracy` | `Main.java:86,146–148::has → TracyBootstrap.setup()` | Pomme CLI/launcher Tracy起動経路なし。profiling owner比較unknown; launcher契約未確定、100%残条件。 |
| `tracyNoImages` | `Main.java:87,197,225::has → captureTracyImages → GameData` | Pomme Tracy image capture設定なし。profiling owner比較unknown; launcher契約未確定、100%残条件。 |
| `quickPlayPath` | `Main.java:88,210,231::parseArgument → QuickPlayData.logPath` | Pomme path CLI/runtimeなし; quick-play owner比較unknown、launcher契約未確定。 |
| `quickPlaySingleplayer` | `Main.java:89,211,231::getQuickPlayVariant` | Pomme相当CLI/runtimeなし。singleplayer owner比較unknown; launcher契約未確定、100%残条件。 |
| `quickPlayMultiplayer` | `Main.java:90,211,231::getQuickPlayVariant` | `--quick-access-multiplayer` (`pomme-client/src/args.rs:30–31`), launcher handoff `pomme-launcher/src-tauri/src/commands.rs:563–565`, consume `pomme-client/src/app/mod.rs:396`。概念対応あり、value/log semantics別途要確認。 |
| `quickPlayRealms` | `Main.java:91,211,231::getQuickPlayVariant` | Realms quick-play pathなし。adjacent owner比較unknown; launcher契約未確定、100%残条件。 |
| `gameDir` | `Main.java:92,123,218::default '.' / parseArgument → FolderData.gameDir` | `--game-dir` (`pomme-client/src/args.rs:27–28`), handoff `pomme-launcher/src-tauri/src/commands.rs:554–555`, resolve `pomme-client/src/main.rs:126–131`。対応あり、generation固定は未成立。 |
| `assetsDir` | `Main.java:93,202,218::FolderData.assetsDir` (default `gameDir/assets`) | `--assets-dir` (`pomme-client/src/args.rs:21–22`), handoff `pomme-launcher/src-tauri/src/commands.rs:548–551`, resolve `pomme-client/src/main.rs:126–131`。対応あり。 |
| `resourcePackDir` | `Main.java:94,203,218::FolderData.resourcePackDir` (default `gameDir/resourcepacks`) | `ResourcePackManager::new(gameDir)` (`pomme-client/src/app/core.rs:3315`)、別dir CLIなし。runtime defaultあり/handoffなし、resource owner比較。 |
| `proxyHost` | `Main.java:95,169–175,216::host指定時 SOCKS Proxy → UserData` | Pomme `pomme-client/src/net/connection.rs:769–776::SessionServerJoinOpts`は`proxy: None`、proxy host設定CLIなし。接続proxy要否をnetwork owner比較unknown; Java-onlyとせずlauncher契約未確定、100%残条件。 |
| `proxyPort` | `Main.java:96,173::host指定時 port使用、default 8080` | `pomme-client/src/net/connection.rs:769–776`にport/config handoffなし。proxy owner比較unknown; launcher契約未確定、100%残条件。 |
| `proxyUser` | `Main.java:97,178–186::proxy有効かつuser/pass有効時 Authenticator` | Pomme proxy credential handoff/runtime設定なし。security/network owner比較unknown; launcher契約未確定、100%残条件。 |
| `proxyPass` | `Main.java:98,178–186::同上 PasswordAuthentication` | Pomme proxy credential handoff/runtime設定なし。secretを含むがJava-only断定せずowner比較unknown; launcher契約未確定、100%残条件。 |
| `username` | `Main.java:99,204–206,212–214::UUID fallback/User組立` | `--username` (`pomme-client/src/args.rs:9–10`), handoff `pomme-launcher/src-tauri/src/commands.rs:544–548`、対応あり。 |
| `offlineDeveloperMode` | `Main.java:100,229::has → GameData` | Pomme CLI/runtime equivalent未確認。dev owner比較unknown; launcher契約未確定、100%残条件。 |
| `uuid` | `Main.java:101,204–206,212–214::valid UUID parse or offline UUID` | `--uuid` (`pomme-client/src/args.rs:12–13`), handoff `pomme-launcher/src-tauri/src/commands.rs:557–561`, `pomme-client/src/user.rs:21–28`。対応あり、identity semanticsはauth owner確認。 |
| `xuid` | `Main.java:102,208,212–214::optional/default empty → User` | Pomme CLI/runtime XUIDなし。auth/protocol owner比較unknown; launcher契約未確定、100%残条件。 |
| `clientId` | `Main.java:103,209,212–214::optional/default empty → User` | Launcher Microsoft OAuth `CLIENT_ID` (`pomme-launcher/src-tauri/src/auth.rs:5,112–120`)は別用途で使用されるがRust game CLI/User handoffはなし。同じ意味のruntime valueとはみなさずauth owner比較unknown; launcher applicability未決、100%残条件。 |
| `accessToken` | `Main.java:104,213::required → User` | `--access-token` (`pomme-client/src/args.rs:15–16`), raw argv handoff `pomme-launcher/src-tauri/src/commands.rs:557–562`, runtime `pomme-client/src/app/mod.rs:163–195`。対応あり、secret transport課題§4.4; auth生成はowner外。 |
| `version` | `Main.java:105,123–124,221::required → launchedVersion/GameData` | `--version` (`pomme-client/src/args.rs:6–7`), handoff `pomme-launcher/src-tauri/src/commands.rs:544–546`, `pomme-client/src/main.rs:119–160`。対応あり。 |
| `width` | `Main.java:106,189–190,217::default 854 → DisplayData` | Installation.width (`pomme-launcher/src-tauri/src/installations/mod.rs:245–255`)は保持、spawn引渡しなし。window起動反映はunknown;既存値とlauncher不足を分けowner確認。 |
| `height` | `Main.java:107,189–190,217::default 480 → DisplayData` | Installation.heightは保持するがspawn handoffなし。renderer比較unknown、契約未確定。 |
| `fullscreenWidth` | `Main.java:108,191,217::optional → DisplayData` | Pomme起動時resolution CLI/settings不在。renderer比較unknown; launcher契約未確定、100%残条件。 |
| `fullscreenHeight` | `Main.java:109,191,217::optional → DisplayData` | Pomme起動時resolution CLI/settings不在。renderer比較unknown; launcher契約未確定、100%残条件。 |
| `assetIndex` | `Main.java:110,207–218::FolderData.assetIndex` | `--asset-index`なし (`pomme-client/src/args.rs:21–25`); runtime index resolve `pomme-client/src/assets.rs:168–209`はdirs経由。runtime consumptionあり、explicit ID handoffなし; launcher契約未確定。 |
| `versionType` | `Main.java:111,199,222::default release → GameData.versionType` | Launcher versions type取得/display (`pomme-launcher/src-tauri/src/commands.rs:180–209,233–255`)、spawn/runtime argumentなし。Java-only断定せずversion owner比較unknown;契約未確定、100%残条件。 |
| positional `nonOptions()` | declaration `Main.java:112`; `:164–167::valuesOf`、残余あれば “Completely ignored arguments” log | Rust `LaunchArgs` (`pomme-client/src/args.rs:5–48`)にpositionalなし。公式もignoreするので任意転送しない。 |

35 named optionsすべてと `nonOptions()` declarationを表に含めた。各行は宣言のみ/実consumeなしも明示した。隣接ownerで対応判定不能なら「比較継続unknown; launcher適用有無は契約未確定、100%残条件」であり適用外確定ではない。graphics/authの実装をlauncher scopeへ取り込まず、launch option契約とprocess引渡しの有無を調べる。Pomme runtime同等機能があれば、Vulkan validationのようにruntime既定条件とlauncher引渡し不足を区別する。

Rust起動argvの母集合は `pomme-client/src/args.rs:3–48::LaunchArgs` の全12 field（以下）であり、公式 `Main.java` のoption集合とは別に照合する。`Option<T>` はclap上の省略可否で、runtime必須条件とは区別する。`main.rs:97–160` ではversion省略時にjoinable defaultを選び、release buildではlaunch-tokenを要求する。launcherの設計契約ではresolved version/pathsを明示してplanに束縛する。

| Rust CLI field / argv | 現行の省略・既定 | 意味・owner / launcher handoff |
|---|---|---|
| `version: Option<String>` / `--version` | clap省略可、runtimeはjoinable versionへdefault | resolved versionはlauncher LaunchPlan owner、supported capability/protocol選択はprotocol owner。prepare/spawnで同一値必須。 |
| `username: Option<String>` / `--username` | 省略可 | account identityの表示名はauth/account owner、launcherは安全なaccount referenceを受領。 |
| `uuid: Option<String>` / `--uuid` | 省略可 | identity値生成/選択はauth/account owner。意味・fallbackは同owner合意。 |
| `access_token: Option<String>` / `--access-token` | 省略可 | secret。認証生成/refreshはauth/account owner。現commands.rs argv handoffを安全なhandle契約へ移す。 |
| `launch_token: Option<String>` / `--launch-token` | CLI省略可、release `main.rs:100–117` はpath存在を要求し確認後unlinkを試行 | Pomme独自起動gateでありaccess-tokenではない。credential handoffとは別のlauncher/client process-start契約としてnonce/path/lifecycle/cleanupを合意。 |
| `assets_dir: Option<String>` / `--assets-dir` | 省略可、dirs resolver既定あり | launcherが検証済asset rootを束縛、clientはruntimeでresolve/consume。 |
| `versions_dir: Option<String>` / `--versions-dir` | 省略可、dirs resolver既定あり | launcherが検証済version/JAR rootを束縛、clientはresolve/verify。 |
| `game_dir: Option<String>` / `--game-dir` | 省略可、dirs resolver既定あり | 選択installationからlauncherが固定。singleplayer/world lifecycleの実体はclient/Steel owner。 |
| `quick_access_multiplayer: Option<String>` / `--quick-access-multiplayer` | 省略可 | UI/server requestからlauncherが受渡し、接続意味はprotocol owner。 |
| `auto_fps_benchmark: bool` / `--auto-fps-benchmark` | 既定false | Pomme専用benchmark entry/runner owner。通常launchはfalse。 |
| `auto_fps_run_id: Option<String>` / `--auto-fps-run-id` | 省略可、flag依存あり | benchmark ownerが生成するrun correlation id。benchmark entryのみ。 |
| `render_probe_root: Option<PathBuf>` / `--render-probe-root` | 省略可 | renderer/probe owner。通常/benchmark起動へ無断追加しない。 |

通常起動とbenchmark起動は同じLaunchPlan/preflight/child lifecycle境界を通すが、benchmark固有flag・run id/statusは明示的に別policyとして保持する。Rust parserのoptional/defaultと公式引数の必須性は混同せず、official `Main.java` にないlaunch-token/benchmark/probe/versions-dirはJava互換optionではない。

### 2.2 26.2 metadataの有限schema inventory

保存済み `minecraft-26.2-decompiled/cache/26.2.json` のtop-level fieldは **14個**: `arguments`, `assetIndex`, `assets`, `complianceLevel`, `downloads`, `id`, `javaVersion`, `libraries`, `logging`, `mainClass`, `minimumLauncherVersion`, `releaseTime`, `time`, `type`。14 field名と下表記載のschema/valuesは確認済みだが、これは全nested value配列/rules/entryごとの行動を監査した意味ではない。

| Field / nested inventory | 26.2で確認した内容 | Pommeとの関係 |
|---|---|---|
| `arguments` | top-level group namesは `default-user-jvm` (3 entries), `game` (26), `jvm` (13)の計42 entries。30件はstring、12件はobject。objectの`value`はstringまたはarrayで、`rules`はlist | group/entry countとschema形状を確認したのみ。array内value・個別rule/条件ごとの行動は未調査。Java launcher expansion全体を複製せず、Pomme適用fieldは個別レビューする。 |
| `assetIndex` | `id=32`, `sha1=52695890153d94cf946455da532806db8c530831`, `size=586366`, `totalSize=480526482`, URL付き | identity/hash/sizeを固定してindex取得・検査する基準。 |
| `assets`, `complianceLevel` | `assets`は文字列`"32"`（数値ではなく文字列key）；`complianceLevel`はmetadata内に存在 | `assets`値の用途・Pommeでの必要性をinventoryし、不要扱いも根拠を記録する。 |
| `downloads` | `client`, `server`。client descriptorにはurl/sha1/sizeの現行downloaderが読む値 | 選択version client artifactのidentity/完全性。server downloadをclient launch必須物に混ぜない。 |
| `id`, `mainClass`, `minimumLauncherVersion`, `releaseTime`, `time`, `type` | version identity・entrypoint・launcher gate・時刻・種別 | version selection/release snapshotへ固定し、Java mainClassをPomme binary entrypointと誤認しない。 |
| `javaVersion` | component `java-runtime-epsilon`, majorVersion `25` | Pommeはnative Rust clientをspawnする。metadata指定JRE取得を必要条件としていないが、互換対象から無言で除外せず、どのruntime fieldを非適用とするか記録する。 |
| `libraries` | **131件**。group-prefix count: `org.lwjgl` 80, `io.netty` 15, `com.mojang` 12, `org.apache.logging.log4j` 3, `com.google.guava` 2, `net.java.dev.jna` 2、残り17件は16 group | 131件のcount/group集計のみで、各entryの個別rules/artifact/actionは未監査。Java-runtime librariesはlauncherのartifact selectionがPomme runtimeにも必要かownerがreviewする残条件。これだけで全体100%を不可と固定せず、適用可否が判定されるまで100%未確定。 |
| `logging` | `client` configuration | Java client logging設定。Pommeログ・launcher consoleとの同等性は別ownerの確認事項。 |

**Asset object集合は未確定。** 手元cacheにはasset index documentがなく、26.2全object count/category/key/sizeはunknown。`assetIndex.totalSize=480526482`はmetadata宣言値でobject countや実取得割合の分母ではない。categoryを推測列挙しない。version metadataの `assetIndex.sha1/size` は手元 `26.2.json` のtrusted expected fieldであり、実index bodyは未入手のためactual digest/size未確認。将来index実体と照合し、全object key/hash/sizeを棚卸しする。

ネストschemaは保存metadataで確認したfield名までを記録し、各array内value/rule/library entryの個別行動は未調査と区別する。`arguments`は42 entries (30 strings, 12 objects); objectの`value`はstringまたはarray、`rules`はlist。rule fieldは`action/os/features`; `os`は`name/arch/versionRange`、rangeは`min/max`; `features` key集合は `has_custom_resolution`, `has_quick_plays_support`, `is_demo_user`, `is_quick_play_multiplayer`, `is_quick_play_realms`, `is_quick_play_singleplayer`。`assetIndex` schemaは`id/sha1/size/totalSize/url`、`downloads.client/server`は`sha1/size/url`、`libraries` entryは`name/downloads/rules`、`downloads.artifact`は`path/sha1/size/url`、`logging.client`は`argument/file/type`、logging `file`は`id/sha1/size/url`、`javaVersion`は`component/majorVersion`。`assets` field valueは文字列`"32"`。schema listingは保存JSON上の構造確認であり、個別配列全件の監査やasset object inventoryではない。

このため「100%達成済み」や総合互換率は宣言しない。分母は **26.2適用引数契約 + metadata fieldとnested applicability判定 + 全取得object/client JAR artifact + 対応platform/release契約 + failure case**。asset full inventory不在中は総合100%不可。Java-runtime libraries 131件の個別artifact selectionは隣接owner reviewの残条件だが、それだけで100%を恒久的に不可と決めず、Pomme runtimeへの適用可否が確定するまでは100%未確定とする。

## 3. 現行実装と差分

以下はソースから確認できる静的事実と未検証の実動作を区別する。Pomme pathはClient checkout root、公式Main pathは `minecraft-26.2-decompiled/src/net/minecraft/client/main/Main.java`、metadata pathは `minecraft-26.2-decompiled/cache/26.2.json` と `minecraft-26.2-decompiled/cache/version_manifest_v2.json`（workspace root relative）。記載lineは1始まり。sourceの存在だけで実機動作・公式との一致を主張しない。

| 領域 | 現行実装（path:line / symbol） | 設計上の差分・未確認 |
|---|---|---|
| UI準備→起動 | `pomme-launcher/src/App.tsx:233–307::handleLaunch`, `:183–190::ensureAssets`。通常entryはensureAssets(currentInstall.version)後にinstall ID/serverVersion overrideを別々に渡す。server overrideがある場合の準備versionとspawn versionの不一致可能性を確認。 | P0/P1: overrideあり/なしとも一度解決した単一versionをimmutable LaunchPlanへ固定し、準備・検証・spawnで使う。requestID/generation表示なし。account/versionが処理中に変わっても古い結果で現選択をready表示/spawnしない。 |
| benchmark entry | `pomme-launcher/src-tauri/src/auto_benchmark.rs:213–251` は `ensure_assets`後に共有 `commands::spawn_game` を呼ぶ。 | 現行source path確認のみ、実benchmark比較なし。将来通常launchと同じserviceを通しdirect spawn bypass禁止。active childをselection変更で黙殺終了しない。 |
| ensure/readiness | `pomme-launcher/src-tauri/src/commands.rs:350–355::ensure_assets`; `pomme-launcher/src-tauri/src/downloader.rs:66–72::needs_download`。index file存在とextracted marker存在の否定でdownload判定。 | 存在だけでindex integrity/object completeness/content hashが満たされる保証はない。 |
| metadata/index | `downloader.rs:13–22::VersionManifest/VersionEntry`はmanifest recordの`id,url`だけをdeserializeし、`:74–124::download`はそれらからmetadata/indexを取得。`AssetIndexRef`はURLのみ (`:24–34`)。 | 保存 `version_manifest_v2.json` の26.2 recordは`sha1=33c420747ce582e48dff1d8c5d8e67e5bb6257c9`を持つが、downloader structはsha1を保持/検証せずmetadata sizeも宣言されない。expected metadata SHA1はtrusted manifest record由来（手元recordがなければ将来manifest取得に依存）、sizeは未宣言として扱い、捏造しない。metadata/index identity snapshotとHTTP status検査が必要。hashはtransport authenticity/signature証明ではない。asset-index SHA1/sizeはmetadataの`assetIndex` fieldを使うがindex body cacheがなく現actual unknown。 |
| asset objects | `downloader.rs:129–211::download_objects`。32並列、既存はsizeだけで再利用し、body直writeで受信hash/size/statusを検査しない。 | expected object hash/sizeは実asset index document由来。document未入手のため現行26.2 object inventory/expected values unknown。32は公式義務でない。tmp→validate→publish→readiness-last。必要量上限値もpolicy未確定。 |
| client JAR / extraction | `downloader.rs:214–282::download_jar`, `:285–322::extract_jar`。client descriptor由来sha1/sizeを使用するがexisting JARはsize reuse、SHA1 mismatch後も続行。 | `downloads.client.sha1/size` expected sourceを保持し、tmp→hash/size validate→publish。metadata SHA1はofficial metadata field整合性であり署名/transport trustとは別。archive path risk未実証を維持。 |
| runtime assets | `pomme-client/src/assets.rs:168–209::AssetIndex::load/resolve`; `pomme-client/src/app/core.rs:3306–3338` consumer。40 hex SHA1形式をfilterし、object resolve後にJAR assets fallback。 | launcherはindex/objectの完全性とreadinessを担い、asset runtime優先順位/reloadはclient ownerに置く。asset index full inventory未確定。 |
| Pomme client release | `pomme-launcher/src-tauri/src/client_updater.rs:34–94::platform_asset/ensure_client`はWindows x86_64, Linux x86_64 gnu, macOS aarch64選択とnetwork failure時cache fallback。`:96–148::install`はrelease SHA256 checksum存在時のみ検証し、欠落ならwarning後install/marker。 | SHA256 expected sourceはPomme release checksum asset。欠落時fail/resume policyは公式義務でなくsafe-launch policyとして明示決定。verified artifactのみtmp→validate→publish→readiness-last。archive riskは未実証。 |
| spawn / args | `pomme-launcher/src-tauri/src/commands.rs:365–385::launch_game`, `:480–580::spawn_game`。binary選択後、install/override versionを選びargsを構築してspawn。client側は `pomme-client/src/args.rs:3–48` をparseし、`pomme-client/src/main.rs:97–183` でrelease-only launch-tokenの存在確認/削除、protocol-supported version選択、directories verifyを行う。`dirs.rs:15–73` がdirsを解決/verifyする。 | `LaunchPlan`をprepareとspawnで共有し、unsupported version/platform、path・version不一致をspawn前にfail closedにする。Java Main引数とRust CLIの宣言/意味を個別mappingする。 |
| credentials / child lifecycle | `pomme-launcher/src-tauri/src/commands.rs:498–503` は固定temp path `pomme_launch_token` にtokenを書き、`:557–562` はaccount UUID/access tokenをraw argvに含める。`pomme-client/src/main.rs:100–117` はreleaseでlaunch tokenのpath存在を検査後削除。spawn child stdout/stderr/exitは `pomme-launcher/src-tauri/src/commands.rs:386–465` で収集。 | raw credentials argvと共有固定token pathはcredential ownerとの明示handoff contractなしには不十分。secret生成/認証はprotocol ownerの責任であり、この計画で上流authを上書きしない。 |
| second launch path | `pomme-launcher/src-tauri/src/auto_benchmark.rs:213–251` が `commands::spawn_game` を呼ぶ。今回は静的に呼出しを確認しただけ。 | benchmark実行・build・app起動は禁止。本番と同じLaunchPlan/preflight境界を将来共有するかは実装milestoneで確認する。 |

## 4. 互換にする設計

### 4.1 一貫したversion・artifact snapshot

UIから `LaunchRequest(installationId, explicit version, gameDir, platform, binary policy)` を受けた時点で、PreparationServiceがinstallation設定を一度解決し、request/generation ID + immutable LaunchPlanを発行する。serviceはsingle-owner command actorまたは同等mutexでrequest受理、cancel/invalidate、ready publish、spawn acceptanceを直列化し、network/hash/extract I/Oはlock外で行って完了をactorへ戻す。UIはrequestId/generation/monotonic revision付きmessageを現selectionと照合し、stale delayed Ready/Error/Progressで表示を上書きしない。cache/選択stateを正本にせず、古い取得結果は共有cacheへ安全にpublish可能でもready表示/spawn不可。prepare/spawnは必ず同じplanで、Tauri側で別のinstallation.versionを再解決しない。

準備完了後もready publishはactor内のcurrent-generation照合と同時にlinearizeする。spawn acceptanceでは同じserialization domain内でcurrent generation照合とspawn reservationをatomicに確定し、その予約を得たactorがOS spawnを実行してactive childを生成・管理する。新request/cancelがreservation commitより先なら古いspawn reject、予約commitが先なら旧generation spawnは受理済childとして管理し黙殺killしない。予約後OS spawn失敗はgeneration付き`SpawnFailed`として返しsafe handoff cleanupするがvalid prepared cacheを維持する。generation equalityだけでなくactor ordering/reservation stateが勝敗を決める。選択中のversion情報とは別に先行受理spawn child statusを保持・表示する。active child generationは保持し、複数child/cancel policyを明示決定する。

### 4.2 段階ごとの検証とreadiness

順序は次の通り。いずれかのstageが失敗・中断・不一致なら、そのgenerationはreadyにならず、既存の正常cacheを維持する。

1. 新requestの `installationId, explicit version, gameDir, platform, binary policy` を受理時にinstallationから一度だけ解決し、generation ID + immutable LaunchPlanへ固定。supported version/platformをpreflightする。
2. trusted official version manifest recordでversion entryを固定する。26.2の保存recordはSHA1あり、downloader `VersionEntry`は現在id/urlしか保持しない。metadata expected SHA1はmanifest record由来で照合し、manifest側sizeは未宣言として扱う。size capは別policy値で上限を設けるが数値は未決unknown。metadata transport/status/URL policyも検査する。SHA1一致は署名や配布元authenticityの証明ではない。
3. version metadata `assetIndex.sha1/size`と `downloads.client.sha1/size`をexpected値として固定する。index bodyをSHA1/size照合後parseしobjects一覧を作る。metadata内のexpected値欠落は推測せず未宣言として扱い、構造検証とsize upper-bound policyを適用（limit値unknown）。
4. object hash/sizeは実asset index entry、JAR hash/sizeはmetadata `downloads.client`由来。既存cacheも両方検証し、欠落・mismatch時は一時fileへHTTP成功status付きで取得し、expected size/hash/destinationを確認する。asset index bodyがない現状は全object values unknown。
5. metadata/index/object/JAR/binaryそれぞれ一時write → expected identity/size/hashと構造をvalidate → 同一filesystemでatomic publishの順を守り、準備物全体成功後にだけreadiness-last markerを確定。中断/mismatchで不完全fileをfinal-ready扱いにしない。
6. JARを安全な一時領域へ展開し、archive entry path containment検査後publish。extraction completion markerも最後にだけ書く。
7. Pomme client release/platformを選び、release checksumからSHA256を検証する。checksum欠落時のfail/resumeは公式launcher義務ではなくPomme safe-launch契約として決定する（未決）。検証前binaryをspawnしない。binary + matching release/platform + verified markerがreadiness条件。
8. asset/index/JAR/platform binaryのidentityとvalidated directories/launch policyをplanへ固定し、credential opaque handleを安全にbind。actor内でcurrent generationを照合してready publishをlinearizeする。
9. UI status messageにrequestId/generation/monotonic revisionを付し、現selection照合でstale delayed Ready/Error/Progressの上書きを防ぐ。spawn acceptanceはactor内のcurrent generation照合 + spawn reservationを一atomic stepでcommitし、reservation後のOS spawnはそのactorがactive-child生成まで責任を持つ。新request/cancelがcommitより先ならreject、reservation commitが先なら旧generation spawnを受理済childとして管理し黙殺killしない。reservation後のOS failureはgeneration付き`SpawnFailed`、安全handoff cleanup、prepared valid cache維持。結果はgeneration equalityだけでなくactor ordering/reservation stateに基づく。Tauri commandでinstallation/versionを再解決しない。child exit/statusは選択version情報と別stateに保持し、秘密情報なしで報告し、一時credentialをcleanup。

既存cacheはsource-specific expected identityと完全性を再確認したものだけ再利用。version metadata SHA1 expectedはmanifest record（手元にない環境では将来取得）、asset index SHA1/sizeは26.2.json `assetIndex`、object hash/sizeは実asset index（現欠落でunknown）、client JAR SHA1/sizeは`downloads.client`、Pomme binary SHA256はrelease checksumから得る。metadata expected sizeが未宣言なら未宣言のまま、size cap/JSON structure validationを適用し、上限数値はpolicy未確定unknown。metadata SHA1は公式metadata値との照合に過ぎず、HTTPS transport trust/signature/publisher authenticityと混同しない。Pomme release checksum absent時のfail/resumeはofficial obligationでなくPomme safe-launch contractとしてP0でsecurity/release ownerが決定。各artifactは一時write→validate→publish、readiness markerは全準備完了後最後に確定する。

### 4.3 path/archiveと並列性

全download/extraction destinationを期待root配下へ正規化し、archive entryの親参照、絶対path、symlink semantics、同名entry collisionをrejectまたは明示policyに従わせる。現状flatten extractionのbasename衝突やzip entry pathの影響は実証されていないため、実装前に対象archive仕様と既存zip library APIを確認し、事実に基づいて対処する。既存の32並列はtuning可能な実装値であって互換要件ではない。最初に正しさ・中断回復・resource limitsを定め、並列数最適化は計測根拠がある場合のみ扱う。

### 4.4 process・credential契約

`access-token`をargvへ渡さない契約を目標とし、protocol/auth ownerが認証・account選択・credential生成/更新を所有した上で、安全なopaque reference/handleだけをlauncherに渡す。handleのOS/child境界、権限、寿命、一回利用、cleanup方式（private file/pipe等）はplatform実現性とthreat modelをauth/protocol ownerと合意する。現sourceではaccess tokenをargv引数へ組み立て、child stdout/stderr lineをevent/log/error bufferへ流す経路があるため、secretが実際に露出したとの断定はせず、argv/log/event/error/temp file全経路を監査・sanitizeする受入条件とする。launcherはsecret内容を解釈・生成せず、childが開始しない場合もhandleをcleanupする。Rust clientのrelease-only `--launch-token` は別目的の起動gateで、現状固定temp pathへ生成・pathで渡し、clientは存在確認後unlinkを試みる。access tokenと同一視せず、nonce/collision/cleanup failure policyをprotocol/security ownerと合意する。認証生成/refresh/session semanticsの変更は本計画のscope外。

child監視はstdout/stderrの有限保持、終了code/signal、起動前/起動後のfailure distinction、user向け安全なerrorを定義する。現 `commands.rs:386–465` はstdout/stderrをeventとlog bufferへ送り、終了eventを発行するがgeneration相関はplan目標として未確認。将来のstatus/progress/exit/error eventにはrequest generationとchild ID/revisionを付し、古いchild eventが新しい選択/再試行のstatusを上書きしないようにする。secretが実際にlogされたとは断定せず、secret-bearing line/errorのsanitize・bounded retentionを検証する。child failureを成功表示やready状態へ誤変換しない。retryは新generationとして旧generationをinvalidateし、旧childは勝手に終了せず別statusで管理する。cancelは準備task cancel/invalidateとspawn reservation前後で意味を分け、予約後childのkill有無・複数child許容は証拠なしに確定せずowner合意事項にする。handoff/temporary artifactsはspawn拒否・spawn failure・child exitの各terminal pathでcleanupし、cleanup failureもgeneration相関のsafe errorとして報告する。

## 5. 実装順序

各milestoneは実装時の許可されたscope/checkを別途確定してから着手する。現在はplanのみで、下記checkや比較は未実施。

| Priority / milestone | 目的・依存 | 想定scope / 将来check | 比較ケース | 残条件 |
|---|---|---|---|---|
| **P0 — state・integrity・secret preflight** | UI LaunchRequest受理でinstallation/account reference/explicit versionを一度解決しgeneration ID + immutable LaunchPlanを作る。特にApp.tsx prepare=`currentInstall.version` 対 spawn=`serverVersion` override差を解消。prepare/spawn同一plan、single-owner actor serializationによるready publish/spawn reservation linearization、stale invalidate、artifact integrity/readinessを決める。P1の基盤。 | launcher preparation/download/readiness/client updater/credential contract最小files。将来: override有/無、準備中account/version変更、artifact hash/size/status、破損・中断時旧cache維持、競合request/cancelの両順序、stale cache publish許容/ready拒否、spawn reservation前後の競合、generation付きprogress/error/exit eventによる旧event隔離、revision付きstale UI防止、generation付きSpawnFailed/active child保持、retry/cancel/cleanup、credential cleanupのfocused check。 | A/Bでstale/corruptではspawnしない。LaunchPlan resolved version == prepared metadata/index/JAR version == spawned `--version`。request/cancel先行ではspawn reject、reservation先行ではchild管理。old child eventが新generation UIを上書きせず、retryは新generation、cleanup結果は成功/失敗どちらも対応するgenerationへ届く。child generationを保持し、新selectionで黙殺終了しない。benchmark比較は未実施。 | metadata manifest SHA1、metadata size未宣言時policy/limit、release SHA256 absent safe-launch policy、asset index expected fields、launch-token protocol owner合意、複数child/cancel後のchild policy。 |
| **P1 — main prepare→spawn path** | P0後、通常 `App.tsx::handleLaunch` と `auto_benchmark.rs` の双方をPreparationService経由に移す。direct Tauri spawn bypassを禁止し同一plan handoff。 | `ensureAssets`/`launch_game`/shared spawn mapping、version/dirs/binary/credential-handle mapping。将来: plan-to-argv contract, child lifecycle checks。 | overrideなし/あり双方でprepare identityとspawn `--version` が一致。account/version変更中の旧generationはspawn不可。通常/benchmark両entryが同じ境界を使用し、benchmark固有引数だけpolicyで差分化。A: Pomme launcherから公式26.2 serverへ接続開始まで。transport/protocol比較はowner。実benchmark比較は未実施。 | UI/Tauri/local-or-release binary/runtime assetsの引渡し整合。 |
| **P2 — platform/version/failure breadth** | primary path後に全supported version/platformとoffline/error/recoveryを網羅。 | metadata schema handling、release asset matrix、platform-specific path/archive behavior。将来: supported OS/arch matrixとHTTP/auth/disk/path/zip/network/offline cases。 | Aの各supported platformで同じmetadata identity・起動境界を比較。 | supported platform集合とrelease publication policyをownerが確定。未対応platformは明示fail。 |
| **P3 — progress/fidelity** | P0–P2の契約が安定した後、progress、retry UX、公式実装と対応する適用可能なlaunch behaviorを整える。 | event/progress/UI statusとobservability。将来: stage別progress、cancel/retry、safe error copy/static contract check。 | A/Bで準備段階・failure reportingの意味を比較。 | 公式launcher source/behaviorがない領域はofficial implementation parityと呼ばず、metadata/Main期待値との適合に限定。 |

Aのserver接続における通信方式・認証・protocol成功はlauncher単独の責任ではなくprotocol ownerとの共同判定。BはSteelMCでsingleplayer process/world開始が境界で、save/tickをlauncher一致判定に含めない。SteelMC pinは記録済だがsubmodule treeは未初期化のため、Bの実装比較は未調査。

## 6. 完了条件と比較ケース

### 6.1 完了条件

- **Selection / launch linearization invariant:** request受理で一度解決されたinstallation設定、明示version/gameDir/platform/binary policy、metadata/index identity、binary releaseとspawn引数は同じimmutable LaunchPlan由来。single-owner actorまたは同等mutex serialization domainがrequest受理、cancel/invalidate、ready publish、spawn acceptanceをlinearizeし、network/hash/extract I/Oはlock外。UIはrequestId/generation/monotonic revisionを現selectionと照合し、stale delayed Ready/Error/Progressを表示へ反映しない。ready publishはactor内でcurrent generation照合と同時に確定。spawn acceptanceはcurrent generation照合 + reservationをatomic commitし、request/cancel先行ならreject、reservation先行なら旧generation spawnを受理済childとして管理し黙殺killしない。予約後spawn failureはgeneration付き`SpawnFailed`・safe handoff cleanupで処理しvalid cacheを維持。勝敗はgeneration equality単独でなくactor ordering/reservation stateによる。active child statusをselection versionと別stateに保持し、App通常起動/benchmark両entryはservice経由で直接spawn bypassなし。複数child/cancel policyを明示。
- **Artifact invariant:** index/object/JAR/release binaryは期待identity/digest/sizeを確認してからpublish/readiness化される。HTTP failure・short/extra body・hash mismatch・disk write failure・中断ではmarkerを成功状態にせず、既存valid cacheを失わない。
- **Boundary invariant:** path containment/archive extraction、supported platform、unsupported protocol version、directories、Pomme-specific vs official argument mappingに対し、spawn前に成功/拒否を決定できる。
- **Secret invariant:** access token等のsecretはargv/log/UI/errorへ現れず、opaque handleの権限・一意性・寿命・cleanup・child failure動作がauth ownerと合意済み。launcherは認証実装を複製しない。
- **Lifecycle invariant:** child起動・終了code/signal・safe failureが正しく報告され、singleplayer world/tick/saveやresource reload等の隣接ownerの責務を混同しない。
- **Coverage denominator:** 26.2 applicable args + metadata field/nested applicability (including arguments values/rules and library entries) + all asset objects/client JAR + supported platform/release + failure cases. 131 Java-runtime library entry-by-entry applicability is neighbor-owner review in progress; alone it does not permanently prohibit 100%, but until applicability is determined 100% is unconfirmed. **Current asset index body/object set missing means full asset denominator unknown and overall 100% cannot be declared.**
- static check通過は構文・静的契約の確認に限り、公式との動作同一性を証明しない。A/Bの比較結果はケースごとに記録し、未実施を一致へ数えない。

### 6.2 比較ケース（将来実行。今回はすべて未実施）

| Case | 環境と境界 | 比較条件 / 判定点 |
|---|---|---|
| **A — Pomme → official 26.2 server** | Pomme launcherが26.2 asset/versionを準備しRust clientをspawn、公式26.2 serverへの接続開始まで。 | version/index identity、directories、argv/handoff契約、既存cache再利用、準備failure時にspawnしないこと、child exit reportingを記録。実protocol接続/認証/transportはprotocol ownerと分担しlauncher単独の一致へ帰属しない。 |
| **B — SteelMC singleplayer → official integrated server** | launcher境界からRust clientのsingleplayer world開始まで。比較相手は公式26.2 integrated server。 | game-dir selection、process launch、startup/failure境界を記録。world lifecycle/tick/saveはclient/Steel ownerでありlauncherの比較対象外。pinは `0b1f87c36a664f08d81397e942fb1e19f6eb282c`。 |
| **Negative / recovery** | A/B共通の将来case。 | corrupt cache、missing/unsupported metadata/platform、HTTP non-success、wrong size/hash、interrupted/partial download、archive path/collision、missing checksum、disk full、child early exit、credential cleanup失敗を、ready/spawn/秘密値を含まない報告で判定。選択変更中はrequest/cancelがspawn reservation commitより先のケース（reject）とreservation commitが先のケース（旧generation childを管理）を比較し、遅延Ready/Error/Progress/child-exit eventがrevision照合で現selectionを上書きしないこと、retryが新generationであること、予約後SpawnFailed時にcacheが保持されhandoff cleanupされることも確認。複数childとreservation後cancel時のkill semanticsはowner合意前に固定しない。 |

## 7. 依存・未確認事項

1. **公式launcherの実装が不在。** Main.java/26.2 metadataの宣言・artifact期待値と比較する計画であり、独自のdownload retry/cache/extraction仕様を「公式との差」として直接断定しない。
2. **artifact expectationsと未取得物。** 手元 `minecraft-26.2-decompiled/cache/version_manifest_v2.json` の26.2 `versions[]` recordは`sha1=33c420747ce582e48dff1d8c5d8e67e5bb6257c9`を含み、手元 `26.2.json` の実SHA1も同値とread-only照合した。現 `pomme-launcher/src-tauri/src/downloader.rs:18–22::VersionEntry`はid/urlしかdecodeせず、metadata sizeはmanifestで未宣言、現行ではSHA1照合もしない。将来expected metadata SHA1はofficial version-manifest record由来（record未存環境は取得に依存）、size未宣言として上限policy値unknown。metadata `assetIndex.sha1/size=52695890153d94cf946455da532806db8c530831/586366`はexpected values、actual index documentは未取得。objectsのhash/sizeはそのindex entry（現unknown）、client JARは`downloads.client` SHA1/size、Pomme binary SHA256はrelease checksum由来。checksum不在policyはofficial obligationではなくPomme safe-launch契約で決める。metadata SHA1をtransport trust/signature authenticityと混同しない。metadata/index/object/JAR/binary全て一時write→validate→publishしreadiness最後。
3. **credential owner契約が未決。** `pomme-launcher/src-tauri/src/commands.rs:557–562` raw argv access tokenと`:498–503` fixed temp token path、release clientの `pomme-client/src/main.rs:100–117` consumptionを解消する方式、OS権限、cleanup、開発build時挙動、失敗時policyをprotocol/security ownerと決める。auth generation/session仕様をlauncher計画が勝手に定めない。
4. **release checksum availability/policyが未決。** `pomme-launcher/src-tauri/src/client_updater.rs:96–148` はchecksum assetがないと警告継続。Pomme release側のchecksum署名/取得信頼、absence policy、既存cached artifactの再検証とoffline fallback条件をrelease ownerと決める。SHA1/SHA256一致自体をpublisher authenticityの証明とみなさない。
5. **platform/releaseの完全な対応範囲未確認。** ソース上の3 OS/arch asset名は確認したが、リリース運用やsupported runtimeの総契約、古いverified cache fallbackの許容性は未確認。全OS/arch比較を実施済としない。
6. **archive edge casesはrisk/verification item。** JAR extractionのentry path containment、client zipのflattenによるduplicate basename、symlink semanticsはsource reviewで要検証。現状悪用可能なvulnerabilityを確認済とは言わない。
7. **metadata version matrix未確認。** 26.2のfinite schemaをinventoryしただけで、他versionsのoptional field/rulesや将来metadata互換性を網羅していない。supported-version capability ownerと必要範囲を決める。
8. **SteelMCはpinのみ確認。** submodule tree未初期化。submodule updateをせず、singleplayer internal implementationを読んだ・比較したとは扱わない。
9. **本作業はdocs-only。** source/config/SteelMC/global設定、`docs/compatibility/README.md`、`docs/compatibility/data.json`は変更対象外かつ不変。Cargo/build/test/benchmark/live server/appは実行していない。通常App launchは `App.tsx:233–307` のensureAssets→launchGame、benchmarkは `pomme-launcher/src-tauri/src/auto_benchmark.rs:213–251` のensure_assets→shared spawn_gameと静的照合したのみ。実benchmark比較/A/Bは未実施。
