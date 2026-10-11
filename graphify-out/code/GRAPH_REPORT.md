# Scope: code-only structural graph

Rust/JavaScript/Python source and tests only. The older documentation graph remains stale. No LLM token cost.

AST navigation limitations: {"dangling_endpoint_edges": 590, "directed_same_endpoint_collapsed_edges": 322}. Raw extraction retains edge evidence; this graph is not correctness proof.

# Graph Report - /home/orangepi/Workspace/brews-bingo  (2026-10-11)

## Corpus Check
- 174 files · ~165,084 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 3407 nodes · 9557 edges · 239 communities (144 shown, 95 thin omitted)
- Extraction: 97% EXTRACTED · 3% INFERRED · 0% AMBIGUOUS · INFERRED: 314 edges (avg confidence: 0.8)
- Token cost: 0 input · 0 output

## Community Hubs (Navigation)
- Auth Test Coverage
- Auth Test Coverage
- Auth Test Coverage
- Auth Test Coverage
- Backend Workspace Structure
- Auth Test Coverage
- Backend Workspace Structure
- Durable SQLite Storage
- Worker HTTP Boundary
- Durable SQLite Storage
- Backend Workspace Structure
- Backend Workspace Structure
- Auth Test Coverage
- Durable SQLite Storage
- Worker HTTP Boundary
- Backend Workspace Structure
- Durable SQLite Storage
- Worker HTTP Boundary
- Worker HTTP Boundary
- Backend Workspace Structure
- Worker HTTP Boundary
- Worker HTTP Boundary
- Worker HTTP Boundary
- Backend Workspace Structure
- Backend Workspace Structure
- Worker HTTP Boundary
- Auth Test Coverage
- Backend Workspace Structure
- Account Authentication Authority
- Account Authentication Authority
- Durable SQLite Storage
- Auth Test Coverage
- Auth Test Coverage
- Worker HTTP Boundary
- Durable SQLite Storage
- Account Authentication Authority
- Durable SQLite Storage
- Backend Workspace Structure
- Account Authentication Authority
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Account Authentication Authority
- Durable SQLite Storage
- Durable SQLite Storage
- Account Authentication Authority
- Durable SQLite Storage
- Discovery APIs
- Worker HTTP Boundary
- Auth Test Coverage
- Backend Workspace Structure
- Account Authentication Authority
- Backend Workspace Structure
- Account Authentication Authority
- Backend Workspace Structure
- Backend Workspace Structure
- Typed Environment Configuration
- Auth Test Coverage
- Worker HTTP Boundary
- Account Authentication Authority
- Worker HTTP Boundary
- Auth Test Coverage
- Backend Workspace Structure
- Typed Environment Configuration
- Backend Workspace Structure
- Auth Test Coverage
- Shared Account Domain
- Backend Workspace Structure
- Shared Account Domain
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Auth Test Coverage
- Auth Test Coverage
- Backend Workspace Structure
- Account Authentication Authority
- Backend Workspace Structure
- Account Authentication Authority
- Worker HTTP Boundary
- Account Authentication Authority
- Discovery APIs
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Worker HTTP Boundary
- Backend Workspace Structure
- Auth Test Coverage
- Durable SQLite Storage
- Account Authentication Authority
- Worker HTTP Boundary
- Worker HTTP Boundary
- Auth Test Coverage
- Discovery APIs
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Account Authentication Authority
- Account Authentication Authority
- Worker HTTP Boundary
- Auth Test Coverage
- Auth Test Coverage
- Auth Test Coverage
- Shared Account Domain
- Account Authentication Authority
- Backend Workspace Structure
- Backend Workspace Structure
- Durable SQLite Storage
- Worker HTTP Boundary
- Discovery APIs
- Backend Workspace Structure
- Auth Test Coverage
- Auth Test Coverage
- Auth Test Coverage
- Shared Account Domain
- Backend Workspace Structure
- Discovery APIs
- Shared Account Domain
- Auth Test Coverage
- Backend Workspace Structure
- Account Authentication Authority
- Backend Workspace Structure
- Auth Test Coverage
- Backend Workspace Structure
- Auth Test Coverage
- Typed Environment Configuration
- Backend Workspace Structure
- Backend Workspace Structure
- Typed Environment Configuration
- Durable SQLite Storage
- Discovery APIs
- Backend Workspace Structure
- Worker HTTP Boundary
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

## God Nodes (most connected - your core abstractions)
1. `GameError` - 154 edges
2. `migrate()` - 93 edges
3. `GameService<'a, D, R>` - 80 edges
4. `migrate_directory()` - 70 edges
5. `ManagementError` - 64 edges
6. `operation()` - 62 edges
7. `AuthError` - 56 edges
8. `account()` - 56 edges
9. `text()` - 49 edges
10. `string()` - 40 edges

## Surprising Connections (you probably didn't know these)
- `sqlite_username_boundaries_match_domain_validation_on_insert_and_update()` --calls--> `validate_username()`  [INFERRED]
  backend/src/db/schema/auth_schema.rs → shared/domain/src/accounts.rs
- `validate_summary()` --calls--> `validate_alias()`  [INFERRED]
  backend/src/worker_runtime/history_wire.rs → shared/domain/src/games.rs
- `CliRequest` --references--> `ManagementCommand`  [EXTRACTED]
  backend/src/api/developer.rs → shared/contracts/src/management.rs
