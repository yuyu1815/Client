# End Portal / End Gateway 描画 実装計画

## 確定している欠落・適用範囲

`world/block_entity.rs::rendered_kind` は End Portal/Gateway を描画対象にせず、`is_block_entity_block` は両者をBEブロックとして扱う。よって `renderer/chunk/mesher.rs` は通常モデルのないブロック面を出さず、`renderer/pipelines/block_entity.rs` にも専用描画がない（調査根拠: `rendered_kind` 643–687、mesher 2112–2116、BE pipeline 551–625/1534–1537）。スクリーンショット `Pomme screenshots2026-10-04_20.21.37` の暗い面と Modrinth `2026-10-04_20.19.57` の星空を区別し、暗色planeの代用は禁止。

今回の範囲は両BEの「面」描画と End Gateway の条件付き beam。Gateway beam は面の別要素として明記し、NBT等から稼働状態を確実に復元できない場合は安全に非表示（誤った常時beamを出さない）とする。ゲートウェイ煙 `animateTick` は独立した小項目で含める。テレポート/サーバー挙動、ポータル接触時画面効果は対象外。

## 公式動作の対応

参照した decompiled 26.2: `AbstractEndPortalRenderer` は block entity が許可する全方向の単位cube面を提出し、Sampler0=`textures/environment/end_sky.png` と Sampler1=`textures/entity/end_portal/end_portal.png` を使う。`TheEndPortalRenderer` は unit cube に translation y=0.375、scale y=0.375 を適用するため面は y=0.375..0.75、x/z=0..1。表示面は上下2面のみ。`TheEndGatewayRenderer` は近隣ブロックによる face occlusion で表示面を決める。面には同じ2 textureを使うが RenderPipelines の `PORTAL_LAYERS` は Portal=15、Gateway=16。

### 公式 26.2 shader / pipeline 根拠（今回補完）

実環境の公式資産は存在する。読み取り元は `C:/Users/yuzum/AppData/Roaming/.pomme/data/versions/26.2/26.2.jar`（同版 `.minecraft/versions/26.2/26.2.jar` も存在）。zipfileでjar内を直接読み、ファイル抽出・書換はしていない。`assets/minecraft/shaders/core/rendertype_end_portal.vsh` / `.fsh`、`assets/minecraft/shaders/include/{projection,fog,globals,matrix}.glsl` が公式根拠。主な公式shader抜粋:

```glsl
// rendertype_end_portal.vsh: 投影後clip positionを射影テクスチャ座標にする
 gl_Position = ProjMat * ModelViewMat * vec4(Position, 1.0);
 texProj0 = projection_from_position(gl_Position);

// projection.glsl
vec4 projection = position * 0.5;
projection.xy = vec2(projection.x + projection.w, projection.y + projection.w);
projection.zw = position.zw;

// rendertype_end_portal.fsh: 1-based layer L
translate.x = 17.0 / L;
translate.y = (2.0 + L / 1.5) * (GameTime * 1.5);
rotate = mat2_rotate_z(radians((L * L * 4321.0 + L * 9.0) * 2.0));
scale = mat2((4.5 - L / 4.0) * 2.0);
layerMatrix = mat4(scale * rotate) * translate * SCALE_TRANSLATE;
color = textureProj(Sampler0, texProj0).rgb * COLORS[0];
for (int i = 0; i < PORTAL_LAYERS; ++i)
    color += textureProj(Sampler1, texProj0 * end_portal_layer(float(i + 1))).rgb * COLORS[i];
```

移植時の要件: `Sampler0=environment/end_sky.png`、`Sampler1=entity/end_portal/end_portal.png`。PositionからUVを作るのではなく `gl_Position` を用いる projective texcoord（テクスチャ参照時のw除算）であり、`SCALE_TRANSLATE` はXYを0.5倍して0.25移動（textureの中央領域）する。2 samplerのRGBをアルファ合成せず16色定数配列で加算、最終alpha=1。不変の15色＋背景の配列値は公式 `.fsh` 全文が一次根拠（抜粋では省略）。shaderの`PORTAL_LAYERS`はPortal=15、Gateway=16。背景も `COLORS[0]`、レイヤーも `COLORS[i]` なのでPortalでは index 0..14、Gatewayのみ index 15まで使う。各層の scale/rotation/translation 式は上記を同じ式で実装し、推測近似はしない。

