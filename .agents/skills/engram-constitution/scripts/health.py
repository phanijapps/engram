#!/usr/bin/env python3
"""Constitution health check (lane 2: detect + advise).

Detects: size law crossings, orphan knowledge fragments, rules without
catalogue entries, manifest gaps. Emits findings — never drafted diffs.

Usage: health.py <repo>
Advisory: >= 2,400 tokens per knowledge file. Enforcement: > 3,000.
"""
from __future__ import annotations

import re, sys
from pathlib import Path

repo = Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
C = repo / "docs/constitution"
findings: list[str] = []

# 1. size law on knowledge files
for kn in C.rglob("knowledge.md"):
    wc = len(kn.read_text(errors="replace").split())
    est_tokens = int(wc * 1.3)  # rough word→token
    rel = kn.relative_to(repo)
    if est_tokens > 3000:
        findings.append(f"SIZE-LAW-BLOCKING: {rel} ~{est_tokens} tokens (>3,000) — split required")
    elif est_tokens >= 2400:
        findings.append(f"SIZE-LAW-ADVISORY: {rel} ~{est_tokens} tokens (>=2,400) — split recommended")

# 2. knowledge/ fragments without manifest
for kd in C.rglob("knowledge"):
    if not kd.is_dir(): continue
    idx = kd / "INDEX.md"
    if not idx.is_file():
        findings.append(f"MANIFEST-MISSING: {kd.relative_to(repo)}/ has no INDEX.md manifest")
    else:
        manifest = idx.read_text(errors="replace")
        for frag in kd.glob("*.md"):
            if frag.name == "INDEX.md": continue
            if frag.stem not in manifest:
                findings.append(f"FRAGMENT-ORPHAN: {frag.relative_to(repo)} not listed in manifest")

# 3. rules not in catalogue
cat_path = C / "registry/constitution-catalogue.json"
if cat_path.is_file():
    import json
    cat = json.loads(cat_path.read_text())
    cat_ids = {e.get("rule_id") for e in cat.get("packs", [])}
    for rf in C.rglob("rules/*.md"):
        body = rf.read_text(errors="replace")
        m = re.search(r"^#\s+([A-Z]{3,4}-\d{3})\b", body, re.M)
        if m and m.group(1) not in cat_ids:
            findings.append(f"RULE-UNCATALOGUED: {m.group(1)} at {rf.relative_to(repo)} — run derive_catalogue --write")

# 4. packs without structure
for pd in C.iterdir():
    if pd.is_dir() and pd.name not in ("registry", "modules"):
        for req in ("00-INDEX.md",):
            if not (pd / req).is_file():
                findings.append(f"PACK-INCOMPLETE: {pd.name}/ missing {req}")
        has_kn = (pd / "knowledge.md").is_file() or (pd / "knowledge").is_dir()
        has_ru = (pd / "rules").is_dir()
        if not has_kn:
            findings.append(f"PACK-INCOMPLETE: {pd.name}/ missing knowledge")
        if not has_ru:
            findings.append(f"PACK-INCOMPLETE: {pd.name}/ missing rules/")

# 5. modules: root must index them
modules_dir = C / "modules"
if modules_dir.is_dir():
    root_idx = (C / "00-INDEX.md").read_text(errors="replace") if (C / "00-INDEX.md").is_file() else ""
    for md in modules_dir.iterdir():
        if md.is_dir() and md.name not in root_idx:
            findings.append(f"MODULE-UNDECLARED: modules/{md.name} not referenced in root 00-INDEX.md")

if findings:
    print(f"constitution-health: {len(findings)} finding(s)")
    for f in findings:
        print(f"  {f}", file=sys.stderr)
    sys.exit(1)
print("constitution-health: OK — no findings")
