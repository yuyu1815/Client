//! Client-side prediction of survival container clicks: a port of vanilla
//! `AbstractContainerMenu.doClick` for the menus we open (player inventory,
//! crafting table, furnace, chests). The server stays authoritative and
//! reconciles, so a wrong prediction only causes a self-correcting glitch,
//! never item dup/loss.

use azalea_inventory::components::{EquipmentSlot, Equippable, MaxStackSize};
use azalea_inventory::item::MaxStackSizeExt;
use azalea_inventory::operations::{
    ClickOperation, PickupClick, QuickCraftKind, QuickMoveClick, ThrowClick,
};
use azalea_inventory::{ItemStack, ItemStackData, Menu, Player, SlotList};
use azalea_registry::builtin::ItemKind;

/// Which container menu a click applies to. `Furnace` covers the furnace,
/// blast furnace, and smoker menus, which share the same slot structure;
/// `Chest` covers every generic 9xN menu (chests, ender chests, barrels, ...).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ContainerKind {
    Player,
    CraftingTable,
    Furnace,
    Chest { rows: u8 },
    ShulkerBox,
    Anvil,
    Enchantment,
    Beacon,
    Merchant,
    Horse { columns: u8 },
}

impl ContainerKind {
    pub fn slot_count(self) -> usize {
        match self {
            Self::Player | Self::CraftingTable => 46,
            Self::Furnace | Self::Anvil => 39,
            Self::Chest { rows } => rows as usize * 9 + 36,
            Self::ShulkerBox => 63,
            Self::Enchantment => 38,
            Self::Beacon => 37,
            Self::Merchant => 39,
            Self::Horse { columns } => 38 + 3 * columns as usize,
        }
    }

    /// First menu slot backed by the player inventory; menu slot `i` maps to
    /// player inventory slot `i - inv_start() + 9` from here on.
    pub fn inv_start(self) -> usize {
        match self {
            Self::Player | Self::CraftingTable => 10,
            Self::Furnace | Self::Anvil => 3,
            Self::Chest { rows } => rows as usize * 9,
            Self::ShulkerBox => 27,
            Self::Enchantment => 2,
            Self::Beacon => 1,
            Self::Merchant => 3,
            Self::Horse { columns } => 2 + 3 * columns as usize,
        }
    }

    /// Result slots we leave server-authoritative because taking them has
    /// recipe/experience effects that this predictor does not model.
    fn crafting_result_slot(self) -> Option<usize> {
        match self {
            Self::Player | Self::CraftingTable => Some(0),
            Self::Anvil | Self::Merchant => Some(2),
            Self::Furnace
            | Self::Chest { .. }
            | Self::ShulkerBox
            | Self::Enchantment
            | Self::Beacon
            | Self::Horse { .. } => None,
        }
    }

    /// Menu slot holding hotbar index `i` (0-8), the SWAP click target.
    pub fn hotbar_menu_slot(self, i: u8) -> u16 {
        match self {
            Self::Player => 36 + i as u16,
            _ => (self.slot_count() - 9) as u16 + i as u16,
        }
    }

    pub fn offhand_menu_slot(self) -> Option<u16> {
        matches!(self, Self::Player).then_some(45)
    }

    /// Per-slot stack limit where a menu overrides the item's maximum.
    fn slot_limit(self, s: usize) -> i32 {
        match (self, s) {
            (Self::Player, 5..=8) | (Self::Enchantment, 0) | (Self::Beacon, 0) => 1,
            _ => i32::MAX,
        }
    }

