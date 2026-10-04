# Legacy particle option codecs

## Goal

Fix the confirmed legacy `level_particles` dust decode bug for the supported protocol range 763–777 without changing the native 26.2 particle codec. Preserve particle option meaning and packet framing across the full raw-frame → parser → spawn/extract path; do not infer screenshot root cause from this code defect.

## Confirmed evidence and boundary

Inbound game frames in `pomme-client/src/net/connection.rs` are translated by `Translation::translate_game_frame` before `handle_raw_game_packet` sees them. `translate_level_particles_765` in `pomme-client/src/net/translate.rs` (around 3019–3032) relocates the particle id, inserts `alwaysShow`, and copies option bytes unchanged. `parse_level_particles_for_protocol` in `pomme-client/src/net/handler.rs` (around 2622–2673) then reads Dust unconditionally as `i32 packed_color + f32 scale` (8 bytes).

At least protocol 765/766 Dust wire options are `red, green, blue, scale` as four floats (16 bytes). For RGB=(1,0,0), scale=1, the current 8-byte read interprets red's float bits as packed color `0x3f800000`, green as scale `0`, leaves blue and scale unread, and the existing Dust spawn path clamps the resulting size to its minimum. This is a definite parser bug for those protocol inputs; the screenshot's protocol is unknown, so it is not evidence that this caused that screenshot.

| Protocol | Dust option layout status | Required evidence before implementation |
|---|---|---|
| 763 | unverified | Official version jar/mappings + packet/particle codec |
| 764 | unverified | same |
| 765 | RGB float ×3 + scale float (confirmed) | retain source reference in implementation/test comment |
| 766 | RGB float ×3 + scale float (confirmed) | retain source reference in implementation/test comment |
| 767 | unverified | Official version jar/mappings + codec |
| 768 | unverified | same |
| 769 | unverified | same |
| 770 | unverified | same |
| 771 | unverified | same |
| 772 | unverified | same |
| 773 | unverified | same |
| 774 | unverified | same |
| 775 | unverified | same |
| 776 | unverified | same |
| 777 | unverified | same |

Do not replace the table with a 765/766-only special case: first establish every layout boundary in the supported range, including `dust_color_transition` and any changed particle names/options, using each applicable official jar/mappings (or an equally authoritative schema). Keep version labels/protocol mapping tied to the project's protocol table rather than guessing aliases.

## Codec ownership and implementation boundary

1. Keep `connection.rs`'s translation-before-handler route and `translate_level_particles_765`'s header-only responsibility unless a verified version-specific option transformation is necessary. The 765 header rewrite currently preserves option bytes by design.
2. Make `parse_level_particles_for_protocol` in `pomme-client/src/net/handler.rs` the single protocol-aware decoder for raw `level_particles`: select the wire layout from the active source protocol, consume exactly that layout, and produce the existing common `ServerParticleOptions` values. Unknown/unverified layouts must not be guessed.
3. Define ownership explicitly: the raw packet handler decodes option payloads that only it consumes; `copy_particle_payload` in `pomme-client/src/net/translate.rs` owns only metadata/registry particle payload copying and normalization required for the native metadata codec. Do not add a second dust conversion to that shared metadata path. It already handles payload sizes by particle name and version flags; investigate/version this path consistently, preserving the current native 26.2 codec and existing `copy_particle_payload` behavior unless official codec evidence requires a narrowly scoped correction.
4. Trace callers/contracts for `packetsHeaderNative` versus `optionWire` mixtures in the translator and raw handler before moving any conversion. State the exact API/file boundary in the change; don't create another nested helper abstraction for a single layout decision.
5. For old RGB float Dust → common packed RGB, preserve color rather than float bit patterns: conceptually `channel = round(clamp(rgb, 0, 1) * 255)` for each channel, then pack according to the existing extractor's RGB layout (`R<<16 | G<<8 | B`; `particle.rs` extracts those three bytes and normalizes by 255). Confirm Java's actual clamp/round semantics and whether alpha is absent/implicit from the official provider before coding; do not invent alpha semantics. Preserve scale as the fourth float exactly, subject only to the existing downstream spawn behavior.
6. `dust_color_transition` has two RGB triplets plus scale in layouts that use float color options: establish its exact codec and how/if the current common representation supports it before changing behavior. Do not silently consume extra/trailing bytes or introduce shader-size/speculative rendering changes.

