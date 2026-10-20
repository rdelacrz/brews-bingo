//! Allocation-conscious, component-scoped command retention. Bodies are erased on drop.
use crate::client::{self, ClientError};
use brews_contracts::games::{
    GameResponse,
    commands::{CommandResult, CommandResultStatus},
};
use dioxus::prelude::*;
use serde_json::Value;
use std::{cell::RefCell, rc::Rc};
use zeroize::{Zeroize, Zeroizing};

struct Original {
    id: String,
    method: String,
    path: String,
    body: RefCell<Option<Zeroizing<String>>>,
    created_at: i64,
}

impl Original {
    fn retire_body(&self) {
        drop(self.body.borrow_mut().take());
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(super) struct GameCommand {
    original: Signal<Option<Rc<Original>>>,
    pub busy: Signal<bool>,
    pub error: Signal<Option<String>>,
    pub result: Signal<Option<GameResponse>>,
    pub denied: Signal<bool>,
    pub needs_refresh: Signal<bool>,
    unknown: Signal<bool>,
}

pub(super) fn use_game_command() -> GameCommand {
    let mut original = use_signal(|| None);
    use_drop(move || {
        if let Ok(mut original) = original.try_write() {
            *original = None;
        }
    });
    GameCommand {
        original,
        busy: use_signal(|| false),
        error: use_signal(|| None),
        result: use_signal(|| None),
        denied: use_signal(|| false),
        needs_refresh: use_signal(|| false),
        unknown: use_signal(|| false),
    }
}

impl GameCommand {
    pub fn blocked(self) -> bool {
        (self.busy)() || self.original.read().is_some()
    }
    pub fn retained(self) -> bool {
        self.original.read().is_some()
    }

    pub async fn send(mut self, method: &str, path: &str, mut body: Value) -> Option<GameResponse> {
        if self.blocked() {
            erase(&mut body);
            return None;
        }
        self.busy.set(true);
        if let Err(error) = client::prepare_command_clock().await {
            self.busy.set(false);
            self.error.set(Some(error.to_string()));
            erase(&mut body);
            return None;
        }
        let id = match client::new_command_id() {
            Ok(id) => id,
            Err(error) => {
                self.busy.set(false);
                self.error.set(Some(error.to_string()));
                erase(&mut body);
                return None;
            }
        };
        let serialized = serde_json::to_string(&body);
        erase(&mut body);
        let Ok(body) = serialized else {
            self.busy.set(false);
            self.error
                .set(Some("The command could not be prepared.".to_string()));
            return None;
        };
        self.original.set(Some(Rc::new(Original {
            id,
            method: method.into(),
            path: path.into(),
            body: RefCell::new(Some(Zeroizing::new(body))),
            created_at: client::now_ms(),
        })));
        self.unknown.set(false);
        self.denied.set(false);
        self.needs_refresh.set(false);
        self.error.set(None);
        self.result.set(None);
        self.busy.set(false);
        self.execute().await
    }

    pub async fn retry(self) -> Option<GameResponse> {
        if (self.busy)() {
            return None;
        }
        self.execute().await
    }

    async fn execute(mut self) -> Option<GameResponse> {
        let original = self.original.read().clone()?;
        if original.body.borrow().is_none() {
            self.error.set(Some("Access ended and private request material was cleared. Check the original result if your current authority permits it.".to_string()));
            return None;
        }
        if client::now_ms() < original.created_at
            || client::now_ms() - original.created_at
                >= brews_contracts::games::GAME_RECEIPT_RETENTION_MS
        {
            self.error.set(Some("This command is too old to retry safely. Revalidate access and inspect the game; do not repeat its effect.".to_string()));
            return None;
        }
        let mut body = {
            let retained = original.body.borrow();
            serde_json::from_str::<Value>(retained.as_ref()?).ok()?
        };
        self.busy.set(true);
        self.error.set(None);
        let response = client::request::<GameResponse>(
            &original.method,
            &original.path,
            &body,
            Some(&original.id),
        )
        .await;
        erase(&mut body);
        self.busy.set(false);
        match response {
            Ok(response)
                if response.validate().is_ok() && matches_original(&response, &original) =>
            {
                if matches!(response, GameResponse::Pending { .. }) {
                    self.unknown.set(true);
                    self.error.set(Some("The server is coordinating this command. Check its result or retry the original request.".to_string()));
                } else {
                    self.original.set(None);
                }
                self.result.set(Some(response.clone()));
                Some(response)
            }
            Ok(_) => {
                self.unknown.set(true);
                self.error.set(Some("The response could not be verified. The command outcome is unknown; retry the original request.".to_string()));
                None
            }
            Err(error) => {
                self.reject(error);
                None
            }
        }
    }

    pub fn retire_body(self) {
        if let Some(original) = self.original.read().as_ref() {
            original.retire_body();
        }
    }

    fn reject(mut self, error: ClientError) {
        self.denied.set(error.unauthorized());
        self.needs_refresh.set(matches!(
            error,
            ClientError::Conflict | ClientError::StaleCommand
        ));
        if !retain_after_error(error, (self.unknown)()) {
            self.original.set(None);
        }
        if error.unauthorized() || error == ClientError::NotFound {
            self.retire_body();
        }
        if error.uncertain() {
            self.unknown.set(true);
        }
        self.error.set(Some(error.to_string()));
    }

    pub async fn check(mut self, game_id: &str, view: &str) {
        if (self.busy)() {
            return;
        }
        let Some(original) = self.original.read().clone() else {
            return;
        };
        self.busy.set(true);
        let result = client::get::<CommandResult>(&format!(
            "/api/games/{game_id}/commands/{}?view={view}",
            original.id
        ))
        .await;
        self.busy.set(false);
        match result {
            Ok(result) if result.validate().is_ok() && result.game_id.to_string() == game_id && result.command_id.to_string() == original.id => match result.result_status {
                CommandResultStatus::Committed { receipt, follow_up_pending: false } => {
                    self.original.set(None);
                    self.error.set(None);
                    self.result.set(Some(GameResponse::Committed { receipt }));
                }
                CommandResultStatus::Committed { follow_up_pending: true, .. } | CommandResultStatus::Pending { .. } => self.error.set(Some("The command is recorded; coordination is still pending. Check again or retry the original request.".to_string())),
                CommandResultStatus::Unknown {} => self.error.set(Some("No result is available. This does not prove failure. Retry only the original request.".to_string())),
                CommandResultStatus::Expired {} | CommandResultStatus::Inadmissible {} => self.error.set(Some("The original command can no longer be resolved here. Do not repeat its effect with a new command.".to_string())),
            },
            Ok(_) => self.error.set(Some("The command result could not be verified.".to_string())),
            Err(error) => {
                self.denied.set(error.unauthorized());
                if error.unauthorized() || error == ClientError::NotFound {
                    self.retire_body();
                }
                self.error.set(Some(error.to_string()));
            },
        }
    }
}

fn retain_after_error(error: ClientError, unknown: bool) -> bool {
    // A later rejection says nothing about whether an earlier attempt committed.
    unknown || error.uncertain()
}

pub(super) fn erase(value: &mut Value) {
    match value {
        Value::String(value) => value.zeroize(),
        Value::Array(values) => values.iter_mut().for_each(erase),
        Value::Object(values) => values.values_mut().for_each(erase),
        _ => {}
    }
}

#[component]
pub(super) fn Feedback(
    command: GameCommand,
    #[props(default)] game_id: String,
    #[props(default)] view: String,
    #[props(default = true)] enabled: bool,
) -> Element {
    let error = (command.error)();
    rsx! {
        if (command.busy)() {
            p {
                role: "status",
                "Sending command…"
            }
        }
        if let Some(message) = error {
            crate::components::Notice {
                message,
            }
        }
        if command.retained() {
            p {
                class: "muted",
                "Other commands are blocked until the original outcome is resolved."
            }
            div {
                class: "actions",
                button {
                    class: "button secondary",
                    disabled: (command.busy)() || !enabled,
                    onclick: move |_| async move {
                        if enabled {
                            command.retry().await;
                        }
                    },
                    "Retry original request"
                }
                if !game_id.is_empty() {
                    button {
                        class: "button ghost",
                        disabled: (command.busy)(),
                        onclick: move |_| {
                            let id = game_id.clone();
                            let view = view.clone();
                            async move {
                                command.check(&id, &view).await;
                            }
                        },
                        "Check original result"
                    }
                }
            }
        }
    }
}

fn matches_original(response: &GameResponse, original: &Original) -> bool {
    let receipt = match response {
        GameResponse::Pending { .. } => return true,
        GameResponse::AdmissionContext { .. } => return false,
        GameResponse::ConfigurationUpdated { receipt, .. }
        | GameResponse::Resumed { receipt, .. }
        | GameResponse::HostTransferred { receipt, .. }
        | GameResponse::ParticipantChanged { receipt, .. }
        | GameResponse::Exited { receipt, .. }
        | GameResponse::Created { receipt, .. }
        | GameResponse::LobbyOpened { receipt, .. }
        | GameResponse::Started { receipt, .. }
        | GameResponse::Committed { receipt, .. }
        | GameResponse::PlayerJoined { receipt, .. }
        | GameResponse::AliasRenamed { receipt, .. }
        | GameResponse::PlayerLeft { receipt, .. }
        | GameResponse::RecoveryAnswerChanged { receipt, .. }
        | GameResponse::CallAccepted { receipt, .. }
        | GameResponse::Terminalized { receipt, .. } => receipt,
    };
    if receipt.command_id.to_string() != original.id {
        return false;
    }
    original.path == "/api/games"
        || original
            .path
            .strip_prefix("/api/games/")
            .and_then(|path| path.split('/').next())
            .is_some_and(|id| id == receipt.game_id.to_string())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "Fixed test fixtures.")]
mod tests {
    use super::*;
    #[test]
    fn replay_rejection_never_resolves_an_unknown_original() {
        assert!(retain_after_error(ClientError::UnknownOutcome, false));
        assert!(retain_after_error(ClientError::Offline, true));
        assert!(retain_after_error(ClientError::BrowserUnavailable, true));
        assert!(!retain_after_error(ClientError::Offline, false));
        for error in [
            ClientError::Forbidden,
            ClientError::Unauthorized,
            ClientError::Conflict,
            ClientError::InvalidInput,
            ClientError::NotFound,
            ClientError::RateLimited,
        ] {
            assert!(retain_after_error(error, true));
            assert!(!retain_after_error(error, false));
        }
    }
    #[test]
    fn recursive_body_cleanup_erases_nested_material() {
        let mut body = serde_json::json!({"proof":"private test material","nested":[{"proof":"other test material"}]});
        erase(&mut body);
        assert_eq!(body["proof"], "");
        assert_eq!(body["nested"][0]["proof"], "");
    }
    #[test]
    fn retiring_body_erases_shared_retention_without_losing_lookup_identity() {
        let original = Rc::new(Original {
            id: "original-command".into(),
            method: "POST".into(),
            path: "/api/games/example/players".into(),
            body: RefCell::new(Some(Zeroizing::new("private test material".into()))),
            created_at: 1,
        });
        let in_flight = Rc::clone(&original);
        original.retire_body();
        assert!(in_flight.body.borrow().is_none());
        assert_eq!(in_flight.id, "original-command");
        assert_eq!(in_flight.created_at, 1);
    }
    #[test]
    fn response_is_bound_to_the_original_command_and_game() {
        use brews_contracts::games::{GameOutcome, GameReceipt};
        let game = "01890f3e-53b7-7d28-9b05-4f65092d5701";
        let command = "01890f3e-53b7-7d28-9b05-4f65092d5702";
        let mut original = Original {
            id: command.into(),
            method: "POST".into(),
            path: format!("/api/games/{game}/resume"),
            body: RefCell::new(Some(Zeroizing::new("{}".into()))),
            created_at: 1,
        };
        let response = GameResponse::Committed {
            receipt: GameReceipt {
                version: 1,
                command_id: command.parse().unwrap(),
                game_id: game.parse().unwrap(),
                outcome: GameOutcome::Resumed { view_revision: 1 },
                completed_at: 1,
                expires_at: 86_400_001,
            },
        };
        assert!(matches_original(&response, &original));
        original.retire_body();
        assert!(original.body.borrow().is_none());
        assert_eq!(original.id, command);
        assert!(matches_original(&response, &original));
        original.path = format!("/api/games/{command}/resume");
        assert!(!matches_original(&response, &original));
        original.path = format!("/api/games/{game}/resume");
        original.id = game.into();
        assert!(!matches_original(&response, &original));
    }
}
