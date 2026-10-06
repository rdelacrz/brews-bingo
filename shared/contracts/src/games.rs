//! Allocation-conscious game wire boundary; request secrets are never response fields.
use brews_domain::games::{
    BOARD_CELLS_MAX, CellPosition, GameConfiguration, WinningPattern, validate_alias,
};
use serde::{Deserialize, Serialize};

pub const GAME_BODY_MAX_BYTES: usize = 4_096;
pub const GAME_QUERY_KNOWN_REVISION: &str = "known_revision";
pub const GAME_QUERY_VIEW: &str = "view";
pub const GAME_VIEW_ACCOUNT: &str = "account";
pub const GAME_VIEW_PLAYER: &str = "player";
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("Invalid game JSON.")]
pub struct GameDecodeError;
fn object<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Visitor<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Visitor<T> {
        type Value = T;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a JSON object")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(self, map: A) -> Result<T, A::Error> {
            T::deserialize(serde::de::value::MapAccessDeserializer::new(map))
        }
    }
    d.deserialize_map(Visitor(std::marker::PhantomData))
}
macro_rules! map_struct {
    ($vis:vis $name:ident { $($(#[$meta:meta])* $field:ident : $ty:ty),* $(,)? } $(; $validate:ident)?) => {
        #[derive(Clone,Debug,Serialize)]
        #[serde(deny_unknown_fields)]
        $vis struct $name { $($(#[$meta])* pub $field:$ty),* }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D:serde::Deserializer<'de>>(d:D)->Result<Self,D::Error> {
                #[derive(Deserialize)] #[serde(deny_unknown_fields)]
                struct Wire { $($(#[$meta])* $field:$ty),* }
                let wire:Wire=object(d)?;
                let value = Self { $($field:wire.$field),* };
                $(value.$validate().map_err(serde::de::Error::custom)?;)?
                Ok(value)
            }
        }
    };
}
macro_rules! map_enum {
    ($vis:vis $name:ident, $tag:literal, { $($variant:ident { $($(#[$meta:meta])* $field:ident : $ty:ty),* $(,)? }),* $(,)? } $(; $validate:ident)?) => {
        #[derive(Clone, Debug, Serialize)]
        #[serde(tag = $tag, rename_all = "snake_case", deny_unknown_fields)]
        $vis enum $name { $($variant { $($(#[$meta])* $field: $ty),* }),* }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                #[derive(Deserialize)]
                #[serde(tag = $tag, rename_all = "snake_case", deny_unknown_fields)]
                enum Wire { $($variant { $($(#[$meta])* $field: $ty),* }),* }
                let wire: Wire = object(d)?;
                let value = match wire { $(Wire::$variant { $($field),* } => Self::$variant { $($field),* }),* };
                $(value.$validate().map_err(serde::de::Error::custom)?;)?
                Ok(value)
            }
        }
    };
}
fn nullable_value<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d)
}
fn non_null_option<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(d).map(Some)
}
map_struct!(Position {
    row: u8,
    column: u8
});
fn positions<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<Vec<CellPosition>>, D::Error> {
    let positions = Vec::<Position>::deserialize(d)?;
    if positions.len() > BOARD_CELLS_MAX {
        return Err(serde::de::Error::custom("Too many free cells."));
    }
    Ok(Some(
        positions
            .into_iter()
            .map(|p| CellPosition {
                row: p.row,
                column: p.column,
            })
            .collect(),
    ))
}
map_struct!(ConfigurationOverrides {
    #[serde(default,deserialize_with="non_null_option")] numeric_upper_bound:Option<u32>,
    #[serde(default,deserialize_with="non_null_option")] board_side_length:Option<u8>,
    #[serde(default,deserialize_with="non_null_option")] free_cells_enabled:Option<bool>,
    #[serde(default,deserialize_with="positions")] free_cell_positions:Option<Vec<CellPosition>>,
    #[serde(default,deserialize_with="non_null_option")] player_capacity:Option<u8>,
    #[serde(default,deserialize_with="non_null_option")] spectator_capacity:Option<u8>,
    #[serde(default,deserialize_with="non_null_option")] winning_pattern:Option<WinningPattern>,
});
impl ConfigurationOverrides {
    fn resolve(self) -> Result<GameConfiguration, GameDecodeError> {
        let defaults = GameConfiguration::default();
        let side = self.board_side_length.unwrap_or(defaults.board_side_length);
        let enabled = self
            .free_cells_enabled
            .unwrap_or(defaults.free_cells_enabled);
        let positions = self.free_cell_positions.unwrap_or_else(|| {
            if enabled {
                vec![CellPosition {
                    row: side.div_ceil(2),
                    column: side.div_ceil(2),
                }]
            } else {
                vec![]
            }
        });
        let configuration = GameConfiguration {
            numeric_upper_bound: self
                .numeric_upper_bound
                .unwrap_or(defaults.numeric_upper_bound),
            board_side_length: side,
            free_cells_enabled: enabled,
            free_cell_positions: positions,
            player_capacity: self.player_capacity.unwrap_or(defaults.player_capacity),
            spectator_capacity: self
                .spectator_capacity
                .unwrap_or(defaults.spectator_capacity),
            winning_pattern: self.winning_pattern.unwrap_or(defaults.winning_pattern),
        };
        configuration.validate().map_err(|_| GameDecodeError)?;
        Ok(configuration)
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct CreateGame {
    pub configuration: GameConfiguration,
}
impl<'de> Deserialize<'de> for CreateGame {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            #[serde(default, deserialize_with = "non_null_option")]
            configuration: Option<ConfigurationOverrides>,
        }
        let wire: Wire = object(d)?;
        let configuration = match wire.configuration {
            Some(overrides) => overrides.resolve().map_err(serde::de::Error::custom)?,
            None => GameConfiguration::default(),
        };
        Ok(Self { configuration })
    }
}
struct BoundedJsonSink {
    bytes: Vec<u8>,
    limit: usize,
}
impl std::io::Write for BoundedJsonSink {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|end| end > self.limit)
        {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn encode<T: Serialize>(value: &T, limit: usize) -> Result<Vec<u8>, GameDecodeError> {
    let mut sink = BoundedJsonSink {
        bytes: Vec::with_capacity(limit),
        limit,
    };
    serde_json::to_writer(&mut sink, value).map_err(|_| GameDecodeError)?;
    Ok(sink.bytes)
}
fn decode<T: serde::de::DeserializeOwned>(
    bytes: &[u8],
    bound: usize,
) -> Result<T, GameDecodeError> {
    if bytes.len() > bound
        || bytes
            .iter()
            .find(|b| !matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
            != Some(&b'{')
    {
        return Err(GameDecodeError);
    }
    serde_json::from_slice(bytes).map_err(|_| GameDecodeError)
}
impl CreateGame {
    pub fn decode_json(bytes: &[u8]) -> Result<Self, GameDecodeError> {
        decode(bytes, GAME_BODY_MAX_BYTES)
    }
}
pub const MAX_SAFE_REVISION: u64 = 9_007_199_254_740_991;
fn safe_revision<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    let revision = u64::deserialize(d)?;
    if revision > MAX_SAFE_REVISION {
        return Err(serde::de::Error::custom("Revision out of range."));
    }
    Ok(revision)
}
map_struct!(pub RevisionCommand {
    #[serde(deserialize_with = "safe_revision")]
    expected_revision: u64
});
impl RevisionCommand {
    pub fn decode_json(bytes: &[u8]) -> Result<Self, GameDecodeError> {
        decode(bytes, GAME_BODY_MAX_BYTES)
    }
}
fn lookup_code<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<brews_domain::games::GameCode, D::Error> {
    brews_domain::games::GameCode::from_lookup(&String::deserialize(d)?)
        .map_err(serde::de::Error::custom)
}
map_struct!(pub AdmissionContextInput {
    #[serde(deserialize_with = "lookup_code")]
    game_code: brews_domain::games::GameCode
});
impl AdmissionContextInput {
    pub fn decode_json(bytes: &[u8]) -> Result<Self, GameDecodeError> {
        decode(bytes, GAME_BODY_MAX_BYTES)
    }
}
/// Optional authentication material; zeroized on drop and never serialized.
pub struct RecoveryAnswer(zeroize::Zeroizing<String>);
impl zeroize::Zeroize for RecoveryAnswer {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}
impl RecoveryAnswer {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Debug for RecoveryAnswer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[redacted]")
    }
}
impl<'de> Deserialize<'de> for RecoveryAnswer {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d).map(|value| Self(zeroize::Zeroizing::new(value)))
    }
}
#[derive(Debug)]
pub struct JoinPlayer {
    pub game_code: brews_domain::games::GameCode,
    pub alias: String,
    pub recovery_answer: Option<RecoveryAnswer>,
}
impl<'de> Deserialize<'de> for JoinPlayer {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            #[serde(deserialize_with = "lookup_code")]
            game_code: brews_domain::games::GameCode,
            alias: String,
            #[serde(default, deserialize_with = "non_null_option")]
            recovery_answer: Option<RecoveryAnswer>,
        }
        let wire: Wire = object(d)?;
        let alias = validate_alias(&wire.alias)
            .map_err(serde::de::Error::custom)?
            .to_owned();
        Ok(Self {
            game_code: wire.game_code,
            alias,
            recovery_answer: wire.recovery_answer,
        })
    }
}
impl JoinPlayer {
    pub fn decode_json(bytes: &[u8]) -> Result<Self, GameDecodeError> {
        decode(bytes, GAME_BODY_MAX_BYTES)
    }
}
use brews_domain::{
    games::GameState,
    ids::{AccountId, CommandId, GameId, OperationId, PlayerId},
};
fn canonical_alias<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let alias = String::deserialize(d)?;
    if validate_alias(&alias).map_err(serde::de::Error::custom)? != alias {
        return Err(serde::de::Error::custom("Noncanonical alias."));
    }
    Ok(alias)
}
pub const GAME_RECEIPT_MAX_BYTES: usize = 4_096;
pub const GAME_RECEIPT_RETENTION_MS: i64 = 86_400_000;
const PRE_START_IDLE_MS: i64 = 86_400_000;
const PLAYER_SESSION_LIFETIME_MS: i64 = 86_400_000;
pub const MAX_SAFE_TIMESTAMP: i64 = 9_007_199_254_740_991;
fn validate_timestamp(value: i64) -> Result<(), GameDecodeError> {
    if !(0..=MAX_SAFE_TIMESTAMP).contains(&value) {
        return Err(GameDecodeError);
    }
    Ok(())
}
fn safe_timestamp<'de, D: serde::Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    let value = i64::deserialize(d)?;
    validate_timestamp(value).map_err(serde::de::Error::custom)?;
    Ok(value)
}
fn checked_deadline(completed_at: i64, duration_ms: i64) -> Result<i64, GameDecodeError> {
    validate_timestamp(completed_at)?;
    let deadline = completed_at
        .checked_add(duration_ms)
        .ok_or(GameDecodeError)?;
    validate_timestamp(deadline)?;
    Ok(deadline)
}
pub const GAME_FRAME_MAX_BYTES: usize = 256 * 1_024;
map_enum!(pub GameOutcome, "operation", {
    Created {
        #[serde(deserialize_with = "safe_revision")]
        view_revision: u64,
        #[serde(deserialize_with = "safe_timestamp")]
        idle_cancel_due_at: i64,
    },
    LobbyOpened {
        #[serde(deserialize_with = "safe_revision")]
        view_revision: u64,
    },
    Started {
        #[serde(deserialize_with = "safe_revision")]
        view_revision: u64,
        #[serde(deserialize_with = "safe_timestamp")]
        started_at: i64,
    },
    PlayerJoined {
        player_id: PlayerId,
        #[serde(deserialize_with = "safe_revision")]
        view_revision: u64,
        #[serde(deserialize_with = "safe_timestamp")]
        session_expires_at: i64,
    },
});
fn version_one<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u8, D::Error> {
    let version = u8::deserialize(d)?;
    if version != 1 {
        return Err(serde::de::Error::custom("Unsupported version."));
    }
    Ok(version)
}
map_struct!(pub GameReceipt {
    #[serde(deserialize_with = "version_one")]
    version: u8,
    command_id: CommandId,
    game_id: GameId,
    outcome: GameOutcome,
    completed_at: i64,
    expires_at: i64
}; validate);
impl GameReceipt {
    pub fn validate(&self) -> Result<(), GameDecodeError> {
        if self.version != 1
            || self.expires_at != checked_deadline(self.completed_at, GAME_RECEIPT_RETENTION_MS)?
        {
            return Err(GameDecodeError);
        }
        let revision = match &self.outcome {
            GameOutcome::Created {
                view_revision,
                idle_cancel_due_at,
            } => {
                validate_timestamp(*idle_cancel_due_at)?;
                // Initialization can complete after the stored creation time.
                if *view_revision != 0
                    || *idle_cancel_due_at <= self.completed_at
                    || *idle_cancel_due_at > self.expires_at
                {
                    return Err(GameDecodeError);
                }
                *view_revision
            }
            GameOutcome::LobbyOpened { view_revision } => *view_revision,
            GameOutcome::Started {
                view_revision,
                started_at,
            } => {
                validate_timestamp(*started_at)?;
                if *started_at > self.completed_at {
                    return Err(GameDecodeError);
                }
                *view_revision
            }
            GameOutcome::PlayerJoined {
                view_revision,
                session_expires_at,
                ..
            } => {
                if *session_expires_at
                    != checked_deadline(self.completed_at, PLAYER_SESSION_LIFETIME_MS)?
                {
                    return Err(GameDecodeError);
                }
                *view_revision
            }
        };
        if revision > MAX_SAFE_REVISION {
            return Err(GameDecodeError);
        }
        Ok(())
    }
    pub fn decode_json(bytes: &[u8]) -> Result<Self, GameDecodeError> {
        decode(bytes, GAME_RECEIPT_MAX_BYTES)
    }
    pub fn encode_json(&self) -> Result<Vec<u8>, GameDecodeError> {
        self.validate()?;
        encode(self, GAME_RECEIPT_MAX_BYTES)
    }
}
fn configuration<'de, D: serde::Deserializer<'de>>(d: D) -> Result<GameConfiguration, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Wire {
        numeric_upper_bound: u32,
        board_side_length: u8,
        free_cells_enabled: bool,
        free_cell_positions: Vec<Position>,
        player_capacity: u8,
        spectator_capacity: u8,
        winning_pattern: WinningPattern,
    }
    let wire: Wire = object(d)?;
    let config = GameConfiguration {
        numeric_upper_bound: wire.numeric_upper_bound,
        board_side_length: wire.board_side_length,
        free_cells_enabled: wire.free_cells_enabled,
        free_cell_positions: wire
            .free_cell_positions
            .into_iter()
            .map(|p| CellPosition {
                row: p.row,
                column: p.column,
            })
            .collect(),
        player_capacity: wire.player_capacity,
        spectator_capacity: wire.spectator_capacity,
        winning_pattern: wire.winning_pattern,
    };
    config.validate().map_err(serde::de::Error::custom)?;
    Ok(config)
}
map_struct!(pub GameSummary {
    game_id: GameId,
    state: GameState,
    designated_host_id: AccountId,
    #[serde(deserialize_with = "safe_timestamp")]
    created_at: i64,
    #[serde(deserialize_with = "safe_revision")]
    view_revision: u64
});
map_enum!(pub GameResponse, "result", {
    Created {
        game: GameSummary,
        #[serde(deserialize_with = "configuration")]
        configuration: GameConfiguration,
        idle_cancel_due_at: i64,
        receipt: GameReceipt,
    },
    LobbyOpened {
        game_id: GameId,
        state: GameState,
        game_code: brews_domain::games::GameCode,
        #[serde(deserialize_with = "configuration")]
        configuration: GameConfiguration,
        #[serde(deserialize_with = "safe_revision")]
        view_revision: u64,
        receipt: GameReceipt,
    },
    Started {
        game_id: GameId,
        state: GameState,
        started_at: i64,
        #[serde(deserialize_with = "safe_revision")]
        view_revision: u64,
        receipt: GameReceipt,
    },
    Committed {
        receipt: GameReceipt,
    },
    Pending {
        operation_id: OperationId,
    },
    AdmissionContext {
        expires_at: i64,
    },
    PlayerJoined {
        player_id: PlayerId,
        #[serde(deserialize_with = "canonical_alias")]
        alias: String,
        session_expires_at: i64,
        #[serde(deserialize_with = "safe_revision")]
        view_revision: u64,
        receipt: GameReceipt,
    },
}; validate);
impl GameResponse {
    /// Validates acknowledgement coherence, not actor authority or current owner state.
    pub fn validate(&self) -> Result<(), GameDecodeError> {
        let coherent = match self {
            Self::Created {
                game,
                configuration,
                idle_cancel_due_at,
                receipt,
            } => {
                receipt.validate()?;
                configuration.validate().map_err(|_| GameDecodeError)?;
                game.state == GameState::New
                    && game.game_id == receipt.game_id
                    && game.created_at <= receipt.completed_at
                    && *idle_cancel_due_at == checked_deadline(game.created_at, PRE_START_IDLE_MS)?
                    && game.view_revision == 0
                    && matches!(&receipt.outcome, GameOutcome::Created { view_revision, idle_cancel_due_at: due }
                        if *view_revision == game.view_revision && due == idle_cancel_due_at)
            }
            Self::LobbyOpened {
                game_id,
                state,
                configuration,
                view_revision,
                receipt,
                ..
            } => {
                receipt.validate()?;
                configuration.validate().map_err(|_| GameDecodeError)?;
                *state == GameState::AwaitingPlayers
                    && *game_id == receipt.game_id
                    && matches!(&receipt.outcome, GameOutcome::LobbyOpened { view_revision: revision }
                        if revision == view_revision)
            }
            Self::Started {
                game_id,
                state,
                started_at,
                view_revision,
                receipt,
            } => {
                receipt.validate()?;
                *state == GameState::InProgress
                    && *game_id == receipt.game_id
                    && matches!(&receipt.outcome, GameOutcome::Started { view_revision: revision, started_at: start }
                        if revision == view_revision && start == started_at)
            }
            Self::PlayerJoined {
                player_id,
                alias,
                session_expires_at,
                view_revision,
                receipt,
            } => {
                receipt.validate()?;
                validate_alias(alias).map_err(|_| GameDecodeError)? == alias
                    && matches!(&receipt.outcome, GameOutcome::PlayerJoined { player_id: player, view_revision: revision, session_expires_at: expiry }
                        if player == player_id && revision == view_revision && expiry == session_expires_at)
            }
            Self::Committed { receipt } => {
                receipt.validate()?;
                true
            }
            Self::Pending { .. } => true,
            Self::AdmissionContext { expires_at } => {
                validate_timestamp(*expires_at)?;
                *expires_at > 0
            }
        };
        if !coherent {
            return Err(GameDecodeError);
        }
        Ok(())
    }
    pub fn encode_json(&self) -> Result<Vec<u8>, GameDecodeError> {
        self.validate()?;
        encode(self, GAME_FRAME_MAX_BYTES)
    }
    pub fn decode_json(bytes: &[u8]) -> Result<Self, GameDecodeError> {
        decode(bytes, GAME_FRAME_MAX_BYTES)
    }
    pub const fn status(&self) -> u16 {
        if matches!(self, Self::Pending { .. }) {
            202
        } else {
            200
        }
    }
}
use brews_domain::{
    games::{BoardCell, BoardCellKind, CompletedLine},
    ids::ConnectionId,
};
fn cell_kind<'de, D: serde::Deserializer<'de>>(d: D) -> Result<BoardCellKind, D::Error> {
    map_enum!(Wire, "kind", {
        Free {},
        Value { value: String },
    });
    Ok(match Wire::deserialize(d)? {
        Wire::Free {} => BoardCellKind::Free,
        Wire::Value { value } => BoardCellKind::Value(value),
    })
}
fn completed_lines<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<CompletedLine>, D::Error> {
    map_enum!(Wire, "kind", {
        Row { index: u8 },
        Column { index: u8 },
        MainDiagonal {},
        AntiDiagonal {},
    });
    let lines = Vec::<Wire>::deserialize(d)?;
    if lines.len() > usize::from(brews_domain::games::BOARD_SIDE_MAX) * 2 + 2 {
        return Err(serde::de::Error::custom("Too many completed lines."));
    }
    lines
        .into_iter()
        .map(|line| match line {
            Wire::Row { index } | Wire::Column { index }
                if index == 0 || index > brews_domain::games::BOARD_SIDE_MAX =>
            {
                Err(serde::de::Error::custom("Line index out of range."))
            }
            Wire::Row { index } => Ok(CompletedLine::Row(index)),
            Wire::Column { index } => Ok(CompletedLine::Column(index)),
            Wire::MainDiagonal {} => Ok(CompletedLine::MainDiagonal),
            Wire::AntiDiagonal {} => Ok(CompletedLine::AntiDiagonal),
        })
        .collect()
}
fn cells<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<BoardCell>, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct CellWire {
        position: Position,
        #[serde(deserialize_with = "cell_kind")]
        kind: BoardCellKind,
        is_matched: bool,
    }
    struct CellObject(CellWire);
    impl<'de> Deserialize<'de> for CellObject {
        fn deserialize<E: serde::Deserializer<'de>>(d: E) -> Result<Self, E::Error> {
            object(d).map(Self)
        }
    }
    let values = Vec::<CellObject>::deserialize(d)?;
    if values.len() > BOARD_CELLS_MAX {
        return Err(serde::de::Error::custom("Too many cells."));
    }
    Ok(values
        .into_iter()
        .map(|v| BoardCell {
            position: CellPosition {
                row: v.0.position.row,
                column: v.0.position.column,
            },
            kind: v.0.kind,
            is_matched: v.0.is_matched,
        })
        .collect())
}
map_struct!(pub BoardView {side_length:u8,#[serde(deserialize_with="cells")] cells:Vec<BoardCell>,qualified:bool,#[serde(deserialize_with="completed_lines")] qualifying_lines:Vec<CompletedLine>});
map_struct!(pub HostPlayerView {player_id:PlayerId,#[serde(deserialize_with="canonical_alias")] alias:String,connected:bool,#[serde(default,deserialize_with="non_null_option",skip_serializing_if="Option::is_none")] board:Option<BoardView>});
map_enum!(pub GameView, "role", {
    Host {
        game: GameSummary,
        #[serde(deserialize_with = "configuration")]
        configuration: GameConfiguration,
        #[serde(deserialize_with = "nullable_value")]
        game_code: Option<brews_domain::games::GameCode>,
        #[serde(deserialize_with = "nullable_value")]
        started_at: Option<i64>,
        #[serde(deserialize_with = "nullable_value")]
        ended_at: Option<i64>,
        #[serde(deserialize_with = "nullable_value")]
        idle_cancel_due_at: Option<i64>,
        players: Vec<HostPlayerView>,
        connected_player_count: u8,
        spectator_count: u8,
        connected_spectator_count: u8,
    },
    Player {
        game_id: GameId,
        state: GameState,
        #[serde(deserialize_with = "configuration")]
        configuration: GameConfiguration,
        #[serde(deserialize_with = "nullable_value")]
        game_code: Option<brews_domain::games::GameCode>,
        #[serde(deserialize_with = "nullable_value")]
        started_at: Option<i64>,
        #[serde(deserialize_with = "nullable_value")]
        ended_at: Option<i64>,
        #[serde(deserialize_with = "safe_revision")]
        view_revision: u64,
        player_id: PlayerId,
        alias: String,
        #[serde(default, deserialize_with = "non_null_option", skip_serializing_if = "Option::is_none")]
        board: Option<BoardView>,
    },
}; validate);
impl GameView {
    pub const fn game_id(&self) -> GameId {
        match self {
            Self::Host { game, .. } => game.game_id,
            Self::Player { game_id, .. } => *game_id,
        }
    }
    pub const fn view_revision(&self) -> u64 {
        match self {
            Self::Host { game, .. } => game.view_revision,
            Self::Player { view_revision, .. } => *view_revision,
        }
    }
    pub fn decode_json(bytes: &[u8]) -> Result<Self, GameDecodeError> {
        decode(bytes, GAME_FRAME_MAX_BYTES)
    }
    pub fn validate(&self) -> Result<(), GameDecodeError> {
        if self.view_revision() > MAX_SAFE_REVISION {
            return Err(GameDecodeError);
        }
        match self {
            Self::Host {
                game,
                configuration,
                players,
                connected_player_count,
                spectator_count,
                connected_spectator_count,
                game_code,
                started_at,
                ended_at,
                idle_cancel_due_at,
            } => {
                configuration.validate().map_err(|_| GameDecodeError)?;
                validate_timestamp(game.created_at)?;
                validate_lifecycle(game.state, game_code.is_some(), *started_at, *ended_at)?;
                if started_at.is_some_and(|start| start < game.created_at)
                    || ended_at.is_some_and(|end| end < game.created_at)
                {
                    return Err(GameDecodeError);
                }
                if matches!(game.state, GameState::New | GameState::AwaitingPlayers) {
                    let due = idle_cancel_due_at.ok_or(GameDecodeError)?;
                    validate_timestamp(due)?;
                    if due < checked_deadline(game.created_at, PRE_START_IDLE_MS)? {
                        return Err(GameDecodeError);
                    }
                } else if idle_cancel_due_at.is_some() {
                    return Err(GameDecodeError);
                }
                if game.state == GameState::New && (!players.is_empty() || *spectator_count != 0) {
                    return Err(GameDecodeError);
                }
                if players.len() > usize::from(configuration.player_capacity)
                    || usize::from(*connected_player_count)
                        != players.iter().filter(|p| p.connected).count()
                    || *spectator_count > configuration.spectator_capacity
                    || connected_spectator_count > spectator_count
                {
                    return Err(GameDecodeError);
                }
                for (i, p) in players.iter().enumerate() {
                    if players[..i]
                        .iter()
                        .any(|a| a.player_id == p.player_id || a.alias == p.alias)
                    {
                        return Err(GameDecodeError);
                    }
                    validate_projection_player(
                        game.state,
                        configuration,
                        &p.alias,
                        p.board.as_ref(),
                    )?;
                }
            }
            Self::Player {
                state,
                configuration,
                alias,
                board,
                game_code,
                started_at,
                ended_at,
                ..
            } => {
                configuration.validate().map_err(|_| GameDecodeError)?;
                validate_lifecycle(*state, game_code.is_some(), *started_at, *ended_at)?;
                if *state == GameState::New
                    || (*state == GameState::AwaitingPlayers && game_code.is_none())
                {
                    return Err(GameDecodeError);
                }
                validate_projection_player(*state, configuration, alias, board.as_ref())?;
            }
        }
        Ok(())
    }
}
fn validate_lifecycle(
    state: GameState,
    has_code: bool,
    started_at: Option<i64>,
    ended_at: Option<i64>,
) -> Result<(), GameDecodeError> {
    for time in [started_at, ended_at].into_iter().flatten() {
        validate_timestamp(time)?;
    }
    if started_at
        .zip(ended_at)
        .is_some_and(|(start, end)| end < start)
    {
        return Err(GameDecodeError);
    }
    let coherent = match state {
        GameState::New => !has_code && started_at.is_none() && ended_at.is_none(),
        // Host code is intentionally hidden until Directory publication commits.
        GameState::AwaitingPlayers => started_at.is_none() && ended_at.is_none(),
        GameState::InProgress => has_code && started_at.is_some() && ended_at.is_none(),
        GameState::Resolved => has_code && started_at.is_some() && ended_at.is_some(),
        GameState::Cancelled => ended_at.is_some() && (started_at.is_none() || has_code),
    };
    if !coherent {
        return Err(GameDecodeError);
    }
    Ok(())
}
fn validate_projection_player(
    state: GameState,
    config: &GameConfiguration,
    alias: &str,
    board: Option<&BoardView>,
) -> Result<(), GameDecodeError> {
    if validate_alias(alias).map_err(|_| GameDecodeError)? != alias {
        return Err(GameDecodeError);
    }
    if matches!(state, GameState::New | GameState::AwaitingPlayers) && board.is_some() {
        return Err(GameDecodeError);
    }
    if state == GameState::InProgress && board.is_none() {
        return Err(GameDecodeError);
    }
    if let Some(board) = board {
        if board.side_length != config.board_side_length {
            return Err(GameDecodeError);
        }
        let lines =
            <brews_domain::games::SingleLine as brews_domain::games::SingleLinePattern>::evaluate(
                board.side_length,
                &board.cells,
            )
            .map_err(|_| GameDecodeError)?;
        if lines != board.qualifying_lines || board.qualified != !lines.is_empty() {
            return Err(GameDecodeError);
        }
        for (i, cell) in board.cells.iter().enumerate() {
            if (cell.kind == BoardCellKind::Free)
                != config.free_cell_positions.contains(&cell.position)
            {
                return Err(GameDecodeError);
            }
            if let BoardCellKind::Value(value) = &cell.kind {
                let n = value.parse::<u32>().map_err(|_| GameDecodeError)?;
                if n == 0
                    || n > config.numeric_upper_bound
                    || value != &n.to_string()
                    || board.cells[..i].iter().any(|c| c.kind == cell.kind)
                {
                    return Err(GameDecodeError);
                }
            }
        }
    }
    Ok(())
}
/// Non-bearer transport correlation, not an authority or a proof of randomness.
#[derive(Clone, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct DeliveryId(String);
pub const DELIVERY_ID_BYTES: usize = 32;
pub const DELIVERY_ID_ENCODED_BYTES: usize = 43;
const BASE64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
impl std::str::FromStr for DeliveryId {
    type Err = GameDecodeError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() != DELIVERY_ID_ENCODED_BYTES
            || !value.bytes().all(|b| BASE64URL.contains(&b))
            || !b"AEIMQUYcgkosw048".contains(&value.as_bytes()[DELIVERY_ID_ENCODED_BYTES - 1])
        {
            return Err(GameDecodeError);
        }
        Ok(Self(value.to_owned()))
    }
}
impl<'de> Deserialize<'de> for DeliveryId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}
impl std::fmt::Debug for DeliveryId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DeliveryId([redacted])")
    }
}
impl DeliveryId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotKind {
    Snapshot,
}
pub const SNAPSHOT_ACK_MAX_BYTES: usize = 512;
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotAckKind {
    SnapshotAck,
}
map_struct!(pub SnapshotAck {
    #[serde(deserialize_with = "version_one")] version: u8,
    kind: SnapshotAckKind,
    connection_id: ConnectionId,
    delivery_id: DeliveryId,
    #[serde(deserialize_with = "safe_revision")] view_revision: u64
});
impl SnapshotAck {
    pub fn decode_json(bytes: &[u8]) -> Result<Self, GameDecodeError> {
        decode(bytes, SNAPSHOT_ACK_MAX_BYTES)
    }
    pub fn encode_json(&self) -> Result<Vec<u8>, GameDecodeError> {
        let bytes = encode(self, SNAPSHOT_ACK_MAX_BYTES)?;
        Self::decode_json(&bytes)?;
        Ok(bytes)
    }
}
map_struct!(pub SnapshotFrame {
    #[serde(deserialize_with = "version_one")]
    version: u8,
    kind: SnapshotKind,
    game_id: GameId,
    #[serde(deserialize_with = "safe_revision")]
    view_revision: u64,
    connection_id: ConnectionId,
    session_expires_at: i64,
    view: GameView,
    delivery_id: DeliveryId
}; validate);
fn nullable_snapshot<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<GameView>, D::Error> {
    Option::<GameView>::deserialize(d)
}
map_struct!(pub SyncResponse {up_to_date:bool,#[serde(deserialize_with="safe_revision")] view_revision:u64,#[serde(deserialize_with="nullable_snapshot")] snapshot:Option<GameView>}; validate);
impl SnapshotFrame {
    pub fn validate(&self) -> Result<(), GameDecodeError> {
        self.view.validate()?;
        validate_timestamp(self.session_expires_at)?;
        if self.version != 1
            || self.session_expires_at == 0
            || self.game_id != self.view.game_id()
            || self.view_revision != self.view.view_revision()
        {
            return Err(GameDecodeError);
        }
        Ok(())
    }
    pub fn decode_json(bytes: &[u8]) -> Result<Self, GameDecodeError> {
        decode(bytes, GAME_FRAME_MAX_BYTES)
    }
    pub fn encode_json(&self) -> Result<Vec<u8>, GameDecodeError> {
        self.validate()?;
        encode(self, GAME_FRAME_MAX_BYTES)
    }
}
impl SyncResponse {
    pub fn validate(&self) -> Result<(), GameDecodeError> {
        if self.view_revision > MAX_SAFE_REVISION || self.up_to_date != self.snapshot.is_none() {
            return Err(GameDecodeError);
        }
        if let Some(view) = &self.snapshot {
            view.validate()?;
            if view.view_revision() != self.view_revision {
                return Err(GameDecodeError);
            }
        }
        Ok(())
    }
    pub fn decode_json(bytes: &[u8]) -> Result<Self, GameDecodeError> {
        decode(bytes, GAME_FRAME_MAX_BYTES)
    }
    pub fn encode_json(&self) -> Result<Vec<u8>, GameDecodeError> {
        self.validate()?;
        encode(self, GAME_FRAME_MAX_BYTES)
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "Tests fail fast on invalid bounded fixtures."
)]
mod tests {
    use super::*;
    const ID: &str = "01890f3e-53b7-7d28-9b05-4f65092d5711";
    #[test]
    fn review_map_receipt_outcome_sequence() {
        let wire = r##"{"version":1,"command_id":"01890f3e-53b7-7d28-9b05-4f65092d5711","game_id":"01890f3e-53b7-7d28-9b05-4f65092d5711","outcome":["created",0,86400001],"completed_at":1,"expires_at":86400001}"##;
        assert!(
            GameReceipt::decode_json(wire.as_bytes()).is_err(),
            "positional receipt_outcome_sequence accepted by byte decoder"
        );
        assert!(
            serde_json::from_str::<GameReceipt>(wire).is_err(),
            "positional receipt_outcome_sequence accepted by Deserialize"
        );
    }
    #[test]
    fn review_map_sync_sequence_game_view() {
        let wire = r##"{"up_to_date":false,"view_revision":1,"snapshot":["player","01890f3e-53b7-7d28-9b05-4f65092d5711","awaiting_players",{"numeric_upper_bound":75,"board_side_length":5,"free_cells_enabled":true,"free_cell_positions":[{"row":3,"column":3}],"player_capacity":20,"spectator_capacity":50,"winning_pattern":"single_line"},"AB12CD34",null,null,1,"01890f3e-53b7-7d28-9b05-4f65092d5711","Alice",null]}"##;
        assert!(
            SyncResponse::decode_json(wire.as_bytes()).is_err(),
            "positional sync_sequence_game_view accepted by byte decoder"
        );
        assert!(
            serde_json::from_str::<SyncResponse>(wire).is_err(),
            "positional sync_sequence_game_view accepted by Deserialize"
        );
    }
    #[test]
    fn review_map_board_free_kind_sequence() {
        let wire = r##"{"role":"player","game_id":"01890f3e-53b7-7d28-9b05-4f65092d5711","state":"in_progress","configuration":{"numeric_upper_bound":75,"board_side_length":2,"free_cells_enabled":true,"free_cell_positions":[{"row":1,"column":1}],"player_capacity":20,"spectator_capacity":50,"winning_pattern":"single_line"},"game_code":"AB12CD34","started_at":2,"ended_at":null,"view_revision":1,"player_id":"01890f3e-53b7-7d28-9b05-4f65092d5711","alias":"Alice","board":{"side_length":2,"cells":[{"position":{"row":1,"column":1},"kind":["free"],"is_matched":true},{"position":{"row":1,"column":2},"kind":{"kind":"value","value":"1"},"is_matched":false},{"position":{"row":2,"column":1},"kind":{"kind":"value","value":"2"},"is_matched":false},{"position":{"row":2,"column":2},"kind":{"kind":"value","value":"3"},"is_matched":false}],"qualified":false,"qualifying_lines":[]}}"##;
        assert!(
            GameView::decode_json(wire.as_bytes()).is_err(),
            "positional board_free_kind_sequence accepted by byte decoder"
        );
        assert!(
            serde_json::from_str::<GameView>(wire).is_err(),
            "positional board_free_kind_sequence accepted by Deserialize"
        );
    }
    #[test]
    fn review_map_board_value_kind_sequence() {
        let wire = r##"{"role":"player","game_id":"01890f3e-53b7-7d28-9b05-4f65092d5711","state":"in_progress","configuration":{"numeric_upper_bound":75,"board_side_length":2,"free_cells_enabled":true,"free_cell_positions":[{"row":1,"column":1}],"player_capacity":20,"spectator_capacity":50,"winning_pattern":"single_line"},"game_code":"AB12CD34","started_at":2,"ended_at":null,"view_revision":1,"player_id":"01890f3e-53b7-7d28-9b05-4f65092d5711","alias":"Alice","board":{"side_length":2,"cells":[{"position":{"row":1,"column":1},"kind":{"kind":"free"},"is_matched":true},{"position":{"row":1,"column":2},"kind":["value","1"],"is_matched":false},{"position":{"row":2,"column":1},"kind":{"kind":"value","value":"2"},"is_matched":false},{"position":{"row":2,"column":2},"kind":{"kind":"value","value":"3"},"is_matched":false}],"qualified":false,"qualifying_lines":[]}}"##;
        assert!(
            GameView::decode_json(wire.as_bytes()).is_err(),
            "positional board_value_kind_sequence accepted by byte decoder"
        );
        assert!(
            serde_json::from_str::<GameView>(wire).is_err(),
            "positional board_value_kind_sequence accepted by Deserialize"
        );
    }
    #[test]
    fn review_map_completed_lines_sequences() {
        let wire = r##"{"role":"player","game_id":"01890f3e-53b7-7d28-9b05-4f65092d5711","state":"in_progress","configuration":{"numeric_upper_bound":75,"board_side_length":2,"free_cells_enabled":true,"free_cell_positions":[{"row":1,"column":1}],"player_capacity":20,"spectator_capacity":50,"winning_pattern":"single_line"},"game_code":"AB12CD34","started_at":2,"ended_at":null,"view_revision":1,"player_id":"01890f3e-53b7-7d28-9b05-4f65092d5711","alias":"Alice","board":{"side_length":2,"cells":[{"position":{"row":1,"column":1},"kind":{"kind":"free"},"is_matched":true},{"position":{"row":1,"column":2},"kind":{"kind":"value","value":"1"},"is_matched":true},{"position":{"row":2,"column":1},"kind":{"kind":"value","value":"2"},"is_matched":true},{"position":{"row":2,"column":2},"kind":{"kind":"value","value":"3"},"is_matched":true}],"qualified":true,"qualifying_lines":[["row",1],["row",2],["column",1],["column",2],["main_diagonal"],["anti_diagonal"]]}}"##;
        assert!(
            GameView::decode_json(wire.as_bytes()).is_err(),
            "positional completed_lines_sequences accepted by byte decoder"
        );
        assert!(
            serde_json::from_str::<GameView>(wire).is_err(),
            "positional completed_lines_sequences accepted by Deserialize"
        );
    }
    #[test]
    fn review_map_game_view_trait_rejects_positional_projection() {
        let config = serde_json::to_string(&GameConfiguration::default()).unwrap();
        let wire = format!(
            r#"["player","{ID}","awaiting_players",{config},"AB12CD34",null,null,1,"{ID}","Alice",null]"#
        );
        assert!(
            serde_json::from_str::<GameView>(&wire).is_err(),
            "positional GameView accepted by Deserialize"
        );
    }
    #[test]
    fn review_map_game_response_trait_rejects_positional_result() {
        let wire = format!(r#"["pending","{ID}"]"#);
        assert!(
            serde_json::from_str::<GameResponse>(&wire).is_err(),
            "positional GameResponse accepted by Deserialize"
        );
    }

    #[test]
    fn review_receipt_rejects_negative_completion() {
        let raw = receipt().replace("\"completed_at\":1", "\"completed_at\":-1");
        assert!(GameReceipt::decode_json(raw.as_bytes()).is_err());
        assert!(serde_json::from_str::<GameReceipt>(&raw).is_err());
    }
    #[test]
    fn review_receipt_rejects_inverted_expiry() {
        let raw = receipt().replace("\"expires_at\":86400001", "\"expires_at\":0");
        assert!(GameReceipt::decode_json(raw.as_bytes()).is_err());
        assert!(serde_json::from_str::<GameReceipt>(&raw).is_err());
    }
    #[test]
    fn review_receipt_encoding_rejects_negative_completion() {
        let mut receipt = GameReceipt::decode_json(receipt().as_bytes()).unwrap();
        receipt.completed_at = -1;
        assert!(receipt.encode_json().is_err());
    }
    #[test]
    fn review_receipt_encoding_rejects_inverted_expiry() {
        let mut receipt = GameReceipt::decode_json(receipt().as_bytes()).unwrap();
        receipt.expires_at = 0;
        assert!(receipt.encode_json().is_err());
    }
    #[test]
    fn review_receipt_requires_exact_checked_one_day_retention() {
        for expiry in [1, 86400000, 86400002, 9007199254740992, i64::MAX] {
            let raw = receipt().replace(
                "\"expires_at\":86400001",
                &format!("\"expires_at\":{expiry}"),
            );
            assert!(
                GameReceipt::decode_json(raw.as_bytes()).is_err(),
                "bad expiry: {expiry}"
            );
            assert!(serde_json::from_str::<GameReceipt>(&raw).is_err());
        }
        let mut value = GameReceipt::decode_json(receipt().as_bytes()).unwrap();
        value.completed_at = i64::MAX;
        value.expires_at = i64::MAX;
        assert!(value.encode_json().is_err());
    }
    #[test]
    fn review_receipt_outcome_times_are_coherent_with_completion() {
        for outcome in [
            r#"{"operation":"created","view_revision":0,"idle_cancel_due_at":1}"#,
            r#"{"operation":"created","view_revision":0,"idle_cancel_due_at":86400002}"#,
            r#"{"operation":"started","view_revision":2,"started_at":2}"#,
            r#"{"operation":"started","view_revision":2,"started_at":-1}"#,
            r#"{"operation":"player_joined","player_id":"01890f3e-53b7-7d28-9b05-4f65092d5711","view_revision":0,"session_expires_at":86400000}"#,
        ] {
            let raw = format!(
                r#"{{"version":1,"command_id":"{ID}","game_id":"{ID}","outcome":{outcome},"completed_at":1,"expires_at":86400001}}"#
            );
            assert!(
                GameReceipt::decode_json(raw.as_bytes()).is_err(),
                "incoherent receipt outcome"
            );
            assert!(serde_json::from_str::<GameReceipt>(&raw).is_err());
        }
    }
    fn operation_receipt(outcome: &str, completed_at: i64) -> String {
        let expires_at = completed_at.checked_add(86_400_000).unwrap();
        format!(
            r#"{{"version":1,"command_id":"{ID}","game_id":"{ID}","outcome":{outcome},"completed_at":{completed_at},"expires_at":{expires_at}}}"#
        )
    }
    fn valid_start_response() -> String {
        let receipt = operation_receipt(
            r#"{"operation":"started","view_revision":2,"started_at":2}"#,
            2,
        );
        format!(
            r#"{{"result":"started","game_id":"{ID}","state":"in_progress","started_at":2,"view_revision":2,"receipt":{receipt}}}"#
        )
    }
    fn valid_join_response() -> String {
        let outcome = format!(
            r#"{{"operation":"player_joined","player_id":"{ID}","view_revision":0,"session_expires_at":86400002}}"#
        );
        let receipt = operation_receipt(&outcome, 2);
        format!(
            r#"{{"result":"player_joined","player_id":"{ID}","alias":"Alice","session_expires_at":86400002,"view_revision":0,"receipt":{receipt}}}"#
        )
    }
    fn assert_response_rejected(wire: &str) {
        assert!(
            GameResponse::decode_json(wire.as_bytes()).is_err(),
            "incoherent result accepted by byte decoder"
        );
        assert!(
            serde_json::from_str::<GameResponse>(wire).is_err(),
            "incoherent result accepted by Deserialize"
        );
    }
    #[test]
    fn review_response_started_requires_started_outcome() {
        let wire = valid_start_response();
        assert!(GameResponse::decode_json(wire.as_bytes()).is_ok());
        let receipt = operation_receipt(
            r#"{"operation":"started","view_revision":2,"started_at":2}"#,
            2,
        );
        assert_response_rejected(&wire.replace(&receipt, &self::receipt()));
    }
    #[test]
    fn review_response_started_requires_same_game_id() {
        let wire = valid_start_response().replacen(ID, "01890f3e-53b7-7d28-9b05-4f65092d5712", 1);
        assert_response_rejected(&wire);
    }
    #[test]
    fn review_response_started_requires_in_progress() {
        assert_response_rejected(&valid_start_response().replace("in_progress", "new"));
    }
    #[test]
    fn review_response_started_requires_matching_revision() {
        assert_response_rejected(&valid_start_response().replacen(
            "\"view_revision\":2",
            "\"view_revision\":3",
            1,
        ));
    }
    #[test]
    fn review_response_started_requires_matching_timestamp() {
        assert_response_rejected(&valid_start_response().replacen(
            "\"started_at\":2",
            "\"started_at\":1",
            1,
        ));
    }
    #[test]
    fn review_response_join_requires_joined_outcome() {
        let wire = valid_join_response();
        assert!(GameResponse::decode_json(wire.as_bytes()).is_ok());
        let outcome = format!(
            r#"{{"operation":"player_joined","player_id":"{ID}","view_revision":0,"session_expires_at":86400002}}"#
        );
        assert_response_rejected(&wire.replace(&operation_receipt(&outcome, 2), &receipt()));
    }
    #[test]
    fn review_response_join_requires_matching_member_revision_and_expiry() {
        for wire in [
            valid_join_response().replacen(ID, "01890f3e-53b7-7d28-9b05-4f65092d5712", 1),
            valid_join_response().replacen("\"view_revision\":0", "\"view_revision\":1", 1),
            valid_join_response().replacen(
                "\"session_expires_at\":86400002",
                "\"session_expires_at\":86400003",
                1,
            ),
        ] {
            assert_response_rejected(&wire);
        }
    }
    #[test]
    fn review_response_created_matches_summary_and_initial_deadline() {
        let config = serde_json::to_string(&GameConfiguration::default()).unwrap();
        let receipt = receipt();
        let wire = format!(
            r#"{{"result":"created","game":{{"game_id":"{ID}","state":"new","designated_host_id":"{ID}","created_at":1,"view_revision":0}},"configuration":{config},"idle_cancel_due_at":86400001,"receipt":{receipt}}}"#
        );
        assert!(GameResponse::decode_json(wire.as_bytes()).is_ok());
        for bad in [
            wire.replace("\"state\":\"new\"", "\"state\":\"awaiting_players\""),
            wire.replacen(ID, "01890f3e-53b7-7d28-9b05-4f65092d5712", 1),
            wire.replacen("\"view_revision\":0", "\"view_revision\":1", 1),
            wire.replacen(
                "\"idle_cancel_due_at\":86400001",
                "\"idle_cancel_due_at\":86400002",
                1,
            ),
            wire.replace("\"created_at\":1", "\"created_at\":2"),
        ] {
            assert_response_rejected(&bad);
        }
    }
    #[test]
    fn review_response_lobby_requires_matching_lobby_acknowledgement() {
        let config = serde_json::to_string(&GameConfiguration::default()).unwrap();
        let ack = operation_receipt(r#"{"operation":"lobby_opened","view_revision":1}"#, 2);
        let wire = format!(
            r#"{{"result":"lobby_opened","game_id":"{ID}","state":"awaiting_players","game_code":"AB12CD34","configuration":{config},"view_revision":1,"receipt":{ack}}}"#
        );
        assert!(GameResponse::decode_json(wire.as_bytes()).is_ok());
        for bad in [
            wire.replace(&ack, &receipt()),
            wire.replace("awaiting_players", "in_progress"),
            wire.replacen(ID, "01890f3e-53b7-7d28-9b05-4f65092d5712", 1),
            wire.replacen("\"view_revision\":1", "\"view_revision\":2", 1),
        ] {
            assert_response_rejected(&bad);
        }
    }
    #[test]
    fn review_response_admission_expiry_is_positive_and_javascript_safe() {
        for expiry in [0, -1, 9007199254740992, i64::MAX] {
            assert_response_rejected(&format!(
                r#"{{"result":"admission_context","expires_at":{expiry}}}"#
            ));
        }
    }
    fn player_lobby_view() -> String {
        let config = serde_json::to_string(&GameConfiguration::default()).unwrap();
        format!(
            r#"{{"role":"player","game_id":"{ID}","state":"awaiting_players","configuration":{config},"game_code":"AB12CD34","started_at":null,"ended_at":null,"view_revision":0,"player_id":"{ID}","alias":"Alice"}}"#
        )
    }
    fn host_new_view() -> String {
        let config = serde_json::to_string(&GameConfiguration::default()).unwrap();
        format!(
            r#"{{"role":"host","game":{{"game_id":"{ID}","state":"new","designated_host_id":"{ID}","created_at":1,"view_revision":0}},"configuration":{config},"game_code":null,"started_at":null,"ended_at":null,"idle_cancel_due_at":86400001,"players":[],"connected_player_count":0,"spectator_count":0,"connected_spectator_count":0}}"#
        )
    }
    fn assert_view_rejected(wire: &str) {
        assert!(
            GameView::decode_json(wire.as_bytes()).is_err(),
            "incoherent view accepted by byte decoder"
        );
        assert!(
            serde_json::from_str::<GameView>(wire).is_err(),
            "incoherent view accepted by Deserialize"
        );
    }
    #[test]
    fn review_view_new_has_no_member_projection_or_lobby_metadata() {
        assert_view_rejected(&player_lobby_view().replace("awaiting_players", "new"));
        let wire = host_new_view();
        assert!(GameView::decode_json(wire.as_bytes()).is_ok());
        for bad in [
            wire.replace("\"game_code\":null", "\"game_code\":\"AB12CD34\""),
            wire.replace("\"started_at\":null", "\"started_at\":1"),
            wire.replace("\"ended_at\":null", "\"ended_at\":1"),
            wire.replace(
                "\"players\":[]",
                &format!(r#""players":[{{"player_id":"{ID}","alias":"Alice","connected":false}}]"#),
            ),
            wire.replace("\"spectator_count\":0", "\"spectator_count\":1"),
        ] {
            assert_view_rejected(&bad);
        }
    }
    #[test]
    fn review_view_awaiting_requires_pre_start_lifecycle_but_allows_hidden_host_code() {
        let host = host_new_view().replace("\"state\":\"new\"", "\"state\":\"awaiting_players\"");
        assert!(GameView::decode_json(host.as_bytes()).is_ok());
        for bad in [
            player_lobby_view().replace("\"game_code\":\"AB12CD34\"", "\"game_code\":null"),
            player_lobby_view().replace("\"started_at\":null", "\"started_at\":1"),
            player_lobby_view().replace("\"ended_at\":null", "\"ended_at\":1"),
            host.replace(
                "\"idle_cancel_due_at\":86400001",
                "\"idle_cancel_due_at\":null",
            ),
        ] {
            assert_view_rejected(&bad);
        }
    }
    #[test]
    fn review_view_in_progress_requires_code_and_start_without_end_or_idle() {
        let wire = host_new_view()
            .replace("\"state\":\"new\"", "\"state\":\"in_progress\"")
            .replace("\"game_code\":null", "\"game_code\":\"AB12CD34\"")
            .replace("\"started_at\":null", "\"started_at\":2")
            .replace(
                "\"idle_cancel_due_at\":86400001",
                "\"idle_cancel_due_at\":null",
            );
        assert!(GameView::decode_json(wire.as_bytes()).is_ok());
        for bad in [
            wire.replace("\"game_code\":\"AB12CD34\"", "\"game_code\":null"),
            wire.replace("\"started_at\":2", "\"started_at\":null"),
            wire.replace("\"ended_at\":null", "\"ended_at\":3"),
            wire.replace(
                "\"idle_cancel_due_at\":null",
                "\"idle_cancel_due_at\":86400001",
            ),
            wire.replace("\"started_at\":2", "\"started_at\":0"),
        ] {
            assert_view_rejected(&bad);
        }
    }
    #[test]
    fn review_view_timestamps_are_safe_and_host_idle_is_coherent() {
        let host = host_new_view();
        for bad in [
            host.replace("\"created_at\":1", "\"created_at\":-1"),
            host.replace("\"created_at\":1", "\"created_at\":9007199254740992"),
            host.replace(
                "\"idle_cancel_due_at\":86400001",
                "\"idle_cancel_due_at\":1",
            ),
            host.replace(
                "\"idle_cancel_due_at\":86400001",
                "\"idle_cancel_due_at\":-1",
            ),
            host.replace(
                "\"idle_cancel_due_at\":86400001",
                "\"idle_cancel_due_at\":9007199254740992",
            ),
        ] {
            assert_view_rejected(&bad);
        }
    }
    #[test]
    fn review_view_requires_explicit_nullable_lifecycle_fields() {
        let player = player_lobby_view();
        for field in ["game_code", "started_at", "ended_at"] {
            let missing = if field == "game_code" {
                player.replace("\"game_code\":\"AB12CD34\",", "")
            } else {
                player.replace(&format!("\"{field}\":null,"), "")
            };
            assert_view_rejected(&missing);
        }
        assert_view_rejected(&host_new_view().replace("\"idle_cancel_due_at\":86400001,", ""));
    }
    #[test]
    fn review_view_optional_board_is_absent_not_null() {
        let player = player_lobby_view();
        let null_board = player.trim_end_matches('}').to_owned() + ",\"board\":null}";
        assert_view_rejected(&null_board);
    }
    #[test]
    fn review_view_trait_validates_presence_counts_and_canonical_aliases() {
        let raw = host_new_view().replace(
            "\"connected_player_count\":0",
            "\"connected_player_count\":1",
        );
        assert!(serde_json::from_str::<GameView>(&raw).is_err());
        let raw = player_lobby_view().replace("\"alias\":\"Alice\"", "\"alias\":\" Alice \"");
        assert!(serde_json::from_str::<GameView>(&raw).is_err());
    }
    #[test]
    fn review_view_snapshot_session_expiry_is_positive_and_safe() {
        let view = player_lobby_view();
        for expiry in [0_i64, -1, 9007199254740992] {
            let wire = format!(
                r#"{{"version":1,"kind":"snapshot","game_id":"{ID}","view_revision":0,"connection_id":"{ID}","session_expires_at":{expiry},"view":{view},"delivery_id":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}}"#
            );
            assert!(SnapshotFrame::decode_json(wire.as_bytes()).is_err());
            assert!(serde_json::from_str::<SnapshotFrame>(&wire).is_err());
        }
    }
    #[test]
    fn review_view_sync_trait_rejects_inconsistent_snapshot_metadata() {
        let view = player_lobby_view();
        for wire in [
            format!(r#"{{"up_to_date":true,"view_revision":0,"snapshot":{view}}}"#),
            format!(r#"{{"up_to_date":false,"view_revision":1,"snapshot":{view}}}"#),
        ] {
            assert!(serde_json::from_str::<SyncResponse>(&wire).is_err());
        }
    }
    #[test]
    fn review_range_standalone_outcomes_and_summary_reject_unsafe_timestamps() {
        for time in [-1_i64, 9007199254740992, i64::MAX] {
            for raw in [
                format!(
                    r#"{{"operation":"created","view_revision":0,"idle_cancel_due_at":{time}}}"#
                ),
                format!(r#"{{"operation":"started","view_revision":1,"started_at":{time}}}"#),
                format!(
                    r#"{{"operation":"player_joined","player_id":"{ID}","view_revision":0,"session_expires_at":{time}}}"#
                ),
            ] {
                assert!(
                    serde_json::from_str::<GameOutcome>(&raw).is_err(),
                    "unsafe standalone outcome"
                );
            }
            let raw = format!(
                r#"{{"game_id":"{ID}","state":"new","designated_host_id":"{ID}","created_at":{time},"view_revision":0}}"#
            );
            assert!(serde_json::from_str::<GameSummary>(&raw).is_err());
        }
    }
    #[test]
    fn review_range_created_receipt_is_an_initial_revision_acknowledgement() {
        let raw = receipt().replace("\"view_revision\":0", "\"view_revision\":1");
        assert!(GameReceipt::decode_json(raw.as_bytes()).is_err());
        assert!(serde_json::from_str::<GameReceipt>(&raw).is_err());
        assert_response_rejected(&format!(r#"{{"result":"committed","receipt":{raw}}}"#));
    }
    #[test]
    fn review_range_host_member_alias_is_canonical_at_trait_boundary() {
        let raw = format!(r#"{{"player_id":"{ID}","alias":" Alice ","connected":false}}"#);
        assert!(serde_json::from_str::<HostPlayerView>(&raw).is_err());
    }
    #[test]
    fn hardened_tagged_maps_preserve_original_duplicates_and_closed_fields() {
        let outcome = r#"{"operation":"started","view_revision":2,"started_at":2}"#;
        assert!(serde_json::from_str::<GameOutcome>(outcome).is_ok());
        for raw in [
            outcome.replace(
                "\"operation\":\"started\"",
                "\"operation\":\"started\",\"\\u006fperation\":\"started\"",
            ),
            outcome.replace(
                "\"view_revision\":2",
                "\"view_revision\":2,\"\\u0076iew_revision\":2",
            ),
            outcome.replace(
                "\"started_at\":2",
                "\"started_at\":2,\"\\u0073tarted_at\":2",
            ),
            outcome.trim_end_matches('}').to_owned() + ",\"unknown\":null}",
        ] {
            assert!(serde_json::from_str::<GameOutcome>(&raw).is_err());
        }
        let player = player_lobby_view();
        for raw in [
            player.replace(
                "\"started_at\":null",
                "\"started_at\":null,\"\\u0073tarted_at\":null",
            ),
            player.replace(
                "\"game_code\":\"AB12CD34\"",
                "\"game_code\":\"AB12CD34\",\"\\u0067ame_code\":null",
            ),
            player.trim_end_matches('}').to_owned() + ",\"unknown\":null}",
        ] {
            assert_view_rejected(&raw);
        }
        let response = valid_start_response();
        for raw in [
            response.replace(
                "\"result\":\"started\"",
                "\"result\":\"started\",\"\\u0072esult\":\"started\"",
            ),
            response.trim_end_matches('}').to_owned() + ",\"unknown\":null}",
        ] {
            assert_response_rejected(&raw);
        }
    }
    #[test]
    fn public_board_adapters_leave_domain_serde_compatibility_unchanged() {
        assert_eq!(
            serde_json::from_str::<BoardCellKind>(r#"{"kind":"free","value":null}"#).unwrap(),
            BoardCellKind::Free
        );
        assert_eq!(
            serde_json::from_str::<CompletedLine>(r#"{"kind":"main_diagonal","index":null}"#)
                .unwrap(),
            CompletedLine::MainDiagonal
        );
        let config = r#"[75,5,true,[[3,3]],20,50,"single_line"]"#;
        assert_eq!(
            serde_json::from_str::<GameConfiguration>(config).unwrap(),
            GameConfiguration::default()
        );
    }
    #[test]
    fn created_completion_can_be_later_than_creation_without_moving_idle_deadline() {
        let config = serde_json::to_string(&GameConfiguration::default()).unwrap();
        let ack = operation_receipt(
            r#"{"operation":"created","view_revision":0,"idle_cancel_due_at":86400001}"#,
            2,
        );
        let wire = format!(
            r#"{{"result":"created","game":{{"game_id":"{ID}","state":"new","designated_host_id":"{ID}","created_at":1,"view_revision":0}},"configuration":{config},"idle_cancel_due_at":86400001,"receipt":{ack}}}"#
        );
        let response = GameResponse::decode_json(wire.as_bytes()).unwrap();
        assert!(response.validate().is_ok());
        assert_eq!(response.encode_json().unwrap(), wire.as_bytes());
    }
    #[test]
    fn response_encoder_uses_the_same_acknowledgement_validator() {
        let response = GameResponse::decode_json(valid_start_response().as_bytes()).unwrap();
        assert!(response.validate().is_ok());
        let wire = response.encode_json().unwrap();
        assert!(wire.len() <= GAME_FRAME_MAX_BYTES);
        assert!(GameResponse::decode_json(&wire).is_ok());
        for mutate in 0..5 {
            let mut value = response.clone();
            if let GameResponse::Started {
                game_id,
                state,
                started_at,
                view_revision,
                receipt,
            } = &mut value
            {
                match mutate {
                    0 => *game_id = "01890f3e-53b7-7d28-9b05-4f65092d5712".parse().unwrap(),
                    1 => *state = GameState::New,
                    2 => *started_at = 1,
                    3 => *view_revision = MAX_SAFE_REVISION + 1,
                    _ => receipt.expires_at = receipt.completed_at,
                }
            }
            assert!(value.validate().is_err());
            assert!(value.encode_json().is_err());
        }
        let mut joined = GameResponse::decode_json(valid_join_response().as_bytes()).unwrap();
        if let GameResponse::PlayerJoined { alias, .. } = &mut joined {
            *alias = " Alice ".to_owned();
        }
        assert!(joined.encode_json().is_err());
        let mut receipt = GameReceipt::decode_json(receipt().as_bytes()).unwrap();
        receipt.version = 2;
        assert!(GameResponse::Committed { receipt }.encode_json().is_err());
        let pending = GameResponse::Pending {
            operation_id: ID.parse().unwrap(),
        };
        assert_eq!(
            pending.encode_json().unwrap(),
            format!(r#"{{"result":"pending","operation_id":"{ID}"}}"#).as_bytes()
        );
    }
    #[test]
    fn receipt_checked_retention_accepts_safe_maximum_without_overflow() {
        let completed = MAX_SAFE_TIMESTAMP - GAME_RECEIPT_RETENTION_MS;
        let outcome = format!(
            r#"{{"operation":"started","view_revision":9007199254740991,"started_at":{completed}}}"#
        );
        let raw = operation_receipt(&outcome, completed);
        let value = GameReceipt::decode_json(raw.as_bytes()).unwrap();
        assert_eq!(value.expires_at, MAX_SAFE_TIMESTAMP);
        assert_eq!(value.encode_json().unwrap(), raw.as_bytes());
        let bad = operation_receipt(&outcome, completed + 1);
        assert!(GameReceipt::decode_json(bad.as_bytes()).is_err());
    }
    fn receipt() -> String {
        format!(
            r#"{{"version":1,"command_id":"{ID}","game_id":"{ID}","outcome":{{"operation":"created","view_revision":0,"idle_cancel_due_at":86400001}},"completed_at":1,"expires_at":86400001}}"#
        )
    }
    #[test]
    fn snapshot_delivery_id_round_trips_as_the_exact_last_field() {
        let config = serde_json::to_string(&GameConfiguration::default()).unwrap();
        let delivery = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        let view = format!(
            r#"{{"role":"player","game_id":"{ID}","state":"awaiting_players","configuration":{config},"game_code":"AB12CD34","started_at":null,"ended_at":null,"view_revision":1,"player_id":"{ID}","alias":"Alice"}}"#
        );
        let wire = format!(
            r#"{{"version":1,"kind":"snapshot","game_id":"{ID}","view_revision":1,"connection_id":"{ID}","session_expires_at":9,"view":{view},"delivery_id":"{delivery}"}}"#
        );
        let decoded = SnapshotFrame::decode_json(wire.as_bytes());
        assert!(decoded.is_ok(), "approved delivery field must decode");
        assert_eq!(decoded.unwrap().encode_json().unwrap(), wire.as_bytes());
    }
    #[test]
    fn delivery_id_rejects_noncanonical_32_byte_base64url() {
        for value in [
            "",
            ID,
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAB",
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA+",
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA/",
        ] {
            assert!(
                serde_json::from_str::<DeliveryId>(&format!("\"{value}\"")).is_err(),
                "invalid delivery: {value}"
            );
        }
        let value = "_-7dzLuqmYh3ZlVEMyIRAAARIjNEVWZ3iJmqu8zd7v8";
        assert_eq!(
            serde_json::from_str::<DeliveryId>(&format!("\"{value}\""))
                .unwrap()
                .as_str(),
            value
        );
    }
    #[test]
    fn transport_ack_is_a_closed_bounded_original_map_with_safe_revision() {
        let wire = format!(
            r#"{{"version":1,"kind":"snapshot_ack","connection_id":"{ID}","delivery_id":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA","view_revision":9007199254740991}}"#
        );
        let ack = SnapshotAck::decode_json(wire.as_bytes());
        assert!(ack.is_ok(), "approved acknowledgement must decode");
        assert_eq!(ack.unwrap().encode_json().unwrap(), wire.as_bytes());
        for bad in [
            "[]".to_owned(),
            "null".to_owned(),
            wire.replace("snapshot_ack", "call_number"),
            wire.replace("\"version\":1", "\"version\":2"),
            wire.replace("9007199254740991", "9007199254740992"),
            wire.replace("9007199254740991", "-1"),
            wire.replace("9007199254740991", "18446744073709551616"),
            wire.replace("9007199254740991", "1e0"),
            wire.replace("9007199254740991", "{}"),
            wire.replace("\"version\":1", "\"version\":1,\"\\u0076ersion\":1"),
            wire.replace(
                "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
                "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAB",
            ),
            wire.trim_end_matches('}').to_owned() + ",\"board\":[]}",
            wire.trim_end_matches('}').to_owned() + ",\"delivery_id\":{}}",
            " ".repeat(SNAPSHOT_ACK_MAX_BYTES) + &wire,
        ] {
            assert!(SnapshotAck::decode_json(bad.as_bytes()).is_err(), "{bad}");
            assert!(
                serde_json::from_slice::<SnapshotAck>(bad.as_bytes()).is_err()
                    || bad.len() > SNAPSHOT_ACK_MAX_BYTES
            );
        }
    }
    #[test]
    fn json_output_sink_rejects_overflow_before_growing_or_partial_append() {
        use std::io::Write as _;
        let mut sink = BoundedJsonSink {
            bytes: Vec::with_capacity(3),
            limit: 3,
        };
        assert_eq!(sink.write(b"abc").unwrap(), 3);
        let capacity = sink.bytes.capacity();
        assert!(sink.write(b"d").is_err());
        assert_eq!(sink.bytes, b"abc");
        assert_eq!(sink.bytes.capacity(), capacity);
        let mut sink = BoundedJsonSink {
            bytes: Vec::with_capacity(3),
            limit: 3,
        };
        assert!(sink.write(b"abcd").is_err());
        assert!(sink.bytes.is_empty());
    }
    #[test]
    fn game_tags_and_safe_receipt_encoding_round_trip_without_parser_details() {
        for (state, tag) in [
            (GameState::New, "new"),
            (GameState::AwaitingPlayers, "awaiting_players"),
            (GameState::InProgress, "in_progress"),
            (GameState::Resolved, "resolved"),
            (GameState::Cancelled, "cancelled"),
        ] {
            assert_eq!(serde_json::to_string(&state).unwrap(), format!("\"{tag}\""));
            assert_eq!(tag.parse::<GameState>(), Ok(state));
            assert!(
                state
                    .to_string()
                    .to_uppercase()
                    .parse::<GameState>()
                    .is_err()
            );
        }
        assert_eq!(
            serde_json::to_string(&WinningPattern::SingleLine).unwrap(),
            "\"single_line\""
        );
        let receipt = GameReceipt::decode_json(receipt().as_bytes()).unwrap();
        assert!(receipt.encode_json().unwrap().len() <= GAME_RECEIPT_MAX_BYTES);
        let error = CreateGame::decode_json(b"{invalid").unwrap_err();
        assert_eq!(error, GameDecodeError);
        assert!(std::error::Error::source(&error).is_none());
        assert_eq!(error.to_string(), "Invalid game JSON.");
    }
    #[test]
    fn host_projection_includes_disconnected_retained_boards_without_sessions() {
        let configuration = GameConfiguration {
            board_side_length: 2,
            free_cell_positions: vec![CellPosition { row: 1, column: 1 }],
            ..GameConfiguration::default()
        };
        let ids = [
            ID.parse().unwrap(),
            "01890f3e-53b7-7d28-9b05-4f65092d5712".parse().unwrap(),
        ];
        let mut n = 8u32;
        let boards = brews_domain::games::generate_boards(&configuration, &ids, &mut |bytes| {
            bytes.copy_from_slice(&n.to_le_bytes());
            n += 1;
            Ok(())
        })
        .unwrap();
        let players = boards
            .into_iter()
            .enumerate()
            .map(|(i, b)| HostPlayerView {
                player_id: b.player_id,
                alias: format!("Player{i}"),
                connected: i == 0,
                board: Some(BoardView {
                    side_length: b.side_length,
                    cells: b.cells,
                    qualified: !b.qualifying_lines.is_empty(),
                    qualifying_lines: b.qualifying_lines,
                }),
            })
            .collect();
        let view = GameView::Host {
            game: GameSummary {
                game_id: ID.parse().unwrap(),
                state: GameState::InProgress,
                designated_host_id: ID.parse().unwrap(),
                created_at: 1,
                view_revision: 2,
            },
            configuration,
            game_code: Some("AB12CD34".parse().unwrap()),
            started_at: Some(2),
            ended_at: None,
            idle_cancel_due_at: None,
            players,
            connected_player_count: 1,
            spectator_count: 0,
            connected_spectator_count: 0,
        };
        let json = serde_json::to_string(&view).unwrap();
        assert!(GameView::decode_json(json.as_bytes()).is_ok());
        assert!(!json.contains("session"));
        assert!(!json.contains("recovery"));
        if let GameView::Host { players, .. } = GameView::decode_json(json.as_bytes()).unwrap() {
            assert_eq!(players.len(), 2);
            assert!(!players[1].connected);
            assert!(players[1].board.is_some());
        } else {
            panic!("Expected Host projection");
        }
    }
    #[test]
    fn joined_response_alias_is_canonical_printable_ascii_not_an_unbounded_string() {
        let receipt = receipt();
        for alias in [" Alice ", "é", "xxxxxxxxxxxxxxxxxxxxx", ""] {
            let bytes = format!(
                r#"{{"result":"player_joined","player_id":"{ID}","alias":"{alias}","session_expires_at":1,"view_revision":1,"receipt":{receipt}}}"#
            );
            assert!(GameResponse::decode_json(bytes.as_bytes()).is_err());
            assert!(serde_json::from_slice::<GameResponse>(bytes.as_bytes()).is_err());
        }
    }
    #[test]
    fn sync_response_requires_explicit_nullable_snapshot_field() {
        assert!(SyncResponse::decode_json(br#"{"up_to_date":true,"view_revision":1}"#).is_err());
        assert!(
            serde_json::from_slice::<SyncResponse>(br#"{"up_to_date":true,"view_revision":1}"#)
                .is_err()
        );
    }
    #[test]
    fn valueless_free_cell_and_diagonal_variants_reject_explicit_null_payloads() {
        let board = r#"{"side_length":2,"cells":[{"position":{"row":1,"column":1},"kind":{"kind":"free","value":null},"is_matched":true}],"qualified":false,"qualifying_lines":[]}"#;
        assert!(serde_json::from_str::<BoardView>(board).is_err());
        let lines = r#"{"side_length":2,"cells":[],"qualified":true,"qualifying_lines":[{"kind":"main_diagonal","index":null}]}"#;
        assert!(serde_json::from_str::<BoardView>(lines).is_err());
    }
    #[test]
    fn recovery_answer_explicit_zeroization_clears_owned_sensitive_bytes() {
        use zeroize::Zeroize as _;
        let mut join = JoinPlayer::decode_json(
            br#"{"game_code":"AB12CD34","alias":"Alice","recovery_answer":"private-marker"}"#,
        )
        .unwrap();
        let answer = join.recovery_answer.as_mut().unwrap();
        answer.zeroize();
        assert!(answer.as_str().is_empty());
    }
    #[test]
    fn snapshot_frame_and_sync_validate_version_revision_and_unchanged_null_rule() {
        let config = serde_json::to_string(&GameConfiguration::default()).unwrap();
        let view = format!(
            r#"{{"role":"player","game_id":"{ID}","state":"awaiting_players","configuration":{config},"game_code":"AB12CD34","started_at":null,"ended_at":null,"view_revision":1,"player_id":"{ID}","alias":"Alice"}}"#
        );
        let frame = format!(
            r#"{{"version":1,"kind":"snapshot","game_id":"{ID}","view_revision":1,"connection_id":"{ID}","session_expires_at":9,"view":{view},"delivery_id":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}}"#
        );
        assert!(SnapshotFrame::decode_json(frame.as_bytes()).is_ok());
        let parsed = SnapshotFrame::decode_json(frame.as_bytes()).unwrap();
        assert!(SnapshotFrame::decode_json(&parsed.encode_json().unwrap()).is_ok());
        let unchanged =
            SyncResponse::decode_json(br#"{"up_to_date":true,"view_revision":1,"snapshot":null}"#)
                .unwrap();
        assert!(SyncResponse::decode_json(&unchanged.encode_json().unwrap()).is_ok());
        assert!(
            SnapshotFrame::decode_json(frame.replace("\"version\":1", "\"version\":2").as_bytes())
                .is_err()
        );
        assert!(
            SnapshotFrame::decode_json(
                frame
                    .replacen("\"view_revision\":1", "\"view_revision\":2", 1)
                    .as_bytes()
            )
            .is_err()
        );
        assert!(SnapshotFrame::decode_json(frame.replace(&view, "[]").as_bytes()).is_err());
        assert!(
            SyncResponse::decode_json(br#"{"up_to_date":true,"view_revision":1,"snapshot":null}"#)
                .is_ok()
        );
        assert!(
            SyncResponse::decode_json(br#"{"up_to_date":false,"view_revision":1,"snapshot":null}"#)
                .is_err()
        );
        assert!(
            SyncResponse::decode_json(
                format!(r#"{{"up_to_date":false,"view_revision":1,"snapshot":{view}}}"#).as_bytes()
            )
            .is_ok()
        );
        assert!(
            SyncResponse::decode_json(
                format!(r#"{{"up_to_date":true,"view_revision":1,"snapshot":{view}}}"#).as_bytes()
            )
            .is_err()
        );
    }
    #[test]
    fn projections_show_no_boards_before_start_and_only_own_player_board_after_start() {
        let config = serde_json::to_string(&GameConfiguration::default()).unwrap();
        let bytes = format!(
            r#"{{"role":"player","game_id":"{ID}","state":"awaiting_players","configuration":{config},"game_code":"AB12CD34","started_at":null,"ended_at":null,"view_revision":1,"player_id":"{ID}","alias":"Alice"}}"#
        );
        let view = GameView::decode_json(bytes.as_bytes());
        assert!(view.is_ok());
        let encoded = serde_json::to_string(&view.unwrap()).unwrap();
        assert!(!encoded.contains("\"board\":"));
        for extra in [
            r#", "players":[]"#,
            r#", "session_expires_at":1"#,
            r#", "recovery_answer":"x"#,
        ] {
            let bad = bytes.trim_end_matches('}').to_owned() + extra + "}";
            assert!(GameView::decode_json(bad.as_bytes()).is_err());
        }
        let board = r#"{"side_length":2,"cells":[{"position":{"row":1,"column":1},"kind":{"kind":"free"},"is_matched":true},{"position":{"row":1,"column":2},"kind":{"kind":"value","value":"1"},"is_matched":false},{"position":{"row":2,"column":1},"kind":{"kind":"value","value":"2"},"is_matched":false},{"position":{"row":2,"column":2},"kind":{"kind":"value","value":"3"},"is_matched":false}],"qualified":false,"qualifying_lines":[]}"#;
        let new_config = serde_json::to_string(&GameConfiguration {
            board_side_length: 2,
            free_cell_positions: vec![CellPosition { row: 1, column: 1 }],
            ..GameConfiguration::default()
        })
        .unwrap();
        let started = bytes
            .replace("awaiting_players", "in_progress")
            .replace(&config, &new_config)
            .replace("\"started_at\":null", "\"started_at\":2");
        let with_board = started.trim_end_matches('}').to_owned() + ",\"board\":" + board + "}";
        assert!(GameView::decode_json(with_board.as_bytes()).is_ok());
        assert!(
            GameView::decode_json(
                (bytes.trim_end_matches('}').to_owned() + ",\"board\":" + board + "}").as_bytes()
            )
            .is_err()
        );
        assert!(
            GameView::decode_json(
                with_board
                    .replace(
                        "\"position\":{\"row\":1,\"column\":1}",
                        "\"position\":[1,1]"
                    )
                    .as_bytes()
            )
            .is_err()
        );
    }
    #[test]
    fn response_configuration_rejects_positional_nested_cells_and_invalid_effective_config() {
        let raw = receipt();
        for config in [
            r#"[75,5,true,[{"row":3,"column":3}],20,50,"single_line"]"#,
            r#"{"numeric_upper_bound":75,"board_side_length":5,"free_cells_enabled":true,"free_cell_positions":[[3,3]],"player_capacity":20,"spectator_capacity":50,"winning_pattern":"single_line"}"#,
            r#"{"numeric_upper_bound":1,"board_side_length":5,"free_cells_enabled":true,"free_cell_positions":[{"row":3,"column":3}],"player_capacity":20,"spectator_capacity":50,"winning_pattern":"single_line"}"#,
        ] {
            let bytes = format!(
                r#"{{"result":"lobby_opened","game_id":"{ID}","state":"awaiting_players","game_code":"AB12CD34","configuration":{config},"view_revision":1,"receipt":{raw}}}"#
            );
            assert!(GameResponse::decode_json(bytes.as_bytes()).is_err());
            assert!(serde_json::from_slice::<GameResponse>(bytes.as_bytes()).is_err());
        }
    }
    #[test]
    fn responses_and_receipts_are_closed_safe_tagged_original_map_shapes() {
        let raw = receipt();
        assert!(GameReceipt::decode_json(raw.as_bytes()).is_ok());
        let configuration = serde_json::to_string(&GameConfiguration::default()).unwrap();
        let summary = format!(
            r#"{{"game_id":"{ID}","state":"new","designated_host_id":"{ID}","created_at":1,"view_revision":0}}"#
        );
        let lobby_receipt =
            operation_receipt(r#"{"operation":"lobby_opened","view_revision":1}"#, 2);
        let start_receipt = operation_receipt(
            r#"{"operation":"started","view_revision":2,"started_at":1}"#,
            2,
        );
        let joined = valid_join_response();
        let responses = [
            format!(
                r#"{{"result":"created","game":{summary},"configuration":{configuration},"idle_cancel_due_at":86400001,"receipt":{raw}}}"#
            ),
            format!(
                r#"{{"result":"lobby_opened","game_id":"{ID}","state":"awaiting_players","game_code":"AB12CD34","configuration":{configuration},"view_revision":1,"receipt":{lobby_receipt}}}"#
            ),
            format!(
                r#"{{"result":"started","game_id":"{ID}","state":"in_progress","started_at":1,"view_revision":2,"receipt":{start_receipt}}}"#
            ),
            format!(r#"{{"result":"committed","receipt":{raw}}}"#),
            format!(r#"{{"result":"pending","operation_id":"{ID}"}}"#),
            r#"{"result":"admission_context","expires_at":1}"#.into(),
            joined,
        ];
        for bytes in responses {
            let response = GameResponse::decode_json(bytes.as_bytes()).unwrap();
            assert_eq!(
                response.status(),
                if bytes.contains("pending") { 202 } else { 200 }
            );
            assert!(GameResponse::decode_json(&serde_json::to_vec(&response).unwrap()).is_ok());
        }
        for bytes in [
            raw.replace("\"version\":1", "\"version\":2"),
            raw.replace("\"view_revision\":0", "\"view_revision\":9007199254740992"),
            raw.replace(
                "\"completed_at\":1",
                "\"completed_at\":1,\"completed_at\":2",
            ),
            raw.replace("\"outcome\":{", "\"outcome\":{\"bearer\":\"x\","),
            format!("{} ", " ".repeat(GAME_RECEIPT_MAX_BYTES)),
        ] {
            assert!(GameReceipt::decode_json(bytes.as_bytes()).is_err());
        }
        for bytes in [
            r#"{"result":"admission_context","expires_at":null}"#,
            r#"{"result":"admission_context","expires_at":1,"\u0065xpires_at":2}"#,
            r#"{"result":"admission_context","expires_at":1,"bearer":"x"}"#,
            r#"{"result":"committed","receipt":[]}"#,
            r#"{"result":"created","game":[],"configuration":[],"idle_cancel_due_at":1,"receipt":{}}"#,
        ] {
            assert!(GameResponse::decode_json(bytes.as_bytes()).is_err());
        }
    }
    #[test]
    fn admission_and_join_validate_lookup_alias_and_never_debug_recovery_material() {
        let input = AdmissionContextInput::decode_json(br#"{"game_code":" ab12Cd34 "}"#);
        assert!(input.is_ok());
        assert_eq!(input.unwrap().game_code.as_str(), "AB12CD34");
        let marker = std::collections::hash_map::RandomState::new();
        use std::hash::BuildHasher as _;
        let marker = marker.hash_one("private-recovery-test").to_string();
        let bytes = format!(
            r#"{{"game_code":" ab12cd34 ","alias":" Alice  ! ","recovery_answer":"{marker}"}}"#
        );
        let joined = JoinPlayer::decode_json(bytes.as_bytes()).unwrap();
        assert_eq!(joined.alias, "Alice  !");
        assert_eq!(joined.recovery_answer.as_ref().unwrap().as_str(), marker);
        assert!(!format!("{joined:?}").contains(&marker));
        for bytes in [
            b"[]".as_slice(),
            br#"{"game_code":"AB12CD34","alias":""}"#,
            br#"{"game_code":"AB12CD34","alias":"Alice","recovery_answer":null}"#,
            br#"{"game_code":"AB12CD34","alias":"Alice","\u0061lias":"Bob"}"#,
            br#"{"game_code":"AB12CD34","alias":"Alice","player_id":"x"}"#,
        ] {
            assert!(JoinPlayer::decode_json(bytes).is_err());
        }
    }
    #[test]
    fn revision_command_requires_one_original_integer_with_javascript_safe_bound() {
        assert_eq!(
            RevisionCommand::decode_json(br#"{"expected_revision":0}"#)
                .map(|v| v.expected_revision),
            Ok(0)
        );
        assert_eq!(
            RevisionCommand::decode_json(br#"{"expected_revision":9007199254740991}"#)
                .map(|v| v.expected_revision),
            Ok(MAX_SAFE_REVISION)
        );
        for bytes in [
            b"[]".as_slice(),
            b"{}",
            br#"{"expected_revision":null}"#,
            br#"{"expected_revision":1.0}"#,
            br#"{"expected_revision":1e0}"#,
            br#"{"expected_revision":-1}"#,
            br#"{"expected_revision":9007199254740992}"#,
            br#"{"expected_revision":1,"\u0065xpected_revision":2}"#,
            br#"{"expected_revision":1,"roster":[]}"#,
        ] {
            assert!(RevisionCommand::decode_json(bytes).is_err());
        }
    }
    #[test]
    fn creation_resolves_defaults_and_overrides_with_effective_center() {
        let created = CreateGame::decode_json(b"{}");
        assert!(created.is_ok());
        assert_eq!(created.unwrap().configuration, GameConfiguration::default());
        let c = CreateGame::decode_json(
            br#"{"configuration":{"board_side_length":4,"spectator_capacity":0}}"#,
        )
        .unwrap()
        .configuration;
        assert_eq!(c.board_side_length, 4);
        assert_eq!(
            c.free_cell_positions,
            vec![brews_domain::games::CellPosition { row: 2, column: 2 }]
        );
        assert_eq!(c.spectator_capacity, 0);
        let c = CreateGame::decode_json(br#"{"configuration":{"free_cells_enabled":false}}"#)
            .unwrap()
            .configuration;
        assert!(c.free_cell_positions.is_empty());
        for bytes in [
            b"[]".as_slice(),
            b"null",
            br#"{"configuration":null}"#,
            br#"{"configuration":[]}"#,
            br#"{"configuration":{"board_side_length":null}}"#,
            br#"{"configuration":{"numeric_upper_bound":1}}"#,
            br#"{"configuration":{"free_cell_positions":[[1,1]]}}"#,
            br#"{"configuration":{"board_side_length":4,"\u0062oard_side_length":5}}"#,
            br#"{"configuration":{},"configuration":{}}"#,
            br#"{"configuration":{"seed":1}}"#,
            br#"{"host_id":"x"}"#,
        ] {
            assert!(CreateGame::decode_json(bytes).is_err());
        }
    }
}
