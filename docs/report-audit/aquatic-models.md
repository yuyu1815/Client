# Aquatic models — 26.2 bytecode verification and integration notes

`pomme-client/src/renderer/entity_models/aquatic.rs` transcribes the five missing aquatic kinds (Axolotl, Dolphin, Guardian, ElderGuardian, Turtle); adult/baby layers are independent where 26.2 defines them. Squid, GlowSquid, Cod, Salmon, Pufferfish, TropicalFish already have MobDefs and were left alone. Exactly seven public bake functions cover the five kinds: adult/baby Axolotl, adult/baby Dolphin, one Guardian/ElderGuardian selector, adult/baby Turtle.

## Bytecode source and mapping names

Jar: `C:/Users/yuzum/AppData/Roaming/.minecraft/versions/26.2/26.2.jar`. The jar contains mapped classes, not a `source/reference` directory. Read-only JDK `javap -p -c` was used to inspect the full `createBodyLayer()` method bodies and static transformer/enum initialization. Model class names:

- `net.minecraft.client.model.animal.axolotl.AdultAxolotlModel`
- `net.minecraft.client.model.animal.axolotl.BabyAxolotlModel`
- `net.minecraft.client.model.animal.dolphin.DolphinModel`
- `net.minecraft.client.model.animal.dolphin.BabyDolphinModel`
- `net.minecraft.client.model.monster.guardian.GuardianModel`
- `net.minecraft.client.model.animal.turtle.AdultTurtleModel`
- `net.minecraft.client.model.animal.turtle.BabyTurtleModel`
- Render sources inspected: `AxolotlRenderer`, `GuardianRenderer`, `ElderGuardianRenderer`, `TurtleRenderer`; variant enum `net.minecraft.world.entity.animal.axolotl.Axolotl$Variant`.

The output was captured directly from the real 26.2 jar using `javap -p -c` on each class; methods called by the layer builders were traced through `CubeListBuilder.texOffs/addBox`, `PartDefinition.addOrReplaceChild`, `PartPose.offset/offsetAndRotation/rotation`, and `LayerDefinition.create/apply`. Guardian additionally required `GuardianModel` static `SPIKE_*` arrays and `getSpikeOffset/X/Y/Z`; its body method uses all four helpers.

## Geometry evidence table

All origins, sizes, pivots and UVs below are pixel-space values exactly as passed to `addBox`/`PartPose`; deformation defaults to `0.0`, except where noted. Parent relationships from `addOrReplaceChild` are retained by `EntityPart.parent`.

