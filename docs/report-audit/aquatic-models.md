# Aquatic model family (26.2) implementation/integration notes

Implemented new bake APIs in `pomme-client/src/renderer/entity_models/aquatic.rs`; module declaration and MobDef registration are intentionally left to the integration agent (shared files are out of scope here).

## Scope versus already registered

Existing MobDefs in `renderer/pipelines/entity_renderer.rs` already cover Squid, GlowSquid, Cod, Salmon (size variants), TropicalFish (small/large shape), and Pufferfish (three puff meshes). No changes are needed for these seven kinds. The new file provides 7 bake functions for Axolotl (adult/baby), Dolphin (adult/baby), Guardian (shared Guardian/ElderGuardian), and Turtle (adult/baby): `bake_axolotl_model`, `bake_baby_axolotl_model`, `bake_dolphin_model`, `bake_baby_dolphin_model`, `bake_guardian_model`, `bake_turtle_model`, `bake_baby_turtle_model`.

## Texture paths verified in the 26.2 jar

Jar: `C:/Users/yuzum/AppData/Roaming/.minecraft/versions/26.2/26.2.jar`; all listed assets were present under `assets/minecraft/textures/entity/` (also checked in the extracted 26.2 asset tree).

- Axolotl: `axolotl/axolotl_{lucy,wild,gold,cyan,blue}.png` and matching `_baby.png`. Five appearance variants; integration must map actual synchronized Axolotl variant metadata/index to this exact order (do not assume index ordering without reading the registry holder's asset_id).
- Dolphin: `dolphin/dolphin.png`; baby texture `dolphin/dolphin_baby.png`.
- Guardian: `guardian/guardian.png`; ElderGuardian distinct texture `guardian/guardian_elder.png`; beam asset `guardian/guardian_beam.png` is not a model texture. Same Guardian model family; Elder scale/pulse/spikes must be applied by renderer.
- Turtle: `turtle/turtle.png`; baby texture `turtle/turtle_baby.png`; equipment scute overlays are separate assets (`equipment/humanoid/turtle_scute.png`, `equipment/humanoid_baby/turtle_scute.png`), not baked into base geometry.

## Integration contract / metadata

The aquatic `EntityKind`s to integrate are `Axolotl`, `Dolphin`, `Guardian`, `ElderGuardian`, `Turtle`; do not re-add the seven kinds already in MobDefs. For adult/baby mesh selection use living age metadata (Ageable baby boolean is the generic mob data index 16 in 26.2). Axolotl's appearance is a registry-backed variant and should map the synchronized variant's `asset_id` to the five adult/baby path pairs above, not treat its integer as an arbitrary index until the registry order is confirmed. Guardian and ElderGuardian are distinct entity kinds rather than a variant; Elder uses Elder texture and vanilla scale 2.0. Turtle's scute overlay is equipment/layer state, not represented by these base bakes.

## Renderer module patch suggestion

Add `pub mod aquatic;` in `pomme-client/src/renderer/mod.rs` (or the existing entity-models module declaration chosen by integration); import the seven named functions from `renderer::entity_models::aquatic` in `entity_renderer.rs`, then add five MobDefs with `AnimationType::...`/adult and baby texture sets above. Guardian and ElderGuardian can call `bake_guardian_model()`; renderer still needs ElderGuardian scale and Guardian beam/attack animation. Don't register Aquatic functions until their `BakedEntityModel` mapping is selected by MobDef. No existing private-helper visibility change is necessary: the new file uses public `bake_model` and public model data types.

## Limitations requiring verification before calling these bakes exact

The supplied `26.2.jar` contains mapped class files and textures, but no `source/reference` source tree was found in the repository or at the named jar path. `javap -p` confirmed model class names such as `AdultAxolotlModel`; source body layer constants/UVs were not extracted from bytecode in this task. Consequently the current part/UV layouts in `aquatic.rs` are candidate vanilla-shaped meshes, **not verified exact 26.2 ModelLayer cube/UV geometry**. The baby functions currently derive adult parts by scaling pivots rather than using independently verified Baby*Model layers. Do not claim exact geometry or ship MobDefs as production-ready until these are reconciled against `createBodyLayer()` via read-only `javap -c -p` or actual source/reference. Axolotl metadata index/order likewise needs confirmation from its 26.2 entity data schema/registry. These are known follow-ups, not build/test claims.

Checks permitted/requested: `rustfmt --edition 2024` only. No cargo check, build, or tests were run; an unregistered module would not be covered by a normal cargo check anyway.
