# CICD-01–05 local repair handoff

Completed the five requested local repairs in the five allowed files. Capture: 2026-10-14T04:18:28.183823+00:00. Repository HEAD: `af4d4083a8431626e9efe80a0b0a8258af0dbb17`. This is an uncommitted working-source cut, not a clean frozen release or completed independent review. The parent still owns full gates, graph refresh, runtime/E2E changes, planning reconciliation and publication decisions.

No authenticated provider reads, real staging game, deployment, provisioning, secret delivery, commit or push occurred. No cargo/Worker build/full Vitest/server ran. No repository AGENTS, old probes/reports or dependencies were modified. The Ubuntu mold package install/check is defined for the two native-building CI jobs only; no local ARM install was attempted. No staging proof of actual deployment was manufactured.

## Repairs

- **CICD-01:** selected version GET top-level `version.annotations["workers/tag"]` and `["workers/message"]` is the commit/code authority, corroborated against installed pinned Wrangler’s real version formatter. Settings annotations are ignored. Supported settings without annotations passes; contradictory settings cannot rescue wrong selected-version annotations.
- **CICD-02:** after upload UUID extraction, `selected_version` validates exact response UUID, all `resources.bindings` (namespace/class/Worker/origin/secrets/version metadata), migration tag and compatibility date/flags plus annotations **before** `versions deploy VERSION@100% --yes`. Old deployment preflight remains separate. Post-promotion readback checks exact active uploaded version, its full bindings/annotations/runtime, separate live settings, serving endpoint UUID and two equal active-deployment reads. Mock wrong-upload tests prove only one Wrangler command (upload), zero versions-deploy calls; upload itself is a provider write during an authorized real run, not described as mutation-free.
- **CICD-03:** `staging-test` runs the actual frozen-source full-game harness with captured stdout/stderr, actual exit 0 and file/stdout agreement. Only a strictly passed staging summary with own-account removal, 3 players, 5/6 calls, >=5 ACKs, unique sorted exact checks/counts and retained-expiry History can produce proof. Check inventory derives from verified original frozen source AST label expressions, including all random-first-row branches and cleanup absence. It verifies source/manifest/assets/archive before and after execution and rereads exact deployment immediately before proof creation. Only redacted summary/identity hashes and allowlisted values enter proof. The CLI does not print boards, identities of games/accounts/players, cookies, tokens or raw child output.
- **CICD-03 transfer:** immutable same-run artifact `staging-proof-${{ github.run_id }}` plus staging job digest output. Proof binds schema/run/commit/manifest/archive/canonical entire inventory, exact tested staging deployment/version/Worker/code and canonical successful summary digest/summary. Production validates this proof with its current inventory (environment override drift rejects), original source and trusted outputs, then rereads tested staging **after** production approval and **before** production mutation. Only that read step receives the separately provisioned `STAGING_READ_ONLY_CLOUDFLARE_API_TOKEN`; later production deploy uses the separate production token and revalidates proof. Missing read token rejects. Production has no staging management key. Trusted workflow outputs + immutable same-run artifact are the provenance boundary, not a standalone signature. Readiness remains separately mandatory before staging and production; helpers cannot authorize it.
- **CICD-04:** production environment GET plus main branch GET plus classic main protection GET. Required exact requested main SHA/protected state, enforced administrators, no force pushes/deletions, approving reviews, stale dismissal, last-push approval/no review bypass and strict `validate` status check bound to positive app ID. Environment protected-branches-only alone no longer suffices. Guide specifies Environments/Administration/Contents read permissions and actual GitHub Actions app/check verification. Ruleset-only, unsupported-plan/permission or unresolved data fails closed; no fallback/bypass.
- **CICD-05:** validation and freeze jobs install Ubuntu OS `mold`, require x86_64, print linker version and compile/run a `cc -fuse-ld=mold` smoke binary before native cargo tool installation/test. Pinned tool/action versions stay unchanged. Staging/production do not rebuild.

## Runtime policy integration

Read the runtime owner’s completed `/root/.hermes/cache/scratch/brews-cicd-runtime-policy-report.md` and actual `contract-safe-summary.json`; reused its exact `validate_snapshot_contract` rather than inventing fields. `deployment.local_gate` now requires passed_authorization_time_snapshot / authorization_time_snapshot, mandatory exact 20-case physical inventory and input hashes, raw exit 0/no failures/skips. Historical nested receiving-cut result must preserve failed_acceptance_assertion/RUNTIME-FINAL-01/status200/passed0/failed1/skipped0/raw exit1/superseded true. It is never counted as passed or used to deny the new approved policy. Old status/no new cases/skips/fake history reject.

