# 在庫・コンテナ — Java Edition 26.2 互換計画

## 1. 対象と責任境界

### 固定条件・根拠の読み方

- 公式対象は Java Edition 26.2。decompiled source tree は `minecraft-26.2-decompiled/src/net/minecraft`（本計画の公式path表記はそのsource tree root相対）。Pommeのpathはこのworktreeのroot相対（`pomme-client/...`）。両方のsource lineは1始まり。公式参照の存在・列挙と、runtimeでの互換性は別の事実である。
- 計画baseは `a12e38d9ea09d290a0b1736fe48a1cdafe49325f`。SteelMC pinは `third_party/SteelMC` の `0b1f87c36a664f08d81397e942fb1e19f6eb282b`（baseでgitlinkを確認、clean）。SteelMCのsingleplayerは独立した実装/権威として扱い、公式serverと混同しない。
- 比較Aは同一の公式26.2 serverを固定し、Pomme clientと公式clientを比較する（client差を分離する）。比較Bはsingleplayer同士で、Pomme client＋内蔵SteelMC serverと公式client＋公式integrated serverを比較する。Bではclient差とserver実装差を別々に記録し、Aの同一server比較と混同しない。各構成で対応するserverがinventory/menuの最終権威となり、clientのclick・craft・trade・creative操作はrequest/predictionに留める。A/B runtime比較は未実施。
- 以下は「確認済み事実」「設計提案」「未確認」を明記する。decompiled JavaやRustの静的sourceを読んだこと、またはUI dispatchがあることを、動作一致や網羅確認とみなさない。

### 所有する動作・状態

在庫機能は、player 46-slot inventory、active menu slotsとそのplayer inventory alias mapping、carried/cursor stack、menu properties/data、menu固有入力/表示stateの同期と入力意味を計画する。権威snapshotを唯一の正本とし、menu viewとplayer inventory viewはそのsnapshotおよび明示slot mappingからderivedにする。predictionは正本を破壊せず、pending clickごとのoverlayとして重ね、server correction/rejectionで確定・破棄する。menuとplayer inventoryを別々に書き換えられる二つの正本にしない。

境界: menusはmodal shell/layout/focus/accessibility、interactionはmenuを開く一般要求、connection-and-protocolはwire encoding/transport/orderの提供、resourcesはitem/recipe/catalog data、server-gameplayはcraft/result/remainder/xp/trade/equipment/drop/permissionの最終validation・消費・生成・unlockを所有する。在庫はこれらを受け、menu lifetime・mapping・prediction・UI同期を保証し、serverの結果を推測して権威化しない。他機能のtreeやsourceは本計画の範囲で変更しない。

## 2. 公式母集合

### MenuType全件とcoverage台帳

`minecraft-26.2-decompiled/src/net/minecraft/world/inventory/MenuType.java:37-61` には25件の登録がある。想定数にかかわらず、この登録25件を分母とする。dispatch coverageは表示handlerの有無だけを示す。slot意味論・同期・結果・入力対応とは別の列で評価する。

