//! Bounded private game envelopes. Parsed credentials are not authority proofs.
use crate::api::{GameOperation, GamePayload, GameViewSelector, GamesRequest};
use brews_contracts::games::{
    AdmissionContextInput, CreateGame, GAME_BODY_MAX_BYTES, JoinPlayer, RevisionCommand,
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

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum GameCookieWire {
    None,
    Admission { token: String, expires_at: i64 },
    Player { token: String, expires_at: i64 },
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
                if request.operation != GameAction::Sync {
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
                        ) | (
                            Some(ViewSelector::Player),
                            brews_contracts::games::GameView::Player { .. }
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
            GameResponse::Committed { receipt } => receipt,
            GameResponse::AdmissionContext { .. }
                if request.operation == GameAction::AdmissionContext =>
            {
                return Ok(());
            }
            GameResponse::Pending { .. }
                if matches!(
                    request.operation,
                    GameAction::Create | GameAction::Lobby | GameAction::Start
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
        if !matches!(
            (request.operation, &receipt.outcome),
            (GameAction::Create, GameOutcome::Created { .. })
                | (GameAction::Lobby, GameOutcome::LobbyOpened { .. })
                | (GameAction::Start, GameOutcome::Started { .. })
                | (GameAction::JoinPlayer, GameOutcome::PlayerJoined { .. })
        ) {
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
            GameAction::Lobby | GameAction::Start => RevisionCommand::decode_json(bytes)
                .map(GamePayload::Revision)
                .map_err(|_| ()),
            GameAction::AdmissionContext => AdmissionContextInput::decode_json(bytes)
                .map(GamePayload::AdmissionContext)
                .map_err(|_| ()),
            GameAction::JoinPlayer => JoinPlayer::decode_json(bytes)
                .map(GamePayload::JoinPlayer)
                .map_err(|_| ()),
            GameAction::Stream | GameAction::Sync if bytes.is_empty() => Ok(GamePayload::Empty),
            _ => Err(()),
        }
    }
    fn validate(&self) -> Result<(), ()> {
        let read = matches!(self.operation, GameAction::Stream | GameAction::Sync);
        let retry_sensitive = matches!(
            self.operation,
            GameAction::Create | GameAction::Lobby | GameAction::Start | GameAction::JoinPlayer
        );
        if retry_sensitive != self.command_id.is_some()
            || (self.operation != GameAction::Create && self.game_id.is_none())
            || (!read && (self.view.is_some() || self.known_revision.is_some()))
            || (read && self.view.is_none())
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
