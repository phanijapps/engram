#!/usr/bin/env python3
"""Unit tests for check_console_data_access (job 012; job-011 gate+fixture pattern).

Negative fixtures: a planted engine driver or SQL literal inside console
source fails the scan naming the file and line; clean engine-surface source
passes.
"""
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import check_console_data_access as gate


class ConsoleDataAccess(unittest.TestCase):
    def scan_dir(self, text: str, suffix: str = ".ts"):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            source = root / "src"
            source.mkdir()
            (source / f"probe{suffix}").write_text(text)
            return gate.scan_paths([source])

    def test_planted_driver_import_fails_with_file_named(self):
        violations = self.scan_dir(
            'import { DatabaseSync } from "node:sqlite";\nexport const x = 1;\n'
        )
        self.assertTrue(
            any("node:sqlite" in v and "probe.ts:1" in v for v in violations), violations
        )

    def test_planted_store_connection_fails(self):
        violations = self.scan_dir(
            'export const url = "postgres://engram:engram@localhost/engram";\n'
        )
        self.assertTrue(
            any("store connection" in v and "probe.ts:1" in v for v in violations), violations
        )

    def test_planted_sql_literal_fails(self):
        violations = self.scan_dir(
            'export const q = "SELECT tenant FROM memories";\n'
        )
        self.assertTrue(
            any("SQL literal" in v and "probe.ts:1" in v for v in violations), violations
        )

    def test_clean_engine_surface_source_passes(self):
        violations = self.scan_dir(
            'import { loadNativeBinding } from "@engram/node";\n'
            "export const provider = loadNativeBinding();\n"
            'export const label = "(unset)";\n'
        )
        self.assertEqual(violations, [])

    def test_missing_declared_root_fails_closed(self):
        violations = gate.scan_paths([Path("/nonexistent/console/src")])
        self.assertTrue(any("missing on disk" in v for v in violations), violations)

    def test_real_console_source_is_clean(self):
        repo_root = Path(__file__).resolve().parents[3]
        self.assertEqual(gate.scan(repo_root), [])


if __name__ == "__main__":
    unittest.main()
