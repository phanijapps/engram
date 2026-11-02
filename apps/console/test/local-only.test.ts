/**
 * TASK-008 focused proofs (GATE-011, AC-011): the console starts without
 * credentials, serves a single trusted operator, and listens on loopback
 * ONLY — any non-loopback bind is a defect (REQ-007 negative). Uses the
 * server core with an injected provider stub; the real addon path is
 * exercised by the dogfood gate.
 */
import { describe, expect, it, afterEach } from "vitest";
import {
  createConsoleServer,
  assertLoopbackHost,
  partitionKey,
  parsePartitionKey
} from "../server/app.mjs";
import type { Server } from "node:http";

/** A `NativeProviderBinding`-shaped stub: the ONLY surface the server touches. */
function stubProvider() {
  const partition = { tenant: "acme", workspace: null };
  return {
    capabilitiesJson: () => '{"scope_inventory":{"supported":true}}',
    requireScopeInventoryApi: () => ({
      inventoryJson: () => JSON.stringify([{ scope: partition, memoryCount: 1, entityCount: 0, beliefCount: 0 }]),
      partitionMemoriesJson: () => JSON.stringify({ items: [], nextOffset: null }),
      partitionEntitiesJson: () => JSON.stringify({ items: [], nextOffset: null }),
      partitionRelationshipsJson: () => JSON.stringify({ items: [], nextOffset: null }),
      partitionRecentMemoriesJson: () => "[]"
    })
  };
}

const servers: Server[] = [];
async function startServer() {
  const app = createConsoleServer({
    provider: stubProvider(),
    backend: "sqlite",
    storage: "/tmp/engram-console-test"
  });
  const server = await app.listen(0, "127.0.0.1");
  servers.push(server);
  const address = server.address();
  if (typeof address === "string" || address === null) {
    throw new Error("expected a TCP address");
  }
  return `http://127.0.0.1:${address.port}`;
}

afterEach(() => {
  while (servers.length > 0) {
    servers.pop()?.close();
  }
});

describe("job012 console local-only surface (STEP-007)", () => {
  it("local-only: binds 127.0.0.1 and answers health + inventory without credentials", async () => {
    const base = await startServer();
    const health = await fetch(`${base}/api/health`);
    expect(health.status).toBe(200);
    expect(health.headers.get("www-authenticate")).toBeNull();
    const healthBody = await health.json();
    expect(healthBody.ok).toBe(true);
    expect(healthBody.backend).toBe("sqlite");

    const inventory = await fetch(`${base}/api/inventory`);
    const inventoryBody = await inventory.json();
    expect(inventoryBody.state).toBe("ready");
    expect(inventoryBody.partitions).toHaveLength(1);
    expect(inventoryBody.partitions[0].scope.workspace).toBeNull();
  });

  it("local-only: any non-loopback bind is refused (REQ-007 negative)", () => {
    expect(() => assertLoopbackHost("0.0.0.0")).toThrow(/non-loopback/);
    expect(() => assertLoopbackHost("::")).toThrow(/non-loopback/);
    expect(() => assertLoopbackHost("192.168.1.10")).toThrow(/non-loopback/);
    expect(() => assertLoopbackHost("example.internal")).toThrow(/non-loopback/);
    expect(() => assertLoopbackHost("127.0.0.1")).not.toThrow();
    expect(() => assertLoopbackHost("::1")).not.toThrow();
    expect(() => assertLoopbackHost("localhost")).not.toThrow();
  });

  it("local-only: the server itself refuses a non-loopback listen", async () => {
    const app = createConsoleServer({
      provider: stubProvider(),
      backend: "sqlite",
      storage: "/tmp/engram-console-test"
    });
    await expect(app.listen(0, "0.0.0.0")).rejects.toThrow(/non-loopback/);
  });

  it("local-only: no authentication or authorization surface exists anywhere", async () => {
    const base = await startServer();
    for (const path of ["/api/health", "/api/inventory", "/"]) {
      const response = await fetch(base + path);
      expect(response.status).not.toBe(401);
      expect(response.status).not.toBe(403);
      expect(response.headers.get("www-authenticate")).toBeNull();
    }
  });

  it("local-only: partition keys round-trip the exact tuple", () => {
    const scope = { tenant: "acme", subject: null, workspace: "proj", session: null, environment: null };
    const key = partitionKey(scope);
    expect(key).toMatch(/^[A-Za-z0-9_-]+$/);
    expect(parsePartitionKey(key)).toEqual(scope);
  });
});
