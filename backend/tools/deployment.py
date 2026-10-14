"""Frozen local release artifacts and fail-closed, explicitly invoked promotion."""
from __future__ import annotations

import argparse
import ast
import importlib.util
from datetime import datetime, timezone
import hashlib
import ipaddress
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import ssl
import subprocess
import sys
import tempfile
import tomllib
from urllib import request
import uuid
import zipfile

ROOT = Path(__file__).resolve().parents[2]
CLASSES = {"ACCOUNTS": "AccountsObject", "GAME_DIRECTORY": "GameDirectoryObject", "GAMES": "GameObject"}
ASSETS = ("backend/build/index.js", "backend/build/index_bg.wasm", "backend/build/package.json", "backend/build/worker/shim.mjs", "target/debug/brews", "target/wasm32-unknown-unknown/release/brews_backend.wasm")
TOOLS = {"rust": "1.99.0", "wrangler": "4.147.0", "worker_build": "0.8.7", "nextest": "0.9.140", "python": "3.14", "node": "22.16.0"}
GATES = ("argon2_target", "unicode_clock_csprng_sdk", "owner_forecast_max_game", "quotas_two_times_headroom", "history_query_cost", "copies_retention_restore", "isolation_secrets_bootstrap", "deployed_security_load_operations")
MAX_JSON = 2 * 1024 * 1024
MAX_ARCHIVE = 128 * 1024 * 1024


class DeploymentError(Exception):
    """A required release invariant was not established; messages contain no inputs."""


def require(condition: bool, message: str) -> None:
    if not condition:
        raise DeploymentError(message)


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def hex_value(value: object, length: int) -> str:
    require(isinstance(value, str) and re.fullmatch(f"[0-9a-f]{{{length}}}", value) is not None, "Invalid digest or identifier")
    return value


def canonical(value: object) -> bytes:
    return (json.dumps(value, sort_keys=True, indent=2) + "\n").encode()


def json_bytes(data: bytes) -> object:
    require(len(data) <= MAX_JSON, "Response too large")
    def unique(pairs):
        result = {}
        for key, value in pairs:
            require(key not in result, "Duplicate JSON key")
            result[key] = value
        return result
    return json.loads(data, object_pairs_hook=unique)


def save(path: Path, value: object) -> None:
    with path.open("xb") as output:
        output.write(canonical(value))
    path.chmod(0o600)


def origin(value: object) -> str:
    require(isinstance(value, str) and value == value.strip() and all(ord(char) > 32 and ord(char) < 127 for char in value), "Missing or malformed HTTPS origin")
    from urllib.parse import urlsplit
    parts = urlsplit(value)
    host = parts.hostname or ""
    require(parts.scheme == "https" and parts.netloc == host and not parts.path and not parts.query and not parts.fragment and not parts.username, "Unsafe HTTPS origin")
    require(re.fullmatch(r"[a-z0-9](?:[a-z0-9.-]*[a-z0-9])?", host) is not None and "." in host and not any(host.endswith(suffix) for suffix in (".invalid", ".localhost", ".local", ".test")), "Unprovisioned origin")
    require(all(label and len(label) <= 63 and not label.startswith("-") and not label.endswith("-") for label in host.split(".")), "Invalid origin hostname")
    try:
        ipaddress.ip_address(host)
    except ValueError:
        return value
    raise DeploymentError("IP origins forbidden")


def inventory(value: object) -> dict:
    require(isinstance(value, dict) and set(value) == {"staging", "production"}, "Both environment inventories required")
    namespaces, references, origins, names = set(), set(), set(), set()
    for environment in ("staging", "production"):
        target = value[environment]
        require(isinstance(target, dict) and set(target) == {"name", "account_id", "origin", "namespace_ids", "rate_secret_ref", "dev_secret_ref"}, "Unexpected inventory fields")
        require(target["name"] == f"brews-bingo-api-{environment}", "Wrong environment worker name")
        hex_value(target["account_id"], 32)
        current_origin = origin(target["origin"])
        require(current_origin not in origins and target["name"] not in names, "Shared environment origin or name")
        origins.add(current_origin)
        names.add(target["name"])
        require(isinstance(target["namespace_ids"], dict) and set(target["namespace_ids"]) == set(CLASSES), "Incomplete namespace inventory")
        for namespace in target["namespace_ids"].values():
            hex_value(namespace, 32)
            require(namespace not in namespaces, "Shared namespace")
            namespaces.add(namespace)
        for field in ("rate_secret_ref", "dev_secret_ref"):
            reference = target[field]
            require(isinstance(reference, str) and re.fullmatch(environment + r"/[a-zA-Z0-9_-]{1,64}", reference) is not None and reference not in references, "Shared or wrong environment secret reference")
            references.add(reference)
    return value


def public_inventory() -> dict:
    return inventory(json_bytes(os.environ.get("BREWS_DEPLOYMENT_INVENTORY", "").encode()))


