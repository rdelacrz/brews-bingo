"""Parser/unit fault tests; synthetic reporter fixtures are not execution evidence."""
import argparse
import copy

import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
import threading
from concurrent.futures import ThreadPoolExecutor
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("release_gate", Path(__file__).parents[1] / "release_gate.py")
assert SPEC is not None and SPEC.loader is not None
gate = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(gate)
SCRATCH = Path(os.environ.get("TMPDIR", str(Path.home() / ".hermes/cache/scratch")))
SCRATCH.mkdir(parents=True, exist_ok=True)


def worker(name="/fixture/a.test.js", case="unit fixture case"):
    return {"success": True, "numTotalTests": 1, "numPassedTests": 1,
            "numTotalTestSuites": 1, "numPassedTestSuites": 1,
            "numFailedTestSuites": 0, "numPendingTestSuites": 0,
            "numFailedTests": 0, "numPendingTests": 0, "numRuntimeErrorTestSuites": 0,
            "testResults": [{"name": name, "status": "passed", "assertionResults": [
                {"fullName": case, "status": "passed", "failureMessages": ["must not persist"]}]}]}


def snapshot_report():
    value = worker(str(gate.ROOT / gate.ACCEPTANCE_SUITE))
    cases = [{"fullName": name, "status": "passed", "failureMessages": []} for name in sorted(gate.ACCEPTANCE_CASES)]
    value["testResults"][0]["assertionResults"] = cases
    value["numTotalTests"] = value["numPassedTests"] = len(cases)
    return value


def historical_report(failed=True):
    name = "G4 shared RUNTIME-FINAL-01: actual recovery during final return must deny the old session"
    value = worker(str(gate.ROOT / "docs/reviews/evidence/command-lookup-final-return.test.js"), name)
    if failed:
        value.update(success=False, numPassedTests=0, numFailedTests=1, numPassedTestSuites=0, numFailedTestSuites=1)
        value["testResults"][0]["status"] = "failed"
        value["testResults"][0]["assertionResults"][0].update(status="failed", failureMessages=[
            "AssertionError: expected [ 401, 503 ] to include 200\n at command-lookup-final-return.test.js:85:24"])
    return value


def snapshot_summary():
    return {**gate.parse_snapshot_acceptance(snapshot_report(), 0),
            "release_input_sha256": gate.contract_inputs(),
            "superseded_receiving_cut_probe": gate.historical_observation(historical_report(), 1)}


