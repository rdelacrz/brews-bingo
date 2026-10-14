# Graph Report - /home/orangepi/Workspace/brews-bingo  (2026-10-14)

## Corpus Check
- 264 files · ~0 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 5113 nodes · 15532 edges · 211 communities (204 shown, 7 thin omitted)
- Extraction: 94% EXTRACTED · 6% INFERRED · 0% AMBIGUOUS · INFERRED: 897 edges (avg confidence: 0.76)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `af4d4083`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- Game Accounts
- Game
- Games Core
- Account Management
- Deployment
- Game Wire
- Operator
- Games
- Games
- Spectators
- Observability
- Games
- Directory
- Game Object
- Game Delivery
- Spectators
- Migration
- Game Authority
- Games
- Directory Games
- Test Deployment
- Mod
- Directory Games
- Game Sockets
- Auth Audit
- Test Release Gate
- Games
- Users
- Shared
- Removal-Recovery.Test
- Directory
- Management
- Player Recovery
- Users Core
- Service
- Shared
- Game Reads
- Player Recovery
- Mod
- Operator Runtime
- Scenario
- Game Directory
- Games
- Directory Host Transfer
- Shared
- Decode
- Shared
- Reads
- Game Authority
- Mod
- Users
- Test Full Game E2E
- Game Resources
- Users
- Privileged Audit
- Test Full Game E2E
- Mod
- Terminal
- Audit Repairs
- Discovery.Test
- Backend
- Shared
- Test Full Game E2E
- Sql Policy Tags
- Shared
- Tests
- Shared
- Games
- Removal
- Removal
- Player Lifecycle
- Operator Peers
- Command Lookup.Test
- Args
- Operator
- Directory Games
- History
- Resolution
- Read Routes.Test
- Discovery
- Game Reads
- Mod
- Mod
- Test Full Game E2E
- Reads
- Local Readiness
- Shared
- Users
- Reads
- Mod
- Operator Peers
- Command Lookup
- Receipt Atomicity
- Player Lifecycle Boundaries.Test
- Test Support
- Wire
- Management
- Player Lifecycle Review
- Mod
- Operator.Test
- History
- Shared
- Test Full Game E2E
- Shared
- Frontend
- Host Recipient
- Terminal
- Link File
- Game
- Wire
- Game Object.Test
- Output
- Reads
- Discovery
- Shared
- Mod
- Protected Response Snapshot.Test
- Spectators.Test
- Shared
- Game Resource Probe
- Write Regressions
- Developer
- Command Lookup
- Discovery
- Terminal
- Dedicated Reads Https Wss
- Game Audit Https Wss
- Game Resources.Test
- Directory Schema
- Directory
- Owner
- Gameplay Https Wss
- Game Storage Audit
- Start Publication
- Operator Https Wss
- Backend
- Auth Schema
- Edge
- Management
- Player Lifecycle.Test
- Discovery
- Commands
- Deployment Identity
- Terminal Regressions
- Shared
- Shared
- Terminal Faults
- History Api
- Game Audit Repairs.Test
- Player Recovery Boundaries.Test
- Mod
- Sync Output
- Recovery Answers
- Privileged Game Audit.Test
- Connection Capacity
- Game Schema
- Previous Command Game Schema
- Previous Read Game Schema
- Previous Saved Game Schema
- Directory
- Shared
- Storage Bounds.Test
- Mod
- Backend Once Error
- Backend Once Success
- Typed Struct
- Links
- Host Transfer
- Discovery Boundaries.Test
- Player Recovery Review Regressions.Test
- Frontend
- Maintenance
- Mod
- Protocol
- Mod
- Mod
- Game Operator Schema
- Previous Command Operator Schema
- Previous Read Operator Schema
- Previous Saved Operator Schema
- Operator Peers
- Accounts
- Audit
- Origin
- Game Delivery Schema
- History
- Main
- Once Concurrent Error
- Once Scopes
-   Init
- Error

## God Nodes (most connected - your core abstractions)
1. `GameError` - 363 edges
2. `migrate_directory()` - 133 edges
3. `GameAccountAuthority` - 111 edges
4. `AuthError` - 103 edges
5. `migrate()` - 101 edges
6. `StorageError` - 97 edges
7. `post()` - 95 edges
8. `DirectoryError` - 85 edges
9. `E2EError` - 76 edges
10. `ApiError` - 73 edges

## Surprising Connections (you probably didn't know these)
- `sqlite_username_boundaries_match_domain_validation_on_insert_and_update()` --calls--> `validate_username()`  [INFERRED]
  backend/src/db/schema/auth_schema.rs → shared/domain/src/accounts.rs
- `GameOwnerResponse` --references--> `HostTransferTargets`  [EXTRACTED]
  backend/src/worker_runtime/game_wire.rs → shared/contracts/src/games.rs
- `validate_summary()` --calls--> `validate_alias()`  [INFERRED]
  backend/src/worker_runtime/history/wire.rs → shared/domain/src/games.rs
- `list()` --references--> `ManagementCommand`  [EXTRACTED]
  backend/tests/users_core.rs → shared/contracts/src/management.rs
- `CliRequest` --references--> `ManagementCommand`  [EXTRACTED]
  backend/src/api/developer.rs → shared/contracts/src/management.rs

## Import Cycles
- 2-file cycle: `backend/src/db/game.rs -> backend/src/db/game/player_lifecycle.rs -> backend/src/db/game.rs`

## Communities (211 total, 7 thin omitted)

### Community 0 - "Game Accounts"
Cohesion: 0.05
Nodes (99): DiscoveryDirectoryRequest, account_registration_and_post_gate_recheck_use_real_accounts_core(), AccountAction, AccountAuthorityWire, AccountCloseIdentity, AccountCloseReply, AccountCloseRequest, AccountConnectionIdentity (+91 more)

### Community 1 - "Game"
Cohesion: 0.08
Nodes (32): add_time(), Admission, blob(), GameService<'a, D, R>, number(), parse_transfer(), Row, optional_number() (+24 more)

### Community 2 - "Games Core"
Cohesion: 0.06
Nodes (94): creation_fingerprint(), Err, Result, Self, account_socket_grants_require_fresh_accounts_and_do_not_renew_expiry_or_presence(), actual_sqlite_reopen_preserves_lobby_intent_and_lost_publication_ack(), admission_is_verifier_only_short_lived_and_consumed_retries_never_mint_cookies(), answer_kdf_runs_outside_sqlite_write_and_rechecks_real_competing_admission() (+86 more)

### Community 3 - "Account Management"
Cohesion: 0.07
Nodes (99): migrate(), D, a_current_host_session_is_attributed_when_admin_authority_is_denied(), account_detail_is_safe_and_deleted_or_unknown_target_is_not_found(), account_pages_are_bounded_and_continue_without_duplicates(), audit_write_failure_rolls_back_account_link_and_receipt(), authenticated_rejections_are_durably_audited(), command_id() (+91 more)

### Community 4 - "Deployment"
Cohesion: 0.08
Nodes (80): AST, Path, active_deployment(), canonical(), check_label(), cloudflare(), deploy(), DeploymentError (+72 more)

