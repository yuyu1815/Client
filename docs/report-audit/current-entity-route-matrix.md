# Current entity production-route matrix (snapshot HEAD `8ffc5465bf81ffa25cf1867866e8c64f23e88857`)

Static source inventory only; no source edits. Scope is the exact living/nonliving mother-set from report-index 793's existing entity species tables, not the 793-record target and not all `mob_definitions()` entries: 55 living + 47 nonliving = 102 rows. `draw path` means a production caller is present in source; it does not mean visual parity or all layers complete. Unit tests/bake registration alone are not evidence of a production draw.

## Shared route legend (shared references; not repeated in every row)

- **L ingress/store**: `net/handler.rs::dispatch_world_packet` parses AddEntity/SetEntityData (AddEntity around 1193; typed metadata around 1360–1761); `app/core.rs::register_nonliving_spawn` at 1054 and `NetworkEvent::EntitySpawned` at 3433 choose living vs vehicles; `entity/mod.rs::is_living_mob` at 3073 is the classifier. Living metadata enters `EntityStore::apply_entity_data`; update/tick is in `in_game.rs` fixed world loop (`fixed_tick_count`, 3191–3385) and `core.rs` fixed-tick movement functions. See row-specific holes below.
- **L extract/draw**: `app/phases/in_game.rs` living `EntityRenderInfo` collection around 5000–5230 reads `EntityStore.living` and `entity_extras`; `renderer/mod.rs::render_world` at 1685 receives that vector; its world pass invokes `EntityRenderer::draw`; `renderer/pipelines/entity_renderer.rs::draw` at 3797 resolves `self.mobs`, visibility, bake variant and submits grouped instance draws. A kind in the store but without a matching MobDef is skipped at `self.mobs.get()`; a MobDef/bake without extractor/caller is not production draw.
- **N ingress/store**: same `dispatch_world_packet`, core `EntitySpawned` dispatch and `VehicleState` in `entity/mod.rs`; kind-specific metadata is only retained where explicit vehicle setter/application paths exist. Generic `SetEntityData` is not proof that typed payload survives.
- **N extract/draw**: `in_game.rs` dedicated `boat_render_infos` 6219 / `minecart_render_infos` 6718 and entity extraction around 5355; other vehicle entities enter the `EntityRenderInfo` list and renderer draw, while item/block-mesh entities become `ItemRenderInfo` and the item-entity pipeline. `render_world` invokes the relevant pipelines. ItemDisplay, FallingBlock, TNT, mannequin, orb, crystal have dedicated current extractors/call sites (below).
- **Readiness labels**: `route-present` = caller chain exists; `partial` = concrete known semantics/layers remain; `missing` = production route is absent/incomplete or a key source proof remains unknown. All rows remain **not visually verified**; overall `allReady=false`.

## 55 living species