| # | official id → class | 現行dispatch観測 | 全挙動coverage gate |
|---:|---|---|---|
| 1 | `generic_9x1` → `ChestMenu` | chest系画面handlerあり | open/close, 9 container slots, property/data, shift directions, server effectsを未監査 |
| 2 | `generic_9x2` → `ChestMenu` | 同上 | 18 slotsと同上を未監査 |
| 3 | `generic_9x3` → `ChestMenu` | 同上 | 27 slotsと同上を未監査 |
| 4 | `generic_9x4` → `ChestMenu` | 同上 | 36 slotsと同上を未監査 |
| 5 | `generic_9x5` → `ChestMenu` | 同上 | 45 slotsと同上を未監査 |
| 6 | `generic_9x6` → `ChestMenu` | 同上 | 54 slotsと同上を未監査 |
| 7 | `generic_3x3` → `DispenserMenu` | dispenser表示handlerあり | dispenser/dropper open context distinction、9 slots、button/effects未監査 |
| 8 | `crafter_3x3` → `CrafterMenu` | crafter handler有無・意味は未確定 | 9 input slots、disabled slots/property、craft trigger/result未監査 |
| 9 | `anvil` → `AnvilMenu` | anvil handler/rename UI観測 | input/result, cost/property, mayPickup, rename validation/effect未監査 |
| 10 | `beacon` → `BeaconMenu` | beacon handler観測 | payment/effect selection, permission/data, take constraints未監査 |
| 11 | `blast_furnace` → `BlastFurnaceMenu` | furnace-family prediction codeあり | slots/fuel/recipe/result/progress/quick-move未監査 |
| 12 | `brewing_stand` → `BrewingStandMenu` | dispatchあり。固有predictionは未確認 | bottle/fuel/ingredient slots, data, effects, shift未監査 |
| 13 | `crafting` → `CraftingMenu` | crafting handler観測 | 3x3 inputs, recipe/result/remainder/award/quick craft未監査 |
| 14 | `enchantment` → `EnchantmentMenu` | 限定prediction codeあり | enchant options, seed/cost/data/permission/result correction未監査 |
| 15 | `furnace` → `FurnaceMenu` | furnace-family prediction codeあり | fuel/recipe/result/data/quick move全数未監査 |
| 16 | `grindstone` → `GrindstoneMenu` | dispatchあり、predictionなし | result/XP/enchant transfer/slot rules未監査 |
| 17 | `hopper` → `HopperMenu` | 限定prediction codeあり | 5 slots/player mapping/quick move未監査 |
| 18 | `lectern` → `LecternMenu` | `menu_screen_for`未対応、現状fallback経路あり | book slot/page property/close/eject/button全挙動未監査 |
| 19 | `loom` → `LoomMenu` | dispatchあり、固有prediction未確認 | pattern selection/banner/dye/result/quick move未監査 |
| 20 | `merchant` → `MerchantMenu` | merchant UI/offer modelあり | offer index/costs/stock/xp/result/click authority未監査 |
| 21 | `shulker_box` → `ShulkerBoxMenu` | limited prediction codeあり | 27 slots, close/container effects, routing未監査 |
| 22 | `smithing` → `SmithingMenu` | dispatchあり、固有prediction未確認 | template/base/addition/result/recipe display/quick move未監査 |
| 23 | `smoker` → `SmokerMenu` | furnace-family prediction codeあり | recipe/fuel/data/result/routing差分未監査 |
| 24 | `cartography_table` → `CartographyTableMenu` | dispatchあり、prediction未確認 | map inputs/result/copy/scale/lock effects未監査 |
| 25 | `stonecutter` → `StonecutterMenu` | dispatch/recipe-choice表示あり | choice IDs, stale selection/result/quick move未監査 |

表中の「handler/predictionあり」はscreen dispatchまたはコード経路だけの静的観測であり、対応完了の意味ではない。根拠は `pomme-client/src/app/core.rs:8033-8084 (menu_screen_for)`、`pomme-client/src/player/menu_click.rs:129-200,243-294 (ContainerKind::build_menu/apply_click)`。Lecternのfallbackは同 `core.rs:4862-4870` の未認識menu処理に基づく。その他の「未監査」は、未確認を表し、公式仕様との差を断定しない。

別母集合として、`InventoryMenu`（46 player slotsと2x2 crafting）、`HorseInventoryMenu`のhorse/donkey/mule/llama/chest-bearing variants、`NautilusInventoryMenu`、creative modeを含める。entityごとのequip/storage slot、dynamic column数、open/close/despawn、permission、shift mappingは別々に確認する。`pomme-client/src/player/inventory.rs:4-16` はplayer inventoryの46位置（0 crafting result、1-4 input、5-8 armor、9-35 main、36-44 hotbar、45 offhand）を定義する。

