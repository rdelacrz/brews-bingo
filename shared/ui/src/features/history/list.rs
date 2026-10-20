use super::{state_label, use_clock, utc_time};
use crate::{client, components::*, session::AccountSession};
use brews_contracts::history::HistoryPage;
use dioxus::prelude::*;

pub(super) fn history_path(outcome: &str, cursor: Option<&str>) -> String {
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    query.append_pair("limit", "20");
    if matches!(outcome, "resolved" | "cancelled") {
        query.append_pair("outcome", outcome);
    }
    if let Some(cursor) = cursor {
        query.append_pair("cursor", cursor);
    }
    format!("/api/history?{}", query.finish())
}

#[component]
pub(crate) fn HistoryList() -> Element {
    let account = use_context::<AccountSession>();
    let epoch = (account.epoch)();
    let mut outcome = use_signal(String::new);
    let mut cursor = use_signal(|| None::<String>);
    let mut refresh = use_signal(|| 0_u64);
    let clock = use_clock();
    let mut page = use_signal(|| None::<Result<HistoryPage, client::ClientError>>);
    let mut loading = use_signal(|| true);
    let mut generation = use_signal(|| 0_u64);
    use_effect(move || {
        let path = history_path(&outcome(), cursor.read().as_deref());
        let _ = refresh();
        let ticket = generation.peek().wrapping_add(1);
        generation.set(ticket);
        page.set(None);
        loading.set(true);
        spawn(async move {
            let mut response = client::get::<HistoryPage>(&path).await;
            if (account.epoch)() != epoch || !account.normal() || *generation.peek() != ticket {
                return;
            }
            if response.as_ref().is_err_and(|error| error.unauthorized()) {
                account.clear();
            }
            if let Ok(page) = &mut response {
                page.games.retain(|game| game.expires_at > client::now_ms());
            }
            page.set(Some(response));
            loading.set(false);
        });
    });
    use_effect(move || {
        let now = clock();
        let expired = page.peek().as_ref().is_some_and(|result| {
            result
                .as_ref()
                .is_ok_and(|page| page.games.iter().any(|game| game.expires_at <= now))
        });
        if expired && let Some(Ok(page)) = page.write().as_mut() {
            page.games.retain(|game| game.expires_at > now);
        }
    });
    if !account.normal() || (account.epoch)() != epoch {
        return rsx! {
            Notice {
                message: "Sign in to view History.",
            }
        };
    }
    let response = page.read().clone();
    rsx! {
        section {
            class: "card stack",
            div {
                class: "card-header",
                h2 {

                    "Historical games"
                }
                button {
                    class: "button ghost",
                    onclick: move |_| refresh += 1,
                    "Refresh history"
                }
            }
            div {
                class: "field",
                label {
                    r#for: "history-outcome",
                    "Outcome"
                }
                select {
                    id: "history-outcome",
                    value: outcome(),
                    onchange: move |event| {
                        cursor.set(None);
                        outcome.set(event.value());
                    },
                    option {
                        value: "",
                        "All outcomes"
                    }
                    option {
                        value: "resolved",
                        "Resolved"
                    }
                    option {
                        value: "cancelled",
                        "Cancelled"
                    }
                }
            }
            if loading() {
                Loading {

                }
            } else {
                match response {
                    None => rsx! {
                        Loading {

                        }
                    },
                    Some(Err(error)) => rsx! {
                        Notice {
                            message: format!("History could not be loaded. {error}"),
                        }
                    },
                    Some(Ok(page)) => rsx! {
                        if page.games.iter().all(|game| game.expires_at <= client::now_ms()) {
                            p {
                                class: "empty-state",
                                "No unexpired historical games on this page."
                            }
                        }
                        div {
                            class: "table-wrap",
                            table {

                                caption {
                                    class: "sr-only",
                                    "Historical games, newest ended first"
                                }
                                thead {

                                    tr {

                                        th {

                                            "Code"
                                        }
                                        th {

                                            "Outcome"
                                        }
                                        th {

                                            "Ended"
                                        }
                                        th {

                                            "Winner"
                                        }
                                        th {

                                            "Expires"
                                        }
                                        th {

                                            "Details"
                                        }
                                    }
                                }
                                tbody {

                                    for game in page.games.iter().filter(|game| game.expires_at > client::now_ms()) {
                                        tr {
                                            key: "{game.game_id}",
                                            td {

                                                "{game.game_code.as_str()}"
                                            }
                                            td {

                                                "{state_label(game.outcome)}"
                                            }
                                            td {

                                                "{utc_time(game.ended_at)}"
                                            }
                                            td {

                                                "{game.winner.as_ref().map_or(\"—\", |winner| winner.alias.as_str())}"
                                            }
                                            td {

                                                "{utc_time(game.expires_at)}"
                                            }
                                            td {

                                                a {
                                                    href: format!("/history/{}", game.game_id),
                                                    "View history"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        div {
                            class: "actions",
                            if cursor.read().is_some() {
                                button {
                                    class: "button secondary",
                                    onclick: move |_| cursor.set(None),
                                    "First page"
                                }
                            }
                            if let Some(next) = page.next_cursor {
                                button {
                                    class: "button secondary",
                                    onclick: move |_| cursor.set(Some(next.clone())),
                                    "Next page"
                                }
                            }
                        }
                    },
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cursor_is_bound_to_filter_and_encoded_as_one_value() {
        assert_eq!(history_path("", None), "/api/history?limit=20");
        assert_eq!(
            history_path("cancelled", Some("v1.123.id.c")),
            "/api/history?limit=20&outcome=cancelled&cursor=v1.123.id.c"
        );
        assert!(
            history_path("resolved", Some("a&outcome=cancelled"))
                .contains("cursor=a%26outcome%3Dcancelled")
        );
    }
}
