mod input;
mod login;
mod password;
mod wire;

pub use login::Login;
pub(crate) use password::provide_initial_link;
pub use password::{Enrollment, PasswordReset};

fn needs_session_recheck<T>(result: &Result<T, crate::client::ClientError>) -> bool {
    // A cancelled/expired queue never dispatched; do not wait in that same lane again.
    !matches!(result, Err(crate::client::ClientError::Busy))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::ClientError;

    #[test]
    fn unsent_queue_failure_does_not_start_a_second_session_wait() {
        assert!(!needs_session_recheck(&Err::<(), _>(ClientError::Busy)));
        assert!(needs_session_recheck(&Ok::<_, ClientError>(())));
        for error in [
            ClientError::UnknownOutcome,
            ClientError::Unauthorized,
            ClientError::InvalidCredentials,
        ] {
            assert!(needs_session_recheck(&Err::<(), _>(error)));
        }
    }
}
