//! Allocation-conscious, bounded projections for passive game reads.
use super::*;
use brews_domain::{games::BOARD_SIDE_MAX, ids::SpectatorId};

const QUALIFYING_LINES_MAX: usize = 2 * BOARD_SIDE_MAX as usize + 2;
pub const READ_PLAYERS_MAX: usize = brews_domain::games::PLAYER_CAPACITY_MAX as usize;
pub const READ_VALUES_MAX: usize = NUMERIC_UPPER_BOUND_MAX as usize;
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadKind {
    Game,
    Membership,
    Players,
    Calls,
    RemainingValues,
    OwnBoard,
    Boards,
    Qualification,
    Result,
}
impl ReadKind {
    pub const fn account_only(self) -> bool {
        matches!(self, Self::Players | Self::RemainingValues | Self::Boards)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MembershipAccess {
    OriginalLive,
    FinalOnly,
}
map_enum!(pub Membership, "role", {
    Player { player_id:PlayerId, #[serde(deserialize_with="canonical_alias")] alias:String, access:MembershipAccess, #[serde(deserialize_with="safe_timestamp")] session_expires_at:i64, recovery_enabled:bool },
    Spectator { spectator_id:SpectatorId, access:MembershipAccess, #[serde(deserialize_with="safe_timestamp")] session_expires_at:i64 },
});
map_struct!(pub GameDetail { game:GameView });
map_struct!(pub MembershipRead { membership:Membership, #[serde(deserialize_with="safe_revision")] view_revision:u64 });
map_struct!(pub RosterPlayer { player_id:PlayerId, #[serde(deserialize_with="canonical_alias")] alias:String, connected:bool, membership_retained:bool, #[serde(deserialize_with="nullable_value")] left_during_play:Option<bool> });
map_struct!(pub PlayerRoster { players:Vec<RosterPlayer>, #[serde(deserialize_with="safe_revision")] view_revision:u64 }; validate);
map_struct!(pub CallsRead { calls:Vec<CallView>, #[serde(deserialize_with="nullable_value")] latest:Option<CallView>, #[serde(deserialize_with="safe_revision")] view_revision:u64 }; validate);
map_struct!(pub RemainingValues { remaining_values:Vec<String>, remaining_count:u32, exhausted:bool, #[serde(deserialize_with="safe_revision")] view_revision:u64 }; validate);
map_struct!(pub OwnBoard { #[serde(deserialize_with="nullable_value")] board:Option<BoardView>, #[serde(deserialize_with="safe_revision")] view_revision:u64 }; validate);
map_struct!(pub InspectedBoard { player_id:PlayerId, #[serde(deserialize_with="canonical_alias")] alias:String, #[serde(deserialize_with="nullable_value")] board:Option<BoardView> });
map_struct!(pub BoardsRead { boards:Vec<InspectedBoard>, #[serde(deserialize_with="safe_revision")] view_revision:u64 }; validate);
map_struct!(pub QualifiedPlayer { player_id:PlayerId, #[serde(deserialize_with="canonical_alias")] alias:String, #[serde(deserialize_with="completed_lines")] qualifying_lines:Vec<CompletedLine> });
map_enum!(pub QualificationRead, "role", {
    Account { qualified_players:Vec<QualifiedPlayer>, #[serde(deserialize_with="safe_revision")] view_revision:u64 },
    Player { qualified:bool, #[serde(deserialize_with="completed_lines")] qualifying_lines:Vec<CompletedLine>, #[serde(deserialize_with="safe_revision")] view_revision:u64 },
}; validate);
map_struct!(pub ResultRead { result:GameView }; validate);
fn bounded_ids<'a>(
    ids: impl Iterator<Item = &'a PlayerId>,
    len: usize,
) -> Result<(), GameDecodeError> {
    if len > READ_PLAYERS_MAX {
        return Err(GameDecodeError);
    }
    let mut previous = None;
    for id in ids {
        if previous.is_some_and(|p| p >= id) {
            return Err(GameDecodeError);
        }
        previous = Some(id);
    }
    Ok(())
}
fn validate_board(board: &BoardView) -> Result<(), GameDecodeError> {
    let calls = board
        .cells
        .iter()
        .filter_map(|c| match &c.kind {
            BoardCellKind::Value(v) if c.is_matched => Some(CallView {
                sequence_no: 1,
                value: v.clone(),
            }),
            _ => None,
        })
        .collect::<Vec<_>>();
    super::validate_final_board("Board", board, &calls)
}
impl PlayerRoster {
    pub fn validate(&self) -> Result<(), GameDecodeError> {
        bounded_ids(
            self.players.iter().map(|p| &p.player_id),
            self.players.len(),
        )?;
        Ok(())
    }
}
impl CallsRead {
    pub fn validate(&self) -> Result<(), GameDecodeError> {
        validate_call_ledger(&self.calls, NUMERIC_UPPER_BOUND_MAX)?;
        if serde_json::to_vec(&self.latest).map_err(|_| GameDecodeError)?
            != serde_json::to_vec(&self.calls.last()).map_err(|_| GameDecodeError)?
        {
            return Err(GameDecodeError);
        }
        Ok(())
    }
}
impl RemainingValues {
    pub fn validate(&self) -> Result<(), GameDecodeError> {
        if self.remaining_values.len() > READ_VALUES_MAX
            || self.remaining_count as usize != self.remaining_values.len()
            || self.exhausted != self.remaining_values.is_empty()
        {
            return Err(GameDecodeError);
        }
        let mut previous = 0;
        for value in &self.remaining_values {
            if !valid_call_value(value) {
                return Err(GameDecodeError);
            }
            let n = value.parse::<u32>().map_err(|_| GameDecodeError)?;
            if n <= previous {
                return Err(GameDecodeError);
            }
            previous = n;
        }
        Ok(())
    }
}
impl OwnBoard {
    pub fn validate(&self) -> Result<(), GameDecodeError> {
        if let Some(b) = &self.board {
            validate_board(b)?;
        }
        Ok(())
    }
}
impl BoardsRead {
    pub fn validate(&self) -> Result<(), GameDecodeError> {
        bounded_ids(self.boards.iter().map(|p| &p.player_id), self.boards.len())?;
        for p in &self.boards {
            if let Some(b) = &p.board {
                validate_board(b)?;
            }
        }
        Ok(())
    }
}
impl QualificationRead {
    pub fn validate(&self) -> Result<(), GameDecodeError> {
        match self {
            Self::Account {
                qualified_players, ..
            } => {
                bounded_ids(
                    qualified_players.iter().map(|p| &p.player_id),
                    qualified_players.len(),
                )?;
                for p in qualified_players {
                    if p.qualifying_lines.is_empty() {
                        return Err(GameDecodeError);
                    }
                    validate_qualifying_lines(&p.qualifying_lines)?;
                }
            }
            Self::Player {
                qualified,
                qualifying_lines,
                ..
            } => {
                if *qualified == qualifying_lines.is_empty() {
                    return Err(GameDecodeError);
                }
                validate_qualifying_lines(qualifying_lines)?;
            }
        }
        Ok(())
    }
}
fn validate_qualifying_lines(lines: &[CompletedLine]) -> Result<(), GameDecodeError> {
    if lines.len() > QUALIFYING_LINES_MAX {
        return Err(GameDecodeError);
    }
    let mut previous = None;
    for line in lines {
        let key = match *line {
            CompletedLine::Row(index) if (1..=BOARD_SIDE_MAX).contains(&index) => (0, index),
            CompletedLine::Column(index) if (1..=BOARD_SIDE_MAX).contains(&index) => (1, index),
            CompletedLine::MainDiagonal => (2, 0),
            CompletedLine::AntiDiagonal => (3, 0),
            _ => return Err(GameDecodeError),
        };
        if previous.is_some_and(|value| value >= key) {
            return Err(GameDecodeError);
        }
        previous = Some(key);
    }
    Ok(())
}
impl ResultRead {
    pub fn validate(&self) -> Result<(), GameDecodeError> {
        if !matches!(
            self.result,
            GameView::FinalHost { .. } | GameView::FinalPlayer { .. }
        ) {
            return Err(GameDecodeError);
        }
        self.result.validate()
    }
}
macro_rules! codec {($($t:ty),*)=>{$(impl $t {pub fn decode_json(bytes:&[u8])->Result<Self,GameDecodeError>{decode(bytes,GAME_FRAME_MAX_BYTES)}pub fn encode_json(&self)->Result<Vec<u8>,GameDecodeError>{let bytes=encode(self,GAME_FRAME_MAX_BYTES)?;Self::decode_json(&bytes)?;Ok(bytes)}})*};}
codec!(
    GameDetail,
    MembershipRead,
    PlayerRoster,
    CallsRead,
    RemainingValues,
    OwnBoard,
    BoardsRead,
    QualificationRead,
    ResultRead
);
/// Private owner envelope selects an endpoint DTO; public responses have no envelope tag.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "projection",
    content = "response",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ReadProjection {
    Game(GameDetail),
    Membership(MembershipRead),
    Players(PlayerRoster),
    Calls(CallsRead),
    RemainingValues(RemainingValues),
    OwnBoard(OwnBoard),
    Boards(BoardsRead),
    Qualification(QualificationRead),
    Result(ResultRead),
}
impl ReadProjection {
    pub fn kind(&self) -> ReadKind {
        match self {
            Self::Game(_) => ReadKind::Game,
            Self::Membership(_) => ReadKind::Membership,
            Self::Players(_) => ReadKind::Players,
            Self::Calls(_) => ReadKind::Calls,
            Self::RemainingValues(_) => ReadKind::RemainingValues,
            Self::OwnBoard(_) => ReadKind::OwnBoard,
            Self::Boards(_) => ReadKind::Boards,
            Self::Qualification(_) => ReadKind::Qualification,
            Self::Result(_) => ReadKind::Result,
        }
    }
    pub fn encode_json(&self) -> Result<Vec<u8>, GameDecodeError> {
        match self {
            Self::Game(v) => v.encode_json(),
            Self::Membership(v) => v.encode_json(),
            Self::Players(v) => v.encode_json(),
            Self::Calls(v) => v.encode_json(),
            Self::RemainingValues(v) => v.encode_json(),
            Self::OwnBoard(v) => v.encode_json(),
            Self::Boards(v) => v.encode_json(),
            Self::Qualification(v) => v.encode_json(),
            Self::Result(v) => v.encode_json(),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "Typed boundary fixtures fail fast.")]
mod tests {
    use super::*;
    #[test]
    fn dedicated_read_dto_closed_null_bounds_sorting_and_latest() {
        for raw in [
            r#"{"calls":[],"view_revision":0}"#,
            r#"{"calls":[],"latest":null,"view_revision":9007199254740992}"#,
            r#"{"calls":[],"latest":null,"view_revision":0,"x":1}"#,
            r#"{"calls":[],"calls":[],"latest":null,"view_revision":0}"#,
            r#"{"calls":[],"latest":{"sequence_no":1,"value":"1"},"view_revision":0}"#,
        ] {
            assert!(CallsRead::decode_json(raw.as_bytes()).is_err());
        }
        assert!(OwnBoard::decode_json(br#"{"view_revision":0}"#).is_err());
        assert!(OwnBoard::decode_json(br#"{"board":null,"view_revision":0}"#).is_ok());
        for raw in [
            r#"{"remaining_values":["10","2"],"remaining_count":2,"exhausted":false,"view_revision":0}"#,
            r#"{"remaining_values":["1"],"remaining_count":2,"exhausted":false,"view_revision":0}"#,
            r#"{"remaining_values":[],"remaining_count":0,"exhausted":false,"view_revision":0}"#,
            r#"{"remaining_values":["01"],"remaining_count":1,"exhausted":false,"view_revision":0}"#,
        ] {
            assert!(RemainingValues::decode_json(raw.as_bytes()).is_err());
        }
    }
    #[test]
    fn dedicated_read_full_pool_and_full_call_ledger_are_bounded_valid_typed_roundtrips() {
        let calls = (1..=NUMERIC_UPPER_BOUND_MAX)
            .map(|n| CallView {
                sequence_no: n,
                value: n.to_string(),
            })
            .collect::<Vec<_>>();
        let v = CallsRead {
            latest: calls.last().cloned(),
            calls,
            view_revision: 0,
        };
        assert_eq!(
            CallsRead::decode_json(&v.encode_json().unwrap())
                .unwrap()
                .calls
                .len(),
            READ_VALUES_MAX
        );
        let v = RemainingValues {
            remaining_values: (1..=NUMERIC_UPPER_BOUND_MAX)
                .map(|n| n.to_string())
                .collect(),
            remaining_count: NUMERIC_UPPER_BOUND_MAX,
            exhausted: false,
            view_revision: 0,
        };
        assert_eq!(
            RemainingValues::decode_json(&v.encode_json().unwrap())
                .unwrap()
                .remaining_values
                .len(),
            READ_VALUES_MAX
        );
    }
    fn qualification_views(lines: Vec<CompletedLine>) -> [QualificationRead; 2] {
        [
            QualificationRead::Player {
                qualified: !lines.is_empty(),
                qualifying_lines: lines.clone(),
                view_revision: 0,
            },
            QualificationRead::Account {
                qualified_players: vec![QualifiedPlayer {
                    player_id: "01890f3e-53b7-7d28-9b05-4f65092d5701".parse().unwrap(),
                    alias: "Reader".into(),
                    qualifying_lines: lines,
                }],
                view_revision: 0,
            },
        ]
    }
    #[test]
    fn dedicated_read_qualification_rejects_duplicate_unordered_and_out_of_bounds_lines() {
        use CompletedLine::{AntiDiagonal, Column, MainDiagonal, Row};
        for lines in [
            vec![Row(1), Row(1)],
            vec![Row(2), Row(1)],
            vec![Column(1), Row(1)],
            vec![Column(1), Column(1)],
            vec![Column(2), Column(1)],
            vec![MainDiagonal, Row(1)],
            vec![AntiDiagonal, MainDiagonal],
            vec![MainDiagonal, MainDiagonal],
            vec![AntiDiagonal, AntiDiagonal],
            vec![Row(0)],
            vec![Column(BOARD_SIDE_MAX + 1)],
            vec![Row(1); QUALIFYING_LINES_MAX + 1],
        ] {
            for view in qualification_views(lines) {
                let raw = serde_json::to_vec(&view).unwrap();
                assert!(QualificationRead::decode_json(&raw).is_err());
                assert!(view.validate().is_err());
                assert!(view.encode_json().is_err());
            }
        }
    }
    #[test]
    fn dedicated_read_qualification_accepts_canonical_subset_and_maximum_lines() {
        use CompletedLine::{AntiDiagonal, Column, MainDiagonal, Row};
        let maximum = (1..=BOARD_SIDE_MAX)
            .map(Row)
            .chain((1..=BOARD_SIDE_MAX).map(Column))
            .chain([MainDiagonal, AntiDiagonal])
            .collect::<Vec<_>>();
        assert_eq!(maximum.len(), QUALIFYING_LINES_MAX);
        for lines in [
            maximum,
            vec![Row(1), Column(BOARD_SIDE_MAX), MainDiagonal, AntiDiagonal],
        ] {
            for view in qualification_views(lines) {
                let encoded = view.encode_json().unwrap();
                assert_eq!(
                    QualificationRead::decode_json(&encoded)
                        .unwrap()
                        .encode_json()
                        .unwrap(),
                    encoded
                );
            }
        }
        let unqualified = QualificationRead::Player {
            qualified: false,
            qualifying_lines: vec![],
            view_revision: 0,
        };
        assert!(unqualified.encode_json().is_ok());
        let empty_account = QualificationRead::Account {
            qualified_players: vec![],
            view_revision: 0,
        };
        assert!(empty_account.encode_json().is_ok());
        assert!(qualification_views(vec![])[1].encode_json().is_err());
    }
    #[test]
    fn dedicated_read_terminal_roster_unknown_leave_requires_explicit_nullable_boolean() {
        let id = "01890f3e-53b7-7d28-9b05-4f65092d5701";
        let raw = format!(
            r#"{{"players":[{{"player_id":"{id}","alias":"Reader","connected":true,"membership_retained":true,"left_during_play":null}}],"view_revision":0}}"#
        );
        assert!(PlayerRoster::decode_json(raw.as_bytes()).is_ok());
        assert!(
            PlayerRoster::decode_json(raw.replace(",\"left_during_play\":null", "").as_bytes())
                .is_err()
        );
        assert!(PlayerRoster::decode_json(raw.replace("null", "123").as_bytes()).is_err());
    }
}
