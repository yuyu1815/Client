use std::collections::HashMap;

use azalea_inventory::components::UseCooldown;
use azalea_inventory::{ItemStack, ItemStackData};
use azalea_registry::identifier::Identifier;

#[derive(Default)]
pub(crate) struct CooldownTracker {
    tick: i64,
    cooldowns: HashMap<Identifier, Cooldown>,
}

#[derive(Clone, Copy)]
struct Cooldown {
    start: i64,
    end: i64,
}

impl CooldownTracker {
    pub(crate) fn apply(&mut self, group: Identifier, duration: i32) {
        if duration == 0 {
            self.cooldowns.remove(&group);
        } else {
            self.cooldowns.insert(
                group,
                Cooldown {
                    start: self.tick,
                    end: self.tick + i64::from(duration),
                },
            );
        }
    }

    pub(crate) fn tick(&mut self) {
        self.tick += 1;
        self.cooldowns
            .retain(|_, cooldown| cooldown.end > self.tick);
    }

    pub(crate) fn fraction(&self, stack: &ItemStack, partial_tick: f32) -> f32 {
        let Some(stack) = present_stack(stack) else {
            return 0.0;
        };
        let group = cooldown_group(stack);
        let Some(cooldown) = self.cooldowns.get(&group) else {
            return 0.0;
        };
        let duration = (cooldown.end - cooldown.start) as f32;
        let remaining = (cooldown.end - self.tick) as f32 - partial_tick;
        (remaining / duration).clamp(0.0, 1.0)
    }

    pub(crate) fn is_on_cooldown(&self, stack: &ItemStack) -> bool {
        self.fraction(stack, 0.0) > 0.0
    }
}

fn present_stack(stack: &ItemStack) -> Option<&ItemStackData> {
    match stack {
        ItemStack::Present(data) if data.count > 0 => Some(data),
        ItemStack::Empty | ItemStack::Present(_) => None,
    }
}

fn cooldown_group(stack: &ItemStackData) -> Identifier {
    stack
        .get_component::<UseCooldown>()
        .and_then(|cooldown| cooldown.cooldown_group.clone())
        .unwrap_or_else(|| Identifier::new(stack.kind.to_string()))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use azalea_buf::AzBuf;
    use azalea_inventory::DataComponentPatch;
    use azalea_inventory::components::DataComponentUnion;
    use azalea_registry::builtin::{DataComponentKind, ItemKind};

    use super::*;

    fn stack(kind: ItemKind, group: Option<&str>) -> ItemStack {
        let mut data = ItemStackData::new(kind, 1);
        if let Some(group) = group {
            let mut patch = DataComponentPatch::default();
            let component = UseCooldown {
                seconds: 1.0,
                cooldown_group: Some(Identifier::new(group)),
            };
            let mut encoded = Vec::new();
            component.azalea_write(&mut encoded).unwrap();
            let mut cursor = Cursor::new(encoded.as_slice());
            let union =
                DataComponentUnion::azalea_read_as(DataComponentKind::UseCooldown, &mut cursor)
                    .unwrap();
            // SAFETY: the union was decoded with the matching component kind.
            unsafe {
                patch.unchecked_insert_component(DataComponentKind::UseCooldown, Some(union));
            }
            data.component_patch = patch;
        }
        ItemStack::Present(data)
    }

    #[test]
    fn explicit_groups_are_shared_and_default_groups_are_per_item() {
        let mut tracker = CooldownTracker::default();
        let shared_a = stack(ItemKind::Stone, Some("test:shared"));
        let shared_b = stack(ItemKind::Dirt, Some("test:shared"));
        let stone = stack(ItemKind::Stone, None);
        let dirt = stack(ItemKind::Dirt, None);

        tracker.apply(Identifier::new("test:shared"), 8);
        tracker.apply(Identifier::new("minecraft:stone"), 8);
        assert!(tracker.is_on_cooldown(&shared_a));
        assert!(tracker.is_on_cooldown(&shared_b));
        assert!(tracker.is_on_cooldown(&stone));
        assert!(!tracker.is_on_cooldown(&dirt));
    }

    #[test]
    fn replacement_removal_and_tick_expiry() {
        let stone = stack(ItemKind::Stone, None);
        let mut tracker = CooldownTracker::default();
        let group = Identifier::new("minecraft:stone");
        tracker.apply(group.clone(), 10);
        tracker.tick();
        tracker.apply(group.clone(), 4);
        assert_eq!(tracker.fraction(&stone, 0.0), 1.0);
        tracker.tick();
        assert_eq!(tracker.fraction(&stone, 0.5), 0.625);
        tracker.apply(group.clone(), 0);
        assert!(!tracker.is_on_cooldown(&stone));

        tracker.apply(group, 1);
        assert!(tracker.is_on_cooldown(&stone));
        tracker.tick();
        assert!(!tracker.is_on_cooldown(&stone));
    }

    #[test]
    fn fraction_keeps_short_cooldowns_precise_after_large_absolute_ticks() {
        let stone = stack(ItemKind::Stone, None);
        let mut tracker = CooldownTracker {
            tick: (1 << 24) + 1,
            cooldowns: HashMap::new(),
        };
        let group = Identifier::new("minecraft:stone");
        tracker.apply(group, 4);
        assert_eq!(tracker.fraction(&stone, 0.0), 1.0);
        assert_eq!(tracker.fraction(&stone, 0.5), 0.875);
        tracker.tick();
        tracker.tick();
        tracker.tick();
        assert_eq!(tracker.fraction(&stone, 0.0), 0.25);
        tracker.tick();
        assert_eq!(tracker.fraction(&stone, 0.0), 0.0);
        assert!(!tracker.is_on_cooldown(&stone));
    }

    #[test]
    fn negative_duration_matches_vanilla_until_next_tick() {
        let stone = stack(ItemKind::Stone, None);
        let mut tracker = CooldownTracker::default();
        tracker.apply(Identifier::new("minecraft:stone"), -40);
        assert_eq!(tracker.fraction(&stone, 0.5), 1.0);
        assert!(tracker.is_on_cooldown(&stone));
        tracker.tick();
        assert_eq!(tracker.fraction(&stone, 0.0), 0.0);
    }
}
