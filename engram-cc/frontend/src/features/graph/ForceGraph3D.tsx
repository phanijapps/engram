//! ForceGraph3D — the ACTUAL graph in immersive 3D. Real symbol nodes +
//! resolved call edges on a WebGL force simulation. Nodes colored by kind,
//! sized by degree, with orbit controls and click-to-select.
//!
//! The 3d-force-graph library is imperative: we create it once in a
//! ref-tracked useEffect, give it explicit dimensions via a ResizeObserver,
//! and clean up on unmount.

import { useEffect, useRef } from "react";
import ForceGraph3D from "3d-force-graph";
import type { SymbolGraphEdge, SymbolGraphNode } from "../../lib/api.ts";

/* Kind palette — tuned for the dark 3D viewport. */
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

/* eslint-disable @typescript-eslint/no-explicit-any */
interface GraphNode {
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
interface GraphLink {
  source: string | GraphNode;
  target: string | GraphNode;
  predicate: string;
}

export interface ForceGraph3DProps {
  nodes: SymbolGraphNode[];
  edges: SymbolGraphEdge[];
  selectedEntityId: string | null;
  onSelect: (id: string) => void;
}

export function Graph3DView({ nodes, edges, selectedEntityId, onSelect }: ForceGraph3DProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const graphRef = useRef<any>(null);
  const selectedRef = useRef<string | null>(null);
  selectedRef.current = selectedEntityId;

  useEffect(() => {
    const container = containerRef.current;
    if (!container || graphRef.current) return;

    // Wait for layout — the container must have non-zero dimensions
    const w = container.clientWidth || container.parentElement?.clientWidth || 800;
    const h = container.clientHeight || container.parentElement?.clientHeight || 600;

    if (w < 10 || h < 10) {
      console.warn("[ForceGraph3D] container has zero dimensions, retrying…");
      const t = setTimeout(() => {
        // Force a re-render by touching state — the parent will re-run this effect
        container.dataset.retry = String(Date.now());
      }, 100);
      return () => clearTimeout(t);
    }

    const maxDegree = Math.max(...nodes.map((n) => n.degree), 1);
    const graphData = {
      nodes: nodes.map((n) => ({
        id: n.id,
        name: n.name,
        kind: n.kind,
        degree: n.degree,
        color: kindColor(n.kind),
        val: 1 + (n.degree / maxDegree) * 6,
      })),
      links: edges.map((e) => ({
        source: e.source,
        target: e.target,
        predicate: e.predicate,
      })),
    };

    try {
      const graph = (ForceGraph3D as any)(container)
        .width(w)
        .height(h)
        .graphData(graphData)
        .backgroundColor("#07080d")
        .nodeLabel((n: any) => {
          const node = n as GraphNode;
          return `<div style="font-family: monospace; font-size: 11px; background: rgba(7,8,13,0.92); border: 1px solid rgba(125,249,255,0.25); border-radius: 4px; padding: 4px 8px; color: #e0e0e0;">
            <span style="color: ${kindColor(node.kind)}">${node.name}</span>
            <span style="color: #666; margin-left: 6px;">${node.kind} · ${node.degree} edges</span>
          </div>`;
        })
        .nodeColor((n: any) => {
          const node = n as GraphNode;
          return node.id === selectedRef.current ? "#ffffff" : node.color;
        })
        .nodeVal("val")
        .nodeRelSize(4.5)
        .nodeOpacity(0.9)
        .linkColor(() => "rgba(125, 249, 255, 0.18)")
        .linkOpacity(0.25)
        .linkWidth(0.5)
        .onNodeClick((n: any) => {
          const node = n as GraphNode;
          onSelect(node.id);
        })
        .onNodeHover((n: any) => {
          container.style.cursor = n ? "pointer" : "grab";
        })
        .warmupTicks(60)
        .cooldownTime(8000)
        .showNavInfo(false);

      graphRef.current = graph;

      // Handle resize
      const ro = new ResizeObserver((entries) => {
        for (const entry of entries) {
          const { width, height } = entry.contentRect;
          if (width > 10 && height > 10 && graphRef.current) {
            graphRef.current.width(width).height(height);
          }
        }
      });
      ro.observe(container);

      return () => {
        ro.disconnect();
        if (graphRef.current?._destructor) {
          graphRef.current._destructor();
        }
        graphRef.current = null;
        container.innerHTML = "";
      };
    } catch (err) {
      console.error("[ForceGraph3D] init failed:", err);
    }
  }, [nodes, edges, onSelect]);

  // Update selection highlight without rebuilding
  useEffect(() => {
    if (!graphRef.current) return;
    graphRef.current.nodeColor((n: any) => {
      const node = n as GraphNode;
      return node.id === selectedEntityId ? "#ffffff" : node.color;
    });
  }, [selectedEntityId]);

  return (
    <div
      ref={containerRef}
      style={{
        width: "100%",
        height: "100%",
        position: "relative",
        minHeight: 400,
      }}
    />
  );
}
