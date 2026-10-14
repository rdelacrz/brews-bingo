# Independent read-only review: local CI/CD support

Decision: changes are required before treating this slice as an operational promotion implementation. The frozen deployment tests pass, but the provider readback and staging-to-production evidence chain have blocking gaps. This is a review of local implementation, not authorization or certification of provider state.

The authorization-time HTTP snapshot policy in `docs/plans/protected-response-policy.md` is explicitly approved and normative. I did not report the adopted policy as unapproved. The parent-owned release-gate/snapshot-suite/docs integration was in progress at capture time; its old contract status strings and documentation are not separate findings in this report.

## Blocking findings

### CICD-01 — P1: commit/code annotation readback uses a settings field unsupported by the provider interfaces

Frozen `backend/tools/deployment.py:295–306` GETs the exact version but then reads `settings["annotations"]` from `/scripts/{name}/settings`. The official SDK's script-and-version settings interface provides bindings and runtime settings, not version annotations. Pinned Wrangler's real version helper reads `version.annotations["workers/tag"]` and `version.annotations["workers/message"]` (installed `cli.js:163329–163336`). Wrangler uploads annotations in the multipart **upload metadata** (`cli.js:176385–176390`); the SDK describes that request as `VersionCreateParams.Metadata.annotations`. That request location must not be confused with a proved `VersionGetResponse.metadata.annotations` response location: both the pinned SDK and current SDK omit annotations from their GET response type, while Wrangler itself consumes the returned top-level version annotations.

Operational consequence: an ordinary conforming settings response cannot satisfy the check, so the command may fail after it has already promoted the Worker. A fail-closed exception is useful protection but is not a working release implementation. Moreover, the current code accepts contradictory annotations on the selected version when the synthetic settings annotations match the manifest.

Repair: derive and check annotations from the actual selected version response using a schema corroborated by pinned Wrangler or an authoritative fixture; never infer a response field solely from upload metadata. Replace the invented settings annotation fixture in `test_deployment.py:220–221`, and cover an absent settings annotations field and a mismatched selected-version tag/message. No authenticated response was obtained during this review, so target-specific runtime shape remains an external check.

