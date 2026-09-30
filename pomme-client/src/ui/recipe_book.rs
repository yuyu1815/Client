use std::collections::BTreeMap;

use azalea_inventory::ItemStack;
use azalea_protocol::common::recipe::{Ingredient, RecipeDisplayData, SlotDisplayData};
use azalea_protocol::packets::game::c_recipe_book_add::ClientboundRecipeBookAdd;
use azalea_protocol::packets::game::c_recipe_book_settings::RecipeBookSettings;
use azalea_protocol::packets::game::c_update_recipes::ClientboundUpdateRecipes;
use azalea_registry::builtin::ItemKind;
use azalea_registry::{HolderSet, Registry};

use super::text_edit::{SystemClipboard, TextFieldState, TextInputEvent};

/// Native 26.2 recipe-book state. Recipe IDs here are server-issued display
/// IDs, not legacy recipe identifiers.
#[derive(Default)]
pub struct RecipeBookState {
    pub displays: BTreeMap<u32, RecipeDisplayData>,
    /// RecipeDisplayEntry metadata retained alongside each display.
    pub groups: BTreeMap<u32, u32>,
    pub categories: BTreeMap<u32, azalea_registry::builtin::RecipeBookCategory>,
    /// ClientboundRecipeBookAdd entry flags (notification/highlight bits).
    pub flags: BTreeMap<u32, u8>,
    pub requirements: BTreeMap<u32, Option<Vec<Ingredient>>>,
    /// Only tags actually received from the server may satisfy named
    /// ingredients.
    pub item_tags: BTreeMap<String, Vec<ItemKind>>,
    pub settings: Option<RecipeBookSettings>,
    pub updates: Option<ClientboundUpdateRecipes>,
    pub open: bool,
    pub craftable_only: bool,
    pub search: String,
    pub search_field: Option<TextFieldState>,
    pub search_dragging: bool,
    pub toggle_focused: bool,
    pub popup: Option<(Vec<u32>, f32, f32)>,
    pub cycle_started: Option<std::time::Instant>,
    pub search_focused: bool,
    pub category: Option<azalea_registry::builtin::RecipeBookCategory>,
    pub page: usize,
    /// Server-provided display, including recipes not in the unlocked book.
    pub ghost_recipe: Option<RecipeDisplayData>,
    pub clicked_ui: bool,
    pub hovered_ui: bool,
    pub settings_loaded: bool,
    pub settings_type: Option<u32>,
    local_settings: [Option<(bool, bool)>; 4],
    pub settings_dirty: bool,
}

impl RecipeBookState {
    pub fn reset_menu(&mut self) {
        self.ghost_recipe = None;
        self.popup = None;
        self.search_focused = false;
        self.search_dragging = false;
        self.toggle_focused = false;
    }

    pub fn load_settings(&mut self, book_type: u32) {
        if self.settings_type == Some(book_type) && self.settings_loaded {
            return;
        }
        self.settings_type = Some(book_type);
        self.settings_loaded = false;
        if let Some(settings) = &self.settings {
            match book_type {
                0 => {
                    self.open = settings.gui_open;
                    self.craftable_only = settings.filtering_craftable;
                }
                1 => {
                    self.open = settings.furnace_gui_open;
                    self.craftable_only = settings.furnace_filtering_craftable;
                }
                2 => {
                    self.open = settings.blast_furnace_gui_open;
                    self.craftable_only = settings.blast_furnace_filtering_craftable;
                }
                3 => {
                    self.open = settings.smoker_gui_open;
                    self.craftable_only = settings.smoker_filtering_craftable;
                }
                _ => return,
            }
        } else if let Some((open, filtering)) = self
            .local_settings
            .get(book_type as usize)
            .copied()
            .flatten()
        {
            self.open = open;
            self.craftable_only = filtering;
        } else {
            self.open = false;
            self.craftable_only = false;
        }
        self.settings_loaded = true;
    }

    /// Keep the local settings model current; the server does not necessarily
    /// echo a settings packet after a client change.
    pub fn store_settings(&mut self, book_type: u32) {
        let Some(local) = self.local_settings.get_mut(book_type as usize) else {
            return;
        };
        *local = Some((self.open, self.craftable_only));
        let Some(settings) = &mut self.settings else {
            return;
        };
        match book_type {
            0 => {
                settings.gui_open = self.open;
                settings.filtering_craftable = self.craftable_only;
            }
            1 => {
                settings.furnace_gui_open = self.open;
                settings.furnace_filtering_craftable = self.craftable_only;
            }
            2 => {
                settings.blast_furnace_gui_open = self.open;
                settings.blast_furnace_filtering_craftable = self.craftable_only;
            }
            3 => {
                settings.smoker_gui_open = self.open;
                settings.smoker_filtering_craftable = self.craftable_only;
            }
            _ => {}
        }
    }

