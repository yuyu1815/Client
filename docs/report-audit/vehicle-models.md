# 26.2 vehicle model, texture, and state audit

## Source and artifact identity

Mapped client artifact: `C:\Users\yuzum\AppData\Roaming\.minecraft\versions\26.2\26.2.jar`, SHA-256 `40896ee9f1e2bec3c934daac7e93d41e9e3d9c2f8ae0ca366d52ffbfd1afa290`.

Inspected mapped classes via JAR class inventory and `javap -c -p`: `BoatModel`, `RaftModel`, `MinecartModel`, `AbstractBoatModel`, `BoatRenderer`, `AbstractBoatRenderer`, `AbstractMinecartRenderer`, `BoatRenderState`, and `MinecartRenderState`. JAR PNG sizes below were read from PNG IHDR, not inferred from model-layer dimensions.

Implementation: `pomme-client/src/renderer/entity_models/vehicles.rs`; existing `EntityPart`, `ModelCube`, `bake_model` convention reused. This file adds source-backed geometry assertions; the test module is not registered/executed because build/test are explicitly out of scope while this file remains unregistered.

## Model-layer source golden values

Cube positions/dimensions/UVs below are literal `CubeListBuilder` values in the mapped bytecode. Pivots and rotations are literal `PartPose` values. Parts are all direct children of the root; there is no nested child hierarchy in these three source models.

| Source class/layer | Part | Cube origin, size | `texOffs` | Pivot | Rotation (XYZ radians) |
|---|---|---|---|---|---|
| `BoatModel.createBoatModel` | bottom | `(-14,-9,-3)`, `(28,16,3)` | `(0,0)` | `(0,3,1)` | `(1.5707964,0,0)` |
| same | back | `(-13,-7,-1)`, `(18,6,2)` | `(0,19)` | `(-15,4,4)` | `(0,4.712389,0)` |
| same | front | `(-8,-7,-1)`, `(16,6,2)` | `(0,27)` | `(15,4,0)` | `(0,1.5707964,0)` |
| same | right | `(-14,-7,-1)`, `(28,6,2)` | `(0,35)` | `(0,4,-9)` | `(0,3.1415927,0)` |
| same | left | `(-14,-7,-1)`, `(28,6,2)` | `(0,43)` | `(0,4,9)` | `(0,0,0)` |
| same | left paddle blade / loom | `(-1,0,-5)`, `(2,2,18)` / `(-1.001,-3,8)`, `(1,6,7)` | both `(62,0)` | `(3,-5,9)` | `(0,0,0.19634955)` |
| same | right paddle blade / loom | `(-1,0,-5)`, `(2,2,18)` / `(0.001,-3,8)`, `(1,6,7)` | both `(62,20)` | `(3,-5,-9)` | `(0,3.1415927,0.19634955)` |
| `RaftModel.createRaftModel` | bottom plank 1 / 2 | `(-14,-11,-4)`, `(28,20,4)` / `(-14,-9,-8)`, `(28,16,4)` | both `(0,0)` | `(0,-2.1,1)` | `(1.5708,0,0)` |
| same | left paddle blade / loom | `(-1,0,-5)`, `(2,2,18)` / `(-1.001,-3,8)`, `(1,6,7)` | both `(0,24)` | `(3,-4,9)` | `(0,0,0.19634955)` |
| same | right paddle blade / loom | `(-1,0,-5)`, `(2,2,18)` / `(0.001,-3,8)`, `(1,6,7)` | both `(40,24)` | `(3,-4,-9)` | `(0,3.1415927,0.19634955)` |
| `BoatModel.createChestBoatModel` | chest_bottom | `(0,0,0)`, `(12,8,12)` | `(0,76)` | `(-2,-5,-6)` | `(0,-1.5707964,0)` |
| same | chest_lid | `(0,0,0)`, `(12,4,12)` | `(0,59)` | `(-2,-9,-6)` | `(0,-1.5707964,0)` |
| same | chest_lock | `(0,0,0)`, `(2,4,1)` | `(0,59)` | `(-1,-6,-1)` | `(0,-1.5707964,0)` |
| `RaftModel.createChestRaftModel` | chest_bottom | `(0,0,0)`, `(12,8,12)` | `(0,76)` | `(-2,-10.1,-6)` | `(0,-1.5707964,0)` |
| same | chest_lid | `(0,0,0)`, `(12,4,12)` | `(0,59)` | `(-2,-14.1,-6)` | `(0,-1.5707964,0)` |
| same | chest_lock | `(0,0,0)`, `(2,4,1)` | `(0,59)` | `(-1,-11.1,-1)` | `(0,-1.5707964,0)` |
| `MinecartModel.createBodyLayer` | bottom | `(-10,-8,-1)`, `(20,16,2)` | `(0,10)` | `(0,4,0)` | `(1.5707964,0,0)` |
| same | front | `(-8,-9,-1)`, `(16,8,2)` | `(0,0)` | `(-9,4,0)` | `(4.712389,0,0)` |
| same | back | `(-8,-9,-1)`, `(16,8,2)` | `(0,0)` | `(9,4,0)` | `(1.5707964,0,0)` |
| same | left | `(-8,-9,-1)`, `(16,8,2)` | `(0,0)` | `(0,4,-7)` | `(0,3.1415927,0)` |
| same | right | `(-8,-9,-1)`, `(16,8,2)` | `(0,0)` | `(0,4,7)` | `(0,0,0)` |

