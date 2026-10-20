//! Separate public code entry and guarded account dashboard; no participant authority.
mod dashboard;
mod policy;

use crate::{client, components::*, session::AccountSession};
use brews_contracts::discovery::GameEntry;
use brews_domain::accounts::SessionScope;
use dioxus::prelude::*;
use policy::{CODE_LENGTH, insert_code, normalize_code};

#[component]
pub fn Home() -> Element {
    rsx! {
        section { class: "stack", CodeEntry {} }
    }
}

#[component]
pub fn Games() -> Element {
    let account = use_context::<AccountSession>();
    let epoch = (account.epoch)();
    if !(account.ready)() {
        return rsx! { Loading {} };
    }
    if !account.normal() {
        let scope = account
            .session
            .read()
            .as_ref()
            .filter(|session| session.expires_at > client::now_ms())
            .map(|session| session.scope);
        return rsx! {
            section { class: "stack",
                match scope {
                    Some(SessionScope::EnrollmentOnly) => rsx! {
                        Notice { message: "Finish account setup before opening Games." }
                        a { class: "button", href: "/enroll", "Finish setup" }
                    },
                    Some(SessionScope::PasswordResetOnly) => rsx! {
                        Notice { message: "Finish your password reset, then sign in to open Games." }
                        a { class: "button", href: "/password-reset", "Finish reset" }
                    },
                    _ => rsx! {
                        Notice { message: "Sign in with a normal Host or Admin account to view Games." }
                        a { class: "button", href: "/login", "Sign in" }
                    },
                }
            }
        };
    }
    rsx! {
        section { class: "stack",
            Keyed { identity: format!("dashboard-{epoch}"), dashboard::Dashboard {} }
        }
    }
}

#[component]
fn CodeEntry() -> Element {
    let mut boxes = use_signal(|| std::array::from_fn::<_, CODE_LENGTH, _>(|_| String::new()));
    let mut inputs = use_signal(|| vec![None::<std::rc::Rc<MountedData>>; CODE_LENGTH]);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    let navigator = use_navigator();
    let submit = move |event: FormEvent| {
        event.prevent_default();
        if busy() {
            return;
        }
        let code = match normalize_code(&boxes.read().concat()) {
            Ok(code) => code,
            Err(message) => {
                error.set(Some(message.into()));
                return;
            }
        };
        busy.set(true);
        error.set(None);
        spawn(async move {
            match client::get::<GameEntry>(&format!("/api/games/codes/{code}")).await {
                Ok(_) => {
                    navigator.push(format!("/join/{code}"));
                }
                Err(failure) => error.set(Some(format!(
                    "The game could not be opened. {failure} You can check the code and retry."
                ))),
            }
            busy.set(false);
        });
    };
    rsx! {
        section {
            class: "hero",
            div {
                class: "hero-copy stack",
                PageTitle {
                    eyebrow: "ROCKVILLE BREWS",
                    title: "Good company. Great games.",
                    description: "Find your seat, follow the calls, and enjoy a little friendly competition.",
                }
                form {
                    class: "form",
                    novalidate: true,
                    onsubmit: submit,
                    fieldset {
                        disabled: busy(),
                        legend {

                            "Enter your game code"
                        }
                        p {
                            id: "game-code-help",
                            class: "muted",
                            "Eight letters and numbers from your host. Paste a full code into any box."
                        }
                        div {
                            class: "code-inputs",
                            for index in 0..CODE_LENGTH {
                                input {
                                    class: "code-box",
                                    r#type: "text",
                                    value: "{boxes.read()[index]}",
                                    aria_label: format!("Game code character {}", index + 1),
                                    aria_describedby: "game-code-help",
                                    autocomplete: "off",
                                    autocapitalize: "characters",
                                    spellcheck: "false",
                                    onmounted: move |event| inputs.write()[index] = Some(event.data()),
                                    onfocus: move |_| {
                                        if let Some(target) = inputs.read()[index].as_ref() {
                                            select_input(target);
                                        }
                                    },
                                    onpaste: move |event| {
                                        if let Some(value) = paste_text(&event.data()) {
                                            event.prevent_default();
                                            let update = insert_code(&boxes.read(), index, &value);
                                            match update {
                                                Ok((next, focus)) => {
                                                    boxes.set(next);
                                                    error.set(None);
                                                    let target = inputs.read()[focus].clone();
                                                    if let Some(target) = target {
                                                        spawn(async move {
                                                            let _ = target.set_focus(true).await;
                                                        });
                                                    }
                                                }
                                                Err(message) => error.set(Some(message.into())),
                                            }
                                        }
                                    },
                                    oninput: move |event| {
                                        let update = insert_code(&boxes.read(), index, &event.value());
                                        match update {
                                            Ok((next, focus)) => {
                                                boxes.set(next);
                                                error.set(None);
                                                let target = inputs.read()[focus].clone();
                                                if let Some(target) = target {
                                                    spawn(async move {
                                                        let _ = target.set_focus(true).await;
                                                    });
                                                }
                                            }
                                            Err(message) => error.set(Some(message.into())),
                                        }
                                    },
                                    onkeydown: move |event| {
                                        let focus = match event.key() {
                                            Key::ArrowLeft => Some(index.saturating_sub(1)),
                                            Key::ArrowRight => Some((index + 1).min(CODE_LENGTH - 1)),
                                            Key::Home => Some(0),
                                            Key::End => Some(CODE_LENGTH - 1),
                                            Key::Backspace if boxes.read()[index].is_empty() && index > 0 => {
                                                boxes.write()[index - 1].clear();
                                                Some(index - 1)
                                            }
                                            Key::Delete => {
                                                boxes.write()[index].clear();
                                                None
                                            }
                                            _ => None,
                                        };
                                        if let Some(focus) = focus {
                                            event.prevent_default();
                                            let target = inputs.read()[focus].clone();
                                            if let Some(target) = target {
                                                spawn(async move {
                                                    let _ = target.set_focus(true).await;
                                                });
                                            }
                                        }
                                    },
                                }
                            }
                        }
                    }
                    button {
                        class: "button",
                        r#type: "submit",
                        disabled: busy(),
                        if busy() {
                            "Checking code…"
                        } else {
                            "Join a game"
                        }
                    }
                }
                div {
                    class: "actions",
                    a {
                        href: "/recover",
                        "Recover player access"
                    }
                }
            }
            div {
                class: "hero-art",
                aria_hidden: "true",
                img {
                    src: "/assets/bingo-night.svg",
                    alt: "",
                    width: "420",
                    height: "420",
                }
            }
        }
        if let Some(message) = error() {
            Confirm {
                title: "Check your game code",
                busy: false,
                onconfirm: move |_| error.set(None),
                oncancel: move |_| error.set(None),
                p {
                    role: "alert",
                    "{message}"
                }
                p {
                    class: "muted",
                    "Dismiss this message to edit the code or try again."
                }
            }
        }
    }
}

fn paste_text(event: &ClipboardData) -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        let event = event.downcast::<web_sys::Event>()?;
        let clipboard = js_sys::Reflect::get(event.as_ref(), &"clipboardData".into()).ok()?;
        let function = js_sys::Reflect::get(&clipboard, &"getData".into())
            .ok()?
            .dyn_into::<js_sys::Function>()
            .ok()?;
        function
            .call1(&clipboard, &"text/plain".into())
            .ok()?
            .as_string()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = event;
        None
    }
}
fn select_input(input: &MountedData) {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        if let Some(element) = input.downcast::<web_sys::Element>()
            && let Some(input) = element.dyn_ref::<web_sys::HtmlInputElement>()
        {
            input.select();
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = input;
}
