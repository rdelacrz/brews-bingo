# Scope: code-only structural graph

Rust/JavaScript source and tests only. No semantic documentation refresh; the older graph in the parent directory is not marked current. No LLM token cost for this extraction.

AST health warnings: {"dangling_endpoint_edges": 300, "directed_same_endpoint_collapsed_edges": 122}. Unresolved references are not verified relationships; structural-extraction.json retains raw edge evidence.

# Graph Report - brews-bingo  (2026-10-05)

## Corpus Check
- 111 files · ~62,182 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 1622 nodes · 3760 edges · 84 communities (72 shown, 12 thin omitted)
- Extraction: 94% EXTRACTED · 6% INFERRED · 0% AMBIGUOUS · INFERRED: 235 edges (avg confidence: 0.85)
- Token cost: 0 input · 0 output

## Community Hubs (Navigation)
- Auth Test Coverage
- Auth Test Coverage
- Durable SQLite Storage
- Auth Test Coverage
- Auth Test Coverage
- Account Authentication Authority
- Worker HTTP Boundary
- Worker HTTP Boundary
- Shared Account Domain
- Auth Test Coverage
- Backend Workspace Structure
- Durable SQLite Storage
- Credential Cryptography
- Auth Test Coverage
- Worker HTTP Boundary
- Typed Environment Configuration
- Account Authentication Authority
- Account Authentication Authority
- Account Authentication Authority
- Account Authentication Authority
- Auth Test Coverage
- Auth Test Coverage
- Backend Workspace Structure
- Auth Test Coverage
- Typed Environment Configuration
- Shared Account Domain
- Account Authentication Authority
- Worker HTTP Boundary
- Backend Workspace Structure
- Account Authentication Authority
- Worker HTTP Boundary
- Shared Account Domain
- Backend Workspace Structure
- Account Authentication Authority
- Worker HTTP Boundary
- Worker HTTP Boundary
- Backend Workspace Structure
- Auth Test Coverage
- Shared Account Domain
- Shared Account Domain
- Worker HTTP Boundary
- Backend Workspace Structure
- Shared Account Domain
- Backend Workspace Structure
- Durable SQLite Storage
- Backend Workspace Structure
- Backend Workspace Structure
- Shared Account Domain
- Shared Account Domain
- Backend Workspace Structure
- Account Authentication Authority
- Backend Workspace Structure
- Backend Workspace Structure
- Shared Account Domain
- Backend Workspace Structure
- Account Authentication Authority
- Account Authentication Authority
- Backend Workspace Structure
- Backend Workspace Structure
- Account Authentication Authority
- Durable SQLite Storage
- Backend Workspace Structure
- Worker HTTP Boundary
- Typed Environment Configuration
- Worker HTTP Boundary
- Worker HTTP Boundary
- Account Authentication Authority
- Account Authentication Authority
- Auth Test Coverage
- Worker HTTP Boundary
- Backend Workspace Structure
- Typed Environment Configuration
- Backend Workspace Structure
- Typed Environment Configuration
- Worker HTTP Boundary
- Backend Workspace Structure
- Shared Account Domain

## God Nodes (most connected - your core abstractions)
1. `migrate()` - 93 edges
2. `ManagementError` - 74 edges
3. `AuthError` - 68 edges
4. `migrate_directory()` - 64 edges
5. `operation()` - 61 edges
6. `account()` - 56 edges
7. `ConfigError` - 46 edges
8. `ManagementCommand` - 45 edges
9. `ManagementResponse` - 40 edges
10. `DirectoryError` - 37 edges

## Surprising Connections (you probably didn't know these)
- `sqlite_username_boundaries_match_domain_validation_on_insert_and_update()` --calls--> `validate_username()`  [INFERRED]
  backend/src/db/schema/auth_schema.rs → shared/domain/src/accounts.rs
- `CliRequest` --references--> `ManagementCommand`  [EXTRACTED]
  backend/src/api/developer.rs → shared/contracts/src/management.rs
- `UsersRequest` --references--> `ManagementCommand`  [EXTRACTED]
  backend/src/api/users.rs → shared/contracts/src/management.rs
- `Route` --references--> `AccountRole`  [EXTRACTED]
  backend/src/api/users.rs → shared/domain/src/accounts.rs
- `list_command()` --references--> `ManagementCommand`  [EXTRACTED]
  backend/src/api/users.rs → shared/contracts/src/management.rs