全menu共通のcoverage列は、**open/close/lifetime、full-content、slot delta、cursor/carried stack、property/data、result/input/slot validity、quick-move方向とmapping、server side effects、rejection/correction**。各列の値は全menu×variantごとに比較caseへtraceし、空欄をpassと扱わない。全menuでmenu id/type、player inventory `-2` domain、active menu mappingが交差するpacketを確認する。

### Click/mouse/key/touch入力母集合

公式 `ContainerInput` の7値を全て含める: `PICKUP(0)`, `QUICK_MOVE(1)`, `SWAP(2)`, `CLONE(3)`, `THROW(4)`, `QUICK_CRAFT(5)`, `PICKUP_ALL(6)` (`minecraft-26.2-decompiled/src/net/minecraft/world/inventory/ContainerInput.java:15-22`)。

操作軸は全7 click type × official button値 × slot class（ordinary/input/result/restricted/player/hotbar/offhand/outside/invalid）× survival/creative/permission × empty/partial/full/component-bearing stack。mouse left/right/middle, number-key 1-9, offhand swap, shift, drop key, double-click, drag start/add/end, outside click, touch tap/drag, keyboard focus/navigation, close/cancelを入力 matrixにする。実際のkey/mouse/touch mappingはRust sourceで識別済みの経路のみ確定し、他入力の実装有無や各platform動作は未確認として追跡する。`-999` outside clickとcreative slot `-1`系のpacket semanticsを混同しない。quick moveはmenuごとのrouting、SWAPのhotbar/offhand、THROW、CLONE permission、PICKUP_ALL、dragのheader/type/stateを個別に照合する。

### Recipe book / crafting / trading / creative

公式登録の有限母集合（recipes datapack内の全recipe件数を意味しない）:

- Recipe serializer 21 (`minecraft-26.2-decompiled/src/net/minecraft/world/item/crafting/RecipeSerializers.java:30-52`): `crafting_shaped`, `crafting_shapeless`, `crafting_dye`, `crafting_imbue`, `crafting_transmute`, `crafting_decorated_pot`, `crafting_special_bookcloning`, `crafting_special_mapextending`, `crafting_special_firework_rocket`, `crafting_special_firework_star`, `crafting_special_firework_star_fade`, `crafting_special_bannerduplicate`, `crafting_special_shielddecoration`, `crafting_special_repairitem`, `smelting`, `blasting`, `smoking`, `campfire_cooking`, `stonecutting`, `smithing_transform`, `smithing_trim`。
- Recipe display 5 (`.../crafting/display/RecipeDisplays.java:14-20`): `crafting_shapeless`, `crafting_shaped`, `furnace`, `stonecutter`, `smithing`。
- Slot display 11 (`.../crafting/display/SlotDisplays.java:10-21`): `empty`, `any_fuel`, `with_any_potion`, `only_with_component`, `item`, `item_stack`, `tag`, `dyed`, `smithing_trim`, `with_remainder`, `composite`。
- Recipe-book category 13 (`.../crafting/RecipeBookCategories.java:10-23`): `crafting_building_blocks`, `crafting_redstone`, `crafting_equipment`, `crafting_misc`, `furnace_food`, `furnace_blocks`, `furnace_misc`, `blast_furnace_blocks`, `blast_furnace_misc`, `smoker_food`, `stonecutter`, `smithing`, `campfire`。

Inventory/re­cipe-book cases include all server-issued known recipe IDs/display IDs, ghost ingredient/filter/category, settings/search, recipe placement, 2x2 and 3x3 input grids, result take, ingredient consumption/remainder, shift craft, stale placement/menu and unlock update. Inventory owns display/input synchronization; server-gameplay owns final recipe validation, consumption, generation, awards/unlock. All actual recipe datapack contents and total count are **unknown**: decompiled Java registration is not a complete data-pack inventory.

