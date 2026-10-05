# 非生体エンティティ — 公式26.2互換計画

- **比較基準:** Java Edition 26.2 decompiled source (`minecraft-26.2-decompiled/src/net/minecraft/`); 公式EntityTypes登録と継承class、entity/packet/rendererの実装、およびresource登録を母集合とする。
- **共通base:** `a12e38d9ea09d290a0b1736fe48a1cdafe49325f`。現行実装の静的照合はこのbaseのPomme sourceに対して行う。
- **SteelMC:** gitlink pinは `third_party/SteelMC` = `0b1f87c36a664f08d81397e942fb1e19f6eb282b`。この専用treeではsubmodule内容が未初期化のためSteel sourceは未読、server動作も未実行・未比較。
- **比較環境:** AはPomme client→公式26.2 dedicated server、BはSteelMC singleplayer→公式 integrated server。描画・client predictionとserver authorityの判定を分離し、A/Bの結果を混ぜない。今回は静的調査・計画だけで、いずれも未実行。
- **分母出所:** `minecraft-26.2-decompiled/src/net/minecraft/world/entity/EntityTypes.java:165-322` の登録宣言とclass継承を照合し、対象65（boat/raft 20、minecart 7、projectile/flying 20、other 18）を確認した。表はその65 unique type名を全列挙する。living 93は本表の除外対象で、LivingEntity継承classを分類根拠とする。公式renderer登録は `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/EntityRenderers.java:70-274` を別途確認する。65はtype登録分母だけであり、metadata schema/value、variant、packet順序、時間境界のscenario分母はまだ確定していない。未列挙scenarioを合格/100%に数えない。

## 対象と責任境界

対象は公式EntityTypesの非living class 65登録について、spawnから状態同期・補間・可視化・副作用・破棄までのclient semantic consumerである。Boat/raft、minecartとそのpayload、projectile/flying entity、Display、item/frame、TNT/falling block、orb/crystal、その他の非livingを含む。Entity typeのwire ID解釈やversion conversionは`connection-and-protocol`、生体classは`living-entities`、地形block/model/lighting consumerは`block-rendering`、item/model/skin資産consumerは`item-rendering`、resource lifetimeは`resources`、particle/audio eventの実現は各ownerに属する。

このplanはネットワークで受け取った非生体のsemantic stateを、適切なconsumerへ意味を失わず届ける契約を所有する。Protocol ownerはwire ID/packetをversion-awareにdecodeし、unknown type/schemaを明示的に保持して渡す。ここでは既知typeの意味・metadata適用・entity identity・client presentation lifecycleを所有する。Invalid frame/packetの切断等のtransport policyはprotocol ownerとの契約で決め、この機能で独自に接続を続行しない。

- **正本:** server同期済みentity state。client補間値・trajectory予測値・render snapshotは派生物であり、server correctionで置換可能。予測projectileはvisual-onlyで、hit/damage/block mutation/despawnを決めない。
- **共通サービス:** 衝突問い合わせは`physics`のquery契約を使うが、このfeatureはprojectile/vehicle側のvisual stateと適用対象を決める。vehicle/projectileのauthority、item use、落下block/TNTのsimulationはserver-gameplay/server-world側。
- **分類規則:** official class hierarchyが基準。ArmorStand/Mannequin/Playerは公式LivingEntity系としてliving denominatorに含め、Client側`vehicles` mapに入ることがあっても分類を変えない。`minecraft-26.2-decompiled/src/net/minecraft/world/entity/EntityTypes.java:170,248,321`。Client storeの型名を公式機能分類と混同しない。
- **表示を持たないtype:** 公式NoopRendererはInteraction/Markerの描画が無いという意味だけで、server semanticsを無視する意味ではない。AreaEffectCloudのNoop geometryもparticle/effect出力と別契約。

隣接ownerとの境界条件:

| 境界 | このownerが渡す/受け取る契約 | owner外に残すもの |
|---|---|---|
| connection-and-protocol | negotiated registry解決済みの識別子、spawn/motion/teleport/metadata/passenger/removeを順序とgeneration付きで受け取る。未知type/schemaは別状態として保持し、別typeへfallbackしない。 | wire ID変換、packet decode/invalid-frame transport policy |
| physics | entity transform/必要なcollision query入力を消費する。乗員座席位置と視点はphysics/living契約を使う。 | 一般衝突・player移動予測・vehicle操縦physics |
| server-gameplay / server-world | 予測をauthorityとして扱わず、server correction/eventを正本として描画へ反映する。 | projectile impact/combat、vehicle運動、TNT/falling block/world mutation |
| block-rendering / item-rendering | typed block state/item stackとentity transform/lighting条件を受け渡す。 | mesh/asset/modelの生成とresource解決 |
| particles / audio | entity lifecycle eventと位置・種別・payloadを一回性/時系列契約とともに渡す。 | effect provider、音響mixer、実際のemit/mix |
| living-entities | passengerがliving/playerである場合も同一entity identityとattachment transformを共有する。 | 生体のpose・skin/equipment層の所有と描画 |

