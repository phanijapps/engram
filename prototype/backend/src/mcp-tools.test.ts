import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { InMemoryTransport } from "@modelcontextprotocol/sdk/inMemory.js";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { describe, expect, it } from "vitest";
import { registerEngramTools } from "./mcp/tools.js";
import type { ToolDeps } from "./mcp/executors.js";

function stubDeps(): ToolDeps {
  return {
    ingest: {
      startScanJob: async (i) => ({ jobId: "job-1", root: i.root, force: i.force }),
      getScanJob: async (id) => ({ jobId: id, status: "done" }),
    },
    knowledge: {
      listEntities: async () => [
        { name: "FooService", kind: "class", sourceRefs: [{ location: { path: "foo.rs" } }] },
        { name: "bar", kind: "function" },
      ],
    },
    answerQuestion: async (q) => ({ answer: `answer to: ${q}` }),
    scope: { tenant: "t", workspace: "w", environment: "test" },
    policy: { visibility: "workspace", retention: "durable" },
    actor: { id: "actor-demo", kind: "agent" },
  };
}

async function connect(deps: ToolDeps) {
  const server = new McpServer({ name: "engram", version: "0.0.0-test" });
  registerEngramTools(server, deps);
  const client = new Client({ name: "test-client", version: "0.0.0" });
  const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
  await Promise.all([client.connect(clientTransport), server.connect(serverTransport)]);
  return client;
}

describe("registerEngramTools", () => {
  it("completes initialize advertising tools capability", async () => {
    const client = await connect(stubDeps());
    expect(client.getServerCapabilities()?.tools).toBeDefined();
  });

  it("lists exactly the four engram tools", async () => {
    const client = await connect(stubDeps());
    const list = await client.listTools();
    expect(list.tools.map((t) => t.name).sort()).toEqual([
      "agentic_search",
      "get_job",
      "index_repo",
      "search",
    ]);
  });

  it("executes each tool and returns text content", async () => {
    const client = await connect(stubDeps());
    const calls: Array<{ name: string; arguments: Record<string, unknown> }> = [
      { name: "index_repo", arguments: { path: "/repo" } },
      { name: "get_job", arguments: { jobId: "job-1" } },
      { name: "search", arguments: { query: "foo" } },
      { name: "agentic_search", arguments: { question: "why?" } },
    ];
    for (const call of calls) {
      const res = (await client.callTool(call)) as {
        content: Array<{ type: string; text?: string }>;
      };
      expect(Array.isArray(res.content)).toBe(true);
      expect(res.content[0]?.type).toBe("text");
      expect(typeof res.content[0]?.text).toBe("string");
    }
  });

  it("search matches entities by name via the injected stub", async () => {
    const client = await connect(stubDeps());
    const res = (await client.callTool({ name: "search", arguments: { query: "foo" } })) as {
      content: Array<{ text: string }>;
    };
    const parsed = JSON.parse(res.content[0].text) as {
      entities: Array<{ name: string; file: string }>;
      total: number;
    };
    expect(parsed.total).toBe(2);
    expect(parsed.entities).toHaveLength(1);
    expect(parsed.entities[0]).toMatchObject({ name: "FooService", file: "foo.rs" });
  });
});