`Globals.GameTime` は生tickではなく公式 `GlobalSettingsUniform.java` が `((gameTime % 24000L) + partialTick) / 24000.0F` として渡す 0..1 の一日周期値（shader内でさらに `*1.5`）。現在のclient `SkyState.game_time` はtickなので、同等の `rem_euclid(24000)` /24000 + partialを専用uniformへ渡す。`fog.glsl` は spherical=`length(Position)`、cylindrical=`max(length(Position.xz),abs(Position.y))`、両fog値のmaxで環境色へRGB mixしalphaは維持する。現行`camera_ubo.glsl`は単一のCameraUniformにview_proj/camera/fog等を持つだけで vanillaのProjMat/ModelViewMat/Globals/Fog bind groupsとは異なる。client側の既存view-proj、camera-relative Position、`fog.glsl` の`total_fog_value`/`apply_fog`を再利用し、projective座標を作る時点のclip-spaceとuniform行列規約を合わせること。GameTime用laneは現行UBOにないため、既存UBOの破壊的拡張より専用push constant/uniformか既存world time供給先の再利用を選ぶ。fog入力は既存camera fog値を使い、距離の基準がPosition/camera-relative座標と一致することを確認する。

公式 `RenderPipelines.java` の `END_PORTAL_SNIPPET` は `GLOBALS`, `MATRICES_PROJECTION`, `FOG`, `SAMPLER0_SAMPLER1` layouts、POSITION vertex、QUADS、`DepthStencilState.DEFAULT`。END_PORTAL / END_GATEWAY はそれぞれ `PORTAL_LAYERS=15/16`。snippetにcolor blend指定がなく既定のcolor state。クライアント独自Vulkan passでは、現在のpipeline既定blend/depth/cullを確認してopaque出力・depth test/writeの挙動を決める（vanillaと同じく独自に透過blendを足さない）。公式shader資産に対応する通常pipeline JSONはjar内には見当たらず、上記decompiled Javaをpipeline state根拠として併用。

同jar内のtextureも確認済み: `assets/minecraft/textures/environment/end_sky.png`, `assets/minecraft/textures/entity/end_portal/end_portal.png`, Gateway beam用 `assets/minecraft/textures/entity/end_portal/end_gateway_beam.png`。

Gateway状態について、公式 `TheEndGatewayBlockEntity.saveAdditional` / `getUpdateTag` は `Age` と `exit_portal`、必要時`ExactTeleport`をsaveCustomOnly経由で送るが、`teleportCooldown` は保存/update tagに含めない。Ageのspawn判定は `age < 200`、spawn percent=`clamp((age+partial)/200,0,1)`。Cooldownはprivateの`teleportCooldown > 0`で、tickで40から減少し、`getCooldownPercent=1-clamp((cooldown-partial)/40,0,1)`。したがってUpdate tagだけからcooldown beamの稼働状態を復元するのは不可能。別途公式BlockEvent action_id=1 がcooldown開始通知であり、現clientは`NetworkEvent::BlockEvent`を受け取るがCoreではBell/Pot/chestだけ処理しGateway cooldown stateを保持しない。実装時はAge tagから200tick spawn beamは再現可能。cooldown beamはaction1通知をGateway限定で保存しローカル40tick countdownとして追加すれば状態推定できるが、イベント欠落/遅延では完全再現保証なし。tagのみ実装ならcooldown中beamは非表示という計画の安全側を維持。公式 beam式は`TheEndGatewayRenderer.extractRenderState`: spawnは`sin(spawnPercent*PI)`×`floor(level.maxY)`、cooldownは`sin(cooldownPercent*PI)`×`floor(50)`、色はMAGENTA/PURPLE、animationTime=`floorMod(gameTime,40)+partial`。beam位置/幅/textureは同rendererの`BeaconRenderer.submitBeaconBeam`呼び出しで、Gateway面shaderとは独立。

