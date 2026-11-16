"""Disposable controller coverage for the v3 pre-build flow, owner-receipt-bound Start and gate-bound execution."""
import json
import unittest
from pathlib import Path
import test_factory


class CurrentWorkflowTests(unittest.TestCase):
    setUp = test_factory.FactoryTests.setUp
    git = staticmethod(test_factory.FactoryTests.git)
    run_cli = test_factory.FactoryTests.run_cli
    job = test_factory.FactoryTests.job
    fill = test_factory.FactoryTests.fill

    def setUpCurrent(self):
        brief = self.root / "brief.md"
        brief.write_text(test_factory.FactoryTests.valid_brief(self))
        result = self.run_cli("create", "--feature-file", str(self.feature), "--slug", "v3-example", "--brief-file", str(brief))
        self.job_id = result["result"]["job_id"]
        self.jobdir = self.repo / ".engram-factory" / "jobs" / self.job_id
        self.run_cli("activate", "--job", self.job_id)
        self.assertFalse((self.jobdir / "tasks.md").exists())
        self.assertTrue((self.jobdir / "tasks").is_dir())
        (self.jobdir / "records/operations/reviewers.json").write_text(json.dumps({"red-forman": "reviewer-1"}))
        (self.jobdir / "records/operations/findings.json").write_text(json.dumps({"findings": {}, "resolves": []}))

    def test_spec_requires_red_forman_pass_and_gates_before_execution(self):
        self.setUpCurrent()
        self.fill("spec", "engram-analyzer", ["feature.md", "brief.md"], append="Addresses LED-1: preserved verbatim.")
        self.run_cli("complete", "--job", self.job_id, "--stage", "analyze", "--artifacts", "spec.md", fail=True)
        self.run_cli("review-freeze", "--job", self.job_id, "--stage", "spec", "--author", "analyzer-1",
                     "--reviewers-file", "records/operations/reviewers.json")
        self.run_cli("review-record", "--job", self.job_id, "--stage", "spec", "--candidate", "1",
                     "--role", "red-forman", "--invocation", "reviewer-1", "--verdict", "PASS",
                     "--findings-file", "records/operations/findings.json")
        self.run_cli("complete", "--job", self.job_id, "--stage", "analyze", "--artifacts", "spec.md")
        self.assertEqual(self.job()["stage"], "plan")
        self.run_cli("snapshot", "--job", self.job_id, "--environment", "missing.json", fail=True)

    def test_accepted_review_receipt_tamper_blocks_downstream(self):
        self.setUpCurrent()
        self.fill("spec", "engram-analyzer", ["feature.md", "brief.md"], append="Addresses LED-1: preserved verbatim.")
        self.run_cli("review-freeze", "--job", self.job_id, "--stage", "spec", "--author", "analyzer-1",
                     "--reviewers-file", "records/operations/reviewers.json")
        self.run_cli("review-record", "--job", self.job_id, "--stage", "spec", "--candidate", "1",
                     "--role", "red-forman", "--invocation", "reviewer-1", "--verdict", "PASS",
                     "--findings-file", "records/operations/findings.json")
        self.run_cli("complete", "--job", self.job_id, "--stage", "analyze", "--artifacts", "spec.md")
        receipt = self.jobdir / "reviews/prebuild/spec/review-0001-red-forman.json"
        receipt.write_text(receipt.read_text() + " ")
        self.fill("plan", "engram-planner", ["spec.md"])
        self.run_cli("review-freeze", "--job", self.job_id, "--stage", "plan", "--author", "author-1",
                     "--reviewers-file", "records/operations/reviewers.json", fail=True)

    def test_changed_candidate_cannot_advance(self):
        self.setUpCurrent()
        self.fill("spec", "engram-analyzer", ["feature.md", "brief.md"], append="Addresses LED-1: preserved verbatim.")
        self.run_cli("review-freeze", "--job", self.job_id, "--stage", "spec", "--author", "analyzer-1",
                     "--reviewers-file", "records/operations/reviewers.json")
        self.run_cli("review-record", "--job", self.job_id, "--stage", "spec", "--candidate", "1",
                     "--role", "red-forman", "--invocation", "reviewer-1", "--verdict", "PASS",
                     "--findings-file", "records/operations/findings.json")
        (self.jobdir / "spec.md").write_text((self.jobdir / "spec.md").read_text() + "changed")
        self.run_cli("complete", "--job", self.job_id, "--stage", "analyze", "--artifacts", "spec.md", fail=True)

    def test_third_spec_review_changes_blocks_board_and_preserves_history(self):
        self.setUpCurrent()
        self.fill("spec", "engram-analyzer", ["feature.md", "brief.md"], append="Addresses LED-1: preserved verbatim.")
        findings = self.jobdir / "records/operations/findings.json"
        for number in range(1, 4):
            if number > 1:
                spec_path = self.jobdir / "spec.md"
                spec_path.write_text(spec_path.read_text() + f"\nRevision {number}.\n")
                self.run_cli("spec-prepare", "--job", self.job_id)
            findings.write_text(json.dumps({"findings": {f"F-{number}": "Needs correction"}, "resolves": []}))
            self.run_cli("review-freeze", "--job", self.job_id, "--stage", "spec", "--author", "author-1",
                         "--reviewers-file", "records/operations/reviewers.json")
            result = self.run_cli("review-record", "--job", self.job_id, "--stage", "spec", "--candidate", str(number),
                                  "--role", "red-forman", "--invocation", "reviewer-1", "--verdict", "CHANGES",
                                  "--findings-file", "records/operations/findings.json")
        self.assertEqual(result["result"]["review_status"]["verdict"], "NEEDS_HUMAN")
        self.assertEqual(self.job()["stage"], "needs-human")
        self.assertEqual(self.job()["blocker"]["code"], "prebuild-exhausted")
        self.assertEqual(len(list((self.jobdir / "reviews/prebuild/spec").glob("candidate-*.json"))), 3)
        status = self.run_cli("review-status", "--job", self.job_id, "--stage", "spec")
        self.assertEqual(status["result"]["verdict"], "NEEDS_HUMAN")
        self.run_cli("review-clarification", "--job", self.job_id, "--stage", "spec", "--question", "Which scope?",
                     "--answer", "Keep both", "--owner", "owner", "--reference", "message-1")
        self.run_cli("resume", "--job", self.job_id, "--reason", "Try to bypass", fail=True)
        self.run_cli("review-freeze", "--job", self.job_id, "--stage", "spec", "--author", "author-1",
                     "--reviewers-file", "records/operations/reviewers.json", fail=True)

    def test_prebuild_error_resume_checks_integrity_without_build_snapshot(self):
        self.setUpCurrent()
        self.fill("spec", "engram-analyzer", ["feature.md", "brief.md"], append="Addresses LED-1: preserved verbatim.")
        self.run_cli("review-freeze", "--job", self.job_id, "--stage", "spec", "--author", "author-1",
                     "--reviewers-file", "records/operations/reviewers.json")

        self.run_cli("review-record", "--job", self.job_id, "--stage", "spec", "--candidate", "1",
                     "--role", "red-forman", "--invocation", "reviewer-1", "--verdict", "ERROR",
                     "--findings-file", "records/operations/findings.json")
        self.assertEqual(self.job()["blocker"]["code"], "prebuild-review-error")
        self.assertIsNone(self.job()["snapshot"])
        self.assertEqual(self.run_cli("review-status", "--job", self.job_id, "--stage", "spec")["result"]["verdict"], "ERROR")
        spec_path = self.jobdir / "spec.md"
        original = spec_path.read_text()
        spec_path.write_text(original + "tampered")
        self.run_cli("resume", "--job", self.job_id, "--reason", "Issue resolved", fail=True)
        spec_path.write_text(original)
        self.run_cli("resume", "--job", self.job_id, "--reason", "Reviewer access restored")
        self.assertEqual(self.job()["stage"], "analyze")
        self.run_cli("review-freeze", "--job", self.job_id, "--stage", "spec", "--author", "author-1",
                     "--reviewers-file", "records/operations/reviewers.json")

    def test_third_error_exhausts_and_does_not_allow_resume(self):
        self.setUpCurrent()
        self.fill("spec", "engram-analyzer", ["feature.md", "brief.md"], append="Addresses LED-1: preserved verbatim.")
        for number in range(1, 4):
            self.run_cli("review-freeze", "--job", self.job_id, "--stage", "spec", "--author", "author-1",
                         "--reviewers-file", "records/operations/reviewers.json")
            self.run_cli("review-record", "--job", self.job_id, "--stage", "spec", "--candidate", str(number),
                         "--role", "red-forman", "--invocation", "reviewer-1", "--verdict", "ERROR",
                         "--findings-file", "records/operations/findings.json")
            if number < 3:
                self.run_cli("resume", "--job", self.job_id, "--reason", "Reviewer access restored")
        self.assertEqual(self.job()["blocker"]["code"], "prebuild-exhausted")
        self.run_cli("resume", "--job", self.job_id, "--reason", "Bypass", fail=True)
        self.assertEqual(self.run_cli("review-status", "--job", self.job_id, "--stage", "spec")["result"]["verdict"], "ERROR")

    def test_registered_brief_binds_exact_revision(self):
        brief = self.root / "brief.md"
        instruction = self.root / "instruction.txt"
        brief.write_text(test_factory.FactoryTests.valid_brief(self))
        instruction.write_text("Ship the sourced-batch behavior.")
        first = self.run_cli("brief-create", "--file", str(brief), "--actor", "owner",
                             "--instruction-file", str(instruction), "--expected-brief-revision", "0",
                             "--operation-id", "create-1")
        result = self.run_cli("create", "--feature-file", str(self.feature), "--slug", "with-brief",
                              "--brief-id", first["brief_id"], "--brief-revision", "1")
        job = result["result"]
        self.assertEqual(job["brief_binding"]["sha256"], first["sha256"])
        self.assertEqual(job["artifacts"]["brief"]["sha256"], first["sha256"])
        self.run_cli("create", "--feature-file", str(self.feature), "--slug", "bad-revision",
                     "--brief-id", first["brief_id"], "--brief-revision", "99", fail=True)

    def test_draft_or_instruction_mismatch_brief_does_not_register_job(self):
        brief = self.root / "draft.md"
        instruction = self.root / "instruction.txt"
        brief.write_text(test_factory.FactoryTests.valid_brief(self).replace('status: "ratified"', 'status: "draft"'))
        instruction.write_text("Different owner instruction")
        first = self.run_cli("brief-create", "--file", str(brief), "--actor", "owner", "--instruction-file", str(instruction),
                             "--expected-brief-revision", "0", "--operation-id", "draft-1")
        self.run_cli("create", "--feature-file", str(self.feature), "--slug", "draft-brief",
                     "--brief-id", first["brief_id"], "--brief-revision", "1", fail=True)
        self.assertEqual(self.run_cli("status")["result"]["jobs"], [])

    def test_registry_instruction_must_match_ratification_cell(self):
        brief = self.root / "ratified.md"
        instruction = self.root / "instruction.txt"
        brief.write_text(test_factory.FactoryTests.valid_brief(self))
        instruction.write_text("LED-1")  # Appears in ledger, not the owner confirmation.
        entry = self.run_cli("brief-create", "--file", str(brief), "--actor", "owner", "--instruction-file", str(instruction),
                             "--expected-brief-revision", "0", "--operation-id", "mismatch-1")
        self.run_cli("create", "--feature-file", str(self.feature), "--slug", "mismatch",
                     "--brief-id", entry["brief_id"], "--brief-revision", "1", fail=True)
        self.assertEqual(self.run_cli("status")["result"]["jobs"], [])

    def test_plan_and_phase_reviews_are_candidate_bound_with_owner_receipt_start(self):
        self.setUpCurrent()
        for stage, kind, role, inputs in (("spec", "spec", "engram-analyzer", ["feature.md", "brief.md"]),
                                          ("plan", "plan", "engram-planner", ["spec.md"])):
            if stage == "plan":
                self.fill("contracts", "engram-planner", ["spec.md"], path="context/contracts.md")
                inputs = inputs + ["context/contracts.md"]
            self.fill(kind, role, inputs, append="Addresses LED-1: preserved verbatim." if stage == "spec" else "")
            self.run_cli("review-freeze", "--job", self.job_id, "--stage", stage, "--author", "author-1",
                         "--reviewers-file", "records/operations/reviewers.json")
            self.run_cli("review-record", "--job", self.job_id, "--stage", stage, "--candidate", "1",
                         "--role", "red-forman", "--invocation", "reviewer-1", "--verdict", "PASS",
                         "--findings-file", "records/operations/findings.json")
            artifacts = [kind + ".md"] + (["context/contracts.md"] if stage == "plan" else [])
            self.run_cli("complete", "--job", self.job_id, "--stage", "analyze" if stage == "spec" else "plan",
                         "--artifacts", *artifacts)
        task = self.jobdir / "tasks/task-001.md"
        task.write_text("# Deliver the fixture\n\nVerify using the independent fixture test.\n")
        manifest = {"schema_version": 1, "job_id": self.job_id, "cycle": 1, "phases": [{"id": "PHASE-1", "tasks": [
            {"id": "TASK-001", "document": "tasks/task-001.md", "sha256": test_factory.factory.digest(task), "depends_on": []}]}]}
        (self.jobdir / "records/operations/candidate.json").write_text(json.dumps(manifest))
        self.run_cli("tasks-initialize", "--job", self.job_id, "--manifest", "records/operations/candidate.json")
        self.fill("release", "engram-tasks", ["spec.md", "plan.md", "context/contracts.md"])
        frozen_release = (self.jobdir / "release.md").read_bytes()
        self.run_cli("review-freeze", "--job", self.job_id, "--stage", "tasks", "--phase", "PHASE-1", "--author", "author-1",
                     "--reviewers-file", "records/operations/reviewers.json")
        self.run_cli("review-record", "--job", self.job_id, "--stage", "tasks", "--phase", "PHASE-1", "--candidate", "1",
                     "--role", "red-forman", "--invocation", "reviewer-1", "--verdict", "PASS",
                     "--findings-file", "records/operations/findings.json")
        result = self.run_cli("review-status", "--job", self.job_id, "--stage", "tasks", "--phase", "PHASE-1")
        self.assertEqual(result["result"]["verdict"], "PASS")
        self.run_cli("complete", "--job", self.job_id, "--stage", "tasks", "--artifacts", "tasks/manifest.json", fail=True)
        self.fill("release", "engram-tasks", ["spec.md", "plan.md"])
        self.run_cli("complete", "--job", self.job_id, "--stage", "tasks", "--artifacts",
                     "tasks/manifest.json", "tasks/task.json", "release.md", fail=True)
        (self.jobdir / "release.md").write_bytes(frozen_release)
        self.run_cli("complete", "--job", self.job_id, "--stage", "tasks", "--artifacts",
                     "tasks/manifest.json", "tasks/task.json", "release.md")
        self.assertEqual(self.job()["status"], "awaiting-start")
        instruction = self.root / "start.txt"
        instruction.write_text("I do not accept this plan and do not start.")
        plan_hash = self.job()["artifacts"]["plan"]["sha256"]
        self.run_cli("start-build", "--job", self.job_id, "--actor", "owner", "--instruction-file", str(instruction),
                     "--decision", "accept-and-start", "--plan-sha256", plan_hash, fail=True)
        for refusal in ("No", "I will decide later"):
            instruction.write_text(refusal)
            self.run_cli("start-build", "--job", self.job_id, "--actor", "owner", "--instruction-file", str(instruction),
                         "--decision", "accept-and-start", "--plan-sha256", plan_hash, fail=True)
        instruction.write_text("Approved the exact plan; start implementation.")
        self.run_cli("start-build", "--job", self.job_id, "--actor", "owner", "--instruction-file", str(instruction),
                     "--decision", "accept-and-start", "--plan-sha256", plan_hash)
        self.assertEqual(self.job()["status"], "running")
        # Execution is enabled but remains gate-bound: optimistic concurrency
        # blocks stale revisions, and approval cannot precede the final review
        # of the accepted build.
        self.run_cli("task-transition", "--job", self.job_id, "--task", "TASK-001", "--to", "in_progress",
                     "--task-revision", "0", "--actor", "builder-1", "--reason", "begin")
        self.run_cli("task-transition", "--job", self.job_id, "--task", "TASK-001", "--to", "done",
                     "--task-revision", "0", "--actor", "builder-1", "--reason", "stale replay", fail=True)
        self.run_cli("approve", "--job", self.job_id, "--actor", "owner", "--decision", "approve",
                     "--instruction-file", str(instruction), fail=True)
        self.run_cli("merge", "--job", self.job_id, fail=True)
        self.assertEqual(self.job()["status"], "running")

    def test_default_create_is_v3_and_requires_a_ratified_brief(self):
        brief = self.root / "brief.md"
        brief.write_text(test_factory.FactoryTests.valid_brief(self))
        # Default create (no --layout) pins the headline behavior.
        result = self.run_cli("create", "--feature-file", str(self.feature), "--slug", "default-layout",
                              "--brief-file", str(brief))
        self.assertEqual(result["result"]["layout_version"], 3)
        # And the default refusal without a brief names the requirement.
        self.run_cli("create", "--feature-file", str(self.feature), "--slug", "no-brief", fail=True)

    def test_v3_build_completes_into_final_review(self):
        self.setUpCurrent()
        for stage, kind, role, inputs in (("spec", "spec", "engram-analyzer", ["feature.md", "brief.md"]),
                                          ("plan", "plan", "engram-planner", ["spec.md"])):
            if stage == "plan":
                self.fill("contracts", "engram-planner", ["spec.md"], path="context/contracts.md")
                inputs = inputs + ["context/contracts.md"]
            self.fill(kind, role, inputs, append="Addresses LED-1: preserved verbatim." if stage == "spec" else "")
            self.run_cli("review-freeze", "--job", self.job_id, "--stage", stage, "--author", "author-1",
                         "--reviewers-file", "records/operations/reviewers.json")
            self.run_cli("review-record", "--job", self.job_id, "--stage", stage, "--candidate", "1",
                         "--role", "red-forman", "--invocation", "reviewer-1", "--verdict", "PASS",
                         "--findings-file", "records/operations/findings.json")
            artifacts = [kind + ".md"] + (["context/contracts.md"] if stage == "plan" else [])
            self.run_cli("complete", "--job", self.job_id, "--stage", "analyze" if stage == "spec" else "plan",
                         "--artifacts", *artifacts)
        task = self.jobdir / "tasks/task-001.md"
        task.write_text("# Deliver the fixture\n\nVerify using the independent fixture test.\n")
        manifest = {"schema_version": 1, "job_id": self.job_id, "cycle": 1, "phases": [{"id": "PHASE-1", "tasks": [
            {"id": "TASK-001", "document": "tasks/task-001.md", "sha256": test_factory.factory.digest(task), "depends_on": []}]}]}
        (self.jobdir / "records/operations/candidate.json").write_text(json.dumps(manifest))
        self.run_cli("tasks-initialize", "--job", self.job_id, "--manifest", "records/operations/candidate.json")
        self.fill("release", "engram-tasks", ["spec.md", "plan.md", "context/contracts.md"])
        frozen_release = (self.jobdir / "release.md").read_bytes()
        self.run_cli("review-freeze", "--job", self.job_id, "--stage", "tasks", "--phase", "PHASE-1", "--author", "author-1",
                     "--reviewers-file", "records/operations/reviewers.json")
        self.run_cli("review-record", "--job", self.job_id, "--stage", "tasks", "--phase", "PHASE-1", "--candidate", "1",
                     "--role", "red-forman", "--invocation", "reviewer-1", "--verdict", "PASS",
                     "--findings-file", "records/operations/findings.json")
        (self.jobdir / "release.md").write_bytes(frozen_release)
        self.run_cli("complete", "--job", self.job_id, "--stage", "tasks", "--artifacts",
                     "tasks/manifest.json", "tasks/task.json", "release.md")
        instruction = self.root / "start.txt"
        instruction.write_text("Approved the exact plan; start implementation.")
        plan_hash = self.job()["artifacts"]["plan"]["sha256"]
        self.run_cli("start-build", "--job", self.job_id, "--actor", "owner", "--instruction-file", str(instruction),
                     "--decision", "accept-and-start", "--plan-sha256", plan_hash)
        # Execute the task through its gate-bound transition, then snapshot.
        self.run_cli("task-transition", "--job", self.job_id, "--task", "TASK-001", "--to", "in_progress",
                     "--task-revision", "0", "--actor", "builder-1", "--reason", "begin")
        self.run_cli("task-transition", "--job", self.job_id, "--task", "TASK-001", "--to", "done",
                     "--task-revision", "1", "--actor", "builder-1", "--reason", "done",
                     "--evidence", "tasks/task-001.md")
        worktree = Path(self.job()["worktree"])
        self.git(worktree, "commit", "--allow-empty", "-m", "fixture implementation")
        env = self.jobdir / "environment.json"
        env.write_text(json.dumps({"environment": "disposable-test", "product": "fixture",
                                   "version": "1", "observed_at": test_factory.factory.now(), "baseline": "fixture-1"}))
        snap = self.run_cli("snapshot", "--job", self.job_id, "--environment", str(env))["result"]
        snap_path = snap["snapshot"]["path"] if isinstance(snap.get("snapshot"), dict) else snap["path"]
        accepted = [item["path"] for item in self.job()["artifacts"].values()]
        self.fill("build", "engram-builder", accepted + ["tasks/task.json", snap_path],
                  snapshot=snap_path, path="records/operations/build-handoff-c1-r1.md", invocation_id="builder-1")
        self.run_cli("complete", "--job", self.job_id, "--stage", "build", "--snapshot", snap_path,
                     "--reviewers", "code,security,platform", "--artifacts", "records/operations/build-handoff-c1-r1.md")
        self.assertEqual((self.job()["stage"], self.job()["status"]), ("review", "running"))

    def test_v3_final_review_changes_reopens_build_for_round_2(self):
        CurrentWorkflowTests.test_v3_build_completes_into_final_review(self)
        # Record CHANGES from all three reviewers -> rebuild loop, not a wall.
        for reviewer in ("code", "security", "platform"):
            job = self.job()
            path = f"reviews/cycles/cycle-{job['cycle']}/round-{job['round']}/review-{reviewer}.md"
            self.fill("review", "reviewer-" + reviewer, [i["path"] for i in job["artifacts"].values()],
                      job["snapshot"]["path"], path, round=job["round"], discipline=reviewer, verdict="CHANGES",
                      invocation_id="final-reviewer-" + reviewer)
            self.run_cli("review", "--job", self.job_id, "--discipline", reviewer, "--artifact", path)
        self.assertEqual((self.job()["stage"], self.job()["status"]), ("build", "running"))
        self.assertIsNone(self.job()["snapshot"])

    def test_v3_rework_resets_prebuild_and_supports_second_cycle(self):
        CurrentWorkflowTests.test_v3_final_review_changes_reopens_build_for_round_2(self)
        # Rebuild loop is open at build/running after round-1 CHANGES; drive a
        # clean round 2, then reach a decision boundary via manual block.
        self.assertEqual((self.job()["stage"], self.job()["status"]), ("build", "running"))
        job = self.job()
        instruction = self.root / "start2.txt"
        instruction.write_text("Continue the approved plan for the correction round.")
        worktree = Path(job["worktree"])
        self.git(worktree, "commit", "--allow-empty", "-m", "correction round")
        env = self.jobdir / "environment2.json"
        env.write_text(json.dumps({"environment": "disposable-test", "product": "fixture",
                                   "version": "1", "observed_at": test_factory.factory.now(), "baseline": "fixture-1"}))
        snap2 = self.run_cli("snapshot", "--job", self.job_id, "--environment", str(env))["result"]
        snap2_path = snap2["snapshot"]["path"] if isinstance(snap2.get("snapshot"), dict) else snap2["path"]
        accepted = [item["path"] for item in self.job()["artifacts"].values()]
        self.fill("build", "engram-builder", accepted + ["tasks/task.json", snap2_path],
                  snapshot=snap2_path, path="records/operations/build-handoff-c1-r2.md", invocation_id="builder-2")
        self.run_cli("complete", "--job", self.job_id, "--stage", "build", "--snapshot", snap2_path,
                     "--reviewers", "code,security,platform", "--artifacts", "records/operations/build-handoff-c1-r2.md")
        for reviewer in ("code", "security", "platform"):
            job = self.job()
            path = f"reviews/cycles/cycle-{job['cycle']}/round-{job['round']}/review-{reviewer}.md"
            self.fill("review", "reviewer-" + reviewer, [i["path"] for i in job["artifacts"].values()],
                      job["snapshot"]["path"], path, round=job["round"], discipline=reviewer, verdict="PASS",
                      invocation_id="final2-" + reviewer)
            self.run_cli("review", "--job", self.job_id, "--discipline", reviewer, "--artifact", path)
        self.assertEqual(self.job()["stage"], "approve")
        self.run_cli("block", "--job", self.job_id, "--quiesced", "--reason", "owner decision boundary")
        self.assertEqual(self.job()["blocker"]["code"], "manual")
        note = self.root / "rework.txt"
        note.write_text("Address final-review findings and revalidate.")
        self.run_cli("rework", "--job", self.job_id, "--note-file", str(note))
        job = self.job()
        self.assertEqual((job["cycle"], job["stage"], job["status"]), (2, "analyze", "running"))
        self.assertNotIn("tasks.md", [p.name for p in self.jobdir.iterdir()])
        self.assertFalse((self.jobdir / "reviews" / "prebuild").exists() or
                         any((self.jobdir / "reviews" / "prebuild").glob("*") if (self.jobdir / "reviews" / "prebuild").exists() else []))
        self.assertTrue((self.jobdir / "history" / "cycle-1" / "reviews-prebuild").is_dir())
        self.assertIsNone(job.get("task_index_sha256"))
        # Cycle 2 can freeze a fresh spec candidate and complete analyze.
        self.fill("spec", "engram-analyzer", ["feature.md", "brief.md", "rework-2.md"], append="Addresses LED-1: preserved verbatim.")
        (self.jobdir / "records/operations/reviewers.json").write_text(json.dumps({"red-forman": "reviewer-2"}))
        self.run_cli("review-freeze", "--job", self.job_id, "--stage", "spec", "--author", "author-2",
                     "--reviewers-file", "records/operations/reviewers.json")
        self.run_cli("review-record", "--job", self.job_id, "--stage", "spec", "--candidate", "1",
                     "--role", "red-forman", "--invocation", "reviewer-2", "--verdict", "PASS",
                     "--findings-file", "records/operations/findings.json")
        self.run_cli("complete", "--job", self.job_id, "--stage", "analyze", "--artifacts", "spec.md")
        self.assertEqual(self.job()["stage"], "plan")

    def test_freeze_rejects_invalid_readiness_without_consuming_candidate(self):
        self.setUpCurrent()
        self.fill('spec', 'engram-analyzer', ['feature.md', 'brief.md'], append='Addresses LED-1. TODO')
        board = (self.repo / '.engram-factory/board.json').read_bytes()
        failure = self.run_cli('review-freeze', '--job', self.job_id, '--stage', 'spec',
                              '--author', 'author-1', '--reviewers-file', 'records/operations/reviewers.json', fail=True)
        self.assertIn('spec-prepare', failure['error'])
        self.assertEqual(board, (self.repo / '.engram-factory/board.json').read_bytes())
        self.assertFalse((self.jobdir / 'reviews/prebuild/spec').exists())
        self.fill('spec', 'engram-analyzer', ['feature.md', 'brief.md'], append='Addresses LED-1.')
        result = self.run_cli('review-freeze', '--job', self.job_id, '--stage', 'spec',
                              '--author', 'author-1', '--reviewers-file', 'records/operations/reviewers.json')
        self.assertEqual(1, result['result']['number'])

    def test_current_task_review_always_binds_release_and_both_receipts(self):
        self.setUpCurrent()
        self.fill('spec', 'engram-analyzer', [], append='Addresses LED-1.')
        test_factory.FactoryTests.accepted_complete(self, 'complete', '--job', self.job_id, '--stage', 'analyze', '--artifacts', 'spec.md')
        self.fill('plan', 'engram-planner', [])
        test_factory.FactoryTests.accepted_complete(self, 'complete', '--job', self.job_id, '--stage', 'plan', '--artifacts', 'plan.md')
        task = self.jobdir / 'tasks/task-001.md'
        task.write_text('Deliver the current fixture with independently checked evidence.')
        manifest = {'schema_version': 1, 'job_id': self.job_id, 'cycle': 1, 'phases': [{'id': 'PHASE-1', 'tasks': [{'id': 'TASK-001', 'document': 'tasks/task-001.md', 'sha256': test_factory.factory.digest(task), 'depends_on': []}]}]}
        (self.jobdir / 'tasks/manifest.json').write_text(json.dumps(manifest))
        paths = test_factory.factory.v3_candidate_paths(self.jobdir, self.job(), 'tasks', 'PHASE-1')
        self.assertTrue({'release.md', 'records/spec/receipt.json', 'records/plan/receipt.json'} <= set(paths))
        self.assertNotIn('handoff_contract', self.job())


if __name__ == "__main__":
    unittest.main()
