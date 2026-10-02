# Entity model bake-call resolution (read-only audit)

Snapshot: `git show 9bad9308e7b1b3b4ce6a0ed2af9c2cd29761ca41:<path>` (commit message `Fix block entity model test type checks`). Confirmed `ec173dc` and `d5d6324` are ancestors of that snapshot. No source edits/build/tests/checks performed.

## Resolution / result

`pomme-client/src/renderer/mod.rs:6-11` declares inline `pub(crate) mod entity_models { pub mod aquatic; pub mod flying; pub mod humanoid; pub mod terrestrial; }`. `entity_renderer.rs:15` imports this module by the exact path `use crate::renderer::{MAX_FRAMES_IN_FLIGHT, entity_model, entity_models, shader, util};`—there is no alias or glob hiding the calls. Its module-qualified production calls are present in `mob_definitions()`; the audited registration block is at approximately lines 1398-1946. Results: **40 distinct MobDef kinds** use these four modules: aquatic 5, terrestrial 15, humanoid 20, flying 0. Therefore the literal `42` attribution is not supported by this committed initializer: the enumerated source calls resolve to 40. The reported 13 remaining kinds were not investigated or enumerated here (per task, they remain ce511's responsibility); do not infer a 55-total denominator from these 40 calls.

There is no `pub use`/reexport of these bake functions in `renderer/entity_model.rs`: it is the separate legacy/other model module (the file begins with model implementation and `super::chunk::mesher::ChunkVertex`). Thus `entity_model::*` is not evidence against the module-qualified calls. The call syntax is direct `entity_models::<module>::bake_*`, with no import alias or macro-generated function name.

`mob_definitions()` is called in production by `EntityRenderer::new` (`entity_renderer.rs:1991`); the production constructor caller is `renderer/mod.rs:547`. `new` consumes each `MobDef`, builds adult/baby/overlay `MobVariant`s with `build_variants`, then inserts `mobs.insert(def.kind, MobEntry { ... })` (roughly lines 2060-2100). Rendering later resolves `self.mobs.get(&info.entity_kind)` in `EntityRenderer::draw` (roughly line 2525). This is an actual production initialization/draw lookup chain, not merely test reachability.

## Module bake -> MobDef kind registration table

Rows describe bake functions called when `mob_definitions()` executes. Baby/overlay entries are additional calls for the same kind, not extra species. `AnimationType::Static` is specified in these entries; this table makes no claim about model geometry or animation quality.

| module | `MobDef.kind` | `mob_definitions()` bake calls (adult; baby / overlay where present) | MobDef animation |
|---|---|---|---|
| aquatic | Axolotl | `bake_axolotl_model`; `bake_baby_axolotl_model` | Static |
| aquatic | Dolphin | `bake_dolphin_model`; `bake_baby_dolphin_model` | Static |
| aquatic | Guardian | `bake_guardian_model(false)` | Static |
| aquatic | ElderGuardian | `bake_guardian_model(true)` | Static |
| aquatic | Turtle | `bake_turtle_model`; `bake_baby_turtle_model` | Static |
| terrestrial | Armadillo | `bake_armadillo_model`; `bake_baby_armadillo_model` | Static |
| terrestrial | Camel | `bake_camel_model`; `bake_baby_camel_model` | Static |
| terrestrial | CamelHusk | `bake_camel_husk_model` | Static |
| terrestrial | Fox | `bake_fox_model`; `bake_baby_fox_model` | Static |
| terrestrial | Frog | `bake_frog_model` | Static |
| terrestrial | Goat | `bake_goat_model`; `bake_baby_goat_model` | Static |
| terrestrial | Hoglin | `bake_hoglin_model`; `bake_baby_hoglin_model` | Static |
| terrestrial | Zoglin | `bake_zoglin_model`; `bake_baby_zoglin_model` | Static |
| terrestrial | Panda | `bake_panda_model`; `bake_baby_panda_model` | Static |
| terrestrial | PolarBear | `bake_polar_bear_model`; `bake_baby_polar_bear_model` | Static |
| terrestrial | Ravager | `bake_ravager_model` | Static |
| terrestrial | Sniffer | `bake_sniffer_model`; `bake_baby_sniffer_model` | Static |
| terrestrial | Strider | `bake_strider_model`; `bake_baby_strider_model` | Static |
| terrestrial | Llama | `bake_llama_model`; `bake_baby_llama_model` | Static |
| terrestrial | TraderLlama | `bake_llama_model`; `bake_baby_llama_model` | Static |
| humanoid | Endermite | `bake_endermite_model` | Static |
| humanoid | Silverfish | `bake_silverfish_model` | Static |
| humanoid | SnowGolem | `bake_snow_golem_model` | Static |
| humanoid | Tadpole | `bake_tadpole_model` | Static |
| humanoid | WanderingTrader | `bake_wandering_trader_model` | Villager |
| humanoid | Parched | `bake_parched_model` | Skeleton |
| humanoid | WitherSkeleton | `bake_wither_skeleton_model` | Skeleton |
| humanoid | Evoker | `bake_illager_model` | Humanoid |
| humanoid | Illusioner | `bake_illager_model` | Humanoid |
| humanoid | Pillager | `bake_illager_model` | Humanoid |
| humanoid | Vindicator | `bake_illager_model` | Humanoid |
| humanoid | Piglin | `bake_piglin_model` | Humanoid |
| humanoid | PiglinBrute | `bake_piglin_model` | Humanoid |
| humanoid | ZombifiedPiglin | `bake_piglin_model` | Humanoid |
| humanoid | CopperGolem | `bake_copper_golem_model` | Golem |
| humanoid | Creaking | `bake_creaking_model` | Static |
| humanoid | Parrot | `bake_parrot_model` | Static |
| humanoid | Warden | `bake_warden_model` | Static |
| humanoid | Nautilus | `bake_nautilus_model`; `bake_baby_nautilus_model` | Static |
| humanoid | ZombieNautilus | `bake_zombie_nautilus_model`; `bake_baby_nautilus_model`; `bake_zombie_nautilus_coral_model` overlay | Static |
| flying | — | **No production call in `mob_definitions()`** | — |

## Macro and test boundaries

`mob_definitions()` defines `tex_table!`, `villager_type_table!`, `villager_profession_table!`, and `villager_level_table!` (lines 495-524). They expand texture-path slices, not bake calls or `MobDef`s. The ordinary `opaque(...)` helper (around line 710) wraps an already-evaluated `BakedEntityModel` in a `VariantDef`; it does not defer/alter bake resolution. No macro-generated bake function is involved in these four module registrations.

Separate test-only references: `all_mob_meshes_bake_and_definitions_are_unique`, `giant_and_cave_spider_have_distinct_vanilla_mesh_entries_and_scales`, and other unit tests call `super::mob_definitions()` to validate returned definitions; these are not the sole callers because `EntityRenderer::new` is a production caller. In particular, there is no test-only `use entity_models::...` that explains the grep result.

The exact fully qualified string `entity_models::aquatic::...` search being empty in any other checkout cannot establish dead code: imports, module aliases, globs, and inline module declarations must be resolved. Here, resolution is even more direct: the fixed committed initializer imports the parent `entity_models` name and makes 40 explicitly module-qualified calls.

## Checks (read-only)

- `git show -s --format='%H %s' 9bad930`: resolved to `9bad9308e7b1b3b4ce6a0ed2af9c2cd29761ca41`; exit 0.
- `git merge-base --is-ancestor ec173dc 9bad930` and the same for `d5d6324`: both exit 0.
- `git show 9bad930:<entity_renderer.rs> | grep 'kind: EntityKind::' | sort -u` equivalent count: **78 distinct kinds in the full file**; explicit module registration slice verified separately as **40**. (This full-file count includes unrelated entities and is not a living-species denominator.)
- `git grep 'EntityRenderer::new' 9bad930 -- pomme-client/src`: one production caller, `pomme-client/src/renderer/mod.rs:547`.
- No cargo/build/test/check command run, as explicitly prohibited.