Java比較根拠は `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/{blockentity/AbstractEndPortalRenderer.java,blockentity/TheEndPortalRenderer.java,blockentity/TheEndGatewayRenderer.java,RenderPipelines.java,rendertype/RenderTypes.java}`、`.../GlobalSettingsUniform.java`、および `minecraft-26.2-decompiled/src/net/minecraft/world/level/block/entity/TheEndGatewayBlockEntity.java`。
## 推奨実装（既存資産の再利用）

1. **対象の生成** — `world/block_entity.rs::rendered_kind`, `is_rendered` にEndPortal/EndGatewayを追加し `sync_block_entity` が通常BE同様に初期化する。既存 `BlockEntityRenderInfo` に gateway/portal distinction、相対anchor transform用のblock position、face mask、Gateway beam stateを足す。抽出元（`app/phases/in_game.rs` の block entity render-info構築箇所）ではPortal maskを上下だけ、Gateway maskを `ChunkStore` の隣接block opaque/face-occlusion helperに基づき算出。未ロード近傍は可視扱いにして穴を避ける。Gateway beam状態は `TheEndGatewayBlockEntity` の update tagフィールドと spawn/cooldown tick semanticsを照合してからのみ抽出し、欠損/不正NBTはheight=0。
2. **専用描画** — 新規 `renderer/pipelines/end_portal.rs` にCPU面geometry builder、共通2 sampler descriptor、Portal/Gateway layer-count別fragment pipeline、Gateway beam drawを置く。既存 `assets::{resolve_asset_path_with_pack_dirs, AssetIndex}` と画像upload/descriptor生成 helper（`renderer/pipelines/block_entity.rs::build_rgba_texture_slot`相当）を再利用し、Jar→resource pack順で上記2 PNGを解決。新規依存なし。Vertexは block-local xyz（Gateway面6方向 / Portal上下）をanchor-relativeに変換して既存CameraUniformへ渡す。面シェーダーは専用 `renderer/shaders/end_portal.vert/.frag`、beamは既存 `entity_renderer::BlendMode::Beam`/`crystal_beam.frag` と既存Beacon/EnderDragon用beam geometry/stateを再利用できるか先に確認（専用新シェーダーを増やさない）。
3. **GPU状態** — `pomme-client/build.rs` shader listへ2面shaderを登録し、既存 `shader::include_spirv!` のbuild compileを使う。再利用pipelineの深度テストは有効、面はshader出力を不透明でdepth-write、通常面の背面cullingは無効（上下/面向き両面で確実に見える）を基本とし、公式pipelineのdepth/blend/cull設定をshader/RenderPipeline比較後に確定する。Gateway beamは通常world opaque描画後、透過/beam順で既存render pass中に描画。面/beam描画はterrain depthを尊重し、overlay/text clear等の後段へ漏らさない。`update_camera`, `recreate_pipeline`, `destroy`, `reload_assets` は画像descriptor/資源を所有する専用pipelineに追加し、reload時は既存BlockEntityPipelineと同じ GPU idle後の置換規約を守る。
4. **tick particle** — `app/phases/in_game.rs` 3694付近のCampfireBlockEntity::particleTick接続が既存のローカルBE particle発生パターン。そこに混ぜず独立した `end_gateway` 対象列挙/関数を追加し、ゲーム固定tickごとにGateway位置・既存乱数・既存 `ParticleStore` の通常particle生成APIを使ってPortal particleを出す。decompiled `TheEndGatewayBlockEntity`/BlockStateのanimateTickを読んで発生確率・位置・速度を移植し、サーバーparticle packetとは二重にしない。パーティクルを外しても面描画は成立する。
5. **描画配線** — `renderer/mod.rs` の `Renderer::new`, `recreate_pipeline`, `reload_assets`, per-frame camera update/draw, teardown に専用pipelineを配線し、描画リストを同じworld anchorで引き渡す。Renderer mod.rs は held-item 作業との共有競合箇所のため、そこでの編集を小さく限定し、統合時に親/担当者間で順序調整する。block entity generic `draw` へmesh/descriptorを押し込まない。

