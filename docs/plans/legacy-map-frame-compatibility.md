# 旧版 ItemFrame / map packet 互換計画（検証確定版）

## Goal / boundary

旧版サーバー上の地図入り `ItemFrame` / `GlowItemFrame` について、ワイヤー受信から地図面の基本表示までを、現行26.2のtyped packet・`MapStore`・map quadへ接続する。今回確定する第一段は**ItemFrame metadata indexの互換**で、MapItemData旧codecの完成を前提にしない。第二段のMapItemDataは公式wire layout・registry ID対応の根拠が版ごとに揃った範囲だけ実装する。スクリーンショットの根因は接続先protocolが不明なため未確定であり、欠落をスクショ根因の証明として扱わない。native 26.2の経路、shader/depth、wall-normalを変更しない。`ItemFrameInvisible` の本体問題は独立残件であり、今回の基本表示互換テストと混ぜない。

本体コード・build設定は変更しない。本計画書だけを更新した。通常build/testによるasset downloadは行わない。

## 調査で確認したコード経路

### ItemFrame metadata / entity kind

- `pomme-client/src/net/translate.rs::translate_entity_data` (約3853–3942) はmetadata serializer id/valueを現行typed decoder向けに変換する一方、index byteをそのまま出力する。TODOはpre-1.21.6 hanging entity indexを明記している。
- 境界は**protocol <=770 (1.21.5)** と **>=771 (1.21.6)**。旧ItemFrameはitem stack index 8、rotation index 9。1.21.6以降はdirection index 8、item index 9、rotation index 10。rotationはwire metadata上VarInt/typed `EntityDataValue::Int`。旧spawn-dataによる初期方向は既存経路のまま維持する。
- `pomme-client/src/net/handler.rs::handle_game_packet` / `dispatch_world_packet` は `ClientboundSetEntityData(p)` とsession protocolで `NetworkEvent` を作るが、ここにcoreの`EntityStore`参照はない。現状のItemFrame抽出は8=`Direction`、9=`ItemStack`、10=`Int`。
- AddEntity dispatchは `NetworkEvent::EntitySpawned { id, entity_type, spawn_data, item_frame_direction, ... }` を出す。`pomme-client/src/app/core.rs` はeventを順番に適用し、非living entityを`register_nonliving_spawn`→`EntityStore::set_vehicle_kind`に登録してからframe方向を適用する。`pomme-client/src/entity/mod.rs::VehicleState.kind` が `EntityKind`を保持する。`EntityStore::set_item_frame_item`, `set_item_frame_rotation`, `set_item_frame_direction` は既にItemFrame/GlowItemFrameだけを更新する。よってcore event適用時には実entity kindが利用でき、別kind tracking mapは不要。
- `RemoveEntities`は `NetworkEvent::EntitiesRemoved`→`EntityStore::remove_entity`を通る。ID再利用時は新spawnが同一store entryのkindを上書きする。Login/resetは`clear_world_entities`、Respawn/dimension resetは`EntityStore::new()`でクリアする。kind trackingをtranslatorへ新設すると、remove・ID reuse・reset cleanupの重複状態が必要になり、利点がない。
- 現index8 ItemStackはItem entityのmetadataにも使われる。handlerはそれを`EntityItemData`として発行し、coreは現在無条件に`ItemEntityStore::set_item_data`とrenderer mesh準備を行う。このため旧frame index8候補を既存`ItemFrameItem`にも送るだけでは、frame stackがItemEntityStoreにも誤適用される。**coreの`EntityItemData`適用を`EntityStore.vehicles[id].kind == EntityKind::Item`でguardする**。frame側は既存`set_item_frame_item`のkind guardを再利用し、mesh準備もframe kindを確認した後に行う。Spawn-before-metadataのevent順序をfixtureで固定する。

### MapItemData / downstream

- `pomme-client/src/net/translate.rs::Translation::translate_game_frame` がpacket ID/layoutの翻訳点。`GameIds.v764.drops`の `map_item_data` / `server_data` はprotocol <=764でpacketをhandler前にdropする。765以上はdropされないが、旧MapItemDataのlegacy layoutを現native typed decoderへ合わせる処理はない。
- `pomme-client/src/net/handler.rs::handle_game_packet` MapItemData armは現typed packetのdecoration registry ID→`MapDecorationAsset::from_registry_id`、name、patchを既存 `NetworkEvent::MapItemData`に移す。
- `pomme-client/src/app/core.rs` MapItemData eventは`MapStore::apply`を更新。`pomme-client/src/world/maps.rs::MapStore::apply`が128x128範囲と色data長を検証してpatchを反映する。`pomme-client/src/app/phases/in_game.rs`のframe stack `azalea_inventory::components::MapId`からquad候補を作る。通常ItemStackのMapId componentと、MapItemData packetのmap IDは**別の値経路**であり、混同しない。
- `pomme-protocol/src/registries.rs::ClientRegistry`には現状MapDecorationType registryがない。`RegistryTable::remap`を使う案は、MapDecorationTypeのsource/native tableが存在する版に限る。現行`MapDecorationAsset::from_registry_id`へlegacy IDをそのまま渡すのは禁止。legacy ordinal/ID→decoration semantic key→26.2 asset/native IDの明示対応を根拠付きで作るか、登録表を拡張して既存registry remapに載せる。未知値は誤spriteにaliasしない。

## 第一段：実装開始可能な確定案（ItemFrame metadataのみ）

### 採用案: handlerがprotocolでindex/valueを抽出、coreの既存EntityStore kind guardで確定適用

Translator kind tracking案ではなくこの案を採用する。根拠は`EntityStore.vehicles[id].kind`がspawn後coreに既に保存され、各ItemFrame setterがframe kindを検証する一方、translator/handlerはstoreを保持しないため。kind mapを重ねるより、現行のevent→store構造を一段だけ広げるのが最小である。

1. `pomme-client/src/net/handler.rs`のSetEntityData抽出を`crate::version::session_protocol()`で分岐する。
   - `protocol <= 770`: index 8 + `EntityDataValue::ItemStack` → 既存`NetworkEvent::ItemFrameItem { id, item }`; index 9 + `EntityDataValue::Int` → 既存`NetworkEvent::ItemFrameRotation { id, rotation }`。
   - `protocol >= 771`: 現行通り index 8 `Direction`, 9 `ItemStack`, 10 `Int`。旧branchでindex8をdirection扱いしない。
   - AddEntity `spawn_data`→既存`item_frame_direction`経路は変更しない。metadata index8のnative directionを旧wireへ合成しないため二重shiftもない。