## Old spell/entity-effect questions: evidence gate

Latest-version `ClientPacketListener` evidence that `count == 0` does not imply RGB is not sufficient to settle historical providers. Inspect official older `SpellParticle` and `EntityEffectParticle`/provider implementations for each relevant layout boundary:

- **Unresolved:** whether old `effect` / `instant_effect` (including historical `ambient_entity_effect`) providers interpreted velocity arguments as color despite a no-payload packet codec. Separate what the wire codec sends from how the emitter populates particle velocity/color; do not infer one from the other.
- **Unresolved:** the precise removal/rename boundary for `ambient_entity_effect` and whether a compatibility alias exists in each supported version. Keep rename/alias handling separate from Dust payload parsing.
- **Confirmed current raw-parser risk:** `parse_level_particles_for_protocol` reads four bytes for native `EntityEffect` unconditionally. Establish from old official codecs whether older supported `entity_effect` carries an option payload; if absent, change only that protocol-specific raw decode while consuming the correct subsequent bytes. Do not use the latest client alone to assert the old payload was absent.
- **Out of scope until verified:** emitter appearance differences not explained by a wire codec mismatch; record them as observation follow-up, not as implementation claims.

## Minimal implementation and tests (follow-up work; no code in this plan task)

After evidence closes the version table, make the smallest change at the above ownership boundary and add focused tests:

- Layout table/boundary tests for every distinct Dust and `dust_color_transition` format across protocols 763–777, including exact cursor advancement and a sentinel byte/field after options to prove following bytes are consumed correctly.
- Raw translated-frame integration test: build source `level_particles` frame → `translate_game_frame` → `handle_raw_game_packet`/`parse_level_particles_for_protocol` → assert extracted/spawn input has RGB red/blue/green and scale unchanged. Include the 765 header relocation and payload preservation path.
- Malformed/truncated option tests that reject/skip safely without fabricating values.
- Regression coverage for payload-less/payload-bearing old spell and entity-effect protocols only after official provider+codec evidence resolves their boundary.
- Retain/run native 26.2 Dust and existing metadata `copy_particle_payload` tests to prove current native codec and metadata behavior do not regress.

No implementation, protocol-specific magic workaround, shader change, or unrelated cleanup is authorized by this plan.

---

## Investigation addendum: verified codec boundaries and implementation decision

This addendum corrects the provisional claims above. The table and conversion below supersede the prior “763–777 unverified” table, the assumed four-float range ending at 766, and the proposed `round(clamp(...) * 255)` conversion. The original plan text is retained as requested; only spell/provider items explicitly called out as unverified remain gated.

### Version table (protocol labels from the repository's actual `pomme-protocol/src/version.rs` `VERSIONS` and `EMBEDDED` tables)

The supported versions share two option-codec groups. Byte size means only particle option bytes, excluding particle id and the enclosing packet fields.