Trading covers every offer index and ordered offer packet source; both cost inputs including item/components/count, result, disabled/selected/stale index, uses/max uses, restock, villager XP/level/progress, demand/price modifiers, selection and click sequence. Offer packet provenance and server authority must be recorded; merchant screen presence alone is not coverage.

Creative official tabs are 14 (`minecraft-26.2-decompiled/src/net/minecraft/world/item/CreativeModeTabs.java:63-79,87-1568`): `building_blocks`, `colored_blocks`, `natural_blocks`, `functional_blocks`, `redstone_blocks`, `hotbar`, `search`, `tools_and_utilities`, `combat`, `food_and_drinks`, `ingredients`, `spawn_eggs`, `op_blocks`, `inventory`. Current UI shows 12 tabs and omits `hotbar` and `op_blocks` (`pomme-client/src/ui/creative_inventory.rs:238-252`). Full dynamic registry/catalog/features, permission, component variants, search, hotbar save/load, creative slot-set/drop/clone and server rejection are coverage gates. Local catalog visibility must never be treated as proof of server grant permission.

## 3. 現行実装差分（source observation）

Source facts below are static observations at base; no runtime conclusion is implied.

| layer | actual tree-root path and symbols/lines | observed fact / status |
|---|---|---|
| packet receive | `pomme-client/src/net/handler.rs:711-742,747-770,841-850,1038-1065` | source has container content/slot/cursor, open menu, merchant/data/recipe event handling; packet ordering and every event mapping remain to audit |
| apply | `pomme-client/src/app/core.rs:4551-4573,4611-4630,4638-4658,4749-4760,4813-4879` | applies full content/stateId/cursor; routes slot updates to player inventory/menu domains; matches offer/data to active id; handles open/unsupported/server close. Epoch identity and stale packet fences are not present in observed menu identity |
| state & mapping | `pomme-client/src/app/phases/in_game.rs:123-149,292-353,1095-1176`; `pomme-client/src/player/inventory.rs:4-35` | open menu stores id/state and UI-related fields; inventory has 46 slots, snapshot resize and indexed setter; menu/player alias behavior must be made one explicit invariant |
| input & send | `pomme-client/src/app/phases/in_game.rs:3343-3430`; `pomme-client/src/player/menu_click.rs:129-200,243-294`; `pomme-client/src/ui/container.rs:20-54,1760-1787,1828-1868` | captures menu id/kind/state, mutates predicted state/drag and emits click operation(s). Multiple operations can be sent with same captured stateId in one route. Whether server permits/reconciles that exact sequence is an independent gate; do not assume correct revision progression |
| screen routing | `pomme-client/src/app/core.rs:8033-8084,4852-4870` | dispatcher has handlers for observed menu families, Lectern falls through unsupported close path. Dispatch is not semantic parity |
| inventory display | `pomme-client/src/ui/inventory.rs:39-55,73-125,139-209` | player inventory screen layout/slots; exact screen input parity still unverified |
| recipe book | `pomme-client/src/ui/recipe_book.rs:13-48,197-240,477-533`; `pomme-client/src/app/phases/in_game.rs:5800-5828` | client has recipe book state and recipe placement/settings/stonecutter path. Complete official registrations/data and unsupported behavior unverified |
| trades | `pomme-client/src/ui/merchant.rs:24-85,109-114`; `pomme-client/src/app/phases/in_game.rs:5817-5819` | merchant offer model/selection and transport path exist; full offer authority/behavior unverified |
| creative | `pomme-client/src/ui/creative_inventory.rs:66-84,238-252,317-324,598-600,655-662`; `pomme-client/src/app/phases/in_game.rs:5832-5895` | 12 tabs observed; Q action TODO at 598-600; outside carried stack currently locally cleared with TODO to send drop at 655-662. This is a concrete loss-risk path, not a measured loss rate or runtime A/B result |
| stack translation | `pomme-client/src/net/translate.rs:1505-1509,3774-3786` | source comments describe bare item/count reconstruction in a legacy-protocol hashed-stack conversion route. Scope is the 1.21.4 protocol translation path only; it does not prove a defect in direct 26.2 protocol stack handling |

