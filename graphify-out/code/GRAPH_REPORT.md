# Scope: code-only structural graph

Rust/JavaScript source and tests only. No semantic documentation refresh; the older graph in the parent directory is not marked current. No LLM token cost for this extraction.

AST health warnings: {"dangling_endpoint_edges": 123, "directed_same_endpoint_collapsed_edges": 22}. Unresolved references are not verified relationships; structural-extraction.json retains raw edge evidence.

# Graph Report - brews-bingo  (2026-10-05)

## Corpus Check
- 58 files · ~17,364 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 606 nodes · 1223 edges · 32 communities (24 shown, 8 thin omitted)
- Extraction: 95% EXTRACTED · 5% INFERRED · 0% AMBIGUOUS · INFERRED: 64 edges (avg confidence: 0.85)
- Token cost: 0 input · 0 output

## Community Hubs (Navigation)
- Account Authentication Authority
- Worker HTTP Boundary
- Worker HTTP Boundary
- Shared Account Domain
- Auth Test Coverage
- Typed Environment Configuration
- Account Authentication Authority
- Backend Workspace Structure
- Auth Test Coverage
- Typed Environment Configuration
- Account Authentication Authority
- Credential Cryptography
- Worker HTTP Boundary
- Worker HTTP Boundary
- Auth Test Coverage
- Shared Account Domain
- Auth Test Coverage
- Auth Test Coverage
- Auth Test Coverage
- Account Authentication Authority
- Durable SQLite Storage
- Durable SQLite Storage
- Worker HTTP Boundary
- Auth Test Coverage
- Typed Environment Configuration
- Auth Test Coverage
- Typed Environment Configuration
- Shared Account Domain

## God Nodes (most connected - your core abstractions)
1. `AuthError` - 63 edges
2. `ConfigError` - 41 edges
3. `migrate()` - 34 edges
4. `context()` - 21 edges
5. `seed()` - 20 edges
6. `AuthService<'a, D, R>` - 19 edges
7. `StorageError` - 18 edges
8. `SqlValue` - 17 edges
9. `ApiError` - 15 edges
10. `login()` - 15 edges

## Surprising Connections (you probably didn't know these)
- `Account` --references--> `AccountRole`  [EXTRACTED]
  backend/src/auth/records.rs → shared/domain/src/accounts.rs
- `Account` --references--> `AccountStatus`  [EXTRACTED]
  backend/src/auth/records.rs → shared/domain/src/accounts.rs
- `Session` --references--> `SessionScope`  [EXTRACTED]
  backend/src/auth/records.rs → shared/domain/src/accounts.rs
- `get_backend_config()` --references--> `ConfigError`  [EXTRACTED]
  backend/src/config/mod.rs → shared/config/src/env/error.rs
- `sqlite_username_boundaries_match_domain_validation_on_insert_and_update()` --calls--> `validate_username()`  [INFERRED]
  backend/src/storage/schema.rs → shared/domain/src/accounts.rs

## Import Cycles
- None detected.

## Communities (32 total, 8 thin omitted)

### Community 0 - "Account Authentication Authority"
Cohesion: 0.07
Nodes (32): AuthError, Conflict, Crypto, InvalidCredentials, InvalidInput, InvalidLink, RateLimited, StaleCommand (+24 more)

### Community 1 - "Worker HTTP Boundary"
Cohesion: 0.07
Nodes (36): all_seven_operations_accept_their_exact_payload(), assert_array_root_rejected(), BODY_LIMIT, CompleteBody, COOKIE_HEADER_MAX_BYTES, cookie_is_read_and_duplicate_session_values_reject(), decode(), Decoded (+28 more)

### Community 2 - "Worker HTTP Boundary"
Cohesion: 0.07
Nodes (16): ApiError, Forbidden, InvalidInput, MethodNotAllowed, NotFound, PayloadTooLarge, Unavailable, UnsupportedMediaType (+8 more)