def template(root: Path = ROOT) -> dict:
    value = tomllib.loads((root / "backend/wrangler.toml").read_text())
    require(value["name"] == "brews-bingo-api" and value["main"] == "build/worker/shim.mjs" and value["workers_dev"] is False, "Unexpected local template")
    expected_migrations = [{"tag": tag, "new_sqlite_classes": [class_name]} for tag, class_name in zip(("accounts", "directory", "games"), CLASSES.values())]
    require(value["migrations"] == expected_migrations, "DO lifecycle change needs separate authorization")
    for environment in ("staging", "production"):
        remote = value["env"][environment]
        require(set(remote) == {"name", "workers_dev", "preview_urls", "routes", "vars", "durable_objects", "version_metadata"}, "Unexpected environment settings")
        require(remote["name"] == f"brews-bingo-api-{environment}" and remote["workers_dev"] is False and remote["preview_urls"] is False and remote["routes"] == [], "Unsafe remote template")
        require(remote["vars"] == {"APP_ORIGIN": f"https://brews-bingo-{environment}.invalid"}, "Unsafe default origin")
        require(remote["version_metadata"] == {"binding": "CF_VERSION_METADATA"}, "Missing version metadata binding")
        bindings = remote["durable_objects"]["bindings"]
        require(bindings == [{"name": name, "class_name": cls} for name, cls in CLASSES.items()], "Wrong or cross-linked DO bindings")
    return value


def git(root: Path, *arguments: str) -> str:
    result = subprocess.run(["git", "-c", f"safe.directory={root}", "-C", str(root), *arguments], capture_output=True, check=True)
    return result.stdout.decode().strip()


def source_names(root: Path) -> list[str]:
    names = git(root, "ls-files", "-z").split("\0")
    fixed = {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo/config.toml",
             "AGENTS.md", "docs/rust-best-practices.md", "docs/operations/cicd.md"}
    return sorted(name for name in names if name and (name in fixed or (name.split("/")[0] in {"backend", "cli", "shared"} and Path(name).suffix in {".rs", ".py", ".js", ".mjs", ".toml", ".json", ".sql"}) or name.startswith(".github/") and Path(name).suffix in {".yml", ".yaml", ".json", ".toml", ".md", ".py", ".js", ".mjs"} or name.startswith("docs/reviews/evidence/command-lookup-final-return.") or name.startswith("docs/plans/") and name.endswith(".md")) and not any(part in {"node_modules", "build", "target", "__pycache__"} or part.startswith(".env") or part.startswith(".dev.vars") for part in PurePosixPath(name).parts))


def safe_file(root: Path, name: str) -> Path:
    parts = PurePosixPath(name)
    require(not parts.is_absolute() and ".." not in parts.parts and "\\" not in name and str(parts) == name, "Unsafe artifact path")
    path = root / name
    require(root.resolve() in path.resolve().parents and path.is_file() and not any((root / Path(*parts.parts[:index])).is_symlink() for index in range(1, len(parts.parts) + 1)), "Unsafe or absent release file")
    return path


def hashes(root: Path, names: list[str] | tuple[str, ...]) -> dict:
    return {name: sha(safe_file(root, name).read_bytes()) for name in names}


def local_gate(value: dict, sources: dict, assets: dict, commit: str, root: Path = ROOT) -> None:
    require(value.get("baseline_commit") == commit and value.get("source_unchanged") is True and value.get("artifact_unchanged") is True and type(value.get("local_execution_exit_code")) is int and value["local_execution_exit_code"] == 0 and value.get("requested_lanes") == ["native", "worker", "tls", "contract"] and "execution_error" not in value, "Local release gate did not pass")
    snapshot_contract(root, value["lanes"]["contract"])
    require(value.get("source_sha256") and all(sources.get(name) == digest for name, digest in value["source_sha256"].items()), "Local gate source drift")
    require(value.get("worker_artifact_sha256") and all(assets.get(name) == digest for name, digest in value["worker_artifact_sha256"].items()), "Local gate artifact drift")


def snapshot_contract(root: Path, contract: dict) -> None:
    # Reuse the runner's exact policy, case inventory and historical classification.
    spec = importlib.util.spec_from_file_location("frozen_release_gate", root / "backend/tools/release_gate.py")
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    runner.validate_snapshot_contract(contract)

    require(contract["status"] == "passed_authorization_time_snapshot" and contract["passed"] > 0
            and contract["failed"] == 0 and contract["skipped"] == 0 and contract["raw_exit_code"] == 0,
            "Approved snapshot acceptance did not pass")


def tool_versions() -> None:
    require(sys.version_info[:2] == (3, 14), "Python version mismatch")
    checks = ((["cargo", "--version"], r"cargo 1\.99\.0 "),
              (["cargo", "nextest", "--version"], r"cargo-nextest 0\.9\.140 "),
              (["worker-build", "--version"], r"(?:worker-build )?0\.8\.7(?:\s|$)"),
              (["node", "--version"], r"v22\.16\.0(?:\s|$)"))
    for command, pattern in checks:
        result = subprocess.run(command, capture_output=True, timeout=30)
        require(result.returncode == 0 and re.match(pattern, result.stdout.decode()) is not None, "Installed tool version mismatch")


