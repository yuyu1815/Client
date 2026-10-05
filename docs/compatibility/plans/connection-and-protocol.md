# 認証・接続・プロトコル — 公式26.2互換計画

**Base:** Pomme `a12e38d9ea09d290a0b1736fe48a1cdafe49325f`. **Steel pin:** `0b1f87c36a664f08d81397e942fb1e19f6eb282b`; root submodule is clean at this pin and was referenced read-only only. The pin itself was checked, but Steel implementation review and B comparison remain unperformed. No update/init was done. **Target:** JE 26.2 protocol 776. **Comparisons:** A=Pomme to official 26.2 server; B=SteelMC singleplayer to official integrated server (connection/transport only; gameplay parity is separate). **Source inventory:** read-only local decompiled snapshot without a verifiable Git revision; no revision is fabricated. A finite SHA-256 manifest is in Appendix D. Static source scan is not runtime behavior proof.

## 対象と責任境界

接続はlauncher由来の明示identityからsocket/memory transport、wire framing、暗号/圧縮、protocol/phase、session registry epoch、semantic consumerへ順序を守って引き渡す。所有者を分け、process-global protocol/tableを接続ごとに書換える分散状態を正本にしない。

- `ConnectionSession` を唯一のtransport/codec/phase/identity参照/registry epoch/version所有者とする。connectionがphase transition、socket/memory lifecycle、frame cipher/compression状態、decode/encodeとoutbound orderingを管理し、feature/UI側へcodecやID表を公開しない。session毎にprotocol identityを固定し、並列接続でも別session mappingを参照しない。
- `AuthCredential` はlauncher/native認証所有。**Current boundary:** `AuthAccount` is serializable and includes the bearer token; refresh command returns it to the frontend (`commands.rs:262-275`), and `spawn_game` passes it as `--access-token` argv (`commands.rs:480-562`). The client parses argv (`pomme-client/src/args.rs:9-19`), stores it in `UserData` (`pomme-client/src/user.rs:12-28`), and clones it into menu/network/chat consumers (`pomme-client/src/app/mod.rs:165-195,409`; `pomme-client/src/net/connection.rs:90,752-790`; `pomme-client/src/net/chat_security.rs:385-448`). This is an actual bearer-secret process-command-line handoff, not the separate `--launch-token` file marker (random launch marker at `commands.rs:498-502,552-553`; checked/deleted by `main.rs:101-112`). Proposed design removes bearer token from serializable UI/React state, argv, logs, and errors; passes it by private one-shot launcher-to-client capability, with bounded lifetime, consume-once semantics, and best-effort secret cleanup. `OnlineVerified(uuid, name, profile, expiry, refresh capability/outcome)`と`ExplicitOffline`を別representationにする。malformed/partial credential、identity不整合、refresh/profile errorをOfflineへ降格させず拒否する。
- launcherはasset確保とprocess spawnだけを担当。認証/refresh/account identityはこちら。chat ownerはrich/secure message意味と署名consumerを担当し、profile-keyの取得/expiry/revocation/UUID照合はauth identity ownerが担う。feature ownerは個別packet意味・予測・UI反映、server gameplayはauthority、ConnectionSessionはtransport/wire運搬のみ。
- Typed event契約: singleplayer-lifecycle/Memoryは接続の開始・停止・終了理由を受ける。server-world/gameplayはauthoritative world/gameplay registry/provider値を受ける。resourcesはpack download/apply/reloadとrequired/deny responseを所有。HUDは通知/表示を所有。接続はこれらへversion付きordered typed eventを渡し、意味の補完・権威判定・resource/UI policyを内包しない。
- Diff anchorは双方のroot相対path、1始まりline、symbol。Pomme側rootはrepository、公式側rootは上記decompiled root。例: `pomme-launcher/src-tauri/src/auth.rs:163-325 callback/PKCE`、公式 `minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientHandshakePacketListenerImpl.java:110-168 handleHello`。transport例: `pomme-client/src/net/connection.rs:99-149 spawn_connection/ConnectionHandle`、`minecraft-26.2-decompiled/src/net/minecraft/network/Connection.java:249-279 setupInboundProtocol`。編集時には行を再確認する。

## 公式の機能母集合

### Phase/stateとregistry universe

State graph: `Resolving -> Handshake -> (StatusQuery -> Terminal | Login -> Configuration -> Play)`. A status probe owns a dedicated `ConnectionSession` and ends at `Terminal`; never promote it in-place to Login. After selection, start a fresh session `Resolving -> Handshake(intent=LOGIN) -> Login`. Official wire intent is `ClientIntentionPacket.intention`: `minecraft-26.2-decompiled/src/net/minecraft/network/protocol/handshake/ClientIntentionPacket.java:14-27 ClientIntentionPacket`. Official client status/login methods: `minecraft-26.2-decompiled/src/net/minecraft/network/Connection.java:291-312 initiateServerboundStatusConnection/initiateServerboundPlayConnection`; status call site `minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ServerStatusPinger.java:153-156`; login call site `minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/ConnectScreen.java:138-144`; server branch `minecraft-26.2-decompiled/src/net/minecraft/server/network/ServerHandshakePacketListenerImpl.java:33-64 handleIntention`. Pomme status-list route opens and uses a Status probe connection for status/ping: `pomme-client/src/net/resolve.rs:90-110 request_status/connect`, `pomme-client/src/ui/server_list.rs:150-169`. Pomme join-time protocol probe is dropped before selected-intention connection: `pomme-client/src/net/connection.rs:631-635` and `:202-204`. In-game `Reconfiguration` returns through Configuration; `Transfer` terminates the current session and negotiates a distinct destination; `Terminal` covers close/error/cancel/drop. Phase transitions wait for relevant ACK write-completion and queued feature-event apply barriers. `ConnectionSession` owns `WireRegistry(epoch, protocol, ordered entries incl. absent-payload positions, tags, known-pack proof)`. Missing payload does not remove an ID position. Pomme config sequence: `pomme-client/src/net/connection.rs:795-1050 config_sequence`; official client receipt: `minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientConfigurationPacketListenerImpl.java:80-205` and `minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/RegistryDataCollector.java:63-167`.

Official `RegistryDataLoader.SYNCHRONIZED_REGISTRIES` is 29 entries at `minecraft-26.2-decompiled/src/net/minecraft/resources/RegistryDataLoader.java:102-103 SYNCHRONIZED_REGISTRIES`, distinct from the 148 declaration denominator. Names/keys: `BANNER_PATTERN/banner_pattern`, `BIOME/worldgen/biome`, `CAT_SOUND_VARIANT/cat_sound_variant`, `CAT_VARIANT/cat_variant`, `CHAT_TYPE/chat_type`, `CHICKEN_SOUND_VARIANT/chicken_sound_variant`, `CHICKEN_VARIANT/chicken_variant`, `ZOMBIE_NAUTILUS_VARIANT/zombie_nautilus_variant`, `COW_SOUND_VARIANT/cow_sound_variant`, `COW_VARIANT/cow_variant`, `DAMAGE_TYPE/damage_type`, `DIALOG/dialog`, `DIMENSION_TYPE/dimension_type`, `ENCHANTMENT/enchantment`, `FROG_VARIANT/frog_variant`, `INSTRUMENT/instrument`, `JUKEBOX_SONG/jukebox_song`, `PAINTING_VARIANT/painting_variant`, `PIG_SOUND_VARIANT/pig_sound_variant`, `PIG_VARIANT/pig_variant`, `SULFUR_CUBE_ARCHETYPE/sulfur_cube_archetype`, `TEST_ENVIRONMENT/test_environment`, `TEST_INSTANCE/test_instance`, `TIMELINE/timeline`, `TRIM_MATERIAL/trim_material`, `TRIM_PATTERN/trim_pattern`, `WOLF_SOUND_VARIANT/wolf_sound_variant`, `WOLF_VARIANT/wolf_variant`, `WORLD_CLOCK/world_clock`. `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:156-303` contains 148 declarations and 147 unique keys because DIMENSION and LEVEL_STEM share `minecraft:dimension`. Classification is sync dynamic 29, other dynamic loader 18, built-in/static 95 (Pomme static table present 17, unidentified 78), manual/pseudo 5, runtime registration unknown 1. This is not exhaustive consumer audit; entry-level official-vs-Pomme fidelity remains uninvestigated.

Registry payload/static entry/tag/data-driven valueの一致を宣言しない。将来fixtureは公式26.2 server固定のresource/data pack hashを記録し、全entry order, optional payload, tags, known-pack selectionを凍結・比較する。resourcesがasset/resource解決、feature ownerがsemantic解釈、protocolがlossless運搬・validationを担う。

### Auth/profile boundary

**Current vs proposed:** Current `listen_for_callback` checks OAuth `error` before comparing `state`, interpolates `error_description` directly into an HTML response without escaping, then parses/decodes code; target order is listener-ready -> exact state validation -> OAuth error/status handling -> code extraction and URL decode -> PKCE S256 exchange -> XBL/XSTS -> Minecraft token -> profile fetch/validation. Reject mismatched state before processing even an OAuth error; require both state and PKCE. Escape every reflected HTML value. Current callback accept alone has a 300-second timeout; exchange and subsequent requests have no explicit cancellation/overall deadline in this path. Proposed cancellation, explicit timeout, HTTP status classification at every service step, and refusing partial credentials are not current behavior. Source: `pomme-launcher/src-tauri/src/auth.rs:163-225 oauth_sign_in`, `:265-327 listen_for_callback/reject/send_http_response`, `:329-405 exchange_msa_to_minecraft` (recheck line ranges when implementation begins).

**Current vs proposed:** current `exchange_msa_to_minecraft` discards provider expiry metadata and assigns `unix_now() + 86400`; this is a hard-coded 24-hour local value, not verified service expiry. Keyring access uses `Option`/ignored write errors, and account/refresh-token JSON parse/read/write/delete failures are swallowed or become empty/default state; `try_restore_or_refresh` also collapses failure to `None`. Proposed `OnlineVerified` binds token to fetched profile UUID/name and provider-derived expiry/refresh outcome; after refresh, re-fetch profile and verify identity; explicitly surface token rotation/absence and persistence/read/delete failures. Keyring failure requires an explicit temporary-session decision or launch refusal. No token is allowed only in explicit offline mode. Authlib/Microsoft/Xbox/Minecraft service retry/status/refresh semantics are outside this decompiled source scope. Source: `pomme-launcher/src-tauri/src/auth.rs:64-160 keyring_read/write, restore, refresh, save/remove`; `:329-405 exchange_msa_to_minecraft`; `pomme-launcher/src-tauri/src/commands.rs:262-275 refresh_account/fresh_token`. Official context: `minecraft-26.2-decompiled/src/net/minecraft/client/User.java:7-45`, `minecraft-26.2-decompiled/src/net/minecraft/client/Minecraft.java:406-423,814-815`, `minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientHandshakePacketListenerImpl.java:110-168 handleHello`.

