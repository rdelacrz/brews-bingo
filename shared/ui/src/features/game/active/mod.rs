mod speech;
use super::{CodeBadge, GameContext, model};
use crate::{
    client,
    components::{Board, Calls, Confirm, Notice, StatusBadge, StatusTone},
};
use brews_contracts::games::{GameView, reads::RemainingValues};
use dioxus::prelude::*;
use serde_json::json;

#[component]
pub(super) fn Play() -> Element {
    let game = use_context::<GameContext>();
    let mut venue = use_signal(|| false);
    let Some(snapshot) = (game.snapshot)() else {
        return rsx! {};
    };
    let calls = model::calls(&snapshot);
    let latest = calls.last().map(|call| call.value.clone());
    let code = match &snapshot {
        GameView::Host { game_code, .. } | GameView::Player { game_code, .. } => {
            game_code.as_ref().map(|code| code.as_str().to_string())
        }
        GameView::Spectator { game_code, .. } => Some(game_code.as_str().to_string()),
        _ => None,
    };
    rsx! {div {class:if venue(){"venue stack"}else{"stack"},id:"game-venue",
        if let Some(code)=code{CodeBadge {code}}
        p {class:"eyebrow","Single Line — one complete row, column, or diagonal"}
        section {class:"card current-call","aria-label":"Latest committed call",
            if let Some(value)=latest{p {"Current call"}strong {role:"status","aria-live":"polite","{value}"}}
            else{p {"No values called yet"}}
        }
        speech::SpeechControls {}
        match snapshot {
            GameView::Host {..}=>rsx!{HostControls {} super::settings::OperatorSettings {}},
            GameView::Player {alias,board,..}=>rsx!{
                h2 {"{alias}’s board"}
                if let Some(board)=board {
                    if board.qualified{p {class:"notice",role:"status","Your board qualifies — say Bingo and your alias to the host. The host selects the winner."}}
                    p {class:"muted",{board.qualifying_lines.iter().map(model::line_label).collect::<Vec<_>>().join(" · ")}}
                    Board {board}
                }else{Notice {message:"The assigned board is unavailable. Reconnect to synchronize."}}
                super::settings::ParticipantSettings {}
            },
            GameView::Spectator {exhausted,..}=>rsx!{
                if exhausted{p {class:"notice","All values have been called. The host will select a winner or end the game."}}
                button {class:"button secondary",onclick:move |_|venue.set(!venue()),if venue(){"Exit display mode"}else{"Display mode"}}
                button {class:"button ghost",onclick:move |_|{speech::fullscreen();},"Toggle fullscreen"}
                super::settings::ParticipantSettings {}
            },
            _=>rsx!{},
        }
        section {class:"card stack",h2 {"Call history"}Calls {calls}}
    }}
}