## Import Cycles
- None detected.

## Communities (84 total, 12 thin omitted)

### Community 0 - "Auth Test Coverage"
Cohesion: 0.08
Nodes (72): migrate(), a_current_host_session_is_attributed_when_admin_authority_is_denied(), account_detail_is_safe_and_deleted_or_unknown_target_is_not_found(), account_pages_are_bounded_and_continue_without_duplicates(), audit_write_failure_rolls_back_account_link_and_receipt(), authenticated_rejections_are_durably_audited(), command_id(), create() (+64 more)

### Community 1 - "Auth Test Coverage"
Cohesion: 0.11
Nodes (63): migrate_directory(), account(), acquire_persists_a_presence_only_gate_with_separate_operation_identity(), acquired_gate_rejects_transfer_without_changing_the_original_host(), competing_operation_returns_busy_without_replacing_the_current_gate(), completed_operation_cannot_reacquire_or_clear_a_newer_gate(), concurrent_operations_on_one_account_serialize_to_one_grant_and_one_busy(), corrupted_completion_time_cannot_erase_the_fence_or_authorize_an_ack() (+55 more)

### Community 2 - "Durable SQLite Storage"
Cohesion: 0.10
Nodes (22): DIRECTORY_SCHEMA_VERSION, DIRECTORY_TABLES, DirectoryService, DirectoryService<'a, D, R>, MINIFLARE_METADATA_TABLE, operation_timestamp(), parse_completion_row(), parse_operation_row() (+14 more)

### Community 3 - "Auth Test Coverage"
Cohesion: 0.12
Nodes (27): directoryStub(), initializeDirectory(), commandId(), cookie(), current(), enroll(), inOwner(), origin (+19 more)

### Community 4 - "Auth Test Coverage"
Cohesion: 0.07
Nodes (32): main(), assert_issued_handoff(), audit_reads_preserve_required_targets_and_read_operations(), authorization_debug_is_redacted_and_server_errors_discard_bodies(), BrokenPipe, ca_file_read_is_bounded_and_errors_are_redacted(), client_rejects_noncanonical_or_non_https_origins_before_network(), command_runner_exercises_real_https_to_safe_metadata() (+24 more)

### Community 5 - "Account Authentication Authority"
Cohesion: 0.07
Nodes (18): AuthService<'_, D, R>, AuthService<'_, D, R>, AuthService<'_, D, R>, ManagementError, Busy, Conflict, Crypto, Forbidden (+10 more)

### Community 6 - "Worker HTTP Boundary"
Cohesion: 0.07
Nodes (36): all_seven_operations_accept_their_exact_payload(), assert_array_root_rejected(), BODY_LIMIT, CompleteBody, COOKIE_HEADER_MAX_BYTES, cookie_is_read_and_duplicate_session_values_reject(), decode(), Decoded (+28 more)

### Community 7 - "Worker HTTP Boundary"
Cohesion: 0.09
Nodes (28): ACCOUNT, action_routes_reject_other_methods_without_decoding_payloads(), bodyless_mutation_routes_map_to_their_closed_management_commands(), bodyless_mutations(), collection_get_maps_to_default_list_without_claiming_authority(), COMMAND, create(), create_requires_one_supported_json_media_type() (+20 more)

### Community 8 - "Shared Account Domain"
Cohesion: 0.08
Nodes (25): action(), AuthService<'_, D, R>, AuthService<'_, D, R>, fingerprint(), validate_receipt(), target(), list(), AuditActor (+17 more)

### Community 9 - "Auth Test Coverage"
Cohesion: 0.17
Nodes (22): new_token(), command_id(), count(), create(), Fixture, ORIGIN, seed(), session() (+14 more)

### Community 10 - "Backend Workspace Structure"
Cohesion: 0.06
Nodes (31): AUDIT_RETENTION_MS, id_check(), LINK_RECEIPT_RETENTION_MS, management_schema_statements(), REMOVAL_INITIAL_DELAY_MS, REMOVAL_MAX_DELAY_MS, ACCESS_LINK_LIFETIME_MS, ACCESS_LINK_METADATA_RETENTION_MS (+23 more)

