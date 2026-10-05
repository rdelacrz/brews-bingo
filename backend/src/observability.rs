//! Allocation-conscious security records: closed tags only, never request/error data.

use tracing_subscriber::fmt::MakeWriter;

const TARGET: &str = "brews.security";

#[derive(Clone, Copy)]
pub(crate) enum Boundary {
    AuthIngress,
    CliIngress,
    AccountsAuth,
    AccountsManagement,
    AccountsAlarm,
    DirectoryRequest,
    DirectoryAlarm,
    DirectoryPeer,
    AccountsPeer,
    Durability,
}

impl Boundary {
    fn as_str(self) -> &'static str {
        match self {
            Self::AuthIngress => "auth_ingress",
            Self::CliIngress => "cli_ingress",
            Self::AccountsAuth => "accounts_auth",
            Self::AccountsManagement => "accounts_management",
            Self::AccountsAlarm => "accounts_alarm",
            Self::DirectoryRequest => "directory_request",
            Self::DirectoryAlarm => "directory_alarm",
            Self::DirectoryPeer => "directory_peer",
            Self::AccountsPeer => "accounts_peer",
            Self::Durability => "durability",
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Failure {
    InvalidInput,
    Forbidden,
    PayloadTooLarge,
    Configuration,
    Storage,
    Crypto,
    PeerTransport,
    PeerProtocol,
    ProofMismatch,
    RateLimited,
    Conflict,
    StaleCommand,
    InvalidCredentials,
    InvalidLink,
    Unavailable,
}

impl Failure {
    fn as_str(self) -> &'static str {
        match self {
            Self::InvalidInput => "invalid_input",
            Self::Forbidden => "forbidden",
            Self::PayloadTooLarge => "payload_too_large",
            Self::Configuration => "configuration",
            Self::Storage => "storage",
            Self::Crypto => "crypto",
            Self::PeerTransport => "peer_transport",
            Self::PeerProtocol => "peer_protocol",
            Self::ProofMismatch => "proof_mismatch",
            Self::RateLimited => "rate_limited",
            Self::Conflict => "conflict",
            Self::StaleCommand => "stale_command",
            Self::InvalidCredentials => "invalid_credentials",
            Self::InvalidLink => "invalid_link",
            Self::Unavailable => "unavailable",
        }
    }

    fn is_error(self) -> bool {
        match self {
            Self::Configuration
            | Self::Storage
            | Self::Crypto
            | Self::PeerProtocol
            | Self::ProofMismatch
            | Self::Unavailable => true,
            Self::InvalidInput
            | Self::Forbidden
            | Self::PayloadTooLarge
            | Self::PeerTransport
            | Self::RateLimited
            | Self::Conflict
            | Self::StaleCommand
            | Self::InvalidCredentials
            | Self::InvalidLink => false,
        }
    }
}

pub(crate) fn failure(boundary: Boundary, reason: Failure) {
    if matches!(boundary, Boundary::Durability) || reason.is_error() {
        tracing::error!(target: TARGET, event = "boundary_failure", boundary = boundary.as_str(), reason = reason.as_str());
    } else {
        tracing::warn!(target: TARGET, event = "boundary_failure", boundary = boundary.as_str(), reason = reason.as_str());
    }
}

#[derive(Clone, Copy)]
pub(crate) enum RemovalEvent {
    Committed,
    AbortStarted,
    Aborted,
    RetryScheduled,
    AwaitingAcknowledgements,
    Completed,
}

impl RemovalEvent {
    fn as_str(self) -> &'static str {
        match self {
            Self::Committed => "committed",
            Self::AbortStarted => "abort_started",
            Self::Aborted => "aborted",
            Self::RetryScheduled => "retry_scheduled",
            Self::AwaitingAcknowledgements => "awaiting_acknowledgements",
            Self::Completed => "completed",
        }
    }
}

