# ItemPickupParticle non-ItemEntity support (Java 26.2)

This is a client-internal render animation, **not** another entry in the 125-kind particle registry or wire particle codec. It is tracked separately in [`particle-internal-render-events.csv`](particle-internal-render-events.csv).

## Java scope confirmed

- `minecraft-26.2-decompiled/src/net/minecraft/world/entity/LivingEntity.java::take` sends `ClientboundTakeItemEntityPacket` only when the entity is not removed, the level is server-side, and the source is `ItemEntity`, `AbstractArrow`, or `ExperienceOrb` (lines 3101–3104).
- `AbstractArrow` concrete subclasses in the Java source search are `Arrow`, `SpectralArrow`, and `ThrownTrident`; `ThrownTrident extends AbstractArrow`. No additional Java source type is being added by this change.
- `ClientPacketListener.handleTakeItemEntity` extracts a source `EntityRenderState` for every packet source. It removes `ItemEntity` once its stack is empty and removes non-orb, non-item entities; `ExperienceOrb` remains server-removal-owned. `ItemPickupParticleGroup` submits the retained state through the standard `EntityRenderDispatcher`, with 3-tick squared interpolation.
- ItemEntity remains on its existing stack-before-shrink snapshot and stack-decrement behavior. No world entity is re-registered for any pickup animation.

## Implementation and ownership

- `entity::EntityStore::pickup_entity_view` copies only the render inputs needed for supported non-item source kinds to a `PickupEntityView` enum. Arrow views preserve normal/spectral/tipped variant and tint; trident views preserve rotation, age, and foil; orb views preserve XP value, age, and source block/sky light.
- `ItemEntityStore` owns these inert source snapshots alongside existing `ItemEntity` pickup animations. They survive source entity removal; their normal 3-tick lifetime and collector interpolation remain shared.
- `app/core.rs::NetworkEvent::ItemPickedUp` captures the view before removal, queues the snapshot, removes AbstractArrow-derived entities locally, and leaves XP orb removal to the server. It retains Java's separate ExperienceOrb pickup sound.
- `app/phases/in_game.rs` converts snapshots into normal `EntityRenderInfo` inputs and appends them to the existing entity renderer list. The flight position changes while model-specific fields stay frozen. This uses existing orb overlay/light behavior, arrow model/material, and trident renderer/glint path; it does not synthesize item icons, white quads, or another particle renderer. Entity-view snapshots are explicitly excluded from item-icon `ItemRenderInfo` extraction.

## Tests added

- `EntityStore` snapshots ExperienceOrb, Arrow, SpectralArrow, and Trident data; after source entities are removed, the pickup store still supplies all four snapshots and expires them after three ticks.
- Collector target height and squared interpolation are checked using local eye height; pre-shrink item stack count stays covered by the existing take regression.
- `pickup_entity_render_infos` test checks the emitted normal renderer model kinds and arrow variant/trident foil, proving the snapshot consumer translates to the ordinary model renderer path.
- Core take test verifies only Java's four concrete source kinds (orb, normal/spectral arrow, trident) create animations; Snowball remains excluded.

## Verification caveat

Automated tests and build checks must be run after the concurrent workspace compilation errors are resolved. No game-client visual comparison has yet been performed. See the task report for commands and exact status.
