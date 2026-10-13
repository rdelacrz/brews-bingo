"""Run local maximum-configured-game observations, never a provider/forecast capacity gate."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import subprocess
import sys

import release_gate as gates

MARKER = "BREWS-GAME-RESOURCES"
STATIC_LABELS = {
    "maximum_numeric_VALUE_cells_no_free_cells",
    "native_Workerd_Upgrade_WebSocket_pairs_not_network_TLS_WSS",
    "Workerd_crypto_getRandomValues_not_native_TestRuntime",
    "undrained_execute_cursor_row_work",
    "SDK_cursor_row_work_runtime_observer_trap",
}


def summaries(text: str, expected_target: str) -> dict:
    values = []
    def check(value: object, depth: int = 0) -> None:
        if depth > 10:
            raise gates.GateError("Resource summary exceeds the allowed nesting")
        if isinstance(value, dict):
            if len(value) > 100:
                raise gates.GateError("Resource summary exceeds the object bound")
            for key, item in value.items():
                if not isinstance(key, str) or key.lower() in {
                    "password", "token", "cookie", "secret", "credential", "credentials",
                    "verifier", "salt", "answer", "fingerprint", "board", "snapshot", "payload", "sql", "params", "bindings",
                }:
                    raise gates.GateError("Private content is not a resource metric")
                check(item, depth + 1)
        elif isinstance(value, list):
            if len(value) > 100:
                raise gates.GateError("Resource summary exceeds the list bound")
            for item in value:
                check(item, depth + 1)
        elif isinstance(value, str):
            if (len(value) > 300 or any(marker in value for marker in ("$argon2", "__Secure-", "Bearer "))
                    or re.search(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}", value, re.I)
                    or re.fullmatch(r"[A-Za-z0-9_-]{32,}", value)
                    and value not in STATIC_LABELS):
                raise gates.GateError("Resource string contains private or oversized content")
        elif isinstance(value, bool) or value is None:
            pass
        elif isinstance(value, (int, float)):
            if not 0 <= value <= 2**53 - 1:
                raise gates.GateError("Resource metric is not finite, safe and nonnegative")
        else:
            raise gates.GateError("Resource metric has an unsupported type")
    for match in re.finditer(r"BREWS-GAME-RESOURCES (\{[^\n]*\})", text):
        value = json.loads(match[1])
        if not isinstance(value, dict) or value.get("target") != expected_target:
            raise gates.GateError("Wrong-target resource observation")
        check(value)
        values.append(value)
    if len(values) != 1:
        raise gates.GateError("Resource test did not emit exactly one summary")
    return values[0]


def validate_sample(value: dict) -> None:
    source = (gates.ROOT / "shared/domain/src/games.rs").read_text()
    def maximum(name: str) -> int:
        found = re.search(r"pub const " + name + r": [^=]+ = ([0-9_]+);", source)
        if not found:
            raise gates.GateError("Domain maximum is unavailable")
        return int(found[1].replace("_", ""))
    players, spectators = maximum("PLAYER_CAPACITY_MAX"), maximum("SPECTATOR_CAPACITY_MAX")
    side, pool = maximum("BOARD_SIDE_MAX"), maximum("NUMERIC_UPPER_BOUND_MAX")
    configuration, sample = value["configuration"], value["sample"]
    native = value["target"] == "native_sqlite"
    if configuration.get("branch") != "maximum_numeric_VALUE_cells_no_free_cells":
        raise gates.GateError("Wrong resource configuration branch")
    expected = {"players": players, "spectators": spectators,
                "board_side" if native else "side": side, "numeric_pool" if native else "numeric": pool}
    if any(type(configuration.get(k)) is not int or configuration[k] != v for k, v in expected.items()):
        raise gates.GateError("Resource configuration is not the actual current maximum branch")
    stages = value["stages"]
    names = [stage["name"] for stage in stages]
    required = {"lobby", "started", "called", "history_cleanup_release"}
    if len(names) != len(set(names)) or set(names) != required | ({"auth_and_migrations"} if native else set()):
        raise gates.GateError("Resource stage inventory is incomplete or duplicate")
    by_name = {stage["name"]: stage for stage in stages}
    lobby = by_name["lobby"]["storage"]["game"]["tables"]
    started = by_name["started"]["storage"]["game"]["tables"]
    called = by_name["called"]["storage"]["game"]["tables"]
    terminal = by_name["history_cleanup_release"]["storage"]["game"]["tables"]
    calls = sample["manual_calls"]
    if (type(calls) is not int or not 1 <= calls <= pool or native and calls != pool
            or lobby.get("game_players") != players or lobby.get("game_spectators") != spectators
            or started.get("game_boards") != players or started.get("game_board_cells") != players * side * side
            or called.get("game_calls") != calls or terminal.get("game_history_calls") != calls
            or terminal.get("game_history_board_cells") != players * side * side
            or terminal.get("game_spectators") != 0):
        raise gates.GateError("Resource state/cardinality does not match accepted operations")
    for stage in stages:
        for owner in ("accounts", "directory", "game"):
            storage = stage["storage"][owner]
            if not all(type(n) is int and n >= 0 for n in storage["tables"].values()):
                raise gates.GateError("Resource table cardinalities must be integer counts")
            if not native:
                size = storage["sqlite_database_size_bytes"]
                if type(size) is not int or size < 0 or storage["page_pragmas_available"] is not False:
                    raise gates.GateError("Actual SDK storage metric is malformed or mislabeled")
                work = stage.get("sql_sdk_exhausted_cursor_work", {})
                if set(work) != {"accounts", "directory", "game"}:
                    raise gates.GateError("Unavailable SDK work owner inventory is missing")
                observation = work[owner]
                if (not isinstance(observation, dict) or set(observation) != {"measurement_available"}
                        or observation["measurement_available"] is not False):
                    raise gates.GateError("SDK work must remain explicitly unavailable, not fabricated")
                continue
            pages, size, free = (storage[k] for k in ("page_count", "page_size", "freelist_count"))
            if (not all(type(n) is int and n >= 0 for n in (pages, size, free))
                    or size < 1 or free > pages or storage["sqlite_page_image_allocation_bytes"] != pages * size):
                raise gates.GateError("Resource page allocation metrics are inconsistent")
    if native:
        if (type(sample["socket_load"]) is not int or type(sample["logical_connections"]) is not int
                or sample["socket_load"] != 0 or sample["logical_connections"] != players + spectators):
            raise gates.GateError("Logical native presence was mislabeled as socket load")
    else:
        frames, totals = sample["frames"], sample["totals"]
        pending = value.get("limits_pending")
        if (not isinstance(pending, list)
                or pending.count("SDK_cursor_row_work_runtime_observer_trap") != 1):
            raise gates.GateError("Required unavailable SDK work qualification is missing")
        if (not isinstance(frames, list) or not all(isinstance(item, dict) for item in frames)
                or [item.get("view") for item in frames] != ["player", "player", "spectator"]
                or type(sample["socket_load"]) is not int or type(sample["admissions"]) is not int
                or sample["socket_load"] != len(frames) or sample["admissions"] != players + spectators
                or lobby.get("game_current_connections") != len(frames)):
            raise gates.GateError("Actual socket/admission sample accounting differs")
        fields = ("frames", "frame_bytes", "snapshots", "terminal_notices", "ack_frames", "ack_bytes")
        for item in frames:
            if (not all(type(item.get(key)) is int and item[key] >= 0 for key in fields)
                    or item.get("closed") is not True or item["snapshots"] < 1 or item["frame_bytes"] < 1
                    or item["ack_bytes"] < 1 or item["frames"] != item["snapshots"] + item["terminal_notices"]
                    or item["ack_frames"] != item["snapshots"]):
                raise gates.GateError("Observed socket closure/frame/ACK accounting differs")
        for key in fields:
            if type(totals.get(key)) is not int or totals[key] != sum(item[key] for item in frames):
                raise gates.GateError("Observed socket aggregate totals differ")


def run(args: argparse.Namespace) -> int:
    output = args.output.expanduser().resolve()
    if output == gates.ROOT or gates.ROOT in output.parents:
        raise gates.GateError("Use a new private scratch directory outside the repository")
    output.mkdir(mode=0o700, parents=True, exist_ok=False)
    before = gates.source_hashes(gates.ROOT)
    runner = gates.Runner(output, args.deadline, 1)
    report = {
        "kind": "local_game_resource_observations_not_forecast_or_production_certification",
        "baseline_commit": subprocess.check_output(
            ["git", "-c", f"safe.directory={gates.ROOT}", "-C", str(gates.ROOT), "rev-parse", "HEAD"], text=True,
        ).strip(),
        "deployment": "none", "production_release": "blocked", "owner_forecast": "not_supplied",
        "applicable_provider_capacity_and_headroom": "unverified", "source_sha256": before, "observations": {},
        "open_production_gates": list(gates.PRODUCTION_BLOCKERS),
    }
    exit_code = 0
    try:
        channel = gates.tomllib.loads((gates.ROOT / "rust-toolchain.toml").read_text())["toolchain"]["channel"]
        _, version = runner.command("cargo-version", ["cargo", "--version"])
        if not version.startswith(f"cargo {channel} "):
            raise gates.GateError("Cargo does not match the repository's pinned toolchain")
        for profile in ("debug", "release-assertions"):
            command = ["cargo", "nextest", "run", "--locked", "-p", "brews-backend", "--test", "game_resources", "--success-output", "immediate"]
            if profile != "debug":
                command += ["--cargo-profile", profile]
            _, text = runner.command("resources-" + profile, command)
            inventory = gates.native_inventory(text)
            sample = summaries(text, "native_sqlite")
            validate_sample(sample)
            report["observations"][profile] = {"passed": len(inventory), "case_ids_sha256": inventory,
                                                  "sample": sample}
        if report["observations"]["debug"]["case_ids_sha256"] != report["observations"]["release-assertions"]["case_ids_sha256"]:
            raise gates.GateError("Resource native profile case inventories differ")
        runner.command("resource-worker-build", ["npm", "run", "build"], gates.BACKEND)
        runner.require_artifacts()
        _, text = runner.command("resource-workerd", [str(gates.BACKEND / "node_modules/.bin/vitest"), "run",
                                  "worker-tests/game_resources.test.js", "--reporter=default", "--reporter=json"], gates.BACKEND)
        result = gates.reconcile_workers([gates.parse_worker(text)], {str(gates.BACKEND / "worker-tests/game_resources.test.js")})
        result["sample"] = summaries(text, "local_workerd_emulated_SDK")
        validate_sample(result["sample"])
        runner.require_artifacts()
        report["observations"]["workerd"] = result
    except (gates.GateError, KeyError, TypeError, ValueError, OSError) as error:
        report["execution_error"] = type(error).__name__
        exit_code = 1
    except KeyboardInterrupt:
        gates.cancel_children()
        report["execution_error"] = "Cancelled"
        exit_code = 130
    finally:
        report["source_unchanged"] = before == gates.source_hashes(gates.ROOT)
        report["worker_artifact_sha256"] = runner.active_artifacts or {}
        if not report["source_unchanged"]:
            exit_code = 1
        if runner.active_artifacts is not None:
            try:
                runner.require_artifacts()
            except (gates.GateError, OSError):
                report["artifact_unchanged"] = False
                exit_code = 1
            else:
                report["artifact_unchanged"] = True
        report["local_execution_exit_code"] = exit_code
        gates.save(output / "report.json", report)
    print(f"Resource report: {output / 'report.json'}; forecast and production gates remain pending")
    return exit_code


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path, help="New private directory outside the repository")
    parser.add_argument("--deadline", default=900, type=int, choices=range(30, 1801), metavar="SECONDS")
    try:
        return run(parser.parse_args())
    except (gates.GateError, OSError, subprocess.SubprocessError) as error:
        print(f"Resource probe unavailable ({type(error).__name__}); no capacity certification", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
