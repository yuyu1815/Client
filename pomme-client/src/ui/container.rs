//! Shared slot drawing and click/drag gesture pieces for inventory, chest,
//! merchant, mount, and other container screens.

use std::collections::HashMap;
use std::time::Instant;

use azalea_inventory::ItemStack;
use azalea_inventory::operations::{
    ClickOperation, CloneClick, PickupAllClick, PickupClick, QuickCraftClick, QuickCraftKind,
    QuickCraftStatus, QuickMoveClick, SwapClick, ThrowClick,
};

use super::common::{
    FONT_SIZE, SLOT_LABEL_COLOR, SLOT_SIZE, SLOT_STRIDE, WHITE, hit_test, push_gradient_overlay,
    push_item_icon, push_slot,
};
use crate::player::menu_click::{self, ContainerKind};
use crate::renderer::pipelines::menu_overlay::{MenuElement, SpriteId};

const DOUBLE_CLICK_MS: u128 = 250;

/// Active click-drag: which button, and the slots covered so far.
pub type DragState = (QuickCraftKind, Vec<u16>);

/// What a container screen's frame produced.
pub struct ContainerResult {
    pub clicked_outside: bool,
    /// Container-click operations to send this frame (usually 0-1; a drag
    /// release emits a start/add.../end sequence).
    pub ops: Vec<ClickOperation>,
    /// Menu button clicked this frame (`ServerboundContainerButtonClick`),
    /// e.g. an enchantment option.
    pub button: Option<u32>,
    pub recipe_id: Option<(u32, bool)>,
}

/// Input for a container screen this frame.
#[derive(Clone, Copy)]
pub struct ContainerInput {
    pub left_pressed: bool,
    pub right_pressed: bool,
    pub middle_pressed: bool,
    pub left_held: bool,
    pub right_held: bool,
    pub shift: bool,
    /// Hotbar digit pressed this frame (vanilla `keyHotbarSlots` swap).
    pub hotbar_swap: Option<u8>,
    /// F pressed: swap the hovered slot with the offhand.
    pub swap_offhand: bool,
    /// Q pressed: throw from the hovered slot.
    pub throw: bool,
    /// Ctrl held with Q: throw the whole stack.
    pub throw_all: bool,
}

/// The centered container panel's placement on screen.
pub struct Panel {
    pub scale: f32,
    pub ox: f32,
    pub oy: f32,
    pub w: f32,
    pub h: f32,
}

impl Panel {
    pub fn contains(&self, cursor: (f32, f32)) -> bool {
        hit_test(cursor, [self.ox, self.oy, self.w, self.h])
    }

    /// A dark, unshadowed menu label at GUI-unit position.
    pub fn label(&self, elements: &mut Vec<MenuElement>, x: f32, y: f32, text: &str) {
        elements.push(MenuElement::TextFlat {
            x: self.ox + x * self.scale,
            y: self.oy + y * self.scale,
            text: text.into(),
            scale: FONT_SIZE * self.scale,
            color: SLOT_LABEL_COLOR,
        });
    }

    /// An untinted sprite at a GUI-unit rectangle.
    pub fn image(
        &self,
        elements: &mut Vec<MenuElement>,
        sprite: SpriteId,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    ) {
        elements.push(MenuElement::Image {
            x: self.ox + x * self.scale,
            y: self.oy + y * self.scale,
            w: w * self.scale,
            h: h * self.scale,
            sprite,
            tint: WHITE,
        });
    }
}

/// The dimmed backdrop and the centered panel placement for a `panel_w` x
/// `panel_h` (GUI units) container, without a background sprite.
pub fn push_backdrop(
    elements: &mut Vec<MenuElement>,
    screen_w: f32,
    screen_h: f32,
    gs: f32,
    panel_w: f32,
    panel_h: f32,
) -> Panel {
    let scale = gs.min(screen_w / panel_w).min(screen_h / panel_h);
    let w = panel_w * scale;
    let h = panel_h * scale;
    let ox = (screen_w - w) / 2.0;
    let oy = (screen_h - h) / 2.0;

    push_gradient_overlay(
        elements,
        screen_w,
        screen_h,
        [0.0627, 0.0627, 0.0627, 0.7529],
        [0.0627, 0.0627, 0.0627, 0.8157],
    );

    Panel {
        scale,
        ox,
        oy,
        w,
        h,
    }
}

/// The dimmed backdrop and the centered 176 x `panel_h` container background
/// sprite.
pub fn push_panel(
    elements: &mut Vec<MenuElement>,
    screen_w: f32,
    screen_h: f32,
    gs: f32,
    panel_h: f32,
    sprite: SpriteId,
) -> Panel {
    let panel = push_backdrop(elements, screen_w, screen_h, gs, 176.0, panel_h);
    panel.image(elements, sprite, 0.0, 0.0, 176.0, panel_h);
    panel
}

/// A sprite scissored to a sub-rectangle, both `[x, y, w, h]` in GUI units.
pub fn push_clipped_sprite(
    elements: &mut Vec<MenuElement>,
    panel: &Panel,
    sprite: SpriteId,
    rect: [f32; 4],
    clip: [f32; 4],
) {
    let s = panel.scale;
    elements.push(MenuElement::ScissorPush {
        x: panel.ox + clip[0] * s,
        y: panel.oy + clip[1] * s,
        w: clip[2] * s,
        h: clip[3] * s,
    });
    panel.image(elements, sprite, rect[0], rect[1], rect[2], rect[3]);
    elements.push(MenuElement::ScissorPop);
}

