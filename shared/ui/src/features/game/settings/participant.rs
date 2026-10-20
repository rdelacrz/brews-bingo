use super::super::{
    GameContext,
    model::{self, ViewContext},
};
use crate::{
    client,
    components::{Confirm, Notice, PasswordTextField},
};
use brews_contracts::{
    discovery::GameAvailability,
    games::{
        GameView,
        reads::{Membership, MembershipRead},
    },
};
use brews_domain::games::{GameState, validate_alias};
use dioxus::prelude::*;
use serde_json::json;
use zeroize::{Zeroize, Zeroizing};

#[component]
pub(in crate::features::game) fn ParticipantSettings() -> Element {
    let game = use_context::<GameContext>();
    let mut show = use_signal(|| false);
    let mut leave = use_signal(|| false);
    let mut switch = use_signal(|| false);
    let lifecycle = use_memo(move || game.snapshot.read().as_ref().map(model::state));
    use_effect(move || {
        let _state = lifecycle();
        leave.set(false);
        switch.set(false);
    });
    let Some(snapshot) = (game.snapshot)() else {
        return rsx! {};
    };
    let state = model::state(&snapshot);
    let context = (game.context)();
    if context == ViewContext::Account || model::terminal(&snapshot) {
        return rsx! {};
    }
    let can_switch = state == GameState::AwaitingPlayers
        && match (game.availability)() {
            Some(GameAvailability::Player {
                can_join_spectator, ..
            }) => can_join_spectator,
            Some(GameAvailability::Spectator {
                can_join_player, ..
            }) => can_join_player,
            _ => false,
        };
    rsx! {section {class:"card stack",
        h2 {"Your participation"}
        if context==ViewContext::Player {
            button {class:"button secondary",disabled:!game.can_command(),onclick:move |_|show.set(!show()),"Alias and recovery settings"}
            if show(){PlayerSettings {key:"settings:{state}"}}
        }
        if state==GameState::AwaitingPlayers {
            button {class:"button secondary",disabled:!game.can_command() || !can_switch,onclick:move |_|switch.set(true),if context==ViewContext::Player{"Switch to spectator"}else{"Switch to player"}}
        }
        button {class:"button danger",disabled:!game.can_command(),onclick:move |_|leave.set(true),"Leave game"}
        if leave(){Confirm {title:"Leave this game?".to_string(),busy:!game.can_command(),oncancel:move |_|leave.set(false),onconfirm:move |_|{leave.set(false);spawn(async move{game.send("POST","membership/leave",json!({"expected_state":state})).await;});},
            if context==ViewContext::Spectator{p {"Your spectator seat is released immediately."}}
            else if state==GameState::InProgress{p {"Your seat, assigned board, and winner eligibility remain until game end. Returning requires a valid session or enrolled recovery proof."}}
            else{p {"Your player seat, alias, and recovery answer are removed. Rejoining requires a fresh admission."}}
        }}
        if switch() && context==ViewContext::Player {Confirm {title:"Give up your player seat?".to_string(),busy:!game.can_command() || !can_switch,oncancel:move |_|switch.set(false),onconfirm:move |_|{switch.set(false);spawn(async move{game.send("POST","membership/spectator",json!({})).await;});},p {"Switching removes your player alias and recovery answer. A spectator seat is admitted only if capacity remains."}}}
        if switch() && context==ViewContext::Spectator {SwitchToPlayer {oncancel:move |_|switch.set(false)}}
    }}
}

