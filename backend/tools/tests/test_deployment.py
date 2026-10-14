"""Local fixture and mocked provider boundaries; no authenticated provider calls."""
import copy
from datetime import datetime, timedelta, timezone
from email.message import Message
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch, MagicMock
import zipfile

SPEC = importlib.util.spec_from_file_location("deployment", Path(__file__).parents[1] / "deployment.py")
d = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(d)


def targets():
    return {env: {"name": f"brews-bingo-api-{env}", "account_id": digit * 32, "origin": f"https://{env}.brews.example.com", "namespace_ids": {name: f"{index:032x}" for index, name in enumerate(d.CLASSES, start=start)}, "rate_secret_ref": env + "/rate", "dev_secret_ref": env + "/dev"} for env, digit, start in (("staging", "a", 1), ("production", "b", 4))}


def bindings(target):
    return [{"name": name, "type": "durable_object_namespace", "class_name": cls, "namespace_id": target["namespace_ids"][name]} for name, cls in d.CLASSES.items()] + [{"name": "APP_ORIGIN", "type": "plain_text", "text": target["origin"]}, {"name": "RATE_LIMIT_KEY", "type": "secret_text"}, {"name": "DEV_CLI_KEY", "type": "secret_text"}, {"name": "CF_VERSION_METADATA", "type": "version_metadata"}]


def deployment():
    return {"deployments": [{"id": "11111111-1111-4111-8111-111111111111", "created_on": "2026-10-14T00:00:00Z", "strategy": "percentage", "versions": [{"version_id": "22222222-2222-4222-8222-222222222222", "percentage": 100}]}]}


