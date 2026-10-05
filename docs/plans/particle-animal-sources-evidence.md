# Java 26.2 animal/entity client-particle source evidence

対象は Java source callsite を caller / client-server 条件ごとに分類した記録。Java 原本は `../../minecraft-26.2-decompiled/src/net/minecraft/world/entity/`。旧 handoff の推測を根拠にせず source を再確認。`sendParticles` と既存 EntityEvent 経路は別 owner を維持し、client tick で重複しない。

## Client tick implementation

| Callsite | 実際の caller / 条件 | Rust implementation |
|---|---|---|
| `AgeableMob.java:197` | `aiStep` client path。`ageUp(..., true)` で始まった `forcedAgeTimer > 0` を4 tickごとに happy-villager。 | `tick_ageable`; 成功interaction後に `start_age_up_particle_timer` を呼ぶ。Baby状態だけでは開始しない。 |
| `AgeableMob.java:221` | `makeAgeLockedParticle`。interactionでtimer=40、偶数残数だけclient spawn。age-lockedなら `PAUSE_MOB_GROWTH`、解除なら `RESET_MOB_GROWTH`。 | `tick_ageable`; `start_age_lock_particle_timer` は成功 Golden Dandelion interaction後の入口。Metadata index17は `LivingEntity.age_locked`。 |
| `Bee.java:268–277` | `Bee.tick`: `hasNectar && getCropsGrownSincePollination()<10 && random.nextFloat()<.05f`; count=`nextInt(2)+1`; x/z を各 `[-.3,+.3)`、y=`getY(.5)`, zero velocityで `FALLING_NECTAR`。 | `tick_bee`: 同期 bee flags nectar bit `0x08` がある時だけ、Javaと同じ1回の5% trial、1–2 request、座標幅・高さ・速度を実装。未同期server counterを推測コピーしない。 |
| `Dolphin.java:253–261` | `Dolphin.aiStep` client-side、水中かつvelocity長² >.03で、左右一対×2組の `DOLPHIN`。 | `tick_dolphin`: server-synced yaw/pitch/velocity と common fluid state使用。 |
| `AbstractNautilus.java:278–290` | `tick` が水中で `spawnBubbles`; 乱数 < `clamp(speed*2,.15,1)`。look pitchを±10度制限、mouth originからspread付きbubble。 | `tick_nautilus`: `Nautilus`, `ZombieNautilus` の yaw/pitch・velocity・fluid state使用。 |
| `Panda.java:480` | `Panda.tick` 内 sneeze counter >20 で `afterSneeze`; synced flags/counter。mouth位置はbody yaw、bbox幅、eyeY、velocity依存。 | `tick_panda`: metadata counter19 / flags23に基づき20→21で一度生成。body yaw/寸法/velocityを使用。 |
| `Panda.java:413–424` | `tick → handleEating → addEatingParticles`; `isEating` (= synced eat counter >0)、counter `%5==0` の tickに、mainhandが非emptyならITEMを6個。stack item、count、componentsは `ItemStackTemplate.fromNonEmptyStack` 由来。 | `tick_panda_eating`: metadata index20とmainhand equipmentを保持。Java同様5 tick周期・6粒。item_id/count/typed component patchを `Options::Item` に複写しraw payloadは現スタック型に存在しないため `None`。yaw/pitch/bodyYawでvelocityとmouth local pointを回転。 |
| `Sniffer.java:311` | `tick`; synced `State.DIGGING` と `AnimationState` elapsed 1700–6000ms、head block下のrenderable stateを30個。 | `tick_sniffer`: Sniffer state index18、client clock、yaw由来head block下のblock stateを使用。 |
| `Wolf.java:366` | `Wolf.tick`; server EntityEvent 8開始、56キャンセル。shake animation >.4時だけsin countのSPLASH。 | `tick_wolf` と `handle_entity_event` helper; parent event dispatch接続待ち。 |

## Immediate interaction request helper

操作由来のeffectは `InteractionState::start_use_item` の実 entity-interact packet 経路で、実際に選ばれたhandのheld stack・target・synced entity state・サーバー item tagsからactionを作る。Coreの同じ入力tick内にactionをdrainし、timerは `EntityStore` animal stateへ、即時requestは `ParticleStore::add_particle_spawn_request` へ渡す。ローカルinventoryは消費/更新せず、serverの成功処理と二重実行しない。`AgeUpHappyVillager`はforced=falseの即時client particleなのでAgeableMob timerと同時起動しない。

