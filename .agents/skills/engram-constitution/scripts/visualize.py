#!/usr/bin/env python3
"""Visualize the constitution as a D3.js force-directed graph.

Reads the knowledge graph from graph.py's JSON output and generates a
standalone HTML file with an interactive force simulation.

Usage: visualize.py <repo> [--output <file>] [--open]
"""
from __future__ import annotations

import json, re, subprocess, sys, webbrowser
from pathlib import Path

repo = Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
output = Path(sys.argv[sys.argv.index("--output") + 1]) if "--output" in sys.argv else repo / "docs/constitution/graph.html"
open_browser = "--open" in sys.argv

# Get graph data
graph_script = Path(__file__).parent / "graph.py"
r = subprocess.run(
    [sys.executable, str(graph_script), str(repo), "--json"],
    capture_output=True, text=True)
if r.returncode != 0:
    print(f"visualize: graph.py failed: {r.stderr}", file=sys.stderr)
    sys.exit(1)
data = json.loads(r.stdout)
concepts, edges = data["concepts"], data["links"] if "links" in data else data.get("edges", [])

# Deduplicate edges and filter out sources (they're not concept-to-concept)
seen = set()
clean_edges = []
for e in edges:
    if e["type"] == "sources":
        continue
    key = (e["from"], e["to"], e["type"])
    if key not in seen:
        seen.add(key)
        clean_edges.append(e)

# Build D3 data
nodes = [{"id": c["id"], "title": c.get("title", c["id"]), "type": c["type"],
          "pack": c.get("pack", ""), "status": c.get("status", ""),
          "description": c.get("description", ""), "tags": c.get("tags", []),
          "degree": sum(1 for e in clean_edges if e["from"] == c["id"] or e["to"] == c["id"])}
         for c in concepts]
links = [{"source": e["from"], "target": e["to"], "type": e["type"]} for e in clean_edges]

# Count types for legend
type_counts = {}
for n in nodes:
    type_counts[n["type"]] = type_counts.get(n["type"], 0) + 1
edge_type_counts = {}
for l in links:
    edge_type_counts[l["type"]] = edge_type_counts.get(l["type"], 0) + 1

