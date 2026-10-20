use super::{
    input::SecretBody,
    wire::{AuthReply, Operation},
};
use crate::{
    client::{self, ClientError},
    components::{Notice, PageTitle, PasswordTextField},
    session::AccountSession,
};
use brews_domain::accounts::{SessionScope, validate_password, validate_username};
use dioxus::prelude::*;
use zeroize::{Zeroize, Zeroizing};

struct LoginSecrets {
    epoch: u64,
}
impl LoginSecrets {
    fn new(epoch: u64) -> Self {
        Self { epoch }
    }
    fn invalidate(&mut self, epoch: u64, password: &mut Zeroizing<String>, visible: &mut bool) {
        if self.epoch != epoch {
            password.zeroize();
            *visible = false;
            self.epoch = epoch;
        }
    }
}

#[component]
pub fn Login() -> Element {
    let account = use_context::<AccountSession>();
    let navigate = use_navigator();
    let mut username = use_signal(String::new);
    let mut password = use_signal(|| Zeroizing::new(String::new()));
    let mut show_password = use_signal(|| false);
    let epoch = (account.epoch)();
    let mut secrets = use_signal(|| LoginSecrets::new(epoch));
    let mut busy = use_signal(|| false);
    let mut message = use_signal(|| None::<String>);
    // Clear before rendering, including when login stayed mounted while signed in.
    if secrets.peek().epoch != epoch {
        secrets
            .write()
            .invalidate(epoch, &mut password.write(), &mut show_password.write());
        username.set(String::new());
    }
    use_drop(move || {
        if let Ok(mut password) = password.try_write() {
            password.zeroize();
        }
    });
    rsx! {
        PageTitle { eyebrow: "Login", title: "Welcome back", description: "Log in to prepare and host your next game." }
        if account.normal() {
            section { class: "card stack", p { "You’re signed in." } Link { class: "button", to: "/games", "Open Games" } }
        } else if let Some(session) = account.session.read().as_ref() {
            if session.scope == SessionScope::EnrollmentOnly { Link { class: "button", to: "/enroll", "Finish account setup" } }
            else if session.scope == SessionScope::PasswordResetOnly { Link { class: "button", to: "/password-reset", "Finish password reset" } }
        } else {
            section { class: "card stack",
                if let Some(text) = message() { Notice { message: text } }
                form { class: "form", onsubmit: move |event| {
                    event.prevent_default();
                    if busy() { return; }
                    let credentials = (|| {
                        let name = validate_username(&username()).map_err(|_| ClientError::InvalidInput)?.to_owned();
                        let pass = Zeroizing::new(password.read().to_string());
                        validate_password(&pass).map_err(|_| ClientError::InvalidInput)?;
                        Ok::<_, ClientError>((name, pass))
                    })();
                    let (name, pass) = match credentials { Ok(value) => value, Err(_) => {
                        message.set(Some("Use a 10–50 character ASCII username without internal whitespace and a 10–50 character ASCII password. Username case matters; passwords are preserved exactly.".to_owned()));
                        return;
                    }};
                    let body = SecretBody(serde_json::json!({"username":name,"password":pass.as_str()}));
                    let submitted_epoch = match account.begin_authentication() { Ok(epoch) => epoch, Err(error) => { message.set(Some(error.to_string())); return; } };
                    busy.set(true);
                    message.set(None);
                    password.write().zeroize();
                    spawn(async move {
                        let result = async {
                            client::prepare_command_clock().await?;
                            if (account.epoch)() != submitted_epoch { return Err(ClientError::Busy); }
                            let id = client::new_command_id()?;
                            let reply = client::auth_request::<AuthReply>("/api/auth/login", &body.0, &id, None,
                                move || account.authentication_current(submitted_epoch)).await?;
                            Ok::<_, ClientError>((id, reply))
                        }.await;
                        drop(body);
                        drop(pass);
                        if (account.epoch)() != submitted_epoch { busy.set(false); return; }
                        if super::needs_session_recheck(&result) { account.refresh().await; }
                        match result {
                            Ok((id, reply)) => {
                                if !reply.normal_reply() && !reply.replay(Operation::Login, &id, None, client::now_ms()) {
                                    message.set(Some("The response did not confirm a usable login. Your password was cleared. Check your session or log in again.".to_owned()));
                                    busy.set(false);
                                    return;
                                }
                                let expected = reply.account_id();
                                if account.normal() && expected.is_some() && account.account.read().as_ref().map(|a| a.account_id) == expected {
                                    navigate.push("/games");
                                } else {
                                    message.set(Some("The login command committed, but a usable session was not recovered. Log in again with your password; a receipt cannot reissue a lost cookie.".to_owned()));
                                }
                            }
                            Err(error) if error.uncertain() => {
                                message.set(Some("The login response was lost. Your password has been cleared. If no session was recovered, enter it again for a fresh login.".to_owned()));
                            }
                            Err(error) => { message.set(Some(error.to_string())); }
                        }
                        busy.set(false);
                    });
                },
                    div { class: "field", label { r#for: "login-username", "Username" }
                        input { id: "login-username", name: "username", autocomplete: "username", required: true, spellcheck: "false", autocapitalize: "none", value: username(), disabled: busy(), oninput: move |event| username.set(event.value()) }
                        small { "Case-sensitive. Leading and trailing ASCII whitespace is trimmed." }
                    }
                    PasswordTextField { input_id: "login-password", input_name: "password", label: "Password", autocomplete: "current-password", password, visible: show_password, disabled: busy() }
                    button { class: "button", r#type: "submit", disabled: busy(), if busy() { "Logging in…" } else { "Log in" } }
                }
                p { class: "muted", "Need account setup or a password reset? Ask an authorized admin or developer for a private link." }
            }
        }

    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_change_clears_unsubmitted_password_and_visibility() {
        let mut secrets = LoginSecrets::new(1);
        let mut password = Zeroizing::new("unsubmitted-fixture".to_owned());
        let mut visible = true;
        // A routine revalidation retains the current form.
        secrets.invalidate(1, &mut password, &mut visible);
        assert!(!password.is_empty());
        assert!(visible);
        // Other-tab authentication followed by logout cannot restore the input.
        secrets.invalidate(2, &mut password, &mut visible);
        secrets.invalidate(3, &mut password, &mut visible);
        assert!(password.is_empty());
        assert!(!visible);
        assert_eq!(secrets.epoch, 3);
    }
}
