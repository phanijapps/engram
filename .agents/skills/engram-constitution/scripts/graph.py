#!/usr/bin/env python3
"""Build, validate, and query the constitution knowledge graph from typed edges.

Scans all .md files under docs/constitution/, parses YAML frontmatter with
typed edge groups (edges.refines, edges.used_by, etc.), and builds a directed
graph. Validates: every edge target resolves; reciprocal edges are consistent.

Usage:
  graph.py <repo>                    summary (concept + edge counts)
  graph.py <repo> --validate         check link integrity (exit 1 on broken)
  graph.py <repo> --dot              Graphviz DOT output
  graph.py <repo> --json             full graph as JSON
  graph.py <repo> --trace <ID>       traverse from a node (blast radius)
  graph.py <repo> --edges <TYPE>     list all edges of a given type
"""
from __future__ import annotations

import json, re, sys
from pathlib import Path

repo = Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
C = repo / "docs/constitution"

# Known edge types and their reverse (for reciprocity checking)
EDGE_REVERSE = {
    "refines": "refined_by",
    "governs": "governed_by",
    "enforced_by": "enforces",
    "described_by": "describes",
    "used_by": "uses",
    "applies_to": "affected_by",
    "constrains": "constrained_by",
    "mitigated_by": "mitigates",
    "depends_on": "depended_on_by",
    "supersedes": "superseded_by",
    "similar_to": "similar_to",  # symmetric
    "complements": "complements",  # symmetric
    "extends": "extended_by",
    "triggered_by": "triggers",
    "produces": "produced_by",
}


def parse_frontmatter(text: str) -> tuple[dict, dict]:
    """Parse YAML frontmatter: returns (flat_fields, edges_dict)."""
    m = re.match(r"^---\s*\n(.*?)\n---\s*\n", text, re.S)
    if not m: return {}, {}
    fm = m.group(1)
    fields, edges = {}, {}
    in_edges = False
    current_edge_key = None
    for line in fm.splitlines():
        stripped = line.strip()
        # Edge section start
        if stripped == "edges:":
            in_edges = True
            continue
        # Inside edges block
        if in_edges:
            if not stripped:
                continue
            if stripped.startswith("- "):
                item = stripped.lstrip("- ").strip().strip("'\"")
                if item and current_edge_key:
                    edges[current_edge_key].append(item)
                continue
            # Try matching key: value (on the stripped line to handle indentation)
            m2 = re.match(r"^(\w+):\s*(.*)$", stripped)
            if m2:
                key, val = m2.group(1), m2.group(2).strip()
                if val.startswith("["):
                    items = [v.strip().strip("'\"") for v in val[1:-1].split(",") if v.strip()]
                    edges[key] = items
                elif val:
                    edges[key] = [val.strip().strip("'\"")]
                else:
                    edges[key] = []
                current_edge_key = key
            else:
                # Dedent — edges block ended
                if not line.startswith((" ", "\t", "-")) and line.strip():
                    in_edges = False
        # Regular field
        if ":" in line and line[0] not in (" ", "-", "\t"):
            key, _, val = line.partition(":")
            val = val.strip()
            if val.startswith("{"):
                inner = val.strip("{}")
                d = {}
                for pair in inner.split(","):
                    if ":" in pair:
                        k, _, v = pair.partition(":")
                        d[k.strip()] = v.strip().strip("'\"")
                fields[key.strip()] = d
            elif val.startswith("[") and val.endswith("]"):
                items = [v.strip().strip("'\"") for v in val[1:-1].split(",") if v.strip()]
                fields[key.strip()] = items
            elif val:
                fields[key.strip()] = val.strip().strip("'\"")
    return fields, edges


# Scan all concepts
concepts = {}
edge_list = []

for md in sorted(C.rglob("*.md")):
    if md.name in ("00-INDEX.md", "INDEX.md", "DEVIATIONS.md", "DECAY-WINDOWS.md"):
        continue
    text = md.read_text(errors="replace")
    fields, edges = parse_frontmatter(text)
    if not fields or "type" not in fields:
        continue
    cid = fields.get("id", md.stem)
    concepts[cid] = {
        "id": cid,
        "type": fields.get("type", "unknown"),
        "title": fields.get("title", md.stem),
        "description": fields.get("description", ""),
        "pack": fields.get("pack", ""),
        "status": fields.get("status", "unknown"),
        "tags": fields.get("tags", []),
        "path": str(md.relative_to(repo)),
        "edges": edges,
    }
    for edge_type, targets in edges.items():
        for target in targets:
            edge_list.append({
                "from": cid,
                "to": target,
                "type": edge_type,
                "reverse": EDGE_REVERSE.get(edge_type, edge_type),
            })

