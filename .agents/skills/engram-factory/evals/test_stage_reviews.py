"""Isolated pre-build review state and safety tests; no live factory state."""
import importlib.util
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "stage_reviews.py"
spec = importlib.util.spec_from_file_location("stage_reviews", SCRIPT)
reviews = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reviews)


class StageReviewTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.job = Path(self.tmp.name)
        (self.job / "spec.md").write_text("initial requirement")
        self.roster = {"red-forman": "independent-1", "security": "security-1"}

    def freeze(self, stage="spec", phase=None, roster=None, paths=None):
        return reviews.freeze_candidate(self.job, stage, phase, "writer-1", paths or ["spec.md"], roster or self.roster)

    def review(self, candidate=1, role="red-forman", verdict="PASS", findings=None, resolves=None, stage="spec", phase=None):
        invocation = self.roster[role]
        return reviews.record_review(self.job, stage, phase, candidate, role, invocation, verdict, findings or {}, resolves or [])

    def test_complete_review_is_bound_to_current_artifacts(self):
        candidate = self.freeze()
        self.assertEqual(reviews.review_status(self.job, "spec")["verdict"], "PENDING")
        self.review()
        self.review(role="security")
        self.assertEqual(reviews.require_stage_pass(self.job, "spec", candidate["artifacts"])["verdict"], "PASS")
        with self.assertRaises(ValueError):
            reviews.require_stage_pass(self.job, "spec", [])
        (self.job / "spec.md").write_text("changed after review")
        with self.assertRaises(ValueError):
            reviews.require_stage_pass(self.job, "spec", candidate["artifacts"])

    def test_findings_require_revision_and_explicit_recheck(self):
        self.freeze()
        self.review(verdict="CHANGES", findings={"F-1": "Missing failure case"})
        self.review(role="security")
        self.assertEqual(reviews.review_status(self.job, "spec")["verdict"], "CHANGES")
        with self.assertRaises(ValueError):
            self.freeze()
        (self.job / "spec.md").write_text("failure case added")
        self.freeze()
        self.review(candidate=2)  # An unsubstantiated PASS doesn't clear F-1.
        self.review(candidate=2, role="security")
        self.assertEqual(reviews.review_status(self.job, "spec")["verdict"], "CHANGES")
        (self.job / "spec.md").write_text("more clarification")
        self.freeze()
        self.review(candidate=3, resolves=["F-1"])
        self.review(candidate=3, role="security")
        self.assertEqual(reviews.review_status(self.job, "spec")["verdict"], "PASS")
        with self.assertRaises(FileExistsError):
            self.review(candidate=3)

    def test_all_reviewers_wait_and_error_can_retry_unchanged(self):
        self.freeze()
        self.review(verdict="ERROR")
        self.assertEqual(reviews.review_status(self.job, "spec")["verdict"], "PENDING")
        self.review(role="security")
        self.assertEqual(reviews.review_status(self.job, "spec")["verdict"], "ERROR")
        self.freeze()  # Retry after access is restored; old receipt preserved.
        self.review(candidate=2)
        self.review(candidate=2, role="security")
        self.assertEqual(reviews.review_status(self.job, "spec")["verdict"], "PASS")

    def test_reviewers_cannot_be_dropped_or_impersonate_author(self):
        with self.assertRaises(ValueError):
            self.freeze(roster={"red-forman": "writer-1"})
        self.freeze()
        self.review(verdict="CHANGES", findings={"F-1": "A"})
        self.review(role="security")
        (self.job / "spec.md").write_text("revision")
        with self.assertRaises(ValueError):
            self.freeze(roster={"red-forman": "independent-1"})
        with self.assertRaises(ValueError):
            reviews.record_review(self.job, "spec", None, 1, "red-forman", "writer-1", "PASS", {}, [])

    def test_each_task_phase_has_independent_review(self):
        self.freeze(stage="tasks", phase="phase-1")
        self.review(stage="tasks", phase="phase-1")
        self.review(stage="tasks", phase="phase-1", role="security")
        self.assertEqual(reviews.review_status(self.job, "tasks", "phase-1")["verdict"], "PASS")
        with self.assertRaises(ValueError):
            reviews.review_status(self.job, "tasks", "phase-2")
        with self.assertRaises(ValueError):
            self.freeze(stage="tasks")

    def test_noncanonical_or_escaping_artifacts_rejected(self):
        for path in ("../spec.md", "./spec.md", "/etc/passwd"):
            with self.subTest(path=path), self.assertRaises(ValueError):
                self.freeze(paths=[path])

    def test_prior_candidate_bytes_are_preserved_and_tamper_detected(self):
        candidate = self.freeze()
        archive = self.job / "reviews" / "prebuild" / "spec" / "artifacts" / candidate["artifacts"][0]["sha256"]
        self.assertEqual(archive.read_text(), "initial requirement")
        self.review(verdict="CHANGES", findings={"F-1": "Missing failure case"})
        self.review(role="security")
        (self.job / "spec.md").write_text("revised requirement")
        self.freeze()
        self.assertEqual(archive.read_text(), "initial requirement")
        self.review(candidate=2, resolves=["F-1"])
        self.review(candidate=2, role="security")
        archive.write_text("tampered")
        with self.assertRaisesRegex(ValueError, "Archived candidate bytes changed"):
            reviews.review_status(self.job, "spec")

    def test_clarification_does_not_waive_findings(self):
        self.freeze()
        self.review(verdict="CHANGES", findings={"F-1": "Scope unclear"})
        self.review(role="security")
        reviews.record_clarification(self.job, "spec", None, "Which scope?", "Include both.", "owner", "message-42")
        self.assertEqual(reviews.review_status(self.job, "spec")["verdict"], "CHANGES")
        with self.assertRaises(ValueError):
            reviews.record_clarification(self.job, "spec", None, "Which scope?", "", "owner", "message-42")

    def test_third_changes_blocks_spec_and_plan_without_fourth_candidate(self):
        for stage in ("spec", "plan"):
            with self.subTest(stage=stage):
                for number in range(1, 4):
                    if number > 1:
                        (self.job / "spec.md").write_text(f"{stage} revision {number}")
                    self.freeze(stage=stage)
                    self.review(candidate=number, verdict="CHANGES", findings={f"F-{number}": "Unresolved"}, stage=stage)
                    if number == 3:
                        self.assertEqual(reviews.review_status(self.job, stage)["verdict"], "PENDING")
                    self.review(candidate=number, role="security", stage=stage)
                status = reviews.review_status(self.job, stage)
                self.assertEqual(status["verdict"], "NEEDS_HUMAN")
                self.assertEqual(status["outstanding"]["red-forman"], ["F-1", "F-2", "F-3"])
                with self.assertRaisesRegex(ValueError, "limit exhausted"):
                    self.freeze(stage=stage)

    def test_third_review_can_pass_and_task_phases_do_not_share_cap(self):
        self.freeze()
        self.review(verdict="CHANGES", findings={"F-1": "Missing case"})
        self.review(role="security")
        (self.job / "spec.md").write_text("revision two")
        self.freeze()
        self.review(candidate=2, verdict="CHANGES", findings={"F-2": "Another case"}, resolves=["F-1"])
        self.review(candidate=2, role="security")
        (self.job / "spec.md").write_text("revision three")
        third = self.freeze()
        self.review(candidate=3, resolves=["F-2"])
        self.review(candidate=3, role="security")
        self.assertEqual(reviews.require_stage_pass(self.job, "spec", third["artifacts"])["verdict"], "PASS")
        for number in range(1, 5):
            (self.job / "spec.md").write_text(f"phase revision {number}")
            self.freeze(stage="tasks", phase="phase-1")
            self.review(candidate=number, verdict="CHANGES", findings={f"P-{number}": "Issue"}, stage="tasks", phase="phase-1")
            self.review(candidate=number, role="security", stage="tasks", phase="phase-1")
        self.assertEqual(reviews.review_status(self.job, "tasks", "phase-1")["candidate"], 4)

    def test_third_error_cannot_be_used_to_bypass_limit(self):
        for number in range(1, 4):
            self.freeze()
            self.review(candidate=number, verdict="ERROR")
            self.review(candidate=number, role="security")
            self.assertEqual(reviews.review_status(self.job, "spec")["verdict"], "ERROR")
        with self.assertRaisesRegex(ValueError, "limit exhausted"):
            self.freeze()


if __name__ == "__main__":
    unittest.main()
