//! Read-only Game discovery cuts. Successful cuts allocate only bounded projections.
use super::*;
use brews_contracts::discovery::{CurrentGameSummary, GameAvailability, GameEntry};

const SQL_SELECT_DISCOVERY_PLAYER_CONNECTIONS: &str = "SELECT c.session_id,c.principal_id,c.connection_id,c.epoch,c.expires_at FROM game_current_connections c WHERE c.viewer_kind=? ORDER BY c.session_id LIMIT ?";

/// Only the owner supplies authority and validated Directory publication metadata.
#[derive(Clone, Copy)]
pub enum DiscoveryViewer<'a> {
    Summary(&'a GameAccountAuthority),
    Entry(&'a GameCode),
    Account(&'a GameAccountAuthority),
    Player(&'a str),
}
pub struct DiscoveryCut {
    pub checked_at: i64,
    pub requires_sync: bool,
    pub releasing: bool,
    pub valid_until: Option<i64>,
    pub value: DiscoveryValue,
}
pub enum DiscoveryValue {
    Summary(Option<CurrentGameSummary>),
    Entry(GameEntry),
    Availability(GameAvailability),
}
impl<D: Database, R: Runtime> GameService<'_, D, R> {
    /// Private owning-boundary denial only: both times come from a verified owner
    /// reply and the receiving Worker's clock, never from public request fields.
    pub fn fence_discovery_expiry(
        &self,
        game_id: GameId,
        deadline: i64,
        observed_at: i64,
    ) -> Result<(), GameError> {
        if !(1..=JS_SAFE_INTEGER_MAX).contains(&deadline)
            || !(deadline..=JS_SAFE_INTEGER_MAX).contains(&observed_at)
        {
            return Err(GameError::Storage);
        }
        self.db
            .transaction(|| {
                validate_schema(self.db)?;
                let record = self.record_optional().map_err(|_| StorageError)?;
                if !record.is_some_and(|r| r.id == game_id) {
                    return Err(StorageError);
                }
                self.fence_read_deadline(observed_at)
                    .map_err(|_| StorageError)
            })
            .map_err(|_| GameError::Storage)
    }

    /// Fresh account reads do not enter the game or create access grants. Presence
    /// is intersected with observed sockets and valid local sessions, never reconciled.
    /// A deadline denial writes only the monotonic metadata fence, with readback.
    pub fn discovery(
        &self,
        game_id: GameId,
        viewer: DiscoveryViewer<'_>,
        published_code: Option<&GameCode>,
        reservation_matches: bool,
        observed: &[ConnectionGrant],
    ) -> Result<DiscoveryCut, GameError> {
        self.db
            .transaction(|| {
                validate_schema(self.db)?;
                let now = self.discovery_clock().map_err(|_| StorageError)?;
                let result = (|| {
                    let account = match &viewer {
                        DiscoveryViewer::Summary(a) | DiscoveryViewer::Account(a) => Some(*a),
                        _ => None,
                    };
                    if let Some(a) = account {
                        authorize(a, now)?;
                    }
                    let record = self.record_optional()?;
                    let Some(r) = record else {
                        return if matches!(viewer, DiscoveryViewer::Summary(_)) {
                            Ok(DiscoveryCut {
                                requires_sync: false,
                                releasing: false,
                                checked_at: now,
                                valid_until: account.map(|a| a.expires_at()),
                                value: DiscoveryValue::Summary(None),
                            })
                        } else {
                            Err(GameError::NotFound)
                        };
                    };
                    if r.id != game_id {
                        return Err(GameError::Storage);
                    }
                    if r.created > now || r.revision < 0 || r.revision > JS_SAFE_INTEGER_MAX {
                        return Err(GameError::Storage);
                    }
                    if matches!(r.state, GameState::Resolved | GameState::Cancelled) {
                        return if matches!(viewer, DiscoveryViewer::Summary(_)) {
                            Ok(DiscoveryCut {
                                requires_sync: false,
                                releasing: true,
                                checked_at: now,
                                valid_until: account.map(|a| a.expires_at()),
                                value: DiscoveryValue::Summary(None),
                            })
                        } else {
                            Err(GameError::NotFound)
                        };
                    }
                    if r.due.is_some_and(|due| now >= due) {
                        let rows = self.db.query(
                            SQL_SELECT_GAME_METADATA_LAST_OBSERVED_MS_COMMAND_FLOOR_MS,
                            &[],
                        )?;
                        let previous = number(rows.first().ok_or(GameError::Storage)?, 0)?;
                        let requires_sync = r.due.is_some_and(|due| previous < due);
                        if requires_sync {
                            self.fence_read_deadline(now)?;
                        }
                        if matches!(viewer, DiscoveryViewer::Summary(_)) {
                            return Ok(DiscoveryCut {
                                checked_at: now,
                                requires_sync,
                                releasing: true,
                                valid_until: account.map(|a| a.expires_at()),
                                value: DiscoveryValue::Summary(None),
                            });
                        }
                        return Err(GameError::Expired);
                    }
                    let mut valid_until = r.due;
                    if let Some(a) = account {
                        valid_until =
                            Some(valid_until.map_or(a.expires_at(), |t| t.min(a.expires_at())));
                    }
                    let code = r
                        .code
                        .as_ref()
                        .filter(|code| r.published.is_some() && published_code == Some(*code));
                    if let DiscoveryViewer::Summary(a) = viewer {
                        return Ok(DiscoveryCut {
                            requires_sync: false,
                            releasing: false,
                            checked_at: now,
                            valid_until,
                            value: DiscoveryValue::Summary(Some(CurrentGameSummary {
                                game_id: r.id,
                                state: r.state,
                                designated_host_id: r.host,
                                created_at: r.created,
                                started_at: r.started,
                                idle_due: r.due,
                                game_code: code.cloned(),
                                may_operate: a.account_id() == r.host
                                    || a.role() == AccountRole::Admin,
                                view_revision: self.view_revision(HOST_VIEW)? as u64,
                            })),
                        });
                    }
                    if !matches!(r.state, GameState::AwaitingPlayers | GameState::InProgress)
                        || code.is_none()
                        || !reservation_matches
                    {
                        return Err(GameError::NotFound);
                    }
                    if let DiscoveryViewer::Entry(expected) = viewer
                        && code != Some(expected)
                    {
                        return Err(GameError::NotFound);
                    }
                    if let DiscoveryViewer::Player(token) = viewer {
                        let s = self.player_session(token, &r, now)?;
                        valid_until = Some(valid_until.map_or(s.expires, |t| t.min(s.expires)));
                    }
                    let configuration = self.configuration()?;
                    let rows = self.db.query(SQL_SELECT_GAME_PLAYERS_COUNT, &[])?;
                    let [row] = rows.as_slice() else {
                        return Err(GameError::Storage);
                    };
                    let count = u8::try_from(number(row, 0)?).map_err(|_| GameError::Storage)?;
                    if count > configuration.player_capacity {
                        return Err(GameError::Storage);
                    }
                    let can_join_player = r.state == GameState::AwaitingPlayers
                        && count < configuration.player_capacity;
                    let value = match viewer {
                        DiscoveryViewer::Summary(_) => return Err(GameError::Storage),
                        DiscoveryViewer::Entry(code) => DiscoveryValue::Entry(GameEntry {
                            game_id,
                            game_code: code.clone(),
                            state: r.state,
                            can_join_player,
                            can_join_spectator: false,
                        }),
                        DiscoveryViewer::Player(_) => {
                            DiscoveryValue::Availability(GameAvailability::Player {
                                state: r.state,
                                can_join_player,
                                can_join_spectator: false,
                            })
                        }
                        DiscoveryViewer::Account(a) => {
                            let connected = self.discovery_connected_players(&r, now, observed)?;
                            let can_start = r.state == GameState::AwaitingPlayers
                                && reservation_matches
                                && (a.account_id() == r.host || a.role() == AccountRole::Admin)
                                && connected >= crate::game::START_CONNECTED_PLAYER_MIN
                                && configuration
                                    .validate_start_feasibility(usize::from(count))
                                    .is_ok();
                            DiscoveryValue::Availability(GameAvailability::Account {
                                state: r.state,
                                can_join_player,
                                can_join_spectator: false,
                                player_count: count,
                                connected_player_count: connected,
                                player_capacity: configuration.player_capacity,
                                spectator_count: 0,
                                spectator_capacity: configuration.spectator_capacity,
                                can_start,
                            })
                        }
                    };
                    Ok(DiscoveryCut {
                        requires_sync: false,
                        releasing: false,
                        checked_at: now,
                        valid_until,
                        value,
                    })
                })();
                let result = match result {
                    Err(GameError::Unauthorized) => {
                        // A selected expired session cannot resurrect after clock rollback.
                        self.fence_read_deadline(now).map_err(|_| StorageError)?;
                        Err(GameError::Unauthorized)
                    }
                    result => result,
                };
                match result {
                    Err(GameError::Storage) => Err(StorageError),
                    result => Ok(result),
                }
            })
            .map_err(|_| GameError::Storage)?
    }
    fn discovery_clock(&self) -> Result<i64, GameError> {
        let rows = self.db.query(
            SQL_SELECT_GAME_METADATA_LAST_OBSERVED_MS_COMMAND_FLOOR_MS,
            &[],
        )?;
        let [row] = rows.as_slice() else {
            return Err(GameError::Storage);
        };
        let (previous, floor) = (number(row, 0)?, number(row, 1)?);
        let now = self.runtime.now_ms();
        if !(0..=JS_SAFE_INTEGER_MAX).contains(&now)
            || !(0..=previous).contains(&floor)
            || now < previous
        {
            return Err(GameError::Storage);
        }
        Ok(now)
    }
    fn discovery_connected_players(
        &self,
        r: &Record,
        now: i64,
        observed: &[ConnectionGrant],
    ) -> Result<u8, GameError> {
        if observed.len() > OBSERVED_CONNECTION_LIMIT {
            return Err(GameError::Storage);
        }
        let rows = self.db.query(
            SQL_SELECT_DISCOVERY_PLAYER_CONNECTIONS,
            &[text(PLAYER_KIND), int((RETAINED_PLAYER_LIMIT + 1) as i64)],
        )?;
        if rows.len() > RETAINED_PLAYER_LIMIT {
            return Err(GameError::Storage);
        }
        let mut players = Vec::with_capacity(RETAINED_PLAYER_LIMIT);
        for row in rows {
            let grant = parse_connection(&row, r.id)?;
            if !observed.contains(&grant) {
                continue;
            }
            match self.validate_player_connection(&grant, r, now) {
                Ok(()) if !players.contains(&grant.player) => players.push(grant.player),
                Ok(()) | Err(GameError::Unauthorized) => {}
                Err(error) => return Err(error),
            }
        }
        u8::try_from(players.len()).map_err(|_| GameError::Storage)
    }
}
