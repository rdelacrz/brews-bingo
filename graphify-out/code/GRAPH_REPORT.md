# Graph Report - /home/orangepi/Workspace/brews-bingo  (2026-10-17)

## Corpus Check
- 267 files · ~0 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 5132 nodes · 15590 edges · 224 communities (211 shown, 13 thin omitted)
- Extraction: 94% EXTRACTED · 6% INFERRED · 0% AMBIGUOUS · INFERRED: 930 edges (avg confidence: 0.76)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `f667ad36`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- Game Accounts
- Game
- Games Core
- Account Management
- Deployment
- Operator
- Games
- Games
- Observability
- Directory
- Spectators
- Game Delivery
- Games
- Shared
- Migration
- Game Authority
- Game Object
- Game Wire
- Test Deployment
- Mod
- Scenario
- Terminal
- Test Full Game E2E
- Directory Games
- Games
- Directory Games
- Test Release Gate
- Writes
- Game Sockets
- Mod
- Users
- Shared
- Shared
- Operator Runtime
- Removal-Recovery.Test
- Shared
- Directory
- Player Recovery
- Directory Host Transfer
- Player Recovery
- Service
- Game Directory
- Directory Games
- Auth Audit
- Users Core
- Game Authority
- Spectators
- Authentication
- Games
- Reads
- Removal
- Management
- Users
- Operator Peers
- Reads
- Shared
- Game Reads
- Audit Repairs
- Discovery.Test
- Backend
- Game Wire
- Operator
- Sql Policy Tags
- Mod
- Shared
- Records
- Reads
- Command Lookup.Test
- Args
- Games
- Player Lifecycle
- Test Full Game E2E
- Shared
- Directory
- Test Full Game E2E
- Read Routes.Test
- Resolution
- Player Lifecycle
- Operator Https Wss
- Command Lookup
- Games
- Tests
- Account Expiry
- Mod
- Mod
- Users
- Reads
- Operator Peers
- Privileged Audit
- Receipt Atomicity
- Player Lifecycle Boundaries.Test
- Local Readiness
- Operator.Test
- History
- Removal
- Test Support
- Test Full Game E2E
- Player Lifecycle Review
- Frontend
- Shared
- Users
- Reads
- Shared
- Game Reads
- Wire
- Terminal
- Output
- Link File
- Host Recipient
- Output
- Wire
- Game Resources
- Game Object.Test
- Backend
- Reads
- Mod
- Discovery
- Shared
- Receipts
- Game Resources
- Protected Response Snapshot.Test
- Spectators.Test
- Discovery
- Discovery
- History
- Write Regressions
- Command Lookup
- Shared
- Terminal
- Dedicated Reads Https Wss
- Game Audit Https Wss
- Game Resources.Test
- Removals
- Directory Schema
- Owner
- Game Object
- Auth Audit
- Gameplay Https Wss
- Game Storage Audit
- Start Publication
- Mod
- Privileged Audit
- Mod
- Auth Schema
- Auth Output
- Player Lifecycle.Test
- Commands
- Deployment Identity
- Terminal Regressions
- Shared
- Mod
- Receipts
- Edge
- Shared
- Terminal Faults
- History Api
- Game Audit Repairs.Test
- Player Recovery Boundaries.Test
- Rate
- Mod
- Sync Output
- Recovery Answers
- Privileged Game Audit.Test
- Game Schema
- Previous Command Game Schema
- Previous Read Game Schema
- Previous Saved Game Schema
- Shared
- Shared
- Storage Bounds.Test
- Backend Once Error
- Backend Once Success
- Typed Struct
- Links
- Host Transfer
- Discovery Boundaries.Test
- Player Recovery Review Regressions.Test
- Frontend
- Frontend Once Error
- Shared
- Maintenance
- Mod
- Prestart Maintenance
- Protocol
- Mod
- Shared
- Game Operator Schema
- Previous Command Operator Schema
- Previous Read Operator Schema
- Previous Saved Operator Schema
- Accounts
- Audit
- Origin
- Game Delivery Schema
- Shared
- Game Accounts
- Shared
- History
- Historical Probe.Config
- Main
- Shared
-   Init
- Games
- Error

## God Nodes (most connected - your core abstractions)
1. `GameError` - 363 edges
2. `migrate_directory()` - 133 edges
3. `GameAccountAuthority` - 111 edges
4. `AuthError` - 104 edges
5. `migrate()` - 101 edges
6. `StorageError` - 97 edges
7. `post()` - 95 edges
8. `DirectoryError` - 85 edges
9. `E2EError` - 76 edges
10. `ApiError` - 71 edges

## Surprising Connections (you probably didn't know these)
- `sqlite_username_boundaries_match_domain_validation_on_insert_and_update()` --calls--> `validate_username()`  [INFERRED]
  backend/src/db/schema/auth_schema.rs → shared/domain/src/accounts.rs
- `validate_summary()` --calls--> `validate_alias()`  [INFERRED]
  backend/src/worker_runtime/history/wire.rs → shared/domain/src/games.rs
- `replay_guidance()` --references--> `ReceiptOperation`  [EXTRACTED]
  cli/src/output.rs → shared/contracts/src/management.rs
- `CliRequest` --references--> `ManagementCommand`  [EXTRACTED]
  backend/src/api/developer.rs → shared/contracts/src/management.rs
- `DiscoveryCommand` --references--> `GameCode`  [EXTRACTED]
  backend/src/api/games/reads.rs → shared/domain/src/games.rs

## Import Cycles
- 2-file cycle: `backend/src/db/game.rs -> backend/src/db/game/player_lifecycle.rs -> backend/src/db/game.rs`

## Communities (224 total, 13 thin omitted)

### Community 0 - "Game Accounts"
Cohesion: 0.06
Nodes (97): account_registration_and_post_gate_recheck_use_real_accounts_core(), AccountAction, AccountAuthorityWire, AccountCloseIdentity, AccountCloseReply, AccountCloseRequest, AccountConnectionIdentity, AccountOutcome (+89 more)

### Community 1 - "Game"
Cohesion: 0.08
Nodes (28): account_acceptance_preflight_rejects_fenced_or_replaced_preparations_without_accepting(), add_time(), Admission, close_work(), GameService<'a, D, R>, join_fingerprint(), prepared_account_close_requires_durable_deletion_before_success(), require_admission() (+20 more)

### Community 2 - "Games Core"
Cohesion: 0.06
Nodes (94): creation_fingerprint(), Err, Result, Self, account_socket_grants_require_fresh_accounts_and_do_not_renew_expiry_or_presence(), actual_sqlite_reopen_preserves_lobby_intent_and_lost_publication_ack(), admission_is_verifier_only_short_lived_and_consumed_retries_never_mint_cookies(), answer_kdf_runs_outside_sqlite_write_and_rechecks_real_competing_admission() (+86 more)

### Community 3 - "Account Management"
Cohesion: 0.07
Nodes (99): migrate(), D, a_current_host_session_is_attributed_when_admin_authority_is_denied(), account_detail_is_safe_and_deleted_or_unknown_target_is_not_found(), account_pages_are_bounded_and_continue_without_duplicates(), audit_write_failure_rolls_back_account_link_and_receipt(), authenticated_rejections_are_durably_audited(), command_id() (+91 more)

### Community 4 - "Deployment"
Cohesion: 0.08
Nodes (80): AST, Path, active_deployment(), canonical(), check_label(), cloudflare(), deploy(), DeploymentError (+72 more)

