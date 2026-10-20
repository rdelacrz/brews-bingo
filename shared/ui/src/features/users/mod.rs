//! Admin-only Users, bounded pages, guarded actions and ephemeral link handoff.
mod handoff;
mod policy;
mod target;
use crate::{client, components::*, session::AccountSession};
use brews_contracts::users::UserResponse;
use brews_domain::{
    accounts::{AccountRole, validate_username},
    ids::AccountId,
};
use dioxus::prelude::*;
use handoff::{Handoff, PrivateLink};
use policy::{role_label, status_label, username_display};
use zeroize::Zeroizing;

#[component]
pub fn Users() -> Element {
    let account = use_context::<AccountSession>();
    let epoch = (account.epoch)();
    if !(account.ready)() {
        return rsx! { Loading {} };
    }
    if !account.admin() {
        return rsx! { Notice { message: "A normal, Verified admin session is required to manage users." } a { href: "/login", "Sign in" } };
    }
    rsx! { Keyed { identity: format!("users-{epoch}"), UsersWorkspace {} } }
}

#[component]
fn UsersWorkspace() -> Element {
    let session = use_context::<AccountSession>();
    let epoch = (session.epoch)();
    let mut cursor = use_signal(|| None::<AccountId>);
    let mut refresh = use_signal(|| 0_u64);
    let mut selected = use_signal(|| None::<AccountId>);
    let mut creating = use_signal(|| false);
    let mut link = use_signal(|| None::<PrivateLink>);
    let mut receipt_notice = use_signal(|| None::<String>);
    let mut command = client::use_command();
    use_context_provider(|| command);
    let locked = move || (command.busy)() || command.unresolved();
    let page = use_resource(move || async move {
        let _ = refresh();
        let path = cursor().map_or_else(
            || "/api/users?limit=50".into(),
            |id| format!("/api/users?limit=50&cursor={id}"),
        );
        let result = client::get::<UserResponse>(&path).await;
        if result.as_ref().is_err_and(|error| error.unauthorized()) && (session.epoch)() == epoch {
            session.clear();
        }
        result
    });
    if !session.admin() || (session.epoch)() != epoch {
        return rsx! { Notice { message: "Admin access has changed. Sign in again." } };
    }
    let response = page.read().clone();
    rsx! {
        section { class: "stack",
            PageTitle { eyebrow: "ADMINISTRATION", title: "Users", description: "Create accounts, manage access, and deliver setup links privately." }
            div { class: "actions",
                button { class: "button", disabled: locked(), onclick: move |_| { if !locked() { command.error.set(None); link.set(None); selected.set(None); creating.set(true); } }, "Create account" }
                button { class: "button secondary", onclick: move |_| refresh += 1, "Refresh users" }
                a { class: "button ghost", href: "/games", "Back to Games" }
            }
            if let Some(message) = receipt_notice() { Notice { message } }
            Handoff { link }
            if creating() {
                CreateAccount { onclose: move |_| { if !locked() { creating.set(false); link.set(None); } },
                    onresult: move |response| {
                        if let UserResponse::Created { account, enrollment_url, link_expires_at, .. } = response {
                            selected.set(Some(account.account_id)); creating.set(false);
                            link.set(Some(PrivateLink { url: Zeroizing::new(enrollment_url), purpose: "enrollment", target: username_display(&account.username), expires_at: link_expires_at }));
                            refresh += 1;
                        } else if let UserResponse::Committed { receipt } = response {
                            receipt_notice.set(Some("Creation committed. This receipt cannot recover the private URL. Open the account and explicitly reissue enrollment if the link was not delivered.".into()));
                            selected.set(Some(receipt.account_id)); creating.set(false); refresh += 1;
                        }
                    }
                }
            }
            if let Some(id) = selected() {
                Keyed { identity: format!("account-{id}"),
                    target::AccountDetail { account_id: id, link,
                        onclose: move |_| { if !locked() { selected.set(None); link.set(None); } },
                        onchanged: move |_| refresh += 1,
                    }
                }
            }
            section { class: "card stack", h2 { "Accounts" }
                p { class: "muted", "Ordered by account ID. Each page contains up to 50 accounts." }
                if page.state()() == UseResourceState::Pending { Loading {} }
                else { match response {
                    None => rsx! { Loading {} },
                    Some(Err(error)) => rsx! { Notice { message: format!("Users could not be loaded. {error}") } },
                    Some(Ok(UserResponse::Users { users, next_cursor })) => rsx! {
                        if users.is_empty() { p { class: "empty-state", "No accounts on this page." } }
                        div { class: "table-wrap", table {
                            caption { class: "sr-only", "Account management" }
                            thead { tr { th { "Username" } th { "Role" } th { "Lifecycle" } th { "Access" } th { "Created" } th { "Manage" } } }
                            tbody { for account in users {
                                tr { key: "{account.account_id}",
                                    td { "{username_display(&account.username)}" }
                                    td { "{role_label(account.role)}" }
                                    td { StatusBadge { label: status_label(account.status), tone: StatusTone::from(account.status) } }
                                    td { StatusBadge { label: if account.disabled { "Disabled" } else { "Enabled" }, tone: if account.disabled { StatusTone::Danger } else { StatusTone::Success } } }
                                    td { "{utc_time(account.created_at)}" }
                                    td { button { class: "button ghost", disabled: locked(), onclick: move |_| { if !locked() { command.error.set(None); link.set(None); creating.set(false); selected.set(Some(account.account_id)); } }, "Manage" } }
                                }
                            } }
                        } }
                        div { class: "actions",
                            if cursor().is_some() { button { class: "button secondary", onclick: move |_| cursor.set(None), "First page" } }
                            if let Some(next) = next_cursor { button { class: "button secondary", onclick: move |_| cursor.set(Some(next)), "Next page" } }
                        }
                    },
                    Some(Ok(_)) => rsx! { Notice { message: "The server returned an unexpected Users response. Refresh to try again." } },
                } }
            }
        }
    }
}

