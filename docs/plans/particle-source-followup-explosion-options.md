# Explosion packet typed-particle follow-up

## Java 26.2 evidence

`minecraft-26.2-decompiled/src/net/minecraft/client/multiplayer/ClientPacketListener.java::handleExplosion` plays the packet sound, calls `ClientLevel.addParticle(packet.explosionParticle(), center, 1.0, 0.0, 0.0)`, tracks `(center, radius, blockCount, blockParticles)`, then applies optional player knockback. `ClientExplosionTracker` selects one explosion by `blockCount`, rejects nonpositive radius / non-air candidate positions, chooses `ExplosionParticleInfo` from the weighted list, and preserves its `scaling` and `speed` in particle position and velocity. Empty lists are not tracked. No particle ID/state/item remap is performed in this callback.

## Rust change

`Core::NetworkEvent::Explosion` keeps sound and knockback behavior and calls `ParticleStore::queue_explosion_packet_particles`. That production consumer routes the primary typed option through the metadata codec helper and ordinary packet-spawn boundary, then retains the original weighted list in the existing explosion tracker. `ParticleStore::spawn_tracked_explosion_particles` resolves weighted positions against current chunks and dispatches each selected typed option through the same provider boundary with current camera, block registry, chunks, and biome climate. This removes the five-kind explosion whitelist from packet dispatch; internal emitter-generated explosion children keep their dedicated provider path.

## Verification

Focused validation commands (run from `Client`, serially):

- `mise exec -- cargo test -p pomme-client explosion_packet_options_use_shared_typed_particle_spawn_boundary --locked`
- `mise exec -- cargo test -p pomme-client explosion_ingress_preserves_the_complete_native_payload --locked`

The particle test covers typed GUST/GUST_EMITTER, Dust, Effect, zero/empty weights, and existing Explosion/ExplosionEmitter/EndRod/Poof/Smoke options. The handler test sends an Explosion packet through the production handler, checks preserved primary/weighted payload, and passes the resulting event through the production `ParticleStore` consumer. Validation is currently blocked before these tests compile by unrelated missing helpers/imports in `world/particle_tick/blocks.rs`, which is being recovered separately. No GUI parity is claimed.
