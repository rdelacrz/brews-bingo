//! Account authority only; anonymous game membership has a separate owner.
use crate::client::{self, ClientError};
use brews_contracts::{SafeAccount, SessionView};
use brews_domain::accounts::{AccountRole, AccountStatus, SessionScope};
use dioxus::prelude::*;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CurrentSession {
    authenticated: bool,
    #[serde(default, deserialize_with = "client::optional_object")]
    account: Option<SafeAccount>,
    #[serde(deserialize_with = "client::optional_object")]
    session: Option<SessionView>,
}

/// The restricted authority displayed by a password form, including its fixed deadline.
#[derive(Clone, Copy)]
#[cfg_attr(
    not(any(target_arch = "wasm32", test)),
    allow(
        dead_code,
        reason = "Browser preflight validates the captured scope and deadline."
    )
)]
pub(crate) struct ExpectedRestrictedSession {
    pub(crate) account_id: brews_domain::ids::AccountId,
    scope: SessionScope,
    expires_at: i64,
}
impl ExpectedRestrictedSession {
    pub(crate) fn new(session: &SessionView, scope: SessionScope, now: i64) -> Option<Self> {
        (matches!(
            scope,
            SessionScope::EnrollmentOnly | SessionScope::PasswordResetOnly
        ) && session.scope == scope
            && session.expires_at > now)
            .then_some(Self {
                account_id: session.account_id,
                scope,
                expires_at: session.expires_at,
            })
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn validate(self, bytes: &[u8], now: i64) -> Result<(), ClientError> {
        let current: CurrentSession = client::decode(bytes)?;
        match (current.authenticated, current.account, current.session) {
            (true, Some(account), Some(session))
                if eligible(&account, &session, now)
                    && session.account_id == self.account_id
                    && session.scope == self.scope
                    && session.expires_at == self.expires_at =>
            {
                Ok(())
            }
            _ => Err(ClientError::Unauthorized),
        }
    }
}

#[derive(Clone, Copy)]
pub struct AccountSession {
    pub account: Signal<Option<SafeAccount>>,
    pub session: Signal<Option<SessionView>>,
    pub ready: Signal<bool>,
    pub epoch: Signal<u64>,
    generation: Signal<u64>,
    logging_out: Signal<bool>,
    validation_error: Signal<Option<ClientError>>,
}
impl AccountSession {
    pub fn normal(&self) -> bool {
        if !*self.ready.read() || *self.logging_out.read() {
            return false;
        }
        let account = self.account.read();
        let session = self.session.read();
        matches!((account.as_ref(), session.as_ref()), (Some(account), Some(session))
            if eligible(account, session, client::now_ms()) && session.scope == SessionScope::Normal && account.status == AccountStatus::Verified)
    }
    pub fn admin(&self) -> bool {
        self.normal()
            && self
                .account
                .read()
                .as_ref()
                .is_some_and(|a| a.role == AccountRole::Admin)
    }
    pub fn failure(&self) -> Option<ClientError> {
        *self.validation_error.read()
    }
    pub async fn refresh(mut self) {
        if self
            .logging_out
            .try_read()
            .map(|value| *value)
            .unwrap_or(true)
        {
            return;
        }
        let ticket = match self.generation.try_write() {
            Ok(mut generation) => {
                *generation = generation.wrapping_add(1);
                *generation
            }
            Err(_) => return,
        };
        let result = client::get::<CurrentSession>("/api/session").await;
        if self
            .generation
            .try_read()
            .map(|generation| *generation != ticket)
            .unwrap_or(true)
        {
            return;
        }
        match result {
            Ok(CurrentSession {
                authenticated: true,
                account: Some(account),
                session: Some(session),
            }) if eligible(&account, &session, client::now_ms()) => {
                let changed = self.session.read().as_ref().is_none_or(|previous| {
                    previous.account_id != session.account_id
                        || previous.scope != session.scope
                        || previous.expires_at != session.expires_at
                        || previous.role != session.role
                });
                if changed {
                    self.bump_epoch();
                }
                self.account.set(Some(account));
                self.session.set(Some(session));
                self.validation_error.set(None);
                self.ready.set(true);
            }
            Ok(CurrentSession {
                authenticated: false,
                account: None,
                session: None,
            }) => self.revoke(false),
            Ok(_) => {
                self.revoke(false);
                self.validation_error
                    .set(Some(ClientError::InvalidResponse));
            }
            Err(error) => {
                self.revoke(false);
                self.validation_error.set(Some(error));
            }
        }
    }
    pub fn clear(self) {
        self.revoke(true);
    }
    fn revoke(mut self, force_epoch: bool) {
        if let Ok(mut error) = self.validation_error.try_write() {
            *error = None;
        }
        if let Ok(mut generation) = self.generation.try_write() {
            *generation = generation.wrapping_add(1);
        }
        if force_epoch
            || self
                .account
                .try_read()
                .map(|a| a.is_some())
                .unwrap_or(false)
            || self
                .session
                .try_read()
                .map(|s| s.is_some())
                .unwrap_or(false)
        {
            self.bump_epoch();
        }
        if let Ok(mut ready) = self.ready.try_write() {
            *ready = false;
        }
        let account_cleared = if let Ok(mut account) = self.account.try_write() {
            *account = None;
            true
        } else {
            false
        };
        let session_cleared = if let Ok(mut session) = self.session.try_write() {
            *session = None;
            true
        } else {
            false
        };
        if let Ok(mut ready) = self.ready.try_write() {
            *ready = account_cleared && session_cleared;
        }
    }
    pub(crate) fn authentication_current(self, epoch: u64) -> bool {
        self.epoch.try_read().is_ok_and(|current| *current == epoch)
            && self
                .logging_out
                .try_read()
                .is_ok_and(|logging_out| !*logging_out)
    }
    pub(crate) fn begin_authentication(mut self) -> Result<u64, ClientError> {
        if *self.logging_out.read() {
            return Err(ClientError::Busy);
        }
        self.ready.set(false);
        self.generation
            .with_mut(|generation| *generation = generation.wrapping_add(1));
        self.bump_epoch();
        Ok((self.epoch)())
    }
    fn bump_epoch(mut self) {
        if let Ok(mut epoch) = self.epoch.try_write() {
            *epoch = epoch.wrapping_add(1);
        }
    }
}
fn eligible(account: &SafeAccount, session: &SessionView, now: i64) -> bool {
    !account.disabled
        && account.account_id == session.account_id
        && account.role == session.role
        && session.expires_at > now
        && matches!(
            (session.scope, account.status),
            (SessionScope::Normal, AccountStatus::Verified)
                | (
                    SessionScope::EnrollmentOnly,
                    AccountStatus::PendingEnrollment
                )
                | (
                    SessionScope::PasswordResetOnly,
                    AccountStatus::ResetRequired
                )
        )
}

#[cfg(target_arch = "wasm32")]
const ACCOUNT_SESSION_MAX_MS: i64 = 86_400_000;

pub(crate) fn provide_account_session() -> AccountSession {
    let account = use_signal(|| None);
    let session = use_signal(|| None);
    let ready = use_signal(|| false);
    let epoch = use_signal(|| 0);
    let generation = use_signal(|| 0);
    let logging_out = use_signal(|| false);
    let validation_error = use_signal(|| None);
    let context = use_context_provider(|| AccountSession {
        account,
        session,
        ready,
        epoch,
        generation,
        logging_out,
        validation_error,
    });
    use_future(move || async move {
        context.refresh().await;
    });
    #[cfg(target_arch = "wasm32")]
    {
        use_hook(move || browser::install_visibility_refresh(context));
        let mut expiry_task = use_signal(|| None::<dioxus::core::Task>);
        let mut expiry_target = use_signal(|| None::<(u64, i64)>);
        use_effect(move || {
            let deadline = context
                .session
                .read()
                .as_ref()
                .map(|session| session.expires_at);
            let epoch = (context.epoch)();
            let target = deadline.map(|deadline| (epoch, deadline));
            if *expiry_target.peek() == target {
                return;
            }
            expiry_target.set(target);
            if let Some(task) = expiry_task.write().take() {
                task.cancel();
            }
            if let Some(deadline) = deadline {
                // Keep the first timer for this authority. Refreshes and wall-clock
                // rollback cannot move its original local expiry into the future.
                let wait = deadline
                    .saturating_sub(client::now_ms())
                    .clamp(0, ACCOUNT_SESSION_MAX_MS) as u32;
                expiry_task.set(Some(spawn(async move {
                    gloo_timers::future::TimeoutFuture::new(wait).await;
                    if (context.epoch)() == epoch {
                        context.clear();
                    }
                })));
            }
        });
        use_future(move || async move {
            loop {
                let deadline = context
                    .session
                    .read()
                    .as_ref()
                    .map(|session| session.expires_at);
                let wait = deadline
                    .map(|deadline| {
                        deadline.saturating_sub(client::now_ms()).clamp(1, 15_000) as u32
                    })
                    .unwrap_or(15_000);
                gloo_timers::future::TimeoutFuture::new(wait).await;
                if context
                    .session
                    .read()
                    .as_ref()
                    .is_some_and(|s| s.expires_at <= client::now_ms())
                {
                    context.clear();
                }
                if web_sys::window()
                    .and_then(|w| w.document())
                    .is_some_and(|d| d.visibility_state() == web_sys::VisibilityState::Visible)
                {
                    context.refresh().await;
                }
            }
        });
    }
    context
}

pub async fn logout(mut session: AccountSession) -> Result<(), ClientError> {
    #[derive(Deserialize)]
    struct LoggedOut {
        logged_out: bool,
    }
    // Hide protected content immediately, even when server revocation needs retry.
    session.logging_out.set(true);
    session.clear();
    let result: LoggedOut =
        client::request("POST", "/api/auth/logout", &serde_json::Value::Null, None).await?;
    if result.logged_out {
        session.logging_out.set(false);
        Ok(())
    } else {
        Err(ClientError::UnknownOutcome)
    }
}

#[cfg(target_arch = "wasm32")]
mod browser {
    use super::AccountSession;
    use dioxus::prelude::*;
    use wasm_bindgen::{JsCast, closure::Closure};
    pub(super) struct VisibilityListener {
        document: web_sys::Document,
        callback: Closure<dyn FnMut(web_sys::Event)>,
    }
    impl Drop for VisibilityListener {
        fn drop(&mut self) {
            let _ = self.document.remove_event_listener_with_callback(
                "visibilitychange",
                self.callback.as_ref().unchecked_ref(),
            );
        }
    }
    pub(super) fn install_visibility_refresh(
        session: AccountSession,
    ) -> Option<std::rc::Rc<VisibilityListener>> {
        let document = web_sys::window().and_then(|window| window.document())?;
        let observed = document.clone();
        let callback = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            // Apply local expiry before waiting on a visible-tab network refresh.
            if session
                .session
                .read()
                .as_ref()
                .is_some_and(|authority| authority.expires_at <= crate::client::now_ms())
            {
                session.clear();
            }
            if observed.visibility_state() == web_sys::VisibilityState::Visible {
                wasm_bindgen_futures::spawn_local(async move {
                    session.refresh().await;
                });
            }
        });
        document
            .add_event_listener_with_callback("visibilitychange", callback.as_ref().unchecked_ref())
            .ok()?;
        Some(std::rc::Rc::new(VisibilityListener { document, callback }))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (SafeAccount, SessionView) {
        let id = brews_domain::ids::AccountId::try_from(uuid::Uuid::from_bytes([
            0, 0, 0, 0, 0, 1, 0x70, 0, 0x80, 0, 0, 0, 0, 0, 0, 1,
        ]));
        let Ok(id) = id else {
            panic!("fixture id");
        };
        (
            SafeAccount {
                account_id: id,
                username: "ExampleHost".to_owned(),
                role: AccountRole::Host,
                status: AccountStatus::Verified,
                disabled: false,
                created_at: 1,
            },
            SessionView {
                kind: brews_contracts::SessionKind::Account,
                scope: SessionScope::Normal,
                expires_at: 100,
                account_id: id,
                role: AccountRole::Host,
            },
        )
    }
    fn restricted_fixture(scope: SessionScope) -> (SafeAccount, SessionView) {
        let (mut account, mut session) = fixture();
        session.scope = scope;
        account.status = match scope {
            SessionScope::EnrollmentOnly => AccountStatus::PendingEnrollment,
            SessionScope::PasswordResetOnly => AccountStatus::ResetRequired,
            SessionScope::Normal => AccountStatus::Verified,
        };
        (account, session)
    }
    fn session_bytes(account: &SafeAccount, session: &SessionView) -> Vec<u8> {
        serde_json::json!({ "authenticated": true, "account": account, "session": session })
            .to_string()
            .into_bytes()
    }
    #[test]
    fn password_binding_rejects_another_accounts_cookie_before_completion() {
        for scope in [
            SessionScope::EnrollmentOnly,
            SessionScope::PasswordResetOnly,
        ] {
            let (mut account, mut session) = restricted_fixture(scope);
            let Some(expected) = ExpectedRestrictedSession::new(&session, scope, 99) else {
                panic!("restricted fixture");
            };
            assert_eq!(
                expected.validate(&session_bytes(&account, &session), 99),
                Ok(())
            );
            let Ok(other_id) = brews_domain::ids::AccountId::try_from(uuid::Uuid::from_bytes([
                0, 0, 0, 0, 0, 1, 0x70, 0, 0x80, 0, 0, 0, 0, 0, 0, 2,
            ])) else {
                panic!("other account fixture");
            };
            account.account_id = other_id;
            session.account_id = other_id;
            // This reply is valid on its own, but belongs to the other tab's account.
            assert!(eligible(&account, &session, 99));
            assert_eq!(
                expected.validate(&session_bytes(&account, &session), 99),
                Err(ClientError::Unauthorized)
            );
        }
    }
    #[test]
    fn password_binding_requires_exact_scope_deadline_and_eligible_session() {
        let (account, session) = restricted_fixture(SessionScope::PasswordResetOnly);
        let Some(expected) = ExpectedRestrictedSession::new(&session, session.scope, 99) else {
            panic!("restricted fixture");
        };
        assert!(ExpectedRestrictedSession::new(&session, SessionScope::Normal, 99).is_none());
        assert!(
            ExpectedRestrictedSession::new(&session, SessionScope::EnrollmentOnly, 99).is_none()
        );
        assert!(ExpectedRestrictedSession::new(&session, session.scope, 100).is_none());
        assert_eq!(
            expected.validate(&session_bytes(&account, &session), 100),
            Err(ClientError::Unauthorized)
        );
        let (enrolling, enrollment) = restricted_fixture(SessionScope::EnrollmentOnly);
        assert_eq!(
            expected.validate(&session_bytes(&enrolling, &enrollment), 99),
            Err(ClientError::Unauthorized)
        );
        for deadline in [99, 101] {
            let mut successor = session.clone();
            successor.expires_at = deadline;
            assert_eq!(
                expected.validate(&session_bytes(&account, &successor), 98),
                Err(ClientError::Unauthorized)
            );
        }
        let mut disabled = account.clone();
        disabled.disabled = true;
        assert_eq!(
            expected.validate(&session_bytes(&disabled, &session), 99),
            Err(ClientError::Unauthorized)
        );
        assert_eq!(
            expected.validate(
                br#"{"authenticated":false,"account":null,"session":null}"#,
                99
            ),
            Err(ClientError::Unauthorized)
        );
        assert_eq!(
            expected.validate(b"{}", 99),
            Err(ClientError::InvalidResponse)
        );
    }
    #[test]
    fn clearing_rejects_old_refresh_generations_and_drops_identity() {
        thread_local! { static CAPTURE: std::cell::Cell<Option<AccountSession>> = const { std::cell::Cell::new(None) }; }
        fn harness() -> Element {
            let context = provide_account_session();
            CAPTURE.set(Some(context));
            rsx! {
                span {

                }
            }
        }
        let mut dom = VirtualDom::new(harness);
        dom.rebuild_in_place();
        let Some(mut context) = CAPTURE.take() else {
            panic!("session context fixture");
        };
        let (account, session) = fixture();
        context.account.set(Some(account));
        context.session.set(Some(session));
        context.ready.set(true);
        let old_ticket = (context.generation)();
        let old_epoch = (context.epoch)();
        context.clear();
        assert_ne!((context.generation)(), old_ticket);
        assert_ne!((context.epoch)(), old_epoch);
        assert!(context.account.read().is_none());
        assert!(context.session.read().is_none());
        assert!((context.ready)());
        assert!(!context.normal());
    }
    #[test]
    fn pending_logout_cannot_be_cleared_by_starting_authentication() {
        thread_local! { static CAPTURE: std::cell::Cell<Option<AccountSession>> = const { std::cell::Cell::new(None) }; }
        fn harness() -> Element {
            CAPTURE.set(Some(provide_account_session()));
            rsx! {
                span {

                }
            }
        }
        let mut dom = VirtualDom::new(harness);
        dom.rebuild_in_place();
        let Some(mut context) = CAPTURE.take() else {
            panic!("session context fixture");
        };
        let (account, session) = fixture();
        context.account.set(Some(account));
        context.session.set(Some(session));
        context.ready.set(true);
        let old_epoch = (context.epoch)();
        assert!(context.begin_authentication().is_ok());
        assert_ne!((context.epoch)(), old_epoch);
        assert!(!(context.ready)());
        assert!(!context.normal());
        context.logging_out.set(true);
        let logout_epoch = (context.epoch)();
        assert_eq!(context.begin_authentication(), Err(ClientError::Busy));
        assert!((context.logging_out)());
        assert_eq!((context.epoch)(), logout_epoch);
    }
    #[test]
    fn expiry_scope_lifecycle_and_disable_are_all_required() {
        let (mut account, mut session) = fixture();
        assert!(eligible(&account, &session, 99));
        assert!(!eligible(&account, &session, 100));
        account.disabled = true;
        assert!(!eligible(&account, &session, 99));
        account.disabled = false;
        session.scope = SessionScope::EnrollmentOnly;
        assert!(!eligible(&account, &session, 99));
        account.status = AccountStatus::PendingEnrollment;
        assert!(eligible(&account, &session, 99));
        session.role = AccountRole::Admin;
        assert!(!eligible(&account, &session, 99));
    }
}