| Protocol | Listed game version(s) | `dust` wire options (order; bytes) | `dust_color_transition` wire options (order; bytes) | Evidence |
|---:|---|---|---|---|
| 763 | 1.20, 1.20.1 | red `f32`, green `f32`, blue `f32`, scale `f32`; 16 | from-red `f32`, from-green `f32`, from-blue `f32`, scale `f32`, to-red `f32`, to-green `f32`, to-blue `f32`; 28 | official 1.20.1 class mappings (`DustParticleOptions(Vector3f,float)`, `DustColorTransitionOptions(Vector3f,Vector3f,float)`); schema field order corroborated by the 1.20.5 schema; protocol mapping in `version.rs` |
| 764 | 1.20.2 | same; 16 | same; 28 | PrismarineJS `data/pc/1.20.2/proto.yml` schema; repository `version.rs` protocol mapping |
| 765 | 1.20.3, 1.20.4 | same; 16 | same; 28 | PrismarineJS `data/pc/1.20.3/proto.yml`; local `translate_level_particles_765` confirms header-only rewrite and preserved payload bytes |
| 766 | 1.20.5, 1.20.6 | same; 16 | same; 28 | PrismarineJS `data/pc/1.20.5/proto.yml`; mappings pages for 1.20.6 `DustParticleOptions(Vector3f,float)` and `DustColorTransitionOptions(Vector3f,Vector3f,float)` confirm float-based options |
| 767 | 1.21, 1.21.1 | same; 16 | same; 28 | PrismarineJS `data/pc/1.21.1/proto.yml` (float field definitions and order) |
| 768 | 1.21.2, 1.21.3 | color `i32`, scale `f32`; 8 | from-color `i32`, to-color `i32`, scale `f32`; 12 | mapped 1.21.2 signatures `DustParticleOptions(int,float)` / transition options (mappings.xhyrom); PrismarineJS `data/pc/1.21.3/proto.yml` confirms the 1.21.3 wire field order; 1.21.2/1.21.3 share protocol 768 in repository `VERSIONS` |
| 769 | 1.21.4 | color `i32`, scale `f32`; 8 | from-color `i32`, to-color `i32`, scale `f32`; 12 | PrismarineJS `data/pc/1.21.4/proto.yml`; mappings for 1.21.4 `DustParticleOptions(int,float)` / `DustColorTransitionOptions(int,int,float)` |
| 770 | 1.21.5 | same; 8 | same; 12 | PrismarineJS `data/pc/1.21.5/proto.yml` codec family; repository embeds protocol-1.21.5 |
| 771 | 1.21.6 | same; 8 | same; 12 | PrismarineJS `data/pc/1.21.6/proto.yml` codec family; repository embeds protocol-1.21.6 |
| 772 | 1.21.7, 1.21.8 | same; 8 | same; 12 | PrismarineJS `data/pc/1.21.8/proto.yml`; repository `VERSIONS` maps both labels to 772 |
| 773 | 1.21.9, 1.21.10 | same; 8 | same; 12 | PrismarineJS `data/pc/1.21.9/proto.yml`; repository `VERSIONS` maps both labels to 773 |
| 774 | 1.21.11 | same; 8 | same; 12 | PrismarineJS `data/pc/1.21.11/proto.yml`; repository protocol table |
| 775 | 26.1, 26.1.1, 26.1.2 | same; 8 | same; 12 | local official client jar `C:/Users/yuzum/AppData/Roaming/.minecraft/versions/26.1.2/26.1.2.jar`, extracted `DustParticleOptions` and `DustColorTransitionOptions` STREAM_CODEC definitions (ByteBufCodecs.INT, INT then FLOAT); repository `VERSIONS` maps all patch labels to 775 and `EMBEDDED` uses 26.1 |
| 776 | 26.2 (native) | same; 8 | same; 12 | checked-in `pomme-protocol/src/data/protocol-26.2.json`, local `minecraft-26.2-decompiled` option classes, and native translation implementation |
| 777 | 26.3 | same; 8 | same; 12 | local official client jar `C:/Users/yuzum/AppData/Roaming/.minecraft/versions/26.3/26.3.jar`, extracted `DustParticleOptions` and `DustColorTransitionOptions` STREAM_CODEC definitions (ByteBufCodecs.INT, INT then FLOAT); repository `VERSIONS` and `EMBEDDED` |

The wire transition order is significant: legacy codecs place `scale` between the source RGB triplet and destination RGB triplet (not after both triplets); packed codecs send source color, destination color, then scale. The field type schemas and exact sequences above are from versioned PrismarineJS protocol schemas; official mapped class signatures independently support the change at 1.21.2. Same-protocol patch releases are listed under the source-of-truth labels used by `version.rs`, rather than treated as separate guessed wire layouts.

