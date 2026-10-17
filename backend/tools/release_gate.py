"""Provider-neutral local checks; this tool cannot authorize or certify a deployment."""
from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import time
import tomllib
import threading

ROOT = Path(__file__).resolve().parents[2]
BACKEND = ROOT / "backend"
LANES = ("native", "worker", "tls", "contract")
SNAPSHOT_POLICY = "authorization_time_snapshot"
SNAPSHOT_STATUS = "passed_authorization_time_snapshot"
POLICY_DOCUMENT = "docs/plans/protected-response-policy.md"
ACCEPTANCE_SUITE = "backend/worker-tests/protected_response_snapshot.test.js"
HISTORICAL_INPUTS = {
    "docs/reviews/evidence/command-lookup-final-return.test.js": "28cec08b858f5ca8ab0335007d8a1afcf3cbd63612ace70c5f21261f86875a11",
    "docs/reviews/evidence/command-lookup-final-return.config.mjs": "fa946f6c1fcec6da2e54f1c207a1ebddd9d730489121adface64bb0ea99d3355",
}
ACCEPTANCE_CASES = frozenset(
    [f"RUNTIME-FINAL-01 G4 recovery {timing} final cut" for timing in ("before", "after")]
    + [f"RUNTIME-FINAL-01 dedicated {label} recovery {timing} final cut"
       for timing in ("before", "after") for label in ("membership", "board", "privacy")]
    + [f"RUNTIME-FINAL-01 {mutation} {timing} final cut content"
       for timing in ("before", "after") for mutation in ("delete-answer", "rename")]
    + [f"RUNTIME-FINAL-01 SQL authoritative account epoch invalidation {timing} final cut"
       for timing in ("before", "after")]
    + [f"RUNTIME-FINAL-01 cross-session final Exit {timing} final cut" for timing in ("before", "after")]
    + ["RUNTIME-FINAL-01 original cap expiry after genuine final reply" + suffix
       for suffix in ("", " with durable denial rollback")]
    + [f"RUNTIME-FINAL-01 stale {path} challenge substitution is 503 and fresh revoked request is 401"
       for path in ("/read", "/read-release")]
)
PRODUCTION_BLOCKERS = (
    "Slice2 final independent review",
    "Slice6/7 final independent review",
    "Actual deployed Unicode/clock/CSPRNG/SDK validation",
    "Owner-provided peak event/History forecast and maximum-legal-game per-owner load",
    "Current provider quotas and measured two-times headroom",
    "Provider copy/backup/log retention and expiry-preserving restore validation",
    "Authorized isolated environments, provisioning, secrets and deployed release/security/load gates",
)


class GateError(Exception):
    """A local check failed or its evidence was incomplete."""


CHILD_LOCK = threading.Lock()
CHILDREN: set[subprocess.Popen] = set()
CANCELLED = threading.Event()


def kill_group(process: subprocess.Popen) -> None:
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass


def cancel_children() -> None:
    with CHILD_LOCK:
        CANCELLED.set()
        for process in CHILDREN:
            kill_group(process)


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def save(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    path.chmod(0o600)


def source_hashes(root: Path) -> dict[str, str]:
    names = subprocess.check_output(
        ["git", "-c", f"safe.directory={root}", "-C", str(root),
         "ls-files", "--cached", "--others", "--exclude-standard"], text=True,
    ).splitlines()
    selected = {p for p in names if (
        p in {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo/config.toml",
              "AGENTS.md", "docs/rust-best-practices.md", "docs/operations/cicd.md", POLICY_DOCUMENT,
              "docs/reviews/evidence/command-lookup-final-return.config.mjs",
              "docs/reviews/evidence/command-lookup-final-return.test.js"}
        or p.startswith("docs/plans/") and Path(p).suffix == ".md"
        or p.startswith(".github/") and Path(p).suffix in {".yml", ".yaml", ".json", ".toml", ".md", ".py", ".js", ".mjs"}
        or p.split("/")[0] in {"backend", "cli", "shared"}
        and Path(p).suffix in {".rs", ".py", ".js", ".mjs", ".toml", ".json", ".sql"}
    )}
    return {p: digest((root / p).read_bytes()) for p in sorted(selected) if (root / p).is_file()}


def execute(command: list[str], cwd: Path, target: Path, deadline: int) -> tuple[int, str]:
    """Keep raw child output in memory, never in an automatically persisted log."""
    started = time.monotonic()
    with CHILD_LOCK:
        if CANCELLED.is_set():
            raise GateError("Local execution was cancelled")
        process = subprocess.Popen(command, cwd=cwd, stdout=subprocess.PIPE,
                                   stderr=subprocess.PIPE, start_new_session=True)
        CHILDREN.add(process)
    timed_out = False
    try:
        stdout, stderr = process.communicate(timeout=deadline)
    except subprocess.TimeoutExpired:
        timed_out = True
        kill_group(process)
        stdout, stderr = process.communicate()
    except BaseException:
        kill_group(process)
        process.wait()
        raise
    finally:
        with CHILD_LOCK:
            CHILDREN.discard(process)
    # Test failure messages can contain credentials. Persist hashes and exit metadata,
    # not raw stdout/stderr, environment values or assertion payloads.
    save(target, {"command": command, "exit_code": process.returncode,
                  "timed_out": timed_out, "elapsed_wall_seconds": time.monotonic() - started,
                  "stdout_sha256": digest(stdout), "stderr_sha256": digest(stderr),
                  "raw_output_persisted": False})
    return (124 if timed_out else process.returncode), stdout.decode("utf-8", errors="replace") + "\n" + stderr.decode("utf-8", errors="replace")


def native_inventory(text: str) -> list[str]:
    summaries = re.findall(r"Summary \[.*?\] (\d+) tests? run: (\d+) passed(?: \([^)]*\))?, (\d+) skipped", text)
    if len(summaries) != 1:
        raise GateError("Missing or ambiguous nextest summary")
    ran, passed, skipped = map(int, summaries[0])
    identities = re.findall(r"^\s*PASS \[[^\]]+\]\s+(?:\(\s*\d+/\d+\)\s+)?(.+?)\s*$", text, re.M)
    if ran < 1 or ran != passed or skipped or len(identities) != passed or len(set(identities)) != passed:
        raise GateError("Incomplete native execution")
    return sorted(digest(name.encode()) for name in identities)


def parse_native(text: str) -> int:
    return len(native_inventory(text))


def parse_doctests(text: str) -> int:
    summaries = re.findall(r"test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out", text)
    targets = re.findall(r"Doc-tests (\S+)", text)
    if not summaries or len(summaries) != len(targets) or len(targets) != len(set(targets)):
        raise GateError("Incomplete doctest target inventory")
    total = 0
    for status, passed, *other in summaries:
        if status != "ok" or any(int(count) for count in other):
            raise GateError("Doctests failed, skipped, measured or filtered")
        total += int(passed)
    if not total:
        raise GateError("No doctest executed")
    return total


def parse_worker(text: str) -> dict:
    decoder = json.JSONDecoder()
    candidates = []
    # Workerd may write non-JSON diagnostic lines before the JSON reporter.
    for match in re.finditer(r"(?m)^\s*\{", text):
        try:
            value, _ = decoder.raw_decode(text[match.start():].lstrip())
        except json.JSONDecodeError:
            continue
        if isinstance(value, dict) and "testResults" in value:
            candidates.append(value)
    if len(candidates) != 1:
        raise GateError("Missing or ambiguous Worker reporter output")
    return candidates[0]


def reconcile_workers(reports: list[dict], expected: set[str]) -> dict:
    suites, seen_cases = {}, set()
    count = 0
    for report in reports:
        cases = [case for suite in report["testResults"] for case in suite["assertionResults"]]
        if (not report["success"] or report["numFailedTests"] or report["numPendingTests"]
                or report.get("numRuntimeErrorTestSuites", 0) or report.get("unhandledErrors", [])
                or not cases or len(cases) != report["numTotalTests"]
                or len(cases) != report["numPassedTests"]):
            raise GateError("Worker failures, skips, errors or inconsistent totals")
        for suite in report["testResults"]:
            name = str(Path(suite["name"]).resolve())
            if name in suites or suite["status"] != "passed" or not suite["assertionResults"]:
                raise GateError("Duplicate, empty or unsuccessful Worker suite")
            safe_cases = []
            for case in suite["assertionResults"]:
                key = (name, case["fullName"])
                if key in seen_cases or case["status"] != "passed":
                    raise GateError("Duplicate or unsuccessful Worker case")
                seen_cases.add(key)
                safe_cases.append({"case_id_sha256": digest(case["fullName"].encode()), "status": case["status"]})
            suites[name] = safe_cases
        count += len(cases)
    if set(suites) != expected:
        raise GateError("Worker file inventory does not match all expected test files")
    return {"passed": count, "failed": 0, "skipped": 0,
            "physical_suites": len(suites), "case_inventory": suites}


def parse_tls(text: str, key: str) -> dict:
    reports = []
    for line in text.splitlines():
        try:
            value = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(value, dict) and key in value:
            reports.append(value)
    if len(reports) != 1:
        raise GateError("Missing or ambiguous HTTPS/WSS report")
    value = reports[0]
    checks = value["checks"]
    if (value[key] != "passed" or value["deployment"] != "none"
            or not checks or len(checks) != len(set(checks)) or len(checks) != value["check_count"]):
        raise GateError("Incomplete or nonlocal HTTPS/WSS execution")
    return {"check_count": len(checks), "check_ids_sha256": [digest(name.encode()) for name in checks], "deployment": "none"}


def measurement_lines(text: str, target: str) -> list[dict]:
    summaries = []
    allowed = {"marker", "target", "profile", "arch", "os", "sqlite_version", "timing",
               "history", "recovery", "unicode", "pending", "clock", "csprng"}
    forbidden = {"password", "cookie", "session", "token", "verifier", "salt", "answer", "key", "secret", "credentials", "digest"}
    def check(value: object) -> None:
        if isinstance(value, dict):
            if any(str(key).lower() in forbidden for key in value):
                raise GateError("Measurement contains a forbidden credential field")
            for item in value.values():
                check(item)
        elif isinstance(value, list):
            for item in value:
                check(item)
        elif isinstance(value, str) and any(marker in value for marker in ("$argon2", "__Secure-", "Bearer ")):
            raise GateError("Measurement contains credential-like content")
    for match in re.finditer(r"LOCAL-SLICE9-MEASUREMENTS (\{[^\n]*\})", text):
        value = json.loads(match[1])
        if (not isinstance(value, dict) or set(value) - allowed
                or value.get("marker") != "LOCAL-SLICE9-MEASUREMENTS" or value.get("target") != target):
            raise GateError("Malformed or wrong-target local measurement")
        check(value)
        summaries.append(value)
    return summaries


def parse_contract(report: dict, exit_code: int) -> dict:
    expected_file = ROOT / "docs/reviews/evidence/command-lookup-final-return.test.js"
    suites = report["testResults"]
    if (exit_code not in (0, 1) or len(suites) != 1
            or Path(suites[0]["name"]).resolve() != expected_file
            or suites[0]["status"] != ("passed" if exit_code == 0 else "failed")):
        raise GateError("Abnormal or wrong-suite contract execution")
    expected_counts = {"numTotalTestSuites": 1, "numPassedTestSuites": int(exit_code == 0),
                       "numFailedTestSuites": int(exit_code == 1), "numPendingTestSuites": 0,
                       "numTotalTests": 1, "numPassedTests": int(exit_code == 0),
                       "numFailedTests": int(exit_code == 1), "numPendingTests": 0}
    if any(type(report.get(key)) is not int or report[key] != value for key, value in expected_counts.items()):
        raise GateError("Contract reporter counters are incomplete or inconsistent")
    if any(type(report.get(key, 0)) is not int or report.get(key, 0) != 0
           for key in ("numRuntimeErrorTestSuites", "numTodoTests")):
        raise GateError("Contract reporter contains runtime errors or todo tests")
    cases = [c for suite in report["testResults"] for c in suite["assertionResults"]]
    expected = "G4 shared RUNTIME-FINAL-01: actual recovery during final return must deny the old session"
    if (len(cases) != 1 or cases[0]["fullName"] != expected
            or report["numTotalTests"] != 1 or report["numPendingTests"]
            or report.get("numRuntimeErrorTestSuites", 0) or report.get("unhandledErrors", [])):
        raise GateError("Protected-response acceptance probe did not execute exactly once")
    status = cases[0]["status"]
    if status == "passed" and exit_code == 0 and report["success"] and report["numPassedTests"] == 1 and report["numFailedTests"] == 0:
        return {"status": "passed_observation_not_contract_approval", "passed": 1, "failed": 0, "skipped": 0}
    if status == "failed" and exit_code != 0 and not report["success"] and report["numFailedTests"] == 1 and report["numPassedTests"] == 0:
        messages = cases[0].get("failureMessages", [])
        if (len(messages) != 1 or "expected [ 401, 503 ] to include 200" not in messages[0]
                or "command-lookup-final-return.test.js:85:" not in messages[0]):
            raise GateError("Acceptance probe failed without confirming the last-hop assertion")
        return {"status": "failed_acceptance_assertion", "finding": "RUNTIME-FINAL-01",
                "observed_returned_status": 200, "passed": 0, "failed": 1, "skipped": 0}
    raise GateError("Inconsistent protected-response result")


def contract_inputs(root: Path = ROOT) -> dict[str, str]:
    try:
        policy = (root / POLICY_DOCUMENT).read_bytes()
        acceptance = (root / ACCEPTANCE_SUITE).read_bytes()
        historical = {name: digest((root / name).read_bytes()) for name in HISTORICAL_INPUTS}
    except OSError as error:
        raise GateError("Missing approved policy or contract release input") from error
    if (b"RUNTIME-FINAL-01" not in policy or b"approved authorization-time snapshots" not in policy
            or b"It may finish because it was authorized before revocation. Revocation blocks subsequent requests." not in policy
            or not acceptance or historical != HISTORICAL_INPUTS):
        raise GateError("Unapproved policy or changed historical probe input")
    return {POLICY_DOCUMENT: digest(policy), ACCEPTANCE_SUITE: digest(acceptance), **historical}


def parse_snapshot_acceptance(report: dict, exit_code: int) -> dict:
    suites = report["testResults"]
    if (type(exit_code) is not int or exit_code not in (0, 1) or len(suites) != 1
            or Path(suites[0]["name"]).resolve() != ROOT / ACCEPTANCE_SUITE):
        raise GateError("Abnormal or wrong-suite snapshot acceptance execution")
    cases = suites[0]["assertionResults"]
    names = [case["fullName"] for case in cases]
    if len(names) != len(ACCEPTANCE_CASES) or len(set(names)) != len(names) or set(names) != ACCEPTANCE_CASES:
        raise GateError("Snapshot acceptance case inventory is incomplete or duplicated")
    if any(case["status"] not in ("passed", "failed") for case in cases):
        raise GateError("Snapshot acceptance skipped or unfinished case")
    passed = sum(case["status"] == "passed" for case in cases)
    failed = len(cases) - passed
    expected_counts = {"numTotalTestSuites": 1, "numPassedTestSuites": int(failed == 0),
                       "numFailedTestSuites": int(failed != 0), "numPendingTestSuites": 0,
                       "numTotalTests": len(ACCEPTANCE_CASES), "numPassedTests": passed,
                       "numFailedTests": failed, "numPendingTests": 0}
    if (any(type(report.get(key)) is not int or report[key] != value for key, value in expected_counts.items())
            or any(type(report.get(key, 0)) is not int or report.get(key, 0) != 0
                   for key in ("numRuntimeErrorTestSuites", "numTodoTests"))
            or report.get("unhandledErrors", []) or type(report.get("success")) is not bool
            or report["success"] != (failed == 0) or exit_code != int(failed != 0)
            or suites[0]["status"] != ("failed" if failed else "passed")):
        raise GateError("Snapshot acceptance reporter is inconsistent or contains runtime errors")
    return {"status": SNAPSHOT_STATUS if not failed else "failed_authorization_time_snapshot",
            "policy": SNAPSHOT_POLICY, "passed": passed, "failed": failed, "skipped": 0,
            "physical_suites": 1, "raw_exit_code": exit_code,
            "execution_scope": "contract_only_excluded_from_worker_lane",
            "case_inventory": {str(ROOT / ACCEPTANCE_SUITE): [
                {"case_id_sha256": digest(case["fullName"].encode()), "status": case["status"]}
                for case in cases]}}


def historical_observation(report: dict, exit_code: int) -> dict:
    # Preserve the original physical assertion's parser/result without relabeling it.
    if type(exit_code) is not int:
        raise GateError("Historical exit code is not a physical process result")
    physical = parse_contract(report, exit_code)
    if exit_code == 1:
        message = report["testResults"][0]["assertionResults"][0]["failureMessages"][0]
        lines = message.strip().splitlines()
        if (not lines or lines[0].strip() != "AssertionError: expected [ 401, 503 ] to include 200"
                or not any(re.search(r"command-lookup-final-return\.test\.js:85:24\)?$", line.strip()) for line in lines[1:])
                or any(line.strip() and not line.strip().startswith("at ") for line in lines[1:])):
            raise GateError("Historical failure is not exactly the superseded assertion")
    return {**physical, "raw_exit_code": exit_code, "superseded": True}


def validate_snapshot_contract(result: dict) -> None:
    """Reject stale tags or a summary that bypasses required physical inventories."""
    inputs = contract_inputs()
    if (result.get("policy") != SNAPSHOT_POLICY or result.get("release_input_sha256") != inputs
            or result.get("status") not in (SNAPSHOT_STATUS, "failed_authorization_time_snapshot")
            or result.get("execution_scope") != "contract_only_excluded_from_worker_lane"
            or type(result.get("physical_suites")) is not int or result["physical_suites"] != 1
            or type(result.get("skipped")) is not int or result["skipped"] != 0):
        raise GateError("Missing or stale approved snapshot contract evidence")
    cases = result.get("case_inventory", {}).get(str(ROOT / ACCEPTANCE_SUITE), [])
    names = {digest(name.encode()) for name in ACCEPTANCE_CASES}
    if (set(result.get("case_inventory", {})) != {str(ROOT / ACCEPTANCE_SUITE)}
            or len(cases) != len(names) or {c.get("case_id_sha256") for c in cases} != names
            or any(c.get("status") not in ("passed", "failed") for c in cases)):
        raise GateError("Missing snapshot acceptance inventory")
    passed = sum(c["status"] == "passed" for c in cases)
    failed = len(cases) - passed
    if (any(type(result.get(k)) is not int or result[k] != v
            for k, v in {"passed": passed, "failed": failed, "raw_exit_code": int(failed != 0)}.items())
            or result["status"] != (SNAPSHOT_STATUS if failed == 0 else "failed_authorization_time_snapshot")):
        raise GateError("Snapshot acceptance summary counters are inconsistent")
    historical = result.get("superseded_receiving_cut_probe")
    allowed = (
        {"status": "failed_acceptance_assertion", "finding": "RUNTIME-FINAL-01", "observed_returned_status": 200,
         "passed": 0, "failed": 1, "skipped": 0, "raw_exit_code": 1, "superseded": True},
        {"status": "passed_observation_not_contract_approval", "passed": 1, "failed": 0,
         "skipped": 0, "raw_exit_code": 0, "superseded": True},
    )
    if (not isinstance(historical, dict) or historical not in allowed
            or historical.get("superseded") is not True
            or any(type(historical.get(key)) is not int for key in ("passed", "failed", "skipped", "raw_exit_code"))
            or (historical["failed"] and type(historical.get("observed_returned_status")) is not int)):
        raise GateError("Missing or invalid superseded historical observation")


class Runner:
    def __init__(self, output: Path, deadline: int, shards: int):
        self.output, self.deadline, self.shards = output, deadline, shards
        self.active_artifacts = None

    @staticmethod
    def artifacts() -> dict[str, str]:
        paths = (BACKEND / "build/index_bg.wasm", BACKEND / "build/index.js",
                 BACKEND / "build/worker/shim.mjs", ROOT / "target/wasm32-unknown-unknown/release/brews_backend.wasm")
        if not all(p.is_file() for p in paths):
            raise GateError("A complete fresh local Worker build is required")
        return {str(p.relative_to(ROOT)): digest(p.read_bytes()) for p in paths}

    def require_artifacts(self) -> None:
        current = self.artifacts()
        if self.active_artifacts is None:
            self.active_artifacts = current
        elif self.active_artifacts != current:
            raise GateError("Worker artifacts changed during local execution")

    def command(self, name: str, command: list[str], cwd: Path = ROOT, required: bool = True) -> tuple[int, str]:
        code, text = execute(command, cwd, self.output / (name + ".json"), self.deadline)
        print(f"{name}: exit {code}", flush=True)
        if required and code:
            raise GateError(f"{name} failed; raw error payloads were not persisted")
        return code, text

    def native(self) -> dict:
        channel = tomllib.loads((ROOT / "rust-toolchain.toml").read_text())["toolchain"]["channel"]
        _, version = self.command("cargo-version", ["cargo", "--version"])
        if not version.startswith(f"cargo {channel} "):
            raise GateError("Cargo does not match the repository's pinned toolchain")
        self.command("format", ["cargo", "fmt", "--all", "--check"])
        counts = {}
        measurements = {}
        inventories = {}
        for profile in ("debug", "release-assertions"):
            command = ["cargo", "nextest", "run", "--workspace", "--all-features", "--locked", "--no-tests=fail", "--success-output", "immediate"]
            if profile != "debug":
                command += ["--cargo-profile", profile]
            _, text = self.command("native-" + profile, command)
            counts[profile] = parse_native(text)
            inventories[profile] = native_inventory(text)
            samples = measurement_lines(text, "native")
            if len(samples) != 1:
                raise GateError("Native measurement did not execute exactly once")
            measurements[profile] = samples[0]
        if len(set(counts.values())) != 1 or inventories["debug"] != inventories["release-assertions"]:
            raise GateError("Native profile inventories differ")
        _, text = self.command("doctests", ["cargo", "test", "--workspace", "--all-features", "--locked", "--doc"])
        counts["doctests"] = parse_doctests(text)
        self.command("clippy-native", ["cargo", "clippy", "--workspace", "--all-targets", "--all-features", "--locked", "--", "-D", "warnings"])
        self.command("clippy-wasm", ["cargo", "clippy", "-p", "brews-backend", "--target", "wasm32-unknown-unknown", "--locked", "--", "-D", "warnings"])
        counts["measurements"] = measurements
        counts["case_inventory_sha256"] = inventories["debug"]
        return counts

    def worker(self) -> dict:
        self.command("worker-build", ["npm", "run", "build"], BACKEND)
        self.require_artifacts()
        def shard(index: int) -> tuple[dict, list[dict]]:
            config = self.output / f"worker-shard-{index}.config.mjs"
            config.write_text(f"import base from {json.dumps((BACKEND / 'vitest.config.js').as_uri())};\n"
                              f"export default {{ ...base, cacheDir: {json.dumps(str(self.output / f'cache-{index}'))} }};\n")
            _, text = self.command(f"worker-shard-{index}", [str(BACKEND / "node_modules/.bin/vitest"), "run", "--config", str(config),
                                                            f"--shard={index}/{self.shards}", "--exclude", "worker-tests/protected_response_snapshot.test.js", "--reporter=default", "--reporter=json"], BACKEND)
            return parse_worker(text), measurement_lines(text, "actual-local-Workerd")
        with ThreadPoolExecutor(max_workers=self.shards) as pool:
            try:
                shards = list(pool.map(shard, range(1, self.shards + 1)))
            except BaseException:
                cancel_children()
                raise
        reports = [report for report, _ in shards]
        measurements = [sample for _, samples in shards for sample in samples]
        if len(measurements) != 1:
            raise GateError("Workerd measurement did not execute exactly once")
        expected = {str(p.resolve()) for p in (BACKEND / "worker-tests").rglob("*.test.js")
                    if p.resolve() != ROOT / ACCEPTANCE_SUITE}
        self.require_artifacts()
        result = reconcile_workers(reports, expected)
        result["measurements"] = measurements[0]
        return result

    def tls(self) -> dict:
        self.require_artifacts()
        self.command("cli-build", ["cargo", "build", "-p", "brews-cli", "--locked"])
        reports = {}
        for name, key in (("gameplay", "game"), ("operator", "operator"), ("dedicated_reads", "read"), ("game_audit", "game_audit")):
            _, text = self.command("tls-" + name, [sys.executable, f"tests/{name}_https_wss.py"], BACKEND)
            reports[name] = parse_tls(text, "actual_https_wss_" + key + "_checks")
        self.require_artifacts()
        return {"check_count": sum(v["check_count"] for v in reports.values()), "reports": reports}

    def contract(self) -> dict:
        inputs = contract_inputs()
        self.require_artifacts()
        code, text = self.command("protected-response-snapshot-acceptance", [str(BACKEND / "node_modules/.bin/vitest"), "run",
                                  "worker-tests/protected_response_snapshot.test.js", "--reporter=json"], BACKEND, required=False)
        acceptance = parse_snapshot_acceptance(parse_worker(text), code)
        self.require_artifacts()
        code, text = self.command("superseded-receiving-cut-probe", [str(BACKEND / "node_modules/.bin/vitest"), "run", "--config",
                                  str(BACKEND / "tools/historical_probe.config.mjs"), "--reporter=json"], BACKEND, required=False)
        historical = historical_observation(parse_worker(text), code)
        self.require_artifacts()
        result = {**acceptance, "release_input_sha256": inputs, "superseded_receiving_cut_probe": historical}
        validate_snapshot_contract(result)
        return result


def run(args: argparse.Namespace) -> int:
    CANCELLED.clear()
    output = args.output.expanduser().resolve()
    if output == ROOT or ROOT in output.parents:
        raise GateError("Use a new output directory outside the repository, normally under TMPDIR")
    output.mkdir(mode=0o700, parents=True, exist_ok=False)
    before = source_hashes(ROOT)
    lanes = LANES if args.lane == "all" else (args.lane,)
    runner = Runner(output, args.deadline, args.shards)
    report = {"kind": "local_release_checks_not_production_certification", "deployment": "none",
              "production_release": "blocked", "production_blockers": list(PRODUCTION_BLOCKERS),
              "requested_lanes": list(lanes), "source_sha256": before, "lanes": {}}
    report["baseline_commit"] = subprocess.check_output(
        ["git", "-c", f"safe.directory={ROOT}", "-C", str(ROOT), "rev-parse", "HEAD"], text=True,
    ).strip()
    report["python_version"] = sys.version.splitlines()[0]
    exit_code = 0
    try:
        if "worker" not in lanes and any(lane in lanes for lane in ("tls", "contract")):
            runner.command("standalone-worker-build", ["npm", "run", "build"], BACKEND)
            runner.require_artifacts()
        for lane in lanes:
            report["lanes"][lane] = getattr(runner, lane)()
            if lane == "contract":
                validate_snapshot_contract(report["lanes"][lane])
                if report["lanes"][lane]["failed"]:
                    exit_code = 2
    except (GateError, KeyError, TypeError, ValueError, OSError) as error:
        # No external raw exception/error chain is part of the report.
        report["execution_error"] = type(error).__name__
        exit_code = 1
    except KeyboardInterrupt:
        cancel_children()
        report["execution_error"] = "Cancelled"
        exit_code = 130
    finally:
        try:
            report["source_unchanged"] = before == source_hashes(ROOT)
        except (OSError, subprocess.SubprocessError):
            report["source_unchanged"] = False
        if not report["source_unchanged"]:
            exit_code = 1
        report["worker_artifact_sha256"] = runner.active_artifacts or {}
        if runner.active_artifacts is not None:
            try:
                runner.require_artifacts()
            except (GateError, OSError):
                exit_code = 1
                report["artifact_unchanged"] = False
            else:
                report["artifact_unchanged"] = True
        report["local_execution_exit_code"] = exit_code
        report["production_release"] = "blocked"
        save(output / "report.json", report)
    print(f"Report: {output / 'report.json'}; production release remains blocked", flush=True)
    return exit_code


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True, help="New private directory outside the repository; raw child output is not retained")
    parser.add_argument("--lane", choices=(*LANES, "all"), default="all")
    parser.add_argument("--shards", type=int, choices=range(1, 4), default=3)
    parser.add_argument("--deadline", type=int, choices=range(30, 3601), default=1800, metavar="SECONDS", help="Per-command bound, 30–3600 seconds")
    try:
        return run(parser.parse_args())
    except (GateError, OSError, subprocess.SubprocessError) as error:
        print(f"Local gate unavailable ({type(error).__name__}); no deployment authorized", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
