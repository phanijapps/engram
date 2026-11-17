#!/usr/bin/env python3
"""Validate revision-bound PLT-001 provenance; emit only on explicit application."""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

RULE = "docs/constitution/platform/rules/PLT-001.md"
REGISTRY = "docs/constitution/registry/events/registry.json"


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def safe_path(root: Path, relative: str) -> Path:
    if Path(relative).is_absolute() or ".." in Path(relative).parts:
        raise ValueError("evidence path must be confined and relative")
    root = root.absolute()
    if root.is_symlink() or root.resolve() != root:
        raise ValueError("evidence root contains a symlink")
    path = root / relative
    for candidate in (path, *path.parents):
        if candidate.is_symlink():
            raise ValueError("symlinked evidence path is forbidden")
        if candidate == root:
            break
    return path


def git_blob(repo: Path, revision: str, path: str) -> bytes:
    if not re.fullmatch(r"[0-9a-f]{40}", revision):
        raise ValueError("source revision must be a full commit identity")
    return subprocess.check_output(["git", "show", f"{revision}:{path}"], cwd=repo)


def proposed_boundary(proposal: str) -> str:
    section = proposal.split("## Exact proposed replacement normative text\n", 1)[1]
    section = section.split("## Observable boundary", 1)[0]
    normative, scopes = section.split("## Scope definitions and limits\n", 1)
    sentences = [line[2:] for line in normative.splitlines() if line.startswith("> ")]
    if len(sentences) != 5:
        raise ValueError("complete five-sentence proposed boundary is required")
    return "\n".join(sentences + [scopes.strip()])


def prepare_event(repo: Path, job_root: Path, *, base_revision: str,
                  applied_revision: str, actor: str) -> dict:
    if job_root.parent.name != "jobs" or job_root.parent.parent.name != ".engram-factory":
        raise ValueError("job evidence must use the canonical .engram-factory/jobs layout")
    if not actor.strip():
        raise ValueError("actual applying actor is required")
    registry = json.loads(safe_path(repo, REGISTRY).read_text())
    family = next((f for f in registry["families"] if f["family"] == "constitution.amended"), None)
    if family is None:
        raise ValueError("constitution.amended is not registered")
    proposal_path = safe_path(job_root, "runs/constitution/PLT-001-amendment.md")
    ratification_path = safe_path(job_root, "runs/constitution/PLT-001-ratification.md")
    review_path = safe_path(job_root, "runs/constitution/PLT-001-amendment-review.md")
    review = review_path.read_bytes()
    proposal = proposal_path.read_bytes()
    ratification = ratification_path.read_bytes()
    base = git_blob(repo, base_revision, RULE)
    applied = git_blob(repo, applied_revision, RULE)
    current = safe_path(repo, RULE).read_bytes()
    if applied == base or applied != current:
        raise ValueError("amendment is unapplied or current rule differs from applying revision")
    if subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip() != applied_revision:
        raise ValueError("applying revision must be the actual current source commit")
    receipt = ratification.decode()
    if not receipt.startswith("# Owner ratification:") or any(value not in receipt for value in (digest(proposal), digest(base), base_revision)):
        raise ValueError("ratification does not bind the exact proposal and canonical base")
    if not review.decode().startswith("# Independent review:") or any(value not in review.decode() for value in (digest(proposal), digest(base), base_revision)):
        raise ValueError("independent draft review does not bind the exact proposal and canonical base")
    boundary = proposed_boundary(proposal.decode())
    # Every proposed sentence and scope definition is retained, despite Markdown wrapping.
    normalize = lambda value: " ".join(value.replace("> ", "").split())
    canonical = normalize(applied.decode())
    for paragraph in boundary.split("\n"):
        if paragraph.strip() and normalize(paragraph) not in canonical:
            raise ValueError("applied rule omits the ratified normative/scope boundary")
    event = {
        "schema": "engram.evidence.v1", "family": "constitution.amended",
        "actor": actor, "at": datetime.now(timezone.utc).isoformat(),
        "job_id": job_root.name, "rail": "platform", "rule_ids": ["PLT-001"],
        "supersession": {"base_revision": base_revision, "applied_revision": applied_revision},
        "source_revision": applied_revision,
        "artifact_refs": {"proposal": str(proposal_path.relative_to(job_root)),
                          "ratification": str(ratification_path.relative_to(job_root)),
                          "draft_review": str(review_path.relative_to(job_root)), "applied_rule": RULE},
        "artifact_hashes": {"proposal": digest(proposal), "ratification": digest(ratification),
                            "draft_review": digest(review),
                            "base_rule": digest(base), "applied_rule": digest(applied)},
    }
    if set(family.get("fields", [])) - event.keys():
        raise ValueError("event is missing registered family fields")
    return event


def emit_event(job_root: Path, event: dict) -> None:
    # Fixed canonical destination; caller cannot redirect privileged evidence elsewhere.
    path = safe_path(job_root, "events.jsonl")
    descriptor = os.open(path, os.O_WRONLY | os.O_APPEND | os.O_CREAT | os.O_NOFOLLOW, 0o600)
    with os.fdopen(descriptor, "a") as stream:
        stream.write(json.dumps(event, sort_keys=True) + "\n")
        stream.flush()
        os.fsync(stream.fileno())


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--job-root", type=Path, required=True)
    parser.add_argument("--base-revision", required=True)
    parser.add_argument("--applied-revision", required=True)
    parser.add_argument("--actor", required=True)
    parser.add_argument("--emit", action="store_true")
    args = parser.parse_args()
    event = prepare_event(args.repo, args.job_root, base_revision=args.base_revision,
                          applied_revision=args.applied_revision, actor=args.actor)
    if args.emit:
        emit_event(args.job_root, event)
    else:
        print(json.dumps({"status": "validated-preview", "event": event}, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
