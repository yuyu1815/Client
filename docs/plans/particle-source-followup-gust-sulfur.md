# Particle source follow-up: Gust and Potent Sulfur (Java 26.2)

## Potent Sulfur animateTick

Java reference: `minecraft-26.2-decompiled/src/net/minecraft/world/level/block/PotentSulfurBlock.java`, `animateTick` (lines 115 onward). The block-state property is `STATE`, bound to `BlockStateProperties.POTENT_SULFUR_STATE`; its serialized key is `potent_sulfur_state`. Java returns for `DRY`, and otherwise emits two `SULFUR_BUBBLES` only if the fluid immediately above is a source of water. The native 26.2 state table represents this using actual `BlockState`s and the same property name.

Rust `world/particle_tick/blocks.rs` now checks `potent_sulfur_state != dry` and requires water above with `amount == 8 && !falling`. This is specifically the sampled block `animateTick` source. It remains separate from `world/block_entity_particle.rs`'s Potent Sulfur block-entity eruption timer, geyser plume/base/poof, and gas sources; none of those paths were changed.

The focused test `world::particle_tick::blocks::tests::potent_sulfur_animate_tick_uses_real_state_property_and_source_water_only` initializes the 26.2 native state table and uses real states selected with `block::find_state`: dry, wet, dormant, erupting, continuous; source water (`level=0`), flowing water (`level=1`), and air. It invokes the real block sampler and checks the output count/type/vertical placement/zero velocity.

## Gust provider contract

Java references:
- `client/particle/GustSeedParticle.java`: the seed uses its constructor's scale/lifetime/delay, emits three children at offsets `(random.nextDouble() - random.nextDouble()) * scale` on the configured cadence, and passes `(float)age / lifetime` as the first auxiliary value when it calls `addParticle(GUST, ...)`.
- `client/particle/GustParticle.java`: both `Provider.createParticle` and `SmallProvider.createParticle` ignore all three auxiliary values. The constructor establishes the child lifetime as `12 + random.nextInt(4)`, sets quad size 1; SmallProvider subsequently scales the particle by 0.15. Its `tick` advances age/removes/changes sprite only; it never applies movement from auxiliary velocity.
- `client/particle/ParticleResources.java`: large seed provider is `(scale=3,lifetime=7,delay=0)`, small is `(scale=1,lifetime=3,delay=2)`.

Rust `particle/emitters.rs` therefore now discards the seed's age/lifetime auxiliary value instead of assigning it to Y velocity. Generated children have zero velocity, lifetime 12..=15, preserved random position bounds, opaque layer and sprite-frame animation. Small Gust children preserve the Java 0.15 size. Direct `GUST`/`SMALL_GUST` packets are routed through `particle/magic.rs`; that existing provider path already discards packet velocity and stores `DVec3::ZERO`, matching Java's ignored auxiliary values. It was not changed.

The focused emitter test ticks the actual seed state and checks child totals (large: 21 across seven ticks; small: 3 across three ticks), kind, zero velocity, Java size/lifetime, position bounds, and the fixture's sprite UV fallback. Direct GUST's existing zero-velocity provider behavior is verified by code inspection, not a new isolated packet test.

## Verification

Commands ran serially from `Client` with the pinned mise toolchain:

- `mise exec -- cargo test -p pomme-client --locked --profile dev-fast gust_seed_providers_emit_three_gusts_with_java_scale_and_timing -- --test-threads=1` — exit **0**, **1 passed**.
- `mise exec -- cargo test -p pomme-client --locked --profile dev-fast world::particle_tick::blocks::tests -- --test-threads=1` — exit **0**, **6 passed** (includes real-state sulfur and redstone sampler tests).
- `mise exec -- cargo test -p pomme-client --locked --profile dev-fast explosion_packet_options_use_shared_typed_particle_spawn_boundary -- --test-threads=1` — exit **0**, **1 passed**; direct GUST and weighted GUST packet options reach their ordinary typed provider boundary.
- Final `mise run test` passes the latest workspace snapshot (counts in `particle-integration-verification.md`).

Block-entity/geyser/gas behavior is a separate source owner; this note does not reclassify those sources.
