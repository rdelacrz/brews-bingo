//! Allocation-conscious private proof, failure buckets and epoch-close coordination.
use super::*;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use zeroize::{Zeroize as _, Zeroizing};

use crate::limits::{RATE_LIMIT_BLOCK_MS, RATE_LIMIT_MAX_FAILURES, RATE_LIMIT_WINDOW_MS};
const SQL_PROOF: &str = "SELECT p.player_id,p.session_epoch,r.answer_verifier,r.normalization_version,r.updated_at FROM game_players p JOIN game_recovery r ON r.player_id=p.player_id WHERE p.alias=?";
const SQL_BUCKET: &str = "SELECT window_started_at,attempt_count,blocked_until,expires_at FROM game_recovery_buckets WHERE subject_key=?";
const SQL_SET_BUCKET: &str = "INSERT INTO game_recovery_buckets VALUES(?,?,?,?,?) ON CONFLICT(subject_key) DO UPDATE SET window_started_at=excluded.window_started_at,attempt_count=excluded.attempt_count,blocked_until=excluded.blocked_until,expires_at=excluded.expires_at";
const SQL_CLEAR_BUCKET: &str = "DELETE FROM game_recovery_buckets WHERE subject_key=?";
const SQL_CLOSES: &str = "SELECT connection_id,session_id,player_id,epoch,next_attempt_at,attempt_count FROM game_socket_closures ORDER BY next_attempt_at,connection_id LIMIT 100";

