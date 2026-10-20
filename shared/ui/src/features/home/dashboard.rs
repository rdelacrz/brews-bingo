use crate::{client, components::*, features::history::HistoryList, session::AccountSession};
use brews_contracts::discovery::{CurrentGame, GameSlot};
use brews_domain::games::GameState;
use dioxus::prelude::*;

#[component]
pub(super) fn Dashboard() -> Element {
    let account = use_context::<AccountSession>();
    let epoch = (account.epoch)();
    let mut refresh = use_signal(|| 0_u64);
    let mut current = use_resource(move || async move {
        let _ = refresh();
        let result = client::get::<CurrentGame>("/api/games/current").await;
        if result.as_ref().is_err_and(|error| error.unauthorized()) && (account.epoch)() == epoch {
            account.clear();
        }
        result
    });
    let clock = use_clock();
    use_effect(move || {
        let now = clock();
        let expired = current.peek().as_ref().is_some_and(|result| {
            result.as_ref().is_ok_and(|current| {
                current
                    .game
                    .as_ref()
                    .is_some_and(|game| game.idle_due.is_some_and(|due| due <= now))
            })
        });
        if expired {
            current.set(None);
            refresh += 1;
        }
    });
    if !account.normal() || (account.epoch)() != epoch {
        return rsx! {
            Notice {
                message: "Sign in to view Games.",
            }
        };
    }
    let result = current.read().clone();
    let pending = current.state()() == UseResourceState::Pending;
    let can_create = result.as_ref().is_some_and(|result| match result {
        Ok(current) => current.game.is_none() && current.slot == GameSlot::Free,
        // A failed lookup is not evidence that the slot is occupied.
        Err(_) => true,
    });
    let refresh_label = result.as_ref().and_then(|result| match result {
        Err(_) => Some("Retry current game lookup"),
        Ok(current)
            if current
                .game
                .as_ref()
                .map_or(current.slot != GameSlot::Free, |game| {
                    !matches!(game.state, GameState::Resolved | GameState::Cancelled)
                }) =>
        {
            Some("Refresh current game")
        }
        Ok(_) => None,
    });
    rsx! {
        section {
            class: "stack",
            PageTitle {
                eyebrow: "HOST & ADMIN",
                title: "Games",
                description: "Your current game and the games worth remembering.",
            }
            section {
                class: "card stack current-game",
                div {
                    class: "current-game-heading",
                    h2 { "Current game" }
                    if let Some(label) = refresh_label {
                        button {
                            class: "button secondary icon-button",
                            r#type: "button",
                            "aria-label": label,
                            title: label,
                            disabled: pending,
                            onclick: move |_| refresh += 1,
                            svg {
                                view_box: "0 0 24 24",
                                fill: "none",
                                stroke: "currentColor",
                                stroke_width: "2",
                                stroke_linecap: "round",
                                stroke_linejoin: "round",
                                "aria-hidden": "true",
                                "focusable": "false",
                                path { d: "M3 12a9 9 0 0 1 15.36-6.36L21 8" }
                                path { d: "M21 3v5h-5" }
                                path { d: "M21 12a9 9 0 0 1-15.36 6.36L3 16" }
                                path { d: "M8 16H3v5" }
                            }
                        }
                    }
                }
                if pending {
                    Loading {

                    }
                } else {
                    match result {
                        None => rsx! {
                            Loading {

                            }
                        },
                        Some(Err(error)) => rsx! {
                            Notice {
                                message: format!("Current game unavailable. {error}"),
                            }
                        },
                        Some(Ok(current)) => rsx! {
                            if let Some(game) = current.game {
                                if game.idle_due.is_some_and(|due| due <= client::now_ms()) {
                                    Notice {
                                        message: "The game's idle deadline was reached. Refreshing the current slot…",
                                    }
                                } else {
                                    div {
                                        class: "game-status-row",
                                        StatusBadge { label: game_state_label(game.state), tone: StatusTone::from(game.state) }
                                        if let Some(code) = &game.game_code {
                                            span { class: "game-publication published", "aria-label": "Published game code", "{code.as_str()}" }
                                        } else {
                                            span { class: "game-publication", "Not published" }
                                        }
                                    }
                                    dl {
                                        class: "game-metadata",
                                        div { class: "metadata-wide", dt { "Designated host" } dd { DesignatedHostAlias { account_id: game.designated_host_id } } }
                                        div { dt { "Created" } dd { "{utc_time(game.created_at)}" } }
                                        if let Some(due) = game.idle_due {
                                            div { dt { "Idle deadline" } dd { "{utc_time(due)}" } }
                                        }
                                    }
                                    if !game.may_operate {
                                        Notice {
                                            message: "Read-only account view. The designated host or an admin operates this game.",
                                        }
                                    }
                                    a {
                                        class: "button",
                                        href: format!("/games/{}/account", game.game_id),
                                        if !game.may_operate {
                                            "View read-only"
                                        } else if game.state == GameState::New {
                                            "Resume setup"
                                        } else if game.state == GameState::AwaitingPlayers {
                                            "Open lobby"
                                        } else {
                                            "Open play"
                                        }
                                    }
                                    p {
                                        class: "muted",
                                        "The global game slot is occupied. Open the current game to continue."
                                    }
                                }
                            } else {
                                match current.slot {
                                    GameSlot::Free => rsx! {
                                        p {
                                            class: "empty-state",
                                            "No current game. Create one when you’re ready."
                                        }
                                    },
                                    GameSlot::Preparing => rsx! {
                                        Notice {
                                            message: "A game is being prepared. Refresh to check its progress; the slot is reserved.",
                                        }
                                    },
                                    GameSlot::Releasing => rsx! {
                                        Notice {
                                            message: "The previous game is releasing its slot. Refresh before creating another game.",
                                        }
                                    },
                                    GameSlot::Occupied => rsx! {
                                        Notice {
                                            message: "The game slot is occupied. Refresh to load the current game.",
                                        }
                                    },
                                }
                            }
                        },
                    }
                }
                if can_create && !pending {
                    a { class: "button", href: "/games/new", "Create game" }
                }
            }
            HistoryList {

            }
        }
    }
}
