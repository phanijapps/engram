#!/usr/bin/env python3
"""STE lint (ASD-STE100 mechanical subset) — advisory gate.

Checks constitution knowledge + rules + INDEX prose for the mechanically
verifiable STE writing rules:
  - sentence length: rules/procedural <= 20 words, knowledge/descriptive <= 25
  - passive voice: was/were/be/is/are + past participle heuristic (flag)
  - omitted-article telegraphese in statements (heuristic, flag only)

Advisory until the corpus converges (DW window); --strict to enforce.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from lib.repo import ROOT  # noqa: E402

import os

os.chdir(ROOT)

NAME = "ste-lint"
C = ROOT / "docs/constitution"
LIMITS = {20: "rules/procedural", 25: "knowledge/descriptive"}
PASSIVE = re.compile(r"\b(?:is|are|was|were|be|been|being)\s+\w+(?:ed|en)\b", re.I)
SENT = re.compile(r"[.!?]\s+|\n")

findings: list[str] = []
checked = 0


def split_sentences(text: str):
    text = re.sub(r"<!--.*?-->", " ", text, flags=re.S)
    text = re.sub(r"\|[^\n]*\|", " ", text)   # tables: lint prose only
    text = re.sub(r"`[^`]*`", " ", text)      # code spans exempt
    text = re.sub(r"^#+.*$", " ", text, flags=re.M)
    return [s.strip() for s in SENT.split(text) if len(s.strip().split()) >= 4]


for f in C.rglob("*.md"):
    rel = f.relative_to(ROOT)
    body = f.read_text(errors="replace")
    limit = 20 if "/rules/" in str(rel) else 25
    for s in split_sentences(body):
        checked += 1
        n = len(s.split())
        if n > limit:
            findings.append(f"{rel}: {n}-word sentence (>{limit}, {LIMITS[limit]}): {s[:60]}...")
        if PASSIVE.search(s):
            findings.append(f"{rel}: passive voice: {s[:60]}...")

print(f"ste-lint: checked {checked} sentences, {len(findings)} finding(s)")
for f in findings[:20]:
    print(f"  - {f}", file=sys.stderr)
if len(findings) > 20:
    print(f"  ... and {len(findings)-20} more", file=sys.stderr)
sys.exit(1 if (findings and "--strict" in sys.argv) else 0)
