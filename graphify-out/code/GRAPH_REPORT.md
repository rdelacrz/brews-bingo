# Scope: code-only structural graph

Rust/JavaScript/Python source and tests only. The older documentation graph remains stale. No LLM token cost.

AST navigation limitations: {"dangling_endpoint_edges": 627, "directed_same_endpoint_collapsed_edges": 334}. Raw extraction retains edge evidence; this graph is not correctness proof.

# Graph Report - /home/orangepi/Workspace/brews-bingo  (2026-10-11)

## Corpus Check
- 179 files · ~172,933 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 3579 nodes · 9857 edges · 266 communities (129 shown, 137 thin omitted)
- Extraction: 97% EXTRACTED · 3% INFERRED · 0% AMBIGUOUS · INFERRED: 295 edges (avg confidence: 0.8)
- Token cost: 0 input · 0 output

## Community Hubs (Navigation)
- Durable SQLite Storage
- Auth Test Coverage
- Worker HTTP Boundary
- Auth Test Coverage
- Auth Test Coverage
- Worker HTTP Boundary
- Worker HTTP Boundary
- Shared Account Domain
- Auth Test Coverage
- Backend Workspace Structure
- Worker HTTP Boundary
- Durable SQLite Storage
- Durable SQLite Storage
- Auth Test Coverage
- Backend Workspace Structure
- Backend Workspace Structure
- Auth Test Coverage
- Durable SQLite Storage
- Backend Workspace Structure
- Worker HTTP Boundary
- Worker HTTP Boundary
- Worker HTTP Boundary
- Discovery APIs
- Backend Workspace Structure
- Account Authentication Authority
- Backend Workspace Structure
- Auth Test Coverage
- Backend Workspace Structure
- Worker HTTP Boundary
- Worker HTTP Boundary
- Account Authentication Authority
- Auth Test Coverage
- Auth Test Coverage
- Worker HTTP Boundary
- Account Authentication Authority
- Auth Test Coverage
- Backend Workspace Structure
- Account Authentication Authority
- Account Authentication Authority
- Account Authentication Authority
- Durable SQLite Storage
- Auth Test Coverage
- Account Authentication Authority
- Backend Workspace Structure
- Account Authentication Authority
- Auth Test Coverage
- Backend Workspace Structure
- Backend Workspace Structure
- Worker HTTP Boundary
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Auth Test Coverage
- Backend Workspace Structure
- Account Authentication Authority
- Backend Workspace Structure
- Account Authentication Authority
- Worker HTTP Boundary
- Account Authentication Authority
- Backend Workspace Structure
- Worker HTTP Boundary
- Backend Workspace Structure
- Auth Test Coverage
- Backend Workspace Structure
- Backend Workspace Structure
- Typed Environment Configuration
- Worker HTTP Boundary
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Account Authentication Authority
- Discovery APIs
- Worker HTTP Boundary
- Discovery APIs
- Worker HTTP Boundary
- Account Authentication Authority
- Account Authentication Authority
- Typed Environment Configuration
- Backend Workspace Structure
- Backend Workspace Structure
- Auth Test Coverage
- Durable SQLite Storage
- Durable SQLite Storage
- Backend Workspace Structure
- Backend Workspace Structure
- Auth Test Coverage
- Discovery APIs
- Backend Workspace Structure
- Shared Account Domain
- Account Authentication Authority
- Auth Test Coverage
- Auth Test Coverage
- Backend Workspace Structure
- Account Authentication Authority
- Backend Workspace Structure
- Backend Workspace Structure
- Worker HTTP Boundary
- Worker HTTP Boundary
- Discovery APIs
- Auth Test Coverage
- Backend Workspace Structure
- Auth Test Coverage
- Auth Test Coverage
- Auth Test Coverage
- Durable SQLite Storage
- Discovery APIs
- Auth Test Coverage
- Auth Test Coverage
- Auth Test Coverage
- Backend Workspace Structure
- Auth Test Coverage
- Backend Workspace Structure
- Auth Test Coverage
- Backend Workspace Structure
- Backend Workspace Structure
- Typed Environment Configuration
- Backend Workspace Structure
- Durable SQLite Storage
- Discovery APIs
- Backend Workspace Structure
- Auth Test Coverage
- Shared Account Domain
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure

## God Nodes (most connected - your core abstractions)
1. `migrate()` - 93 edges
2. `GameService<'a, D, R>` - 80 edges
3. `migrate_directory()` - 70 edges
4. `ManagementError` - 64 edges
5. `operation()` - 62 edges
6. `AuthError` - 56 edges
7. `account()` - 56 edges
8. `text()` - 51 edges
9. `string()` - 40 edges
10. `ConfigError` - 39 edges

## Surprising Connections (you probably didn't know these)
- `sqlite_username_boundaries_match_domain_validation_on_insert_and_update()` --calls--> `validate_username()`  [INFERRED]
  backend/src/db/schema/auth_schema.rs → shared/domain/src/accounts.rs