    fn build_menu(self, slots: &[ItemStack]) -> Option<Menu> {
        let mut menu = match self {
            Self::Player => Menu::Player(Player::default()),
            Self::CraftingTable => Menu::Crafting {
                result: ItemStack::Empty,
                grid: SlotList::default(),
                player: SlotList::default(),
            },
            Self::Furnace => Menu::Furnace {
                ingredient: ItemStack::Empty,
                fuel: ItemStack::Empty,
                result: ItemStack::Empty,
                player: SlotList::default(),
            },
            Self::Chest { rows: 1 } => Menu::Generic9x1 {
                contents: SlotList::default(),
                player: SlotList::default(),
            },
            Self::Chest { rows: 2 } => Menu::Generic9x2 {
                contents: SlotList::default(),
                player: SlotList::default(),
            },
            Self::Chest { rows: 3 } => Menu::Generic9x3 {
                contents: SlotList::default(),
                player: SlotList::default(),
            },
            Self::Chest { rows: 4 } => Menu::Generic9x4 {
                contents: SlotList::default(),
                player: SlotList::default(),
            },
            Self::Chest { rows: 5 } => Menu::Generic9x5 {
                contents: SlotList::default(),
                player: SlotList::default(),
            },
            Self::Chest { .. } => Menu::Generic9x6 {
                contents: SlotList::default(),
                player: SlotList::default(),
            },
            Self::ShulkerBox => Menu::ShulkerBox {
                contents: SlotList::default(),
                player: SlotList::default(),
            },
            Self::Anvil => Menu::Anvil {
                first: ItemStack::Empty,
                second: ItemStack::Empty,
                result: ItemStack::Empty,
                player: SlotList::default(),
            },
            Self::Enchantment => Menu::Enchantment {
                item: ItemStack::Empty,
                lapis: ItemStack::Empty,
                player: SlotList::default(),
            },
            // Azalea has no native beacon menu model; leave its clicks server-authoritative.
            Self::Beacon => return None,
            // Azalea has no native merchant or mount menu model. Never predict
            // these by pretending they are a chest or another menu.
            Self::Merchant | Self::Horse { .. } => return None,
        };
        for (i, item) in slots.iter().enumerate() {
            if let Some(s) = menu.slot_mut(i) {
                *s = item.clone();
            }
        }
        Some(menu)
    }

    /// Predict placement restrictions for menu slots whose rule is local and
    /// deterministic. Furnace fuel remains server-authoritative (tag
    /// dependent).
    fn may_place(self, s: usize, item: &ItemStackData) -> bool {
        match (self, s) {
            (Self::Player | Self::CraftingTable, 0) | (Self::Furnace, 1 | 2) | (Self::Anvil, 2) => {
                false
            }
            (Self::Player, 5..=8) => {
                let want = match s {
                    5 => EquipmentSlot::Head,
                    6 => EquipmentSlot::Chest,
                    7 => EquipmentSlot::Legs,
                    _ => EquipmentSlot::Feet,
                };
                item.get_component::<Equippable>().map(|c| c.slot) == Some(want)
            }
            (Self::ShulkerBox, 0..=26) => {
                !crate::player::inventory::item_resource_name(item.kind).ends_with("shulker_box")
            }
            // Merchant payment slots and horse storage/player slots accept
            // ordinary stacks. Horse equipment rules depend on the entity.
            (Self::Merchant, 0..=1) => true,
            (Self::Merchant, _) | (Self::Horse { .. }, 0..=1) => false,
            (Self::Horse { .. }, _) => true,
            (Self::Enchantment, 1) => item.kind == ItemKind::LapisLazuli,
            _ => true,
        }
    }
}

/// Predict a non-drag click against the given menu slots, returning the
/// changed slots (the caller applies them). Returns empty for ops we don't
/// predict, leaving those server-authoritative.
pub fn apply_click(
    kind: ContainerKind,
    slots: &[ItemStack],
    cursor: &mut ItemStack,
    op: &ClickOperation,
    creative: bool,
) -> Vec<(u16, ItemStack)> {
    if matches!(
        kind,
        ContainerKind::Beacon | ContainerKind::Merchant | ContainerKind::Horse { .. }
    ) {
        return Vec::new();
    }
    if op
        .slot_num()
        .is_some_and(|s| Some(s as usize) == kind.crafting_result_slot())
    {
        return Vec::new();
    }
    // Furnace player-slot quick-move needs server-side recipe/fuel data.
    if kind == ContainerKind::Furnace
        && matches!(op, ClickOperation::QuickMove(_))
        && op
            .slot_num()
            .is_some_and(|s| s as usize >= kind.inv_start())
    {
        return Vec::new();
    }
    let Some(mut menu) = kind.build_menu(slots) else {
        return Vec::new();
    };
    apply_op(kind, &mut menu, cursor, op, creative);

    let mut changed = Vec::new();
    for (i, before) in slots.iter().enumerate() {
        let after = menu.slot(i).cloned().unwrap_or(ItemStack::Empty);
        if after != *before {
            changed.push((i as u16, after));
        }
    }
    changed
}

