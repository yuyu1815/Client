//! Dedicated layouts for server-opened menus without local recipe models.
use azalea_inventory::ItemStack;

use super::common;
use super::container::{
    ContainerInput, ContainerResult, DragState, SlotCtx, push_cursor_stack, push_panel,
    resolve_gesture,
};
use crate::player::menu_click::ContainerKind;
use crate::renderer::pipelines::menu_overlay::{MenuElement, SpriteId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpecialMenu {
    Dispenser,
    BrewingStand,
    Cartography,
    Grindstone,
    Smithing,
    Crafter,
    Stonecutter,
    Loom,
}

impl SpecialMenu {
    pub const fn storage_slots(self) -> usize {
        match self {
            Self::Dispenser | Self::Crafter => 9,
            Self::BrewingStand => 5,
            Self::Cartography | Self::Grindstone => 3,
            Self::Stonecutter => 2,
            Self::Smithing | Self::Loom => 4,
        }
    }

    pub const fn slot_count(self) -> usize {
        self.storage_slots() + 36
    }

    pub const fn result_slot(self) -> Option<usize> {
        match self {
            Self::Cartography | Self::Grindstone => Some(2),
            Self::Stonecutter => Some(1),
            Self::Smithing | Self::Loom => Some(3),
            _ => None,
        }
    }

    fn kind(self) -> ContainerKind {
        match self {
            Self::Dispenser => ContainerKind::Dispenser,
            Self::BrewingStand => ContainerKind::BrewingStand,
            Self::Cartography => ContainerKind::Cartography,
            Self::Grindstone => ContainerKind::Grindstone,
            Self::Smithing => ContainerKind::Smithing,
            Self::Crafter => ContainerKind::Crafter,
            Self::Stonecutter => ContainerKind::Stonecutter,
            Self::Loom => ContainerKind::Loom,
        }
    }
}

fn crafter_slot_disabled(property: i16, slot: u16) -> bool {
    slot < 9 && (property as u16 & (1 << slot)) != 0
}

fn crafter_button_id(slot: u16) -> Option<u32> {
    (slot < 9).then_some(slot as u32)
}

/// Draws a menu-specific slot arrangement. Inputs/results are always updated by
/// the server; no recipe outputs or selectable recipe order are synthesized.
#[allow(clippy::too_many_arguments)]
pub fn build(
    elements: &mut Vec<MenuElement>,
    screen_w: f32,
    screen_h: f32,
    cursor: (f32, f32),
    input: &ContainerInput,
    menu: SpecialMenu,
    slots: &[ItemStack],
    data: &[i16; 10],
    title: &str,
    cursor_item: &ItemStack,
    drag: &mut Option<DragState>,
    last_click: &mut Option<(u16, std::time::Instant)>,
    gs: f32,
    advanced_tooltips: bool,
) -> ContainerResult {
    let (height, slot_positions, player_y) = match menu {
        SpecialMenu::Dispenser | SpecialMenu::Crafter => (
            166.0,
            (0..9)
                .map(|i| (62.0 + (i % 3) as f32 * 18.0, 17.0 + (i / 3) as f32 * 18.0))
                .collect::<Vec<_>>(),
            84.0,
        ),
        SpecialMenu::BrewingStand => (
            166.0,
            vec![
                (56.0, 51.0),
                (79.0, 51.0),
                (102.0, 51.0),
                (79.0, 17.0),
                (17.0, 17.0),
            ],
            84.0,
        ),
        SpecialMenu::Cartography | SpecialMenu::Grindstone => {
            (166.0, vec![(15.0, 17.0), (15.0, 53.0), (124.0, 35.0)], 84.0)
        }
        SpecialMenu::Smithing => (
            166.0,
            vec![(8.0, 17.0), (26.0, 35.0), (8.0, 53.0), (124.0, 35.0)],
            84.0,
        ),
        SpecialMenu::Stonecutter => (166.0, vec![(20.0, 35.0), (143.0, 35.0)], 84.0),
        SpecialMenu::Loom => (
            166.0,
            vec![(13.0, 17.0), (13.0, 35.0), (13.0, 53.0), (143.0, 35.0)],
            84.0,
        ),
    };
    let background = match menu {
        SpecialMenu::Dispenser => SpriteId::DispenserBackground,
        SpecialMenu::BrewingStand => SpriteId::BrewingStandBackground,
        SpecialMenu::Cartography => SpriteId::CartographyBackground,
        SpecialMenu::Grindstone => SpriteId::GrindstoneBackground,
        SpecialMenu::Smithing => SpriteId::SmithingBackground,
        SpecialMenu::Crafter => SpriteId::CrafterBackground,
        SpecialMenu::Stonecutter => SpriteId::StonecutterBackground,
        SpecialMenu::Loom => SpriteId::LoomBackground,
    };
    let panel = push_panel(elements, screen_w, screen_h, gs, height, background);
    panel.label(elements, 8.0, 6.0, title);
    panel.label(elements, 8.0, 72.0, "Inventory");
    let kind = menu.kind();
    let mut ctx = SlotCtx::new(elements, &panel, cursor, kind, slots, cursor_item, drag);
    for (index, &(x, y)) in slot_positions.iter().enumerate() {
        ctx.slot(
            x as f32,
            y as f32,
            slots.get(index).unwrap_or(&ItemStack::Empty),
            None,
            index as u16,
        );
    }
    let inv = menu.storage_slots() as u16;
    ctx.player_rows(slots, inv, inv + 27, player_y);
    let (hovered, shown_cursor) = ctx.finish(cursor_item);
    if menu == SpecialMenu::Crafter {
        for i in 0..9 {
            if crafter_slot_disabled(data[0], i) {
                elements.push(MenuElement::Rect {
                    x: panel.ox + (60.0 + (i % 3) as f32 * 18.0) * panel.scale,
                    y: panel.oy + (15.0 + (i / 3) as f32 * 18.0) * panel.scale,
                    w: 18.0 * panel.scale,
                    h: 18.0 * panel.scale,
                    corner_radius: 0.0,
                    color: [0.1, 0.1, 0.1, 0.7],
                });
            }
        }
    }
    push_cursor_stack(elements, cursor, panel.scale, &shown_cursor);
    if cursor_item.is_empty()
        && let Some(item) = hovered
            .and_then(|slot| slots.get(slot as usize))
            .and_then(ItemStack::as_present)
        && let Ok(value) = serde_json::to_value(item)
    {
        let lines = crate::ui::chat::item_tooltip_lines(&value, None, advanced_tooltips);
        if !lines.is_empty() {
            common::push_tooltip_lines(elements, cursor, screen_w, screen_h, panel.scale, lines);
        }
    }
    let (mut ops, clicked_outside) = resolve_gesture(
        input,
        hovered,
        &panel,
        cursor,
        kind,
        slots,
        cursor_item,
        drag,
        last_click,
    );
    let button = if menu == SpecialMenu::Crafter && input.left_pressed {
        hovered.and_then(crafter_button_id)
    } else {
        None
    };
    if button.is_some() {
        ops.clear();
    }
    ContainerResult {
        clicked_outside,
        ops,
        button,
        recipe_id: None,
    }
}

#[cfg(test)]
mod tests {
    use super::SpecialMenu as M;
    #[test]
    fn crafter_button_ids_follow_slot_order_and_property_bits() {
        for slot in 0..9u16 {
            assert_eq!(super::crafter_button_id(slot), Some(slot as u32));
            assert_eq!(super::crafter_slot_disabled(1 << slot, slot), true);
        }
        assert_eq!(super::crafter_button_id(9), None);
        assert!(!super::crafter_slot_disabled(0x1ff, 9));
    }

    #[test]
    fn special_menu_slots_and_output_boundaries() {
        for (menu, count, result) in [
            (M::Dispenser, 45, None),
            (M::BrewingStand, 41, None),
            (M::Cartography, 39, Some(2)),
            (M::Grindstone, 39, Some(2)),
            (M::Smithing, 40, Some(3)),
            (M::Crafter, 45, None),
            (M::Stonecutter, 38, Some(1)),
            (M::Loom, 40, Some(3)),
        ] {
            assert_eq!(menu.slot_count(), count);
            assert_eq!(menu.result_slot(), result);
        }
    }
}