def freeze(root: Path, output: Path, gate_path: Path, commit: str) -> str:
    hex_value(commit, 40)
    require(git(root, "rev-parse", "HEAD") == commit and not git(root, "status", "--porcelain", "--untracked-files=normal"), "Freeze requires clean requested commit")
    template(root)
    tool_versions()
    require(tomllib.loads((root / "rust-toolchain.toml").read_text())["toolchain"]["channel"] == TOOLS["rust"] and json.loads((root / "backend/package.json").read_text())["devDependencies"]["wrangler"] == TOOLS["wrangler"], "Toolchain lock mismatch")
    sources, assets = hashes(root, source_names(root)), hashes(root, ASSETS)
    gate = json_bytes(gate_path.read_bytes())
    local_gate(gate, sources, assets, commit, root)
    require(output.resolve() != root.resolve() and root.resolve() not in output.resolve().parents, "Release output must be outside repository")
    output.mkdir(mode=0o700, exist_ok=False)
    for name in ASSETS:
        destination = output / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(safe_file(root, name), destination)
        destination.chmod(0o500 if name.endswith("/brews") else 0o400)
    manifest = {"schema": 1, "commit": commit, "tools": TOOLS, "source_sha256": sources, "asset_sha256": assets, "local_gate_sha256": sha(gate_path.read_bytes()), "code_sha256": sha(canonical(assets)), "probe_inputs": [name for name in sources if "final-return" in name or name.startswith("backend/tests/")], "local_gate": gate}
    save(output / "manifest.json", manifest)
    return sha((output / "manifest.json").read_bytes())


def verify(root: Path, artifact: Path, digest: str, commit: str) -> dict:
    hex_value(digest, 64)
    hex_value(commit, 40)
    manifest_path = safe_file(artifact, "manifest.json")
    require(sha(manifest_path.read_bytes()) == digest, "Manifest digest mismatch")
    value = json_bytes(manifest_path.read_bytes())
    require(set(value) == {"schema", "commit", "tools", "source_sha256", "asset_sha256", "local_gate_sha256", "code_sha256", "probe_inputs", "local_gate"} and value["schema"] == 1 and value["commit"] == commit and value["tools"] == TOOLS, "Manifest identity mismatch")
    require(value["source_sha256"] == hashes(root, source_names(root)) and git(root, "rev-parse", "HEAD") == commit, "Source drift")
    require(set(value["asset_sha256"]) == set(ASSETS) and value["asset_sha256"] == hashes(artifact, ASSETS), "Frozen artifact drift")
    require(value["code_sha256"] == sha(canonical(value["asset_sha256"])) and value["local_gate_sha256"] == sha(canonical(value["local_gate"])), "Manifest evidence mismatch")
    local_gate(value["local_gate"], value["source_sha256"], value["asset_sha256"], commit, root)
    files = {p.relative_to(artifact).as_posix() for p in artifact.rglob("*") if p.is_file()}
    require(files == {*ASSETS, "manifest.json"}, "Unexpected archive files")
    return value


def pack(artifact: Path, archive: Path) -> None:
    with zipfile.ZipFile(archive, "x", compression=zipfile.ZIP_DEFLATED) as bundle:
        for name in ("manifest.json", *ASSETS):
            bundle.write(safe_file(artifact, name), name)
    archive.chmod(0o600)


def unpack(archive: Path, output: Path) -> None:
    require(archive.stat().st_size <= MAX_ARCHIVE, "Archive too large")
    with zipfile.ZipFile(archive) as bundle:
        members = bundle.infolist()
        require({m.filename for m in members} == {"manifest.json", *ASSETS} and len(members) == len(ASSETS) + 1 and sum(m.file_size for m in members) <= MAX_ARCHIVE, "Unexpected archive inventory")
        require(all(not m.is_dir() and ((m.external_attr >> 16) & 0o170000) in (0, 0o100000) for m in members), "Archive links forbidden")
        output.mkdir(mode=0o700, exist_ok=False)
        for member in members:
            destination = output / member.filename
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(bundle.read(member))
            destination.chmod(0o500 if member.filename.endswith("/brews") else 0o400)


def release_config(root: Path, artifact: Path, value: dict, targets: dict, environment: str) -> dict:
    require(environment in targets, "Unknown environment")
    base = template(root)
    target = inventory(targets)[environment]
    bindings = [{"name": name, "class_name": cls} for name, cls in CLASSES.items()]
    return {"name": target["name"], "account_id": target["account_id"], "main": str(safe_file(artifact, "backend/build/worker/shim.mjs").resolve()), "base_dir": str((artifact / "backend/build").resolve()), "compatibility_date": base["compatibility_date"], "compatibility_flags": base.get("compatibility_flags", []), "workers_dev": False, "preview_urls": False, "routes": [], "keep_vars": True, "no_bundle": True, "find_additional_modules": True, "rules": [{"type": "ESModule", "globs": ["**/*.js", "**/*.mjs"], "fallthrough": True}, {"type": "CompiledWasm", "globs": ["**/*.wasm"], "fallthrough": True}], "version_metadata": {"binding": "CF_VERSION_METADATA"}, "vars": {"APP_ORIGIN": target["origin"]}, "durable_objects": {"bindings": bindings}, "migrations": base["migrations"]}