### Community 5 - "Game Wire"
Cohesion: 0.05
Nodes (56): GameOperation, GameViewSelector, GameId, SpectatorAuthority, GameId, PlayerId, SessionId, SyncPlayerBinding (+48 more)

### Community 6 - "Operator"
Cohesion: 0.08
Nodes (42): GameAccountAuthority, Runtime, account_actor(), authorize(), authorize_mutation(), ensure_before_idle(), bounded_random_index(), call_fingerprint() (+34 more)

### Community 7 - "Games"
Cohesion: 0.06
Nodes (25): CodeGrant, CreationAck, CreationFingerprint, CreationReadyProof, CreationWork, DiscoveryRoute, GameProjection, ProjectionAck (+17 more)

### Community 8 - "Games"
Cohesion: 0.06
Nodes (57): admission_and_join_validate_lookup_alias_and_never_debug_recovery_material(), assert_response_rejected(), assert_view_rejected(), created_completion_can_be_later_than_creation_without_moving_idle_deadline(), creation_resolves_defaults_and_overrides_with_effective_center(), game_tags_and_safe_receipt_encoding_round_trip_without_parser_details(), gameplay_command_inputs_are_closed_canonical_and_state_fenced(), hardened_tagged_maps_preserve_original_duplicates_and_closed_fields() (+49 more)

### Community 9 - "Spectators"
Cohesion: 0.11
Nodes (28): advance(), make_id(), Uuid, actor(), AudienceCommand, AudienceMutation, digest(), fingerprint() (+20 more)

### Community 10 - "Observability"
Cohesion: 0.05
Nodes (59): Arc, api_failure(), auth_failure(), Boundary, boundary_failures_have_closed_fields_and_expected_levels(), Capture, Delivery, Failure (+51 more)

### Community 11 - "Games"
Cohesion: 0.09
Nodes (32): GamePayload, AdmissionContextInput, BoundedJsonSink, CallManualInput, CallView, CancelGameInput, checked_deadline(), ConfigurationOverrides (+24 more)

### Community 12 - "Directory"
Cohesion: 0.11
Nodes (68): migrate_directory(), account(), acquire_persists_a_presence_only_gate_with_separate_operation_identity(), acquired_gate_rejects_transfer_without_changing_the_original_host(), competing_operation_returns_busy_without_replacing_the_current_gate(), completed_operation_cannot_reacquire_or_clear_a_newer_gate(), concurrent_operations_on_one_account_serialize_to_one_grant_and_one_busy(), corrupted_completion_time_cannot_erase_the_fence_or_authorize_an_ack() (+60 more)

### Community 13 - "Game Object"
Cohesion: 0.10
Nodes (36): GameService, D, R, GameObject, OwnerDatabase, Self, close_checked(), GameObject (+28 more)

### Community 14 - "Game Delivery"
Cohesion: 0.09
Nodes (50): binding_params(), DeliveryBinding, DeliveryBudget, DeliveryError, DeliveryService, DeliveryService<'a, D, R>, DeliveryService<'_, D, R>, migrate_game_delivery() (+42 more)

### Community 15 - "Spectators"
Cohesion: 0.06
Nodes (53): String, Zeroizing, SecretCookie, alias_exact_trim_noop_recovery_and_membership_session_invariants(), connect(), durable_close_identity_and_stale_ack_never_touch_successor_or_expiry(), inprogress_leave_retains_gameplay_return_authority_and_timestamp_and_retry_preserves_successor(), inprogress_timestamp_ignore_abort_roll_back_closure_and_receipt_and_answer_recovery_retains_event() (+45 more)

### Community 16 - "Migration"
Cohesion: 0.13
Nodes (56): canonicalize_table_identifiers(), command_lookup_migration_counter_six_rejects_every_table_column_and_index_drift_without_writes(), command_lookup_migration_ignored_or_failed_counter_rolls_back_schema_and_rows(), command_lookup_migration_known_counter_six_preserves_all_tables_and_audit(), dedicated_read_audit_migration_ignored_or_failed_marker_rolls_back_ddl_and_rows(), dedicated_read_audit_migration_preserves_frozen_published_inventory_rows_and_optional_delivery(), dedicated_read_audit_migration_rejects_full_unknown_shape_before_any_write(), dedicated_read_published_constraint_foreign_key_strict_and_partial_delivery_drift_writes_nothing() (+48 more)

### Community 17 - "Game Authority"
Cohesion: 0.09
Nodes (25): AuthService<'_, D, R>, command(), GameSocketCloseWork, removal_finishes_only_after_all_exact_socket_close_acknowledgements(), AccountId, Cell, CommandId, Connection (+17 more)

### Community 18 - "Games"
Cohesion: 0.07
Nodes (44): PublicTerminalNotice, GameId, Option, FnMut, BoardView, validate_projection_player(), history_cells(), D (+36 more)

### Community 19 - "Directory Games"
Cohesion: 0.14
Nodes (58): accepted_started_terminal_proof_cannot_rewrite_original_expiry_with_newer_revision(), account(), assignment_gate_blocks_newer_projection_to_removing_host(), claim_reserves_identity_indexes_host_and_replays_same_command(), clock_advance_during_entropy_rechecks_common_fresh_admission_floor(), code_rejection_sampling_has_bounded_entropy_and_rolls_back_on_exhaustion(), command(), completion_cleanup_retires_commands_at_exact_boundary_but_keeps_pending() (+50 more)

### Community 20 - "Test Deployment"
Cohesion: 0.08
Nodes (5): bindings(), deployment(), DeploymentTests, Local fixture and mocked provider boundaries; no authenticated provider calls., targets()

### Community 21 - "Mod"
Cohesion: 0.11
Nodes (44): absent_after_deadline_keeps_reservation_and_saturating_retry(), command(), Fixture, genuine_committed_lobby_recovers_directory_publication_without_faking_game_ack(), genuine_initializer_evidence_acknowledges_only_current_unready_creation(), genuine_new_cancellation_releases_only_the_hidden_directory_association(), genuine_owner_terminal_evidence_releases_reservation_after_deadline(), hidden_code_release_preserves_exact_terminal_identity_and_publication_fences() (+36 more)

### Community 22 - "Directory Games"
Cohesion: 0.17
Nodes (19): command_timestamp(), deadline(), DirectoryService<'_, D, R>, integer(), optional_integer(), parse_creation(), parse_index(), parse_optional_integer() (+11 more)

### Community 23 - "Game Sockets"
Cohesion: 0.13
Nodes (24): Attachment, CloseCode, connection_id(), GameObject, method(), ready_state(), AccountId, Cell (+16 more)

### Community 24 - "Auth Audit"
Cohesion: 0.10
Nodes (41): AuditDb, authorize(), Completed, connection_id(), context(), decision_sample_fences_current_game_connection_and_admin_expiry_denials(), Fault, fence() (+33 more)

### Community 25 - "Test Release Gate"
Cohesion: 0.06
Nodes (9): Resource summary parser fixtures are unit data, not measured service output., ResourceParserTests, ExecutionTests, historical_report(), Parser/unit fault tests; synthetic reporter fixtures are not execution evidence., ReporterTests, snapshot_report(), snapshot_summary() (+1 more)

