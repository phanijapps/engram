"""Versioned task definitions and status storage for the current factory layout.

This module is a storage primitive only. It does not authorize execution, grant
leases, approve phase reviews, or advance a factory job. The factory controller
enforces those workflow gates separately.
"""
import datetime as dt
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import tempfile


class TaskIndexError(ValueError):
    pass


def require(condition, message):
    if not condition:
        raise TaskIndexError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def canonical(value):
    return (json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False) + "\n").encode()


def confined(root, relative):
    require(isinstance(relative, str) and relative, "Missing relative path")
    part = Path(relative)
    require(not part.is_absolute() and relative == part.as_posix() and
            all(p not in (".", "..") for p in relative.split("/")), "Noncanonical path")
    cursor = root
    for component in part.parts:
        cursor = cursor / component
        require(not cursor.is_symlink(), "Symlink path is not permitted")
    path = cursor.resolve()
    require(path.is_relative_to(root.resolve()), "Path escapes job")
    return path


def write_atomic(path, payload):
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp = tempfile.mkstemp(prefix=".task-index-", dir=path.parent)
    try:
        with os.fdopen(fd, "wb") as out:
            out.write(payload)
            out.flush()
            os.fsync(out.fileno())
        os.replace(tmp, path)
        directory = os.open(path.parent, os.O_DIRECTORY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    finally:
        if os.path.exists(tmp):
            os.unlink(tmp)


def validate_manifest(jobdir, manifest):
    require(isinstance(manifest, dict) and manifest.get("schema_version") == 1, "Invalid manifest schema")
    require(isinstance(manifest.get("job_id"), str) and manifest["job_id"], "Missing job identity")
    require(type(manifest.get("cycle")) is int and manifest["cycle"] > 0, "Invalid cycle")
    phases = manifest.get("phases")
    require(isinstance(phases, list) and phases, "Missing phases")
    ids, phase_ids, docs, edges = set(), set(), set(), {}
    for phase in phases:
        pid = phase.get("id") if isinstance(phase, dict) else None
        require(isinstance(pid, str) and re.fullmatch(r"PHASE-[0-9]+", pid), "Invalid phase ID")
        require(pid not in phase_ids, "Duplicate phase ID")
        phase_ids.add(pid)
        require(isinstance(phase.get("tasks"), list) and phase["tasks"], "Empty phase")
        for task in phase["tasks"]:
            tid = task.get("id") if isinstance(task, dict) else None
            require(isinstance(tid, str) and re.fullmatch(r"TASK-[0-9]+", tid), "Invalid task ID")
            require(tid not in ids, "Duplicate task ID")
            ids.add(tid)
            path = task.get("document")
            require(path == "tasks/" + tid.lower() + ".md", "Noncanonical task document")
            require(path not in docs, "Duplicate document")
            docs.add(path)
            target = confined(jobdir, path)
            require(target.is_file() and not target.is_symlink(), "Missing task document")
            require(task.get("sha256") == sha(target.read_bytes()), "Task document hash mismatch")
            deps = task.get("depends_on")
            require(isinstance(deps, list) and len(deps) == len(set(deps)) and
                    all(isinstance(d, str) for d in deps), "Invalid dependencies")
            edges[tid] = deps
    visiting, visited = set(), set()
    def visit(tid):
        require(tid not in visiting, "Task dependency cycle")
        if tid in visited:
            return
        visiting.add(tid)
        for dependency in edges[tid]:
            require(dependency in ids, "Unknown task dependency")
            visit(dependency)
        visiting.remove(tid)
        visited.add(tid)
    for tid in ids:
        visit(tid)
    return edges


def initialize(jobdir, manifest):
    """Create immutable definitions and mutable index once; fail closed on retry.

    Must be invoked only after the orchestrator validates candidate structure;
    independent phase review and acceptance follow initialization. A crash between files is reported, never silently repaired.
    Revision reinitialize(jobdir, manifest) may reset a pending-only index after
    a task-phase review returned CHANGES and the task documents were revised;
    it preserves the prior manifest bytes as evidence under records/.
    """
    jobdir = Path(jobdir)
    taskdir = jobdir / "tasks"
    taskdir.mkdir(parents=True, exist_ok=True)
    with (taskdir / ".task.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        edges = validate_manifest(jobdir, manifest)
        definition = taskdir / "manifest.json"
        index = taskdir / "task.json"
        require(not definition.exists() and not index.exists(), "Task index already exists or initialization interrupted")
        definition_bytes = canonical(manifest)
        state = {"schema_version": 1, "job_id": manifest["job_id"], "cycle": manifest["cycle"],
                 "revision": 0, "manifest_sha256": sha(definition_bytes),
                 "phases": [{"id": phase["id"], "task_ids": [task["id"] for task in phase["tasks"]]}
                            for phase in manifest["phases"]],
                 "tasks": {tid: {"status": "pending", "evidence": []} for tid in edges}}
        write_atomic(definition, definition_bytes)
        write_atomic(index, canonical(state))
        return state


def reinitialize(jobdir, manifest):
    """Replace a still-pending task index after a pre-build CHANGES verdict.

    Fail closed: every task in the existing index must still be pending with no
    evidence, the job stage must not have advanced past task review, and the
    superseded manifest bytes are archived (never deleted) under records/.
    Callers must re-run per-phase review-freeze before any task transition.
    """
    jobdir = Path(jobdir)
    taskdir = jobdir / "tasks"
    taskdir.mkdir(parents=True, exist_ok=True)
    with (taskdir / ".task.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        edges = validate_manifest(jobdir, manifest)
        definition = taskdir / "manifest.json"
        index = taskdir / "task.json"
        require(definition.is_file() and index.is_file(), "No task index to replace")
        prior_state = json.loads(index.read_text())
        require(all(row.get("status") == "pending" and not row.get("evidence")
                    for row in prior_state.get("tasks", {}).values()),
                "Only a fully pending index may be reinitialized")
        prior_manifest = definition.read_bytes()
        prior = json.loads(prior_manifest)
        archive = jobdir / "records" / "task-manifest-superseded"
        archive.mkdir(parents=True, exist_ok=True)
        stamp = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
        write_atomic(archive / f"{prior['job_id']}-{stamp}-manifest.json", prior_manifest)
        write_atomic(archive / f"{prior['job_id']}-{stamp}-task.json", canonical(prior_state))
        definition_bytes = canonical(manifest)
        state = {"schema_version": 1, "job_id": manifest["job_id"], "cycle": manifest["cycle"],
                 "revision": 0, "manifest_sha256": sha(definition_bytes),
                 "phases": [{"id": phase["id"], "task_ids": [task["id"] for task in phase["tasks"]]}
                            for phase in manifest["phases"]],
                 "tasks": {tid: {"status": "pending", "evidence": []} for tid in edges}}
        write_atomic(definition, definition_bytes)
        write_atomic(index, canonical(state))
        return state


def transition(jobdir, task_id, target, expected_revision, evidence=None, actor=None):
    """Atomically change status with optimistic concurrency and evidence checks.

    The caller must enforce owner authority, phase approval and active lease.
    Evidence hashes show bytes exist, not that a claim is true or authorized.
    """
    jobdir = Path(jobdir)
    require(isinstance(actor, str) and actor.strip(), "Task actor required")
    taskdir = jobdir / "tasks"
    with (taskdir / ".task.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        manifest_bytes = (taskdir / "manifest.json").read_bytes()
        manifest = json.loads(manifest_bytes)
        edges = validate_manifest(jobdir, manifest)
        state = json.loads((taskdir / "task.json").read_text())
        require(state.get("schema_version") == 1 and state.get("job_id") == manifest["job_id"] and
                state.get("cycle") == manifest["cycle"] and state.get("manifest_sha256") == sha(manifest_bytes),
                "Task index identity or manifest mismatch")
        require(type(expected_revision) is int and state.get("revision") == expected_revision, "Stale task revision")
        require(set(state.get("tasks", {})) == set(edges), "Task index does not match manifest")
        require(state.get("phases") == [{"id": phase["id"], "task_ids": [task["id"] for task in phase["tasks"]]}
                                        for phase in manifest["phases"]], "Phase order does not match manifest")
        require(task_id in edges, "Unknown task")
        item = state["tasks"][task_id]
        allowed = {"pending": {"in_progress"}, "in_progress": {"blocked", "done"},
                   "blocked": {"in_progress"}, "done": set()}
        require(item.get("status") in allowed and target in allowed[item["status"]], "Invalid task transition")
        if target == "in_progress":
            require(all(state["tasks"][dep]["status"] == "done" for dep in edges[task_id]), "Dependencies incomplete")
            for phase in manifest["phases"]:
                if any(task["id"] == task_id for task in phase["tasks"]):
                    break
                require(all(state["tasks"][task["id"]]["status"] == "done" for task in phase["tasks"]),
                        "Previous phase incomplete")
            item["actor"] = actor
        else:
            require(item.get("actor") == actor, "Only the active task actor can transition it")
        if target == "done":
            require(isinstance(evidence, list) and evidence, "Completion requires evidence")
            for entry in evidence:
                require(isinstance(entry, dict) and set(entry) == {"path", "sha256"}, "Invalid evidence reference")
                path = confined(jobdir, entry["path"])
                require(path.is_file() and sha(path.read_bytes()) == entry["sha256"], "Evidence missing or changed")
            item["evidence"] = evidence
        item["status"] = target
        state["revision"] += 1
        write_atomic(taskdir / "task.json", canonical(state))
        return state
