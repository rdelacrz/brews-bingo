use super::{CodeBadge, GameContext, Rules};
use crate::components::{Confirm, StatusBadge, StatusTone};
use brews_contracts::{discovery::GameAvailability, games::GameView};
use dioxus::prelude::*;
use serde_json::json;

#[component]
pub(super) fn Lobby() -> Element {
    let game = use_context::<GameContext>();
    let mut confirm_single_player_start = use_signal(|| false);
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
                    {let (can_start,connected)=match (game.availability)(){Some(GameAvailability::Account{can_start,connected_player_count,..})=>(can_start,connected_player_count),_=>(false,connected_player_count)};rsx!{
                        button {class:"button",disabled:!game.operate() || !can_start,onclick:move |_|{
                            if !game.operate() || !can_start{return;}
                            if connected < 2 {confirm_single_player_start.set(true);}
                            else {let expected_revision=summary.view_revision;spawn(async move{if game.operate(){game.send("POST","start",json!({"expected_revision":expected_revision})).await;}});}
                        },"Start game"}
                        if connected == 0 {p {class:"muted","At least one connected eligible player is required to start. The server checks the full retained roster."}}
                        else if !can_start {p {class:"muted","The saved configuration must support distinct boards for the full retained roster before Start."}}
                        if confirm_single_player_start(){Confirm {
                            title:"Start with one connected player?".to_string(),
                            cancel_label:"No".to_string(),
                            confirm_label:Some("Yes".to_string()),
                            busy:!game.operate(),
                            oncancel:move |_|confirm_single_player_start.set(false),
                            onconfirm:move |_|{confirm_single_player_start.set(false);let expected_revision=summary.view_revision;spawn(async move{if game.operate(){game.send("POST","start",json!({"expected_revision":expected_revision})).await;}});},
                            p {"Only one player is connected. Once the game starts, additional players cannot join. Start anyway?"}
                        }}
                    }}
                }
            },
            GameView::Player {alias,game_code,configuration,..}=>rsx!{
                p {class:"lead","Playing as {alias}"}
                if let Some(code)=game_code {CodeBadge {code:code.as_str().to_string()}}
                Rules {configuration}
            },
            GameView::Spectator {game_code,configuration,..}=>rsx!{
                p {class:"lead","Spectator view"}
                CodeBadge {code:game_code.as_str().to_string()}
                Rules {configuration}
            },
            _=>rsx!{},
        }
    }}
}
