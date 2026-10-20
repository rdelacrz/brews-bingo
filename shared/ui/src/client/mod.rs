//! Same-origin, bounded transport. Request material never becomes diagnostic data.
#[cfg(target_arch = "wasm32")]
mod browser;
mod command;

pub use command::{CommandState, use_command};
use serde::de::DeserializeOwned;
use serde_json::Value;
use zeroize::Zeroizing;

#[cfg(any(target_arch = "wasm32", test))]
pub const RESPONSE_MAX_BYTES: usize = 2 * 1024 * 1024;
const REQUEST_MAX_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        dead_code,
        reason = "Browser transport constructs these errors; native tests cover selected cases."
    )
)]
pub enum ClientError {
    #[error("Your session has ended. Sign in again or use eligible player recovery.")]
    Unauthorized,
    #[error("You do not have access to this action.")]
    Forbidden,
    #[error("Check the submitted fields and try again.")]
    InvalidInput,
    #[error(
        "Unable to sign in with those credentials. Contact an authorized admin if you need a setup or reset link."
    )]
    InvalidCredentials,
    #[error("This link is unavailable. Ask an authorized issuer for a fresh link.")]
    InvalidLink,
    #[error("Too many requests. Wait before trying again.")]
    RateLimited,
    #[error("The requested resource is unavailable or access has ended.")]
    NotFound,
    #[error(
        "The server state changed or this command conflicts with an earlier request. Refresh before continuing."
    )]
    Conflict,
    #[error(
        "This account hosts a current game. Transfer or explicitly end/cancel the game before disabling or deleting the account."
    )]
    HostedGame,
    #[error(
        "Keep at least one enabled, Verified Admin. Another Admin must finish setup before this action is allowed."
    )]
    LastAdmin,
    #[error(
        "This command can no longer be reconciled safely. Refresh the authoritative state before continuing."
    )]
    StaleCommand,
    #[error("The service is temporarily unavailable. Try again later.")]
    Unavailable,
    #[error(
        "The command outcome is unknown. Retry the unchanged request to check the original command; do not issue a replacement."
    )]
    UnknownOutcome,
    #[error("The server response could not be validated. Refresh before continuing.")]
    InvalidResponse,
    #[error("The request or response exceeds the supported size.")]
    TooLarge,
    #[error(
        "An earlier command is unresolved. Retry its original inputs before changing this action."
    )]
    ChangedCommand,
    #[error("A command is already in progress.")]
    Busy,
    #[error("You are offline. Reconnect and refresh before sending a command.")]
    Offline,
    #[error("This action requires a supported browser.")]
    BrowserUnavailable,
    #[error("Copy was unavailable. Select and copy the text manually.")]
    ClipboardUnavailable,
}
impl ClientError {
    pub const fn unauthorized(&self) -> bool {
        matches!(self, Self::Unauthorized | Self::Forbidden)
    }
    pub const fn uncertain(&self) -> bool {
        matches!(self, Self::UnknownOutcome | Self::StaleCommand)
    }
}

pub async fn get<T: DeserializeOwned>(path: &str) -> Result<T, ClientError> {
    send("GET", path, None, None).await
}

pub async fn request<T: DeserializeOwned>(
    method: &str,
    path: &str,
    body: &Value,
    command_id: Option<&str>,
) -> Result<T, ClientError> {
    let encoded = encode(body)?;
    send(method, path, Some(&encoded), command_id).await
}

#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        dead_code,
        reason = "Only browser transport dispatches authentication requests."
    )
)]
struct AuthAuthority {
    expected: Option<crate::session::ExpectedRestrictedSession>,
    current: Box<dyn Fn() -> bool>,
}

pub(crate) async fn auth_request<T: DeserializeOwned>(
    path: &str,
    body: &Value,
    command_id: &str,
    expected: Option<crate::session::ExpectedRestrictedSession>,
    current: impl Fn() -> bool + 'static,
) -> Result<T, ClientError> {
    let encoded = encode(body)?;
    send_authorized(
        "POST",
        path,
        Some(&encoded),
        Some(command_id),
        Some(AuthAuthority {
            expected,
            current: Box::new(current),
        }),
    )
    .await
}