## Credential process handoff (proposed design; not implemented or platform-verified here)

**Unverified future proposal, not current implementation:** use a private one-shot capability, not a token-bearing argument or environment value. Candidate Windows design: unnamed anonymous pipes; parent creates child-side credential-read and ACK-write handles; `CreateProcessW` plus `STARTUPINFOEXW/PROC_THREAD_ATTRIBUTE_HANDLE_LIST` allowlists child-side handles and required standard handles; parent pipe ends are non-inheritable, and duplicated child-side copies close after spawn. If handles are passed as numeric argv/env values, those values must be non-secret and handle inheritance/validation must be proven. API availability/behavior, exact handle numbering, ACL/inheritance, synchronous-I/O cancellation, and all numeric bounds/timeouts in this proposal are unverified and require an M0 platform-specific prototype/review against the selected Windows SDK before implementation. No public pipe name, file, or network address is proposed. Same-user debugger/admin process-memory inspection is outside the guarantee.

### Launch ownership, process identity, and arbitration

The auth parent owns the sealed, single-use launch-attempt record and is the only authority that commits any state transition, including terminal outcomes. Its states are `Pending -> Spawned -> Sent -> Acked | Aborted`; a single event executor or atomic/CAS transition serializes ACK, deadline, and child-exit events so exactly one terminal outcome wins. The successful process-creation timestamp `t0` starts the handoff deadline. On Windows retain the exact `PROCESS_INFORMATION.hProcess` returned by `CreateProcessW` as the sole stop capability for that child process instance. PID and executable image path are display/audit metadata only: Abort must never re-lookup the PID, call `OpenProcess`, or kill by name. Abort terminates via the retained handle and confirms exit before closing it; close the unneeded thread handle promptly. No PID-reuse race can redirect termination to another process.

### Framing, EOF, and non-circular pipe ordering

Proposed v1 envelope is length-prefixed and capped at 64 KiB, with random 256-bit launch nonce, verified UUID/name/profile and token/service expiry. The 64 KiB cap and 256-bit nonce are unverified local design proposals, not official values or measured platform limits. Parent writer and child reader run concurrently, so correctness does not depend on the OS pipe capacity being at least the envelope size. Parent performs one complete write-all of the envelope and closes its credential-pipe writer before waiting for ACK; `Sent` linearizes only after both write-all succeeds and writer close completes. Child rejects length >64 KiB, short payload, read error, or any extra byte; it must observe credential-pipe EOF after the complete payload before validating/consuming it and sending ACK. Thus no reader waits for EOF while the parent keeps a writer open waiting for ACK.

ACK is one complete, fixed-format write containing version, nonce, and success only (no token). Child closes its ACK writer immediately after write-all. Parent accepts ACK only after reading its exact expected length and then EOF; short/extra bytes, read error or nonce mismatch reject it. An ACK observed before the parent has committed `Sent` is retained as a candidate and validated only after `Sent`; once `Acked` or `Aborted` is committed, all late ACKs are ignored. Child consumes the credential through a Once-typed take exactly once and checks nonce, expiry, verified profile and token presence.

### One absolute handoff deadline and abort cleanup

Proposed deadline: one parent/child-shared monotonic basis, with `t0 + 10 seconds` where `t0` is successful spawn timestamp (candidate Windows `GetTickCount64` or equivalent). The clock choice, sharing semantics, 10-second bound, and OS behavior are unverified proposals, separate from UTC service-token expiry; measure/review before fixing implementation requirements. The same absolute deadline bounds credential write, child payload read and EOF, ACK write, and parent ACK read and EOF; do not restart a relative timeout for each operation. The OS process-creation call's own duration is outside this handoff timer and is a separate spawn-timeout/unknown-outcome design condition, not part of the 10 seconds.

The parent controller's authoritative watchdog must still fire when a child cannot read the payload. Pipe workers perform I/O without blocking the controller; watchdog arbitration at the deadline commits `Aborted`, terminates and confirms exit through the exact process handle, cancels worker I/O, closes pipes, joins workers, then destroys secret buffers. Windows anonymous pipes must not be assumed to support overlapped I/O automatically: choosing worker cancellation (for example `CancelSynchronousIo`) or another mechanism and proving it against the selected SDK/API behavior is an M0 verification requirement. Short/extra bytes, read/write error, nonce mismatch, timeout, child exit/early exit abort; no resend, argv secret fallback or automatic Offline fallback. Long-lived refresh credential remains owned by the keyring/auth owner. Existing legacy secret CLI is rejected during migration except explicit tokenless Offline launch.

For other OSes, online handoff is supported only where an equivalent same-process-instance stop capability (for example Linux `pidfd`) plus unnamed inherited FD allowlisting and cancellable, deadline-bounded I/O satisfies this contract. Unconditional Unix PID-based termination is not equivalent; platforms without the required capability fail closed and refuse online handoff. No secret argv fallback. The child necessarily holds a token in memory for session join, so memory non-exposure is not claimed. These platform APIs are future design only; none are executed or implemented in this task.

Online auth must bind challenge/session join to the same verified profile UUID and bearer credential. Encryption starts only after successful join and key-packet write. `shouldAuthenticate=false` is not authenticated identity; explicit offline is separate. Official LAN-only Authlib-error fallback (`minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientHandshakePacketListenerImpl.java:129-145`) is not generalized to remote login.

### Frame, liveness, event order, bundle, custom payload

Official source: `minecraft-26.2-decompiled/src/net/minecraft/network/Varint21FrameDecoder.java:26,39-48` accepts a maximum 3-byte VarInt21 prefix (21-bit maximum 2,097,151-byte frame); `minecraft-26.2-decompiled/src/net/minecraft/network/Varint21LengthFieldPrepender.java:23` confirms 3 bytes. `minecraft-26.2-decompiled/src/net/minecraft/network/CompressionDecoder.java:24-25,37-47,69-77`: source declares `MAXIMUM_COMPRESSED_LENGTH=0x200000` (2 MiB) and enforces uncompressed output max `0x800000` (8 MiB); a full source scan found no use of the compressed-limit constant, so enforcement of that 2 MiB value is unconfirmed and must not be asserted. Declared uncompressed length below server-selected threshold is rejected (except declared zero means uncompressed), >8 MiB rejected, actual/declaration mismatch rejected. Negotiated threshold is wire data, not one universal numeric threshold. `minecraft-26.2-decompiled/src/net/minecraft/network/Connection.java:471-485 pipeline,545-548 fromChannel/ReadTimeoutHandler(30)` remote read timeout is 30 seconds. Pomme `pomme-client/src/net/conn.rs:30,73-187` caps VarInt21 and bounded exact decompression; dependency writer behavior still requires verification. Official `minecraft-26.2-decompiled/src/net/minecraft/network/protocol/BundlerInfo.java:22,50-58` sets a 4096-packet bundle member limit (the 4097th add errors); preserve this exact source behavior.

Use bounded outbound/event queues with explicit policy: ordered gameplay/config/auth/control packets are never silently dropped; saturated producers backpressure or return visible failure; coalescing is allowed only for explicitly replaceable UI telemetry. Keepalive/control response has reserved timely processing while preserving related main-thread semantic order. Bound phase and idle-read timeout. Malformed length/over-limit frame/bad compressed stream/decrypt/codec corruption are terminal security errors; legal unknown packet identifier is separate and may be safely ignored/opaque/fail-closed only when frame boundary remains trusted. Never treat malformed as legal unknown. Cancel/drop closes transport, wakes waiters, emits one terminal event, releases session registry, scrubs secrets where possible.

Bundle delimiter is an ordered atomic packet group/end marker: contained packets dispatch in order without interleaving groups or crossing phase/epoch barriers. Official `minecraft-26.2-decompiled/src/net/minecraft/network/protocol/BundlerInfo.java:22,28-35,40-60` limits a bundle to 4096 members and wraps its ordered subpackets with delimiters; `minecraft-26.2-decompiled/src/net/minecraft/network/protocol/game/ClientboundBundlePacket.java:12-26`, `minecraft-26.2-decompiled/src/net/minecraft/network/Connection.java:249-279`, `minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java:2337-2343`; Pomme `connection.rs:1960-2311 game_loop`. Pomme implementation parity/limit remains to investigate. Custom payload official sources: `minecraft-26.2-decompiled/src/net/minecraft/network/protocol/common/ClientboundCustomPayloadPacket.java:27-28` and `minecraft-26.2-decompiled/src/net/minecraft/network/protocol/common/ServerboundCustomPayloadPacket.java:25` register built-in `BrandPayload` (`minecraft:brand`, `minecraft-26.2-decompiled/src/net/minecraft/network/protocol/common/custom/BrandPayload.java:10-23`); unknown IDs route through `DiscardedPayload.codec`, bounded at 0x100000 clientbound common / Short.MAX_VALUE serverbound common. Unknown identifier is legal unknown, not a malformed packet. Opaque retention only if exact bounded bytes can be losslessly re-emitted; otherwise explicit drop/log or fail closed. Feature owner, not protocol, decides payload meaning.

Concrete built-in custom-payload name inventory in these two packet codecs: one distinct registered identifier per direction, `minecraft:brand` (`BrandPayload`). `DiscardedPayload` is the bounded unknown-ID fallback, not another registered wire-name entry. Thus the source-cited inventory count is 1 clientbound / 1 serverbound / 1 distinct built-in identifier; this is not a claim about modded identifiers.

### Protocol denominators

Native 776 is first and no-loss goal. Ordered registration equality is not handler parity: 256 phase/direction IDs vs play clientbound 141 conservatively classified 111 semantic route, 4 transport/raw special, 26 scan-unknown. Unknown does not prove defect/no-op; generic route must be investigated. Packet tables and full unknown list are embedded below.

26.2 play clientbound unknown scan list (26): `3 award_stats; 5 block_destruction; 10 change_difficulty; 24 custom_payload; 26 debug/block_value; 27 debug/chunk_value; 28 debug/entity_value; 29 debug/event; 30 debug_sample; 39 game_rule_values; 40 game_test_highlight_pos; 50 low_disk_space_warning; 55 move_minecart_along_track; 62 pong_response; 66 player_combat_end; 67 player_combat_enter; 71 player_look_at; 85 select_advancements_tab; 86 server_data; 93 set_camera; 97 set_default_spawn_position; 100 set_entity_link; 123 tag_query; 126 test_instance_block_status; 135 projectile_power; 136 custom_report_details`. Four separate transport/raw routes: `0 bundle_delimiter`, `22 cooldown`, `47 level_particles`, `60 open_sign_editor`. Other raw+typed semantic routes counted once semantic.

