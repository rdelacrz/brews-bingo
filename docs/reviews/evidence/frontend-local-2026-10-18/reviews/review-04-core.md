No concrete blockers or nonblocking bugs found within this bounded closure review.

Source inspection supports the repairs:

- [Session binding](/root/.hermes/cache/scratch/brews-ui-review-04/source/shared/ui/src/session.rs:47) checks account, scope, exact deadline and eligibility inside the [cookie lock held through password POST](/root/.hermes/cache/scratch/brews-ui-review-04/source/shared/ui/src/client/browser.rs:252).
- Queue cancellation bounds both lock waits, drops queued secret bodies on caller drop, and rechecks caller validity after preflight before dispatch.
- [Keyed](/root/.hermes/cache/scratch/brews-ui-review-04/source/shared/ui/src/components/keyed.rs:4) places the changing key in a sibling list, providing the remount boundary for private hook state. Account, scope, deadline and role changes advance the epoch used by protected views.
- Auth request/reply shapes match the supplied backend, including fragment tokens, secret-free receipts, fixed enrollment deadlines and reset completion without login. No concrete accessibility regression surfaced.

Read-only source review only. Existing regression tests were inspected, not executed; compilation and browser behavior remain for the parent’s validation.