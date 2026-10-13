# Local maximum-configured-game resource measurement implementation

Owned files only:
- backend/tests/game_resources.rs
- backend/worker-tests/game_resources.test.js

No production, schema, dependencies, config, existing tests/fixtures/runner, AGENTS, project docs or graph were edited by this work. Parent-owned concurrent changes are outside this report. No commit, push, Worker rebuild, deployment, provider mutation or provisioning was performed. Parent owns graph refresh/build/full frozen gates.

Implemented normal bounded tests (neither ignored nor stubbed): real service/API admissions for 20 players and 50 spectators; maximum numeric pool 1000, side 10, all 2000 cells VALUE because free cells are disabled; actual Start and terminal no-winner History/cleanup/release. Maximum configuration constants are imported natively and extracted from the actual Rust domain source through Vite raw import in Worker. This exact branch is not proof of every maximum configuration or worst-case workload.

Native: AuthService management create/enrollment/setup, DirectoryService and GameService with real SQLite migrations, deterministic TestRuntime explicitly labeled, 70 actual service logical connections. Real 1000 accepted manual calls, not seeded rows. Instrumented Database delegates identical SQL and parameter slices, counts query/execute calls, materialized rows and total_changes deltas. Storage introspection bypasses application-work counters. Page_count*page_size is SQLite page-image allocation, not logical payload, resident memory or provider billable rows; the databases are in-memory SQLite. Changed rows count successful statement changes, not net committed rows. Calls stage also includes actual host account_view reads for fresh revisions. No actual native sockets, OS entropy or WSS is claimed.

Workerd: real 70 API admissions, 20 boards, one manual call, measured account/player/spectator Sync JSON byte sizes. Exact sample of 2 players + 1 spectator using native Workerd Upgrade WebSocket pairs. ACK queue is settled before stage boundaries; all three actual close flags are checked. This is not network TLS/WSS or full 70-socket load. Actual observed totals: 12 frames / 63033 JSON bytes, 11 snapshot ACK frames / 1848 bytes, including one terminal spectator notice. No nominal fanout estimate. Frame count equals snapshots plus terminal notices; ACK count equals snapshot count. All metrics recursively validated finite and nonnegative. Exact table cardinalities, 20 boards/2000 cells and pool bound 1000 are asserted. Workerd PRAGMA page_count/page_size/freelist denied by SQLite authorization; actual SDK databaseSize and application table counts are used instead, with provider-owned underscore tables excluded. SDK cursor row-work measurement remains unavailable after observer attempts caused runtime traps. No fake cursor results or replaced business logic remain in the final test.

Passed final focused commands:
- cargo nextest run -p brews-backend --all-features --locked --test game_resources --success-output immediate
- cargo nextest run -p brews-backend --all-features --locked --test game_resources --cargo-profile release-assertions --success-output immediate
- cargo clippy -p brews-backend --all-features --test game_resources --locked -- -D warnings
- cargo fmt --all; cargo fmt --all --check
- (cd backend) npm run test:worker -- worker-tests/game_resources.test.js --reporter=default --reporter=json --outputFile=/root/.hermes/cache/scratch/brews-game-resources-worker-sixth.json

Development outer subprocess bounds: native nextest 200 seconds, Clippy 120 seconds, Worker attempts 90–180 seconds. Worker test-only timeout is 300000ms; native full-call loop checks total elapsed <180s. Original tests/timeouts were not relaxed. No whole-suite build or Worker build was run.

Attempts retained:
- native-first: compile failed because chosen UUID helper did not exist and spectator mutation cookie is nested; corrected locally.
- native-debug: passed before total_changes counter refinement.
- native-release: final owned native source passed, one test, ~15.081s wall.
- native-debug-final: final owned native source passed, one test, ~45.929s wall.
- worker initial, second, third: runtime observer trap, stopped within bounded attempts; cursor observation removed, SDK row-work left pending.
- worker-fourth: page PRAGMA denied; corrected to SDK databaseSize.
- worker-fifth: provider-owned underscore table read denied; excluded internal tables.
- worker-sixth: passed one test, ~16.265s test wall, default and JSON reporters.

Safe metric JSON extracted from actual stdout, without payload data:
- brews-game-resources-native-debug-final-metrics.json
- brews-game-resources-native-release-metrics.json
- brews-game-resources-worker-sixth-metrics.json
- brews-game-resources-metric-summary.json

Native game page-image bytes at lobby/started/called/History: 454656 / 679936 / 1548288 / 3821568. Started host-view/player-Sync/spectator-Sync JSON bytes: 186081 / 9679 / 508. Full-call samples: 229467 / 43950 / 34329. History JSON: 188318 bytes. Workerd game databaseSize at lobby/started/called/History: 479232 / 704512 / 704512 / 839680. Started host/player/spectator Sync: 186187 / 9688 / 508 bytes; one-call samples: 186212 / 9717 / 562 bytes. The native host-view and Worker host-Sync envelopes differ and are labeled separately.

Restrictions/open limits: SDK exhausted-cursor row-work, actual network TLS/WSS, full 70-socket load, full 1000-call Workerd workload, OS entropy native fixture, other maximum branches/worst-case workloads, owner forecast, traffic/concurrency, provider quota/headroom, production capacity and RUNTIME-FINAL-01 remain pending. No stale-privacy-disclosure or full production gate claim. Fixed retention/pending-work policy untouched.

Inspected owned source and existing published artifact hashes (no rebuild):
- backend/tests/game_resources.rs: 14245149500be2b494c3fab8c3780b0111bf2bffd63aa08d1f290c22a26e3491
- backend/worker-tests/game_resources.test.js: eff5d3f7d325711a2a90e74b06c1712e9ce2417addc1603ec0dec2f4b3f38a22
- backend/build/index.js: 1078cf91d1ef5c8ce713eb8ccdd53830fa96d1e16469117a323245f3e64583e6
- backend/build/index_bg.wasm: 3dcb5b64076e96f82cd5f3a929b04512278dbc42736026aa1cc29d8318805359
- backend/build/package.json: 9eb6bc2af50fe42e73ea9271984927cf9779d6b88c96b38f1326daf3192c8a6e
- backend/build/worker/shim.mjs: ac99177861405a6046923cbacabbd6adfa1853a56d0c679b9f5e09097c2f4728
- backend/src/worker_runtime/database.rs: ae3f2008cb047a0f6dc56ba91ec1f5e89c55531d9218cdf7be112cc63bf74ea3
- shared/domain/src/games.rs: 73abfcddb0a15b0e8f66a921a84d375b7cfe3ee89ae6be16648c9ea3ae68b454
