//! Atomic saved setup/resume/assignment commands and fail-closed privileged capture.
use super::*;
use crate::game::{
    HostTransferAck, HostTransferGrant, HostTransferProof, HostTransferWork, TransferPhase,
};
use brews_contracts::games::{ConfigurationPatch, HostTransferInput};

const GAME_AUDIT_RETENTION_MS: i64 = crate::game::GAME_ADMIN_AUDIT_RETENTION_MS;
const SQL_UPDATE_OPERATOR_ACTIVITY: &str = "UPDATE game_record SET last_host_activity=?,idle_due=? WHERE singleton=1 AND host_id=? AND idle_due IS NOT NULL";
const SQL_INSERT_GAME_AUDIT: &str = "INSERT INTO game_admin_audit VALUES(?,?,?,?,?,?,?)";
const SQL_SELECT_TRANSFER: &str = "SELECT operation_id,game_id,actor_account_id,command_id,fingerprint,old_host_id,target_account_id,expected_assignment_revision,prepared_game_revision,nonce,phase,next_attempt_at,retry_attempts FROM game_host_transfers ORDER BY operation_id LIMIT 2";
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatorPurpose {
    Configuration,
    Resume,
    HostTransfer,
    HostTransferTargets,
}
impl OperatorPurpose {
    pub const fn tag(self) -> &'static str {
        match self {
            Self::Configuration => "configuration",
            Self::Resume => "resume",
            Self::HostTransfer => "host_transfer",
            Self::HostTransferTargets => "host_transfer_targets",
        }
    }
}
#[derive(Clone, Copy)]
pub(crate) enum OperatorAuditOutcome {
    Succeeded,
    Rejected,
    Failed,
}
impl OperatorAuditOutcome {
    pub(crate) const fn tag(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Rejected => "rejected",
            Self::Failed => "failed",
        }
    }
}
pub enum TransferPreparation {
    Prepared(HostTransferWork),
    Committed(brews_contracts::games::GameReceipt),
}
pub(crate) fn patch_fingerprint(patch: &ConfigurationPatch) -> Result<[u8; 32], GameError> {
    use sha2::{Digest, Sha256};
    let mut patch = patch.clone();
    if let Some(positions) = &mut patch.free_cell_positions {
        positions.sort_by_key(|p| (p.row, p.column));
    }
    let mut hash = Sha256::new();
    hash.update(b"brews-configuration-patch-v1\0");
    hash.update(serde_json::to_vec(&patch).map_err(|_| GameError::InvalidInput)?);
    Ok(hash.finalize().into())
}
pub(crate) fn transfer_fingerprint(input: &HostTransferInput) -> Result<[u8; 32], GameError> {
    use sha2::{Digest, Sha256};
    if !input.confirmed {
        return Err(GameError::InvalidInput);
    }
    let mut hash = Sha256::new();
    hash.update(b"brews-host-transfer-v1\0");
    hash.update(input.target_account_id.to_string().as_bytes());
    hash.update(revision(input.expected_host_assignment_revision)?.to_be_bytes());
    Ok(hash.finalize().into())
}
fn parse_transfer(row: &super::super::Row) -> Result<HostTransferWork, GameError> {
    if row.len() != 13 {
        return Err(GameError::Storage);
    }
    let result = HostTransferWork {
        operation: string(row, 0)?.parse().map_err(|_| GameError::Storage)?,
        game: string(row, 1)?.parse().map_err(|_| GameError::Storage)?,
        actor: string(row, 2)?.parse().map_err(|_| GameError::Storage)?,
        command: string(row, 3)?.parse().map_err(|_| GameError::Storage)?,
        fingerprint: blob(row, 4)?.try_into().map_err(|_| GameError::Storage)?,
        old_host: string(row, 5)?.parse().map_err(|_| GameError::Storage)?,
        target: string(row, 6)?.parse().map_err(|_| GameError::Storage)?,
        expected_assignment: number(row, 7)?,
        prepared_revision: number(row, 8)?,
        nonce: blob(row, 9)?.try_into().map_err(|_| GameError::Storage)?,
        phase: TransferPhase::parse(string(row, 10)?)?,
        next: number(row, 11)?,
        retry_attempts: u32::try_from(number(row, 12)?).map_err(|_| GameError::Storage)?,
    };
    if result.retry_attempts > crate::game::HOST_TRANSFER_RETRY_MAX_SHIFT
        || result.old_host == result.target
        || [
            result.expected_assignment,
            result.prepared_revision,
            result.next,
        ]
        .iter()
        .any(|v| !(0..=JS_SAFE_INTEGER_MAX).contains(v))
    {
        return Err(GameError::Storage);
    }
    Ok(result)
}
impl<D: Database, R: Runtime> GameService<'_, D, R> {
    fn operator_attempt<T>(
        &self,
        authority: &GameAccountAuthority,
        purpose: OperatorPurpose,
        action: impl FnOnce(i64) -> Result<(T, bool), GameError>,
    ) -> Result<T, GameError> {
        let game = match self.operator_game_identity()? {
            Some(game) => Some(game),
            None => self.record_optional()?.map(|r| r.id),
        };
        let authenticated_admin = std::cell::Cell::new(
            authority.role() == AccountRole::Admin
                && authorize(authority, self.runtime.now_ms()).is_ok(),
        );
        let result = self.maintenance(|now| {
            authenticated_admin
                .set(authority.role() == AccountRole::Admin && authorize(authority, now).is_ok());
            let out = action(now);
            if authenticated_admin.get()
                && let Some(game) = game
            {
                match &out {
                    Ok((_, true)) => self.operator_audit(
                        authority.account_id(),
                        game,
                        purpose,
                        OperatorAuditOutcome::Succeeded,
                        now,
                    )?,
                    Err(error)
                        if !matches!(error, GameError::Storage | GameError::RandomUnavailable) =>
                    {
                        self.operator_audit(
                            authority.account_id(),
                            game,
                            purpose,
                            OperatorAuditOutcome::Rejected,
                            now,
                        )?
                    }
                    _ => {}
                }
            }
            out.map(|(value, _)| value)
        });
        if matches!(
            result.as_ref().err(),
            Some(GameError::Storage | GameError::RandomUnavailable)
        ) && authenticated_admin.get()
            && let Some(game) = game
        {
            self.maintenance(|now| {
                self.operator_audit(
                    authority.account_id(),
                    game,
                    purpose,
                    OperatorAuditOutcome::Failed,
                    now,
                )
            })?;
        }
        result
    }
    fn operator_audit(
        &self,
        actor: AccountId,
        game: GameId,
        purpose: OperatorPurpose,
        outcome: OperatorAuditOutcome,
        now: i64,
    ) -> Result<(), GameError> {
        let audit = make_id(self.runtime, now).map_err(|_| GameError::Storage)?;
        let row = vec![
            text(audit),
            text(actor),
            text(game),
            text(purpose.tag()),
            text(outcome.tag()),
            int(now),
            int(add_time(now, GAME_AUDIT_RETENTION_MS)?),
        ];
        self.db.execute(SQL_INSERT_GAME_AUDIT, &row)?;
        if self.db.query("SELECT audit_id,actor_account_id,target_game_id,operation,outcome,occurred_at,expires_at FROM game_admin_audit WHERE audit_id=?",&[text(audit)])?!=vec![row] { return Err(GameError::Storage); }
        Ok(())
    }
    pub fn capture_operator_failure(
        &self,
        authority: &GameAccountAuthority,
        purpose: OperatorPurpose,
    ) -> Result<(), GameError> {
        self.maintenance(|now| {
            authorize(authority, now)?;
            if authority.role() == AccountRole::Admin
                && let Some(game) = self
                    .record_optional()?
                    .map(|r| r.id)
                    .or(self.operator_game_identity()?)
            {
                self.operator_audit(
                    authority.account_id(),
                    game,
                    purpose,
                    OperatorAuditOutcome::Failed,
                    now,
                )?;
            }
            Ok(())
        })
    }
    fn renew_operator_activity(
        &self,
        r: &Record,
        authority: &GameAccountAuthority,
        now: i64,
    ) -> Result<bool, GameError> {
        if authority.account_id() != r.host || r.due.is_none() {
            return Ok(false);
        }
        ensure_before_idle(r, now)?;
        let due = add_time(now, HOST_IDLE_MS)?;
        if r.due == Some(due) {
            return Ok(false);
        }
        self.db.execute(
            SQL_UPDATE_OPERATOR_ACTIVITY,
            &[int(now), int(due), text(r.host)],
        )?;
        if self.record()?.due != Some(due) {
            return Err(GameError::Storage);
        }
        Ok(true)
    }
    fn assignment_revision(&self) -> Result<i64, GameError> {
        let rows = self.db.query(
            "SELECT host_assignment_revision FROM game_record WHERE singleton=1",
            &[],
        )?;
        number(rows.first().ok_or(GameError::NotFound)?, 0)
    }
    pub fn update_configuration(
        &self,
        authority: GameAccountAuthority,
        command: CommandId,
        patch: &ConfigurationPatch,
    ) -> Result<GameResponse, GameError> {
        let fp = patch_fingerprint(patch)?;
        self.operator_attempt(&authority,OperatorPurpose::Configuration,|now| {
            authorize(&authority,now)?;
            let r=self.record()?;ensure_before_idle(&r,now)?;
            let actor=account_actor(authority.account_id());
            if let Some(receipt)=self.receipt(&actor,command,fp,now)? { return Ok((GameResponse::Committed {receipt},false)); }
            authorize_mutation(&authority,&r,now)?;self.admit(command,now)?;
            if r.state!=GameState::New { return Err(GameError::Conflict); }
            if self.view_revision(HOST_VIEW)?!=revision(patch.expected_revision)? { return Err(GameError::StaleRevision); }
            if !self.all_work(1)?.is_empty() || !self.host_transfer_work()?.is_empty() { return Err(GameError::Conflict); }
            let current=self.configuration()?;
            let next=patch.apply(&current).map_err(|_|GameError::InvalidInput)?;
            let changed=crate::game::creation_fingerprint(&current)?!=crate::game::creation_fingerprint(&next)?;
            let mut view=self.view_revision(HOST_VIEW)?;
            if changed {
                self.db.execute("UPDATE game_configuration SET numeric_upper_bound=?,board_side_length=?,free_cells_enabled=?,player_capacity=?,spectator_capacity=?,winning_pattern=? WHERE singleton=1",&[int(i64::from(next.numeric_upper_bound)),int(i64::from(next.board_side_length)),int(i64::from(next.free_cells_enabled)),int(i64::from(next.player_capacity)),int(i64::from(next.spectator_capacity)),text(next.winning_pattern)])?;
                self.db.execute(SQL_DELETE_GAME_FREE_CELLS,&[])?;
                for p in &next.free_cell_positions { self.db.execute(SQL_INSERT_GAME_FREE_CELLS,&[int(i64::from(p.row)),int(i64::from(p.column))])?; }
                if crate::game::creation_fingerprint(&self.configuration()?)?!=crate::game::creation_fingerprint(&next)? { return Err(GameError::Storage); }
                self.db.execute("UPDATE game_record SET revision=? WHERE singleton=1 AND revision=?",&[int(advance(r.revision)?),int(r.revision)])?;
                if self.record()?.revision!=advance(r.revision)? { return Err(GameError::Storage); }
                view=advance(view)?;self.set_view_revision(HOST_VIEW,view)?;
                self.renew_operator_activity(&r,&authority,now)?;
            }
            let receipt=self.store_receipt(&actor,command,fp,GameOutcome::ConfigurationUpdated {view_revision:view as u64,changed},r.id,now)?;
            Ok((GameResponse::ConfigurationUpdated {configuration:next,receipt},true))
        })
    }
    pub fn resume_game(
        &self,
        authority: GameAccountAuthority,
        command: CommandId,
    ) -> Result<GameResponse, GameError> {
        let fp = command_fingerprint("resume", 0);
        self.operator_attempt(&authority, OperatorPurpose::Resume, |now| {
            authorize(&authority, now)?;
            let r = self.record()?;
            ensure_before_idle(&r, now)?;
            if !matches!(
                r.state,
                GameState::New | GameState::AwaitingPlayers | GameState::InProgress
            ) {
                return Err(GameError::Conflict);
            }
            let actor = account_actor(authority.account_id());
            if let Some(receipt) = self.receipt(&actor, command, fp, now)? {
                return Ok((GameResponse::Committed { receipt }, false));
            }
            self.admit(command, now)?;
            if self.renew_operator_activity(&r, &authority, now)? {
                self.db.execute(
                    "UPDATE game_record SET revision=? WHERE singleton=1 AND revision=?",
                    &[int(advance(r.revision)?), int(r.revision)],
                )?;
                if self.record()?.revision != advance(r.revision)? {
                    return Err(GameError::Storage);
                }
                self.set_view_revision(HOST_VIEW, advance(self.view_revision(HOST_VIEW)?)?)?;
            }
            let r = self.record()?;
            let game = self.host_projection(&r, now)?;
            let receipt = self.store_receipt(
                &actor,
                command,
                fp,
                GameOutcome::Resumed {
                    view_revision: game.view_revision(),
                },
                r.id,
                now,
            )?;
            Ok((
                GameResponse::Resumed {
                    game,
                    can_mutate: authority.account_id() == r.host
                        || authority.role() == AccountRole::Admin,
                    host_assignment_revision: self.assignment_revision()? as u64,
                    receipt,
                },
                true,
            ))
        })
    }
    pub fn host_transfer_work(&self) -> Result<Vec<HostTransferWork>, GameError> {
        let rows = self.db.query(SQL_SELECT_TRANSFER, &[])?;
        if rows.len() > 1 {
            return Err(GameError::Storage);
        }
        rows.iter().map(parse_transfer).collect()
    }
    pub fn prepare_host_transfer(
        &self,
        authority: GameAccountAuthority,
        command: CommandId,
        input: &HostTransferInput,
    ) -> Result<TransferPreparation, GameError> {
        let fp = transfer_fingerprint(input)?;
        // Preparation is not a successful privileged action. Capture rejection/failure;
        // success audit belongs atomically to the final assignment/receipt transaction.
        self.operator_attempt(&authority, OperatorPurpose::HostTransfer, |now| {
            authorize(&authority, now)?;
            let r = self.record()?;
            ensure_before_idle(&r, now)?;
            let actor = account_actor(authority.account_id());
            if let Some(receipt) = self.receipt(&actor, command, fp, now)? {
                return Ok((TransferPreparation::Committed(receipt), false));
            }
            authorize_mutation(&authority, &r, now)?;
            self.admit(command, now)?;
            if !matches!(
                r.state,
                GameState::New | GameState::AwaitingPlayers | GameState::InProgress
            ) || input.target_account_id == r.host
            {
                return Err(GameError::Conflict);
            }
            if self.assignment_revision()? != revision(input.expected_host_assignment_revision)? {
                return Err(GameError::StaleRevision);
            }
            if !self.all_work(1)?.is_empty() {
                return Err(GameError::Conflict);
            }
            if let Some(work) = self.host_transfer_work()?.first() {
                if work.actor == authority.account_id()
                    && work.command == command
                    && work.fingerprint == fp
                    && work.phase == TransferPhase::Prepared
                {
                    return Ok((TransferPreparation::Prepared(work.clone()), false));
                }
                return Err(GameError::Conflict);
            }
            // Check all counter arithmetic before reserving assignment coordination.
            advance(self.assignment_revision()?)?;
            advance(r.revision)?;
            advance(self.view_revision(HOST_VIEW)?)?;
            let mut nonce = [0; 32];
            self.runtime
                .fill_random(&mut nonce)
                .map_err(|_| GameError::RandomUnavailable)?;
            let work = HostTransferWork {
                operation: make_id(self.runtime, now)?
                    .try_into()
                    .map_err(|_| GameError::Storage)?,
                game: r.id,
                actor: authority.account_id(),
                command,
                fingerprint: fp,
                old_host: r.host,
                target: input.target_account_id,
                expected_assignment: self.assignment_revision()?,
                prepared_revision: r.revision,
                nonce,
                phase: TransferPhase::Prepared,
                next: add_time(now, crate::game::RETRY_INITIAL_MS)?,
                retry_attempts: 0,
            };
            self.db.execute(
                "INSERT INTO game_host_transfers VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)",
                &[
                    text(work.operation),
                    text(work.game),
                    text(work.actor),
                    text(work.command),
                    SqlValue::Blob(fp.to_vec()),
                    text(work.old_host),
                    text(work.target),
                    int(work.expected_assignment),
                    int(work.prepared_revision),
                    SqlValue::Blob(nonce.to_vec()),
                    text(work.phase.tag()),
                    int(work.next),
                    int(i64::from(work.retry_attempts)),
                ],
            )?;
            if self.host_transfer_work()? != vec![work.clone()] {
                return Err(GameError::Storage);
            }
            Ok((TransferPreparation::Prepared(work), false))
        })
    }
    pub fn commit_host_transfer(
        &self,
        authority: GameAccountAuthority,
        recipient: crate::auth::HostRecipientAuthority,
        grant: &HostTransferGrant,
    ) -> Result<GameResponse, GameError> {
        self.operator_attempt(&authority,OperatorPurpose::HostTransfer,|now|{
            authorize(&authority,now)?;
            let r=self.record()?;ensure_before_idle(&r,now)?;
            let work=self.host_transfer_work()?.into_iter().next().ok_or(GameError::Conflict)?;
            if !work.same_binding(&grant.work) || work.actor!=authority.account_id() { return Err(GameError::Conflict); }
            let actor=account_actor(work.actor);
            if let Some(receipt)=self.receipt(&actor,work.command,work.fingerprint,now)? { return Ok((GameResponse::Committed {receipt},false)); }
            authorize_mutation(&authority,&r,now)?;
            if !self.all_work(1)?.is_empty() || work.phase!=TransferPhase::Prepared || r.id!=work.game || r.host!=work.old_host || r.revision!=work.prepared_revision || self.assignment_revision()?!=work.expected_assignment || !matches!(r.state,GameState::New|GameState::AwaitingPlayers|GameState::InProgress) { return Err(GameError::Conflict); }
            if !recipient.matches_transfer(&work) || recipient.account_id()!=work.target || recipient.checked_at()>now { return Err(GameError::Unauthorized); }
            let assignment=advance(work.expected_assignment)?;let rev=advance(r.revision)?;let view=advance(self.view_revision(HOST_VIEW)?)?;
            self.renew_operator_activity(&r,&authority,now)?;
            self.db.execute("UPDATE game_record SET host_id=?,host_assignment_revision=?,revision=? WHERE singleton=1 AND host_id=? AND host_assignment_revision=? AND revision=?",&[text(work.target),int(assignment),int(rev),text(work.old_host),int(work.expected_assignment),int(r.revision)])?;
            if self.record()?.host!=work.target || self.record()?.revision!=rev || self.assignment_revision()?!=assignment { return Err(GameError::Storage); }
            self.set_view_revision(HOST_VIEW,view)?;
            let receipt=self.store_receipt(&actor,work.command,work.fingerprint,GameOutcome::HostTransferred {view_revision:view as u64,designated_host_id:work.target,host_assignment_revision:assignment as u64},r.id,now)?;
            self.set_transfer_phase(&work,TransferPhase::Committed,now)?;
            Ok((GameResponse::HostTransferred {receipt},true))
        })
    }
    fn set_transfer_phase(
        &self,
        work: &HostTransferWork,
        phase: TransferPhase,
        now: i64,
    ) -> Result<(), GameError> {
        self.db.execute("UPDATE game_host_transfers SET phase=?,next_attempt_at=?,retry_attempts=0 WHERE operation_id=? AND phase=?",&[text(phase.tag()),int(add_time(now,crate::game::RETRY_INITIAL_MS)?),text(work.operation),text(work.phase.tag())])?;
        let actual = self
            .host_transfer_work()?
            .into_iter()
            .next()
            .ok_or(GameError::Storage)?;
        if !actual.same_binding(work) || actual.phase != phase || actual.retry_attempts != 0 {
            return Err(GameError::Storage);
        }
        Ok(())
    }
    pub fn host_transfer_proof(
        &self,
        expected: &HostTransferWork,
        abort_prepared: bool,
    ) -> Result<HostTransferProof, GameError> {
        self.maintenance(|now| {
            let work = self
                .host_transfer_work()?
                .into_iter()
                .next()
                .ok_or(GameError::Conflict)?;
            if !work.same_binding(expected) {
                return Err(GameError::Conflict);
            }
            if work.phase == TransferPhase::Prepared {
                if !abort_prepared {
                    return Err(GameError::Conflict);
                }
                self.set_transfer_phase(&work, TransferPhase::Aborted, now)?;
            }
            let current = self
                .host_transfer_work()?
                .into_iter()
                .next()
                .ok_or(GameError::Storage)?;
            let projection = match self.record_optional()? {
                Some(r) => r.projection()?,
                None => self
                    .terminal_routing_projection()?
                    .ok_or(GameError::NotFound)?,
            };
            if projection.game_id() != current.game
                || (current.phase == TransferPhase::Committed
                    && projection.designated_host_id() != current.target)
                || (current.phase == TransferPhase::Aborted
                    && projection.designated_host_id() != current.old_host)
            {
                return Err(GameError::Conflict);
            }
            Ok(HostTransferProof {
                work: current,
                projection,
            })
        })
    }
    pub fn retry_host_transfer(&self, expected: &HostTransferWork) -> Result<(), GameError> {
        self.maintenance(|now|{
            let Some(current)=self.host_transfer_work()?.into_iter().next() else{return Ok(());};
            if !current.same_binding(expected) || current.retry_attempts!=expected.retry_attempts || (current.phase==expected.phase && current.next!=expected.next) {return Ok(());}
            let attempts=(current.retry_attempts+1).min(crate::game::HOST_TRANSFER_RETRY_MAX_SHIFT);
            let delay=crate::game::host_transfer_retry_delay(attempts).ok_or(GameError::Storage)?;
            let next=add_time(now,delay)?;
            self.db.execute("UPDATE game_host_transfers SET next_attempt_at=?,retry_attempts=? WHERE operation_id=? AND phase=? AND retry_attempts=?",&[int(next),int(i64::from(attempts)),text(current.operation),text(current.phase.tag()),int(i64::from(current.retry_attempts))])?;
            let actual=self.host_transfer_work()?.into_iter().next().ok_or(GameError::Storage)?;
            if !actual.same_binding(&current) || actual.phase!=current.phase || actual.next!=next || actual.retry_attempts!=attempts{return Err(GameError::Storage);}Ok(())
        })
    }
    pub fn acknowledge_host_transfer(
        &self,
        proof: &HostTransferProof,
        ack: HostTransferAck,
    ) -> Result<(), GameError> {
        self.maintenance(|_| {
            let work = self
                .host_transfer_work()?
                .into_iter()
                .next()
                .ok_or(GameError::Conflict)?;
            if !work.same_binding(&proof.work)
                || !work.same_binding(&ack.work)
                || work.phase != proof.work.phase
                || work.phase != ack.work.phase
                || work.phase == TransferPhase::Prepared
            {
                return Err(GameError::Conflict);
            }
            self.db.execute(
                "DELETE FROM game_host_transfers WHERE operation_id=? AND phase=?",
                &[text(work.operation), text(work.phase.tag())],
            )?;
            if !self.host_transfer_work()?.is_empty() {
                return Err(GameError::Storage);
            }
            Ok(())
        })
    }
}

