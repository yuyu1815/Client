use std::time::Instant;

use azalea_inventory::ItemStack;
use azalea_inventory::operations::ClickOperation;

use super::common::{SLOT_STRIDE, push_tooltip_lines};
use super::container::{
    ContainerInput, DragState, SlotCtx, push_cursor_stack, push_panel, resolve_gesture,
};
use crate::player::inventory::{self, Inventory};
use crate::player::menu_click::ContainerKind;
use crate::renderer::PlayerPreview;
use crate::renderer::pipelines::menu_overlay::{MenuElement, SpriteId};

// Vanilla player-menu slot indices, as u16 for click ops.
const SLOT_CRAFT_RESULT: u16 = inventory::CRAFT_OUTPUT as u16;
const SLOT_CRAFT_BASE: u16 = inventory::CRAFT_INPUT_START as u16;
const SLOT_ARMOR_BASE: u16 = inventory::ARMOR_START as u16;
const SLOT_MAIN_BASE: u16 = inventory::MAIN_START as u16;
const SLOT_HOTBAR_BASE: u16 = inventory::HOTBAR_START as u16;
const SLOT_OFFHAND: u16 = inventory::OFFHAND as u16;

const ARMOR_EMPTY_SPRITES: [SpriteId; 4] = [
    SpriteId::EmptyHelmet,
    SpriteId::EmptyChestplate,
    SpriteId::EmptyLeggings,
    SpriteId::EmptyBoots,
];

pub struct InventoryResult {
    pub clicked_outside: bool,
    /// Container-click operations to send this frame (usually 0-1; a drag
    /// release emits a start/add.../end sequence).
    pub ops: Vec<ClickOperation>,
    pub player_preview: PlayerPreview,
    pub recipe_id: Option<(u32, bool)>,
}

