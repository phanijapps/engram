"""Run: python3 -m unittest discover -s evals -v. Uses disposable repositories."""

import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
from current_fixtures import SINGLE, PLAN

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
        self.brief_file = self.root / "brief.md"
        self.brief_file.write_text(FactoryTests.valid_brief(self))

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
        result = subprocess.run(command + list(args) + (["--quiesced"] if args[0] == "block" else []), text=True, capture_output=True)
        if fail:
            self.assertNotEqual(result.returncode, 0, result.stdout)
            self.assertNotIn("Traceback", result.stderr)
            return json.loads(result.stderr)
        self.assertEqual(result.returncode, 0, result.stderr)
        return json.loads(result.stdout)

    def create(self, profiles="full-stack"):
        result = self.run_cli("create", "--feature-file", str(self.feature), "--slug", "approval-flow",
                              "--profiles", profiles, "--brief-file", str(self.brief_file))
        self.job_id = result["result"]["job_id"]
        self.jobdir = self.repo / ".engram-factory" / "jobs" / self.job_id
        self.run_cli("activate", "--job", self.job_id)
        return result["result"]

    def job(self):
        return self.run_cli("status", "--job", self.job_id)["result"]

    def fill(self, kind, role, inputs, snapshot=None, path=None, **extra):
        job = self.job()
        path = path or kind + ".md"
        output = self.jobdir / path
        output.parent.mkdir(parents=True, exist_ok=True)
        if kind in {"spec", "plan"}:
            text = SINGLE if kind == "spec" else PLAN
            if kind == "spec":
                reviewers = getattr(self, "analysis_reviewers", ["code", "security", "platform"])
                text = text.replace('required_reviewers: ["code", "security", "platform"]', 'required_reviewers: ' + json.dumps(reviewers))
                if "mode" in extra:
                    text = text.replace('scope: "single"', 'scope: "single"\nmode: ' + json.dumps(extra["mode"]))
            else:
                supports = [p for p in inputs if p in factory.plan_document.CONTEXT]
                if supports:
                    text += '\n## Supporting documents\n| Path | Purpose |\n| --- | --- |\n'
                    text += ''.join('| ' + p + ' | Independently maintained fixture design contract |\n' for p in supports)
            output.write_text(text + (extra.get("append") or ""))
            # Fixture authoring prepares only current compact contracts; invalid drafts
            # deliberately remain unprepared for the caller's negative assertion.
            subprocess.run([sys.executable, str(SCRIPT), "--repo", str(self.repo), kind + "-prepare", "--job", self.job_id],
                           capture_output=True, text=True)
            return path
        fields = {"schema_version": "1.0", "artifact_type": kind, "job_id": self.job_id,
                  "cycle": job["cycle"], "status": "ready", "author_role": role,
                  "profiles": job["profiles"], "source_snapshot": snapshot,
                  "input_artifacts": [factory.ref(self.jobdir, p) for p in inputs],
                  "updated_at": factory.now(), **extra}
        if kind == "build":
            fields.setdefault("invocation_id", "builder-1")
        if kind in {"review", "decision"}:
            fields.setdefault("invocation_id", "review-" + str(extra.get("discipline", "approver")) + "-" + str(job["round"]))
        if path.startswith("context/"):
            output.write_text('# Fixture design contract\n\nThis plain document defines the independently maintained fixture boundary and failure contract for source-based review.\n')
        else:
            header = "\n".join(key + ": " + json.dumps(value) for key, value in fields.items())
            output.write_text("---\n" + header + "\n---\n\n# Controller fixture\n\n## Evidence\n\n"
                              "Synthetic disposable controller fixture, not a claim of real execution or owner authority. REQ-001 AC-001 AC-002\n"
                              + (extra.get("append") or ""))
        return path

    def upstream(self):
        self.fill("spec", "engram-analyzer", [])
        self.accepted_complete("complete", "--job", self.job_id, "--stage", "analyze", "--artifacts", "spec.md")
        self.fill("plan", "engram-planner", [])
        self.accepted_complete("complete", "--job", self.job_id, "--stage", "plan", "--artifacts", "plan.md")
        self.accept_tasks()

    def build(self, exports=None, reviewers="code,security,platform", fail=False):
        job = self.job(); worktree = Path(job["worktree"])
        self.build_count = getattr(self, "build_count", 0) + 1
        (worktree / "app.txt").write_text(f"implementation {job['cycle']} {job['round']} {self.build_count}\n")
        self.git(worktree, "add", "app.txt"); self.git(worktree, "commit", "-m", "fixture implementation")
        env = self.jobdir / "environment.json"
        if not env.exists():
            env.write_text(json.dumps({"environment": "disposable-test", "product": "fixture", "version": "1", "observed_at": factory.now(), "baseline": "fixture-1"}))
        args = ["snapshot", "--job", self.job_id, "--environment", str(env)]
        if exports:
            args += ["--exports", *map(str, exports)]
        self.run_cli(*args)
        job = self.job(); snapshot = job["snapshot"]["path"]
        path = f"records/operations/build-handoff-c{job['cycle']}-r{job['round'] + 1}.md"
        self.fill("build", "engram-builder", [x["path"] for x in job["artifacts"].values()] + ["tasks/task.json", snapshot], snapshot, path)
        self.run_cli("complete", "--job", self.job_id, "--stage", "build", "--snapshot", snapshot, "--reviewers", reviewers, "--artifacts", path, fail=fail)

    def review(self, discipline, verdict="PASS", attempt=""):
        job = self.job()
        path = f"reviews/cycles/cycle-{job['cycle']}/round-{job['round']}/review-{discipline}.md"
        self.fill("review", "reviewer-" + discipline, [i["path"] for i in job["artifacts"].values()],
                  job["snapshot"]["path"], path, round=job["round"], discipline=discipline, verdict=verdict)
        return self.run_cli("review", "--job", self.job_id, "--discipline", discipline, "--artifact", path)

    def decision(self, recommendation="APPROVE"):
        job = self.job()
        inputs = [i["path"] for i in job["artifacts"].values()] + [i["path"] for i in job["reviews"].values()]
        self.fill("decision", "engram-approver", inputs, job["snapshot"]["path"], path="reviews/final-review.md", recommendation=recommendation)
        self.run_cli("decision", "--job", self.job_id, "--artifact", "reviews/final-review.md")

    def accepted_complete(self, *args):
        stage = args[args.index("--stage") + 1]
        review_stage = "spec" if stage == "analyze" else stage
        roster = self.jobdir / "records/operations/reviewers.json"
        findings = self.jobdir / "records/operations/findings.json"
        roster.write_text(json.dumps({"red-forman": "prebuild-reviewer"}))
        findings.write_text(json.dumps({"findings": {}, "resolves": []}))
        self.run_cli("review-freeze", "--job", self.job_id, "--stage", review_stage, "--author", "phase-author", "--reviewers-file", "records/operations/reviewers.json")
        self.run_cli("review-record", "--job", self.job_id, "--stage", review_stage, "--candidate", "1", "--role", "red-forman", "--invocation", "prebuild-reviewer", "--verdict", "PASS", "--findings-file", "records/operations/findings.json")
        return self.run_cli(*args)

    def accept_tasks(self):
        job = self.job()
        task = self.jobdir / "tasks/task-001.md"
        task.write_text("# Deliver fixture behavior\n\nImplement REQ-001 and verify AC-001/AC-002 through the accepted boundary with the planned fixture.\n")
        manifest = {"schema_version": 1, "job_id": self.job_id, "cycle": job["cycle"], "phases": [{"id": "PHASE-1", "tasks": [{"id": "TASK-001", "document": "tasks/task-001.md", "sha256": factory.digest(task), "depends_on": []}]}]}
        (self.jobdir / "records/operations/candidate.json").write_text(json.dumps(manifest))
        self.run_cli("tasks-initialize", "--job", self.job_id, "--manifest", "records/operations/candidate.json")
        required = [item["path"] for key,item in job["artifacts"].items() if key in {"spec", "plan", "architecture", "data-models", "interfaces", "contracts"}]
        self.fill("release", "engram-tasks", required)
        self.run_cli("review-freeze", "--job", self.job_id, "--stage", "tasks", "--phase", "PHASE-1", "--author", "task-author", "--reviewers-file", "records/operations/reviewers.json")
        self.run_cli("review-record", "--job", self.job_id, "--stage", "tasks", "--phase", "PHASE-1", "--candidate", "1", "--role", "red-forman", "--invocation", "prebuild-reviewer", "--verdict", "PASS", "--findings-file", "records/operations/findings.json")
        self.run_cli("complete", "--job", self.job_id, "--stage", "tasks", "--artifacts", "tasks/manifest.json", "tasks/task.json", "release.md")
        instruction = self.root / "start.txt"; instruction.write_text("Accept this exact plan and start implementation.")
        self.run_cli("start-build", "--job", self.job_id, "--actor", "fixture-owner", "--instruction-file", str(instruction), "--plan-sha256", self.job()["artifacts"]["plan"]["sha256"], "--decision", "accept-and-start")
        self.run_cli("task-transition", "--job", self.job_id, "--task", "TASK-001", "--to", "in_progress", "--task-revision", "0", "--actor", "builder-1", "--reason", "fixture execution")
        self.run_cli("task-transition", "--job", self.job_id, "--task", "TASK-001", "--to", "done", "--task-revision", "1", "--actor", "builder-1", "--reason", "fixture proof", "--evidence", "tasks/task-001.md")

    def to_approval(self):
        self.create()
        self.upstream()
        self.build()
        for reviewer in ("code", "security", "platform"):
            self.review(reviewer)
        self.decision()

    def approve(self):
        instruction = self.root / "approval.txt"
        instruction.write_text("Approve the exact reviewed snapshot for source merge.")
        return self.run_cli("approve", "--job", self.job_id, "--actor", "test-human", "--instruction-file", str(instruction), "--decision", "approve")

    def register(self, slug, priority=0):
        return self.run_cli("create", "--feature-file", str(self.feature), "--slug", slug,
                            "--priority", str(priority), "--brief-file", str(self.brief_file))["result"]

    def test_design_artifacts_are_pinned_required_and_tamper_evident(self):
        self.create(); self.fill('spec', 'engram-analyzer', [])
        self.accepted_complete('complete', '--job', self.job_id, '--stage', 'analyze', '--artifacts', 'spec.md')
        for name in ['architecture', 'data-models']:
            self.fill(name, 'engram-planner', [], path='context/' + name + '.md')
        supports = ['context/architecture.md', 'context/data-models.md']
        self.fill('plan', 'engram-planner', ['spec.md', *supports])
        self.run_cli('complete', '--job', self.job_id, '--stage', 'plan', '--artifacts', 'plan.md', fail=True)
        self.accepted_complete('complete', '--job', self.job_id, '--stage', 'plan', '--artifacts', 'plan.md', *supports)
        job = self.job()
        self.assertTrue({'architecture','data-models'} <= set(job['artifacts']))
        path = self.jobdir / supports[0]; original = path.read_bytes(); path.write_text('Changed accepted design')
        with self.assertRaises(ValueError):
            factory.intact(self.jobdir, job['artifacts']['architecture'])
        path.write_bytes(original)
        self.accept_tasks()
        self.run_cli('block', '--job', self.job_id, '--reason', 'test owner rework')
        note=self.root/'note.txt'; note.write_text('Revisit the design without deleting evidence.')
        self.run_cli('rework', '--job', self.job_id, '--note-file', str(note))
        self.assertEqual(original, (self.jobdir/'history/cycle-1/context/architecture.md').read_bytes())

    def test_queue_priority_ties_and_blocked_jobs(self):
        a, b = self.register("first"), self.register("second")
        self.assertEqual(self.run_cli("status")["result"]["queue_order"], [a["job_id"], b["job_id"]])
        self.run_cli("activate", "--job", b["job_id"], fail=True)
        self.run_cli("priority", "--job", b["job_id"], "--priority", "5", "--actor", "owner", "--reason", "urgent")
        self.assertEqual(self.run_cli("status")["result"]["queue_order"], [b["job_id"], a["job_id"]])
        self.run_cli("activate", "--job", b["job_id"])
        (Path(b["worktree"]) / "app.txt").write_text("unfinished source")
        self.run_cli("block", "--job", b["job_id"], "--reason", "awaiting owner")
        self.register("third")
        self.run_cli("activate", "--job", a["job_id"])
        self.run_cli("resume", "--job", b["job_id"], "--reason", "owner returned", fail=True)

    def test_normal_merge_and_stale_blocked_job_do_not_wedge_intake(self):
        self.to_approval()
        self.approve()
        self.run_cli("merge", "--job", self.job_id)
        next_job = self.register("after-merge")
        self.run_cli("activate", "--job", next_job["job_id"])
        self.run_cli("block", "--job", next_job["job_id"], "--reason", "owner needed")
        (self.repo / "app.txt").write_text("target moves independently")
        self.git(self.repo, "add", "app.txt")
        self.git(self.repo, "commit", "-m", "target change")
        later = self.register("after-stale-block")
        self.run_cli("activate", "--job", later["job_id"])
        self.assertEqual(self.run_cli("status", "--job", later["job_id"])["result"]["base_sha"], self.git(self.repo, "rev-parse", "main"))

    def test_retirement_is_reversible_and_preserves_evidence(self):
        job = self.register("retire")
        before = job["artifacts"]
        self.run_cli("cancel", "--job", job["job_id"], "--actor", "owner", "--reason", "defer")
        self.assertEqual(self.run_cli("status", "--job", job["job_id"])["result"]["artifacts"], before)
        self.run_cli("restore", "--job", job["job_id"], "--actor", "owner", "--reason", "resume queue")
        self.run_cli("activate", "--job", job["job_id"])
        self.run_cli("cancel", "--job", job["job_id"], "--actor", "owner", "--reason", "unsafe", fail=True)

    def test_missing_board_reports_orphans_and_refuses_init(self):
        self.create()
        boardpath = self.repo / ".engram-factory" / "board.json"
        boardpath.unlink()
        report = self.run_cli("integrity")
        kinds = {i["kind"] for i in report["issues"]}
        self.assertTrue({"missing-board", "orphan-branch", "orphan-worktree", "orphan-directory"} <= kinds)
        self.run_cli("init", fail=True)
        self.assertFalse(boardpath.exists())

    def test_recovery_preview_preserves_state_and_apply_is_explicitly_blocked(self):
        self.create()
        boardpath = self.repo / ".engram-factory" / "board.json"
        backup = self.root / "backup.json"
        backup.write_bytes(boardpath.read_bytes())
        boardpath.unlink()
        report = self.run_cli("recover", "--board-file", str(backup))
        self.assertEqual(report["issues"], [])
        self.assertFalse(boardpath.exists())
        self.run_cli("recover", "--board-file", str(backup), "--apply-sha256", report["candidate_sha256"], "--actor", "owner", fail=True)
        self.assertFalse(boardpath.exists())

    def test_recovery_never_accepts_manufactured_history(self):
        job = self.register("queued")
        boardpath = self.repo / ".engram-factory" / "board.json"
        forged = json.loads(boardpath.read_text())
        forged["jobs"][0]["history"].append({"at": factory.now(), "action": "human-approval", "detail": "invented"})
        backup = self.root / "forged.json"
        backup.write_text(json.dumps(forged))
        boardpath.unlink()
        report = self.run_cli("recover", "--board-file", str(backup))
        self.assertFalse(report["apply_supported"])
        self.run_cli("recover", "--board-file", str(backup), "--apply-sha256", report["candidate_sha256"], "--actor", "owner", fail=True)
        self.assertFalse(boardpath.exists())

    def test_damaged_evidence_blocks_recovery_without_mutating(self):
        self.create()
        boardpath = self.repo / ".engram-factory" / "board.json"
        backup = self.root / "backup.json"
        backup.write_bytes(boardpath.read_bytes())
        (self.jobdir / "feature.md").write_text("damaged")
        boardpath.unlink()
        report = self.run_cli("recover", "--board-file", str(backup))
        self.assertIn("damaged-evidence", {i["kind"] for i in report["issues"]})
        self.run_cli("recover", "--board-file", str(backup), "--apply-sha256", report["candidate_sha256"], "--actor", "owner", fail=True)
        self.assertFalse(boardpath.exists())

    def test_registration_failpoints_preserve_inspectable_orphans(self):
        for phase in ("record", "scaffold_v3", "board-commit"):
            with self.subTest(phase=phase):
                if phase != "record":
                    self.setUp()
                boardpath = self.repo / ".engram-factory" / "board.json"
                before = boardpath.read_bytes()
                args = ["--repo", str(self.repo), "create", "--feature-file", str(self.feature), "--slug", "interrupted", "--brief-file", str(self.brief_file)]
                if phase in {"record", "scaffold_v3"}:
                    real = getattr(factory, phase)
                    def crash_after(*args, _real=real, **kwargs):
                        _real(*args, **kwargs)
                        raise OSError("simulated registration failure")
                    patcher = mock.patch.object(factory, phase, side_effect=crash_after)
                else:
                    real = factory.atomic
                    def fail_board(path, value):
                        if path == boardpath:
                            raise OSError("simulated board replacement failure")
                        return real(path, value)
                    patcher = mock.patch.object(factory, "atomic", side_effect=fail_board)
                with patcher, self.assertRaises(OSError):
                    factory.main(args)
                self.assertEqual(boardpath.read_bytes(), before)
                report = self.run_cli("integrity")
                self.assertIn("orphan-directory", {i["kind"] for i in report["issues"]})
                artifact = self.repo / ".engram-factory" / "jobs" / "001-interrupted" / "feature.md"
                self.assertEqual(artifact.read_bytes(), self.feature.read_bytes())
                self.run_cli("create", "--feature-file", str(self.feature), "--slug", "retry", "--brief-file", str(self.brief_file), fail=True)
                self.assertEqual(artifact.read_bytes(), self.feature.read_bytes())

    def test_activation_crash_is_detectable_and_retry_non_destructive(self):
        job = self.register("interrupted")
        board = self.board_file()
        args = factory.parser().parse_args(["activate", "--job", job["job_id"]])
        real_git = factory.git
        def crash_after_git(*args):
            result = real_git(*args)
            if "worktree" in args and "add" in args:
                raise OSError("simulated process failure after Git")
            return result
        with mock.patch.object(factory, "git", side_effect=crash_after_git):
            with self.assertRaises(OSError):
                factory.execute(args, self.repo, self.repo / ".engram-factory", board)
        report = self.run_cli("integrity")
        self.assertIn("queued-source-remnant", {i["kind"] for i in report["issues"]})
        head = self.git(job["worktree"], "rev-parse", "HEAD")
        self.run_cli("activate", "--job", job["job_id"], fail=True)
        self.assertEqual(self.git(job["worktree"], "rev-parse", "HEAD"), head)
        self.run_cli("recover-activation", "--job", job["job_id"], "--actor", "owner", "--reason", "finish recorded activation", "--quiesced")
        restored = self.run_cli("status", "--job", job["job_id"])["result"]
        self.assertEqual(restored["base_sha"], head)
        self.assertEqual(restored["stage"], "analyze")
        self.run_cli("recover-activation", "--job", job["job_id"], "--actor", "owner", "--reason", "duplicate", "--quiesced", fail=True)

    def test_branch_only_interrupted_activation_reuses_exact_intended_branch(self):
        job = self.register("branch-only")
        board = self.board_file()
        args = factory.parser().parse_args(["activate", "--job", job["job_id"]])
        real_git = factory.git
        def interrupt_before_worktree(*args):
            if "worktree" in args and "add" in args:
                real_git(self.repo, "branch", job["branch"], args[-1])
                raise OSError("branch created; worktree interrupted")
            return real_git(*args)
        with mock.patch.object(factory, "git", side_effect=interrupt_before_worktree):
            with self.assertRaises(OSError):
                factory.execute(args, self.repo, self.repo / ".engram-factory", board)
        self.assertFalse(Path(job["worktree"]).exists())
        base = self.git(self.repo, "rev-parse", job["branch"])
        self.run_cli("recover-activation", "--job", job["job_id"], "--actor", "owner", "--reason", "complete interrupted worktree", "--quiesced")
        self.assertEqual(self.git(job["worktree"], "rev-parse", "HEAD"), base)

    def test_interrupted_activation_target_drift_cannot_be_recovered(self):
        job = self.register("target-drift")
        board = self.board_file()
        args = factory.parser().parse_args(["activate", "--job", job["job_id"]])
        factory.execute(args, self.repo, self.repo / ".engram-factory", board)
        (self.repo / "app.txt").write_text("new target")
        self.git(self.repo, "add", "app.txt")
        self.git(self.repo, "commit", "-m", "target advances")
        original = self.git(job["worktree"], "rev-parse", "HEAD")
        self.run_cli("recover-activation", "--job", job["job_id"], "--actor", "owner", "--reason", "restore", "--quiesced", fail=True)
        self.assertEqual(self.git(job["worktree"], "rev-parse", "HEAD"), original)
        self.assertEqual(self.run_cli("status", "--job", job["job_id"])["result"]["base_sha"], None)

    def test_queued_rework_does_not_bypass_activation(self):
        job = self.register("queued-rework")
        self.run_cli("block", "--job", job["job_id"], "--reason", "scope clarification")
        note = self.root / "rework.txt"
        note.write_text("Owner clarification before execution")
        self.run_cli("rework", "--job", job["job_id"], "--note-file", str(note))
        restored = self.run_cli("status", "--job", job["job_id"])["result"]
        self.assertEqual((restored["stage"], restored["status"], restored["base_sha"]), ("queued", "queued", None))
        self.assertFalse(Path(job["worktree"]).exists())
        self.run_cli("activate", "--job", job["job_id"])

    def test_recovery_of_changed_activation_source_is_blocked(self):
        job = self.register("interrupted-dirty")
        board = self.board_file()
        args = factory.parser().parse_args(["activate", "--job", job["job_id"]])
        factory.execute(args, self.repo, self.repo / ".engram-factory", board)
        (Path(job["worktree"]) / "app.txt").write_text("valuable uncommitted changes")
        self.run_cli("recover-activation", "--job", job["job_id"], "--actor", "owner", "--reason", "restore", "--quiesced", fail=True)
        self.assertEqual((Path(job["worktree"]) / "app.txt").read_text(), "valuable uncommitted changes")
        self.assertEqual(self.run_cli("status", "--job", job["job_id"])["result"]["stage"], "queued")

    def test_old_board_is_rejected_before_lock_backup_or_migration(self):
        job = self.register('old-board')
        path = self.rewrite_board(lambda board: board.update(schema_version='1.0'))
        lock = path.parent / 'board.lock'; lock.unlink()
        before = {str(p):p.read_bytes() for p in path.parent.rglob('*') if p.is_file()}
        self.run_cli('status', fail=True)
        self.run_cli('activate', '--job', job['job_id'], fail=True)
        after = {str(p):p.read_bytes() for p in path.parent.rglob('*') if p.is_file()}
        self.assertEqual(before, after)
        self.assertFalse(lock.exists())
        self.assertFalse((path.parent / 'board-v1-backup.json').exists())

    def test_compact_spec_trace_errors_are_rejected_before_review(self):
        self.create()
        for text in [SINGLE.replace('AC-001, AC-002', 'AC-999'), SINGLE.replace('REQ-001 | absent input', 'REQ-999 | absent input')]:
            (self.jobdir / 'spec.md').write_text(text)
            before = (self.repo / '.engram-factory/board.json').read_bytes()
            self.run_cli('spec-check', '--job', self.job_id)
            result = self.run_cli('spec-check', '--job', self.job_id)['result']
            self.assertEqual('FAIL', result['structural'])
            self.assertEqual(before, (self.repo / '.engram-factory/board.json').read_bytes())
            self.assertFalse((self.jobdir / 'reviews/prebuild/spec').exists())

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
        for _ in range(2):
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
        for _ in range(2):
            self.build()
            self.review("code", "CHANGES"); self.review("security"); self.review("platform")
        self.assertEqual(self.job()["blocker"]["code"], "round-limit")
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
        self.create(); before=self.run_cli('status')['revision']
        self.run_cli('complete','--job',self.job_id,'--stage','analyze','--artifacts','spec.md',fail=True)
        self.assertEqual(before,self.run_cli('status')['revision'])
        self.fill('spec','engram-analyzer',[])
        receipt=self.jobdir/'records/spec/receipt.json'; data=json.loads(receipt.read_text())
        data['fields']['author_role']='engram-builder'; receipt.write_text(json.dumps(data))
        self.run_cli('preflight','--job',self.job_id,'--stage','spec',fail=True)
        self.assertEqual('analyze',self.job()['stage'])

    def test_missing_or_stale_required_input_blocks_handoff(self):
        self.create(); self.fill('spec', 'engram-analyzer', [])
        receipt=self.jobdir/'records/spec/receipt.json'; data=json.loads(receipt.read_text())
        data['fields']['input_artifacts']=[]; receipt.write_text(json.dumps(data))
        self.run_cli('preflight','--job',self.job_id,'--stage','spec',fail=True)
        (self.jobdir/'feature.md').write_text('Changed accepted feature')
        self.run_cli('spec-prepare','--job',self.job_id,fail=True)
        self.assertEqual('analyze',self.job()['stage'])

    def valid_brief(self, ask=None):
        ask = ask or self.feature.read_text()
        return ('---\nstatus: "ratified"\n---\n\n# Synthetic brief\n\n## The ask\n\n' + ask
                + '\n## Decision ledger\n\nLED-1: preserve the fixture boundary.\n\n## Owner ratification\n\n'
                + '| Actor | Channel | Date | Confirmation (verbatim) |\n|---|---|---|---|\n'
                + '| test-owner | session | 2026-10-25 | "Ship the sourced-batch behavior." |\n')

    def test_brief_is_registered_and_required_by_analysis(self):
        self.create(); self.assertIn('brief',self.job()['artifacts'])
        self.fill('spec','engram-analyzer',[])
        path=self.jobdir/'spec.md'; path.write_text(path.read_text().replace('LED-1','unrecorded decision'))
        result=self.run_cli('spec-check','--job',self.job_id)['result']
        self.assertEqual('FAIL',result['structural'])
        self.fill('spec','engram-analyzer',[])
        self.accepted_complete('complete','--job',self.job_id,'--stage','analyze','--artifacts','spec.md')
        (self.jobdir/'brief.md').write_text('Changed accepted brief')
        self.fill('plan','engram-planner',[])
        self.run_cli('plan-prepare','--job',self.job_id,fail=True)
        self.assertEqual('plan',self.job()['stage'])

    def test_missing_brief_is_rejected_before_registration(self):
        before = (self.repo / '.engram-factory/board.json').read_bytes()
        result = subprocess.run([sys.executable, str(SCRIPT), '--repo', str(self.repo), 'create', '--feature-file', str(self.feature), '--slug', 'no-brief'], capture_output=True, text=True)
        self.assertNotEqual(0, result.returncode)
        self.assertEqual(before, (self.repo / '.engram-factory/board.json').read_bytes())
        self.assertEqual([], list((self.repo / '.engram-factory/jobs').glob('*')) if (self.repo / '.engram-factory/jobs').exists() else [])

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
        self.create(); self.upstream()
        self.run_cli('block','--job',self.job_id,'--reason','owner rework')
        note=self.root/'note.txt'; note.write_text('Revalidate the same behavior and scope.')
        self.run_cli('rework','--job',self.job_id,'--note-file',str(note))
        self.fill('spec','engram-analyzer',[])
        data=json.loads((self.jobdir/'records/spec/receipt.json').read_text())
        inputs={r['path'] for r in data['fields']['input_artifacts']}
        self.assertTrue({'feature.md','brief.md',self.job()['artifacts']['rework']['path']} <= inputs)
        self.accepted_complete('complete','--job',self.job_id,'--stage','analyze','--artifacts','spec.md')
        self.assertEqual('plan',self.job()['stage'])

    def test_single_job_and_stale_revision_enforced(self):
        self.create()
        second = self.run_cli("create", "--feature-file", str(self.feature), "--slug", "another", "--brief-file", str(self.brief_file))["result"]
        self.run_cli("activate", "--job", second["job_id"], fail=True)
        self.run_cli("block", "--job", self.job_id, "--reason", "blocked", revision=0, fail=True)

    def test_concurrent_starts_have_one_winner(self):
        a = self.run_cli("create", "--feature-file", str(self.feature), "--slug", "first", "--brief-file", str(self.brief_file))["result"]
        b = self.run_cli("create", "--feature-file", str(self.feature), "--slug", "second", "--brief-file", str(self.brief_file))["result"]
        command = [sys.executable, str(SCRIPT), "--repo", str(self.repo), "activate", "--job"]
        first = subprocess.Popen(command + [a["job_id"]], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        second = subprocess.Popen(command + [b["job_id"]], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        first.communicate(timeout=20)
        second.communicate(timeout=20)
        self.assertEqual(sorted([first.returncode, second.returncode]), [0, 2])
        self.assertEqual(len(self.run_cli("status")["result"]["jobs"]), 2)

    def test_platform_snapshot_requires_export_evidence(self):
        self.create("maximo"); self.upstream()
        env = self.jobdir / "environment.json"
        env.write_text(json.dumps({"environment": "test", "product": "fixture", "version": "1",
                                   "observed_at": factory.now(), "baseline": "baseline"}))
        self.run_cli("snapshot", "--job", self.job_id, "--environment", str(env), fail=True)

    def test_required_reviewer_cannot_be_omitted(self):
        self.create(); self.upstream(); self.build()
        self.review('code','CHANGES'); self.review('security'); self.review('platform')
        self.build(reviewers='code,security',fail=True)
        self.assertEqual('build',self.job()['stage'])

    def test_analysis_specific_reviewer_cannot_be_dropped(self):
        self.analysis_reviewers = ["code", "security", "platform", "data"]
        self.create(); self.upstream()
        self.build(fail=True)
        self.assertEqual(self.job()["stage"], "build")
        self.assertIn("data", self.job()["required_reviewers"])

    def test_dirty_primary_prevents_start_but_not_status(self):
        (self.repo / "app.txt").write_text("user work\n")
        job = self.run_cli("create", "--feature-file", str(self.feature), "--slug", "dirty", "--brief-file", str(self.brief_file))["result"]
        self.run_cli("activate", "--job", job["job_id"], fail=True)
        self.assertEqual(self.run_cli("status")["result"]["jobs"][0]["base_sha"], None)

    def test_artifact_cannot_escape_job(self):
        self.create()
        self.run_cli("validate-artifact", "--job", self.job_id, "--artifact", "../outside.md", fail=True)

    def test_archive_tampering_blocks_handoff(self):
        self.create()
        archive = self.jobdir / self.job()["artifacts"]["feature"]["archive"]
        archive.write_text("tampered archive")
        self.fill("spec", "engram-analyzer", ["feature.md"])
        self.run_cli("complete", "--job", self.job_id, "--stage", "analyze", "--artifacts", "spec.md", fail=True)

    def test_current_scaffold_is_compact_and_keeps_selected_profiles(self):
        self.create('full-stack,salesforce,sap,servicenow,maximo')
        for name,marker in [('spec','spec_format: "compact-v1"'),('plan','plan_format: "compact-v1"')]:
            text = (self.jobdir / (name + '.md')).read_text()
            self.assertIn(marker, text)
            self.assertNotIn('input_artifacts:', text)
        self.assertFalse((self.jobdir / 'tasks.md').exists())
        self.assertFalse((self.jobdir / 'verification.md').exists())
        self.assertEqual(set(factory.PROFILES), set(self.job()['profiles']))

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
        args = ["create", "--feature-file", str(self.feature), "--slug", "approval-flow", "--brief-file", str(self.brief_file)]
        if mode is not None:
            args += ["--mode", mode]
        result = self.run_cli(*args)
        self.job_id = result["result"]["job_id"]
        self.jobdir = self.repo / ".engram-factory" / "jobs" / self.job_id
        self.run_cli("activate", "--job", self.job_id)
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

    def test_old_job_layout_is_rejected_without_adoption(self):
        self.create_mode()
        path = self.rewrite_board(lambda board: board['jobs'][0].update(layout_version=1))
        lock = path.parent / 'board.lock'; lock.unlink()
        before = path.read_bytes()
        self.run_cli('status', '--job', self.job_id, fail=True)
        self.run_cli('priority', '--job', self.job_id, '--priority', '9', '--actor', 'owner', '--reason', 'not adoption', fail=True)
        self.assertEqual(before, path.read_bytes())
        self.assertFalse(lock.exists())

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

    def analyze_declaring(self, mode, extra=()):
        self.fill("spec", "engram-analyzer", ["feature.md", *extra], mode=mode)
        self.accepted_complete("complete", "--job", self.job_id, "--stage", "analyze", "--artifacts", "spec.md")

    def test_mode_declaration_applies_on_unpinned_job(self):
        self.create_mode()
        self.analyze_declaring("light")
        job = self.job()
        self.assertEqual(job["mode"], "light")
        self.assertEqual(job["stage"], "plan")
        self.assertFalse([e for e in job["history"] if e["action"] == "mode-refused"])

    def test_mode_declaration_matching_pin_is_a_no_op(self):
        self.create_mode("light")
        self.analyze_declaring("light")
        job = self.job()
        self.assertEqual(job["mode"], "light")
        self.assertEqual(job["stage"], "plan")
        self.assertFalse([e for e in job["history"] if e["action"] == "mode-refused"])

    def test_mode_declaration_refused_on_create_pinned_job(self):
        self.create_mode("full")
        self.analyze_declaring("light")
        job = self.job()
        self.assertEqual(job["mode"], "full")
        self.assertEqual(job["stage"], "plan")
        refusals = [e for e in job["history"] if e["action"] == "mode-refused"]
        self.assertEqual(len(refusals), 1)
        self.assertEqual(refusals[0]["detail"], {"declared": "light", "pinned": "full"})

    def test_mode_declaration_invalid_value_is_rejected(self):
        self.create_mode()
        self.fill("spec", "engram-analyzer", ["feature.md"], mode="fast")
        result = self.run_cli("spec-check", "--job", self.job_id)["result"]
        self.assertEqual("FAIL", result["structural"])
        self.assertIn("mode", " ".join(result["findings"]).lower())
        self.assertEqual(self.job()["mode"], "full")

    def test_mode_pin_superseded_by_rework_note(self):
        self.create_mode("full")
        self.upstream()
        self.run_cli("block", "--job", self.job_id, "--reason", "owner rework")
        note = self.root / "rework.txt"
        note.write_text("Owner override: run this job in light mode.")
        self.run_cli("rework", "--job", self.job_id, "--note-file", str(note))
        self.assertEqual((self.job()["cycle"], self.job()["stage"]), (2, "analyze"))
        rework_path = self.job()["artifacts"]["rework"]["path"]
        self.analyze_declaring("light", [rework_path])
        job = self.job()
        self.assertEqual(job["mode"], "light")
        self.assertEqual(job["stage"], "plan")
        self.assertFalse([e for e in job["history"] if e["action"] == "mode-refused"])

    def upstream_light(self):
        self.create_mode("light")
        self.upstream()

    def review_light(self, discipline, verdict="PASS", role="red-forman", attempt="", fail=False):
        job = self.job()
        path = f"reviews/cycles/cycle-{job['cycle']}/round-{job['round']}/review-{discipline}.md"
        self.fill("review", role, [i["path"] for i in job["artifacts"].values()],
                  job["snapshot"]["path"], path, round=job["round"], discipline=discipline, verdict=verdict)
        return self.run_cli("review", "--job", self.job_id, "--discipline", discipline, "--artifact", path, fail=fail)

    def test_light_mode_scaffolds_same_per_discipline_files_with_red_forman_author(self):
        self.upstream_light()
        self.build()
        job = self.job()
        self.assertEqual(job["mode"], "light")
        self.assertEqual(sorted(job["reviews"]), ["code", "platform", "security"])
        self.assertEqual(sorted(job["required_reviewers"]), ["code", "platform", "security"])
        self.assertTrue(all(r["verdict"] == "pending" for r in job["reviews"].values()))
        for reviewer in ("code", "security", "platform"):
            path = self.jobdir / "reviews/cycles/cycle-1" / "round-1" / ("review-" + reviewer + ".md")
            self.assertTrue(path.is_file(), "missing scaffold: " + reviewer)
            self.assertIn('author_role: "red-forman"', path.read_text())

    def test_light_author_contract_full_mode_still_scaffolds_specialist_authors(self):
        self.create(); self.upstream(); self.build()
        for reviewer in ("code", "security", "platform"):
            path = self.jobdir / "reviews/cycles/cycle-1" / "round-1" / ("review-" + reviewer + ".md")
            self.assertIn('author_role: "reviewer-' + reviewer + '"', path.read_text())

    def test_light_mode_full_geometry_rejects_red_forman_author(self):
        self.create(); self.upstream(); self.build()
        before = self.run_cli("status")["revision"]
        error = self.review_light("code", role="red-forman", fail=True)
        self.assertIn("Wrong artifact author role", error["error"])
        self.assertEqual(self.run_cli("status")["revision"], before)
        self.assertEqual(self.job()["reviews"]["code"]["verdict"], "pending")

    def test_light_mode_accepts_red_forman_only_and_keeps_other_validations(self):
        self.upstream_light(); self.build()
        result = self.review_light("code")
        self.assertEqual(result["result"]["reviews"]["code"]["verdict"], "PASS")
        before = self.run_cli("status")["revision"]
        error = self.review_light("security", role="reviewer-security", fail=True)
        self.assertIn("Wrong artifact author role", error["error"])
        self.assertEqual(self.run_cli("status")["revision"], before)
        error = self.review_light("platform", verdict="BLOCKER", fail=True)
        self.assertIn("Invalid verdict", error["error"])
        self.review_light("security"); self.review_light("platform")
        self.assertEqual(self.job()["stage"], "approve")

    def test_light_mode_end_to_end(self):
        self.upstream_light()
        self.build()
        self.assertEqual(self.job()["mode"], "light")
        self.review_light("code", "CHANGES")
        self.review_light("security")
        self.review_light("platform")
        job = self.job()
        self.assertEqual((job["stage"], job["round"]), ("build", 1))
        self.build()
        job = self.job()
        self.assertEqual(job["round"], 2)
        error = self.review_light("platform", verdict="BLOCKER", fail=True)
        self.assertIn("Invalid verdict", error["error"])
        prior = ("\n## Prior findings status\n\n"
                 "| Prior finding | Status | Verified by |\n"
                 "|---|---|---|\n"
                 "| FIND-code-001 | resolved | rebuild commit · factory eval rerun |\n")
        path = f"reviews/cycles/cycle-{job['cycle']}/round-{job['round']}/review-code.md"
        self.fill("review", "red-forman", [i["path"] for i in job["artifacts"].values()],
                  job["snapshot"]["path"], path, round=job["round"], discipline="code",
                  verdict="PASS", append=prior)
        self.run_cli("review", "--job", self.job_id, "--discipline", "code", "--artifact", path)
        recorded = self.job()["reviews"]["code"]
        self.assertEqual(recorded["verdict"], "PASS")
        self.assertIn("Prior findings status", (self.jobdir / recorded["path"]).read_text())
        self.review_light("security")
        self.review_light("platform")
        self.decision()
        self.assertEqual((self.job()["stage"], self.job()["status"]), ("approve", "awaiting-approval"))
        try:
            import jsonschema
        except ImportError:
            jsonschema = None
        if jsonschema is not None:
            schema = json.loads((SKILL / "assets" / "board.schema.json").read_text())
            jsonschema.Draft202012Validator(schema).validate(self.run_cli("status")["result"])

    def test_removed_layout_options_reject_without_registration_writes(self):
        before = {str(p):p.read_bytes() for p in (self.repo / '.engram-factory').rglob('*') if p.is_file()}
        for choice in ['legacy', 'v3-preview']:
            result = subprocess.run([sys.executable,str(SCRIPT),'--repo',str(self.repo),'create','--feature-file',str(self.feature),'--brief-file',str(self.brief_file),'--slug','no-layout','--layout',choice],capture_output=True,text=True)
            self.assertNotEqual(0,result.returncode)
            self.assertIn('unrecognized arguments',result.stderr)
            self.assertNotIn('Traceback',result.stderr)
        self.assertEqual(before,{str(p):p.read_bytes() for p in (self.repo / '.engram-factory').rglob('*') if p.is_file()})

    def test_noncompact_current_job_artifact_rejected_before_lock_or_mutation(self):
        self.create()
        path=self.jobdir/'spec.md'; path.write_text('---\nschema_version: "1.0"\nartifact_type: "spec"\n---\nOld source retained.\n')
        lock=self.repo/'.engram-factory/board.lock';lock.unlink()
        before={str(p):p.read_bytes() for p in (self.repo/'.engram-factory').rglob('*') if p.is_file()}
        self.run_cli('status','--job',self.job_id,fail=True)
        self.run_cli('priority','--job',self.job_id,'--priority','3','--actor','owner','--reason','no adoption',fail=True)
        self.assertEqual(before,{str(p):p.read_bytes() for p in (self.repo/'.engram-factory').rglob('*') if p.is_file()})
        self.assertFalse(lock.exists())

    def test_missing_current_layout_is_not_inferred(self):
        self.create()
        path=self.rewrite_board(lambda board:board['jobs'][0].pop('layout_version'))
        before=path.read_bytes()
        self.run_cli('status','--job',self.job_id,fail=True)
        self.assertEqual(before,path.read_bytes())

    def test_execution_artifact_aliases_are_not_accepted(self):
        self.create();self.fill('release','engram-tasks',['feature.md'])
        path=self.jobdir/'release.md'; original=path.read_text()
        self.run_cli('validate-artifact','--job',self.job_id,'--artifact','release.md')
        for invalid in [original.replace('schema_version: "1.0"','schema_version: 2'),original.replace('artifact_type:','artifact:')]:
            path.write_text(invalid)
            self.run_cli('validate-artifact','--job',self.job_id,'--artifact','release.md',fail=True)
        path.write_text(original)


    def test_removed_monolithic_handoffs_fail_before_lock_or_artifact_writes(self):
        self.create()
        lock=self.repo/'.engram-factory/board.lock';lock.unlink()
        before={str(p):p.read_bytes() for p in (self.repo/'.engram-factory').rglob('*') if p.is_file()}
        cases=[(['complete','--job',self.job_id,'--stage','tasks','--artifacts','tasks.md','verification.md','release.md'],'Current tasks'),
               (['complete','--job',self.job_id,'--stage','build','--artifacts','build.md','tasks.md','verification.md','release.md'],'Current build'),
               (['review','--job',self.job_id,'--discipline','code','--artifact','cycle-1/round-1/review-code.md'],'Current review'),
               (['decision','--job',self.job_id,'--artifact','decision.md'],'Current decision')]
        for args,message in cases:
            result=self.run_cli(*args,fail=True)
            self.assertIn(message,result['error'])
            self.assertEqual(before,{str(p):p.read_bytes() for p in (self.repo/'.engram-factory').rglob('*') if p.is_file()})
            self.assertFalse(lock.exists())



if __name__ == "__main__":
    unittest.main()
