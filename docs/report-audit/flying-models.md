# Flying / insect models: Minecraft Java 26.2 source audit

## Java source discovery and extraction

The installed client jar is `C:/Users/yuzum/AppData/Roaming/.minecraft/versions/26.2/26.2.jar`. Model classes are actual `.class` entries in this jar, not under `net/minecraft/references/`. `py -3` ZIP enumeration identified these classes:

| Species | Model layer class (jar entry) | Other distinct model layers |
|---|---|---|
| Allay | `net/minecraft/client/model/animal/allay/AllayModel.class` | — |
| Bee | `net/minecraft/client/model/animal/bee/AdultBeeModel.class` | `BabyBeeModel`, `BeeModel` |
| Blaze | `net/minecraft/client/model/monster/blaze/BlazeModel.class` | — |
| Breeze | `net/minecraft/client/model/monster/breeze/BreezeModel.class` | `createWindLayer`, `createEyesLayer` in the same class |
| Ghast | `net/minecraft/client/model/monster/ghast/GhastModel.class` | — |
| Happy Ghast | `net/minecraft/client/model/animal/ghast/HappyGhastModel.class` | `HAPPY_GHAST_BABY`, `HappyGhastHarnessModel`, harness and rope layers |
| Phantom | `net/minecraft/client/model/monster/phantom/PhantomModel.class` | eyes layer is renderer texture layer |
| Vex | `net/minecraft/client/model/monster/vex/VexModel.class` | — |
| Wither | `net/minecraft/client/model/monster/wither/WitherBossModel.class` | armor layer |
| Ender Dragon | `net/minecraft/client/model/monster/dragon/EnderDragonModel.class` | eyes and death/exploding layers |

Full `javap -c -p` output for each listed model class (plus `BeeModel`, `BabyBeeModel`, `HappyGhastHarnessModel`, `LayerDefinitions`, and all ten species renderer classes) was captured outside the checkout at `C:/Users/yuzum/AppData/Local/Temp/26.2-model-javap/`. Command form:

```text
"C:/Program Files/Java/jdk-21/bin/javap.exe" -classpath "C:/Users/yuzum/AppData/Roaming/.minecraft/versions/26.2/26.2.jar" -c -p <fully.qualified.Class>
```

`javap` exited 0 for the 13 model classes listed in that capture. The installed JDK 21 can disassemble the jar’s Java 25 class files, but cannot compile/run them (class version 69 vs JDK 21 maximum 65); the attempted runtime check for ghast RNG therefore was not used. Ghast tentacle lengths were derived from the disassembled `SingleThreadedRandomSource` LCG (`createThreadLocalInstance(1660)`) and `nextInt(7)+8`.

## Geometry transcription and layer scales

`flying.rs` now uses the source part tree, pivots/rotations, cube origins/sizes, texture offsets, mirror flags, deformation, and texture-layer atlas sizes. Layer scales are applied through a synthetic root after UV baking so scaling does not distort UV rectangles.