HTML = """<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Constitution Graph</title>
<style>
* { margin: 0; padding: 0; box-sizing: border-box; }
body { background: #0d1117; color: #c9d1d9; font-family: 'Segoe UI', system-ui, sans-serif; overflow: hidden; }
#header { position: fixed; top: 0; left: 0; right: 0; z-index: 10; background: #161b22; border-bottom: 1px solid #30363d; padding: 10px 20px; display: flex; align-items: center; gap: 16px; }
#header h1 { font-size: 16px; color: #58a6ff; margin: 0; }
#header .stats { font-size: 12px; color: #8b949e; }
#search { padding: 6px 12px; background: #0d1117; border: 1px solid #30363d; border-radius: 6px; color: #c9d1d9; font-size: 13px; width: 200px; outline: none; }
#search:focus { border-color: #58a6ff; }
#filters { display: flex; gap: 6px; flex-wrap: wrap; }
.filter-btn { padding: 3px 10px; border-radius: 12px; font-size: 11px; border: 1px solid transparent; cursor: pointer; transition: all .15s; font-family: inherit; }
.filter-btn.active { border-color: currentColor; }
#legend { position: fixed; bottom: 16px; left: 16px; z-index: 10; background: #161b22ee; border: 1px solid #30363d; border-radius: 8px; padding: 12px 16px; font-size: 12px; max-height: 300px; overflow-y: auto; }
#legend h3 { font-size: 11px; text-transform: uppercase; letter-spacing: .08em; color: #8b949e; margin: 8px 0 6px; }
#legend .legend-item { display: flex; align-items: center; gap: 8px; margin: 3px 0; cursor: pointer; }
#legend .legend-item:hover { color: #fff; }
#legend .dot { width: 10px; height: 10px; border-radius: 50%; flex-shrink: 0; }
#legend .line { width: 20px; height: 2px; flex-shrink: 0; }
#detail { position: fixed; top: 56px; right: 16px; width: 320px; max-height: calc(100vh - 80px); overflow-y: auto; z-index: 10; background: #161b22ee; border: 1px solid #30363d; border-radius: 8px; padding: 16px; font-size: 13px; display: none; backdrop-filter: blur(8px); }
#detail h2 { font-size: 15px; color: #58a6ff; margin-bottom: 8px; }
#detail .meta { color: #8b949e; font-size: 12px; margin-bottom: 8px; }
#detail .desc { margin-bottom: 12px; line-height: 1.5; }
#detail .edges-section { margin-top: 12px; }
#detail .edges-section h4 { font-size: 11px; text-transform: uppercase; color: #8b949e; margin: 8px 0 4px; }
#detail .edge-link { color: #7aa2f7; cursor: pointer; margin: 2px 0; font-size: 12px; }
#detail .edge-link:hover { text-decoration: underline; color: #e0af68; }
#detail .close { position: absolute; top: 8px; right: 12px; cursor: pointer; color: #8b949e; font-size: 18px; }
#detail .close:hover { color: #f85149; }
svg { width: 100vw; height: 100vh; }
.node { cursor: pointer; transition: opacity .2s; }
.node circle { stroke-width: 2; transition: r .2s; }
.node:hover circle { r: function(d) { return Math.max(d.radius * 1.3, 8); } }
.node text { font-size: 10px; fill: #8b949e; pointer-events: none; text-anchor: middle; }
.node.selected circle { stroke: #e0af68; stroke-width: 3; }
.link { stroke-opacity: .3; transition: stroke-opacity .2s; }
.link.highlighted { stroke-opacity: .9; }
.link-label { font-size: 8px; fill: #484f58; pointer-events: none; }
.dimmed { opacity: 0.1; }
.tooltip { position: fixed; pointer-events: none; background: #161b22; border: 1px solid #30363d; border-radius: 6px; padding: 6px 10px; font-size: 11px; color: #c9d1d9; z-index: 20; display: none; white-space: nowrap; }
</style>
</head>
<body>
<div id="header">
  <h1>⚡ Constitution Graph</h1>
  <span class="stats">__NODE_COUNT__ concepts · __EDGE_COUNT__ edges</span>
  <input type="text" id="search" placeholder="Search concepts..." autocomplete="off">
  <div id="filters"></div>
</div>
<svg id="graph"></svg>
<div id="legend"></div>
<div id="detail"><span class="close" onclick="document.getElementById('detail').style.display='none'">×</span></div>
<div class="tooltip" id="tooltip"></div>

<script src="https://cdn.jsdelivr.net/npm/d3@7/dist/d3.min.js"></script>
<script>
const NODES = __NODES_JSON__;
const LINKS = __LINKS_JSON__;
const TYPE_COLORS = __TYPE_COLORS__;
const EDGE_COLORS = __EDGE_COLORS__;

// Compute node radius based on degree
const maxDeg = d3.max(NODES, d => d.degree) || 1;
NODES.forEach(n => { n.radius = 6 + (n.degree / maxDeg) * 14; });

const svg = d3.select("#graph");
const width = window.innerWidth, height = window.innerHeight;

// Zoom
const g = svg.append("g");
svg.call(d3.zoom().scaleExtent([0.2, 5]).on("zoom", e => g.attr("transform", e.transform)));

// Arrow markers for edges
const defs = svg.append("defs");
const edgeTypes = [...new Set(LINKS.map(l => l.type))];
edgeTypes.forEach(t => {
  defs.append("marker")
    .attr("id", `arrow-${t}`).attr("viewBox", "0 -5 10 10")
    .attr("refX", 20).attr("refY", 0).attr("markerWidth", 6).attr("markerHeight", 6)
    .attr("orient", "auto")
    .append("path").attr("d", "M0,-5L10,0L0,5")
    .attr("fill", EDGE_COLORS[t] || "#484f58");
});

// Simulation
const sim = d3.forceSimulation(NODES)
  .force("link", d3.forceLink(LINKS).id(d => d.id).distance(d => 80 + Math.random() * 40).strength(0.3))
  .force("charge", d3.forceManyBody().strength(d => -300 - d.degree * 20))
  .force("center", d3.forceCenter(width / 2, height / 2))
  .force("collide", d3.forceCollide().radius(d => d.radius + 4))
  .alphaDecay(0.02);

// Draw links
const link = g.selectAll(".link").data(LINKS).enter().append("line")
  .attr("class", "link")
  .attr("stroke", d => EDGE_COLORS[d.type] || "#484f58")
  .attr("stroke-width", d => d.type === "refines" ? 2 : 1)
  .attr("stroke-dasharray", d => d.type === "similar_to" ? "4,4" : d.type === "mitigated_by" ? "8,4" : null)
  .attr("marker-end", d => `url(#arrow-${d.type})`);

// Draw link labels (only for highlighted)
const linkLabel = g.selectAll(".link-label").data(LINKS).enter().append("text")
  .attr("class", "link-label")
  .text(d => d.type)
  .style("opacity", 0);

// Draw nodes
const node = g.selectAll(".node").data(NODES).enter().append("g")
  .attr("class", "node")
  .call(d3.drag()
    .on("start", (e, d) => { if (!e.active) sim.alphaTarget(0.3).restart(); d.fx = d.x; d.fy = d.y; })
    .on("drag", (e, d) => { d.fx = e.x; d.fy = e.y; })
    .on("end", (e, d) => { if (!e.active) sim.alphaTarget(0); d.fx = null; d.fy = null; }));

node.append("circle")
  .attr("r", d => d.radius)
  .attr("fill", d => TYPE_COLORS[d.type] || "#8b949e")
  .attr("stroke", "#0d1117")
  .attr("stroke-width", 2);

node.append("text")
  .attr("dy", d => -(d.radius + 4))
  .text(d => d.id);

// Tooltip
const tooltip = d3.select("#tooltip");
node.on("mousemove", (e, d) => {
  tooltip.style("display", "block")
    .style("left", (e.clientX + 12) + "px")
    .style("top", (e.clientY - 10) + "px")
    .html(`<strong>${d.id}</strong> (${d.type}, ${d.pack})<br>${d.title}`);
}).on("mouseleave", () => tooltip.style("display", "none"));

// Click → detail panel
const detail = d3.select("#detail");
node.on("click", (e, d) => {
  d3.selectAll(".node").classed("selected", false);
  d3.select(e.currentTarget).classed("selected", true);

  // Highlight connected edges
  const connected = new Set(LINKS.filter(l =>
    l.source.id === d.id || l.target.id === d.id
  ).map(l => `${l.source.id}|${l.target.id}`));

  link.classed("highlighted", l => connected.has(`${l.source.id}|${l.target.id}`));
  link.style("stroke-opacity", l => connected.has(`${l.source.id}|${l.target.id}`) ? 0.9 : 0.05);
  linkLabel.style("opacity", l => connected.has(`${l.source.id}|${l.target.id}`) ? 1 : 0);
  node.classed("dimmed", n => {
    if (n.id === d.id) return false;
    return !LINKS.some(l => (l.source.id === d.id && l.target.id === n.id) || (l.target.id === d.id && l.source.id === n.id));
  });

  // Build detail HTML
  let html = `<h2>${d.id}</h2>`;
  html += `<div class="meta">${d.type} · ${d.pack} · ${d.status} · ${d.degree} connections</div>`;
  if (d.title) html += `<div class="desc"><strong>${d.title}</strong></div>`;
  if (d.description) html += `<div class="desc">${d.description}</div>`;

  const outEdges = LINKS.filter(l => l.source.id === d.id);
  const inEdges = LINKS.filter(l => l.target.id === d.id);
  if (outEdges.length) {
    html += `<div class="edges-section"><h4>Outgoing (${outEdges.length})</h4>`;
    outEdges.forEach(l => {
      html += `<div class="edge-link" onclick="selectNode('${l.target.id}')">→ ${l.target.id} <span style="color:#484f58">(${l.type})</span></div>`;
    });
    html += `</div>`;
  }
  if (inEdges.length) {
    html += `<div class="edges-section"><h4>Incoming (${inEdges.length})</h4>`;
    inEdges.forEach(l => {
      html += `<div class="edge-link" onclick="selectNode('${l.source.id}')">← ${l.source.id} <span style="color:#484f58">(${l.type})</span></div>`;
    });
    html += `</div>`;
  }
  detail.html(html);
  detail.style("display", "block");
  e.stopPropagation();
});

svg.on("click", () => {
  detail.style("display", "none");
  link.style("stroke-opacity", .3);
  linkLabel.style("opacity", 0);
  node.classed("dimmed", false).classed("selected", false);
});

// Search
d3.select("#search").on("input", function() {
  const q = this.value.toLowerCase();
  if (!q) {
    node.classed("dimmed", false);
    return;
  }
  node.classed("dimmed", n =>
    !n.id.toLowerCase().includes(q) &&
    !n.title.toLowerCase().includes(q) &&
    !n.pack.toLowerCase().includes(q) &&
    !n.type.toLowerCase().includes(q)
  );
});

// Simulation tick
sim.on("tick", () => {
  link.attr("x1", d => d.source.x).attr("y1", d => d.source.y)
      .attr("x2", d => d.target.x).attr("y2", d => d.target.y);
  linkLabel.attr("x", d => (d.source.x + d.target.x) / 2)
      .attr("y", d => (d.source.y + d.target.y) / 2);
  node.attr("transform", d => `translate(${d.x},${d.y})`);
});

// Legend
const legend = d3.select("#legend");
let legendHtml = `<h3>Nodes</h3>`;
Object.entries(TYPE_COLORS).forEach(([type, color]) => {
  const count = NODES.filter(n => n.type === type).length;
  if (count > 0) legendHtml += `<div class="legend-item"><span class="dot" style="background:${color}"></span> ${type} (${count})</div>`;
});
const usedEdgeTypes = [...new Set(LINKS.map(l => l.type))];
if (usedEdgeTypes.length) {
  legendHtml += `<h3>Edges</h3>`;
  usedEdgeTypes.forEach(t => {
    legendHtml += `<div class="legend-item"><span class="line" style="background:${EDGE_COLORS[t] || "#484f58"}"></span> ${t} (${LINKS.filter(l => l.type === t).length})</div>`;
  });
}
legend.html(legendHtml);

// Select node by ID (for detail panel edge clicks)
window.selectNode = function(id) {
  const n = NODES.find(n => n.id === id);
  if (n) {
    const fakeEvent = { currentTarget: node.filter(d => d.id === id).node() };
    node.dispatch("click", { detail: n });
  }
};
</script>
</body>
</html>"""

