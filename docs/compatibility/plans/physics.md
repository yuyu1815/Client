# 移動物理 — 公式26.2互換計画

- **共通base:** `a12e38d9ea09d290a0b1736fe48a1cdafe49325f`（作業treeのsource基準）。
- **SteelMC:** `Client/third_party/SteelMC` の参照pin `0b1f87c36a664f08d81397e942fb1e19f6eb282b`。親Clientの `git submodule status -- third_party/SteelMC` でこのpinに一致し、親Client statusはclean（`master...origin/master`）だった。コードは読み取り専用。本planではimplementation comparisonもserver実行もしていない。pin確認だけではSteelMC挙動を証明しない。
- **公式参照版:** Minecraft Java Edition 26.2 decompiled sources。実際に読み取り確認した絶対rootは `C:/Users/yuzum/Desktop/mine_rust/minecraft-26.2-decompiled/src/net/minecraft`（worktree外・read-only）。例: `world/entity/player/Player.java:1282-1306` と `world/entity/Entity.java:758-812`。このtree/versionの再現可能なrevision hashは未確認なのでversion/path以上のprovenanceはunknown。
- **将来の比較環境:** A=Pomme client → 公式26.2 server（physicsの予測とサーバー補正を記録）。B=SteelMC singleplayer → 公式integrated server（移動予測に加えauthority差を記録）。Bのserver実装・権威は`server-gameplay`等の責務であり、本planが所有しない。両環境の比較は未実施。
- **母集合出所:** 公式`world/entity/Pose.java`, `world/entity/Entity.java`, `world/entity/LivingEntity.java`, `world/entity/player/Player.java`, `world/phys` shapes/AABB、登録済みblock/stateとblock behavior/tags、attributes/effectsおよび共有contact-effect dispatch。client側の補助照合には`Client/pomme-client/src/world/block/data/blocks-26.2.json`と`shapes-26.2.json`を使用する。属性/effect有限母集合は`world/entity/ai/attributes/Attributes.java`全40登録と`world/effect/MobEffects.java`全40登録。公式runtime block registryの完全なID集合とclient表とのjoinはまだ証明されておらず、完了分母のblock部分は未確定。

## 対象と責任境界

対象はローカルplayerの予測移動である。地上・空中移動、衝突とstep、姿勢寸法・接地・support/fall距離、sneak edge backoff、jump/sprint/climb、creative flight、water/lava/bubble、fall-flying、attribute/effectおよびblock contextによる運動変化を含む。

**状態と処理の境界:** physicsは入力frameと直近の権威player snapshot、属性/effect/equipment、loaded worldのblock/fluid/collision情報、entity collider・world borderから、tick候補としてposition/velocity/on-ground/horizontal-collision/support/fall-distance/pose/modeを計算する。`LocalPlayer`にある値はクライアント予測状態であり、server truthではない。serverからのcorrectionは基準snapshotとして適用・rebaseし、古い候補を正として上書きしない。physicsは補正packetのcodec/sequence/transportを所有せず、適用すべき意味情報をprotocolから受け取る。

- `connection-and-protocol`: packet codec/順序/transport、server correctionのdecodeとsemantic state delivery。physicsはposition/velocity/abilities等の適用契約を定義する。
- `interaction`/`inventory`: use slowdown、boots/equipment・装備変更・slot authority。physicsは確定済みsnapshotを読むだけでitem useやinventoryを変更しない。
- `block-rendering`/`server-world`: block/fluid/stateとcollision shapeの更新元・versioned data。physicsはvisual mesh/AOを使わず、world snapshot内のcollision契約を消費する。
- `server-gameplay`: server移動authority、fall damage・effect/block mutation・最終受理。physicsはクライアントの候補を出しdamage/world mutationを確定しない。
- `nonliving-entities`/`living-entities`: vehicle/passenger/horse travel authorityおよび他生体移動。physicsはlocal player移動に集中し、playerのpose dimensionsだけを扱う。生体variantのpose/animationはliving owner。
- `interaction`: outline/raycast/hit-test。本planは移動衝突shapeとの共有データ契約だけを扱い、outline差の利用側を引き取らない。

未ロードchunk、欠損block/entity collider、border未確定は「利用可能な空間」と同一視しない。入力をUnavailableとして表現し、予測を保留/停止しauthorityを待つ等、公式通信・chunk契約に矛盾しない処理をM0で決める。solidやempty shapeを推測するfallback、未送信intentの自動retry、server authorityの偽装を互換仕様として導入しない。

## 公式の機能母集合

### Pose

回収時の公式`world/entity/Pose.java:18`由来inventoryは18種。client `entity/mod.rs:184`にも同じ名称・順序があると回収draftは記録（source-confirmed exhaustive comparisonではない。姿勢遷移・寸法・利用可能性は未検証）。ID/label inventory (recovered/unverified):

| ID | Pose | local playerでの適用状況 |
|---:|---|---|
| 0 | STANDING | 適用候補。条件/寸法要照合 |
| 1 | FALL_FLYING | 適用候補。elytra条件/遷移要照合 |
| 2 | SLEEPING | player状態・復帰条件要照合 |
| 3 | SWIMMING | 適用候補。fluid/space条件要照合 |
| 4 | SPIN_ATTACK | player applicability未確認 |
| 5 | CROUCHING | 適用候補。support/space要照合 |
| 6 | LONG_JUMPING | player applicability未確認 |
| 7 | DYING | 適用候補。death tickとの条件要照合 |
| 8 | CROAKING | player applicability未確認 |
| 9 | USING_TONGUE | player applicability未確認 |
| 10 | SITTING | player applicability未確認 |
| 11 | ROARING | player applicability未確認 |
| 12 | SNIFFING | player applicability未確認 |
| 13 | EMERGING | player applicability未確認 |
| 14 | DIGGING | player applicability未確認 |
| 15 | SLIDING | player applicability未確認 |
| 16 | SHOOTING | player applicability未確認 |
| 17 | INHALING | player applicability未確認 |

