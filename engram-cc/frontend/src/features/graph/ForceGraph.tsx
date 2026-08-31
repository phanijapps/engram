//! ForceGraph — d3-force community meta-graph on a 2D canvas (the Observatory
//! viewport's renderer). Replaces the original deck.gl orthographic view:
//! communities are force-directed nodes (radius ∝ √memberCount, seeded from the
//! server's layout coords), meta-edges are links (distance ∝ 1/weight), and a
//! drill explodes a community's member entities as a violet cluster anchored to
//! it. Interactions: wheel zoom, drag-pan, drag a node to pin it, hover
//! tooltip, click a community to drill / a member to select it. Rendering is
//! plain canvas 2D (d3-force owns layout only) — no d3-selection dependency.

import { useEffect, useRef, useState, type RefObject } from "react";
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

import type {
  CommunityMetaEdge,
  CommunityMetaNode,
  GraphEntityView,
  GraphRelationshipView,
} from "../../lib/api.ts";

/* Palette — tuned for the dark Observatory viewport. */
const ACCENT = "rgba(125, 249, 255, "; // cyan — overview communities/edges
const VIOLET = "rgba(192, 139, 255, "; // drill members/edges
const WHITE = "rgba(255, 255, 255, ";

type FNode = SimulationNodeDatum & {
  id: string;
  name: string;
  kind: "community" | "member";
  r: number; // world-space radius
  memberCount?: number;
};

type FLink = SimulationLinkDatum<FNode> & {
  kind: "meta" | "anchor" | "member";
  weight?: number;
};

export interface ForceGraphProps {
  communities: CommunityMetaNode[];
  edges: CommunityMetaEdge[];
  highlight: string;
  drill: { id: string } | null;
  members: GraphEntityView[];
  memberEdges: GraphRelationshipView[];
  selectedEntityId: string | null;
  onCommunityClick: (node: CommunityMetaNode) => void;
  onMemberClick: (id: string) => void;
}

