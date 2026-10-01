# Fabric Movement Observer

Independent, client-only, opt-in Minecraft 26.2 observer. This is **not** a 1:1 replacement for Pomme's recorder yet: packet allowlisting and schema-1 JSONL framing are shared, but this version captures packet type only (no packet fields), and movement records are a smaller snapshot. Missing/unused travel, collision, attribute, and input fields are deliberately not synthesized as if they were measured.

## Install and record

1. Use Minecraft **26.2**, Java **25+**, Fabric Loader **0.19.5+**, and Fabric API **0.161.0+26.2**.
2. Close Minecraft normally. Back up nothing and remove/replace no existing files: copy `build/libs/fabric-movement-observer-1.0.0.jar` into that instance's `mods` folder. For a clean observation, do not load the separate `fabric-render-probe` mod or other mods that alter rendering/gameplay.
3. Launch the Fabric client. Recording is off by default. Press **F8** to start; press **F8** again to stop. Disconnect and normal game shutdown also stop/drain the writer. The chat/status notification identifies the output path and reports writer failure or drops.
4. Files are created under `<Fabric game directory>/movement-observations/movement-<UUID>.jsonl`. Share the file only after checking its footer has `complete: true`, `dropped: 0`, and `oversize_omitted: 0`.

## Captured data and boundaries

- UTF-8 JSON Lines schema 1, `seq`, monotonic `offset_us`, `direction`, `stage`, `data`; header/footer carry start/end time, protocol 776, `client_kind: "fabric"`, queue/row/file caps, drop counts, reason, and completeness.
- A 128-row bounded non-blocking queue and single writer; the writer creates a unique file, drains on stop, writes a footer and flushes. Producers serialize only while recording is active. A bounded queue can drop records; gaps and footer counters indicate incomplete evidence.
- `LocalPlayer.tick` head/tail snapshots: own-player position, velocity, collision flags, orientation, pose/bounds, sprint/crouch/fluid flags, and current input keys. These are snapshots, not actual-used travel friction/jump/collision-resolver inputs. Effective movement/jump attributes, gravity, step height, and food level are snapshots, not proof of the precise value consumed during an individual physics operation. No GUI scale, bob, HUD, rendering, movement option, or physics state is changed.
- Connection channel observer captures an explicit movement/input/action/item-use/swing/teleport-confirm and correction/block/attribute allowlist. Packet payloads are extracted only through typed getters; no `toString`, reflection, arbitrary serialization, chat, auth, URL, or NBT fields are captured. Numeric Java floats are written as doubles (retaining the exact f32 value). The negotiated numeric wire ID is not exposed at this hook (`native_id: null`); normalized `packet` kind and typed fields are present. Native game queue/dequeue stages are not exposed and are not fabricated. `transport_write_attempt` means channel write invocation; `transport_write_success`/`transport_write_failure` use the actual Netty promise, with only the exception class emitted on failure, and do not prove server acceptance. Inbound `received` is channel-read-before-listener. Client packet listener head/tail hooks record main-thread applied states for own-player corrections, own entity velocity/teleport/attributes, block updates/sections, and block acknowledgements. Off-thread `PacketUtils` dispatch calls are skipped at listener head, so the eventual main-thread invocation is what is recorded. Block lists are capped at 4096; ack prediction internals are unavailable and emitted null. Snapshots describe observed state at the hook, not per-travel inputs or physics-used values.
- No chat, sign text, arbitrary packet, raw bytes, URL, or NBT payload is copied. Header executable identity is the current Java process executable (path/size/mtime/SHA-256); it is not mislabeled as the Minecraft artifact. `minecraft_artifact` is null because its runtime file identity was not reliably available. `source_build_revision` is embedded by the build.

## Build

From the repository root, with Java 25 selected:

```sh
fabric-render-probe/gradlew -p Client/diagnostic/fabric-movement-observer clean build
```

The separate render-probe project contributes only its Gradle wrapper and cached dependency artifacts; its mod/classes/mixins are not a dependency and are not packaged. The production jar is `build/libs/fabric-movement-observer-1.0.0.jar`.
