# Scope: code-only structural graph

Rust/JavaScript/Python source and tests only. No semantic documentation refresh; the older graph in the parent directory is not marked current. No LLM token cost for this extraction.

AST health warnings: {"dangling_endpoint_edges": 507, "directed_same_endpoint_collapsed_edges": 211}. Unresolved references are not verified relationships; structural-extraction.json retains raw edge evidence.

# Graph Report - brews-bingo  (2026-10-08)

## Corpus Check
- 161 files · ~151,837 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 3596 nodes · 9245 edges · 135 communities (119 shown, 16 thin omitted)
- Extraction: 96% EXTRACTED · 4% INFERRED · 0% AMBIGUOUS · INFERRED: 386 edges (avg confidence: 0.85)
- Token cost: 0 input · 0 output

## Community Hubs (Navigation)
- Durable SQLite Storage
- Auth Test Coverage
- Auth Test Coverage
- Worker HTTP Boundary
- Auth Test Coverage
- Shared Account Domain
- Backend Workspace Structure
- Durable SQLite Storage
- Auth Test Coverage
- Account Authentication Authority
- Auth Test Coverage
- Worker HTTP Boundary
- Worker HTTP Boundary
- Worker HTTP Boundary
- Worker HTTP Boundary
- Durable SQLite Storage
- Durable SQLite Storage
- Durable SQLite Storage
- Auth Test Coverage
- Shared Account Domain
- Auth Test Coverage
- Account Authentication Authority
- Durable SQLite Storage
- Durable SQLite Storage
- Worker HTTP Boundary
- Worker HTTP Boundary
- Shared Account Domain
- Backend Workspace Structure
- Worker HTTP Boundary
- Auth Test Coverage
- Account Authentication Authority
- Backend Workspace Structure
- Auth Test Coverage
- Durable SQLite Storage
- Backend Workspace Structure
- Worker HTTP Boundary
- Durable SQLite Storage
- Durable SQLite Storage
- Account Authentication Authority
- Worker HTTP Boundary
- Account Authentication Authority
- Worker HTTP Boundary
- Shared Account Domain
- Account Authentication Authority
- Account Authentication Authority
- Durable SQLite Storage
- Account Authentication Authority
- Shared Account Domain
- Shared Account Domain
- Backend Workspace Structure
- Account Authentication Authority
- Backend Workspace Structure
- Typed Environment Configuration
- Credential Cryptography
- Auth Test Coverage
- Shared Account Domain
- Shared Account Domain
- Backend Workspace Structure
- Worker HTTP Boundary
- Worker HTTP Boundary
- Worker HTTP Boundary
- Backend Workspace Structure
- Typed Environment Configuration
- Shared Account Domain
- Worker HTTP Boundary
- Durable SQLite Storage
- Backend Workspace Structure
- Durable SQLite Storage
- Worker HTTP Boundary
- Backend Workspace Structure
- Worker HTTP Boundary
- Auth Test Coverage
- Account Authentication Authority
- Typed Environment Configuration
- Shared Account Domain
- Auth Test Coverage
- Durable SQLite Storage
- Backend Workspace Structure
- Auth Test Coverage
- Auth Test Coverage
- Worker HTTP Boundary
- Worker HTTP Boundary
- Shared Account Domain
- Backend Workspace Structure
- Shared Account Domain
- Worker HTTP Boundary
- Backend Workspace Structure
- Auth Test Coverage
- Typed Environment Configuration
- Durable SQLite Storage
- Backend Workspace Structure
- Account Authentication Authority
- Worker HTTP Boundary
- Durable SQLite Storage
- Account Authentication Authority
- Account Authentication Authority
- Durable SQLite Storage
- Backend Workspace Structure
- Backend Workspace Structure
- Shared Account Domain
- Shared Account Domain
- Account Authentication Authority
- Durable SQLite Storage
- Backend Workspace Structure
- Auth Test Coverage
- Typed Environment Configuration
- Worker HTTP Boundary
- Worker HTTP Boundary
- Worker HTTP Boundary
- Auth Test Coverage
- Worker HTTP Boundary
- Backend Workspace Structure
- Backend Workspace Structure
- Worker HTTP Boundary
- Worker HTTP Boundary
- Worker HTTP Boundary
- Backend Workspace Structure
- Backend Workspace Structure
- Typed Environment Configuration
- Shared Account Domain
- Auth Test Coverage
- Backend Workspace Structure
- Shared Account Domain
- Shared Account Domain
- Backend Workspace Structure
- Durable SQLite Storage
- Shared Account Domain
- Shared Account Domain

## God Nodes (most connected - your core abstractions)
1. `GameError` - 186 edges
2. `migrate_directory()` - 123 edges
3. `migrate()` - 97 edges
4. `AuthError` - 89 edges
5. `DirectoryError` - 86 edges
6. `GameService<'a, D, R>` - 80 edges
7. `ManagementError` - 74 edges
8. `operation()` - 63 edges
9. `account()` - 56 edges
10. `GameAccountAuthority` - 52 edges

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

## Communities (135 total, 16 thin omitted)

### Community 0 - "Durable SQLite Storage"
Cohesion: 0.01
Nodes (189): CELL_FREE, CELL_VALUE, GAME_DEADLINE_SOURCE_COUNT, HISTORY_RETENTION_CALENDAR_MONTHS, LINE_ANTI, LINE_COLUMN, LINE_MAIN, LINE_ROW (+181 more)

