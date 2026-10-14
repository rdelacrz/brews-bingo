# Final independent read-only follow-up: CICD-01–05 and selector integration

**Disposition: no remaining blocking defect independently reproduced in the five CICD repairs or selector/snapshot integration at the captured source cut.** CICD-01–05 and the two integration/documentation findings are locally resolved within this bounded review. This is source/offline-test review of operational definitions, not approval of a provider deployment or production readiness. No unrelated domain blanket approval is given.

The approved normative policy is `docs/plans/protected-response-policy.md`. No policy decision is pending. Its authorization-time snapshot **may finish** after subsequent revocation; it does not require every such response to finish. Fresh authorization, original caps, invocation/projection binding, durability and receiving-side expiry/rollback fencing remain required. The known original receiving-cut assertion remains genuine superseded historical failure; a genuinely passing original observation is also allowed without inventing a failure.

## Cut and scope

Read the original independent review, five-repair report and snapshot/E2E independent report named in the request. The private source capture is `/root/.hermes/cache/scratch/brews-cicd-final-review-frozen-20261014`, captured **2026-10-14T04:22:59.665650+00:00**, HEAD `af4d4083a8431626e9efe80a0b0a8258af0dbb17`. This is an uncommitted working-source review copy, not a clean committed release. All 363 captured source/document files matched the live repository at final comparison; full hashes are in `inspected-hashes.json`. The parent-owned source freeze/full gates and graph were not edited or refreshed.

No repo modifications, authenticated/provider API reads, deployment, provisioning, secret delivery, publication or real remote staging execution occurred. No cargo, Worker build, Vitest, server or competing full gate ran. Writes were restricted to scratch copies, fixture repositories created by deployment unit tests, local evidence and this report. The initial Git ownership rejection was handled with per-command `safe.directory`, without changing Git configuration.

## Individual dispositions

### CICD-01 — resolved locally: selected-version annotations are authoritative

`deployment.py:selected_version` GETs the exact version ID and checks its **top-level** `annotations["workers/tag"]` and `["workers/message"]` against the exact commit/code digest. The installed, pinned Wrangler 4.147.0 formatter reads this same top-level response shape (`cli.js:163329–163336`); this was directly inspected. Settings annotations are neither required nor authoritative. Independent offline execution accepted settings with no annotations, rejected contradictory selected-version annotations even when settings carried correct annotations, and refused bad uploaded annotations before traffic promotion. Actual authenticated target response shape remains external verification.

### CICD-02 — resolved locally: uploaded resources checked before promotion

The upload UUID is parsed once, then `selected_version` checks exact response ID, all `resources.bindings`, migration tag and compatibility date/flags **before** `versions deploy VERSION@100%`. DO namespaces/class/own-script scope, APP_ORIGIN, secret binding kinds, version metadata binding, duplicate/unexpected bindings and missing resources fail closed. Independent negatives for wrong uploaded namespace, missing bindings, wrong annotations and wrong compatibility each executed only the mocked upload command and **zero versions-deploy commands**. Upload itself would be a provider write in an authorized real run.

Post-promotion readback checks the exact active version and its annotations/resources, separately checks live bindings, verifies anonymous HTTPS `/api/deployment` serves that version, and requires equal active deployment/version reads around these checks. The returned version must equal the uploaded version. Old active-state preflight is additional verification, not a replacement for upload verification.

### CICD-03 — resolved locally: actual remote runner and bound same-run staging evidence

`staging-test` invokes the original verified-source harness against the exact staging inventory origin with the frozen tested CLI. It requires real child exit zero, a new summary file, and exact stdout/file agreement. Child credentials are staging harness credentials only; provider/GitHub credentials are withheld from that child. Process output remains captured. The producer checks source/manifest/assets and original archive before and after the game and checks the same deployed/serving identity before execution and again after cleanup immediately before writing proof.

The successful summary must be staging, pass, remove its own account, preserve contractual History, contain three players, five/six accepted calls, bounded ACK count and exact sorted unique static check labels. Labels derive from the verified original source AST; no game or oracle is executed to generate proof. The actual scenario makes one random call, then skips a first-row manual call only when already called. Independently exercised all **12 inventory branches**: six choices for the skipped manual index (including none), each with/without optional spectator terminal notice. Six-call summaries contain **95/96** labels; five-call summaries contain **86/87**. The optional notice changes the count by one. These are synthetic summary-validation branches, not additional executed games.