## 公式の機能母集合

以下は`minecraft-26.2-decompiled/src/net/minecraft/world/entity/EntityTypes.java:165-322`の登録宣言と継承classを照合した**65/65**の非living inventory。boat/raft 20、minecart 7、projectile/flying 20、other 18。ArmorStand/Mannequin/Playerはliving側。各行では公式renderer登録とPomme本番consumerの証拠を別欄にする。renderer登録はPomme実装の証拠ではなく、Pomme consumerが検索で見つからない場合も未実装確定ではない。`minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/EntityRenderers.java:70-274`は登録表、個別drawはframe assemblyから具体consumerまで追跡する。statusは静的source追跡のみで動作一致の証明ではない。

| 公式EntityType | class / official renderer registration | Pomme production state / consumer finding (static) |
|---|---|---|
| ACACIA_BOAT | Boat / BoatRenderer | `VehicleState`→boat render info/variant: draw pathあり |
| ACACIA_CHEST_BOAT | ChestBoat / BoatRenderer | 同boat path、chest variant: draw pathあり |
| BAMBOO_RAFT | Raft / RaftRenderer | 同boat path、raft variant: draw pathあり |
| BAMBOO_CHEST_RAFT | ChestRaft / RaftRenderer | 同boat path、chest raft variant: draw pathあり |
| BIRCH_BOAT | Boat / BoatRenderer | 同boat path/wood variant: draw pathあり |
| BIRCH_CHEST_BOAT | ChestBoat / BoatRenderer | 同boat path/chest wood variant: draw pathあり |
| CHERRY_BOAT | Boat / BoatRenderer | 同boat path/wood variant: draw pathあり |
| CHERRY_CHEST_BOAT | ChestBoat / BoatRenderer | 同boat path/chest wood variant: draw pathあり |
| DARK_OAK_BOAT | Boat / BoatRenderer | 同boat path/wood variant: draw pathあり |
| DARK_OAK_CHEST_BOAT | ChestBoat / BoatRenderer | 同boat path/chest wood variant: draw pathあり |
| JUNGLE_BOAT | Boat / BoatRenderer | 同boat path/wood variant: draw pathあり |
| JUNGLE_CHEST_BOAT | ChestBoat / BoatRenderer | 同boat path/chest wood variant: draw pathあり |
| MANGROVE_BOAT | Boat / BoatRenderer | 同boat path/wood variant: draw pathあり |
| MANGROVE_CHEST_BOAT | ChestBoat / BoatRenderer | 同boat path/chest wood variant: draw pathあり |
| OAK_BOAT | Boat / BoatRenderer | 同boat path/wood variant: draw pathあり |
| OAK_CHEST_BOAT | ChestBoat / BoatRenderer | 同boat path/chest wood variant: draw pathあり |
| PALE_OAK_BOAT | Boat / BoatRenderer | 同boat path/wood variant: draw pathあり |
| PALE_OAK_CHEST_BOAT | ChestBoat / BoatRenderer | 同boat path/chest wood variant: draw pathあり |
| SPRUCE_BOAT | Boat / BoatRenderer | 同boat path/wood variant: draw pathあり |
| SPRUCE_CHEST_BOAT | ChestBoat / BoatRenderer | 同boat path/chest wood variant: draw pathあり |
| MINECART | Minecart / MinecartRenderer | cart body pathあり、cargo blockなし |
| CHEST_MINECART | MinecartChest / MinecartRenderer | body+chest block pathあり; cargoはblock-rendering境界 |
| FURNACE_MINECART | MinecartFurnace / MinecartRenderer | body+furnace state pathあり |
| TNT_MINECART | MinecartTNT / TntMinecartRenderer | shared cart+TNT pathあり; fuse/flash fidelity unknown |
| HOPPER_MINECART | MinecartHopper / MinecartRenderer | shared cart+hopper block pathあり |
| COMMAND_BLOCK_MINECART | MinecartCommandBlock / MinecartRenderer | shared cart+command block state pathあり |
| SPAWNER_MINECART | MinecartSpawner / MinecartRenderer | shared cart+spawner block pathあり; animated contents unknown |
| ARROW | Arrow / TippableArrowRenderer | arrow state/render-info pathあり; tipped data/rotation/impact edge unknown |
| SPECTRAL_ARROW | SpectralArrow / SpectralArrowRenderer | shared arrow path; spectral variant/effect fidelity unknown |
| TRIDENT | ThrownTrident / ThrownTridentRenderer | projectile model/foil pathあり |
| DRAGON_FIREBALL | DragonFireball / DragonFireballRenderer | model pathあり |
| SHULKER_BULLET | ShulkerBullet / ShulkerBulletRenderer | model/overlay pathあり |
| WITHER_SKULL | WitherSkull / WitherSkullRenderer | model/invulnerability texture pathあり |
| LLAMA_SPIT | LlamaSpit / LlamaSpitRenderer | model pathあり |
| EGG | ThrownEgg / ThrownItemRenderer | thrown-item billboard pathあり |
| ENDER_PEARL | ThrownEnderpearl / ThrownItemRenderer | thrown-item pathあり |
| EXPERIENCE_BOTTLE | ThrownExperienceBottle / ThrownItemRenderer | thrown-item pathあり |
| FIREBALL | LargeFireball / ThrownItemRenderer | thrown-item/full-bright pathあり |
| LINGERING_POTION | ThrownLingeringPotion / ThrownItemRenderer | thrown-item pathあり |
| SMALL_FIREBALL | SmallFireball / ThrownItemRenderer | thrown-item/full-bright pathあり |
| SNOWBALL | Snowball / ThrownItemRenderer | thrown-item pathあり |
| SPLASH_POTION | ThrownSplashPotion / ThrownItemRenderer | thrown-item pathあり |
| BREEZE_WIND_CHARGE | BreezeWindCharge / WindChargeRenderer | state/particle pathあり; dedicated draw consumer未発見（gap候補、汎用assembly再確認要） |
| WIND_CHARGE | WindCharge / WindChargeRenderer | state/particle pathあり; dedicated draw consumer未発見（gap候補、汎用assembly再確認要） |
| EYE_OF_ENDER | EyeOfEnder / ThrownItemRenderer | generic spawnのみ確認; type-specific draw consumer未発見（gap候補） |
| FIREWORK_ROCKET | FireworkRocketEntity / FireworkEntityRenderer | stack/life/launch event/particle stateあり; model draw consumer未発見（gap候補） |
| FISHING_BOBBER | FishingHook / FishingHookRenderer | bobber/line draw consumer未発見（gap候補） |
| AREA_EFFECT_CLOUD | AreaEffectCloud / NoopRenderer | geometryなし; particle metadata/duration/radius coverage unknown |
| BLOCK_DISPLAY | Display.BlockDisplay / BlockDisplayRenderer | typed block state→block display extraction/mesh pathあり |
| ITEM_DISPLAY | Display.ItemDisplay / ItemDisplayRenderer | stack/context→item pipeline pathあり; context/interpolation fidelity unknown |
| TEXT_DISPLAY | Display.TextDisplay / TextDisplayRenderer | payload/style→text display pathあり; layout/interpolation fidelity unknown |
| END_CRYSTAL | EndCrystal / EndCrystalRenderer | age/beam/base extraction+geometry pathあり |
| EXPERIENCE_ORB | ExperienceOrb / ExperienceOrbRenderer | value/age+model pathあり; animation fidelity unknown |
| FALLING_BLOCK | FallingBlockEntity / FallingBlockRenderer | native spawn block-state→block renderer pathあり; converted protocol mapping unknown |
| TNT | PrimedTnt / TntRenderer | fuse/blockstate render info pathあり; flash/fuse interpolation unknown |
| GLOW_ITEM_FRAME | GlowItemFrame / ItemFrameRenderer | frame/item/rotation/map+glow pathあり |
| ITEM_FRAME | ItemFrame / ItemFrameRenderer | frame/item/rotation/map pathあり |
| ITEM | ItemEntity / ItemEntityRenderer | separate `ItemEntityStore`→drop item pipelineあり; physicsは別owner |
| EVOKER_FANGS | EvokerFangs / EvokerFangsRenderer | entity-specific production draw未発見（gap候補） |
| INTERACTION | Interaction / NoopRenderer | 意図的no-visual; hitbox semantics server-gameplay/interaction |
| LEASH_KNOT | LeashFenceKnotEntity / LeashKnotRenderer | production draw未発見（gap候補） |
| LIGHTNING_BOLT | LightningBolt / LightningBoltRenderer | spawnでflash tickのみ; bolt mesh未発見（gap候補） |
| MARKER | Marker / NoopRenderer | 意図的no-visual |
| OMINOUS_ITEM_SPAWNER | OminousItemSpawner / OminousItemSpawnerRenderer | production draw未発見（gap候補） |
| PAINTING | Painting / PaintingRenderer | production painting draw未発見（gap候補） |

