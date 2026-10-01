# Remaining 16 living-mob model bakes (vanilla 26.2)

Source is `C:/Users/yuzum/AppData/Roaming/.minecraft/versions/26.2/26.2.jar`. There is no `source/ref` directory, but JDK 21 `javap -c -p` resolves the named model and entity classes directly. Bake source added/extended in `pomme-client/src/renderer/entity_models/humanoid.rs`. No parent module or shared renderer/pipeline/entity/net file is edited here; the follow-up integrator must register this unregistered module.

## Verified model function coverage

| Species | Bake function(s) | 26.2 source / exact geometry notes | Adult texture key(s) verified in jar |
|---|---|---|---|
| CopperGolem | `bake_copper_golem_model` | `CopperGolemModel.createBodyLayer`; base/head/nose/arms/legs, 64x64. Pose animations not baked. `createEyesLayer` retains a part named `eyes`, but `createBodyLayer` does not declare one, so no synthetic eyes mesh was added. | `textures/entity/copper_golem/copper_golem.png`; states `..._exposed.png`, `..._weathered.png`, `..._oxidized.png`; eye assets `..._eyes*.png` exist in the jar |
| Creaking | `bake_creaking_model`, `bake_creaking_eyes_model` | `CreakingModel.createMesh` and `createEyesLayer`; root/upper-body/head, side bark panels, branch arms/legs, 64x64. Eyes are a distinct head-only pass. | `textures/entity/creaking/creaking.png`, `creaking_eyes.png` |
| Endermite | `bake_endermite_model` | `EndermiteModel.createBodyLayer`; 4 segments, sizes/UV tables copied from class initializer, 64x32. | `textures/entity/endermite/endermite.png` |
| MagmaCube | `bake_magma_cube_model` | `MagmaCubeModel.createBodyLayer`; eight 8x1x8 stacked shell plates and the 4x4x4 core, 64x64. The dynamic segment bounce/size transform is not baked. | `textures/entity/slime/magmacube.png` |
| Nautilus | `bake_nautilus_model`, `bake_baby_nautilus_model` | `NautilusModel.createBodyMesh` / `createBabyBodyLayer`; shell plates, body and mouth pieces, adult 128x128 and baby 64x64. The 26.2 class does **not** declare tentacle cuboids in either body mesh; none were fabricated. | `textures/entity/nautilus/nautilus.png`, `nautilus_baby.png`; equipment `textures/entity/equipment/nautilus_body/{copper,iron,gold,diamond,netherite}.png`, `nautilus_saddle/saddle.png` |
| Parched | `bake_parched_model` | Reproduces `SkeletonModel.createSingleModelDualBodyLayer`, which is the `ModelLayers.PARCHED` source, including the extra body/head shell cubes and 64x64 UV. Not the regular 64x32 skeleton layer. | `textures/entity/skeleton/parched.png` |
| Parrot | `bake_parrot_model` | `ParrotModel.createBodyLayer`; head/beak/feather, wings, tail and feet with source pivots, static pose, 32x32. | `textures/entity/parrot/parrot_{blue,green,grey,red_blue,yellow_blue}.png` |
| Shulker | `bake_shulker_model` | `ShulkerModel.createShellMesh/createBodyLayer`; separate 16x12 lid, 16x8 base, 6x6 head, 64x64. Peek-driven lid translation/rotation remains unimplemented. | `textures/entity/shulker/shulker.png` + all 16 `shulker_<color>.png` variants; `spark.png` is separate effect art |
| Silverfish | `bake_silverfish_model` | `SilverfishModel.createBodyLayer`; seven body segments and the three layered plates, dimensions/UV arrays from `<clinit>`, 64x32. Segment animation is static. | `textures/entity/silverfish/silverfish.png` |
| SnowGolem | `bake_snow_golem_model` | `SnowGolemModel.createBodyLayer`; 8x8 head, 12x2x2 stick arms, 10x10 and 12x12 snow bodies, signed -0.5 deformation, 64x64. Pumpkin is vanilla's separate head/equipment layer, not part of this body layer. | `textures/entity/snow_golem/snow_golem.png` |
| SulfurCube | `bake_sulfur_cube_outer_model`, `bake_sulfur_cube_inner_model`, `bake_sulfur_cube_small_outer_model`, `bake_sulfur_cube_small_inner_model` | `SulfurCubeModel` and `SmallSulfurCubeModel` create separate normal/small inner and outer cubes. Normal atlas 128x128 (18 outer/16 inner); small atlas 64x64 (10 outer/8 inner). Keep inner and outer as separate geometry/material submissions. | `textures/entity/sulfur_cube/sulfur_cube_{outer,inner}.png` and `_outer_small.png` / `_inner_small.png` |
| Tadpole | `bake_tadpole_model` | `TadpoleModel.createBodyLayer`; 3x2x3 body, zero-width 2x7 tail plane, 16x16. | `textures/entity/tadpole/tadpole.png` |
| WanderingTrader | `bake_wandering_trader_model` | `WanderingTraderRenderer` uses `VillagerModel` (nose/robe); exact existing `bake_villager_model()` reused rather than a new generic humanoid. No profession layer is assigned. | `textures/entity/wandering_trader/wandering_trader.png` |
| Warden | `bake_warden_model` | `WardenModel.createBodyLayer`; bone/root, body, rib plates, horned head, two zero-depth tendrils, long arms/legs, 128x128. Tendrils/heart/bioluminescent/pulsating animations and passes are not evaluated here. | `textures/entity/warden/warden.png`, `warden_heart.png`, `warden_bioluminescent_layer.png`, `warden_pulsating_spots_1.png`, `_2.png` |
| WitherSkeleton | `bake_wither_skeleton_model` | Reuses `SkeletonModel` cuboids and applies exact `LayerDefinitions` `MeshTransformer.scaling(1.2)` to every pivot/cube/deformation. Vanilla skeleton arm/equipment posing is separate. | `textures/entity/skeleton/wither_skeleton.png` |
| ZombieNautilus | `bake_zombie_nautilus_model`, `bake_zombie_nautilus_coral_model` | `ModelLayers.ZOMBIE_NAUTILUS` is the Nautilus body mesh; `ZombieNautilusCoralModel.createBodyLayer` adds posed coral branches. Coral branch geometry is a separate bake; branch animation/state is static. | `textures/entity/nautilus/zombie_nautilus.png`, `zombie_nautilus_coral.png` |