Actual existing safe runtime summary was passed through the new deployment snapshot validator, with no full-gate envelope fabrication or runtime rerun. Result accepted 20 passed / 0 failed / 0 skipped and retained historical 0 passed / 1 failed / raw exit 1. Integration metadata: `brews-cicd-repair-contract-integration.json`. These 20/1 are the runtime owner’s prior actual execution counts, not new test executions by this task. All eight actual external readiness gates remain OPEN until separately authorized provisioning/target evidence; no deployment/certification/source-fully-reviewed claim is made.

## Executed checks and limits

- Final `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s backend/tools/tests -p test_deployment.py -v`: **41 tests, 41 passed, 0 failures/errors/skips**, actual exit 0. Final log: `brews-cicd-repair-deployment-unit.log`. Includes nested negative fixture cases for selected-version wrong/missing resources, annotations/settings contradictions, wrong namespace/origin/secrets types, failed/missing/malformed/local/duplicate summary, source/archive drift during producer execution, wrong run/commit/digests/inventory/identity, post-approval staging drift, missing read credential, branch protection gaps and workflow read-before-deploy/token isolation/mold placement.
- Earlier intermediate 40-test run had 4 errors from concurrent parent E2E label changes (admin -> host, cleanup-absence addition). Inventory and independent fixtures were reconciled with actual source; final 41-test run passes. No remaining deployment unit failures. Original historical runtime probe remains failed by design under its superseded assertion, as above.
- Python AST parsing, credential-free deployment `--help`, PyYAML parsing of both workflows, `bash -n` for every workflow run block all passed. PyYAML was only a preinstalled local syntax-check tool, not added to project or required by unit suites. `actionlint` unavailable; no hosted workflow, live branch/environment protection or actual token permissions were tested.
- Actual pinned Wrangler **4.147.0** packaging command on copied prior review assets: **exit 0**, elapsed 1.36s. `deploy --config PRIVATE_CONFIG --no-bundle --keep-vars --tag EXACT_40_HEX_COMMIT --message brews-code-sha256:DIGEST --dry-run --outdir PRIVATE_OUT`. Credentials removed from child environment; metrics disabled; logs/private config in scratch. Outputs: README.md, index.js, index_bg.wasm, shim.mjs. No build or authenticated provider access. Copied assets are packaging inputs only, not proof of current runtime/source certification. Detailed command/input/output hashes: `brews-cicd-repair-packaging/metadata.json`; raw packaging log has no provider credentials.

## New command shapes

All use `--commit COMMIT --manifest-sha256 MANIFEST --artifact FROZEN_DIR`:

- `staging-test --archive RELEASE_ZIP --archive-sha256 ARCHIVE --run-id SAME_RUN_ID --expected-identity STAGING_IDENTITY --output NEW_PROOF_FILE` (actual harness + before/after identity/source checks; stdout only proof digest).
- `verify-staging --archive-sha256 ARCHIVE --run-id SAME_RUN_ID --proof SAME_RUN_PROOF_FILE --proof-sha256 STAGING_JOB_OUTPUT` (read-only staging identity recheck, dedicated scoped token).
- `deploy --environment production --archive-sha256 ARCHIVE --run-id SAME_RUN_ID --proof SAME_RUN_PROOF_FILE --proof-sha256 STAGING_JOB_OUTPUT --output NEW_PRODUCTION_IDENTITY` (separate readiness/protection + exact new version validation + production token).

Ordinary freeze remains clean exact committed source/full-gate dependent; no synthetic release or actual proof was produced locally. A detected cut/deployment change requires new staging execution. Independently reviewed external readiness/settings, resource measurements and true target/API response/token shape remain external gates.

## Owned file hashes

| Path | SHA-256 |
| --- | --- |
| `backend/tools/deployment.py` | `07a4e571510a47b872e43a386422c802c15bf0f2eb1ab4f71e7c90393cdc0305` |
| `backend/tools/tests/test_deployment.py` | `16497f8894cdb70e500f2f0bed5975e26637e372bb69ea358ab33def997c0424` |
| `.github/workflows/release.yml` | `fede1442001eda8b4f5fba43007b2bf3a333933aa2841d2118cb86d75765d06f` |
| `.github/workflows/validation.yml` | `b0ea11a99d9d1477b8773c8b2c3407c8351e39c56bde9baa5a0c30cb8f3d61ed` |
| `docs/operations/cicd.md` | `cd955e2667890d1e60e530b8795accf8c87f2185957c5ebdad63b58732bae994` |

## Read-only integration source cuts

