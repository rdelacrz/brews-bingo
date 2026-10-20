use super::{ClientError, encode, new_command_id, send};
use dioxus::prelude::*;
use serde::de::DeserializeOwned;
use serde_json::Value;
use zeroize::Zeroizing;

struct RequestBody(Value);
impl Drop for RequestBody {
    fn drop(&mut self) {
        fn erase(value: &mut Value) {
            use zeroize::Zeroize;
            match value {
                Value::String(value) => value.zeroize(),
                Value::Array(values) => values.iter_mut().for_each(erase),
                Value::Object(values) => values.values_mut().for_each(erase),
                _ => {}
            }
        }
        erase(&mut self.0);
    }
}

// Serialized request strings zeroize on release; no Debug implementation can expose them.
struct RetainedCommand {
    id: String,
    method: String,
    path: String,
    body: Zeroizing<String>,
}
impl RetainedCommand {
    fn matches(&self, method: &str, path: &str, body: &str) -> bool {
        self.method == method && self.path == path && self.body.as_str() == body
    }
}
#[derive(Default)]
struct RetryState {
    retained: Option<RetainedCommand>,
    uncertain: bool,
}
impl RetryState {
    fn begin(
        &mut self,
        method: &str,
        path: &str,
        body: Zeroizing<String>,
        id: impl FnOnce() -> Result<String, ClientError>,
    ) -> Result<String, ClientError> {
        if let Some(original) = &self.retained {
            return if original.matches(method, path, &body) {
                Ok(original.id.clone())
            } else {
                Err(ClientError::ChangedCommand)
            };
        }
        let id = id()?;
        self.retained = Some(RetainedCommand {
            id: id.clone(),
            method: method.to_owned(),
            path: path.to_owned(),
            body,
        });
        Ok(id)
    }
    fn finish(&mut self, error: Option<&ClientError>) {
        if error.is_some_and(ClientError::uncertain) {
            self.uncertain = true;
        } else if !(self.uncertain && error.is_some()) {
            self.retained = None;
            self.uncertain = false;
        }
    }
}

