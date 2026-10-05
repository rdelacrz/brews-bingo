use brews_domain::{
    accounts::AccountRole,
    ids::{AccountId, CommandId},
};
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

const DEFAULT_PAGE_ENTRIES: u32 = 50;
fn canonical_id<T: std::str::FromStr>(value: &str) -> Result<T, &'static str> {
    const INVALID_ID: &str = "ID must be a canonical lowercase UUID-v7";
    if value.len() != 36
        || !value.bytes().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_digit() || matches!(b, b'a'..=b'f')
            }
        })
        || value.as_bytes()[14] != b'7'
        || !matches!(value.as_bytes()[19], b'8' | b'9' | b'a' | b'b')
    {
        return Err(INVALID_ID);
    }
    value.parse().map_err(|_| INVALID_ID)
}

#[derive(Parser)]
#[command(
    name = "brews",
    bin_name = "brews",
    about = "Restricted developer account operations"
)]
pub(crate) struct Cli {
    #[arg(long, global = true, help = "Emit safe JSON metadata, never link URLs")]
    pub(crate) json: bool,
    #[command(subcommand)]
    pub(crate) command: RootCommand,
}

#[derive(Subcommand)]
pub(crate) enum RootCommand {
    #[command(subcommand)]
    Accounts(AccountCommand),
    #[command(subcommand)]
    Audit(AuditCommand),
}

#[derive(Args)]
pub(crate) struct Page {
    #[arg(long, value_parser = canonical_id::<AccountId>)]
    pub(crate) after: Option<AccountId>,
    #[arg(long, default_value_t = DEFAULT_PAGE_ENTRIES)]
    pub(crate) limit: u32,
}

#[derive(Args)]
pub(crate) struct AuditPage {
    #[arg(long, value_parser = canonical_id::<brews_domain::ids::AuditId>)]
    pub(crate) after: Option<brews_domain::ids::AuditId>,
    #[arg(long, default_value_t = DEFAULT_PAGE_ENTRIES)]
    pub(crate) limit: u32,
}

#[derive(Args)]
pub(crate) struct Mutation {
    #[arg(long, value_parser = canonical_id::<CommandId>, help = "Exact lowercase UUID-v7; reuse for explicit uncertain-outcome retries")]
    pub(crate) command_id: Option<CommandId>,
}

#[derive(Args)]
pub(crate) struct LinkMutation {
    #[arg(
        long,
        help = "NEW private 0600 file; existing targets are rejected before HTTP"
    )]
    pub(crate) link_output: PathBuf,
    #[command(flatten)]
    pub(crate) mutation: Mutation,
}

#[derive(Subcommand)]
pub(crate) enum AccountCommand {
    List(Page),
    Get {
        #[arg(value_parser = canonical_id::<AccountId>)]
        account_id: AccountId,
    },
    Create {
        #[arg(long, help = "Account role: host or admin")]
        role: AccountRole,
        #[arg(long)]
        username: String,
        #[command(flatten)]
        link: LinkMutation,
    },
    ReissueEnrollment {
        #[arg(value_parser = canonical_id::<AccountId>)]
        account_id: AccountId,
        #[command(flatten)]
        link: LinkMutation,
    },
    ResetPassword {
        #[arg(value_parser = canonical_id::<AccountId>)]
        account_id: AccountId,
        #[command(flatten)]
        link: LinkMutation,
    },
    Disable {
        #[arg(value_parser = canonical_id::<AccountId>)]
        account_id: AccountId,
        #[arg(long)]
        yes: bool,
        #[command(flatten)]
        mutation: Mutation,
    },
    Delete {
        #[arg(value_parser = canonical_id::<AccountId>)]
        account_id: AccountId,
        #[arg(long)]
        yes: bool,
        #[command(flatten)]
        mutation: Mutation,
    },
    Enable {
        #[arg(value_parser = canonical_id::<AccountId>)]
        account_id: AccountId,
        #[command(flatten)]
        mutation: Mutation,
    },
}

#[derive(Subcommand)]
pub(crate) enum AuditCommand {
    List(AuditPage),
}

#[cfg(test)]
impl Cli {
    pub(crate) fn prepare(self) -> Result<crate::operations::Prepared, crate::error::CliError> {
        crate::operations::prepare(self.command)
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "Test fixtures fail fast without logging secrets."
)]
mod tests {
    use super::*;