clientの未知pose ID fallback（`Client/pomme-client/src/entity/mod.rs:207`）が公式codecの範囲外値処理と同じかは未確認。全poseについて公式寸法・eye height・collision clearanceとplayerへの適用性を調査する。

### 移動mode・状態・block context

公式`LivingEntity.travel`のair/fluid/fall-flying経路、player固有の`Player` travel/pose/input、`Entity.move` collision semanticsを起点に、適用条件付きの全経路を網羅する。範囲は通常地上/air、jumpとjump delay・held/fresh press、sprint開始停止/二重入力/hunger/use条件/sprint-jump、sneak/edge support、step-up/climb、姿勢変更と空間確保、water・water current・fluid stand-on・underwater上下入力、lava閾値、bubble列の上下、creative flight/vertical input/speed/着地停止、fall-flying/elytra pitch・lift/dive/turn・fall距離、dead local path。vehicle/ridden horseは除外・隣接ownerへ契約する。

#### 有限属性登録（回収時の全40名・仮分類、網羅性未確認）
Recovered inventory claims 40 official `world/entity/ai/attributes/Attributes.java` registrations; registration scan/count was not re-run here. The following recovered names are mutually exclusive provisional labels (sum 40), not a source-confirmed registry. Use/owner search and modifier operation/order review must establish relevance/exclusion in M0.

- **physics直接寄与候補 (13):** `AIR_DRAG_MODIFIER`, `BOUNCINESS`, `FLYING_SPEED`, `FRICTION_MODIFIER`, `GRAVITY`, `JUMP_STRENGTH`, `MOVEMENT_EFFICIENCY`, `MOVEMENT_SPEED`, `SAFE_FALL_DISTANCE`, `SCALE`, `SNEAKING_SPEED`, `STEP_HEIGHT`, `WATER_MOVEMENT_EFFICIENCY`。公式使用根拠の確認例: `LivingEntity.java:2083-2084` `getEntityBounciness`, `:2349` `computeModifiedFriction`/`FRICTION_MODIFIER`, `:2357-2360` `AIR_DRAG_MODIFIER`, `:553-559` `getScale`, `:3487-3488` `getDimensions`/scale適用、`:2393-2395` `DOLPHINS_GRACE`による水drag（effect）。player base値、適用poseへのscale寸法、全属性modifiersは未確定。
- **authority/state境界から間接寄与 (4):** `EXPLOSION_KNOCKBACK_RESISTANCE`, `FALL_DAMAGE_MULTIPLIER`, `KNOCKBACK_RESISTANCE`, `OXYGEN_BONUS`。impact/knockback/fallの権威的結果・survival/air状態からcandidate velocityや入力可否へ届く経路だけを調べ、実state deliveryはserver-gameplay/connection-and-protocol ownerと契約する。physicsが結果やdamageを所有しない。
- **physics外の暫定候補 (23):** `ARMOR`, `ARMOR_TOUGHNESS`, `ATTACK_DAMAGE`, `ATTACK_KNOCKBACK`, `ATTACK_SPEED`, `BELOW_NAME_DISTANCE`, `BLOCK_BREAK_SPEED`, `BLOCK_INTERACTION_RANGE`, `BURNING_TIME`, `CAMERA_DISTANCE`, `ENTITY_INTERACTION_RANGE`, `FOLLOW_RANGE`, `LUCK`, `MAX_ABSORPTION`, `MAX_HEALTH`, `MINING_EFFICIENCY`, `NAME_TAG_DISTANCE`, `SPAWN_REINFORCEMENTS_CHANCE`, `SUBMERGED_MINING_SPEED`, `SWEEPING_DAMAGE_RATIO`, `TEMPT_RANGE`, `WAYPOINT_TRANSMIT_RANGE`, `WAYPOINT_RECEIVE_RANGE`。根拠群はcombat/interaction/range/UI/AI/health/mining/waypoint用途であり、対象移動式・寸法・姿勢・collision計算に直接入力しない。該当状態がphysics input gateを変える例が見つかれば間接群へ移す。
- **未確認登録名:** 0 in recovered list; official registration completeness and all use paths are **recovered/unverified**, not source-confirmed. 13/4/23 are inventory labels, not confirmed relevance/exclusion proof; M0 must prove registration and modifiers/use/ordering. The 23 outside classifications remain tentative until their paths and applicability are evidenced.

#### 有限effect登録（回収時の全40名・仮分類、網羅性未確認）
Recovered inventory claims all 40 `world/effect/MobEffects.java` registrations (not reverified here); total 40 in recovered list, no recovered name omitted. This is not source-confirmed registration completeness.

