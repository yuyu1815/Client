# 操作相互作用 — Java Edition 26.2 互換計画

- **共通base:** `a12e38d9ea09d290a0b1736fe48a1cdafe49325f`（Pomme source snapshot）。
- **対象版:** Java Edition 26.2。公式decompiled source root: `C:/Users/yuzum/Desktop/mine_rust/minecraft-26.2-decompiled/src/net/minecraft`。
- **比較経路:** A候補=Pomme client → 公式26.2 dedicated server、公式baseline=公式26.2 client → 同じdedicated server。B候補=Pomme client + SteelMC built-in singleplayer、公式baseline=公式26.2 client + 公式integrated server。BはSteelMCがserverとして動く候補構成であり、SteelMC clientが公式integrated serverへ接続する意味ではない。両候補をそれぞれ対応する公式26.2 baselineと比較し、server権威差を混同しない。
- **SteelMC観測:** read-only参照先`Client/third_party/SteelMC`のHEAD=`0b1f87c36a664f08d81397e942fb1e19f6eb282b`、確認時porcelain status clean。これはPomme共通base`a12e38d9ea09d290a0b1736fe48a1cdafe49325f`とは別の外部観測snapshotであり、同一性・parityの主張ではない。
- **状態:** 静的source照合に基づく計画。A/B実行比較、ゲーム起動、試験、互換率測定は未実施。100%は後述の閉じた分母・観測条件に対する目標であり、現在値を示さない。

## 対象・母集合と境界

対象は入力受付、照準候補選択、attack/stab、block destroy start/continue/finish、block/entity/item interaction、block placement、use-in-air/release、両手の結果順、cooldown/using/swing表示、sequence付きclient predictionとserver reconciliation。drop/swap等のPlayerActionも操作入口の母集合に含め、inventoryへ委譲する。

公式の操作母集合は`ServerboundPlayerActionPacket.Action` 8種（`minecraft-26.2-decompiled/src/net/minecraft/network/protocol/game/ServerboundPlayerActionPacket.java:74-82`）と、`ServerboundUseItemOnPacket`, `ServerboundUseItemPacket`, `ServerboundInteractPacket`, `ServerboundAttackPacket`, `ServerboundSwingPacket`等。関連packetの各hand/sequence/hit/secondary-use値を入口からserver handlerまで追う。結果母集合は`InteractionResult`の`Success`, `Fail`, `Pass`, `TryEmptyHandInteraction`とSuccessのswing source/item context（同`world/InteractionResult.java:12-70`）。use animationは`ItemUseAnimation`全値（同`world/item/ItemUseAnimation.java:18-30`）。

有限母集合をsource declarationだけで閉じたことにしない。`DataComponents`宣言（`minecraft-26.2-decompiled/src/net/minecraft/core/component/DataComponents.java:115-227`）、Item/Block宣言（同`world/item/Items.java:151-1358`, `world/level/block/Blocks.java:319-1234`）、`references/ItemIds.java`, `BlockIds.java`, `BlockItemIds.java`、built-in登録入口・data-driven component/tag割当、virtual dispatch/callbackを照合する。既知のsource宣言数（Item symbols 1,177、Block symbols 852、DataComponentType宣言111、対象Item behavior owner 49＋ItemStack dispatcher、対象Block use/attack直接宣言owner 66）は調査補助のカウントであり、unique registry ID数、全state数、全behavior閉包、互換率ではない。aliases/inheritance/動的・data-driven登録のID対応をresolveするまで母集合は未閉鎖。

**母集合の追跡単位と現状:**

| 集合/dispatch | source inventoryと現状 | 未閉鎖点 |
|---|---|---|
| PlayerAction / packets | 8 enum値を確認。Pomme start/continue/stop, use, attack, drop/swapの入口を照合 | 全packet ID/hand/sequenceと全server handler、各actionのmode/permission結果 |
| InteractionResult / hands | 4結果variant、3 swing source、item context確認。Pomme `ItemUseResult`はPass/Success/Failのbool相当縮約 | 全callerのresult-driven hand loopと全item/block/entity override接続 |
| Components | 111宣言を母集合候補とし、attack/use/break関連型を抽出 | 全componentの行動影響スキャン、全IDへの割当、tag/resource/data-driven値 |
| Item registry/behavior | declarationsと49のmethod-owner候補を列挙 | registry key→runtime item→継承dispatch→component/tag/callback全件対応 |
| Block registry/behavior | declarationsと66の直接use/attack owner候補を列挙 | ID/state→override、destroy lifecycle callbacks/fluid/neighbor/drop全件対応 |
| Animations/using/cooldowns | animation enumとPomme cooldown/use state経路を特定 | 全itemへのmapping、tick境界・同期・lifecycle比較 |