SteelMC pin/clean state is observed at Git metadata above; inventory comparison of its runtime behavior remains unperformed. Source references to its implementation and full 25-menu official slot rules have not been exhaustively audited. Do not extrapolate a source observation to those unknowns.

## 4. 互換設計

### Lifetime, snapshots, revision and events

Menu identity for client state is `(container id, menu type, client open epoch)`. `stateId` is a wrapping revision, **not** menu identity; it must not be used to create identity or sorted with naïve integer `>` across wrap. A local open epoch fences local gestures/predictions and events already associated with a known lifetime, but cannot identify an old wire packet that carries only a reused id and no epoch/type: this is an explicit protocol limitation, not solved by epoch alone. For each event type record whether wire contains menu id, type, stateId, or none. Apply id-bearing event only when matching currently active lifetime/revision rules; for events without menu id (notably cursor/carried updates where the wire omits container identity), rely on protocol receive order and documented global-player/menu semantics, serialize application with the corresponding authoritative stream, and do not claim epoch can disambiguate it. If order/association cannot be proven, keep the event pending/mark ambiguous and request a protocol-owner decision; do not overwrite unrelated active view on guesswork.

Authoritative model holds snapshot, identity/lifetime, explicit menu-slot→player-slot mapping, cursor, properties/data and accepted revision. Derived views render the authoritative snapshot plus a separate prediction overlay. On full snapshot: validate type/id/count/domain; atomically replace matching authoritative menu/player/cursor state, reconcile pending clicks in send order, remove accepted/rejected overlays according to server evidence. Slot/property/cursor delta: validate event domain and identity/order, update only its authoritative field then recompute derived views. Close/reopen: end old epoch and cancel old gesture/prediction; no cross-epoch carry. Death/disconnect: discard transient prediction without deleting the last authoritative stack state before lifecycle owner settles it; clear/rebuild views only from lifecycle-authoritative result. Out-of-range or malformed data must not index/truncate silently; recovery action (close, wait, or available full snapshot) is explicit per actual protocol contract. Do not invent a resync packet: only use a packet proven to exist or wait for normal authoritative update.

Full snapshot, slot delta, cursor/carried, close/reopen, death/disconnect, server correction, click rejection/rollback and pending click reconciliation are all required cases. `stateId` is the modulo-32768 revision (`AbstractContainerMenu.incrementStateId`, official `minecraft-26.2-decompiled/src/net/minecraft/world/inventory/AbstractContainerMenu.java:859-865`); wire signedness/encoding and server check policy require protocol/source validation. Current path emits multiple operations using a captured same `stateId`; define and verify ordering/revision behavior as an independent release gate before changing it. Never silently claim one click per revision or synthesize progression without packet/server evidence.

### Stack and click invariants

Preserve stack identity/count/components, item maximum, slot maximum, equipment slot rule, menu-specific `mayPlace`/`mayPickup`, result-only take effects, dirty/changed slots and predicted changed-slot set. A derived player/menu alias must show the same authority value under the mapping. Rejected prediction rolls back only the relevant pending overlay then reapplies subsequent still-valid intents or requests an available authoritative update; no duplication or loss. Invalid slot, permission denial, stale menu, result/input restriction, packet rejection and correction are explicit rollback/resync comparison cases. Resync is a behavior category, not an assertion that a dedicated resync packet exists.

