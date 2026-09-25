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
    pub settings: Option<RecipeBookSettings>,
    pub updates: Option<ClientboundUpdateRecipes>,
}

impl RecipeBookState {
    pub fn add(&mut self, packet: ClientboundRecipeBookAdd) {
        if packet.replace {
            self.displays.clear();
        }
        for entry in packet.entries {
            self.displays
                .insert(entry.contents.id, entry.contents.display);
        }
    }

    pub fn remove(&mut self, ids: &[u32]) {
        for id in ids {
            self.displays.remove(id);
        }
    }
}