### Community 5 - "Operator"
Cohesion: 0.09
Nodes (39): GameAccountAuthority, account_actor(), authorize(), authorize_mutation(), ensure_before_idle(), bounded_random_index(), call_fingerprint(), choose_remaining_value() (+31 more)

### Community 6 - "Games"
Cohesion: 0.06
Nodes (25): CodeGrant, CreationAck, CreationFingerprint, CreationReadyProof, CreationWork, DiscoveryRoute, GameProjection, ProjectionAck (+17 more)

### Community 7 - "Games"
Cohesion: 0.06
Nodes (57): admission_and_join_validate_lookup_alias_and_never_debug_recovery_material(), assert_response_rejected(), assert_view_rejected(), created_completion_can_be_later_than_creation_without_moving_idle_deadline(), creation_resolves_defaults_and_overrides_with_effective_center(), game_tags_and_safe_receipt_encoding_round_trip_without_parser_details(), gameplay_command_inputs_are_closed_canonical_and_state_fenced(), hardened_tagged_maps_preserve_original_duplicates_and_closed_fields() (+49 more)

### Community 8 - "Observability"
Cohesion: 0.05
Nodes (59): Arc, api_failure(), auth_failure(), Boundary, boundary_failures_have_closed_fields_and_expected_levels(), Capture, Delivery, Failure (+51 more)

### Community 9 - "Directory"
Cohesion: 0.11
Nodes (68): migrate_directory(), account(), acquire_persists_a_presence_only_gate_with_separate_operation_identity(), acquired_gate_rejects_transfer_without_changing_the_original_host(), competing_operation_returns_busy_without_replacing_the_current_gate(), completed_operation_cannot_reacquire_or_clear_a_newer_gate(), concurrent_operations_on_one_account_serialize_to_one_grant_and_one_busy(), corrupted_completion_time_cannot_erase_the_fence_or_authorize_an_ack() (+60 more)

### Community 10 - "Spectators"
Cohesion: 0.10
Nodes (26): advance(), make_id(), Uuid, actor(), AudienceCommand, AudienceMutation, digest(), fingerprint() (+18 more)

### Community 11 - "Game Delivery"
Cohesion: 0.09
Nodes (50): binding_params(), DeliveryBinding, DeliveryBudget, DeliveryError, DeliveryService, DeliveryService<'a, D, R>, DeliveryService<'_, D, R>, migrate_game_delivery() (+42 more)

### Community 12 - "Games"
Cohesion: 0.12
Nodes (24): BoundedJsonSink, CallView, checked_deadline(), ConfigurationOverrides, decode(), encode(), GameDecodeError, BoardView (+16 more)

### Community 13 - "Shared"
Cohesion: 0.05
Nodes (37): Self, Self, Row, Value, Vec, sql_params(), StorageError, int() (+29 more)

### Community 14 - "Migration"
Cohesion: 0.13
Nodes (56): canonicalize_table_identifiers(), command_lookup_migration_counter_six_rejects_every_table_column_and_index_drift_without_writes(), command_lookup_migration_ignored_or_failed_counter_rolls_back_schema_and_rows(), command_lookup_migration_known_counter_six_preserves_all_tables_and_audit(), dedicated_read_audit_migration_ignored_or_failed_marker_rolls_back_ddl_and_rows(), dedicated_read_audit_migration_preserves_frozen_published_inventory_rows_and_optional_delivery(), dedicated_read_audit_migration_rejects_full_unknown_shape_before_any_write(), dedicated_read_published_constraint_foreign_key_strict_and_partial_delivery_drift_writes_nothing() (+48 more)

### Community 15 - "Game Authority"
Cohesion: 0.09
Nodes (25): AuthService<'_, D, R>, command(), GameSocketCloseWork, removal_finishes_only_after_all_exact_socket_close_acknowledgements(), AccountId, Cell, CommandId, Connection (+17 more)

### Community 16 - "Game Object"
Cohesion: 0.14
Nodes (25): GameService, D, R, GameObject, OwnerDatabase, close_checked(), GameObject, reload() (+17 more)

### Community 17 - "Game Wire"
Cohesion: 0.09
Nodes (36): AttemptRuntime, audience_ack_proof_binds_every_original_request_field(), audience_release_attempt_new_draws_fresh_challenge_for_identical_requests(), audience_release_attempt_new_fails_closed_on_clock_or_partial_rng_failure(), AudienceReleaseAttempt, AudienceReleaseBinding, AudienceReleaseChallenge, AudienceReleaseReply (+28 more)

### Community 18 - "Test Deployment"
Cohesion: 0.08
Nodes (5): bindings(), deployment(), DeploymentTests, Local fixture and mocked provider boundaries; no authenticated provider calls., targets()

### Community 19 - "Mod"
Cohesion: 0.11
Nodes (44): absent_after_deadline_keeps_reservation_and_saturating_retry(), command(), Fixture, genuine_committed_lobby_recovers_directory_publication_without_faking_game_ack(), genuine_initializer_evidence_acknowledges_only_current_unready_creation(), genuine_new_cancellation_releases_only_the_hidden_directory_association(), genuine_owner_terminal_evidence_releases_reservation_after_deadline(), hidden_code_release_preserves_exact_terminal_identity_and_publication_fences() (+36 more)

### Community 20 - "Scenario"
Cohesion: 0.12
Nodes (19): Restricted CLI handoff; credentials only in environment, links promptly unlinked, Command, Origin-bound actors and exact-payload command retries., The UUID and serialized body are immutable across every explicit retry., call_values(), history_expiry(), Independent test-only single-line bingo model. No production imports., DO-071: three UTC calendar months, preserving milliseconds and clamping day. (+11 more)

### Community 21 - "Terminal"
Cohesion: 0.11
Nodes (28): add_three_calendar_months(), blob(), command_fingerprint(), GameService<'_, D, R>, Option, Result, SessionId, id() (+20 more)

### Community 22 - "Test Full Game E2E"
Cohesion: 0.09
Nodes (14): Bootstrap, run_cli(), Reply, Checks, E2EError, private_output(), Exception, Contains only a fixed diagnostic code, never a submitted value. (+6 more)

### Community 23 - "Directory Games"
Cohesion: 0.17
Nodes (19): command_timestamp(), deadline(), DirectoryService<'_, D, R>, integer(), optional_integer(), parse_creation(), parse_index(), parse_optional_integer() (+11 more)

### Community 24 - "Games"
Cohesion: 0.09
Nodes (36): PublicTerminalNotice, GameId, Option, FnMut, accepted_value_matches_all_cells_once_and_recomputes_single_line(), apply_called_value(), AssignedBoard, BoardCell (+28 more)

### Community 25 - "Directory Games"
Cohesion: 0.15
Nodes (51): accepted_started_terminal_proof_cannot_rewrite_original_expiry_with_newer_revision(), account(), assignment_gate_blocks_newer_projection_to_removing_host(), claim_reserves_identity_indexes_host_and_replays_same_command(), clock_advance_during_entropy_rechecks_common_fresh_admission_floor(), code_rejection_sampling_has_bounded_entropy_and_rolls_back_on_exhaustion(), command(), completion_cleanup_retires_commands_at_exact_boundary_but_keeps_pending() (+43 more)

### Community 26 - "Test Release Gate"
Cohesion: 0.06
Nodes (9): Resource summary parser fixtures are unit data, not measured service output., ResourceParserTests, ExecutionTests, historical_report(), Parser/unit fault tests; synthetic reporter fixtures are not execution evidence., ReporterTests, snapshot_report(), snapshot_summary() (+1 more)