| Bake | Source geometry/layer facts | Vanilla layer scale |
|---|---|---:|
| `bake_allay_model` | `AllayModel.createBodyLayer`, root at y=23.5; 5³ head; two body cubes including deformation -0.2; asymmetrically offset arms; 0×5×8 wings; 32×32 layer | 1.0; `AllayRenderer` constructor float 0.4 is shadow radius, not model scale |
| `bake_bee_model` | `AdultBeeModel.createBodyLayer`; body, antennae, stinger, 2 wings and 3 separately UV’d leg planes; 64×64 | 1.0 |
| `bake_baby_bee_model` | separate `BabyBeeModel.createBodyLayer`, not an adult mesh scale; baby body, antennal cuboids, wings and leg planes; 32×32 | 1.0 |
| `bake_blaze_model` | `BlazeModel.createBodyLayer`; 8³ head and 12 rods in three rings, using exact source trigonometric pivots; 64×32 | 1.0 |
| `bake_breeze_model` | `BreezeModel.createBodyLayer`; core head/eyes, three rods with source rotations and exact pivots; 32×32 | 1.0 |
| `bake_breeze_eyes_model` | `BreezeModel.createEyesLayer`; separate eye geometry; 32×32 | 1.0 |
| `bake_breeze_wind_model` | `BreezeModel.createWindLayer`; nested bottom/middle/top wind pieces; 128×128 | 1.0 |
| `bake_ghast_model` | `GhastModel.createBodyLayer`; 16³ body at y=17.6 and nine seed-1660 tentacles; 64×32 layer | 4.5 (`MeshTransformer.scaling(4.5)`) |
| `bake_happy_ghast_model` | `HappyGhastModel.createBodyLayer(false, CubeDeformation.NONE)`; 16³ body and nine individually dimensioned/placed tentacles; 64×64 | 4.0 (`MeshTransformer.scaling(4.0)`) |
| `bake_baby_happy_ghast_model` | separate `createBodyLayer(true, NONE)` includes `inner_body`, then `HAPPY_GHAST_BABY` transformer; 64×64 | 0.95 (= 4.0 × `BABY_TRANSFORMER` 0.2375) |
| `bake_happy_ghast_harness_model(baby)` | `HappyGhastHarnessModel.createHarnessLayer`; harness cube plus deformed goggles; 64×64 | 4.0 adult; 0.95 baby (`BABY_TRANSFORMER`) |
| `bake_phantom_model` | `PhantomModel.createBodyLayer`; torso, tail base/tip, left/right wing base/tip and head with source rotations; 64×64 | 1.0 base; size state is dynamic |
| `bake_vex_model` | `VexModel.createBodyLayer`; root/head/body, two body cubes, arms and mirrored wings; 32×32 | 1.0 |
| `bake_wither_model` | `WitherBossModel.createBodyLayer(CubeDeformation.ZERO)`; shoulders, rotated ribcage, tail and three heads; 64×64 | 1.0 |
| `bake_ender_dragon_model` | `EnderDragonModel.createBodyLayer`; head/jaw, five necks, twelve tail pieces, torso/scales, mirrored articulated wings and four articulated legs/feet; 256×256 | 1.0 |

Source model names, atlas dimensions, and part/cube values are also pinned by `vanilla_layer_golden_geometry` in `flying.rs`; it checks representative exact cube/UV tuples, source part counts, and baked ghast/Happy-Ghast scales, not a visual capture.

## Texture keys and variants/layers

All paths were checked as actual PNG entries in the same jar under `assets/minecraft/textures/`; texture dimensions are image dimensions (distinct from `LayerDefinition` atlas dimensions above).

| Model/state | Actual texture key |
|---|---|
| Allay | `entity/allay/allay.png` (32×32 PNG) |
| Bee adult, calm/no nectar | `entity/bee/bee.png` (64×64 PNG) |
| Bee adult, angry | `entity/bee/bee_angry.png` (64×64 PNG) |
| Bee adult, calm/nectar | `entity/bee/bee_nectar.png` (64×64 PNG) |
| Bee adult, angry/nectar | `entity/bee/bee_angry_nectar.png` (64×64 PNG) |
| Bee baby, calm/no nectar | `entity/bee/bee_baby.png` (32×32 PNG) |
| Bee baby, angry | `entity/bee/bee_angry_baby.png` (32×32 PNG) |
| Bee baby, calm/nectar | `entity/bee/bee_nectar_baby.png` (32×32 PNG) |
| Bee baby, angry/nectar | `entity/bee/bee_angry_nectar_baby.png` (32×32 PNG) |
| Blaze | `entity/blaze/blaze.png` (64×32 PNG) |
| Breeze skin / eyes / wind | `entity/breeze/breeze.png`, `entity/breeze/breeze_eyes.png`, `entity/breeze/breeze_wind.png` |
| Ghast calm / shooting | `entity/ghast/ghast.png`, `entity/ghast/ghast_shooting.png` |
| Happy Ghast adult / baby / ropes | `entity/ghast/happy_ghast.png` (128×128), `entity/ghast/happy_ghast_baby.png` (64×64), `entity/ghast/happy_ghast_ropes.png` (128×128) |
| Phantom skin / eyes | `entity/phantom/phantom.png`, `entity/phantom/phantom_eyes.png` |
| Vex normal / charging | `entity/illager/vex.png`, `entity/illager/vex_charging.png` |
| Wither base / armor | `entity/wither/wither.png`, `entity/wither/wither_armor.png`; also `entity/wither/wither_invulnerable.png` for invulnerability appearance |
| Ender Dragon skin / eyes / death | `entity/enderdragon/dragon.png`, `entity/enderdragon/dragon_eyes.png`, `entity/enderdragon/dragon_exploding.png` |