Proof binds schema/run/commit/manifest/archive/full canonical inventory, exact staging deployment/version/Worker/code, canonical successful summary and its digest. The same-run immutable artifact name plus trusted staging job digest output is the provenance boundary. Production downloads that run's proof, validates against its environment inventory and trusted outputs, independently rereads tested staging **after production manual approval** with a separate staging **read-only** token, then uses only the separate production token in the later production mutation step and rechecks proof there. Missing read token, proof/source/digest/run/identity drift and environment inventory overrides reject. The read-before-mutation order/token placement were inspected in the workflow; unit tests and independent offline negatives verify the boundaries. This is not a signed arbitrary-artifact trust claim or proof that provisioned token scopes are correct.

### CICD-04 — resolved locally with explicitly retained external prerequisites

Production protection checks production environment GET, exact `main` branch GET/SHA and classic main protection GET. Required checks cover independent reviewers/self-review prevention/admin approval bypass, protected-branch restriction, protected main, enforced admins, no force pushes/deletions, required approving review/stale dismissal/last-push approval, no review bypass actors and strict `validate` status bound to a positive app ID. Independent offline admin/review bypass and absent-validation cases reject; the suite covers exact SHA and unresolved protection responses.

Classic protection must positively establish those requirements. Ruleset-only configuration, unsupported plan/permissions and unresolved/missing data fail closed. There is no fallback or bypass. The guide explicitly reserves confirmation that the app ID is actually GitHub Actions and that the check represents the complete validation job to separately reviewed target setup. Effective settings, supported target plan, credential permissions and external writers were not remotely inspected, and remain external readiness evidence; a positive arbitrary app ID alone is not claimed as proof of GitHub Actions identity.

### CICD-05 — resolved locally: x86 mold prerequisite declared and smoked before Cargo

Both native-building jobs (`validation.validate`, `release.freeze`) install Ubuntu `mold`, require x86_64, print its version and compile/run `cc -fuse-ld=mold` smoke code before installing native Cargo tools or running the full gate. This matches `.cargo/config.toml`. Staging/production consume the freeze artifact without native rebuilding. Shell syntax and prerequisite ordering passed review; no hosted Ubuntu execution or local package install occurred. ARM local passes do not certify x86 execution.

### Selector and snapshot integration — resolved locally

Deployment inventory now includes `AGENTS.md`, the Rust guide, operations CICD guide, `.github` supported policy/workflow files and planning policy documents. The new regression uses the runner's **honest** `source_hashes` in a clean committed fixture and requires exact selector agreement, rather than a report fabricated from deployment's own selection.

Independent comparison found **277 tracked deployment source paths**, all agreeing with the actual runner's hashes. The runner additionally sees **18 currently untracked source paths** (295 total), including the new workflows/tool/policy/tests. That difference is expected in this dirty local checkout: supplying both selectors the same all-committed inventory gives exact hash-map equality, without changing the real index. Independent actual `freeze` invocation against the dirty repository rejected before tool checks or reading any gate file and created no output. No fake local release-ready state was manufactured. The inspected actual selected inventory contains no dependency/build/secret input paths.

Deployment delegates to the runner's exact snapshot validator. It requires new exact 20-case passing acceptance and input hashes, and allows only the exact historical pass or exact confirmed historical failure shapes, retaining raw counters/exits. Independent execution accepted both shapes and rejected an invented contradictory failure exit. The existing actual runtime safe summary validated against the frozen source after **only relocating its absolute physical suite path** to the review copy; input hashes/results were not changed. It retains 20 new passes and historical 0 passed/1 failed/raw exit 1. This was validation of existing evidence, not a new runtime run.

README and operations guide now describe current authorization-time acceptance, preservation of the historical failure, the genuine-pass alternative and explicit omission of the 20-case suite from standalone Worker execution. `all` runs the separate contract lane exactly once; source execution and inventory exclusion agree. No pending documentation blocker from the earlier follow-up remains in these scoped files.

## Executed independent checks