### Community 27 - "Writes"
Cohesion: 0.10
Nodes (43): admission_context_is_keyless_and_join_uses_separate_game_scoped_cookie(), admission_forwards_an_existing_player_cookie_without_claiming_authority(), all_transport_inputs_have_release_active_finite_bounds(), audit_account_view(), audit_read_rejection_context(), creation_route_decodes_original_configuration_without_caller_identity(), decode_empty_game_body(), decode_games() (+35 more)

### Community 28 - "Game Sockets"
Cohesion: 0.14
Nodes (22): Attachment, CloseCode, connection_id(), GameObject, method(), ready_state(), AccountId, Cell (+14 more)

### Community 29 - "Mod"
Cohesion: 0.07
Nodes (44): assert_issued_handoff(), audit_reads_preserve_required_targets_and_read_operations(), authorization_debug_is_redacted_and_server_errors_discard_bodies(), BrokenPipe, ca_file_read_is_bounded_and_errors_are_redacted(), client_rejects_noncanonical_or_non_https_origins_before_network(), command_runner_exercises_real_https_to_safe_metadata(), existing_output_target_prevents_any_http_mutation() (+36 more)

### Community 30 - "Users"
Cohesion: 0.09
Nodes (40): account_get_preserves_the_canonical_typed_target(), action_routes_reject_other_methods_without_decoding_payloads(), bodyless_mutation_routes_map_to_their_closed_management_commands(), bodyless_mutations(), collection_get_maps_to_default_list_without_claiming_authority(), collection_query(), collection_query_byte_bound_precedes_parsing(), collection_query_maps_only_cursor_and_bounded_limit_in_either_order() (+32 more)

### Community 31 - "Shared"
Cohesion: 0.10
Nodes (39): GameService<'_, D, R>, GameId, Result, capture_owner_failure(), confirm_expiry_denial(), current(), current_targeted(), dispatch() (+31 more)

### Community 32 - "Shared"
Cohesion: 0.07
Nodes (38): list(), GameAuditEvent, GameAuditOperation, AuditId, GameId, Option, validate_page(), AuditEvent (+30 more)

### Community 33 - "Operator Runtime"
Cohesion: 0.12
Nodes (31): GameDirectoryObject, Result, GameOwnerResponse, audit_failed_transport(), audit_rejected_transport(), audit_transport(), denial_binding(), fence_edge_operator_expiry() (+23 more)

### Community 34 - "Removal-Recovery.Test"
Cohesion: 0.11
Nodes (30): armClock(), auth(), authorize(), bearer(), completed(), floor(), gameAuth(), normalFixture() (+22 more)

### Community 35 - "Shared"
Cohesion: 0.10
Nodes (27): AuthService<'_, D, R>, AccountId, CommandId, Result, actor_key(), AuthService<'_, D, R>, deadline(), ManagementError (+19 more)

### Community 36 - "Directory"
Cohesion: 0.17
Nodes (25): application_tables(), DirectoryService, DirectoryService<'a, D, R>, operation_timestamp(), parse_completion_row(), parse_operation_row(), AccountId, D (+17 more)

### Community 37 - "Player Recovery"
Cohesion: 0.11
Nodes (23): GameService<'_, D, R>, keyed(), PrivateRows, Proof, RecoveryOutcome, RecoverySocketClose, CommandId, ConnectionId (+15 more)

### Community 38 - "Directory Host Transfer"
Cohesion: 0.09
Nodes (24): DirectoryService<'_, D, R>, parse_directory_transfer(), require_shape(), AccountId, GameId, Option, Result, Row (+16 more)

### Community 39 - "Player Recovery"
Cohesion: 0.10
Nodes (36): actual_kdf_gap_answer_replacement_deletion_alias_epoch_and_terminal_changes_fail_closed(), aged_recovery_commands_are_rejected_after_receipt_cleanup_and_idle_denial_is_uniform(), answer_maintenance_kdf_gap_revoked_session_rolls_back_private_update(), close_retry_backoff_is_durable_bounded_and_ack_ignored_write_retains_work(), entropy_failure_unsupported_stored_profile_and_phc_fail_without_epoch_or_failure_accounting(), epoch(), exact_current_alias_and_uniform_absent_wrong_disabled_proof(), FaultReadback (+28 more)

### Community 40 - "Service"
Cohesion: 0.19
Nodes (18): AuthError, From, AuthOutcome, Value, add_deadline(), AuthService<'a, D, R>, AccountId, D (+10 more)

### Community 41 - "Game Directory"
Cohesion: 0.12
Nodes (26): checked_revision(), directory_claim_ack_and_code_use_real_owner_core_and_exact_binding(), directory_publication_recovery_and_release_use_real_core(), DirectoryGameOutcome, DirectoryGameRejection, DirectoryGameReply, DirectoryGameRequest, dispatch() (+18 more)

### Community 42 - "Directory Games"
Cohesion: 0.10
Nodes (32): ByteRuntime, coordination_snapshot(), durable_command_floor_survives_reopen_and_rejects_clock_rollback(), history_expiry_boundary_survives_actual_reopen_without_clock_or_code_resurrection(), ignored_clock_observation_cannot_acknowledge_history_expiry(), ignored_code_allocation_never_returns_an_uncommitted_grant(), ignored_expiry_release_delete_beyond_compaction_batch_never_acknowledges_purge(), independent_connections_serialize_competing_hosts_to_one_reserved_game() (+24 more)

### Community 43 - "Auth Audit"
Cohesion: 0.14
Nodes (31): AuditDb, authorize(), Completed, connection_id(), context(), decision_sample_fences_current_game_connection_and_admin_expiry_denials(), Fault, fence() (+23 more)

### Community 44 - "Users Core"
Cohesion: 0.17
Nodes (29): new_token(), command_id(), count(), create(), Fixture, AccountId, Cell, CommandId (+21 more)

### Community 45 - "Game Authority"
Cohesion: 0.24
Nodes (33): bearer(), clock_rollback_and_unavailable_storage_never_release_authority(), close_pages_are_bounded_stably_ordered_and_do_not_discard_backlog(), close_retry_is_durable_capped_and_saturates_count_and_deadline(), connection(), context(), current_normal_host_and_admin_produce_private_game_authority(), due_close_work_is_bounded_and_preserves_exact_revoked_binding() (+25 more)

### Community 46 - "Spectators"
Cohesion: 0.13
Nodes (28): audience(), connect(), contents(), count(), initial_unconnected_seat_has_bounded_exact_grace_and_clock_rollback_fence(), proof(), Fixture, Row (+20 more)

### Community 47 - "Authentication"
Cohesion: 0.11
Nodes (27): all_seven_operations_accept_their_exact_payload(), assert_array_root_rejected(), AuthenticationOperation, AuthenticationPayload, AuthenticationRequest, CompleteBody, cookie_is_read_and_duplicate_session_values_reject(), decode_authentication() (+19 more)

### Community 48 - "Games"
Cohesion: 0.17
Nodes (29): ApiError, GameRejection, answer_receipt(), audit_game_transport_failure(), dispatch(), fence_recovery_output(), handle(), membership_receipt() (+21 more)

### Community 49 - "Reads"
Cohesion: 0.15
Nodes (23): availability_selects_exact_context_without_credential_fallback(), AvailabilityView, decode(), decode_discovery(), decode_game_read(), DiscoveryCommand, DiscoveryRequest, game_read_route() (+15 more)

