# VarInt continuation guard + DisplaySlot parity integration

Run: `20260924T094722Z` (the per-command log is `audit-filter/implementation/varint-continuation-integration-20260924T094722Z.log`). The canonical contracts read before editing were `C:/Users/yuzum/Desktop/mine_rust/audit-filter/implementation/varint-continuation-contract.md` and `scoreboard-display-slot-codec-contract.md`.

## Changes

- `vendor/azalea-buf/src/impls/primitives.rs`, `AzBufVar for i32/i64::azalea_read_var`: return the existing `InvalidVarInt` / `InvalidVarLong` immediately if byte 5 / 10 still has continuation set. `u32` and `u64` inherit the guard through their existing signed-reader delegation. The code does not read byte 6 / 11. Earlier EOF behavior is unchanged (`i32/u32` remain `Io`; `i64/u64` remain `InvalidVarLong`). Writers are untouched.
- Added four primitive unit tests covering signed and unsigned zero/min/max/-1 round trips; nonminimal terminating encodings and unused high payload bits; cap continuation with next byte present and exact EOF, exact errors/cursor positions for all four types; and pre-cap EOF error variants. The existing UTF-16 tests were retained and ran.
- `pomme-client/src/net/azalea_compat.rs`, `display_slot_typed_decode_matches_native_id_fallback`: obtains the ID by name from the native `PacketTable`, then uses the real `deserialize_packet::<ClientboundGamePacket>` path. Checks slots 0..18, ID -1/19/300 to LIST, terminating overlong VarInt(18) to TEAM_WHITE, and terminating overlong out-of-range VarInt(19) to LIST; all 24 successful fixtures preserve `objective` and consume the full cursor. A separate five-continuation-byte slot input returns `Err` and consumes only the five slot bytes; it is not asserted as a slot-zero case. No normalizer or production DisplaySlot conversion was added.

## Behavioral boundary and packet handling

The cap guard rejects on byte 5/10. Official 26.2 reads byte 6/11 before throwing the “too big” runtime exception (or underflows attempting that read at EOF), so rejection/consumed cursor position/error representation are intentionally **not identical**; the local reader leaves byte 6/11 unread. A packet decoder is operating on a frame-bounded cursor, and `connection.rs` routes typed decode errors through `skip_malformed_packet(e)?`; its `ReadPacketError::Parse`, `UnknownPacketId`, and `LeftoverData` arms warn and return `Ok`, then the game-loop `continue` receives the next frame. Other error kinds still propagate/disconnect. Thus a malformed frame is skipped, not resumed at its remaining bytes. This is a codec fixture plus static route inspection, **not** a full-server E2E test.

The initial #13-34 “unknown DisplaySlot drops packet” premise remains a false positive for terminating VarInts: unknown IDs select `List` in the existing decoder. The added cap-malformed case validates only the separate length guard; it does not justify a DisplaySlot-specific normalization or imply exploitability/security severity.

## Validation

Final source checkpoint (unchanged for the successful final test runs):

- `vendor/azalea-buf/src/impls/primitives.rs`: SHA-256 `5801ab683422186b746d6fadccd4b28ce5d1f6b21243c2f947846b9314b9b4be`
- `pomme-client/src/net/azalea_compat.rs`: SHA-256 `59dba2e46452add8a8f98331729a25a8a9b9291adcea0729b480a638230ca008`

Successful commands used offline mode and `RUSTFLAGS='-C link-arg=C:/Users/yuzum/AppData/Local/Temp/pomme-vulkan-import/vulkan-1.lib'`:

- `cargo test --manifest-path vendor/azalea-buf/Cargo.toml --lib --offline --config 'patch.crates-io.azalea-buf-macros.path="C:/Users/yuzum/Desktop/mine_rust/Client/vendor/azalea-buf/azalea-buf-macros"'`: exit 0, **17 passed**, including all four new tests and existing UTF-16 tests. The standalone manifest alone first failed offline (exit 101) because its resolution did not see root patches; rerun with the existing local macro path patch passed. The root patch tables were not changed.
- `cargo check -p pomme-client --offline`: exit 0.
- `cargo test -p pomme-client --no-run --offline` (binary test target; no `--lib`): exit 0.
- Final-checkpoint `cargo test -p pomme-client display_slot_typed_decode_matches_native_id_fallback --offline`: exit 0, **1 passed, 818 filtered**.
- Final-checkpoint `cargo test -p pomme-client --offline`: exit 0, **819 passed**.
- Final-checkpoint `cargo test -p pomme-protocol --offline`: exit 0, **49 passed** (plus zero doc tests).
- `rustfmt --edition 2024 --check vendor/azalea-buf/src/impls/primitives.rs`: exit 0; `git diff --check -- vendor/azalea-buf/src/impls/primitives.rs pomme-client/src/net/azalea_compat.rs`: exit 0.

The focused packet attempt before the `i32` literal/cast correction failed to compile (exit 101); its failure and the successful retries are preserved in the run log. At run start `vendor/azalea-buf/Cargo.lock` was absent (recorded with an explicit absence check); standalone Cargo tests generated it with SHA-256 `efa304cdd8a5b070c22ad658b89c0dd09a56295e560cb8f2ac6897dc6697af84`, and only that run-generated lock was removed. The existing `vendor/azalea-buf/target` was preserved. No commits, resets, cleans, ledger edits, or global Cargo checkout/cache source edits were performed.
