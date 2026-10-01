# Minecraft 26.2 terrestrial geometry audit

## Verified source geometry

`terrestrial.rs` now contains static geometry transcribed from the mapped client jar's actual `createBodyLayer` or shared mesh-builder methods. Geometry is considered verified only from source bytecode, not from texture presence. Full class methods were captured with `javap -p -c`; source layer atlases, part counts, cube counts, parent paths, pivots, rotations, UV origins, origins, sizes, mirror flags and deformations are exhaustively listed below.

**15 of the 16 newly missing target species have renderer-confirmed model geometry in this module.** `Mooshroom` is intentionally unsupported here: vanilla draws actual mushroom block models dynamically and must not be approximated with handmade cubes. Existing Ocelot, Rabbit, Donkey, Mule, SkeletonHorse and ZombieHorse registrations were not duplicated.

| Species | Vanilla renderer / model layer(s) | Atlas, parts/cubes | Source detail |
|---|---|---|---|
| Armadillo | `ArmadilloRenderer`: `AdultArmadilloModel`, `BabyArmadilloModel` | 64×64 13/11; 64×64 12/11 | both complete layer bodies |
| Camel | `CamelRenderer`: `AdultCamelModel`, `BabyCamelModel` | 128×128 10/12; 64×64 9/11 | adult layer delegates to `createBodyMesh`, followed and read |
| CamelHusk | `CamelHuskRenderer`: `AdultCamelModel` only | 128×128 10/12 | renderer constructs adult model directly; its own texture/equipment, no baby-model selection |
| Fox | `FoxRenderer`: `AdultFoxModel`, `BabyFoxModel` | 48×32 10/10; 32×32 7/10 | complete adult and baby layers |
| Frog | `FrogRenderer`: `FrogModel` | 48×48 16/16 | no `BabyFrogModel`; vanilla uses `FrogModel` |
| Goat | `GoatRenderer`: `GoatModel`, `BabyGoatModel` | 64×64 9/12; 64×64 11/12 | complete adult and baby layers |
| Hoglin | `AbstractHoglinRenderer`: `HoglinModel`, `BabyHoglinModel` | 128×64 11/11; 64×64 8/11 | shared family layers |
| Zoglin | `ZoglinRenderer` → `AbstractHoglinRenderer`: same two Hoglin layers | same | renderer supplies `ZOGLIN` and `ZOGLIN_BABY` layers; textures differ, shape does not |
| Panda | `PandaRenderer`: `PandaModel`, `BabyPandaModel` | 64×64 6/9; 64×64 6/9 | complete adult and baby layers |
| PolarBear | `PolarBearRenderer`: `PolarBearModel`, `BabyPolarBearModel` | 128×64 6/10; 64×64 6/9 | adult has `MeshTransformer.scaling(1.2)` |
| Ravager | `RavagerRenderer`: `RavagerModel` | 128×128 10/12 | no baby layer |
| Sniffer | `SnifferRenderer`: `SnifferModel`, `SniffletModel` | 192×192 13/15; 128×128 13/15 | baby class is `SniffletModel` |
| Strider | `StriderRenderer`: `AdultStriderModel`, `BabyStriderModel` | 64×128 9/9; 32×32 6/6 | complete adult and baby layers |
| Llama | `LlamaRenderer`: `LlamaModel`, `BabyLlamaModel` | 128×64 8/11; 64×64 8/11 | same geometry supports base/decor with CubeDeformation |
| TraderLlama | `LlamaRenderer` (same model classes) | same | renderer sets `isTraderLlama`, uses separate equipment decoration layer |

Source class files were inspected by running `javap -classpath <26.2.jar> -p -c <mapped-class>` on every listed layer plus `LayerDefinitions`, `MeshTransformer`, `LayerDefinition`, and `PartDefinition`. Adult Camel's `createBodyMesh` helper was followed. Renderer bytecode was checked for all layer mappings, including inherited Zoglin/Hoglin selection and CamelHusk's adult-only renderer.

## Complete cube/pivot/UV evidence ledger

Entries below are the source insertion-ordered `PartDefinition` tree. `pivot` and `rotation` are the `PartPose` in model pixels/radians. Each cube has source `texOffs`, `addBox` origin and size, `CubeDeformation` amount, and mirror flag. `PartPose.ZERO` is written as zero. Hierarchy paths preserve the parent relation encoded into `EntityPart.parent`.

### `AdultArmadilloModel` — 64×64 atlas
- `body` pivot=(0, 21, 4) rot=(0, 0, 0) :: UV(0, 20) origin=(-4, -7, -10) size=(8, 8, 12) inflate=0.3 mirror=false
- `body` pivot=(0, 21, 4) rot=(0, 0, 0) :: UV(0, 40) origin=(-4, -7, -10) size=(8, 8, 12) inflate=0 mirror=false
- `body/tail` pivot=(0, -3, 1) rot=(0.5061, 0, 0) :: UV(44, 53) origin=(-0.5, -0.0865, 0.0933) size=(1, 6, 1) inflate=0 mirror=false
- `body/head` pivot=(0, -2, -11) rot=(0, 0, 0) (no cubes; parent node)
- `body/head/head_cube` pivot=(0, 0, 0) rot=(-0.3927, 0, 0) :: UV(43, 15) origin=(-1.5, -1, -1) size=(3, 5, 2) inflate=0 mirror=false
- `body/head/right_ear` pivot=(-1, -1, 0) rot=(0, 0, 0) (no cubes; parent node)
- `body/head/right_ear/right_ear_cube` pivot=(-0.5, 0, -0.6) rot=(0.1886, -0.3864, -0.0718) :: UV(43, 10) origin=(-2, -3, 0) size=(2, 5, 0) inflate=0 mirror=false
- `body/head/left_ear` pivot=(1, -2, 0) rot=(0, 0, 0) (no cubes; parent node)
- `body/head/left_ear/left_ear_cube` pivot=(0.5, 1, -0.6) rot=(0.1886, 0.3864, 0.0718) :: UV(47, 10) origin=(0, -3, 0) size=(2, 5, 0) inflate=0 mirror=false
- `right_hind_leg` pivot=(-2, 21, 4) rot=(0, 0, 0) :: UV(51, 31) origin=(-1, 0, -1) size=(2, 3, 2) inflate=0 mirror=false
- `left_hind_leg` pivot=(2, 21, 4) rot=(0, 0, 0) :: UV(42, 31) origin=(-1, 0, -1) size=(2, 3, 2) inflate=0 mirror=false
- `right_front_leg` pivot=(-2, 21, -4) rot=(0, 0, 0) :: UV(51, 43) origin=(-1, 0, -1) size=(2, 3, 2) inflate=0 mirror=false
- `left_front_leg` pivot=(2, 21, -4) rot=(0, 0, 0) :: UV(42, 43) origin=(-1, 0, -1) size=(2, 3, 2) inflate=0 mirror=false
- `cube` pivot=(0, 24, 0) rot=(0, 0, 0) :: UV(0, 0) origin=(-5, -10, -6) size=(10, 10, 10) inflate=0 mirror=false

