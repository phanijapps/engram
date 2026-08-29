//! ForceGraph3D — the ACTUAL graph in immersive 3D. Real symbol nodes +
//! resolved call edges rendered on a WebGL force simulation (three.js via
//! 3d-force-graph). Nodes colored by kind, sized by degree, with orbit
//! controls, hover tooltips, and click-to-select into the entity-detail panel.
//! Data from /api/graph/subgraph (same endpoint as the 2D view).

import { useEffect, useRef, useState, type CSSProperties } from "react";
import ForceGraph3D from "3d-force-graph";
import type { SymbolGraphEdge, SymbolGraphNode } from "../../lib/api.ts";

/* Kind palette — glowing variants tuned for the dark 3D viewport. */
const KIND_COLORS: Record<string, string> = {
  function: "#f5d28d",
  method: "#f5d28d",
  class: "#8ee5b8",
  struct: "#8ee5b8",
  interface: "#8ee5b8",
  trait: "#8ee5b8",
  enum: "#9adff0",
  type_alias: "#9adff0",
  module: "#a4b6ff",
  file: "#a4b6ff",
  endpoint: "#c9a8ff",
  api: "#c9a8ff",
  repository: "#dccba5",
};
const DEFAULT_COLOR = "#a4b6ff";
const kindColor = (kind: string): string => KIND_COLORS[kind] ?? DEFAULT_COLOR;

interface F3DNode {
  id: string;
  name: string;
  kind: string;
  degree: number;
  color: string;
  val: number;
  x?: number;
  y?: number;
  z?: number;
}
interface F3DLink {
  source: string | F3DNode;
  target: string | F3DNode;
  predicate: string;
}

export interface ForceGraph3DProps {
  nodes: SymbolGraphNode[];
  edges: SymbolGraphEdge[];
  highlight: string;
  selectedEntityId: string | null;
  onSelect: (id: string) => void;
}

export function Graph3DView(props: ForceGraph3DProps) {
  const { nodes, edges, highlight, selectedEntityId, onSelect } = props;
  const containerRef = useRef<HTMLDivElement | null>(null);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const graphRef = useRef<any>(null);
  const [ready, setReady] = useState(false);

  useEffect(() => {
    const container = containerRef.current;
    if (!container || graphRef.current) return;

    const maxDegree = Math.max(...nodes.map((n) => n.degree), 1);
    const graphData = {
      nodes: nodes.map((n) => ({
        id: n.id,
        name: n.name,
        kind: n.kind,
        degree: n.degree,
        color: kindColor(n.kind),
        val: 1 + (n.degree / maxDegree) * 6,
      })) as unknown as F3DNode[],
      links: edges.map((e) => ({ source: e.source, target: e.target, predicate: e.predicate })) as unknown as F3DLink[],
    };

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const graph = (ForceGraph3D as any)(container)
      .graphData(graphData)
      .backgroundColor("#07080d")
      .nodeLabel((n: F3DNode) =>
        `<div style="font-family: monospace; font-size: 11px; background: rgba(7,8,13,0.9); border: 1px solid rgba(125,249,255,0.2); border-radius: 4px; padding: 4px 8px;">
          <span style="color: ${kindColor(n.kind)}">${n.name}</span>
          <span style="color: #666; margin-left: 6px;">${n.kind} · ${n.degree} edges</span>
        </div>`)
      .nodeColor((n: F3DNode) => (n.id === selectedEntityId ? "#ffffff" : n.color))
      .nodeVal("val")
      .nodeRelSize(4.5)
      .linkColor(() => "rgba(125, 249, 255, 0.15)")
      .linkOpacity(0.3)
      .linkWidth(0.5)
      .onNodeClick((n: F3DNode) => {
        onSelect(n.id);
      })
      .onNodeHover((n: F3DNode | null) => {
        container.style.cursor = n ? "pointer" : "grab";
      })
      .warmupTicks(80)
      .cooldownTime(10000);

    graphRef.current = graph;
    setReady(true);

    return () => {
      if (graphRef.current) {
        if (graphRef.current._destructor) graphRef.current._destructor();
        graphRef.current = null;
      }
      container.innerHTML = "";
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Update selection highlight without rebuilding
  useEffect(() => {
    if (!graphRef.current || !ready) return;
    graphRef.current
      .nodeColor((n: F3DNode) => (n.id === selectedEntityId ? "#ffffff" : n.color))
      .nodeRelSize(selectedEntityId ? 6 : 4.5);
  }, [selectedEntityId, ready]);

  return <div ref={containerRef} style={{ width: "100%", height: "100%", position: "relative" }} />;
}



