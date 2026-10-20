//! One owned connection per mounted game/context; dropping its resource closes it.
use super::{
    GameContext, StreamState,
    model::{self, Connection, GameUiError, ViewContext},
};
use crate::{client, session::AccountSession};
use brews_contracts::{
    discovery::{CurrentGame, GameAvailability},
    games::{
        GameView, SyncResponse,
        reads::{Membership, MembershipRead},
    },
};
use dioxus::prelude::*;

#[cfg(target_arch = "wasm32")]
const FRESHNESS_MS: i64 = 20_000;

struct Revalidated {
    view: GameView,
    expires_at: i64,
    may_operate: bool,
    availability: Option<GameAvailability>,
}

enum RevalidationError {
    Http(client::ClientError),
    View(GameUiError),
}
impl From<client::ClientError> for RevalidationError {
    fn from(error: client::ClientError) -> Self {
        Self::Http(error)
    }
}
impl From<GameUiError> for RevalidationError {
    fn from(error: GameUiError) -> Self {
        Self::View(error)
    }
}
impl RevalidationError {
    fn apply(self, game: GameContext) -> GameUiError {
        match self {
            Self::Http(error) => denied(game, error),
            Self::View(GameUiError::Expired) => {
                game.clear(Connection::Expired);
                GameUiError::Expired
            }
            Self::View(error) => error,
        }
    }
}

// Stage all HTTP reads: a pending read must not clear a socket notice or
// overwrite snapshots received while its response was in flight.
async fn revalidate(
    mut game: GameContext,
    account: AccountSession,
) -> Result<Revalidated, RevalidationError> {
    let previous = game.snapshot.peek().clone();
    client::prepare_command_clock().await?;
    let context = (game.context)();
    let mut member = None;
    let mut expires_at = if context == ViewContext::Account {
        if !account.normal() {
            return Err(GameUiError::Context.into());
        }
        account
            .session
            .read()
            .as_ref()
            .map(|s| s.expires_at)
            .ok_or(GameUiError::Context)?
    } else {
        let membership = client::get::<MembershipRead>(&format!(
            "{}?view={}",
            game.path("membership"),
            context.as_str()
        ))
        .await?;
        let expiry = match (context, &membership.membership) {
            (
                ViewContext::Player,
                Membership::Player {
                    session_expires_at, ..
                },
            )
            | (
                ViewContext::Spectator,
                Membership::Spectator {
                    session_expires_at, ..
                },
            ) => *session_expires_at,
            _ => return Err(GameUiError::Context.into()),
        };
        member = Some(membership.membership);
        expiry
    };
    // Tighten known access bounds immediately; remaining HTTP reads cannot
    // postpone the independent expiry watchdog.
    game.deadline.set(Some(model::cap(
        (game.deadline)(),
        expires_at,
        client::now_ms(),
    )?));
    let response =
        client::get::<SyncResponse>(&format!("{}?view={}", game.path("sync"), context.as_str()))
            .await?;
    response.validate().map_err(|_| GameUiError::Snapshot)?;
    let view = response.snapshot.ok_or(GameUiError::Snapshot)?;
    if view.game_id() != (game.id)() || !context.accepts(&view) {
        return Err(GameUiError::Context.into());
    }
    let identity_matches = match (&member, &view) {
        (
            Some(Membership::Player {
                player_id: expected,
                ..
            }),
            GameView::Player { player_id, .. } | GameView::FinalPlayer { player_id, .. },
        ) => expected == player_id,
        (
            Some(Membership::Spectator {
                spectator_id: expected,
                ..
            }),
            GameView::Spectator { spectator_id, .. },
        ) => expected == spectator_id,
        (None, GameView::Host { .. } | GameView::FinalHost { .. }) => true,
        _ => false,
    };
    if !identity_matches {
        return Err(GameUiError::Context.into());
    }
    if let Some(previous) = previous.as_ref() {
        model::validate_transition(previous, &view)?;
    }
    if let GameView::FinalHost {
        history_expires_at, ..
    }
    | GameView::FinalPlayer {
        history_expires_at, ..
    } = &view
    {
        expires_at = expires_at.min(*history_expires_at);
        game.deadline.set(Some(model::cap(
            (game.deadline)(),
            expires_at,
            client::now_ms(),
        )?));
    }
    let may_operate = if context == ViewContext::Account && !model::terminal(&view) {
        let current = client::get::<CurrentGame>("/api/games/current").await?;
        current
            .game
            .as_ref()
            .is_some_and(|g| g.game_id == (game.id)() && g.may_operate)
    } else {
        false
    };
    let availability = if published(model::state(&view)) {
        Some(
            client::get::<GameAvailability>(&format!(
                "{}?view={}",
                game.path("availability"),
                context.as_str()
            ))
            .await?,
        )
    } else {
        None
    };
    Ok(Revalidated {
        view,
        expires_at,
        may_operate,
        availability,
    })
}