- **physics直接寄与候補 (6):** `SPEED`, `SLOWNESS`（`MOVEMENT_SPEED` modifier）, `JUMP_BOOST`（`SAFE_FALL_DISTANCE` modifierとjump/fall pathの適用性を照合）, `LEVITATION`, `SLOW_FALLING`, `DOLPHINS_GRACE`（water drag; `LivingEntity.java:2393-2395`）。effect固有tick/attribute modifierとplayer適用条件は全件追跡中。
- **authority/state境界から間接寄与 (2):** `HUNGER`（food/sprint eligibility）、`WATER_BREATHING`（underwater survival/air state）。physicsは受信stateを消費するだけでeffect duration/food/airを所有しない。
- **physics外の暫定候補 (32):** `HASTE`, `MINING_FATIGUE`, `STRENGTH`, `INSTANT_HEALTH`, `INSTANT_DAMAGE`, `NAUSEA`, `REGENERATION`, `RESISTANCE`, `FIRE_RESISTANCE`, `INVISIBILITY`, `BLINDNESS`, `NIGHT_VISION`, `WEAKNESS`, `POISON`, `WITHER`, `HEALTH_BOOST`, `ABSORPTION`, `SATURATION`, `GLOWING`, `LUCK`, `UNLUCK`, `CONDUIT_POWER`, `BAD_OMEN`, `HERO_OF_THE_VILLAGE`, `DARKNESS`, `TRIAL_OMEN`, `RAID_OMEN`, `WIND_CHARGED`, `WEAVING`, `OOZING`, `INFESTED`, `BREATH_OF_THE_NAUTILUS`。現時点で移動式・pose/dimensions・collisionへ作用する登録hookを特定していない分類。全`MobEffect` subclass/tick/attribute hooks確認後、移動関連block contact/effect dispatchから到達するものがあれば再分類。
- **未確認登録名:** 0 in recovered list; official registration completeness and all hooks are **recovered/unverified**, not source-confirmed. 6/2/32 are inventory labels, not confirmed relevance/exclusion proof; M0 must prove registrations, subclass/tick/attribute hooks, player applicability and dispatch. The 32 outside classifications remain tentative until evidenced.

両表の登録母集合出所は上記公式Java filesの全`register`宣言。属性の同期登録可否とmob別`AttributeSupplier`/player base values、effect modifier operation、effects added/removed orderingは別途追跡し、候補名だけで実装・比較範囲を閉じない。

block母集合は公式26.2の**全runtime registry ID → Block instance/class → state/property → relevant hook/tag/context**のjoinを分母とする。`Blocks.java`宣言数はruntime registryの証明ではない。現在client JSONは1,196 ID/32,366 states、`shapes-26.2.json`は1,196 records/799 distinct shapes/16,980 state overridesだが、公式全登録へのjoinは未完了。公式の完全な登録一覧と全state definitionを確定し、client JSONは別途比較する。完了までblock分母はunknownであり、件数を互換率へ換算しない。

公式全登録を以下の適用可能カテゴリへ分類し、全state/contextを列挙する（ここに列挙するclass名は検索起点であり全対象の列挙完了を意味しない）。

| behavior category | 公式対象・確認入口（各登録stateに展開して未分類残を追う） |
|---|---|
| collision shape | empty/full/partial/dynamic、collisionとoutlineの差、entity `CollisionContext`、neighbor依存。確認例: stairs/slabs/fence/wall/gate/pane/door/trapdoor/chest/scaffolding/snow/piston、公式`BlockBehaviour.BlockStateBase`とconcrete `getCollisionShape`。 |
| offset・moving block | block offset、piston extension/retraction・moving block entity progress/方向/head、BE欠損時。`PistonMovingBlockEntity`も含む。 |
| fluid | water/lava source/flowing/falling、waterlogged各state、液面・current、bubble column、fluid collision/stand-on閾値。 |
| support/factors | 全登録stateのfriction/slipperiness、speed/jump factor、support block選択。ice variants、honey/soul sand、slime/bounce、hay/bed等fall responseを列挙。 |
| contact/step/fall | 全concrete/inherited `entityInside`, `stepOn`, `fallOn`と実際のpost-fall dispatch、shared `EntityInsideBlockEffectApplier`、火/portal/berry/cactus等 contact effects。server damage/mutationはserver-gameplay owner。 |
| special contexts | powder snow (fall/boots/descending/feet)、scaffolding(bottom/descending/distance)、web/honey/slime、climbable tags (ladder/vine/scaffolding)、soul sand、bubble、moving piston、neighbor-dependent shapes。 |
| collider set | loaded block shapes、moving entity collider、world-border・entity collider、chunk availability/境界。other entity gravity/semanticsは別owner。 |

#### 回収draft由来hook/class inventory（全件性・再現性とも未検証）
以下の数と一覧はrecovered/unverified。回収時に意図したscopeは公式rootの `world/level/block/**/*.java` recursively で、`getCollisionShape`, `entityInside`, `stepOn`, `fallOn`, `getFriction`, `getSpeedFactor`, `getJumpFactor` method declaration occurrences。実行した検索コマンド・除外条件・生成元は回収draftに保存されておらず、本作業では全scanを再実行していない。従って数字・class一覧は候補で、source-confirmed exhaustive inventoryではない。登録block ID/stateへの継承/override/tag/context joinも未確認。M0で検索を固定・再現し、全runtime registrationsへjoinする。

