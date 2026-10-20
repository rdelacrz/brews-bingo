use super::{
    AuthAuthority, AuthorityClock, COOKIE_QUEUE_TIMEOUT_MS, ClientError, CommandClock, CookieQueue,
    CookieWriteGate, RESPONSE_MAX_BYTES, cookie_lane_required, decode, response_error,
};
use futures_util::{
    FutureExt,
    future::{AbortHandle, Abortable, select},
};
use js_sys::{Function, Promise, Reflect, Uint8Array};
use serde::de::DeserializeOwned;
use std::{
    cell::RefCell,
    rc::Rc,
    task::{Poll, Waker},
};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use wasm_bindgen_futures::{JsFuture, future_to_promise};
use zeroize::Zeroizing;

const REQUEST_TIMEOUT_MS: u32 = 30_000;
const COOKIE_LOCK_NAME: &str = "brews-account-cookie";

struct AbortOnDrop(Option<web_sys::AbortController>);
impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        if let Some(controller) = &self.0 {
            controller.abort();
        }
    }
}

thread_local! {
    static COMMAND_CLOCK: RefCell<Option<CommandClock>> = const { RefCell::new(None) };
    static AUTHORITY_CLOCK: RefCell<AuthorityClock> = RefCell::new(AuthorityClock::default());
    static COOKIE_GATE: RefCell<CookieWriteGate> = RefCell::new(CookieWriteGate::default());
    static COOKIE_LANE: Rc<futures_util::lock::Mutex<()>> = Rc::new(futures_util::lock::Mutex::new(()));
}

fn monotonic_ms() -> Result<f64, ClientError> {
    let window = web_sys::window().ok_or(ClientError::BrowserUnavailable)?;
    let performance = Reflect::get(window.as_ref(), &"performance".into())
        .map_err(|_| ClientError::BrowserUnavailable)?;
    call(&performance, "now")?
        .as_f64()
        .filter(|value| value.is_finite())
        .ok_or(ClientError::BrowserUnavailable)
}
fn calibrate(response: &web_sys::Response, sent_ms: f64) -> Result<(), ClientError> {
    let date = response
        .headers()
        .get("Date")
        .ok()
        .flatten()
        .ok_or(ClientError::Unavailable)?;
    let server_ms = js_sys::Date::parse(&date);
    if !server_ms.is_finite() || server_ms < 0.0 {
        return Err(ClientError::Unavailable);
    }
    let received_ms = monotonic_ms()?;
    AUTHORITY_CLOCK.with(|clock| {
        clock
            .borrow_mut()
            .calibrate(server_ms as i64, sent_ms, received_ms)
    })?;
    COMMAND_CLOCK.with(|clock| {
        *clock.borrow_mut() = Some(CommandClock {
            server_ms: server_ms as i64,
            monotonic_ms: received_ms,
        })
    });
    Ok(())
}
pub(super) fn now_ms() -> i64 {
    monotonic_ms()
        .and_then(|now| AUTHORITY_CLOCK.with(|clock| clock.borrow_mut().timestamp(now)))
        .unwrap_or(i64::MAX)
}
pub(super) async fn prepare_command_clock() -> Result<(), ClientError> {
    if COMMAND_CLOCK.with(|clock| clock.borrow().is_some()) && now_ms() != i64::MAX {
        return Ok(());
    }
    let _: serde_json::Value = send("GET", "/api/session", None, None, None).await?;
    COMMAND_CLOCK
        .with(|clock| clock.borrow().is_some())
        .then_some(())
        .filter(|_| now_ms() != i64::MAX)
        .ok_or(ClientError::Unavailable)
}

struct CookieRequest {
    method: String,
    path: String,
    body: Option<Zeroizing<String>>,
    command_id: Option<String>,
    authority: Option<AuthAuthority>,
}

type PendingCookieRequest = Rc<RefCell<CookieQueue<CookieRequest>>>;