### Community 1 - "Auth Test Coverage"
Cohesion: 0.05
Nodes (129): migrate_directory(), account(), acquire_persists_a_presence_only_gate_with_separate_operation_identity(), acquired_gate_rejects_transfer_without_changing_the_original_host(), competing_operation_returns_busy_without_replacing_the_current_gate(), completed_operation_cannot_reacquire_or_clear_a_newer_gate(), concurrent_operations_on_one_account_serialize_to_one_grant_and_one_busy(), corrupted_completion_time_cannot_erase_the_fence_or_authorize_an_ack() (+121 more)

### Community 2 - "Auth Test Coverage"
Cohesion: 0.06
Nodes (92): migrate(), a_current_host_session_is_attributed_when_admin_authority_is_denied(), account_detail_is_safe_and_deleted_or_unknown_target_is_not_found(), account_pages_are_bounded_and_continue_without_duplicates(), audit_write_failure_rolls_back_account_link_and_receipt(), authenticated_rejections_are_durably_audited(), command_id(), create() (+84 more)

### Community 3 - "Worker HTTP Boundary"
Cohesion: 0.06
Nodes (31): GameService, OwnerDatabase, close_checked(), GameObject, peer_error(), private_response(), RegistryAvailability, Available (+23 more)

### Community 4 - "Auth Test Coverage"
Cohesion: 0.08
Nodes (48): directoryStub(), initializeDirectory(), commandId(), cookie(), current(), enroll(), inOwner(), origin (+40 more)

### Community 5 - "Shared Account Domain"
Cohesion: 0.05
Nodes (60): admission_and_join_validate_lookup_alias_and_never_debug_recovery_material(), assert_response_rejected(), BASE64URL, created_completion_can_be_later_than_creation_without_moving_idle_deadline(), creation_resolves_defaults_and_overrides_with_effective_center(), DELIVERY_ID_BYTES, DELIVERY_ID_ENCODED_BYTES, GAME_BODY_MAX_BYTES (+52 more)

### Community 6 - "Backend Workspace Structure"
Cohesion: 0.05
Nodes (20): CodeGrant, CREATION_FINGERPRINT_BYTES, CREATION_LIFETIME_MS, CREATION_RETRY_INITIAL_MS, CREATION_RETRY_MAX_MS, CreationAck, CreationFingerprint, CreationReadyProof (+12 more)

### Community 7 - "Durable SQLite Storage"
Cohesion: 0.07
Nodes (47): binding_params(), DELIVERY_CLEANUP_BATCH_SIZE, DELIVERY_ID_GENERATION_MAX_ATTEMPTS, DeliveryBinding, DeliveryBudget, DeliveryError, BindingMismatch, Capacity (+39 more)

### Community 8 - "Auth Test Coverage"
Cohesion: 0.05
Nodes (47): account_exit_receipt_scalar_faults_roll_back_then_original_command_retries(), lobby_cancel_receipt_scalar_faults_roll_back_then_original_command_retries(), manual_call_receipt_scalar_faults_roll_back_then_original_command_retries(), new_cancel_receipt_scalar_faults_roll_back_then_original_command_retries(), Operation, AccountExit, LobbyCancel, ManualCall (+39 more)

### Community 9 - "Account Authentication Authority"
Cohesion: 0.05
Nodes (27): AuthService<'_, D, R>, AuthService<'_, D, R>, AuthService<'_, D, R>, AuthService<'_, D, R>, AuthService<'_, D, R>, deadline(), ManagementError, Busy (+19 more)

### Community 10 - "Auth Test Coverage"
Cohesion: 0.14
Nodes (41): creation_fingerprint(), SecretCookie, account_socket_grants_require_fresh_accounts_and_do_not_renew_expiry_or_presence(), actual_sqlite_reopen_preserves_lobby_intent_and_lost_publication_ack(), admission_is_verifier_only_short_lived_and_consumed_retries_never_mint_cookies(), answer_kdf_runs_outside_sqlite_write_and_rechecks_real_competing_admission(), command_receipts_use_variant_scoped_account_and_player_actor_namespaces(), cookie() (+33 more)

### Community 11 - "Worker HTTP Boundary"
Cohesion: 0.06
Nodes (45): ApiError, Forbidden, InvalidInput, MethodNotAllowed, NotFound, PayloadTooLarge, Unavailable, UnsupportedMediaType (+37 more)

### Community 12 - "Worker HTTP Boundary"
Cohesion: 0.06
Nodes (37): checked_revision(), directory_claim_ack_and_code_use_real_owner_core_and_exact_binding(), directory_publication_recovery_and_release_use_real_core(), DirectoryGameOutcome, Acknowledged, Code, CodeAllocated, Due (+29 more)

### Community 13 - "Worker HTTP Boundary"
Cohesion: 0.10
Nodes (36): absent_after_deadline_keeps_reservation_and_saturating_retry(), command(), Fixture, genuine_committed_lobby_recovers_directory_publication_without_faking_game_ack(), genuine_initializer_evidence_acknowledges_only_current_unready_creation(), genuine_new_cancellation_releases_only_the_hidden_directory_association(), genuine_owner_terminal_evidence_releases_reservation_after_deadline(), hidden_code_release_preserves_exact_terminal_identity_and_publication_fences() (+28 more)