- `GamePayload` --references--> `AdmissionContextInput`  [EXTRACTED]
  backend/src/api/games.rs → shared/contracts/src/games.rs
- `GamePayload` --references--> `CallManualInput`  [EXTRACTED]
  backend/src/api/games.rs → shared/contracts/src/games.rs

## Import Cycles
- None detected.

## Communities (239 total, 95 thin omitted)

### Community 0 - "Auth Test Coverage"
Cohesion: 0.07
Nodes (102): migrate(), D, a_current_host_session_is_attributed_when_admin_authority_is_denied(), account_detail_is_safe_and_deleted_or_unknown_target_is_not_found(), account_pages_are_bounded_and_continue_without_duplicates(), audit_write_failure_rolls_back_account_link_and_receipt(), authenticated_rejections_are_durably_audited(), command_id() (+94 more)

### Community 1 - "Auth Test Coverage"
Cohesion: 0.08
Nodes (84): accepted_started_terminal_proof_cannot_rewrite_original_expiry_with_newer_revision(), account(), assignment_gate_blocks_newer_projection_to_removing_host(), ByteRuntime, claim_reserves_identity_indexes_host_and_replays_same_command(), clock_advance_during_entropy_rechecks_common_fresh_admission_floor(), code_rejection_sampling_has_bounded_entropy_and_rolls_back_on_exhaustion(), command() (+76 more)

### Community 2 - "Auth Test Coverage"
Cohesion: 0.09
Nodes (75): migrate_directory(), account(), acquire_persists_a_presence_only_gate_with_separate_operation_identity(), acquired_gate_rejects_transfer_without_changing_the_original_host(), competing_operation_returns_busy_without_replacing_the_current_gate(), completed_operation_cannot_reacquire_or_clear_a_newer_gate(), concurrent_operations_on_one_account_serialize_to_one_grant_and_one_busy(), corrupted_completion_time_cannot_erase_the_fence_or_authorize_an_ack() (+67 more)

### Community 3 - "Auth Test Coverage"
Cohesion: 0.09
Nodes (63): AuthOutcome, creation_fingerprint(), GameConfiguration, Result, Self, StorageError, String, Zeroizing (+55 more)

### Community 4 - "Backend Workspace Structure"
Cohesion: 0.08
Nodes (48): AccountConnectionGrant, DiscoveryCut, DiscoveryValue, DiscoveryViewer, GameService<'_, D, R>, CurrentGameSummary, GameAccountAuthority, GameAvailability (+40 more)

### Community 5 - "Auth Test Coverage"
Cohesion: 0.08
Nodes (49): directoryStub(), initializeDirectory(), commandId(), cookie(), current(), enroll(), inOwner(), password() (+41 more)

### Community 6 - "Backend Workspace Structure"
Cohesion: 0.06
Nodes (22): CodeGrant, CreationAck, CreationFingerprint, CreationReadyProof, CreationWork, DiscoveryRoute, GameProjection, ProjectionAck (+14 more)

### Community 7 - "Durable SQLite Storage"
Cohesion: 0.09
Nodes (49): binding_params(), DeliveryBinding, DeliveryBudget, DeliveryError, DeliveryService, DeliveryService<'a, D, R>, DeliveryService<'_, D, R>, migrate_game_delivery() (+41 more)

### Community 8 - "Worker HTTP Boundary"
Cohesion: 0.09
Nodes (63): account_reply(), acknowledge_creation(), allocate_game_code(), authorize_account(), authorize_account_connection(), authorize_discovery_account(), bounded_objects(), call() (+55 more)

### Community 9 - "Durable SQLite Storage"
Cohesion: 0.09
Nodes (34): application_tables(), DirectoryService, DirectoryService<'a, D, R>, operation_timestamp(), parse_completion_row(), parse_operation_row(), AccountId, D (+26 more)

### Community 10 - "Backend Workspace Structure"
Cohesion: 0.07
Nodes (43): admission_and_join_validate_lookup_alias_and_never_debug_recovery_material(), assert_response_rejected(), created_completion_can_be_later_than_creation_without_moving_idle_deadline(), creation_resolves_defaults_and_overrides_with_effective_center(), game_tags_and_safe_receipt_encoding_round_trip_without_parser_details(), gameplay_command_inputs_are_closed_canonical_and_state_fenced(), hardened_tagged_maps_preserve_original_duplicates_and_closed_fields(), host_projection_includes_disconnected_retained_boards_without_sessions() (+35 more)

### Community 11 - "Backend Workspace Structure"
Cohesion: 0.11
Nodes (36): AccountCloseRequest, AuthPolicy, GameService, R, close_checked(), GameObject, peer_error(), private_response() (+28 more)

### Community 12 - "Auth Test Coverage"
Cohesion: 0.09
Nodes (47): changes(), final_release_reproves_live_admin_without_writes_or_audit(), fixture(), id(), AccountId, CommandId, Row, Sqlite (+39 more)

### Community 13 - "Durable SQLite Storage"
Cohesion: 0.17
Nodes (23): command_timestamp(), deadline(), DirectoryService<'_, D, R>, integer(), optional_integer(), parse_creation(), parse_index(), parse_optional_integer() (+15 more)