**網羅性と分母:** 表のtype名は65件で、20+7+20+18=65。公式登録宣言は `minecraft-26.2-decompiled/src/net/minecraft/world/entity/EntityTypes.java:165-322`、renderer mapは `minecraft-26.2-decompiled/src/net/minecraft/client/renderer/entity/EntityRenderers.java:70-274`。各行の公式rendererは登録の有無を示すだけであり、Pomme production consumer欄は別のsource trace結果である。WindCharge/BreezeWindChargeも同様に、公式renderer登録があることとPommeで本番描画されることを分ける。scenario分母（全metadata accessor/schema/value、variant、packet順序、描画時刻/境界、effect）は未列挙なので、type全件表だけで互換100%やscenario全網羅とはしない。

## 現行本番route・比較レイヤー

以下のrouteは専用treeのbase sourceを入口からrender frame/consumerまで追った静的経路。function/symbolとpathを示し、汎用helperの存在だけをlive routeと見なさない。`in_game.rs`内のhelper定義とframe assembly callを区別する。`EntityRenderers`の登録は公式側の登録確認であり、Pommeのdraw完了証拠ではない。

| type群 / 範囲 | Pomme production route（入口 → state/metadata → frame → consumer） | 確認状態 |
|---|---|---|
| 全entity spawn / movement / teleport / remove | `pomme-client/src/app/core.rs:5545-5645` `NetworkEvent::EntitySpawned` → `pomme-client/src/entity/mod.rs:1588-1731` `VehicleState`/`EntityStore`; delta/rotation/motion/teleport handlers in `pomme-client/src/app/core.rs:5652-5835` update living, item, vehicle and `entity_positions`; metadata in `pomme-client/src/app/core.rs:6100-6191` → `EntityStore::apply_vehicle_metadata` etc. at `pomme-client/src/entity/mod.rs:2140-2400,2960-3045`; `NetworkEvent::SetPassengers` `pomme-client/src/app/core.rs:4430-4440` → `EntityStore::set_passengers` `pomme-client/src/entity/mod.rs:1899-2013`; `NetworkEvent::EntitiesRemoved` `pomme-client/src/app/core.rs:6038-6070` → `EntityStore::remove_entity` `pomme-client/src/entity/mod.rs:3709-3730`. |入口/状態/主要更新を確認。cleanup/generation・packet ordering全組合せは未確認。event-to-render stateとは別にserver movement authorityを保持。|
| boat/raft (20) | `pomme-client/src/app/core.rs:5618-5628` spawn → `pomme-client/src/entity/mod.rs:1588-1715` vehicle state + boat metadata → frame call `pomme-client/src/app/phases/in_game.rs:6411-6415` to `boat_render_infos` (definition `pomme-client/src/app/phases/in_game.rs:7365-7438`) → `EntityRenderInfo` → `pomme-client/src/renderer/mod.rs:1789-1825` `Renderer::render_world` → `pomme-client/src/renderer/pipelines/entity_renderer.rs:4141-4180` `EntityRenderer::draw`. |generic entity draw route exists for boat family; all wood/chest/raft variants and pixel fidelity not independently proven. |
| minecart (7) + cargo | same spawn/metadata route → `VehicleState` (`minecart_display_state`, offset/furnace flags) → frame call `pomme-client/src/app/phases/in_game.rs:6416` to `minecart_render_infos` (definition `pomme-client/src/app/phases/in_game.rs:8044-8110`) → `Renderer::render_world` → `EntityRenderer::draw`; cargo call `pomme-client/src/app/phases/in_game.rs:6481-6492` to `minecart_cargo_render_infos` (definition `pomme-client/src/app/phases/in_game.rs:7535-7700`) → item/block render lists → `pomme-client/src/renderer/mod.rs:3260-3262` item draw and block entity pipeline. |body/cargo route found. cargo uses shared item/block consumers; this does not establish all payload/contents equivalence. |
| projectile / flying (20) | spawn/motion/move/teleport/metadata in `pomme-client/src/app/core.rs:5545-5835,6100-6191` → projectile display state/`tick_projectile_displays` in `pomme-client/src/entity/mod.rs:1459-1513,1859-1870,3830-3920` → frame calls `pomme-client/src/app/phases/in_game.rs:6402-6410` and producers `pomme-client/src/app/phases/in_game.rs:8111-8884` → `EntityRenderInfo`/item records → `pomme-client/src/renderer/mod.rs:1789-1825` `Renderer::render_world` → `EntityRenderer::draw` or item pipeline. |Arrow and generic projectile routes found; do not infer each official type or WindCharge dedicated visual from shared helper alone. WindCharge-specific Pomme draw consumer not confirmed; generic assembly still requires per-kind tracing. |
| Display (BLOCK/ITEM/TEXT) | `NetworkEvent::EntitySpawned` and Display metadata handlers `pomme-client/src/app/core.rs:5545-5645,6100-6191` → `VehicleState.display`/payload setters `pomme-client/src/entity/mod.rs:1518-1556,2319-2400` → block/item extraction `pomme-client/src/app/phases/in_game.rs:6490-6788`, text extraction/draw `pomme-client/src/renderer/mod.rs:3362-3379` `extract_text_displays_scaled`/`draw_text_display` → block/item/text render pipelines via `Renderer::render_world`. |production extraction route found for block/item/text; official client interpolation clock/rules versus current snapshot handling are not established as equivalent. |
| TNT / falling block | spawn data and metadata `pomme-client/src/app/core.rs:5545-5645,6100-6191` → `VehicleState.falling_block`/TNT fuse/state (`pomme-client/src/entity/mod.rs:1588-1715,2140-2235`) → block mesh/item records in `pomme-client/src/app/phases/in_game.rs:6490-6788` → `pomme-client/src/renderer/mod.rs:1789-1825` `Renderer::render_world` and item/block pipeline. |specific assembly found. Server fuse/explosion/falling simulation is not this client route. |
| crystal / orb / other generic entity kinds | spawn/state/metadata routes above → frame calls `pomme-client/src/app/phases/in_game.rs:6417,6420-6425` to `end_crystal_render_infos` (`pomme-client/src/app/phases/in_game.rs:7701-7740`) and `experience_orb_render_infos` (`pomme-client/src/app/phases/in_game.rs:7934-7974`), plus `projectile_render_infos` (`pomme-client/src/app/phases/in_game.rs:8711-8780`) → `pomme-client/src/renderer/mod.rs:1789-1825` `Renderer::render_world` → `pomme-client/src/renderer/pipelines/entity_renderer.rs:4141-4180` `EntityRenderer::draw`. |Crystal/orb/projectile generic draw route found. This does not prove all registered nonliving types have a matching producer or equivalent output. |
| item, frames, effect-only/specials | ITEM spawn additionally enters `ItemEntityStore` at `pomme-client/src/app/core.rs:5628-5632`, `build_item_render_infos` at `pomme-client/src/app/phases/in_game.rs:8885-8925` → `Renderer::render_world` → item pipeline. ItemFrame metadata at `pomme-client/src/app/core.rs:6100-6145` → frame state and extraction at `pomme-client/src/app/phases/in_game.rs:9088-9185` → item render lists/map quads → `Renderer::render_world`; this route was source-traced but not output-compared. Lightning spawn currently sets flash ticks only (`pomme-client/src/app/core.rs:5557`). Searches in listed producer roots found no type-specific producer for FishingHook/Painting/LeashKnot/EvokerFangs/OminousItemSpawner. |final assembly and concrete consumer not verified for every row. Absence in searched producer symbols is a gap candidate, not proof of absent behavior. Noop/effect-only semantics are separate. |

