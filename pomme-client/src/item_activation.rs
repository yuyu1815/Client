use azalea_inventory::components::DeathProtection;
use azalea_inventory::{ItemStack, ItemStackData};
use azalea_registry::builtin::{DataComponentKind, ItemKind};

use crate::player::inventory::Inventory;

/// Exact 26.2 EntityTypes -> nearest getSoundSource mapping. Rabbit metadata
/// was already normalized by EntityStore::apply_entity_data (wire EVIL=99 ->
/// stored variant=6).
pub fn sound_category(
    kind: azalea_registry::builtin::EntityKind,
    rabbit_variant: Option<u32>,
) -> Option<crate::audio::SoundCategory> {
    use azalea_registry::builtin::EntityKind;

    use crate::audio::SoundCategory;
    Some(match kind {
        EntityKind::AcaciaBoat => SoundCategory::Neutral,
        EntityKind::AcaciaChestBoat => SoundCategory::Neutral,
        EntityKind::Allay => SoundCategory::Neutral,
        EntityKind::AreaEffectCloud => SoundCategory::Neutral,
        EntityKind::Armadillo => SoundCategory::Neutral,
        EntityKind::ArmorStand => SoundCategory::Neutral,
        EntityKind::Arrow => SoundCategory::Neutral,
        EntityKind::Axolotl => SoundCategory::Neutral,
        EntityKind::BambooChestRaft => SoundCategory::Neutral,
        EntityKind::BambooRaft => SoundCategory::Neutral,
        EntityKind::Bat => SoundCategory::Neutral,
        EntityKind::Bee => SoundCategory::Neutral,
        EntityKind::BirchBoat => SoundCategory::Neutral,
        EntityKind::BirchChestBoat => SoundCategory::Neutral,
        EntityKind::Blaze => SoundCategory::Hostile,
        EntityKind::BlockDisplay => SoundCategory::Neutral,
        EntityKind::Bogged => SoundCategory::Hostile,
        EntityKind::Breeze => SoundCategory::Hostile,
        EntityKind::BreezeWindCharge => SoundCategory::Neutral,
        EntityKind::Camel => SoundCategory::Neutral,
        EntityKind::CamelHusk => SoundCategory::Neutral,
        EntityKind::Cat => SoundCategory::Neutral,
        EntityKind::CaveSpider => SoundCategory::Hostile,
        EntityKind::CherryBoat => SoundCategory::Neutral,
        EntityKind::CherryChestBoat => SoundCategory::Neutral,
        EntityKind::ChestMinecart => SoundCategory::Neutral,
        EntityKind::Chicken => SoundCategory::Neutral,
        EntityKind::Cod => SoundCategory::Neutral,
        EntityKind::CopperGolem => SoundCategory::Neutral,
        EntityKind::CommandBlockMinecart => SoundCategory::Neutral,
        EntityKind::Cow => SoundCategory::Neutral,
        EntityKind::Creaking => SoundCategory::Hostile,
        EntityKind::Creeper => SoundCategory::Hostile,
        EntityKind::DarkOakBoat => SoundCategory::Neutral,
        EntityKind::DarkOakChestBoat => SoundCategory::Neutral,
        EntityKind::Dolphin => SoundCategory::Neutral,
        EntityKind::Donkey => SoundCategory::Neutral,
        EntityKind::DragonFireball => SoundCategory::Neutral,
        EntityKind::Drowned => SoundCategory::Hostile,
        EntityKind::Egg => SoundCategory::Neutral,
        EntityKind::ElderGuardian => SoundCategory::Hostile,
        EntityKind::Enderman => SoundCategory::Hostile,
        EntityKind::Endermite => SoundCategory::Hostile,
        EntityKind::EnderDragon => SoundCategory::Hostile,
        EntityKind::EnderPearl => SoundCategory::Neutral,
        EntityKind::EndCrystal => SoundCategory::Neutral,
        EntityKind::Evoker => SoundCategory::Hostile,
        EntityKind::EvokerFangs => SoundCategory::Neutral,
        EntityKind::ExperienceBottle => SoundCategory::Neutral,
        EntityKind::ExperienceOrb => SoundCategory::Ambient,
        EntityKind::EyeOfEnder => SoundCategory::Neutral,
        EntityKind::FallingBlock => SoundCategory::Neutral,
        EntityKind::Fireball => SoundCategory::Neutral,
        EntityKind::FireworkRocket => SoundCategory::Neutral,
        EntityKind::Fox => SoundCategory::Neutral,
        EntityKind::Frog => SoundCategory::Neutral,
        EntityKind::FurnaceMinecart => SoundCategory::Neutral,
        EntityKind::Ghast => SoundCategory::Hostile,
        EntityKind::HappyGhast => SoundCategory::Neutral,
        EntityKind::Giant => SoundCategory::Hostile,
        EntityKind::GlowItemFrame => SoundCategory::Neutral,
        EntityKind::GlowSquid => SoundCategory::Neutral,
        EntityKind::Goat => SoundCategory::Neutral,
        EntityKind::Guardian => SoundCategory::Hostile,
        EntityKind::Hoglin => SoundCategory::Hostile,
        EntityKind::HopperMinecart => SoundCategory::Neutral,
        EntityKind::Horse => SoundCategory::Neutral,
        EntityKind::Husk => SoundCategory::Hostile,
        EntityKind::Illusioner => SoundCategory::Hostile,
        EntityKind::Interaction => SoundCategory::Neutral,
        EntityKind::IronGolem => SoundCategory::Neutral,
        EntityKind::Item => SoundCategory::Ambient,
        EntityKind::ItemDisplay => SoundCategory::Neutral,
        EntityKind::ItemFrame => SoundCategory::Neutral,
        EntityKind::JungleBoat => SoundCategory::Neutral,
        EntityKind::JungleChestBoat => SoundCategory::Neutral,
        EntityKind::LeashKnot => SoundCategory::Neutral,
        EntityKind::LightningBolt => SoundCategory::Weather,
        EntityKind::Llama => SoundCategory::Neutral,
        EntityKind::LlamaSpit => SoundCategory::Neutral,
        EntityKind::MagmaCube => SoundCategory::Hostile,
        EntityKind::MangroveBoat => SoundCategory::Neutral,
        EntityKind::MangroveChestBoat => SoundCategory::Neutral,
        EntityKind::Mannequin => SoundCategory::Neutral,
        EntityKind::Marker => SoundCategory::Neutral,
        EntityKind::Minecart => SoundCategory::Neutral,
        EntityKind::Mooshroom => SoundCategory::Neutral,
        EntityKind::Mule => SoundCategory::Neutral,
        EntityKind::Nautilus => SoundCategory::Neutral,
        EntityKind::OakBoat => SoundCategory::Neutral,
        EntityKind::OakChestBoat => SoundCategory::Neutral,
        EntityKind::Ocelot => SoundCategory::Neutral,
        EntityKind::OminousItemSpawner => SoundCategory::Neutral,
        EntityKind::Painting => SoundCategory::Neutral,
        EntityKind::PaleOakBoat => SoundCategory::Neutral,
        EntityKind::PaleOakChestBoat => SoundCategory::Neutral,
        EntityKind::Panda => SoundCategory::Neutral,
        EntityKind::Parched => SoundCategory::Hostile,
        EntityKind::Parrot => SoundCategory::Neutral,
        EntityKind::Phantom => SoundCategory::Hostile,
        EntityKind::Pig => SoundCategory::Neutral,
        EntityKind::Piglin => SoundCategory::Hostile,
        EntityKind::PiglinBrute => SoundCategory::Hostile,
        EntityKind::Pillager => SoundCategory::Hostile,
        EntityKind::PolarBear => SoundCategory::Neutral,
        EntityKind::SplashPotion => SoundCategory::Neutral,
        EntityKind::LingeringPotion => SoundCategory::Neutral,
        EntityKind::Pufferfish => SoundCategory::Neutral,
        EntityKind::Rabbit => {
            if rabbit_variant == Some(6) {
                SoundCategory::Hostile
            } else {
                SoundCategory::Neutral
            }
        }
        EntityKind::Ravager => SoundCategory::Hostile,
        EntityKind::Salmon => SoundCategory::Neutral,
        EntityKind::Sheep => SoundCategory::Neutral,
        EntityKind::Shulker => SoundCategory::Hostile,
        EntityKind::ShulkerBullet => SoundCategory::Hostile,
        EntityKind::Silverfish => SoundCategory::Hostile,
        EntityKind::Skeleton => SoundCategory::Hostile,
        EntityKind::SkeletonHorse => SoundCategory::Neutral,
        EntityKind::Slime => SoundCategory::Hostile,
        EntityKind::SmallFireball => SoundCategory::Neutral,
        EntityKind::Sniffer => SoundCategory::Neutral,
        EntityKind::Snowball => SoundCategory::Neutral,
        EntityKind::SnowGolem => SoundCategory::Neutral,
        EntityKind::SpawnerMinecart => SoundCategory::Neutral,
        EntityKind::SpectralArrow => SoundCategory::Neutral,
        EntityKind::Spider => SoundCategory::Hostile,
        EntityKind::SpruceBoat => SoundCategory::Neutral,
        EntityKind::SpruceChestBoat => SoundCategory::Neutral,
        EntityKind::Squid => SoundCategory::Neutral,
        EntityKind::Stray => SoundCategory::Hostile,
        EntityKind::Strider => SoundCategory::Neutral,
        EntityKind::SulfurCube => SoundCategory::Neutral,
        EntityKind::Tadpole => SoundCategory::Neutral,
        EntityKind::TextDisplay => SoundCategory::Neutral,
        EntityKind::Tnt => SoundCategory::Neutral,
        EntityKind::TntMinecart => SoundCategory::Neutral,
        EntityKind::TraderLlama => SoundCategory::Neutral,
        EntityKind::Trident => SoundCategory::Neutral,
        EntityKind::TropicalFish => SoundCategory::Neutral,
        EntityKind::Turtle => SoundCategory::Neutral,
        EntityKind::Vex => SoundCategory::Hostile,
        EntityKind::Villager => SoundCategory::Neutral,
        EntityKind::Vindicator => SoundCategory::Hostile,
        EntityKind::WanderingTrader => SoundCategory::Neutral,
        EntityKind::Warden => SoundCategory::Hostile,
        EntityKind::WindCharge => SoundCategory::Neutral,
        EntityKind::Witch => SoundCategory::Hostile,
        EntityKind::Wither => SoundCategory::Hostile,
        EntityKind::WitherSkeleton => SoundCategory::Hostile,
        EntityKind::WitherSkull => SoundCategory::Neutral,
        EntityKind::Wolf => SoundCategory::Neutral,
        EntityKind::Zoglin => SoundCategory::Hostile,
        EntityKind::Zombie => SoundCategory::Hostile,
        EntityKind::ZombieHorse => SoundCategory::Neutral,
        EntityKind::ZombieNautilus => SoundCategory::Neutral,
        EntityKind::ZombieVillager => SoundCategory::Hostile,
        EntityKind::ZombifiedPiglin => SoundCategory::Hostile,
        EntityKind::Player => SoundCategory::Players,
        EntityKind::FishingBobber => SoundCategory::Neutral,
    })
}

