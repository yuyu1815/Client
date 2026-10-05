# Compatibility roadmap — Java Edition 26.2

## 基準・読み方

このroadmapと20件のfeature planは、common rootの`Client` repository、base `a12e38d9ea09d290a0b1736fe48a1cdafe49325f`に対する静的計画である。今回の統合では各planの記録済みchecked commit/blobをそのまま取り込み、依存表記の誤aliasだけを補正した。公式client/serverの固定JAR・resource/data corpusの取得、runtime比較、SteelMC再調査はいずれもこの計画統合の実施項目ではない。

20 planはそれぞれ異なる機能の**静的母集合・現在の根拠・未知範囲・owner境界**を持つ索引である。登録数や各planの候補数はそのplanに記載されたinventoryであって互換率ではなく、物理、操作、UI、描画、protocol/resources、singleplayer/serverなど異種の分母を足して「総合率」を作らない。依存は機能全体間の直列実装順を意味しない。明記のない挙動、実行時に観測していない結果、未取得resourceはunknownとして残す。

## Feature plan索引とowner

| ID / plan | 主な責務・静的母集合 | 主な引渡し / 現時点の未知 |
|---|---|---|
| [physics](plans/physics.md) | pose、movement mode、block context、移動・流体・衝突 | protocolの補正/authority、tick境界、全状態組合せ |
| [interaction](plans/interaction.md) | 照準、attack/destroy/place/use、sequence/ACK、cooldown | inventory stack、physics raycast、server permission/authorityとの契約 |
| [inventory](plans/inventory.md) | menu型、slot/click/input、recipe/trade/creative、stack/cursor | wire revision/ACK、server authority、全menuの実例比較 |
| [hud](plans/hud.md) | HUD情報・状態からdraw consumerへの合成 | 各producer snapshot、公式条件・数値、runtime描画比較 |
| [chat](plans/chat.md) | chat/command、signature・suggestion・UTF-16境界 | 全producer/consumer、permissions、失敗とserver ACK |
| [menus](plans/menus.md) | 画面遷移、options/bindings/settings、named screen | 全画面条件、Realms scope、settings伝播とauthority |
| [block-rendering](plans/block-rendering.md) | terrain/block/model/material/light、chunk mesh consumer | 26.2 assets、全state/model/neighbor、reloadと描画比較 |
| [atmosphere](plans/atmosphere.md) | sky/weather/fog/dimension/border presentation | resource completeness、shared lighting/transport/state snapshot |
| [living-entities](plans/living-entities.md) | living types、pose、species/baby/variant/layerとdraw | 93 living registrationsは静的inventoryのみ。全appearance/assets/runtime条件は未閉包 |
| [nonliving-entities](plans/nonliving-entities.md) | projectile/vehicle/display等の非living entity presentation | server simulation/authority、全variant/consumer/runtime条件 |
| [block-entities](plans/block-entities.md) | block-entity型、decode/update、renderer/delegate | 49型/26 rendererは静的inventory。非renderer consumer、NBT/resource/runtimeはunknown |
| [item-rendering](plans/item-rendering.md) | item/equipment/player/map visual consumers | stack/use、living pose、map state owner、資源と全visual context |
| [particles](plans/particles.md) | particle identity/options/provider/update/removal/render | assets/options/resource failure、全triggerとlifecycle比較 |
| [audio](plans/audio.md) | sound/music/subtitle producer、redirect、playback/lifetime | asset/OGG・subtitle corpus、device/backendと全trigger |
| [resources](plans/resources.md) | pack repository/priority, asset resolution, metadata, reload | corpus/version/hash、consumersのsnapshot・reload・rollback agreement |
| [connection-and-protocol](plans/connection-and-protocol.md) | auth/session, phase/registry/packet dispatch, transport/handoff | exact server fixture、Steel path、未閉包dispatch/orderingと失敗条件 |
| [launcher](plans/launcher.md) | version/artifact解決、download/validation、process handoff | pinned official metadata/artifacts、platform failure/runtime比較 |
| [singleplayer-lifecycle](plans/singleplayer-lifecycle.md) | singleplayer start, lock/save/recovery/shutdown/session UI | Steel/official integrated lifecycle、save/failure ordering/runtime evidence |
| [server-world](plans/server-world.md) | server worldgen/state/tick/clock/weather | full data/assets/world fixtures、lifecycleとのsave/clock boundary |
| [server-gameplay](plans/server-gameplay.md) | server authority/gameplay/entities/items/commands/progression | complete data/runtime/persistence closure、client-visible A/B outcomes |

Inventory内の具体数値（例: 93 living/65 nonliving、49 block entities/26 renderers、125 particle、25 MenuType）は個別planの根拠つき静的inventoryであり、実装coverageや互換率ではない。各planの未確認事項はroadmapから消さず、ownerとclosure条件を満たすまで開いたままにする。

## 比較モデルと判定軸

- **比較A — client parity:** Pommeと公式clientを、同一の公式26.2 server・同じ固定入力/state corpusに接続して比較する。server image/build、seed/state、trace、client設定・資源、入力とoracleを固定し、client予測とserver authorityの結果を別々に記録する。
- **比較B — 別環境のend-to-end:** Pomme + SteelMCと公式client + 公式integrated serverを別環境として比較する。server実装・authority・save/worldgen差を含むため、Bの差をclient単独の不足と呼ばない。A/Bの結果は混ぜず、それぞれのserver identityと期待値を保管する。
- compile/doccheck/static inventoryは実行時parityと別軸。今回のworkflowはdocs-onlyであり、build/test/ゲーム起動/実機比較をしていない。実際の比較許可とcaseごとのoracle/toleranceが未承認・未定義なら「互換確認済み100%」を主張しない。