### Community 14 - "Worker HTTP Boundary"
Cohesion: 0.07
Nodes (46): admission_context_is_keyless_and_join_uses_separate_game_scoped_cookie(), ADMISSION_COOKIE_NAME, admission_forwards_an_existing_player_cookie_without_claiming_authority(), all_transport_inputs_have_release_active_finite_bounds(), command_id(), creation_route_decodes_original_configuration_without_caller_identity(), decode_empty_game_body(), decode_games() (+38 more)

### Community 15 - "Durable SQLite Storage"
Cohesion: 0.09
Nodes (22): bounded_random_index(), choose_remaining_value(), PlayerSession, Record, StoredGameCall, GameService<'_, D, R>, TerminalCommit, GameError (+14 more)

### Community 16 - "Durable SQLite Storage"
Cohesion: 0.10
Nodes (5): GameAccountAuthority, authorize(), text(), AccountConnectionGrant, ConnectionGrant

### Community 17 - "Durable SQLite Storage"
Cohesion: 0.13
Nodes (21): account_actor(), add_time(), Admission, advance(), authorize_mutation(), ensure_before_idle(), join_fingerprint(), line_parts() (+13 more)

### Community 18 - "Auth Test Coverage"
Cohesion: 0.07
Nodes (32): main(), assert_issued_handoff(), audit_reads_preserve_required_targets_and_read_operations(), authorization_debug_is_redacted_and_server_errors_discard_bodies(), BrokenPipe, ca_file_read_is_bounded_and_errors_are_redacted(), client_rejects_noncanonical_or_non_https_origins_before_network(), command_runner_exercises_real_https_to_safe_metadata() (+24 more)

### Community 19 - "Shared Account Domain"
Cohesion: 0.10
Nodes (19): CallView, checked_deadline(), ConfigurationOverrides, decode(), encode(), GameDecodeError, same_winner(), SnapshotAck (+11 more)

### Community 20 - "Auth Test Coverage"
Cohesion: 0.14
Nodes (36): bearer(), clock_rollback_and_unavailable_storage_never_release_authority(), close_pages_are_bounded_stably_ordered_and_do_not_discard_backlog(), close_retry_is_durable_capped_and_saturates_count_and_deadline(), connection(), context(), current_normal_host_and_admin_produce_private_game_authority(), due_close_work_is_bounded_and_preserves_exact_revoked_binding() (+28 more)

### Community 21 - "Account Authentication Authority"
Cohesion: 0.12
Nodes (5): AuthService<'_, D, R>, GameSocketCloseWork, removal_finishes_only_after_all_exact_socket_close_acknowledgements(), Sqlite, subscription_values()

### Community 22 - "Durable SQLite Storage"
Cohesion: 0.13
Nodes (20): DirectoryService<'a, D, R>, operation_timestamp(), parse_completion_row(), parse_operation_row(), DirectoryError, AssignmentBlocked, Busy, Clock (+12 more)

### Community 23 - "Durable SQLite Storage"
Cohesion: 0.05
Nodes (42): EXPIRED_HISTORY_PURGE, FINAL_GRANT_LIMIT, SQL_DELETE_FINAL_PLAYER_ACCESS, SQL_DELETE_FINAL_PLAYER_GRANTS, SQL_DELETE_FINAL_PLAYER_SESSION, SQL_DELETE_FINAL_PRINCIPAL_CONNECTION_GRANTS, SQL_DELETE_FINAL_PRINCIPAL_CONNECTIONS, SQL_DELETE_FINAL_VIEW_COUNTER (+34 more)

### Community 24 - "Worker HTTP Boundary"
Cohesion: 0.07
Nodes (36): all_seven_operations_accept_their_exact_payload(), assert_array_root_rejected(), BODY_LIMIT, CompleteBody, COOKIE_HEADER_MAX_BYTES, cookie_is_read_and_duplicate_session_values_reject(), decode(), Decoded (+28 more)

### Community 25 - "Worker HTTP Boundary"
Cohesion: 0.07
Nodes (21): DELIVERY_TABLES, GAME_SCHEMA_VERSION, GAME_TABLES, PREVIOUS_GAME_SCHEMA_VERSION, PREVIOUS_GAME_TABLES, call_success_and_receipt_only_retry_match_exact_manual_request(), exit_success_and_receipt_retry_are_bound_only_to_selected_exit_command(), GAME_WIRE_MAX_BYTES (+13 more)

### Community 26 - "Shared Account Domain"
Cohesion: 0.07
Nodes (30): account_fields_reject_positional_sequences(), ACCOUNT_OBJECT, ACCOUNT_SEQUENCE, assert_invalid_response(), CreateUsername, deserialize_object(), deserialize_users(), every_public_variant_round_trips_with_safe_debug_and_http_status() (+22 more)

### Community 27 - "Backend Workspace Structure"
Cohesion: 0.09
Nodes (6): report_rehash_failure(), OwnerManagementRequest, ENROLLMENT_PATH, LINK_TOKEN_BYTES, LINK_TOKEN_ENCODED_BYTES, PASSWORD_RESET_PATH

### Community 28 - "Worker HTTP Boundary"
Cohesion: 0.16
Nodes (29): account_reply(), acknowledge_creation(), allocate_game_code(), authorize_account(), authorize_account_connection(), call(), call_game_close(), claim_game() (+21 more)

