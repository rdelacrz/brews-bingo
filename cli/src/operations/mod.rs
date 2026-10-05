//! Concrete account/audit request construction and safe operation execution.
mod accounts;
mod audit;

use crate::{
    args::RootCommand, client::ManagementClient, error::CliError, link_file::LinkFile, output,
};
use brews_config::env::cli::CliConfig;
use brews_contracts::management::{ManagementCommand, ManagementResponse};
use brews_domain::ids::CommandId;
use std::{io::Write, path::PathBuf};

const MAX_PAGE_ENTRIES: u32 = 100;

pub(crate) struct Prepared {
    pub(crate) command: ManagementCommand,
    pub(crate) command_id: Option<CommandId>,
    pub(crate) link_output: Option<PathBuf>,
}

pub(crate) fn prepare(operation: RootCommand) -> Result<Prepared, CliError> {
    let mut prepared = match operation {
        RootCommand::Accounts(operation) => accounts::prepare(operation)?,
        RootCommand::Audit(operation) => audit::prepare(operation)?,
    };
    if prepared.command.is_mutating() && prepared.command_id.is_none() {
        // Native uuid v7 uses the platform CSPRNG and has no entropy fallback.
        prepared.command_id =
            Some(CommandId::try_from(uuid::Uuid::now_v7()).map_err(|_| CliError::InvalidInput)?);
    }
    Ok(prepared)
}

pub(crate) fn execute(
    config: &CliConfig,
    prepared: Prepared,
    json: bool,
    out: &mut impl Write,
    notice: &mut impl Write,
) -> Result<(), CliError> {
    let mut link_file = prepared
        .link_output
        .as_deref()
        .map(LinkFile::reserve)
        .transpose()?;
    let client = ManagementClient::new(config)?;
    if let Some(command_id) = prepared.command_id {
        output::write_notice(notice, command_id, json)?;
    }
    let mut response = client.execute(&prepared)?;
    if let ManagementResponse::Issued { url, .. } = &mut response {
        let url = zeroize::Zeroizing::new(std::mem::take(url));
        link_file
            .as_mut()
            .ok_or(CliError::Protocol)?
            .persist(&url)?;
    }
    output::write_response(out, &response, json)
}

#[cfg(test)]
pub(crate) fn run(
    config: &CliConfig,
    cli: crate::args::Cli,
    out: &mut impl Write,
    notice: &mut impl Write,
) -> Result<(), CliError> {
    let json = cli.json;
    execute(config, prepare(cli.command)?, json, out, notice)
}