#[derive(Clone)]
struct QueueCancellation {
    pending: PendingCookieRequest,
    lane: AbortHandle,
    lock: web_sys::AbortController,
}
impl QueueCancellation {
    fn cancel(&self) {
        self.pending.borrow_mut().invalidate();
        self.lane.abort();
        // Web Locks removes the pending callback when this signal is aborted.
        // Once granted, aborting this signal does not interrupt its HTTP request.
        self.lock.abort();
    }
}
struct CancelQueuedOnDrop(QueueCancellation);
impl Drop for CancelQueuedOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

pub(super) async fn send<T: DeserializeOwned>(
    method: &str,
    path: &str,
    body: Option<&str>,
    command_id: Option<&str>,
    authority: Option<AuthAuthority>,
) -> Result<T, ClientError> {
    let mutation = method != "GET";
    // Session revalidation can clear an invalid cookie, so it shares this lane.
    let bytes = if cookie_lane_required(method, path) {
        let logout = path == "/api/auth/logout";
        let generation = COOKIE_GATE.with(|gate| {
            let mut gate = gate.borrow_mut();
            if logout {
                gate.logout()
            } else {
                gate.generation
            }
        });
        let pending = Rc::new(RefCell::new(CookieQueue {
            pending: Some(CookieRequest {
                method: method.to_owned(),
                path: path.to_owned(),
                body: body.map(|body| Zeroizing::new(body.to_owned())),
                command_id: command_id.map(str::to_owned),
                authority,
            }),
            queued_ms: monotonic_ms()?,
        }));
        let (abort_lane, registration) = AbortHandle::new_pair();
        let cancellation = QueueCancellation {
            pending: pending.clone(),
            lane: abort_lane,
            lock: web_sys::AbortController::new().map_err(|_| ClientError::BrowserUnavailable)?,
        };
        let _cancel_on_drop = CancelQueuedOnDrop(cancellation.clone());
        let expired = cancellation.clone();
        // One deadline covers both the local queue and origin-wide lock wait.
        let timer = Rc::new(RefCell::new(Some(gloo_timers::callback::Timeout::new(
            COOKIE_QUEUE_TIMEOUT_MS,
            move || expired.cancel(),
        ))));
        let lane = COOKIE_LANE.with(Rc::clone);
        let delivery = Rc::new(RefCell::new((None, None::<Waker>)));
        let completed = delivery.clone();
        // Queued bodies are cleared by caller drop. Only an already dispatched
        // HTTP request keeps the lane and Web Lock until its response finishes.
        wasm_bindgen_futures::spawn_local(async move {
            let result = match Abortable::new(lane.lock(), registration).await {
                Ok(_guard) => {
                    send_with_cookie_lock(pending, cancellation.lock, timer, generation, logout)
                        .await
                }
                Err(_) => Err(ClientError::Busy),
            };
            if logout
                && result.as_ref().is_ok_and(|bytes| {
                    #[derive(serde::Deserialize)]
                    struct LoggedOut {
                        logged_out: bool,
                    }
                    decode::<LoggedOut>(bytes).is_ok_and(|reply| reply.logged_out)
                })
            {
                COOKIE_GATE.with(|gate| gate.borrow_mut().revoked(generation));
            }
            let mut completed = completed.borrow_mut();
            completed.0 = Some(result);
            if let Some(waker) = completed.1.take() {
                waker.wake();
            }
        });
        futures_util::future::poll_fn(move |cx| {
            let mut delivery = delivery.borrow_mut();
            if let Some(result) = delivery.0.take() {
                Poll::Ready(result)
            } else {
                delivery.1 = Some(cx.waker().clone());
                Poll::Pending
            }
        })
        .await?
    } else {
        send_bytes(method, path, body, command_id).await?
    };
    decode(&bytes).map_err(|error| {
        if mutation {
            ClientError::UnknownOutcome
        } else {
            error
        }
    })
}

