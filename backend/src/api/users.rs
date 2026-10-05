//! Bounded public Users transport; account authority is proved by the owner.
use super::ApiError;
use brews_contracts::{management::ManagementCommand, users::CreateUsername};
use brews_domain::{
    accounts::AccountRole,
    ids::{AccountId, CommandId},
};
use http::{HeaderMap, Method};
use zeroize::Zeroize as _;

/// A grammar-checked command, not an authentication or authorization proof.
///
/// The retained cookie is zeroized on drop. Transfer it with `session_token.take()`;
/// borrow or clone the command instead of moving fields out of this `Drop` type.
/// This secret-bearing envelope deliberately does not implement `Debug`.
///
/// ```compile_fail,E0277
/// fn requires_debug<T: std::fmt::Debug>() {}
/// requires_debug::<brews_backend::api::UsersRequest>();
/// ```
pub struct UsersRequest {
    pub command: ManagementCommand,
    pub command_id: Option<CommandId>,
    pub session_token: Option<String>,
}

impl Drop for UsersRequest {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl zeroize::Zeroize for UsersRequest {
    fn zeroize(&mut self) {
        self.session_token.zeroize();
    }
}

const DEFAULT_PAGE_SIZE: u32 = 50;
const MAX_PAGE_SIZE: u32 = 100;
const QUERY_MAX_BYTES: usize = 512;
const QUERY_MAX_FIELDS: usize = 2;

#[derive(Clone, Copy)]
enum Route {
    Collection,
    Account(AccountId),
    Create(AccountRole),
    ReissueEnrollment(AccountId),
    ResetPassword(AccountId),
    Disable(AccountId),
    Enable(AccountId),
}

fn route(path: &str) -> Result<Route, ApiError> {
    match path {
        "/api/users" => Ok(Route::Collection),
        "/api/users/hosts" => Ok(Route::Create(AccountRole::Host)),
        "/api/users/admins" => Ok(Route::Create(AccountRole::Admin)),
        _ => {
            let target = path.strip_prefix("/api/users/").ok_or(ApiError::NotFound)?;
            let (id, action) = target
                .split_once('/')
                .map_or((target, None), |(id, action)| (id, Some(action)));
            let account_id = id.parse().map_err(|_| ApiError::NotFound)?;
            match action {
                None => Ok(Route::Account(account_id)),
                Some("enrollment-links") => Ok(Route::ReissueEnrollment(account_id)),
                Some("password-reset-links") => Ok(Route::ResetPassword(account_id)),
                Some("disable") => Ok(Route::Disable(account_id)),
                Some("enable") => Ok(Route::Enable(account_id)),
                _ => Err(ApiError::NotFound),
            }
        }
    }
}

fn list_command(query: Option<&str>) -> Result<ManagementCommand, ApiError> {
    let mut after = None;
    let mut limit = None;
    if let Some(query) = query {
        if query.is_empty() || query.len() > QUERY_MAX_BYTES || !query.is_ascii() {
            return Err(ApiError::InvalidInput);
        }
        for (index, field) in query.split('&').enumerate() {
            if index >= QUERY_MAX_FIELDS {
                return Err(ApiError::InvalidInput);
            }
            let (key, value) = field.split_once('=').ok_or(ApiError::InvalidInput)?;
            if value.is_empty() {
                return Err(ApiError::InvalidInput);
            }
            match key {
                "cursor" if after.is_none() => {
                    after = Some(value.parse().map_err(|_| ApiError::InvalidInput)?);
                }
                "limit" if limit.is_none() => {
                    if !value.bytes().all(|byte| byte.is_ascii_digit()) {
                        return Err(ApiError::InvalidInput);
                    }
                    let parsed = value.parse().map_err(|_| ApiError::InvalidInput)?;
                    if !(1..=MAX_PAGE_SIZE).contains(&parsed) {
                        return Err(ApiError::InvalidInput);
                    }
                    limit = Some(parsed);
                }
                _ => return Err(ApiError::InvalidInput),
            }
        }
    }
    Ok(ManagementCommand::ListAccounts {
        after,
        limit: limit.unwrap_or(DEFAULT_PAGE_SIZE),
    })
}

/// Decodes the exact public B1–B9 routes without proving account authority.
///
/// Paths, identifiers and query values are never normalized or percent-decoded.
/// Only collection GET accepts `cursor` and `limit`: at most two fields and 512
/// bytes, with a default page size of 50 and explicit limits in `1..=100`.
/// Only account creation accepts a JSON body; every other route is bodyless.
///
/// # Allocation
///
/// Allocation-conscious: the shared body bound limits username decoding, and the
/// shared cookie decoder bounds the optional owned cookie. No intermediate JSON
/// value or form-decoding allocation is used.
///
/// # Errors
///
/// Returns fixed, value-free transport errors for unknown routes, wrong methods,
/// forbidden origins or Authorization headers, malformed grammar and size/media
/// violations. Missing or untrusted session cookies are left to the Accounts
/// owner for authoritative authentication and administrator authorization.
pub fn decode_users(
    method: &Method,
    path: &str,
    query: Option<&str>,
    headers: &HeaderMap,
    body: &[u8],
    origin: &str,
) -> Result<UsersRequest, ApiError> {
    let route = route(path)?;
    let allowed_method = match route {
        Route::Collection => method == Method::GET,
        Route::Account(_) => method == Method::GET || method == Method::DELETE,
        _ => method == Method::POST,
    };
    if !allowed_method {
        return Err(ApiError::MethodNotAllowed);
    }
    if headers.contains_key("authorization") {
        return Err(ApiError::Forbidden);
    }
    let origins = headers.get_all("origin");
    let origin_count = origins.iter().count();
    if (method != Method::GET || origin_count != 0)
        && (origin_count != 1
            || headers.get("origin").and_then(|value| value.to_str().ok()) != Some(origin))
    {
        return Err(ApiError::Forbidden);
    }
    if !matches!(route, Route::Collection) && query.is_some() {
        return Err(ApiError::InvalidInput);
    }
    if body.len() > super::BODY_LIMIT {
        return Err(ApiError::PayloadTooLarge);
    }
    if !matches!(route, Route::Create(_)) && !body.is_empty() {
        return Err(ApiError::InvalidInput);
    }
    let command = match route {
        Route::Collection => list_command(query)?,
        Route::Account(account_id) if method == Method::DELETE => {
            ManagementCommand::DeleteAccount { account_id }
        }
        Route::Account(account_id) => ManagementCommand::GetAccount { account_id },
        Route::ReissueEnrollment(account_id) => ManagementCommand::ReissueEnrollment { account_id },
        Route::ResetPassword(account_id) => ManagementCommand::ResetPassword { account_id },
        Route::Disable(account_id) => ManagementCommand::DisableAccount { account_id },
        Route::Enable(account_id) => ManagementCommand::EnableAccount { account_id },
        Route::Create(role) => {
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
            // Parse the original bytes so duplicate fields remain observable.
            let CreateUsername { username } =
                serde_json::from_slice(body).map_err(|_| ApiError::InvalidInput)?;
            ManagementCommand::CreateAccount { username, role }
        }
    };
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
    Ok(UsersRequest {
        command,
        command_id,
        session_token: super::decode::session_cookie(headers)?,
    })
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "Tests fail on invalid public fixtures.")]
mod tests {
    use super::*;
    const ORIGIN: &str = "https://localhost:8787";
    const ACCOUNT: &str = "019ce56a-1b00-7000-8000-000000000001";
    const COMMAND: &str = "019ce56a-1b00-7000-8000-000000000002";