# Fill in data
html = (HTML
    .replace("__NODE_COUNT__", str(len(nodes)))
    .replace("__EDGE_COUNT__", str(len(links)))
    .replace("__NODES_JSON__", json.dumps(nodes))
    .replace("__LINKS_JSON__", json.dumps(links))
    .replace("__TYPE_COLORS__", json.dumps({
        "rule": "#e0af68", "knowledge": "#7aa2f7", "runbook": "#9ece6a",
        "index": "#f7768e", "unknown": "#8b949e"}))
    .replace("__EDGE_COLORS__", json.dumps({
        "refines": "#e0af68", "governs": "#f7768e", "enforced_by": "#9ece6a",
        "described_by": "#7aa2f7", "used_by": "#bb9af7", "applies_to": "#7dcfff",
        "mitigated_by": "#ff9e64", "depends_on": "#89ddff", "supersedes": "#db4b4b",
        "similar_to": "#414868", "complements": "#c0caf5", "extends": "#ff757f",
        "triggered_by": "#e0af68", "produces": "#73daca"})))

output.parent.mkdir(parents=True, exist_ok=True)
output.write_text(html)
print(f"visualize: wrote {output} ({len(nodes)} nodes, {len(links)} edges)")

if open_browser:
    webbrowser.open(f"file://{output.resolve()}")
