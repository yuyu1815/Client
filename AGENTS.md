# Agent commands

Use mise as the entry point for Rust build/check/test commands. Run from the
repository root, regardless of the checkout's directory name or location.

## Setup

```sh
mise trust .mise.toml
mise install
mise exec -- rustc --version
mise tasks
```

Mise pins Rust as well as Node/pnpm. Keep its Rust version/components identical to
`rust-toolchain.toml` (used by CI and rustup). Do not substitute another nightly or
stable toolchain to bypass errors. Initialize the SteelMC submodule on a new clone
(`git submodule update --init --recursive`).

A working Vulkan development library and native build tools are still required;
mise does not install the Vulkan SDK. Machine-specific environment belongs in
ignored `.mise.local.toml`, not in tracked config or repeated shell prefixes.
Review and trust that file if present. Configure `CARGO_TARGET_DIR`,
`PKG_CONFIG_PATH`, or `DYLD_LIBRARY_PATH` there only when your environment needs
them. Without an override, Cargo uses the repository's `target/` directory.
Do not assume a neighboring profiling directory or copy another machine's paths.
Never force-add local config to Git: check both `git check-ignore -v .mise.local.toml`
and `git ls-files .mise.local.toml` (the latter must be empty).
Historical audit/diagnostic notes may contain old machine paths; they are evidence,
not portable setup instructions. Use the current mise tasks and local environment.
The test tasks intentionally use `mise exec` to restore the configured environment
after macOS strips `DYLD_*` when starting the task shell. Keep that handling in
mise rather than inventing per-command linker flags.

## Normal workflow

| Command | Purpose |
|---|---|
| `mise run check` | Quick type/borrow check; no runnable binary |
| `mise run build` | Client build with `dev-fast`, including singleplayer |
| `mise run test` | Client/protocol/singleplayer tests, with failures preserved |
| `mise run test-registry` | Steel registry unit tests |
| `mise run build-release` | Explicit runtime-performance/release build |

Build/test use `--locked` and default features. Cargo handles freshness; do not
add mise source/output caching that can accidentally skip tests. Run Cargo tasks
serially, especially when measuring times. Use `mise run --dry-run <task>` to
inspect a command instead of inventing an alternative.

## Do not force a passing or faster result

- Do not run `cargo clean`, delete target/incremental directories, change
  `CARGO_TARGET_DIR`, or switch profiles/toolchains during normal iteration.
  OpenAL staging must follow `CARGO_TARGET_DIR` when it is set.
  A new profile/cache needs a full first build; a no-op build is not an edit build.
- Do not inject `RUSTFLAGS`, `RUSTC_BOOTSTRAP`, extra `--config` overrides,
  `--no-default-features`, alternate codegen backends, or linker workarounds to
  get past a failure. Report missing prerequisites; do not fake an SDK/library.
- Do not remove tests, add `--skip`/`--ignored`, disable assertions/overflow
  checks, or swallow nonzero exit codes to make validation green. A user-requested
  focused test or controlled experiment is fine; record its scope explicitly.
- Keep normal dev/release optimization settings and Steel worldgen/math overrides.
  `dev-fast` trades runtime optimization for compile speed: its FPS is unmeasured.
- Existing macOS baseline: five `ui::text_edit::tests` fail (`clipboard_copy_paste_cut`,
  `copy_with_no_selection_clears_clipboard`, `ctrl_a_selects_all_cursor_at_end`,
  `delete_word_backwards`, `replace_selection_with_stale_window_does_not_panic`).
  They failed in normal dev too. Report actual outcomes; do not assume any new
  failure is baseline or claim the suite passed.
- Steel's Git-watch fix is in the fork selected by `.gitmodules`. Publish Steel
  changes to that fork before updating the parent repository's pinned submodule
  commit. Preserve local changes; do not reset/update forcibly. Read Steel's own
  `AGENTS.md` before changing its code.

For an explicitly requested optimization experiment, use a separate target
folder, record the command/profile/cache state, compare like with like, and undo
only your temporary source edits. Never turn experimental flags into defaults
without measuring and explaining the trade-off.
