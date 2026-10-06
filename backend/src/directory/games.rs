//! Typed nonsecret private-peer coordination. No browser actor grants.
/// A validated Game-owner revision, not caller time or a global coordination counter.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SourceRevision(i64);
impl SourceRevision {
    pub const INITIAL: Self = Self(0);
    pub const fn value(self) -> i64 {
        self.0
    }
}
impl TryFrom<i64> for SourceRevision {
    type Error = super::DirectoryError;
    fn try_from(value: i64) -> Result<Self, Self::Error> {
        if value < 0 {
            Err(super::DirectoryError::ProofMismatch)
        } else {
            Ok(Self(value))
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CreationReadyProof {
    game_id: GameId,
    account_id: AccountId,
    command_id: CommandId,
    fingerprint: CreationFingerprint,
    created_at: i64,
    source_revision: SourceRevision,
}
impl CreationReadyProof {
    pub(crate) const fn new(
        game_id: GameId,
        account_id: AccountId,
        command_id: CommandId,
        fingerprint: CreationFingerprint,
        created_at: i64,
        source_revision: SourceRevision,
    ) -> Self {
        Self {
            game_id,
            account_id,
            command_id,
            fingerprint,
            created_at,
            source_revision,
        }
    }
    pub const fn game_id(&self) -> GameId {
        self.game_id
    }
    pub const fn account_id(&self) -> AccountId {
        self.account_id
    }
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
    pub const fn fingerprint(&self) -> CreationFingerprint {
        self.fingerprint
    }
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }
    pub const fn source_revision(&self) -> i64 {
        self.source_revision.value()
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CreationAck(CreationReadyProof);
impl CreationAck {
    pub(crate) const fn new(proof: CreationReadyProof) -> Self {
        Self(proof)
    }
    pub const fn proof(&self) -> CreationReadyProof {
        self.0
    }
    pub const fn game_id(&self) -> GameId {
        self.0.game_id()
    }
}

use brews_domain::{
    games::{GameCode, GameState},
    ids::{AccountId, CommandId, GameId},
};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReservationProof {
    game_id: GameId,
}
impl ReservationProof {
    pub(crate) const fn new(game_id: GameId) -> Self {
        Self { game_id }
    }
    pub const fn game_id(&self) -> GameId {
        self.game_id
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalProof(GameProjection);
impl TerminalProof {
    pub(crate) fn new(projection: GameProjection) -> Self {
        Self(projection)
    }
    pub fn projection(&self) -> &GameProjection {
        &self.0
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReleaseAck {
    game_id: GameId,
}
impl ReleaseAck {
    pub(crate) const fn new(game_id: GameId) -> Self {
        Self { game_id }
    }
    pub const fn game_id(&self) -> GameId {
        self.game_id
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodeGrant {
    creation: CreationAck,
    game_code: GameCode,
}
impl CodeGrant {
    pub(crate) fn new(creation: CreationAck, game_code: GameCode) -> Self {
        Self {
            creation,
            game_code,
        }
    }
    pub const fn game_id(&self) -> GameId {
        self.creation.game_id()
    }
    pub fn game_code(&self) -> &GameCode {
        &self.game_code
    }
    pub const fn creation(&self) -> CreationAck {
        self.creation
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GameProjection {
    game_id: GameId,
    designated_host_id: AccountId,
    fingerprint: CreationFingerprint,
    state: GameState,
    source_revision: SourceRevision,
    game_code: Option<GameCode>,
    created_at: i64,
    started_at: Option<i64>,
    ended_at: Option<i64>,
    history_expires_at: Option<i64>,
}
impl GameProjection {
    #[allow(
        clippy::too_many_arguments,
        reason = "Private validated owner projection row."
    )]
    pub(crate) fn new(
        game_id: GameId,
        designated_host_id: AccountId,
        fingerprint: CreationFingerprint,
        state: GameState,
        source_revision: SourceRevision,
        game_code: Option<GameCode>,
        created_at: i64,
        started_at: Option<i64>,
        ended_at: Option<i64>,
        history_expires_at: Option<i64>,
    ) -> Self {
        Self {
            game_id,
            designated_host_id,
            fingerprint,
            state,
            source_revision,
            game_code,
            created_at,
            started_at,
            ended_at,
            history_expires_at,
        }
    }
    pub const fn game_id(&self) -> GameId {
        self.game_id
    }
    pub const fn designated_host_id(&self) -> AccountId {
        self.designated_host_id
    }
    pub const fn fingerprint(&self) -> CreationFingerprint {
        self.fingerprint
    }
    pub const fn state(&self) -> GameState {
        self.state
    }
    pub const fn source_revision(&self) -> i64 {
        self.source_revision.value()
    }
    pub fn game_code(&self) -> Option<&GameCode> {
        self.game_code.as_ref()
    }
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }
    pub const fn started_at(&self) -> Option<i64> {
        self.started_at
    }
    pub const fn ended_at(&self) -> Option<i64> {
        self.ended_at
    }
    pub const fn history_expires_at(&self) -> Option<i64> {
        self.history_expires_at
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProjectionAck {
    game_id: GameId,
    source_revision: SourceRevision,
    published: bool,
}
impl ProjectionAck {
    pub(crate) fn new(game_id: GameId, source_revision: i64, published: bool) -> Self {
        Self {
            game_id,
            source_revision: SourceRevision(source_revision),
            published,
        }
    }
    pub const fn game_id(&self) -> GameId {
        self.game_id
    }
    pub const fn source_revision(&self) -> i64 {
        self.source_revision.value()
    }
    pub const fn published(&self) -> bool {
        self.published
    }
}

pub const CREATION_FINGERPRINT_BYTES: usize = 32;
pub const CREATION_LIFETIME_MS: i64 = 24 * 60 * 60 * 1_000;
pub const CREATION_RETRY_INITIAL_MS: i64 = 1_000;
pub const CREATION_RETRY_MAX_MS: i64 = 5 * 60 * 1_000;
pub const GAME_CODE_MAX_CANDIDATES: usize = 32;
pub const GAME_CODE_ENTROPY_BATCHES: usize = 32;
pub const PUBLICATION_PENDING: &str = "pending";
pub const PUBLICATION_PUBLISHED: &str = "published";

/// SHA-256 of the versioned canonical effective configuration, computed by Game.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CreationFingerprint([u8; CREATION_FINGERPRINT_BYTES]);
impl CreationFingerprint {
    pub const fn from_digest(digest: [u8; CREATION_FINGERPRINT_BYTES]) -> Self {
        Self(digest)
    }
    pub const fn digest(&self) -> &[u8; CREATION_FINGERPRINT_BYTES] {
        &self.0
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CreationWork {
    account_id: AccountId,
    command_id: CommandId,
    game_id: GameId,
    fingerprint: CreationFingerprint,
    created_at: i64,
    deadline: i64,
    next_retry_at: Option<i64>,
    attempts: u32,
    ready_revision: Option<i64>,
}
impl CreationWork {
    #[allow(
        clippy::too_many_arguments,
        reason = "Validated owner-local scalar row mapping."
    )]
    pub(crate) const fn new(
        account_id: AccountId,
        command_id: CommandId,
        game_id: GameId,
        fingerprint: CreationFingerprint,
        created_at: i64,
        deadline: i64,
        next_retry_at: Option<i64>,
        attempts: u32,
        ready_revision: Option<i64>,
    ) -> Self {
        Self {
            account_id,
            command_id,
            game_id,
            fingerprint,
            created_at,
            deadline,
            next_retry_at,
            attempts,
            ready_revision,
        }
    }
    pub const fn account_id(&self) -> AccountId {
        self.account_id
    }
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }
    pub const fn game_id(&self) -> GameId {
        self.game_id
    }
    pub const fn fingerprint(&self) -> CreationFingerprint {
        self.fingerprint
    }
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }
    pub const fn deadline(&self) -> i64 {
        self.deadline
    }
    pub const fn next_retry_at(&self) -> Option<i64> {
        self.next_retry_at
    }
    pub const fn attempts(&self) -> u32 {
        self.attempts
    }
    pub const fn ready_revision(&self) -> Option<i64> {
        self.ready_revision
    }
}
