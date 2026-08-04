//! T6 — pure community projection logic (TDD). rankCommunities groups a
//! name→label map into ranked, bounded, deterministically-laid-out meta-nodes;
//! tallyMetaEdges tallies inter-community edges from name pairs. The integration
//! path (Louvain + streaming relationships) is smoke-tested against the live
//! store via smoke-agentzero.ts.

import { describe, it, expect } from "vitest";

import {
  MAX_COMMUNITY_EDGES,
  MAX_COMMUNITY_NODES,
  rankCommunities,
  tallyMetaEdges,
} from "../src/aggregation/communities.ts";

// name → label: a,b → 1; c,d,e → 2; f → 3.
const map = { a: 1, b: 1, c: 2, d: 2, e: 2, f: 3 };

describe("rankCommunities", () => {
  it("groups by label, ranks by membership, truncates to maxNodes", () => {
    const { nodes, topLabels } = rankCommunities(map, 2);
    expect(nodes).toHaveLength(2);
    // label 2 has 3 members (largest), label 1 has 2; label 3 (1) dropped.
    expect(nodes[0]).toMatchObject({ id: "c2", memberCount: 3 });
    expect(nodes[1]).toMatchObject({ id: "c1", memberCount: 2 });
    expect(topLabels).toEqual(new Set([2, 1]));
  });

  it("assigns a deterministic layout (no RNG) within the bounds", () => {
    const { nodes } = rankCommunities(map, MAX_COMMUNITY_NODES);
    expect(nodes.length).toBeLessThanOrEqual(MAX_COMMUNITY_NODES);
    for (const n of nodes) {
      expect(typeof n.x).toBe("number");
      expect(typeof n.y).toBe("number");
      expect(Number.isFinite(n.x)).toBe(true);
    }
    // Same input → same output (deterministic).
    expect(rankCommunities(map, MAX_COMMUNITY_NODES).nodes).toEqual(nodes);
  });

  it("handles an empty map", () => {
    expect(rankCommunities({}, 10).nodes).toEqual([]);
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
