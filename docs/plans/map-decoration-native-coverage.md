# Native map-decoration coverage: minimal plan

## Recommendation

Vendor only the pinned Azalea `azalea-protocol` crate and append its missing eight
`DecorationType` variants (IDs 27–34). Keep the existing native packet codec and
`map_item_data_event → MapDecorationAsset::from_registry_id → MapStore` path;
do not add a raw packet parser or registry tracking. This fixes known 26.2 IDs
without changing unrelated protocol behavior.

## Evidence and exact IDs

`Cargo.lock` pins Azalea `0.16.0+mc26.2` at `ffedf17c9b6ff9fbafbf7e0d14c8d2366b2d0a32`.
That revision's `azalea-protocol/src/packets/game/c_map_item_data.rs` has only
27 enum variants (IDs 0–26). `DecorationType` derives `AzBuf`; the
actual macro in `vendor/azalea-buf/azalea-buf-macros/src/read.rs` reads a VarInt
and defaults every unrecognized value to the first variant (`Player`). Thus
27–34 currently deserialize successfully but alias to Player.

Minecraft 26.2's extracted registry evidence is
`third_party/SteelMC/steel-registry/build_assets/map_decoration_types.json`
(entries 27–34; registry has 35 entries, IDs 0–34). `pomme-client/src/world/maps.rs`
`MapDecorationAsset::from_registry_id` already maps these IDs and asset keys:

| ID | Registry key → native asset | `MapDecorationAsset` |
|---:|---|---|
| 27 | `village_desert` → `desert_village` | `DesertVillage` |
| 28 | `village_plains` → `plains_village` | `PlainsVillage` |
| 29 | `village_savanna` → `savanna_village` | `SavannaVillage` |
| 30 | `village_snowy` → `snowy_village` | `SnowyVillage` |
| 31 | `village_taiga` → `taiga_village` | `TaigaVillage` |
| 32 | `jungle_temple` → `jungle_temple` | `JungleTemple` |
| 33 | `swamp_hut` → `swamp_hut` | `SwampHut` |
| 34 | `trial_chambers` → `trial_chambers` | `TrialChambers` |

Official protocol evidence supplied for this task: 766–776 `MapDecoration` uses
direct registry IDs; 26.2 registers 0–34. Existing source translator confirms
legacy source bounds 763: 0–26 and 764/765: 0–33 (`pomme-client/src/net/translate.rs`).

## Smallest implementation delta (not performed here)

1. Copy the pinned crate from the Azalea `ffedf17…` source into
   `Client/vendor/azalea-protocol/`; patch only `src/packets/game/c_map_item_data.rs`
   with the eight variants in the table/order above. Do not copy the Azalea
   workspace or other crates. The available pinned source directory measures
   about 785 KiB; compare with existing `vendor/azalea-entity` (~549 KiB, 17
   files) as precedent. No `azalea-protocol` vendor currently exists.
2. In `Client/Cargo.toml`, add `azalea-protocol = { path = "vendor/azalea-protocol" }`
   to both `[patch.crates-io]` and `[patch."https://github.com/azalea-rs/azalea"]`,
   matching the existing `azalea-entity` patch pattern. Preserve crate version
   `0.16.0+mc26.2` and its manifest/dependencies. Update `Cargo.lock` only as the
   normal consequence of selecting the patched path (no cache edits).
3. Keep `AzBuf` derive and first-variant fallback unchanged for this bounded
   known-ID fix. IDs >=35 remain unknown and still decode as `Player`: document
   this ceiling; do not claim forward-compatible unknown handling. A strict
   closed enum would make decode fail for an unknown decoration and consequently
   drop/fail the entire packet, not just that decoration. The derive has no
   strict-unknown option. A raw-ID enum/custom `AzBuf` reader+writer could retain
   unknown IDs, but needs handler conversion changes; skipping only the icon
   would need additional packet decoding logic. Neither is part of this minimal
   patch. Decide separately if >=35 support becomes required.
4. Once 27–34 are represented natively, raise only `NATIVE_DECORATION_MAX`
   from 26 to 34 in `pomme-client/src/net/translate.rs`. Keep
   `LEGACY_763_MAX = 26` and `LEGACY_764_765_MAX = 33` unchanged: legacy inputs
   above each source's valid maximum must still be omitted, never reinterpreted.

## Tests and checks

