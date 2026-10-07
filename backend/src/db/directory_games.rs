//! Directory-local game SQL; no remote calls or configuration copies.
use super::*;
use crate::directory::games::*;
use brews_domain::{
    games::{GameCode, GameState},
    ids::{CommandId, GameId},
};

const HISTORY_PAGE_MAX: u32 = brews_contracts::history::HISTORY_MAX_LIMIT;
const SQL_SELECT_HISTORY_PAGE: &str = "SELECT game_id,designated_host_id,fingerprint,state,source_revision,game_code,created_at,started_at,ended_at,history_expires_at,publication_state FROM directory_game_index WHERE state IN (?,?) AND started_at IS NOT NULL AND history_expires_at>? AND (? IS NULL OR state=?) AND (? IS NULL OR ended_at<? OR (ended_at=? AND game_id<?)) ORDER BY ended_at DESC,game_id DESC LIMIT ?";
const CREATION_COLUMNS: &str = "account_id,command_id,game_id,fingerprint,created_at,deadline,next_retry_at,attempts,ready_revision,completed_at";
const INDEX_COLUMNS: &str = "game_id,designated_host_id,fingerprint,state,source_revision,game_code,created_at,started_at,ended_at,history_expires_at,publication_state";
const SQL_DELETE_EXPIRED_HISTORY_INDEX: &str =
    "DELETE FROM directory_game_index WHERE game_id=? AND history_expires_at=?";
