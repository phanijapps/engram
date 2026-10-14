/* Smoke: starts the HTTP-MCP server with a REAL provider (the addon) + an MCP
 * client lists the tools — the real-addon end-to-end (T4 mocks the transport).
 * Skips when the build chain isn't ready. */
import { describe, it, expect } from "vitest";
import { existsSync, mkdtempSync, mkdirSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawn } from "node:child_process";

import { Client, StreamableHTTPClientTransport } from "@modelcontextprotocol/client";
import { createNativeProviderTransport } from "@engram/node";

import { startMcpHttpServer } from "../src/mcp/server.js";

const ADDON = fileURLToPath(new URL("../../node/engram_node.node", import.meta.url));
const NODE_DIST = fileURLToPath(new URL("../../node/dist/index.js", import.meta.url));
const ready = existsSync(ADDON) && existsSync(NODE_DIST);

describe.skipIf(!ready)("engram-mcp-http (real addon)", () => {
  it("reads source through the built CLI with a trusted context file", async () => {
    const base = mkdtempSync(join(tmpdir(), "engram-source-cli-"));
    const repo = join(base,"repo"); mkdirSync(repo);
    const text = "function checkout() { function nested() {} return 42; }\r\n";
    writeFileSync(join(repo,"a.ts"),text);
    const config = {storage_path:join(base,"store"),trusted_root:base,scope_policy:"Strict",enable_vector:false,
      embedding_provider:{provider_type:"none",model:"none",dimensions:384,prompt_profile:"query"},
      migration_mode:"Apply",capability_policy:"FailClosed"};
    const scope = {tenant:"cli",workspace:"source"};
    const transport = createNativeProviderTransport({configJson:JSON.stringify(config)});
    await transport.scan({path:repo,scope});
    const context = {readerVersion:1 as const,scope,requester:{actor:{id:"engram-node",kind:"agent" as const}}};
    const sources = await transport.listCodeSources(context);
    const configPath=join(base,"config.json"), contextPath=join(base,"context.json");
    writeFileSync(configPath,JSON.stringify(config)); writeFileSync(contextPath,JSON.stringify(context));
    const port=6200+Math.floor(Math.random()*1000);
    const child=spawn(process.execPath,[fileURLToPath(new URL("../dist/mcp/bin.js",import.meta.url)),"--config",configPath,"--source-context",contextPath,"--port",String(port)],{stdio:["ignore","ignore","pipe"]});
    const client=new Client({name:"source-cli-proof",version:"1"},{versionNegotiation:{mode:"auto"}});
    try {
      await new Promise<void>((resolve,reject)=>{
        const timer=setTimeout(()=>reject(new Error("CLI startup timed out")),10000);
        child.once("exit",code=>{clearTimeout(timer);reject(new Error(`CLI exited ${code}`));});
        child.stderr.on("data",data=>{if(String(data).includes("listening on")){clearTimeout(timer);resolve();}});
      });
      await client.connect(new StreamableHTTPClientTransport(new URL(`http://127.0.0.1:${port}/mcp`)));
      const result=await client.callTool({name:"source_read",arguments:{repository:sources[0]!.file.repository,path:"a.ts"}});
      expect(result.isError).not.toBe(true);
      expect(JSON.parse((result.content[0] as {text:string}).text).text).toBe(text);
    } finally {
      await client.close().catch(()=>{});
      child.kill("SIGTERM");
    }
  },15000);

  it("serves MCP + lists tools over a real provider", async () => {
    const store = mkdtempSync(join(tmpdir(), "engram-mcp-http-smoke-"));
    const configJson = JSON.stringify({
      storage_path: join(store, "db"),
      trusted_root: store,
      scope_policy: "Strict",
      enable_vector: false,
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
    const scope = {tenant:"http-smoke",workspace:"source-fixture"};
    const repo = join(store,"repo"); mkdirSync(repo);
    const sourceText = "\ufefffunction checkout() { const fee=1; function helper() {} return fee; }\r\n";
    writeFileSync(join(repo,"fixture.ts"), sourceText);
    await transport.scan({path:repo,scope});
    const sourceContext = {readerVersion:1 as const,scope,requester:{actor:{id:"engram-node",kind:"agent" as const}}};
    const sources = await transport.listCodeSources(sourceContext);
    const server = await startMcpHttpServer({ transport, port, sourceContext });
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
      "code_sources",
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
        "source_read",
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

      const source = await client.callTool({name:"source_read",arguments:{repository:sources[0]!.file.repository,path:"fixture.ts"}});
      expect(source.isError).not.toBe(true);
      const data = JSON.parse((source.content[0] as {text:string}).text) as {text:string;coverage:{role:string}[]};
      expect(data.text).toBe(sourceText);
      expect(data.coverage.some(c=>c.role==="symbol")).toBe(true);
      const denied = await client.callTool({name:"source_read",arguments:{repository:sources[0]!.file.repository,path:"fixture.ts",scope:{tenant:"other"}}});
      expect(denied.isError).toBe(true);
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

  // Live model QA is explicit: default tests must not require a cached model
  // or download one. Enable with ENGRAM_LIVE_FASTEMBED=1 and the local cache.
  it.skipIf(process.env.ENGRAM_LIVE_FASTEMBED !== "1")("backfills real code vectors through trusted HTTP maintenance", async () => {
    const base = mkdtempSync(join(tmpdir(), "engram-http-code-vectors-"));
    const repo = join(base,"repo");
    mkdirSync(repo);
    const text = "export function computeShippingPrice(weight: number) { return weight * 5; }\n";
    writeFileSync(join(repo,"shipping.ts"),text);
    const scope = {tenant:"http-code",workspace:"shipping"};
    const sourceContext = {readerVersion:1 as const,scope,requester:{actor:{id:"engram-node",kind:"agent" as const}}};
    const transport = createNativeProviderTransport({configJson:JSON.stringify({
      storage_path:join(base,"store"),trusted_root:base,scope_policy:"Strict",
      embedding_provider:{provider_type:"fastembed",model:"BAAI/bge-small-en-v1.5",dimensions:384,prompt_profile:"query"},
      enable_vector:true,migration_mode:"Apply",capability_policy:"FailClosed"
    })});
    await transport.scan({path:repo,scope});
    const server = await startMcpHttpServer({transport,sourceContext,port:0});
    const address = server.address();
    if (!address || typeof address === "string") throw new Error("missing HTTP address");
    const client = new Client({name:"http-code-vector-test",version:"1"});
    try {
      await client.connect(new StreamableHTTPClientTransport(new URL(`http://127.0.0.1:${address.port}/mcp`)));
      const result = await client.callTool({name:"maintenance_run",arguments:{op:"reindex",limit:64}});
      expect(result.isError).not.toBe(true);
      const report = JSON.parse((result.content[0] as {text:string}).text);
      expect(report.codeEmbedded).toBeGreaterThan(0);
      expect(report.remaining).toBe(0);
      const again = await client.callTool({name:"maintenance_run",arguments:{op:"reindex",limit:64}});
      expect(again.isError).not.toBe(true);
      expect(JSON.parse((again.content[0] as {text:string}).text)).toMatchObject({embedded:0,remaining:0});
      const recall = await client.callTool({name:"recall",arguments:{query:"calculate shipping cost from parcel weight",scope,limit:5}});
      expect(recall.isError).not.toBe(true);
      expect(JSON.parse((recall.content[0] as {text:string}).text)).toContain("computeShippingPrice");
      // The public HTTP recall output is excerpted text; the typed native
      // recall retains lane attribution for the same canonical code result.
      const typedRecall = await transport.recall({query:"calculate shipping cost from parcel weight",scope,requester:sourceContext.requester,modes:["semantic"],limit:5});
      expect(typedRecall.items.some(item => item.content.includes("computeShippingPrice") && item.fusionTrace?.source.split("+").includes("vector"))).toBe(true);
      console.log("[code-vectors] HTTP:", JSON.stringify({report,vectorHits:typedRecall.items.filter(item => item.fusionTrace?.source.split("+").includes("vector")).length}));
    } finally {
      await client.close().catch(() => {});
      await new Promise<void>(resolve => server.close(() => resolve()));
      rmSync(base,{recursive:true,force:true});
    }
  }, 60_000);

});
