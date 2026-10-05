#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Test fixtures fail fast."
)]
mod support;
use brews_backend::{
    auth::{AuthError, AuthPolicy},
    security::{hash_password, new_token, token_digest, verify_password},
};
use support::TestRuntime;

#[test]
fn phc_verifies_exact_bytes_and_rejects_unbounded_profiles() {
    let rt = TestRuntime::new();
    let mut bytes = vec![120; 10];
    bytes.extend_from_slice(&[0, 9, 32, 127]);
    let password = String::from_utf8(bytes).unwrap();
    let phc = hash_password(&password, AuthPolicy::default(), &rt).unwrap();
    assert!(phc.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"));
    assert_eq!(verify_password(&password, &phc), Ok(true));
    assert_eq!(
        verify_password(&String::from_utf8(vec![121; 12]).unwrap(), &phc),
        Ok(false)
    );
    for bad in [
        phc.replace("m=19456", "m=4294967295"),
        phc.replace("t=2", "t=7"),
        phc.replace("p=1", "p=2"),
        phc.replace("v=19", "v=16"),
        phc.replace("argon2id", "argon2i"),
        "x".repeat(1025),
    ] {
        assert_eq!(verify_password(&password, &bad), Err(AuthError::Crypto));
    }
}

#[test]
fn malformed_phc_metadata_is_rejected() {
    use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};
    let rt = TestRuntime::new();
    let password = String::from_utf8(vec![120; 10]).unwrap();
    let phc = hash_password(&password, AuthPolicy::default(), &rt).unwrap();
    let fields: Vec<_> = phc.split('$').collect();
    assert_eq!(fields.len(), 6);
    for params in [
        "m=19456,p=1",
        "m=19456,t=2",
        "t=2,p=1",
        "m=19456,t=2,p=1,t=2",
        "m=19456,t=2,p=1,x=1",
    ] {
        let malformed = format!("$argon2id$v=19${params}${}${}", fields[4], fields[5]);
        assert!(matches!(
            verify_password(&password, &malformed),
            Err(AuthError::Crypto)
        ));
    }
    for salt in [
        String::new(),
        "#".repeat(22),
        STANDARD_NO_PAD.encode([1; 15]),
        STANDARD_NO_PAD.encode([1; 17]),
    ] {
        let malformed = format!("$argon2id$v=19${}${salt}${}", fields[3], fields[5]);
        assert!(matches!(
            verify_password(&password, &malformed),
            Err(AuthError::Crypto)
        ));
    }
    for output in [
        String::new(),
        "#".repeat(43),
        STANDARD_NO_PAD.encode([1; 31]),
        STANDARD_NO_PAD.encode([1; 33]),
    ] {
        let malformed = format!("$argon2id$v=19${}${}${output}", fields[3], fields[4]);
        assert!(matches!(
            verify_password(&password, &malformed),
            Err(AuthError::Crypto)
        ));
    }
}

#[test]
fn tokens_require_canonical_base64url_and_rng_failure_is_closed() {
    let rt = TestRuntime::new();
    let token = new_token(&rt).unwrap();
    assert_eq!(token.len(), 43);
    assert!(token_digest(&token).is_ok());
    assert!(token_digest(&(token.clone() + "=")).is_err());
    assert!(token_digest(&"x".repeat(43)).is_err());
    rt.fail.set(true);
    assert!(matches!(new_token(&rt), Err(AuthError::Crypto)));
    assert!(matches!(
        hash_password(
            &String::from_utf8(vec![120; 10]).unwrap(),
            AuthPolicy::default(),
            &rt
        ),
        Err(AuthError::Crypto)
    ));
}