/// Borrowed activation payload consumed by the renderer for this frame.
#[derive(Clone, Copy, Debug)]
pub struct ItemActivationDraw<'a> {
    pub stack: &'a ItemStack,
    pub ticks_remaining: u32,
    pub offset_x: f32,
    pub offset_y: f32,
    pub partial_tick: f32,
}

/// Client-side, non-authoritative view of vanilla's 40-tick item activation.
#[derive(Clone, Debug)]
pub struct ItemActivation {
    stack: ItemStack,
    ticks_remaining: u32,
    offset_x: f32,
    offset_y: f32,
}

impl ItemActivation {
    pub const DURATION: u32 = 40;

    pub fn new(stack: ItemStack, offset_x: f32, offset_y: f32) -> Self {
        Self {
            stack,
            ticks_remaining: Self::DURATION,
            offset_x,
            offset_y,
        }
    }

    /// Advance once per client simulation tick, never once per rendered frame.
    /// Returns false on the tick which reaches zero (and while already zero).
    pub fn tick(&mut self) -> bool {
        if self.ticks_remaining == 0 {
            return false;
        }
        self.ticks_remaining -= 1;
        self.ticks_remaining != 0
    }

    /// Drawing is observational: repeated calls do not advance the animation.
    pub fn draw(&self, partial_tick: f32) -> Option<ItemActivationDraw<'_>> {
        (self.ticks_remaining != 0).then_some(ItemActivationDraw {
            stack: &self.stack,
            ticks_remaining: self.ticks_remaining,
            offset_x: self.offset_x,
            offset_y: self.offset_y,
            partial_tick,
        })
    }

    pub fn ticks_remaining(&self) -> u32 {
        self.ticks_remaining
    }
}