Play serverbound 69 below are registrations only. Each generated caller and field equivalence is **not audited**. No claim of 69 caller parity. Audit outgoing producer, packet fields/defaults, conditions, ordering, and version conversion. Protocol bridge versions 777 and 775–763 remain separate explicitly lossy scope and never dilute 776 denominator. Unsupported conversion cannot silently strip fields/packets/registry IDs: opaque preserve only if lossless and bounded; else explicit loss/fail closed before semantic corruption. Distinguish legal optional ignored by official clients from semantic-mandatory state; unsupported optional alone does not force disconnect. Current adapter evidence `pomme-client/src/net/translate.rs:1-150,369-835,900-1559`.

## 現行実装と差分

| concern | Pomme path:line symbol | JE26.2 path:line symbol | plan |
|---|---|---|---|
| auth/UI/process | `pomme-launcher/src-tauri/src/auth.rs:163-325 callback`, `:328-405 service flow`; `pomme-launcher/src-tauri/src/commands.rs:480-562 spawn_game`; `pomme-client/src/args.rs:9-19 LaunchArgs`; `pomme-client/src/user.rs:3-36 UserData` | `minecraft-26.2-decompiled/src/net/minecraft/client/User.java:7-45 User`; `minecraft-26.2-decompiled/src/net/minecraft/client/Minecraft.java:406-423,814-815`; `minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientHandshakePacketListenerImpl.java:110-168 handleHello` | Current independent optional argv can degrade malformed identity to offline UUID; Tauri exposes token. Typed credential, protected handoff. Authlib internals unknown. |
| transport/frame | `pomme-client/src/net/conn.rs:30,73-187 MAX_FRAME_LENGTH/RawReader`; `pomme-client/src/net/connection.rs:99-149 spawn_connection` | `minecraft-26.2-decompiled/src/net/minecraft/network/Varint21FrameDecoder.java:26-54`; `minecraft-26.2-decompiled/src/net/minecraft/network/CompressionDecoder.java:24-77`; `minecraft-26.2-decompiled/src/net/minecraft/network/Connection.java:461-485,545-548 fromChannel/ReadTimeoutHandler(30),361 flushChannel` | Numeric static bounds broadly align; actual Azalea writer/dependency and idle liveness need checks; queue unbounded. |
| session/phase | `pomme-client/src/net/connection.rs:171-303 connect_recorded`, `:795-1209 config_sequence`, `:1960-2311 game_loop`; `pomme-client/src/net/translate.rs:369-835 Translation` | `minecraft-26.2-decompiled/src/net/minecraft/network/Connection.java:249-279`; `minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientHandshakePacketListenerImpl.java:170-201`; `minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientConfigurationPacketListenerImpl.java:80-205` | Process-global protocol/block table reset on drop and separate event/ack flow: session owner + write/consumer epoch barrier. |
| registry | `pomme-protocol/src/registries.rs:18-155 ClientRegistry/RegistryTable`, `:158-185 DynamicRegistries`; `pomme-client/src/net/connection.rs:991-1046 known-pack/config install` | `minecraft-26.2-decompiled/src/net/minecraft/resources/RegistryDataLoader.java:102-103`; `minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/RegistryDataCollector.java:63-167`; `minecraft-26.2-decompiled/src/net/minecraft/network/protocol/configuration/ClientboundRegistryDataPacket.java:23-26` | 17 static tables/received dynamic values do not establish 148 declarations/29 consumers/all-entry fidelity. Preserve order and missing payload positions. |
| packet handling | `pomme-client/src/net/connection.rs:1960-2311 game_loop`; `pomme-client/src/net/handler.rs:388-403,2564`; `pomme-client/src/net/chat.rs:277`; `pomme-client/src/net/dialog.rs:36` | `minecraft-26.2-decompiled/src/net/minecraft/network/protocol/handshake/HandshakeProtocols.java`, `minecraft-26.2-decompiled/src/net/minecraft/network/protocol/status/StatusProtocols.java`, `minecraft-26.2-decompiled/src/net/minecraft/network/protocol/login/LoginProtocols.java`, `minecraft-26.2-decompiled/src/net/minecraft/network/protocol/configuration/ConfigurationProtocols.java`, `minecraft-26.2-decompiled/src/net/minecraft/network/protocol/game/GameProtocols.java` and matching PacketTypes; `minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java:2337-2343` | 256 wire sequence match != 256 consumers. 111/4/26 split; audit all 69 SB producers. |
| memory/adjacent | `pomme-client/src/net/connection.rs:281-303,1129-1144,2239-2254`; `pomme-client/src/net/chat_security.rs:391-453` | `minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientCommonPacketListenerImpl.java:170-215,294-313`; `minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java:2638-2687` | Memory stream differs in transport but not phase/event contract. Transfer/cookies close old session and create destination. Chat message meaning remains chat owner; key identity lifecycle is auth. |

A=Pomme remote TCP vs official dedicated server. B=SteelMC singleplayer Memory vs official integrated server; connection/transport comparison only. Steel pin source exists in the read-only root tree but has not been inspected for this plan; this owned tree has no checkout, and B remains unperformed.

## 互換にする設計

1. **Credential invariant:** OnlineVerified only with canonical valid UUID, profile/name source, service-derived expiry, coherent refresh result and explicit persistence outcome. ExplicitOffline has no bearer token. Callback order listener ready → exact state compare → OAuth status/error → code parse/decode → PKCE exchange. Escape reflected HTML, cancel all waits, classify HTTP status. Never downgrade refresh/profile mismatch/keyring failure to offline or claim durable success falsely. Keep secrets out of argv/log/UI/errors. One-shot handoff is unpredictable, bound to receiver/process, expiring, non-replayable and atomically consumed; minimize/zeroize buffers; keyring failure has explicit refusal or documented nonpersistent policy.
2. **ConnectionSession authority:** Own transport, cipher/compression, protocol/phase, identity reference, registry epoch, bounded queues and cancellation. No process-global protocol/table mutation on drop. Decode and semantic events use `(protocol, phase, epoch)`. On Reconfiguration stop old decode/dispatch, drain/apply or invalidate pending old events, atomically update registry/tags/known packs, fully write ConfigurationAcknowledged, then activate new epoch/resume Play. Queue enqueue is not ACK completion.
3. **WireRegistry:** Keep server-ordered registry entries, including payload-missing known-pack positions, with protocol/epoch/key/payload-presence/tags/known-pack proof. Reject duplicates/malformed key/illegal IDs by explicit policy. Mapping by key/name is versioned; absent mapping never guesses ID. Resources resolves bytes, semantic features interpret, protocol transports/validates opaque values. Freeze data pack fixture before entry fidelity claim.
4. **Loss/unknown policy:** Native 776 must roundtrip every supported field without silent stripping. Track version/direction/phase/packet/field/component/registry losses for 777/775–763. Unsupported mandatory conversion fails before use; opaque retention only if bounded and provably byte-lossless. Unknown packet ID, legal unknown custom payload, missing registry entry, malformed packet and unsupported feature are distinct. Continue when official-compatible optional can be ignored; stop only for malformed/security violation or unrepresentable mandatory semantic state. Every packet has reviewed semantic, safe explicit ignore/transport, or fail-closed route; unknown optional does not automatically disconnect.
5. **Backpressure/lifecycle:** Bound queues with capacities selected from measured memory budgets (not invented protocol constants). Control keepalive/cookie/ack/cancel gets timely processing without violating causal feature-event order. Full UI queue blocks cancellation-aware producer; coalesce only explicitly replaceable state snapshots. Surface outbound saturation; no unbounded channel. Separate TCP connect/status/phase/idle deadlines. Cancel/drop closes transport, wakes waiters, emits one terminal event, releases epoch and scrubs auth/cipher state where possible. Memory and remote share phase/event contract.
6. **Bundle/custom payload:** Bundle delimiter groups ordered packets, no interleaving or phase/epoch crossing; verify official bundler's exact atomic rules before implementation. Decode built-in brand and enforce official per-direction custom payload bounds; unknown identifiers can be safely opaque only with bounded exact re-emission, else explicit policy. Custom payload meaning belongs to feature owner.
7. **Adjacent contracts:** launcher prepares assets/spawns/requests one-shot credential only. Chat owns rich/secure message semantics; auth owns verified identity/profile-key fetch, expiry, refresh/revocation and matching UUID. Feature modules own semantic use/prediction; server-gameplay remains authoritative. singleplayer-lifecycle/Memory receives typed opened/closed/reason; server-world/gameplay owns authoritative world registries/providers; resources owns pack load/reload and required accept/reject; HUD owns notice rendering.

## 実装順序

| milestone | scope/dependencies | completion/check gate | limits even at 100% |
|---|---|---|---|
| **M0 security/identity first** | Typed identity and proposed Windows anonymous inherited-pipe handoff above (select only after SDK/API prototype/review); argv/UI token removal; callback state/status/escape/cancel/timeout; profile/token coherence; keyring errors; malformed frame bounds and cleanup. | Source audit every credential sink; reject legacy secret CLI except explicit tokenless Offline. Future comparison gate (not run): partial/mismatch/callback/replay/cancel/malformed-frame; duplicate launch, concurrent consumption, non-inherited process, invalid handle, wrong nonce; 64 KiB exact/over and envelope larger than OS pipe capacity with concurrent write/read; payload plus extra byte, EOF withheld and no ACK before EOF; ACK exact length/nonce plus EOF; early ACK racing Sent; Sent-vs-timeout-vs-child-exit arbitration and late ACK after either terminal state; PID-reuse simulation proves another process instance is not stopped; blocked write/read and missing EOF prove deadline watchdog terminates exact child, cancels I/O and reaps workers; verify `t0` spawn timestamp and 10-second handoff deadline are distinct, including spawn-call duration outside the handoff timer. Demonstrate chosen Windows anonymous-pipe cancellation against selected SDK/API; do not assume overlapped support. Verify token absent argv/env/logs. Explicitly exclude same-user/admin memory inspection and child token memory required for session join from threat claims. This is a blocking M0 gate and remains 100% unverified until all cases pass; secret exposure scan. | Authlib/Microsoft internals unknown; OS/runtime zeroization is best-effort; no live auth without separate approval. |
| **M1 native 776 no-loss session** | ConnectionSession ownership, phase model, codec/identity/registry epoch; ack-write+queued-event barrier; bounded backpressure/cancel/keepalive; remote and Memory. Depends M0. | All 256 protocol codecs roundtrip; frame boundaries, phase/reconfig race, epoch isolation, native no silent loss. Then authorized A/B cases. | Does not establish external service, all gameplay semantics, every server's resources. |
| **M2 dispatch/custom payload** | Trace 111 semantic routes to features; resolve 26 unknown CBs and 4 transport routes; audit all 69 SB caller(s)/fields; built-in brand and unknown custom identifiers. | For every row, record the full Pomme chain `phase/direction/id decode -> game_loop dispatch -> typed/raw handler -> concrete state/UI/network consumer`, and the official chain `protocol registration/type -> listener dispatch -> handle method -> resulting consumer/state`; cite both sides' root-relative file:line:symbol. Packet definition/codec presence alone does not count as implementation. Each unknown resolves to evidenced semantic route, official-safe ignore, transport special, or explicit unsupported policy; all 69 outbound origins/fields audited; custom bounds/opaque behavior cases. | Rare states/modded payload IDs remain outside named fixtures. |
| **M3 reconfiguration/transfer/registry** | Ordered IDs/tags/payloads, known packs, transfer/cookies/identity, bundle group, resources contract. Depends M1/M2. | Delayed ack/queued events never cross epoch; transfer closes old once and creates fresh session; omitted known-pack retains slot; order/tag/registry fixtures compared. | Per-server data packs vary; only pinned fixture sets covered. |
| **M4 comparative fidelity** | Explicit version bridges and loss matrix for 777/775–763; authorized A and B comparisons. | Named cases all report pass/fail/unrun; no aggregate % for unknown denominator. | 100% means only declared 26.2 fixtures/phases/fields, not arbitrary mods, external auth services, or nonconnection gameplay. |

