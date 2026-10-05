# Scope: code-only structural graph

Rust/JavaScript source and tests only. No semantic documentation refresh; the older graph in the parent directory is not marked current. No LLM token cost for this extraction.

AST health warnings: {"dangling_endpoint_edges": 266, "directed_same_endpoint_collapsed_edges": 88}. Unresolved references are not verified relationships; structural-extraction.json retains raw edge evidence.

# Graph Report - brews-bingo  (2026-10-05)

## Corpus Check
- 104 files · ~51,836 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 1428 nodes · 3295 edges · 87 communities (66 shown, 21 thin omitted)
- Extraction: 93% EXTRACTED · 7% INFERRED · 0% AMBIGUOUS · INFERRED: 220 edges (avg confidence: 0.85)
- Token cost: 0 input · 0 output

## Community Hubs (Navigation)
- Auth Test Coverage
- Auth Test Coverage
- Backend Workspace Structure
- Auth Test Coverage
- Account Authentication Authority
- Auth Test Coverage
- Backend Workspace Structure
- Credential Cryptography
- Shared Account Domain
- Account Authentication Authority
- Worker HTTP Boundary
- Typed Environment Configuration
- Shared Account Domain
- Typed Environment Configuration
- Account Authentication Authority
- Account Authentication Authority
- Auth Test Coverage
- Auth Test Coverage
- Auth Test Coverage
- Backend Workspace Structure
- Account Authentication Authority
- Account Authentication Authority
- Account Authentication Authority
- Backend Workspace Structure
- Backend Workspace Structure
- Auth Test Coverage
- Backend Workspace Structure
- Worker HTTP Boundary
- Worker HTTP Boundary
- Worker HTTP Boundary
- Backend Workspace Structure
- Auth Test Coverage
- Account Authentication Authority
- Backend Workspace Structure
- Account Authentication Authority
- Backend Workspace Structure
- Worker HTTP Boundary
- Account Authentication Authority
- Account Authentication Authority
- Worker HTTP Boundary
- Backend Workspace Structure
- Backend Workspace Structure
- Durable SQLite Storage
- Durable SQLite Storage
- Backend Workspace Structure
- Typed Environment Configuration
- Backend Workspace Structure
- Account Authentication Authority
- Durable SQLite Storage
- Account Authentication Authority
- Backend Workspace Structure
- Typed Environment Configuration
- Shared Account Domain
- Shared Account Domain
- Worker HTTP Boundary
- Backend Workspace Structure
- Worker HTTP Boundary
- Auth Test Coverage
- Shared Account Domain
- Backend Workspace Structure
- Worker HTTP Boundary
- Account Authentication Authority
- Shared Account Domain
- Backend Workspace Structure
- Backend Workspace Structure
- Worker HTTP Boundary
- Backend Workspace Structure
- Typed Environment Configuration
- Worker HTTP Boundary
- Account Authentication Authority
- Backend Workspace Structure
- Backend Workspace Structure
- Backend Workspace Structure
- Typed Environment Configuration
- Backend Workspace Structure
- Account Authentication Authority
- Typed Environment Configuration
- Shared Account Domain
- Auth Test Coverage
- Typed Environment Configuration
- Shared Account Domain

## God Nodes (most connected - your core abstractions)
1. `migrate()` - 91 edges
2. `AuthError` - 68 edges
3. `ManagementError` - 67 edges
4. `migrate_directory()` - 64 edges
5. `operation()` - 61 edges
6. `account()` - 56 edges
7. `ConfigError` - 46 edges
8. `DirectoryError` - 37 edges
9. `ManagementResponse` - 37 edges
10. `ManagementCommand` - 36 edges

## Surprising Connections (you probably didn't know these)
- `sqlite_username_boundaries_match_domain_validation_on_insert_and_update()` --calls--> `validate_username()`  [INFERRED]
  backend/src/storage/schema.rs → shared/domain/src/accounts.rs
- `CliRequest` --references--> `ManagementCommand`  [EXTRACTED]
  backend/src/api/developer.rs → shared/contracts/src/management.rs
