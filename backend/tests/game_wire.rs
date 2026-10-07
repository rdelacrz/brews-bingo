//! Exact-source native verification of the private Worker game boundary.
#![allow(clippy::unwrap_used, clippy::expect_used, reason = "Tests fail fast.")]
#[path = "../src/worker_runtime/game_wire.rs"]
mod game_wire;

pub use brews_backend::security;
#[path = "../src/api/game_body.rs"]
mod game_body;
mod api {
    pub(crate) use crate::game_body::decode_empty_game_body;
    pub use brews_backend::api::*;
}
#[path = "../src/limits.rs"]
#[allow(
    dead_code,
    reason = "Exact-source policy module is wider than this transport harness."
)]
mod limits;

#[test]
fn private_reply_never_replays_a_player_cookie_with_a_committed_receipt() {
    use game_wire::GameOwnerResponse;
    let id = "01890f3e-53b7-7d28-9b05-4f65092d5711";
    let encoded = format!(
        r#"{{"owner_result":"success","response":{{"result":"committed","receipt":{{"version":1,"command_id":"{id}","game_id":"{id}","outcome":{{"operation":"started","view_revision":1,"started_at":1}},"completed_at":1,"expires_at":86400001}}}},"cookie":{{"kind":"player","token":"{}","expires_at":86400001}}}}"#,
        "A".repeat(brews_backend::security::TOKEN_ENCODED_LEN)
    );
    assert!(GameOwnerResponse::decode_json(encoded.as_bytes()).is_err());
}

#[test]
fn private_decoded_command_preserves_original_payload_and_transfers_secrets() {
    use game_wire::GameIngress;
    let body = br#"{"configuration":{"board_side_length":4}}"#;
    let mut headers = http::HeaderMap::new();
    headers.insert("content-type", "application/json".parse().unwrap());
    headers.insert("origin", "https://localhost:8787".parse().unwrap());
    headers.insert(
        "idempotency-key",
        "01890f3e-53b7-7d28-9b05-4f65092d5711".parse().unwrap(),
    );
    headers.insert("cookie", "__Host-brews_session=fixture".parse().unwrap());
    let mut decoded = api::decode_games(
        &http::Method::POST,
        "/api/games",
        None,
        &headers,
        body,
        "https://localhost:8787",
    )
    .unwrap();
    let wire = GameIngress::from_decoded(&mut decoded, body).unwrap();
    assert!(decoded.session_token.is_none());
    assert_eq!(wire.body.as_bytes(), body);
    assert!(wire.session_token.as_deref() == Some("fixture"));
    let bytes = serde_json::to_vec(&wire).unwrap();
    assert!(GameIngress::decode_json(&bytes).is_ok());
}

#[test]
fn private_reply_rejects_a_positional_cookie_variant() {
    use game_wire::GameOwnerResponse;
    let id = "01890f3e-53b7-7d28-9b05-4f65092d5711";
    let encoded = format!(
        r#"{{"owner_result":"success","response":{{"result":"committed","receipt":{{"version":1,"command_id":"{id}","game_id":"{id}","outcome":{{"operation":"started","view_revision":1,"started_at":1}},"completed_at":1,"expires_at":86400001}}}},"cookie":["none"]}}"#
    );
    assert!(GameOwnerResponse::decode_json(encoded.as_bytes()).is_err());
}

#[test]
fn private_reply_none_cookie_is_a_closed_object_in_original_bytes() {
    use game_wire::GameOwnerResponse;
    let id = "01890f3e-53b7-7d28-9b05-4f65092d5711";
    let prefix = format!(
        r#"{{"owner_result":"success","response":{{"result":"committed","receipt":{{"version":1,"command_id":"{id}","game_id":"{id}","outcome":{{"operation":"started","view_revision":1,"started_at":1}},"completed_at":1,"expires_at":86400001}}}},"cookie":"#
    );
    let valid = format!("{prefix}{{\"kind\":\"none\"}}}}");
    assert!(GameOwnerResponse::decode_json(valid.as_bytes()).is_ok());
    assert!(serde_json::from_slice::<GameOwnerResponse>(valid.as_bytes()).is_ok());
    for cookie in [
        r#"{"kind":"none","unknown":1}"#,
        r#"{"kind":"none","\u0074oken":null}"#,
        r#"{"kind":"none","expires_at":null}"#,
        r#"{"kind":"none","kind":"none"}"#,
        r#"{"kind":"none","\u006bind":"none"}"#,
        r#"["none"]"#,
    ] {
        let invalid = format!("{prefix}{cookie}}}");
        assert!(
            GameOwnerResponse::decode_json(invalid.as_bytes()).is_err(),
            "{cookie}"
        );
        assert!(
            serde_json::from_slice::<GameOwnerResponse>(invalid.as_bytes()).is_err(),
            "{cookie}"
        );
    }
}

