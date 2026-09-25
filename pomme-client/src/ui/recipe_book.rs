use std::collections::BTreeMap;

use azalea_protocol::common::recipe::RecipeDisplayData;
use azalea_protocol::packets::game::c_recipe_book_add::ClientboundRecipeBookAdd;
use azalea_protocol::packets::game::c_recipe_book_settings::RecipeBookSettings;
use azalea_protocol::packets::game::c_update_recipes::ClientboundUpdateRecipes;

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
    pub settings: Option<RecipeBookSettings>,
    pub updates: Option<ClientboundUpdateRecipes>,
    pub open: bool,
    pub craftable_only: bool,
    pub search: String,
    pub search_focused: bool,
    pub category: Option<azalea_registry::builtin::RecipeBookCategory>,
    pub page: usize,
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
        }
        for entry in packet.entries {
            let id = entry.contents.id;
            self.displays.insert(id, entry.contents.display);
            self.groups.insert(id, entry.contents.group);
            self.categories.insert(id, entry.contents.category);
            self.flags.insert(id, entry.flags);
        }
    }

    pub fn remove(&mut self, ids: &[u32]) {
        for id in ids {
            self.displays.remove(id);
            self.groups.remove(id);
            self.categories.remove(id);
            self.flags.remove(id);
        }
    }
}