### Community 14 - "Worker HTTP Boundary"
Cohesion: 0.11
Nodes (44): absent_after_deadline_keeps_reservation_and_saturating_retry(), command(), Fixture, genuine_committed_lobby_recovers_directory_publication_without_faking_game_ack(), genuine_initializer_evidence_acknowledges_only_current_unready_creation(), genuine_new_cancellation_releases_only_the_hidden_directory_association(), genuine_owner_terminal_evidence_releases_reservation_after_deadline(), hidden_code_release_preserves_exact_terminal_identity_and_publication_fences() (+36 more)

### Community 15 - "Backend Workspace Structure"
Cohesion: 0.08
Nodes (36): id_check(), management_schema_statements(), String, Vec, DirectoryRejection, DirectoryRequest, DirectoryResponse, GameDirectoryObject (+28 more)

### Community 16 - "Durable SQLite Storage"
Cohesion: 0.08
Nodes (34): account_acceptance_preflight_rejects_fenced_or_replaced_preparations_without_accepting(), add_three_calendar_months(), authority(), civil_from_days(), close_work(), days_from_civil(), days_in_month(), forward_schema_migration_accepts_legitimate_prestart_idle_cancellation_and_pending_release() (+26 more)

### Community 17 - "Worker HTTP Boundary"
Cohesion: 0.09
Nodes (40): account_get_preserves_the_canonical_typed_target(), action_routes_reject_other_methods_without_decoding_payloads(), bodyless_mutation_routes_map_to_their_closed_management_commands(), bodyless_mutations(), collection_get_maps_to_default_list_without_claiming_authority(), collection_query(), collection_query_byte_bound_precedes_parsing(), collection_query_maps_only_cursor_and_bounded_limit_in_either_order() (+32 more)

### Community 18 - "Worker HTTP Boundary"
Cohesion: 0.09
Nodes (38): GameService<'_, D, R>, GameId, Result, confirm_expiry_denial(), current(), dispatch(), failure(), handle() (+30 more)

### Community 19 - "Backend Workspace Structure"
Cohesion: 0.07
Nodes (42): game_record_statement(), String, Vec, statements(), GameObject, peer_error(), route(), DiscoveryRoute (+34 more)

### Community 20 - "Worker HTTP Boundary"
Cohesion: 0.10
Nodes (33): checked_revision(), directory_claim_ack_and_code_use_real_owner_core_and_exact_binding(), directory_publication_recovery_and_release_use_real_core(), DirectoryGameOutcome, DirectoryGameRejection, DirectoryGameReply, DirectoryGameRequest, dispatch() (+25 more)

### Community 21 - "Worker HTTP Boundary"
Cohesion: 0.14
Nodes (36): account_registration_and_post_gate_recheck_use_real_accounts_core(), AccountAction, AccountCloseReply, AccountCloseRequest, AccountConnectionIdentity, AccountOutcome, AccountReply, AccountRequest (+28 more)

### Community 22 - "Worker HTTP Boundary"
Cohesion: 0.13
Nodes (37): ApiError, admission_context_is_keyless_and_join_uses_separate_game_scoped_cookie(), admission_forwards_an_existing_player_cookie_without_claiming_authority(), all_transport_inputs_have_release_active_finite_bounds(), command_id(), creation_route_decodes_original_configuration_without_caller_identity(), decode_empty_game_body(), decode_games() (+29 more)

### Community 23 - "Backend Workspace Structure"
Cohesion: 0.08
Nodes (32): action(), replay_guidance(), AuditEvent, AuditOperation, AuditOutcome, AuditTarget, deserialize(), json_decode_errors_discard_values_and_parser_sources() (+24 more)

### Community 24 - "Backend Workspace Structure"
Cohesion: 0.09
Nodes (35): report_rehash_failure(), discovery_accounts_owner_fences_original_sample_despite_rollback_without_renewal(), discovery_expiry_fence_readback_and_transaction_failure_roll_back(), discovery_expiry_fence_rejects_wrong_token_identity_metadata_and_sample(), discovery_expiry_fence_sqlite_ignore_abort_and_tampered_readback_never_ack(), fence_account_expiry(), Fixture, read_fence() (+27 more)

### Community 25 - "Worker HTTP Boundary"
Cohesion: 0.10
Nodes (28): GameOperation, GameViewSelector, call_success_and_receipt_only_retry_match_exact_manual_request(), exit_success_and_receipt_retry_are_bound_only_to_selected_exit_command(), GameAction, GameCookieWire, GameIngress, GameOwnerResponse (+20 more)

### Community 26 - "Auth Test Coverage"
Cohesion: 0.10
Nodes (34): assert_issued_handoff(), audit_reads_preserve_required_targets_and_read_operations(), authorization_debug_is_redacted_and_server_errors_discard_bodies(), BrokenPipe, ca_file_read_is_bounded_and_errors_are_redacted(), client_rejects_noncanonical_or_non_https_origins_before_network(), command_runner_exercises_real_https_to_safe_metadata(), existing_output_target_prevents_any_http_mutation() (+26 more)

### Community 27 - "Backend Workspace Structure"
Cohesion: 0.07
Nodes (28): Arc, api_failure(), auth_failure(), Boundary, Capture, Delivery, Failure, init() (+20 more)