Also retained the earlier functions: `bake_illager_model` (Evoker/Illusioner/Pillager/Vindicator) and `bake_piglin_model` (Piglin/PiglinBrute/ZombifiedPiglin). Their layer dimensions/UVs and the Piglin adult arm/leg UV child-layer were checked against `IllagerModel.createBodyLayer`, `AbstractPiglinModel.addHead`, and `AdultPiglinModel.createBodyLayer`. Piglin has adult skins for all three species; baby textures exist for Piglin and ZombifiedPiglin but baby geometry is not included here.

All named PNG paths were checked in the 26.2 jar; **33 queried PNGs were present, 0 missing**, and IHDR dimensions matched the model atlases (CopperGolem/Creaking/SnowGolem/Shulker etc. 64x64; Endermite/Silverfish/WitherSkeleton 64x32; Parrot 32x32; Tadpole 16x16; Warden/Nautilus adult/SulfurCube standard 128x128; Nautilus baby and small SulfurCube 64x64). No local resource-pack override was tested.

Exact texture selectors confirmed: Copper weather states map UNAFFECTED/EXPOSED/WEATHERED/OXIDIZED to `copper_golem.png`, `_exposed.png`, `_weathered.png`, `_oxidized.png`; their respective `copper_golem_eyes*.png` are also present, though the named 26.2 `eyes` model part is absent from `createBodyLayer`, so no custom extra geometry was asserted. Creaking `is_active` controls its head-only `creaking_eyes.png` pass. Parrot ids map 0 red_blue, 1 blue, 2 green, 3 yellow_blue, 4 gray (resource uses filename `grey`). Shulker id 16 is no-color base; color byte 0–15 maps to DyeColor order white, orange, magenta, light_blue, yellow, lime, pink, gray, light_gray, cyan, purple, blue, brown, green, red, black. Warden's heart, bioluminescent and pulsating layers use the matching separate PNGs. No Java model functions in this file simulate live pose, held equipment, saddles, face direction, peek amount, cube size/fuse state, or renderer layer blending: those are integration/runtime responsibilities, not unverified geometry substitutions.

## Exact entity metadata from `steel-registry/build_assets/entities.json`