### Community 29 - "Auth Test Coverage"
Cohesion: 0.08
Nodes (17): ConfigError, Deserialization, InvalidValue, MissingKey, validate_origin(), failed_get_caches_a_safe_error_without_retrying(), NeverRead, get_initializes_lazily_and_never_acquires_again() (+9 more)

### Community 30 - "Account Authentication Authority"
Cohesion: 0.11
Nodes (17): AuthError, Conflict, Crypto, InvalidCredentials, InvalidInput, InvalidLink, RateLimited, StaleCommand (+9 more)

### Community 31 - "Backend Workspace Structure"
Cohesion: 0.06
Nodes (31): AUDIT_RETENTION_MS, id_check(), LINK_RECEIPT_RETENTION_MS, management_schema_statements(), REMOVAL_INITIAL_DELAY_MS, REMOVAL_MAX_DELAY_MS, ACCESS_LINK_LIFETIME_MS, ACCESS_LINK_METADATA_RETENTION_MS (+23 more)

### Community 32 - "Auth Test Coverage"
Cohesion: 0.18
Nodes (22): new_token(), command_id(), count(), create(), Fixture, ORIGIN, seed(), session() (+14 more)

### Community 33 - "Durable SQLite Storage"
Cohesion: 0.07
Nodes (9): StorageError, schema_statement_for(), add_game_coordination(), DIRECTORY_SCHEMA_VERSION, DIRECTORY_TABLES, initialize_removal(), REMOVAL_SCHEMA_VERSION, REMOVAL_TABLES (+1 more)

### Community 34 - "Backend Workspace Structure"
Cohesion: 0.06
Nodes (25): ACCOUNT_KIND, ADMISSION_LIMIT, ADMISSION_MS, HOST_IDLE_MS, HOST_IDLE_REASON, HOST_VIEW, LIVE_ACCESS, LobbyPreparation (+17 more)

### Community 35 - "Worker HTTP Boundary"
Cohesion: 0.12
Nodes (23): account_registration_and_post_gate_recheck_use_real_accounts_core(), AccountAction, Authorize, AuthorizeConnection, Register, Unregister, AccountReply, AccountRequest (+15 more)

### Community 36 - "Durable SQLite Storage"
Cohesion: 0.21
Nodes (6): DirectoryService<'_, D, R>, integer(), optional_integer(), parse_index(), text(), validate_projection()

### Community 38 - "Account Authentication Authority"
Cohesion: 0.21
Nodes (6): AuthOutcome, RequestContext, add_deadline(), AuthService<'a, D, R>, token_digest(), validate_password()

### Community 39 - "Worker HTTP Boundary"
Cohesion: 0.15
Nodes (8): AccountsObject, management_error(), management_result(), AccountsObject, fetch(), AccountsObject, safe_response_headers(), Request

### Community 40 - "Account Authentication Authority"
Cohesion: 0.17
Nodes (12): actor_key(), AuthService<'_, D, R>, parse_work(), RemovalGateGrant, RemovalPhase, Aborting, Committed, Prepared (+4 more)

### Community 41 - "Worker HTTP Boundary"
Cohesion: 0.08
Nodes (26): GameOperation, AdmissionContext, CallManual, CallRandom, Cancel, Create, Exit, JoinPlayer (+18 more)

### Community 42 - "Shared Account Domain"
Cohesion: 0.10
Nodes (16): action(), AuditEvent, AuditTarget, Account, AccountsOwner, deserialize(), json_decode_errors_discard_values_and_parser_sources(), ManagementResponse (+8 more)

### Community 43 - "Account Authentication Authority"
Cohesion: 0.12
Nodes (12): ManagementPrincipal, AdminSession, DeveloperCli, AuthService<'_, D, R>, changes(), final_release_reproves_live_admin_without_writes_or_audit(), fixture(), id() (+4 more)

### Community 44 - "Account Authentication Authority"
Cohesion: 0.33
Nodes (20): admin_session(), app_admin_self_disable_delete_and_enable_are_forbidden(), command_id(), committed_removal_phase_is_bound_and_persisted_in_real_sqlite(), deletion_does_not_require_an_unused_credential_epoch_increment(), deletion_requires_matching_gate_and_retains_intent_until_release_ack(), developer_cli_cannot_remove_the_last_enabled_verified_admin(), disable_changes_epoch_once_and_repeated_no_op_keeps_timestamp() (+12 more)

### Community 45 - "Durable SQLite Storage"
Cohesion: 0.16
Nodes (20): account_acceptance_preflight_rejects_fenced_or_replaced_preparations_without_accepting(), authority(), call_fingerprint(), close_work(), command_fingerprint(), forward_schema_migration_accepts_legitimate_prestart_idle_cancellation_and_pending_release(), forward_schema_migration_preserves_existing_game_configuration_and_board(), gameplay_schema_supports_resolved_games_and_unique_ordered_calls() (+12 more)

### Community 46 - "Account Authentication Authority"
Cohesion: 0.09
Nodes (14): AuthCommand, Complete, Current, Login, Logout, Redeem, CookieWire, Clear (+6 more)

