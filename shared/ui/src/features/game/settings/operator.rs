use super::super::{GameContext, model};
use crate::{
    client,
    components::{Confirm, Notice, visible_text},
};
use brews_contracts::games::HostTransferTargets;
use brews_domain::games::GameState;
use dioxus::prelude::*;
use serde_json::json;

#[component]
pub(in crate::features::game) fn OperatorSettings(
    #[props(default = true)] allow_navigation: bool,
) -> Element {
    let game = use_context::<GameContext>();
    let mut cancel = use_signal(|| false);
    let mut transfer = use_signal(|| false);
    let mut target = use_signal(|| None::<(String, String, u64)>);
    let lifecycle = use_memo(move || game.snapshot.read().as_ref().map(model::state));
    use_effect(move || {
        let _state = lifecycle();
        cancel.set(false);
        target.set(None);
    });
    let Some(snapshot) = (game.snapshot)() else {
        return rsx! {};
    };
    let state = model::state(&snapshot);
    if model::terminal(&snapshot) || !(game.may_operate)() {
        return rsx! {};
    }
    rsx! {section {class:"card stack",
        h2 {"Game management"}
        button {class:"button secondary",disabled:!game.operate(),onclick:move |_|transfer.set(!transfer()),"Transfer host"}
        if transfer(){TransferTargets {on_select:move |value|target.set(Some(value))}}
        button {class:"button danger",disabled:!game.operate(),onclick:move |_|cancel.set(true),if state==GameState::InProgress{"End without winner"}else{"Cancel game"}}
        if cancel(){Confirm {
            title:if state==GameState::InProgress{"End this game without a winner?".to_string()}else{"Cancel this game?".to_string()},
            busy:!game.operate(),oncancel:move |_|cancel.set(false),onconfirm:move |_|{cancel.set(false);spawn(async move{if game.operate(){game.send("POST","cancel",json!({"confirmed":true,"expected_state":state})).await;}});},
            if state==GameState::InProgress{p {"This ends play as Cancelled and retains the started game in History. No winner will be selected."}}else{p {"This removes the unstarted game and its memberships. It creates no History entry."}}
        }}
        if let Some((id,name,revision))=target(){Confirm {
            title:format!("Transfer host to {name}?"),busy:!game.operate(),oncancel:move |_|target.set(None),onconfirm:{let id=id.clone();move |_|{let id=id.clone();target.set(None);spawn(async move{if game.operate(){game.send("POST","host-transfer",json!({"target_account_id":id,"confirmed":true,"expected_host_assignment_revision":revision})).await;}});}},
            p {"The assignment changes immediately, with no acceptance step. The outgoing ordinary host becomes read-only. Admin authority remains separate."}
            p {class:"muted","Target account ID: {id}"}
        }}
        if allow_navigation{a {class:"button ghost",href:"/games","Back to Games"}}
    }}
}

#[component]
fn TransferTargets(on_select: EventHandler<(String, String, u64)>) -> Element {
    let game = use_context::<GameContext>();
    let mut cursor = use_signal(|| None::<String>);
    let targets = use_resource(move || {
        let cursor = cursor();
        let path = format!(
            "{}{}",
            game.path("host-transfer-targets"),
            cursor.map_or_else(String::new, |cursor| format!("?cursor={cursor}"))
        );
        async move { client::get::<HostTransferTargets>(&path).await }
    });
    rsx! {div {class:"stack",
        h3 {"Eligible host targets"}
        match &*targets.read(){
            None=>rsx!{crate::components::Loading{}},
            Some(Err(error))=>rsx!{Notice {message:error.to_string()}},
            Some(Ok(targets)) if targets.validate().is_ok() && targets.game_id==(game.id)()=>rsx!{
                if targets.targets.is_empty(){p {"No eligible targets on this page."}}
                for target in &targets.targets {
                    {let id=target.account_id.to_string();let name=visible_text(&target.username);let label=name.clone();let revision=targets.host_assignment_revision;rsx!{
                        button {class:"button secondary",disabled:!game.operate(),onclick:move |_|on_select.call((id.clone(),name.clone(),revision)),"{label}"}
                    }}
                }
                if let Some(next)=targets.next_cursor {button {class:"button ghost",onclick:move |_|cursor.set(Some(next.to_string())),"Next targets"}}
            },
            _=>rsx!{Notice {message:"The target list could not be verified."}},
        }
        if cursor.read().is_some(){button {class:"button ghost",onclick:move |_|cursor.set(None),"First targets"}}
    }}
}
