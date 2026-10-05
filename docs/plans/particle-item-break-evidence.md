# Java 26.2 LivingEntity breakItem particle / sound evidence

Java reference: `net/minecraft/world/entity/LivingEntity.java` (`handleEntityEvent`, `breakItem`, `spawnItemParticles`) and `EquipmentSlot.java`. Entity events preserve their normal packet order; the client does not clear or mutate equipment in response to the visual event.

## Java slot mapping and source snapshot

| Entity event | `EquipmentSlot` | Local Player source | Remote entity source |
|---:|---|---|---|
| 47 | MAINHAND | selected hotbar slot at event handling | `LivingEntity.equipment[Mainhand]` |
| 48 | OFFHAND | offhand inventory slot | `LivingEntity.equipment[Offhand]` |
| 49 | HEAD | helmet inventory slot | `LivingEntity.equipment[Head]` |
| 50 | CHEST | chest inventory slot | `LivingEntity.equipment[Chest]` |
| 51 | LEGS | legs inventory slot | `LivingEntity.equipment[Legs]` |
| 52 | FEET | boots inventory slot | `LivingEntity.equipment[Feet]` |
| 65 | BODY | no player body inventory mapping | `LivingEntity.equipment[Body]` |
| 68 | SADDLE | no player saddle inventory mapping | `LivingEntity.equipment[Saddle]` |

`handler.rs` forwards the eight event values as `EntityParticleEvent` before subsequent equipment metadata. Core snapshots the source stack at the event's current queue position, before the following empty-slot `SetEquipment` can replace it. Empty stacks produce neither item particles nor sound. The visual consumer does not update equipment; the regular metadata update remains authoritative.

Each nonempty stack produces five Java-shaped `ITEM` requests from the entity eye point using yaw/pitch-rotated samples and passes its native item ID, count, typed `DataComponentPatch`, and preserved raw components to the existing item-model material resolver. Break sound is played only when the entity is not silent and the stack carries `BreakSound`; category is player/entity appropriate and Java volume/pitch range is preserved.

## Ordering / duplication boundary

- EntityEvent dispatch occurs before the following equipment update and before unrelated existing Golem/Ravager/firework event behavior is changed.
- No local equipment shrink/clear is performed. Death, Golem/Ravager handling, and firework event 17 remain on their existing branches and are not re-emitted by break-item handling.
- BODY/SADDLE are read from the actual remote entity equipment map; the local-player slot helper intentionally returns no stack for slots absent from player inventory.

## Automated checks (serial from `Client/`)

- `mise exec -- cargo test -p pomme-client --locked --profile dev-fast living_equipment_break_events_map_to_java_slots_and_snapshot_inventory -- --nocapture`: exit **0**, **1 passed**.
- `mise exec -- cargo test -p pomme-client --locked --profile dev-fast living_item_break_particles_keep_stack_and_java_emission_shape -- --nocapture`: exit **0**, **1 passed**.
- `mise exec -- cargo test -p pomme-client --locked --profile dev-fast equipment_break_event_is_dispatched_before_the_following_empty_slot_update -- --nocapture`: exit **0**, **1 passed**.
- `mise exec -- cargo check -p pomme-client --tests --locked --profile dev-fast`: exit **0** on the integrated snapshot.

Sound playback on a live audio device, actual remote equipment packet capture, and GUI particle A/B were not performed. Test results validate slot selection, typed stack snapshots/request geometry, and packet event-before-empty-equipment ordering; they are not a visual sign-off.
