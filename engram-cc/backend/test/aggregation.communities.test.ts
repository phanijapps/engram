//! T6/T0 — pure community projection logic (TDD). rankCommunities groups a
//! name→label map into ranked, top-N meta-nodes (NO positions — layoutGraph
//! assigns them) + totalCommunities; tallyMetaEdges tallies inter-community
//! edges from name pairs; layoutGraph is the deterministic concentric-ring
//! layout. The integration path (Louvain + streaming relationships) is
//! smoke-tested against the live store via smoke-agentzero.ts.

import { describe, it, expect } from "vitest";

import {
  DEFAULT_COMMUNITY_LIMIT,
  MAX_COMMUNITY_EDGES,
  MAX_COMMUNITY_NODES,
  rankCommunities,
  tallyMetaEdges,
  layoutGraph,
} from "../src/aggregation/communities.ts";
import type { CommunityMetaEdge, CommunityMetaNode } from "../src/views/types.ts";

// name → label: a,b → 1; c,d,e → 2; f → 3.
const map = { a: 1, b: 1, c: 2, d: 2, e: 2, f: 3 };

describe("rankCommunities", () => {
  it("groups by label, ranks by membership, truncates to limit, reports totalCommunities", () => {
    const { nodes, topLabels, totalCommunities } = rankCommunities(map, 2);
    expect(nodes).toHaveLength(2);
    // label 2 has 3 members (largest), label 1 has 2; label 3 (1) dropped.
    expect(nodes[0]).toMatchObject({ id: "c2", memberCount: 3 });
    expect(nodes[1]).toMatchObject({ id: "c1", memberCount: 2 });
    expect(topLabels).toEqual(new Set([2, 1]));
    expect(totalCommunities).toBe(3); // pre-truncation
  });

  it("returns nodes WITHOUT positions (layoutGraph assigns x/y)", () => {
    const { nodes } = rankCommunities(map, MAX_COMMUNITY_NODES);
    for (const n of nodes) {
      expect(n.x).toBeUndefined();
      expect(n.y).toBeUndefined();
    }
  });

  it("defaults to the legible overview limit (150, not the 2000 cap)", () => {
    const big: Record<string, number> = {};
    for (let i = 0; i < 500; i++) big[`n${i}`] = i; // 500 singleton communities
    const { nodes, totalCommunities } = rankCommunities(big);
    expect(nodes.length).toBe(DEFAULT_COMMUNITY_LIMIT);
    expect(totalCommunities).toBe(500);
  });

  it("handles an empty map", () => {
    const { nodes, totalCommunities } = rankCommunities({}, 10);
    expect(nodes).toEqual([]);
    expect(totalCommunities).toBe(0);
  });
});

describe("tallyMetaEdges", () => {
  it("tallies undirected inter-community edges, skips same-community + out-of-top", () => {
    const pairs = [
      { subjectName: "a", objectName: "c" }, // 1 → 2
      { subjectName: "a", objectName: "c" }, // 1 → 2 (weight 2)
      { subjectName: "c", objectName: "e" }, // 2 → 2 (same, skip)
      { subjectName: "f", objectName: "a" }, // 3 → 1 (3 not in top {1,2}, skip)
    ];
    const edges = tallyMetaEdges(pairs, map, new Set([1, 2]));
    expect(edges).toEqual([{ source: "c1", target: "c2", weight: 2 }]);
  });

  it("ranks by weight and truncates to maxEdges", () => {
    const pairs = [
      { subjectName: "a", objectName: "c" },
      { subjectName: "a", objectName: "c" },
      { subjectName: "a", objectName: "c" }, // 1↔2 weight 3
      { subjectName: "b", objectName: "d" }, // 1↔2 weight +1 → 4
      { subjectName: "a", objectName: "e" }, // 1↔2 weight +1 → 5
    ];
    const edges = tallyMetaEdges(pairs, map, new Set([1, 2]), MAX_COMMUNITY_EDGES);
    // all collapse to one 1↔2 edge, weight 5
    expect(edges).toHaveLength(1);
    expect(edges[0].weight).toBe(5);
  });
});

describe("layoutGraph", () => {
  it("assigns finite positions to every node", () => {
    const { nodes } = rankCommunities(map, MAX_COMMUNITY_NODES);
    const edges: CommunityMetaEdge[] = [
      { source: "c1", target: "c2", weight: 2 },
    ];
    layoutGraph(nodes, edges);
    expect(nodes.length).toBe(3);
    for (const n of nodes) {
      expect(typeof n.x).toBe("number");
      expect(typeof n.y).toBe("number");
      expect(Number.isFinite(n.x)).toBe(true);
      expect(Number.isFinite(n.y)).toBe(true);
    }
  });

  it("is deterministic — identical input yields identical output", () => {
    const run = (): CommunityMetaNode[] => {
      const { nodes } = rankCommunities(map, MAX_COMMUNITY_NODES);
      layoutGraph(nodes, [{ source: "c1", target: "c2", weight: 2 }]);
      return nodes.map((n) => [n.x, n.y]);
    };
    expect(run()).toEqual(run());
  });

  it("places every node at a distinct position", () => {
    const big: Record<string, number> = {};
    for (let i = 0; i < 60; i++) {
      big[`a${i}`] = i;
      big[`b${i}`] = i; // 60 two-member communities
    }
    const { nodes } = rankCommunities(big, 60);
    layoutGraph(nodes, []);
    const pts = new Set(nodes.map((n) => `${n.x!.toFixed(2)},${n.y!.toFixed(2)}`));
    expect(pts.size).toBe(nodes.length);
  });

  it("places the connectivity-core (highest-degree node) innermost; rings spread", () => {
    // star: c0 connects to c1..c5 (hub); c6..c9 isolated.
    const g: Record<string, number> = {};
    for (let i = 0; i < 10; i++) g[`x${i}`] = i;
    const { nodes } = rankCommunities(g, 10);
    const edges: CommunityMetaEdge[] = [1, 2, 3, 4, 5].map((j) => ({
      source: "c0",
      target: `c${j}`,
      weight: 1,
    }));
    layoutGraph(nodes, edges);
    const rad = (id: string) => {
      const nd = nodes.find((x) => x.id === id)!;
      return Math.hypot(nd.x!, nd.y!);
    };
    // the hub is on the innermost ring — at least as central as every other node.
    const hub = rad("c0");
    for (const nd of nodes) {
      expect(hub).toBeLessThanOrEqual(Math.hypot(nd.x!, nd.y!));
    }
    // periphery exists (≥2 distinct ring radii).
    const radii = new Set(nodes.map((nd) => Math.round(Math.hypot(nd.x!, nd.y!))));
    expect(radii.size).toBeGreaterThanOrEqual(2);
  });

  it("is a no-op on an empty node set", () => {
    const nodes: CommunityMetaNode[] = [];
    layoutGraph(nodes, []);
    expect(nodes).toEqual([]);
  });

  it("handles a single node", () => {
    const nodes: CommunityMetaNode[] = [{ id: "c1", name: "Community 1", memberCount: 5 }];
    layoutGraph(nodes, []);
    expect(nodes[0].x).toBe(0);
    expect(nodes[0].y).toBe(0);
  });
});