fn apply_revalidation(mut game: GameContext, result: Revalidated) -> Result<u64, GameUiError> {
    let revision = result.view.view_revision();
    let apply = model::validate_http_snapshot(game.snapshot.peek().as_ref(), &result.view)?;
    let deadline = model::cap((game.deadline)(), result.expires_at, client::now_ms())?;
    game.deadline.set(Some(deadline));
    if apply {
        game.idle_due.set(match &result.view {
            GameView::Host {
                idle_cancel_due_at, ..
            } => *idle_cancel_due_at,
            _ => None,
        });
        game.snapshot.set(Some(result.view));
    }
    game.may_operate.set(result.may_operate);
    game.availability.set(
        if game
            .snapshot
            .peek()
            .as_ref()
            .is_some_and(|view| published(model::state(view)))
        {
            result.availability
        } else {
            None
        },
    );
    game.error.set(None);
    Ok(revision)
}

async fn synchronize(mut game: GameContext, account: AccountSession) -> Result<(), GameUiError> {
    game.status.set(Connection::Synchronizing);
    game.may_operate.set(false);
    let result = revalidate(game, account)
        .await
        .map_err(|error| error.apply(game))?;
    apply_revalidation(game, result)?;
    Ok(())
}

#[cfg(any(target_arch = "wasm32", test))]
enum TransportEvent<M, H> {
    Socket(Option<M>),
    Http(H),
    Tick,
}

// Poll the socket first, including when HTTP has completed with an error.
#[cfg(any(target_arch = "wasm32", test))]
async fn next_event<S, H, T>(
    socket: &mut S,
    http: Option<std::pin::Pin<&mut H>>,
    timer: T,
) -> TransportEvent<S::Item, H::Output>
where
    S: futures_util::Stream + Unpin,
    H: std::future::Future + ?Sized,
    T: std::future::Future<Output = ()>,
{
    use futures_util::{
        StreamExt,
        future::{Either, select},
    };
    let incoming = socket.next();
    let response = async {
        match http {
            Some(http) => http.await,
            None => std::future::pending().await,
        }
    };
    futures_util::pin_mut!(incoming, response, timer);
    let other = select(response, timer);
    futures_util::pin_mut!(other);
    match select(incoming, other).await {
        Either::Left((message, _)) => TransportEvent::Socket(message),
        Either::Right((Either::Left((result, _)), _)) => TransportEvent::Http(result),
        Either::Right((Either::Right(_), _)) => TransportEvent::Tick,
    }
}

fn published(state: brews_domain::games::GameState) -> bool {
    matches!(
        state,
        brews_domain::games::GameState::AwaitingPlayers
            | brews_domain::games::GameState::InProgress
    )
}

#[cfg(target_arch = "wasm32")]
fn expired(game: GameContext) -> bool {
    if model::access_expired((game.deadline)(), (game.idle_due)(), client::now_ms()) {
        game.clear(Connection::Expired);
        true
    } else {
        false
    }
}

// Owned by the mounted screen, independently of reconnect/request progress.
#[cfg(target_arch = "wasm32")]
pub(super) async fn watch_expiry(game: GameContext) {
    loop {
        if expired(game) {
            return;
        }
        gloo_timers::future::TimeoutFuture::new(250).await;
    }
}

fn denied(mut game: GameContext, error: client::ClientError) -> GameUiError {
    game.error.set(Some(error.to_string()));
    game.clear(if error.unauthorized() {
        Connection::Denied
    } else {
        Connection::Reconnecting
    });
    if error.unauthorized() {
        GameUiError::Context
    } else {
        GameUiError::Transport
    }
}

pub(super) async fn run(game: GameContext, account: AccountSession) {
    if (game.stream)() == StreamState::Paused {
        return;
    }
    #[cfg(target_arch = "wasm32")]
    browser::run(game, account).await;
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut game = game;
        let _ = synchronize(game, account).await;
        game.status.set(Connection::Unsupported);
    }
}

