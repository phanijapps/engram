#!/usr/bin/env python3
"""Local factory state controller. No remote agents or platform writes."""

import argparse
import datetime as dt
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
from types import SimpleNamespace

sys.dont_write_bytecode = True  # Read-only commands must not create local import caches.
sys.path.insert(0, str(Path(__file__).resolve().parent))
import stage_reviews
import task_index
import brief_registry
import handoff
import spec_document
import plan_document
import spec_session
import plan_session
import e2e

ROOT = Path(__file__).resolve().parents[1]
PROFILES = ("full-stack", "servicenow", "salesforce", "sap", "maximo")
REVIEWERS = ("code", "security", "platform", "ux", "ui", "backend", "data", "operations")
MANDATORY = {"code", "security", "platform"}
MODES = ("full", "light")
STAGES = ("analyze", "plan", "tasks", "build", "e2e", "review", "approve", "merge", "merged", "needs-human", "queued", "cancelled", "archived")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def now():
    return dt.datetime.now(dt.timezone.utc).isoformat()


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def git(repo, *args):
    result = subprocess.run(["git", "-C", str(repo), *args], capture_output=True, text=True)
    require(result.returncode == 0, result.stderr.strip() or "Git command failed")
    return result.stdout.strip()


def clean(repo):
    require(not git(repo, "status", "--porcelain"), "Working tree is dirty: " + str(repo))


