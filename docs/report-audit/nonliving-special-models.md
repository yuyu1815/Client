# Nonliving special model/reference implementation (Minecraft 26.2)

This is the geometry and downstream integration handoff for `armor_stand`, `mannequin`, `end_crystal`, and `experience_orb`, plus static-mesh reuse decisions for other special entities. New Rust geometry is intentionally limited to ArmorStand and EndCrystal: those are the only two requested as dedicated cube-model bake functions. No shared state, network, core, renderer orchestration, or existing model file was changed.

## Implemented geometry

`pomme-client/src/renderer/entity_models/nonliving_special.rs` exports:

- `bake_armor_stand_model() -> BakedEntityModel`: adult ArmorStand geometry from `ArmorStandModel.createBodyLayer()` plus inherited `ArmorStandArmorModel.createBaseMesh()`. 10 independent parts / 10 cubes: head, body, arms, legs, two body sticks, shoulder stick, and base plate. Standard UV sheet is 64×64. Pose input is not baked; six rotations attach to parts in the renderer integration.
- `bake_end_crystal_model() -> BakedEntityModel`: literal Y-up crystal mesh from `EndCrystalModel.createBodyLayer()`. 4 parts / 4 cubes: outer glass, nested inner glass, nested core each use vanilla 8³ source cubes; `part_scales` .875 and .765625 on the nested parts produce effective inner 7³ and core 5.359375³. Base is 12×4×12. UV sheet is 64×32, intentionally lower resolution than the 128×64 texture. Uses `ModelConvention::BlockYUp`.
- Local test `vanilla_special_geometries_bake_nonempty_with_in_atlas_uv_origins` asserts non-empty vertices, all ArmorStand cube origins/dimensions and UV origins in golden order, EndCrystal cube dimensions/UVs, hierarchy and nested scale factors, plus positive dimensions and in-sheet origins. It has not been run, per request.

Armor stand's `base_plate`, arms, body/leg sticks are separate parts so flags can hide specific components. This file does not encode flags, pose, scale, marker visibility, armor, or equipment: these are runtime state/draw responsibilities. The mesh represents the vanilla model layer independent of the renderer's adult/small selection; small is a renderer scale, not a second cube definition.

## 26.2 extraction, renderer constants, and asset verification

Mapped-client bytecode inspected with `javap -c -p` against `C:/Users/yuzum/AppData/Roaming/.minecraft/versions/26.2/26.2.jar`:

- `net.minecraft.client.model.object.armorstand.ArmorStandModel#createBodyLayer`, `ArmorStandArmorModel#createBaseMesh`, and `net.minecraft.client.renderer.entity.ArmorStandRenderer`: armor-stand body layer, inherited pieces, flags and render behavior.
- `net.minecraft.client.model.object.crystal.EndCrystalModel#createBodyLayer` and `net.minecraft.client.renderer.entity.EndCrystalRenderer`: four crystal parts, scales, bob/rotation, beam and bottom behavior.
- `net.minecraft.client.renderer.entity.ExperienceOrbRenderer`: native camera-facing quad, atlas UV frame selection, color, translation, and 0.3 scale.
- Local decompiled mapped source `minecraft-26.2-decompiled/src/net/minecraft/world/entity/ExperienceOrb.java#getIcon`: experience value to icon frame thresholds and lifetime.

Verified JAR assets by `unzip -l`, extracted PNG headers with `file`:

| Asset | JAR path | Dimensions | SHA-256 (extracted PNG) |
|---|---|---:|---|
| Armor stand | `assets/minecraft/textures/entity/armorstand/armorstand.png` | 64×64 | `eb0dc8fd65f60560c8945033c589d86093487925f0e86c1fd8dd1c20ce922261` |
| Crystal | `assets/minecraft/textures/entity/end_crystal/end_crystal.png` | 128×64 | `753671b80bd3a88f301b7ec44e26a48003afc176f32d0a3338b37df284edc103` |
| Crystal beam | `assets/minecraft/textures/entity/end_crystal/end_crystal_beam.png` | 16×256 | `314e5d35de8f8c0ebb2fe2cf044199ecdaf5841ed0ff6da393fcf1e0c23fbee2` |
| XP orb | `assets/minecraft/textures/entity/experience/experience_orb.png` | 64×64 | `fe55d3823ae0f2c8114e051ae26a38d48808abc31a514401047ed08862ac8ec0` |