## 完了条件と比較ケース

The acceptance bar is a future scoped target, not a current compatibility assertion: native JE 26.2 / protocol 776 only, against the listed packet/field, state, registry-fixture, and consumer boundaries. No statement guarantees every protocol version, registry consumer, server, mod, or play behavior.

All comparisons below are **future work and not run**. Start with fake auth/network and fixed fixture hashes; live servers/app/auth require separate permission. Record phase/direction/id/name, protocol, field fingerprint, registry epoch, event order, semantic result and status. Unknown denominator is never turned into a global compatibility percentage. Static route presence is not runtime parity.

- **Auth/security:** premium success exact UUID/name; absent token only explicit offline; partial/malformed UUID+token; profile mismatch; expired token with refresh rotation; refresh/profile failure; missing refresh; keyring unavailable/read/write/delete; callback mismatch even with OAuth error; malformed query; HTML injection escaped; replay; cancel before listener/after code/during exchange; timeout/bind error. Verify token absent argv, logs, Tauri state/errors/crash diagnostics; one-shot replay/expiry/wrong handle denied; duplicate launch/concurrent consume, non-inherited process/invalid handle, wrong nonce; exact 64 KiB/over and payload larger than OS pipe capacity with concurrent writer/reader; payload+extra, withheld EOF/no ACK, exact ACK+EOF, ACK racing Sent, late ACK after Acked/Aborted, Sent-vs-timeout-vs-childexit single-winner arbitration; PID reuse must not stop another process instance; blocked write/read or missing EOF must trigger the `t0 + 10s` watchdog, stop via exact process handle, cancel I/O and join workers; separately prove spawn-call duration is outside handoff timer. Validate Windows anonymous-pipe cancellation against selected SDK/API. These are future unrun cases. Token absent argv/env/logs; exclude child memory required for session join and same-user/admin inspection from non-exposure claims; inspect zeroization limits.
- **Frame/liveness:** prefix 0/valid 1-3 byte/max 2,097,151/>max/third-byte continuation; partial read and cancellation; compression threshold −1/equal/+1 and declared zero, 8,388,608/+1, 2MiB compressed boundary, short/long decompressed, invalid/truncated zlib; cipher/decode error. Assert no unbounded allocation, malformed terminal vs legal unknown separate. Saturated queues with keepalive; main-thread order/backpressure; connect/status/login/config/reconfig/idle timeout; drop releases session and emits one terminal event.
- **Phase/epoch/bundle:** Status probe reaches Terminal; selecting a server creates a fresh connection whose first intention is LOGIN, never Status-to-Login on the probe. LoginAcknowledged and ConfigurationAcknowledged wait for actual writer completion; queued events around reconfig cannot deliver epoch N+1 IDs to epoch N handlers. Bundle inner order/end marker/no interleave/no barrier cross; first establish official bundler cap/atomicity rather than inventing count. Abort midway and boundary packet tests.
- **Registry/config:** all 29 synchronized keys; exact ordered IDs, payload presence/omitted known-pack positions, tags, enabled features, data values against fixed official data/resource pack hash. Reconfig unchanged/omitted/replaced sets. Duplicate/unknown key and malformed ID policy. 148 declarations remain separate denominator. No entry parity until fixtures run.
- **Transfer/identity/cookie:** login/config/play cookies and transfer; destination is new connection and keeps only explicitly allowed identity/cookies; expiry/revalidation, transfer fail/loop/cancel, stale old-session event, cookie bounds. Never carry old protocol/registry epoch to destination.
- **Packets/callers:** roundtrip all 256 registered packet IDs/fields at native 776; specifically review each unknown 26 handler/custom_payload identifier; list each of 69 SB producers, conditions, defaults, fields, ordering and mapping. Decode/route alone not semantic equivalence. Feature-owner gates test gameplay; server authority remains server-side.
- **A/B:** A=Pomme remote TCP→official dedicated; B=SteelMC singleplayer Memory→official integrated. This plan compares only transport/phase/events, not singleplayer world gameplay. Test remote vs Memory read/write/close/backpressure/cancel and registry epochs, log fixture/server SHA and scrub secrets. B remains unrun/Steel source unknown.

## 依存・未確認事項

- The owned tree lacks a Steel checkout. Root submodule pin `0b1f87c36a664f08d81397e942fb1e19f6eb282b` and clean state were checked read-only; Steel implementation review and B comparison remain unperformed. No submodule update/init was performed.
- Authlib/Microsoft/Xbox/Minecraft Services internals absent from supplied official source. Verify separately; current `auth.rs:399-400` hard-codes `unix_now() + 86400`, not provider expiry. Current keyring read/write/parse failures can be swallowed (`auth.rs:68-89,125-159`); current callback checks OAuth error before state and reflects HTML without escaping (`auth.rs:265-327`). Profile-key endpoint/revocation behavior needs auth owner confirmation.
- Per-server dynamic registry values/tags, plugins/mods and every official registry consumer are not exhaustively inspected. 29 sync definition is at `minecraft-26.2-decompiled/src/net/minecraft/resources/RegistryDataLoader.java:102-103`; 148 declarations/147 unique keys because DIMENSION and LEVEL_STEM share a key. Static registry count is not consumer coverage.
- Pomme SB 69 callers/field equivalence uninvestigated; CB 26 is conservative source scan; all typed decode/dependency-writer/bundle semantics and event consumer timing need follow-up. singleplayer Memory lifecycle outside connection scope needs typed owner review.
- Source-grounded limits: 3-byte VarInt21 / 21-bit frame length, declared compressed-input constant 2MiB whose enforcement use was not found, decompressed output max 8MiB, server-negotiated threshold, remote timeout 30s, and official bundle group max 4096. Queue capacities must be measured, not fabricated.

## 付録A: 256 registered packet IDs and ordered names

Pomme source `pomme-protocol/src/data/protocol-26.2.json`; official comparisons: `minecraft-26.2-decompiled/src/net/minecraft/network/protocol/handshake/HandshakeProtocols.java`, `minecraft-26.2-decompiled/src/net/minecraft/network/protocol/status/StatusProtocols.java`, `minecraft-26.2-decompiled/src/net/minecraft/network/protocol/login/LoginProtocols.java`, `minecraft-26.2-decompiled/src/net/minecraft/network/protocol/configuration/ConfigurationProtocols.java`, `minecraft-26.2-decompiled/src/net/minecraft/network/protocol/game/GameProtocols.java` and PacketTypes. All 10 phase/direction lists match order, including empty Handshake clientbound; this is static equality only. Tables follow in full.

### handshake serverbound (1)
| id | name |
|---:|---|
| 0 | `intention` |

### handshake clientbound (0)
— (登録なし)

### status serverbound (2)
| id | name |
|---:|---|
| 0 | `status_request` |
| 1 | `ping_request` |

### status clientbound (2)
| id | name |
|---:|---|
| 0 | `status_response` |
| 1 | `pong_response` |

### login serverbound (5)
| id | name |
|---:|---|
| 0 | `hello` |
| 1 | `key` |
| 2 | `custom_query_answer` |
| 3 | `login_acknowledged` |
| 4 | `cookie_response` |

### login clientbound (6)
| id | name |
|---:|---|
| 0 | `login_disconnect` |
| 1 | `hello` |
| 2 | `login_finished` |
| 3 | `login_compression` |
| 4 | `custom_query` |
| 5 | `cookie_request` |

### configuration serverbound (10)
| id | name |
|---:|---|
| 0 | `client_information` |
| 1 | `cookie_response` |
| 2 | `custom_payload` |
| 3 | `finish_configuration` |
| 4 | `keep_alive` |
| 5 | `pong` |
| 6 | `resource_pack` |
| 7 | `select_known_packs` |
| 8 | `custom_click_action` |
| 9 | `accept_code_of_conduct` |

### configuration clientbound (20)
| id | name |
|---:|---|
| 0 | `cookie_request` |
| 1 | `custom_payload` |
| 2 | `disconnect` |
| 3 | `finish_configuration` |
| 4 | `keep_alive` |
| 5 | `ping` |
| 6 | `reset_chat` |
| 7 | `registry_data` |
| 8 | `resource_pack_pop` |
| 9 | `resource_pack_push` |
| 10 | `store_cookie` |
| 11 | `transfer` |
| 12 | `update_enabled_features` |
| 13 | `update_tags` |
| 14 | `select_known_packs` |
| 15 | `custom_report_details` |
| 16 | `server_links` |
| 17 | `clear_dialog` |
| 18 | `show_dialog` |
| 19 | `code_of_conduct` |

