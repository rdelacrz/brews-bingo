//! Pure game rules. Allocation-conscious: owned configurations and bounded boards.
use serde::{Deserialize, Serialize};

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum GameState {
    New,
    AwaitingPlayers,
    InProgress,
    Resolved,
    Cancelled,
}
#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum WinningPattern {
    SingleLine,
}
#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum CallMode {
    Random,
    Manual,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("Invalid game input.")]
pub struct GameValidationError;

/// Borrowed alias validation; no allocation or case folding.
pub const ALIAS_MAX_BYTES: usize = 20;
pub fn validate_alias(alias: &str) -> Result<&str, GameValidationError> {
    let alias = alias.trim_matches(' ');
    if alias.is_empty()
        || alias.len() > ALIAS_MAX_BYTES
        || !alias.bytes().all(|b| (32..=126).contains(&b))
    {
        return Err(GameValidationError);
    }
    Ok(alias)
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct GameCode(String);
impl GameCode {
    pub fn from_lookup(text: &str) -> Result<Self, GameValidationError> {
        let text = text.trim_matches(' ');
        if text.len() != 8 {
            return Err(GameValidationError);
        }
        text.to_ascii_uppercase().parse()
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl std::str::FromStr for GameCode {
    type Err = GameValidationError;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if text.len() != 8
            || !text
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        {
            return Err(GameValidationError);
        }
        Ok(Self(text.to_owned()))
    }
}
impl Serialize for GameCode {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}
impl<'de> Deserialize<'de> for GameCode {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CellPosition {
    pub row: u8,
    pub column: u8,
}

pub const BOARD_SIDE_MIN: u8 = 2;
pub const BOARD_SIDE_MAX: u8 = 10;
pub const BOARD_CELLS_MAX: usize = (BOARD_SIDE_MAX as usize) * (BOARD_SIDE_MAX as usize);
pub const PLAYER_CAPACITY_MAX: u8 = 20;
pub const SPECTATOR_CAPACITY_MAX: u8 = 50;
pub const NUMERIC_UPPER_BOUND_MAX: u32 = 1_000;
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameConfiguration {
    pub numeric_upper_bound: u32,
    pub board_side_length: u8,
    pub free_cells_enabled: bool,
    pub free_cell_positions: Vec<CellPosition>,
    pub player_capacity: u8,
    pub spectator_capacity: u8,
    pub winning_pattern: WinningPattern,
}
impl Default for GameConfiguration {
    fn default() -> Self {
        Self {
            numeric_upper_bound: 75,
            board_side_length: 5,
            free_cells_enabled: true,
            free_cell_positions: vec![CellPosition { row: 3, column: 3 }],
            player_capacity: 20,
            spectator_capacity: 50,
            winning_pattern: WinningPattern::SingleLine,
        }
    }
}
impl GameConfiguration {
    pub fn validate(&self) -> Result<(), GameValidationError> {
        if !(1..=NUMERIC_UPPER_BOUND_MAX).contains(&self.numeric_upper_bound)
            || !(BOARD_SIDE_MIN..=BOARD_SIDE_MAX).contains(&self.board_side_length)
            || !(2..=PLAYER_CAPACITY_MAX).contains(&self.player_capacity)
            || self.spectator_capacity > SPECTATOR_CAPACITY_MAX
            || self.free_cell_positions.len() > usize::from(self.board_side_length).pow(2)
            || self.free_cells_enabled == self.free_cell_positions.is_empty()
        {
            return Err(GameValidationError);
        }
        for (i, p) in self.free_cell_positions.iter().enumerate() {
            if p.row == 0
                || p.column == 0
                || p.row > self.board_side_length
                || p.column > self.board_side_length
                || self.free_cell_positions[..i].contains(p)
            {
                return Err(GameValidationError);
            }
        }
        let k = u32::from(self.board_side_length).pow(2) - self.free_cell_positions.len() as u32;
        if self.numeric_upper_bound < k {
            return Err(GameValidationError);
        }
        Ok(())
    }
    /// Counts layouts only up to the complete retained roster size (not connections).
    pub fn validate_start_feasibility(
        &self,
        retained_players: usize,
    ) -> Result<(), GameValidationError> {
        let k = self.non_free_cell_count()?;
        if retained_players > usize::from(self.player_capacity) {
            return Err(GameValidationError);
        }
        let cap = retained_players as u32;
        let mut layouts = 1;
        for i in 0..k {
            layouts = (layouts * (self.numeric_upper_bound - i)).min(cap);
            if layouts >= cap {
                break;
            }
        }
        if layouts < cap {
            return Err(GameValidationError);
        }
        Ok(())
    }
    pub fn non_free_cell_count(&self) -> Result<u32, GameValidationError> {
        self.validate()?;
        Ok(u32::from(self.board_side_length).pow(2) - self.free_cell_positions.len() as u32)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoardCell {
    pub position: CellPosition,
    pub kind: BoardCellKind,
    pub is_matched: bool,
}
pub const BOARD_CELL_KIND_FREE: &str = "free";
pub const BOARD_CELL_KIND_VALUE: &str = "value";
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum BoardCellKind {
    Free,
    Value(String),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "index",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum CompletedLine {
    Row(u8),
    Column(u8),
    MainDiagonal,
    AntiDiagonal,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("Invalid board shape.")]
pub struct PatternEvaluationError;
pub trait SingleLinePattern {
    fn evaluate(
        side_length: u8,
        cells: &[BoardCell],
    ) -> Result<Vec<CompletedLine>, PatternEvaluationError>;
}
pub struct SingleLine;
impl SingleLinePattern for SingleLine {
    fn evaluate(
        side_length: u8,
        cells: &[BoardCell],
    ) -> Result<Vec<CompletedLine>, PatternEvaluationError> {
        if !(BOARD_SIDE_MIN..=BOARD_SIDE_MAX).contains(&side_length)
            || cells.len() != usize::from(side_length).pow(2)
        {
            return Err(PatternEvaluationError);
        }
        let side = usize::from(side_length);
        let mut matched = [false; BOARD_CELLS_MAX];
        let mut seen = [false; BOARD_CELLS_MAX];
        for cell in cells {
            let p = cell.position;
            if p.row == 0
                || p.column == 0
                || p.row > side_length
                || p.column > side_length
                || (cell.kind == BoardCellKind::Free && !cell.is_matched)
            {
                return Err(PatternEvaluationError);
            }
            let index = usize::from(p.row - 1) * side + usize::from(p.column - 1);
            if seen[index] {
                return Err(PatternEvaluationError);
            }
            seen[index] = true;
            matched[index] = cell.is_matched;
        }
        let mut lines = Vec::with_capacity(2 * side + 2);
        for r in 0..side {
            if (0..side).all(|c| matched[r * side + c]) {
                lines.push(CompletedLine::Row((r + 1) as u8));
            }
        }
        for c in 0..side {
            if (0..side).all(|r| matched[r * side + c]) {
                lines.push(CompletedLine::Column((c + 1) as u8));
            }
        }
        if (0..side).all(|i| matched[i * side + i]) {
            lines.push(CompletedLine::MainDiagonal);
        }
        if (0..side).all(|i| matched[i * side + side - 1 - i]) {
            lines.push(CompletedLine::AntiDiagonal);
        }
        Ok(lines)
    }
}
/// Apply one accepted value to every matching ordinary cell without toggling prior matches.
pub fn apply_called_value(
    side_length: u8,
    cells: &mut [BoardCell],
    value: &str,
) -> Result<(Vec<CompletedLine>, bool), PatternEvaluationError> {
    SingleLine::evaluate(side_length, cells)?;
    let mut changed = false;
    for cell in cells.iter_mut() {
        if let BoardCellKind::Value(cell_value) = &cell.kind
            && cell_value == value
            && !cell.is_matched
        {
            cell.is_matched = true;
            changed = true;
        }
    }
    let lines = SingleLine::evaluate(side_length, cells)?;
    Ok((lines, changed))
}

pub const START_CANDIDATE_BUDGET: usize = 1_024;
pub const RANDOM_INDEX_REJECTION_BUDGET: usize = 128;
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("Random source unavailable.")]
pub struct RandomSourceError;
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum BoardGenerationError {
    #[error("Invalid start configuration or roster.")]
    InvalidConfiguration,
    #[error("Duplicate player identity.")]
    DuplicatePlayer,
    #[error("Random source unavailable.")]
    RandomUnavailable,
    #[error("Random index rejection budget exhausted.")]
    RejectionBudgetExhausted,
    #[error("Start candidate budget exhausted.")]
    CandidateBudgetExhausted,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssignedBoard {
    pub player_id: crate::ids::PlayerId,
    pub side_length: u8,
    pub cells: Vec<BoardCell>,
    pub qualifying_lines: Vec<CompletedLine>,
}
/// All-or-none board construction; at most 20 boards and 1,024 total candidates.
/// Random bytes are supplied by the adapter; no clocks, seeds or SDK dependencies.
pub fn generate_boards(
    configuration: &GameConfiguration,
    players: &[crate::ids::PlayerId],
    random: &mut impl FnMut(&mut [u8]) -> Result<(), RandomSourceError>,
) -> Result<Vec<AssignedBoard>, BoardGenerationError> {
    configuration
        .validate_start_feasibility(players.len())
        .map_err(|_| BoardGenerationError::InvalidConfiguration)?;
    let mut players = players.to_vec();
    players.sort_unstable();
    if players.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(BoardGenerationError::DuplicatePlayer);
    }
    let side = configuration.board_side_length;
    let mut boards: Vec<AssignedBoard> = Vec::with_capacity(players.len());
    let mut candidates = 0;
    for player_id in players {
        loop {
            if candidates >= START_CANDIDATE_BUDGET {
                return Err(BoardGenerationError::CandidateBudgetExhausted);
            }
            candidates += 1;
            // Sparse swaps keep the numeric pool implicit, using at most k entries.
            let mut swaps = [(0u32, 0u32); BOARD_CELLS_MAX];
            let mut swap_len = 0;
            let mut remaining = configuration.numeric_upper_bound;
            let mut cells = Vec::with_capacity(usize::from(side).pow(2));
            for row in 1..=side {
                for column in 1..=side {
                    let position = CellPosition { row, column };
                    let free = configuration.free_cell_positions.contains(&position);
                    let kind = if free {
                        BoardCellKind::Free
                    } else {
                        let index = bounded_index(remaining, random)?;
                        let mapped = |key| {
                            swaps[..swap_len]
                                .iter()
                                .find(|entry| entry.0 == key)
                                .map_or(key + 1, |entry| entry.1)
                        };
                        let value = mapped(index);
                        let last = mapped(remaining - 1);
                        if let Some(entry) =
                            swaps[..swap_len].iter_mut().find(|entry| entry.0 == index)
                        {
                            entry.1 = last;
                        } else {
                            swaps[swap_len] = (index, last);
                            swap_len += 1;
                        }
                        remaining -= 1;
                        BoardCellKind::Value(value.to_string())
                    };
                    cells.push(BoardCell {
                        position,
                        kind,
                        is_matched: free,
                    });
                }
            }
            if boards.iter().any(|board| {
                board
                    .cells
                    .iter()
                    .zip(&cells)
                    .all(|(a, b)| a.position == b.position && a.kind == b.kind)
            }) {
                continue;
            }
            let qualifying_lines = SingleLine::evaluate(side, &cells)
                .map_err(|_| BoardGenerationError::InvalidConfiguration)?;
            boards.push(AssignedBoard {
                player_id,
                side_length: side,
                cells,
                qualifying_lines,
            });
            break;
        }
    }
    Ok(boards)
}
fn bounded_index(
    bound: u32,
    random: &mut impl FnMut(&mut [u8]) -> Result<(), RandomSourceError>,
) -> Result<u32, BoardGenerationError> {
    let threshold = bound.wrapping_neg() % bound;
    for _ in 0..RANDOM_INDEX_REJECTION_BUDGET {
        let mut bytes = [0; 4];
        random(&mut bytes).map_err(|_| BoardGenerationError::RandomUnavailable)?;
        let value = u32::from_le_bytes(bytes);
        if value >= threshold {
            return Ok(value % bound);
        }
    }
    Err(BoardGenerationError::RejectionBudgetExhausted)
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "Tests fail fast on invalid bounded fixtures."
)]
mod tests {
    use super::*;
    #[test]
    fn generation_budget_covers_whole_start_and_random_errors_return_no_partial_boards() {
        let ids = [
            "01890f3e-53b7-7d28-9b05-4f65092d5711".parse().unwrap(),
            "01890f3e-53b7-7d28-9b05-4f65092d5712".parse().unwrap(),
        ];
        let c = GameConfiguration {
            board_side_length: 2,
            numeric_upper_bound: 2,
            free_cell_positions: vec![
                CellPosition { row: 1, column: 1 },
                CellPosition { row: 1, column: 2 },
                CellPosition { row: 2, column: 1 },
            ],
            ..GameConfiguration::default()
        };
        let mut calls = 0;
        assert_eq!(
            generate_boards(&c, &ids, &mut |bytes| {
                calls += 1;
                bytes.fill(0);
                Ok(())
            }),
            Err(BoardGenerationError::CandidateBudgetExhausted)
        );
        assert_eq!(calls, START_CANDIDATE_BUDGET);
        assert_eq!(
            generate_boards(&c, &ids, &mut |_| Err(RandomSourceError)),
            Err(BoardGenerationError::RandomUnavailable)
        );
        assert_eq!(
            generate_boards(&c, &[ids[0], ids[0]], &mut |_| unreachable!()),
            Err(BoardGenerationError::DuplicatePlayer)
        );
        let mut calls = 0;
        assert_eq!(
            generate_boards(&c, &ids, &mut |bytes| {
                calls += 1;
                if calls == 2 {
                    return Err(RandomSourceError);
                }
                bytes.fill(0);
                Ok(())
            }),
            Err(BoardGenerationError::RandomUnavailable)
        );
    }
    #[test]
    fn bounded_index_rejects_biased_tail_and_fails_closed_on_rejection_exhaustion() {
        let mut calls = 0;
        assert_eq!(
            bounded_index(3, &mut |bytes| {
                calls += 1;
                bytes.copy_from_slice(&(if calls == 1 { 0u32 } else { 4 }).to_le_bytes());
                Ok(())
            }),
            Ok(1)
        );
        assert_eq!(calls, 2);
        let mut calls = 0;
        assert_eq!(
            bounded_index(3, &mut |bytes| {
                calls += 1;
                bytes.fill(0);
                Ok(())
            }),
            Err(BoardGenerationError::RejectionBudgetExhausted)
        );
        assert_eq!(calls, RANDOM_INDEX_REJECTION_BUDGET);
    }
    #[test]
    fn generation_is_deterministic_stable_id_order_unique_and_initially_qualified() {
        let ids = [
            "01890f3e-53b7-7d28-9b05-4f65092d5712".parse().unwrap(),
            "01890f3e-53b7-7d28-9b05-4f65092d5711".parse().unwrap(),
        ];
        let c = GameConfiguration {
            board_side_length: 2,
            numeric_upper_bound: 2,
            free_cell_positions: vec![
                CellPosition { row: 1, column: 1 },
                CellPosition { row: 1, column: 2 },
                CellPosition { row: 2, column: 1 },
            ],
            ..GameConfiguration::default()
        };
        let run = |roster: &[_]| {
            let mut value = 0u32;
            generate_boards(&c, roster, &mut |bytes| {
                bytes.copy_from_slice(&value.to_le_bytes());
                value += 1;
                Ok(())
            })
        };
        let result = run(&ids);
        assert!(result.is_ok());
        let boards = result.unwrap();
        assert_eq!(boards, run(&[ids[1], ids[0]]).unwrap());
        assert_eq!(boards[0].player_id, ids[1]);
        assert_ne!(boards[0].cells, boards[1].cells);
        assert_eq!(
            boards[0].qualifying_lines,
            vec![
                CompletedLine::Row(1),
                CompletedLine::Column(1),
                CompletedLine::AntiDiagonal
            ]
        );
        assert_eq!(boards[0].cells[3].kind, BoardCellKind::Value("1".into()));
        assert!(!boards[0].cells[3].is_matched);
    }
    #[test]
    fn single_line_returns_all_lines_in_approved_order_and_rejects_invalid_shape() {
        let cells = (1..=2)
            .flat_map(|row| {
                (1..=2).map(move |column| BoardCell {
                    position: CellPosition { row, column },
                    kind: BoardCellKind::Free,
                    is_matched: true,
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(
            SingleLine::evaluate(2, &cells),
            Ok(vec![
                CompletedLine::Row(1),
                CompletedLine::Row(2),
                CompletedLine::Column(1),
                CompletedLine::Column(2),
                CompletedLine::MainDiagonal,
                CompletedLine::AntiDiagonal
            ])
        );
        assert!(SingleLine::evaluate(1, &cells).is_err());
        assert!(SingleLine::evaluate(2, &cells[..3]).is_err());
        let mut bad = cells.clone();
        bad[0].position = bad[1].position;
        assert!(SingleLine::evaluate(2, &bad).is_err());
        let mut ordinary = cells;
        ordinary[0].kind = BoardCellKind::Value("1".into());
        ordinary[0].is_matched = false;
        assert_eq!(
            SingleLine::evaluate(2, &ordinary),
            Ok(vec![
                CompletedLine::Row(2),
                CompletedLine::Column(2),
                CompletedLine::AntiDiagonal
            ])
        );
    }
    #[test]
    fn accepted_value_matches_all_cells_once_and_recomputes_single_line() {
        let mut cells = vec![
            BoardCell {
                position: CellPosition { row: 1, column: 1 },
                kind: BoardCellKind::Value("7".into()),
                is_matched: false,
            },
            BoardCell {
                position: CellPosition { row: 1, column: 2 },
                kind: BoardCellKind::Value("8".into()),
                is_matched: true,
            },
            BoardCell {
                position: CellPosition { row: 2, column: 1 },
                kind: BoardCellKind::Free,
                is_matched: true,
            },
            BoardCell {
                position: CellPosition { row: 2, column: 2 },
                kind: BoardCellKind::Value("7".into()),
                is_matched: false,
            },
        ];

        let (lines, changed) = apply_called_value(2, &mut cells, "7").unwrap();
        assert!(changed);
        assert_eq!(
            cells.iter().map(|cell| cell.is_matched).collect::<Vec<_>>(),
            [true, true, true, true]
        );
        assert_eq!(
            lines,
            vec![
                CompletedLine::Row(1),
                CompletedLine::Row(2),
                CompletedLine::Column(1),
                CompletedLine::Column(2),
                CompletedLine::MainDiagonal,
                CompletedLine::AntiDiagonal,
            ]
        );

        let (retry_lines, changed) = apply_called_value(2, &mut cells, "7").unwrap();
        assert!(!changed);
        assert_eq!(retry_lines, lines);
        assert_eq!(
            cells.iter().map(|cell| cell.is_matched).collect::<Vec<_>>(),
            [true; 4]
        );
    }
    #[test]
    fn invalid_board_is_unchanged_when_progression_fails() {
        let mut cells = vec![BoardCell {
            position: CellPosition { row: 1, column: 1 },
            kind: BoardCellKind::Value("7".into()),
            is_matched: false,
        }];
        let before = cells.clone();
        assert!(apply_called_value(2, &mut cells, "7").is_err());
        assert_eq!(cells, before);
    }
    #[test]
    fn call_mode_storage_tags_are_stable_snake_case() {
        assert_eq!(CallMode::Random.to_string(), "random");
        assert_eq!(CallMode::Manual.to_string(), "manual");
        assert_eq!("random".parse::<CallMode>(), Ok(CallMode::Random));
        assert_eq!("manual".parse::<CallMode>(), Ok(CallMode::Manual));
        assert!("other".parse::<CallMode>().is_err());
    }
    #[test]
    fn feasibility_counts_entire_roster_with_capped_falling_factorial() {
        assert_eq!(
            GameConfiguration::default().validate_start_feasibility(20),
            Ok(())
        );
        let mut c = GameConfiguration {
            board_side_length: 2,
            numeric_upper_bound: 1,
            free_cell_positions: vec![
                CellPosition { row: 1, column: 1 },
                CellPosition { row: 1, column: 2 },
                CellPosition { row: 2, column: 1 },
                CellPosition { row: 2, column: 2 },
            ],
            ..GameConfiguration::default()
        };
        assert_eq!(c.validate_start_feasibility(1), Ok(()));
        assert!(c.validate_start_feasibility(2).is_err());
        c.free_cell_positions.pop();
        c.numeric_upper_bound = 2;
        assert_eq!(c.validate_start_feasibility(2), Ok(()));
        assert!(c.validate_start_feasibility(3).is_err());
        assert!(
            GameConfiguration::default()
                .validate_start_feasibility(21)
                .is_err()
        );
    }
    #[test]
    fn complete_configuration_enforces_approved_ranges_free_positions_and_pool() {
        let default = GameConfiguration::default();
        assert_eq!(default.validate(), Ok(()));
        assert_eq!(default.non_free_cell_count(), Ok(24));
        for n in [0, 1001, 23] {
            let c = GameConfiguration {
                numeric_upper_bound: n,
                ..default.clone()
            };
            assert!(c.validate().is_err());
        }
        for side in [0, 1, 11, 255] {
            assert!(
                GameConfiguration {
                    board_side_length: side,
                    ..default.clone()
                }
                .validate()
                .is_err()
            );
        }
        for cap in [0, 1, 21, 255] {
            assert!(
                GameConfiguration {
                    player_capacity: cap,
                    ..default.clone()
                }
                .validate()
                .is_err()
            );
        }
        assert!(
            GameConfiguration {
                spectator_capacity: 51,
                ..default.clone()
            }
            .validate()
            .is_err()
        );
        for positions in [
            vec![],
            vec![CellPosition { row: 0, column: 1 }],
            vec![CellPosition { row: 1, column: 6 }],
            vec![CellPosition { row: 3, column: 3 }; 2],
        ] {
            assert!(
                GameConfiguration {
                    free_cell_positions: positions,
                    ..default.clone()
                }
                .validate()
                .is_err()
            );
        }
        assert!(
            GameConfiguration {
                free_cells_enabled: false,
                ..default.clone()
            }
            .validate()
            .is_err()
        );
        assert!(
            GameConfiguration {
                free_cells_enabled: false,
                free_cell_positions: vec![],
                numeric_upper_bound: 25,
                spectator_capacity: 0,
                ..default
            }
            .validate()
            .is_ok()
        );
    }
    #[test]
    fn code_lookup_alone_canonicalizes_ordinary_spaces_and_ascii_case() {
        assert_eq!(
            GameCode::from_lookup(" ab12Cd34 ").unwrap().as_str(),
            "AB12CD34"
        );
        assert_eq!("AB12CD34".parse::<GameCode>().unwrap().as_str(), "AB12CD34");
        for text in [
            "ab12Cd34",
            " AB12CD34 ",
            "AB12 CD34",
            "AB12CD3",
            "AB12CD345",
            "AB12CD3é",
            "AB12CD3_",
            "\tAB12CD34",
        ] {
            assert!(text.parse::<GameCode>().is_err());
        }
        for text in ["AB12 CD3", "\tAB12CD34", "AB12CD34\n", "AB12CD3é"] {
            assert!(GameCode::from_lookup(text).is_err());
        }
    }
    #[test]
    fn alias_trims_only_outer_spaces_and_preserves_case_and_internal_spaces() {
        assert_eq!(validate_alias("  Alice  !  "), Ok("Alice  !"));
        assert_ne!(validate_alias("Alice"), validate_alias("alice"));
        assert!(validate_alias("                    ").is_err());
        assert!(validate_alias(&"x".repeat(21)).is_err());
        assert!(validate_alias(&"x".repeat(20)).is_ok());
        for value in ["\tAlice", "Alice\n", "A\0B", "\x7f", "é", "\u{a0}Alice"] {
            assert!(validate_alias(value).is_err());
        }
        assert_eq!(GameState::AwaitingPlayers.to_string(), "awaiting_players");
        assert_eq!(WinningPattern::SingleLine.to_string(), "single_line");
    }
}