fn cookie_request_current(pending: &PendingCookieRequest, generation: u64, logout: bool) -> bool {
    (logout || COOKIE_GATE.with(|gate| gate.borrow().permits(generation)))
        && pending.borrow().pending.as_ref().is_some_and(|request| {
            request
                .authority
                .as_ref()
                .is_none_or(|authority| (authority.current)())
        })
}

// The callback promise holds the same lock through preflight and completion.
async fn send_with_cookie_lock(
    pending: PendingCookieRequest,
    controller: web_sys::AbortController,
    timer: Rc<RefCell<Option<gloo_timers::callback::Timeout>>>,
    generation: u64,
    logout: bool,
) -> Result<Zeroizing<Vec<u8>>, ClientError> {
    let unavailable = |_| ClientError::BrowserUnavailable;
    let window = web_sys::window().ok_or(ClientError::BrowserUnavailable)?;
    if Reflect::get(window.as_ref(), &"isSecureContext".into())
        .map_err(unavailable)?
        .as_bool()
        != Some(true)
    {
        return Err(ClientError::BrowserUnavailable);
    }
    let locks = Reflect::get(window.navigator().as_ref(), &"locks".into()).map_err(unavailable)?;
    let request: Function = Reflect::get(&locks, &"request".into())
        .map_err(unavailable)?
        .dyn_into()
        .map_err(unavailable)?;
    let options = js_sys::Object::new();
    Reflect::set(&options, &"signal".into(), controller.signal().as_ref()).map_err(unavailable)?;
    let result = Rc::new(RefCell::new(None));
    let completed = result.clone();
    let callback = Closure::<dyn FnMut(JsValue) -> Promise>::new(move |lock: JsValue| {
        timer.borrow_mut().take();
        let pending = pending.clone();
        let completed = completed.clone();
        future_to_promise(async move {
            let response = async {
                if lock.is_null() || lock.is_undefined() {
                    return Err(ClientError::BrowserUnavailable);
                }
                pending.borrow_mut().acquired(monotonic_ms()?)?;
                if !cookie_request_current(&pending, generation, logout) {
                    pending.borrow_mut().invalidate();
                    return Err(ClientError::Busy);
                }
                let expected = pending
                    .borrow()
                    .pending
                    .as_ref()
                    .and_then(|request| request.authority.as_ref())
                    .and_then(|authority| authority.expected);
                if let Some(expected) = expected {
                    // Call the transport directly: reacquiring this lock would deadlock.
                    let bytes = send_bytes("GET", "/api/session", None, None).await?;
                    expected.validate(&bytes, now_ms())?;
                }
                // A dropped caller can let the preflight finish, but never send its password.
                let current = cookie_request_current(&pending, generation, logout);
                let request = pending.borrow_mut().take_if_current(current)?;
                send_bytes(
                    &request.method,
                    &request.path,
                    request.body.as_ref().map(|body| body.as_str()),
                    request.command_id.as_deref(),
                )
                .await
            }
            .await;
            pending.borrow_mut().invalidate();
            *completed.borrow_mut() = Some(response);
            Ok(JsValue::UNDEFINED)
        })
    });
    let promise: Promise = request
        .call3(
            &locks,
            &COOKIE_LOCK_NAME.into(),
            &options,
            callback.as_ref(),
        )
        .map_err(unavailable)?
        .dyn_into()
        .map_err(unavailable)?;
    // Await the aborted lock request too, so its JS callback is removed before
    // dropping the Rust Closure. A race with a timeout alone cannot do this.
    JsFuture::from(promise).await.map_err(|_| {
        if controller.signal().aborted() {
            ClientError::Busy
        } else {
            ClientError::BrowserUnavailable
        }
    })?;
    result
        .borrow_mut()
        .take()
        .ok_or(ClientError::BrowserUnavailable)?
}

