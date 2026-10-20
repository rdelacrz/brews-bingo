No blocking regressions identified by supplied-source inspection. One nonblocking functional gap remains.

- **P3 — Users loses actionable guard feedback.** [client/mod.rs:421](/root/.hermes/cache/scratch/brews-ui-review-05/source/shared/ui/src/client/mod.rs:421) maps `hosted_game` and `last_admin` responses to generic `Conflict`. VIEW-15 requires the safe blocking reason and an appropriate next step. Reproduce by attempting to disable/delete another host assigned a nonterminal game: the UI advises refreshing without explaining the assignment guard. Minimal repair: map these known codes to typed, fixed messages. Add a decoder test and a browser assertion that the hosted-game rejection explains transferring or ending the game.

The review04 fixes are confirmed in source:

- [Home/Games:12](/root/.hermes/cache/scratch/brews-ui-review-05/source/shared/ui/src/features/home/mod.rs:12): Home stays public; Games requires normal account access and supplies setup/reset guidance.
- [BoardGrid:132](/root/.hermes/cache/scratch/brews-ui-review-05/source/shared/ui/src/components/mod.rs:132): the labelled region owns focus and overflow; cells retain coordinates and non-color match labels.
- [Large-History test:65](/root/.hermes/cache/scratch/brews-ui-review-05/source/platform/web/tests/endings.mjs:65): explicitly selects `LargeOne`, focuses its region, uses ArrowRight, and asserts last-column visibility.

Within scope, the remaining implementation aligns with the plans: bounded account-only lists, lifecycle/self-action guards, ephemeral private-link handoff and receipt handling, fixed History expiry, read-only final boards, API-first asset routing, and restrictive static headers.

This was read-only inspection inside the supplied snapshot. I made no edits or network calls and ran no compile, browser, native, or packaging tests; their results remain with the parent.