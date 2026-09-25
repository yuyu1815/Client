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

/// Draws supported recipe outputs and returns the server-issued display ID
/// clicked this frame. Unsupported/tag/composite outputs are intentionally
/// hidden.
pub fn push_recipe_entries(
    elements: &mut Vec<MenuElement>,
    panel: &Panel,
    book: &mut crate::ui::recipe_book::RecipeBookState,
    cursor: (f32, f32),
    clicked: bool,
    native: bool,
    furnace_variant: Option<crate::ui::furnace::FurnaceVariant>,
    use_max_items: bool,
    _columns: usize,
    _rows: usize,
    x: f32,
    y: f32,
) -> Option<(u32, bool)> {
    book.clicked_ui = false;
    if !native {
        panel.label(elements, x, y, "Recipe book unavailable");
        return None;
    }
    let book_type = match furnace_variant {
        None => 0,
        Some(crate::ui::furnace::FurnaceVariant::Furnace) => 1,
        Some(crate::ui::furnace::FurnaceVariant::BlastFurnace) => 2,
        Some(crate::ui::furnace::FurnaceVariant::Smoker) => 3,
    };
    book.load_settings(book_type);
    let button_rect = [
        panel.ox + x * panel.scale,
        panel.oy + y * panel.scale,
        18.0 * panel.scale,
        18.0 * panel.scale,
    ];
    let button_hover = crate::ui::common::hit_test(cursor, button_rect);
    panel.image(
        elements,
        if book.open {
            SpriteId::RecipeBookButtonHighlighted
        } else {
            SpriteId::RecipeBookButton
        },
        x,
        y,
        18.0,
        18.0,
    );
    if button_hover && clicked {
        book.clicked_ui = true;
        book.open = !book.open;
        book.search_focused = false;
        book.settings_dirty = true;
    }
    if !book.open {
        return None;
    }
    let bx = x - 86.0;
    let by = y - 16.0;
    let r = [
        panel.ox + bx * panel.scale,
        panel.oy + by * panel.scale,
        147.0 * panel.scale,
        166.0 * panel.scale,
    ];
    if clicked && crate::ui::common::hit_test(cursor, r) {
        book.clicked_ui = true;
    }
    elements.push(MenuElement::Rect {
        x: r[0],
        y: r[1],
        w: r[2],
        h: r[3],
        corner_radius: 0.0,
        color: [0.12, 0.12, 0.12, 0.96],
    });
    let search = [
        r[0] + 25.0 * panel.scale,
        r[1] + 13.0 * panel.scale,
        81.0 * panel.scale,
        14.0 * panel.scale,
    ];
    elements.push(MenuElement::Rect {
        x: search[0],
        y: search[1],
        w: search[2],
        h: search[3],
        corner_radius: 0.0,
        color: [0.04, 0.04, 0.04, 1.0],
    });
    let search_hover = crate::ui::common::hit_test(cursor, search);
    if clicked {
        book.search_focused = search_hover;
        book.clicked_ui |= search_hover;
    }
    panel.label(
        elements,
        bx + 27.0,
        by + 14.0,
        if book.search.is_empty() {
            "Search..."
        } else {
            &book.search
        },
    );
    let filter = [
        r[0] + 110.0 * panel.scale,
        r[1] + 12.0 * panel.scale,
        26.0 * panel.scale,
        16.0 * panel.scale,
    ];
    elements.push(MenuElement::Rect {
        x: filter[0],
        y: filter[1],
        w: filter[2],
        h: filter[3],
        corner_radius: 0.0,
        color: if book.craftable_only {
            [0.2, 0.65, 0.2, 1.0]
        } else {
            [0.3, 0.3, 0.3, 1.0]
        },
    });
    panel.label(
        elements,
        bx + 119.0,
        by + 15.0,
        if book.craftable_only { "✓" } else { "·" },
    );
    if crate::ui::common::hit_test(cursor, filter) && clicked {
        book.clicked_ui = true;
        book.craftable_only = !book.craftable_only;
        book.settings_dirty = true;
        book.page = 0;
    }
    let mut categories: Vec<_> = book
        .categories
        .values()
        .copied()
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
    for (i, category) in categories.iter().take(5).enumerate() {
        let tab = [
            r[0] - 30.0 * panel.scale,
            r[1] + (3.0 + 27.0 * i as f32) * panel.scale,
            35.0 * panel.scale,
            27.0 * panel.scale,
        ];
        elements.push(MenuElement::Rect {
            x: tab[0],
            y: tab[1],
            w: tab[2],
            h: tab[3],
            corner_radius: 0.0,
            color: if book.category == Some(*category) {
                [0.56, 0.42, 0.22, 1.0]
            } else {
                [0.2, 0.2, 0.2, 1.0]
            },
        });
        let label = match category {
            azalea_registry::builtin::RecipeBookCategory::CraftingEquipment => "Gear",
            azalea_registry::builtin::RecipeBookCategory::CraftingBuildingBlocks => "Build",
            azalea_registry::builtin::RecipeBookCategory::CraftingMisc => "Misc",
            azalea_registry::builtin::RecipeBookCategory::CraftingRedstone => "Red",
            azalea_registry::builtin::RecipeBookCategory::FurnaceFood
            | azalea_registry::builtin::RecipeBookCategory::BlastFurnaceBlocks
            | azalea_registry::builtin::RecipeBookCategory::SmokerFood => "Food",
            azalea_registry::builtin::RecipeBookCategory::FurnaceBlocks
            | azalea_registry::builtin::RecipeBookCategory::BlastFurnaceMisc => "Blocks",
            _ => "Misc",
        };
        panel.label(elements, bx - 28.0, by + 11.0 + 27.0 * i as f32, label);
        if crate::ui::common::hit_test(cursor, tab) && clicked {
            book.clicked_ui = true;
            book.category = Some(*category);
            book.page = 0;
        }
    }
    let mut recipes = Vec::new();
    for (id, display) in &book.displays {
        let Some(category) = book.categories.get(id) else {
            continue;
        };
        if !categories.contains(category) || book.category.is_some_and(|c| c != *category) {
            continue;
        }
        let result = match (furnace_variant, display) {
            (Some(_), azalea_protocol::common::recipe::RecipeDisplayData::Furnace(d)) => &d.result,
            (None, azalea_protocol::common::recipe::RecipeDisplayData::Shapeless(d)) => &d.result,
            (None, azalea_protocol::common::recipe::RecipeDisplayData::Shaped(d)) => &d.result,
            _ => continue,
        };
        let Some((name, count)) = recipe_output_icon(result) else {
            continue;
        };
        let haystack = name.to_ascii_lowercase().replace('_', " ");
        if !book.search.is_empty() && !haystack.contains(&book.search.to_ascii_lowercase()) {
            continue;
        }
        recipes.push((*id, name, count));
    }
    let pages = recipes.len().div_ceil(20).max(1);
    book.page = book.page.min(pages - 1);
    let previous = [
        r[0] + 38.0 * panel.scale,
        r[1] + 137.0 * panel.scale,
        12.0 * panel.scale,
        17.0 * panel.scale,
    ];
    let next = [
        r[0] + 93.0 * panel.scale,
        r[1] + 137.0 * panel.scale,
        12.0 * panel.scale,
        17.0 * panel.scale,
    ];
    if book.page > 0 {
        elements.push(MenuElement::Rect {
            x: previous[0],
            y: previous[1],
            w: previous[2],
            h: previous[3],
            corner_radius: 0.0,
            color: [0.55, 0.55, 0.55, 1.0],
        });
    }
    if book.page + 1 < pages {
        elements.push(MenuElement::Rect {
            x: next[0],
            y: next[1],
            w: next[2],
            h: next[3],
            corner_radius: 0.0,
            color: [0.55, 0.55, 0.55, 1.0],
        });
    }
    if clicked && crate::ui::common::hit_test(cursor, previous) && book.page > 0 {
        book.clicked_ui = true;
        book.page -= 1;
    }
    if clicked && crate::ui::common::hit_test(cursor, next) && book.page + 1 < pages {
        book.clicked_ui = true;
        book.page += 1;
    }
    if pages > 1 {
        panel.label(
            elements,
            bx + 68.0,
            by + 142.0,
            &format!("{} / {pages}", book.page + 1),
        );
    }
    let mut selected = None;
    for (index, (id, name, count)) in recipes.iter().skip(book.page * 20).take(20).enumerate() {
        let gx = bx + 11.0 + (index % 5) as f32 * 25.0;
        let gy = by + 31.0 + (index / 5) as f32 * 25.0;
        let px = panel.ox + gx * panel.scale;
        let py = panel.oy + gy * panel.scale;
        let size = 18.0 * panel.scale;
        let rect = [px, py, size, size];
        let hovered = crate::ui::common::hit_test(cursor, rect);
        if book.flags.get(id).is_some_and(|flags| flags & 2 != 0) {
            elements.push(MenuElement::Rect {
                x: px,
                y: py,
                w: size,
                h: size,
                corner_radius: 0.0,
                color: [0.85, 0.65, 0.12, 0.8],
            });
        }
        if hovered {
            elements.push(MenuElement::Rect {
                x: px,
                y: py,
                w: size,
                h: size,
                corner_radius: 0.0,
                color: [0.45, 0.45, 0.45, 0.9],
            });
            if clicked {
                book.clicked_ui = true;
                selected = Some((*id, use_max_items));
                if let Some(flags) = book.flags.get_mut(id) {
                    *flags &= !2;
                }
            }
        }
        elements.push(MenuElement::ItemIcon {
            x: px,
            y: py,
            w: size,
            h: size,
            item_name: name.clone(),
            tint: [1.0; 4],
        });
        if *count > 1 {
            elements.push(MenuElement::TextFlat {
                x: px + size * 0.52,
                y: py + size * 0.58,
                text: count.to_string(),
                scale: crate::ui::common::FONT_SIZE * panel.scale * 0.7,
                color: [1.0; 4],
            });
        }
    }
    selected
}