All 7 official `ContainerInput` values plus button/slot combinations follow official `AbstractContainerMenu` handling. For QUICK_CRAFT, use the official packed mask definitions `header = mask & 3`, `type = (mask >> 2) & 3`, `mask = (header & 3) | ((type & 3) << 2)` (`minecraft-26.2-decompiled/src/net/minecraft/world/inventory/AbstractContainerMenu.java:741-750`); header 0/1/2 denotes start/add/end and invalid transitions or empty cursor reset the gesture (`:753-761`). Type 0 evenly distributes floor(count/eligible slots), type 1 places one per eligible slot, type 2 fills to max and requires infinite-material permission (`:777-783`). End must account for component-aware stack identity, placement/drag permission and min(item max, slot max), preserving remainder on cursor; one eligible slot may use ordinary pickup semantics (`:411-437`). These exact rules are proposed only after verifying the 26.2 source at implementation; gesture cancellation/menu close resets state and late Add/End cannot affect a new epoch. Keyboard number/offhand/drop, mouse buttons/double-click/drag/outside, touch, hotbar, focus/cancel must converge on the same validated intent semantics; unverified input path remains unknown, not implicitly covered by mouse dispatch.

### Recipe, trade, creative and server authority

Recipe book requests reference known server-issued recipe/display IDs, never locally guessed IDs. Client ghost/filter/settings/search/placement are views and input requests. For 2x2/3x3, result/take/remainder/shift-craft, server-gameplay validates and owns consumption, production, unlock/award. Missing recipe pack or unknown registered display is a hard coverage blocker, not zero recipes.

Trade UI reflects the complete server offer list/index/order, both cost inputs, result and uses/stock/xp/level/progress/modifiers as supplied. Client selection/click is a request; final eligibility, cost consumption, output, restock and rejection belong to server authority. Reordered/stale offer updates must not be applied to another menu lifetime.

Creative catalog supports all 14 official tabs and dynamic registry/catalog/feature flags/permissions/search/hotbar save-load. Creative slot-set/drop/clone are requests. A Q TODO and current local cursor-clear are confirmed source facts; design must preserve cursor/item until authority accepts the drop or corrects it, without assuming local catalog access grants permission. Compare survival↔creative, permission absent↔present, denial and mode transition; distinguish visible local catalog from authoritative ability to set/drop/clone.

## 5. 実装順序 / milestone gates

Each milestone is a design/implementation proposal, not a code change in this plan. Candidate paths identify likely owners only; exact diff is determined after source audit. Each exit is conjunctive: a successful Cargo/check cannot replace behavioral comparison. Shared owner contracts are dependencies, not permission to write those trees.