/// Vanilla recipe-book sprites, collections, input and server-driven ghosts.
/// Returns the selected recipe and whether a drawn ghost slot is hovered.
#[allow(clippy::too_many_arguments)]
pub fn push_recipe_entries(
    elements: &mut Vec<MenuElement>,
    panel: &Panel,
    book: &mut crate::ui::recipe_book::RecipeBookState,
    cursor: (f32, f32),
    input: &ContainerInput,
    native: bool,
    furnace_variant: Option<crate::ui::furnace::FurnaceVariant>,
    columns: usize,
    rows: usize,
    available: &[ItemStack],
    grid: &[ItemStack],
    screen_w: f32,
    screen_h: f32,
    text_width_fn: &dyn Fn(&str, f32) -> f32,
) -> (Option<(u32, bool)>, bool) {
    if !native {
        return (None, false);
    }
    let cycle = book.cycle_index();
    let s = panel.scale;
    let narrow = screen_w / s < 379.0;
    let ghost_hovered = (!narrow || !book.open)
        && push_recipe_ghost(
            elements,
            panel,
            book,
            cursor,
            screen_w,
            screen_h,
            columns,
            rows,
            furnace_variant.is_some(),
            grid,
            cycle,
        );
    if !book.open {
        return (None, ghost_hovered);
    }
    let recipe_panel = recipe_book_panel(screen_w, screen_h, s);
    let r = [
        recipe_panel.ox,
        recipe_panel.oy,
        recipe_panel.w,
        recipe_panel.h,
    ];
    let rect = |x: f32, y: f32, w: f32, h: f32| [r[0] + x * s, r[1] + y * s, w * s, h * s];
    let clicked = input.left_pressed;
    let popup_was_open = book.popup.is_some();
    let press = input.left_pressed || input.right_pressed || input.middle_pressed;
    book.hovered_ui = hit_test(cursor, r);
    book.clicked_ui |= press && (narrow || book.hovered_ui || popup_was_open);
    recipe_panel.image(
        elements,
        SpriteId::RecipeBookBackground,
        0.0,
        0.0,
        147.0,
        166.0,
    );
    let search_hover = hit_test(cursor, rect(8.0, 13.0, 98.0, 14.0));
    let wf = |text: &str| text_width_fn(text, FONT_SIZE);
    if clicked && !popup_was_open {
        book.toggle_focused = false;
        if search_hover && !book.search_focused {
            book.search_field().set_focused(true);
        }
        book.search_focused = search_hover;
        book.search_dragging = search_hover;
        if search_hover {
            let pos = book
                .search_field()
                .pos_from_click((cursor.0 - r[0]) / s - 29.0, 73.0, &wf);
            book.search_field().on_click(pos, input.shift, 73.0, &wf);
        }
    }
    if book.search_dragging && input.left_held {
        let pos = book
            .search_field()
            .pos_from_click((cursor.0 - r[0]) / s - 29.0, 73.0, &wf);
        book.search_field().on_drag(pos, 73.0, &wf);
    } else if !input.left_held {
        book.search_dragging = false;
    }
    let focused = book.search_focused;
    recipe_panel.image(
        elements,
        if focused {
            SpriteId::RecipeSearchFieldHighlighted
        } else {
            SpriteId::RecipeSearchField
        },
        25.0,
        13.0,
        81.0,
        14.0,
    );
    let field = book.search_field();
    let info = field.render_info(73.0, focused, &wf);
    let shown = &field.value()[info.display_start..info.display_end];
    elements.push(MenuElement::ScissorPush {
        x: r[0] + 29.0 * s,
        y: r[1] + 13.0 * s,
        w: 73.0 * s,
        h: 14.0 * s,
    });
    // 26.2 EditBox is bordered: text x+4, y+(height-8)/2.
    super::common::push_field_text(
        elements,
        &info,
        shown,
        None,
        r[0] + 29.0 * s,
        r[1] + 16.0 * s,
        FONT_SIZE * s,
        s,
        s,
        WHITE,
        None,
        &|t| wf(t) * s,
    );
    if field.value().is_empty() && !focused {
        elements.push(MenuElement::TextFlat {
            x: r[0] + 29.0 * s,
            y: r[1] + 16.0 * s,
            text: crate::lang::translate("gui.recipebook.search_hint")
                .unwrap_or("Search...")
                .into(),
            scale: FONT_SIZE * s,
            color: super::common::rgb(0x808080),
        });
    }
    elements.push(MenuElement::ScissorPop);
    let filter_hover = hit_test(cursor, rect(110.0, 12.0, 26.0, 16.0));
    let filter_sprite = match (furnace_variant.is_some(), book.craftable_only, filter_hover) {
        (false, true, false) => SpriteId::RecipeFilterEnabled,
        (false, true, true) => SpriteId::RecipeFilterEnabledHighlighted,
        (false, false, false) => SpriteId::RecipeFilterDisabled,
        (false, false, true) => SpriteId::RecipeFilterDisabledHighlighted,
        (true, true, false) => SpriteId::FurnaceRecipeFilterEnabled,
        (true, true, true) => SpriteId::FurnaceRecipeFilterEnabledHighlighted,
        (true, false, false) => SpriteId::FurnaceRecipeFilterDisabled,
        (true, false, true) => SpriteId::FurnaceRecipeFilterDisabledHighlighted,
    };
    recipe_panel.image(elements, filter_sprite, 110.0, 12.0, 26.0, 16.0);
    if filter_hover && clicked && !popup_was_open {
        book.craftable_only = !book.craftable_only;
        book.settings_dirty = true;
        book.page = 0;
    }
    let mut categories: Vec<_> = book
        .categories
        .iter()
        .filter(|(id, _)| {
            book.displays.get(*id).is_some_and(|d| {
                crate::ui::recipe_book::display_fits(d, columns, rows, furnace_variant.is_some())
            })
        })
        .map(|(_, c)| *c)
        .filter(|c| match furnace_variant {
            Some(v) => furnace_category_matches(v, c),
            None => matches!(
                c,
                azalea_registry::builtin::RecipeBookCategory::CraftingEquipment
                    | azalea_registry::builtin::RecipeBookCategory::CraftingBuildingBlocks
                    | azalea_registry::builtin::RecipeBookCategory::CraftingMisc
                    | azalea_registry::builtin::RecipeBookCategory::CraftingRedstone
            ),
        })
        .collect();
    categories.sort_by_key(|c| match c {
        azalea_registry::builtin::RecipeBookCategory::FurnaceFood
        | azalea_registry::builtin::RecipeBookCategory::SmokerFood => 0,
        azalea_registry::builtin::RecipeBookCategory::FurnaceBlocks
        | azalea_registry::builtin::RecipeBookCategory::BlastFurnaceBlocks => 1,
        azalea_registry::builtin::RecipeBookCategory::FurnaceMisc
        | azalea_registry::builtin::RecipeBookCategory::BlastFurnaceMisc => 2,
        azalea_registry::builtin::RecipeBookCategory::CraftingEquipment => 0,
        azalea_registry::builtin::RecipeBookCategory::CraftingBuildingBlocks => 1,
        azalea_registry::builtin::RecipeBookCategory::CraftingMisc => 2,
        azalea_registry::builtin::RecipeBookCategory::CraftingRedstone => 3,
        _ => 4,
    });
    categories.dedup();
    if book.category.is_some_and(|c| !categories.contains(&c)) {
        book.category = None;
    }
    for (i, category) in std::iter::once(None)
        .chain(categories.iter().copied().map(Some))
        .take(5)
        .enumerate()
    {
        let selected = book.category == category;
        let tx = -30.0 + if selected { -2.0 } else { 0.0 };
        let ty = 3.0 + 27.0 * i as f32;
        recipe_panel.image(
            elements,
            if selected {
                SpriteId::RecipeBookTabSelected
            } else {
                SpriteId::RecipeBookTab
            },
            tx,
            ty,
            35.0,
            27.0,
        );
        let icons = recipe_tab_icons(category);
        for (i, icon) in icons.iter().enumerate() {
            push_recipe_icon(
                elements,
                &recipe_panel,
                tx + if icons.len() == 1 {
                    9.0
                } else {
                    3.0 + 11.0 * i as f32
                },
                ty + 5.0,
                16.0,
                &ItemStack::from(*icon),
            );
        }
        let tab_hover = hit_test(cursor, rect(-30.0, ty, 35.0, 27.0));
        book.hovered_ui |= tab_hover;
        book.clicked_ui |= tab_hover && press;
        if tab_hover && clicked && !popup_was_open {
            book.category = category;
            book.page = 0;
        }
    }
    let selected_categories = book.category.map(|c| vec![c]).unwrap_or(categories);
    let recipes = book.collections(
        &selected_categories,
        columns,
        rows,
        furnace_variant.is_some(),
        available,
    );
    let pages = recipes.len().div_ceil(20).max(1);
    book.page = book.page.min(pages - 1);
    for (forward, visible, px) in [
        (false, book.page > 0, 38.0),
        (true, book.page + 1 < pages, 93.0),
    ] {
        if !visible {
            continue;
        }
        let hovered = hit_test(cursor, rect(px, 137.0, 12.0, 17.0));
        let sprite = match (forward, hovered) {
            (false, false) => SpriteId::RecipePageBackward,
            (false, true) => SpriteId::RecipePageBackwardHighlighted,
            (true, false) => SpriteId::RecipePageForward,
            (true, true) => SpriteId::RecipePageForwardHighlighted,
        };
        recipe_panel.image(elements, sprite, px, 137.0, 12.0, 17.0);
        if hovered && clicked && !popup_was_open {
            if forward {
                book.page += 1;
            } else {
                book.page -= 1;
            }
        }
    }
    if pages > 1 {
        let text = format!("{} / {pages}", book.page + 1);
        elements.push(MenuElement::Text {
            x: r[0] + 73.0 * s,
            y: r[1] + 141.0 * s,
            text,
            scale: FONT_SIZE * s,
            color: WHITE,
            centered: true,
        });
    }
    let mut selected = None;
    for (index, ids) in recipes.iter().skip(book.page * 20).take(20).enumerate() {
        let gx = 11.0 + (index % 5) as f32 * 25.0;
        let gy = 31.0 + (index / 5) as f32 * 25.0;
        let craftable = ids
            .iter()
            .any(|id| book.can_craft(*id, available) == Some(true));
        let sprite = match (ids.len() > 1, craftable) {
            (false, true) => SpriteId::RecipeSlotCraftable,
            (false, false) => SpriteId::RecipeSlotUncraftable,
            (true, true) => SpriteId::RecipeSlotManyCraftable,
            (true, false) => SpriteId::RecipeSlotManyUncraftable,
        };
        recipe_panel.image(elements, sprite, gx, gy, 25.0, 25.0);
        let id = ids[cycle % ids.len()];
        let items = crate::ui::recipe_book::display_result(&book.displays[&id])
            .map(|d| book.resolve(d))
            .unwrap_or_default();
        let item = items.get((cycle / ids.len()) % items.len().max(1));
        if let Some(item) = item {
            let same_result = ids.len() > 1
                && item.as_present().is_some_and(|first| {
                    ids.iter()
                        .filter_map(|id| crate::ui::recipe_book::display_result(&book.displays[id]))
                        .flat_map(|display| book.resolve(display))
                        .all(|stack| {
                            stack
                                .as_present()
                                .is_some_and(|stack| first.is_same_item_and_components(stack))
                        })
                });
            if same_result {
                push_recipe_icon(elements, &recipe_panel, gx + 5.0, gy + 5.0, 16.0, item);
            }
            let offset = if same_result { 3.0 } else { 4.0 };
            push_recipe_icon(
                elements,
                &recipe_panel,
                gx + offset,
                gy + offset,
                16.0,
                item,
            );
        }
        for id in ids {
            if let Some(flags) = book.flags.get_mut(id) {
                *flags &= !2;
            }
        }
        if hit_test(cursor, rect(gx, gy, 25.0, 25.0)) && !popup_was_open {
            if let Some(item) = item {
                push_recipe_tooltip(elements, item, cursor, screen_w, screen_h, s, ids.len() > 1);
            }
            if clicked {
                selected = Some((id, input.shift));
            }
            if input.right_pressed && ids.len() > 1 {
                let mut ids = ids.clone();
                ids.sort_by_key(|id| book.can_craft(*id, available) != Some(true));
                let cols = if ids.len() <= 16 { 4 } else { 5 };
                let px = gx
                    - (((gx + ids.len().min(cols) as f32 * 25.0 - 123.0) / 25.0)
                        .max(0.0)
                        .floor()
                        * 25.0);
                let py = gy
                    - (((gy + ids.len().div_ceil(cols) as f32 * 25.0 - 146.0) / 25.0)
                        .max(0.0)
                        .ceil()
                        * 25.0);
                book.popup = Some((ids, px, py.max(-4.0)));
            }
        }
    }
    if let Some((ids, px, py)) = &book.popup {
        let cols = if ids.len() <= 16 { 4 } else { 5 };
        recipe_panel.image(
            elements,
            SpriteId::RecipeOverlay,
            *px,
            *py,
            ids.len().min(cols) as f32 * 25.0 + 8.0,
            ids.len().div_ceil(cols) as f32 * 25.0 + 8.0,
        );
        for (i, id) in ids.iter().enumerate() {
            let gx = *px + 4.0 + (i % cols) as f32 * 25.0;
            let gy = *py + 5.0 + (i / cols) as f32 * 25.0;
            let hovered = hit_test(cursor, rect(gx, gy, 24.0, 24.0));
            let craftable = book.can_craft(*id, available) == Some(true);
            let sprite = match (furnace_variant.is_some(), craftable, hovered) {
                (false, true, false) => SpriteId::RecipeCraftingOverlay,
                (false, true, true) => SpriteId::RecipeCraftingOverlayHighlighted,
                (false, false, false) => SpriteId::RecipeCraftingOverlayDisabled,
                (false, false, true) => SpriteId::RecipeCraftingOverlayDisabledHighlighted,
                (true, true, false) => SpriteId::RecipeFurnaceOverlay,
                (true, true, true) => SpriteId::RecipeFurnaceOverlayHighlighted,
                (true, false, false) => SpriteId::RecipeFurnaceOverlayDisabled,
                (true, false, true) => SpriteId::RecipeFurnaceOverlayDisabledHighlighted,
            };
            recipe_panel.image(elements, sprite, gx, gy, 24.0, 24.0);
            for (index, d) in crate::ui::recipe_book::ingredient_positions(&book.displays[id], 3, 3)
            {
                let items = book.resolve(d);
                if let Some(item) = items.get(cycle % items.len().max(1)) {
                    push_recipe_icon(
                        elements,
                        &recipe_panel,
                        gx + 2.0 + (index % 3) as f32 * 7.0,
                        gy + 2.0 + (index / 3) as f32 * 7.0,
                        6.0,
                        item,
                    );
                }
            }
            if hovered && clicked && popup_was_open {
                selected = Some((*id, input.shift));
            }
        }
    }
    if popup_was_open && press {
        book.popup = None;
    }
    if selected.is_some() {
        book.ghost_recipe = None;
        book.popup = None;
        if narrow {
            book.open = false;
            book.settings_dirty = true;
            book.search_focused = false;
        }
    }
    (selected, ghost_hovered)
}

