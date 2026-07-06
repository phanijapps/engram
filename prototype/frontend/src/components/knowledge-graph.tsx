import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import ForceGraph2D from "react-force-graph-2d";
import {
  assignCommunities,
  colorForCommunity,
  highlightSet,
  tierForKind,
  type GraphEdge,
  type GraphNode,
} from "@/lib/graph-model";

type Props = { nodes: GraphNode[]; edges: GraphEdge[] };

type VizNode = GraphNode & {
  community: number;
  color: string;
  size: number;
  alwaysLabel: boolean;
  x?: number;
  y?: number;
};

const BG = "#0c1124";

export function KnowledgeGraph({ nodes, edges }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [dims, setDims] = useState({ width: 800, height: 600 });
  const [hovered, setHovered] = useState<string | null>(null);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const ro = new ResizeObserver((entries) => {
      const r = entries[0]?.contentRect;
      if (r) setDims({ width: Math.max(1, r.width), height: Math.max(1, r.height) });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const communities = useMemo(() => assignCommunities(nodes, edges), [nodes, edges]);

  const data = useMemo(() => {
    const maxDeg = Math.max(1, ...nodes.map((n) => n.degree ?? 0));
    const vizNodes: VizNode[] = nodes.map((n) => {
      const t = tierForKind(n.kind);
      const community = communities.get(n.id) ?? 0;
      const size = t.baseSize + Math.min(9, ((n.degree ?? 0) / maxDeg) * 9);
      return {
        ...n,
        community,
        color: colorForCommunity(community),
        size,
        alwaysLabel: t.alwaysLabel,
      };
    });
    const links = edges.map((e) => ({ source: e.subject, target: e.object }));
    return { nodes: vizNodes, links };
  }, [nodes, edges, communities]);

  const highlighted = useMemo(
    () => (hovered ? highlightSet(edges, hovered) : null),
    [hovered, edges],
  );

  const paintNode = useCallback(
    (node: VizNode, ctx: CanvasRenderingContext2D, scale: number) => {
      const dim = highlighted && !highlighted.has(node.id);
      const size = node.size;
      ctx.globalAlpha = dim ? 0.12 : 1;

      ctx.beginPath();
      ctx.arc(node.x ?? 0, node.y ?? 0, size, 0, 2 * Math.PI);
      ctx.fillStyle = node.color;
      ctx.fill();

      const showLabel = node.alwaysLabel || scale > 2.2 || (highlighted?.has(node.id) ?? false);
      if (showLabel && !dim) {
        const fontSize = Math.max(3, size * 0.85);
        ctx.font = `${fontSize}px ui-sans-serif, system-ui, sans-serif`;
        ctx.textAlign = "center";
        ctx.textBaseline = "top";
        ctx.fillStyle = "#e6ecff";
        ctx.fillText(node.name, node.x ?? 0, (node.y ?? 0) + size + 1);
      }
      ctx.globalAlpha = 1;
    },
    [highlighted],
  );

  const linkColor = useCallback(
    (link: { source: unknown; target: unknown }) => {
      if (!highlighted) return "rgba(150,166,189,0.18)";
      const s = idOf(link.source);
      const t = idOf(link.target);
      return highlighted.has(s) && highlighted.has(t)
        ? "rgba(110,168,255,0.7)"
        : "rgba(150,166,189,0.05)";
    },
    [highlighted],
  );

  return (
    <div ref={containerRef} className="relative h-full w-full" style={{ background: BG }}>
      <ForceGraph2D
        width={dims.width}
        height={dims.height}
        graphData={data}
        backgroundColor={BG}
        nodeRelSize={1}
        nodeVal={(n: VizNode) => n.size}
        nodeCanvasObject={paintNode}
        linkColor={linkColor}
        linkWidth={(l: { source: unknown; target: unknown }) =>
          highlighted && highlighted.has(idOf(l.source)) && highlighted.has(idOf(l.target)) ? 1.5 : 0.4
        }
        cooldownTicks={120}
        onNodeHover={(n: VizNode | null) => setHovered(n?.id ?? null)}
        enableNodeDrag={false}
      />
      <Legend nodes={data.nodes} />
    </div>
  );
}

function idOf(endpoint: unknown): string {
  if (typeof endpoint === "string") return endpoint;
  if (endpoint && typeof endpoint === "object" && "id" in endpoint) {
    return String((endpoint as { id: unknown }).id);
  }
  return "";
}

function Legend({ nodes }: { nodes: VizNode[] }) {
  const communities = [...new Set(nodes.map((n) => n.community))].sort((a, b) => a - b).slice(0, 8);
  return (
    <div className="pointer-events-none absolute bottom-3 left-3 flex flex-col gap-1 rounded-md bg-black/40 p-2 text-[10px] text-slate-300 backdrop-blur">
      <span className="text-slate-400">size = degree · color = cluster</span>
      <div className="flex flex-wrap gap-1.5">
        {communities.map((c) => (
          <span key={c} className="flex items-center gap-1">
            <span
              className="inline-block size-2 rounded-full"
              style={{ background: colorForCommunity(c) }}
            />
            {c}
          </span>
        ))}
      </div>
    </div>
  );
}