    pub fn wants_text_input(&self) -> bool {
        self.open && (self.search_focused || self.toggle_focused)
    }

    pub fn search_field(&mut self) -> &mut TextFieldState {
        self.search_field
            .get_or_insert_with(|| TextFieldState::new(50))
    }

    pub fn handle_text_events(
        &mut self,
        events: &[TextInputEvent],
        width_fn: &dyn Fn(&str) -> f32,
    ) {
        if !self.wants_text_input() {
            return;
        }
        for event in events {
            use winit::keyboard::KeyCode;
            if let TextInputEvent::Key { code, .. } = event {
                match code {
                    KeyCode::Escape => {
                        self.search_focused = false;
                        self.toggle_focused = false;
                        break;
                    }
                    KeyCode::Tab => {
                        self.toggle_focused = !self.toggle_focused;
                        self.search_focused = !self.toggle_focused;
                        if self.search_focused {
                            self.search_field().set_focused(true);
                        }
                        continue;
                    }
                    KeyCode::Enter | KeyCode::Space if self.toggle_focused => {
                        self.open = !self.open;
                        self.settings_dirty = true;
                        self.popup = None;
                        self.toggle_focused = false;
                        continue;
                    }
                    _ => {}
                }
            }
            if self.search_focused {
                self.search_field()
                    .handle(event, &mut SystemClipboard, 73.0, width_fn);
            }
        }
        let value = self.search_field().value().to_owned();
        if self.search != value {
            self.search = value;
            self.page = 0;
            self.popup = None;
        }
    }

    pub fn cycle_index(&mut self) -> usize {
        (self
            .cycle_started
            .get_or_insert_with(std::time::Instant::now)
            .elapsed()
            .as_millis()
            / 1500) as usize
    }

    /// Native packet includes a menu ID, not a book display ID. Ignore stale
    /// menus.
    pub fn receive_ghost(
        &mut self,
        container_id: i32,
        active_container_id: i32,
        display: RecipeDisplayData,
    ) {
        if container_id == active_container_id {
            self.ghost_recipe = Some(display);
        }
    }

    /// Category + optional group is the vanilla collection key. Zero on wire
    /// means absent; unrelated ungrouped recipes must remain separate buttons.
    pub fn collections(
        &self,
        categories: &[azalea_registry::builtin::RecipeBookCategory],
        columns: usize,
        rows: usize,
        furnace: bool,
        available: &[ItemStack],
    ) -> Vec<Vec<u32>> {
        let mut collections: Vec<Vec<u32>> = Vec::new();
        for category in categories {
            let mut grouped = BTreeMap::<u32, usize>::new();
            for (&id, display) in &self.displays {
                if self.categories.get(&id) != Some(category)
                    || !display_fits(display, columns, rows, furnace)
                {
                    continue;
                }
                let group = self.groups.get(&id).copied().unwrap_or(0);
                if group != 0
                    && let Some(&index) = grouped.get(&group)
                {
                    collections[index].push(id);
                } else {
                    if group != 0 {
                        grouped.insert(group, collections.len());
                    }
                    collections.push(vec![id]);
                }
            }
        }
        let query = self.search.to_lowercase();
        collections.retain(|ids| {
            query.is_empty()
                || ids.iter().any(|id| {
                    display_result(&self.displays[id]).is_some_and(|result| {
                        self.resolve(result).iter().any(|stack| {
                            stack.as_present().is_some_and(|stack| {
                                super::common::item_display_name(stack)
                                    .to_lowercase()
                                    .contains(&query)
                                    || stack.kind.to_string().to_lowercase().contains(&query)
                            })
                        })
                    })
                })
        });
        if self.craftable_only {
            for ids in &mut collections {
                ids.retain(|id| self.can_craft(*id, available) == Some(true));
            }
        }
        collections.retain(|ids| !ids.is_empty());
        collections
    }