### game serverbound (69)
| id | name |
|---:|---|
| 0 | `accept_teleportation` |
| 1 | `attack` |
| 2 | `block_entity_tag_query` |
| 3 | `bundle_item_selected` |
| 4 | `change_difficulty` |
| 5 | `change_game_mode` |
| 6 | `chat_ack` |
| 7 | `chat_command` |
| 8 | `chat_command_signed` |
| 9 | `chat` |
| 10 | `chat_session_update` |
| 11 | `chunk_batch_received` |
| 12 | `client_command` |
| 13 | `client_tick_end` |
| 14 | `client_information` |
| 15 | `command_suggestion` |
| 16 | `configuration_acknowledged` |
| 17 | `container_button_click` |
| 18 | `container_click` |
| 19 | `container_close` |
| 20 | `container_slot_state_changed` |
| 21 | `cookie_response` |
| 22 | `custom_payload` |
| 23 | `debug_subscription_request` |
| 24 | `edit_book` |
| 25 | `entity_tag_query` |
| 26 | `interact` |
| 27 | `jigsaw_generate` |
| 28 | `keep_alive` |
| 29 | `lock_difficulty` |
| 30 | `move_player_pos` |
| 31 | `move_player_pos_rot` |
| 32 | `move_player_rot` |
| 33 | `move_player_status_only` |
| 34 | `move_vehicle` |
| 35 | `paddle_boat` |
| 36 | `pick_item_from_block` |
| 37 | `pick_item_from_entity` |
| 38 | `ping_request` |
| 39 | `place_recipe` |
| 40 | `player_abilities` |
| 41 | `player_action` |
| 42 | `player_command` |
| 43 | `player_input` |
| 44 | `player_loaded` |
| 45 | `pong` |
| 46 | `recipe_book_change_settings` |
| 47 | `recipe_book_seen_recipe` |
| 48 | `rename_item` |
| 49 | `resource_pack` |
| 50 | `seen_advancements` |
| 51 | `select_trade` |
| 52 | `set_beacon` |
| 53 | `set_carried_item` |
| 54 | `set_command_block` |
| 55 | `set_command_minecart` |
| 56 | `set_creative_mode_slot` |
| 57 | `set_game_rule` |
| 58 | `set_jigsaw_block` |
| 59 | `set_structure_block` |
| 60 | `set_test_block` |
| 61 | `sign_update` |
| 62 | `spectator_action` |
| 63 | `swing` |
| 64 | `teleport_to_entity` |
| 65 | `test_instance_block_action` |
| 66 | `use_item_on` |
| 67 | `use_item` |
| 68 | `custom_click_action` |

### game clientbound (141)
| id | name |
|---:|---|
| 0 | `bundle_delimiter` |
| 1 | `add_entity` |
| 2 | `animate` |
| 3 | `award_stats` |
| 4 | `block_changed_ack` |
| 5 | `block_destruction` |
| 6 | `block_entity_data` |
| 7 | `block_event` |
| 8 | `block_update` |
| 9 | `boss_event` |
| 10 | `change_difficulty` |
| 11 | `chunk_batch_finished` |
| 12 | `chunk_batch_start` |
| 13 | `chunks_biomes` |
| 14 | `clear_titles` |
| 15 | `command_suggestions` |
| 16 | `commands` |
| 17 | `container_close` |
| 18 | `container_set_content` |
| 19 | `container_set_data` |
| 20 | `container_set_slot` |
| 21 | `cookie_request` |
| 22 | `cooldown` |
| 23 | `custom_chat_completions` |
| 24 | `custom_payload` |
| 25 | `damage_event` |
| 26 | `debug/block_value` |
| 27 | `debug/chunk_value` |
| 28 | `debug/entity_value` |
| 29 | `debug/event` |
| 30 | `debug_sample` |
| 31 | `delete_chat` |
| 32 | `disconnect` |
| 33 | `disguised_chat` |
| 34 | `entity_event` |
| 35 | `entity_position_sync` |
| 36 | `explode` |
| 37 | `forget_level_chunk` |
| 38 | `game_event` |
| 39 | `game_rule_values` |
| 40 | `game_test_highlight_pos` |
| 41 | `mount_screen_open` |
| 42 | `hurt_animation` |
| 43 | `initialize_border` |
| 44 | `keep_alive` |
| 45 | `level_chunk_with_light` |
| 46 | `level_event` |
| 47 | `level_particles` |
| 48 | `light_update` |
| 49 | `login` |
| 50 | `low_disk_space_warning` |
| 51 | `map_item_data` |
| 52 | `merchant_offers` |
| 53 | `move_entity_pos` |
| 54 | `move_entity_pos_rot` |
| 55 | `move_minecart_along_track` |
| 56 | `move_entity_rot` |
| 57 | `move_vehicle` |
| 58 | `open_book` |
| 59 | `open_screen` |
| 60 | `open_sign_editor` |
| 61 | `ping` |
| 62 | `pong_response` |
| 63 | `place_ghost_recipe` |
| 64 | `player_abilities` |
| 65 | `player_chat` |
| 66 | `player_combat_end` |
| 67 | `player_combat_enter` |
| 68 | `player_combat_kill` |
| 69 | `player_info_remove` |
| 70 | `player_info_update` |
| 71 | `player_look_at` |
| 72 | `player_position` |
| 73 | `player_rotation` |
| 74 | `recipe_book_add` |
| 75 | `recipe_book_remove` |
| 76 | `recipe_book_settings` |
| 77 | `remove_entities` |
| 78 | `remove_mob_effect` |
| 79 | `reset_score` |
| 80 | `resource_pack_pop` |
| 81 | `resource_pack_push` |
| 82 | `respawn` |
| 83 | `rotate_head` |
| 84 | `section_blocks_update` |
| 85 | `select_advancements_tab` |
| 86 | `server_data` |
| 87 | `set_action_bar_text` |
| 88 | `set_border_center` |
| 89 | `set_border_lerp_size` |
| 90 | `set_border_size` |
| 91 | `set_border_warning_delay` |
| 92 | `set_border_warning_distance` |
| 93 | `set_camera` |
| 94 | `set_chunk_cache_center` |
| 95 | `set_chunk_cache_radius` |
| 96 | `set_cursor_item` |
| 97 | `set_default_spawn_position` |
| 98 | `set_display_objective` |
| 99 | `set_entity_data` |
| 100 | `set_entity_link` |
| 101 | `set_entity_motion` |
| 102 | `set_equipment` |
| 103 | `set_experience` |
| 104 | `set_health` |
| 105 | `set_held_slot` |
| 106 | `set_objective` |
| 107 | `set_passengers` |
| 108 | `set_player_inventory` |
| 109 | `set_player_team` |
| 110 | `set_score` |
| 111 | `set_simulation_distance` |
| 112 | `set_subtitle_text` |
| 113 | `set_time` |
| 114 | `set_title_text` |
| 115 | `set_titles_animation` |
| 116 | `sound_entity` |
| 117 | `sound` |
| 118 | `start_configuration` |
| 119 | `stop_sound` |
| 120 | `store_cookie` |
| 121 | `system_chat` |
| 122 | `tab_list` |
| 123 | `tag_query` |
| 124 | `take_item_entity` |
| 125 | `teleport_entity` |
| 126 | `test_instance_block_status` |
| 127 | `ticking_state` |
| 128 | `ticking_step` |
| 129 | `transfer` |
| 130 | `update_advancements` |
| 131 | `update_attributes` |
| 132 | `update_mob_effect` |
| 133 | `update_recipes` |
| 134 | `update_tags` |
| 135 | `projectile_power` |
| 136 | `custom_report_details` |
| 137 | `server_links` |
| 138 | `waypoint` |
| 139 | `clear_dialog` |
| 140 | `show_dialog` |

## 付録B: Play Clientbound dispatch scan (141)

Source: `pomme-client/src/net/connection.rs:1960 game_loop`; `pomme-client/src/net/handler.rs:388-403 handle_game_packet_with_display_text`, `:2564 handle_raw_game_packet_with_translation`; raw chat/dialog `pomme-client/src/net/chat.rs:277`, `pomme-client/src/net/dialog.rs:36`. `semantic-owner`はpacket-specific routeをstatic sourceで発見したという意味だけで、field同値/実動作を意味しない。

### 111 semantic route (id:name)

`1:add_entity`, `2:animate`, `4:block_changed_ack`, `6:block_entity_data`, `7:block_event`, `8:block_update`, `9:boss_event`, `11:chunk_batch_finished`
`12:chunk_batch_start`, `13:chunks_biomes`, `14:clear_titles`, `15:command_suggestions`, `16:commands`, `17:container_close`, `18:container_set_content`, `19:container_set_data`
`20:container_set_slot`, `21:cookie_request`, `23:custom_chat_completions`, `25:damage_event`, `31:delete_chat`, `32:disconnect`, `33:disguised_chat`, `34:entity_event`
`35:entity_position_sync`, `36:explode`, `37:forget_level_chunk`, `38:game_event`, `41:mount_screen_open`, `42:hurt_animation`, `43:initialize_border`, `44:keep_alive`
`45:level_chunk_with_light`, `46:level_event`, `48:light_update`, `49:login`, `51:map_item_data`, `52:merchant_offers`, `53:move_entity_pos`, `54:move_entity_pos_rot`
`56:move_entity_rot`, `57:move_vehicle`, `58:open_book`, `59:open_screen`, `61:ping`, `63:place_ghost_recipe`, `64:player_abilities`, `65:player_chat`
`68:player_combat_kill`, `69:player_info_remove`, `70:player_info_update`, `72:player_position`, `73:player_rotation`, `74:recipe_book_add`, `75:recipe_book_remove`, `76:recipe_book_settings`
`77:remove_entities`, `78:remove_mob_effect`, `79:reset_score`, `80:resource_pack_pop`, `81:resource_pack_push`, `82:respawn`, `83:rotate_head`, `84:section_blocks_update`
`87:set_action_bar_text`, `88:set_border_center`, `89:set_border_lerp_size`, `90:set_border_size`, `91:set_border_warning_delay`, `92:set_border_warning_distance`, `94:set_chunk_cache_center`, `95:set_chunk_cache_radius`
`96:set_cursor_item`, `98:set_display_objective`, `99:set_entity_data`, `101:set_entity_motion`, `102:set_equipment`, `103:set_experience`, `104:set_health`, `105:set_held_slot`
`106:set_objective`, `107:set_passengers`, `108:set_player_inventory`, `109:set_player_team`, `110:set_score`, `111:set_simulation_distance`, `112:set_subtitle_text`, `113:set_time`
`114:set_title_text`, `115:set_titles_animation`, `116:sound_entity`, `117:sound`, `118:start_configuration`, `119:stop_sound`, `120:store_cookie`, `121:system_chat`
`122:tab_list`, `124:take_item_entity`, `125:teleport_entity`, `127:ticking_state`, `128:ticking_step`, `129:transfer`, `130:update_advancements`, `131:update_attributes`
`132:update_mob_effect`, `133:update_recipes`, `134:update_tags`, `137:server_links`, `138:waypoint`, `139:clear_dialog`, `140:show_dialog`

### 4 transport/raw special routes

