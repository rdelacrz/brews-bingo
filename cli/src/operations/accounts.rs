use super::{MAX_PAGE_ENTRIES, Prepared};
use crate::{args::AccountCommand, error::CliError};
use brews_contracts::management::ManagementCommand as Command;

pub(super) fn prepare(operation: AccountCommand) -> Result<Prepared, CliError> {
    if matches!(
        &operation,
        AccountCommand::Disable { yes: false, .. } | AccountCommand::Delete { yes: false, .. }
    ) {
        return Err(CliError::Confirmation);
    }
    let (command, command_id, link_output) = match operation {
        AccountCommand::List(page) => {
            if !(1..=MAX_PAGE_ENTRIES).contains(&page.limit) {
                return Err(CliError::InvalidInput);
            }
            (
                Command::ListAccounts {
                    after: page.after,
                    limit: page.limit,
                },
                None,
                None,
            )
        }
        AccountCommand::Get { account_id } => (Command::GetAccount { account_id }, None, None),
        AccountCommand::Create {
            role,
            username,
            link,
        } => {
            let username = brews_domain::accounts::validate_username(&username)
                .map_err(|_| CliError::InvalidInput)?
                .to_owned();
            (
                Command::CreateAccount { role, username },
                link.mutation.command_id,
                Some(link.link_output),
            )
        }
        AccountCommand::ReissueEnrollment { account_id, link } => (
            Command::ReissueEnrollment { account_id },
            link.mutation.command_id,
            Some(link.link_output),
        ),
        AccountCommand::ResetPassword { account_id, link } => (
            Command::ResetPassword { account_id },
            link.mutation.command_id,
            Some(link.link_output),
        ),
        AccountCommand::Disable {
            account_id,
            mutation,
            ..
        } => (
            Command::DisableAccount { account_id },
            mutation.command_id,
            None,
        ),
        AccountCommand::Delete {
            account_id,
            mutation,
            ..
        } => (
            Command::DeleteAccount { account_id },
            mutation.command_id,
            None,
        ),
        AccountCommand::Enable {
            account_id,
            mutation,
        } => (
            Command::EnableAccount { account_id },
            mutation.command_id,
            None,
        ),
    };
    Ok(Prepared {
        command,
        command_id,
        link_output,
    })
}
