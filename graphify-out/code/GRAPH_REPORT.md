# Scope: code-only structural graph

Rust/JavaScript source and tests only. No semantic documentation refresh; the older graph in the parent directory is not marked current. No LLM token cost for this extraction.

AST health warnings: {"dangling_endpoint_edges": 440, "directed_same_endpoint_collapsed_edges": 187}. Unresolved references are not verified relationships; structural-extraction.json retains raw edge evidence.

# Graph Report - brews-bingo  (2026-10-06)

## Corpus Check
- 141 files · ~119,706 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 3079 nodes · 7948 edges · 123 communities (99 shown, 24 thin omitted)
- Extraction: 96% EXTRACTED · 4% INFERRED · 0% AMBIGUOUS · INFERRED: 339 edges (avg confidence: 0.85)
- Token cost: 0 input · 0 output

## Community Hubs (Navigation)
- Durable SQLite Storage
- Auth Test Coverage
- Auth Test Coverage
- Durable SQLite Storage
- Shared Account Domain
- Worker HTTP Boundary
- Auth Test Coverage
- Worker HTTP Boundary
- Worker HTTP Boundary
- Account Authentication Authority
- Backend Workspace Structure
- Shared Account Domain
- Auth Test Coverage
- Worker HTTP Boundary
- Backend Workspace Structure
- Durable SQLite Storage
- Backend Workspace Structure
- Worker HTTP Boundary
- Durable SQLite Storage
- Worker HTTP Boundary
- Durable SQLite Storage
- Account Authentication Authority
- Shared Account Domain
- Shared Account Domain
- Backend Workspace Structure
- Auth Test Coverage
- Shared Account Domain
- Worker HTTP Boundary
- Shared Account Domain
- Account Authentication Authority
- Auth Test Coverage
- Backend Workspace Structure
- Auth Test Coverage
- Durable SQLite Storage
- Worker HTTP Boundary
- Backend Workspace Structure
- Durable SQLite Storage
- Backend Workspace Structure
- Typed Environment Configuration
- Worker HTTP Boundary
- Account Authentication Authority
- Backend Workspace Structure
- Account Authentication Authority
- Account Authentication Authority
- Account Authentication Authority
- Backend Workspace Structure
- Account Authentication Authority
- Backend Workspace Structure
- Auth Test Coverage
- Worker HTTP Boundary
- Account Authentication Authority
- Durable SQLite Storage
- Account Authentication Authority
- Worker HTTP Boundary
- Auth Test Coverage
- Backend Workspace Structure
- Typed Environment Configuration
- Shared Account Domain
- Shared Account Domain
- Durable SQLite Storage
- Worker HTTP Boundary
- Backend Workspace Structure
- Backend Workspace Structure
- Worker HTTP Boundary
- Worker HTTP Boundary
- Shared Account Domain
- Worker HTTP Boundary
- Auth Test Coverage
- Shared Account Domain
- Worker HTTP Boundary
- Durable SQLite Storage
- Backend Workspace Structure
- Shared Account Domain
- Worker HTTP Boundary
- Worker HTTP Boundary
- Backend Workspace Structure
- Backend Workspace Structure
- Typed Environment Configuration
- Worker HTTP Boundary
- Account Authentication Authority
- Worker HTTP Boundary
- Backend Workspace Structure
- Shared Account Domain
- Account Authentication Authority
- Worker HTTP Boundary
- Backend Workspace Structure
- Backend Workspace Structure
- Shared Account Domain
- Durable SQLite Storage
- Backend Workspace Structure
- Typed Environment Configuration
- Account Authentication Authority
- Durable SQLite Storage
- Durable SQLite Storage
- Shared Account Domain
- Worker HTTP Boundary
- Worker HTTP Boundary
- Account Authentication Authority
- Backend Workspace Structure
- Backend Workspace Structure
- Worker HTTP Boundary
- Backend Workspace Structure
- Typed Environment Configuration
- Worker HTTP Boundary
- Auth Test Coverage
- Typed Environment Configuration
- Worker HTTP Boundary
- Backend Workspace Structure
- Backend Workspace Structure
- Typed Environment Configuration
- Backend Workspace Structure
- Auth Test Coverage
- Auth Test Coverage
- Shared Account Domain
- Typed Environment Configuration
- Auth Test Coverage
- Shared Account Domain

## God Nodes (most connected - your core abstractions)
1. `GameError` - 140 edges
2. `migrate_directory()` - 106 edges
3. `migrate()` - 97 edges
4. `AuthError` - 88 edges
5. `DirectoryError` - 82 edges
6. `ManagementError` - 74 edges
7. `GameService<'a, D, R>` - 69 edges
8. `operation()` - 63 edges
9. `account()` - 56 edges
10. `StorageError` - 50 edges