- `getCollisionShape` overrides/implementations (24): `BambooStalkBlock`, `BellBlock`, `BigDripleafBlock`, `CactusBlock`, `ComposterBlock`, `CrossCollisionBlock`, `FenceGateBlock`, `GrindstoneBlock`, `HoneyBlock`, `LecternBlock`, `LiquidBlock`, `MossyCarpetBlock`, `MudBlock`, `PitcherCropBlock`, `PowderSnowBlock`, `ScaffoldingBlock`, `SculkShriekerBlock`, `SnowLayerBlock`, `SoulSandBlock`, `WallBlock`, `WallHangingSignBlock`, `piston/MovingPistonBlock`, `piston/PistonMovingBlockEntity`, `state/BlockBehaviour`。
- `entityInside` (27): `BaseFireBlock`, `BasePressurePlateBlock`, `BigDripleafBlock`, `BubbleColumnBlock`, `ButtonBlock`, `CactusBlock`, `CampfireBlock`, `CropBlock`, `DetectorRailBlock`, `EndGatewayBlock`, `EndPortalBlock`, `EyeblossomBlock`, `FrogspawnBlock`, `HoneyBlock`, `HopperBlock`, `LavaCauldronBlock`, `LayeredCauldronBlock`, `LilyPadBlock`, `NetherPortalBlock`, `PitcherCropBlock`, `PowderSnowBlock`, `SweetBerryBushBlock`, `TripWireBlock`, `WebBlock`, `WitherRoseBlock`, `entity/HopperBlockEntity`, `state/BlockBehaviour`。
- `stepOn` (7): `Block`, `MagmaBlock`, `RedStoneOreBlock`, `SculkSensorBlock`, `SculkShriekerBlock`, `SlimeBlock`, `TurtleEggBlock`; `fallOn` (9): `BedBlock`, `Block`, `FarmlandBlock`, `HayBlock`, `HoneyBlock`, `PointedDripstoneBlock`, `PowderSnowBlock`, `SlimeBlock`, `TurtleEggBlock`。
- `getFriction`, `getSpeedFactor`, `getJumpFactor`: 各1 declaration、いずれも`Block.java`。state property values・block selectionとの全ID joinは未完了。
- climb/contactのtag/function入口: `BlockTags.CLIMBABLE` (`tags/BlockTags.java:107`) は公式`Entity.java:992`と`LivingEntity.java:1619`で参照。値の出所は`data/tags/VanillaBlockTagsProvider.java:83-84`。`SOUL_SPEED_BLOCKS` (`BlockTags.java:105`, data provider `:81`) のmovement enchantment pathは`world/item/enchantment/Enchantments.java:157-159`。powder snow special-caseは`Entity.java:992`ほか、boots/fall/descendingは`world/level/block/PowderSnowBlock.java:119`を追跡する。bubble contextの`ENABLES_BUBBLE_COLUMN_DRAG_DOWN`/`...PUSH_UP`は`BlockTags.java:198-199`、data membership `VanillaBlockTagsProvider.java:172-173`、処理`world/level/block/BubbleColumnBlock.java:113-123`。tag membersと全適用contextのsemantic照合は未完了。
- contact tag scanでmovement-relevantと判定した他の参照群: `SnowLayerBlock`のsnow support/override tags、`Block`の`UNSTABLE_BOTTOM_CENTER`、`WallBlock`の`WALLS`/`WALL_POST_OVERRIDE`、`FenceGateBlock`の`WALLS`。これらはsupport/neighbor-state computation用。scan上の他BlockTags参照（たとえば crop support、beehive、campfire等）は別機能条件に分類し、移動寄与性を未確認のままphysicsへ取り込まない。

公式hook検索での具体クラス・tag入口は上記の通り列挙済み。次に公式全runtime registry ID/stateと各classのinheritance/override/tag/contextへjoinし、client table/algorithmの対象・例外・未調査を照合する。登録ID/stateのuncategorized残数ゼロをM0完了条件とし、未結合・適用関係不明はunknownのままblock分母に残す。

## 現行実装と差分

以下はsource上の具体経路と公式側の対照点であり、**静的差分候補であって実動作の非互換確定ではない**。lineは各source tree基準の1始まり。公式側は`minecraft-26.2-decompiled/src/net/minecraft/`以下。