/// Distribute the carried stack across dragged slots (left = even split,
/// right = one each), capped by both item components and slot rules.
pub fn drag_distribution(
    container: ContainerKind,
    slots: &[ItemStack],
    cursor: &ItemStack,
    kind: &QuickCraftKind,
    covered: &[u16],
) -> (Vec<(u16, ItemStack)>, ItemStack) {
    if matches!(
        container,
        ContainerKind::Beacon | ContainerKind::Merchant | ContainerKind::Horse { .. }
    ) {
        return (Vec::new(), cursor.clone());
    }
    let ItemStack::Present(carried) = cursor else {
        return (Vec::new(), cursor.clone());
    };
    let eligible: Vec<u16> = covered
        .iter()
        .copied()
        .filter(|&s| drag_slot_eligible(container, slots, cursor, s))
        .collect();
    let n = eligible.len() as i32;
    if n == 0 {
        return (Vec::new(), cursor.clone());
    }
    let max = max_stack_size(carried);
    let place = match kind {
        QuickCraftKind::Left => carried.count / n,
        QuickCraftKind::Right => 1,
        QuickCraftKind::Middle => max,
    };
    let mut remaining = carried.count;
    let mut changed = Vec::new();
    for &s in &eligible {
        let it = slots.get(s as usize).unwrap_or(&ItemStack::Empty);
        let existing = if same_item(cursor, it) { it.count() } else { 0 };
        let new_count =
            (place + existing).min(effective_stack_limit(max, container.slot_limit(s as usize)));
        remaining -= new_count - existing;
        let mut stack = carried.clone();
        stack.count = new_count;
        changed.push((s, ItemStack::Present(stack)));
    }
    (changed, with_count(carried.clone(), remaining))
}

pub fn drag_slot_eligible(
    container: ContainerKind,
    slots: &[ItemStack],
    cursor: &ItemStack,
    slot: u16,
) -> bool {
    let slot_index = slot as usize;
    let drag_allowed = match container {
        ContainerKind::Merchant => slot_index != 2 && slot_index < container.slot_count(),
        ContainerKind::Beacon => false,
        ContainerKind::Horse { .. } => slot_index >= 2 && slot_index < container.slot_count(),
        _ => true,
    };
    if !drag_allowed {
        return false;
    }
    let ItemStack::Present(carried) = cursor else {
        return false;
    };
    if !container.may_place(slot as usize, carried) {
        return false;
    }
    let it = slots.get(slot as usize).unwrap_or(&ItemStack::Empty);
    it.is_empty() || same_item(cursor, it)
}