| Layer | Atlas | Cube data `(uv; origin; size)` and parent pose |
|---|---:|---|
| AdultAxolotl `createBodyLayer` | 64x64 | body root pose `(0,19.5,5)`: `(0,11;-4,-2,-9;8,4,10)`, `(2,17;0,-3,-8;0,5,9)`; child head pose `(0,0,-9)`: `(0,1;-4,-3,-5;8,5,5)` deformation `.001`; head children top_gills pose `(0,-3,-1)` `(3,37;-4,-3,0;8,3,0)`, left_gills `(-4,0,-1)` `(0,40;-3,-5,0;3,7,0)`, right_gills `(4,0,-1)` `(11,40;0,-5,0;3,7,0)`, each deformation `.001`. Body child legs: right-hind `(-3.5,1,-1)` / right-front `(-3.5,1,-8)` use `(2,13;-2,0,0;3,5,0)`; left-hind `(3.5,1,-1)` / left-front `(3.5,1,-8)` use `(2,13;-1,0,0;3,5,0)`, all deformation `.001`. Tail pose `(0,0,1)`: `(2,19;0,-3,0;0,5,12)`. |
| BabyAxolotl `createBodyLayer` | 32x32 | root pose `(0,24,0)`. Body child pose `(0,-1.25,1.75)`: `(0,0;-2,-.75,-2.75;4,2,6)`, `(0,12;0,-1.75,-2.75;0,3,5)`. Head is body child pose `(0,.25,-2.75)`: `(0,8;-3,-2,-4;6,3,4)`; head child gills: left `(20,8;0,-3.5,0;3,5,0)` pose `(3,-.5,-2)`; right `(20,3;-3,-3.5,0;3,5,0)` pose `(-3,-.5,-2)`; top `(20,0;-3,-3,0;6,3,0)` pose `(0,-2,-2)`. Body child front legs UV `(20,16)` and `(20,13)`, hind legs `(20,14)`, cube origins `(-3,0,-.5)`/`(0,0,-.5)`, size `(3,0,1)` and source poses ±2,.25,±1.25/1.75; right hind has nested rotations `(0,1.5708,1.5708)` then `(-1.5708,0,1.5708)`. Tail child pose `(0,-.25,3.25)`: `(10,9;0,-1.5,-1;0,3,8)`. |
| Dolphin `createBodyLayer` | 64x64 | body root `(0,22,-5)`: `(22,0;-4,-7,0;8,7,13)`. Body child back_fin pose `(0,0,0)` rotation `(1.0471976,0,0)`: `(51,0;-.5,0,8;1,4,5)`. left_fin pose `(2,-2,4)`, rotation `(1.0471976,0,2.0943952)`, mirror true: `(48,20;-.5,-4,0;1,4,7)`; right_fin pose `(-2,-2,4)`, rotation `(1.0471976,0,-2.0943952)`, mirror false, same box/UV. Tail body child pose `(0,-2.5,11)`, rotation X `-.10471976`: `(0,19;-2,-2.5,0;4,5,11)`. tail_fin tail child pose `(0,0,9)`: `(19,20;-5,-.5,0;10,1,6)`. Head body child pose `(0,-4,-3)`: `(0,0;-4,-3,-3;8,7,6)`; nose head child pose zero: `(0,13;-1,2,-7;2,2,4)`. |
| BabyDolphin `createBodyLayer` | 64x64 | Dedicated layer, not the adult mesh scaled. Body `(0,21.5,0)`: `(20,0;-3,-2.5,-4;6,5,8)`; head child `(0,1,-4)`: `(0,0;-3,-3.5,-4;6,5,4)`; nose child `(0,.5,-4)`: `(0,9;-1,-1,-2;2,2,2)`. Fins UV `(34,18)` / `(48,18)`, origin `(-.5,-1.5,-.5)`, size `(1,3,6)`, poses ±1.8,.85,-2.6 and rotations `(0.8727,0,±1.7017)`; both final mirror false (right builder resets `.mirror(false)`). Tail `(0,1,4)`, X rotation `-.10471976`, `(0,13;-2,-1.5,0;4,3,7)`; tail_fin child `(0,0,9)` `(22,13;-4,-.5,-1;8,1,4)`; back_fin `(0,-1,-2.7)` rotation X `.8727` `(42,0;-.5,-1,1;1,3,4)`. |
| Guardian `createBodyLayer` | 64x64 | head root has body `(0,0;-6,10,-8;12,12,16)`, four shell strips `(0,28;-8,10,-6;2,12,12)`, `(0,28;6,10,-6;2,12,12)` (the latter uses the bytecode's `addBox(..., true)` mirror flag), `(16,40;-6,8,-6;12,2,12)`, `(16,40;-6,22,-6;12,2,12)`. Twelve spikes each `(0,0;-1,-4.5,-1;2,9,2)` with generated pivots/rotation arrays recorded in source; eye `(8,0;-1,15,0;2,2,1)` pose `(0,0,-8.25)`. tail0 `(40,0;-2,14,7;4,4,8)`; tail1 child pose `(-1.5,.5,14)` `(0,54;0,14,0;3,3,7)`; tail2 child pose `(.5,.5,6)` has `(41,32;0,14,0;2,2,6)` and `(25,19;1,10.5,3;1,9,9)`. |
| AdultTurtle `createBodyLayer` | 128x64 | head `(3,0;-3,-1,-3;6,5,6)` pose `(0,19,-10)`. Body pose `(0,11,-10)`, X rotation π/2: shell `(7,37;-9.5,3,-10;19,20,6)`, belly `(31,1;-5.5,3,-13;11,18,3)`. Egg belly `(70,33;-4.5,3,-14;9,18,1)` same pose. Right/left hind legs UV `(1,23)/(1,12)`, box `(-2,0,0;4,1,10)`, poses `(-3.5,22,11)/(3.5,22,11)`. Right/left front UV `(27,30)/(27,24)`, boxes `(-13,0,-2;13,1,5)/(0,0,-2;13,1,5)`, poses `(-5,21,-4)/(5,21,-4)`. |
| BabyTurtle `createBodyLayer` | 16x16 | body `(0,0;-2,-1,-2;4,2,4)` pose `(0,22.9,1)`; head `(0,6;-1.5,-2,-3;3,3,3)` pose `(0,22.9,-1)`. Hind legs use UV `(-1,0)/(-1,1)`, boxes `(-2,0,-.5;2,0,1)/(0,0,-.5;2,0,1)`, poses `(-2,23.9,2.5)/(2,23.9,2.5)`. Front UV `(8,6)/(8,7)` with same size, boxes `(-2,0,-.5;2,0,1)/(0,0,-.5;2,0,1)`, poses `(-2,23.9,-.5)/(2,23.9,-.5)`. |

## Coordinate and transform mapping

`ModelCube.origin/size` and `EntityPart.offset` retain the bytecode's model-pixel, Y-down coordinates; cube generation negates cube Y and `ModelConvention::EntityYDown` negates part-pivot Y, applies the root `24.016` rebase and the engine's X half of vanilla `scale(-1,-1,1)`. This matches the existing renderer convention; do not negate coordinates again in integration. Part parent indices preserve hierarchical pose composition, and `default_rotation` is vanilla `(xRot,yRot,zRot)` in radians; current transform code performs EntityYDown conjugation (x/z sign inversion, y unchanged). ModelCube mirror is used only where vanilla builder retains `.mirror()`.

ElderGuardian uses `GuardianModel.createElderGuardianLayer()`, which applies `MeshTransformer.scaling(2.35f)`; `bake_guardian_model(true)` scales geometry and pivots by 2.35. `ElderGuardianRenderer` separately passes 1.2f into the `MobRenderer` constructor while `GuardianRenderer` passes .5f. These constructor floats are shadow radii, **not** additional mesh scales. Guardian uses `bake_guardian_model(false)`.

## Axolotl variant order / texture map

`Axolotl$Variant` static initialization confirms legacy IDs and declaration order: `0 LUCY`, `1 WILD`, `2 GOLD`, `3 CYAN`, `4 BLUE` (default Lucy). `AxolotlRenderer.lambda$static$0` constructs adult and baby asset paths from `getName()` using `axolotl_%s.png` and `axolotl_%s_baby.png`. `Axolotl.DATA_VARIANT` is the first Axolotl-defined synchronized `INT`; the Entity/Living/Mob/Ageable inheritance layout places it at metadata index 18 (the following Axolotl fields are playing-dead 19 and from-bucket 20). The IDs at index 18 map to the exact texture order below:

| Legacy variant id | texture path suffix |
|---:|---|
| 0 Lucy | `axolotl/axolotl_lucy.png`, `axolotl/axolotl_lucy_baby.png` |
| 1 Wild | `axolotl/axolotl_wild.png`, `axolotl/axolotl_wild_baby.png` |
| 2 Gold | `axolotl/axolotl_gold.png`, `axolotl/axolotl_gold_baby.png` |
| 3 Cyan | `axolotl/axolotl_cyan.png`, `axolotl/axolotl_cyan_baby.png` |
| 4 Blue | `axolotl/axolotl_blue.png`, `axolotl/axolotl_blue_baby.png` |

Asset paths were confirmed in the 26.2 jar/extracted asset set. Integration must connect the 26.2 protocol Axolotl variant integer at metadata index 18 to these legacy IDs. Vanilla's out-of-range behavior is `ByIdMap.OutOfBoundsStrategy.ZERO` (Lucy).

Other texture keys (all present in 26.2 assets):

- Dolphin adult/baby: `textures/entity/dolphin/dolphin.png`, `textures/entity/dolphin/dolphin_baby.png`.
- Guardian/ElderGuardian: `textures/entity/guardian/guardian.png`, `textures/entity/guardian/guardian_elder.png`; `guardian_beam.png` is a beam layer, not a model skin.
- Turtle adult/baby: `textures/entity/turtle/turtle.png`, `textures/entity/turtle/turtle_baby.png`; scute equipment layer textures are separate.

## Integration

Only owned files were edited. Integration patch should add `pub mod entity_models;` in `pomme-client/src/renderer/mod.rs` and `pub mod aquatic;` in the shared `pomme-client/src/renderer/entity_models/mod.rs` (coordinate with the other family files before creating that module file), then add five MobDefs: Axolotl (adult/baby five paired textures and variant IDs above), Dolphin (adult/baby), Guardian and ElderGuardian (`bake_guardian_model(false/true)` and matching textures), Turtle (adult/baby). No private helper visibility change is needed. Mesh animation, guardian beam, turtle egg-belly visibility, equipment scute, aquatic spawn classification, and metadata delivery remain integration work; geometry correctness alone does not implement those behaviors.

## Verification limits

- Per-model cube/UV/pose/deformation and vanilla transforms above are grounded in read-only 26.2 `javap -p -c` output and mapped method names. Source `.java` files were not available; `javap` bytecode, rather than a source/reference checkout, is the primary evidence.
- In-file tests assert nonempty vertices, expected cube counts, representative exact UV/origin/size values for each layer family (including elder 2.35x geometry), and that each face UV rect can be wrapped into its source atlas bounds using the same rule as existing `generate_cube_vertices`. Tests were not run, per instruction.
- Ran `rustfmt --edition 2024 --check` and `git diff --check` only. No cargo check/build/test. The module remains unregistered and must be checked after integration.