| Path | SHA-256 |
| --- | --- |
| `backend/tools/release_gate.py` | `601aa2c5d2f394ac3f67b212916e381b75c91284393dd8063d5ac77ffc8b7d97` |
| `backend/tools/tests/test_release_gate.py` | `a238c23f5bb106fbe5031466ff4ce0f8941d06353b6be3f0b8e7782418faa8c0` |
| `backend/worker-tests/protected_response_snapshot.test.js` | `5b0a339b10d4723177bcdf79163126b43fe6b2ebb98e5335b5dada7854703d9d` |
| `docs/plans/protected-response-policy.md` | `fa1aa9d4e1602f36d2eb841e1ad590fc1ccbd344ea5c02a1dcae8964d2f2db1d` |
| `docs/reviews/evidence/command-lookup-final-return.test.js` | `28cec08b858f5ca8ab0335007d8a1afcf3cbd63612ace70c5f21261f86875a11` |
| `docs/reviews/evidence/command-lookup-final-return.config.mjs` | `fa946f6c1fcec6da2e54f1c207a1ebddd9d730489121adface64bb0ea99d3355` |
| `backend/tests/full_game_e2e.py` | `32b97b898e87c64f9093199ce379052e4f40217e2021c0967af43c74f41bb3e5` |
| `backend/tests/e2e_support/scenario.py` | `49908981836d38a35063f01153b3d1e702e7a44675f223b14ed4f706d1fede24` |
| `backend/tests/e2e_support/bootstrap.py` | `97cc1cbd437046513b6e7e9fc70b3f8073b6430d66608ae01a4a1faee117e270` |
| `backend/tests/e2e_support/safety.py` | `880fccb464f71fe4c8ad432f4005338f9738b6146e895c614f3b5a4911e57aee` |
| `backend/wrangler.toml` | `85b346ed5d39c296f6e96273e38315de76e2264ef770d856b39167bf2cea8365` |
| `backend/node_modules/wrangler/package.json` | `75c3ccb272cabfbb1e1c555ddad7479d0b31a83a325b40859125941301d49865` |
| `backend/node_modules/wrangler/wrangler-dist/cli.js` | `f2ec95b41d7f7665a42b75fc9631100e95ea6c0dbabc09eb65ae7f772dbc2fed` |

## Local evidence hashes

| Scratch path | SHA-256 |
| --- | --- |
| `brews-cicd-repair-deployment-unit.log` | `a64285d01cb60eefc34ff04495daa67fb4369ec3d755798f4b24b3729f3e3c74` |
| `brews-cicd-repair-contract-integration.json` | `116f82009e02630d8d040070da820733e48d4f2c80842197a01c812186724569` |
| `brews-runtime-policy-final/contract-safe-summary.json` | `027806014e3671a97822a1ac9d518e48f2b1aae065c21f14957aa0aa25ea002a` |
| `brews-cicd-repair-packaging/metadata.json` | `6295c08b617ce02adb61617e5ac449331b2bc065ea5b2788e42fe6542a10848a` |
| `brews-cicd-repair-packaging/packaging.log` | `0e10252d753588f2c1cb8af40230f8a4d11d9d5f813d6c2ba1aa7b897fd27bd4` |

## Copied packaging input and emitted output hashes

These artifacts were copied from the original immutable review capture, not newly built.

| Input | SHA-256 |
| --- | --- |
| `backend/build/index.js` | `40a1af52a746a9b13c6af4939ad39ce281f2ffe42dfce83c213cc9ea2360a281` |
| `backend/build/index_bg.wasm` | `0970f27cd948b73b05318c9f9eaef1b370cd5b23dde8f0b5d349c3cac646e9e2` |
| `backend/build/package.json` | `9eb6bc2af50fe42e73ea9271984927cf9779d6b88c96b38f1326daf3192c8a6e` |
| `backend/build/worker/shim.mjs` | `ac99177861405a6046923cbacabbd6adfa1853a56d0c679b9f5e09097c2f4728` |
| `target/debug/brews` | `74d0f7142cccb165df60dc1790d03865043151fc736e538ddab5c4484b7befb3` |
| `target/wasm32-unknown-unknown/release/brews_backend.wasm` | `7d72f185c25b57654a3e54bed3d96abd817096fbf62c32126d678ca3ff7b5a69` |

| Output | SHA-256 |
| --- | --- |
| `README.md` | `8e011575ee0d8efc4b3a49eb7d911e736a86ae744d89618cb45ab31d340ea147` |
| `index.js` | `40a1af52a746a9b13c6af4939ad39ce281f2ffe42dfce83c213cc9ea2360a281` |
| `index_bg.wasm` | `0970f27cd948b73b05318c9f9eaef1b370cd5b23dde8f0b5d349c3cac646e9e2` |
| `shim.mjs` | `ac99177861405a6046923cbacabbd6adfa1853a56d0c679b9f5e09097c2f4728` |
