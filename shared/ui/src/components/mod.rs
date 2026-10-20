//! Reusable semantic views; none of these components changes game state.
mod designated_host_alias;
mod dialog;
mod keyed;
mod password_text_field;
mod status_badge;
mod time;
pub(crate) use designated_host_alias::DesignatedHostAlias;
pub use dialog::Confirm;
pub use keyed::Keyed;
pub(crate) use password_text_field::PasswordTextField;
pub(crate) use status_badge::{StatusBadge, StatusTone, game_state_label};
pub(crate) use time::{use_clock, utc_time};

use crate::client::CommandState;
use brews_contracts::games::{BoardView, CallView};
use brews_domain::games::{BoardCell, BoardCellKind};
use dioxus::prelude::*;

/// Make permitted control characters visible without changing the stored identity.
pub fn visible_text(value: &str) -> String {
    let mut visible = String::with_capacity(value.len());
    for character in value.chars() {
        if character == '\\' {
            visible.push_str("\\\\");
        } else if character.is_control() {
            visible.push_str(&format!("\\x{:02X}", character as u32));
        } else {
            visible.push(character);
        }
    }
    visible
}

#[component]
pub fn Notice(message: String) -> Element {
    rsx! {
        div {
            class: "notice",
            role: "status",
            "{message}"
        }
    }
}
#[component]
pub fn Loading() -> Element {
    rsx! {
        p {
            class: "loading",
            role: "status",
            "Loading…"
        }
    }
}
#[component]
pub fn PageTitle(
    #[props(default)] eyebrow: String,
    title: String,
    #[props(default)] description: String,
) -> Element {
    rsx! {
        header {
            class: "page-heading",
            if !eyebrow.is_empty() {
                p {
                    class: "eyebrow",
                    "{eyebrow}"
                }
            }
            h1 {

                "{title}"
            }
            if !description.is_empty() {
                p {
                    class: "lead",
                    "{description}"
                }
            }
        }
    }
}
#[component]
pub fn CommandFeedback(command: CommandState) -> Element {
    rsx! {
        if *command.busy.read() {
            p {
                role: "status",
                "Sending command…"
            }
        }
        if let Some(error) = *command.error.read() {
            p {
                class: "notice",
                role: "alert",
                "{error}"
            }
        }
    }
}

#[derive(Props, Clone)]
pub struct BoardProps {
    pub board: BoardView,
}
impl PartialEq for BoardProps {
    fn eq(&self, other: &Self) -> bool {
        self.board.side_length == other.board.side_length
            && self.board.cells == other.board.cells
            && self.board.qualified == other.board.qualified
            && self.board.qualifying_lines == other.board.qualifying_lines
    }
}
#[allow(non_snake_case)]
pub fn Board(props: BoardProps) -> Element {
    let board = props.board;
    rsx! {
        div {
            class: "stack",
            p {
                class: "muted",
                if board.qualified {
                    "Qualified — the host selects the winner."
                } else {
                    "Matches update automatically as calls are committed."
                }
            }
            BoardGrid {
                side: board.side_length,
                cells: board.cells,
                label: "Game board",
            }
        }
    }
}

#[component]
pub fn BoardGrid(side: u8, cells: Vec<BoardCell>, label: String) -> Element {
    rsx! {
        div {
            class: "board-region",
            tabindex: "0",
            role: "region",
            "aria-label": format!("{label}; scroll to inspect every cell"),
            div {
                class: "board",
                role: "table",
                "aria-label": label,
                style: "grid-template-columns: repeat({side}, minmax(3rem, 1fr));",
                for row_number in 1..=side {
                    div {
                        role: "row",
                        style: "display: contents;",
                        for cell in cells.iter().filter(|cell| cell.position.row == row_number) {
                            {
                                let (value, free) = match &cell.kind {
                                    BoardCellKind::Free => ("FREE".to_owned(), true),
                                    BoardCellKind::Value(value) => (value.clone(), false),
                                };
                                let row = cell.position.row;
                                let column = cell.position.column;
                                let state = if cell.is_matched { "matched" } else { "unmatched" };
                                let classes = format!(
                                    "board-cell{}{}",
                                    if cell.is_matched { " matched" } else { "" },
                                    if free { " free" } else { "" },
                                );
                                rsx! {
                                    div {
                                        class: classes,
                                        role: "cell",
                                        "aria-label": "Row {row}, column {column}: {value}, {state}",
                                        "{value}"
                                        if cell.is_matched {
                                            span {
                                                "aria-hidden": "true",
                                                " ✓"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Props, Clone)]
pub struct CallsProps {
    pub calls: Vec<CallView>,
}
impl PartialEq for CallsProps {
    fn eq(&self, other: &Self) -> bool {
        self.calls.len() == other.calls.len()
            && self
                .calls
                .iter()
                .zip(&other.calls)
                .all(|(a, b)| a.sequence_no == b.sequence_no && a.value == b.value)
    }
}
#[allow(non_snake_case)]
pub fn Calls(props: CallsProps) -> Element {
    rsx! {
        section {
            class: "stack",
            "aria-label": "Committed calls",
            h2 {

                "Call history"
            }
            if props.calls.is_empty() {
                p {
                    class: "muted",
                    "No calls yet."
                }
            } else {
                ol {
                    class: "call-list",
                    for call in props.calls {
                        li {
                            key: "{call.sequence_no}",
                            value: "{call.sequence_no}",
                            "aria-label": "Call {call.sequence_no}: {call.value}",
                            "{call.value}"
                        }
                    }
                }
            }
        }
    }
}