class DeploymentTests(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="brews-deployment-unit-")
        self.addCleanup(self.scratch.cleanup)
        self.base = Path(self.scratch.name)
        self.root = self.base / "source"
        self.root.mkdir()
        (self.root / "backend").mkdir()
        (self.root / "backend/wrangler.toml").write_bytes((d.ROOT / "backend/wrangler.toml").read_bytes())
        (self.root / "rust-toolchain.toml").write_text('[toolchain]\nchannel = "1.99.0"\n')
        (self.root / "backend/package.json").write_text('{"devDependencies":{"wrangler":"4.147.0"}}\n')
        (self.root / "backend/source.rs").write_text('// disposable unit fixture\n')
        installed = self.root / "backend/node_modules/wrangler/package.json"
        installed.parent.mkdir(parents=True)
        installed.write_text('{"version":"4.147.0"}')
        for name in ("backend/tools/release_gate.py", "docs/plans/protected-response-policy.md",
                     "AGENTS.md", "docs/rust-best-practices.md", "docs/operations/cicd.md",
                     ".github/workflows/validation.yml", ".github/workflows/release.yml",
                     "backend/worker-tests/protected_response_snapshot.test.js",
                     "docs/reviews/evidence/command-lookup-final-return.test.js",
                     "docs/reviews/evidence/command-lookup-final-return.config.mjs",
                     "backend/tests/e2e_support/scenario.py"):
            destination = self.root / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes((d.ROOT / name).read_bytes())
        (self.root / ".gitignore").write_text("backend/node_modules/\nbackend/build/\ntarget/\n")
        for args in (("init",), ("add", "."), ("-c", "user.name=Unit Fixture", "-c", "user.email=unit@example.invalid", "commit", "-m", "fixture")):
            subprocess.run(["git", "-C", str(self.root), *args], check=True, capture_output=True)
        self.commit = d.git(self.root, "rev-parse", "HEAD")
        self.artifact = self.base / "frozen"
        self.artifact.mkdir()
        for name in d.ASSETS:
            path = self.artifact / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"disposable local fixture: " + name.encode())
        self.sources = d.hashes(self.root, d.source_names(self.root))
        self.assets = d.hashes(self.artifact, d.ASSETS)
        spec = importlib.util.spec_from_file_location("fixture_gate", self.root / "backend/tools/release_gate.py")
        runner = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(runner)
        self.contract = {"status": runner.SNAPSHOT_STATUS, "policy": runner.SNAPSHOT_POLICY,
                         "passed": len(runner.ACCEPTANCE_CASES), "failed": 0, "skipped": 0,
                         "physical_suites": 1, "raw_exit_code": 0,
                         "execution_scope": "contract_only_excluded_from_worker_lane",
                         "release_input_sha256": runner.contract_inputs(self.root),
                         "case_inventory": {str(self.root / runner.ACCEPTANCE_SUITE): [
                             {"case_id_sha256": d.sha(name.encode()), "status": "passed"}
                             for name in sorted(runner.ACCEPTANCE_CASES)]},
                         "superseded_receiving_cut_probe": {"status": "failed_acceptance_assertion",
                             "finding": "RUNTIME-FINAL-01", "observed_returned_status": 200,
                             "passed": 0, "failed": 1, "skipped": 0, "raw_exit_code": 1, "superseded": True}}
        self.gate = {"baseline_commit": self.commit, "source_unchanged": True, "artifact_unchanged": True, "local_execution_exit_code": 0, "requested_lanes": ["native", "worker", "tls", "contract"], "lanes": {"contract": self.contract}, "source_sha256": self.sources, "worker_artifact_sha256": {name: digest for name, digest in self.assets.items() if name.endswith(".wasm")}}
        self.manifest = {"schema": 1, "commit": self.commit, "tools": d.TOOLS, "source_sha256": self.sources, "asset_sha256": self.assets, "local_gate_sha256": d.sha(d.canonical(self.gate)), "code_sha256": d.sha(d.canonical(self.assets)), "probe_inputs": [], "local_gate": self.gate}
        d.save(self.artifact / "manifest.json", self.manifest)
        self.digest = d.sha((self.artifact / "manifest.json").read_bytes())

    def verify(self):
        return d.verify(self.root, self.artifact, self.digest, self.commit)

    def test_fixture_manifest_verifies(self):
        self.assertEqual(self.verify(), self.manifest)

    def test_deployment_and_honest_gate_include_the_same_policy_sources(self):
        spec = importlib.util.spec_from_file_location('honest_fixture_gate', self.root / 'backend/tools/release_gate.py')
        assert spec is not None and spec.loader is not None
        runner = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(runner)
        honest_sources = runner.source_hashes(self.root)
        self.assertEqual(d.hashes(self.root, d.source_names(self.root)), honest_sources)
        for name in ('AGENTS.md', 'docs/rust-best-practices.md', 'docs/operations/cicd.md'):
            self.assertIn(name, honest_sources)
        d.local_gate(self.gate | {'source_sha256': honest_sources}, self.sources,
                     self.assets, self.commit, self.root)

    def test_genuine_passing_historical_observation_is_not_required_to_fail(self):
        contract = copy.deepcopy(self.contract)
        contract['superseded_receiving_cut_probe'] = {
            'status': 'passed_observation_not_contract_approval', 'passed': 1,
            'failed': 0, 'skipped': 0, 'raw_exit_code': 0, 'superseded': True,
        }
        d.snapshot_contract(self.root, contract)

    def test_artifact_drift(self):
        (self.artifact / d.ASSETS[0]).write_bytes(b"drift")
        with self.assertRaises(d.DeploymentError):
            self.verify()

    def test_source_drift(self):
        (self.root / "backend/source.rs").write_text("changed")
        with self.assertRaises(d.DeploymentError):
            self.verify()

    def test_extra_secret_file_forbidden(self):
        (self.artifact / ".dev.vars").write_text("ephemeral fixture only")
        with self.assertRaises(d.DeploymentError):
            self.verify()

    def test_manifest_tampering(self):
        (self.artifact / "manifest.json").write_text("{}")
        with self.assertRaises(d.DeploymentError):
            self.verify()

    def test_symlink_forbidden(self):
        asset = self.artifact / d.ASSETS[0]
        asset.unlink()
        asset.symlink_to(self.root / "backend/source.rs")
        with self.assertRaises(d.DeploymentError):
            self.verify()

    def test_safe_transfer(self):
        archive = self.base / "release.zip"
        d.pack(self.artifact, archive)
        destination = self.base / "received"
        d.unpack(archive, destination)
        self.assertEqual(d.verify(self.root, destination, self.digest, self.commit), self.manifest)
        self.assertEqual((destination / "target/debug/brews").stat().st_mode & 0o777, 0o500)

    def test_zip_traversal_absolute_duplicate_and_link(self):
        for index, unsafe in enumerate(("../escape", "/absolute", "backend/build/index.js", "link")):
            archive = self.base / f"unsafe-{index}.zip"
            with zipfile.ZipFile(archive, "w") as bundle:
                for name in ("manifest.json", *d.ASSETS):
                    bundle.writestr(name, b"fixture")
                if unsafe == "link":
                    info = zipfile.ZipInfo("link")
                    info.external_attr = 0o120777 << 16
                    bundle.writestr(info, "../escape")
                else:
                    import warnings
                    with warnings.catch_warnings():
                        warnings.simplefilter("ignore")
                        bundle.writestr(unsafe, b"fixture")
            with self.subTest(unsafe=unsafe), self.assertRaises(d.DeploymentError):
                d.unpack(archive, self.base / f"output-{index}")

    def test_unsafe_origins(self):
        for value in ("http://staging.example.com", "https://a.invalid", "https://localhost", "https://127.0.0.1", "https://user@a.example.com", "https://a.example.com/", "https://a.example.com:443", "https://a.example.com?q=1", "https://a.example.com#x", "https://a..example.com", "https://a.local", "https://a.example.com\n"):
            with self.subTest(value=value), self.assertRaises(d.DeploymentError):
                d.origin(value)

    def test_inventory_and_crosslinks(self):
        self.assertEqual(d.inventory(targets()), targets())
        for field, wrong in (("name", "brews-bingo-api-production"), ("origin", targets()["production"]["origin"]), ("account_id", "missing"), ("rate_secret_ref", "production/rate"), ("dev_secret_ref", "staging/rate")):
            value = targets()
            value["staging"][field] = wrong
            with self.subTest(field=field), self.assertRaises(d.DeploymentError):
                d.inventory(value)
        value = targets()
        value["staging"]["namespace_ids"]["GAMES"] = value["production"]["namespace_ids"]["GAMES"]
        with self.assertRaises(d.DeploymentError):
            d.inventory(value)

    def test_generate_real_config(self):
        config = d.release_config(self.root, self.artifact, self.manifest, targets(), "staging")
        output = self.base / "generated.json"
        d.save(output, config)
        loaded = json.loads(output.read_bytes())
        self.assertNotIn("build", loaded)
        self.assertNotIn("env", loaded)
        self.assertEqual(loaded["main"], str(self.artifact / "backend/build/worker/shim.mjs"))
        self.assertEqual(loaded["migrations"], d.template(self.root)["migrations"])
        self.assertEqual(loaded["vars"], {"APP_ORIGIN": targets()["staging"]["origin"]})
        self.assertTrue(loaded["no_bundle"])
        self.assertFalse(any("script_name" in b for b in loaded["durable_objects"]["bindings"]))
        self.assertEqual(output.stat().st_mode & 0o777, 0o600)

    def test_template_duplicate_and_cross_worker_bindings(self):
        path = self.root / "backend/wrangler.toml"
        original = path.read_text()
        for modified in (original.replace('class_name = "GameObject" }]', 'class_name = "GameObject", script_name = "brews-bingo-api-production" }]'), original.replace('tag = "games"', 'tag = "modernized"')):
            path.write_text(modified)
            with self.assertRaises(d.DeploymentError):
                d.template(self.root)
        path.write_text(original)

    def test_exit_two_is_blocked(self):
        for change in ({"local_execution_exit_code": 2}, {"requested_lanes": ["native"]}, {"artifact_unchanged": False}, {"execution_error": "failure"}):
            gate = self.gate | change
            with self.assertRaises(d.DeploymentError):
                d.local_gate(gate, self.sources, self.assets, self.commit)

    def test_missing_manual_protection(self):
        good = {"name": "production", "can_admins_bypass": False, "deployment_branch_policy": {"protected_branches": True, "custom_branch_policies": False}, "protection_rules": [{"type": "required_reviewers", "prevent_self_review": True, "reviewers": [{"type": "User", "reviewer": {"id": 123}}]}]}
        d.manual_protection(good)
        for change in ({"can_admins_bypass": True}, {"protection_rules": []}, {"deployment_branch_policy": None}):
            with self.assertRaises(d.DeploymentError):
                d.manual_protection(good | change)
        good["protection_rules"][0]["prevent_self_review"] = False
        with self.assertRaises(d.DeploymentError):
            d.manual_protection(good)

    def test_external_gates_required_separately(self):
        proof = {"status": "passed", "evidence_sha256": "a" * 64}
        value = {"schema": 1, "commit": self.commit, "manifest_sha256": self.digest, "inventory_sha256": d.sha(d.canonical(targets())), "reviewed_by": "reviewer", "expires_at": (datetime.now(timezone.utc) + timedelta(hours=1)).isoformat(), "runtime_final_01": proof, "slice_reviews": proof, "autodeploy_disabled": True, "gates": {name: proof.copy() for name in d.GATES}}
        d.readiness(value, self.manifest, self.digest, targets())
        for name in d.GATES:
            bad = copy.deepcopy(value)
            bad["gates"][name]["status"] = "OPEN"
            with self.subTest(gate=name), self.assertRaises(d.DeploymentError):
                d.readiness(bad, self.manifest, self.digest, targets())
        for change in ({"autodeploy_disabled": False}, {"manifest_sha256": "b" * 64}, {"expires_at": "2000-01-01T00:00:00Z"}):
            with self.assertRaises(d.DeploymentError):
                d.readiness(value | change, self.manifest, self.digest, targets())

    def test_remote_duplicate_wrong_origin_secrets_and_namespace(self):
        target = targets()["staging"]
        good = bindings(target)
        d.remote_bindings(good, target)
        candidates = [good + [good[0]], good[:-1], good + [{"name": "UNEXPECTED", "type": "plain_text"}]]
        for field, wrong in (("script_name", targets()["production"]["name"]), ("namespace_id", targets()["production"]["namespace_ids"]["ACCOUNTS"]), ("class_name", "OtherClass")):
            changed = copy.deepcopy(good)
            changed[0][field] = wrong
            candidates.append(changed)
        changed = copy.deepcopy(good)
        changed[3]["text"] = targets()["production"]["origin"]
        candidates.append(changed)
        for bad in candidates:
            with self.assertRaises(d.DeploymentError):
                d.remote_bindings(bad, target)

    def test_active_deployment_rejects_malformed_partial_absent(self):
        self.assertEqual(d.active_deployment(deployment())[0], deployment()["deployments"][0]["id"])
        for bad in ({}, {"deployments": []}, {"deployments": [deployment()["deployments"][0]] * 2}):
            with self.assertRaises(d.DeploymentError):
                d.active_deployment(bad)
        bad = deployment()
        bad["deployments"][0]["versions"][0]["percentage"] = 50
        with self.assertRaises(d.DeploymentError):
            d.active_deployment(bad)

    def test_exact_remote_readback_and_mid_read_drift(self):
        target = targets()["staging"]
        version = self.version()
        settings = {"bindings": bindings(target)}
        with patch.object(d, "serving_identity"), patch.object(d, "cloudflare", side_effect=[deployment(), version, settings, deployment()]):
            identity = d.remote_identity(target, self.manifest)
        self.assertEqual(identity["commit"], self.commit)
        changed = deployment()
        changed["deployments"][0]["id"] = "33333333-3333-4333-8333-333333333333"
        with patch.object(d, "serving_identity"), patch.object(d, "cloudflare", side_effect=[deployment(), version, settings, changed]), self.assertRaises(d.DeploymentError):
            d.remote_identity(target, self.manifest)
        for field in ("workers/tag", "workers/message"):
            bad = self.version()
            bad["annotations"][field] = "mismatch"
            settings["annotations"] = version["annotations"]
            with patch.object(d, "cloudflare", side_effect=[deployment(), bad, settings]), self.assertRaises(d.DeploymentError):
                d.remote_identity(target, self.manifest)

    def version(self):
        return {"id": deployment()["deployments"][0]["versions"][0]["version_id"],
                "annotations": {"workers/tag": self.commit, "workers/message": "brews-code-sha256:" + self.manifest["code_sha256"]},
                "resources": {"bindings": bindings(targets()["staging"]),
                              "script_runtime": {"migration_tag": "games", "compatibility_date": d.template()["compatibility_date"]}}}

    def test_preflight_never_provisions(self):
        with patch.object(d, "cloudflare", return_value=[]), self.assertRaises(d.DeploymentError):
            d.preflight(targets()["staging"])
        with patch.object(d, "cloudflare", return_value=[{"id": targets()["staging"]["name"], "migration_tag": "directory"}]), self.assertRaises(d.DeploymentError):
            d.preflight(targets()["staging"])

    def test_cloudflare_failure_envelopes(self):
        for value in ({}, {"success": False, "errors": [], "result": {}}, {"success": True, "errors": [{"message": "withheld"}], "result": {}}):
            with patch.object(d, "get_json", return_value=value), self.assertRaises(d.DeploymentError):
                d.cloudflare("accounts/fixture")

    def test_tls_response_validation(self):
        for status, final_url, content, payload in ((200, "https://api.cloudflare.com/safe", "application/json", b'{"ok":true}'), (302, "https://api.cloudflare.com/safe", "application/json", b'{}'), (200, "https://other.example.com", "application/json", b'{}'), (200, "https://api.cloudflare.com/safe", "text/html", b'{}'), (200, "https://api.cloudflare.com/safe", "application/json", b'{"ok":1,"ok":2}')):
            response = MagicMock()
            response.__enter__.return_value = response
            response.status = status
            response.geturl.return_value = final_url
            response.headers = Message()
            response.headers["Content-Type"] = content
            response.read.return_value = payload
            opener = MagicMock()
            opener.open.return_value = response
            with patch.object(d.request, "build_opener", return_value=opener), patch.object(d.ssl, "create_default_context", wraps=d.ssl.create_default_context) as tls:
                if payload == b'{"ok":true}':
                    self.assertEqual(d.get_json("https://api.cloudflare.com/safe", "ephemeral-unit-value"), {"ok": True})
                else:
                    with self.assertRaises(d.DeploymentError):
                        d.get_json("https://api.cloudflare.com/safe", "ephemeral-unit-value")
                tls.assert_called_once_with()

    def test_no_redirects_or_raw_provider_errors(self):
        with self.assertRaises(d.DeploymentError):
            d.NoRedirect().redirect_request(None, None, 302, "", {}, "https://other.example.com")
        opener = MagicMock()
        opener.open.side_effect = OSError("ephemeral-sensitive-fixture")
        with patch.object(d.request, "build_opener", return_value=opener), self.assertRaisesRegex(d.DeploymentError, "^Provider read unavailable$"):
            d.get_json("https://api.cloudflare.com/safe", "ephemeral-unit-value")
        with self.assertRaises(d.DeploymentError):
            d.get_json("http://api.cloudflare.com/safe", "ephemeral-unit-value")

    def test_dry_run_command_has_no_build_or_token(self):
        with patch.object(d.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, b"", b"")) as run, patch.dict(d.os.environ, {"CLOUDFLARE_API_TOKEN": "ephemeral-unit-value", "BREWS_DEV_CLI_KEY": "ephemeral-unit-value"}):
            d.deploy(self.root, self.artifact, self.manifest, targets(), "staging", True)
        command = run.call_args.args[0]
        self.assertIn("--dry-run", command)
        self.assertIn("--no-bundle", command)
        self.assertNotIn("CLOUDFLARE_API_TOKEN", run.call_args.kwargs["env"])
        self.assertNotIn("BREWS_DEV_CLI_KEY", run.call_args.kwargs["env"])
        self.assertTrue(run.call_args.kwargs["capture_output"])

    def test_real_promotion_uses_versions_and_exact_readback(self):
        identity = {"version_id": "22222222-2222-4222-8222-222222222222"}
        uploaded = subprocess.CompletedProcess([], 0, b"Worker Version ID: 22222222-2222-4222-8222-222222222222\n", b"")
        promoted = subprocess.CompletedProcess([], 0, b"captured output", b"")
        with patch.object(d, "preflight") as preflight, patch.object(d, "remote_identity", return_value=identity), patch.object(d, "cloudflare", return_value=self.version()), patch.object(d.subprocess, "run", side_effect=[uploaded, promoted]) as run, patch.dict(d.os.environ, {"CLOUDFLARE_API_TOKEN": "ephemeral-unit-value"}):
            self.assertEqual(d.deploy(self.root, self.artifact, self.manifest, targets(), "staging", False), identity)
        preflight.assert_called_once_with(targets()["staging"])
        self.assertEqual(run.call_args_list[0].args[0][1:3], ["versions", "upload"])
        self.assertEqual(run.call_args_list[1].args[0][1:4], ["versions", "deploy", identity["version_id"] + "@100%"])
        self.assertNotIn("ephemeral-unit-value", repr(run.call_args_list[0].args[0]))

    def test_freeze_fixture_binds_gate_and_locked_files(self):
        for name in d.ASSETS:
            destination = self.root / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes((self.artifact / name).read_bytes())
        gate_path = self.base / "local-fixture-gate.json"
        d.save(gate_path, self.gate)
        destination = self.base / "new-freeze"
        with patch.object(d, "tool_versions"):
            digest = d.freeze(self.root, destination, gate_path, self.commit)
        manifest = d.verify(self.root, destination, digest, self.commit)
        self.assertEqual(manifest["asset_sha256"], self.assets)
        self.assertEqual(manifest["local_gate_sha256"], d.sha(gate_path.read_bytes()))

    def test_origin_serving_identity_exact_version(self):
        target = targets()["staging"]
        version_id = deployment()["deployments"][0]["versions"][0]["version_id"]
        for identity, valid in (({"version": 1, "version_id": version_id}, True), ({"version": 1, "version_id": "33333333-3333-4333-8333-333333333333"}, False), ({"version": True, "version_id": version_id}, False), ({"version": 1, "version_id": version_id, "extra": "value"}, False)):
            response = MagicMock()
            response.__enter__.return_value = response
            response.status = 200
            response.geturl.return_value = target["origin"] + "/api/deployment"
            response.headers = Message()
            response.headers["Content-Type"] = "application/json"
            response.read.return_value = json.dumps(identity).encode()
            opener = MagicMock()
            opener.open.return_value = response
            with patch.object(d.request, "build_opener", return_value=opener):
                if valid:
                    d.serving_identity(target, version_id)
                else:
                    with self.assertRaises(d.DeploymentError):
                        d.serving_identity(target, version_id)
                req = opener.open.call_args.args[0]
                self.assertFalse(req.has_header("Authorization"))

    def test_workflow_conditions_and_pins(self):
        workflow = (d.ROOT / ".github/workflows/release.yml").read_text()
        validation = (d.ROOT / ".github/workflows/validation.yml").read_text()
        self.assertIn("cancel-in-progress: false", workflow)
        self.assertNotIn("continue-on-error", workflow + validation)
        self.assertNotIn("pull_request_target", workflow + validation)
        self.assertNotIn("secrets.", validation)
        self.assertIn("inputs.commit == github.sha", workflow)
        self.assertIn("environment: production", workflow)
        self.assertIn("needs: [freeze, staging]", workflow)
        self.assertIn("deployment.py protection", workflow)
        self.assertEqual(workflow.count("deployment.py staging-test"), 1)
        import re
        for uses in re.findall(r"uses: ([^\s]+)", workflow + validation):
            self.assertRegex(uses, r"^actions/[a-z-]+@[0-9a-f]{40}$")
        production = workflow.split("  production:\n", 1)[1]
        self.assertNotIn("BREWS_DEV_CLI_KEY", production)
        self.assertNotIn("cargo ", production)
        self.assertNotIn("worker-build", production)

    def summary(self, manual=tuple(range(5)), spectator=True):
        # Fixture independently lists the actual harness checks, without invoking its game/model.
        labels = set('''readiness_anonymous_before_bootstrap genuine_start_presence throwaway_host_enrolled cleanup_own_account_absent
            single_slot_initially_free create_new_standard_board open_lobby start_real_presence
            qualification_without_auto_award winner_commits_terminal_history spectator_post_terminal_denied
            history_exact_outcome_calls_times history_exact_boards history_list_contains_exact_summary
            terminal_reservation_released history_immutable_after_exit account_final_grant_exit_denied
            released_slot_recreate_confirmed_cancel history_immutable_after_successor successor_reservation_released
            all_streams_real_snapshot_ack'''.split())
        if spectator:
            labels.add('spectator_terminal_public_if_delivered')
        for index in range(3):
            labels.add(f'player_{index}_separate_admission_wss')
            labels.add(f'terminal_player_{index}_own_board')
        for index in manual:
            labels.add(f'manual_{index}_accepted')
        for stage in ('start', 'random_call', 'terminal', *(f'manual_{i}' for i in manual)):
            labels.update(stage + suffix for suffix in ('_host_calls', '_host_wss', '_roster_identity'))
            labels.update(f'{stage}_player_{i}_oracle_wss' for i in range(3))
            if stage != 'terminal':
                labels.add(stage + '_spectator_public_wss')
        labels.update(stage + '_receipt_replay_preserves_each_view' for stage in (
            'start', 'random_call', 'winner', *(f'manual_{i}' for i in manual)))
        return {'full_game_e2e': 'passed', 'environment': 'staging', 'checks': sorted(labels),
                'check_count': len(labels), 'cleanup': 'own_account_removed',
                'history_policy': 'retained_until_contract_expiry', 'accepted_call_count': len(manual) + 1,
                'player_count': 3, 'snapshot_ack_count': 5}

    def proof(self):
        summary = self.summary()
        return {'schema': 1, 'run_id': '12345', 'commit': self.commit,
                'manifest_sha256': self.digest, 'archive_sha256': 'c' * 64,
                'inventory_sha256': d.sha(d.canonical(targets())),
                'staging_identity': {'deployment_id': deployment()['deployments'][0]['id'],
                    'version_id': self.version()['id'], 'worker': targets()['staging']['name'],
                    'commit': self.commit, 'code_sha256': self.manifest['code_sha256']},
                'summary': summary, 'summary_sha256': d.sha(d.canonical(summary))}

    def validate_proof(self, proof, inventory=None):
        return d.staging_proof(proof, self.root, self.manifest, self.digest, 'c' * 64,
                               targets() if inventory is None else inventory, '12345')

    def test_selected_version_complete_sdk_bindings_and_annotations(self):
        good = self.version()
        for field in ('bindings', 'script_runtime'):
            bad = copy.deepcopy(good)
            del bad['resources'][field]
            with patch.object(d, 'cloudflare', return_value=bad), self.assertRaises(d.DeploymentError):
                d.selected_version(targets()['staging'], good['id'], self.manifest)
        candidates = []
        bad = copy.deepcopy(good)
        del bad['annotations']
        candidates.append(bad)
        bad = copy.deepcopy(good)
        bad['resources']['bindings'] = bindings(targets()['production'])
        candidates.append(bad)
        bad = copy.deepcopy(good)
        bad['resources']['script_runtime']['compatibility_flags'] = ['unexpected']
        candidates.append(bad)
        for bad in candidates:
            with patch.object(d, 'cloudflare', return_value=bad), self.assertRaises(d.DeploymentError):
                d.selected_version(targets()['staging'], good['id'], self.manifest)
        with patch.object(d, 'cloudflare', return_value=good):
            d.selected_version(targets()['staging'], good['id'], self.manifest)

    def test_wrong_uploaded_version_refuses_before_versions_deploy(self):
        good = self.version()
        for wrong in ('namespaces', 'annotations', 'missing_bindings', 'runtime'):
            bad = copy.deepcopy(good)
            if wrong == 'namespaces':
                bad['resources']['bindings'] = bindings(targets()['production'])
            elif wrong == 'annotations':
                bad['annotations']['workers/tag'] = 'b' * 40
            elif wrong == 'runtime':
                bad['resources']['script_runtime']['migration_tag'] = 'directory'
            else:
                del bad['resources']['bindings']
            uploaded = subprocess.CompletedProcess([], 0, ('Worker Version ID: ' + good['id']).encode(), b'')
            with patch.object(d, 'preflight') as old, patch.object(d, 'cloudflare', return_value=bad), patch.object(d.subprocess, 'run', return_value=uploaded) as run, patch.dict(d.os.environ, {'CLOUDFLARE_API_TOKEN': 'ephemeral-unit-value'}), self.assertRaises(d.DeploymentError):
                d.deploy(self.root, self.artifact, self.manifest, targets(), 'staging', False)
            old.assert_called_once_with(targets()['staging'])
            self.assertEqual(run.call_count, 1)
            self.assertEqual(run.call_args.args[0][1:3], ['versions', 'upload'])

    def test_settings_annotations_never_override_selected_version(self):
        settings = {'bindings': bindings(targets()['staging']), 'annotations': {'workers/tag': 'contradictory'}}
        with patch.object(d, 'serving_identity'), patch.object(d, 'cloudflare', side_effect=[deployment(), self.version(), settings, deployment()]):
            d.remote_identity(targets()['staging'], self.manifest)
        settings['bindings'] = bindings(targets()['production'])
        with patch.object(d, 'cloudflare', side_effect=[deployment(), self.version(), settings]), self.assertRaises(d.DeploymentError):
            d.remote_identity(targets()['staging'], self.manifest)

    def test_game_summary_exact_inventory_all_random_row_cases(self):
        for skipped in (None, *range(5)):
            for spectator in (False, True):
                summary = self.summary(tuple(i for i in range(5) if i != skipped), spectator)
                self.assertEqual(d.game_summary(self.root, summary), summary)

    def test_game_summary_missing_malformed_local_failed_cleanup_counts(self):
        for field, value in (('environment', 'local'), ('cleanup', 'failed_own_resources_may_remain'),
                             ('full_game_e2e', 'failed'), ('player_count', 2), ('accepted_call_count', 4),
                             ('snapshot_ack_count', 4), ('player_count', True), ('check_count', 0)):
            summary = self.summary()
            summary[field] = value
            with self.subTest(field=field), self.assertRaises(d.DeploymentError):
                d.game_summary(self.root, summary)
        for bad in (None, {}, {**self.summary(), 'failure_code': 'failed'}, {**self.summary(), 'boards': []}):
            with self.assertRaises(d.DeploymentError):
                d.game_summary(self.root, bad)
        for change in ('duplicate', 'missing', 'invented'):
            bad = self.summary()
            if change == 'duplicate':
                bad['checks'].append(bad['checks'][0])
            elif change == 'missing':
                bad['checks'].remove('history_exact_boards')
            else:
                bad['checks'].append('synthetic_pass')
            bad['checks'].sort()
            bad['check_count'] = len(bad['checks'])
            with self.assertRaises(d.DeploymentError):
                d.game_summary(self.root, bad)

    def test_staging_proof_binds_run_commit_inventory_manifest_archive_summary(self):
        self.assertEqual(self.validate_proof(self.proof()), self.proof())
        for field in ('run_id', 'commit', 'manifest_sha256', 'archive_sha256', 'inventory_sha256', 'summary_sha256'):
            bad = self.proof()
            bad[field] = 'd' * 64
            with self.subTest(field=field), self.assertRaises(d.DeploymentError):
                self.validate_proof(bad)
        for bad in ({}, {**self.proof(), 'unexpected': True}):
            with self.assertRaises(d.DeploymentError):
                self.validate_proof(bad)
        bad = self.proof()
        bad['staging_identity']['commit'] = 'b' * 40
        with self.assertRaises(d.DeploymentError):
            self.validate_proof(bad)
        changed = targets()
        changed['staging']['namespace_ids']['GAMES'] = 'f' * 32
        with self.assertRaises(d.DeploymentError):
            self.validate_proof(self.proof(), changed)

    def test_proof_file_missing_digest_malformed_and_drifted_source(self):
        path = self.base / 'fixture-proof.json'
        proof = self.proof()
        d.save(path, proof)
        digest = d.sha(path.read_bytes())
        args = (self.root, self.manifest, self.digest, 'c' * 64, targets(), '12345')
        self.assertEqual(d.read_staging_proof(path, digest, *args), proof)
        for bad_path, bad_digest in ((self.base / 'missing', digest), (path, 'f' * 64)):
            with self.assertRaises(d.DeploymentError):
                d.read_staging_proof(bad_path, bad_digest, *args)
        path.write_bytes(b'{invalid')
        with self.assertRaises(ValueError):
            d.read_staging_proof(path, d.sha(path.read_bytes()), *args)
        (self.root / 'backend/source.rs').write_text('changed after testing')
        with self.assertRaises(d.DeploymentError):
            self.validate_proof(proof)

    def test_staging_read_after_approval_detects_version_drift_and_missing_token(self):
        with patch.dict(d.os.environ, {}, clear=True), patch.object(d, 'remote_identity') as read, self.assertRaises(d.DeploymentError):
            d.verify_staging_readback(self.proof(), targets(), self.manifest)
        read.assert_not_called()
        changed = deployment()
        changed['deployments'][0]['id'] = '33333333-3333-4333-8333-333333333333'
        with patch.dict(d.os.environ, {'CLOUDFLARE_API_TOKEN': 'ephemeral-unit-value'}), patch.object(d, 'serving_identity'), patch.object(d, 'cloudflare', side_effect=[changed, self.version(), {'bindings': bindings(targets()['staging'])}, changed]), self.assertRaises(d.DeploymentError):
            d.verify_staging_readback(self.proof(), targets(), self.manifest)

    def test_staging_producer_actual_command_redaction_and_source_recheck(self):
        archive = self.base / 'release.zip'
        d.pack(self.artifact, archive)
        archive_digest = d.sha(archive.read_bytes())
        summary = self.summary()
        identity = self.proof()['staging_identity']
        def actual_fixture(command, **kwargs):
            self.assertIn('--environment', command)
            self.assertEqual(command[command.index('--environment') + 1], 'staging')
            self.assertNotIn('CLOUDFLARE_API_TOKEN', kwargs['env'])
            self.assertNotIn('BREWS_GITHUB_READ_TOKEN', kwargs['env'])
            self.assertTrue(kwargs['capture_output'])
            output = Path(command[command.index('--output') + 1])
            output.mkdir()
            d.save(output / 'summary.json', summary)
            return subprocess.CompletedProcess(command, 0, d.canonical(summary), b'')
        env = {'BREWS_API_ORIGIN': targets()['staging']['origin'], 'BREWS_DEV_CLI_KEY': 'ephemeral-unit-value',
               'CLOUDFLARE_API_TOKEN': 'ephemeral-unit-value', 'BREWS_GITHUB_READ_TOKEN': 'ephemeral-unit-value'}
        # Keep real git/hash/manifest validation while mocking only the external harness process.
        original_run = d.subprocess.run
        def run(command, **kwargs):
            return actual_fixture(command, **kwargs) if command[0] == d.sys.executable else original_run(command, **kwargs)
        with patch.dict(d.os.environ, env), patch.object(d, 'remote_identity', return_value=identity) as read, patch.object(d.subprocess, 'run', side_effect=run):
            output = self.base / 'generated-fixture-proof.json'
            digest = d.staging_test(self.root, self.artifact, self.manifest, self.digest, archive,
                                   archive_digest, targets(), '12345', identity, output)
        self.assertEqual(digest, d.sha(output.read_bytes()))
        self.assertEqual(read.call_count, 2)
        self.assertNotIn('ephemeral-unit-value', output.read_text())
        self.assertNotIn('origin', json.loads(output.read_bytes()))

    def test_staging_producer_never_proves_failed_missing_local_or_changed_execution(self):
        archive = self.base / 'negative-release.zip'
        d.pack(self.artifact, archive)
        archive_digest = d.sha(archive.read_bytes())
        identity = self.proof()['staging_identity']
        source = self.root / 'backend/source.rs'
        original_source = source.read_bytes()
        original_run = d.subprocess.run
        for failure in ('exit', 'missing', 'malformed', 'local', 'cleanup', 'stdout', 'source', 'archive'):
            output = self.base / (failure + '-must-not-exist.json')
            def run(command, **kwargs):
                if command[0] != d.sys.executable:
                    return original_run(command, **kwargs)
                summary = self.summary()
                if failure == 'local':
                    summary['environment'] = 'local'
                if failure == 'cleanup':
                    summary['cleanup'] = 'failed_own_resources_may_remain'
                directory = Path(command[command.index('--output') + 1])
                directory.mkdir()
                if failure != 'missing':
                    (directory / 'summary.json').write_bytes(b'{invalid' if failure == 'malformed' else d.canonical(summary))
                if failure == 'source':
                    source.write_text('changed during actual execution')
                if failure == 'archive':
                    archive.write_bytes(b'changed during actual execution')
                return subprocess.CompletedProcess(command, 1 if failure == 'exit' else 0,
                    b'{}' if failure == 'stdout' else d.canonical(summary), b'')
            with patch.dict(d.os.environ, {'BREWS_API_ORIGIN': targets()['staging']['origin'], 'BREWS_DEV_CLI_KEY': 'ephemeral-unit-value'}), patch.object(d, 'remote_identity', return_value=identity), patch.object(d.subprocess, 'run', side_effect=run), self.subTest(failure=failure), self.assertRaises(Exception):
                d.staging_test(self.root, self.artifact, self.manifest, self.digest, archive,
                               archive_digest, targets(), '12345', identity, output)
            self.assertFalse(output.exists())
            source.write_bytes(original_source)
            if failure == 'archive':
                archive.unlink()
                d.pack(self.artifact, archive)
        with patch.dict(d.os.environ, {'BREWS_API_ORIGIN': 'https://127.0.0.1', 'BREWS_DEV_CLI_KEY': 'ephemeral-unit-value'}), patch.object(d, 'remote_identity') as read, self.assertRaises(d.DeploymentError):
            d.staging_test(self.root, self.artifact, self.manifest, self.digest, archive,
                           d.sha(archive.read_bytes()), targets(), '12345', identity, self.base / 'loopback-proof.json')
        read.assert_not_called()

    def test_local_gate_rejects_old_status_skip_missing_case_and_fake_history(self):
        for mutate in (
            lambda c: c.update(status='passed_observation_not_contract_approval'),
            lambda c: c.update(skipped=1), lambda c: c.update(passed=0),
            lambda c: c['case_inventory'].clear(),
            lambda c: c['superseded_receiving_cut_probe'].update(raw_exit_code=0),
            lambda c: c.pop('superseded_receiving_cut_probe'),
        ):
            gate = copy.deepcopy(self.gate)
            mutate(gate['lanes']['contract'])
            with self.assertRaises(Exception):
                d.local_gate(gate, self.sources, self.assets, self.commit, self.root)

    def protected_main(self):
        return {'enforce_admins': {'enabled': True}, 'allow_force_pushes': {'enabled': False},
                'allow_deletions': {'enabled': False},
                'required_pull_request_reviews': {'required_approving_review_count': 1,
                    'dismiss_stale_reviews': True, 'require_last_push_approval': True},
                'required_status_checks': {'strict': True, 'checks': [{'context': 'validate', 'app_id': 15368}]}}

    def test_main_effective_protection_requires_reviews_checks_admin_enforcement(self):
        branch = {'name': 'main', 'protected': True}
        d.main_protection(branch, self.protected_main())
        for key in self.protected_main():
            bad = self.protected_main()
            del bad[key]
            with self.subTest(key=key), self.assertRaises(d.DeploymentError):
                d.main_protection(branch, bad)
        for field, value in (('required_approving_review_count', 0), ('dismiss_stale_reviews', False),
                             ('require_last_push_approval', False), ('bypass_pull_request_allowances', {'users': [{'id': 1}]})):
            bad = self.protected_main()
            bad['required_pull_request_reviews'][field] = value
            with self.assertRaises(d.DeploymentError):
                d.main_protection(branch, bad)
        for checks in ({'strict': False, 'checks': [{'context': 'validate', 'app_id': 15368}]},
                       {'strict': True, 'checks': []}, {'strict': True, 'checks': [{'context': 'validate', 'app_id': -1}]}):
            bad = self.protected_main()
            bad['required_status_checks'] = checks
            with self.assertRaises(d.DeploymentError):
                d.main_protection(branch, bad)
        with self.assertRaises(d.DeploymentError):
            d.main_protection({'name': 'main', 'protected': False}, self.protected_main())

    def test_protection_reads_effective_main_and_rejects_unresolved(self):
        environment = {'name': 'production', 'can_admins_bypass': False,
                       'protection_rules': [{'type': 'required_reviewers', 'prevent_self_review': True,
                                             'reviewers': [{'type': 'User', 'reviewer': {'id': 1}}]}],
                       'deployment_branch_policy': {'protected_branches': True, 'custom_branch_policies': False}}
        branch = {'name': 'main', 'protected': True, 'commit': {'sha': self.commit}}
        with patch.dict(d.os.environ, {'GITHUB_REPOSITORY': 'fixture/repo', 'RELEASE_COMMIT': self.commit}), patch.object(d, 'get_json', side_effect=[environment, branch, self.protected_main()]) as read:
            d.protection()
        self.assertEqual(read.call_count, 3)
        self.assertTrue(read.call_args_list[-1].args[0].endswith('/branches/main/protection'))
        with patch.dict(d.os.environ, {'GITHUB_REPOSITORY': 'fixture/repo', 'RELEASE_COMMIT': self.commit}), patch.object(d, 'get_json', side_effect=[environment, branch, d.DeploymentError('unresolved ruleset/plan')]), self.assertRaises(d.DeploymentError):
            d.protection()

    def test_workflow_proof_readonly_order_same_run_and_mold(self):
        release = (d.ROOT / '.github/workflows/release.yml').read_text()
        validation = (d.ROOT / '.github/workflows/validation.yml').read_text()
        for job in (release.split('  staging:', 1)[0], validation):
            self.assertLess(job.index('apt-get install --yes mold'), job.index('cargo '))
            self.assertIn('cc -fuse-ld=mold', job)
        production = release.split('  production:', 1)[1]
        self.assertIn('PROOF_SHA256: ${{ needs.staging.outputs.proof }}', production)
        self.assertLess(production.index('deployment.py verify-staging '), production.index('deployment.py deploy '))
        read = production.split('      - name: Recheck tested staging', 1)[1].split('      - name:', 1)[0]
        deploy = production.split('      - name: Promote frozen code', 1)[1].split('      - name:', 1)[0]
        self.assertIn('CLOUDFLARE_API_TOKEN: ${{ secrets.STAGING_READ_ONLY_CLOUDFLARE_API_TOKEN }}', read)
        self.assertNotIn('secrets.CLOUDFLARE_API_TOKEN', read)
        self.assertIn('CLOUDFLARE_API_TOKEN: ${{ secrets.CLOUDFLARE_API_TOKEN }}', deploy)
        self.assertNotIn('STAGING_READ_ONLY', deploy)
        self.assertEqual(production.count('secrets.STAGING_READ_ONLY_CLOUDFLARE_API_TOKEN'), 1)
        for step in (read, deploy):
            self.assertIn('--proof-sha256 "$PROOF_SHA256"', step)
            self.assertIn('--run-id "${{ github.run_id }}"', step)
            self.assertIn('--archive-sha256 "$ARCHIVE_SHA256"', step)
        self.assertIn('name: staging-proof-${{ github.run_id }}', production)
        self.assertNotIn('          run-id:', production)
        self.assertNotIn('          github-token:', production)


if __name__ == "__main__":
    unittest.main()
