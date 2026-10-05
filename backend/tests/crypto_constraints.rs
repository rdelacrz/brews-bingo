use brews_backend::{
    auth::AuthPolicy,
    security::{
        DIGEST_BYTES, MAX_PHC_LEN, PASSWORD_HASH_BYTES, PASSWORD_SALT_BYTES, PHC_VERSION,
        TOKEN_BYTES, TOKEN_ENCODED_LEN,
    },
};
use brews_config::env::backend::{
    ARGON2_DEFAULT_ITERATIONS, ARGON2_DEFAULT_MEMORY_KIB, ARGON2_PARALLELISM,
};

#[test]
fn named_crypto_limits_preserve_the_approved_profile() {
    assert_eq!(PASSWORD_SALT_BYTES, 16);
    assert_eq!(PASSWORD_HASH_BYTES, 32);
    assert_eq!(DIGEST_BYTES, 32);
    assert_eq!(TOKEN_BYTES, 32);
    assert_eq!(TOKEN_ENCODED_LEN, 43);
    assert_eq!(MAX_PHC_LEN, 256);
    assert_eq!(PHC_VERSION, 19);
}

#[test]
fn auth_and_environment_defaults_use_the_same_kdf_profile() {
    let policy = AuthPolicy::default();
    assert_eq!(policy.m_cost, ARGON2_DEFAULT_MEMORY_KIB);
    assert_eq!(policy.t_cost, ARGON2_DEFAULT_ITERATIONS);
    assert_eq!(policy.p_cost, ARGON2_PARALLELISM);
}
