//! SymbolGraph — the ACTUAL graph on a 2D canvas: real entity nodes (symbols)
//! and real call edges, not the community meta-graph. Nodes are degree-sized
//! and kind-colored; `calls` edges are faint cyan, `routes_to` violet. Loads
//! the top-degree subgraph from /api/graph/subgraph (resolved-only default —
//! every rendered edge is a cross-file-resolved call). Interactions mirror
//! ForceGraph: wheel zoom, drag-pan, drag a node to pin, hover tooltip, click
//! selects (entity-detail panel). Rendering is plain canvas 2D (d3-force owns
//! layout only).

import { useEffect, useRef, useState, type CSSProperties } from "react";
import {
  forceCollide,
  forceLink,
  forceManyBody,
  forceSimulation,
  forceX,
  forceY,
  type Simulation,
  type SimulationLinkDatum,
  type SimulationNodeDatum,
} from "d3-force";

import type { SymbolGraphEdge, SymbolGraphNode } from "../../lib/api.ts";

/* Kind palette — tuned for the dark viewport. */
const KIND_COLORS: Record<string, string> = {
  function: "#f5d28d", // amber — the bulk of most codebases
  method: "#f5d28d",
  class: "#8ee5b8", // green — types
  struct: "#8ee5b8",
  interface: "#8ee5b8",
  trait: "#8ee5b8",
  enum: "#9adff0", // cyan — value types
  type_alias: "#9adff0",
  module: "#a4b6ff", // periwinkle — structure
  file: "#a4b6ff",
  endpoint: "#c9a8ff", // violet — HTTP surface
  api: "#c9a8ff",
  repository: "#dccba5", // cream — roots
};
const DEFAULT_COLOR = "#a4b6ff";
const SELECTED = "rgba(245, 236, 217, "; // cream highlight ring
const EDGE_CALLS = "rgba(125, 249, 255, 0.14)";
const EDGE_ROUTES = "rgba(192, 139, 255, 0.30)";
const LABEL = "rgba(245, 236, 217, 0.75)";

const kindColor = (kind: string): string => KIND_COLORS[kind] ?? DEFAULT_COLOR;
const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, v));

type SNode = SimulationNodeDatum & {
  id: string;
  name: string;
  kind: string;
  degree: number;
  r: number;
  pinned?: boolean;
};

type SLink = SimulationLinkDatum<SNode> & { predicate: string };

export interface SymbolGraphProps {
  nodes: SymbolGraphNode[];
  edges: SymbolGraphEdge[];
  highlight: string;
  selectedEntityId: string | null;
  onSelect: (id: string) => void;
}