- `validate_summary()` --calls--> `validate_alias()`  [INFERRED]
  backend/src/worker_runtime/history_wire.rs → shared/domain/src/games.rs
- `list()` --references--> `ManagementCommand`  [EXTRACTED]
  backend/tests/users_core.rs → shared/contracts/src/management.rs
- `replay_guidance()` --references--> `ReceiptOperation`  [EXTRACTED]
  cli/src/output.rs → shared/contracts/src/management.rs
- `CliRequest` --references--> `ManagementCommand`  [EXTRACTED]
  backend/src/api/developer.rs → shared/contracts/src/management.rs

## Import Cycles
- None detected.

## Communities (266 total, 137 thin omitted)

### Community 0 - "Durable SQLite Storage"
Cohesion: 0.06
Nodes (110): AccountConnectionGrant, AuthPolicy, account_acceptance_preflight_rejects_fenced_or_replaced_preparations_without_accepting(), account_actor(), add_three_calendar_months(), add_time(), Admission, advance() (+102 more)

### Community 1 - "Auth Test Coverage"
Cohesion: 0.06
Nodes (106): metadata_clock_bounds_match_the_named_safe_integer_limit(), metadata_statement(), migrate(), D, StorageError, String, a_current_host_session_is_attributed_when_admin_authority_is_denied(), account_detail_is_safe_and_deleted_or_unknown_target_is_not_found() (+98 more)

### Community 2 - "Worker HTTP Boundary"
Cohesion: 0.07
Nodes (61): AccountCloseRequest, close_checked(), GameObject, peer_error(), private_response(), RegistryAvailability, rejection(), reload() (+53 more)

### Community 3 - "Auth Test Coverage"
Cohesion: 0.08
Nodes (84): accepted_started_terminal_proof_cannot_rewrite_original_expiry_with_newer_revision(), account(), assignment_gate_blocks_newer_projection_to_removing_host(), ByteRuntime, claim_reserves_identity_indexes_host_and_replays_same_command(), clock_advance_during_entropy_rechecks_common_fresh_admission_floor(), code_rejection_sampling_has_bounded_entropy_and_rolls_back_on_exhaustion(), command() (+76 more)

### Community 4 - "Auth Test Coverage"
Cohesion: 0.09
Nodes (76): migrate_directory(), Database, account(), acquire_persists_a_presence_only_gate_with_separate_operation_identity(), acquired_gate_rejects_transfer_without_changing_the_original_host(), competing_operation_returns_busy_without_replacing_the_current_gate(), completed_operation_cannot_reacquire_or_clear_a_newer_gate(), concurrent_operations_on_one_account_serialize_to_one_grant_and_one_busy() (+68 more)

### Community 5 - "Worker HTTP Boundary"
Cohesion: 0.08
Nodes (65): account_reply(), acknowledge_creation(), allocate_game_code(), authorize_account(), authorize_account_connection(), authorize_discovery_account(), bounded_objects(), call() (+57 more)

### Community 6 - "Worker HTTP Boundary"
Cohesion: 0.06
Nodes (68): availability_selects_exact_context_without_credential_fallback(), AvailabilityView, decode_discovery(), DiscoveryCommand, DiscoveryRequest, no_query(), query_fields(), read() (+60 more)

### Community 7 - "Shared Account Domain"
Cohesion: 0.05
Nodes (55): additional_terminal_required_writes_ignore_abort_rollback_and_exact_retry(), all_call_write_faults_rollback_then_original_command_retries(), host_revision(), random_call_unbiased_rejection_chooses_remaining_index_and_replay_uses_no_entropy(), Cell, ConnectionId, Fixture, Result (+47 more)

### Community 8 - "Auth Test Coverage"
Cohesion: 0.08
Nodes (49): directoryStub(), initializeDirectory(), commandId(), cookie(), current(), enroll(), inOwner(), password() (+41 more)

### Community 9 - "Backend Workspace Structure"
Cohesion: 0.06
Nodes (22): CodeGrant, CreationAck, CreationFingerprint, CreationReadyProof, CreationWork, DiscoveryRoute, GameProjection, ProjectionAck (+14 more)

### Community 10 - "Worker HTTP Boundary"
Cohesion: 0.06
Nodes (56): ApiError, call_success_and_receipt_only_retry_match_exact_manual_request(), exit_success_and_receipt_retry_are_bound_only_to_selected_exit_command(), GameAction, GameCookieWire, GameIngress, GameOwnerResponse, gameplay_private_ingress_preserves_original_bytes_and_rejects_action_shape_mismatch() (+48 more)

### Community 11 - "Durable SQLite Storage"
Cohesion: 0.09
Nodes (49): binding_params(), DeliveryBinding, DeliveryBudget, DeliveryError, DeliveryService, DeliveryService<'a, D, R>, DeliveryService<'_, D, R>, migrate_game_delivery() (+41 more)