impl<D: Database, R: Runtime> GameService<'_, D, R> {
    pub fn authorize_host_target_discovery(
        &self,
        authority: &GameAccountAuthority,
    ) -> Result<(GameId, AccountId, u64), GameError> {
        self.operator_attempt(authority, OperatorPurpose::HostTransferTargets, |now| {
            let r = self.record()?;
            authorize_mutation(authority, &r, now)?;
            ensure_before_idle(&r, now)?;
            if !matches!(
                r.state,
                GameState::New | GameState::AwaitingPlayers | GameState::InProgress
            ) {
                return Err(GameError::Conflict);
            }
            Ok(((r.id, r.host, self.assignment_revision()? as u64), false))
        })
    }
    pub fn audit_host_target_discovery(
        &self,
        authority: &GameAccountAuthority,
        expected: (GameId, AccountId, u64),
    ) -> Result<(), GameError> {
        self.operator_attempt(authority, OperatorPurpose::HostTransferTargets, |now| {
            let r = self.record()?;
            authorize_mutation(authority, &r, now)?;
            ensure_before_idle(&r, now)?;
            if (r.id, r.host, self.assignment_revision()? as u64) != expected {
                return Err(GameError::Conflict);
            }
            Ok(((), true))
        })
    }
    pub fn authorize_operator_request_output(
        &self,
        authority: &GameAccountAuthority,
        response: &GameResponse,
        purpose: OperatorPurpose,
        input_fingerprint: [u8; 32],
    ) -> Result<i64, GameError> {
        let deadline = self.authorize_operator_output(authority, response)?;
        self.maintenance(|now| {
            let receipt = match response {
                GameResponse::Committed { receipt }
                | GameResponse::ConfigurationUpdated { receipt, .. }
                | GameResponse::Resumed { receipt, .. }
                | GameResponse::HostTransferred { receipt } => receipt,
                _ => return Err(GameError::InvalidInput),
            };
            let stored = self
                .receipt(
                    &account_actor(authority.account_id()),
                    receipt.command_id,
                    input_fingerprint,
                    now,
                )?
                .ok_or(GameError::Unauthorized)?;
            let coherent = matches!(
                (purpose, &stored.outcome),
                (
                    OperatorPurpose::Configuration,
                    GameOutcome::ConfigurationUpdated { .. }
                ) | (OperatorPurpose::Resume, GameOutcome::Resumed { .. })
                    | (
                        OperatorPurpose::HostTransfer,
                        GameOutcome::HostTransferred { .. }
                    )
            );
            if !coherent
                || stored.encode_json().map_err(|_| GameError::Storage)?
                    != receipt.encode_json().map_err(|_| GameError::Storage)?
            {
                return Err(GameError::Unauthorized);
            }
            Ok(deadline)
        })
    }
    pub fn authorize_operator_output(
        &self,
        authority: &GameAccountAuthority,
        response: &GameResponse,
    ) -> Result<i64, GameError> {
        self.maintenance(|now| {
            authorize(authority, now)?;
            let r = self.record()?;
            ensure_before_idle(&r, now)?;
            let receipt = match response {
                GameResponse::Committed { receipt }
                | GameResponse::ConfigurationUpdated { receipt, .. }
                | GameResponse::Resumed { receipt, .. }
                | GameResponse::HostTransferred { receipt } => receipt,
                _ => return Err(GameError::InvalidInput),
            };
            let rows = self.db.query(
                SQL_SELECT_REQUIRED_GAME_RECEIPT,
                &[
                    text(account_actor(authority.account_id())),
                    text(receipt.command_id),
                ],
            )?;
            let row = rows.first().ok_or(GameError::Unauthorized)?;
            if receipt.game_id != r.id
                || number(row, 5)? <= now
                || string(row, 3)?
                    != String::from_utf8(receipt.encode_json().map_err(|_| GameError::Storage)?)
                        .map_err(|_| GameError::Storage)?
            {
                return Err(GameError::Unauthorized);
            }
            match response {
                GameResponse::Resumed {
                    game,
                    can_mutate,
                    host_assignment_revision,
                    ..
                } => {
                    if !matches!(
                        r.state,
                        GameState::New | GameState::AwaitingPlayers | GameState::InProgress
                    ) || game.view_revision() != self.view_revision(HOST_VIEW)? as u64
                        || *host_assignment_revision != self.assignment_revision()? as u64
                        || *can_mutate
                            != (authority.account_id() == r.host
                                || authority.role() == AccountRole::Admin)
                    {
                        return Err(GameError::Conflict);
                    }
                }
                GameResponse::ConfigurationUpdated { configuration, .. } => {
                    authorize_mutation(authority, &r, now)?;
                    if r.state != GameState::New
                        || crate::game::creation_fingerprint(configuration)?
                            != crate::game::creation_fingerprint(&self.configuration()?)?
                    {
                        return Err(GameError::Conflict);
                    }
                }
                _ => {}
            }
            Ok(authority
                .expires_at()
                .min(receipt.expires_at)
                .min(r.due.unwrap_or(JS_SAFE_INTEGER_MAX)))
        })
    }
    pub(super) fn cleanup_operator_audit(&self, now: i64) -> Result<(), GameError> {
        let rows=self.db.query("SELECT audit_id FROM game_admin_audit WHERE expires_at<=? ORDER BY expires_at,audit_id LIMIT ?",&[int(now),int(crate::limits::CLEANUP_BATCH_SIZE)])?;
        for row in rows {
            let id = string(&row, 0)?;
            self.db.execute(
                "DELETE FROM game_admin_audit WHERE audit_id=? AND expires_at<=?",
                &[text(id), int(now)],
            )?;
            if !self
                .db
                .query(
                    "SELECT 1 FROM game_admin_audit WHERE audit_id=?",
                    &[text(id)],
                )?
                .is_empty()
            {
                return Err(GameError::Storage);
            }
        }
        Ok(())
    }
}

