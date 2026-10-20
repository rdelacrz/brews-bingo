use dioxus::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT_DIALOG: AtomicU64 = AtomicU64::new(1);

#[component]
pub fn Confirm(
    title: String,
    children: Element,
    onconfirm: EventHandler<()>,
    oncancel: EventHandler<()>,
    #[props(default)] busy: bool,
) -> Element {
    let id = use_hook(|| {
        format!(
            "confirmation-{}",
            NEXT_DIALOG.fetch_add(1, Ordering::Relaxed)
        )
    });
    let title_id = format!("{id}-title");
    let mounted_id = id.clone();
    #[cfg(target_arch = "wasm32")]
    {
        let cleanup_id = id.clone();
        let opener = use_hook(browser::active_element);
        use_drop(move || {
            browser::close(&cleanup_id);
            if let Some(opener) = opener {
                let _ = opener.focus();
            }
        });
    }
    rsx! {
        dialog {
            id,
            class: "dialog",
            "aria-labelledby": title_id.clone(),
            "aria-modal": "true",
            onmounted: move |_| {
                #[cfg(target_arch = "wasm32")]
                browser::open(&mounted_id);
                #[cfg(not(target_arch = "wasm32"))]
                let _ = &mounted_id;
            },
            onkeydown: move |event| {
                if event.key() == Key::Escape {
                    event.prevent_default();
                    if !busy {
                        oncancel.call(());
                    }
                }
            },
            oncancel: move |event| {
                event.prevent_default();
                if !busy {
                    oncancel.call(());
                }
            },
            h2 {
                id: title_id,
                "{title}"
            }
            {children}
            div {
                class: "actions",
                button {
                    class: "button secondary",
                    r#type: "button",
                    autofocus: true,
                    disabled: busy,
                    onclick: move |_| oncancel.call(()),
                    "Go back"
                }
                button {
                    class: "button danger",
                    r#type: "button",
                    disabled: busy,
                    onclick: move |_| onconfirm.call(()),
                    "{title}"
                }
            }
            if busy {
                p {
                    role: "status",
                    "Waiting for the server…"
                }
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod browser {
    use js_sys::{Function, Reflect};
    use wasm_bindgen::JsCast;
    pub(super) fn active_element() -> Option<web_sys::HtmlElement> {
        web_sys::window()?
            .document()?
            .active_element()?
            .dyn_into()
            .ok()
    }
    pub(super) fn open(id: &str) {
        let Some(document) = web_sys::window().and_then(|w| w.document()) else {
            return;
        };
        let Some(element) = document.get_element_by_id(id) else {
            return;
        };
        // Native modal dialogs trap focus, inert the background and restore the opener.
        if let Ok(function) = Reflect::get(element.as_ref(), &"showModal".into())
            .and_then(|v| v.dyn_into::<Function>())
        {
            let _ = function.call0(element.as_ref());
        }
    }
    pub(super) fn close(id: &str) {
        let Some(element) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id(id))
        else {
            return;
        };
        if let Ok(function) =
            Reflect::get(element.as_ref(), &"close".into()).and_then(|v| v.dyn_into::<Function>())
        {
            let _ = function.call0(element.as_ref());
        }
    }
}
