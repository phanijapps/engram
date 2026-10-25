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

ROOT = Path(__file__).resolve().parents[1]
PROFILES = ("full-stack", "servicenow", "salesforce", "sap", "maximo")
REVIEWERS = ("code", "security", "platform", "ux", "ui", "backend", "data", "operations")
MANDATORY = {"code", "security", "platform"}
STAGES = ("analyze", "plan", "tasks", "build", "review", "approve", "merge", "merged", "needs-human")


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
    return fields, body


def ref(jobdir, path):
    absolute = inside(jobdir, path)
    require(absolute.is_file(), "Missing artifact: " + path)
    return {"path": path, "sha256": digest(absolute)}


def validate_artifact(jobdir, path, job, kind=None, roles=None, required=(), snapshot=None):
    absolute = inside(jobdir, path)
    fields, body = metadata(absolute)
    # Template placeholders are lowercase words (render keys and <profile>_<phase>_evidence);
    # evidence quoting real Jinja/Handlebars syntax ({{ user.name }}, {{x|default}}) stays legal.
    require(not re.search(r"\{\{[a-z_][a-z0-9_-]*\}\}", absolute.read_text()), "Unresolved placeholders: " + path)
    keys = {"schema_version", "artifact_type", "job_id", "cycle", "status", "author_role",
            "profiles", "source_snapshot", "input_artifacts", "updated_at"}
    require(keys <= fields.keys(), "Missing common artifact metadata")
    require(fields["schema_version"] == "1.0", "Unsupported artifact version")
    require(fields["status"] == "ready", "Artifact is not ready: " + path)
    require(fields["job_id"] == job["job_id"], "Wrong artifact job")
    require(type(fields["cycle"]) is int and fields["cycle"] == job["cycle"], "Wrong artifact cycle")
    require(fields["profiles"] == job["profiles"], "Wrong artifact profiles")
    require(not kind or fields["artifact_type"] == kind, "Wrong artifact type")
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
    if name in {"analysis", "plan", "tasks", "verification", "release"}:
        for profile in job["profiles"]:
            source = (ROOT / "references" / (profile + ".md")).read_text()
            section = re.search(r"^## " + name.capitalize() + r"\n(.*?)(?=^## |\Z)", source, re.M | re.S)
            if section:
                text += "\n## Profile: " + profile + "\n\n" + section.group(1).strip()
                text += "\n\nJob-specific response and evidence: {{" + profile + "_" + name + "_evidence}}\n"
    return text


def scaffold(jobdir, job):
    # Render every document before writing any, so a bad template or profile
    # cannot leave a half-scaffolded job directory behind.
    documents = [(name, render(name, job)) for name in ("analysis", "plan", "tasks", "verification", "release", "build", "decision")]
    for name, text in documents:
        path = jobdir / (name + ".md")
        if not path.exists():
            path.write_text(text)


def check_board(board):
    require(board.get("schema_version") == "1.0", "Unsupported board schema")
    require(type(board.get("revision")) is int and board["revision"] >= 0, "Invalid revision")
    require(isinstance(board.get("jobs"), list), "Invalid jobs")
    ids = set()
    for job in board["jobs"]:
        require(job["job_id"] not in ids, "Duplicate job ID")
        ids.add(job["job_id"])
        require(job["stage"] in STAGES, "Invalid stage")
        require(1 <= job["cycle"] and 0 <= job["round"] <= 3, "Invalid cycle/round")
        require(set(job["profiles"]) <= set(PROFILES), "Unknown profile")
        require(set(job["reviews"]) <= set(job["required_reviewers"]), "Unexpected reviewer")
    require(sum(j["stage"] != "merged" for j in board["jobs"]) <= 1, "Multiple active jobs")