## 責務・状態所有

- **interaction:** 1回のinput/tickからaction intentを作り、照準結果をactionに配送し、official resultに基づく順序・一時的なprediction ledger・表示用attack/use/cooldown mirrorを所有する。server成功やstack countを正本化しない。
- **physics:** 視点・collision・ray geometryの入力を提供する。item/attribute reach選択・attack range適用をphysics側へ重複実装しない。target selectionの規則はclient picking owner、attack eligibilityの判定はcombat/server ownerに分ける。
- **inventory:** 選択slot、両手stack・component・count・server updateを所有する。interactionは操作時のstack/hand snapshotを読む。creative count復元やitem transformationのcanonical結果をinteraction側で確定しない。
- **protocol/connection:** packet codec、wire sequence型、受信順・ACK搬送・接続/level epochを所有。interactionは意味と必要な順序を要求し、wire representationを重複所有しない。
- **server-gameplay/server-world:** entity combat、permission、held item validity/use/cooldownとblock break/place/fluid/callbackの権威を所有する。client-side effect/predictionはserver acceptanceを意味しない。
- **render/HUD/animation/audio/particles:** interactionの結果や予測状態をconsumeするだけで、operation outcomeを決めない。

## 現行実装と公式source根拠

### 照準選択とvalidationは異なる契約

公式clientの照準選択は`LocalPlayer.raycastHitResult`: held active itemに`ATTACK_RANGE`があればそのrangeのclosest hitを優先し、block resultをblock interaction rangeでfilterし、MISS/未選択なら通常のblock/entity pickingへfallbackする。通常pickはblock traceを行い、block hit距離でentity rayを切り詰め、entity hitがblockより近い場合のみ選び、block/entity各rangeでfilterする（`minecraft-26.2-decompiled/src/net/minecraft/client/player/LocalPlayer.java:1189-1235`）。これは**client target picking**であり、server attack validation式を流用する箇所ではない。

Pommeの`InteractionState::update_target`は固定`ENTITY_REACH`とcreative bonusでentity reachを算出し、block `REACH` rayを行い、block距離までのentity候補と近さを比較する（assigned tree `pomme-client/src/player/interaction.rs:38-40,837-876`; `entity_hit_wins`は同file内）。この更新経路は`ATTACK_RANGE` item componentを参照せず、公式の特殊pick/fallback経路との差を埋める。Pommeはcomponent全般が欠如しているわけではなく、`menu_click::component<T>`がstack patch override/removed component/default prototypeを解決し（assigned `pomme-client/src/player/menu_click.rs:718-732`）、attack gateの`MinimumAttackCharge`とcooldown delayの`AttributeModifiers`を既に使う（`pomme-client/src/player/interaction.rs:1253-1261,2388-2432`）。攻撃component dispatch全体を確認し、既存対応も分母へ含める。公式serverは別にheld stackの`ATTACK_RANGE`を用い、entity target AABBに対して`Player.isWithinAttackRange(..., 3.0)`で攻撃を再検証する（`minecraft-26.2-decompiled/src/net/minecraft/server/network/ServerGamePacketListenerImpl.java:1756-1787`; `world/entity/player/Player.java:1891-1893`; `world/entity/LivingEntity.java:2120-2123`; `world/item/component/AttackRange.java:40-100`）。この3.0 bufferはserver側attack target AABB判定の引数であり、client pickingのreachへ足す共通公式値ではない。block interaction、entity interaction、item-use validationも各々別条件とし、1本の万能range式に統合しない。

### Attack gateとPIERCING/STAB

Pomme `InteractionState::tick`はinput snapshot・main/offhand stack/cooldown・modeを受け取り、click edgeで`start_attack`, held destroyで`continue_attack`, use press/held/use delayで`start_use_item`を順序付けし、using item中はattackをgateし、spectatorでmining/use stateをclearする（`pomme-client/src/player/interaction.rs:879-1089`）。`start_attack`はmiss cooldown/minimum charge gate、entity attack packet、block destroyへの分岐を持つ（同`interaction.rs:1253-1343`）。既存のticker、held componentによる`MinimumAttackCharge`、`AttributeModifiers` attack speed/delay処理がある（同`interaction.rs:1229-1261,2388-2432`）。ここを「通常attack/input/use機能がない」と誤記せず、公式のinput order/gate/range/cooldown/dispatchとの差を個別に評価する。