2. 既存`NetworkEvent::ItemFrameItem` / `ItemFrameRotation`を再利用し、event型追加やwire値の再decodeはしない。coreは`set_item_frame_item` / `set_item_frame_rotation`を通す。item eventのmesh準備だけ先にkind-checkし、`vehicles[id].kind`がItemFrame/GlowItemFrame以外なら副作用を起こさない。
3. `NetworkEvent::EntityItemData` core armでは、`vehicles[id].kind == Some(EntityKind::Item)`の場合だけitem entity renderer/`ItemEntityStore`を更新する。これは旧frame index8 stackと実Item index8 stackをhandlerが両方の既存typed eventとして出すための対称guard。unknown/non-item kindは適用しない。AddEntity eventは同じqueue上でmetadataより先に処理されることをfixtureで確認する。
4. `EntityStore`にkind registryを新設しない。RemoveEntities・ID再利用・dimension/login resetのcleanupは既存EntityStore lifecycleを利用する。現行>=771とnative 776のdispatch/extractionはバイト・eventとも維持する。

### 第一段テスト配置と具体fixture

- Wire→Translation→typed metadata fixture: `pomme-client/src/net/azalea_compat.rs`の既存`translate_and_decode`・`translation_for(protocol)`を使い、protocol 770のSetEntityData index 8 ItemStack(MapId=17)/index 9 Int rotation、およびprotocol 771・776のindex 8 Direction/index 9 ItemStack(MapId=17)/index 10 Intをraw frameで作る。`pomme-client/src/net/translate.rs::translate_entity_data`がindexを変えずserializer/valueを保持することも検査。
- Handler event fixture: `pomme-client/src/net/handler.rs`既存tokio test patternと`dispatch_world_packet`を使う。protocol 770のfixtureはsession protocolをテスト用に設定し、SetEntityDataから`ItemFrameItem`(index8)と`ItemFrameRotation`(index9)だけが出る。771/776はnative mappingのeventを確認。
- Core/entity fixture: `pomme-client/src/entity/mod.rs::EntityStore::set_vehicle_kind`/`set_item_frame_item`/`set_item_frame_rotation`を使い、GlowItemFrame/ItemFrameはmap stack+rotationを受理、別kind(例 MinecartまたはItem)は同じindex相当値で`VehicleState`を不変にする。coreのEntityItemData guardではItem entityだけが`ItemEntityStore`更新し、frameとnon-itemは無変更・mesh side effectなしを確認。
- 統合回帰fixtureはAddEntity frame spawn→spawn-data direction→旧metadata→native 26.2 metadataの順にeventを流し、frame中心/facingがspawn fallbackどおり維持され、MapId=17がstack componentに残ることを検証する。古いmetadata index8がdirection eventへ誤変換されないこと、native index8 directionが従来通り機能することをassertする。
- 旧MapItemData decoderのテストを第一段の必須依存にしない。MapItemData不在でもframe stack保持・正しいframe entityのみ更新のテストは独立して成立する。

## 第二段：MapItemData旧版wire/layoutの証拠と作業境界

### 調査した資料・適用範囲

- `C:/Users/yuzum/AppData/Roaming/.minecraft/versions`を確認。既存official jars: 1.21.1, 1.21.2, 1.21.3, 1.21.4, 1.21.5, 1.21.6, 1.21.10, 1.21.11, 26.1.2。1.20.x / 1.21.7–1.21.9 official jarsは同folderに見つからない。`.pomme/data/versions`には1.8.2、26.2、26.3だけがあり、指定済み`26.2/26.2.jar`を確認。旧版763–766 jarなし。
- 公式mappings.dev検索結果は1.20.1 packet Map ID+scale+locked+decorations+MapPatch構成と、1.20.6以降の`MapId`, optional decorations, optional patch形、および`MapDecoration` typeがregistry Holderになった差を示す。ただしmappings field summaryだけではwire serializerの実装根拠にならず、parameter表示にもartifactがある。
- wiki.vg Booky protocol検索では766 (1.20.5) Map DataがMapId VarInt, scale byte, locked bool, optional icons, numeric decoration type VarInt, coordinate/direction bytes, optional Text Component, columns=0でpatchなし、正値でrows/x/z/dataを持つ構成と記載。検索結果はBooky本文が1.21.1に更新されているため、protocol 766の確定一次根拠にはせず、版別official jar/mappingsまたはPrismarine `data/pc/<version>/protocol.json` raw schemaでfixtures生成時に照合する。
- 該当参照URL: `https://mappings.dev/1.20.1/net/minecraft/network/protocol/game/ClientboundMapItemDataPacket.html`, `https://mappings.dev/1.20.6/net/minecraft/network/protocol/game/ClientboundMapItemDataPacket.html`, `https://mappings.dev/1.21.5/net/minecraft/network/protocol/game/ClientboundMapItemDataPacket.html`, `https://mappings.dev/1.21.6/net/minecraft/network/protocol/game/ClientboundMapItemDataPacket.html`, `https://mappings.dev/1.21.10/net/minecraft/network/protocol/game/ClientboundMapItemDataPacket.html`; patch: `https://mappings.dev/1.20.1/net/minecraft/world/level/saveddata/maps/MapItemSavedData%24MapPatch.html`, `https://mappings.dev/1.20.6/net/minecraft/world/level/saveddata/maps/MapItemSavedData%24MapPatch.html`; icon: `https://mappings.dev/1.20.1/net/minecraft/world/level/saveddata/maps/MapDecoration.html`。

### Version/layout table (wire 763–775)

表中`VarInt/Byte/Bool`は候補familyのpacket schema。未根拠箇所を実装時に推測してはいけない。`Text JSON/NBT`はtext componentのpacket encodingであり、item componentのSNBT/JSONとは別。