fn apply_op(
    kind: ContainerKind,
    menu: &mut Menu,
    cursor: &mut ItemStack,
    op: &ClickOperation,
    creative: bool,
) {
    match op {
        ClickOperation::Pickup(p) => match p {
            PickupClick::Left { slot: Some(s) } => {
                pickup_click(kind, menu, cursor, *s as usize, true)
            }
            PickupClick::Right { slot: Some(s) } => {
                pickup_click(kind, menu, cursor, *s as usize, false)
            }
            PickupClick::Left { slot: None } | PickupClick::LeftOutside => {
                *cursor = ItemStack::Empty
            }
            PickupClick::Right { slot: None } | PickupClick::RightOutside => shrink(cursor, 1),
        },
        ClickOperation::QuickMove(q) => {
            let s = match q {
                QuickMoveClick::Left { slot } | QuickMoveClick::Right { slot } => *slot as usize,
            };
            quick_move(kind, menu, s);
        }
        ClickOperation::PickupAll(_) => pickup_all(kind, menu, cursor),
        ClickOperation::Swap(s) => {
            let target = match s.target_slot {
                i @ 0..=8 => Some(kind.hotbar_menu_slot(i) as usize),
                40 => kind.offhand_menu_slot().map(usize::from),
                _ => None,
            };
            let Some(target) = target else { return };
            swap_click(kind, menu, s.source_slot as usize, target);
        }
        ClickOperation::Throw(t) => {
            if cursor.is_present() {
                return;
            }
            match t {
                ThrowClick::Single { slot } => {
                    let mut item = take_slot(menu, *slot as usize);
                    shrink(&mut item, 1);
                    put_slot(menu, *slot as usize, item);
                }
                ThrowClick::All { slot } => put_slot(menu, *slot as usize, ItemStack::Empty),
            }
        }
        ClickOperation::Clone(c) => {
            if creative
                && cursor.is_empty()
                && let Some(ItemStack::Present(d)) = menu.slot(c.slot as usize)
            {
                let mut full = d.clone();
                full.count = max_stack_size(&full);
                *cursor = ItemStack::Present(full);
            }
        }
        ClickOperation::QuickCraft(_) => {}
    }
}

fn swap_click(kind: ContainerKind, menu: &mut Menu, source: usize, target: usize) {
    let held = take_slot(menu, target);
    let slot_item = take_slot(menu, source);
    let (new_slot, new_held) = match (held, slot_item) {
        (ItemStack::Empty, ItemStack::Empty) => (ItemStack::Empty, ItemStack::Empty),
        (ItemStack::Empty, item) => (ItemStack::Empty, item),
        (ItemStack::Present(held), ItemStack::Empty) => {
            let max = effective_stack_limit(max_stack_size(&held), kind.slot_limit(source));
            if kind.may_place(source, &held) {
                let (placed, remaining) = split_stack_count(held.count, max);
                (
                    with_count(held.clone(), placed),
                    with_count(held, remaining),
                )
            } else {
                (ItemStack::Empty, ItemStack::Present(held))
            }
        }
        (ItemStack::Present(held), slot_item) => {
            let max = effective_stack_limit(max_stack_size(&held), kind.slot_limit(source));
            if kind.may_place(source, &held) && held.count <= max {
                (ItemStack::Present(held), slot_item)
            } else {
                (slot_item, ItemStack::Present(held))
            }
        }
    };
    put_slot(menu, source, new_slot);
    put_slot(menu, target, new_held);
}

fn pickup_click(
    kind: ContainerKind,
    menu: &mut Menu,
    cursor: &mut ItemStack,
    s: usize,
    primary: bool,
) {
    let mut slot_item = take_slot(menu, s);
    let mut carried = std::mem::take(cursor);
    if slot_item.is_empty() {
        if carried.as_present().is_some_and(|c| kind.may_place(s, c)) {
            let amount = if primary { carried.count() } else { 1 };
            safe_insert(kind, s, &mut slot_item, &mut carried, amount);
        }
    } else if carried.is_empty() {
        let amount = if primary {
            slot_item.count()
        } else {
            (slot_item.count() + 1) / 2
        };
        carried = slot_item.split(amount as u32);
    } else if carried.as_present().is_some_and(|c| kind.may_place(s, c)) {
        if same_item(&carried, &slot_item) {
            let amount = if primary { carried.count() } else { 1 };
            safe_insert(kind, s, &mut slot_item, &mut carried, amount);
        } else if carried.as_present().is_some_and(|c| {
            c.count <= effective_stack_limit(max_stack_size(c), kind.slot_limit(s))
        }) {
            std::mem::swap(&mut carried, &mut slot_item);
        }
    } else if same_item(&carried, &slot_item) {
        merge_into(&mut carried, &mut slot_item, kind.slot_limit(s));
    }
    put_slot(menu, s, slot_item);
    *cursor = carried;
}

