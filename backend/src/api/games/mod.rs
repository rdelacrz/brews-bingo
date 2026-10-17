//! Game transport decoders; authoritative permissions remain owner-local.
mod reads;
mod writes;

#[cfg(target_arch = "wasm32")]
pub(crate) use reads::read_rejection_context;
pub use reads::{GameReadRequest, ReadRole, ReadSelection, decode_game_read, game_read_route};

#[cfg(test)]
pub(super) use writes::GAME_HEADERS_MAX_BYTES;
#[cfg(any(test, target_arch = "wasm32"))]
pub(crate) use writes::decode_empty_game_body;
#[allow(
    unused_imports,
    reason = "Private Worker wire is also compiled by native boundary tests."
)]
pub(crate) use writes::decode_recovery_input;
pub use writes::{
    ADMISSION_COOKIE_NAME, GameOperation, GamePayload, GameViewSelector, GamesRequest,
    PLAYER_COOKIE_NAME, RecoverPlayerInput, RecoveryAnswerInput, SPECTATOR_ADMISSION_COOKIE_NAME,
    SPECTATOR_COOKIE_NAME, decode_games, game_cookie_path,
};
pub(super) use writes::{GAME_PATH_MAX_BYTES, GAME_QUERY_MAX_BYTES, validate_headers};
#[cfg(target_arch = "wasm32")]
pub(crate) use writes::{audit_read_rejection_context, operator_rejection_context};
