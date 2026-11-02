/**
 * TASK-013 focused proofs (GATE-008, AC-009): the anomaly panel surfaces the
 * minimal documented set (same subject under multiple partitions; same entity
 * identity under multiple partitions) from engine reads, offers drill-down to
 * the affected partitions, and offers NO repair, prune, or enforcement action
 * (W-001 / LED-003 negative).
 */
import { describe, expect, it, afterEach } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { AnomaliesView, AnomaliesList, AnomalyCard, partitionLabel } from "../src/views/Anomalies";
import { createConsoleServer } from "../server/app.mjs";
import type { Server } from "node:http";

/** Fixture store: subject 'agent-1' in two partitions; entity 'Alpha' in two. */
const PARTITIONS = [
  { scope: { tenant: "acme", subject: "agent-1", workspace: "proj-a", session: null, environment: null }, memoryCount: 1, entityCount: 1, beliefCount: 0 },
  { scope: { tenant: "acme", subject: "agent-1", workspace: "proj-b", session: null, environment: null }, memoryCount: 1, entityCount: 1, beliefCount: 0 },
  { scope: { tenant: "acme", subject: "agent-2", workspace: "proj-a", session: null, environment: null }, memoryCount: 1, entityCount: 1, beliefCount: 0 }
];

function provider() {
  return {
    capabilitiesJson: () => "{}",
    requireScopeInventoryApi: () => ({
      inventoryJson: () => JSON.stringify(PARTITIONS),
      partitionMemoriesJson: () => JSON.stringify({ items: [], nextOffset: null }),
      partitionEntitiesJson: (requestJson: string) => {
        const { partition } = JSON.parse(requestJson);
        // 'Alpha' exists under both agent-1 partitions; the agent-2 partition
        // holds 'Gamma' only — the engine-side truth the detector composes.
        const entities =
          partition.subject === "agent-1"
            ? [{ id: `e-alpha-${partition.workspace}`, kind: "project", name: "Alpha" }]
            : [{ id: "e-gamma", kind: "concept", name: "Gamma" }];
        return JSON.stringify({ items: entities, nextOffset: null });
      },
      partitionRelationshipsJson: () => JSON.stringify({ items: [], nextOffset: null }),
      partitionRecentMemoriesJson: () => "[]"
    })
  };
}

const servers: Server[] = [];
afterEach(() => {
  while (servers.length > 0) servers.pop()?.close();
});

describe("job012 anomaly panel (STEP-011)", () => {
  it("anomalies: same subject under multiple partitions is detected", async () => {
    const app = createConsoleServer({ provider: provider(), backend: "sqlite", storage: "/tmp/x" });
    const server = await app.listen(0, "127.0.0.1");
    servers.push(server);
    const address = server.address();
    if (typeof address === "string" || address === null) throw new Error("tcp expected");
    const body = await (await fetch(`http://127.0.0.1:${address.port}/api/anomalies`)).json();
    expect(body.state).toBe("ready");
    const subjectAnomaly = body.anomalies.find((a: { kind: string }) => a.kind === "subject-partitions");
    expect(subjectAnomaly).toBeDefined();
    expect(subjectAnomaly.partitionCount).toBe(2);
    expect(subjectAnomaly.partitions.map((p: { workspace: string | null }) => p.workspace).sort()).toEqual([
      "proj-a",
      "proj-b"
    ]);
  });

  it("anomalies: same entity identity under multiple partitions is detected", async () => {
    const app = createConsoleServer({ provider: provider(), backend: "sqlite", storage: "/tmp/x" });
    const server = await app.listen(0, "127.0.0.1");
    servers.push(server);
    const address = server.address();
    if (typeof address === "string" || address === null) throw new Error("tcp expected");
    const body = await (await fetch(`http://127.0.0.1:${address.port}/api/anomalies`)).json();
    const entityAnomaly = body.anomalies.find((a: { kind: string }) => a.kind === "entity-identity");
    expect(entityAnomaly).toBeDefined();
    expect(entityAnomaly.title).toContain("Alpha");
    expect(entityAnomaly.partitionCount).toBe(2);
  });

  it("anomalies: drill-down lists the affected partitions with labels", () => {
    const anomaly = {
      kind: "subject-partitions" as const,
      title: "Subject 'agent-1' appears in 2 partitions",
      partitionCount: 2,
      partitions: PARTITIONS.slice(0, 2).map((p) => p.scope)
    };
    const markup = renderToStaticMarkup(
      <AnomalyCard anomaly={anomaly} onOpenPartition={() => {}} />
    );
    expect(markup).toContain("agent-1");
    expect(markup).toContain(partitionLabel(anomaly.partitions[0]));
    expect(markup).toContain("proj-a");
    expect(markup).toContain("proj-b");
  });

  it("anomalies: NO repair, prune, or enforcement action is offered anywhere", async () => {
    const app = createConsoleServer({ provider: provider(), backend: "sqlite", storage: "/tmp/x" });
    const server = await app.listen(0, "127.0.0.1");
    servers.push(server);
    const address = server.address();
    if (typeof address === "string" || address === null) throw new Error("tcp expected");
    const base = `http://127.0.0.1:${address.port}`;

    // The UI vocabulary carries no action surface.
    const view = renderToStaticMarkup(
      <AnomaliesView
        onLoad={async () => ({ state: "ready", anomalies: [] })}
        onOpenPartition={() => {}}
      />
    );
    for (const forbidden of ["repair", "prune", "enforce", "delete", "apply"]) {
      expect(view.toLowerCase()).not.toContain(forbidden);
    }

    // The API carries no mutating anomaly route.
    for (const path of ["/api/anomalies/repair", "/api/anomalies/prune", "/api/anomalies/enforce"]) {
      const post = await fetch(base + path, { method: "POST", body: "{}" });
      expect(post.status).toBe(404);
    }
  });

  it("anomalies: clean store renders the no-anomalies state", () => {
    const markup = renderToStaticMarkup(
      <AnomaliesList response={{ state: "ready", anomalies: [] }} onOpenPartition={() => {}} />
    );
    expect(markup).toContain("No cross-partition anomalies detected");
  });
});