### Community 50 - "Removal"
Cohesion: 0.26
Nodes (26): admin_session(), app_admin_self_disable_delete_and_enable_are_forbidden(), command_id(), committed_removal_phase_is_bound_and_persisted_in_real_sqlite(), deletion_does_not_require_an_unused_credential_epoch_increment(), deletion_requires_matching_gate_and_retains_intent_until_release_ack(), developer_cli_cannot_remove_the_last_enabled_verified_admin(), disable_changes_epoch_once_and_repeated_no_op_keeps_timestamp() (+18 more)

### Community 51 - "Management"
Cohesion: 0.12
Nodes (24): AccountsObject, call_owner(), dispatch(), GameAuditErrorFields, GameAuditErrorReply, GameAuditExpiryFence, GameAuditTransportContext, GameAuditTransportPurpose (+16 more)

### Community 52 - "Users"
Cohesion: 0.11
Nodes (23): AccountsObject, deserialize_error_fields(), dispatch(), ErrorEnvelope, ErrorFields, handle(), log_delivery(), management_auth_error() (+15 more)

### Community 53 - "Operator Peers"
Cohesion: 0.22
Nodes (21): authorize(), complete(), decode_challenge(), directory_error(), filter_targets(), fresh_challenge(), operator_fresh_challenges_differ_at_identical_clock_and_entropy_failure_denies(), OperatorAccountCut (+13 more)

### Community 54 - "Reads"
Cohesion: 0.10
Nodes (19): BoardsRead, bounded_ids(), dedicated_read_qualification_accepts_canonical_subset_and_maximum_lines(), dedicated_read_qualification_rejects_duplicate_unordered_and_out_of_bounds_lines(), MembershipAccess, OwnBoard, PlayerRoster, qualification_views() (+11 more)

### Community 55 - "Shared"
Cohesion: 0.13
Nodes (21): authenticate_cli(), CliRequest, decode_cli(), CommandId, HeaderMap, Method, Option, Result (+13 more)

### Community 56 - "Game Reads"
Cohesion: 0.12
Nodes (25): creation_pending(), Env, Result, super::directory::GameDirectoryObject, account_identity(), clone_request(), digest(), dispatch() (+17 more)

### Community 57 - "Audit Repairs"
Cohesion: 0.13
Nodes (23): audit(), bounded_audit_page_does_not_scan_every_live_row(), Exhaust, explicit_creation_target_transient_identity_query_failure_is_capturable(), invalid_typed_call_winner_cancel_configuration_sync_all_capture_independently(), names(), privileged_stream_acceptance_and_success_are_atomic_and_replay_is_quiet(), random_generation_budget_is_required_failed_audit() (+15 more)

### Community 58 - "Discovery.Test"
Cohesion: 0.12
Nodes (20): directory(), get(), lobby(), newGame(), owner(), player(), rows(), value() (+12 more)

### Community 59 - "Backend"
Cohesion: 0.11
Nodes (19): ConfigError, InvalidValueKind, acquisition_can_only_request_the_declared_inventory(), assert_redacted(), dev_cli_key_is_optional_but_supplied_values_are_validated(), explicit_argon2_costs_are_deserialized_as_numbers(), keys_require_canonical_unpadded_base64url_for_exactly_32_bytes(), Result (+11 more)

### Community 60 - "Game Wire"
Cohesion: 0.10
Nodes (18): GameViewSelector, GameId, SpectatorAuthority, GameId, PlayerId, SessionId, SyncPlayerBinding, PlayerSyncReleaseReply (+10 more)

### Community 61 - "Operator"
Cohesion: 0.19
Nodes (17): alter(), FirstAuditEntropyFailure, Fixture, operator_audit_entropy_failure_rolls_back_each_mutation_and_separately_records_failed(), operator_identity_is_immutable_and_remains_after_prestart_terminal_cleanup(), operator_transfer_aborted_work_and_stale_grants_cannot_commit_after_removal_wins(), operator_transfer_grant_proof_ack_binds_all_immutable_fields_and_never_clears_successor(), operator_transfer_ignored_assignment_or_phase_write_rolls_back_receipt_and_assignment() (+9 more)

### Community 62 - "Sql Policy Tags"
Cohesion: 0.16
Nodes (22): assert_verifier_rehashed(), assertion_failure_diagnostics_do_not_disclose_verifiers(), command_id(), context(), cookie(), create(), enrollment_and_reset_bind_status_purpose_and_scope_without_changing_stored_tags(), RecordedSqlite (+14 more)

### Community 63 - "Mod"
Cohesion: 0.11
Nodes (13): Vec, same_work_target(), CreationResult, LobbyPreparation, ParticipantOutcome, PendingWork, AccountId, CommandId (+5 more)

### Community 64 - "Shared"
Cohesion: 0.09
Nodes (14): AuthCommand, Debug, Drop, Formatter, Option, Result, String, AccessLinkPurpose (+6 more)

### Community 65 - "Records"
Cohesion: 0.17
Nodes (18): parse_work(), Row, Account, integer(), optional_integer(), optional_text(), AccountId, Option (+10 more)

### Community 66 - "Reads"
Cohesion: 0.18
Nodes (23): account(), dedicated_reads_admin_purposes_success_rejection_failure_atomic_capture(), dedicated_reads_awaiting_own_membership_board_recovery_and_alias_are_current(), dedicated_reads_complete_numeric_pool_calls_qualification_and_exhaustion(), dedicated_reads_full_thousand_actual_accepted_calls_remain_in_progress_and_exhausted(), dedicated_reads_history_purge_denies_deleted_original_player_grant(), dedicated_reads_maximum_legal_pool_roster_boards_and_policy_aliases_are_bounded(), dedicated_reads_new_cross_host_are_passive_and_sync_still_enters() (+15 more)

### Community 67 - "Command Lookup.Test"
Cohesion: 0.11
Nodes (17): fixture(), lobby(), owner(), player(), rows(), directoryStub(), initializeDirectory(), commandId() (+9 more)

### Community 68 - "Args"
Cohesion: 0.12
Nodes (20): AccountCommand, AuditCommand, AuditPage, canonical_id(), Cli, LinkMutation, Mutation, Page (+12 more)

### Community 69 - "Games"
Cohesion: 0.22
Nodes (22): call_sequence(), call_value(), canonical_alias(), cell_kind(), cells(), completed_lines(), configuration(), JoinPlayer (+14 more)

### Community 70 - "Player Lifecycle"
Cohesion: 0.22
Nodes (14): player_actor(), fingerprint(), GameService<'_, D, R>, MembershipOutcome, CommandId, ConnectionId, GameId, Option (+6 more)

### Community 71 - "Test Full Game E2E"
Cohesion: 0.17
Nodes (7): Transport, FakeSocket, frame(), Offline unit tests for the remote runner; these are not remote run evidence., receiver_fixture(), snapshot_fixture(), StreamTests

### Community 72 - "Shared"
Cohesion: 0.11
Nodes (20): AccountId, String, SafeAccount, SessionKind, SessionView, deserialize_object(), deserialize_users(), AccountId (+12 more)

### Community 73 - "Directory"
Cohesion: 0.14
Nodes (11): DirectoryRejection, DirectoryRequest, DirectoryResponse, GameDirectoryObject, AccountId, DurableObject, Env, OperationId (+3 more)

