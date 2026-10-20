use super::{CodeBadge, GameContext, Rules};
use crate::components::{StatusBadge, StatusTone};
use brews_contracts::{discovery::GameAvailability, games::GameView};
use dioxus::prelude::*;
use serde_json::json;

#[component]
pub(super) fn Lobby() -> Element {
    let game = use_context::<GameContext>();
    let Some(snapshot) = (game.snapshot)() else {
        return rsx! {};
    };
    rsx! {div {class:"stack lobby-content",
        h2 {"Waiting for the host to start"}
        p {class:"lead","Boards are assigned when the game starts."}
        match snapshot {
            GameView::Host {game:summary,game_code,configuration,players,connected_player_count,spectator_count,connected_spectator_count,..}=>rsx!{
                if let Some(code)=game_code {CodeBadge {code:code.as_str().to_string()}}
                dl {class:"lobby-metrics", "aria-label":"Lobby attendance summary",
                    div {class:"lobby-metric",
                        dt {"Players"}
                        dd {
                            div {class:"lobby-metric-total", strong {"{players.len()}"} span {"/ {configuration.player_capacity} seats"}}
                            p {class:"lobby-metric-presence", span {class:if connected_player_count > 0 {"presence-dot connected"} else {"presence-dot"}, "aria-hidden":"true"} "{connected_player_count} connected"}
                        }
                    }
                    div {class:"lobby-metric",
                        dt {"Spectators"}
                        dd {
                            div {class:"lobby-metric-total", strong {"{spectator_count}"} span {"/ {configuration.spectator_capacity} seats"}}
                            p {class:"lobby-metric-presence", span {class:if connected_spectator_count > 0 {"presence-dot connected"} else {"presence-dot"}, "aria-hidden":"true"} "{connected_spectator_count} connected"}
                        }
                    }
                }
                section {class:"card stack attendance-card",
                    div {class:"attendance-heading",
                        h3 {"Player attendance"}
                        span {class:"attendance-count", "{players.len()} / {configuration.player_capacity}"}
                    }
                    if players.is_empty(){p {class:"empty-state attendance-empty","No players have joined yet. Share the game code to invite your group."}}
                    else {div {class:"attendance-list", role:"list", "aria-label":"Admitted players",
                        for player in players {div { key:"{player.player_id}", class:"attendance-row", role:"listitem",
                            p { "{player.alias}" }
                            StatusBadge { label: if player.connected { "Connected" } else { "Disconnected — seat retained" }, tone: if player.connected { StatusTone::Success } else { StatusTone::Warning } }
                        }}
                    }}
                }
                Rules {configuration}
                if (game.may_operate)(){
                    {let can_start=matches!((game.availability)(),Some(GameAvailability::Account{can_start:true,..}));rsx!{
                        button {class:"button",disabled:!game.operate() || !can_start,onclick:move |_|async move{if game.operate(){game.send("POST","start",json!({"expected_revision":summary.view_revision})).await;}},"Start game"}
                        if !can_start{p {class:"muted","Start requires at least two connected eligible players and a feasible saved configuration. The server checks the full retained roster."}}
                    }}
                }
                super::settings::OperatorSettings {}
            },
            GameView::Player {alias,game_code,configuration,..}=>rsx!{
                p {class:"lead","Playing as {alias}"}
                if let Some(code)=game_code {CodeBadge {code:code.as_str().to_string()}}
                Rules {configuration}
                super::settings::ParticipantSettings {}
            },
            GameView::Spectator {game_code,configuration,..}=>rsx!{
                p {class:"lead","Spectator view"}
                CodeBadge {code:game_code.as_str().to_string()}
                Rules {configuration}
                super::settings::ParticipantSettings {}
            },
            _=>rsx!{},
        }
    }}
}
