# Argon2 defaults and removal of dedicated performance acceptance

## Approved revision — 2026-10-14

The user directed: **“Argon measurements are for password hashing performance testing based on different parameters, right? Remove that, it is not important. Just use minimal argon defaults.”**

Use the existing default profile, matching the pinned RustCrypto `argon2` 0.5.3 library: **Argon2id v19, `m_cost=19456` KiB (19 MiB), `t_cost=2`, `p_cost=1`, 32-byte output**, with a fresh independent 16-byte CSPRNG salt. This applies to new account passwords and player recovery-answer enrollment. “Defaults” means this library profile, not Argon2's weakest syntactically legal parameters. No runtime cost reduction, credential rewrite or storage migration is needed: the backend already uses this profile when the optional cost overrides are absent.

The dedicated Argon2 parameter-tuning/performance measurement and certification requirement is **removed**, not marked passed. This revision supersedes DO-024/DO-051 and companion planning prose only insofar as they required target hashing memory/CPU/latency/concurrency benchmarks to select or approve work factors. Remove `argon2_target` from the deployment readiness manifest; exactly seven remaining external gates are required. Existing eight-key manifests must be updated explicitly rather than silently accepted.

Remove dedicated standalone hash/verify and recovery-hashing wall-time samples from local readiness tests. Preserve the History SQL measurements and the functional native/Workerd password, recovery, Unicode, clock and randomness regressions. Historical review reports and measurement artifacts retain their original bytes and scope; they are not evidence of a new passing benchmark.

## Safeguards and scope preserved

- Keep salts, PHC encoding, private verifier storage and fail-closed CSPRNG/hashing behavior.
- Keep existing PHC limits and validate algorithm/version/size/parameter bounds before expensive verification. Supported stored verifiers continue to use their embedded parameters; do not invalidate or downgrade existing credentials.
- Keep request/password limits, rate limits, commit-time authority revalidation, fixed expiry and credential/session revocation. Existing bounded optional cost overrides are not a new tuning requirement.
- Keep deployed functional/security/runtime checks, workload/capacity/quota/headroom acceptance, retention/restore, actual isolation/secrets/bootstrap, independent slice reviews and release protections. No separate Argon2 performance report is required, but a real failure of login/recovery on the target is still a functional failure.
- No frontend, provider provisioning, secrets, deployment, commit or push is authorized by this revision.

This policy removes a requirement; it does not claim deployed hashing feasibility or production readiness.
