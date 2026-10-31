"""Shared helpers for engram repository operation scripts.

Every gate/ops script under tools/ resolves the repo root through this
package so scripts work from any CWD on any platform (no bash-isms).
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def git(*args: str, cwd: Path | None = None) -> str:
    """Run a git command, return stdout; empty string on failure."""
    try:
        out = subprocess.run(
            ["git", *args], cwd=cwd or ROOT, capture_output=True, text=True, check=True
        )
        return out.stdout
    except (subprocess.CalledProcessError, FileNotFoundError):
        return ""


def git_tracked(pattern: str) -> list[str]:
    return [p for p in git("ls-files", pattern).splitlines() if p.strip()]


def report(name: str, message: str) -> None:
    print(f"{name}: {message}")


def fail(name: str, failures: list[str], limit: int = 25) -> int:
    for f in failures[:limit]:
        print(f"{name}: FAIL {f}", file=sys.stderr)
    if len(failures) > limit:
        print(f"{name}: ... and {len(failures) - limit} more", file=sys.stderr)
    report(name, f"{len(failures)} failure(s)")
    return 1


def ok(name: str, message: str) -> int:
    report(name, f"OK ({message})")
    return 0
