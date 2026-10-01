# Flying / insect model implementation notes (26.2)

## Scope and source evidence

Added standalone model bakes in `pomme-client/src/renderer/entity_models/flying.rs`. `Bat` was excluded from new work because it is already registered in `entity_renderer.rs` (`EntityKind::Bat`, `AnimationType::Bat`) and has `entity_model::bake_bat_model()`; it is not a missing model after the requested `f43477c` baseline.

The installed `26.2.jar` is `C:/Users/yuzum/AppData/Roaming/.minecraft/versions/26.2/26.2.jar`. Its `net/minecraft/references/` entries are only ID classes, not model source. Read-only ZIP inspection confirmed these real texture keys and PNG dimensions:

| Entity | Texture key (under `assets/minecraft/textures/`) | PNG |
|---|---|---:|
| Allay | `entity/allay/allay.png` | 32x32 |
| Bee | `entity/bee/bee.png` | 64x64 |
| Blaze | `entity/blaze/blaze.png` | 64x32 |
| Breeze | `entity/breeze/breeze.png` | 32x32 |
| Ghast | `entity/ghast/ghast.png` | 128x64 |
| Happy Ghast | `entity/ghast/happy_ghast.png` | 128x64 |
| Phantom | `entity/phantom/phantom.png` | 64x64 |
| Vex | `entity/illager/vex.png` | 32x32 |
| Wither | `entity/wither/wither.png` | 64x64 |
| Ender Dragon | `entity/enderdragon/dragon.png` | 256x256 |

Related checked textures: bee state skins `bee_angry.png`, `bee_nectar.png`, `bee_angry_nectar.png`; baby counterparts `bee_baby.png`, `bee_angry_baby.png`, `bee_nectar_baby.png`, `bee_angry_nectar_baby.png`; `ghast/ghast_shooting.png`; `enderdragon/dragon_exploding.png`; `wither/wither_armor.png`; happy-ghast baby `ghast/happy_ghast_baby.png`; happy-ghast harnesses under `entity/equipment/happy_ghast_body/*_harness.png`. These are distinct texture/layer states, not separate geometry bakes.

## New API and geometry

All bakes return nonempty `BakedEntityModel`s using existing `bake_model`, `EntityPart` and `ModelCube`; all authored cubes use nonnegative UV offsets and vanilla pixel model coordinates. New public functions:

- `bake_allay_model`: head, torso, arms, two thin wings; renderer scale 0.35 is expected externally.
- `bake_bee_model`: body/head, stinger, six legs, paired thin wings. Adult base geometry only.
- `bake_blaze_model`: head/body and twelve separate orbit-rod parts.
- `bake_breeze_model`: head/body and four separated wind appendages.
- `bake_ghast_model`, `bake_happy_ghast_model`: distinct 16x body plus nine hanging tentacle parts. Happy-ghast harness is not part of base model.
- `bake_phantom_model`: head, long body, tail, paired wing and wing-tip parts, two legs.
- `bake_vex_model`: head, robe/body, arms and thin wings.
- `bake_wither_model`: three heads, central body, rib and lower segment.
- `bake_ender_dragon_model`: head/jaw, body, five neck and five tail segments, paired wings/tips, four legs.

`#[cfg(test)] flying_bakes_have_geometry_uvs_and_bounded_cubes` checks each model has parts and vertices, matching ranges, and positive-size cubes with nonnegative UV origins. This test was added but deliberately not run per task instruction.

## Texture/variant mappings needed when integrating MobDef

Use the keys above as direct `MobDef` texture keys (`minecraft:textures/...png` per existing `tex_table!` convention); do not use fallback pig/player assets. Bat remains its existing key `entity/bat/bat.png`.

- Allay: single skin; no variant pool.
- Bee: adult keys in consistent bit-state order: calm/no nectar `bee.png`, angry `bee_angry.png`, nectar `bee_nectar.png`, angry+nectar `bee_angry_nectar.png`. Baby versions have separate PNGs and need a baby variant pool/model policy. Current bake is adult proportions, so do not claim baby geometry support.
- Blaze/Breeze: single base skin.
- Ghast: calm `ghast.png`, shooting `ghast_shooting.png`; state selection must use synchronized attack state, not a registry variant.
- Happy Ghast: base `happy_ghast.png`; baby key `happy_ghast_baby.png` is an age texture and requires an explicit baby mapping. Harnesses are equipment overlays, not base variants.
- Phantom/Vex: charging/eyes are additional texture layers (`phantom_eyes.png`, `vex_charging.png`) if effects are later implemented; base maps are given above.
- Wither: base `wither.png`; invulnerability texture alternatives are `wither_invulnerable.png` and armor layer `wither_armor.png`.
- Ender Dragon: base `dragon.png`; exploding texture `dragon_exploding.png`, eyes layer `dragon_eyes.png`.

No variant-registry asset IDs are required for these static base models. Bee anger/nectar, Ghast shooting, Wither invulnerability, Dragon death/explosion, and Vex/Phantom state selection need live metadata/animation integration. This implementation does not register MobDefs or edit the living-mob spawn path.

## Exact integration handoff

Later integrator should add `pub mod flying;` under `renderer/entity_models` (currently unregistered) and dispatch the above `flying::bake_*_model()` functions from `entity_renderer.rs::mob_definitions()`. Do not edit `entity_model.rs` to expose local helpers: this file composes its own private `cube`/`part` helpers through existing public `bake_model`. There is no required visibility change or shared-file conflict. Add the texture keys and variant/state mapping above to their MobDefs; expected standalone species count is 10 new functions/models (Bat is preexisting).

Static shape is implemented, but moving geometry is not: Blaze rods, Breeze wind curls, Bee wings/legs, Allay/Vex wings and arm pose, Happy-Ghast/ghast tentacles, Phantom wings, Wither heads/flight, and Dragon neck/tail/wings need animation transforms; eye, equipment, charge, attack, and death layers are also not drawn by these base bakes. The imported `26.2.jar` does not provide Java model source under `net/minecraft/references`; exact vanilla ModelLayer UV-part/source comparison was unavailable. The bakes are hand-authored atlas-coordinate approximations and were not visually compared in-game.
