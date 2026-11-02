/**
 * TASK-017 focused proofs (GATE-012, AC-013): absent store directory, empty
 * store, locked store, and unreachable pgvector each render a NAMED explicit
 * state — never a crash, never a partial or invented inventory.
 */
import { describe, expect, it, afterEach } from "vitest";
import { execFileSync, spawnSync } from "node:child_process";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { createConsoleServer, classifyDegradedState } from "../server/app.mjs";
import { createServer as createServerNode } from "node:net";
import type { Server } from "node:http";

const MAIN = fileURLToPath(new URL("../server/main.mjs", import.meta.url));

function providerThrowing(message: string) {
  return {
    capabilitiesJson: () => "{}",
    codeStatusJson: () => "{}",
    requireScopeInventoryApi: () => ({
      inventoryJson: () => {
        throw new Error(message);
      },
      partitionMemoriesJson: () => {
        throw new Error(message);
      },
      partitionEntitiesJson: () => {
        throw new Error(message);
      },
      partitionRelationshipsJson: () => {
        throw new Error(message);
      },
      partitionRecentMemoriesJson: () => "[]"
    })
  };
}

function emptyProvider() {
  return {
    capabilitiesJson: () => "{}",
    codeStatusJson: () => "{}",
    requireScopeInventoryApi: () => ({
      inventoryJson: () => "[]",
      partitionMemoriesJson: () => JSON.stringify({ items: [], nextOffset: null }),
      partitionEntitiesJson: () => JSON.stringify({ items: [], nextOffset: null }),
      partitionRelationshipsJson: () => JSON.stringify({ items: [], nextOffset: null }),
      partitionRecentMemoriesJson: () => "[]"
    })
  };
}

const servers: Server[] = [];
afterEach(() => {
  while (servers.length > 0) servers.pop()?.close();
});

async function start(provider: ReturnType<typeof emptyProvider>) {
  const app = createConsoleServer({ provider, backend: "sqlite", storage: "/tmp/x" });
  const server = await app.listen(0, "127.0.0.1");
  servers.push(server);
  const address = server.address();
  if (typeof address === "string" || address === null) throw new Error("tcp expected");
  return `http://127.0.0.1:${address.port}`;
}

describe("job012 degraded states (STEP-012)", () => {
  it("degraded: absent store directory refuses startup with the named state", () => {
    const absent = join(mkdtempSync(join(tmpdir(), "job012-absent-")), "no-such-store");
    const result = spawnSync(process.execPath, [MAIN, "--storage", absent, "--port", "0"], {
      encoding: "utf8",
      timeout: 30_000
    });
    expect(result.status).toBe(3);
    expect(result.stderr).toContain("state: store-absent");
    expect(result.stderr).toContain("never creates a store");
  });

  it("degraded: empty store is the named empty state — ready, zero partitions, guidance upstream", async () => {
    const base = await start(emptyProvider());
    const body = await (await fetch(`${base}/api/inventory`)).json();
    expect(body.state).toBe("ready");
    expect(body.partitions).toEqual([]);
    expect(body.partitionCount).toBe(0);
  });

  it("degraded: locked store renders store-locked with no partial inventory", async () => {
    const base = await start(providerThrowing("database is locked (5): SQLITE_BUSY") as never);
    const body = await (await fetch(`${base}/api/inventory`)).json();
    expect(body.state).toBe("store-locked");
    expect(body.partitions).toEqual([]);
    expect(body.message).toContain("locked");
  });

  it("degraded: unreachable pgvector renders backend-unreachable with no partial inventory", async () => {
    const base = await start(
      providerThrowing("error connecting to server: Connection refused (os error 111)") as never
    );
    const body = await (await fetch(`${base}/api/inventory`)).json();
    expect(body.state).toBe("backend-unreachable");
    expect(body.partitions).toEqual([]);
  });

  it("degraded: port-in-use startup failure is a named state with a distinct exit (FIND-ops-005)", async () => {
    const blocker = await new Promise<import("node:net").Server>((resolve) => {
      const srv = createServerNode();
      srv.listen(0, "127.0.0.1", () => resolve(srv));
    });
    const { port } = blocker.address() as { port: number };
    const result = spawnSync(
      process.execPath,
      [MAIN, "--storage", "/tmp", "--backend", "sqlite", "--port", String(port)],
      { encoding: "utf8", timeout: 30_000 }
    );
    blocker.close();
    expect(result.status).toBe(4);
    expect(result.stderr).toContain("state: port-in-use");
    expect(result.stderr).toContain("--port");
  });

  it("degraded: the classifier covers every documented state name", () => {
    expect(classifyDegradedState("database is locked").state).toBe("store-locked");
    expect(classifyDegradedState("Connection refused to 127.0.0.1:5432").state).toBe("backend-unreachable");
    expect(classifyDegradedState("unable to open database file").state).toBe("store-invalid");
    expect(classifyDegradedState("configuration validation failed: bad").state).toBe("configuration-invalid");
    expect(classifyDegradedState("something else").state).toBe("engine-error");
  });
});