### Community 12 - "Durable SQLite Storage"
Cohesion: 0.09
Nodes (34): application_tables(), DirectoryService, DirectoryService<'a, D, R>, operation_timestamp(), parse_completion_row(), parse_operation_row(), AccountId, D (+26 more)

### Community 13 - "Auth Test Coverage"
Cohesion: 0.12
Nodes (55): AuthOutcome, account_socket_grants_require_fresh_accounts_and_do_not_renew_expiry_or_presence(), actual_sqlite_reopen_preserves_lobby_intent_and_lost_publication_ack(), admission_is_verifier_only_short_lived_and_consumed_retries_never_mint_cookies(), answer_kdf_runs_outside_sqlite_write_and_rechecks_real_competing_admission(), command_receipts_use_variant_scoped_account_and_player_actor_namespaces(), cookie(), creation_configuration_set_order_is_canonical_without_a_blob_authority() (+47 more)

### Community 14 - "Backend Workspace Structure"
Cohesion: 0.07
Nodes (39): DiscoveryCut, DiscoveryValue, DiscoveryViewer, GameService<'_, D, R>, CurrentGameSummary, GameAccountAuthority, GameAvailability, GameCode (+31 more)

### Community 15 - "Backend Workspace Structure"
Cohesion: 0.07
Nodes (43): admission_and_join_validate_lookup_alias_and_never_debug_recovery_material(), assert_response_rejected(), created_completion_can_be_later_than_creation_without_moving_idle_deadline(), creation_resolves_defaults_and_overrides_with_effective_center(), game_tags_and_safe_receipt_encoding_round_trip_without_parser_details(), gameplay_command_inputs_are_closed_canonical_and_state_fenced(), hardened_tagged_maps_preserve_original_duplicates_and_closed_fields(), host_projection_includes_disconnected_retained_boards_without_sessions() (+35 more)

### Community 16 - "Auth Test Coverage"
Cohesion: 0.06
Nodes (46): fresh_history_login(), history_at_last_retained_millisecond(), history_final_response_expiry_fence_faults_roll_back_all_rows(), history_final_response_expiry_fence_survives_rollback_and_file_reopen(), history_final_response_expiry_read_error_rolls_back_required_write(), immutable_history_survives_actual_file_reopen_after_final_grants_exit(), Fixture, GameId (+38 more)

### Community 17 - "Durable SQLite Storage"
Cohesion: 0.17
Nodes (23): command_timestamp(), deadline(), DirectoryService<'_, D, R>, integer(), optional_integer(), parse_creation(), parse_index(), parse_optional_integer() (+15 more)

### Community 18 - "Backend Workspace Structure"
Cohesion: 0.08
Nodes (36): id_check(), management_schema_statements(), String, Vec, DirectoryRejection, DirectoryRequest, DirectoryResponse, GameDirectoryObject (+28 more)

### Community 19 - "Worker HTTP Boundary"
Cohesion: 0.11
Nodes (43): absent_after_deadline_keeps_reservation_and_saturating_retry(), command(), Fixture, genuine_committed_lobby_recovers_directory_publication_without_faking_game_ack(), genuine_initializer_evidence_acknowledges_only_current_unready_creation(), genuine_new_cancellation_releases_only_the_hidden_directory_association(), genuine_owner_terminal_evidence_releases_reservation_after_deadline(), hidden_code_release_preserves_exact_terminal_identity_and_publication_fences() (+35 more)

### Community 20 - "Worker HTTP Boundary"
Cohesion: 0.09
Nodes (40): account_get_preserves_the_canonical_typed_target(), action_routes_reject_other_methods_without_decoding_payloads(), bodyless_mutation_routes_map_to_their_closed_management_commands(), bodyless_mutations(), collection_get_maps_to_default_list_without_claiming_authority(), collection_query(), collection_query_byte_bound_precedes_parsing(), collection_query_maps_only_cursor_and_bounded_limit_in_either_order() (+32 more)

### Community 21 - "Worker HTTP Boundary"
Cohesion: 0.10
Nodes (34): checked_revision(), directory_claim_ack_and_code_use_real_owner_core_and_exact_binding(), directory_publication_recovery_and_release_use_real_core(), DirectoryGameOutcome, DirectoryGameRejection, DirectoryGameReply, DirectoryGameRequest, dispatch() (+26 more)

### Community 22 - "Discovery APIs"
Cohesion: 0.08
Nodes (37): GameObject, peer_error(), route(), DiscoveryRoute, Env, GameAccountAuthority, GameError, GamePeerError (+29 more)

### Community 23 - "Backend Workspace Structure"
Cohesion: 0.08
Nodes (34): AccountId, String, SafeAccount, SessionKind, SessionView, account_fields_reject_positional_sequences(), assert_invalid_response(), CreateUsername (+26 more)