fn encode(body: &Value) -> Result<Zeroizing<String>, ClientError> {
    let encoded =
        Zeroizing::new(serde_json::to_string(body).map_err(|_| ClientError::InvalidInput)?);
    if encoded.len() > REQUEST_MAX_BYTES {
        return Err(ClientError::TooLarge);
    }
    Ok(encoded)
}

async fn send<T: DeserializeOwned>(
    method: &str,
    path: &str,
    body: Option<&str>,
    command_id: Option<&str>,
) -> Result<T, ClientError> {
    send_authorized(method, path, body, command_id, None).await
}

async fn send_authorized<T: DeserializeOwned>(
    method: &str,
    path: &str,
    body: Option<&str>,
    command_id: Option<&str>,
    authority: Option<AuthAuthority>,
) -> Result<T, ClientError> {
    validate_path(path)?;
    if !matches!(method, "GET" | "POST" | "PATCH" | "PUT" | "DELETE") {
        return Err(ClientError::InvalidInput);
    }
    if let Some(id) = command_id {
        let parsed = uuid::Uuid::parse_str(id).map_err(|_| ClientError::InvalidInput)?;
        if parsed.get_version_num() != 7 {
            return Err(ClientError::InvalidInput);
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        browser::send(method, path, body, command_id, authority).await
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (body, command_id, authority);
        Err(ClientError::BrowserUnavailable)
    }
}

fn validate_path(path: &str) -> Result<(), ClientError> {
    if !path.starts_with("/api/")
        || path.len() > 4096
        || !path.is_ascii()
        || path
            .bytes()
            .any(|b| b.is_ascii_control() || matches!(b, b'\\' | b'#' | b' '))
    {
        return Err(ClientError::InvalidInput);
    }
    let base =
        url::Url::parse("https://same-origin.invalid").map_err(|_| ClientError::InvalidInput)?;
    let resolved = base.join(path).map_err(|_| ClientError::InvalidInput)?;
    if resolved.origin() != base.origin() || !resolved.path().starts_with("/api/") {
        return Err(ClientError::InvalidInput);
    }
    Ok(())
}

// A response Date is second precision. Anchor its lower bound at receipt time,
// so network latency and header rounding cannot put a command in the future.
#[cfg(any(target_arch = "wasm32", test))]
#[derive(Clone, Copy)]
struct CommandClock {
    server_ms: i64,
    monotonic_ms: f64,
}
#[cfg(any(target_arch = "wasm32", test))]
impl CommandClock {
    fn timestamp(self, monotonic_ms: f64) -> Result<i64, ClientError> {
        let elapsed = monotonic_ms - self.monotonic_ms;
        if !elapsed.is_finite() || elapsed < 0.0 {
            return Err(ClientError::Unavailable);
        }
        Ok(self
            .server_ms
            .saturating_sub(1_000)
            .saturating_add(elapsed.floor() as i64))
    }
}

// Expiry uses the upper Date bound plus the entire request round trip. UUIDs
// deliberately use the separate lower bound above to avoid future commands.
#[cfg(any(target_arch = "wasm32", test))]
#[derive(Default)]
struct AuthorityClock {
    anchor: Option<CommandClock>,
    floor_ms: i64,
}
#[cfg(any(target_arch = "wasm32", test))]
impl AuthorityClock {
    fn calibrate(
        &mut self,
        server_ms: i64,
        sent_ms: f64,
        received_ms: f64,
    ) -> Result<(), ClientError> {
        let round_trip = received_ms - sent_ms;
        if server_ms < 0
            || !sent_ms.is_finite()
            || !received_ms.is_finite()
            || !round_trip.is_finite()
            || round_trip < 0.0
        {
            return Err(ClientError::Unavailable);
        }
        if self.anchor.is_some() {
            self.timestamp(received_ms)?;
        }
        self.floor_ms = self.floor_ms.max(
            server_ms
                .saturating_add(1_000)
                .saturating_add(round_trip.ceil() as i64),
        );
        self.anchor = Some(CommandClock {
            server_ms: self.floor_ms,
            monotonic_ms: received_ms,
        });
        Ok(())
    }
    fn timestamp(&mut self, monotonic_ms: f64) -> Result<i64, ClientError> {
        let anchor = self.anchor.ok_or(ClientError::Unavailable)?;
        let elapsed = monotonic_ms - anchor.monotonic_ms;
        if !elapsed.is_finite() || elapsed < 0.0 {
            // A broken monotonic source must never resurrect local authority.
            self.floor_ms = i64::MAX;
            return Err(ClientError::Unavailable);
        }
        self.floor_ms = self
            .floor_ms
            .max(anchor.server_ms.saturating_add(elapsed.ceil() as i64));
        Ok(self.floor_ms)
    }
}

#[cfg(any(target_arch = "wasm32", test))]
fn cookie_lane_required(method: &str, path: &str) -> bool {
    path == "/api/session" || (method != "GET" && path.starts_with("/api/auth/"))
}

#[cfg(any(target_arch = "wasm32", test))]
const COOKIE_QUEUE_TIMEOUT_MS: u32 = 30_000;

// Queued secrets remain removable while a detached session preflight finishes.
#[cfg(any(target_arch = "wasm32", test))]
struct CookieQueue<T> {
    pending: Option<T>,
    queued_ms: f64,
}
#[cfg(any(target_arch = "wasm32", test))]
impl<T> CookieQueue<T> {
    fn invalidate(&mut self) {
        self.pending = None;
    }
    fn acquired(&mut self, now_ms: f64) -> Result<(), ClientError> {
        let elapsed = now_ms - self.queued_ms;
        if self.pending.is_none()
            || !elapsed.is_finite()
            || elapsed < 0.0
            || elapsed >= f64::from(COOKIE_QUEUE_TIMEOUT_MS)
        {
            self.invalidate();
            return Err(ClientError::Busy);
        }
        Ok(())
    }
    fn take_if_current(&mut self, current: bool) -> Result<T, ClientError> {
        if !current {
            self.invalidate();
        }
        self.pending.take().ok_or(ClientError::Busy)
    }
}

#[cfg(any(target_arch = "wasm32", test))]
#[derive(Default)]
struct CookieWriteGate {
    generation: u64,
    logout_pending: bool,
}
#[cfg(any(target_arch = "wasm32", test))]
impl CookieWriteGate {
    fn logout(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.logout_pending = true;
        self.generation
    }
    fn permits(&self, generation: u64) -> bool {
        !self.logout_pending && self.generation == generation
    }
    fn revoked(&mut self, generation: u64) {
        if self.generation == generation {
            self.logout_pending = false;
        }
    }
}

pub(crate) async fn prepare_command_clock() -> Result<(), ClientError> {
    #[cfg(target_arch = "wasm32")]
    {
        browser::prepare_command_clock().await
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Err(ClientError::BrowserUnavailable)
    }
}

pub fn new_command_id() -> Result<String, ClientError> {
    #[cfg(target_arch = "wasm32")]
    {
        browser::new_command_id()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        // Native tests do not require a browser or operating-system RNG adapter.
        Err(ClientError::BrowserUnavailable)
    }
}
pub fn now_ms() -> i64 {
    #[cfg(target_arch = "wasm32")]
    {
        browser::now_ms()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .and_then(|d| i64::try_from(d.as_millis()).ok())
            .unwrap_or(0)
    }
}
pub fn take_link_fragment() -> Result<Option<String>, ClientError> {
    #[cfg(target_arch = "wasm32")]
    {
        browser::take_link_fragment()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Err(ClientError::BrowserUnavailable)
    }
}
pub async fn copy_text(text: &str) -> Result<(), ClientError> {
    #[cfg(target_arch = "wasm32")]
    {
        browser::copy_text(text).await
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = text;
        Err(ClientError::BrowserUnavailable)
    }
}

#[cfg(any(target_arch = "wasm32", test))]
pub(crate) fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, ClientError> {
    if bytes.len() > RESPONSE_MAX_BYTES {
        return Err(ClientError::TooLarge);
    }
    if bytes
        .iter()
        .find(|byte| !matches!(byte, b' ' | b'\t' | b'\n' | b'\r'))
        != Some(&b'{')
    {
        return Err(ClientError::InvalidResponse);
    }
    serde_json::from_slice(bytes).map_err(|_| ClientError::InvalidResponse)
}

#[cfg(any(target_arch = "wasm32", test))]
fn response_error(status: u16, bytes: &[u8], mutation: bool) -> ClientError {
    #[derive(serde::Deserialize)]
    struct Envelope {
        error: ErrorCode,
    }
    #[derive(serde::Deserialize)]
    struct ErrorCode {
        code: String,
    }
    if status >= 500 || status == 408 {
        return if mutation {
            ClientError::UnknownOutcome
        } else {
            ClientError::Unavailable
        };
    }
    let code = decode::<Envelope>(bytes).ok();
    match code.as_ref().map(|e| e.error.code.as_str()) {
        Some("invalid_credentials") => ClientError::InvalidCredentials,
        Some("invalid_link") => ClientError::InvalidLink,
        Some("stale_command") => ClientError::StaleCommand,
        Some("hosted_game") if status == 409 => ClientError::HostedGame,
        Some("last_admin") if status == 409 => ClientError::LastAdmin,
        _ => match status {
            400 | 413 | 415 | 422 => ClientError::InvalidInput,
            401 => ClientError::Unauthorized,
            403 => ClientError::Forbidden,
            404 | 410 => ClientError::NotFound,
            409 | 412 => ClientError::Conflict,
            429 => ClientError::RateLimited,
            _ if mutation => ClientError::UnknownOutcome,
            _ => ClientError::Unavailable,
        },
    }
}

// Keep map entries intact so Serde can reject duplicate fields in nested DTOs.
pub(crate) fn optional_object<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    struct Optional<T>(std::marker::PhantomData<T>);
    impl<'de, T: serde::Deserialize<'de>> serde::de::Visitor<'de> for Optional<T> {
        type Value = Option<T>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("an object or null")
        }
        fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_some<D: serde::Deserializer<'de>>(
            self,
            deserializer: D,
        ) -> Result<Self::Value, D::Error> {
            struct Object<T>(std::marker::PhantomData<T>);
            impl<'de, T: serde::Deserialize<'de>> serde::de::Visitor<'de> for Object<T> {
                type Value = T;
                fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    formatter.write_str("an object")
                }
                fn visit_map<A: serde::de::MapAccess<'de>>(self, map: A) -> Result<T, A::Error> {
                    T::deserialize(serde::de::value::MapAccessDeserializer::new(map))
                }
            }
            deserializer
                .deserialize_map(Object(std::marker::PhantomData))
                .map(Some)
        }
    }
    deserializer.deserialize_option(Optional(std::marker::PhantomData))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expiry_uses_the_upper_server_bound_and_monotonic_elapsed() {
        let mut clock = AuthorityClock::default();
        assert_eq!(clock.timestamp(10.0), Err(ClientError::Unavailable));
        assert_eq!(clock.calibrate(100_000, 10.0, 210.5), Ok(()));
        // Header rounding and the full network interval are charged immediately.
        assert_eq!(clock.timestamp(210.5), Ok(101_201));
        let deadline = 105_000;
        assert!(clock.timestamp(4_008.5).is_ok_and(|now| now < deadline));
        // No wall-clock input or server refresh is needed, even while hidden.
        assert_eq!(clock.timestamp(4_009.5), Ok(deadline));
    }
    #[test]
    fn recalibration_never_retreats_or_postpones_the_original_deadline() {
        let mut clock = AuthorityClock::default();
        assert_eq!(clock.calibrate(100_000, 0.0, 0.0), Ok(()));
        let deadline = 111_000;
        let original_wait = deadline - clock.timestamp(0.0).unwrap_or(i64::MAX);
        assert_eq!(original_wait, 10_000);
        // A later response with an older Date cannot extend retained authority.
        assert_eq!(clock.calibrate(90_000, 1_000.0, 2_000.0), Ok(()));
        assert_eq!(clock.timestamp(2_000.0), Ok(103_000));
        assert_eq!(clock.timestamp(original_wait as f64), Ok(deadline));
        // Even a monotonic-source fault cannot resurrect expired authority.
        assert_eq!(clock.timestamp(1_000.0), Err(ClientError::Unavailable));
        assert_eq!(clock.timestamp(10_000.0), Ok(i64::MAX));
    }
    #[test]
    fn invalid_calibration_fails_closed_and_uuid_bias_stays_separate() {
        for (sent, received) in [(2.0, 1.0), (f64::NAN, 1.0), (0.0, f64::INFINITY)] {
            assert_eq!(
                AuthorityClock::default().calibrate(100_000, sent, received),
                Err(ClientError::Unavailable)
            );
        }
        let mut authority = AuthorityClock::default();
        assert_eq!(authority.calibrate(100_000, 0.0, 500.0), Ok(()));
        let command = CommandClock {
            server_ms: 100_000,
            monotonic_ms: 500.0,
        };
        assert_eq!(authority.timestamp(500.0), Ok(101_500));
        assert_eq!(command.timestamp(500.0), Ok(99_000));
    }
    #[test]
    fn destructive_session_reads_share_the_auth_cookie_lane() {
        assert!(cookie_lane_required("GET", "/api/session"));
        for path in [
            "login",
            "logout",
            "enrollment/redeem",
            "enrollment/complete",
            "password-reset/redeem",
            "password-reset/complete",
        ] {
            assert!(cookie_lane_required("POST", &format!("/api/auth/{path}")));
        }
        assert!(!cookie_lane_required("GET", "/api/games"));
        assert!(!cookie_lane_required("POST", "/api/games"));
    }
    #[test]
    fn command_clock_uses_server_time_and_monotonic_elapsed() {
        let clock = CommandClock {
            server_ms: 100_000,
            monotonic_ms: 25.0,
        };
        // Advancing wall time by a minute has no input into this clock.
        assert_eq!(clock.timestamp(25.0), Ok(99_000));
        assert_eq!(clock.timestamp(85.75), Ok(99_060));
        assert_eq!(clock.timestamp(1_025.0), Ok(100_000));
        assert_eq!(clock.timestamp(24.0), Err(ClientError::Unavailable));
        assert_eq!(clock.timestamp(f64::NAN), Err(ClientError::Unavailable));
    }
    #[test]
    fn cookie_queue_deadline_covers_both_lock_waits() {
        let mut queue = CookieQueue {
            pending: Some(()),
            queued_ms: 100.0,
        };
        assert_eq!(queue.acquired(101.0), Ok(())); // Local lane acquired.
        assert_eq!(queue.acquired(30_099.0), Ok(()));
        assert_eq!(queue.acquired(30_100.0), Err(ClientError::Busy)); // Web Lock too late.
        assert!(queue.pending.is_none());
        for now in [99.0, f64::NAN, f64::INFINITY] {
            let mut queue = CookieQueue {
                pending: Some(()),
                queued_ms: 100.0,
            };
            assert_eq!(queue.acquired(now), Err(ClientError::Busy));
            assert!(queue.pending.is_none());
        }
    }
    #[test]
    fn cancellation_during_queue_or_preflight_drops_body_and_prevents_dispatch() {
        struct Body(std::rc::Rc<std::cell::Cell<usize>>);
        impl Drop for Body {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }
        for preflight_started in [false, true] {
            let dropped = std::rc::Rc::new(std::cell::Cell::new(0));
            let mut queue = CookieQueue {
                pending: Some(Body(dropped.clone())),
                queued_ms: 0.0,
            };
            if preflight_started {
                assert_eq!(queue.acquired(1.0), Ok(()));
            }
            queue.invalidate();
            assert_eq!(dropped.get(), 1);
            // Finishing an already sent GET never authorizes a subsequent POST.
            assert!(matches!(
                queue.take_if_current(true),
                Err(ClientError::Busy)
            ));
            assert_eq!(dropped.get(), 1);
        }
    }
    #[test]
    fn invalidation_before_transmission_fences_auth_but_dispatched_work_finishes() {
        let mut stale = CookieQueue {
            pending: Some(()),
            queued_ms: 0.0,
        };
        assert!(matches!(
            stale.take_if_current(false),
            Err(ClientError::Busy)
        ));
        assert!(stale.pending.is_none());
        let mut active = CookieQueue {
            pending: Some(String::from("fixture")),
            queued_ms: 0.0,
        };
        let dispatched = active.take_if_current(true);
        active.invalidate();
        assert_eq!(dispatched.as_deref(), Ok("fixture"));
        assert!(matches!(
            active.take_if_current(true),
            Err(ClientError::Busy)
        ));
    }
    #[test]
    fn logout_fences_queued_auth_until_revoked_and_rejects_old_tickets() {
        let mut gate = CookieWriteGate::default();
        let original = gate.generation;
        assert!(gate.permits(original));
        let logout = gate.logout();
        assert!(!gate.permits(original));
        assert!(!gate.permits(logout));
        // An unconfirmed logout leaves every authentication fenced.
        gate.revoked(original);
        assert!(!gate.permits(logout));
        gate.revoked(logout);
        assert!(!gate.permits(original));
        assert!(gate.permits(gate.generation));
    }
    #[test]
    fn older_logout_cannot_release_a_newer_pending_logout() {
        let mut gate = CookieWriteGate::default();
        let first = gate.logout();
        let latest = gate.logout();
        gate.revoked(first);
        assert!(!gate.permits(latest));
        gate.revoked(latest);
        assert!(gate.permits(latest));
    }
    #[test]
    fn paths_cannot_escape_the_api_origin_or_root() {
        for path in [
            "https://other.invalid/api/session",
            "//other.invalid/api/x",
            "/api/../private",
            "/api/%2e%2e/private",
            "/api/x#secret",
            "/api/\\other",
            "/api/x\n",
        ] {
            assert_eq!(validate_path(path), Err(ClientError::InvalidInput));
        }
        assert!(validate_path("/api/history?limit=10&cursor=abc").is_ok());
    }
    #[test]
    fn original_bytes_keep_duplicate_field_rejection() {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Probe {
            count: u8,
        }
        assert_eq!(
            decode::<Probe>(br#"{"count":2}"#).ok().map(|p| p.count),
            Some(2)
        );
        assert!(decode::<Probe>(br#"{"count":1,"count":2}"#).is_err());
        assert!(decode::<Probe>(&vec![b' '; RESPONSE_MAX_BYTES + 1]).is_err());
    }
    #[test]
    fn management_guards_have_fixed_actionable_messages() {
        for (code, expected) in [
            ("hosted_game", ClientError::HostedGame),
            ("last_admin", ClientError::LastAdmin),
        ] {
            let body =
                serde_json::json!({"error": {"code": code, "message": "ignored server content"}})
                    .to_string();
            assert_eq!(response_error(409, body.as_bytes(), true), expected);
            assert_eq!(
                response_error(401, body.as_bytes(), true),
                ClientError::Unauthorized
            );
            assert!(!expected.to_string().contains("ignored server content"));
        }
    }

    #[test]
    fn native_transport_returns_unavailable_without_browser_execution() {
        use std::future::Future;
        let mut future = std::pin::pin!(get::<Value>("/api/session"));
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        assert!(matches!(
            future.as_mut().poll(&mut context),
            std::task::Poll::Ready(Err(ClientError::BrowserUnavailable))
        ));
    }
    #[test]
    fn errors_never_reflect_server_messages_or_payloads() {
        let error = response_error(
            401,
            br#"{"error":{"code":"invalid_credentials","message":"private-marker"}}"#,
            true,
        );
        assert_eq!(error, ClientError::InvalidCredentials);
        assert!(!format!("{error:?} {error}").contains("private-marker"));
        assert!(response_error(503, b"", true).uncertain());
        assert!(!response_error(429, b"", true).uncertain());
    }
}