| # | Kind (report mother-set) | Store → extraction consumer | Concrete bake / MobDef and production layer status |
|---:|---|---|---|
| 1 | allay | L ingress/store → L extract/draw | `bake_allay_model` / MobDef; animation/pose flags incomplete. |
| 2 | armadillo | L → L | `bake_armadillo_model`, `bake_baby_armadillo_model` / MobDef; shell pose/state layer not fully audited. |
| 3 | axolotl | L typed variant→L | `bake_axolotl_model`, `bake_baby_axolotl_model` / MobDef; five variant texture pool; swim/playing-dead animation remains. |
| 4 | bee | L → L | `bake_bee_model`, `bake_baby_bee_model` / MobDef; flags/anger/nectar/flying animation mapping remains. |
| 5 | blaze | L → L | `bake_blaze_model` / MobDef; rod orbit animation remains. |
| 6 | breeze | L → L | `bake_breeze_model` plus eyes/wind bakes / MobDef; keyframe/state animation remains. |
| 7 | camel | L → L | `bake_camel_model`, `bake_baby_camel_model` / MobDef; sitting/dashing/rider pose layer remains. |
| 8 | camel_husk | L → L | `bake_camel_husk_model` / MobDef; adult-only source, unique pose/equipment not fully confirmed. |
| 9 | cave_spider | L → L | shared spider bake / MobDef + cave texture; scale/eyes semantics require source/visual check. |
| 10 | copper_golem | L → L | `bake_copper_golem_model` / MobDef; statue-vs-active pose/state distinction remains. |
| 11 | creaking | L → L | `bake_creaking_model` + eyes layer / MobDef; eyes/glow/state layer requires verification. |
| 12 | dolphin | L → L | `bake_dolphin_model`, `bake_baby_dolphin_model` / MobDef; swimming animation/layers remain. |
| 13 | elder_guardian | L → L | `bake_guardian_model(true)` / MobDef; beam/spike animation residual. |
| 14 | endermite | L → L | `bake_endermite_model` / MobDef; animation review. |
| 15 | evoker | L → L | `bake_illager_model` / MobDef; spellcasting arms/layer remains. |
| 16 | fox | L → L | `bake_fox_model`, `bake_baby_fox_model` / MobDef; sleeping/sitting/held-item pose residual. |
| 17 | frog | L → L | `bake_frog_model` / MobDef; croak/tongue state residual. |
| 18 | ghast | L → L | `bake_ghast_model` / MobDef; tentacle animation and shooting texture state residual. |
| 19 | giant | L → L | zombie-family bake / MobDef; giant transform/culling/anchor correctness unknown. |
| 20 | goat | L → L | `bake_goat_model`, `bake_baby_goat_model` / MobDef; ram animation/horn break layer remains. |
| 21 | guardian | L → L | `bake_guardian_model(false)` / MobDef; spikes/beam animation residual. |
| 22 | happy_ghast | L → L | adult/baby `bake_happy_ghast_model` / MobDef; harness layer exists, ropes/equipment/lifecycle not complete. |
| 23 | hoglin | L → L | `bake_hoglin_model`, `bake_baby_hoglin_model` / MobDef; attack/charge animation residual. |
| 24 | illusioner | L → L | `bake_illager_model` / MobDef; illusion-specific behavior/layers unknown. |
| 25 | llama | L → L | `bake_llama_model`, baby + decor bake / MobDef; carpet/chest/equipment layer status unknown. |
| 26 | magma_cube | L → L | `bake_magma_cube_model` / MobDef; segmented squish animation residual. |
| 27 | mooshroom | L → L | cow bake / MobDef; **mushroom block-model layer absent** (known remaining). |
| 28 | nautilus | L → L | `bake_nautilus_model`, baby bake / MobDef; saddle/pose layer unknown. |
| 29 | panda | L → L | `bake_panda_model`, baby bake / MobDef; state animation residual. |
| 30 | parched | L → L | `bake_parched_model` / MobDef; skeleton-like starting bake, species layer/animation needs review. |
| 31 | parrot | L → L | `bake_parrot_model` / MobDef; shoulder/perch/flying pose residual. |
| 32 | phantom | L → L | `bake_phantom_model` / MobDef; size/wing/keyframe animation residual. |
| 33 | piglin | L → L | `bake_piglin_model` / MobDef; pose/equipment/texture layer incomplete. |
| 34 | piglin_brute | L → L | piglin bake / MobDef; brute-specific pose/equipment layer incomplete. |
| 35 | pillager | L → L | `bake_illager_model` / MobDef; crossbow/attack pose residual. |
| 36 | polar_bear | L → L | `bake_polar_bear_model`, baby bake / MobDef; attack/sitting pose residual. |
| 37 | ravager | L → L | `bake_ravager_model` / MobDef; attack/stun pose residual. |
| 38 | shulker | L → L | `bake_shulker_model` / MobDef; **teleport bounds/peek and shell transform residual**. |
| 39 | silverfish | L → L | `bake_silverfish_model` / MobDef; segment animation residual. |
| 40 | sniffer | L → L | `bake_sniffer_model`, baby bake / MobDef; sniff/dig/keyframe state residual. |
| 41 | snow_golem | L → L | `bake_snow_golem_model` / MobDef; pumpkin/equipment layer residual. |
| 42 | strider | L → L | `bake_strider_model`, baby bake / MobDef; saddle/rider/leg animation residual. |
| 43 | sulfur_cube | L → L | small/large inner/outer `bake_sulfur_cube_*` / MobDef; size/squish/material layer needs review. |
| 44 | tadpole | L → L | `bake_tadpole_model` / MobDef; swim animation residual. |
| 45 | trader_llama | L → L | llama + decor bake / MobDef; trader carpet/equipment layer residual. |
| 46 | turtle | L → L | `bake_turtle_model`, baby bake / MobDef; egg belly/scute layer residual. |
| 47 | vex | L → L | `bake_vex_model` / MobDef; charging flags/wings animation residual. |
| 48 | vindicator | L → L | `bake_illager_model` / MobDef; attack/weapon pose residual. |
| 49 | wandering_trader | L → L | `bake_wandering_trader_model` / MobDef; villager hat/profession-equivalent overlays unknown. |
| 50 | warden | L → L | `bake_warden_model` / MobDef; tendril/sonic animation/layer residual. |
| 51 | wither | L → L | `bake_wither_model` / MobDef; target-head indices/state and animation incomplete. |
| 52 | wither_skeleton | L → L | `bake_wither_skeleton_model` / MobDef; scale/weapon pose residual. |
| 53 | zoglin | L → L | `bake_zoglin_model`, baby bake / MobDef; charge animation residual. |
| 54 | zombie_nautilus | L → L | `bake_zombie_nautilus_model` / MobDef; coral overlay / gear state remains. |
| 55 | zombified_piglin | L → L | zombie-family bake / MobDef; snout/ears/weapon layer residual. |