- `action()` --references--> `AuditOperation`  [EXTRACTED]
  backend/src/auth/management/audit.rs → shared/contracts/src/management.rs
- `action()` --references--> `AuditTarget`  [EXTRACTED]
  backend/src/auth/management/audit.rs → shared/contracts/src/management.rs
- `action()` --references--> `ManagementCommand`  [EXTRACTED]
  backend/src/auth/management/audit.rs → shared/contracts/src/management.rs

## Import Cycles
- None detected.

## Communities (87 total, 21 thin omitted)

### Community 0 - "Auth Test Coverage"
Cohesion: 0.08
Nodes (72): migrate(), a_current_host_session_is_attributed_when_admin_authority_is_denied(), account_detail_is_safe_and_deleted_or_unknown_target_is_not_found(), account_pages_are_bounded_and_continue_without_duplicates(), audit_write_failure_rolls_back_account_link_and_receipt(), authenticated_rejections_are_durably_audited(), command_id(), create() (+64 more)

### Community 1 - "Auth Test Coverage"
Cohesion: 0.11
Nodes (63): migrate_directory(), account(), acquire_persists_a_presence_only_gate_with_separate_operation_identity(), acquired_gate_rejects_transfer_without_changing_the_original_host(), competing_operation_returns_busy_without_replacing_the_current_gate(), completed_operation_cannot_reacquire_or_clear_a_newer_gate(), concurrent_operations_on_one_account_serialize_to_one_grant_and_one_busy(), corrupted_completion_time_cannot_erase_the_fence_or_authorize_an_ack() (+55 more)

### Community 2 - "Backend Workspace Structure"
Cohesion: 0.10
Nodes (22): DirectoryError, AssignmentBlocked, Busy, Clock, Completed, HostedGame, OperationMismatch, StaleOperation (+14 more)

### Community 3 - "Auth Test Coverage"
Cohesion: 0.07
Nodes (32): main(), assert_issued_handoff(), audit_reads_preserve_required_targets_and_read_operations(), authorization_debug_is_redacted_and_server_errors_discard_bodies(), BrokenPipe, ca_file_read_is_bounded_and_errors_are_redacted(), client_rejects_noncanonical_or_non_https_origins_before_network(), command_runner_exercises_real_https_to_safe_metadata() (+24 more)

### Community 4 - "Account Authentication Authority"
Cohesion: 0.07
Nodes (22): AuthService<'_, D, R>, AuthService<'_, D, R>, deadline(), ManagementError, Busy, Conflict, Crypto, Forbidden (+14 more)

### Community 5 - "Auth Test Coverage"
Cohesion: 0.14
Nodes (22): directoryStub(), initializeDirectory(), commandId(), cookie(), current(), enroll(), inOwner(), origin (+14 more)

### Community 6 - "Backend Workspace Structure"
Cohesion: 0.06
Nodes (31): ACCESS_LINK_LIFETIME_MS, ACCESS_LINK_METADATA_RETENTION_MS, CALLER_IDENTITY_MAX_BYTES, CLEANUP_BATCH_SIZE, COMMAND_RECEIPT_MAX_BYTES, COMMAND_RECEIPT_RETENTION_MS, JS_SAFE_INTEGER_MAX, LOGIN_VERIFICATION_MAX_ATTEMPTS (+23 more)

### Community 7 - "Credential Cryptography"
Cohesion: 0.07
Nodes (21): TestRuntime, AuthPolicy, Runtime, bounded_phc(), DIGEST_BYTES, hash_password(), MAX_PHC_LEN, new_token() (+13 more)

### Community 8 - "Shared Account Domain"
Cohesion: 0.07
Nodes (22): AuthService<'_, D, R>, AuditEvent, AuditTarget, Account, AccountsOwner, ManagementCommand, CreateAccount, DeleteAccount (+14 more)

### Community 9 - "Account Authentication Authority"
Cohesion: 0.22
Nodes (5): AuthOutcome, RequestContext, add_deadline(), AuthService<'a, D, R>, token_digest()