/// Resolve DeathProtection without reviving an explicitly removed default.
pub fn has_death_protection(stack: &ItemStackData) -> bool {
    for (kind, value) in stack.component_patch.iter() {
        if kind == DataComponentKind::DeathProtection {
            return value.is_some();
        }
    }
    azalea_inventory::default_components::get_default_component::<DeathProtection>(stack.kind)
        .is_some()
}

/// Vanilla hand order: selected main-hand stack, offhand, then a fresh default
/// Totem.
pub fn find_totem_stack(inventory: &Inventory, selected_slot: u8) -> ItemStack {
    if let Some(stack) = inventory
        .held_stack(selected_slot)
        .filter(|stack| has_death_protection(stack))
    {
        return ItemStack::Present(stack.clone());
    }
    if let ItemStack::Present(stack) = inventory.offhand()
        && stack.count > 0
        && has_death_protection(stack)
    {
        return ItemStack::Present(stack.clone());
    }
    ItemStack::from(ItemKind::TotemOfUndying)
}

/// Return activation only for an existing local-player target; remote/unknown
/// events never overlay.
pub fn activation_for_event(
    entity_id: i32,
    local_player_id: i32,
    entity_exists: bool,
    inventory: &Inventory,
    selected_slot: u8,
    offset_x: f32,
    offset_y: f32,
) -> Option<ItemActivation> {
    (entity_exists && entity_id == local_player_id).then(|| {
        ItemActivation::new(
            find_totem_stack(inventory, selected_slot),
            offset_x,
            offset_y,
        )
    })
}

