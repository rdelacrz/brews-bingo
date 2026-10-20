//! Recovery issuance uses fresh proof; a receipt cannot replay a session cookie.
use crate::{
    client,
    components::{Notice, PageTitle, PasswordTextField},
};
use brews_contracts::{
    discovery::GameEntry,
    games::{BoardView, GameOutcome, GameReceipt, GameView},
};
use brews_domain::{
    games::{GameCode, validate_alias},
    ids::PlayerId,
};
use dioxus::prelude::*;
use serde::Deserialize;
use serde_json::json;
use zeroize::{Zeroize, Zeroizing};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecoveredMembership {
    player_id: PlayerId,
    alias: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecoveryResponse {
    receipt: GameReceipt,
    #[serde(default)]
    membership: Option<RecoveredMembership>,
    #[serde(default)]
    game: Option<GameView>,
    #[serde(default)]
    board: Option<BoardView>,
}
impl RecoveryResponse {
    fn verified_game(&self) -> Option<String> {
        self.receipt.validate().ok()?;
        let GameOutcome::PlayerRecovered { player_id, .. } = self.receipt.outcome else {
            return None;
        };
        let GameView::Player {
            game_id,
            player_id: actual,
            alias,
            board,
            ..
        } = self.game.as_ref()?
        else {
            return None;
        };
        self.game.as_ref()?.validate().ok()?;
        let membership = self.membership.as_ref()?;
        if *game_id != self.receipt.game_id
            || *actual != player_id
            || membership.player_id != player_id
            || membership.alias != *alias
            || serde_json::to_vec(&self.board).ok()? != serde_json::to_vec(board).ok()?
        {
            return None;
        }
        Some(game_id.to_string())
    }
}

#[component]
pub fn Recovery() -> Element {
    let mut code = use_signal(String::new);
    let mut alias = use_signal(String::new);
    let mut answer = use_signal(|| Zeroizing::new(String::new()));
    let show_answer = use_signal(|| false);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut spectator_game = use_signal(|| None::<String>);
    let navigator = use_navigator();
    use_drop(move || {
        if let Ok(mut answer) = answer.try_write() {
            answer.zeroize();
        }
    });
    rsx! {div {class:"stack",
        PageTitle {eyebrow:"Return to your board".to_string(),title:"Recover player access".to_string(),description:"Use the game code, your exact current alias, and your previously enrolled answer.".to_string()}
        form {class:"form card stack",onsubmit:move |event|{
            event.prevent_default();if busy(){return;}
            let Ok(game_code)=GameCode::from_lookup(&code()) else {error.set(Some("Enter a valid eight-character game code.".to_string()));return;};
            let alias_value=match validate_alias(&alias()){Ok(value)=>value.to_owned(),Err(_)=>{error.set(Some("Enter your exact current alias.".to_string()));return;}};
            if answer.read().is_empty(){error.set(Some("Enter your enrolled recovery answer.".to_string()));return;}
            let mut body=json!({"game_code":game_code,"alias":alias_value,"recovery_answer":answer.read().as_str()});
            answer.write().zeroize();busy.set(true);error.set(None);spectator_game.set(None);
            spawn(async move{
                let result=async {
                    client::prepare_command_clock().await?;
                    let entry=client::get::<GameEntry>(&format!("/api/games/codes/{}",game_code.as_str())).await?;
                    if entry.game_code!=game_code{return Err(client::ClientError::InvalidResponse);}
                    if crate::features::game::player_entry_membership(entry.game_id, &game_code).await?==Some("spectator"){
                        spectator_game.set(Some(entry.game_id.to_string()));
                        return Err(client::ClientError::Conflict);
                    }
                    let id=client::new_command_id()?;
                    client::request::<RecoveryResponse>("POST","/api/games/player-recovery",&body,Some(&id)).await
                }.await;
                erase(&mut body);busy.set(false);
                match result {
                    Ok(response)=>if let Some(id)=response.verified_game(){navigator.push(format!("/games/{id}/player"));}else{error.set(Some("A usable session could not be verified. Re-enter your proof for a fresh recovery attempt.".to_string()));},
                    Err(reason)=>error.set(Some(if spectator_game.read().is_some(){"Leave your existing spectator participation before recovering a player. Recovery will not silently discard that seat.".to_string()}else if reason.uncertain(){"The recovery response was interrupted. Re-enter your proof for a fresh recovery attempt; a receipt cannot replay a session cookie.".to_string()}else{reason.to_string()})),
                }
            });
        },
            label {class:"field","Game code" input {value:code,disabled:busy,autocomplete:"off",oninput:move |e|code.set(e.value())}}
            label {class:"field","Exact current alias" input {value:alias,disabled:busy,autocomplete:"nickname",oninput:move |e|alias.set(e.value())}}
            PasswordTextField { input_id: "recovery-answer", input_name: "recovery_answer", label: "Your private recovery answer", autocomplete: "off", password: answer, visible: show_answer, disabled: busy(), required: false }
            if let Some(message)=error(){Notice {message}}
            if let Some(id)=spectator_game(){a {class:"button secondary",href:format!("/games/{id}/spectator"),"Restore spectator participation"}}
            button {class:"button",r#type:"submit",disabled:busy,if busy(){"Checking proof…"}else{"Recover access"}}
        }
        p {class:"muted","Recovery replaces prior player sessions and keeps the same board. Closed games, forgotten aliases, and missing answers have no alternative recovery or host override."}

    }}
}
fn erase(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(value) => value.zeroize(),
        serde_json::Value::Object(values) => values.values_mut().for_each(erase),
        _ => {}
    }
}