### Community 47 - "Shared Account Domain"
Cohesion: 0.10
Nodes (18): AuthService<'_, D, R>, fingerprint(), validate_receipt(), list(), ManagementCommand, CreateAccount, DeleteAccount, DisableAccount (+10 more)

### Community 48 - "Shared Account Domain"
Cohesion: 0.10
Nodes (17): Account, Session, AccountStatus, PendingEnrollment, ResetRequired, Verified, ValidationError, PASSWORD_MAX_LEN (+9 more)

### Community 49 - "Backend Workspace Structure"
Cohesion: 0.12
Nodes (4): call(), check(), join(), SocketClient

### Community 50 - "Account Authentication Authority"
Cohesion: 0.10
Nodes (8): value(), Sqlite, int(), SqlValue, Blob, Integer, Null, Text

### Community 51 - "Backend Workspace Structure"
Cohesion: 0.10
Nodes (11): AuditCommand, List, AuditPage, canonical_id(), Cli, DEFAULT_PAGE_ENTRIES, prepares_exact_operation_and_command_identity_once(), RootCommand (+3 more)

### Community 52 - "Typed Environment Configuration"
Cohesion: 0.09
Nodes (18): APP_ORIGIN, APP_ORIGIN_MAX_BYTES, ARGON2_DEFAULT_ITERATIONS, ARGON2_DEFAULT_MEMORY_KIB, ARGON2_M_COST, ARGON2_MAX_ITERATIONS, ARGON2_MAX_MEMORY_KIB, ARGON2_MIN_ITERATIONS (+10 more)

### Community 53 - "Credential Cryptography"
Cohesion: 0.11
Nodes (17): AuthPolicy, bounded_phc(), DIGEST_BYTES, hash_password(), MAX_PHC_LEN, PASSWORD_HASH_BYTES, PASSWORD_SALT_BYTES, PHC_PARAMETER_COUNT (+9 more)

### Community 54 - "Auth Test Coverage"
Cohesion: 0.20
Nodes (11): assert_verifier_rehashed(), assertion_failure_diagnostics_do_not_disclose_verifiers(), command_id(), context(), cookie(), create(), enrollment_and_reset_bind_status_purpose_and_scope_without_changing_stored_tags(), ORIGIN (+3 more)

### Community 55 - "Shared Account Domain"
Cohesion: 0.18
Nodes (16): call_sequence(), call_value(), canonical_alias(), cell_kind(), cells(), completed_lines(), configuration(), lookup_code() (+8 more)

### Community 56 - "Shared Account Domain"
Cohesion: 0.10
Nodes (15): ALIAS_MAX_BYTES, BOARD_CELL_KIND_FREE, BOARD_CELL_KIND_VALUE, BOARD_CELLS_MAX, BOARD_SIDE_MAX, BOARD_SIDE_MIN, complete_configuration_enforces_approved_ranges_free_positions_and_pool(), feasibility_counts_entire_roster_with_capped_falling_factorial() (+7 more)

### Community 57 - "Backend Workspace Structure"
Cohesion: 0.12
Nodes (6): authenticate_cli(), CliRequest, decode_cli(), history_queries_are_canonical_bounded_unique_and_filter_bound(), ORIGIN, read_history()

### Community 58 - "Worker HTTP Boundary"
Cohesion: 0.15
Nodes (9): DirectoryRequest, Acquire, Reconcile, Release, DirectoryResponse, Acquired, Rejected, Released (+1 more)

### Community 59 - "Worker HTTP Boundary"
Cohesion: 0.16
Nodes (14): GameRejection, Conflict, Forbidden, InvalidInput, NotFound, StaleCommand, Unauthorized, Unavailable (+6 more)

### Community 60 - "Worker HTTP Boundary"
Cohesion: 0.14
Nodes (8): deserialize_error_fields(), dispatch(), ErrorEnvelope, ErrorFields, handle(), log_delivery(), OwnerUsersRequest, validate_error()

### Community 61 - "Backend Workspace Structure"
Cohesion: 0.11
Nodes (13): ManagementClient, CliError, Configuration, Confirmation, HttpStatus, InvalidInput, LinkFile, Output (+5 more)

### Community 62 - "Typed Environment Configuration"
Cohesion: 0.16
Nodes (15): absent_public_settings_use_serde_defaults(), API_PATH, assert_redacted(), CONFIG, explicit_public_values_are_preserved(), FRONTEND_KEYS, get_frontend_config(), maximum_display_name_is_measured_in_utf8_bytes() (+7 more)

### Community 63 - "Shared Account Domain"
Cohesion: 0.11
Nodes (19): operation_tag(), replay_guidance(), AuditOperation, CreateAccount, DeleteAccount, DisableAccount, EnableAccount, GetAccount (+11 more)

### Community 64 - "Worker HTTP Boundary"
Cohesion: 0.22
Nodes (12): confirm_expiry_denial(), current(), dispatch(), failure(), handle(), HistoryError, Transport, Unauthorized (+4 more)

### Community 65 - "Durable SQLite Storage"
Cohesion: 0.15
Nodes (10): ACCOUNT_ID, auth_schema_ddl_matches_the_pre_refactor_snapshot(), auth_schema_statements(), connection(), ENTITY_ID, sqlite_account_caps_match_verifier_and_safe_integer_limits(), sqlite_rate_buckets_match_failure_count_and_digest_limits(), sqlite_receipts_match_payload_digest_and_retention_limits() (+2 more)