The armor stand asset in this JAR is `armorstand/armorstand.png` (not the older `armorstand/wood.png`). The orb asset is under `entity/experience/`, not `entity/experience_orb/`. Crystal base/beam textures are separate concerns: the crystal mesh uses the crystal texture; beam metadata must use the beam texture/render path.

### ExperienceOrb render specification (native billboard, no cube bake)

- Synced field index 8 is `Int value`; preserve it as signed integer.
- Vanilla `ExperienceOrb.getIcon()` mapping: frame 10 for value ≥2477, 9 ≥1237, 8 ≥617, 7 ≥307, 6 ≥149, 5 ≥73, 4 ≥37, 3 ≥17, 2 ≥7, 1 ≥3, else 0.
- The 64×64 atlas is 4 columns × 4 rows of 16×16 frames. Given icon `i`, frame UVs are `u=[(i mod 4)*16, (i mod 4)*16+16]/64`, `v=[floor(i/4)*16, floor(i/4)*16+16]/64`.
- `ExperienceOrbRenderer.submit`: camera quaternion billboard, translate y=0.1, scale 0.3; frame color uses `sin(age/2)` and `sin(age/2 + 4.1887903)` channels as in bytecode, plus vanilla additive green channel; add 7 block light, clamp 0..15. Current renderer should use its actual atlas frame/material path, not generic dropped-item geometry.
- Vanilla entity age lifetime is 6000 ticks (`ExperienceOrb.LIFETIME`); age is local entity age, not synced field 8. Do not use `value` as lifetime. Interpolate/track age if the client simulation provides it; otherwise use client spawn age as animation phase and document the limit.

### EndCrystal render specification

- Synced metadata: index 8 `Optional<BlockPos>` beam target; index 9 `Bool` show bottom. The target is converted to block center before subtracting interpolated crystal position.
- Model texture: `minecraft:textures/entity/end_crystal/end_crystal.png`; `EndCrystalRenderer.submit` applies root scale `(2,2,2)` and then translates `(0,-0.5,0)`. The renderer's `0.5` is its shadow radius, not a model scale. With `BlockYUp` baking (`/16`), preserve the renderer root transform exactly once; this yields effective 1/8 block per model pixel. The 64×32 model layer UV coordinates are intentionally normalized against the 128×64 image, so the sampled region is the top-left half-resolution layout, not a requirement to resize the texture.
- For age `t`, vanilla getY: `s=(sin(0.2*t)/2)+0.5`; return `s*s+s` times 0.4 minus 1.4. Preserve the JAR's part hierarchy (`outer_glass` → `inner_glass` → `cube`) and nested scales `.875`, `.765625` (effective dimensions 8, 7, 5.359375 model pixels). The 0.5 in `EndCrystalRenderer` is only `shadowRadius`; it is not another crystal model scale. Mapped `setupAnim` applies the outer quaternion sequence `YP.rotationDegrees(3*t).rotateAxis(pi/3, sin45, 0, sin45)`; inner/core each use `setAngleAxis(pi/3, sin45, 0, sin45).rotateY(3*t radians)`. Preserve each local rotation in the nested parent transforms; do not flatten them into one shared root rotation. The outer glass rises an additional half of `getY(t)*16` model units from y=24.
- Beam target requires `end_crystal_beam.png` and a beam mesh/render submission equivalent to vanilla `EnderDragonRenderer.submitCrystalBeams`; beam rendering is not included in the bake function.