### Community 11 - "Durable SQLite Storage"
Cohesion: 0.08
Nodes (7): Sqlite, StorageError, AUTH_SCHEMA_VERSION, CURRENT_SCHEMA_VERSION, Database, metadata_clock_bounds_match_the_named_safe_integer_limit(), metadata_statement()

### Community 12 - "Credential Cryptography"
Cohesion: 0.08
Nodes (17): AuthPolicy, bounded_phc(), DIGEST_BYTES, hash_password(), MAX_PHC_LEN, PASSWORD_HASH_BYTES, PASSWORD_SALT_BYTES, PHC_PARAMETER_COUNT (+9 more)

### Community 13 - "Auth Test Coverage"
Cohesion: 0.09
Nodes (16): ConfigError, Deserialization, InvalidValue, MissingKey, validate_origin(), failed_get_caches_a_safe_error_without_retrying(), NeverRead, get_initializes_lazily_and_never_acquires_again() (+8 more)

### Community 14 - "Worker HTTP Boundary"
Cohesion: 0.10
Nodes (17): DirectoryRejection, Busy, Completed, HostedGame, InvalidOperation, StaleOperation, Unavailable, UnknownOperation (+9 more)

### Community 15 - "Typed Environment Configuration"
Cohesion: 0.08
Nodes (21): APP_ORIGIN, APP_ORIGIN_MAX_BYTES, ARGON2_DEFAULT_ITERATIONS, ARGON2_DEFAULT_MEMORY_KIB, ARGON2_M_COST, ARGON2_MAX_ITERATIONS, ARGON2_MAX_MEMORY_KIB, ARGON2_MIN_ITERATIONS (+13 more)

### Community 16 - "Account Authentication Authority"
Cohesion: 0.17
Nodes (11): actor_key(), deadline(), AuthService<'_, D, R>, parse_work(), RemovalGateGrant, RemovalPhase, Aborting, Committed (+3 more)

### Community 17 - "Account Authentication Authority"
Cohesion: 0.33
Nodes (20): admin_session(), app_admin_self_disable_delete_and_enable_are_forbidden(), command_id(), committed_removal_phase_is_bound_and_persisted_in_real_sqlite(), deletion_does_not_require_an_unused_credential_epoch_increment(), deletion_requires_matching_gate_and_retains_intent_until_release_ack(), developer_cli_cannot_remove_the_last_enabled_verified_admin(), disable_changes_epoch_once_and_repeated_no_op_keeps_timestamp() (+12 more)

### Community 18 - "Account Authentication Authority"
Cohesion: 0.26
Nodes (4): AuthOutcome, add_deadline(), AuthService<'a, D, R>, token_digest()

### Community 19 - "Account Authentication Authority"
Cohesion: 0.12
Nodes (13): AuthService<'_, D, R>, DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE, page_size(), safe(), changes(), final_release_reproves_live_admin_without_writes_or_audit(), fixture() (+5 more)

### Community 20 - "Auth Test Coverage"
Cohesion: 0.25
Nodes (20): all_unknown_issuance_operations_use_the_exact_common_admission_floor(), auth_completion_id_conflicts_with_management_for_the_same_proven_actor(), command_id(), completion_rechecks_reciprocal_receipts_after_password_hashing(), context(), count(), create(), cross_operation_command_knowledge_never_substitutes_for_actor_proof() (+12 more)

### Community 21 - "Auth Test Coverage"
Cohesion: 0.17
Nodes (11): assert_verifier_rehashed(), assertion_failure_diagnostics_do_not_disclose_verifiers(), command_id(), context(), cookie(), create(), enrollment_and_reset_bind_status_purpose_and_scope_without_changing_stored_tags(), ORIGIN (+3 more)

### Community 23 - "Backend Workspace Structure"
Cohesion: 0.10
Nodes (11): AuditCommand, List, AuditPage, canonical_id(), Cli, DEFAULT_PAGE_ENTRIES, prepares_exact_operation_and_command_identity_once(), RootCommand (+3 more)

### Community 24 - "Auth Test Coverage"
Cohesion: 0.11
Nodes (4): enrollment_url(), link_token(), NativeSource, constructor_acquires_every_declared_key_once()

### Community 25 - "Typed Environment Configuration"
Cohesion: 0.16
Nodes (15): absent_public_settings_use_serde_defaults(), API_PATH, assert_redacted(), CONFIG, explicit_public_values_are_preserved(), FRONTEND_KEYS, get_frontend_config(), maximum_display_name_is_measured_in_utf8_bytes() (+7 more)