公式`Minecraft.startAttack`はmiss/hands-busy/spectator/feature/item gateの後、`cannotAttackWithItem(heldItem, 0)`を通し、`PIERCING_WEAPON` componentがあれば通常entity/block switchを通らず`MultiPlayerGameMode.piercingAttack`してmain handをswingする（`minecraft-26.2-decompiled/src/net/minecraft/client/Minecraft.java:1700-1755`）。通常entity hitはitem `ATTACK_RANGE.isInRange`を追加確認し、block hitはdestroyに送る。同method以降のswitch末尾やheld itemの関連component（`MINIMUM_ATTACK_CHARGE`, `ATTACK_RANGE`, attack modifiers, `PIERCING_WEAPON`, `KINETIC_WEAPON`, `WEAPON`, `DAMAGE_TYPE`, `BLOCKS_ATTACKS`, `SWING_ANIMATION`等）をすべて調査し、未読挙動を推測しない。`MultiPlayerGameMode.piercingAttack`は`Action.STAB`を送信しattack callbacks/soundを行う（同`MultiPlayerGameMode.java:515-521`）。server STAB handlerはspectatorと`cannotAttackWithItem(item,5)`を確認し、held `PIERCING_WEAPON`があればattackする（`ServerGamePacketListenerImpl.java:1232-1253`）。Pomme側にはfull-charge spear `PIERCING_WEAPON` dispatch TODOがある（`pomme-client/src/player/interaction.rs:1282-1284`）。normal ATTACKとSTABはgate・range・cooldown・server handlerを分けて比較する。

### Destroy lifecycleとprediction reconciliation

公式は`MultiPlayerGameMode.startDestroyBlock`, `continueDestroyBlock`, `stopDestroyBlock`, `destroyBlock`を別状態遷移として扱う。開始はblock restriction/world borderを確認し、target/tool identity、`state.attack`、instant/normal miningと`START_DESTROY_BLOCK`を設定。継続はcreative+border分岐、同一target、state destroy progress、完了時の`STOP_DESTROY_BLOCK`へ進む。target変更時は新しい開始へ遷移し、停止は通常の`ABORT_DESTROY_BLOCK`を送りvisual progressを消す（`minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/MultiPlayerGameMode.java:151-224,227-293`）。`destroyBlock`自体は`blockActionRestricted`, held item `canDestroyBlock`, GameMaster block許可、air gateを通し、`playerWillDestroy`の後fluid legacy blockに置換し、置換成功時にblock `destroy` callbackを呼ぶ（同`MultiPlayerGameMode.java:117-149`）。これらを単一の「予測でAIRにする」操作へまとめない。

Pommeには分離されたstart/continue/stop状態、held tool/target change、progress計算、action送信がある（`pomme-client/src/player/interaction.rs:2173-2385,2465-2525`）。ただし`predict_destroy`はprediction ledger保存後にworld stateをAIRへ置き、sound/particles/mesh dirtyを更新するだけである（同`interaction.rs:617-644`）。これが見た目の先行表示を示すだけで、`playerWillDestroy`/`destroy`, fluid legacy replacement, permission/gamemaster/world-border/canDestroy checksの意味的callback parityを示すものではない。修正設計ではserver権威world mutateとclient visual overlayを区別し、block callbacksをinventしない。

Pommeはpending stateにsequence/server state/player positionを保持し、server block updateをpending entryへ吸収し、ACKでrollbackし、rollback collision時のposition補正も行う（同`interaction.rs:590-698`; `app/core.rs:2124-2156,5395-5426`）。placementにも別のpending use ledgerが存在する（同`interaction.rs:340-380,748-790`; `try_place_block`同`interaction.rs:2135-2171`）。これらを壊さず、late update、rejected/accepted state、同一位置複数予測、inventory count updateとの順序、rollback player position条件を検証する。

### Sequence/ACK契約