#[component]
fn PlayerSettings() -> Element {
    let game = use_context::<GameContext>();
    let mut alias = use_signal(String::new);
    let mut answer = use_signal(|| Zeroizing::new(String::new()));
    let show_answer = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut remove = use_signal(|| false);
    let membership = use_resource(move || {
        let _revision = game.snapshot.read().as_ref().map(|s| s.view_revision());
        async move {
            client::get::<MembershipRead>(&format!("{}?view=player", game.path("membership"))).await
        }
    });
    use_drop(move || {
        if let Ok(mut answer) = answer.try_write() {
            answer.zeroize();
        }
    });
    let state = game.snapshot.read().as_ref().map(model::state);
    rsx! {div {class:"stack",
        if state==Some(GameState::AwaitingPlayers){
            form {class:"form",onsubmit:move |event|{
                event.prevent_default();
                let value=match validate_alias(&alias()){Ok(value)=>value.to_owned(),Err(_)=>{error.set(Some("Enter a nonempty alias of at most 20 printable ASCII characters.".to_string()));return;}};
                spawn(async move{game.send("PATCH","membership/alias",json!({"alias":value})).await;});
            },label {class:"field","New case-sensitive alias" input {value:alias,disabled:!game.can_command(),oninput:move |e|alias.set(e.value())}}button {class:"button secondary",r#type:"submit",disabled:!game.can_command(),"Rename alias"}}
        }
        match &*membership.read(){
            Some(Ok(read))=>match &read.membership{Membership::Player{recovery_enabled,..}=>rsx!{p {if *recovery_enabled{"Recovery enabled"}else{"Recovery not enabled"}}},_=>rsx!{}},
            Some(Err(reason))=>rsx!{Notice {message:reason.to_string()}},
            None=>rsx!{crate::components::Loading{}},
        }
        form {class:"form",onsubmit:move |event|{
            event.prevent_default();if !game.can_command(){return;}
            if answer.read().is_empty(){error.set(Some("Enter a new private recovery answer.".to_string()));return;}
            let body=json!({"recovery_answer":answer.read().as_str()});answer.write().zeroize();
            spawn(async move{game.send("PUT","membership/recovery-answer",body).await;});
        },PasswordTextField { input_id: "membership-recovery-answer", input_name: "recovery_answer", label: "New private recovery answer", autocomplete: "off", password: answer, visible: show_answer, disabled: !game.can_command(), required: false }button {class:"button secondary",r#type:"submit",disabled:!game.can_command(),"Set or replace answer"}}
        button {class:"button danger",disabled:!game.can_command(),onclick:move |_|remove.set(true),"Remove recovery answer"}
        if remove(){Confirm {title:"Remove recovery proof?".to_string(),busy:!game.can_command(),oncancel:move |_|remove.set(false),onconfirm:move |_|{remove.set(false);answer.write().zeroize();spawn(async move{game.send("DELETE","membership/recovery-answer",json!({})).await;});},p {"Lost-session recovery will be unavailable until you enroll another answer."}}}
        if let Some(message)=error(){Notice {message}}
        p {class:"muted","The saved answer is never shown. Updating it does not extend your session."}
    }}
}

#[component]
fn SwitchToPlayer(oncancel: EventHandler<()>) -> Element {
    let game = use_context::<GameContext>();
    let mut alias = use_signal(String::new);
    let mut answer = use_signal(|| Zeroizing::new(String::new()));
    let show_answer = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    use_drop(move || {
        if let Ok(mut answer) = answer.try_write() {
            answer.zeroize();
        }
    });
    rsx! {div {class:"card stack",h3 {"Become a player"}p {"This gives up the spectator seat only if the player admission succeeds."}
        form {class:"form",onsubmit:move |event|{
            event.prevent_default();if !game.can_command(){return;}
            let alias=match validate_alias(&alias()){Ok(value)=>value.to_owned(),Err(_)=>{error.set(Some("Enter a valid case-sensitive alias, at most 20 printable ASCII characters.".to_string()));return;}};
            let Some(GameView::Spectator {game_code,state:GameState::AwaitingPlayers,..})=(game.snapshot)() else{return;};
            let mut body=json!({"game_code":game_code,"alias":alias});if !answer.read().is_empty(){body["recovery_answer"]=json!(answer.read().as_str());}answer.write().zeroize();
            spawn(async move{game.send("POST","membership/player",body).await;});
        },label {class:"field","Player alias" input {value:alias,disabled:!game.can_command(),oninput:move |e|alias.set(e.value())}}PasswordTextField { input_id: "switch-player-recovery-answer", input_name: "recovery_answer", label: "Optional private recovery answer", autocomplete: "off", password: answer, visible: show_answer, disabled: !game.can_command(), required: false }
            if let Some(message)=error(){Notice {message}}
            button {class:"button",r#type:"submit",disabled:!game.can_command(),"Switch to player"}
            button {class:"button ghost",r#type:"button",onclick:move |_|oncancel.call(()),"Keep spectator seat"}
        }
    }}
}
