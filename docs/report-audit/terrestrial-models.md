# Terrestrial model bake tranche (26.2)

## Scope and existing coverage

This file introduces static bakes for `Armadillo`, `Camel`, `CamelHusk`, `Fox`, `Frog`, `Goat`, `Hoglin`, `Zoglin`, `Mooshroom`'s mushroom layer, `Panda`, `PolarBear`, `Ravager`, `Sniffer`, `Strider`, `Llama`, and `TraderLlama`. Existing `entity_renderer.rs` definitions already cover `Ocelot`, `Rabbit`, `Donkey`, `Mule`, `SkeletonHorse`, and `ZombieHorse`; no duplicate bake is proposed for them. CamelHusk shares the Camel body bake but uses its own texture.

The new file is standalone and deliberately not module-registered. `renderer/mod.rs`, `entity_model.rs`, `pipelines/entity_renderer.rs`, entity/core/net code remain untouched. Integration must add `pub mod terrestrial;` from the appropriate `entity_models` module after creating/registering that parent module; the current checkout has no `renderer/entity_models/mod.rs` yet.

## Bakes added

| Function | Family/texture atlas dimensions | Notes |
|---|---:|---|
| `bake_armadillo_model` | 64x64 | body shell/head/feet; curled animation and baby bake absent |
| `bake_camel_model`, `bake_camel_husk_model` | 128x128 | shared body shape; no seat/saddle layer, baby model or sitting pose |
| `bake_fox_model` | 48x32 | geometry follows mapped 26.2 `AdultFoxModel` head/body/leg/tail part coordinates; sleep/snow state textures and baby bake not selected here |
| `bake_frog_model` | 48x48 | static body/head/feet; tongue/jump animation absent |
| `bake_goat_model` | 64x64 | cow-like base and horn geometry; ram/horn-broken pose state absent |
| `bake_hoglin_model`, `bake_zoglin_model` | 128x64 | separate exported bakes, shared coarse family shape; baby geometry/animation absent |
| `bake_mooshroom_mushrooms_model` | 64x32 declared | real 3-cap/stem overlay geometry, distinct from cow body. The caps use block mushroom appearance; integration must draw it as a separate layer with the correct mushroom material/UV atlas. Red/brown body texture alone is not completion. |
| `bake_panda_model` | 64x64 | base body only; gene-specific texture/state and poses absent |
| `bake_polar_bear_model` | 128x64 | static bear body; baby/attack pose absent |
| `bake_ravager_model` | 128x128 | static head/body/legs; roar/attack/charge state absent |
| `bake_sniffer_model` | 192x192 | static head/body/feet; snifflet model and sniff/dig animation absent |
| `bake_strider_model` | 64x128 | static body/legs; cold/baby variants, saddle, and walk pose absent |
| `bake_llama_model(trader)` | 128x64 | common body bake; trader flag does not alter geometry; carpets/decorations and baby scale/model absent |

**Important completion limitation:** these are first-pass, static family meshes, not a claim of fully exact vanilla parity. The FOX bake's adult part coordinates were cross-checked against `javap -p -c` output; other model cube UV layouts/part dimensions have not yet been transcribed and compared cube-for-cube. Several bakes use placeholder UV origins and family approximations; before MobDef registration they need an exact per-class `createBodyLayer()` audit and UV assignment. Test coverage only proves nonempty vertices and in-atlas UV origins, not visual/vanilla equivalence. Do not describe this tranche as satisfying exact-geometry acceptance until that audit is finished.

## Verified 26.2 texture assets

Read directly from `C:/Users/yuzum/AppData/Roaming/.minecraft/versions/26.2/26.2.jar` under `assets/minecraft/textures/` (PNG IHDR width/height):

- `entity/armadillo/armadillo.png` 64x64; `armadillo_baby.png` 64x64.
- `entity/camel/camel.png` 128x128; `camel_baby.png` 64x64; `camel_husk.png` 128x128. `entity/equipment/camel_saddle/saddle.png` and `camel_husk_saddle/saddle.png` exist as distinct equipment layers.
- `entity/fox/fox.png`, `fox_sleep.png`, `fox_snow.png`, `fox_snow_sleep.png` 48x32; corresponding `_baby` files 32x32.
- `entity/frog/frog_cold.png`, `frog_temperate.png`, `frog_warm.png` 48x48 each.
- `entity/goat/goat.png`, `goat_baby.png` 64x64.
- `entity/hoglin/hoglin.png`, `zoglin.png` 128x64; `hoglin_baby.png`, `zoglin_baby.png` 64x64.
- `entity/cow/mooshroom_red.png`, `mooshroom_brown.png`, and both `_baby` files 64x64 each.
- `entity/panda/panda.png`, `panda_aggressive.png`, `panda_brown.png`, `panda_lazy.png`, `panda_playful.png`, `panda_weak.png`, `panda_worried.png` and `panda_baby.png` are 64x64. Gene-specific baby assets exist for aggressive/brown/lazy/playful/weak/worried; exact asset names are listed by the jar query recorded below.
- `entity/bear/polarbear.png` 128x64; `polarbear_baby.png` 64x64.
- `entity/illager/ravager.png` 128x128.
- `entity/sniffer/sniffer.png` 192x192; `snifflet.png` 128x128.
- `entity/strider/strider.png`, `strider_cold.png` 64x128; adult/baby cold/normal variants present (baby 32x32).
- `entity/llama/llama_{brown,creamy,gray,white}.png` 128x64 and matching `_baby.png` files 64x64.

The texture existence/dimension audit command used Python 3.11 `zipfile` + PNG IHDR extraction against the exact jar. Command exited 0. These findings verify asset presence, not correct model UV mapping, sampling, or use of each variant at runtime.

## Variant metadata / integration requirements

No metadata mapping is implemented in this geometry-only file. Determine the exact per-entity synced-data accessor IDs from 26.2 mapped entity classes and protocol serializer definitions before connecting variants. In particular, Frog variant, Panda gene/hidden gene, Llama variant and Mooshroom type require a stable mapping from synced holder/enum to texture key; don't assume registry order or clamp to an arbitrary texture. Current asset sets: frog `{cold,temperate,warm}`; llama `{brown,creamy,gray,white}`; panda `{normal,aggressive,brown,lazy,playful,weak,worried}`; Mooshroom `{red,brown}`. CamelHusk differs from Camel by texture; Fox's sleep/snow texture depends on state; Strider has cold/adult/baby variants; Hoglin/Zoglin and Armadillo/Camel/Goat/Panda/PolarBear have distinct baby files. Exact wire indices and adult/baby selection are intentionally **unverified and unimplemented** here.

## Checks and known gaps

- Added `terrestrial_tests.rs`: checks all declared bakes produce vertices and UV origins start within expected texture dimensions. It is not run per task instructions.
- No build/test/cargo check run, as requested. Rustfmt and diff checks only.
- No baby-specific geometry except existing unrelated models; no animation/pose behavior beyond static rest shape.
- Mooshroom overlay requires a correct block-mushroom texture source and separate draw/layer registration; the current model's UVs/material are not yet production-valid.
- Several static meshes and UVs are placeholders. Required next step: use 26.2 mapped `net.minecraft.client.model.*` `createBodyLayer` bytecode (`javap -p -c`) to transcribe exact part trees, cube origins/sizes/UVs and texture dimensions, then compare generated vertices and add adult/baby/overlay definitions in the integration patch.
