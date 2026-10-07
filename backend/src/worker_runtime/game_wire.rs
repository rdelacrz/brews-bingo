//! Bounded private game envelopes. Parsed credentials are not authority proofs.
use crate::api::{GameOperation, GamePayload, GameViewSelector, GamesRequest};
use brews_contracts::games::{
    AdmissionContextInput, CallManualInput, CancelGameInput, CreateGame, GAME_BODY_MAX_BYTES,
    JoinPlayer, RevisionCommand, WinnerInput,
};
use brews_contracts::games::{GameResponse, SyncResponse};
use brews_domain::ids::{CommandId, GameId};
use serde::{Deserialize, Serialize};
use zeroize::Zeroize as _;

pub(super) const GAME_WIRE_MAX_BYTES: usize = 16_384;
// Workerd upgrade forwarding is GET-only and discards a subrequest body.
#[cfg(target_arch = "wasm32")]
pub(super) const STREAM_INGRESS_HEADER: &str = "X-Brews-Game-Ingress";

fn object<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct ObjectVisitor<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for ObjectVisitor<T> {
        type Value = T;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a JSON object")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(self, map: A) -> Result<T, A::Error> {
            T::deserialize(serde::de::value::MapAccessDeserializer::new(map))
        }
    }
    deserializer.deserialize_map(ObjectVisitor(std::marker::PhantomData))
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum GameCookieWire {
    None,
    Admission { token: String, expires_at: i64 },
    Player { token: String, expires_at: i64 },
}
impl<'de> Deserialize<'de> for GameCookieWire {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // A tagged unit variant ignores unknown fields; close the wire shape without
        // changing the owner/ingress cookie representation or serialized bytes.
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum CookieObject {
            None {},
            Admission { token: String, expires_at: i64 },
            Player { token: String, expires_at: i64 },
        }
        Ok(match CookieObject::deserialize(deserializer)? {
            CookieObject::None {} => Self::None,
            CookieObject::Admission { token, expires_at } => Self::Admission { token, expires_at },
            CookieObject::Player { token, expires_at } => Self::Player { token, expires_at },
        })
    }
}
impl Drop for GameCookieWire {
    fn drop(&mut self) {
        match self {
            Self::Admission { token, .. } | Self::Player { token, .. } => token.zeroize(),
            Self::None => {}
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum GameRejection {
    InvalidInput,
    Unauthorized,
    Forbidden,
    NotFound,
    Conflict,
    StaleCommand,
    Unavailable,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "owner_result", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum GameOwnerResponse {
    Success {
        #[serde(deserialize_with = "object")]
        response: GameResponse,
        #[serde(deserialize_with = "object")]
        cookie: GameCookieWire,
    },
    Synced {
        #[serde(deserialize_with = "object")]
        response: SyncResponse,
    },
    Rejected {
        code: GameRejection,
    },
}
impl GameOwnerResponse {
    pub(super) fn validate_for(&self, request: &GameIngress) -> Result<(), ()> {
        use brews_contracts::games::GameOutcome;
        let Self::Success { response, .. } = self else {
            if let Self::Synced { response } = self {
                if request.operation != GameAction::Sync
                    || (response.up_to_date
                        && request.known_revision != Some(response.view_revision))
                {
                    return Err(());
                }
                if let Some(view) = &response.snapshot {
                    if Some(view.game_id()) != request.game_id {
                        return Err(());
                    }
                    let matches_role = matches!(
                        (request.view, view),
                        (
                            Some(ViewSelector::Account),
                            brews_contracts::games::GameView::Host { .. }
                                | brews_contracts::games::GameView::FinalHost { .. }
                        ) | (
                            Some(ViewSelector::Player),
                            brews_contracts::games::GameView::Player { .. }
                                | brews_contracts::games::GameView::FinalPlayer { .. }
                        )
                    );
                    if !matches_role {
                        return Err(());
                    }
                }
            }
            return Ok(());
        };
        let receipt = match response {
            GameResponse::Created { game, receipt, .. }
                if request.operation == GameAction::Create =>
            {
                if Some(game.game_id) != request.game_id {
                    return Err(());
                }
                receipt
            }
            GameResponse::LobbyOpened {
                game_id, receipt, ..
            } if request.operation == GameAction::Lobby => {
                if Some(*game_id) != request.game_id {
                    return Err(());
                }
                receipt
            }
            GameResponse::Started {
                game_id, receipt, ..
            } if request.operation == GameAction::Start => {
                if Some(*game_id) != request.game_id {
                    return Err(());
                }
                receipt
            }
            GameResponse::CallAccepted { receipt, .. }
                if matches!(
                    request.operation,
                    GameAction::CallRandom | GameAction::CallManual
                ) =>
            {
                receipt
            }
            GameResponse::Terminalized {
                game_id, receipt, ..
            } if matches!(request.operation, GameAction::Winner | GameAction::Cancel) => {
                if Some(*game_id) != request.game_id {
                    return Err(());
                }
                receipt
            }
            GameResponse::PlayerJoined { receipt, alias, .. }
                if request.operation == GameAction::JoinPlayer =>
            {
                if let GamePayload::JoinPlayer(input) = request.payload()? {
                    if input.alias != *alias {
                        return Err(());
                    }
                } else {
                    return Err(());
                }
                receipt
            }
            GameResponse::Exited { game_id, receipt } if request.operation == GameAction::Exit => {
                if Some(*game_id) != request.game_id {
                    return Err(());
                }
                receipt
            }
            GameResponse::Committed { receipt } => receipt,
            GameResponse::AdmissionContext { .. }
                if request.operation == GameAction::AdmissionContext =>
            {
                return Ok(());
            }
            GameResponse::Pending { .. }
                if matches!(
                    request.operation,
                    GameAction::Create
                        | GameAction::Lobby
                        | GameAction::Start
                        | GameAction::Winner
                        | GameAction::Cancel
                ) =>
            {
                return Ok(());
            }
            _ => return Err(()),
        };
        if Some(receipt.game_id) != request.game_id
            || Some(receipt.command_id) != request.command_id
        {
            return Err(());
        }
        let coherent = match (request.operation, &receipt.outcome) {
            (GameAction::Create, GameOutcome::Created { .. })
            | (GameAction::Lobby, GameOutcome::LobbyOpened { .. })
            | (GameAction::Start, GameOutcome::Started { .. })
            | (GameAction::JoinPlayer, GameOutcome::PlayerJoined { .. })
            | (GameAction::CallRandom, GameOutcome::CallAccepted { .. })
            | (GameAction::Exit, GameOutcome::Exited {}) => true,
            (GameAction::CallManual, GameOutcome::CallAccepted { call, .. }) => {
                matches!(request.payload()?, GamePayload::CallManual(input) if input.value == call.value)
            }
            (
                GameAction::Winner,
                GameOutcome::Terminalized {
                    expected_state,
                    state,
                    winner,
                    ..
                },
            ) => {
                *expected_state == brews_domain::games::GameState::InProgress
                    && *state == brews_domain::games::GameState::Resolved
                    && matches!(request.payload()?, GamePayload::Winner(input) if winner.as_ref().is_some_and(|selected| selected.player_id == input.player_id))
            }
            (
                GameAction::Cancel,
                GameOutcome::Terminalized {
                    expected_state,
                    state,
                    winner,
                    ..
                },
            ) => {
                *state == brews_domain::games::GameState::Cancelled
                    && winner.is_none()
                    && matches!(request.payload()?, GamePayload::Cancel(input) if input.expected_state == *expected_state && input.confirmed)
            }
            _ => false,
        };
        if !coherent {
            return Err(());
        }
        Ok(())
    }
    pub(super) fn decode_json(bytes: &[u8]) -> Result<Self, ()> {
        if bytes.len() > brews_contracts::games::GAME_FRAME_MAX_BYTES + GAME_WIRE_MAX_BYTES
            || bytes
                .iter()
                .find(|b| !matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
                != Some(&b'{')
        {
            return Err(());
        }
        let value: Self = serde_json::from_slice(bytes).map_err(|_| ())?;
        match &value {
            Self::Success { response, cookie } => match (response, cookie) {
                (
                    GameResponse::AdmissionContext { expires_at },
                    GameCookieWire::Admission {
                        token,
                        expires_at: cookie_expiry,
                    },
                )
                | (
                    GameResponse::PlayerJoined {
                        session_expires_at: expires_at,
                        ..
                    },
                    GameCookieWire::Player {
                        token,
                        expires_at: cookie_expiry,
                    },
                ) => {
                    if expires_at != cookie_expiry
                        || !(1..=crate::limits::JS_SAFE_INTEGER_MAX).contains(expires_at)
                    {
                        return Err(());
                    }
                    crate::security::token_digest(token).map_err(|_| ())?;
                }
                (GameResponse::PlayerJoined { .. }, GameCookieWire::None) => return Err(()),
                (_, GameCookieWire::None) => {}
                _ => return Err(()),
            },
            Self::Synced { response } => {
                response.encode_json().map_err(|_| ())?;
            }
            Self::Rejected { .. } => {}
        }
        Ok(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum GameAction {
    Create,
    Lobby,
    Start,
    CallRandom,
    CallManual,
    Winner,
    Cancel,
    Exit,
    AdmissionContext,
    JoinPlayer,
    Stream,
    Sync,
}
impl From<GameOperation> for GameAction {
    fn from(operation: GameOperation) -> Self {
        match operation {
            GameOperation::Create => Self::Create,
            GameOperation::Lobby => Self::Lobby,
            GameOperation::Start => Self::Start,
            GameOperation::CallRandom => Self::CallRandom,
            GameOperation::CallManual => Self::CallManual,
            GameOperation::Winner => Self::Winner,
            GameOperation::Cancel => Self::Cancel,
            GameOperation::Exit => Self::Exit,
            GameOperation::AdmissionContext => Self::AdmissionContext,
            GameOperation::JoinPlayer => Self::JoinPlayer,
            GameOperation::Stream => Self::Stream,
            GameOperation::Sync => Self::Sync,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ViewSelector {
    Account,
    Player,
}
impl From<GameViewSelector> for ViewSelector {
    fn from(value: GameViewSelector) -> Self {
        match value {
            GameViewSelector::Account => Self::Account,
            GameViewSelector::Player => Self::Player,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GameIngress {
    pub operation: GameAction,
    pub game_id: Option<GameId>,
    pub command_id: Option<CommandId>,
    pub view: Option<ViewSelector>,
    pub known_revision: Option<u64>,
    pub session_token: Option<String>,
    pub admission_token: Option<String>,
    pub body: String,
}
impl Drop for GameIngress {
    fn drop(&mut self) {
        self.session_token.zeroize();
        self.admission_token.zeroize();
        self.body.zeroize();
    }
}
impl GameIngress {
    pub(super) fn from_decoded(request: &mut GamesRequest, body: &[u8]) -> Result<Self, ()> {
        let value = Self {
            operation: request.operation.into(),
            game_id: request.game_id,
            command_id: request.command_id,
            view: request.view.map(Into::into),
            known_revision: request.known_revision,
            session_token: request.session_token.take(),
            admission_token: request.admission_token.take(),
            body: std::str::from_utf8(body).map_err(|_| ())?.to_owned(),
        };
        value.validate()?;
        Ok(value)
    }
    pub(super) fn decode_json(bytes: &[u8]) -> Result<Self, ()> {
        if bytes.len() > GAME_WIRE_MAX_BYTES
            || bytes
                .iter()
                .find(|b| !matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
                != Some(&b'{')
        {
            return Err(());
        }
        let value: Self = serde_json::from_slice(bytes).map_err(|_| ())?;
        value.validate()?;
        Ok(value)
    }
    pub(super) fn payload(&self) -> Result<GamePayload, ()> {
        let bytes = self.body.as_bytes();
        if bytes.len() > GAME_BODY_MAX_BYTES {
            return Err(());
        }
        match self.operation {
            GameAction::Create => CreateGame::decode_json(bytes)
                .map(GamePayload::Create)
                .map_err(|_| ()),
            GameAction::Lobby | GameAction::Start | GameAction::CallRandom => {
                RevisionCommand::decode_json(bytes)
                    .map(GamePayload::Revision)
                    .map_err(|_| ())
            }
            GameAction::CallManual => CallManualInput::decode_json(bytes)
                .map(GamePayload::CallManual)
                .map_err(|_| ()),
            GameAction::Winner => WinnerInput::decode_json(bytes)
                .map(GamePayload::Winner)
                .map_err(|_| ()),
            GameAction::Cancel => CancelGameInput::decode_json(bytes)
                .map(GamePayload::Cancel)
                .map_err(|_| ()),
            GameAction::AdmissionContext => AdmissionContextInput::decode_json(bytes)
                .map(GamePayload::AdmissionContext)
                .map_err(|_| ()),
            GameAction::JoinPlayer => JoinPlayer::decode_json(bytes)
                .map(GamePayload::JoinPlayer)
                .map_err(|_| ()),
            GameAction::Exit => crate::api::decode_empty_game_body(bytes)
                .map(|()| GamePayload::Empty)
                .map_err(|_| ()),
            GameAction::Stream | GameAction::Sync if bytes.is_empty() => Ok(GamePayload::Empty),
            _ => Err(()),
        }
    }
    fn validate(&self) -> Result<(), ()> {
        let selected_view = matches!(
            self.operation,
            GameAction::Stream | GameAction::Sync | GameAction::Exit
        );
        let retry_sensitive = matches!(
            self.operation,
            GameAction::Create
                | GameAction::Lobby
                | GameAction::Start
                | GameAction::JoinPlayer
                | GameAction::CallRandom
                | GameAction::CallManual
                | GameAction::Winner
                | GameAction::Cancel
                | GameAction::Exit
        );
        if retry_sensitive != self.command_id.is_some()
            || (self.operation != GameAction::Create && self.game_id.is_none())
            || (!selected_view && (self.view.is_some() || self.known_revision.is_some()))
            || (selected_view && self.view.is_none())
            || (self.operation != GameAction::Sync && self.known_revision.is_some())
            || self
                .known_revision
                .is_some_and(|value| value > brews_contracts::games::MAX_SAFE_REVISION)
            || (!matches!(
                self.operation,
                GameAction::AdmissionContext | GameAction::JoinPlayer
            ) && self.admission_token.is_some())
        {
            return Err(());
        }
        self.payload()?;
        Ok(())
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "Bounded private-wire test fixtures fail fast."
)]
mod tests {
    use super::*;
    use brews_contracts::games::{CallView, GameOutcome, GameReceipt, WinnerView};
    use brews_domain::games::GameState;

    const ID: &str = "01890f3e-53b7-7d28-9b05-4f65092d5711";
    const OTHER_ID: &str = "01890f3e-53b7-7d28-9b05-4f65092d5712";

    fn ingress(action: GameAction, body: &str) -> GameIngress {
        let request = GameIngress {
            operation: action,
            game_id: Some(ID.parse().unwrap()),
            command_id: Some(ID.parse().unwrap()),
            view: None,
            known_revision: None,
            session_token: None,
            admission_token: None,
            body: body.into(),
        };
        request.validate().unwrap();
        request
    }
    fn receipt(outcome: GameOutcome) -> GameReceipt {
        GameReceipt {
            version: 1,
            command_id: ID.parse().unwrap(),
            game_id: ID.parse().unwrap(),
            outcome,
            completed_at: 1_000,
            expires_at: 1_000 + brews_contracts::games::GAME_RECEIPT_RETENTION_MS,
        }
    }
    #[test]
    fn call_success_and_receipt_only_retry_match_exact_manual_request() {
        let request = ingress(
            GameAction::CallManual,
            r#"{"value":"2","expected_revision":1}"#,
        );
        let saved = receipt(GameOutcome::CallAccepted {
            call: CallView {
                sequence_no: 1,
                value: "2".into(),
            },
            remaining_count: 3,
            exhausted: false,
            view_revision: 2,
        });
        let response = GameOwnerResponse::Success {
            response: GameResponse::CallAccepted {
                call: CallView {
                    sequence_no: 1,
                    value: "2".into(),
                },
                remaining_count: 3,
                exhausted: false,
                view_revision: 2,
                receipt: saved.clone(),
            },
            cookie: GameCookieWire::None,
        };
        assert!(response.validate_for(&request).is_ok());
        let retry = GameOwnerResponse::Success {
            response: GameResponse::Committed { receipt: saved },
            cookie: GameCookieWire::None,
        };
        assert!(retry.validate_for(&request).is_ok());
        let wrong_value = ingress(
            GameAction::CallManual,
            r#"{"value":"3","expected_revision":1}"#,
        );
        assert!(response.validate_for(&wrong_value).is_err());
        assert!(retry.validate_for(&wrong_value).is_err());
        let random = ingress(GameAction::CallRandom, r#"{"expected_revision":1}"#);
        assert!(response.validate_for(&random).is_ok());
    }
    #[test]
    fn exit_success_and_receipt_retry_are_bound_only_to_selected_exit_command() {
        let mut request = GameIngress {
            operation: GameAction::Exit,
            game_id: Some(ID.parse().unwrap()),
            command_id: Some(ID.parse().unwrap()),
            view: Some(ViewSelector::Player),
            known_revision: None,
            session_token: None,
            admission_token: None,
            body: "{}".into(),
        };
        request.validate().unwrap();
        let saved = receipt(GameOutcome::Exited {});
        let fresh = GameOwnerResponse::Success {
            response: GameResponse::Exited {
                game_id: ID.parse().unwrap(),
                receipt: saved.clone(),
            },
            cookie: GameCookieWire::None,
        };
        let retry = GameOwnerResponse::Success {
            response: GameResponse::Committed { receipt: saved },
            cookie: GameCookieWire::None,
        };
        assert!(fresh.validate_for(&request).is_ok());
        assert!(retry.validate_for(&request).is_ok());
        request.view = Some(ViewSelector::Account);
        assert!(fresh.validate_for(&request).is_ok());
        request.command_id = Some(OTHER_ID.parse().unwrap());
        assert!(fresh.validate_for(&request).is_err());
        request.command_id = Some(ID.parse().unwrap());
        request.operation = GameAction::Winner;
        assert!(fresh.validate_for(&request).is_err());
        assert!(retry.validate_for(&request).is_err());
    }
    #[test]
    fn terminal_receipts_bind_selected_winner_and_confirmed_state_branch() {
        let winner = Some(WinnerView {
            player_id: ID.parse().unwrap(),
            alias: "Alice".into(),
        });
        let resolved = GameOwnerResponse::Success {
            response: GameResponse::Committed {
                receipt: receipt(GameOutcome::Terminalized {
                    expected_state: GameState::InProgress,
                    state: GameState::Resolved,
                    ended_at: 1_000,
                    history_available: true,
                    history_expires_at: Some(10_000),
                    winner,
                    view_revision: 2,
                }),
            },
            cookie: GameCookieWire::None,
        };
        let request = ingress(
            GameAction::Winner,
            &format!(r#"{{"player_id":"{ID}","expected_revision":1}}"#),
        );
        assert!(resolved.validate_for(&request).is_ok());
        let wrong = ingress(
            GameAction::Winner,
            &format!(r#"{{"player_id":"{OTHER_ID}","expected_revision":1}}"#),
        );
        assert!(resolved.validate_for(&wrong).is_err());
        let cancel = ingress(
            GameAction::Cancel,
            r#"{"confirmed":true,"expected_state":"in_progress"}"#,
        );
        assert!(resolved.validate_for(&cancel).is_err());
        let cancelled = GameOwnerResponse::Success {
            response: GameResponse::Committed {
                receipt: receipt(GameOutcome::Terminalized {
                    expected_state: GameState::AwaitingPlayers,
                    state: GameState::Cancelled,
                    ended_at: 1_000,
                    history_available: false,
                    history_expires_at: None,
                    winner: None,
                    view_revision: 2,
                }),
            },
            cookie: GameCookieWire::None,
        };
        let before_start = ingress(
            GameAction::Cancel,
            r#"{"confirmed":true,"expected_state":"awaiting_players"}"#,
        );
        assert!(cancelled.validate_for(&before_start).is_ok());
        assert!(cancelled.validate_for(&cancel).is_err());
        assert!(cancelled.validate_for(&request).is_err());
    }
    #[test]
    fn gameplay_private_ingress_preserves_original_bytes_and_rejects_action_shape_mismatch() {
        let body = r#"{"value":"2","expected_revision":1}"#;
        let request = ingress(GameAction::CallManual, body);
        let bytes = serde_json::to_vec(&request).unwrap();
        let decoded = GameIngress::decode_json(&bytes).unwrap();
        assert_eq!(decoded.body, body);
        let duplicate = ingress(GameAction::CallManual, body);
        for invalid in [
            r#"{"value":"2","value":"3","expected_revision":1}"#,
            r#"{"value":"2","\u0076alue":"3","expected_revision":1}"#,
            r#"["2",1]"#,
            r#"{"value":"02","expected_revision":1}"#,
            r#"{"value":"2","expected_revision":1,"actor_id":"untrusted"}"#,
        ] {
            let mut altered = serde_json::to_value(&duplicate).unwrap();
            altered["body"] = invalid.into();
            assert!(GameIngress::decode_json(&serde_json::to_vec(&altered).unwrap()).is_err());
        }
        for field in ["command_id", "game_id"] {
            let mut altered = serde_json::to_value(&request).unwrap();
            altered[field] = serde_json::Value::Null;
            assert!(GameIngress::decode_json(&serde_json::to_vec(&altered).unwrap()).is_err());
        }
        let mut altered = serde_json::to_value(&request).unwrap();
        altered["view"] = "player".into();
        assert!(GameIngress::decode_json(&serde_json::to_vec(&altered).unwrap()).is_err());
    }
}
