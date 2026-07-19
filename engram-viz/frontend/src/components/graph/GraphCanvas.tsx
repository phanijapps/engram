//! Full-viewport force-directed code graph.
//!
//! Nodes are entities on `calls` edges, colored by Louvain community and sized by
//! degree (call-graph connectivity). Clicking a node opens the detail panel;
//! focusing a node (from insights/search) recenters the camera on it.
//!
//! T7: node-count limiter (top-N by degree), community filter, hover tooltips.
//! T9: kind filter (renders only nodes of the selected kind).
//! T10: highlight overlay (blast-radius/path), path-mode clicks, group-by-kind.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import Graph from "react-force-graph-2d";
import { useGraphStore } from "../../store/graphStore";
import { communityColor, kindColor } from "../../lib/colors";
import type { GraphNode } from "../../lib/types";
import { GraphControls } from "./GraphControls";
import { KindLegend } from "./KindLegend";
import { PathStatus } from "./PathStatus";

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

  // T7: controls.
  const nodeLimit = useGraphStore((s) => s.nodeLimit);
  const hiddenCommunities = useGraphStore((s) => s.hiddenCommunities);

  // T9: kind filter.
  const kindFilter = useGraphStore((s) => s.kindFilter);

  // T10: highlights + path mode + grouping.
  const highlightNodeIds = useGraphStore((s) => s.highlightNodeIds);
  const highlightColor = useGraphStore((s) => s.highlightColor);
  const pathMode = useGraphStore((s) => s.pathMode);
  const pathFromId = useGraphStore((s) => s.pathFromId);
  const groupByKind = useGraphStore((s) => s.groupByKind);

  // Tooltip state — tracks mouse position for accurate tooltip placement.
  const mousePos = useRef<{ x: number; y: number }>({ x: 0, y: 0 });
  const [tooltip, setTooltip] = useState<{
    x: number;
    y: number;
    node: SimNode;
  } | null>(null);

  // --- Filtering pipeline --------------------------------------------------

  const hiddenSet = useMemo(
    () => new Set(hiddenCommunities),
    [hiddenCommunities],
  );

  const filteredNodes = useMemo(() => {
    let result = nodes;

    // Community filter.
    if (hiddenSet.size > 0) {
      result = result.filter(
        (n) => n.community === undefined || !hiddenSet.has(n.community),
      );
    }

    // Kind filter.
    if (kindFilter) {
      result = result.filter(
        (n) => n.kind.toLowerCase() === kindFilter.toLowerCase(),
      );
    }

    // Node-count limiter: keep top-N by degree.
    if (nodeLimit !== null && result.length > nodeLimit) {
      result = [...result]
        .sort((a, b) => b.degree - a.degree)
        .slice(0, nodeLimit);
    }

    return result;
  }, [nodes, hiddenSet, kindFilter, nodeLimit]);

  // Keep only links whose endpoints survive filtering.
  const visibleIds = useMemo(
    () => new Set(filteredNodes.map((n) => n.id)),
    [filteredNodes],
  );

  // Pre-size nodes by degree and attach a radius for the canvas painter.
  const graphData = useMemo(() => {
    const sized: SimNode[] = filteredNodes.map((n) => ({
      ...n,
      r: Math.min(4 + Math.sqrt(n.degree) * 1.6, 14),
    }));
    const filteredLinks =
      hiddenSet.size > 0 || kindFilter || nodeLimit !== null
        ? links.filter(
            (l) => visibleIds.has(l.source) && visibleIds.has(l.target),
          )
        : links;
    return { nodes: sized, links: [...filteredLinks] };
  }, [filteredNodes, links, hiddenSet, kindFilter, nodeLimit, visibleIds]);

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

  // --- Node painting -------------------------------------------------------

  const highlightSet = highlightNodeIds;
  const hlColor = highlightColor ?? "#d29922";

  const nodeColor = useCallback(
    (node: SimNode) => {
      if (groupByKind) return kindColor(node.kind) as unknown as string;
      return communityColor(node.community) as unknown as string;
    },
    [groupByKind],
  );

  const paintNode = useCallback(
    (node: SimNode, ctx: CanvasRenderingContext2D) => {
      const isFocus = node.id === focusNodeId;
      const isHover = node.id === hoveredNodeId;
      const isSelected = node.id === pathFromId;
      const isHighlighted = highlightSet.has(node.id);
      const radius = node.r ?? 5;

      // Determine fill color: highlight overlay takes precedence.
      let color: string;
      if (isHighlighted) {
        color = hlColor;
      } else if (groupByKind) {
        color = kindColor(node.kind);
      } else {
        color = communityColor(node.community);
      }

      // Path-mode endpoint ring.
      if (isSelected) {
        ctx.beginPath();
        ctx.arc(node.x!, node.y!, radius + 6, 0, 2 * Math.PI);
        ctx.strokeStyle = "#3fb950";
        ctx.lineWidth = 2;
        ctx.stroke();
      }

      // Glow / focus ring.
      if (isFocus || isHover || isHighlighted) {
        ctx.beginPath();
        ctx.arc(node.x!, node.y!, radius + 4, 0, 2 * Math.PI);
        ctx.fillStyle = color;
        ctx.globalAlpha = isHighlighted ? 0.4 : isFocus ? 0.35 : 0.18;
        ctx.fill();
        ctx.globalAlpha = 1;
      }

      ctx.beginPath();
      ctx.arc(node.x!, node.y!, radius, 0, 2 * Math.PI);
      ctx.fillStyle = color;
      ctx.globalAlpha = isFocus || isHighlighted ? 1 : 0.9;
      ctx.fill();
      ctx.globalAlpha = 1;

      // Dim non-highlighted nodes when a highlight is active.
      if (highlightSet.size > 0 && !isHighlighted) {
        ctx.beginPath();
        ctx.arc(node.x!, node.y!, radius, 0, 2 * Math.PI);
        ctx.fillStyle = "rgba(10, 14, 20, 0.55)";
        ctx.fill();
      }

      // Label prominent nodes when zoomed in enough.
      if (
        (isHover || isFocus || isHighlighted || node.degree >= 6) &&
        node.name?.length
      ) {
        const label =
          node.name.length > 22 ? node.name.slice(0, 21) + "…" : node.name;
        ctx.font = "5px ui-monospace, monospace";
        ctx.textAlign = "center";
        ctx.textBaseline = "bottom";
        ctx.fillStyle = isHighlighted ? hlColor : "#c9d1d9";
        ctx.fillText(label, node.x!, node.y! - radius - 1);
      }
    },
    [
      focusNodeId,
      hoveredNodeId,
      pathFromId,
      highlightSet,
      hlColor,
      groupByKind,
    ],
  );

  // --- Click handling: path mode vs. normal select ------------------------

  const handleNodeClick = useCallback(
    (node: any) => {
      if (pathMode) {
        // Path mode is handled by the PathStatus component via store actions.
        // Here we just select the node visually.
        selectNode(node.id);
        return;
      }
      selectNode(node.id);
    },
    [pathMode, selectNode],
  );

  const handleNodeHover = useCallback(
    (node: any) => {
      setHovered(node ? node.id : null);
      if (node) {
        setTooltip({
          x: mousePos.current.x,
          y: mousePos.current.y,
          node: node as SimNode,
        });
      } else {
        setTooltip(null);
      }
    },
    [setHovered],
  );

  return (
    <>
      <div
        ref={containerRef}
        className="absolute inset-0"
        onMouseMove={(e) => {
          mousePos.current = { x: e.clientX, y: e.clientY };
        }}
      >
        <Graph
          ref={fgRef}
          graphData={graphData}
          nodeId="id"
          linkSource="source"
          linkTarget="target"
          backgroundColor="#0a0e14"
          linkColor={(link: any) => {
            // Highlight links between highlighted nodes.
            if (highlightSet.size > 0) {
              const s =
                typeof link.source === "object"
                  ? link.source.id
                  : link.source;
              const t =
                typeof link.target === "object"
                  ? link.target.id
                  : link.target;
              if (highlightSet.has(s) && highlightSet.has(t)) {
                return hlColor;
              }
              return "rgba(125, 135, 148, 0.06)";
            }
            return "rgba(125, 135, 148, 0.18)";
          }}
          linkDirectionalArrowLength={3}
          linkDirectionalArrowRelPos={1}
          nodeRelSize={1}
          nodeColor={nodeColor as any}
          nodeCanvasObject={paintNode as any}
          nodeCanvasObjectMode={() => "replace"}
          cooldownTicks={120}
          onNodeClick={handleNodeClick}
          onNodeHover={handleNodeHover}
          enableNodeDrag={false}
        />
      </div>

      {/* Floating controls (T7 + T10) */}
      <GraphControls fgRef={fgRef} nodes={nodes} />

      {/* Entity-kind legend (T9) */}
      <KindLegend nodes={nodes} />

      {/* Path-mode status bar (T10) */}
      {pathMode && <PathStatus />}

      {/* Hover tooltip (T7) */}
      {tooltip && (
        <div
          className="pointer-events-none fixed z-50 max-w-xs rounded-md border border-base-600 bg-base-900/97 px-3 py-2 shadow-xl backdrop-blur"
          style={{
            left: tooltip.x + 12,
            top: tooltip.y - 48,
          }}
        >
          <div className="truncate font-mono text-xs text-ink">
            {tooltip.node.name}
          </div>
          <div className="flex items-center gap-1.5 text-[10px] text-ink-faint">
            <span
              className="inline-block h-2 w-2 rounded-full"
              style={{ backgroundColor: kindColor(tooltip.node.kind) }}
            />
            <span className="capitalize">{tooltip.node.kind}</span>
          </div>
          {tooltip.node.file && (
            <div className="truncate font-mono text-[10px] text-ink-faint">
              {tooltip.node.file}
            </div>
          )}
        </div>
      )}
    </>
  );
}