### `BabyArmadilloModel` — 64×64 atlas
- `body` pivot=(0, 20, 0.5) rot=(0, 0, 0) :: UV(0, 0) origin=(-2.5, -2, -3.5) size=(5, 4, 7) inflate=0.3 mirror=false
- `body` pivot=(0, 20, 0.5) rot=(0, 0, 0) :: UV(0, 11) origin=(-2.5, -2, -3) size=(5, 4, 6) inflate=0 mirror=false
- `body/tail` pivot=(0, 0, 3.4) rot=(0, 0, 0) (no cubes; parent node)
- `body/tail/right_ear_cube` pivot=(0, 1.5, 1) rot=(-1.0472, 0, 0) :: UV(22, 11) origin=(-0.5, -0.5, -2) size=(1, 1, 4) inflate=0 mirror=false
- `body/head` pivot=(0, 0, -3.2) rot=(0, 0, 0) (no cubes; parent node)
- `body/head/head_cube` pivot=(0, 0, 0) rot=(0.7417649, 0, 0) :: UV(20, 17) origin=(-1, -2, -4) size=(2, 2, 4) inflate=0 mirror=false
- `body/head/head_cube/right_ear` pivot=(-1, -2, -0.3) rot=(-0.4363, -0.1134, 0.0524) :: UV(28, 8) origin=(-1.8, -2, 0) size=(2, 3, 0) inflate=0 mirror=true
- `body/head/head_cube/left_ear` pivot=(1, -2, -0.3) rot=(-0.4363, 0.1134, -0.0524) :: UV(28, 8) origin=(-0.2, -2, 0) size=(2, 3, 0) inflate=0 mirror=false
- `right_hind_leg` pivot=(-1.5, 22, 2.5) rot=(0, 0, 0) :: UV(20, 27) origin=(-1, 0, -1) size=(2, 2, 2) inflate=0 mirror=true
- `left_hind_leg` pivot=(1.5, 22, 2.5) rot=(0, 0, 0) :: UV(20, 27) origin=(-1, 0, -1) size=(2, 2, 2) inflate=0 mirror=false
- `right_front_leg` pivot=(1.5, 22, -1.5) rot=(0, 0, 0) :: UV(20, 23) origin=(-1, 0, -1) size=(2, 2, 2) inflate=0 mirror=false
- `left_front_leg` pivot=(-1.5, 22, -1.5) rot=(0, 0, 0) :: UV(24, 0) origin=(-1, 0, -1) size=(2, 2, 2) inflate=0 mirror=true
- `cube` pivot=(0, 20.7, 0.5) rot=(0, 0, 0) :: UV(0, 25) origin=(-3, -3, -3) size=(6, 6, 6) inflate=0.3 mirror=false

### `AdultCamelModel` — 128×128 atlas
- `body` pivot=(0, 4, 9.5) rot=(0, 0, 0) :: UV(0, 25) origin=(-7.5, -12, -23.5) size=(15, 12, 27) inflate=0 mirror=false
- `body/hump` pivot=(0, -12, -10) rot=(0, 0, 0) :: UV(74, 0) origin=(-4.5, -5, -5.5) size=(9, 5, 11) inflate=0 mirror=false
- `body/tail` pivot=(0, -9, 3.5) rot=(0, 0, 0) :: UV(122, 0) origin=(-1.5, 0, 0) size=(3, 14, 0) inflate=0 mirror=false
- `body/head` pivot=(0, -3, -19.5) rot=(0, 0, 0) :: UV(60, 24) origin=(-3.5, -7, -15) size=(7, 8, 19) inflate=0 mirror=false
- `body/head` pivot=(0, -3, -19.5) rot=(0, 0, 0) :: UV(21, 0) origin=(-3.5, -21, -15) size=(7, 14, 7) inflate=0 mirror=false
- `body/head` pivot=(0, -3, -19.5) rot=(0, 0, 0) :: UV(50, 0) origin=(-2.5, -21, -21) size=(5, 5, 6) inflate=0 mirror=false
- `body/head/left_ear` pivot=(2.5, -21, -9.5) rot=(0, 0, 0) :: UV(45, 0) origin=(-0.5, 0.5, -1) size=(3, 1, 2) inflate=0 mirror=false
- `body/head/right_ear` pivot=(-2.5, -21, -9.5) rot=(0, 0, 0) :: UV(67, 0) origin=(-2.5, 0.5, -1) size=(3, 1, 2) inflate=0 mirror=false
- `left_hind_leg` pivot=(4.9, 1, 9.5) rot=(0, 0, 0) :: UV(58, 16) origin=(-2.5, 2, -2.5) size=(5, 21, 5) inflate=0 mirror=false
- `right_hind_leg` pivot=(-4.9, 1, 9.5) rot=(0, 0, 0) :: UV(94, 16) origin=(-2.5, 2, -2.5) size=(5, 21, 5) inflate=0 mirror=false
- `left_front_leg` pivot=(4.9, 1, -10.5) rot=(0, 0, 0) :: UV(0, 0) origin=(-2.5, 2, -2.5) size=(5, 21, 5) inflate=0 mirror=false
- `right_front_leg` pivot=(-4.9, 1, -10.5) rot=(0, 0, 0) :: UV(0, 26) origin=(-2.5, 2, -2.5) size=(5, 21, 5) inflate=0 mirror=false

### `BabyCamelModel` — 64×64 atlas
- `body` pivot=(0, 7, 0) rot=(0, 0, 0) :: UV(0, 14) origin=(-4.5, -4, -8) size=(9, 8, 16) inflate=0 mirror=false
- `body/tail` pivot=(0, -1.5, 8.05) rot=(0, 0, 0) :: UV(50, 38) origin=(-1.5, -0.5, 0) size=(3, 9, 0) inflate=0 mirror=false
- `body/head` pivot=(0, 1, -7.5) rot=(0, 0, 0) :: UV(20, 0) origin=(-2.5, -3, -7.5) size=(5, 5, 7) inflate=0 mirror=false
- `body/head` pivot=(0, 1, -7.5) rot=(0, 0, 0) :: UV(0, 0) origin=(-2.5, -12, -7.5) size=(5, 9, 5) inflate=0 mirror=false
- `body/head` pivot=(0, 1, -7.5) rot=(0, 0, 0) :: UV(0, 14) origin=(-2.5, -12, -10.5) size=(5, 4, 3) inflate=0 mirror=false
- `body/head/right_ear` pivot=(-2.5, -11, -4) rot=(0, 0, 0) :: UV(37, 0) origin=(-3, -0.5, -1) size=(3, 1, 2) inflate=0 mirror=false
- `body/head/left_ear` pivot=(2.5, -11, -4) rot=(0, 0, 0) :: UV(47, 0) origin=(0, -0.5, -1) size=(3, 1, 2) inflate=0 mirror=false
- `right_front_leg` pivot=(-3, 11.5, -5.5) rot=(0, 0, 0) :: UV(36, 14) origin=(-1.5, -0.5, -1.5) size=(3, 13, 3) inflate=0 mirror=false
- `left_front_leg` pivot=(3, 11.5, -5.5) rot=(0, 0, 0) :: UV(48, 14) origin=(-1.5, -0.5, -1.5) size=(3, 13, 3) inflate=0 mirror=false
- `left_hind_leg` pivot=(3, 11.5, 5.5) rot=(0, 0, 0) :: UV(12, 38) origin=(-1.5, -0.5, -1.5) size=(3, 13, 3) inflate=0 mirror=false
- `right_hind_leg` pivot=(-3, 11.5, 5.5) rot=(0, 0, 0) :: UV(0, 38) origin=(-1.5, -0.5, -1.5) size=(3, 13, 3) inflate=0 mirror=false

### `AdultFoxModel` — 48×32 atlas
- `head` pivot=(-1, 16.5, -3) rot=(0, 0, 0) :: UV(1, 5) origin=(-3, -2, -5) size=(8, 6, 6) inflate=0 mirror=false
- `head/right_ear` pivot=(0, 0, 0) rot=(0, 0, 0) :: UV(8, 1) origin=(-3, -4, -4) size=(2, 2, 1) inflate=0 mirror=false
- `head/left_ear` pivot=(0, 0, 0) rot=(0, 0, 0) :: UV(15, 1) origin=(3, -4, -4) size=(2, 2, 1) inflate=0 mirror=false
- `head/nose` pivot=(0, 0, 0) rot=(0, 0, 0) :: UV(6, 18) origin=(-1, 2.01, -8) size=(4, 2, 3) inflate=0 mirror=false
- `body` pivot=(0, 16, -6) rot=(1.5707964, 0, 0) :: UV(24, 15) origin=(-3, 3.999, -3.5) size=(6, 11, 6) inflate=0 mirror=false
- `body/tail` pivot=(-4, 15, -1) rot=(-0.05235988, 0, 0) :: UV(30, 0) origin=(2, 0, -1) size=(4, 9, 5) inflate=0 mirror=false
- `right_hind_leg` pivot=(-5, 17.5, 7) rot=(0, 0, 0) :: UV(13, 24) origin=(2, 0.5, -1) size=(2, 6, 2) inflate=0.001 mirror=false
- `left_hind_leg` pivot=(-1, 17.5, 7) rot=(0, 0, 0) :: UV(4, 24) origin=(2, 0.5, -1) size=(2, 6, 2) inflate=0.001 mirror=false
- `right_front_leg` pivot=(-5, 17.5, 0) rot=(0, 0, 0) :: UV(13, 24) origin=(2, 0.5, -1) size=(2, 6, 2) inflate=0.001 mirror=false
- `left_front_leg` pivot=(-1, 17.5, 0) rot=(0, 0, 0) :: UV(4, 24) origin=(2, 0.5, -1) size=(2, 6, 2) inflate=0.001 mirror=false