### Community 24 - "Account Authentication Authority"
Cohesion: 0.18
Nodes (17): AuthError, From, add_deadline(), AuthService<'a, D, R>, report_rehash_failure(), AccountId, D, FnOnce (+9 more)

### Community 25 - "Backend Workspace Structure"
Cohesion: 0.09
Nodes (30): action(), AuditEvent, AuditOperation, AuditOutcome, AuditTarget, deserialize(), json_decode_errors_discard_values_and_parser_sources(), ManagementReceipt (+22 more)

### Community 26 - "Auth Test Coverage"
Cohesion: 0.14
Nodes (33): command_id(), count(), create(), Fixture, list(), AccountId, Cell, CommandId (+25 more)

### Community 27 - "Backend Workspace Structure"
Cohesion: 0.07
Nodes (28): Arc, api_failure(), auth_failure(), Boundary, Capture, Delivery, Failure, init() (+20 more)

### Community 28 - "Worker HTTP Boundary"
Cohesion: 0.17
Nodes (30): account_registration_and_post_gate_recheck_use_real_accounts_core(), AccountAction, AccountConnectionIdentity, AccountReply, AccountRequest, accounts_nested_metadata_requires_maps_and_safe_scalars(), AccountTarget, apply_account_request() (+22 more)

### Community 29 - "Worker HTTP Boundary"
Cohesion: 0.12
Nodes (33): confirm_expiry_denial(), current(), dispatch(), failure(), handle(), HistoryError, output(), GameObject (+25 more)

### Community 30 - "Account Authentication Authority"
Cohesion: 0.09
Nodes (21): AuthService<'_, D, R>, AccountId, CommandId, Result, AuthService<'_, D, R>, Option, Result, ManagementError (+13 more)

### Community 31 - "Auth Test Coverage"
Cohesion: 0.24
Nodes (33): bearer(), clock_rollback_and_unavailable_storage_never_release_authority(), close_pages_are_bounded_stably_ordered_and_do_not_discard_backlog(), close_retry_is_durable_capped_and_saturates_count_and_deadline(), connection(), context(), current_normal_host_and_admin_produce_private_game_authority(), due_close_work_is_bounded_and_preserves_exact_revoked_binding() (+25 more)

### Community 32 - "Auth Test Coverage"
Cohesion: 0.12
Nodes (30): assert_issued_handoff(), audit_reads_preserve_required_targets_and_read_operations(), authorization_debug_is_redacted_and_server_errors_discard_bodies(), client_rejects_noncanonical_or_non_https_origins_before_network(), command_runner_exercises_real_https_to_safe_metadata(), http_status_and_cookie_policy_are_fail_closed(), issued_fragments_must_be_canonical_32_byte_base64url(), issued_link_is_private_and_replay_has_no_url() (+22 more)

### Community 33 - "Worker HTTP Boundary"
Cohesion: 0.13
Nodes (28): all_seven_operations_accept_their_exact_payload(), assert_array_root_rejected(), CompleteBody, cookie_is_read_and_duplicate_session_values_reject(), decode(), Decoded, enrollment_complete_rejects_positional_json_array_root(), enrollment_redeem_rejects_positional_json_array_root() (+20 more)

### Community 34 - "Account Authentication Authority"
Cohesion: 0.13
Nodes (21): AuthService<'_, D, R>, AccountId, CommandId, Result, actor_key(), AuthService<'_, D, R>, deadline(), operation_tag() (+13 more)

### Community 35 - "Auth Test Coverage"
Cohesion: 0.14
Nodes (27): actual_kdf_gap_answer_replacement_deletion_alias_epoch_and_terminal_changes_fail_closed(), aged_recovery_commands_are_rejected_after_receipt_cleanup_and_idle_denial_is_uniform(), answer_maintenance_kdf_gap_revoked_session_rolls_back_private_update(), close_retry_backoff_is_durable_bounded_and_ack_ignored_write_retains_work(), entropy_failure_unsupported_stored_profile_and_phc_fail_without_epoch_or_failure_accounting(), epoch(), exact_current_alias_and_uniform_absent_wrong_disabled_proof(), in_progress_recovery_preserves_complete_board_matching_layout_and_history() (+19 more)

### Community 36 - "Backend Workspace Structure"
Cohesion: 0.20
Nodes (25): BoardCell, CellPosition, call_sequence(), call_value(), canonical_alias(), cell_kind(), cells(), completed_lines() (+17 more)

### Community 37 - "Account Authentication Authority"
Cohesion: 0.25
Nodes (11): AuthError, AuthService<'_, D, R>, removal_finishes_only_after_all_exact_socket_close_acknowledgements(), FnOnce, Result, Row, StorageError, T (+3 more)

### Community 38 - "Account Authentication Authority"
Cohesion: 0.20
Nodes (15): AuthService<'_, D, R>, parse_work(), RemovalGateGrant, RemovalPhase, RemovalReleaseAck, RemovalWork, AccountId, CommandId (+7 more)