### Community 74 - "Test Full Game E2E"
Cohesion: 0.12
Nodes (5): Actor, CookiePolicy, NoRedirect, ActorTests, Response

### Community 75 - "Read Routes.Test"
Cohesion: 0.16
Nodes (22): directory(), get(), intercept(), owner(), restore(), setup(), cookie(), call() (+14 more)

### Community 76 - "Resolution"
Cohesion: 0.17
Nodes (18): add_three_calendar_months(), civil_from_days(), days_from_civil(), days_in_month(), GameService<'_, D, R>, AccountId, CommandId, GameId (+10 more)

### Community 77 - "Player Lifecycle"
Cohesion: 0.19
Nodes (18): String, Zeroizing, SecretCookie, alias_exact_trim_noop_recovery_and_membership_session_invariants(), connect(), durable_close_identity_and_stale_ack_never_touch_successor_or_expiry(), inprogress_leave_retains_gameplay_return_authority_and_timestamp_and_retry_preserves_successor(), inprogress_timestamp_ignore_abort_roll_back_closure_and_receipt_and_answer_recovery_retains_event() (+10 more)

### Community 78 - "Operator Https Wss"
Cohesion: 0.15
Nodes (13): Parser, call(), check(), provision(), Local gameplay HTTPS/WSS checks; build the Worker and CLI before running., SocketClient, main(), Namespace (+5 more)

### Community 79 - "Command Lookup"
Cohesion: 0.13
Nodes (17): ReadRole, command_lookup_creation_lobby_own_actor_unknown_cross_actor_and_no_effects(), command_lookup_current_player_admission_alias_recovery_switch_and_reuse(), command_lookup_directory_creation_exact_original_account_only_and_passive(), command_lookup_future_floor_exact_expiry_rollback_and_ignored_clock(), command_lookup_old_required_work_remains_pending_but_committed_receipt_expires(), command_lookup_pending_prepared_and_committed_follow_up_are_not_peer_completion(), command_lookup_start_call_terminal_original_grants_and_exit() (+9 more)

### Community 80 - "Games"
Cohesion: 0.10
Nodes (18): GamePayload, AdmissionContextInput, CallManualInput, CancelGameInput, ConfigurationPatch, CreateGame, DeliveryId, LeavePlayerInput (+10 more)

### Community 81 - "Tests"
Cohesion: 0.16
Nodes (14): access_link_cleanup_and_alarm_share_the_exact_metadata_retention_deadline(), cleanup_limits_each_delete_and_expired_close_enqueue_to_the_named_batch_size(), entity_id(), Cell, Connection, FnOnce, RefCell, Result (+6 more)

### Community 82 - "Account Expiry"
Cohesion: 0.17
Nodes (18): discovery_accounts_owner_fences_original_sample_despite_rollback_without_renewal(), discovery_expiry_fence_readback_and_transaction_failure_roll_back(), discovery_expiry_fence_rejects_wrong_token_identity_metadata_and_sample(), discovery_expiry_fence_sqlite_ignore_abort_and_tampered_readback_never_ack(), fence_account_expiry(), Fixture, read_fence(), ReadbackFault (+10 more)

### Community 83 - "Mod"
Cohesion: 0.28
Nodes (21): capture_owner_failure(), current_output(), current_targeted(), deny_observed_expiry(), DiscoveryError, dispatch(), failure(), fence_expired_account() (+13 more)

### Community 84 - "Mod"
Cohesion: 0.20
Nodes (13): AccountsObject, decode_private_json(), fetch(), DurableObject, Env, Option, Result, Self (+5 more)

### Community 85 - "Users"
Cohesion: 0.16
Nodes (17): AuthService<'_, D, R>, changes(), final_release_expiry_fence_is_atomic_and_blocks_clock_rollback(), final_release_is_read_only_on_success_and_fences_authority_denials(), fixture(), id(), AccountId, CommandId (+9 more)

### Community 86 - "Reads"
Cohesion: 0.21
Nodes (18): authority(), initialized(), dedicated_release_fences_actual_inner_expiry_sample_and_rejects_rollback(), dedicated_release_identity_epoch_role_assignment_and_deleted_record_deny(), dedicated_release_ignored_fence_is_unavailable_and_rolls_back(), dedicated_release_original_cap_never_extends_after_resume_and_equality_fences_before_entropy(), dedicated_release_payload_changed_at_same_identity_is_withheld(), read_purpose() (+10 more)

### Community 87 - "Operator Peers"
Cohesion: 0.17
Nodes (20): OperatorAccountsReply, OperatorAccountsRequest, OperatorChallenge, OperatorDirectoryReply, OperatorDirectoryRequest, AccountId, CommandId, Deserialize (+12 more)

### Community 88 - "Privileged Audit"
Cohesion: 0.24
Nodes (20): domain_rows(), events(), ignored_capture_with_identical_existing_entropy_collision_cannot_commit_action(), page(), privileged_action_and_capture_entropy_errors_rollback_then_durably_record_failed(), privileged_action_storage_error_rolls_back_and_captures_failed_using_actual_actor(), privileged_call_and_cancel_ignored_or_failed_capture_rolls_back_every_effect_receipt_and_history(), privileged_calls_winner_actual_cross_admin_and_exact_replay() (+12 more)

### Community 89 - "Receipt Atomicity"
Cohesion: 0.16
Nodes (18): account_exit_receipt_scalar_faults_roll_back_then_original_command_retries(), lobby_cancel_receipt_scalar_faults_roll_back_then_original_command_retries(), manual_call_receipt_scalar_faults_roll_back_then_original_command_retries(), new_cancel_receipt_scalar_faults_roll_back_then_original_command_retries(), Operation, player_exit_receipt_scalar_faults_roll_back_then_original_command_retries(), Prepared, random_call_receipt_scalar_faults_roll_back_then_original_command_retries() (+10 more)

### Community 90 - "Player Lifecycle Boundaries.Test"
Cohesion: 0.13
Nodes (15): nativeRegistry(), fixture(), join(), leave(), owner(), recover(), snapshot(), started() (+7 more)

### Community 91 - "Local Readiness"
Cohesion: 0.18
Nodes (16): binding(), fresh_command(), history_measurements(), local_slice9_measurements(), MeasuredDb, OsRuntime, recovery_owner(), Cell (+8 more)

### Community 92 - "Operator.Test"
Cohesion: 0.13
Nodes (12): enroll(), admin(), fixture(), owner(), patch(), rows(), transfer(), owner() (+4 more)

### Community 93 - "History"
Cohesion: 0.22
Nodes (15): decode_history(), filter_tag(), HistoryCommand, HistoryCursor, HistoryQuery, HistoryRequest, list_query(), Drop (+7 more)

### Community 94 - "Removal"
Cohesion: 0.30
Nodes (8): AuthService<'_, D, R>, RemovalPhase, RemovalWork, CommandId, OperationId, Option, Result, target()

### Community 95 - "Test Support"
Cohesion: 0.14
Nodes (12): Cell, Connection, FnOnce, RefCell, Result, Row, Self, T (+4 more)

### Community 96 - "Test Full Game E2E"
Cohesion: 0.13
Nodes (3): BoardOracle, board_fixture(), OracleTests

### Community 97 - "Player Lifecycle Review"
Cohesion: 0.19
Nodes (15): connect(), durable_snapshot(), ignored_or_aborted_explicit_leave_timestamp_rolls_back_detachment_and_receipt(), lobby(), ReadbackFault, required_readback_faults_roll_back_every_lifecycle_effect_and_clock_metadata(), Cell, Fixture (+7 more)