fn safe_insert(
    kind: ContainerKind,
    s: usize,
    slot: &mut ItemStack,
    carried: &mut ItemStack,
    amount: i32,
) {
    let ItemStack::Present(c) = carried.clone() else {
        return;
    };
    let max = effective_stack_limit(max_stack_size(&c), kind.slot_limit(s));
    let take = match slot {
        ItemStack::Empty => amount.min(c.count).min(max),
        ItemStack::Present(d) => amount.min(c.count).min((max - d.count).max(0)),
    };
    if take <= 0 {
        return;
    }
    match slot {
        ItemStack::Present(d) => d.count += take,
        ItemStack::Empty => {
            let mut d = c;
            d.count = take;
            *slot = ItemStack::Present(d);
        }
    }
    shrink(carried, take);
}

fn quick_move(kind: ContainerKind, menu: &mut Menu, s: usize) {
    for _ in 0..menu.len() {
        let before = menu.slot(s).map(ItemStack::count).unwrap_or(0);
        if before == 0 {
            break;
        }
        match kind {
            ContainerKind::Chest { .. } | ContainerKind::ShulkerBox => {
                let split = kind.inv_start();
                if s < split {
                    move_item_stack_to(kind, menu, s, split..menu.len(), true);
                } else {
                    move_item_stack_to(kind, menu, s, 0..split, false);
                }
            }
            ContainerKind::Anvil => {
                if s < 3 {
                    move_item_stack_to(kind, menu, s, 3..menu.len(), false);
                } else {
                    move_item_stack_to(kind, menu, s, 0..2, false);
                }
            }
            ContainerKind::Furnace => move_item_stack_to(kind, menu, s, 3..menu.len(), s == 2),
            ContainerKind::Enchantment => {
                if s < 2 {
                    move_item_stack_to(kind, menu, s, 2..menu.len(), true);
                } else if menu
                    .slot(s)
                    .and_then(ItemStack::as_present)
                    .is_some_and(|d| d.kind == ItemKind::LapisLazuli)
                {
                    move_item_stack_to(kind, menu, s, 1..2, true);
                } else if menu.slot(0).is_some_and(ItemStack::is_empty)
                    && let ItemStack::Present(d) = take_slot(menu, s)
                {
                    put_slot(menu, s, with_count(d.clone(), d.count - 1));
                    put_slot(menu, 0, with_count(d, 1));
                }
            }
            _ => {
                menu.quick_move_stack(s);
            }
        }
        if menu.slot(s).map(ItemStack::count).unwrap_or(0) == before {
            break;
        }
    }
}

fn move_item_stack_to(
    kind: ContainerKind,
    menu: &mut Menu,
    src: usize,
    range: std::ops::Range<usize>,
    reverse: bool,
) {
    let mut moving = take_slot(menu, src);
    let indices: Vec<usize> = if reverse {
        range.rev().collect()
    } else {
        range.collect()
    };
    if moving.as_present().is_some_and(|d| max_stack_size(d) > 1) {
        for &i in &indices {
            if moving.is_empty() {
                break;
            }
            if let Some(slot) = menu.slot_mut(i)
                && same_item(slot, &moving)
            {
                merge_into(slot, &mut moving, kind.slot_limit(i));
            }
        }
    }
    for &i in &indices {
        let ItemStack::Present(data) = moving.clone() else {
            break;
        };
        if !kind.may_place(i, &data) {
            continue;
        }
        if let Some(slot) = menu.slot_mut(i)
            && slot.is_empty()
        {
            let take = data.count.min(effective_stack_limit(
                max_stack_size(&data),
                kind.slot_limit(i),
            ));
            *slot = with_count(data, take);
            shrink(&mut moving, take);
        }
    }
    put_slot(menu, src, moving);
}

fn pickup_all(kind: ContainerKind, menu: &mut Menu, cursor: &mut ItemStack) {
    let ItemStack::Present(carried) = cursor else {
        return;
    };
    let max = max_stack_size(carried);
    for pass in 0..2 {
        for s in 0..menu.len() {
            if Some(s) == kind.crafting_result_slot() {
                continue;
            }
            if cursor.count() >= max {
                break;
            }
            let slot_count = menu.slot(s).map(ItemStack::count).unwrap_or(0);
            if slot_count == 0 || !same_item(cursor, menu.slot(s).unwrap()) {
                continue;
            }
            if pass == 0 && slot_count >= max {
                continue;
            }
            let take = (max - cursor.count()).min(slot_count);
            shrink_slot(menu, s, take);
            if let ItemStack::Present(c) = cursor {
                c.count += take;
            }
        }
    }
}