### Server simulation, network trace, and client presentation are separate

| layer | meaning / evidence | this feature owns / excludes |
|---|---|---|
| **Server-authoritative entity tick** | Official dedicated/integrated server runs `minecraft-26.2-decompiled/src/net/minecraft/server/level/ServerLevel.java:736-767` `tickNonPassenger`/`tickPassenger`: non-passenger `entity.tick()`, passenger `entity.rideTick()`, then recursive passenger ticks. Entity subclass ticks determine movement, gravity/drag, lifetime, collision, impact, fuse/explosion and world mutation; inspect each concrete subclass before asserting coefficients/order. This server simulation is separate from Display's client interpolation branch. |Compare traces/visible consequences; this feature must not implement server authority. Vehicle operation, damage/impact, TNT/falling-block mutation belongs to `server-gameplay`/`server-world`; collision query contract to `physics`. |
| **Networked state trace** | Spawn, relative movement, rotation, motion, teleport/correction, metadata, passenger and removal events form server-to-client observations; Pomme handlers above were statically traced only. Packet ordering/coverage and version ID mapping depend on `connection-and-protocol`. |Record wire/decode result, IDs/order/tick, server transform/metadata and removal separately for A and B; this feature owns semantic application after decoded events, not transport decoding. |
| **Client interpolation / drawing** | Pomme client tick `pomme-client/src/app/phases/in_game.rs:3914-3915` ticks projectile display approximations; interpolation occurs in producers `pomme-client/src/app/phases/in_game.rs:7365-7438,8044-8110,8711-8780`; Display extraction is at `pomme-client/src/app/phases/in_game.rs:6490-6788` and text consumer at `pomme-client/src/renderer/mod.rs:3362-3379`. |Compare presentation clocks and output against official client rules separately from server tick and received state. Official `Display.tick` gates interpolation start/duration/render-state updates on `level().isClientSide()` (`minecraft-26.2-decompiled/src/net/minecraft/world/entity/Display.java:151-175`), and `calculateInterpolationProgress` uses client `tickCount` plus partial tick with clamp (`minecraft-26.2-decompiled/src/net/minecraft/world/entity/Display.java:360-364`). This client interpolation is not `ServerLevel` entity ticking. A/B runtime observations have not been made. |