#[cfg(target_arch = "wasm32")]
mod browser {
    use super::*;
    use brews_contracts::games::{SnapshotAck, SnapshotAckKind, SnapshotFrame};
    use futures_util::{FutureExt, SinkExt};
    use gloo_net::websocket::{Message, WebSocketError, futures::WebSocket};
    use gloo_timers::future::TimeoutFuture;

    const RECONNECT_ATTEMPTS: u32 = 8;
    const TICK_MS: u32 = 1_000;
    const INITIAL_SNAPSHOT_TIMEOUT_MS: i64 = 10_000;

    fn visible() -> bool {
        web_sys::window()
            .and_then(|w| w.document())
            .is_some_and(|d| !d.hidden())
    }
    fn url(game: GameContext) -> Result<String, GameUiError> {
        let location = web_sys::window().ok_or(GameUiError::Transport)?.location();
        if location.protocol().map_err(|_| GameUiError::Transport)? != "https:" {
            return Err(GameUiError::Transport);
        }
        Ok(format!(
            "wss://{}{}?view={}",
            location.host().map_err(|_| GameUiError::Transport)?,
            game.path("stream"),
            (game.context)().as_str()
        ))
    }

    pub(super) async fn run(mut game: GameContext, account: AccountSession) {
        let mut failures = 0;
        loop {
            if (game.stream)() == StreamState::Paused {
                return;
            }
            while (game.context)() == ViewContext::Account && !(account.ready)() {
                game.clear(Connection::Synchronizing);
                TimeoutFuture::new(TICK_MS).await;
                if expired(game) {
                    return;
                }
            }
            while !visible() {
                game.clear(Connection::Reconnecting);
                TimeoutFuture::new(TICK_MS).await;
            }
            if (game.stream)() == StreamState::Paused {
                return;
            }
            match synchronize(game, account).await {
                Ok(_) => {}
                Err(error) => {
                    if !retry(game, error, &mut failures).await {
                        return;
                    }
                    continue;
                }
            }
            if (game.stream)() == StreamState::Paused {
                return;
            }
            let is_final = game.snapshot.read().as_ref().is_some_and(model::terminal);
            if is_final {
                game.status.set(Connection::Live);
                // Final reads do not create or extend a grant. Revalidate until Exit/expiry.
                for _ in 0..(FRESHNESS_MS as u32 / TICK_MS) {
                    TimeoutFuture::new(TICK_MS).await;
                    if (game.stream)() == StreamState::Paused {
                        return;
                    }
                    if expired(game) {
                        return;
                    }
                    if !visible() {
                        break;
                    }
                }
                continue;
            }
            let address = match url(game) {
                Ok(url) => url,
                Err(error) => {
                    game.error.set(Some(error.to_string()));
                    game.clear(Connection::Unsupported);
                    return;
                }
            };
            let socket = WebSocket::open(&address);
            let Ok(mut socket) = socket else {
                if !retry(game, GameUiError::Transport, &mut failures).await {
                    return;
                }
                continue;
            };
            let mut connection_id = None;
            let mut checked_at = client::now_ms();
            let mut http_revision = game
                .snapshot
                .peek()
                .as_ref()
                .map_or(0, GameView::view_revision);
            let mut revalidation: Option<
                futures_util::future::LocalBoxFuture<
                    'static,
                    Result<Revalidated, RevalidationError>,
                >,
            > = None;
            let outcome = loop {
                let event = next_event(
                    &mut socket,
                    revalidation.as_mut().map(|http| http.as_mut()),
                    TimeoutFuture::new(TICK_MS),
                )
                .await;
                if (game.stream)() == StreamState::Paused {
                    return;
                }
                if expired(game) {
                    break Err(GameUiError::Expired);
                }
                if !visible() {
                    game.clear(Connection::Reconnecting);
                    break Ok(());
                }
                if (game.context)() == ViewContext::Account && !account.normal() {
                    if !(account.ready)() {
                        game.clear(Connection::Synchronizing);
                        break Ok(());
                    }
                    game.clear(Connection::Denied);
                    break Err(GameUiError::Context);
                }
                match event {
                    TransportEvent::Socket(Some(Ok(Message::Text(text)))) => {
                        let frame = SnapshotFrame::decode_json(text.as_bytes());
                        if let Ok(frame) = frame {
                            let validation = model::validate_stream_frame(
                                &frame,
                                (game.id)(),
                                (game.context)(),
                                game.snapshot.read().as_ref(),
                                http_revision,
                                client::now_ms(),
                            );
                            let apply = match validation {
                                Ok(apply) => apply,
                                Err(error) => break Err(error),
                            };
                            if connection_id.is_some_and(|id| id != frame.connection_id) {
                                break Err(GameUiError::Snapshot);
                            }
                            connection_id = Some(frame.connection_id);
                            let deadline = match model::cap(
                                (game.deadline)(),
                                match &frame.view {
                                    GameView::FinalHost {
                                        history_expires_at, ..
                                    }
                                    | GameView::FinalPlayer {
                                        history_expires_at, ..
                                    } => frame.session_expires_at.min(*history_expires_at),
                                    _ => frame.session_expires_at,
                                },
                                client::now_ms(),
                            ) {
                                Ok(deadline) => deadline,
                                Err(error) => break Err(error),
                            };
                            game.deadline.set(Some(deadline));
                            let ack = SnapshotAck {
                                version: 1,
                                kind: SnapshotAckKind::SnapshotAck,
                                connection_id: frame.connection_id,
                                delivery_id: frame.delivery_id,
                                view_revision: frame.view_revision,
                            };
                            let ack = match ack
                                .encode_json()
                                .ok()
                                .and_then(|bytes| String::from_utf8(bytes).ok())
                            {
                                Some(ack) => ack,
                                None => break Err(GameUiError::Snapshot),
                            };
                            if socket.send(Message::Text(ack)).await.is_err() {
                                break Err(GameUiError::Transport);
                            }
                            if (game.stream)() == StreamState::Paused {
                                return;
                            }
                            if apply {
                                let assignment_changed = matches!((game.snapshot.read().as_ref(),&frame.view),(Some(GameView::Host{game:previous,..}),GameView::Host{game:next,..}) if previous.designated_host_id!=next.designated_host_id);
                                game.idle_due.set(match &frame.view {
                                    GameView::Host {
                                        idle_cancel_due_at, ..
                                    } => *idle_cancel_due_at,
                                    _ => None,
                                });
                                game.snapshot.set(Some(frame.view));
                                if assignment_changed {
                                    game.may_operate.set(false);
                                    // Revalidate the new assignment without dropping queued closes.
                                    revalidation = None;
                                    checked_at = client::now_ms() - FRESHNESS_MS;
                                }
                                if revalidation.is_none() && !assignment_changed {
                                    game.status.set(Connection::Live);
                                    failures = 0;
                                }
                            }
                        } else {
                            let previous = game.snapshot.read().clone();
                            let Some(previous) = previous else {
                                break Err(GameUiError::Snapshot);
                            };
                            match model::TerminalNotice::decode(
                                &text,
                                (game.id)(),
                                &previous,
                                client::now_ms(),
                            ) {
                                Ok(notice) if (game.context)() == ViewContext::Spectator => {
                                    game.snapshot.set(None);
                                    game.notice.set(Some(notice));
                                    game.status.set(Connection::Ended);
                                    drop(revalidation.take());
                                    drop(socket);
                                    // Audience has no final read/reconnect grant.
                                    loop {
                                        TimeoutFuture::new(TICK_MS).await;
                                        if expired(game) {
                                            return;
                                        }
                                    }
                                }
                                _ => break Err(GameUiError::Snapshot),
                            }
                        }
                    }
                    TransportEvent::Socket(Some(Err(WebSocketError::ConnectionClose(close))))
                        if close.code == 1008 =>
                    {
                        game.stream.set(StreamState::Paused);
                        game.command.retire_body();
                        game.clear(Connection::Paused);
                        game.error.set(Some("The server closed or replaced this connection. Restore explicitly to revalidate access.".into()));
                        return;
                    }
                    TransportEvent::Socket(_) => break Err(GameUiError::Transport),
                    TransportEvent::Http(result) => {
                        revalidation = None;
                        let result = match result {
                            Ok(result) => result,
                            Err(error) => break Err(error.apply(game)),
                        };
                        match apply_revalidation(game, result) {
                            Ok(revision) => {
                                checked_at = client::now_ms();
                                http_revision = revision;
                                game.status.set(Connection::Live);
                            }
                            Err(error) => break Err(error),
                        }
                    }
                    TransportEvent::Tick => {}
                }
                if connection_id.is_none()
                    && client::now_ms() - checked_at >= INITIAL_SNAPSHOT_TIMEOUT_MS
                {
                    break Err(GameUiError::Transport);
                }
                if revalidation.is_none() && client::now_ms() - checked_at >= FRESHNESS_MS {
                    game.status.set(Connection::Synchronizing);
                    game.may_operate.set(false);
                    revalidation = Some(revalidate(game, account).boxed_local());
                }
            };
            drop(socket);
            match outcome {
                Ok(()) => continue,
                Err(error) => {
                    if !retry(game, error, &mut failures).await {
                        return;
                    }
                }
            }
        }
    }
    async fn retry(mut game: GameContext, error: GameUiError, failures: &mut u32) -> bool {
        if (game.stream)() == StreamState::Paused {
            return false;
        }
        if expired(game) || matches!(error, GameUiError::Context | GameUiError::Expired) {
            game.clear(if error == GameUiError::Expired {
                Connection::Expired
            } else {
                Connection::Denied
            });
            return false;
        }
        game.status.set(Connection::Reconnecting);
        game.may_operate.set(false);
        game.error.set(Some(error.to_string()));
        *failures += 1;
        if *failures >= RECONNECT_ATTEMPTS {
            game.error.set(Some(
                "Automatic reconnect paused. Reconnect explicitly when the service is available."
                    .into(),
            ));
            return false;
        }
        let delay = reconnect_delay(*failures, js_sys::Math::random());
        for _ in 0..delay.div_ceil(TICK_MS) {
            TimeoutFuture::new(TICK_MS).await;
            if (game.stream)() == StreamState::Paused {
                return false;
            }
            if expired(game) {
                return false;
            }
        }
        true
    }
    fn reconnect_delay(attempt: u32, jitter: f64) -> u32 {
        ((1_000u32.saturating_mul(
            1u32.checked_shl(attempt.saturating_sub(1))
                .unwrap_or(u32::MAX),
        ))
        .min(29_000)
            + (jitter.clamp(0.0, 1.0) * 1_000.0) as u32)
            .min(30_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use brews_domain::games::GameState;

    use futures_util::{StreamExt, stream, task::noop_waker_ref};
    use std::{
        future::{Future, pending, ready},
        task::{Context, Poll},
    };

    fn poll_ready<F: Future>(future: F) -> F::Output {
        let mut future = Box::pin(future);
        match future
            .as_mut()
            .poll(&mut Context::from_waker(noop_waker_ref()))
        {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("fixture should be ready"),
        }
    }

    #[test]
    fn socket_queue_precedes_a_completed_http_error() {
        for (queued, error) in [
            ("snapshot needing ACK", client::ClientError::Unavailable),
            ("policy close 1008", client::ClientError::Unavailable),
            ("terminal notice", client::ClientError::Unauthorized),
        ] {
            let mut socket = stream::iter([queued]).chain(stream::pending());
            let mut http = Box::pin(ready(Err::<(), _>(error)));
            assert!(matches!(
                poll_ready(next_event(&mut socket, Some(http.as_mut()), pending())),
                TransportEvent::Socket(Some(message)) if message == queued
            ));
            assert!(matches!(
                poll_ready(next_event(&mut socket, Some(http.as_mut()), pending())),
                TransportEvent::Http(Err(actual)) if actual == error
            ));
        }
    }

    #[test]
    fn pending_revalidation_keeps_socket_and_expiry_ticks_running() {
        let mut socket =
            stream::iter(["snapshot", "ACK followup", "policy close"]).chain(stream::pending());
        let mut http = Box::pin(pending::<()>());
        for expected in ["snapshot", "ACK followup", "policy close"] {
            assert!(matches!(
                poll_ready(next_event(&mut socket, Some(http.as_mut()), ready(()))),
                TransportEvent::Socket(Some(message)) if message == expected
            ));
        }
        assert!(matches!(
            poll_ready(next_event(&mut socket, Some(http.as_mut()), ready(()))),
            TransportEvent::Tick
        ));
    }

    #[test]
    fn availability_is_only_requested_after_publication() {
        assert!(!published(GameState::New));
        assert!(published(GameState::AwaitingPlayers));
        assert!(published(GameState::InProgress));
        assert!(!published(GameState::Resolved));
        assert!(!published(GameState::Cancelled));
    }
}