### Community 10 - "Worker HTTP Boundary"
Cohesion: 0.10
Nodes (17): DirectoryRejection, Busy, Completed, HostedGame, InvalidOperation, StaleOperation, Unavailable, UnknownOperation (+9 more)

### Community 11 - "Typed Environment Configuration"
Cohesion: 0.08
Nodes (21): APP_ORIGIN, APP_ORIGIN_MAX_BYTES, ARGON2_DEFAULT_ITERATIONS, ARGON2_DEFAULT_MEMORY_KIB, ARGON2_M_COST, ARGON2_MAX_ITERATIONS, ARGON2_MAX_MEMORY_KIB, ARGON2_MIN_ITERATIONS (+13 more)

### Community 12 - "Shared Account Domain"
Cohesion: 0.08
Nodes (21): AuthCommand, Complete, Current, Login, Logout, Redeem, AccessLinkPurpose, Enrollment (+13 more)

### Community 13 - "Typed Environment Configuration"
Cohesion: 0.13
Nodes (21): ConfigError, Deserialization, InvalidValue, MissingKey, absent_public_settings_use_serde_defaults(), API_PATH, assert_redacted(), CONFIG (+13 more)

### Community 14 - "Account Authentication Authority"
Cohesion: 0.19
Nodes (10): actor_key(), AuthService<'_, D, R>, RemovalGateGrant, RemovalPhase, Aborting, Committed, Prepared, RemovalReleaseAck (+2 more)

### Community 15 - "Account Authentication Authority"
Cohesion: 0.34
Nodes (20): admin_session(), app_admin_self_disable_delete_and_enable_are_forbidden(), command_id(), committed_removal_phase_is_bound_and_persisted_in_real_sqlite(), deletion_does_not_require_an_unused_credential_epoch_increment(), deletion_requires_matching_gate_and_retains_intent_until_release_ack(), developer_cli_cannot_remove_the_last_enabled_verified_admin(), disable_changes_epoch_once_and_repeated_no_op_keeps_timestamp() (+12 more)

### Community 16 - "Auth Test Coverage"
Cohesion: 0.25
Nodes (20): all_unknown_issuance_operations_use_the_exact_common_admission_floor(), auth_completion_id_conflicts_with_management_for_the_same_proven_actor(), command_id(), completion_rechecks_reciprocal_receipts_after_password_hashing(), context(), count(), create(), cross_operation_command_knowledge_never_substitutes_for_actor_proof() (+12 more)

### Community 17 - "Auth Test Coverage"
Cohesion: 0.19
Nodes (11): assert_verifier_rehashed(), assertion_failure_diagnostics_do_not_disclose_verifiers(), command_id(), context(), cookie(), create(), enrollment_and_reset_bind_status_purpose_and_scope_without_changing_stored_tags(), ORIGIN (+3 more)

### Community 18 - "Auth Test Coverage"
Cohesion: 0.10
Nodes (8): failed_get_caches_a_safe_error_without_retrying(), NeverRead, constructor_is_zero_argument_and_uses_only_public_build_inputs(), public_singleton_caches_the_constructor_outcome(), get_initializes_the_public_config_and_returns_the_same_instance(), racing_backend_get_calls_share_one_cached_failure(), backend_failure_does_not_poison_the_public_singleton(), constructor_acquires_every_declared_key_once()

### Community 19 - "Backend Workspace Structure"
Cohesion: 0.11
Nodes (10): AuditCommand, List, AuditPage, canonical_id(), Cli, DEFAULT_PAGE_ENTRIES, prepares_exact_operation_and_command_identity_once(), RootCommand (+2 more)

### Community 20 - "Account Authentication Authority"
Cohesion: 0.14
Nodes (11): AuthError, Conflict, Crypto, InvalidCredentials, InvalidInput, InvalidLink, RateLimited, StaleCommand (+3 more)

### Community 22 - "Account Authentication Authority"
Cohesion: 0.19
Nodes (8): access_link_cleanup_and_alarm_share_the_exact_metadata_retention_deadline(), ACCOUNT_ID, cleanup_limits_each_delete_and_expired_close_enqueue_to_the_named_batch_size(), entity_id(), seed_account(), sql_params(), Sqlite, TestRuntime

