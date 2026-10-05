# Bubble consumer fixture stability investigation

## Final resolution (latest snapshot)

The original intermittent consumer-fixture failure is fixed in the test input, not production behavior. The count-1 `LevelParticles` route samples packet Gaussian velocity even with zero positional deviation. With `max_speed=1`, seed 1 yielded velocity `(0.0805222, -0.1627827, 0.5254976)` and the one-tick endpoint `(0.5805222, 64.3392173, 1.0254976)`, leaving the sole source-water cell `(0,64,0)` for air `(0,64,1)`. Bubble correctly dies outside water; it was age 1 of lifetime 10. A controlled run with only that endpoint cell set to water passed, confirming the cause.

The real packet fixture now keeps **count=1**, still-source water and stone support assertions, and `dist=DVec3::ZERO`, while passing **`max_speed=0` only for Bubble**. This zeros the packet-generated Gaussian velocity but preserves Bubble provider jitter and production movement/filtering. The fixture explicitly asserts that the Bubble packet route created a pending Bubble, that the Bubble survives its actual `store.tick`, and retains the common quad-extraction → `build_particle_vertices` assertions. Failure messages include packet max speed, source state, and spawn position. The temporary environment-variable seed/destination controls and hand-projected tick diagnostics were removed. Production provider/physics conditions and assertions were not relaxed; the separate `water::tests::bubbles_exit_when_not_in_water_and_hanging_drips_transition` check still asserts that a Bubble outside water is removed.

Revalidation on this snapshot: exact 125-kind consumer fixture **10/10 passed** (1 test/run, each 0.07s); `particle::tests` **82/82 passed** with `--test-threads=1` and **82/82 passed** at normal test parallelism; reload and item world-context tests each **1/1 passed**; Brewing Stand/water-bucket input regression **1/1 passed**; `mise run check` exit 0; whole workspace `mise run test` **1,769 client + 52 protocol + 1 singleplayer passed**, 1 singleplayer test ignored, on **two consecutive runs**, both exit 0. Cargo tasks were run one at a time. Hash manifest for 9,894 Rust/JSON/TOML/YAML/lock/shader/CSV/validation sources and configs: `tmp/particle-final-source-hashes.txt` (ignored local manifest), SHA-256 `981ea62ab1d9da88f34a236e641b317c7462a097d7f4a0c14c91e979140837d5`.

## Result

The intermittent Bubble fixture failure is explained by the fixture's **one-cell water volume combined with a nonzero-count particle packet's randomized Gaussian velocity**. It is not evidence of a lifetime, registry, mode, or provider-rendering failure.

In `particle.rs::tests::every_native_particle_kind_spawns_from_a_valid_typed_packet_fixture`, the loop calls `add_particles_from_packet(..., dist = DVec3::ZERO, max_speed = 1.0, count = 1, ...)` for Bubble. In `ParticleStore::add_particles_from_packet`, `count > 0` samples three Gaussian velocity components even when `dist` is zero. Bubble then adds its small provider jitter. Under the reproducible seed below, the Z velocity is `0.5254975501`, enough to leave the sole water cell in one tick.

The fixture starts Bubble at `(0.5, 64.5, 0.5)`, in source water `(0, 64, 0)` with stone support `(0, 63, 0)`. After the exact collision resolver used by `water::move_particle`, the projected one-tick position is `(0.5805222299, 64.3392172766, 1.0254975501)`, whose floor cell is `(0, 64, 1)`. That cell is air / `FluidKind::Empty`. `water::tick` moves the particle, applies friction, then checks `fluid_at(chunks, p.pos)` and returns `false` outside water; `ParticleStore::tick` removes it before quad extraction. The lifetime is 10 and the projected age is only 1, so lifetime expiry is ruled out.

A controlled run that added **only the exact destination cell** `(0, 64, 1)` as water passed the unchanged tick → quad → CPU vertex assertions. This is a causal control, not a recommendation to broadly flood the fixture. It confirms the destination-fluid check is what removes the particle.

## Diagnostic additions

The intermediate diagnostic-only experiment in `pomme-client/src/particle.rs` had test-only instrumentation in the failing fixture (removed in the final resolution above):

- asserts that the Bubble source block is the expected still-water state (fluid Water, amount 8, not falling) and support is stone;
- on failure prints seed, typed options, store mode, active particle family/kind, age/lifetime, previous/current position, floored block state and fluid, velocity, projected exact-collision tick state, source water/support, and the post-tick active list;
- optional experiment controls: `POMME_BUBBLE_TEST_SEED=<u64>` and `POMME_BUBBLE_TEST_DESTINATION_WATER=1` (the latter adds only the measured adjacent endpoint cell).

The original consumer assertions are unchanged: the fixture still calls `store.tick`, `store.extract`, and `build_particle_vertices`, and still requires nonempty quads and six CPU vertices per quad. No provider or production behavior was changed.

## Experiments and results

