# Maximum-configured-game resource observations

This is a **local measurement continuation inside Slice 9**, not a new approved product slice, traffic forecast or production capacity certificate. It does not select weaker KDF settings, change schemas/queries/limits, provision an account or namespace, deploy or authorize restore. The user separately approved [authorization-time response snapshots](../plans/protected-response-policy.md); the resource probe does not choose that policy.

The published readiness foundation is `b158f1375d9de0b3308de47da1a5bdd1e9647047`. Its original [review/evidence](../reviews/runtime-readiness-slice-2026-10-13.md) remains an immutable checkpoint; [publication reconciliation](../reviews/runtime-readiness-slice-publication-reconciliation.json) records exact remote readback. The resource continuation was separately published as `1f742c92b0c394864644890d8478f4563bcae5e4`; its [publication reconciliation](../reviews/game-resource-continuation-publication-reconciliation.json) preserves the exact verified scope. Publication is not provider capacity certification.

## Run the observations

From the repository root with the pinned/locked toolchain and dependencies already installed:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s backend/tools/tests -v
PYTHONDONTWRITEBYTECODE=1 python3 backend/tools/game_resource_probe.py \
  --output "$TMPDIR/brews-game-resource-01"
```

Use a **new private output directory outside the repository**. The probe reuses the existing local runner's process deadlines, opaque test identities, source-before/after hashes, fresh Worker build and stable artifact checks. It runs actual native `game_resources` tests in debug and optimized `release-assertions`, then the actual local Workerd `game_resources.test.js`. It stores structured safe metric summaries and command exit/output hashes, never raw child output or private assertion payloads. Resource values must be finite/nonnegative/JavaScript-safe; containers and strings are bounded; credentials, private snapshots/boards/bindings and credential-like identifiers are denied by the metric parser. This is a producer-specific safe-data boundary, not a general arbitrary-secret detector.

The normal full native and Worker suites also execute the resource tests. Existing unrelated test assertions/deadlines and the retained final-return diagnostic must remain unchanged. A passing isolated resource probe proves only its named observations. The full release runner now requires the approved snapshot-policy acceptance, verified in the [CI/CD continuation](../reviews/cicd-snapshot-2026-10-14.md); the older receiving-cut assertion remains a separately preserved historical failure. Production release remains blocked by the seven remaining external Slice 9 gates, not by that superseded assertion.

## Workload and interpretation

The measured branch uses current domain limits for the numeric pool, board side, player and spectator occupancy. **No free cells** maximizes VALUE-cell storage for that branch. Actual production admission, Start, calls, no-winner terminal History and reservation-release operations create the state; direct SQL board/call inserts or synthetic socket responses are not an acceptable substitute.

Native logical connections are not native WebSockets. TestRuntime's deterministic entropy is fixture-only and not a CSPRNG claim. Actual Workerd socket observations must identify the real connected subset, received/ACKed frames and bytes; they must not extrapolate nominal occupancy into observed fanout. A capacity-configured game does not imply maximum active account connections, every possible free-cell arrangement, worst-case rejected/replayed commands or a sustained/concurrent venue workload.

Per-owner native SQLite `page_count × page_size`, including freelist pages, describes the allocated SQLite page image, **not** live logical payload, provider charged storage, peak memory or process RSS. Query-port calls, materialized rows and SQLite successful-statement changed-row counts describe that instrumentation cut; they are not provider billable SQL rows/requests, and attempted writes may not equal net committed changes. Direct test-observation queries must be excluded from operation accounting or explicitly inventoried. SDK cursor rowsRead/rowsWritten, when available, are local runtime observations and not native-counter equivalence.

Wall time is not active provider CPU. Serialized response/frame **byte counts** are observations, not permission to emit private payloads or evidence that a stale protected response became authorized. Retention/authority/ACK assertions must remain intact while measuring.

## Still needed for DO-103

- Owner-provided peak games/events/History forecast and time horizon. No numeric values are assumed here.
- Applicable account/plan/configured provider quotas, maximum-game **plus forecast** load, storage/row work/requests/provider CPU/duration/alarms/WSS amplification and measured two-times headroom.
- Deployed clock/CSPRNG/SDK/Unicode and functional login/recovery validation. The [dedicated Argon2 performance gate is removed](../plans/argon2-default-policy.md); existing library defaults and PHC bounds remain.
- Other maximal configuration/active-connection/error/replay branches and worst-case workloads not actually exercised.
- Authorized isolated provider backup/copy/log/restore, provisioning/secret/operator and deployed release/security/load procedures.
- Final Slice 2/6/7 reviews/acceptance beyond the separately approved and locally verified `RUNTIME-FINAL-01` response contract.

Stop for architecture/cost review if the actual capacity gate fails. Do not convert local page counts, one operation sample or absence of a timeout into production headroom. No original production checkbox is closed by this continuation alone.