class NoRedirect(request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise DeploymentError("API redirect forbidden")


def get_json(url: str, token: str | None = None) -> dict:
    from urllib.parse import urlsplit
    parts = urlsplit(url)
    host = parts.hostname
    require(parts.netloc == host and parts.scheme == "https" and not parts.fragment and host in {"api.cloudflare.com", "api.github.com"} and token, "Unsafe or unauthenticated API request")
    headers = {"Accept": "application/json", "Authorization": f"Bearer {token}"}
    if host == "api.github.com":
        headers["X-GitHub-Api-Version"] = "2022-11-28"
    opener = request.build_opener(NoRedirect(), request.HTTPSHandler(context=ssl.create_default_context()))
    try:
        with opener.open(request.Request(url, headers=headers), timeout=30) as response:
            require(response.status == 200 and response.geturl() == url and response.headers.get_content_type() == "application/json", "Invalid API response")
            value = json_bytes(response.read(MAX_JSON + 1))
            require(isinstance(value, dict), "Malformed API envelope")
            return value
    except DeploymentError:
        raise
    except Exception:
        raise DeploymentError("Provider read unavailable") from None


def cloudflare(path: str) -> object:
    value = get_json("https://api.cloudflare.com/client/v4/" + path, os.environ.get("CLOUDFLARE_API_TOKEN"))
    require(value.get("success") is True and value.get("errors") == [] and "result" in value, "Cloudflare operation failed")
    return value["result"]


def uuid_value(value: object) -> str:
    require(isinstance(value, str) and str(uuid.UUID(value)) == value and uuid.UUID(value).int != 0, "Invalid deployment identity")
    return value


def active_deployment(value: object) -> tuple[str, str]:
    require(isinstance(value, dict) and isinstance(value.get("deployments"), list) and value["deployments"], "No active deployment")
    deployments = value["deployments"]
    times = [datetime.fromisoformat(item["created_on"].replace("Z", "+00:00")) for item in deployments]
    require(all(time.tzinfo for time in times) and len(times) == len(set(times)), "Ambiguous deployment ordering")
    current = deployments[times.index(max(times))]
    require(current["strategy"] == "percentage" and len(current["versions"]) == 1 and type(current["versions"][0]["percentage"]) in (int, float) and current["versions"][0]["percentage"] == 100, "Partial deployment forbidden")
    return uuid_value(current["id"]), uuid_value(current["versions"][0]["version_id"])


def remote_bindings(bindings: object, target: dict) -> None:
    require(isinstance(bindings, list) and all(isinstance(b, dict) and isinstance(b.get("name"), str) for b in bindings), "Missing remote bindings")
    mapped = {b["name"]: b for b in bindings}
    require(len(mapped) == len(bindings) and set(mapped) == {*CLASSES, "APP_ORIGIN", "RATE_LIMIT_KEY", "DEV_CLI_KEY", "CF_VERSION_METADATA"}, "Duplicate or unexpected remote bindings")
    for name, cls in CLASSES.items():
        binding = mapped[name]
        require(binding.get("type") == "durable_object_namespace" and binding.get("class_name") == cls and binding.get("namespace_id") == target["namespace_ids"][name] and binding.get("script_name") in (None, target["name"]) and not binding.get("environment") and not binding.get("dispatch_namespace"), "Wrong remote DO namespace")
    require(mapped["CF_VERSION_METADATA"].get("type") == "version_metadata", "Missing serving identity binding")
    require(mapped["APP_ORIGIN"].get("type") == "plain_text" and mapped["APP_ORIGIN"].get("text") == target["origin"], "Wrong remote origin")
    require(all(mapped[name].get("type") == "secret_text" for name in ("RATE_LIMIT_KEY", "DEV_CLI_KEY")), "Required secrets unprovisioned")


def selected_version(target: dict, version_id: str, value: dict | None = None) -> None:
    prefix = f"accounts/{target['account_id']}/workers/scripts/{target['name']}"
    version = cloudflare(prefix + "/versions/" + uuid_value(version_id))
    require(isinstance(version, dict) and version.get("id") == version_id, "Wrong version response")
    resources = version.get("resources", {})
    runtime = resources.get("script_runtime", {})
    require(runtime.get("migration_tag") == "games", "Remote DO lifecycle mismatch")
    configured = template()
    require(runtime.get("compatibility_date") == configured["compatibility_date"]
            and runtime.get("compatibility_flags", []) == configured.get("compatibility_flags", []),
            "Remote compatibility settings mismatch")
    remote_bindings(resources.get("bindings"), target)
    if value:
        # Pinned Wrangler reads GET version.annotations, never settings annotations.
        annotations = version.get("annotations", {})
        require(annotations.get("workers/tag") == value["commit"]
                and annotations.get("workers/message") == "brews-code-sha256:" + value["code_sha256"],
                "Remote commit or code digest mismatch")


def remote_identity(target: dict, value: dict | None = None, expected: dict | None = None) -> dict:
    prefix = f"accounts/{target['account_id']}/workers/scripts/{target['name']}"
    deployment_id, version_id = active_deployment(cloudflare(prefix + "/deployments"))
    selected_version(target, version_id, value)
    settings = cloudflare(prefix + "/settings")
    remote_bindings(settings.get("bindings"), target)
    result = {"deployment_id": deployment_id, "version_id": version_id, "worker": target["name"], "commit": value["commit"] if value else None, "code_sha256": value["code_sha256"] if value else None}
    if value:
        serving_identity(target, version_id)
    require(active_deployment(cloudflare(prefix + "/deployments")) == (deployment_id, version_id), "Deployment changed during readback")
    if expected is not None:
        require(result == expected, "Remote deployment changed during verification")
    return result


def serving_identity(target: dict, version_id: str) -> None:
    url = origin(target["origin"]) + "/api/deployment"
    opener = request.build_opener(NoRedirect(), request.HTTPSHandler(context=ssl.create_default_context()))
    try:
        with opener.open(request.Request(url, method="GET", headers={"Accept": "application/json"}), timeout=30) as response:
            require(response.status == 200 and response.geturl() == url and response.headers.get_content_type() == "application/json", "Invalid serving identity response")
            identity = json_bytes(response.read(MAX_JSON + 1))
            require(isinstance(identity, dict) and set(identity) == {"version", "version_id"} and type(identity["version"]) is int and identity["version"] == 1 and uuid_value(identity["version_id"]) == version_id, "Origin serves a different Worker version")
    except DeploymentError:
        raise
    except Exception:
        raise DeploymentError("Serving identity unavailable") from None


def preflight(target: dict) -> None:
    scripts = cloudflare(f"accounts/{target['account_id']}/workers/scripts")
    matches = [item for item in scripts if item.get("id") == target["name"]]
    require(len(matches) == 1 and matches[0].get("migration_tag") == "games", "First provisioning or migration change forbidden")
    remote_identity(target)


def readiness(value: object, manifest: dict, manifest_digest: str, targets: dict) -> None:
    require(isinstance(value, dict) and set(value) == {"schema", "commit", "manifest_sha256", "inventory_sha256", "reviewed_by", "expires_at", "runtime_final_01", "slice_reviews", "autodeploy_disabled", "gates"}, "Incomplete reviewed readiness manifest")
    require(value["schema"] == 1 and value["commit"] == manifest["commit"] and value["manifest_sha256"] == manifest_digest and value["inventory_sha256"] == sha(canonical(targets)), "Readiness scope mismatch")
    expires = datetime.fromisoformat(value["expires_at"].replace("Z", "+00:00"))
    require(expires.tzinfo is not None and expires > datetime.now(timezone.utc) and isinstance(value["reviewed_by"], str) and re.fullmatch(r"[a-zA-Z0-9_-]{1,64}", value["reviewed_by"]) is not None, "Missing or expired review")
    require(value["autodeploy_disabled"] is True and set(value["gates"]) == set(GATES), "Parallel autodeploy or incomplete gates")
    for gate in [value["runtime_final_01"], value["slice_reviews"], *value["gates"].values()]:
        require(isinstance(gate, dict) and set(gate) == {"status", "evidence_sha256"} and gate["status"] == "passed", "External gate remains open")
        hex_value(gate["evidence_sha256"], 64)


def manual_protection(value: dict) -> None:
    require(value.get("name") == "production" and value.get("can_admins_bypass") is False, "Production environment can bypass approval")
    rules = value.get("protection_rules")
    require(isinstance(rules, list), "Missing manual protection")
    reviewers = [rule for rule in rules if rule.get("type") == "required_reviewers"]
    require(len(reviewers) == 1 and reviewers[0].get("prevent_self_review") is True and reviewers[0].get("reviewers") and all(item.get("type") in {"User", "Team"} and type(item.get("reviewer", {}).get("id")) is int and item["reviewer"]["id"] > 0 for item in reviewers[0]["reviewers"]), "Required independent reviewers missing")
    require(value.get("deployment_branch_policy") == {"protected_branches": True, "custom_branch_policies": False}, "Protected branch restriction missing")


def main_protection(branch: dict, protected: dict) -> None:
    require(branch.get("name") == "main" and branch.get("protected") is True,
            "Main is not protected")
    require(protected.get("enforce_admins", {}).get("enabled") is True
            and protected.get("allow_force_pushes", {}).get("enabled") is False
            and protected.get("allow_deletions", {}).get("enabled") is False,
            "Main administrator or rewrite enforcement missing")
    reviews = protected.get("required_pull_request_reviews", {})
    require(type(reviews.get("required_approving_review_count")) is int
            and reviews["required_approving_review_count"] >= 1
            and reviews.get("dismiss_stale_reviews") is True
            and reviews.get("require_last_push_approval") is True,
            "Main independent review enforcement missing")
    bypass = reviews.get("bypass_pull_request_allowances", {})
    require(isinstance(bypass, dict) and not any(bypass.values()), "Main review bypass exists")
    checks = protected.get("required_status_checks", {})
    require(checks.get("strict") is True and isinstance(checks.get("checks"), list)
            and any(c.get("context") == "validate" and type(c.get("app_id")) is int
                    and c["app_id"] > 0 for c in checks["checks"]),
            "Main local validation status enforcement missing")


def protection() -> None:
    repository = os.environ.get("GITHUB_REPOSITORY", "")
    require(re.fullmatch(r"[a-zA-Z0-9_.-]+/[a-zA-Z0-9_.-]+", repository) is not None, "Missing GitHub repository")
    token = os.environ.get("BREWS_GITHUB_READ_TOKEN")
    prefix = f"https://api.github.com/repos/{repository}"
    manual_protection(get_json(prefix + "/environments/production", token))
    branch = get_json(prefix + "/branches/main", token)
    main_protection(branch, get_json(prefix + "/branches/main/protection", token))
    commit = os.environ.get("RELEASE_COMMIT")
    hex_value(commit, 40)
    require(branch.get("commit", {}).get("sha") == commit, "Main changed since requested release")


def deploy(root: Path, artifact: Path, value: dict, targets: dict, environment: str, dry_run: bool) -> dict | None:
    if not dry_run:
        preflight(targets[environment])
    with tempfile.TemporaryDirectory(prefix="brews-config-") as scratch:
        config = Path(scratch) / "wrangler.json"
        save(config, release_config(root, artifact, value, targets, environment))
        wrangler = str(root / "backend/node_modules/.bin/wrangler")
        installed = json.loads((root / "backend/node_modules/wrangler/package.json").read_bytes())
        require(installed.get("version") == TOOLS["wrangler"], "Installed Wrangler version mismatch")
        operation = ["deploy"] if dry_run else ["versions", "upload"]
        command = [wrangler, *operation, "--config", str(config), "--no-bundle", "--keep-vars", "--tag", value["commit"], "--message", "brews-code-sha256:" + value["code_sha256"]]
        if dry_run:
            command.extend(["--dry-run", "--outdir", str(Path(scratch) / "out")])
        child_env = {k: v for k, v in os.environ.items() if not k.startswith(("BREWS_", "WRANGLER_", "CLOUDFLARE_", "CF_"))}
        child_env.update({"CI": "true", "WRANGLER_SEND_METRICS": "false", "WRANGLER_LOG_PATH": str(Path(scratch) / "private-logs")})
        if not dry_run:
            require(os.environ.get("CLOUDFLARE_API_TOKEN"), "Missing deployment credential")
            child_env["CLOUDFLARE_API_TOKEN"] = os.environ["CLOUDFLARE_API_TOKEN"]
        result = subprocess.run(command, cwd=scratch, env=child_env, capture_output=True, timeout=300)
        require(result.returncode == 0, "Wrangler deployment failed; raw output withheld")
        if not dry_run:
            versions = re.findall(rb"Worker Version ID:\s*([0-9a-f-]{36})", result.stdout)
            require(len(versions) == 1, "Wrangler version identity missing")
            version_id = uuid_value(versions[0].decode())
            selected_version(targets[environment], version_id, value)
            promotion = subprocess.run([wrangler, "versions", "deploy", version_id + "@100%", "--yes", "--config", str(config)], cwd=scratch, env=child_env, capture_output=True, timeout=300)
            require(promotion.returncode == 0, "Version promotion failed; raw output withheld")
            identity = remote_identity(targets[environment], value)
            require(identity["version_id"] == versions[0].decode(), "Deployment readback differs from uploaded version")
            return identity
    return None


def check_label(expression: ast.AST, values: dict) -> str:
    if isinstance(expression, ast.Constant) and isinstance(expression.value, str):
        return expression.value
    if isinstance(expression, ast.Name) and expression.id in values:
        return values[expression.id]
    if isinstance(expression, ast.BinOp) and isinstance(expression.op, ast.Add):
        return check_label(expression.left, values) + check_label(expression.right, values)
    if (isinstance(expression, ast.Call) and isinstance(expression.func, ast.Name)
            and expression.func.id == "str" and len(expression.args) == 1 and not expression.keywords):
        return str(check_label(expression.args[0], values))
    raise DeploymentError("Unrecognized frozen harness check expression")


def game_summary(root: Path, value: object) -> dict:
    fields = {"full_game_e2e", "environment", "checks", "check_count", "cleanup", "history_policy",
              "accepted_call_count", "player_count", "snapshot_ack_count"}
    require(isinstance(value, dict) and set(value) == fields and value["full_game_e2e"] == "passed"
            and value["environment"] == "staging" and value["cleanup"] == "own_account_removed"
            and value["history_policy"] == "retained_until_contract_expiry", "Actual staging game did not pass cleanup")
    labels = value["checks"]
    require(isinstance(labels, list) and all(isinstance(label, str) for label in labels)
            and labels == sorted(set(labels)) and type(value["check_count"]) is int
            and value["check_count"] == len(labels) > 0, "Game check inventory missing or duplicated")
    require(type(value["player_count"]) is int and value["player_count"] == 3
            and type(value["accepted_call_count"]) is int and value["accepted_call_count"] in (5, 6)
            and type(value["snapshot_ack_count"]) is int and 5 <= value["snapshot_ack_count"] <= 2**53 - 1,
            "Game player/call/ack counts invalid")
    manual = [i for i in range(5) if f"manual_{i}_accepted" in labels]
    require(len(manual) == value["accepted_call_count"] - 1, "Game manual call count inconsistent")
    # Read only label expressions from the verified frozen source; never execute a model/game.
    tree = ast.parse((root / "backend/tests/e2e_support/scenario.py").read_bytes())
    scenario = next(node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == "Scenario")
    methods = {node.name: node for node in scenario.body if isinstance(node, ast.FunctionDef)}
    def expressions(method):
        return [node.args[1] for node in ast.walk(methods[method])
                if isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute)
                and isinstance(node.func.value, ast.Name) and node.func.value.id == "self"
                and node.func.attr == "check" and len(node.args) == 2]
    expected = set()
    for method in ("readiness", "poll_presence", "run", "cleanup"):
        for expression in expressions(method):
            if isinstance(expression, ast.Constant):
                expected.add(check_label(expression, {}))
            else:
                contexts = manual if "manual_" in ast.unparse(expression) else range(3)
                for index in contexts:
                    expected.add(check_label(expression, {"index": index}))
    optional = "spectator_terminal_public_if_delivered"
    require(optional in expected, "Frozen harness optional check changed")
    if optional not in labels:
        expected.remove(optional)
    for stage in ("start", "random_call", "terminal", *(f"manual_{i}" for i in manual)):
        for expression in expressions("assert_model"):
            if stage == "terminal" and "_spectator_public_wss" in ast.unparse(expression):
                continue
            for index in range(3):
                expected.add(check_label(expression, {"stage": stage, "index": index}))
    for stage in ("start", "random_call", "winner", *(f"manual_{i}" for i in manual)):
        for expression in expressions("verify_replay"):
            expected.add(check_label(expression, {"stage": stage}))
    require(set(labels) == expected, "Actual game check inventory differs from frozen harness")
    return value


