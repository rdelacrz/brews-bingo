---
type: "query"
date: "2026-10-04T17:25:51.520921+00:00"
question: "How is asynchronous cleanup of stale History entries handled compared with EventBridge-style scheduling?"
contributor: "graphify"
outcome: "corrected"
correction: "DO-071, DO-074, DO-075, DO-084 and DO-094 are approved in the current canonical ledger; do not use old pending graph labels."
source_nodes: ["purge_expired_history", "History and retention"]
---

# Q: How is asynchronous cleanup of stale History entries handled compared with EventBridge-style scheduling?

## Answer

Expanded from graph vocabulary: history alarm expiry purge. Existing graph supplied navigation only; its pending labels are stale. Canonical DO-071/074/075/084/094 are approved: fixed UTC three-calendar-month deadline, immediate read denial/code reuse eligibility at expiry, idempotent Game/Directory purge guarded by stable game_id, bounded durable retry and one earliest-deadline alarm per Object. Physical scheduling/reconciliation integration remains unimplemented. Cloudflare Cron Triggers are the periodic Worker equivalent; Durable Object Alarms are the selected per-owner deadline mechanism. Optional periodic watchdog remains a recommendation, not a selected new requirement. See current canonical docs and official Cloudflare Alarms/Cron Triggers docs.

## Outcome

- Signal: corrected
- Correction: DO-071, DO-074, DO-075, DO-084 and DO-094 are approved in the current canonical ledger; do not use old pending graph labels.

## Source Nodes

- purge_expired_history
- History and retention