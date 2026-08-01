/* Smoke: starts the HTTP-MCP server with a REAL provider (the addon) + an MCP
 * client lists the tools — the real-addon end-to-end (T4 mocks the transport).
 * Skips when the build chain isn't ready. */
import { describe, it, expect } from "vitest";
import { existsSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { Client, StreamableHTTPClientTransport } from "@modelcontextprotocol/client";
import { createNativeProviderTransport } from "@engram/node";

import { startMcpHttpServer } from "../src/mcp/server.js";

const ADDON = fileURLToPath(new URL("../../node/engram_node.node", import.meta.url));
const NODE_DIST = fileURLToPath(new URL("../../node/dist/index.js", import.meta.url));
const ready = existsSync(ADDON) && existsSync(NODE_DIST);

describe.skipIf(!ready)("engram-mcp-http (real addon)", () => {
  it("serves MCP + lists tools over a real provider", async () => {
    const store = mkdtempSync(join(tmpdir(), "engram-mcp-http-smoke-"));
    const configJson = JSON.stringify({
      storage_path: join(store, "db"),
      trusted_root: store,
      scope_policy: "Strict",
      embedding_provider: {
        provider_type: "none",
        model: "none",
        dimensions: 384,
        prompt_profile: "query"
      },
      migration_mode: "Apply",
      capability_policy: "FailClosed"
    });
    const port = 5000 + Math.floor(Math.random() * 1000);
    const transport = createNativeProviderTransport({ configJson });
    const server = await startMcpHttpServer({ transport, port });
    try {
      const client = new Client(
        { name: "smoke", version: "1.0.0" },
        { versionNegotiation: { mode: "auto" } }
      );
      await client.connect(
        new StreamableHTTPClientTransport(new URL(`http://127.0.0.1:${port}/mcp`))
      );
      const { tools } = await client.listTools();
      expect(tools.map((t) => t.name).sort()).toEqual([
        "belief_put",
        "forget",
        "put_entity",
        "put_relationship",
        "recall",
        "write_memory"
      ]);

      // tools/call against the REAL provider (recall → facade → binding → sqlite).
      const callResult = await client.callTool({
        name: "recall",
        arguments: { query: "anything", scope: { tenant: "t" } }
      });
      expect(callResult.content).toBeInstanceOf(Array);
      expect(callResult.content.length).toBeGreaterThan(0);

      await client.close();
    } finally {
      await new Promise<void>((r) => server.close(() => r()));
    }
  }, 15000);
});