- Private copy: `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s backend/tools/tests -p test_deployment.py -v`. **43 tests, 43 passed, zero failures/errors/skips**, unittest log `Ran 43 tests in 6.273s / OK`; launcher exit 0. Tests create disposable fixture Git repositories and mock provider/game operations. These are unit boundaries, not provider observations.
- `PYTHONDONTWRITEBYTECODE=1 python3 independent-reproductions.py`: **exit 0**; 17 separate boundary dispositions, all expected, plus the 12 successful-summary branches, full same-inventory selector comparison and existing runtime-summary validation. `independent-reproductions.json` explicitly labels synthetic scope and records exact outcomes.
- Python AST parsing for deployment/runner/deployment tests; YAML parsing of both workflows; `bash -n` for **19** workflow run blocks: all passed. No workflow execution/actionlint/provider check is implied.
- Actual pinned Wrangler **4.147.0** dry-run packaging of **previously copied existing review assets** with the current generated standalone config: **exit 0**, elapsed **2.336256066977512 seconds**, emitted `README.md`, `index.js`, `index_bg.wasm`, `shim.mjs`. Credentials were removed from the child environment; metrics disabled; no build or authenticated request. The first attempted copy used an absent earlier scratch asset path and stopped before packaging; the final successful check copied the repair handoff's preserved assets. Neither old assets nor this packaging result certify current runtime/source correctness. Exact command/input/output/config/log hashes are in `packaging-final/metadata.json`.
- Final comparison: **363 captured source/document inputs, zero differences**. No Graphify or parent gate inventory refresh occurred.

## Existing actual runtime/E2E evidence inspected, not independently rerun

The actual `brews-cicd-e2e-loopback-06/execution.json` records exit 0; its safe summary records **96 checks**, host plus three players and one spectator per the reviewed scenario, **six accepted calls, 44 ACKs, own_account_removed**, and the actual `cleanup_own_account_absent` check. Current bootstrap parsing requires the exact same-account CLI GET exit 1 and single exact HTTP-404 error object from bounded captured output. This fixes the prior stderr-parser issue; it is local HTTPS/WSS evidence, not remote staging proof.

The preserved loopback-05 summary remains **failed**, 95 checks, `cleanup_failed` and `failed_own_resources_may_remain`; it is not relabeled success. The existing runtime safe summary retains **20 current acceptance passes / unchanged older probe one failure** under the explicitly approved policy. Earlier independent runtime/E2E report conclusions were used only within their documented cut/scope; no new native/Worker/runtime/game executions are claimed here.

## Remaining external limits and ownership

All eight production readiness gates remain **OPEN**. Requiring separately reviewed readiness even before staging is the deliberate stricter promotion prerequisite; source tests do not establish those gates. Initial provisioning/bootstrap and obtaining first actual external evidence require separately authorized operations. No circular automated readiness claim is made.

Production remains blocked on actual target/provider/KDF/Unicode/clock/randomness/SDK behavior, owner forecasts/resources, quotas/headroom, History cost, copies/retention/restore, namespace/secret separation, deployment/security/load/operations evidence and effective protected-repository/environment/token settings. Actual uploaded-version API shapes and serving identity are still target checks. Parent owns current full gates, graph/hash reconciliation and final documents. This bounded follow-up identifies no new source blocker requiring repair; it grants no deployment, commit or push authorization.

## Exact reviewed hashes

