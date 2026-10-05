# Java 26.2 entity/projectile client particle evidence

> **Historical snapshot:** the source-by-source `未対応` / `部分実装` labels below describe an earlier checkpoint and are not current status. Animal, mob, phase, boss, misc, common-contact and projectile source paths were implemented afterward. Use [`particle-java-callsite-source-audit-26.2.md`](particle-java-callsite-source-audit-26.2.md), its current CSV recheck columns, and [`particle-integration-verification.md`](particle-integration-verification.md) for latest status and tests. This table is retained as early investigation history, not as an open-gap list.

共通 `Entity` / `LivingEntity` helperの条件・要求数は [`particle-entity-common-evidence.md`](particle-entity-common-evidence.md) を参照。対象は `Client/docs/plans/particle-java-callsite-inventory-26.2.csv` の entity/projectile 53 source と混在 6 source（計59）。Java原本は `minecraft-26.2-decompiled/src/net/minecraft/world/entity/**`。呼び出しは各 `java_line_evidence` の行を参照し、Rust側は `pomme-client/src/entity/mod.rs::EntityStore::client_particle_requests` と `app/phases/in_game.rs` のrequest dispatch。

## 実装したclient tick

- living: Blaze (`Blaze.java:101`) から2 LARGE_SMOKE/tick、GlowSquid (`GlowSquid.java:97`) からGLOW/tick、Phantom (`Phantom.java:131–132`) から寸法と同期yawに基づくMYCELIUM 2/tick。
- vehicle/projectile: PrimedTnt (`PrimedTnt.java:131`) fuse中のSMOKE、ShulkerBullet (`ShulkerBullet.java:224`)のEND_ROD。Arrow/ThrowableProjectile/AbstractHurtingProjectileの水中BUBBLE、Arrowの同期色EFFECT、SpectralArrowのEFFECT spell。明示サーバー送信や衝突時生成は再発生させない。
- water判定は現在のprojectile座標のChunkStore fluid state、vehicle位置・速度は既存同期/予測状態を使用。炎/生物のランダム位置はentity dimensions/BBox相当の幅・高さから決める。ParticleStoreへ既存 `ParticleSpawnRequest` だけを渡す。

この範囲は**部分対応**であり、entity callsite全体完了ではない。Java tickに必要な未同期AI/phase/target/rotation dataを推測で代用しない。FireworkRocketEntityの同期item component (shape/colors/fade/trail/twinkle) provider stateも未実装。ItemPickup/interactionおよびhandler event 53/54/60/67は別担当。

## 全59 source ledger