#[cfg(test)]
mod tests {
    use azalea_inventory::components::{DataComponentUnion, DeathProtection};
    use azalea_inventory::{ItemStack, ItemStackData};
    use azalea_registry::builtin::{DataComponentKind, ItemKind};

    use super::{ItemActivation, activation_for_event, find_totem_stack, has_death_protection};
    use crate::player::inventory::{Inventory, OFFHAND};

    fn stack(kind: ItemKind) -> ItemStackData {
        ItemStackData::from(kind)
    }

    fn added_protection(mut stack: ItemStackData) -> ItemStackData {
        // SAFETY: union field matches DataComponentKind::DeathProtection.
        unsafe {
            stack.component_patch.unchecked_insert_component(
                DataComponentKind::DeathProtection,
                Some(DataComponentUnion::from(DeathProtection {
                    death_effects: Vec::new(),
                })),
            );
        }
        stack
    }

    fn removed_protection(mut stack: ItemStackData) -> ItemStackData {
        // SAFETY: None is an explicit tombstone and has no union payload.
        unsafe {
            stack
                .component_patch
                .unchecked_insert_component(DataComponentKind::DeathProtection, None);
        }
        stack
    }

    fn inventory_with(selected: ItemStack, offhand: ItemStack) -> Inventory {
        let mut inventory = Inventory::new();
        inventory.set_slot(36, selected);
        inventory.set_slot(OFFHAND, offhand);
        inventory
    }

    #[test]
    fn main_hand_wins_and_cloned_snapshot_keeps_custom_stack_data() {
        let mut custom = added_protection(stack(ItemKind::Stone));
        custom.count = 7;
        let offhand = ItemStack::from(ItemKind::TotemOfUndying);
        let inventory = inventory_with(ItemStack::Present(custom.clone()), offhand);
        let selected = find_totem_stack(&inventory, 0);
        assert_eq!(selected, ItemStack::Present(custom));
        assert_ne!(selected, ItemStack::from(ItemKind::TotemOfUndying));
        assert!(matches!(selected, ItemStack::Present(ref data) if data.count == 7));
    }