### `BabyFoxModel` — 32×32 atlas
- `head` pivot=(0, 18.125, 0.125) rot=(0, 0, 0) :: UV(0, 0) origin=(-3, -2.125, -5.125) size=(6, 5, 5) inflate=0 mirror=false
- `head` pivot=(0, 18.125, 0.125) rot=(0, 0, 0) :: UV(18, 20) origin=(-1, 0.875, -7.125) size=(2, 2, 2) inflate=0 mirror=false
- `head` pivot=(0, 18.125, 0.125) rot=(0, 0, 0) :: UV(22, 8) origin=(-3, -4.125, -4.125) size=(2, 2, 1) inflate=0 mirror=false
- `head` pivot=(0, 18.125, 0.125) rot=(0, 0, 0) :: UV(22, 11) origin=(1, -4.125, -4.125) size=(2, 2, 1) inflate=0 mirror=false
- `right_hind_leg` pivot=(-1.5, 22, 4) rot=(0, 0, 0) :: UV(22, 4) origin=(-1, 0, -1) size=(2, 2, 2) inflate=0 mirror=false
- `left_hind_leg` pivot=(1.5, 22, 4) rot=(0, 0, 0) :: UV(22, 0) origin=(-1, 0, -1) size=(2, 2, 2) inflate=0 mirror=false
- `right_front_leg` pivot=(-1.5, 22, 0) rot=(0, 0, 0) :: UV(22, 4) origin=(-1, 0, -1) size=(2, 2, 2) inflate=0 mirror=false
- `left_front_leg` pivot=(1.5, 22, 0) rot=(0, 0, 0) :: UV(22, 0) origin=(-1, 0, -1) size=(2, 2, 2) inflate=0 mirror=false
- `body` pivot=(0, 20, 2) rot=(0, 0, 0) :: UV(0, 10) origin=(-2.5, -2, -3) size=(5, 4, 6) inflate=0 mirror=false
- `body/tail` pivot=(0, -0.5, 3) rot=(0, 0, 0) :: UV(0, 20) origin=(-1.5, -1.48, -1) size=(3, 3, 6) inflate=0 mirror=false

### `FrogModel` — 48×48 atlas
- `root` pivot=(0, 24, 0) rot=(0, 0, 0) (no cubes; parent node)
- `root/body` pivot=(0, -2, 4) rot=(0, 0, 0) :: UV(3, 1) origin=(-3.5, -2, -8) size=(7, 3, 9) inflate=0 mirror=false
- `root/body` pivot=(0, -2, 4) rot=(0, 0, 0) :: UV(23, 22) origin=(-3.5, -1, -8) size=(7, 0, 9) inflate=0 mirror=false
- `root/body/head` pivot=(0, -2, -1) rot=(0, 0, 0) :: UV(23, 13) origin=(-3.5, -1, -7) size=(7, 0, 9) inflate=0 mirror=false
- `root/body/head` pivot=(0, -2, -1) rot=(0, 0, 0) :: UV(0, 13) origin=(-3.5, -2, -7) size=(7, 3, 9) inflate=0 mirror=false
- `root/body/head/eyes` pivot=(-0.5, 0, 2) rot=(0, 0, 0) (no cubes; parent node)
- `root/body/head/eyes/right_eye` pivot=(-1.5, -3, -6.5) rot=(0, 0, 0) :: UV(0, 0) origin=(-1.5, -1, -1.5) size=(3, 2, 3) inflate=0 mirror=false
- `root/body/head/eyes/left_eye` pivot=(2.5, -3, -6.5) rot=(0, 0, 0) :: UV(0, 5) origin=(-1.5, -1, -1.5) size=(3, 2, 3) inflate=0 mirror=false
- `root/body/croaking_body` pivot=(0, -1, -5) rot=(0, 0, 0) :: UV(26, 5) origin=(-3.5, -0.1, -2.9) size=(7, 2, 3) inflate=-0.1 mirror=false
- `root/body/tongue` pivot=(0, -1.01, 1) rot=(0, 0, 0) :: UV(17, 13) origin=(-2, 0, -7.1) size=(4, 0, 7) inflate=0 mirror=false
- `root/body/left_arm` pivot=(4, -1, -6.5) rot=(0, 0, 0) :: UV(0, 32) origin=(-1, 0, -1) size=(2, 3, 3) inflate=0 mirror=false
- `root/body/left_arm/left_hand` pivot=(0, 3, -1) rot=(0, 0, 0) :: UV(18, 40) origin=(-4, 0.01, -4) size=(8, 0, 8) inflate=0 mirror=false
- `root/body/right_arm` pivot=(-4, -1, -6.5) rot=(0, 0, 0) :: UV(0, 38) origin=(-1, 0, -1) size=(2, 3, 3) inflate=0 mirror=false
- `root/body/right_arm/right_hand` pivot=(0, 3, 0) rot=(0, 0, 0) :: UV(2, 40) origin=(-4, 0.01, -5) size=(8, 0, 8) inflate=0 mirror=false
- `root/left_leg` pivot=(3.5, -3, 4) rot=(0, 0, 0) :: UV(14, 25) origin=(-1, 0, -2) size=(3, 3, 4) inflate=0 mirror=false
- `root/left_leg/left_foot` pivot=(2, 3, 0) rot=(0, 0, 0) :: UV(2, 32) origin=(-4, 0.01, -4) size=(8, 0, 8) inflate=0 mirror=false
- `root/right_leg` pivot=(-3.5, -3, 4) rot=(0, 0, 0) :: UV(0, 25) origin=(-2, 0, -2) size=(3, 3, 4) inflate=0 mirror=false
- `root/right_leg/right_foot` pivot=(-2, 3, 0) rot=(0, 0, 0) :: UV(18, 32) origin=(-4, 0.01, -4) size=(8, 0, 8) inflate=0 mirror=false

### `GoatModel` — 64×64 atlas
- `head` pivot=(1, 14, 0) rot=(0, 0, 0) :: UV(2, 61) origin=(-6, -11, -10) size=(3, 2, 1) inflate=0 mirror=false
- `head` pivot=(1, 14, 0) rot=(0, 0, 0) :: UV(2, 61) origin=(2, -11, -10) size=(3, 2, 1) inflate=0 mirror=true
- `head` pivot=(1, 14, 0) rot=(0, 0, 0) :: UV(23, 52) origin=(-0.5, -3, -14) size=(0, 7, 5) inflate=0 mirror=true
- `head/left_horn` pivot=(0, 0, 0) rot=(0, 0, 0) :: UV(12, 55) origin=(-0.01, -16, -10) size=(2, 7, 2) inflate=0 mirror=false
- `head/right_horn` pivot=(0, 0, 0) rot=(0, 0, 0) :: UV(12, 55) origin=(-2.99, -16, -10) size=(2, 7, 2) inflate=0 mirror=false
- `head/nose` pivot=(0, -8, -8) rot=(0.9599, 0, 0) :: UV(34, 46) origin=(-3, -4, -8) size=(5, 7, 10) inflate=0 mirror=false
- `body` pivot=(0, 24, 0) rot=(0, 0, 0) :: UV(1, 1) origin=(-4, -17, -7) size=(9, 11, 16) inflate=0 mirror=false
- `body` pivot=(0, 24, 0) rot=(0, 0, 0) :: UV(0, 28) origin=(-5, -18, -8) size=(11, 14, 11) inflate=0 mirror=false
- `left_hind_leg` pivot=(1, 14, 4) rot=(0, 0, 0) :: UV(36, 29) origin=(0, 4, 0) size=(3, 6, 3) inflate=0 mirror=false
- `right_hind_leg` pivot=(-3, 14, 4) rot=(0, 0, 0) :: UV(49, 29) origin=(0, 4, 0) size=(3, 6, 3) inflate=0 mirror=false
- `left_front_leg` pivot=(1, 14, -6) rot=(0, 0, 0) :: UV(49, 2) origin=(0, 0, 0) size=(3, 10, 3) inflate=0 mirror=false
- `right_front_leg` pivot=(-3, 14, -6) rot=(0, 0, 0) :: UV(35, 2) origin=(0, 0, 0) size=(3, 10, 3) inflate=0 mirror=false