### Community 26 - "Shared Account Domain"
Cohesion: 0.16
Nodes (17): account_fields_reject_positional_sequences(), ACCOUNT_OBJECT, ACCOUNT_SEQUENCE, assert_invalid_response(), CreateUsername, every_public_variant_round_trips_with_safe_debug_and_http_status(), nested_accounts_preserve_duplicate_and_unknown_field_rejection(), nested_objects_preserve_exact_wire_bytes() (+9 more)

### Community 27 - "Account Authentication Authority"
Cohesion: 0.14
Nodes (11): AuthError, Conflict, Crypto, InvalidCredentials, InvalidInput, InvalidLink, RateLimited, StaleCommand (+3 more)

### Community 28 - "Worker HTTP Boundary"
Cohesion: 0.15
Nodes (8): deserialize_error_fields(), dispatch(), ErrorEnvelope, ErrorFields, handle(), log_delivery(), OwnerUsersRequest, validate_error()

### Community 29 - "Backend Workspace Structure"
Cohesion: 0.16
Nodes (13): execute(), MAX_PAGE_ENTRIES, prepare(), Prepared, run(), BREWS_API_ORIGIN, BREWS_DEV_CLI_KEY, BREWS_TLS_CA_FILE (+5 more)

### Community 30 - "Account Authentication Authority"
Cohesion: 0.19
Nodes (8): access_link_cleanup_and_alarm_share_the_exact_metadata_retention_deadline(), ACCOUNT_ID, cleanup_limits_each_delete_and_expired_close_enqueue_to_the_named_batch_size(), entity_id(), seed_account(), sql_params(), Sqlite, TestRuntime

### Community 31 - "Worker HTTP Boundary"
Cohesion: 0.19
Nodes (8): Bindings, get_backend_config(), AccountsObject, management_error(), management_result(), AccountsObject, safe_response_headers(), Request

### Community 32 - "Shared Account Domain"
Cohesion: 0.11
Nodes (18): replay_guidance(), AuditOperation, CreateAccount, DeleteAccount, DisableAccount, EnableAccount, GetAccount, ListAccounts (+10 more)

### Community 33 - "Backend Workspace Structure"
Cohesion: 0.11
Nodes (16): Failure, Configuration, Conflict, Crypto, Forbidden, InvalidCredentials, InvalidInput, InvalidLink (+8 more)

### Community 34 - "Account Authentication Authority"
Cohesion: 0.26
Nodes (6): AuthService<'_, D, R>, ACCOUNT_SELECT, integer(), optional_integer(), optional_text(), text()

### Community 36 - "Worker HTTP Boundary"
Cohesion: 0.29
Nodes (4): AuthService, OwnerDatabase, AccountsObject, WorkerRuntime

### Community 37 - "Backend Workspace Structure"
Cohesion: 0.14
Nodes (13): AccountCommand, Create, Delete, Disable, Enable, Get, List, ReissueEnrollment (+5 more)

### Community 38 - "Auth Test Coverage"
Cohesion: 0.15
Nodes (9): acquisition_can_only_request_the_declared_inventory(), assert_redacted(), dev_cli_key_is_optional_but_supplied_values_are_validated(), explicit_argon2_costs_are_deserialized_as_numbers(), keys_require_canonical_unpadded_base64url_for_exactly_32_bytes(), unsafe_or_noncanonical_origins_fail_closed(), unusable_supplied_numeric_values_never_become_defaults_or_leak(), valid_origins_are_stored_without_an_optional_root_slash() (+1 more)

### Community 39 - "Shared Account Domain"
Cohesion: 0.12
Nodes (11): UserResponse, Account, Committed, Created, Deleted, Disabled, Enabled, EnrollmentLink (+3 more)

### Community 40 - "Shared Account Domain"
Cohesion: 0.14
Nodes (14): AccessLinkPurpose, Enrollment, PasswordReset, ValidationError, PASSWORD_MAX_LEN, PASSWORD_MIN_LEN, SessionScope, EnrollmentOnly (+6 more)

### Community 41 - "Worker HTTP Boundary"
Cohesion: 0.17
Nodes (3): authenticate_cli(), CliRequest, decode_cli()

### Community 42 - "Backend Workspace Structure"
Cohesion: 0.15
Nodes (12): CliError, Configuration, Confirmation, HttpStatus, InvalidInput, LinkFile, Output, Protocol (+4 more)