async fn send_bytes(
    method: &str,
    path: &str,
    body: Option<&str>,
    command_id: Option<&str>,
) -> Result<Zeroizing<Vec<u8>>, ClientError> {
    let window = web_sys::window().ok_or(ClientError::BrowserUnavailable)?;
    if !window.navigator().on_line() {
        return Err(ClientError::Offline);
    }
    let mutation = method != "GET";
    let controller =
        web_sys::AbortController::new().map_err(|_| ClientError::BrowserUnavailable)?;
    let mut abort = AbortOnDrop(Some(controller.clone()));
    let options = web_sys::RequestInit::new();
    options.set_method(method);
    options.set_credentials(web_sys::RequestCredentials::Include);
    options.set_cache(web_sys::RequestCache::NoStore);
    options.set_signal(Some(&controller.signal()));
    // Reject redirects before a request can disclose any body to another endpoint.
    Reflect::set(options.as_ref(), &"redirect".into(), &"error".into())
        .map_err(|_| ClientError::BrowserUnavailable)?;
    if mutation
        && path != "/api/auth/logout"
        && let Some(body) = body.filter(|body| *body != "null")
    {
        options.set_body(&JsValue::from_str(body));
    }
    let request = web_sys::Request::new_with_str_and_init(path, &options)
        .map_err(|_| ClientError::InvalidInput)?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|_| ClientError::BrowserUnavailable)?;
    if mutation && path != "/api/auth/logout" && body.is_some_and(|body| body != "null") {
        request
            .headers()
            .set("Content-Type", "application/json")
            .map_err(|_| ClientError::BrowserUnavailable)?;
    }
    if let Some(id) = command_id {
        request
            .headers()
            .set("Idempotency-Key", id)
            .map_err(|_| ClientError::InvalidInput)?;
    }
    let sent_ms = monotonic_ms()?;
    let operation = async {
        let value = JsFuture::from(window.fetch_with_request(&request))
            .await
            .map_err(|_| transport_error(mutation))?;
        let response: web_sys::Response =
            value.dyn_into().map_err(|_| transport_error(mutation))?;
        // Session authority always requires its explicit server clock header.
        let calibrated = calibrate(&response, sent_ms);
        if path == "/api/session" {
            calibrated?;
        }
        let status = response.status();
        let media = response.headers().get("Content-Type").ok().flatten();
        if !media.is_some_and(|media| {
            media
                .split(';')
                .next()
                .is_some_and(|m| m.trim().eq_ignore_ascii_case("application/json"))
        }) {
            return Err(transport_error(mutation));
        }
        let bytes = read_bounded(&response)
            .await
            .map_err(|_| transport_error(mutation))?;
        if !(200..300).contains(&status) {
            return Err(response_error(status, &bytes, mutation));
        }
        Ok(bytes)
    }
    .boxed_local();
    let timeout = gloo_timers::future::TimeoutFuture::new(REQUEST_TIMEOUT_MS).boxed_local();
    let result = match select(operation, timeout).await {
        futures_util::future::Either::Left((result, _)) => result,
        futures_util::future::Either::Right(_) => Err(transport_error(mutation)),
    };
    if result.is_ok() {
        abort.0 = None;
    }
    result
}
fn transport_error(mutation: bool) -> ClientError {
    if mutation {
        ClientError::UnknownOutcome
    } else {
        ClientError::Unavailable
    }
}