### `BabyGoatModel` — 64×64 atlas
- `left_hind_leg` pivot=(1.5, 19.5, 3) rot=(0, 0, 0) :: UV(29, 12) origin=(-1, -0.5, -1) size=(2, 5, 2) inflate=0 mirror=false
- `right_hind_leg` pivot=(-1.5, 19.5, 3) rot=(0, 0, 0) :: UV(21, 12) origin=(-1, -0.5, -1) size=(2, 5, 2) inflate=0 mirror=false
- `right_front_leg` pivot=(-1.5, 19.5, -2) rot=(0, 0, 0) :: UV(21, 5) origin=(-1, -0.5, -1) size=(2, 5, 2) inflate=0 mirror=false
- `left_front_leg` pivot=(1.5, 19.5, -2) rot=(0, 0, 0) :: UV(29, 5) origin=(-1, -0.5, -1) size=(2, 5, 2) inflate=0 mirror=false
- `body` pivot=(0, 17.8, 0) rot=(0, 0, 0) :: UV(0, 10) origin=(-3, -2.3, -4.5) size=(6, 5, 9) inflate=0 mirror=false
- `body` pivot=(0, 17.8, 0) rot=(0, 0, 0) :: UV(0, 24) origin=(-2.5, -2.2, -4) size=(5, 4, 8) inflate=0 mirror=false
- `head` pivot=(0, 15.5, -3) rot=(0.4363, 0, 0) :: UV(0, 0) origin=(-2, -3.8126, -5.1548) size=(4, 4, 6) inflate=0 mirror=false
- `head/right_horn` pivot=(-1.5, -1.5, -1) rot=(-0.3926991, 0, 0) :: UV(24, 0) origin=(0, -4.5, 0) size=(1, 2, 1) inflate=0 mirror=true
- `head/left_horn` pivot=(-1.5, -1.5, -1) rot=(-0.3926991, 0, 0) :: UV(24, 0) origin=(2, -4.5, 0) size=(1, 2, 1) inflate=0 mirror=true
- `head/right_ear` pivot=(-1.7, -2.3126, 0.1452) rot=(0, -0.5236, 0) :: UV(0, 12) origin=(-2, -0.5, -0.5) size=(2, 1, 1) inflate=0 mirror=true
- `head/left_ear` pivot=(1.7, -2.3126, 0.1452) rot=(0, 0.5236, 0) :: UV(0, 12) origin=(0, -0.5, -0.5) size=(2, 1, 1) inflate=0 mirror=false
- `head/HeadMain` pivot=(0, -1.3126, -1.1548) rot=(0, 0, 0) :: UV(0, 0) origin=(-2, -2.5, -4) size=(4, 4, 6) inflate=0 mirror=false

### `HoglinModel` — 128×64 atlas
- `body` pivot=(0, 7, 0) rot=(0, 0, 0) :: UV(1, 1) origin=(-8, -7, -13) size=(16, 14, 26) inflate=0 mirror=false
- `body/mane` pivot=(0, -14, -7) rot=(0, 0, 0) :: UV(90, 33) origin=(0, 0, -9) size=(0, 10, 19) inflate=0.001 mirror=false
- `head` pivot=(0, 2, -12) rot=(0.87266463, 0, 0) :: UV(61, 1) origin=(-7, -3, -19) size=(14, 6, 19) inflate=0 mirror=false
- `head/right_ear` pivot=(-6, -2, -3) rot=(0, 0, -0.6981317) :: UV(1, 1) origin=(-6, -1, -2) size=(6, 1, 4) inflate=0 mirror=false
- `head/left_ear` pivot=(6, -2, -3) rot=(0, 0, 0.6981317) :: UV(1, 6) origin=(0, -1, -2) size=(6, 1, 4) inflate=0 mirror=false
- `head/right_horn` pivot=(-7, 2, -12) rot=(0, 0, 0) :: UV(10, 13) origin=(-1, -11, -1) size=(2, 11, 2) inflate=0 mirror=false
- `head/left_horn` pivot=(7, 2, -12) rot=(0, 0, 0) :: UV(1, 13) origin=(-1, -11, -1) size=(2, 11, 2) inflate=0 mirror=false
- `right_front_leg` pivot=(-4, 10, -8.5) rot=(0, 0, 0) :: UV(66, 42) origin=(-3, 0, -3) size=(6, 14, 6) inflate=0 mirror=false
- `left_front_leg` pivot=(4, 10, -8.5) rot=(0, 0, 0) :: UV(41, 42) origin=(-3, 0, -3) size=(6, 14, 6) inflate=0 mirror=false
- `right_hind_leg` pivot=(-5, 13, 10) rot=(0, 0, 0) :: UV(21, 45) origin=(-2.5, 0, -2.5) size=(5, 11, 5) inflate=0 mirror=false
- `left_hind_leg` pivot=(5, 13, 10) rot=(0, 0, 0) :: UV(0, 45) origin=(-2.5, 0, -2.5) size=(5, 11, 5) inflate=0 mirror=false

### `BabyHoglinModel` — 64×64 atlas
- `head` pivot=(0, 13, -7) rot=(0.8727, 0, 0) :: UV(0, 0) origin=(-5, -2.2605, -10.547) size=(10, 4, 12) inflate=0 mirror=false
- `head` pivot=(0, 13, -7) rot=(0.8727, 0, 0) :: UV(44, 29) origin=(-7, -4.0981, -8.4879) size=(2, 5, 2) inflate=0 mirror=false
- `head` pivot=(0, 13, -7) rot=(0.8727, 0, 0) :: UV(52, 29) origin=(5, -4.0981, -8.4879) size=(2, 5, 2) inflate=0 mirror=false
- `head/right_ear` pivot=(-5, -1, -1.5) rot=(0, 0, -0.8727) :: UV(32, 5) origin=(-5.1, -0.5, -2) size=(6, 1, 4) inflate=0 mirror=false
- `head/left_ear` pivot=(5, -1, -1.5) rot=(0, 0, 0.8727) :: UV(32, 0) origin=(-0.9, -0.5, -2) size=(6, 1, 4) inflate=0 mirror=true
- `body` pivot=(0, 24, 0) rot=(0, 0, 0) :: UV(0, 16) origin=(-4, -14, -7) size=(8, 8, 14) inflate=0.02 mirror=false
- `body` pivot=(0, 24, 0) rot=(0, 0, 0) :: UV(24, 39) origin=(0, -18, -8) size=(0, 6, 11) inflate=0.02 mirror=false
- `right_hind_leg` pivot=(-2.5, 18, 4.5) rot=(0, 0, 0) :: UV(0, 47) origin=(-1.5, 0, -1.5) size=(3, 6, 3) inflate=0 mirror=false
- `left_hind_leg` pivot=(2.5, 18, 4.5) rot=(0, 0, 0) :: UV(12, 47) origin=(-1.5, 0, -1.5) size=(3, 6, 3) inflate=0 mirror=false
- `right_front_leg` pivot=(-2.5, 18, -4.5) rot=(0, 0, 0) :: UV(0, 38) origin=(-1.5, 0, -1.5) size=(3, 6, 3) inflate=0 mirror=false
- `left_front_leg` pivot=(2.5, 18, -4.5) rot=(0, 0, 0) :: UV(12, 38) origin=(-1.5, 0, -1.5) size=(3, 6, 3) inflate=0 mirror=false

