# 26.2 nonliving projectile/thrown render reference

## Scope and 12-kind coverage

The plan's twelve `EntityKind`s are **DragonFireball, Egg, EnderPearl, ExperienceBottle, Fireball (LargeFireball), LingeringPotion, LlamaSpit, ShulkerBullet, SmallFireball, SplashPotion, Trident, WitherSkull**. These are nonliving projectile/throwable render states; none should be fed to `MobDef`/living-body selection, and `Fireball` must not be conflated with DragonFireball or SmallFireball.

26.2 mapped-jar `EntityRenderers` registration bytecode confirms renderer assignment:

| Kind | Vanilla renderer/model/material path | This patch |
|---|---|---|
| `dragon_fireball` | `DragonFireballRenderer`; camera-facing quad with `textures/entity/enderdragon/dragon_fireball.png`, scale ×2 and full block light 15 | keep as a distinct textured billboard; do not item-stack or 3-D cube it |
| `egg` | `ThrownItemRenderer`; item-model resolver, GROUND context, scale ×1 | vanilla item-stack mesh, exact Egg stack |
| `ender_pearl` | `ThrownItemRenderer`; item-model resolver, GROUND context, scale ×1 | vanilla item-stack mesh, exact Ender Pearl stack |
| `experience_bottle` | `ThrownItemRenderer`; item-model resolver, GROUND context, scale ×1 | vanilla item-stack mesh, exact bottle stack |
| `fireball` (large) | `ThrownItemRenderer`; item-model resolver, GROUND context, scale ×3, full block light 15 | vanilla item-stack mesh, large-fireball stack (not dragon texture) |
| `lingering_potion` | `ThrownItemRenderer`; item-model resolver, GROUND context, scale ×1 | vanilla item-stack mesh including potion components |
| `llama_spit` | `LlamaSpitRenderer` / `LlamaSpitModel`; 7 overlapping 2³ cubes; origin translated +0.15 Y then yaw/pitch aligned | `bake_llama_spit_model()` |
| `shulker_bullet` | `ShulkerBulletRenderer` / three crossed 8×8×2 plates; model is rotated/spun, half-scale plus renderer transform; `textures/entity/shulker/spark.png`, translucent | `bake_shulker_bullet_model()`; retain translucent material and animated orientation in extractor |
| `small_fireball` | `ThrownItemRenderer`; item-model resolver, GROUND context, scale ×0.75, full block light 15 when registered with `fullBright=true` | vanilla item-stack mesh, distinct kind and full-bright property |
| `splash_potion` | `ThrownItemRenderer`; item-model resolver, GROUND context, scale ×1 | vanilla item-stack mesh including potion components |
| `trident` | `ThrownTridentRenderer` / forked TridentModel, 32×32 texture `textures/entity/trident/trident.png`; yaw (yRot−90°), then roll Z (xRot+90°); has separate foil/glint draw when trident `isFoil` | `bake_trident_model()`; add the vanilla transforms, kind-specific texture and optional glint in downstream renderer |
| `wither_skull` | `WitherSkullRenderer` / SkullModel, one 8³ cube at UV (0,35), model flipped on X/Y; texture `textures/entity/wither/wither.png` or invulnerable texture; full block light 15; renderer picks invulnerable state | `bake_wither_skull_model()`; dangerous/charged boolean must select correct texture |

`EntityRenderers` also maps ordinary/spectral/tipped arrows to arrow renderer family, not thrown-item entities. Keep the existing arrow path distinct from this 12-kind table.

## Geometry transcribed into code

`pomme-client/src/renderer/entity_models/projectiles.rs` adds four `BakedEntityModel` bake functions using existing `ModelCube`, `EntityPart`, and `bake_model`:

- `bake_trident_model`: root pole UV (0,6), cube (-.5,2,-.5) size (1,25,1); base UV (4,0), size (3,2,1); three fork spikes UV (4,3)/(0,0), each 1×4×1 (right spike mirrors the UVs). Layer is 32×32. The root owns the four fork children so model transforms are applied to the full weapon.
- `bake_shulker_bullet_model`: three intersecting axis-aligned plates, UV (0,0), (0,10), (20,0), dimensions 8×8×2, 2×8×8, 8×2×8, texture atlas size 64×32.
- `bake_wither_skull_model`: one head cube at (-4,-8,-4), dimensions 8³, UV (0,35), 64×64.
- `bake_llama_spit_model`: seven overlapping 2³ cubes at the exact origins read from `LlamaSpitModel.createBodyLayer`, UV origin (0,0), 64×32.

A unit conversion is intentional: these are vanilla model pixels, matching this repo's existing `bake_model` entity convention. Renderer orientation, world-space scale, texture/shader selection, item atlas lookup and per-entity interpolation belong in the downstream extractor/pipeline, not baked geometry.

The file includes `projectile_meshes_bake_nonempty_with_expected_uv_sizes` structural assertions: all four meshes nonempty, expected model part counts and finite positions / nonzero packed UV data. It is not executed here per request.

### Integration boundary

The user's file allowlist forbids modifying the existing module declaration at `pomme-client/src/renderer/mod.rs::entity_models`. Rust will not compile/reference the new file until the integration owner adds `pub mod projectiles;` (or the matching private module declaration) to that inline module. No existing/shared renderer, core, network, entity-state or pipeline file was changed. This is an intentional, reported integration gap, not a claim the new module is already compiled.

## Texture and inventory policy

The supplied jar has all code-defined texture paths confirmed by ZIP listing:

- `assets/minecraft/textures/entity/enderdragon/dragon_fireball.png`
- `assets/minecraft/textures/entity/shulker/spark.png`
- `assets/minecraft/textures/entity/llama/llama_spit.png`
- `assets/minecraft/textures/entity/trident/trident.png` and `trident_riptide.png`
- `assets/minecraft/textures/entity/wither/wither.png`, `wither_invulnerable.png`, `wither_armor.png`
- `assets/minecraft/textures/item/egg.png`, `ender_pearl.png`, `experience_bottle.png`, `lingering_potion.png`, `splash_potion.png`, `fire_charge.png`

Vanilla `ThrownItemRenderer.extractRenderState` obtains `ItemSupplier.getItem()` and calls `ItemModelResolver.updateForNonLiving(..., ItemDisplayContext.GROUND, entity)`. Thus the correct local equivalent is the existing item-model/atlas draw path with the actual synchronized stack (components included); it is **not** a hardcoded texture sprite. Egg/pearl/bottle/potions / fireball item icons and model layers must be resolved through the existing item pipeline. A blank/default stack fallback is incorrect. `DragonFireballRenderer` is instead a one-texture camera quad. Jar `EntityRenderers` assignments are the evidence for which item kinds use item-model resolution and the per-class renderer; the listing above confirms PNGs only, not image pixels/atlas visual correctness.

## Animation, orientation and state spec