| Protocol | version.rs label(s) | 763–765 legacy family / status |
|---:|---|---|
| 763 | 1.20 / 1.20.1 | legacy packet: map ID VarInt, scale Byte, locked Bool, decorations list, color patch。decoration typeのordinal幅/record field codec・optional name JSON・patch width=0を含む完全wire orderはこのprotocolの公式jar/schema fixtureで未確認。`protocol-1.20.1.json`はpacket名/IDのみ。 |
| 764 | 1.20.2 | 同legacy候補。map_item_dataを現translatorがdrop。1.20.2 official jarなし; embedded JSONはpacket名/IDのみ。decoration/type/name/patch schema未確定。 |
| 765 | 1.20.3 / 1.20.4 | 同legacy候補。official 1.20.4 jarなし; embedded JSONはpacket名/IDのみ。decoration/type/name/patch schema未確定。 |
| 766 | 1.20.5 / 1.20.6 | 1.20.6 official jar/mappingsを今回取得。5-field packet composite、Holder registry codecはdirect registry ID。NBT optional Component、patch width=0 sentinel、35 keys (ID34 trial_chambers)をbytecode確認。 |
| 767 | 1.21 / 1.21.1 | official 1.21.1 jar: direct registry ID、同じ35-key order。 |
| 768 | 1.21.2 / 1.21.3 | official 1.21.3 jar: direct registry ID、同じ35-key order。 |
| 769 | 1.21.4 | official 1.21.4 jar: direct registry ID、同じ35-key order。 |
| 770 | 1.21.5 | official 1.21.5 jar: direct registry ID、同じ35-key order。 |
| 771 | 1.21.6 | official 1.21.6 jar: direct registry ID、同じ35-key order。 |
| 772 | 1.21.7 / 1.21.8 | 1.21.8 official jar/mappingsを今回取得。direct registry ID、same 35-key order。1.21.7独立jarは未所持。 |
| 773 | 1.21.9 / 1.21.10 | 1.21.10 official jar: direct registry ID、same 35-key order。1.21.9独立jarは未所持。 |
| 774 | 1.21.11 | official 1.21.11 jar: direct registry ID、same 35-key order。 |
| 775 | 26.1 / 26.1.1 / 26.1.2 | 26.1.2 official named-class jar: direct registry ID、NBT Component/optional-list/width-zero patchとnative body同型。26.1/.1別jarは未所持。 |
| 776 | 26.2 native | 現native typed path。`ClientboundGamePacket::MapItemData` / handler / MapStore; change forbidden. |

上表は初期調査の未確認欄を含む。763–765のPrismarine schemaと766–775のofficial bytecode確認結果は後続の「2026-10追補」で更新する。少なくともprotocol 766–774は同じfield bodyとdirect registry IDであり、registry ID+1やIdentifier special branchを実装してはならない。

### 第二段を閉じる最小実装/API案

1. `pomme-client/src/net/translate.rs::Translation::translate_game_frame`内のgame packet ID dispatchに`map_item_data` legacy armを置く。763–764の`v764.drops`から`map_item_data`だけ外し、`server_data`はdrop維持。packet raw payloadを厳密にdecodeして現26.2 `ClientboundMapItemData` wire layoutへencodeする。`MapStore`にlegacy wire decoderを追加しない。
2. 版差codecは実測で異なる境界だけ`decode_map_item_data_legacy(protocol, payload)`内で枝分かれし、共通後処理に`Translation::to_native: RegistryRemaps`相当の既存`RegistryTable::remap(ClientRegistry, id)`を再利用する。ただし現`ClientRegistry`にMapDecorationTypeがないため、先に生成registry tablesの有無を確認。無い場合、公式decor semantic key→現`MapDecorationAsset`の小さな明示tableを使う/ registry table生成対象へ加える、どちらも全ID根拠をfixture化してから着手する。raw old IDをnative IDとみなさない。未知type/IDはdecorをdropまたは既存Unknown assetへ落とし、誤ったnative spriteへaliasしない。
3. `MapItemData`のpatchなしは`None`にする。旧 width=0 sentinelはそのlayoutで証明できたfamilyに限定する。scale/locked、optional decorationsのNoneとSome(empty)、optional name(Text Component: JSON/NBT)、signed map id→native `u32`の範囲処理、color data byte lengthを保持/検証する。`MapStore::apply`の128x128 bounds/data length検証を再利用し、再解釈しない。
4. fixturesは`pomme-client/src/net/azalea_compat.rs`の既存`translate_and_decode`を基礎に、各証明済みdistinct protocolのraw MapItemData frame→translated frame→native typed decodeを作る。assert fields: map_id, scale, locked, decorations None/Some(empty), each decoration semantic ID/name optionality, patch None (0-width/absentの区別), x/y/width/height/colors、malformed/truncated拒否。MapId-bearing ItemStack fixtureは`MapId` component値とMapItemData `map_id`が一致しても独立assertする。protocol aliasは`pomme-protocol/src/version.rs::EMBEDDED`の共有protocolごとにfixtureを一つ使う。
5. MapStore/in-game test: `pomme-client/src/world/maps.rs`既存`MapStore::apply` fixtureにMapItemData eventを適用して非zero colorを残し、`pomme-client/src/app/phases/in_game.rs`既存map-frame quad抽出fixtureでItemFrame stackのMapIdが同一IDを指し、候補quad count=1であることをassert。MapStore/quad/render APIの本体変更は、既存fixtureで接続不能が実証された場合だけ。

## 第三段のend-to-end acceptance / scope

MapItemData codecを閉じた後、wire ItemFrame spawn + legacy SetEntityData + legacy MapItemDataをTranslation→native typed decode→handler→core events→EntityStore frame item(MapId) + MapStore patch→quad candidateまで通す。非zero pixelが保持され、同じframeのquad候補が**1件**であることをassert。frame metadata negative fixtureは同index/型の別entityが不変、Item entityのindex8は既存item entity経路のみ、frame index8はItemEntityStoreを変更しないことをassertする。directionはspawn-data fallback維持、native direction metadataを二重shiftしない。既存ItemStack non-empty component fixtureを維持。native 26.2 regression testは従来どおり。`ItemFrameInvisible` visibility testとは別test/別残件とする。

## 変更対象候補 (計画上のAPI境界)

- 第一段: `pomme-client/src/net/handler.rs` (protocol-specific typed extraction), `pomme-client/src/app/core.rs` (`EntityItemData` kind gateとframe renderer side effect guard)。既存`NetworkEvent::ItemFrameItem`/`ItemFrameRotation`および`EntityStore::set_item_frame_*`を再利用し、`NetworkEvent`型追加・translator tracking・全entity index補正はしない。
- 第二段: `pomme-client/src/net/translate.rs` (legacy raw MapItemData decoder/encoderとv764 drop exception)、必要性をfixtureで示せたときだけ `pomme-protocol/src/registries.rs` generated dataまたはmap semantic mappingを追加。
- tests: `pomme-client/src/net/azalea_compat.rs`, handler/core/entity unit tests, `pomme-client/src/world/maps.rs`既存MapStore tests, `pomme-client/src/app/phases/in_game.rs`既存quad tests。
- `pomme-client/src/net/mod.rs` event payload変更、`world/maps.rs`新codec、renderer shader/depth変更は不要。

## 検証記録 / 未確認事項