External evidence URLs consulted:
- `https://github.com/PrismarineJS/minecraft-data/blob/master/data/pc/1.20.2/proto.yml`
- `https://raw.githubusercontent.com/PrismarineJS/minecraft-data/master/data/pc/1.20.3/proto.yml`
- `https://github.com/PrismarineJS/minecraft-data/blob/master/data/pc/1.20.5/proto.yml`
- `https://github.com/PrismarineJS/minecraft-data/blob/master/data/pc/1.21.1/proto.yml`
- `https://mappings.xhyrom.dev/1.21.2/net/minecraft/core/particles/dustparticleoptions`
- `https://mappings.xhyrom.dev/1.21.2/net/minecraft/core/particles/dustcolortransitionoptions`
- `https://raw.githubusercontent.com/PrismarineJS/minecraft-data/master/data/pc/1.21.3/proto.yml`
- `https://github.com/PrismarineJS/minecraft-data/blob/master/data/pc/1.21.4/proto.yml`
- `https://github.com/PrismarineJS/minecraft-data/blob/master/data/pc/1.21.5/proto.yml`
- `https://github.com/PrismarineJS/minecraft-data/blob/master/data/pc/1.21.6/proto.yml`
- `https://github.com/PrismarineJS/minecraft-data/blob/master/data/pc/1.21.8/proto.yml`
- `https://github.com/PrismarineJS/minecraft-data/blob/master/data/pc/1.21.9/proto.yml`
- `https://github.com/PrismarineJS/minecraft-data/blob/master/data/pc/1.21.11/proto.yml`
- `https://mappings.dev/1.20.1/net/minecraft/core/particles/DustParticleOptions.html`
- `https://mappings.xhyrom.dev/1.20.6/net/minecraft/core/particles/dustparticleoptions`
- `https://mappings.xhyrom.dev/1.21.1/net/minecraft/core/particles/dustparticleoptions`
- `https://mappings.xhyrom.dev/1.21.4/net/minecraft/core/particles/dustparticleoptions`
- `https://aldak0.ru/javadoc/1.21.4-21.4.x/net/minecraft/core/particles/DustColorTransitionOptions.html`

### Exact conversion and minimal implementation boundary

For protocols **763–767**, `handler.rs::parse_level_particles_for_protocol` should read precisely three big-endian `f32` RGB channels followed by scale `f32` (16 bytes total), then store common `ServerParticleOptions::Dust { packed_color, scale }`. Use the actual native helper semantics, not the earlier round/clamp proposal:

```text
channel8(x) = floor(x * 255.0f32)  // Java Mth.floor; no implicit clamp
packed_color = ARGB.colorFromFloat(1.0f32, red, green, blue)
             = 0xFF000000 | (channel8(red) << 16)
                         | (channel8(green) << 8)
                         | channel8(blue)
```

The extracted local official 26.1.2 and 26.3 jars and local 26.2 decompile all expose `net.minecraft.util.ARGB.colorFromFloat(a,r,g,b)` calling `as8BitChannel` for all channels and `as8BitChannel(value)` as `Mth.floor(value * 255.0f)`. `ARGB.color` masks channels to 8 bits; it does not clamp to the normalized range. Dust's renderer reads only bits 16–23, 8–15, and 0–7, so the implicit opaque alpha in the ARGB helper does not alter its RGB appearance. Keep scale's decoded float unchanged in the option.

For protocols **768–777**, retain the current raw decoder exactly: read packed `i32` followed by scale `f32` (8 bytes). Do not convert or rewrite those option bytes in translation. The smallest behavior change belongs solely in the `Dust` arm of `parse_level_particles_for_protocol` in `pomme-client/src/net/handler.rs`, branched on the active source `protocol` (`< 768` versus `>= 768`); no `ServerParticleOptions` shape change is needed for Dust.

`dust_color_transition` has **no supported common option representation or render kind** today: `particle.rs::ServerParticleKind` and `ServerParticleOptions` have `Dust { packed_color, scale }` but no transition variant; unsupported particle kinds are deliberately dropped rather than interpreted as simple. Do not reinterpret transition as Dust, add a shader behavior, or falsely claim full transition spawn preservation. Its codec sizes/order are now known above; retain the current unsupported/drop behavior until a separate rendering/options task explicitly adds a representation. Any frame-walker that copies it must still use the verified source layout size (28 below 768, 12 at/above 768) to preserve following metadata/list entries.

### Responsibilities and already-separate paths