#[derive(Clone, Copy)]
pub struct CommandState {
    pub busy: Signal<bool>,
    pub error: Signal<Option<ClientError>>,
    retry: Signal<RetryState>,
}
impl PartialEq for CommandState {
    fn eq(&self, other: &Self) -> bool {
        self.busy == other.busy && self.error == other.error
    }
}
pub fn use_command() -> CommandState {
    let busy = use_signal(|| false);
    let error = use_signal(|| None);
    let mut retry = use_signal(RetryState::default);
    use_drop(move || {
        if let Ok(mut state) = retry.try_write() {
            state.retained = None;
        }
    });
    CommandState { busy, error, retry }
}
impl CommandState {
    pub fn unresolved(self) -> bool {
        self.retry.read().uncertain
    }
    pub async fn run<T: DeserializeOwned>(
        self,
        method: &str,
        path: &str,
        body: Value,
    ) -> Result<T, ClientError> {
        self.run_until(method, path, body, |_| true).await
    }
    pub async fn run_until<T: DeserializeOwned>(
        mut self,
        method: &str,
        path: &str,
        body: Value,
        complete: impl FnOnce(&T) -> bool,
    ) -> Result<T, ClientError> {
        let body = RequestBody(body);
        if self.busy.try_read().map(|busy| *busy).unwrap_or(true) {
            return Err(ClientError::Busy);
        }
        let encoded = match encode(&body.0) {
            Ok(encoded) => encoded,
            Err(error) => {
                self.publish_error(error);
                return Err(error);
            }
        };
        drop(body);
        let id = match self
            .retry
            .try_write()
            .ok()
            .map(|mut state| state.begin(method, path, encoded, new_command_id))
        {
            Some(Ok(id)) => id,
            Some(Err(error)) => {
                self.publish_error(error);
                return Err(error);
            }
            None => return Err(ClientError::BrowserUnavailable),
        };
        // No await occurs before busy is set, so concurrent clicks cannot both begin.
        self.busy.set(true);
        self.error.set(None);
        let result = {
            // Keep an independent zeroizing copy only for the duration of this request.
            let encoded = self
                .retry
                .read()
                .retained
                .as_ref()
                .map(|r| Zeroizing::new(r.body.to_string()));
            match encoded {
                Some(encoded) => send(method, path, Some(&encoded), Some(&id)).await,
                None => Err(ClientError::ChangedCommand),
            }
        };
        if let Ok(mut state) = self.retry.try_write() {
            if result.as_ref().is_ok_and(|value| !complete(value)) {
                state.uncertain = true;
            } else {
                state.finish(result.as_ref().err());
            }
        }
        if let Ok(mut busy) = self.busy.try_write() {
            *busy = false;
        }
        if let Ok(mut error) = self.error.try_write() {
            *error = result.as_ref().err().copied();
        }
        result
    }
    fn publish_error(mut self, value: ClientError) {
        if let Ok(mut error) = self.error.try_write() {
            *error = Some(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn begin(state: &mut RetryState, body: &str, id: &str) -> Result<String, ClientError> {
        state.begin(
            "POST",
            "/api/games",
            Zeroizing::new(body.to_owned()),
            || Ok(id.to_owned()),
        )
    }
    #[test]
    fn uncertain_retry_preserves_id_and_rejects_changed_input() {
        let mut state = RetryState::default();
        assert_eq!(
            begin(&mut state, "{}", "original"),
            Ok("original".to_owned())
        );
        state.finish(Some(&ClientError::UnknownOutcome));
        assert_eq!(
            begin(&mut state, "{}", "replacement"),
            Ok("original".to_owned())
        );
        assert_eq!(
            begin(&mut state, "{\"new\":true}", "replacement"),
            Err(ClientError::ChangedCommand)
        );
        assert_eq!(
            state.begin(
                "DELETE",
                "/api/games",
                Zeroizing::new("{}".to_owned()),
                || Ok("other".to_owned())
            ),
            Err(ClientError::ChangedCommand)
        );
        assert_eq!(
            state.begin(
                "POST",
                "/api/other",
                Zeroizing::new("{}".to_owned()),
                || Ok("other".to_owned())
            ),
            Err(ClientError::ChangedCommand)
        );
    }
    #[test]
    fn success_and_definite_rejections_release_the_original() {
        for error in [
            None,
            Some(ClientError::InvalidInput),
            Some(ClientError::Unauthorized),
        ] {
            let mut state = RetryState::default();
            assert!(begin(&mut state, "{}", "first").is_ok());
            state.finish(error.as_ref());
            assert_eq!(begin(&mut state, "{}", "second"), Ok("second".to_owned()));
        }
    }
    #[test]
    fn offline_retry_does_not_discard_an_unknown_original() {
        let mut state = RetryState::default();
        assert!(begin(&mut state, "{}", "original").is_ok());
        state.finish(Some(&ClientError::UnknownOutcome));
        state.finish(Some(&ClientError::Offline));
        assert_eq!(
            begin(&mut state, "{}", "replacement"),
            Ok("original".to_owned())
        );
        state.finish(None);
        assert!(state.retained.is_none());
    }
    #[test]
    fn pending_or_unknown_command_survives_replay_denial_until_confirmed() {
        for error in [
            ClientError::Offline,
            ClientError::Forbidden,
            ClientError::Conflict,
            ClientError::StaleCommand,
        ] {
            let mut state = RetryState::default();
            assert!(begin(&mut state, "{}", "original").is_ok());
            state.uncertain = true;
            state.finish(Some(&error));
            assert!(state.uncertain);
            assert_eq!(
                begin(&mut state, "{}", "replacement"),
                Ok("original".to_owned())
            );
            state.finish(None);
            assert!(!state.uncertain && state.retained.is_none());
        }
    }
    #[test]
    fn no_id_or_diagnostics_come_from_request_secrets() {
        let mut state = RetryState::default();
        assert_eq!(
            state.begin(
                "POST",
                "/api/games",
                Zeroizing::new("private-marker".to_owned()),
                || Err(ClientError::BrowserUnavailable)
            ),
            Err(ClientError::BrowserUnavailable)
        );
        assert!(state.retained.is_none());
    }
}
