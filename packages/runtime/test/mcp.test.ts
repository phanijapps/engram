import { afterEach, describe, expect, it, vi } from "vitest";
import { request, type Server } from "node:http";

import { Client, StreamableHTTPClientTransport } from "@modelcontextprotocol/client";
import type { NativeProviderTransport } from "@engram/node";

import { buildRetrievalRequest, buildWriteMemoryRequest } from "../src/mcp/requests.js";
import { startMcpHttpServer } from "../src/mcp/server.js";

function mockTransport(): NativeProviderTransport {
  return {
    capabilities: vi.fn(async () => ({})),
    recall: vi.fn(async () => ({ items: [], createdAt: "now" })),
    write: vi.fn(async () => ({ record: { id: "m1" } })),
    scan: vi.fn(async () => ({})),
    consolidate: vi.fn(async () => ({})),
    putEntity: vi.fn(async () => ({})),
    putRelationship: vi.fn(async () => ({})),
    beliefPut: vi.fn(async () => ({})),
    forget: vi.fn(async () => ({})),
    batchIngest: vi.fn(async () => ({}))
  } as unknown as NativeProviderTransport;
}

const servers: Server[] = [];
afterEach(async () => {
  while (servers.length) {
    const s = servers.pop()!;
    await new Promise<void>((r) => s.close(() => r()));
  }
});

async function start(transport: NativeProviderTransport): Promise<number> {
  const port = 4000 + Math.floor(Math.random() * 1000);
  servers.push(await startMcpHttpServer({ transport, port }));
  return port;
}

function postWithHost(port: number, host: string, body: string): Promise<number> {
  return new Promise((resolve) => {
    const req = request(
      {
        host: "127.0.0.1",
        port,
        path: "/mcp",
        method: "POST",
        headers: { "Content-Type": "application/json", Host: host }
      },
      (res) => {
        res.resume();
        resolve(res.statusCode ?? 0);
      }
    );
    req.end(body);
  });
}

describe("request builders", () => {
  it("buildRetrievalRequest injects a default requester", () => {
    const r = buildRetrievalRequest("auth", { tenant: "t" });
    expect(r.query).toBe("auth");
    expect(r.scope).toEqual({ tenant: "t" });
    expect(r.requester.actor.id).toBe("engram-mcp-http");
  });

  it("buildWriteMemoryRequest fills defaults (kind, policy, provenance)", () => {
    const r = buildWriteMemoryRequest({ text: "hi", scope: { tenant: "t" } });
    expect(r.content.text).toBe("hi");
    expect(r.kind).toBe("observation");
    expect(r.policy.visibility).toBe("workspace");
    expect(r.policy.retention).toBe("durable");
    expect(r.provenance.source).toBe("engram-mcp-http");
  });
});

describe("engram-mcp-http guard", () => {
  it("refuses a request with a non-local Host (non-2xx)", async () => {
    const port = await start(mockTransport());
    const status = await postWithHost(
      port,
      "evil.example.com",
      JSON.stringify({ jsonrpc: "2.0", id: 1, method: "initialize", params: {} })
    );
    expect(status).toBeGreaterThanOrEqual(400);
  });
});

describe("engram-mcp-http client (MCP protocol)", () => {
  it("lists the 6 tools and recall dispatches to the facade", async () => {
    const t = mockTransport();
    const port = await start(t);

    const client = new Client(
      { name: "test-harness", version: "1.0.0" },
      { versionNegotiation: { mode: "auto" } }
    );
    await client.connect(
      new StreamableHTTPClientTransport(new URL(`http://127.0.0.1:${port}/mcp`))
    );

    const { tools } = await client.listTools();
    expect(tools.map((x) => x.name).sort()).toEqual([
      "belief_put",
      "forget",
      "put_entity",
      "put_relationship",
      "recall",
      "write_memory"
    ]);

    await client.callTool({
      name: "recall",
      arguments: { query: "auth", scope: { tenant: "t" } }
    });
    expect(t.recall).toHaveBeenCalledTimes(1);

    await client.close();
  }, 15000);
});