    #[test]
    fn literal_id_shape_is_checked_before_domain_parser() {
        struct Probe;
        impl std::str::FromStr for Probe {
            type Err = std::convert::Infallible;
            fn from_str(_value: &str) -> Result<Self, Self::Err> {
                Ok(Self)
            }
        }
        for value in [
            "01890F3E-53B7-7D28-9B05-4F65092D5711",
            "01890f3e53b77d289b054f65092d5711",
            "01890f3e-53b7-4d28-9b05-4f65092d5711",
            "01890f3e-53b7-7d28-0b05-4f65092d5711",
            "{01890f3e-53b7-7d28-9b05-4f65092d5711}",
        ] {
            assert!(canonical_id::<Probe>(value).is_err());
            assert!(Cli::try_parse_from(["brews", "accounts", "get", value]).is_err());
        }
        assert!(canonical_id::<AccountId>("01890f3e-53b7-7d28-9b05-4f65092d5711").is_ok());
    }

    #[test]
    fn rejects_invalid_local_policy_inputs_before_preparing_mutation() {
        for limit in ["0", "101", "4294967295"] {
            assert!(
                Cli::try_parse_from(["brews", "accounts", "list", "--limit", limit])
                    .unwrap()
                    .prepare()
                    .is_err()
            );
            assert!(
                Cli::try_parse_from(["brews", "audit", "list", "--limit", limit])
                    .unwrap()
                    .prepare()
                    .is_err()
            );
        }
        assert!(
            Cli::try_parse_from([
                "brews",
                "accounts",
                "create",
                "--role",
                "host",
                "--username",
                "short",
                "--link-output",
                "new.link"
            ])
            .unwrap()
            .prepare()
            .is_err()
        );
    }

    #[test]
    fn destructive_commands_require_noninteractive_yes() {
        for operation in ["disable", "delete"] {
            let id = "01890f3e-53b7-7d28-9b05-4f65092d5711";
            assert!(
                Cli::try_parse_from(["brews", "accounts", operation, id])
                    .unwrap()
                    .prepare()
                    .is_err()
            );
            assert!(
                Cli::try_parse_from(["brews", "accounts", operation, id, "--yes"])
                    .unwrap()
                    .prepare()
                    .is_ok()
            );
        }
    }

    #[test]
    fn prepares_exact_operation_and_command_identity_once() {
        let id = "01890f3e-53b7-7d28-9b05-4f65092d5711";
        let prepared = Cli::try_parse_from([
            "brews",
            "accounts",
            "create",
            "--role",
            "admin",
            "--username",
            "adminusername",
            "--link-output",
            "new.link",
            "--command-id",
            id,
        ])
        .unwrap()
        .prepare()
        .unwrap();
        assert!(matches!(
            prepared.command,
            brews_contracts::management::ManagementCommand::CreateAccount {
                role: AccountRole::Admin,
                ..
            }
        ));
        assert_eq!(prepared.command_id.unwrap().to_string(), id);
        assert_eq!(prepared.link_output.unwrap(), PathBuf::from("new.link"));
        let auto = Cli::try_parse_from(["brews", "accounts", "enable", id])
            .unwrap()
            .prepare()
            .unwrap();
        assert!(auto.command_id.is_some());
        let read = Cli::try_parse_from(["brews", "accounts", "list"])
            .unwrap()
            .prepare()
            .unwrap();
        assert!(read.command_id.is_none());
    }

    #[test]
    fn accepts_operational_command_grammar() {
        assert!(Cli::try_parse_from(["brews", "accounts", "list"]).is_ok());
        let id = "01890f3e-53b7-7d28-9b05-4f65092d5711";
        let commands = [
            vec!["accounts", "list", "--after", id, "--limit", "100"],
            vec!["accounts", "get", id],
            vec![
                "accounts",
                "create",
                "--role",
                "host",
                "--username",
                "hostusername",
                "--link-output",
                "new.link",
                "--command-id",
                id,
            ],
            vec![
                "accounts",
                "reissue-enrollment",
                id,
                "--link-output",
                "new.link",
            ],
            vec![
                "accounts",
                "reset-password",
                id,
                "--link-output",
                "new.link",
            ],
            vec!["accounts", "disable", id, "--yes"],
            vec!["accounts", "delete", id, "--yes"],
            vec!["accounts", "enable", id],
            vec!["audit", "list", "--after", id, "--limit", "50"],
        ];
        for command in commands {
            assert!(
                Cli::try_parse_from(std::iter::once("brews").chain(command).chain(["--json"]))
                    .is_ok()
            );
        }
    }
}
