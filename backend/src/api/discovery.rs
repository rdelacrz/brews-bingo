//! Closed, allocation-conscious Slice 1 GET grammar. Cookies never imply fallback.
use super::{
    ApiError, games,
    utility::{named_cookie, require_origin, session_cookie},
};
use brews_domain::{
    games::{GameCode, GameState},
    ids::GameId,
};
use http::{HeaderMap, Method};
use zeroize::{Zeroize, Zeroizing};

const DISCOVERY_QUERY_MAX_FIELDS: usize = 2;
const CODE_TRANSPORT_MAX_BYTES: usize = 64;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AvailabilityView {
    Entry,
    Account,
    Player,
    Spectator,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiscoveryCommand {
    List {
        state: Option<GameState>,
    },
    Current,
    Code(GameCode),
    Availability {
        game_id: GameId,
        view: AvailabilityView,
        game_code: Option<GameCode>,
    },
}
pub struct DiscoveryRequest {
    pub command: DiscoveryCommand,
    pub session_token: Option<String>,
}
impl Drop for DiscoveryRequest {
    fn drop(&mut self) {
        self.session_token.zeroize();
    }
}

fn transport_code(raw: &str) -> Result<GameCode, ApiError> {
    if raw.len() > CODE_TRANSPORT_MAX_BYTES || !raw.is_ascii() {
        return Err(ApiError::NotFound);
    }
    let mut bytes = [0; CODE_TRANSPORT_MAX_BYTES];
    let (mut at, mut len) = (0, 0);
    let input = raw.as_bytes();
    while at < input.len() {
        bytes[len] = if input[at] == b'%' {
            let hex = input.get(at + 1..at + 3).ok_or(ApiError::NotFound)?;
            let hex = std::str::from_utf8(hex).map_err(|_| ApiError::NotFound)?;
            let byte = u8::from_str_radix(hex, 16).map_err(|_| ApiError::NotFound)?;
            at += 3;
            byte
        } else {
            let byte = input[at];
            at += 1;
            byte
        };
        len += 1;
    }
    GameCode::from_lookup(std::str::from_utf8(&bytes[..len]).map_err(|_| ApiError::NotFound)?)
        .map_err(|_| ApiError::NotFound)
}
fn query_fields(query: Option<&str>) -> Result<impl Iterator<Item = (&str, &str)>, ApiError> {
    let raw = query.unwrap_or("");
    if query.is_some()
        && (raw.is_empty() || raw.len() > games::GAME_QUERY_MAX_BYTES || !raw.is_ascii())
    {
        return Err(ApiError::InvalidInput);
    }
    // Validate before returning borrowed fields, including the complete field bound.
    for (index, field) in raw.split('&').filter(|_| !raw.is_empty()).enumerate() {
        if index >= DISCOVERY_QUERY_MAX_FIELDS || !field.contains('=') {
            return Err(ApiError::InvalidInput);
        }
    }
    Ok(raw
        .split('&')
        .filter(|_| !raw.is_empty())
        .filter_map(|s| s.split_once('=')))
}
fn no_query(query: Option<&str>) -> Result<(), ApiError> {
    if query.is_some() {
        Err(ApiError::InvalidInput)
    } else {
        Ok(())
    }
}
pub fn decode_discovery(
    method: &Method,
    path: &str,
    query: Option<&str>,
    headers: &HeaderMap,
    body: &[u8],
    origin: &str,
) -> Result<DiscoveryRequest, ApiError> {
    if path.len() > games::GAME_PATH_MAX_BYTES {
        return Err(ApiError::NotFound);
    }
    if method != Method::GET {
        return Err(ApiError::MethodNotAllowed);
    }
    games::validate_headers(headers)?;
    if headers.contains_key("origin") {
        require_origin(headers, origin)?;
    }
    if headers.contains_key("idempotency-key") {
        return Err(ApiError::InvalidInput);
    }
    // Known browser/provider metadata is transport only; context selection is query-only.
    for name in headers.keys() {
        if !matches!(
            name.as_str(),
            "origin"
                | "cookie"
                | "accept"
                | "accept-encoding"
                | "accept-language"
                | "user-agent"
                | "host"
                | "connection"
                | "content-length"
                | "content-type"
                | "cache-control"
                | "pragma"
                | "referer"
                | "sec-fetch-dest"
                | "sec-fetch-mode"
                | "sec-fetch-site"
                | "sec-fetch-user"
                | "priority"
                | "sec-ch-ua"
                | "sec-ch-ua-mobile"
                | "sec-ch-ua-platform"
                | "cf-connecting-ip"
                | "cf-ray"
                | "cf-visitor"
                | "cf-ipcountry"
                | "cf-ew-via"
                | "traceparent"
                | "tracestate"
                | "x-forwarded-proto"
                | "x-forwarded-for"
                | "cdn-loop"
        ) {
            return Err(ApiError::InvalidInput);
        }
    }
    if body.len() > super::BODY_LIMIT {
        return Err(ApiError::PayloadTooLarge);
    }
    if !body.is_empty() {
        return Err(ApiError::InvalidInput);
    }
    let account = Zeroizing::new(session_cookie(headers)?);
    let player = Zeroizing::new(named_cookie(headers, super::PLAYER_COOKIE_NAME)?);
    let spectator = Zeroizing::new(named_cookie(headers, super::SPECTATOR_COOKIE_NAME)?);
    let (command, token) = if path == "/api/games" {
        let mut state = None;
        for (key, value) in query_fields(query)? {
            if key != "state" || state.is_some() {
                return Err(ApiError::InvalidInput);
            }
            state = Some(match value {
                "new" => GameState::New,
                "awaiting_players" => GameState::AwaitingPlayers,
                "in_progress" => GameState::InProgress,
                _ => return Err(ApiError::InvalidInput),
            });
        }
        (DiscoveryCommand::List { state }, account.as_ref().cloned())
    } else if path == "/api/games/current" {
        no_query(query)?;
        (DiscoveryCommand::Current, account.as_ref().cloned())
    } else if let Some(code) = path.strip_prefix("/api/game-codes/") {
        no_query(query)?;
        (DiscoveryCommand::Code(transport_code(code)?), None)
    } else {
        let id = path
            .strip_prefix("/api/games/")
            .and_then(|s| s.strip_suffix("/availability"))
            .ok_or(ApiError::NotFound)?;
        let game_id = id.parse().map_err(|_| ApiError::NotFound)?;
        let (mut view, mut game_code) = (None, None);
        for (key, value) in query_fields(query)? {
            match key {
                "view" if view.is_none() => {
                    view = Some(match value {
                        "entry" => AvailabilityView::Entry,
                        "account" => AvailabilityView::Account,
                        "player" => AvailabilityView::Player,
                        "spectator" => AvailabilityView::Spectator,
                        _ => return Err(ApiError::InvalidInput),
                    })
                }
                "game_code" if game_code.is_none() => game_code = Some(transport_code(value)?),
                _ => return Err(ApiError::InvalidInput),
            }
        }
        let view = match view {
            Some(view) => view,
            None => match (
                account.is_some(),
                player.is_some(),
                spectator.is_some(),
                game_code.is_some(),
            ) {
                (true, false, false, false) => AvailabilityView::Account,
                (false, true, false, false) => AvailabilityView::Player,
                (false, false, false, true) => AvailabilityView::Entry,
                (false, false, true, false) => AvailabilityView::Spectator,
                _ => return Err(ApiError::InvalidInput),
            },
        };
        if (view == AvailabilityView::Entry) != game_code.is_some() {
            return Err(ApiError::InvalidInput);
        }
        let token = match view {
            AvailabilityView::Entry => None,
            AvailabilityView::Account => account.as_ref().cloned(),
            AvailabilityView::Player => player.as_ref().cloned(),
            AvailabilityView::Spectator => spectator.as_ref().cloned(),
        };
        (
            DiscoveryCommand::Availability {
                game_id,
                view,
                game_code,
            },
            token,
        )
    };
    Ok(DiscoveryRequest {
        command,
        session_token: token,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "Bounded transport fixtures fail fast.")]
mod tests {
    use super::*;
    const ORIGIN: &str = "https://localhost:8787";
    const ID: &str = "01900000-0000-7000-8000-000000000001";
    fn read(path: &str, query: Option<&str>, h: &HeaderMap) -> Result<DiscoveryRequest, ApiError> {
        decode_discovery(&Method::GET, path, query, h, b"", ORIGIN)
    }
    #[test]
    fn discovery_singleton_filters_and_exact_routes_are_closed() {
        for value in ["new", "awaiting_players", "in_progress"] {
            assert!(
                read(
                    "/api/games",
                    Some(&format!("state={value}")),
                    &HeaderMap::new()
                )
                .is_ok()
            );
        }
        for query in [
            "",
            "state=resolved",
            "state=cancelled",
            "state=NEW",
            "state=new&state=new",
            "cursor=x",
            "limit=1",
            "state=new&limit=1",
            "unknown=1",
            "state=%6eew",
            "state=new&",
            "state",
            "state=new=foo",
        ] {
            assert_eq!(
                read("/api/games", Some(query), &HeaderMap::new()).err(),
                Some(ApiError::InvalidInput)
            );
        }
        assert!(read("/api/games/current", None, &HeaderMap::new()).is_ok());
        assert_eq!(
            read("/api/games/current", Some("state=new"), &HeaderMap::new()).err(),
            Some(ApiError::InvalidInput)
        );
        for path in [
            "/api/games/current/",
            "/api/games/01900000-0000-4000-8000-000000000001/availability",
            "/api/games/019A0000-0000-7000-8000-000000000001/availability",
            "/api/games/%301900000-0000-7000-8000-000000000001/availability",
        ] {
            assert_eq!(
                read(path, None, &HeaderMap::new()).err(),
                Some(ApiError::NotFound)
            );
        }
    }
    #[test]
    fn code_lookup_normalizes_once_and_strictly_decodes_transport() {
        for raw in ["ab12cd34", "AB12CD34", "%20ab12cd34%20", "%41B12CD34"] {
            assert_eq!(
                read(&format!("/api/game-codes/{raw}"), None, &HeaderMap::new())
                    .unwrap()
                    .command,
                DiscoveryCommand::Code("AB12CD34".parse().unwrap())
            );
        }
        for raw in [
            "AB12CD3",
            "AB12CD345",
            "AB12/CD34",
            "%",
            "%GG",
            "%2",
            "%2541B12CD34",
            "AB12CD3%00",
            "AB12CD3+",
            "%C3%A9B12CD34",
            "%0aAB12CD34",
            "%2fAB12CD34",
        ] {
            assert_eq!(
                read(&format!("/api/game-codes/{raw}"), None, &HeaderMap::new()).err(),
                Some(ApiError::NotFound)
            );
        }
        assert_eq!(
            read(
                "/api/game-codes/AB12CD34",
                Some("view=entry"),
                &HeaderMap::new()
            )
            .err(),
            Some(ApiError::InvalidInput)
        );
    }
    #[test]
    fn availability_selects_exact_context_without_credential_fallback() {
        let path = format!("/api/games/{ID}/availability");
        let mut h = HeaderMap::new();
        h.insert(
            "cookie",
            "__Host-brews_session=account; __Secure-brews-player=player"
                .parse()
                .unwrap(),
        );
        for query in [
            None,
            Some("game_code=AB12CD34"),
            Some("view=entry"),
            Some("view=account&game_code=AB12CD34"),
            Some("view=player&game_code=AB12CD34"),
            Some("view=player&view=player"),
            Some("view=other"),
            Some("view=entry&game_code=AB12CD34&limit=1"),
        ] {
            assert_eq!(read(&path, query, &h).err(), Some(ApiError::InvalidInput));
        }
        let account = read(&path, Some("view=account"), &h).unwrap();
        assert_eq!(account.session_token.as_deref(), Some("account"));
        let player = read(&path, Some("view=player"), &h).unwrap();
        assert_eq!(player.session_token.as_deref(), Some("player"));
        let entry = read(&path, Some("view=entry&game_code=ab12cd34"), &h).unwrap();
        assert!(entry.session_token.is_none());
        assert!(
            read(&path, Some("view=account"), &HeaderMap::new())
                .unwrap()
                .session_token
                .is_none()
        );
        h.insert(
            "cookie",
            "__Secure-brews-player=a; __Secure-brews-player=b"
                .parse()
                .unwrap(),
        );
        assert_eq!(
            read(&path, Some("view=player"), &h).err(),
            Some(ApiError::InvalidInput)
        );
    }
    #[test]
    fn discovery_rejects_body_retry_bearer_unknown_and_unbounded_headers() {
        for path in [
            "/api/games",
            "/api/games/current",
            "/api/game-codes/AB12CD34",
        ] {
            for (name, value, error) in [
                ("authorization", "Bearer", ApiError::Forbidden),
                ("idempotency-key", "", ApiError::InvalidInput),
                ("x-view", "account", ApiError::InvalidInput),
                ("origin", "null", ApiError::Forbidden),
            ] {
                let mut h = HeaderMap::new();
                h.insert(name, value.parse().unwrap());
                assert_eq!(read(path, None, &h).err(), Some(error));
            }
            for method in [Method::POST, Method::PUT, Method::HEAD, Method::DELETE] {
                assert_eq!(
                    decode_discovery(&method, path, None, &HeaderMap::new(), b"", ORIGIN).err(),
                    Some(ApiError::MethodNotAllowed)
                );
            }
            for body in [b"{}".as_slice(), b" ", b"[]"] {
                assert_eq!(
                    decode_discovery(&Method::GET, path, None, &HeaderMap::new(), body, ORIGIN)
                        .err(),
                    Some(ApiError::InvalidInput)
                );
            }
        }
        let mut h = HeaderMap::new();
        h.insert("origin", ORIGIN.parse().unwrap());
        assert!(read("/api/games", None, &h).is_ok());
        h.append("origin", ORIGIN.parse().unwrap());
        assert_eq!(
            read("/api/games", None, &h).err(),
            Some(ApiError::Forbidden)
        );
        h.clear();
        h.insert(
            "accept",
            "x".repeat(games::GAME_HEADERS_MAX_BYTES).parse().unwrap(),
        );
        assert_eq!(
            read("/api/games", None, &h).err(),
            Some(ApiError::InvalidInput)
        );
        h.clear();
        for n in 0..65 {
            h.append("accept", n.to_string().parse().unwrap());
        }
        assert_eq!(
            read("/api/games", None, &h).err(),
            Some(ApiError::InvalidInput)
        );
    }
}
