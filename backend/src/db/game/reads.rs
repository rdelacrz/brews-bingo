//! Passive reads allocate only bounded existing owner projections. No entry or renewal.
use super::*;
use crate::api::{ReadRole, ReadSelection};
use brews_contracts::games::reads::*;
use brews_domain::{
    accounts::AccountRole,
    ids::{SessionId, SpectatorId},
};
use serde::{Deserialize, Serialize};
const SQL_SELECT_PLAYER_LEAVE: &str =
    "SELECT last_explicit_leave_at FROM game_players WHERE player_id=?";
const SQL_CHECK_RECOVERY_ENABLED: &str = "SELECT 1 FROM game_recovery WHERE player_id=? LIMIT 1";
const SQL_SELECT_OBSERVED_PLAYER_CONNECTIONS: &str = "SELECT c.connection_id FROM game_current_connections c JOIN game_sessions s ON s.session_id=c.session_id AND s.player_id=c.principal_id AND s.session_epoch=c.epoch JOIN game_players p ON p.player_id=s.player_id AND p.session_epoch=s.session_epoch WHERE c.viewer_kind=? AND c.principal_id=? AND c.expires_at>? AND s.expires_at>? AND s.revoked_at IS NULL LIMIT ?";
const SQL_SELECT_OBSERVED_FINAL_PLAYER_CONNECTIONS: &str = "SELECT c.connection_id FROM game_current_connections c JOIN game_terminal_views s ON s.session_id=c.session_id AND s.player_id=c.principal_id AND s.session_epoch=c.epoch WHERE c.viewer_kind=? AND c.principal_id=? AND c.expires_at>? AND s.expires_at>? AND c.expires_at=s.expires_at LIMIT ?";
const SQL_SELECT_OBSERVED_SPECTATOR_CONNECTIONS: &str = "SELECT c.connection_id FROM game_current_connections c JOIN game_spectator_sessions s ON s.session_id=c.session_id AND s.spectator_id=c.principal_id AND s.session_epoch=c.epoch JOIN game_spectators p ON p.spectator_id=s.spectator_id AND p.session_epoch=s.session_epoch WHERE c.viewer_kind=? AND c.expires_at>? AND s.expires_at>? AND (p.grace_expires_at IS NULL OR p.grace_expires_at>?) LIMIT ?";
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ReadIdentity {
    Account {
        account_id: AccountId,
        session_id: SessionId,
        account_role: AccountRole,
        epoch: i64,
        expires_at: i64,
    },
    Player {
        player_id: PlayerId,
        session_id: SessionId,
        epoch: i64,
        expires_at: i64,
    },
    Spectator {
        spectator_id: SpectatorId,
        session_id: SessionId,
        epoch: i64,
        expires_at: i64,
    },
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadBinding {
    pub identity: ReadIdentity,
    pub valid_until: i64,
    pub host_assignment_revision: i64,
}
pub(crate) struct ReadCut {
    pub projection: ReadProjection,
    pub binding: ReadBinding,
    pub checked_at: i64,
}
pub(crate) fn read_purpose(kind: ReadKind) -> OperatorPurpose {
    match kind {
        ReadKind::Game => OperatorPurpose::GameRead,
        ReadKind::Membership => OperatorPurpose::MembershipRead,
        ReadKind::Players => OperatorPurpose::PlayersRead,
        ReadKind::Calls => OperatorPurpose::CallsRead,
        ReadKind::RemainingValues => OperatorPurpose::RemainingValuesRead,
        ReadKind::OwnBoard => OperatorPurpose::OwnBoardRead,
        ReadKind::Boards => OperatorPurpose::BoardsRead,
        ReadKind::Qualification => OperatorPurpose::QualificationRead,
        ReadKind::Result => OperatorPurpose::ResultRead,
        ReadKind::CommandLookup(_) => OperatorPurpose::CommandLookup,
    }
}
impl<D: Database, R: Runtime> GameService<'_, D, R> {
    pub fn dedicated_read(
        &self,
        selection: ReadSelection,
        token: &str,
        account: Option<&GameAccountAuthority>,
        observed: &[ConnectionId],
    ) -> Result<ReadProjection, GameError> {
        self.capture_read(selection, token, account, observed, true)
            .map(|c| c.projection)
    }
    pub(crate) fn capture_read(
        &self,
        selection: ReadSelection,
        token: &str,
        account: Option<&GameAccountAuthority>,
        observed: &[ConnectionId],
        audit: bool,
    ) -> Result<ReadCut, GameError> {
        let action = |now| self.read_at(selection, token, account, observed, now);
        if audit && let Some(a) = account {
            self.operator_attempt(a, read_purpose(selection.kind), |now| {
                action(now).map(|c| (c, true))
            })
        } else {
            self.maintenance(action)
        }
    }
    fn read_at(
        &self,
        selection: ReadSelection,
        token: &str,
        account: Option<&GameAccountAuthority>,
        observed: &[ConnectionId],
        now: i64,
    ) -> Result<ReadCut, GameError> {
        if let ReadKind::CommandLookup(command) = selection.kind {
            return self.command_lookup_at(selection, command, token, account, now);
        }
        if observed.len() > OBSERVED_CONNECTION_LIMIT {
            return Err(GameError::Storage);
        }
        if selection.kind.account_only() && selection.role != ReadRole::Account
            || selection.kind == ReadKind::OwnBoard && selection.role != ReadRole::Player
            || selection.kind == ReadKind::Membership && selection.role == ReadRole::Account
            || selection.player_id.is_some() && selection.kind != ReadKind::Boards
        {
            return Err(GameError::InvalidInput);
        }
        let r = match self.record_optional()? {
            Some(record) => record,
            None => {
                if selection.role != ReadRole::Account {
                    return Err(GameError::Unauthorized);
                }
                authorize(account.ok_or(GameError::Unauthorized)?, now)?;
                if self
                    .terminal_routing_projection()?
                    .is_some_and(|route| route.game_id() == selection.game_id)
                {
                    return Err(GameError::Unauthorized);
                }
                return Err(GameError::NotFound);
            }
        };
        if r.id != selection.game_id {
            return Err(GameError::NotFound);
        }
        ensure_before_idle(&r, now)?;
        let terminal = matches!(r.state, GameState::Resolved | GameState::Cancelled);
        let (mut view, identity, expires) = match selection.role {
            ReadRole::Account => {
                let a = account.ok_or(GameError::Unauthorized)?;
                authorize(a, now)?;
                if terminal {
                    self.require_final_history(&r, now)?;
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
                    self.host_projection(&r, now)?,
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
                let s = self.player_session(token, &r, now)?;
                (
                    self.player_projection(&r, s.player)?,
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
                let s = self.spectator_session(token, &r, now)?;
                (
                    self.spectator_projection(&r, s.spectator)?,
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
        // Prepared slots and orphaned current rows are never observed WSS presence.
        if let GameView::Host {
            players,
            connected_player_count,
            connected_spectator_count,
            ..
        } = &mut view
        {
            for p in players.iter_mut() {
                let rows = self.db.query(
                    SQL_SELECT_OBSERVED_PLAYER_CONNECTIONS,
                    &[
                        text(PLAYER_KIND),
                        text(p.player_id),
                        int(now),
                        int(now),
                        int(OBSERVED_CONNECTION_LIMIT as i64 + 1),
                    ],
                )?;
                p.connected = rows.iter().any(|row| {
                    string(row, 0)
                        .ok()
                        .and_then(|v| v.parse::<ConnectionId>().ok())
                        .is_some_and(|id| observed.contains(&id))
                });
            }
            *connected_player_count = players.iter().filter(|p| p.connected).count() as u8;
            let rows = self.db.query(
                SQL_SELECT_OBSERVED_SPECTATOR_CONNECTIONS,
                &[
                    text(crate::game::SPECTATOR_KIND),
                    int(now),
                    int(now),
                    int(now),
                    int(OBSERVED_CONNECTION_LIMIT as i64 + 1),
                ],
            )?;
            *connected_spectator_count = rows
                .iter()
                .filter(|row| {
                    string(row, 0)
                        .ok()
                        .and_then(|v| v.parse::<ConnectionId>().ok())
                        .is_some_and(|id| observed.contains(&id))
                })
                .count() as u8;
        }
        view.validate().map_err(|_| GameError::Storage)?;
        let valid_until = expires.min(if terminal {
            r.history_expires.ok_or(GameError::Unauthorized)?
        } else {
            r.due.unwrap_or(expires)
        });
        if now >= valid_until {
            return Err(GameError::Unauthorized);
        }
        let projection = self.narrow_read(selection, view, &identity, terminal, observed, now)?;
        projection.encode_json().map_err(|_| GameError::Storage)?;
        Ok(ReadCut {
            projection,
            binding: ReadBinding {
                identity,
                valid_until,
                host_assignment_revision: self.assignment_revision()?,
            },
            checked_at: now,
        })
    }
    fn narrow_read(
        &self,
        selection: ReadSelection,
        view: GameView,
        identity: &ReadIdentity,
        terminal: bool,
        observed: &[ConnectionId],
        now: i64,
    ) -> Result<ReadProjection, GameError> {
        let rev = view.view_revision();
        if selection.kind == ReadKind::Game {
            return Ok(ReadProjection::Game(GameDetail { game: view }));
        }
        if selection.kind == ReadKind::Result {
            if !matches!(
                view,
                GameView::FinalHost { .. } | GameView::FinalPlayer { .. }
            ) {
                return Err(GameError::ResultNotCommitted);
            }
            return Ok(ReadProjection::Result(ResultRead { result: view }));
        }
        let (calls, boards, own) = match view {
            GameView::Host { calls, players, .. } => (
                calls,
                players
                    .into_iter()
                    .map(|p| InspectedBoard {
                        player_id: p.player_id,
                        alias: p.alias,
                        board: p.board,
                    })
                    .collect::<Vec<_>>(),
                None,
            ),
            GameView::FinalHost { calls, players, .. } => (
                calls,
                players
                    .into_iter()
                    .map(|p| InspectedBoard {
                        player_id: p.player_id,
                        alias: p.alias,
                        board: Some(p.board),
                    })
                    .collect(),
                None,
            ),
            GameView::Player {
                calls,
                player_id,
                alias,
                board,
                ..
            } => (
                calls,
                vec![],
                Some(InspectedBoard {
                    player_id,
                    alias,
                    board,
                }),
            ),
            GameView::FinalPlayer {
                calls,
                player_id,
                alias,
                board,
                ..
            } => (
                calls,
                vec![],
                Some(InspectedBoard {
                    player_id,
                    alias,
                    board: Some(board),
                }),
            ),
            GameView::Spectator { calls, .. } => (calls, vec![], None),
        };
        Ok(match selection.kind {
            ReadKind::Membership => {
                let membership = match identity {
                    ReadIdentity::Player {
                        player_id,
                        expires_at,
                        ..
                    } => Membership::Player {
                        player_id: *player_id,
                        alias: own.ok_or(GameError::Storage)?.alias,
                        access: if terminal {
                            MembershipAccess::FinalOnly
                        } else {
                            MembershipAccess::OriginalLive
                        },
                        session_expires_at: *expires_at,
                        recovery_enabled: !terminal
                            && !self
                                .db
                                .query(SQL_CHECK_RECOVERY_ENABLED, &[text(player_id)])?
                                .is_empty(),
                    },
                    ReadIdentity::Spectator {
                        spectator_id,
                        expires_at,
                        ..
                    } => Membership::Spectator {
                        spectator_id: *spectator_id,
                        access: MembershipAccess::OriginalLive,
                        session_expires_at: *expires_at,
                    },
                    _ => return Err(GameError::Forbidden),
                };
                ReadProjection::Membership(MembershipRead {
                    membership,
                    view_revision: rev,
                })
            }
            ReadKind::Calls => ReadProjection::Calls(CallsRead {
                latest: calls.last().cloned(),
                calls,
                view_revision: rev,
            }),
            ReadKind::RemainingValues => {
                if terminal {
                    return Err(GameError::RemainingValuesUnavailable);
                }
                let upper = self.configuration()?.numeric_upper_bound;
                let values = (1..=upper)
                    .map(|n| n.to_string())
                    .filter(|v| !calls.iter().any(|c| c.value == *v))
                    .collect::<Vec<_>>();
                ReadProjection::RemainingValues(RemainingValues {
                    remaining_count: values.len() as u32,
                    exhausted: values.is_empty(),
                    remaining_values: values,
                    view_revision: rev,
                })
            }
            ReadKind::OwnBoard => ReadProjection::OwnBoard(OwnBoard {
                board: own.ok_or(GameError::Forbidden)?.board,
                view_revision: rev,
            }),
            ReadKind::Boards => {
                let boards = if let Some(id) = selection.player_id {
                    vec![
                        boards
                            .into_iter()
                            .find(|p| p.player_id == id)
                            .ok_or(GameError::NotFound)?,
                    ]
                } else {
                    boards
                };
                ReadProjection::Boards(BoardsRead {
                    boards,
                    view_revision: rev,
                })
            }
            ReadKind::Players => {
                let mut players = Vec::with_capacity(boards.len());
                for p in boards {
                    let left_during_play = if terminal {
                        None
                    } else {
                        let rows = self
                            .db
                            .query(SQL_SELECT_PLAYER_LEAVE, &[text(p.player_id)])?;
                        Some(optional_number(rows.first().ok_or(GameError::Storage)?, 0)?.is_some())
                    };
                    let connected =
                        self.read_player_connected(p.player_id, terminal, observed, now)?;
                    players.push(RosterPlayer {
                        player_id: p.player_id,
                        alias: p.alias,
                        connected,
                        membership_retained: true,
                        left_during_play,
                    });
                }
                ReadProjection::Players(PlayerRoster {
                    players,
                    view_revision: rev,
                })
            }
            ReadKind::Qualification => match selection.role {
                ReadRole::Spectator => return Err(GameError::Forbidden),
                ReadRole::Player => {
                    let board = own.ok_or(GameError::Storage)?.board;
                    ReadProjection::Qualification(QualificationRead::Player {
                        qualified: board.as_ref().is_some_and(|b| b.qualified),
                        qualifying_lines: board.map_or_else(Vec::new, |b| b.qualifying_lines),
                        view_revision: rev,
                    })
                }
                ReadRole::Account => ReadProjection::Qualification(QualificationRead::Account {
                    qualified_players: boards
                        .into_iter()
                        .filter_map(|p| {
                            p.board.filter(|b| b.qualified).map(|b| QualifiedPlayer {
                                player_id: p.player_id,
                                alias: p.alias,
                                qualifying_lines: b.qualifying_lines,
                            })
                        })
                        .collect(),
                    view_revision: rev,
                }),
            },
            ReadKind::Game | ReadKind::Result | ReadKind::CommandLookup(_) => {
                return Err(GameError::Storage);
            }
        })
    }
    fn read_player_connected(
        &self,
        player: PlayerId,
        terminal: bool,
        observed: &[ConnectionId],
        now: i64,
    ) -> Result<bool, GameError> {
        let sql = if terminal {
            SQL_SELECT_OBSERVED_FINAL_PLAYER_CONNECTIONS
        } else {
            SQL_SELECT_OBSERVED_PLAYER_CONNECTIONS
        };
        let rows = self.db.query(
            sql,
            &[
                text(PLAYER_KIND),
                text(player),
                int(now),
                int(now),
                int(OBSERVED_CONNECTION_LIMIT as i64 + 1),
            ],
        )?;
        if rows.len() > OBSERVED_CONNECTION_LIMIT {
            return Err(GameError::Storage);
        }
        let mut connected = false;
        for row in rows {
            let id = string(&row, 0)?
                .parse::<ConnectionId>()
                .map_err(|_| GameError::Storage)?;
            connected |= observed.contains(&id);
        }
        Ok(connected)
    }
    pub(crate) fn read_clock(&self) -> Result<i64, GameError> {
        self.discovery_clock()
    }
    pub(crate) fn release_read(
        &self,
        selection: ReadSelection,
        token: &str,
        account: Option<&GameAccountAuthority>,
        observed: &[ConnectionId],
        binding: &ReadBinding,
        expected: &ReadProjection,
    ) -> Result<(i64, i64), GameError> {
        let now = self.discovery_clock()?;
        if now >= binding.valid_until {
            self.fence_operator_output_expiry(now)?;
            return Err(GameError::Unauthorized);
        }
        let (checked_at, cut) = self
            .db
            .transaction(|| {
                validate_schema(self.db)?;
                let now = self.discovery_clock().map_err(|_| StorageError)?;
                match self.read_at(selection, token, account, observed, now) {
                    Err(GameError::Storage) => Err(StorageError),
                    out => Ok((now, out)),
                }
            })
            .map_err(|_| GameError::Storage)?;
        let cut = match cut {
            Ok(c) => c,
            Err(GameError::Unauthorized | GameError::Expired | GameError::NotFound) => {
                self.fence_operator_output_expiry(checked_at)?;
                return Err(GameError::Unauthorized);
            }
            Err(e) => return Err(e),
        };
        if cut.binding.identity != binding.identity
            || cut.binding.host_assignment_revision != binding.host_assignment_revision
        {
            self.fence_operator_output_expiry(checked_at)?;
            return Err(GameError::Unauthorized);
        }
        let deadline = binding.valid_until.min(cut.binding.valid_until);
        if cut.checked_at >= deadline {
            self.fence_operator_output_expiry(cut.checked_at)?;
            return Err(GameError::Unauthorized);
        }
        if cut
            .projection
            .encode_json()
            .map_err(|_| GameError::Storage)?
            != expected.encode_json().map_err(|_| GameError::Storage)?
        {
            return Err(GameError::Storage);
        }
        Ok((cut.checked_at, deadline))
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Real SQLite fixtures fail fast."
)]
mod tests {
    use super::super::tests::{authority, id, initialized};
    use super::*;
    use crate::auth::test_support::{Sqlite, TestRuntime};
    fn service<'a>(db: &'a Sqlite, rt: &'a TestRuntime) -> GameService<'a, Sqlite, TestRuntime> {
        GameService::new(db, rt, AuthPolicy::default(), &[9; 32]).unwrap()
    }
    fn selection(db: &Sqlite, rt: &TestRuntime) -> ReadSelection {
        ReadSelection {
            game_id: service(db, rt).record().unwrap().id,
            kind: ReadKind::Game,
            role: ReadRole::Account,
            player_id: None,
        }
    }
    #[test]
    fn dedicated_release_original_cap_never_extends_after_resume_and_equality_fences_before_entropy()
     {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        initialized(&db, &rt);
        let svc = service(&db, &rt);
        let a = authority(&rt, 2);
        let s = selection(&db, &rt);
        let mut cut = svc.capture_read(s, "", Some(&a), &[], false).unwrap();
        cut.binding.valid_until = rt.now.get() + 5;
        assert_eq!(
            svc.release_read(s, "", Some(&a), &[], &cut.binding, &cut.projection)
                .unwrap()
                .1,
            cut.binding.valid_until
        );
        rt.now.set(cut.binding.valid_until);
        rt.fail.set(true);
        assert_eq!(
            svc.release_read(s, "", Some(&a), &[], &cut.binding, &cut.projection),
            Err(GameError::Unauthorized)
        );
        assert_eq!(
            db.query("SELECT last_observed_ms FROM game_metadata", &[])
                .unwrap(),
            vec![vec![int(rt.now.get())]]
        );
        rt.now.set(rt.now.get() - 1);
        assert_eq!(
            svc.release_read(s, "", Some(&a), &[], &cut.binding, &cut.projection),
            Err(GameError::Storage)
        );
    }
    #[test]
    fn dedicated_release_ignored_fence_is_unavailable_and_rolls_back() {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        initialized(&db, &rt);
        let svc = service(&db, &rt);
        let a = authority(&rt, 2);
        let s = selection(&db, &rt);
        let cut = svc.capture_read(s, "", Some(&a), &[], false).unwrap();
        db.execute("CREATE TRIGGER deny_read_fence BEFORE UPDATE ON game_metadata BEGIN SELECT RAISE(IGNORE); END",&[]).unwrap();
        rt.now.set(cut.binding.valid_until);
        assert_eq!(
            svc.release_read(s, "", Some(&a), &[], &cut.binding, &cut.projection),
            Err(GameError::Storage)
        );
    }
    struct SampledClock<'a> {
        base: &'a TestRuntime,
        samples: std::cell::RefCell<std::collections::VecDeque<i64>>,
    }
    impl Runtime for SampledClock<'_> {
        fn now_ms(&self) -> i64 {
            self.samples
                .borrow_mut()
                .pop_front()
                .unwrap_or(self.base.now.get())
        }
        fn fill_random(&self, bytes: &mut [u8]) -> Result<(), crate::auth::AuthError> {
            self.base.fill_random(bytes)
        }
    }
    #[test]
    fn dedicated_release_fences_actual_inner_expiry_sample_and_rejects_rollback() {
        for fault in [false, true] {
            let db = Sqlite::new();
            let rt = TestRuntime::new();
            initialized(&db, &rt);
            let svc = service(&db, &rt);
            let original = authority(&rt, 2);
            let account = GameAccountAuthority::from_trusted_peer(
                original.account_id(),
                original.session_id(),
                original.role(),
                original.credential_epoch(),
                rt.now.get() + 1000,
            )
            .unwrap();
            let s = selection(&db, &rt);
            let cut = svc.capture_read(s, "", Some(&account), &[], false).unwrap();
            let expiry = account.expires_at();
            rt.now.set(expiry - 1);
            let clock = SampledClock {
                base: &rt,
                samples: std::cell::RefCell::new([expiry - 1, expiry].into()),
            };
            let sampled = GameService::new(&db, &clock, AuthPolicy::default(), &[9; 32]).unwrap();
            if fault {
                db.execute("CREATE TRIGGER deny_read_fence BEFORE UPDATE ON game_metadata BEGIN SELECT RAISE(IGNORE); END", &[]).unwrap();
            }
            assert_eq!(
                sampled.release_read(s, "", Some(&account), &[], &cut.binding, &cut.projection),
                Err(if fault {
                    GameError::Storage
                } else {
                    GameError::Unauthorized
                }),
            );
            if !fault {
                assert_eq!(
                    db.query("SELECT last_observed_ms FROM game_metadata", &[])
                        .unwrap(),
                    vec![vec![int(expiry)]],
                );
                assert!(matches!(
                    sampled.capture_read(s, "", Some(&account), &[], false),
                    Err(GameError::Storage)
                ));
            }
        }
    }
    #[test]
    fn dedicated_release_payload_changed_at_same_identity_is_withheld() {
        let db = Sqlite::new();
        let rt = TestRuntime::new();
        initialized(&db, &rt);
        let svc = service(&db, &rt);
        let a = authority(&rt, 2);
        let s = selection(&db, &rt);
        let cut = svc.capture_read(s, "", Some(&a), &[], false).unwrap();
        let patch = brews_contracts::games::ConfigurationPatch::decode_json(
            br#"{"expected_revision":0,"player_capacity":3}"#,
        )
        .unwrap();
        svc.update_configuration(authority(&rt, 2), id(&rt, 8), &patch)
            .unwrap();
        assert_eq!(
            svc.release_read(s, "", Some(&a), &[], &cut.binding, &cut.projection),
            Err(GameError::Storage)
        );
    }
    #[test]
    fn dedicated_release_identity_epoch_role_assignment_and_deleted_record_deny() {
        for change in ["epoch", "role", "assignment", "delete"] {
            let db = Sqlite::new();
            let rt = TestRuntime::new();
            initialized(&db, &rt);
            let svc = service(&db, &rt);
            let a = authority(&rt, 2);
            let s = selection(&db, &rt);
            let cut = svc.capture_read(s, "", Some(&a), &[], false).unwrap();
            let mut next = authority(&rt, 2);
            if change == "epoch" || change == "role" {
                next = GameAccountAuthority::from_trusted_peer(
                    a.account_id(),
                    a.session_id(),
                    if change == "role" {
                        AccountRole::Admin
                    } else {
                        a.role()
                    },
                    if change == "epoch" { 1 } else { 0 },
                    a.expires_at(),
                )
                .unwrap();
            } else if change == "assignment" {
                db.execute("UPDATE game_record SET host_assignment_revision=1", &[])
                    .unwrap();
            } else {
                db.execute("DELETE FROM game_free_cells", &[]).unwrap();
                db.execute("DELETE FROM game_configuration", &[]).unwrap();
                db.execute("DELETE FROM game_record", &[]).unwrap();
            }
            assert_eq!(
                svc.release_read(s, "", Some(&next), &[], &cut.binding, &cut.projection),
                Err(GameError::Unauthorized),
                "{change}"
            );
        }
    }
}
