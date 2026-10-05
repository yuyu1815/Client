# Java 26.2 全125 particle 手動比較マトリクス

## 目的・範囲

Java 26.2 の particle を Pomme/Rust と1種類ずつ比較するための未実施チェックリスト。対象はnative extracted registryの125種類すべて。コマンド表示に加え、自然発生、寿命・軌道・色・子粒子・過剰生成、リソースパックと表示条件を観察する。**実機でまだ実行していない項目は未確認であり、passではない。** 問題スクリーンショット由来のparticle種類は未特定で、実際の再現確認は別途必要。

比較条件を揃える: Java 26.2 / protocol 776 と Rust native 26.2。両方で同一ワールド・座標・時刻/天候・ゲーム設定・画質・Particle Mode・resource pack・視点・距離を使用。テスト前後の設定とworld stateを記録し、破壊を伴う操作は捨てワールドのみで手動実施。

## 根拠・構文・出所

- Registry名/options分類は `../../third_party/SteelMC/steel-registry/build_assets/particle_types.json` と `../../../minecraft-26.2-decompiled/src/net/minecraft/core/particles/ParticleTypes.java` を照合。MapCodec provider: `BlockParticleOption`, `DustParticleOptions`, `DustColorTransitionOptions`, `SpellParticleOption`, `ColorParticleOption`, `ItemParticleOption`, `VibrationParticleOption`, `TrailParticleOption`, `ShriekParticleOption`, `SculkChargeParticleOptions`, `PowerParticleOption`, `GeyserParticleOptions`, `GeyserBaseParticleOptions`; simpleはunit codec。
- `ParticleArgument.java` は `particle{...}` をmapとしてparseしtype codecへ渡す。`ParticleCommand.java` 引数順は `name pos delta speed count [force|normal [viewers]]`。以下の構文は**静的ソース確認のみ**。26.2実ゲームでの実行・補完・codec decodeは未確認。
- Options fieldsはcodecで検証: dust color/scale、transition from_color/to_color/scale、block_state、item item-stack template、vibration destination + arrival_in_ticks、trail target/color/duration、shriek delay、sculk roll、power、geyser water_blocks、geyser_base water_blocks/burst_impulse_base。block sourceは `type:"minecraft:block",pos:[x,y,z]`。`VibrationParticleOption` は entity sourceを拒否。
- Provenance: `../../../minecraft-26.2-decompiled/README.md` は公式server bundle由来と記載。Installed metadata `%APPDATA%/ModrinthApp/meta/versions/26.2-0.19.5/26.2-0.19.5.json` はclient SHA-1 `2dc72797acbc1b63fc16a11c4ac393605f453754`, size `39193383`; installed jar `%APPDATA%/ModrinthApp/meta/versions/26.2-0.19.5/26.2-0.19.5.jar` の計算SHA-1も同値で、jar内に`ParticleTypes.class`とclient particle class 228件を確認。これはmanifest hash一致であり、Modrinth jarと公式client.jar全体の独立hash比較ではない。metadata id `26.2-0.19.5`, releaseTime `2026-08-28T11:03:06Z`; `../../pomme-protocol/src/version.rs` native 26.2/protocol 776。Java codec根拠はserver bundle由来decompileで、client artifact実行でのcodec照合は未実施。公式clientと完全同一とは断定しない。

## 1粒子ずつの再現

`docs/plans/particle-validation-26.2.mcfunction` は125個のコメントアウト済みcommands。これはメニューであり、全件を同時実行するfixtureではない。捨てワールド用datapackへ対象1行だけコピーしてfunction化するか、ゲーム内チャットに直接貼り付ける。namespace/profile/worldを自動インストール・変更・起動しない。必要なpermissionを用意し、同じ場所でJava/Rustを個別実行して記録する。

標準形 `/particle <type/options> ~ ~1 ~ 0 0 0 0 1 normal`: count=1, delta=0,speed=0で発生点を固定。各粒子で以下の差分を試す:

1. **count=0/direction**: `/particle <type/options> ~ ~1 ~ 0.25 0.5 0 1 0 normal`。count=0は指定deltaを速度ベクトルとして送る経路を検査。固有実装が無視/変換する可能性があり、不可視だけで失敗判定しない。
2. **count正数**: 1基準、20でも確認し個数/速度分布を記録。emitterのcountと内部展開数を区別。
3. **normal/force**: 標準形末尾を `normal`/`force` で比較。例 `/particle minecraft:flame ~ ~1 ~ 0 0 0 0 1 force`。normalは距離制限、forceは距離制限を無視。viewers指定構文は使わない。
4. **距離**: 近距離、距離制限内、通常範囲外でnormal/forceを分ける。コマンド成功と表示成功は別記録。
5. **Particle Mode**: Options > Video Settings > Particles (All / Decreased / Minimal。UI名はlanguageで異なる)を各値で試す。Rustも同等設定を記録し、同一mode同士で比較。
6. **alwaysShow**: `/particle` はforceとalwaysShowを独立指定できず、ParticleCommandは `alwaysShow=false` 固定。packet fixtureでforce=false/true × alwaysShow=false/trueを独立に比較し、距離/particle modeを記録。forceと混同しない。
7. **resource pack**: vanilla baseline → pack A → vanilla復帰 → pack B（particle texture/model override）で同一particleを再生。優先順/reload完了/error logを記録。全125が独自textureを持つとは仮定せず、textureを参照しないmodel/emitterも分離。
8. **フレーム数**: ゲームtick一定で低/高FPSまたは制限なしで撮影、寿命・移動をticks/frames両方で記録しframe依存/tick依存を分離。
9. **特殊model/emitter**: explosion_emitter, gust_emitter_*, ominous_spawning, trial_spawner_detection*, geyser*, fireworkなどは内部展開・model・寿命・子particle・向きを観察。`/particle` はtrigger全体を再現しない場合があるので自然発生も別試験。

## 全registry matrix (125/125)

各行のparticle argumentのみを個別に使用し、suffix `~ ~1 ~ 0 0 0 0 1 normal`を付ける。完全なコマンド行もmcfunction menuにある。