def source_check(repo, jobdir, job, check_target=True):
    require(job["snapshot"], "No source snapshot")
    intact(jobdir, job["snapshot"])
    snapshot = json.loads(inside(jobdir, job["snapshot"]["path"]).read_text())
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
        return selected(board, args.job) if args.job else board
    if cmd == "create":
        require(not any(j["stage"] != "merged" for j in board["jobs"]), "An active job already exists")
        clean(repo)
        require(git(repo, "branch", "--show-current") == board["target"], "Primary checkout must be on target")
        require(re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+){0,7}", args.slug), "Use a short lowercase kebab-case slug")
        feature = Path(args.feature_file).read_text()
        require(feature.strip(), "Empty feature")
        brief = Path(args.brief_file).read_text() if args.brief_file else None
        require(brief is None or brief.strip(), "Empty brief")
        if brief is not None:
            # A registered brief is an authority contract: it must be structurally
            # sound and must frame THIS feature. Bytes alone do not qualify.
            require("## The ask" in brief and "## Owner ratification" in brief
                    and "## Decision ledger" in brief,
                    "Brief must carry The ask, Decision ledger and Owner ratification sections")
            require(feature.strip() in brief, "Brief's ask does not contain the feature text verbatim")
            require(re.search(r"\|[^|]*\|[^|]*\|[^|]*\|[^|]*\"[^|]*\|", brief.split("## Owner ratification")[1]),
                    "Owner ratification must carry a non-empty row with a verbatim quoted confirmation")
        profiles = args.profiles.split(",")
        require(profiles and len(set(profiles)) == len(profiles) and set(profiles) <= set(PROFILES), "Invalid profiles")
        number = max([int(j["job_id"].split("-")[0]) for j in board["jobs"]] or [0]) + 1
        job_id = f"{number:03d}-{args.slug}"
        jobdir, worktree = state / "jobs" / job_id, state / "worktrees" / job_id
        require(not jobdir.exists() and not worktree.exists(), "Orphan job/worktree exists; inspect before recovery")
        for profile in profiles:
            require((ROOT / "references" / (profile + ".md")).is_file(), "Missing profile reference: " + profile)
        branch, base = "factory/" + job_id, git(repo, "rev-parse", board["target"])
        job = {"job_id": job_id, "feature": feature, "profiles": profiles,
               "created_at": now(), "updated_at": now(), "stage": "analyze", "status": "running",
               "cycle": 1, "round": 0, "target": board["target"], "branch": branch,
               "base_sha": base, "worktree": str(worktree), "snapshot": None,
               "required_reviewers": [], "reviews": {}, "artifacts": {}, "approval": None,
               "blocker": None, "resume_from": None, "history": []}
        jobdir.mkdir(parents=True)
        (jobdir / "feature.md").write_text(feature)
        job["artifacts"]["feature"] = record(jobdir, "feature.md")
        if brief is not None:
            (jobdir / "brief.md").write_text(brief)
            job["artifacts"]["brief"] = record(jobdir, "brief.md")
        scaffold(jobdir, job)
        # All job content exists; only now mutate Git. A crash before this line
        # leaves at most an inspectable job directory, never an orphan worktree.
        git(repo, "worktree", "add", "-b", branch, str(worktree), base)
        event(job, "create", {"base_sha": base})
        board["jobs"].append(job)
        return job
    job = selected(board, args.job)
    jobdir = state / "jobs" / job["job_id"]
    if cmd == "validate-artifact":
        return validate_artifact(jobdir, args.artifact, job, snapshot=args.snapshot)
    require(job["stage"] != "merged", "Job already merged")
    if cmd == "complete":
        stage = args.stage
        require(job["stage"] == stage and job["status"] == "running", "Unexpected completion stage")
        expected = {"analyze": ["analysis"], "plan": ["plan"], "tasks": ["tasks", "verification", "release"],
                    "build": ["build", "tasks", "verification", "release"]}[stage]
        require(len(args.artifacts) == len(expected), "Wrong artifact count/order: " + str(expected))
        upstream = {"analyze": ["feature"], "plan": ["analysis"], "tasks": ["analysis", "plan"],
                    "build": ["analysis", "plan"]}[stage]
        if "rework" in job["artifacts"]:
            upstream = upstream + ["rework"]
        if stage == "analyze" and "brief" in job["artifacts"]:
            upstream = upstream + ["brief"]  # registered framing is a required input
            # Every ledger row id in the registered brief must be addressed by the
            # analysis body — presence, not fidelity (fidelity is reviewed by humans).
            brief_text = inside(jobdir, "brief.md").read_text()
            for led in sorted(set(re.findall(r"LED-\d+", brief_text))):
                require(led in inside(jobdir, args.artifacts[0]).read_text(),
                        f"Analysis does not address brief ledger row {led}")
        for name in upstream:
            intact(jobdir, job["artifacts"][name])
        required = [job["artifacts"][n]["path"] for n in upstream]
        for name, item in job["artifacts"].items():
            if stage != "build" or name not in {"tasks", "verification", "release", "build"}:
                intact(jobdir, item)
        if stage == "build":
            require(args.snapshot and job["snapshot"] and args.snapshot == job["snapshot"]["path"], "Supply current snapshot")
            # Mutable build inputs are validated below; immutable upstream evidence must remain intact.
            mutable = {k: job["artifacts"].pop(k) for k in ("tasks", "verification", "release", "build") if k in job["artifacts"]}
            try:
                source_check(repo, jobdir, job)
            finally:
                job["artifacts"].update(mutable)
            reviewers = args.reviewers.split(",") if args.reviewers else []
            require(len(reviewers) == len(set(reviewers)) and MANDATORY <= set(reviewers) <= set(REVIEWERS), "Invalid required reviewers")
            require(set(job["required_reviewers"]) <= set(reviewers), "Cannot omit previously required reviewers")
            require(job["round"] < 3, "Review rounds exhausted")
        records = {}
        for kind, path in zip(expected, args.artifacts):
            roles = {"analyze": {"engram-analyzer"}, "plan": {"engram-planner"}, "tasks": {"engram-tasks"},
                     "build": {"engram-builder"}}[stage]
            needed = required + ([p for p in args.artifacts[1:]] if kind == "build" else [])
            fields = validate_artifact(jobdir, path, job, kind, roles, needed, args.snapshot if stage == "build" else None)
            if stage == "analyze":
                declared = fields.get("required_reviewers")
                require(isinstance(declared, list) and all(isinstance(r, str) for r in declared), "Analysis must declare required_reviewers")
                require(len(declared) == len(set(declared)) and MANDATORY <= set(declared) <= set(REVIEWERS), "Invalid analysis reviewer set")
                job["required_reviewers"] = declared
            records[kind] = record(jobdir, path)
        job["artifacts"].update(records)
        if stage == "build":
            job["round"] += 1
            job["required_reviewers"] = reviewers
            job["reviews"] = {r: {"verdict": "pending"} for r in reviewers}
            job["stage"] = "review"
            (jobdir / f"cycle-{job['cycle']}" / f"round-{job['round']}").mkdir(parents=True, exist_ok=True)
            for reviewer in reviewers:
                path = jobdir / f"cycle-{job['cycle']}" / f"round-{job['round']}" / ("review-" + reviewer + ".md")
                path.write_text(render("review", job, snapshot_path=args.snapshot, discipline=reviewer, round=job["round"]))
        else:
            job["stage"] = {"analyze": "plan", "plan": "tasks", "tasks": "build"}[stage]
        event(job, "complete-" + stage, records)
    elif cmd == "snapshot":
        require(job["stage"] == "build", "Snapshots are created during build")
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
        sid = hashlib.sha256(json.dumps(snap, sort_keys=True).encode()).hexdigest()[:16]
        path = f"source-c{job['cycle']}-r{job['round'] + 1}-{sid}.json"
        require(not (jobdir / path).exists(), "Snapshot already exists")
        atomic(jobdir / path, snap)
        job["snapshot"] = record(jobdir, path)
        event(job, "snapshot", job["snapshot"])
    elif cmd == "review":
        require(job["stage"] == "review" and args.discipline in job["required_reviewers"], "Unexpected review")
        require(job["reviews"][args.discipline]["verdict"] == "pending", "Review already recorded")
        source_check(repo, jobdir, job)
        inputs = [item["path"] for item in job["artifacts"].values()]
        fields = validate_artifact(jobdir, args.artifact, job, "review", {"reviewer-" + args.discipline}, inputs, job["snapshot"]["path"])
        require(fields.get("round") == job["round"] and fields.get("discipline") == args.discipline, "Wrong review round/discipline")
        require(fields.get("verdict") in {"PASS", "CHANGES", "ERROR"}, "Invalid verdict")
        job["reviews"][args.discipline] = {"verdict": fields["verdict"], **record(jobdir, args.artifact)}
        event(job, "review", {args.discipline: job["reviews"][args.discipline]})
        verdicts = {r["verdict"] for r in job["reviews"].values()}
        if "pending" not in verdicts:
            if "ERROR" in verdicts:
                block(job, "A required review could not complete", "review-error")
            elif "CHANGES" in verdicts:
                if job["round"] >= 3:
                    block(job, "Three review rounds exhausted", "round-limit")
                else:
                    job["stage"] = "build"
                    job["snapshot"] = None
            else:
                job["stage"] = "approve"
                job["status"] = "awaiting-recommendation"
    elif cmd == "decision":
        require(job["stage"] == "approve" and job["status"] == "awaiting-recommendation", "Unexpected decision")
        source_check(repo, jobdir, job)
        inputs = [i["path"] for i in job["artifacts"].values()] + [i["path"] for i in job["reviews"].values()]
        for item in job["reviews"].values():
            require(item["verdict"] == "PASS", "Review not PASS")
            intact(jobdir, item)
        fields = validate_artifact(jobdir, args.artifact, job, "decision", {"engram-approver"}, inputs, job["snapshot"]["path"])
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
        job["approval"] = {"actor": args.actor, "instruction": instruction, "at": now(),
                           "snapshot_sha256": job["snapshot"]["sha256"]}
        job["stage"], job["status"] = "merge", "approved"
        event(job, "human-approval", job["approval"])
    elif cmd == "merge":
        require(job["stage"] == "merge" and job["approval"], "No human approval")
        snap = source_check(repo, jobdir, job, check_target=False)
        require(job["approval"]["snapshot_sha256"] == job["snapshot"]["sha256"], "Stale approval")
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
    elif cmd == "rework":
        require(job["stage"] in {"approve", "merge", "needs-human"}, "Human rework is allowed at a decision boundary")
        note = Path(args.note_file).read_text()
        require(note.strip(), "Empty rework note")
        prior = {"cycle": job["cycle"], "round": job["round"], "reviews": job["reviews"],
                 "snapshot": job["snapshot"], "approval": job["approval"], "artifacts": job["artifacts"].copy()}
        job["cycle"] += 1
        job["round"] = 0
        job["approval"], job["snapshot"], job["blocker"], job["resume_from"] = None, None, None, None
        job["reviews"], job["required_reviewers"] = {}, []
        job["stage"], job["status"] = "analyze", "running"
        # Rework conservatively revalidates all upstream contracts; preserve old bytes first.
        archive = jobdir / "history" / ("cycle-" + str(job["cycle"] - 1))
        archive.mkdir(parents=True, exist_ok=True)
        for name in ("analysis", "plan", "tasks", "verification", "release", "build", "decision"):
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
        scaffold(jobdir, job)
        event(job, "rework", {"previous": prior, "instruction": job["artifacts"]["rework"]})
    elif cmd == "refresh-base":
        require(job["stage"] == "build" and job["round"] == 0, "Refresh base before first review of a cycle")
        clean(job["worktree"])
        base = git(repo, "rev-parse", job["target"])
        git(job["worktree"], "merge-base", "--is-ancestor", base, "HEAD")
        job["base_sha"], job["snapshot"], job["approval"] = base, None, None
        event(job, "refresh-base", {"base_sha": base})
    elif cmd == "block":
        require(job["stage"] != "needs-human", "Already blocked")
        block(job, args.reason)
        event(job, "block", job["blocker"])
    elif cmd == "resume":
        require(job["stage"] == "needs-human" and job["blocker"]["code"] in {"manual", "review-error"}, "Use explicit rework for exhausted rounds or escalated decisions")
        previous = job["resume_from"]
        if job["blocker"]["code"] == "review-error":
            source_check(repo, jobdir, job)
            for reviewer, item in job["reviews"].items():
                if item["verdict"] == "ERROR":
                    job["reviews"][reviewer] = {"verdict": "pending"}
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
    create.add_argument("--feature-file", required=True)
    create.add_argument("--brief-file", help="Optional engram-frame brief; registered as a required analysis input")
    create.add_argument("--slug", required=True)
    create.add_argument("--profiles", default="full-stack", help="Comma-separated profiles")
    status = sub.add_parser("status")
    status.add_argument("--job")
    for cmd in ("complete", "snapshot", "review", "decision", "approve", "merge", "rework", "block", "resume", "refresh-base", "validate-artifact"):
        q = sub.add_parser(cmd)
        q.add_argument("--job", required=True)
        if cmd == "complete":
            q.add_argument("--stage", choices=("analyze", "plan", "tasks", "build"), required=True)
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
        if cmd == "rework":
            q.add_argument("--note-file", required=True)
        if cmd in {"block", "resume"}:
            q.add_argument("--reason", required=True)
    return p