All commands ran from `Client` via mise. Cargo invocations were sequential; no two Cargo processes ran concurrently.

| Experiment | Result |
|---|---|
| Exact fixture, seed 1, source water only | **Failed reproducibly**; 1/1 on final snapshot. Earlier same-seed repeats on the diagnostic-only variant failed 3/3. Failure included the diagnostic state below. |
| Exact fixture, fixed seeds 2, 3, 4, 5, 17, 99, source water only | **6/6 passed** on final diagnostic snapshot. |
| Exact fixture, seed 1, plus only water at `(0,64,1)` | **1/1 passed**; same binary/source snapshot as final seed-1 failure. |
| Unseeded exact fixture repeated as separate single-test processes | **10/10 passed**. This does not erase the seeded reproduction. |
| `cargo test -p pomme-client --locked particle::tests -- --test-threads=1` | **82 passed, 0 failed**, exit 0. Bubble fixture passed. |
| `cargo test -p pomme-client --locked particle::tests` (normal test parallelism) | **82 passed, 0 failed**, exit 0. Bubble fixture passed. |
| `mise run test` (normal workspace parallelism) | client **1,768 passed / 0 failed**; protocol **52 / 0**; singleplayer **1 passed / 1 ignored**; doc tests 0; exit 0. Bubble fixture passed. |

Representative seed-1 failure (full exact output is in `/tmp/bubble-final-seed-1.log`):

```text
seed=Some(1), destination_water=false, options=Some("Simple"), mode=All
before_tick=[("Water(Bubble)", age=0, lifetime=10,
 pos=(0.5,64.5,0.5), floor=(0,64,0), block=86,
 fluid=(Water,8,false), vel=(0.0805222299,-0.1627827234,0.5254975501))]
projected_tick=Some((age=1, lifetime=10,
 prev_pos=(0.5,64.5,0.5), pos=(0.5805222299,64.3392172766,1.0254975501),
 floor=(0,64,1), block=0, fluid=(Empty,0,false),
 post-friction velocity=(0.0684438954,-0.1366653149,0.4466729176)))
after_tick=[]
```

The fixture obtains native 26.2 state data under `test_protocol_guard()` and reinitializes `26.2`. Its local world reads back water block-state ID **86**, stone ID **1**, and still-source fluid at the Bubble origin. Each loop iteration clears a local `ParticleStore` and explicitly sets `ParticleMode::All`. The registry/mode/world setup does not show cross-test contamination in these runs. The random packet velocity, not shared world state, accounts for the observed deterministic fail-versus-pass seed outcomes.

## Snapshot / executable identity

For the final diagnostic snapshot used for the seed matrix, serial/parallel particle tests, and final workspace test:

- Final SHA-256 manifest: `/tmp/bubble-final-source-hashes.txt`; **10,654** repository files matching Rust source, JSON, TOML, YAML, lock, or config extensions (excluding `target` and `.git`). Manifest SHA-256: `9054baf6188fc036ce87b157ea7bf0293dbee3397888ec1103fb41a2f5438f7a`.
- The before/after experiment manifests compared byte-for-byte equal (diff exit 0), so the final seed/control, serial/parallel particle, and workspace test runs used one unchanged source/config/JSON snapshot.
- `pomme-client/src/particle.rs`: `d1ba45309be44345617774da323a649317e22ffa94f34b990222bb6b5e4b0fff`.
- Executed test binary: `target/debug/build/pomme-client/eec80b0194df0695/out/pomme_client-eec80b0194df0695.exe`, SHA-256 `7978f769b27d6ec0c3ede296e568480b403a6b0d5c9041141e489a0488b65ea9`.

The initial source hash inventory taken before diagnostic edits differed only at `pomme-client/src/particle.rs`; all other hashed Rust/JSON/config files stayed unchanged. The optional endpoint-water control and diagnostics are test-only fixture changes. No production code, provider condition, or assertion was altered.

## Historical investigation note (pre-fix snapshot)

The statements in this section describe the **earlier investigation snapshot, before the fixture correction**, not the current state. At that time the fixture used a stochastic count-1 packet sample (`max_speed=1`) in one-cell water; the seed-1 failure and reproduction evidence above document that historical behavior. The then-proposed alternative was `count = 0`, zero `dist`. This section's “not applied” wording is likewise historical: **the correction has since been applied to the test fixture**.

## Current fixture and final gate

The real packet fixture now retains **count=1** and `dist = DVec3::ZERO`, and passes **`max_speed=0` only for Bubble**. This removes packet-sampled Gaussian velocity while retaining Bubble provider jitter. It verifies the real packet → `store.tick` → quad extraction → `build_particle_vertices` path. No production behavior was changed. The independent final gate passed: client **1,769**, protocol **52**, singleplayer **1 passed / 1 ignored**; `mise run check` exit **0**. These current results agree with the latest-snapshot resolution at the top of this document. The earlier seed-1 failure, exact diagnostic state, and controlled endpoint-water reproduction above are retained as investigation evidence.
