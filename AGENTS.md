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

The schema review proceeds in ledger order. Batch as many independent pending approvals as possible in one `clarify` call, including across categories; ask dependent decisions separately, track each response independently, and reconcile revisions before marking downstream items approved. All 107 DO ledger items are approved; no pending schema decisions remain. DO-021 username and DO-045 per-game alias case-sensitive comparisons are approved. DO-032–107 decisions and cross-document summaries have been reconciled; implementation-only items remain unverified (production KDF costs/caps, exact Unicode/runtime support, SDK/SQL/outbox/alarm wiring, quota measurements and actual release-gate test results). Do not claim these as completed or tested. If a future `clarify` call times out, is cancelled, skipped, or otherwise returns no answer for an item, do not infer or supply an answer; leave it pending and wait for the user to return. Account lifecycle is `PendingEnrollment`, `Verified`, `ResetRequired`; `disabled_at` alone represents disablement, with no redundant stored boolean.

Never expose or persist passwords, bearer tokens, or other secrets in docs, graph labels, logs, or examples.
