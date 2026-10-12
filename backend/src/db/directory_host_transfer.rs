//! Durable assignment guard ordered against Directory removal. Uncertain intents never expire.
use super::*;
use crate::game::{
    HostTransferAck, HostTransferGrant, HostTransferProof, HostTransferWork, TransferPhase,
};
use brews_domain::ids::GameId;
const SQL_SELECT_DIRECTORY_TRANSFER: &str = "SELECT operation_id,game_id,actor_account_id,command_id,fingerprint,old_host_id,target_account_id,expected_assignment_revision,prepared_game_revision,nonce,phase,next_attempt_at,retry_attempts FROM directory_host_transfers WHERE operation_id=?";
fn work_row(work: &HostTransferWork) -> Vec<SqlValue> {
    vec![
        SqlValue::Text(work.operation.to_string()),
        SqlValue::Text(work.game.to_string()),
        SqlValue::Text(work.actor.to_string()),
        SqlValue::Text(work.command.to_string()),
        SqlValue::Blob(work.fingerprint.to_vec()),
        SqlValue::Text(work.old_host.to_string()),
        SqlValue::Text(work.target.to_string()),
        SqlValue::Integer(work.expected_assignment),
        SqlValue::Integer(work.prepared_revision),
        SqlValue::Blob(work.nonce.to_vec()),
        SqlValue::Text(work.phase.tag().to_owned()),
        SqlValue::Integer(work.next),
        SqlValue::Integer(i64::from(work.retry_attempts)),
    ]
}
fn require_shape(work: &HostTransferWork) -> Result<(), DirectoryError> {
    if work.retry_attempts > crate::game::HOST_TRANSFER_RETRY_MAX_SHIFT
        || work.old_host == work.target
        || [work.expected_assignment, work.prepared_revision, work.next]
            .iter()
            .any(|n| !(0..=JS_SAFE_INTEGER_MAX).contains(n))
    {
        return Err(DirectoryError::ProofMismatch);
    }
    Ok(())
}
impl<D: Database, R: Runtime> DirectoryService<'_, D, R> {
    pub fn prepare_host_transfer(
        &self,
        work: &HostTransferWork,
    ) -> Result<HostTransferGrant, DirectoryError> {
        require_shape(work)?;
        if work.phase != TransferPhase::Prepared {
            return Err(DirectoryError::ProofMismatch);
        }
        self.transaction(|now|{
            let rows=self.db.query(SQL_SELECT_DIRECTORY_TRANSFER,&[SqlValue::Text(work.operation.to_string())])?;
            if !rows.is_empty(){
                if rows.len()!=1 || rows[0].get(..11)!=Some(&work_row(work)[..11]) {return Err(DirectoryError::ProofMismatch); }
                // A matching unresolved guard still blocks removal; this is not authority.
                return Ok(HostTransferGrant {work:work.clone()});
            }
            self.admit_operation(work.operation,now)?;
            self.db.execute("UPDATE directory_host_transfers SET completion_acknowledged_at=coalesce(completion_acknowledged_at,?) WHERE game_id=? AND phase<>?", &[SqlValue::Integer(now),SqlValue::Text(work.game.to_string()),SqlValue::Text(TransferPhase::Prepared.tag().into())])?;
            if !self.db.query("SELECT 1 FROM directory_host_transfers WHERE game_id=? AND phase=? LIMIT 1",&[SqlValue::Text(work.game.to_string()),SqlValue::Text(TransferPhase::Prepared.tag().into())])?.is_empty(){return Err(DirectoryError::Busy);}
            if self.reserved_game()?!=Some(work.game) {return Err(DirectoryError::UnknownGame);}
            let rows=self.db.query("SELECT designated_host_id,source_revision FROM directory_game_index WHERE game_id=?",&[SqlValue::Text(work.game.to_string())])?;
            if rows.len()!=1 || rows[0].first()!=Some(&SqlValue::Text(work.old_host.to_string())) || !matches!(rows[0].get(1),Some(SqlValue::Integer(rev)) if *rev<=work.prepared_revision) {return Err(DirectoryError::ProofMismatch);}
            self.validate_account_gate(work.target,now)?;
            if !self.db.query("SELECT 1 FROM account_assignment_gates WHERE account_id=?",&[SqlValue::Text(work.target.to_string())])?.is_empty(){return Err(DirectoryError::AssignmentBlocked);}
            self.db.execute("INSERT INTO directory_host_transfers(operation_id,game_id,actor_account_id,command_id,fingerprint,old_host_id,target_account_id,expected_assignment_revision,prepared_game_revision,nonce,phase,next_attempt_at,retry_attempts) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)",&work_row(work))?;
            if self.db.query(SQL_SELECT_DIRECTORY_TRANSFER,&[SqlValue::Text(work.operation.to_string())])?!=vec![work_row(work)]{return Err(DirectoryError::Storage);}
            Ok(HostTransferGrant {work:work.clone()})
        })
    }
    pub fn settle_host_transfer(
        &self,
        proof: &HostTransferProof,
    ) -> Result<HostTransferAck, DirectoryError> {
        let work = &proof.work;
        require_shape(work)?;
        if work.phase == TransferPhase::Prepared {
            return Err(DirectoryError::ProofMismatch);
        }
        self.transaction(|now|{
            let projection=&proof.projection;
            let expected=if work.phase==TransferPhase::Committed {work.target}else{work.old_host};
            if projection.game_id()!=work.game || projection.designated_host_id()!=expected || projection.source_revision()<work.prepared_revision || (work.phase==TransferPhase::Committed && projection.source_revision()<=work.prepared_revision) {return Err(DirectoryError::ProofMismatch);}
            let rows=self.db.query(SQL_SELECT_DIRECTORY_TRANSFER,&[SqlValue::Text(work.operation.to_string())])?;
            let mut prepared=work.clone();prepared.phase=TransferPhase::Prepared;
            if rows.len()==1 && rows[0].get(..11)==Some(&work_row(work)[..11]) {return Ok(HostTransferAck {work:work.clone()});}
            // No Directory prepare was dispatched: durable aborted proof records a
            // tombstone, so a delayed prepare cannot acquire this assignment later.
            if rows.is_empty() {
                if work.phase!=TransferPhase::Aborted {return Err(DirectoryError::UnknownOperation);}
                // Below the durable floor, a delayed prepare can never acquire a
                // guard. Retain only the exact abort evidence for completion.
                if operation_timestamp(work.operation)? > self.command_floor()? {
                    self.admit_operation(work.operation,now)?;
                }
                self.db.execute("INSERT INTO directory_host_transfers(operation_id,game_id,actor_account_id,command_id,fingerprint,old_host_id,target_account_id,expected_assignment_revision,prepared_game_revision,nonce,phase,next_attempt_at,retry_attempts) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)",&work_row(work))?;
            } else {
                if rows.len()!=1 || rows[0].get(..11)!=Some(&work_row(&prepared)[..11]) {return Err(DirectoryError::ProofMismatch); }
                let projection=&proof.projection;
                let expected=if work.phase==TransferPhase::Committed {work.target}else{work.old_host};
                if projection.game_id()!=work.game || projection.designated_host_id()!=expected || projection.source_revision()<work.prepared_revision || (work.phase==TransferPhase::Committed && projection.source_revision()<=work.prepared_revision) {return Err(DirectoryError::ProofMismatch);}
                self.apply_host_transfer_projection(projection,work,now)?;
                self.db.execute("UPDATE directory_host_transfers SET phase=?,next_attempt_at=?,retry_attempts=? WHERE operation_id=? AND phase=?",&[SqlValue::Text(work.phase.tag().into()),SqlValue::Integer(work.next),SqlValue::Integer(i64::from(work.retry_attempts)),SqlValue::Text(work.operation.to_string()),SqlValue::Text(TransferPhase::Prepared.tag().into())])?;
            }
            if self.db.query(SQL_SELECT_DIRECTORY_TRANSFER,&[SqlValue::Text(work.operation.to_string())])?!=vec![work_row(work)] {return Err(DirectoryError::Storage);}
            Ok(HostTransferAck {work:work.clone()})
        })
    }
    pub fn filter_host_transfer_targets(
        &self,
        game: GameId,
        targets: &[AccountId],
    ) -> Result<Vec<AccountId>, DirectoryError> {
        if targets.len() > brews_contracts::games::HOST_TRANSFER_TARGET_LIMIT_MAX as usize + 1 {
            return Err(DirectoryError::ProofMismatch);
        }
        self.transaction(|now| {
            self.require_reserved(game)?;
            let mut eligible = Vec::with_capacity(targets.len());
            for target in targets {
                self.validate_account_gate(*target, now)?;
                if self
                    .db
                    .query(
                        "SELECT 1 FROM account_assignment_gates WHERE account_id=?",
                        &[SqlValue::Text(target.to_string())],
                    )?
                    .is_empty()
                {
                    eligible.push(*target);
                }
            }
            Ok(eligible)
        })
    }
}