- `git -C Client status --short --branch`: exit 0, `## master...origin/master [ahead 2]`; 他ファイルに既存変更がある。今回本体コード変更なし。本計画自体は事前からuntracked。
- `Client/AGENTS.md`を読了。Rust build/testの入口はmiseだが本調査ではbuild/test未実行。
- `.minecraft/versions` / `.pomme/data/versions`の既存jarを列挙し、上記バージョンのみMapItemData調査で確認可能。26.2 official jarの存在を確定。
- web search: mappings.dev 1.20.1 vs 1.20.6/1.21.x packet/MapDecoration/MapPatch structure、Booky wiki.vg Map Data; Bookyは現行1.21.1本文へ更新されるため旧protocolの一次fixtureとして未採用。exact per-protocol raw bytes・MapDecorationType registry order・legacy JSON/NBT transitionは未確認。
- 第一段のmetadata採用案は確定。第一段の実装時検証: `mise run test`禁止ではないがコード作業時にClient/AGENTS.md指示に従い、少なくともtranslator/handler/core focused testを実行し、コマンド・exit codeを報告する。本調査では一切実行していない。

## 第一段実装結果（2026-04）

- `pomme-client/src/net/handler.rs`: `item_frame_metadata_event(protocol, id, index, value)`を抽出し、本番`SetEntityData` handlerがsession protocolを渡して呼ぶ。<=770はItemStack8/Int9、>=771はDirection8/ItemStack9/Int10。typed valueとmetadata indexをそのまま使用し、通常entity metadataのindexは一律shiftしない。AddEntity spawn-data directionは変更なし。
- `pomme-client/src/app/core.rs`: `apply_entity_item_data`をcoreの`EntityItemData`分岐とテストで共有し、`EntityStore.vehicles[id].kind == Item`でない場合はstore更新もmesh準備も行わない。frame ItemStack mesh準備もItemFrame/GlowItemFrame確認後に限定。`EntityStore` lifecycleと既存frame setter guardを使用しkind map/event型は追加していない。既存spawn経路では`register_nonliving_spawn`がkind登録し、EntityKind::Itemの場合に同じidを`ItemEntityStore::spawn_item`へ渡すことを確認。
- focused tests: `mise exec -- cargo test -p pomme-client --locked item_frame_metadata_events_follow_protocol_boundary_without_global_shift` はexit 0 / 1 passed (protocol 770, 771, 776のtyped event抽出と旧index8 Direction拒否/native index8 Direction維持)。`mise exec -- cargo test -p pomme-client --locked entity_item_metadata_only_updates_real_item_entities` はexit 0 / 1 passed (frameのEntityItemDataはItemEntityStoreに副作用なし、実Itemのstack情報は保持)。global session protocol変更はテストしていない。
- 全suiteを一度`mise run test`成功 (1505 pomme-client + 52 pomme-protocol + 1 pomme-singleplayer passed、1 ignored)したのはcore test helper追加前。その後の最終`mise run test`はexit 101でコンパイル失敗: 既存作業差分`pomme-client/src/renderer/pipelines/entity_renderer.rs:5699`および`pomme-client/src/renderer/util.rs:640,649`で`vkCommandBuffer`と`CommandBuffer`のtype mismatchが3件。最終状態でfull suite成功とは扱わない。`mise run check`もexit 101で`handler.rs`の既存particle test fixtureにある`0xff00_0000` overflowing i32診断があり、修正していない。
- wire→handler→store MapId=17→MapStore patch→quad count=1統合、non-frame negative integration、およびremove/ID再利用/dimension lifecycleのtestは未追加（focused helper testは上記2件のみ）。
- `git diff --check`は`Client` rootでexit 0。全repo fmtは未実行。指定file rustfmt checkは`handler.rs`/`core.rs`に他作業の既存差分のformat不一致が出たためfmt applyを避けた。MapItemData old codec、drop解除、decoder推測実装は一切していない。MapItemData fixture注入によるquad count=1も今回は未検証。

## 第二段追補（2026-10）：実wire契約の調査結果

この節は元計画の「layout未確認」記述を更新する。確認していない版は推測で埋めない。

### 763–766 raw protocol schema（PrismarineJS）

GitHub raw本文を取得し、Python標準`json`で`play.toClient.types.packet_map`とpacket ID mappingをparseした。`data/dataPaths.json`の`pc[version].protocol` aliasも照合した。参照:
- `https://raw.githubusercontent.com/PrismarineJS/minecraft-data/master/data/pc/1.20/protocol.json`
- `https://raw.githubusercontent.com/PrismarineJS/minecraft-data/master/data/pc/1.20.2/protocol.json`
- `https://raw.githubusercontent.com/PrismarineJS/minecraft-data/master/data/pc/1.20.3/protocol.json`
- `https://raw.githubusercontent.com/PrismarineJS/minecraft-data/master/data/pc/1.20.5/protocol.json`
- `https://raw.githubusercontent.com/PrismarineJS/minecraft-data/master/data/dataPaths.json`
- `https://raw.githubusercontent.com/PrismarineJS/minecraft-data/master/data/pc/1.20.2/mapIcons.json`

| Protocol (release labels) | dataPaths alias | packet ID | exact body order and widths | name / patch |
|---|---|---|---|---|
| 763 (1.20, 1.20.1) | 1.20.1→`pc/1.20` | `0x29` | `itemDamage:VarInt`, `scale:i8`, `locked:bool`, `icons:option(array[VarInt count]{type:VarInt,x:i8,z:i8,direction:u8,displayName:option(string)})`, `columns:u8`; columns=0なら後続なし、非0なら`rows:u8,x:u8,y:u8,data:VarInt-length bytes` | JSON component string; columns=0=no patch |
| 764 (1.20.2) | `pc/1.20.2` | `0x2a` | 763と同じfield順/幅 | displayNameもoption(string) |
| 765 (1.20.3,1.20.4) | 1.20.4→`pc/1.20.3` | `0x2a` | 同じfield順/幅 | displayNameはoption(anonymousNbt) |
| 766 (1.20.5,1.20.6) | 1.20.6→`pc/1.20.5` | `0x2c` | 同じfield順/幅 | option(anonymousNbt); columns=0 sentinel |

従って旧raw schemaのname codec境界は763–764 JSON string / 765–766 NBT component。decorationsはoptional bool + VarInt count。typeはschema上numeric VarInt、x/z signed byte、direction unsigned byte。patchはwidth(`columns`)=0 sentinel; nonzero時`rows:u8,x:u8,y:u8,VarInt length+colors`。`1.20.4`/`1.20.6`直接schema URLは404なのでdataPaths aliasを使用。Prismarineはfield契約の根拠であり、766の実codec/registry semanticsは以下のofficial bytecode調査で確定する。

### 767–774 official codec family / decoration type semantics

既存version JSONの`downloads.client_mappings.url`から公式mappingsをread-only取得し、既存jarへ `C:\Program Files\Java\jdk-21\bin\javap.exe -c -p -classpath <jar> <obfuscated-class>`を実行。利用可能な代表版のpacket `STREAM_CODEC` static initializerとdecoration/patch依存codecを比較した。protocol 767/768/769/770/771/773/774で5-field composite (MapId, byte scale, bool locked, optional decoration list, optional patch)の構成は同じ。`MapDecoration`は`Holder<MapDecorationType>` + x/y/rot bytes + optional Componentでenum ordinalではない。

