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
  isLoopbackHostHeader,
  isLoopbackOriginHeader,
  partitionKey,
  parsePartitionKey
} from "../server/app.mjs";
import { request as httpRequest, type Server } from "node:http";

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

  it("local-only: rebind-shape requests are refused (FIND-sec-001 negative)", async () => {
    const base = await startServer();
    const { port } = new URL(base);

    // DNS-rebinding shape: an external hostname resolving to 127.0.0.1. The
    // Host header is forbidden in fetch, so drive a raw request for it.
    const statusWithHost = (host: string) =>
      new Promise<number>((resolve, reject) => {
        const req = httpRequest(
          { host: "127.0.0.1", port: Number(port), path: "/api/inventory", headers: { Host: host } },
          (res) => {
            res.resume();
            resolve(res.statusCode ?? 0);
          }
        );
        req.on("error", reject);
        req.end();
      });

    expect(await statusWithHost("attacker.example:8788")).toBe(403);
    expect(await statusWithHost("127.0.0.1:" + port)).toBe(200);

    // Origin header (browsers send it on every cross-site fetch; fetch allows it).
    const reboundOrigin = await fetch(`${base}/api/inventory`, {
      headers: { Origin: "http://attacker.example" }
    });
    expect(reboundOrigin.status).toBe(403);
    expect((await reboundOrigin.json()).state).toBe("origin-refused");

    // The no-preflight CSRF shape against the shutdown route also dies at the gate.
    const crossOriginShutdown = await fetch(`${base}/api/shutdown`, {
      method: "POST",
      headers: { Origin: "http://attacker.example", "Content-Type": "text/plain" },
      body: "{}"
    });
    expect(crossOriginShutdown.status).toBe(403);

    // Legitimate loopback Origin still passes (negative's positive twin).
    const legit = await fetch(`${base}/api/inventory`, {
      headers: { Origin: new URL(base).origin }
    });
    expect(legit.status).toBe(200);
  });

  it("local-only: the header classifiers accept loopback and reject external names", () => {
    expect(isLoopbackHostHeader("127.0.0.1:8788")).toBe(true);
    expect(isLoopbackHostHeader("localhost:8788")).toBe(true);
    expect(isLoopbackHostHeader("[::1]:8788")).toBe(true);
    expect(isLoopbackHostHeader("attacker.example:8788")).toBe(false);
    expect(isLoopbackHostHeader(undefined)).toBe(false);
    expect(isLoopbackOriginHeader(undefined)).toBe(true);
    expect(isLoopbackOriginHeader("http://127.0.0.1:8788")).toBe(true);
    expect(isLoopbackOriginHeader("http://localhost:8788")).toBe(true);
    expect(isLoopbackOriginHeader("http://attacker.example")).toBe(false);
    expect(isLoopbackOriginHeader("not-a-url")).toBe(false);
  });

  it("local-only: GET on a directory path serves the SPA entry, never a crash (FIND-backend-001)", async () => {
    const { mkdtempSync, mkdirSync, writeFileSync } = await import("node:fs");
    const { tmpdir } = await import("node:os");
    const { join } = await import("node:path");
    const uiDir = mkdtempSync(join(tmpdir(), "job012-uidir-"));
    writeFileSync(join(uiDir, "index.html"), "<!doctype html><title>engram console</title>");
    mkdirSync(join(uiDir, "assets"));
    writeFileSync(join(uiDir, "assets", "app.js"), "console.log('app');");

    const app = createConsoleServer({
      provider: stubProvider(),
      backend: "sqlite",
      storage: "/tmp/engram-console-test",
      uiDir
    });
    const server = await app.listen(0, "127.0.0.1");
    servers.push(server);
    const address = server.address();
    if (typeof address === "string" || address === null) throw new Error("tcp expected");
    const base = `http://127.0.0.1:${address.port}`;

    // The exact repro from the finding: /assets exists as a directory.
    const directoryGet = await fetch(`${base}/assets`);
    expect(directoryGet.status).toBe(200);
    expect(directoryGet.headers.get("content-type")).toContain("text/html");
    expect(await directoryGet.text()).toContain("engram console");

    // The process must still be alive and serving afterwards.
    const after = await fetch(`${base}/api/health`);
    expect(after.status).toBe(200);

    // Regular files still serve with their own content type.
    const asset = await fetch(`${base}/assets/app.js`);
    expect(asset.status).toBe(200);
    expect(asset.headers.get("content-type")).toContain("text/javascript");
  });

  it("local-only: partition keys round-trip the exact tuple", () => {
    const scope = { tenant: "acme", subject: null, workspace: "proj", session: null, environment: null };
    const key = partitionKey(scope);
    expect(key).toMatch(/^[A-Za-z0-9_-]+$/);
    expect(parsePartitionKey(key)).toEqual(scope);
  });
});