## Surprising Connections (you probably didn't know these)
- `GamePayload` --references--> `AdmissionContextInput`  [EXTRACTED]
  backend/src/api/games.rs → shared/contracts/src/games.rs
- `GamePayload` --references--> `RevisionCommand`  [EXTRACTED]
  backend/src/api/games.rs → shared/contracts/src/games.rs
- `sqlite_username_boundaries_match_domain_validation_on_insert_and_update()` --calls--> `validate_username()`  [INFERRED]
  backend/src/db/schema/auth_schema.rs → shared/domain/src/accounts.rs
- `CliRequest` --references--> `ManagementCommand`  [EXTRACTED]
  backend/src/api/developer.rs → shared/contracts/src/management.rs
- `GamePayload` --references--> `CreateGame`  [EXTRACTED]
  backend/src/api/games.rs → shared/contracts/src/games.rs

## Import Cycles
- None detected.

## Communities (123 total, 24 thin omitted)

### Community 0 - "Durable SQLite Storage"
Cohesion: 0.02
Nodes (123): CELL_FREE, CELL_VALUE, GAME_DEADLINE_SOURCE_COUNT, LINE_ANTI, LINE_COLUMN, LINE_MAIN, LINE_ROW, OBSERVED_CONNECTION_LIMIT (+115 more)

### Community 1 - "Auth Test Coverage"
Cohesion: 0.06
Nodes (107): migrate_directory(), account(), acquire_persists_a_presence_only_gate_with_separate_operation_identity(), acquired_gate_rejects_transfer_without_changing_the_original_host(), competing_operation_returns_busy_without_replacing_the_current_gate(), completed_operation_cannot_reacquire_or_clear_a_newer_gate(), concurrent_operations_on_one_account_serialize_to_one_grant_and_one_busy(), corrupted_completion_time_cannot_erase_the_fence_or_authorize_an_ack() (+99 more)

### Community 2 - "Auth Test Coverage"
Cohesion: 0.06
Nodes (92): migrate(), a_current_host_session_is_attributed_when_admin_authority_is_denied(), account_detail_is_safe_and_deleted_or_unknown_target_is_not_found(), account_pages_are_bounded_and_continue_without_duplicates(), audit_write_failure_rolls_back_account_link_and_receipt(), authenticated_rejections_are_durably_audited(), command_id(), create() (+84 more)

### Community 3 - "Durable SQLite Storage"
Cohesion: 0.07
Nodes (48): binding_params(), DELIVERY_CLEANUP_BATCH_SIZE, DELIVERY_ID_GENERATION_MAX_ATTEMPTS, DeliveryBinding, DeliveryBudget, DeliveryError, BindingMismatch, Capacity (+40 more)

### Community 4 - "Shared Account Domain"
Cohesion: 0.05
Nodes (56): admission_and_join_validate_lookup_alias_and_never_debug_recovery_material(), assert_response_rejected(), BASE64URL, created_completion_can_be_later_than_creation_without_moving_idle_deadline(), creation_resolves_defaults_and_overrides_with_effective_center(), DELIVERY_ID_BYTES, DELIVERY_ID_ENCODED_BYTES, GAME_BODY_MAX_BYTES (+48 more)

### Community 5 - "Worker HTTP Boundary"
Cohesion: 0.09
Nodes (20): GameService, OwnerDatabase, close_checked(), GameObject, private_response(), reload(), Attachment, CloseCode (+12 more)

### Community 6 - "Auth Test Coverage"
Cohesion: 0.09
Nodes (40): directoryStub(), initializeDirectory(), commandId(), cookie(), current(), enroll(), inOwner(), origin (+32 more)

### Community 7 - "Worker HTTP Boundary"
Cohesion: 0.05
Nodes (41): directory_claim_ack_and_code_use_real_owner_core_and_exact_binding(), directory_publication_recovery_and_release_use_real_core(), DirectoryGameOutcome, Acknowledged, Code, CodeAllocated, Due, Lookup (+33 more)

### Community 8 - "Worker HTTP Boundary"
Cohesion: 0.10
Nodes (37): DirectoryService, absent_after_deadline_keeps_reservation_and_saturating_retry(), command(), Fixture, genuine_committed_lobby_recovers_directory_publication_without_faking_game_ack(), genuine_initializer_evidence_acknowledges_only_current_unready_creation(), genuine_new_cancellation_releases_only_the_hidden_directory_association(), genuine_owner_terminal_evidence_releases_reservation_after_deadline() (+29 more)

