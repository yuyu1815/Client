//! Dedicated native MountScreenOpen horse inventory layout. `columns` remains
//! the packet's chest-column count; it is never reinterpreted as chest rows.

use std::time::Instant;

use azalea_inventory::ItemStack;

use super::container::{
    ContainerInput, ContainerResult, DragState, SlotCtx, push_backdrop, push_cursor_stack,
    resolve_gesture,
};
use crate::player::menu_click::ContainerKind;
use crate::renderer::pipelines::menu_overlay::MenuElement;

const PANEL_W: f32 = 176.0;
const PLAYER_Y: f32 = 84.0;
const PANEL_H: f32 = 160.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HorseLayout {
    pub columns: u8,
}

impl HorseLayout {
    pub const fn slot_count(self) -> usize {
        38 + 3 * self.columns as usize
    }

    pub const fn player_start(self) -> usize {
        2 + 3 * self.columns as usize
    }

    pub const fn storage_slots(self) -> usize {
        3 * self.columns as usize
    }
}

#[allow(clippy::too_many_arguments)]
pub fn build_horse(
    elements: &mut Vec<MenuElement>,
    screen_w: f32,
    screen_h: f32,
    cursor: (f32, f32),
    input: &ContainerInput,
    layout: HorseLayout,
    slots: &[ItemStack],
    title: &str,
    cursor_item: &ItemStack,
    drag: &mut Option<DragState>,
    last_click: &mut Option<(u16, Instant)>,
    gs: f32,
) -> ContainerResult {
    let panel = push_backdrop(elements, screen_w, screen_h, gs, PANEL_W, PANEL_H);
    // Dedicated neutral mount panel, not a generic chest background.
    elements.push(MenuElement::Rect {
        x: panel.ox,
        y: panel.oy,
        w: panel.w,
        h: panel.h,
        corner_radius: 0.0,
        color: [0.18, 0.14, 0.10, 0.94],
    });
    panel.label(elements, 8.0, 6.0, title);
    panel.label(elements, 8.0, 72.0, "Inventory");

    if layout.columns > 0 {
        elements.push(MenuElement::Rect {
            x: panel.ox + 76.0 * panel.scale,
            y: panel.oy + 16.0 * panel.scale,
            w: layout.columns as f32 * 18.0 * panel.scale,
            h: 56.0 * panel.scale,
            corner_radius: 0.0,
            color: [0.08, 0.07, 0.06, 0.8],
        });
    }
    let kind = ContainerKind::Horse {
        columns: layout.columns,
    };
    let mut ctx = SlotCtx::new(elements, &panel, cursor, kind, slots, cursor_item, drag);
    ctx.slot(
        8.0,
        18.0,
        slots.get(0).unwrap_or(&ItemStack::Empty),
        None,
        0,
    );
    ctx.slot(
        8.0,
        36.0,
        slots.get(1).unwrap_or(&ItemStack::Empty),
        None,
        1,
    );
    for row in 0..3u16 {
        for column in 0..layout.columns as u16 {
            let slot = 2 + row * layout.columns as u16 + column;
            let item = slots.get(slot as usize).unwrap_or(&ItemStack::Empty);
            ctx.slot(
                79.0 + column as f32 * 18.0,
                18.0 + row as f32 * 18.0,
                item,
                None,
                slot,
            );
        }
    }
    let player_start = layout.player_start() as u16;
    ctx.player_rows(slots, player_start, player_start + 27, PLAYER_Y);
    let (hovered, shown_cursor) = ctx.finish(cursor_item);
    push_cursor_stack(elements, cursor, panel.scale, &shown_cursor);
    let (ops, clicked_outside) = resolve_gesture(
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
    ContainerResult {
        clicked_outside,
        ops,
        button: None,
        recipe_id: None,
    }
}

/// Utility for callers that need to validate/apply a raw mount packet without
/// changing the native columns meaning.
pub fn mount_slot_count(columns: u8) -> usize {
    HorseLayout { columns }.slot_count()
}

#[cfg(test)]
mod tests {
    use super::{HorseLayout, mount_slot_count};

    #[test]
    fn native_columns_define_mount_storage_and_player_slot_offset() {
        let empty = HorseLayout { columns: 0 };
        assert_eq!(empty.storage_slots(), 0);
        assert_eq!(empty.slot_count(), 38);
        assert_eq!(empty.player_start(), 2);

        let chest = HorseLayout { columns: 5 };
        assert_eq!(chest.storage_slots(), 15);
        assert_eq!(chest.slot_count(), 53);
        assert_eq!(chest.player_start(), 17);
        assert_eq!(mount_slot_count(5), 53);
    }
}