## 共通milestones

### Stage 0 — 固定対象と証拠の来歴

公式26.2 client/server JAR、data/assets/index/生成表、decompiler出力のversion/hash/provenanceを固定し、Pomme現行source baselineと合法なcase corpus（取得元・利用条件・fixture/hash・環境）を記録する。source declaration数だけで全variant/state/例外/consumerの閉包を済ませない。93/65 entity、49/26 block-entity、125 particle、25 MenuType等はplanに出典のある静的inventoryとしてのみ扱う。未取得または利用条件不明のcorpusはblocker/unknownであり、代替値や推定率で埋めない。

### Stage 1 — 損失防止と高リスクの既存契約

まず既存境界でデータ喪失・誤ったsession・寿命破綻を防ぐ。launcherはresolved versionと実artifact/resourceの整合、protocolはsession/registry/dispatchとcredential/process handoff境界、inventoryはstack/cursor/state revisionの権威・同期、singleplayerはlock/save/shutdown、resourcesはreload/error/lifetimeを具体的な失敗・競合条件とともに確定する。提案された新secret channelや汎用generation frameworkを一律必須にしない。公式との差分、再現可能なrace/failure、threat modelが示された場合だけ必要な変更を選ぶ。block-entitiesの現decode-await経路についてはstale raceの証明がないため、generation追加を既定要件にしない。

### Stage 2 — 本番経路をつなぐ

各featureは現行のproduction producer → authoritative/state owner → immutable snapshot → consumer/draw/sendまで経路を閉じる。関連owner間でデータsnapshot/reload、stack/use/cooldown、entity pose/appearance、MapStoreと各visual consumer、tick/clock/authorityの所有契約を合意してから、独立実装を専用worktreeごとに並列化する。既存機構と標準library/platform機能を優先し、用途のない抽象層や先行frameworkを加えない。client visual ownerはserver authorityを複製せず、transportはdomain stateのownerにならない。

### Stage 3 — 各母集合の閉包

各planの対象ごとに全variant/baby/condition/layer/provider/option/menu/setting/world/gameplayを列挙し、登録/rendererの有無とsemantic一致を別々に判定する。singleplayer lifecycle、server-world、server-gameplayはclient-Aの比較に影響しない独立laneとして並列進行できる。feature全体を単純な依存nodeにして循環を作らず、共有契約の合意を先行node、各featureの実装・case closureをmilestone nodeとして扱う。実測できない/根拠のない範囲はunknownとして分母とownerを明示する。

責任の重なる境界は以下の通り分離する: `resources`はasset取得/pack優先順位/reload、`launcher`はartifact/version解決と起動。mapの共有state/protocolは`connection-and-protocol`等のstate/transport owner、item mapの描画consumerは`item-rendering`。living entity pose/stateとplayer appearance/equipment visualのconsumerは`living-entities`から`item-rendering`へ明示handoff。`server-world`はworld生成/clock/tick基盤、`server-gameplay`は権威gameplay、`singleplayer-lifecycle`は起動・save・shutdown/session調停を所有する。境界契約を共有してもownerを重複させない。

### Stage 4 — 固定corpus上の比較と完了判断

公式数値・tick順序・RNG seed/呼出順、visual/audio/input、platform/failure case corpusを固定し、A/Bごとに期待値/実測値、差分、許容差とその根拠を記録する。比較許可と具体的oracle/toleranceが得られ、全対象の未知範囲が解消するまでcompatibility 100%を宣言しない。compile/doccheck成功をruntime parityの代用にしない。Realms、動的resource pack、未取得/動的外部corpus等の未決scopeは分母から黙って除外して割合を水増しせず、scope未決として記録する。

## 次の小規模実装batch候補（提案のみ）

1. **inventory — creative操作/stack同期:** `plans/inventory.md`のcreative tab/search/clone/drop操作、component付きstack・cursor・state revision/server correctionが具体gap。入口: protocol packet/revisionとauthorityのfixture/contractを確定。退出: 代表creative操作の正確なclient→server→snapshot経路、stack/cursor期待値とfailure caseをAで記録。ここでは実装しない。
2. **menus — keybind/settings:** `plans/menus.md`のoptions/bindings永続母集合と画面伝播が入口。入口: 固定options/default/boundsとUI input owner契約。退出: key conflict/rebind/reset/reload/invalid settingを列挙し、保存値から有効consumerまでの経路・比較oracleを確定。
3. **launcher — resolved version/artifact整合:** `plans/launcher.md`のversion/metadata/artifact snapshot差が入口。入口: 公式26.2 metadata/JAR/indexのprovenance/hashを固定。退出: resolved versionからdownloaded/verified artifactおよびresource rootまで一貫するnegative/failure casesとprocess readiness境界を明文化。
4. **singleplayer lifecycle — save/shutdown境界:** `plans/singleplayer-lifecycle.md`のsession lifecycle・failure liveness・pause/autosave/shutdown契約が入口。退出: lock/save/flush/error/close順と再起動後の期待状態を、同一fixtureでSteelと公式integrated serverそれぞれの観測oracleに合意。大規模IPC/generation共通基盤を先行させない。

これらは次batchの候補であり、今回の統合で実装・比較・承認された作業ではない。