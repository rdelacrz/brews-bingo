use crate::{
    features::{
        auth::{Enrollment, Login, PasswordReset},
        game::{CreateGame, GamePage, Join},
        history::{HistoryDetail, HistoryPage},
        home::{Games, Home},
        recovery::Recovery,
        users::Users,
    },
    layouts::base_layout::BaseLayout,
};
use dioxus::prelude::*;

#[derive(Clone, Debug, PartialEq, Routable)]
pub(crate) enum Route {
    #[layout(BaseLayout)]
    #[route("/")]
    Home {},
    #[route("/games")]
    Games {},
    #[route("/login")]
    Login {},
    #[route("/enroll")]
    Enrollment {},
    #[route("/password-reset")]
    PasswordReset {},
    #[route("/recover")]
    Recovery {},
    #[route("/join/:code")]
    Join { code: String },
    #[route("/games/new")]
    CreateGame {},
    #[route("/games/:game_id/:view")]
    GamePage { game_id: String, view: String },
    #[route("/history")]
    HistoryPage {},
    #[route("/history/:game_id")]
    HistoryDetail { game_id: String },
    #[route("/users")]
    Users {},
    #[route("/:..segments")]
    NotFound { segments: Vec<String> },
}
#[component]
fn NotFound(segments: Vec<String>) -> Element {
    let _ = segments;
    rsx! {
        crate::components::PageTitle {
            title: "Page unavailable",
            description: "Use Home to continue.",
        }

    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn password_reset_uses_the_issued_backend_path() {
        assert_eq!(Route::PasswordReset {}.to_string(), "/password-reset");
        assert!(matches!(
            "/password-reset".parse::<Route>(),
            Ok(Route::PasswordReset {})
        ));
    }
}
