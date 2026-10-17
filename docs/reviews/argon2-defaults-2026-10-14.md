# Argon2 default-policy revision — local verification, 2026-10-14

## Decision and scope

The user removed dedicated Argon2 work-factor/performance acceptance and directed use of minimal defaults. The [approved policy](../plans/argon2-default-policy.md) selects the **existing pinned RustCrypto `argon2` 0.5.3 defaults**, Argon2id v19 / 19 MiB / 2 iterations / parallelism 1, fresh 16-byte CSPRNG salts and 32-byte output. These are library defaults, not the weakest syntactically legal parameters. Runtime default/configuration and verifier logic are unchanged; no credential rewrite or migration is needed.

The dedicated `argon2_target` readiness requirement and local standalone hash/verify/recovery-hashing timings are removed. Exactly seven other external gate keys are still required. Tests cover acceptance without the removed gate, rejection of the obsolete eight-key shape, and rejection of each missing or failing remaining gate. The local evidence parser no longer accepts an `argon2` measurement object.

Password/recovery correctness, independent salts, PHC validation and embedded-parameter compatibility, request/rate limits, Unicode, clock/randomness, authority revalidation, original expiry and revocation remain. History SQL measurements are unchanged in scope. Historical review/evidence bytes remain intact; a removed requirement is not a passing performance result.

## Executed verification

- **135 Python tests passed**, including deployment readiness, release evidence, resource parser and full-game harness unit tests.
- Fresh complete release runner **exit 0**: **959 native tests in each profile**, **12 doctests**, strict native/Wasm Clippy, formatting and fresh Worker/CLI builds.
- **723 general Workerd cases in 39 suites**, plus **20 mandatory current snapshot-contract cases** in their separate suite, passed with complete inventories and no skips/failures.
- **373 actual local HTTPS/WSS checks** passed across the existing gameplay, operator, dedicated-read and audit harnesses. The remote-target full-game loopback was not separately rerun for this revision.
- Both native result summaries retain 36 History-query observations and four functional recovery cases; Workerd retains four functional recovery cases and clock/session-distinctness checks. Actual result objects contain no dedicated hashing-performance fields.
- **296 release source inputs and four runtime artifacts** remained unchanged during the final run and match the current bytes. The structural graph verifies **264 code-source hashes**; its extraction limitations remain, and the documentation graph was not refreshed.
- Current checklist and documented readiness schema both have exactly **seven** external gates. Relative documentation links and `git diff --check` pass.

[Machine-readable evidence](argon2-defaults-2026-10-14-evidence.json) binds the full reports, descriptors, exact source/artifact hashes and current graph. The first attempt failed at the Cargo-version prerequisite before test execution; its report is preserved. The explicitly pinned retry passed. A scratch recording script initially assumed list-shaped measurements; it was corrected to the actual dictionary schema before recording evidence, without changing tests or their results.

The unchanged older receiving-cut probe still has raw exit 1 under the superseded snapshot contract, separately preserved by the passing current-policy gate; it is not relabeled passed.

## Remaining boundaries

The seven remaining provider/runtime/forecast/retention/isolation/deployed-release gates and unrelated final slice reviews remain open. No provider resources, secrets, deployment, actual staging run, frontend, commit or push were performed for this revision. Existing published baseline is `2992962a20a512422ee5ec1796e9fdebc89f4a33`. These local checks do not certify production readiness.