### Community 28 - "Account Authentication Authority"
Cohesion: 0.11
Nodes (24): AuthService<'_, D, R>, AccountId, CommandId, Result, AuthService<'_, D, R>, AccountId, CommandId, Result (+16 more)

### Community 29 - "Account Authentication Authority"
Cohesion: 0.22
Nodes (16): AuthError, From, add_deadline(), AuthService<'a, D, R>, AccountId, D, FnOnce, Option (+8 more)

### Community 30 - "Durable SQLite Storage"
Cohesion: 0.14
Nodes (10): advance(), PlayerSession, ConnectionId, GameView, SessionId, ToString, text(), ConnectionGrant (+2 more)

### Community 31 - "Auth Test Coverage"
Cohesion: 0.08
Nodes (23): ConfigError, InvalidValueKind, acquisition_can_only_request_the_declared_inventory(), assert_redacted(), dev_cli_key_is_optional_but_supplied_values_are_validated(), explicit_argon2_costs_are_deserialized_as_numbers(), keys_require_canonical_unpadded_base64url_for_exactly_32_bytes(), Result (+15 more)

### Community 32 - "Auth Test Coverage"
Cohesion: 0.24
Nodes (33): bearer(), clock_rollback_and_unavailable_storage_never_release_authority(), close_pages_are_bounded_stably_ordered_and_do_not_discard_backlog(), close_retry_is_durable_capped_and_saturates_count_and_deadline(), connection(), context(), current_normal_host_and_admin_produce_private_game_authority(), due_close_work_is_bounded_and_preserves_exact_revoked_binding() (+25 more)

### Community 33 - "Worker HTTP Boundary"
Cohesion: 0.13
Nodes (28): all_seven_operations_accept_their_exact_payload(), assert_array_root_rejected(), CompleteBody, cookie_is_read_and_duplicate_session_values_reject(), decode(), Decoded, enrollment_complete_rejects_positional_json_array_root(), enrollment_redeem_rejects_positional_json_array_root() (+20 more)

### Community 34 - "Durable SQLite Storage"
Cohesion: 0.23
Nodes (14): account_actor(), add_time(), authorize(), authorize_mutation(), command_fingerprint(), ensure_before_idle(), line_parts(), revision() (+6 more)

### Community 35 - "Account Authentication Authority"
Cohesion: 0.24
Nodes (13): AuthError, AuthService<'_, D, R>, removal_finishes_only_after_all_exact_socket_close_acknowledgements(), FnOnce, Result, Row, StorageError, T (+5 more)

### Community 36 - "Durable SQLite Storage"
Cohesion: 0.21
Nodes (6): GameService<'a, D, R>, OperationId, Result, T, Vec, FnOnce

### Community 37 - "Backend Workspace Structure"
Cohesion: 0.20
Nodes (18): CallView, checked_deadline(), ConfigurationOverrides, GameDecodeError, GameView, BoardView, Err, GameId (+10 more)

### Community 38 - "Account Authentication Authority"
Cohesion: 0.20
Nodes (15): actor_key(), AuthService<'_, D, R>, RemovalGateGrant, RemovalPhase, RemovalReleaseAck, RemovalWork, AccountId, CommandId (+7 more)

### Community 39 - "Backend Workspace Structure"
Cohesion: 0.17
Nodes (27): current_output(), deny_observed_expiry(), DiscoveryError, dispatch(), failure(), fence_expired_account(), handle(), lookup() (+19 more)

### Community 40 - "Backend Workspace Structure"
Cohesion: 0.12
Nodes (23): account_fields_reject_positional_sequences(), assert_invalid_response(), CreateUsername, deserialize_object(), deserialize_users(), every_public_variant_round_trips_with_safe_debug_and_http_status(), nested_accounts_preserve_duplicate_and_unknown_field_rejection(), nested_objects_preserve_exact_wire_bytes() (+15 more)

### Community 41 - "Backend Workspace Structure"
Cohesion: 0.12
Nodes (14): GamePayload, AdmissionContextInput, CallManualInput, CancelGameInput, CreateGame, decode(), GameReceipt, GameResponse (+6 more)

### Community 42 - "Account Authentication Authority"
Cohesion: 0.13
Nodes (18): access_link_cleanup_and_alarm_share_the_exact_metadata_retention_deadline(), cleanup_limits_each_delete_and_expired_close_enqueue_to_the_named_batch_size(), entity_id(), Cell, Connection, FnOnce, RefCell, Result (+10 more)

### Community 43 - "Durable SQLite Storage"
Cohesion: 0.12
Nodes (23): Admission, bounded_random_index(), call_fingerprint(), choose_remaining_value(), Record, AccountId, GameId, GameState (+15 more)

### Community 44 - "Durable SQLite Storage"
Cohesion: 0.24
Nodes (8): optional_text(), GameService<'_, D, R>, CommandId, Option, PlayerId, Result, GameError, From

### Community 45 - "Account Authentication Authority"
Cohesion: 0.33
Nodes (22): admin_session(), app_admin_self_disable_delete_and_enable_are_forbidden(), command_id(), committed_removal_phase_is_bound_and_persisted_in_real_sqlite(), deletion_does_not_require_an_unused_credential_epoch_increment(), deletion_requires_matching_gate_and_retains_intent_until_release_ack(), developer_cli_cannot_remove_the_last_enabled_verified_admin(), disable_changes_epoch_once_and_repeated_no_op_keeps_timestamp() (+14 more)