### Community 98 - "Frontend"
Cohesion: 0.22
Nodes (16): IntoIterator, absent_public_settings_use_serde_defaults(), assert_redacted(), default_api_path(), explicit_public_values_are_preserved(), get_frontend_config(), maximum_display_name_is_measured_in_utf8_bytes(), PublicAppConfig (+8 more)

### Community 99 - "Shared"
Cohesion: 0.13
Nodes (14): CliConfig, get_cli_config(), Option, PathBuf, Result, Self, String, D (+6 more)

### Community 100 - "Users"
Cohesion: 0.19
Nodes (15): account_fields_reject_positional_sequences(), assert_invalid_response(), CreateUsername, every_public_variant_round_trips_with_safe_debug_and_http_status(), nested_accounts_preserve_duplicate_and_unknown_field_rejection(), nested_objects_preserve_exact_wire_bytes(), nested_objects_reject_other_nonobject_shapes(), nested_receipts_preserve_nullable_duplicates_and_unknown_field_rejection() (+7 more)

### Community 101 - "Reads"
Cohesion: 0.27
Nodes (11): ReadSelection, PlayerId, GameService<'_, D, R>, ConnectionId, Option, Result, GameDetail, MembershipRead (+3 more)

### Community 102 - "Shared"
Cohesion: 0.14
Nodes (12): action(), AuthService<'_, D, R>, FnOnce, Option, Result, T, ManagementPrincipal, Debug (+4 more)

### Community 103 - "Game Reads"
Cohesion: 0.33
Nodes (9): ReadBinding, ReadCut, ExpiryRequest, GameObject, ReadRequest, ReleaseRequest, Option, Result (+1 more)

### Community 104 - "Wire"
Cohesion: 0.21
Nodes (17): discovery_expiry_ack_is_bound_and_cannot_replace_a_read_projection(), discovery_private_original_bytes_reject_unknown_duplicate_positional_and_unsafe_fields(), discovery_reply_identity_lifecycle_admission_and_deadlines_are_bound(), DiscoveryAction, DiscoveryDirectoryRequest, DiscoveryOutcome, DiscoveryOwnerReply, DiscoveryOwnerRequest (+9 more)

### Community 105 - "Terminal"
Cohesion: 0.18
Nodes (16): all_rows(), count(), end_without_winner(), explicit_prestart_cancellation_is_atomic_replayable_and_has_no_history(), final_exit_removes_all_player_or_account_grants_but_never_changes_history(), gameplay_response_guard_is_read_only_and_accepts_terminal_without_new_view_grant(), history_expiry_denies_immediately_then_purges_with_recoverable_directory_release(), malformed_history_cannot_be_erased_without_validating_its_self_contained_snapshot() (+8 more)

### Community 106 - "Output"
Cohesion: 0.22
Nodes (15): CliError, human_and_json_account_output_escape_all_controls(), issued_output_projects_metadata_without_link_secrets(), replay_guidance(), CommandId, Result, String, Value (+7 more)

### Community 107 - "Link File"
Cohesion: 0.22
Nodes (12): failure_cleanup_truncates_only_owned_handle_not_replaced_path(), LinkFile, persists_link_without_displaying_or_reopening_path(), replaced_path_is_rejected_before_secret_write(), reserves_new_private_file_before_network(), Drop, PathBuf, Result (+4 more)

### Community 108 - "Host Recipient"
Cohesion: 0.24
Nodes (8): AuthService<'_, D, R>, HostRecipientAuthority, AccountId, HostTransferTarget, Option, Result, Self, Vec

### Community 109 - "Output"
Cohesion: 0.22
Nodes (13): AuthOutputAuthority, AuthService<'_, D, R>, fixture(), output_fence_preserves_original_sample_and_never_renews_or_resurrects(), output_fence_requires_exact_identity_deadline_sample_and_verified_write(), output_release_rechecks_revocation_lifecycle_role_and_safe_projection(), AccountId, Result (+5 more)

### Community 110 - "Wire"
Cohesion: 0.22
Nodes (13): auth_body(), CookieWire, OwnerRequest, OwnerResponse, D, Deserialize, Error, Option (+5 more)

### Community 111 - "Game Resources"
Cohesion: 0.16
Nodes (14): bytes(), cookie(), finite_metrics(), id(), MeasuredDb, Cell, Serialize, Sqlite (+6 more)

### Community 112 - "Game Object.Test"
Cohesion: 0.13
Nodes (9): closed(), holdConnectionProofs(), join(), messages(), newGame(), openLobby(), releaseConnectionProofs(), stream() (+1 more)

### Community 113 - "Backend"
Cohesion: 0.17
Nodes (11): BackendConfig, get_backend_config(), Option, Result, Self, String, deserialize_error(), EnvSource (+3 more)

### Community 114 - "Reads"
Cohesion: 0.18
Nodes (11): AuthService<'_, D, R>, page_size(), AccountId, AuditId, Option, Result, safe(), id_check() (+3 more)

### Community 115 - "Mod"
Cohesion: 0.19
Nodes (13): AuthPolicy, Default, Self, bounded_phc(), hash_password(), Result, String, validate_policy() (+5 more)

### Community 116 - "Discovery"
Cohesion: 0.23
Nodes (15): all_rows(), discovery_authoritative_state_advances_without_directory_revision_equality(), discovery_availability_counts_valid_observed_connections_without_reconciliation(), discovery_fence_storage_fault_is_unavailable_and_rolls_back(), discovery_idle_denial_is_durable_write_free_recut_and_rollback_fenced(), discovery_player_deadline_denial_survives_clock_rollback_without_other_writes(), discovery_read_only_cuts_hide_allocations_and_do_not_create_grants_or_timers(), discovery_remote_expiry_fences_the_original_sample_without_other_writes() (+7 more)

### Community 117 - "Shared"
Cohesion: 0.19
Nodes (10): BTreeMap, NativeSource, Into, Mutex, Option, Result, Self, String (+2 more)

### Community 118 - "Receipts"
Cohesion: 0.24
Nodes (11): AuthService<'_, D, R>, command_id(), CommandOutcome, fingerprint(), AccountId, CommandId, LinkId, Option (+3 more)

### Community 119 - "Game Resources"
Cohesion: 0.33
Nodes (7): Fixture, maximum_value_cell_game_resources(), AccountId, CommandId, Self, TestRuntime, MeasuredDb

### Community 120 - "Protected Response Snapshot.Test"
Cohesion: 0.22
Nodes (13): actualStart(), cleanup(), denied(), directory(), fixture(), get(), holdFinal(), mutateContent() (+5 more)

### Community 121 - "Spectators.Test"
Cohesion: 0.26
Nodes (14): assertPurged(), clock(), expectClearedSpectator(), fixture(), get(), observeTerminalSend(), owner(), player() (+6 more)

### Community 122 - "Discovery"
Cohesion: 0.38
Nodes (7): DiscoveryCut, DiscoveryValue, DiscoveryViewer, GameService<'_, D, R>, GameId, Option, Result

### Community 123 - "Discovery"
Cohesion: 0.24
Nodes (13): live(), CurrentGame, CurrentGames, CurrentGameSummary, GameAvailability, GameEntry, GameSlot, AccountId (+5 more)