Java packet sequenceはsigned `int`であり（`minecraft-26.2-decompiled/src/net/minecraft/network/protocol/game/ServerboundPlayerActionPacket.java:17-30,66-72`）、`MultiPlayerGameMode.startPrediction`は`int sequence = prediction.currentSequence()`を受けてprediction/packetを関連付ける（同`client/multiplayer/MultiPlayerGameMode.java:295-300`）。公式`BlockStatePredictionHandler.endPredictionsUpTo`は`serverVerifiedState.sequence <= sequence`をliteralに使い、`startPredicting`でint counterをincrementする（同`client/multiplayer/prediction/BlockStatePredictionHandler.java:38-54,63-68`）。本planは公式契約としてこの実装を記述し、modular orderingを公式仕様として導入しない。

Pomme interactionは`u32` sequence、`+= 1`、teleport sequenceとの大小比較とACKの`<=`/`>` retirementを使う（`pomme-client/src/player/interaction.rs:456-506,613-616,646-698,1484-1518,1794-1800`）。signed Java intとu32のwire decode/negative値/wrap、negative ACK解釈、再接続/level/respawn/dimension/reconfigure時にcounterがどのownerでresetされるか、late packetをどう除外するかの根拠を調査する。これを確認せずsigned比較への変更もmodular comparisonの追加もしない。既存のsequence, server-state shadow, rollback, player-position, late updateケースは保持し、epoch/reset boundaryをprotocol/connectionと合意する。

### Place/use, both-hand result loop

公式外側のuse loopは`Minecraft.startUseItem`で`InteractionHand.values()`順にhandごとのentity/block/item dispatchを行う。entity/block useでSuccessならswing sourceを見てswingし終了、block Failも終了、PASS等では必要な条件のもと次段階/次handへ進む。block item useの前後にはsecondary-use抑止、entity border/interaction range、feature gateがある（`minecraft-26.2-decompiled/src/net/minecraft/client/Minecraft.java:1774-1834`）。内側の公式block useは`MultiPlayerGameMode.performUseItemOn`でspectatorを処理し、secondary-use中に手持ちがあればblock actionを抑止し、block `useItemOn`を呼ぶ。resultがactionをconsumeすればその結果を返し、main-hand `TRY_WITH_EMPTY_HAND`なら`useWithoutItem`を再試行する。続いてcooldownでないitemの`useOn`へ進み、creativeではその呼出し中のstack countをrestoreする（`minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/MultiPlayerGameMode.java:339-382`）。`FAIL`, `PASS`, `Success`は一律boolではなく、outer callerのFail/Success分岐とresult semanticsに従う。entity interactionの全result条件など未確認caller枝は追加追跡し、未確認branchを仮定しない。

Official air-use `useItem`はspectatorでPASSし、carried slotを同期後、sequence付きpacketを作り、cooldown時はlocal interactionをPASSにする一方でprediction wrapper経由のpacketとresultを扱う（同`MultiPlayerGameMode.java:384-415`）。従ってpacket送信の有無とlocal item methodの実行/戻り値は別々に比較する。ItemStack `useOn/use`はitem override dispatch、instant-consumable side effect/remainder/cooldown/transformed held stackに接続する（`world/item/ItemStack.java:362-375,381-409`）。`InteractionResult.Success`はswing source `NONE/CLIENT/SERVER`とitem context/transformed stackを保持する（`world/InteractionResult.java:24-58`）。

Pomme `start_use_item`はentity/block/item/offhand分岐、具体的な特例、use packet、placement predictionを既に持つ（`pomme-client/src/player/interaction.rs:1405-1743,1774-1810`）。記述上の「unknown behaviorを省略」「entity resultはserver-authoritative」等の既存制約を維持し、bool/ID特例の数がofficial full dispatch parityを意味すると主張しない。比較ではmain-hand resultを先に受けて公式loop条件に従いoffhandへ進むか、block `useItemOn` → main-hand empty-hand retry → item `useOn` → air `use`のどこまで進むかをresult値ごとに記録する。各段階のswing source, local/server result, packet順, hand, secondary-use flag, transformed stack/count, use-state遷移を別fieldで測る。

Place予測も`try_place_block` local world overlayとpending inventory use/server updateを分ける。server-worldが保有するreplaceability, placement context(face/hit/inside), survival/feature/adventure/border/neighbor/fluid結果をclient success扱いにしない。inventoryがcanonical count/creative restorationを持ち、interaction predictionは拒否時rollbackとlate stack updateを調整する。

### Cooldownとusing lifecycle