export function ForceGraph(props: ForceGraphProps) {
  const {
    communities,
    edges,
    highlight,
    drill,
    members,
    memberEdges,
    selectedEntityId,
    onCommunityClick,
    onMemberClick,
  } = props;

  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const simRef = useRef<Simulation<FNode, FLink> | null>(null);
  const nodesRef = useRef<FNode[]>([]);
  const linksRef = useRef<FLink[]>([]);
  /** positions carried across rebuilds so layout changes don't jump */
  const prevPosRef = useRef<Map<string, { x: number; y: number }>>(new Map());
  const transformRef = useRef({ k: 1, x: 0, y: 0 });
  const hoverRef = useRef<FNode | null>(null);
  const [hover, setHover] = useState<{ node: FNode; sx: number; sy: number } | null>(null);

  /* ---- build / rebuild the simulation ------------------------------------- */
  useEffect(() => {
    const prev = prevPosRef.current;
    const nodes: FNode[] = communities.map((c) => {
      const seed = prev.get(c.id) ?? { x: c.x ?? 0, y: c.y ?? 0 };
      return {
        id: c.id,
        name: c.name,
        kind: "community" as const,
        r: clamp(Math.sqrt(c.memberCount) * 1.2, 4, 18),
        memberCount: c.memberCount,
        x: seed.x,
        y: seed.y,
      };
    });
    const byId = new Map(nodes.map((n) => [n.id, n]));

    const links: FLink[] = edges.map((e) => ({ ...e, kind: "meta" as const }));

    // drill cluster: member entities anchored to their community node, with
    // the relationships among members drawn as violet links.
    if (drill) {
      const center = byId.get(drill.id);
      if (center) {
        for (const m of members) {
          const seed = prev.get(m.id) ?? {
            x: (center.x ?? 0) + (Math.random() - 0.5) * 40,
            y: (center.y ?? 0) + (Math.random() - 0.5) * 40,
          };
          const n: FNode = {
            id: m.id,
            name: m.name,
            kind: "member",
            r: 5,
            x: seed.x,
            y: seed.y,
          };
          nodes.push(n);
          byId.set(m.id, n);
          links.push({ source: drill.id, target: m.id, kind: "anchor" });
        }
        for (const e of memberEdges) {
          if (byId.has(e.source) && byId.has(e.target)) {
            links.push({ ...e, kind: "member" });
          }
        }
      }
    }

    nodesRef.current = nodes;
    linksRef.current = links;

    const sim = forceSimulation<FNode, FLink>(nodes)
      .force(
        "link",
        forceLink<FNode, FLink>(links)
          .id((d) => d.id)
          .distance((l) =>
            l.kind === "anchor" ? 26 : l.kind === "member" ? 34 : 46 + 60 / (1 + (l.weight ?? 1)),
          )
          .strength((l) => (l.kind === "anchor" ? 0.8 : l.kind === "member" ? 0.25 : 0.08)),
      )
      .force(
        "charge",
        forceManyBody<FNode>()
          .strength((d) => (d.kind === "member" ? -50 : -160))
          .distanceMax(600),
      )
      .force("collide", forceCollide<FNode>((d) => d.r + 3).iterations(2))
      .force("cx", forceX(0).strength(0.03))
      .force("cy", forceY(0).strength(0.03))
      .alphaDecay(0.02);

    sim.on("tick", () => {
      // carry live positions for the next rebuild
      for (const n of nodes) prev.set(n.id, { x: n.x ?? 0, y: n.y ?? 0 });
      draw(canvasRef.current, nodesRef.current, linksRef.current, transformRef.current, {
        highlight,
        selectedEntityId,
        hover: hoverRef.current,
      });
    });
    simRef.current = sim;

    // first paint before the first tick
    const wrap = wrapRef.current;
    fitInitial(transformRef, communities, wrap?.clientWidth ?? 800, wrap?.clientHeight ?? 600);
    draw(canvasRef.current, nodes, links, transformRef.current, {
      highlight,
      selectedEntityId,
      hover: null,
    });

    return () => {
      sim.stop();
      simRef.current = null;
    };
    // rebuild on data identity or drill membership, not on highlight/selection
    // (those restyle only — handled by the draw effect below).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [communities, edges, drill?.id, members, memberEdges]);

  /* ---- restyle-only redraw (highlight / selection / hover) ---------------- */
  useEffect(() => {
    draw(canvasRef.current, nodesRef.current, linksRef.current, transformRef.current, {
      highlight,
      selectedEntityId,
      hover: hoverRef.current,
    });
  }, [highlight, selectedEntityId, hover]);

  /* ---- e2e hook: live node screen positions (deterministic drill clicking;
   * a force layout moves nodes off their server-coord seeds, so tests cannot
   * compute click targets from API data alone) ------------------------------ */
  useEffect(() => {
    const w = window as unknown as Record<string, unknown>;
    w.__engramForceGraphNodes = () => {
      const canvas = canvasRef.current;
      if (!canvas) return [];
      const rect = canvas.getBoundingClientRect();
      const t = transformRef.current;
      return nodesRef.current.map((n) => ({
        id: n.id,
        kind: n.kind,
        name: n.name,
        memberCount: n.memberCount ?? 0,
        x: rect.left + t.x + (n.x ?? 0) * t.k,
        y: rect.top + t.y + (n.y ?? 0) * t.k,
        r: n.r * t.k,
      }));
    };
    return () => {
      delete w.__engramForceGraphNodes;
    };
  }, []);

  /* ---- canvas sizing (DPR-aware) ------------------------------------------ */
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
      });
    };
    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(wrap);
    return () => ro.disconnect();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /* ---- interactions -------------------------------------------------------- */
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    let mode: "idle" | "pan" | "node" = "idle";
    let dragNode: FNode | null = null;
    let moved = 0;
    let last = { x: 0, y: 0 };
    let downWorld = { x: 0, y: 0 };

    const toWorld = (cx: number, cy: number) => {
      const rect = canvas.getBoundingClientRect();
      const t = transformRef.current;
      return {
        x: (cx - rect.left - t.x) / t.k,
        y: (cy - rect.top - t.y) / t.k,
      };
    };

    const pick = (wx: number, wy: number): FNode | null => {
      const nodes = nodesRef.current;
      for (let i = nodes.length - 1; i >= 0; i--) {
        const n = nodes[i];
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
      const px = e.clientX - rect.left;
      const py = e.clientY - rect.top;
      const k = clamp(t.k * Math.exp(-e.deltaY * 0.0012), 0.15, 10);
      // zoom about the pointer: keep the world point under it fixed
      t.x = px - ((px - t.x) / t.k) * k;
      t.y = py - ((py - t.y) / t.k) * k;
      t.k = k;
      draw(canvas, nodesRef.current, linksRef.current, t, {
        highlight,
        selectedEntityId,
        hover: hoverRef.current,
      });
    };

    const onPointerDown = (e: PointerEvent) => {
      canvas.setPointerCapture(e.pointerId);
      last = { x: e.clientX, y: e.clientY };
      moved = 0;
      const w = toWorld(e.clientX, e.clientY);
      downWorld = w;
      const n = pick(w.x, w.y);
      if (n) {
        mode = "node";
        dragNode = n;
      } else {
        mode = "pan";
      }
    };

    const onPointerMove = (e: PointerEvent) => {
      const dx = e.clientX - last.x;
      const dy = e.clientY - last.y;
      if (mode === "node" && dragNode) {
        moved += Math.abs(dx) + Math.abs(dy);
        const w = toWorld(e.clientX, e.clientY);
        dragNode.fx = w.x;
        dragNode.fy = w.y;
        simRef.current?.alphaTarget(0.25).restart();
      } else if (mode === "pan") {
        moved += Math.abs(dx) + Math.abs(dy);
        transformRef.current.x += dx;
        transformRef.current.y += dy;
        draw(canvas, nodesRef.current, linksRef.current, transformRef.current, {
          highlight,
          selectedEntityId,
          hover: hoverRef.current,
        });
      } else {
        // idle hover
        const w = toWorld(e.clientX, e.clientY);
        const n = pick(w.x, w.y);
        hoverRef.current = n;
        setHover(n ? { node: n, sx: e.clientX, sy: e.clientY } : null);
        canvas.style.cursor = n ? "pointer" : "default";
      }
      last = { x: e.clientX, y: e.clientY };
    };

    const onPointerUp = (e: PointerEvent) => {
      const wasNode = mode === "node" && dragNode;
      const n = dragNode;
      if (n) {
        n.fx = null;
        n.fy = null;
        simRef.current?.alphaTarget(0);
      }
      // a press-release without movement is a click
      if (moved < 4) {
        const hit = n ?? pick(downWorld.x, downWorld.y);
        if (hit?.kind === "community" && hit.memberCount !== undefined) {
          onCommunityClick({ id: hit.id, name: hit.name, memberCount: hit.memberCount });
        } else if (hit?.kind === "member") {
          onMemberClick(hit.id);
        }
      }
      mode = "idle";
      dragNode = null;
      void e;
    };

    const onLeave = () => {
      hoverRef.current = null;
      setHover(null);
    };

    canvas.addEventListener("wheel", onWheel, { passive: false });
    canvas.addEventListener("pointerdown", onPointerDown);
    canvas.addEventListener("pointermove", onPointerMove);
    canvas.addEventListener("pointerup", onPointerUp);
    canvas.addEventListener("pointerleave", onLeave);
    return () => {
      canvas.removeEventListener("wheel", onWheel);
      canvas.removeEventListener("pointerdown", onPointerDown);
      canvas.removeEventListener("pointermove", onPointerMove);
      canvas.removeEventListener("pointerup", onPointerUp);
      canvas.removeEventListener("pointerleave", onLeave);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [highlight, selectedEntityId]);

  return (
    <div
      ref={wrapRef}
      style={{ position: "absolute", inset: 0, overflow: "hidden" }}
    >
      <canvas
        ref={canvasRef}
        style={{ display: "block", width: "100%", height: "100%", touchAction: "none" }}
      />
      {hover && (
        <div
          style={{
            position: "fixed",
            left: hover.sx + 12,
            top: hover.sy + 12,
            fontFamily: "var(--font-mono)",
            fontSize: 11,
            color: "var(--foreground)",
            background: "var(--popover)",
            border: "1px solid var(--border)",
            borderRadius: "var(--radius-md)",
            boxShadow: "var(--shadow-dropdown)",
            padding: "4px 8px",
            pointerEvents: "none",
            zIndex: 5,
            whiteSpace: "pre",
          }}
        >
          {hover.node.kind === "community"
            ? `${hover.node.name}\n${hover.node.memberCount} members`
            : hover.node.name}
        </div>
      )}
    </div>
  );
}

