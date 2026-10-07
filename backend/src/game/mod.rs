//! Game owner rules and private coordination types; SQL lives in db::game.
pub(crate) mod recovery_answers;
use crate::directory::games::{CreationFingerprint, CreationReadyProof};
use brews_contracts::games::GameResponse;
use brews_domain::games::GameConfiguration;
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum GameError {
    #[error("game storage unavailable")]
    Storage,
    #[error("game authority required")]
    Unauthorized,
    #[error("game permission denied")]
    Forbidden,
    #[error("invalid game command")]
    InvalidInput,
    #[error("game command conflict")]
    Conflict,
    #[error("stale game revision")]
    StaleRevision,
    #[error("stale game command")]
    StaleCommand,
    #[error("game not found")]
    NotFound,
    #[error("game expired")]
    Expired,
    #[error("game capacity reached")]
    Capacity,
    #[error("game entropy unavailable")]
    RandomUnavailable,
    #[error("game layout infeasible")]
    Infeasible,
    #[error("game generation budget exhausted")]
    GenerationExhausted,
}
impl From<crate::db::StorageError> for GameError {
    fn from(_: crate::db::StorageError) -> Self {
        Self::Storage
    }
}
pub struct CreationResult {
    pub ready: CreationReadyProof,
    pub response: GameResponse,
}
/// Versioned binary semantic encoding, shared by Directory claim and initializer.
/// Free positions are a set, encoded in sorted row/column order.
pub fn creation_fingerprint(
    configuration: &GameConfiguration,
) -> Result<CreationFingerprint, GameError> {
    use sha2::{Digest, Sha256};
    configuration
        .validate()
        .map_err(|_| GameError::InvalidInput)?;
    let mut hash = Sha256::new();
    hash.update(b"brews-game-configuration-v1\0");
    hash.update(configuration.numeric_upper_bound.to_be_bytes());
    hash.update([
        configuration.board_side_length,
        u8::from(configuration.free_cells_enabled),
        configuration.player_capacity,
        configuration.spectator_capacity,
    ]);
    hash.update(configuration.winning_pattern.to_string().as_bytes());
    let mut positions = configuration.free_cell_positions.clone();
    positions.sort_by_key(|p| (p.row, p.column));
    hash.update((positions.len() as u16).to_be_bytes());
    for p in positions {
        hash.update([p.row, p.column]);
    }
    Ok(CreationFingerprint::from_digest(hash.finalize().into()))
}
use brews_contracts::games::GameReceipt;
use brews_domain::ids::{AccountId, CommandId, OperationId};
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingWork {
    pub(crate) operation: OperationId,
    pub(crate) kind: WorkKind,
    pub(crate) phase: WorkPhase,
    pub(crate) actor: String,
    pub(crate) command: CommandId,
    pub(crate) fingerprint: [u8; 32],
    pub(crate) expected: i64,
    pub(crate) fence: i64,
    pub(crate) created: i64,
    pub(crate) next: i64,
    pub(crate) attempts: u32,
}
impl PendingWork {
    pub fn operation_id(&self) -> OperationId {
        self.operation
    }
    pub fn kind(&self) -> WorkKind {
        self.kind
    }
    pub fn phase(&self) -> WorkPhase {
        self.phase
    }
    pub fn next_attempt_at(&self) -> i64 {
        self.next
    }
    pub fn attempt_count(&self) -> u32 {
        self.attempts
    }
    pub fn command_id(&self) -> CommandId {
        self.command
    }
    pub fn account_id(&self) -> Option<AccountId> {
        self.actor.strip_prefix("account:")?.parse().ok()
    }
    pub fn fence_revision(&self) -> i64 {
        self.fence
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkKind {
    Lobby,
    StartProjection,
    Release,
}
impl WorkKind {
    pub const fn tag(self) -> &'static str {
        match self {
            Self::Lobby => "lobby",
            Self::StartProjection => "start_projection",
            Self::Release => "release",
        }
    }
}
impl std::str::FromStr for WorkKind {
    type Err = GameError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "lobby" => Ok(Self::Lobby),
            "start_projection" => Ok(Self::StartProjection),
            "release" => Ok(Self::Release),
            _ => Err(GameError::Storage),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkPhase {
    Prepared,
    AwaitingAcknowledgement,
}
impl WorkPhase {
    pub const fn tag(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::AwaitingAcknowledgement => "awaiting_acknowledgement",
        }
    }
}
impl std::str::FromStr for WorkPhase {
    type Err = GameError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "prepared" => Ok(Self::Prepared),
            "awaiting_acknowledgement" => Ok(Self::AwaitingAcknowledgement),
            _ => Err(GameError::Storage),
        }
    }
}
pub enum LobbyPreparation {
    Ready(PendingWork),
    Committed(GameReceipt),
}