## 検証・段階

- **A CPU**: 独立geometry関数テスト。Portalは6頂点×2面=12頂点、範囲x/z[0,1], y={0.375,0.75}; Gatewayはneighbor-maskに従い4頂点/面で0〜24頂点、anchor再基準位置とside windingも検査。beamのspawn/cooldown曲線は`sin(percent*PI)`、height floor、magenta/purple、40tick+partialを公式式goldenで確認。15/16層別variantをテスト。
- **B shader/資源**: `mise run check`（Client/AGENTS.md:Rust確認はmise経由）で build.rs shadercがコンパイルすることを確認。shader formulaを公式golden UV/color/timeと比較、Sampler0/1 descriptor数、asset pack override/fallback/reload、layout/descriptor数、pipeline再生成/破棄を確認。
- **C runtime**: `mise run build`、Vulkan validation layerでdescriptor/layout/depth警告なし、End Portalの星空が上下面に表示、Gateway面と稼働時のみbeam、遮蔽/複数隣接Portalを実画面確認。GPU/ゲーム起動環境未確認なのでCPU geometryとshader compileだけで実画面確認済みと主張しない。

主なリスクは公式shader式の未照合、opaque-neighbor可視判定の不一致、resource pack reload中のGPU資源寿命、Gateway update tagでは稼働中Stateを復元できない可能性、renderer/mod.rs held-item同時編集競合。最低ゲートはA+Bの全項目と通常 `mise run check`、実画面確認はCで別記録する。

## 実装進捗

- `rendered_kind` / `is_rendered` / `sync_block_entity` に EndPortal・EndGateway を登録。in-game抽出でPortalは上下mask、Gatewayは `shape_occludes` による6方向neighbor遮蔽（未ロード列はvisible）を使用。
- 専用 `renderer/pipelines/end_portal.rs` と `end_portal.vert/.frag` を作成し、専用opaque/depth-write pipelineからframe drawへ接続。公式26.2 fragment式の16色・projective clip座標・行列定数・15/16層・GameTime周期を移植。Portal面は12頂点、Gatewayはmaskに応じ0〜36頂点（triangles、4頂点/faceではなく6頂点/face）。Sampler0/1はjar/resource-pack解決、resource reloadではGPU idle後に置換。
- 未実装: Gateway beam（Ageを読めるが描画経路なし）、Gateway煙/particle。公式状態・beamと混同させず、ここでは出力しない。
- GPU画面/validation確認なし。最終 `mise run check` は成功し（shadercで2本の専用shaderもbuild compile）、focused `mise exec -- cargo test -p pomme-client --locked --profile dev-fast renderer::pipelines::end_portal::tests::geometry_count_height_and_anchor` は1 passed / 0 failed。初回検査では他者作業中の `net/handler.rs:1430` に型不一致が一時発生したが、再確認時には解消済みで当該ファイルは編集していない。
- **レビュー6指摘への追従**: `EndPortalPipeline::draw` は当frameの全drawを連結し単一upload後、非重複rangeごとにPortal/Gateway layer count (15/16) をpushして描画する。capacity拡張は描画frameのfence完了後にのみ置換する前提（Rendererのframe fence待ち）とした。通常Portal×2 + Gateway異面数のrange/元形状CPU testを追加。
- **Fog誤記訂正**: `camera_pos.w` はfog startではなくfog start distance、`fog_color.w` が `fog_end` distanceである。fogは既存`fog.glsl::total_fog_value`と同じspherical/cylindrical maxを0..1にclampして適用する。Positionはcamera-relative world軸なのでpitch時も公式submit positionのdistance基準と一致し、view-spaceへ変更しない。
- **Projective UV**: `Camera::view_projection_with_fov` がVulkan用にclip Y反転済み。gl_Positionを維持し、公式射影UV計算に限ってclip Y符号を戻す処理を追加。CPU期待値とshader source assertionを用意（CPU goldenはshader自体を実行しない）。
- **Texture/color space**: End Portal専用textureを`R8G8B8A8_UNORM`で生成。公式のUNORM sampleと定数をencoded空間で加算し、linear UBO fog色をencoded sRGBへ変換してfog mix後、SRGB swapchain targetへ書く直前にRGBをlinear化する。共通texture helper/global formatは変更しない。CPU sample/fog/color期待値とshader source assertionを追加。
- **Winding/culling**: Portal上下face orderingを修正し、6方向Gatewayのoutward winding CPU testとBack-face culling/CCW/depth-test/depth-write pipeline source assertionを追加。GPU実画面でのfront-face挙動は未確認。
- **Asset load**: 2 PNGをGPU object生成前にdecode・RGBA寸法検証し、破損pack assetはjar builtin画像にfallback。invalid/truncated PNG testを追加。reloadはreplacement pipeline作成完了後に旧pipelineをdestroy/swapする既存順序を維持。GPU allocation/upload失敗時cleanup、明示reload transaction test、bundled asset自身のdecode失敗時にpanic-free無効化は未対応。
- **確認済み**: shader編集後に実行した `mise run check` はshadercを含め一度成功（exit 0）。focused portal testsも直前の変更状態で5 passed / 0 failed。後から同じworktreeの `net/translate.rs:3318` の未関連編集が `usize` vs `u32` で失敗し、最新状態の`mise run check`と`mise run test`はいずれもexit 101。このため、3x3 batch golden等の直近test編集は再実行できていない。`git diff --check`はexit 0。
- **未確認/範囲外**: 全tests/diffcheckは未実施。GPU validation、実画面、実resource reload、buffer capacity resize、Vulkan culling behaviorは未確認。CPU goldenはshader executionではない。Gateway beam/particleは未実装で今回の修正範囲外。

