use crate::{error::CliError, operations::Prepared};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use brews_contracts::management::{ManagementCommand, ManagementReceipt, ReceiptOperation};
use brews_domain::accounts::AccessLinkPurpose;
use zeroize::Zeroizing;

const ENROLLMENT_PATH: &str = "/enroll";
const PASSWORD_RESET_PATH: &str = "/password-reset";
const LINK_TOKEN_BYTES: usize = 32;
const LINK_TOKEN_ENCODED_BYTES: usize = (LINK_TOKEN_BYTES * 8).div_ceil(6);

pub(crate) fn validate_response(
    prepared: &Prepared,
    response: &brews_contracts::management::ManagementResponse,
) -> Result<(), CliError> {
    use ManagementCommand as Command;
    use brews_contracts::management::ManagementResponse as Response;
    let valid = match (&prepared.command, response) {
        (Command::ListAccounts { limit, .. }, Response::Accounts { accounts, .. }) => {
            accounts.len() <= *limit as usize
        }
        (Command::GetAccount { account_id }, Response::Account { account }) => {
            *account_id == account.account_id
        }
        (Command::ListAudit { limit, .. }, Response::Audit { events, .. }) => {
            events.len() <= *limit as usize
                && events
                    .iter()
                    .all(|event| event.expires_at > event.occurred_at)
        }
        (
            Command::DisableAccount { .. } | Command::DeleteAccount { .. },
            Response::Pending { .. },
        ) => true,
        (_, Response::Committed { receipt }) => return validate_receipt(prepared, receipt),
        (
            Command::CreateAccount { .. }
            | Command::ReissueEnrollment { .. }
            | Command::ResetPassword { .. },
            Response::Issued { receipt, url },
        ) => {
            let parsed = url::Url::parse(url).map_err(|_| CliError::Protocol)?;
            let expected_path = if matches!(&prepared.command, Command::ResetPassword { .. }) {
                PASSWORD_RESET_PATH
            } else {
                ENROLLMENT_PATH
            };
            if parsed.scheme() != "https"
                || parsed.host_str().is_none()
                || !parsed.username().is_empty()
                || parsed.password().is_some()
                || parsed.path() != expected_path
                || parsed.query().is_some()
                || parsed.as_str() != url
                || url.chars().any(|c| c.is_control() || c.is_whitespace())
            {
                return Err(CliError::Protocol);
            }
            let token = parsed.fragment().ok_or(CliError::Protocol)?;
            if token.len() != LINK_TOKEN_ENCODED_BYTES {
                return Err(CliError::Protocol);
            }
            let mut decoded = Zeroizing::new([0u8; LINK_TOKEN_BYTES]);
            let written = URL_SAFE_NO_PAD
                .decode_slice(token, &mut *decoded)
                .map_err(|_| CliError::Protocol)?;
            if written != LINK_TOKEN_BYTES {
                return Err(CliError::Protocol);
            }
            return validate_receipt(prepared, receipt);
        }
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(CliError::Protocol)
    }
}

pub(crate) fn validate_receipt(
    prepared: &Prepared,
    receipt: &ManagementReceipt,
) -> Result<(), CliError> {
    use ManagementCommand as Command;
    let (operation, account_id, purpose) = match &prepared.command {
        Command::CreateAccount { .. } => (
            ReceiptOperation::CreateAccount,
            None,
            Some(AccessLinkPurpose::Enrollment),
        ),
        Command::ReissueEnrollment { account_id } => (
            ReceiptOperation::ReissueEnrollment,
            Some(*account_id),
            Some(AccessLinkPurpose::Enrollment),
        ),
        Command::ResetPassword { account_id } => (
            ReceiptOperation::ResetPassword,
            Some(*account_id),
            Some(AccessLinkPurpose::PasswordReset),
        ),
        Command::DisableAccount { account_id } => {
            (ReceiptOperation::DisableAccount, Some(*account_id), None)
        }
        Command::DeleteAccount { account_id } => {
            (ReceiptOperation::DeleteAccount, Some(*account_id), None)
        }
        Command::EnableAccount { account_id } => {
            (ReceiptOperation::EnableAccount, Some(*account_id), None)
        }
        _ => return Err(CliError::Protocol),
    };
    if receipt.version != 1
        || Some(receipt.command_id) != prepared.command_id
        || receipt.operation != operation
        || account_id.is_some_and(|id| id != receipt.account_id)
        || receipt.completed_at < 0
        || receipt.expires_at <= receipt.completed_at
        || receipt.purpose != purpose
    {
        return Err(CliError::Protocol);
    }
    if purpose.is_some() {
        if receipt.link_id.is_none()
            || receipt
                .link_expires_at
                .is_none_or(|expires| expires <= receipt.completed_at)
        {
            return Err(CliError::Protocol);
        }
    } else if receipt.link_id.is_some() || receipt.link_expires_at.is_some() {
        return Err(CliError::Protocol);
    }
    Ok(())
}
