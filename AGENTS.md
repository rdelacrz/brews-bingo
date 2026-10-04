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

The schema review proceeds in ledger order. You may batch independent approvals from the same category into one `clarify` call; ask dependent decisions separately, track each response independently, and reconcile revisions before marking downstream items approved. Current cursor: DO-032 (next pending item after DO-029–DO-031 approvals). If a `clarify` call times out, is cancelled, skipped, or otherwise returns no answer for an item, do not infer or supply an answer yourself and do not advance that item; leave it pending and wait for the user to return and answer. On resumption, present the same unanswered item(s). Account lifecycle is `PendingEnrollment`, `Verified`, `ResetRequired`; `disabled_at` alone represents disablement, with no redundant stored boolean.

## Graphify

Before answering repository architecture, relationship, or planning questions, load and follow the `graphify` skill (`skill_view(name="graphify")`) and use its query/path/explain workflows to retrieve relevant information. When source documents change, run the skill’s incremental Graphify update and verify the refreshed outputs. Treat the graph as a navigation aid, not authority: cite/check the planning source documents, preserve EXTRACTED/INFERRED/AMBIGUOUS distinctions, and disclose when the graph is stale.

Never expose or persist passwords, bearer tokens, or other secrets in docs, graph labels, logs, or examples.