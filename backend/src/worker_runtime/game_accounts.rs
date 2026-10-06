//! Private Accounts authority metadata and exact connection-close acknowledgements.
use super::game_peers::{GamePeerError, SecretToken};
use crate::auth::{AuthError, GameAccountAuthority, GameSocketCloseWork};
use brews_domain::{
    accounts::AccountRole,
    ids::{AccountId, ConnectionId, GameId, OperationId, SessionId},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum AccountAction {
    Authorize,
    Register,
    AuthorizeConnection,
    Unregister,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum AccountRequest {
    Authorize {
        token: SecretToken,
    },
    Register {
        token: SecretToken,
        game_id: GameId,
        connection_id: ConnectionId,
    },
    AuthorizeConnection {
        #[serde(deserialize_with = "super::game_peers::object")]
        identity: AccountConnectionIdentity,
    },
    Unregister {
        #[serde(deserialize_with = "super::game_peers::object")]
        identity: AccountConnectionIdentity,
    },
}
impl AccountRequest {
    pub(super) fn action(&self) -> AccountAction {
        match self {
            Self::Authorize { .. } => AccountAction::Authorize,
            Self::Register { .. } => AccountAction::Register,
            Self::AuthorizeConnection { .. } => AccountAction::AuthorizeConnection,
            Self::Unregister { .. } => AccountAction::Unregister,
        }
    }
    fn target(&self) -> Option<AccountTarget> {
        match self {
            Self::Authorize { .. } => None,
            Self::Register {
                game_id,
                connection_id,
                ..
            } => Some(AccountTarget {
                game_id: *game_id,
                connection_id: *connection_id,
            }),
            Self::AuthorizeConnection { identity } | Self::Unregister { identity } => {
                Some(identity.target())
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AccountConnectionIdentity {
    pub(super) account_id: AccountId,
    pub(super) session_id: SessionId,
    pub(super) game_id: GameId,
    pub(super) connection_id: ConnectionId,
    #[serde(deserialize_with = "super::game_peers::safe_integer")]
    pub(super) epoch: i64,
    #[serde(deserialize_with = "super::game_peers::positive_integer")]
    pub(super) expires: i64,
}
impl AccountConnectionIdentity {
    fn target(&self) -> AccountTarget {
        AccountTarget {
            game_id: self.game_id,
            connection_id: self.connection_id,
        }
    }
    pub(super) fn from_authority(
        proof: &GameAccountAuthority,
        game_id: GameId,
        connection_id: ConnectionId,
    ) -> Self {
        Self {
            account_id: proof.account_id(),
            session_id: proof.session_id(),
            game_id,
            connection_id,
            epoch: proof.credential_epoch(),
            expires: proof.expires_at(),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AccountTarget {
    game_id: GameId,
    connection_id: ConnectionId,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AccountAuthorityWire {
    account_id: AccountId,
    session_id: SessionId,
    role: AccountRole,
    #[serde(deserialize_with = "super::game_peers::safe_integer")]
    epoch: i64,
    #[serde(deserialize_with = "super::game_peers::positive_integer")]
    expires: i64,
}
impl From<&GameAccountAuthority> for AccountAuthorityWire {
    fn from(p: &GameAccountAuthority) -> Self {
        Self {
            account_id: p.account_id(),
            session_id: p.session_id(),
            role: p.role(),
            epoch: p.credential_epoch(),
            expires: p.expires_at(),
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum AccountOutcome {
    Authorized {
        #[serde(deserialize_with = "super::game_peers::object")]
        authority: AccountAuthorityWire,
    },
    Unregistered {
        #[serde(deserialize_with = "super::game_peers::object")]
        identity: AccountConnectionIdentity,
    },
    Rejected {
        code: AccountRejection,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum AccountRejection {
    Unauthorized,
    Conflict,
    InvalidInput,
    Unavailable,
}
impl From<AuthError> for AccountRejection {
    fn from(e: AuthError) -> Self {
        match e {
            AuthError::Unauthorized | AuthError::InvalidCredentials => Self::Unauthorized,
            AuthError::Conflict => Self::Conflict,
            AuthError::InvalidInput => Self::InvalidInput,
            _ => Self::Unavailable,
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AccountReply {
    action: AccountAction,
    #[serde(deserialize_with = "super::game_peers::optional_object")]
    target: Option<AccountTarget>,
    #[serde(deserialize_with = "super::game_peers::object")]
    outcome: AccountOutcome,
}

pub(super) fn verified_authority(
    request: &AccountRequest,
    reply: AccountReply,
    now: i64,
) -> Result<GameAccountAuthority, GamePeerError> {
    verify_binding(request, &reply)?;
    let wire = match reply.outcome {
        AccountOutcome::Authorized { authority } => authority,
        AccountOutcome::Rejected { code } => return Err(code.into()),
        _ => return Err(GamePeerError::Unavailable),
    };
    if wire.expires <= now || now < 0 {
        return Err(GamePeerError::Unavailable);
    }
    let proof = GameAccountAuthority::from_trusted_peer(
        wire.account_id,
        wire.session_id,
        wire.role,
        wire.epoch,
        wire.expires,
    )
    .map_err(|_| GamePeerError::Unavailable)?;
    if let AccountRequest::AuthorizeConnection { identity } = request
        && AccountConnectionIdentity::from_authority(
            &proof,
            identity.game_id,
            identity.connection_id,
        ) != *identity
    {
        return Err(GamePeerError::Unavailable);
    }
    if matches!(request, AccountRequest::Unregister { .. }) {
        return Err(GamePeerError::Unavailable);
    }
    Ok(proof)
}
pub(super) fn verified_unregistered(
    request: &AccountRequest,
    reply: AccountReply,
) -> Result<(), GamePeerError> {
    verify_binding(request, &reply)?;
    match (request, reply.outcome) {
        (
            AccountRequest::Unregister { identity },
            AccountOutcome::Unregistered { identity: found },
        ) if *identity == found => Ok(()),
        (_, AccountOutcome::Rejected { code }) => Err(code.into()),
        _ => Err(GamePeerError::Unavailable),
    }
}
fn verify_binding(request: &AccountRequest, reply: &AccountReply) -> Result<(), GamePeerError> {
    if request.action() != reply.action || request.target() != reply.target {
        Err(GamePeerError::Unavailable)
    } else {
        Ok(())
    }
}
impl From<AccountRejection> for GamePeerError {
    fn from(code: AccountRejection) -> Self {
        match code {
            AccountRejection::Unauthorized => Self::Unauthorized,
            AccountRejection::Conflict => Self::Conflict,
            AccountRejection::InvalidInput => Self::Unavailable,
            AccountRejection::Unavailable => Self::Unavailable,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CloseCaller {
    Accounts,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CloseAction {
    CloseAccount,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AccountCloseIdentity {
    pub(super) operation_id: OperationId,
    pub(super) account_id: AccountId,
    pub(super) session_id: SessionId,
    pub(super) game_id: GameId,
    pub(super) connection_id: ConnectionId,
    #[serde(deserialize_with = "super::game_peers::safe_integer")]
    pub(super) epoch: i64,
    #[serde(deserialize_with = "super::game_peers::positive_integer")]
    pub(super) expires: i64,
    #[serde(deserialize_with = "super::game_peers::safe_integer")]
    pub(super) created_at: i64,
}
impl From<&GameSocketCloseWork> for AccountCloseIdentity {
    fn from(w: &GameSocketCloseWork) -> Self {
        Self {
            operation_id: w.operation_id(),
            account_id: w.account_id(),
            session_id: w.session_id(),
            game_id: w.game_id(),
            connection_id: w.connection_id(),
            epoch: w.credential_epoch(),
            expires: w.expires_at(),
            created_at: w.created_at(),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AccountCloseRequest {
    pub(super) caller: CloseCaller,
    pub(super) action: CloseAction,
    #[serde(deserialize_with = "super::game_peers::object")]
    pub(super) target: AccountCloseIdentity,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CloseResult {
    Closed,
    Unavailable,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AccountCloseReply {
    pub(super) result: CloseResult,
    #[serde(deserialize_with = "super::game_peers::object")]
    pub(super) request: AccountCloseRequest,
}
pub(super) fn verify_closed(
    request: &AccountCloseRequest,
    reply: AccountCloseReply,
) -> Result<(), GamePeerError> {
    if reply.result == CloseResult::Closed && reply.request == *request {
        Ok(())
    } else {
        Err(GamePeerError::Unavailable)
    }
}

pub(super) fn apply_account_request<D: crate::db::Database, R: crate::auth::Runtime>(
    service: &crate::auth::AuthService<'_, D, R>,
    request: &AccountRequest,
    register: bool,
) -> AccountReply {
    let result = match request {
        AccountRequest::Authorize { token } => {
            service
                .authorize_game_account(token.as_str())
                .map(|p| AccountOutcome::Authorized {
                    authority: (&p).into(),
                })
        }
        AccountRequest::Register {
            token,
            game_id,
            connection_id,
        } => {
            let proof = if register {
                service.register_game_account_connection(token.as_str(), *game_id, *connection_id)
            } else {
                service
                    .authorize_game_account(token.as_str())
                    .and_then(|p| {
                        service.authorize_game_account_connection(
                            p.account_id(),
                            p.session_id(),
                            p.credential_epoch(),
                            *game_id,
                            *connection_id,
                        )
                    })
            };
            proof.map(|p| AccountOutcome::Authorized {
                authority: (&p).into(),
            })
        }
        AccountRequest::AuthorizeConnection { identity } => service
            .authorize_game_account_connection(
                identity.account_id,
                identity.session_id,
                identity.epoch,
                identity.game_id,
                identity.connection_id,
            )
            .and_then(|p| {
                if p.expires_at() != identity.expires {
                    return Err(AuthError::Unauthorized);
                }
                Ok(AccountOutcome::Authorized {
                    authority: (&p).into(),
                })
            }),
        AccountRequest::Unregister { identity } => service
            .unregister_game_account_connection(
                identity.account_id,
                identity.session_id,
                identity.epoch,
                identity.game_id,
                identity.connection_id,
            )
            .map(|()| AccountOutcome::Unregistered {
                identity: *identity,
            }),
    };
    AccountReply {
        action: request.action(),
        target: request.target(),
        outcome: result.unwrap_or_else(|e| AccountOutcome::Rejected { code: e.into() }),
    }
}

#[cfg(target_arch = "wasm32")]
impl super::AccountsObject {
    pub(super) async fn execute_game_authority_request(
        &self,
        request: worker::Request,
    ) -> worker::Result<worker::Response> {
        use super::{OwnerDatabase, WorkerRuntime};
        use axum::body::{Body, to_bytes};
        if request.method() != worker::Method::Post
            || request.path() != "/game-authority"
            || request.url()?.query().is_some()
        {
            return worker::Response::error("invalid private game request", 400);
        }
        let http: worker::HttpRequest = request.try_into()?;
        let bytes = zeroize::Zeroizing::new(
            to_bytes(
                Body::new(http.into_body()),
                super::game_peers::GAME_PEER_MAX_BYTES,
            )
            .await
            .map_err(|_| worker::Error::RustError("private request bound".into()))?
            .to_vec(),
        );
        let message: AccountRequest = match super::game_peers::decode(&bytes) {
            Ok(m) => m,
            Err(_) => return worker::Response::error("invalid private game request", 400),
        };
        let db = OwnerDatabase::new(self.state.storage());
        let rt = WorkerRuntime;
        let result = match crate::db::migrate(&db)
            .map_err(AuthError::from)
            .and_then(|()| self.service(&db, &rt))
        {
            Ok(service) => {
                let result = apply_account_request(&service, &message, true);
                // Registration and the combined recovery alarm precede every outbound proof.
                self.game_accounts_output_gate(&service)
                    .await
                    .map_err(|_| {
                        worker::Error::RustError("game authority durability failed".into())
                    })?;
                if matches!(&result.outcome, AccountOutcome::Authorized { .. }) {
                    apply_account_request(&service, &message, false)
                } else {
                    result
                }
            }
            Err(error) => AccountReply {
                action: message.action(),
                target: message.target(),
                outcome: AccountOutcome::Rejected { code: error.into() },
            },
        };
        // Encoding contains no cookie/token. A failed gate never releases metadata as proof.
        let encoded = super::game_peers::encode(&result)
            .map_err(|_| worker::Error::RustError("private response bound".into()))?;
        let mut response = worker::Response::from_bytes(encoded.to_vec())?;
        response
            .headers_mut()
            .set("Content-Type", "application/json")?;
        response.headers_mut().set("Cache-Control", "no-store")?;
        Ok(response)
    }
    pub(super) async fn game_accounts_output_gate(
        &self,
        service: &crate::auth::AuthService<'_, super::OwnerDatabase, super::WorkerRuntime>,
    ) -> Result<(), AuthError> {
        self.schedule(Self::combined_deadline(service)?)
            .await
            .map_err(|_| {
                crate::observability::failure(
                    crate::observability::Boundary::Durability,
                    crate::observability::Failure::Storage,
                );
                AuthError::Storage
            })?;
        self.state.storage().sync().await.map_err(|_| {
            crate::observability::failure(
                crate::observability::Boundary::Durability,
                crate::observability::Failure::Storage,
            );
            AuthError::Storage
        })
    }
    pub(super) async fn dispatch_game_socket_closes(
        &self,
        service: &crate::auth::AuthService<'_, super::OwnerDatabase, super::WorkerRuntime>,
    ) -> Result<(), AuthError> {
        for work in service.list_due_game_socket_closes(super::game_peers::GAME_PEER_WORK_LIMIT)? {
            let request = AccountCloseRequest {
                caller: CloseCaller::Accounts,
                action: CloseAction::CloseAccount,
                target: (&work).into(),
            };
            self.game_accounts_output_gate(service).await?;
            let reply = super::game_peers::call_game_close(&self.env, &request).await;
            // Both helpers reload the durable target and fence every identity field.
            let result = if reply
                .and_then(|reply| verify_closed(&request, reply))
                .is_ok()
            {
                service.acknowledge_game_socket_closed(&work)
            } else {
                service.mark_game_socket_close_retry(&work)
            };
            if let Err(error) = result
                && error != AuthError::Conflict
            {
                return Err(error);
            }
        }
        self.game_accounts_output_gate(service).await
    }
}

#[cfg(test)]
use crate::auth::test_support;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Private wire fixtures fail fast."
)]
mod tests {
    use super::super::game_peers::decode;
    use super::*;
    fn id<T: std::str::FromStr>(n: u8) -> T
    where
        T::Err: std::fmt::Debug,
    {
        format!("01900000-0000-7000-8000-{n:012}").parse().unwrap()
    }
    fn proof() -> GameAccountAuthority {
        GameAccountAuthority::from_trusted_peer(id(1), id(2), AccountRole::Host, 3, 1000).unwrap()
    }
    fn reply(p: &GameAccountAuthority) -> AccountReply {
        AccountReply {
            action: AccountAction::Authorize,
            target: None,
            outcome: AccountOutcome::Authorized {
                authority: p.into(),
            },
        }
    }
    #[test]
    fn malformed_private_denial_and_wrong_attachment_never_become_authority() {
        let p = proof();
        let identity = AccountConnectionIdentity::from_authority(&p, id(4), id(5));
        let request = AccountRequest::AuthorizeConnection { identity };
        let good = || AccountReply {
            action: AccountAction::AuthorizeConnection,
            target: Some(identity.target()),
            outcome: AccountOutcome::Authorized {
                authority: (&p).into(),
            },
        };
        assert!(verified_authority(&request, good(), 900).is_ok());
        for field in 0..4 {
            let mut wrong = good();
            if let AccountOutcome::Authorized { authority } = &mut wrong.outcome {
                match field {
                    0 => authority.account_id = id(8),
                    1 => authority.session_id = id(8),
                    2 => authority.epoch += 1,
                    _ => authority.expires += 1,
                }
            };
            assert!(matches!(
                verified_authority(&request, wrong, 900),
                Err(GamePeerError::Unavailable)
            ));
        }
        let reply = AccountReply {
            action: request.action(),
            target: request.target(),
            outcome: AccountOutcome::Rejected {
                code: AccountRejection::InvalidInput,
            },
        };
        assert!(matches!(
            verified_authority(&request, reply, 900),
            Err(GamePeerError::Unavailable)
        ));
        for error in [
            AuthError::Storage,
            AuthError::Crypto,
            AuthError::InvalidLink,
            AuthError::RateLimited,
        ] {
            assert_eq!(AccountRejection::from(error), AccountRejection::Unavailable);
        }
        let wire = serde_json::to_string(&good()).unwrap();
        for bad in [
            wire.replace("\"epoch\":3", "\"epoch\":-1"),
            wire.replace("\"expires\":1000", "\"expires\":0"),
            wire.replace("\"expires\":1000", "\"expires\":9007199254740992"),
            wire.replace("\"epoch\":3", "\"epoch\":3.0"),
            wire.replace(
                "\"role\":\"Host\"",
                "\"role\":\"Host\",\"principal\":\"Admin\"",
            ),
        ] {
            assert!(decode::<AccountReply>(bad.as_bytes()).is_err());
        }
    }
    #[test]
    fn accounts_nested_metadata_requires_maps_and_safe_scalars() {
        let p = proof();
        let authority = serde_json::to_string(&AccountAuthorityWire::from(&p)).unwrap();
        let good = format!(
            r#"{{"action":"authorize","target":null,"outcome":{{"result":"authorized","authority":{authority}}}}}"#
        );
        assert!(decode::<AccountReply>(good.as_bytes()).is_ok());
        let sequence = format!(
            r#"["{}","{}","Host",3,1000]"#,
            p.account_id(),
            p.session_id()
        );
        assert!(decode::<AccountReply>(good.replace(&authority, &sequence).as_bytes()).is_err());
        assert!(
            decode::<AccountReply>(
                good.replace("\"epoch\":3", "\"epoch\":9007199254740992")
                    .as_bytes()
            )
            .is_err()
        );
        assert!(decode::<AccountReply>(good.replace("\"target\":null,", "").as_bytes()).is_err());
        assert!(
            decode::<AccountReply>(
                good.replace("\"epoch\":3", "\"epoch\":3,\"\\u0065poch\":3")
                    .as_bytes()
            )
            .is_err()
        );
    }
    #[test]
    fn account_close_ack_is_bound_to_every_identity_field() {
        let request = AccountCloseRequest {
            caller: CloseCaller::Accounts,
            action: CloseAction::CloseAccount,
            target: AccountCloseIdentity {
                operation_id: id(9),
                account_id: id(1),
                session_id: id(2),
                game_id: id(4),
                connection_id: id(5),
                epoch: 3,
                expires: 1000,
                created_at: 900,
            },
        };
        let reply = AccountCloseReply {
            result: CloseResult::Closed,
            request,
        };
        assert!(verify_closed(&request, reply).is_ok());
        for i in 0..8 {
            let mut wrong = reply;
            match i {
                0 => wrong.request.target.operation_id = id(8),
                1 => wrong.request.target.account_id = id(8),
                2 => wrong.request.target.session_id = id(8),
                3 => wrong.request.target.game_id = id(8),
                4 => wrong.request.target.connection_id = id(8),
                5 => wrong.request.target.epoch += 1,
                6 => wrong.request.target.expires += 1,
                _ => wrong.request.target.created_at += 1,
            };
            assert!(verify_closed(&request, wrong).is_err());
        }
        assert!(
            verify_closed(
                &request,
                AccountCloseReply {
                    result: CloseResult::Unavailable,
                    request
                }
            )
            .is_err()
        );
        let wire = serde_json::to_string(&reply).unwrap();
        assert!(decode::<AccountCloseReply>(wire.as_bytes()).is_ok());
        assert!(
            decode::<AccountCloseReply>(
                wire.replace("\"epoch\":3", "\"epoch\":3,\"\\u0065poch\":3")
                    .as_bytes()
            )
            .is_err()
        );
    }
    use super::test_support as support;
    #[test]
    fn account_registration_and_post_gate_recheck_use_real_accounts_core() {
        use crate::auth::{
            AuthCommand, AuthPolicy, AuthService, CookieEffect, ManagementPrincipal, RequestContext,
        };
        use crate::db::Database;
        use brews_contracts::management::{ManagementCommand, ManagementResponse};
        use brews_domain::accounts::{AccessLinkPurpose, SessionScope};
        let db = support::Sqlite::new();
        let rt = support::TestRuntime::new();
        crate::db::migrate(&db).unwrap();
        let service = AuthService::new(&db, &rt, AuthPolicy::default(), &[9; 32]).unwrap();
        let command = |n: u8| {
            let mut bytes = [0; 16];
            bytes[..6].copy_from_slice(&(rt.now.get() as u64).to_be_bytes()[2..]);
            bytes[6] = 0x70;
            bytes[8] = 0x80;
            bytes[15] = n;
            uuid::Uuid::from_bytes(bytes).to_string()
        };
        let created = service
            .execute_management(
                ManagementCommand::CreateAccount {
                    username: "PrivateGameHost".into(),
                    role: AccountRole::Host,
                },
                ManagementPrincipal::DeveloperCli,
                Some(command(1).parse().unwrap()),
                "https://app.example.test",
            )
            .unwrap();
        let link = match created {
            ManagementResponse::Issued { url, .. } => url.rsplit('#').next().unwrap().to_owned(),
            _ => panic!("Expected enrollment"),
        };
        let context = |n| RequestContext {
            command_id: Some(command(n)),
            caller_identity: "native-private-peer".into(),
        };
        let token = match service
            .execute(
                AuthCommand::Redeem {
                    purpose: AccessLinkPurpose::Enrollment,
                    token: link,
                },
                context(2),
            )
            .unwrap()
            .cookie
        {
            CookieEffect::Set { token, .. } => token,
            _ => panic!("Expected cookie"),
        };
        let token = match service
            .execute(
                AuthCommand::Complete {
                    scope: SessionScope::EnrollmentOnly,
                    token,
                    new_password: crate::security::new_token(&rt).unwrap(),
                },
                context(3),
            )
            .unwrap()
            .cookie
        {
            CookieEffect::Set { token, .. } => token,
            _ => panic!("Expected cookie"),
        };
        let request = AccountRequest::Register {
            token: SecretToken::new(&token),
            game_id: id(4),
            connection_id: id(5),
        };
        let initial = apply_account_request(&service, &request, true);
        let p = verified_authority(&request, initial, rt.now.get()).unwrap();
        let identity = AccountConnectionIdentity::from_authority(&p, id(4), id(5));
        let check = AccountRequest::AuthorizeConnection { identity };
        assert!(
            verified_authority(
                &check,
                apply_account_request(&service, &check, false),
                rt.now.get()
            )
            .is_ok()
        );
        let unregister = AccountRequest::Unregister { identity };
        let outcome = apply_account_request(&service, &unregister, true);
        assert!(verified_unregistered(&unregister, outcome).is_ok());
        assert!(matches!(
            apply_account_request(&service, &check, false).outcome,
            AccountOutcome::Rejected {
                code: AccountRejection::Unauthorized
            }
        ));
        let p = verified_authority(
            &request,
            apply_account_request(&service, &request, true),
            rt.now.get(),
        )
        .unwrap();
        assert_eq!(p.session_id(), identity.session_id);
        db.execute("UPDATE accounts SET disabled_at=created_at", &[])
            .unwrap();
        assert!(matches!(
            apply_account_request(&service, &request, false).outcome,
            AccountOutcome::Rejected {
                code: AccountRejection::Unauthorized
            }
        ));
        assert!(matches!(
            apply_account_request(&service, &check, false).outcome,
            AccountOutcome::Rejected {
                code: AccountRejection::Unauthorized
            }
        ));
    }
    #[test]
    fn genuine_account_reply_reconstructs_only_fresh_matching_authority() {
        let request = AccountRequest::Authorize {
            token: SecretToken::new("native-test-placeholder"),
        };
        let p = proof();
        let result = verified_authority(&request, reply(&p), 900).unwrap();
        assert_eq!(result.account_id(), p.account_id());
        assert!(verified_authority(&request, reply(&p), 1000).is_err());
        let mut wrong = reply(&p);
        wrong.action = AccountAction::Register;
        assert!(verified_authority(&request, wrong, 900).is_err());
        let mut wrong = reply(&p);
        wrong.target = Some(AccountTarget {
            game_id: id(4),
            connection_id: id(5),
        });
        assert!(verified_authority(&request, wrong, 900).is_err());
        let bytes = serde_json::to_vec(&reply(&p)).unwrap();
        assert!(decode::<AccountReply>(&bytes).is_ok());
    }
}