/// All book geometry uses GUI units at the container's existing scale.
pub fn recipe_book_panel(screen_w: f32, screen_h: f32, scale: f32) -> Panel {
    let width = screen_w / scale;
    Panel {
        scale,
        ox: (((width - 147.0) / 2.0).floor() - if width < 379.0 { 0.0 } else { 86.0 }) * scale,
        oy: ((screen_h / scale - 166.0) / 2.0).floor() * scale,
        w: 147.0 * scale,
        h: 166.0 * scale,
    }
}

fn recipe_container_x(screen_w: f32, scale: f32, open: bool) -> f32 {
    let width = screen_w / scale;
    if open && width >= 379.0 {
        (177.0 + ((width - 176.0 - 200.0) / 2.0).floor()) * scale
    } else {
        ((width - 176.0) / 2.0).floor() * scale
    }
}

/// Process the toggle before drawing/hit-testing so every caller moves
/// together.
#[allow(clippy::too_many_arguments)]
pub fn push_recipe_panel(
    elements: &mut Vec<MenuElement>,
    screen_w: f32,
    screen_h: f32,
    gs: f32,
    background: SpriteId,
    book: &mut crate::ui::recipe_book::RecipeBookState,
    native: bool,
    book_type: u32,
    cursor: (f32, f32),
    input: &ContainerInput,
    button: (f32, f32),
) -> (Panel, bool) {
    let mut panel = push_backdrop(elements, screen_w, screen_h, gs, 176.0, 166.0);
    panel.oy = ((screen_h / panel.scale - 166.0) / 2.0).floor() * panel.scale;
    book.clicked_ui = false;
    book.hovered_ui = false;
    if native {
        book.load_settings(book_type);
        panel.ox = recipe_container_x(screen_w, panel.scale, book.open);
        let hidden = book.open && screen_w / panel.scale < 379.0;
        let hovered = hit_test(
            cursor,
            [
                panel.ox + button.0 * panel.scale,
                panel.oy + button.1 * panel.scale,
                20.0 * panel.scale,
                18.0 * panel.scale,
            ],
        );
        if !hidden && hovered && input.left_pressed {
            book.open = !book.open;
            book.search_focused = false;
            book.toggle_focused = false;
            book.search_dragging = false;
            book.popup = None;
            book.settings_dirty = true;
            book.clicked_ui = true;
            panel.ox = recipe_container_x(screen_w, panel.scale, book.open);
        }
    }
    panel.image(elements, background, 0.0, 0.0, 176.0, 166.0);
    let hidden = native && book.open && screen_w / panel.scale < 379.0;
    if native && !hidden {
        let hover = hit_test(
            cursor,
            [
                panel.ox + button.0 * panel.scale,
                panel.oy + button.1 * panel.scale,
                20.0 * panel.scale,
                18.0 * panel.scale,
            ],
        );
        panel.image(
            elements,
            if hover || book.toggle_focused {
                SpriteId::RecipeBookButtonHighlighted
            } else {
                SpriteId::RecipeBookButton
            },
            button.0,
            button.1,
            20.0,
            18.0,
        );
    }
    (panel, hidden)
}

