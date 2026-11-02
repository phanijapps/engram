#!/usr/bin/env python3
"""Lint constitution prose: sentence length + passive voice.
Usage: lint.py <repo> [--strict]"""
import re, sys
from pathlib import Path
repo = Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
strict = "--strict" in sys.argv
C = repo / "docs/constitution"
PASSIVE = re.compile(r"\b(?:is|are|was|were|be|been|being)\s+\w+(?:ed|en)\b", re.I)
SENT = re.compile(r"[.!?]\s+|\n")
n_find = n_sent = 0
for f in C.rglob("*.md"):
    limit = 20 if "/rules/" in str(f) else 25
    text = re.sub(r"<!--.*?-->|`[^`]*`|\|[^\n]*\|", " ", f.read_text(errors="replace"), flags=re.S)
    for s in [x.strip() for x in SENT.split(text) if len(x.strip().split()) >= 4]:
        n_sent += 1
        if len(s.split()) > limit:
            n_find += 1; print(f"{f.relative_to(repo)}: {len(s.split())}-word sentence (> {limit}): {s[:60]}...", file=sys.stderr)
        elif PASSIVE.search(s):
            n_find += 1; print(f"{f.relative_to(repo)}: passive: {s[:60]}...", file=sys.stderr)
print(f"ste-lint: {n_sent} sentences, {n_find} finding(s)")
sys.exit(1 if n_find and strict else 0)
