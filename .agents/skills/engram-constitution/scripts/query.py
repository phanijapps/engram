#!/usr/bin/env python3
"""Query the constitution: rules, knowledge fragments, runbooks.

Usage: query.py <repo> "<search terms>" [--pack <name>] [--type rules|knowledge|runbooks|all]

Returns matching items sized for progressive loading: the fragment/rule/runbook
path, a one-line summary, prerequisites (for knowledge), and the pack it lives in.
"""
from __future__ import annotations

import json, re, sys
from pathlib import Path

repo = Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
query = " ".join(sys.argv[2:]) if len(sys.argv) > 2 else ""
pack_filter = None
type_filter = "all"
for i, arg in enumerate(sys.argv):
    if arg == "--pack" and i + 1 < len(sys.argv): pack_filter = sys.argv[i + 1]
    if arg == "--type": type_filter = sys.argv[i + 1] if i + 1 < len(sys.argv) else "all"

C = repo / "docs/constitution"
if not C.is_dir():
    print(f"query: no constitution at {C}", file=sys.stderr); sys.exit(2)

terms = [t.lower() for t in query.split() if t.strip()]
if not terms:
    print("query: provide search terms", file=sys.stderr); sys.exit(2)

results = []

def score(text: str) -> int:
    t = text.lower()
    return sum(1 for term in terms if term in t)

def add(kind: str, path: str, summary: str, prereq: str, pack: str, sc: int):
    if sc > 0:
        results.append((sc, kind, path, summary[:120], prereq, pack))

# Search rules
if type_filter in ("all", "rules"):
    cat_path = C / "registry/constitution-catalogue.json"
    if cat_path.is_file():
        cat = json.loads(cat_path.read_text())
        for e in cat.get("packs", []):
            if pack_filter and e.get("pack") != pack_filter: continue
            sc = score(e.get("statement", "")) + score(e.get("rule_id", ""))
            add("rule", e["path"], f"{e['rule_id']}: {e.get('statement', '')[:100]}", "", e.get("pack", ""), sc)

# Search knowledge fragments
if type_filter in ("all", "knowledge"):
    for idx in C.rglob("knowledge/INDEX.md"):
        pack = idx.relative_to(C).parts[0]
        if pack_filter and pack != pack_filter: continue
        manifest = idx.read_text(errors="replace")
        for line in manifest.splitlines():
            if line.startswith("| ["):
                cells = [c.strip() for c in line.strip("|").split("|")]
                if len(cells) >= 4:
                    title, size, prereq, routes = cells[0], cells[1], cells[2], cells[3]
                    # extract the file path from the markdown link
                    m = re.search(r"\]\((.*?)\)", title)
                    fpath = m.group(1) if m else ""
                    sc = score(title) + score(routes)
                    if sc:
                        full = str(idx.parent.parent / fpath.replace("knowledge/", "", 1)) if fpath else ""
                        add("knowledge", full, title.replace("[", "").replace("]", "").split("]")[0], prereq, pack, sc)

# Search runbooks
if type_filter in ("all", "runbooks"):
    for rb in C.rglob("runbooks/*.md"):
        pack = rb.relative_to(C).parts[0]
        if pack_filter and pack != pack_filter: continue
        body = rb.read_text(errors="replace")
        title = body.splitlines()[0].replace("# ", "") if body else ""
        trigger = ""
        for line in body.splitlines():
            if line.startswith("## Trigger"):
                idx = body.splitlines().index(line)
                trigger = body.splitlines()[idx + 1] if idx + 1 < len(body.splitlines()) else ""
                break
        sc = score(title) + score(trigger) + score(body[:500])
        add("runbook", str(rb.relative_to(repo)), f"{title} — {trigger[:80]}", "", pack, sc)

# Sort by score descending
results.sort(key=lambda x: -x[0])

if not results:
    print(f"query: no matches for '{query}'")
    sys.exit(0)

print(f"query: '{query}' — {len(results)} result(s)")
for sc, kind, path, summary, prereq, pack in results[:15]:
    pr = f" (prereq: {prereq})" if prereq and prereq != "none" else ""
    print(f"  [{kind:9s}] {pack:14s} {summary}{pr}")
    print(f"             → {path}")
if len(results) > 15:
    print(f"  ... and {len(results) - 15} more")