def staging_proof(value: object, root: Path, manifest: dict, manifest_digest: str,
                  archive_digest: str, targets: dict, run_id: str) -> dict:
    require(isinstance(run_id, str) and re.fullmatch(r"[1-9][0-9]{0,19}", run_id) is not None,
            "Missing trusted workflow run identity")
    for digest in (manifest_digest, archive_digest):
        hex_value(digest, 64)
    require(sha(canonical(manifest)) == manifest_digest
            and manifest["source_sha256"] == hashes(root, source_names(root))
            and git(root, "rev-parse", "HEAD") == manifest["commit"], "Staging proof frozen source mismatch")
    fields = {"schema", "run_id", "commit", "manifest_sha256", "archive_sha256", "inventory_sha256",
              "staging_identity", "summary_sha256", "summary"}
    require(isinstance(value, dict) and set(value) == fields and type(value["schema"]) is int
            and value["schema"] == 1 and value["run_id"] == run_id and value["commit"] == manifest["commit"]
            and value["manifest_sha256"] == manifest_digest and value["archive_sha256"] == archive_digest
            and value["inventory_sha256"] == sha(canonical(inventory(targets))), "Staging proof scope mismatch")
    identity = value["staging_identity"]
    require(isinstance(identity, dict) and set(identity) == {"deployment_id", "version_id", "worker", "commit", "code_sha256"}
            and identity["worker"] == targets["staging"]["name"] and identity["commit"] == manifest["commit"]
            and identity["code_sha256"] == manifest["code_sha256"], "Staging identity scope mismatch")
    uuid_value(identity["deployment_id"])
    uuid_value(identity["version_id"])
    game_summary(root, value["summary"])
    hex_value(value["summary_sha256"], 64)
    require(value["summary_sha256"] == sha(canonical(value["summary"])), "Staging summary digest mismatch")
    return value


