use super::{
    input::SecretBody,
    wire::{AuthReply, Operation},
};
use crate::{
    client::{self, ClientError},
    components::{Keyed, Loading, Notice, PageTitle, PasswordTextField},
    session::{AccountSession, ExpectedRestrictedSession},
};
use brews_domain::accounts::{SessionScope, validate_password};
use dioxus::prelude::*;
use zeroize::{Zeroize, Zeroizing};

type LinkFragment = Result<Option<Zeroizing<String>>, ClientError>;

struct InitialLink {
    reset: bool,
    fragment: LinkFragment,
}

pub(crate) fn provide_initial_link() {
    // Router canonicalization removes unmodeled fragments before page mounting.
    let initial = use_signal(|| {
        #[cfg(target_arch = "wasm32")]
        {
            let path = web_sys::window()?.location().pathname().ok()?;
            if !matches!(path.as_str(), "/enroll" | "/password-reset") {
                return None;
            }
            Some(InitialLink {
                reset: path == "/password-reset",
                fragment: client::take_link_fragment().map(|value| value.map(Zeroizing::new)),
            })
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            None::<InitialLink>
        }
    });
    use_context_provider(|| initial);
}

struct LinkRedemption {
    generation: u64,
    pending: Option<LinkFragment>,
}
impl LinkRedemption {
    fn new(initial: LinkFragment) -> Self {
        Self {
            generation: 0,
            pending: Some(initial),
        }
    }
    #[cfg(any(target_arch = "wasm32", test))]
    fn capture(&mut self, next: LinkFragment) -> bool {
        if matches!(next, Ok(None)) {
            return false;
        }
        self.generation = self.generation.wrapping_add(1);
        self.pending = Some(next);
        true
    }
    fn take(&mut self) -> Option<(u64, LinkFragment)> {
        self.pending.take().map(|token| (self.generation, token))
    }
    fn current(&self, generation: u64) -> bool {
        self.generation == generation
    }
}

#[component]
pub fn Enrollment() -> Element {
    rsx! { PasswordFlow { reset: false } }
}
#[component]
pub fn PasswordReset() -> Element {
    rsx! { PasswordFlow { reset: true } }
}

