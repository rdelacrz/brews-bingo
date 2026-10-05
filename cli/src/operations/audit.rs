use super::{MAX_PAGE_ENTRIES, Prepared};
use crate::{args::AuditCommand, error::CliError};
use brews_contracts::management::ManagementCommand;

pub(super) fn prepare(operation: AuditCommand) -> Result<Prepared, CliError> {
    let AuditCommand::List(page) = operation;
    if !(1..=MAX_PAGE_ENTRIES).contains(&page.limit) {
        return Err(CliError::InvalidInput);
    }
    Ok(Prepared {
        command: ManagementCommand::ListAudit {
            after: page.after,
            limit: page.limit,
        },
        command_id: None,
        link_output: None,
    })
}