| Java callsite | 成功条件 / caller hook位置 | Request |
|---|---|---|
| `AgeableMob.java:76–88, 124–137; Animal.java:144–163` | Golden DandelionはBabyとtimer=0でtoggle（すでにlockedでも解除可）、timer=40。通常Animalのtag foodはBabyかつunlockedのforced `ageUp(seconds,true)` のみ。Adult love/healやfood外はtimerなし。 | `PAUSE_MOB_GROWTH` / `RESET_MOB_GROWTH`を40tick、forced-ageは40tickの `HAPPY_VILLAGER` timer。 |
| `Camel.java:357–374, 402–429` | `ItemTags.CAMEL_FOOD`（26.2 tagはCactus）を `fedFood` に通し、`handleEating` の `canAgeUp()` が真 (babyかつnot age-locked)。単にheal/loveだけの成人操作はparticleなし。CamelHuskはoverride tag `CAMEL_HUSK_FOOD` (Rabbit Foot) だが同じ親 `handleEating` / baby age-up branch。`handleEating`内の `canAgeUp` が成立した位置で呼ぶ。 | 1 HAPPY_VILLAGER。entity bboxからrandomX/Y/Z。 |
| `AbstractHorse.java:402–408, 410–475` | `fedFood` が `handleEating` を呼ぶ。age-up particle条件はbaby、`ageUp > 0`, not age-locked。ageUp>0 items: Wheat, Sugar, Hay Block, Apple, Carrot, Golden Carrot, Golden Apple, Enchanted Golden Apple。Red MushroomはageUp=0なので粒子なし。HP回復/temper/loveのみの結果では呼ばない。`handleEating`の条件成立branchでcall。 | 1 HAPPY_VILLAGER。forced-age timerと別物。 |
| `Llama.java:185–227` | `LLAMA_FOOD` tagはWheat/Hay Block。babyかつnot age-lockedで ageUp=10/90 のbranchのみ粒子。health/love/temperだけではなし。`handleEating` age-up branchでcall。 | 1 HAPPY_VILLAGER。 |
| `Tadpole.java:158–163, 212–224` | `FROG_FOOD` (Slime Ball) かつnot age-lockedなら `mobInteract` 成功。feed内でageUp後、isBaby条件なしでparticle。 | 1 HAPPY_VILLAGER。Golden Dandelion branchは別timerでありhelperを呼ばない。 |
| `MushroomCow.java:140–159` | `BROWN` variant、adult、held itemの`getEffectsFromItemStack` nonemptyで `SUCCESS` branch。existing `stewEffects != null`:2 SMOKE; null:consume item/set effects後4 EFFECT Spell(color=-1,power=1). `mobInteract` 分岐確定直後に対応actionでcall。Bowl/shears/empty-effectsは呼ばない。 | repeat/apply actionを区別し正しいkind/options・座標/velocity生成。`shear`のEXPLOSIONはServerLevel.sendParticles packet owner。 |

### Interaction hook parameters to pass

- `AgeUpHappyVillager`: 対象idとseed。Callerがfood tag/item predicateおよび`canAgeUp`/`ageUp>0` branchの成立時だけ呼ぶ。CamelHusk固有food tagをCamelのtagと混同しない。
- Mooshroom: idと`BrownMooshroomStewRepeat`または`BrownMooshroomStewApply`、seed。CallerがBrown/adult、effectsFromItemStack nonempty、SUCCESS結果、stewEffects事前状態を判定する。
- 呼出元は `player/interaction.rs::start_use_item`（`wire::encode_interact` と同じ選択hand/stack）で、Core input tickがactionを適用する。Camel/CamelHusk/Llama/Frog foodsは `RecipeBook.item_tags` のサーバータグを使い、horse feedはJavaの正確なageUp>0 item集合、ordinary Animal foodsは対応サーバータグで判定する。Golden Dandelionはbabyならlocked状態からの解除も通し、40 tick cooldown中のtimer再起動を抑止。Pandaのforced-ageはsynced main gene / on-back / thunder条件も確認する。

## Bee state investigation (why client pollination count is zero)