A = **Pomme client → official 26.2 dedicated server**; B = **SteelMC singleplayer → official 26.2 integrated server**. Maintain separate trace/result ledgers and owners: A records client-facing network/presentation gaps owned by this feature plus protocol dependencies; B records integrated-server/SteelMC gaps under singleplayer/server owners, while this feature owns only its client semantic consumer contract. SteelMC pin is recorded above; its source and runtime have not been inspected here. Do not combine A/B evidence or assign server simulation bugs to client drawing.

### 確認済みと未調査

- **確認済み静的:** 公式登録表から65 type名を分類; 公式renderer registration roots、Pomme spawn/state/movement/metadata/passenger handlers、boat/minecart/projectile/display/TNT/falling-block/crystal/orbのframe producersと一般consumerへのrouteを確認。これは動作同等性を証明しない。
- **gap候補（未確定）:** WindCharge/BreezeWindCharge/EyeOfEnder/FireworkRocket/FishingBobber/EvokerFangs/LeashKnot/LightningBolt/OminousItemSpawner/Paintingのtype-specific production consumer。共通entity draw assemblyや共有pipelineで消費される可能性を最終追跡できていないため、未実装断定を避ける。ItemFrameのvisual output比較、および全65のproduction consumer対renderer一対一照合も未了。
- **未調査:** 全65の全metadata schema/type/value、全variantのgeometry/resource、全official subclass motion係数と更新順、passenger packet permutation、delete/reset/teleport/ID generation順序、A/B runtime。renderer pathを見つけた種も画素/挙動一致は未証明。
- **判断しないこと:** 既存TODO/古い監査指摘だけを未実装根拠にしない。Noop renderer=entity lifecycle/effect無効とも、draw pathあり=同等動作ともみなさない。

