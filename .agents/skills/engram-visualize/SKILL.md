---
name: engram-visualize
description: Visualize the constitution as an interactive D3.js force-directed knowledge graph — nodes colored by type (rule, knowledge, runbook), edges colored by relationship (refines, enforced_by, described_by). Click to traverse, search to filter, zoom to explore. Trigger on "visualize the constitution", "show the knowledge graph", "graph the rules", or after building/updating a constitution when you want to see its structure.
---

# engram-visualize

Render the constitution's knowledge graph as a standalone interactive HTML file
using D3.js force simulation. One command, opens in any browser.

## Usage

```bash
python3 scripts/visualize.py <repo> [--open] [--output <file>]
```

- `<repo>` — the repository containing `docs/constitution/`
- `--open` — open the output in your default browser
- `--output` — custom output path (default: `docs/constitution/graph.html`)

## What it produces

A single self-contained HTML file with:

| Feature | Description |
|---|---|
| **Force simulation** | D3.js physics — nodes repel, edges attract, layout settles naturally |
| **Typed nodes** | Gold=rules, blue=knowledge, green=runbooks, red=index; size ∝ connection count |
| **Typed edges** | Color-coded by relationship (refines, enforced_by, described_by, used_by, etc.) |
| **Click traversal** | Click a node → its connections highlight, everything else dims; detail panel shows all incoming/outgoing edges; click an edge to jump to that node |
| **Search** | Filter by ID, title, pack, or type |
| **Zoom + pan + drag** | Mouse wheel zooms, drag pans, drag nodes to rearrange |
| **Legend** | Node types and edge types with live counts |

## Dependencies

- Reads the graph from the `engram-constitution` skill's `graph.py --json` output
- Requires internet for the D3.js CDN (`cdn.jsdelivr.net`)
- No server needed — the output is a static HTML file

## Relationship with engram-constitution

This skill is a **consumer** of the constitution's knowledge graph. It does not
create or modify constitution artifacts. After any constitution change (new rules,
new knowledge fragments, new runbooks), re-run visualize to see the updated graph.
