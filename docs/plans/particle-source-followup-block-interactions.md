# Particle source follow-up: block interactions

Java 26.2 source audit and Rust implementation for the two confirmed local-input particle sources.

## Candle extinguish

References: `minecraft-26.2-decompiled/src/net/minecraft/world/level/block/AbstractCandleBlock.java:86`, `CandleBlock.java:76-79`, `CandleCakeBlock.java:86-91`.

- Connected to the normal block-use branch in `pomme-client/src/player/interaction.rs::InteractionState::start_use_item`, after the actual block hit and packet submission.
- Requires a lit vanilla candle/candle-cake, empty interacting stack, `LocalPlayer.may_build`; candle-cake additionally requires `hit.y - block.y > 0.5`.
- Reuses Java's exact candle-count offsets (1–4) and candle-cake center-top offset. Emits one `SMOKE` request per offset with velocity `(0, 0.1, 0)`.
- Predicts `lit=false` via the existing sequence/known-state reconciliation and visual-edit path before emitting. Repeated use while awaiting the server update cannot duplicate the extinguish burst. No inventory changes or local sound added.
- Block-use particle generation is separate from periodic `animateTick` particles.

## Dragon egg teleport

Reference: `minecraft-26.2-decompiled/src/net/minecraft/world/level/block/DragonEggBlock.java::useWithoutItem/attack/teleport`.

- Connected at the actual main-hand default block-interaction branch and initial successful block attack branch; no server block move or local AI/state teleport is performed. `DragonEggBlock.useWithoutItem`'s name is not used as evidence of an empty-hand-only rule.
- Java `BlockBehaviour.useItemOn` defaults to `TRY_WITH_EMPTY_HAND` regardless of the stack; `MultiPlayerGameMode.performUseItemOn` calls `useWithoutItem` for that result only with `MAIN_HAND`, unless secondary-use suppresses block use because either hand holds an item. Therefore a nonempty main-hand stack may still reach DragonEggBlock's override, and this Rust path intentionally does not require an empty stack. It also respects `hand == MainHand` and the existing `suppress_block_use` sneak gate. The empty-hand requirement that applies to Candle/CandleCake comes from their explicit `useItemOn` condition and remains separate.
- Tries up to 1,000 Java-shaped offset candidates (`nextInt(16)-nextInt(16)`, `nextInt(8)-nextInt(8)`, `nextInt(16)-nextInt(16)`). Candidate must be air, have a non-air block below, be inside loaded world/build height, and pass the existing world-border bounds check. No candidate means no particles.
- Emits exactly 128 `PORTAL` requests at Java's random interpolation/offset positions, with per-axis `(nextFloat()-0.5)*0.2` velocities. Candidate and particle random draws use a seedable `fastrand::Rng` for deterministic tests.
- Requests are submitted to the existing `ParticleStore`; no new LevelParticles packet or server event path was introduced.

## Validation

Tests in `pomme-client/src/player/interaction.rs` cover actual candle block-use prediction/repeat suppression and ParticleStore output, candle counts 1–4, candle-cake lower/upper hit, explicit empty-hand/build/lit gates, Dragon Egg use with both a nonempty and empty main hand plus attack, candidate absence, border/build-height rejection, 128 count and velocity bounds. The Rust test setup represents Java's block-use `TRY_WITH_EMPTY_HAND` fallthrough; sneaking-with-held-item is independently gated by `suppress_block_use`.

Latest verification: the restored production sampler is present (`blocks.rs` 1,990 lines; final hash `83ff1b53…038f13c1`). `mise run check` exits 0, and focused interaction tests pass: candle block-use 2/2, Dragon Egg use/attack 1/1, Redstone Ore 7/7, water bucket 3/3 (including generated Java-native replaceability plus the full `BucketItem.use`/`emptyContents` output path), continued mining 1/1. Workspace tests were rerun on the same source snapshot; final counts are in `particle-integration-verification.md`.
