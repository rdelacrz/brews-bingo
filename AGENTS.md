# Agent guidance

## Project state and planning sources

Brews Bingo is still in planning. Do not implement the application, run application tests, create accounts/tokens, provision infrastructure, or deploy without explicit authorization. Do not commit or push later planning edits unless explicitly asked for them.

Use these documents as the planning sources of truth:
- `plans/requirements.md` — business requirements and scope.
- `plans/hld.md` — architecture and high-level policies.
- `plans/lld.md` — domain rules, workflows, and implementation planning.
- `plans/api-design.md` — API operations and request/response contracts.
- `plans/durable-object-design.md` — storage schemas and the sequential decision ledger.
- `plans/research.md` — hosting/provider evidence and caveats.

The schema review proceeds in ledger order. Batch as many independent pending approvals as possible in one `clarify` call, including across categories; ask dependent decisions separately, track each response independently, and reconcile revisions before marking downstream items approved. DO-032–DO-060 are approved; current cursor: DO-061 (next pending item). DO-047’s role-switch/Leave behavior, DO-048’s retained post-return Leave timestamp, DO-050’s answer-normalization choice, DO-051’s Argon2id/PHC verifier profile, DO-052’s recovery transaction/session/socket-fencing policy, DO-053’s spectator record/grace-deadline fields, DO-054’s disconnect/reconnect/expiry race ordering and DO-055’s ParticipantSessionRecord fields/live-final access policy are reconciled in the Durable Object, LLD, HLD and API planning documents; production KDF costs/caps, exact Unicode implementation, throttling mechanisms/thresholds, hibernation reconstruction, physical session/socket integration and DO-061+ decisions remain pending. DO-021's case-sensitive username uniqueness/login revision and DO-045's case-sensitive per-game alias revision are approved. If a `clarify` call times out, is cancelled, skipped, or otherwise returns no answer for an item, do not infer or supply an answer yourself and do not advance that item; leave it pending and wait for the user to return and answer. On resumption, present the same unanswered item(s). Account lifecycle is `PendingEnrollment`, `Verified`, `ResetRequired`; `disabled_at` alone represents disablement, with no redundant stored boolean.

Never expose or persist passwords, bearer tokens, or other secrets in docs, graph labels, logs, or examples.