def atomic(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temp = tempfile.mkstemp(prefix=".factory-", dir=path.parent)
    try:
        with os.fdopen(fd, "w") as out:
            json.dump(value, out, indent=2)
            out.write("\n")
            out.flush()
            os.fsync(out.fileno())
        os.replace(temp, path)
        parent_fd = os.open(path.parent, os.O_DIRECTORY)
        try:
            os.fsync(parent_fd)
        finally:
            os.close(parent_fd)
    finally:
        if os.path.exists(temp):
            os.unlink(temp)


def checkpoint(common, board):
    payload = (json.dumps(board, indent=2) + "\n").encode()
    sha = hashlib.sha256(payload).hexdigest()
    path = common / "factory-checkpoints" / (sha + ".json")
    if path.exists():
        require(path.read_bytes() == payload, "Controller checkpoint modified")
    else:
        atomic(path, board)
    return path


def inside(root, relative):
    require(not Path(relative).is_absolute(), "Expected job-relative path")
    path = (root / relative).resolve()
    require(path.is_relative_to(root.resolve()), "Path escapes job directory")
    return path


def metadata(path):
    text = path.read_text()
    require(text.startswith("---\n"), "Missing frontmatter: " + str(path))
    _, header, body = text.split("---\n", 2)
    fields = {}
    for line in header.splitlines():
        require(":" in line, "Frontmatter must use one JSON value per line")
        key, value = line.split(":", 1)
        require(key not in fields, "Duplicate frontmatter key: " + key)
        fields[key] = json.loads(value.strip())
    if Path(path).name == "spec.md" or fields.get("artifact") == "spec" or fields.get("artifact_type") == "spec":
        require(fields.get("spec_format") == "compact-v1", "Unsupported spec format: current factory requires compact-v1")
        require(Path(path).name == "spec.md", "Compact spec must use canonical spec.md")
        fields = spec_document.load(path, fields, body)
    if Path(path).name == "plan.md" or fields.get("artifact") == "plan" or fields.get("artifact_type") == "plan":
        require(fields.get("plan_format") == "compact-v1", "Unsupported plan format: current factory requires compact-v1")
        require(Path(path).name == "plan.md", "Compact plan must use canonical plan.md")
        fields = plan_document.load(path, fields, body)
    return fields, body


def ref(jobdir, path):
    absolute = inside(jobdir, path)
    require(absolute.is_file(), "Missing artifact: " + path)
    return {"path": path, "sha256": digest(absolute)}


def validate_artifact(jobdir, path, job, kind=None, roles=None, required=(), snapshot=None):
    absolute = inside(jobdir, path)
    fields, body = metadata(absolute)
    if fields.get("spec_format") == "compact-v1":
        require(fields.get("spec_receipt_base") == job.get("base_sha"), "Spec receipt source base changed; reprepare before review")
    if fields.get("plan_format") == "compact-v1":
        require(fields.get("plan_receipt_base") == job.get("base_sha"), "Plan receipt source base changed")
    # Template placeholders are lowercase words (render keys and <profile>_<phase>_evidence);
    # evidence quoting real Jinja/Handlebars syntax ({{ user.name }}, {{x|default}}) stays legal.
    require(not re.search(r"\{\{[a-z_][a-z0-9_-]*\}\}", absolute.read_text()), "Unresolved placeholders: " + path)
    keys = {"schema_version", "job_id", "cycle", "status", "author_role",
            "profiles", "source_snapshot", "input_artifacts", "updated_at"}
    require(keys <= fields.keys(), "Missing common artifact metadata")
    identity = fields.get("artifact") if fields.get("spec_format") == "compact-v1" or fields.get("plan_format") == "compact-v1" else fields.get("artifact_type")
    if identity in {"spec", "plan"}:
        require(type(fields["schema_version"]) is int and fields["schema_version"] == 2,
                "Current spec/plan metadata requires numeric schema_version 2")
    else:
        require(fields["schema_version"] == "1.0" and "artifact_type" in fields and "artifact" not in fields,
                "Unsupported artifact format: current execution artifacts require schema_version 1.0 and artifact_type")
    require(fields["status"] == "ready", "Artifact is not ready: " + path)
    require(fields["job_id"] == job["job_id"], "Wrong artifact job")
    require(type(fields["cycle"]) is int and fields["cycle"] == job["cycle"], "Wrong artifact cycle")
    require(fields["profiles"] == job["profiles"], "Wrong artifact profiles")
    require(identity in {"spec", "plan", "release", "build", "review", "decision", "rework"}, "Unsupported artifact type")
    require(not kind or identity == kind, "Wrong artifact type")
    require(not roles or fields["author_role"] in roles, "Wrong artifact author role")
    stamp = dt.datetime.fromisoformat(fields["updated_at"].replace("Z", "+00:00"))
    require(stamp.tzinfo is not None, "Timestamp needs timezone")
    require(len(body.strip()) > 80 and "\n## " in body, "Artifact body is incomplete")
    require(fields["source_snapshot"] == snapshot, "Wrong source snapshot in " + path)
    refs = fields["input_artifacts"]
    require(isinstance(refs, list), "input_artifacts must be an array")
    seen = set()
    for item in refs:
        require(isinstance(item, dict) and set(item) == {"path", "sha256"}, "Invalid input reference")
        require(item["path"] not in seen and item["path"] != path, "Duplicate or self input")
        seen.add(item["path"])
        require(ref(jobdir, item["path"]) == item, "Stale input: " + item["path"])
    require(set(required) <= seen, "Missing required direct inputs: " + str(set(required) - seen))
    return fields




def prepare_spec(jobdir, job):
    require(job.get("layout_version") == 3 and job["stage"] == "analyze" and job["status"] == "running",
            "Spec preparation requires active v3 analysis")
    for item in job["artifacts"].values():
        intact(jobdir, item)
    review_dir = stage_reviews._dir(jobdir, "spec", None)
    if review_dir.exists() and list(review_dir.glob("candidate-*.json")):
        with stage_reviews._locked(review_dir):
            candidates = stage_reviews._candidates(review_dir)
            current = json.loads(candidates[-1].read_text())
            status = stage_reviews._status(review_dir, jobdir, current, check_inputs=False)
            require(status["verdict"] in {"CHANGES", "ERROR"} and len(candidates) < 3,
                    "Do not alter pending, PASS or exhausted candidate inputs")
    draft = spec_document.draft(globals(), jobdir, job)
    path = inside(jobdir, "spec.md")
    text, fields, body, derived = draft["text"], draft["fields"], draft["body"], draft["derived"]
    required = [job["artifacts"][n]["path"] for n in ("feature", "brief", "rework") if n in job["artifacts"]]
    generated = {"schema_version": 2, "artifact": "spec", "job_id": job["job_id"], "cycle": job["cycle"],
                 "status": "ready", "author_role": "engram-analyzer", "profiles": job["profiles"],
                 "source_snapshot": None, "input_artifacts": [ref(jobdir, p) for p in required]}
    proposed = {**fields, **generated, **derived, "updated_at": now()}
    receipt = {"schema_version": 1, "spec_sha256": digest(path), "base_revision": job["base_sha"], "fields": generated}
    target = spec_document.receipt_path(path)
    if target.exists():
        old = spec_document.read_receipt(target)
        old_fields = dict(old.get("fields", {}))
        old_fields.pop("updated_at", None)
        if {**old, "fields": old_fields} == receipt:
            return {"receipt": ref(jobdir, spec_document.RECEIPT), "changed": False}
    generated["updated_at"] = proposed["updated_at"]
    # Authored bytes and upstream identities must remain stable through validation.
    require(path.read_text() == text and all(ref(jobdir, r["path"]) == r for r in generated["input_artifacts"]),
            "Spec inputs changed during preparation")
    atomic(target, receipt)
    return {"receipt": ref(jobdir, spec_document.RECEIPT), "changed": True, "independent_review": "required"}


def record(jobdir, path):
    item = ref(jobdir, path)
    archive = "evidence/" + item["sha256"] + "/" + Path(path).name
    target = inside(jobdir, archive)
    target.parent.mkdir(parents=True, exist_ok=True)
    if target.exists():
        require(digest(target) == item["sha256"], "Evidence archive has been modified")
    else:
        shutil.copyfile(inside(jobdir, path), target)
    return {**item, "archive": archive}


def intact(jobdir, item):
    require(ref(jobdir, item["path"])["sha256"] == item["sha256"], "Accepted evidence changed: " + item["path"])
    require(digest(inside(jobdir, item["archive"])) == item["sha256"], "Evidence archive changed")


def render(name, job, **extra):
    text = (ROOT / "assets" / "templates" / (name + ".md")).read_text()
    values = {"job_id": job["job_id"], "cycle": job["cycle"], "profiles_json": json.dumps(job["profiles"]),
              "feature_title": job["feature"].splitlines()[0], "timestamp": now(), **extra}
    for key, value in values.items():
        text = text.replace("{{" + key + "}}", str(value))
    return text




def scaffold_v3(jobdir, job):
    """Create the current reviewable layout."""
    for directory in ("tasks", "context", "reviews", "records/snapshots",
                      "records/evidence", "records/cycles", "records/operations"):
        (jobdir / directory).mkdir(parents=True, exist_ok=True)
    for name in ("spec", "plan", "release"):
        path = jobdir / (name + ".md")
        if not path.exists():
            path.write_text(render(name, job))


def v3_candidate_paths(jobdir, job, stage, phase=None, during_build=False):
    """Derive review inputs in the controller, never trust a caller's subset."""
    require(stage in {"spec", "plan", "tasks"}, "Invalid v3 review stage")
    require((stage == "tasks") == (phase is not None), "Phase required only for tasks")
    expected_stage = "build" if during_build and stage == "tasks" else {"spec": "analyze", "plan": "plan", "tasks": "tasks"}[stage]
    require(job["stage"] == expected_stage
            and job["status"] == "running", "Review does not match current stage")
    paths = ["feature.md"] + (["brief.md"] if "brief" in job["artifacts"] else []) + ["spec.md"]
    if "rework" in job["artifacts"]:
        paths.append(job["artifacts"]["rework"]["path"])
    if stage in {"plan", "tasks"}:
        paths.append("plan.md")
        for item in ("context/architecture.md", "context/data-models.md", "context/interfaces.md",
                     "context/contracts.md"):
            if inside(jobdir, item).is_file():
                paths.append(item)
    if stage == "tasks":
        manifest_path = inside(jobdir, "tasks/manifest.json")
        manifest = json.loads(manifest_path.read_text())
        task_index.validate_manifest(jobdir, manifest)
        require(manifest.get("job_id") == job["job_id"] and manifest.get("cycle") == job["cycle"],
                "Task manifest job or cycle mismatch")
        phases = [row for row in manifest["phases"] if row["id"] == phase]
        require(len(phases) == 1, "Unknown task phase")
        paths.append("tasks/manifest.json")
        paths.append("release.md")
        paths.extend(row["document"] for row in phases[0]["tasks"])
    if "spec-session" in job["artifacts"]:
        paths.append(job["artifacts"]["spec-session"]["path"])
    paths.append(spec_document.RECEIPT)
    if stage in {"plan", "tasks"} and "plan-session" in job["artifacts"]:
        paths.append(job["artifacts"]["plan-session"]["path"])
    if stage in {"plan", "tasks"}:
        paths.append(plan_document.RECEIPT)
    return sorted(paths)


def v3_record_review_bundle(jobdir, stage, phase=None):
    """Bind every reviewed candidate/receipt/archive into downstream integrity."""
    directory = stage_reviews._dir(jobdir, stage, phase)
    require(directory.is_dir(), "Missing review bundle")
    files = sorted(path for path in directory.rglob("*") if path.is_file() and path.name != ".lock")
    require(files, "Empty review bundle")
    result = {}
    for path in files:
        require(not path.is_symlink(), "Review bundle symlink is forbidden")
        relative = path.relative_to(jobdir).as_posix()
        key = "prebuild-" + hashlib.sha256(relative.encode()).hexdigest()
        result[key] = record(jobdir, relative)
    return result


def v3_accept_tasks(jobdir, job, args):
    """Freeze reviewed per-phase definitions before asking owner to start build."""
    checked = handoff.validate(globals(), jobdir, job, "tasks", args.artifacts)
    manifest, index_path = checked["manifest"], checked["index_path"]
    for phase in manifest["phases"]:
        paths = v3_candidate_paths(jobdir, job, "tasks", phase["id"])
        stage_reviews.require_stage_pass(jobdir, "tasks", [ref(jobdir, path) for path in paths], phase["id"])
    records = {"tasks-manifest": record(jobdir, "tasks/manifest.json"),
               "release": record(jobdir, "release.md")}
    for phase in manifest["phases"]:
        records.update(v3_record_review_bundle(jobdir, "tasks", phase["id"]))
        for item in phase["tasks"]:
            records["task-" + item["id"]] = record(jobdir, item["document"])
    job["artifacts"].update(records)
    job["task_index_sha256"] = digest(index_path)
    job["stage"], job["status"] = "build", "awaiting-start"
    event(job, "complete-tasks", records)
    return job


def v3_check_task_state(jobdir, job, require_done=False):
    """Check frozen definitions and all referenced task-evidence bytes."""
    index_path = inside(jobdir, "tasks/task.json")
    require(digest(index_path) == job.get("task_index_sha256"), "Task index changed or write interrupted")
    manifest_path = inside(jobdir, "tasks/manifest.json")
    manifest = json.loads(manifest_path.read_text())
    edges = task_index.validate_manifest(jobdir, manifest)
    state = json.loads(index_path.read_text())
    require(state.get("job_id") == job["job_id"] and state.get("cycle") == job["cycle"]
            and state.get("manifest_sha256") == digest(manifest_path)
            and set(state.get("tasks", {})) == set(edges), "Task state identity mismatch")
    for task in state["tasks"].values():
        require(task.get("status") in {"pending", "in_progress", "blocked", "done"}, "Invalid task state")
        if require_done:
            require(task["status"] == "done" and task.get("evidence"), "Every task must be done with evidence")
        for evidence in task.get("evidence", []):
            require(ref(jobdir, evidence["path"]) == evidence, "Task evidence changed")
    return state


def v3_accept_build(repo, jobdir, job, args):
    """Freeze v3 build handoff and begin independent final-review round."""
    require(job["round"] < 2, "Review rounds exhausted")
    attempt = f"-a{job['e2e']['attempts'] + 1}" if "e2e" in job else ""
    handoff_path = f"records/operations/build-handoff-c{job['cycle']}-r{job['round'] + 1}{attempt}.md"
    require(args.artifacts == [handoff_path], "v3 build needs the round-scoped canonical handoff " + handoff_path)
    require(args.snapshot and job.get("snapshot") and args.snapshot == job["snapshot"]["path"], "Supply current snapshot")
    source_check(repo, jobdir, job)
    reviewers = args.reviewers.split(",") if args.reviewers else []
    require(len(reviewers) == len(set(reviewers)) and MANDATORY <= set(reviewers) <= set(REVIEWERS), "Invalid reviewer set")
    require(set(job["required_reviewers"]) <= set(reviewers), "Cannot omit previously required reviewers")
    required = [item["path"] for item in job["artifacts"].values()] + ["tasks/task.json", job["snapshot"]["path"]]
    fields = validate_artifact(jobdir, args.artifacts[0], job, "build", {"engram-builder"}, required, args.snapshot)
    require(isinstance(fields.get("invocation_id"), str) and fields["invocation_id"].strip(), "Builder invocation required")
    handoff = record(jobdir, args.artifacts[0])
    job["artifacts"]["build"] = handoff
    job["build_invocation"] = fields["invocation_id"]
    if "e2e" in job:
        e2e.begin(globals(), jobdir, job, reviewers)
    else:
        v3_start_review(jobdir, job, reviewers)
    event(job, "complete-build", {"build": handoff})
    return job


def v3_start_review(jobdir, job, reviewers):
    require(job["round"] < 2, "Review rounds exhausted")
    next_round = job["round"] + 1
    directory = jobdir / "reviews" / "cycles" / f"cycle-{job['cycle']}" / f"round-{next_round}"
    require(not directory.exists(), "Review round already exists; inspect interrupted handoff")
    directory.mkdir(parents=True)
    for reviewer in reviewers:
        path = directory / ("review-" + reviewer + ".md")
        path.write_text(render("review", job, snapshot_path=job["snapshot"]["path"], discipline=reviewer,
                               round=next_round, review_author=review_author(job, reviewer)))
    job["round"] = next_round
    job["required_reviewers"] = reviewers
    job["reviews"] = {reviewer: {"verdict": "pending"} for reviewer in reviewers}
    job["stage"], job["status"] = "review", "running"


def check_board(board):
    require(board.get("schema_version") == "2.0", "Unsupported board format: current factory requires schema_version 2.0; preserve existing records")
    require(type(board.get("revision")) is int and board["revision"] >= 0, "Invalid revision")
    require(isinstance(board.get("jobs"), list), "Invalid jobs")
    ids, branches, worktrees = set(), set(), set()
    for job in board["jobs"]:
        require({"mode", "priority"} <= job.keys(), "Incomplete current job format: mode and priority are required")
        require(type(job.get("layout_version")) is int and job["layout_version"] == 3, "Unsupported job format: current factory requires layout_version 3; preserve existing records")
        require(job["job_id"] not in ids, "Duplicate job ID")
        require(re.fullmatch(r"[0-9]{3,}-[a-z0-9]+(?:-[a-z0-9]+)*", job["job_id"]), "Invalid job ID")
        ids.add(job["job_id"])
        require(job["branch"] == "factory/" + job["job_id"], "Conflicting job branch")
        require(job["branch"] not in branches and job["worktree"] not in worktrees, "Conflicting job source identities")
        branches.add(job["branch"])
        worktrees.add(job["worktree"])
        require(job["target"] == board["target"], "Conflicting target")
        require(job["stage"] in STAGES, "Invalid stage")
        require(1 <= job["cycle"] and 0 <= job["round"] <= 2, "Invalid cycle/round")
        require(set(job["profiles"]) <= set(PROFILES), "Unknown profile")
        require(job["mode"] in MODES, "Invalid mode")
        require(set(job["reviews"]) <= set(job["required_reviewers"]), "Unexpected reviewer")
    # One execution lease is acquired at activation.
    require(sum(executing(j) for j in board["jobs"]) <= 1, "Multiple execution leases")
    for job in board["jobs"]:
        require(type(job["priority"]) is int, "Invalid priority")


TERMINAL = {"merged", "cancelled", "archived"}


def executing(job):
    return job["stage"] not in TERMINAL | {"queued", "needs-human"}


def lease(board, job):
    require(not any(executing(other) and other["job_id"] != job["job_id"]
                    for other in board["jobs"]), "Execution lease held by another job")


def inventory(repo, state, board=None):
    """Read-only facts; never infer missing approvals or adopt source."""
    issues = []
    jobs = {j["job_id"]: j for j in (board or {}).get("jobs", [])}
    if board is None:
        issues.append({"kind": "missing-board", "path": str(state / "board.json")})
    directories = list((state / "jobs").iterdir()) if (state / "jobs").exists() else []
    for path in directories:
        if path.name not in jobs:
            issues.append({"kind": "orphan-directory", "path": str(path)})
    branches = git(repo, "for-each-ref", "--format=%(refname:short)", "refs/heads/factory/").splitlines()
    for branch in branches:
        if branch not in {j["branch"] for j in jobs.values()}:
            issues.append({"kind": "orphan-branch", "branch": branch})
    worktrees = []
    for chunk in git(repo, "worktree", "list", "--porcelain").split("\n\n"):
        facts = dict(line.split(" ", 1) for line in chunk.splitlines() if " " in line)
        if facts.get("branch", "").startswith("refs/heads/factory/"):
            worktrees.append(facts)
            if facts.get("worktree") not in {j["worktree"] for j in jobs.values()}:
                issues.append({"kind": "orphan-worktree", **facts})
            path = Path(facts["worktree"])
            if path.exists() and git(path, "status", "--porcelain"):
                issues.append({"kind": "dirty-worktree", "path": str(path)})
    for job in jobs.values():
        if Path(job["worktree"]).resolve() != (state / "worktrees" / job["job_id"]).resolve():
            issues.append({"kind": "conflicting-worktree-path", "job": job["job_id"], "path": job["worktree"]})
        try:
            git(repo, "rev-parse", "--verify", "refs/heads/" + job["target"])
            if job.get("base_sha"):
                git(repo, "cat-file", "-e", job["base_sha"] + "^{commit}")
                git(repo, "merge-base", "--is-ancestor", job["base_sha"], job["branch"])
                if git(repo, "rev-parse", job["target"]) != job["base_sha"]:
                    issues.append({"kind": "target-moved", "job": job["job_id"], "base_sha": job["base_sha"]})
        except ValueError as exc:
            issues.append({"kind": "invalid-source-identity", "job": job["job_id"], "error": str(exc)})
        if job["stage"] == "queued" and (job["branch"] in branches or any(w.get("worktree") == job["worktree"] for w in worktrees)):
            issues.append({"kind": "queued-source-remnant", "job": job["job_id"], "path": job["worktree"]})
        jobdir = state / "jobs" / job["job_id"]
        if not jobdir.is_dir():
            issues.append({"kind": "missing-job-directory", "path": str(jobdir)})
        refs = list(job["artifacts"].values()) + [r for r in job["reviews"].values() if "path" in r]
        if job.get("snapshot"):
            refs.append(job["snapshot"])
        for item in refs:
            try:
                intact(jobdir, item)
            except (ValueError, OSError) as exc:
                issues.append({"kind": "damaged-evidence", "job": job["job_id"], "error": str(exc)})
        if job.get("base_sha") is not None:
            found = [w for w in worktrees if w.get("worktree") == job["worktree"]
                     and w.get("branch") == "refs/heads/" + job["branch"]]
            if not found:
                issues.append({"kind": "missing-or-conflicting-worktree", "job": job["job_id"], "path": job["worktree"]})
    return {"board": str(state / "board.json"), "issues": issues,
            "branches": branches, "worktrees": worktrees,
            "job_directories": [str(p) for p in directories]}


def review_author(job, discipline):
    # DEC-001/DEC-004: light mode concentrates authorship in red-forman while
    # every per-discipline file, the mandatory set and the rubric stay intact.
    return "red-forman" if job["mode"] == "light" else "reviewer-" + discipline


def mode_pin(job):
    # DEC-P02: pin provenance lives in the append-only history. A create-time
    # owner pin stays authoritative until any rework event supersedes it —
    # the rework note is the newer owner authority (DEC-003).
    pinned = any(e["action"] == "create" and e.get("detail", {}).get("mode_pinned")
                 for e in job["history"])
    reworked = any(e["action"] == "rework" for e in job["history"])
    return pinned and not reworked


def source_check(repo, jobdir, job, check_target=True):
    e2e.check_policy(globals(), jobdir, job)
    require(job["snapshot"], "No source snapshot")
    intact(jobdir, job["snapshot"])
    if job["stage"] in {"review", "approve", "merge"}:
        e2e.require_pass(globals(), jobdir, job)
    snapshot = json.loads(inside(jobdir, job["snapshot"]["path"]).read_text())
    v3_check_task_state(jobdir, job, require_done=True)
    require(snapshot.get("task_index_sha256") == job.get("task_index_sha256"), "Task state changed after snapshot")
    require(git(job["worktree"], "branch", "--show-current") == job["branch"], "Wrong worktree branch")
    clean(job["worktree"])
    require(git(job["worktree"], "rev-parse", "HEAD") == snapshot["head_sha"], "Source changed after snapshot")
    if check_target:
        require(git(repo, "rev-parse", job["target"]) == snapshot["base_sha"], "Target advanced; integrate and re-review")
    # Accepted evidence lives in the job archive; out-of-tree originals are
    # checked only while they still exist, so a tmp-cleaned export cannot wedge reviews.
    original = Path(snapshot["environment_file"])
    require(not original.exists() or digest(original) == snapshot["environment_sha256"], "Environment fingerprint changed")
    for item in snapshot["exports"]:
        original = Path(item["original_path"])
        require(not original.exists() or digest(original) == item["sha256"], "External package changed")
        require(digest(inside(jobdir, item["archive"])) == item["sha256"], "Archived package changed")
    for item in job["artifacts"].values():
        intact(jobdir, item)
    return snapshot


def event(job, action, detail):
    job["updated_at"] = now()
    job["history"].append({"at": now(), "action": action, "detail": detail})


def emit_evidence(jobdir, job, family, actor, **fields):
    """L3 canonical evidence (rail 06): append one registered-family event to
    <jobdir>/events.jsonl. Families are declared canonical-first in
    docs/contracts/constitution/v1/events/registry.json; check-canonical-evidence.py
    fails on undeclared families or malformed records."""
    record_ = {
        "schema": "engram.evidence.v1",
        "family": family,
        "job_id": job["job_id"],
        "actor": actor,
        "at": now(),
        **fields,
    }
    with (jobdir / "events.jsonl").open("a", encoding="utf-8") as fh:
        fh.write(json.dumps(record_, sort_keys=True) + "\n")


def block(job, reason, code="manual"):
    job["resume_from"] = {"stage": job["stage"], "status": job["status"]}
    job["stage"], job["status"] = "needs-human", "blocked"
    job["blocker"] = {"code": code, "reason": reason}


def selected(board, job_id):
    jobs = [j for j in board["jobs"] if j["job_id"] == job_id]
    require(len(jobs) == 1, "Unknown job")
    return jobs[0]


def execute(args, repo, state, board):
    cmd = args.command
    if cmd == "status":
        if args.job:
            job = selected(board, args.job)
            return {**job, "mode": job["mode"]}
        return {**board, "queue_order": [j["job_id"] for j in sorted(board["jobs"], key=lambda j: (-j["priority"], board["jobs"].index(j))) if j["stage"] == "queued"], "jobs": [{**job, "mode": job["mode"]} for job in board["jobs"]]}
    if cmd == "create":
        require(not [i for i in inventory(repo, state, board)["issues"] if i["kind"] not in {"dirty-worktree", "target-moved"}], "Integrity issues; run integrity before registering")
        require(re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+){0,7}", args.slug), "Use a short lowercase kebab-case slug")
        feature = Path(args.feature_file).read_text()
        require(feature.strip(), "Empty feature")
        require(not (args.brief_id and args.brief_file), "Choose --brief-file or --brief-id, not both")
        require((args.brief_revision is None) == (args.brief_id is None),
                "Registered brief binding requires both ID and exact revision")
        brief_binding = None
        if args.brief_id:
            # execute() is called with board.lock held; use the lock-aware read
            # API rather than calling get(), which would acquire the lock again.
            registry = brief_registry.Registry(state)
            brief_binding, brief_bytes = registry.get_locked(args.brief_id, args.brief_revision)
            brief = brief_bytes.decode("utf-8")
        else:
            brief = Path(args.brief_file).read_text() if args.brief_file else None
        require(brief is not None and brief.startswith("---\n"),
                "Current factory requires a ratified brief with frontmatter; frame one with engram-frame")
        parts = brief.split("---\n", 2)
        require(len(parts) == 3, "Invalid brief frontmatter")
        status_lines = [line.split(":", 1)[1].strip() for line in parts[1].splitlines()
                        if line.startswith("status:")]
        require(len(status_lines) == 1 and json.loads(status_lines[0]) == "ratified",
                "Current factory requires brief status ratified")
        if brief_binding:
            require("## Owner ratification" in brief, "Missing owner ratification section")
            ratification = brief.split("## Owner ratification", 1)[1].split("\n## ", 1)[0]
            quoted_cells = []
            for row in ratification.splitlines():
                cells = [cell.strip() for cell in row.strip().strip("|").split("|")]
                if len(cells) >= 4:
                    value = cells[-1]
                    if len(value) >= 2 and ((value[0] == value[-1] == '"') or
                                            (value[0] == '“' and value[-1] == '”')):
                        quoted_cells.append(value[1:-1])
            require(brief_binding["instruction"].strip() in quoted_cells,
                    "Registry instruction must exactly match a quoted owner ratification table cell")
        require("## The ask" in brief and "## Owner ratification" in brief
                and "## Decision ledger" in brief,
                "Brief must carry The ask, Decision ledger and Owner ratification sections")
        require(feature.strip() in brief, "Brief's ask does not contain the feature text verbatim")
        require(re.search(r"\|[^|]*\|[^|]*\|[^|]*\|[^|]*\"[^|]*\|", brief.split("## Owner ratification")[1]),
                "Owner ratification must carry a non-empty row with a verbatim quoted confirmation")
        profiles = args.profiles.split(",")
        require(profiles and len(set(profiles)) == len(profiles) and set(profiles) <= set(PROFILES), "Invalid profiles")
        mode = args.mode or "full"  # absent flag means full, unpinned (DEC-P02)
        number = max([int(j["job_id"].split("-")[0]) for j in board["jobs"]] or [0]) + 1
        job_id = f"{number:03d}-{args.slug}"
        jobdir, worktree = state / "jobs" / job_id, state / "worktrees" / job_id
        require(not jobdir.exists() and not worktree.exists(), "Orphan job/worktree exists; inspect before recovery")
        for profile in profiles:
            require((ROOT / "references" / (profile + ".md")).is_file(), "Missing profile reference: " + profile)
        branch, base = "factory/" + job_id, None
        job = {"job_id": job_id, "feature": feature, "profiles": profiles,
               "created_at": now(), "updated_at": now(), "stage": "queued", "status": "queued", "priority": args.priority,
               "mode": mode,
               "cycle": 1, "round": 0, "target": board["target"], "branch": branch,
               "base_sha": base, "worktree": str(worktree), "snapshot": None,
               "required_reviewers": [], "reviews": {}, "artifacts": {}, "approval": None,
               "blocker": None, "resume_from": None, "history": []}
        job["layout_version"] = 3
        if brief_binding:
            job["brief_binding"] = {"brief_id": brief_binding["brief_id"],
                                    "revision": brief_binding["revision"],
                                    "sha256": brief_binding["sha256"]}
        jobdir.mkdir(parents=True)
        (jobdir / "feature.md").write_text(feature)
        job["artifacts"]["feature"] = record(jobdir, "feature.md")
        (jobdir / "brief.md").write_text(brief)
        job["artifacts"]["brief"] = record(jobdir, "brief.md")
        scaffold_v3(jobdir, job)
        # Registration captures intent only; activation captures reproducible source.
        # A failed registration preserves an inspectable job directory.
        event(job, "create", {"base_sha": base, "mode": mode, "mode_pinned": args.mode is not None})
        emit_evidence(jobdir, job, "job.created", "agent",
                      brief_sha256=job["artifacts"]["brief"]["sha256"],
                      size_tier=job.get("size_tier"))
        board["jobs"].append(job)
        return job
    job = selected(board, args.job)
    jobdir = state / "jobs" / job["job_id"]
    if cmd == "plan-session-open":
        lease(board, job)
        return plan_session.open_session(globals(), jobdir, job, args.author, args.reviewers_file)
    if cmd == "plan-submit":
        lease(board, job)
        return plan_session.submit(globals(), jobdir, job, board["revision"], args)
    if cmd == "spec-session-open":
        lease(board, job)
        return spec_session.open_session(globals(), jobdir, job, args.author, args.reviewers_file)
    if cmd == "spec-submit":
        lease(board, job)
        return spec_session.submit(globals(), jobdir, job, board["revision"], args)
    if cmd == "plan-prepare":
        return plan_document.prepare(globals(), jobdir, job)
    if cmd == "spec-prepare":
        return prepare_spec(jobdir, job)
    if cmd == "validate-artifact":
        return validate_artifact(jobdir, args.artifact, job, snapshot=args.snapshot)
    if cmd in {"review-freeze", "review-record", "review-status", "review-clarification", "tasks-initialize", "tasks-reinitialize"}:
        require(job.get("layout_version") == 3, "Current jobs require layout_version 3")
        blocked_review = (job["stage"] == "needs-human" and job.get("blocker", {}).get("code")
                          in {"prebuild-review-error", "prebuild-exhausted"})
        if not blocked_review:
            lease(board, job)
        for accepted in job["artifacts"].values():
            intact(jobdir, accepted)
        if blocked_review:
            require(cmd in {"review-status", "review-clarification"}, "Resolve the blocked pre-build review first")
            expected = {"analyze": "spec", "plan": "plan"}.get(job["resume_from"]["stage"])
            require(args.stage == expected and args.phase is None, "Review does not match blocked stage")
            result = stage_reviews.review_status(jobdir, args.stage)
            if cmd == "review-status":
                return result
            receipt = stage_reviews.record_clarification(jobdir, args.stage, None,
                                                        args.question, args.answer, args.owner, args.reference)
            event(job, "review-clarification", {"stage": args.stage, "phase": None})
            return receipt
        if cmd == "tasks-initialize":
            require(job["stage"] == "tasks" and job["status"] == "running", "Unexpected task initialization")
            manifest = json.loads(inside(jobdir, args.manifest).read_text())
            require(manifest.get("job_id") == job["job_id"] and manifest.get("cycle") == job["cycle"], "Wrong manifest identity")
            result = task_index.initialize(jobdir, manifest)
            event(job, "tasks-initialize", {"manifest_sha256": result["manifest_sha256"]})
            return result
        if cmd == "tasks-reinitialize":
            require(job["stage"] == "tasks" and job["status"] == "running", "Unexpected task reinitialization")
            manifest = json.loads(inside(jobdir, args.manifest).read_text())
            require(manifest.get("job_id") == job["job_id"] and manifest.get("cycle") == job["cycle"], "Wrong manifest identity")
            result = task_index.reinitialize(jobdir, manifest)
            # The shared tasks/manifest.json is part of every phase candidate; a
            # sanctioned revision supersedes all existing phase candidates. Archive
            # them as evidence and clear the live dirs so each phase re-reviews fresh.
            superseded = jobdir / "records" / "task-review-superseded"
            stamp = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
            for phase_dir in sorted((jobdir / "reviews" / "prebuild").glob("tasks-*")):
                if phase_dir.is_dir():
                    dest = superseded / stamp / phase_dir.name
                    dest.mkdir(parents=True, exist_ok=True)
                    shutil.copytree(phase_dir, dest, dirs_exist_ok=True)
                    shutil.rmtree(phase_dir)
            event(job, "tasks-reinitialize", {"manifest_sha256": result["manifest_sha256"]})
            return result
        paths = v3_candidate_paths(jobdir, job, args.stage, args.phase)
        if cmd == "review-clarification":
            result = stage_reviews.record_clarification(jobdir, args.stage, args.phase,
                                                        args.question, args.answer, args.owner, args.reference)
            event(job, "review-clarification", {"stage": args.stage, "phase": args.phase})
            return result
        if cmd == "review-freeze":
            stage = {"spec": "analyze", "plan": "plan", "tasks": "tasks"}[args.stage]
            handoff.validate(globals(), jobdir, job, stage,
                             handoff.artifacts_for(globals(), jobdir, job, stage))
            reviewers = json.loads(inside(jobdir, args.reviewers_file).read_text())
            if args.stage in {"spec", "plan"} and args.stage + "-session" in job["artifacts"]:
                interface = spec_session if args.stage == "spec" else plan_session
                shared = interface.session(SimpleNamespace(**globals()), jobdir, job)
                require(args.author == shared["author"] and reviewers == shared["reviewers"], "Spec session author/reviewer roster is fixed")
            result = stage_reviews.freeze_candidate(jobdir, args.stage, args.phase, args.author, paths, reviewers)
            event(job, "review-freeze", {"stage": args.stage, "phase": args.phase, "candidate": result["number"]})
            return result
        if cmd == "review-record":
            findings = json.loads(inside(jobdir, args.findings_file).read_text())
            result = stage_reviews.record_review(jobdir, args.stage, args.phase, args.candidate,
                                                args.role, args.invocation, args.verdict,
                                                findings.get("findings", {}), findings.get("resolves", []))
            event(job, "review-record", {"stage": args.stage, "phase": args.phase, "candidate": args.candidate, "role": args.role})
            current = stage_reviews.review_status(jobdir, args.stage, args.phase)
            if current["verdict"] == "NEEDS_HUMAN":
                block(job, f"{args.stage} review still requests changes after three candidates; owner decision required", "prebuild-exhausted")
            elif args.stage in {"spec", "plan"} and current["verdict"] == "ERROR":
                code = "prebuild-exhausted" if current["candidate"] >= 3 else "prebuild-review-error"
                block(job, f"{args.stage} review could not be completed; owner attention required", code)
            result["review_status"] = current
            return result
        return stage_reviews.review_status(jobdir, args.stage, args.phase)
    if cmd == "priority":
        require(args.actor.strip() and args.reason.strip(), "Capture owner actor and reason")
        require(job["stage"] not in TERMINAL, "Terminal job cannot be prioritized")
        job["priority"] = args.priority
        event(job, "priority", {"priority": args.priority, "actor": args.actor, "reason": args.reason})
        return job
    if cmd in {"cancel", "archive", "restore"}:
        require(args.actor.strip() and args.reason.strip(), "Capture owner actor and reason")
        if cmd == "restore":
            require(job["stage"] in {"cancelled", "archived"}, "Not retired")
            previous = job["retired_from"]
            job["stage"], job["status"] = previous["stage"], previous["status"]
            if executing(job):
                lease(board, job)
        else:
            require(job["stage"] in {"queued", "needs-human"}, "Block active work before retirement")
            job["retired_from"] = {"stage": job["stage"], "status": job["status"]}
            job["stage"], job["status"] = ("cancelled" if cmd == "cancel" else "archived"), "complete"
        event(job, cmd, {"actor": args.actor, "reason": args.reason})
        return job
    require(job["stage"] not in TERMINAL, "Job is terminal")
    if cmd == "start-build":
        require(job.get("layout_version") == 3 and job["stage"] == "build" and job["status"] == "awaiting-start",
                "No v3 build awaiting Start")
        lease(board, job)
        for item in job["artifacts"].values():
            intact(jobdir, item)
        require(args.actor.strip(), "Owner actor required")
        instruction = Path(args.instruction_file).read_text()
        require(instruction.strip() and args.decision == "accept-and-start", "Owner instruction and affirmative decision required")
        require(not re.search(r"\b(?:do not|don't|cannot|can't|will not|won't|never)\s+(?:\w+\s+){0,2}(?:accept|start)\b|\b(?:reject|decline)\b", instruction, re.I),
                "Instruction appears to reject acceptance or Start; obtain owner clarification")
        require(not re.search(r"^\s*(?:no|nope|not yet)\b|\b(?:decide|decision|approval)\b.{0,24}\b(?:later|pending|undecided)\b", instruction, re.I),
                "Instruction appears to withhold approval; obtain owner clarification")
        require(args.plan_sha256 == job["artifacts"]["plan"]["sha256"], "Plan approval identity mismatch")
        require(digest(inside(jobdir, "tasks/task.json")) == job["task_index_sha256"], "Task index changed")
        receipt = {"schema_version": 1, "actor": args.actor, "instruction": instruction, "decision": args.decision,
                   "plan_sha256": args.plan_sha256, "manifest_sha256": job["artifacts"]["tasks-manifest"]["sha256"],
                   "authority": "unverified caller assertion", "at": now()}
        receipt_path = f"records/operations/build-start-c{job['cycle']}.json"
        require(not inside(jobdir, receipt_path).exists(), "Build Start receipt already exists")
        atomic(inside(jobdir, receipt_path), receipt)
        job["artifacts"]["build-start"] = record(jobdir, receipt_path)
        job["status"] = "running"
        event(job, "start-build", {"receipt": job["artifacts"]["build-start"]})
        return job
    if cmd == "task-transition":
        require(job.get("layout_version") == 3 and job["stage"] == "build" and job["status"] == "running",
                "Task execution requires a started v3 build")
        require(job.get("snapshot") is None, "Task updates require an unsnapshotted build")
        lease(board, job)
        require(args.actor.strip() and args.reason.strip(), "Actor and reason required")
        for item in job["artifacts"].values():
            intact(jobdir, item)
        index_path = inside(jobdir, "tasks/task.json")
        v3_check_task_state(jobdir, job)
        manifest = json.loads(inside(jobdir, "tasks/manifest.json").read_text())
        owners = [phase for phase in manifest["phases"] if any(task["id"] == args.task for task in phase["tasks"])]
        require(len(owners) == 1, "Task not found in exactly one phase")
        phase = owners[0]["id"]
        paths = v3_candidate_paths(jobdir, job, "tasks", phase, during_build=True)
        stage_reviews.require_stage_pass(jobdir, "tasks", [ref(jobdir, path) for path in paths], phase)
        evidence = [ref(jobdir, path) for path in args.evidence]
        state = task_index.transition(jobdir, args.task, args.to, args.task_revision, evidence, actor=args.actor)
        job["task_index_sha256"] = digest(index_path)
        event(job, "task-transition", {"task": args.task, "to": args.to, "actor": args.actor,
                                       "reason": args.reason, "revision": state["revision"], "evidence": evidence})
        return state
    if cmd == "recover-activation":
        require(job["stage"] == "queued" and job["base_sha"] is None,
                "Recovery only completes an interrupted initial activation")
        require(args.actor.strip() and args.reason.strip() and args.quiesced,
                "Capture owner actor/reason and confirm all invocations stopped")
        lease(board, job)
        intent_path = jobdir / "runs" / ("activation-intent-c" + str(job["cycle"]) + ".json")
        require(intent_path.is_file(), "No recorded activation intent; preserve assets for owner reconciliation")
        intent = json.loads(intent_path.read_text())
        require(intent["job_id"] == job["job_id"] and intent["branch"] == job["branch"]
                and intent["worktree"] == job["worktree"] and intent["cycle"] == job["cycle"]
                and intent["artifacts"] == job["artifacts"], "Activation intent conflicts with registered job")
        require(job["approval"] is None and job["snapshot"] is None and job["round"] == 0,
                "Interrupted activation cannot recover review or approval state")
        require(not [i for i in inventory(repo, state, board)["issues"]
                     if i["kind"] not in {"target-moved", "queued-source-remnant"}],
                "Recovery has unrelated or dirty integrity findings")
        for item in job["artifacts"].values():
            intact(jobdir, item)
        base = intent["base_sha"]
        clean(repo)
        require(git(repo, "branch", "--show-current") == job["target"], "Primary checkout must be target")
        require(git(repo, "rev-parse", job["target"]) == base, "Target moved since activation intent; owner reconciliation required")
        require(git(repo, "rev-parse", job["branch"]) == base, "Branch differs from intended activation base; preserve source")
        eligible = sorted((j for j in board["jobs"] if j["stage"] == "queued"),
                          key=lambda j: (-j["priority"], board["jobs"].index(j)))
        require(eligible[0]["job_id"] == job["job_id"], "Reprioritize explicitly before activation recovery")
        worktree = Path(job["worktree"])
        if not worktree.exists():
            # Git can leave a branch without a worktree. Reuse only the exact
            # unmodified intended commit; never reset or force an occupied ref.
            registrations = git(repo, "worktree", "list", "--porcelain")
            require("branch refs/heads/" + job["branch"] not in registrations,
                    "Branch is already registered elsewhere; preserve and inspect")
            git(repo, "worktree", "add", job["worktree"], job["branch"])
        require(git(worktree, "branch", "--show-current") == job["branch"], "Conflicting worktree branch")
        require(git(worktree, "rev-parse", "HEAD") == base, "Worktree differs from intended activation base")
        clean(worktree)
        job["base_sha"] = base
        job["stage"], job["status"] = "analyze", "running"
        event(job, "recover-activation", {"actor": args.actor, "reason": args.reason,
                                           "intent_sha256": digest(intent_path), "base_sha": base})
    elif cmd == "activate":
        require(job["stage"] == "queued", "Only queued jobs can activate")
        lease(board, job)
        eligible = sorted((j for j in board["jobs"] if j["stage"] == "queued"),
                          key=lambda j: (-j["priority"], board["jobs"].index(j)))
        require(eligible and eligible[0]["job_id"] == job["job_id"], "Activate highest-priority queued job; reprioritize explicitly to override")
        require(not [i for i in inventory(repo, state, board)["issues"] if i["kind"] not in {"dirty-worktree", "target-moved"}], "Integrity issues; run integrity before activation")
        clean(repo)
        require(git(repo, "branch", "--show-current") == job["target"], "Primary checkout must be target")
        base = git(repo, "rev-parse", job["target"])
        require(not Path(job["worktree"]).exists(), "Worktree exists; run integrity and recover")
        intent = {"job_id": job["job_id"], "cycle": job["cycle"], "branch": job["branch"],
                  "worktree": job["worktree"], "base_sha": base, "artifacts": job["artifacts"],
                  "at": now(), "board_revision": board["revision"]}
        intent_path = jobdir / "runs" / ("activation-intent-c" + str(job["cycle"]) + ".json")
        if intent_path.exists():
            prior_intent = json.loads(intent_path.read_text())
            require(prior_intent["base_sha"] == base and prior_intent["artifacts"] == job["artifacts"]
                    and prior_intent["cycle"] == job["cycle"], "Activation intent changed; preserve and inspect")
        else:
            atomic(intent_path, intent)
        git(repo, "worktree", "add", "-b", job["branch"], job["worktree"], base)
        job["base_sha"] = base
        job["stage"], job["status"] = "analyze", "running"
        event(job, "activate", {"base_sha": base})
    elif cmd == "complete":
        stage = args.stage
        lease(board, job)
        require(job["stage"] == stage and job["status"] == "running", "Unexpected completion stage")
        if stage == "e2e":
            return e2e.complete(globals(), repo, jobdir, job, args, v3_start_review)
        if stage == "tasks":
            return v3_accept_tasks(jobdir, job, args)
        if stage == "build":
            return v3_accept_build(repo, jobdir, job, args)
        require(stage in {"analyze", "plan"},
                "Unsupported completion stage")
        review_stage = {"analyze": "spec", "plan": "plan"}[stage]
        paths = v3_candidate_paths(jobdir, job, review_stage)
        stage_reviews.require_stage_pass(jobdir, review_stage, [ref(jobdir, path) for path in paths])
        checked = handoff.validate(globals(), jobdir, job, stage, args.artifacts)
        expected, required = checked["expected"], checked["required"]
        records = {}
        for kind, path in zip(expected, args.artifacts):
            fields = checked["fields"][kind]
            if stage == "analyze":
                declared = fields["required_reviewers"]
                job["required_reviewers"] = declared
                # DEC-P01: the analysis mode declaration is validated when
                # present and tolerated when absent; DEC-P02/DEC-003: apply it
                # only without an active owner pin, refuse and report otherwise.
                declared_mode = fields.get("mode")
                if declared_mode is not None:
                    require(declared_mode in MODES, "Invalid analysis mode declaration")
                    if declared_mode != job["mode"]:
                        if mode_pin(job):
                            event(job, "mode-refused", {"declared": declared_mode, "pinned": job["mode"]})
                        else:
                            previous = job["mode"]
                            job["mode"] = declared_mode
                            event(job, "mode-applied", {"mode": declared_mode, "previous": previous})
            records[kind] = record(jobdir, path)
            if stage == "plan" and kind == "plan":
                e2e.pin(job, fields, metadata(inside(jobdir, path))[1], ref(jobdir, path))
            if stage == "plan" and kind == "plan" and fields.get("plan_format") == "compact-v1":
                records["plan-receipt"] = record(jobdir, plan_document.RECEIPT)
            if stage == "analyze" and fields.get("spec_format") == "compact-v1":
                records["spec-receipt"] = record(jobdir, spec_document.RECEIPT)
        if stage in {"analyze", "plan"}:
            records.update(v3_record_review_bundle(jobdir, {"analyze": "spec", "plan": "plan"}[stage]))
        job["artifacts"].update(records)
        job["stage"] = {"analyze": "plan", "plan": "tasks"}[stage]
        event(job, "complete-" + stage, records)
    elif cmd == "snapshot":
        e2e.check_policy(globals(), jobdir, job)
        require(job["stage"] == "build", "Snapshots are created during build")
        require(job["status"] == "running", "Build must be started before snapshot")
        require(job.get("snapshot") is None, "v3 build snapshot already exists")
        for item in job["artifacts"].values():
            intact(jobdir, item)
        state = v3_check_task_state(jobdir, job, require_done=True)
        manifest = json.loads(inside(jobdir, "tasks/manifest.json").read_text())
        for phase in manifest["phases"]:
            paths = v3_candidate_paths(jobdir, job, "tasks", phase["id"], during_build=True)
            stage_reviews.require_stage_pass(jobdir, "tasks", [ref(jobdir, path) for path in paths], phase["id"])
        clean(job["worktree"])
        require(git(job["worktree"], "branch", "--show-current") == job["branch"], "Wrong job branch")
        require(git(repo, "rev-parse", job["target"]) == job["base_sha"], "Target advanced; integrate and refresh-base")
        git(job["worktree"], "merge-base", "--is-ancestor", job["base_sha"], "HEAD")
        environment_file = Path(args.environment).resolve()
        environment = json.loads(environment_file.read_text())
        require(isinstance(environment, dict) and {"environment", "product", "version", "observed_at", "baseline"} <= environment.keys(), "Incomplete environment fingerprint")
        require(all(isinstance(environment[k], str) and environment[k].strip() for k in ("environment", "product", "version", "observed_at", "baseline")), "Invalid environment fingerprint fields")
        require(job["profiles"] == ["full-stack"] or args.exports, "Platform jobs require exported package evidence")
        exports = []
        for source in args.exports:
            path = Path(source).resolve()
            sha = digest(path)
            archive = "evidence/packages/" + sha + "/" + path.name
            target = inside(jobdir, archive)
            target.parent.mkdir(parents=True, exist_ok=True)
            if target.exists():
                require(digest(target) == sha, "Archived package modified")
            else:
                shutil.copyfile(path, target)
            exports.append({"original_path": str(path), "sha256": sha, "archive": archive})
        snap = {"schema_version": "1.0", "job_id": job["job_id"], "cycle": job["cycle"],
                "next_round": job["round"] + 1, "created_at": now(), "base_sha": job["base_sha"],
                "head_sha": git(job["worktree"], "rev-parse", "HEAD"),
                "environment": environment, "environment_file": str(environment_file),
                "environment_sha256": digest(environment_file), "exports": exports}
        snap["task_index_sha256"] = job["task_index_sha256"]
        sid = hashlib.sha256(json.dumps(snap, sort_keys=True).encode()).hexdigest()[:16]
        prefix = "records/snapshots/"
        path = prefix + f"source-c{job['cycle']}-r{job['round'] + 1}-{sid}.json"
        require(not (jobdir / path).exists(), "Snapshot already exists")
        atomic(jobdir / path, snap)
        job["snapshot"] = record(jobdir, path)
        event(job, "snapshot", job["snapshot"])
    elif cmd == "review":
        require(job["stage"] == "review" and args.discipline in job["required_reviewers"], "Unexpected review")
        require(job["reviews"][args.discipline]["verdict"] == "pending", "Review already recorded")
        expected = f"reviews/cycles/cycle-{job['cycle']}/round-{job['round']}/review-{args.discipline}.md"
        require(args.artifact == expected, "Final review must use canonical discipline path")
        source_check(repo, jobdir, job)
        inputs = [item["path"] for item in job["artifacts"].values()]
        fields = validate_artifact(jobdir, args.artifact, job, "review", {review_author(job, args.discipline)}, inputs, job["snapshot"]["path"])
        invocation = fields.get("invocation_id")
        require(isinstance(invocation, str) and invocation.strip() and invocation != job.get("build_invocation"),
                "Reviewer invocation must differ from builder")
        if "e2e" in job:
            qa_report = json.loads(inside(jobdir, job["e2e"]["passed"]["report"]["path"]).read_text())
            require(invocation != qa_report["invocation_id"], "Final reviewer invocation must differ from E2E QA")
        if job["mode"] != "light":
            require(all(item.get("invocation_id") != invocation for item in job["reviews"].values()),
                    "Full-mode review invocations must differ")
        require(fields.get("round") == job["round"] and fields.get("discipline") == args.discipline, "Wrong review round/discipline")
        require(fields.get("verdict") in {"PASS", "CHANGES", "ERROR"}, "Invalid verdict")
        job["reviews"][args.discipline] = {"verdict": fields["verdict"], **record(jobdir, args.artifact)}
        job["reviews"][args.discipline]["invocation_id"] = fields["invocation_id"]
        event(job, "review", {args.discipline: job["reviews"][args.discipline]})
        verdicts = {r["verdict"] for r in job["reviews"].values()}
        if "pending" not in verdicts:
            if "ERROR" in verdicts:
                block(job, "A required review could not complete", "review-error")
            elif "CHANGES" in verdicts:
                if job["round"] >= 2:
                    block(job, "Review rounds exhausted", "round-limit")
                else:
                    # Return to build with
                    # the snapshot cleared; the next complete-build creates round+1
                    # and re-reviews fresh bytes. Task evidence stays immutable.
                    job["stage"] = "build"
                    job["status"] = "running"
                    job["snapshot"] = None
                    if "e2e" in job:
                        job["e2e"]["passed"] = None
                        if job["e2e"]["attempts"] >= e2e.MAX_ATTEMPTS:
                            block(job, "E2E attempts exhausted; owner decision required", "e2e-exhausted")
            else:
                job["stage"] = "approve"
                job["status"] = "awaiting-recommendation"
    elif cmd == "decision":
        require(job["stage"] == "approve" and job["status"] == "awaiting-recommendation", "Unexpected decision")
        require(args.artifact == "reviews/final-review.md", "v3 decision must use reviews/final-review.md")
        source_check(repo, jobdir, job)
        inputs = [i["path"] for i in job["artifacts"].values()] + [i["path"] for i in job["reviews"].values()]
        for item in job["reviews"].values():
            require(item["verdict"] == "PASS", "Review not PASS")
            intact(jobdir, item)
        fields = validate_artifact(jobdir, args.artifact, job, "decision", {"engram-approver"}, inputs, job["snapshot"]["path"])
        invocation = fields.get("invocation_id")
        require(isinstance(invocation, str) and invocation.strip() and invocation != job.get("build_invocation")
                and all(item.get("invocation_id") != invocation for item in job["reviews"].values()),
                "Approver invocation must differ from builder and reviewers")
        require(fields.get("recommendation") in {"APPROVE", "ESCALATE"}, "Invalid recommendation")
        job["artifacts"]["decision"] = record(jobdir, args.artifact)
        if fields["recommendation"] == "ESCALATE":
            block(job, "Approver escalated; see decision artifact", "decision")
        else:
            job["status"] = "awaiting-approval"
        event(job, "decision", fields["recommendation"])
    elif cmd == "approve":
        require(job["stage"] == "approve" and job["status"] == "awaiting-approval", "Not awaiting human approval")
        source_check(repo, jobdir, job)
        for item in job["reviews"].values():
            intact(jobdir, item)
        instruction = Path(args.instruction_file).read_text()
        require(instruction.strip() and args.actor.strip(), "Capture actual approving instruction and actor")
        require(args.decision == "approve", "v3 approval needs explicit orchestrator approval assertion")
        require(not re.search(r"^\s*(?:no|nope|not yet)\b|\b(?:do not|don't|cannot|can't|will not|won't|never)\s+(?:\w+\s+){0,2}(?:approve|merge)\b|\b(?:reject|decline)\b|\b(?:decide|decision|approval)\b.{0,24}\b(?:later|pending|undecided)\b", instruction, re.I),
                "Instruction appears to withhold approval; obtain owner clarification")
        job["approval"] = {"actor": args.actor, "instruction": instruction, "at": now(),
                           "snapshot_sha256": job["snapshot"]["sha256"]}
        job["approval"].update({"authority": "unverified caller assertion",
                                "artifact_hashes": {key: item["sha256"] for key, item in job["artifacts"].items()},
                                "review_hashes": {key: item["sha256"] for key, item in job["reviews"].items()}})
        job["stage"], job["status"] = "merge", "approved"
        event(job, "human-approval", job["approval"])
        emit_evidence(jobdir, job, "job.approved", "owner",
                      cycle=job["cycle"],
                      approver=str(job["approval"].get("approver", "owner")),
                      artifact_hashes={k: v["sha256"] for k, v in job["artifacts"].items()})
    elif cmd == "merge":
        require(job["stage"] == "merge" and job["approval"], "No human approval")
        snap = source_check(repo, jobdir, job, check_target=False)
        require(job["approval"]["snapshot_sha256"] == job["snapshot"]["sha256"], "Stale approval")
        require(job["approval"].get("artifact_hashes") == {key: item["sha256"] for key, item in job["artifacts"].items()}
                and job["approval"].get("review_hashes") == {key: item["sha256"] for key, item in job["reviews"].items()},
                "Approved evidence identities changed")
        for item in job["reviews"].values():
            intact(jobdir, item)
        clean(repo)
        require(git(repo, "branch", "--show-current") == job["target"], "Primary checkout is not target")
        target = git(repo, "rev-parse", job["target"])
        require(target in {snap["base_sha"], snap["head_sha"]}, "Target advanced; approval invalid")
        if target != snap["head_sha"]:
            # Merge the approved object, never a branch ref that can move after validation.
            git(repo, "merge", "--ff-only", snap["head_sha"])
        job["stage"], job["status"] = "merged", "complete"
        event(job, "merged", {"head_sha": snap["head_sha"], "platform_promoted": False})
        emit_evidence(jobdir, job, "job.released", "owner",
                      merge_commit=snap["head_sha"],
                      release_notes_ref=str(job["artifacts"].get("release", {}).get("path", "")))
    elif cmd == "rework":
        lease(board, job)
        require(job["stage"] in {"approve", "merge", "needs-human"}, "Human rework is allowed at a decision boundary")
        note = Path(args.note_file).read_text()
        require(note.strip(), "Empty rework note")
        prior = {"cycle": job["cycle"], "round": job["round"], "reviews": job["reviews"],
                 "snapshot": job["snapshot"], "approval": job["approval"], "artifacts": job["artifacts"].copy()}
        job["cycle"] += 1
        job["round"] = 0
        job["approval"], job["snapshot"], job["blocker"], job["resume_from"] = None, None, None, None
        job["reviews"], job["required_reviewers"] = {}, []
        job["stage"], job["status"] = (("queued", "queued") if job["base_sha"] is None else ("analyze", "running"))
        # Rework conservatively revalidates all upstream contracts; preserve old bytes first.
        archive = jobdir / "history" / ("cycle-" + str(job["cycle"] - 1))
        archive.mkdir(parents=True, exist_ok=True)
        for name in ("spec", "plan", "release"):
            path = jobdir / (name + ".md")
            if path.exists():
                shutil.copyfile(path, archive / path.name)
                path.unlink()
        job["artifacts"] = {"feature": prior["artifacts"]["feature"]}
        if "brief" in prior["artifacts"]:
            job["artifacts"]["brief"] = prior["artifacts"]["brief"]  # intent contract survives rework
        path = "rework-" + str(job["cycle"]) + ".md"
        (jobdir / path).write_text(render("rework", job, human_note=note))
        job["artifacts"]["rework"] = record(jobdir, path)
        job.pop("task_index_sha256", None)
        job.pop("build_invocation", None)
        job.pop("e2e", None)
        prebuild = jobdir / "reviews" / "prebuild"
        if prebuild.exists():
            shutil.copytree(prebuild, archive / "reviews-prebuild", dirs_exist_ok=True)
            shutil.rmtree(prebuild)
        for area in ("tasks", "context"):
            source = jobdir / area
            if source.exists():
                shutil.copytree(source, archive / area, dirs_exist_ok=True)
                shutil.rmtree(source)
        scaffold_v3(jobdir, job)
        event(job, "rework", {"previous": prior, "instruction": job["artifacts"]["rework"]})
        emit_evidence(jobdir, job, "job.reworked", "owner",
                      cycle=job["cycle"], note=note.strip(),
                      review_refs=[r["path"] for r in job["artifacts"].values() if "rework" in str(r.get("path", ""))])
    elif cmd == "refresh-base":
        require(job["stage"] == "build" and job["round"] == 0, "Refresh base before first review of a cycle")
        clean(job["worktree"])
        base = git(repo, "rev-parse", job["target"])
        git(job["worktree"], "merge-base", "--is-ancestor", base, "HEAD")
        job["base_sha"], job["snapshot"], job["approval"] = base, None, None
        event(job, "refresh-base", {"base_sha": base})
    elif cmd == "block":
        require(args.quiesced, "Stop and confirm all job invocations quiesced before releasing execution lease")
        require(job["stage"] != "needs-human", "Already blocked")
        block(job, args.reason)
        event(job, "block", job["blocker"])
    elif cmd == "resume":
        lease(board, job)
        require(job["stage"] == "needs-human" and job["blocker"]["code"] in {"manual", "review-error", "prebuild-review-error"}, "Use explicit rework for exhausted rounds or escalated decisions")
        previous = job["resume_from"]
        if job["blocker"]["code"] == "prebuild-review-error":
            require(previous["stage"] in {"analyze", "plan"}, "Invalid pre-build recovery stage")
            require(git(job["worktree"], "branch", "--show-current") == job["branch"], "Wrong worktree branch")
            clean(job["worktree"])
            require(git(job["worktree"], "rev-parse", "HEAD") == job["base_sha"], "Worktree source changed")
            require(git(repo, "rev-parse", job["target"]) == job["base_sha"], "Target advanced; reconcile source before resuming")
            for item in job["artifacts"].values():
                intact(jobdir, item)
            stage = {"analyze": "spec", "plan": "plan"}[previous["stage"]]
            current = stage_reviews.review_status(jobdir, stage)
            require(current["verdict"] == "ERROR" and current["candidate"] < 3, "Review cannot safely retry")
        if job["blocker"]["code"] == "review-error":
            source_check(repo, jobdir, job)
            for reviewer, item in job["reviews"].items():
                if item["verdict"] == "ERROR":
                    job["reviews"][reviewer] = {"verdict": "pending"}
        if (job["blocker"]["code"] == "manual" and previous["stage"] == "review"
                and "CHANGES" in {item["verdict"] for item in job["reviews"].values()}
                and job["round"] < 2):
            # A final-review CHANGES block reopens the
            # build loop; the next complete-build resets reviews for round+1.
            job["stage"], job["status"], job["snapshot"] = "build", "running", None
        else:
            job["stage"], job["status"] = previous["stage"], previous["status"]
        job["blocker"], job["resume_from"] = None, None
        event(job, "resume", {"reason": args.reason})
    else:
        raise ValueError("Unsupported command")
    return job


def parser():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--repo", default=".", help="Primary repository checkout")
    p.add_argument("--expected-revision", type=int)
    sub = p.add_subparsers(dest="command", required=True)
    init = sub.add_parser("init")
    init.add_argument("--target", default="main")
    create = sub.add_parser("create")
    create.add_argument("--priority", type=int, default=0, help="Higher first; ties use creation order")
    create.add_argument("--feature-file", required=True)
    create.add_argument("--brief-file", help="Ratified engram-frame brief; choose this or an exact registered brief revision")
    create.add_argument("--brief-id", help="Bind a registered brief ID; requires --brief-revision")
    create.add_argument("--brief-revision", type=int)
    create.add_argument("--slug", required=True)
    create.add_argument("--profiles", default="full-stack", help="Comma-separated profiles")
    create.add_argument("--mode", choices=MODES, help="Owner pin: full (default) or light team mode")
    sub.add_parser("integrity")
    for command in ("spec-context", "spec-check", "spec-session-open", "spec-submit",
                    "plan-context", "plan-check", "plan-session-open", "plan-submit"):
        q = sub.add_parser(command)
        q.add_argument("--job", required=True)
        if command in {"spec-session-open", "spec-submit", "plan-session-open", "plan-submit"}:
            q.add_argument("--author", required=True)
        if command in {"spec-session-open", "plan-session-open"}:
            q.add_argument("--reviewers-file", required=True)
        if command in {"spec-submit", "plan-submit"}:
            q.add_argument("--operation-id", required=True)
            q.add_argument("--context-sha256", required=True)
            q.add_argument("--spec-sha256" if command == "spec-submit" else "--plan-sha256", required=True)
            if command == "plan-submit":
                q.add_argument("--draft-sha256", required=True)
    plan_prepare = sub.add_parser("plan-prepare")
    plan_prepare.add_argument("--job", required=True)
    prepare = sub.add_parser("spec-prepare")
    prepare.add_argument("--job", required=True)
    preflight = sub.add_parser("preflight")
    preflight.add_argument("--job", required=True)
    preflight.add_argument("--stage", choices=("spec", "analyze", "plan", "tasks"), required=True)
    for name in ("brief-create", "brief-supersede", "brief-list", "brief-show"):
        b = sub.add_parser(name)
        if name in {"brief-supersede", "brief-show"}:
            b.add_argument("--brief-id", required=True)
        if name == "brief-show":
            b.add_argument("--brief-revision", type=int)
        if name in {"brief-create", "brief-supersede"}:
            b.add_argument("--file", required=True)
            b.add_argument("--actor", required=True)
            b.add_argument("--instruction-file", required=True)
            b.add_argument("--expected-brief-revision", type=int, required=True)
            b.add_argument("--operation-id", required=True)
    recovery = sub.add_parser("recover")
    recovery.add_argument("--board-file", required=True, help="Intact backed-up board, never inferred history")
    recovery.add_argument("--apply-sha256", help="Reserved; recovery apply is unsupported pending independently verified provenance")
    recovery.add_argument("--actor")
    status = sub.add_parser("status")
    status.add_argument("--job")
    for cmd in ("complete", "snapshot", "review", "decision", "approve", "merge", "rework", "block", "resume", "refresh-base", "validate-artifact", "activate", "priority", "cancel", "archive", "restore", "recover-activation", "review-freeze", "review-record", "review-status", "review-clarification", "tasks-initialize", "tasks-reinitialize", "start-build", "task-transition"):
        q = sub.add_parser(cmd)
        q.add_argument("--job", required=True)
        if cmd in {"priority", "cancel", "archive", "restore", "recover-activation"}:
            q.add_argument("--actor", required=True)
            q.add_argument("--reason", required=True)
        if cmd == "priority":
            q.add_argument("--priority", type=int, required=True)
        if cmd == "complete":
            q.add_argument("--stage", choices=("analyze", "plan", "tasks", "build", "e2e"), required=True)
            q.add_argument("--artifacts", nargs="+", required=True, help="Job-relative paths in documented order")
            q.add_argument("--snapshot")
            q.add_argument("--reviewers")
        if cmd == "snapshot":
            q.add_argument("--environment", required=True, help="Sanitized JSON fingerprint")
            q.add_argument("--exports", nargs="*", default=[], help="Exported package files")
        if cmd in {"review", "decision", "validate-artifact"}:
            q.add_argument("--artifact", required=True)
        if cmd == "validate-artifact":
            q.add_argument("--snapshot")
        if cmd == "review":
            q.add_argument("--discipline", choices=REVIEWERS, required=True)
        if cmd == "approve":
            q.add_argument("--actor", required=True)
            q.add_argument("--instruction-file", required=True)
            q.add_argument("--decision", choices=("approve",), help="Explicit v3 human approval assertion; not authentication")
        if cmd == "rework":
            q.add_argument("--note-file", required=True)
        if cmd in {"block", "recover-activation"}:
            q.add_argument("--quiesced", action="store_true", help="All job agent/process invocations have stopped")
        if cmd in {"block", "resume"}:
            q.add_argument("--reason", required=True)
        if cmd in {"review-freeze", "review-record", "review-status", "review-clarification"}:
            q.add_argument("--stage", choices=("spec", "plan", "tasks"), required=True)
            q.add_argument("--phase")
        if cmd == "review-clarification":
            q.add_argument("--question", required=True)
            q.add_argument("--answer", required=True)
            q.add_argument("--owner", required=True)
            q.add_argument("--reference", required=True)
        if cmd == "review-freeze":
            q.add_argument("--author", required=True)
            q.add_argument("--reviewers-file", required=True, help="Job-relative reviewer-to-invocation JSON")
        if cmd == "review-record":
            q.add_argument("--candidate", type=int, required=True)
            q.add_argument("--role", required=True)
            q.add_argument("--invocation", required=True)
            q.add_argument("--verdict", choices=("PASS", "CHANGES", "ERROR"), required=True)
            q.add_argument("--findings-file", required=True, help="Job-relative findings/resolves JSON")
        if cmd == "tasks-initialize":
            q.add_argument("--manifest", required=True, help="Job-relative candidate task manifest JSON")
        if cmd == "tasks-reinitialize":
            q.add_argument("--manifest", required=True, help="Job-relative revised candidate task manifest JSON; only a fully pending index may be replaced")
        if cmd == "start-build":
            q.add_argument("--actor", required=True)
            q.add_argument("--instruction-file", required=True)
            q.add_argument("--plan-sha256", required=True)
            q.add_argument("--decision", choices=("accept-and-start",), required=True,
                           help="Orchestrator asserts actual owner acceptance and Start; not authentication")
        if cmd == "task-transition":
            q.add_argument("--task", required=True)
            q.add_argument("--to", choices=("in_progress", "blocked", "done"), required=True)
            q.add_argument("--task-revision", type=int, required=True)
            q.add_argument("--actor", required=True)
            q.add_argument("--reason", required=True)
            q.add_argument("--evidence", nargs="*", default=[])
    return p


def check_format_headers(state, board):
    """Reject unsupported live artifacts without creating locks or backups."""
    for job in board["jobs"]:
        jobdir = state / "jobs" / job["job_id"]
        for kind in ("spec", "plan"):
            path = jobdir / (kind + ".md")
            if path.is_file():
                fields, _ = plan_document.header(path.read_text())
                require(type(fields.get("schema_version")) is int and fields["schema_version"] == 2
                        and fields.get(kind + "_format") == "compact-v1",
                        "Unsupported " + kind + " format: current factory requires compact-v1; preserve existing records")


def check_command_formats(args, state, board):
    """Reject obsolete artifact interfaces before lock creation; this never accepts work."""
    if args.command == "complete":
        paths = args.artifacts
        if args.stage == "analyze":
            require(paths == ["spec.md"], "Current analysis handoff requires spec.md")
        elif args.stage == "plan":
            require(paths and paths[0] == "plan.md" and set(paths[1:]) <= set(plan_document.CONTEXT),
                    "Current plan handoff requires plan.md and declared context documents")
        elif args.stage == "tasks":
            require(paths == ["tasks/manifest.json", "tasks/task.json", "release.md"],
                    "Current tasks handoff requires manifest, task index and release")
        elif args.stage == "build":
            require(len(paths) == 1 and re.fullmatch(r"records/operations/build-handoff-c[0-9]+-r[0-9]+(?:-a[0-9]+)?\.md", paths[0]),
                    "Current build handoff requires a canonical round-scoped operations record")
        elif args.stage == "e2e":
            require(len(paths) == 1 and re.fullmatch(r"records/e2e/report-c[0-9]+-a[0-9]+\.json", paths[0]),
                    "Current E2E handoff requires a canonical attempt report")
    elif args.command == "review":
        require(re.fullmatch(r"reviews/cycles/cycle-[0-9]+/round-[0-9]+/review-[a-z]+\.md", args.artifact),
                "Current review requires a canonical cycle/round discipline record")
    elif args.command == "decision":
        require(args.artifact == "reviews/final-review.md", "Current decision requires reviews/final-review.md")
    elif args.command == "validate-artifact":
        job = selected(board, args.job)
        validate_artifact(state / "jobs" / job["job_id"], args.artifact, job, snapshot=args.snapshot)


def main(argv=None):
    args = parser().parse_args(argv)
    repo = Path(git(Path(args.repo).resolve(), "rev-parse", "--show-toplevel"))
    # Require the primary checkout, not a linked worktree with a separate runtime directory.
    common = Path(git(repo, "rev-parse", "--path-format=absolute", "--git-common-dir"))
    actual = Path(git(repo, "rev-parse", "--path-format=absolute", "--git-dir"))
    require(common == actual, "Pass --repo as the primary checkout, not the job worktree")
    state, boardpath = repo / ".engram-factory", repo / ".engram-factory" / "board.json"
    if boardpath.exists():
        admitted = json.loads(boardpath.read_text())
        check_board(admitted)
        check_format_headers(state, admitted)
        check_command_formats(args, state, admitted)
    if args.command in {"spec-context", "spec-check", "plan-context", "plan-check"}:
        before = boardpath.read_bytes()
        board = json.loads(before)
        check_board(board)
        require(args.expected_revision is None or args.expected_revision == board["revision"], "Stale board revision")
        job = selected(board, args.job)
        jobdir = state / "jobs" / job["job_id"]
        interface = spec_session if args.command.startswith("spec-") else plan_session
        document = "spec.md" if args.command.startswith("spec-") else "plan.md"
        if args.command.endswith("-context"):
            result = interface.context(globals(), jobdir, job, board["revision"])
        else:
            initial = inside(jobdir, document).read_bytes()
            try:
                result = interface.check(globals(), jobdir, job)
                require(initial == inside(jobdir, document).read_bytes(), "Draft changed during structural check")
            except (ValueError, OSError, KeyError, TypeError) as exc:
                result = {"structural": "FAIL", "findings": [str(exc)], "independent_review": "required"}
        require(before == boardpath.read_bytes(), "Controller state changed during read; retry")
        print(json.dumps({"revision": board["revision"], "result": result}, indent=2))
        return
    if args.command == "preflight":
        before = boardpath.read_bytes()
        board = json.loads(before)
        check_board(board)
        require(args.expected_revision is None or args.expected_revision == board["revision"], "Stale board revision")
        job = selected(board, args.job)
        stage = "analyze" if args.stage == "spec" else args.stage
        jobdir = state / "jobs" / job["job_id"]
        paths = handoff.artifacts_for(globals(), jobdir, job, stage)
        # Capture both sides: never label changing bytes as a stable preflight.
        inputs = paths + [item["path"] for item in job["artifacts"].values()]
        inputs.append(spec_document.RECEIPT)
        if stage == "tasks":
            manifest = json.loads(inside(jobdir, "tasks/manifest.json").read_text())
            inputs += [t["document"] for p in manifest["phases"] for t in p["tasks"]]
        compact_plan = stage == "plan"
        if compact_plan:
            inputs.append(plan_document.RECEIPT)
        for path in paths:
            if path.endswith(".md") and not (compact_plan and path != "plan.md"):
                fields, _ = metadata(inside(jobdir, path))
                inputs.extend(item["path"] for item in fields.get("input_artifacts", []))
        refs = [ref(jobdir, p) for p in sorted(set(inputs))]
        handoff.validate(globals(), jobdir, job, stage, paths)
        require(refs == [ref(jobdir, r["path"]) for r in refs] and before == boardpath.read_bytes(),
                "Inputs changed during preflight; retry on stable bytes")
        print(json.dumps({"revision": board["revision"], "result": {"stage": stage, "structural": "PASS",
              "artifacts": paths, "inputs": refs, "board_sha256": hashlib.sha256(before).hexdigest(),
              "independent_review": "required", "authority": "no Start or Finish granted"}}, indent=2))
        return
    if args.command in {"brief-create", "brief-supersede", "brief-list", "brief-show"}:
        require(boardpath.exists(), "Missing board; inspect factory state before using brief registry")
        registry = brief_registry.Registry(state)
        if args.command == "brief-list":
            result = registry.list()
        elif args.command == "brief-show":
            meta, content = registry.get(args.brief_id, args.brief_revision)
            result = {"metadata": meta, "content": content.decode("utf-8")}
        else:
            kwargs = {"actor": args.actor, "instruction": Path(args.instruction_file).read_text(),
                      "expected_revision": args.expected_brief_revision, "operation_id": args.operation_id}
            content = Path(args.file).read_bytes()
            result = (registry.create(content, **kwargs) if args.command == "brief-create"
                      else registry.supersede(args.brief_id, content, **kwargs))
        print(json.dumps(result, indent=2))
        return
    if args.command == "integrity":
        board = json.loads(boardpath.read_text()) if boardpath.exists() else None
        if board is not None:
            check_board(board)
        print(json.dumps(inventory(repo, state, board), indent=2))
        return
    if args.command == "recover":
        candidate_path = Path(args.board_file)
        candidate = json.loads(candidate_path.read_text())
        check_board(candidate)
        check_format_headers(state, candidate)
        report = inventory(repo, state, candidate)
        report["candidate_sha256"] = digest(candidate_path)
        report["apply_supported"] = False
        report["next_action"] = ("Preserve board candidates, job files and all source worktrees. "
                                 "Owner must reconcile missing history and invocation state independently; "
                                 "this controller cannot authenticate historical approval or repair orphan adoption.")
        require(not args.apply_sha256,
                "Recovery is preview-only: checksums cannot authenticate approval/history. "
                "Preserve all assets and obtain independently verified owner reconciliation; no automatic apply is supported")
        print(json.dumps(report, indent=2))
        return
    if args.command == "init":
        require(not boardpath.exists(), "Factory already initialized")
        git(repo, "check-ref-format", "--branch", args.target)
        git(repo, "rev-parse", "--verify", "refs/heads/" + args.target)
        require(not git(repo, "ls-files", ".engram-factory"), "Runtime state must not be tracked")
        report = inventory(repo, state)
        require(len(report["issues"]) == 1, "Orphan factory state; run integrity and preview recovery")
        exclude = common / "info" / "exclude"
        exclude.parent.mkdir(parents=True, exist_ok=True)
        existing = exclude.read_text() if exclude.exists() else ""
        if "/.engram-factory/" not in existing.splitlines():
            exclude.write_text(existing.rstrip() + "\n/.engram-factory/\n")
        state.mkdir(parents=True, exist_ok=True)
        with (state / "board.lock").open("a") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            require(not boardpath.exists(), "Factory already initialized")
            require(len(inventory(repo, state)["issues"]) == 1, "Orphan factory state; run integrity")
            initial = {"schema_version": "2.0", "revision": 0, "target": args.target, "jobs": []}
            checkpoint(common, initial)
            atomic(boardpath, initial)
        print(json.dumps({"board": str(boardpath), "target": args.target}))
        return
    require(boardpath.exists(), "Missing board; run integrity before init or recovery, preserving any existing factory assets")
    with (state / "board.lock").open("a") as lock:
        read_only = args.command in {"status", "validate-artifact", "review-status"}
        fcntl.flock(lock, fcntl.LOCK_SH if read_only else fcntl.LOCK_EX)
        board = json.loads(boardpath.read_text())
        check_board(board)
        check_format_headers(state, board)
        if args.command in {"spec-submit", "plan-submit"}:
            interface = spec_session if args.command == "spec-submit" else plan_session
            job = selected(board, args.job)
            replay = interface.replay(globals(), state / "jobs" / job["job_id"], job, args)
            if replay is not None:
                print(json.dumps({"revision": board["revision"], "result": replay}, indent=2))
                return
        require(args.expected_revision is None or args.expected_revision == board["revision"], "Stale board revision")
        result = execute(args, repo, state, board)
        if not read_only:
            board["revision"] += 1
            check_board(board)
            checkpoint(common, board)
            atomic(boardpath, board)
        print(json.dumps({"revision": board["revision"], "result": result}, indent=2))


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, KeyError, TypeError) as exc:
        print(json.dumps({"error": str(exc)}), file=sys.stderr)
        sys.exit(2)
