//! Bounded Unicode profile-one normalization and private Argon2 enrollment.
use super::GameError;
use crate::auth::{AuthPolicy, Runtime};
use argon2::{Algorithm, Argon2, Params, PasswordHasher, Version, password_hash::SaltString};
use unicode_casefold::UnicodeCaseFold;
use unicode_normalization::UnicodeNormalization;
use zeroize::Zeroizing;
pub const NORMALIZATION_VERSION: i64 = 1;
const ANSWER_INPUT_BYTES: usize = 4_096;
const ANSWER_CANONICAL_BYTES: usize = 3 * ANSWER_INPUT_BYTES;
fn push(output: &mut String, value: char) -> Result<(), GameError> {
    if output.len() + value.len_utf8() > ANSWER_CANONICAL_BYTES {
        return Err(GameError::InvalidInput);
    }
    output.push(value);
    Ok(())
}
pub(crate) fn normalize(input: &str) -> Result<Zeroizing<String>, GameError> {
    if input.len() > ANSWER_INPUT_BYTES {
        return Err(GameError::InvalidInput);
    }
    let mut nfc = Zeroizing::new(String::new());
    for c in input.nfc() {
        push(&mut nfc, c)?;
    }
    let mut folded = Zeroizing::new(String::new());
    for c in nfc.trim_matches(char::is_whitespace).case_fold() {
        push(&mut folded, c)?;
    }
    if folded.is_empty() {
        return Err(GameError::InvalidInput);
    }
    Ok(folded)
}
pub(crate) fn enroll(
    answer: &str,
    policy: AuthPolicy,
    runtime: &impl Runtime,
) -> Result<Zeroizing<String>, GameError> {
    crate::security::validate_policy(policy).map_err(|_| GameError::InvalidInput)?;
    let mut random = Zeroizing::new([0u8; crate::security::PASSWORD_SALT_BYTES]);
    runtime
        .fill_random(random.as_mut())
        .map_err(|_| GameError::RandomUnavailable)?;
    let salt = SaltString::encode_b64(random.as_ref()).map_err(|_| GameError::Storage)?;
    let params = Params::new(
        policy.m_cost,
        policy.t_cost,
        policy.p_cost,
        Some(crate::security::PASSWORD_HASH_BYTES),
    )
    .map_err(|_| GameError::Storage)?;
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password(answer.as_bytes(), &salt)
        .map(|s| Zeroizing::new(s.to_string()))
        .map_err(|_| GameError::Storage)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "Golden profile regressions fail fast.")]
mod tests {
    use super::*;
    #[test]
    fn pinned_profile_one_unicode_golden_vectors_are_stable() {
        assert_eq!(unicode_normalization::UNICODE_VERSION, (17, 0, 0));
        assert_eq!(unicode_casefold::UNICODE_VERSION, (9, 0, 0));
        for (source, expected) in [
            ("\u{2003}CAFE\u{301} Straße\u{a0}", "café strasse"),
            ("Σςσ", "σσσ"),
            ("İIı", "i\u{307}iı"),
            (" A  B! ", "a  b!"),
        ] {
            assert_eq!(normalize(source).unwrap().as_str(), expected);
        }
        assert_eq!(normalize("\u{2003} ").err(), Some(GameError::InvalidInput));
        assert_eq!(
            normalize(&"x".repeat(ANSWER_INPUT_BYTES + 1)).err(),
            Some(GameError::InvalidInput)
        );
        println!(
            "NFC {:?}; full non-Turkic fold {:?}; Rust White_Space {:?}",
            unicode_normalization::UNICODE_VERSION,
            unicode_casefold::UNICODE_VERSION,
            std::char::UNICODE_VERSION
        );
    }
}
