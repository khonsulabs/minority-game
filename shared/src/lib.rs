use bonsaidb::core::custom_api::{CustomApi, Infallible};
use serde::{Deserialize, Serialize};

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

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Eq, PartialEq)]
pub enum Choice {
    GoOut,
    StayIn,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Response {
    Welcome {
        player_id: u64,
        happiness: f32,
    },

    ChoiceSet(Choice),

    RoundPending {
        seconds_remaining: u32,
        number_of_players: u32,
        current_rank: u32,
        number_of_tells: u32,
        tells_going_out: u32,
    },

    RoundComplete {
        won: bool,
        happiness: f32,
        current_rank: u32,
        number_of_players: u32,
        number_of_liars: u32,
        number_of_tells: u32,
    },
}

#[derive(Debug)]
pub enum Api {}

impl CustomApi for Api {
    type Error = Infallible;
    type Request = Request;
    type Response = Response;
}

pub fn whole_percent(happiness: f32) -> u32 {
    (happiness * 100.).round() as u32
}
