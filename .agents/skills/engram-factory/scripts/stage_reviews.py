"""Pre-build independent review receipts for spec, plan and per-phase tasks.

This module does not authenticate actor identities, dispatch reviewers, change the
factory board, or authorize build. The controller must enforce stage sequencing,
execution isolation, owner clarification provenance and approval policy.
"""
import contextlib
import datetime as dt
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re

_ID = re.compile(r"[A-Za-z0-9][A-Za-z0-9_-]{0,79}\Z")
_SHA = re.compile(r"[0-9a-f]{64}\Z")
_STAGES = {"spec", "plan", "tasks"}
_MAX_CANDIDATES = 3  # Spec/plan: initial review plus at most two re-reviews (per cycle; rework resets the prebuild directory).


def _require(condition, message):
    if not condition:
        raise ValueError(message)


def _id(value, name):
    _require(isinstance(value, str) and bool(_ID.fullmatch(value)), "Invalid " + name)
    return value


def _text(value, name):
    _require(isinstance(value, str) and bool(value.strip()), "Missing " + name)
    return value.strip()


def _dir(jobdir, stage, phase):
    _require(stage in _STAGES, "Invalid pre-build stage")
    _require((stage == "tasks") == (phase is not None), "Phase required only for task reviews")
    name = stage if phase is None else "tasks-" + _id(phase, "phase")
    root = Path(jobdir).resolve()
    parent = root / "reviews" / "prebuild" / name
    _require(parent.resolve().is_relative_to(root), "Review directory escapes job")
    return parent


def _artifact(jobdir, relative):
    _require(isinstance(relative, str) and relative and not Path(relative).is_absolute(), "Invalid artifact path")
    _require(str(Path(relative)) == relative and ".." not in Path(relative).parts and "." not in relative.split("/"), "Noncanonical artifact path")
    root = Path(jobdir).resolve()
    path = (root / relative).resolve()
    _require(path.is_relative_to(root) and path.is_file(), "Missing or unsafe artifact " + relative)
    return {"path": relative, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}


def _load(path):
    with path.open(encoding="utf-8") as stream:
        return json.load(stream)


def _write_once(path, value):
    data = (json.dumps(value, sort_keys=True, indent=2) + "\n").encode()
    _write_bytes_once(path, data)


def _write_bytes_once(path, data):
    # Exclusive creation protects prior receipts; caller holds lock.
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "wb") as out:
        out.write(data)
        out.flush()
        os.fsync(out.fileno())
    directory = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(directory)
    finally:
        os.close(directory)


@contextlib.contextmanager
def _locked(parent):
    parent.mkdir(parents=True, exist_ok=True)
    with (parent / ".lock").open("a+") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        try:
            yield
        finally:
            fcntl.flock(lock, fcntl.LOCK_UN)


def _candidates(parent):
    return sorted(parent.glob("candidate-[0-9][0-9][0-9][0-9].json"))


def _review_path(parent, candidate, reviewer):
    return parent / (f"review-{candidate:04d}-" + _id(reviewer, "reviewer") + ".json")


def _verify_candidate(jobdir, candidate):
    for old in candidate["artifacts"]:
        _require(_artifact(jobdir, old["path"]) == old, "Candidate artifact changed: " + old["path"])


def _verify_archives(parent, candidate):
    for ref in candidate["artifacts"]:
        sha = ref["sha256"]
        _require(bool(_SHA.fullmatch(sha)), "Invalid archived digest")
        archived = parent / "artifacts" / sha
        _require(archived.is_file() and not archived.is_symlink(), "Missing archived candidate bytes")
        _require(hashlib.sha256(archived.read_bytes()).hexdigest() == sha, "Archived candidate bytes changed")


def validate_reviewers(author, reviewers):
    author = _id(author, "author invocation")
    _require(isinstance(reviewers, dict) and "red-forman" in reviewers, "Red Forman is mandatory")
    _require(len(set(reviewers.values())) == len(reviewers), "Review invocations must be distinct")
    for role, invocation in reviewers.items():
        _id(role, "reviewer role")
        _id(invocation, "reviewer invocation")
        _require(invocation != author, "Author cannot review own candidate")
    return dict(sorted(reviewers.items()))