### Community 26 - "Games"
Cohesion: 0.11
Nodes (44): admission_context_is_keyless_and_join_uses_separate_game_scoped_cookie(), admission_forwards_an_existing_player_cookie_without_claiming_authority(), all_transport_inputs_have_release_active_finite_bounds(), audit_account_view(), audit_read_rejection_context(), command_id(), creation_route_decodes_original_configuration_without_caller_identity(), decode_empty_game_body() (+36 more)

### Community 27 - "Users"
Cohesion: 0.09
Nodes (40): account_get_preserves_the_canonical_typed_target(), action_routes_reject_other_methods_without_decoding_payloads(), bodyless_mutation_routes_map_to_their_closed_management_commands(), bodyless_mutations(), collection_get_maps_to_default_list_without_claiming_authority(), collection_query(), collection_query_byte_bound_precedes_parsing(), collection_query_maps_only_cursor_and_bounded_limit_in_either_order() (+32 more)

### Community 28 - "Shared"
Cohesion: 0.10
Nodes (39): GameService<'_, D, R>, GameId, Result, capture_owner_failure(), confirm_expiry_denial(), current(), current_targeted(), dispatch() (+31 more)

### Community 29 - "Removal-Recovery.Test"
Cohesion: 0.11
Nodes (30): armClock(), auth(), authorize(), bearer(), completed(), floor(), gameAuth(), normalFixture() (+22 more)

### Community 30 - "Directory"
Cohesion: 0.17
Nodes (25): application_tables(), DirectoryService, DirectoryService<'a, D, R>, operation_timestamp(), parse_completion_row(), parse_operation_row(), AccountId, D (+17 more)

### Community 31 - "Management"
Cohesion: 0.07
Nodes (32): action(), AuthService<'_, D, R>, FnOnce, Option, Result, T, replay_guidance(), GameAuditEvent (+24 more)

### Community 32 - "Player Recovery"
Cohesion: 0.11
Nodes (21): GameService<'_, D, R>, keyed(), PrivateRows, Proof, RecoveryOutcome, RecoverySocketClose, CommandId, ConnectionId (+13 more)

### Community 33 - "Users Core"
Cohesion: 0.12
Nodes (36): new_token(), String, command_id(), count(), create(), Fixture, list(), AccountId (+28 more)

### Community 34 - "Service"
Cohesion: 0.17
Nodes (18): AuthError, From, add_deadline(), AuthService<'a, D, R>, report_rehash_failure(), AccountId, D, FnOnce (+10 more)

### Community 35 - "Shared"
Cohesion: 0.08
Nodes (29): Self, Self, discovery_accounts_owner_fences_original_sample_despite_rollback_without_renewal(), discovery_expiry_fence_readback_and_transaction_failure_roll_back(), discovery_expiry_fence_rejects_wrong_token_identity_metadata_and_sample(), discovery_expiry_fence_sqlite_ignore_abort_and_tampered_readback_never_ack(), fence_account_expiry(), Fixture (+21 more)

### Community 36 - "Game Reads"
Cohesion: 0.13
Nodes (30): read_purpose(), ReadBinding, OperatorPurpose, clone_request(), digest(), dispatch(), ExpiryReply, ExpiryRequest (+22 more)

### Community 37 - "Player Recovery"
Cohesion: 0.10
Nodes (35): actual_kdf_gap_answer_replacement_deletion_alias_epoch_and_terminal_changes_fail_closed(), aged_recovery_commands_are_rejected_after_receipt_cleanup_and_idle_denial_is_uniform(), answer_maintenance_kdf_gap_revoked_session_rolls_back_private_update(), close_retry_backoff_is_durable_bounded_and_ack_ignored_write_retains_work(), entropy_failure_unsupported_stored_profile_and_phc_fail_without_epoch_or_failure_accounting(), epoch(), exact_current_alias_and_uniform_absent_wrong_disabled_proof(), FaultReadback (+27 more)

### Community 38 - "Mod"
Cohesion: 0.11
Nodes (23): actor_key(), AuthService<'_, D, R>, deadline(), ManagementError, operation_tag(), AccountId, CommandId, FnOnce (+15 more)

### Community 39 - "Operator Runtime"
Cohesion: 0.15
Nodes (26): GameOwnerResponse, audit_failed_transport(), audit_rejected_transport(), audit_transport(), denial_binding(), fence_edge_operator_expiry(), GameObject, OperatorExpiryDenialReply (+18 more)

### Community 40 - "Scenario"
Cohesion: 0.16
Nodes (16): Command, Origin-bound actors and exact-payload command retries., The UUID and serialized body are immutable across every explicit retry., call_values(), history_expiry(), Independent test-only single-line bingo model. No production imports., DO-071: three UTC calendar months, preserving milliseconds and clamping day., identifier() (+8 more)

### Community 41 - "Game Directory"
Cohesion: 0.12
Nodes (26): checked_revision(), directory_claim_ack_and_code_use_real_owner_core_and_exact_binding(), directory_publication_recovery_and_release_use_real_core(), DirectoryGameOutcome, DirectoryGameRejection, DirectoryGameReply, DirectoryGameRequest, dispatch() (+18 more)

### Community 42 - "Games"
Cohesion: 0.12
Nodes (31): call_sequence(), call_value(), canonical_alias(), cell_kind(), cells(), completed_lines(), configuration(), CreateGame (+23 more)

### Community 43 - "Directory Host Transfer"
Cohesion: 0.10
Nodes (22): DirectoryService<'_, D, R>, parse_directory_transfer(), require_shape(), AccountId, GameId, Option, Result, Row (+14 more)

### Community 44 - "Shared"
Cohesion: 0.11
Nodes (26): AuthService<'_, D, R>, command_id(), CommandOutcome, fingerprint(), AccountId, CommandId, LinkId, Option (+18 more)

### Community 45 - "Decode"
Cohesion: 0.12
Nodes (29): all_seven_operations_accept_their_exact_payload(), assert_array_root_rejected(), CompleteBody, cookie_is_read_and_duplicate_session_values_reject(), decode(), Decoded, enrollment_complete_rejects_positional_json_array_root(), enrollment_redeem_rejects_positional_json_array_root() (+21 more)

### Community 46 - "Shared"
Cohesion: 0.07
Nodes (21): AuthCommand, Debug, Drop, Formatter, Option, Result, String, AccountId (+13 more)

### Community 47 - "Reads"
Cohesion: 0.09
Nodes (24): GameDetail, MembershipRead, BoardsRead, bounded_ids(), CallsRead, dedicated_read_qualification_accepts_canonical_subset_and_maximum_lines(), dedicated_read_qualification_rejects_duplicate_unordered_and_out_of_bounds_lines(), MembershipAccess (+16 more)