## ArmorStand state / hooks

26.2 `entities.json` schema (Steel registry reference already documented by `nonliving-entity-plan.md`) and mapped `ArmorStandRenderer` should drive this state:

- Inherited Living metadata indices 8–14 belong to generic living health/effect data, not special mesh input.
- ArmorStand index 15: `Byte client_flags`. Vanilla bits: `0x01` small, `0x04` show arms, `0x08` no baseplate, `0x10` marker. Hiding the baseplate is the inverse of bit 0x08; arms are enabled only by 0x04. Preserve shared invisibility (common shared-flags metadata index 0, bit 0x20) separately from these client flags.
- Indices 16–21 are `Rotations` (three float angles in degrees), in order head, body, left arm, right arm, left leg, right leg. Convert each axis to radians at render time and map to matching baked part; account for vanilla y-down model rotation signs used by `EntityYDown` transforms.
- `small` applies vanilla half-size model/armor transform; `marker` selects vanilla's special render type: normal non-outline opaque rendering returns no render type, while outline/translucent paths can still submit it. Preserve that visibility/render-mode behavior instead of treating marker as simply another small-scale bit. Baseplate/arms visibility are independent flags.
- Use texture above and existing item render meshes for armor/held items. Extend the SetEquipment flow to preserve every slot if/when integration is authorized. Do not make armor into body cubes.
- Suggested extraction: `ArmorStandRenderInfo { entity_id, transform, flags, [pose;6], equipment }` from typed entity state, consumed by a separate special-model draw in world render order. Build baked model once (`bake_armor_stand_model`) and update six part transforms per entity. Runtime renderer module is currently not registered.

## Mannequin: existing player model reuse, no new geometry

`Mannequin` is a distinct nonliving entity, but its visible body is the player humanoid mesh. Do not duplicate body/limb geometry. Existing public function `renderer::entity_model::bake_player_model(slim: bool)` provides the wide/slim 64×64 player body mesh. Call it for profile model metadata (wide vs slim) from the eventual dedicated mannequin renderer; do not modify `entity_model.rs` or route Mannequin through living `EntityKind::Player` classification.

26.2 Mannequin metadata after inherited Living 8–14: index 15 `HumanoidArm` main hand; index 16 `Byte` customization; index 17 `ResolvableProfile`; index 18 `Bool` immovable; index 19 `Optional<Component>` description. Profile/name/skin fetch selects the texture and wide/slim arm layout; customization controls player overlays/parts; description is an optional label concern. Preserve equipment through SetEquipment, reusing existing item draws. Existing `bake_player_model(bool)` is geometry only and does not resolve a profile/skin or implement mannequin flags. No mannequin renderer class/model layer was found among the JAR renderer/model classes searched; verify the 26.2 native entity/render registration before implementing its exact player state policy.

## Nonliving with no new static cube geometry

- `falling_block`, `tnt`, `BlockDisplay`: resolve their full synchronized/spawn block state, then reuse existing `world/block/model.rs` baked block mesh/material and state-property resolution. FallingBlock blockstate comes from AddEntity spawn data; metadata 8 is start `BlockPos`, not the falling block state. TNT metadata 8 is `Int fuse`, metadata 9 `BlockState` (default TNT). Fuse flash/pulse is a dynamic transform/material on the reused TNT block model.
- `ItemDisplay`: index 23 is typed full `ItemStack`; index 24 is `Byte ItemDisplayContext`; common Display indices 8–22 carry interpolation/translation/scale/quaternions/billboard/lighting/visibility. Resolve and draw existing item mesh. `DisplayResolver` only picks item JSON context transform; it is not an entity display renderer.
- `BlockDisplay`: index 23 is typed `BlockState`; apply Display common matrix then block model. It is not a generic cube.
- Other dropped/thrown item objects can use existing item mesh extraction where the vanilla kind really is an item; preserve item stack components and orientation. Non-item projectile models remain distinct.
- Boat/raft and minecart have renderer-specific authored geometry/material, not static block meshes; this assignment does not implement them. Minecart/boat state/draw integration remains its own family.