| protocol | representative / protocol.json ID | mapped obfuscated classes: packet, decoration, type, patch, ByteBufCodecs | evidence |
|---:|---|---|---|
| 767 | 1.21.1 / `pc/1.21.1`, `0x2c` | `adm`, `eqn`, `eqo`, `eqt$b`, `yv` | codec + dependents inspected |
| 768 | 1.21.3 / `pc/1.21.3`, `0x2d` | `aek`, `evl`, `evm`, `evr$c`, `zr` | representative inspected; 1.21.2 not bytecode-compared |
| 769 | 1.21.4 / `pc/1.21.4`, `0x2d` | `ade`, `euo`, `eup`, `euu$c`, `yl` | inspected |
| 770 | 1.21.5 / `pc/1.21.5`, `0x2c` | `adx`, `ezp`, `ezq`, `ezv$c`, `zc` | inspected |
| 771 | 1.21.6 / `pc/1.21.6`, `0x2c` | `aei`, `fca`, `fcb`, `fcg$c`, `zk` | inspected |
| 772 | 1.21.7/1.21.8 / `pc/1.21.8`, `0x2c` | 1.21.8 jar/mappings fetched and verified in tempdir | `fcb -> zk$22`; direct registry ID |
| 773 | 1.21.10 / `pc/1.21.9`, `0x31` | `aeo`, `fhp`, `fhq`, `fhv$c`, `aac` | inspected; 1.21.9 unavailable |
| 774 | 1.21.11 / `pc/1.21.11`, `0x31` | `aex`, `fmq`, `fmr`, `fmw$c`, `aam` | inspected |
| 775 | 26.1.2 | named-class client jar; no mappings URL needed | direct registry ID; packet body matches native shape |

`MapDecorationType.STREAM_CODEC` in 1.21.1–1.21.11 does call `ByteBufCodecs.holderRegistry`, but its one-argument factory uses the ordinary registry-ID codec: decode is `VarInt.read -> holder IdMap.byIdOrThrow(id)` and encode is `getIdOrThrow -> VarInt.write`, with **no +/-1**. 1.21.1 callchain: `eqo::<clinit> -> yv.b(registryKey) -> yv.a(key, Registry::asHolderIdMap) -> yv$16.decode/encode`; 1.21.11: `fmr::<clinit> -> aam.b(key) -> aam.a(key, Registry::asHolderIdMap) -> aam$22.decode/encode`. In both, wire `0` is numeric registry ID 0 (player), not an Identifier and not inline data. The previously cited `yv$17` / `aam$24` are the separate two-argument `holder(key,directCodec)` codec; their inline marker semantics are not on MapDecorationType's callchain. `yv$18` / `aam$25` are holder-set codecs. The earlier `+1/wire-0 inline` conclusion is superseded and incorrect. Decorations still carry `Holder<MapDecorationType>` in the Java model; that type alone does not imply an offset.

### 26.2 native output and semantic mapping

- `minecraft-26.2-decompiled/src/net/minecraft/network/protocol/game/ClientboundMapItemDataPacket.java::STREAM_CODEC`: MapId, byte scale, bool locked, optional list, optional patch.
- `MapDecoration.java::STREAM_CODEC`: holder type, x/y/rot bytes, optional component; constructor masks rotation with `& 0xF`.
- `MapDecorationType.java::STREAM_CODEC` uses holderRegistry, but native `minecraft-26.2-decompiled/src/net/minecraft/network/codec/ByteBufCodecs.java::registry` calls `VarInt.write(registry.getIdOrThrow(value))` directly (no +1).
- `MapItemSavedData.java::MapPatch` absent emits width 0; present writes width,height,startX,startY bytes then VarInt-length color bytes.

Therefore protocol 766–774 builtin decoration type IDs are direct registry IDs, like native 776; no holder-reference offset/name branch is needed. Resolve the numeric source ID to its semantic key, then emit the native direct ID. Protocol 766 and 772 bytecode are confirmed below.

`data/pc/1.20.2/mapIcons.json` has 34 IDs 0–33 and documents the older semantic sequence: 0 player, 1 frame, 2 red_marker, 3 blue_marker, 4 target_x, 5 target_point, 6 player_off_map, 7 player_off_limits, 8 mansion, 9 monument, 10–25 banners white/orange/magenta/light_blue/yellow/lime/pink/gray/light_gray/cyan/purple/blue/brown/green/red/black, 26 red_x, 27–31 desert/plains/savanna/snowy/taiga village, 32 jungle_temple, 33 swamp_hut. Official `MapDecorationTypes` static registration bytecode adds `trial_chambers` as ID 34 already in 1.20.6 (766), and uses the same ordered 35 keys in 1.21.1–1.21.11 representatives and native 26.2. Thus native ID 34 is valid for **766–776**; protocols 763–765 remain the 34-entry/0–33 family. Do not infer absence in 766 from the older 1.20.2 `mapIcons.json`. Use the explicit 35-key semantic table for 766+; unknown numeric decoration ID omits only that icon while preserving map fields and valid patch. There is no wire-0 Identifier/direct-holder special case on this codec.

### raw fixture samples and implementation/drop boundary

Protocol 763 body samples (includes packet ID, excludes outer frame length/compression):
- no decorations/no patch `29 01 00 00 00 00` → mapId=1, scale=0, locked=false, icons=None, patch=None.
- named one decoration `29 01 00 00 01 01 01 10 20 03 01 0c 7b 22 74 65 78 74 22 3a 22 41 22 7d 00` → type=1/frame, x=16,z=32,rotation=3,name `{"text":"A"}`, patch=None.
- no decorations, 2x2 colors `[1,2,3,4]`: `29 01 00 00 00 02 02 00 00 04 01 02 03 04` → x=0,y=0,width=2,height=2, colors preserved.

These exact fixtures were parsed by inline Python following schema field order; final three assertions PASS (exit 0). Initial print helper used wrong nested JSON index (exit 1, corrected); first named fixture omitted list-count byte (exit 1, corrected hex above). No decoder/source fixture file added.

Implement at `pomme-client/src/net/translate.rs::Translation::translate_game_frame` via one bounded `decode_legacy_map_item_data(protocol,payload)`, not MapStore. 763–764 names are JSON component strings; 765–774 names are optional NBT-backed Components. For 766–774, parse decoration type as direct registry VarInt (IDs 0–34), then map semantic key to native direct ID; no Identifier-holder branch. Parse optional decoration-list bool/count strictly; patch width 0→None and present patch as width/height/x/y/VarInt-length colors. Preserve map ID separate from ItemStack MapId and retain patch when an unknown numeric icon is skipped. Do not use `RegistryTable::remap(ClientRegistry,id)` for absent MapDecorationType registry; explicit semantic map is smaller.

