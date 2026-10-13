"""Resource summary parser fixtures are unit data, not measured service output."""
import importlib.util
from copy import deepcopy
import json
from pathlib import Path
import sys
import unittest

TOOLS = Path(__file__).parents[1]
sys.path.insert(0, str(TOOLS))
SPEC = importlib.util.spec_from_file_location("resource_probe", TOOLS / "game_resource_probe.py")
assert SPEC is not None and SPEC.loader is not None
probe = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(probe)


class ResourceParserTests(unittest.TestCase):
    @staticmethod
    def text(value):
        return "BREWS-GAME-RESOURCES " + json.dumps(value)

    def test_safe_numeric_summary_and_target(self):
        value = {"target": "native", "configuration": {"player_capacity": 20},
                 "stages": [{"stage": "started", "cell_count": 2000, "wall_ns": 12345}],
                 "limits_pending": ["owner forecast", "provider headroom"]}
        self.assertEqual(probe.summaries(self.text(value), "native"), value)
        with self.assertRaises(probe.gates.GateError):
            probe.summaries(self.text(value), "actual-local-Workerd")

    def test_missing_duplicate_marker_or_wrong_root_fails(self):
        text = self.text({"target": "native"})
        for value in ("No marker", text + "\n" + text, self.text({"target": "wrong"})):
            with self.assertRaises(probe.gates.GateError):
                probe.summaries(value, "native")

    def test_negative_unsafe_nonfinite_metrics_fail(self):
        for number in (-1, 2**53, float("nan"), float("inf"), -float("inf")):
            with self.subTest(number=number), self.assertRaises(probe.gates.GateError):
                probe.summaries(self.text({"target": "native", "metric": number}), "native")

    def test_private_fields_and_credential_like_values_fail(self):
        for field in ("password", "token", "cookie", "verifier", "salt", "board", "payload", "bindings", "snapshot"):
            with self.subTest(field=field), self.assertRaises(probe.gates.GateError):
                probe.summaries(self.text({"target": "native", "stage": {field: "unit fixture"}}), "native")
        for value in ("Bearer unit fixture", "$argon2id$unit fixture", "__Secure-unit fixture",
                      "01890f3e-53b7-7d28-9b05-4f65092d5701", "x" * 43, "long " * 70):
            with self.assertRaises(probe.gates.GateError):
                probe.summaries(self.text({"target": "native", "label": value}), "native")

    def test_resource_containers_are_bounded(self):
        values = [{"target": "native", "items": list(range(101))},
                  {"target": "native", **{f"metric{n}": n for n in range(101)}}]
        nested = {}
        for _ in range(12):
            nested = {"next": nested}
        values.append({"target": "native", "nested": nested})
        for value in values:
            with self.assertRaises(probe.gates.GateError):
                probe.summaries(self.text(value), "native")

    def test_single_test_nextest_summary_requires_a_real_unique_pass(self):
        value = "PASS [0.1s] (1/1) binary measured_case\nSummary [0.1s] 1 test run: 1 passed, 0 skipped"
        self.assertEqual(probe.gates.parse_native(value), 1)
        with self.assertRaises(probe.gates.GateError):
            probe.gates.parse_native(value.replace("PASS [0.1s] (1/1) binary measured_case\n", ""))

    def test_native_stage_cardinality_and_storage_equations_are_checked(self):
        storage = {"page_count": 1, "page_size": 4096, "freelist_count": 0,
                   "sqlite_page_image_allocation_bytes": 4096, "tables": {}}
        stages = []
        for name in ("auth_and_migrations", "lobby", "started", "called", "history_cleanup_release"):
            tables = {"game_players": 20, "game_spectators": 50}
            if name == "started":
                tables.update(game_boards=20, game_board_cells=20 * 10 * 10)
            if name == "called":
                tables.update(game_calls=1000)
            if name == "history_cleanup_release":
                tables.update(game_spectators=0, game_history_calls=1000, game_history_board_cells=20 * 10 * 10)
            owners = {owner: deepcopy(storage) for owner in ("accounts", "directory", "game")}
            owners["game"]["tables"] = tables
            stages.append({"name": name, "storage": owners})
        value = {"target": "native_sqlite", "configuration": {"branch": "maximum_numeric_VALUE_cells_no_free_cells",
                 "players": 20, "spectators": 50, "board_side": 10, "numeric_pool": 1000},
                 "sample": {"manual_calls": 1000, "socket_load": 0, "logical_connections": 70}, "stages": stages}
        probe.validate_sample(value)
        for mutation in (lambda v: v["configuration"].update(players=True),
                         lambda v: v["sample"].update(socket_load=70),
                         lambda v: v["stages"].pop(),
                         lambda v: v["stages"][2]["storage"]["game"]["tables"].update(game_board_cells=1),
                         lambda v: v["stages"][0]["storage"]["accounts"].update(sqlite_page_image_allocation_bytes=1)):
            invalid = deepcopy(value)
            mutation(invalid)
            with self.assertRaises(probe.gates.GateError):
                probe.validate_sample(invalid)

    @staticmethod
    def worker_fixture():
        # Unit data only: no synthetic values are used as resource observations.
        stages = []
        for name in ("lobby", "started", "called", "history_cleanup_release"):
            tables = {"game_players": 20, "game_spectators": 50, "game_current_connections": 3}
            if name == "started":
                tables.update(game_boards=20, game_board_cells=20 * 10 * 10)
            if name == "called":
                tables.update(game_calls=1)
            if name == "history_cleanup_release":
                tables.update(game_spectators=0, game_history_calls=1, game_history_board_cells=20 * 10 * 10)
            storage = {owner: {"sqlite_database_size_bytes": 4096, "page_pragmas_available": False,
                              "tables": tables if owner == "game" else {}}
                       for owner in ("accounts", "directory", "game")}
            stages.append({"name": name, "storage": storage,
                           "sql_sdk_exhausted_cursor_work": {owner: {"measurement_available": False}
                                                            for owner in ("accounts", "directory", "game")}})
        frames = [{"view": view, "closed": True, "frames": 1, "frame_bytes": 200, "snapshots": 1,
                   "terminal_notices": 0, "ack_frames": 1, "ack_bytes": 168}
                  for view in ("player", "player", "spectator")]
        totals = {key: sum(item[key] for item in frames)
                  for key in ("frames", "frame_bytes", "snapshots", "terminal_notices", "ack_frames", "ack_bytes")}
        return {"target": "local_workerd_emulated_SDK",
                "configuration": {"branch": "maximum_numeric_VALUE_cells_no_free_cells",
                                  "players": 20, "spectators": 50, "side": 10, "numeric": 1000},
                "sample": {"admissions": 70, "socket_load": 3, "manual_calls": 1, "frames": frames, "totals": totals},
                "stages": stages, "limits_pending": ["SDK_cursor_row_work_runtime_observer_trap"]}

    def test_worker_socket_inventory_closure_and_integer_accounting_are_checked(self):
        value = self.worker_fixture()
        probe.validate_sample(value)
        mutations = (lambda v: v["sample"].update(frames=[], socket_load=0, totals={k: 0 for k in v["sample"]["totals"]}),
                     lambda v: v["sample"]["frames"][0].update(view="spectator"),
                     lambda v: v["sample"]["frames"][0].update(closed="yes"),
                     lambda v: v["sample"]["frames"][0].update(closed=1),
                     lambda v: v["sample"]["frames"][0].update(frames=True),
                     lambda v: v["sample"]["frames"][0].update(frame_bytes=200.0),
                     lambda v: v["sample"]["frames"][0].update(ack_bytes=-1),
                     lambda v: v["sample"]["totals"].update(snapshots=3.0),
                     lambda v: v["sample"].update(socket_load=3.0),
                     lambda v: v["stages"][0]["storage"]["game"]["tables"].update(game_current_connections=0))
        for mutation in mutations:
            invalid = deepcopy(value)
            mutation(invalid)
            with self.assertRaises(probe.gates.GateError):
                probe.validate_sample(invalid)

    def test_worker_sdk_unavailability_cannot_be_omitted_or_replaced_with_zero_counters(self):
        value = self.worker_fixture()
        probe.validate_sample(value)
        for mutation in (lambda v: v.pop("limits_pending"),
                         lambda v: v["stages"][0].pop("sql_sdk_exhausted_cursor_work"),
                         lambda v: v["stages"][0]["sql_sdk_exhausted_cursor_work"].pop("game"),
                         lambda v: v["stages"][0]["sql_sdk_exhausted_cursor_work"]["game"].update(measurement_available=0),
                         lambda v: v["stages"][0]["sql_sdk_exhausted_cursor_work"]["game"].update(measurement_available=True),
                         lambda v: v["stages"][0]["sql_sdk_exhausted_cursor_work"]["game"].update(rows_read=0)):
            invalid = deepcopy(value)
            mutation(invalid)
            with self.assertRaises(probe.gates.GateError):
                probe.validate_sample(invalid)


if __name__ == "__main__":
    unittest.main()
