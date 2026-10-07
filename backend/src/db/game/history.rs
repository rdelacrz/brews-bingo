//! History reads are current-account permission, not original-session final access.
use super::{
    Database, GameAccountAuthority, GameError, GameId, GameService, GameView, Runtime, StorageError,
};
use brews_contracts::history::{History, HistoryPlayer, HistoryWinner};

impl<D: Database, R: Runtime> GameService<'_, D, R> {
    /// Read the complete immutable snapshot for any fresh enabled Verified Normal account.
    /// No game ownership, admission, final grants or view counters are consulted or created.
    pub fn history(
        &self,
        authority: GameAccountAuthority,
        game_id: GameId,
    ) -> Result<History, GameError> {
        self.read_history(authority, game_id, true)
    }
    /// Final post-await cut: successful reads are write-free; expiry denials persist a fence.
    pub fn history_for_response(
        &self,
        authority: GameAccountAuthority,
        game_id: GameId,
    ) -> Result<History, GameError> {
        self.read_history(authority, game_id, false)
    }
    fn read_history(
        &self,
        authority: GameAccountAuthority,
        game_id: GameId,
        observe: bool,
    ) -> Result<History, GameError> {
        self.db
            .transaction(|| {
                super::validate_schema(self.db)?;
                let now = if observe {
                    self.observe_clock().map_err(|_| StorageError)?
                } else {
                    let rows = self.db.query(
                        super::SQL_SELECT_GAME_METADATA_LAST_OBSERVED_MS_COMMAND_FLOOR_MS,
                        &[],
                    )?;
                    let [row] = rows.as_slice() else {
                        return Err(StorageError);
                    };
                    let previous = super::number(row, 0).map_err(|_| StorageError)?;
                    let floor = super::number(row, 1).map_err(|_| StorageError)?;
                    let now = self.runtime.now_ms();
                    if !(0..=super::JS_SAFE_INTEGER_MAX).contains(&now)
                        || !(0..=previous).contains(&floor)
                        || now < previous
                    {
                        return Err(StorageError);
                    }
                    now
                };
                let result = (|| {
                    super::authorize(&authority, now)?;
                    let record = self.record_optional()?.ok_or(GameError::NotFound)?;
                    if record.id != game_id {
                        return Err(GameError::Storage);
                    }
                    if let Err(error) = self.require_final_history(&record, now) {
                        if error == GameError::Expired {
                            if !observe {
                                self.fence_history_expiry(now)?;
                            }
                            return Err(GameError::NotFound);
                        }
                        return Err(error);
                    }
                    let GameView::FinalHost {
                        game_id,
                        game_code,
                        designated_host_id,
                        state,
                        started_at,
                        ended_at,
                        history_expires_at,
                        winner,
                        calls,
                        players,
                        ..
                    } = self.final_snapshot(&record)?
                    else {
                        return Err(GameError::Storage);
                    };
                    Ok(History {
                        game_id,
                        game_code,
                        designated_host_id,
                        outcome: state,
                        started_at,
                        ended_at,
                        expires_at: history_expires_at,
                        winner: winner.map(|w| HistoryWinner {
                            player_id: w.player_id,
                            alias: w.alias,
                        }),
                        ordered_calls: calls.into_iter().map(|c| c.value).collect(),
                        players: players
                            .into_iter()
                            .map(|p| HistoryPlayer {
                                player_id: p.player_id,
                                alias: p.alias,
                                side_length: p.board.side_length,
                                cells: p.board.cells,
                            })
                            .collect(),
                    })
                })();
                match result {
                    Err(GameError::Storage) => Err(StorageError),
                    result => Ok(result),
                }
            })
            .map_err(|_| GameError::Storage)?
    }
    fn fence_history_expiry(&self, now: i64) -> Result<(), GameError> {
        let rows = self.db.query(
            super::SQL_SELECT_GAME_METADATA_LAST_OBSERVED_MS_COMMAND_FLOOR_MS,
            &[],
        )?;
        let [row] = rows.as_slice() else {
            return Err(GameError::Storage);
        };
        let previous = super::number(row, 0)?;
        let floor = super::number(row, 1)?;
        if !(0..=super::JS_SAFE_INTEGER_MAX).contains(&now)
            || !(0..=now).contains(&previous)
            || !(0..=previous).contains(&floor)
        {
            return Err(GameError::Storage);
        }
        // Persist the denial's sampled time, never a second possibly rolled-back clock sample.
        let expected = vec![
            super::int(now),
            super::int(floor.max(now.saturating_sub(super::RECEIPT_MS))),
        ];
        self.db
            .execute(super::SQL_UPDATE_GAME_METADATA, &expected)?;
        if self.db.query(
            super::SQL_SELECT_GAME_METADATA_LAST_OBSERVED_MS_COMMAND_FLOOR_MS,
            &[],
        )? != vec![expected]
        {
            return Err(GameError::Storage);
        }
        Ok(())
    }
}