impl<D: Database, R: Runtime> GameService<'_, D, R> {
    pub fn capture_operator_rejection(
        &self,
        authority: &GameAccountAuthority,
        purpose: OperatorPurpose,
    ) -> Result<(), GameError> {
        self.maintenance(|now| {
            authorize(authority, now)?;
            if authority.role() == AccountRole::Admin
                && let Some(game) = self
                    .record_optional()?
                    .map(|r| r.id)
                    .or(self.operator_game_identity()?)
            {
                self.operator_audit(
                    authority.account_id(),
                    game,
                    purpose,
                    OperatorAuditOutcome::Rejected,
                    now,
                )?;
            }
            Ok(())
        })
    }
    pub fn operator_discovery_deadline(
        &self,
        authority: &GameAccountAuthority,
    ) -> Result<i64, GameError> {
        self.maintenance(|now| {
            let r = self.record()?;
            authorize_mutation(authority, &r, now)?;
            ensure_before_idle(&r, now)?;
            Ok(authority
                .expires_at()
                .min(r.due.unwrap_or(JS_SAFE_INTEGER_MAX)))
        })
    }
    pub fn fence_operator_output_expiry(&self, observed_at: i64) -> Result<(), GameError> {
        self.db
            .transaction(|| {
                super::validate_schema(self.db)?;
                self.fence_read_deadline(observed_at)
                    .map_err(|_| StorageError)
            })
            .map_err(|_| GameError::Storage)
    }
}