### Community 39 - "Account Authentication Authority"
Cohesion: 0.12
Nodes (19): Runtime, access_link_cleanup_and_alarm_share_the_exact_metadata_retention_deadline(), cleanup_limits_each_delete_and_expired_close_enqueue_to_the_named_batch_size(), entity_id(), Cell, Connection, FnOnce, RefCell (+11 more)

### Community 40 - "Durable SQLite Storage"
Cohesion: 0.23
Nodes (8): GameService<'_, D, R>, CommandId, Option, PlayerId, Result, GameError, GameView, GameId

### Community 41 - "Auth Test Coverage"
Cohesion: 0.16
Nodes (22): assert_verifier_rehashed(), assertion_failure_diagnostics_do_not_disclose_verifiers(), command_id(), context(), cookie(), create(), enrollment_and_reset_bind_status_purpose_and_scope_without_changing_stored_tags(), RecordedSqlite (+14 more)

### Community 42 - "Account Authentication Authority"
Cohesion: 0.12
Nodes (10): AccountId, AccountRole, GameAccountAuthority, GameSocketCloseWork, subscription_values(), trusted_close_metadata_preserves_each_identity_without_granting_account_authority(), trusted_peer_metadata_reconstruction_is_range_checked(), GameId (+2 more)

### Community 43 - "Backend Workspace Structure"
Cohesion: 0.11
Nodes (15): CreationResult, LobbyPreparation, ParticipantOutcome, PendingWork, CommandId, CreationReadyProof, FromStr, OperationId (+7 more)

### Community 44 - "Account Authentication Authority"
Cohesion: 0.33
Nodes (22): admin_session(), app_admin_self_disable_delete_and_enable_are_forbidden(), command_id(), committed_removal_phase_is_bound_and_persisted_in_real_sqlite(), deletion_does_not_require_an_unused_credential_epoch_increment(), deletion_requires_matching_gate_and_retains_intent_until_release_ack(), developer_cli_cannot_remove_the_last_enabled_verified_admin(), disable_changes_epoch_once_and_repeated_no_op_keeps_timestamp() (+14 more)

### Community 45 - "Auth Test Coverage"
Cohesion: 0.13
Nodes (17): ConfigError, InvalidValueKind, acquisition_can_only_request_the_declared_inventory(), assert_redacted(), dev_cli_key_is_optional_but_supplied_values_are_validated(), explicit_argon2_costs_are_deserialized_as_numbers(), keys_require_canonical_unpadded_base64url_for_exactly_32_bytes(), Result (+9 more)

### Community 46 - "Backend Workspace Structure"
Cohesion: 0.16
Nodes (21): discovery_accounts_owner_fences_original_sample_despite_rollback_without_renewal(), discovery_expiry_fence_readback_and_transaction_failure_roll_back(), discovery_expiry_fence_rejects_wrong_token_identity_metadata_and_sample(), discovery_expiry_fence_sqlite_ignore_abort_and_tampered_readback_never_ack(), fence_account_expiry(), Fixture, read_fence(), ReadbackFault (+13 more)

### Community 47 - "Backend Workspace Structure"
Cohesion: 0.21
Nodes (24): current_output(), deny_observed_expiry(), DiscoveryError, dispatch(), failure(), fence_expired_account(), handle(), lookup() (+16 more)

### Community 48 - "Worker HTTP Boundary"
Cohesion: 0.11
Nodes (19): deserialize_error_fields(), dispatch(), ErrorEnvelope, ErrorFields, handle(), log_delivery(), OwnerUsersRequest, AxumRequest (+11 more)

### Community 49 - "Backend Workspace Structure"
Cohesion: 0.13
Nodes (19): AccountCommand, AuditCommand, AuditPage, canonical_id(), Cli, LinkMutation, Mutation, Page (+11 more)

### Community 50 - "Backend Workspace Structure"
Cohesion: 0.10
Nodes (14): AuthCommand, Debug, Drop, Formatter, Option, Result, String, CookieWire (+6 more)

### Community 51 - "Backend Workspace Structure"
Cohesion: 0.12
Nodes (14): Self, Self, StorageError, add_game_coordination(), initialize_removal(), D, Result, Self (+6 more)

### Community 52 - "Auth Test Coverage"
Cohesion: 0.15
Nodes (19): account_exit_receipt_scalar_faults_roll_back_then_original_command_retries(), lobby_cancel_receipt_scalar_faults_roll_back_then_original_command_retries(), manual_call_receipt_scalar_faults_roll_back_then_original_command_retries(), new_cancel_receipt_scalar_faults_roll_back_then_original_command_retries(), Operation, player_exit_receipt_scalar_faults_roll_back_then_original_command_retries(), Prepared, random_call_receipt_scalar_faults_roll_back_then_original_command_retries() (+11 more)

### Community 53 - "Backend Workspace Structure"
Cohesion: 0.25
Nodes (15): Err, CallView, checked_deadline(), BoardView, GameState, Option, same_winner(), valid_terminal_shape() (+7 more)