CooldownはPommeに既存実装がある。`CooldownTracker`はtick、group→start/endを保持し、apply/expire/fraction/is_on_cooldownを提供する（assigned `pomme-client/src/player/cooldown.rs:7-69`）。比較対象はofficial `ItemCooldowns`のgroup選択（`USE_COOLDOWN.cooldown_group`またはitem registry ID）、`tickCount`, end/start arithmetic, `isOnCooldown`, expiry `endTime <= tickCount`, percent/clamp, apply/reapply/sync lifecycle（`minecraft-26.2-decompiled/src/net/minecraft/world/item/ItemCooldowns.java:20-72`）。`UseCooldown`は正secondsと任意groupを持ち、ticksは`(int)(seconds * 20.0f)`（`world/item/component/UseCooldown.java:26-42`）、ItemStackのuse side effectで適用される（`world/item/ItemStack.java:398-409`）。Pomme decode/sync/apply/tick/use gateも調べ、float→int truncation、zero、tick端点、同一group別item、再適用、packet/local result、lifecycle reset差を照合する。 cooldown不在と書かず、違いは証拠取得までunknownとする。

Pomme server-driven using flagのclearは処理するがtrueで使用状態を開始しないTODOがある（`pomme-client/src/player/interaction.rs:1935-1947`）。持続use hand/animation/duration/release/completionはitem behaviorとserver stateに結び、using bit単独から推論しない。slot/stack change, release, depleted/transformed stack, death/respawn, menu, teleport/dimension, disconnect等の破棄責務と各イベント送信元は別ownerと照合する。swing requestと実swing consumerも分ける。

## 互換設計・不変条件

1. **Target selection:** interactionがcurrent active stack/componentとmode等から公式client pickingの入力をまとめ、physicsの幾何結果をblock/entity candidatesとして評価する。ATTACK_RANGE優先pick、通常fallback、block obstruction/nearest candidate、block/entityごとのinteraction reachを独立判定する。server 3.0 attack bufferをclient pickingへ混ぜない。selection resultは操作可能/成功を保証しない。
2. **Attack:** input/cooldown/charge gate後に通常`ATTACK`かcomponent-gated `STAB`のいずれかを選択し、通常hit entity/block branchとpiercing pathを分ける。client click eligibility/predictionとserver item/range/game-mode validationは各ownerで行い、attack componentsは既存ticker/modifier処理を基盤に差分調査する。攻撃結果・damageはserver権威。
3. **Destroy:** mining sessionはtarget position + held item/components identity + modeに属し、start/continue/abort/finishを明示する。target/tool/mode/input transition時に前sessionを公式packet順で閉じる。`destroyBlock`相当のrestrictions, border, canDestroy, GameMaster, air, fluid-preserving replacement, callbacksは意味的なbreak結果として管理し、予測描画overlayから切り離す。
4. **Prediction:** operation ledgerはaction sequence/sessionとoperation, position, known authoritative state, predicted visual state, relevant hand/position contextを結び、incoming server updatesはpending prediction解決規則に沿い吸収/反映する。ACKは正規のserver confirmationに基づいてresolveし、rollback/late update/player-position correctionとrenderer/light dirty処理を一度だけ行う。sequenceのsigned/wrap/resetは証拠に従い決める。
5. **Place/use:** item/block behavior dispatch結果はtyped resultとcontextを保ち、公式のresult/hand orderを再現する。client placement/use effectsはoverlay/feedbackに限定し、server world mutationとinventory count/transformationは権威update後に確定。send packetとlocal method invocation/resultは別イベントとして記録する。
6. **Cooldown/using/swing:** cooldownはofficial group/time/tick arithmeticをPomme existing trackerへ対応づけ、server syncが正本。usingはhand/stack generationと開始/終了イベントを関連づける。animation/swingはresult `SwingSource`/handとdownstream consumerによって発生し、packet送信のみから推定しない。

## Milestones (P0/P1/P2/P3)