    /// Reuse the slot-display unwrapping for results, popup ingredients and
    /// ghosts. ponytail: component-derived variants use the underlying
    /// stack; extend the existing item renderer when it supports
    /// potion/trim/component variants.
    pub fn resolve(&self, display: &SlotDisplayData) -> Vec<ItemStack> {
        use SlotDisplayData as D;
        match display {
            D::Empty => Vec::new(),
            D::AnyFuel => {
                // FuelValues.vanillaBurnTimes (26.2), with server-synced tag members.
                use ItemKind as I;
                let mut items = vec![
                    I::LavaBucket,
                    I::CoalBlock,
                    I::BlazeRod,
                    I::Coal,
                    I::Charcoal,
                    I::BambooMosaic,
                    I::BambooMosaicStairs,
                    I::BambooMosaicSlab,
                    I::NoteBlock,
                    I::Bookshelf,
                    I::ChiseledBookshelf,
                    I::Lectern,
                    I::Jukebox,
                    I::Chest,
                    I::TrappedChest,
                    I::CraftingTable,
                    I::DaylightDetector,
                    I::Bow,
                    I::FishingRod,
                    I::Ladder,
                    I::WoodenShovel,
                    I::WoodenSword,
                    I::WoodenSpear,
                    I::WoodenHoe,
                    I::WoodenAxe,
                    I::WoodenPickaxe,
                    I::Stick,
                    I::Bowl,
                    I::DriedKelpBlock,
                    I::Crossbow,
                    I::Bamboo,
                    I::DeadBush,
                    I::ShortDryGrass,
                    I::TallDryGrass,
                    I::Scaffolding,
                    I::Loom,
                    I::Barrel,
                    I::CartographyTable,
                    I::FletchingTable,
                    I::SmithingTable,
                    I::Composter,
                    I::Azalea,
                    I::FloweringAzalea,
                    I::MangroveRoots,
                    I::LeafLitter,
                ];
                for tag in [
                    "logs",
                    "bamboo_blocks",
                    "planks",
                    "wooden_stairs",
                    "wooden_slabs",
                    "wooden_trapdoors",
                    "wooden_pressure_plates",
                    "wooden_shelves",
                    "wooden_fences",
                    "fence_gates",
                    "banners",
                    "signs",
                    "hanging_signs",
                    "wooden_doors",
                    "boats",
                    "wool",
                    "wooden_buttons",
                    "saplings",
                    "wool_carpets",
                ] {
                    if let Some(tag) = self.item_tags.get(&format!("minecraft:{tag}")) {
                        items.extend(tag);
                    }
                }
                let non_flammable = self.item_tags.get("minecraft:non_flammable_wood");
                items.retain(|item| !non_flammable.is_some_and(|tag| tag.contains(item)));
                let mut seen = std::collections::BTreeSet::new();
                items
                    .into_iter()
                    .filter(|item| seen.insert(*item))
                    .map(ItemStack::from)
                    .collect()
            }
            D::Item(d) => vec![ItemStack::from(d.item)],
            D::ItemStack(d) => d
                .stack
                .is_present()
                .then(|| d.stack.clone())
                .into_iter()
                .collect(),
            D::Tag(d) => self
                .item_tags
                .get(&d.tag.to_string())
                .into_iter()
                .flatten()
                .copied()
                .map(ItemStack::from)
                .collect(),
            D::Composite(d) => d.contents.iter().flat_map(|d| self.resolve(d)).collect(),
            D::WithRemainder(d) => self.resolve(&d.input),
            D::WithAnyPotion(d) => self.resolve(&d.contents),
            D::OnlyWithComponent(d) => self.resolve(&d.contents),
            D::Dyed(d) => self.resolve(&d.target),
            D::SmithingTrim(d) => self.resolve(&d.base),
        }
    }

    pub fn add(&mut self, packet: ClientboundRecipeBookAdd) {
        self.popup = None;
        if packet.replace {
            self.displays.clear();
            self.groups.clear();
            self.categories.clear();
            self.flags.clear();
            self.requirements.clear();
            self.ghost_recipe = None;
        }
        for entry in packet.entries {
            let id = entry.contents.id;
            self.displays.insert(id, entry.contents.display);
            self.requirements
                .insert(id, entry.contents.crafting_requirements);
            self.groups.insert(id, entry.contents.group);
            self.categories.insert(id, entry.contents.category);
            self.flags.insert(id, entry.flags);
        }
    }

    pub fn update_item_tags(&mut self, tags: &azalea_protocol::common::tags::TagMap) {
        let Some(items) = tags
            .0
            .iter()
            .find(|(key, _)| key.to_string() == "minecraft:item")
        else {
            return;
        };
        self.item_tags = items
            .1
            .iter()
            .map(|tag| {
                (
                    tag.name.to_string(),
                    tag.elements
                        .iter()
                        .filter_map(|&id| u32::try_from(id).ok().and_then(ItemKind::from_u32))
                        .collect(),
                )
            })
            .collect();
    }