def read_staging_proof(path: Path, digest: str, root: Path, manifest: dict, manifest_digest: str,
                       archive_digest: str, targets: dict, run_id: str) -> dict:
    hex_value(digest, 64)
    require(path.is_file() and not path.is_symlink(), "Missing immutable staging proof")
    raw = path.read_bytes()
    require(sha(raw) == digest, "Staging proof digest mismatch")
    return staging_proof(json_bytes(raw), root, manifest, manifest_digest, archive_digest, targets, run_id)


def staging_test(root: Path, artifact: Path, manifest: dict, manifest_digest: str,
                 archive: Path, archive_digest: str, targets: dict, run_id: str,
                 expected: dict, output: Path) -> str:
    require(sha(archive.read_bytes()) == archive_digest, "Frozen archive changed")
    verify(root, artifact, manifest_digest, manifest["commit"])
    require(os.environ.get("BREWS_API_ORIGIN") == origin(targets["staging"]["origin"])
            and os.environ.get("BREWS_DEV_CLI_KEY"), "Missing staging game credentials or target mismatch")
    remote_identity(targets["staging"], manifest, expected)
    with tempfile.TemporaryDirectory(prefix="brews-actual-staging-") as scratch:
        summary_dir = Path(scratch) / "game"
        command = [sys.executable, str(root / "backend/tests/full_game_e2e.py"),
                   "--origin", targets["staging"]["origin"], "--expected-origin", targets["staging"]["origin"],
                   "--environment", "staging", "--output", str(summary_dir),
                   "--bootstrap-cli", str(safe_file(artifact, "target/debug/brews"))]
        child_env = {k: v for k, v in os.environ.items() if not k.startswith(("CLOUDFLARE_", "CF_", "WRANGLER_", "BREWS_"))}
        child_env.update({key: os.environ[key] for key in ("BREWS_API_ORIGIN", "BREWS_DEV_CLI_KEY")})
        result = subprocess.run(command, cwd=root, env=child_env, capture_output=True, timeout=900)
        require(result.returncode == 0, "Actual staging game failed; raw output withheld")
        summary = game_summary(root, json_bytes(safe_file(summary_dir, "summary.json").read_bytes()))
        require(json_bytes(result.stdout) == summary, "Actual staging game summary differs from process output")
    # Recheck both source and deployment after cleanup, immediately before proof creation.
    verify(root, artifact, manifest_digest, manifest["commit"])
    require(sha(archive.read_bytes()) == archive_digest, "Frozen archive changed during game")
    identity = remote_identity(targets["staging"], manifest, expected)
    proof = {"schema": 1, "run_id": run_id, "commit": manifest["commit"], "manifest_sha256": manifest_digest,
             "archive_sha256": archive_digest, "inventory_sha256": sha(canonical(targets)),
             "staging_identity": identity, "summary_sha256": sha(canonical(summary)), "summary": summary}
    staging_proof(proof, root, manifest, manifest_digest, archive_digest, targets, run_id)
    save(output, proof)
    return sha(output.read_bytes())