| # | 現行client: path:line:symbol | 公式26.2: path:line:symbol | 照合課題 |
|---:|---|---|---|
| 1 | `Client/pomme-client/src/physics/movement.rs:218` `travel` | `world/entity/LivingEntity.java:2309` `travel(Vec3)`; fluid stand-on gate `world/entity/LivingEntity.java:2290` `canStandOnFluid(FluidState)` | clientのwater/lava/fall-flying/else-land分岐と公式fluid stand-on・vehicle・fluid effect条件を比較。player overrideまで追う。 |
| 2 | `Client/pomme-client/src/physics/movement.rs:387` `jump_from_ground` | `world/entity/LivingEntity.java:2264` `jumpFromGround` | jump power/attribute、existing vertical velocityのmax、sprint impulse、Jump Boost modifierとsafe-fall属性を照合。 |
| 3 | `Client/pomme-client/src/physics/movement.rs:977` `levitation_travel_y_delta` | `world/entity/LivingEntity.java:2347` `travelInAir` | Levitation/Slow Falling/gravity経路と全登録modifier・effect hookの網羅性を確認。 |
| 4 | `Client/pomme-client/src/physics/collision.rs:29` `collision_cell` | `world/level/block/piston/PistonMovingBlockEntity.java:325` `getCollisionShape` | clientのBE state/progressからのshape再構成と公式方向/進退/進行率/欠損時を比較。 |
| 5 | `Client/pomme-client/src/physics/collision.rs:62` `visit_block_aabbs_bounded`（powder-snow分岐`:101`） | `world/level/block/PowderSnowBlock.java:119` `getCollisionShape` | feet/fall/descending/leather-boots閾値、collision/climb/slow-fallの適用順を照合。 |
| 6 | `Client/pomme-client/src/physics/collision.rs:62` `visit_block_aabbs_bounded`（scaffolding分岐`:117`） | `world/level/block/ScaffoldingBlock.java:130` `getCollisionShape` | bottom/distance/descending/feet条件とcollision・climb判定を全stateで比較。 |
| 7 | `Client/pomme-client/src/physics/movement.rs:1200` `back_off_from_edge` | `world/entity/Entity.java:783` `move`→`maybeBackOffFromEdge`; base `Entity.java:1084`, player override `world/entity/player/Player.java:843` `maybeBackOffFromEdge` | partial support・diagonal・collider・stepの順序、epsilonと可動範囲を比較。 |
| 8 | `Client/pomme-client/src/physics/movement.rs:1469` `block_movement_factor` | `world/level/block/Block.java:452` `getFriction` (`:456 getSpeedFactor`, `:460 getJumpFactor`) | clientのselected hard-coded factor/fallbackと公式state property、support selection・modifierを登録全stateで照合。 |
| 9 | `Client/pomme-client/src/physics/movement.rs:681` `apply_collision` | `world/level/block/state/BlockBehaviour.java:367` `entityInside`、`world/level/block/Block.java:444` `fallOn`、`world/entity/Entity.java:1490` `checkFallDamage` | collision clipだけでなくentity-inside/step/fall・bounceの呼出順を追う。fall damage/actionはserver owner、client predictionとの意味差を分離。 |
| 10 | `Client/pomme-client/src/physics/aabb.rs:7` `EPSILON` / `Client/pomme-client/src/physics/aabb.rs:118` `clip_axis` | `world/phys/AABB.java:24` `EPSILON` と`world/phys/shapes/VoxelShape.java:240` `collideX` | half-open境界、axis order、float→double変換、epsilonとJava側shape clippingを境界値で比較。 |

### Current production path (verified client call order)

`Client/pomme-client/src/app/core.rs:7394-7911` `tick_physics` is the caller; its live movement/send portion is `:7590-7839`. Order differs for dead and live player:

- **Not loaded:** world/other-entity handling precedes the guard; `client_loaded == false` clears queued presses and returns without player physics/input/position sends.
- **Dead:** loaded player snapshots transform; `tick_death` and dead interaction state run; until removal, `movement::tick_dead_with_context` runs with current chunk/entity/border inputs, then walk animation/bob; neutral abilities, input and position sends follow. Finished death returns before this tail. This is not the normal movement tick.
- **Normal live tick:** consume queued key actions; form input; ride jump charging; camera look update; airborne elytra-start request; update interaction target and tick interaction; tick other entities; collect entity AABBs and use-speed inputs; choose `tick_ridden_horse` or `movement::tick_with_context`; update walk/bob; send abilities, input and position, then record movement trace. Thus interaction/remote entities precede movement, while packets follow it. See `core.rs:7394-7911`, `:7470-7525`, `:7590-7839` (`tick_physics`, dead branch, interactions, vehicle selection and sends); these are client production order claims, not official/server order.

Inside `pomme-client/src/physics/movement.rs:107-215`, the currently verified sequence is: reset travel observation; decrement jump delay; update water state; apply fluid currents; reset fall-distance tick state; crouch state; eye height; compute float movement input with use/sneak factors; update sprint; update flight; apply flight vertical input; zero small velocity; climb jump; fluid/ground jump; `travel`; update pose; air supply; stop flight on ground; save forward/jump pressed edges. `travel` selects water, lava, fall-flying, then ordinary land/air paths (`movement.rs:217-278`). It does **not** establish that these branches match Player's overrides. `tick_dead_with_context` is separately at `movement.rs:295-337`.

State/consumer anchors: `player/mod.rs:145-240` `LocalPlayer`; `:584-625` pose dimensions; `:723-791` water/eye-fluid state. `collision.rs:62-151` block AABB collection including context-special shapes; `:212-242` support; `:287-342` collision axis resolution; `:462-533` aggregate collision/step candidates. `movement.rs:483-608` water, glide and lava routines; `:728-751` web/powder/climb effects; `:825-861` honey/slime; `:890-938` bubble; `:1120-1165` pose; `:1167-1235` support and edge backoff; `:1283-1398` fluid current; `:1469-1515` movement factors. The client also has registered block/state and shape data; helper/table existence is not proof of exhaustive runtime block-state/context joins.

Correction ingress is partially but directly verified: `net/handler.rs:644-652` maps the player-position packet to a network event; `core.rs:4334-4348` dispatches it to `apply_player_correction` (`:2863-2905`). Non-passenger correction resolves relative/absolute position, velocity delta and look against current state and separately previous position/look, applies position/previous position/velocity/look/previous look, then sends teleport acceptance and position/look echo; event handling clears pending command-block interaction, calls teleport interaction handling and moves chunk center/camera (`core.rs:2863-2905`, `:4334-4365`). Passenger path skips local player transform correction but still acknowledges/echoes. This is ingress/side-effect evidence, not proof of correct prediction reconciliation. Normal live output call is `core.rs:7839`; sender body `:7957-8029` selects mounted movement or position/rotation/status-only packets from deltas, cadence and on-ground/horizontal-collision flags, then updates send baselines. The implementation resolves relative/absolute fields and emits teleport acknowledgment plus position/look echo, but semantic agreement with official behavior is unverified. Event queue order at tick boundary, ordering with standalone velocity/abilities/attributes/effects updates, and correction impact on support/fall/pose/last-send baseline remain unknown. Record ack/echo ordering and compare all relative flags in M0/M1 traces.