Use fixed skin texture keys for base MobDefs; do not treat state textures as registry variants. Bee uses a two-axis selector: angry chooses `bee_angry{_nectar}{_baby}.png`, calm chooses `bee{_nectar}{_baby}.png`; precisely map the eight paths from the table. Ghast boolean charging chooses its shooting texture. Happy Ghast baby selects both the baby geometry and baby texture; its equipment uses the `HAPPY_GHAST_BODY` layer (harness assets under `entity/equipment/happy_ghast_body/`) and its ropes texture is a separate layer. Breeze eyes/wind are separate meshes; Phantom eyes, Vex charging, Wither invulnerable/armor, and Dragon eyes/death are separate texture layers.

## Entity dimensions / rendering scale

The Steel `entities.json` dimensions are world collision/render-state dimensions, not model texture atlas size. 26.2 adult width×height (and baby where distinct): Allay 0.35×0.6; Bee 0.55×0.5 (baby 0.275×0.25); Blaze 0.6×1.8; Breeze 0.6×1.77; Ghast 4×4; Happy Ghast 4×4 (baby 0.95×0.95); Phantom 0.9×0.5; Vex 0.4×0.8; Wither 0.9×3.5; Ender Dragon 16×8. These do not replace source `MeshTransformer` factors: Ghast uses 4.5, Happy Ghast 4.0, and the Happy-Ghast baby layer multiplies 4.0 by 0.2375 (=0.95). The remaining listed model layers have no global source `MeshTransformer` scale; Phantom's `id_size` changes its runtime shape dynamically.

## Authoritative 26.2 metadata from Steel `entities.json`

Checked `third_party/SteelMC/steel-registry/build_assets/entities.json` entries and each `synched_data.layers` field’s index, name, serializer and default. These are the relevant fields (common Entity/LivingEntity/Mob fields omitted):

| Kind | Actual index/type | Meaning / mapping |
|---|---|---|
| Allay | 16 bool `dancing`; 17 bool `can_duplicate` | Animation/state only; neither selects texture |
| Bee | 16 bool `baby`; 18 byte `flags_id`; 19 long `anger_end_time` | Byte flag `0x08` = nectar, `0x04` = stung, `0x02` = rolling (`Bee` bytecode); angry state comes from anger timer, not a bit in flags. Adult/baby use separate texture keys and separate model layers. |
| Blaze | 16 byte `flags_id` | No texture-variant mapping used by this base bake |
| Breeze | no species field | Animation states come from Breeze entity/render state, not synchronized variant metadata |
| Ghast | 16 bool `is_charging` | false=`ghast.png`, true=`ghast_shooting.png` |
| Happy Ghast | 16 bool baby; 18 bool `is_leash_holder`; 19 bool `stays_still` | Baby chooses baby model/texture; leash/riding/ropes and equipment remain render-state/layer behavior |
| Phantom | 16 int `id_size` | Geometry size is dynamic; not a texture registry variant |
| Vex | 16 byte `flags_id` | `Vex.isCharging()` reads flag mask `0x01`; selects charging texture/pose |
| Wither | 16/17/18 int target ids; 19 int `id_inv` | `id_inv` is invulnerability ticks; nonzero selects invulnerable appearance |
| Ender Dragon | 16 int `phase`, default 10 | Phase controls animation/state; dying phase chooses exploding layer |

Types/indices above come from Steel JSON. Bee/Vex bit semantics were cross-checked against `Bee`/`Vex` class bytecode. Ghast/Wither/Dragon model/entity accessors were inspected with `javap` and agree with the Steel field names; do not renumber them during integration.

## Static-only limitations and integration

Static rest geometry is source-transcribed. Animation is intentionally not part of these bakes: Allay dance/spin and item pose; Bee flight/ground/facing; Blaze orbit motion; Breeze keyframe sequences and wind motion; Ghast tentacle motion; Happy-Ghast tentacles/riding/still pose; Phantom wings/tail and `id_size`; Vex wing/charging-arm pose; Wither heads/ribcage/tail; Ender Dragon phase-dependent pose are still needed. Texture overlays/equipment layers listed above are also not drawn by the base meshes.

No module registration or MobDef was added here, so this work does not expose these models for accidental use yet. Integrator should register `renderer/entity_models/flying.rs` as `pub mod flying;`, route adults and babies/layers to the distinct functions above, then use the exact texture/state mapping. This change only edits the owned `flying.rs` and this report file.