### Community 46 - "Durable SQLite Storage"
Cohesion: 0.22
Nodes (17): blob(), join_fingerprint(), make_id(), number(), optional_number(), parse_connection(), parse_game_call(), parse_line() (+9 more)

### Community 47 - "Discovery APIs"
Cohesion: 0.16
Nodes (21): availability_selects_exact_context_without_credential_fallback(), AvailabilityView, decode_discovery(), DiscoveryCommand, DiscoveryRequest, no_query(), query_fields(), read() (+13 more)

### Community 48 - "Worker HTTP Boundary"
Cohesion: 0.11
Nodes (19): deserialize_error_fields(), dispatch(), ErrorEnvelope, ErrorFields, handle(), log_delivery(), OwnerUsersRequest, AxumRequest (+11 more)

### Community 49 - "Auth Test Coverage"
Cohesion: 0.19
Nodes (20): assert_verifier_rehashed(), assertion_failure_diagnostics_do_not_disclose_verifiers(), command_id(), context(), cookie(), create(), enrollment_and_reset_bind_status_purpose_and_scope_without_changing_stored_tags(), RecordedSqlite (+12 more)

### Community 50 - "Backend Workspace Structure"
Cohesion: 0.13
Nodes (19): AccountCommand, AuditCommand, AuditPage, canonical_id(), Cli, LinkMutation, Mutation, Page (+11 more)

### Community 51 - "Account Authentication Authority"
Cohesion: 0.18
Nodes (17): parse_work(), Row, Account, integer(), optional_integer(), optional_text(), AccountId, Option (+9 more)

### Community 52 - "Backend Workspace Structure"
Cohesion: 0.29
Nodes (20): call_sequence(), call_value(), canonical_alias(), cell_kind(), cells(), completed_lines(), configuration(), lookup_code() (+12 more)

### Community 53 - "Account Authentication Authority"
Cohesion: 0.13
Nodes (7): AccountId, AccountRole, GameAccountAuthority, GameSocketCloseWork, AccountAuthorityWire, OperationId, SessionId

### Community 54 - "Backend Workspace Structure"
Cohesion: 0.12
Nodes (15): AuthService<'_, D, R>, FnOnce, Option, Result, T, ManagementPrincipal, Debug, Drop (+7 more)

### Community 55 - "Backend Workspace Structure"
Cohesion: 0.13
Nodes (12): same_work_target(), CreationResult, LobbyPreparation, PendingWork, CommandId, GameReceipt, GameResponse, OperationId (+4 more)

### Community 56 - "Typed Environment Configuration"
Cohesion: 0.12
Nodes (14): BackendConfig, get_backend_config(), Option, Result, Self, String, EnvSource, D (+6 more)

### Community 57 - "Auth Test Coverage"
Cohesion: 0.16
Nodes (18): account_exit_receipt_scalar_faults_roll_back_then_original_command_retries(), lobby_cancel_receipt_scalar_faults_roll_back_then_original_command_retries(), manual_call_receipt_scalar_faults_roll_back_then_original_command_retries(), new_cancel_receipt_scalar_faults_roll_back_then_original_command_retries(), Operation, player_exit_receipt_scalar_faults_roll_back_then_original_command_retries(), Prepared, random_call_receipt_scalar_faults_roll_back_then_original_command_retries() (+10 more)

### Community 58 - "Worker HTTP Boundary"
Cohesion: 0.22
Nodes (16): decode_history(), filter_tag(), HistoryCommand, HistoryCursor, HistoryQuery, HistoryRequest, list_query(), Drop (+8 more)

### Community 59 - "Account Authentication Authority"
Cohesion: 0.14
Nodes (12): Cell, Connection, FnOnce, RefCell, Result, Row, Self, T (+4 more)

### Community 60 - "Worker HTTP Boundary"
Cohesion: 0.23
Nodes (11): AuthService, D, R, OwnerDatabase, Self, AccountsObject, CommandId, OperationId (+3 more)

### Community 61 - "Auth Test Coverage"
Cohesion: 0.14
Nodes (12): Cell, Connection, FnOnce, RefCell, Result, Row, Self, T (+4 more)

### Community 62 - "Backend Workspace Structure"
Cohesion: 0.20
Nodes (13): failure_cleanup_truncates_only_owned_handle_not_replaced_path(), LinkFile, persists_link_without_displaying_or_reopening_path(), replaced_path_is_rejected_before_secret_write(), reserves_new_private_file_before_network(), Drop, Path, PathBuf (+5 more)

### Community 63 - "Typed Environment Configuration"
Cohesion: 0.22
Nodes (16): IntoIterator, absent_public_settings_use_serde_defaults(), assert_redacted(), default_api_path(), explicit_public_values_are_preserved(), get_frontend_config(), maximum_display_name_is_measured_in_utf8_bytes(), PublicAppConfig (+8 more)

### Community 64 - "Backend Workspace Structure"
Cohesion: 0.24
Nodes (17): dispatch(), handle(), json_response(), peer_error(), rejected(), ApiError, Env, GameId (+9 more)