#[test]
fn private_reply_must_match_the_exact_command_operation_and_target() {
    use game_wire::{GameIngress, GameOwnerResponse};
    let id = "01890f3e-53b7-7d28-9b05-4f65092d5711";
    let other = "01890f3e-53b7-7d28-9b05-4f65092d5712";
    let request = format!(
        r#"{{"operation":"start","game_id":"{id}","command_id":"{id}","view":null,"known_revision":null,"session_token":null,"admission_token":null,"body":"{{\"expected_revision\":0}}"}}"#
    );
    let request = GameIngress::decode_json(request.as_bytes()).unwrap();
    let committed = format!(
        r#"{{"owner_result":"success","response":{{"result":"committed","receipt":{{"version":1,"command_id":"{id}","game_id":"{id}","outcome":{{"operation":"started","view_revision":1,"started_at":1}},"completed_at":1,"expires_at":86400001}}}},"cookie":{{"kind":"none"}}}}"#
    );
    let matching = GameOwnerResponse::decode_json(committed.as_bytes()).unwrap();
    assert!(matching.validate_for(&request).is_ok());
    for encoded in [
        committed.replace(
            &format!(r#""command_id":"{id}""#),
            &format!(r#""command_id":"{other}""#),
        ),
        committed.replace(
            &format!(r#""game_id":"{id}""#),
            &format!(r#""game_id":"{other}""#),
        ),
        committed.replace(
            r#""operation":"started","view_revision":1,"started_at":1"#,
            r#""operation":"lobby_opened","view_revision":1"#,
        ),
    ] {
        let reply = GameOwnerResponse::decode_json(encoded.as_bytes()).unwrap();
        assert!(reply.validate_for(&request).is_err());
    }
}

#[test]
fn private_up_to_date_sync_is_bound_to_the_exact_requested_known_revision() {
    use game_wire::{GameIngress, GameOwnerResponse};
    let id = "01890f3e-53b7-7d28-9b05-4f65092d5711";
    let bytes = br#"{"owner_result":"synced","response":{"up_to_date":true,"view_revision":2,"snapshot":null}}"#;
    let reply = GameOwnerResponse::decode_json(bytes).unwrap();
    let request = |known: &str| {
        let bytes = format!(
            r#"{{"operation":"sync","game_id":"{id}","command_id":null,"view":"player","known_revision":{known},"session_token":null,"admission_token":null,"body":""}}"#
        );
        GameIngress::decode_json(bytes.as_bytes()).unwrap()
    };
    assert!(reply.validate_for(&request("2")).is_ok());
    for known in ["null", "0", "1", "3"] {
        assert!(
            reply.validate_for(&request(known)).is_err(),
            "known_revision={known}"
        );
    }
    // The standalone public DTO has no request context and retains its existing meaning.
    assert!(
        brews_contracts::games::SyncResponse::decode_json(
            br#"{"up_to_date":true,"view_revision":2,"snapshot":null}"#
        )
        .is_ok()
    );
}

#[test]
fn private_synced_reply_is_a_closed_map_in_original_bytes() {
    use game_wire::GameOwnerResponse;
    let valid = br#"{"owner_result":"synced","response":{"up_to_date":true,"view_revision":2,"snapshot":null}}"#;
    assert!(GameOwnerResponse::decode_json(valid).is_ok());
    for response in [
        r#"{"up_to_date":true,"view_revision":2,"snapshot":null,"unknown":1}"#,
        r#"{"up_to_date":true,"\u0075p_to_date":true,"view_revision":2,"snapshot":null}"#,
        r#"{"up_to_date":true,"view_revision":2,"snapshot":null,"snapshot":null}"#,
        r#"{"up_to_date":true,"view_revision":2}"#,
        r#"[true,2,null]"#,
    ] {
        let bytes = format!(r#"{{"owner_result":"synced","response":{response}}}"#);
        assert!(
            GameOwnerResponse::decode_json(bytes.as_bytes()).is_err(),
            "{response}"
        );
        assert!(
            serde_json::from_slice::<GameOwnerResponse>(bytes.as_bytes()).is_err(),
            "{response}"
        );
    }
}

#[test]
fn private_ingress_rejects_duplicate_and_nonobject_commands() {
    use game_wire::GameIngress;
    let id = "01890f3e-53b7-7d28-9b05-4f65092d5711";
    let valid = format!(
        r#"{{"operation":"start","game_id":"{id}","command_id":"{id}","view":null,"known_revision":null,"session_token":null,"admission_token":null,"body":"{{\"expected_revision\":0}}"}}"#
    );
    assert!(GameIngress::decode_json(valid.as_bytes()).is_ok());
    let duplicate = valid.replacen(
        "\"operation\":\"start\"",
        "\"operation\":\"start\",\"\\u006fperation\":\"start\"",
        1,
    );
    assert!(GameIngress::decode_json(duplicate.as_bytes()).is_err());
    assert!(GameIngress::decode_json(b"[]").is_err());
}