## 47 nonliving species

| # | Kind | Store → extraction consumer | Concrete bake/MobKind and remaining layer/state |
|---:|---|---|---|
| 1 | acacia_boat | N → `boat_render_infos` → EntityRenderer | `bake_boat_model(false)`; wood material routed; water mask predicate fix in source; ordered paddles/cloth/lifecycle visuals remain. |
| 2 | acacia_chest_boat | N → boat renderer | `bake_boat_model(true)`; chest cargo geometry review unresolved. |
| 3 | bamboo_raft | N → boat renderer | `bake_raft_model(false)`; raft material; lifecycle/cloth residual. |
| 4 | bamboo_chest_raft | N → boat renderer | `bake_raft_model(true)`; chest cargo geometry review unresolved. |
| 5 | birch_boat | N → boat renderer | boat bake + birch material; boat animation state partial. |
| 6 | birch_chest_boat | N → boat renderer | chest boat bake + birch material; cargo geometry review unresolved. |
| 7 | cherry_boat | N → boat renderer | boat bake + cherry material; boat animation state partial. |
| 8 | cherry_chest_boat | N → boat renderer | chest boat bake + cherry material; cargo geometry review unresolved. |
| 9 | dark_oak_boat | N → boat renderer | boat bake + dark-oak material; boat animation state partial. |
| 10 | dark_oak_chest_boat | N → boat renderer | chest boat bake + dark-oak material; cargo geometry review unresolved. |
| 11 | jungle_boat | N → boat renderer | boat bake + jungle material; boat animation state partial. |
| 12 | jungle_chest_boat | N → boat renderer | chest boat bake + jungle material; cargo geometry review unresolved. |
| 13 | mangrove_boat | N → boat renderer | boat bake + mangrove material; boat animation state partial. |
| 14 | mangrove_chest_boat | N → boat renderer | chest boat bake + mangrove material; cargo geometry review unresolved. |
| 15 | oak_boat | N → boat renderer | boat bake + oak material; boat animation state partial. |
| 16 | oak_chest_boat | N → boat renderer | chest boat bake + oak material; cargo geometry review unresolved. |
| 17 | pale_oak_boat | N → boat renderer | boat bake + pale-oak material; boat animation state partial. |
| 18 | pale_oak_chest_boat | N → boat renderer | chest boat bake + pale-oak material; cargo geometry review unresolved. |
| 19 | spruce_boat | N → boat renderer | boat bake + spruce material; boat animation state partial. |
| 20 | spruce_chest_boat | N → boat renderer | chest boat bake + spruce material; cargo geometry review unresolved. |
| 21 | minecart | N → `minecart_render_infos` → EntityRenderer | `bake_minecart_model`; current rail pose incomplete. |
| 22 | chest_minecart | N → minecart renderer | cart bake + chest content; old rail pose incomplete, cargo geometry review unresolved. |
| 23 | command_block_minecart | N → minecart renderer | cart bake + command block content; old rail pose incomplete. |
| 24 | furnace_minecart | N → minecart renderer | cart bake + furnace content; old rail pose incomplete. |
| 25 | hopper_minecart | N → minecart renderer | cart bake + hopper content; old rail pose incomplete. |
| 26 | spawner_minecart | N → minecart renderer | cart bake + spawner content; old rail pose incomplete. |
| 27 | tnt_minecart | N → minecart renderer | cart bake + TNT content; old rail pose incomplete. |
| 28 | dragon_fireball | N → projectile render infos → EntityRenderer | `bake_dragon_fireball_model`; animation/material state needs review. |
| 29 | egg | N → item/projectile extraction | current item mesh route; actual item stack/context component fidelity unknown. |
| 30 | ender_pearl | N → item/projectile extraction | current item mesh route; actual item stack/context component fidelity unknown. |
| 31 | experience_bottle | N → item/projectile extraction | item mesh route; rotation/throw pose residual. |
| 32 | fireball | N → projectile render infos | distinct large-fireball route; texture/state proof unknown. |
| 33 | lingering_potion | N → item/projectile extraction | item mesh route; components/rotation residual. |
| 34 | llama_spit | N → projectile render infos → EntityRenderer | `bake_llama_spit_model`; animation/material check remains. |
| 35 | shulker_bullet | N → projectile render infos → EntityRenderer | `bake_shulker_bullet_model`; age is fixed-tick interpolated; teleport/curvature integration review remains. |
| 36 | small_fireball | N → projectile/item extraction | actual distinct kind route; material and item stack source unknown. |
| 37 | splash_potion | N → item/projectile extraction | item mesh route; potion components/orientation residual. |
| 38 | trident | N → projectile render infos → EntityRenderer | `bake_trident_model`; foil draw path exists; user glint controls fixed (not configurable). |
| 39 | wither_skull | N → projectile render infos → EntityRenderer | `bake_wither_skull_model`; dangerous flag/material state residual. |
| 40 | item_display | N typed ItemStack/common Display state → `build_item_render_infos` → ItemEntityPipeline | item mesh, common transform/billboard path exists; **full components/contextTransforms/interpolation/viewrange/glow incomplete**. |
| 41 | block_display | N typed BlockState/common Display → item/block mesh path | bake is existing block model resolver; production extraction/draw consumer **unknown/not verified**; do not count bake alone. |
| 42 | falling_block | N spawn data + start-pos metadata → block mesh `ItemRenderInfo` | block model path exists; native state decoding gate; lifecycle interpolation/position exactness needs review. |
| 43 | tnt | N fuse/block metadata → TNT render info/item mesh | actual block model and fuse pulse route exists; material/brightness parity incomplete. |
| 44 | experience_orb | N value → `experience_orb_render_infos` → EntityRenderer | dedicated billboard/atlas icon route; age and light interpolation exist; source-synced target-level behaviors need review. |
| 45 | end_crystal | N beam/bottom metadata → crystal `EntityRenderInfo` → EntityRenderer | `bake_end_crystal_model` + beam geometry/material; exact state/UV/runtime parity unknown. |
| 46 | armor_stand | N metadata/equipment → living-style `EntityRenderInfo`/MobDef → EntityRenderer | `bake_armor_stand_model`; flags/pose/equipment fields populated at some path; detailed layer/marker behavior needs verification. |
| 47 | mannequin | N profile/pose/equipment → `mannequin_render_infos` → EntityRenderer | player wide/slim bake, cape path; **skin-patch/name-only fetch** remains, profile/skin full-data unresolved. |

