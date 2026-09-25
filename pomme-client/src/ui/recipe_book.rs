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
}

impl RecipeBookState {
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
