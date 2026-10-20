use dioxus::prelude::*;
use zeroize::Zeroizing;

#[component]
pub(crate) fn PasswordTextField(
    input_id: String,
    input_name: String,
    label: String,
    autocomplete: String,
    password: Signal<Zeroizing<String>>,
    visible: Signal<bool>,
    disabled: bool,
    #[props(default = true)] required: bool,
) -> Element {
    rsx! {
        div { class: "field",
            label { r#for: input_id.clone(), "{label}" }
            div { class: "input-with-action",
                input {
                    id: input_id,
                    name: input_name,
                    r#type: if visible() { "text" } else { "password" },
                    autocomplete,
                    required,
                    value: password.read().as_str(),
                    disabled,
                    oninput: move |event| password.set(Zeroizing::new(event.value()))
                }
                button {
                    class: "input-password-toggle",
                    r#type: "button",
                    "aria-label": if visible() { "Hide password" } else { "Show password" },
                    title: if visible() { "Hide password" } else { "Show password" },
                    disabled,
                    onclick: move |_| visible.set(!visible()),
                    svg {
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        "stroke-width": "1.8",
                        "stroke-linecap": "round",
                        "stroke-linejoin": "round",
                        "aria-hidden": "true",
                        if visible() {
                            path { d: "M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7-10-7-10-7Z" }
                            circle { cx: "12", cy: "12", r: "3" }
                        } else {
                            path { d: "M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7-10-7-10-7Z" }
                            path { d: "M3 3l18 18" }
                        }
                    }
                }
            }
        }
    }
}
