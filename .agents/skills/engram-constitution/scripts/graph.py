#!/usr/bin/env python3
"""Build the constitution knowledge graph from OKF frontmatter.

Scans all .md files under docs/constitution/, parses YAML frontmatter,
and builds a directed graph from forward_links / backward_links / semantic_links.
Validates: every forward_link target resolves; backward_links are reciprocal.

Usage: graph.py <repo> [--validate] [--dot] [--json]
  --validate   check link integrity (exit 1 on broken links)
  --dot        output Graphviz DOT
  --json       output JSON graph
"""
from __future__ import annotations

import json, re, sys
from pathlib import Path

repo = Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
C = repo / "docs/constitution"

# Minimal YAML frontmatter parser (handles flat keys, inline dicts, lists of dicts)
def parse_frontmatter(text: str) -> dict:
    m = re.match(r"^---\s*\n(.*?)\n---\s*\n", text, re.S)
    if not m: return {}
    fm_text = m.group(1)
    result = {}
    for line in fm_text.splitlines():
        if line.startswith("  ") or line.startswith("- "): continue
        if ":" not in line: continue
        key, _, value = line.partition(":")
        key = key.strip()
        value = value.strip()
        if value.startswith("{"):
            # inline dict: { by: agent:x, at: date }
            inner = value.strip("{}")
            d = {}
            for pair in inner.split(","):
                if ":" in pair:
                    k, _, v = pair.partition(":")
                    d[k.strip()] = v.strip()
            result[key] = d
        elif value.startswith("[") and value.endswith("]"):
            items = [v.strip() for v in value[1:-1].split(",") if v.strip()]
            result[key] = items
        elif value:
            result[key] = value
    return result


def parse_link_list(fm_text: str, field: str) -> list[dict]:
    """Parse forward_links/backward_links/semantic_links from raw frontmatter."""
    links = []
    in_field = False
    for line in fm_text.splitlines():
        if line.strip().startswith(f"{field}:"):
            in_field = True
            continue
        if in_field:
            if line.strip().startswith("- {"):
                inner = line.strip().lstrip("- ").strip("{}")
                d = {}
                for pair in inner.split(","):
                    if ":" in pair:
                        k, _, v = pair.partition(":")
                        d[k.strip()] = v.strip()
                if d: links.append(d)
            elif line and not line.startswith(("  ", "-", "\t")):
                in_field = False
    return links


# Scan all concepts
concepts = {}
all_links = []

for md in sorted(C.rglob("*.md")):
    if md.name in ("00-INDEX.md", "INDEX.md", "DEVIATIONS.md", "DECAY-WINDOWS.md"):
        continue
    text = md.read_text(errors="replace")
    fm = parse_frontmatter(text)
    if not fm or "type" not in fm:
        continue
    m = re.match(r"^---\s*\n(.*?)\n---\s*\n", text, re.S)
    raw_fm = m.group(1) if m else ""
    cid = fm.get("id", md.stem)
    concepts[cid] = {
        "id": cid,
        "type": fm.get("type", "unknown"),
        "title": fm.get("title", md.stem),
        "description": fm.get("description", ""),
        "pack": fm.get("pack", ""),
        "status": fm.get("status", "unknown"),
        "tags": fm.get("tags", []),
        "path": str(md.relative_to(repo)),
        "forward_links": parse_link_list(raw_fm, "forward_links"),
        "backward_links": parse_link_list(raw_fm, "backward_links"),
        "semantic_links": parse_link_list(raw_fm, "semantic_links"),
    }

# Build edge list
for cid, c in concepts.items():
    for link in c["forward_links"]:
        all_links.append({**link, "from": cid, "kind": "forward"})
    for link in c["backward_links"]:
        all_links.append({**link, "from": cid, "kind": "backward"})
    for link in c["semantic_links"]:
        all_links.append({**link, "from": cid, "kind": "semantic"})

# Validate
if "--validate" in sys.argv:
    errors = []
    for link in all_links:
        target = link.get("target", "")
        if target and target not in concepts:
            errors.append(f"BROKEN LINK: {link['from']} -> {target} (relation: {link.get('relation', '?')}, kind: {link['kind']})")
    # Check backward_links reciprocity (A->B forward means B should list A in backward)
    forward_pairs = {(l["from"], l["target"]) for l in all_links if l["kind"] == "forward"}
    for l in all_links:
        if l["kind"] == "backward":
            pair = (l["target"], l["from"])
            if pair not in forward_pairs:
                errors.append(f"NON-RECIPROCAL: {l['from']} claims backward link to {l['target']} but no forward link exists")
    if errors:
        for e in errors:
            print(f"graph-validate: {e}", file=sys.stderr)
        print(f"graph-validate: {len(errors)} error(s)", file=sys.stderr)
        sys.exit(1)
    print(f"graph-validate: OK ({len(concepts)} concepts, {len(all_links)} links, all valid)")

# DOT output
if "--dot" in sys.argv:
    colors = {"rule": "#e0af68", "knowledge": "#7aa2f7", "runbook": "#9ece6a", "index": "#f7768e"}
    print("digraph constitution {")
    print('  rankdir=LR; node [shape=box, style="rounded,filled", fontname="Helvetica"];')
    for cid, c in concepts.items():
        color = colors.get(c["type"], "#dfe5ec")
        print(f'  "{cid}" [label="{c["title"]}\\n({c["type"]})", fillcolor="{color}"];')
    edge_styles = {"forward": "solid", "backward": "dashed", "semantic": "dotted"}
    for l in all_links:
        style = edge_styles.get(l["kind"], "solid")
        label = l.get("relation", "")
        print(f'  "{l["from"]}" -> "{l["target"]}" [label="{label}", style={style}];')
    print("}")
    sys.exit(0)

# JSON output
if "--json" in sys.argv:
    print(json.dumps({"concepts": list(concepts.values()), "links": all_links}, indent=2))
    sys.exit(0)

# Default: summary
by_type = {}
for c in concepts.values():
    by_type[c["type"]] = by_type.get(c["type"], 0) + 1
by_kind = {}
for l in all_links:
    by_kind[l["kind"]] = by_kind.get(l["kind"], 0) + 1
print(f"constitution graph: {len(concepts)} concepts, {len(all_links)} links")
for t, n in sorted(by_type.items()):
    print(f"  {t:12s} {n}")
for k, n in sorted(by_kind.items()):
    print(f"  {k:12s} {n} links")