### Official source comparison (static reference only)

Read-only official root: `C:/Users/yuzum/Desktop/mine_rust/minecraft-26.2-decompiled/src/net/minecraft`; source revision hash not established. Verified `world/entity/player/Player.java:1282-1306` `travel(Vec3)`, swim view-direction vertical acceleration and its conditions, flight `super.travel` followed by Y velocity damping `*0.6`, and `canGlide` denial while flying. `world/entity/player/Player.java:1311-1318` further gates swimming while flying. Verified `world/entity/Entity.java:758-812` `move`: no-physics early return; piston clamp; edge backoff then collision; position/resolved movement; horizontal/vertical collisions, on-ground/support and fall checks; restitution on collision; block speed factor. `Entity.java:783` names `maybeBackOffFromEdge` and player override is `world/entity/player/Player.java:843`; `world/phys/AABB.java:24` defines `EPSILON`; `world/phys/shapes/VoxelShape.java:237-240` clips X. These samples do not settle all axis/step order or all geometry cases.

Client water path (`movement.rs:483-547`) adds movement, when `swimming` steers vertical velocity toward look Y, then resolves collision, applies horizontal/vertical drag, fluid-fall adjustment and flight-Y overwrite. Compared with official Player override, its swimming acceleration conditions and flying vertical-damping interaction are not established as equivalent; source existence is confirmed, parity is **unverified**. Client glide (`:549-609`) computes pitch/lift/dive/turn, damps velocity and collides; official Player forbids glide while flying. Need prove eligibility/transition constraints at caller/state level; do not mark the client path unimplemented or confirmed parity.

Fluid implementation is present: `player/mod.rs:723-791` calculates water/lava height/eye state; `movement.rs:1283-1398` samples fluid cells, upper same-fluid surface, flowing heights, horizontal neighbors, empty/non-motion neighbors and below neighbors, falling-flow obstacles, average/current thresholds and impulses. These facts establish candidate code, not semantic parity. M0/M2 must compare official `Entity` fluid-height and `EntityFluidInteraction`/`FlowingFluid.getFlow` paths for source/flowing/falling, upper neighbor, feet/eye/submersion, water/lava/bubble, current up/down, obstacles, stand-on-fluid gates, water efficiency/Dolphin's Grace, swim eligibility, and unloaded/unknown cells. No unloaded fallback may silently act as empty or solid.

Collision and post-collision comparison must cover AABB precision/epsilon, axis ordering, loaded block + moving-block/entity + world-border aggregation, every step candidate and tie choice, partial/diagonal support and ground/fall state, crouch edge backoff, vertical restitution/bounce and velocity zeroing, speed/jump/friction factors, and dispatch/order for inside-block, stepOn, fallOn and fluid effects. Source anchors include official `Entity.java:758-812`, `world/level/block/Block.java:444-460` (`fallOn`, friction/speed/jump factors), `world/level/block/state/BlockBehaviour.java:367` (`entityInside`), and client `collision.rs:62-151,212-242,287-342,462-533`. Special cases to join through registry/class/state/inheritance/tag/context include moving piston; powder snow + boots/pose/fall; scaffolding + sneak/from-above; web; ladder/vine/trapdoor/climb; honey/slime; bubble drag; soul speed. These are examples, not the complete block denominator. Fire/portal/berry/cactus contact health/status/transfer is a cross-owner **authority contract**; geometry/displacement/velocity stay in physics where applicable, while damage/status/world transfer belongs with server-gameplay and relevant owners.

Horse fluid is not player water evidence: `physics/horse.rs:32-59` defines the horse routine/context and its `UnsupportedFluid` result is exercised at `:506-526`; production caller `core.rs:2623-2699` handles that result by zeroing horse velocity and pending jump, holding position, and syncing transform. This is an adjacent vehicle-owner contract only, neither player fluid parity nor SteelMC guarantee.

## 互換にする設計と条件母集合の閉じ方

Physics computes a **local predicted player candidate**, not server truth: from one tick's input frame, last authoritative baseline, accepted abilities/effects/attributes/equipment, and versioned loaded world/fluid/collider/border context, derive requested and resolved movement plus pose/support/fall/send intent. Ingress, acceptance, damage and world mutation remain owned by protocol/server-gameplay/world/interaction as applicable. An unavailable chunk/collider is an explicit unknown input, never guessed empty/solid.

M0 must reconstruct official 26.2's actual Player → LivingEntity → Entity call order and ingress/output edges; do not impose a generic travel ordering. At minimum follow `Player.travel` override: swim view-Y acceleration with its jumping/fluid-above conditions; flight calls `super.travel` then damps vertical movement by 0.6; `canGlide` excludes flying; `updateSwimming` excludes flying. Compare each directly with current client water/lava/fall-flying paths and eligibility, recording confirmed code facts separately from unverified semantic equivalence. For movement, preserve Java double position/AABB and float input/attributes boundaries, operation order, casts, epsilon, thresholds, clamps, candidate ties and protocol quantization as separate specifications.

### Executable inventory contract (no naive Cartesian product)

Keep a registry-derived inventory with stable case IDs, provenance (official class/method/registration/state/tag and client consumer), applicability predicate, owner, relevant context and result. Join **every** official runtime block ID → instance/class/inheritance → registered state/properties → override/inherited hook → tags/context; join separately to client ID/state/data. Partition the universe into: (a) applicable behavior cases and required interactions, (b) impossible cases with official evidence, (c) tentative outside-scope rows with evidence still required, and (d) unknown/unjoined rows. Case generation must produce reproducible partitions, not an unbounded ID×pose×effect cross product. Shared behavior can be represented once with member IDs/properties and explicit exceptions; no row vanishes merely because a sample passed.