### Community 66 - "Backend Workspace Structure"
Cohesion: 0.18
Nodes (7): command(), dispatch(), failure(), handle(), json_response(), sdk_error(), decode_private_json()

### Community 67 - "Durable SQLite Storage"
Cohesion: 0.30
Nodes (11): blob(), number(), optional_number(), optional_text(), parse_connection(), parse_game_call(), parse_line(), parse_work() (+3 more)

### Community 68 - "Worker HTTP Boundary"
Cohesion: 0.19
Nodes (11): decode_history(), filter_tag(), HISTORY_QUERY_MAX_BYTES, HISTORY_QUERY_MAX_FIELDS, HistoryCommand, Detail, List, HistoryCursor (+3 more)

### Community 69 - "Backend Workspace Structure"
Cohesion: 0.11
Nodes (16): Failure, Configuration, Conflict, Crypto, Forbidden, InvalidCredentials, InvalidInput, InvalidLink (+8 more)

### Community 70 - "Worker HTTP Boundary"
Cohesion: 0.20
Nodes (14): HISTORY_REPLY_MAX_BYTES, history_summary_rejects_alias_normalization_instead_of_repairing_owner_bytes(), HistoryOwnerOutcome, Detail, NotFound, Summary, Unauthorized, Unavailable (+6 more)

### Community 71 - "Auth Test Coverage"
Cohesion: 0.14
Nodes (3): enrollment_url(), link_token(), NativeSource

### Community 72 - "Account Authentication Authority"
Cohesion: 0.22
Nodes (8): access_link_cleanup_and_alarm_share_the_exact_metadata_retention_deadline(), ACCOUNT_ID, cleanup_limits_each_delete_and_expired_close_enqueue_to_the_named_batch_size(), entity_id(), seed_account(), sql_params(), Sqlite, TestRuntime

### Community 73 - "Typed Environment Configuration"
Cohesion: 0.18
Nodes (8): Bindings, get_backend_config(), BackendConfig, get_backend_config(), deserialize_error(), EnvSource, missing_values_only_identify_known_keys(), third_party_deserialization_errors_are_discarded()

### Community 74 - "Shared Account Domain"
Cohesion: 0.13
Nodes (5): CreateGame, DeliveryId, JoinPlayer, recovery_answer_explicit_zeroization_clears_owned_sensitive_bytes(), RecoveryAnswer

### Community 75 - "Auth Test Coverage"
Cohesion: 0.12
Nodes (5): TestRuntime, Runtime, ByteRuntime, ScriptRuntime, TestRuntime

### Community 76 - "Durable SQLite Storage"
Cohesion: 0.23
Nodes (10): command_timestamp(), CREATION_COLUMNS, deadline(), HISTORY_PAGE_MAX, INDEX_COLUMNS, parse_creation(), parse_optional_integer(), parse_retired() (+2 more)

### Community 77 - "Backend Workspace Structure"
Cohesion: 0.14
Nodes (13): AccountCommand, Create, Delete, Disable, Enable, Get, List, ReissueEnrollment (+5 more)

### Community 78 - "Auth Test Coverage"
Cohesion: 0.15
Nodes (9): acquisition_can_only_request_the_declared_inventory(), assert_redacted(), dev_cli_key_is_optional_but_supplied_values_are_validated(), explicit_argon2_costs_are_deserialized_as_numbers(), keys_require_canonical_unpadded_base64url_for_exactly_32_bytes(), unsafe_or_noncanonical_origins_fail_closed(), unusable_supplied_numeric_values_never_become_defaults_or_leak(), valid_origins_are_stored_without_an_optional_root_slash() (+1 more)

### Community 79 - "Auth Test Coverage"
Cohesion: 0.20
Nodes (7): value(), additional_terminal_required_writes_ignore_abort_rollback_and_exact_retry(), all_call_write_faults_rollback_then_original_command_retries(), host_revision(), random_call_unbiased_rejection_chooses_remaining_index_and_replay_uses_no_entropy(), started_with_cleanup_sources(), value()

### Community 80 - "Worker HTTP Boundary"
Cohesion: 0.20
Nodes (7): AccountAuthorityWire, AccountCloseIdentity, AccountConnectionIdentity, AccountOutcome, Authorized, Rejected, Unregistered

### Community 81 - "Worker HTTP Boundary"
Cohesion: 0.16
Nodes (11): AccountCloseReply, AccountCloseRequest, CloseAction, CloseAccount, CloseCaller, Accounts, CloseResult, Closed (+3 more)

### Community 82 - "Shared Account Domain"
Cohesion: 0.20
Nodes (12): history_cells(), accepted_value_matches_all_cells_once_and_recomputes_single_line(), apply_called_value(), BoardCell, CellPosition, CompletedLine, AntiDiagonal, Column (+4 more)

### Community 83 - "Backend Workspace Structure"
Cohesion: 0.30
Nodes (7): failure_cleanup_truncates_only_owned_handle_not_replaced_path(), LinkFile, persists_link_without_displaying_or_reopening_path(), replaced_path_is_rejected_before_secret_write(), reserves_new_private_file_before_network(), weakened_permissions_or_hardlinks_fail_before_secret_write(), write_failure_is_typed_without_secret_sources()

