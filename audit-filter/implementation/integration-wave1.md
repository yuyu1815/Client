# 第1波統合レポート

## Findings

- 統合後 `cargo check -p pomme-client` は成功（exit 0）。`winit v0.30.13` の将来非互換警告が1件だけ残る。
- 実テスト `cargo test -p pomme-client --quiet` は **661 passed / 0 failed / 0 ignored**（exit 0）。
- `cargo fmt --check` は失敗（exit 1、101箇所の差分）。既存の作業ツリー全体に及ぶフォーマット差分が含まれ、他agentの変更や未依頼ファイルを巻き込まないため全体整形はしていない。今回追加した `net/azalea_compat.rs`、`net/handler.rs`、`net/mod.rs`、`net/sender.rs`、`net/connection.rs`、`world/chunk.rs`、`physics/block_shape.rs` は個別 `rustfmt --check` 成功。`app/core.rs`、`app/phases/in_game.rs` 等には既存差分のフォーマット指摘が残る。
- `git diff --check` は成功。コミット、reset、既存未追跡ファイルの変更・削除なし。

## Relevant Files

### 今回の統合修正

- `pomme-client/src/net/mod.rs`
  - `NetworkEvent::ChunkBiomes { pos, data }` を追加。
- `pomme-client/src/net/handler.rs`
  - `ClientboundGamePacket::ChunksBiomes` の全chunk payloadを `ChunkBiomes` eventへ接続。
  - ping/pong、cookie保存・照会の処理とテストを保持。既存のhandlerテストに複数chunk biome eventの契約検査も追加。
- `pomme-client/src/app/core.rs`
  - `ChunkBiomes` を `ChunkStore::replace_biomes` に渡し、成功時は該当チャンク周辺meshをdirty化。未ロードchunkは無視、破損payloadは警告して破棄。
  - 既存wave実装のローカルプレイヤーmotion、block entity更新、CoC/transfer event consumerを保持。
- `pomme-client/src/world/chunk.rs`
  - biome差し替えを未ロードchunk判定後に行う。未ロードchunkではpayloadが完全でなくても `Ok(false)` を返し、loaded chunkでは全sectionを検証してから反映。
- `pomme-client/src/net/azalea_compat.rs`
  - protocol 764 `set_score` をnative codecに翻訳した際、追加されたdisplay/numberFormat optionalがAbsentとしてdecodeされる回帰テスト。
- `pomme-client/src/net/connection.rs`, `pomme-client/src/net/sender.rs`
  - CoC受諾UI/フローがないのに到達不能だった `AcceptCodeOfConduct` outbound APIを除去。CoCはeventをアプリへ通知した後、未対応理由を返して即時切断（サーバー応答待ちのhangを回避）。config/game Pingは対応Pongを即送信し、CookieRequest/StoreCookieはconfigとgame phase間で同じcookie mapを使用。
- `pomme-client/src/physics/block_shape.rs`
  - bed collision regression assertionを実装shape（各part 3 box）に合わせた。既存の新規shape testsを維持。

### 保護した他の作業ツリー変更

`pomme-client/src/app/phases/in_game.rs`, `pomme-client/src/physics/collision.rs`, `pomme-client/src/physics/movement.rs`, `pomme-client/src/player/interaction.rs`, `pomme-client/src/player/mod.rs`, `pomme-client/src/renderer/chunk/mesher.rs`, `pomme-client/src/renderer/pipelines/block_entity.rs`, `pomme-client/src/ui/player_tab.rs`, `pomme-client/src/world/block_entity.rs` は差分を保持し、巻き戻し・一括整形していない。

## Migration / Investigation Notes

### net → app → world / ui / player