impl<D: Database, R: Runtime> GameService<'_, D, R> {
    pub(crate) fn operator_game_identity(&self) -> Result<Option<GameId>, GameError> {
        let rows = self.db.query(
            "SELECT game_id FROM game_operator_identity WHERE singleton=1",
            &[],
        )?;
        match rows.as_slice() {
            [] => Ok(None),
            [row] => string(row, 0)?
                .parse()
                .map(Some)
                .map_err(|_| GameError::Storage),
            _ => Err(GameError::Storage),
        }
    }
    // Caller must first prove its actual private Durable Object namespace identity.
    // This retains only a stable target ID, never game content or authorization.
    #[cfg(any(test, target_arch = "wasm32"))]
    pub(crate) fn bind_operator_game_identity(&self, id: GameId) -> Result<(), GameError> {
        self.maintenance(|_| {
            if self.record_optional()?.is_some_and(|r| r.id != id)
                || self
                    .terminal_routing_projection()?
                    .is_some_and(|p| p.game_id() != id)
            {
                return Err(GameError::Conflict);
            }
            match self.operator_game_identity()? {
                Some(current) if current != id => return Err(GameError::Conflict),
                Some(_) => return Ok(()),
                None => {}
            }
            self.db.execute(
                "INSERT INTO game_operator_identity VALUES(1,?)",
                &[text(id)],
            )?;
            if self.operator_game_identity()? != Some(id) {
                return Err(GameError::Storage);
            }
            Ok(())
        })
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Real SQLite operator fixtures fail fast."
)]
mod tests {
    use super::*;
    use crate::auth::test_support::{Sqlite, TestRuntime};
    use crate::db::directory::{DirectoryService, migrate_directory};
    struct Fixture {
        game: Sqlite,
        directory: Sqlite,
        rt: TestRuntime,
    }
    impl Fixture {
        fn new() -> Self {
            let f = Self {
                game: Sqlite::new(),
                directory: Sqlite::new(),
                rt: TestRuntime::new(),
            };
            super::super::migrate_game(&f.game).unwrap();
            migrate_directory(&f.directory).unwrap();
            let config = GameConfiguration::default();
            let work = f
                .directory()
                .claim_game(
                    f.id(1),
                    f.id(3),
                    crate::game::creation_fingerprint(&config).unwrap(),
                )
                .unwrap();
            let created = f.service().initialize(&work, f.actor(1), &config).unwrap();
            f.directory().acknowledge_creation(created.ready).unwrap();
            f
        }
        fn id<T: std::str::FromStr>(&self, n: u8) -> T
        where
            T::Err: std::fmt::Debug,
        {
            let mut b = [0; 16];
            b[..6].copy_from_slice(&self.rt.now.get().to_be_bytes()[2..]);
            b[6] = 0x70;
            b[8] = 0x80;
            b[15] = n;
            uuid::Uuid::from_bytes(b).to_string().parse().unwrap()
        }
        fn actor(&self, n: u8) -> GameAccountAuthority {
            GameAccountAuthority::from_trusted_peer(
                self.id(n),
                self.id(n + 20),
                AccountRole::Host,
                0,
                self.rt.now.get() + HOST_IDLE_MS + 10000,
            )
            .unwrap()
        }
        fn service(&self) -> GameService<'_, Sqlite, TestRuntime> {
            GameService::new(&self.game, &self.rt, AuthPolicy::default(), &[9; 32]).unwrap()
        }
        fn directory(&self) -> DirectoryService<'_, Sqlite, TestRuntime> {
            DirectoryService::new(&self.directory, &self.rt).unwrap()
        }
        fn prepared(
            &self,
            actor: u8,
            target: u8,
            assignment: u64,
            n: u8,
        ) -> (HostTransferWork, HostTransferGrant) {
            let input = HostTransferInput {
                target_account_id: self.id(target),
                confirmed: true,
                expected_host_assignment_revision: assignment,
            };
            let work = match self
                .service()
                .prepare_host_transfer(self.actor(actor), self.id(n), &input)
                .unwrap()
            {
                TransferPreparation::Prepared(w) => w,
                _ => panic!("Prepared work required"),
            };
            let grant = self.directory().prepare_host_transfer(&work).unwrap();
            (work, grant)
        }
        fn recipient(&self, work: &HostTransferWork) -> crate::auth::HostRecipientAuthority {
            crate::auth::HostRecipientAuthority::from_trusted_peer(
                work.target,
                0,
                self.rt.now.get(),
            )
            .unwrap()
            .bind_transfer(work)
            .unwrap()
        }
    }
    struct FirstAuditEntropyFailure<'a> {
        inner: &'a TestRuntime,
        draws: std::cell::Cell<u32>,
    }
    impl Runtime for FirstAuditEntropyFailure<'_> {
        fn now_ms(&self) -> i64 {
            self.inner.now_ms()
        }
        fn fill_random(&self, bytes: &mut [u8]) -> Result<(), crate::auth::AuthError> {
            let draw = self.draws.get();
            self.draws.set(draw + 1);
            if draw == 0 {
                return Err(crate::auth::AuthError::Crypto);
            }
            self.inner.fill_random(bytes)
        }
    }
    #[test]
    fn operator_audit_entropy_failure_rolls_back_each_mutation_and_separately_records_failed() {
        for operation in 0..3 {
            let f = Fixture::new();
            let host = f.service().record().unwrap().host;
            let admin = || {
                GameAccountAuthority::from_trusted_peer(
                    host,
                    f.id(20),
                    AccountRole::Admin,
                    0,
                    f.rt.now.get() + HOST_IDLE_MS + 10000,
                )
                .unwrap()
            };
            let command = f.id(6);
            let prepared = if operation == 2 {
                let input = HostTransferInput {
                    target_account_id: f.id(2),
                    confirmed: true,
                    expected_host_assignment_revision: 0,
                };
                let TransferPreparation::Prepared(work) = f
                    .service()
                    .prepare_host_transfer(admin(), command, &input)
                    .unwrap()
                else {
                    panic!("prepared");
                };
                let grant = f.directory().prepare_host_transfer(&work).unwrap();
                Some((work, grant))
            } else {
                None
            };
            let snapshot = || {
                [
                    "game_record",
                    "game_configuration",
                    "game_receipts",
                    "game_view_revisions",
                    "game_host_transfers",
                ]
                .map(|table| {
                    f.game
                        .query(&format!("SELECT * FROM {table} ORDER BY 1"), &[])
                        .unwrap()
                })
            };
            let before = snapshot();
            f.rt.now.set(f.rt.now.get() + 1);
            let rt = FirstAuditEntropyFailure {
                inner: &f.rt,
                draws: std::cell::Cell::new(0),
            };
            let service = GameService::new(&f.game, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
            let result = match operation {
                0 => service.update_configuration(
                    admin(),
                    command,
                    &ConfigurationPatch::decode_json(
                        br#"{"expected_revision":0,"player_capacity":3}"#,
                    )
                    .unwrap(),
                ),
                1 => service.resume_game(admin(), command),
                _ => {
                    let (work, grant) = prepared.as_ref().unwrap();
                    service.commit_host_transfer(admin(), f.recipient(work), grant)
                }
            };
            assert!(matches!(result, Err(GameError::Storage)));
            assert_eq!(snapshot(), before);
            let rows = f
                .game
                .query(
                    "SELECT outcome FROM game_admin_audit ORDER BY occurred_at,audit_id",
                    &[],
                )
                .unwrap();
            // Preparation is not successful mutation capture; each failed action
            // has one separate accepted failure row and no successful action row.
            assert_eq!(rows, vec![vec![text("failed")]]);
        }
    }
    #[test]
    fn operator_transfer_retry_deadlines_back_off_survive_stale_retry_and_keep_uncertain_guards() {
        let f = Fixture::new();
        let (original, _) = f.prepared(1, 2, 0, 4);
        let mut game_work = original.clone();
        let mut directory_work = original;
        for attempt in 1..=12 {
            let now = f.rt.now.get();
            let shift = attempt.min(crate::game::HOST_TRANSFER_RETRY_MAX_SHIFT);
            f.service().retry_host_transfer(&game_work).unwrap();
            f.directory().retry_host_transfer(&directory_work).unwrap();
            let game = f.service().host_transfer_work().unwrap().remove(0);
            f.rt.now.set(game.next);
            let directory = f.directory().due_host_transfers().unwrap().remove(0);
            let delay = crate::game::host_transfer_retry_delay(shift).unwrap();
            assert_eq!(game.next, now + delay);
            assert_eq!(directory.next, game.next);
            assert_eq!(game.retry_attempts, shift);
            assert_eq!(directory.retry_attempts, shift);
            f.service().retry_host_transfer(&game_work).unwrap();
            f.directory().retry_host_transfer(&directory_work).unwrap();
            assert_eq!(
                f.service().host_transfer_work().unwrap(),
                vec![game.clone()]
            );
            assert_eq!(
                f.directory().due_host_transfers().unwrap(),
                vec![directory.clone()]
            );
            let removal: brews_domain::ids::OperationId = f.id(20);
            assert!(f.directory().acquire_removal(removal, game.target).is_err());
            game_work = game;
            directory_work = directory;
        }
    }
    #[test]
    fn operator_identity_is_immutable_and_remains_after_prestart_terminal_cleanup() {
        let f = Fixture::new();
        let game = f.service().record().unwrap().id;
        f.service().bind_operator_game_identity(game).unwrap();
        assert_eq!(
            f.service().bind_operator_game_identity(f.id(9)),
            Err(GameError::Conflict)
        );
        f.service()
            .cancel_game(f.actor(1), f.id(5), GameState::New, true)
            .unwrap();
        f.service().cleanup().unwrap();
        assert_eq!(f.service().operator_game_identity().unwrap(), Some(game));
    }
    #[test]
    fn operator_transfer_grant_proof_ack_binds_all_immutable_fields_and_never_clears_successor() {
        let f = Fixture::new();
        let (work, grant) = f.prepared(1, 2, 0, 4);
        for field in 0..10 {
            let mut changed = grant.clone();
            alter(&f, &mut changed.work, field);
            assert!(
                f.service()
                    .commit_host_transfer(f.actor(1), f.recipient(&work), &changed)
                    .is_err(),
                "grant field {field}"
            );
            assert_eq!(f.service().record().unwrap().host, f.id::<AccountId>(1));
        }
        f.service()
            .commit_host_transfer(f.actor(1), f.recipient(&work), &grant)
            .unwrap();
        let proof = f.service().host_transfer_proof(&work, false).unwrap();
        for field in 0..10 {
            let mut changed = proof.clone();
            alter(&f, &mut changed.work, field);
            assert!(
                f.directory().settle_host_transfer(&changed).is_err(),
                "proof field {field}"
            );
        }
        let ack = f.directory().settle_host_transfer(&proof).unwrap();
        for field in 0..10 {
            let mut changed = ack.clone();
            alter(&f, &mut changed.work, field);
            assert!(
                f.service()
                    .acknowledge_host_transfer(&proof, changed)
                    .is_err(),
                "ack field {field}"
            );
            assert_eq!(f.service().host_transfer_work().unwrap().len(), 1);
        }
        f.service()
            .acknowledge_host_transfer(&proof, ack.clone())
            .unwrap();
        let (successor, _) = f.prepared(2, 5, 1, 6);
        assert!(f.service().acknowledge_host_transfer(&proof, ack).is_err());
        assert_eq!(f.service().host_transfer_work().unwrap(), vec![successor]);
    }
    fn alter(f: &Fixture, w: &mut HostTransferWork, field: u8) {
        match field {
            0 => w.operation = f.id(90),
            1 => w.game = f.id(90),
            2 => w.actor = f.id(90),
            3 => w.command = f.id(90),
            4 => w.fingerprint[0] ^= 1,
            5 => w.old_host = f.id(90),
            6 => w.target = f.id(90),
            7 => w.expected_assignment += 1,
            8 => w.prepared_revision += 1,
            _ => w.nonce[0] ^= 1,
        }
    }
    #[test]
    fn operator_transfer_aborted_work_and_stale_grants_cannot_commit_after_removal_wins() {
        let f = Fixture::new();
        let (work, grant) = f.prepared(1, 2, 0, 4);
        let proof = f.service().host_transfer_proof(&work, true).unwrap();
        let ack = f.directory().settle_host_transfer(&proof).unwrap();
        f.service().acknowledge_host_transfer(&proof, ack).unwrap();
        assert!(f.directory().acquire_removal(f.id(8), f.id(2)).is_ok());
        assert!(
            f.service()
                .commit_host_transfer(f.actor(1), f.recipient(&work), &grant)
                .is_err()
        );
        assert!(f.directory().prepare_host_transfer(&work).is_err());
        assert_eq!(f.service().record().unwrap().host, f.id::<AccountId>(1));
    }
    #[test]
    fn operator_transfer_ignored_assignment_or_phase_write_rolls_back_receipt_and_assignment() {
        for clause in [
            "BEFORE UPDATE OF host_id ON game_record",
            "BEFORE UPDATE OF phase ON game_host_transfers",
        ] {
            let f = Fixture::new();
            let (work, grant) = f.prepared(1, 2, 0, 4);
            f.game.execute(&format!("CREATE TRIGGER ignore_operator_write {clause} BEGIN SELECT RAISE(IGNORE); END"),&[]).unwrap();
            assert_eq!(
                f.service()
                    .commit_host_transfer(f.actor(1), f.recipient(&work), &grant)
                    .err(),
                Some(GameError::Storage)
            );
            assert_eq!(f.service().record().unwrap().host, f.id::<AccountId>(1));
            assert_eq!(f.service().assignment_revision().unwrap(), 0);
            assert!(
                f.service()
                    .receipt(
                        &account_actor(f.id(1)),
                        work.command,
                        work.fingerprint,
                        f.rt.now.get()
                    )
                    .unwrap()
                    .is_none()
            );
            assert_eq!(
                f.service().host_transfer_work().unwrap()[0].phase,
                TransferPhase::Prepared
            );
        }
    }
}
