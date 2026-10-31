#!/usr/bin/env python3
"""Distribution-drift gate (RULE-2 / rail 04, library/SDK reinterpretation).

Repo-side published-surface reconciliation. Fails when:
  - the installed agent bundle (packages/launcher/skills) is out of sync with
    the authored source (.agents/skills) — directory inventory + SKILL.md
    name tags (DD-002),
  - any *.tgz build artifact is tracked in the repository tree (DD-003),
  - an npm payload entry declared in packages/launcher/package.json `files`
    does not exist on disk (declared == shipped).
Network publication checks (npm/crates.io state) are deliberately out of
scope for blocking CI (declared in the distributions registry, run scheduled).
Exit 1 on violation. Stdlib only.
"""
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
fail = []


def git_tracked_tgz() -> list[str]:
    out = subprocess.run(
        ["git", "ls-files", "*.tgz"], cwd=ROOT, capture_output=True, text=True
    ).stdout.split()
    return out


def skill_dirs(base: Path) -> set[str]:
    if not base.is_dir():
        return set()
    return {p.name for p in base.iterdir() if p.is_dir() and (p / "SKILL.md").is_file()}


def main() -> int:
    authored = skill_dirs(ROOT / ".agents/skills")
    installed = skill_dirs(ROOT / "packages/launcher/skills")

    only_authored = authored - installed
    only_installed = installed - authored
    for n in sorted(only_authored):
        fail.append(f"RULE-2/DD-002: skill authored but missing from launcher payload: {n}")
    for n in sorted(only_installed):
        fail.append(f"RULE-2/DD-002: skill in launcher payload with no authored source: {n}")

    # SKILL.md content sync (name tag only; full-text sync is the installer's job)
    for n in sorted(authored & installed):
        a = (ROOT / ".agents/skills" / n / "SKILL.md").read_text(errors="replace")
        i = (ROOT / "packages/launcher/skills" / n / "SKILL.md").read_text(errors="replace")
        if a.splitlines()[:6] != i.splitlines()[:6]:
            fail.append(f"RULE-2/DD-002: SKILL.md header drift between source and payload: {n}")

    for tgz in git_tracked_tgz():
        fail.append(f"RULE-2/DD-003: build artifact tracked in repo: {tgz}")

    pkg = ROOT / "packages/launcher/package.json"
    if pkg.is_file():
        data = json.loads(pkg.read_text())
        for entry in data.get("files", []):
            if not (ROOT / "packages/launcher" / entry.rstrip("/")).exists():
                fail.append(f"RULE-2: npm `files` entry missing on disk: {entry}")

    for f in fail:
        print(f"distribution-drift: FAIL {f}", file=sys.stderr)
    if fail:
        print(f"distribution-drift: {len(fail)} failure(s)", file=sys.stderr)
        return 1
    print("distribution-drift: OK (bundle synced, no tracked artifacts, payload declared)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
