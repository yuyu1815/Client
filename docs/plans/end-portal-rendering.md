# End Portal / End Gateway 描画 実装計画

## 確定している欠落・適用範囲

`world/block_entity.rs::rendered_kind` は End Portal/Gateway を描画対象にせず、`is_block_entity_block` は両者をBEブロックとして扱う。よって `renderer/chunk/mesher.rs` は通常モデルのないブロック面を出さず、`renderer/pipelines/block_entity.rs` にも専用描画がない（調査根拠: `rendered_kind` 643–687、mesher 2112–2116、BE pipeline 551–625/1534–1537）。スクリーンショット `Pomme screenshots2026-10-04_20.21.37` の暗い面と Modrinth `2026-10-04_20.19.57` の星空を区別し、暗色planeの代用は禁止。

今回の範囲は両BEの「面」描画と End Gateway の条件付き beam。Gateway beam は面の別要素として明記し、NBT等から稼働状態を確実に復元できない場合は安全に非表示（誤った常時beamを出さない）とする。ゲートウェイ煙 `animateTick` は独立した小項目で含める。テレポート/サーバー挙動、ポータル接触時画面効果は対象外。

## 公式動作の対応

参照した decompiled 26.2: `AbstractEndPortalRenderer` は block entity が許可する全方向の単位cube面を提出し、Sampler0=`textures/environment/end_sky.png` と Sampler1=`textures/entity/end_portal/end_portal.png` を使う。`TheEndPortalRenderer` は unit cube に translation y=0.375、scale y=0.375 を適用するため面は y=0.375..0.75、x/z=0..1。表示面は上下2面のみ。`TheEndGatewayRenderer` は近隣ブロックによる face occlusion で表示面を決める。面には同じ2 textureを使うが RenderPipelines の `PORTAL_LAYERS` は Portal=15、Gateway=16。

Portal shader の正確な式は公式 client jar の shader resource をこの checkout で発見できず（存在したのは decompiled Java と clientOnly class jar）、推測で書き写さないこと。実装前に配布 client jar/assets の `assets/minecraft/shaders/core/rendertype_end_portal.{vsh,fsh}` と `RenderPipelines`/`RenderTypes` を照合し、座標投影・GameTimeの単位/ラップ・各層のUV縮尺/移動速度・色重みを転記し、15/16層以外も一致テストする。Java比較は `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/{blockentity/AbstractEndPortalRenderer.java,blockentity/TheEndPortalRenderer.java,blockentity/TheEndGatewayRenderer.java,RenderPipelines.java,rendertype/RenderTypes.java}`。

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
