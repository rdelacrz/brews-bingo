//! Bounded HTTP transport parsing.
mod decode;
pub mod error;
pub use decode::{BODY_LIMIT, Decoded, Operation, Payload, decode};
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
    ADMISSION_COOKIE_NAME, GameOperation, GamePayload, GameViewSelector, GamesRequest,
    PLAYER_COOKIE_NAME, RecoverPlayerInput, RecoveryAnswerInput, SPECTATOR_ADMISSION_COOKIE_NAME,
    SPECTATOR_COOKIE_NAME, decode_games, game_cookie_path,
};
#[cfg(target_arch = "wasm32")]
pub(crate) use games::{audit_read_rejection_context, operator_rejection_context};

mod discovery;
pub use discovery::{AvailabilityView, DiscoveryCommand, DiscoveryRequest, decode_discovery};

mod game_reads;
pub use game_reads::{GameReadRequest, ReadRole, ReadSelection, decode_game_read, game_read_route};

#[cfg(target_arch = "wasm32")]
pub(crate) use game_reads::read_rejection_context;
