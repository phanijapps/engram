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
        old = dict(os.environ)
        try:
            with tempfile.TemporaryDirectory() as temp:
                fixture = self.private_env(Path(temp))
                os.environ.update({"ENGRAM_TEST_REQUIRE_PG": "1", matrix.PG_ENV_FILE: str(fixture), "ENGRAM_TEST_PG_URL": fixture.read_text().split("=", 1)[1].strip()})
                self.assertEqual(matrix.main(["--sqlite-temp", "--fail-on-skip", "--stub-command", "/not/run"]), 2)
        finally:
            os.environ.clear(); os.environ.update(old)

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
            receipt = json.loads(next(Path(temp).glob("*.json")).read_text())
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

    def test_mcp_rows_execute_authorized_real_transport_harnesses(self):
        root = Path("/tmp/matrix-control")
        stdio = matrix.default_command(matrix.Row("binding", "sqlite", "mcp-stdio", True), root, "ENGRAM_TEST_PG_URL")
        oneshot = matrix.default_command(matrix.Row("binding", "pgvector", "mcp-oneshot", True), root, "ENGRAM_TEST_PG_URL")
        self.assertEqual(stdio[:10], ["cargo", "test", "-p", "engram-mcp", "--no-default-features", "--features", "pgvector", "--test", "bound_transports", "task_021::stdio_dispatch_parity"])
        self.assertIn("task_021::stdio_dispatch_parity", stdio)
        self.assertIn("task_022::oneshot_dispatch_parity", oneshot)
        self.assertNotIn("--pg-connection-string-env", oneshot)

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
                return subprocess.CompletedProcess(argv, 0, "ok", "")
            result = matrix.MatrixRunner(fake).execute(rows, lambda row: ["synthetic", row.name], Path(temp), {}, True)
            self.assertEqual(result, 0)
            receipts = list(Path(temp).glob("*.json"))
            self.assertEqual(len(receipts), len(rows))
            self.assertEqual({json.loads(path.read_text())["row"] for path in receipts}, {row.name for row in rows})

    def test_sanitized_receipts_do_not_contain_credential_material(self):
        with tempfile.TemporaryDirectory() as temp:
            secret = "private-password"
            row = matrix.Row("binding", "sqlite", "rust", True)
            def fake(argv, **kwargs):
                return subprocess.CompletedProcess(argv, 1, f"postgresql://u:{secret}@host/db token={secret}", "")
            matrix.MatrixRunner(fake).execute([row], lambda _: ["synthetic", f"postgresql://u:{secret}@host/db"], Path(temp), {}, True)
            receipt = next(Path(temp).glob("*.json")).read_text()
            self.assertNotIn(secret, receipt)
            self.assertIn("[REDACTED]", receipt)


if __name__ == "__main__":
    unittest.main()
