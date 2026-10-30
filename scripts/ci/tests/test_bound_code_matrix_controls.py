"""Synthetic controls for the fail-closed bound-code matrix runner."""
import importlib.util
import json
import os
import stat
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / "check-bound-code-matrix.py"
spec = importlib.util.spec_from_file_location("bound_matrix", SCRIPT)
matrix = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = matrix
spec.loader.exec_module(matrix)


class BoundCodeMatrixControls(unittest.TestCase):
    def private_env(self, directory: Path, url="postgresql://u:private-password@127.0.0.1:5432/engram_test_ec536887ebee") -> Path:
        directory.chmod(0o700)
        fixture = directory / "pg.env"
        fixture.write_text(f"ENGRAM_TEST_PG_URL={url}\n", encoding="utf-8")
        fixture.chmod(0o600)
        return fixture

    def test_marker_validation_refuses_before_operation(self):
        with tempfile.TemporaryDirectory() as temp:
            fixture = self.private_env(Path(temp))
            calls = []
            def fake(argv, **kwargs):
                calls.append(argv)
                if argv[1] == "inspect":
                    identity = {"Id": matrix.PINNED_CONTAINER_ID, "State": {"Running": True}, "Config": {"Labels": matrix.PINNED_LABELS}, "Mounts": [], "HostConfig": {"Tmpfs": {"/var/lib/postgresql/data": "rw,size=2g"}}, "NetworkSettings": {"Ports": {"5432/tcp": [{"HostIp": "127.0.0.1", "HostPort": "5432"}]}}}
                    return subprocess.CompletedProcess(argv, 0, json.dumps([identity]), "")
                return subprocess.CompletedProcess(argv, 0, "wrong-marker", "")
            runner = matrix.MatrixRunner(fake)
            with self.assertRaisesRegex(ValueError, "marker mismatch"):
                runner.preflight_pg("ENGRAM_TEST_PG_URL", {matrix.PG_ENV_FILE: str(fixture), "ENGRAM_TEST_PG_URL": fixture.read_text().split("=", 1)[1].strip()})
            self.assertEqual(len(calls), 2)

    def test_unmarked_database_is_refused_by_main_before_rows(self):
        old_env, old_runner = dict(os.environ), matrix.MatrixRunner
        try:
            with tempfile.TemporaryDirectory() as temp:
                fixture = self.private_env(Path(temp))
                calls = []
                def fake(argv, **kwargs):
                    calls.append(argv)
                    if argv[1] == "inspect":
                        identity = {"Id": matrix.PINNED_CONTAINER_ID, "State": {"Running": True}, "Config": {"Labels": matrix.PINNED_LABELS}, "Mounts": [], "HostConfig": {"Tmpfs": {"/var/lib/postgresql/data": "rw,size=2g"}}, "NetworkSettings": {"Ports": {"5432/tcp": [{"HostIp": "127.0.0.1", "HostPort": "5432"}]}}}
                        return subprocess.CompletedProcess(argv, 0, json.dumps([identity]), "")
                    return subprocess.CompletedProcess(argv, 0, "wrong-marker", "")
                matrix.MatrixRunner = lambda: old_runner(fake)
                os.environ.update({"ENGRAM_TEST_REQUIRE_PG": "1", matrix.PG_ENV_FILE: str(fixture), "ENGRAM_TEST_PG_URL": fixture.read_text().split("=", 1)[1].strip()})
                self.assertEqual(matrix.main(["--sqlite-temp", "--fail-on-skip", "--stub-command", "/not/run"]), 2)
                self.assertEqual([call[1] for call in calls], ["inspect", "exec"])
        finally:
            matrix.MatrixRunner = old_runner
            os.environ.clear(); os.environ.update(old_env)

    def test_missing_required_credentials_refuses_not_skips(self):
        old = dict(os.environ)
        try:
            os.environ.pop("ENGRAM_TEST_REQUIRE_PG", None)
            self.assertEqual(matrix.main(["--sqlite-temp", "--fail-on-skip", "--stub-command", sys.executable]), 2)
        finally:
            os.environ.clear(); os.environ.update(old)

    def test_fail_on_skip_makes_required_rows_fatal(self):
        with tempfile.TemporaryDirectory() as temp:
            runner = matrix.MatrixRunner()
            row = matrix.Row("binding", "sqlite", "rust", True)
            code = runner.execute([row], lambda _: (_ for _ in ()).throw(FileNotFoundError("synthetic absent")), Path(temp), {}, True)
            self.assertEqual(code, 1)
            receipt = json.loads(next(Path(temp).glob("[0-9][0-9][0-9]-*.json")).read_text())
            self.assertEqual(receipt["status"], "skipped")

    def test_optional_skip_is_not_fatal_without_fail_on_skip(self):
        with tempfile.TemporaryDirectory() as temp:
            row = matrix.Row("binding", "pgvector", "rust", False)
            code = matrix.MatrixRunner().execute([row], lambda _: (_ for _ in ()).throw(FileNotFoundError("synthetic absent")), Path(temp), {}, False)
            self.assertEqual(code, 0)

    def test_private_fixture_requires_absolute_owned_mode_0600_file(self):
        with tempfile.TemporaryDirectory() as temp:
            fixture = Path(temp) / "pg.env"
            fixture.write_text("ENGRAM_TEST_PG_URL=postgresql://u:p@host/db\n")
            fixture.chmod(0o644)
            with self.assertRaisesRegex(ValueError, "custody"):
                matrix.load_private_pg_env(str(fixture), "ENGRAM_TEST_PG_URL")

    def test_row_aware_mcp_mapping_uses_transport_receipts(self):
        root = Path("/tmp/matrix-control")
        stdio = matrix.default_command(matrix.Row("binding", "sqlite", "mcp-stdio", True), root, "ENGRAM_TEST_PG_URL")
        oneshot = matrix.default_command(matrix.Row("backfill", "pgvector", "mcp-oneshot", True), root, "ENGRAM_TEST_PG_URL")
        stdio_backfill = matrix.default_command(matrix.Row("backfill", "sqlite", "mcp-stdio", True), root, "ENGRAM_TEST_PG_URL")
        reset = matrix.default_command(matrix.Row("reset", "sqlite", "mcp-stdio", True), root, "ENGRAM_TEST_PG_URL")
        self.assertIn("task_021::stdio_dispatch_parity", stdio.argv)
        self.assertEqual(stdio.marker, "stdio_dispatch_parity")
        self.assertIn("task_021::stdio_backfill_not_rescan", stdio_backfill.argv)
        self.assertEqual(stdio_backfill.marker, "stdio_backfill_not_rescan")
        self.assertIn("task_022::backfill_not_rescan", oneshot.argv)
        self.assertEqual(oneshot.marker, "backfill_not_rescan")
        self.assertEqual(reset.surface, "transport")
        self.assertIsNone(reset.transport_surface)

    def test_pg_rust_rows_select_pg_recipe_proofs_and_execute_ignored_reset(self):
        root = Path("/tmp/matrix-control")
        expected = {
            "scan": "real_pg_scan_publish_content",
            "retrieval": "real_pg_retrieval_eligible_content",
            "graph": "real_pg_graph_bound_answer",
            "source": "real_pg_source_exact_bytes_and_span",
        }
        for operation, marker in expected.items():
            command = matrix.default_command(matrix.Row(operation, "pgvector", "rust", True), root, "ENGRAM_TEST_PG_URL")
            self.assertIn("engram-backend-pgvector", command.argv)
            self.assertIn("bound_recipe", command.argv)
            self.assertTrue(any(marker in argument for argument in command.argv))
            self.assertEqual(command.marker, marker)
            self.assertIn("--ignored", command.argv)
        reset = matrix.default_command(matrix.Row("reset", "pgvector", "rust", True), root, "ENGRAM_TEST_PG_URL")
        self.assertIn("code_reset_recovery", reset.argv)
        self.assertIn("--ignored", reset.argv)

    def test_zero_executed_or_ignored_only_suite_fails_even_with_marker(self):
        with tempfile.TemporaryDirectory() as temp:
            row = matrix.Row("reset", "pgvector", "rust", True)
            def fake(argv, **kwargs):
                return subprocess.CompletedProcess(argv, 0, "marker\n\ntest result: ok. 0 passed; 0 failed; 1 ignored", "")
            result = matrix.MatrixRunner(fake).execute([row], lambda _: matrix.CommandSpec(["synthetic"], "marker"), Path(temp), {}, True)
            self.assertEqual(result, 1)
            receipt = json.loads(next(Path(temp).glob("[0-9][0-9][0-9]-*.json")).read_text())
            self.assertEqual(receipt["status"], "failed")
            self.assertEqual(receipt["executed_tests"], 0)
            self.assertIn("selected suite executed zero tests", receipt["output"])

    def test_stale_artifact_invalidation_and_loaded_recipe_selection_are_synthetic(self):
        calls = []
        stale = matrix.NativeRecipeArtifact("sqlite", "/artifacts/sqlite.node", "old-content")
        rebuilt = matrix.NativeRecipeArtifact("sqlite", "/artifacts/sqlite.node", "new-content")
        loaded = matrix.NativeRecipeArtifact("sqlite", "/artifacts/sqlite.node", "new-content")
        selected = matrix.resolve_native_recipe_artifact(
            "sqlite",
            "new-content",
            lambda recipe: stale,
            lambda recipe: calls.append(recipe) or rebuilt,
            lambda path: loaded,
        )
        self.assertEqual(calls, ["sqlite"], "content drift rebuilds despite an unchanged path")
        self.assertEqual(selected, loaded)

        wrong_loaded = matrix.NativeRecipeArtifact("pgvector", "/artifacts/sqlite.node", "new-content")
        with self.assertRaisesRegex(ValueError, "loaded native recipe identity"):
            matrix.resolve_native_recipe_artifact(
                "sqlite",
                "new-content",
                lambda recipe: rebuilt,
                lambda recipe: self.fail("fresh artifact must not rebuild"),
                lambda path: wrong_loaded,
            )

    def test_helper_argument_assembles_selected_recipe_only(self):
        argv = matrix.native_helper_argv("pgvector")
        self.assertEqual(argv[:3], ["node", "--input-type=module", "--eval"])
        self.assertIn("withRecipeArtifacts", argv[3])
        self.assertIn('recipe:"pgvector"', argv[3])
        self.assertIn('a.paths["pgvector"]', argv[3])
        with self.assertRaises(ValueError):
            matrix.native_helper_argv("other")

    def test_every_running_suite_receipt_is_accounted(self):
        with tempfile.TemporaryDirectory() as temp:
            rows = [matrix.Row("binding", "sqlite", "rust", True), matrix.Row("scan", "sqlite", "native", True)]
            def fake(argv, **kwargs):
                return subprocess.CompletedProcess(argv, 0, f"{argv[-1]}\ntest result: ok. 1 passed; 0 failed; 0 ignored", "")
            result = matrix.MatrixRunner(fake).execute(rows, lambda row: matrix.CommandSpec(["synthetic", row.name], row.name), Path(temp), {}, True)
            self.assertEqual(result, 0)
            receipts = list(Path(temp).glob("[0-9][0-9][0-9]-*.json"))
            self.assertEqual(len(receipts), len(rows))
            self.assertEqual({json.loads(path.read_text())["row"] for path in receipts}, {row.name for row in rows})
            manifest = json.loads((Path(temp) / "manifest.json").read_text())
            self.assertEqual(manifest["receipt_count"], len(rows))
            self.assertEqual({entry["row"] for entry in manifest["receipts"]}, {row.name for row in rows})

    def test_sanitized_receipts_do_not_contain_credential_material(self):
        with tempfile.TemporaryDirectory() as temp:
            secret = "private-password"
            row = matrix.Row("binding", "sqlite", "rust", True)
            def fake(argv, **kwargs):
                return subprocess.CompletedProcess(argv, 1, f"postgresql://u:{secret}@host/db token={secret}", "")
            matrix.MatrixRunner(fake).execute([row], lambda _: matrix.CommandSpec(["synthetic", f"postgresql://u:{secret}@host/db"], row.name), Path(temp), {}, True)
            receipt = next(Path(temp).glob("[0-9][0-9][0-9]-*.json")).read_text()
            self.assertNotIn(secret, receipt)
            self.assertIn("[REDACTED]", receipt)


if __name__ == "__main__":
    unittest.main()
