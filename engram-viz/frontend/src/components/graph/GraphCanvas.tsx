//! Full-viewport force-directed code graph.
//!
//! Nodes are entities on `calls` edges, colored by Louvain community and sized by
//! degree (call-graph connectivity). Clicking a node opens the detail panel;
//! focusing a node (from insights/search) recenters the camera on it.

import { useEffect, useMemo, useRef } from "react";
import Graph from "react-force-graph-2d";
import { useGraphStore } from "../../store/graphStore";
import { communityColor } from "../../lib/colors";
import type { GraphNode } from "../../lib/types";

// Augmented node shape the force simulation mutates in place.
interface SimNode extends GraphNode {
  x?: number;
  y?: number;
  r?: number;
}

export function GraphCanvas() {
  const fgRef = useRef<any>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const nodes = useGraphStore((s) => s.nodes);
  const links = useGraphStore((s) => s.links);
  const focusNodeId = useGraphStore((s) => s.focusNodeId);
  const selectNode = useGraphStore((s) => s.selectNode);
  const setHovered = useGraphStore((s) => s.setHovered);
  const hoveredNodeId = useGraphStore((s) => s.hoveredNodeId);

  // Pre-size nodes by degree and attach a radius for the canvas painter.
  const graphData = useMemo(() => {
    const sized: SimNode[] = nodes.map((n) => ({
      ...n,
      r: Math.min(4 + Math.sqrt(n.degree) * 1.6, 14),
    }));
    return { nodes: sized, links: [...links] };
  }, [nodes, links]);

  // Configure the force simulation once data lands.
  useEffect(() => {
    const fg = fgRef.current;
    if (!fg) return;
    fg.d3Force("charge")?.strength(-18);
    fg.d3Force("link")?.distance(28).strength(0.12);
    fg.d3Force("center")?.strength(0.04);
    fg.d3ReheatSimulation();
  }, [graphData]);

  // Recenter on the focused node (from insight/search clicks).
  useEffect(() => {
    const fg = fgRef.current;
    if (!fg || !focusNodeId) return;
    const node = graphData.nodes.find((n) => n.id === focusNodeId);
    if (node && node.x !== undefined && node.y !== undefined) {
      fg.centerAt(node.x, node.y, 600);
      fg.zoom(2.2, 600);
    }
  }, [focusNodeId, graphData]);

  const nodeColor = (node: SimNode) =>
    communityColor(node.community) as unknown as string;

  const paintNode = (node: SimNode, ctx: CanvasRenderingContext2D) => {
    const isFocus = node.id === focusNodeId;
    const isHover = node.id === hoveredNodeId;
    const radius = node.r ?? 5;
    const color = communityColor(node.community);

    // Glow / focus ring.
    if (isFocus || isHover) {
      ctx.beginPath();
      ctx.arc(node.x!, node.y!, radius + 4, 0, 2 * Math.PI);
      ctx.fillStyle = color;
      ctx.globalAlpha = isFocus ? 0.35 : 0.18;
      ctx.fill();
      ctx.globalAlpha = 1;
    }

    ctx.beginPath();
    ctx.arc(node.x!, node.y!, radius, 0, 2 * Math.PI);
    ctx.fillStyle = color;
    ctx.globalAlpha = isFocus ? 1 : 0.9;
    ctx.fill();
    ctx.globalAlpha = 1;

    // Label prominent nodes when zoomed in enough.
    if ((isHover || isFocus || (node.degree >= 6)) && (node.name?.length)) {
      const label = node.name.length > 22 ? node.name.slice(0, 21) + "…" : node.name;
      ctx.font = "5px ui-monospace, monospace";
      ctx.textAlign = "center";
      ctx.textBaseline = "bottom";
      ctx.fillStyle = "#c9d1d9";
      ctx.fillText(label, node.x!, node.y! - radius - 1);
    }
  };

  return (
    <div ref={containerRef} className="absolute inset-0">
      <Graph
        ref={fgRef}
        graphData={graphData}
        nodeId="id"
        linkSource="source"
        linkTarget="target"
        backgroundColor="#0a0e14"
        linkColor={() => "rgba(125, 135, 148, 0.18)"}
        linkDirectionalArrowLength={3}
        linkDirectionalArrowRelPos={1}
        nodeRelSize={1}
        nodeColor={nodeColor as any}
        nodeCanvasObject={paintNode as any}
        nodeCanvasObjectMode={() => "replace"}
        cooldownTicks={120}
        onNodeClick={(node: any) => selectNode(node.id)}
        onNodeHover={(node: any) => setHovered(node ? node.id : null)}
        enableNodeDrag={false}
      />
    </div>
  );
}