## 互換にする設計

### 状態所有・identity

entityの意味ある状態は一つのserver同期正本で所有し、描画用途ごとに別々のpacket解釈を持たない。identityは`(world_epoch, entity_id, spawn_generation)`とし、UUIDは識別・profile cacheの補助であってentity ID再利用防止の代用にしない。`world_epoch`はdimension/world/session resetごとに変え、`spawn_generation`は同IDの再spawnごとに更新する。metadataは既知typeごとのschema付き値として保持し、unknown type/index/valueは既知kindへ誤変換せず診断可能な形で保全する。invalid packetの切断/拒否はprotocol契約に従う。

Passenger stateは順序付きpassenger ID listを正本、`vehicle_of`を逆引きとする。現行`EntityStore::set_passengers` (`pomme-client/src/entity/mod.rs:1899-2013`) は `vehicles.entry(vehicle_id)` にkind=Noneのvehicle endpoint placeholderを作る一方、spawnされていないpassenger IDは`vehicle_of`のedgeとして保存する。この2つは別状態であり、passenger用entity placeholderを生成するものではない。現行コードのこの経路にcycle拒否・重複ID拒否を確認できていないため、それらは目標不変条件/検証対象であって既存保証ではない。packet到着時に片端未spawnのedgeをどう解消するか、重複/順序/replace/remove/re-spawn後の整合は将来検証する。複数passengerと多段乗車を許容する目標とし、座席offset・視点はphysics/living側の型別契約を使う。位置近接から乗車関係を推測しない。

### 更新順と派生値

受信したイベントはentity identity/generationとsource tick/orderを照合し、次の順序で一貫して適用する。

1. world epoch切替時に旧state/worker/snapshot/effect attachmentを破棄する。
2. spawn/同ID置換時に旧generationのconsumer/effect/passenger edgeを解除し、新generationを作る。
3. server metadata・velocity・relative move・teleport/correctionを該当schemaへ反映し、correction対象の補間/予測revisionをinvalidateする。
4. passenger ordered edgesを整合させる。
5. presentation用補間・短期predictionを進める。authority値は変更しない。
6. confirmed eventからparticle/audio依頼を一回だけ生成し、重複event/replay/ID reuseによる二重発火を防ぐ。
7. 完全に反映された状態からimmutable render snapshotを作り、各render consumerへ渡す。
8. despawn/replace/reset後にold epoch/revision/generationのasync結果は必ず捨てる。

現行sourceから全packet順序・delete/teleport/epoch順の一致が確認できていないため、これは目標設計であり既存実装の主張ではない。protocol側packet順とcallback timingとの具体照合をM0で行う。

### Projectile・vehicle presentation

公式server stateが位置/速度/impact/despawnの正本。client projectile予測はrender補間に限り、authority packetの間を視覚的につなぐ。速度更新、metadata、ground/in-ground/no-physics変化、teleport/correction、world epoch、despawn/replacementで予測revisionを更新し、遅延worker結果は`(epoch,id,generation,revision)`一致時だけ採用する。予測接触からdamageやblock mutationを発生させない。現行の64 tick extrapolation/水域drag固定/壁越しghostは比較対象となる既知の近似であり、公式26.2のProjectile/Arrow/Throwable/Firework/WindCharge等の個別tick順と数値をsourceから抽出するまでは互換と宣言しない。値や境界をsource未確認のまま推測で埋めない。

Boat/minecartは受信位置・角度・metadata・passenger edgeを使い、client-side steering/predictionがserver correctionを上書きしない。wood/chest/raft、cart payload/block entity content、furnace fuel、TNT fuse/flash等のvariant/payloadは型付き意味を保ってblock/item consumerへ渡す。落下block、TNT、projectile collision・impactはclient描画とserver-world/gameplay処理を区別する。

### Display・その他の表現

