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
pack_filter = None
type_filter = "all"
query_parts = []
for i, arg in enumerate(sys.argv):
    if arg == "--pack" and i + 1 < len(sys.argv): pack_filter = sys.argv[i + 1]; continue
    if arg == "--type" and i + 1 < len(sys.argv): type_filter = sys.argv[i + 1]; continue
    if arg.startswith("--"): continue
    if i > 0 and sys.argv[i-1] in ("--pack", "--type"): continue
    if i > 1 and not arg.startswith('-'): query_parts.append(arg)
query = " ".join(query_parts)

C = repo / "docs/constitution"
if not C.is_dir():
    print(f"query: no constitution at {C}", file=sys.stderr); sys.exit(2)

terms = [t.lower() for t in query.split() if t.strip()]
if not terms:
    print("query: provide search terms", file=sys.stderr); sys.exit(2)

results = []

def score(text: str) -> int:
    t = text.lower()
    hits = sum(1 for term in terms if term in t)
    if hits < 2 and len(terms) > 2: return 0  # need >=2 matches for multi-term queries
    if query.lower() in t: hits += 3  # exact phrase boost
    return hits

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

# Search knowledge fragments (manifest titles + fragment bodies)
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
                    m = re.search(r"\]\((.*?)\)", title)
                    frag_rel = m.group(1) if m else ""
                    # Fix 3: correct path (resolve relative to the manifest's parent)
                    frag_path = idx.parent / frag_rel.replace("knowledge/", "", 1) if frag_rel else None
                    title_clean = title.replace("[", "").replace("]", "").split("]")[0]
                    sc = score(title_clean) + score(routes)
                    # also search the fragment body if the path resolves
                    if frag_path and frag_path.is_file():
                        body = frag_path.read_text(errors="replace")
                        sc += score(body[:2000])  # search first 2000 chars
                    if sc:
                        add("knowledge", str(frag_path.relative_to(repo)) if frag_path else frag_rel,
                            title_clean, prereq, pack, sc)

# Search root INDEX (universal rules + principles)
if type_filter in ("all", "rules"):
    root_idx = C / "00-INDEX.md"
    if root_idx.is_file():
        body = root_idx.read_text(errors="replace")
        # search full body text (multi-line rules work)
        sc = score(body)
        if sc:
            # find the best matching section
            for line in body.splitlines():
                ls = score(line)
                if ls >= 2:
                    add("rule", str(root_idx.relative_to(repo)), line.strip()[:120], "", "root", ls)

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
