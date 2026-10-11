//! Allocation-conscious, bounded discovery projections; discovery grants no access.
use brews_domain::{
    games::{GameCode, GameState},
    ids::{AccountId, GameId},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameSlot {
    Free,
    Preparing,
    Occupied,
    Releasing,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentGameSummary {
    pub game_id: GameId,
    pub state: GameState,
    pub designated_host_id: AccountId,
    pub created_at: i64,
    #[serde(deserialize_with = "Option::<i64>::deserialize")]
    pub started_at: Option<i64>,
    #[serde(deserialize_with = "Option::<i64>::deserialize")]
    pub idle_due: Option<i64>,
    #[serde(deserialize_with = "Option::<GameCode>::deserialize")]
    pub game_code: Option<GameCode>,
    pub may_operate: bool,
    pub view_revision: u64,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentGame {
    pub slot: GameSlot,
    #[serde(deserialize_with = "crate::games::nullable_value")]
    pub game: Option<CurrentGameSummary>,
}
#[derive(Serialize)]
pub struct CurrentGames {
    pub slot: GameSlot,
    /// The global reservation limits this array to at most one game.
    pub games: Vec<CurrentGameSummary>,
    pub next_cursor: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameEntry {
    pub game_id: GameId,
    pub game_code: GameCode,
    pub state: GameState,
    pub can_join_player: bool,
    pub can_join_spectator: bool,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "view", rename_all = "snake_case", deny_unknown_fields)]
pub enum GameAvailability {
    Entry {
        state: GameState,
        can_join_player: bool,
        can_join_spectator: bool,
    },
    Player {
        state: GameState,
        can_join_player: bool,
        can_join_spectator: bool,
    },
    Account {
        state: GameState,
        can_join_player: bool,
        can_join_spectator: bool,
        player_count: u8,
        connected_player_count: u8,
        player_capacity: u8,
        spectator_count: u8,
        spectator_capacity: u8,
        can_start: bool,
    },
}