fn recipe_tab_icons(
    category: Option<azalea_registry::builtin::RecipeBookCategory>,
) -> Vec<azalea_registry::builtin::ItemKind> {
    use azalea_registry::builtin::{ItemKind as I, RecipeBookCategory as C};
    match category {
        None => vec![I::Compass],
        Some(C::CraftingEquipment) => vec![I::IronAxe, I::GoldenSword],
        Some(C::CraftingBuildingBlocks) => vec![I::Bricks],
        Some(C::CraftingMisc) => vec![I::LavaBucket, I::Apple],
        Some(C::CraftingRedstone) => vec![I::Redstone],
        Some(C::FurnaceFood | C::SmokerFood) => vec![I::Porkchop],
        Some(C::FurnaceBlocks) => vec![I::Stone],
        Some(C::FurnaceMisc) => vec![I::LavaBucket, I::Emerald],
        Some(C::BlastFurnaceBlocks) => vec![I::RedstoneOre],
        Some(C::BlastFurnaceMisc) => vec![I::IronShovel, I::GoldenLeggings],
        _ => Vec::new(),
    }
}

fn push_recipe_icon(
    elements: &mut Vec<MenuElement>,
    panel: &Panel,
    x: f32,
    y: f32,
    size: f32,
    item: &ItemStack,
) {
    if let Some(item) = item.as_present() {
        let mut icon = item.clone();
        icon.count = 1; // fakeItem: no list or ingredient count decorations.
        push_item_icon(
            elements,
            panel.ox + x * panel.scale,
            panel.oy + y * panel.scale,
            size * panel.scale,
            panel.scale,
            &icon,
        );
    }
}