### Community 48 - "Game Authority"
Cohesion: 0.24
Nodes (33): bearer(), clock_rollback_and_unavailable_storage_never_release_authority(), close_pages_are_bounded_stably_ordered_and_do_not_discard_backlog(), close_retry_is_durable_capped_and_saturates_count_and_deadline(), connection(), context(), current_normal_host_and_admin_produce_private_game_authority(), due_close_work_is_bounded_and_preserves_exact_revoked_binding() (+25 more)

### Community 49 - "Mod"
Cohesion: 0.11
Nodes (31): assert_issued_handoff(), audit_reads_preserve_required_targets_and_read_operations(), authorization_debug_is_redacted_and_server_errors_discard_bodies(), client_rejects_noncanonical_or_non_https_origins_before_network(), command_runner_exercises_real_https_to_safe_metadata(), game_audit_cli_real_tls_has_exact_target_no_browser_headers_and_checks_closed_response(), http_status_and_cookie_policy_are_fail_closed(), issued_fragments_must_be_canonical_32_byte_base64url() (+23 more)

### Community 50 - "Users"
Cohesion: 0.10
Nodes (28): account_fields_reject_positional_sequences(), assert_invalid_response(), CreateUsername, deserialize_object(), deserialize_users(), every_public_variant_round_trips_with_safe_debug_and_http_status(), nested_accounts_preserve_duplicate_and_unknown_field_rejection(), nested_objects_preserve_exact_wire_bytes() (+20 more)

### Community 51 - "Test Full Game E2E"
Cohesion: 0.12
Nodes (8): Stream, Transport, FakeSocket, frame(), Offline unit tests for the remote runner; these are not remote run evidence., receiver_fixture(), snapshot_fixture(), StreamTests

### Community 52 - "Game Resources"
Cohesion: 0.14
Nodes (21): bytes(), cookie(), finite_metrics(), Fixture, id(), maximum_value_cell_game_resources(), MeasuredDb, AccountId (+13 more)

### Community 53 - "Users"
Cohesion: 0.11
Nodes (23): AccountsObject, deserialize_error_fields(), dispatch(), ErrorEnvelope, ErrorFields, handle(), log_delivery(), management_auth_error() (+15 more)

### Community 54 - "Privileged Audit"
Cohesion: 0.14
Nodes (26): domain_rows(), events(), FirstEntropyCollision, FirstEntropyFailure, ignored_capture_with_identical_existing_entropy_collision_cannot_commit_action(), page(), privileged_action_and_capture_entropy_errors_rollback_then_durably_record_failed(), privileged_action_storage_error_rolls_back_and_captures_failed_using_actual_actor() (+18 more)

### Community 55 - "Test Full Game E2E"
Cohesion: 0.14
Nodes (9): Bootstrap, Restricted CLI handoff; credentials only in environment, links promptly unlinked, run_cli(), E2EError, json_object(), Exception, Contains only a fixed diagnostic code, never a submitted value., Target (+1 more)

### Community 56 - "Mod"
Cohesion: 0.10
Nodes (15): TransferPreparation, Vec, same_work_target(), CreationResult, LobbyPreparation, ParticipantOutcome, PendingWork, AccountId (+7 more)

### Community 57 - "Terminal"
Cohesion: 0.20
Nodes (9): Record, AccountId, string(), GameService<'_, D, R>, CommandId, Option, PlayerId, Result (+1 more)

### Community 58 - "Audit Repairs"
Cohesion: 0.13
Nodes (23): audit(), bounded_audit_page_does_not_scan_every_live_row(), Exhaust, explicit_creation_target_transient_identity_query_failure_is_capturable(), invalid_typed_call_winner_cancel_configuration_sync_all_capture_independently(), names(), privileged_stream_acceptance_and_success_are_atomic_and_replay_is_quiet(), random_generation_budget_is_required_failed_audit() (+15 more)

### Community 59 - "Discovery.Test"
Cohesion: 0.12
Nodes (20): directory(), get(), lobby(), newGame(), owner(), player(), rows(), value() (+12 more)

### Community 60 - "Backend"
Cohesion: 0.11
Nodes (19): ConfigError, InvalidValueKind, acquisition_can_only_request_the_declared_inventory(), assert_redacted(), dev_cli_key_is_optional_but_supplied_values_are_validated(), explicit_argon2_costs_are_deserialized_as_numbers(), keys_require_canonical_unpadded_base64url_for_exactly_32_bytes(), Result (+11 more)

### Community 61 - "Shared"
Cohesion: 0.12
Nodes (22): AuthOutputAuthority, AuthService<'_, D, R>, fixture(), output_fence_preserves_original_sample_and_never_renews_or_resurrects(), output_fence_requires_exact_identity_deadline_sample_and_verified_write(), output_release_rechecks_revocation_lifecycle_role_and_safe_projection(), AccountId, Result (+14 more)

### Community 62 - "Test Full Game E2E"
Cohesion: 0.14
Nodes (5): Reply, Checks, CommandTests, EntrypointTests, FakeActor

### Community 63 - "Sql Policy Tags"
Cohesion: 0.16
Nodes (22): assert_verifier_rehashed(), assertion_failure_diagnostics_do_not_disclose_verifiers(), command_id(), context(), cookie(), create(), enrollment_and_reset_bind_status_purpose_and_scope_without_changing_stored_tags(), RecordedSqlite (+14 more)

### Community 64 - "Shared"
Cohesion: 0.12
Nodes (22): AuthService<'_, D, R>, AccountId, CommandId, Result, AuthService<'_, D, R>, fingerprint(), CommandId, Option (+14 more)

### Community 65 - "Tests"
Cohesion: 0.13
Nodes (18): access_link_cleanup_and_alarm_share_the_exact_metadata_retention_deadline(), cleanup_limits_each_delete_and_expired_close_enqueue_to_the_named_batch_size(), entity_id(), Cell, Connection, FnOnce, RefCell, Result (+10 more)

### Community 66 - "Shared"
Cohesion: 0.10
Nodes (20): int(), Database, metadata_clock_bounds_match_the_named_safe_integer_limit(), metadata_statement(), Debug, Formatter, Option, Result (+12 more)

### Community 67 - "Games"
Cohesion: 0.23
Nodes (24): ApiError, GameRejection, answer_receipt(), audit_game_transport_failure(), dispatch(), fence_recovery_output(), handle(), json_response() (+16 more)

### Community 68 - "Removal"
Cohesion: 0.33
Nodes (22): admin_session(), app_admin_self_disable_delete_and_enable_are_forbidden(), command_id(), committed_removal_phase_is_bound_and_persisted_in_real_sqlite(), deletion_does_not_require_an_unused_credential_epoch_increment(), deletion_requires_matching_gate_and_retains_intent_until_release_ack(), developer_cli_cannot_remove_the_last_enabled_verified_admin(), disable_changes_epoch_once_and_repeated_no_op_keeps_timestamp() (+14 more)

### Community 69 - "Removal"
Cohesion: 0.22
Nodes (14): AuthService<'_, D, R>, parse_work(), RemovalGateGrant, RemovalPhase, RemovalReleaseAck, RemovalWork, AccountId, CommandId (+6 more)