|#|registry name|options type / codec fields|individual particle argument|結果 / 記録|
|--:|---|---|---|---|
|1|`angry_villager`|`simple` — none (simple codec unit)|`minecraft:angry_villager`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|2|`block`|`block` — block_state: block state|`minecraft:block{block_state:{Name:"minecraft:stone"}}`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|3|`block_marker`|`block` — block_state: block state|`minecraft:block_marker{block_state:{Name:"minecraft:stone"}}`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|4|`bubble`|`simple` — none (simple codec unit)|`minecraft:bubble`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|5|`sulfur_bubbles`|`simple` — none (simple codec unit)|`minecraft:sulfur_bubbles`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|6|`noxious_gas`|`simple` — none (simple codec unit)|`minecraft:noxious_gas`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|7|`noxious_gas_cloud`|`simple` — none (simple codec unit)|`minecraft:noxious_gas_cloud`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|8|`geyser`|`geyser` — positive water_blocks|`minecraft:geyser{water_blocks:3}`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|9|`geyser_base`|`geyser_base` — positive water_blocks + burst_impulse_base|`minecraft:geyser_base{water_blocks:3,burst_impulse_base:0.5}`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|10|`geyser_poof`|`geyser_base` — positive water_blocks + burst_impulse_base|`minecraft:geyser_poof{water_blocks:3,burst_impulse_base:0.5}`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|11|`geyser_plume`|`geyser` — positive water_blocks|`minecraft:geyser_plume{water_blocks:3}`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|12|`cloud`|`simple` — none (simple codec unit)|`minecraft:cloud`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|13|`copper_fire_flame`|`simple` — none (simple codec unit)|`minecraft:copper_fire_flame`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|14|`crit`|`simple` — none (simple codec unit)|`minecraft:crit`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|15|`damage_indicator`|`simple` — none (simple codec unit)|`minecraft:damage_indicator`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|16|`dragon_breath`|`power` — optional power|`minecraft:dragon_breath{power:1.0}`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|17|`dripping_lava`|`simple` — none (simple codec unit)|`minecraft:dripping_lava`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|18|`falling_lava`|`simple` — none (simple codec unit)|`minecraft:falling_lava`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|19|`landing_lava`|`simple` — none (simple codec unit)|`minecraft:landing_lava`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|20|`dripping_water`|`simple` — none (simple codec unit)|`minecraft:dripping_water`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|21|`falling_water`|`simple` — none (simple codec unit)|`minecraft:falling_water`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|22|`dust`|`dust` — color RGB + scale|`minecraft:dust{color:16711680,scale:1.0}`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|23|`dust_color_transition`|`dust_color_transition` — from_color/to_color RGB + scale|`minecraft:dust_color_transition{from_color:16711680,to_color:255,scale:1.0}`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|24|`effect`|`spell` — optional color RGB + power|`minecraft:effect{color:65280,power:1.0}`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|25|`elder_guardian`|`simple` — none (simple codec unit)|`minecraft:elder_guardian`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|26|`enchanted_hit`|`simple` — none (simple codec unit)|`minecraft:enchanted_hit`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|27|`enchant`|`simple` — none (simple codec unit)|`minecraft:enchant`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|28|`end_rod`|`simple` — none (simple codec unit)|`minecraft:end_rod`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|29|`entity_effect`|`color` — color ARGB|`minecraft:entity_effect{color:-65281}`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|30|`explosion_emitter`|`simple` — none (simple codec unit)|`minecraft:explosion_emitter`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|31|`explosion`|`simple` — none (simple codec unit)|`minecraft:explosion`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|32|`gust`|`simple` — none (simple codec unit)|`minecraft:gust`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|33|`small_gust`|`simple` — none (simple codec unit)|`minecraft:small_gust`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|34|`gust_emitter_large`|`simple` — none (simple codec unit)|`minecraft:gust_emitter_large`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|35|`gust_emitter_small`|`simple` — none (simple codec unit)|`minecraft:gust_emitter_small`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|36|`sonic_boom`|`simple` — none (simple codec unit)|`minecraft:sonic_boom`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|37|`falling_dust`|`block` — block_state: block state|`minecraft:falling_dust{block_state:{Name:"minecraft:stone"}}`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|38|`firework`|`simple` — none (simple codec unit)|`minecraft:firework`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|39|`fishing`|`simple` — none (simple codec unit)|`minecraft:fishing`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|40|`flame`|`simple` — none (simple codec unit)|`minecraft:flame`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|41|`infested`|`simple` — none (simple codec unit)|`minecraft:infested`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|42|`cherry_leaves`|`simple` — none (simple codec unit)|`minecraft:cherry_leaves`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|43|`pale_oak_leaves`|`simple` — none (simple codec unit)|`minecraft:pale_oak_leaves`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|44|`tinted_leaves`|`color` — color ARGB|`minecraft:tinted_leaves{color:-65281}`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|45|`sculk_soul`|`simple` — none (simple codec unit)|`minecraft:sculk_soul`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|46|`sculk_charge`|`sculk_charge` — roll|`minecraft:sculk_charge{roll:0.5}`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|47|`sculk_charge_pop`|`simple` — none (simple codec unit)|`minecraft:sculk_charge_pop`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|48|`soul_fire_flame`|`simple` — none (simple codec unit)|`minecraft:soul_fire_flame`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|49|`soul`|`simple` — none (simple codec unit)|`minecraft:soul`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|50|`flash`|`color` — color ARGB|`minecraft:flash{color:-65281}`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|51|`happy_villager`|`simple` — none (simple codec unit)|`minecraft:happy_villager`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|52|`composter`|`simple` — none (simple codec unit)|`minecraft:composter`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|53|`heart`|`simple` — none (simple codec unit)|`minecraft:heart`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|54|`instant_effect`|`spell` — optional color RGB + power|`minecraft:instant_effect{color:65280,power:1.0}`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|55|`item`|`item` — item item-stack template|`minecraft:item{item:{id:"minecraft:stone",count:1}}`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|56|`vibration`|`vibration` — destination block position source + arrival_in_ticks|`minecraft:vibration{destination:{type:"minecraft:block",pos:[0,64,0]},arrival_in_ticks:40}`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|57|`trail`|`trail` — target + RGB color + positive duration|`minecraft:trail{target:[4.0,65.0,0.0],color:16711935,duration:40}`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|58|`pause_mob_growth`|`simple` — none (simple codec unit)|`minecraft:pause_mob_growth`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|59|`reset_mob_growth`|`simple` — none (simple codec unit)|`minecraft:reset_mob_growth`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|60|`item_slime`|`simple` — none (simple codec unit)|`minecraft:item_slime`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|61|`item_cobweb`|`simple` — none (simple codec unit)|`minecraft:item_cobweb`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|62|`item_snowball`|`simple` — none (simple codec unit)|`minecraft:item_snowball`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|63|`large_smoke`|`simple` — none (simple codec unit)|`minecraft:large_smoke`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|64|`lava`|`simple` — none (simple codec unit)|`minecraft:lava`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|65|`mycelium`|`simple` — none (simple codec unit)|`minecraft:mycelium`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|66|`note`|`simple` — none (simple codec unit)|`minecraft:note`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|67|`poof`|`simple` — none (simple codec unit)|`minecraft:poof`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|68|`portal`|`simple` — none (simple codec unit)|`minecraft:portal`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|69|`rain`|`simple` — none (simple codec unit)|`minecraft:rain`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|70|`smoke`|`simple` — none (simple codec unit)|`minecraft:smoke`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|71|`white_smoke`|`simple` — none (simple codec unit)|`minecraft:white_smoke`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|72|`sneeze`|`simple` — none (simple codec unit)|`minecraft:sneeze`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|73|`spit`|`simple` — none (simple codec unit)|`minecraft:spit`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|74|`squid_ink`|`simple` — none (simple codec unit)|`minecraft:squid_ink`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|75|`sweep_attack`|`simple` — none (simple codec unit)|`minecraft:sweep_attack`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|76|`totem_of_undying`|`simple` — none (simple codec unit)|`minecraft:totem_of_undying`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|77|`underwater`|`simple` — none (simple codec unit)|`minecraft:underwater`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|78|`splash`|`simple` — none (simple codec unit)|`minecraft:splash`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|79|`witch`|`simple` — none (simple codec unit)|`minecraft:witch`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|80|`bubble_pop`|`simple` — none (simple codec unit)|`minecraft:bubble_pop`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|81|`current_down`|`simple` — none (simple codec unit)|`minecraft:current_down`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|82|`bubble_column_up`|`simple` — none (simple codec unit)|`minecraft:bubble_column_up`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|83|`nautilus`|`simple` — none (simple codec unit)|`minecraft:nautilus`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|84|`dolphin`|`simple` — none (simple codec unit)|`minecraft:dolphin`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|85|`campfire_cosy_smoke`|`simple` — none (simple codec unit)|`minecraft:campfire_cosy_smoke`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|86|`campfire_signal_smoke`|`simple` — none (simple codec unit)|`minecraft:campfire_signal_smoke`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|87|`dripping_honey`|`simple` — none (simple codec unit)|`minecraft:dripping_honey`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|88|`falling_honey`|`simple` — none (simple codec unit)|`minecraft:falling_honey`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|89|`landing_honey`|`simple` — none (simple codec unit)|`minecraft:landing_honey`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|90|`falling_nectar`|`simple` — none (simple codec unit)|`minecraft:falling_nectar`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|91|`falling_spore_blossom`|`simple` — none (simple codec unit)|`minecraft:falling_spore_blossom`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|92|`ash`|`simple` — none (simple codec unit)|`minecraft:ash`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|93|`crimson_spore`|`simple` — none (simple codec unit)|`minecraft:crimson_spore`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|94|`warped_spore`|`simple` — none (simple codec unit)|`minecraft:warped_spore`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|95|`spore_blossom_air`|`simple` — none (simple codec unit)|`minecraft:spore_blossom_air`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|96|`dripping_obsidian_tear`|`simple` — none (simple codec unit)|`minecraft:dripping_obsidian_tear`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|97|`falling_obsidian_tear`|`simple` — none (simple codec unit)|`minecraft:falling_obsidian_tear`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|98|`landing_obsidian_tear`|`simple` — none (simple codec unit)|`minecraft:landing_obsidian_tear`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|99|`reverse_portal`|`simple` — none (simple codec unit)|`minecraft:reverse_portal`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|100|`white_ash`|`simple` — none (simple codec unit)|`minecraft:white_ash`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|101|`small_flame`|`simple` — none (simple codec unit)|`minecraft:small_flame`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|102|`snowflake`|`simple` — none (simple codec unit)|`minecraft:snowflake`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|103|`dripping_dripstone_lava`|`simple` — none (simple codec unit)|`minecraft:dripping_dripstone_lava`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|104|`falling_dripstone_lava`|`simple` — none (simple codec unit)|`minecraft:falling_dripstone_lava`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|105|`dripping_dripstone_water`|`simple` — none (simple codec unit)|`minecraft:dripping_dripstone_water`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|106|`falling_dripstone_water`|`simple` — none (simple codec unit)|`minecraft:falling_dripstone_water`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|107|`glow_squid_ink`|`simple` — none (simple codec unit)|`minecraft:glow_squid_ink`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|108|`glow`|`simple` — none (simple codec unit)|`minecraft:glow`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|109|`wax_on`|`simple` — none (simple codec unit)|`minecraft:wax_on`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|110|`wax_off`|`simple` — none (simple codec unit)|`minecraft:wax_off`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|111|`electric_spark`|`simple` — none (simple codec unit)|`minecraft:electric_spark`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|112|`scrape`|`simple` — none (simple codec unit)|`minecraft:scrape`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|113|`shriek`|`shriek` — delay|`minecraft:shriek{delay:20}`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|114|`egg_crack`|`simple` — none (simple codec unit)|`minecraft:egg_crack`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|115|`dust_plume`|`simple` — none (simple codec unit)|`minecraft:dust_plume`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|116|`trial_spawner_detection`|`simple` — none (simple codec unit)|`minecraft:trial_spawner_detection`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|117|`trial_spawner_detection_ominous`|`simple` — none (simple codec unit)|`minecraft:trial_spawner_detection_ominous`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|118|`vault_connection`|`simple` — none (simple codec unit)|`minecraft:vault_connection`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|119|`dust_pillar`|`block` — block_state: block state|`minecraft:dust_pillar{block_state:{Name:"minecraft:stone"}}`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|120|`ominous_spawning`|`simple` — none (simple codec unit)|`minecraft:ominous_spawning`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|121|`raid_omen`|`simple` — none (simple codec unit)|`minecraft:raid_omen`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|122|`trial_omen`|`simple` — none (simple codec unit)|`minecraft:trial_omen`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|123|`block_crumble`|`block` — block_state: block state|`minecraft:block_crumble{block_state:{Name:"minecraft:stone"}}`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|124|`firefly`|`simple` — none (simple codec unit)|`minecraft:firefly`|Java ☐ Rust ☐ Δ ☐; notes: ______|
|125|`sulfur_cube_goo`|`simple` — none (simple codec unit)|`minecraft:sulfur_cube_goo`|Java ☐ Rust ☐ Δ ☐; notes: ______|


