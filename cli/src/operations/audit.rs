use super::{MAX_PAGE_ENTRIES, Prepared};
use crate::{args::AuditCommand, error::CliError};
use brews_contracts::management::ManagementCommand;
pub(super) fn prepare(operation: AuditCommand) -> Result<Prepared, CliError> {
    let (page, game) = match operation {
        AuditCommand::List(page) => (page, None),
        AuditCommand::Game { game_id, page } => (page, Some(game_id)),
    };
    if !(1..=MAX_PAGE_ENTRIES).contains(&page.limit) {
        return Err(CliError::InvalidInput);
    }
    let command = match game {
        Some(game_id) => ManagementCommand::ListGameAudit {
            game_id,
            after: page.after,
            limit: page.limit,
        },
        None => ManagementCommand::ListAudit {
            after: page.after,
            limit: page.limit,
        },
    };
    Ok(Prepared {
        command,
        command_id: None,
        link_output: None,
    })
}