### Community 23 - "Backend Workspace Structure"
Cohesion: 0.16
Nodes (4): dispatch(), handle(), json_response(), decode_private_json()

### Community 24 - "Backend Workspace Structure"
Cohesion: 0.11
Nodes (16): Failure, Configuration, Conflict, Crypto, Forbidden, InvalidCredentials, InvalidInput, InvalidLink (+8 more)

### Community 25 - "Auth Test Coverage"
Cohesion: 0.14
Nodes (3): enrollment_url(), link_token(), NativeSource

### Community 28 - "Worker HTTP Boundary"
Cohesion: 0.17
Nodes (12): assert_array_root_rejected(), BODY_LIMIT, CompleteBody, COOKIE_HEADER_MAX_BYTES, enrollment_complete_rejects_positional_json_array_root(), enrollment_redeem_rejects_positional_json_array_root(), EnrollmentBody, login_rejects_positional_json_array_root() (+4 more)

### Community 29 - "Worker HTTP Boundary"
Cohesion: 0.28
Nodes (4): AuthService, OwnerDatabase, AccountsObject, WorkerRuntime

### Community 30 - "Backend Workspace Structure"
Cohesion: 0.14
Nodes (13): AccountCommand, Create, Delete, Disable, Enable, Get, List, ReissueEnrollment (+5 more)

### Community 31 - "Auth Test Coverage"
Cohesion: 0.15
Nodes (9): acquisition_can_only_request_the_declared_inventory(), assert_redacted(), dev_cli_key_is_optional_but_supplied_values_are_validated(), explicit_argon2_costs_are_deserialized_as_numbers(), keys_require_canonical_unpadded_base64url_for_exactly_32_bytes(), unsafe_or_noncanonical_origins_fail_closed(), unusable_supplied_numeric_values_never_become_defaults_or_leak(), valid_origins_are_stored_without_an_optional_root_slash() (+1 more)

### Community 32 - "Account Authentication Authority"
Cohesion: 0.13
Nodes (4): CookieEffect, Clear, None, Set

### Community 33 - "Backend Workspace Structure"
Cohesion: 0.15
Nodes (12): CliError, Configuration, Confirmation, HttpStatus, InvalidInput, LinkFile, Output, Protocol (+4 more)

### Community 34 - "Account Authentication Authority"
Cohesion: 0.30
Nodes (6): parse_work(), ACCOUNT_SELECT, integer(), optional_integer(), optional_text(), text()

### Community 35 - "Backend Workspace Structure"
Cohesion: 0.30
Nodes (7): failure_cleanup_truncates_only_owned_handle_not_replaced_path(), LinkFile, persists_link_without_displaying_or_reopening_path(), replaced_path_is_rejected_before_secret_write(), reserves_new_private_file_before_network(), weakened_permissions_or_hardlinks_fail_before_secret_write(), write_failure_is_typed_without_secret_sources()

### Community 36 - "Worker HTTP Boundary"
Cohesion: 0.21
Nodes (3): authenticate_cli(), CliRequest, decode_cli()

### Community 37 - "Account Authentication Authority"
Cohesion: 0.21
Nodes (7): AuthService<'_, D, R>, ManagementPrincipal, AdminSession, DeveloperCli, AuditActor, Account, DeveloperCli

### Community 38 - "Account Authentication Authority"
Cohesion: 0.19
Nodes (6): Account, Session, AccountStatus, PendingEnrollment, ResetRequired, Verified

### Community 39 - "Worker HTTP Boundary"
Cohesion: 0.24
Nodes (10): all_seven_operations_accept_their_exact_payload(), cookie_is_read_and_duplicate_session_values_reject(), decode(), get_current_is_bodyless_and_has_no_origin_or_retry_requirement(), headers(), login_requires_canonical_uuid_v7_retry_key(), non_json_content_type_is_rejected(), post_requires_exact_single_origin() (+2 more)