### Community 43 - "Shared Account Domain"
Cohesion: 0.14
Nodes (8): AuthService<'_, D, R>, ManagementResponse, Account, Accounts, Audit, Committed, Issued, Pending

### Community 44 - "Backend Workspace Structure"
Cohesion: 0.18
Nodes (3): value(), WEB_CRYPTO_RANDOM_MAX_BYTES, value()

### Community 45 - "Durable SQLite Storage"
Cohesion: 0.16
Nodes (5): SqlValue, Blob, Integer, Null, Text

### Community 46 - "Backend Workspace Structure"
Cohesion: 0.20
Nodes (9): Delivery, Committed, Issued, Pending, management_delivery(), privileged_projection_omits_sensitive_spans_and_unrelated_events(), removal(), TARGET (+1 more)

### Community 47 - "Backend Workspace Structure"
Cohesion: 0.30
Nodes (7): failure_cleanup_truncates_only_owned_handle_not_replaced_path(), LinkFile, persists_link_without_displaying_or_reopening_path(), replaced_path_is_rejected_before_secret_write(), reserves_new_private_file_before_network(), weakened_permissions_or_hardlinks_fail_before_secret_write(), write_failure_is_typed_without_secret_sources()

### Community 48 - "Shared Account Domain"
Cohesion: 0.18
Nodes (8): AuditOutcome, Failed, Rejected, Succeeded, deserialize(), json_decode_errors_discard_values_and_parser_sources(), ManagementResponseDecodeError, receipt_version()

### Community 50 - "Backend Workspace Structure"
Cohesion: 0.19
Nodes (3): report_rehash_failure(), command(), decode_private_json()

### Community 51 - "Account Authentication Authority"
Cohesion: 0.19
Nodes (6): Account, Session, AccountStatus, PendingEnrollment, ResetRequired, Verified

### Community 52 - "Backend Workspace Structure"
Cohesion: 0.23
Nodes (8): api_failure(), auth_failure(), boundary_failures_have_closed_fields_and_expected_levels(), management_failure(), dispatch(), failure(), handle(), json_response()

### Community 53 - "Backend Workspace Structure"
Cohesion: 0.15
Nodes (12): Boundary, AccountsAlarm, AccountsAuth, AccountsManagement, AccountsPeer, AuthIngress, CliIngress, DirectoryAlarm (+4 more)

### Community 54 - "Shared Account Domain"
Cohesion: 0.19
Nodes (9): SafeAccount, SessionKind, Account, SessionView, serialize(), deserialize_users(), AccountRole, Admin (+1 more)

### Community 55 - "Backend Workspace Structure"
Cohesion: 0.17
Nodes (5): HTTP_TIMEOUT, ManagementClient, MAX_CA_PEM_BYTES, MAX_REQUEST_BYTES, MAX_RESPONSE_BYTES

### Community 56 - "Account Authentication Authority"
Cohesion: 0.18
Nodes (6): CookieEffect, Clear, None, Set, RequestContext, command_id()

### Community 57 - "Account Authentication Authority"
Cohesion: 0.20
Nodes (4): ManagementPrincipal, AdminSession, DeveloperCli, AuthService<'_, D, R>

### Community 58 - "Backend Workspace Structure"
Cohesion: 0.17
Nodes (3): Capture, init(), subscriber()

### Community 59 - "Backend Workspace Structure"
Cohesion: 0.32
Nodes (8): human_and_json_account_output_escape_all_controls(), issued_output_projects_metadata_without_link_secrets(), terminal_text(), terminal_text_escapes_username_controls(), write_error(), write_metadata(), write_notice(), write_response()

### Community 60 - "Account Authentication Authority"
Cohesion: 0.20
Nodes (7): CommandOutcome, CompleteEnrollment, CompletePasswordReset, Login, Redeem, fingerprint(), StoredOutcome

### Community 61 - "Durable SQLite Storage"
Cohesion: 0.31
Nodes (10): ACCOUNT_ID, auth_schema_ddl_matches_the_pre_refactor_snapshot(), auth_schema_statements(), connection(), ENTITY_ID, sqlite_account_caps_match_verifier_and_safe_integer_limits(), sqlite_rate_buckets_match_failure_count_and_digest_limits(), sqlite_receipts_match_payload_digest_and_retention_limits() (+2 more)

