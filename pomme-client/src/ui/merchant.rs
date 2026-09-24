//! Dedicated merchant screen. Offer data stays in Azalea's decoded types so
//! component predicates and every native 26.2 trade field survive intact.

use std::time::Instant;

use azalea_inventory::ItemStack;
use azalea_protocol::packets::game::c_merchant_offers::MerchantOffer;

use super::common::{hit_test, item_display_name, push_item_icon};
use super::container::{
    ContainerInput, ContainerResult, DragState, Panel, SlotCtx, push_backdrop, push_cursor_stack,
    resolve_gesture,
};
use crate::player::menu_click::ContainerKind;
use crate::renderer::pipelines::menu_overlay::MenuElement;

pub const VISIBLE_OFFERS: usize = 7;
const PANEL_W: f32 = 276.0;
const ROW_Y: f32 = 18.0;
const ROW_H: f32 = 20.0;
const INVENTORY_Y: f32 = ROW_Y + VISIBLE_OFFERS as f32 * ROW_H + 8.0;
const PANEL_H: f32 = INVENTORY_Y + 114.0;

/// Complete MerchantOffers payload plus local view/selection state.
#[derive(Clone, Debug)]
pub struct MerchantModel {
    pub container_id: i32,
    pub offers: Vec<MerchantOffer>,
    pub villager_level: u32,
    pub villager_xp: u32,
    pub show_progress: bool,
    pub can_restock: bool,
    pub selected: Option<u32>,
    pub scroll: usize,
}

impl MerchantModel {
    pub fn new(
        container_id: i32,
        offers: Vec<MerchantOffer>,
        villager_level: u32,
        villager_xp: u32,
        show_progress: bool,
        can_restock: bool,
    ) -> Self {
        Self {
            container_id,
            offers,
            villager_level,
            villager_xp,
            show_progress,
            can_restock,
            selected: None,
            scroll: 0,
        }
    }

    /// Apply only the offer update for this exact open menu; stale merchant
    /// packets must not replace the currently displayed trades.
    pub fn replace_offers(
        &mut self,
        container_id: i32,
        offers: Vec<MerchantOffer>,
        villager_level: u32,
        villager_xp: u32,
        show_progress: bool,
        can_restock: bool,
    ) -> bool {
        if container_id != self.container_id {
            return false;
        }
        self.offers = offers;
        self.villager_level = villager_level;
        self.villager_xp = villager_xp;
        self.show_progress = show_progress;
        self.can_restock = can_restock;
        self.scroll = self.scroll.min(self.max_scroll());
        if self
            .selected
            .is_some_and(|index| index as usize >= self.offers.len())
        {
            self.selected = None;
        }
        true
    }

    pub fn select_visible(&mut self, row: usize) -> Option<u32> {
        let index = self.scroll.min(self.max_scroll()).checked_add(row)?;
        if row >= VISIBLE_OFFERS || index >= self.offers.len() {
            return None;
        }
        let index = u32::try_from(index).ok()?;
        self.selected = Some(index);
        Some(index)
    }

    pub fn max_scroll(&self) -> usize {
        self.offers.len().saturating_sub(VISIBLE_OFFERS)
    }

    pub fn scroll_by(&mut self, delta: i32) {
        self.scroll = self
            .scroll
            .saturating_add_signed(delta as isize)
            .min(self.max_scroll());
    }
}

/// UI output separates trade selection from ordinary slot clicks: selection
/// must be sent as ServerboundSelectTrade, never ContainerButtonClick.
pub struct MerchantResult {
    pub container: ContainerResult,
    pub select_trade: Option<u32>,
}