### Community 65 - "Auth Test Coverage"
Cohesion: 0.18
Nodes (16): all_rows(), count(), end_without_winner(), explicit_prestart_cancellation_is_atomic_replayable_and_has_no_history(), final_exit_removes_all_player_or_account_grants_but_never_changes_history(), gameplay_response_guard_is_read_only_and_accepts_terminal_without_new_view_grant(), history_expiry_denies_immediately_then_purges_with_recoverable_directory_release(), malformed_history_cannot_be_erased_without_validating_its_self_contained_snapshot() (+8 more)

### Community 66 - "Shared Account Domain"
Cohesion: 0.24
Nodes (11): GameConfiguration, GameValidationError, D, Default, Err, Error, Ok, Result (+3 more)

### Community 67 - "Backend Workspace Structure"
Cohesion: 0.24
Nodes (14): CliError, human_and_json_account_output_escape_all_controls(), issued_output_projects_metadata_without_link_secrets(), CommandId, Result, String, Value, Write (+6 more)

### Community 68 - "Shared Account Domain"
Cohesion: 0.15
Nodes (9): accepted_value_matches_all_cells_once_and_recomputes_single_line(), CallMode, complete_configuration_enforces_approved_ranges_free_positions_and_pool(), feasibility_counts_entire_roster_with_capped_falling_factorial(), generation_budget_covers_whole_start_and_random_errors_return_no_partial_boards(), generation_is_deterministic_stable_id_order_unique_and_initially_qualified(), SingleLine, SingleLinePattern (+1 more)

### Community 69 - "Backend Workspace Structure"
Cohesion: 0.12
Nodes (8): AuthCommand, Debug, Drop, Formatter, Option, Result, String, AccessLinkPurpose

### Community 70 - "Backend Workspace Structure"
Cohesion: 0.15
Nodes (10): Self, Self, StorageError, add_game_coordination(), initialize_removal(), D, Result, Self (+2 more)

### Community 71 - "Backend Workspace Structure"
Cohesion: 0.19
Nodes (13): AuthPolicy, Default, Self, bounded_phc(), hash_password(), Result, String, validate_policy() (+5 more)

### Community 72 - "Auth Test Coverage"
Cohesion: 0.17
Nodes (13): Runtime, additional_terminal_required_writes_ignore_abort_rollback_and_exact_retry(), all_call_write_faults_rollback_then_original_command_retries(), host_revision(), random_call_unbiased_rejection_chooses_remaining_index_and_replay_uses_no_entropy(), Cell, ConnectionId, Fixture (+5 more)

### Community 73 - "Auth Test Coverage"
Cohesion: 0.19
Nodes (10): fresh_history_login(), history_at_last_retained_millisecond(), history_final_response_expiry_fence_faults_roll_back_all_rows(), history_final_response_expiry_fence_survives_rollback_and_file_reopen(), history_final_response_expiry_read_error_rolls_back_required_write(), immutable_history_survives_actual_file_reopen_after_final_grants_exit(), Fixture, GameId (+2 more)

### Community 74 - "Backend Workspace Structure"
Cohesion: 0.19
Nodes (10): BTreeMap, NativeSource, Into, Mutex, Option, Result, Self, String (+2 more)

### Community 75 - "Account Authentication Authority"
Cohesion: 0.18
Nodes (9): command(), Cell, Sqlite, TestRuntime, trusted_close_metadata_preserves_each_identity_without_granting_account_authority(), trusted_peer_metadata_reconstruction_is_range_checked(), Connection, Runtime (+1 more)

### Community 76 - "Backend Workspace Structure"
Cohesion: 0.18
Nodes (10): value(), Result, Vec, SqlValue, FnOnce, Result, Row, T (+2 more)

### Community 77 - "Account Authentication Authority"
Cohesion: 0.24
Nodes (11): AuthService<'_, D, R>, command_id(), CommandOutcome, fingerprint(), AccountId, CommandId, LinkId, Option (+3 more)

### Community 78 - "Worker HTTP Boundary"
Cohesion: 0.22
Nodes (9): ApiError, authenticate_cli(), CliRequest, decode_cli(), CommandId, HeaderMap, Method, Option (+1 more)

### Community 79 - "Account Authentication Authority"
Cohesion: 0.23
Nodes (10): AuthOutcome, CookieEffect, RequestContext, Debug, Formatter, Option, Result, String (+2 more)

### Community 80 - "Discovery APIs"
Cohesion: 0.25
Nodes (13): CurrentGame, CurrentGames, CurrentGameSummary, GameAvailability, GameEntry, GameSlot, AccountId, GameCode (+5 more)

### Community 81 - "Backend Workspace Structure"
Cohesion: 0.16
Nodes (9): DeliveryId, recovery_answer_explicit_zeroization_clears_owned_sensitive_bytes(), RecoveryAnswer, Debug, Formatter, FromStr, String, Zeroize (+1 more)

### Community 82 - "Backend Workspace Structure"
Cohesion: 0.27
Nodes (13): assert_view_rejected(), host_new_view(), player_lobby_view(), review_view_awaiting_requires_pre_start_lifecycle_but_allows_hidden_host_code(), review_view_in_progress_requires_code_and_start_without_end_or_idle(), review_view_new_has_no_member_projection_or_lobby_metadata(), review_view_optional_board_is_absent_not_null(), review_view_requires_explicit_nullable_lifecycle_fields() (+5 more)

