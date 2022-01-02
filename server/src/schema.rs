use bonsaidb::core::schema::{
    Collection, CollectionDocument, CollectionName, CollectionView, DefaultSerialization,
    DefaultViewSerialization, InvalidNameError, MapResult, Name, Schema, SchemaName, Schematic,
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

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct Player {
    pub choice: Option<Choice>,
    #[serde(default)]
    pub tell: Option<Choice>,
    pub stats: PlayerStats,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PlayerStats {
    pub happiness: f32,
    pub times_went_out: u32,
    pub times_stayed_in: u32,
    pub times_lied: u32,
    pub times_told_truth: u32,
}

impl Default for PlayerStats {
    fn default() -> Self {
        Self {
            happiness: 0.5,
            times_went_out: 0,
            times_stayed_in: 0,
            times_lied: 0,
            times_told_truth: 0,
        }
    }
}

impl PlayerStats {
    pub fn score(&self) -> u32 {
        let total_games = self.times_stayed_in + self.times_went_out;
        (self.happiness * total_games as f32) as u32
    }
}

impl Collection for Player {
    fn collection_name() -> Result<CollectionName, InvalidNameError> {
        CollectionName::new("minority-game", "player")
    }

    fn define_views(schema: &mut Schematic) -> Result<(), bonsaidb::core::Error> {
        schema.define_view(PlayerByScore)?;
        Ok(())
    }
}

impl DefaultSerialization for Player {}

#[derive(Debug)]
pub struct PlayerByScore;

impl CollectionView for PlayerByScore {
    type Collection = Player;
    type Key = u32;
    type Value = PlayerStats;

    fn version(&self) -> u64 {
        2
    }

    fn name(&self) -> Result<Name, InvalidNameError> {
        Name::new("by-score")
    }

    fn map(
        &self,
        player: CollectionDocument<Self::Collection>,
    ) -> MapResult<Self::Key, Self::Value> {
        Ok(player.emit_key_and_value(player.contents.stats.score(), player.contents.stats.clone()))
    }
}

impl DefaultViewSerialization for PlayerByScore {}