| 優先度 | 到達内容・依存 | 測定可能な完了条件 |
|---|---|---|
| **P0 — 権威境界と損失/誤認防止** | protocol packet/sequence/ACK lifecycle、inventory snapshot/update、server-gameplay/server-world authorityを確定。client pickingとserver validationを分離。destroy start/continue/finishの状態/acceptanceとSTAB経路を定義 | 各操作種でclient-selected target, outbound packet/sequence, server accept/reject, authoritative stateを記録でき、reject/late update/level resetでserver truthを損なわない。signed/wrap/negative/reset判断にsource根拠がある |
| **P1 — 公式操作遷移とprediction** | P0に依存。block break/place visual overlay、callback/world-state ownership、typed interaction result、both-hand orderを確立 | case table全resultに対しpacket order/hand/swing/local result/server result/count/world state/ACK rollbackを固定。fluid legacy replacement・target/tool switch・creative/spectator/secondary-useを含む |
| **P2 — 登録ID・behavior closure** | built-in registry/ID/resource data、components/tags/inheritance/callback scanを依存先含め解決 | 全built-in ID exactly-onceでdispatch owner + component/tag + exceptionsを記録し、各IDが適用可能なbehavior casesへ対応。declaration countを分母に代用しない |
| **P3 — cooldown/use lifecycle・A/B比較** | P1/P2とofficial server/integrated server比較環境に依存 | tick境界・hand・cooldown group・using/swing/transformed stackを固定ケースごとにA/B比較し一致を記録。全unknownを解決または対象外根拠付けして初めて定義済み分母の100%と報告可能 |

## 比較ケース母行列と完了判定

母行列は、`action {attack, STAB, destroy-start/continue/abort/finish, block-use/place, entity-use, item-air-use, release, drop/swap}` × mode `{survival, creative, adventure, spectator}` × hand `{main, off}`（公式上不適用の組は根拠付きN/A）× target `{miss, entity, block/state, border, occlusion, unloaded}` × registered Item/Block IDs and dispatch owner/component/tag/exception × result `{Success各swing/context, Fail, Pass, TryEmptyHand}` × cooldown/group/using lifecycle × prediction `{なし, same position repeat, update before/after ACK, accept/reject, late update, teleport/level/reset}`で構成する。全積を無条件に試すのではなく、全case cellを生成し、適用/除外/期待観測を記録し、意味のある組合せを選定する。未知ID/behaviorは開いたまま残す。

各caseで観測: input/tickとselected hit (block/entity/range/occlusion)、hand/stack identity、送信packet type/ID/order/sequence/hit/secondary-use、local behavior result/visual overlay/swing/use-state、server result・authoritative world/inventory/cooldown、ACK/update順、rollback/position correction、errors/disconnect。候補A=Pomme client→official dedicated serverと対応baseline=official client→同dedicated serverを比較する。候補B=Pomme client + SteelMC built-in singleplayerと対応baseline=official client + official integrated serverを比較する（SteelMC client→official integrated serverではない）。各系を別採取し、source presence、unit testの存在、declaration数、sample classの一致を実行動同値とみなさない。

完了とは: registry-ID/dispatch/component/tag/callback母集合が閉じ、全固定caseが結果または根拠付きN/Aへ一意に対応し、A/Bでuser-visibleとserver-authoritative observableが公式と一致し、rejection/invalid input/late eventで権威stateを壊さないこと。未測定互換率や架空の成功件数を報告しない。

## 回収したbehavior-owner inventory（source declaration候補）

下記は回収draftで宣言method-ownerとして列挙された名前を保持した追跡用inventory。Itemは49 behavior-ownerに`ItemStack` dispatcher wrapperを加えた50型、Blockは`useItemOn`/`useWithoutItem`/`attack`の直接宣言owner候補66型。対象source領域は公式root下の`world/item/`と`world/level/block/`（Blockのnested ownerは`BlockBehaviour`）。これは**宣言/owner候補inventory**であり、registry ID・継承dispatchの閉包でも、破壊callback（`playerWillDestroy`, `destroy`, `getDestroyProgress`, `canHarvestBlock`, `canSurvive`等）の網羅でもない。各source symbol/line、登録ID対応、behavior closureはP2で検証する。