def main(argv=None):
    args = parser().parse_args(argv)
    repo = Path(git(Path(args.repo).resolve(), "rev-parse", "--show-toplevel"))
    # Require the primary checkout, not a linked worktree with a separate runtime directory.
    common = Path(git(repo, "rev-parse", "--path-format=absolute", "--git-common-dir"))
    actual = Path(git(repo, "rev-parse", "--path-format=absolute", "--git-dir"))
    require(common == actual, "Pass --repo as the primary checkout, not the job worktree")
    state, boardpath = repo / ".engram-factory", repo / ".engram-factory" / "board.json"
    if args.command == "init":
        require(not boardpath.exists(), "Factory already initialized")
        git(repo, "check-ref-format", "--branch", args.target)
        git(repo, "rev-parse", "--verify", "refs/heads/" + args.target)
        require(not git(repo, "ls-files", ".engram-factory"), "Runtime state must not be tracked")
        exclude = common / "info" / "exclude"
        exclude.parent.mkdir(parents=True, exist_ok=True)
        existing = exclude.read_text() if exclude.exists() else ""
        if "/.engram-factory/" not in existing.splitlines():
            exclude.write_text(existing.rstrip() + "\n/.engram-factory/\n")
        state.mkdir(parents=True, exist_ok=True)
        with (state / "board.lock").open("a") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            require(not boardpath.exists(), "Factory already initialized")
            atomic(boardpath, {"schema_version": "1.0", "revision": 0, "target": args.target, "jobs": []})
        print(json.dumps({"board": str(boardpath), "target": args.target}))
        return
    require(boardpath.exists(), "Run init first")
    with (state / "board.lock").open("a") as lock:
        read_only = args.command in {"status", "validate-artifact"}
        fcntl.flock(lock, fcntl.LOCK_SH if read_only else fcntl.LOCK_EX)
        board = json.loads(boardpath.read_text())
        check_board(board)
        require(args.expected_revision is None or args.expected_revision == board["revision"], "Stale board revision")
        result = execute(args, repo, state, board)
        if not read_only:
            board["revision"] += 1
            check_board(board)
            atomic(boardpath, board)
        print(json.dumps({"revision": board["revision"], "result": result}, indent=2))


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, KeyError, TypeError) as exc:
        print(json.dumps({"error": str(exc)}), file=sys.stderr)
        sys.exit(2)
