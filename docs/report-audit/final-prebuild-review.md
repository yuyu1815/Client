# Final pre-build independent review (source-only)

Repo: `C:/Users/yuzum/Desktop/mine_rust/Client`
HEAD: `ff6ee95fb5718252fece9a8d83680e0e2213df25` (`ff6ee95 Extract ordered banner pattern block entity data`)
Branch: `master`, ahead 63. Existing worktree changes/untracked files were preserved. This is a static source/caller review plus test-target compilation checks, **not** test execution, executable build, or runtime/GPU verification.

## Findings

### P1 — Night/day terrain lighting work is not present in the live path; existing reported night difference remains open

- `pomme-client/src/renderer/chunk/mesher.rs:1705-1710`, `world_brightness`, calculates `LIGHT_TABLE[max(get_sky_light, get_block_light)]`. It receives neither world time nor dimension/environment sky-darkening, and collapses raw sky/block channels before shading.
- The live packed terrain vertex at `mesher.rs:89-96` is 16 bytes (`pos[3]`, `uv[2]`, `sprite`, `light_tint[4]`); it has no four raw sky/block corner samples. `renderer/chunk/buffer.rs:201-245`, `chunk_vertex_attributes`, exposes seven attributes, and `renderer/shaders/chunk.vert:36-37` consumes only `light_tint.r` as one `v_light`. There is no 20-byte vertex or raw-four-sample attribute route in this source.
- `renderer/shaders/fog.glsl:45-60`, `shade_chunk_surface`, shades that scalar. The `SkyState` clock in `renderer/pipelines/sky.rs` is used for sky presentation, but it is not connected to `world_brightness`/terrain packed light. Separate shadow code explicitly comments that `skyDarken` is not tracked (`renderer/world_shadow.rs:107-115`).
- This agrees with the still-open `docs/report-audit/remaining-render-plan.md` `RENDER-night` finding; it is **not** a newly runtime-reproduced bug and this review did not infer a replacement shader formula. Do not claim Night raw4/20-byte/uniform work or night-light parity complete. Keep the documented RENDER-night runtime difference open; next source owner must compare 26.2 jar behavior and wire the authoritative day/dimension inputs at the CPU light sampling + GPU interface before modifying a shader/spec.

### P2 — Registered geometry modules still do not mean entity rendering is integrated

- `renderer/mod.rs:5-13` now registers `entity_models::{flying,nonliving_special,projectiles,vehicles}`; this corrects the former *unregistered-module* review finding. `entity_renderer.rs` has production callers for flying models (for example around lines 1957-2161), so the old claim that `flying` has no caller is stale and is rejected.
- Current source search found no production caller of `bake_boat_model`, `bake_minecart_model`, `bake_trident_model`, `bake_shulker_bullet_model`, `bake_wither_skull_model`, `bake_llama_spit_model`, `bake_armor_stand_model`, or `bake_end_crystal_model`; matches are definitions/tests only in `renderer/entity_models/{vehicles,projectiles,nonliving_special}.rs`. The functions are compiled as registered modules but are not visible runtime integrations. The new Rust check still reports dead-code warnings for multiple functions in `nonliving_special.rs` / `projectiles.rs`.
- Consequently do not count the vehicle/projectile/armor-stand/end-crystal bakes as rendered or as completed MobDef coverage. They need entity-kind dispatch, state/pose/texture selection and a production caller; otherwise mark them intentionally geometry-only/incomplete. This finding is distinct from the separate `MobDef` entries already wired to `flying`.

### Prior-review issues rechecked against current source

- The prior offset review's renderer gap is fixed in current `renderer/chunk/mesher.rs`: `model_vertex_origin` calls shared `block_offset` with global `BlockPos`, and baked/multipart/cube/missing-cube emission uses the shifted origin. `edit_state_supported` declines nonzero-offset edits so normal remesh is used. `physics/block_shape.rs` adjacent-cell ray fixture now uses `edge_pos.x + 1.0`; the earlier ray-outside-shape coordinate finding no longer applies. These code/test assertions were **compiled, not executed**; all 110 offset-dependent states, actual GPU alignment, collision/raycast/runtime coverage are not thereby verified.
- Latest Loom and Stonecutter sources are present and routed from `app/phases/in_game.rs`. Loom preserves server pattern/tag indices, gates on complete registry/tags and native registry IDs, and returns candidate button positions with server-authored result-slot pickup. Stonecutter filters `ClientboundUpdateRecipes.stonecutter_recipes` in received order, keeps filtered candidate positions, uses property-0/button index and leaves result authority on synchronized slots. `connection.rs`/`handler.rs` carry registry and tag updates into game state. Their unit tests type-check; no UI/server interaction ran. Protocol translation/old-protocol limitations remain explicit and must not be represented as tested interoperability.
- Crafter remains readonly slot 45 in 46 total slots per `special_container.rs` / `player/menu_click.rs`; no finding in the previous count issue recurred in this static read.
- The explicitly disabled AutoJump/Brightness/Graphics/KeyBindings/SmoothLighting settings remain unimplemented/backend-less UI stubs (for example `ui/menu/options.rs:599-606`). Treat these as deliberate disabled/unfinished scope, **not** newly observed runtime bugs or completed options.
- The 26.2 shapes source status remains historical generator/oracle data, not a fresh roundtrip: `shapes-26.2.json` is expected to encode 1,196 blocks / 32,366 states and report 799 dictionary entries / 16,980 overrides. The external raw Mojang blocks report, oracle JSON, and verified comparison file needed to regenerate are not present in this Client checkout. Steel remains supporting reference, not a replacement oracle. Do not update `coverage-ledger.json` or label entries runtime-verified at this stage.
- Geometry references that cite the official mapped client JAR identify the artifact hash as `40896ee9f1e2bec3c934daac7e93d41e9e3d9c2f8ae0ca366d52ffbfd1afa290`; independently `sha256sum` of both installed `26.2.jar` copies matched that value. The separately cached client-only artifact is a different file/hash (`99055f39f8a18c6f192dfcd3702784378c6a815e1c750179a91898b13346f7ad`); don't substitute its hash for the mapped full client JAR. This confirms artifact identity, not every documented geometry constant.

