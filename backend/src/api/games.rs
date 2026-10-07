//! Bounded public game transport; grammar is not authentication or authorization.
use super::{ApiError, BODY_LIMIT};
use brews_contracts::games::{
    AdmissionContextInput, CallManualInput, CancelGameInput, CreateGame, GAME_QUERY_KNOWN_REVISION,
    GAME_QUERY_VIEW, GAME_VIEW_ACCOUNT, GAME_VIEW_PLAYER, JoinPlayer, RevisionCommand, WinnerInput,
};
use brews_domain::ids::{CommandId, GameId};
use http::{HeaderMap, Method};
use zeroize::Zeroize as _;

pub const PLAYER_COOKIE_NAME: &str = "__Secure-brews-player";
pub const ADMISSION_COOKIE_NAME: &str = "__Secure-brews-admission";
pub const GAME_PATH_MAX_BYTES: usize = 128;
pub const GAME_QUERY_MAX_BYTES: usize = 512;
pub const GAME_HEADERS_MAX_BYTES: usize = 16_384;
pub const GAME_HEADERS_MAX_FIELDS: usize = 64;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GameOperation {
    Create,
    Lobby,
    Start,
    CallRandom,
    CallManual,
    Winner,
    Cancel,
    Exit,
    AdmissionContext,
    JoinPlayer,
    Stream,
    Sync,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GameViewSelector {
    Account,
    Player,
}
pub enum GamePayload {
    Create(CreateGame),
    Revision(RevisionCommand),
    CallManual(CallManualInput),
    Winner(WinnerInput),
    Cancel(CancelGameInput),
    AdmissionContext(AdmissionContextInput),
    JoinPlayer(JoinPlayer),
    Empty,
}
/// Secret-bearing request deliberately has no Debug implementation.
///
/// ```compile_fail,E0277
/// fn requires_debug<T: std::fmt::Debug>() {}
/// requires_debug::<brews_backend::api::GamesRequest>();
/// ```
/// Missing/untrusted selected cookies must be rejected by the authoritative owner.
pub struct GamesRequest {
    pub operation: GameOperation,
    pub game_id: Option<GameId>,
    pub payload: GamePayload,
    pub command_id: Option<CommandId>,
    pub view: Option<GameViewSelector>,
    pub known_revision: Option<u64>,
    pub session_token: Option<String>,
    pub admission_token: Option<String>,
}
impl zeroize::Zeroize for GamesRequest {
    fn zeroize(&mut self) {
        self.session_token.zeroize();
        self.admission_token.zeroize();
        if let GamePayload::JoinPlayer(input) = &mut self.payload {
            input.recovery_answer.zeroize();
        }
    }
}
impl Drop for GamesRequest {
    fn drop(&mut self) {
        self.zeroize();
    }
}
/// Canonical game path for both game-scoped Secure cookies (not account cookies).
pub fn game_cookie_path(id: GameId) -> String {
    format!("/api/games/{id}")
}

pub fn decode_games(
    method: &Method,
    path: &str,
    query: Option<&str>,
    headers: &HeaderMap,
    body: &[u8],
    origin: &str,
) -> Result<GamesRequest, ApiError> {
    let (operation, game_id) = route(path)?;
    let read = matches!(operation, GameOperation::Sync | GameOperation::Stream);
    if method != if read { Method::GET } else { Method::POST } {
        return Err(ApiError::MethodNotAllowed);
    }
    validate_headers(headers)?;
    if (!read || operation == GameOperation::Stream || headers.contains_key("origin"))
        && (headers.get_all("origin").iter().count() != 1
            || headers.get("origin").and_then(|v| v.to_str().ok()) != Some(origin))
    {
        return Err(ApiError::Forbidden);
    }
    let (view, known_revision) = if read || operation == GameOperation::Exit {
        read_query(query, headers, operation == GameOperation::Sync)?
    } else {
        if query.is_some() {
            return Err(ApiError::InvalidInput);
        }
        (None, None)
    };
    if operation == GameOperation::Stream {
        require_upgrade(headers)?;
    }
    if body.len() > super::BODY_LIMIT {
        return Err(ApiError::PayloadTooLarge);
    }
    if read {
        if !body.is_empty() {
            return Err(ApiError::InvalidInput);
        }
    } else {
        require_json(headers)?;
    }
    let payload = match operation {
        GameOperation::Create => {
            GamePayload::Create(CreateGame::decode_json(body).map_err(|_| ApiError::InvalidInput)?)
        }
        GameOperation::Lobby | GameOperation::Start | GameOperation::CallRandom => {
            GamePayload::Revision(
                RevisionCommand::decode_json(body).map_err(|_| ApiError::InvalidInput)?,
            )
        }
        GameOperation::AdmissionContext => GamePayload::AdmissionContext(
            AdmissionContextInput::decode_json(body).map_err(|_| ApiError::InvalidInput)?,
        ),
        GameOperation::JoinPlayer => GamePayload::JoinPlayer(
            JoinPlayer::decode_json(body).map_err(|_| ApiError::InvalidInput)?,
        ),
        GameOperation::CallManual => GamePayload::CallManual(
            CallManualInput::decode_json(body).map_err(|_| ApiError::InvalidInput)?,
        ),
        GameOperation::Winner => {
            GamePayload::Winner(WinnerInput::decode_json(body).map_err(|_| ApiError::InvalidInput)?)
        }
        GameOperation::Cancel => GamePayload::Cancel(
            CancelGameInput::decode_json(body).map_err(|_| ApiError::InvalidInput)?,
        ),
        GameOperation::Exit => {
            decode_empty_game_body(body)?;
            GamePayload::Empty
        }
        GameOperation::Sync | GameOperation::Stream => GamePayload::Empty,
    };
    let command_id = if operation == GameOperation::AdmissionContext || read {
        if headers.contains_key("idempotency-key") {
            return Err(ApiError::InvalidInput);
        }
        None
    } else {
        Some(command_id(headers)?)
    };
    let (session_token, admission_token) = if matches!(
        operation,
        GameOperation::AdmissionContext | GameOperation::JoinPlayer
    ) {
        (
            named_cookie(headers, PLAYER_COOKIE_NAME)?,
            named_cookie(headers, ADMISSION_COOKIE_NAME)?,
        )
    } else if view == Some(GameViewSelector::Player) {
        (named_cookie(headers, PLAYER_COOKIE_NAME)?, None)
    } else {
        (super::decode::session_cookie(headers)?, None)
    };
    Ok(GamesRequest {
        operation,
        game_id,
        payload,
        command_id,
        view,
        known_revision,
        session_token,
        admission_token,
    })
}
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
fn require_upgrade(headers: &HeaderMap) -> Result<(), ApiError> {
    if headers.get_all("upgrade").iter().count() != 1
        || !headers
            .get("upgrade")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.eq_ignore_ascii_case("websocket"))
        || headers.get_all("connection").iter().count() != 1
        || !headers
            .get("connection")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| {
                v.split(',')
                    .any(|v| v.trim().eq_ignore_ascii_case("upgrade"))
            })
        || headers.contains_key("sec-websocket-protocol")
    {
        return Err(ApiError::InvalidInput);
    }
    Ok(())
}
fn read_query(
    query: Option<&str>,
    headers: &HeaderMap,
    allow_revision: bool,
) -> Result<(Option<GameViewSelector>, Option<u64>), ApiError> {
    let mut view = None;
    let mut known = None;
    if let Some(query) = query {
        if query.is_empty() || query.len() > GAME_QUERY_MAX_BYTES || !query.is_ascii() {
            return Err(ApiError::InvalidInput);
        }
        for (index, part) in query.split('&').enumerate() {
            if index >= 2 {
                return Err(ApiError::InvalidInput);
            }
            let (key, value) = part.split_once('=').ok_or(ApiError::InvalidInput)?;
            match key {
                GAME_QUERY_VIEW if view.is_none() => {
                    view = Some(match value {
                        GAME_VIEW_ACCOUNT => GameViewSelector::Account,
                        GAME_VIEW_PLAYER => GameViewSelector::Player,
                        _ => return Err(ApiError::InvalidInput),
                    })
                }
                GAME_QUERY_KNOWN_REVISION if allow_revision && known.is_none() => {
                    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
                        return Err(ApiError::InvalidInput);
                    }
                    let n = value.parse::<u64>().map_err(|_| ApiError::InvalidInput)?;
                    if n > brews_contracts::games::MAX_SAFE_REVISION {
                        return Err(ApiError::InvalidInput);
                    }
                    known = Some(n);
                }
                _ => return Err(ApiError::InvalidInput),
            }
        }
    }
    let mut account = false;
    let mut player = false;
    if let Some(raw) = headers.get("cookie") {
        for part in raw.to_str().map_err(|_| ApiError::InvalidInput)?.split(';') {
            if let Some((key, _)) = part.trim().split_once('=') {
                account |= key == "__Host-brews_session";
                player |= key == PLAYER_COOKIE_NAME;
            }
        }
    }
    if view.is_none() {
        if account && player {
            return Err(ApiError::InvalidInput);
        }
        view = Some(if player {
            GameViewSelector::Player
        } else {
            GameViewSelector::Account
        });
    }
    Ok((view, known))
}
// Project one cookie into the existing account-cookie decoder rather than copying
// its size/duplicate/header policy. Bound the original bytes before projection.
fn named_cookie(headers: &HeaderMap, name: &str) -> Result<Option<String>, ApiError> {
    let mut masked_headers = HeaderMap::new();
    for value in headers.get_all("cookie") {
        let raw = value.to_str().map_err(|_| ApiError::InvalidInput)?;
        let masked =
            zeroize::Zeroizing::new(raw.replace("__Host-brews_session", "__Skip-brews_session"));
        masked_headers.append(
            "cookie",
            http::HeaderValue::from_bytes(masked.as_bytes()).map_err(|_| ApiError::InvalidInput)?,
        );
    }
    super::decode::session_cookie(&masked_headers)?;
    let Some(raw) = headers.get("cookie") else {
        return Ok(None);
    };
    let raw = raw.to_str().map_err(|_| ApiError::InvalidInput)?;
    let mut projected = zeroize::Zeroizing::new(String::with_capacity(raw.len()));
    for part in raw.split(';') {
        if let Some((key, value)) = part.trim().split_once('=')
            && key == name
        {
            if !projected.is_empty() {
                projected.push(';');
            }
            projected.push_str("__Host-brews_session=");
            projected.push_str(value);
        }
    }
    let mut selected_headers = HeaderMap::new();
    selected_headers.insert(
        "cookie",
        http::HeaderValue::from_bytes(projected.as_bytes()).map_err(|_| ApiError::InvalidInput)?,
    );
    super::decode::session_cookie(&selected_headers)
}
fn route(path: &str) -> Result<(GameOperation, Option<GameId>), ApiError> {
    if path.len() > GAME_PATH_MAX_BYTES {
        return Err(ApiError::NotFound);
    }
    if path == "/api/games" {
        return Ok((GameOperation::Create, None));
    }
    let (id, action) = path
        .strip_prefix("/api/games/")
        .and_then(|v| v.split_once('/'))
        .ok_or(ApiError::NotFound)?;
    let id = id.parse().map_err(|_| ApiError::NotFound)?;
    let operation = match action {
        "lobby" => GameOperation::Lobby,
        "start" => GameOperation::Start,
        "calls/random" => GameOperation::CallRandom,
        "calls/manual" => GameOperation::CallManual,
        "winner" => GameOperation::Winner,
        "cancel" => GameOperation::Cancel,
        "exit" => GameOperation::Exit,
        "admission-context" => GameOperation::AdmissionContext,
        "players" => GameOperation::JoinPlayer,
        "sync" => GameOperation::Sync,
        "stream" => GameOperation::Stream,
        _ => return Err(ApiError::NotFound),
    };
    Ok((operation, Some(id)))
}
fn validate_headers(headers: &HeaderMap) -> Result<(), ApiError> {
    if headers.len() > GAME_HEADERS_MAX_FIELDS
        || headers
            .iter()
            .try_fold(0usize, |sum, (name, value)| {
                sum.checked_add(name.as_str().len())?
                    .checked_add(value.as_bytes().len())
            })
            .is_none_or(|size| size > GAME_HEADERS_MAX_BYTES)
    {
        return Err(ApiError::InvalidInput);
    }
    if headers.contains_key("authorization") {
        return Err(ApiError::Forbidden);
    }
    Ok(())
}
fn require_json(headers: &HeaderMap) -> Result<(), ApiError> {
    if headers.get_all("content-type").iter().count() != 1
        || !headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v == "application/json" || v == "application/json; charset=utf-8")
    {
        return Err(ApiError::UnsupportedMediaType);
    }
    Ok(())
}
fn command_id(headers: &HeaderMap) -> Result<CommandId, ApiError> {
    if headers.get_all("idempotency-key").iter().count() != 1 {
        return Err(ApiError::InvalidInput);
    }
    headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .ok_or(ApiError::InvalidInput)?
        .parse()
        .map_err(|_| ApiError::InvalidInput)
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "Tests fail fast on invalid bounded fixtures."
)]
mod tests {
    use super::*;
    const ORIGIN: &str = "https://localhost:8787";
    const ID: &str = "01890f3e-53b7-7d28-9b05-4f65092d5711";
    fn headers() -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("origin", ORIGIN.parse().unwrap());
        h.insert("idempotency-key", ID.parse().unwrap());
        h.insert("content-type", "application/json".parse().unwrap());
        h
    }
    fn routes() -> Vec<(Method, String, &'static [u8], HeaderMap)> {
        let mut context = headers();
        context.remove("idempotency-key");
        let mut stream = HeaderMap::new();
        stream.insert("origin", ORIGIN.parse().unwrap());
        stream.insert("upgrade", "websocket".parse().unwrap());
        stream.insert("connection", "Upgrade".parse().unwrap());
        vec![
            (Method::POST, "/api/games".into(), b"{}", headers()),
            (
                Method::POST,
                format!("/api/games/{ID}/lobby"),
                br#"{"expected_revision":0}"#,
                headers(),
            ),
            (
                Method::POST,
                format!("/api/games/{ID}/start"),
                br#"{"expected_revision":0}"#,
                headers(),
            ),
            (
                Method::POST,
                format!("/api/games/{ID}/calls/random"),
                br#"{"expected_revision":0}"#,
                headers(),
            ),
            (
                Method::POST,
                format!("/api/games/{ID}/calls/manual"),
                br#"{"value":"1","expected_revision":0}"#,
                headers(),
            ),
            (
                Method::POST,
                format!("/api/games/{ID}/winner"),
                br#"{"player_id":"01890f3e-53b7-7d28-9b05-4f65092d5711","expected_revision":0}"#,
                headers(),
            ),
            (
                Method::POST,
                format!("/api/games/{ID}/cancel"),
                br#"{"confirmed":true,"expected_state":"in_progress"}"#,
                headers(),
            ),
            (
                Method::POST,
                format!("/api/games/{ID}/exit"),
                b"{}",
                headers(),
            ),
            (
                Method::POST,
                format!("/api/games/{ID}/admission-context"),
                br#"{"game_code":"AB12CD34"}"#,
                context,
            ),
            (
                Method::POST,
                format!("/api/games/{ID}/players"),
                br#"{"game_code":"AB12CD34","alias":"Alice"}"#,
                headers(),
            ),
            (Method::GET, format!("/api/games/{ID}/stream"), b"", stream),
            (
                Method::GET,
                format!("/api/games/{ID}/sync"),
                b"",
                HeaderMap::new(),
            ),
        ]
    }
    #[test]
    fn request_zeroization_erases_selected_cookie_and_optional_recovery_material() {
        let path = format!("/api/games/{ID}/players");
        let mut h = headers();
        h.insert(
            "cookie",
            format!("{ADMISSION_COOKIE_NAME}=context-value")
                .parse()
                .unwrap(),
        );
        let mut request = decode_games(
            &Method::POST,
            &path,
            None,
            &h,
            br#"{"game_code":"AB12CD34","alias":"Alice","recovery_answer":"private-marker"}"#,
            ORIGIN,
        )
        .unwrap();
        request.zeroize();
        assert!(request.admission_token.is_none());
        assert!(
            matches!(&request.payload,GamePayload::JoinPlayer(input) if input.recovery_answer.is_none())
        );
    }
    #[test]
    fn shared_query_names_preserve_the_public_wire_contract() {
        let path = format!("/api/games/{ID}/sync");
        for (selector, expected) in [
            (GAME_VIEW_ACCOUNT, GameViewSelector::Account),
            (GAME_VIEW_PLAYER, GameViewSelector::Player),
        ] {
            let query = format!("{GAME_QUERY_VIEW}={selector}&{GAME_QUERY_KNOWN_REVISION}=1");
            let h = HeaderMap::new();
            let request = decode_games(&Method::GET, &path, Some(&query), &h, b"", ORIGIN).unwrap();
            assert_eq!(request.view, Some(expected));
            assert_eq!(request.known_revision, Some(1));
        }
    }
    #[test]
    fn exact_routes_reject_other_methods_authorization_and_origin_ambiguity() {
        let routes = routes();
        assert_eq!(routes.len(), 12);
        for (method, path, body, h) in routes {
            assert!(decode_games(&method, &path, None, &h, body, ORIGIN).is_ok());
            for other in [
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::DELETE,
                Method::PATCH,
                Method::OPTIONS,
                Method::HEAD,
            ]
            .into_iter()
            .filter(|v| v != method)
            {
                assert_eq!(
                    decode_games(&other, &path, None, &h, body, ORIGIN).err(),
                    Some(ApiError::MethodNotAllowed)
                );
            }
            let mut bad = h.clone();
            bad.insert("authorization", "untrusted".parse().unwrap());
            assert_eq!(
                decode_games(&method, &path, None, &bad, body, ORIGIN).err(),
                Some(ApiError::Forbidden)
            );
            let mut bad = h.clone();
            bad.insert("origin", "null".parse().unwrap());
            assert_eq!(
                decode_games(&method, &path, None, &bad, body, ORIGIN).err(),
                Some(ApiError::Forbidden)
            );
            let mut bad = h;
            bad.append("origin", ORIGIN.parse().unwrap());
            bad.append("origin", ORIGIN.parse().unwrap());
            assert_eq!(
                decode_games(&method, &path, None, &bad, body, ORIGIN).err(),
                Some(ApiError::Forbidden)
            );
        }
    }
    #[test]
    fn mutations_require_one_supported_media_type_and_exact_retry_header_rules() {
        for (method, path, body, h) in routes() {
            let needs_key = method == Method::POST && !path.ends_with("admission-context");
            if method == Method::POST {
                for media in [
                    None,
                    Some("text/plain"),
                    Some("application/json; charset=iso-8859-1"),
                    Some("application/json; garbage"),
                ] {
                    let mut bad = h.clone();
                    bad.remove("content-type");
                    if let Some(media) = media {
                        bad.insert("content-type", media.parse().unwrap());
                    }
                    assert_eq!(
                        decode_games(&method, &path, None, &bad, body, ORIGIN).err(),
                        Some(ApiError::UnsupportedMediaType)
                    );
                }
                let mut bad = h.clone();
                bad.append("content-type", "application/json".parse().unwrap());
                assert_eq!(
                    decode_games(&method, &path, None, &bad, body, ORIGIN).err(),
                    Some(ApiError::UnsupportedMediaType)
                );
                let mut valid = h.clone();
                valid.insert(
                    "content-type",
                    "application/json; charset=utf-8".parse().unwrap(),
                );
                assert!(decode_games(&method, &path, None, &valid, body, ORIGIN).is_ok());
            }
            if needs_key {
                for key in [
                    None,
                    Some(""),
                    Some("01890F3E-53B7-7D28-9B05-4F65092D5711"),
                    Some("01890f3e-53b7-4d28-9b05-4f65092d5711"),
                    Some(" 01890f3e-53b7-7d28-9b05-4f65092d5711"),
                ] {
                    let mut bad = h.clone();
                    bad.remove("idempotency-key");
                    if let Some(key) = key {
                        bad.insert("idempotency-key", key.parse().unwrap());
                    }
                    assert_eq!(
                        decode_games(&method, &path, None, &bad, body, ORIGIN).err(),
                        Some(ApiError::InvalidInput)
                    );
                }
                let mut bad = h;
                bad.append("idempotency-key", ID.parse().unwrap());
                assert_eq!(
                    decode_games(&method, &path, None, &bad, body, ORIGIN).err(),
                    Some(ApiError::InvalidInput)
                );
            } else {
                let mut bad = h;
                bad.insert("idempotency-key", ID.parse().unwrap());
                assert_eq!(
                    decode_games(&method, &path, None, &bad, body, ORIGIN).err(),
                    Some(ApiError::InvalidInput)
                );
            }
        }
    }
    #[test]
    fn all_transport_inputs_have_release_active_finite_bounds() {
        for (method, path, body, h) in routes() {
            assert_eq!(
                decode_games(
                    &method,
                    &path,
                    None,
                    &h,
                    &vec![b' '; super::super::BODY_LIMIT + 1],
                    ORIGIN
                )
                .err(),
                Some(ApiError::PayloadTooLarge)
            );
            let mut bad = h.clone();
            bad.insert(
                "x-padding",
                "x".repeat(GAME_HEADERS_MAX_BYTES).parse().unwrap(),
            );
            assert_eq!(
                decode_games(&method, &path, None, &bad, body, ORIGIN).err(),
                Some(ApiError::InvalidInput)
            );
            let mut bad = h.clone();
            for i in 0..=GAME_HEADERS_MAX_FIELDS {
                bad.append(
                    http::HeaderName::from_bytes(format!("x-{i}").as_bytes()).unwrap(),
                    "x".parse().unwrap(),
                );
            }
            assert_eq!(
                decode_games(&method, &path, None, &bad, body, ORIGIN).err(),
                Some(ApiError::InvalidInput)
            );
            assert_eq!(
                decode_games(
                    &method,
                    &path,
                    Some(&"x".repeat(GAME_QUERY_MAX_BYTES + 1)),
                    &h,
                    body,
                    ORIGIN
                )
                .err(),
                Some(ApiError::InvalidInput)
            );
            let mut bad = h.clone();
            bad.insert("cookie", "x".repeat(8193).parse().unwrap());
            assert_eq!(
                decode_games(&method, &path, None, &bad, body, ORIGIN).err(),
                Some(ApiError::InvalidInput)
            );
            let mut bad = h.clone();
            bad.append("cookie", "other=x".parse().unwrap());
            bad.append("cookie", "other=y".parse().unwrap());
            assert_eq!(
                decode_games(&method, &path, None, &bad, body, ORIGIN).err(),
                Some(ApiError::InvalidInput)
            );
            let mut bad = h;
            bad.insert("cookie", http::HeaderValue::from_bytes(b"\xff").unwrap());
            assert_eq!(
                decode_games(&method, &path, None, &bad, body, ORIGIN).err(),
                Some(ApiError::InvalidInput)
            );
        }
        let mut body = b"{}".to_vec();
        body.resize(super::super::BODY_LIMIT, b' ');
        assert!(decode_games(&Method::POST, "/api/games", None, &headers(), &body, ORIGIN).is_ok());
        let path = format!("/api/games/{ID}/sync");
        assert_eq!(
            decode_games(&Method::GET, &path, None, &HeaderMap::new(), b" ", ORIGIN).err(),
            Some(ApiError::InvalidInput)
        );
    }
    #[test]
    fn stream_requires_exact_origin_websocket_upgrade_and_noncredential_selector() {
        let path = format!("/api/games/{ID}/stream");
        let mut h = HeaderMap::new();
        h.insert("origin", ORIGIN.parse().unwrap());
        h.insert("upgrade", "websocket".parse().unwrap());
        h.insert("connection", "Upgrade".parse().unwrap());
        let result = decode_games(&Method::GET, &path, Some("view=account"), &h, b"", ORIGIN);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().operation, GameOperation::Stream);
        for name in ["origin", "upgrade", "connection"] {
            let mut bad = h.clone();
            bad.remove(name);
            assert!(
                decode_games(&Method::GET, &path, Some("view=account"), &bad, b"", ORIGIN).is_err()
            );
        }
        let mut bad = h.clone();
        bad.insert("origin", "https://other.example.test".parse().unwrap());
        assert_eq!(
            decode_games(&Method::GET, &path, None, &bad, b"", ORIGIN).err(),
            Some(ApiError::Forbidden)
        );
        assert_eq!(
            decode_games(
                &Method::GET,
                &path,
                Some("known_revision=1"),
                &h,
                b"",
                ORIGIN
            )
            .err(),
            Some(ApiError::InvalidInput)
        );
        let mut bad = h.clone();
        bad.insert("sec-websocket-protocol", "credential".parse().unwrap());
        assert_eq!(
            decode_games(&Method::GET, &path, None, &bad, b"", ORIGIN).err(),
            Some(ApiError::InvalidInput)
        );
        let mut bad = h;
        bad.append("upgrade", "websocket".parse().unwrap());
        assert_eq!(
            decode_games(&Method::GET, &path, None, &bad, b"", ORIGIN).err(),
            Some(ApiError::InvalidInput)
        );
    }
    #[test]
    fn sync_selects_only_requested_authority_when_account_and_player_cookies_coexist() {
        let path = format!("/api/games/{ID}/sync");
        let mut h = HeaderMap::new();
        h.insert(
            "cookie",
            format!("__Host-brews_session=account-value; {PLAYER_COOKIE_NAME}=player-value")
                .parse()
                .unwrap(),
        );
        let result = decode_games(
            &Method::GET,
            &path,
            Some("view=player&known_revision=1"),
            &h,
            b"",
            ORIGIN,
        );
        assert!(result.is_ok());
        let request = result.unwrap();
        assert_eq!(request.operation, GameOperation::Sync);
        assert_eq!(request.view, Some(GameViewSelector::Player));
        assert_eq!(request.known_revision, Some(1));
        assert_eq!(request.session_token.as_deref(), Some("player-value"));
        assert_eq!(
            decode_games(&Method::GET, &path, None, &h, b"", ORIGIN).err(),
            Some(ApiError::InvalidInput)
        );
        h.insert(
            "cookie",
            format!(
                "__Host-brews_session=; __Host-brews_session=bad; {PLAYER_COOKIE_NAME}=player-value"
            )
            .parse()
            .unwrap(),
        );
        assert!(decode_games(&Method::GET, &path, Some("view=player"), &h, b"", ORIGIN).is_ok());
        assert_eq!(
            decode_games(&Method::GET, &path, Some("view=account"), &h, b"", ORIGIN).err(),
            Some(ApiError::InvalidInput)
        );
        h.insert("cookie",format!("__Host-brews_session=account-value; {PLAYER_COOKIE_NAME}=; {PLAYER_COOKIE_NAME}=bad").parse().unwrap());
        assert!(decode_games(&Method::GET, &path, Some("view=account"), &h, b"", ORIGIN).is_ok());
        assert_eq!(
            decode_games(&Method::GET, &path, Some("view=player"), &h, b"", ORIGIN).err(),
            Some(ApiError::InvalidInput)
        );
        for query in [
            "",
            "view=host",
            "view=Player",
            "view=%70layer",
            "view=player&view=player",
            "known_revision=-1",
            "known_revision=1.0",
            "known_revision=9007199254740992",
            "token=x",
            "view=player&known_revision=1&x=y",
        ] {
            assert_eq!(
                decode_games(&Method::GET, &path, Some(query), &h, b"", ORIGIN).err(),
                Some(ApiError::InvalidInput)
            );
        }
    }
    #[test]
    fn admission_forwards_an_existing_player_cookie_without_claiming_authority() {
        for action in ["admission-context", "players"] {
            let mut h = headers();
            if action == "admission-context" {
                h.remove("idempotency-key");
            }
            h.insert(
                "cookie",
                format!("{ADMISSION_COOKIE_NAME}=context-value; {PLAYER_COOKIE_NAME}=player-value")
                    .parse()
                    .unwrap(),
            );
            let body = if action == "admission-context" {
                br#"{"game_code":"AB12CD34"}"#.as_slice()
            } else {
                br#"{"game_code":"AB12CD34","alias":"Alice"}"#.as_slice()
            };
            let request = decode_games(
                &Method::POST,
                &format!("/api/games/{ID}/{action}"),
                None,
                &h,
                body,
                ORIGIN,
            )
            .unwrap();
            assert_eq!(request.session_token.as_deref(), Some("player-value"));
            assert_eq!(request.admission_token.as_deref(), Some("context-value"));
        }
    }

    #[test]
    fn admission_context_is_keyless_and_join_uses_separate_game_scoped_cookie() {
        let context_path = format!("/api/games/{ID}/admission-context");
        let join_path = format!("/api/games/{ID}/players");
        let mut h = headers();
        h.remove("idempotency-key");
        h.insert(
            "cookie",
            format!("{ADMISSION_COOKIE_NAME}=context-value")
                .parse()
                .unwrap(),
        );
        let result = decode_games(
            &Method::POST,
            &context_path,
            None,
            &h,
            br#"{"game_code":" ab12cd34 "}"#,
            ORIGIN,
        );
        assert!(result.is_ok());
        let request = result.unwrap();
        assert_eq!(request.operation, GameOperation::AdmissionContext);
        assert!(request.command_id.is_none());
        assert_eq!(request.admission_token.as_deref(), Some("context-value"));
        h.insert("idempotency-key", ID.parse().unwrap());
        let request = decode_games(
            &Method::POST,
            &join_path,
            None,
            &h,
            br#"{"game_code":"AB12CD34","alias":" Alice "}"#,
            ORIGIN,
        )
        .unwrap();
        assert_eq!(request.operation, GameOperation::JoinPlayer);
        assert_eq!(request.admission_token.as_deref(), Some("context-value"));
        assert!(request.session_token.is_none());
        assert!(matches!(&request.payload,GamePayload::JoinPlayer(input) if input.alias=="Alice"));
        assert_eq!(
            game_cookie_path(ID.parse().unwrap()),
            format!("/api/games/{ID}")
        );
        for body in [
            b"[]".as_slice(),
            br#"{"game_code":"AB12CD34","alias":null}"#,
            br#"{"game_code":"AB12CD34","alias":"Alice","recovery_answer":null}"#,
            br#"{"game_code":"AB12CD34","alias":"Alice","player_id":"x"}"#,
        ] {
            assert_eq!(
                decode_games(&Method::POST, &join_path, None, &h, body, ORIGIN).err(),
                Some(ApiError::InvalidInput)
            );
        }
    }
    #[test]
    fn final_exit_selects_existing_cookie_without_accepting_a_target_or_revision() {
        let path = format!("/api/games/{ID}/exit");
        let mut h = headers();
        h.insert(
            "cookie",
            format!("__Host-brews_session=account-value; {PLAYER_COOKIE_NAME}=player-value")
                .parse()
                .unwrap(),
        );
        for (selector, token) in [("account", "account-value"), ("player", "player-value")] {
            let request = decode_games(
                &Method::POST,
                &path,
                Some(&format!("view={selector}")),
                &h,
                b"{}",
                ORIGIN,
            )
            .unwrap();
            assert_eq!(request.session_token.as_deref(), Some(token));
            assert!(request.known_revision.is_none());
            for body in [
                b"[]".as_slice(),
                b"null",
                br#"{"player_id":"untrusted"}"#,
                br#"{"confirmed":true}"#,
            ] {
                assert!(
                    decode_games(
                        &Method::POST,
                        &path,
                        Some(&format!("view={selector}")),
                        &h,
                        body,
                        ORIGIN
                    )
                    .is_err()
                );
            }
            assert!(
                decode_games(
                    &Method::POST,
                    &path,
                    Some(&format!("view={selector}&known_revision=1")),
                    &h,
                    b"{}",
                    ORIGIN
                )
                .is_err()
            );
        }
        assert!(decode_games(&Method::POST, &path, None, &h, b"{}", ORIGIN).is_err());
    }
    #[test]
    fn gameplay_payloads_preserve_exact_values_identity_and_confirmation() {
        let mut h = headers();
        h.insert(
            "cookie",
            format!("__Host-brews_session=account-value; {PLAYER_COOKIE_NAME}=player-value")
                .parse()
                .unwrap(),
        );
        for (action, body) in [
            ("calls/random", r#"{"expected_revision":1}"#.to_owned()),
            (
                "calls/manual",
                r#"{"value":"2","expected_revision":1}"#.to_owned(),
            ),
            (
                "winner",
                format!(r#"{{"player_id":"{ID}","expected_revision":1}}"#),
            ),
            (
                "cancel",
                r#"{"confirmed":true,"expected_state":"new"}"#.to_owned(),
            ),
            (
                "cancel",
                r#"{"confirmed":true,"expected_state":"awaiting_players"}"#.to_owned(),
            ),
            (
                "cancel",
                r#"{"confirmed":true,"expected_state":"in_progress"}"#.to_owned(),
            ),
        ] {
            let path = format!("/api/games/{ID}/{action}");
            let request =
                decode_games(&Method::POST, &path, None, &h, body.as_bytes(), ORIGIN).unwrap();
            assert_eq!(request.session_token.as_deref(), Some("account-value"));
            assert!(request.admission_token.is_none());
            assert_eq!(request.command_id.unwrap().to_string(), ID);
            assert_eq!(request.game_id.unwrap().to_string(), ID);
            assert!(request.view.is_none());
            for invalid in [b"[]".as_slice(), b"{}", b"null"] {
                assert_eq!(
                    decode_games(&Method::POST, &path, None, &h, invalid, ORIGIN).err(),
                    Some(ApiError::InvalidInput)
                );
            }
            let injected = body.replace('}', &format!(",\"actor_id\":\"{ID}\"}}"));
            assert_eq!(
                decode_games(&Method::POST, &path, None, &h, injected.as_bytes(), ORIGIN).err(),
                Some(ApiError::InvalidInput)
            );
            assert_eq!(
                decode_games(
                    &Method::POST,
                    &path,
                    Some("view=player"),
                    &h,
                    body.as_bytes(),
                    ORIGIN
                )
                .err(),
                Some(ApiError::InvalidInput)
            );
            for suffix in [
                format!("{action}/"),
                action.to_uppercase(),
                format!("{action}/extra"),
            ] {
                assert_eq!(
                    decode_games(
                        &Method::POST,
                        &format!("/api/games/{ID}/{suffix}"),
                        None,
                        &h,
                        body.as_bytes(),
                        ORIGIN
                    )
                    .err(),
                    Some(ApiError::NotFound)
                );
            }
        }
        for body in [
            r#"{"value":"02","expected_revision":1}"#,
            r#"{"value":2,"expected_revision":1}"#,
            r#"{"value":"2","expected_revision":1,"\u0076alue":"3"}"#,
            r#"{"value":"2","expected_revision":1,"board":[]}"#,
        ] {
            assert_eq!(
                decode_games(
                    &Method::POST,
                    &format!("/api/games/{ID}/calls/manual"),
                    None,
                    &h,
                    body.as_bytes(),
                    ORIGIN
                )
                .err(),
                Some(ApiError::InvalidInput)
            );
        }
        for body in [
            r#"{"confirmed":false,"expected_state":"in_progress"}"#,
            r#"{"confirmed":true,"expected_state":"resolved"}"#,
            r#"{"confirmed":true,"expected_state":"new","\u0063onfirmed":true}"#,
            r#"{"confirmed":true,"expected_state":"in_progress","winner":null}"#,
        ] {
            assert_eq!(
                decode_games(
                    &Method::POST,
                    &format!("/api/games/{ID}/cancel"),
                    None,
                    &h,
                    body.as_bytes(),
                    ORIGIN
                )
                .err(),
                Some(ApiError::InvalidInput)
            );
        }
    }
    #[test]
    fn lobby_and_start_require_canonical_paths_and_original_required_revision() {
        for (suffix, operation) in [
            ("lobby", GameOperation::Lobby),
            ("start", GameOperation::Start),
        ] {
            let path = format!("/api/games/{ID}/{suffix}");
            let result = decode_games(
                &Method::POST,
                &path,
                None,
                &headers(),
                br#"{"expected_revision":1}"#,
                ORIGIN,
            );
            assert!(result.is_ok());
            let request = result.unwrap();
            assert_eq!(request.operation, operation);
            assert_eq!(request.game_id.unwrap().to_string(), ID);
            assert!(
                matches!(&request.payload,GamePayload::Revision(input) if input.expected_revision==1)
            );
            for body in [
                b"{}".as_slice(),
                b"[]",
                br#"{"expected_revision":null}"#,
                br#"{"expected_revision":1,"\u0065xpected_revision":1}"#,
                br#"{"expected_revision":1,"seed":2}"#,
                br#"{"expected_revision":1,"roster":[]}"#,
                br#"{"expected_revision":1,"boards":[]}"#,
                br#"{"expected_revision":1,"connected_player_count":2}"#,
            ] {
                assert_eq!(
                    decode_games(&Method::POST, &path, None, &headers(), body, ORIGIN).err(),
                    Some(ApiError::InvalidInput)
                );
            }
            for id in [
                ID.to_uppercase(),
                ID.replace('-', ""),
                ID.replace("7d28", "4d28"),
                format!("%30{}", &ID[1..]),
                format!(" {ID}"),
            ] {
                assert_eq!(
                    decode_games(
                        &Method::POST,
                        &format!("/api/games/{id}/{suffix}"),
                        None,
                        &headers(),
                        b"{}",
                        ORIGIN
                    )
                    .err(),
                    Some(ApiError::NotFound)
                );
            }
            assert_eq!(
                decode_games(
                    &Method::POST,
                    &(path + "/"),
                    None,
                    &headers(),
                    b"{}",
                    ORIGIN
                )
                .err(),
                Some(ApiError::NotFound)
            );
        }
    }
    #[test]
    fn creation_route_decodes_original_configuration_without_caller_identity() {
        let result = decode_games(&Method::POST, "/api/games", None, &headers(), b"{}", ORIGIN);
        assert!(result.is_ok());
        let request = result.unwrap();
        assert_eq!(request.operation, GameOperation::Create);
        assert!(request.game_id.is_none());
        assert_eq!(request.command_id.unwrap().to_string(), ID);
        assert!(
            matches!(&request.payload,GamePayload::Create(input) if input.configuration==brews_domain::games::GameConfiguration::default())
        );
        for body in [
            b"[]".as_slice(),
            br#"{"seed":1}"#,
            br#"{"host_id":"x"}"#,
            br#"{"configuration":null}"#,
            br#"{"configuration":{"board_side_length":4,"\u0062oard_side_length":5}}"#,
        ] {
            assert_eq!(
                decode_games(&Method::POST, "/api/games", None, &headers(), body, ORIGIN).err(),
                Some(ApiError::InvalidInput)
            );
        }
    }
}