fn parse_directory_transfer(row: &crate::db::Row) -> Result<HostTransferWork, DirectoryError> {
    if row.len() != 13 {
        return Err(DirectoryError::Storage);
    }
    let text = |n| match row.get(n) {
        Some(SqlValue::Text(v)) => Ok(v.as_str()),
        _ => Err(DirectoryError::Storage),
    };
    let number = |n| match row.get(n) {
        Some(SqlValue::Integer(v)) => Ok(*v),
        _ => Err(DirectoryError::Storage),
    };
    let bytes = |n| match row.get(n) {
        Some(SqlValue::Blob(v)) => v.as_slice().try_into().map_err(|_| DirectoryError::Storage),
        _ => Err(DirectoryError::Storage),
    };
    let work = HostTransferWork {
        operation: text(0)?.parse().map_err(|_| DirectoryError::Storage)?,
        game: text(1)?.parse().map_err(|_| DirectoryError::Storage)?,
        actor: text(2)?.parse().map_err(|_| DirectoryError::Storage)?,
        command: text(3)?.parse().map_err(|_| DirectoryError::Storage)?,
        fingerprint: bytes(4)?,
        old_host: text(5)?.parse().map_err(|_| DirectoryError::Storage)?,
        target: text(6)?.parse().map_err(|_| DirectoryError::Storage)?,
        expected_assignment: number(7)?,
        prepared_revision: number(8)?,
        nonce: bytes(9)?,
        phase: TransferPhase::parse(text(10)?).map_err(|_| DirectoryError::Storage)?,
        next: number(11)?,
        retry_attempts: u32::try_from(number(12)?).map_err(|_| DirectoryError::Storage)?,
    };
    require_shape(&work)?;
    Ok(work)
}
impl<D: Database, R: Runtime> DirectoryService<'_, D, R> {
    pub fn due_host_transfers(&self) -> Result<Vec<HostTransferWork>, DirectoryError> {
        self.transaction(|now|self.db.query("SELECT operation_id,game_id,actor_account_id,command_id,fingerprint,old_host_id,target_account_id,expected_assignment_revision,prepared_game_revision,nonce,phase,next_attempt_at,retry_attempts FROM directory_host_transfers WHERE completion_acknowledged_at IS NULL AND next_attempt_at<=? ORDER BY next_attempt_at,operation_id LIMIT ?",&[SqlValue::Integer(now),SqlValue::Integer(CLEANUP_BATCH_SIZE)])?.iter().map(parse_directory_transfer).collect())
    }
    pub fn retry_host_transfer(&self, work: &HostTransferWork) -> Result<(), DirectoryError> {
        self.transaction(|now|{
            let rows=self.db.query(SQL_SELECT_DIRECTORY_TRANSFER,&[SqlValue::Text(work.operation.to_string())])?;
            if rows.is_empty(){return Ok(());}
            let current=parse_directory_transfer(&rows[0])?;
            if !current.same_binding(work) || current.phase!=work.phase || current.retry_attempts!=work.retry_attempts || current.next!=work.next{return Ok(());}
            let attempts=(current.retry_attempts+1).min(crate::game::HOST_TRANSFER_RETRY_MAX_SHIFT);
            let delay=crate::game::host_transfer_retry_delay(attempts).ok_or(DirectoryError::Storage)?;
            let next=now.checked_add(delay).filter(|t|*t<=JS_SAFE_INTEGER_MAX).ok_or(DirectoryError::Storage)?;
            self.db.execute("UPDATE directory_host_transfers SET next_attempt_at=?,retry_attempts=? WHERE operation_id=? AND phase=?",&[SqlValue::Integer(next),SqlValue::Integer(i64::from(attempts)),SqlValue::Text(work.operation.to_string()),SqlValue::Text(current.phase.tag().into())])?;
            let rows=self.db.query(SQL_SELECT_DIRECTORY_TRANSFER,&[SqlValue::Text(work.operation.to_string())])?;
            if rows.len()!=1 || (parse_directory_transfer(&rows[0])?.next!=next || parse_directory_transfer(&rows[0])?.retry_attempts!=attempts){return Err(DirectoryError::Storage);}
            Ok(())
        })
    }
    pub(super) fn compact_host_transfer_receipts(&self, now: i64) -> Result<(), DirectoryError> {
        let floor = now.saturating_sub(COMMAND_RECEIPT_RETENTION_MS);
        for row in self.db.query("SELECT operation_id FROM directory_host_transfers WHERE phase<>? AND completion_acknowledged_at<=? ORDER BY completion_acknowledged_at,operation_id LIMIT ?",&[SqlValue::Text(TransferPhase::Prepared.tag().into()),SqlValue::Integer(floor),SqlValue::Integer(CLEANUP_BATCH_SIZE)])?{
            let [SqlValue::Text(operation)]=row.as_slice() else{return Err(DirectoryError::Storage);};
            self.db.execute("DELETE FROM directory_host_transfers WHERE operation_id=? AND phase<>? AND completion_acknowledged_at<=?",&[SqlValue::Text(operation.clone()),SqlValue::Text(TransferPhase::Prepared.tag().into()),SqlValue::Integer(floor)])?;
            if !self.db.query(SQL_SELECT_DIRECTORY_TRANSFER,&[SqlValue::Text(operation.clone())])?.is_empty(){return Err(DirectoryError::Storage);}
        }
        Ok(())
    }
    pub(super) fn next_host_transfer_deadline(&self) -> Result<Option<i64>, DirectoryError> {
        let rows=self.db.query("SELECT min(CASE WHEN completion_acknowledged_at IS NULL THEN next_attempt_at ELSE completion_acknowledged_at+? END) FROM directory_host_transfers",&[SqlValue::Integer(COMMAND_RECEIPT_RETENTION_MS)])?;
        match rows.as_slice() {
            [row] => match row.as_slice() {
                [SqlValue::Null] => Ok(None),
                [SqlValue::Integer(value)] if (0..=JS_SAFE_INTEGER_MAX).contains(value) => {
                    Ok(Some(*value))
                }
                _ => Err(DirectoryError::Storage),
            },
            _ => Err(DirectoryError::Storage),
        }
    }
}

