# Loom container: implementation前調査

調査時点: 現HEAD `914ebf29d0857783515689fdba6f7b97b6fcc354`。調査のみ。コード編集、commit、build、testなし。

## 結論

Loomはchest/generic9xではなく、`MenuKind::Loom` 専用の画面としてdispatchする。正規スロット40個の専用配置、loom-specific selectable pattern UI、通常の `container_button_click`、既存のserver slot/data同期で構成できる。プロトコルpacket追加は不要。選択パターンを固定レジストリIDで送る実装は不可。button idはその状態でloomが表示するselectable pattern list内のindex。

開くまでの現行経路は `NetworkEvent::OpenScreen` → `app/core.rs::container_screen_for_menu` → `OpenContainer` のslot buffer確保 → `ContainerScreen`描画。dispatch関数が `MenuKind::Loom` を扱わず `None` を返すと、OpenScreen処理は未対応menuとしてContainerCloseを返す。現行 `ContainerScreen` / `ContainerKind` / UI描画matchにLoomなし。

## LoomMenu layout / input-output

repoの `third_party/SteelMC/steel-registry/build_assets/menutypes.json` は `loom` のslot_count=40。LoomMenuの標準slot order:

| menu slot | LoomMenu role |
|---:|---|
| 0 | banner input |
| 1 | dye input |
| 2 | optional pattern-item input |
| 3 | result banner |
| 4–30 | player main inventory (27) |
| 31–39 | hotbar (9) |

したがって専用 `ContainerKind::Loom` は `slot_count()=40`, `inv_start()=4`, `crafting_result_slot()=Some(3)`。player inventory click mappingは既存の `inv_start` 規約で再利用可能。MenuType registryは存在するがPommeの画面routeには未接続。

## Packet / server state

- Pattern selectionは `ServerboundContainerButtonClick` (`container_button_click`): container id + button id。`third_party/SteelMC/steel-protocol/src/packets/game/s_container_button_click.rs::SContainerButtonClick` で両フィールドはVarInt→i32。Pommeのserverbound/outgoing型は `ServerboundContainerButtonClick`。button idは現在のselectablePatterns配列位置でありslot番号でもBannerPattern registry idでもない。
- 送信基盤は既存の `ui::container::ContainerResult.button: Option<u32>` → `app/phases/in_game.rs` のcontainer画面共通処理が現在のcontainer idを付けて `ServerboundGamePacket::ContainerButtonClick` を送る。`ui/enchantment.rs` のoption selectionが既存実例。Loom専用packet/senderは作らない。
- 選択済patternの同期はloomの `selectedBannerPatternIndex` DataSlot、property id 0。`container_set_data` の値が選択list内index。Steel packet `c_container_set_data.rs::CContainerSetData` は `container_id: VarInt, id: i16, value: i16`。Pommeの `NetworkEvent::ContainerData` handler (`app/core.rs`) は該当OpenContainerに対しdata[id]とdata_received[id]を既に保存するので、loomは `container.data[0]` と `data_received[0]` を表示へ渡すだけ。
- Inputs/result/banner componentの更新は通常の `container_set_slot` / `container_set_content` と既存ContainerClick/Close経路を使用。Loom専用metadataやblock-entity packetの追加は不要。結果をローカル生成・予測せず、serverから来るslot 3のItemStackを描画・takeする。
- Repo protocol-26.2 packet tableに `container_button_click` が存在。26.2でのmenu dispatchに不足があるだけでwire codec追加不要。

## Selectable patterns / item component

Button indexはserverが現在提示するselectable pattern list上の位置に対応する。リストを単純な全registry ordinalや固定の「6種」で置き換えない。Vanilla LoomMenuは `BannerPattern` registryと `minecraft:no_item_required` tag、およびpattern-item inputの `provides_banner_patterns` component/tagを基に候補を絞る。Conceptually: slot 2 empty → no-item-required patterns; slot 2 present → itemが提供するtag内patterns。Pattern button clickの後はserverが入力を再検査し、property id 0とresult slotを更新する。

Steel registryには必要な部品が既にある: `third_party/SteelMC/steel-registry/src/data_components/components/registry_holder_sets.rs::ProvidesBannerPatterns`, generated `vanilla_banner_pattern_tags.rs` (`NO_ITEM_REQUIRED` / `PATTERN_ITEM_*`), `BannerPatternRegistry`。ただしPomme UI内でこれらを使う既存実装はgrep上存在しない。UIは26.2 `LoomMenu#getSelectablePatterns`の順序を再現してcurrent selectable listを作る。タグの列挙順/Registry順/Direct holder list順は同じと仮定しない。Steel `RegistryHolderSet::contains`は照合APIで、ordered iteration APIとは限らないため、tag/provider listの順序を維持できる既存registry accessを確認してから実装する。Button idはこのserver同順listのindex。具体的な26.2 Mojang `LoomMenu#getSelectablePatterns`実装本文はrepoに同梱されておらず、タグ適用順/データパック拡張の厳密一致は実装前に26.2 server sourceで再確認すること。従ってこの調査ではlistロジックを完全検証済みとは主張しない。