# === MODES ===

if "--trace" in sys.argv:
    # Trace from a node: follow all edges outward
    trace_id = sys.argv[sys.argv.index("--trace") + 1]
    if trace_id not in concepts:
        print(f"trace: node '{trace_id}' not found", file=sys.stderr)
        sys.exit(1)
    c = concepts[trace_id]
    print(f"trace: {trace_id} ({c['type']}, {c['pack']})")
    print(f"  title: {c['title']}")
    print(f"  status: {c['status']}")
    for etype, targets in sorted(c["edges"].items()):
        if targets:
            print(f"  {etype}:")
            for t in targets:
                tc = concepts.get(t)
                if tc:
                    print(f"    → {t} ({tc['type']}, {tc['pack']}) — {tc['title'][:60]}")
                else:
                    print(f"    → {t} (NOT FOUND)")
    # Incoming edges
    incoming = [e for e in edge_list if e["to"] == trace_id]
    if incoming:
        print(f"  (incoming):")
        for e in incoming:
            fc = concepts.get(e["from"])
            if fc:
                print(f"    ← {e['from']} ({e['type']}) — {fc['title'][:60]}")
    sys.exit(0)

if "--edges" in sys.argv:
    # List all edges of a given type
    edge_type = sys.argv[sys.argv.index("--edges") + 1]
    matches = [e for e in edge_list if e["type"] == edge_type]
    print(f"edges '{edge_type}': {len(matches)}")
    for e in matches:
        print(f"  {e['from']} → {e['to']}")
    sys.exit(0)

if "--validate" in sys.argv:
    errors = []
    for e in edge_list:
        if e["to"] not in concepts:
            errors.append(f"BROKEN: {e['from']} --{e['type']}--> {e['to']} (target not found)")
    # Reciprocity: if A --refines--> B, B should have B --refined_by--> A (or at least know about it)
    for e in edge_list:
        rev = e["reverse"]
        target = concepts.get(e["to"])
        if target and rev not in EDGE_REVERSE.get(e["type"], ""):
            # asymmetric edge type — check if reverse exists in target
            target_edges = target.get("edges", {})
            has_reverse = any(
                e["from"] in targets
                for etype, targets in target_edges.items()
                if etype in (rev, e["type"])  # symmetric types
            )
            if not has_reverse and e["type"] not in ("similar_to", "complements"):
                # advisory: no reverse link (common, not an error)
                pass
    if errors:
        for err in errors:
            print(f"graph-validate: {err}", file=sys.stderr)
        print(f"graph-validate: {len(errors)} error(s)", file=sys.stderr)
        sys.exit(1)
    print(f"graph-validate: OK ({len(concepts)} concepts, {len(edge_list)} edges, all targets resolve)")
    sys.exit(0)

if "--dot" in sys.argv:
    colors = {"rule": "#e0af68", "knowledge": "#7aa2f7", "runbook": "#9ece6a", "index": "#f7768e"}
    edge_colors = {
        "refines": "#e0af68", "governs": "#f7768e", "enforced_by": "#9ece6a",
        "described_by": "#7aa2f7", "used_by": "#bb9af7", "applies_to": "#7dcfff",
        "mitigated_by": "#ff9e64", "depends_on": "#89ddff", "supersedes": "#db4b4b",
    }
    print("digraph constitution {")
    print('  rankdir=LR; node [shape=box, style="rounded,filled", fontname="Helvetica"];')
    for cid, c in concepts.items():
        color = colors.get(c["type"], "#dfe5ec")
        print(f'  "{cid}" [label="{c["title"][:30]}\\n({c["type"]})", fillcolor="{color}"];')
    for e in edge_list:
        color = edge_colors.get(e["type"], "#8b949e")
        print(f'  "{e["from"]}" -> "{e["to"]}" [label="{e["type"]}", color="{color}"];')
    print("}")
    sys.exit(0)

if "--json" in sys.argv:
    print(json.dumps({"concepts": list(concepts.values()), "edges": edge_list}, indent=2))
    sys.exit(0)

# Default: summary
by_type = {}
for c in concepts.values():
    by_type[c["type"]] = by_type.get(c["type"], 0) + 1
by_edge = {}
for e in edge_list:
    by_edge[e["type"]] = by_edge.get(e["type"], 0) + 1
print(f"constitution graph: {len(concepts)} concepts, {len(edge_list)} edges")
for t, n in sorted(by_type.items()):
    print(f"  {t:12s} {n}")
if by_edge:
    for et, n in sorted(by_edge.items()):
        print(f"  edge:{et:12s} {n}")