### Community 9 - "Account Authentication Authority"
Cohesion: 0.09
Nodes (12): AuthService<'_, D, R>, command(), GameSocketCloseWork, removal_finishes_only_after_all_exact_socket_close_acknowledgements(), SOCKET_CLOSE_BACKOFF_MAX_SHIFT, SOCKET_CLOSE_MAX_DELAY_MS, Sqlite, subscription_values() (+4 more)

### Community 10 - "Backend Workspace Structure"
Cohesion: 0.06
Nodes (18): CodeGrant, CREATION_FINGERPRINT_BYTES, CREATION_LIFETIME_MS, CREATION_RETRY_INITIAL_MS, CREATION_RETRY_MAX_MS, CreationAck, GAME_CODE_ENTROPY_BATCHES, GAME_CODE_MAX_CANDIDATES (+10 more)

### Community 11 - "Shared Account Domain"
Cohesion: 0.07
Nodes (39): line_parts(), ALIAS_MAX_BYTES, AssignedBoard, BOARD_CELLS_MAX, BOARD_SIDE_MAX, BOARD_SIDE_MIN, BoardCell, BoardCellKind (+31 more)

### Community 12 - "Auth Test Coverage"
Cohesion: 0.17
Nodes (37): creation_fingerprint(), account_socket_grants_require_fresh_accounts_and_do_not_renew_expiry_or_presence(), actual_sqlite_reopen_preserves_lobby_intent_and_lost_publication_ack(), admission_is_verifier_only_short_lived_and_consumed_retries_never_mint_cookies(), answer_kdf_runs_outside_sqlite_write_and_rechecks_real_competing_admission(), command_receipts_use_variant_scoped_account_and_player_actor_namespaces(), cookie(), creation_configuration_set_order_is_canonical_without_a_blob_authority() (+29 more)

### Community 13 - "Worker HTTP Boundary"
Cohesion: 0.08
Nodes (36): ACCOUNT, account_get_preserves_the_canonical_typed_target(), action_routes_reject_other_methods_without_decoding_payloads(), bodyless_mutation_routes_map_to_their_closed_management_commands(), bodyless_mutations(), collection_get_maps_to_default_list_without_claiming_authority(), collection_query(), collection_query_byte_bound_precedes_parsing() (+28 more)

### Community 14 - "Backend Workspace Structure"
Cohesion: 0.05
Nodes (23): AuthPolicy, ANSWER_CANONICAL_BYTES, ANSWER_INPUT_BYTES, enroll(), NORMALIZATION_VERSION, normalize(), push(), bounded_phc() (+15 more)

### Community 15 - "Durable SQLite Storage"
Cohesion: 0.13
Nodes (18): account_actor(), add_time(), Admission, blob(), command_fingerprint(), ensure_before_idle(), join_fingerprint(), make_id() (+10 more)

### Community 16 - "Backend Workspace Structure"
Cohesion: 0.10
Nodes (17): GameService<'a, D, R>, revision(), GameError, Capacity, Conflict, Expired, Forbidden, GenerationExhausted (+9 more)

### Community 17 - "Worker HTTP Boundary"
Cohesion: 0.09
Nodes (37): admission_context_is_keyless_and_join_uses_separate_game_scoped_cookie(), ADMISSION_COOKIE_NAME, admission_forwards_an_existing_player_cookie_without_claiming_authority(), all_transport_inputs_have_release_active_finite_bounds(), command_id(), creation_route_decodes_original_configuration_without_caller_identity(), decode_games(), exact_routes_reject_other_methods_authorization_and_origin_ambiguity() (+29 more)

### Community 18 - "Durable SQLite Storage"
Cohesion: 0.13
Nodes (20): DirectoryService<'a, D, R>, operation_timestamp(), parse_completion_row(), parse_operation_row(), DirectoryError, AssignmentBlocked, Busy, Clock (+12 more)

### Community 19 - "Worker HTTP Boundary"
Cohesion: 0.07
Nodes (36): all_seven_operations_accept_their_exact_payload(), assert_array_root_rejected(), BODY_LIMIT, CompleteBody, COOKIE_HEADER_MAX_BYTES, cookie_is_read_and_duplicate_session_values_reject(), decode(), Decoded (+28 more)

### Community 20 - "Durable SQLite Storage"
Cohesion: 0.15
Nodes (12): command_timestamp(), CREATION_COLUMNS, deadline(), DirectoryService<'_, D, R>, integer(), optional_integer(), parse_creation(), parse_index() (+4 more)

