//! Exact-source native verification of the private Worker game boundary.
#![allow(clippy::unwrap_used, clippy::expect_used, reason = "Tests fail fast.")]
#[path = "../src/worker_runtime/game_wire.rs"]
mod game_wire;

pub use brews_backend::{auth, security};
#[path = "../src/api/mod.rs"]
#[allow(
    dead_code,
    unused_imports,
    reason = "Exact-source API module is wider than this transport harness."
)]
mod api;
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
    let bytes = format!(
        r#"{{"owner_result":"synced","valid_until":100,"player_authority":{{"game_id":"{id}","session_id":"{id}","player_id":"{id}","epoch":1,"session_expires_at":200,"valid_until":100}},"response":{{"up_to_date":true,"view_revision":2,"snapshot":null}}}}"#
    );
    let reply = GameOwnerResponse::decode_json(bytes.as_bytes()).unwrap();
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

#[test]
fn spectator_sync_release_wire_is_closed_and_binds_role_identity_and_deadline() {
    use game_wire::{AudienceReleaseAttempt, AudienceReleaseReply, GameOwnerResponse};
    let id = "01890f3e-53b7-7d28-9b05-4f65092d5711";
    let binding = serde_json::json!({"game_id":id,"session_id":id,"spectator_id":id,"epoch":0,"session_expires_at":86400001,"valid_until":300001});
    let release = serde_json::json!({"challenge":vec![1u8;32],"request":{"release":"sync","token":"A".repeat(brews_backend::security::TOKEN_ENCODED_LEN),"authority":binding}});
    let parsed: AudienceReleaseAttempt = serde_json::from_value(release.clone()).unwrap();
    let acknowledgement_binding = serde_json::to_value(parsed.binding().unwrap()).unwrap();
    let _: AudienceReleaseReply = serde_json::from_value(
        serde_json::json!({"release_result":"released","binding":acknowledgement_binding,"valid_until":300001,"checked_at":1}),
    )
    .unwrap();
    assert!(
        serde_json::from_value::<AudienceReleaseReply>(
            serde_json::json!({"release_result":"released","valid_until":300001,"checked_at":1})
        )
        .is_err()
    );
    let mut positional = release.clone();
    positional["request"]["authority"] = serde_json::json!([id, id, id, 0, 86400001, 300001]);
    assert!(serde_json::from_value::<AudienceReleaseAttempt>(positional).is_err());
    let mut unknown = release;
    unknown["capabilities"] = serde_json::json!([]);
    assert!(serde_json::from_value::<AudienceReleaseAttempt>(unknown).is_err());
    let reply = serde_json::json!({"owner_result":"audience_synced","authority":binding,"response":{"up_to_date":true,"view_revision":1,"snapshot":null}});
    assert!(GameOwnerResponse::decode_json(&serde_json::to_vec(&reply).unwrap()).is_ok());
    let mut positional = reply.clone();
    positional["authority"] = serde_json::json!([id, id, id, 0, 86400001, 300001]);
    assert!(GameOwnerResponse::decode_json(&serde_json::to_vec(&positional).unwrap()).is_err());
    let request = game_wire::GameIngress {
        operation: game_wire::GameAction::Sync,
        game_id: Some(id.parse().unwrap()),
        command_id: None,
        view: Some(game_wire::ViewSelector::Spectator),
        known_revision: Some(1),
        session_token: None,
        admission_token: None,
        recovery_caller: None,
        body: String::new(),
    };
    for (key, value) in [
        ("epoch", -1),
        ("valid_until", 86400002),
        (
            "session_expires_at",
            brews_contracts::games::MAX_SAFE_TIMESTAMP + 1,
        ),
    ] {
        let mut invalid = reply.clone();
        invalid["authority"][key] = value.into();
        assert!(
            GameOwnerResponse::decode_json(&serde_json::to_vec(&invalid).unwrap())
                .unwrap()
                .validate_for(&request)
                .is_err()
        );
    }
}