Required axes and interaction rules: player pose applicability/transitions and dimensions; game modes/abilities; digital/analog/diagonal input and sneak/sprint/jump/use; riding/sleep/dead/noPhysics/flight/glide; attributes/effects/modifiers and operation/application order; fluid kind/source/flowing/falling/height/upper-neighbor/feet/eye/submersion/current/obstacle/stand-on regimes; stateful and contextual block shapes/contact; support/step/collider source (block/entity/moving piston/border); loaded/unloaded and coordinate/world/chunk boundaries; update timing, packet/event ingress, correction/rebase and send ordering. Pose rows marked player-inapplicable require official call-site/transition proof, not an assumption from enum name. The 23 attribute and 32 effect tentative-outside rows require registration, full modifier/tick/subclass/use-path searches, and evidence that no applicable physics input, player gate, dimension, collision, or contact dispatch consumes them. Counts 18/40/40 and 13/4/23, 6/2/32 are inventories/classifications only, not behavioral closure.

**Recovered inventories and diagnostics are not official exhaustive proof.** The 18 poses, 40 attribute names, 40 effect names and category assignments above are retained as recovered/unverified until source scan/registration joins are reproduced. The client JSON diagnostics (blocks 1,196 IDs / 32,366 states; shapes 1,196 records / 799 distinct shapes / 16,980 state overrides) describe current client data only, never official denominator or coverage. Recovered hook scans (including their counts below) are likewise candidate inventories until scan command/scope, inherited dispatch and registry joins are reproducible. Do not calculate parity percentages or completion from any of these counts.

Prediction and authority need separate traces, with three roles kept distinct:
- **Local prediction:** record Pomme input/pre-state and requested/resolved candidate before server response. Compare this candidate against the fixed-version official Java client oracle; do not treat it as server truth.
- **Java authority (networked and integrated):** in networked A, record outgoing intent and the official Java server's accepted state/correction; separately use the official Java integrated server as the authority oracle for a local singleplayer trace. These authority results do not establish local candidate parity by themselves.
- **SteelMC authority:** in SteelMC singleplayer, record Pomme candidate separately from SteelMC accepted/corrected state and compare server behavior against the official Java integrated server using the same case IDs. SteelMC authority implementation parity remains unknown until the pinned clean tree is actually source-compared and traces run. A clean matching gitlink does not prove semantics.
Correction handler→event→core application and relative-field resolution plus teleport ack/echo sends are present; official semantic parity, exact event/tick sequencing and ordering with standalone velocity/abilities/attributes/effects remain unknown until source-compared. Trace effects on support/fall/pose and send baseline. Horse `UnsupportedFluid` is adjacent vehicle-owner contract, not evidence for A, B or player fluid.

Contact health/status/portal transfer is an explicit cross-owner authority interface: physics owns applicable displacement/velocity/geometry; server-gameplay owns accepted damage/status/world transfer; protocol/interaction owners own delivery/action semantics. Specify event identity, timing, input state and correction observable without folding all behavior into physics.

## 実装順序

| Milestone | 目的・依存・scope | 許可される現在check / 将来の比較条件 | 100%到達時に残してはならないもの |
|---|---|---|---|
| M0 — source-confirmed denominator, applicability and ingress/order contracts | Prerequisite for M1–M3: reproduce official registries, state/class/inheritance/tag/context joins; verify Pose/mode/effect/attribute registration and tentative exclusions; establish Player tick call order, correction/ability/effect/equipment ingress and output owner contracts; define unavailable-world behavior and case-ID/provenance format. | **This plan:** read-only static spot checks only. **Future:** reproducible exhaustive joins and dependency-owner traces. | runtime block ID/state or source-scope gap; unproved pose applicability or tentative exclusion; unknown update/correction ordering; absent case partition/provenance; unavailable-input semantics. |
| M1 — correction, collision, support and post-collision safety | Depends on M0 contracts. Rebase/correction baseline, unavailable geometry, block/entity/border aggregation, axis/step/tie, edge backoff, support/ground/fall and restitution/velocity clipping. | **This plan:** static ingress/caller references only. **Future:** traces around absolute/relative teleport, teleport ack/echo, velocity/ability changes, unloaded boundaries, step/edge/support and post-collision contact in A and B. | stale prediction treated as authority; guessed geometry; unresolved on-ground/support/fall or restitution ordering; correction-to-send baseline ambiguity. |
| M2 — player travel, fluids, flight/glide and modifiers | Depends on M0 and M1 safety contracts. Compare Player override, land/air, sprint/jump/climb, water/lava/bubble, swim eligibility, flight vertical damping, glide constraints, fluid currents, direct/indirect attributes/effects and numerical order. | **This plan:** source facts and static candidates, no parity result. **Future:** threshold/current/obstacle, water efficiency/Dolphin's Grace, vertical flight, glide pitch/dive/turn and modifier traces in A and B. | unresolved travel branch/modifier/threshold; difference or missing applicable case; unrun required oracle traces. |
| M3 — pose, block registry joins and contextual dispatch | Depends on M0 denominator, can proceed alongside M1/M2 implementation after interfaces are fixed. Resolve pose applicability/dimensions and every registered state/context through inheritance/tag/contact/factor joins; listed special blocks are seeds, not denominator. | **This plan:** recovered hook scan only. **Future:** reproducible full registry join and partial/full/empty, neighbors, pose clearance, powder snow/equipment, scaffolding/sneak, piston progress, honey/slime/web/climb, bubble/soul speed and contact traces. | any unjoined registry/state/hook/tag/context; any unknown applicability or missed inherited dispatch. |
| M4 — timing, numerics and complete differential evidence | Depends on M0 and completed M1–M3. Close official tick/event ordering, float/double operations, epsilon/tolerances and protocol quantization; execute and retain local-prediction and authoritative traces separately for every supported path. | **This plan:** docs/source checks only. **Future:** same versioned case IDs/input/baseline in local prediction, official Java server/integrated authority and SteelMC authority runs, with candidate, resolved state, send, accepted state and correction compared at fixed, justified tolerance. | any open denominator, unknown/unclassified case, missing applicable case, failed difference or missing separate authority trace. No runtime result may be inferred from static checks. |