### Community 40 - "Backend Workspace Structure"
Cohesion: 0.23
Nodes (8): api_failure(), auth_failure(), boundary_failures_have_closed_fields_and_expected_levels(), init(), management_failure(), subscriber(), TARGET, failure()

### Community 41 - "Backend Workspace Structure"
Cohesion: 0.17
Nodes (11): Boundary, AccountsAlarm, AccountsAuth, AccountsManagement, AccountsPeer, AuthIngress, CliIngress, DirectoryAlarm (+3 more)

### Community 42 - "Durable SQLite Storage"
Cohesion: 0.20
Nodes (5): CURRENT_SCHEMA_VERSION, Database, metadata_clock_bounds_match_the_named_safe_integer_limit(), metadata_statement(), VERSION_ONE

### Community 43 - "Durable SQLite Storage"
Cohesion: 0.20
Nodes (5): SqlValue, Blob, Integer, Null, Text

### Community 44 - "Backend Workspace Structure"
Cohesion: 0.32
Nodes (8): human_and_json_account_output_escape_all_controls(), issued_output_projects_metadata_without_link_secrets(), terminal_text(), terminal_text_escapes_username_controls(), write_error(), write_metadata(), write_notice(), write_response()

### Community 45 - "Typed Environment Configuration"
Cohesion: 0.21
Nodes (8): BREWS_API_ORIGIN, BREWS_DEV_CLI_KEY, BREWS_TLS_CA_FILE, CLI_KEYS, CLI_ORIGIN_MAX_BYTES, CliConfig, CONFIG, get_cli_config()

### Community 47 - "Backend Workspace Structure"
Cohesion: 0.35
Nodes (4): call_accounts(), dispatch(), handle(), OwnerManagementRequest

### Community 48 - "Account Authentication Authority"
Cohesion: 0.20
Nodes (7): CommandOutcome, CompleteEnrollment, CompletePasswordReset, Login, Redeem, fingerprint(), StoredOutcome

### Community 49 - "Durable SQLite Storage"
Cohesion: 0.31
Nodes (10): ACCOUNT_ID, connection(), ENTITY_ID, sqlite_account_caps_match_verifier_and_safe_integer_limits(), sqlite_rate_buckets_match_failure_count_and_digest_limits(), sqlite_receipts_match_payload_digest_and_retention_limits(), sqlite_session_and_link_deadlines_use_their_own_policy_limits(), sqlite_username_boundaries_match_domain_validation_on_insert_and_update() (+2 more)

### Community 50 - "Account Authentication Authority"
Cohesion: 0.31
Nodes (4): AuthService<'_, D, R>, fingerprint(), validate_receipt(), ManagementReceipt

### Community 51 - "Backend Workspace Structure"
Cohesion: 0.22
Nodes (8): Delivery, Committed, Issued, Pending, management_delivery(), privileged_projection_omits_sensitive_spans_and_unrelated_events(), removal(), typed_lifecycle_records_use_closed_fields_and_canonical_ids()

### Community 52 - "Typed Environment Configuration"
Cohesion: 0.24
Nodes (3): SECRET_KEY_BYTES, SECRET_KEY_ENCODED_LEN, SecretKey

### Community 53 - "Shared Account Domain"
Cohesion: 0.27
Nodes (4): deserialize(), json_decode_errors_discard_values_and_parser_sources(), ManagementResponseDecodeError, receipt_version()

### Community 54 - "Shared Account Domain"
Cohesion: 0.20
Nodes (10): AuditOperation, CreateAccount, DeleteAccount, DisableAccount, EnableAccount, GetAccount, ListAccounts, ListAudit (+2 more)

### Community 55 - "Worker HTTP Boundary"
Cohesion: 0.22
Nodes (8): ApiError, Forbidden, InvalidInput, MethodNotAllowed, NotFound, PayloadTooLarge, Unavailable, UnsupportedMediaType

### Community 58 - "Worker HTTP Boundary"
Cohesion: 0.25
Nodes (6): CookieWire, Clear, None, Set, OwnerRequest, OwnerResponse

