use crate::{client, session::AccountSession};
use brews_contracts::users::UserResponse;
use brews_domain::ids::AccountId;
use dioxus::prelude::*;

/// Shows the designated account's public username without exposing its identifier.
#[component]
pub(crate) fn DesignatedHostAlias(account_id: AccountId) -> Element {
    let account = use_context::<AccountSession>();
    let current_alias = account
        .account
        .read()
        .as_ref()
        .filter(|identity| identity.account_id == account_id)
        .map(|identity| super::visible_text(&identity.username));
    let may_lookup = account.admin();
    let alias = use_resource(move || {
        let current_alias = current_alias.clone();
        async move {
            if current_alias.is_some() {
                return current_alias;
            }
            if !may_lookup {
                return None;
            }
            match client::get::<UserResponse>(&format!("/api/users/{account_id}")).await {
                Ok(UserResponse::Account { account }) if account.account_id == account_id => {
                    Some(super::visible_text(&account.username))
                }
                _ => None,
            }
        }
    });
    let label = alias
        .read()
        .as_ref()
        .and_then(Option::as_deref)
        .unwrap_or("Host")
        .to_owned();
    rsx! { span { class: "designated-host-alias", "{label}" } }
}