    #[test]
    fn offhand_is_selected_when_main_has_no_protection() {
        let mut offhand = ItemStackData::from(ItemKind::TotemOfUndying);
        offhand.count = 2;
        let inventory = inventory_with(
            ItemStack::from(ItemKind::Stone),
            ItemStack::Present(offhand.clone()),
        );
        assert_eq!(find_totem_stack(&inventory, 0), ItemStack::Present(offhand));
    }

    #[test]
    fn added_custom_component_qualifies_even_when_item_is_not_totem() {
        let added = added_protection(stack(ItemKind::Stone));
        assert!(has_death_protection(&added));
        let inventory = inventory_with(ItemStack::Present(added.clone()), ItemStack::Empty);
        assert_eq!(find_totem_stack(&inventory, 0), ItemStack::Present(added));
    }

    #[test]
    fn explicit_removal_blocks_totem_default_and_falls_back_only_after_both_hands() {
        let removed = removed_protection(stack(ItemKind::TotemOfUndying));
        assert!(!has_death_protection(&removed));
        let inventory = inventory_with(ItemStack::Present(removed), ItemStack::Empty);
        let selected = find_totem_stack(&inventory, 0);
        assert_eq!(selected, ItemStack::from(ItemKind::TotemOfUndying));
        assert!(
            matches!(selected, ItemStack::Present(ref data) if data.component_patch.iter().next().is_none())
        );
    }

    #[test]
    fn default_protection_selects_totem_and_nonqualifying_inventory_falls_back() {
        assert!(has_death_protection(&stack(ItemKind::TotemOfUndying)));
        assert!(!has_death_protection(&stack(ItemKind::Stone)));
        let inventory = inventory_with(ItemStack::Empty, ItemStack::Empty);
        assert_eq!(
            find_totem_stack(&inventory, 0),
            ItemStack::from(ItemKind::TotemOfUndying)
        );
    }

    #[test]
    fn activation_is_local_only_requires_existing_target_and_snapshots_stack() {
        let inventory = inventory_with(ItemStack::from(ItemKind::TotemOfUndying), ItemStack::Empty);
        let local = activation_for_event(7, 7, true, &inventory, 0, -0.5, 0.25).unwrap();
        assert_eq!(local.ticks_remaining(), 40);
        assert_eq!(
            local.draw(0.5).unwrap().stack,
            &ItemStack::from(ItemKind::TotemOfUndying)
        );
        assert!(activation_for_event(8, 7, true, &inventory, 0, 0.0, 0.0).is_none());
        assert!(activation_for_event(7, 7, false, &inventory, 0, 0.0, 0.0).is_none());
    }

    #[test]
    fn activation_lasts_40_simulation_ticks_and_draw_does_not_decrement() {
        let mut activation = ItemActivation::new(ItemStack::Empty, -0.25, 0.75);
        assert_eq!(activation.ticks_remaining(), 40);
        for _ in 0..4 {
            let draw = activation.draw(0.5).expect("active animation draws");
            assert_eq!(draw.ticks_remaining, 40);
            assert_eq!(draw.partial_tick, 0.5);
        }
        for _ in 0..39 {
            assert!(activation.tick());
        }
        assert_eq!(activation.ticks_remaining(), 1);
        assert!(!activation.tick());
        assert_eq!(activation.ticks_remaining(), 0);
        assert!(activation.draw(0.0).is_none());
        assert!(!activation.tick());
    }

    #[test]
    fn official_sound_mapping_special_cases_and_registry_examples() {
        use azalea_registry::builtin::EntityKind as K;

        use crate::audio::SoundCategory as C;
        assert_eq!(super::sound_category(K::Player, None), Some(C::Players));
        assert_eq!(super::sound_category(K::Item, None), Some(C::Ambient));
        assert_eq!(
            super::sound_category(K::LightningBolt, None),
            Some(C::Weather)
        );
        for k in [K::Bat, K::CamelHusk, K::ZombieNautilus, K::HappyGhast] {
            assert_eq!(super::sound_category(k, None), Some(C::Neutral));
        }
        for k in [K::Ghast, K::Slime, K::Phantom, K::Shulker, K::Hoglin] {
            assert_eq!(super::sound_category(k, None), Some(C::Hostile));
        }
        assert_eq!(super::sound_category(K::Rabbit, Some(6)), Some(C::Hostile));
        assert_eq!(super::sound_category(K::Rabbit, Some(0)), Some(C::Neutral));
        assert_eq!(super::sound_category(K::Rabbit, None), Some(C::Neutral));
    }

