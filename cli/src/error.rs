#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum CliError {
    #[error("invalid command arguments")]
    InvalidInput,
    #[error("invalid developer-client configuration or trusted CA file")]
    Configuration,
    #[error("server response violates the management protocol; outcome may be uncertain")]
    Protocol,
    #[error(
        "cannot write safe command metadata; inspect the private link file before any explicit retry"
    )]
    Output,
    #[error("management response exceeds 128 KiB; outcome may be uncertain")]
    ResponseTooLarge,
    #[error("management endpoint rejected request (HTTP {0}); no automatic retry was made")]
    HttpStatus(u16),
    #[error(
        "network operation failed; its outcome may be uncertain; reuse the printed command ID explicitly, never retry automatically"
    )]
    Transport,
    #[error("disable and delete require explicit --yes confirmation")]
    Confirmation,
    #[error(
        "cannot reserve or write the private link file; existing targets are never overwritten"
    )]
    LinkFile,
}
impl CliError {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::InvalidInput => "invalid_input",
            Self::Configuration => "configuration",
            Self::Protocol => "protocol",
            Self::Output => "output",
            Self::ResponseTooLarge => "response_too_large",
            Self::HttpStatus(_) => "http_rejected",
            Self::Transport => "transport",
            Self::Confirmation => "confirmation",
            Self::LinkFile => "link_file",
        }
    }
}