- `Bee.java:149`: `numCropsGrownSincePollination` はplain `int` field、initializer `0`。
- 書込み: `Bee.java:233` の `readAdditionalSaveData` (NBT load)、`439` reset helper、`443` increment helper。Increment callerは pollination goal 内 `Bee.java:1301` の crop/stem/berry/cave-vine grow成功時のみ。`Mob.serverAiStep` (`Mob.java:749+`) からGoalSelectorが実行されるserver AI処理。resetはdrop-off (`Bee.java:627–630`) 側。
- save/load (`Bee.java:222–233`) はNBT経路だけ。entity spawn packet経路 `ClientPacketListener.handleAddEntity` → `createEntityFromPacket` (`ClientPacketListener.java:576–610`) はentity constructor + `recreateFromPacket` + `ClientLevel.addEntity` で、entity NBTをロードしない。Synced Bee fieldsはDATA_FLAGS byte / anger timestampのみ (`Bee.java:112–118,175–179`); hasNectarはbit `0x08`。
- したがってremote client Beeはcounter初期値0のまま。`Bee.tick`はclientにもparticleを出し、`0 < 10`を満たす。Rustはそのclient意味を実装し、server crop counterを持ち込んでいない。

## Source disposition: event-owned paths connected in the latest snapshot

- EntityEvent-owned/no timer: Animal event18 (7 hearts), Allay override event18 (3 hearts), TamableAnimal/Horse event6/7, Dolphin event38 (7 HAPPY_VILLAGER), Ocelot event40/41, Fox event45 (8 ITEM). Each uses the received EntityParticleEvent and does not locally replay server AI or packet effects.
- Interaction local: 上記helper表。Animal breeding heart (`Animal.java:85`)はserver-side `inLove`だけで同期されず、discrete event18の7 heartsは既存event owner。
- No new client timer: `Squid.java:318`はSquidFleeGoal server AI tick。`MushroomCow.java:167`, `Squid.java:195`, `Wolf.java:431`の`sendParticles`はLevelParticles packet owner。
- `Bee`と`Panda`の前回未対応記述は同期状態・実際のclient callerに基づいて更新済み。Animal `inLove` periodic call is not reachable from client state: it is unsynchronized and the server-local Level.addParticle overload is a no-op; event18 is the visible breeding effect.

## Rust state / hook locations

- `pomme-client/src/entity/particle_animals.rs`: Ageable forced-age / age-lock timer、Camel/Horse/Llama/Tadpole immediate particle、Brown Mooshroom stewEffects client interaction stateと既存動物tick。
- `pomme-client/src/entity/mod.rs`: Panda main gene metadata index21、EAT_COUNTER index20、flags index23、equipment mainhand、Bee flags index18。Animal state lifecycle。
- Animal client tick (`tick_animal_particles`) は `EntityStore::client_particle_requests` から一度だけ呼ばれ、game timeを受け取る。Wolf event 8/56、TamableAnimal 6/7、Animal/Allay 18、Dolphin 38、Ocelot 40/41、Fox 45は既存EntityParticleEvent dispatchからParticleStoreまで接続済み。interaction actionはPlayer input→network command→Core drain→timer/request→ParticleStoreまで接続済み。

## Checks (latest final integration snapshot)

- `cd Client && mise run check`: exit **0** (`cargo check -p pomme-client --locked --profile dev-fast`).
- `mise exec -- cargo test -p pomme-client --locked --profile dev-fast camel_feed_input_routes_only_valid_baby_food_to_the_particle_store -- --test-threads=1`: exit **0**, **1 passed**. Covers selected MainHand vs Offhand, invalid item/adult/locked exclusions, Camel immediate request through `ParticleStore`, Brown Mooshroom first/repeat effects, Golden Dandelion lock/reset timer and forced-age timer output.
- `mise exec -- cargo test -p pomme-client --locked --profile dev-fast entity::particle_animals::tests -- --test-threads=1`: exit **0**, **13 passed**, including Animal/Allay event18, Tamable/Ocelot event6/7/40/41, Dolphin38 and Fox45 request payloads.
- The latest full workspace run after native water-bucket metadata integration (client **1,766**, protocol **52**, singleplayer **1 pass / 1 ignored**) is recorded in `particle-integration-verification.md`.
- `cd Client && mise exec -- cargo check -p pomme-client --tests --locked --profile dev-fast`: exit **0** after integration and before final workspace suite.
- `cd Client && git diff --check`: run again at final snapshot; no commit/reset or broad formatting.
- Automated interaction source is connected: `InteractionState::start_use_item` queues only a predicate-approved action from the selected hand/stack and hit target, and Core input tick drains it to entity timers or `ParticleStore::add_particle_spawn_request`. It does not consume inventory or assume that any click succeeded.
- Java/Rust GUI comparison and original screenshot particle identification remain unverified.
