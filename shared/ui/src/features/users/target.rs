use super::{
    handoff::PrivateLink,
    policy::{Action, role_label, status_label, username_display},
};
use crate::{client, components::*, session::AccountSession};
use brews_contracts::{SafeAccount, users::UserResponse};
use brews_domain::ids::AccountId;
use dioxus::prelude::*;
use zeroize::Zeroizing;

#[derive(Clone)]
struct Confirmation {
    action: Action,
    target: SafeAccount,
}

#[component]
pub(super) fn AccountDetail(
    account_id: AccountId,
    mut link: Signal<Option<PrivateLink>>,
    onclose: EventHandler<()>,
    onchanged: EventHandler<()>,
) -> Element {
    let session = use_context::<AccountSession>();
    let epoch = (session.epoch)();
    let command = use_context::<client::CommandState>();
    let mut refresh = use_signal(|| 0_u64);
    let mut confirmation = use_signal(|| None::<Confirmation>);
    let mut original_confirmation = use_signal(|| None::<Confirmation>);
    let mut checking = use_signal(|| false);
    let mut notice = use_signal(|| None::<String>);

    let detail = use_resource(move || async move {
        let _ = refresh();
        let result = client::get::<UserResponse>(&format!("/api/users/{account_id}")).await;
        if result.as_ref().is_err_and(|error| error.unauthorized()) && (session.epoch)() == epoch {
            session.clear();
        }
        result
    });
    let actor = (session.account)().map(|account| account.account_id);
    if !session.admin() || (session.epoch)() != epoch {
        return rsx! {};
    }
    let response = detail.read().clone();
    let unresolved = command.unresolved();
    rsx! {
        section { class: "card stack",
            div { class: "card-header", h2 { "Account detail" }
                div { class: "actions",
                    button { class: "button ghost", disabled: checking() || (command.busy)(), onclick: move |_| { if !unresolved { confirmation.set(None); } refresh += 1; }, "Refresh account" }
                    button { class: "button ghost", disabled: (command.busy)() || unresolved, onclick: move |_| { if !(command.busy)() && !command.unresolved() { link.set(None); onclose.call(()); } }, "Close detail" }
                }
            }
            if checking() { Loading {} }
            if detail.state()() == UseResourceState::Pending { Loading {} }
            else { match response {
                None => rsx! { Loading {} },
                Some(Err(error)) => rsx! { Notice { message: format!("This account is unavailable or was removed. {error}") } },
                Some(Ok(UserResponse::Account { account })) => rsx! {
                    h3 { "{username_display(&account.username)}" }
                    dl { class: "stats",
                        div { dt { "Account ID" } dd { "{account.account_id}" } }
                        div { dt { "Role" } dd { "{role_label(account.role)}" } }
                        div { dt { "Lifecycle" } dd { StatusBadge { label: status_label(account.status), tone: StatusTone::from(account.status) } } }
                        div { dt { "Access" } dd { StatusBadge { label: if account.disabled { "Disabled" } else { "Enabled" }, tone: if account.disabled { StatusTone::Danger } else { StatusTone::Success } } } }
                        div { dt { "Created" } dd { "{utc_time(account.created_at)}" } }
                    }
                    if account.disabled { p { class: "muted", "Enabling preserves the lifecycle. Enrollment or reset completion may still be required." } }
                    if let Some(actor) = actor {
                        div { class: "actions",
                            for action in [Action::Enrollment, Action::Reset, Action::Disable, Action::Enable, Action::Delete].into_iter().filter(|action| action.allowed(&account, actor)) {
                                button { class: if matches!(action, Action::Delete | Action::Disable) { "button danger" } else { "button secondary" },
                                    disabled: checking() || (command.busy)() || unresolved,
                                    onclick: move |_| {
                                        if checking() || (command.busy)() || command.unresolved() || !session.admin() || (session.epoch)() != epoch { return; }
                                        checking.set(true); notice.set(None);
                                        spawn(async move {
                                            let result = client::get::<UserResponse>(&format!("/api/users/{account_id}")).await;
                                            if !session.admin() || (session.epoch)() != epoch { return; }
                                            match result {
                                                Ok(UserResponse::Account { account: target }) if action.allowed(&target, actor) => confirmation.set(Some(Confirmation { action, target })),
                                                Ok(_) => { notice.set(Some("The target changed. Refresh the account and choose an available action.".into())); refresh += 1; }
                                                Err(error) => {
                                                    if error.unauthorized() { session.clear(); }
                                                    notice.set(Some(format!("The account could not be checked. {error}")));
                                                }
                                            }
                                            checking.set(false);
                                        });
                                    }, "{action.label()}"
                                }
                            }
                        }
                    }
                    p { class: "muted", "Role and username are fixed. The server checks current authority, lifecycle, active-game assignments and admin safety before every action." }
                },
                Some(Ok(_)) => rsx! { Notice { message: "The response did not contain account detail. Refresh to try again." } },
            } }
            if let Some(message) = notice() { Notice { message } }
            CommandFeedback { command }
            if let Some(current) = confirmation() {
                Confirm { title: format!("{}: {}", current.action.label(), username_display(&current.target.username)), busy: (command.busy)(),
                    oncancel: move |_| { if !(command.busy)() { confirmation.set(None); } },
                    onconfirm: move |_| {
                        if (command.busy)() || !session.admin() || (session.epoch)() != epoch { return; }
                        let Some(current) = confirmation() else { return; };
                        original_confirmation.set(Some(current.clone()));
                        link.set(None);
                        spawn(async move {
                            let response = command.run_until::<UserResponse>(current.action.method(), &current.action.path(current.target.account_id), serde_json::Value::Null, |response| !matches!(response, UserResponse::Pending { .. })).await;
                            if !session.admin() || (session.epoch)() != epoch { return; }
                            match response {
                                Ok(UserResponse::EnrollmentLink { enrollment_url, link_expires_at, .. }) => {
                                    link.set(Some(PrivateLink { url: Zeroizing::new(enrollment_url), purpose: "enrollment", target: username_display(&current.target.username), expires_at: link_expires_at }));
                                    notice.set(Some("A new private enrollment link was issued. Use the handoff panel above.".into()));
                                    confirmation.set(None); original_confirmation.set(None); refresh += 1; onchanged.call(());
                                }
                                Ok(UserResponse::PasswordReset { reset_url, link_expires_at, .. }) => {
                                    link.set(Some(PrivateLink { url: Zeroizing::new(reset_url), purpose: "password reset", target: username_display(&current.target.username), expires_at: link_expires_at }));
                                    notice.set(Some("Password reset committed. Deliver the new link from the private handoff panel above.".into()));
                                    confirmation.set(None); original_confirmation.set(None); refresh += 1; onchanged.call(());
                                }
                                Ok(UserResponse::Committed { .. }) => {
                                    link.set(None);
                                    notice.set(Some("The original command committed. This receipt contains no private URL. If a link was not delivered, explicitly issue a successor; earlier links will become invalid.".into()));
                                    confirmation.set(None); original_confirmation.set(None); refresh += 1; onchanged.call(());
                                }
                                Ok(UserResponse::Pending { operation_id }) => {
                                    link.set(None);
                                    notice.set(Some(format!("Removal coordination is pending ({operation_id}). Completion and socket closure are not confirmed. Review and retry the original action to check completion without issuing a new command.")));
                                    confirmation.set(None); refresh += 1; onchanged.call(());
                                }
                                Ok(UserResponse::Deleted { deleted: true, .. }) => {
                                    link.set(None); notice.set(Some("Account deletion committed. Unexpired History is retained.".into()));
                                    confirmation.set(None); original_confirmation.set(None); refresh += 1; onchanged.call(());
                                }
                                Ok(UserResponse::Disabled { disabled: true, .. } | UserResponse::Enabled { disabled: false, .. }) => {
                                    link.set(None); notice.set(Some("The account access change committed. This response does not confirm every socket has closed.".into()));
                                    confirmation.set(None); original_confirmation.set(None); refresh += 1; onchanged.call(());
                                }
                                Ok(_) => { notice.set(Some("The response did not confirm this action. Refresh before taking another action.".into())); confirmation.set(None); original_confirmation.set(None); refresh += 1; }
                                Err(error) => {
                                    if error.unauthorized() { link.set(None); session.clear(); }
                                    else if !command.unresolved() { confirmation.set(None); original_confirmation.set(None); refresh += 1; }
                                }
                            }
                        });
                    },
                    p { "{current.action.consequence()}" }
                    p { class: "muted", "Target account ID: {current.target.account_id}" }
                    if actor == Some(current.target.account_id) && current.action == Action::Reset {
                        Notice { message: "Resetting your own account revokes this session. The server will withhold the private link after revocation. Another authorized admin or the developer CLI must reissue it. You will need to sign in again after completing reset." }
                    }
                    if unresolved { Notice { message: "The previous result remains unresolved. Retry this exact action to check the original command; no new command will be issued." } }
                }
            }
            if unresolved && confirmation.read().is_none() {
                Notice { message: "A command outcome remains unresolved. Retry its unchanged inputs before taking another action." }
                button { class: "button secondary", onclick: move |_| confirmation.set(original_confirmation()), "Review and retry original action" }
            }
        }
    }
}
