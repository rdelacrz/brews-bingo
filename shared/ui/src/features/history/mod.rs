//! Read-only account History. Snapshots do not authorize terminal re-entry.
mod list;

use crate::{client, components::*, session::AccountSession};
use brews_contracts::history::{History, HistoryDetail as HistoryResponse};
use brews_domain::games::GameState;
use dioxus::prelude::*;
pub(crate) use list::HistoryList;

#[component]
pub fn HistoryPage() -> Element {
    let account = use_context::<AccountSession>();
    if !(account.ready)() {
        return rsx! { Loading {} };
    }
    if !account.normal() {
        return rsx! { Notice { message: "Sign in with a normal Host or Admin account to view History." } };
    }
    let epoch = (account.epoch)();
    rsx! { Keyed { identity: format!("history-list-{epoch}"), div { class: "stack", PageTitle { title: "History", description: "Read-only results from completed games." } HistoryList {} } } }
}

#[component]
pub fn HistoryDetail(game_id: String) -> Element {
    let account = use_context::<AccountSession>();
    let epoch = (account.epoch)();
    if !(account.ready)() {
        return rsx! { Loading {} };
    }
    if !account.normal() {
        return rsx! { Notice { message: "A normal host or admin session is required to view History." } a { href: "/login", "Sign in" } };
    }
    rsx! { Keyed { identity: format!("history-{game_id}-{epoch}"), Detail { game_id } } }
}

#[component]
fn Detail(game_id: String) -> Element {
    let account = use_context::<AccountSession>();
    let epoch = (account.epoch)();
    let mut refresh = use_signal(|| 0_u64);
    let mut selected = use_signal(|| None::<String>);
    let mut record = use_signal(|| None::<History>);
    let mut error = use_signal(|| None::<String>);
    let mut loading = use_signal(|| true);
    let clock = use_clock();
    use_effect(move || {
        let id = game_id.clone();
        let generation = refresh();
        record.set(None);
        loading.set(true);
        error.set(None);
        spawn(async move {
            if id.parse::<brews_domain::ids::GameId>().is_err() {
                error.set(Some("This History address is invalid.".into()));
                loading.set(false);
                return;
            }
            let response = client::get::<HistoryResponse>(&format!("/api/history/{id}")).await;
            if (account.epoch)() != epoch || !account.normal() || refresh() != generation {
                return;
            }
            match response {
                Ok(response) if response.history.expires_at > client::now_ms() => {
                    record.set(Some(response.history))
                }
                Ok(_) => error.set(Some("This historical game has expired.".into())),
                Err(failure) => {
                    if failure.unauthorized() {
                        account.clear();
                    }
                    error.set(Some(format!(
                        "This historical game is unavailable. {failure}"
                    )));
                }
            }
            loading.set(false);
        });
    });
    use_effect(move || {
        let now = clock();
        let expired = record
            .peek()
            .as_ref()
            .is_some_and(|history| now >= history.expires_at);
        if expired {
            record.set(None);
            selected.set(None);
            error.set(Some("This historical game has expired.".into()));
        }
    });
    if !account.normal() || (account.epoch)() != epoch {
        return rsx! { Notice { message: "Your account access has changed. Sign in again." } };
    }
    let history = record.read().clone();
    rsx! {
        section { class: "stack",
            PageTitle { eyebrow: "GAME HISTORY", title: "A game to remember", description: "A read-only final snapshot. Viewing it never extends its expiry." }
            div { class: "actions", a { class: "button secondary", href: "/games", "Back to Games" }
                button { class: "button ghost", onclick: move |_| refresh += 1, "Refresh" }
            }
            if loading() { Loading {} }
            if let Some(message) = error() { Notice { message } }
            if let Some(history) = history.filter(|history| history.expires_at > client::now_ms()) {
                section { class: "card stack",
                    h2 { "{history.game_code.as_str()}" }
                    StatusBadge { label: game_state_label(history.outcome), tone: StatusTone::from(history.outcome) }
                    if history.outcome == GameState::Resolved { if let Some(winner) = &history.winner { p { "Winner: {winner.alias}" } } }
                    p { class: "muted", "Designated host: {history.designated_host_id}" }
                    dl { class: "stats",
                        div { class: "stat", dt { "Started" } dd { "{utc_time(history.started_at)}" } }
                        div { class: "stat", dt { "Ended" } dd { "{utc_time(history.ended_at)}" } }
                        div { class: "stat", dt { "Expires" } dd { "{utc_time(history.expires_at)}" } }
                    }
                }
                section { class: "card", h2 { "Final calls" }
                    if history.ordered_calls.is_empty() { p { "No calls were made." } }
                    else { ol { class: "call-list", for call in &history.ordered_calls { li { "{call}" } } } }
                }
                section { class: "card stack", h2 { "Final player boards" }
                    if history.players.is_empty() { p { class: "empty-state", "No participating players in this snapshot." } }
                    else {
                        div { class: "field", label { r#for: "history-player", "Inspect a player's final board" }
                            select { id: "history-player", value: selected().unwrap_or_default(), onchange: move |event| selected.set(Some(event.value())),
                                option { value: "", "Choose a player" }
                                for player in &history.players { option { value: player.player_id.to_string(), "{player.alias}" } }
                            }
                        }
                        for player in history.players.iter().filter(|player| selected().as_deref() == Some(player.player_id.to_string().as_str())) {
                            h3 { "{player.alias}" }
                            BoardGrid { side: player.side_length, cells: player.cells.clone(), label: format!("{} final board", player.alias) }
                        }
                    }
                }
            }
        }
    }
}
