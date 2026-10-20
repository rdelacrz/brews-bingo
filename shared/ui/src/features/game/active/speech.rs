use super::super::{
    GameContext,
    model::{self, Connection},
};
use dioxus::prelude::*;

#[component]
pub(super) fn SpeechControls() -> Element {
    let game = use_context::<GameContext>();
    let mut enabled = use_signal(|| false);
    let mut previous = use_signal(|| None::<u32>);
    let mut error = use_signal(|| None::<String>);
    use_effect(move || {
        let current = game.snapshot.read().as_ref().and_then(|view| {
            model::calls(view)
                .last()
                .map(|call| (call.sequence_no, call.value.clone()))
        });
        let live = (game.status)() == Connection::Live;
        if let Some((sequence, value)) = current {
            let old = *previous.peek();
            if enabled()
                && live
                && old.is_some_and(|old| old.checked_add(1) == Some(sequence))
                && !speak(&value)
            {
                error.set(Some(
                    "Spoken calls are unavailable on this device. Follow the visible call history."
                        .to_string(),
                ));
                enabled.set(false);
            }
            previous.set(Some(sequence));
        } else {
            previous.set(if live { Some(0) } else { None });
        }
    });
    use_drop(cancel);
    rsx! {
        div {
            class: "stack",
            label {
                class: "field",
                input {
                    r#type: "checkbox",
                    checked: enabled,
                    onchange: move |event| {
                        cancel();
                        enabled.set(event.checked());
                    },
                }
                "Speak new calls on this device"
            }
            p {
                class: "muted",
                "Optional device-local speech. Reconnection does not replay missed calls; use visible calls if browser audio is blocked."
            }
            if let Some(message) = error() {
                crate::components::Notice {
                    message,
                }
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn speak(value: &str) -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    let Ok(speech) = window.speech_synthesis() else {
        return false;
    };
    let Ok(utterance) = web_sys::SpeechSynthesisUtterance::new_with_text(value) else {
        return false;
    };
    speech.speak(&utterance);
    true
}
#[cfg(not(target_arch = "wasm32"))]
fn speak(_value: &str) -> bool {
    false
}
#[cfg(target_arch = "wasm32")]
fn cancel() {
    if let Some(window) = web_sys::window()
        && let Ok(speech) = window.speech_synthesis()
    {
        speech.cancel();
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn cancel() {}
#[cfg(target_arch = "wasm32")]
pub(super) fn fullscreen() {
    if let Some(document) = web_sys::window().and_then(|window| window.document()) {
        if document.fullscreen_element().is_some() {
            document.exit_fullscreen();
        } else if let Some(element) = document.get_element_by_id("game-venue") {
            let _ = element.request_fullscreen();
        }
    }
}
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn fullscreen() {}
