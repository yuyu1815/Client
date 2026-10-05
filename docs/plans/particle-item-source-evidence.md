# Java 26.2 item interaction / item-pickup particle evidence

参照ソース: `minecraft-26.2-decompiled/src/`。Java source inventory の item interaction 行は `Client/docs/plans/particle-java-callsite-inventory-26.2.csv` の `BrushItem.java` と `BucketItem.java`（各 `Level.addParticle`）です。server `sendParticles` は packet 処理に任せ、ここでは重ねて生成しません。

## Item interaction

| Java source / condition | Rust path | Rust behavior |
|---|---|---|
| `net/minecraft/world/item/BrushItem.java`: `useOn` は block hit 時 `startUsingItem`; `onUseTick` は elapsed が `10n+5` のときだけ処理。block は `shouldSpawnTerrainParticles()` かつ render shape が invisible でない場合。7..11 個の `BlockParticleOption(BLOCK, state)`、hit face / view vector / brushing arm による速度。 | `player/interaction.rs::InteractionState::start_use_item`, `tick_brush_use`, `spawn_interaction_block_particle` | block 使用時に brush tick 状態を開始し、押下中の毎 tick に elapsed を進める。5, 15, 25... tick で `BLOCK` + typed `ServerParticleOptions::Block(state)` を7..11個生成。Java の terrain-particle / invisible-shape 条件と face ごとの delta、hit point epsilon を反映。サーバーから別途受信する粒子ではないため client input 経路のみ。 |
| `net/minecraft/world/item/BucketItem.java::emptyContents`: `WATER_EVAPORATES` が true かつ content が water の場合に `LARGE_SMOKE` を8個、destination block 内に乱数位置、速度0。 | `player/interaction.rs::InteractionState::start_use_item`, `set_water_evaporates`; `app/core.rs` interaction tick | `WATER_EVAPORATES` の実効値と Java `BucketItem.use` が選んだ destination を使い、typed `LargeSmoke` + `Simple` option を8個生成。通常 bucket や属性 off では生成しない。client interaction で直接生成する Java 経路に対応し、LevelEvent/packet由来を重複生成しない。Brewing Stand など `useWithoutItem` が成功するブロックは、同じ入力境界の block-use consumption 判定で water bucket への fallthrough を止める。sneak + 手持ち item で block-use を抑制した場合は Java 通り bucket 経路へ進む。 |

`WATER_EVAPORATES` は接続先 dimension/environment attributes から解決して `InteractionState` に渡される。既知の Nether 名だけで判定する旧経路ではない。

## Take-item animation

| Java source / condition | Rust path | Rust behavior |
|---|---|---|
| `net/minecraft/client/multiplayer/ClientPacketListener.java::handleTakeItemEntity` (lines 962–1010): missing collector は local player に fallback。`from != null` のとき sound、`extractEntity(from, 1.0F)` で stack/model state を shrink 前に capture、`ItemPickupParticle` を作成。ItemEntity は amount を shrink し empty のみ remove。 | `net/handler.rs` `ClientboundGamePacket::TakeItemEntity` → `app/core.rs::NetworkEvent::ItemPickedUp` → `entity/mod.rs::ItemEntityStore::pickup` | packet IDs/amount は維持。item が存在して nonempty stack のとき pre-shrink item name/stack/id/damage/count/age/bob と source position を pickup animation に保存。amount を stack count から引き、0以下のみ entity を除去。collector 不在時 local player ID/position を使う。arrow fallback sound は既存仕様。 |
| `client/particle/ItemPickupParticle.java` + `ItemPickupParticleGroup.java`: 3 tick lifetime; each tick target x/y/z is refreshed (`y=(getY()+getEyeY())/2`) with old/current samples. Render uses `t=((life+partial)/3)^2`, target interpolation then item-state interpolation, and submits the captured `EntityRenderState` through `EntityRenderDispatcher`. | `entity/mod.rs::ItemEntityStore::active_pickups`; `app/phases/in_game.rs::build_item_render_infos` | 3 tick lifetime、二乗補間、collector current/previous position を render partial tick で補間。既存 dropped-item geometry/mesh/atlas 経路 (`emit_item_copies`) に接続し、quad/icon 代用ではない。light は Java の抽出済み item state に合わせ packet時 source position (`light_position`) で固定。local player は現在の eye height の半分を target Y offset に使用。 |

遠隔 LivingEntity の eye height は Rust entity state に保存されていないため、現在は target y offset を `0.81` としている。local player の crouch 等は eye-height を反映するが、遠隔 collector の pose/baby 固有 eye height は未対応。Java `handleTakeItemEntity` は ExperienceOrb とその他の非 ItemEntity も処理するが、この変更のモデル flight request は実アイテム (`ItemEntityStore`) のみ。XP orb / 矢などの非-item source は現在の separate entity render path に take request を接続しておらず、完全 parity 外である。ElderGuardian particle model route はこの item model animation には適用せず、既存 item renderer が適切な所有境界。

## Automated checks

- `entity::tests::item_take_keeps_pre_shrink_model_follows_collector_and_expires_after_three_ticks`: pre-shrink quantity / remaining stack, moving target, captured source light, three-tick expiry, last removal timing.
- `player::interaction::tests::item_interaction_particles_use_native_options_and_brush_cadence`: typed `BLOCK` + block state / `LARGE_SMOKE` + simple option and 5,15,25 tick cadence.
- `player::interaction::tests::water_bucket_evaporation_is_a_client_local_eight_large_smoke_burst`: dimension + item-kind gate and 8-particle count.

GUI目視は実施していない。focused test 実行時、並行 particle world-tick 変更のコンパイルエラーにより test harness 全体のビルドが停止した場合は、テスト結果ではなく阻害要因として報告する。