## Checks executed

All commands were run from repo root through mise where required; no `cargo test`, `cargo build`, `mise run build`, executable linking, shader runtime, or game/server run was issued.

| Command | Exit | Result |
|---|---:|---|
| `mise tasks` | 0 | Available tasks confirmed: check/build/build-release/test/test-registry. |
| `mise run --dry-run check` | 0 | Actual check task is `cargo check -p pomme-client --locked --profile dev-fast`. |
| `mise run check` | 0 | Client type/borrow check, dev-fast; warnings only. |
| `mise exec -- cargo check -p pomme-client --tests --locked --profile dev-fast` | 0 | Test code type-checked; tests were not executed. Existing warnings (48 for test target incl. unused/mut, dead code) and 3 Steel missing-doc warnings. |
| `mise exec -- cargo check -p blockgen --tests --locked` | 0 | Blockgen test target type-checked only. |
| `mise exec -- cargo check -p pomme-singleplayer --tests --locked --profile dev-fast` | 0 | Singleplayer test target type-checked only. |
| `mise exec -- cargo fmt --all -- --check` | 0 | Workspace formatting clean. |
| `git diff --check` | 0 | No whitespace errors; Git printed LF-to-CRLF advisory for 14 modified Rust files. |
| `mise exec -- node scripts/check-report-audit.cjs` | 0 | `batches=793`, `unique=793`, duplicates 0; ledger 793 unique, missing 0, extra 0; 1,196 block shapes / 32,366 states. |

The commands compile/check Rust test code but do **not** prove assertions pass. No test was run and no linker or shader compilation was triggered. Existing unrelated warning count from the non-test `mise run check` was 94 client warnings plus 3 Steel warnings; do not interpret warnings as proof of dead runtime integration by themselves (the caller search above is the basis for that finding).

## Required later verification / roundtrip

These are **not run** because the relevant source inputs are missing here and build/test/runtime remain gated. After obtaining the official 26.2 DataGenerator blocks report, vanilla shape oracle, and accepted verified-comparison JSON, use temporary output paths:

```sh
mise exec -- cargo run -p blockgen -- blocks <26.2-reports/blocks.json> 26.2 <tmp>/blocks-26.2.json
mise exec -- cargo run -p blockgen -- shapes <tmp>/blocks-26.2.json <26.2-vanilla-shape-oracle.json> <verified-comparison.json> <tmp>/shapes-26.2.json
```

Then perform **semantic JSON** comparisons (not byte/whitespace comparison) against `pomme-client/src/world/block/data/blocks-26.2.json` and `shapes-26.2.json`: expect 1,196 blocks and 32,366 state identities in block output, and shape summary 1,196 blocks / 32,366 states / 799 shape-dictionary entries / 16,980 overrides, with the same oracle verified/skipped/disagreement statuses. The second command must not be called a completed roundtrip without the actual oracle and verified-comparison inputs. `cargo run` is a build/linking action and remains prohibited until the main agent opens that gate.

Before a final acceptance claim, still required: permitted `mise run test` (actual tests execute) and `mise run build` (also shader compilation/linking); GPU/Vulkan runtime and 26.2 comparison for Night lighting, offset geometry vs raycast/collision, full render passes, pack reload, banner/shield assets, menus and server-authoritative operations; native and supported translated-protocol loopbacks for Loom/Stonecutter/commands; movement/effect and block light fixtures. Keep `RENDER-night`, test execution, runtime, GPU and coverage-ledger status explicitly unverified until those runs produce evidence. The user's “after all works, compare official and optimize” gate is not cleared; no optimization was attempted.
