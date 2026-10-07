//! Empty game commands share one original-byte decoder across public/private ingress.
use crate::api::{ApiError, BODY_LIMIT};

pub(crate) fn decode_empty_game_body(body: &[u8]) -> Result<(), ApiError> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct EmptyGameCommand {}
    if body.len() > BODY_LIMIT
        || body
            .iter()
            .find(|byte| !matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
            != Some(&b'{')
    {
        return Err(ApiError::InvalidInput);
    }
    serde_json::from_slice::<EmptyGameCommand>(body).map_err(|_| ApiError::InvalidInput)?;
    Ok(())
}
