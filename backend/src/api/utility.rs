//! Shared transport checks; endpoint-specific policy and ordering stay with callers.
use super::ApiError;
use brews_domain::ids::CommandId;
use http::HeaderMap;
use serde::de::DeserializeOwned;

pub const BODY_LIMIT: usize = 4_096;
pub(super) const COOKIE_HEADER_MAX_BYTES: usize = 8_192;

pub(super) fn require_origin(headers: &HeaderMap, origin: &str) -> Result<(), ApiError> {
    if headers.get_all("origin").iter().count() != 1
        || headers.get("origin").and_then(|value| value.to_str().ok()) != Some(origin)
    {
        return Err(ApiError::Forbidden);
    }
    Ok(())
}

// Authentication intentionally accepts a broader JSON media-type grammar.
pub(super) fn require_json(headers: &HeaderMap) -> Result<(), ApiError> {
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
    Ok(())
}

pub(super) fn command_id(headers: &HeaderMap) -> Result<CommandId, ApiError> {
    if headers.get_all("idempotency-key").iter().count() != 1 {
        return Err(ApiError::InvalidInput);
    }
    headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .ok_or(ApiError::InvalidInput)?
        .parse()
        .map_err(|_| ApiError::InvalidInput)
}

/// Decode original bytes, preserving duplicate-field rejection by the target type.
/// Public ingress checks body size first when its contract requires HTTP 413.
pub(crate) fn decode_object<T: DeserializeOwned>(body: &[u8]) -> Result<T, ApiError> {
    if body.len() > BODY_LIMIT
        || body
            .iter()
            .find(|byte| !matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
            != Some(&b'{')
    {
        return Err(ApiError::InvalidInput);
    }
    serde_json::from_slice(body).map_err(|_| ApiError::InvalidInput)
}

pub(super) fn session_cookie(headers: &HeaderMap) -> Result<Option<String>, ApiError> {
    named_cookie(headers, "__Host-brews_session")
}

pub(super) fn named_cookie(headers: &HeaderMap, name: &str) -> Result<Option<String>, ApiError> {
    if headers.get_all("cookie").iter().count() > 1 {
        return Err(ApiError::InvalidInput);
    }
    let Some(raw) = headers.get("cookie") else {
        return Ok(None);
    };
    let raw = raw.to_str().map_err(|_| ApiError::InvalidInput)?;
    if raw.len() > COOKIE_HEADER_MAX_BYTES {
        return Err(ApiError::InvalidInput);
    }
    let mut found = None;
    for part in raw.split(';') {
        let Some((key, value)) = part.trim().split_once('=') else {
            continue;
        };
        if key == name {
            if found.is_some() {
                return Err(ApiError::InvalidInput);
            }
            found = Some(value);
        }
    }
    Ok(found.map(str::to_owned))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "Closed transport fixtures fail fast.")]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Input {
        value: u8,
    }

    #[test]
    fn object_decoder_preserves_closed_original_bytes_and_body_bound() {
        assert_eq!(
            decode_object::<Input>(b" \t\r\n{\"value\":1}").unwrap(),
            Input { value: 1 }
        );
        for bytes in [
            b"[1]".as_slice(),
            b"null",
            b"{}",
            b"{\"value\":1,\"value\":2}",
            br#"{"value":1,"\u0076alue":2}"#,
            b"{\"value\":1,\"extra\":2}",
            b"{\"value\":1}{}",
            b"\x0b{\"value\":1}",
        ] {
            assert_eq!(decode_object::<Input>(bytes), Err(ApiError::InvalidInput));
        }
        let mut bytes = b"{\"value\":1}".to_vec();
        bytes.resize(BODY_LIMIT, b' ');
        assert!(decode_object::<Input>(&bytes).is_ok());
        bytes.push(b' ');
        assert_eq!(decode_object::<Input>(&bytes), Err(ApiError::InvalidInput));
    }

    #[test]
    fn required_headers_reject_missing_duplicates_and_noncanonical_values() {
        let mut headers = HeaderMap::new();
        assert_eq!(
            require_origin(&headers, "https://app.test"),
            Err(ApiError::Forbidden)
        );
        assert_eq!(require_json(&headers), Err(ApiError::UnsupportedMediaType));
        assert_eq!(command_id(&headers), Err(ApiError::InvalidInput));
        headers.insert("origin", "https://app.test".parse().unwrap());
        headers.insert(
            "content-type",
            "application/json; charset=utf-8".parse().unwrap(),
        );
        headers.insert(
            "idempotency-key",
            "01890f3e-53b7-7d28-9b05-4f65092d5711".parse().unwrap(),
        );
        assert!(require_origin(&headers, "https://app.test").is_ok());
        assert!(require_json(&headers).is_ok());
        assert!(command_id(&headers).is_ok());
        for name in ["origin", "content-type", "idempotency-key"] {
            let mut duplicate = headers.clone();
            duplicate.append(name, headers[name].clone());
            assert!(match name {
                "origin" => require_origin(&duplicate, "https://app.test").is_err(),
                "content-type" => require_json(&duplicate).is_err(),
                _ => command_id(&duplicate).is_err(),
            });
        }
        headers.insert("origin", "https://app.test/".parse().unwrap());
        assert_eq!(
            require_origin(&headers, "https://app.test"),
            Err(ApiError::Forbidden)
        );
        for value in [
            "Application/JSON",
            "application/json; charset=UTF-8",
            "text/plain",
        ] {
            headers.insert("content-type", value.parse().unwrap());
            assert_eq!(require_json(&headers), Err(ApiError::UnsupportedMediaType));
        }
        for value in [
            "01890F3E-53b7-7d28-9b05-4f65092d5711",
            "01890f3e-53b7-4d28-9b05-4f65092d5711",
            "bad",
        ] {
            headers.insert("idempotency-key", value.parse().unwrap());
            assert_eq!(command_id(&headers), Err(ApiError::InvalidInput));
        }
    }
}
