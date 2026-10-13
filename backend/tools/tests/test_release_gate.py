"""Parser/unit fault tests; synthetic reporter fixtures are not execution evidence."""
import argparse

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


class ReporterTests(unittest.TestCase):
    def test_measurement_markers_require_the_right_target_and_safe_fields(self):
        value = {"marker": "LOCAL-SLICE9-MEASUREMENTS", "target": "native", "history": [{"vm_steps": 42}]}
        text = "  LOCAL-SLICE9-MEASUREMENTS " + json.dumps(value)
        self.assertEqual(gate.measurement_lines(text, "native"), [value])
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
                self.assertEqual(code, 1 if drift else 2)
                self.assertEqual(report["production_release"], "blocked")
                self.assertEqual(report["source_unchanged"], not drift)

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
