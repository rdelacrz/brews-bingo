use crate::{
    components::Notice, layouts::base_layout::BrandName, routes::Route,
    session::provide_account_session,
};
use dioxus::prelude::*;

const DEFAULT_APPLICATION_NAME: &str = "Brews Bingo";

#[component]
pub fn App() -> Element {
    crate::features::auth::provide_initial_link();
    let configuration = use_hook(|| {
        brews_config::env::frontend::get_frontend_config().map(|config| {
            config
                .application_name
                .clone()
                .unwrap_or_else(|| DEFAULT_APPLICATION_NAME.to_owned())
        })
    });
    provide_account_session();
    use_context_provider(|| {
        BrandName(
            configuration
                .as_ref()
                .ok()
                .cloned()
                .unwrap_or_else(|| DEFAULT_APPLICATION_NAME.to_owned()),
        )
    });
    match configuration {
        Ok(_) => {
            rsx! { document::Meta { name: "referrer", content: "no-referrer" } Router::<Route> {} }
        }
        Err(_) => {
            rsx! { main { class: "app-main", Notice { message: "The app configuration is unavailable." } } }
        }
    }
}