### Community 3 - "Shared Account Domain"
Cohesion: 0.06
Nodes (30): AuthCommand, Complete, Current, Login, Logout, Redeem, ACCOUNT_ID, connection() (+22 more)

### Community 4 - "Auth Test Coverage"
Cohesion: 0.16
Nodes (34): migrate(), ACCOUNT, bearer(), cleanup_keeps_old_commands_rejected_after_service_restart_and_clock_rollback(), command_admission_rejects_expired_retry_before_and_after_exact_cleanup_restart(), command_admission_treats_initial_zero_floor_as_a_stale_cutoff(), completion_advances_epoch_once_with_purpose_specific_session_effects(), completion_receipt_failure_rolls_back_the_entire_credential_transition() (+26 more)

### Community 5 - "Typed Environment Configuration"
Cohesion: 0.07
Nodes (24): Bindings, get_backend_config(), APP_ORIGIN, APP_ORIGIN_MAX_BYTES, ARGON2_DEFAULT_ITERATIONS, ARGON2_DEFAULT_MEMORY_KIB, ARGON2_M_COST, ARGON2_MAX_ITERATIONS (+16 more)

### Community 6 - "Account Authentication Authority"
Cohesion: 0.17
Nodes (8): AuthService<'_, D, R>, Account, ACCOUNT_SELECT, integer(), optional_integer(), optional_text(), Session, text()

### Community 7 - "Backend Workspace Structure"
Cohesion: 0.08
Nodes (23): ACCESS_LINK_LIFETIME_MS, ACCESS_LINK_METADATA_RETENTION_MS, CALLER_IDENTITY_MAX_BYTES, CLEANUP_BATCH_SIZE, COMMAND_RECEIPT_MAX_BYTES, COMMAND_RECEIPT_RETENTION_MS, JS_SAFE_INTEGER_MAX, LOGIN_VERIFICATION_MAX_ATTEMPTS (+15 more)

### Community 8 - "Auth Test Coverage"
Cohesion: 0.28
Nodes (12): commandId(), cookie(), current(), enroll(), inOwner(), origin, password(), post() (+4 more)

### Community 9 - "Typed Environment Configuration"
Cohesion: 0.16
Nodes (15): absent_public_settings_use_serde_defaults(), API_PATH, assert_redacted(), CONFIG, explicit_public_values_are_preserved(), FRONTEND_KEYS, get_frontend_config(), maximum_display_name_is_measured_in_utf8_bytes() (+7 more)

### Community 10 - "Account Authentication Authority"
Cohesion: 0.19
Nodes (8): access_link_cleanup_and_alarm_share_the_exact_metadata_retention_deadline(), ACCOUNT_ID, cleanup_limits_each_delete_and_expired_close_enqueue_to_the_named_batch_size(), entity_id(), seed_account(), sql_params(), Sqlite, TestRuntime

### Community 11 - "Credential Cryptography"
Cohesion: 0.12
Nodes (9): DIGEST_BYTES, MAX_PHC_LEN, PASSWORD_HASH_BYTES, PASSWORD_SALT_BYTES, PHC_PARAMETER_COUNT, PHC_VERSION, SALT_DECODE_BUFFER_BYTES, TOKEN_BYTES (+1 more)

### Community 12 - "Worker HTTP Boundary"
Cohesion: 0.18
Nodes (3): Storage, StorageError, OwnerDatabase

### Community 14 - "Auth Test Coverage"
Cohesion: 0.14
Nodes (3): Sqlite, TestRuntime, value()

### Community 15 - "Shared Account Domain"
Cohesion: 0.16
Nodes (11): SafeAccount, SessionKind, Account, SessionView, AccountRole, Admin, Host, AccountStatus (+3 more)

