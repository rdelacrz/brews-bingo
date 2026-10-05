#[derive(
    Clone,
    Copy,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    Eq,
    PartialEq,
    strum::Display,
    strum::EnumString,
)]
#[strum(serialize_all = "snake_case")]
pub enum AccountRole {
    Host,
    Admin,
}
#[derive(
    Clone,
    Copy,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    Eq,
    PartialEq,
    strum::Display,
    strum::EnumString,
)]
#[strum(serialize_all = "snake_case")]
pub enum AccountStatus {
    PendingEnrollment,
    Verified,
    ResetRequired,
}
#[derive(
    Clone,
    Copy,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    Eq,
    PartialEq,
    strum::Display,
    strum::EnumString,
)]
#[strum(serialize_all = "snake_case")]
pub enum SessionScope {
    EnrollmentOnly,
    PasswordResetOnly,
    Normal,
}
#[derive(
    Clone,
    Copy,
    Debug,
    serde::Serialize,
    serde::Deserialize,
    Eq,
    PartialEq,
    strum::Display,
    strum::EnumString,
)]
#[strum(serialize_all = "snake_case")]
pub enum AccessLinkPurpose {
    Enrollment,
    PasswordReset,
}

pub const PASSWORD_MIN_LEN: usize = 10;
pub const PASSWORD_MAX_LEN: usize = 50;
pub const USERNAME_MIN_LEN: usize = 10;
pub const USERNAME_MAX_LEN: usize = 50;

mod error;
pub use error::ValidationError;

/// Validate exact decoded password bytes without trimming or normalization.
pub fn validate_password(input: &str) -> Result<(), ValidationError> {
    if input.is_ascii() && (PASSWORD_MIN_LEN..=PASSWORD_MAX_LEN).contains(&input.len()) {
        Ok(())
    } else {
        Err(ValidationError)
    }
}

/// Return the ASCII-whitespace-trimmed, case-preserving username view.
pub fn validate_username(input: &str) -> Result<&str, ValidationError> {
    let input = input.trim_matches(|c: char| matches!(c, '\u{09}'..='\u{0d}' | ' '));
    if input.is_ascii()
        && (USERNAME_MIN_LEN..=USERNAME_MAX_LEN).contains(&input.len())
        && !input.bytes().any(|b| matches!(b, 0x09..=0x0d | 0x20))
    {
        Ok(input)
    } else {
        Err(ValidationError)
    }
}
