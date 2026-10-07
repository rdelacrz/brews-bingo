//! Bounded HTTP transport parsing.
mod decode;
pub mod error;
pub use decode::{BODY_LIMIT, Decoded, Operation, Payload, decode};
pub use error::ApiError;
mod developer;
pub use developer::{CliRequest, authenticate_cli, decode_cli};
mod users;
pub use users::{UsersRequest, decode_users};
mod games;
#[cfg(any(test, target_arch = "wasm32"))]
pub(crate) use games::decode_empty_game_body;
pub use games::{
    ADMISSION_COOKIE_NAME, GameOperation, GamePayload, GameViewSelector, GamesRequest,
    PLAYER_COOKIE_NAME, decode_games, game_cookie_path,
};