#[component]
fn CreateAccount(onclose: EventHandler<()>, onresult: EventHandler<UserResponse>) -> Element {
    let session = use_context::<AccountSession>();
    let epoch = (session.epoch)();
    let command = use_context::<client::CommandState>();
    let mut username = use_signal(String::new);
    let mut role = use_signal(|| AccountRole::Host);
    let mut validation = use_signal(|| None::<String>);
    let mut receipt_notice = use_signal(|| None::<String>);
    let unresolved = command.unresolved();
    if !session.admin() || (session.epoch)() != epoch {
        return rsx! {};
    }
    rsx! {
        section { class: "card stack",
            h2 { "Create account" }
            p { class: "muted", "No initial password is generated here. The recipient sets their own password using a private enrollment link." }
            form { class: "form", onsubmit: move |event| {
                event.prevent_default();
                async move {
                if (command.busy)() || !session.admin() || (session.epoch)() != epoch { return; }
                let username = match validate_username(&username()) { Ok(name) => name.to_owned(), Err(_) => { validation.set(Some("Use 10–50 ASCII characters, without internal spaces or tabs. Capitalization is significant.".into())); return; } };
                validation.set(None);
                let path = if role() == AccountRole::Admin { "/api/users/admins" } else { "/api/users/hosts" };
                let result = command.run::<UserResponse>("POST", path, serde_json::json!({"username":username})).await;
                if !session.admin() || (session.epoch)() != epoch { return; }
                match result {
                    Ok(response @ UserResponse::Created { .. }) => onresult.call(response),
                    Ok(response @ UserResponse::Committed { .. }) => {
                        receipt_notice.set(Some("Creation committed, but this receipt cannot recover the private URL. Open the account and explicitly reissue enrollment if needed.".into()));
                        onresult.call(response);
                    }
                    Ok(UserResponse::Pending { .. }) => receipt_notice.set(Some("The server reports pending coordination. Refresh Users before taking another action; completion has not been confirmed.".into())),
                    Ok(_) => receipt_notice.set(Some("The response did not confirm account creation. Refresh Users before trying another command.".into())),
                    Err(error) => { if error.unauthorized() { session.clear(); } }
                }
              }
            },
                fieldset { disabled: (command.busy)() || unresolved,
                    div { class: "field", label { r#for: "new-username", "Username" }
                        input { id: "new-username", r#type: "text", value: username(), autocomplete: "off", spellcheck: "false", oninput: move |event| username.set(event.value()) }
                    }
                    div { class: "field", label { r#for: "new-account-role", "Role" }
                        select { id: "new-account-role", value: if role() == AccountRole::Host { "host" } else { "admin" }, onchange: move |event| role.set(if event.value() == "admin" { AccountRole::Admin } else { AccountRole::Host }),
                            option { value: "host", "Host" } option { value: "admin", "Admin" }
                        }
                    }
                }
                if role() == AccountRole::Admin { Notice { message: "Admins can manage accounts and operate games across hosts. This role cannot be edited after creation." } }
                if let Some(message) = validation() { p { role: "alert", "{message}" } }
                CommandFeedback { command }
                div { class: "actions",
                    button { class: "button", r#type: "submit", disabled: (command.busy)(), if unresolved { "Retry original creation" } else { "Create account" } }
                    button { class: "button secondary", r#type: "button", disabled: (command.busy)() || unresolved, onclick: move |_| onclose.call(()), "Cancel" }
                }
            }
            if let Some(message) = receipt_notice() { Notice { message } }
        }
    }
}