// Read chunks before allocating Rust storage; do not buffer an unbounded body via text()/json().
async fn read_bounded(response: &web_sys::Response) -> Result<Zeroizing<Vec<u8>>, ClientError> {
    if response
        .headers()
        .get("Content-Length")
        .ok()
        .flatten()
        .and_then(|s| s.parse::<usize>().ok())
        .is_some_and(|size| size > RESPONSE_MAX_BYTES)
    {
        return Err(ClientError::TooLarge);
    }
    let stream = Reflect::get(response.as_ref(), &"body".into())
        .map_err(|_| ClientError::InvalidResponse)?;
    let reader = call(&stream, "getReader")?;
    let mut bytes = Zeroizing::new(Vec::new());
    loop {
        let promise: Promise = call(&reader, "read")?
            .dyn_into()
            .map_err(|_| ClientError::InvalidResponse)?;
        let chunk = JsFuture::from(promise)
            .await
            .map_err(|_| ClientError::InvalidResponse)?;
        if Reflect::get(&chunk, &"done".into())
            .ok()
            .and_then(|v| v.as_bool())
            == Some(true)
        {
            break;
        }
        let value =
            Reflect::get(&chunk, &"value".into()).map_err(|_| ClientError::InvalidResponse)?;
        let values: Uint8Array = value.dyn_into().map_err(|_| ClientError::InvalidResponse)?;
        let len = values.length() as usize;
        if len > RESPONSE_MAX_BYTES.saturating_sub(bytes.len()) {
            let _ = call(&reader, "cancel");
            return Err(ClientError::TooLarge);
        }
        let start = bytes.len();
        bytes.resize(start + len, 0);
        values.copy_to(&mut bytes[start..]);
    }
    Ok(bytes)
}
fn call(target: &JsValue, name: &str) -> Result<JsValue, ClientError> {
    let function: Function = Reflect::get(target, &JsValue::from_str(name))
        .map_err(|_| ClientError::InvalidResponse)?
        .dyn_into()
        .map_err(|_| ClientError::InvalidResponse)?;
    function
        .call0(target)
        .map_err(|_| ClientError::InvalidResponse)
}

pub(super) fn new_command_id() -> Result<String, ClientError> {
    let window = web_sys::window().ok_or(ClientError::BrowserUnavailable)?;
    let mut bytes = [0u8; 16];
    window
        .crypto()
        .map_err(|_| ClientError::BrowserUnavailable)?
        .get_random_values_with_u8_array(&mut bytes)
        .map_err(|_| ClientError::BrowserUnavailable)?;
    let now = COMMAND_CLOCK.with(|clock| {
        clock
            .borrow()
            .ok_or(ClientError::Unavailable)?
            .timestamp(monotonic_ms()?)
    })?;
    if !(0..=0xffff_ffff_ffff).contains(&now) {
        return Err(ClientError::BrowserUnavailable);
    }
    let stamp = now.to_be_bytes();
    bytes[..6].copy_from_slice(&stamp[2..]);
    bytes[6] = (bytes[6] & 0x0f) | 0x70;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Ok(uuid::Uuid::from_bytes(bytes).to_string())
}
pub(super) fn take_link_fragment() -> Result<Option<String>, ClientError> {
    let window = web_sys::window().ok_or(ClientError::BrowserUnavailable)?;
    let location = window.location();
    let fragment = Zeroizing::new(
        location
            .hash()
            .map_err(|_| ClientError::BrowserUnavailable)?,
    );
    let clean = format!(
        "{}{}",
        location
            .pathname()
            .map_err(|_| ClientError::BrowserUnavailable)?,
        location
            .search()
            .map_err(|_| ClientError::BrowserUnavailable)?
    );
    window
        .history()
        .map_err(|_| ClientError::BrowserUnavailable)?
        .replace_state_with_url(&JsValue::NULL, "", Some(&clean))
        .map_err(|_| ClientError::BrowserUnavailable)?;
    let token = fragment.strip_prefix('#').unwrap_or(&fragment);
    if token.is_empty() {
        return Ok(None);
    }
    // Canonical issued tokens are unpadded base64url, not a query string.
    if token.len() > 128
        || !token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    {
        return Err(ClientError::InvalidLink);
    }
    Ok(Some(token.to_owned()))
}
pub(super) async fn copy_text(text: &str) -> Result<(), ClientError> {
    let window = web_sys::window().ok_or(ClientError::ClipboardUnavailable)?;
    JsFuture::from(window.navigator().clipboard().write_text(text))
        .await
        .map_err(|_| ClientError::ClipboardUnavailable)?;
    Ok(())
}