### Layer sizes

These come directly from each `LayerDefinition.create(mesh, width, height)` bytecode call:

| Layer | Size |
|---|---:|
| `BoatModel.createBoatModel` | 128×64 |
| `BoatModel.createChestBoatModel` | 128×128 |
| `RaftModel.createRaftModel` | **128×64** |
| `RaftModel.createChestRaftModel` | 128×128 |
| `MinecartModel.createBodyLayer` | 64×32 |

`RaftModel.createRaftModel` is width **128**, not 64; the PNG header agrees. The previous implementation/report claim of 64×64 and the previous separate UV offsets for the second paddle cubes were wrong; both have been corrected to the actual source values above. The bamboo raft floor is two distinct prisms and the chest raft pivots differ from chest boat pivots; the implementation preserves both facts.

`vehicles.rs` tests now assert exact source part name/order, root parent hierarchy, pivot and rotation, selected cube origin/size/UV, layer dimensions, and generated local vertex bounds for hull/raft/cart bottoms. They also assert paddle UV/cube values and chest layer cube/pivot values. These source-golden tests have **not been executed**; there is no claim of verified test pass or full face-by-face UV equivalence. `bake_model` creates local vertices in normalized block coordinates and negates local cube Y per the existing entity convention; bounds assertions encode that existing contract.

## Material and texture mapping

Mapped JAR contains these families. The material suffix is the wood species in the texture path; hull layer geometry is shared, so per-species geometry is not duplicated.

| Form/material | Texture path in JAR | PNG size | Model layer |
|---|---|---:|---|
| Oak, spruce, birch, jungle, acacia, dark oak, mangrove, cherry, pale oak boats | `assets/minecraft/textures/entity/boat/<wood>.png` | each 128×64 | `BoatModel.createBoatModel` |
| Bamboo raft | `assets/minecraft/textures/entity/boat/bamboo.png` | 128×64 | `RaftModel.createRaftModel` |
| Oak, spruce, birch, jungle, acacia, dark oak, mangrove, cherry, pale oak chest boats | `assets/minecraft/textures/entity/chest_boat/<wood>.png` | each 128×128 | `BoatModel.createChestBoatModel` |
| Bamboo chest raft | `assets/minecraft/textures/entity/chest_boat/bamboo.png` | 128×128 | `RaftModel.createChestRaftModel` |
| All seven minecart variants | `assets/minecraft/textures/entity/minecart/minecart.png` | 64×32 | `MinecartModel.createBodyLayer` |

The listed paths and dimensions were checked in the mapped JAR. The nine non-bamboo wood species have boat and chest-boat forms (18); the two bamboo raft forms bring the requested coverage to 20 vehicle forms. Bamboo uses the corresponding `boat/bamboo` and `chest_boat/bamboo` assets. Seven cart entity types share one shell model/texture: rideable, chest, furnace, TNT, hopper, spawner, command-block. Renderer/entity registration and runtime texture resolver integration are not implemented here.

## State and renderer integration contract

`BoatRenderState` bytecode fields: `yRot:f32`, `hurtDir:i32`, `hurtTime:f32`, `damageTime:f32`, `bubbleAngle:f32`, `isUnderWater:bool`, `rowingTimeLeft:f32`, `rowingTimeRight:f32`. `AbstractBoatRenderer.submit`: translate `(0,0.375,0)`; Y yaw `180°-yRot`; if `hurtTime>0`, X rotate by `sin(hurtTime)*hurtTime*damageTime/10*hurtDir` degrees; only if not underwater and bubble angle is nonzero, rotate about Y by that angle; scale `(-1,-1,+1)` and Y-rotate +90°. Water patch is a distinct vanilla model layer; no separate water-color overlay was found in this submit path.

`AbstractBoatModel.setupAnim` sets each paddle `xRot=clampedLerp(-1.0471976,-0.2617994,(sin(-time)+1)/2)` and `yRot=clampedLerp(-0.7853982,0.7853982,(sin(1-time)+1)/2)`; right paddle then sets `yRot=PI-yRot`.

`MinecartRenderState` contains `xRot`, `yRot`, `offsetSeed`, `hurtDir`, `hurtTime`, `damageTime`, `displayOffset`, `displayBlockModel`, `isNewRender`, and render/rail positions. `AbstractMinecartRenderer.submit` derives three centered `0.004` dither values from bit ranges `[16,23]`, `[20,27]`, `[24,31]` of `offsetSeed`. Resolved cargo is submitted when `displayBlockModel` is nonempty: push; scale `0.75`; translate `(-0.5,(displayOffset-8)/16,0.5)`; Y rotate +90°; submit existing block model; render cart shell afterward. Keep cargo as existing block-state geometry (chest/furnace/TNT/hopper/spawner/command block), not baked into the cart shell.

## Remaining gaps and checks

- New module is not registered, by request/ownership boundary; no crate build/check/test was run. `rustfmt --edition 2024 --check pomme-client/src/renderer/entity_models/vehicles.rs` is the only Rust verification to run here.
- Golden source assertions have been added but not executed. Face-by-face emitted UV rectangles are not compared against a vanilla baked `ModelPart`; exact source `texOffs`/dimensions are asserted for representative hull, oars, chest pieces, and cart base.
- Per-entity renderer dispatch, texture selection/runtime mounted-asset validation, entity-state extraction, dynamic pose transforms and cargo submit remain for the integration owner.