### Community 83 - "Backend Workspace Structure"
Cohesion: 0.19
Nodes (11): AccountId, String, SafeAccount, SessionKind, SessionView, AccountId, Debug, OperationId (+3 more)

### Community 84 - "Worker HTTP Boundary"
Cohesion: 0.35
Nodes (11): command(), dispatch(), failure(), handle(), json_response(), Env, Response, Result (+3 more)

### Community 85 - "Backend Workspace Structure"
Cohesion: 0.30
Nodes (11): call_accounts(), dispatch(), handle(), OwnerManagementRequest, AxumRequest, AxumResponse, CommandId, Env (+3 more)

### Community 86 - "Auth Test Coverage"
Cohesion: 0.41
Nodes (11): accepted_calls_reconcile_pending_start_and_reject_stale_real_acknowledgements(), directory_rows(), host_revision(), pending_row(), pending_start(), pending_start_fence_required_write_faults_roll_back_calls_and_exact_retries(), ConnectionId, Fixture (+3 more)

### Community 87 - "Durable SQLite Storage"
Cohesion: 0.31
Nodes (10): auth_schema_ddl_matches_the_pre_refactor_snapshot(), auth_schema_statements(), connection(), String, Vec, sqlite_account_caps_match_verifier_and_safe_integer_limits(), sqlite_rate_buckets_match_failure_count_and_digest_limits(), sqlite_receipts_match_payload_digest_and_retention_limits() (+2 more)

### Community 88 - "Account Authentication Authority"
Cohesion: 0.31
Nodes (7): AuthService<'_, D, R>, page_size(), AccountId, AuditId, Option, Result, safe()

### Community 89 - "Worker HTTP Boundary"
Cohesion: 0.31
Nodes (6): AccountCloseIdentity, AccountRejection, GamePeerError, Self, From, GameSocketCloseWork

### Community 90 - "Worker HTTP Boundary"
Cohesion: 0.24
Nodes (8): CookieWire, OwnerRequest, OwnerResponse, Option, Result, Self, String, Value

### Community 91 - "Auth Test Coverage"
Cohesion: 0.40
Nodes (9): all_rows(), receipt_rows(), receipt_update_faults_roll_back_then_original_ack_and_command_recover(), CommandId, Fixture, Row, Vec, scalar() (+1 more)

### Community 92 - "Discovery APIs"
Cohesion: 0.29
Nodes (5): directory(), get(), owner(), rows(), value()

### Community 93 - "Backend Workspace Structure"
Cohesion: 0.24
Nodes (7): ManagementClient, Prepared, Result, Self, String, Client, HeaderValue

### Community 94 - "Backend Workspace Structure"
Cohesion: 0.38
Nodes (9): execute(), prepare(), Prepared, CommandId, Option, PathBuf, Result, Write (+1 more)

### Community 95 - "Backend Workspace Structure"
Cohesion: 0.24
Nodes (8): Deserialize, CliConfig, get_cli_config(), Option, PathBuf, Result, Self, String

### Community 96 - "Backend Workspace Structure"
Cohesion: 0.29
Nodes (4): BoundedJsonSink, encode(), Vec, Write

### Community 97 - "Backend Workspace Structure"
Cohesion: 0.36
Nodes (6): AuthService, Request, Response, super::AccountsObject, OwnerDatabase, WorkerRuntime

### Community 98 - "Account Authentication Authority"
Cohesion: 0.42
Nodes (6): AuthService<'_, D, R>, fingerprint(), CommandId, Option, Result, validate_receipt()

### Community 99 - "Account Authentication Authority"
Cohesion: 0.36
Nodes (5): AuthService<'_, D, R>, OperationId, RemovalReleaseAck, Result, Vec

### Community 100 - "Worker HTTP Boundary"
Cohesion: 0.42
Nodes (7): AccountsObject, Response, Result, safe_response_headers(), Request, String, Zeroizing

### Community 102 - "Auth Test Coverage"
Cohesion: 0.50
Nodes (8): call_once(), clear_fault(), every_required_terminal_write_has_real_sqlite_ignored_and_aborted_rollback(), fault(), final_exit_ignored_and_aborted_writes_roll_back_all_access_and_history(), history_purge_faults_roll_back_every_child_route_release_intent_and_clock(), ConnectionId, Fixture

### Community 103 - "Auth Test Coverage"
Cohesion: 0.39
Nodes (8): calendar_history_deadline_is_the_same_persisted_game_and_snapshot_value(), expired_call_receipts_after_cleanup_never_readmit_old_commands_or_clock_rollback(), host_revision(), logged_in(), ConnectionId, Fixture, String, winner_commits_once_for_qualified_retained_departed_player_and_final_views_need_original_grants()

### Community 104 - "Auth Test Coverage"
Cohesion: 0.25
Nodes (5): history_queries_are_canonical_bounded_unique_and_filter_bound(), read_history(), HeaderMap, Option, Result

### Community 105 - "Shared Account Domain"
Cohesion: 0.31
Nodes (5): ValidationError, Result, validate_password(), validate_username(), validators_use_the_declared_length_boundaries()

### Community 107 - "Backend Workspace Structure"
Cohesion: 0.39
Nodes (6): Bindings, get_backend_config(), Env, Option, Result, String

### Community 108 - "Backend Workspace Structure"
Cohesion: 0.50
Nodes (6): enroll(), normalize(), push(), Result, String, Zeroizing