| Java source | Evidence lines | Status | Boundary / missing |
|---|---:|---|---|
| `net/minecraft/world/entity/AgeableMob.java` | 197:addParticle, 221:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/AreaEffectCloud.java` | 196:addAlwaysVisibleParticle, 199:addAlwaysVisibleParticle, 203:addAlwaysVisibleParticle, 206:addAlwaysVisibleParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/Entity.java` | 1608:addParticle, 1615:addParticle, 1648:addParticle, 2754:sendParticles, 2755:sendParticles | server sendParticlesのみ（混在callsiteあり） | sendParticles lines remain native packet path; each direct client addParticle is still pending (see Java evidence). |
| `net/minecraft/world/entity/LivingEntity.java` | 397:sendParticles, 854:addParticle, 1956:addParticle, 2031:addParticle, 2041:addParticle, 3324:addParticle | server sendParticlesのみ（混在callsiteあり） | sendParticles lines remain native packet path; each direct client addParticle is still pending (see Java evidence). |
| `net/minecraft/world/entity/OminousItemSpawner.java` | 76:addParticle, 157:addParticle, 164:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/TamableAnimal.java` | 112:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/animal/Animal.java` | 85:addParticle, 250:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/animal/allay/Allay.java` | 483:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/animal/bee/Bee.java` | 277:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/animal/camel/Camel.java` | 418:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/animal/cow/MushroomCow.java` | 147:addParticle, 153:addParticle, 167:sendParticles | server sendParticlesのみ（混在callsiteあり） | sendParticles lines remain native packet path; each direct client addParticle is still pending (see Java evidence). |
| `net/minecraft/world/entity/animal/dolphin/Dolphin.java` | 259:addParticle, 260:addParticle, 268:addParticle, 274:addParticle, 279:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/animal/equine/AbstractHorse.java` | 460:addParticle, 872:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/animal/equine/Llama.java` | 214:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/animal/feline/Ocelot.java` | 212:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/animal/fox/Fox.java` | 270:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/animal/frog/Tadpole.java` | 219:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/animal/nautilus/AbstractNautilus.java` | 289:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/animal/panda/Panda.java` | 422:addParticle, 480:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/animal/sniffer/Sniffer.java` | 311:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/animal/squid/GlowSquid.java` | 97:addParticle | 部分実装 | 1 GLOW/tick |
| `net/minecraft/world/entity/animal/squid/Squid.java` | 195:sendParticles, 318:addParticle | server sendParticlesのみ（混在callsiteあり） | sendParticles lines remain native packet path; each direct client addParticle is still pending (see Java evidence). |
| `net/minecraft/world/entity/animal/wolf/Wolf.java` | 366:addParticle, 431:sendParticles | server sendParticlesのみ（混在callsiteあり） | sendParticles lines remain native packet path; each direct client addParticle is still pending (see Java evidence). |
| `net/minecraft/world/entity/boss/enderdragon/EnderDragon.java` | 179:addParticle, 476:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/boss/enderdragon/phases/DragonDeathPhase.java` | 35:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/boss/enderdragon/phases/DragonLandingPhase.java` | 42:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/boss/enderdragon/phases/DragonSittingFlamingPhase.java` | 50:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/boss/wither/WitherBoss.java` | 215:addParticle, 217:addParticle, 222:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/item/PrimedTnt.java` | 131:addParticle | 部分実装 | SMOKE at y+0.5 while fuse positive |
| `net/minecraft/world/entity/monster/Blaze.java` | 101:addParticle | 部分実装 | 2 LARGE_SMOKE/tick |
| `net/minecraft/world/entity/monster/EnderMan.java` | 222:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/monster/Endermite.java` | 115:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/monster/Guardian.java` | 211:addParticle, 232:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/monster/Phantom.java` | 131:addParticle, 132:addParticle | 部分実装 | paired MYCELIUM/tick |
| `net/minecraft/world/entity/monster/Ravager.java` | 189:addParticle, 273:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/monster/Witch.java` | 173:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/monster/breeze/Breeze.java` | 167:addParticle, 183:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/monster/cubemob/AbstractCubeMob.java` | 143:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/monster/illager/Illusioner.java` | 118:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/monster/illager/SpellcasterIllager.java` | 110:addParticle, 111:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/monster/warden/Warden.java` | 345:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/npc/villager/AbstractVillager.java` | 215:addParticle, 220:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/npc/villager/Villager.java` | 659:addParticle, 661:addParticle, 663:addParticle, 665:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/projectile/EvokerFangs.java` | 91:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/projectile/EyeOfEnder.java` | 117:addParticle, 120:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/projectile/FireworkRocketEntity.java` | 168:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/projectile/LlamaSpit.java` | 106:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/projectile/ShulkerBullet.java` | 224:addParticle, 297:sendParticles, 325:sendParticles | 部分実装 | END_ROD trail; server sendParticles omitted from local path |
| `net/minecraft/world/entity/projectile/ThrowableProjectile.java` | 71:addParticle | 部分実装 | underwater BUBBLE for supported kinds |
| `net/minecraft/world/entity/projectile/arrow/AbstractArrow.java` | 215:addParticle, 288:addParticle | 部分実装 | underwater BUBBLE only; critical-hit burst missing |
| `net/minecraft/world/entity/projectile/arrow/Arrow.java` | 105:addParticle, 136:addParticle | 部分実装 | water BUBBLE / synced tipped-color aura; pickup/hit flows separate |
| `net/minecraft/world/entity/projectile/arrow/SpectralArrow.java` | 45:addParticle | 部分実装 | underwater BUBBLE and EFFECT spell aura |
| `net/minecraft/world/entity/projectile/hurtingprojectile/AbstractHurtingProjectile.java` | 102:addParticle, 115:addParticle | 部分実装 | underwater BUBBLE; per-kind trails missing |
| `net/minecraft/world/entity/projectile/throwableitemprojectile/Snowball.java` | 52:addParticle | 部分実装 | underwater BUBBLE only; impact burst is event/packet path |
| `net/minecraft/world/entity/projectile/throwableitemprojectile/ThrownEgg.java` | 47:addParticle | 部分実装 | underwater BUBBLE only; impact burst is event/packet path |
| `net/minecraft/world/entity/projectile/throwableitemprojectile/ThrownEnderpearl.java` | 111:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/vehicle/boat/AbstractBoat.java` | 168:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/vehicle/minecart/MinecartFurnace.java` | 67:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |
| `net/minecraft/world/entity/vehicle/minecart/MinecartTNT.java` | 63:addParticle | 未対応 | client-local tick/emission not implemented; requires exact synchronized state or a separate event/interaction owner. |

## 未保持状態・注意

- `FireworkRocketEntity` item componentのshape/colors/fade/trail/twinkleを読む同期状態は別担当で保持中かを未確認。正確な描画には optionsだけでなくprovider初期化状態を渡すAPIが必要。
- Wither head rotations/phase、Dragon phase/parts、Warden roar/ground target、Villager/Animal loveやgossip等の状態は、この差分では追加していない。
- `Entity.java` / `LivingEntity.java` の水面/sprint/block state transitions、effect color、fall/landing等の共通呼び出しは未実装（状態/正しいposition geometryが必要）。
- server `sendParticles` rows, including e.g. ShulkerBullet, Squid, MushroomCow, Wolf and LivingEntity lines remain packet-owned; no entity-local duplicate was added.

## Checks

- Added `synchronized_entity_client_particle_ticks_emit_vanilla_species_trails_once` and `projectile_client_ticks_emit_spectral_and_shulker_bullet_trails` fixtures, including synced Arrow color and a no-event-double-spawn assertion.
- `cd Client && mise run check` exited **101**. The edited `entity/mod.rs` and in_game dispatch had no remaining reported errors in the latest check. That workspace snapshot was blocked by concurrent incomplete `particle/terrain_extra.rs` symbols (`State`, `supports`, `spawn`, `tick`, `appearance`), plus `app/core.rs` (`Option<&str>::copied`) and `net/handler.rs` (ambient NBT type mismatch). Earlier snapshots reported different incomplete particle modules; these files are concurrently changing. Initial entity type errors in Phantom coordinate math were fixed here.
- `cd Client && mise run test` exited **101** before running tests. Its latest compile encountered many unresolved names in concurrently changing `particle/terrain_extra.rs` (including `ServerParticleKind`, `ServerParticleOptions`, `AtlasUVMap`, `DVec3`, `Particle`, and `State`). Added entity unit tests have therefore not executed.
