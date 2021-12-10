use bonsaidb::core::schema::{
    Collection, CollectionName, InvalidNameError, Schema, SchemaName, Schematic,
};
use minority_game_shared::Choice;
use serde::{Deserialize, Serialize};

const AUTHORITY: &str = "minority-game";

#[derive(Debug)]
pub enum GameSchema {}

impl Schema for GameSchema {
    fn schema_name() -> Result<SchemaName, InvalidNameError> {
        SchemaName::new(AUTHORITY, "game")
    }

    fn define_collections(schema: &mut Schematic) -> Result<(), bonsaidb::core::Error> {
        schema.define_collection::<Player>()?;

        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Player {
    pub happiness: f32,
    pub choice: Option<Choice>,
    pub times_went_out: u32,
    pub times_stayed_in: u32,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            happiness: 0.5,
            choice: None,
            times_went_out: 0,
            times_stayed_in: 0,
        }
    }
}

impl Collection for Player {
    fn collection_name() -> Result<CollectionName, InvalidNameError> {
        CollectionName::new("minority-game", "player")
    }

    fn define_views(_schema: &mut Schematic) -> Result<(), bonsaidb::core::Error> {
        Ok(())
    }
}