### Community 70 - "Player Lifecycle"
Cohesion: 0.21
Nodes (14): player_actor(), fingerprint(), GameService<'_, D, R>, MembershipOutcome, CommandId, ConnectionId, GameId, Option (+6 more)

### Community 71 - "Operator Peers"
Cohesion: 0.26
Nodes (18): authorize(), complete(), directory_error(), filter_targets(), fresh_challenge(), operator_fresh_challenges_differ_at_identical_clock_and_entropy_failure_denies(), OperatorAccountCut, prepare() (+10 more)

### Community 72 - "Command Lookup.Test"
Cohesion: 0.11
Nodes (17): fixture(), lobby(), owner(), player(), rows(), directoryStub(), initializeDirectory(), commandId() (+9 more)

### Community 73 - "Args"
Cohesion: 0.12
Nodes (20): AccountCommand, AuditCommand, AuditPage, canonical_id(), Cli, LinkMutation, Mutation, Page (+12 more)

### Community 74 - "Operator"
Cohesion: 0.22
Nodes (16): alter(), FirstAuditEntropyFailure, Fixture, operator_audit_entropy_failure_rolls_back_each_mutation_and_separately_records_failed(), operator_identity_is_immutable_and_remains_after_prestart_terminal_cleanup(), operator_transfer_aborted_work_and_stale_grants_cannot_commit_after_removal_wins(), operator_transfer_grant_proof_ack_binds_all_immutable_fields_and_never_clears_successor(), operator_transfer_ignored_assignment_or_phase_write_rolls_back_receipt_and_assignment() (+8 more)

### Community 75 - "Directory Games"
Cohesion: 0.12
Nodes (20): ByteRuntime, coordination_snapshot(), history_expiry_cleanup_sqlite_faults_rollback_index_retirement_and_clock(), legacy_directory(), legacy_host_index_is_preserved_without_inventing_projection_or_claiming_another_slot(), migration_installs_minimal_single_reservation(), migration_reopens_old_schema_preserving_gate_pending_and_floor(), migration_unknown_versions_and_unexpected_application_tables_fail_closed() (+12 more)

### Community 76 - "History"
Cohesion: 0.12
Nodes (18): durable_command_floor_survives_reopen_and_rejects_clock_rollback(), independent_connections_serialize_competing_hosts_to_one_reserved_game(), open_sqlite(), PathBuf, scratch_db(), fresh_history_login(), history_at_last_retained_millisecond(), history_final_response_expiry_fence_faults_roll_back_all_rows() (+10 more)

### Community 77 - "Resolution"
Cohesion: 0.18
Nodes (18): add_three_calendar_months(), civil_from_days(), days_from_civil(), days_in_month(), GameService<'_, D, R>, AccountId, CommandId, GameId (+10 more)

### Community 78 - "Read Routes.Test"
Cohesion: 0.16
Nodes (22): directory(), get(), intercept(), owner(), restore(), setup(), cookie(), call() (+14 more)

### Community 79 - "Discovery"
Cohesion: 0.16
Nodes (18): availability_selects_exact_context_without_credential_fallback(), AvailabilityView, decode_discovery(), DiscoveryCommand, DiscoveryRequest, no_query(), query_fields(), read() (+10 more)

### Community 80 - "Game Reads"
Cohesion: 0.17
Nodes (17): decode(), decode_game_read(), game_read_route(), GameReadRequest, read_rejection_context(), ReadRole, ReadSelection, Drop (+9 more)

### Community 81 - "Mod"
Cohesion: 0.16
Nodes (11): AuthOutcome, CookieEffect, RequestContext, Debug, Formatter, Option, Result, String (+3 more)

### Community 82 - "Mod"
Cohesion: 0.28
Nodes (21): capture_owner_failure(), current_output(), current_targeted(), deny_observed_expiry(), DiscoveryError, dispatch(), failure(), fence_expired_account() (+13 more)

### Community 83 - "Test Full Game E2E"
Cohesion: 0.13
Nodes (5): Actor, CookiePolicy, NoRedirect, ActorTests, Response

### Community 84 - "Reads"
Cohesion: 0.23
Nodes (21): account(), dedicated_reads_admin_purposes_success_rejection_failure_atomic_capture(), dedicated_reads_awaiting_own_membership_board_recovery_and_alias_are_current(), dedicated_reads_complete_numeric_pool_calls_qualification_and_exhaustion(), dedicated_reads_full_thousand_actual_accepted_calls_remain_in_progress_and_exhausted(), dedicated_reads_history_purge_denies_deleted_original_player_grant(), dedicated_reads_maximum_legal_pool_roster_boards_and_policy_aliases_are_bounded(), dedicated_reads_new_cross_host_are_passive_and_sync_still_enters() (+13 more)

### Community 85 - "Local Readiness"
Cohesion: 0.19
Nodes (16): fresh_command(), history_measurements(), local_slice9_measurements(), MeasuredDb, OsRuntime, recovery_owner(), Cell, CommandId (+8 more)

### Community 86 - "Shared"
Cohesion: 0.18
Nodes (13): ManagementPrincipal, Debug, Drop, Formatter, Zeroize, AuthService, D, R (+5 more)

### Community 87 - "Users"
Cohesion: 0.16
Nodes (17): AuthService<'_, D, R>, changes(), final_release_expiry_fence_is_atomic_and_blocks_clock_rollback(), final_release_is_read_only_on_success_and_fences_authority_denials(), fixture(), id(), AccountId, CommandId (+9 more)

### Community 88 - "Reads"
Cohesion: 0.26
Nodes (13): dedicated_release_identity_epoch_role_assignment_and_deleted_record_deny(), dedicated_release_ignored_fence_is_unavailable_and_rolls_back(), dedicated_release_original_cap_never_extends_after_resume_and_equality_fences_before_entropy(), dedicated_release_payload_changed_at_same_identity_is_withheld(), GameService<'_, D, R>, ReadCut, ConnectionId, Option (+5 more)

### Community 89 - "Mod"
Cohesion: 0.22
Nodes (12): AccountsObject, decode_private_json(), fetch(), DurableObject, Env, Option, Result, Self (+4 more)

### Community 90 - "Operator Peers"
Cohesion: 0.17
Nodes (20): OperatorAccountsReply, OperatorAccountsRequest, OperatorChallenge, OperatorDirectoryReply, OperatorDirectoryRequest, AccountId, CommandId, Deserialize (+12 more)

### Community 91 - "Command Lookup"
Cohesion: 0.14
Nodes (16): command_lookup_creation_lobby_own_actor_unknown_cross_actor_and_no_effects(), command_lookup_current_player_admission_alias_recovery_switch_and_reuse(), command_lookup_directory_creation_exact_original_account_only_and_passive(), command_lookup_future_floor_exact_expiry_rollback_and_ignored_clock(), command_lookup_old_required_work_remains_pending_but_committed_receipt_expires(), command_lookup_pending_prepared_and_committed_follow_up_are_not_peer_completion(), command_lookup_start_call_terminal_original_grants_and_exit(), committed() (+8 more)

