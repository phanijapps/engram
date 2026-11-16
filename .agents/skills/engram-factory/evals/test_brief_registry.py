import concurrent.futures
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from brief_registry import Registry, RegistryError


class BriefRegistryTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.registry = Registry(self.root)

    def create(self, **kwargs):
        params = {"actor": "owner", "instruction": "approve this brief", "expected_revision": 0,
                  "operation_id": "op-1"}
        params.update(kwargs)
        return self.registry.create(b"First brief\n", **params)

    def test_revision_history_immutable_and_authority_not_authenticated(self):
        first = self.create()
        second = self.registry.supersede("brief-001", b"Second brief\n", actor="owner", instruction="revise",
                                         expected_revision=1, operation_id="op-2")
        self.assertEqual(first["path"], "brief-001.md")
        self.assertEqual(second["path"], "brief-001-r002.md")
        self.assertEqual(self.registry.get("brief-001", 1)[1], b"First brief\n")
        self.assertEqual(self.registry.get("brief-001")[1], b"Second brief\n")
        self.assertIn("unverified", second["authority"])

    def test_idempotent_retry_and_conflict(self):
        first = self.create()
        self.assertEqual(first, self.create())
        with self.assertRaisesRegex(RegistryError, "different request"):
            self.registry.create(b"other", actor="owner", instruction="approve this brief",
                                 expected_revision=0, operation_id="op-1")
        with self.assertRaisesRegex(RegistryError, "stale"):
            self.create(operation_id="op-3")

    def test_tampering_and_orphan_fail_closed(self):
        self.create()
        (self.root / "briefs" / "brief-001.md").write_text("modified")
        with self.assertRaisesRegex(RegistryError, "modified"):
            self.registry.list()

    def test_interrupted_index_write_refuses_silent_adoption(self):
        with patch.object(self.registry, "_write_index", side_effect=OSError("simulated crash")):
            with self.assertRaises(OSError):
                self.create()
        with self.assertRaisesRegex(RegistryError, "reconciliation"):
            self.create()
        self.assertEqual((self.root / "briefs" / "brief-001.md").read_bytes(), b"First brief\n")

    def test_concurrent_writers_cannot_share_revision(self):
        def writer(n):
            try:
                return self.registry.create(f"brief {n}".encode(), actor="owner", instruction="record",
                                            expected_revision=0, operation_id=f"op-{n}")
            except RegistryError:
                return None
        with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
            results = list(pool.map(writer, (1, 2)))
        self.assertEqual(sum(value is not None for value in results), 1)
        self.assertEqual(self.registry.list()["revision"], 1)

    def test_rejects_paths_and_symlink_directory(self):
        with self.assertRaises(RegistryError):
            self.registry.supersede("../bad", b"x", actor="o", instruction="x", expected_revision=0,
                                    operation_id="x")
        other = Path(self.tmp.name) / "other"
        other.mkdir()
        state = Path(self.tmp.name) / "state"
        state.mkdir()
        (state / "briefs").symlink_to(other, target_is_directory=True)
        with self.assertRaises(RegistryError):
            Registry(state)


if __name__ == "__main__":
    unittest.main()