### Community 21 - "Account Authentication Authority"
Cohesion: 0.06
Nodes (20): AuthService<'_, D, R>, AuthService<'_, D, R>, ManagementError, Busy, Conflict, Crypto, Forbidden, HostedGame (+12 more)

### Community 22 - "Shared Account Domain"
Cohesion: 0.07
Nodes (30): account_fields_reject_positional_sequences(), ACCOUNT_OBJECT, ACCOUNT_SEQUENCE, assert_invalid_response(), CreateUsername, deserialize_object(), deserialize_users(), every_public_variant_round_trips_with_safe_debug_and_http_status() (+22 more)

### Community 23 - "Shared Account Domain"
Cohesion: 0.09
Nodes (18): AuthService<'_, D, R>, actor_key(), AuthService<'_, D, R>, deadline(), AuditActor, Account, DeveloperCli, AuditEvent (+10 more)

### Community 24 - "Backend Workspace Structure"
Cohesion: 0.06
Nodes (26): ACCOUNT_KIND, ADMISSION_LIMIT, ADMISSION_MS, CreationResult, HOST_IDLE_MS, HOST_IDLE_REASON, HOST_VIEW, LIVE_ACCESS (+18 more)

### Community 25 - "Auth Test Coverage"
Cohesion: 0.10
Nodes (29): assert_issued_handoff(), audit_reads_preserve_required_targets_and_read_operations(), authorization_debug_is_redacted_and_server_errors_discard_bodies(), BrokenPipe, ca_file_read_is_bounded_and_errors_are_redacted(), client_rejects_noncanonical_or_non_https_origins_before_network(), command_runner_exercises_real_https_to_safe_metadata(), existing_output_target_prevents_any_http_mutation() (+21 more)

### Community 26 - "Shared Account Domain"
Cohesion: 0.07
Nodes (25): Route, Account, Collection, Create, Disable, Enable, ReissueEnrollment, ResetPassword (+17 more)

### Community 27 - "Worker HTTP Boundary"
Cohesion: 0.16
Nodes (29): account_reply(), acknowledge_creation(), allocate_game_code(), authorize_account(), authorize_account_connection(), call(), call_game_close(), claim_game() (+21 more)

### Community 28 - "Shared Account Domain"
Cohesion: 0.12
Nodes (13): AdmissionContextInput, checked_deadline(), ConfigurationOverrides, decode(), encode(), GameDecodeError, response_encoder_uses_the_same_acknowledgement_validator(), RevisionCommand (+5 more)

### Community 29 - "Account Authentication Authority"
Cohesion: 0.17
Nodes (10): AuthOutcome, RequestContext, add_deadline(), AuthService<'a, D, R>, token_digest(), SessionScope, EnrollmentOnly, Normal (+2 more)

### Community 30 - "Auth Test Coverage"
Cohesion: 0.17
Nodes (22): new_token(), command_id(), count(), create(), Fixture, ORIGIN, seed(), session() (+14 more)

### Community 31 - "Backend Workspace Structure"
Cohesion: 0.06
Nodes (31): AUDIT_RETENTION_MS, id_check(), LINK_RECEIPT_RETENTION_MS, management_schema_statements(), REMOVAL_INITIAL_DELAY_MS, REMOVAL_MAX_DELAY_MS, ACCESS_LINK_LIFETIME_MS, ACCESS_LINK_METADATA_RETENTION_MS (+23 more)

### Community 32 - "Auth Test Coverage"
Cohesion: 0.23
Nodes (28): bearer(), clock_rollback_and_unavailable_storage_never_release_authority(), close_pages_are_bounded_stably_ordered_and_do_not_discard_backlog(), close_retry_is_durable_capped_and_saturates_count_and_deadline(), connection(), context(), current_normal_host_and_admin_produce_private_game_authority(), due_close_work_is_bounded_and_preserves_exact_revoked_binding() (+20 more)

### Community 33 - "Durable SQLite Storage"
Cohesion: 0.08
Nodes (7): StorageError, AUTH_SCHEMA_VERSION, CURRENT_SCHEMA_VERSION, Database, metadata_clock_bounds_match_the_named_safe_integer_limit(), metadata_statement(), Sqlite

### Community 34 - "Worker HTTP Boundary"
Cohesion: 0.12
Nodes (23): account_registration_and_post_gate_recheck_use_real_accounts_core(), AccountAction, Authorize, AuthorizeConnection, Register, Unregister, AccountReply, AccountRequest (+15 more)

