//! Exact durable assignment metadata. Neither an intent nor a Directory grant authenticates an account.
use brews_domain::ids::{AccountId, CommandId, GameId, OperationId};
#[derive(Clone, Eq, PartialEq)]
pub struct HostTransferWork {
    pub(crate) operation: OperationId,
    pub(crate) game: GameId,
    pub(crate) actor: AccountId,
    pub(crate) command: CommandId,
    pub(crate) fingerprint: [u8; 32],
    pub(crate) old_host: AccountId,
    pub(crate) target: AccountId,
    pub(crate) expected_assignment: i64,
    pub(crate) prepared_revision: i64,
    pub(crate) nonce: [u8; 32],
    pub(crate) phase: TransferPhase,
    pub(crate) next: i64,
    pub(crate) retry_attempts: u32,
}
impl std::fmt::Debug for HostTransferWork {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HostTransferWork([redacted])")
    }
}
impl HostTransferWork {
    pub fn operation_id(&self) -> OperationId {
        self.operation
    }
    pub fn game_id(&self) -> GameId {
        self.game
    }
    pub fn target_account_id(&self) -> AccountId {
        self.target
    }
    pub fn phase(&self) -> TransferPhase {
        self.phase
    }
    pub fn next_attempt_at(&self) -> i64 {
        self.next
    }
    pub(crate) fn same_binding(&self, other: &Self) -> bool {
        self.operation == other.operation
            && self.game == other.game
            && self.actor == other.actor
            && self.command == other.command
            && self.fingerprint == other.fingerprint
            && self.old_host == other.old_host
            && self.target == other.target
            && self.expected_assignment == other.expected_assignment
            && self.prepared_revision == other.prepared_revision
            && self.nonce == other.nonce
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransferPhase {
    Prepared,
    Committed,
    Aborted,
}
impl TransferPhase {
    pub(crate) const fn tag(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::Committed => "committed",
            Self::Aborted => "aborted",
        }
    }
    pub(crate) fn parse(s: &str) -> Result<Self, super::GameError> {
        match s {
            "prepared" => Ok(Self::Prepared),
            "committed" => Ok(Self::Committed),
            "aborted" => Ok(Self::Aborted),
            _ => Err(super::GameError::Storage),
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostTransferGrant {
    pub(crate) work: HostTransferWork,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostTransferProof {
    pub(crate) work: HostTransferWork,
    pub(crate) projection: crate::directory::games::GameProjection,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostTransferAck {
    pub(crate) work: HostTransferWork,
}

pub(crate) const HOST_TRANSFER_RETRY_MAX_SHIFT: u32 = 9;
pub(crate) fn host_transfer_retry_delay(attempts: u32) -> Option<i64> {
    if attempts > HOST_TRANSFER_RETRY_MAX_SHIFT {
        return None;
    }
    super::RETRY_INITIAL_MS
        .checked_mul(1_i64.checked_shl(attempts)?)
        .map(|delay| delay.min(super::RETRY_MAX_MS))
}