## Integration boundary and explicit non-implementation

The task constraints prohibit edits to shared entity/core/net/render files and require new files only. Consequently this change supplies geometry and a verified integration contract, but does not register `renderer::entity_models::nonliving_special` in `renderer/mod.rs`, add event/state storage, add extraction, allocate/upload meshes, invoke draw calls, load assets in the resource resolver, or implement orb/crystal beam rendering. Mannequin, ExperienceOrb, and beam renderer/state hooks are documented for downstream `b956` integration; their runtime drawing is not implemented here. The renderer module is not integrated yet, so these geometries are not reachable in-game until the owner makes that authorized registration and world-pass wiring. No `falling_block`/TNT/ItemDisplay/BlockDisplay static cube bake is appropriate; reuse model pipelines as specified.

## Verification record

Commands run (working directory `C:/Users/yuzum/Desktop/mine_rust` unless noted):

- `cd Client && find . -iname '*nonliving*' -o -name 'nonliving-entity-plan.md' ...`: located the source plan and model files, exit 0.
- `java -version`: OpenJDK 21.0.5; shell PATH lacks `javap`, so invoked `/c/Program Files/Java/jdk-21/bin/javap.exe` explicitly.
- `javap -classpath <26.2.jar> -c -p` on ArmorStandModel, ArmorStandArmorModel, EndCrystalModel, EndCrystalRenderer, ExperienceOrbRenderer: exit 0; raw output was inspected from `/tmp/as.txt`, `/tmp/asa.txt`, `/tmp/ec.txt`, `/tmp/ecr.txt`, `/tmp/orb.txt`.
- `unzip -l <26.2.jar> | grep ...`: confirmed exact armorstand, crystal, beam, and orb entries, exit 0.
- `unzip -p <jar> <png> >/tmp/...png; file /tmp/...png; sha256sum /tmp/{armorstand,crystal,crystal_beam,orb}.png`: ArmorStand 64×64 (`eb0dc8fd...922261`), EndCrystal 128×64 (`753671b8...4edc103`), beam 16×256 (`314e5d35...3fbee2`), ExperienceOrb 64×64 (`fe55d382...ac8ec0`), exit 0.
- Client JAR SHA-256: `40896ee9f1e2bec3c934daac7e93d41e9e3d9c2f8ae0ca366d52ffbfd1afa290`.
- Raw javap output SHA-256: ArmorStandModel `ff0851e25f9b03f9ac6cae634487c755291038e1259e65c08d58b8f13d98509a`; EndCrystalModel `7c0d8452d9546f908f87982538dbefd68b132b0ca9e13b26fe992f8b23702664`; EndCrystalRenderer `ee577d761b088506102e2c175103ae5088ff1debc1a49ad07bde83bc248b67ff`; ExperienceOrbRenderer `ac30a3ff86bc277c11ffc459d7941f26e37c776ee3eb92de58bf45c7faf8b17f`.
- `rustfmt --edition 2024 --check pomme-client/src/renderer/entity_models/nonliving_special.rs`: exit 0 after formatting only this new Rust file. `git diff --cached --check -- pomme-client/src/renderer/entity_models/nonliving_special.rs docs/report-audit/nonliving-special-models.md`: exit 0. Build, tests, and `cargo check` were not run; module registration/type check was intentionally skipped because shared `renderer/mod.rs` is out of scope.

Remaining verification: query actual registry JSON serializer records in a dedicated typed ingress change; validate armor stand draw/pivot/flags and crystal dimension against a captured vanilla gallery; decode/hash beam texture and PNG hashes; verify pack-aware atlas resolution in runtime. JAR geometry and atlas origins are directly supported by mapped bytecode and compile-time test assertions, but tests were not executed.