### Community 60 - "Shared Account Domain"
Cohesion: 0.22
Nodes (8): replay_guidance(), ReceiptOperation, CreateAccount, DeleteAccount, DisableAccount, EnableAccount, ReissueEnrollment, ResetPassword

### Community 62 - "Worker HTTP Boundary"
Cohesion: 0.25
Nodes (8): Operation, CompleteEnrollment, CompleteReset, Current, Login, Logout, RedeemEnrollment, RedeemReset

### Community 63 - "Account Authentication Authority"
Cohesion: 0.29
Nodes (4): AuthService<'_, D, R>, DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE, page_size()

### Community 64 - "Shared Account Domain"
Cohesion: 0.32
Nodes (5): safe(), SafeAccount, SessionKind, Account, SessionView

### Community 66 - "Backend Workspace Structure"
Cohesion: 0.25
Nodes (7): RemovalEvent, Aborted, AbortStarted, AwaitingAcknowledgements, Committed, Completed, RetryScheduled

### Community 67 - "Worker HTTP Boundary"
Cohesion: 0.43
Nodes (4): AccountsObject, management_error(), management_result(), Request

### Community 68 - "Backend Workspace Structure"
Cohesion: 0.46
Nodes (5): execute(), MAX_PAGE_ENTRIES, prepare(), Prepared, run()

### Community 69 - "Typed Environment Configuration"
Cohesion: 0.25
Nodes (7): InvalidValueKind, ApiPath, Origin, OutOfRange, PublicText, SecretKey, TooLong

### Community 70 - "Worker HTTP Boundary"
Cohesion: 0.29
Nodes (7): Decoded, Payload, Complete, Empty, Login, Redeem, command()

### Community 72 - "Backend Workspace Structure"
Cohesion: 0.29
Nodes (4): HTTP_TIMEOUT, MAX_CA_PEM_BYTES, MAX_REQUEST_BYTES, MAX_RESPONSE_BYTES

### Community 74 - "Backend Workspace Structure"
Cohesion: 0.33
Nodes (4): ENROLLMENT_PATH, LINK_TOKEN_BYTES, LINK_TOKEN_ENCODED_BYTES, PASSWORD_RESET_PATH

### Community 78 - "Typed Environment Configuration"
Cohesion: 0.60
Nodes (3): deserialize_error(), missing_values_only_identify_known_keys(), third_party_deserialization_errors_are_discarded()

### Community 79 - "Shared Account Domain"
Cohesion: 0.40
Nodes (4): serialize(), AccountRole, Admin, Host

## Knowledge Gaps
- **304 isolated node(s):** `BODY_LIMIT`, `COOKIE_HEADER_MAX_BYTES`, `Login`, `Current`, `RedeemEnrollment` (+299 more)
  These have ≤1 connection - possible missing edges. (Counts symbols only; 531 node(s) total have ≤1 connection when file, concept and rationale nodes are included.)
- **21 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `ManagementResponse` connect `Shared Account Domain` to `Shared Account Domain`, `Account Authentication Authority`, `Shared Account Domain`?**
  _High betweenness centrality (0.002) - this node is a cross-community bridge._
- **Why does `ManagementError` connect `Account Authentication Authority` to `Account Authentication Authority`, `Account Authentication Authority`?**
  _High betweenness centrality (0.002) - this node is a cross-community bridge._
- **Why does `ManagementCommand` connect `Shared Account Domain` to `Shared Account Domain`?**
  _High betweenness centrality (0.001) - this node is a cross-community bridge._
- **What connects `BODY_LIMIT`, `COOKIE_HEADER_MAX_BYTES`, `Login` to the rest of the system?**
  _304 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Auth Test Coverage` be split into smaller, more focused modules?**
  _Cohesion score 0.07965784549585672 - nodes in this community are weakly interconnected._
- **Should `Auth Test Coverage` be split into smaller, more focused modules?**
  _Cohesion score 0.10602503912363068 - nodes in this community are weakly interconnected._
- **Should `Backend Workspace Structure` be split into smaller, more focused modules?**
  _Cohesion score 0.09523809523809523 - nodes in this community are weakly interconnected._