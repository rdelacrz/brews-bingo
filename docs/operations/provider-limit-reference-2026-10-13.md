# Public provider-limit reference — local Slice 9

Retrieved **2026-10-13** from public Cloudflare documentation, all three responses HTTP 200. This refresh is read-only; no Cloudflare account, namespace, secrets or deployment was accessed. The actual account plan, custom/configured limits, forecast and target measurements are still unknown/unverified. These references do **not** close the applicable-quota or two-times-headroom gate.

## Observed public documentation

| Boundary | Published value / caveat | Source |
| --- | --- | --- |
| API Worker CPU | Workers Free: **10 ms**; Workers Paid table: **5 min**. Consult the CPU section and actual configured/account limit before using this as an applicable budget. | [Workers limits](https://developers.cloudflare.com/workers/platform/limits/) (page says updated 2026-10-12) |
| Worker memory | **128 MB**, both plans in the published table. This is not a measured application/Wasm peak or a per-call allocation allowance. | Same |
| Worker request/subrequest allowances | Free **100,000 requests/day**, **50 subrequests/request**; Paid table has no request-count limit and **10,000 subrequests/request**. Other billing/daily/recursion limits and account-specific configuration still need review. | Same |
| SQLite-backed Durable Object storage | **10 GB per Object**; Workers Free **5 GB total account storage**, Paid table says unlimited account storage. The footnotes specify decimal GB, not GiB. | [Durable Object limits](https://developers.cloudflare.com/durable-objects/platform/limits/) (page says updated 2026-06-01) |
| Durable Object active CPU | **30 seconds default**, configurable up to **5 minutes**. Incoming request/message and eviction/timer semantics differ from API Worker CPU; wall duration is not active CPU. | Same |
| SQLite row/value and received WSS message | String/BLOB/table row: **2 MB**; received WebSocket message: **32 MiB**. These are provider ceilings, not application policy increases. | Same |
| SQL work/billing evidence | `SqlStorageCursor.rowsRead` and `rowsWritten` may increase as the cursor is iterated; final values are used for SQL billing. Collect them from actual exhausted target cursors. | [SQLite storage API](https://developers.cloudflare.com/durable-objects/api/sqlite-storage-api/) |

Native SQLite `VmStep`, `Sort`, `FullscanStep` and returned-row counts are **not** the provider's `rowsRead`/`rowsWritten`, billable requests, active CPU or isolate memory. A sequential local Argon2 wall sample cannot prove that either API Worker or Durable Object CPU budgets pass or fail. It cannot select hashing placement or weaker security parameters. Placement, current explicit work factors, PHC bounds and admitted concurrency remain subject to actual target evidence and the approved policy.

## Required applicability and capacity evidence

Before closing DO-103, separately authorize account/target validation and obtain the owner-provided peak event/History forecast. Record the applicable account/plan/configured limits, source/date, measured maximum legal game and forecast, per-owner storage/row work/requests/CPU/duration/alarms/WSS amplification, and a consistent unit/interval comparison. Require usage at or below half each applicable quota; stop for architecture/cost review if any budget fails. No numeric forecast, extrapolated headroom, custom quota or target-use measurement is invented here.

Raw public retrieval text and SHA-256/provenance metadata remain in private scratch and are referenced by the Slice 9 evidence record. Provider documentation can change; re-fetch at the actual release gate rather than treating this snapshot as permanent account-specific certification.
