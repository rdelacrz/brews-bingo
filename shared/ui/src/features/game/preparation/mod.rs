mod configuration;
use super::{
    CodeBadge, GameContext, Rules,
    command::{Feedback, use_game_command},
};
use crate::{
    client,
    components::{Confirm, Keyed, Notice, PageTitle, utc_time},
    session::AccountSession,
};
use brews_contracts::{
    discovery::{CurrentGame, GameSlot},
    games::{GameResponse, GameView},
};
use brews_domain::games::GameConfiguration;
use dioxus::prelude::*;
use serde_json::json;

#[component]
pub fn CreateGame() -> Element {
    let account = use_context::<AccountSession>();
    let epoch = (account.epoch)();
    if !(account.ready)() && account.session.read().is_none() {
        return rsx! {crate::components::Loading{}};
    }
    if (account.ready)() && !account.normal() {
        return rsx! { Notice {message:"Log in with a normal host or admin session to create a game."} a {class:"button",href:"/login","Host / admin login"} };
    }
    rsx! { Keyed {identity:format!("create:{epoch}"),CreateForm {}} }
}

#[component]
fn CreateForm() -> Element {
    let account = use_context::<AccountSession>();
    let mut command = use_game_command();
    let navigator = use_navigator();
    let mut discard = use_signal(|| false);
    let mut conflict_current = use_signal(|| None::<CurrentGame>);
    let slot = use_resource(|| async { client::get::<CurrentGame>("/api/games/current").await });
    use_effect(move || {
        if let Some(response) = (command.result)() {
            let id = match response {
                GameResponse::Created { game, .. } => Some(game.game_id),
                GameResponse::Committed { receipt } => Some(receipt.game_id),
                _ => None,
            };
            if let Some(id) = id {
                navigator.push(format!("/games/{id}/account"));
            }
        }
    });
    let free = matches!(&*slot.read(),Some(Ok(current)) if current.slot==GameSlot::Free && current.game.is_none());
    let occupied = conflict_current().or_else(|| {
        slot.read()
            .as_ref()
            .and_then(|value| value.as_ref().ok().cloned())
    });
    rsx! { div {class:"stack",
        PageTitle {eyebrow:"Host a round".to_string(),title:"Create a game".to_string(),description:"Save the setup first, then open the lobby when you are ready.".to_string()}
        match &*slot.read() {
            None=>rsx!{crate::components::Loading {}},
            Some(Err(_error))=>rsx!{},
            Some(Ok(current)) if !free=>rsx!{
                if !(command.needs_refresh)() {
                    Notice {message:"The global game slot is occupied or coordinating. A second game cannot be created."}
                    if let Some(game)=&current.game { a {class:"button secondary",href:format!("/games/{}/account",game.game_id),"Open current game"} }
                }
            },
            _=>rsx!{},
        }
        Feedback {command,enabled:account.normal()}
        if (command.needs_refresh)() {
            if let Some(game)=occupied.and_then(|current| current.game) {
                a {class:"button secondary",href:format!("/games/{}/account",game.game_id),"Open current game"}
            } else {
                a {class:"button secondary",href:"/games","Check current game"}
            }
        }
        configuration::ConfigurationEditor {initial:GameConfiguration::default(),busy:command.blocked(),label:"Create game".to_string(),on_save:move |configuration| {
            if account.normal() { spawn(async move {
                command.send("POST","/api/games",json!({"configuration":configuration})).await;
                if (command.needs_refresh)() && account.normal() {
                    let current = client::get::<CurrentGame>("/api/games/current").await.ok();
                    if account.normal() {
                        let message = if current.as_ref().is_some_and(|current| current.slot != GameSlot::Free) {
                            "The game slot is occupied or coordinating. Check the current game before trying again."
                        } else {
                            "Creation conflicted with server state. Check Games for an existing game before trying again."
                        };
                        conflict_current.set(current);
                        command.error.set(Some(message.to_string()));
                    }
                }
            }); }
        }}
        button {class:"button ghost",onclick:move |_| discard.set(true),"Back to Games"}
        if discard() { Confirm {title:"Discard this local setup?".to_string(),busy:command.blocked(),onconfirm:move |_| {navigator.push("/games");},oncancel:move |_| discard.set(false),p {"This leaves the form. Any successfully created game remains saved."}} }
    } }
}

#[component]
pub(super) fn SavedSetup() -> Element {
    let game = use_context::<GameContext>();
    let mut publish = use_signal(|| false);
    let mut dirty = use_signal(|| false);
    let mut discard = use_signal(|| false);
    let navigator = use_navigator();
    let Some(GameView::Host {
        game: summary,
        configuration,
        idle_cancel_due_at,
        game_code,
        ..
    }) = (game.snapshot)()
    else {
        return rsx! {};
    };
    let configuration_for_form = configuration.clone();
    rsx! { div {class:"stack",
        h2 {"Game setup"}
        if let Some(deadline)=idle_cancel_due_at {p {class:"muted","Idle cancellation deadline: {utc_time(deadline)}"}}
        if let Some(code)=game_code {CodeBadge {code:code.as_str().to_string()}}
        configuration::ConfigurationEditor {initial:configuration_for_form,busy:!game.operate(),label:"Save changes".to_string(),on_dirty:move |changed|dirty.set(changed),on_save:move |configuration:GameConfiguration| {
            if game.operate() { let mut body=json!(configuration); body["expected_revision"]=json!(summary.view_revision); spawn(async move{game.send("PATCH","configuration",body).await;}); }
        }}
        Rules {configuration}
        button {class:"button",disabled:!game.operate() || dirty(),onclick:move |_| {if !dirty(){publish.set(true);}},"Open saved lobby"}
        if dirty(){p {class:"notice","Save your changes before opening the lobby."}}
        p {class:"muted","Save all edits before opening the lobby. Publication fixes the saved configuration and issues a game code."}
        if publish() {Confirm {title:"Publish the saved setup?".to_string(),busy:!game.operate(),onconfirm:move |_| {publish.set(false);spawn(async move{game.send("POST","lobby",json!({"expected_revision":summary.view_revision})).await;});},oncancel:move |_|publish.set(false),p {"The lobby uses the last saved configuration. It cannot be edited after publication."}}}
        button {class:"button ghost",onclick:move |_|{if dirty(){discard.set(true);}else{navigator.push("/games");}},"Back to Games"}
        if discard(){Confirm {title:"Discard unsaved setup edits?".to_string(),busy:game.command.blocked(),oncancel:move |_|discard.set(false),onconfirm:move |_|{navigator.push("/games");},p {"Your last saved setup remains available. This does not cancel the game."}}}
    } }
}