def verify_staging_readback(proof: dict, targets: dict, manifest: dict) -> None:
    require(os.environ.get("CLOUDFLARE_API_TOKEN"), "Missing separately provisioned staging read credential")
    remote_identity(targets["staging"], manifest, proof["staging_identity"])


def smoke(target: dict) -> None:
    # Unknown route is deliberately nonmutating and must disclose no protected data.
    url = origin(target["origin"]) + "/__brews_release_smoke__"
    opener = request.build_opener(NoRedirect(), request.HTTPSHandler(context=ssl.create_default_context()))
    from urllib.error import HTTPError
    try:
        opener.open(request.Request(url, method="GET"), timeout=30)
    except HTTPError as response:
        require(response.code == 404 and response.geturl() == url, "Unexpected smoke response")
        return
    raise DeploymentError("Smoke response did not deny unknown route")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("freeze", "pack", "unpack", "verify", "config", "readiness", "protection", "deploy", "readback", "smoke", "staging-test", "verify-staging"))
    parser.add_argument("--artifact", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--archive", type=Path)
    parser.add_argument("--gate", type=Path)
    parser.add_argument("--commit")
    parser.add_argument("--manifest-sha256")
    parser.add_argument("--environment", choices=("staging", "production"))
    parser.add_argument("--expected-identity", type=Path)
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--proof", type=Path)
    parser.add_argument("--proof-sha256")
    parser.add_argument("--archive-sha256")
    parser.add_argument("--run-id")
    args = parser.parse_args()
    try:
        if args.command == "freeze":
            print(freeze(ROOT, args.output, args.gate, args.commit))
            return 0
        if args.command == "unpack":
            unpack(args.archive, args.output)
            return 0
        if args.command == "protection":
            protection()
            return 0
        value = verify(ROOT, args.artifact, args.manifest_sha256, args.commit)
        if args.command == "verify":
            return 0
        if args.command == "pack":
            pack(args.artifact, args.archive)
            return 0
        targets = public_inventory()
        if args.command == "config":
            save(args.output, release_config(ROOT, args.artifact, value, targets, args.environment))
        elif args.command == "readiness":
            readiness(json_bytes(os.environ.get("BREWS_REVIEWED_READINESS", "").encode()), value, args.manifest_sha256, targets)
        elif args.command == "deploy":
            if not args.dry_run:
                readiness(json_bytes(os.environ.get("BREWS_REVIEWED_READINESS", "").encode()), value, args.manifest_sha256, targets)
                if args.environment == "production":
                    read_staging_proof(args.proof, args.proof_sha256, ROOT, value, args.manifest_sha256,
                                       args.archive_sha256, targets, args.run_id)
                    protection()
            identity = deploy(ROOT, args.artifact, value, targets, args.environment, args.dry_run)
            if identity:
                save(args.output, identity)
        elif args.command == "staging-test":
            readiness(json_bytes(os.environ.get("BREWS_REVIEWED_READINESS", "").encode()), value, args.manifest_sha256, targets)
            expected = json_bytes(args.expected_identity.read_bytes())
            print(staging_test(ROOT, args.artifact, value, args.manifest_sha256, args.archive,
                               args.archive_sha256, targets, args.run_id, expected, args.output))
        elif args.command == "verify-staging":
            proof = read_staging_proof(args.proof, args.proof_sha256, ROOT, value, args.manifest_sha256,
                                       args.archive_sha256, targets, args.run_id)
            verify_staging_readback(proof, targets, value)
        elif args.command == "readback":
            expected = json_bytes(args.expected_identity.read_bytes())
            remote_identity(targets[args.environment], value, expected)
        elif args.command == "smoke":
            smoke(targets[args.environment])
        return 0
    except Exception:
        print("Deployment check blocked; required evidence unavailable or invalid (raw details withheld)", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
