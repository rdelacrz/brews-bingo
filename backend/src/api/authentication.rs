//! Bounded account-authentication transport; decoding does not establish authority.
#[cfg(test)]
use super::utility::{COOKIE_HEADER_MAX_BYTES, named_cookie};
use super::{
    ApiError, BODY_LIMIT,
    utility::{command_id, decode_object, require_origin, session_cookie},
};
use http::{HeaderMap, Method};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthenticationOperation {
    Login,
    Current,
    RedeemEnrollment,
    CompleteEnrollment,
    RedeemReset,
    CompleteReset,
    Logout,
}
pub enum AuthenticationPayload {
    Login { username: String, password: String },
    Redeem { token: String },
    Complete { new_password: String },
    Empty,
}
pub struct AuthenticationRequest {
    pub operation: AuthenticationOperation,
    pub payload: AuthenticationPayload,
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

pub fn decode_authentication(
    method: &Method,
    path: &str,
    query: Option<&str>,
    headers: &HeaderMap,
    body: &[u8],
    origin: &str,
) -> Result<AuthenticationRequest, ApiError> {
    let operation = match path {
        "/api/auth/login" => AuthenticationOperation::Login,
        "/api/session" => AuthenticationOperation::Current,
        "/api/auth/enrollment/redeem" => AuthenticationOperation::RedeemEnrollment,
        "/api/auth/enrollment/complete" => AuthenticationOperation::CompleteEnrollment,
        "/api/auth/password-reset/redeem" => AuthenticationOperation::RedeemReset,
        "/api/auth/password-reset/complete" => AuthenticationOperation::CompleteReset,
        "/api/auth/logout" => AuthenticationOperation::Logout,
        _ => return Err(ApiError::NotFound),
    };
    let session_token = session_cookie(headers)?;
    let expected = if operation == AuthenticationOperation::Current {
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
    if method == Method::POST {
        require_origin(headers, origin)?;
    }
    if body.len() > BODY_LIMIT {
        return Err(ApiError::PayloadTooLarge);
    }
    if matches!(
        operation,
        AuthenticationOperation::Current | AuthenticationOperation::Logout
    ) {
        if !body.is_empty() {
            return Err(ApiError::InvalidInput);
        }
        return Ok(AuthenticationRequest {
            operation,
            payload: AuthenticationPayload::Empty,
            command_id: None,
            session_token,
        });
    }
    let command_id = command_id(headers)?;
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
    let payload = match operation {
        AuthenticationOperation::Login => {
            let p: LoginBody = decode_object(body)?;
            AuthenticationPayload::Login {
                username: p.username,
                password: p.password,
            }
        }
        AuthenticationOperation::RedeemEnrollment => {
            let p: EnrollmentBody = decode_object(body)?;
            AuthenticationPayload::Redeem {
                token: p.enrollment_token,
            }
        }
        AuthenticationOperation::RedeemReset => {
            let p: ResetBody = decode_object(body)?;
            AuthenticationPayload::Redeem {
                token: p.reset_token,
            }
        }
        AuthenticationOperation::CompleteEnrollment | AuthenticationOperation::CompleteReset => {
            let p: CompleteBody = decode_object(body)?;
            AuthenticationPayload::Complete {
                new_password: p.new_password,
            }
        }
        AuthenticationOperation::Current | AuthenticationOperation::Logout => {
            AuthenticationPayload::Empty
        }
    };
    Ok(AuthenticationRequest {
        operation,
        payload,
        command_id: Some(command_id.to_string()),
        session_token,
    })
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
                decode_authentication(
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
            decode_authentication(
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
                decode_authentication(
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
        let d = decode_authentication(
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
            decode_authentication(
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
            (
                Method::GET,
                "/api/session",
                AuthenticationOperation::Current,
                None,
            ),
            (
                Method::POST,
                "/api/auth/logout",
                AuthenticationOperation::Logout,
                None,
            ),
            (
                Method::POST,
                "/api/auth/enrollment/redeem",
                AuthenticationOperation::RedeemEnrollment,
                Some(
                    serde_json::json!({"enrollment_token":String::from_utf8(vec![0;43]).unwrap()}),
                ),
            ),
            (
                Method::POST,
                "/api/auth/password-reset/redeem",
                AuthenticationOperation::RedeemReset,
                Some(serde_json::json!({"reset_token":String::from_utf8(vec![0;43]).unwrap()})),
            ),
            (
                Method::POST,
                "/api/auth/enrollment/complete",
                AuthenticationOperation::CompleteEnrollment,
                Some(serde_json::json!({"new_password":String::from_utf8(vec![0;10]).unwrap()})),
            ),
            (
                Method::POST,
                "/api/auth/password-reset/complete",
                AuthenticationOperation::CompleteReset,
                Some(serde_json::json!({"new_password":String::from_utf8(vec![0;10]).unwrap()})),
            ),
        ];
        for (method, path, op, payload) in cases {
            let body = payload
                .map(|p| serde_json::to_vec(&p).unwrap())
                .unwrap_or_default();
            let mut h = headers();
            if matches!(
                op,
                AuthenticationOperation::Current | AuthenticationOperation::Logout
            ) {
                h.remove("idempotency-key");
                h.remove("content-type");
            }
            let d = decode_authentication(&method, path, None, &h, &body, "https://localhost:8787")
                .unwrap();
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
                    decode_authentication(
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
    fn named_cookie_keeps_selection_exact_without_rewriting_values() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "cookie",
            "other=x; other=y; selected=__Host-brews_session=value; Selected=ignored"
                .parse()
                .unwrap(),
        );
        assert_eq!(
            named_cookie(&headers, "selected").unwrap().as_deref(),
            Some("__Host-brews_session=value")
        );
        assert_eq!(named_cookie(&headers, "absent").unwrap(), None);
        headers.insert("cookie", "selected=; selected=second".parse().unwrap());
        assert_eq!(
            named_cookie(&headers, "selected"),
            Err(ApiError::InvalidInput)
        );
    }

    #[test]
    fn named_cookie_applies_original_header_bounds_for_every_cookie_name() {
        for name in [
            "__Host-brews_session",
            super::super::PLAYER_COOKIE_NAME,
            super::super::ADMISSION_COOKIE_NAME,
        ] {
            let mut headers = HeaderMap::new();
            let value = "x".repeat(COOKIE_HEADER_MAX_BYTES - name.len() - 1);
            headers.insert("cookie", format!("{name}={value}").parse().unwrap());
            assert_eq!(
                named_cookie(&headers, name).unwrap().as_deref(),
                Some(value.as_str())
            );
            headers.insert("cookie", format!("{name}={value}x").parse().unwrap());
            assert_eq!(named_cookie(&headers, name), Err(ApiError::InvalidInput));
            headers.insert("cookie", "other=first".parse().unwrap());
            headers.append("cookie", "other=second".parse().unwrap());
            assert_eq!(named_cookie(&headers, name), Err(ApiError::InvalidInput));
            headers.clear();
            headers.insert("cookie", http::HeaderValue::from_bytes(b"\xff").unwrap());
            assert_eq!(named_cookie(&headers, name), Err(ApiError::InvalidInput));
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
        let d = decode_authentication(
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
            decode_authentication(
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
                decode_authentication(
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
            decode_authentication(
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
            decode_authentication(
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
            decode_authentication(
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
            decode_authentication(
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
        let d = decode_authentication(
            &Method::GET,
            "/api/session",
            None,
            &HeaderMap::new(),
            b"",
            "https://localhost:8787",
        )
        .unwrap();
        assert_eq!(d.operation, AuthenticationOperation::Current);
        assert!(d.command_id.is_none());
        assert!(matches!(
            decode_authentication(
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
        let d = decode_authentication(
            &Method::POST,
            "/api/auth/login",
            None,
            &headers(),
            &body,
            "https://localhost:8787",
        )
        .unwrap();
        assert_eq!(d.operation, AuthenticationOperation::Login);
        match d.payload {
            AuthenticationPayload::Login { username, password } => {
                assert_eq!(username, " ExactCaseUser ");
                assert_eq!(password.as_bytes(), &[0; 10]);
            }
            _ => panic!("wrong operation payload"),
        }
    }
}