impl<D: Database, R: Runtime> DirectoryService<'_, D, R> {
    /// One bounded page plus exact lookahead. Pending terminal metadata is eligible;
    /// no snapshots, boards or winner aliases are copied into Directory.
    pub fn history_indexes(
        &self,
        after: Option<(i64, GameId)>,
        limit: u32,
        outcome: Option<GameState>,
    ) -> Result<Vec<GameProjection>, DirectoryError> {
        if !(1..=HISTORY_PAGE_MAX).contains(&limit)
            || outcome.is_some_and(|s| !matches!(s, GameState::Resolved | GameState::Cancelled))
            || after.is_some_and(|(t, _)| !(1..=JS_SAFE_INTEGER_MAX).contains(&t))
        {
            return Err(DirectoryError::ProofMismatch);
        }
        self.transaction_before_compaction(|now| {
            let filter = outcome.map_or(SqlValue::Null, text);
            let time = after.map_or(SqlValue::Null, |(t, _)| integer(t));
            let id = after.map_or(SqlValue::Null, |(_, id)| text(id));
            self.db
                .query(
                    SQL_SELECT_HISTORY_PAGE,
                    &[
                        text(GameState::Resolved),
                        text(GameState::Cancelled),
                        integer(now),
                        filter.clone(),
                        filter,
                        time.clone(),
                        time.clone(),
                        time,
                        id,
                        integer(i64::from(limit) + 1),
                    ],
                )?
                .iter()
                .map(|r| parse_index(r, now).map(|(p, _)| p))
                .collect()
        })
    }
}
fn text(value: impl ToString) -> SqlValue {
    SqlValue::Text(value.to_string())
}
fn integer(value: i64) -> SqlValue {
    SqlValue::Integer(value)
}
fn deadline(at: i64, duration: i64) -> Result<i64, DirectoryError> {
    at.checked_add(duration)
        .filter(|v| *v <= JS_SAFE_INTEGER_MAX)
        .ok_or(DirectoryError::Storage)
}
fn optional_integer(v: Option<i64>) -> SqlValue {
    v.map_or(SqlValue::Null, integer)
}
fn parse_optional_integer(v: &SqlValue) -> Result<Option<i64>, DirectoryError> {
    match v {
        SqlValue::Null => Ok(None),
        SqlValue::Integer(n) => Ok(Some(*n)),
        _ => Err(DirectoryError::Storage),
    }
}
fn validate_projection(p: &GameProjection, now: i64) -> Result<(), DirectoryError> {
    let created = p.created_at();
    if !(1..=now).contains(&created) || p.source_revision() < 0 {
        return Err(DirectoryError::ProofMismatch);
    }
    if p.started_at()
        .is_some_and(|at| !(created..=now).contains(&at))
        || p.ended_at()
            .is_some_and(|at| !(p.started_at().unwrap_or(created)..=now).contains(&at))
        || p.history_expires_at()
            .is_some_and(|at| at > JS_SAFE_INTEGER_MAX || p.ended_at().is_none_or(|end| at <= end))
    {
        return Err(DirectoryError::ProofMismatch);
    }
    let valid = match p.state() {
        GameState::New => {
            p.started_at().is_none() && p.ended_at().is_none() && p.history_expires_at().is_none()
        }
        GameState::AwaitingPlayers => {
            p.game_code().is_some()
                && p.started_at().is_none()
                && p.ended_at().is_none()
                && p.history_expires_at().is_none()
        }
        GameState::InProgress => {
            p.game_code().is_some()
                && p.started_at().is_some()
                && p.ended_at().is_none()
                && p.history_expires_at().is_none()
        }
        GameState::Resolved => {
            p.game_code().is_some()
                && p.started_at().is_some()
                && p.ended_at().is_some()
                && p.history_expires_at().is_some()
        }
        GameState::Cancelled => {
            p.ended_at().is_some()
                && (if p.started_at().is_some() {
                    p.game_code().is_some() && p.history_expires_at().is_some()
                } else {
                    p.history_expires_at().is_none()
                })
        }
    };
    if !valid {
        return Err(DirectoryError::ProofMismatch);
    }
    Ok(())
}
fn parse_index(row: &crate::db::Row, now: i64) -> Result<(GameProjection, bool), DirectoryError> {
    let [
        SqlValue::Text(game),
        SqlValue::Text(host),
        SqlValue::Blob(fp),
        SqlValue::Text(state),
        SqlValue::Integer(revision),
        code,
        SqlValue::Integer(created),
        started,
        ended,
        expiry,
        SqlValue::Text(publication),
    ] = row.as_slice()
    else {
        return Err(DirectoryError::Storage);
    };
    let published = match publication.as_str() {
        PUBLICATION_PENDING => false,
        PUBLICATION_PUBLISHED => true,
        _ => return Err(DirectoryError::Storage),
    };
    let code = match code {
        SqlValue::Null => None,
        SqlValue::Text(code) => Some(code.parse().map_err(|_| DirectoryError::Storage)?),
        _ => return Err(DirectoryError::Storage),
    };
    let p = GameProjection::new(
        game.parse().map_err(|_| DirectoryError::Storage)?,
        host.parse().map_err(|_| DirectoryError::Storage)?,
        CreationFingerprint::from_digest(
            fp.as_slice()
                .try_into()
                .map_err(|_| DirectoryError::Storage)?,
        ),
        state.parse().map_err(|_| DirectoryError::Storage)?,
        SourceRevision::try_from(*revision).map_err(|_| DirectoryError::Storage)?,
        code,
        *created,
        parse_optional_integer(started)?,
        parse_optional_integer(ended)?,
        parse_optional_integer(expiry)?,
    );
    validate_projection(&p, now).map_err(|_| DirectoryError::Storage)?;
    Ok((p, published))
}
fn command_timestamp(command: CommandId) -> Result<i64, DirectoryError> {
    operation_timestamp(
        command
            .to_string()
            .parse()
            .map_err(|_| DirectoryError::Storage)?,
    )
}
fn parse_retired(
    row: &crate::db::Row,
    now: i64,
) -> Result<(AccountId, CommandId, i64), DirectoryError> {
    let [
        SqlValue::Text(account),
        SqlValue::Text(command),
        SqlValue::Blob(fp),
        SqlValue::Integer(completed),
    ] = row.as_slice()
    else {
        return Err(DirectoryError::Storage);
    };
    let account = account.parse().map_err(|_| DirectoryError::Storage)?;
    let command = command.parse().map_err(|_| DirectoryError::Storage)?;
    let issued = command_timestamp(command)?;
    if issued <= 0 || fp.len() != CREATION_FINGERPRINT_BYTES || !(issued..=now).contains(completed)
    {
        return Err(DirectoryError::Storage);
    }
    Ok((account, command, *completed))
}
fn parse_creation(row: &crate::db::Row, now: i64) -> Result<CreationWork, DirectoryError> {
    let [
        SqlValue::Text(account),
        SqlValue::Text(command),
        SqlValue::Text(game),
        SqlValue::Blob(fp),
        SqlValue::Integer(created),
        SqlValue::Integer(due),
        next,
        SqlValue::Integer(attempts),
        revision,
        completed,
    ] = row.as_slice()
    else {
        return Err(DirectoryError::Storage);
    };
    let account = account.parse().map_err(|_| DirectoryError::Storage)?;
    let command = command.parse().map_err(|_| DirectoryError::Storage)?;
    let game = game.parse().map_err(|_| DirectoryError::Storage)?;
    let fp = CreationFingerprint::from_digest(
        fp.as_slice()
            .try_into()
            .map_err(|_| DirectoryError::Storage)?,
    );
    let attempts = u32::try_from(*attempts).map_err(|_| DirectoryError::Storage)?;
    let (next, revision) = match (next, revision, completed) {
        (SqlValue::Integer(next), SqlValue::Null, SqlValue::Null)
            if *next >= *created && *next <= JS_SAFE_INTEGER_MAX =>
        {
            (Some(*next), None)
        }
        (next, SqlValue::Integer(revision), SqlValue::Integer(completed))
            if *revision >= 0 && (*created..=now).contains(completed) =>
        {
            let next = parse_optional_integer(next)?;
            if next.is_some_and(|at| at < *created || at > JS_SAFE_INTEGER_MAX) {
                return Err(DirectoryError::Storage);
            }
            (next, Some(*revision))
        }
        _ => return Err(DirectoryError::Storage),
    };
    let issued = command_timestamp(command)?;
    if issued <= 0
        || !(issued..=now).contains(created)
        || *due != deadline(*created, CREATION_LIFETIME_MS)?
    {
        return Err(DirectoryError::Storage);
    }
    Ok(CreationWork::new(
        account, command, game, fp, *created, *due, next, attempts, revision,
    ))
}
impl<D: Database, R: Runtime> DirectoryService<'_, D, R> {
    /// Accept only authenticated Game initializer evidence for this exact creation.
    pub fn acknowledge_creation(
        &self,
        proof: CreationReadyProof,
    ) -> Result<CreationAck, DirectoryError> {
        self.transaction(|now| self.acknowledge_creation_at(proof, now))
    }
    fn acknowledge_creation_at(
        &self,
        proof: CreationReadyProof,
        now: i64,
    ) -> Result<CreationAck, DirectoryError> {
        self.require_reserved(proof.game_id())?;
        let work = self
            .load_creation(proof.game_id(), now)?
            .ok_or(DirectoryError::UnknownGame)?;
        if proof.account_id() != work.account_id()
            || proof.command_id() != work.command_id()
            || proof.fingerprint() != work.fingerprint()
            || proof.created_at() != work.created_at()
            || proof.source_revision() != 0
        {
            return Err(DirectoryError::ProofMismatch);
        }
        if let Some(revision) = work.ready_revision() {
            if revision != proof.source_revision() {
                return Err(DirectoryError::ProofMismatch);
            }
            return Ok(CreationAck::new(proof));
        }
        if now >= work.deadline() {
            return Err(DirectoryError::Completed);
        }
        self.db.execute("UPDATE directory_game_creations SET ready_revision=?,completed_at=?,next_retry_at=NULL WHERE game_id=? AND ready_revision IS NULL", &[integer(proof.source_revision()),integer(now),text(proof.game_id())])?;
        Ok(CreationAck::new(proof))
    }
    pub fn creation_work(&self, game: GameId) -> Result<Option<CreationWork>, DirectoryError> {
        self.transaction(|now| self.load_creation(game, now))
    }
    fn load_creation(
        &self,
        game: GameId,
        now: i64,
    ) -> Result<Option<CreationWork>, DirectoryError> {
        let rows = self.db.query(
            &format!("SELECT {CREATION_COLUMNS} FROM directory_game_creations WHERE game_id=?"),
            &[text(game)],
        )?;
        match rows.as_slice() {
            [] => Ok(None),
            [row] => parse_creation(row, now).map(Some),
            _ => Err(DirectoryError::Storage),
        }
    }
    /// Allocate once under the exact ready creation; association is hidden until Game commits.
    pub fn allocate_game_code(&self, ack: CreationAck) -> Result<CodeGrant, DirectoryError> {
        self.transaction(|now| self.allocate_game_code_at(ack, now))
    }
    fn allocate_game_code_at(
        &self,
        ack: CreationAck,
        now: i64,
    ) -> Result<CodeGrant, DirectoryError> {
        self.require_reserved(ack.game_id())?;
        let work = self
            .load_creation(ack.game_id(), now)?
            .ok_or(DirectoryError::UnknownGame)?;
        let proof = ack.proof();
        if work.ready_revision() != Some(proof.source_revision())
            || work.account_id() != proof.account_id()
            || work.command_id() != proof.command_id()
            || work.fingerprint() != proof.fingerprint()
            || work.created_at() != proof.created_at()
        {
            return Err(DirectoryError::ProofMismatch);
        }
        let (stored, _) = self
            .load_index(ack.game_id(), now)?
            .ok_or(DirectoryError::UnknownGame)?;
        if let Some(code) = stored.game_code() {
            return Ok(CodeGrant::new(ack, code.clone()));
        }
        if stored.state() != GameState::New || stored.source_revision() != proof.source_revision() {
            return Err(DirectoryError::ProofMismatch);
        }
        for _ in 0..GAME_CODE_MAX_CANDIDATES {
            let code = self.generate_code()?;
            let observed = self.observe_clock()?;
            let collisions = self.db.query(
                &format!(
                    "SELECT {INDEX_COLUMNS} FROM directory_game_index WHERE game_code=? LIMIT 1"
                ),
                &[text(code.as_str())],
            )?;
            if let [row] = collisions.as_slice() {
                let (collision, _) = parse_index(row, observed)?;
                if collision
                    .history_expires_at()
                    .is_none_or(|expires| observed < expires)
                {
                    continue;
                }
                // Compare the old game identity, never delete by a now-reusable code.
                self.purge_history_index(&collision, observed)?;
            } else if !collisions.is_empty() {
                return Err(DirectoryError::Storage);
            }
            self.db.execute(
                "UPDATE directory_game_index SET game_code=? WHERE game_id=? AND game_code IS NULL",
                &[text(code.as_str()), text(ack.game_id())],
            )?;
            self.db.execute("UPDATE directory_game_creations SET next_retry_at=?,attempts=0 WHERE game_id=? AND ready_revision IS NOT NULL",&[integer(observed),text(ack.game_id())])?;
            if self.db.query(
                "SELECT game_code FROM directory_game_index WHERE game_id=?",
                &[text(ack.game_id())],
            )? != vec![vec![text(code.as_str())]]
                || self.load_creation(ack.game_id(), observed)?
                    != Some(CreationWork::new(
                        work.account_id(),
                        work.command_id(),
                        work.game_id(),
                        work.fingerprint(),
                        work.created_at(),
                        work.deadline(),
                        Some(observed),
                        0,
                        work.ready_revision(),
                    ))
            {
                return Err(DirectoryError::Storage);
            }
            return Ok(CodeGrant::new(ack, code));
        }
        Err(DirectoryError::CodeExhausted)
    }
    /// Only authenticated Game revision evidence can cross the initial publication barrier.
    pub fn publish_game(
        &self,
        projection: GameProjection,
    ) -> Result<ProjectionAck, DirectoryError> {
        self.transaction(|now| self.publish_game_at(projection, now))
    }
    fn publish_game_at(
        &self,
        projection: GameProjection,
        now: i64,
    ) -> Result<ProjectionAck, DirectoryError> {
        validate_projection(&projection, now)?;
        let (stored, published) = self
            .load_index(projection.game_id(), now)?
            .ok_or(DirectoryError::UnknownGame)?;
        if stored.fingerprint() != projection.fingerprint()
            || stored.created_at() != projection.created_at()
        {
            return Err(DirectoryError::ProofMismatch);
        }
        if projection.source_revision() < stored.source_revision() {
            return Ok(ProjectionAck::new(
                stored.game_id(),
                stored.source_revision(),
                published,
            ));
        }
        if projection.source_revision() == stored.source_revision() {
            if projection != stored {
                return Err(DirectoryError::ProofMismatch);
            }
            return Ok(ProjectionAck::new(
                stored.game_id(),
                stored.source_revision(),
                published,
            ));
        }
        if matches!(stored.state(), GameState::Cancelled | GameState::Resolved) {
            return Err(DirectoryError::ProofMismatch);
        }
        self.require_reserved(projection.game_id())?;
        if stored.game_code() != projection.game_code() {
            return Err(DirectoryError::ProofMismatch);
        }
        if !published {
            let work = self
                .load_creation(projection.game_id(), now)?
                .ok_or(DirectoryError::UnknownGame)?;
            if work.ready_revision().is_none()
                || projection.state() != GameState::AwaitingPlayers
                || projection.game_code().is_none()
            {
                return Err(DirectoryError::ProofMismatch);
            }
        }
        if stored.designated_host_id() != projection.designated_host_id() {
            self.validate_account_gate(projection.designated_host_id(), now)?;
            if !self
                .db
                .query(
                    "SELECT account_id FROM account_assignment_gates WHERE account_id=?",
                    &[text(projection.designated_host_id())],
                )?
                .is_empty()
            {
                return Err(DirectoryError::AssignmentBlocked);
            }
        }
        self.db.execute("UPDATE directory_game_index SET designated_host_id=?,state=?,source_revision=?,publication_state=?,started_at=?,ended_at=?,history_expires_at=? WHERE game_id=?", &[text(projection.designated_host_id()),text(projection.state()),integer(projection.source_revision()),text(PUBLICATION_PUBLISHED),optional_integer(projection.started_at()),optional_integer(projection.ended_at()),optional_integer(projection.history_expires_at()),text(projection.game_id())])?;
        if matches!(
            projection.state(),
            GameState::New | GameState::AwaitingPlayers | GameState::InProgress
        ) {
            self.db.execute("UPDATE directory_hosted_nonterminal_games SET designated_host_id=? WHERE game_id=?", &[text(projection.designated_host_id()),text(projection.game_id())])?;
        }
        self.db.execute("UPDATE directory_game_creations SET next_retry_at=NULL WHERE game_id=? AND ready_revision IS NOT NULL",&[text(projection.game_id())])?;
        if self.load_index(projection.game_id(), now)? != Some((projection.clone(), true))
            || self
                .load_creation(projection.game_id(), now)?
                .is_some_and(|work| {
                    work.ready_revision().is_some() && work.next_retry_at().is_some()
                })
        {
            return Err(DirectoryError::Storage);
        }
        if matches!(
            projection.state(),
            GameState::New | GameState::AwaitingPlayers | GameState::InProgress
        ) && self.db.query(
            "SELECT designated_host_id FROM directory_hosted_nonterminal_games WHERE game_id=?",
            &[text(projection.game_id())],
        )? != vec![vec![text(projection.designated_host_id())]]
        {
            return Err(DirectoryError::Storage);
        }
        Ok(ProjectionAck::new(
            projection.game_id(),
            projection.source_revision(),
            true,
        ))
    }
    /// Owner-only read of the existing association, including hidden pending codes.
    pub fn game_code(&self, game: GameId) -> Result<Option<GameCode>, DirectoryError> {
        self.transaction(|now| {
            Ok(self
                .load_index(game, now)?
                .filter(|(p, _)| p.history_expires_at().is_none_or(|expires| now < expires))
                .and_then(|(p, _)| p.game_code().cloned()))
        })
    }
    /// Read only published live-code associations; unknown codes never initialize Game.
    pub fn lookup_game_code(&self, code: &GameCode) -> Result<Option<GameId>, DirectoryError> {
        self.transaction(|now| self.lookup_game_code_at(code, now))
    }
    fn lookup_game_code_at(
        &self,
        code: &GameCode,
        now: i64,
    ) -> Result<Option<GameId>, DirectoryError> {
        let rows=self.db.query("SELECT game_id FROM directory_game_index WHERE game_code=? AND publication_state=? AND state IN (?,?) LIMIT 1", &[text(code.as_str()),text(PUBLICATION_PUBLISHED),text(GameState::AwaitingPlayers),text(GameState::InProgress)])?;
        match rows.as_slice() {
            [] => Ok(None),
            [row] => match row.as_slice() {
                [SqlValue::Text(game)] => {
                    let game = game.parse().map_err(|_| DirectoryError::Storage)?;
                    let (projection, published) =
                        self.load_index(game, now)?.ok_or(DirectoryError::Storage)?;
                    if !published || projection.game_code() != Some(code) {
                        return Err(DirectoryError::Storage);
                    }
                    Ok(Some(game))
                }
                _ => Err(DirectoryError::Storage),
            },
            _ => Err(DirectoryError::Storage),
        }
    }
    fn generate_code(&self) -> Result<GameCode, DirectoryError> {
        const ALPHABET: &[u8; 36] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
        const UNBIASED_BYTE_LIMIT: u8 = 252;
        let mut code = [0; 8];
        let mut len = 0;
        for _ in 0..GAME_CODE_ENTROPY_BATCHES {
            let mut random = [0; 32];
            self.runtime
                .fill_random(&mut random)
                .map_err(|_| DirectoryError::Entropy)?;
            for byte in random {
                if byte >= UNBIASED_BYTE_LIMIT {
                    continue;
                }
                code[len] = ALPHABET[usize::from(byte % 36)];
                len += 1;
                if len == code.len() {
                    return std::str::from_utf8(&code)
                        .map_err(|_| DirectoryError::Storage)?
                        .parse()
                        .map_err(|_| DirectoryError::Storage);
                }
            }
        }
        Err(DirectoryError::CodeExhausted)
    }
    fn load_index(
        &self,
        game: GameId,
        now: i64,
    ) -> Result<Option<(GameProjection, bool)>, DirectoryError> {
        let rows = self.db.query(
            &format!("SELECT {INDEX_COLUMNS} FROM directory_game_index WHERE game_id=?"),
            &[text(game)],
        )?;
        match rows.as_slice() {
            [] => Ok(None),
            [row] => parse_index(row, now).map(Some),
            _ => Err(DirectoryError::Storage),
        }
    }
    pub fn confirm_reservation(&self, game: GameId) -> Result<ReservationProof, DirectoryError> {
        self.transaction(|now| {
            self.require_reserved(game)?;
            let (p, _) = self
                .load_index(game, now)?
                .ok_or(DirectoryError::UnknownGame)?;
            if !matches!(
                p.state(),
                GameState::New | GameState::AwaitingPlayers | GameState::InProgress
            ) {
                return Err(DirectoryError::Completed);
            }
            Ok(ReservationProof::new(game))
        })
    }
    /// Requires trusted Game's committed terminal evidence, never a local timeout guess.
    /// Reuse the original proof to purge at its fixed History expiry; absent indexes stay absent.
    /// A delayed old Game proof cannot clear a new reservation or code association.
    pub fn release_game(&self, proof: TerminalProof) -> Result<ReleaseAck, DirectoryError> {
        self.transaction_before_compaction(|now| {
            // Validate the exact target before expiry compaction can erase its evidence.
            let ack = self.release_game_at(proof, now)?;
            self.compact_completions(now)?;
            Ok(ack)
        })
    }
    fn release_game_at(
        &self,
        proof: TerminalProof,
        now: i64,
    ) -> Result<ReleaseAck, DirectoryError> {
        let p = proof.projection();
        validate_projection(p, now)?;
        if !matches!(p.state(), GameState::Cancelled | GameState::Resolved) {
            return Err(DirectoryError::ProofMismatch);
        }
        let index_before = self.load_index(p.game_id(), now)?;
        let retain_index = index_before.is_some()
            && p.started_at().is_some()
            && p.history_expires_at().is_some_and(|expires| now < expires);
        if let Some((stored, published)) = index_before {
            let creation = self.load_creation(p.game_id(), now)?;
            // A hidden allocation may not have reached the Game before cancellation.
            let uncommitted_code = !published
                && stored.state() == GameState::New
                && stored.source_revision() == 0
                && stored.game_code().is_some()
                && p.game_code().is_none()
                && p.state() == GameState::Cancelled
                && p.started_at().is_none()
                && p.history_expires_at().is_none()
                && creation.is_some_and(|work| work.ready_revision() == Some(0));
            if matches!(stored.state(), GameState::Cancelled | GameState::Resolved) && stored != *p
            {
                return Err(DirectoryError::ProofMismatch);
            }
            if stored.fingerprint() != p.fingerprint()
                || stored.created_at() != p.created_at()
                || stored.designated_host_id() != p.designated_host_id()
                || (stored.game_code() != p.game_code() && !uncommitted_code)
                || p.source_revision() < stored.source_revision()
                || (p.source_revision() == stored.source_revision() && *p != stored)
            {
                return Err(DirectoryError::ProofMismatch);
            }
            if let Some(work) = creation {
                let retired = [
                    text(work.account_id()),
                    text(work.command_id()),
                    SqlValue::Blob(work.fingerprint().digest().to_vec()),
                    integer(now),
                ];
                self.db.execute("INSERT INTO directory_retired_creations(account_id,command_id,fingerprint,completed_at) VALUES(?,?,?,?)", &retired)?;
                if self.db.query("SELECT account_id,command_id,fingerprint,completed_at FROM directory_retired_creations WHERE account_id=? AND command_id=?", &[text(work.account_id()),text(work.command_id())])? != vec![retired.to_vec()] {
                    return Err(DirectoryError::Storage);
                }
                self.db.execute(
                    "DELETE FROM directory_game_creations WHERE game_id=?",
                    &[text(p.game_id())],
                )?;
            }
            if p.started_at().is_none()
                || p.history_expires_at().is_some_and(|expires| now >= expires)
            {
                self.db.execute(
                    "DELETE FROM directory_game_index WHERE game_id=?",
                    &[text(p.game_id())],
                )?;
            } else if stored != *p {
                self.db.execute("UPDATE directory_game_index SET state=?,source_revision=?,publication_state=?,started_at=?,ended_at=?,history_expires_at=? WHERE game_id=?", &[text(p.state()),integer(p.source_revision()),text(if published {PUBLICATION_PUBLISHED}else{PUBLICATION_PENDING}),optional_integer(p.started_at()),optional_integer(p.ended_at()),optional_integer(p.history_expires_at()),text(p.game_id())])?;
            }
            self.db.execute(
                "DELETE FROM directory_hosted_nonterminal_games WHERE game_id=?",
                &[text(p.game_id())],
            )?;
        } else if self.reserved_game()? == Some(p.game_id()) {
            return Err(DirectoryError::Storage);
        }
        self.db.execute(
            "UPDATE directory_global_reservation SET game_id=NULL WHERE game_id=?",
            &[text(p.game_id())],
        )?;
        if self.reserved_game()? == Some(p.game_id())
            || self.load_creation(p.game_id(), now)?.is_some()
            || !self
                .db
                .query(
                    "SELECT game_id FROM directory_hosted_nonterminal_games WHERE game_id=?",
                    &[text(p.game_id())],
                )?
                .is_empty()
        {
            return Err(DirectoryError::Storage);
        }
        match self.load_index(p.game_id(), now)? {
            None if !retain_index => {}
            Some((stored, _)) if retain_index && stored == *p => {}
            _ => return Err(DirectoryError::Storage),
        }
        Ok(ReleaseAck::new(p.game_id()))
    }
    /// Bounded known-ID initializer/publication work. Only unready creation uses this deadline.
    /// Ready publication probes Game authority; never substitute the original deadline for Game idle time.
    pub fn due_creation_work(&self, limit: u32) -> Result<Vec<CreationWork>, DirectoryError> {
        if limit == 0 || i64::from(limit) > CLEANUP_BATCH_SIZE {
            return Err(DirectoryError::Storage);
        }
        self.transaction(|now| self.due_creation_work_at(limit, now))
    }
    fn due_creation_work_at(
        &self,
        limit: u32,
        now: i64,
    ) -> Result<Vec<CreationWork>, DirectoryError> {
        let rows=self.db.query(&format!("SELECT {CREATION_COLUMNS} FROM directory_game_creations WHERE next_retry_at<=? ORDER BY next_retry_at,game_id LIMIT ?"), &[integer(now),integer(i64::from(limit))])?;
        rows.iter().map(|row| parse_creation(row, now)).collect()
    }
    /// Fence delayed retry snapshots; durable ready/cancelled state always wins.
    pub fn retry_creation(&self, snapshot: CreationWork) -> Result<CreationWork, DirectoryError> {
        self.transaction(|now| self.retry_creation_at(snapshot, now))
    }
    fn retry_creation_at(
        &self,
        snapshot: CreationWork,
        now: i64,
    ) -> Result<CreationWork, DirectoryError> {
        let current = self
            .load_creation(snapshot.game_id(), now)?
            .ok_or(DirectoryError::UnknownGame)?;
        if current.account_id() != snapshot.account_id()
            || current.command_id() != snapshot.command_id()
            || current.fingerprint() != snapshot.fingerprint()
            || current.created_at() != snapshot.created_at()
        {
            return Err(DirectoryError::ProofMismatch);
        }
        if current != snapshot
            || current.next_retry_at().is_none()
            || current.next_retry_at().is_some_and(|at| at > now)
        {
            return Ok(current);
        }
        self.require_reserved(current.game_id())?;
        let delay =
            (CREATION_RETRY_INITIAL_MS << current.attempts().min(9)).min(CREATION_RETRY_MAX_MS);
        let mut next = deadline(now, delay)?;
        if current.ready_revision().is_none() && now < current.deadline() {
            next = next.min(current.deadline());
        }
        let attempts = current.attempts().saturating_add(1);
        self.db.execute("UPDATE directory_game_creations SET next_retry_at=?,attempts=? WHERE game_id=? AND next_retry_at IS NOT NULL", &[integer(next),integer(i64::from(attempts)),text(current.game_id())])?;
        Ok(CreationWork::new(
            current.account_id(),
            current.command_id(),
            current.game_id(),
            current.fingerprint(),
            current.created_at(),
            current.deadline(),
            Some(next),
            attempts,
            current.ready_revision(),
        ))
    }
    pub(super) fn next_creation_deadline(&self, now: i64) -> Result<Option<i64>, DirectoryError> {
        let mut next = None;
        for select in [
            format!(
                "SELECT {CREATION_COLUMNS} FROM directory_game_creations WHERE next_retry_at IS NOT NULL ORDER BY next_retry_at,game_id LIMIT 1"
            ),
            format!(
                "SELECT {CREATION_COLUMNS} FROM directory_game_creations WHERE ready_revision IS NOT NULL AND next_retry_at IS NULL AND NOT EXISTS(SELECT 1 FROM directory_game_index AS i WHERE i.game_id=directory_game_creations.game_id AND i.publication_state=? AND i.game_code IS NOT NULL) ORDER BY completed_at,game_id LIMIT 1"
            ),
        ] {
            let parameters = if select.contains("publication_state=?") {
                vec![text(PUBLICATION_PENDING)]
            } else {
                Vec::new()
            };
            let rows = self.db.query(&select, &parameters)?;
            for row in rows {
                let work = parse_creation(&row, now)?;
                let at = if let Some(at) = work.next_retry_at() {
                    at
                } else {
                    let SqlValue::Integer(completed) = row[9] else {
                        return Err(DirectoryError::Storage);
                    };
                    deadline(completed, COMMAND_RECEIPT_RETENTION_MS)?
                };
                next = Some(next.map_or(at, |n: i64| n.min(at)));
            }
        }
        for row in self.db.query("SELECT account_id,command_id,fingerprint,completed_at FROM directory_retired_creations ORDER BY completed_at,command_id LIMIT 1",&[])? {
            let (_,_,completed)=parse_retired(&row,now)?;
            let at=deadline(completed,COMMAND_RECEIPT_RETENTION_MS)?;
            next=Some(next.map_or(at,|n:i64|n.min(at)));
        }
        Ok(next)
    }
    pub(super) fn next_history_deadline(&self, now: i64) -> Result<Option<i64>, DirectoryError> {
        let rows = self.db.query(
            &format!("SELECT {INDEX_COLUMNS} FROM directory_game_index WHERE history_expires_at IS NOT NULL ORDER BY history_expires_at,game_id LIMIT 1"),
            &[],
        )?;
        match rows.as_slice() {
            [] => Ok(None),
            [row] => parse_index(row, now)?
                .0
                .history_expires_at()
                .ok_or(DirectoryError::Storage)
                .map(Some),
            _ => Err(DirectoryError::Storage),
        }
    }
    pub(super) fn compact_history_indexes(&self, now: i64) -> Result<(), DirectoryError> {
        let rows = self.db.query(
            &format!("SELECT {INDEX_COLUMNS} FROM directory_game_index WHERE history_expires_at<=? ORDER BY history_expires_at,game_id LIMIT ?"),
            &[integer(now), integer(CLEANUP_BATCH_SIZE)],
        )?;
        for row in rows {
            let (projection, _) = parse_index(&row, now)?;
            self.purge_history_index(&projection, now)?;
        }
        Ok(())
    }
    fn purge_history_index(
        &self,
        projection: &GameProjection,
        now: i64,
    ) -> Result<(), DirectoryError> {
        let expires = projection
            .history_expires_at()
            .filter(|expires| now >= *expires)
            .ok_or(DirectoryError::Storage)?;
        if self.reserved_game()? == Some(projection.game_id())
            || self.load_creation(projection.game_id(), now)?.is_some()
            || !self
                .db
                .query(
                    "SELECT game_id FROM directory_hosted_nonterminal_games WHERE game_id=?",
                    &[text(projection.game_id())],
                )?
                .is_empty()
        {
            // An unacknowledged terminal release is still required durable work.
            return Err(DirectoryError::Storage);
        }
        self.db.execute(
            SQL_DELETE_EXPIRED_HISTORY_INDEX,
            &[text(projection.game_id()), integer(expires)],
        )?;
        if self.load_index(projection.game_id(), now)?.is_some() {
            return Err(DirectoryError::Storage);
        }
        Ok(())
    }
    pub(super) fn compact_creations(&self, now: i64) -> Result<(), DirectoryError> {
        let floor = self.command_floor()?;
        for row in self.db.query(&format!("SELECT {CREATION_COLUMNS} FROM directory_game_creations WHERE ready_revision IS NOT NULL AND next_retry_at IS NULL AND NOT EXISTS(SELECT 1 FROM directory_game_index AS i WHERE i.game_id=directory_game_creations.game_id AND i.publication_state=? AND i.game_code IS NOT NULL) AND completed_at<=? ORDER BY completed_at,game_id LIMIT ?"),&[text(PUBLICATION_PENDING),integer(floor),integer(CLEANUP_BATCH_SIZE)])? {
            let work=parse_creation(&row,now)?;
            self.db.execute("DELETE FROM directory_game_creations WHERE game_id=? AND ready_revision IS NOT NULL",&[text(work.game_id())])?;
        }
        for row in self.db.query("SELECT account_id,command_id,fingerprint,completed_at FROM directory_retired_creations WHERE completed_at<=? ORDER BY completed_at,command_id LIMIT ?",&[integer(floor),integer(CLEANUP_BATCH_SIZE)])? {
            let (account,command,_)=parse_retired(&row,now)?;
            self.db.execute("DELETE FROM directory_retired_creations WHERE account_id=? AND command_id=?",&[text(account),text(command)])?;
        }
        Ok(())
    }
    /// Server-derived creator and canonical effective-config digest only. Never config/credentials.
    pub fn claim_game(
        &self,
        account: AccountId,
        command: CommandId,
        fingerprint: CreationFingerprint,
    ) -> Result<CreationWork, DirectoryError> {
        self.transaction(|now| self.claim_game_at(account, command, fingerprint, now))
    }
    fn claim_game_at(
        &self,
        account: AccountId,
        command: CommandId,
        fingerprint: CreationFingerprint,
        now: i64,
    ) -> Result<CreationWork, DirectoryError> {
        let select = format!(
            "SELECT {CREATION_COLUMNS} FROM directory_game_creations WHERE account_id=? AND command_id=?"
        );
        let rows = self.db.query(&select, &[text(account), text(command)])?;
        if let [row] = rows.as_slice() {
            let work = parse_creation(row, now)?;
            if work.fingerprint() != fingerprint {
                return Err(DirectoryError::CommandConflict);
            }
            self.require_reserved(work.game_id())?;
            return Ok(work);
        }
        if !rows.is_empty() {
            return Err(DirectoryError::Storage);
        }
        let retired=self.db.query("SELECT fingerprint,completed_at FROM directory_retired_creations WHERE account_id=? AND command_id=?", &[text(account),text(command)])?;
        if let [row] = retired.as_slice() {
            let [SqlValue::Blob(fp), SqlValue::Integer(completed)] = row.as_slice() else {
                return Err(DirectoryError::Storage);
            };
            if fp.len() != CREATION_FINGERPRINT_BYTES
                || !(command_timestamp(command)?..=now).contains(completed)
            {
                return Err(DirectoryError::Storage);
            }
            if fp.as_slice() != fingerprint.digest() {
                return Err(DirectoryError::CommandConflict);
            }
            return Err(DirectoryError::Completed);
        }
        if !retired.is_empty() {
            return Err(DirectoryError::Storage);
        }
        let issued = command_timestamp(command)?;
        if issued <= self.command_floor()? || issued > now {
            return Err(DirectoryError::StaleOperation);
        }
        self.validate_account_gate(account, now)?;
        if !self
            .db
            .query(
                "SELECT account_id FROM account_assignment_gates WHERE account_id=?",
                &[text(account)],
            )?
            .is_empty()
        {
            return Err(DirectoryError::AssignmentBlocked);
        }
        if self.reserved_game()?.is_some()
            || !self
                .db
                .query(
                    "SELECT game_id FROM directory_hosted_nonterminal_games LIMIT 1",
                    &[],
                )?
                .is_empty()
        {
            return Err(DirectoryError::ReservationOccupied);
        }
        let mut bytes = [0; 16];
        self.runtime
            .fill_random(&mut bytes)
            .map_err(|_| DirectoryError::Entropy)?;
        let created = self.observe_clock()?;
        if issued <= self.command_floor()? || issued > created {
            return Err(DirectoryError::StaleOperation);
        }
        if created >= (1_i64 << 48) {
            return Err(DirectoryError::Clock);
        }
        bytes[..6].copy_from_slice(&created.to_be_bytes()[2..]);
        bytes[6] = (bytes[6] & 0x0f) | 0x70;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        let game =
            GameId::try_from(uuid::Uuid::from_bytes(bytes)).map_err(|_| DirectoryError::Storage)?;
        let due = deadline(created, CREATION_LIFETIME_MS)?;
        self.db.execute(
            "UPDATE directory_global_reservation SET game_id=? WHERE game_id IS NULL",
            &[text(game)],
        )?;
        self.require_reserved(game)?;
        self.db.execute("INSERT INTO directory_hosted_nonterminal_games(game_id,designated_host_id) VALUES(?,?)", &[text(game),text(account)])?;
        self.db.execute("INSERT INTO directory_game_index(game_id,game_code,designated_host_id,state,source_revision,publication_state,created_at,fingerprint) VALUES(?,NULL,?,?,0,?,?,?)", &[text(game),text(account),text(GameState::New),text(PUBLICATION_PENDING),integer(created),SqlValue::Blob(fingerprint.digest().to_vec())])?;
        self.db.execute("INSERT INTO directory_game_creations(account_id,command_id,game_id,fingerprint,created_at,deadline,next_retry_at,attempts,ready_revision,completed_at) VALUES(?,?,?,?,?,?,?,0,NULL,NULL)", &[text(account),text(command),text(game),SqlValue::Blob(fingerprint.digest().to_vec()),integer(created),integer(due),integer(created)])?;
        Ok(CreationWork::new(
            account,
            command,
            game,
            fingerprint,
            created,
            due,
            Some(created),
            0,
            None,
        ))
    }
    fn reserved_game(&self) -> Result<Option<GameId>, DirectoryError> {
        let rows = self
            .db
            .query("SELECT game_id FROM directory_global_reservation", &[])?;
        match rows.as_slice() {
            [row] => match row.as_slice() {
                [SqlValue::Null] => Ok(None),
                [SqlValue::Text(game)] => {
                    game.parse().map(Some).map_err(|_| DirectoryError::Storage)
                }
                _ => Err(DirectoryError::Storage),
            },
            _ => Err(DirectoryError::Storage),
        }
    }
    fn require_reserved(&self, game: GameId) -> Result<(), DirectoryError> {
        if self.reserved_game()? != Some(game) {
            return Err(DirectoryError::ProofMismatch);
        }
        Ok(())
    }
}