### Community 35 - "Backend Workspace Structure"
Cohesion: 0.11
Nodes (5): GameAccountAuthority, authorize(), authorize_mutation(), AccountConnectionGrant, GameView

### Community 36 - "Durable SQLite Storage"
Cohesion: 0.20
Nodes (5): advance(), parse_connection(), same_work_target(), text(), ConnectionGrant

### Community 38 - "Typed Environment Configuration"
Cohesion: 0.08
Nodes (21): APP_ORIGIN, APP_ORIGIN_MAX_BYTES, ARGON2_DEFAULT_ITERATIONS, ARGON2_DEFAULT_MEMORY_KIB, ARGON2_M_COST, ARGON2_MAX_ITERATIONS, ARGON2_MAX_MEMORY_KIB, ARGON2_MIN_ITERATIONS (+13 more)

### Community 39 - "Worker HTTP Boundary"
Cohesion: 0.08
Nodes (21): rejection(), success(), GAME_WIRE_MAX_BYTES, GameCookieWire, Admission, None, Player, GameOwnerResponse (+13 more)

### Community 40 - "Account Authentication Authority"
Cohesion: 0.11
Nodes (13): AuthError, Conflict, Crypto, InvalidCredentials, InvalidInput, InvalidLink, RateLimited, StaleCommand (+5 more)

### Community 41 - "Backend Workspace Structure"
Cohesion: 0.09
Nodes (10): HTTP_TIMEOUT, MAX_CA_PEM_BYTES, MAX_REQUEST_BYTES, MAX_RESPONSE_BYTES, ENROLLMENT_PATH, LINK_TOKEN_BYTES, LINK_TOKEN_ENCODED_BYTES, PASSWORD_RESET_PATH (+2 more)

### Community 42 - "Account Authentication Authority"
Cohesion: 0.12
Nodes (12): ManagementPrincipal, AdminSession, DeveloperCli, AuthService<'_, D, R>, changes(), final_release_reproves_live_admin_without_writes_or_audit(), fixture(), id() (+4 more)

### Community 43 - "Account Authentication Authority"
Cohesion: 0.33
Nodes (20): admin_session(), app_admin_self_disable_delete_and_enable_are_forbidden(), command_id(), committed_removal_phase_is_bound_and_persisted_in_real_sqlite(), deletion_does_not_require_an_unused_credential_epoch_increment(), deletion_requires_matching_gate_and_retains_intent_until_release_ack(), developer_cli_cannot_remove_the_last_enabled_verified_admin(), disable_changes_epoch_once_and_repeated_no_op_keeps_timestamp() (+12 more)

### Community 44 - "Account Authentication Authority"
Cohesion: 0.15
Nodes (10): value(), access_link_cleanup_and_alarm_share_the_exact_metadata_retention_deadline(), ACCOUNT_ID, cleanup_limits_each_delete_and_expired_close_enqueue_to_the_named_batch_size(), entity_id(), seed_account(), sql_params(), Sqlite (+2 more)

### Community 45 - "Backend Workspace Structure"
Cohesion: 0.13
Nodes (3): CreationFingerprint, CreationReadyProof, CreationWork

### Community 46 - "Account Authentication Authority"
Cohesion: 0.18
Nodes (11): parse_work(), Account, ACCOUNT_SELECT, integer(), optional_integer(), optional_text(), Session, text() (+3 more)

### Community 47 - "Backend Workspace Structure"
Cohesion: 0.10
Nodes (11): AuditCommand, List, AuditPage, canonical_id(), Cli, DEFAULT_PAGE_ENTRIES, prepares_exact_operation_and_command_identity_once(), RootCommand (+3 more)

### Community 48 - "Auth Test Coverage"
Cohesion: 0.13
Nodes (15): ConfigError, Deserialization, InvalidValue, MissingKey, acquisition_can_only_request_the_declared_inventory(), assert_redacted(), dev_cli_key_is_optional_but_supplied_values_are_validated(), explicit_argon2_costs_are_deserialized_as_numbers() (+7 more)

### Community 49 - "Worker HTTP Boundary"
Cohesion: 0.16
Nodes (14): ApiError, Forbidden, InvalidInput, MethodNotAllowed, NotFound, PayloadTooLarge, Unavailable, UnsupportedMediaType (+6 more)

### Community 50 - "Account Authentication Authority"
Cohesion: 0.19
Nodes (9): AuthService<'_, D, R>, RemovalGateGrant, RemovalPhase, Aborting, Committed, Prepared, RemovalReleaseAck, RemovalWork (+1 more)

### Community 51 - "Durable SQLite Storage"
Cohesion: 0.11
Nodes (7): Sqlite, int(), SqlValue, Blob, Integer, Null, Text