#[allow(clippy::too_many_arguments)]
pub fn build_inventory(
    elements: &mut Vec<MenuElement>,
    screen_w: f32,
    screen_h: f32,
    cursor: (f32, f32),
    input: &ContainerInput,
    inventory: &Inventory,
    cursor_item: &ItemStack,
    drag: &mut Option<DragState>,
    last_click: &mut Option<(u16, Instant)>,
    gs: f32,
    recipe_book: &mut crate::ui::recipe_book::RecipeBookState,
    native_recipes: bool,
    advanced_tooltips: bool,
) -> InventoryResult {
    let panel = push_panel(
        elements,
        screen_w,
        screen_h,
        gs,
        166.0,
        SpriteId::InventoryBackground,
    );
    panel.label(elements, 97.0, 6.0, "Crafting");

    let slots = inventory.slots();
    let mut ctx = SlotCtx::new(
        elements,
        &panel,
        cursor,
        ContainerKind::Player,
        slots,
        cursor_item,
        drag,
    );

    ctx.player_rows(slots, SLOT_MAIN_BASE, SLOT_HOTBAR_BASE, 84.0);

    let armor_ys = [8.0, 26.0, 44.0, 62.0];
    for i in 0..4u16 {
        let num = SLOT_ARMOR_BASE + i;
        ctx.slot(
            8.0,
            armor_ys[i as usize],
            inventory.slot(num as usize),
            Some(ARMOR_EMPTY_SPRITES[i as usize]),
            num,
        );
    }

    for row in 0..2u16 {
        for col in 0..2u16 {
            let num = SLOT_CRAFT_BASE + row * 2 + col;
            ctx.slot(
                98.0 + col as f32 * SLOT_STRIDE,
                18.0 + row as f32 * SLOT_STRIDE,
                inventory.slot(num as usize),
                None,
                num,
            );
        }
    }

    ctx.slot(
        154.0,
        28.0,
        inventory.craft_output(),
        None,
        SLOT_CRAFT_RESULT,
    );
    ctx.slot(
        77.0,
        62.0,
        inventory.offhand(),
        Some(SpriteId::EmptyShield),
        SLOT_OFFHAND,
    );

    let (hovered, shown_cursor) = ctx.finish(cursor_item);

    let grid = slots
        .get(SLOT_CRAFT_BASE as usize..SLOT_CRAFT_BASE as usize + 4)
        .unwrap_or(&[]);
    let recipe_items: Vec<_> = slots
        .get(SLOT_MAIN_BASE as usize..SLOT_HOTBAR_BASE as usize + 9)
        .unwrap_or(&[])
        .iter()
        .chain(grid.iter())
        .cloned()
        .collect();
    let recipe_id = crate::ui::container::push_recipe_entries(
        elements,
        &panel,
        recipe_book,
        cursor,
        input.left_pressed,
        native_recipes,
        None,
        input.shift,
        2,
        2,
        &recipe_items,
        grid,
        inventory.craft_output(),
        104.0,
        61.0,
    );
    push_cursor_stack(elements, cursor, panel.scale, &shown_cursor);
    if !cursor_item.is_present()
        && let Some(item) = hovered.and_then(|slot| inventory.slot(slot as usize).as_present())
        && let Ok(value) = serde_json::to_value(item)
    {
        let lines = super::chat::item_tooltip_lines(&value, None, advanced_tooltips);
        if !lines.is_empty() {
            push_tooltip_lines(elements, cursor, screen_w, screen_h, panel.scale, lines);
        }
    }

    let mut gesture_input = *input;
    if recipe_id.is_some() || recipe_book.clicked_ui {
        gesture_input.left_pressed = false;
        gesture_input.right_pressed = false;
        gesture_input.middle_pressed = false;
    }
    let (ops, clicked_outside) = resolve_gesture(
        &gesture_input,
        hovered,
        &panel,
        cursor,
        ContainerKind::Player,
        slots,
        cursor_item,
        drag,
        last_click,
    );

    InventoryResult {
        clicked_outside,
        ops,
        player_preview: PlayerPreview {
            rect: [
                panel.ox + 26.0 * panel.scale,
                panel.oy + 8.0 * panel.scale,
                49.0 * panel.scale,
                70.0 * panel.scale,
            ],
            gui_scale: panel.scale,
            cursor,
        },
        recipe_id,
    }
}

#[cfg(test)]
mod tests {
    use azalea_chat::FormattedText;
    use azalea_inventory::components::{Lore, TooltipDisplay};
    use azalea_registry::builtin::{DataComponentKind, ItemKind};

    use super::*;

    #[test]
    fn hovered_stack_tooltip_includes_serialized_lore() {
        let item = ItemStack::from(ItemKind::Stone).with_component(Lore {
            lines: vec![FormattedText::from("Server lore")],
        });
        let value = serde_json::to_value(item.as_present().expect("item should be present"))
            .expect("item components should serialize");
        let lines = super::super::chat::item_tooltip_lines(&value, None, false);

        assert!(
            lines
                .iter()
                .any(|line| line.spans.iter().any(|span| span.text == "Server lore"))
        );

        let item = ItemStack::from(ItemKind::Stone)
            .with_component(Lore {
                lines: vec![FormattedText::from("Server lore")],
            })
            .with_component(TooltipDisplay {
                hide_tooltip: false,
                hidden_components: vec![DataComponentKind::Lore],
            });
        let value = serde_json::to_value(item.as_present().expect("item should be present"))
            .expect("item components should serialize");
        let lines = super::super::chat::item_tooltip_lines(&value, None, false);
        assert!(
            !lines
                .iter()
                .any(|line| line.spans.iter().any(|span| span.text == "Server lore"))
        );

        let item = ItemStack::from(ItemKind::Stone).with_component(TooltipDisplay {
            hide_tooltip: true,
            hidden_components: Vec::new(),
        });
        let value = serde_json::to_value(item.as_present().expect("item should be present"))
            .expect("item components should serialize");
        assert!(super::super::chat::item_tooltip_lines(&value, None, false).is_empty());
    }
}