def freeze_candidate(jobdir, stage, phase, author, artifact_paths, reviewers, expected_artifacts=None):
    """Freeze input digests; caller supplies full phase definition for task reviews.

    Reviewer mapping is role -> invocation identity. Exact invocation identity is
    an assertion supplied by caller, not proof of independent authentication.
    """
    author = _id(author, "author invocation")
    reviewers = validate_reviewers(author, reviewers)
    _require(isinstance(artifact_paths, list) and artifact_paths and len(set(artifact_paths)) == len(artifact_paths), "Artifacts must be a nonempty unique list")
    parent = _dir(jobdir, stage, phase)
    with _locked(parent):
        candidates = _candidates(parent)
        if stage in {"spec", "plan"}:
            _require(len(candidates) < _MAX_CANDIDATES,
                     "Pre-build review limit exhausted; owner decision required")
        if candidates:
            previous = _load(candidates[-1])
            status = _status(parent, jobdir, previous, check_inputs=False)
            _require(status["verdict"] in {"CHANGES", "ERROR"}, "Prior candidate must have complete CHANGES or ERROR before retry")
            _require(set(reviewers) == set(previous["reviewers"]), "Required reviewer roles cannot be dropped during recheck")
            previous_refs = previous["artifacts"]
        else:
            previous_refs = None
            status = None
        refs = [_artifact(jobdir, name) for name in sorted(artifact_paths)]
        _require(expected_artifacts is None or refs == sorted(expected_artifacts, key=lambda r: r["path"]),
                 "Review inputs changed before freeze")
        _require(previous_refs != refs or (status and status["verdict"] == "ERROR"), "Revision must change candidate or inputs")
        archive = parent / "artifacts"
        archive.mkdir(exist_ok=True)
        _require(not archive.is_symlink(), "Unsafe archive directory")
        for ref in refs:
            data = (Path(jobdir).resolve() / ref["path"]).read_bytes()
            _require(hashlib.sha256(data).hexdigest() == ref["sha256"], "Artifact changed during freeze")
            dest = archive / ref["sha256"]
            if dest.exists():
                _require(not dest.is_symlink() and dest.is_file() and dest.read_bytes() == data, "Archived candidate bytes changed")
            else:
                _write_bytes_once(dest, data)
        number = len(candidates) + 1
        _require(number <= 9999, "Too many candidates")
        candidate = {"schema_version": 1, "stage": stage, "phase": phase, "number": number,
                     "author_invocation": author, "reviewers": dict(sorted(reviewers.items())),
                     "artifacts": refs, "created_at": dt.datetime.now(dt.timezone.utc).isoformat()}
        _write_once(parent / f"candidate-{number:04d}.json", candidate)
        return candidate


def record_review(jobdir, stage, phase, candidate_number, role, invocation, verdict, findings, resolves):
    """Record one immutable reviewer receipt. Findings are IDs plus descriptions.

    A reviewer must explicitly confirm each of their previously outstanding IDs
    as resolved on the new candidate; omission remains CHANGES even if PASS.
    """
    _require(isinstance(candidate_number, int) and not isinstance(candidate_number, bool) and candidate_number > 0, "Invalid candidate number")
    role, invocation = _id(role, "reviewer role"), _id(invocation, "reviewer invocation")
    _require(verdict in {"PASS", "CHANGES", "ERROR"}, "Invalid verdict")
    _require(isinstance(findings, dict) and isinstance(resolves, list), "Invalid findings/resolutions")
    for key, description in findings.items():
        _id(key, "finding ID")
        _text(description, "finding description")
    _require(len(set(resolves)) == len(resolves), "Duplicate resolution")
    for key in resolves:
        _id(key, "resolution ID")
    _require(verdict != "CHANGES" or findings, "CHANGES requires findings")
    _require(verdict != "PASS" or not findings, "PASS cannot introduce findings")
    parent = _dir(jobdir, stage, phase)
    with _locked(parent):
        candidates = _candidates(parent)
        _require(candidates and candidates[-1].name == f"candidate-{candidate_number:04d}.json", "Not current candidate")
        candidate = _load(candidates[-1])
        _verify_candidate(jobdir, candidate)
        _require(candidate["reviewers"].get(role) == invocation, "Reviewer invocation mismatch")
        prior = set()
        for old in candidates[:-1]:
            old_candidate = _load(old)
            old_path = _review_path(parent, old_candidate["number"], role)
            if old_path.exists():
                receipt = _load(old_path)
                prior.update(receipt["findings"])
                prior.difference_update(receipt["resolves"])
        _require(set(resolves) <= prior, "Resolution must reference outstanding finding")
        _require(not (set(findings) & set(resolves)), "Finding cannot be new and resolved")
        receipt = {"schema_version": 1, "stage": stage, "phase": phase, "candidate": candidate_number,
                   "candidate_sha256": hashlib.sha256(candidates[-1].read_bytes()).hexdigest(),
                   "role": role, "invocation": invocation, "verdict": verdict, "findings": findings,
                   "resolves": resolves, "created_at": dt.datetime.now(dt.timezone.utc).isoformat()}
        _write_once(_review_path(parent, candidate_number, role), receipt)
        return receipt


