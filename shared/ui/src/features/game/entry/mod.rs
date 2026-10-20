use super::command::{Feedback, use_game_command};
use crate::{
    client,
    components::{Keyed, Notice, PageTitle, PasswordTextField},
};
use brews_contracts::{
    discovery::GameEntry,
    games::{
        GameOutcome, GameResponse, ParticipantIdentity,
        reads::{Membership, MembershipRead},
    },
};
use brews_domain::games::{GameCode, validate_alias};
use dioxus::prelude::*;
use serde_json::json;
use zeroize::{Zeroize, Zeroizing};

#[component]
pub fn Join(code: String) -> Element {
    let Ok(code) = GameCode::from_lookup(&code) else {
        return rsx! {Notice {message:"Enter an eight-character game code using letters and digits."} a {href:"/","Use another code"}};
    };
    rsx! {Keyed {identity:format!("join:{}",code.as_str()),Lookup {code:code.as_str().to_string()}}}
}

#[component]
fn Lookup(code: String) -> Element {
    let path = format!("/api/games/codes/{code}");
    let mut lookup = use_resource(move || {
        let path = path.clone();
        async move { client::get::<GameEntry>(&path).await }
    });
    rsx! {div {class:"stack",
        PageTitle {eyebrow:"Welcome to the table".to_string(),title:"Join the game".to_string(),description:format!("Game code {code}")}
        match &*lookup.read() {
            None=>rsx!{crate::components::Loading{}},
            Some(Err(error))=>rsx!{Notice {message:error.to_string()}},
            Some(Ok(entry))=>rsx!{Keyed {identity:entry.game_id.to_string(),Admission {entry:entry.clone()}}},
        }
        button {class:"button ghost",onclick:move |_|lookup.restart(),"Refresh entry availability"}
        div {class:"actions",a {class:"button ghost",href:"/recover","Recover existing player"}a {class:"button ghost",href:"/","Use another code"}}
    }}
}

// Probe both roles before minting either cookie. Restoration leads to the
// existing participation controls, where a role switch is explicit and atomic.
pub(in crate::features) async fn existing_membership(
    id: brews_domain::ids::GameId,
) -> Result<Option<&'static str>, client::ClientError> {
    Ok(probe_membership(id).await?.0)
}

async fn probe_membership(
    id: brews_domain::ids::GameId,
) -> Result<(Option<&'static str>, bool), client::ClientError> {
    let mut existing = None;
    let mut rejected_spectator = false;
    for role in ["player", "spectator"] {
        let read =
            client::get::<MembershipRead>(&format!("/api/games/{id}/membership?view={role}")).await;
        rejected_spectator |=
            role == "spectator" && matches!(&read, Err(client::ClientError::Unauthorized));
        if membership_present(role, read)? {
            if existing.is_some() {
                return Err(client::ClientError::Conflict);
            }
            existing = Some(role);
        }
    }
    Ok((existing, rejected_spectator))
}

// Membership reads cannot clear HttpOnly cookies. Fresh code proof lets the
// selected spectator-context endpoint clear only a rejected credential.
pub(in crate::features) async fn player_entry_membership(
    id: brews_domain::ids::GameId,
    code: &GameCode,
) -> Result<Option<&'static str>, client::ClientError> {
    let (existing, rejected_spectator) = probe_membership(id).await?;
    if !rejected_spectator {
        return Ok(existing);
    }
    let cleanup = client::request::<GameResponse>(
        "POST",
        &format!("/api/games/{id}/spectator-admission-context"),
        &json!({"game_code": code}),
        None,
    )
    .await;
    validate_spectator_cleanup(cleanup, client::now_ms())?;
    // The cleanup response has completed, including its Set-Cookie. A spectator
    // restored in the meantime must still block opposite-role issuance.
    existing_membership(id).await
}

fn validate_spectator_cleanup(
    response: Result<GameResponse, client::ClientError>,
    now: i64,
) -> Result<(), client::ClientError> {
    match response {
        Err(client::ClientError::Unauthorized) => Ok(()),
        Ok(GameResponse::AdmissionContext { expires_at }) if now >= 0 && expires_at > now => Ok(()),
        Ok(_) => Err(client::ClientError::InvalidResponse),
        Err(error) => Err(error),
    }
}

fn membership_present(
    role: &str,
    read: Result<MembershipRead, client::ClientError>,
) -> Result<bool, client::ClientError> {
    match read {
        Ok(read) => match (role, read.membership) {
            ("player", Membership::Player { .. }) | ("spectator", Membership::Spectator { .. }) => {
                Ok(true)
            }
            _ => Err(client::ClientError::InvalidResponse),
        },
        Err(client::ClientError::Unauthorized | client::ClientError::NotFound) => Ok(false),
        Err(error) => Err(error),
    }
}