    /// None means the server did not supply requirements or a named tag is
    /// unknown.
    pub fn can_craft(&self, id: u32, available: &[ItemStack]) -> Option<bool> {
        let requirements = self.requirements.get(&id)?.as_ref()?;
        let mut choices = Vec::with_capacity(requirements.len());
        for ingredient in requirements {
            let items = match &ingredient.allowed {
                HolderSet::Direct { contents } => contents.clone(),
                HolderSet::Named { key, .. } => self.item_tags.get(&key.to_string())?.clone(),
            };
            choices.push(items);
        }
        let mut counts = BTreeMap::<ItemKind, i32>::new();
        for stack in available {
            if let Some(item) = stack.as_present() {
                *counts.entry(item.kind).or_default() += item.count.max(0);
            }
        }
        fn assign(choices: &[Vec<ItemKind>], counts: &mut BTreeMap<ItemKind, i32>) -> bool {
            let Some((first, rest)) = choices.split_first() else {
                return true;
            };
            for item in first {
                if let Some(count) = counts.get_mut(item) {
                    if *count > 0 {
                        *count -= 1;
                        if assign(rest, counts) {
                            return true;
                        }
                        *counts.get_mut(item).unwrap() += 1;
                    }
                }
            }
            false
        }
        Some(assign(&choices, &mut counts))
    }

    pub fn remove(&mut self, ids: &[u32]) {
        self.popup = None;
        for id in ids {
            self.displays.remove(id);
            self.groups.remove(id);
            self.categories.remove(id);
            self.flags.remove(id);
            self.requirements.remove(id);
        }
    }
}

pub fn display_result(display: &RecipeDisplayData) -> Option<&SlotDisplayData> {
    match display {
        RecipeDisplayData::Shaped(d) => Some(&d.result),
        RecipeDisplayData::Shapeless(d) => Some(&d.result),
        RecipeDisplayData::Furnace(d) => Some(&d.result),
        _ => None,
    }
}

pub fn display_fits(
    display: &RecipeDisplayData,
    columns: usize,
    rows: usize,
    furnace: bool,
) -> bool {
    match display {
        RecipeDisplayData::Shaped(d) => {
            !furnace
                && d.width > 0
                && d.height > 0
                && d.width as usize <= columns
                && d.height as usize <= rows
                && d.ingredients.len() <= d.width as usize * d.height as usize
        }
        RecipeDisplayData::Shapeless(d) => !furnace && d.ingredients.len() <= columns * rows,
        RecipeDisplayData::Furnace(_) => furnace,
        _ => false,
    }
}

