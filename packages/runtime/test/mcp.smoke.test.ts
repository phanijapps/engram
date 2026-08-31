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
    const client = new Client(
      { name: "smoke", version: "1.0.0" },
      { versionNegotiation: { mode: "auto" } }
    );
    try {
      await client.connect(
        new StreamableHTTPClientTransport(new URL(`http://127.0.0.1:${port}/mcp`))
      );
      const { tools } = await client.listTools();
      // test-hygiene fix: the surface is 42 tools now (maintenance trio +
      // graph_health + explore/file_dependencies). The stale list previously
      // threw, skipped client.close(), and left server.close() waiting on the
      // open connection — surfacing as a confusing 15s TIMEOUT instead of the
      // assertion failure.
      expect(tools.map((t) => t.name).sort()).toEqual([
        "apply_maintenance_plan",
        "architecture",
        "belief_get",
        "belief_list",
        "belief_put",
        "belief_retract",
        "belief_stale_list",
        "build_maintenance_plan",
        "capability_report",
        "change_impact",
        "code_health",
        "consolidate",
        "contradiction_detect",
        "contradiction_list",
        "explore",
        "file_dependencies",
        "forget",
        "get_context",
        "graph_health",
        "graph_neighbors",
        "graph_overview",
        "graph_subgraph",
        "hierarchy_path",
        "list_maintenance_candidates",
        "list_memories",
        "maintenance_run",
        "ontology_read",
        "ping",
        "procedure_increment",
        "procedure_list",
        "procedure_put",
        "put_entity",
        "put_relationship",
        "recall",
        "resolve_entity",
        "scan_repo",
        "search",
        "store_knowledge",
        "symbol_context",
        "taxonomy_read",
        "whats_changed",
        "write_memory",
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
      // test-hygiene fix: close the client defensively (an assertion throw
      // used to skip it, leaving server.close() waiting on the open
      // connection forever — a 15s timeout masking the real failure) and
      // bound the server-close wait so no failure mode hangs the suite.
      try {
        await client.close();
      } catch {
        /* already closed */
      }
      await new Promise<void>((r) => {
        const bail = setTimeout(r, 2000);
        server.close(() => {
          clearTimeout(bail);
          r();
        });
      });
    }
  }, 15000);
});