### Community 109 - "Durable SQLite Storage"
Cohesion: 0.33
Nodes (5): Database, metadata_clock_bounds_match_the_named_safe_integer_limit(), metadata_statement(), StorageError, String

### Community 110 - "Worker HTTP Boundary"
Cohesion: 0.62
Nodes (5): AccountsObject, management_error(), management_result(), Response, Result

### Community 111 - "Discovery APIs"
Cohesion: 0.52
Nodes (6): directory(), get(), intercept(), owner(), restore(), setup()

### Community 112 - "Backend Workspace Structure"
Cohesion: 0.29
Nodes (6): Drop, Option, Server, JoinHandle, Receiver, TempDir

### Community 113 - "Auth Test Coverage"
Cohesion: 0.33
Nodes (5): failed_get_caches_a_safe_error_without_retrying(), NeverRead, Option, Result, String

### Community 114 - "Auth Test Coverage"
Cohesion: 0.33
Nodes (5): get_initializes_lazily_and_never_acquires_again(), NeverRead, Option, Result, String

### Community 115 - "Auth Test Coverage"
Cohesion: 0.33
Nodes (5): constructor_acquires_every_declared_key_once(), Box, Error, Result, serde_deserializes_the_backend_struct_with_optional_defaults()

### Community 116 - "Shared Account Domain"
Cohesion: 0.57
Nodes (6): apply_called_value(), AssignedBoard, BoardCell, CompletedLine, PatternEvaluationError, Vec

### Community 117 - "Backend Workspace Structure"
Cohesion: 0.29
Nodes (6): BoardCellKind, GameCode, Deserialize, FromStr, Serialize, String

### Community 118 - "Discovery APIs"
Cohesion: 0.53
Nodes (4): directory(), get(), owner(), unchangedDomain()

### Community 119 - "Shared Account Domain"
Cohesion: 0.53
Nodes (6): FnMut, BoardGenerationError, bounded_index(), generate_boards(), RandomSourceError, PlayerId

### Community 120 - "Auth Test Coverage"
Cohesion: 0.40
Nodes (5): constructor_is_zero_argument_and_uses_only_public_build_inputs(), Box, Error, Result, serde_deserializes_the_public_struct_with_optional_defaults()

### Community 121 - "Backend Workspace Structure"
Cohesion: 0.33
Nodes (5): history_cells(), D, Error, Result, Vec

### Community 122 - "Account Authentication Authority"
Cohesion: 0.50
Nodes (3): AuthService<'_, D, R>, Option, Result

### Community 123 - "Backend Workspace Structure"
Cohesion: 0.40
Nodes (4): GameDirectoryObject, Request, Response, Result

### Community 124 - "Auth Test Coverage"
Cohesion: 0.60
Nodes (4): migrated_prestart(), migrated_prestart_counter_retirement_ignored_or_aborted_delete_rolls_back_clock_and_release(), migrated_prestart_idle_cancellation_retires_obsolete_counter_in_real_maintenance(), Fixture

### Community 125 - "Backend Workspace Structure"
Cohesion: 0.70
Nodes (4): Prepared, Result, validate_receipt(), validate_response()

### Community 126 - "Auth Test Coverage"
Cohesion: 0.70
Nodes (4): enrollment_url(), link_token(), String, Zeroizing

### Community 127 - "Typed Environment Configuration"
Cohesion: 0.70
Nodes (4): deserialize_error(), missing_values_only_identify_known_keys(), Error, third_party_deserialization_errors_are_discarded()

### Community 129 - "Backend Workspace Structure"
Cohesion: 0.50
Nodes (3): prepare(), Prepared, Result

### Community 130 - "Backend Workspace Structure"
Cohesion: 0.50
Nodes (3): prepare(), Prepared, Result

### Community 132 - "Typed Environment Configuration"
Cohesion: 0.50
Nodes (3): Result, String, validate_origin()

## Knowledge Gaps
- **9 isolated node(s):** `AuthService<'_, D, R>`, `GameObject`, `ConfigurationOverrides`, `SnapshotKind`, `SnapshotAckKind` (+4 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **95 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `Record` connect `Durable SQLite Storage` to `Durable SQLite Storage`, `Durable SQLite Storage`, `Durable SQLite Storage`, `Backend Workspace Structure`?**
  _High betweenness centrality (0.000) - this node is a cross-community bridge._
- **Why does `PendingWork` connect `Backend Workspace Structure` to `Auth Test Coverage`?**
  _High betweenness centrality (0.000) - this node is a cross-community bridge._
- **What connects `AuthService<'_, D, R>`, `GameObject`, `ConfigurationOverrides` to the rest of the system?**
  _9 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Auth Test Coverage` be split into smaller, more focused modules?**
  _Cohesion score 0.06574923547400612 - nodes in this community are weakly interconnected._
- **Should `Auth Test Coverage` be split into smaller, more focused modules?**
  _Cohesion score 0.07715355805243446 - nodes in this community are weakly interconnected._
- **Should `Auth Test Coverage` be split into smaller, more focused modules?**
  _Cohesion score 0.08924050632911393 - nodes in this community are weakly interconnected._
- **Should `Auth Test Coverage` be split into smaller, more focused modules?**
  _Cohesion score 0.08954203691045796 - nodes in this community are weakly interconnected._