### Community 92 - "Receipt Atomicity"
Cohesion: 0.16
Nodes (18): account_exit_receipt_scalar_faults_roll_back_then_original_command_retries(), lobby_cancel_receipt_scalar_faults_roll_back_then_original_command_retries(), manual_call_receipt_scalar_faults_roll_back_then_original_command_retries(), new_cancel_receipt_scalar_faults_roll_back_then_original_command_retries(), Operation, player_exit_receipt_scalar_faults_roll_back_then_original_command_retries(), Prepared, random_call_receipt_scalar_faults_roll_back_then_original_command_retries() (+10 more)

### Community 93 - "Player Lifecycle Boundaries.Test"
Cohesion: 0.13
Nodes (15): nativeRegistry(), fixture(), join(), leave(), owner(), recover(), snapshot(), started() (+7 more)

### Community 94 - "Test Support"
Cohesion: 0.14
Nodes (12): Cell, Connection, FnOnce, RefCell, Result, Row, Self, T (+4 more)

### Community 95 - "Wire"
Cohesion: 0.19
Nodes (19): discovery_expiry_ack_is_bound_and_cannot_replace_a_read_projection(), discovery_private_original_bytes_reject_unknown_duplicate_positional_and_unsafe_fields(), discovery_reply_identity_lifecycle_admission_and_deadlines_are_bound(), DiscoveryAction, DiscoveryOutcome, DiscoveryOwnerReply, DiscoveryOwnerRequest, DiscoveryReadPhase (+11 more)

### Community 96 - "Management"
Cohesion: 0.17
Nodes (19): call_owner(), dispatch(), GameAuditErrorFields, GameAuditErrorReply, GameAuditExpiryFence, GameAuditTransportContext, GameAuditTransportPurpose, GameAuditTransportRejection (+11 more)

### Community 97 - "Player Lifecycle Review"
Cohesion: 0.18
Nodes (15): connect(), durable_snapshot(), ignored_or_aborted_explicit_leave_timestamp_rolls_back_detachment_and_receipt(), lobby(), ReadbackFault, required_readback_faults_roll_back_every_lifecycle_effect_and_clock_metadata(), Cell, Fixture (+7 more)

### Community 98 - "Mod"
Cohesion: 0.14
Nodes (12): Cell, Connection, FnOnce, RefCell, Result, Row, Self, T (+4 more)

### Community 99 - "Operator.Test"
Cohesion: 0.13
Nodes (12): enroll(), admin(), fixture(), owner(), patch(), rows(), transfer(), owner() (+4 more)

### Community 100 - "History"
Cohesion: 0.22
Nodes (15): decode_history(), filter_tag(), HistoryCommand, HistoryCursor, HistoryQuery, HistoryRequest, list_query(), Drop (+7 more)

### Community 101 - "Shared"
Cohesion: 0.13
Nodes (17): ReadIdentity, AccountId, PlayerId, SessionId, SpectatorId, creation_pending(), CreationLookupReply, CreationLookupRequest (+9 more)

### Community 102 - "Test Full Game E2E"
Cohesion: 0.13
Nodes (3): BoardOracle, board_fixture(), OracleTests

### Community 103 - "Shared"
Cohesion: 0.18
Nodes (16): execute(), prepare(), Prepared, CommandId, Option, PathBuf, Result, Write (+8 more)

### Community 104 - "Frontend"
Cohesion: 0.22
Nodes (16): IntoIterator, absent_public_settings_use_serde_defaults(), assert_redacted(), default_api_path(), explicit_public_values_are_preserved(), get_frontend_config(), maximum_display_name_is_measured_in_utf8_bytes(), PublicAppConfig (+8 more)

### Community 105 - "Host Recipient"
Cohesion: 0.22
Nodes (8): AuthService<'_, D, R>, HostRecipientAuthority, AccountId, HostTransferTarget, Option, Result, Self, Vec

### Community 106 - "Terminal"
Cohesion: 0.18
Nodes (16): all_rows(), count(), end_without_winner(), explicit_prestart_cancellation_is_atomic_replayable_and_has_no_history(), final_exit_removes_all_player_or_account_grants_but_never_changes_history(), gameplay_response_guard_is_read_only_and_accepts_terminal_without_new_view_grant(), history_expiry_denies_immediately_then_purges_with_recoverable_directory_release(), malformed_history_cannot_be_erased_without_validating_its_self_contained_snapshot() (+8 more)

### Community 107 - "Link File"
Cohesion: 0.22
Nodes (12): failure_cleanup_truncates_only_owned_handle_not_replaced_path(), LinkFile, persists_link_without_displaying_or_reopening_path(), replaced_path_is_rejected_before_secret_write(), reserves_new_private_file_before_network(), Drop, PathBuf, Result (+4 more)

### Community 108 - "Game"
Cohesion: 0.28
Nodes (15): account_acceptance_preflight_rejects_fenced_or_replaced_preparations_without_accepting(), add_three_calendar_months(), authority(), close_work(), command_fingerprint(), id(), initialized(), join_fingerprint() (+7 more)

### Community 109 - "Wire"
Cohesion: 0.22
Nodes (13): auth_body(), CookieWire, OwnerRequest, OwnerResponse, D, Deserialize, Error, Option (+5 more)

### Community 110 - "Game Object.Test"
Cohesion: 0.13
Nodes (9): closed(), holdConnectionProofs(), join(), messages(), newGame(), openLobby(), releaseConnectionProofs(), stream() (+1 more)

### Community 111 - "Output"
Cohesion: 0.24
Nodes (14): CliError, human_and_json_account_output_escape_all_controls(), issued_output_projects_metadata_without_link_secrets(), CommandId, Result, String, Value, Write (+6 more)

### Community 112 - "Reads"
Cohesion: 0.18
Nodes (11): AuthService<'_, D, R>, page_size(), AccountId, AuditId, Option, Result, safe(), id_check() (+3 more)

### Community 113 - "Discovery"
Cohesion: 0.23
Nodes (15): all_rows(), discovery_authoritative_state_advances_without_directory_revision_equality(), discovery_availability_counts_valid_observed_connections_without_reconciliation(), discovery_fence_storage_fault_is_unavailable_and_rolls_back(), discovery_idle_denial_is_durable_write_free_recut_and_rollback_fenced(), discovery_player_deadline_denial_survives_clock_rollback_without_other_writes(), discovery_read_only_cuts_hide_allocations_and_do_not_create_grants_or_timers(), discovery_remote_expiry_fences_the_original_sample_without_other_writes() (+7 more)

### Community 114 - "Shared"
Cohesion: 0.19
Nodes (10): BTreeMap, NativeSource, Into, Mutex, Option, Result, Self, String (+2 more)

### Community 115 - "Mod"
Cohesion: 0.21
Nodes (12): AuthPolicy, Default, Self, bounded_phc(), hash_password(), Result, validate_policy(), verify_password() (+4 more)

### Community 116 - "Protected Response Snapshot.Test"
Cohesion: 0.22
Nodes (13): actualStart(), cleanup(), denied(), directory(), fixture(), get(), holdFinal(), mutateContent() (+5 more)