#[component]
fn Admission(entry: GameEntry) -> Element {
    let id = entry.game_id;
    let mut membership = use_resource(move || existing_membership(id));
    rsx! {
        match &*membership.read() {
            None => rsx! { crate::components::Loading {} },
            Some(Err(error)) => rsx! { Notice { message: error.to_string() } },
            Some(Ok(Some(role))) => rsx! {
                p { "You already have a {role} session. Restore it to continue or use its participation controls to switch roles." }
                a { class: "button", href: format!("/games/{id}/{role}"), "Restore existing {role} session" }
            },
            Some(Ok(None)) => rsx! { FreshAdmission { entry } },
        }
        button { class: "button ghost", onclick: move |_| membership.restart(), "Recheck existing participation" }
    }
}

#[component]
fn FreshAdmission(entry: GameEntry) -> Element {
    let mut role = use_signal(|| "player".to_string());
    let mut submitted_role = use_signal(|| None::<String>);
    let mut alias = use_signal(String::new);
    let mut answer = use_signal(|| Zeroizing::new(String::new()));
    let show_answer = use_signal(|| false);
    let mut context_busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let command = use_game_command();
    let navigator = use_navigator();
    use_drop(move || {
        if let Ok(mut answer) = answer.try_write() {
            answer.zeroize();
        }
    });
    use_effect(move || {
        if let Some(response) = (command.result)() {
            let context = match response {
                GameResponse::PlayerJoined { .. } => Some("player".to_string()),
                GameResponse::ParticipantChanged { receipt } => match receipt.outcome {
                    GameOutcome::ParticipantChanged {
                        identity: ParticipantIdentity::Player { .. },
                        ..
                    } => Some("player".to_string()),
                    GameOutcome::ParticipantChanged {
                        identity: ParticipantIdentity::Spectator { .. },
                        ..
                    } => Some("spectator".to_string()),
                    _ => None,
                },
                GameResponse::Committed { .. } => submitted_role(),
                _ => None,
            };
            if let Some(context) = context {
                navigator.push(format!("/games/{}/{context}", entry.game_id));
            }
        }
    });
    let id = entry.game_id;
    let code = entry.game_code.as_str().to_string();
    let disabled = command.blocked() || context_busy();
    let allowed = if role() == "player" {
        entry.can_join_player
    } else {
        entry.can_join_spectator
    };
    rsx! {section {class:"card stack",
        h2 {"Choose how to participate"}
        p {class:"badge","{super::model::state_label(entry.state)}"}
        form {class:"form",onsubmit:move |event|{
            event.prevent_default();
            if disabled || !allowed {return;}
            let selected=role();
            let alias_value=if selected=="player" {match validate_alias(&alias()){Ok(value)=>value.to_owned(),Err(_)=>{error.set(Some("Use a nonempty alias of at most 20 printable ASCII characters. Aliases are case-sensitive.".to_string()));return;}}}else{String::new()};
            let code=code.clone();
            let game_code=entry.game_code.clone();
            let mut body=json!({"game_code":code});
            if selected=="player" {
                body["alias"]=json!(alias_value);
                if !answer.read().is_empty(){body["recovery_answer"]=json!(answer.read().as_str());}
            }
            answer.write().zeroize();
            submitted_role.set(Some(selected.clone()));
            context_busy.set(true);error.set(None);
            spawn(async move {
                let participation = if selected == "player" {
                    player_entry_membership(id, &game_code).await
                } else {
                    existing_membership(id).await
                };
                match participation {
                    Ok(Some(role)) => {
                        super::command::erase(&mut body);
                        context_busy.set(false);
                        navigator.push(format!("/games/{id}/{role}"));
                        return;
                    }
                    Ok(None) => {}
                    Err(reason) => {
                        super::command::erase(&mut body);
                        error.set(Some(reason.to_string()));
                        context_busy.set(false);
                        return;
                    }
                }
                let action=if selected=="player"{"admission-context"}else{"spectator-admission-context"};
                let context=client::request::<GameResponse>("POST",&format!("/api/games/{id}/{action}"),&json!({"game_code":code}),None).await;
                match context {
                    Ok(GameResponse::AdmissionContext {expires_at}) if expires_at>client::now_ms()=>{
                        let action=if selected=="player"{"players"}else{"spectators"};
                        command.send("POST",&format!("/api/games/{id}/{action}"),body).await;
                    }
                    Ok(_)=>{super::command::erase(&mut body);error.set(Some("Admission could not be verified. Refresh availability before trying again.".to_string()));}
                    Err(reason)=>{super::command::erase(&mut body);error.set(Some(reason.to_string()));}
                }
                context_busy.set(false);
            });
        },
            label {class:"field",input {r#type:"radio",name:"join-role",value:"player",checked:role()=="player",disabled,onchange:move |_|{answer.write().zeroize();role.set("player".to_string());}}"Play"}
            if !entry.can_join_player {p {class:"muted","New player entry is unavailable. Existing players can restore their session or use recovery."}}
            label {class:"field",input {r#type:"radio",name:"join-role",value:"spectator",checked:role()=="spectator",disabled,onchange:move |_|{answer.write().zeroize();role.set("spectator".to_string());}}"Spectate"}
            if !entry.can_join_spectator {p {class:"muted","Spectator entry is full, disabled, or unavailable."}}
            if role()=="player" {
                label {class:"field","Your alias" input {value:alias,disabled,autocomplete:"nickname",oninput:move |e|alias.set(e.value())}}
                p {class:"muted","Remember your exact alias, including its case. Maximum 20 printable ASCII characters."}
                PasswordTextField { input_id: "admission-recovery-answer", input_name: "recovery_answer", label: "What do you like most about Brews? (optional recovery answer)", autocomplete: "off", password: answer, visible: show_answer, disabled, required: false }
                p {class:"muted","This is private recovery proof. Without an enrolled answer and your exact alias, lost player access cannot be recovered."}
            }
            button {class:"button",r#type:"submit",disabled:disabled || !allowed,if role()=="player"{"Join as player"}else{"Join as spectator"}}
        }
        if let Some(message)=error(){Notice {message}}
        Feedback {command,game_id:id.to_string(),view:submitted_role().unwrap_or_else(&*role)}
        p {class:"muted","If an admission response was lost, verify the selected session below. Receipts cannot replay a missing session cookie."}
        for selected in ["player","spectator"] {
            button {class:"button secondary",disabled,onclick:move |_|async move{
                context_busy.set(true);
                match client::get::<MembershipRead>(&format!("/api/games/{id}/membership?view={selected}")).await {
                    Ok(_)=>{navigator.push(format!("/games/{id}/{selected}"));}
                    Err(reason)=>error.set(Some(reason.to_string())),
                }
                context_busy.set(false);
            },"Restore existing {selected} session"}
        }
    }}
}

#[cfg(test)]
#[allow(clippy::expect_used, reason = "Fixed test fixtures.")]
mod tests {
    use super::*;
    use client::ClientError;

    #[test]
    fn only_completed_spectator_cleanup_allows_rechecking_participation() {
        assert_eq!(
            validate_spectator_cleanup(Err(ClientError::Unauthorized), 100),
            Ok(())
        );
        assert_eq!(
            validate_spectator_cleanup(Ok(GameResponse::AdmissionContext { expires_at: 200 }), 100),
            Ok(())
        );
        for error in [
            ClientError::Unavailable,
            ClientError::Conflict,
            ClientError::NotFound,
            ClientError::Forbidden,
            ClientError::UnknownOutcome,
        ] {
            assert_eq!(validate_spectator_cleanup(Err(error), 100), Err(error));
        }
        for (expires_at, now) in [(100, 100), (99, 100), (200, -1)] {
            assert_eq!(
                validate_spectator_cleanup(Ok(GameResponse::AdmissionContext { expires_at }), now),
                Err(ClientError::InvalidResponse)
            );
        }
    }

    #[test]
    fn only_absent_membership_allows_fresh_admission() {
        for role in ["player", "spectator"] {
            assert_eq!(
                membership_present(role, Err(ClientError::Unauthorized)),
                Ok(false)
            );
            assert_eq!(
                membership_present(role, Err(ClientError::NotFound)),
                Ok(false)
            );
            for error in [
                ClientError::Unavailable,
                ClientError::Forbidden,
                ClientError::InvalidResponse,
            ] {
                assert_eq!(membership_present(role, Err(error)), Err(error));
            }
        }
    }

    #[test]
    fn either_existing_role_requires_restoration() {
        use brews_contracts::games::reads::MembershipAccess;
        let player = || MembershipRead {
            membership: Membership::Player {
                player_id: "01890f3e-53b7-7d28-9b05-4f65092d5701"
                    .parse()
                    .expect("fixture"),
                alias: "Guest".into(),
                access: MembershipAccess::OriginalLive,
                session_expires_at: 200,
                recovery_enabled: false,
            },
            view_revision: 1,
        };
        let spectator = || MembershipRead {
            membership: Membership::Spectator {
                spectator_id: "01890f3e-53b7-7d28-9b05-4f65092d5702"
                    .parse()
                    .expect("fixture"),
                access: MembershipAccess::OriginalLive,
                session_expires_at: 200,
            },
            view_revision: 1,
        };
        assert_eq!(membership_present("player", Ok(player())), Ok(true));
        assert_eq!(membership_present("spectator", Ok(spectator())), Ok(true));
        assert_eq!(
            membership_present("player", Ok(spectator())),
            Err(ClientError::InvalidResponse)
        );
    }
}