- `connection.rs` receives raw game frames, calls `Translation::translate_game_frame`, and then delivers the returned native-header frame to `handle_raw_game_packet` (see game loop around 1658–1663). This is the route where the active source protocol remains available to the raw parser.
- `translate.rs::translate_level_particles_765` is a header rewrite only: old leading particle id is removed, `alwaysShow=false` inserted after limiter, native header fields copied, and option bytes copied from their original span. Leave this route unchanged; its 765 Dust payload must remain four floats for the raw handler to decode.
- `handler.rs::parse_level_particles_for_protocol` is the raw `level_particles` option decode and owns the new legacy-float-to-common-packed conversion.
- `translate.rs::copy_particle_payload` / its callers normalize particle values embedded in entity metadata/registry payloads for the native metadata codec. That is independent from the raw `level_particles` parser. Do not run the conversion twice or broaden this specific raw decoder fix into metadata handling. Preserve current native 26.2 codec/metadata behavior.
- `particle.rs::ServerParticleOptions::Dust` is already the common packed RGB+scale representation; the spawn path extracts RGB24 as `(packed >> 16)`, `(packed >> 8)`, `packed` and divides each by 255. The 26.2 `DustParticleOptions` uses packed color and scale; `ARGB.vector3fFromRGB24` confirms dust consumes the low 24-bit RGB. This path explains why packing opaque ARGB from floats is compatible without an alpha option.

### Scope still unresolved: old spell and entity-effect provider semantics

Do not widen the Dust change based on these unanswered questions. The `entity_effect` codec transition is evidenced at protocol 766: repository `translate.rs` has `color_particles` guarded by its protocol capability; the 1.20.5/1.20.6 type changes and local registries show the legacy ambient name before that boundary and the later entity-effect tint option after it. The handler currently reads four bytes for `EntityEffect` for all protocols, so protocols below 766 need a separate evidence-driven raw decode correction before relying on that path.

The precise old client provider interpretation is **not verified here**. Public class mapping signatures alone do not show `createParticle` method bodies, and the local 26.2 decompile only establishes current behavior. A 1.20.1 Forge API page/source search shows `SpellParticle.AmbientMobProvider` takes three speed arguments and documents their use as RGB, with alpha set to `0.15F`; that is not an official Mojang method-body artifact and does not establish which legacy particle type, packet codec, or emitter calls it in each supported release. Therefore retain as unresolved: old `effect`/`instant_effect`/`ambient_entity_effect` provider x/y/z handling and alpha behavior; exact ambient rename/removal/alias boundary; whether a metadata `entity_effect` conversion exists independently of packet options. Before changing these paths, obtain and cite the appropriate official version client jar/provider source and emitter registration/caller per relevant codec boundary. Latest 26.2 `count == 0` evidence cannot settle historical providers. No guessed handler magic or alias change is approved by this addendum.

### Regression input and byte-consumption expectation

The focused raw-frame regression should include at least:

1. A protocol-765 source `level_particles` packet whose header uses the old particle-id-first order, Dust id resolves to `dust`, and option bytes are `1.0f, 0.0f, 0.5f, 1.25f` (big-endian). After `translate_game_frame`, assert native header order/inserted `alwaysShow` are correct and all 16 option bytes are byte-identical. In the parser unit fixture append a sentinel byte after the one packet's options; after `parse_level_particles_for_protocol`, assert color bytes are R=255, G=0, B=127 (`0xFFFF007F` from opaque floor semantics), scale is exactly `1.25f`, the cursor is exactly 16 bytes past the beginning of Dust options, and the sentinel remains unread. `level_particles` carries one particle type/options value per packet, so do not fabricate a second entry in its frame.
2. A protocol-767 raw frame with the same float option bytes and trailing sentinel in a parser fixture: expect the same option and 16-byte consumption without the 765 header rewrite.
3. A protocol-768 frame: packed color `0x00123456` followed by scale `1.25f`, then sentinel in a parser fixture; expect unchanged RGB and 8-byte consumption. Include protocol 769 and protocol-776 native frames as unchanged packed-codec regressions.
4. For truncated 16-byte legacy and 8-byte packed payloads, expect safe parse failure/skip and no invented color/scale. A separate codec-size test for transition options expects 28-byte legacy and 12-byte packed consumption while it remains unsupported for spawn.

`0.5f` deliberately distinguishes floor (127) from round-to-nearest (128), proving the formula rather than merely passing primary colors. Run existing native Dust and metadata payload tests after implementation; this investigation performed no runtime/build/test changes.
