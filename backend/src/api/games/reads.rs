//! Closed game discovery and passive-read grammar. Cookie selectors never confer authority.
use super::writes::{self, GameViewSelector, PLAYER_COOKIE_NAME, SPECTATOR_COOKIE_NAME};
use crate::api::{
    ApiError, BODY_LIMIT,
    utility::{named_cookie, require_origin, session_cookie},
};
use brews_contracts::games::reads::ReadKind;
use brews_domain::{
    games::{GameCode, GameState},
    ids::{GameId, PlayerId},
};
use http::{HeaderMap, Method};
use serde::{Deserialize, Serialize};
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
        && (raw.is_empty() || raw.len() > writes::GAME_QUERY_MAX_BYTES || !raw.is_ascii())
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
    if path.len() > writes::GAME_PATH_MAX_BYTES {
        return Err(ApiError::NotFound);
    }
    if method != Method::GET {
        return Err(ApiError::MethodNotAllowed);
    }
    writes::validate_headers(headers)?;
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
                | "sec-fetch-storage-access"
                | "priority"
                | "dnt"
                | "sec-gpc"
                | "sec-ch-ua"
                | "sec-ch-ua-mobile"
                | "sec-ch-ua-platform"
                | "sec-ch-ua-arch"
                | "sec-ch-ua-bitness"
                | "sec-ch-ua-full-version"
                | "sec-ch-ua-full-version-list"
                | "sec-ch-ua-model"
                | "sec-ch-ua-platform-version"
                | "sec-ch-ua-wow64"
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
    if body.len() > BODY_LIMIT {
        return Err(ApiError::PayloadTooLarge);
    }
    if !body.is_empty() {
        return Err(ApiError::InvalidInput);
    }
    let account = Zeroizing::new(session_cookie(headers)?);
    let player = Zeroizing::new(named_cookie(headers, PLAYER_COOKIE_NAME)?);
    let spectator = Zeroizing::new(named_cookie(headers, SPECTATOR_COOKIE_NAME)?);
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
    } else if let Some(code) = path.strip_prefix("/api/games/codes/") {
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
    if path.len() > writes::GAME_PATH_MAX_BYTES {
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
        _ => {
            let command = action.strip_prefix("commands/")?.parse().ok()?;
            ReadKind::CommandLookup(command)
        }
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
    writes::validate_headers(headers)?;
    if !body.is_empty() || headers.contains_key("idempotency-key") {
        return Err(ApiError::InvalidInput);
    }
    if headers.contains_key("origin") {
        require_origin(headers, origin)?;
    }
    let mut player_id = None;
    let role = if kind.account_only() {
        if let Some(q) = query {
            if kind != ReadKind::Boards || q.len() > writes::GAME_QUERY_MAX_BYTES || !q.is_ascii() {
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
        match writes::read_query(query, headers, false)?
            .0
            .ok_or(ApiError::InvalidInput)?
        {
            GameViewSelector::Account => ReadRole::Account,
            GameViewSelector::Player => ReadRole::Player,
            GameViewSelector::Spectator => ReadRole::Spectator,
        }
    };
    let token = match role {
        ReadRole::Account => session_cookie(headers)?,
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
    writes::validate_headers(headers)?;
    if !kind.account_only()
        && query.is_some_and(|q| {
            q.split('&')
                .any(|s| matches!(s, "view=player" | "view=spectator"))
        })
    {
        return Ok(None);
    }
    Ok(session_cookie(headers)?.map(|token| (id, kind, token)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "Bounded transport fixtures fail fast.")]
mod tests {
    use super::*;
    const DISCOVERY_ORIGIN: &str = "https://localhost:8787";
    const DISCOVERY_ID: &str = "01900000-0000-7000-8000-000000000001";
    fn read(path: &str, query: Option<&str>, h: &HeaderMap) -> Result<DiscoveryRequest, ApiError> {
        decode_discovery(&Method::GET, path, query, h, b"", DISCOVERY_ORIGIN)
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
    fn code_lookup_uses_games_codes_namespace_only() {
        let headers = HeaderMap::new();
        assert_eq!(
            read("/api/games/codes/AB12CD34", None, &headers)
                .unwrap()
                .command,
            DiscoveryCommand::Code("AB12CD34".parse().unwrap())
        );
        for path in [
            "/api/game-codes/AB12CD34",
            "/api/games/codes",
            "/api/games/codes/",
            "/api/games/codes/AB12CD34/",
            "/api/games/codes/AB12CD34/extra",
        ] {
            assert_eq!(read(path, None, &headers).err(), Some(ApiError::NotFound));
        }
    }
    #[test]
    fn code_lookup_normalizes_once_and_strictly_decodes_transport() {
        for raw in ["ab12cd34", "AB12CD34", "%20ab12cd34%20", "%41B12CD34"] {
            assert_eq!(
                read(&format!("/api/games/codes/{raw}"), None, &HeaderMap::new())
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
                read(&format!("/api/games/codes/{raw}"), None, &HeaderMap::new()).err(),
                Some(ApiError::NotFound)
            );
        }
        assert_eq!(
            read(
                "/api/games/codes/AB12CD34",
                Some("view=entry"),
                &HeaderMap::new()
            )
            .err(),
            Some(ApiError::InvalidInput)
        );
    }
    #[test]
    fn availability_selects_exact_context_without_credential_fallback() {
        let path = format!("/api/games/{DISCOVERY_ID}/availability");
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
            "/api/games/codes/AB12CD34",
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
                    decode_discovery(
                        &method,
                        path,
                        None,
                        &HeaderMap::new(),
                        b"",
                        DISCOVERY_ORIGIN
                    )
                    .err(),
                    Some(ApiError::MethodNotAllowed)
                );
            }
            for body in [b"{}".as_slice(), b" ", b"[]"] {
                assert_eq!(
                    decode_discovery(
                        &Method::GET,
                        path,
                        None,
                        &HeaderMap::new(),
                        body,
                        DISCOVERY_ORIGIN
                    )
                    .err(),
                    Some(ApiError::InvalidInput)
                );
            }
        }
        let mut h = HeaderMap::new();
        h.insert("origin", DISCOVERY_ORIGIN.parse().unwrap());
        assert!(read("/api/games", None, &h).is_ok());
        h.append("origin", DISCOVERY_ORIGIN.parse().unwrap());
        assert_eq!(
            read("/api/games", None, &h).err(),
            Some(ApiError::Forbidden)
        );
        h.clear();
        h.insert(
            "accept",
            "x".repeat(writes::GAME_HEADERS_MAX_BYTES).parse().unwrap(),
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

    const ID: &str = "01890f3e-53b7-7d28-9b05-4f65092d5701";
    const ORIGIN: &str = "https://app.test";
    #[test]
    fn discovery_accepts_browser_privacy_metadata_without_accepting_application_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("cookie", "__Host-brews_session=opaque".parse().unwrap());
        for name in [
            "dnt",
            "sec-gpc",
            "sec-fetch-storage-access",
            "sec-ch-ua-model",
            "sec-ch-ua-platform-version",
        ] {
            headers.insert(name, "1".parse().unwrap());
        }
        for path in ["/api/games/current", "/api/games"] {
            let selected =
                decode_discovery(&Method::GET, path, None, &headers, b"", ORIGIN).unwrap();
            assert_eq!(selected.session_token.as_deref(), Some("opaque"));
            headers.insert("x-view", "account".parse().unwrap());
            assert_eq!(
                decode_discovery(&Method::GET, path, None, &headers, b"", ORIGIN).err(),
                Some(ApiError::InvalidInput)
            );
            headers.remove("x-view");
            headers.insert("idempotency-key", ID.parse().unwrap());
            assert_eq!(
                decode_discovery(&Method::GET, path, None, &headers, b"", ORIGIN).err(),
                Some(ApiError::InvalidInput)
            );
            headers.remove("idempotency-key");
        }
    }
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
    fn command_lookup_route_uses_closed_existing_read_grammar() {
        let command = "01890f3e-53b7-7d28-9b05-4f65092d5702";
        let action = format!("/commands/{command}");
        let cookies = "__Host-brews_session=account; __Secure-brews-player=player; __Secure-brews-spectator=spectator";
        assert_eq!(
            decode(&action, None, cookies).err(),
            Some(ApiError::InvalidInput)
        );
        for (view, token) in [
            ("account", "account"),
            ("player", "player"),
            ("spectator", "spectator"),
        ] {
            assert_eq!(
                decode(&action, Some(&format!("view={view}")), cookies)
                    .unwrap()
                    .token
                    .as_deref(),
                Some(token)
            );
        }
        for query in [
            "view=account&view=player",
            "view=account&command_id=x",
            "view=%70layer",
            "token=x",
            "",
        ] {
            assert_eq!(
                decode(&action, Some(query), cookies).err(),
                Some(ApiError::InvalidInput)
            );
        }
        for action in [
            "/commands/01890F3E-53b7-7d28-9b05-4f65092d5702",
            "/commands/not-an-id",
            "/commands/01890f3e-53b7-4d28-9b05-4f65092d5702",
        ] {
            assert!(game_read_route(&format!("/api/games/{ID}{action}")).is_none());
        }
        let path = format!("/api/games/{ID}{action}");
        let mut h = HeaderMap::new();
        h.insert("cookie", "__Host-brews_session=account".parse().unwrap());
        for body in [b"{}".as_slice(), b" ".as_slice()] {
            assert_eq!(
                decode_game_read(&Method::GET, &path, None, &h, body, ORIGIN).err(),
                Some(ApiError::InvalidInput)
            );
        }
        h.insert("idempotency-key", command.parse().unwrap());
        assert_eq!(
            decode_game_read(&Method::GET, &path, None, &h, b"", ORIGIN).err(),
            Some(ApiError::InvalidInput)
        );
        h.remove("idempotency-key");
        h.insert("origin", "https://wrong.test".parse().unwrap());
        assert_eq!(
            decode_game_read(&Method::GET, &path, None, &h, b"", ORIGIN).err(),
            Some(ApiError::Forbidden)
        );
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
