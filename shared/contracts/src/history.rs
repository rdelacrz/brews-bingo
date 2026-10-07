//! Immutable DO-069 History projections; no final-view or admission fields.
use brews_domain::{
    games::{BoardCell, GameCode, GameState},
    ids::{AccountId, GameId, PlayerId},
};
use serde::{Deserialize, Serialize};

pub const HISTORY_DEFAULT_LIMIT: u32 = 20;
pub const HISTORY_MAX_LIMIT: u32 = 50;

macro_rules! closed {
    ($name:ident { $($(#[$meta:meta])* $field:ident: $ty:ty),* $(,)? }) => {
        #[derive(Clone, Debug, Serialize)]
        pub struct $name { $($(#[$meta])* pub $field: $ty),* }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Wire { $($(#[$meta])* $field: $ty),* }
                let w: Wire = crate::games::object(d)?;
                Ok(Self { $($field: w.$field),* })
            }
        }
    };
}
closed!(HistoryWinner {
    player_id: PlayerId,
    alias: String
});
closed!(HistoryPlayer { player_id: PlayerId, alias: String, side_length: u8,
    #[serde(deserialize_with = "history_cells")]
    cells: Vec<BoardCell> });
fn history_cells<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<BoardCell>, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Cell {
        #[serde(deserialize_with = "crate::games::object")]
        position: brews_domain::games::CellPosition,
        #[serde(deserialize_with = "crate::games::object")]
        kind: brews_domain::games::BoardCellKind,
        is_matched: bool,
    }
    struct MapCell(Cell);
    impl<'de> Deserialize<'de> for MapCell {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            crate::games::object(d).map(Self)
        }
    }
    let cells = Vec::<MapCell>::deserialize(d)?;
    if cells.len() > brews_domain::games::BOARD_CELLS_MAX {
        return Err(serde::de::Error::custom("History cell bound"));
    }
    Ok(cells
        .into_iter()
        .map(|MapCell(c)| BoardCell {
            position: c.position,
            kind: c.kind,
            is_matched: c.is_matched,
        })
        .collect())
}
closed!(HistorySummary {
    game_id: GameId, game_code: GameCode, designated_host_id: AccountId,
    outcome: GameState, started_at: i64, ended_at: i64, expires_at: i64,
    #[serde(deserialize_with = "crate::games::nullable_value")]
    winner: Option<HistoryWinner>,
});
closed!(History {
    game_id: GameId, game_code: GameCode, designated_host_id: AccountId,
    outcome: GameState, started_at: i64, ended_at: i64, expires_at: i64,
    #[serde(deserialize_with = "crate::games::nullable_value")]
    winner: Option<HistoryWinner>,
    ordered_calls: Vec<String>, players: Vec<HistoryPlayer>,
});
closed!(HistoryDetail { history: History });
closed!(HistoryPage {
    games: Vec<HistorySummary>,
    #[serde(deserialize_with = "crate::games::nullable_value")]
    next_cursor: Option<String>,
});
impl History {
    pub fn summary(&self) -> HistorySummary {
        HistorySummary {
            game_id: self.game_id,
            game_code: self.game_code.clone(),
            designated_host_id: self.designated_host_id,
            outcome: self.outcome,
            started_at: self.started_at,
            ended_at: self.ended_at,
            expires_at: self.expires_at,
            winner: self.winner.clone(),
        }
    }
}