#[allow(clippy::too_many_arguments)]
pub fn build_merchant(
    elements: &mut Vec<MenuElement>,
    screen_w: f32,
    screen_h: f32,
    cursor: (f32, f32),
    input: &ContainerInput,
    model: &mut MerchantModel,
    slots: &[ItemStack],
    title: &str,
    cursor_item: &ItemStack,
    drag: &mut Option<DragState>,
    last_click: &mut Option<(u16, Instant)>,
    gs: f32,
) -> MerchantResult {
    let panel = push_backdrop(elements, screen_w, screen_h, gs, PANEL_W, PANEL_H);
    panel.label(elements, 8.0, 6.0, title);
    panel.label(elements, 132.0, INVENTORY_Y + 2.0, "Inventory");
    if model.show_progress {
        panel.label(
            elements,
            190.0,
            6.0,
            &format!("Level {}  XP {}", model.villager_level, model.villager_xp),
        );
    }
    if model.can_restock {
        panel.label(elements, 228.0, 6.0, "Restock");
    }

    let first = model.scroll.min(model.max_scroll());
    let mut select_trade = None;
    for row in 0..VISIBLE_OFFERS {
        let index = first + row;
        let Some(offer) = model.offers.get(index) else {
            continue;
        };
        let y = ROW_Y + row as f32 * ROW_H;
        let rect = [
            panel.ox + 3.0 * panel.scale,
            panel.oy + y * panel.scale,
            (PANEL_W - 6.0) * panel.scale,
            (ROW_H - 1.0) * panel.scale,
        ];
        let hovered = hit_test(cursor, rect);
        let selected = model.selected == u32::try_from(index).ok();
        elements.push(MenuElement::Rect {
            x: rect[0],
            y: rect[1],
            w: rect[2],
            h: rect[3],
            corner_radius: 0.0,
            color: if offer.out_of_stock {
                [0.32, 0.12, 0.12, 0.72]
            } else if selected || hovered {
                [0.45, 0.38, 0.18, 0.78]
            } else {
                [0.12, 0.12, 0.12, 0.62]
            },
        });
        draw_cost(
            elements,
            &panel,
            &offer.base_cost_a,
            effective_cost(offer),
            10.0,
            y + 2.0,
        );
        if let Some(cost) = &offer.cost_b {
            draw_cost(elements, &panel, cost, cost.count, 106.0, y + 2.0);
        }
        draw_stack(elements, &panel, &offer.result, 174.0, y + 2.0);
        if let Some(result) = offer.result.as_present() {
            panel.label(elements, 192.0, y + 6.0, &item_display_name(result));
        }
        let status = if offer.out_of_stock { "OUT" } else { "OK" };
        panel.label(
            elements,
            246.0,
            y + 6.0,
            &format!("{} {}/{}", status, offer.uses, offer.max_uses),
        );
        if input.left_pressed && hovered {
            select_trade = u32::try_from(index).ok();
            model.selected = select_trade;
        }
    }

    let mut ctx = SlotCtx::new(
        elements,
        &panel,
        cursor,
        ContainerKind::Merchant,
        slots,
        cursor_item,
        drag,
    );
    ctx.player_rows(slots, 3, 30, INVENTORY_Y + 12.0);
    ctx.slot(
        8.0,
        INVENTORY_Y - 4.0,
        slots.get(0).unwrap_or(&ItemStack::Empty),
        None,
        0,
    );
    ctx.slot(
        30.0,
        INVENTORY_Y - 4.0,
        slots.get(1).unwrap_or(&ItemStack::Empty),
        None,
        1,
    );
    ctx.slot(
        76.0,
        INVENTORY_Y - 4.0,
        slots.get(2).unwrap_or(&ItemStack::Empty),
        None,
        2,
    );
    let (hovered_slot, shown_cursor) = ctx.finish(cursor_item);
    push_cursor_stack(elements, cursor, panel.scale, &shown_cursor);

    // Offer buttons are not menu slots. Do not let clicks on that list become
    // outside-slot drops or initiate a container drag.
    let mut slot_input = input_for_slots(input);
    if hit_test(
        cursor,
        [
            panel.ox,
            panel.oy + ROW_Y * panel.scale,
            panel.w,
            (VISIBLE_OFFERS as f32 * ROW_H) * panel.scale,
        ],
    ) {
        slot_input.left_pressed = false;
        slot_input.right_pressed = false;
        slot_input.left_held = false;
        slot_input.right_held = false;
    }
    let (ops, clicked_outside) = resolve_gesture(
        &slot_input,
        hovered_slot,
        &panel,
        cursor,
        ContainerKind::Merchant,
        slots,
        cursor_item,
        drag,
        last_click,
    );
    MerchantResult {
        container: ContainerResult {
            clicked_outside,
            ops,
            button: None,
        },
        select_trade,
    }
}