| id | name | scan note | source anchor |
|---:|---|---|---|
| 0 | `bundle_delimiter` | bundle framing delimiter (registered first via official withBundlePacket); no standalone semantic payload | `minecraft-26.2-decompiled/src/net/minecraft/network/protocol/game/GameProtocols.java:248 CLIENTBOUND_TEMPLATE withBundlePacket first registration` |
| 22 | `cooldown` | raw special handler consumes/decodes packet | `pomme-client/src/net/handler.rs:2564 handle_raw_game_packet_with_translation` |
| 47 | `level_particles` | raw special handler consumes/decodes packet | `pomme-client/src/net/handler.rs:2564 handle_raw_game_packet_with_translation` |
| 60 | `open_sign_editor` | raw special handler consumes/decodes packet | `pomme-client/src/net/handler.rs:2564 handle_raw_game_packet_with_translation` |

### 26 unknown static-scan entries

Generic decode/consumer or intentional no-op remains unresolved; not a proven behavior defect or proof of absent typed decoding.

| id | name | typed variant | result |
|---:|---|---|---|
| 3 | `award_stats` | `AwardStats` | production typed arm/raw-special未発見; generic consumer/no-op未解決 |
| 5 | `block_destruction` | `BlockDestruction` | production typed arm/raw-special未発見; generic consumer/no-op未解決 |
| 10 | `change_difficulty` | `ChangeDifficulty` | production typed arm/raw-special未発見; generic consumer/no-op未解決 |
| 24 | `custom_payload` | `CustomPayload` | production typed arm/raw-special未発見; generic consumer/no-op未解決 |
| 26 | `debug/block_value` | `Debug/BlockValue` | production typed arm/raw-special未発見; generic consumer/no-op未解決 |
| 27 | `debug/chunk_value` | `Debug/ChunkValue` | production typed arm/raw-special未発見; generic consumer/no-op未解決 |
| 28 | `debug/entity_value` | `Debug/EntityValue` | production typed arm/raw-special未発見; generic consumer/no-op未解決 |
| 29 | `debug/event` | `Debug/Event` | production typed arm/raw-special未発見; generic consumer/no-op未解決 |
| 30 | `debug_sample` | `DebugSample` | production typed arm/raw-special未発見; generic consumer/no-op未解決 |
| 39 | `game_rule_values` | `GameRuleValues` | production typed arm/raw-special未発見; generic consumer/no-op未解決 |
| 40 | `game_test_highlight_pos` | `GameTestHighlightPos` | production typed arm/raw-special未発見; generic consumer/no-op未解決 |
| 50 | `low_disk_space_warning` | `LowDiskSpaceWarning` | production typed arm/raw-special未発見; generic consumer/no-op未解決 |
| 55 | `move_minecart_along_track` | `MoveMinecart` | production typed arm/raw-special未発見; generic consumer/no-op未解決 |
| 62 | `pong_response` | `PongResponse` | production typed arm/raw-special未発見; generic consumer/no-op未解決 |
| 66 | `player_combat_end` | `PlayerCombatEnd` | production typed arm/raw-special未発見; generic consumer/no-op未解決 |
| 67 | `player_combat_enter` | `PlayerCombatEnter` | production typed arm/raw-special未発見; generic consumer/no-op未解決 |
| 71 | `player_look_at` | `PlayerLookAt` | production typed arm/raw-special未発見; generic consumer/no-op未解決 |
| 85 | `select_advancements_tab` | `SelectAdvancementsTab` | production typed arm/raw-special未発見; generic consumer/no-op未解決 |
| 86 | `server_data` | `ServerData` | production typed arm/raw-special未発見; generic consumer/no-op未解決 |
| 93 | `set_camera` | `SetCamera` | production typed arm/raw-special未発見; generic consumer/no-op未解決 |
| 97 | `set_default_spawn_position` | `SetDefaultSpawnPosition` | production typed arm/raw-special未発見; generic consumer/no-op未解決 |
| 100 | `set_entity_link` | `SetEntityLink` | production typed arm/raw-special未発見; generic consumer/no-op未解決 |
| 123 | `tag_query` | `TagQuery` | production typed arm/raw-special未発見; generic consumer/no-op未解決 |
| 126 | `test_instance_block_status` | `TestInstanceBlockStatus` | production typed arm/raw-special未発見; generic consumer/no-op未解決 |
| 135 | `projectile_power` | `ProjectilePower` | production typed arm/raw-special未発見; generic consumer/no-op未解決 |
| 136 | `custom_report_details` | `CustomReportDetails` | production typed arm/raw-special未発見; generic consumer/no-op未解決 |

## 付録C: 148 official registry declarations

`class` is generic `T` from `ResourceKey<Registry<T>>`; official source is declaration source path:1-based line. Pomme table `—` means table not identified, not semantic coverage. Counts: 29 sync dynamic; 18 other dynamic; 95 static (17 table + 78 without identified table); 5 manual/pseudo; 1 runtime unknown. Total 148 declarations, 147 distinct keys due to DIMENSION/LEVEL_STEM sharing `minecraft:dimension`.