### Community 124 - "History"
Cohesion: 0.22
Nodes (9): fresh_history_login(), history_at_last_retained_millisecond(), history_final_response_expiry_fence_faults_roll_back_all_rows(), history_final_response_expiry_fence_survives_rollback_and_file_reopen(), history_final_response_expiry_read_error_rolls_back_required_write(), immutable_history_survives_actual_file_reopen_after_final_grants_exit(), Fixture, GameId (+1 more)

### Community 125 - "Write Regressions"
Cohesion: 0.21
Nodes (11): additional_terminal_required_writes_ignore_abort_rollback_and_exact_retry(), all_call_write_faults_rollback_then_original_command_retries(), host_revision(), random_call_unbiased_rejection_chooses_remaining_index_and_replay_uses_no_entropy(), Cell, ConnectionId, Fixture, Result (+3 more)

### Community 126 - "Command Lookup"
Cohesion: 0.31
Nodes (8): command_stamp(), GameService<'_, D, R>, CommandId, GameOutcome, Option, Result, validate_outcome_actor(), CommandResultStatus

### Community 127 - "Shared"
Cohesion: 0.17
Nodes (12): ReadIdentity, AccountId, PlayerId, SessionId, SpectatorId, CreationLookupReply, CreationLookupRequest, required_nullable() (+4 more)

### Community 128 - "Terminal"
Cohesion: 0.27
Nodes (6): GameService<'_, D, R>, Result, SpectatorId, String, Vec, SpectatorTerminalNotice

### Community 129 - "Dedicated Reads Https Wss"
Cohesion: 0.32
Nodes (6): call(), check(), provision(), Local dedicated-read HTTPS/WSS checks; build the Worker and CLI before running., read(), SocketClient

### Community 130 - "Game Audit Https Wss"
Cohesion: 0.31
Nodes (6): call(), check(), developer_read(), provision(), Slice 8 real local HTTPS developer CLI and privileged Game auditing checks., SocketClient

### Community 131 - "Game Resources.Test"
Cohesion: 0.21
Nodes (8): ack(), jsonOK(), limits, owner(), pause(), settled(), stream(), sync()

### Community 132 - "Removals"
Cohesion: 0.41
Nodes (7): AuthService, D, R, AccountsObject, CommandId, OperationId, Result

### Community 133 - "Directory Schema"
Cohesion: 0.44
Nodes (11): add_game_coordination(), add_host_transfer_coordination(), game_coordination_statements(), host_transfer_statements(), initialize_removal(), install(), removal_statements(), D (+3 more)

### Community 134 - "Owner"
Cohesion: 0.24
Nodes (9): GameObject, peer_error(), route(), Cell, Env, GamePeerError, Option, Result (+1 more)

### Community 135 - "Game Object"
Cohesion: 0.17
Nodes (11): peer_error(), private_response(), privileged_purpose(), RegistryAvailability, rejection(), GamePeerError, Option, Serialize (+3 more)

### Community 136 - "Auth Audit"
Cohesion: 0.23
Nodes (8): login_failures_block_retired_completion_proof_in_both_directions(), redemption_expiry_decision_is_fenced_before_rollback(), retired_completion_bad_proof_does_not_swallow_rate_storage_failure(), retired_completion_rechecks_login_block_after_password_proof(), retired_completion_retry_rechecks_exact_session_and_account_binding_after_proof(), retired_enrollment_and_reset_retry_cannot_disclose_receipts_when_clock_fencing_fails(), retired_enrollment_and_reset_retry_recheck_expiry_after_password_proof(), Self

### Community 137 - "Gameplay Https Wss"
Cohesion: 0.32
Nodes (5): call(), check(), join(), Local gameplay HTTPS/WSS checks; build the Worker and CLI before running., SocketClient

### Community 138 - "Game Storage Audit"
Cohesion: 0.23
Nodes (8): account_session(), current_connection_capacity_preserves_existing_bindings_and_reclaims_expired_slot(), grant_expiry_reclaims_capacity_and_account_reconnect_reuses_preparation_slot(), Fixture, Option, SessionId, String, shared_grant_capacity_preserves_accepted_viewers_and_allows_both_terminal_outcomes()

### Community 139 - "Start Publication"
Cohesion: 0.41
Nodes (11): accepted_calls_reconcile_pending_start_and_reject_stale_real_acknowledgements(), directory_rows(), host_revision(), pending_row(), pending_start(), pending_start_fence_required_write_faults_roll_back_calls_and_exact_retries(), ConnectionId, Fixture (+3 more)

### Community 140 - "Mod"
Cohesion: 0.27
Nodes (7): CookieEffect, RequestContext, Debug, Formatter, Option, Result, String

### Community 141 - "Privileged Audit"
Cohesion: 0.25
Nodes (7): Runtime, FirstEntropyCollision, FirstEntropyFailure, Cell, Result, TestRuntime, Uuid

### Community 142 - "Mod"
Cohesion: 0.22
Nodes (8): Database, metadata_clock_bounds_match_the_named_safe_integer_limit(), metadata_statement(), Formatter, Option, Result, String, schema_version()

### Community 143 - "Auth Schema"
Cohesion: 0.31
Nodes (10): auth_schema_ddl_matches_the_pre_refactor_snapshot(), auth_schema_statements(), connection(), String, Vec, sqlite_account_caps_match_verifier_and_safe_integer_limits(), sqlite_rate_buckets_match_failure_count_and_digest_limits(), sqlite_receipts_match_payload_digest_and_retention_limits() (+2 more)

### Community 144 - "Auth Output"
Cohesion: 0.24
Nodes (9): AccountsObject, check(), fence(), OutputReply, OutputRequest, Env, Option, Result (+1 more)

### Community 145 - "Player Lifecycle.Test"
Cohesion: 0.27
Nodes (10): counts(), fixture(), join(), leave(), owner(), rename(), restore(), start() (+2 more)

### Community 146 - "Commands"
Cohesion: 0.24
Nodes (7): command_lookup_all_closed_receipt_variants_and_total_envelope_bounds(), CommandResult, PendingCommandOperation, PendingCommandState, Result, Self, Vec

### Community 147 - "Deployment Identity"
Cohesion: 0.29
Nodes (9): CloudflareVersion, DeploymentIdentity, handle(), identity(), Env, Result, SendWrapper, State (+1 more)

### Community 149 - "Terminal Regressions"
Cohesion: 0.33
Nodes (9): calendar_history_deadline_is_the_same_persisted_game_and_snapshot_value(), expired_call_receipts_after_cleanup_never_readmit_old_commands_or_clock_rollback(), host_revision(), logged_in(), ConnectionId, Fixture, String, terminal_grants_receipt_purge_and_release_route_survive_actual_sqlite_reopen() (+1 more)

### Community 150 - "Shared"
Cohesion: 0.24
Nodes (7): ManagementClient, Prepared, Result, Self, String, Client, HeaderValue

### Community 151 - "Mod"
Cohesion: 0.38
Nodes (9): execute(), prepare(), Prepared, CommandId, Option, PathBuf, Result, Write (+1 more)

### Community 152 - "Receipts"
Cohesion: 0.42
Nodes (6): AuthService<'_, D, R>, fingerprint(), CommandId, Option, Result, validate_receipt()

### Community 153 - "Edge"
Cohesion: 0.39
Nodes (8): command(), dispatch(), failure(), handle(), Env, Result, SendWrapper, State

### Community 154 - "Shared"
Cohesion: 0.22
Nodes (7): EnrollmentRaceRuntime, Box, FnOnce, Option, RefCell, Result, TestRuntime

