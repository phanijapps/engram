#!/usr/bin/env python3
"""Unit tests for check_engine_neutrality (EN-004 fail-closed, job 011)."""
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import check_engine_neutrality as gate


class FailClosed(unittest.TestCase):
    def test_missing_declared_path_fails_with_path_named(self):
        violations = gate.scan([Path("/nonexistent/gated/path")])
        self.assertTrue(any("missing on disk" in v and "/nonexistent/gated/path" in v
                            for v in violations), violations)

    def test_present_path_scans_without_missing_violation(self):
        import tempfile
        with tempfile.TemporaryDirectory() as td:
            good = Path(td) / "src"
            good.mkdir()
            (good / "a.rs").write_text("pub fn neutral() {}\n")
            violations = gate.scan([good])
            self.assertFalse(any("missing on disk" in v for v in violations), violations)


if __name__ == "__main__":
    unittest.main()