/// Credential handoff is private and zeroized. No Debug or Serialize implementation.
/// ```compile_fail
/// fn print(cookie: brews_backend::game::SecretCookie) { println!("{cookie:?}"); }
/// ```
pub struct SecretCookie {
    pub(crate) token: zeroize::Zeroizing<String>,
    pub(crate) expires: i64,
}
impl SecretCookie {
    pub fn token(&self) -> &str {
        &self.token
    }
    pub fn expires_at(&self) -> i64 {
        self.expires
    }
}
pub struct ParticipantOutcome {
    pub response: GameResponse,
    pub cookie: Option<SecretCookie>,
}

use brews_domain::ids::{ConnectionId, GameId, PlayerId, SessionId};
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectionGrant {
    pub(crate) game: GameId,
    pub(crate) session: SessionId,
    pub(crate) player: PlayerId,
    pub(crate) connection: ConnectionId,
    pub(crate) epoch: i64,
    pub(crate) expires: i64,
}
impl ConnectionGrant {
    pub fn game_id(&self) -> GameId {
        self.game
    }
    pub fn session_id(&self) -> SessionId {
        self.session
    }
    pub fn player_id(&self) -> PlayerId {
        self.player
    }
    pub fn connection_id(&self) -> ConnectionId {
        self.connection
    }
    pub fn epoch(&self) -> i64 {
        self.epoch
    }
    pub fn expires_at(&self) -> i64 {
        self.expires
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccountConnectionGrant {
    pub(crate) game: GameId,
    pub(crate) session: SessionId,
    pub(crate) account: AccountId,
    pub(crate) connection: ConnectionId,
    pub(crate) epoch: i64,
    pub(crate) expires: i64,
}
impl AccountConnectionGrant {
    pub fn game_id(&self) -> GameId {
        self.game
    }
    pub fn session_id(&self) -> SessionId {
        self.session
    }
    pub fn account_id(&self) -> AccountId {
        self.account
    }
    pub fn connection_id(&self) -> ConnectionId {
        self.connection
    }
    pub fn epoch(&self) -> i64 {
        self.epoch
    }
    pub fn expires_at(&self) -> i64 {
        self.expires
    }
}

pub const HOST_VIEW: &str = "host";
pub const HOST_IDLE_MS: i64 = 86_400_000;
pub const RECEIPT_MS: i64 = 86_400_000;
pub const PLAYER_SESSION_MS: i64 = 86_400_000;
pub const ADMISSION_MS: i64 = 900_000;
pub const ADMISSION_LIMIT: i64 = 100;
pub const RETRY_INITIAL_MS: i64 = 1_000;
pub const RETRY_MAX_MS: i64 = 300_000;
pub const PLAYER_KIND: &str = "player";
pub const ACCOUNT_KIND: &str = "account";
pub const LIVE_ACCESS: &str = "live";
pub const HOST_IDLE_REASON: &str = "host_idle_timeout";
pub const OPERATOR_CANCEL_REASON: &str = "operator_cancelled";
pub const TERMINAL_ACTOR_ACCOUNT: &str = "account";
pub const TERMINAL_ACTOR_SYSTEM: &str = "system";
