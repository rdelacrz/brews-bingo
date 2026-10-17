//! Bounded HTTP transport parsing.
mod authentication;
pub use authentication::{
    AuthenticationOperation, AuthenticationPayload, AuthenticationRequest, BODY_LIMIT,
    decode_authentication,
};
pub mod error;
pub use error::ApiError;
mod developer;
pub use developer::{CliRequest, authenticate_cli, decode_cli};
mod history;
pub use history::{
    HISTORY_DEFAULT_LIMIT, HISTORY_MAX_LIMIT, HistoryCommand, HistoryCursor, HistoryQuery,
    HistoryRequest, decode_history,
};
mod users;
pub use users::{UsersRequest, decode_users};
mod games;
#[cfg(any(test, target_arch = "wasm32"))]
pub(crate) use games::decode_empty_game_body;
#[allow(
    unused_imports,
    reason = "Private Worker wire is also compiled by native boundary tests."
)]
pub(crate) use games::decode_recovery_input;
pub use games::{
    ADMISSION_COOKIE_NAME, GameOperation, GamePayload, GameReadRequest, GameViewSelector,
    GamesRequest, PLAYER_COOKIE_NAME, ReadRole, ReadSelection, RecoverPlayerInput,
    RecoveryAnswerInput, SPECTATOR_ADMISSION_COOKIE_NAME, SPECTATOR_COOKIE_NAME, decode_game_read,
    decode_games, game_cookie_path, game_read_route,
};
#[cfg(target_arch = "wasm32")]
pub(crate) use games::{
    audit_read_rejection_context, operator_rejection_context, read_rejection_context,
};

mod discovery;
pub use discovery::{AvailabilityView, DiscoveryCommand, DiscoveryRequest, decode_discovery};
