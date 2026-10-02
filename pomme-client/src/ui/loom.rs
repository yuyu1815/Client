//! Ordered server-synchronized banner patterns and the dedicated Loom screen.
use std::collections::{HashMap, HashSet};
use std::time::Instant;

use azalea_inventory::ItemStack;
use azalea_registry::{DataRegistry, Registry};
use simdnbt::owned::NbtCompound;

use super::common;
use super::container::{
    ContainerInput, ContainerResult, DragState, SlotCtx, push_cursor_stack, push_panel,
    resolve_gesture,
};
use crate::player::menu_click::ContainerKind;
use crate::renderer::pipelines::menu_overlay::{MenuElement, SpriteId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pattern {
    pub registry_key: String,
    pub asset_id: Option<String>,
    pub translation_key: Option<String>,
}

/// Registry packet order is retained explicitly; tag member IDs index this
/// vector.
#[derive(Clone, Debug, Default)]
pub struct PatternData {
    pub patterns: Vec<Pattern>,
    pub tags: HashMap<String, Vec<usize>>,
    /// Loom item-tag membership; used for eligibility, never candidate
    /// ordering.
    pub item_tags: HashMap<String, HashSet<u32>>,
    pub registry_ready: bool,
    pub tags_ready: bool,
}

impl PatternData {
    pub fn replace_registry(&mut self, entries: Vec<(String, Option<NbtCompound>)>) {
        self.patterns = entries
            .into_iter()
            .map(|(registry_key, data)| Pattern {
                registry_key,
                asset_id: data
                    .as_ref()
                    .and_then(|data| data.string("asset_id"))
                    .map(|v| v.to_string()),
                translation_key: data
                    .as_ref()
                    .and_then(|data| data.string("translation_key"))
                    .map(|v| v.to_string()),
            })
            .collect();
        self.registry_ready = true;
    }

    pub fn replace_tags(
        &mut self,
        tags: HashMap<String, Vec<usize>>,
        item_tags: HashMap<String, HashSet<u32>>,
    ) {
        self.tags = tags;
        self.item_tags = item_tags;
        self.tags_ready = true;
    }

    fn item_has_tag(&self, tag: &str, kind: azalea_registry::builtin::ItemKind) -> bool {
        if !direct_ids_compatible(crate::version::session_protocol()) {
            return false;
        }
        self.item_tags
            .get(tag)
            .is_some_and(|members| members.contains(&kind.to_u32()))
    }

    pub fn named_tag(&self, tag: &str) -> Option<Vec<usize>> {
        self.tags.get(tag).cloned()
    }
}

fn is_banner(kind: azalea_registry::builtin::ItemKind) -> bool {
    use azalea_registry::builtin::ItemKind::*;
    matches!(
        kind,
        WhiteBanner
            | OrangeBanner
            | MagentaBanner
            | LightBlueBanner
            | YellowBanner
            | LimeBanner
            | PinkBanner
            | GrayBanner
            | LightGrayBanner
            | CyanBanner
            | PurpleBanner
            | BlueBanner
            | BrownBanner
            | GreenBanner
            | RedBanner
            | BlackBanner
    )
}

/// Candidates retain every server position. An unknown registry reference stays
/// as a disabled slot rather than compacting later IDs into the wrong button.
fn max_patterns(stack: &ItemStack) -> bool {
    stack.as_present().is_some_and(|banner| {
        crate::player::menu_click::component::<azalea_inventory::components::BannerPatterns>(banner)
            .is_some_and(|layers| layers.patterns.len() >= 6)
    })
}

fn button_id(position: usize, enabled: bool) -> Option<u32> {
    if enabled && position <= i16::MAX as usize {
        Some(position as u32)
    } else {
        None
    }
}

fn direct_ids_compatible(protocol: i32) -> bool {
    protocol == pomme_protocol::version::NATIVE.protocol
}

fn selected_index(values: &[i16; 10], received: &[bool; 10]) -> Option<usize> {
    (received[0] && values[0] >= 0).then_some(values[0] as usize)
}

fn direct_positions(
    contents: &[azalea_registry::data::BannerPatternKind],
    data: &PatternData,
) -> Vec<Option<usize>> {
    contents
        .iter()
        .map(|pattern| {
            usize::try_from(pattern.protocol_id())
                .ok()
                .filter(|&index| index < data.patterns.len())
        })
        .collect()
}

fn candidate_positions(slots: &[ItemStack], data: &PatternData) -> Option<Vec<Option<usize>>> {
    if !data.registry_ready || !data.tags_ready {
        return None;
    }
    let ItemStack::Present(banner) = slots.get(0).unwrap_or(&ItemStack::Empty) else {
        return Some(Vec::new());
    };
    let ItemStack::Present(dye) = slots.get(1).unwrap_or(&ItemStack::Empty) else {
        return Some(Vec::new());
    };
    if !is_banner(banner.kind)
        || !data.item_has_tag("minecraft:loom_dyes", dye.kind)
        || crate::player::menu_click::component::<azalea_inventory::components::Dye>(dye).is_none()
    {
        return Some(Vec::new());
    }
    match slots.get(2).unwrap_or(&ItemStack::Empty) {
        ItemStack::Empty => Some(
            data.named_tag("minecraft:no_item_required")
                .unwrap_or_default()
                .into_iter()
                .map(|index| (index < data.patterns.len()).then_some(index))
                .collect(),
        ),
        ItemStack::Present(item) => {
            if !data.item_has_tag("minecraft:loom_patterns", item.kind) {
                return Some(Vec::new());
            }
            let provided = crate::player::menu_click::component::<
                azalea_inventory::components::ProvidesBannerPatterns,
            >(item);
            let Some(provided) = provided else {
                return Some(Vec::new());
            };
            let positions = match provided.key {
                azalea_registry::HolderSet::Named { key, .. } => data
                    .named_tag(&key.to_string())
                    .unwrap_or_default()
                    .into_iter()
                    .map(|index| (index < data.patterns.len()).then_some(index))
                    .collect(),
                azalea_registry::HolderSet::Direct { contents } => {
                    // The native 26.2 HolderSet encodes registry IDs in this ordered list.
                    // Older wire protocols are not assumed to share those IDs.
                    if !direct_ids_compatible(crate::version::session_protocol()) {
                        return Some(Vec::new());
                    }
                    direct_positions(&contents, data)
                }
            };
            Some(positions)
        }
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
    data_values: &[i16; 10],
    data_received: &[bool; 10],
    patterns: &PatternData,
    scroll: &mut usize,
    title: &str,
    cursor_item: &ItemStack,
    drag: &mut Option<DragState>,
    last_click: &mut Option<(u16, Instant)>,
    gs: f32,
    advanced_tooltips: bool,
) -> ContainerResult {
    const PAGE: usize = 12;
    let panel = push_panel(
        elements,
        screen_w,
        screen_h,
        gs,
        166.0,
        SpriteId::LoomBackground,
    );
    panel.label(elements, 8.0, 6.0, title);
    panel.label(elements, 8.0, 72.0, "Inventory");
    let kind = ContainerKind::Loom;
    let mut ctx = SlotCtx::new(elements, &panel, cursor, kind, slots, cursor_item, drag);
    for (index, (x, y)) in [(13.0, 17.0), (13.0, 35.0), (13.0, 53.0), (143.0, 35.0)]
        .into_iter()
        .enumerate()
    {
        ctx.slot(
            x,
            y,
            slots.get(index).unwrap_or(&ItemStack::Empty),
            None,
            index as u16,
        );
    }
    ctx.player_rows(slots, 4, 31, 84.0);
    let (hovered, shown_cursor) = ctx.finish(cursor_item);
    push_cursor_stack(elements, cursor, panel.scale, &shown_cursor);

    let candidates = candidate_positions(slots, patterns);
    let mut button = None;
    let mut pattern_tooltip = None;
    if let Some(candidates) = candidates.as_ref() {
        let start = (*scroll).min(candidates.len().saturating_sub(PAGE));
        *scroll = start;
        for (visible, candidate) in candidates.iter().enumerate().skip(start).take(PAGE) {
            let local = visible - start;
            let x = 47.0 + (local % 4) as f32 * 20.0;
            let y = 14.0 + (local / 4) as f32 * 17.0;
            let selected = selected_index(data_values, data_received) == Some(visible);
            let enabled = candidate.is_some()
                && button_id(visible, true).is_some()
                && !max_patterns(slots.get(0).unwrap_or(&ItemStack::Empty));
            elements.push(MenuElement::Rect {
                x: panel.ox + x * panel.scale,
                y: panel.oy + y * panel.scale,
                w: 19.0 * panel.scale,
                h: 16.0 * panel.scale,
                corner_radius: 1.0,
                color: if selected {
                    [0.35, 0.55, 0.22, 0.95]
                } else if enabled {
                    [0.18, 0.18, 0.18, 0.88]
                } else {
                    [0.08, 0.08, 0.08, 0.55]
                },
            });
            if let Some(index) = candidate.and_then(|index| patterns.patterns.get(index)) {
                let rect = [
                    panel.ox + x * panel.scale,
                    panel.oy + y * panel.scale,
                    19.0 * panel.scale,
                    16.0 * panel.scale,
                ];
                if common::hit_test(cursor, rect) {
                    pattern_tooltip = Some(index);
                }
                let key = index
                    .translation_key
                    .as_deref()
                    .unwrap_or(&index.registry_key);
                let translated = crate::lang::translate(key).unwrap_or(key);
                let label = translated
                    .rsplit(['.', '/', ':'])
                    .next()
                    .unwrap_or(translated);
                panel.label(
                    elements,
                    x + 1.0,
                    y + 4.0,
                    &label.chars().take(5).collect::<String>(),
                );
            } else {
                panel.label(elements, x + 1.0, y + 4.0, "?");
            }
            let rect = [
                panel.ox + x * panel.scale,
                panel.oy + y * panel.scale,
                19.0 * panel.scale,
                16.0 * panel.scale,
            ];
            if enabled && input.left_pressed && common::hit_test(cursor, rect) {
                button = button_id(visible, enabled);
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
        if start + PAGE < candidates.len() {
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
        panel.label(elements, 47.0, 25.0, "Loading patterns");
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
    if cursor_item.is_empty()
        && let Some(pattern) = pattern_tooltip
    {
        let mut lines = vec![crate::renderer::pipelines::menu_overlay::TooltipLine::new(
            pattern
                .translation_key
                .as_deref()
                .and_then(crate::lang::translate)
                .unwrap_or_else(|| {
                    pattern
                        .translation_key
                        .as_deref()
                        .unwrap_or(&pattern.registry_key)
                })
                .to_owned(),
            common::WHITE,
        )];
        if let Some(asset) = &pattern.asset_id {
            lines.push(crate::renderer::pipelines::menu_overlay::TooltipLine::new(
                format!("asset: {asset}"),
                common::WHITE,
            ));
        }
        common::push_tooltip_lines(elements, cursor, screen_w, screen_h, panel.scale, lines);
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
    // The server-authored result slot remains clickable for pickup; Loom clicks
    // are not locally predicted by ContainerKind::Loom.
    ContainerResult {
        clicked_outside,
        ops,
        button,
        recipe_id: None,
    }
}

#[cfg(test)]
mod tests {
    use azalea_inventory::ItemStackData;
    use azalea_inventory::components::{BannerPatternLayer, BannerPatterns, DataComponentUnion};
    use azalea_inventory::operations::{ClickOperation, PickupClick};
    use azalea_registry::builtin::{DataComponentKind, ItemKind};

    use super::*;

    fn item_dye_tag() -> HashMap<String, HashSet<u32>> {
        HashMap::from([(
            "minecraft:loom_dyes".into(),
            HashSet::from([ItemKind::WhiteDye.to_u32()]),
        )])
    }

    fn input(left_pressed: bool) -> ContainerInput {
        ContainerInput {
            left_pressed,
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

    #[test]
    fn no_item_tag_order_property_zero_and_protocol_positions_are_preserved() {
        let mut data = PatternData::default();
        data.replace_registry(vec![("custom:a".into(), None), ("custom:b".into(), None)]);
        data.replace_tags(
            HashMap::from([("minecraft:no_item_required".into(), vec![1, 0])]),
            item_dye_tag(),
        );
        let slots = vec![
            ItemStack::Present(ItemStackData::new(ItemKind::WhiteBanner, 1)),
            ItemStack::Present(ItemStackData::new(ItemKind::WhiteDye, 1)),
            ItemStack::Empty,
        ];
        assert_eq!(
            candidate_positions(&slots, &data),
            Some(vec![Some(1), Some(0)])
        );
        assert_eq!(button_id(0, true), Some(0));
        assert_eq!(button_id(1, true), Some(1));
        assert_eq!(button_id(i16::MAX as usize + 1, true), None);
        assert_eq!(selected_index(&[0; 10], &[true; 10]), Some(0));
        assert_eq!(selected_index(&[-1; 10], &[true; 10]), None);
    }

    #[test]
    fn six_layers_disable_selection_and_missing_inputs_have_no_candidates() {
        let mut banner = ItemStackData::new(ItemKind::WhiteBanner, 1);
        // SAFETY: each provided value matches the BannerPatterns component.
        unsafe {
            banner.component_patch.unchecked_insert_component(
                DataComponentKind::BannerPatterns,
                Some(DataComponentUnion::from(BannerPatterns {
                    patterns: vec![
                        BannerPatternLayer {
                            pattern: 0,
                            color: 0
                        };
                        6
                    ],
                })),
            );
        }
        let banner = ItemStack::Present(banner);
        assert!(max_patterns(&banner));
        assert_eq!(button_id(0, !max_patterns(&banner)), None);
        assert_eq!(candidate_positions(&[], &PatternData::default()), None);
        assert!(!direct_ids_compatible(
            pomme_protocol::version::NATIVE.protocol - 1
        ));
        assert!(direct_ids_compatible(
            pomme_protocol::version::NATIVE.protocol
        ));
    }

    #[test]
    fn stack_component_removal_overrides_dye_item_default() {
        let mut dye = ItemStackData::new(ItemKind::WhiteDye, 1);
        assert!(
            crate::player::menu_click::component::<azalea_inventory::components::Dye>(&dye)
                .is_some()
        );
        // SAFETY: the tombstone names the Dye component and carries no union payload.
        unsafe {
            dye.component_patch
                .unchecked_insert_component(DataComponentKind::Dye, None);
        }
        assert!(
            crate::player::menu_click::component::<azalea_inventory::components::Dye>(&dye)
                .is_none()
        );
    }

    #[test]
    fn no_inputs_yield_no_candidates_and_direct_holders_keep_order() {
        let mut data = PatternData::default();
        data.replace_registry(vec![("custom:a".into(), None), ("custom:b".into(), None)]);
        data.replace_tags(
            HashMap::from([("minecraft:no_item_required".into(), vec![1, 0])]),
            HashMap::new(),
        );
        assert_eq!(candidate_positions(&[], &data), Some(Vec::new()));
        assert_eq!(
            direct_positions(
                &[
                    azalea_registry::data::BannerPatternKind::new_raw(1),
                    azalea_registry::data::BannerPatternKind::new_raw(0),
                ],
                &data,
            ),
            vec![Some(1), Some(0)]
        );
    }

    #[test]
    fn selection_button_uses_candidate_position_and_result_slot_stays_pickup_clickable() {
        let mut patterns = PatternData::default();
        patterns.replace_registry(vec![("custom:a".into(), None), ("custom:b".into(), None)]);
        patterns.replace_tags(
            HashMap::from([("minecraft:no_item_required".into(), vec![1, 0])]),
            item_dye_tag(),
        );
        let mut slots = vec![ItemStack::Empty; 40];
        slots[0] = ItemStack::Present(ItemStackData::new(ItemKind::WhiteBanner, 1));
        slots[1] = ItemStack::Present(ItemStackData::new(ItemKind::WhiteDye, 1));
        let mut elements = Vec::new();
        let mut drag = None;
        let mut last = None;
        let mut scroll = 0;
        let selection = build(
            &mut elements,
            176.0,
            166.0,
            (56.0, 22.0),
            &input(true),
            &slots,
            &[0; 10],
            &[false; 10],
            &patterns,
            &mut scroll,
            "Loom",
            &ItemStack::Empty,
            &mut drag,
            &mut last,
            1.0,
            false,
        );
        assert_eq!(selection.button, Some(0));
        assert!(selection.ops.is_empty());

        slots[3] = ItemStack::Present(ItemStackData::new(ItemKind::WhiteBanner, 1));
        let result = build(
            &mut elements,
            176.0,
            166.0,
            (151.0, 43.0),
            &input(true),
            &slots,
            &[0; 10],
            &[false; 10],
            &patterns,
            &mut scroll,
            "Loom",
            &ItemStack::Empty,
            &mut drag,
            &mut last,
            1.0,
            false,
        );
        assert!(matches!(
            result.ops.as_slice(),
            [ClickOperation::Pickup(PickupClick::Left { slot: Some(3) })]
        ));
        assert_eq!(
            super::super::special_container::SpecialMenu::Loom.result_slot(),
            Some(3)
        );
    }

    #[test]
    fn registry_and_tag_reloads_keep_explicit_packet_order() {
        let mut a = NbtCompound::new();
        a.insert("asset_id", "asset_z");
        a.insert("translation_key", "block.test.z");
        let mut b = NbtCompound::new();
        b.insert("asset_id", "asset_a");
        let mut data = PatternData::default();
        data.replace_registry(vec![
            ("custom:z".into(), Some(a)),
            ("custom:a".into(), Some(b)),
        ]);
        data.replace_tags(
            HashMap::from([("minecraft:no_item_required".into(), vec![1, 0])]),
            HashMap::new(),
        );
        assert_eq!(data.patterns[0].registry_key, "custom:z");
        assert_eq!(data.patterns[0].asset_id.as_deref(), Some("asset_z"));
        assert_eq!(
            data.named_tag("minecraft:no_item_required"),
            Some(vec![1, 0])
        );
        data.replace_tags(HashMap::new(), HashMap::new());
        assert!(data.named_tag("minecraft:no_item_required").is_none());
    }
}
