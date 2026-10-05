//! Secret-handling and bounded cryptographic primitives.
//! Allocation-conscious: KDF allocation is explicitly capped at 64 MiB.
use crate::auth::{AuthError, AuthPolicy, Runtime};
use argon2::{
    Algorithm, Argon2, Params, PasswordHash, PasswordHasher, PasswordVerifier, Version,
    password_hash::SaltString,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use brews_config::env::backend::{
    ARGON2_MAX_ITERATIONS, ARGON2_MAX_MEMORY_KIB, ARGON2_MIN_ITERATIONS, ARGON2_MIN_MEMORY_KIB,
    ARGON2_PARALLELISM,
};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

pub const PASSWORD_SALT_BYTES: usize = 16;
pub const PASSWORD_HASH_BYTES: usize = 32;
pub const DIGEST_BYTES: usize = 32;
pub const TOKEN_BYTES: usize = 32;
pub const TOKEN_ENCODED_LEN: usize = (TOKEN_BYTES * 8).div_ceil(6);
pub const MAX_PHC_LEN: usize = 256;
pub const PHC_VERSION: u32 = 19;
const PHC_PARAMETER_COUNT: usize = 3;
const SALT_DECODE_BUFFER_BYTES: usize = 64;

/// The default is an OWASP benchmark point, not a production-capacity claim.
pub fn validate_policy(policy: AuthPolicy) -> Result<(), AuthError> {
    if !(ARGON2_MIN_MEMORY_KIB..=ARGON2_MAX_MEMORY_KIB).contains(&policy.m_cost)
        || !(ARGON2_MIN_ITERATIONS..=ARGON2_MAX_ITERATIONS).contains(&policy.t_cost)
        || policy.p_cost != ARGON2_PARALLELISM
    {
        return Err(AuthError::Crypto);
    }
    Ok(())
}

pub fn hash_password(
    password: &str,
    policy: AuthPolicy,
    runtime: &impl Runtime,
) -> Result<String, AuthError> {
    brews_domain::accounts::validate_password(password).map_err(|_| AuthError::InvalidInput)?;
    validate_policy(policy)?;
    let mut bytes = Zeroizing::new([0u8; PASSWORD_SALT_BYTES]);
    runtime.fill_random(bytes.as_mut())?;
    let salt = SaltString::encode_b64(bytes.as_ref()).map_err(|_| AuthError::Crypto)?;
    let params = Params::new(
        policy.m_cost,
        policy.t_cost,
        policy.p_cost,
        Some(PASSWORD_HASH_BYTES),
    )
    .map_err(|_| AuthError::Crypto)?;
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password(password.as_bytes(), &salt)
        .map(|v| v.to_string())
        .map_err(|_| AuthError::Crypto)
}

/// Validate all stored PHC metadata before any expensive KDF work.
pub fn verify_password(password: &str, phc: &str) -> Result<bool, AuthError> {
    let parsed = bounded_phc(phc)?;
    let params = Params::try_from(&parsed).map_err(|_| AuthError::Crypto)?;
    match Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .verify_password(password.as_bytes(), &parsed)
    {
        Ok(()) => Ok(true),
        Err(argon2::password_hash::Error::Password) => Ok(false),
        Err(_) => Err(AuthError::Crypto),
    }
}

pub(crate) fn bounded_phc(phc: &str) -> Result<PasswordHash<'_>, AuthError> {
    if phc.len() > MAX_PHC_LEN {
        return Err(AuthError::Crypto);
    }
    let parsed = PasswordHash::new(phc).map_err(|_| AuthError::Crypto)?;
    if parsed.algorithm.as_str() != "argon2id"
        || parsed.version != Some(PHC_VERSION)
        || parsed.params.iter().count() != PHC_PARAMETER_COUNT
    {
        return Err(AuthError::Crypto);
    }
    let cost = |name| parsed.params.get_decimal(name).ok_or(AuthError::Crypto);
    validate_policy(AuthPolicy {
        m_cost: cost("m")?,
        t_cost: cost("t")?,
        p_cost: cost("p")?,
    })?;
    let salt = parsed.salt.ok_or(AuthError::Crypto)?;
    let mut decoded = [0u8; SALT_DECODE_BUFFER_BYTES];
    if salt
        .decode_b64(&mut decoded)
        .map_err(|_| AuthError::Crypto)?
        .len()
        != PASSWORD_SALT_BYTES
        || parsed.hash.as_ref().ok_or(AuthError::Crypto)?.len() != PASSWORD_HASH_BYTES
    {
        return Err(AuthError::Crypto);
    }
    Ok(parsed)
}

pub fn new_token(runtime: &impl Runtime) -> Result<String, AuthError> {
    let mut bytes = Zeroizing::new([0u8; TOKEN_BYTES]);
    runtime.fill_random(bytes.as_mut())?;
    Ok(URL_SAFE_NO_PAD.encode(bytes.as_ref()))
}
pub fn token_digest(token: &str) -> Result<[u8; DIGEST_BYTES], AuthError> {
    if token.len() != TOKEN_ENCODED_LEN {
        return Err(AuthError::InvalidInput);
    }
    let mut bytes = Zeroizing::new([0u8; TOKEN_BYTES]);
    let n = URL_SAFE_NO_PAD
        .decode_slice(token, bytes.as_mut())
        .map_err(|_| AuthError::InvalidInput)?;
    if n != TOKEN_BYTES || URL_SAFE_NO_PAD.encode(bytes.as_ref()) != token {
        return Err(AuthError::InvalidInput);
    }
    Ok(Sha256::digest(bytes.as_ref()).into())
}