| Kind | Owner names |
|---|---|
| Item behavior owner (49) + dispatcher wrapper (`ItemStack`) | `ArmorStandItem`, `AxeItem`, `BlockItem`, `BoatItem`, `BoneMealItem`, `BottleItem`, `BowItem`, `BrushItem`, `BucketItem`, `BundleItem`, `CompassItem`, `CrossbowItem`, `DebugStickItem`, `EggItem`, `EmptyMapItem`, `EndCrystalItem`, `EnderEyeItem`, `EnderpearlItem`, `ExperienceBottleItem`, `FireChargeItem`, `FireworkRocketItem`, `FishingRodItem`, `FlintAndSteelItem`, `FoodOnAStickItem`, `HangingEntityItem`, `HoeItem`, `HoneycombItem`, `InstrumentItem`, `Item`, `ItemStack` (dispatcher wrapper), `KnowledgeBookItem`, `LeadItem`, `LingeringPotionItem`, `MapItem`, `MaceItem`, `MinecartItem`, `PlaceOnWaterBlockItem`, `PotionItem`, `ShearsItem`, `ShovelItem`, `SnowballItem`, `SolidBucketItem`, `SpawnEggItem`, `SplashPotionItem`, `SpyglassItem`, `ThrowablePotionItem`, `TridentItem`, `WindChargeItem`, `WritableBookItem`, `WrittenBookItem` |
| Block use/attack declaration owner candidates (66) | `AbstractCauldronBlock`, `AbstractFurnaceBlock`, `AnvilBlock`, `BarrelBlock`, `BeaconBlock`, `BedBlock`, `BeehiveBlock`, `BellBlock`, `BrewingStandBlock`, `ButtonBlock`, `CakeBlock`, `CampfireBlock`, `CandleBlock`, `CandleCakeBlock`, `CartographyTableBlock`, `CaveVinesBlock`, `CaveVinesPlantBlock`, `CeilingHangingSignBlock`, `ChestBlock`, `ChiseledBookShelfBlock`, `CommandBlock`, `ComparatorBlock`, `ComposterBlock`, `CopperGolemStatueBlock`, `CrafterBlock`, `CraftingTableBlock`, `DaylightDetectorBlock`, `DecoratedPotBlock`, `DispenserBlock`, `DoorBlock`, `DragonEggBlock`, `EnchantingTableBlock`, `EnderChestBlock`, `FenceBlock`, `FenceGateBlock`, `FlowerPotBlock`, `GrindstoneBlock`, `HopperBlock`, `JigsawBlock`, `JukeboxBlock`, `LecternBlock`, `LeverBlock`, `LightBlock`, `LoomBlock`, `NoteBlock`, `PumpkinBlock`, `RedStoneOreBlock`, `RedStoneWireBlock`, `RepeaterBlock`, `RespawnAnchorBlock`, `ShelfBlock`, `ShulkerBoxBlock`, `SignBlock`, `SmithingTableBlock`, `StonecutterBlock`, `StructureBlock`, `SweetBerryBushBlock`, `TestBlock`, `TestInstanceBlock`, `TntBlock`, `TrapDoorBlock`, `VaultBlock`, `WallHangingSignBlock`, `WeatheringCopperGolemStatueBlock`, `piston/MovingPistonBlock`, `state/BlockBehaviour` |

## 依存・未確認・対象外

- **依存:** `physics`はeye pose/raycast/collision input、`inventory`はslot/hand stack/component/count lifecycle、`connection-and-protocol`はcodec/signed sequence wire/ACK transport/reset、`server-gameplay`はcombat/item-use permissions and entity authority、`server-world`はblock break/place/fluid/callback authorityを提供する。interactionはこれらの正本を複製しない。
- **未確認:** official block/item/entity use result loopの全caller・interaction packet順、all PlayerAction server paths; sequence negative/wrap/reset/level epoch behavior; `ATTACK_RANGE` client entity candidate exact details beyond cited method; destroy restriction caller全経路; full per-ID registry and inherited callback closure; every component/tag assignment; cooldown sync/reapply/lifecycle; all using/swing consumers; Pomme placer server rejection/count ordering; SteelMCとPomme/公式26.2とのbehavior parity。SteelMC pinとclean statusは上記観測snapshot時点で確認済みであり、互換性の証拠ではない。各未確認調査前に互換性の結論を作らない。
- **登録母集合:** `Items`/`Blocks` declaration countとoverride owner scanは候補source inventory。registry ID cardinality/ID→dispatch mapping/resource assignmentを解決するまではopen-world caveatを保持する。
- **範囲外:** packet codec全体の互換性はprotocol owner、canonical stack/inventory処理はinventory owner、physics全体はphysics owner、server gameplay/world callbacksは対応server ownerのplanで決める。本planはその接続契約を扱う。
- **検証制限:** 本文書段階ではCargo/test/build/benchmark/runtime/A-B比較を行わない。docs diffと参照sourceのline/symbol存在だけを確認する。