## 観察記録テンプレート (各粒子)

- ID/date/tester: ____; version/protocol: ____; world/seed (private): ____; position/facing: ____; time/weather: ____
- trigger/command/options/count/delta/speed: ____; normal/force: ____; distance/viewers: ____; Java/Rust Particle Mode: ____
- resource pack stack/order/hash/reload: ____; FPS cap/observed FPS/tick duration: ____; render distance/logs: ____
- visibility (Java/Rust): ____ / ____; shape/size/texture/model: ____ / ____; lifetime (ticks/frames): ____ / ____
- trajectory/gravity/drag/spread: ____ / ____; color start/end/transition: ____ / ____; child particles/emitter/model: ____ / ____
- spawn count/over-generation/missing/duplicate: ____; distance/limit behavior: ____; result PASS / FAIL / BLOCKED / NOT RUN + evidence: ____
- screenshot/video (no private data): ____; difference/suspected boundary: ____; repeat count: ____

## 自然発生経路 (commandの代替ではない)

捨てワールドで各状態を**手動**に作り、両クライアントで同じblock state/entity/item/tickを再現。設置・流体配置・火/爆発/破壊/mob/アイテム操作はworld変更なので自動実行しない。random tick依存は両方で十分な回数を観察し、event駆動と周期発生を区別。