### Community 84 - "Shared Account Domain"
Cohesion: 0.25
Nodes (14): assert_view_rejected(), hardened_tagged_maps_preserve_original_duplicates_and_closed_fields(), host_new_view(), player_lobby_view(), review_view_awaiting_requires_pre_start_lifecycle_but_allows_hidden_host_code(), review_view_in_progress_requires_code_and_start_without_end_or_idle(), review_view_new_has_no_member_projection_or_lobby_metadata(), review_view_optional_board_is_absent_not_null() (+6 more)

### Community 85 - "Worker HTTP Boundary"
Cohesion: 0.18
Nodes (5): ATTACHMENT_MAX_BYTES, delivery_error(), SIZE_PLACEHOLDER, SOCKET_LIMIT, WEB_CRYPTO_RANDOM_MAX_BYTES

### Community 86 - "Backend Workspace Structure"
Cohesion: 0.15
Nodes (12): Boundary, AccountsAlarm, AccountsAuth, AccountsManagement, AccountsPeer, AuthIngress, CliIngress, DirectoryAlarm (+4 more)

### Community 87 - "Auth Test Coverage"
Cohesion: 0.23
Nodes (6): fresh_history_login(), history_at_last_retained_millisecond(), history_final_response_expiry_fence_faults_roll_back_all_rows(), history_final_response_expiry_fence_survives_rollback_and_file_reopen(), history_final_response_expiry_read_error_rolls_back_required_write(), immutable_history_survives_actual_file_reopen_after_final_grants_exit()

### Community 88 - "Typed Environment Configuration"
Cohesion: 0.19
Nodes (8): BREWS_API_ORIGIN, BREWS_DEV_CLI_KEY, BREWS_TLS_CA_FILE, CLI_KEYS, CLI_ORIGIN_MAX_BYTES, CliConfig, CONFIG, get_cli_config()

### Community 89 - "Durable SQLite Storage"
Cohesion: 0.17
Nodes (4): DELIVERY_PENDING_MAX, DELIVERY_QUEUE_MAX_BYTES, DELIVERY_SCHEMA_VERSION, DELIVERY_TABLES

### Community 90 - "Backend Workspace Structure"
Cohesion: 0.21
Nodes (6): ANSWER_CANONICAL_BYTES, ANSWER_INPUT_BYTES, enroll(), NORMALIZATION_VERSION, normalize(), push()

### Community 91 - "Account Authentication Authority"
Cohesion: 0.21
Nodes (5): AuthService<'_, D, R>, DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE, page_size(), safe()

### Community 93 - "Durable SQLite Storage"
Cohesion: 0.29
Nodes (7): application_tables(), DirectoryService, MINIFLARE_METADATA_TABLE, schema_version(), validate_inventory(), validate_schema(), WORKERD_METADATA_TABLE

### Community 94 - "Account Authentication Authority"
Cohesion: 0.18
Nodes (4): CookieEffect, Clear, None, Set

### Community 95 - "Account Authentication Authority"
Cohesion: 0.20
Nodes (7): CommandOutcome, CompleteEnrollment, CompletePasswordReset, Login, Redeem, fingerprint(), StoredOutcome

### Community 96 - "Durable SQLite Storage"
Cohesion: 0.22
Nodes (5): AUTH_SCHEMA_VERSION, CURRENT_SCHEMA_VERSION, Database, metadata_clock_bounds_match_the_named_safe_integer_limit(), metadata_statement()

### Community 97 - "Backend Workspace Structure"
Cohesion: 0.20
Nodes (6): api_failure(), auth_failure(), init(), management_failure(), subscriber(), TARGET

### Community 98 - "Backend Workspace Structure"
Cohesion: 0.36
Nodes (8): human_and_json_account_output_escape_all_controls(), issued_output_projects_metadata_without_link_secrets(), terminal_text(), terminal_text_escapes_username_controls(), write_error(), write_metadata(), write_notice(), write_response()

### Community 99 - "Shared Account Domain"
Cohesion: 0.24
Nodes (10): AssignedBoard, BoardGenerationError, CandidateBudgetExhausted, DuplicatePlayer, InvalidConfiguration, RandomUnavailable, RejectionBudgetExhausted, bounded_index() (+2 more)

### Community 101 - "Account Authentication Authority"
Cohesion: 0.22
Nodes (6): command(), SOCKET_CLOSE_BACKOFF_MAX_SHIFT, SOCKET_CLOSE_MAX_DELAY_MS, TestRuntime, trusted_close_metadata_preserves_each_identity_without_granting_account_authority(), trusted_peer_metadata_reconstruction_is_range_checked()

### Community 103 - "Backend Workspace Structure"
Cohesion: 0.22
Nodes (8): Delivery, Committed, Issued, Pending, management_delivery(), privileged_projection_omits_sensitive_spans_and_unrelated_events(), removal(), typed_lifecycle_records_use_closed_fields_and_canonical_ids()

### Community 104 - "Auth Test Coverage"
Cohesion: 0.47
Nodes (7): accepted_calls_reconcile_pending_start_and_reject_stale_real_acknowledgements(), directory_rows(), host_revision(), pending_row(), pending_start(), pending_start_fence_required_write_faults_roll_back_calls_and_exact_retries(), start_receipt_row()