### `PandaModel` — 64×64 atlas
- `head` pivot=(0, 11.5, -17) rot=(0, 0, 0) :: UV(0, 6) origin=(-6.5, -5, -4) size=(13, 10, 9) inflate=0 mirror=false
- `head` pivot=(0, 11.5, -17) rot=(0, 0, 0) :: UV(45, 16) origin=(-3.5, 0, -6) size=(7, 5, 2) inflate=0 mirror=false
- `head` pivot=(0, 11.5, -17) rot=(0, 0, 0) :: UV(52, 25) origin=(3.5, -8, -1) size=(5, 4, 1) inflate=0 mirror=false
- `head` pivot=(0, 11.5, -17) rot=(0, 0, 0) :: UV(52, 25) origin=(-8.5, -8, -1) size=(5, 4, 1) inflate=0 mirror=false
- `body` pivot=(0, 10, 0) rot=(1.5707964, 0, 0) :: UV(0, 25) origin=(-9.5, -13, -6.5) size=(19, 26, 13) inflate=0 mirror=false
- `right_hind_leg` pivot=(-5.5, 15, 9) rot=(0, 0, 0) :: UV(40, 0) origin=(-3, 0, -3) size=(6, 9, 6) inflate=0 mirror=false
- `left_hind_leg` pivot=(5.5, 15, 9) rot=(0, 0, 0) :: UV(40, 0) origin=(-3, 0, -3) size=(6, 9, 6) inflate=0 mirror=false
- `right_front_leg` pivot=(-5.5, 15, -9) rot=(0, 0, 0) :: UV(40, 0) origin=(-3, 0, -3) size=(6, 9, 6) inflate=0 mirror=false
- `left_front_leg` pivot=(5.5, 15, -9) rot=(0, 0, 0) :: UV(40, 0) origin=(-3, 0, -3) size=(6, 9, 6) inflate=0 mirror=false

### `BabyPandaModel` — 64×64 atlas
- `body` pivot=(0, 18.5, 2.5) rot=(0, 0, 0) :: UV(0, 11) origin=(-4.5, -3.5, -5.5) size=(9, 7, 11) inflate=0 mirror=false
- `head` pivot=(0, 19, -3) rot=(0, 0, 0) :: UV(0, 0) origin=(-3.5, -3, -5) size=(7, 6, 5) inflate=0 mirror=false
- `head` pivot=(0, 19, -3) rot=(0, 0, 0) :: UV(24, 6) origin=(-2, 1, -6) size=(4, 2, 1) inflate=0 mirror=false
- `head` pivot=(0, 19, -3) rot=(0, 0, 0) :: UV(24, 0) origin=(-4.5, -4, -3.5) size=(3, 3, 1) inflate=0 mirror=false
- `head` pivot=(0, 19, -3) rot=(0, 0, 0) :: UV(33, 0) origin=(1.5, -4, -3.5) size=(3, 3, 1) inflate=0 mirror=false
- `right_hind_leg` pivot=(-3, 22, 6.5) rot=(0, 0, 0) :: UV(0, 34) origin=(-1.5, 0, -1.5) size=(3, 2, 3) inflate=0 mirror=false
- `left_hind_leg` pivot=(3, 22, 6.5) rot=(0, 0, 0) :: UV(12, 34) origin=(-1.5, 0, -1.5) size=(3, 2, 3) inflate=0 mirror=false
- `right_front_leg` pivot=(-3, 22, -1.5) rot=(0, 0, 0) :: UV(0, 29) origin=(-1.5, 0, -1.5) size=(3, 2, 3) inflate=0 mirror=false
- `left_front_leg` pivot=(3, 22, -1.5) rot=(0, 0, 0) :: UV(12, 29) origin=(-1.5, 0, -1.5) size=(3, 2, 3) inflate=0 mirror=false

### `PolarBearModel` — 128×64 atlas
- `head` pivot=(0, 10, -16) rot=(0, 0, 0) :: UV(0, 0) origin=(-3.5, -3, -3) size=(7, 7, 7) inflate=0 mirror=false
- `head` pivot=(0, 10, -16) rot=(0, 0, 0) :: UV(0, 44) origin=(-2.5, 1, -6) size=(5, 3, 3) inflate=0 mirror=false
- `head` pivot=(0, 10, -16) rot=(0, 0, 0) :: UV(26, 0) origin=(-4.5, -4, -1) size=(2, 2, 1) inflate=0 mirror=false
- `head` pivot=(0, 10, -16) rot=(0, 0, 0) :: UV(26, 0) origin=(2.5, -4, -1) size=(2, 2, 1) inflate=0 mirror=true
- `body` pivot=(-2, 9, 12) rot=(1.5707964, 0, 0) :: UV(0, 19) origin=(-5, -13, -7) size=(14, 14, 11) inflate=0 mirror=false
- `body` pivot=(-2, 9, 12) rot=(1.5707964, 0, 0) :: UV(39, 0) origin=(-4, -25, -7) size=(12, 12, 10) inflate=0 mirror=false
- `right_hind_leg` pivot=(-4.5, 14, 6) rot=(0, 0, 0) :: UV(50, 22) origin=(-2, 0, -2) size=(4, 10, 8) inflate=0 mirror=false
- `left_hind_leg` pivot=(4.5, 14, 6) rot=(0, 0, 0) :: UV(50, 22) origin=(-2, 0, -2) size=(4, 10, 8) inflate=0 mirror=false
- `right_front_leg` pivot=(-3.5, 14, -8) rot=(0, 0, 0) :: UV(50, 40) origin=(-2, 0, -2) size=(4, 10, 6) inflate=0 mirror=false
- `left_front_leg` pivot=(3.5, 14, -8) rot=(0, 0, 0) :: UV(50, 40) origin=(-2, 0, -2) size=(4, 10, 6) inflate=0 mirror=false

### `BabyPolarBearModel` — 64×64 atlas
- `body` pivot=(0, 17.5, 0) rot=(0, 0, 0) :: UV(0, 9) origin=(-4, -3.5, -6) size=(8, 7, 12) inflate=0 mirror=false
- `head` pivot=(0, 18.625, -5.75) rot=(0, 0, 0) :: UV(0, 0) origin=(-3, -2.625, -4.25) size=(6, 5, 4) inflate=0 mirror=false
- `head` pivot=(0, 18.625, -5.75) rot=(0, 0, 0) :: UV(20, 3) origin=(-2, 0.375, -6.25) size=(4, 2, 2) inflate=0 mirror=false
- `head` pivot=(0, 18.625, -5.75) rot=(0, 0, 0) :: UV(20, 0) origin=(-4, -3.625, -2.75) size=(2, 2, 1) inflate=0 mirror=false
- `head` pivot=(0, 18.625, -5.75) rot=(0, 0, 0) :: UV(26, 0) origin=(2, -3.625, -2.75) size=(2, 2, 1) inflate=0 mirror=false
- `right_hind_leg` pivot=(-2.5, 21.5, 4.5) rot=(0, 0, 0) :: UV(0, 34) origin=(-1.5, -0.5, -1.5) size=(3, 3, 3) inflate=0 mirror=false
- `left_hind_leg` pivot=(2.5, 21.5, 4.5) rot=(0, 0, 0) :: UV(12, 34) origin=(-1.5, -0.5, -1.5) size=(3, 3, 3) inflate=0 mirror=false
- `right_front_leg` pivot=(-2.5, 21.5, -4.5) rot=(0, 0, 0) :: UV(0, 28) origin=(-1.5, -0.5, -1.5) size=(3, 3, 3) inflate=0 mirror=false
- `left_front_leg` pivot=(2.5, 21.5, -4.5) rot=(0, 0, 0) :: UV(12, 28) origin=(-1.5, -0.5, -1.5) size=(3, 3, 3) inflate=0 mirror=false

