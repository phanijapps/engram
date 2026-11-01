#!/usr/bin/env python3
"""SDD artifact gate (PC-002 + OR-004). Ported from check-job-artifacts.sh. --strict fails on findings."""

from __future__ import annotations

import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from lib.repo import ROOT  # noqa: E402

os.chdir(ROOT)

import json
import re

strict = "--strict" in sys.argv[1:]
findings, jobs = [], 0
CANONICAL = {"brief.md", "analysis.md", "spec.md", "plan.md", "tasks.md",
             "build.md", "verification.md", "release.md", "decision.md", "board.json"}
FMI_RE = re.compile(r"^(schema_version|artifact|job_id|status|size_tier|owner|ratified_at|cycle|constitution_rules_touched|brief_ref|required_reviewers|input_hashes):", re.M)
STUB_RE = re.compile(r"\b(TBD|TODO|FIXME)\b")

def find_ids(text, prefix):
    return set(re.findall(rf"\b{prefix}-\d{{3}}\b", text))

for job_dir in sorted(Path(".engram-factory/jobs").glob("[0-9]*-*")):
    if not job_dir.is_dir(): continue
    jobs += 1
    jid = job_dir.name
    arts = {p.name for p in job_dir.glob("*.md")}
    # layout (OR-004): top-level md files beyond the canonical set = sprawl
    extra = arts - CANONICAL
    if extra:
        findings.append(f"{jid}: {len(extra)} non-canonical top-level md file(s) (OR-004, DW-3 advisory)")
    texts = {}
    for name in ("brief.md", "analysis.md", "spec.md", "plan.md", "tasks.md"):
        p = job_dir / name
        if p.is_file(): texts[name] = p.read_text()
    # frontmatter (DW-5 advisory until 2026-11-28)
    for name, text in texts.items():
        if not text.startswith("---"):
            findings.append(f"{jid}/{name}: no frontmatter (pre-adoption format, DW-5 advisory)")
        else:
            fm = text.split("---", 2)[1] if text.count("---") >= 2 else ""
            for key in ("schema_version", "artifact", "job_id", "status", "size_tier"):
                if not re.search(rf"^{key}:", fm, re.M):
                    findings.append(f"{jid}/{name}: frontmatter missing key `{key}`")
            # constitution rules validation (rails must exist; tripwire for
            # gated-path touches with empty rules claim)
            m = re.search(r"^constitution_rules_touched:\s*(\[.*?\])\s*$", fm, re.M)
            if m:
                try:
                    rules = json.loads(m.group(1))
                except json.JSONDecodeError:
                    rules = None; findings.append(f"{jid}/{name}: constitution_rules_touched not valid JSON")
                if isinstance(rules, list):
                    rail_ids = set()
                    for rail in Path("docs/constitution").glob("0*.md"):
                        for line in rail.read_text(errors="replace").splitlines():
                            for rid in re.findall(r"\b([A-Z]{2}-\d{3})\b", line):
                                rail_ids.add(rid)
                    for rid in rules:
                        if rid not in rail_ids:
                            findings.append(f"{jid}/{name}: constitution rule `{rid}` does not exist in any rail")
                    if not rules and name in ("spec.md", "analysis.md"):
                        spec_text = texts.get(name, "")
                        if re.search(r"adapters/|bindings/|contracts/|backends/|docs/contracts", spec_text):
                            findings.append(f"{jid}/{name}: touches governed paths but claims no constitution rules (tripwire)")
    # ID referential integrity (only when v2 IDs present)
    spec = texts.get("spec.md") or texts.get("analysis.md") or ""
    plan, tasks = texts.get("plan.md", ""), texts.get("tasks.md", "")
    if spec or plan or tasks:
        reqs = find_ids(spec, "REQ"); steps = find_ids(plan, "STEP"); taks = find_ids(tasks, "TASK")
        for s in find_ids(plan, "REQ"):
            if reqs and s not in reqs:
                findings.append(f"{jid}/plan.md: STEP references {s} not defined in spec")
        for t in find_ids(tasks, "STEP"):
            if steps and t not in steps:
                findings.append(f"{jid}/tasks.md: TASK references {t} not defined in plan")
        for t in find_ids(tasks, "REQ"):
            if reqs and t not in reqs:
                findings.append(f"{jid}/tasks.md: TASK references {t} not defined in spec")
        # coverage totality: every REQ appears in tasks
        if reqs and taks:
            treqs = find_ids(tasks, "REQ")
            missing = reqs - treqs
            if missing:
                findings.append(f"{jid}: REQ without covering task: {', '.join(sorted(missing))} (PC-002)")
    # stub markers in shipped artifacts (advisory for pre-adoption jobs)
    for name, text in texts.items():
        hits = STUB_RE.findall(text)
        if hits:
            findings.append(f"{jid}/{name}: {len(hits)} TBD/TODO/FIXME marker(s) (PC-002 advisory)")
    # ratification-before-build (RULE-4 ordering) — only when frontmatter exists
    brief = texts.get("brief.md", "")
    build_p = job_dir / "build.md"
    if brief.startswith("---") and build_p.is_file():
        fm = brief.split("---", 2)[1]
        if re.search(r"^status:\s*draft\s*$", fm, re.M) and build_p.stat().st_mtime > 0:
            findings.append(f"{jid}: build.md exists while brief status=draft (ratification-before-build, RULE-4 ordering)")

print(f"job-artifacts: scanned {jobs} job(s), {len(findings)} finding(s)")
for f in findings:
    print(f"  - {f}", file=sys.stderr)
if len(findings) > 40:
    pass
sys.exit(1 if (strict and findings) else 0)