/* ---- rendering ------------------------------------------------------------ */

interface StyleState {
  highlight: string;
  selectedEntityId: string | null;
  hover: FNode | null;
}

function draw(
  canvas: HTMLCanvasElement | null,
  nodes: FNode[],
  links: FLink[],
  t: { k: number; x: number; y: number },
  s: StyleState,
) {
  if (!canvas) return;
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  const dpr = window.devicePixelRatio || 1;

  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  // dark viewport background
  ctx.fillStyle = "#07080d";
  ctx.fillRect(0, 0, canvas.width, canvas.height);

  ctx.setTransform(t.k * dpr, 0, 0, t.k * dpr, t.x * dpr, t.y * dpr);

  const term = s.highlight.trim().toLowerCase();

  const matches = (n: FNode) =>
    !term || n.id.toLowerCase().includes(term) || n.name.toLowerCase().includes(term);

  // edges beneath nodes
  ctx.lineWidth = 1 / t.k;
  for (const l of links) {
    const a = l.source as FNode | string;
    const b = l.target as FNode | string;
    const an = typeof a === "string" ? null : a;
    const bn = typeof b === "string" ? null : b;
    if (!an || !bn) continue;
    ctx.strokeStyle =
      l.kind === "meta" ? `${ACCENT}0.30)` : l.kind === "member" ? `${VIOLET}0.55)` : `${VIOLET}0.16)`;
    ctx.beginPath();
    ctx.moveTo(an.x ?? 0, an.y ?? 0);
    ctx.lineTo(bn.x ?? 0, bn.y ?? 0);
    ctx.stroke();
  }

  // nodes
  for (const n of nodes) {
    const isHover = s.hover === n;
    if (n.kind === "community") {
      const dim = term ? !matches(n) : false;
      ctx.fillStyle = dim ? `${ACCENT}0.07)` : `${ACCENT}${isHover ? 0.85 : 0.5})`;
      ctx.strokeStyle = dim ? `${ACCENT}0.2)` : `${ACCENT}0.9)`;
      ctx.lineWidth = 1 / t.k;
      ctx.beginPath();
      ctx.arc(n.x ?? 0, n.y ?? 0, n.r, 0, Math.PI * 2);
      ctx.fill();
      ctx.stroke();
      // label at readable zoom levels
      if (t.k > 0.9 && !dim) {
        ctx.fillStyle = `${ACCENT}0.75)`;
        ctx.font = '10px "JetBrains Mono", ui-monospace, monospace';
        ctx.textAlign = "center";
        ctx.fillText(n.name, n.x ?? 0, (n.y ?? 0) + n.r + 11 / t.k);
      }
    } else {
      const selected = n.id === s.selectedEntityId;
      ctx.fillStyle = selected ? VIOLET + "1)" : `${VIOLET}0.75)`;
      ctx.strokeStyle = WHITE + (selected ? "1)" : "0.8)");
      ctx.lineWidth = (selected ? 2 : 1) / t.k;
      ctx.beginPath();
      ctx.arc(n.x ?? 0, n.y ?? 0, n.r, 0, Math.PI * 2);
      ctx.fill();
      ctx.stroke();
      if (isHover || (t.k > 1.6 && selected)) {
        ctx.fillStyle = `${VIOLET}0.95)`;
        ctx.font = "10px monospace";
        ctx.textAlign = "center";
        ctx.fillText(n.name, n.x ?? 0, (n.y ?? 0) - n.r - 4 / t.k);
      }
    }
  }
}

/** Seed the transform so the whole meta-graph starts centered in the viewport. */
function fitInitial(
  t: RefObject<{ k: number; x: number; y: number }>,
  communities: CommunityMetaNode[],
  viewW: number,
  viewH: number,
) {
  const cur = t.current;
  if (!cur || !communities.length) return;
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  for (const c of communities) {
    minX = Math.min(minX, c.x ?? 0);
    maxX = Math.max(maxX, c.x ?? 0);
    minY = Math.min(minY, c.y ?? 0);
    maxY = Math.max(maxY, c.y ?? 0);
  }
  const w = Math.max(1, maxX - minX);
  const h = Math.max(1, maxY - minY);
  // world units are already pixel-ish (√memberCount radii) — fit with margin
  cur.k = clamp(Math.min((viewW - 80) / w, (viewH - 80) / h), 0.15, 2);
  cur.x = viewW / 2 - (minX + w / 2) * cur.k;
  cur.y = viewH / 2 - (minY + h / 2) * cur.k;
}

function clamp(v: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, v));
}