/// Resolve the representative item vanilla exposes for wrapped slot displays.
/// Component-specific variants keep their underlying item icon; the GUI item
/// atlas does not yet render arbitrary item components.
fn recipe_output_icon(
    display: &azalea_protocol::common::recipe::SlotDisplayData,
) -> Option<(String, u8)> {
    use azalea_protocol::common::recipe::SlotDisplayData as D;

    match display {
        D::Empty | D::AnyFuel | D::Tag(_) => None,
        D::Item(item) => Some((crate::player::inventory::item_resource_name(item.item), 1)),
        D::ItemStack(item) => match &item.stack {
            azalea_inventory::ItemStack::Present(stack) => Some((
                crate::player::inventory::item_resource_name(stack.kind),
                u8::try_from(stack.count).ok()?,
            )),
            azalea_inventory::ItemStack::Empty => None,
        },
        D::WithAnyPotion(item) => recipe_output_icon(&item.contents),
        D::OnlyWithComponent(item) => recipe_output_icon(&item.contents),
        D::Dyed(item) => recipe_output_icon(&item.target),
        D::SmithingTrim(item) => recipe_output_icon(&item.base),
        D::WithRemainder(item) => recipe_output_icon(&item.input),
        D::Composite(item) => item.contents.iter().find_map(recipe_output_icon),
    }
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
        }
    }

    /// Draws a slot at GUI-unit position (x, y), recording it as hovered when
    /// the cursor is over it.
    pub fn slot(&mut self, x: f32, y: f32, item: &ItemStack, empty: Option<SpriteId>, num: u16) {
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