/// PlaceRecipeHelper centers only dimensions smaller than half the grid.
/// Returned indices are input-grid indices, not menu slot numbers.
pub fn ingredient_positions(
    display: &RecipeDisplayData,
    columns: usize,
    rows: usize,
) -> Vec<(usize, &SlotDisplayData)> {
    match display {
        RecipeDisplayData::Shaped(d) if display_fits(display, columns, rows, false) => {
            let w = d.width as usize;
            let h = d.height as usize;
            let x = if w * 2 < columns {
                (columns - w) / 2
            } else {
                0
            };
            let y = if h * 2 < rows { (rows - h) / 2 } else { 0 };
            d.ingredients
                .iter()
                .enumerate()
                .map(|(i, ingredient)| ((y + i / w) * columns + x + i % w, ingredient))
                .collect()
        }
        RecipeDisplayData::Shapeless(d) if display_fits(display, columns, rows, false) => {
            d.ingredients.iter().enumerate().collect()
        }
        RecipeDisplayData::Furnace(d) => vec![(columns + 1, &d.ingredient)],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shaped(width: u32, height: u32) -> RecipeDisplayData {
        RecipeDisplayData::Shaped(
            azalea_protocol::common::recipe::ShapedCraftingRecipeDisplay {
                width,
                height,
                ingredients: vec![
                    SlotDisplayData::Item(
                        azalea_protocol::common::recipe::ItemSlotDisplay {
                            item: ItemKind::Stone
                        }
                    );
                    (width * height) as usize
                ],
                result: SlotDisplayData::ItemStack(
                    azalea_protocol::common::recipe::ItemStackSlotDisplay {
                        stack: ItemStack::new(ItemKind::Stone, 4),
                    },
                ),
                crafting_station: SlotDisplayData::Empty,
            },
        )
    }

    #[test]
    fn collections_share_only_category_and_nonzero_group_and_filter_to_grid() {
        use azalea_registry::builtin::RecipeBookCategory as C;
        let mut book = RecipeBookState::default();
        for (id, group, category, display) in [
            (1, 8, C::CraftingMisc, shaped(1, 1)),
            (2, 8, C::CraftingMisc, shaped(2, 2)),
            (3, 0, C::CraftingMisc, shaped(1, 1)),
            (4, 0, C::CraftingMisc, shaped(1, 1)),
            (5, 8, C::CraftingMisc, shaped(3, 1)),
            (6, 8, C::CraftingBuildingBlocks, shaped(1, 1)),
            (
                7,
                0,
                C::CraftingMisc,
                RecipeDisplayData::Shapeless(
                    azalea_protocol::common::recipe::ShapelessCraftingRecipeDisplay {
                        ingredients: vec![SlotDisplayData::Empty; 5],
                        result: SlotDisplayData::Empty,
                        crafting_station: SlotDisplayData::Empty,
                    },
                ),
            ),
        ] {
            book.displays.insert(id, display);
            book.groups.insert(id, group);
            book.categories.insert(id, category);
        }
        let categories = [C::CraftingMisc, C::CraftingBuildingBlocks];
        assert_eq!(
            book.collections(&categories, 2, 2, false, &[]),
            vec![vec![1, 2], vec![3], vec![4], vec![6]]
        );
        assert_eq!(
            book.collections(&categories, 3, 3, false, &[]),
            vec![vec![1, 2, 5], vec![3], vec![4], vec![7], vec![6]]
        );
        book.search = "minecraft:stone".into();
        assert_eq!(book.collections(&categories, 2, 2, false, &[]).len(), 4);
        book.search = "no_such_item".into();
        assert!(book.collections(&categories, 2, 2, false, &[]).is_empty());
        book.search.clear();
        book.craftable_only = true;
        book.requirements.insert(2, Some(vec![]));
        assert_eq!(
            book.collections(&categories, 2, 2, false, &[]),
            vec![vec![2]]
        );
    }

    #[test]
    fn server_ghost_requires_matching_menu_and_centering_matches_place_recipe_helper() {
        let mut book = RecipeBookState::default();
        book.receive_ghost(4, 3, shaped(1, 1));
        assert!(book.ghost_recipe.is_none());
        book.receive_ghost(3, 3, shaped(1, 1));
        assert_eq!(
            ingredient_positions(book.ghost_recipe.as_ref().unwrap(), 3, 3)[0].0,
            4
        );
        book.receive_ghost(4, 3, shaped(2, 2));
        assert_eq!(book.ghost_recipe, Some(shaped(1, 1)));
        assert_eq!(ingredient_positions(&shaped(1, 1), 2, 2)[0].0, 0);
        assert_eq!(
            ingredient_positions(&shaped(2, 2), 3, 3)
                .iter()
                .map(|(slot, _)| *slot)
                .collect::<Vec<_>>(),
            vec![0, 1, 3, 4]
        );
        assert!(ingredient_positions(&shaped(3, 1), 2, 2).is_empty());
    }

    #[test]
    fn search_uses_existing_selection_cursor_and_scroll_model() {
        use winit::keyboard::KeyCode;

        use super::super::text_edit::{KeyMods, TextInputEvent};
        let mut book = RecipeBookState {
            open: true,
            search_focused: true,
            ..Default::default()
        };
        let width = |s: &str| s.chars().count() as f32 * 6.0;
        book.handle_text_events(
            &[TextInputEvent::Commit("abcdefghijklmnopqrstuvwxyz".into())],
            &width,
        );
        assert!(
            book.search_field()
                .render_info(73.0, true, &width)
                .display_start
                > 0
        );
        book.handle_text_events(
            &[
                TextInputEvent::Key {
                    code: KeyCode::Home,
                    mods: KeyMods {
                        shift: true,
                        ctrl: false,
                        alt: false,
                        super_key: false,
                    },
                },
                TextInputEvent::Commit("石".into()),
            ],
            &width,
        );
        assert_eq!(book.search, "石");
        assert_eq!(book.search_field().cursor(), "石".len());
    }

    #[test]
    fn requirements_consume_distinct_units_and_unknown_is_not_craftable() {
        let mut book = RecipeBookState::default();
        let stone = Ingredient {
            allowed: vec![ItemKind::Stone].into(),
        };
        book.requirements
            .insert(1, Some(vec![stone.clone(), stone]));
        assert_eq!(
            book.can_craft(1, &[ItemStack::new(ItemKind::Stone, 1)]),
            Some(false)
        );
        assert_eq!(
            book.can_craft(1, &[ItemStack::new(ItemKind::Stone, 2)]),
            Some(true)
        );
        book.requirements.insert(2, None);
        assert_eq!(
            book.can_craft(2, &[ItemStack::new(ItemKind::Stone, 2)]),
            None
        );
    }
}
