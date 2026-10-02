//! Server-ordered 26.2 stonecutter recipe candidates and selection UI.
use std::collections::BTreeMap;
use std::time::Instant;

use azalea_inventory::ItemStack;
use azalea_protocol::common::recipe::{Ingredient, SlotDisplayData};
use azalea_protocol::packets::game::c_update_recipes::ClientboundUpdateRecipes;
use azalea_registry::HolderSet;
use azalea_registry::builtin::ItemKind;

use super::common;
use super::container::{
    ContainerInput, ContainerResult, DragState, SlotCtx, push_cursor_stack, push_panel,
    resolve_gesture,
};
use crate::player::inventory::item_resource_name;
use crate::player::menu_click::ContainerKind;
use crate::renderer::pipelines::menu_overlay::{MenuElement, SpriteId};

const PAGE: usize = 12;

fn button_id(filtered_index: usize) -> Option<u32> {
    // ContainerButtonClick accepts Java int, but property 0 is a signed short;
    // beyond this limit the authoritative selected index cannot round-trip.
    (filtered_index <= i16::MAX as usize).then_some(filtered_index as u32)
}

fn selected_index(data: &[i16; 10], received: &[bool; 10]) -> Option<usize> {
    (received[0] && data[0] >= 0).then_some(data[0] as usize)
}

fn accepts(
    ingredient: &Ingredient,
    input: ItemKind,
    tags: &BTreeMap<String, Vec<ItemKind>>,
) -> bool {
    match &ingredient.allowed {
        HolderSet::Direct { contents } => contents.contains(&input),
        HolderSet::Named { key, .. } => tags
            .get(&key.to_string())
            .is_some_and(|items| items.contains(&input)),
    }
}

fn candidates<'a>(
    packet: Option<&'a ClientboundUpdateRecipes>,
    input: &ItemStack,
    tags: &BTreeMap<String, Vec<ItemKind>>,
) -> Option<
    Vec<(
        usize,
        &'a azalea_protocol::packets::game::c_update_recipes::SingleInputEntry,
    )>,
> {
    let packet = packet?;
    let ItemStack::Present(stack) = input else {
        return Some(Vec::new());
    };
    Some(
        packet
            .stonecutter_recipes
            .iter()
            .enumerate()
            .filter(|(_, entry)| accepts(&entry.input, stack.kind, tags))
            .collect(),
    )
}