### Community 52 - "Account Authentication Authority"
Cohesion: 0.11
Nodes (12): AuthCommand, Complete, Current, Login, Logout, Redeem, CookieWire, Clear (+4 more)

### Community 53 - "Worker HTTP Boundary"
Cohesion: 0.14
Nodes (8): deserialize_error_fields(), dispatch(), ErrorEnvelope, ErrorFields, handle(), log_delivery(), OwnerUsersRequest, validate_error()

### Community 54 - "Auth Test Coverage"
Cohesion: 0.22
Nodes (11): assert_verifier_rehashed(), assertion_failure_diagnostics_do_not_disclose_verifiers(), command_id(), context(), cookie(), create(), enrollment_and_reset_bind_status_purpose_and_scope_without_changing_stored_tags(), ORIGIN (+3 more)

### Community 55 - "Backend Workspace Structure"
Cohesion: 0.11
Nodes (13): ManagementClient, CliError, Configuration, Confirmation, HttpStatus, InvalidInput, LinkFile, Output (+5 more)

### Community 56 - "Typed Environment Configuration"
Cohesion: 0.16
Nodes (15): absent_public_settings_use_serde_defaults(), API_PATH, assert_redacted(), CONFIG, explicit_public_values_are_preserved(), FRONTEND_KEYS, get_frontend_config(), maximum_display_name_is_measured_in_utf8_bytes() (+7 more)

### Community 57 - "Shared Account Domain"
Cohesion: 0.19
Nodes (15): canonical_alias(), cell_kind(), cells(), completed_lines(), configuration(), lookup_code(), non_null_option(), nullable_snapshot() (+7 more)

### Community 58 - "Shared Account Domain"
Cohesion: 0.11
Nodes (19): operation_tag(), replay_guidance(), AuditOperation, CreateAccount, DeleteAccount, DisableAccount, EnableAccount, GetAccount (+11 more)

### Community 59 - "Durable SQLite Storage"
Cohesion: 0.15
Nodes (10): ACCOUNT_ID, auth_schema_ddl_matches_the_pre_refactor_snapshot(), auth_schema_statements(), connection(), ENTITY_ID, sqlite_account_caps_match_verifier_and_safe_integer_limits(), sqlite_rate_buckets_match_failure_count_and_digest_limits(), sqlite_receipts_match_payload_digest_and_retention_limits() (+2 more)

### Community 60 - "Worker HTTP Boundary"
Cohesion: 0.12
Nodes (7): DELIVERY_TABLES, GAME_SCHEMA_VERSION, GAME_TABLES, ATTACHMENT_MAX_BYTES, SIZE_PLACEHOLDER, SOCKET_LIMIT, WEB_CRYPTO_RANDOM_MAX_BYTES

### Community 61 - "Backend Workspace Structure"
Cohesion: 0.16
Nodes (6): command(), dispatch(), handle(), json_response(), sdk_error(), decode_private_json()

### Community 62 - "Backend Workspace Structure"
Cohesion: 0.11
Nodes (16): Failure, Configuration, Conflict, Crypto, Forbidden, InvalidCredentials, InvalidInput, InvalidLink (+8 more)

### Community 63 - "Worker HTTP Boundary"
Cohesion: 0.12
Nodes (16): DirectoryRejection, Busy, Completed, HostedGame, InvalidOperation, StaleOperation, Unavailable, UnknownOperation (+8 more)

### Community 64 - "Worker HTTP Boundary"
Cohesion: 0.12
Nodes (16): GameOperation, AdmissionContext, Create, JoinPlayer, Lobby, Start, Stream, Sync (+8 more)

### Community 65 - "Shared Account Domain"
Cohesion: 0.13
Nodes (13): action(), AuthService<'_, D, R>, list(), ManagementCommand, CreateAccount, DeleteAccount, DisableAccount, EnableAccount (+5 more)

### Community 67 - "Auth Test Coverage"
Cohesion: 0.12
Nodes (6): constructor_is_zero_argument_and_uses_only_public_build_inputs(), public_singleton_caches_the_constructor_outcome(), get_initializes_the_public_config_and_returns_the_same_instance(), racing_backend_get_calls_share_one_cached_failure(), backend_failure_does_not_poison_the_public_singleton(), constructor_acquires_every_declared_key_once()

### Community 68 - "Shared Account Domain"
Cohesion: 0.13
Nodes (5): CreateGame, DeliveryId, JoinPlayer, recovery_answer_explicit_zeroization_clears_owned_sensitive_bytes(), RecoveryAnswer

