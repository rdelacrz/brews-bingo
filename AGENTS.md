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

The schema review proceeds in ledger order. You may batch independent approvals from the same category into one `clarify` call; ask dependent decisions separately, track each response independently, and reconcile revisions before marking downstream items approved. DO-032–DO-047 and DO-049–DO-050 are approved; current cursor: DO-048 (next pending item). DO-047’s role-switch/Leave behavior and DO-050’s answer-normalization choice are reconciled in the Durable Object, LLD, HLD and API planning documents; implementation-level transaction/session/verifier details remain pending. DO-021's case-sensitive username uniqueness/login revision and DO-045's case-sensitive per-game alias revision are approved. If a `clarify` call times out, is cancelled, skipped, or otherwise returns no answer for an item, do not infer or supply an answer yourself and do not advance that item; leave it pending and wait for the user to return and answer. On resumption, present the same unanswered item(s). Account lifecycle is `PendingEnrollment`, `Verified`, `ResetRequired`; `disabled_at` alone represents disablement, with no redundant stored boolean.

Never expose or persist passwords, bearer tokens, or other secrets in docs, graph labels, logs, or examples.