### Community 105 - "Typed Environment Configuration"
Cohesion: 0.24
Nodes (3): SECRET_KEY_BYTES, SECRET_KEY_ENCODED_LEN, SecretKey

### Community 106 - "Worker HTTP Boundary"
Cohesion: 0.22
Nodes (8): Route, Account, Collection, Create, Disable, Enable, ReissueEnrollment, ResetPassword

### Community 107 - "Worker HTTP Boundary"
Cohesion: 0.36
Nodes (6): bounded_objects(), object(), optional_integer(), optional_object(), positive_integer(), safe_integer()

### Community 110 - "Auth Test Coverage"
Cohesion: 0.42
Nodes (6): all_rows(), receipt_rows(), RECEIPT_UPDATE_FAULTS, receipt_update_faults_roll_back_then_original_ack_and_command_recover(), scalar(), unpublished_lobby()

### Community 111 - "Worker HTTP Boundary"
Cohesion: 0.29
Nodes (7): GameViewSelector, Account, Player, read_query(), ViewSelector, Account, Player

### Community 113 - "Backend Workspace Structure"
Cohesion: 0.25
Nodes (7): RemovalEvent, Aborted, AbortStarted, AwaitingAcknowledgements, Committed, Completed, RetryScheduled

### Community 114 - "Worker HTTP Boundary"
Cohesion: 0.25
Nodes (8): DirectoryRejection, Busy, Completed, HostedGame, InvalidOperation, StaleOperation, Unavailable, UnknownOperation

### Community 115 - "Worker HTTP Boundary"
Cohesion: 0.29
Nodes (6): AccountRejection, Conflict, InvalidInput, Unauthorized, Unavailable, GamePeerError

### Community 116 - "Worker HTTP Boundary"
Cohesion: 0.25
Nodes (7): DirectoryGameRejection, AssignmentBlocked, Completed, Conflict, InvalidProof, Unavailable, UnknownGame

### Community 117 - "Backend Workspace Structure"
Cohesion: 0.43
Nodes (3): call_accounts(), dispatch(), handle()

### Community 118 - "Backend Workspace Structure"
Cohesion: 0.46
Nodes (5): execute(), MAX_PAGE_ENTRIES, prepare(), Prepared, run()

### Community 119 - "Typed Environment Configuration"
Cohesion: 0.25
Nodes (7): InvalidValueKind, ApiPath, Origin, OutOfRange, PublicText, SecretKey, TooLong

### Community 120 - "Shared Account Domain"
Cohesion: 0.29
Nodes (6): GameState, AwaitingPlayers, Cancelled, InProgress, New, Resolved

### Community 121 - "Auth Test Coverage"
Cohesion: 0.38
Nodes (5): migrated_prestart(), migrated_prestart_counter_retirement_ignored_or_aborted_delete_rolls_back_clock_and_release(), migrated_prestart_idle_cancellation_retires_obsolete_counter_in_real_maintenance(), ORIGINAL_RECORD_SCALARS, PREVIOUS_IDLE_CANCELLATION

### Community 122 - "Backend Workspace Structure"
Cohesion: 0.29
Nodes (4): HTTP_TIMEOUT, MAX_CA_PEM_BYTES, MAX_REQUEST_BYTES, MAX_RESPONSE_BYTES

### Community 123 - "Shared Account Domain"
Cohesion: 0.48
Nodes (4): GameConfiguration, GameValidationError, WinningPattern, SingleLine

### Community 124 - "Shared Account Domain"
Cohesion: 0.40
Nodes (4): BoardCellKind, Free, Value, single_line_returns_all_lines_in_approved_order_and_rejects_invalid_shape()

### Community 126 - "Durable SQLite Storage"
Cohesion: 0.50
Nodes (4): add_three_calendar_months(), civil_from_days(), days_from_civil(), days_in_month()

## Knowledge Gaps
- **851 isolated node(s):** `BODY_LIMIT`, `COOKIE_HEADER_MAX_BYTES`, `Login`, `Current`, `RedeemEnrollment` (+846 more)
  These have ≤1 connection - possible missing edges. (Counts symbols only; 1314 node(s) total have ≤1 connection when file, concept and rationale nodes are included.)
- **16 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `migrate_directory()` connect `Auth Test Coverage` to `Durable SQLite Storage`, `Durable SQLite Storage`?**
  _High betweenness centrality (0.001) - this node is a cross-community bridge._
- **Why does `ManagementResponse` connect `Shared Account Domain` to `Auth Test Coverage`, `Shared Account Domain`?**
  _High betweenness centrality (0.000) - this node is a cross-community bridge._
- **Why does `DirectoryError` connect `Durable SQLite Storage` to `Durable SQLite Storage`?**
  _High betweenness centrality (0.000) - this node is a cross-community bridge._
- **What connects `BODY_LIMIT`, `COOKIE_HEADER_MAX_BYTES`, `Login` to the rest of the system?**
  _851 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Durable SQLite Storage` be split into smaller, more focused modules?**
  _Cohesion score 0.010471204188481676 - nodes in this community are weakly interconnected._
- **Should `Auth Test Coverage` be split into smaller, more focused modules?**
  _Cohesion score 0.05477961182659169 - nodes in this community are weakly interconnected._
- **Should `Auth Test Coverage` be split into smaller, more focused modules?**
  _Cohesion score 0.05791335101679929 - nodes in this community are weakly interconnected._