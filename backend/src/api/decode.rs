use super::ApiError;
use http::{HeaderMap, Method};

pub const BODY_LIMIT: usize = 4_096;
const COOKIE_HEADER_MAX_BYTES: usize = 8_192;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Operation {
    Login,
    Current,
    RedeemEnrollment,
    CompleteEnrollment,
    RedeemReset,
    CompleteReset,
    Logout,
}
pub enum Payload {
    Login { username: String, password: String },
    Redeem { token: String },
    Complete { new_password: String },
    Empty,
}
pub struct Decoded {
    pub operation: Operation,
    pub payload: Payload,
    pub command_id: Option<String>,
    pub session_token: Option<String>,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct LoginBody {
    username: String,
    password: String,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct EnrollmentBody {
    enrollment_token: String,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ResetBody {
    reset_token: String,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CompleteBody {
    new_password: String,
}

pub fn decode(
    method: &Method,
    path: &str,
    query: Option<&str>,
    headers: &HeaderMap,
    body: &[u8],
    origin: &str,
) -> Result<Decoded, ApiError> {
    let operation = match path {
        "/api/auth/login" => Operation::Login,
        "/api/session" => Operation::Current,
        "/api/auth/enrollment/redeem" => Operation::RedeemEnrollment,
        "/api/auth/enrollment/complete" => Operation::CompleteEnrollment,
        "/api/auth/password-reset/redeem" => Operation::RedeemReset,
        "/api/auth/password-reset/complete" => Operation::CompleteReset,
        "/api/auth/logout" => Operation::Logout,
        _ => return Err(ApiError::NotFound),
    };
    let session_token = session_cookie(headers)?;
    let expected = if operation == Operation::Current {
        Method::GET
    } else {
        Method::POST
    };
    if method != expected {
        return Err(ApiError::MethodNotAllowed);
    }
    if query.is_some() {
        return Err(ApiError::InvalidInput);
    }
    if method == Method::POST
        && (headers.get_all("origin").iter().count() != 1
            || headers.get("origin").and_then(|x| x.to_str().ok()) != Some(origin))
    {
        return Err(ApiError::Forbidden);
    }
    if body.len() > BODY_LIMIT {
        return Err(ApiError::PayloadTooLarge);
    }
    if matches!(operation, Operation::Current | Operation::Logout) {
        if !body.is_empty() {
            return Err(ApiError::InvalidInput);
        }
        return Ok(Decoded {
            operation,
            payload: Payload::Empty,
            command_id: None,
            session_token,
        });
    }
    let key = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .ok_or(ApiError::InvalidInput)?;
    let id = uuid::Uuid::parse_str(key).map_err(|_| ApiError::InvalidInput)?;
    if headers.get_all("idempotency-key").iter().count() != 1
        || id.get_version_num() != 7
        || id.get_variant() != uuid::Variant::RFC4122
        || id.to_string() != key
    {
        return Err(ApiError::InvalidInput);
    }
    let media = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .ok_or(ApiError::UnsupportedMediaType)?;
    if headers.get_all("content-type").iter().count() != 1
        || !media
            .split(';')
            .next()
            .is_some_and(|m| m.trim().eq_ignore_ascii_case("application/json"))
    {
        return Err(ApiError::UnsupportedMediaType);
    }
    // Check the root without collapsing duplicate fields.
    if body
        .iter()
        .find(|&&byte| !matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
        != Some(&b'{')
    {
        return Err(ApiError::InvalidInput);
    }
    let payload = match operation {
        Operation::Login => {
            let p: LoginBody = serde_json::from_slice(body).map_err(|_| ApiError::InvalidInput)?;
            Payload::Login {
                username: p.username,
                password: p.password,
            }
        }
        Operation::RedeemEnrollment => {
            let p: EnrollmentBody =
                serde_json::from_slice(body).map_err(|_| ApiError::InvalidInput)?;
            Payload::Redeem {
                token: p.enrollment_token,
            }
        }
        Operation::RedeemReset => {
            let p: ResetBody = serde_json::from_slice(body).map_err(|_| ApiError::InvalidInput)?;
            Payload::Redeem {
                token: p.reset_token,
            }
        }
        Operation::CompleteEnrollment | Operation::CompleteReset => {
            let p: CompleteBody =
                serde_json::from_slice(body).map_err(|_| ApiError::InvalidInput)?;
            Payload::Complete {
                new_password: p.new_password,
            }
        }
        Operation::Current | Operation::Logout => Payload::Empty,
    };
    Ok(Decoded {
        operation,
        payload,
        command_id: Some(key.to_owned()),
        session_token,
    })
}

pub(super) fn session_cookie(headers: &HeaderMap) -> Result<Option<String>, ApiError> {
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
        if key == "__Host-brews_session" {
            if found.is_some() {
                return Err(ApiError::InvalidInput);
            }
            found = Some(value.to_owned());
        }
    }
    Ok(found)
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "Tests fail on invalid fixtures.")]
mod tests {
    use super::*;
    #[test]
    fn post_requires_exact_single_origin() {
        let body=serde_json::to_vec(&serde_json::json!({"username":"ExactCaseUser","password":String::from_utf8(vec![0;10]).unwrap()})).unwrap();
        for bad in [
            None,
            Some("https://other.invalid"),
            Some("null"),
            Some("https://localhost:8787/"),
        ] {
            let mut h = headers();
            h.remove("origin");
            if let Some(v) = bad {
                h.insert("origin", v.parse().unwrap());
            }
            assert!(matches!(
                decode(
                    &Method::POST,
                    "/api/auth/login",
                    None,
                    &h,
                    &body,
                    "https://localhost:8787"
                ),
                Err(ApiError::Forbidden)
            ));
        }
    }

    #[test]
    fn body_bound_is_enforced_before_parsing() {
        assert!(matches!(
            decode(
                &Method::POST,
                "/api/auth/login",
                None,
                &headers(),
                &vec![b' '; BODY_LIMIT + 1],
                "https://localhost:8787"
            ),
            Err(ApiError::PayloadTooLarge)
        ));
    }

    #[test]
    fn login_requires_canonical_uuid_v7_retry_key() {
        let body=serde_json::to_vec(&serde_json::json!({"username":"ExactCaseUser","password":String::from_utf8(vec![0;10]).unwrap()})).unwrap();
        for key in [
            None,
            Some("not-a-uuid"),
            Some("019CE56A-1B00-7000-8000-000000000001"),
            Some("019ce56a-1b00-4000-8000-000000000001"),
        ] {
            let mut h = headers();
            h.remove("idempotency-key");
            if let Some(k) = key {
                h.insert("idempotency-key", k.parse().unwrap());
            }
            assert!(matches!(
                decode(
                    &Method::POST,
                    "/api/auth/login",
                    None,
                    &h,
                    &body,
                    "https://localhost:8787"
                ),
                Err(ApiError::InvalidInput)
            ));
        }
        let d = decode(
            &Method::POST,
            "/api/auth/login",
            None,
            &headers(),
            &body,
            "https://localhost:8787",
        )
        .unwrap();
        assert_eq!(
            d.command_id.as_deref(),
            Some("019ce56a-1b00-7000-8000-000000000001")
        );
    }

    #[test]
    fn non_json_content_type_is_rejected() {
        let mut h = headers();
        h.insert("content-type", "text/plain".parse().unwrap());
        let body=serde_json::to_vec(&serde_json::json!({"username":"ExactCaseUser","password":String::from_utf8(vec![0;10]).unwrap()})).unwrap();
        assert!(matches!(
            decode(
                &Method::POST,
                "/api/auth/login",
                None,
                &h,
                &body,
                "https://localhost:8787"
            ),
            Err(ApiError::UnsupportedMediaType)
        ));
    }

    #[test]
    fn all_seven_operations_accept_their_exact_payload() {
        let cases = [
            (Method::GET, "/api/session", Operation::Current, None),
            (Method::POST, "/api/auth/logout", Operation::Logout, None),
            (
                Method::POST,
                "/api/auth/enrollment/redeem",
                Operation::RedeemEnrollment,
                Some(
                    serde_json::json!({"enrollment_token":String::from_utf8(vec![0;43]).unwrap()}),
                ),
            ),
            (
                Method::POST,
                "/api/auth/password-reset/redeem",
                Operation::RedeemReset,
                Some(serde_json::json!({"reset_token":String::from_utf8(vec![0;43]).unwrap()})),
            ),
            (
                Method::POST,
                "/api/auth/enrollment/complete",
                Operation::CompleteEnrollment,
                Some(serde_json::json!({"new_password":String::from_utf8(vec![0;10]).unwrap()})),
            ),
            (
                Method::POST,
                "/api/auth/password-reset/complete",
                Operation::CompleteReset,
                Some(serde_json::json!({"new_password":String::from_utf8(vec![0;10]).unwrap()})),
            ),
        ];
        for (method, path, op, payload) in cases {
            let body = payload
                .map(|p| serde_json::to_vec(&p).unwrap())
                .unwrap_or_default();
            let mut h = headers();
            if matches!(op, Operation::Current | Operation::Logout) {
                h.remove("idempotency-key");
                h.remove("content-type");
            }
            let d = decode(&method, path, None, &h, &body, "https://localhost:8787").unwrap();
            assert_eq!(d.operation, op);
        }
    }

    #[test]
    fn login_rejects_positional_json_array_root() {
        assert_array_root_rejected(
            "/api/auth/login",
            &[
                String::from_utf8(vec![0; 10]).unwrap(),
                String::from_utf8(vec![0; 10]).unwrap(),
            ],
        );
    }

    #[test]
    fn enrollment_redeem_rejects_positional_json_array_root() {
        assert_array_root_rejected(
            "/api/auth/enrollment/redeem",
            &[String::from_utf8(vec![0; 43]).unwrap()],
        );
    }

    #[test]
    fn enrollment_complete_rejects_positional_json_array_root() {
        assert_array_root_rejected(
            "/api/auth/enrollment/complete",
            &[String::from_utf8(vec![0; 10]).unwrap()],
        );
    }

    #[test]
    fn reset_redeem_rejects_positional_json_array_root() {
        assert_array_root_rejected(
            "/api/auth/password-reset/redeem",
            &[String::from_utf8(vec![0; 43]).unwrap()],
        );
    }

    #[test]
    fn reset_complete_rejects_positional_json_array_root() {
        assert_array_root_rejected(
            "/api/auth/password-reset/complete",
            &[String::from_utf8(vec![0; 10]).unwrap()],
        );
    }

    fn assert_array_root_rejected(path: &str, fields: &[String]) {
        let json = serde_json::to_vec(fields).unwrap();
        for prefix in [b"".as_slice(), b" \t\r\n".as_slice()] {
            let body = [prefix, json.as_slice()].concat();
            assert!(
                matches!(
                    decode(
                        &Method::POST,
                        path,
                        None,
                        &headers(),
                        &body,
                        "https://localhost:8787"
                    ),
                    Err(ApiError::InvalidInput)
                ),
                "positional JSON array accepted at {path}"
            );
        }
    }

    #[test]
    fn cookie_is_read_and_duplicate_session_values_reject() {
        let mut h = headers();
        let token = "A".repeat(43);
        h.insert(
            "cookie",
            format!("other=value; __Host-brews_session={token}")
                .parse()
                .unwrap(),
        );
        let d = decode(
            &Method::GET,
            "/api/session",
            None,
            &h,
            b"",
            "https://localhost:8787",
        )
        .unwrap();
        assert!(d.session_token.as_deref() == Some(token.as_str()));
        h.insert(
            "cookie",
            format!("__Host-brews_session={token}; __Host-brews_session={token}")
                .parse()
                .unwrap(),
        );
        assert!(matches!(
            decode(
                &Method::GET,
                "/api/session",
                None,
                &h,
                b"",
                "https://localhost:8787"
            ),
            Err(ApiError::InvalidInput)
        ));
    }

    #[test]
    fn unknown_json_fields_and_duplicate_keys_are_invalid() {
        for b in [
            br#"{"username":"ExactCaseUser","password":"","role":"Admin"}"#.as_slice(),
            br#"{"username":"ExactCaseUser","username":"OtherCaseUser","password":""}"#.as_slice(),
        ] {
            assert!(matches!(
                decode(
                    &Method::POST,
                    "/api/auth/login",
                    None,
                    &headers(),
                    b,
                    "https://localhost:8787"
                ),
                Err(ApiError::InvalidInput)
            ));
        }
    }
    #[test]
    fn internal_paths_unknown_apis_wrong_methods_and_query_inputs_reject() {
        assert!(matches!(
            decode(
                &Method::GET,
                "/internal/auth",
                None,
                &headers(),
                b"",
                "https://localhost:8787"
            ),
            Err(ApiError::NotFound)
        ));
        assert!(matches!(
            decode(
                &Method::GET,
                "/api/not-a-route",
                None,
                &headers(),
                b"",
                "https://localhost:8787"
            ),
            Err(ApiError::NotFound)
        ));
        assert!(matches!(
            decode(
                &Method::GET,
                "/api/auth/login",
                None,
                &headers(),
                b"",
                "https://localhost:8787"
            ),
            Err(ApiError::MethodNotAllowed)
        ));
        assert!(matches!(
            decode(
                &Method::GET,
                "/api/session",
                Some("token"),
                &headers(),
                b"",
                "https://localhost:8787"
            ),
            Err(ApiError::InvalidInput)
        ));
    }
    #[test]
    fn get_current_is_bodyless_and_has_no_origin_or_retry_requirement() {
        let d = decode(
            &Method::GET,
            "/api/session",
            None,
            &HeaderMap::new(),
            b"",
            "https://localhost:8787",
        )
        .unwrap();
        assert_eq!(d.operation, Operation::Current);
        assert!(d.command_id.is_none());
        assert!(matches!(
            decode(
                &Method::GET,
                "/api/session",
                None,
                &HeaderMap::new(),
                b"{}",
                "https://localhost:8787"
            ),
            Err(ApiError::InvalidInput)
        ));
    }

    fn headers() -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("origin", "https://localhost:8787".parse().unwrap());
        h.insert("content-type", "application/json".parse().unwrap());
        h.insert(
            "idempotency-key",
            "019ce56a-1b00-7000-8000-000000000001".parse().unwrap(),
        );
        h
    }
    #[test]
    fn valid_login_dispatches_without_normalizing_credentials() {
        let body=serde_json::to_vec(&serde_json::json!({"username": " ExactCaseUser ", "password": String::from_utf8(vec![0;10]).unwrap()})).unwrap();
        let d = decode(
            &Method::POST,
            "/api/auth/login",
            None,
            &headers(),
            &body,
            "https://localhost:8787",
        )
        .unwrap();
        assert_eq!(d.operation, Operation::Login);
        match d.payload {
            Payload::Login { username, password } => {
                assert_eq!(username, " ExactCaseUser ");
                assert_eq!(password.as_bytes(), &[0; 10]);
            }
            _ => panic!("wrong operation payload"),
        }
    }
}