Do not remove `v764.drops.map_item_data` yet: remove only after 763 and 764 fixtures both pass translation→native typed decode and unknown icon test proves patch retained; keep `server_data` dropped. The 766+ codecs are now evidence-ready but have not been implemented; 775's packet body needs no codec rewrite, only a native pass-through regression fixture. Unknown numeric icon is skipped only when record boundary is safely parsed; malformed/truncated packet is rejected.

Suggested tests use `pomme-client/src/net/azalea_compat.rs::{translate_and_decode,translation_for}`: map_id/scale/locked, decorations None vs Some(empty), semantic type/name optionality, patch None and coordinates/colors, malformed/truncated; then existing `MapStore::apply` nonzero pixel fixture, ItemFrame stack MapId=17 quad candidate count exactly 1. Latest first-stage result has only two focused helper tests; full wire→handler→store MapId17→quad1 integration, non-frame negative integration, remove/ID reuse/dimension lifecycle tests are still missing.

### 調査検証

- Raw JSON fetch/parse used Python `urllib.request` and stdlib `json`: first nested print helper failed (exit 1), corrected query printed four exact schemas, packet IDs, aliases, mapIcons count/range (exit 0).
- `javap -c -p` on mapped packet, decoration, decoration type, patch, ByteBufCodecs classes for official 1.21.1/1.21.3/1.21.4/1.21.5/1.21.6/1.21.10/1.21.11 jars: exit 0 for mapped classes. Initial `javap` PATH lookup failed exit 127; used existing `C:\Program Files\Java\jdk-21\bin\javap.exe` (no installation).
- Previous pass: no Rust build/test, asset download, or user settings/cache writes. This follow-up fetched only official 1.20.6 and 1.21.8 client jars/mappings into `C:\Users\yuzum\AppData\Local\Temp\map-codec-766-772`; SHA1 and byte sizes matched official manifest (details in the final follow-up section). No assets/dependencies/cache/settings were fetched or changed. 766/772/775 codec questions are now resolved from real codec callchains below.

## 第二段763-765実装結果

- `pomme-client/src/net/translate.rs::Translation::translate_game_frame`にprotocol 763/764/765限定のMapItemData decoderを追加。packet IDはembedded `PacketTable`のnative `map_item_data` entryから解決し、固定IDは散在させない。旧packet IDからnative IDへremap後、既存`v764.drops`判定より先にdecoderへdispatchする。`map_item_data`のみdrop解除し、`server_data`は引き続きdrop。765の従来native-passも旧wire decode対象。
- 契約根拠は上記Prismarine schemaとfixture。763/764はoptional JSON stringを既存`json_to_nbt`でNBT component化、765はanonymous NBTをcopy。decorationsのNone/Some(empty)、scale/locked、signed VarInt map idのbit pattern、patch width=0と非zero patchを保持。native decorationは確認済みlegacy 0–33のsemantic対応だけ直接registry idへ出力し、unknown iconはrecord全体をconsumeして省略、patch/map fieldsを保つ。763–765以外のholder形式には適用しない。
- size上限は`azalea_protocol::read::MAXIMUM_UNCOMPRESSED_LENGTH`、countは残payloadで上限確認し、option bool・JSON・NBT・VarInt-length data・全payload消費を検証。巨大count/lengthの予約をせず、malformed/truncatedをreject。patch geometryの適用判定は既存`MapStore::apply`を利用。
- `pomme-client/src/net/handler.rs::map_item_data_event`をpacket→既存`NetworkEvent::MapItemData`変換の共有点とし、本番handlerとfixture双方で使用。`pomme-client/src/net/azalea_compat.rs` fixtureは763 named JSON/map 17/frame, 764 Some(empty)+no patch, 765 named NBT+2x2 colors, unknown decoration+valid patch, decorations None, signed map id, malformed count/length/JSON/truncated prefixes, native 776 unchanged packet decode、763/764 `server_data` dropを確認する。765 fixtureは実handler event conversionを通して`MapStore::apply`し、pixels `[1,2,3,4]`が2x2位置に反映されることをassert。
- focused `mise exec -- cargo test -p pomme-client --locked legacy_map_item_data_763_765_translates_through_native_decode_and_map_store` はdecoder確認時にexit 0 / 1 passed。後からhandler共通event helperと追加fixtureを入れた最終状態では、`net::azalea_compat::`及び`mise run test`がexit 101で、他作業のdirty `app/core.rs` / `app/phases/in_game.rs` test fixtureに合計6 compile errors (private `LookDirection`, `Registry::to_u32`未import2箇所、`EntityStore::remove`未定義2箇所、`MapId::new`未定義)が出て実行前に停止。未検証を成功扱いしない。最終コードのproduction `mise run check`はexit 0。
- `git diff --check`はexit 0。全repo fmtは実行していない。`rustfmt --check`の最初の確認はexit 1: 本件追加部分に加えdirty handler内の既存particle作業差分にも整形差分があったため、handler全体へfmtをかけず本件行のみ整形。quad統合/候補数1は別frame→quad担当範囲のため追加しない。766以降は全て変更していない。

## 第二段追補（2026-10 follow-up）：766/772実codec・registry ID再確認

前の調査メモが`ByteBufCodecs.holder`の別helperをMapDecorationTypeの実codecと誤認し、767/774を`+1`、wire `0`をinlineとしたため訂正する。**MapDecorationTypeは766–776すべて直接registry ID**。旧誤記はこの追補より前の記録として扱わず、上記の修正済み説明を正とする。

### Official artifact取得・照合

- Official version manifest `https://piston-meta.mojang.com/mc/game/version_manifest_v2.json` → version metadata `1.20.6` / `1.21.8`。そのclient jarとclient_mappingsのみを取得。assets/index/dependenciesは取得していない。
- 一時dir: `C:\Users\yuzum\AppData\Local\Temp\map-codec-766-772`
- `1.20.6-client.jar`: 26,565,641 bytes, SHA1 `05b6f1c6b46a29d6ea82b4e0d42190e42402030f`（manifest一致）。`1.20.6-client_mappings.txt`: 9,422,442 bytes, SHA1 `de46c8f33d7826eb83e8ef0e9f80dc1f08cb9498`（一致）。
- `1.21.8-client.jar`: 29,525,242 bytes, SHA1 `a19d9badbea944a4369fd0059e53bf7286597576`（一致）。`1.21.8-client_mappings.txt`: 10,989,738 bytes, SHA1 `bdeb624c3aefba11d9d40f34bc96176350b549b6`（一致）。

