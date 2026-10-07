//! Closed F3/F4 grammar. Authentication remains Accounts-owned.
use super::ApiError;
use brews_domain::{games::GameState, ids::GameId};
use http::{HeaderMap, Method};
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

pub use brews_contracts::history::{HISTORY_DEFAULT_LIMIT, HISTORY_MAX_LIMIT};
const HISTORY_QUERY_MAX_BYTES: usize = 512;
const HISTORY_QUERY_MAX_FIELDS: usize = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryCursor {
    pub ended_at: i64,
    pub game_id: GameId,
}
impl HistoryCursor {
    pub fn encode(self, outcome: Option<GameState>) -> String {
        format!(
            "v1.{}.{}.{}",
            self.ended_at,
            self.game_id,
            filter_tag(outcome)
        )
    }
    fn parse(value: &str, outcome: Option<GameState>) -> Result<Self, ApiError> {
        let parts: Vec<_> = value.split('.').collect();
        let ["v1", ended, game, filter] = parts.as_slice() else {
            return Err(ApiError::InvalidInput);
        };
        if *filter != filter_tag(outcome)
            || ended.starts_with('0')
            || !ended.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(ApiError::InvalidInput);
        }
        let ended_at = ended.parse().map_err(|_| ApiError::InvalidInput)?;
        if !(1..=crate::limits::JS_SAFE_INTEGER_MAX).contains(&ended_at) {
            return Err(ApiError::InvalidInput);
        }
        Ok(Self {
            ended_at,
            game_id: game.parse().map_err(|_| ApiError::InvalidInput)?,
        })
    }
}
fn filter_tag(outcome: Option<GameState>) -> &'static str {
    match outcome {
        Some(GameState::Resolved) => "r",
        Some(GameState::Cancelled) => "c",
        _ => "a",
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HistoryQuery {
    pub after: Option<HistoryCursor>,
    pub limit: u32,
    pub outcome: Option<GameState>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HistoryCommand {
    List(HistoryQuery),
    Detail(GameId),
}
pub struct HistoryRequest {
    pub command: HistoryCommand,
    pub session_token: Option<String>,
}
impl Drop for HistoryRequest {
    fn drop(&mut self) {
        self.session_token.zeroize();
    }
}

fn list_query(query: Option<&str>) -> Result<HistoryQuery, ApiError> {
    let (mut cursor, mut limit, mut outcome) = (None, None, None);
    if let Some(query) = query {
        if query.is_empty() || query.len() > HISTORY_QUERY_MAX_BYTES || !query.is_ascii() {
            return Err(ApiError::InvalidInput);
        }
        for (index, field) in query.split('&').enumerate() {
            if index >= HISTORY_QUERY_MAX_FIELDS {
                return Err(ApiError::InvalidInput);
            }
            let (key, value) = field.split_once('=').ok_or(ApiError::InvalidInput)?;
            match key {
                "cursor" if cursor.is_none() => cursor = Some(value),
                "limit" if limit.is_none() => {
                    if value.is_empty()
                        || value.starts_with('0')
                        || !value.bytes().all(|b| b.is_ascii_digit())
                    {
                        return Err(ApiError::InvalidInput);
                    }
                    let n = value.parse().map_err(|_| ApiError::InvalidInput)?;
                    if !(1..=HISTORY_MAX_LIMIT).contains(&n) {
                        return Err(ApiError::InvalidInput);
                    }
                    limit = Some(n);
                }
                "outcome" if outcome.is_none() => {
                    outcome = Some(match value {
                        "resolved" => GameState::Resolved,
                        "cancelled" => GameState::Cancelled,
                        _ => return Err(ApiError::InvalidInput),
                    })
                }
                _ => return Err(ApiError::InvalidInput),
            }
        }
    }
    Ok(HistoryQuery {
        after: cursor
            .map(|s| HistoryCursor::parse(s, outcome))
            .transpose()?,
        limit: limit.unwrap_or(HISTORY_DEFAULT_LIMIT),
        outcome,
    })
}
/// Decode exact read-only paths and bounded canonical keyset/outcome grammar.
pub fn decode_history(
    method: &Method,
    path: &str,
    query: Option<&str>,
    headers: &HeaderMap,
    body: &[u8],
    origin: &str,
) -> Result<HistoryRequest, ApiError> {
    let detail = if path == "/api/history" {
        None
    } else {
        Some(
            path.strip_prefix("/api/history/")
                .ok_or(ApiError::NotFound)?
                .parse()
                .map_err(|_| ApiError::NotFound)?,
        )
    };
    if method != Method::GET {
        return Err(ApiError::MethodNotAllowed);
    }
    if headers.contains_key("authorization") {
        return Err(ApiError::Forbidden);
    }
    let origins = headers.get_all("origin");
    if origins.iter().count() != 0
        && (origins.iter().count() != 1
            || headers.get("origin").and_then(|v| v.to_str().ok()) != Some(origin))
    {
        return Err(ApiError::Forbidden);
    }
    if headers.contains_key("idempotency-key") {
        return Err(ApiError::InvalidInput);
    }
    if body.len() > super::BODY_LIMIT {
        return Err(ApiError::PayloadTooLarge);
    }
    if !body.is_empty() {
        return Err(ApiError::InvalidInput);
    }
    let command = match detail {
        Some(id) => {
            if query.is_some() {
                return Err(ApiError::InvalidInput);
            }
            HistoryCommand::Detail(id)
        }
        None => HistoryCommand::List(list_query(query)?),
    };
    Ok(HistoryRequest {
        command,
        session_token: super::decode::session_cookie(headers)?,
    })
}