DisplayはTRS、quaternion両側、billboard、start delay/duration、transform durationとposition/rotation duration、teleport duration、brightness、shadow、view range、width/height、glow overrideを型付き・losslessに保持する。補間の時間基準はserver synchronized updateを受けたclient tick clockとし、delay中は前snapshotを維持、開始後は公式26.2の進捗/clamp/transform規則に沿い、duration終端でtargetに正確に収束する。BlockDisplay payloadはblock-rendering、ItemDisplay payload/contextはitem-rendering、TextDisplay component/style/font consumerはchat/resourcesと契約する。このownerはentity変換・寿命・snapshot組立を担う。異常な非有限TRS/quaternionや不正enumを隣接consumerへ伝播させず、公式側の受理/default/error規則をsource照合して確定する。

TNT/Firework/Crystal/Orb/ItemFrame/Painting/FishingHook/LeashKnot/EvokerFangs/OminousItemSpawner/AreaEffectCloud/Interaction/Marker/LightningBoltは各自のmetadata、age/fuse, attachment, payload, no-visual/effect lifecycleを同じidentity lifecycleに結び付ける。意図的なNoop rendererはvisual bodyを作らないが、lifecycle/hitbox/server semantics/event contractを消さない。Particle/audio side effectはevent ID/sequenceを保ってownerへ引き渡す。

## 実装順序

| Milestone | 目的・優先根拠 | 依存とscope | Check / 将来比較 | 完了しても残るもの |
|---|---|---|---|---|
| **M0 — 65 registration / renderer denominator** | target setを早期に固定し、renderer有無から本番実装を誤推定しない。 | `EntityTypes`登録/class継承から65 type全件を再生成可能にし、official renderer registration、Pomme production consumer、scenario inventoryを別欄で記録する。 | 全type名・群別件数照合。全65でspawn/update/remove/metadata適用性、variant/accessor、未確認を識別。consumer未発見はgap candidateのまま。これは将来作業で現時点のscenario網羅率は未算出。 | 数値fidelity、runtime比較。 |
| **M1 — network→identity/state→cleanup route** | stale stateや誤った型/ID適用を防止し、decoded observationから表示stateまでの所有を明確化。 | `connection-and-protocol`のregistry/decode/order契約に依存。spawn/置換、move/rotation/motion/teleport、metadata、remove/reset、async invalidationをtrace。server authoritative tickとnetwork observed stateは別記録する。 | event順、metadata-before/after-spawn、teleport/remove/ID再利用/resetを将来case化。routeの各遷移と最終consumerをsource・traceで確認。 | gameplay authority、全type rendering完了。 |
| **M2 — passenger ordering / endpoints / cleanup** | 乗車関連のidentity・順序・片端未生成状態を正しく表す。 | M1依存。vehicle endpoint placeholderと未spawn passenger ID edgeを区別。ordered list/逆引き、attach/detach/reorder、spawn順・packet順・remove/re-spawnの状態遷移を定義し、physics/livingのseat/view契約と接続。cycle/duplicate拒否は現状仮定せず目標として検証。 | vehicle先行/passenger先行、passenger packet重複・順序違い、複数/多段乗車、vehicle/passenger remove・同ID再spawnをA/B各々将来比較。今は未実行。 | 全座席variant・描画fidelity。 |
| **M3 — client rendering / interpolation and effects** | network stateをpresentationへ届けるとき server tick/predictionを混同しない。 | M1/M2依存。65 typeごとproducer→frame→concrete consumer/no-visual/effect-onlyを確定。各projectileのclient display prediction、boat/cart/item/block/display payload、Displayのclient-side interpolation、time/lighting/cullingを公式26.2と比較する。server physics/fuse/impactは担当ownerへ渡す。 | helperの単体存在で完了とせず、live frame routeとfinal pipelineをsource/unit・後日実行traceで確認。Display開始遅延/途中/終端、projectile correction、variantとeffectsを記録。未実行。 | A/Bのserver runtime差、隣接owner依存。 |
| **M4 — independent A/B comparison and gap gate** | 互換主張を接続方式ごとの証拠に紐付ける。 | M0-M3および`connection-and-protocol`, `server-gameplay`, `server-world`, `singleplayer-lifecycle`, `physics`, `block-rendering`, `item-rendering`, `particles`, `audio`, `resources`との契約を待つ。AはPomme client→official dedicated、BはSteelMC singleplayer→official integratedを個別fixture・ledgerで維持。 | 各caseについてserver tick/output、wire/state trace、client interpolation/drawの責任ownerと差分を別判定。両ledgerの適用caseが個別passまたは公式根拠あるnon-applicable、unknown=0となるまで100%にしない。今は比較未実行でready判定なし。 | 新版追加には別の固定分母が必要。 |

## 完了条件と比較ケース

### 互換100%の分母

100%は**Java Edition 26.2の非living EntityType 65登録**を固定し、次の適用可能caseを先にinventory化したうえで判定する。母集合や未確定caseを分母から除外して達成率を作らない。