### Factoryから実匿名codecまで（`javap -c -p`, 全対象終了コード0）

- 766 / 1.20.6 mappings: packet `aea`, decoration `epg`, type `eph`, patch `epm$b`, codecs `zl`。`eph::<clinit>` calls `zl.b(registryKey)` (= `holderRegistry`); factory delegates to `zl.a(key, Registry::asHolderIdMap)`, which constructs `zl$16`. `zl$16.decode`: `VarInt.read`をそのままIdMap `byIdOrThrow`へ、encode: `getIdOrThrow`をそのまま`VarInt.write`へ。offsetなし。2引数inline helper `zl$17`は別factory。
- 772 / 1.21.8 mappings: packet `aei`, decoration `fca`, type `fcb`, patch `fcg$c`, codecs `zk`。`fcb::<clinit> -> zk.b(key) -> zk.a(key, Registry::asHolderIdMap) -> zk$22.decode/encode`も同じdirect-ID処理。`zk$24`が2引数inline helperでMapDecorationType callchainではない。
- 代表767 (`eqo -> yv.b -> yv$16`) / 774 (`fmr -> aam.b -> aam$22`)を同じ方法で再確認。1.21.1の`yv$16` / 1.21.11の`aam$22`もdirect VarInt ID。`yv$17` / `aam$24`がinline `holder(key,directCodec)`、`yv$18` / `aam$25`がholderSet codec。以前の「wire 0 Identifier参照」も誤り。**このMapDecorationType codecではwire 0=registry ID 0=player**。

### Field / patch / component確認

- 766 `aea::<clinit>`と772 `aei::<clinit>`はMapId, scale byte, locked bool, optional list decorations, optional MapPatchの5-field composite。listはoptional bool + VarInt count。decorationはtype, x, y, rotationの各codecとoptional Component。patch codec自体がwidth byte sentinelを扱い、optional boolは追加しない。
- 766 `epg` / 772 `fca`のname codecはComponent optional stream codec。`xr` / `xq`のstatic codec factoryは`ByteBufCodecs.fromCodecWithRegistries`→NBT `tagCodec` (`NbtOps.INSTANCE`)を使う。Noneはoptional flag false、SomeはNBT tag。JSON name stringは763–764のみ、NBT Componentは765以降。
- 766 `epm$b` / 772 `fcg$c`の実decodeはunsigned widthを読み、`width==0`ならNone、正値ならheight, x, y, VarInt-length byte array。encodeもNone時はwidth `0`、present時width,height,x,y,data。確認したraw orderは`width,height,x,y,data`。

### 766–774 protocol boundary / semantic IDs

| Protocol | Representative bytecode checked | type contract | built-in source IDs |
|---:|---|---|---|
| 766 | 1.20.6 (`eph -> zl$16`) | direct registry VarInt; wire 0=player | 0–34; ID34 trial_chambers |
| 767 | 1.21.1 (`eqo -> yv$16`) | direct registry VarInt | 0–34; ID34 trial_chambers |
| 768 | 1.21.3; same family | direct registry VarInt | same 35-key order |
| 769 | 1.21.4; same family | direct registry VarInt | same 35-key order |
| 770 | 1.21.5; same family | direct registry VarInt | same 35-key order |
| 771 | 1.21.6; same family | direct registry VarInt | same 35-key order |
| 772 | 1.21.8 (`fcb -> zk$22`) | direct registry VarInt; wire 0=player | 0–34; ID34 trial_chambers |
| 773 | 1.21.10; same family | direct registry VarInt | same 35-key order |
| 774 | 1.21.11 (`fmr -> aam$22`) | direct registry VarInt | same 35-key order |

Official bootstrap static initializer key order was checked on 1.20.6, 1.21.1/.2/.3/.4/.5/.6/.8/.10/.11; each has 35 keys and identical key-sequence SHA1 `1a5adb41ed9cd117b1e961a2c711485920618366`. ID semantics follow registration order: 0 player, 1 frame, 2 red_marker, 3 blue_marker, 4 target_x, 5 target_point, 6 player_off_map, 7 player_off_limits, 8 mansion, 9 monument, 10–25 banners (white, orange, magenta, light_blue, yellow, lime, pink, gray, light_gray, cyan, purple, blue, brown, green, red, black), 26 red_x, 27–31 village_desert/plains/savanna/snowy/taiga, 32 jungle_temple, 33 swamp_hut, 34 trial_chambers. Therefore 766–774 all support native semantic ID34. Prismarine `1.20.2/mapIcons.json`'s 34 IDs is an older-version list, not evidence against 1.20.6's ID34.

### 775 native pass-through conclusion / minimal fixture

26.1.2 named classes and 26.2 decompiled sources both use direct registry IDs, optional NBT Component, optional decoration list and MapPatch width-zero sentinel. Their MapItemData **body** is compatible; do not rewrite 775 decoration IDs or body. Keep ordinary packet-ID mapping/dispatch separate and add a 775 native-decode regression fixture.

Fixture body (packet ID and outer frame length excluded), suitable for 766, 772, 775 and native 776:

```text
01 00 00 01 01 22 00 00 00 00 00
```

Expected: `map_id=1, scale=0, locked=false, decorations=Some([trial_chambers(id 34), x=0,y=0,rot=0,name=None]), patch=None`. Byte `22` is VarInt 34, not `34+1`; the same body should decode in native 776. This fixture distinguishes the older 763–765 0–33 table and proves the 775 no-rewrite boundary without implementing a decoder here.

確認command: Python `urllib`でmanifest/metadata取得、指定4成果物のsize/SHA1検証は終了コード0。Pythonから `C:\Program Files\Java\jdk-21\bin\javap.exe -c -p -classpath <jar> <class>`を実行し、766/772 packet/type/decor/patch/codec helperおよびexisting 767/774/775/native classesを確認、各class `javap`終了コード0。Rust build/test、plan以外のファイル編集、asset/dependency download、user cache/settings変更はなし。

## 第二段 follow-up：NBT list / JSON MUTF-8 bounds と native 未対応 decoration