#[component]
fn HostControls() -> Element {
    let game = use_context::<GameContext>();
    let mut manual = use_signal(String::new);
    let mut selected = use_signal(|| None::<String>);
    let mut award = use_signal(|| None::<(String, String, u64)>);
    let mut error = use_signal(|| None::<String>);
    let revision = use_memo(move || game.snapshot.read().as_ref().map(GameView::view_revision));
    let mut remaining = use_resource(move || {
        let _revision = revision();
        async move { client::get::<RemainingValues>(&game.path("remaining-values")).await }
    });
    use_effect(move || {
        let _revision = revision();
        award.set(None);
    });
    let Some(GameView::Host {
        game: summary,
        configuration,
        players,
        calls,
        ..
    }) = (game.snapshot)()
    else {
        return rsx! {};
    };
    let exhausted = calls.len() >= configuration.numeric_upper_bound as usize;
    let inspect = selected()
        .and_then(|id| players.iter().find(|p| p.player_id.to_string() == id))
        .or_else(|| players.first());
    rsx! {div {class:"stack",
        if exhausted{p {class:"notice","The value pool is exhausted. This does not automatically end the game."}}
        else{p {class:"muted","{configuration.numeric_upper_bound as usize-calls.len()} values remain"}}
        if (game.may_operate)(){
            section {class:"card stack",h2 {"Call a value"}
                button {class:"button",disabled:!game.operate() || exhausted,onclick:move |_|async move{if game.operate(){let revision=game.snapshot.read().as_ref().map(GameView::view_revision);if let Some(revision)=revision{game.send("POST","calls/random",json!({"expected_revision":revision})).await;}}},"Draw random value"}
                form {class:"form",onsubmit:move |event|{
                    event.prevent_default();if !game.operate() || exhausted{return;}
                    let value=manual();
                    let Some(GameView::Host{game:current,calls:current_calls,..})=(game.snapshot)() else{return;};
                    let valid=matches!(&*remaining.read(),Some(Ok(values)) if values.validate().is_ok() && values.remaining_values.contains(&value)) && !current_calls.iter().any(|call|call.value==value);
                    if !valid{error.set(Some("Enter an exact value from the current remaining pool, without spaces or leading zeros.".to_string()));return;}
                    error.set(None);spawn(async move{game.send("POST","calls/manual",json!({"value":value,"expected_revision":current.view_revision})).await;});
                },label {class:"field","Manual value" input {r#type:"text",inputmode:"numeric",value:manual,disabled:!game.operate() || exhausted,oninput:move |e|manual.set(e.value())}}
                    button {class:"button secondary",r#type:"submit",disabled:!game.operate() || exhausted || !matches!(&*remaining.read(),Some(Ok(values)) if values.view_revision==summary.view_revision),"Call value"}
                }
                match &*remaining.read(){
                    Some(Ok(values)) if values.validate().is_ok()=>rsx!{details {summary {"Remaining value pool"}p {class:"call-list", {values.remaining_values.join(", ")}}}},
                    Some(Err(reason))=>rsx!{Notice {message:reason.to_string()}button {class:"button ghost",disabled:!game.operate(),onclick:move |_|remaining.restart(),"Refresh remaining pool"}},
                    _=>rsx!{p {class:"muted","Refreshing the remaining value pool…"}},
                }
                if let Some(message)=error(){Notice {message}}
            }
        }
        section {class:"card stack",h2 {"Private board inspection"}
            label {class:"field","Player board" select {value:selected().unwrap_or_default(),onchange:move |e|selected.set(Some(e.value())),option {value:"","Choose a player"}for player in &players{option {value:player.player_id.to_string(),"{player.alias}"}}}}
            if let Some(player)=inspect {
                h3 {"{player.alias}"}
                StatusBadge { label: if player.connected { "Connected" } else { "Not connected" }, tone: if player.connected { StatusTone::Success } else { StatusTone::Warning } }
                if !player.connected { p { class: "muted", "Qualification is independent of presence." } }
                if let Some(board)=&player.board{p {class:"muted",{board.qualifying_lines.iter().map(model::line_label).collect::<Vec<_>>().join(" · ")}}Board {board:board.clone()}}
            }
        }
        section {class:"card stack",h2 {"Qualifying players"}
            if !players.iter().any(|p|p.board.as_ref().is_some_and(|b|b.qualified)){p {"No boards qualify yet."}}
            for player in players.iter().filter(|p|p.board.as_ref().is_some_and(|b|b.qualified)){
                {let id=player.player_id.to_string();let alias=player.alias.clone();rsx!{
                    div {class:"actions",p {"{player.alias}"} StatusBadge { label: "Qualified", tone: StatusTone::Success }
                        if (game.may_operate)(){button {class:"button secondary",disabled:!game.operate(),onclick:move |_|{selected.set(Some(id.clone()));award.set(Some((id.clone(),alias.clone(),summary.view_revision)));},"Select as winner"}}
                    }
                }}
            }
        }
        if let Some((id,alias,revision))=award(){Confirm {title:format!("Award this game to {alias}?"),busy:!game.operate(),oncancel:move |_|award.set(None),onconfirm:move |_|{let id=id.clone();award.set(None);spawn(async move{if game.operate(){game.send("POST","winner",json!({"player_id":id,"expected_revision":revision})).await;}});},p {"Submitting this winner immediately resolves the game and releases its reservation. The server rechecks this board’s qualification."}}}
    }}
}
