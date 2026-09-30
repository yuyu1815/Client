//! The crafting table screen (vanilla `CraftingScreen` + `CraftingMenu`):
//! result slot 0, 3x3 grid 1..9, player inventory 10..36, hotbar 37..45.

use std::time::Instant;

use azalea_inventory::ItemStack;

use super::common::SLOT_STRIDE;
use super::container::{
    ContainerInput, ContainerResult, DragState, SlotCtx, push_cursor_stack, push_recipe_panel,
    resolve_gesture,
};
use crate::player::menu_click::ContainerKind;
use crate::renderer::pipelines::menu_overlay::{MenuElement, SpriteId};

const SLOT_RESULT: u16 = 0;
const SLOT_GRID_BASE: u16 = 1;
const SLOT_MAIN_BASE: u16 = 10;
const SLOT_HOTBAR_BASE: u16 = 37;

#[allow(clippy::too_many_arguments)]
pub fn build_crafting_table(
    elements: &mut Vec<MenuElement>,
    screen_w: f32,
    screen_h: f32,
    cursor: (f32, f32),
    input: &ContainerInput,
    slots: &[ItemStack],
    title: &str,
    cursor_item: &ItemStack,
    drag: &mut Option<DragState>,
    last_click: &mut Option<(u16, Instant)>,
    gs: f32,
    recipe_book: &mut crate::ui::recipe_book::RecipeBookState,
    native_recipes: bool,
    text_width_fn: &dyn Fn(&str, f32) -> f32,
    advanced_tooltips: bool,
) -> ContainerResult {
    let (panel, hidden) = push_recipe_panel(
        elements,
        screen_w,
        screen_h,
        gs,
        SpriteId::CraftingTableBackground,
        recipe_book,
        native_recipes,
        0,
        cursor,
        input,
        (5.0, 34.0),
    );
    if !hidden {
        panel.label(elements, 29.0, 6.0, title);
        panel.label(elements, 8.0, 72.0, "Inventory");
    }

    let mut ctx = SlotCtx::new(
        elements,
        &panel,
        cursor,
        ContainerKind::CraftingTable,
        slots,
        cursor_item,
        drag,
    );

    ctx.set_hidden(hidden);
    ctx.player_rows(slots, SLOT_MAIN_BASE, SLOT_HOTBAR_BASE, 84.0);

    for row in 0..3u16 {
        for col in 0..3u16 {
            let num = SLOT_GRID_BASE + row * 3 + col;
            let item = slots.get(num as usize).unwrap_or(&ItemStack::Empty);
            ctx.slot(
                30.0 + col as f32 * SLOT_STRIDE,
                17.0 + row as f32 * SLOT_STRIDE,
                item,
                None,
                num,
            );
        }
    }

    let result = slots.get(SLOT_RESULT as usize).unwrap_or(&ItemStack::Empty);
    ctx.slot(124.0, 35.0, result, None, SLOT_RESULT);

    let (hovered, shown_cursor) = ctx.finish(cursor_item);

    let grid = slots
        .get(SLOT_GRID_BASE as usize..SLOT_MAIN_BASE as usize)
        .unwrap_or(&[]);
    let recipe_items: Vec<_> = grid
        .iter()
        .chain(
            slots
                .get(SLOT_MAIN_BASE as usize..SLOT_HOTBAR_BASE as usize + 9)
                .unwrap_or(&[])
                .iter(),
        )
        .cloned()
        .collect();
    let (recipe_id, ghost_hovered) = crate::ui::container::push_recipe_entries(
        elements,
        &panel,
        recipe_book,
        cursor,
        input,
        native_recipes,
        None,
        3,
        3,
        &recipe_items,
        grid,
        screen_w,
        screen_h,
        text_width_fn,
    );
    push_cursor_stack(elements, cursor, panel.scale, &shown_cursor);

    super::container::push_container_tooltip(
        elements,
        slots,
        hovered.filter(|_| !ghost_hovered),
        cursor_item,
        cursor,
        screen_w,
        screen_h,
        panel.scale,
        advanced_tooltips,
    );
    let (ops, clicked_outside) = if hidden || recipe_book.clicked_ui {
        *drag = None;
        (Vec::new(), false)
    } else {
        resolve_gesture(
            input,
            hovered,
            &panel,
            if recipe_book.hovered_ui {
                (panel.ox, panel.oy)
            } else {
                cursor
            },
            ContainerKind::CraftingTable,
            slots,
            cursor_item,
            drag,
            last_click,
        )
    };
    if (!ops.is_empty() || input.left_pressed || input.right_pressed)
        && !recipe_book.clicked_ui
        && hovered.is_some_and(|slot| slot <= 9)
    {
        recipe_book.ghost_recipe = None;
    }

    ContainerResult {
        clicked_outside,
        ops,
        button: None,
        recipe_id,
    }
}