Primary references: [official pinned SDK settings interface](https://raw.githubusercontent.com/cloudflare/cloudflare-typescript/v5.2.0/src/resources/workers/scripts/script-and-version-settings.ts), [official pinned SDK version interface](https://raw.githubusercontent.com/cloudflare/cloudflare-typescript/v5.2.0/src/resources/workers/scripts/versions.ts), and the frozen installed Wrangler implementation.

### CICD-02 — P1: namespace isolation is checked against live settings rather than the selected uploaded version, and only after promotion

`deployment.py:300–306` uses the selected version for migration/compatibility, but checks bindings only in live settings. The selected version exposes `resources.bindings` in the official version interface. `deploy:383–390` obtains an upload ID, immediately deploys it at 100%, then performs the identity check. No check of the new version's namespace IDs, APP_ORIGIN, secret binding kinds, serving identity binding or annotations runs before promotion.

An isolated negative fixture passed through the actual frozen `remote_identity` function with the selected version containing production namespaces and contradictory annotations, while settings contained the expected staging values. This is a synthetic boundary reproduction, not a provider observation. It establishes that selected-version isolation is not enforced by the code. Preflight of the old deployed version cannot prove what the new upload binds, particularly because `--keep-vars` and secrets inherit previous version state.

Repair: inspect the exact returned upload version before `versions deploy`, check its complete binding inventory and version annotations/runtime, then check the exact active version again after deployment. Retain any live-settings checks as separate checks where needed. Add coverage where only `version.resources.bindings` is wrong or absent. This does not require provisioning or changing historical migration policy.

Primary reference: [official version resources/bindings interface](https://raw.githubusercontent.com/cloudflare/cloudflare-typescript/v5.2.0/src/resources/workers/scripts/versions.ts).

### CICD-03 — P1: production does not consume a staging proof bound to the inventory or recheck staging after approval

Frozen `release.yml:100–114` writes staging identity and full-game summary only to that runner's temporary files. The staging job publishes neither outputs nor an immutable evidence artifact. Production (`:121–158`) loads inventory/readiness again from its own environment variables and accepts any separately valid readiness matching that newly loaded inventory. `needs: [freeze, staging]` establishes job success but does not establish that production's staging inventory, origin, namespaces or readiness digest equal the values used for the successful test. Environment-specific variable overrides can differ even without an intervening edit.

There is also no staging identity readback in production after the human approval wait. The final staging read occurs before that wait. Workflow concurrency controls other runs in this group; it cannot detect a dashboard/external writer changing staging during the wait. Production only checks its own target.

Repair: emit a redacted immutable staging proof binding run ID, exact commit, frozen manifest/archive digest, canonical inventory digest, exact staging deployment/version and successful full-game summary digest. Transfer it to production, validate it against the same inventory cut and independently recheck the tested staging identity before the production mutation. Treat mismatches/missing evidence as failure and rerun staging when the cut changes. Preserve the conservative eight-gate readiness requirement before staging.

### CICD-04 — P2: environment protection check does not establish the claimed protected-main trust boundary

`deployment.py:348–360` requires the production environment's `protected_branches: true` flag and checks reviewers/self-review/admin bypass. It never checks that `main` actually has branch protection. GitHub documents a material default: when no branch protection rules exist, “protected branches only” permits all branches. The existing fixture is accepted without any main protection proof. The YAML pins the event's main SHA, but an exact commit on unprotected main is not evidence of reviewed/trusted main.

The docs explicitly require protected main; readiness's isolation/operations evidence may independently attest it, but the automated protection checker currently cannot make the advertised live verification claim. Repair by checking the effective main protection/ruleset requirements with suitable read permission, or by making a precise independently reviewed main-trust prerequisite explicit in the external-settings evidence and narrowing the automated claim. Do not rely on the environment flag's implicit provider default. Environment-scoped credential placement and effective token resource scope still require actual external review.

Primary reference: [GitHub deployment branches and environment secrets](https://docs.github.com/en/actions/reference/workflows-and-actions/deployments-and-environments).

### CICD-05 — P2: Ubuntu workflows omit the repository's native linker prerequisite

Frozen `.cargo/config.toml:2–4` selects `cc` with `-fuse-ld=mold` for `x86_64-unknown-linux-gnu`. Both workflows run the native gate and install native Rust binaries on `ubuntu-24.04`, but install/check no mold linker. The current official Ubuntu 24.04 image inventory (image `20261004.327.1`) does not list mold, and its clang installer installs lld rather than mold. The local ARM host avoids these x86-specific flags, so passing local ARM checks do not establish that the hosted workflow can link its tested CLI.

Repair by supplying/verifying the required native linker in both jobs that build, or using a deliberately supported CI linker configuration. This is a missing declared prerequisite supported by source/image inventory, not an observed hosted-CI failure; no cargo build or hosted workflow was run during this review. All three release jobs already use the same Ubuntu architecture, which supports transferring the tested native CLI once the freeze build succeeds.

Primary references: [official Ubuntu image inventory](https://raw.githubusercontent.com/actions/runner-images/main/images/ubuntu/Ubuntu2404-Readme.md), [official clang installer](https://raw.githubusercontent.com/actions/runner-images/main/images/ubuntu/scripts/build/install-clang.sh).

## Checks and conclusions that are supported

- Actual isolated Python execution on frozen copies: deployment unit suite **27 passed**; full-game E2E unit suite **55 passed**. These are fixture/unit boundaries, not an actual game or provider deployment.
- Actual pinned Wrangler **4.147.0** `deploy --dry-run --no-bundle --keep-vars` packaging on copied existing artifacts and generated config, private scratch only: **exit 0**. It accepted the exact 40-character commit tag and emitted shim/JS/Wasm modules. No build, provider mutation or authenticated API call ran. Packaging output hash: `0e10252d753588f2c1cb8af40230f8a4d11d9d5f813d6c2ba1aa7b897fd27bd4`.
- Deployments GET's `result.deployments` object/list shape is supported by pinned Wrangler `fetchLatestDeployments` (`cli.js:163187–163194`) and the official deployments SDK. Do not change it to a bare result array. [Official deployments interface](https://raw.githubusercontent.com/cloudflare/cloudflare-typescript/v5.2.0/src/resources/workers/scripts/deployments.ts).
- `resources.script_runtime.migration_tag` exists in the official version schema. The current request tag is 40 ASCII bytes; current official upload schema permits version tags up to 100 bytes. There is no evidence for a tag-length blocker. [Current official version interface](https://raw.githubusercontent.com/cloudflare/cloudflare-typescript/main/src/resources/workers/scripts/versions.ts).
- Pinned Wrangler explicitly supports `--keep-vars`/`--no-bundle`; versions upload preserves secrets and rejects pending DO migrations (`cli.js:176314–176325`). The historical migrations can remain configured when the remote service has already applied `games`; first provisioning remains separately authorized. No production initial-provision workaround is required.
- Frozen allowlist/hash/archive protections reject traversal, duplicate entries, links and unexpected members; source freeze requires a clean exact commit. Same-run immutable artifact names and digest job outputs are a reasonable trusted-workflow provenance boundary. They are not independently signed arbitrary artifacts.
- Release is manual, main/event-SHA constrained, serialized without cancellation, and production depends on staging. Validation has read-only permissions and no secrets. Effective environment rules, main trust, token scope and external writers were not inspected remotely.
- The minimal anonymous deployment endpoint reveals a validated nonnil canonical UUID only. No protected payload or runtime setting is returned. I inspected code but did not execute Worker/runtime tests.

## Limits and follow-up ownership

No repository edits, Graphify refresh, authenticated reads, provider resources, deployments, staging games, secret actions, commits or pushes occurred. No cargo, Worker build, full Vitest or server execution occurred. Dry-run packaging used existing copied artifacts; their current runtime correctness and module execution were not established. The parent owns integrated verification and any repairs, including policy/snapshot acceptance wiring and docs reconciliation. Production gate measurements, actual namespaces/secret separation, hosting routes, external autodeploy settings, backup/restore and deployed load/security behavior remain unverified.

Frozen snapshot and private local evidence are in `/root/.hermes/cache/scratch/brews-cicd-review-frozen-20261014/`. Findings refer to that capture, not concurrent later edits. Python logs contain only this isolated unit execution. `negative-boundary-reproduction.json` explicitly labels its synthetic provider fixtures.

## Frozen inspected file hashes (SHA-256)

Initial capture time: `2026-10-14T03:56:21.867810+00:00`. E2E support files were captured separately before their isolated unit run.

| Inspected path | SHA-256 |
| --- | --- |
| `.cargo/config.toml` | `57fe85194e6c0cf21a90c162ebc365618307766cc40694d3f4df9623a940d46a` |
| `.github/workflows/release.yml` | `5c96afcbed6e52a80eab1b71350d339dd1b11d1122b899620052d7821a33e3ee` |
| `.github/workflows/validation.yml` | `1705a96a1ecd0cd3e773d8dcf8070c2a575243befbbabeead3cf47b8aedc7d81` |
| `AGENTS.md` | `f99c90f3d787ee32f62cd0496d927f3e678dbd6c1837c80c6307054ad4c9f248` |
| `backend/node_modules/wrangler/package.json` | `75c3ccb272cabfbb1e1c555ddad7479d0b31a83a325b40859125941301d49865` |
| `backend/node_modules/wrangler/wrangler-dist/cli.js` | `f2ec95b41d7f7665a42b75fc9631100e95ea6c0dbabc09eb65ae7f772dbc2fed` |
| `backend/package-lock.json` | `490ddc232da8da02bf3080a1269f9733ef8a6ad923b5c3344f79ce3e8aacd10e` |
| `backend/package.json` | `b60ec01f45ab37ed6d3bd786ccb6b2186349bd62422aad7c8916e7270a8b331a` |
| `backend/src/worker_runtime/deployment_identity.rs` | `438bdf21aec944ae45110ce646ffb880fdcea90113a31d86d4066f8412a65794` |
| `backend/tests/e2e_support/__init__.py` | `7c85136a0f752e0487a121ddf6f4cc84cb556a663197b15fd2e08c5490850c44` |
| `backend/tests/e2e_support/bootstrap.py` | `f555d3438f9bdcfdcb25eb202651bc3b7b96b78c9031be8e40a2782b4c94c11e` |
| `backend/tests/e2e_support/http.py` | `ad5ff008a7feccb20f19dc6b6c124a5c0eb79a5e35e85133be49a1b8564175f8` |
| `backend/tests/e2e_support/oracle.py` | `b7a6fceea7c5c52773959593635a43b9e1ade84da9332af7c064cc8c4b4097b7` |
| `backend/tests/e2e_support/safety.py` | `880fccb464f71fe4c8ad432f4005338f9738b6146e895c614f3b5a4911e57aee` |
| `backend/tests/e2e_support/scenario.py` | `29a44104401c8b068970c5133d167e2b312d990d1bd1211375c0a47b73b8cef6` |
| `backend/tests/e2e_support/stream.py` | `1c4be1950dedf3101bffd533fd1f16ce625b128718c1cc4f770ee6831976ec26` |
| `backend/tests/full_game_e2e.py` | `7810010146d7179cad441e70b0e281fc04f7ab61ababfacf913d8b58bda934f6` |
| `backend/tools/deployment.py` | `dc1dc8f089f25aff6f8a58f9e272152c25c8e59b6262cd0a96dbd98436a0578a` |
| `backend/tools/release_gate.py` | `6da802e63bdc15fef234d8ad38409087041315d44d3f5d13206540c23289d162` |
| `backend/tools/tests/test_deployment.py` | `8fbfc7dd22559aa2d2077985b6f40f126cafa2dbba216e45eb93d045afb58a0b` |
| `backend/tools/tests/test_full_game_e2e.py` | `bcc4a81c45666ea3eb7afe2f1d9afdb2a65417eda85641e6845042b15b9fd1e3` |
| `backend/tools/tests/test_release_gate.py` | `46d7b523e14bdd155d74f294d7406ef10acaf8fe1a880d7e497ff452feb15ed3` |
| `backend/wrangler.toml` | `85b346ed5d39c296f6e96273e38315de76e2264ef770d856b39167bf2cea8365` |
| `docs/operations/cicd.md` | `c7a4cee6b265fcdd112754d343992d89d654b134dd6be93bd755d4ee04d7544f` |
| `docs/plans/protected-response-policy.md` | `fa1aa9d4e1602f36d2eb841e1ad590fc1ccbd344ea5c02a1dcae8964d2f2db1d` |
| `rust-toolchain.toml` | `f85f74c3f9bf13cfb68855f5e125bbd206d6e07440f7d3145a5872ad14e2bdb2` |

Public primary evidence retrieved without authentication and retained locally:

| Scratch evidence path | SHA-256 |
| --- | --- |
| `public-sdk/deployments.ts` | `eea411fece28547409e574bbda3124128a255e8940c515f22ba7991e7acc25c5` |
| `public-sdk/runner-install.txt` | `17df3ade8507d0fa8c419588ca6c1d453155400110266eacca4a141ec7868d86` |
| `public-sdk/script-and-version-settings.ts` | `5272b9d2927a3a07c513ef9a84f5999eb28a5e93a87ba63d420582676a90744f` |
| `public-sdk/settings.ts` | `15f23394a6f077ca073a5784c9120814099bab4278292812244af6719ca6262e` |
| `public-sdk/ubuntu2404.txt` | `bb4d9304fcfa12f5fc029af848705b47be323cb3ca980913033ca097a48361b3` |
| `public-sdk/versions-current.txt` | `d27c7c50deb8eeb0a3be0026141e30f8af5f29fef43b5929cd04324043448a64` |
| `public-sdk/versions.ts` | `9aceeadfe840f469a06bbd0e2be319e4fa3e07d6cfc5f73b94281ecf69d55ce7` |

Local verification evidence:

| Scratch evidence path | SHA-256 |
| --- | --- |
| `deployment-unit.log` | `783d96ccea86dbf7b952330449daa6fa55c8f6a6850447cdf8a62de6fd856bb2` |
| `e2e-unit.log` | `5d5cf2b06f6aa4c94b395d3d0c6509b00054d8f8738c36dee52aa16436c53b72` |
| `negative-boundary-reproduction.json` | `23c2e288d2917ce53d5976d35db760d2d2690e70a8755b4be1934d057a3315b1` |
| `packaging.log` | `0e10252d753588f2c1cb8af40230f8a4d11d9d5f813d6c2ba1aa7b897fd27bd4` |
| `packaging-config.json` | `18d2ff69a97e8fa8d917e2187e82714a345357afca73531af7be31ad366b1cf4` |

Existing assets copied for the isolated packaging check (not newly built or runtime-certified):

| Frozen packaging input | SHA-256 |
| --- | --- |
| `backend/build/index.js` | `40a1af52a746a9b13c6af4939ad39ce281f2ffe42dfce83c213cc9ea2360a281` |
| `backend/build/index_bg.wasm` | `0970f27cd948b73b05318c9f9eaef1b370cd5b23dde8f0b5d349c3cac646e9e2` |
| `backend/build/package.json` | `9eb6bc2af50fe42e73ea9271984927cf9779d6b88c96b38f1326daf3192c8a6e` |
| `backend/build/worker/shim.mjs` | `ac99177861405a6046923cbacabbd6adfa1853a56d0c679b9f5e09097c2f4728` |
| `target/debug/brews` | `74d0f7142cccb165df60dc1790d03865043151fc736e538ddab5c4484b7befb3` |
| `target/wasm32-unknown-unknown/release/brews_backend.wasm` | `7d72f185c25b57654a3e54bed3d96abd817096fbf62c32126d678ca3ff7b5a69` |
