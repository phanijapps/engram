#!/usr/bin/env python3
"""Owner-view lint for factory plan.md / tasks.md artifacts.

Enforces the plain-language contract mechanically (advisory until wired):
  - the owner-view section exists (for status-ready artifacts)
  - no executor vocabulary inside it: REQ-/AC-/STEP-/GATE-/DEC-/NFR-/LED- ids,
    code backticks, file paths, mermaid
  - plan: the three labeled lines (Order:, How we know it worked:,
    If it goes wrong:) are present
  - tasks: every line is a TASK-NNN item; any RED: line has a GREEN: partner
  - brevity cap: the section stays small enough to read in ~30 seconds

Usage:
  python3 tools/ci/check_owner_view.py <artifact.md> [<artifact.md> ...]

Exit 0 = clean; exit 1 = findings (each printed with a rule id).
Legacy exemption: artifacts whose frontmatter cycle/job predates job 015 may
carry --legacy to require nothing (reports only).
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ID_RE = re.compile(r"\b(REQ|AC|STEP|GATE|DEC|NFR|LED|CHG)-\d+\b")
PATH_RE = re.compile(r"[\w.-]+/[\w./-]+|\.(?:py|ts|tsx|rs|css|md|mjs|toml|json)\b")
TICK_RE = re.compile(r"`[^`]+`")
TASK_LINE_RE = re.compile(r"^- (TASK-\d{3}) — ")
PLAN_LABELS = ("Order:", "How we know it worked:", "If it goes wrong:")


def owner_view_span(text: str) -> tuple[int, int] | None:
    lines = text.splitlines()
    start = None
    for i, line in enumerate(lines):
        if line.startswith("## What this plan does") or line.startswith("## What this job does"):
            start = i + 1
            break
    if start is None:
        return None
    end = len(lines)
    for j in range(start, len(lines)):
        if lines[j].startswith("## "):
            end = j
            break
    return start, end


def check(path: Path, legacy: bool) -> list[str]:
    findings: list[str] = []
    text = path.read_text(encoding="utf-8")
    span = owner_view_span(text)
    is_ready = re.search(r'^status: "?(ready|draft)"?', text, re.M) is not None
    if span is None:
        if not legacy and is_ready:
            findings.append(f"OV-001 {path.name}: owner-view section missing")
        return findings
    if legacy:
        return findings
    start, end = span
    section = [l for l in text.splitlines()[start:end] if l.strip()]
    body = "\n".join(section)
    name = path.name
    for i, line in enumerate(section, start=start + 1):
        for matcher, rule in ((ID_RE, "OV-002 executor id"), (TICK_RE, "OV-003 code backtick"), (PATH_RE, "OV-004 file path")):
            if matcher.search(line):
                findings.append(f"{rule} in {name}:{i}: {line.strip()[:90]}")
        if "```" in line:
            findings.append(f"OV-005 code fence in {name}:{i}")
    if len(section) > 40:
        findings.append(f"OV-006 {name}: owner view too long ({len(section)} lines; cap 40)")
    if name == "plan.md":
        for label in PLAN_LABELS:
            if label not in body:
                findings.append(f"OV-007 plan.md: labeled line missing: {label}")
    if name == "tasks.md":
        items = [l for l in section if l.startswith("- ")]
        bad = [l for l in items if not TASK_LINE_RE.match(l)]
        for l in bad:
            findings.append(f"OV-008 tasks.md: malformed task line: {l[:80]}")
        reds = sum(1 for l in section if l.strip().startswith("RED:") or " RED: " in l)
        greens = sum(1 for l in section if l.strip().startswith("GREEN:") or " GREEN: " in l)
        if reds and reds != greens:
            findings.append(f"OV-009 tasks.md: RED lines ({reds}) without matching GREEN lines ({greens})")
    return findings


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("artifacts", nargs="+", type=Path)
    ap.add_argument("--legacy", action="store_true", help="pre-015 artifacts: report only")
    args = ap.parse_args()
    all_findings: list[str] = []
    for p in args.artifacts:
        if not p.is_file():
            all_findings.append(f"OV-000 {p}: not a file")
            continue
        all_findings.extend(check(p, args.legacy))
    if all_findings:
        print("\n".join(all_findings))
        print(f"owner-view lint: {len(all_findings)} finding(s)")
        return 1
    print("owner-view lint: clean")
    return 0


if __name__ == "__main__":
    sys.exit(main())
