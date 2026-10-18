# Backend `/ponytail full` fixes — 2026-10-18

Local repairs for the four confirmed findings in the [2026-10-17 scan](backend-ponytail-scan-2026-10-17.md), based on published commit `f667ad36dbcfc5f7e7d40244d8d1a8fdb53aa19f`. Implementation began on October 17; follow-up verification continued on October 18 UTC. At the verification freeze, the user had authorized fixes but not publication. The user subsequently authorized committing and pushing this reviewed checkpoint; deployment remains unauthorized. The machine-readable evidence preserves its original pre-publication status.

## Repairs

| Finding | Change | Regression coverage |
| --- | --- | --- |
| **PT-HISTORY-EXPIRY** | Final local account-expiry denial fences its actual timestamp through the existing checked Game metadata write. The owner syncs that denial before returning. Successful final reads remain write-free and gain no await. | Native equality/rollback; genuine Accounts proofs valid at receipt followed by local expiry, for detail and summary; ignored fence and failed sync return unavailable. |
| **PT-READ-EXPIRY-SAMPLE** | Carry the inner transaction's sampled timestamp with its result; fence that timestamp for expiry/authorization and identity/assignment denials, not the earlier outer sample. | Original unchanged SQLite probe, plus durable inner-sample equality/rollback and ignored-fence regression. |
| **PT-RECOVERY-CAP** | Retain the first reply's deadline across the final owner await for recovery and PUT/DELETE answer maintenance. Fence an expired original cap before inspecting the later reply, and use the minimum of both deadlines. | Genuine host resume between replies; success immediately before the original cap, denial at equality, no cookie/private output on denial, and rollback retry rejection. |
| **PT-RECOVERY-PRESENCE** | Shared player detachment reports whether current connections were removed. Recovery advances the host revision transactionally only in that case. Leave reuses the same result instead of repeating the query. | Connected/offline/prepared cases, replay after reconstruction, ignored host-revision write with rollback, and real WSS plus conditional host Sync. |

Only six production files change. No new dependency, configuration, schema, public route, authority type or renewal policy is introduced. Existing epoch/session invalidation, stable player identity/board, exact receipt/proof binding, socket-close durability, readbacks, selective auditing and approved authorization-time snapshot policy remain.

The History repair durably fences the **Game owner's** exact observation. It does not introduce or claim a new global Accounts fence for that final local denial. The dedicated-read finding remains demonstrated at the native service boundary; no public exploit is claimed.

## Original failing probes, unchanged assertions

All four original Workerd probes now pass against the rebuilt Worker:

- History: the Game clock floor reaches the observed account expiry, and the fresh public rollback retry returns **503**, not 200.
- Recovery and answer management: a genuine resume can extend the later reply's deadline, but output at the original cap returns **401**, without a cookie.
- Recovery presence: connected count changes **1 → 0**, host revision **4 → 5**, and conditional Sync returns a changed snapshot instead of `up_to_date: true`.

The original native probe also passes, with its probe source hash unchanged and its isolated production reads prefix byte-identical to the repaired repository prefix: expiry `1800000001000`, persisted floor `1800000001000`, rollback `1800000000999`, `fresh_read_succeeded: false`. This is one selected test with 212 other tests intentionally filtered, not a complete native suite. The original failing run remains archived separately: test exit 100 and floor `1800000000999`.

## Verification

The complete unchanged-source retry, `brews-ponytail-fixes-gates-02`, exits **0** across all requested local lanes:

| Check | Verified result |
| --- | --- |
| Native debug | **971 passed**, zero skips |
| Optimized `release-assertions` | **971 passed**, same exact test inventory, zero skips |
| Doctests | **12 passed** |
| General Worker | **739 passed** across 39 physical suites, zero skips/runtime errors |
| Current snapshot-policy acceptance | **20 passed**, separate exact inventory |
| Real local HTTPS/WSS | **375 checks passed** across gameplay, operator, dedicated reads and privileged audit; labels are unique within each harness |
| Python/tooling | **136 passed** |
| Formatting and strict native/Wasm Clippy | Passed |

All **299 release-input hashes** and complete Worker artifact hashes stayed unchanged through the integrated run. The first failed run and passing retry bind to the same source and Worker artifact maps. The [machine-readable evidence](backend-ponytail-fixes-2026-10-18-evidence.json) archives both runs, command descriptors, safe observations, exact passing case inventories, original probe hash checks, reviews and graph verification. Raw credential-bearing output is not persisted.

- Focused Worker diagnosis: all **14 new cases pass**, with other cases deliberately filtered.
- Complete affected Worker suites: **57 passed**, zero skips or runtime errors.
- Two independent bounded supplied-source static reviews found no blocking findings in their assigned expiry/cap and presence scopes. They did not execute tests or provide whole-slice/provider acceptance. An optional later-write rollback test was suggested; existing transactional/readback checks and fault coverage were retained without expanding scope.

### First full run remains failed evidence

`brews-ponytail-fixes-gates-01` passed both native profiles, doctests, Clippy and Worker build; Worker shard 2 exited 0 and shard 1 exited **1**. Its individual assertion details were not retained by the existing runner. The cause remains **unconfirmed**. The 14-case diagnostic and complete 57-case owning-suite rerun passed without changing sources or assertions. The complete retry adds bounded Worker failure diagnostics, not a runtime change, altered timeout or relaxed acceptance.

The complete unchanged-source retry passed, but does not establish the first failure's cause. No failure is relabeled passed and no unproven repair is inferred from that retry.

## Review and release boundaries

The code-only Graphify corpus is refreshed and its **267 source hashes** match. Structural diagnostics still report 1,103 dangling-endpoint edges and 541 collapsed directed same-endpoint edges; they are navigation limitations, not runtime validation. The separate documentation graph remains stale.

The archived `command-lookup-final-return.{config.mjs,test.js}` files remain byte-identical. Their old receiving-cut assertion is superseded by the approved snapshot policy, not rewritten or counted as passing.

Unrelated cleanup suggestions, exhaustive migration/schema review, frontend work and the seven external provider/production gates are outside this repair. No deployment, provisioning, secret change, restore/load certification or production acceptance is performed. Commit/push authorization does not close these boundaries or alter the archived failed attempts.
