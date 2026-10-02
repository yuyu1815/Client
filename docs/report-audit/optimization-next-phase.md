# Post-implementation optimization phase (26.2)

## Gate — do not start early

This is a deferred phase, not authorization to change implementation code now. Start only after **all** current implementation agents and their dependent agents have finished, the independent final review is complete and its serious findings are fixed, and the final combined build/test validation for the implementation phase succeeds. Record the completed gate and exact commands/results before proceeding. If any gate fails, stop and return to the implementation/review phase.

Once the gate is met, use Luna/background agents for the comparison and later optimization work. Until then, do not launch optimization agents, edit implementation files, or run builds/tests/benchmarks for this phase. This plan is the only change in this task.

## Scope and comparison standard

1. Derive scope from the **actual final source diff of this implementation effort** (not every file merely mentioned in the report). Record file, symbol, and call path; do not optimize unrelated existing code.
2. Compare each in-scope model, renderer, physics, menu, and packet behavior to the corresponding **official Minecraft 26.2** implementation. Prefer the vanilla 26.2 decompiled/reference source when it contains that client code; otherwise use the official mapped client jar with `javap -p -c -s` and inspect the relevant method/fields. The official server decompilation is authoritative only for server-side behavior it contains. Cite class/method and source/jar path for every comparison.
3. Available local official evidence:
   - Official client jar: `C:/Users/yuzum/AppData/Roaming/.minecraft/versions/26.2/26.2.jar`; a second local copy is `C:/Users/yuzum/AppData/Roaming/.pomme/data/versions/26.2/26.2.jar`. Verify the file exists and identify its mapping/class names before using it; these are local installation artifacts, not tracked inputs.
   - Official server bundle/source: `C:/Users/yuzum/Desktop/mine_rust/minecraft-debug-server/server.jar`, nested `versions/26.2/server-26.2.jar`, and decompiled tree `C:/Users/yuzum/Desktop/mine_rust/minecraft-26.2-decompiled/src/net/minecraft/`. Its `README.md` records official metadata SHA-1 `33c420747ce582e48dff1d8c5d8e67e5bb6257c9`, outer server SHA-1 `823e2250d24b3ddac457a60c92a6a941943fcd6a`, inner jar SHA-256 `183c0499c5f855570ee487dd38e141a53f0121f83a0b07a3bac2d8b6698823e8`, and CFR 0.152 provenance.
   - `C:/Users/yuzum/Desktop/mine_rust/Client/third_party/SteelMC/` may help locate likely behavior or terminology, but is a port, **not official implementation evidence**. Never represent it as vanilla or use it to claim exact equivalence.
4. Preserve official behavior, ordering, edge cases, and visual/network semantics. Do not trade correctness for speed. Do not invent performance gains or rankings before profiling.

## Investigation and ranking

First capture an unchanged baseline on the post-implementation tree, then inspect only symbols in the implementation diff. Create a compact finding per candidate: Pomme path/symbol and hot call path; vanilla class/method; observed cost and scenario; suspected cause; proposed minimal change; behavioral risks; validation and measurement method. Rank only candidates supported by a reproducible profile, benchmark, allocation/count evidence, or a concrete asymptotic/repeated-work observation corroborated by the call path. Mark unmeasured speculation as such and do not optimize it by default.

Prioritize checking (not presuming defects in): shape lookup/state-table load and allocations; model mesh bake/cache and texture reload; translucent sorting/draw calls; menu construction scans and pattern references; particle creation; swept-block physics effects; tick metadata lookup. Include other measured hotspots in scope. Consider cold-start and warm steady-state separately; capture both memory/allocation and speed where relevant, with identical inputs/settings/build profile. Repeated runs and variance matter. Reuse existing benchmark/profiling facilities; no new dependency, framework, or speculative pool/cache abstraction without measurements.

## Execution and validation

- Parallelize only independent symbols/files with explicit ownership boundaries. Luna agents should report official evidence, before/after measurements, tests/checks, and exact changed files. Avoid parallel edits to shared files.
- Establish and record a reproducible baseline before optimization. The repository's benchmark data and timing code live in `Client/pomme-client/src/benchmark.rs` and renderer timing points are in `Client/pomme-client/src/renderer/mod.rs`; inspect their current invocation/semantics before choosing a scenario. `Client/justfile` documents the opt-in `just auto-benchmark <server>` flow (and explicitly requires a separately built client); `Client/pomme-launcher/src-tauri/src/auto_benchmark.rs` contains its orchestration. Use normal documented release/performance settings, same machine, scene/server, resolution, render distance, warm-up, duration, and cache state for before/after. `dev-fast` is not a runtime FPS baseline. If a required build is necessary, it belongs after the implementation gate, not during this planning/investigation task.
- While optimization agents are working, run `mise run check` only (and static inspection as needed); do not build/test mid-flight. After all changes and independent review complete, run the combined build and test validation once, then the same benchmark scenarios against the baseline. Report regressions and uncertainty rather than suppressing them. Follow `Client/AGENTS.md` and use mise for Rust commands.
- Keep optimizations minimal and attributable. Prefer standard library/native facilities; no speculative pooling/framework/dependency. Preserve correct semantics. Commit after validation, grouped by reasonably coherent file sets; record commit hashes and checks.

## Current status

No comparison, profiling, optimization, build, test, or commit was performed for this deferred phase. No performance figures or improvement percentages are established by this plan.