fn push_recipe_tooltip(
    elements: &mut Vec<MenuElement>,
    item: &ItemStack,
    cursor: (f32, f32),
    screen_w: f32,
    screen_h: f32,
    scale: f32,
    many: bool,
) {
    if let Some(item) = item.as_present()
        && let Ok(value) = serde_json::to_value(item)
    {
        let mut lines = super::chat::item_tooltip_lines(&value, None, false);
        if many {
            lines.push(crate::renderer::pipelines::menu_overlay::TooltipLine::new(
                crate::lang::translate("gui.recipebook.moreRecipes")
                    .unwrap_or("Right click for more")
                    .into(),
                WHITE,
            ));
        }
        if !lines.is_empty() {
            super::common::push_tooltip_lines(elements, cursor, screen_w, screen_h, scale, lines);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn push_recipe_ghost(
    elements: &mut Vec<MenuElement>,
    panel: &Panel,
    book: &crate::ui::recipe_book::RecipeBookState,
    cursor: (f32, f32),
    screen_w: f32,
    screen_h: f32,
    columns: usize,
    rows: usize,
    furnace: bool,
    grid: &[ItemStack],
    cycle: usize,
) -> bool {
    use crate::ui::recipe_book::{display_fits, display_result, ingredient_positions};
    let Some(display) = &book.ghost_recipe else {
        return false;
    };
    if !display_fits(display, columns, rows, furnace) {
        return false;
    }
    let mut positions = Vec::new();
    if furnace {
        if let azalea_protocol::common::recipe::RecipeDisplayData::Furnace(d) = display {
            positions.push((56.0, 17.0, &d.ingredient, false));
            if !grid.get(1).is_some_and(ItemStack::is_present) {
                positions.push((56.0, 53.0, &d.fuel, false));
            }
        }
    } else {
        for (slot, d) in ingredient_positions(display, columns, rows) {
            let (x, y) = if columns == 2 {
                (98.0, 18.0)
            } else {
                (30.0, 17.0)
            };
            positions.push((
                x + (slot % columns) as f32 * 18.0,
                y + (slot / columns) as f32 * 18.0,
                d,
                false,
            ));
        }
    }
    if let Some(result) = display_result(display) {
        let (x, y) = if furnace {
            (116.0, 35.0)
        } else if columns == 2 {
            (154.0, 28.0)
        } else {
            (124.0, 35.0)
        };
        positions.push((x, y, result, true));
    }
    let mut hovered = false;
    for (x, y, d, result) in positions {
        let items = book.resolve(d);
        let Some(item) = items.get(cycle % items.len().max(1)) else {
            continue;
        };
        let s = panel.scale;
        let px = panel.ox + x * s;
        let py = panel.oy + y * s;
        let big = result && columns != 2;
        elements.push(MenuElement::Rect {
            x: px - if big { 4.0 * s } else { 0.0 },
            y: py - if big { 4.0 * s } else { 0.0 },
            w: if big { 24.0 * s } else { 16.0 * s },
            h: if big { 24.0 * s } else { 16.0 * s },
            corner_radius: 0.0,
            color: [1.0, 0.0, 0.0, 48.0 / 255.0],
        });
        push_recipe_icon(elements, panel, x, y, 16.0, item);
        elements.push(MenuElement::Rect {
            x: px,
            y: py,
            w: 16.0 * s,
            h: 16.0 * s,
            corner_radius: 0.0,
            color: [1.0, 1.0, 1.0, 48.0 / 255.0],
        });
        if result
            && let Some(stack) = item.as_present()
            && stack.count > 1
        {
            super::common::push_item_count(elements, px, py, 16.0 * s, s, stack.count);
        }
        if hit_test(cursor, [px, py, 16.0 * s, 16.0 * s]) {
            hovered = true;
            push_recipe_tooltip(elements, item, cursor, screen_w, screen_h, s, false);
        }
    }
    hovered
}

fn furnace_category_matches(
    variant: crate::ui::furnace::FurnaceVariant,
    category: &azalea_registry::builtin::RecipeBookCategory,
) -> bool {
    use azalea_registry::builtin::RecipeBookCategory as C;

    use crate::ui::furnace::FurnaceVariant as F;

    match variant {
        F::Furnace => matches!(category, C::FurnaceFood | C::FurnaceBlocks | C::FurnaceMisc),
        F::BlastFurnace => matches!(category, C::BlastFurnaceBlocks | C::BlastFurnaceMisc),
        F::Smoker => matches!(category, C::SmokerFood),
    }
}

/// Per-frame slot drawing context: positions slots in GUI units, substitutes
/// the live drag preview, and accumulates the hovered slot.
pub struct SlotCtx<'e> {
    elements: &'e mut Vec<MenuElement>,
    scale: f32,
    ox: f32,
    oy: f32,
    cursor: (f32, f32),
    /// What each drag-covered slot would receive and the remainder left on the
    /// cursor. Read-only; the real change happens on release.
    preview: Option<(HashMap<u16, ItemStack>, ItemStack)>,
    hovered: Option<u16>,
    hidden: bool,
}

impl<'e> SlotCtx<'e> {
    pub fn new(
        elements: &'e mut Vec<MenuElement>,
        panel: &Panel,
        cursor: (f32, f32),
        kind: ContainerKind,
        slots: &[ItemStack],
        cursor_item: &ItemStack,
        drag: &Option<DragState>,
    ) -> Self {
        let preview = drag.as_ref().map(|(drag_kind, covered)| {
            let (changed, remainder) =
                menu_click::drag_distribution(kind, slots, cursor_item, drag_kind, covered);
            (changed.into_iter().collect(), remainder)
        });
        Self {
            elements,
            scale: panel.scale,
            ox: panel.ox,
            oy: panel.oy,
            cursor,
            preview,
            hovered: None,
            hidden: false,
        }
    }

    pub fn set_hidden(&mut self, hidden: bool) {
        self.hidden = hidden;
        if hidden {
            self.preview = None;
        }
    }

    /// Draws a slot at GUI-unit position (x, y), recording it as hovered when
    /// the cursor is over it.
    pub fn slot(&mut self, x: f32, y: f32, item: &ItemStack, empty: Option<SpriteId>, num: u16) {
        if self.hidden {
            return;
        }
        let shown = self
            .preview
            .as_ref()
            .and_then(|(m, _)| m.get(&num))
            .unwrap_or(item);
        let px = self.ox + x * self.scale;
        let py = self.oy + y * self.scale;
        let size = SLOT_SIZE * self.scale;
        if push_slot(
            self.elements,
            px,
            py,
            size,
            self.scale,
            self.cursor,
            shown,
            empty,
        ) {
            self.hovered = self.hovered.or(Some(num));
        }
    }

    /// The three main-inventory rows starting at GUI-unit `main_y` and the
    /// hotbar 58 below, reading container slots starting at the given bases.
    pub fn player_rows(
        &mut self,
        slots: &[ItemStack],
        main_base: u16,
        hotbar_base: u16,
        main_y: f32,
    ) {
        for row in 0..3u16 {
            for col in 0..9u16 {
                let num = main_base + row * 9 + col;
                let item = slots.get(num as usize).unwrap_or(&ItemStack::Empty);
                self.slot(
                    8.0 + col as f32 * SLOT_STRIDE,
                    main_y + row as f32 * SLOT_STRIDE,
                    item,
                    None,
                    num,
                );
            }
        }
        for col in 0..9u16 {
            let num = hotbar_base + col;
            let item = slots.get(num as usize).unwrap_or(&ItemStack::Empty);
            self.slot(
                8.0 + col as f32 * SLOT_STRIDE,
                main_y + 58.0,
                item,
                None,
                num,
            );
        }
    }

    /// Ends slot drawing: the hovered slot, and the stack that should ride the
    /// cursor (the un-distributed drag remainder while dragging).
    pub fn finish(self, cursor_item: &ItemStack) -> (Option<u16>, ItemStack) {
        let shown = self
            .preview
            .map(|(_, r)| r)
            .unwrap_or_else(|| cursor_item.clone());
        (self.hovered, shown)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn push_container_tooltip(
    elements: &mut Vec<MenuElement>,
    slots: &[ItemStack],
    hovered: Option<u16>,
    cursor_item: &ItemStack,
    cursor: (f32, f32),
    screen_w: f32,
    screen_h: f32,
    scale: f32,
    advanced: bool,
) {
    if cursor_item.is_empty()
        && let Some(item) = hovered
            .and_then(|slot| slots.get(slot as usize))
            .and_then(ItemStack::as_present)
        && let Ok(value) = serde_json::to_value(item)
    {
        let lines = super::chat::item_tooltip_lines(&value, None, advanced);
        if !lines.is_empty() {
            super::common::push_tooltip_lines(elements, cursor, screen_w, screen_h, scale, lines);
        }
    }
}

/// The carried stack rides the cursor, on top of everything.
pub fn push_cursor_stack(
    elements: &mut Vec<MenuElement>,
    cursor: (f32, f32),
    scale: f32,
    item: &ItemStack,
) {
    if let ItemStack::Present(data) = item {
        let size = SLOT_SIZE * scale;
        push_item_icon(
            elements,
            cursor.0 - size / 2.0,
            cursor.1 - size / 2.0,
            size,
            scale,
            data,
        );
    }
}

/// Keyboard shortcuts on the hovered slot, vanilla
/// `AbstractContainerScreen.keyPressed`: with an empty cursor, F / 1-9 swap it
/// with the offhand / a hotbar slot; over an item, middle-click CLONEs and
/// Q THROWs (Ctrl = whole stack).
fn resolve_key_ops(
    input: &ContainerInput,
    hovered: Option<u16>,
    slots: &[ItemStack],
    cursor_item: &ItemStack,
) -> Vec<ClickOperation> {
    let mut ops = Vec::new();
    let Some(slot) = hovered else {
        return ops;
    };
    if cursor_item.is_empty() {
        if input.swap_offhand {
            ops.push(ClickOperation::Swap(SwapClick {
                source_slot: slot,
                target_slot: 40,
            }));
        } else if let Some(i) = input.hotbar_swap {
            ops.push(ClickOperation::Swap(SwapClick {
                source_slot: slot,
                target_slot: i,
            }));
        }
    }
    if slots.get(slot as usize).is_some_and(ItemStack::is_present) {
        if input.middle_pressed {
            ops.push(ClickOperation::Clone(CloneClick { slot }));
        } else if input.throw {
            ops.push(ClickOperation::Throw(if input.throw_all {
                ThrowClick::All { slot }
            } else {
                ThrowClick::Single { slot }
            }));
        }
    }
    ops
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idle_input() -> ContainerInput {
        ContainerInput {
            left_pressed: false,
            right_pressed: false,
            middle_pressed: false,
            left_held: false,
            right_held: false,
            shift: false,
            hotbar_swap: None,
            swap_offhand: false,
            throw: false,
            throw_all: false,
        }
    }

    fn open_book() -> crate::ui::recipe_book::RecipeBookState {
        let mut book = crate::ui::recipe_book::RecipeBookState::default();
        book.open = true;
        book.settings_type = Some(0);
        book.settings_loaded = true;
        book
    }

    #[test]
    fn recipe_layout_uses_vanilla_sprites_and_slot_hit_mapping_at_wide_narrow_and_scaled_sizes() {
        use crate::player::inventory::Inventory;
        for s in [1.0f32, 2.0, 3.0] {
            for width in [378.0f32, 379.0, 640.0] {
                let sw = width * s;
                let sh = 360.0 * s;
                let mut book = open_book();
                let mut elements = Vec::new();
                let mut input = idle_input();
                input.left_pressed = true;
                input.shift = true;
                let x = if width < 379.0 {
                    ((width - 176.0) / 2.0).floor()
                } else {
                    177.0 + ((width - 376.0) / 2.0).floor()
                };
                let y = 97.0;
                let result = crate::ui::inventory::build_inventory(
                    &mut elements,
                    sw,
                    sh,
                    ((x + 16.0) * s, (y + 92.0) * s),
                    &input,
                    &Inventory::new(),
                    &ItemStack::Empty,
                    &mut None,
                    &mut None,
                    s,
                    &mut book,
                    true,
                    false,
                    &|t, size| t.len() as f32 * size * 0.75,
                );
                let bp = recipe_book_panel(sw, sh, s);
                assert!(elements.iter().any(|e|matches!(e,MenuElement::Image {x,y,w,h,sprite:SpriteId::RecipeBookBackground,..}
                    if *x==bp.ox && *y==bp.oy && *w==147.0*s && *h==166.0*s)));
                assert!(elements.iter().any(|e|matches!(e,MenuElement::Image {x,y,w,h,sprite:SpriteId::RecipeBookTabSelected,..}
                    if *x==bp.ox-32.0*s && *y==bp.oy+3.0*s && *w==35.0*s && *h==27.0*s)));
                assert!(elements.iter().any(
                    |e| matches!(e,MenuElement::Image {x,y,sprite:SpriteId::RecipeSearchField,..}
                    if *x==bp.ox+25.0*s && *y==bp.oy+13.0*s)
                ));
                if width < 379.0 {
                    assert!(result.ops.is_empty());
                    assert!(result.player_preview.is_none());
                    assert!(!elements.iter().any(|e| matches!(
                        e,
                        MenuElement::Image {
                            sprite: SpriteId::RecipeBookButton
                                | SpriteId::RecipeBookButtonHighlighted
                                | SpriteId::SlotHighlightFront,
                            ..
                        }
                    )));
                } else {
                    assert!(matches!(
                        result.ops.as_slice(),
                        [ClickOperation::QuickMove(QuickMoveClick::Left { slot: 9 })]
                    ));
                    assert_eq!(result.player_preview.unwrap().rect[0], (x + 26.0) * s);
                    assert!(elements.iter().any(|e|matches!(e,MenuElement::Image {w,h,sprite:SpriteId::RecipeBookButton,..} if *w==20.0*s && *h==18.0*s)));
                }
            }
        }
    }

    #[test]
    fn toggle_hover_focus_and_20px_hit_box_do_not_depend_on_open_state() {
        let mut book = open_book();
        book.open = false;
        let mut elements = Vec::new();
        let mut input = idle_input();
        // Closed container x=232; click the final two pixels of the 20-wide button.
        let cursor = (232.0 + 104.0 + 19.0, 97.0 + 61.0 + 9.0);
        push_recipe_panel(
            &mut elements,
            640.0,
            360.0,
            1.0,
            SpriteId::InventoryBackground,
            &mut book,
            true,
            0,
            cursor,
            &input,
            (104.0, 61.0),
        );
        assert!(elements.iter().any(|e| matches!(
            e,
            MenuElement::Image {
                sprite: SpriteId::RecipeBookButtonHighlighted,
                ..
            }
        )));
        input.left_pressed = true;
        push_recipe_panel(
            &mut Vec::new(),
            640.0,
            360.0,
            1.0,
            SpriteId::InventoryBackground,
            &mut book,
            true,
            0,
            cursor,
            &input,
            (104.0, 61.0),
        );
        assert!(book.open && book.clicked_ui);
        input.left_pressed = false;
        book.toggle_focused = true;
        elements.clear();
        push_recipe_panel(
            &mut elements,
            640.0,
            360.0,
            1.0,
            SpriteId::InventoryBackground,
            &mut book,
            true,
            0,
            (-1.0, -1.0),
            &input,
            (104.0, 61.0),
        );
        assert!(elements.iter().any(|e| matches!(
            e,
            MenuElement::Image {
                sprite: SpriteId::RecipeBookButtonHighlighted,
                ..
            }
        )));
        book.toggle_focused = false;
        elements.clear();
        push_recipe_panel(
            &mut elements,
            640.0,
            360.0,
            1.0,
            SpriteId::InventoryBackground,
            &mut book,
            true,
            0,
            (-1.0, -1.0),
            &input,
            (104.0, 61.0),
        );
        assert!(elements.iter().any(|e| matches!(
            e,
            MenuElement::Image {
                sprite: SpriteId::RecipeBookButton,
                ..
            }
        )));
    }

    #[test]
    fn recipe_container_slots_move_and_drag_release_over_book_never_drops_carried_items() {
        use azalea_registry::builtin::ItemKind;
        let mut input = idle_input();
        input.left_pressed = true;
        input.shift = true;
        for width in [378.0, 640.0] {
            let x = if width < 379.0 { 101.0 } else { 309.0 };
            let mut book = open_book();
            let crafting = crate::ui::crafting_table::build_crafting_table(
                &mut Vec::new(),
                width,
                360.0,
                (x + 38.0, 97.0 + 25.0),
                &input,
                &vec![ItemStack::Empty; 46],
                "Crafting",
                &ItemStack::Empty,
                &mut None,
                &mut None,
                1.0,
                &mut book,
                true,
                &|t, size| t.len() as f32 * size,
                false,
            );
            let mut book = open_book();
            book.settings_type = Some(1);
            let furnace = crate::ui::furnace::build_furnace(
                &mut Vec::new(),
                width,
                360.0,
                (x + 64.0, 97.0 + 25.0),
                &input,
                crate::ui::furnace::FurnaceVariant::Furnace,
                &vec![ItemStack::Empty; 39],
                &[0; 4],
                "Furnace",
                &ItemStack::Empty,
                &mut None,
                &mut None,
                1.0,
                &|t, size| t.len() as f32 * size,
                &mut book,
                true,
                false,
            );
            if width < 379.0 {
                assert!(crafting.ops.is_empty() && furnace.ops.is_empty());
            } else {
                assert!(matches!(
                    crafting.ops.as_slice(),
                    [ClickOperation::QuickMove(QuickMoveClick::Left { slot: 1 })]
                ));
                assert!(matches!(
                    furnace.ops.as_slice(),
                    [ClickOperation::QuickMove(QuickMoveClick::Left { slot: 0 })]
                ));
            }
        }
        let mut book = open_book();
        let mut drag = Some((QuickCraftKind::Left, vec![]));
        let result = crate::ui::inventory::build_inventory(
            &mut Vec::new(),
            640.0,
            360.0,
            (170.0, 120.0),
            &idle_input(),
            &crate::player::inventory::Inventory::new(),
            &ItemStack::from(ItemKind::Stone),
            &mut drag,
            &mut None,
            1.0,
            &mut book,
            true,
            false,
            &|t, size| t.len() as f32 * size,
        );
        assert!(result.ops.is_empty());
        assert!(drag.is_none());
    }

    #[test]
    fn recipe_collection_slots_are_25px_with_normal_16px_icons_and_popup_selects_server_ids() {
        use azalea_protocol::common::recipe::*;
        use azalea_registry::builtin::{ItemKind, RecipeBookCategory};
        let mut book = open_book();
        for id in [10, 11] {
            book.displays.insert(
                id,
                RecipeDisplayData::Shapeless(ShapelessCraftingRecipeDisplay {
                    ingredients: vec![SlotDisplayData::Item(ItemSlotDisplay {
                        item: ItemKind::Stone,
                    })],
                    result: SlotDisplayData::ItemStack(ItemStackSlotDisplay {
                        stack: ItemStack::new(ItemKind::Stone, 4),
                    }),
                    crafting_station: SlotDisplayData::Empty,
                }),
            );
            book.groups.insert(id, 1);
            book.categories.insert(id, RecipeBookCategory::CraftingMisc);
        }
        if let Some(RecipeDisplayData::Shapeless(d)) = book.displays.get_mut(&11) {
            d.result = SlotDisplayData::ItemStack(ItemStackSlotDisplay {
                stack: ItemStack::new(ItemKind::Cobblestone, 4),
            });
        }
        let panel = Panel {
            scale: 2.0,
            ox: 618.0,
            oy: 194.0,
            w: 352.0,
            h: 332.0,
        };
        let bp = recipe_book_panel(1280.0, 720.0, 2.0);
        let mut elements = Vec::new();
        let mut input = idle_input();
        input.right_pressed = true;
        // Hit the slot's right edge, outside its item icon.
        let click = (bp.ox + 35.0 * 2.0, bp.oy + 55.0 * 2.0);
        assert!(
            push_recipe_entries(
                &mut elements,
                &panel,
                &mut book,
                click,
                &input,
                true,
                None,
                2,
                2,
                &[],
                &[],
                1280.0,
                720.0,
                &|t, size| t.len() as f32 * size
            )
            .0
            .is_none()
        );
        assert!(book.popup.is_some());
        assert!(book.clicked_ui);
        assert!(elements.iter().any(|e|matches!(e,MenuElement::Image {x,y,w,h,sprite:SpriteId::RecipeSlotManyUncraftable,..}
            if *x==bp.ox+22.0 && *y==bp.oy+62.0 && *w==50.0 && *h==50.0)));
        assert!(elements.iter().any(|e|matches!(e,MenuElement::ItemIcon {x,y,w,h,item_name,tint,..}
            if item_name=="stone" && *x==bp.ox+30.0 && *y==bp.oy+70.0 && *w==32.0 && *h==32.0 && *tint==WHITE)));
        assert!(!elements.iter().any(
            |e| matches!(e,MenuElement::TextFlat {text,..}|MenuElement::Text {text,..} if text=="4")
        ));
        let (_, px, py) = book.popup.as_ref().unwrap();
        let click = (
            bp.ox + (*px + 4.0 + 25.0 + 12.0) * 2.0,
            bp.oy + (*py + 5.0 + 12.0) * 2.0,
        );
        input.right_pressed = false;
        input.left_pressed = true;
        assert_eq!(
            push_recipe_entries(
                &mut Vec::new(),
                &panel,
                &mut book,
                click,
                &input,
                true,
                None,
                2,
                2,
                &[],
                &[],
                1280.0,
                720.0,
                &|t, size| t.len() as f32 * size
            ),
            (Some((11, false)), false)
        );
        assert!(book.ghost_recipe.is_none());
        if let Some(RecipeDisplayData::Shapeless(d)) = book.displays.get_mut(&11) {
            d.result = SlotDisplayData::ItemStack(ItemStackSlotDisplay {
                stack: ItemStack::new(ItemKind::Stone, 4),
            });
        }
        input.left_pressed = false;
        elements.clear();
        push_recipe_entries(
            &mut elements,
            &panel,
            &mut book,
            (-1.0, -1.0),
            &input,
            true,
            None,
            2,
            2,
            &[],
            &[],
            1280.0,
            720.0,
            &|t, size| t.len() as f32 * size,
        );
        for offset in [3.0, 5.0] {
            assert!(elements.iter().any(|e|matches!(e,MenuElement::ItemIcon {x,y,w,h,item_name,..}
                if item_name=="stone" && *x==bp.ox+(11.0+offset)*2.0 && *y==bp.oy+(31.0+offset)*2.0 && *w==32.0 && *h==32.0)));
        }
    }

    #[test]
    fn furnace_server_ghost_cycles_candidates_draws_opaque_icons_overlays_and_result_count() {
        use azalea_protocol::common::recipe::*;
        use azalea_registry::builtin::ItemKind;
        let mut book = crate::ui::recipe_book::RecipeBookState::default();
        let item = |kind| SlotDisplayData::Item(ItemSlotDisplay { item: kind });
        book.receive_ghost(
            8,
            8,
            RecipeDisplayData::Furnace(FurnaceRecipeDisplay {
                ingredient: SlotDisplayData::Composite(CompositeSlotDisplay {
                    contents: vec![item(ItemKind::IronOre), item(ItemKind::RawIron)],
                }),
                fuel: SlotDisplayData::AnyFuel,
                result: SlotDisplayData::ItemStack(ItemStackSlotDisplay {
                    stack: ItemStack::new(ItemKind::IronIngot, 4),
                }),
                crafting_station: SlotDisplayData::Empty,
                duration: 200,
                experience: 0.7,
            }),
        );
        let panel = Panel {
            scale: 2.0,
            ox: 0.0,
            oy: 0.0,
            w: 352.0,
            h: 332.0,
        };
        let mut elements = Vec::new();
        push_recipe_ghost(
            &mut elements,
            &panel,
            &book,
            (-1.0, -1.0),
            640.0,
            360.0,
            1,
            1,
            true,
            &[ItemStack::Empty, ItemStack::Empty],
            1,
        );
        assert!(elements.iter().any(|e|matches!(e,MenuElement::ItemIcon {x,y,item_name,tint,..} if *x==112.0 && *y==34.0 && item_name=="raw_iron" && *tint==WHITE)));
        assert!(
            elements
                .iter()
                .any(|e| matches!(e,MenuElement::ItemIcon {x,y,..} if *x==112.0 && *y==106.0))
        );
        assert!(elements.iter().any(|e|matches!(e,MenuElement::Rect {x,y,w,h,color,..} if *x==224.0 && *y==62.0 && *w==48.0 && *h==48.0 && *color==[1.0,0.0,0.0,48.0/255.0])));
        assert!(elements.iter().any(|e|matches!(e,MenuElement::Rect {x,y,color,..} if *x==232.0 && *y==70.0 && *color==[1.0,1.0,1.0,48.0/255.0])));
        assert!(
            elements
                .iter()
                .any(|e| matches!(e,MenuElement::Text {text,..} if text=="4"))
        );
        elements.clear();
        push_recipe_ghost(
            &mut elements,
            &panel,
            &book,
            (-1.0, -1.0),
            640.0,
            360.0,
            1,
            1,
            true,
            &[ItemStack::Empty, ItemStack::from(ItemKind::Coal)],
            1,
        );
        assert!(
            !elements
                .iter()
                .any(|e| matches!(e,MenuElement::ItemIcon {x,y,..} if *x==112.0 && *y==106.0))
        );
    }

    #[test]
    fn ghost_tooltip_only_overrides_drawn_slots_in_all_recipe_screens() {
        use azalea_protocol::common::recipe::*;
        use azalea_registry::builtin::ItemKind;

        use crate::player::inventory::Inventory;
        use crate::ui::{crafting_table, furnace, inventory};

        let item = |kind| SlotDisplayData::Item(ItemSlotDisplay { item: kind });
        let slots = vec![ItemStack::from(ItemKind::Coal); 46];
        let mut inventory = Inventory::new();
        inventory.set_contents(slots.clone());
        let width = |t: &str, size| t.len() as f32 * size;
        for scale in [1.0, 2.0] {
            for screen in 0..3 {
                let (display, ingredient, uncovered, result) = match screen {
                    0 => (
                        RecipeDisplayData::Furnace(FurnaceRecipeDisplay {
                            ingredient: item(ItemKind::Stone),
                            fuel: SlotDisplayData::AnyFuel,
                            result: item(ItemKind::IronIngot),
                            crafting_station: SlotDisplayData::Empty,
                            duration: 200,
                            experience: 0.7,
                        }),
                        (56.0, 17.0),
                        (56.0, 53.0), // Occupied fuel is not a ghost slot.
                        (116.0, 35.0),
                    ),
                    _ => (
                        RecipeDisplayData::Shaped(ShapedCraftingRecipeDisplay {
                            width: 1,
                            height: 1,
                            ingredients: vec![item(ItemKind::Stone)],
                            result: item(ItemKind::IronIngot),
                            crafting_station: SlotDisplayData::Empty,
                        }),
                        if screen == 1 {
                            (98.0, 18.0)
                        } else {
                            (48.0, 35.0)
                        },
                        if screen == 1 {
                            (116.0, 18.0)
                        } else {
                            (30.0, 17.0)
                        },
                        if screen == 1 {
                            (154.0, 28.0)
                        } else {
                            (124.0, 35.0)
                        },
                    ),
                };
                for native in [true, false] {
                    for (position, expected) in [
                        (ingredient, ItemKind::Stone),
                        (uncovered, ItemKind::Coal),
                        (result, ItemKind::IronIngot),
                    ] {
                        let mut book = crate::ui::recipe_book::RecipeBookState::default();
                        book.receive_ghost(0, 0, display.clone());
                        let cursor = (
                            (232.0 + position.0 + 8.0) * scale,
                            (97.0 + position.1 + 8.0) * scale,
                        );
                        let mut elements = Vec::new();
                        match screen {
                            0 => {
                                furnace::build_furnace(
                                    &mut elements,
                                    640.0 * scale,
                                    360.0 * scale,
                                    cursor,
                                    &idle_input(),
                                    furnace::FurnaceVariant::Furnace,
                                    &slots,
                                    &[0; 4],
                                    "Furnace",
                                    &ItemStack::Empty,
                                    &mut None,
                                    &mut None,
                                    scale,
                                    &width,
                                    &mut book,
                                    native,
                                    false,
                                );
                            }
                            1 => {
                                inventory::build_inventory(
                                    &mut elements,
                                    640.0 * scale,
                                    360.0 * scale,
                                    cursor,
                                    &idle_input(),
                                    &inventory,
                                    &ItemStack::Empty,
                                    &mut None,
                                    &mut None,
                                    scale,
                                    &mut book,
                                    native,
                                    false,
                                    &width,
                                );
                            }
                            _ => {
                                crafting_table::build_crafting_table(
                                    &mut elements,
                                    640.0 * scale,
                                    360.0 * scale,
                                    cursor,
                                    &idle_input(),
                                    &slots,
                                    "Crafting",
                                    &ItemStack::Empty,
                                    &mut None,
                                    &mut None,
                                    scale,
                                    &mut book,
                                    native,
                                    &width,
                                    false,
                                );
                            }
                        }
                        let tooltips: Vec<_> = elements
                            .iter()
                            .filter_map(|e| match e {
                                MenuElement::TooltipLines { lines, .. } => Some(lines),
                                _ => None,
                            })
                            .collect();
                        assert_eq!(
                            tooltips.len(),
                            1,
                            "screen={screen}, native={native}, position={position:?}"
                        );
                        let expected =
                            ItemStack::from(if native { expected } else { ItemKind::Coal });
                        assert_eq!(
                            tooltips[0][0]
                                .spans
                                .iter()
                                .map(|span| span.text.as_str())
                                .collect::<String>(),
                            super::super::common::item_display_name(expected.as_present().unwrap()),
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn furnace_recipe_categories_match_the_screen_variant() {
        use azalea_registry::builtin::RecipeBookCategory as C;

        use crate::ui::furnace::FurnaceVariant as F;

        assert!(furnace_category_matches(F::Furnace, &C::FurnaceMisc));
        assert!(furnace_category_matches(
            F::BlastFurnace,
            &C::BlastFurnaceBlocks
        ));
        assert!(furnace_category_matches(F::Smoker, &C::SmokerFood));
        assert!(!furnace_category_matches(F::Smoker, &C::FurnaceFood));
    }

    #[test]
    fn empty_handed_outside_click_does_not_close_container() {
        let input = ContainerInput {
            left_pressed: true,
            right_pressed: false,
            middle_pressed: false,
            left_held: false,
            right_held: false,
            shift: false,
            hotbar_swap: None,
            swap_offhand: false,
            throw: false,
            throw_all: false,
        };
        let panel = Panel {
            scale: 1.0,
            ox: 10.0,
            oy: 10.0,
            w: 176.0,
            h: 166.0,
        };
        let (ops, close) = resolve_gesture(
            &input,
            None,
            &panel,
            (0.0, 0.0),
            ContainerKind::Chest { rows: 3 },
            &[],
            &ItemStack::Empty,
            &mut None,
            &mut None,
        );

        assert!(!close);
        assert!(matches!(
            ops.as_slice(),
            [ClickOperation::Pickup(PickupClick::Left { slot: None })]
        ));
    }
}

/// Turns this frame's input + hover into container-click operations, driving
/// the drag state machine. The server applies and resyncs, so no local
/// prediction. Returns the ops and whether the menu should close.
#[allow(clippy::too_many_arguments)]
pub fn resolve_gesture(
    input: &ContainerInput,
    hovered: Option<u16>,
    panel: &Panel,
    cursor: (f32, f32),
    kind: ContainerKind,
    slots: &[ItemStack],
    cursor_item: &ItemStack,
    drag: &mut Option<DragState>,
    last_click: &mut Option<(u16, Instant)>,
) -> (Vec<ClickOperation>, bool) {
    let carrying = cursor_item.is_present();
    let outside = !panel.contains(cursor);
    let mut ops = resolve_key_ops(input, hovered, slots, cursor_item);

    if let Some((drag_kind, covered)) = drag {
        let held = matches!(
            (&drag_kind, input.left_held, input.right_held),
            (QuickCraftKind::Left, true, _) | (QuickCraftKind::Right, _, true)
        );
        if held {
            // Match vanilla's accumulation so our slot set (and split) equals the
            // server's: only eligible slots, and only while items remain to share.
            if let Some(slot) = hovered
                && !covered.contains(&slot)
                && (cursor_item.count() as usize) > covered.len()
                && menu_click::drag_slot_eligible(kind, slots, cursor_item, slot)
            {
                covered.push(slot);
            }
            return (ops, false);
        }
        // Released: distribute across 2+ slots; one covered slot converts to a
        // normal click (vanilla quickCraftToSlots), none falls back to a click
        // wherever the cursor is now (vanilla mouseReleased, -999 outside).
        let drag_kind = drag_kind.clone();
        let covered = std::mem::take(covered);
        *drag = None;
        if covered.len() >= 2 {
            ops.push(quick_craft(&drag_kind, QuickCraftStatus::Start));
            for s in covered {
                ops.push(quick_craft(&drag_kind, QuickCraftStatus::Add { slot: s }));
            }
            ops.push(quick_craft(&drag_kind, QuickCraftStatus::End));
        } else if let Some(&s) = covered.first() {
            ops.push(pickup(&drag_kind, Some(s)));
        } else if carrying {
            // Vanilla only falls back to a click while still carrying.
            if let Some(s) = hovered {
                ops.push(pickup(&drag_kind, Some(s)));
            } else if outside {
                ops.push(pickup(&drag_kind, None));
            }
        }
        return (ops, false);
    }

    if !(input.left_pressed || input.right_pressed) {
        return (ops, false);
    }
    let click_kind = if input.left_pressed {
        QuickCraftKind::Left
    } else {
        QuickCraftKind::Right
    };

    if outside {
        // Vanilla sends PICKUP at slot -999; an empty cursor makes it a no-op.
        ops.push(pickup(&click_kind, None));
        return (ops, false);
    }

    let Some(slot) = hovered else {
        // Panel background: no-op, but a carrying press still enters the drag
        // state machine like vanilla (with no slots covered yet).
        if carrying {
            *drag = Some((click_kind, Vec::new()));
        }
        return (ops, false);
    };

    // Timing-based like vanilla; the server only gathers if it has a cursor item
    // (avoids depending on the round-trip-lagged local carried state).
    let double = input.left_pressed
        && matches!(last_click, Some((s, t)) if *s == slot && t.elapsed().as_millis() <= DOUBLE_CLICK_MS);

    if input.shift {
        ops.push(ClickOperation::QuickMove(match click_kind {
            QuickCraftKind::Left => QuickMoveClick::Left { slot },
            _ => QuickMoveClick::Right { slot },
        }));
    } else if double {
        ops.push(ClickOperation::PickupAll(PickupAllClick {
            slot,
            reversed: false,
        }));
        *last_click = None;
    } else {
        if carrying {
            // Start a drag; only an eligible slot joins the covered set (vanilla
            // gates every quick-craft slot on mayPlace). A single-slot or empty
            // set resolves to a normal click on release.
            let covered = if menu_click::drag_slot_eligible(kind, slots, cursor_item, slot) {
                vec![slot]
            } else {
                Vec::new()
            };
            *drag = Some((click_kind, covered));
        } else {
            ops.push(pickup(&click_kind, Some(slot)));
        }
        if input.left_pressed {
            *last_click = Some((slot, Instant::now()));
        }
    }
    (ops, false)
}

fn pickup(kind: &QuickCraftKind, slot: Option<u16>) -> ClickOperation {
    ClickOperation::Pickup(match kind {
        QuickCraftKind::Left => PickupClick::Left { slot },
        _ => PickupClick::Right { slot },
    })
}

fn quick_craft(kind: &QuickCraftKind, status: QuickCraftStatus) -> ClickOperation {
    ClickOperation::QuickCraft(QuickCraftClick {
        kind: kind.clone(),
        status,
    })
}