### Community 54 - "Account Authentication Authority"
Cohesion: 0.13
Nodes (14): AuthService<'_, D, R>, FnOnce, Option, Result, T, ManagementPrincipal, Debug, Drop (+6 more)

### Community 55 - "Backend Workspace Structure"
Cohesion: 0.23
Nodes (6): decode(), encode(), GameDecodeError, Self, Vec, SnapshotFrame

### Community 56 - "Account Authentication Authority"
Cohesion: 0.22
Nodes (14): Account, integer(), optional_integer(), optional_text(), AccountId, Option, Result, Row (+6 more)

### Community 57 - "Worker HTTP Boundary"
Cohesion: 0.22
Nodes (16): decode_history(), filter_tag(), HistoryCommand, HistoryCursor, HistoryQuery, HistoryRequest, list_query(), Drop (+8 more)

### Community 58 - "Account Authentication Authority"
Cohesion: 0.14
Nodes (12): Cell, Connection, FnOnce, RefCell, Result, Row, Self, T (+4 more)

### Community 59 - "Backend Workspace Structure"
Cohesion: 0.16
Nodes (17): session_principal_zeroizes_its_owned_token_and_redacts_debug(), AuthPolicy, Self, bounded_phc(), hash_password(), new_token(), Result, String (+9 more)

### Community 60 - "Worker HTTP Boundary"
Cohesion: 0.23
Nodes (11): AuthService, D, R, OwnerDatabase, Self, AccountsObject, CommandId, OperationId (+3 more)

### Community 61 - "Backend Workspace Structure"
Cohesion: 0.15
Nodes (7): AccountConnectionGrant, ConnectionGrant, AccountId, ConnectionId, GameId, PlayerId, SessionId

### Community 62 - "Auth Test Coverage"
Cohesion: 0.14
Nodes (12): Cell, Connection, FnOnce, RefCell, Result, Row, Self, T (+4 more)

### Community 63 - "Backend Workspace Structure"
Cohesion: 0.20
Nodes (13): failure_cleanup_truncates_only_owned_handle_not_replaced_path(), LinkFile, persists_link_without_displaying_or_reopening_path(), replaced_path_is_rejected_before_secret_write(), reserves_new_private_file_before_network(), Drop, Path, PathBuf (+5 more)

### Community 64 - "Backend Workspace Structure"
Cohesion: 0.13
Nodes (13): Debug, Formatter, FromStr, CreateGame, DeliveryId, JoinPlayer, recovery_answer_explicit_zeroization_clears_owned_sensitive_bytes(), RecoveryAnswer (+5 more)

### Community 65 - "Typed Environment Configuration"
Cohesion: 0.22
Nodes (16): IntoIterator, absent_public_settings_use_serde_defaults(), assert_redacted(), default_api_path(), explicit_public_values_are_preserved(), get_frontend_config(), maximum_display_name_is_measured_in_utf8_bytes(), PublicAppConfig (+8 more)

### Community 66 - "Worker HTTP Boundary"
Cohesion: 0.17
Nodes (14): AccountAuthorityWire, AccountCloseIdentity, AccountCloseReply, AccountCloseRequest, AccountOutcome, AccountRejection, CloseAction, CloseCaller (+6 more)

### Community 67 - "Backend Workspace Structure"
Cohesion: 0.22
Nodes (15): CliError, human_and_json_account_output_escape_all_controls(), issued_output_projects_metadata_without_link_secrets(), replay_guidance(), CommandId, Result, String, Value (+7 more)

### Community 68 - "Backend Workspace Structure"
Cohesion: 0.18
Nodes (16): execute(), prepare(), Prepared, CommandId, Option, PathBuf, Result, Write (+8 more)

### Community 69 - "Backend Workspace Structure"
Cohesion: 0.13
Nodes (13): FaultReadback, HookRuntime, Cell, FnOnce, Option, RefCell, Result, Runtime (+5 more)

### Community 70 - "Backend Workspace Structure"
Cohesion: 0.19
Nodes (11): AuthOutcome, CookieEffect, RequestContext, Debug, Formatter, Option, Result, String (+3 more)

### Community 71 - "Backend Workspace Structure"
Cohesion: 0.19
Nodes (10): BTreeMap, NativeSource, Into, Mutex, Option, Result, Self, String (+2 more)

### Community 72 - "Account Authentication Authority"
Cohesion: 0.24
Nodes (11): AuthService<'_, D, R>, command_id(), CommandOutcome, fingerprint(), AccountId, CommandId, LinkId, Option (+3 more)

### Community 73 - "Discovery APIs"
Cohesion: 0.25
Nodes (14): all_rows(), discovery_authoritative_state_advances_without_directory_revision_equality(), discovery_availability_counts_valid_observed_connections_without_reconciliation(), discovery_fence_storage_fault_is_unavailable_and_rolls_back(), discovery_idle_denial_is_durable_write_free_recut_and_rollback_fenced(), discovery_player_deadline_denial_survives_clock_rollback_without_other_writes(), discovery_read_only_cuts_hide_allocations_and_do_not_create_grants_or_timers(), discovery_remote_expiry_fences_the_original_sample_without_other_writes() (+6 more)