### Community 16 - "Auth Test Coverage"
Cohesion: 0.19
Nodes (9): ConfigError, Deserialization, InvalidValue, MissingKey, failed_get_caches_a_safe_error_without_retrying(), NeverRead, racing_backend_get_calls_share_one_cached_failure(), racing_backend_get_calls_share_one_acquisition() (+1 more)

### Community 17 - "Auth Test Coverage"
Cohesion: 0.14
Nodes (6): get_initializes_lazily_and_never_acquires_again(), NeverRead, constructor_is_zero_argument_and_uses_only_public_build_inputs(), public_singleton_caches_the_constructor_outcome(), get_initializes_the_public_config_and_returns_the_same_instance(), backend_failure_does_not_poison_the_public_singleton()

### Community 18 - "Auth Test Coverage"
Cohesion: 0.18
Nodes (8): acquisition_can_only_request_the_declared_inventory(), assert_redacted(), explicit_argon2_costs_are_deserialized_as_numbers(), keys_require_canonical_unpadded_base64url_for_exactly_32_bytes(), unsafe_or_noncanonical_origins_fail_closed(), unusable_supplied_numeric_values_never_become_defaults_or_leak(), valid_origins_are_stored_without_an_optional_root_slash(), validation_failure_still_acquires_each_declared_key_once()

### Community 19 - "Account Authentication Authority"
Cohesion: 0.22
Nodes (7): CommandOutcome, CompleteEnrollment, CompletePasswordReset, Login, Redeem, fingerprint(), StoredOutcome

### Community 20 - "Durable SQLite Storage"
Cohesion: 0.24
Nodes (4): CURRENT_SCHEMA_VERSION, Database, metadata_clock_bounds_match_the_named_safe_integer_limit(), metadata_statement()

### Community 21 - "Durable SQLite Storage"
Cohesion: 0.22
Nodes (5): SqlValue, Blob, Integer, Null, Text

### Community 22 - "Worker HTTP Boundary"
Cohesion: 0.25
Nodes (6): CookieWire, Clear, None, Set, OwnerRequest, OwnerResponse

### Community 24 - "Typed Environment Configuration"
Cohesion: 0.25
Nodes (7): InvalidValueKind, ApiPath, Origin, OutOfRange, PublicText, SecretKey, TooLong

### Community 27 - "Typed Environment Configuration"
Cohesion: 0.60
Nodes (3): deserialize_error(), missing_values_only_identify_known_keys(), third_party_deserialization_errors_are_discarded()

## Knowledge Gaps
- **136 isolated node(s):** `BODY_LIMIT`, `COOKIE_HEADER_MAX_BYTES`, `Login`, `Current`, `RedeemEnrollment` (+131 more)
  These have ≤1 connection - possible missing edges. (Counts symbols only; 232 node(s) total have ≤1 connection when file, concept and rationale nodes are included.)
- **8 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `AuthError` connect `Account Authentication Authority` to `Worker HTTP Boundary`?**
  _High betweenness centrality (0.003) - this node is a cross-community bridge._
- **Why does `ConfigError` connect `Auth Test Coverage` to `Typed Environment Configuration`?**
  _High betweenness centrality (0.002) - this node is a cross-community bridge._
- **What connects `BODY_LIMIT`, `COOKIE_HEADER_MAX_BYTES`, `Login` to the rest of the system?**
  _136 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Account Authentication Authority` be split into smaller, more focused modules?**
  _Cohesion score 0.06523855890944498 - nodes in this community are weakly interconnected._
- **Should `Worker HTTP Boundary` be split into smaller, more focused modules?**
  _Cohesion score 0.06666666666666667 - nodes in this community are weakly interconnected._
- **Should `Worker HTTP Boundary` be split into smaller, more focused modules?**
  _Cohesion score 0.06976744186046512 - nodes in this community are weakly interconnected._
- **Should `Shared Account Domain` be split into smaller, more focused modules?**
  _Cohesion score 0.06039488966318235 - nodes in this community are weakly interconnected._