## Explicit known non-completion; never count hidden-complete

- World lighting: nighttime terrain raw-light inputs and SkyDarken entity shader remain undone.
- Banner ordered cloth is 0; ShieldPatterns + real Loom preview are 0.
- Mooshroom mushroom layer 0; Shulker teleport bounds remain.
- Flying 7 animation flags/keyframes + Wither target-head state remain.
- Minecart old/new rail pose remains; default chest cargo geometry review has no result.
- ItemDisplay full components/context transforms are missing; common Display interpolation/view-range/glow remain.
- Mannequin SkinPatch/name-only skin fetch remains; user glint knobs remain fixed.
- Rope/leash 0; skull jaw/ear animation 0; shape-offset position-inclusive support 0/110.
- Keep the latest harness panic fix, boat-mask predicate, cape lifecycle UV and fixed-tick motion persistence as source-present only; they are not proof of remaining behavior or runtime correctness.

## Blocker ranking for implementation planning

1. **P0 shared render correctness**: lighting/night raw data + SkyDarken shader; settles frame-wide visual baseline before species tuning.
2. **P1 networked state→extractor completeness**: ItemDisplay components/common fields, mannequin profile/name/skin, Wither/Shulker/boat/minecart state; without these payloads species bake quality is immaterial.
3. **P1 visible defining layers**: Mooshroom mushrooms, Banner ordered cloth, ShieldPatterns/Loom preview, boats/cargo geometry, rope/leash, skull animation.
4. **P2 animation & fidelity**: 7 flying flags/keyframes; minecart new/old rail pose; default cargo review; glint configuration; position-aware shape offsets.
5. **P3 broad visual/runtime verification**: every route source-only so far; run only under separately authorized verification scope.

Source closure is not full readiness: **allReady=false (102/102 route inventory rows; visual/runtime readiness not claimed).**