### 再レビュー残件の追記

- DOWN Portal quadは`build_vertices`の頂点順を下向きcrossになるよう反転。生成後の実triangle cross productでPortal lower=-Y / upper=+Y とGateway全6方向のwindingを確認するtestに変更。3×3 Portal test geometryもx/z両軸3位置で検査し、独立batch rangeとlayer countを維持する。
- raster/depth assertionはsource文字列検索をやめ、pipelineで実使用する`rasterization_state` / `depth_stencil_state` production helperを直接testする。fogは`CameraUniform::new`が実際に生成するlaneを読み、RD16のrender band 230.4..256 / environment band 0..1024、distance=128で0.125、near=0/far=1を確認。world→view fog変更なし。shader source文字列検査はshader実行とみなさない。
- Reload CPU asset preparationは初回StartupとReloadを区別。Startupのみ破損pack assetからbuiltinへfallbackし、builtin decode/validation失敗は`RendererError::EndPortal`へ伝播。Reloadは指定画像のどちらかがinvalidならErr、`portal_transaction` prepare成功時だけreplacementをcommitする。Rendererはreload前にdevice idle結果を確認し、wait失敗なら既存renderer resourcesを保持してreturn。CPU callback testはstartup fallback、invalid override、unknown builtin、prepare error非commit、成功swap callbackを確認する。
- **未完了: GPU prepare/cleanup transaction**。`EndPortalPipeline::new`内のdescriptor/layout/pipeline/buffer/image/sampler生成には依然`expect`/panic経路があり、部分GPU資源cleanupとupload `Result` 配線が済んでいない。`util::try_upload_image`はsubmit後のqueue wait失敗時にcommand bufferを返すAPIを持つが、Portal側のretirement/所有者へまだ接続していないため、そこから新image/stagingを安全に解放する保証もない。この段階の失敗はtransactional Errとして保証せず、明示未完了とする。
- `Renderer::reload_assets` はqueue/device idle `Err` 時に旧Portal pipeline等を保持してreloadを中断する。これはreload側がwait失敗後に破棄しない契約であり、context teardownのnon-DeviceLost wait retryは現状無期限retryでshutdown/world保存を阻害し得る。並行skin reviewの調査対象であるため、Portalから「Renderer shutdownは完全安全」と主張しない。skin pending/retirement contractや共通wait方針は今回変更していない。
- 通常SRGB swapchain target以外へfallbackした際のPortal出力色契約は独立残件として維持し、swapchain全般の改造は今回しない。

