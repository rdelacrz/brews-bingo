//! Exact-source native verification of the private Worker game boundary.
#![allow(clippy::unwrap_used, clippy::expect_used, reason = "Tests fail fast.")]
#[path = "../src/worker_runtime/game_wire.rs"]
mod game_wire;

pub use brews_backend::{api, security};
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
