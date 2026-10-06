//! Provider-independent account and game domain; allocation-conscious at text boundaries.
#![forbid(unsafe_code)]

pub mod accounts;
pub mod games;
pub mod ids;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, reason = "Tests fail fast.")]
mod tests {
    use super::accounts::{validate_password, validate_username};

    #[test]
    fn enum_storage_tags_are_generated_snake_case_and_unknowns_fail() {
        use super::accounts::{AccessLinkPurpose, AccountRole, AccountStatus, SessionScope};
        assert_eq!(AccountRole::Host.to_string(), "host");
        assert_eq!(
            AccountStatus::PendingEnrollment.to_string(),
            "pending_enrollment"
        );
        assert_eq!(
            AccessLinkPurpose::PasswordReset.to_string(),
            "password_reset"
        );
        assert_eq!(SessionScope::EnrollmentOnly.to_string(), "enrollment_only");
        assert_eq!(
            "verified".parse::<AccountStatus>(),
            Ok(AccountStatus::Verified)
        );
        assert!("unknown".parse::<AccountStatus>().is_err());
    }

    #[test]
    fn username_treats_all_six_approved_ascii_whitespace_bytes_consistently() {
        for byte in [9u8, 10, 11, 12, 13, 32] {
            let c = char::from(byte);
            let padded = format!("{c}abcdefghij{c}");
            assert_eq!(validate_username(&padded).unwrap(), "abcdefghij");
            let internal = format!("abcde{c}fghij");
            assert!(validate_username(&internal).is_err());
        }
    }

    #[test]
    fn password_is_exact_ascii_with_a_fifty_byte_cap() {
        let mut bytes = vec![120; 10];
        bytes.extend_from_slice(&[0, 127, 9, 32]);
        assert!(validate_password(&String::from_utf8(bytes).unwrap()).is_ok());
        assert!(validate_password(&String::from_utf8(vec![120; 50]).unwrap()).is_ok());
        assert!(validate_password(&String::from_utf8(vec![120; 51]).unwrap()).is_err());
        assert!(validate_password(&String::from_utf8(vec![120; 9]).unwrap()).is_err());
        let mut non_ascii = String::from_utf8(vec![120; 10]).unwrap();
        non_ascii.push(char::from(233));
        assert!(validate_password(&non_ascii).is_err());
    }

    #[test]
    fn username_trims_ascii_whitespace_without_folding_case_or_controls() {
        assert_eq!(
            validate_username("\t \rUser\0name\x7f1\n").unwrap(),
            "User\0name\x7f1"
        );
        assert!(validate_username("abcde fghij").is_err());
        assert!(validate_username(&"x".repeat(51)).is_err());
        assert!(validate_username("abcdefghijé").is_err());
        assert_ne!(
            validate_username("ABCDEFGHIJ").unwrap(),
            validate_username("abcdefghij").unwrap()
        );
    }
}
