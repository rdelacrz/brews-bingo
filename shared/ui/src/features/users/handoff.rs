use crate::{client, components::*};
use dioxus::prelude::*;
use zeroize::Zeroizing;

// Never implement Debug or serialize the handoff. Dropping the panel erases its buffer.
pub(super) struct PrivateLink {
    pub url: Zeroizing<String>,
    pub purpose: &'static str,
    pub target: String,
    pub expires_at: i64,
}

#[component]
pub(super) fn Handoff(mut link: Signal<Option<PrivateLink>>) -> Element {
    let mut message = use_signal(|| None::<String>);
    let clock = use_clock();
    use_effect(move || {
        let now = clock();
        if link
            .peek()
            .as_ref()
            .is_some_and(|link| link.expires_at <= now)
        {
            link.set(None);
            message.set(Some("The private link expired and was cleared. Explicitly issue a successor link if needed.".into()));
        }
    });
    let data = link.read();
    rsx! {
        if let Some(link_value) = data.as_ref().filter(|link| link.expires_at > client::now_ms()) {
            section { class: "card stack", aria_label: "Private link handoff",
                h2 { "Private {link_value.purpose} link" }
                p { "For {link_value.target}. Deliver this link privately to its intended recipient." }
                p { "Expires: {utc_time(link_value.expires_at)}" }
                Notice { message: "This link is shown only now. Dismissing it clears this copy. A retry returns a receipt without the URL; a successor reissue invalidates the earlier link." }
                label { r#for: "private-account-link", "Private access URL" }
                textarea { id: "private-account-link", readonly: true, value: link_value.url.as_str(), rows: "3", spellcheck: "false", autocomplete: "off" }
                div { class: "actions",
                    button { class: "button", onclick: move |_| async move {
                        let value = link.read().as_ref().filter(|link| link.expires_at > client::now_ms()).map(|link| link.url.clone());
                        if let Some(value) = value {
                            message.set(Some(match client::copy_text(value.as_str()).await { Ok(()) => "Copied. Deliver the link privately.".into(), Err(_) => "Copy was unavailable. Select the URL and copy it manually.".into() }));
                        }
                    }, "Copy private link" }
                    button { class: "button secondary", onclick: move |_| { link.set(None); message.set(Some("Private link cleared. Issue a successor link if delivery is still needed.".into())); }, "Dismiss and clear" }
                }
            }
        }
        if let Some(message) = message() { p { role: "status", "{message}" } }
    }
}