|経路|手動で作る場面|期待する観察|結果|
|---|---|---|---|
|block/state|Torch/copper/soul torch、candle点火/消火、campfire (hay bale有無)、spore blossom天井設置、cherry/pale oak/azalea leavesを観察/破壊、sculk sensor/shrieker/trial spawner/vault|flame種、smoke/campfire smoke、spore/leaves tint、sculk/trial/vault particle。位置/色/頻度/emitter|______|
|block entity/machine|Brewing stand動作、hive honey滴下、furnace/blast furnace/smoker稼働|smoke/滴下/魔法系。BE animationとparticle寿命/位置/頻度|______|
|fluid/environment|water/bubble column/水中、雨天屋外、lava接触/滴下、pointed dripstoneと水/溶岩/cauldron、honey、obsidian tear source、snow|bubble/current/underwater/splash/rain/lava/water/honey/nectar/obsidian tear/dripstone/snowflake等。fluid stateと頻度|______|
|entity|villager交渉/繁殖/怒り、elder guardian、dragon breath、warden sonic boom/sculk、breeze gust、slime/cobweb/snowball projectile、squid ink、llama spit、fishing bobber、mob damage/totem/raid/trial omen|emotion/elder/dragon/sonic/gust/sculk/item/ink/spit/fishing/damage/crit/totem/omen等。攻撃本体とparticle-only分離|______|
|item/player event|enchanting table、effect potion、firework、item break/drop、sweep attack、infested block break、wax/scrape copper、egg hatch|enchant/effect/firework/item/sweep/infested/wax/scrape/egg_crack/crit等。item component/色/子particle|______|
|biome/world event|crimson/warped biome、mycelium、ash/white ash ambience。firefly/sulfur/noxious featureは26.2 worldで利用可能と確認後|biome ambience/sulfur/noxious/firefly/omen。feature/blockがあることを先に確認|______|

上記は候補場面であり、正確なparticle/provider triggerをJava sourceで見つけられない場合は未確認とし、名称から存在やtriggerを推測しない。

## スクリーンショット由来particle (未特定)

発生条件・particle名未特定。matrixのどの型か断定しない。行動/block/entity/server plugin/command、world/version/protocol、同位置・設定・pack、時刻、録画が分かった後に候補triggerかpacketを絞り再現確認する。結論: **未確認 / 未実行**。