### Community 69 - "Worker HTTP Boundary"
Cohesion: 0.17
Nodes (7): GameViewSelector, Account, Player, GameIngress, ViewSelector, Account, Player

### Community 70 - "Durable SQLite Storage"
Cohesion: 0.28
Nodes (11): account_acceptance_preflight_rejects_fenced_or_replaced_preparations_without_accepting(), authority(), close_work(), id(), initialized(), inventory(), migrate_game(), prepared_account_close_requires_durable_deletion_before_success() (+3 more)

### Community 71 - "Backend Workspace Structure"
Cohesion: 0.14
Nodes (13): AccountCommand, Create, Delete, Disable, Enable, Get, List, ReissueEnrollment (+5 more)

### Community 72 - "Shared Account Domain"
Cohesion: 0.13
Nodes (14): AccessLinkPurpose, Enrollment, PasswordReset, AccountStatus, PendingEnrollment, ResetRequired, Verified, ValidationError (+6 more)

### Community 73 - "Worker HTTP Boundary"
Cohesion: 0.20
Nodes (7): AccountAuthorityWire, AccountCloseIdentity, AccountConnectionIdentity, AccountOutcome, Authorized, Rejected, Unregistered

### Community 74 - "Worker HTTP Boundary"
Cohesion: 0.16
Nodes (11): AccountCloseReply, AccountCloseRequest, CloseAction, CloseAccount, CloseCaller, Accounts, CloseResult, Closed (+3 more)

### Community 75 - "Backend Workspace Structure"
Cohesion: 0.30
Nodes (7): failure_cleanup_truncates_only_owned_handle_not_replaced_path(), LinkFile, persists_link_without_displaying_or_reopening_path(), replaced_path_is_rejected_before_secret_write(), reserves_new_private_file_before_network(), weakened_permissions_or_hardlinks_fail_before_secret_write(), write_failure_is_typed_without_secret_sources()

### Community 76 - "Backend Workspace Structure"
Cohesion: 0.15
Nodes (12): Boundary, AccountsAlarm, AccountsAuth, AccountsManagement, AccountsPeer, AuthIngress, CliIngress, DirectoryAlarm (+4 more)

### Community 77 - "Typed Environment Configuration"
Cohesion: 0.19
Nodes (8): BREWS_API_ORIGIN, BREWS_DEV_CLI_KEY, BREWS_TLS_CA_FILE, CLI_KEYS, CLI_ORIGIN_MAX_BYTES, CliConfig, CONFIG, get_cli_config()

### Community 78 - "Worker HTTP Boundary"
Cohesion: 0.23
Nodes (3): authenticate_cli(), CliRequest, decode_cli()

### Community 79 - "Account Authentication Authority"
Cohesion: 0.17
Nodes (4): TestRuntime, Runtime, ByteRuntime, TestRuntime

### Community 81 - "Backend Workspace Structure"
Cohesion: 0.23
Nodes (8): api_failure(), auth_failure(), boundary_failures_have_closed_fields_and_expected_levels(), init(), management_failure(), subscriber(), TARGET, failure()

### Community 82 - "Shared Account Domain"
Cohesion: 0.29
Nodes (12): assert_view_rejected(), host_new_view(), player_lobby_view(), review_view_awaiting_requires_pre_start_lifecycle_but_allows_hidden_host_code(), review_view_in_progress_requires_code_and_start_without_end_or_idle(), review_view_new_has_no_member_projection_or_lobby_metadata(), review_view_optional_board_is_absent_not_null(), review_view_requires_explicit_nullable_lifecycle_fields() (+4 more)

### Community 83 - "Account Authentication Authority"
Cohesion: 0.20
Nodes (7): CommandOutcome, CompleteEnrollment, CompletePasswordReset, Login, Redeem, fingerprint(), StoredOutcome

### Community 85 - "Backend Workspace Structure"
Cohesion: 0.35
Nodes (4): call_accounts(), dispatch(), handle(), OwnerManagementRequest

### Community 86 - "Backend Workspace Structure"
Cohesion: 0.36
Nodes (8): human_and_json_account_output_escape_all_controls(), issued_output_projects_metadata_without_link_secrets(), terminal_text(), terminal_text_escapes_username_controls(), write_error(), write_metadata(), write_notice(), write_response()

### Community 88 - "Durable SQLite Storage"
Cohesion: 0.20
Nodes (4): DELIVERY_PENDING_MAX, DELIVERY_QUEUE_MAX_BYTES, DELIVERY_SCHEMA_VERSION, DELIVERY_TABLES