fn display_item(display: &SlotDisplayData) -> Option<String> {
    match display {
        SlotDisplayData::Item(d) => Some(item_resource_name(d.item)),
        SlotDisplayData::ItemStack(d) => match &d.stack {
            ItemStack::Present(stack) => Some(item_resource_name(stack.kind)),
            ItemStack::Empty => None,
        },
        SlotDisplayData::WithAnyPotion(d) => display_item(&d.contents),
        SlotDisplayData::OnlyWithComponent(d) => display_item(&d.contents),
        SlotDisplayData::Dyed(d) => display_item(&d.target),
        SlotDisplayData::SmithingTrim(d) => display_item(&d.base),
        SlotDisplayData::WithRemainder(d) => display_item(&d.input),
        SlotDisplayData::Composite(d) => d.contents.iter().find_map(display_item),
        SlotDisplayData::Empty | SlotDisplayData::AnyFuel | SlotDisplayData::Tag(_) => None,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn build(
    elements: &mut Vec<MenuElement>,
    screen_w: f32,
    screen_h: f32,
    cursor: (f32, f32),
    input: &ContainerInput,
    slots: &[ItemStack],
    data: &[i16; 10],
    received: &[bool; 10],
    packet: Option<&ClientboundUpdateRecipes>,
    tags: &BTreeMap<String, Vec<ItemKind>>,
    scroll: &mut usize,
    title: &str,
    cursor_item: &ItemStack,
    drag: &mut Option<DragState>,
    last_click: &mut Option<(u16, Instant)>,
    gs: f32,
    advanced_tooltips: bool,
) -> ContainerResult {
    let panel = push_panel(
        elements,
        screen_w,
        screen_h,
        gs,
        166.0,
        SpriteId::StonecutterBackground,
    );
    panel.label(elements, 8.0, 6.0, title);
    panel.label(elements, 8.0, 72.0, "Inventory");
    let kind = ContainerKind::Stonecutter;
    let mut ctx = SlotCtx::new(elements, &panel, cursor, kind, slots, cursor_item, drag);
    ctx.slot(
        20.0,
        35.0,
        slots.first().unwrap_or(&ItemStack::Empty),
        None,
        0,
    );
    ctx.slot(
        143.0,
        35.0,
        slots.get(1).unwrap_or(&ItemStack::Empty),
        None,
        1,
    );
    ctx.player_rows(slots, 2, 29, 84.0);
    let (hovered, shown_cursor) = ctx.finish(cursor_item);
    push_cursor_stack(elements, cursor, panel.scale, &shown_cursor);

    let list = if crate::version::session_protocol() == pomme_protocol::version::NATIVE.protocol {
        candidates(packet, slots.first().unwrap_or(&ItemStack::Empty), tags)
    } else {
        None
    };
    let mut button = None;
    let mut tooltip = None;
    if let Some(list) = list {
        let start = (*scroll).min(list.len().saturating_sub(PAGE));
        *scroll = start;
        for (visible, (_, entry)) in list.iter().enumerate().skip(start).take(PAGE) {
            let local = visible - start;
            let x = 47.0 + (local % 4) as f32 * 20.0;
            let y = 14.0 + (local / 4) as f32 * 17.0;
            let rect = [
                panel.ox + x * panel.scale,
                panel.oy + y * panel.scale,
                19.0 * panel.scale,
                16.0 * panel.scale,
            ];
            // Vanilla button IDs and property 0 are positions in the filtered
            // set, not recipe IDs or positions in the unfiltered packet list.
            let selected = selected_index(data, received) == Some(visible);
            let button_id = button_id(visible);
            elements.push(MenuElement::Rect {
                x: rect[0],
                y: rect[1],
                w: rect[2],
                h: rect[3],
                corner_radius: 1.0,
                color: if selected {
                    [0.35, 0.55, 0.22, 0.95]
                } else if button_id.is_some() {
                    [0.18, 0.18, 0.18, 0.88]
                } else {
                    [0.08, 0.08, 0.08, 0.55]
                },
            });
            let result = display_item(&entry.recipe.option_display);
            if let Some(name) = result {
                elements.push(MenuElement::ItemIcon {
                    x: rect[0],
                    y: rect[1],
                    w: 14.0 * panel.scale,
                    h: 14.0 * panel.scale,
                    item_name: name.clone(),
                    tint: [1.0; 4],
                    stack_dye_rgb: None,
                    player_head_profile_source: None,
                });
                let label = name.rsplit(':').next().unwrap_or(&name).replace('_', " ");
                panel.label(
                    elements,
                    x + 12.0,
                    y + 5.0,
                    &label.chars().take(2).collect::<String>(),
                );
                if common::hit_test(cursor, rect) {
                    tooltip = Some(name);
                }
            } else {
                panel.label(elements, x + 4.0, y + 4.0, "?");
            }
            if input.left_pressed && common::hit_test(cursor, rect) {
                button = button_id;
            }
        }
        if start > 0 {
            panel.label(elements, 47.0, 67.0, "<");
            if input.left_pressed
                && common::hit_test(
                    cursor,
                    [
                        panel.ox + 46.0 * panel.scale,
                        panel.oy + 65.0 * panel.scale,
                        14.0 * panel.scale,
                        14.0 * panel.scale,
                    ],
                )
            {
                *scroll = start.saturating_sub(PAGE);
            }
        }
        if start + PAGE < list.len() {
            panel.label(elements, 119.0, 67.0, ">");
            if input.left_pressed
                && common::hit_test(
                    cursor,
                    [
                        panel.ox + 116.0 * panel.scale,
                        panel.oy + 65.0 * panel.scale,
                        14.0 * panel.scale,
                        14.0 * panel.scale,
                    ],
                )
            {
                *scroll = start.saturating_add(PAGE);
            }
        }
    } else {
        panel.label(elements, 47.0, 25.0, "Loading recipes");
    }

    if cursor_item.is_empty()
        && let Some(name) = tooltip
    {
        common::push_tooltip_lines(
            elements,
            cursor,
            screen_w,
            screen_h,
            panel.scale,
            vec![crate::renderer::pipelines::menu_overlay::TooltipLine::new(
                name.replace('_', " "),
                common::WHITE,
            )],
        );
    }
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
    use azalea_protocol::packets::game::c_update_recipes::{
        SelectableRecipe as DisplayRecipe, SingleInputEntry,
    };
    use azalea_registry::HolderSet;

    use super::*;

    #[test]
    fn ordered_filtering_preserves_server_positions_and_input_swap_or_clear() {
        let packet = ClientboundUpdateRecipes {
            item_sets: Default::default(),
            stonecutter_recipes: [ItemKind::Stone, ItemKind::Granite, ItemKind::Stone]
                .into_iter()
                .map(|kind| SingleInputEntry {
                    input: azalea_protocol::common::recipe::Ingredient {
                        allowed: HolderSet::Direct {
                            contents: vec![kind].into(),
                        },
                    },
                    recipe: DisplayRecipe {
                        option_display: SlotDisplayData::Item(
                            azalea_protocol::common::recipe::ItemSlotDisplay { item: kind },
                        ),
                    },
                })
                .collect(),
        };
        let tags = BTreeMap::new();
        let stone = ItemStack::Present(azalea_inventory::ItemStackData::new(ItemKind::Stone, 1));
        let granite =
            ItemStack::Present(azalea_inventory::ItemStackData::new(ItemKind::Granite, 1));
        let filtered = candidates(Some(&packet), &stone, &tags).unwrap();
        assert_eq!(filtered.iter().map(|(i, _)| *i).collect::<Vec<_>>(), [0, 2]);
        assert_eq!(button_id(0), Some(0));
        assert_eq!(button_id(1), Some(1));
        assert_eq!(button_id(i16::MAX as usize), Some(i16::MAX as u32));
        assert_eq!(button_id(i16::MAX as usize + 1), None);
        assert_eq!(
            selected_index(&[1, 0, 0, 0, 0, 0, 0, 0, 0, 0], &[true; 10]),
            Some(1)
        );
        assert_eq!(
            selected_index(&[1, 0, 0, 0, 0, 0, 0, 0, 0, 0], &[false; 10]),
            None
        );
        assert_eq!(
            candidates(Some(&packet), &granite, &tags)
                .unwrap()
                .iter()
                .map(|(i, _)| *i)
                .collect::<Vec<_>>(),
            [1]
        );
        assert!(
            candidates(Some(&packet), &ItemStack::Empty, &tags)
                .unwrap()
                .is_empty()
        );
        let tagged = azalea_protocol::common::recipe::Ingredient {
            allowed: HolderSet::Named {
                key: azalea_registry::identifier::Identifier::new("minecraft:stonecuttable"),
                contents: vec![azalea_registry::identifier::Identifier::new(
                    "minecraft:stone",
                )],
            },
        };
        let received_tags =
            BTreeMap::from([("minecraft:stonecuttable".into(), vec![ItemKind::Stone])]);
        assert!(accepts(&tagged, ItemKind::Stone, &received_tags));
        assert!(!accepts(&tagged, ItemKind::Granite, &received_tags));
    }
}