- **Item-based throwables:** preserve full entity metadata stack at sync index **8** (registry schema: Egg, EnderPearl, ExperienceBottle, SplashPotion, LingeringPotion, Fireball and SmallFireball). Draw with `GROUND` context, vanilla scale above, and normal item shader/material/component behavior. These models are deliberately billboard-capable because vanilla's thrown item uses a camera-oriented item pose; do not rotate the item by projectile yaw/pitch as if it were an arrow. Keep potion contents/color from the stack's potion components. Large Fireball uses scale 3 and fullbright; SmallFireball uses scale .75 and fullbright. Fireball kind is not material classification.
- **DragonFireball:** renderer is camera orientation, 2×, `entityCutout`/cutout texture layer, block light 15. Do not give it Fireball's item stack or full item glint.
- **SmallFireball:** different entity kind from dragon/large fireball; uses item model (`scale=0.75`) but renderer requests full block light 15. Do not copy dragon-fireball's hand-built quad shader.
- **Llama spit:** entity yaw/pitch aligns the 2-pixel spit's flight direction; origin has +0.15 Y offset from renderer. Retain projectile kind-specific shader and rotation; do not reuse fireball billboard.
- **Shulker bullet:** 26.2 has the distinctive three-plate model, `textures/entity/shulker/spark.png`, translucent layer, scale/spin transforms, interpolated yaw/pitch and an additional sinusoidal roll in renderer/model state. Its full state extraction also tracks attached shulker/target and peek/animation where available; current network state lacks these values, so implement deterministic orientation from packet velocity first and mark target-driven animation missing. Keep actual hit/movement packet transform authoritative.
- **Trident:** retain AbstractArrow's interpolated `xRot`,`yRot`, in-ground state, and thrown-trident renderer's exact yaw `yRot-90°` followed by roll `xRot+90°`; do not substitute ArrowModel. Trident foil flag is inherited synced data index **12** (with common entity fields before arrow fields: 8 flags, 9 pierce, 10 in-ground, 11 loyalty, 12 foil in the 26.2 registry); when true, perform entity-glint overlay draw. Vanilla's flying Trident uses TridentModel, not the player's inventory/held item model. On returning/loyalty state, server position remains authority; avoid predicting independent spin absent required state.
- **Wither skull:** keep its 8³ skull body, pitch/yaw and model X/Y inversion. `WitherSkull` synced field index **8** is dangerous/invulnerable; vanilla texture choice is `wither_invulnerable.png` for invulnerable, otherwise `wither.png`. The in-repo state currently doesn't carry this typed bit, so texture/state selector is an integration requirement. Vanilla WitherSkullRenderer `getBlockLightLevel` returns 15 (fullbright); keep this class behavior and select invulnerable texture when its dangerous flag is set.
- **Arrow / SpectralArrow / tipped-arrow appearance:** the existing ArrowModel is two crossed 16×4 planes plus a 5×5 fletching plane, with 32×32 UVs. Protocol/schema entity kinds are `arrow` (id 6) and `spectral_arrow`; there is **no** `TippedArrow` entity type or `EntityKind::TippedArrow`. Mapped schema says `arrow` is class `Arrow extends AbstractArrow`; synced index **11** is `id_effect_color`, serializer **Int**, default **-1**. This Arrow int controls vanilla `Arrow.getColor() > 0`; `TippableArrowRenderer` then selects `arrow_tipped.png` instead of `arrow.png`. Preserve it as Arrow metadata and select the texture from its positive value—do not add an entity kind or infer potion NBT effects from the color. `SpectralArrowRenderer` always uses `arrow_spectral.png`; the SpectralArrow schema has no index-11 color. The three arrow PNGs are 32×32 with no `.png.mcmeta` (static, no texture frame-rate animation). Preserve Arrow metadata indices 8 flags, 9 pierce, 10 in-ground, 11 Int effect color; Trident has its separate inherited layout (index 11 loyalty, 12 foil). Dispatch metadata by actual entity kind.
- **Arrow shake animation:** mapped `ArrowRenderer.extractRenderState` computes `shake = AbstractArrow.shakeTime - partialTick`. The renderer, when positive, applies a Z rotation of `-sin(shake*3) * shake` degrees before model draw. Existing Rust `EntityRenderInfo` has no throw animation/shake field (search result: no `ThrowAnimationState`/`throw_animation` symbol); following owners must add `shake_time_remaining` to the **arrow-only** extract state if entity metadata/tick source can supply it. Do not make up a generic throw-animation state or apply shake to eggs, fireballs or tridents. The server-facing data/packet path's ability to retain shake ticks was not verified in this asset/model-only task.

## `projectilemobKind` / type classification

The exact 12 are renderer-special, all in `EntityStore.vehicles`/nonliving spawn handling, never `is_living_mob`, `MobDef`, generic cube or pig fallback. Keep kind switches distinct:

1. `item_billboard`: Egg, EnderPearl, ExperienceBottle, LingeringPotion, Fireball, SmallFireball, SplashPotion → full `ItemStack` + existing item-model resolver/atlas. Render scales 1/3/.75 per table; Fireball and SmallFireball are fullbright.
2. `special_mesh`: LlamaSpit, ShulkerBullet, Trident, WitherSkull → bake functions/new model and class texture/shader/pose state.
3. `special_billboard`: DragonFireball → camera-facing quad, dragon texture, cutout + block light 15.

`Fireball` and `SmallFireball` carry synced `ItemStack` index 8 per 26.2 schema, just like the thrown item family; DragonFireball has no such field. `WitherSkull` bool index 8 does not mean item stack. ShulkerBullet and LlamaSpit do not have per-kind synced payloads beyond common entity metadata in the local generated registry. Trident has the inherited arrow metadata layout above. Do not use an existing generic `projectile` state currently enabled only for Arrow, SpectralArrow, Snowball as the definition of this 12-kind classification; extend that state only with kind guards and don't let Snowball's item path consume all unknown projectile kinds.

## Verification evidence / hash

Mapped JAR: `C:/Users/yuzum/AppData/Roaming/.minecraft/versions/26.2/26.2.jar`, ZIP size **39,193,383 bytes**. SHA-256 (command `sha256sum '/c/Users/yuzum/AppData/Roaming/.minecraft/versions/26.2/26.2.jar'`): `40896ee9f1e2bec3c934daac7e93d41e9e3d9c2f8ae0ca366d52ffbfd1afa290`; jar file exists locally.

Commands used (from repo shell):

- `unzip -l .../26.2.jar | grep -Ei '(ArrowRenderer|TridentRenderer|ShulkerBulletRenderer|FireballRenderer|ThrownItemRenderer|...)'` — confirmed mapped `.class` names including `ArrowRenderer`, `DragonFireballRenderer`, `LlamaSpitRenderer`, `ShulkerBulletRenderer`, `ThrownItemRenderer`, `ThrownTridentRenderer`, `WitherSkullRenderer`, `SpectralArrowRenderer`, `TippableArrowRenderer`.
- `'/c/Program Files/Java/jdk-21/bin/javap.exe' -classpath '.../26.2.jar' -c -p <classes>` — exit **0**; decompiled renderer `submit`/`extractRenderState` and model `createLayer` code for transforms, scale, UV texture offsets, model cube dimensions, shader/light flags and exact renderer classes.
- `unzip -l .../26.2.jar | grep -E 'assets/minecraft/textures/(entity/(trident|shulker/spark|wither/|enderdragon/dragon_fireball)|item/...)'` — exit **0**; texture paths above exist. `unzip -p ... <path> | file -` confirmed Arrow normal/spectral/tipped are each 32×32, Trident 32×32, Shulker spark 64×32 and Dragon fireball 16×16; arrow paths have no `.png.mcmeta` entry. This does not validate pixel/UV visual appearance.
- `grep -R 'ThrowAnimation\|throw_animation\|throwanimation' pomme-client/src` — no matches; no existing throw-animation state in checked Rust source.

Asset image dimensions/pixel inspection remains outstanding; SHA-256 above is verified.

## Explicitly not completed

- No extractor, item mesh drawing, material/shader registration, index8 metadata storage, Trident foil/Wither dangerous texture selector, Fireball/SmallFireball brightness, vanilla interpolation/spin integration, Arrow shake sync source, Arrow index-11 effect-color extraction and tipped-texture selection, or projectile kind dispatch integration (prohibited shared renderer/core/net/state edits in this assignment).
- Rust module not registered in `renderer/mod.rs` as prohibited; downstream integration must declare it, reuse existing item renderer, route kind-specific state, and call the four bake functions.
- No Cargo fmt, build/check, test, or runtime/gallery was run (explicit instruction: fmt/diff only, no build/test/module registration check). Model tests are present but unexecuted.
