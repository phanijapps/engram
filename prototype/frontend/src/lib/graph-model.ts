// Pure graph-model helpers for the knowledge graph view.
//
// No React, no rendering — just the deterministic logic that turns
// /knowledge/graph-data into structural tiers, communities, and highlight sets.
// See docs/specs/demo-reimagine.

import Graph from "graphology";
import louvain from "graphology-communities-louvain";

export type GraphNode = {
  id: string;
  name: string;
  kind: string;
  degree?: number;
  sourcePath?: string;
};

export type GraphEdge = { subject: string; predicate: string; object: string };

export type Tier = {
  /** 1 = module/file/repo (largest), 2 = class, 3 = method (smallest). */
  tier: 1 | 2 | 3;
  baseSize: number;
  alwaysLabel: boolean;
};

const TIER1 = new Set(["module", "file", "repository", "project", "organization"]);
const TIER2 = new Set(["class", "struct", "trait", "interface", "enum"]);
const TIER3 = new Set(["method", "function", "variable", "api"]);

/**
 * Maps an entity `kind` to a structural tier, base node size, and whether the
 * node is always labeled. Concept maps to the class tier but is not
 * always-labeled; any unrecognized kind falls back to the method tier.
 */
export function tierForKind(kind: string): Tier {
  const k = kind.toLowerCase();
  if (TIER1.has(k)) return { tier: 1, baseSize: 12, alwaysLabel: true };
  if (TIER2.has(k)) return { tier: 2, baseSize: 7, alwaysLabel: true };
  if (k === "concept") return { tier: 2, baseSize: 6, alwaysLabel: false };
  if (TIER3.has(k)) return { tier: 3, baseSize: 3, alwaysLabel: false };
  return { tier: 3, baseSize: 3, alwaysLabel: false };
}

// Deterministic seeded PRNG (mulberry32) so Louvain is reproducible across runs.
function mulberry32(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a |= 0;
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const CALL_WEIGHT = 1;
const CONTAINMENT_WEIGHT = 3;

/**
 * Derives containment edges from shared `sourcePath`: within each file, the
 * highest-tier node (module/class) anchors the rest (its methods), so a class
 * and its methods cluster together. Weighted higher than call/mention edges.
 */
export function deriveContainmentEdges(
  nodes: GraphNode[],
): Array<{ source: string; target: string; weight: number }> {
  const byPath = new Map<string, GraphNode[]>();
  for (const n of nodes) {
    const p = n.sourcePath?.trim();
    if (!p) continue;
    let arr = byPath.get(p);
    if (!arr) {
      arr = [];
      byPath.set(p, arr);
    }
    arr.push(n);
  }

  const edges: Array<{ source: string; target: string; weight: number }> = [];
  for (const group of byPath.values()) {
    if (group.length < 2) continue;
    const anchor = [...group].sort((a, b) => {
      const ta = tierForKind(a.kind).tier;
      const tb = tierForKind(b.kind).tier;
      if (ta !== tb) return ta - tb;
      const da = a.degree ?? 0;
      const db = b.degree ?? 0;
      if (da !== db) return db - da;
      return a.id < b.id ? -1 : 1;
    })[0];
    for (const n of group) {
      if (n.id === anchor.id) continue;
      edges.push({ source: anchor.id, target: n.id, weight: CONTAINMENT_WEIGHT });
    }
  }
  return edges;
}

/**
 * Assigns each node a community id via Louvain over call/mention edges plus
 * derived containment edges. Deterministic (seeded RNG).
 */
export function assignCommunities(nodes: GraphNode[], edges: GraphEdge[]): Map<string, number> {
  const g = new Graph({ type: "undirected", multi: false });
  for (const n of nodes) if (!g.hasNode(n.id)) g.addNode(n.id);

  const addWeighted = (s: string, t: string, w: number) => {
    if (s === t || !g.hasNode(s) || !g.hasNode(t)) return;
    if (g.hasEdge(s, t)) {
      const prev = g.getEdgeAttribute(s, t, "weight") as number;
      g.setEdgeAttribute(s, t, "weight", prev + w);
    } else {
      g.addEdge(s, t, { weight: w });
    }
  };

  for (const e of edges) addWeighted(e.subject, e.object, CALL_WEIGHT);
  for (const c of deriveContainmentEdges(nodes)) addWeighted(c.source, c.target, c.weight);

  if (g.order === 0) return new Map();

  const rng = mulberry32(0x5eed);
  const mapping = louvain(g, { getEdgeWeight: "weight", rng, resolution: 1 }) as Record<
    string,
    number
  >;
  return new Map(Object.entries(mapping));
}

/** The hovered node plus its direct neighbors. Pure adjacency lookup. */
export function highlightSet(edges: GraphEdge[], hoveredId: string): Set<string> {
  const set = new Set<string>([hoveredId]);
  for (const e of edges) {
    if (e.subject === hoveredId) set.add(e.object);
    if (e.object === hoveredId) set.add(e.subject);
  }
  return set;
}

const PALETTE = [
  "#6ea8ff",
  "#7bd88f",
  "#e0b34a",
  "#b07cff",
  "#ff8a65",
  "#4dd0e1",
  "#f06292",
  "#a0e0a0",
];

/** Stable color for a community id, wrapping around the palette. */
export function colorForCommunity(id: number): string {
  return PALETTE[((id % PALETTE.length) + PALETTE.length) % PALETTE.length];
}