### Community 117 - "Spectators.Test"
Cohesion: 0.26
Nodes (14): assertPurged(), clock(), expectClearedSpectator(), fixture(), get(), observeTerminalSend(), owner(), player() (+6 more)

### Community 118 - "Shared"
Cohesion: 0.14
Nodes (12): GameDirectoryObject, Result, Request, Drop, Option, String, Vec, Zeroizing (+4 more)

### Community 119 - "Game Resource Probe"
Cohesion: 0.21
Nodes (10): private_output(), main(), Parser, main(), Namespace, Run local maximum-configured-game observations, never a provider/forecast capaci, run(), summaries() (+2 more)

### Community 120 - "Write Regressions"
Cohesion: 0.21
Nodes (11): additional_terminal_required_writes_ignore_abort_rollback_and_exact_retry(), all_call_write_faults_rollback_then_original_command_retries(), host_revision(), random_call_unbiased_rejection_chooses_remaining_index_and_replay_uses_no_entropy(), Cell, ConnectionId, Fixture, Result (+3 more)

### Community 121 - "Developer"
Cohesion: 0.24
Nodes (8): authenticate_cli(), CliRequest, decode_cli(), CommandId, HeaderMap, Method, Option, Result

### Community 122 - "Command Lookup"
Cohesion: 0.31
Nodes (8): command_stamp(), GameService<'_, D, R>, CommandId, GameOutcome, Option, Result, validate_outcome_actor(), CommandResultStatus

### Community 123 - "Discovery"
Cohesion: 0.41
Nodes (7): DiscoveryCut, DiscoveryValue, DiscoveryViewer, GameService<'_, D, R>, GameId, Option, Result

### Community 124 - "Terminal"
Cohesion: 0.27
Nodes (6): GameService<'_, D, R>, Result, SpectatorId, String, Vec, SpectatorTerminalNotice

### Community 125 - "Dedicated Reads Https Wss"
Cohesion: 0.32
Nodes (6): call(), check(), provision(), Local dedicated-read HTTPS/WSS checks; build the Worker and CLI before running., read(), SocketClient

### Community 126 - "Game Audit Https Wss"
Cohesion: 0.31
Nodes (6): call(), check(), developer_read(), provision(), Slice 8 real local HTTPS developer CLI and privileged Game auditing checks., SocketClient

### Community 127 - "Game Resources.Test"
Cohesion: 0.21
Nodes (8): ack(), jsonOK(), limits, owner(), pause(), settled(), stream(), sync()

### Community 128 - "Directory Schema"
Cohesion: 0.44
Nodes (11): add_game_coordination(), add_host_transfer_coordination(), game_coordination_statements(), host_transfer_statements(), initialize_removal(), install(), removal_statements(), D (+3 more)

### Community 129 - "Directory"
Cohesion: 0.36
Nodes (6): GameDirectoryObject, DurableObject, Env, Result, Self, State

### Community 130 - "Owner"
Cohesion: 0.24
Nodes (9): GameObject, peer_error(), route(), Cell, Env, GamePeerError, Option, Result (+1 more)

### Community 131 - "Gameplay Https Wss"
Cohesion: 0.32
Nodes (5): call(), check(), join(), Local gameplay HTTPS/WSS checks; build the Worker and CLI before running., SocketClient

### Community 132 - "Game Storage Audit"
Cohesion: 0.23
Nodes (8): account_session(), current_connection_capacity_preserves_existing_bindings_and_reclaims_expired_slot(), grant_expiry_reclaims_capacity_and_account_reconnect_reuses_preparation_slot(), Fixture, Option, SessionId, String, shared_grant_capacity_preserves_accepted_viewers_and_allows_both_terminal_outcomes()

### Community 133 - "Start Publication"
Cohesion: 0.41
Nodes (11): accepted_calls_reconcile_pending_start_and_reject_stale_real_acknowledgements(), directory_rows(), host_revision(), pending_row(), pending_start(), pending_start_fence_required_write_faults_roll_back_calls_and_exact_retries(), ConnectionId, Fixture (+3 more)

### Community 134 - "Operator Https Wss"
Cohesion: 0.33
Nodes (5): call(), check(), provision(), Local gameplay HTTPS/WSS checks; build the Worker and CLI before running., SocketClient

### Community 135 - "Backend"
Cohesion: 0.23
Nodes (7): BackendConfig, get_backend_config(), Option, Result, Self, String, EnvSource

### Community 136 - "Auth Schema"
Cohesion: 0.31
Nodes (10): auth_schema_ddl_matches_the_pre_refactor_snapshot(), auth_schema_statements(), connection(), String, Vec, sqlite_account_caps_match_verifier_and_safe_integer_limits(), sqlite_rate_buckets_match_failure_count_and_digest_limits(), sqlite_receipts_match_payload_digest_and_retention_limits() (+2 more)

### Community 137 - "Edge"
Cohesion: 0.35
Nodes (10): command(), dispatch(), failure(), handle(), json_response(), Env, Result, SendWrapper (+2 more)

### Community 138 - "Management"
Cohesion: 0.38
Nodes (5): AccountsObject, management_error(), management_result(), Result, super::game_object::GameObject

### Community 139 - "Player Lifecycle.Test"
Cohesion: 0.27
Nodes (10): counts(), fixture(), join(), leave(), owner(), rename(), restore(), start() (+2 more)

### Community 140 - "Discovery"
Cohesion: 0.31
Nodes (10): CurrentGame, CurrentGames, CurrentGameSummary, GameEntry, GameSlot, AccountId, GameId, Option (+2 more)

### Community 141 - "Commands"
Cohesion: 0.24
Nodes (7): command_lookup_all_closed_receipt_variants_and_total_envelope_bounds(), CommandResult, PendingCommandOperation, PendingCommandState, Result, Self, Vec

### Community 142 - "Deployment Identity"
Cohesion: 0.29
Nodes (9): CloudflareVersion, DeploymentIdentity, handle(), identity(), Env, Result, SendWrapper, State (+1 more)

### Community 144 - "Terminal Regressions"
Cohesion: 0.33
Nodes (9): calendar_history_deadline_is_the_same_persisted_game_and_snapshot_value(), expired_call_receipts_after_cleanup_never_readmit_old_commands_or_clock_rollback(), host_revision(), logged_in(), ConnectionId, Fixture, String, terminal_grants_receipt_purge_and_release_route_survive_actual_sqlite_reopen() (+1 more)

### Community 145 - "Shared"
Cohesion: 0.24
Nodes (7): ManagementClient, Prepared, Result, Self, String, Client, HeaderValue

### Community 146 - "Shared"
Cohesion: 0.24
Nodes (7): D, Deserialize, Error, Result, Self, Zeroizing, SecretKey

### Community 147 - "Terminal Faults"
Cohesion: 0.50
Nodes (8): call_once(), clear_fault(), every_required_terminal_write_has_real_sqlite_ignored_and_aborted_rollback(), fault(), final_exit_ignored_and_aborted_writes_roll_back_all_access_and_history(), history_purge_faults_roll_back_every_child_route_release_intent_and_clock(), ConnectionId, Fixture

