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

fn membership_nullable<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
    Option::<i64>::deserialize(d)
}

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
    SpectatorAdmission { token: String, expires_at: i64 },
    Spectator { token: String, expires_at: i64 },
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
            SpectatorAdmission { token: String, expires_at: i64 },
            Spectator { token: String, expires_at: i64 },
        }
        Ok(match CookieObject::deserialize(deserializer)? {
            CookieObject::None {} => Self::None,
            CookieObject::Admission { token, expires_at } => Self::Admission { token, expires_at },
            CookieObject::Player { token, expires_at } => Self::Player { token, expires_at },
            CookieObject::SpectatorAdmission { token, expires_at } => {
                Self::SpectatorAdmission { token, expires_at }
            }
            CookieObject::Spectator { token, expires_at } => Self::Spectator { token, expires_at },
        })
    }
}
impl Drop for GameCookieWire {
    fn drop(&mut self) {
        match self {
            Self::Admission { token, .. }
            | Self::Player { token, .. }
            | Self::Spectator { token, .. }
            | Self::SpectatorAdmission { token, .. } => token.zeroize(),
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
    InvalidRecovery,
    RecoveryBlocked,
}
fn optional_object<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    #[derive(Deserialize)]
    #[serde(bound(deserialize = "T: Deserialize<'de>"))]
    struct Object<T>(#[serde(deserialize_with = "object")] T);
    Option::<Object<T>>::deserialize(deserializer).map(|value| value.map(|value| value.0))
}
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct SyncAccountAuthority {
    account_id: brews_domain::ids::AccountId,
    session_id: brews_domain::ids::SessionId,
    role: brews_domain::accounts::AccountRole,
    epoch: i64,
    expires_at: i64,
}
#[cfg(target_arch = "wasm32")]
impl SyncAccountAuthority {
    pub(super) fn denial_authority(&self) -> Result<crate::auth::GameAccountAuthority, ()> {
        crate::auth::GameAccountAuthority::from_trusted_peer(
            self.account_id,
            self.session_id,
            self.role,
            self.epoch,
            self.expires_at,
        )
        .map_err(|_| ())
    }
}
#[cfg(target_arch = "wasm32")]
impl From<&crate::auth::GameAccountAuthority> for SyncAccountAuthority {
    fn from(proof: &crate::auth::GameAccountAuthority) -> Self {
        Self {
            account_id: proof.account_id(),
            session_id: proof.session_id(),
            role: proof.role(),
            epoch: proof.credential_epoch(),
            expires_at: proof.expires_at(),
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SyncPlayerAuthority {
    pub game_id: GameId,
    pub session_id: brews_domain::ids::SessionId,
    pub player_id: brews_domain::ids::PlayerId,
    pub epoch: i64,
    pub session_expires_at: i64,
    pub valid_until: i64,
}
impl SyncPlayerAuthority {
    pub(super) fn validate(&self) -> Result<(), ()> {
        if !(1..=crate::limits::JS_SAFE_INTEGER_MAX).contains(&self.valid_until)
            || !(self.valid_until..=crate::limits::JS_SAFE_INTEGER_MAX)
                .contains(&self.session_expires_at)
            || !(0..=crate::limits::JS_SAFE_INTEGER_MAX).contains(&self.epoch)
        {
            return Err(());
        }
        Ok(())
    }
    #[cfg(target_arch = "wasm32")]
    pub(super) fn binding(&self) -> crate::db::game::SyncPlayerBinding {
        crate::db::game::SyncPlayerBinding {
            game_id: self.game_id,
            session_id: self.session_id,
            player_id: self.player_id,
            epoch: self.epoch,
            session_expires_at: self.session_expires_at,
            valid_until: self.valid_until,
        }
    }
}
#[cfg(target_arch = "wasm32")]
impl From<crate::db::game::SyncPlayerBinding> for SyncPlayerAuthority {
    fn from(binding: crate::db::game::SyncPlayerBinding) -> Self {
        Self {
            game_id: binding.game_id,
            session_id: binding.session_id,
            player_id: binding.player_id,
            epoch: binding.epoch,
            session_expires_at: binding.session_expires_at,
            valid_until: binding.valid_until,
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PlayerSyncReleaseRequest {
    pub token: String,
    #[serde(deserialize_with = "object")]
    pub authority: SyncPlayerAuthority,
}
impl Drop for PlayerSyncReleaseRequest {
    fn drop(&mut self) {
        self.token.zeroize();
    }
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "release_result", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum PlayerSyncReleaseReply {
    Released {
        #[serde(deserialize_with = "object")]
        authority: SyncPlayerAuthority,
        checked_at: i64,
    },
    Rejected {
        code: GameRejection,
    },
}
impl PlayerSyncReleaseReply {
    pub(super) fn validate_for(&self, expected: &SyncPlayerAuthority) -> Result<(), ()> {
        expected.validate()?;
        if let Self::Released {
            authority,
            checked_at,
        } = self
            && (authority != expected || !(0..expected.valid_until).contains(checked_at))
        {
            return Err(());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SyncSpectatorAuthority {
    pub game_id: GameId,
    pub session_id: brews_domain::ids::SessionId,
    pub spectator_id: brews_domain::ids::SpectatorId,
    pub epoch: i64,
    pub session_expires_at: i64,
    pub valid_until: i64,
}
#[cfg(target_arch = "wasm32")]
impl From<crate::db::game::SpectatorAuthority> for SyncSpectatorAuthority {
    fn from(a: crate::db::game::SpectatorAuthority) -> Self {
        Self {
            game_id: a.game_id,
            session_id: a.session_id,
            spectator_id: a.spectator_id,
            epoch: a.epoch,
            session_expires_at: a.session_expires_at,
            valid_until: a.valid_until,
        }
    }
}
#[cfg(target_arch = "wasm32")]
impl SyncSpectatorAuthority {
    pub(super) fn binding(&self) -> crate::db::game::SpectatorAuthority {
        crate::db::game::SpectatorAuthority {
            game_id: self.game_id,
            session_id: self.session_id,
            spectator_id: self.spectator_id,
            epoch: self.epoch,
            session_expires_at: self.session_expires_at,
            valid_until: self.valid_until,
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "owner_result", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum GameOwnerResponse {
    Audience {
        #[serde(deserialize_with = "object")]
        response: GameResponse,
        #[serde(deserialize_with = "object")]
        cookie: GameCookieWire,
        #[serde(deserialize_with = "membership_nullable")]
        valid_until: Option<i64>,
        clear_player_cookie: bool,
        clear_spectator_cookie: bool,
    },
    AudienceSynced {
        #[serde(deserialize_with = "object")]
        response: SyncResponse,
        #[serde(deserialize_with = "object")]
        authority: SyncSpectatorAuthority,
    },
    Membership {
        #[serde(deserialize_with = "object")]
        response: GameResponse,
        #[serde(deserialize_with = "membership_nullable")]
        valid_until: Option<i64>,
        clear_player_cookie: bool,
    },
    RecoveryAnswer {
        #[serde(deserialize_with = "object")]
        response: GameResponse,
        valid_until: i64,
    },
    Recovered {
        valid_until: Option<i64>,
        receipt: brews_contracts::games::GameReceipt,
        game: Option<brews_contracts::games::GameView>,
        cookie: GameCookieWire,
    },
    Success {
        #[serde(deserialize_with = "object")]
        response: GameResponse,
        #[serde(deserialize_with = "object")]
        cookie: GameCookieWire,
    },
    Synced {
        valid_until: Option<i64>,
        #[serde(default, deserialize_with = "optional_object")]
        authority: Option<SyncAccountAuthority>,
        #[serde(default, deserialize_with = "optional_object")]
        player_authority: Option<SyncPlayerAuthority>,
        #[serde(deserialize_with = "object")]
        response: SyncResponse,
    },
    Rejected {
        code: GameRejection,
    },
}
impl GameOwnerResponse {
    pub(super) fn validate_for(&self, request: &GameIngress) -> Result<(), ()> {
        if let Self::AudienceSynced {
            response,
            authority,
        } = self
        {
            if request.operation != GameAction::Sync
                || request.view != Some(ViewSelector::Spectator)
                || Some(authority.game_id) != request.game_id
                || authority.valid_until <= 0
                || authority.valid_until > authority.session_expires_at
                || authority.epoch < 0
                || authority.epoch > brews_contracts::games::MAX_SAFE_REVISION as i64
                || authority.session_expires_at > brews_contracts::games::MAX_SAFE_TIMESTAMP
                || (response.up_to_date && request.known_revision != Some(response.view_revision))
            {
                return Err(());
            }
            response.encode_json().map_err(|_| ())?;
            if let Some(view) = &response.snapshot
                && !matches!(view,brews_contracts::games::GameView::Spectator{game_id,spectator_id,..} if *game_id==authority.game_id && *spectator_id==authority.spectator_id)
            {
                return Err(());
            }
            return Ok(());
        }
        if let Self::Audience {
            response,
            cookie,
            valid_until,
            clear_player_cookie,
            clear_spectator_cookie,
        } = self
        {
            response.validate().map_err(|_| ())?;
            if request.operation == GameAction::SpectatorContext {
                if *clear_player_cookie || *clear_spectator_cookie || valid_until.is_none() {
                    return Err(());
                }
                return match (response, cookie) {
                    (
                        GameResponse::AdmissionContext { expires_at },
                        GameCookieWire::SpectatorAdmission {
                            token,
                            expires_at: expiry,
                        },
                    ) if expires_at == expiry && valid_until.is_some_and(|d| d <= *expiry) => {
                        crate::security::token_digest(token).map_err(|_| ())?;
                        Ok(())
                    }
                    (GameResponse::AdmissionContext { expires_at }, GameCookieWire::None)
                        if valid_until.is_some_and(|d| d <= *expires_at) =>
                    {
                        Ok(())
                    }
                    _ => Err(()),
                };
            }
            let receipt = match response {
                GameResponse::ParticipantChanged { receipt }
                | GameResponse::Committed { receipt } => receipt,
                _ => return Err(()),
            };
            if Some(receipt.game_id) != request.game_id
                || Some(receipt.command_id) != request.command_id
            {
                return Err(());
            }
            let brews_contracts::games::GameOutcome::ParticipantChanged {
                change,
                identity,
                session_expires_at,
                ..
            } = &receipt.outcome
            else {
                return Err(());
            };
            use brews_contracts::games::{ParticipantChange, ParticipantIdentity};
            let valid = match (request.operation, change, identity) {
                (
                    GameAction::JoinSpectator,
                    ParticipantChange::Admitted | ParticipantChange::Reconnected,
                    ParticipantIdentity::Spectator { .. },
                ) => !*clear_player_cookie && !*clear_spectator_cookie,
                (
                    GameAction::SwitchToSpectator,
                    ParticipantChange::Switched,
                    ParticipantIdentity::Spectator { .. },
                ) => *clear_player_cookie && !*clear_spectator_cookie,
                (
                    GameAction::SwitchToPlayer,
                    ParticipantChange::Switched,
                    ParticipantIdentity::Player { alias, .. },
                ) => {
                    !*clear_player_cookie
                        && *clear_spectator_cookie
                        && matches!(request.payload()?,GamePayload::JoinPlayer(input) if input.alias==*alias)
                }
                (
                    GameAction::LeaveSpectator,
                    ParticipantChange::Left,
                    ParticipantIdentity::Spectator { .. },
                ) => !*clear_player_cookie && *clear_spectator_cookie,
                _ => false,
            };
            if !valid
                || valid_until.is_none() != (*change == ParticipantChange::Left)
                || valid_until.is_some_and(|d| d <= receipt.completed_at || d > receipt.expires_at)
            {
                return Err(());
            }
            match (identity, cookie, session_expires_at) {
                (
                    ParticipantIdentity::Spectator { .. },
                    GameCookieWire::Spectator { token, expires_at },
                    Some(e),
                )
                | (
                    ParticipantIdentity::Player { .. },
                    GameCookieWire::Player { token, expires_at },
                    Some(e),
                ) if e == expires_at => {
                    crate::security::token_digest(token).map_err(|_| ())?;
                }
                (_, GameCookieWire::None, _)
                    if matches!(response, GameResponse::Committed { .. })
                        || *change == ParticipantChange::Left => {}
                _ => return Err(()),
            }
            return Ok(());
        }

        if let Self::Membership {
            response,
            valid_until,
            clear_player_cookie,
        } = self
        {
            if !matches!(
                request.operation,
                GameAction::RenameAlias
                    | GameAction::LeavePlayer
                    | GameAction::RenameAliasOutput
                    | GameAction::RenameAliasReceiptOutput
                    | GameAction::LeavePlayerOutput
            ) {
                return Err(());
            }
            response.validate().map_err(|_| ())?;
            let receipt = match response {
                GameResponse::AliasRenamed { receipt, .. }
                | GameResponse::PlayerLeft { receipt, .. }
                | GameResponse::Committed { receipt } => receipt,
                _ => return Err(()),
            };
            if valid_until.is_some_and(|deadline| {
                deadline <= receipt.completed_at || deadline > receipt.expires_at
            }) {
                return Err(());
            }
            let deleted = matches!(
                response,
                GameResponse::PlayerLeft {
                    expected_state: brews_domain::games::GameState::AwaitingPlayers,
                    ..
                }
            );
            if *clear_player_cookie != deleted
                || valid_until.is_none() != deleted
                || (deleted && request.operation != GameAction::LeavePlayer)
                || valid_until
                    .is_some_and(|d| !(1..=crate::limits::JS_SAFE_INTEGER_MAX).contains(&d))
            {
                return Err(());
            }
            let operation = match request.operation {
                GameAction::RenameAliasOutput | GameAction::RenameAliasReceiptOutput => {
                    GameAction::RenameAlias
                }
                GameAction::LeavePlayerOutput => GameAction::LeavePlayer,
                other => other,
            };
            let mut selected = GameIngress {
                operation,
                game_id: request.game_id,
                command_id: request.command_id,
                view: request.view,
                known_revision: request.known_revision,
                session_token: None,
                admission_token: None,
                recovery_caller: None,
                body: request.body.clone(),
            };
            let checked = Self::Success {
                response: response.clone(),
                cookie: GameCookieWire::None,
            }
            .validate_success_for(&selected);
            selected.body.zeroize();
            return checked;
        }
        if matches!(
            request.operation,
            GameAction::RenameAlias
                | GameAction::LeavePlayer
                | GameAction::RenameAliasOutput
                | GameAction::RenameAliasReceiptOutput
                | GameAction::LeavePlayerOutput
        ) && !matches!(self, Self::Rejected { .. })
        {
            return Err(());
        }
        if let Self::Recovered {
            receipt,
            game,
            cookie,
            valid_until,
        } = self
        {
            if !matches!(
                request.operation,
                GameAction::RecoverPlayer | GameAction::RecoveryOutput
            ) || Some(receipt.command_id) != request.command_id
                || Some(receipt.game_id) != request.game_id
            {
                return Err(());
            }
            receipt.validate().map_err(|_| ())?;
            let brews_contracts::games::GameOutcome::PlayerRecovered {
                player_id,
                session_expires_at,
                ..
            } = receipt.outcome
            else {
                return Err(());
            };
            if game.is_some() != valid_until.is_some()
                || valid_until.is_some_and(|deadline| {
                    deadline <= receipt.completed_at || deadline > session_expires_at
                })
            {
                return Err(());
            }
            match (game, cookie) {
                (None, GameCookieWire::None) => {}
                (
                    Some(brews_contracts::games::GameView::Player {
                        game_id,
                        player_id: own,
                        ..
                    }),
                    GameCookieWire::Player { token, expires_at },
                ) if *own == player_id
                    && *game_id == receipt.game_id
                    && *expires_at == session_expires_at =>
                {
                    crate::security::token_digest(token).map_err(|_| ())?;
                    if request.operation == GameAction::RecoveryOutput
                        && request.session_token.as_deref() != Some(token.as_str())
                    {
                        return Err(());
                    }
                }
                _ => return Err(()),
            }
            return Ok(());
        }
        if let Self::RecoveryAnswer {
            response,
            valid_until,
        } = self
        {
            if !matches!(
                request.operation,
                GameAction::PutRecoveryAnswer | GameAction::DeleteRecoveryAnswer
            ) || !(1..=crate::limits::JS_SAFE_INTEGER_MAX).contains(valid_until)
            {
                return Err(());
            }
            return Self::Success {
                response: response.clone(),
                cookie: GameCookieWire::None,
            }
            .validate_success_for(request);
        }
        if matches!(
            request.operation,
            GameAction::PutRecoveryAnswer | GameAction::DeleteRecoveryAnswer
        ) && !matches!(self, Self::Rejected { .. })
        {
            return Err(());
        }
        self.validate_success_for(request)
    }
    fn validate_success_for(&self, request: &GameIngress) -> Result<(), ()> {
        use brews_contracts::games::GameOutcome;
        let Self::Success { response, .. } = self else {
            if let Self::Synced {
                response,
                valid_until,
                authority,
                player_authority,
            } = self
            {
                match request.view {
                    Some(ViewSelector::Player) => {
                        let binding = player_authority.as_ref().ok_or(())?;
                        binding.validate()?;
                        if authority.is_some()
                            || Some(binding.game_id) != request.game_id
                            || Some(binding.valid_until) != *valid_until
                        {
                            return Err(());
                        }
                        if let Some(
                            brews_contracts::games::GameView::Player { player_id, .. }
                            | brews_contracts::games::GameView::FinalPlayer { player_id, .. },
                        ) = &response.snapshot
                            && *player_id != binding.player_id
                        {
                            return Err(());
                        }
                    }
                    Some(ViewSelector::Account) if player_authority.is_none() => {}
                    _ => return Err(()),
                }
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
            GameResponse::RecoveryAnswerChanged { receipt, .. }
                if matches!(
                    request.operation,
                    GameAction::PutRecoveryAnswer | GameAction::DeleteRecoveryAnswer
                ) =>
            {
                receipt
            }
            GameResponse::AliasRenamed { receipt, .. }
                if request.operation == GameAction::RenameAlias =>
            {
                receipt
            }
            GameResponse::PlayerLeft { receipt, .. }
                if request.operation == GameAction::LeavePlayer =>
            {
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
        receipt.validate().map_err(|_| ())?;
        let coherent = match (request.operation, &receipt.outcome) {
            (GameAction::RenameAlias, GameOutcome::AliasRenamed { alias, .. }) => {
                matches!(request.payload()?, GamePayload::RenameAlias(input) if brews_domain::games::validate_alias(&input.alias).ok() == Some(alias.as_str()))
            }
            (GameAction::LeavePlayer, GameOutcome::PlayerLeft { expected_state, .. }) => {
                matches!(request.payload()?, GamePayload::LeavePlayer(input) if input.expected_state == *expected_state)
            }
            (GameAction::Create, GameOutcome::Created { .. })
            | (GameAction::Lobby, GameOutcome::LobbyOpened { .. })
            | (GameAction::Start, GameOutcome::Started { .. })
            | (GameAction::JoinPlayer, GameOutcome::PlayerJoined { .. })
            | (GameAction::CallRandom, GameOutcome::CallAccepted { .. })
            | (GameAction::Exit, GameOutcome::Exited {})
            | (
                GameAction::PutRecoveryAnswer | GameAction::DeleteRecoveryAnswer,
                GameOutcome::RecoveryAnswerChanged { .. },
            ) => true,
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
            Self::Membership {
                response,
                valid_until,
                ..
            } => {
                response.validate().map_err(|_| ())?;
                if valid_until
                    .is_some_and(|d| !(1..=crate::limits::JS_SAFE_INTEGER_MAX).contains(&d))
                {
                    return Err(());
                }
            }
            Self::RecoveryAnswer {
                response,
                valid_until,
            } => {
                response.encode_json().map_err(|_| ())?;
                if !(1..=crate::limits::JS_SAFE_INTEGER_MAX).contains(valid_until) {
                    return Err(());
                }
            }
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
            Self::Synced {
                response,
                valid_until,
                player_authority,
                ..
            } => {
                if let Some(binding) = player_authority {
                    binding.validate()?;
                }
                if valid_until.is_some_and(|deadline| {
                    !(1..=crate::limits::JS_SAFE_INTEGER_MAX).contains(&deadline)
                }) {
                    return Err(());
                }
                response.encode_json().map_err(|_| ())?;
            }
            Self::Recovered { receipt, game, .. } => {
                receipt.validate().map_err(|_| ())?;
                if let Some(game) = game {
                    game.validate().map_err(|_| ())?;
                }
            }
            Self::Audience { response, .. } => {
                response.validate().map_err(|_| ())?;
            }
            Self::AudienceSynced { response, .. } => {
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
    SpectatorContext,
    JoinSpectator,
    SwitchToSpectator,
    SwitchToPlayer,
    LeaveSpectator,
    RecoverPlayer,
    RecoveryOutput,
    RenameAlias,
    LeavePlayer,
    RenameAliasOutput,
    RenameAliasReceiptOutput,
    LeavePlayerOutput,
    PutRecoveryAnswer,
    DeleteRecoveryAnswer,
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
            GameOperation::SpectatorContext => Self::SpectatorContext,
            GameOperation::JoinSpectator => Self::JoinSpectator,
            GameOperation::SwitchToSpectator => Self::SwitchToSpectator,
            GameOperation::SwitchToPlayer => Self::SwitchToPlayer,
            GameOperation::LeaveSpectator => Self::LeaveSpectator,
            GameOperation::RecoverPlayer => Self::RecoverPlayer,
            GameOperation::RenameAlias => Self::RenameAlias,
            GameOperation::LeavePlayer => Self::LeavePlayer,
            GameOperation::PutRecoveryAnswer => Self::PutRecoveryAnswer,
            GameOperation::DeleteRecoveryAnswer => Self::DeleteRecoveryAnswer,
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
    Spectator,
}
impl From<GameViewSelector> for ViewSelector {
    fn from(value: GameViewSelector) -> Self {
        match value {
            GameViewSelector::Account => Self::Account,
            GameViewSelector::Player => Self::Player,
            GameViewSelector::Spectator => Self::Spectator,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_caller: Option<String>,
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
            recovery_caller: None,
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
            GameAction::RenameAlias
            | GameAction::RenameAliasOutput
            | GameAction::RenameAliasReceiptOutput => {
                brews_contracts::games::RenameAliasInput::decode_json(bytes)
                    .map(GamePayload::RenameAlias)
                    .map_err(|_| ())
            }
            GameAction::LeavePlayer
            | GameAction::LeavePlayerOutput
            | GameAction::LeaveSpectator => {
                brews_contracts::games::LeavePlayerInput::decode_json(bytes)
                    .map(GamePayload::LeavePlayer)
                    .map_err(|_| ())
            }
            GameAction::RecoverPlayer | GameAction::RecoveryOutput => {
                crate::api::decode_recovery_input(bytes)
                    .map(GamePayload::RecoverPlayer)
                    .map_err(|_| ())
            }
            GameAction::PutRecoveryAnswer => crate::api::decode_recovery_input(bytes)
                .map(GamePayload::RecoveryAnswer)
                .map_err(|_| ()),
            GameAction::DeleteRecoveryAnswer => crate::api::decode_empty_game_body(bytes)
                .map(|()| GamePayload::Empty)
                .map_err(|_| ()),
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
            GameAction::AdmissionContext
            | GameAction::SpectatorContext
            | GameAction::JoinSpectator => AdmissionContextInput::decode_json(bytes)
                .map(GamePayload::AdmissionContext)
                .map_err(|_| ()),
            GameAction::JoinPlayer | GameAction::SwitchToPlayer => JoinPlayer::decode_json(bytes)
                .map(GamePayload::JoinPlayer)
                .map_err(|_| ()),
            GameAction::Exit | GameAction::SwitchToSpectator => {
                crate::api::decode_empty_game_body(bytes)
                    .map(|()| GamePayload::Empty)
                    .map_err(|_| ())
            }
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
                | GameAction::JoinSpectator
                | GameAction::SwitchToSpectator
                | GameAction::SwitchToPlayer
                | GameAction::LeaveSpectator
                | GameAction::RecoverPlayer
                | GameAction::RecoveryOutput
                | GameAction::RenameAlias
                | GameAction::LeavePlayer
                | GameAction::RenameAliasOutput
                | GameAction::RenameAliasReceiptOutput
                | GameAction::LeavePlayerOutput
                | GameAction::PutRecoveryAnswer
                | GameAction::DeleteRecoveryAnswer
                | GameAction::CallRandom
                | GameAction::CallManual
                | GameAction::Winner
                | GameAction::Cancel
                | GameAction::Exit
        );
        if retry_sensitive != self.command_id.is_some()
            || (!matches!(
                self.operation,
                GameAction::Create | GameAction::RecoverPlayer
            ) && self.game_id.is_none())
            || (!selected_view && (self.view.is_some() || self.known_revision.is_some()))
            || (selected_view && self.view.is_none())
            || (self.operation != GameAction::Sync && self.known_revision.is_some())
            || self
                .known_revision
                .is_some_and(|value| value > brews_contracts::games::MAX_SAFE_REVISION)
            || (!matches!(
                self.operation,
                GameAction::AdmissionContext
                    | GameAction::JoinPlayer
                    | GameAction::SpectatorContext
                    | GameAction::JoinSpectator
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
            recovery_caller: None,
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
            recovery_caller: None,
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
    #[test]
    fn membership_wire_requires_original_maps_exact_receipt_outcome_and_narrow_clear_cookie() {
        let request = ingress(GameAction::RenameAlias, r#"{"alias":" Alice "}"#);
        let game_id = request.game_id.unwrap();
        let command_id = request.command_id.unwrap();
        let player_id = game_id.to_string().parse().unwrap();
        let completed_at = 1_000;
        let receipt = brews_contracts::games::GameReceipt {
            version: 1,
            game_id,
            command_id,
            completed_at,
            expires_at: completed_at + 86_400_000,
            outcome: brews_contracts::games::GameOutcome::AliasRenamed {
                player_id,
                alias: "Alice".into(),
                session_expires_at: 2_000,
                view_revision: 1,
            },
        };
        let response = GameOwnerResponse::Membership {
            response: GameResponse::AliasRenamed {
                player_id,
                alias: "Alice".into(),
                session_expires_at: 2_000,
                view_revision: 1,
                receipt,
            },
            valid_until: Some(2_000),
            clear_player_cookie: false,
        };
        assert!(response.validate_for(&request).is_ok());
        let original = serde_json::to_value(&response).unwrap();
        for (field, value) in [
            ("clear_player_cookie", serde_json::json!(true)),
            ("valid_until", serde_json::Value::Null),
            ("valid_until", serde_json::json!(completed_at)),
            ("valid_until", serde_json::json!(completed_at + 86_400_001)),
        ] {
            let mut altered = original.clone();
            altered[field] = value;
            let bad = GameOwnerResponse::decode_json(&serde_json::to_vec(&altered).unwrap());
            assert!(bad.is_err() || bad.unwrap().validate_for(&request).is_err());
        }
        let mut altered = original.clone();
        altered["response"]["alias"] = "Different".into();
        assert!(GameOwnerResponse::decode_json(&serde_json::to_vec(&altered).unwrap()).is_err());
        let mut altered = original;
        altered["response"]["receipt"]["outcome"]["alias"] = "Different".into();
        assert!(GameOwnerResponse::decode_json(&serde_json::to_vec(&altered).unwrap()).is_err());
        for (action, body) in [
            (
                GameAction::RenameAlias,
                r#"{"alias":"Alice","player_id":"x"}"#,
            ),
            (
                GameAction::LeavePlayer,
                r#"{"expected_state":"awaiting_players","expected_state":"in_progress"}"#,
            ),
            (GameAction::LeavePlayer, "[]"),
        ] {
            let mut request = ingress(GameAction::RenameAlias, r#"{"alias":"Alice"}"#);
            request.operation = action;
            request.body = body.to_owned();
            assert!(GameIngress::decode_json(&serde_json::to_vec(&request).unwrap()).is_err());
        }
    }
    #[test]
    fn player_sync_release_requires_exact_original_proof_and_closed_objects() {
        let proof = SyncPlayerAuthority {
            game_id: ID.parse().unwrap(),
            session_id: ID.parse().unwrap(),
            player_id: ID.parse().unwrap(),
            epoch: 1,
            session_expires_at: 200,
            valid_until: 100,
        };
        let reply = PlayerSyncReleaseReply::Released {
            authority: proof.clone(),
            checked_at: 99,
        };
        assert!(reply.validate_for(&proof).is_ok());
        for field in [
            "game_id",
            "session_id",
            "player_id",
            "epoch",
            "session_expires_at",
            "valid_until",
            "checked_at",
        ] {
            let mut bad = serde_json::to_value(&reply).unwrap();
            if field == "checked_at" {
                bad[field] = 100.into();
            } else {
                bad["authority"][field] = if field.ends_with("_id") {
                    OTHER_ID.into()
                } else {
                    2.into()
                };
            }
            let bad: PlayerSyncReleaseReply = serde_json::from_value(bad).unwrap();
            assert!(bad.validate_for(&proof).is_err(), "{field}");
        }
        for bytes in [
            r#"{"release_result":"released","checked_at":99}"#,
            r#"{"release_result":"released","authority":[],"checked_at":99}"#,
            r#"{"release_result":"rejected","code":"unauthorized","unknown":1}"#,
        ] {
            assert!(serde_json::from_str::<PlayerSyncReleaseReply>(bytes).is_err());
        }
        let release_request = PlayerSyncReleaseRequest {
            token: String::new(),
            authority: proof.clone(),
        };
        assert!(
            serde_json::from_slice::<PlayerSyncReleaseRequest>(
                &serde_json::to_vec(&release_request).unwrap()
            )
            .is_ok()
        );
        let request = GameIngress {
            operation: GameAction::Sync,
            game_id: Some(proof.game_id),
            command_id: None,
            view: Some(ViewSelector::Player),
            known_revision: Some(2),
            session_token: None,
            admission_token: None,
            recovery_caller: None,
            body: String::new(),
        };
        let missing = GameOwnerResponse::decode_json(br#"{"owner_result":"synced","valid_until":100,"response":{"up_to_date":true,"view_revision":2,"snapshot":null}}"#).unwrap();
        assert!(missing.validate_for(&request).is_err());
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "release", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum AudienceReleaseRequest {
    Sync {
        token: String,
        #[serde(deserialize_with = "object")]
        authority: SyncSpectatorAuthority,
    },
    Receipt {
        token: Option<String>,
        context: Option<String>,
        #[serde(deserialize_with = "object")]
        receipt: brews_contracts::games::GameReceipt,
    },
    Context {
        token: String,
        game_code: brews_domain::games::GameCode,
        game_id: GameId,
        expires_at: i64,
    },
}
const AUDIENCE_RELEASE_CHALLENGE_BYTES: usize = 32;

// Per-call non-bearer challenge: never Debug, persisted, or exposed publicly.
#[derive(Serialize, Deserialize)]
#[serde(transparent)]
struct AudienceReleaseChallenge([u8; AUDIENCE_RELEASE_CHALLENGE_BYTES]);
impl Drop for AudienceReleaseChallenge {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

#[derive(Serialize)]
pub(super) struct AudienceReleaseAttempt {
    challenge: AudienceReleaseChallenge,
    pub(super) request: AudienceReleaseRequest,
}
impl<'de> Deserialize<'de> for AudienceReleaseAttempt {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            challenge: AudienceReleaseChallenge,
            #[serde(deserialize_with = "object")]
            request: AudienceReleaseRequest,
        }
        let fields: Fields = object(deserializer)?;
        Ok(Self {
            challenge: fields.challenge,
            request: fields.request,
        })
    }
}
impl AudienceReleaseAttempt {
    pub(super) fn new(
        request: AudienceReleaseRequest,
        runtime: &impl crate::auth::Runtime,
        observed_at: i64,
    ) -> Result<Self, ()> {
        if !(0..=crate::limits::JS_SAFE_INTEGER_MAX).contains(&observed_at) {
            return Err(());
        }
        let mut challenge = zeroize::Zeroizing::new([0; AUDIENCE_RELEASE_CHALLENGE_BYTES]);
        runtime.fill_random(challenge.as_mut()).map_err(|_| ())?;
        Ok(Self {
            challenge: AudienceReleaseChallenge(*challenge),
            request,
        })
    }

    pub(super) fn binding(&self) -> Result<AudienceReleaseBinding, ()> {
        use sha2::{Digest as _, Sha256};
        let encoded = zeroize::Zeroizing::new(serde_json::to_vec(self).map_err(|_| ())?);
        let mut digest = Sha256::new();
        digest.update(b"brews-audience-release-attempt\0");
        digest.update(encoded.as_slice());
        Ok(AudienceReleaseBinding(digest.finalize().into()))
    }
}

// Credential-derived private ACK proof: never Debug, persisted, or public JSON.
#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub(super) struct AudienceReleaseBinding([u8; 32]);
impl Drop for AudienceReleaseBinding {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl Drop for AudienceReleaseRequest {
    fn drop(&mut self) {
        match self {
            Self::Sync { token, .. } | Self::Context { token, .. } => token.zeroize(),
            Self::Receipt { token, context, .. } => {
                token.zeroize();
                context.zeroize();
            }
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "release_result", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum AudienceReleaseReply {
    Released {
        binding: AudienceReleaseBinding,
        valid_until: i64,
        checked_at: i64,
    },
    Rejected {
        binding: AudienceReleaseBinding,
        code: GameRejection,
    },
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "Exact private wire tests fail fast.")]
mod audience_release_tests {
    use super::*;
    use crate::auth::Runtime as _;

    struct AttemptRuntime {
        now: i64,
        draws: std::cell::Cell<u8>,
        rng_fails: bool,
    }
    impl crate::auth::Runtime for AttemptRuntime {
        fn now_ms(&self) -> i64 {
            self.now
        }
        fn fill_random(&self, bytes: &mut [u8]) -> Result<(), crate::auth::AuthError> {
            let draw = self.draws.get() + 1;
            self.draws.set(draw);
            bytes.fill(draw);
            if self.rng_fails {
                Err(crate::auth::AuthError::Crypto)
            } else {
                Ok(())
            }
        }
    }
    fn context_request() -> AudienceReleaseRequest {
        serde_json::from_value(serde_json::json!({"release":"context", "token":"test-only-context", "game_code":"ABCDEFGH", "game_id":"01890f3e-53b7-7d28-9b05-4f65092d5711", "expires_at":100})).unwrap()
    }

    #[test]
    fn audience_release_attempt_new_draws_fresh_challenge_for_identical_requests() {
        let runtime = AttemptRuntime {
            now: 1,
            draws: std::cell::Cell::new(0),
            rng_fails: false,
        };
        let first =
            AudienceReleaseAttempt::new(context_request(), &runtime, runtime.now_ms()).unwrap();
        let second =
            AudienceReleaseAttempt::new(context_request(), &runtime, runtime.now_ms()).unwrap();
        assert!(first.binding().unwrap() != second.binding().unwrap());
        assert_eq!(runtime.draws.get(), 2);
        assert!(
            serde_json::to_vec(&first.request).unwrap()
                == serde_json::to_vec(&second.request).unwrap()
        );
    }

    #[test]
    fn audience_release_attempt_new_fails_closed_on_clock_or_partial_rng_failure() {
        for now in [-1, crate::limits::JS_SAFE_INTEGER_MAX + 1] {
            let runtime = AttemptRuntime {
                now,
                draws: std::cell::Cell::new(0),
                rng_fails: false,
            };
            assert!(
                AudienceReleaseAttempt::new(context_request(), &runtime, runtime.now_ms()).is_err()
            );
            assert_eq!(runtime.draws.get(), 0);
        }
        let runtime = AttemptRuntime {
            now: 1,
            draws: std::cell::Cell::new(0),
            rng_fails: true,
        };
        assert!(
            AudienceReleaseAttempt::new(context_request(), &runtime, runtime.now_ms()).is_err()
        );
        assert_eq!(runtime.draws.get(), 1);
    }

    #[test]
    fn audience_release_attempt_wire_requires_closed_object_and_exact_challenge_width() {
        let original = serde_json::json!({"challenge":vec![1u8;32],"request":context_request()});
        let parsed: AudienceReleaseAttempt = serde_json::from_value(original.clone()).unwrap();
        let mut changed = original.clone();
        changed["challenge"][0] = 2.into();
        let changed: AudienceReleaseAttempt = serde_json::from_value(changed).unwrap();
        assert!(parsed.binding().unwrap() != changed.binding().unwrap());
        for key in ["challenge", "request"] {
            let mut missing = original.clone();
            missing.as_object_mut().unwrap().remove(key);
            assert!(serde_json::from_value::<AudienceReleaseAttempt>(missing).is_err());
            let mut null = original.clone();
            null[key] = serde_json::Value::Null;
            assert!(serde_json::from_value::<AudienceReleaseAttempt>(null).is_err());
        }
        for challenge in [
            serde_json::json!([]),
            serde_json::json!(vec![1u8; 31]),
            serde_json::json!(vec![1u8; 33]),
            serde_json::json!(vec![256u16; 32]),
            serde_json::json!("untyped"),
        ] {
            let mut changed = original.clone();
            changed["challenge"] = challenge;
            assert!(serde_json::from_value::<AudienceReleaseAttempt>(changed).is_err());
        }
        let mut unknown = original.clone();
        unknown["extra"] = 1.into();
        assert!(serde_json::from_value::<AudienceReleaseAttempt>(unknown).is_err());
        assert!(
            serde_json::from_value::<AudienceReleaseAttempt>(serde_json::json!([
                original["challenge"],
                original["request"]
            ]))
            .is_err()
        );
        let encoded = serde_json::to_string(&original).unwrap();
        let duplicate = encoded.replacen("\"challenge\":", "\"challenge\":[],\"challenge\":", 1);
        assert!(serde_json::from_str::<AudienceReleaseAttempt>(&duplicate).is_err());
        assert!(
            serde_json::from_value::<AudienceReleaseAttempt>(original["request"].clone()).is_err()
        );
        for reply in [
            serde_json::json!({"release_result":"released","valid_until":100,"checked_at":1}),
            serde_json::json!({"release_result":"rejected","code":"unauthorized"}),
        ] {
            assert!(serde_json::from_value::<AudienceReleaseReply>(reply).is_err());
        }
    }

    #[test]
    fn audience_ack_proof_binds_every_original_request_field() {
        let sync = serde_json::json!({"release":"sync", "token":"test-only-high-entropy-proof", "authority":{"game_id":"01890f3e-53b7-7d28-9b05-4f65092d5711", "session_id":"01890f3e-53b7-7d28-9b05-4f65092d5711", "spectator_id":"01890f3e-53b7-7d28-9b05-4f65092d5711", "epoch":1, "session_expires_at":200, "valid_until":100}});
        let context = serde_json::json!({"release":"context", "token":"test-only-context", "game_code":"ABCDEFGH", "game_id":"01890f3e-53b7-7d28-9b05-4f65092d5711", "expires_at":100});
        let receipt = serde_json::json!({"release":"receipt", "token":"test-only-session", "context":"test-only-context", "receipt":{"version":1,"game_id":"01890f3e-53b7-7d28-9b05-4f65092d5711","command_id":"01890f3e-53b7-7d28-9b05-4f65092d5711","outcome":{"operation":"started","view_revision":1,"started_at":1},"completed_at":1,"expires_at":86400001}});
        for original in [sync, context, receipt] {
            let request: AudienceReleaseAttempt = serde_json::from_value(
                serde_json::json!({"challenge":vec![1u8;32],"request":original}),
            )
            .unwrap();
            let proof = request.binding().unwrap();
            let roundtrip: AudienceReleaseAttempt =
                serde_json::from_slice(&serde_json::to_vec(&request).unwrap()).unwrap();
            assert!(proof == roundtrip.binding().unwrap());
            let mut paths = Vec::new();
            for (key, value) in original.as_object().unwrap() {
                if key == "release" {
                    continue;
                }
                if let Some(object) = value.as_object() {
                    for child in object.keys() {
                        if child != "outcome" && child != "version" {
                            paths.push(vec![key.clone(), child.clone()]);
                        }
                    }
                } else {
                    paths.push(vec![key.clone()]);
                }
            }
            for path in paths {
                let mut changed = original.clone();
                let mut target = &mut changed;
                for key in &path {
                    target = &mut target[key];
                }
                *target = if target.is_number() {
                    serde_json::json!(3)
                } else if path.last().unwrap().ends_with("_id") {
                    serde_json::json!("01890f3e-53b7-7d28-9b05-4f65092d5712")
                } else {
                    serde_json::json!("different-private-proof")
                };
                if path.last().unwrap() == "game_code" {
                    *target = serde_json::json!("BCDEFGHJ");
                }
                if path == ["receipt", "completed_at"] || path == ["receipt", "expires_at"] {
                    changed["receipt"]["completed_at"] = 3.into();
                    changed["receipt"]["expires_at"] = 86400003.into();
                    changed["receipt"]["outcome"]["started_at"] = 3.into();
                }
                let changed: AudienceReleaseAttempt = serde_json::from_value(
                    serde_json::json!({"challenge":vec![1u8;32],"request":changed}),
                )
                .unwrap();
                assert!(proof != changed.binding().unwrap(), "{path:?}");
            }
        }
    }
}