| Source path | SHA-256 |
| --- | --- |
| `.cargo/config.toml` | `57fe85194e6c0cf21a90c162ebc365618307766cc40694d3f4df9623a940d46a` |
| `.github/workflows/release.yml` | `fede1442001eda8b4f5fba43007b2bf3a333933aa2841d2118cb86d75765d06f` |
| `.github/workflows/validation.yml` | `b0ea11a99d9d1477b8773c8b2c3407c8351e39c56bde9baa5a0c30cb8f3d61ed` |
| `AGENTS.md` | `f99c90f3d787ee32f62cd0496d927f3e678dbd6c1837c80c6307054ad4c9f248` |
| `backend/tools/deployment.py` | `e683c2e0cf8155284522a68346d2715a67f4d0ed5b8d7c37fee95dce09d4b3c0` |
| `backend/tools/tests/test_deployment.py` | `12a8d2b572439ab81620e0d48f277fd6ae2f41c64e002d683e0adb4e48918440` |
| `backend/tools/release_gate.py` | `601aa2c5d2f394ac3f67b212916e381b75c91284393dd8063d5ac77ffc8b7d97` |
| `backend/tools/tests/test_release_gate.py` | `a238c23f5bb106fbe5031466ff4ce0f8941d06353b6be3f0b8e7782418faa8c0` |
| `backend/tools/README.md` | `e25ae6fdb97f1f42030487a488f200fb7e81c7ffd4b80ec25ad51c1accedb4cb` |
| `backend/wrangler.toml` | `85b346ed5d39c296f6e96273e38315de76e2264ef770d856b39167bf2cea8365` |
| `backend/tests/full_game_e2e.py` | `32b97b898e87c64f9093199ce379052e4f40217e2021c0967af43c74f41bb3e5` |
| `backend/tests/e2e_support/scenario.py` | `49908981836d38a35063f01153b3d1e702e7a44675f223b14ed4f706d1fede24` |
| `backend/tests/e2e_support/bootstrap.py` | `97cc1cbd437046513b6e7e9fc70b3f8073b6430d66608ae01a4a1faee117e270` |
| `backend/worker-tests/protected_response_snapshot.test.js` | `5b0a339b10d4723177bcdf79163126b43fe6b2ebb98e5335b5dada7854703d9d` |
| `backend/worker-tests/deployment_identity.test.js` | `4172bbdd9e583dac72523cc403cc361f002ea8d7a975be876d4ec867ca73940d` |
| `backend/src/worker_runtime/deployment_identity.rs` | `438bdf21aec944ae45110ce646ffb880fdcea90113a31d86d4066f8412a65794` |
| `docs/operations/cicd.md` | `d48fc9e8a9195392351ddd8bc3609d51a7c163b0f4a12f6abae1ebb68eba4469` |
| `docs/plans/protected-response-policy.md` | `fa1aa9d4e1602f36d2eb841e1ad590fc1ccbd344ea5c02a1dcae8964d2f2db1d` |
| `docs/rust-best-practices.md` | `1d7d2fd0d5ea2f94d81124e16598557b0cd8e5a32aee951b8cc2a2c3adc4bab5` |
| `docs/reviews/evidence/command-lookup-final-return.config.mjs` | `fa946f6c1fcec6da2e54f1c207a1ebddd9d730489121adface64bb0ea99d3355` |
| `docs/reviews/evidence/command-lookup-final-return.test.js` | `28cec08b858f5ca8ab0335007d8a1afcf3cbd63612ace70c5f21261f86875a11` |

Pinned implementation evidence (read-only original installed dependency):

| Path | SHA-256 |
| --- | --- |
| `backend/node_modules/wrangler/package.json` | `75c3ccb272cabfbb1e1c555ddad7479d0b31a83a325b40859125941301d49865` |
| `backend/node_modules/wrangler/wrangler-dist/cli.js` | `f2ec95b41d7f7665a42b75fc9631100e95ea6c0dbabc09eb65ae7f772dbc2fed` |

Private review evidence:

| Scratch relative path | SHA-256 |
| --- | --- |
| `capture.json` | `000b6d13014a77ca9a22103feb6b16da734473c433c8d3660883bb6a25635cae` |
| `inspected-hashes.json` | `6ea152e7dcdc6e367ac4cccf8138125146bb270b17a8f1e4341579c0d1877770` |
| `deployment-unit.log` | `dbe39a6c72dfb0141dbe162012fe6cd8bc56cb4cd802dd79e9efca1aa13e9a25` |
| `independent-reproductions.py` | `cc8e33bd1f4f3f9901c6d3bbadf76788909e54c47a12ef11a9dd2a7e9eb5aa2d` |
| `independent-reproductions.json` | `9a92d499e2b6cf069592ee928ba21963b68713baf79a5e1f65b9e240f18a51f5` |
| `final-source-comparison.json` | `2e0d836e6121a11e23b45b9b41e736a8872e8c01be9065880a1330d758d9da9a` |
| `packaging-final/metadata.json` | `350f5c5d3015bac362c46bcb84f3fa3667cb3051bd1a9ca02ce914cc070283a7` |
| `packaging-final/packaging.log` | `0e10252d753588f2c1cb8af40230f8a4d11d9d5f813d6c2ba1aa7b897fd27bd4` |

Existing safe evidence inspected:

| Scratch relative path | SHA-256 |
| --- | --- |
| `brews-runtime-policy-final/contract-safe-summary.json` | `027806014e3671a97822a1ac9d518e48f2b1aae065c21f14957aa0aa25ea002a` |
| `brews-cicd-e2e-loopback-06/execution.json` | `0eb43481bdb3f3d36dd2162d52e6daca372602ea6136823f7a5c1ee59c18bd19` |
| `brews-cicd-e2e-loopback-06/e2e/summary.json` | `3ed15634f4983b17a0e0a8f3eba38cd248c61be877c050c6068b0b5785c25ab9` |
| `brews-cicd-e2e-loopback-05/e2e/summary.json` | `c0b2e49816f3cd87ba5ea789a3e04b06756f51ec01712252ac3a7c2ff89bed9f` |

Report completed: 2026-10-14T04:27:47.148885+00:00.
