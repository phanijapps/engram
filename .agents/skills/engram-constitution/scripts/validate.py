#!/usr/bin/env python3
"""Validate a constitution tree (portable, stdlib only).

Checks: root INDEX exists; every pack has 00-INDEX.md + knowledge.md +
rules/*.md; catalogue rule paths exist; rule IDs unique + sentence length check. Exit 1 on structural failure; length findings are advisory
unless --strict.
"""
import json, re, sys
from pathlib import Path

repo = Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
C = repo / "docs/constitution"
strict = "--strict" in sys.argv
fails, ste = [], []
if not (C / "00-INDEX.md").is_file(): fails.append("missing root 00-INDEX.md")
cat = C / "registry/constitution-catalogue.json"
ids = set()
if cat.is_file():
    d = json.loads(cat.read_text())
    for e in d.get("packs", []):
        if not (repo / e["path"]).is_file(): fails.append(f"catalogue {e['rule_id']}: missing {e['path']}")
        if e["rule_id"] in ids: fails.append(f"duplicate rule id {e['rule_id']}")
        ids.add(e["rule_id"])
for pd in sorted(p for p in C.iterdir() if p.is_dir() and p.name not in ("registry",)):
    if not (pd / "00-INDEX.md").is_file(): fails.append(f"{pd.name}: missing 00-INDEX.md")
    if not (pd / "knowledge.md").is_file(): fails.append(f"{pd.name}: missing knowledge.md")
    rules = list((pd / "rules").glob("*.md")) if (pd / "rules").is_dir() else []
    if not rules: fails.append(f"{pd.name}: empty/missing rules/")
    for rf in rules:
        body = rf.read_text(errors="replace")
        for m in re.finditer(r"^#\s+([A-Z]{3}-\d{3})\b", body, re.M):
            if m.group(1) in ids and str(rf.relative_to(repo)) not in str(ids): pass
# sentence length check
SENT = re.compile(r"[.!?]\s+|\n")
for rf in C.rglob("*.md"):
    limit = 20 if "/rules/" in str(rf) else 25
    text = re.sub(r"<!--.*?-->|`[^`]*`|\|[^\n]*\|", " ", rf.read_text(errors="replace"), flags=re.S)
    for s in [x.strip() for x in SENT.split(text) if len(x.strip().split()) > limit]:
        ste.append(f"{rf.relative_to(repo)}: {len(s.split())}-word sentence")
for f in fails: print(f"constitution-validate: FAIL {f}", file=sys.stderr)
if ste:
    print(f"constitution-validate: {len(ste)} length advisory finding(s){' — STRICT FAIL' if strict else ''}", file=sys.stderr)
sys.exit(1 if fails or (strict and ste) else 0)
