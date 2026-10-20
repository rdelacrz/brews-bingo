No blockers or concrete nonblocking bugs found in the requested scope.

- `Busy` occurs before auth mutation dispatch: queue expiry/cancellation or stale-authority rejection. Dispatched transport failures become `UnknownOutcome`. See [browser.rs:261](shared/ui/src/client/browser.rs:261) and [browser.rs:406](shared/ui/src/client/browser.rs:406).
- The shared recheck skip cannot grant authority; login/redemption replies do not install session authority. Successful, uncertain, and authorization-failed attempts still refresh. See [auth/mod.rs:10](shared/ui/src/features/auth/mod.rs:10).
- `hosted_game` and `last_admin` map to fixed client messages only for HTTP 409, matching backend semantics. Server message text is ignored. See [client/mod.rs:412](shared/ui/src/client/mod.rs:412).

Source inspection also covered expiry, identity invalidation, fragment scrubbing/replay, response validation, hooks, browser paths, and accessibility, with auth wire shapes checked against backend source.

No files changed or runtime checks performed; parent retains final compile/browser gates.