fn input_for_slots(input: &ContainerInput) -> ContainerInput {
    ContainerInput {
        left_pressed: input.left_pressed,
        right_pressed: input.right_pressed,
        middle_pressed: input.middle_pressed,
        left_held: input.left_held,
        right_held: input.right_held,
        shift: input.shift,
        hotbar_swap: input.hotbar_swap,
        swap_offhand: input.swap_offhand,
        throw: input.throw,
        throw_all: input.throw_all,
    }
}

fn effective_cost(offer: &MerchantOffer) -> i32 {
    let cost_stack = offer.base_cost_a.clone().into_item_stack();
    let max_stack_size = crate::player::menu_click::max_stack_size(&cost_stack);
    effective_cost_with_max_stack(offer, max_stack_size)
}

fn effective_cost_with_max_stack(offer: &MerchantOffer, max_stack_size: i32) -> i32 {
    let base = offer.base_cost_a.count;
    let adjustment = ((base.wrapping_mul(offer.demand) as f32) * offer.price_multiplier).floor();
    let adjustment = (adjustment as i32).max(0);
    base.wrapping_add(adjustment)
        .wrapping_add(offer.special_price_diff)
        .clamp(1, max_stack_size)
}

fn draw_cost(
    elements: &mut Vec<MenuElement>,
    panel: &Panel,
    cost: &azalea_protocol::packets::game::c_merchant_offers::ItemCost,
    count: i32,
    x: f32,
    y: f32,
) {
    let mut stack = cost.clone().into_item_stack();
    stack.count = count;
    let item = ItemStack::Present(stack);
    draw_stack(elements, panel, &item, x, y);
    let name = item.as_present().map(item_display_name).unwrap_or_default();
    panel.label(elements, x + 19.0, y + 5.0, &format!("{} x{}", name, count));
}

fn draw_stack(elements: &mut Vec<MenuElement>, panel: &Panel, item: &ItemStack, x: f32, y: f32) {
    if let ItemStack::Present(data) = item {
        push_item_icon(
            elements,
            panel.ox + x * panel.scale,
            panel.oy + y * panel.scale,
            16.0 * panel.scale,
            panel.scale,
            data,
        );
    }
}

#[cfg(test)]
mod tests {
    use azalea_inventory::components::{DataComponentUnion, MaxStackSize};
    use azalea_inventory::{ItemStack, ItemStackData};
    use azalea_protocol::packets::game::c_merchant_offers::{
        DataComponentExactPredicate, ItemCost, MerchantOffer,
    };
    use azalea_registry::builtin::{DataComponentKind, ItemKind};

    use super::{MerchantModel, VISIBLE_OFFERS, effective_cost, effective_cost_with_max_stack};
    use crate::player::menu_click::max_stack_size;

    fn offer(uses: i32) -> MerchantOffer {
        MerchantOffer {
            base_cost_a: ItemCost {
                item: ItemKind::Emerald,
                count: 4,
                components: DataComponentExactPredicate { expected: vec![] },
            },
            result: ItemStack::Empty,
            cost_b: None,
            out_of_stock: false,
            uses,
            max_uses: 12,
            xp: 3,
            special_price_diff: -1,
            price_multiplier: 0.2,
            demand: 2,
        }
    }

