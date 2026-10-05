//! Developer-only management transport; no account authority comes from this body.
use super::ApiError;
use brews_contracts::management::ManagementCommand;
use brews_domain::ids::CommandId;
use http::{HeaderMap, Method};
use sha2::{Digest as _, Sha256};
use subtle::ConstantTimeEq as _;

pub struct CliRequest {
    pub command: ManagementCommand,
    pub command_id: Option<CommandId>,
}

pub fn authenticate_cli(headers: &HeaderMap, key: Option<&[u8]>) -> Result<(), ApiError> {
    let key = key.ok_or(ApiError::NotFound)?;
    if headers.get_all("authorization").iter().count() != 1
        || headers.contains_key("cookie")
        || headers.contains_key("origin")
    {
        return Err(ApiError::Forbidden);
    }
    let header = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .ok_or(ApiError::Forbidden)?;
    let token = header.strip_prefix("Bearer ").ok_or(ApiError::Forbidden)?;
    let actual = crate::security::token_digest(token).map_err(|_| ApiError::Forbidden)?;
    let expected = Sha256::digest(key);
    if !bool::from(actual.as_slice().ct_eq(expected.as_slice())) {
        return Err(ApiError::Forbidden);
    }
    Ok(())
}

pub fn decode_cli(
    method: &Method,
    query: Option<&str>,
    headers: &HeaderMap,
    body: &[u8],
    key: Option<&[u8]>,
) -> Result<CliRequest, ApiError> {
    authenticate_cli(headers, key)?;
    if method != Method::POST {
        return Err(ApiError::MethodNotAllowed);
    }
    if query.is_some() {
        return Err(ApiError::InvalidInput);
    }
    if body.len() > super::BODY_LIMIT {
        return Err(ApiError::PayloadTooLarge);
    }
    if headers.get_all("content-type").iter().count() != 1
        || !headers
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value == "application/json" || value == "application/json; charset=utf-8"
            })
    {
        return Err(ApiError::UnsupportedMediaType);
    }
    if body
        .iter()
        .find(|byte| !matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
        != Some(&b'{')
    {
        return Err(ApiError::InvalidInput);
    }
    let command: ManagementCommand =
        serde_json::from_slice(body).map_err(|_| ApiError::InvalidInput)?;
    let keys = headers.get_all("idempotency-key");
    let command_id = match (command.is_mutating(), keys.iter().count()) {
        (true, 1) => Some(
            keys.iter()
                .next()
                .and_then(|value| value.to_str().ok())
                .ok_or(ApiError::InvalidInput)?
                .parse()
                .map_err(|_| ApiError::InvalidInput)?,
        ),
        (false, 0) => None,
        _ => return Err(ApiError::InvalidInput),
    };
    Ok(CliRequest {
        command,
        command_id,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "Tests fail fast.")]
mod tests {
    use super::{ApiError, authenticate_cli};
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use http::{HeaderMap, HeaderValue};

    #[test]
    fn management_decoder_requires_closed_object_payloads_and_exact_command_headers() {
        use super::decode_cli;
        use http::Method;
        let key = [8u8; 32];
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_str(&["Bearer", &URL_SAFE_NO_PAD.encode(key)].join(" ")).unwrap(),
        );
        headers.insert("content-type", HeaderValue::from_static("application/json"));
        let read = br#"{"operation":"list_accounts","after":null,"limit":50}"#;
        assert!(decode_cli(&Method::POST, None, &headers, read, Some(&key)).is_ok());
        for invalid in [
            br#"["list_accounts",null,50]"#.as_slice(),
            br#"{"operation":"list_accounts","operation":"list_accounts","limit":50}"#,
            br#"{"operation":"list_accounts","limit":50,"actor":"developer_cli"}"#,
            br#"{"operation":"list_accounts","limit":50,"limit":50}"#,
        ] {
            assert!(decode_cli(&Method::POST, None, &headers, invalid, Some(&key)).is_err());
        }
        assert!(decode_cli(&Method::POST, Some(""), &headers, read, Some(&key)).is_err());
        assert!(decode_cli(&Method::GET, None, &headers, read, Some(&key)).is_err());
        headers.insert(
            "idempotency-key",
            HeaderValue::from_static("01890f3e-53b7-7d28-9b05-4f65092d5711"),
        );
        assert!(decode_cli(&Method::POST, None, &headers, read, Some(&key)).is_err());
        let mutation = br#"{"operation":"create_account","username":"CliHostName","role":"host"}"#;
        assert!(decode_cli(&Method::POST, None, &headers, mutation, Some(&key)).is_ok());
        headers.append(
            "idempotency-key",
            HeaderValue::from_static("01890f3e-53b7-7d28-9b05-4f65092d5711"),
        );
        assert!(decode_cli(&Method::POST, None, &headers, mutation, Some(&key)).is_err());
        headers.remove("idempotency-key");
        assert!(decode_cli(&Method::POST, None, &headers, mutation, Some(&key)).is_err());
    }

    #[test]
    fn cli_credentials_never_authorize_browser_or_ambiguous_headers() {
        let key = [8u8; 32];
        let mut headers = HeaderMap::new();
        let credential =
            HeaderValue::from_str(&["Bearer", &URL_SAFE_NO_PAD.encode(key)].join(" ")).unwrap();
        headers.insert("authorization", credential.clone());
        for name in ["origin", "cookie"] {
            headers.insert(name, HeaderValue::from_static(""));
            assert!(authenticate_cli(&headers, Some(&key)).is_err());
            headers.remove(name);
        }
        headers.append("authorization", credential);
        assert!(authenticate_cli(&headers, Some(&key)).is_err());
    }

    #[test]
    fn cli_gate_requires_the_dedicated_binding_and_bearer() {
        let key = [5u8; 32];
        let mut headers = HeaderMap::new();
        assert_eq!(authenticate_cli(&headers, None), Err(ApiError::NotFound));
        assert_eq!(
            authenticate_cli(&headers, Some(&key)),
            Err(ApiError::Forbidden)
        );
        headers.insert(
            "authorization",
            HeaderValue::from_str(&format!("Bearer {}", URL_SAFE_NO_PAD.encode(key))).unwrap(),
        );
        assert_eq!(authenticate_cli(&headers, Some(&key)), Ok(()));
        assert_eq!(
            authenticate_cli(&headers, Some(&[6u8; 32])),
            Err(ApiError::Forbidden)
        );
    }
}