### Community 155 - "Terminal Faults"
Cohesion: 0.50
Nodes (8): call_once(), clear_fault(), every_required_terminal_write_has_real_sqlite_ignored_and_aborted_rollback(), fault(), final_exit_ignored_and_aborted_writes_roll_back_all_access_and_history(), history_purge_faults_roll_back_every_child_route_release_intent_and_clock(), ConnectionId, Fixture

### Community 156 - "History Api"
Cohesion: 0.25
Nodes (5): history_queries_are_canonical_bounded_unique_and_filter_bound(), read_history(), HeaderMap, Option, Result

### Community 157 - "Game Audit Repairs.Test"
Cohesion: 0.25
Nodes (4): cancel(), fixture(), owner(), rows()

### Community 158 - "Player Recovery Boundaries.Test"
Cohesion: 0.25
Nodes (4): fixture(), owner(), recover(), rows()

### Community 160 - "Mod"
Cohesion: 0.39
Nodes (6): Bindings, get_backend_config(), Env, Option, Result, String

### Community 161 - "Sync Output"
Cohesion: 0.39
Nodes (5): GameService<'_, D, R>, ConnectionId, Option, Result, SyncResponse

### Community 162 - "Recovery Answers"
Cohesion: 0.50
Nodes (6): enroll(), normalize(), push(), Result, String, Zeroizing

### Community 163 - "Privileged Game Audit.Test"
Cohesion: 0.32
Nodes (5): actor(), adminRequest(), fixture(), owner(), rows()

### Community 164 - "Game Schema"
Cohesion: 0.62
Nodes (6): game_record_statement(), recovery_coordination_statements(), String, Vec, spectator_statements(), statements()

### Community 165 - "Previous Command Game Schema"
Cohesion: 0.62
Nodes (6): game_record_statement(), recovery_coordination_statements(), String, Vec, spectator_statements(), statements()

### Community 166 - "Previous Read Game Schema"
Cohesion: 0.62
Nodes (6): game_record_statement(), recovery_coordination_statements(), String, Vec, spectator_statements(), statements()

### Community 167 - "Previous Saved Game Schema"
Cohesion: 0.62
Nodes (6): game_record_statement(), recovery_coordination_statements(), String, Vec, spectator_statements(), statements()

### Community 168 - "Shared"
Cohesion: 0.29
Nodes (6): InspectHostedRead, Cell, FnOnce, Result, Sqlite, T

### Community 169 - "Shared"
Cohesion: 0.29
Nodes (5): FnOnce, Result, Row, T, Vec

### Community 170 - "Storage Bounds.Test"
Cohesion: 0.43
Nodes (6): cancelAndReadHistory(), connect(), owner(), seedCalls(), startedGame(), sync()

### Community 171 - "Backend Once Error"
Cohesion: 0.33
Nodes (5): failed_get_caches_a_safe_error_without_retrying(), NeverRead, Option, Result, String

### Community 172 - "Backend Once Success"
Cohesion: 0.33
Nodes (5): get_initializes_lazily_and_never_acquires_again(), NeverRead, Option, Result, String

### Community 173 - "Typed Struct"
Cohesion: 0.33
Nodes (5): constructor_acquires_every_declared_key_once(), Box, Error, Result, serde_deserializes_the_backend_struct_with_optional_defaults()

### Community 174 - "Links"
Cohesion: 0.53
Nodes (4): AuthService<'_, D, R>, AccountId, CommandId, Result

### Community 175 - "Host Transfer"
Cohesion: 0.33
Nodes (3): Formatter, Result, Self

### Community 176 - "Discovery Boundaries.Test"
Cohesion: 0.53
Nodes (5): directory(), get(), lobby(), owner(), unchangedDomain()

### Community 177 - "Player Recovery Review Regressions.Test"
Cohesion: 0.40
Nodes (4): fixture(), owner(), recover(), restore()

### Community 178 - "Frontend"
Cohesion: 0.40
Nodes (5): constructor_is_zero_argument_and_uses_only_public_build_inputs(), Box, Error, Result, serde_deserializes_the_public_struct_with_optional_defaults()

### Community 179 - "Frontend Once Error"
Cohesion: 0.27
Nodes (4): public_singleton_caches_the_constructor_outcome(), Result, get_initializes_the_public_config_and_returns_the_same_instance(), Result

### Community 180 - "Shared"
Cohesion: 0.33
Nodes (5): history_cells(), D, Error, Result, Vec

### Community 181 - "Maintenance"
Cohesion: 0.50
Nodes (3): AuthService<'_, D, R>, Option, Result

### Community 182 - "Mod"
Cohesion: 0.60
Nodes (4): RemovalGrant, RemovalReleaseAck, AccountId, OperationId

### Community 183 - "Prestart Maintenance"
Cohesion: 0.60
Nodes (4): migrated_prestart(), migrated_prestart_counter_retirement_ignored_or_aborted_delete_rolls_back_clock_and_release(), migrated_prestart_idle_cancellation_retires_obsolete_counter_in_real_maintenance(), Fixture

### Community 185 - "Protocol"
Cohesion: 0.70
Nodes (4): Prepared, Result, validate_receipt(), validate_response()

### Community 186 - "Mod"
Cohesion: 0.70
Nodes (4): enrollment_url(), link_token(), String, Zeroizing

### Community 189 - "Game Operator Schema"
Cohesion: 0.50
Nodes (3): String, Vec, statements()

### Community 190 - "Previous Command Operator Schema"
Cohesion: 0.50
Nodes (3): String, Vec, statements()

### Community 191 - "Previous Read Operator Schema"
Cohesion: 0.50
Nodes (3): String, Vec, statements()

### Community 192 - "Previous Saved Operator Schema"
Cohesion: 0.50
Nodes (3): String, Vec, statements()

### Community 193 - "Accounts"
Cohesion: 0.50
Nodes (3): prepare(), Prepared, Result

### Community 194 - "Audit"
Cohesion: 0.50
Nodes (3): prepare(), Prepared, Result

### Community 196 - "Origin"
Cohesion: 0.50
Nodes (3): Result, String, validate_origin()

## Knowledge Gaps
- **20 isolated node(s):** `Input`, `AuthService<'_, D, R>`, `AccountsObject`, `GameObject`, `super::directory::GameDirectoryObject` (+15 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **13 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `migrate_directory()` connect `Directory` to `Directory`?**
  _High betweenness centrality (0.000) - this node is a cross-community bridge._
- **Why does `GameCode` connect `Games` to `Games`?**
  _High betweenness centrality (0.000) - this node is a cross-community bridge._
- **Why does `Record` connect `Terminal` to `Game`, `Discovery`, `Player Lifecycle`, `Games`?**
  _High betweenness centrality (0.000) - this node is a cross-community bridge._
- **What connects `Input`, `AuthService<'_, D, R>`, `AccountsObject` to the rest of the system?**
  _20 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Game Accounts` be split into smaller, more focused modules?**
  _Cohesion score 0.05537757437070938 - nodes in this community are weakly interconnected._
- **Should `Game` be split into smaller, more focused modules?**
  _Cohesion score 0.07850707850707851 - nodes in this community are weakly interconnected._
- **Should `Games Core` be split into smaller, more focused modules?**
  _Cohesion score 0.06247836621668398 - nodes in this community are weakly interconnected._
## Structural extraction limitations

AST health reports 1103 dangling-endpoint edges and 541 collapsed directed same-endpoint edges. These are navigation limitations, not runtime validation. The separate documentation graph remains stale.