    fn mutation_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("origin", ORIGIN.parse().unwrap());
        headers.insert("idempotency-key", COMMAND.parse().unwrap());
        headers.insert("content-type", "application/json".parse().unwrap());
        headers
    }

    fn create(path: &str, body: &[u8]) -> Result<UsersRequest, ApiError> {
        decode_users(&Method::POST, path, None, &mutation_headers(), body, ORIGIN)
    }

    #[test]
    fn creation_routes_fix_host_or_admin_role_and_preserve_username() {
        for (path, expected_role) in [
            (
                "/api/users/hosts",
                brews_domain::accounts::AccountRole::Host,
            ),
            (
                "/api/users/admins",
                brews_domain::accounts::AccountRole::Admin,
            ),
        ] {
            let request = create(path, br#" {"username":" ExactCaseUser "} "#).unwrap();
            assert!(
                matches!(&request.command, ManagementCommand::CreateAccount { username, role } if username == " ExactCaseUser " && *role == expected_role)
            );
        }
    }

    #[test]
    fn create_requires_original_json_object_with_only_one_username() {
        for body in [
            b"".as_slice(),
            b" \t\n\r",
            br#"["ExactCaseUser"]"#,
            b"null",
            b"42",
            br#""ExactCaseUser""#,
            b"{}",
            br#"{"username":null}"#,
            br#"{"username":12}"#,
            br#"{"username":["ExactCaseUser"]}"#,
            br#"{"username":"first","username":"second"}"#,
            br#"{"username":"first","\u0075sername":"second"}"#,
            br#"{"username":"ExactCaseUser","role":"Admin"}"#,
            br#"{"username":"ExactCaseUser","actor":"developer_cli"}"#,
            br#"{"username":"ExactCaseUser","principal":"admin_session"}"#,
            br#"{"username":"ExactCaseUser","password":""}"#,
            br#"{"username":"ExactCaseUser","account_id":"019ce56a-1b00-7000-8000-000000000001"}"#,
            br#"{"Username":"ExactCaseUser"}"#,
            br#"{"username":"ExactCaseUser"}{}"#,
            b"\x0b{\"username\":\"ExactCaseUser\"}",
            b"{\"username\":\"\xff\"}",
        ] {
            for path in ["/api/users/hosts", "/api/users/admins"] {
                assert_eq!(
                    create(path, body).err(),
                    Some(ApiError::InvalidInput),
                    "untrusted creation shape accepted"
                );
            }
        }
    }

    #[test]
    fn known_creation_paths_reject_every_other_method() {
        for path in ["/api/users/hosts", "/api/users/admins"] {
            for method in [
                Method::GET,
                Method::DELETE,
                Method::PUT,
                Method::PATCH,
                Method::HEAD,
                Method::OPTIONS,
            ] {
                assert_eq!(
                    decode_users(&method, path, None, &mutation_headers(), b"", ORIGIN).err(),
                    Some(ApiError::MethodNotAllowed)
                );
            }
        }
    }

    fn bodyless_mutations() -> [(Method, String, brews_contracts::management::AuditOperation); 5] {
        use brews_contracts::management::AuditOperation;
        [
            (
                Method::POST,
                format!("/api/users/{ACCOUNT}/enrollment-links"),
                AuditOperation::ReissueEnrollment,
            ),
            (
                Method::POST,
                format!("/api/users/{ACCOUNT}/password-reset-links"),
                AuditOperation::ResetPassword,
            ),
            (
                Method::POST,
                format!("/api/users/{ACCOUNT}/disable"),
                AuditOperation::DisableAccount,
            ),
            (
                Method::DELETE,
                format!("/api/users/{ACCOUNT}"),
                AuditOperation::DeleteAccount,
            ),
            (
                Method::POST,
                format!("/api/users/{ACCOUNT}/enable"),
                AuditOperation::EnableAccount,
            ),
        ]
    }

    #[test]
    fn bodyless_mutation_routes_map_to_their_closed_management_commands() {
        use brews_contracts::management::AuditOperation;
        for (method, path, expected) in bodyless_mutations() {
            let mut headers = mutation_headers();
            headers.remove("content-type");
            let request = decode_users(&method, &path, None, &headers, b"", ORIGIN).unwrap();
            let (actual, target) = match request.command {
                ManagementCommand::ReissueEnrollment { account_id } => {
                    (AuditOperation::ReissueEnrollment, account_id)
                }
                ManagementCommand::ResetPassword { account_id } => {
                    (AuditOperation::ResetPassword, account_id)
                }
                ManagementCommand::DisableAccount { account_id } => {
                    (AuditOperation::DisableAccount, account_id)
                }
                ManagementCommand::DeleteAccount { account_id } => {
                    (AuditOperation::DeleteAccount, account_id)
                }
                ManagementCommand::EnableAccount { account_id } => {
                    (AuditOperation::EnableAccount, account_id)
                }
                _ => panic!("wrong management command"),
            };
            assert_eq!(actual, expected);
            assert_eq!(target.to_string(), ACCOUNT);
        }
    }

    #[test]
    fn reads_and_actions_require_exactly_zero_body_bytes() {
        for (method, path, _, headers) in valid_routes()
            .into_iter()
            .filter(|(_, _, body, _)| body.is_empty())
        {
            for body in [
                b" ".as_slice(),
                b"\t\r\n",
                b"{}",
                b"[]",
                b"null",
                b"\0",
                br#"{"actor":"developer_cli"}"#,
                br#"{"role":"Admin"}"#,
            ] {
                assert_eq!(
                    decode_users(&method, &path, None, &headers, body, ORIGIN).err(),
                    Some(ApiError::InvalidInput)
                );
            }
        }
    }

    #[test]
    fn action_routes_reject_other_methods_without_decoding_payloads() {
        for (_, path, _) in bodyless_mutations()
            .into_iter()
            .filter(|(method, _, _)| *method == Method::POST)
        {
            for method in [
                Method::GET,
                Method::DELETE,
                Method::PUT,
                Method::PATCH,
                Method::HEAD,
                Method::OPTIONS,
            ] {
                assert_eq!(
                    decode_users(&method, &path, None, &mutation_headers(), b"{}", ORIGIN).err(),
                    Some(ApiError::MethodNotAllowed)
                );
            }
        }
    }

    #[test]
    fn encoded_cased_or_extended_actions_are_not_routes() {
        for suffix in [
            "Disable",
            "disable/",
            "disable/extra",
            "%64isable",
            "disable%2f",
            "disable?",
            "disable#",
            "password-reset",
            "enrollment-link",
            "audit",
            "roles",
            "",
        ] {
            assert_eq!(
                create(&format!("/api/users/{ACCOUNT}/{suffix}"), b"").err(),
                Some(ApiError::NotFound)
            );
        }
    }

    fn collection_query(query: &str) -> Result<UsersRequest, ApiError> {
        decode_users(
            &Method::GET,
            "/api/users",
            Some(query),
            &HeaderMap::new(),
            b"",
            ORIGIN,
        )
    }

    #[test]
    fn collection_query_maps_only_cursor_and_bounded_limit_in_either_order() {
        for (query, expected_after, expected_limit) in [
            ("limit=1".to_owned(), None, 1),
            ("limit=100".to_owned(), None, 100),
            (format!("cursor={ACCOUNT}"), Some(ACCOUNT), 50),
            (format!("cursor={ACCOUNT}&limit=37"), Some(ACCOUNT), 37),
            (format!("limit=37&cursor={ACCOUNT}"), Some(ACCOUNT), 37),
        ] {
            let request = collection_query(&query).unwrap();
            match request.command {
                ManagementCommand::ListAccounts { after, limit } => {
                    assert_eq!(after.map(|id| id.to_string()).as_deref(), expected_after);
                    assert_eq!(limit, expected_limit);
                }
                _ => panic!("wrong collection command"),
            }
        }
    }

    #[test]
    fn collection_query_rejects_ambiguous_unknown_duplicate_or_empty_fields() {
        for query in [
            "",
            "limit",
            "=1",
            "limit=",
            "cursor=",
            "&limit=1",
            "limit=1&",
            "limit=1&&cursor=",
            "Limit=1",
            "role=Admin",
            "status=Verified",
            "after=019ce56a-1b00-7000-8000-000000000001",
            "limit=1&limit=2",
            "limit=1&limit=1",
            "cursor=019ce56a-1b00-7000-8000-000000000001&cursor=019ce56a-1b00-7000-8000-000000000001",
            "limit=1&cursor=019ce56a-1b00-7000-8000-000000000001&other=1",
            "limit=1;cursor=019ce56a-1b00-7000-8000-000000000001",
            "limit=0",
            "limit=101",
            "limit=-1",
            "limit=+1",
            "limit= 1",
            "limit=1 ",
            "limit=1.0",
            "limit=1e1",
            "limit=4294967296",
            "limit=１",
            "limit=1\0",
            "limit=1#fragment",
            "limit=1=1",
            "limit[]=1",
            "%6cimit=1",
            "limit=%31",
            "limit=1%26limit=2",
            "cursor=019CE56A-1B00-7000-8000-000000000001",
            "cursor=019ce56a1b0070008000000000000001",
            "cursor=019ce56a-1b00-4000-8000-000000000001",
            "cursor=%3019ce56a-1b00-7000-8000-000000000001",
            "cursor=019ce56a-1b00-7000-8000-000000000001+",
            "cursor=☃",
            "cursor=null",
        ] {
            assert_eq!(
                collection_query(query).err(),
                Some(ApiError::InvalidInput),
                "invalid collection query accepted"
            );
        }
    }

    #[test]
    fn collection_query_byte_bound_precedes_parsing() {
        let exact_bound = format!("limit={}1", "0".repeat(505));
        assert_eq!(exact_bound.len(), 512);
        let request = collection_query(&exact_bound).unwrap();
        assert!(matches!(
            request.command,
            ManagementCommand::ListAccounts {
                after: None,
                limit: 1
            }
        ));
        assert_eq!(
            collection_query(&"x".repeat(513)).err(),
            Some(ApiError::InvalidInput)
        );
        assert_eq!(
            collection_query(&format!("limit={}1", "0".repeat(507))).err(),
            Some(ApiError::InvalidInput)
        );
    }

    #[test]
    fn every_noncollection_endpoint_forbids_even_an_empty_query() {
        for (method, path, body, headers) in valid_routes()
            .into_iter()
            .filter(|(_, path, _, _)| path != "/api/users")
        {
            for query in [
                "",
                "limit=1",
                "cursor=019ce56a-1b00-7000-8000-000000000001",
                "actor=developer_cli",
            ] {
                assert_eq!(
                    decode_users(&method, &path, Some(query), &headers, body, ORIGIN).err(),
                    Some(ApiError::InvalidInput)
                );
            }
        }
    }

    fn valid_routes() -> Vec<(Method, String, &'static [u8], HeaderMap)> {
        let mut cases = bodyless_mutations()
            .map(|(method, path, _)| (method, path, b"".as_slice(), mutation_headers()))
            .to_vec();
        cases.extend(["/api/users/hosts", "/api/users/admins"].map(|path| {
            (
                Method::POST,
                path.to_owned(),
                br#"{"username":"ExactCaseUser"}"#.as_slice(),
                mutation_headers(),
            )
        }));
        cases.extend(
            ["/api/users".to_owned(), format!("/api/users/{ACCOUNT}")]
                .map(|path| (Method::GET, path, b"".as_slice(), HeaderMap::new())),
        );
        assert_eq!(cases.len(), 9);
        cases
    }

    #[test]
    fn every_route_enforces_body_byte_limit_before_body_parsing() {
        for (method, path, _, headers) in valid_routes() {
            assert_eq!(
                decode_users(&method, &path, None, &headers, &vec![b' '; 4097], ORIGIN).err(),
                Some(ApiError::PayloadTooLarge)
            );
        }
        let mut body = br#"{"username":"ExactCaseUser"}"#.to_vec();
        body.resize(4096, b' ');
        assert!(create("/api/users/hosts", &body).is_ok());
        body.push(b' ');
        assert_eq!(
            create("/api/users/hosts", &body).err(),
            Some(ApiError::PayloadTooLarge)
        );
    }

    #[test]
    fn create_requires_one_supported_json_media_type() {
        for path in ["/api/users/hosts", "/api/users/admins"] {
            let body = br#"{"username":"ExactCaseUser"}"#;
            for media in [
                None,
                Some(""),
                Some("text/plain"),
                Some("application/json-patch+json"),
                Some("application/json; charset=iso-8859-1"),
                Some("application/json; garbage"),
                Some("application/json, application/json"),
            ] {
                let mut headers = mutation_headers();
                headers.remove("content-type");
                if let Some(media) = media {
                    headers.insert("content-type", media.parse().unwrap());
                }
                assert_eq!(
                    decode_users(&Method::POST, path, None, &headers, body, ORIGIN).err(),
                    Some(ApiError::UnsupportedMediaType)
                );
            }
            for media in ["application/json", "application/json; charset=utf-8"] {
                let mut headers = mutation_headers();
                headers.insert("content-type", media.parse().unwrap());
                assert!(decode_users(&Method::POST, path, None, &headers, body, ORIGIN).is_ok());
                headers.append("content-type", media.parse().unwrap());
                assert_eq!(
                    decode_users(&Method::POST, path, None, &headers, body, ORIGIN).err(),
                    Some(ApiError::UnsupportedMediaType)
                );
            }
            let mut headers = mutation_headers();
            headers.insert(
                "content-type",
                http::HeaderValue::from_bytes(b"\xff").unwrap(),
            );
            assert_eq!(
                decode_users(&Method::POST, path, None, &headers, body, ORIGIN).err(),
                Some(ApiError::UnsupportedMediaType)
            );
        }
    }

    #[test]
    fn every_mutation_requires_one_canonical_typed_uuid_v7_retry_key() {
        for (method, path, body, original) in valid_routes()
            .into_iter()
            .filter(|(method, _, _, _)| *method != Method::GET)
        {
            let request = decode_users(&method, &path, None, &original, body, ORIGIN).unwrap();
            assert_eq!(
                request.command_id.map(|id| id.to_string()).as_deref(),
                Some(COMMAND)
            );
            for key in [
                None,
                Some(""),
                Some("not-a-uuid"),
                Some("019CE56A-1B00-7000-8000-000000000002"),
                Some("019ce56a1b0070008000000000000002"),
                Some("019ce56a-1b00-4000-8000-000000000002"),
                Some("019ce56a-1b00-7000-c000-000000000002"),
                Some(" 019ce56a-1b00-7000-8000-000000000002"),
                Some("019ce56a-1b00-7000-8000-000000000002 "),
                Some("019ce56a-1b00-7000-8000-000000000002,019ce56a-1b00-7000-8000-000000000002"),
            ] {
                let mut headers = original.clone();
                headers.remove("idempotency-key");
                if let Some(key) = key {
                    headers.insert("idempotency-key", key.parse().unwrap());
                }
                assert_eq!(
                    decode_users(&method, &path, None, &headers, body, ORIGIN).err(),
                    Some(ApiError::InvalidInput)
                );
            }
            let mut headers = original.clone();
            headers.append("idempotency-key", COMMAND.parse().unwrap());
            assert_eq!(
                decode_users(&method, &path, None, &headers, body, ORIGIN).err(),
                Some(ApiError::InvalidInput)
            );
            headers.insert(
                "idempotency-key",
                http::HeaderValue::from_bytes(b"\xff").unwrap(),
            );
            assert_eq!(
                decode_users(&method, &path, None, &headers, body, ORIGIN).err(),
                Some(ApiError::InvalidInput)
            );
        }
    }

    #[test]
    fn reads_reject_all_retry_headers_instead_of_ignoring_them() {
        for (method, path, body, mut headers) in valid_routes()
            .into_iter()
            .filter(|(method, _, _, _)| *method == Method::GET)
        {
            for key in ["", COMMAND, "malformed"] {
                headers.insert("idempotency-key", key.parse().unwrap());
                assert_eq!(
                    decode_users(&method, &path, None, &headers, body, ORIGIN).err(),
                    Some(ApiError::InvalidInput)
                );
            }
            headers.append("idempotency-key", COMMAND.parse().unwrap());
            assert_eq!(
                decode_users(&method, &path, None, &headers, body, ORIGIN).err(),
                Some(ApiError::InvalidInput)
            );
        }
    }

    #[test]
    fn mutation_origin_is_required_and_read_origin_is_optional_but_exact_if_present() {
        for (method, path, body, original) in valid_routes() {
            for origin in [
                None,
                Some(""),
                Some("null"),
                Some("https://other.invalid"),
                Some("http://localhost:8787"),
                Some("https://localhost:8787/"),
                Some("HTTPS://localhost:8787"),
                Some(" https://localhost:8787"),
                Some("https://localhost:8787 "),
                Some("https://localhost:8787, https://localhost:8787"),
            ] {
                let mut headers = original.clone();
                headers.remove("origin");
                if let Some(origin) = origin {
                    headers.insert("origin", origin.parse().unwrap());
                }
                let result = decode_users(&method, &path, None, &headers, body, ORIGIN);
                if method == Method::GET && origin.is_none() {
                    assert!(result.is_ok());
                } else {
                    assert_eq!(result.err(), Some(ApiError::Forbidden));
                }
            }
            let mut headers = original.clone();
            headers.insert("origin", ORIGIN.parse().unwrap());
            assert!(decode_users(&method, &path, None, &headers, body, ORIGIN).is_ok());
            headers.append("origin", ORIGIN.parse().unwrap());
            assert_eq!(
                decode_users(&method, &path, None, &headers, body, ORIGIN).err(),
                Some(ApiError::Forbidden)
            );
            headers.insert("origin", http::HeaderValue::from_bytes(b"\xff").unwrap());
            assert_eq!(
                decode_users(&method, &path, None, &headers, body, ORIGIN).err(),
                Some(ApiError::Forbidden)
            );
        }
    }

    #[test]
    fn every_route_rejects_authorization_instead_of_substituting_developer_authority() {
        for (method, path, body, original) in valid_routes() {
            for authorization in ["", "Bearer", "Basic"] {
                let mut headers = original.clone();
                headers.insert("authorization", authorization.parse().unwrap());
                assert_eq!(
                    decode_users(&method, &path, None, &headers, body, ORIGIN).err(),
                    Some(ApiError::Forbidden)
                );
                headers.append("authorization", authorization.parse().unwrap());
                assert_eq!(
                    decode_users(&method, &path, None, &headers, body, ORIGIN).err(),
                    Some(ApiError::Forbidden)
                );
            }
        }
    }

    #[test]
    fn standard_header_names_are_case_insensitive_without_normalizing_values() {
        for (method, path, body, original) in valid_routes() {
            let mut headers = HeaderMap::new();
            for (name, value) in &original {
                headers.insert(
                    http::HeaderName::from_bytes(name.as_str().to_uppercase().as_bytes()).unwrap(),
                    value.clone(),
                );
            }
            assert!(decode_users(&method, &path, None, &headers, body, ORIGIN).is_ok());
        }
    }

    #[test]
    fn every_route_forwards_only_the_shared_optional_session_cookie_without_authenticating_it() {
        for (method, path, body, original) in valid_routes() {
            let request = decode_users(&method, &path, None, &original, body, ORIGIN).unwrap();
            assert!(request.session_token.is_none());
            for token in [
                String::new(),
                "invalid-session-shape".to_owned(),
                "A".repeat(43),
            ] {
                let mut headers = original.clone();
                headers.insert(
                    "cookie",
                    format!("other=value; __Host-brews_session={token}")
                        .parse()
                        .unwrap(),
                );
                let request = decode_users(&method, &path, None, &headers, body, ORIGIN).unwrap();
                assert!(
                    request.session_token.as_deref() == Some(token.as_str()),
                    "cookie must be forwarded without an authority claim"
                );
            }
            let mut headers = original.clone();
            headers.insert(
                "cookie",
                "__host-brews_session=ignored; other=value".parse().unwrap(),
            );
            let request = decode_users(&method, &path, None, &headers, body, ORIGIN).unwrap();
            assert!(request.session_token.is_none());
        }
    }

    #[test]
    fn every_route_reuses_shared_duplicate_cookie_and_malformed_header_rejection() {
        for (method, path, body, original) in valid_routes() {
            let mut headers = original.clone();
            headers.insert(
                "cookie",
                "__Host-brews_session=; __Host-brews_session="
                    .parse()
                    .unwrap(),
            );
            assert_eq!(
                decode_users(&method, &path, None, &headers, body, ORIGIN).err(),
                Some(ApiError::InvalidInput)
            );
            headers.insert("cookie", "other=value".parse().unwrap());
            headers.append("cookie", "another=value".parse().unwrap());
            assert_eq!(
                decode_users(&method, &path, None, &headers, body, ORIGIN).err(),
                Some(ApiError::InvalidInput)
            );
            headers.insert("cookie", http::HeaderValue::from_bytes(b"\xff").unwrap());
            assert_eq!(
                decode_users(&method, &path, None, &headers, body, ORIGIN).err(),
                Some(ApiError::InvalidInput)
            );
        }
    }

    #[test]
    fn users_and_auth_share_the_same_cookie_byte_limit() {
        let mut headers = HeaderMap::new();
        for (length, accepted) in [(8192, true), (8193, false)] {
            headers.insert("cookie", "x".repeat(length).parse().unwrap());
            let users = decode_users(&Method::GET, "/api/users", None, &headers, b"", ORIGIN);
            let auth = super::super::decode::decode(
                &Method::GET,
                "/api/session",
                None,
                &headers,
                b"",
                ORIGIN,
            );
            assert_eq!(users.is_ok(), accepted);
            assert_eq!(users.err(), auth.err());
        }
    }

    #[test]
    fn request_cleanup_zeroizes_its_optional_session_buffer_without_changing_command() {
        let mut request = read("/api/users").unwrap();
        request.session_token = Some("A".repeat(43));
        request.zeroize();
        assert!(request.session_token.is_none());
        assert!(matches!(
            request.command,
            ManagementCommand::ListAccounts {
                after: None,
                limit: 50
            }
        ));
        request.zeroize();
        assert!(request.session_token.is_none());
        drop(request);
    }

    fn read(path: &str) -> Result<UsersRequest, ApiError> {
        decode_users(&Method::GET, path, None, &HeaderMap::new(), b"", ORIGIN)
    }

    #[test]
    fn account_get_preserves_the_canonical_typed_target() {
        let request = read(&format!("/api/users/{ACCOUNT}")).unwrap();
        assert!(
            matches!(request.command, ManagementCommand::GetAccount { account_id } if account_id.to_string() == ACCOUNT)
        );
        assert!(request.command_id.is_none());
    }

    #[test]
    fn unknown_or_noncanonical_paths_are_not_routes() {
        let bad_ids = [
            "",
            "audit",
            "not-a-uuid",
            "019CE56A-1B00-7000-8000-000000000001",
            "019ce56a1b0070008000000000000001",
            "019ce56a-1b00-4000-8000-000000000001",
            "019ce56a-1b00-7000-c000-000000000001",
            "%3019ce56a-1b00-7000-8000-000000000001",
            "019ce56a-1b00-7000-8000-000000000001%2f",
            "019ce56a-1b00-7000-8000-000000000001/",
        ];
        for id in bad_ids {
            assert_eq!(
                read(&format!("/api/users/{id}")).err(),
                Some(ApiError::NotFound)
            );
        }
        for path in [
            "/api/users/",
            "/api/users//",
            "/api/Users",
            "/api/users?limit=1",
            "/api/users#fragment",
            "/_dev/accounts",
            "/management",
            "/api/users/audit/events",
        ] {
            assert_eq!(read(path).err(), Some(ApiError::NotFound));
        }
    }

    #[test]
    fn known_account_and_collection_paths_reject_other_methods() {
        for path in ["/api/users".to_owned(), format!("/api/users/{ACCOUNT}")] {
            for method in [
                Method::POST,
                Method::PUT,
                Method::PATCH,
                Method::HEAD,
                Method::OPTIONS,
            ] {
                assert_eq!(
                    decode_users(&method, &path, None, &HeaderMap::new(), b"", ORIGIN).err(),
                    Some(ApiError::MethodNotAllowed)
                );
            }
        }
    }

    #[test]
    fn collection_get_maps_to_default_list_without_claiming_authority() {
        let request = decode_users(
            &Method::GET,
            "/api/users",
            None,
            &HeaderMap::new(),
            b"",
            ORIGIN,
        )
        .unwrap();
        assert!(matches!(
            request.command,
            ManagementCommand::ListAccounts {
                after: None,
                limit: 50
            }
        ));
        assert!(request.command_id.is_none());
        assert!(request.session_token.is_none());
    }
}