The JSON `synched_data.layers[].fields` gives the absolute 26.2 indices and wire serializers. Inherited fields are omitted below; common Entity/LivingEntity/Mob fields remain at indices 0–15. These values are documentation only; this bake-only change does not touch metadata application.

| Kind | Entity type id; dimensions (width x height) | Subclass synchronized fields (index: name / serializer / default) |
|---|---|---|
| CopperGolem | 28; 0.49 x 0.98 | 16 `weather_state` / `weathering_copper_state` / UNAFFECTED; 17 `copper_golem_state` / `copper_golem_state` / IDLE |
| Creaking | 31; 0.9 x 2.7 | 16 `can_move` / boolean / true; 17 `is_active` / boolean / false; 18 `is_tearing_down` / boolean / false; 19 `home_pos` / optional_block_pos |
| Endermite | 42; 0.4 x 0.3 | none |
| MagmaCube | 80; 0.52 x 0.52 | 16 baby boolean; 17 age_locked boolean; 18 `id_size` / int / 1 |
| Nautilus | 88; 0.875 x 0.95 | 16 baby; 17 age_locked; 18 tamable flags byte; 19 owner optional living reference; 20 `dash` / boolean |
| Parched | 97; 0.6 x 1.99 | none |
| Parrot | 98; 0.5 x 0.9 | 16 baby; 17 age_locked; 18 tamable flags; 19 owner optional living reference; 20 `variant_id` / int / 0 |
| Shulker | 112; 1.0 x 1.0 | 16 `attach_face_id` / direction / DOWN; 17 `peek_id` / byte / 0; 18 `color_id` / byte / 16 |
| Silverfish | 114; 0.4 x 0.3 | none |
| SnowGolem | 121; 0.7 x 1.9 | 16 `pumpkin_id` / byte / 16 |
| SulfurCube | 130; 0.49 x 0.49 | 16 baby; 17 age_locked; 18 `id_size` / int / 1; 19 `max_fuse` / int / -1; 20 `from_bucket` / boolean / false |
| Tadpole | 131; 0.4 x 0.3 | 16 `from_bucket` / boolean / false; 17 age_locked / boolean / false |
| WanderingTrader | 142; 0.6 x 1.95 | 16 baby; 17 age_locked; 18 `unhappy_counter` / int / 0 |
| Warden | 143; 0.9 x 2.9 | 16 `client_anger_level` / int / 0 |
| WitherSkeleton | 147; 0.7 x 2.4 | none |
| ZombieNautilus | 153; 0.875 x 0.95 | 16 baby; 17 age_locked; 18 tamable flags; 19 owner optional living reference; 20 dash; 21 `variant_id` / `zombie_nautilus_variant` / `minecraft:temperate |

Parrot variant int maps exactly by enum id: 0 `red_blue`, 1 `blue`, 2 `green`, 3 `yellow_blue`, 4 `gray` (renderer filename is `parrot_grey.png`). Shulker color byte default 16 selects uncolored `shulker.png`; 0–15 select the 16 named color skins. Copper enum weather state selects unaffected/exposed/weathered/oxidized base and eye texture names. ZombieNautilus variant ids are 0 `minecraft:temperate` → `entity/nautilus/zombie_nautilus.png`, 1 `minecraft:warm` → `entity/nautilus/zombie_nautilus_coral.png` and model `warm`; both entries were read from jar `data/minecraft/zombie_nautilus_variant/{temperate,warm}.json`. Exact protocol index conversion, variant handling, renderer scaling/size, and state animation remain outside this geometry-only file.

## Tests, registration and integration

A `#[cfg(test)]` geometry golden test in `humanoid.rs` checks every added bake for a nonempty mesh and part ranges, valid parent ordering, expected part counts and source-derived UV anchors (including both sulfur layers, the magma stack/core, Shulker lid/base/head, Warden tendrils and Nautilus shell/mouth). It is intentionally not run. `renderer/entity_models/humanoid.rs` remains unregistered. The coordinating patch must add `renderer/entity_models/mod.rs`, declare the module in `renderer/mod.rs`, and map the functions/materials above in `mob_definitions()` or dedicated layered submissions; no change to `BakedEntityModel` / `ModelPart` visibility is needed. Source functions are static geometry bakes; dynamic state animation and equipment layers remain not implemented.