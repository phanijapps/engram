"""Focused tests for the unintegrated versioned task storage primitive."""
import importlib.util
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "task_index.py"
spec = importlib.util.spec_from_file_location("task_index", SCRIPT)
task_index = importlib.util.module_from_spec(spec)
spec.loader.exec_module(task_index)


class TaskIndexTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.job = Path(self.tmp.name)
        (self.job / "tasks").mkdir()
        self.docs = []
        for number in (1, 2):
            path = self.job / "tasks" / f"task-{number:03}.md"
            path.write_text(f"# Task {number}\n\nBounded handoff with evidence.\n")
            self.docs.append(path)
        self.manifest = {"schema_version": 1, "job_id": "001-example", "cycle": 1,
                         "phases": [{"id": "PHASE-1", "tasks": [
                             {"id": "TASK-001", "document": "tasks/task-001.md",
                              "sha256": task_index.sha(self.docs[0].read_bytes()), "depends_on": []},
                             {"id": "TASK-002", "document": "tasks/task-002.md",
                              "sha256": task_index.sha(self.docs[1].read_bytes()), "depends_on": ["TASK-001"]}]}]}

    def test_status_evidence_and_revision(self):
        state = task_index.initialize(self.job, self.manifest)
        self.assertEqual(state["revision"], 0)
        with self.assertRaisesRegex(task_index.TaskIndexError, "Dependencies"):
            task_index.transition(self.job, "TASK-002", "in_progress", 0, actor="builder")
        state = task_index.transition(self.job, "TASK-001", "in_progress", 0, actor="builder")
        with self.assertRaisesRegex(task_index.TaskIndexError, "Stale"):
            task_index.transition(self.job, "TASK-001", "done", 0, actor="builder")
        with self.assertRaisesRegex(task_index.TaskIndexError, "requires evidence"):
            task_index.transition(self.job, "TASK-001", "done", 1, actor="builder")
        evidence = self.job / "proof.txt"
        evidence.write_text("check ran")
        state = task_index.transition(self.job, "TASK-001", "done", 1,
                                     [{"path": "proof.txt", "sha256": task_index.sha(evidence.read_bytes())}], actor="builder")
        self.assertEqual(state["tasks"]["TASK-001"]["status"], "done")
        self.assertEqual(task_index.transition(self.job, "TASK-002", "in_progress", 2, actor="builder")["revision"], 3)

    def test_doc_drift_and_incomplete_init_fail_closed(self):
        task_index.initialize(self.job, self.manifest)
        self.docs[0].write_text("changed")
        with self.assertRaisesRegex(task_index.TaskIndexError, "hash mismatch"):
            task_index.transition(self.job, "TASK-001", "in_progress", 0, actor="builder")
        with self.assertRaises(task_index.TaskIndexError):
            task_index.initialize(self.job, self.manifest)

    def test_cycle_and_path_escape_rejected(self):
        self.manifest["phases"][0]["tasks"][0]["depends_on"] = ["TASK-002"]
        with self.assertRaisesRegex(task_index.TaskIndexError, "cycle"):
            task_index.initialize(self.job, self.manifest)
        with self.assertRaises(task_index.TaskIndexError):
            task_index.confined(self.job, "../outside")


if __name__ == "__main__":
    unittest.main()
