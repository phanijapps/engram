/**
 * TASK-012 focused proofs (GATE-007, AC-008): the partition graph derives
 * its nodes and links from exactly the selected partition's engine reads —
 * links resolve only inside the partition's entity set (confinement), the
 * /graph endpoint issues BOTH engine listings scoped to the exact tuple, and
 * the empty partition renders a named state.
 */
import { describe, expect, it, afterEach } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { PartitionGraphView, toGraphData } from "../src/views/PartitionGraph";
import { createConsoleServer, partitionKey } from "../server/app.mjs";
import type { Server } from "node:http";

const PARTITION = { tenant: "acme", subject: null, workspace: "proj", session: null, environment: null };

const ENTITIES = [
  { id: "e1", kind: "project", name: "Alpha" },
  { id: "e2", kind: "concept", name: "Beta" },
  { id: "e3", kind: "concept", name: "Gamma" }
];
const RELATIONSHIPS = [
  { id: "r1", subject: { id: "e1" }, predicate: "relates-to", object: { id: "e2" } },
  // Dangling: references an entity NOT in this partition.
  { id: "r2", subject: { id: "e1" }, predicate: "imports", object: { id: "outside" } }
];

describe("job012 partition graph derivation (STEP-010)", () => {
  it("partition-graph: nodes come from the partition's entities", () => {
    const graph = toGraphData(ENTITIES, []);
    expect(graph.nodes.map((n) => n.id)).toEqual(["e1", "e2", "e3"]);
    expect(graph.links).toEqual([]);
  });

  it("partition-graph: links resolve only inside the partition (confinement)", () => {
    const graph = toGraphData(ENTITIES, RELATIONSHIPS);
    expect(graph.links).toHaveLength(1);
    expect(graph.links[0]).toEqual({ source: "e1", target: "e2", predicate: "relates-to" });
    const ids = new Set(graph.nodes.map((n) => n.id));
    for (const link of graph.links) {
      expect(ids.has(link.source)).toBe(true);
      expect(ids.has(link.target)).toBe(true);
    }
  });

  it("partition-graph: empty partition renders the named empty state", () => {
    const markup = renderToStaticMarkup(<PartitionGraphView scope={PARTITION} />);
    expect(markup).toContain("loading partition graph");
    const empty = toGraphData([], []);
    expect(empty).toEqual({ nodes: [], links: [] });
  });

  it("partition-graph: /graph issues both listings scoped to the exact tuple", async () => {
    const requested: { view: string; partition: unknown }[] = [];
    const provider = {
      capabilitiesJson: () => "{}",
      requireScopeInventoryApi: () => ({
        inventoryJson: () => "[]",
        partitionMemoriesJson: () => JSON.stringify({ items: [], nextOffset: null }),
        partitionEntitiesJson: (requestJson: string) => {
          const { partition } = JSON.parse(requestJson);
          requested.push({ view: "entities", partition });
          return JSON.stringify({ items: ENTITIES, nextOffset: null });
        },
        partitionRelationshipsJson: (requestJson: string) => {
          const { partition } = JSON.parse(requestJson);
          requested.push({ view: "relationships", partition });
          return JSON.stringify({ items: RELATIONSHIPS, nextOffset: null });
        },
        partitionRecentMemoriesJson: () => "[]"
      })
    };
    const app = createConsoleServer({ provider, backend: "sqlite", storage: "/tmp/x" });
    const server = await app.listen(0, "127.0.0.1");
    const address = server.address();
    if (typeof address === "string" || address === null) throw new Error("tcp address expected");
    const cleanup = () => server.close();
    try {
      const body = await (
        await fetch(`http://127.0.0.1:${address.port}/api/partitions/${partitionKey(PARTITION)}/graph?limit=500`)
      ).json();
      expect(body.entities).toHaveLength(3);
      expect(body.relationships).toHaveLength(2);
      expect(requested.map((r) => r.view).sort()).toEqual(["entities", "relationships"]);
      for (const request of requested) {
        expect(request.partition).toEqual(PARTITION);
      }
    } finally {
      cleanup();
    }
  });
});
