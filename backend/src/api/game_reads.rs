//! Closed passive-read grammar. Cookie selectors never confer authority.
use super::{
    ApiError,
    decode::named_cookie,
    games::{self, GameViewSelector, PLAYER_COOKIE_NAME, SPECTATOR_COOKIE_NAME},
};
use brews_contracts::games::reads::ReadKind;
use brews_domain::ids::{GameId, PlayerId};
use http::{HeaderMap, Method};
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadRole {
    Account,
    Player,
    Spectator,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadSelection {
    pub game_id: GameId,
    pub kind: ReadKind,
    pub role: ReadRole,
    pub player_id: Option<PlayerId>,
}
/// Selected credential only; no Debug or credential echo.
pub struct GameReadRequest {
    pub selection: ReadSelection,
    pub token: Option<String>,
}
impl Drop for GameReadRequest {
    fn drop(&mut self) {
        self.token.zeroize();
    }
}
pub fn game_read_route(path: &str) -> Option<(GameId, ReadKind)> {
    if path.len() > games::GAME_PATH_MAX_BYTES {
        return None;
    }
    let rest = path.strip_prefix("/api/games/")?;
    let (id, action) = rest.split_once('/').unwrap_or((rest, ""));
    let id = id.parse().ok()?;
    let kind = match action {
        "" => ReadKind::Game,
        "membership" => ReadKind::Membership,
        "players" => ReadKind::Players,
        "calls" => ReadKind::Calls,
        "remaining-values" => ReadKind::RemainingValues,
        "membership/board" => ReadKind::OwnBoard,
        "boards" => ReadKind::Boards,
        "qualification" => ReadKind::Qualification,
        "result" => ReadKind::Result,
        _ => return None,
    };
    Some((id, kind))
}
pub fn decode_game_read(
    method: &Method,
    path: &str,
    query: Option<&str>,
    headers: &HeaderMap,
    body: &[u8],
    origin: &str,
) -> Result<GameReadRequest, ApiError> {
    let (game_id, kind) = game_read_route(path).ok_or(ApiError::NotFound)?;
    if method != Method::GET {
        return Err(ApiError::MethodNotAllowed);
    }
    games::validate_headers(headers)?;
    if !body.is_empty() || headers.contains_key("idempotency-key") {
        return Err(ApiError::InvalidInput);
    }
    if headers.contains_key("origin")
        && (headers.get_all("origin").iter().count() != 1
            || headers.get("origin").and_then(|v| v.to_str().ok()) != Some(origin))
    {
        return Err(ApiError::Forbidden);
    }
    let mut player_id = None;
    let role = if kind.account_only() {
        if let Some(q) = query {
            if kind != ReadKind::Boards || q.len() > games::GAME_QUERY_MAX_BYTES || !q.is_ascii() {
                return Err(ApiError::InvalidInput);
            }
            let raw = q.strip_prefix("player_id=").ok_or(ApiError::InvalidInput)?;
            player_id = Some(raw.parse().map_err(|_| ApiError::InvalidInput)?);
        }
        ReadRole::Account
    } else if kind == ReadKind::OwnBoard {
        if query.is_some_and(|q| q != "view=player") {
            return Err(ApiError::InvalidInput);
        }
        ReadRole::Player
    } else if kind == ReadKind::Membership {
        match query {
            Some("view=player") => ReadRole::Player,
            Some("view=spectator") => ReadRole::Spectator,
            Some(_) => return Err(ApiError::InvalidInput),
            None => {
                let raw = headers
                    .get("cookie")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("");
                let has = |name| {
                    raw.split(';')
                        .any(|v| v.trim().split_once('=').is_some_and(|(k, _)| k == name))
                };
                match (has(PLAYER_COOKIE_NAME), has(SPECTATOR_COOKIE_NAME)) {
                    (true, true) => return Err(ApiError::InvalidInput),
                    (false, true) => ReadRole::Spectator,
                    _ => ReadRole::Player,
                }
            }
        }
    } else {
        match games::read_query(query, headers, false)?
            .0
            .ok_or(ApiError::InvalidInput)?
        {
            GameViewSelector::Account => ReadRole::Account,
            GameViewSelector::Player => ReadRole::Player,
            GameViewSelector::Spectator => ReadRole::Spectator,
        }
    };
    let token = match role {
        ReadRole::Account => super::decode::session_cookie(headers)?,
        ReadRole::Player => named_cookie(headers, PLAYER_COOKIE_NAME)?,
        ReadRole::Spectator => named_cookie(headers, SPECTATOR_COOKIE_NAME)?,
    };
    Ok(GameReadRequest {
        selection: ReadSelection {
            game_id,
            kind,
            role,
            player_id,
        },
        token,
    })
}
#[cfg(target_arch = "wasm32")]
pub(crate) fn read_rejection_context(
    method: &Method,
    path: &str,
    query: Option<&str>,
    headers: &HeaderMap,
) -> Result<Option<(GameId, ReadKind, String)>, ApiError> {
    if *method != Method::GET {
        return Ok(None);
    }
    let Some((id, kind)) = game_read_route(path) else {
        return Ok(None);
    };
    if matches!(kind, ReadKind::Membership | ReadKind::OwnBoard) {
        return Ok(None);
    }
    games::validate_headers(headers)?;
    if !kind.account_only()
        && query.is_some_and(|q| {
            q.split('&')
                .any(|s| matches!(s, "view=player" | "view=spectator"))
        })
    {
        return Ok(None);
    }
    Ok(super::decode::session_cookie(headers)?.map(|token| (id, kind, token)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "Bounded transport fixtures fail fast.")]
mod tests {
    use super::*;
    const ID: &str = "01890f3e-53b7-7d28-9b05-4f65092d5701";
    const ORIGIN: &str = "https://app.test";
    fn decode(action: &str, q: Option<&str>, cookie: &str) -> Result<GameReadRequest, ApiError> {
        let mut h = HeaderMap::new();
        h.insert("cookie", cookie.parse().unwrap());
        decode_game_read(
            &Method::GET,
            &format!("/api/games/{ID}{action}"),
            q,
            &h,
            b"",
            ORIGIN,
        )
    }
    #[test]
    fn dedicated_reads_closed_selectors_and_participant_isolation() {
        for action in [
            "",
            "/membership",
            "/players",
            "/calls",
            "/remaining-values",
            "/membership/board",
            "/boards",
            "/qualification",
            "/result",
        ] {
            let path = format!("/api/games/{ID}{action}");
            let headers = HeaderMap::new();
            for body in [b"{}".as_slice(), b"{\"role\":\"admin\"}".as_slice()] {
                assert_eq!(
                    decode_game_read(&Method::GET, &path, None, &headers, body, ORIGIN).err(),
                    Some(ApiError::InvalidInput)
                );
            }
            assert_eq!(
                decode_game_read(&Method::PUT, &path, None, &headers, b"", ORIGIN).err(),
                Some(ApiError::MethodNotAllowed)
            );
        }
        let cookies = "__Host-brews_session=account; __Secure-brews-player=player; __Secure-brews-spectator=spectator";
        for path in ["", "/calls", "/qualification", "/result"] {
            assert_eq!(
                decode(path, None, cookies).err(),
                Some(ApiError::InvalidInput)
            );
            assert_eq!(
                decode(path, Some("view=player"), cookies)
                    .unwrap()
                    .token
                    .as_deref(),
                Some("player")
            );
        }
        for path in ["/players", "/remaining-values", "/boards"] {
            assert_eq!(
                decode(path, None, cookies).unwrap().token.as_deref(),
                Some("account")
            );
            assert_eq!(
                decode(path, Some("view=player"), cookies).err(),
                Some(ApiError::InvalidInput)
            );
        }
        assert_eq!(
            decode("/membership", None, cookies).err(),
            Some(ApiError::InvalidInput)
        );
        assert_eq!(
            decode("/membership", Some("view=account"), cookies).err(),
            Some(ApiError::InvalidInput)
        );
        assert_eq!(
            decode("/membership/board", None, cookies)
                .unwrap()
                .token
                .as_deref(),
            Some("player")
        );
        for q in [
            "view=player&view=player",
            "role=admin",
            "known_revision=1",
            "view=%70layer",
            "player_id=bad",
            "",
        ] {
            assert_eq!(
                decode("/calls", Some(q), cookies).err(),
                Some(ApiError::InvalidInput)
            );
        }
        for q in [
            "player_id=bad",
            "player_id=01890F3E-53B7-7D28-9B05-4F65092D5701",
            "player_id=01890f3e-53b7-7d28-9b05-4f65092d5701&player_id=01890f3e-53b7-7d28-9b05-4f65092d5701",
        ] {
            assert_eq!(
                decode("/boards", Some(q), cookies).err(),
                Some(ApiError::InvalidInput)
            );
        }
    }
}
