use super::{GameContext, model::ViewContext};
use crate::components::{Board, Calls, StatusBadge, StatusTone, game_state_label, utc_time};
use brews_contracts::games::GameView;
use dioxus::prelude::*;

#[component]
pub(super) fn ResultView() -> Element {
    let game = use_context::<GameContext>();
    let Some(snapshot) = (game.snapshot)() else {
        return rsx! {};
    };
    let calls = super::model::calls(&snapshot);
    let context = (game.context)();
    let history_id = (game.id)().to_string();
    let ended_at = match &snapshot {
        GameView::FinalHost { ended_at, .. } | GameView::FinalPlayer { ended_at, .. } => {
            Some(*ended_at)
        }
        _ => None,
    };
    rsx! {section {class:"card stack",
        h2 {"Game result"}
        if let Some(ended_at) = ended_at { p { class:"muted", "Ended: {utc_time(ended_at)}" } }
        match snapshot {
            GameView::FinalHost {winner,players,history_expires_at,game_code,..}=>rsx!{
                p {class:"game-code","{game_code.as_str()}"}
                if let Some(winner)=winner{p {class:"lead","Winner: {winner.alias}"}}else{p {class:"lead","Ended without a winner"}}
                p {class:"muted","History expires: {utc_time(history_expires_at)}"}
                for player in players{details {summary {"{player.alias}’s final board"}Board {board:player.board}}}
            },
            GameView::FinalPlayer {alias,board,winner,..}=>rsx!{
                if let Some(winner)=winner{p {class:"lead","Winner: {winner.alias}"}}else{p {class:"lead","Ended without a winner"}}
                h3 {"{alias}’s final board"}Board {board}
            },
            _=>rsx!{p {"This game was cancelled before play. No History entry was created."}},
        }
        Calls {calls}
        button {class:"button",disabled:!game.can_command(),onclick:move |_|{
            spawn(async move{game.exit(if context==ViewContext::Account{"/games".to_string()}else{"/".to_string()}).await;});
        },if context==ViewContext::Account{"Exit to Games"}else{"Exit to Home"}}
        if context==ViewContext::Account {
            button {class:"button secondary",disabled:!game.can_command(),onclick:move |_|{let destination=format!("/history/{history_id}");spawn(async move{game.exit(destination).await;});},"Exit and view History"}
        }
        p {class:"muted","Exit retires this final-view access. A pending response does not prove that access retirement or all cleanup has completed."}
    }}
}

#[component]
pub(super) fn AudienceResult() -> Element {
    let game = use_context::<GameContext>();
    let Some(notice) = (game.notice)() else {
        return rsx! {};
    };
    rsx! {section {class:"card stack",h2 {"Game ended"}
        StatusBadge { label: game_state_label(notice.state), tone: StatusTone::from(notice.state) }
        p {class:"muted","Ended: {utc_time(notice.ended_at)}"}
        if let Some(winner)=notice.winner{p {class:"lead","Winner: {winner.alias}"}}else{p {"Ended without a winner"}}
        Calls {calls:notice.calls}
        p {class:"muted","This is the result delivered to this spectator connection. Refreshing or reconnecting cannot retrieve it again."}
        button {class:"button",onclick:move |_|{spawn(async move {game.exit("/".to_string()).await;});},"Exit to Home"}
    }}
}