### `RavagerModel` — 128×128 atlas
- `neck` pivot=(0, -7, 5.5) rot=(0, 0, 0) :: UV(68, 73) origin=(-5, -1, -18) size=(10, 10, 18) inflate=0 mirror=false
- `neck/head` pivot=(0, 16, -17) rot=(0, 0, 0) :: UV(0, 0) origin=(-8, -20, -14) size=(16, 20, 16) inflate=0 mirror=false
- `neck/head` pivot=(0, 16, -17) rot=(0, 0, 0) :: UV(0, 0) origin=(-2, -6, -18) size=(4, 8, 4) inflate=0 mirror=false
- `neck/head/right_horn` pivot=(-10, -14, -8) rot=(1.0995574, 0, 0) :: UV(74, 55) origin=(0, -14, -2) size=(2, 14, 4) inflate=0 mirror=false
- `neck/head/left_horn` pivot=(8, -14, -8) rot=(1.0995574, 0, 0) :: UV(74, 55) origin=(0, -14, -2) size=(2, 14, 4) inflate=0 mirror=true
- `neck/head/mouth` pivot=(0, -2, 2) rot=(0, 0, 0) :: UV(0, 36) origin=(-8, 0, -16) size=(16, 3, 16) inflate=0 mirror=false
- `body` pivot=(0, 1, 2) rot=(1.5707964, 0, 0) :: UV(0, 55) origin=(-7, -10, -7) size=(14, 16, 20) inflate=0 mirror=false
- `body` pivot=(0, 1, 2) rot=(1.5707964, 0, 0) :: UV(0, 91) origin=(-6, 6, -7) size=(12, 13, 18) inflate=0 mirror=false
- `right_hind_leg` pivot=(-8, -13, 18) rot=(0, 0, 0) :: UV(96, 0) origin=(-4, 0, -4) size=(8, 37, 8) inflate=0 mirror=false
- `left_hind_leg` pivot=(8, -13, 18) rot=(0, 0, 0) :: UV(96, 0) origin=(-4, 0, -4) size=(8, 37, 8) inflate=0 mirror=true
- `right_front_leg` pivot=(-8, -13, -5) rot=(0, 0, 0) :: UV(64, 0) origin=(-4, 0, -4) size=(8, 37, 8) inflate=0 mirror=false
- `left_front_leg` pivot=(8, -13, -5) rot=(0, 0, 0) :: UV(64, 0) origin=(-4, 0, -4) size=(8, 37, 8) inflate=0 mirror=true

### `SnifferModel` — 192×192 atlas
- `bone` pivot=(0, 5, 0) rot=(0, 0, 0) (no cubes; parent node)
- `bone/body` pivot=(0, 0, 0) rot=(0, 0, 0) :: UV(62, 68) origin=(-12.5, -14, -20) size=(25, 29, 40) inflate=0 mirror=false
- `bone/body` pivot=(0, 0, 0) rot=(0, 0, 0) :: UV(62, 0) origin=(-12.5, -14, -20) size=(25, 24, 40) inflate=0.5 mirror=false
- `bone/body` pivot=(0, 0, 0) rot=(0, 0, 0) :: UV(87, 68) origin=(-12.5, 12, -20) size=(25, 0, 40) inflate=0 mirror=false
- `bone/body/head` pivot=(0, 6.5, -19.48) rot=(0, 0, 0) :: UV(8, 15) origin=(-6.5, -7.5, -11.5) size=(13, 18, 11) inflate=0 mirror=false
- `bone/body/head` pivot=(0, 6.5, -19.48) rot=(0, 0, 0) :: UV(8, 4) origin=(-6.5, 7.5, -11.5) size=(13, 0, 11) inflate=0 mirror=false
- `bone/body/head/left_ear` pivot=(6.51, -7.5, -4.51) rot=(0, 0, 0) :: UV(2, 0) origin=(0, 0, -3) size=(1, 19, 7) inflate=0 mirror=false
- `bone/body/head/right_ear` pivot=(-6.51, -7.5, -4.51) rot=(0, 0, 0) :: UV(48, 0) origin=(-1, 0, -3) size=(1, 19, 7) inflate=0 mirror=false
- `bone/body/head/nose` pivot=(0, -4.5, -11.5) rot=(0, 0, 0) :: UV(10, 45) origin=(-6.5, -2, -9) size=(13, 2, 9) inflate=0 mirror=false
- `bone/body/head/lower_beak` pivot=(0, 2.5, -12.5) rot=(0, 0, 0) :: UV(10, 57) origin=(-6.5, -7, -8) size=(13, 12, 9) inflate=0 mirror=false
- `bone/right_front_leg` pivot=(-7.5, 10, -15) rot=(0, 0, 0) :: UV(32, 87) origin=(-3.5, -1, -4) size=(7, 10, 8) inflate=0 mirror=false
- `bone/right_mid_leg` pivot=(-7.5, 10, 0) rot=(0, 0, 0) :: UV(32, 105) origin=(-3.5, -1, -4) size=(7, 10, 8) inflate=0 mirror=false
- `bone/right_hind_leg` pivot=(-7.5, 10, 15) rot=(0, 0, 0) :: UV(32, 123) origin=(-3.5, -1, -4) size=(7, 10, 8) inflate=0 mirror=false
- `bone/left_front_leg` pivot=(7.5, 10, -15) rot=(0, 0, 0) :: UV(0, 87) origin=(-3.5, -1, -4) size=(7, 10, 8) inflate=0 mirror=false
- `bone/left_mid_leg` pivot=(7.5, 10, 0) rot=(0, 0, 0) :: UV(0, 105) origin=(-3.5, -1, -4) size=(7, 10, 8) inflate=0 mirror=false
- `bone/left_hind_leg` pivot=(7.5, 10, 15) rot=(0, 0, 0) :: UV(0, 123) origin=(-3.5, -1, -4) size=(7, 10, 8) inflate=0 mirror=false

### `SniffletModel` — 128×128 atlas
- `bone` pivot=(0, 24, 0) rot=(0, 0, 0) (no cubes; parent node)
- `bone/body` pivot=(6, -3, -9.5) rot=(0, 0, 0) :: UV(0, 35) origin=(-13, -14, -0.5) size=(14, 14, 20) inflate=0.25 mirror=false
- `bone/body` pivot=(6, -3, -9.5) rot=(0, 0, 0) :: UV(0, 0) origin=(-13, -14, -0.5) size=(14, 15, 20) inflate=0 mirror=false
- `bone/body` pivot=(6, -3, -9.5) rot=(0, 0, 0) :: UV(68, 0) origin=(-13, 0, -0.5) size=(14, 0, 20) inflate=0 mirror=false
- `bone/body/head` pivot=(-6, -4.75, 0) rot=(0, 0, 0) :: UV(68, 20) origin=(-5, -4.25, -7.5) size=(10, 9, 9) inflate=0 mirror=false
- `bone/body/head` pivot=(-6, -4.75, 0) rot=(0, 0, 0) :: UV(88, 20) origin=(-5, 3.75, -7.5) size=(10, 0, 9) inflate=0 mirror=false
- `bone/body/head/left_ear` pivot=(5, -4.25, -1.5) rot=(0, 0, 0) :: UV(104, 38) origin=(0, 0, -2) size=(1, 11, 3) inflate=0 mirror=false
- `bone/body/head/right_ear` pivot=(-5, -4.25, -1.5) rot=(0, 0, 0) :: UV(96, 38) origin=(-1, 0, -2) size=(1, 11, 3) inflate=0 mirror=false
- `bone/body/head/nose` pivot=(0, -1.25, -9.5) rot=(0, 0, 0) :: UV(68, 47) origin=(-5, -3, -2) size=(10, 3, 4) inflate=0 mirror=false
- `bone/body/head/lower_beak` pivot=(0, 1.25, -9.5) rot=(0, 0, 0) :: UV(68, 38) origin=(-5, -2.5, -2) size=(10, 5, 4) inflate=0 mirror=false
- `bone/right_front_leg` pivot=(-4, -4, -7) rot=(0, 0, 0) :: UV(0, 69) origin=(-2, -1, -2) size=(4, 5, 4) inflate=0 mirror=false
- `bone/right_mid_leg` pivot=(-4, -4, 0) rot=(0, 0, 0) :: UV(0, 78) origin=(-2, -1, -2) size=(4, 5, 4) inflate=0 mirror=false
- `bone/right_hind_leg` pivot=(-4, -4, 7) rot=(0, 0, 0) :: UV(0, 87) origin=(-2, -1, -2) size=(4, 5, 4) inflate=0 mirror=false
- `bone/left_front_leg` pivot=(4, -4, -7) rot=(0, 0, 0) :: UV(16, 69) origin=(-2, -1, -2) size=(4, 5, 4) inflate=0 mirror=false
- `bone/left_mid_leg` pivot=(4, -4, 0) rot=(0, 0, 0) :: UV(16, 78) origin=(-2, -1, -2) size=(4, 5, 4) inflate=0 mirror=false
- `bone/left_hind_leg` pivot=(4, -4, 7) rot=(0, 0, 0) :: UV(16, 87) origin=(-2, -1, -2) size=(4, 5, 4) inflate=0 mirror=false

