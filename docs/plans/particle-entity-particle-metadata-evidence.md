# Java 26.2 particle-bearing entity metadata integration evidence

This note records the production receive → state → tick request → particle consumer path. Java reference classes: `world/entity/AreaEffectCloud.java` and `world/entity/LivingEntity.java`. Source-callsite inventory ownership remains with the separate auditor; this does not update its CSV.

## Wire decoding and identity

- `net/handler.rs` recognizes entity metadata `Particle` and `Particles` values and serializes the typed Azalea particle value to the native 26.2 option representation via `particle_options_from_typed`.
- That helper uses the same existing LevelParticles option decoder, rather than a second codec. Native particle/item/component translations remain at the existing translation boundary and are not repeated in Core.
- Metadata events carry typed `(ServerParticleKind, ServerParticleOptions)` pairs into Core. `Unknown`/malformed option data is not interpreted as `Simple`; the metadata item decoder owns its bounded payload, so failed particle-option decoding does not consume the next serializer's bytes.
- `NetworkEvent::ParticleMetadata` is consumed in `app/core.rs::apply_particle_metadata`. AreaEffectCloud gets its arbitrary kind/options via `EntityStore::misc_particle_cloud_options`; living entities and local player retain the exact list, including explicit empty list vs not-yet-received state. It does not collapse options to a generic Simple particle or white EntityEffect.

## State, request, and consumer

- AreaEffectCloud radius/waiting continue through the Java-indexed scalar metadata path; particle kind/options are kept separately in `MiscParticleState`. The periodic cloud source reads the stored `cloud_particle` and `cloud_options` and returns real `ParticleSpawnRequest`s from `particle_misc::tick`.
- Remote `LivingEntity.effect_particle_options` and local player's effect option state feed the living-effect particle request path in `app/phases/in_game.rs`; `ParticleStore::add_living_effect_server_particles` routes each selected typed option through the native provider dispatch and then the ordinary quad/model consumer.
- State is cleared on entity removal/world reset through existing lifecycle owners. An explicit empty `ParticleList` replaces the old list; it does not resurrect a previous particle.

## Automated checks

Executed serially from `Client/`:

- `mise exec -- cargo test -p pomme-client --locked --profile dev-fast typed_metadata_particles_use_the_level_particle_option_codec -- --nocapture`: exit **0**, **1 passed**. Includes typed Effect metadata reaching Core/store through the production helper with the native colour/power options.
- `mise exec -- cargo test -p pomme-client --locked --profile dev-fast area_cloud_radius_and_waiting_metadata_drive_particle_phase -- --nocapture`: exit **0**, **1 passed**. Checks synced phase inputs and a typed Dust option in the request path.
- `mise exec -- cargo check -p pomme-client --tests --locked --profile dev-fast`: exit **0** on the integration snapshot.

The focused checks are not GUI visual parity. Unknown serializer/cursor and option cases are covered by the handler's typed metadata fixtures; final broad workspace result is recorded in `particle-integration-verification.md`.
