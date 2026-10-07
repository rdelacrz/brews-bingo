#![allow(clippy::unwrap_used, reason = "Grammar fixtures fail fast.")]
use brews_backend::api::{self, ApiError};
use http::{HeaderMap, Method};
const ORIGIN: &str = "https://localhost:8787";
#[test]
fn history_collection_get_is_a_bodyless_account_cookie_read() {
    assert!(
        api::decode_history(
            &Method::GET,
            "/api/history",
            None,
            &HeaderMap::new(),
            b"",
            ORIGIN
        )
        .is_ok(),
        "History collection route must decode"
    );
}

#[test]
fn history_cells_reject_positional_arrays_at_every_nested_boundary() {
    let player = r#"{"player_id":"019ce56a-1b00-7000-8000-000000000001","alias":"First","side_length":2,"cells":[{"position":[0,0],"kind":{"kind":"value","value":"1"},"is_matched":false}]}"#;
    assert!(
        serde_json::from_str::<brews_contracts::history::HistoryPlayer>(player).is_err(),
        "History cell position must be map-only"
    );
}

fn read_history(
    path: &str,
    query: Option<&str>,
    headers: &HeaderMap,
    body: &[u8],
) -> Result<api::HistoryRequest, ApiError> {
    api::decode_history(&Method::GET, path, query, headers, body, ORIGIN)
}
#[test]
fn history_queries_are_canonical_bounded_unique_and_filter_bound() {
    use api::{HistoryCommand, HistoryCursor};
    let id = "019ce56a-1b00-7000-8000-000000000001".parse().unwrap();
    let cursor = HistoryCursor {
        ended_at: 123,
        game_id: id,
    }
    .encode(Some(brews_domain::games::GameState::Cancelled));
    let request = read_history(
        "/api/history",
        Some(&format!("cursor={cursor}&limit=50&outcome=cancelled")),
        &HeaderMap::new(),
        b"",
    )
    .unwrap();
    assert!(
        matches!(request.command,HistoryCommand::List(q) if q.limit==50 && q.after.unwrap().game_id==id)
    );
    for query in [
        "",
        "limit",
        "limit=",
        "limit=0",
        "limit=01",
        "limit=51",
        "limit=-1",
        "limit=+1",
        "limit=%31",
        "limit=１",
        "limit=1&limit=1",
        "limit=1&",
        "outcome=Resolved",
        "outcome=resolved&outcome=resolved",
        "host=other",
        "search=winner",
        "start=1",
        "cursor=",
        "cursor=null",
        "limit=1;outcome=resolved",
        "limit=1&unknown=1",
    ] {
        assert_eq!(
            read_history("/api/history", Some(query), &HeaderMap::new(), b"").err(),
            Some(ApiError::InvalidInput),
            "query grammar"
        );
    }
    assert_eq!(
        read_history(
            "/api/history",
            Some(&format!("cursor={cursor}&outcome=resolved")),
            &HeaderMap::new(),
            b""
        )
        .err(),
        Some(ApiError::InvalidInput)
    );
    assert_eq!(
        read_history(
            "/api/history",
            Some(&"x".repeat(513)),
            &HeaderMap::new(),
            b""
        )
        .err(),
        Some(ApiError::InvalidInput)
    );
    for path in [
        "/api/history/",
        "/api/History",
        "/api/history/019CE56A-1B00-7000-8000-000000000001",
        "/api/history/019ce56a-1b00-4000-8000-000000000001",
        "/api/history/019ce56a-1b00-7000-8000-000000000001/",
        "/api/history/%3019ce56a-1b00-7000-8000-000000000001",
    ] {
        assert_eq!(
            read_history(path, None, &HeaderMap::new(), b"").err(),
            Some(ApiError::NotFound)
        );
    }
}
#[test]
fn history_read_grammar_never_accepts_bearer_retry_body_or_ambiguous_cookie() {
    for path in [
        "/api/history".to_owned(),
        "/api/history/019ce56a-1b00-7000-8000-000000000001".to_owned(),
    ] {
        for (header, value, error) in [
            ("authorization", "Bearer", ApiError::Forbidden),
            ("idempotency-key", "", ApiError::InvalidInput),
            ("origin", "null", ApiError::Forbidden),
            (
                "cookie",
                "__Host-brews_session=a; __Host-brews_session=b",
                ApiError::InvalidInput,
            ),
        ] {
            let mut h = HeaderMap::new();
            h.insert(header, value.parse().unwrap());
            assert_eq!(read_history(&path, None, &h, b"").err(), Some(error));
        }
        let mut headers = HeaderMap::new();
        headers.insert("origin", ORIGIN.parse().unwrap());
        assert!(read_history(&path, None, &headers, b"").is_ok());
        headers.append("origin", ORIGIN.parse().unwrap());
        assert_eq!(
            read_history(&path, None, &headers, b"").err(),
            Some(ApiError::Forbidden)
        );
        for body in [b" ".as_slice(), b"{}", b"null", b"[]"] {
            assert_eq!(
                read_history(&path, None, &HeaderMap::new(), body).err(),
                Some(ApiError::InvalidInput)
            );
        }
        assert_eq!(
            read_history(&path, None, &HeaderMap::new(), &vec![b' '; 4097]).err(),
            Some(ApiError::PayloadTooLarge)
        );
        assert_eq!(
            api::decode_history(&Method::POST, &path, None, &HeaderMap::new(), b"", ORIGIN).err(),
            Some(ApiError::MethodNotAllowed)
        );
    }
    assert_eq!(
        read_history(
            "/api/history/019ce56a-1b00-7000-8000-000000000001",
            Some(""),
            &HeaderMap::new(),
            b""
        )
        .err(),
        Some(ApiError::InvalidInput)
    );
}