class ReporterTests(unittest.TestCase):
    def test_measurement_markers_require_the_right_target_and_safe_fields(self):
        value = {"marker": "LOCAL-SLICE9-MEASUREMENTS", "target": "native", "history": [{"vm_steps": 42}]}
        text = "  LOCAL-SLICE9-MEASUREMENTS " + json.dumps(value)
        self.assertEqual(gate.measurement_lines(text, "native"), [value])
        obsolete = value | {"argon2": {"hash_wall_ns": 42}}
        with self.assertRaises(gate.GateError):
            gate.measurement_lines("LOCAL-SLICE9-MEASUREMENTS " + json.dumps(obsolete), "native")
        self.assertFalse(any("KDF" in blocker or "Argon2" in blocker for blocker in gate.PRODUCTION_BLOCKERS))
        self.assertEqual(gate.measurement_lines("No marker", "native"), [])
        for change in ({"target": "wrong"}, {"unknown": "field"}, {"history": [{"cookie": "unit fixture"}]},
                       {"pending": ["Bearer unit fixture"]}):
            with self.assertRaises(gate.GateError):
                gate.measurement_lines("LOCAL-SLICE9-MEASUREMENTS " + json.dumps({**value, **change}), "native")

    def test_complete_native_requires_both_summary_and_physical_pass_lines(self):
        text = "PASS [0.01s] fixture one\nPASS [0.01s] fixture two\nSummary [0.1s] 2 tests run: 2 passed, 0 skipped"
        self.assertEqual(gate.parse_native(text), 2)
        for invalid in (text + "\n" + text, text.replace("0 skipped", "1 skipped"), text.replace("2 passed", "1 passed"),
                        text.replace("PASS [0.01s] fixture two\n", ""), "Summary [0s] 0 tests run: 0 passed, 0 skipped"):
            with self.subTest(invalid=invalid), self.assertRaises(gate.GateError):
                gate.parse_native(invalid)

    def test_native_inventory_checks_unique_case_identity_not_only_counts(self):
        text = "PASS [0s] (1/2) binary one\nPASS [0s] (2/2) binary two\nSummary [0s] 2 tests run: 2 passed, 0 skipped"
        with self.assertRaises(gate.GateError):
            gate.native_inventory(text.replace("binary two", "binary one"))
        self.assertNotEqual(gate.native_inventory(text), gate.native_inventory(text.replace("binary two", "binary three")))

    def test_doctests_reject_ignored_filtered_failed_or_missing_targets(self):
        text = "Doc-tests fixture\ntest result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out"
        self.assertEqual(gate.parse_doctests(text), 2)
        for invalid in (text.replace("0 ignored", "1 ignored"), text.replace("0 filtered", "1 filtered"),
                        text.replace("0 failed", "1 failed"), text.replace("Doc-tests fixture", ""), text + "\n" + text):
            with self.assertRaises(gate.GateError):
                gate.parse_doctests(invalid)

    def test_json_report_can_follow_workerd_diagnostics_but_cannot_be_ambiguous(self):
        text = "Worker starting\n" + json.dumps(worker()) + "\nWorker stopped"
        self.assertEqual(gate.parse_worker(text), worker())
        for invalid in ("No reporter output", text + "\n" + json.dumps(worker())):
            with self.assertRaises(gate.GateError):
                gate.parse_worker(invalid)

    def test_full_file_inventory_not_declared_nested_suite_count(self):
        result = gate.reconcile_workers([worker(), worker("/fixture/b.test.js")],
                                        {"/fixture/a.test.js", "/fixture/b.test.js"})
        self.assertEqual((result["passed"], result["physical_suites"]), (2, 2))
        self.assertNotIn("failureMessages", json.dumps(result))
        self.assertNotIn("unit fixture case", json.dumps(result))
        with self.assertRaises(gate.GateError):
            gate.reconcile_workers([worker()], {"/fixture/a.test.js", "/fixture/b.test.js"})
        with self.assertRaises(gate.GateError):
            gate.reconcile_workers([worker()], {"/fixture/extra.test.js"})

    def test_duplicate_files_or_cases_fail(self):
        with self.assertRaises(gate.GateError):
            gate.reconcile_workers([worker(), worker()], {"/fixture/a.test.js"})
        value = worker()
        value["testResults"][0]["assertionResults"] *= 2
        value["numTotalTests"] = value["numPassedTests"] = 2
        with self.assertRaises(gate.GateError):
            gate.reconcile_workers([value], {"/fixture/a.test.js"})

    def test_skips_failures_runtime_errors_empty_suites_and_count_drift_fail(self):
        mutations = (
            lambda r: r.update(success=False), lambda r: r.update(numPendingTests=1),
            lambda r: r.update(numFailedTests=1), lambda r: r.update(numRuntimeErrorTestSuites=1),
            lambda r: r.update(unhandledErrors=["unit fixture error"]), lambda r: r.update(numTotalTests=2),
            lambda r: r["testResults"][0].update(status="failed"),
            lambda r: r["testResults"][0]["assertionResults"][0].update(status="pending"),
            lambda r: r["testResults"][0].update(assertionResults=[]),
        )
        for mutation in mutations:
            value = worker()
            mutation(value)
            with self.subTest(mutation=mutation), self.assertRaises(gate.GateError):
                gate.reconcile_workers([value], {"/fixture/a.test.js"})

    def test_tls_must_be_unique_local_complete_and_unambiguous(self):
        key = "actual_https_wss_game_checks"
        value = {key: "passed", "checks": ["fixture-one", "fixture-two"], "check_count": 2, "deployment": "none"}
        self.assertEqual(gate.parse_tls(json.dumps(value), key)["check_count"], 2)
        self.assertNotIn("fixture-one", json.dumps(gate.parse_tls(json.dumps(value), key)))
        for change in ({"deployment": "production"}, {"check_count": 1}, {"checks": ["duplicate", "duplicate"]},
                       {key: "failed"}, {"checks": [], "check_count": 0}):
            with self.assertRaises(gate.GateError):
                gate.parse_tls(json.dumps({**value, **change}), key)
        with self.assertRaises(gate.GateError):
            gate.parse_tls(json.dumps(value) + "\n" + json.dumps(value), key)

    def test_contract_fail_remains_a_failed_assertion_not_an_expected_pass(self):
        name = "G4 shared RUNTIME-FINAL-01: actual recovery during final return must deny the old session"
        value = worker(name=str(gate.ROOT / "docs/reviews/evidence/command-lookup-final-return.test.js"), case=name)
        self.assertEqual(gate.parse_contract(value, 0)["status"], "passed_observation_not_contract_approval")
        value["success"] = False
        value["numPassedTests"] = 0
        value["numFailedTests"] = 1
        value["numPassedTestSuites"] = 0
        value["numFailedTestSuites"] = 1
        value["testResults"][0]["status"] = "failed"
        value["testResults"][0]["assertionResults"][0]["status"] = "failed"
        value["testResults"][0]["assertionResults"][0]["failureMessages"] = [
            "AssertionError: expected [ 401, 503 ] to include 200\n at command-lookup-final-return.test.js:85:24"]
        self.assertEqual(gate.parse_contract(value, 1)["failed"], 1)
        for mutated, code in ((value, 0), (value, -9), (value, 124), ({**value, "numPendingTests": 1}, 1), (worker(), 1),
                              ({**value, "testResults": value["testResults"] + [{"status": "failed", "assertionResults": []}]}, 1)):
            with self.assertRaises(gate.GateError):
                gate.parse_contract(mutated, code)
        value["testResults"][0]["assertionResults"][0]["failureMessages"] = ["Fixture failed earlier"]
        with self.assertRaises(gate.GateError):
            gate.parse_contract(value, 1)


    def test_snapshot_requires_exact_suite_and_case_names_without_duplicates(self):
        self.assertEqual(gate.parse_snapshot_acceptance(snapshot_report(), 0)["passed"], 20)
        for mutation in (
            lambda r: r["testResults"][0].update(name="/fixture/other.test.js"),
            lambda r: r["testResults"][0]["assertionResults"].pop(),
            lambda r: r["testResults"][0]["assertionResults"].append(r["testResults"][0]["assertionResults"][0]),
            lambda r: r["testResults"][0]["assertionResults"][0].update(fullName="renamed fixture"),
            lambda r: r["testResults"].append(copy.deepcopy(r["testResults"][0])),
        ):
            value = snapshot_report()
            mutation(value)
            with self.assertRaises(gate.GateError):
                gate.parse_snapshot_acceptance(value, 0)

    def test_snapshot_rejects_skips_runtime_errors_inconsistent_and_ambiguous_reporters(self):
        for mutation in (
            lambda r: r.update(numPendingTests=1), lambda r: r.update(numPendingTestSuites=1),
            lambda r: r.update(numTodoTests=1), lambda r: r.update(numRuntimeErrorTestSuites=1),
            lambda r: r.update(unhandledErrors=["fixture"]), lambda r: r.update(success=1),
            lambda r: r.update(numPassedTests=19), lambda r: r.update(numTotalTests=True),
            lambda r: r["testResults"][0]["assertionResults"][0].update(status="pending"),
            lambda r: r["testResults"][0]["assertionResults"][0].update(status="todo"),
        ):
            value = snapshot_report()
            mutation(value)
            with self.assertRaises(gate.GateError):
                gate.parse_snapshot_acceptance(value, 0)
        raw = json.dumps(snapshot_report())
        with self.assertRaises(gate.GateError):
            gate.parse_worker(raw + "\n" + raw)
        for code in (1, 124, -9, False):
            with self.assertRaises(gate.GateError):
                gate.parse_snapshot_acceptance(snapshot_report(), code)

    def test_real_acceptance_failure_remains_nonzero_and_payloads_are_not_reported(self):
        value = snapshot_report()
        value.update(success=False, numPassedTests=19, numFailedTests=1, numPassedTestSuites=0, numFailedTestSuites=1)
        value["testResults"][0]["status"] = "failed"
        value["testResults"][0]["assertionResults"][0].update(status="failed", failureMessages=["private fixture"])
        parsed = gate.parse_snapshot_acceptance(value, 1)
        self.assertEqual((parsed["status"], parsed["passed"], parsed["failed"]), ("failed_authorization_time_snapshot", 19, 1))
        self.assertNotIn("private fixture", json.dumps(parsed))
        with self.assertRaises(gate.GateError):
            gate.parse_snapshot_acceptance(value, 0)

    def test_historical_failure_is_preserved_and_unrelated_failures_block(self):
        observed = gate.historical_observation(historical_report(), 1)
        self.assertEqual((observed["raw_exit_code"], observed["failed"], observed["observed_returned_status"]), (1, 1, 200))
        self.assertTrue(observed["superseded"])
        passed = gate.historical_observation(historical_report(False), 0)
        self.assertEqual((passed["raw_exit_code"], passed["passed"], passed["failed"]), (0, 1, 0))
        for mutation in (
            lambda r: r.update(numRuntimeErrorTestSuites=1), lambda r: r.update(unhandledErrors=["fixture error"]),
            lambda r: r["testResults"][0]["assertionResults"][0].update(failureMessages=["earlier infrastructure failure"]),
            lambda r: r["testResults"][0]["assertionResults"][0].update(failureMessages=[
                "AssertionError: expected [ 401, 503 ] to include 200\n at command-lookup-final-return.test.js:85:24\nUnrelated failure"]),
            lambda r: r["testResults"][0]["assertionResults"][0].update(failureMessages=[
                "expected [ 401, 503 ] to include 200 at command-lookup-final-return.test.js:84:24"]),
        ):
            value = historical_report()
            mutation(value)
            with self.assertRaises(gate.GateError):
                gate.historical_observation(value, 1)

    def test_summary_cannot_bypass_policy_new_acceptance_or_historical_classification(self):
        summary = snapshot_summary()
        gate.validate_snapshot_contract(summary)
        for mutation in (
            lambda r: r.pop("policy"), lambda r: r.update(policy="receiving_cut"),
            lambda r: r.update(status="passed_observation_not_contract_approval"),
            lambda r: r.pop("release_input_sha256"), lambda r: r["release_input_sha256"].pop(gate.POLICY_DOCUMENT),
            lambda r: r.update(case_inventory={}), lambda r: r.update(skipped=1),
            lambda r: r.update(passed=19), lambda r: r.pop("superseded_receiving_cut_probe"),
            lambda r: r["superseded_receiving_cut_probe"].update(superseded=False),
            lambda r: r["superseded_receiving_cut_probe"].update(raw_exit_code=0),
            lambda r: r["superseded_receiving_cut_probe"].update(raw_exit_code=True),
            lambda r: r["superseded_receiving_cut_probe"].update(failed=0, status="passed"),
            lambda r: r.update(execution_scope="worker_and_contract_double_counted"),
        ):
            value = copy.deepcopy(summary)
            mutation(value)
            with self.assertRaises(gate.GateError):
                gate.validate_snapshot_contract(value)

    def test_contract_inputs_require_approved_policy_new_suite_and_unchanged_original(self):
        with tempfile.TemporaryDirectory(dir=SCRATCH) as directory:
            root = Path(directory)
            paths = [gate.POLICY_DOCUMENT, gate.ACCEPTANCE_SUITE, *gate.HISTORICAL_INPUTS]
            for name in paths:
                target = root / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes((gate.ROOT / name).read_bytes())
            self.assertEqual(gate.contract_inputs(root), gate.contract_inputs())
            for name in paths:
                target = root / name
                saved = target.read_bytes()
                target.unlink()
                with self.assertRaises(gate.GateError):
                    gate.contract_inputs(root)
                target.write_bytes(saved)
                target.write_bytes(b"superseded unapproved fixture")
                if name != gate.ACCEPTANCE_SUITE:
                    with self.assertRaises(gate.GateError):
                        gate.contract_inputs(root)
                target.write_bytes(saved)

    def test_historical_route_adapter_changes_only_the_archived_probe_url(self):
        inputs = gate.contract_inputs()
        script = """
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import config, { historicalRecoveryRoute as adapter } from './tools/historical_probe.config.mjs';
const path = fileURLToPath(new URL('../docs/reviews/evidence/command-lookup-final-return.test.js', import.meta.url));
const source = readFileSync(path, 'utf8');
const oldRoute = '"/api/player-recovery"';
const expected = source.replace(oldRoute, '"/api/games/player-recovery"');
assert.equal(config.plugins[0], adapter);
for (const id of [path, path + '?v=fixture']) {
  const actual = adapter.transform(source, id);
  assert.equal(actual.code, expected);
  assert.equal(actual.code.split('\\n').length, source.split('\\n').length);
}
assert.equal(adapter.transform(source, path + '.other'), null);
assert.equal(adapter.transform(source, '/fixture/current.test.js'), null);
assert.throws(() => adapter.transform('', path));
assert.throws(() => adapter.transform(source + oldRoute, path));
assert.equal(readFileSync(path, 'utf8'), source);
"""
        result = gate.subprocess.run(["node", "--input-type=module", "-e", script],
                                     cwd=gate.BACKEND, capture_output=True, text=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(gate.contract_inputs(), inputs)

    def test_source_manifest_includes_approved_policy_and_github_policy_inputs(self):
        with tempfile.TemporaryDirectory(dir=SCRATCH) as directory:
            root = Path(directory)
            names = [gate.POLICY_DOCUMENT, gate.ACCEPTANCE_SUITE, ".github/workflows/validation.yml",
                     ".github/workflows/release.yml", "docs/operations/cicd.md", "docs/plans/lld.md",
                     "backend/tools/historical_probe.config.mjs", "platform/web/src/main.rs",
                     "platform/web/index.html", "platform/web/package-lock.json",
                     "platform/web/public/assets/favicon.svg", "shared/ui/assets/theme.css", "private.env"]
            for name in names:
                target = root / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text("unit fixture")
            with patch.object(gate.subprocess, "check_output", return_value="\n".join(names)):
                self.assertEqual(set(gate.source_hashes(root)), set(names) - {"private.env"})


class ExecutionTests(unittest.TestCase):
    def test_registered_parallel_children_are_cancelled_from_the_main_thread(self):
        gate.CANCELLED.clear()
        ready = threading.Event()
        original = gate.subprocess.Popen
        created = 0
        def spawn(*args, **kwargs):
            nonlocal created
            process = original(*args, **kwargs)
            created += 1
            if created == 2:
                ready.set()
            return process
        with tempfile.TemporaryDirectory(dir=SCRATCH) as directory:
            try:
                with patch.object(gate.subprocess, "Popen", side_effect=spawn), ThreadPoolExecutor(max_workers=2) as pool:
                    futures = [pool.submit(gate.execute, [sys.executable, "-c", "import time; time.sleep(20)"],
                                           SCRATCH, Path(directory) / f"child-{index}.json", 30) for index in range(2)]
                    self.assertTrue(ready.wait(timeout=3))
                    gate.cancel_children()
                    self.assertTrue(all(future.result(timeout=3)[0] != 0 for future in futures))
                    self.assertFalse(gate.CHILDREN)
            finally:
                gate.CANCELLED.clear()

    def test_real_child_deadline_terminates_the_process_group(self):
        with tempfile.TemporaryDirectory(dir=SCRATCH) as directory:
            target = Path(directory) / "timeout.json"
            code, _ = gate.execute([sys.executable, "-c", "import time; time.sleep(5)"], SCRATCH, target, 1)
            self.assertNotEqual(code, 0)
            self.assertTrue(json.loads(target.read_text())["timed_out"])

    def test_worker_artifact_changes_are_rejected(self):
        runner = gate.Runner(SCRATCH, 30, 1)
        with patch.object(gate.Runner, "artifacts", side_effect=[{"fixture": "before"}, {"fixture": "after"}]):
            runner.require_artifacts()
            with self.assertRaises(gate.GateError):
                runner.require_artifacts()

    def test_real_child_output_is_hashed_not_persisted(self):
        with tempfile.TemporaryDirectory(dir=SCRATCH) as directory:
            target = Path(directory) / "execution.json"
            with patch.dict(os.environ, {"RELEASE_GATE_TEST_OUTPUT": "private-test-fixture-output"}):
                code, output = gate.execute([sys.executable, "-c", "import os; print(os.environ['RELEASE_GATE_TEST_OUTPUT'])"],
                                            SCRATCH, target, 30)
            self.assertEqual(code, 0)
            self.assertIn("private-test-fixture-output", output)
            saved = target.read_text()
            self.assertNotIn("private-test-fixture-output", saved)
            self.assertFalse(json.loads(saved)["raw_output_persisted"])
            self.assertEqual(target.stat().st_mode & 0o777, 0o600)

    def test_existing_output_directory_is_not_reused(self):
        with tempfile.TemporaryDirectory(dir=SCRATCH) as directory:
            args = argparse.Namespace(output=Path(directory), deadline=30, lane="native", shards=1)
            with self.assertRaises(FileExistsError):
                gate.run(args)

    def test_output_cannot_be_inside_the_repository(self):
        args = argparse.Namespace(output=gate.ROOT / "unit-fixture-output", deadline=30, lane="native", shards=1)
        with self.assertRaises(gate.GateError):
            gate.run(args)

    def test_failed_contract_and_source_drift_cannot_be_green(self):
        for drift in (False, True):
            with tempfile.TemporaryDirectory(dir=SCRATCH) as directory:
                output = Path(directory) / "new-run"
                args = argparse.Namespace(output=output, deadline=30, lane="contract", shards=1)
                with patch.object(gate, "source_hashes", side_effect=[{"fixture": "before"}, {"fixture": "after" if drift else "before"}]), \
                     patch.object(gate.Runner, "command", return_value=(0, "")), \
                     patch.object(gate.Runner, "require_artifacts"), \
                     patch.object(gate.Runner, "contract", return_value={"status": "failed_acceptance_assertion", "failed": 1}):
                    code = gate.run(args)
                report = json.loads((output / "report.json").read_text())
                self.assertEqual(code, 1)
                self.assertEqual(report["production_release"], "blocked")
                self.assertEqual(report["source_unchanged"], not drift)

    def test_approved_snapshot_with_preserved_historical_failure_is_green_locally(self):
        for acceptance_failed in (False, True):
            summary = snapshot_summary()
            if acceptance_failed:
                summary.update(status="failed_authorization_time_snapshot", passed=19, failed=1, raw_exit_code=1)
                summary["case_inventory"][str(gate.ROOT / gate.ACCEPTANCE_SUITE)][0]["status"] = "failed"
            with tempfile.TemporaryDirectory(dir=SCRATCH) as directory:
                args = argparse.Namespace(output=Path(directory) / "new-run", deadline=30, lane="contract", shards=1)
                with patch.object(gate, "source_hashes", return_value={}), \
                     patch.object(gate.Runner, "command", return_value=(0, "")), \
                     patch.object(gate.Runner, "require_artifacts"), \
                     patch.object(gate.Runner, "contract", return_value=summary):
                    self.assertEqual(gate.run(args), 2 if acceptance_failed else 0)
                report = json.loads((args.output / "report.json").read_text())
                self.assertEqual(report["lanes"]["contract"]["superseded_receiving_cut_probe"]["failed"], 1)
                self.assertEqual(report["production_release"], "blocked")

    def test_contract_executes_separate_new_acceptance_and_unchanged_original_without_build(self):
        with tempfile.TemporaryDirectory(dir=SCRATCH) as directory:
            runner = gate.Runner(Path(directory), 30, 1)
            responses = [(0, json.dumps(snapshot_report())), (1, json.dumps(historical_report()))]
            with patch.object(runner, "require_artifacts"), patch.object(runner, "command", side_effect=responses) as command:
                result = runner.contract()
            self.assertEqual(result["passed"], 20)
            self.assertEqual(result["failed"], 0)
            self.assertEqual(command.call_count, 2)
            self.assertIn("worker-tests/protected_response_snapshot.test.js", command.call_args_list[0].args[1])
            self.assertIn(str(gate.BACKEND / "tools/historical_probe.config.mjs"), command.call_args_list[1].args[1])

    def test_passing_local_lane_is_not_production_certification(self):
        with tempfile.TemporaryDirectory(dir=SCRATCH) as directory:
            args = argparse.Namespace(output=Path(directory) / "new-run", deadline=30, lane="native", shards=1)
            with patch.object(gate, "source_hashes", return_value={}), patch.object(gate.Runner, "native", return_value={"unit_fixture": 1}):
                self.assertEqual(gate.run(args), 0)
            report = json.loads((args.output / "report.json").read_text())
            self.assertEqual(report["production_release"], "blocked")
            self.assertEqual(report["deployment"], "none")
            self.assertTrue(report["production_blockers"])


if __name__ == "__main__":
    unittest.main()