struct PrivateRows(Vec<super::super::Row>);
impl std::ops::Deref for PrivateRows {
    type Target = Vec<super::super::Row>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl PartialEq for PrivateRows {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl Drop for PrivateRows {
    fn drop(&mut self) {
        for row in &mut self.0 {
            for value in row {
                if let SqlValue::Text(value) = value {
                    value.zeroize();
                }
            }
        }
    }
}
// No Debug: verifier snapshots are private even during failed optimistic checks.
#[derive(Clone, PartialEq, Eq)]
struct Proof {
    player: PlayerId,
    epoch: i64,
    verifier: Zeroizing<String>,
    version: i64,
    updated: i64,
}
pub struct RecoveryOutcome {
    proof: Proof,
    alias: String,
    code: GameCode,
    pub receipt: GameReceipt,
    pub cookie: Option<SecretCookie>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoverySocketClose {
    pub connection: ConnectionId,
    pub session: SessionId,
    pub player: PlayerId,
    pub epoch: i64,
    pub next_attempt_at: i64,
    pub attempt_count: u32,
}
fn keyed(key: &[u8], domain: &[u8], parts: &[&[u8]]) -> Result<[u8; 32], GameError> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).map_err(|_| GameError::Storage)?;
    mac.update(domain);
    for part in parts {
        mac.update(&(part.len() as u64).to_be_bytes());
        mac.update(part);
    }
    Ok(mac.finalize().into_bytes().into())
}
impl<D: Database, R: Runtime> GameService<'_, D, R> {
    fn recovery_live(&self, code: Option<&GameCode>, now: i64) -> Result<Record, GameError> {
        let r = self.record_optional()?.ok_or(GameError::InvalidRecovery)?;
        if !matches!(r.state, GameState::AwaitingPlayers | GameState::InProgress)
            || r.published.is_none()
            || code.is_some_and(|code| r.code.as_ref() != Some(code))
            || r.due.is_some_and(|due| due <= now)
        {
            return Err(GameError::InvalidRecovery);
        }
        Ok(r)
    }
    fn recovery_proof(&self, alias: &str) -> Result<Option<Proof>, GameError> {
        let rows = PrivateRows(self.db.query(SQL_PROOF, &[text(alias)])?);
        rows.first()
            .map(|row| {
                Ok(Proof {
                    player: string(row, 0)?.parse().map_err(|_| GameError::Storage)?,
                    epoch: number(row, 1)?,
                    verifier: Zeroizing::new(string(row, 2)?.to_owned()),
                    version: number(row, 3)?,
                    updated: number(row, 4)?,
                })
            })
            .transpose()
    }
    fn recovery_keys(
        &self,
        game: GameId,
        alias: &str,
        caller: &str,
    ) -> Result<[[u8; 32]; 3], GameError> {
        // This metadata comes only from the edge's Cloudflare IP parser.
        if caller.len() > 45 || caller.parse::<std::net::IpAddr>().is_err() {
            return Err(GameError::Storage);
        }
        let game = game.to_string();
        Ok([
            keyed(
                self.rate_key,
                b"brews-recovery-subject\0",
                &[game.as_bytes(), alias.as_bytes()],
            )?,
            keyed(
                self.rate_key,
                b"brews-recovery-caller\0",
                &[game.as_bytes(), caller.as_bytes()],
            )?,
            keyed(self.rate_key, b"brews-recovery-game\0", &[game.as_bytes()])?,
        ])
    }
    fn recovery_rate_check(&self, keys: &[[u8; 32]; 3], now: i64) -> Result<(), GameError> {
        for key in keys {
            let rows = self.db.query(SQL_BUCKET, &[SqlValue::Blob(key.to_vec())])?;
            if let Some(row) = rows.first() {
                if number(row, 0)? > now {
                    return Err(GameError::Storage);
                }
                if optional_number(row, 2)?.is_some_and(|until| until > now) {
                    return Err(GameError::RecoveryBlocked);
                }
            }
        }
        Ok(())
    }
    fn recovery_failure(&self, keys: &[[u8; 32]; 3], now: i64) -> Result<(), GameError> {
        self.recovery_rate_check(keys, now)?;
        for key in keys {
            let binding = SqlValue::Blob(key.to_vec());
            let rows = self.db.query(SQL_BUCKET, std::slice::from_ref(&binding))?;
            let (start, attempts) = match rows.first() {
                Some(row) if number(row, 3)? > now => (number(row, 0)?, number(row, 1)? + 1),
                _ => (now, 1),
            };
            let blocked = (attempts == RATE_LIMIT_MAX_FAILURES)
                .then(|| add_time(now, RATE_LIMIT_BLOCK_MS))
                .transpose()?;
            let expires = blocked.unwrap_or(add_time(start, RATE_LIMIT_WINDOW_MS)?);
            let expected = vec![
                int(start),
                int(attempts),
                blocked.map_or(SqlValue::Null, int),
                int(expires),
            ];
            self.db.execute(
                SQL_SET_BUCKET,
                &[
                    binding.clone(),
                    expected[0].clone(),
                    expected[1].clone(),
                    expected[2].clone(),
                    expected[3].clone(),
                ],
            )?;
            if self.db.query(SQL_BUCKET, &[binding])? != vec![expected] {
                return Err(GameError::Storage);
            }
        }
        Ok(())
    }
    fn clear_recovery_rates(&self, keys: &[[u8; 32]; 3]) -> Result<(), GameError> {
        for key in keys {
            let binding = [SqlValue::Blob(key.to_vec())];
            self.db.execute(SQL_CLEAR_BUCKET, &binding)?;
            if !self.db.query(SQL_BUCKET, &binding)?.is_empty() {
                return Err(GameError::Storage);
            }
        }
        Ok(())
    }
    pub(super) fn cleanup_recovery_buckets(&self, now: i64) -> Result<(), GameError> {
        let rows = self.db.query("SELECT subject_key FROM game_recovery_buckets WHERE expires_at<=? ORDER BY expires_at LIMIT ?", &[int(now), int(crate::limits::CLEANUP_BATCH_SIZE)])?;
        for row in rows {
            let binding = [row.first().ok_or(GameError::Storage)?.clone()];
            self.db.execute(SQL_CLEAR_BUCKET, &binding)?;
            if !self.db.query(SQL_BUCKET, &binding)?.is_empty() {
                return Err(GameError::Storage);
            }
        }
        Ok(())
    }
    /// Every attempt proves the current answer, including an exact receipt retry.
    pub fn recover_player(
        &self,
        command: CommandId,
        code: &GameCode,
        alias: &str,
        answer: &str,
        caller: &str,
    ) -> Result<RecoveryOutcome, GameError> {
        let alias =
            brews_domain::games::validate_alias(alias).map_err(|_| GameError::InvalidRecovery)?;
        let (proof, keys) = self.maintenance(|now| {
            let r = self.recovery_live(Some(code), now)?;
            let keys = self.recovery_keys(r.id, alias, caller)?;
            self.recovery_rate_check(&keys, now)?;
            Ok((self.recovery_proof(alias)?, keys))
        })?;
        if let Some(proof) = &proof {
            if proof.version != crate::game::recovery_answers::NORMALIZATION_VERSION {
                return Err(GameError::Storage);
            }
            // Stored profile and PHC metadata precede canonicalization/KDF.
            crate::security::bounded_phc(&proof.verifier).map_err(|_| GameError::Storage)?;
        }
        let normalized = crate::game::recovery_answers::normalize(answer);
        let valid = match (&proof, &normalized) {
            (Some(proof), Ok(answer)) => crate::security::verify_password(answer, &proof.verifier)
                .map_err(|_| GameError::Storage)?,
            _ => false,
        };
        self.runtime.player_recovery_proof_complete();
        let fp = keyed(
            self.rate_key,
            b"brews-player-recovery-command\0",
            &[
                code.as_str().as_bytes(),
                alias.as_bytes(),
                answer.as_bytes(),
            ],
        )?;
        let authorized = self.maintenance(|now| {
            self.recovery_live(Some(code), now)?;
            self.recovery_rate_check(&keys, now)?;
            if self.recovery_proof(alias)? != proof {
                return Err(GameError::InvalidRecovery);
            }
            if !valid {
                self.recovery_failure(&keys, now)?;
                return Err(GameError::InvalidRecovery);
            }
            let proof = proof.as_ref().ok_or(GameError::Storage)?;
            let receipt = self.receipt(&player_actor(proof.player), command, fp, now)?;
            if receipt.is_none() {
                self.admit(command, now)?;
            }
            if receipt.is_some() {
                self.clear_recovery_rates(&keys)?;
            }
            Ok(receipt)
        })?;
        if let Some(receipt) = authorized {
            return Ok(RecoveryOutcome {
                proof: proof.ok_or(GameError::Storage)?,
                alias: alias.to_owned(),
                code: code.clone(),
                receipt,
                cookie: None,
            });
        }
        let proof = proof.ok_or(GameError::Storage)?;
        let token = Zeroizing::new(
            crate::security::new_token(self.runtime).map_err(|_| GameError::RandomUnavailable)?,
        );
        let digest = crate::security::token_digest(&token).map_err(|_| GameError::Storage)?;
        let sid: SessionId = make_id(self.runtime, self.runtime.now_ms())?
            .try_into()
            .map_err(|_| GameError::Storage)?;
        self.maintenance(|now| {
            let r = self.recovery_live(Some(code), now)?;
            self.recovery_rate_check(&keys, now)?;
            if self.recovery_proof(alias)?.as_ref() != Some(&proof) { return Err(GameError::InvalidRecovery); }
            if let Some(receipt) = self.receipt(&player_actor(proof.player), command, fp, now)? {
                return Ok(RecoveryOutcome { proof: proof.clone(), alias: alias.to_owned(), code: code.clone(), receipt, cookie: None });
            }
            self.admit(command, now)?;
            let epoch = advance(proof.epoch)?;
            let expires = add_time(now, PLAYER_SESSION_MS)?;
            let closes = self.db.query("SELECT connection_id,session_id,epoch FROM game_current_connections WHERE viewer_kind=? AND principal_id=? ORDER BY connection_id LIMIT 100", &[text(PLAYER_KIND), text(proof.player)])?;
            for row in closes {
                let expected = [text(string(&row, 1)?), text(proof.player), int(number(&row, 2)?), int(now)];
                let connection = text(string(&row, 0)?);
                self.db.execute("INSERT INTO game_socket_closures VALUES(?,?,?,?,?,0) ON CONFLICT(connection_id) DO NOTHING", &[connection.clone(), expected[0].clone(), expected[1].clone(), expected[2].clone(), expected[3].clone()])?;
                if self.db.query("SELECT session_id,player_id,epoch FROM game_socket_closures WHERE connection_id=?", &[connection])? != vec![expected[..3].to_vec()] { return Err(GameError::Storage); }
            }
            self.db.execute("UPDATE game_players SET session_epoch=? WHERE player_id=? AND session_epoch=?", &[int(epoch), text(proof.player), int(proof.epoch)])?;
            let mut after_proof = proof.clone();
            after_proof.epoch = epoch;
            if self.recovery_proof(alias)?.as_ref() != Some(&after_proof) { return Err(GameError::Storage); }
            self.db.execute("UPDATE game_sessions SET revoked_at=? WHERE player_id=? AND revoked_at IS NULL", &[int(now), text(proof.player)])?;
            if !self.db.query("SELECT 1 FROM game_sessions WHERE player_id=? AND revoked_at IS NULL LIMIT 1", &[text(proof.player)])?.is_empty() { return Err(GameError::Storage); }
            self.db.execute("INSERT INTO game_sessions VALUES(?,?,?,?,?,?,?,?,?,NULL)", &[text(sid), SqlValue::Blob(digest.to_vec()), text(r.id), text(code.as_str()), text(proof.player), int(epoch), text(LIVE_ACCESS), int(now), int(expires)])?;
            let session = self.player_session(&token, &r, now).map_err(|_| GameError::Storage)?;
            if session.player != proof.player || session.epoch != epoch || session.session != sid || session.expires != expires { return Err(GameError::Storage); }
            let revision = advance(self.view_revision(&proof.player.to_string())?)?;
            self.set_view_revision(&proof.player.to_string(), revision)?;
            self.clear_recovery_rates(&keys)?;
            let receipt = self.store_receipt(&player_actor(proof.player), command, fp, GameOutcome::PlayerRecovered { player_id: proof.player, view_revision: revision as u64, session_expires_at: expires }, r.id, now)?;
            Ok(RecoveryOutcome { proof: after_proof, alias: alias.to_owned(), code: code.clone(), receipt, cookie: Some(SecretCookie { token, expires }) })
        })
    }
    /// Current player authority only; old answer proof cannot authorize this operation.
    pub fn manage_recovery_answer(
        &self,
        command: CommandId,
        token: &str,
        answer: Option<&str>,
    ) -> Result<GameResponse, GameError> {
        let fp = keyed(
            self.rate_key,
            b"brews-recovery-answer-command\0",
            &[
                if answer.is_some() { b"put" } else { b"delete" },
                answer.unwrap_or_default().as_bytes(),
            ],
        )?;
        let (session, original, replay) = self.tx(|now| {
            let r = self.recovery_live(None, now).map_err(|e| {
                if e == GameError::InvalidRecovery {
                    GameError::Unauthorized
                } else {
                    e
                }
            })?;
            let session = self.player_session(token, &r, now)?;
            let original = PrivateRows(self.db.query(
                SQL_SELECT_GAME_RECOVERY_ANSWER_VERIFIER_NORMALIZATION_VERSION_UPDATED_AT,
                &[text(session.player)],
            )?);
            let receipt = self.receipt(&player_actor(session.player), command, fp, now)?;
            if receipt.is_none() {
                self.admit(command, now)?;
            }
            Ok((session, original, receipt))
        })?;
        if let Some(receipt) = replay {
            return Ok(GameResponse::Committed { receipt });
        }
        if let Some(row) = original.first() {
            if number(row, 1)? != crate::game::recovery_answers::NORMALIZATION_VERSION {
                return Err(GameError::Storage);
            }
            crate::security::bounded_phc(string(row, 0)?).map_err(|_| GameError::Storage)?;
        }
        let canonical = answer
            .map(crate::game::recovery_answers::normalize)
            .transpose()?;
        let unchanged = match (canonical.as_ref(), original.first()) {
            (None, None) => true,
            (Some(answer), Some(row)) => crate::security::verify_password(answer, string(row, 0)?)
                .map_err(|_| GameError::Storage)?,
            _ => false,
        };
        let verifier = if unchanged {
            None
        } else {
            canonical
                .as_ref()
                .map(|answer| {
                    crate::game::recovery_answers::enroll(answer, self.policy, self.runtime)
                })
                .transpose()?
        };
        self.runtime.player_recovery_proof_complete();
        self.tx(|now| {
            let r = self.recovery_live(None, now).map_err(|e| if e == GameError::InvalidRecovery { GameError::Unauthorized } else { e })?;
            let current = self.player_session(token, &r, now)?;
            if current.session != session.session || current.player != session.player || current.epoch != session.epoch { return Err(GameError::Unauthorized); }
            if let Some(receipt) = self.receipt(&player_actor(current.player), command, fp, now)? { return Ok(GameResponse::Committed { receipt }); }
            self.admit(command, now)?;
            if PrivateRows(self.db.query(SQL_SELECT_GAME_RECOVERY_ANSWER_VERIFIER_NORMALIZATION_VERSION_UPDATED_AT, &[text(current.player)])?) != original { return Err(GameError::Conflict); }
            let view = current.player.to_string();
            let revision = if unchanged { self.view_revision(&view)? } else { advance(self.view_revision(&view)?)? };
            if !unchanged {
                if let Some(verifier) = verifier.as_ref() {
                    let bindings = PrivateRows(vec![vec![text(current.player), text(verifier.as_str()), int(crate::game::recovery_answers::NORMALIZATION_VERSION), int(now)]]);
                    self.db.execute("INSERT INTO game_recovery VALUES(?,?,?,?) ON CONFLICT(player_id) DO UPDATE SET answer_verifier=excluded.answer_verifier,normalization_version=excluded.normalization_version,updated_at=excluded.updated_at", &bindings[0])?;
                    let stored = PrivateRows(self.db.query(SQL_SELECT_GAME_RECOVERY_ANSWER_VERIFIER_NORMALIZATION_VERSION_UPDATED_AT, &[text(current.player)])?);
                    if stored.first().map(Vec::as_slice) != Some(&bindings[0][1..]) { return Err(GameError::Storage); }
                } else {
                    self.db.execute("DELETE FROM game_recovery WHERE player_id=?", &[text(current.player)])?;
                    if !self.db.query(SQL_SELECT_GAME_RECOVERY_ANSWER_VERIFIER_NORMALIZATION_VERSION_UPDATED_AT, &[text(current.player)])?.is_empty() { return Err(GameError::Storage); }
                }
                self.set_view_revision(&view, revision)?;
            }
            let receipt = self.store_receipt(&player_actor(current.player), command, fp, GameOutcome::RecoveryAnswerChanged { recovery_enabled: answer.is_some(), view_revision: revision as u64 }, r.id, now)?;
            Ok(GameResponse::RecoveryAnswerChanged { recovery_enabled: answer.is_some(), view_revision: revision as u64, receipt })
        })
    }
    pub fn validate_recovery_outcome(&self, outcome: &RecoveryOutcome) -> Result<(), GameError> {
        self.maintenance(|now| {
            let record = self.recovery_live(Some(&outcome.code), now)?;
            if self.recovery_proof(&outcome.alias)?.as_ref() != Some(&outcome.proof) {
                return Err(GameError::InvalidRecovery);
            }
            if let Some(cookie) = &outcome.cookie {
                self.player_session(cookie.token(), &record, now)?;
            }
            Ok(())
        })
    }
    pub fn recovered_player_view(&self, token: &str) -> Result<GameView, GameError> {
        self.maintenance(|now| {
            let r = self.recovery_live(None, now)?;
            self.player_session(token, &r, now)?;
            Ok(())
        })?;
        let view = self.player_view(token)?;
        if !matches!(view, GameView::Player { .. }) {
            return Err(GameError::InvalidRecovery);
        }
        Ok(view)
    }
    pub fn recovered_player_deadline(&self, token: &str) -> Result<i64, GameError> {
        self.maintenance(|now| {
            let r = self.recovery_live(None, now)?;
            let session = self.player_session(token, &r, now)?;
            Ok(r.due
                .map_or(session.expires, |due| due.min(session.expires)))
        })
    }
    pub fn recovery_output_receipt(
        &self,
        command: CommandId,
        token: &str,
    ) -> Result<GameReceipt, GameError> {
        self.maintenance(|now| {
            let r = self.recovery_live(None, now)?;
            let session = self.player_session(token, &r, now)?;
            let rows = self.db.query(SQL_SELECT_REQUIRED_GAME_RECEIPT, &[text(player_actor(session.player)), text(command)])?;
            let row = rows.first().ok_or(GameError::Unauthorized)?;
            let receipt = GameReceipt::decode_json(string(row, 3)?.as_bytes()).map_err(|_| GameError::Storage)?;
            if receipt.expires_at <= now || !matches!(receipt.outcome, GameOutcome::PlayerRecovered { player_id, session_expires_at, .. } if player_id == session.player && session_expires_at == session.expires && receipt.completed_at == session.expires - PLAYER_SESSION_MS) { return Err(GameError::Unauthorized); }
            Ok(receipt)
        })
    }
    pub fn pending_recovery_socket_closes(&self) -> Result<Vec<RecoverySocketClose>, GameError> {
        self.maintenance(|_| {
            self.db
                .query(SQL_CLOSES, &[])?
                .iter()
                .map(|row| {
                    Ok(RecoverySocketClose {
                        connection: string(row, 0)?.parse().map_err(|_| GameError::Storage)?,
                        session: string(row, 1)?.parse().map_err(|_| GameError::Storage)?,
                        player: string(row, 2)?.parse().map_err(|_| GameError::Storage)?,
                        epoch: number(row, 3)?,
                        next_attempt_at: number(row, 4)?,
                        attempt_count: u32::try_from(number(row, 5)?)
                            .map_err(|_| GameError::Storage)?,
                    })
                })
                .collect()
        })
    }
    pub fn acknowledge_recovery_socket_close(
        &self,
        close: &RecoverySocketClose,
    ) -> Result<(), GameError> {
        self.maintenance(|_| {
            let binding = [text(close.connection), text(close.session), text(close.player), int(close.epoch)];
            self.db.execute("DELETE FROM game_socket_closures WHERE connection_id=? AND session_id=? AND player_id=? AND epoch=?", &binding)?;
            if !self.db.query("SELECT 1 FROM game_socket_closures WHERE connection_id=? AND session_id=? AND player_id=? AND epoch=?", &binding)?.is_empty() { return Err(GameError::Storage); }
            Ok(())
        })
    }
    pub fn retry_recovery_socket_close(
        &self,
        close: &RecoverySocketClose,
    ) -> Result<(), GameError> {
        self.maintenance(|now| {
            let attempts = close.attempt_count.saturating_add(1);
            let delay = crate::game::RETRY_INITIAL_MS.saturating_mul(1i64 << attempts.min(9)).min(crate::game::RETRY_MAX_MS);
            let next = add_time(now, delay)?;
            self.db.execute("UPDATE game_socket_closures SET next_attempt_at=?,attempt_count=? WHERE connection_id=? AND session_id=? AND player_id=? AND epoch=? AND attempt_count=?", &[int(next), int(i64::from(attempts)), text(close.connection), text(close.session), text(close.player), int(close.epoch), int(i64::from(close.attempt_count))])?;
            if self.db.query("SELECT next_attempt_at,attempt_count FROM game_socket_closures WHERE connection_id=? AND session_id=? AND player_id=? AND epoch=?", &[text(close.connection), text(close.session), text(close.player), int(close.epoch)])? != vec![vec![int(next), int(i64::from(attempts))]] { return Err(GameError::Storage); }
            Ok(())
        })
    }
}