- Replace/rename `map_item_data_766_775_passthrough_exposes_native_unknown_decoration_fallback`
  in `pomme-client/src/net/azalea_compat.rs`: native passthrough for each ID
  27–34 on 766–775 must decode to the same numeric enum ID, reach
  `map_item_data_event`, and store the corresponding asset in `MapStore`; include
  native 776 direct decoding. Test 0–34 through typed decode→event→store so the
  previous 0–26 mapping remains covered. Remove the old assertions that ID 34
  decodes as Player; retain an ID-0 Player control. Do not assert that >=35 has
  been fixed; a separate limitation/behavior test may pin the documented fallback.
- Preserve `legacy_map_item_data_763_765_translates_through_native_decode_and_map_store`
  and its 763/764/765 range guards. After the native ceiling change, verify all
  source-valid IDs arrive typed while 763 IDs >26 and 764/765 IDs >33 remain
  omitted. Update stale comments claiming native supports only 0–26/omits 27–33.
- From `Client/`, run `mise exec -- cargo test -p pomme-client map_item_data_766_775`
  and `mise exec -- cargo test -p pomme-client legacy_map_item_data_763_765`;
  then `mise run check` (per `AGENTS.md`). Confirm with
  `mise exec -- cargo tree --locked -i azalea-protocol` / `cargo tree --locked -d`
  that exactly one `azalea-protocol 0.16.0+mc26.2` path package resolves and no
  git-source duplicate remains; inspect lock source. Run `mise exec -- cargo fmt
  --all --check`. No cache or target cleanup.

## Implementation results

- Vendored only `azalea-protocol` from revision `ffedf17c9b6ff9fbafbf7e0d14c8d2366b2d0a32` into `vendor/azalea-protocol` (about 785 KiB). The upstream crate manifest was normalized into a standalone manifest with the pinned revision's workspace package/dependency versions; upstream `LICENSE.md` was included. Relative to the pinned upstream crate, source edits are limited to appending the eight `DecorationType` variants in IDs 27–34 order. No packet parser, derive macro, handler, registry tracking, or `MapDecorationAsset` changes.
- Both root patch tables select the same local protocol path. All existing Azalea git patch entries now use the same exact `ffedf17…` revision rather than a moving branch: offline resolution could not find `origin/26.2`, while the previous lockfile's branch source had resolved to this exact commit. This canonicalizes source IDs across the Azalea fixed-revision family; `Cargo.lock` changes only their source strings and the protocol package to a path source (12 additions/13 deletions, 25 changed lines); no dependency versions/checksums were upgraded or hand-authored. `cargo metadata --offline --locked` resolves successfully, and `cargo tree --offline --locked -i azalea-protocol` reports only `vendor/azalea-protocol`.
- `NATIVE_DECORATION_MAX` is now 34. The source constraints remain `LEGACY_763_MAX = 26` and `LEGACY_764_765_MAX = 33`; IDs outside the old source's range remain skipped. `azalea_compat.rs` now tests all IDs 0–34 through typed decode, event asset mapping, and `MapStore`; enum encode/decode roundtrips each one-byte ID; 766–775 translated and 776 direct paths check IDs 0/34; type 34's name, coordinates/rotation, and 2x2 pixels are preserved. Legacy tests cover 763's 0–26, 764/765's 0–33, invalid-source icon skipping, JSON names, and patch retention.
- Additional regression tests exercise anonymous 765 NBT compound lists with huge End count, negative count, invalid element type, and truncated element data, plus a valid empty End list carrying `text: "ok"` through event/store with a nonzero 2x2 patch. Both 763 and 764 integration frames cover NUL, supplementary-plane characters, and nested MUTF-8 keys at and over the u16 boundary. Existing `translate_entity_data_compound_tag_old_versions` now includes a valid empty End list.
- Validation: `mise exec -- cargo test --locked -p pomme-client map_item_data` — exit 0, 5 passed; `mise run check` — exit 0; `mise exec -- cargo metadata --offline --locked --format-version 1` — exit 0; `mise exec -- cargo tree --offline --locked -i azalea-protocol` — exit 0, one local package. One `mise run test` full run executed 1,535 pomme-client tests: 1,534 passed and the filled-map frame fixture failed an exact float comparison (`0.9999999` vs `1.0`). Changed that assertion to `abs_diff_eq(..., 1e-6)` and reran only `legacy_filled_map_frame_metadata_runs_translation_decode_remap_event_and_store` — exit 0, 1 passed. Per the one-full-suite limit, the full suite was not rerun after that fixture-only tolerance fix. `git diff --check` on touched implementation files passed. No GPU execution was performed.
- Scope ceiling: known IDs 0–34 are covered only. Native IDs 35+ still fall back to `Player` through AzBuf's existing unknown-enum behavior; this does **not** make all unknown native IDs safe or forward-compatible. The actual GPU rendering path was not exercised.