1. 各65 typeについて、公式class/renderer/no-visual理由、spawn data、全SynchedEntityData accessor/schema及び適法値/境界/unknown時の扱い、資産・block/item payloadを列挙する。
2. 各適法type/schemaについてspawn、初期metadata、metadata update、relative move、velocity、absolute teleport/correction、passenger attach/detach/reorder、event side effect、despawn、duplicate ID reuse、world/dimension resetを適用可能性付きで直積inventory化する。
3. 各描画/時間依存typeについてtick 0、transition前/開始/境界/終了、部分tick、可視距離/culling、light/weather/medium、variant全種を列挙。projectile subclassごとのinitial velocity、重力、drag、衝突状態、no-physics/in-ground、rotation/owner例外を公式ソースから確定する。
4. 比較方式A/Bそれぞれで適用可能な表示とserver semantic caseを分類する。client-only visual caseはserver authorityを通じた意味的同等を、server-only gameplayは該当ownerが検証し、このplanに描画test済みと誤計上しない。
5. Caseごとに静的追跡/実行比較/未実施/非適用と根拠を記録し、全適用caseが合格または公式仕様に基づく明示的非適用となり、unknown=0で初めてこのscopeの100%とする。単一の総合率や件数からの推定率は出さない。

### 将来の比較case（未実行）

- **Lifecycle/ID:** type TをID nでspawnしmetadata→teleport→remove→同じnで別type spawn、さらに遅延した旧predictionを完了させる。新generationのtransform/描画だけ残り、古いworker/effectが状態を変更しない。
- **Passenger:** boatに複数乗員、乗員spawn前passenger packet、vehicle replacement、nested vehicle、detachとdestroyの順序入れ替え。ordered listと逆引きが一致し、cycle/二重vehicle edgeが発生せず、座席/視点は公式attachmentに一致。
- **Projectile bounds:** 各classでtick 0/1、water進入前後、ground/in-ground、no-physics、server correction直前/直後、遅延update間、impact/despawn。位置/rotation/visual trailを公式26.2と比較し、client予測がdamage/impact authorityにならない。
- **Display interpolation:** update delay `<0,0,1>`、duration `0,1,N`、start前/開始/tick境界/終了、独立pos-transform duration、teleport duration、scale zero/nonuniform、quaternion edge、billboard全mode、brightness/shadow/viewRange/glow、長文/改行/opacity/style flags。公式進捗と最終transform/layout/pixelを照合。
- **Variant/payload:** boat/raft全wood×chest、全minecart payload及びspawner/furnace/TNT state、frame/glow/map/rotation、item display context、block display registry state、falling block native/conversion、TNT fuse境界、crystal beam/base、orb value/age、firework item/effect/rider、painting motive/rotation。
- **No-visual/effects:** AreaEffectCloud radius/age/color/particle, Interaction/Marker no-body semantics、LightningBolt geometry対atmospheric flash、LeashKnot、FishingHook line、EvokerFangs timing、OminousItemSpawner. Particle/audio emit countとlifecycleを各owner contractに照らす。
- 各caseで記録する値は同一公式version・resource set・tick条件を固定。比較実行前にprotocol traceが同じentity/metadataを伝えたことを確認し、transport mismatchをrenderer差と誤判定しない。

### 完了条件

- 公式EntityTypesの65 unique登録と93 living exclusionの再生成可能なinventoryが一致。
- 全65でconsumerまたは意図的no-visual/effect-only扱いを説明でき、gap候補はframe assemblyを含めて確定済み。
- metadata schema・packet order・generation lifecycle・passenger graph・async invalidationにunknownがない。
- official source-derivedのprojectile subclass数値/順序とDisplay interpolation ruleを実装/比較caseで網羅。
- AとBのscope適用可能全caseを個別記録し、server authoritative stateとclient visual stateに矛盾がない。今の静的planはこの条件を満たしたという意味ではない。

## 依存・未確認事項

- `connection-and-protocol`: 26.2 registry mapping、spawn/metadata/passenger packet ordering、unknown ID/schemaと invalid packet handling。ここが不明なままunknown typeを既知種にmappingしない。
- `physics`: common collision query、乗員座席/視点のattachment transform。こちらで別実装のcollision simulationを作らない。
- `server-gameplay`/`server-world`: projectile/vehicle/TNT/falling blockのauthoritative gameplay/world state。client predictionで代替しない。
- `block-rendering`/`item-rendering`/`resources`: block state→mesh、item stack/display context、entity resource/model lifetimeとreload契約。
- `particles`/`audio`: effectsのsequence、位置、重複/停止時の扱い。
- 現静的調査の要追加: official全EntityRenderers登録とPomme world frame assemblyの完全な対応、特に表のgap候補; SetPassengersで作るplaceholderの解消とdelete/teleport/reset全順序; metadata-before-spawn等のprotocol order; full 65 type data accessor inventory; projectile subclassごとの数値とcollision/movement順序; Display render-state更新/lerp equation全体; SteelMC/server authorityに関する比較。
- 全文書作成時点でRust source/launcher/SteelMC/lockfilesは変更しておらず、build/check/test/app起動/live comparisonは未実施。実行結果や互換率は主張しない。
