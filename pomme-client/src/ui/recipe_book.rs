use std::collections::BTreeMap;

use azalea_inventory::ItemStack;
use azalea_protocol::common::recipe::{Ingredient, RecipeDisplayData};
use azalea_protocol::packets::game::c_recipe_book_add::ClientboundRecipeBookAdd;
use azalea_protocol::packets::game::c_recipe_book_settings::RecipeBookSettings;
use azalea_protocol::packets::game::c_update_recipes::ClientboundUpdateRecipes;
use azalea_registry::builtin::ItemKind;
use azalea_registry::{HolderSet, Registry};

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
    pub search_focused: bool,
    pub category: Option<azalea_registry::builtin::RecipeBookCategory>,
    pub page: usize,
    /// Uncraftable recipe selected for a translucent ingredient preview.
    pub ghost_recipe: Option<u32>,
    pub clicked_ui: bool,
    pub settings_loaded: bool,
    pub settings_type: Option<u32>,
    local_settings: [Option<(bool, bool)>; 4],
    pub settings_dirty: bool,
}

impl RecipeBookState {
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
        self.open && self.search_focused
    }

    pub fn handle_text_events(&mut self, events: &[crate::ui::text_edit::TextInputEvent]) {
        if !self.wants_text_input() {
            return;
        }
        for event in events {
            match event {
                crate::ui::text_edit::TextInputEvent::Char(c)
                    if !c.is_control() && self.search.chars().count() < 50 =>
                {
                    self.search.push(*c);
                    self.page = 0;
                }
                crate::ui::text_edit::TextInputEvent::Key {
                    code: winit::keyboard::KeyCode::Backspace,
                    ..
                } => {
                    self.search.pop();
                    self.page = 0;
                }
                crate::ui::text_edit::TextInputEvent::Key {
                    code: winit::keyboard::KeyCode::Escape,
                    ..
                } => {
                    self.search_focused = false;
                }
                _ => {}
            }
        }
    }

    pub fn add(&mut self, packet: ClientboundRecipeBookAdd) {
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
        for id in ids {
            self.displays.remove(id);
            self.groups.remove(id);
            self.categories.remove(id);
            self.flags.remove(id);
            self.requirements.remove(id);
            if self.ghost_recipe == Some(*id) {
                self.ghost_recipe = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
