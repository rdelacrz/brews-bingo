use super::{CodeBadge, GameContext, Rules};
use brews_contracts::{discovery::GameAvailability, games::GameView};
use dioxus::prelude::*;
use serde_json::json;

#[component]
pub(super) fn Lobby() -> Element {
    let game = use_context::<GameContext>();
    let Some(snapshot) = (game.snapshot)() else {
        return rsx! {};
    };
    rsx! {div {class:"stack",
        h2 {"Waiting for the host to start"}
        p {class:"lead","Boards are assigned when the game starts."}
        match snapshot {
            GameView::Host {game:summary,game_code,configuration,players,connected_player_count,spectator_count,connected_spectator_count,..}=>rsx!{
                if let Some(code)=game_code {CodeBadge {code:code.as_str().to_string()}}
                div {class:"stats",
                    p {class:"stat","{players.len()} / {configuration.player_capacity} occupied player seats"}
                    p {class:"stat","{connected_player_count} connected players"}
                    p {class:"stat","{spectator_count} / {configuration.spectator_capacity} occupied spectator seats"}
                    p {class:"stat","{connected_spectator_count} connected spectators"}
                }
                section {class:"card stack",h3 {"Player attendance"}
                    if players.is_empty(){p {class:"empty-state","No admitted players yet."}}
                    for player in players {p {"{player.alias} · ",if player.connected{"Connected"}else{"Disconnected — seat retained"}}}
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
