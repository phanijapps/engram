#!/usr/bin/env python3
"""Evals for engram-constitution scripts: scaffold + validate on tmp repos."""
import json, subprocess, sys, tempfile, unittest
from pathlib import Path

SKILL = Path(__file__).resolve().parents[1]
SCAFFOLD = ["python3", str(SKILL / "scripts/scaffold.py")]
VALIDATE = ["python3", str(SKILL / "scripts/validate.py")]
LINT = ["python3", str(SKILL / "scripts/lint.py")]

class ScaffoldTests(unittest.TestCase):
    def run_cli(self, *args, cmd=SCAFFOLD):
        return subprocess.run(cmd + list(args), capture_output=True, text=True)

    def test_greenfield_scaffold(self):
        with tempfile.TemporaryDirectory() as td:
            r = self.run_cli("--repo", td, "--mode", "greenfield")
            self.assertEqual(r.returncode, 0, r.stderr)
            C = Path(td) / "docs/constitution"
            self.assertTrue((C / "00-INDEX.md").is_file())
            for p in ("product", "architecture", "dev", "testing", "ci", "security", "release", "platform"):
                self.assertTrue((C / p / "00-INDEX.md").is_file(), p)
                self.assertTrue((C / p / "knowledge.md").is_file(), p)
                self.assertTrue((C / p / "rules").is_dir(), p)
            v = self.run_cli(str(Path(td)), cmd=VALIDATE)
            self.assertEqual(v.returncode, 0, v.stderr)

    def test_idempotent_refuses_overwrite(self):
        with tempfile.TemporaryDirectory() as td:
            self.run_cli("--repo", td, "--mode", "greenfield")
            r = self.run_cli("--repo", td, "--mode", "greenfield")
            self.assertEqual(r.returncode, 2)

    def test_merged_packs(self):
        with tempfile.TemporaryDirectory() as td:
            r = self.run_cli("--repo", td, "--mode", "greenfield", "--packs", "product,architecture,ci")
            self.assertEqual(r.returncode, 0, r.stderr)
            self.assertTrue((Path(td)/"docs/constitution/ci/00-INDEX.md").is_file())
            self.assertFalse((Path(td)/"docs/constitution/testing").exists())

    def test_platform_pack_default(self):
        with tempfile.TemporaryDirectory() as td:
            self.run_cli("--repo", td, "--mode", "greenfield")
            self.assertTrue((Path(td)/"docs/constitution/platform/00-INDEX.md").is_file())
            self.assertTrue((Path(td)/"docs/constitution/platform/rules/PLT-001.md").is_file())

    def test_module_scaffold(self):
        with tempfile.TemporaryDirectory() as td:
            self.run_cli("--repo", td, "--mode", "greenfield")
            r = self.run_cli("--repo", td, "--mode", "brownfield", "--module", "payments")
            self.assertEqual(r.returncode, 0, r.stderr)
            self.assertTrue((Path(td)/"docs/constitution/modules/payments/00-INDEX.md").is_file())
            self.assertTrue((Path(td)/"docs/constitution/modules/payments/product/rules/PRD-001.md").is_file())

    def test_module_requires_root(self):
        with tempfile.TemporaryDirectory() as td:
            r = self.run_cli("--repo", td, "--mode", "brownfield", "--module", "orphan")
            self.assertEqual(r.returncode, 2)

    def test_derive_catalogue(self):
        import json as j
        with tempfile.TemporaryDirectory() as td:
            self.run_cli("--repo", td, "--mode", "greenfield")
            r = subprocess.run(["python3", str(SKILL/"scripts/derive_catalogue.py"), td, "--write"],
                               capture_output=True, text=True)
            self.assertEqual(r.returncode, 0, r.stderr)
            cat = j.loads((Path(td)/"docs/constitution/registry/constitution-catalogue.json").read_text())
            self.assertTrue(any(e["rule_id"].startswith("PLT-") for e in cat["packs"]))

    def test_health_detects_missing_manifest(self):
        with tempfile.TemporaryDirectory() as td:
            self.run_cli("--repo", td, "--mode", "greenfield")
            # create an orphan fragment
            kd = Path(td)/"docs/constitution/architecture/knowledge"
            kd.mkdir(parents=True, exist_ok=True)
            (kd/"orphan-topic.md").write_text("# orphan\n")
            subprocess.run(["python3", str(SKILL/"scripts/derive_catalogue.py"), td, "--write"], capture_output=True)
            r = subprocess.run(["python3", str(SKILL/"scripts/health.py"), td],
                               capture_output=True, text=True)
            self.assertIn("MANIFEST-MISSING", r.stderr)

    def test_unknown_pack_rejected(self):
        with tempfile.TemporaryDirectory() as td:
            r = self.run_cli("--repo", td, "--mode", "greenfield", "--packs", "bogus")
            self.assertEqual(r.returncode, 2)

    def test_catalogue_rule_path_validation(self):
        with tempfile.TemporaryDirectory() as td:
            self.run_cli("--repo", td, "--mode", "greenfield")
            cat = Path(td)/"docs/constitution/registry/constitution-catalogue.json"
            d = json.loads(cat.read_text())
            d["packs"] = [{"rule_id":"PRD-999","pack":"product","path":"docs/constitution/product/rules/PRD-999.md"}]
            cat.write_text(json.dumps(d))
            v = self.run_cli(str(Path(td)), cmd=VALIDATE)
            self.assertNotEqual(v.returncode, 0)  # missing rule file must fail

    def test_lint_strict_flag(self):
        with tempfile.TemporaryDirectory() as td:
            self.run_cli("--repo", td, "--mode", "greenfield")
            a = self.run_cli(str(Path(td)), cmd=LINT)
            self.assertEqual(a.returncode, 0)
            s = self.run_cli(str(Path(td)), "--strict", cmd=LINT)
            # placeholder sentences may exceed caps -> strict may fail; must not crash
            self.assertIn(s.returncode, (0, 1))

if __name__ == "__main__":
    unittest.main()
