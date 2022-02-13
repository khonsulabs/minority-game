use minority_game_shared::whole_percent;
use serde::{Deserialize, Serialize};

use crate::schema::PlayerStats;

#[derive(Serialize, Deserialize, Debug)]
struct RankedPlayer {
    id: u64,
    rank: u32,
    happiness: u32,
    times_went_out: u32,
    times_stayed_in: u32,
}

impl RankedPlayer {
    pub fn from_player_stats(player: &PlayerStats, id: u64, index: usize) -> Self {
        Self {
            id,
            rank: index as u32 + 1,
            happiness: whole_percent(player.happiness),
            times_stayed_in: player.times_stayed_in,
            times_went_out: player.times_went_out,
        }
    }
}