| milestone | candidate change areas (not changes) / dependencies | entry → exit conditions, compare input → observed output, block/unknown |
|---|---|---|
| **M1 identity + authority snapshot + stack** | Candidate: `pomme-client/src/app/core.rs`, `app/phases/in_game.rs`, `player/inventory.rs`, protocol receive contract. Depends on protocol ordering/field presence and lifecycle source review. | Entry: record each event's wire identity fields/order; agree authoritative snapshot and slot mapping. Exit: epoch/type/id lifetime, revision separate, explicit mapping and mirror invariant; full/slot/cursor/property, close/reopen same id, death/disconnect, malformed/invalid slot, server correction/rejection/pending click preserve components/count/equipment and reconcile without loss/duplication. Input: delayed old update, full snapshot + delta + cursor, same-id reopen, invalid indices, permission denial. Observe: exact authoritative/model/UI stacks and overlays at each step; recovery uses only existing protocol mechanism. Block if event cannot be associated/order unknown or resync mechanism assumed rather than verified. |
| **M2 all 7 click types + player/basic menus** | Candidate: `player/menu_click.rs`, `ui/container.rs`, `app/phases/in_game.rs`, inventory tests; protocol packet ordering. Depends on M1. | Entry: mapping invariant holds. Exit: all seven types/buttons and keyboard/mouse/touch-origin inputs route through validation; 46 slots and basic/player + six generic chest sizes and basic furnace/hopper/shulker routes audited; component/max/equipment/result and dirty/predicted changed slots preserved. Input matrix: pickup, shift both ways, hotbar/offhand, clone/throw/pickup-all, drag, outside, doubles, invalid slots, fast sequential clicks same stateId. Observe: operation sequence, stateId/revision sent, authoritative corrections and visible prediction. Block on unverified same-stateId batching or untested menu route. |
| **M3 every unique menu + recipe/crafting/trading/mount** | Candidate: menu dispatch/model + recipe_book/merchant and server-gameplay/protocol contract. Depends on M1-M2, all 25 registrations, official menu semantic audit, recipe/data fixtures and offer authority. | Entry: one row per menu×variant with slots/properties/shift/result/close state; server ownership contract. Exit: all 25 MenuTypes + InventoryMenu + Horse/donkey/mule/llama/chest variants + Nautilus covered; recipe serializer/display/slot-display/category registries listed and available recipes/data pack separately enumerated; known recipe/display IDs, ghost/filter/settings/search/placement, 2x2/3x3/result/consumption/shift craft and all trade fields/click covered. Input: open/close, every slot/quick move, result, page/recipe, stale offer, restock, mount variant/despawn. Observe: full state, request, accepted/rejected server result and A/B separation. Block while any source audit, datapack count, mount variant or server authority output remains unknown. |
| **M4 creative 14-tab fidelity** | Candidate: `ui/creative_inventory.rs`, app input/send, resources catalog, server permission contract. Depends on M1-M3 plus dynamic registry/catalog/features and permission data. | Entry: official 14 tabs enumerated and current 12-tab difference confirmed. Exit: all tabs/content/components/variants/search/hotbar save-load, Q/drop/clone/set-slot and all permission outcomes compared; cursor/item retained until accepted. Input: survival↔creative and no-permission↔permission transitions, op_blocks/hotbar, denied creative packet, component-bearing stack drop/clone. Observe: catalog contents separate from server acceptance, persisted hotbar and conservation. Block on missing catalog, feature/operator matrix or unresolved Q path. |
| **M5 input/fidelity integration** | Candidate: inventory↔menus/HUD/resources/protocol/server-gameplay contracts; no ownership shift implied. Depends on M1-M4 and all per-case gates. | Entry: matrix traces every finite registration and explicit unknown/data-dependent universe. Exit: all keyboard/mouse/touch, focus/cancel, accessibility feedback, timing/failure/correction integrated in A and B; each case records expected and observed state, logs/fixture, server identity. Input: latency, reorder/duplicate/stale ack, close/reopen, disconnect with prediction, complete menus/click/recipe/trade/creative cases. Observe: no unintended loss/duplication, correct final authoritative state and equivalent user-visible behavior. Block if any unknown/unsupported/data unavailable/A-B unrun; Cargo/check success alone is insufficient. |

## 6. 完了条件と比較ケース

### Traceable case matrix

For each row in §2 MenuType table, plus InventoryMenu and every listed mount variant, create individual case IDs for open/close, full contents, each slot and delta, cursor, each property/data, input/result restriction, every applicable ClickType/button, shift routing, server side effect, rejection/correction. For click matrix record the actual valid button/slot cross-product from official source, not a guessed cartesian pass. Recipe matrix traces all 21 serializers, 5 displays, 11 slot displays, 13 categories and each available datapack recipe; trade matrix every offer index and both costs; creative matrix all 14 tabs and each display variant. Every case row has source refs, status (`not audited`, `expected defined`, `A pending/pass/fail`, `B pending/pass/fail`), fixture/input, observed output, discrepancy owner. No unrun/unknown row is complete.

Required case groups:

- **I1 lifetime/sync:** full snapshot, slot delta, cursor, properties, close/reopen with reused id; delayed id-bearing packet; cursor event without menu id; prove ordering/association instead of claiming local epoch fences wire packet. Include death/disconnect and pending click.
- **I2 correction/failure:** server correction, invalid slot/count, input/result/equipment restriction, permission denial, click rejection, prediction rollback/replay, available resync behavior. Confirm no fictional packet or silent data discard.
- **I3 click/input:** all seven `ContainerInput` kinds; per-type buttons and valid slot classes; left/right/middle, key number/offhand/drop/shift, double click, drag sequence, outside, touch, keyboard focus/cancel. Include partial/full/empty/component stacks and stack/slot maxima.
- **I4 menu trace:** all 25 listed `MenuType` rows and Inventory/Horse/Nautilus separately; inspect full-content, slot, cursor, property/data, result/input, quick move mapping, effects and close behavior. Explicitly compare Lectern because dispatch falls through today; test generic_3x3 dispenser vs dropper context; do not substitute screen dispatch for semantic result.
- **I5 recipe/crafting:** known recipe IDs/display IDs/ghost/filter/settings/search/placement; 2x2 and 3x3, result take, ingredients/remainder/shift craft, all serializer/display/slot display/category types, stale menu and server unlock. Record exact pack source/count; if absent, remain blocked/unknown.
- **I6 trading/mount:** every server offer index/order, both cost stacks/components, result, uses/max, restock, XP/progress/level/modifier/disabled click; each mount/equipment/storage/columns, close and despawn.
- **I7 creative:** 14 tabs and dynamic entries/components/features/search/hotbar save-load; creative slot-set/drop/clone; Q/drop TODO behavior, cursor retention, denial/correction; survival/creative and permission transitions.
- **I8 sequence:** multiple click operations sharing captured stateId; rapid click then correction, wrap `32767→0`, delayed/reordered/duplicate updates, latency, cancel/close, packet loss/disconnect. This is a separate gate; do not derive sequential validity from isolated-click passes.

### 100% meaning

100% is a target for this fixed 26.2 scope, not a current percentage. It requires every enumerated registered menu/click/recipe registration/creative tab, all applicable input combinations and data-dependent contents to be case-traced; both A and B independently compared; zero unresolved unknown, unsupported behavior, missing fixture or failed case. A/B require recorded server build/identity, packet/log/state fixture and exact expected-vs-observed final authoritative inventory/cursor plus user-visible state. A successful static audit, screen dispatch, Rust check, or only one environment passing does not establish compatibility. The finite Java registry lists do not close the datapack recipe universe; until recipe pack and dynamic catalog data are inventoried, denominator is open and 100% cannot be declared.

## 7. 依存・未確認事項

- **Protocol/connection:** each packet's presence/absence of menu id/type/revision, cursor association and ordering; stateId wire signedness/encoding and server acceptance; same captured stateId for multiple operations. Verify actual packet types before choosing sequencing/recovery. No made-up resync packet.
- **Official menu semantics:** all 25 classes' ordered slot creation, slot limits, mayPlace/mayPickup, properties/data, quick-move routes, close/result side effects and entity-specific variants remain unreviewed unless individually recorded in §2/case matrix. Lectern fallback fact is confirmed; all further Lectern behavior must be audited.
- **Data/resources:** actual 26.2 recipe datapack contents and dynamic creative registry/catalog/feature-flag variants are unavailable in decompiled Java tree; counts unknown. Recipe serializers/display registration is not recipe-data enumeration.
- **Server gameplay:** final craft/result/remainder/xp/trade/restock/equipment/drop/unlock and creative permission decisions plus rejection behavior require explicit authority contract. Inventory does not implement server policy.
- **Input and mirrors:** exact key/mouse/touch dispatch for every listed input and live consistency of player/menu aliases require runtime/source tracing. `set_contents` uses resize to 46 and therefore fills short vectors/truncates overlong vectors; this is an observed implementation behavior, not proof the protocol boundary should accept/truncate such packets. Decoder count constraints and official invalid-length behavior remain unknown.
- **Component translation:** `translate.rs` bare item/count reconstruction applies to the 1.21.4 translation route only. Verify separately; do not report it as a defect of the direct 26.2 path.
- **Comparisons:** neither A nor B has been executed; no A/B pass, measured compatibility rate, server log evidence or end-to-end equivalence is claimed. All unrun cases remain pending.
