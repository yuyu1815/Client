# #13-11 scoreboard display-slot integration

## Findings

Integrated display-slot ingress/state support into the live client. The patch carries the decoded `DisplaySlot` unchanged from `SetDisplayObjective` packet handling through `NetworkEvent::ScoreboardDisplay`, `AppCore`, and one `Scoreboard.displays: [Option<String>; 19]` array. `default_sidebar()` supplies the existing sidebar renderer from the `Sidebar` array entry.

Each slot update affects only that index. Empty/unknown objective names clear only the requested slot; a subsequently-created objective does not resurrect an unknown assignment. Removing an objective clears all slots referencing it, leaving other objectives untouched. `Scoreboard::clear()` clears the array along with the existing objective/score/team state. Existing scores, teams, number formatting, player-name behavior, and sidebar drawing logic were not otherwise changed. No unsafe enum fabrication or native invalid-slot codec changes were made.

This is **not full #13-11 resolution**. New LIST/tab, BELOW_NAME/heart, and colored TEAM_* sidebar rendering consumers are not implemented; no stubs were added.

## Relevant Files

- `pomme-client/src/net/handler.rs` — forward every decoded display slot and retain a representative-slot typed event test.
- `pomme-client/src/net/mod.rs` — add typed protocol `DisplaySlot` to the event.
- `pomme-client/src/app/core.rs` — forward slot and name to `Scoreboard`.
- `pomme-client/src/ui/hud.rs` — single 19-slot state, slot-scoped updates/removal/clear, `default_sidebar()`, and tests.
- `implementation/scoreboard-slots-integration-20260924-1411-*.log` — command outputs, exit codes, and integrated-source hashes.

Candidate reviewed at `../audit-filter/implementation/patches/scoreboard-slots/`; original candidate diff is 191 lines. Candidate hashes are in its `hashes.json` and were not overwritten.

## Migration / Investigation Notes

- Reconstructed the original source snapshots in a temporary directory by copying the candidate source files and reverse-applying `minimal.diff` (`patch -R -p1`). Patch exited 0. Compared those reconstructed snapshots directly against the four live files; did not use whole-repo `git diff` to infer the origin of changes.
- Direct comparison found only the intentional scoreboard hunks and the added tests. In particular, the live HUD's pre-existing cooldown overlay parameters/drawing remain present; no HUD source was replaced wholesale. Candidate-original HUD snapshot SHA was `7cf84895b1c2170c6b49504eb2c47e19467e943e9bce85666ce8776a3660ed98`; reviewed baseline HUD SHA noted by reviewer was `f80280820494c3a2017e1b6bb1e15e15e596cf833caff68eae2e6b15546a0b4d`. The saved original snapshot was reconstructed and compared directly; the reason for the earlier reported `f802...` vs `7cf...` mismatch could not be independently classified as formatting-only or semantic because that exact pre-integration file was not separately preserved. The direct snapshot/live comparison exposed no cooldown conflict, and the cooldown path remained unchanged by this integration. After integration/formatting, current HUD hash is recorded below.
- Current integrated source SHA-256:
  - `net/handler.rs`: `348c76b5c88859e78288fa8b94a2d930c7b26de0188354d1e1a1f538a2d170fb`
  - `net/mod.rs`: `ad43ceab37e5abbd8a5f0f170bbfe1ae0c538e3d3479cc93a1a40b535f0e12b3`
  - `app/core.rs`: `eda26f018fca49e1e7f6c36d69fc9234c6a0f09dcd74d362d4272f80f9bf5efb`
  - `ui/hud.rs`: `842d3055530b3851adcf932b9369623940d573d6813713df9ab6c8d7348a961a`
- Formatting was limited to the four owned files with `rustfmt --edition 2024 --config skip_children=true`. `rustfmt --check` and `git diff --check` both exited 0.
- Native invalid-slot decoding was not changed or newly validated. Slot indexing depends on the already-decoded contiguous protocol enum (0..=18).

### Verification

All cargo invocations used `RUSTFLAGS='-C link-arg=C:/Users/yuzum/AppData/Local/Temp/pomme-vulkan-import/vulkan-1.lib'`. `--lib` was not used.

| Check | Command | Result |
|---|---|---|
| Client check | `cargo check -p pomme-client` | exit 0 |
| Focused scoreboard | `cargo test -p pomme-client --bin pomme-client scoreboard_display -- --nocapture` | exit 0; 3 passed, 0 failed |
| Focused handler | `cargo test -p pomme-client --bin pomme-client display_event_retains_representative_slots_and_objective -- --nocapture` | exit 0; 1 passed, 0 failed |
| Test compilation | `cargo test -p pomme-client --no-run` | exit 0 |
| Full client | `cargo test -p pomme-client` | exit 0; 779 passed, 0 failed |
| Protocol | `cargo test -p pomme-protocol` | exit 0; 49 passed, 0 failed; doc-tests 0 |
| Rustfmt | four touched Rust files, `--check --config skip_children=true` | exit 0 |
| Diff whitespace | `git diff --check` on four touched Rust files | exit 0 |

Warnings in cargo output are existing dead-code/future-incompatibility warnings; no test failures occurred. Detailed stdout/stderr and actual numeric exit codes are in the run-id-specific logs named above.

## Recommended Next Steps

1. Track LIST/tab, BELOW_NAME, and TEAM_* rendering as separate consumer work; keep #13-11 marked partially addressed until those consumers are implemented and verified.
2. Native invalid-slot codec behavior remains outside this change and unverified.