export function SymbolGraph(props: SymbolGraphProps) {
  const { nodes, edges, highlight, selectedEntityId, onSelect } = props;

  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const simRef = useRef<Simulation<SNode, SLink> | null>(null);
  const nodesRef = useRef<SNode[]>([]);
  const linksRef = useRef<SLink[]>([]);
  const labelSetRef = useRef<Set<string>>(new Set());
  const transformRef = useRef({ k: 1, x: 0, y: 0 });
  const hoverRef = useRef<SNode | null>(null);
  const [hover, setHover] = useState<{ node: SNode; sx: number; sy: number } | null>(null);

  /* ---- build / rebuild the simulation ------------------------------------- */
  useEffect(() => {
    const maxDegree = nodes.reduce((m, n) => Math.max(m, n.degree), 1);
    const snodes: SNode[] = nodes.map((n) => ({
      id: n.id,
      name: n.name,
      kind: n.kind,
      degree: n.degree,
      r: clamp(3 + Math.sqrt(n.degree / maxDegree) * 14, 3, 17),
    }));
    const byId = new Map(snodes.map((n) => [n.id, n]));
    const slinks: SLink[] = edges
      .filter((e) => byId.has(e.source) && byId.has(e.target))
      .map((e) => ({ source: e.source, target: e.target, predicate: e.predicate }));

    // Label budget: the top 40 by degree — drawing all labels is noise.
    labelSetRef.current = new Set(
      [...snodes].sort((a, b) => b.degree - a.degree).slice(0, 40).map((n) => n.id),
    );

    nodesRef.current = snodes;
    linksRef.current = slinks;

    const sim = forceSimulation<SNode, SLink>(snodes)
      .force(
        "link",
        forceLink<SNode, SLink>(slinks)
          .id((d) => d.id)
          .distance(
            (l) =>
              30 +
              90 /
                (1 +
                  Math.min(
                    typeof l.source === "object" ? l.source.degree : 1,
                    typeof l.target === "object" ? l.target.degree : 1,
                  )),
          )
          .strength(0.12),
      )
      .force("charge", forceManyBody<SNode>().strength(-120).distanceMax(500))
      .force("collide", forceCollide<SNode>((d) => d.r + 2).iterations(2))
      .force("cx", forceX(0).strength(0.04))
      .force("cy", forceY(0).strength(0.04))
      .alphaDecay(0.025);

    sim.on("tick", () => {
      draw(canvasRef.current, nodesRef.current, linksRef.current, transformRef.current, {
        highlight,
        selectedEntityId,
        hover: hoverRef.current,
        labels: labelSetRef.current,
      });
    });
    simRef.current = sim;

    // first paint before the first tick
    draw(canvasRef.current, snodes, slinks, transformRef.current, {
      highlight,
      selectedEntityId,
      hover: null,
      labels: labelSetRef.current,
    });

    return () => {
      sim.stop();
      simRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [nodes, edges]);

  /* ---- restyle-only redraw -------------------------------------------------- */
  useEffect(() => {
    draw(canvasRef.current, nodesRef.current, linksRef.current, transformRef.current, {
      highlight,
      selectedEntityId,
      hover: hoverRef.current,
      labels: labelSetRef.current,
    });
  }, [highlight, selectedEntityId, hover]);

  /* ---- e2e hook: live node screen positions (force layouts move nodes, so
   * tests cannot compute click targets from API data alone) ------------------ */
  useEffect(() => {
    const w = window as unknown as Record<string, unknown>;
    w.__engramSymbolGraphNodes = () => {
      const canvas = canvasRef.current;
      if (!canvas) return [];
      const rect = canvas.getBoundingClientRect();
      const t = transformRef.current;
      return nodesRef.current.map((n) => ({
        id: n.id,
        name: n.name,
        kind: n.kind,
        degree: n.degree,
        x: rect.left + t.x + (n.x ?? 0) * t.k,
        y: rect.top + t.y + (n.y ?? 0) * t.k,
        r: n.r * t.k,
      }));
    };
    return () => {
      delete w.__engramSymbolGraphNodes;
    };
  }, []);

  /* ---- canvas sizing (DPR-aware) -------------------------------------------- */
  useEffect(() => {
    const canvas = canvasRef.current;
    const wrap = wrapRef.current;
    if (!canvas || !wrap) return;
    const resize = () => {
      const dpr = window.devicePixelRatio || 1;
      const w = wrap.clientWidth;
      const h = wrap.clientHeight;
      canvas.width = Math.max(1, Math.round(w * dpr));
      canvas.height = Math.max(1, Math.round(h * dpr));
      canvas.style.width = `${w}px`;
      canvas.style.height = `${h}px`;
      draw(canvas, nodesRef.current, linksRef.current, transformRef.current, {
        highlight,
        selectedEntityId,
        hover: hoverRef.current,
        labels: labelSetRef.current,
      });
    };
    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(wrap);
    return () => ro.disconnect();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /* ---- interactions --------------------------------------------------------- */
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    let mode: "idle" | "pan" | "node" = "idle";
    let dragNode: SNode | null = null;
    let moved = 0;
    let last = { x: 0, y: 0 };

    const toWorld = (cx: number, cy: number) => {
      const rect = canvas.getBoundingClientRect();
      const t = transformRef.current;
      return { x: (cx - rect.left - t.x) / t.k, y: (cy - rect.top - t.y) / t.k };
    };

    const pick = (wx: number, wy: number): SNode | null => {
      const ns = nodesRef.current;
      for (let i = ns.length - 1; i >= 0; i--) {
        const n = ns[i];
        const dx = (n.x ?? 0) - wx;
        const dy = (n.y ?? 0) - wy;
        if (dx * dx + dy * dy <= (n.r + 4) * (n.r + 4)) return n;
      }
      return null;
    };

    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const t = transformRef.current;
      const rect = canvas.getBoundingClientRect();
      const mx = e.clientX - rect.left;
      const my = e.clientY - rect.top;
      const factor = Math.exp(-e.deltaY * 0.0012);
      const k = clamp(t.k * factor, 0.15, 8);
      t.x = mx - ((mx - t.x) / t.k) * k;
      t.y = my - ((my - t.y) / t.k) * k;
      t.k = k;
      draw(canvas, nodesRef.current, linksRef.current, t, {
        highlight,
        selectedEntityId,
        hover: hoverRef.current,
        labels: labelSetRef.current,
      });
    };

    const onDown = (e: PointerEvent) => {
      canvas.setPointerCapture(e.pointerId);
      const w = toWorld(e.clientX, e.clientY);
      const hit = pick(w.x, w.y);
      moved = 0;
      last = { x: e.clientX, y: e.clientY };
      if (hit) {
        mode = "node";
        dragNode = hit;
        hit.fx = hit.x;
        hit.fy = hit.y;
        simRef.current?.alphaTarget(0.25).restart();
      } else {
        mode = "pan";
      }
    };

    const onMove = (e: PointerEvent) => {
      if (mode === "idle") {
        const w = toWorld(e.clientX, e.clientY);
        const hit = pick(w.x, w.y);
        if (hit !== hoverRef.current) {
          hoverRef.current = hit;
          const rect = canvas.getBoundingClientRect();
          setHover(hit ? { node: hit, sx: e.clientX - rect.left, sy: e.clientY - rect.top } : null);
        }
        return;
      }
      moved += Math.abs(e.clientX - last.x) + Math.abs(e.clientY - last.y);
      last = { x: e.clientX, y: e.clientY };
      if (mode === "pan") {
        const t = transformRef.current;
        t.x += e.movementX;
        t.y += e.movementY;
        draw(canvas, nodesRef.current, linksRef.current, t, {
          highlight,
          selectedEntityId,
          hover: hoverRef.current,
          labels: labelSetRef.current,
        });
      } else if (mode === "node" && dragNode) {
        const w = toWorld(e.clientX, e.clientY);
        dragNode.fx = w.x;
        dragNode.fy = w.y;
      }
    };

    const onUp = (e: PointerEvent) => {
      if (canvas.hasPointerCapture(e.pointerId)) canvas.releasePointerCapture(e.pointerId);
      if (mode === "node" && dragNode) {
        if (moved < 4) {
          // click (not drag): select — pinned until deselect keeps it steady
          onSelect(dragNode.id);
          hoverRef.current = dragNode;
        }
        if (!selectedSet(selectedEntityId, dragNode.id)) {
          dragNode.fx = null;
          dragNode.fy = null;
        }
        simRef.current?.alphaTarget(0);
      }
      mode = "idle";
      dragNode = null;
    };

    const onLeave = () => {
      hoverRef.current = null;
      setHover(null);
    };

    canvas.addEventListener("wheel", onWheel, { passive: false });
    canvas.addEventListener("pointerdown", onDown);
    canvas.addEventListener("pointermove", onMove);
    canvas.addEventListener("pointerup", onUp);
    canvas.addEventListener("pointerleave", onLeave);
    return () => {
      canvas.removeEventListener("wheel", onWheel);
      canvas.removeEventListener("pointerdown", onDown);
      canvas.removeEventListener("pointermove", onMove);
      canvas.removeEventListener("pointerup", onUp);
      canvas.removeEventListener("pointerleave", onLeave);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [onSelect]);

  return (
    <div ref={wrapRef} style={wrapStyle}>
      <canvas
        ref={canvasRef}
        style={{ display: "block", width: "100%", height: "100%", cursor: "crosshair" }}
      />
      {hover && (
        <div style={{ ...tooltipStyle, left: hover.sx + 14, top: hover.sy + 10 }}>
          <span style={{ color: kindColor(hover.node.kind) }}>{hover.node.name}</span>
          <span style={tooltipMeta}>
            {" "}
            {hover.node.kind} · {hover.node.degree} edges
          </span>
        </div>
      )}
    </div>
  );
}

function selectedSet(selectedEntityId: string | null, id: string): boolean {
  return selectedEntityId === id;
}

interface DrawOpts {
  highlight: string;
  selectedEntityId: string | null;
  hover: SNode | null;
  labels: Set<string>;
}

function draw(
  canvas: HTMLCanvasElement | null,
  nodes: SNode[],
  links: SLink[],
  t: { k: number; x: number; y: number },
  opts: DrawOpts,
): void {
  if (!canvas) return;
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  const dpr = window.devicePixelRatio || 1;
  const w = canvas.width / dpr;
  const h = canvas.height / dpr;

  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, w, h);
  ctx.translate(t.x, t.y);
  ctx.scale(t.k, t.k);

  // edges
  ctx.lineWidth = 1 / Math.sqrt(t.k);
  for (const l of links) {
    const s = l.source as SNode;
    const tg = l.target as SNode;
    ctx.strokeStyle = l.predicate === "routes_to" ? EDGE_ROUTES : EDGE_CALLS;
    if (l.predicate === "routes_to") ctx.setLineDash([4, 3]);
    else ctx.setLineDash([]);
    ctx.beginPath();
    ctx.moveTo(s.x ?? 0, s.y ?? 0);
    ctx.lineTo(tg.x ?? 0, tg.y ?? 0);
    ctx.stroke();
  }
  ctx.setLineDash([]);

  // highlight: edges touching the selected node glow
  const selected = nodes.find((n) => n.id === opts.selectedEntityId);
  if (selected) {
    ctx.strokeStyle = "rgba(245, 236, 217, 0.55)";
    ctx.lineWidth = 1.5 / Math.sqrt(t.k);
    for (const l of links) {
      const s = l.source as SNode;
      const tg = l.target as SNode;
      if (s === selected || tg === selected) {
        ctx.beginPath();
        ctx.moveTo(s.x ?? 0, s.y ?? 0);
        ctx.lineTo(tg.x ?? 0, tg.y ?? 0);
        ctx.stroke();
      }
    }
  }

  // labels (before nodes so circles sit on top of the text baseline)
  if (t.k > 0.5) {
    ctx.font = "10px var(--font-mono), ui-monospace, monospace";
    ctx.textAlign = "center";
    ctx.textBaseline = "top";
    for (const n of nodes) {
      if (!opts.labels.has(n.id) && n !== opts.hover && n !== selected) continue;
      ctx.fillStyle = LABEL;
      ctx.fillText(n.name, n.x ?? 0, (n.y ?? 0) + n.r + 3);
    }
  }

  // nodes
  for (const n of nodes) {
    const isSel = n.id === opts.selectedEntityId;
    const isHov = n === opts.hover;
    const isMatch = opts.highlight !== "" && n.name.toLowerCase().includes(opts.highlight.toLowerCase());
    // soft glow disc for hover/selected
    if (isSel || isHov) {
      ctx.fillStyle = `${SELECTED}${isSel ? 0.25 : 0.15})`;
      ctx.beginPath();
      ctx.arc(n.x ?? 0, n.y ?? 0, n.r + 4, 0, Math.PI * 2);
      ctx.fill();
    }
    ctx.fillStyle = kindColor(n.kind);
    ctx.globalAlpha = isMatch || isSel || isHov ? 1 : 0.85;
    ctx.beginPath();
    ctx.arc(n.x ?? 0, n.y ?? 0, n.r, 0, Math.PI * 2);
    ctx.fill();
    ctx.globalAlpha = 1;
    if (isSel) {
      ctx.strokeStyle = "rgba(245, 236, 217, 0.9)";
      ctx.lineWidth = 1.5 / Math.sqrt(t.k);
      ctx.stroke();
    }
  }
}

const wrapStyle: CSSProperties = { width: "100%", height: "100%", position: "relative" };
const tooltipStyle: CSSProperties = {
  position: "absolute",
  pointerEvents: "none",
  fontFamily: "var(--font-mono)",
  fontSize: 11,
  background: "var(--sidebar)",
  border: "1px solid var(--border)",
  borderRadius: "var(--radius-sm)",
  padding: "3px 7px",
  whiteSpace: "nowrap",
  zIndex: 10,
};
const tooltipMeta: CSSProperties = { color: "var(--muted-foreground)" };
