"""Run: python3 -m unittest discover -s evals -v. Uses disposable repositories."""

import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

SKILL = Path(__file__).resolve().parents[1]
SCRIPT = SKILL / "scripts" / "factory.py"
spec = importlib.util.spec_from_file_location("factory_under_test", SCRIPT)
factory = importlib.util.module_from_spec(spec)
spec.loader.exec_module(factory)


class FactoryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="factory-eval-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.repo = self.root / "repo"
        self.repo.mkdir()
        self.git(self.repo, "init", "-b", "main")
        self.git(self.repo, "config", "user.name", "Factory Test")
        self.git(self.repo, "config", "user.email", "factory-test@example.invalid")
        (self.repo / "app.txt").write_text("baseline\n")
        self.git(self.repo, "add", "app.txt")
        self.git(self.repo, "commit", "-m", "baseline")
        self.feature = self.root / "feature.txt"
        self.feature.write_text("Add an audited approval transition for authorized operators.\n")
        self.run_cli("init")
        self.job_id = None

    @staticmethod
    def git(repo, *args):
        result = subprocess.run(["git", "-C", str(repo), *args], capture_output=True, text=True)
        if result.returncode:
            raise AssertionError(result.stderr)
        return result.stdout.strip()

    def run_cli(self, *args, fail=False, revision=None):
        command = [sys.executable, str(SCRIPT), "--repo", str(self.repo)]
        if revision is not None:
            command += ["--expected-revision", str(revision)]
        result = subprocess.run(command + list(args), text=True, capture_output=True)
        if fail:
            self.assertNotEqual(result.returncode, 0, result.stdout)
            self.assertNotIn("Traceback", result.stderr)
            return json.loads(result.stderr)
        self.assertEqual(result.returncode, 0, result.stderr)
        return json.loads(result.stdout)

    def create(self, profiles="full-stack"):
        result = self.run_cli("create", "--feature-file", str(self.feature), "--slug", "approval-flow", "--profiles", profiles)
        self.job_id = result["result"]["job_id"]
        self.jobdir = self.repo / ".engram-factory" / "jobs" / self.job_id
        return result["result"]

    def job(self):
        return self.run_cli("status", "--job", self.job_id)["result"]

    def fill(self, kind, role, inputs, snapshot=None, path=None, **extra):
        job = self.job()
        path = path or kind + ".md"
        fields = {"schema_version": "1.0", "artifact_type": kind, "job_id": self.job_id,
                  "cycle": job["cycle"], "status": "ready", "author_role": role,
                  "profiles": job["profiles"], "source_snapshot": snapshot,
                  "input_artifacts": [factory.ref(self.jobdir, p) for p in inputs],
                  "updated_at": factory.now(), **extra}
        if kind == "analysis":
            fields["required_reviewers"] = getattr(self, "analysis_reviewers", ["code", "security", "platform"])
        output = self.jobdir / path
        output.parent.mkdir(parents=True, exist_ok=True)
        header = "\n".join(key + ": " + json.dumps(value) for key, value in fields.items())
        output.write_text("---\n" + header + "\n---\n\n# Controller fixture\n\n## Evidence\n\n"
                          "Synthetic fixture for exercising controller integrity gates only. "
                          "This is not a real feature analysis, execution result, review or approval.\n"
                          + (extra.get("append") or ""))
        return path

    def upstream(self):
        job = self.job()
        extra = [job["artifacts"]["rework"]["path"]] if "rework" in job["artifacts"] else []
        brief = ["brief.md"] if "brief" in job["artifacts"] else []
        led = "Addresses LED-1: preserved verbatim." if brief else ""
        self.fill("analysis", "engram-analyzer", ["feature.md"] + brief + extra, append=led)
        self.run_cli("complete", "--job", self.job_id, "--stage", "analyze", "--artifacts", "analysis.md")
        self.fill("plan", "engram-planner", ["analysis.md"] + extra)
        self.run_cli("complete", "--job", self.job_id, "--stage", "plan", "--artifacts", "plan.md")
        for kind in ("tasks", "verification", "release"):
            self.fill(kind, "engram-tasks", ["analysis.md", "plan.md"] + extra)
        self.run_cli("complete", "--job", self.job_id, "--stage", "tasks", "--artifacts", "tasks.md", "verification.md", "release.md")

    def build(self, exports=None, reviewers="code,security,platform", fail=False):
        job = self.job()
        worktree = Path(job["worktree"])
        self.build_count = getattr(self, "build_count", 0) + 1
        (worktree / "app.txt").write_text(f"implementation {job['cycle']} {job['round']} {self.build_count}\n")
        self.git(worktree, "add", "app.txt")
        self.git(worktree, "commit", "-m", "implement approval transition")
        env = self.jobdir / "environment.json"
        if not env.exists():
            env.write_text(json.dumps({"environment": "disposable-test", "product": "fixture",
                                      "version": "1", "observed_at": factory.now(), "baseline": "fixture-1"}))
        args = ["snapshot", "--job", self.job_id, "--environment", str(env)]
        if exports:
            args += ["--exports", *map(str, exports)]
        self.run_cli(*args)
        job = self.job()
        snapshot = job["snapshot"]["path"]
        upstream = ["analysis.md", "plan.md"]
        if "rework" in job["artifacts"]:
            upstream += [job["artifacts"]["rework"]["path"]]
        for kind in ("tasks", "verification", "release"):
            self.fill(kind, "engram-builder", upstream, snapshot)
        self.fill("build", "engram-builder", upstream + ["tasks.md", "verification.md", "release.md"], snapshot)
        self.run_cli("complete", "--job", self.job_id, "--stage", "build", "--snapshot", snapshot,
                     "--reviewers", reviewers, "--artifacts", "build.md", "tasks.md", "verification.md", "release.md", fail=fail)

    def review(self, discipline, verdict="PASS", attempt=""):
        job = self.job()
        path = f"cycle-{job['cycle']}/round-{job['round']}/review-{discipline}{attempt}.md"
        self.fill("review", "reviewer-" + discipline, [i["path"] for i in job["artifacts"].values()],
                  job["snapshot"]["path"], path, round=job["round"], discipline=discipline, verdict=verdict)
        return self.run_cli("review", "--job", self.job_id, "--discipline", discipline, "--artifact", path)

    def decision(self, recommendation="APPROVE"):
        job = self.job()
        inputs = [i["path"] for i in job["artifacts"].values()] + [i["path"] for i in job["reviews"].values()]
        self.fill("decision", "engram-approver", inputs, job["snapshot"]["path"], recommendation=recommendation)
        self.run_cli("decision", "--job", self.job_id, "--artifact", "decision.md")

    def to_approval(self):
        self.create()
        self.upstream()
        self.build()
        for reviewer in ("code", "security", "platform"):
            self.review(reviewer)
        self.decision()

    def approve(self):
        instruction = self.root / "approval.txt"
        instruction.write_text("/factory approve " + self.job_id)
        return self.run_cli("approve", "--job", self.job_id, "--actor", "test-human", "--instruction-file", str(instruction))

    def test_happy_path_requires_explicit_approval_and_merges(self):
        self.to_approval()
        self.run_cli("merge", "--job", self.job_id, fail=True)
        self.approve()
        self.run_cli("merge", "--job", self.job_id)
        job = self.job()
        self.assertEqual(job["stage"], "merged")
        self.assertEqual(self.git(self.repo, "rev-parse", "HEAD"), self.git(job["worktree"], "rev-parse", "HEAD"))
        self.assertFalse(self.git(self.repo, "status", "--porcelain"))
        self.assertFalse(self.git(self.repo, "ls-files", ".engram-factory"))

    def test_waits_for_all_reviewers_before_rebuild(self):
        self.create(); self.upstream(); self.build()
        self.review("code", "CHANGES")
        self.assertEqual(self.job()["stage"], "review")
        self.review("security")
        self.assertEqual(self.job()["stage"], "review")
        self.review("platform")
        self.assertEqual(self.job()["stage"], "build")
        self.build()
        self.assertEqual(self.job()["round"], 2)
        self.assertTrue(all(r["verdict"] == "pending" for r in self.job()["reviews"].values()))

    def test_round_limit_cannot_be_reset_by_resume(self):
        self.create(); self.upstream()
        for _ in range(3):
            self.build()
            self.review("code", "CHANGES")
            self.review("security")
            self.review("platform")
        self.assertEqual(self.job()["blocker"]["code"], "round-limit")
        self.run_cli("resume", "--job", self.job_id, "--reason", "try again", fail=True)
        note = self.root / "rework.txt"
        note.write_text("Address unresolved transition defect and revalidate scope.")
        self.run_cli("rework", "--job", self.job_id, "--note-file", str(note))
        self.assertEqual((self.job()["cycle"], self.job()["round"], self.job()["stage"]), (2, 0, "analyze"))
        self.assertIsNone(self.job()["approval"])
        self.upstream()

    def test_manual_block_and_resume_round_trip(self):
        self.create(); self.upstream(); self.build()
        self.run_cli("block", "--job", self.job_id, "--reason", "operator question")
        job = self.job()
        self.assertEqual((job["stage"], job["blocker"]["code"]), ("needs-human", "manual"))
        self.assertEqual(job["resume_from"], {"stage": "review", "status": "running"})
        self.run_cli("resume", "--job", self.job_id, "--reason", "operator answered")
        job = self.job()
        self.assertEqual((job["stage"], job["status"]), ("review", "running"))
        self.assertIsNone(job["blocker"])
        self.assertTrue(all(r["verdict"] == "pending" for r in job["reviews"].values()))

    def test_refresh_base_integrates_advanced_target(self):
        self.create(); self.upstream()
        (self.repo / "other.txt").write_text("concurrent target change\n")
        self.git(self.repo, "add", "other.txt"); self.git(self.repo, "commit", "-m", "concurrent")
        env = self.jobdir / "environment.json"
        env.write_text(json.dumps({"environment": "disposable-test", "product": "fixture",
                                   "version": "1", "observed_at": factory.now(), "baseline": "fixture-1"}))
        self.run_cli("snapshot", "--job", self.job_id, "--environment", str(env), fail=True)
        self.assertEqual(self.job()["stage"], "build")
        worktree = Path(self.job()["worktree"])
        self.git(worktree, "merge", self.job()["target"])
        self.run_cli("refresh-base", "--job", self.job_id)
        self.assertEqual(self.job()["base_sha"], self.git(self.repo, "rev-parse", "main"))
        self.build()
        self.assertEqual(self.job()["round"], 1)

    def test_rework_completes_full_second_cycle_and_merges(self):
        self.create(); self.upstream()
        for _ in range(3):
            self.build()
            self.review("code", "CHANGES"); self.review("security"); self.review("platform")
        note = self.root / "rework.txt"
        note.write_text("Address unresolved transition defect and revalidate scope.")
        self.run_cli("rework", "--job", self.job_id, "--note-file", str(note))
        self.upstream(); self.build()
        for reviewer in ("code", "security", "platform"):
            self.review(reviewer)
        self.decision(); self.approve()
        self.run_cli("merge", "--job", self.job_id)
        job = self.job()
        self.assertEqual((job["cycle"], job["stage"]), (2, "merged"))
        self.assertEqual(self.git(self.repo, "rev-parse", "HEAD"), self.git(job["worktree"], "rev-parse", "HEAD"))

    def test_missing_external_original_falls_back_to_archive(self):
        self.create("salesforce"); self.upstream()
        package = self.root / "metadata.zip"; package.write_bytes(b"fixture-package")
        self.build([package])
        package.unlink()
        result = self.review("code")
        self.assertEqual(result["result"]["reviews"]["code"]["verdict"], "PASS")

    def test_error_recovery_preserves_successful_reviews(self):
        self.create(); self.upstream(); self.build()
        self.review("code", "ERROR"); self.review("security"); self.review("platform")
        old = self.job()["reviews"]["security"].copy()
        self.run_cli("resume", "--job", self.job_id, "--reason", "review environment repaired")
        self.assertEqual(self.job()["reviews"]["security"], old)
        self.review("code", attempt="-attempt-2")
        self.assertEqual(self.job()["stage"], "approve")

    def test_approver_escalation_blocks_generic_resume(self):
        self.create(); self.upstream(); self.build()
        for reviewer in ("code", "security", "platform"):
            self.review(reviewer)
        self.decision("ESCALATE")
        self.assertEqual(self.job()["stage"], "needs-human")
        self.run_cli("resume", "--job", self.job_id, "--reason", "override", fail=True)

    def test_changed_code_invalidates_approval(self):
        self.to_approval()
        worktree = Path(self.job()["worktree"])
        (worktree / "app.txt").write_text("unreviewed change\n")
        self.git(worktree, "add", "app.txt"); self.git(worktree, "commit", "-m", "unreviewed")
        instruction = self.root / "approval.txt"; instruction.write_text("approve")
        self.run_cli("approve", "--job", self.job_id, "--actor", "test", "--instruction-file", str(instruction), fail=True)

    def test_changed_target_invalidates_merge(self):
        self.to_approval(); self.approve()
        (self.repo / "other.txt").write_text("concurrent target change\n")
        self.git(self.repo, "add", "other.txt"); self.git(self.repo, "commit", "-m", "concurrent")
        self.run_cli("merge", "--job", self.job_id, fail=True)
        self.assertEqual(self.job()["stage"], "merge")

    def test_merge_recovers_when_git_finished_before_board_update(self):
        self.to_approval(); self.approve()
        self.git(self.repo, "merge", "--ff-only", self.job()["branch"])
        self.run_cli("merge", "--job", self.job_id)
        self.assertEqual(self.job()["stage"], "merged")

    def test_merge_pins_sha_if_job_branch_moves_after_validation(self):
        self.to_approval(); self.approve()
        board = self.run_cli("status")["result"]
        job = board["jobs"][0]
        approved = json.loads((self.jobdir / job["snapshot"]["path"]).read_text())["head_sha"]
        real_check = factory.source_check

        def move_after_check(*args, **kwargs):
            snapshot = real_check(*args, **kwargs)
            worktree = Path(job["worktree"])
            (worktree / "app.txt").write_text("unreviewed concurrent commit\n")
            self.git(worktree, "add", "app.txt")
            self.git(worktree, "commit", "-m", "branch moved after validation")
            return snapshot

        args = factory.parser().parse_args(["merge", "--job", self.job_id])
        with mock.patch.object(factory, "source_check", side_effect=move_after_check):
            factory.execute(args, self.repo, self.repo / ".engram-factory", board)
        self.assertEqual(self.git(self.repo, "rev-parse", "HEAD"), approved)

    def test_package_drift_blocks_reviews(self):
        self.create("salesforce"); self.upstream()
        package = self.root / "metadata.zip"; package.write_bytes(b"fixture-package")
        self.build([package])
        package.write_bytes(b"different-package")
        job = self.job()
        self.fill("review", "reviewer-code", [i["path"] for i in job["artifacts"].values()], job["snapshot"]["path"],
                  "review-code.md", round=1, discipline="code", verdict="PASS")
        self.run_cli("review", "--job", self.job_id, "--discipline", "code", "--artifact", "review-code.md", fail=True)

    def test_environment_change_blocks_review(self):
        self.create(); self.upstream(); self.build()
        (self.jobdir / "environment.json").write_text('{"environment":"changed"}')
        self.run_cli("review", "--job", self.job_id, "--discipline", "code", "--artifact", "missing.md", fail=True)

    def test_invalid_artifact_does_not_advance_or_increment_revision(self):
        self.create()
        before = self.run_cli("status")["revision"]
        self.run_cli("complete", "--job", self.job_id, "--stage", "analyze", "--artifacts", "analysis.md", fail=True)
        self.assertEqual(self.run_cli("status")["revision"], before)
        self.fill("analysis", "engram-builder", ["feature.md"])
        self.run_cli("complete", "--job", self.job_id, "--stage", "analyze", "--artifacts", "analysis.md", fail=True)
        self.assertEqual(self.job()["stage"], "analyze")

    def test_missing_or_stale_required_input_blocks_handoff(self):
        self.create()
        self.fill("analysis", "engram-analyzer", [])
        self.run_cli("complete", "--job", self.job_id, "--stage", "analyze", "--artifacts", "analysis.md", fail=True)
        self.fill("analysis", "engram-analyzer", ["feature.md"])
        (self.jobdir / "feature.md").write_text("tampered feature")
        self.run_cli("complete", "--job", self.job_id, "--stage", "analyze", "--artifacts", "analysis.md", fail=True)

    def valid_brief(self, ask=None):
        ask = ask or self.feature.read_text()
        return ("# Brief\n\n## The ask\n\n" + ask + "\n## Decision ledger\n\n"
                "| ID | Choice | Authority | Statement / boundary | Revisit trigger |\n"
                "|---|---|---|---|---|\n"
                "| LED-1 | rollout owner | HUMAN-DECIDED | owner picked A: keep behavior | on incident |\n\n"
                "## Owner ratification\n\n"
                "| Actor | Channel | Date | Confirmation (verbatim) |\n"
                "|---|---|---|---|\n"
                "| test-owner | session | 2026-10-25 | \"Ship the sourced-batch behavior.\" |\n")

    def test_brief_is_registered_and_required_by_analysis(self):
        brief = self.root / "brief.md"
        brief.write_text(self.valid_brief())
        result = self.run_cli("create", "--feature-file", str(self.feature), "--slug", "approval-flow", "--brief-file", str(brief))
        self.job_id = result["result"]["job_id"]
        self.jobdir = self.repo / ".engram-factory" / "jobs" / self.job_id
        self.assertIn("brief", self.job()["artifacts"])
        self.fill("analysis", "engram-analyzer", ["feature.md"], append="Notes: none.")
        self.run_cli("complete", "--job", self.job_id, "--stage", "analyze", "--artifacts", "analysis.md", fail=True)
        self.fill("analysis", "engram-analyzer", ["feature.md", "brief.md"], append="Ignores the rollout-owner decision entirely.")
        self.run_cli("complete", "--job", self.job_id, "--stage", "analyze", "--artifacts", "analysis.md", fail=True)
        self.fill("analysis", "engram-analyzer", ["feature.md", "brief.md"], append="Addresses LED-1: preserved verbatim.")
        self.run_cli("complete", "--job", self.job_id, "--stage", "analyze", "--artifacts", "analysis.md")
        self.assertEqual(self.job()["stage"], "plan")
        (self.jobdir / "brief.md").write_text("tampered")
        self.fill("plan", "engram-planner", ["analysis.md"])
        self.run_cli("complete", "--job", self.job_id, "--stage", "plan", "--artifacts", "plan.md", fail=True)

    def test_brief_backward_compatible_without_one(self):
        self.create()  # no --brief-file
        self.assertNotIn("brief", self.job()["artifacts"])
        self.fill("analysis", "engram-analyzer", ["feature.md"])
        self.run_cli("complete", "--job", self.job_id, "--stage", "analyze", "--artifacts", "analysis.md")
        self.assertEqual(self.job()["stage"], "plan")

    def test_empty_brief_is_rejected_at_create(self):
        empty = self.root / "empty-brief.md"
        empty.write_text("   \n")
        self.run_cli("create", "--feature-file", str(self.feature), "--slug", "approval-flow",
                     "--brief-file", str(empty), fail=True)

    def test_brief_must_frame_the_registered_feature(self):
        mismatch = self.root / "brief.md"
        mismatch.write_text(self.valid_brief(ask="Some other older feature entirely.\n"))
        self.run_cli("create", "--feature-file", str(self.feature), "--slug", "approval-flow",
                     "--brief-file", str(mismatch), fail=True)
        hollow = self.root / "hollow.md"
        hollow.write_text(self.valid_brief().replace("| test-owner | session | 2026-10-25 | \"Ship the sourced-batch behavior.\" |", "| — | — | — | — |"))
        self.run_cli("create", "--feature-file", str(self.feature), "--slug", "approval-flow-2",
                     "--brief-file", str(hollow), fail=True)

    def test_brief_requirement_survives_rework_cycle(self):
        brief = self.root / "brief.md"
        brief.write_text(self.valid_brief())
        result = self.run_cli("create", "--feature-file", str(self.feature), "--slug", "approval-flow", "--brief-file", str(brief))
        self.job_id = result["result"]["job_id"]
        self.jobdir = self.repo / ".engram-factory" / "jobs" / self.job_id
        self.upstream()  # analysis..tasks with brief listed
        self.run_cli("block", "--job", self.job_id, "--reason", "owner rework at boundary")
        note = self.root / "rework.txt"
        note.write_text("Re-scope: tighten supersession boundaries.")
        self.run_cli("rework", "--job", self.job_id, "--note-file", str(note))
        # cycle 2: analysis MUST still include feature + rework + brief
        rework_path = self.job()["artifacts"]["rework"]["path"]
        self.fill("analysis", "engram-analyzer", ["feature.md", rework_path])
        self.run_cli("complete", "--job", self.job_id, "--stage", "analyze", "--artifacts", "analysis.md", fail=True)
        self.fill("analysis", "engram-analyzer", ["feature.md", rework_path, "brief.md"], append="Addresses LED-1: preserved verbatim.")
        self.run_cli("complete", "--job", self.job_id, "--stage", "analyze", "--artifacts", "analysis.md")
        self.assertEqual(self.job()["stage"], "plan")

    def test_single_job_and_stale_revision_enforced(self):
        self.create()
        self.run_cli("create", "--feature-file", str(self.feature), "--slug", "another", fail=True)
        self.run_cli("block", "--job", self.job_id, "--reason", "blocked", revision=0, fail=True)

    def test_concurrent_starts_have_one_winner(self):
        command = [sys.executable, str(SCRIPT), "--repo", str(self.repo), "create", "--feature-file", str(self.feature)]
        first = subprocess.Popen(command + ["--slug", "first"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        second = subprocess.Popen(command + ["--slug", "second"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        first.communicate(timeout=20)
        second.communicate(timeout=20)
        self.assertEqual(sorted([first.returncode, second.returncode]), [0, 2])
        self.assertEqual(len(self.run_cli("status")["result"]["jobs"]), 1)

    def test_platform_snapshot_requires_export_evidence(self):
        self.create("maximo"); self.upstream()
        env = self.jobdir / "environment.json"
        env.write_text(json.dumps({"environment": "test", "product": "fixture", "version": "1",
                                   "observed_at": factory.now(), "baseline": "baseline"}))
        self.run_cli("snapshot", "--job", self.job_id, "--environment", str(env), fail=True)

    def test_required_reviewer_cannot_be_omitted(self):
        self.create(); self.upstream(); self.build()
        self.review("code", "CHANGES"); self.review("security"); self.review("platform")
        job = self.job()
        env = self.jobdir / "environment.json"
        self.run_cli("snapshot", "--job", self.job_id, "--environment", str(env))
        snapshot = self.job()["snapshot"]["path"]
        for kind in ("tasks", "verification", "release"):
            self.fill(kind, "engram-builder", ["analysis.md", "plan.md"], snapshot)
        self.fill("build", "engram-builder", ["analysis.md", "plan.md", "tasks.md", "verification.md", "release.md"], snapshot)
        self.run_cli("complete", "--job", self.job_id, "--stage", "build", "--snapshot", snapshot,
                     "--reviewers", "code,security", "--artifacts", "build.md", "tasks.md", "verification.md", "release.md", fail=True)
        self.assertEqual(self.job()["stage"], "build")

    def test_analysis_specific_reviewer_cannot_be_dropped(self):
        self.analysis_reviewers = ["code", "security", "platform", "data"]
        self.create(); self.upstream()
        self.build(fail=True)
        self.assertEqual(self.job()["stage"], "build")
        self.assertIn("data", self.job()["required_reviewers"])

    def test_dirty_primary_prevents_start_but_not_status(self):
        (self.repo / "app.txt").write_text("user work\n")
        self.run_cli("create", "--feature-file", str(self.feature), "--slug", "dirty", fail=True)
        self.assertEqual(self.run_cli("status")["result"]["jobs"], [])

    def test_artifact_cannot_escape_job(self):
        self.create()
        self.run_cli("validate-artifact", "--job", self.job_id, "--artifact", "../outside.md", fail=True)

    def test_archive_tampering_blocks_handoff(self):
        self.create()
        archive = self.jobdir / self.job()["artifacts"]["feature"]["archive"]
        archive.write_text("tampered archive")
        self.fill("analysis", "engram-analyzer", ["feature.md"])
        self.run_cli("complete", "--job", self.job_id, "--stage", "analyze", "--artifacts", "analysis.md", fail=True)

    def test_profiles_compose_into_phase_specific_templates(self):
        self.create("full-stack,salesforce,sap,servicenow,maximo")
        for name in ("analysis", "plan", "tasks", "verification", "release"):
            text = (self.jobdir / (name + ".md")).read_text()
            for profile in factory.PROFILES:
                self.assertIn("## Profile: " + profile, text)
                self.assertIn("{{" + profile + "_" + name + "_evidence}}", text)

    def test_board_matches_published_schema_when_validator_available(self):
        try:
            import jsonschema
        except ImportError:
            self.skipTest("Optional jsonschema package unavailable; core tests use stdlib")
        self.to_approval(); self.approve()
        board = self.run_cli("status")["result"]
        schema = json.loads((SKILL / "assets" / "board.schema.json").read_text())
        jsonschema.Draft202012Validator(schema).validate(board)

    def create_mode(self, mode=None):
        args = ["create", "--feature-file", str(self.feature), "--slug", "approval-flow"]
        if mode is not None:
            args += ["--mode", mode]
        result = self.run_cli(*args)
        self.job_id = result["result"]["job_id"]
        self.jobdir = self.repo / ".engram-factory" / "jobs" / self.job_id
        return result["result"]

    def board_file(self):
        return json.loads((self.repo / ".engram-factory" / "board.json").read_text())

    def rewrite_board(self, mutate):
        boardpath = self.repo / ".engram-factory" / "board.json"
        disk = json.loads(boardpath.read_text())
        mutate(disk)
        boardpath.write_text(json.dumps(disk, indent=2) + "\n")
        return boardpath

    def test_mode_create_light_is_recorded_and_displayed(self):
        job = self.create_mode("light")
        self.assertEqual(job["mode"], "light")
        self.assertEqual(self.job()["mode"], "light")
        self.assertEqual(self.board_file()["jobs"][0]["mode"], "light")

    def test_mode_default_create_records_full_explicitly(self):
        job = self.create_mode()
        self.assertEqual(job["mode"], "full")
        self.assertEqual(self.board_file()["jobs"][0]["mode"], "full")

    def test_mode_legacy_record_without_mode_loads_and_displays_full(self):
        self.create_mode()
        def strip(disk):
            del disk["jobs"][0]["mode"]
        boardpath = self.rewrite_board(strip)
        before = boardpath.read_bytes()
        self.assertEqual(self.job()["mode"], "full")
        self.assertEqual(self.run_cli("status")["result"]["jobs"][0]["mode"], "full")
        self.assertEqual(boardpath.read_bytes(), before)

    def test_mode_unknown_value_refuses_every_command(self):
        self.create_mode()
        def poison(disk):
            disk["jobs"][0]["mode"] = "fast"
        self.rewrite_board(poison)
        self.run_cli("status", fail=True)
        self.run_cli("status", "--job", self.job_id, fail=True)

    def test_mode_create_flag_rejects_unknown_values(self):
        command = [sys.executable, str(SCRIPT), "--repo", str(self.repo), "create",
                   "--feature-file", str(self.feature), "--slug", "approval-flow", "--mode", "fast"]
        result = subprocess.run(command, text=True, capture_output=True)
        self.assertEqual(result.returncode, 2)
        self.assertIn("invalid choice", result.stderr)
        self.assertEqual(self.run_cli("status")["result"]["jobs"], [])

    def test_mode_board_with_mode_validates_against_published_schema(self):
        try:
            import jsonschema
        except ImportError:
            self.skipTest("Optional jsonschema package unavailable; core tests use stdlib")
        self.create_mode("light")
        self.upstream()
        board = self.run_cli("status")["result"]
        self.assertEqual(board["jobs"][0]["mode"], "light")
        schema = json.loads((SKILL / "assets" / "board.schema.json").read_text())
        jsonschema.Draft202012Validator(schema).validate(board)


if __name__ == "__main__":
    unittest.main()