fn merge_into(dst: &mut ItemStack, src: &mut ItemStack, slot_limit: i32) {
    if let (ItemStack::Present(d), ItemStack::Present(s)) = (&mut *dst, &mut *src) {
        let moved = (effective_stack_limit(max_stack_size(d), slot_limit) - d.count)
            .max(0)
            .min(s.count);
        d.count += moved;
        s.count -= moved;
    }
    src.update_empty();
}

fn take_slot(menu: &mut Menu, s: usize) -> ItemStack {
    menu.slot_mut(s)
        .map(std::mem::take)
        .unwrap_or(ItemStack::Empty)
}

fn put_slot(menu: &mut Menu, s: usize, item: ItemStack) {
    if let Some(sl) = menu.slot_mut(s) {
        *sl = item;
    }
}

fn shrink(item: &mut ItemStack, n: i32) {
    if let ItemStack::Present(d) = item {
        d.count -= n;
    }
    item.update_empty();
}

fn shrink_slot(menu: &mut Menu, s: usize, n: i32) {
    if let Some(sl) = menu.slot_mut(s) {
        shrink(sl, n);
    }
}

fn same_item(a: &ItemStack, b: &ItemStack) -> bool {
    match (a, b) {
        (ItemStack::Present(x), ItemStack::Present(y)) => x.is_same_item_and_components(y),
        _ => false,
    }
}

fn component<T: azalea_inventory::default_components::DefaultableComponent + Clone>(
    stack: &ItemStackData,
) -> Option<T> {
    stack
        .component_patch
        .get::<T>()
        .cloned()
        .or_else(|| azalea_inventory::default_components::get_default_component::<T>(stack.kind))
}

pub(crate) fn max_stack_size(stack: &ItemStackData) -> i32 {
    component::<MaxStackSize>(stack).map_or_else(|| stack.kind.max_stack_size(), |c| c.count)
}

fn effective_stack_limit(item_limit: i32, slot_limit: i32) -> i32 {
    item_limit.min(slot_limit)
}

fn split_stack_count(count: i32, limit: i32) -> (i32, i32) {
    let placed = count.min(limit.max(0));
    (placed, count - placed)
}

fn with_count(mut data: ItemStackData, count: i32) -> ItemStack {
    if count > 0 {
        data.count = count;
        ItemStack::Present(data)
    } else {
        ItemStack::Empty
    }
}

#[cfg(test)]
mod tests {
    use super::{effective_stack_limit, split_stack_count};

    #[test]
    fn merchant_and_horse_use_native_slot_layouts() {
        let merchant = super::ContainerKind::Merchant;
        assert_eq!(merchant.slot_count(), 39);
        assert_eq!(merchant.inv_start(), 3);
        assert_eq!(merchant.hotbar_menu_slot(0), 30);
        assert_eq!(merchant.hotbar_menu_slot(8), 38);
        assert_eq!(merchant.crafting_result_slot(), Some(2));

        let horse = super::ContainerKind::Horse { columns: 0 };
        assert_eq!(horse.slot_count(), 38);
        assert_eq!(horse.inv_start(), 2);
        let chest_horse = super::ContainerKind::Horse { columns: 5 };
        assert_eq!(chest_horse.slot_count(), 53);
        assert_eq!(chest_horse.inv_start(), 17);
        assert_eq!(chest_horse.hotbar_menu_slot(8), 52);
    }

    #[test]
    fn slot_max_never_exceeds_item_or_slot_limits() {
        assert_eq!(effective_stack_limit(64, 1), 1);
        assert_eq!(effective_stack_limit(16, 64), 16);
        assert_eq!(split_stack_count(5, 2), (2, 3));
        assert_eq!(split_stack_count(5, 0), (0, 5));
    }
}