    #[test]
    fn every_fixed_official_entity_mapping_matches_the_source_map() {
        use azalea_registry::builtin::EntityKind as K;

        use crate::audio::SoundCategory as C;
        let fixed = [
            (K::AcaciaBoat, C::Neutral),
            (K::AcaciaChestBoat, C::Neutral),
            (K::Allay, C::Neutral),
            (K::AreaEffectCloud, C::Neutral),
            (K::Armadillo, C::Neutral),
            (K::ArmorStand, C::Neutral),
            (K::Arrow, C::Neutral),
            (K::Axolotl, C::Neutral),
            (K::BambooChestRaft, C::Neutral),
            (K::BambooRaft, C::Neutral),
            (K::Bat, C::Neutral),
            (K::Bee, C::Neutral),
            (K::BirchBoat, C::Neutral),
            (K::BirchChestBoat, C::Neutral),
            (K::Blaze, C::Hostile),
            (K::BlockDisplay, C::Neutral),
            (K::Bogged, C::Hostile),
            (K::Breeze, C::Hostile),
            (K::BreezeWindCharge, C::Neutral),
            (K::Camel, C::Neutral),
            (K::CamelHusk, C::Neutral),
            (K::Cat, C::Neutral),
            (K::CaveSpider, C::Hostile),
            (K::CherryBoat, C::Neutral),
            (K::CherryChestBoat, C::Neutral),
            (K::ChestMinecart, C::Neutral),
            (K::Chicken, C::Neutral),
            (K::Cod, C::Neutral),
            (K::CopperGolem, C::Neutral),
            (K::CommandBlockMinecart, C::Neutral),
            (K::Cow, C::Neutral),
            (K::Creaking, C::Hostile),
            (K::Creeper, C::Hostile),
            (K::DarkOakBoat, C::Neutral),
            (K::DarkOakChestBoat, C::Neutral),
            (K::Dolphin, C::Neutral),
            (K::Donkey, C::Neutral),
            (K::DragonFireball, C::Neutral),
            (K::Drowned, C::Hostile),
            (K::Egg, C::Neutral),
            (K::ElderGuardian, C::Hostile),
            (K::Enderman, C::Hostile),
            (K::Endermite, C::Hostile),
            (K::EnderDragon, C::Hostile),
            (K::EnderPearl, C::Neutral),
            (K::EndCrystal, C::Neutral),
            (K::Evoker, C::Hostile),
            (K::EvokerFangs, C::Neutral),
            (K::ExperienceBottle, C::Neutral),
            (K::ExperienceOrb, C::Ambient),
            (K::EyeOfEnder, C::Neutral),
            (K::FallingBlock, C::Neutral),
            (K::Fireball, C::Neutral),
            (K::FireworkRocket, C::Neutral),
            (K::Fox, C::Neutral),
            (K::Frog, C::Neutral),
            (K::FurnaceMinecart, C::Neutral),
            (K::Ghast, C::Hostile),
            (K::HappyGhast, C::Neutral),
            (K::Giant, C::Hostile),
            (K::GlowItemFrame, C::Neutral),
            (K::GlowSquid, C::Neutral),
            (K::Goat, C::Neutral),
            (K::Guardian, C::Hostile),
            (K::Hoglin, C::Hostile),
            (K::HopperMinecart, C::Neutral),
            (K::Horse, C::Neutral),
            (K::Husk, C::Hostile),
            (K::Illusioner, C::Hostile),
            (K::Interaction, C::Neutral),
            (K::IronGolem, C::Neutral),
            (K::Item, C::Ambient),
            (K::ItemDisplay, C::Neutral),
            (K::ItemFrame, C::Neutral),
            (K::JungleBoat, C::Neutral),
            (K::JungleChestBoat, C::Neutral),
            (K::LeashKnot, C::Neutral),
            (K::LightningBolt, C::Weather),
            (K::Llama, C::Neutral),
            (K::LlamaSpit, C::Neutral),
            (K::MagmaCube, C::Hostile),
            (K::MangroveBoat, C::Neutral),
            (K::MangroveChestBoat, C::Neutral),
            (K::Mannequin, C::Neutral),
            (K::Marker, C::Neutral),
            (K::Minecart, C::Neutral),
            (K::Mooshroom, C::Neutral),
            (K::Mule, C::Neutral),
            (K::Nautilus, C::Neutral),
            (K::OakBoat, C::Neutral),
            (K::OakChestBoat, C::Neutral),
            (K::Ocelot, C::Neutral),
            (K::OminousItemSpawner, C::Neutral),
            (K::Painting, C::Neutral),
            (K::PaleOakBoat, C::Neutral),
            (K::PaleOakChestBoat, C::Neutral),
            (K::Panda, C::Neutral),
            (K::Parched, C::Hostile),
            (K::Parrot, C::Neutral),
            (K::Phantom, C::Hostile),
            (K::Pig, C::Neutral),
            (K::Piglin, C::Hostile),
            (K::PiglinBrute, C::Hostile),
            (K::Pillager, C::Hostile),
            (K::PolarBear, C::Neutral),
            (K::SplashPotion, C::Neutral),
            (K::LingeringPotion, C::Neutral),
            (K::Pufferfish, C::Neutral),
            (K::Ravager, C::Hostile),
            (K::Salmon, C::Neutral),
            (K::Sheep, C::Neutral),
            (K::Shulker, C::Hostile),
            (K::ShulkerBullet, C::Hostile),
            (K::Silverfish, C::Hostile),
            (K::Skeleton, C::Hostile),
            (K::SkeletonHorse, C::Neutral),
            (K::Slime, C::Hostile),
            (K::SmallFireball, C::Neutral),
            (K::Sniffer, C::Neutral),
            (K::Snowball, C::Neutral),
            (K::SnowGolem, C::Neutral),
            (K::SpawnerMinecart, C::Neutral),
            (K::SpectralArrow, C::Neutral),
            (K::Spider, C::Hostile),
            (K::SpruceBoat, C::Neutral),
            (K::SpruceChestBoat, C::Neutral),
            (K::Squid, C::Neutral),
            (K::Stray, C::Hostile),
            (K::Strider, C::Neutral),
            (K::SulfurCube, C::Neutral),
            (K::Tadpole, C::Neutral),
            (K::TextDisplay, C::Neutral),
            (K::Tnt, C::Neutral),
            (K::TntMinecart, C::Neutral),
            (K::TraderLlama, C::Neutral),
            (K::Trident, C::Neutral),
            (K::TropicalFish, C::Neutral),
            (K::Turtle, C::Neutral),
            (K::Vex, C::Hostile),
            (K::Villager, C::Neutral),
            (K::Vindicator, C::Hostile),
            (K::WanderingTrader, C::Neutral),
            (K::Warden, C::Hostile),
            (K::WindCharge, C::Neutral),
            (K::Witch, C::Hostile),
            (K::Wither, C::Hostile),
            (K::WitherSkeleton, C::Hostile),
            (K::WitherSkull, C::Neutral),
            (K::Wolf, C::Neutral),
            (K::Zoglin, C::Hostile),
            (K::Zombie, C::Hostile),
            (K::ZombieHorse, C::Neutral),
            (K::ZombieNautilus, C::Neutral),
            (K::ZombieVillager, C::Hostile),
            (K::ZombifiedPiglin, C::Hostile),
            (K::Player, C::Players),
            (K::FishingBobber, C::Neutral),
        ];
        assert_eq!(fixed.len(), 157);
        for (kind, expected) in fixed {
            assert_eq!(
                super::sound_category(kind, None),
                Some(expected),
                "{kind:?}"
            );
        }
    }

    #[test]
    fn replacement_restarts_full_duration_and_keeps_offsets() {
        let mut activation = ItemActivation::new(ItemStack::Empty, 0.1, 0.2);
        for _ in 0..11 {
            assert!(activation.tick());
        }
        activation = ItemActivation::new(ItemStack::Empty, -0.4, 0.6);
        assert_eq!(activation.ticks_remaining(), 40);
        let draw = activation.draw(0.25).unwrap();
        assert_eq!((draw.offset_x, draw.offset_y), (-0.4, 0.6));
    }
}