### P1 GPU prepare / submit寿命の実装追記（2026-10-05）

- `EndPortalPipeline::new` はimage/view creation (`try_create_gpu_image_2d`)、staging/uniform/vertex buffer (`try_create_mapped_buffer`)、sampler、descriptor layouts/pool/sets、pipeline/shader modulesをResult化。`Build` guardが作成済みpipeline/layout/pool/buffer/image等を所有し、pre-submitのどの後続prepare failureでもDropで解放する。二枚目texture/descriptor/pipeline allocation failureでも、guardに追加済みの一枚目や先行bufferは保持/cleanupされる。Sampler生成失敗では当該image/view/allocationを直接解放。
- uploadは旧`util::upload_image`ではなく`util::try_upload_image`。submit前Errorはstagingとimage/view/allocationを解放、submit後wait不明Errorはcommand/staging/staging allocation/image/view/image allocationを単一`PendingPortalUpload`としてContext collectionへ移してErr。`VulkanContext`がRenderer::new完了前からcollectionを所有し、teardown idle successまたはDeviceLost後、command pool/allocator破棄前にreclaimする。non-DeviceLost teardownは既存の無限wait retry契約のまま（abort/free/独自retry変更なし）。
- pendingが残る状態で再reload/prepareする場合、追加allocation前にdevice idleを要求し、失敗ならErrで拒否、成功時のみpending一式をreclaimしてから準備する。reload commitは新pipeline/resources完成後だけ。resizeでは新Swapchainのrender passに対する新pipelineを先に作り、成功した場合だけ旧pipeline破棄とswapchain置換を行う。startup asset fallback / reload strict decode条件は維持。
- 実productionのCPU stateでGPU failure injectionを動かしたrollback検証は未実施。現在のportal module CPU testsは4件（geometry、batch range/layer、triangle winding/state、UV/fog/color CPU contract）で、allocation段別cleanup・pending owner idle drain・resize prepare failureの実所有CPU test matrixは未完了。GPU実機failure injection/validationも未確認。static inspectionでGPU callsiteの`.expect`、panic helper呼び出し、旧unsafe upload callsiteは残っていない。
- 確認: `cd Client && mise run check` は再構成後に一度exit 0。その後`Cargo.toml`に依存source変更が並行して入り、`--locked`ではlock update要求でexit 101（当時`Cargo.lock`は触らず）。テスト試行では`Cargo.lock`がworktreeで変更状態となり、その後test/checkがgit依存`https://github.com/azalea-rs/azalea?branch=26.2`のremote-tracking branch `origin/26.2`不足でexit 101。Cargo.lockは当agentでは編集していない。`cargo test --no-run`は一度`powf`型曖昧エラーも出し、修正後の再試行は上記git branch errorで停止。対象Rust 3ファイルrustfmt check / `git diff --check` exit 0。既存のgeometry/fog/color契約はコードを作業中に置き換えてしまい、近似的に再構成したテストを含むため、**元の静的確認/元testsを保持したとは言えず復旧が必要**。このportal GPU作業でそれらを触らない指示を満たせなかった点を明記する。
- **今回確認**: focused portal tests 6 passed / 0 failed（geometry, actual triangle winding/state, 3×3 batches, asset policy/transaction callback等）、生成CameraUniformのfog test 1 passed / 0 failed。初回fog testはdistance引数の片方を0にしていたため失敗したが修正後再実行済み。`mise run check` exit 0。`mise run test` はClient 1528 passed / 1 failed、protocol 52 passed、singleplayer 1 passed / 1 ignored。残る失敗は並行変更中の `net::azalea_compat::legacy_filled_map_frame_metadata_runs_translation_decode_remap_event_and_store`（`net/azalea_compat.rs:954` の`Option::unwrap()`）のみ。reload source assertionは新しいwait結果確認を検索するよう最小更新し、focused `equipment_texture_cache_is_texture_keyed_and_cleared_after_gpu_idle_on_reload` 1 passed / 0 failed。`git diff --check` exit 0（CRLF warningのみ）。GPU/device画面・validation・実reloadは未確認。beam/smoke未対応を維持する。
