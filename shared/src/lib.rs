use bonsaidb::core::custom_api::{CustomApi, Infallible};
use serde::{Deserialize, Serialize};

/// A game network request.
#[derive(Serialize, Deserialize, Debug)]
#[cfg_attr(feature = "actionable-traits", derive(actionable::Actionable))]
pub enum Request {
    /// Set the current choice.
    #[cfg_attr(feature = "actionable-traits", actionable(protection = "none"))]
    SetChoice(Choice),
    /// Set the current tell.
    #[cfg_attr(feature = "actionable-traits", actionable(protection = "none"))]
    SetTell(Choice),
}

/// A player's choice in the game.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Eq, PartialEq)]
pub enum Choice {
    GoOut,
    StayIn,
}

/// A game network response.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Response {
    /// The server has set up our player record.
    Welcome { player_id: u64, happiness: f32 },

    /// Our choice has been set.
    ChoiceSet(Choice),

    /// A round is pending.
    RoundPending {
        seconds_remaining: u32,
        number_of_players: u32,
        current_rank: u32,
        number_of_tells: u32,
        tells_going_out: u32,
    },

    /// A round has finished.
    RoundComplete {
        /// The player's happiness has gone up this round.
        won: bool,
        happiness: f32,
        current_rank: u32,
        number_of_players: u32,
        number_of_liars: u32,
        number_of_tells: u32,
    },
}

/// The [`CustomApi`] for the game.
#[derive(Debug)]
pub enum Api {}

impl CustomApi for Api {
    type Error = Infallible;
    type Request = Request;
    type Response = Response;
}

/// Converts a `percent` to its nearest whole number.
pub fn whole_percent(percent: f32) -> u32 {
    (percent * 100.).round() as u32
}
