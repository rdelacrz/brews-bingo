//! Allocation-conscious game screens. Account and participant authority stay separate.
mod active;
mod command;
mod entry;
mod finished;
mod model;
mod preparation;
mod settings;
mod transport;
mod waiting;

pub use entry::Join;
pub(in crate::features) use entry::player_entry_membership;
pub use preparation::CreateGame;

use crate::{
    components::{
        DesignatedHostAlias, Keyed, Notice, PageTitle, StatusBadge, StatusTone, game_state_label,
    },
    session::AccountSession,
};
use brews_contracts::{
    discovery::GameAvailability,
    games::{GameResponse, GameView},
};
use brews_domain::ids::GameId;
use command::{Feedback, GameCommand, use_game_command};
use dioxus::prelude::*;
use model::{Connection, ViewContext};

#[derive(Clone, Copy)]
struct GameContext {
    account: AccountSession,
    id: Signal<GameId>,
    context: Signal<ViewContext>,
    snapshot: Signal<Option<GameView>>,
    notice: Signal<Option<model::TerminalNotice>>,
    cancelled: Signal<bool>,
    status: Signal<Connection>,
    deadline: Signal<Option<i64>>,
    idle_due: Signal<Option<i64>>,
    may_operate: Signal<bool>,
    availability: Signal<Option<GameAvailability>>,
    error: Signal<Option<String>>,
    generation: Signal<u64>,
    stream: Signal<StreamState>,
    exit_destination: Signal<Option<String>>,
    command: GameCommand,
}
impl GameContext {
    fn can_command(self) -> bool {
        self.fresh() && !self.command.blocked()
    }
    fn fresh(self) -> bool {
        (self.status)() == Connection::Live
            && self.call_view_current()
            && (self.stream)() == StreamState::Open
            && ((self.context)() != ViewContext::Account || self.account.normal())
            && self
                .deadline
                .read()
                .is_some_and(|d| d > crate::client::now_ms())
            && self
                .idle_due
                .read()
                .is_none_or(|d| d > crate::client::now_ms())
    }
    fn operate(self) -> bool {
        self.can_command() && (self.may_operate)()
    }
    fn call_view_current(self) -> bool {
        self.command.result.read().as_ref().is_none_or(|response| {
            call_revision(response).is_none()
                || call_snapshot_current(
                    response,
                    self.snapshot.read().as_ref().map(GameView::view_revision),
                )
        })
    }
    fn path(self, action: &str) -> String {
        format!("/api/games/{}/{action}", (self.id)())
    }
    fn refresh(mut self) {
        if (self.stream)() == StreamState::Paused {
            return;
        }
        self.may_operate.set(false);
        self.status.set(Connection::Synchronizing);
        self.generation.with_mut(|v| *v = v.wrapping_add(1));
    }
    fn clear(mut self, status: Connection) {
        if matches!(
            status,
            Connection::Denied | Connection::Expired | Connection::Ended
        ) {
            self.command.retire_body();
        }
        self.snapshot.set(None);
        self.notice.set(None);
        self.may_operate.set(false);
        self.availability.set(None);
        self.status.set(status);
    }
    fn restore(mut self) {
        self.stream.set(StreamState::Open);
        self.refresh();
    }
    async fn exit(mut self, destination: String) {
        if self.notice.read().is_some() {
            self.exit_destination.set(Some(destination));
            return;
        }
        if self.can_command() {
            self.exit_destination.set(Some(destination));
            self.send(
                "POST",
                &format!("exit?view={}", (self.context)().as_str()),
                serde_json::json!({}),
            )
            .await;
        }
    }
    async fn send(mut self, method: &str, action: &str, mut body: serde_json::Value) {
        if self.can_command() {
            if (self.stream)().before_command(action) == StreamState::Paused {
                self.stream.set(StreamState::Paused);
                self.status.set(Connection::Paused);
                self.may_operate.set(false);
            }
            let method = method.to_owned();
            let path = self.path(action);
            // A committed snapshot can unmount the initiating lifecycle panel before HTTP returns.
            dioxus::core::Runtime::current().spawn(self.command.busy.origin_scope(), async move {
                self.command.send(&method, &path, body).await;
                // Reconcile once by read-only lookup, never by issuing another mutation.
                if self.command.retained()
                    && (self.fresh() || (self.stream)() == StreamState::Paused)
                {
                    self.command
                        .check(&(self.id)().to_string(), (self.context)().as_str())
                        .await;
                }
            });
        } else {
            command::erase(&mut body);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StreamState {
    Open,
    Paused,
}
impl StreamState {
    fn before_command(self, action: &str) -> Self {
        if matches!(
            action.split('?').next(),
            Some("membership/leave" | "membership/player" | "membership/spectator" | "exit")
        ) {
            Self::Paused
        } else {
            self
        }
    }
}

#[component]
pub fn GamePage(game_id: String, view: String) -> Element {
    let account = use_context::<AccountSession>();
    let Ok(id) = game_id.parse::<GameId>() else {
        return rsx! { Notice { message: "This game address is invalid." } };
    };
    let Some(context) = ViewContext::parse(&view) else {
        return rsx! { Notice { message: "Choose an explicit account, player, or spectator view." } };
    };
    let epoch = if context == ViewContext::Account {
        (account.epoch)()
    } else {
        0
    };
    if context == ViewContext::Account && !(account.ready)() && account.session.read().is_none() {
        return rsx! {crate::components::Loading {}};
    }
    if context == ViewContext::Account && (account.ready)() && !account.normal() {
        return rsx! { Notice { message: "A normal host or admin session is required for this account view." } a { class: "button", href: "/login", "Host / admin login" } };
    }
    rsx! { Keyed { identity: format!("{game_id}:{view}:{epoch}"), GameScreen { id, context } } }
}

#[component]
fn GameScreen(id: GameId, context: ViewContext) -> Element {
    let account = use_context::<AccountSession>();
    let game = GameContext {
        account,
        id: use_signal(|| id),
        context: use_signal(|| context),
        snapshot: use_signal(|| None),
        notice: use_signal(|| None),
        cancelled: use_signal(|| false),
        status: use_signal(|| Connection::Connecting),
        deadline: use_signal(|| None),
        idle_due: use_signal(|| None),
        may_operate: use_signal(|| false),
        availability: use_signal(|| None),
        error: use_signal(|| None),
        generation: use_signal(|| 0),
        stream: use_signal(|| StreamState::Open),
        exit_destination: use_signal(|| None),
        command: use_game_command(),
    };
    use_context_provider(|| game);
    let mut connection = use_future(move || async move {
        transport::run(game, account).await;
    });
    #[cfg(target_arch = "wasm32")]
    use_future(move || async move {
        transport::watch_expiry(game).await;
        connection.cancel();
    });
    use_effect(move || {
        let _generation = (game.generation)();
        if (game.stream)() == StreamState::Paused {
            connection.cancel();
        } else {
            connection.restart();
        }
    });
    let navigator = use_navigator();
    use_effect(move || {
        let destination = (game.exit_destination)();
        let audience = game.notice.peek().is_some();
        if audience && let Some(destination) = destination {
            connection.cancel();
            game.clear(Connection::Ended);
            navigator.push(destination);
        }
    });
    use_effect(move || {
        if let Some(response) = (game.command.result)() {
            if let Some(destination) = command_destination(
                &response,
                id,
                context,
                game.exit_destination.peek().as_deref(),
            ) {
                connection.cancel();
                game.clear(Connection::Ended);
                navigator.push(destination);
            } else if cancelled_before_start(&response) {
                connection.cancel();
                game.clear(Connection::Ended);
                let mut cancelled = game.cancelled;
                cancelled.set(true);
            } else if !matches!(response, GameResponse::Pending { .. })
                && call_revision(&response).is_none()
            {
                game.refresh();
            }
        }
    });
    use_effect(move || {
        if (game.command.denied)() {
            connection.cancel();
            game.clear(Connection::Denied);
        }
    });
    use_effect(move || {
        if (game.command.needs_refresh)() {
            let mut needed = game.command.needs_refresh;
            needed.set(false);
            game.refresh();
        }
    });
    let snapshot = (game.snapshot)();
    let notice = (game.notice)();
    let status = (game.status)();
    let game_state = snapshot.as_ref().map(model::state);
    rsx! {
        div { class: "stack",
            PageTitle { eyebrow: format!("{} view",context.as_str()), title: "Your game".to_string(), description: "Single Line · Rockville Brews".to_string() }
            if let Some(state) = game_state {
                div { class: "game-live-status-row", role: "status",
                    StatusBadge { label: status.label(), tone: status.tone() }
                    StatusBadge { label: game_state_label(state), tone: StatusTone::from(state) }
                }
            } else {
                div { class: "connection-status", role: "status", StatusBadge { label: status.label(), tone: status.tone() } }
            }
            if let Some(error) = (game.error)() { Notice { message: error } }
            Feedback { command: game.command, game_id: id.to_string(), view: context.as_str().to_string(),enabled:game.fresh() }
            if (game.cancelled)() {
                p {class:"notice", "The unstarted game was cancelled. No History entry was created."}
                a {class:"button",href:"/games","Back to Games"}
            } else if notice.is_some() {
                finished::AudienceResult {}
            } else if let Some(snapshot) = snapshot {
                if context==ViewContext::Account && !model::terminal(&snapshot) {
                    if let GameView::Host{game:summary,..}=&snapshot{p {class:"designated-host-line", span {"Designated host"} DesignatedHostAlias { account_id: summary.designated_host_id }}}
                    if !(game.may_operate)() { p { class: "muted", "Read-only account view. Operator controls require current server permission." } }
                }
                match model::state(&snapshot) {
                    brews_domain::games::GameState::New => rsx! { preparation::SavedSetup {} },
                    brews_domain::games::GameState::AwaitingPlayers => rsx! { waiting::Lobby {} },
                    brews_domain::games::GameState::InProgress => rsx! { active::Play {} },
                    _ => rsx! { finished::ResultView {} },
                }
            } else {
                p { "An authorized game snapshot is required before any board or controls can be shown." }
                if matches!(status,Connection::Denied | Connection::Expired | Connection::Unsupported) {
                    a { class: "button secondary", href: "/", "Home" }
                    if context==ViewContext::Player { a { class: "button secondary", href: "/recover", "Recover player access" } }
                }
            }
            if !matches!(status,Connection::Live | Connection::Ended) {
                button { class: "button secondary", disabled: (game.command.busy)(), onclick: move |_| game.restore(),
                    if (game.stream)()==StreamState::Paused {"Restore connection and revalidate"}else{"Reconnect and revalidate"}
                }
                if (game.stream)()==StreamState::Paused {
                    p {class:"muted","Automatic reconnection is paused. Resolve any original command before restoring; restoration can reconnect a retained player session."}
                }
            }
            if !game.snapshot.read().as_ref().is_some_and(model::terminal) && game.notice.read().is_none() {
                p { class: "muted", "Navigation does not cancel a game or commit Leave or Exit." }
            }
        }
    }
}

fn command_destination(
    response: &GameResponse,
    id: GameId,
    context: ViewContext,
    exit_destination: Option<&str>,
) -> Option<String> {
    use brews_contracts::games::{GameOutcome, ParticipantChange, ParticipantIdentity};
    let outcome = match response {
        GameResponse::PlayerLeft { .. } => return Some("/".into()),
        GameResponse::Exited { receipt, .. }
        | GameResponse::ParticipantChanged { receipt }
        | GameResponse::Committed { receipt } => &receipt.outcome,
        _ => return None,
    };
    match outcome {
        GameOutcome::PlayerLeft { .. }
        | GameOutcome::ParticipantChanged {
            change: ParticipantChange::Left,
            ..
        } => Some("/".into()),
        GameOutcome::ParticipantChanged {
            change: ParticipantChange::Switched,
            identity,
            ..
        } => {
            let role = match identity {
                ParticipantIdentity::Player { .. } => "player",
                ParticipantIdentity::Spectator { .. } => "spectator",
            };
            Some(format!("/games/{id}/{role}"))
        }
        GameOutcome::Exited {} => Some(
            exit_destination
                .unwrap_or(if context == ViewContext::Account {
                    "/games"
                } else {
                    "/"
                })
                .into(),
        ),
        _ => None,
    }
}

fn call_snapshot_current(response: &GameResponse, observed: Option<u64>) -> bool {
    call_revision(response)
        .is_some_and(|revision| observed.is_some_and(|observed| observed >= revision))
}

fn call_revision(response: &GameResponse) -> Option<u64> {
    match response {
        GameResponse::CallAccepted { view_revision, .. } => Some(*view_revision),
        GameResponse::Committed { receipt } => match receipt.outcome {
            brews_contracts::games::GameOutcome::CallAccepted { view_revision, .. } => {
                Some(view_revision)
            }
            _ => None,
        },
        _ => None,
    }
}

fn cancelled_before_start(response: &GameResponse) -> bool {
    use brews_domain::games::GameState;
    match response {
        GameResponse::Terminalized {
            expected_state: GameState::New | GameState::AwaitingPlayers,
            state: GameState::Cancelled,
            ..
        } => true,
        GameResponse::Committed { receipt } => matches!(
            receipt.outcome,
            brews_contracts::games::GameOutcome::Terminalized {
                expected_state: GameState::New | GameState::AwaitingPlayers,
                state: GameState::Cancelled,
                ..
            }
        ),
        _ => false,
    }
}

#[component]
fn CodeBadge(code: String) -> Element {
    let mut error = use_signal(|| None::<String>);
    let mut copied = use_signal(|| false);
    rsx! {
        div { class: "card game-code-card",
            div { class: "game-code-content",
                span { class: "eyebrow", "Game code" }
                strong { class: "game-code", "{code}" }
            }
            button { class: "button secondary code-copy-button", r#type: "button", "aria-label": if copied() { "Game code copied" } else { "Copy game code" }, onclick: move |_| { let code=code.clone(); async move { match crate::client::copy_text(&code).await { Ok(()) => { copied.set(true); error.set(None); }, Err(reason) => error.set(Some(reason.to_string())) } } }, if copied() { "Copied" } else { "Copy code" } }
            if let Some(message) = error() { Notice { message } }
        }
    }
}

#[component]
fn Rules(configuration: brews_domain::games::GameConfiguration) -> Element {
    rsx! { section { class: "card stack",
        h2 { "The rules" }
        p { "Values 1–{configuration.numeric_upper_bound} · {configuration.board_side_length} × {configuration.board_side_length} board" }
        p { "Complete one row, column, or diagonal. Matches and qualification are automatic." }
        p { "Player capacity: {configuration.player_capacity} · Spectator capacity: {configuration.spectator_capacity}" }
        p { "Free cells: {configuration.free_cell_positions.len()}" }
    } }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "Fixed contract fixture.")]
mod tests {
    use super::*;
    use brews_contracts::games::{
        GameOutcome, GameReceipt, ParticipantChange, ParticipantIdentity,
    };

    const ID: &str = "01890f3e-53b7-7d28-9b05-4f65092d5711";
    fn receipt(outcome: GameOutcome) -> GameReceipt {
        GameReceipt {
            version: 1,
            game_id: ID.parse().unwrap(),
            command_id: "01890f3e-53b7-7d28-9b05-4f65092d5712".parse().unwrap(),
            outcome,
            completed_at: 1,
            expires_at: 86_400_001,
        }
    }

    #[test]
    fn delayed_switch_success_routes_without_a_source_snapshot() {
        for (identity, role) in [
            (
                ParticipantIdentity::Spectator {
                    spectator_id: ID.parse().unwrap(),
                },
                "spectator",
            ),
            (
                ParticipantIdentity::Player {
                    player_id: ID.parse().unwrap(),
                    alias: "Alice".into(),
                },
                "player",
            ),
        ] {
            let receipt = receipt(GameOutcome::ParticipantChanged {
                change: ParticipantChange::Switched,
                identity,
                session_expires_at: Some(200),
                view_revision: 3,
            });
            // Navigation uses only the verified response, even if revalidation
            // denied access and removed the source panel while HTTP was pending.
            for response in [
                GameResponse::ParticipantChanged {
                    receipt: receipt.clone(),
                },
                GameResponse::Committed { receipt },
            ] {
                assert_eq!(
                    command_destination(&response, ID.parse().unwrap(), ViewContext::Player, None),
                    Some(format!("/games/{ID}/{role}"))
                );
            }
        }
    }

    #[test]
    fn departure_pauses_streams_without_treating_pending_as_success() {
        let pending = GameResponse::Pending {
            operation_id: ID.parse().unwrap(),
        };
        for action in [
            "membership/leave",
            "membership/player",
            "membership/spectator",
            "exit?view=player",
        ] {
            let paused = StreamState::Open.before_command(action);
            assert_eq!(paused, StreamState::Paused);
            assert_eq!(
                command_destination(&pending, ID.parse().unwrap(), ViewContext::Player, None),
                None
            );
            // Ordinary refresh/command processing cannot restore a departed stream.
            assert_eq!(paused.before_command("calls/random"), StreamState::Paused);
        }
        assert_eq!(
            StreamState::Open.before_command("calls/random"),
            StreamState::Open
        );
        let left = GameResponse::Committed {
            receipt: receipt(GameOutcome::PlayerLeft {
                left: true,
                expected_state: brews_domain::games::GameState::InProgress,
                membership_retained: true,
                fresh_admission_required: false,
                player_id: ID.parse().unwrap(),
                session_expires_at: Some(200),
                view_revision: 3,
            }),
        };
        assert_eq!(
            command_destination(&left, ID.parse().unwrap(), ViewContext::Player, None),
            Some("/".into())
        );
    }

    #[test]
    fn exit_receipt_keeps_the_parent_owned_destination() {
        let receipt = receipt(GameOutcome::Exited {});
        for response in [
            GameResponse::Exited {
                game_id: ID.parse().unwrap(),
                receipt: receipt.clone(),
            },
            GameResponse::Committed { receipt },
        ] {
            assert_eq!(
                command_destination(
                    &response,
                    ID.parse().unwrap(),
                    ViewContext::Account,
                    Some("/history/example")
                ),
                Some("/history/example".into())
            );
            assert_eq!(
                command_destination(&response, ID.parse().unwrap(), ViewContext::Player, None),
                Some("/".into())
            );
        }
    }

    #[test]
    fn observed_call_receipt_does_not_require_stream_reconnection() {
        let response: GameResponse = serde_json::from_value(serde_json::json!({
            "result": "committed",
            "receipt": {
                "version": 1,
                "command_id": "01890f3e-53b7-7d28-9b05-4f65092d5712",
                "game_id": "01890f3e-53b7-7d28-9b05-4f65092d5711",
                "outcome": { "operation": "call_accepted", "call": { "sequence_no": 1, "value": "1" }, "remaining_count": 8, "exhausted": false, "view_revision": 4 },
                "completed_at": 1,
                "expires_at": 86_400_001
            }
        })).unwrap();
        assert!(call_snapshot_current(&response, Some(4)));
        assert!(call_snapshot_current(&response, Some(5)));
        assert!(!call_snapshot_current(&response, Some(3)));
        assert!(!call_snapshot_current(&response, None));
    }
}