## 完了条件と比較ケース

**100%の定義 (target, not current status):** pin Java Edition 26.2; first close M0's source-confirmed universe and applicable interactions. Every applicable case has stable ID/provenance, applicability evidence, owner, input/pre-state/context and fixed-version result; every impossible case has official proof; outside rows have proven owner/exclusion; unknown/unclassified count is zero. Compare and resolve every applicable case at documented numerical tolerance and retain separate Pomme local-candidate parity, official Java network/integrated authority traces, and SteelMC authority traces for each supported path. A percentage cannot be calculated while denominator is unknown. All 18 poses remain inventoried; a pose may be marked player-inapplicable only with official use/transition evidence and still gets a representation/decoding contract case if network/state encoding reaches it. Likewise each tentative outside attribute/effect classification needs exhaustive source-backed non-use or a different owner path. The 1,196 client block IDs/32,366 states and shape diagnostics are not the official denominator. No 18×40×40 blind cross product; generate only applicable interactions and justify impossible partitions. Source presence, recovered scan counts, static checks, and representative examples do not establish 100%.

Future case matrix records input/pre-state, attribute/effect/equipment, world/block/fluid context, requested/resolved delta, per-tick candidate, send intent, authoritative response/correction and next baseline. Include:

- land/air: flat ground、斜め入力正規化、walk/sprint、jump押下edge/held/delay、sprint-jump、horizontal/vertical wall clip、step-upの両candidate、落下/着地。
- pose/edge: crouch toggleとledge support、partial/diagonal support、pose寸法/eye heightと天井clearance、water swim enter/exit、姿勢変更拒否/authority update。
- fluid/mode: water surface threshold/current/stand-on、submerged up/down、lava depth threshold、bubble up/down、climb contact、creative flight enable/disable/ground stop。
- elytra/effect: pitch上向き/水平/下向き境界、lift/dive/turnとcollide、fall distance cap、Slow Falling/Levitation、Speed/Slowness/Jump Boost、各関連attribute modifier上限・下限。
- blocks/collider: all registered state shapes、neighbor forms、ice friction、soul speed、honey/slime/web/snow/scaffold/powder snow、bubble, piston progress/direction/head, offset, fall/step/contact; entity and world-border collider; loaded/unloaded chunk boundary。
- authority/timing: absolute/relative teleport・velocity/ability/effect correctionがmovement tick前後に来る系列、knockback、correction後のsupport/fall/pose/send baseline、ground flags、input-to-send cadence。server authoritative fall damage/block actionはsemantic boundaryで比較しphysicsのclient予測値と混同しない。

AとBを別のoracle結果として保持し、同じphysics期待でもBのSteelMC authority差を切り分ける。数値許容値はJava演算精度とprotocol encode/decode quantizationの根拠から定め、未計測の許容幅を緩めない。現段階ではsource static evidenceとplanのみで、上記比較を実行していない。

## 依存・未確認事項

- `connection-and-protocol`: inbound player position correctionの相対flag・teleport acknowledgment/echo、velocity/abilities/attributes/effectsのtyped payload・delivery order。確認済みの一部は`Client/pomme-client/src/net/handler.rs:644-652` → `Client/pomme-client/src/app/core.rs:4334-4348` → `:2863-2905` `apply_player_correction`; tick境界・全更新種別・rebase状態は未確認。
- `server-world`/`block-rendering`: block/fluid registry snapshot、collision shapeとstate update、chunk unavailable/world-border/entity-collider契約。render meshとは分離する。
- `interaction`/`inventory`: active use slowdown、boots/equipment/effect/attribute更新の適用時点。
- `nonliving-entities`/`living-entities`: vehicle/horse/passenger移動・jump、playerと他生体pose/travel境界。horse water/lavaは現client `UnsupportedFluid`だが公式との意味比較未完。
- `server-gameplay`: authority受理、fall damage/block effect、movement reject/correction方針。client predictionのみを一致判定に使わない。
- 公式runtime block registry件数/IDとclient JSON join、全official state/property mapping、全override/inherited behaviorとshared effect dispatch、全effect/attribute modifierの移動影響、全poseのlocal applicability、player-specific official call orderの完遂、collision unloaded入力、correction時のsupport/fall/pose/last-send整合が未確認。Hook/class inventory counts are recovered/unverified and are not source-confirmed exhaustive counts.
- source経路・定数の静的存在は振る舞い一致の証拠ではない。追加調査・将来oracle比較の完了まで互換達成や互換率を主張しない。
