//! Current original-actor authority precedes every receipt/age branch. No replay effects.
use super::reads::{ReadBinding, ReadCut, ReadIdentity};
use super::*;
use crate::api::{ReadRole, ReadSelection};
use crate::game::TransferPhase;
use brews_contracts::games::{
    HostTransferInput,
    commands::*,
    reads::{ReadKind, ReadProjection},
};

const SQL_SELECT_ACTOR_TRANSFER: &str = "SELECT operation_id,game_id,actor_account_id,command_id,fingerprint,old_host_id,target_account_id,expected_assignment_revision,prepared_game_revision,nonce,phase,next_attempt_at,retry_attempts FROM game_host_transfers WHERE actor_account_id=? AND command_id=? LIMIT 2";

impl<D: Database, R: Runtime> GameService<'_, D, R> {
    #[cfg_attr(
        not(target_arch = "wasm32"),
        allow(dead_code, reason = "Private creation cuts are exercised by Workerd.")
    )]
    pub(crate) fn command_game_missing(&self) -> Result<bool, GameError> {
        Ok(self.record_optional()?.is_none() && self.terminal_routing_projection()?.is_none())
    }
    #[cfg_attr(
        not(target_arch = "wasm32"),
        allow(dead_code, reason = "Private creation cuts are exercised by Workerd.")
    )]
    pub(crate) fn capture_creation_command(
        &self,
        selection: ReadSelection,
        authority: &GameAccountAuthority,
        created: i64,
        audit: bool,
    ) -> Result<ReadCut, GameError> {
        let ReadKind::CommandLookup(command) = selection.kind else {
            return Err(GameError::InvalidInput);
        };
        let action = |now| {
            authorize(authority, now)?;
            if selection.role != ReadRole::Account
                || selection.player_id.is_some()
                || !self.command_game_missing()?
                || created < command_stamp(command)?
                || created - command_stamp(command)? >= RECEIPT_MS
                || created > now
            {
                return Err(GameError::Storage);
            }
            let projection = ReadProjection::CommandLookup(CommandResult {
                version: 1,
                game_id: selection.game_id,
                command_id: command,
                result_status: CommandResultStatus::Pending {
                    operation: PendingCommandOperation::Creation,
                    state: PendingCommandState::Prepared,
                },
            });
            Ok(ReadCut {
                projection,
                binding: ReadBinding {
                    identity: ReadIdentity::Account {
                        account_id: authority.account_id(),
                        session_id: authority.session_id(),
                        account_role: authority.role(),
                        epoch: authority.credential_epoch(),
                        expires_at: authority.expires_at(),
                    },
                    valid_until: authority.expires_at(),
                    host_assignment_revision: 0,
                },
                checked_at: now,
            })
        };
        if audit {
            self.operator_attempt(authority, OperatorPurpose::CommandLookup, |now| {
                action(now).map(|c| (c, true))
            })
        } else {
            self.maintenance(action)
        }
    }
    pub(super) fn command_lookup_at(
        &self,
        selection: ReadSelection,
        command: CommandId,
        token: &str,
        account: Option<&GameAccountAuthority>,
        now: i64,
    ) -> Result<ReadCut, GameError> {
        if selection.player_id.is_some() {
            return Err(GameError::InvalidInput);
        }
        // Deleted objects cannot expose retained receipts. Account authority is still verified.
        if selection.role == ReadRole::Account {
            authorize(account.ok_or(GameError::Unauthorized)?, now)?;
        }
        let record = self
            .record_optional()?
            .ok_or(if selection.role == ReadRole::Account {
                GameError::NotFound
            } else {
                GameError::Unauthorized
            })?;
        if record.id != selection.game_id {
            return Err(GameError::NotFound);
        }
        ensure_before_idle(&record, now)?;
        let terminal = matches!(record.state, GameState::Resolved | GameState::Cancelled);
        let (actor, identity, expires) = match selection.role {
            ReadRole::Account => {
                let a = account.ok_or(GameError::Unauthorized)?;
                if terminal {
                    self.require_final_history(&record, now)?;
                    let rows = self.db.query(
                        SQL_SELECT_GAME_CONNECTION_GRANTS_PRINCIPAL_ID_EPOCH_EXPIRES_AT,
                        &[text(a.session_id()), text(ACCOUNT_KIND)],
                    )?;
                    let row = rows.first().ok_or(GameError::Unauthorized)?;
                    if string(row, 0)? != a.account_id().to_string()
                        || number(row, 1)? != a.credential_epoch()
                        || number(row, 2)? != a.expires_at()
                    {
                        return Err(GameError::Unauthorized);
                    }
                }
                (
                    account_actor(a.account_id()),
                    ReadIdentity::Account {
                        account_id: a.account_id(),
                        session_id: a.session_id(),
                        account_role: a.role(),
                        epoch: a.credential_epoch(),
                        expires_at: a.expires_at(),
                    },
                    a.expires_at(),
                )
            }
            ReadRole::Player => {
                let s = self.player_session(token, &record, now)?;
                (
                    player_actor(s.player),
                    ReadIdentity::Player {
                        player_id: s.player,
                        session_id: s.session,
                        epoch: s.epoch,
                        expires_at: s.expires,
                    },
                    s.expires,
                )
            }
            ReadRole::Spectator => {
                let s = self.spectator_session(token, &record, now)?;
                (
                    format!("spectator:{}", s.spectator),
                    ReadIdentity::Spectator {
                        spectator_id: s.spectator,
                        session_id: s.session,
                        epoch: s.epoch,
                        expires_at: s.expires,
                    },
                    s.expires.min(s.grace.unwrap_or(s.expires)),
                )
            }
        };
        let mut cap = expires.min(if terminal {
            record.history_expires.ok_or(GameError::Unauthorized)?
        } else {
            record.due.unwrap_or(expires)
        });
        if now >= cap {
            return Err(GameError::Unauthorized);
        }
        let (status, receipt_expiry) =
            self.lookup_command_facts(&record, &actor, &identity, command, now)?;
        if let Some(expiry) = receipt_expiry {
            cap = cap.min(expiry);
        }
        let projection = ReadProjection::CommandLookup(CommandResult {
            version: 1,
            game_id: record.id,
            command_id: command,
            result_status: status,
        });
        projection.encode_json().map_err(|_| GameError::Storage)?;
        Ok(ReadCut {
            projection,
            binding: ReadBinding {
                identity,
                valid_until: cap,
                host_assignment_revision: self.assignment_revision()?,
            },
            checked_at: now,
        })
    }
    fn lookup_command_facts(
        &self,
        record: &Record,
        actor: &str,
        identity: &ReadIdentity,
        command: CommandId,
        now: i64,
    ) -> Result<(CommandResultStatus, Option<i64>), GameError> {
        let game = record.id;
        let stamp = command_stamp(command)?;
        let rows = self.db.query(
            SQL_SELECT_REQUIRED_GAME_RECEIPT,
            &[text(actor), text(command)],
        )?;
        if rows.len() > 1 {
            return Err(GameError::Storage);
        }
        let receipt = rows
            .first()
            .map(|row| {
                if row.len() != 6
                    || string(row, 0)? != actor
                    || string(row, 1)? != command.to_string()
                    || blob(row, 2)?.len() != 32
                {
                    return Err(GameError::Storage);
                }
                let receipt = GameReceipt::decode_json(string(row, 3)?.as_bytes())
                    .map_err(|_| GameError::Storage)?;
                if receipt.game_id != game
                    || receipt.command_id != command
                    || receipt.completed_at != number(row, 4)?
                    || receipt.expires_at != number(row, 5)?
                    || receipt.completed_at < stamp
                    || receipt.completed_at - stamp >= RECEIPT_MS
                    || receipt.completed_at > now
                {
                    return Err(GameError::Storage);
                }
                validate_outcome_actor(&receipt.outcome, identity)?;
                Ok((receipt, blob(row, 2)?.to_vec()))
            })
            .transpose()?;
        let work_rows = self.db.query(
            SQL_SELECT_PENDING_WORK_BY_ACTOR_AND_COMMAND,
            &[text(actor), text(command)],
        )?;
        if work_rows.len() > 1 {
            return Err(GameError::Storage);
        }
        let mut pending = None;
        let mut fingerprint = None;
        let mut transfer_aborted = false;
        let mut transfer = None;
        if let Some(row) = work_rows.first() {
            let w = parse_work(row)?;
            if row.len() != 11
                || w.actor != actor
                || w.command != command
                || w.created < stamp
                || w.created - stamp >= RECEIPT_MS
                || w.created > now
                || w.next < w.created
                || [w.expected, w.fence, w.created, w.next]
                    .iter()
                    .any(|v| !(0..=JS_SAFE_INTEGER_MAX).contains(v))
            {
                return Err(GameError::Storage);
            }
            let operation = match w.kind {
                WorkKind::Lobby => PendingCommandOperation::LobbyPublication,
                WorkKind::StartProjection => PendingCommandOperation::StartPublication,
                WorkKind::Release => PendingCommandOperation::TerminalRelease,
            };
            let state = match w.phase {
                WorkPhase::Prepared => PendingCommandState::Prepared,
                WorkPhase::AwaitingAcknowledgement => PendingCommandState::AwaitingAcknowledgement,
            };
            pending = Some((operation, state));
            fingerprint = Some(w.fingerprint.to_vec());
        }
        if let ReadIdentity::Account { account_id, .. } = identity {
            let transfers = self.db.query(
                SQL_SELECT_ACTOR_TRANSFER,
                &[text(account_id), text(command)],
            )?;
            if transfers.len() > 1 || !transfers.is_empty() && pending.is_some() {
                return Err(GameError::Storage);
            }
            if let Some(row) = transfers.first() {
                let w = operator::parse_transfer(row)?;
                let prepared_stamp = command_stamp(
                    w.operation
                        .to_string()
                        .parse()
                        .map_err(|_| GameError::Storage)?,
                )?;
                if w.game != game
                    || w.actor != *account_id
                    || w.command != command
                    || prepared_stamp < stamp
                    || prepared_stamp - stamp >= RECEIPT_MS
                    || prepared_stamp > now
                    || w.next < prepared_stamp
                    || operator::transfer_fingerprint(&HostTransferInput {
                        target_account_id: w.target,
                        expected_host_assignment_revision: u64::try_from(w.expected_assignment)
                            .map_err(|_| GameError::Storage)?,
                        confirmed: true,
                    })
                    .map_err(|_| GameError::Storage)?
                        != w.fingerprint
                {
                    return Err(GameError::Storage);
                }
                transfer_aborted = w.phase == TransferPhase::Aborted;
                pending = Some((
                    PendingCommandOperation::HostTransfer,
                    if w.phase == TransferPhase::Prepared {
                        PendingCommandState::Prepared
                    } else {
                        PendingCommandState::AwaitingAcknowledgement
                    },
                ));
                fingerprint = Some(w.fingerprint.to_vec());
                transfer = Some(w);
            }
        }
        if let Some((r, fp)) = receipt {
            if transfer_aborted
                || fingerprint.as_ref().is_some_and(|p| p != &fp)
                || pending.is_some_and(|(_, s)| s == PendingCommandState::Prepared)
            {
                return Err(GameError::Storage);
            }
            if let Some(work) = transfer {
                let GameOutcome::HostTransferred {
                    designated_host_id,
                    host_assignment_revision,
                    ..
                } = &r.outcome
                else {
                    return Err(GameError::Storage);
                };
                let assignment = work
                    .expected_assignment
                    .checked_add(1)
                    .ok_or(GameError::Storage)?;
                if work.phase != TransferPhase::Committed
                    || work.target != *designated_host_id
                    || u64::try_from(assignment).map_err(|_| GameError::Storage)?
                        != *host_assignment_revision
                {
                    return Err(GameError::Storage);
                }
            }
            if now >= r.expires_at {
                return Ok((CommandResultStatus::Expired {}, None));
            }
            if pending.is_some_and(|(op, _)| {
                !matches!(
                    (op, &r.outcome),
                    (
                        PendingCommandOperation::LobbyPublication,
                        GameOutcome::LobbyOpened { .. }
                    ) | (
                        PendingCommandOperation::StartPublication,
                        GameOutcome::Started { .. }
                    ) | (
                        PendingCommandOperation::TerminalRelease,
                        GameOutcome::Terminalized { .. }
                    ) | (
                        PendingCommandOperation::HostTransfer,
                        GameOutcome::HostTransferred { .. }
                    )
                )
            }) {
                return Err(GameError::Storage);
            }
            let terminal_release = if let GameOutcome::Terminalized {
                state, ended_at, ..
            } = &r.outcome
            {
                if record.state != *state
                    || record.ended != Some(*ended_at)
                    || !matches!(identity,ReadIdentity::Account {account_id,..} if record.terminal_actor_id==Some(*account_id))
                {
                    return Err(GameError::Storage);
                }
                match self.command_work(SYSTEM_ACTOR, record.creation_command)? {
                    None => false,
                    Some(work) => {
                        if work.operation.to_string() != record.creation_command.to_string()
                            || work.kind != WorkKind::Release
                            || work.phase != WorkPhase::AwaitingAcknowledgement
                            || work.actor != SYSTEM_ACTOR
                            || work.command != record.creation_command
                            || work.fingerprint != *record.fp.digest()
                            || work.created != *ended_at
                            || work.fence != record.revision
                            || work.expected != 0
                            || work.next < work.created
                            || work.next > JS_SAFE_INTEGER_MAX
                        {
                            return Err(GameError::Storage);
                        }
                        true
                    }
                }
            } else {
                false
            };
            let expiry = r.expires_at;
            return Ok((
                CommandResultStatus::Committed {
                    receipt: r,
                    follow_up_pending: pending.is_some() || terminal_release,
                },
                Some(expiry),
            ));
        }
        if let Some((operation, state)) = pending {
            return Ok((CommandResultStatus::Pending { operation, state }, None));
        }
        if stamp > now {
            return Ok((CommandResultStatus::Inadmissible {}, None));
        }
        let rows = self
            .db
            .query(SQL_SELECT_GAME_METADATA_COMMAND_FLOOR_MS, &[])?;
        if stamp <= number(rows.first().ok_or(GameError::Storage)?, 0)? {
            return Ok((CommandResultStatus::Expired {}, None));
        }
        Ok((CommandResultStatus::Unknown {}, None))
    }
}
fn command_stamp(command: CommandId) -> Result<i64, GameError> {
    let id = uuid::Uuid::parse_str(&command.to_string()).map_err(|_| GameError::InvalidInput)?;
    let b = id.as_bytes();
    Ok(i64::from_be_bytes([
        0, 0, b[0], b[1], b[2], b[3], b[4], b[5],
    ]))
}
fn validate_outcome_actor(outcome: &GameOutcome, identity: &ReadIdentity) -> Result<(), GameError> {
    let valid = match outcome {
        GameOutcome::PlayerJoined { player_id, .. }
        | GameOutcome::PlayerRecovered { player_id, .. }
        | GameOutcome::AliasRenamed { player_id, .. }
        | GameOutcome::PlayerLeft { player_id, .. } => {
            matches!(identity,ReadIdentity::Player {player_id:p,..} if p==player_id)
        }
        GameOutcome::RecoveryAnswerChanged { .. } => {
            matches!(identity, ReadIdentity::Player { .. })
        }
        GameOutcome::ParticipantChanged {
            change,
            identity: target,
            ..
        } => match (change, target, identity) {
            (
                brews_contracts::games::ParticipantChange::Switched,
                brews_contracts::games::ParticipantIdentity::Player { .. },
                ReadIdentity::Spectator { .. },
            )
            | (
                brews_contracts::games::ParticipantChange::Switched,
                brews_contracts::games::ParticipantIdentity::Spectator { .. },
                ReadIdentity::Player { .. },
            ) => true,
            (
                _,
                brews_contracts::games::ParticipantIdentity::Player { player_id, .. },
                ReadIdentity::Player { player_id: p, .. },
            ) => p == player_id,
            (
                _,
                brews_contracts::games::ParticipantIdentity::Spectator { spectator_id },
                ReadIdentity::Spectator {
                    spectator_id: s, ..
                },
            ) => s == spectator_id,
            _ => false,
        },
        GameOutcome::Exited {} => matches!(
            identity,
            ReadIdentity::Player { .. } | ReadIdentity::Account { .. }
        ),
        _ => matches!(identity, ReadIdentity::Account { .. }),
    };
    if valid {
        Ok(())
    } else {
        Err(GameError::Storage)
    }
}
