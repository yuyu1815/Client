//! Beacon menu (26.2 `BeaconMenu`): payment slot 0, player slots 1..37,
//! data slots 0=level, 1=primary effect, 2=secondary effect.

use std::time::Instant;

use azalea_inventory::ItemStack;

use super::common::hit_test;
use super::container::{
    ContainerInput, ContainerResult, DragState, SlotCtx, push_backdrop, push_cursor_stack,
    resolve_gesture,
};
use crate::player::menu_click::ContainerKind;
use crate::renderer::pipelines::menu_overlay::MenuElement;

const DATA_LEVEL: usize = 0;
const DATA_PRIMARY: usize = 1;
const DATA_SECONDARY: usize = 2;
const SLOT_PAYMENT: u16 = 0;
const SLOT_MAIN_BASE: u16 = 1;
const SLOT_HOTBAR_BASE: u16 = 28;
const EFFECT_ROW_HEIGHT: f32 = 15.0;
const EFFECTS: [u32; 6] = [0, 2, 10, 7, 4, 9]; // speed, haste, resistance, jump boost, strength, regeneration

pub struct BeaconResult {
    pub container: ContainerResult,
    pub effects: Option<(Option<u32>, Option<u32>)>,
}

#[allow(clippy::too_many_arguments)]
pub fn build_beacon(
    elements: &mut Vec<MenuElement>,
    screen_w: f32,
    screen_h: f32,
    cursor: (f32, f32),
    input: &ContainerInput,
    slots: &[ItemStack],
    data: &[i16],
    data_received: &[bool; 10],
    title: &str,
    cursor_item: &ItemStack,
    drag: &mut Option<DragState>,
    last_click: &mut Option<(u16, Instant)>,
    gs: f32,
) -> BeaconResult {
    let panel = push_backdrop(elements, screen_w, screen_h, gs, 280.0, 278.0);
    panel.label(elements, 8.0, 7.0, title);
    panel.label(elements, 8.0, 24.0, "Payment");
    let level = if data_received[DATA_LEVEL] {
        data[DATA_LEVEL].to_string()
    } else {
        "waiting".to_owned()
    };
    panel.label(elements, 8.0, 42.0, &format!("Pyramid level: {level}"));
    panel.label(elements, 8.0, 61.0, "Primary effect");
    panel.label(elements, 145.0, 61.0, "Secondary effect");

    let data_complete =
        data_received[DATA_LEVEL] && data_received[DATA_PRIMARY] && data_received[DATA_SECONDARY];
    let effects_valid =
        effect_state_valid(data[DATA_PRIMARY]) && effect_state_valid(data[DATA_SECONDARY]);
    let level_valid = (0..=4).contains(&data[DATA_LEVEL]);
    let known = data_complete && effects_valid && level_valid;
    let tier = if known {
        data[DATA_LEVEL].clamp(0, 4) as usize
    } else {
        0
    };
    let payment = slots
        .get(SLOT_PAYMENT as usize)
        .and_then(ItemStack::as_present)
        .is_some_and(|item| {
            item.count > 0
                && matches!(
                    item.kind,
                    azalea_registry::builtin::ItemKind::IronIngot
                        | azalea_registry::builtin::ItemKind::GoldIngot
                        | azalea_registry::builtin::ItemKind::Emerald
                        | azalea_registry::builtin::ItemKind::Diamond
                        | azalea_registry::builtin::ItemKind::NetheriteIngot
                )
        });
    let selected_primary = known.then(|| effect_value(data[DATA_PRIMARY])).flatten();
    let selected_secondary = known.then(|| effect_value(data[DATA_SECONDARY])).flatten();
    let primary_options: Vec<u32> = EFFECTS
        .iter()
        .copied()
        .filter(|id| effect_tier(*id) <= tier && tier > 0)
        .collect();
    let secondary_options: Vec<u32> = if tier >= 4 {
        selected_primary
            .map(|primary| {
                if primary == 9 {
                    vec![9]
                } else {
                    vec![primary, 9]
                }
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let effect_state_eligible = selected_primary
        .is_none_or(|primary| primary_options.contains(&primary))
        && selected_secondary.is_none_or(|secondary| {
            tier >= 4
                && selected_primary.is_some_and(|primary| secondary == primary || secondary == 9)
        });
    let can_select = known && tier > 0 && payment && effect_state_eligible;

    let mut effect_selection = None;
    for (index, id) in primary_options.iter().copied().enumerate() {
        let y = 78.0 + index as f32 * EFFECT_ROW_HEIGHT;
        let name = effect_name(id);
        let label = if selected_primary == Some(id) {
            format!("> {name}")
        } else {
            name.to_owned()
        };
        panel.label(elements, 8.0, y, &label);
        if can_select
            && input.left_pressed
            && hit_test(
                cursor,
                [
                    panel.ox + 5.0 * panel.scale,
                    panel.oy + (y - 2.0) * panel.scale,
                    130.0 * panel.scale,
                    EFFECT_ROW_HEIGHT * panel.scale,
                ],
            )
        {
            let secondary =
                selected_secondary.filter(|secondary| *secondary == id || *secondary == 9);
            effect_selection = Some((Some(id), secondary));
        }
    }
    for (index, id) in secondary_options.iter().copied().enumerate() {
        let y = 78.0 + index as f32 * EFFECT_ROW_HEIGHT;
        let name = effect_name(id);
        let label = if selected_secondary == Some(id) {
            format!("> {name}")
        } else {
            name.to_owned()
        };
        panel.label(elements, 145.0, y, &label);
        if can_select
            && input.left_pressed
            && selected_primary.is_some()
            && hit_test(
                cursor,
                [
                    panel.ox + 142.0 * panel.scale,
                    panel.oy + (y - 2.0) * panel.scale,
                    130.0 * panel.scale,
                    EFFECT_ROW_HEIGHT * panel.scale,
                ],
            )
        {
            effect_selection = Some((selected_primary, Some(id)));
        }
    }
    if !data_complete {
        panel.label(elements, 145.0, 122.0, "Waiting for server state");
    } else if !effects_valid {
        panel.label(elements, 145.0, 122.0, "Unknown server effect ID");
    } else if !level_valid {
        panel.label(elements, 145.0, 122.0, "Unknown beacon level");
    } else if tier > 0 && !payment {
        panel.label(elements, 145.0, 122.0, "Valid payment required");
    } else if known && !effect_state_eligible {
        panel.label(elements, 145.0, 122.0, "Inconsistent server effect state");
    }
    panel.label(elements, 8.0, 170.0, "Inventory");

    let mut ctx = SlotCtx::new(
        elements,
        &panel,
        cursor,
        ContainerKind::Beacon,
        slots,
        cursor_item,
        drag,
    );
    ctx.slot(
        250.0,
        24.0,
        slots
            .get(SLOT_PAYMENT as usize)
            .unwrap_or(&ItemStack::Empty),
        None,
        SLOT_PAYMENT,
    );
    ctx.player_rows(slots, SLOT_MAIN_BASE, SLOT_HOTBAR_BASE, 184.0);
    let (hovered, shown_cursor) = ctx.finish(cursor_item);
    push_cursor_stack(elements, cursor, panel.scale, &shown_cursor);

    let (ops, clicked_outside) = resolve_gesture(
        input,
        hovered,
        &panel,
        cursor,
        ContainerKind::Beacon,
        slots,
        cursor_item,
        drag,
        last_click,
    );
    BeaconResult {
        container: ContainerResult {
            clicked_outside,
            ops,
            button: None,
            recipe_id: None,
        },
        effects: effect_selection,
    }
}

fn effect_state_valid(value: i16) -> bool {
    value == -1 || effect_value(value).is_some()
}

fn effect_value(value: i16) -> Option<u32> {
    if value == -1 {
        None
    } else {
        let id = u32::try_from(value).ok()?;
        crate::mob_effect::info(id).map(|_| id)
    }
}

fn effect_tier(id: u32) -> usize {
    match id {
        0 | 2 => 1,
        10 | 7 => 2,
        4 => 3,
        9 => 4,
        _ => usize::MAX,
    }
}

fn effect_name(id: u32) -> &'static str {
    crate::mob_effect::info(id).map_or("Unknown effect", |effect| effect.name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beacon_effect_ids_match_valid_server_effects_and_tiers() {
        assert_eq!(EFFECTS.map(effect_tier), [1, 1, 2, 2, 3, 4]);
        for id in EFFECTS {
            assert!(effect_state_valid(id as i16));
        }
        assert!(effect_state_valid(-1));
        assert!(!effect_state_valid(-2));
        assert!(!effect_state_valid(40));
    }

    #[test]
    fn beacon_container_has_one_payment_slot_and_no_drag_slots() {
        let kind = ContainerKind::Beacon;
        assert_eq!(kind.inv_start(), 1);
        assert_eq!(kind.slot_count(), 37);
        assert!((0..37).all(|slot| {
            !crate::player::menu_click::drag_slot_eligible(kind, &[], &ItemStack::Empty, slot)
        }));
    }
}