| declaration | key | class T | classification | Pomme static table | official declaration source |
|---|---|---|---|---|---|
| `ACTIVITY` | `minecraft:activity` | `Activity` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:156` |
| `ATTRIBUTE` | `minecraft:attribute` | `Attribute` | built-in/static, pomme static ID table present | `attribute` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:157` |
| `BIOME_SOURCE` | `minecraft:worldgen/biome_source` | `MapCodec<? extends BiomeSource>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:158` |
| `BLOCK_ENTITY_TYPE` | `minecraft:block_entity_type` | `BlockEntityType<?>` | built-in/static, pomme static ID table present | `block_entity_type` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:159` |
| `BLOCK_PREDICATE_TYPE` | `minecraft:block_predicate_type` | `BlockPredicateType<?>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:160` |
| `BLOCK_STATE_PROVIDER_TYPE` | `minecraft:worldgen/block_state_provider_type` | `BlockStateProviderType<?>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:161` |
| `BLOCK_TYPE` | `minecraft:block_type` | `MapCodec<? extends Block>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:162` |
| `BLOCK` | `minecraft:block` | `Block` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:163` |
| `CARVER` | `minecraft:worldgen/carver` | `WorldCarver<?>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:164` |
| `CHUNK_GENERATOR` | `minecraft:worldgen/chunk_generator` | `MapCodec<? extends ChunkGenerator>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:165` |
| `CHUNK_STATUS` | `minecraft:chunk_status` | `ChunkStatus` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:166` |
| `COMMAND_ARGUMENT_TYPE` | `minecraft:command_argument_type` | `ArgumentTypeInfo<?, ?>` | built-in/static, pomme static ID table present | `command_argument_type` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:167` |
| `CONSUME_EFFECT_TYPE` | `minecraft:consume_effect_type` | `ConsumeEffect.Type<?>` | built-in/static, pomme static ID table present | `consume_effect_type` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:168` |
| `CREATIVE_MODE_TAB` | `minecraft:creative_mode_tab` | `CreativeModeTab` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:169` |
| `CUSTOM_STAT` | `minecraft:custom_stat` | `Identifier` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:170` |
| `DATA_COMPONENT_PREDICATE_TYPE` | `minecraft:data_component_predicate_type` | `DataComponentPredicate.Type<?>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:171` |
| `DATA_COMPONENT_TYPE` | `minecraft:data_component_type` | `DataComponentType<?>` | built-in/static, pomme static ID table present | `data_component_type` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:172` |
| `GAME_RULE` | `minecraft:game_rule` | `GameRule<?>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:173` |
| `DEBUG_SUBSCRIPTION` | `minecraft:debug_subscription` | `DebugSubscription<?>` | built-in/static, pomme static ID table present | `debug_subscription` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:174` |
| `DECORATED_POT_PATTERN` | `minecraft:decorated_pot_pattern` | `DecoratedPotPattern` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:175` |
| `DENSITY_FUNCTION_TYPE` | `minecraft:worldgen/density_function_type` | `MapCodec<? extends DensityFunction>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:176` |
| `DIALOG_BODY_TYPE` | `minecraft:dialog_body_type` | `MapCodec<? extends DialogBody>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:177` |
| `DIALOG_TYPE` | `minecraft:dialog_type` | `MapCodec<? extends Dialog>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:178` |
| `ENCHANTMENT_EFFECT_COMPONENT_TYPE` | `minecraft:enchantment_effect_component_type` | `DataComponentType<?>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:179` |
| `ENCHANTMENT_ENTITY_EFFECT_TYPE` | `minecraft:enchantment_entity_effect_type` | `MapCodec<? extends EnchantmentEntityEffect>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:180` |
| `ENCHANTMENT_LEVEL_BASED_VALUE_TYPE` | `minecraft:enchantment_level_based_value_type` | `MapCodec<? extends LevelBasedValue>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:181` |
| `ENCHANTMENT_LOCATION_BASED_EFFECT_TYPE` | `minecraft:enchantment_location_based_effect_type` | `MapCodec<? extends EnchantmentLocationBasedEffect>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:182` |
| `ENCHANTMENT_PROVIDER_TYPE` | `minecraft:enchantment_provider_type` | `MapCodec<? extends EnchantmentProvider>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:183` |
| `ENCHANTMENT_VALUE_EFFECT_TYPE` | `minecraft:enchantment_value_effect_type` | `MapCodec<? extends EnchantmentValueEffect>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:184` |
| `ENTITY_SUB_PREDICATE_TYPE` | `minecraft:entity_sub_predicate_type` | `Codec<? extends EntitySubPredicate>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:185` |
| `ENTITY_TYPE` | `minecraft:entity_type` | `EntityType<?>` | built-in/static, pomme static ID table present | `entity_type` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:186` |
| `ENVIRONMENT_ATTRIBUTE` | `minecraft:environment_attribute` | `EnvironmentAttribute<?>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:187` |
| `ATTRIBUTE_TYPE` | `minecraft:attribute_type` | `AttributeType<?>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:188` |
| `FEATURE_SIZE_TYPE` | `minecraft:worldgen/feature_size_type` | `FeatureSizeType<?>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:189` |
| `FEATURE` | `minecraft:worldgen/feature` | `Feature<?>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:190` |
| `FLOAT_PROVIDER_TYPE` | `minecraft:float_provider_type` | `MapCodec<? extends FloatProvider>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:191` |
| `FLUID` | `minecraft:fluid` | `Fluid` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:192` |
| `FOLIAGE_PLACER_TYPE` | `minecraft:worldgen/foliage_placer_type` | `FoliagePlacerType<?>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:193` |
| `GAME_EVENT` | `minecraft:game_event` | `GameEvent` | built-in/static, pomme static ID table present | `game_event` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:194` |
| `HEIGHT_PROVIDER_TYPE` | `minecraft:height_provider_type` | `HeightProviderType<?>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:195` |
| `INPUT_CONTROL_TYPE` | `minecraft:input_control_type` | `MapCodec<? extends InputControl>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:196` |
| `INT_PROVIDER_TYPE` | `minecraft:int_provider_type` | `MapCodec<? extends IntProvider>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:197` |
| `ITEM` | `minecraft:item` | `Item` | built-in/static, pomme static ID table present | `item` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:198` |
| `SLOT_SOURCE_TYPE` | `minecraft:slot_source_type` | `MapCodec<? extends SlotSource>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:199` |
| `LOOT_CONDITION_TYPE` | `minecraft:loot_condition_type` | `MapCodec<? extends LootItemCondition>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:200` |
| `LOOT_FUNCTION_TYPE` | `minecraft:loot_function_type` | `MapCodec<? extends LootItemFunction>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:201` |
| `LOOT_NBT_PROVIDER_TYPE` | `minecraft:loot_nbt_provider_type` | `MapCodec<? extends NbtProvider>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:202` |
| `LOOT_NUMBER_PROVIDER_TYPE` | `minecraft:loot_number_provider_type` | `MapCodec<? extends NumberProvider>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:203` |
| `LOOT_POOL_ENTRY_TYPE` | `minecraft:loot_pool_entry_type` | `MapCodec<? extends LootPoolEntryContainer>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:204` |
| `LOOT_SCORE_PROVIDER_TYPE` | `minecraft:loot_score_provider_type` | `MapCodec<? extends ScoreboardNameProvider>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:205` |
| `MAP_DECORATION_TYPE` | `minecraft:map_decoration_type` | `MapDecorationType` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:206` |
| `MATERIAL_CONDITION` | `minecraft:worldgen/material_condition` | `MapCodec<? extends SurfaceRules.ConditionSource>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:207` |
| `MATERIAL_RULE` | `minecraft:worldgen/material_rule` | `MapCodec<? extends SurfaceRules.RuleSource>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:208` |
| `MEMORY_MODULE_TYPE` | `minecraft:memory_module_type` | `MemoryModuleType<?>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:209` |
| `MENU` | `minecraft:menu` | `MenuType<?>` | built-in/static, pomme static ID table present | `menu` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:210` |
| `MOB_EFFECT` | `minecraft:mob_effect` | `MobEffect` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:211` |
| `NUMBER_FORMAT_TYPE` | `minecraft:number_format_type` | `NumberFormatType<?>` | built-in/static, pomme static ID table present | `number_format_type` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:212` |
| `PARTICLE_TYPE` | `minecraft:particle_type` | `ParticleType<?>` | built-in/static, pomme static ID table present | `particle_type` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:213` |
| `PLACEMENT_MODIFIER_TYPE` | `minecraft:worldgen/placement_modifier_type` | `PlacementModifierType<?>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:214` |
| `POINT_OF_INTEREST_TYPE` | `minecraft:point_of_interest_type` | `PoiType` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:215` |
| `POOL_ALIAS_BINDING` | `minecraft:worldgen/pool_alias_binding` | `MapCodec<? extends PoolAliasBinding>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:216` |
| `POSITION_SOURCE_TYPE` | `minecraft:position_source_type` | `PositionSourceType<?>` | built-in/static, pomme static ID table present | `position_source_type` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:217` |
| `POS_RULE_TEST` | `minecraft:pos_rule_test` | `PosRuleTestType<?>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:218` |
| `POTION` | `minecraft:potion` | `Potion` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:219` |
| `RECIPE_BOOK_CATEGORY` | `minecraft:recipe_book_category` | `RecipeBookCategory` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:220` |
| `RECIPE_DISPLAY` | `minecraft:recipe_display` | `RecipeDisplay.Type<?>` | built-in/static, pomme static ID table present | `recipe_display` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:221` |
| `RECIPE_SERIALIZER` | `minecraft:recipe_serializer` | `RecipeSerializer<?>` | built-in/static, pomme static ID table present | `recipe_serializer` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:222` |
| `RECIPE_TYPE` | `minecraft:recipe_type` | `RecipeType<?>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:223` |
| `ROOT_PLACER_TYPE` | `minecraft:worldgen/root_placer_type` | `RootPlacerType<?>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:224` |
| `RULE_BLOCK_ENTITY_MODIFIER` | `minecraft:rule_block_entity_modifier` | `RuleBlockEntityModifierType<?>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:225` |
| `RULE_TEST` | `minecraft:rule_test` | `RuleTestType<?>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:226` |
| `SENSOR_TYPE` | `minecraft:sensor_type` | `SensorType<?>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:227` |
| `SLOT_DISPLAY` | `minecraft:slot_display` | `SlotDisplay.Type<?>` | built-in/static, pomme static ID table present | `slot_display` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:228` |
| `SOUND_EVENT` | `minecraft:sound_event` | `SoundEvent` | built-in/static, pomme static ID table present | `sound_event` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:229` |
| `SPAWN_CONDITION_TYPE` | `minecraft:spawn_condition_type` | `MapCodec<? extends SpawnCondition>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:230` |
| `STAT_TYPE` | `minecraft:stat_type` | `StatType<?>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:231` |
| `STRUCTURE_PIECE` | `minecraft:worldgen/structure_piece` | `StructurePieceType` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:232` |
| `STRUCTURE_PLACEMENT` | `minecraft:worldgen/structure_placement` | `StructurePlacementType<?>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:233` |
| `STRUCTURE_POOL_ELEMENT` | `minecraft:worldgen/structure_pool_element` | `StructurePoolElementType<?>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:234` |
| `STRUCTURE_PROCESSOR` | `minecraft:worldgen/structure_processor` | `MapCodec<? extends StructureProcessor>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:235` |
| `STRUCTURE_TYPE` | `minecraft:worldgen/structure_type` | `StructureType<?>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:236` |
| `DIALOG_ACTION_TYPE` | `minecraft:dialog_action_type` | `MapCodec<? extends Action>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:237` |
| `TEST_ENVIRONMENT_DEFINITION_TYPE` | `minecraft:test_environment_definition_type` | `MapCodec<? extends TestEnvironmentDefinition<?>>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:238` |
| `TEST_FUNCTION` | `minecraft:test_function` | `Consumer<GameTestHelper>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:239` |
| `TEST_INSTANCE_TYPE` | `minecraft:test_instance_type` | `MapCodec<? extends GameTestInstance>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:240` |
| `TICKET_TYPE` | `minecraft:ticket_type` | `TicketType` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:241` |
| `TREE_DECORATOR_TYPE` | `minecraft:worldgen/tree_decorator_type` | `TreeDecoratorType<?>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:242` |
| `TRUNK_PLACER_TYPE` | `minecraft:worldgen/trunk_placer_type` | `TrunkPlacerType<?>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:243` |
| `VILLAGER_PROFESSION` | `minecraft:villager_profession` | `VillagerProfession` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:244` |
| `VILLAGER_TYPE` | `minecraft:villager_type` | `VillagerType` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:245` |
| `INCOMING_RPC_METHOD` | `minecraft:incoming_rpc_methods` | `IncomingRpcMethod<?, ?>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:246` |
| `OUTGOING_RPC_METHOD` | `minecraft:outgoing_rpc_methods` | `OutgoingRpcMethod<?, ?>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:247` |
| `PERMISSION_TYPE` | `minecraft:permission_type` | `MapCodec<? extends Permission>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:248` |
| `PERMISSION_CHECK_TYPE` | `minecraft:permission_check_type` | `MapCodec<? extends PermissionCheck>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:249` |
| `BANNER_PATTERN` | `minecraft:banner_pattern` | `BannerPattern` | synced dynamic | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:250` |
| `BIOME` | `minecraft:worldgen/biome` | `Biome` | synced dynamic | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:251` |
| `CAT_SOUND_VARIANT` | `minecraft:cat_sound_variant` | `CatSoundVariant` | synced dynamic | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:252` |
| `CAT_VARIANT` | `minecraft:cat_variant` | `CatVariant` | synced dynamic | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:253` |
| `CHAT_TYPE` | `minecraft:chat_type` | `ChatType` | synced dynamic | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:254` |
| `CHICKEN_SOUND_VARIANT` | `minecraft:chicken_sound_variant` | `ChickenSoundVariant` | synced dynamic | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:255` |
| `CHICKEN_VARIANT` | `minecraft:chicken_variant` | `ChickenVariant` | synced dynamic | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:256` |
| `ZOMBIE_NAUTILUS_VARIANT` | `minecraft:zombie_nautilus_variant` | `ZombieNautilusVariant` | synced dynamic | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:257` |
| `CONFIGURED_CARVER` | `minecraft:worldgen/configured_carver` | `ConfiguredWorldCarver<?>` | other dynamic loader (not network-synchronized by this list) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:258` |
| `CONFIGURED_FEATURE` | `minecraft:worldgen/configured_feature` | `ConfiguredFeature<?, ?>` | other dynamic loader (not network-synchronized by this list) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:259` |
| `COW_SOUND_VARIANT` | `minecraft:cow_sound_variant` | `CowSoundVariant` | synced dynamic | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:260` |
| `COW_VARIANT` | `minecraft:cow_variant` | `CowVariant` | synced dynamic | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:261` |
| `DAMAGE_TYPE` | `minecraft:damage_type` | `DamageType` | synced dynamic | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:262` |
| `DENSITY_FUNCTION` | `minecraft:worldgen/density_function` | `DensityFunction` | other dynamic loader (not network-synchronized by this list) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:263` |
| `DIALOG` | `minecraft:dialog` | `Dialog` | synced dynamic | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:264` |
| `DIMENSION_TYPE` | `minecraft:dimension_type` | `DimensionType` | synced dynamic | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:265` |
| `ENCHANTMENT_PROVIDER` | `minecraft:enchantment_provider` | `EnchantmentProvider` | other dynamic loader (not network-synchronized by this list) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:266` |
| `ENCHANTMENT` | `minecraft:enchantment` | `Enchantment` | synced dynamic | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:267` |
| `FLAT_LEVEL_GENERATOR_PRESET` | `minecraft:worldgen/flat_level_generator_preset` | `FlatLevelGeneratorPreset` | other dynamic loader (not network-synchronized by this list) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:268` |
| `FROG_VARIANT` | `minecraft:frog_variant` | `FrogVariant` | synced dynamic | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:269` |
| `INSTRUMENT` | `minecraft:instrument` | `Instrument` | synced dynamic | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:270` |
| `JUKEBOX_SONG` | `minecraft:jukebox_song` | `JukeboxSong` | synced dynamic | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:271` |
| `MULTI_NOISE_BIOME_SOURCE_PARAMETER_LIST` | `minecraft:worldgen/multi_noise_biome_source_parameter_list` | `MultiNoiseBiomeSourceParameterList` | other dynamic loader (not network-synchronized by this list) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:272` |
| `NOISE_SETTINGS` | `minecraft:worldgen/noise_settings` | `NoiseGeneratorSettings` | other dynamic loader (not network-synchronized by this list) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:273` |
| `NOISE` | `minecraft:worldgen/noise` | `NormalNoise.NoiseParameters` | other dynamic loader (not network-synchronized by this list) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:274` |
| `PAINTING_VARIANT` | `minecraft:painting_variant` | `PaintingVariant` | synced dynamic | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:275` |
| `PIG_SOUND_VARIANT` | `minecraft:pig_sound_variant` | `PigSoundVariant` | synced dynamic | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:276` |
| `PIG_VARIANT` | `minecraft:pig_variant` | `PigVariant` | synced dynamic | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:277` |
| `PLACED_FEATURE` | `minecraft:worldgen/placed_feature` | `PlacedFeature` | other dynamic loader (not network-synchronized by this list) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:278` |
| `PROCESSOR_LIST` | `minecraft:worldgen/processor_list` | `StructureProcessorList` | other dynamic loader (not network-synchronized by this list) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:279` |
| `STRUCTURE_SET` | `minecraft:worldgen/structure_set` | `StructureSet` | other dynamic loader (not network-synchronized by this list) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:280` |
| `STRUCTURE` | `minecraft:worldgen/structure` | `Structure` | other dynamic loader (not network-synchronized by this list) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:281` |
| `SULFUR_CUBE_ARCHETYPE` | `minecraft:sulfur_cube_archetype` | `SulfurCubeArchetype` | synced dynamic | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:282` |
| `TEMPLATE_POOL` | `minecraft:worldgen/template_pool` | `StructureTemplatePool` | other dynamic loader (not network-synchronized by this list) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:283` |
| `TEST_ENVIRONMENT` | `minecraft:test_environment` | `TestEnvironmentDefinition<?>` | synced dynamic | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:284` |
| `TEST_INSTANCE` | `minecraft:test_instance` | `GameTestInstance` | synced dynamic | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:285` |
| `TIMELINE` | `minecraft:timeline` | `Timeline` | synced dynamic | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:286` |
| `TRADE_SET` | `minecraft:trade_set` | `TradeSet` | other dynamic loader (not network-synchronized by this list) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:287` |
| `TRIAL_SPAWNER_CONFIG` | `minecraft:trial_spawner` | `TrialSpawnerConfig` | other dynamic loader (not network-synchronized by this list) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:288` |
| `TRIGGER_TYPE` | `minecraft:trigger_type` | `CriterionTrigger<?>` | built-in/static, no pomme static ID table identified (unknown consumer coverage) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:289` |
| `TRIM_MATERIAL` | `minecraft:trim_material` | `TrimMaterial` | synced dynamic | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:290` |
| `TRIM_PATTERN` | `minecraft:trim_pattern` | `TrimPattern` | synced dynamic | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:291` |
| `VILLAGER_TRADE` | `minecraft:villager_trade` | `VillagerTrade` | other dynamic loader (not network-synchronized by this list) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:292` |
| `WOLF_VARIANT` | `minecraft:wolf_variant` | `WolfVariant` | synced dynamic | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:293` |
| `WOLF_SOUND_VARIANT` | `minecraft:wolf_sound_variant` | `WolfSoundVariant` | synced dynamic | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:294` |
| `WORLD_CLOCK` | `minecraft:world_clock` | `WorldClock` | synced dynamic | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:295` |
| `WORLD_PRESET` | `minecraft:worldgen/world_preset` | `WorldPreset` | other dynamic loader (not network-synchronized by this list) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:296` |
| `DIMENSION` | `minecraft:dimension` | `Level` | declaration only / runtime registration not established (unknown) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:297` |
| `LEVEL_STEM` | `minecraft:dimension` | `LevelStem` | other dynamic loader (not network-synchronized by this list) | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:298` |
| `LOOT_TABLE` | `minecraft:loot_table` | `LootTable` | manual/pseudo registry | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:299` |
| `ITEM_MODIFIER` | `minecraft:item_modifier` | `LootItemFunction` | manual/pseudo registry | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:300` |
| `PREDICATE` | `minecraft:predicate` | `LootItemCondition` | manual/pseudo registry | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:301` |
| `ADVANCEMENT` | `minecraft:advancement` | `Advancement` | manual/pseudo registry | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:302` |
| `RECIPE` | `minecraft:recipe` | `Recipe<?>` | manual/pseudo registry | `—` | `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java:303` |