- `skip_nbt_payload`共用部はListの負countを拒否し、End element typeはcount 0のみ許可する。element type 0–12以外を拒否し、正countは残りpayload byte数以下に制限（全tag payloadが最低1 byte進むため総反復数は入力byte数以下）。各反復でcursor進捗も確認し、既存depth capは維持。compound/list/array等の全callerに適用される共用境界であり、入力全体サイズ上限のみには依存しない。
- 763/764 JSON nameは既存`json_to_nbt`を保ち、変換前に再帰的にすべてのJSON文字列値とobject keyをJava modified UTF-8 byte長で検査。NUL=2 byte、ASCII=1、U+00E9 `é`=2、U+8A9E `語`=3、補助面 `𝄞`=6としてu16上限65535超を拒否。境界testはそれぞれ32767/32768、21845/21846、10922/10923文字。serde JSON parse depth制約の下で安全な再帰チェックを使い、別frameworkは導入しない。
- Source semantic evidence scope: protocol 763 known map icon IDs 0–26、764/765 known IDs 0–33。vendored Azalea `c_map_item_data::DecorationType` enumは0–26のみで、native decode後のfallbackはPlayer。今回はnative vendor/enum拡張をしない。source known 27–33はdecor recordだけconsumeして省略し、patch/map本体は維持。763のsource範囲外および各protocolのsource範囲外もaliasしない。native trial_chambers ID34は公式766以降であり本source mappingに含めない（766/公式34以降の変更は別scope）。
- Regression coverage: `translate.rs` unit testsで巨大End-list count、negative/invalid/truncated list、通常byte/compound list、深さ境界、およびASCII/NUL/BMP/補助面のMUTF byte境界・nested keysを確認。`azalea_compat.rs`はfull source frameから巨大NBT list/JSON nameを拒否する試験、ASCII MUTF 65535はnative nameを全文 decodeしcursor全消費、65536は拒否、protocolごとの全source-known icon IDをnative decode→event assetまで検証する。
- focused実行履歴: `mise exec -- cargo test -p pomme-client --locked nbt_lists_reject_unbounded_or_malformed_counts_and_preserve_valid_lists` は1 passed/exit 0。最初の深さ境界fixtureはnested生成のcountを誤って作りexit 101になったが、MAX_DEPTHそのものを512 accepted / 513 rejectedでassertする形に修正しpass。`... json_to_nbt_string_validation_counts_modified_utf8_for_nested_values_and_keys` はBMPをU+8A9E `語`へ置き換えた状態で1 passed/exit 0。追加したU+00E9 32767/32768 assertionsとfull-frame malformed-list assertionsを含む最新版はまだ再実行できていない。
- `mise run test` はexit 101でtest execution前compile fail。`azalea_compat.rs`の別frame fixture (Vec3型不一致、`Direction::Clientbound` 2箇所)に3 errors。より後のfocused compile試行では同ファイル別framefixtureに加え、同時作業中のrenderer/end_portal/context/core差分のcompile errorsが出た（最新ログでは9 errors）。このscope外差分は編集せず保持。`mise run check`もexit 101: concurrent renderer/end_portal/context差分に6 compile errors。未確認を成功扱いしない。
- `git diff --check -- pomme-client/src/net/translate.rs pomme-client/src/net/azalea_compat.rs docs/plans/legacy-map-frame-compatibility.md` exit 0。repo全体`git diff --check`は並行編集された`pomme-client/src/renderer/context.rs:86` trailing whitespaceによりexit 2。全repo fmtは実行していない。

## 766–775 passthrough 回帰fixture / native typed enum deficiency (2026-10 follow-up)

- 独立再確認では、766–775のTranslationはMapItemData payloadをdecoder/re-encoderせずnative packet IDへmappingしbodyをpass throughする。追加test `map_item_data_766_775_passthrough_exposes_native_unknown_decoration_fallback` は各protocolのsource IDを`PacketTable`から構築し、translation後native packet IDとbody bytes一致をassert、native typed decode→production `map_item_data_event`→`MapStore::apply`まで通す。runtime decoder変更は0件。
- 先行planの「共通bodyの期待type34=trial_chambers」は**wire semanticの期待**であり、現Azalea typed resultとして成立しない。fixed Azalea checkout `ffedf17`, `azalea-protocol/src/packets/game/c_map_item_data.rs::DecorationType` は0–26の27 variantのみ。derive enum codecで27–34がunknown fallbackになることを踏まえ、testはbody上のID34 `0x22`を保持しつつdecoded valueとproduction event/storeのassetが`Player`へ誤aliasされる現在の欠陥を明示的にassertする。別のnative controlとしてwire ID0もPlayerになることを分けて確認する。これは誤aliasの再現fixtureであり、ID34がPlayerで正しい/互換性完了という意味ではない。意図的failureやignored testにはせず、通常suiteを壊さない既知deficiency testとした。
- fixtureには別々にdecorations=None / Some(empty)、ID0 playerとID34 raw decoration、optional NBT name有無、width=0 no patch、2x2 nonzero patchを含める。776 native raw fixtureも同じID34 bodyでtyped fallbackを確認する。763–765 decoderやAzalea enum/vendorはこの追補で変更しない。ID34を正しく表現するnative vendor/registry拡張は別計画で扱う。
- fresh bytecode確認: JDK 21 `javap -c -p -s`。767/1.21.1の`eqo::<clinit>`は`yv.b:(Lakq;)Lyx;`を呼び、`yv.b(key)`→`yv.a:(Lakq;Ljava/util/function/Function;)Lyx;`が`yv$16`を生成する。774/1.21.11の`fmr::<clinit>`は`aam.b:(Lamt;)Laao;`、`aam.b(key)`→`aam.a:(Lamt;Ljava/util/function/Function;)Laao;`が`aam$22`を生成する。実匿名codecは`VarInt.read`値を`IdMap.byIdOrThrow`へ、`getIdOrThrow`値を`VarInt.write`へ渡し、加減算なし。overload `(registryKey, directCodec)`はそれぞれ別匿名codec (`yv$17` / `aam$24`)を生成し、このcallchainではない。各javap exit 0。したがってregistry wire契約の766–775 direct ID結論は維持するが、Azalea typed enumのsemantic coverageは別問題。
- 確認: `mise exec -- cargo test -p pomme-client --no-run --locked` exit 101; `mise exec -- cargo test -p pomme-client --locked map_item_data_766_775_passthrough_exposes_native_unknown_decoration_fallback -- --nocapture` exit 101; `mise run check` exit 0; `mise run test`を一度実行しexit 101。test build blockersは既存dirty差分で、現test出力は`azalea_compat.rs:765,791`の`Cursor<&Box<[u8]>>`/`Cursor<&[u8]>`不一致、no-runは別時点の`azalea_compat.rs:1129` metadata fixtureの`Option<i32>`比較、`renderer/mod.rs:2502` missing `reload_skin`、`renderer/pipelines/hand.rs:561` moved `Allocation`を報告。focused run時点はhand.rs moved `Allocation`で停止。Rust test binary未起動なのでfixture件数・実行結果は未確認。checkはproduction targetの型検査成功のみ。
- 触ったコードはtest-only追加 `pomme-client/src/net/azalea_compat.rs` 1 test。plan追記以外に既存差分を修正せず、全repo fmt・commitは行わない。最終diff/statusと`git diff --check`は引き継ぎ前に確認。