#[component]
fn PasswordFlow(reset: bool) -> Element {
    let account = use_context::<AccountSession>();
    let mut initial = use_context::<Signal<Option<InitialLink>>>();
    // Extraction and history replacement are synchronous and precede every await.
    let mut links = use_signal(|| {
        LinkRedemption::new(
            initial
                .write()
                .take()
                .filter(|link| link.reset == reset)
                .map_or_else(
                    || client::take_link_fragment().map(|value| value.map(Zeroizing::new)),
                    |link| link.fragment,
                ),
        )
    });
    let mut checking = use_signal(|| true);
    let mut blocked = use_signal(|| false);
    let mut message = use_signal(|| None::<String>);
    let mut completed = use_signal(|| false);
    let scope = if reset {
        SessionScope::PasswordResetOnly
    } else {
        SessionScope::EnrollmentOnly
    };
    #[cfg(target_arch = "wasm32")]
    use_hook(move || {
        browser::watch_fragments(
            if reset { "/password-reset" } else { "/enroll" },
            move || {
                // Scrub inside the navigation callback, before scheduling redemption.
                let next = client::take_link_fragment().map(|value| value.map(Zeroizing::new));
                if !links.write().capture(next) {
                    return;
                }
                account.clear();
                checking.set(true);
                blocked.set(false);
                completed.set(false);
                message.set(None);
            },
        )
    });
    use_effect(move || {
        if links.read().pending.is_none() {
            return;
        }
        let Some((ticket, token)) = links.write().take() else {
            return;
        };
        spawn(async move {
            if !links.read().current(ticket) {
                return;
            }
            match token {
                Ok(Some(token)) => {
                    let submitted_epoch = match account.begin_authentication() {
                        Ok(epoch) => epoch,
                        Err(error) => {
                            blocked.set(true);
                            message.set(Some(error.to_string()));
                            checking.set(false);
                            return;
                        }
                    };
                    let result = async {
                        client::prepare_command_clock().await?;
                        if (account.epoch)() != submitted_epoch || !links.read().current(ticket) {
                            return Err(ClientError::Busy);
                        }
                        let id = client::new_command_id()?;
                        let body = SecretBody(if reset {
                            serde_json::json!({"reset_token":token.as_str()})
                        } else {
                            serde_json::json!({"enrollment_token":token.as_str()})
                        });
                        let path = if reset {
                            "/api/auth/password-reset/redeem"
                        } else {
                            "/api/auth/enrollment/redeem"
                        };
                        let result = client::auth_request::<AuthReply>(
                            path,
                            &body.0,
                            &id,
                            None,
                            move || {
                                account.authentication_current(submitted_epoch)
                                    && links.try_read().is_ok_and(|links| links.current(ticket))
                            },
                        )
                        .await;
                        drop(body);
                        result.map(|reply| (id, reply))
                    }
                    .await;
                    drop(token);
                    if !links.read().current(ticket) {
                        return;
                    }
                    if (account.epoch)() != submitted_epoch {
                        checking.set(false);
                        return;
                    }
                    if super::needs_session_recheck(&result) {
                        account.refresh().await;
                    }
                    if !links.read().current(ticket) {
                        return;
                    }
                    let recovered = account.session.read().as_ref().is_some_and(|session| {
                        session.scope == scope && session.expires_at > client::now_ms()
                    });
                    match result {
                        Ok((id, reply)) => {
                            let expected = reply.account_id();
                            let matches_account = expected.is_some()
                                && account.session.read().as_ref().map(|s| s.account_id)
                                    == expected;
                            if !recovered
                                || !matches_account
                                || !(reply.restricted_reply(reset)
                                    || reply.replay(
                                        Operation::Redeem,
                                        &id,
                                        expected,
                                        client::now_ms(),
                                    ))
                            {
                                blocked.set(true);
                                message.set(Some("A usable restricted session was not confirmed. Ask an authorized issuer for a fresh link. A receipt cannot replay the session cookie.".to_owned()));
                            }
                        }
                        Err(error) if error.uncertain() && recovered => {
                            message.set(Some("The link response was lost. The existing restricted session was revalidated. Check the account shown below before continuing.".to_owned()));
                        }
                        Err(error) => {
                            blocked.set(true);
                            message.set(Some(if error.uncertain() {
                            "The link result is unknown and no restricted session was recovered. Ask an authorized issuer for a fresh link; this page will not redeem the token again.".to_owned()
                        } else { error.to_string() }));
                        }
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    blocked.set(true);
                    message.set(Some(error.to_string()));
                }
            }
            if links.read().current(ticket) {
                checking.set(false);
            }
        });
    });
    let form_epoch = (account.epoch)();
    let restricted =
        (account.ready)()
            && account.session.read().as_ref().is_some_and(|session| {
                session.scope == scope && session.expires_at > client::now_ms()
            });
    rsx! {
        PageTitle { eyebrow: "Private account access", title: if reset { "Set a new password" } else { "Make yourself at home" }, description: if reset { "Complete the password reset authorized for your account." } else { "Finish setting up your provisioned host or admin account." } }
        section { class: "card stack",
            if let Some(text) = message() { Notice { message: text } }
            if checking() || !(account.ready)() { Loading {} }
            else if completed() { Link { class: "button", to: "/login", "Continue to login" } }
            else if restricted && !blocked() {
              Keyed { identity: format!("authority-{form_epoch}"), div { class: "stack",
                if let Some(identity) = account.account.read().as_ref() { p { "Account: {crate::components::visible_text(&identity.username)}" } }
                if reset { p { "The old password is unavailable while reset is pending. An expired reset link does not restore it." } }
                PasswordForm { reset, oncomplete: move |_| completed.set(true), onmessage: move |text| message.set(Some(text)) }
              } }
            } else if !blocked() && account.normal() && !reset {
                p { "Your account is already set up and signed in." }
                Link { class: "button", to: "/games", "Open Games" }
            } else if !blocked() {
                Notice { message: "No eligible restricted session is available. Open a fresh private link supplied by an authorized issuer. If you already completed setup or reset, log in with your password." }
            }
            Link { to: "/login", "Go to login" }
        }
    }
}

#[component]
fn PasswordForm(
    reset: bool,
    oncomplete: EventHandler<()>,
    onmessage: EventHandler<String>,
) -> Element {
    let account = use_context::<AccountSession>();
    let navigate = use_navigator();
    let mut password = use_signal(|| Zeroizing::new(String::new()));
    let mut confirmation = use_signal(|| Zeroizing::new(String::new()));
    let show_password = use_signal(|| false);
    let show_confirmation = use_signal(|| false);
    let mut busy = use_signal(|| false);
    let mut stopped = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    use_drop(move || {
        if let Ok(mut password) = password.try_write() {
            password.zeroize();
        }
        if let Ok(mut confirmation) = confirmation.try_write() {
            confirmation.zeroize();
        }
    });
    let binding = account.session.read().as_ref().and_then(|session| {
        ExpectedRestrictedSession::new(
            session,
            if reset {
                SessionScope::PasswordResetOnly
            } else {
                SessionScope::EnrollmentOnly
            },
            client::now_ms(),
        )
    });
    let permitted = (account.ready)() && binding.is_some();
    rsx! {
        if let Some(text) = error() { Notice { message: text } }
        form { class: "form", onsubmit: move |event| {
            event.prevent_default();
            if busy() || stopped() || !permitted { return; }
            let candidate = (|| {
                let candidate = Zeroizing::new(password.read().to_string());
                validate_password(&candidate).map_err(|_| ClientError::InvalidInput)?;
                if candidate.as_str() != confirmation.read().as_str() { return Err(ClientError::InvalidInput); }
                Ok::<_, ClientError>(candidate)
            })();
            let candidate = match candidate { Ok(candidate) => candidate, Err(_) => { error.set(Some("Enter matching passwords of 10–50 ASCII characters. Spaces and control characters count and are preserved.".to_owned())); return; } };
            let Some(binding) = binding else { return; };
            let expected = Some(binding.account_id);
            let submitted_epoch = (account.epoch)();
            let body = SecretBody(serde_json::json!({"new_password":candidate.as_str()}));
            busy.set(true);
            error.set(None);
            password.write().zeroize();
            confirmation.write().zeroize();
            spawn(async move {
                let path = if reset { "/api/auth/password-reset/complete" } else { "/api/auth/enrollment/complete" };
                let result = async {
                    client::prepare_command_clock().await?;
                    if (account.epoch)() != submitted_epoch || !(account.ready)() { return Err(ClientError::Busy); }
                    let id = client::new_command_id()?;
                    let reply = client::auth_request::<AuthReply>(path, &body.0, &id, Some(binding),
                        move || account.authentication_current(submitted_epoch)
                            && account.ready.try_read().is_ok_and(|ready| *ready)).await?;
                    Ok::<_, ClientError>((id, reply))
                }.await;
                drop(body);
                drop(candidate);
                if (account.epoch)() != submitted_epoch { busy.set(false); return; }
                match result {
                    Ok((id, reply)) if reset && (reply.reset_reply() || reply.replay(Operation::Reset, &id, expected, client::now_ms())) => {
                        // No login request or normal-session creation follows password reset.
                        account.clear();
                        onmessage.call("Password reset completed. Log in with your new password.".to_owned());
                        oncomplete.call(());
                    }
                    Ok((id, reply)) if !reset && (reply.normal_reply() || reply.replay(Operation::Enrollment, &id, expected, client::now_ms())) => {
                        let reply_id = reply.account_id();
                        account.refresh().await;
                        if account.normal() && expected.is_some() && expected == reply_id && account.account.read().as_ref().map(|a| a.account_id) == expected {
                            navigate.push("/games");
                        } else {
                            stopped.set(true);
                            onmessage.call("Setup committed, but a usable normal session was not recovered. Log in with the password you just set; the completion receipt cannot reissue a cookie.".to_owned());
                        }
                    }
                    Ok(_) => { stopped.set(true); onmessage.call("The response could not confirm completion. Try password login, or ask an authorized issuer for a successor link if setup is still required.".to_owned()); }
                    Err(failure) if failure.uncertain() => {
                        stopped.set(true);
                        onmessage.call("The completion result is unknown. Your entered passwords have been cleared. Try login with the new password. If setup or reset is still required, ask an authorized issuer for a successor link; do not repeat completion blindly.".to_owned());
                    }
                    Err(failure) => {
                        error.set(Some(failure.to_string()));
                        if failure.unauthorized() {
                            stopped.set(true);
                            password.write().zeroize();
                            confirmation.write().zeroize();
                            account.clear();
                        }
                    }
                }
                busy.set(false);
            });
        },
            PasswordTextField { input_id: "new-password", input_name: "new-password", label: "New password", autocomplete: "new-password", password, visible: show_password, disabled: busy() || stopped() || !permitted }
                small { "10-50 characters" }
            PasswordTextField { input_id: "confirm-password", input_name: "confirm-password", label: "Confirm password", autocomplete: "new-password", password: confirmation, visible: show_confirmation, disabled: busy() || stopped() || !permitted }
            button { class: "button", r#type: "submit", disabled: busy() || stopped() || !permitted, if busy() { "Saving password…" } else if reset { "Set new password" } else { "Set password" } }
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod browser {
    use wasm_bindgen::{JsCast, closure::Closure};
    pub(super) struct FragmentListener {
        window: web_sys::Window,
        callback: Closure<dyn FnMut(web_sys::Event)>,
    }
    impl Drop for FragmentListener {
        fn drop(&mut self) {
            for event in ["hashchange", "popstate"] {
                let _ = self.window.remove_event_listener_with_callback(
                    event,
                    self.callback.as_ref().unchecked_ref(),
                );
            }
        }
    }
    pub(super) fn watch_fragments(
        path: &'static str,
        mut capture: impl FnMut() + 'static,
    ) -> Option<std::rc::Rc<FragmentListener>> {
        let window = web_sys::window()?;
        let observed = window.clone();
        let callback = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            if observed.location().pathname().ok().as_deref() == Some(path) {
                capture();
            }
        });
        let listener = FragmentListener { window, callback };
        for event in ["hashchange", "popstate"] {
            listener
                .window
                .add_event_listener_with_callback(event, listener.callback.as_ref().unchecked_ref())
                .ok()?;
        }
        Some(std::rc::Rc::new(listener))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "Fixed test fixtures.")]
mod tests {
    use super::*;
    #[test]
    fn successor_navigation_consumes_once_and_fences_late_results() {
        let mut links = LinkRedemption::new(Ok(None));
        let (initial, _) = links.take().unwrap();
        assert!(links.take().is_none());
        assert!(links.capture(Ok(Some(Zeroizing::new("first".to_owned())))));
        let (first, token) = links.take().unwrap();
        assert!(matches!(token, Ok(Some(_))));
        assert!(!links.current(initial));
        assert!(links.current(first));
        assert!(links.take().is_none());
        assert!(!links.capture(Ok(None)));
        assert!(links.current(first));
        assert!(links.capture(Ok(Some(Zeroizing::new("successor".to_owned())))));
        assert!(!links.current(first));
        assert!(links.take().is_some());
        assert!(links.take().is_none());
    }
    #[test]
    fn invalid_successor_is_consumed_without_reusing_an_old_token() {
        let mut links = LinkRedemption::new(Ok(Some(Zeroizing::new("superseded".to_owned()))));
        assert!(links.capture(Err(ClientError::InvalidLink)));
        let (_, token) = links.take().unwrap();
        assert!(matches!(token, Err(ClientError::InvalidLink)));
        assert!(links.take().is_none());
    }
}