Pattern selection actionは `ContainerResult.button=Some(index as u32)` とし、同じclickをslot operationとして二重送信しない。未選択値はserver DataSlot由来。resultはserver slot update由来。

## 既存UI基盤と差分案

再利用できる基盤:
- `ui/container.rs`: `Panel`, `push_panel`, `push_backdrop`, `SlotCtx`, `resolve_gesture`, `ContainerResult`, cursor/tooltip/slot click処理。slot clickはstate id・hashed stacks・server correction込みの `send_container_clicks` が既に送る。
- `app/phases/in_game.rs`: open-containerの画面dispatch、result.buttonのgeneric送信、container slots/data/state id保持。
- `app/core.rs`: OpenScreen, ContainerData, slot updates, close route。
- `player/menu_click.rs`: `inv_start`等の位置計算とslot click prediction。Loomはslot placement predicate/quick-moveが専用なので、最小版では既存Menu modelに偽装せずLoomのlocal predictionを明示的に避け、slot clickをserver-authoritativeにする（Beacon/Merchant/Horseの先例）。ContainerKindに追加する際は `apply_click`, `drag_distribution`, `drag_slot_eligible`, `build_menu` の各matchも確認する。
- UI asset pathは `renderer/pipelines/menu_overlay.rs` の `SpriteId` + background sprite tableが `minecraft/textures/gui/container/*.png` をasset managerから読みatlas化し、UI側 `push_panel` が使う方式。Loom登録は現状なし。vanilla 26.2に `minecraft/textures/gui/container/loom.png` が供給されるなら `SpriteId::LoomBackground` とasset table entryを足し、176x166 panelで利用。assetsはrepoに同梱された個別pngではなくランタイムresourceで解決されるため、26.2 resourceが存在することを別途確認。背景spritesが解決しなくても初期版は `push_backdrop` +既存slot/button/label primitivesで専用画面を成立させる。

石切台についての重要注意: `RecipeDisplayData::Stonecutter`, recipe book support, `MenuKind::Stonecutter` metadata, protocol translatorはあるが、`ui/stonecutter.rs` / `ContainerScreen::Stonecutter` / menu routeはrepoに見当たらない。よってloomが直接再利用できるstonecutter-specific screenはない。Enchanting UIはdedicated button operationの良い送信例だが、その3-option semanticsをloomに流用しない。

## 最小具体変更案 (コード変更は未実施)

1. `pomme-client/src/app/core.rs::container_screen_for_menu`: `MenuKind::Loom => Some(ContainerScreen::Loom)`追加。`OpenScreen`共通slot bufferはContainerKind::Loom.slot_count=40から自然に40枠作る。
2. `pomme-client/src/app/phases/in_game.rs`: `ContainerScreen::Loom` variant、`click_kind()` arm。OpenContainerには既存の`data[0]`と`data_received[0]`で足りるため専用selection field/stateを足さない。UI render matchから新しい`ui::loom::build_loom`へ slots/data/title/cursor/drag argsを渡す。
3. `pomme-client/src/player/menu_click.rs::ContainerKind`: Loom variant; slot_count 40, inv_start 4, result slot 3。Azalea `Menu`にLoom modelを追加しない。Loomは `apply_click`/`drag_distribution` のローカルprediction skipに含めてサーバーを権威とする。slot3 resultも通常click predictorに通さない。prediction skip/drag rulesの全matchを漏らさず調整。
4. `pomme-client/src/ui/mod.rs`: loom module登録。新規 `pomme-client/src/ui/loom.rs` は`SlotCtx`でslot0–3 + player rows 4–30/hotbar31–39を描き、pattern listを専用buttonとして描画・hit-test。クリックはContainerResult.buttonにindexを返し、そのframeでは通常slot clickにしない。data[0]をselected marker、slot3をserver resultとして描画。Generic chest screen/UI/slot layoutに偽装しない。
5. `pomme-client/src/renderer/pipelines/menu_overlay.rs`: vanilla loom GUI background spriteを登録する場合だけ`SpriteId`とasset mappingを追加。Pattern listは既存`MenuElement`/text/item icon and atlas renderingを使用; 新renderer/pipelineは不要。

## 検証案 (今回は実行禁止のため未実施)

- menu-route unit test: `MenuKind::Loom`→`ContainerScreen::Loom`→`ContainerKind::Loom`; slot_count=40, inv_start=4, result=3。既存`hopper_open_screen_dispatch_uses_native_layout`同型。
- Loom slot map test: 0..3 inputs/result; player inventory/hotbar starts 4/31; ensure no generic-9x chest mapping.
- menu UI/action test: supported example inputs + matching selectable list returns correct list-index button, not registry id or slot number; selecting pattern click doesn't also emit `ops`; DataSlot0 marker and server result stack displayed.
- Minecraft 26.2 loopback integration: loom open leaves `LoomMenu` active; place banner+dye and optional pattern item; select button and verify server echo property 0/result slot 3; take result, observe server-authoritative item with `minecraft:banner_patterns`; invalid/unavailable pattern doesn't fabricate output; close/reopen/control crafting/chest.