### `AdultStriderModel` — 64×128 atlas
- `right_leg` pivot=(-4, 8, 0) rot=(0, 0, 0) :: UV(0, 32) origin=(-2, 0, -2) size=(4, 16, 4) inflate=0 mirror=false
- `left_leg` pivot=(4, 8, 0) rot=(0, 0, 0) :: UV(0, 55) origin=(-2, 0, -2) size=(4, 16, 4) inflate=0 mirror=false
- `body` pivot=(0, 1, 0) rot=(0, 0, 0) :: UV(0, 0) origin=(-8, -6, -8) size=(16, 14, 16) inflate=0 mirror=false
- `body/right_bottom_bristle` pivot=(-8, 4, -8) rot=(0, 0, -1.2217305) :: UV(16, 65) origin=(-12, 0, 0) size=(12, 0, 16) inflate=0 mirror=false
- `body/right_middle_bristle` pivot=(-8, -1, -8) rot=(0, 0, -1.134464) :: UV(16, 49) origin=(-12, 0, 0) size=(12, 0, 16) inflate=0 mirror=false
- `body/right_top_bristle` pivot=(-8, -5, -8) rot=(0, 0, -0.87266463) :: UV(16, 33) origin=(-12, 0, 0) size=(12, 0, 16) inflate=0 mirror=false
- `body/left_top_bristle` pivot=(8, -6, -8) rot=(0, 0, 0.87266463) :: UV(16, 33) origin=(0, 0, 0) size=(12, 0, 16) inflate=0 mirror=false
- `body/left_middle_bristle` pivot=(8, -2, -8) rot=(0, 0, 1.134464) :: UV(16, 49) origin=(0, 0, 0) size=(12, 0, 16) inflate=0 mirror=false
- `body/left_bottom_bristle` pivot=(8, 3, -8) rot=(0, 0, 1.2217305) :: UV(16, 65) origin=(0, 0, 0) size=(12, 0, 16) inflate=0 mirror=false

### `BabyStriderModel` — 32×32 atlas
- `body` pivot=(0, 16.75, 0) rot=(0, 0, 0) :: UV(0, 0) origin=(-3.5, -3.75, -4) size=(7, 7, 8) inflate=0 mirror=false
- `body/bristle0` pivot=(0, -4.25, 2) rot=(0, 0, 0) :: UV(0, 21) origin=(-3.5, -2.5, 0) size=(7, 3, 0) inflate=0 mirror=false
- `body/bristle1` pivot=(0, -4.25, 0) rot=(0, 0, 0) :: UV(0, 18) origin=(-3.5, -2.5, 0) size=(7, 3, 0) inflate=0 mirror=false
- `body/bristle2` pivot=(0, -4.25, -2) rot=(0, 0, 0) :: UV(0, 15) origin=(-3.5, -2.5, 0) size=(7, 3, 0) inflate=0 mirror=false
- `right_leg` pivot=(-1.5, 20, 0) rot=(0, 0, 0) :: UV(0, 24) origin=(-1, 0, -1) size=(2, 4, 2) inflate=0 mirror=false
- `left_leg` pivot=(1.5, 20, 0) rot=(0, 0, 0) :: UV(8, 24) origin=(-1, 0, -1) size=(2, 4, 2) inflate=0 mirror=false

### `LlamaModel` — 128×64 atlas
- `head` pivot=(0, 7, -6) rot=(0, 0, 0) :: UV(0, 0) origin=(-2, -14, -10) size=(4, 4, 9) inflate=parameter mirror=false
- `head` pivot=(0, 7, -6) rot=(0, 0, 0) :: UV(0, 14) origin=(-4, -16, -6) size=(8, 18, 6) inflate=parameter mirror=false
- `head` pivot=(0, 7, -6) rot=(0, 0, 0) :: UV(17, 0) origin=(-4, -19, -4) size=(3, 3, 2) inflate=parameter mirror=false
- `head` pivot=(0, 7, -6) rot=(0, 0, 0) :: UV(17, 0) origin=(1, -19, -4) size=(3, 3, 2) inflate=parameter mirror=false
- `body` pivot=(0, 5, 2) rot=(1.5707964, 0, 0) :: UV(29, 0) origin=(-6, -10, -7) size=(12, 18, 10) inflate=parameter mirror=false
- `right_chest` pivot=(-8.5, 3, 3) rot=(0, 1.5707964, 0) :: UV(45, 28) origin=(-3, 0, 0) size=(8, 8, 3) inflate=parameter mirror=false
- `left_chest` pivot=(5.5, 3, 3) rot=(0, 1.5707964, 0) :: UV(45, 41) origin=(-3, 0, 0) size=(8, 8, 3) inflate=parameter mirror=false
- `right_hind_leg` pivot=(-3.5, 10, 6) rot=(0, 0, 0) :: UV(29, 29) origin=(-2, 0, -2) size=(4, 14, 4) inflate=parameter mirror=false
- `left_hind_leg` pivot=(3.5, 10, 6) rot=(0, 0, 0) :: UV(29, 29) origin=(-2, 0, -2) size=(4, 14, 4) inflate=parameter mirror=false
- `right_front_leg` pivot=(-3.5, 10, -5) rot=(0, 0, 0) :: UV(29, 29) origin=(-2, 0, -2) size=(4, 14, 4) inflate=parameter mirror=false
- `left_front_leg` pivot=(3.5, 10, -5) rot=(0, 0, 0) :: UV(29, 29) origin=(-2, 0, -2) size=(4, 14, 4) inflate=parameter mirror=false

### `BabyLlamaModel` — 64×64 atlas
- `head` pivot=(0, 12, -4) rot=(0, 0, 0) :: UV(0, 0) origin=(-3, -9, -4) size=(6, 11, 4) inflate=parameter mirror=false
- `head` pivot=(0, 12, -4) rot=(0, 0, 0) :: UV(0, 15) origin=(-1.5, -7, -7) size=(3, 3, 3) inflate=parameter mirror=false
- `head` pivot=(0, 12, -4) rot=(0, 0, 0) :: UV(20, 4) origin=(0.5, -11, -3) size=(2, 2, 2) inflate=parameter mirror=false
- `head` pivot=(0, 12, -4) rot=(0, 0, 0) :: UV(20, 0) origin=(-2.5, -11, -3) size=(2, 2, 2) inflate=parameter mirror=false
- `right_hind_leg` pivot=(-2.5, 16.5, 4.5) rot=(0, 0, 0) :: UV(0, 45) origin=(-1.4, -0.5, -1.5) size=(3, 8, 3) inflate=parameter mirror=false
- `left_hind_leg` pivot=(2.5, 16.5, 4.5) rot=(0, 0, 0) :: UV(12, 45) origin=(-1.6, -0.5, -1.5) size=(3, 8, 3) inflate=parameter mirror=false
- `right_front_leg` pivot=(-2.5, 16.5, -3.5) rot=(0, 0, 0) :: UV(0, 34) origin=(-1.4, -0.5, -1.5) size=(3, 8, 3) inflate=parameter mirror=false
- `left_front_leg` pivot=(2.5, 16.5, -3.5) rot=(0, 0, 0) :: UV(12, 34) origin=(-1.6, -0.5, -1.5) size=(3, 8, 3) inflate=parameter mirror=false
- `body` pivot=(0, 14, 2.5) rot=(0, 0, 0) :: UV(0, 15) origin=(-4, -3, -8.5) size=(8, 6, 13) inflate=parameter mirror=false
- `right_chest` pivot=(-8.5, 4, 3) rot=(0, 1.5707964, 0) :: UV(45, 28) origin=(-3, 0, 0) size=(8, 8, 3) inflate=parameter mirror=false
- `left_chest` pivot=(5.5, 4, 3) rot=(0, 1.5707964, 0) :: UV(45, 41) origin=(-3, 0, 0) size=(8, 8, 3) inflate=parameter mirror=false

