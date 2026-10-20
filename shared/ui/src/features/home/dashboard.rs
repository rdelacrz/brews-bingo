use crate::{
    client,
    components::*,
    features::history::{HistoryList, state_label},
    session::AccountSession,
};
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
    rsx! {
        section {
            class: "stack",
            PageTitle {
                eyebrow: "HOST & ADMIN",
                title: "Games",
                description: "Your current game and the games worth remembering.",
            }
            div {
                class: "actions",
                if result.as_ref().is_some_and(|result| {
                    result.as_ref().is_ok_and(|current| {
                        current.game.as_ref().is_some_and(|game| {
                            !matches!(game.state, GameState::Resolved | GameState::Cancelled)
                        })
                    })
                }) {
                    button {
                        class: "button secondary",
                        onclick: move |_| refresh += 1,
                        "Refresh current game"
                    }
                } else if result.as_ref().is_some_and(Result::is_err) {
                    button {
                        class: "button secondary",
                        onclick: move |_| refresh += 1,
                        "Retry current game lookup"
                    }
                }
                a { class: "button", href: "/games/new", "Create game" }
                if account.admin() {
                    a {
                        class: "button ghost",
                        href: "/users",
                        "Manage users"
                    }
                }
            }
            section {
                class: "card stack",
                h2 {

                    "Current game"
                }
                if current.state()() == UseResourceState::Pending {
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
                                    p {
                                        class: "badge",
                                        "{state_label(game.state)}"
                                    }
                                    p {
                                        class: "game-code",
                                        "{game.game_code.as_ref().map_or(\"Not published\", |code| code.as_str())}"
                                    }
                                    p {
                                        class: "muted",
                                        "Designated host: {game.designated_host_id}"
                                    }
                                    p {

                                        "Created: {utc_time(game.created_at)}"
                                    }
                                    if let Some(due) = game.idle_due {
                                        p {

                                            "Idle deadline: {utc_time(due)}"
                                        }
                                    }
                                    if game.may_operate {
                                        if account.admin() {
                                            Notice {
                                                message: "Admin account view: operator access is verified by the server.",
                                            }
                                        }
                                    } else {
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
            }
            HistoryList {

            }
        }
    }
}
