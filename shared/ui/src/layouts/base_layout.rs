use crate::{
    components::{Loading, Notice},
    routes::Route,
    session::AccountSession,
};
use dioxus::prelude::*;

#[derive(Clone)]
pub(crate) struct BrandName(pub(crate) String);

#[component]
pub(crate) fn BaseLayout() -> Element {
    let account = use_context::<AccountSession>();
    let brand = use_context::<BrandName>();
    let enrollment_route = matches!(use_route::<Route>(), Route::Enrollment {});
    let show_account_controls = account.normal() && !enrollment_route;
    let mut account_menu_open = use_signal(|| false);
    let mut logout_busy = use_signal(|| false);
    let mut logout_error = use_signal(|| None::<crate::client::ClientError>);
    rsx! {
        button { class: "skip-link", r#type: "button", onclick: move |_| focus_main_content(), "Skip to content" }
        header { class: "app-header",
            Link { class: "brand", to: Route::Home {}, span { class: "brand-mark", "aria-hidden": "true", "B" } "{brand.0}" }
            nav { class: "nav", "aria-label": "Main navigation",
                if show_account_controls {
                    if let Some(identity) = account.account.read().as_ref() {
                        div { class: "account-menu",
                            button {
                                class: "account-menu-toggle",
                                r#type: "button",
                                "aria-label": "Account menu",
                                "aria-haspopup": "true",
                                "aria-expanded": account_menu_open(),
                                "aria-controls": "account-menu-panel",
                                onclick: move |_| account_menu_open.set(!account_menu_open()),
                                "{crate::components::visible_text(&identity.username)}"
                            }
                            if account_menu_open() {
                                div { class: "account-menu-panel", id: "account-menu-panel",
                                    Link { to: Route::Games {}, onclick: move |_| account_menu_open.set(false), "Games" }
                                    Link { to: Route::HistoryPage {}, onclick: move |_| account_menu_open.set(false), "History" }
                                    if account.admin() { Link { to: Route::Users {}, onclick: move |_| account_menu_open.set(false), "Users" } }
                                    hr { class: "account-menu-divider" }
                                    button {
                                        class: "button ghost account-menu-logout",
                                        r#type: "button",
                                        disabled: logout_busy(),
                                        onclick: move |_| async move {
                                            account_menu_open.set(false);
                                            logout_busy.set(true);
                                            let result = crate::session::logout(account).await;
                                            logout_error.set(result.err());
                                            logout_busy.set(false);
                                        },
                                        if logout_busy() { "Logging out…" } else if logout_error().is_some() { "Retry logout" } else { "Log out" }
                                    }
                                }
                            }
                        }
                    }
                } else { Link { class: "button secondary", to: Route::Login {}, "Login" } }
                if logout_error.read().is_some() && !enrollment_route {
                    button { class: "button ghost", r#type: "button", disabled: logout_busy(), onclick: move |_| async move {
                        logout_busy.set(true);
                        let result = crate::session::logout(account).await;
                        logout_error.set(result.err());
                        logout_busy.set(false);
                    }, if logout_busy() { "Logging out…" } else { "Retry logout" } }
                }
            }
        }
        main { class: "app-main", id: "main-content", tabindex: "-1",
            if let Some(error) = logout_error() { Notice { message: format!("Account content cleared locally. Server logout is not yet confirmed. {error}") } }
            if let Some(error) = account.failure() { Notice { message: format!("Account access could not be revalidated. Protected account content has been cleared. {error}") } }
            if !(account.ready)() { Loading {} }
            Outlet::<Route> {}
        }
        footer { class: "app-footer", p { "Made for good company. Rockville Brews." } }
    }
}

fn focus_main_content() {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        if let Some(main) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id("main-content"))
            .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
        {
            let _ = main.focus();
        }
    }
}