### PolarBear mesh transform

`PolarBearModel.createBodyLayer()` creates a 128×64 mesh and applies `MeshTransformer.scaling(1.2f)`. `MeshTransformer.scaling` calls `PartPose.scaled(1.2)` then `.translated(0, 24.016*(1-1.2), 0)` for every part pose. The Rust bake therefore transforms the mesh root only (PartDefinition.transformed copies its children unchanged), so descendants inherit root scale 1.2 and Y translation −4.8032. With no explicit mesh root in the Rust representation, the bake materializes the equivalent result: scale every source pivot, cube origin/size and deformation by 1.2, then add −4.8032 Y to each source top-level pivot; UVs and rotations stay unchanged. This is the transformer implementation, not a blanket guessed cube scale. Baby PolarBear has no transformer.

## Texture and metadata mapping (26.2)

PNG entries/dimensions were read from the exact jar under `assets/minecraft/textures`; presence alone was not used as geometry evidence.

| Mob/state | Texture paths (all below `textures/`) and atlas size | Exact synced metadata from `third_party/SteelMC/steel-registry/build_assets/entities.json` |
|---|---|---|
| Armadillo | `entity/armadillo/armadillo.png`, `_baby.png` (64×64 each) | baby bool 16; `armadillo_state` enum 18. Enum IDs: IDLE=0, ROLLING=1, SCARED=2, UNROLLING=3. |
| Camel | `entity/camel/camel.png` (128×128), `camel_baby.png` (64×64); equipment `entity/equipment/camel_saddle/saddle.png` | baby bool 16, `id_flags` byte 18, dash bool 19, last-pose-change long 20. |
| CamelHusk | `entity/camel/camel_husk.png` (128×128); `entity/equipment/camel_husk_saddle/saddle.png` | schema inherits Camel indices; renderer uses adult model only. |
| Fox | `{fox,fox_sleep,fox_snow,fox_snow_sleep}.png` (48×32) and matching `_baby.png` (32×32) | variant `type_id` int 18: RED=0, SNOW=1; flags byte 19; baby bool 16. Renderer selects texture by variant and sleep state. |
| Frog | `entity/frog/frog_{cold,temperate,warm}.png` (48×48) | holder variant serializer `frog_variant` index 18, default `minecraft:temperate`; tongue target optional unsigned int 19. Steel generated vanilla registry registration order is COLD=0, TEMPERATE=1, WARM=2; asset IDs are `entity/frog/frog_cold`, `...frog_temperate`, `...frog_warm`. Resolve holder ID, not unordered map order. |
| Goat | `entity/goat/goat.png`, `goat_baby.png` (64×64) | screaming bool 18, left horn bool 19, right horn bool 20; baby bool 16. |
| Hoglin/Zoglin | Hoglin/Zoglin adult (128×64) and `_baby` (64×64) | baby bool 16; Hoglin immunity bool 18. Hoglin and Zoglin share both model classes. |
| Panda | adult `panda.png`, `panda_lazy.png`, `panda_worried.png`, `panda_playful.png`, `panda_brown.png`, `panda_weak.png`, `panda_aggressive.png`; all 64×64. Baby renderer paths: `panda_baby.png`, `lazy_panda_baby.png`, `worried_panda_baby.png`, `playful_panda_baby.png`, `brown_panda_baby.png`, `weak_panda_baby.png`, `aggressive_panda_baby.png`, all 64×64. | main gene byte 21 and hidden gene byte 22. `Panda.Gene` IDs: NORMAL=0, LAZY=1, WORRIED=2, PLAYFUL=3, BROWN=4, WEAK=5, AGGRESSIVE=6. Main gene selects skin. |
| PolarBear | `entity/bear/polarbear.png` 128×64; `_baby.png` 64×64 | standing bool 18. |
| Ravager | `entity/illager/ravager.png` 128×128 | celebrating bool 16. |
| Sniffer | `entity/sniffer/sniffer.png` 192×192; `snifflet.png` 128×128 | baby bool 16; sniffer state enum 18; seed drop tick int 19. |
| Strider | `entity/strider/strider.png`, `strider_cold.png` 64×128; baby files 32×32 | baby bool 16; boost time int 18; suffocating bool 19. Cold appearance is renderer state, not another mesh. |
| Llama/TraderLlama | `entity/llama/llama_{creamy,white,brown,gray}.png` 128×64 and matching `_baby.png` 64×64. Equipment has `entity/equipment/llama_body/trader_llama.png` and `_baby.png`; 15 standard dye assets exist for that layer too. | baby bool 16, flags byte 18, chest bool 19, strength int 20, variant int 21. Variant IDs: CREAMY=0, WHITE=1, BROWN=2, GRAY=3. Both entity schemas agree. |

`LayerDefinitions` calls `LlamaModel.createBodyLayer(CubeDeformation.NONE)` for base geometry and `CubeDeformation(0.5)` for adult decor. Baby layers use NONE and `CubeDeformation(0.2)`. The source bake exposes base and decor versions with those exact values. `LlamaRenderer` adds `LlamaDecorLayer`, which uses `LLAMA_DECOR` / `LLAMA_BABY_DECOR` and equipment layer type `LLAMA_BODY`; trader texture is equipment, not a different body mesh.

## Mooshroom is intentionally unsupported in this geometry module

No `bake_mooshroom_*` function remains. `MushroomCowMushroomLayer.submit` (read with `javap -p -c`) receives `MushroomCowRenderState.mushroomModel: BlockModelRenderState`, skips babies/empty model, submits the block model twice at body positions and once after `CowModel.getHead()` transform. Back A: translate `(0.2,-0.35,0.5)`, Y rotate −48°, scale `(-1,-1,1)`, translate `(-0.5,-0.5,-0.5)`. Back B: same initial translate, Y rotate +42°, translate `(0.1,0,-0.6)`, Y rotate −48°, same scale and final translate. Head: call head `translateAndRotate`, translate `(0,-0.7,-0.2)`, Y rotate −78°, then same scale and final translate. It uses `BlockModelRenderState.submit`, i.e. the actual red/brown mushroom block model and block atlas. `MushroomCow.Variant` is RED id 0/default and BROWN id 1; synchronized `type` int index is 18. Correct integration patch: resolve RED_MUSHROOM/BROWN_MUSHROOM from variant through existing block-model/atlas plumbing; carry that `BlockModelRenderState` into render state; submit three instances at the exact vanilla transforms and gate for babies. Do not approximate with mushroom caps, invented UVs, or plain cow texture. Existing cow body bake can be evaluated separately; it does not complete the mushroom layer.

## Scope and checks

- Only `pomme-client/src/renderer/entity_models/terrestrial.rs`, `terrestrial_tests.rs`, and this report are owned files. No shared source, private helper visibility, module registration, dependencies, or integration patch changed.
- Geometry code is static rest pose; entity animation/pose behavior is not implemented.
- `terrestrial_tests.rs` checks source part/cube counts, nonempty vertices, and each cube UV origin against the actual model atlas; tests were not run.
- Build, cargo check, and tests were not run per instruction. Only standalone rustfmt and diff checks were performed.
- Mooshroom is the only target species not presented as a verified model bake here.