### Community 74 - "Worker HTTP Boundary"
Cohesion: 0.31
Nodes (12): ApiError, command(), dispatch(), failure(), handle(), json_response(), Env, Response (+4 more)

### Community 75 - "Discovery APIs"
Cohesion: 0.25
Nodes (13): CurrentGame, CurrentGames, CurrentGameSummary, GameAvailability, GameEntry, GameSlot, AccountId, GameCode (+5 more)

### Community 76 - "Worker HTTP Boundary"
Cohesion: 0.24
Nodes (8): authenticate_cli(), CliRequest, decode_cli(), CommandId, HeaderMap, Method, Option, Result

### Community 77 - "Account Authentication Authority"
Cohesion: 0.19
Nodes (8): command(), Cell, Sqlite, TestRuntime, CommandId, Connection, Runtime, Self

### Community 78 - "Account Authentication Authority"
Cohesion: 0.27
Nodes (12): changes(), final_release_reproves_live_admin_without_writes_or_audit(), fixture(), id(), AccountId, CommandId, Row, Sqlite (+4 more)

### Community 79 - "Typed Environment Configuration"
Cohesion: 0.23
Nodes (11): BackendConfig, get_backend_config(), Option, Result, Self, String, deserialize_error(), EnvSource (+3 more)

### Community 80 - "Backend Workspace Structure"
Cohesion: 0.27
Nodes (13): assert_view_rejected(), host_new_view(), player_lobby_view(), review_view_awaiting_requires_pre_start_lifecycle_but_allows_hidden_host_code(), review_view_in_progress_requires_code_and_start_without_end_or_idle(), review_view_new_has_no_member_projection_or_lobby_metadata(), review_view_optional_board_is_absent_not_null(), review_view_requires_explicit_nullable_lifecycle_fields() (+5 more)

### Community 81 - "Backend Workspace Structure"
Cohesion: 0.30
Nodes (11): call_accounts(), dispatch(), handle(), OwnerManagementRequest, AxumRequest, AxumResponse, CommandId, Env (+3 more)

### Community 82 - "Auth Test Coverage"
Cohesion: 0.41
Nodes (11): accepted_calls_reconcile_pending_start_and_reject_stale_real_acknowledgements(), directory_rows(), host_revision(), pending_row(), pending_start(), pending_start_fence_required_write_faults_roll_back_calls_and_exact_retries(), ConnectionId, Fixture (+3 more)

### Community 83 - "Durable SQLite Storage"
Cohesion: 0.36
Nodes (5): GameService<'_, D, R>, GameId, Result, History, HistorySummary

### Community 84 - "Durable SQLite Storage"
Cohesion: 0.31
Nodes (10): auth_schema_ddl_matches_the_pre_refactor_snapshot(), auth_schema_statements(), connection(), String, Vec, sqlite_account_caps_match_verifier_and_safe_integer_limits(), sqlite_rate_buckets_match_failure_count_and_digest_limits(), sqlite_receipts_match_payload_digest_and_retention_limits() (+2 more)

### Community 85 - "Backend Workspace Structure"
Cohesion: 0.24
Nodes (8): creation_fingerprint(), CreationFingerprint, Err, GameConfiguration, Result, Self, StorageError, explicit_prestart_cancellation_is_atomic_replayable_and_has_no_history()

### Community 86 - "Backend Workspace Structure"
Cohesion: 0.36
Nodes (8): AccountsObject, Response, Result, safe_response_headers(), Request, String, Vec, Zeroizing

### Community 87 - "Auth Test Coverage"
Cohesion: 0.40
Nodes (9): all_rows(), receipt_rows(), receipt_update_faults_roll_back_then_original_ack_and_command_recover(), CommandId, Fixture, Row, Vec, scalar() (+1 more)

### Community 88 - "Discovery APIs"
Cohesion: 0.29
Nodes (5): directory(), get(), owner(), rows(), value()

### Community 89 - "Backend Workspace Structure"
Cohesion: 0.24
Nodes (7): ManagementClient, Prepared, Result, Self, String, Client, HeaderValue

### Community 90 - "Shared Account Domain"
Cohesion: 0.27
Nodes (6): ValidationError, Result, SessionScope, validate_password(), validate_username(), validators_use_the_declared_length_boundaries()

### Community 91 - "Account Authentication Authority"
Cohesion: 0.42
Nodes (6): AuthService<'_, D, R>, fingerprint(), CommandId, Option, Result, validate_receipt()

### Community 93 - "Auth Test Coverage"
Cohesion: 0.25
Nodes (5): history_queries_are_canonical_bounded_unique_and_filter_bound(), read_history(), HeaderMap, Option, Result