def _status(parent, jobdir, candidate, check_inputs):
    if check_inputs:
        _verify_candidate(jobdir, candidate)
    for path in _candidates(parent):
        _verify_archives(parent, _load(path))
    receipts = []
    for role in candidate["reviewers"]:
        path = _review_path(parent, candidate["number"], role)
        if not path.exists():
            return {"verdict": "PENDING", "candidate": candidate["number"]}
        receipt = _load(path)
        _require(receipt["candidate_sha256"] == hashlib.sha256((parent / f"candidate-{candidate['number']:04d}.json").read_bytes()).hexdigest(), "Candidate receipt mismatch")
        receipts.append(receipt)
    if any(item["verdict"] == "ERROR" for item in receipts):
        return {"verdict": "ERROR", "candidate": candidate["number"]}
    outstanding = {}
    for path in _candidates(parent):
        current = _load(path)
        for role in candidate["reviewers"]:
            receipt_path = _review_path(parent, current["number"], role)
            if receipt_path.exists():
                receipt = _load(receipt_path)
                issues = outstanding.setdefault(role, set())
                issues.update(receipt["findings"])
                issues.difference_update(receipt["resolves"])
    remaining = {key: sorted(value) for key, value in outstanding.items() if value}
    verdict = "CHANGES" if remaining or any(x["verdict"] == "CHANGES" for x in receipts) else "PASS"
    if verdict == "CHANGES" and candidate["stage"] in {"spec", "plan"} and candidate["number"] >= _MAX_CANDIDATES:
        verdict = "NEEDS_HUMAN"
    return {"verdict": verdict, "candidate": candidate["number"], "outstanding": remaining}


def review_status(jobdir, stage, phase=None):
    parent = _dir(jobdir, stage, phase)
    with _locked(parent):
        candidates = _candidates(parent)
        _require(candidates, "No review candidate")
        return _status(parent, jobdir, _load(candidates[-1]), check_inputs=True)


def record_clarification(jobdir, stage, phase, question, answer, owner, reference):
    """Preserve a supplied exchange; it neither authenticates nor resolves findings."""
    question = _text(question, "question")
    answer = _text(answer, "answer")
    owner = _text(owner, "owner")
    reference = _text(reference, "owner-message reference")
    parent = _dir(jobdir, stage, phase)
    with _locked(parent):
        candidates = _candidates(parent)
        _require(candidates, "No review candidate")
        number = len(list(parent.glob("clarification-[0-9][0-9][0-9][0-9].json"))) + 1
        _require(number <= 9999, "Too many clarifications")
        receipt = {"schema_version": 1, "stage": stage, "phase": phase,
                   "candidate": _load(candidates[-1])["number"], "question": question,
                   "answer": answer, "owner": owner, "reference": reference,
                   "created_at": dt.datetime.now(dt.timezone.utc).isoformat()}
        _write_once(parent / f"clarification-{number:04d}.json", receipt)
        return receipt


def require_stage_pass(jobdir, stage, candidate_refs, phase=None):
    """Fail closed unless latest independent receipt set passes these exact refs.

    Read-only: caller coordinates locking and must keep board/artifact identity
    stable between this check and the board transition. A PASS is not owner
    approval. The caller must separately authenticate provenance where required.
    """
    parent = _dir(jobdir, stage, phase)
    _require(parent.is_dir(), "No review candidate")
    candidates = _candidates(parent)
    _require(candidates, "No review candidate")
    candidate = _load(candidates[-1])
    _require(isinstance(candidate_refs, list) and all(isinstance(x, dict) and set(x) == {"path", "sha256"} for x in candidate_refs), "Invalid expected artifact identities")
    _require(sorted(candidate_refs, key=lambda x: x["path"]) == candidate["artifacts"], "Reviewed inputs do not match transition inputs")
    result = _status(parent, jobdir, candidate, check_inputs=True)
    _require(result["verdict"] == "PASS", "Pre-build review is not PASS: " + result["verdict"])
    return result