Official classification sources: `minecraft-26.2-decompiled/src/net/minecraft/resources/RegistryDataLoader.java:102-103` (sync set), `minecraft-26.2-decompiled/src/net/minecraft/core/RegistrySynchronization.java:32-68`, `minecraft-26.2-decompiled/src/net/minecraft/core/registries/BuiltInRegistries.java:188-285`, `minecraft-26.2-decompiled/src/net/minecraft/data/info/DatapackStructureReport.java:46,75-78`; client receipt `minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/RegistryDataCollector.java:63-167`. Entry data, tags and full consumer parity remain unverified.

## Appendix D: official local source snapshot SHA-256 manifest

The supplied decompiled tree has no verifiable Git revision; no revision is asserted. This finite source inventory is a reproducible starting point for rechecking protocol, registry, framing, bundler and connection evidence. Each digest hashes exact local file bytes. This is not a whole-tree hash. If any digest differs, revalidate cited source evidence against that snapshot.

| root-relative source file | SHA-256 |
|---|---|
| `minecraft-26.2-decompiled/src/net/minecraft/network/protocol/handshake/HandshakeProtocols.java` | `cb7e338da49c8a19330fa53c204882a17c2e912c3dbaeb61fd3ec78840ba0ca5` |
| `minecraft-26.2-decompiled/src/net/minecraft/network/protocol/status/StatusProtocols.java` | `b1ba3b754f7e5dda8033bbcd89becf03b28321d35956cc0be3b4f0a557325762` |
| `minecraft-26.2-decompiled/src/net/minecraft/network/protocol/login/LoginProtocols.java` | `0cfbbc6c7097db8b7c07eb280c33c3bbd47b720fd4d613b4e3215bd6a680ec63` |
| `minecraft-26.2-decompiled/src/net/minecraft/network/protocol/configuration/ConfigurationProtocols.java` | `2bae298f3c90ce1e3f3ce0a892a9898fc8e03a14567627dfa19d3fc52630b85b` |
| `minecraft-26.2-decompiled/src/net/minecraft/network/protocol/game/GameProtocols.java` | `29ee613e41092c870c659880f37d11bdeed0e447518daf010d873e0660bc0b46` |
| `minecraft-26.2-decompiled/src/net/minecraft/core/registries/Registries.java` | `369e497a07962a209e1855840284b4a7c4033901665e0e832cd5ea3b536bee6b` |
| `minecraft-26.2-decompiled/src/net/minecraft/resources/RegistryDataLoader.java` | `171733b22949bf0247dba3b97f1dd3c5f9e8909bc96d91d6de30283f671acfd2` |
| `minecraft-26.2-decompiled/src/net/minecraft/core/registries/BuiltInRegistries.java` | `10b114377788644b0a4df1852b69f1b312a907585307b3f831ba26199597cdf8` |
| `minecraft-26.2-decompiled/src/net/minecraft/core/RegistrySynchronization.java` | `0c475e31be46950180e60689392ee434d69591e3236ef93dfc2187d26f8e8f78` |
| `minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/RegistryDataCollector.java` | `15047e11cc9f05b38298220346836994085d3063ed8eeac0adb774bbb2352ab5` |
| `minecraft-26.2-decompiled/src/net/minecraft/network/protocol/common/ClientboundCustomPayloadPacket.java` | `75e57d14df35eebd0f12b8f852309c0c426796452d846288324a7a07d49d2f2c` |
| `minecraft-26.2-decompiled/src/net/minecraft/network/protocol/common/ServerboundCustomPayloadPacket.java` | `703549f92989b40db5ca12a8c1a9d06895200ed867662a98120092cd22e11da7` |
| `minecraft-26.2-decompiled/src/net/minecraft/network/protocol/common/custom/BrandPayload.java` | `8f1628513f154c79864ba3628d04569a89b9515338b4f0304f311ff34b1fe3e7` |
| `minecraft-26.2-decompiled/src/net/minecraft/network/protocol/common/custom/DiscardedPayload.java` | `7ccccfe627a17c2bb2a7f1e78af33cbcc879a71a35b57d43c38f1fa7be45aabe` |
| `minecraft-26.2-decompiled/src/net/minecraft/network/Connection.java` | `03582a1c53523c7744ad26eda1617d63fa44716b289a8059a70e7377e99be071` |
| `minecraft-26.2-decompiled/src/net/minecraft/network/Varint21FrameDecoder.java` | `0dd51e532e673f4c51159cbaecac511438df35a7819ea93b55b82808b68e8f38` |
| `minecraft-26.2-decompiled/src/net/minecraft/network/Varint21LengthFieldPrepender.java` | `9d6123dbca5e3ed9c3a88aae765c43800e8ac3dee6b74756ce6e4353f24ccea8` |
| `minecraft-26.2-decompiled/src/net/minecraft/network/CompressionDecoder.java` | `d188839eda5f889ce93cb1afb4c177d96ef98c5fc77882f0ca946509747cb17f` |
| `minecraft-26.2-decompiled/src/net/minecraft/network/protocol/BundlerInfo.java` | `accedc2790ee88367f7ff21edad472396d54c2aeeb0b4b6ea31f8fb7febf56bb` |
| `minecraft-26.2-decompiled/src/net/minecraft/network/protocol/handshake/ClientIntentionPacket.java` | `32ad322f62b3723845af1c5fc7199fd78d749828d04af8f61ad544c9079b3c0e` |
| `minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ServerStatusPinger.java` | `9430d592b812e049adccd65ae6fd602a63ffd3fd0095a6de1b0dabf9d9c0e179` |
| `minecraft-26.2-decompiled/src/net/minecraft/client/gui/screens/ConnectScreen.java` | `6abcaae548dcf1bb88b23c7b8df915c94ae6f8a9e92c12351588a5c91e3ef32a` |
| `minecraft-26.2-decompiled/src/net/minecraft/server/network/ServerHandshakePacketListenerImpl.java` | `1feec339181b8e5964a989fa3a217c88c3887d9ef34d542dc207753df5087154` |
