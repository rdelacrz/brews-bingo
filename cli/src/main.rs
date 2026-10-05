//! Allocation-conscious native developer operations client.
#![forbid(unsafe_code)]
mod args;
mod client;
mod error;
#[cfg(test)]
#[path = "../tests/fixtures/mod.rs"]
mod fixtures;
mod link_file;
mod operations;
mod output;
mod protocol;

use clap::Parser;
use std::{io::Write, process::ExitCode};

fn main() -> ExitCode {
    let cli = match args::Cli::try_parse() {
        Ok(cli) => cli,
        Err(error)
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) =>
        {
            return if std::io::stdout()
                .lock()
                .write_all(error.to_string().as_bytes())
                .is_ok()
            {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            };
        }
        Err(_) => {
            let _ = output::write_error(
                &mut std::io::stderr().lock(),
                error::CliError::InvalidInput,
                true,
            );
            return ExitCode::FAILURE;
        }
    };
    let json = cli.json;
    let result = operations::prepare(cli.command).and_then(|prepared| {
        brews_config::env::cli::get_cli_config()
            .map_err(|_| error::CliError::Configuration)
            .and_then(|config| {
                operations::execute(
                    config,
                    prepared,
                    json,
                    &mut std::io::stdout().lock(),
                    &mut std::io::stderr().lock(),
                )
            })
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = output::write_error(&mut std::io::stderr().lock(), error, json);
            ExitCode::FAILURE
        }
    }
}