pub(crate) fn removal(event: RemovalEvent, operation_id: brews_domain::ids::OperationId) {
    match event {
        RemovalEvent::Committed | RemovalEvent::Aborted | RemovalEvent::Completed => {
            tracing::info!(target: TARGET, event = "removal_state", action = event.as_str(), %operation_id);
        }
        RemovalEvent::AbortStarted
        | RemovalEvent::RetryScheduled
        | RemovalEvent::AwaitingAcknowledgements => {
            tracing::warn!(target: TARGET, event = "removal_state", action = event.as_str(), %operation_id);
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Delivery {
    Issued,
    Committed,
    Pending,
}

pub(crate) fn management_delivery(
    operation: brews_contracts::management::ReceiptOperation,
    command_id: brews_domain::ids::CommandId,
    delivery: Delivery,
) {
    use brews_contracts::management::ReceiptOperation;
    let operation = match operation {
        ReceiptOperation::CreateAccount => "create_account",
        ReceiptOperation::ReissueEnrollment => "reissue_enrollment",
        ReceiptOperation::ResetPassword => "reset_password",
        ReceiptOperation::DisableAccount => "disable_account",
        ReceiptOperation::DeleteAccount => "delete_account",
        ReceiptOperation::EnableAccount => "enable_account",
    };
    let delivery = match delivery {
        Delivery::Issued => "issued",
        Delivery::Committed => "committed",
        Delivery::Pending => "pending",
    };
    tracing::info!(target: TARGET, event = "management_delivery", operation, %command_id, delivery);
}

pub(crate) fn api_failure(boundary: Boundary, error: crate::api::ApiError) {
    use crate::api::ApiError;
    let reason = match error {
        ApiError::InvalidInput | ApiError::UnsupportedMediaType => Failure::InvalidInput,
        ApiError::Forbidden => Failure::Forbidden,
        ApiError::PayloadTooLarge => Failure::PayloadTooLarge,
        ApiError::Unavailable => Failure::Unavailable,
        ApiError::NotFound | ApiError::MethodNotAllowed => return,
    };
    failure(boundary, reason);
}

pub(crate) fn auth_failure(boundary: Boundary, error: crate::auth::AuthError) {
    use crate::auth::AuthError;
    let reason = match error {
        AuthError::InvalidInput => Failure::InvalidInput,
        AuthError::InvalidCredentials => Failure::InvalidCredentials,
        AuthError::InvalidLink => Failure::InvalidLink,
        AuthError::RateLimited => Failure::RateLimited,
        AuthError::Conflict => Failure::Conflict,
        AuthError::StaleCommand => Failure::StaleCommand,
        AuthError::Storage => Failure::Storage,
        AuthError::Crypto => Failure::Crypto,
        AuthError::Unauthorized => return,
    };
    failure(boundary, reason);
}

pub(crate) fn management_failure(boundary: Boundary, error: crate::auth::ManagementError) {
    use crate::auth::ManagementError;
    let reason = match error {
        ManagementError::InvalidInput => Failure::InvalidInput,
        ManagementError::Unauthorized | ManagementError::Forbidden => Failure::Forbidden,
        ManagementError::Conflict
        | ManagementError::LastAdmin
        | ManagementError::HostedGame
        | ManagementError::Busy => Failure::Conflict,
        ManagementError::StaleCommand => Failure::StaleCommand,
        ManagementError::Storage => Failure::Storage,
        ManagementError::Crypto => Failure::Crypto,
        ManagementError::NotFound => return,
    };
    failure(boundary, reason);
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn init() {
    static INITIALIZED: std::sync::Once = std::sync::Once::new();
    INITIALIZED.call_once(|| {
        // new() selects one console JSON argument; with_pretty_level() would add CSS arguments.
        let writer = tracing_web::MakeWebConsoleWriter::new();
        // A pre-existing subscriber must not make Worker initialization panic.
        let _ = tracing::subscriber::set_global_default(subscriber(writer));
    });
}

fn subscriber<W>(writer: W) -> impl tracing::Subscriber + Send + Sync
where
    W: for<'a> MakeWriter<'a> + Send + Sync + 'static,
{
    use tracing_subscriber::{
        Layer,
        filter::{FilterExt, LevelFilter, filter_fn},
        layer::SubscriberExt,
    };
    tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .json()
            .without_time()
            .with_ansi(false)
            .with_target(true)
            .with_current_span(false)
            .with_span_list(false)
            .with_writer(writer)
            .with_filter(LevelFilter::INFO.and(filter_fn(|metadata| metadata.target() == TARGET))),
    )
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Tests fail fast on invalid fixtures."
)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::{
        io,
        sync::{Arc, Mutex},
    };
    use tracing_subscriber::fmt::MakeWriter;

    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<u8>>>);

    impl io::Write for Capture {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0
                .lock()
                .map_err(|_| io::Error::other("Log capture unavailable."))?
                .extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for Capture {
        type Writer = Self;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    fn capture(emit: impl FnOnce()) -> Vec<Value> {
        let output = Capture::default();
        tracing::subscriber::with_default(subscriber(output.clone()), emit);
        let bytes = output.0.lock().unwrap();
        let text = std::str::from_utf8(&bytes).unwrap();
        text.lines()
            .map(|line| {
                assert!(
                    line.len() <= 1024,
                    "Security record exceeds its byte budget."
                );
                serde_json::from_str(line).expect("Security record must be JSON.")
            })
            .collect()
    }

    #[test]
    fn typed_error_mappers_cover_every_variant_and_skip_routine_misses() {
        use crate::{
            api::ApiError,
            auth::{AuthError, ManagementError},
        };
        macro_rules! check {
            ($mapper:ident, $boundary:expr, $name:literal, $cases:expr) => {
                for (error, expected) in $cases {
                    let output = capture(|| $mapper($boundary, error));
                    let expected: Option<(&str, &str)> = expected;
                    match expected {
                        Some((reason, level)) => assert_eq!(output, [json!({
                            "level": level, "target": "brews.security",
                            "fields": {"event":"boundary_failure", "boundary":$name, "reason":reason}
                        })]),
                        None => assert!(output.is_empty(), "Routine miss must not emit a record."),
                    }
                }
            };
        }
        check!(
            api_failure,
            Boundary::CliIngress,
            "cli_ingress",
            [
                (ApiError::InvalidInput, Some(("invalid_input", "WARN"))),
                (ApiError::Forbidden, Some(("forbidden", "WARN"))),
                (ApiError::NotFound, None),
                (ApiError::MethodNotAllowed, None),
                (
                    ApiError::PayloadTooLarge,
                    Some(("payload_too_large", "WARN"))
                ),
                (
                    ApiError::UnsupportedMediaType,
                    Some(("invalid_input", "WARN"))
                ),
                (ApiError::Unavailable, Some(("unavailable", "ERROR"))),
            ]
        );
        check!(
            auth_failure,
            Boundary::AccountsAuth,
            "accounts_auth",
            [
                (AuthError::InvalidInput, Some(("invalid_input", "WARN"))),
                (
                    AuthError::InvalidCredentials,
                    Some(("invalid_credentials", "WARN"))
                ),
                (AuthError::InvalidLink, Some(("invalid_link", "WARN"))),
                (AuthError::Unauthorized, None),
                (AuthError::RateLimited, Some(("rate_limited", "WARN"))),
                (AuthError::Conflict, Some(("conflict", "WARN"))),
                (AuthError::StaleCommand, Some(("stale_command", "WARN"))),
                (AuthError::Storage, Some(("storage", "ERROR"))),
                (AuthError::Crypto, Some(("crypto", "ERROR"))),
            ]
        );
        check!(
            management_failure,
            Boundary::AccountsManagement,
            "accounts_management",
            [
                (
                    ManagementError::InvalidInput,
                    Some(("invalid_input", "WARN"))
                ),
                (ManagementError::Unauthorized, Some(("forbidden", "WARN"))),
                (ManagementError::Forbidden, Some(("forbidden", "WARN"))),
                (ManagementError::NotFound, None),
                (ManagementError::Conflict, Some(("conflict", "WARN"))),
                (
                    ManagementError::StaleCommand,
                    Some(("stale_command", "WARN"))
                ),
                (ManagementError::LastAdmin, Some(("conflict", "WARN"))),
                (ManagementError::HostedGame, Some(("conflict", "WARN"))),
                (ManagementError::Busy, Some(("conflict", "WARN"))),
                (ManagementError::Storage, Some(("storage", "ERROR"))),
                (ManagementError::Crypto, Some(("crypto", "ERROR"))),
            ]
        );
    }

    #[test]
    fn privileged_projection_omits_sensitive_spans_and_unrelated_events() {
        use brews_contracts::management::{
            ManagementCommand, ManagementReceipt, ManagementResponse, ReceiptOperation,
        };
        use brews_domain::{
            accounts::{AccessLinkPurpose, AccountRole},
            ids::OperationId,
        };
        use std::time::{SystemTime, UNIX_EPOCH};
        let secret = format!(
            "canary-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let command = ManagementCommand::CreateAccount {
            username: secret.clone(),
            role: AccountRole::Admin,
        };
        let response = ManagementResponse::Issued {
            receipt: ManagementReceipt {
                version: 1,
                command_id: "01890f3e-53b7-7d28-9b05-4f65092d5711".parse().unwrap(),
                operation: ReceiptOperation::CreateAccount,
                account_id: "01890f3e-53b7-7d28-9b05-4f65092d5712".parse().unwrap(),
                link_id: Some("01890f3e-53b7-7d28-9b05-4f65092d5713".parse().unwrap()),
                purpose: Some(AccessLinkPurpose::Enrollment),
                link_expires_at: Some(2),
                completed_at: 1,
                expires_at: 2,
            },
            url: format!("https://invalid.example/access?token={secret}"),
        };
        let ManagementCommand::CreateAccount { username, .. } = command else {
            unreachable!()
        };
        let ManagementResponse::Issued { receipt, url } = response else {
            unreachable!()
        };
        let operation_id: OperationId = "01890f3e-53b7-7d28-9b05-4f65092d5714".parse().unwrap();
        let output = capture(|| {
            let parent = tracing::info_span!(target: TARGET, "privileged_parent",
                username, %url, account_id = %receipt.account_id,
                link_id = ?receipt.link_id, password = secret.as_str(), cookie = secret.as_str(),
                authorization = secret.as_str(), headers = secret.as_str(), body = secret.as_str(),
                phc = secret.as_str(), ip = secret.as_str(), sql_row = secret.as_str(),
                runtime_error = secret.as_str(), error_chain = secret.as_str());
            let _parent = parent.enter();
            let child = tracing::info_span!(target: TARGET, "privileged_child", sensitive = secret.as_str());
            let _child = child.enter();
            tracing::error!(target: "third_party", sensitive = secret.as_str());
            tracing::warn!(target: "third_party", sensitive = secret.as_str());
            tracing::info!(target: "third_party", sensitive = secret.as_str());
            tracing::error!(target: "brews.security.extra", sensitive = secret.as_str());
            tracing::debug!(target: TARGET, sensitive = secret.as_str());
            tracing::trace!(target: TARGET, sensitive = secret.as_str());
            failure(Boundary::AccountsManagement, Failure::Forbidden);
            management_delivery(receipt.operation, receipt.command_id, Delivery::Issued);
            removal(RemovalEvent::Completed, operation_id);
        });
        // Boolean-only assertions must not print canaries or DTO contents on failure.
        assert!(output.len() == 3, "Unexpected security record count.");
        assert!(
            output
                == [
                    json!({"level":"WARN", "target":"brews.security", "fields":{"event":"boundary_failure", "boundary":"accounts_management", "reason":"forbidden"}}),
                    json!({"level":"INFO", "target":"brews.security", "fields":{"event":"management_delivery", "operation":"create_account", "command_id":receipt.command_id.to_string(), "delivery":"issued"}}),
                    json!({"level":"INFO", "target":"brews.security", "fields":{"event":"removal_state", "action":"completed", "operation_id":operation_id.to_string()}}),
                ],
            "Security records must contain only the safe projection."
        );
        let bytes = serde_json::to_vec(&output).unwrap();
        assert!(
            !bytes
                .windows(secret.len())
                .any(|window| window == secret.as_bytes()),
            "Sensitive canary reached a security record."
        );
    }

    #[test]
    fn typed_lifecycle_records_use_closed_fields_and_canonical_ids() {
        use brews_contracts::management::ReceiptOperation;
        use brews_domain::ids::{CommandId, OperationId};
        const ID: &str = "ffffffff-ffff-7fff-bfff-ffffffffffff";
        let operation_id: OperationId = ID.parse().unwrap();
        let command_id: CommandId = ID.parse().unwrap();
        let actions = [
            (RemovalEvent::Committed, "committed", "INFO"),
            (RemovalEvent::AbortStarted, "abort_started", "WARN"),
            (RemovalEvent::Aborted, "aborted", "INFO"),
            (RemovalEvent::RetryScheduled, "retry_scheduled", "WARN"),
            (
                RemovalEvent::AwaitingAcknowledgements,
                "awaiting_acknowledgements",
                "WARN",
            ),
            (RemovalEvent::Completed, "completed", "INFO"),
        ];
        let operations = [
            (ReceiptOperation::CreateAccount, "create_account"),
            (ReceiptOperation::ReissueEnrollment, "reissue_enrollment"),
            (ReceiptOperation::ResetPassword, "reset_password"),
            (ReceiptOperation::DisableAccount, "disable_account"),
            (ReceiptOperation::DeleteAccount, "delete_account"),
            (ReceiptOperation::EnableAccount, "enable_account"),
        ];
        let deliveries = [
            (Delivery::Issued, "issued"),
            (Delivery::Committed, "committed"),
            (Delivery::Pending, "pending"),
        ];
        let output = capture(|| {
            for (action, _, _) in actions {
                removal(action, operation_id);
            }
            for (operation, _) in operations {
                for (delivery, _) in deliveries {
                    management_delivery(operation, command_id, delivery);
                }
            }
        });
        assert_eq!(
            output.len(),
            actions.len() + operations.len() * deliveries.len()
        );
        for (record, (_, action, level)) in output.iter().zip(actions) {
            assert_eq!(
                record,
                &json!({
                    "level": level, "target": "brews.security",
                    "fields": { "event": "removal_state", "action": action, "operation_id": ID }
                })
            );
        }
        for (record, ((_, operation), (_, delivery))) in output.iter().skip(actions.len()).zip(
            operations
                .iter()
                .flat_map(|operation| deliveries.iter().map(move |delivery| (operation, delivery))),
        ) {
            assert_eq!(
                record,
                &json!({
                    "level": "INFO", "target": "brews.security",
                    "fields": { "event": "management_delivery", "operation": operation, "command_id": ID, "delivery": delivery }
                })
            );
        }
    }

    #[test]
    fn boundary_failures_have_closed_fields_and_expected_levels() {
        let boundaries = [
            (Boundary::AuthIngress, "auth_ingress"),
            (Boundary::CliIngress, "cli_ingress"),
            (Boundary::AccountsAuth, "accounts_auth"),
            (Boundary::AccountsManagement, "accounts_management"),
            (Boundary::AccountsAlarm, "accounts_alarm"),
            (Boundary::DirectoryRequest, "directory_request"),
            (Boundary::DirectoryAlarm, "directory_alarm"),
            (Boundary::DirectoryPeer, "directory_peer"),
            (Boundary::AccountsPeer, "accounts_peer"),
            (Boundary::Durability, "durability"),
        ];
        let reasons = [
            (Failure::InvalidInput, "invalid_input", "WARN"),
            (Failure::Forbidden, "forbidden", "WARN"),
            (Failure::PayloadTooLarge, "payload_too_large", "WARN"),
            (Failure::Configuration, "configuration", "ERROR"),
            (Failure::Storage, "storage", "ERROR"),
            (Failure::Crypto, "crypto", "ERROR"),
            (Failure::PeerTransport, "peer_transport", "WARN"),
            (Failure::PeerProtocol, "peer_protocol", "ERROR"),
            (Failure::ProofMismatch, "proof_mismatch", "ERROR"),
            (Failure::RateLimited, "rate_limited", "WARN"),
            (Failure::Conflict, "conflict", "WARN"),
            (Failure::StaleCommand, "stale_command", "WARN"),
            (Failure::InvalidCredentials, "invalid_credentials", "WARN"),
            (Failure::InvalidLink, "invalid_link", "WARN"),
            (Failure::Unavailable, "unavailable", "ERROR"),
        ];
        let output = capture(|| {
            for (boundary, _) in boundaries {
                for (reason, _, _) in reasons {
                    failure(boundary, reason);
                }
            }
        });
        assert_eq!(output.len(), boundaries.len() * reasons.len());
        for (record, ((boundary, name), (_, reason, level))) in output.iter().zip(
            boundaries
                .iter()
                .flat_map(|boundary| reasons.iter().map(move |reason| (boundary, reason))),
        ) {
            let level = if matches!(boundary, Boundary::Durability) {
                "ERROR"
            } else {
                level
            };
            assert_eq!(
                record,
                &json!({
                    "level": level,
                    "target": "brews.security",
                    "fields": { "event": "boundary_failure", "boundary": name, "reason": reason }
                })
            );
        }
    }
}