### Community 62 - "Backend Workspace Structure"
Cohesion: 0.35
Nodes (4): call_accounts(), dispatch(), handle(), OwnerManagementRequest

### Community 63 - "Worker HTTP Boundary"
Cohesion: 0.20
Nodes (9): ApiError, Forbidden, InvalidInput, MethodNotAllowed, NotFound, PayloadTooLarge, Unavailable, UnsupportedMediaType (+1 more)

### Community 64 - "Typed Environment Configuration"
Cohesion: 0.24
Nodes (3): SECRET_KEY_BYTES, SECRET_KEY_ENCODED_LEN, SecretKey

### Community 65 - "Worker HTTP Boundary"
Cohesion: 0.25
Nodes (6): CookieWire, Clear, None, Set, OwnerRequest, OwnerResponse

### Community 66 - "Worker HTTP Boundary"
Cohesion: 0.22
Nodes (8): Route, Account, Collection, Create, Disable, Enable, ReissueEnrollment, ResetPassword

### Community 67 - "Account Authentication Authority"
Cohesion: 0.22
Nodes (6): AuthCommand, Complete, Current, Login, Logout, Redeem

### Community 68 - "Account Authentication Authority"
Cohesion: 0.22
Nodes (3): TestRuntime, Runtime, TestRuntime

### Community 70 - "Worker HTTP Boundary"
Cohesion: 0.36
Nodes (4): account_get_preserves_the_canonical_typed_target(), read(), request_cleanup_zeroizes_its_optional_session_buffer_without_changing_command(), UsersRequest

### Community 71 - "Backend Workspace Structure"
Cohesion: 0.25
Nodes (7): RemovalEvent, Aborted, AbortStarted, AwaitingAcknowledgements, Committed, Completed, RetryScheduled

### Community 72 - "Typed Environment Configuration"
Cohesion: 0.25
Nodes (7): InvalidValueKind, ApiPath, Origin, OutOfRange, PublicText, SecretKey, TooLong

### Community 73 - "Backend Workspace Structure"
Cohesion: 0.33
Nodes (4): ENROLLMENT_PATH, LINK_TOKEN_BYTES, LINK_TOKEN_ENCODED_BYTES, PASSWORD_RESET_PATH

### Community 74 - "Typed Environment Configuration"
Cohesion: 0.60
Nodes (3): deserialize_error(), missing_values_only_identify_known_keys(), third_party_deserialization_errors_are_discarded()

### Community 77 - "Worker HTTP Boundary"
Cohesion: 0.67
Nodes (3): collection_query(), collection_query_byte_bound_precedes_parsing(), collection_query_maps_only_cursor_and_bounded_limit_in_either_order()

## Knowledge Gaps
- **336 isolated node(s):** `BODY_LIMIT`, `COOKIE_HEADER_MAX_BYTES`, `Login`, `Current`, `RedeemEnrollment` (+331 more)
  These have ≤1 connection - possible missing edges. (Counts symbols only; 593 node(s) total have ≤1 connection when file, concept and rationale nodes are included.)
- **12 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `ManagementError` connect `Account Authentication Authority` to `Durable SQLite Storage`, `Account Authentication Authority`?**
  _High betweenness centrality (0.002) - this node is a cross-community bridge._
- **Why does `AuthError` connect `Account Authentication Authority` to `Durable SQLite Storage`?**
  _High betweenness centrality (0.001) - this node is a cross-community bridge._
- **Why does `ManagementResponse` connect `Shared Account Domain` to `Shared Account Domain`, `Shared Account Domain`, `Shared Account Domain`?**
  _High betweenness centrality (0.001) - this node is a cross-community bridge._
- **What connects `BODY_LIMIT`, `COOKIE_HEADER_MAX_BYTES`, `Login` to the rest of the system?**
  _336 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Auth Test Coverage` be split into smaller, more focused modules?**
  _Cohesion score 0.07965784549585672 - nodes in this community are weakly interconnected._
- **Should `Auth Test Coverage` be split into smaller, more focused modules?**
  _Cohesion score 0.10602503912363068 - nodes in this community are weakly interconnected._
- **Should `Durable SQLite Storage` be split into smaller, more focused modules?**
  _Cohesion score 0.10235690235690235 - nodes in this community are weakly interconnected._