### Community 148 - "History Api"
Cohesion: 0.25
Nodes (5): history_queries_are_canonical_bounded_unique_and_filter_bound(), read_history(), HeaderMap, Option, Result

### Community 149 - "Game Audit Repairs.Test"
Cohesion: 0.25
Nodes (4): cancel(), fixture(), owner(), rows()

### Community 150 - "Player Recovery Boundaries.Test"
Cohesion: 0.25
Nodes (4): fixture(), owner(), recover(), rows()

### Community 151 - "Mod"
Cohesion: 0.39
Nodes (6): Bindings, get_backend_config(), Env, Option, Result, String

### Community 152 - "Sync Output"
Cohesion: 0.39
Nodes (5): GameService<'_, D, R>, ConnectionId, Option, Result, SyncResponse

### Community 153 - "Recovery Answers"
Cohesion: 0.50
Nodes (6): enroll(), normalize(), push(), Result, String, Zeroizing

### Community 154 - "Privileged Game Audit.Test"
Cohesion: 0.32
Nodes (5): actor(), adminRequest(), fixture(), owner(), rows()

### Community 155 - "Connection Capacity"
Cohesion: 0.38
Nodes (4): GameService<'_, D, R>, Option, Result, SessionId

### Community 156 - "Game Schema"
Cohesion: 0.62
Nodes (6): game_record_statement(), recovery_coordination_statements(), String, Vec, spectator_statements(), statements()

### Community 157 - "Previous Command Game Schema"
Cohesion: 0.62
Nodes (6): game_record_statement(), recovery_coordination_statements(), String, Vec, spectator_statements(), statements()

### Community 158 - "Previous Read Game Schema"
Cohesion: 0.62
Nodes (6): game_record_statement(), recovery_coordination_statements(), String, Vec, spectator_statements(), statements()

### Community 159 - "Previous Saved Game Schema"
Cohesion: 0.62
Nodes (6): game_record_statement(), recovery_coordination_statements(), String, Vec, spectator_statements(), statements()

### Community 160 - "Directory"
Cohesion: 0.43
Nodes (6): DirectoryRejection, DirectoryRequest, DirectoryResponse, AccountId, OperationId, Body

### Community 161 - "Shared"
Cohesion: 0.29
Nodes (6): InspectHostedRead, Cell, FnOnce, Result, Sqlite, T

### Community 162 - "Storage Bounds.Test"
Cohesion: 0.43
Nodes (6): cancelAndReadHistory(), connect(), owner(), seedCalls(), startedGame(), sync()

### Community 163 - "Mod"
Cohesion: 0.33
Nodes (5): BrokenPipe, ca_file_read_is_bounded_and_errors_are_redacted(), existing_output_target_prevents_any_http_mutation(), Result, Write

### Community 164 - "Backend Once Error"
Cohesion: 0.33
Nodes (5): failed_get_caches_a_safe_error_without_retrying(), NeverRead, Option, Result, String

### Community 165 - "Backend Once Success"
Cohesion: 0.33
Nodes (5): get_initializes_lazily_and_never_acquires_again(), NeverRead, Option, Result, String

### Community 166 - "Typed Struct"
Cohesion: 0.33
Nodes (5): constructor_acquires_every_declared_key_once(), Box, Error, Result, serde_deserializes_the_backend_struct_with_optional_defaults()

### Community 168 - "Links"
Cohesion: 0.53
Nodes (4): AuthService<'_, D, R>, AccountId, CommandId, Result

### Community 169 - "Host Transfer"
Cohesion: 0.33
Nodes (3): Formatter, Result, Self

### Community 170 - "Discovery Boundaries.Test"
Cohesion: 0.53
Nodes (5): directory(), get(), lobby(), owner(), unchangedDomain()

### Community 171 - "Player Recovery Review Regressions.Test"
Cohesion: 0.40
Nodes (4): fixture(), owner(), recover(), restore()

### Community 172 - "Frontend"
Cohesion: 0.40
Nodes (5): constructor_is_zero_argument_and_uses_only_public_build_inputs(), Box, Error, Result, serde_deserializes_the_public_struct_with_optional_defaults()

### Community 173 - "Maintenance"
Cohesion: 0.50
Nodes (3): AuthService<'_, D, R>, Option, Result

### Community 174 - "Mod"
Cohesion: 0.60
Nodes (4): RemovalGrant, RemovalReleaseAck, AccountId, OperationId

### Community 176 - "Protocol"
Cohesion: 0.70
Nodes (4): Prepared, Result, validate_receipt(), validate_response()

### Community 177 - "Mod"
Cohesion: 0.70
Nodes (4): enrollment_url(), link_token(), String, Zeroizing

### Community 178 - "Mod"
Cohesion: 0.70
Nodes (4): deserialize_error(), missing_values_only_identify_known_keys(), Error, third_party_deserialization_errors_are_discarded()

### Community 180 - "Game Operator Schema"
Cohesion: 0.50
Nodes (3): String, Vec, statements()

### Community 181 - "Previous Command Operator Schema"
Cohesion: 0.50
Nodes (3): String, Vec, statements()

### Community 182 - "Previous Read Operator Schema"
Cohesion: 0.50
Nodes (3): String, Vec, statements()

### Community 183 - "Previous Saved Operator Schema"
Cohesion: 0.50
Nodes (3): String, Vec, statements()

### Community 184 - "Operator Peers"
Cohesion: 1.00
Nodes (3): decode_challenge(), D, Error

### Community 185 - "Accounts"
Cohesion: 0.50
Nodes (3): prepare(), Prepared, Result

### Community 186 - "Audit"
Cohesion: 0.50
Nodes (3): prepare(), Prepared, Result

### Community 188 - "Origin"
Cohesion: 0.50
Nodes (3): Result, String, validate_origin()

## Knowledge Gaps
- **17 isolated node(s):** `AuthService<'_, D, R>`, `AccountsObject`, `GameObject`, `super::directory::GameDirectoryObject`, `GameDirectoryObject` (+12 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **7 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `migrate_directory()` connect `Directory` to `Directory`?**
  _High betweenness centrality (0.000) - this node is a cross-community bridge._
- **Why does `GameProjection` connect `Games` to `Wire`?**
  _High betweenness centrality (0.000) - this node is a cross-community bridge._
- **Why does `GameCode` connect `Games` to `Games`?**
  _High betweenness centrality (0.000) - this node is a cross-community bridge._
- **What connects `AuthService<'_, D, R>`, `AccountsObject`, `GameObject` to the rest of the system?**
  _17 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Game Accounts` be split into smaller, more focused modules?**
  _Cohesion score 0.05298390542657741 - nodes in this community are weakly interconnected._
- **Should `Game` be split into smaller, more focused modules?**
  _Cohesion score 0.07736516357206012 - nodes in this community are weakly interconnected._
- **Should `Games Core` be split into smaller, more focused modules?**
  _Cohesion score 0.06247836621668398 - nodes in this community are weakly interconnected._
## Structural extraction limitations

AST health reports 1094 dangling-endpoint edges and 541 collapsed directed same-endpoint edges. These are navigation limitations, not runtime validation. The separate documentation graph remains stale.