impl<D: Database, R: Runtime> DirectoryService<'_, D, R> {
    /// Called only after the owning Game durably consumes this exact settled ACK.
    pub fn confirm_host_transfer_completion(
        &self,
        work: &HostTransferWork,
    ) -> Result<(), DirectoryError> {
        require_shape(work)?;
        if work.phase == TransferPhase::Prepared {
            return Err(DirectoryError::ProofMismatch);
        }
        self.transaction(|now| {
            let rows = self.db.query(SQL_SELECT_DIRECTORY_TRANSFER, &[SqlValue::Text(work.operation.to_string())])?;
            if rows.len() != 1 || rows[0].get(..11) != Some(&work_row(work)[..11]) {
                return Err(DirectoryError::ProofMismatch);
            }
            self.db.execute("UPDATE directory_host_transfers SET completion_acknowledged_at=coalesce(completion_acknowledged_at,?) WHERE operation_id=?", &[SqlValue::Integer(now),SqlValue::Text(work.operation.to_string())])?;
            if !matches!(self.db.query("SELECT completion_acknowledged_at FROM directory_host_transfers WHERE operation_id=?", &[SqlValue::Text(work.operation.to_string())])?.as_slice(), [row] if matches!(row.as_slice(), [SqlValue::Integer(t)] if *t <= now)) { return Err(DirectoryError::Storage); }
            Ok(())
        })
    }
}
