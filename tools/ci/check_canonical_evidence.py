#!/usr/bin/env python3
"""Canonical-evidence gate (RULE-4 / rail 06 / CE-001, CE-002). L3.

Fails when:
  - an events.jsonl record references an event family NOT declared in
    docs/constitution/registry/events/registry.json (canonical-first),
  - a record is malformed (not JSON, missing schema/family/actor/at/job_id),
  - a required per-family field (per the registry) is absent,
  - a job created after L3 adoption (job number > 010) reached approve/merge
    without emitting the corresponding evidence family.
Historical jobs 001-010 predate L3 (DW-8): validated when events.jsonl
exists, never required to have one. Exit 1 on violation. Stdlib only.
"""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
REGISTRY = ROOT / "docs/constitution/registry/events/registry.json"
JOBS = ROOT / ".engram-factory/jobs"
ADOPTION_JOB = 10  # jobs 001-010 are pre-L3 (DW-8)

fail = []


def job_number(job_id: str) -> int:
    digits = job_id.split("-", 1)[0]
    return int(digits) if digits.isdigit() else 0


def main() -> int:
    families = {f["family"]: set(f.get("fields", [])) for f in json.loads(REGISTRY.read_text())["families"]}
    base_fields = {"schema", "family", "job_id", "actor", "at"}

    advisory = 0
    for job_dir in sorted(JOBS.glob("[0-9]*-*")) if JOBS.is_dir() else []:
        if not job_dir.is_dir():
            continue
        jid = job_dir.name
        ev = job_dir / "events.jsonl"
        seen: set[str] = set()

        if ev.is_file():
            for i, line in enumerate(ev.read_text(errors="replace").splitlines(), 1):
                if not line.strip():
                    continue
                try:
                    rec = json.loads(line)
                except json.JSONDecodeError:
                    fail.append(f"{jid}/events.jsonl:{i}: not JSON")
                    continue
                missing = base_fields - rec.keys()
                if missing:
                    fail.append(f"{jid}/events.jsonl:{i}: missing base field(s) {sorted(missing)}")
                    continue
                fam = rec["family"]
                if fam not in families:
                    fail.append(f"{jid}/events.jsonl:{i}: UNDECLARED family `{fam}` (CE-002 canonical-first)")
                    continue
                need = families[fam] - {"job_id"}
                miss = {f for f in need if f not in rec}
                if miss:
                    fail.append(f"{jid}/events.jsonl:{i}: family {fam} missing field(s) {sorted(miss)}")
                if rec.get("schema") != "engram.evidence.v1":
                    fail.append(f"{jid}/events.jsonl:{i}: wrong schema {rec.get('schema')}")
                if rec.get("job_id") != jid:
                    fail.append(f"{jid}/events.jsonl:{i}: job_id mismatch ({rec.get('job_id')})")
                seen.add(fam)

        # Post-adoption privileged actions MUST have evidence
        board = job_dir / "board.json"
        stage = ""
        if board.is_file():
            try:
                stage = json.loads(board.read_text()).get("stage", "")
            except (json.JSONDecodeError, OSError):
                pass
        if job_number(jid) > ADOPTION_JOB:
            required: set[str] = set()
            if stage in {"approve", "merge", "merged"}:
                required.add("job.approved")
            if stage == "merged":
                required.add("job.released")
            for fam in sorted(required - seen):
                advisory_or_fail = (
                    f"{jid}: reached stage {stage} without `{fam}` evidence (CE-001)"
                )
                fail.append(advisory_or_fail)

    for f in fail[:25]:
        print(f"canonical-evidence: FAIL {f}", file=sys.stderr)
    if len(fail) > 25:
        print(f"canonical-evidence: ... and {len(fail)-25} more", file=sys.stderr)
    if fail:
        return 1
    print("canonical-evidence: OK (declared families, well-formed records, L3 coverage)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