### Community 89 - "Backend Workspace Structure"
Cohesion: 0.22
Nodes (8): Delivery, Committed, Issued, Pending, management_delivery(), privileged_projection_omits_sensitive_spans_and_unrelated_events(), removal(), typed_lifecycle_records_use_closed_fields_and_canonical_ids()

### Community 90 - "Typed Environment Configuration"
Cohesion: 0.24
Nodes (3): SECRET_KEY_BYTES, SECRET_KEY_ENCODED_LEN, SecretKey

### Community 91 - "Account Authentication Authority"
Cohesion: 0.31
Nodes (4): AuthService<'_, D, R>, fingerprint(), validate_receipt(), ManagementReceipt

### Community 92 - "Durable SQLite Storage"
Cohesion: 0.39
Nodes (6): application_tables(), MINIFLARE_METADATA_TABLE, schema_version(), validate_inventory(), validate_schema(), WORKERD_METADATA_TABLE

### Community 93 - "Durable SQLite Storage"
Cohesion: 0.25
Nodes (6): add_game_coordination(), DIRECTORY_SCHEMA_VERSION, DIRECTORY_TABLES, initialize_removal(), REMOVAL_SCHEMA_VERSION, REMOVAL_TABLES

### Community 94 - "Shared Account Domain"
Cohesion: 0.22
Nodes (7): validate_projection_player(), GameState, AwaitingPlayers, Cancelled, InProgress, New, Resolved

### Community 95 - "Worker HTTP Boundary"
Cohesion: 0.36
Nodes (6): bounded_objects(), object(), optional_integer(), optional_object(), positive_integer(), safe_integer()

### Community 97 - "Account Authentication Authority"
Cohesion: 0.25
Nodes (4): CookieEffect, Clear, None, Set

### Community 99 - "Backend Workspace Structure"
Cohesion: 0.25
Nodes (7): RemovalEvent, Aborted, AbortStarted, AwaitingAcknowledgements, Committed, Completed, RetryScheduled

### Community 100 - "Worker HTTP Boundary"
Cohesion: 0.29
Nodes (6): AccountRejection, Conflict, InvalidInput, Unauthorized, Unavailable, GamePeerError

### Community 101 - "Backend Workspace Structure"
Cohesion: 0.46
Nodes (5): execute(), MAX_PAGE_ENTRIES, prepare(), Prepared, run()

### Community 102 - "Typed Environment Configuration"
Cohesion: 0.25
Nodes (7): InvalidValueKind, ApiPath, Origin, OutOfRange, PublicText, SecretKey, TooLong

### Community 103 - "Worker HTTP Boundary"
Cohesion: 0.52
Nodes (3): AccountsObject, safe_response_headers(), Request

### Community 106 - "Worker HTTP Boundary"
Cohesion: 0.60
Nodes (3): AccountsObject, management_error(), management_result()

### Community 109 - "Typed Environment Configuration"
Cohesion: 0.60
Nodes (3): deserialize_error(), missing_values_only_identify_known_keys(), third_party_deserialization_errors_are_discarded()

## Knowledge Gaps
- **686 isolated node(s):** `BODY_LIMIT`, `COOKIE_HEADER_MAX_BYTES`, `Login`, `Current`, `RedeemEnrollment` (+681 more)
  These have ≤1 connection - possible missing edges. (Counts symbols only; 1090 node(s) total have ≤1 connection when file, concept and rationale nodes are included.)
- **24 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `GameRecoveryOutcome` connect `Worker HTTP Boundary` to `Worker HTTP Boundary`?**
  _High betweenness centrality (0.001) - this node is a cross-community bridge._
- **Why does `AuthError` connect `Account Authentication Authority` to `Durable SQLite Storage`?**
  _High betweenness centrality (0.001) - this node is a cross-community bridge._
- **Why does `Record` connect `Durable SQLite Storage` to `Backend Workspace Structure`, `Backend Workspace Structure`, `Shared Account Domain`?**
  _High betweenness centrality (0.000) - this node is a cross-community bridge._
- **What connects `BODY_LIMIT`, `COOKIE_HEADER_MAX_BYTES`, `Login` to the rest of the system?**
  _686 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Durable SQLite Storage` be split into smaller, more focused modules?**
  _Cohesion score 0.016 - nodes in this community are weakly interconnected._
- **Should `Auth Test Coverage` be split into smaller, more focused modules?**
  _Cohesion score 0.06333595594020457 - nodes in this community are weakly interconnected._
- **Should `Auth Test Coverage` be split into smaller, more focused modules?**
  _Cohesion score 0.0620978120978121 - nodes in this community are weakly interconnected._