    #[test]
    fn selection_maps_visible_row_to_global_offer_and_keeps_packet_metadata() {
        let mut model =
            MerchantModel::new(12, vec![offer(7); VISIBLE_OFFERS + 2], 3, 40, true, true);
        assert_eq!(model.container_id, 12);
        assert!(!model.replace_offers(13, vec![], 0, 0, false, false));
        assert_eq!(model.offers.len(), VISIBLE_OFFERS + 2);
        assert_eq!(model.offers[0].uses, 7);
        assert_eq!(model.offers[0].base_cost_a.count, 4);
        assert_eq!(model.offers[0].base_cost_a.item, ItemKind::Emerald);
        assert!(model.offers[0].base_cost_a.components.expected.is_empty());
        assert_eq!(model.offers[0].cost_b, None);
        assert_eq!(model.offers[0].result, ItemStack::Empty);
        assert_eq!(model.offers[0].uses, 7);
        assert_eq!(model.offers[0].max_uses, 12);
        assert_eq!(model.offers[0].xp, 3);
        assert_eq!(model.offers[0].special_price_diff, -1);
        assert_eq!(effective_cost(&model.offers[0]), 4);
        assert_eq!(model.offers[0].price_multiplier, 0.2);
        assert_eq!(model.offers[0].demand, 2);
        assert_eq!(model.villager_level, 3);
        assert_eq!(model.villager_xp, 40);
        assert!(model.show_progress && model.can_restock);
        assert_eq!(model.max_scroll(), 2);
        assert_eq!(model.select_visible(VISIBLE_OFFERS), None);
        model.scroll_by(99);
        assert_eq!(model.scroll, 2);
        assert_eq!(model.select_visible(VISIBLE_OFFERS), None);
        assert_eq!(model.select_visible(0), Some(2));
        model.scroll_by(-99);
        assert_eq!(model.scroll, 0);
        assert_eq!(model.selected, Some(2));
    }

    #[test]
    fn negative_demand_does_not_discount() {
        let mut offer = offer(0);
        offer.demand = -1;
        offer.special_price_diff = 0;
        offer.price_multiplier = 1.0;
        assert_eq!(effective_cost(&offer), 4);
    }

    #[test]
    fn negative_special_price_is_applied_independently() {
        let mut offer = offer(0);
        offer.demand = -1;
        offer.special_price_diff = -2;
        assert_eq!(effective_cost(&offer), 2);
    }

    #[test]
    fn high_demand_is_clamped_to_item_max_stack() {
        let mut offer = offer(0);
        offer.demand = 100;
        offer.price_multiplier = 10.0;
        assert_eq!(effective_cost(&offer), 64);
    }

    #[test]
    fn default_item_stack_limits_include_sixteen_and_one_stack_items() {
        let mut offer = offer(0);
        offer.base_cost_a.count = 100;
        offer.special_price_diff = 0;
        offer.base_cost_a.item = ItemKind::EnderPearl;
        assert_eq!(effective_cost(&offer), 16);
        offer.base_cost_a.item = ItemKind::Bow;
        assert_eq!(effective_cost(&offer), 1);
    }

    #[test]
    fn max_stack_component_override_is_respected() {
        let mut stack = ItemStackData {
            kind: ItemKind::Emerald,
            count: 1,
            component_patch: Default::default(),
        };
        let component = DataComponentUnion::from(MaxStackSize { count: 7 });
        // SAFETY: the union value matches MaxStackSize.
        unsafe {
            stack
                .component_patch
                .unchecked_insert_component(DataComponentKind::MaxStackSize, Some(component));
        }
        assert_eq!(max_stack_size(&stack), 7);
        let mut offer = offer(0);
        offer.base_cost_a.count = 100;
        offer.special_price_diff = 0;
        assert_eq!(
            effective_cost_with_max_stack(&offer, max_stack_size(&stack)),
            7
        );
    }

    #[test]
    fn ordinary_price_is_unchanged() {
        assert_eq!(effective_cost(&offer(0)), 4);
    }
}