- Biome climate: `connection::extract_biome_climate` → `NetworkEvent::BiomeColors` → `AppCore` の `game.biome_climate` と `mesh_dispatcher.set_biome_climate`。再configuration時のregistry更新経路も既存接続を確認。
- Chunk biome palettes: Azalea `ClientboundChunksBiomes.chunk_biome_data` → `handler::ChunksBiomes` → `NetworkEvent::ChunkBiomes` → `AppCore` → `ChunkStore::replace_biomes` → mesh-neighborhood invalidation。未ロード/不正入力で部分更新しない。
- Tick freeze: `handler` が `TickingState` / `TickingStep` のserver packetをevent化し、`AppCore`がtick rate/frozen/step budgetを更新。`app/phases/in_game.rs` の既存frozen cadence testsも全件通過。
- Local motion: `EntityMotion` は全entity storeへ反映しつつ、entity idがローカルplayer idと一致する時だけ `game.player.velocity` を更新。ユニットテストで一致/不一致を確認。
- Score optional: `translate_set_score_764` がdisplay/numberFormatをwire上で `None, None` として補い、native `SetScore` decoderと既存app scoreboard consumerに届く。新テストが両optionalの `None` を確認。
- CoC: user-consent UIが未実装なので肯定応答は送らない。NetworkEventで本文をアプリへ届け、即座に「未対応」disconnectにする。未接続のaccept methodを残して成功を装うことは避けた。
- Server transfer: configuration/game双方でeventを通知して接続を終了するところまで。新hostへの自動再接続は未実装で、現状はアプリがtransfer先を表示できるだけ。
- Ping/Pongとcookies: configuration Ping→config Pong、game Ping→game Pong。StoreCookieはセッションmapへ保存、CookieRequestは該当keyのpayload（未保存ならNone）を応答。mapはconfiguration→game→mid-session reconfigurationを通して維持。

### Mojang/Vulkan参照

- 公式client source jar: `C:/Users/yuzum/Desktop/mine_rust/fabric-render-probe/.gradle/loom-cache/minecraftMaven/net/minecraft/minecraft-clientOnly-201bed67e3/26.2/minecraft-clientOnly-201bed67e3-26.2-sources.jar`（2,561,906 bytes）。`net/minecraft/client/multiplayer/ClientPacketListener.java` を確認し、`handleTickingState` / `handleTickingStep` はTickRateManagerへ状態反映、`handleSetScore` はscoreとdisplay/numberFormat optionalをobjectiveへ反映、`handlePongResponse` はping monitorへ渡すことを確認。
- 近傍の `minecraft-common-201bed67e3-26.2.jar` は存在するが、同版common source jarはcacheに見つからず。scoreの旧protocol翻訳契約はrepo内のAzalea 26.2 packet codecと `translate_set_score_764` を照合し、optional decode回帰テストで検証。
- Vulkan SDKの開発用 `vulkan-1.lib` は探索した `C:/Program Files`、`C:/Program Files (x86)`、`C:/VulkanSDK`、MinGW/Cargo範囲に見つからなかった。最初の `cargo test --no-run` は `vkGetInstanceProcAddr` 未解決でlink失敗（exit 101）。
- Windows実体 `C:/Windows/System32/vulkan-1.dll` は存在し、`objdump -p` でexport `[201] vkGetInstanceProcAddr` を確認。`vulkaninfo.exe --summary` はexit 0、Vulkan instance 1.4.341、AMD Radeon RX 7900 XTX等2 GPUを認識。
- fake symbol implementationは作らず、この実loader DLLを指すCOFF import libraryだけを `/tmp/pomme-vulkan-import/vulkan-1.lib` に生成（MinGW `dlltool`, `LIBRARY vulkan-1.dll`, export `vkGetInstanceProcAddr`）。`RUSTFLAGS="-C link-arg=C:/Users/yuzum/AppData/Local/Temp/pomme-vulkan-import/vulkan-1.lib"` でテストexeをlinkし、661件を実行・成功。Import libraryはrepo外。
- 誤った試行も記録: `cargo test -p pomme-client --lib --no-run` はこのpackageがlibrary targetでないためexit 101。正しいtargetはbin `src/main.rs` であり、`cargo test -p pomme-client` を使用した。

### fmt / 作業ツリー

- `cargo fmt --check` は全workspaceのformat差分を報告。確認された対象には `app/probe.rs`, `renderer/pipelines/item_entity.rs`, `world/block/model.rs` など本件対象外ファイルと、他agentが変更中のファイルが含まれる。ユーザー指示どおり無関係な差分拡大を避け、format全体適用はしなかった。
- git status時点の既存未追跡は `diagnostic/__pycache__/*.pyc` と `tmp/drop-*-final-tests.*`。それらはそのまま保護。統合レポート `audit-filter/implementation/integration-wave1.md` のみ今回新規作成。

## Recommended Next Steps

1. CoCを受諾可能にする場合は、利用者が本文を確認・承諾できるUIを追加し、承諾actionをconfig phaseへ接続する。現在は安全に拒否する。
2. Server transfer先へ自動再接続する場合は、接続screen/navigation側でhost/portを引き継ぐフローを追加する。現在は通知して切断する。
3. 101件の全体fmt差分は他agentの作業確定後に、差分所有者と合意して整理する。今回の作業ではformat変更を一括適用しない。
