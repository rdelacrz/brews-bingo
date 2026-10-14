# Protected HTTP response policy

## RUNTIME-FINAL-01 — approved authorization-time snapshots

On 2026-10-14 the user explicitly selected: **“It may finish because it was authorized before revocation. Revocation blocks subsequent requests.”** This supersedes the receiving-cut revocation requirement recorded during the Slice 6/7 reviews. It does not weaken authentication for a new request or extend any expiry.

### Authoritative cuts

A protected HTTP response is a bounded snapshot authorized at the relevant server-owned cuts, not a promise that remote state is unchanged at edge receipt or browser delivery:

- **Accounts:** the authoritative validation of the exact account/session/credential epoch/role/scope and original expiry used by the operation.
- **Game:** the authoritative local transaction validating the selected participant/final grant, assignment, lifecycle, private projection or command result and its original non-extending cap. Account operations use the exact bound Accounts proof; this is a sequential Accounts-then-Game validation, not an atomic transaction across owners.
- **Directory:** the authoritative reservation/creation/discovery metadata cut when the response depends on Directory facts. It is a separate sequential cut, not simultaneous Accounts/Game/Directory state.

Existing post-await recuts, per-invocation challenges, exact request/projection digests, denial fences, durability checks and caps remain in place. A request merely starting before revocation is not enough: it must actually obtain the required valid authoritative authorization. Revocation that precedes a required later authorization cut can still deny an in-flight request. Once all required cuts have successfully authorized its snapshot, a competing mutation may overtake that response; the already-authorized response **may** complete with the selected bytes. No new mutation waiting, read locks or response-cancellation protocol is introduced.

### Revocation and content changes

Recovery/session replacement, credential-epoch invalidation, disable/reset/remove, participant deletion and final Exit take effect for subsequent authoritative validations without waiting for prior HTTP responses. A request whose required authorization follows the committed revocation must not obtain protected data using the revoked authority. Earlier invocation ACKs may not authorize a new request, even for identical payloads or the same actor.

Alias/recovery-answer/game-content/assignment changes after the successful relevant cuts may likewise overtake a previously authorized snapshot. A subsequent authorized read reflects the new state; it must not reuse an earlier invocation's projection or authorization. HTTP snapshot completion is not evidence that a viewer still has current authority or that the snapshot is current when displayed.

### Limits that are not relaxed

- Original session, admission, idle, receipt, final-grant and History deadlines remain non-extending. Existing receiving-side deadline checks and durable expiry/rollback fencing still deny expired output, including equality at the cap. This policy is a revocation/content-ordering decision, not an expiry allowance.
- Malformed or substituted proofs, cross-actor/cross-game responses, failed owner durability, unauthenticated requests and stale-invocation ACKs continue to fail closed.
- Mutation commits, idempotency receipts, cookie rules, passive-read side effects, selective auditing and cleanup/retention rules are unchanged. Owner audit success records the authorized owner operation, not downstream delivery or current browser entitlement.
- WebSocket attachment, per-frame authorization/deadline/revision/ACK and terminal best-effort delivery continue to follow their separately approved transport contracts. This HTTP decision does not waive those controls.
- Already transmitted bytes cannot be recalled; no physical network/browser-receipt invalidation guarantee is made.

### Acceptance and historical evidence

Retain `docs/reviews/evidence/command-lookup-final-return.config.mjs` and `command-lookup-final-return.test.js` unchanged. Their old denial assertion remains genuine evidence of failure under the **superseded** receiving-cut requirement; it is not a passing assertion under that requirement. New acceptance must independently exercise real authoritative mutations after genuine same-invocation authorization, allow the held snapshot, deny fresh revoked-session requests and verify new-state reads, request binding and receiving-side expiry/fencing.

Release reports must distinguish the new acceptance results from the original probe's raw exit/result. An infrastructure error, an unrelated old-probe failure, skipped new coverage or malformed evidence cannot become success. A passing local snapshot contract is not production certification and does not close the eight external production gates or unrelated slice review items.
