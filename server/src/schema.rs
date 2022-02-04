use bonsaidb::core::{
    document::CollectionDocument,
    schema::{
        Collection, CollectionName, CollectionViewSchema, DefaultSerialization,
        DefaultViewSerialization, Name, Schema, SchemaName, Schematic, View, ViewMapResult,
    },
};
use minority_game_shared::Choice;
use serde::{Deserialize, Serialize};

const AUTHORITY: &str = "minority-game";

#[derive(Debug)]
pub enum GameSchema {}

impl Schema for GameSchema {
    fn schema_name() -> SchemaName {
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
    fn collection_name() -> CollectionName {
        CollectionName::new("minority-game", "player")
    }

    fn define_views(schema: &mut Schematic) -> Result<(), bonsaidb::core::Error> {
        schema.define_view(PlayerByScore)?;
        Ok(())
    }
}

impl DefaultSerialization for Player {}

#[derive(Debug, Clone)]
pub struct PlayerByScore;

impl View for PlayerByScore {
    type Collection = Player;
    type Key = u32;
    type Value = PlayerStats;

    fn name(&self) -> Name {
        Name::new("by-score")
    }
}

impl CollectionViewSchema for PlayerByScore {
    type View = Self;

    fn version(&self) -> u64 {
        2
    }

    fn map(
        &self,
        player: CollectionDocument<<Self::View as View>::Collection>,
    ) -> ViewMapResult<Self::View> {
        Ok(player
            .header
            .emit_key_and_value(player.contents.stats.score(), player.contents.stats))
    }
}

impl DefaultViewSerialization for PlayerByScore {}