### Community 95 - "Backend Workspace Structure"
Cohesion: 0.25
Nodes (6): value(), Result, Vec, SqlValue, Row, Vec

### Community 97 - "Backend Workspace Structure"
Cohesion: 0.39
Nodes (6): Bindings, get_backend_config(), Env, Option, Result, String

### Community 98 - "Backend Workspace Structure"
Cohesion: 0.50
Nodes (6): enroll(), normalize(), push(), Result, String, Zeroizing

### Community 99 - "Worker HTTP Boundary"
Cohesion: 0.43
Nodes (4): AuthService, Request, Response, super::AccountsObject

### Community 100 - "Worker HTTP Boundary"
Cohesion: 0.62
Nodes (5): AccountsObject, management_error(), management_result(), Response, Result

### Community 101 - "Discovery APIs"
Cohesion: 0.52
Nodes (6): directory(), get(), intercept(), owner(), restore(), setup()

### Community 102 - "Auth Test Coverage"
Cohesion: 0.33
Nodes (5): BrokenPipe, ca_file_read_is_bounded_and_errors_are_redacted(), existing_output_target_prevents_any_http_mutation(), Result, Write

### Community 103 - "Backend Workspace Structure"
Cohesion: 0.29
Nodes (6): Drop, Option, Server, JoinHandle, Receiver, TempDir

### Community 104 - "Auth Test Coverage"
Cohesion: 0.33
Nodes (5): failed_get_caches_a_safe_error_without_retrying(), NeverRead, Option, Result, String

### Community 105 - "Auth Test Coverage"
Cohesion: 0.33
Nodes (5): get_initializes_lazily_and_never_acquires_again(), NeverRead, Option, Result, String

### Community 106 - "Auth Test Coverage"
Cohesion: 0.33
Nodes (5): constructor_acquires_every_declared_key_once(), Box, Error, Result, serde_deserializes_the_backend_struct_with_optional_defaults()

### Community 107 - "Durable SQLite Storage"
Cohesion: 0.60
Nodes (5): game_record_statement(), recovery_coordination_statements(), String, Vec, statements()

### Community 108 - "Discovery APIs"
Cohesion: 0.53
Nodes (4): directory(), get(), owner(), unchangedDomain()

### Community 110 - "Auth Test Coverage"
Cohesion: 0.40
Nodes (5): constructor_is_zero_argument_and_uses_only_public_build_inputs(), Box, Error, Result, serde_deserializes_the_public_struct_with_optional_defaults()

### Community 111 - "Auth Test Coverage"
Cohesion: 0.27
Nodes (4): public_singleton_caches_the_constructor_outcome(), Result, get_initializes_the_public_config_and_returns_the_same_instance(), Result

### Community 112 - "Backend Workspace Structure"
Cohesion: 0.40
Nodes (4): GameDirectoryObject, Request, Response, Result

### Community 114 - "Backend Workspace Structure"
Cohesion: 0.70
Nodes (4): Prepared, Result, validate_receipt(), validate_response()

### Community 115 - "Auth Test Coverage"
Cohesion: 0.70
Nodes (4): enrollment_url(), link_token(), String, Zeroizing

### Community 117 - "Backend Workspace Structure"
Cohesion: 0.50
Nodes (3): prepare(), Prepared, Result

### Community 118 - "Backend Workspace Structure"
Cohesion: 0.50
Nodes (3): prepare(), Prepared, Result

### Community 120 - "Typed Environment Configuration"
Cohesion: 0.50
Nodes (3): Result, String, validate_origin()

## Knowledge Gaps
- **9 isolated node(s):** `AuthService<'_, D, R>`, `GameObject`, `CallMode`, `IdError`, `super::directory::GameDirectoryObject` (+4 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **137 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `settle_probe()` connect `Worker HTTP Boundary` to `Durable SQLite Storage`?**
  _High betweenness centrality (0.000) - this node is a cross-community bridge._
- **Why does `migrate_directory()` connect `Auth Test Coverage` to `Durable SQLite Storage`?**
  _High betweenness centrality (0.000) - this node is a cross-community bridge._
- **What connects `AuthService<'_, D, R>`, `GameObject`, `CallMode` to the rest of the system?**
  _9 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Durable SQLite Storage` be split into smaller, more focused modules?**
  _Cohesion score 0.06017791732077447 - nodes in this community are weakly interconnected._
- **Should `Auth Test Coverage` be split into smaller, more focused modules?**
  _Cohesion score 0.06041189931350115 - nodes in this community are weakly interconnected._
- **Should `Worker HTTP Boundary` be split into smaller, more focused modules?**
  _Cohesion score 0.06777445855115757 - nodes in this community are weakly interconnected._
- **Should `Auth Test Coverage` be split into smaller, more focused modules?**
  _Cohesion score 0.07715355805243446 - nodes in this community are weakly interconnected._