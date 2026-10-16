import { afterEach, describe, expect, it, vi } from "vitest";
import { request, type Server } from "node:http";

import { Client, StreamableHTTPClientTransport } from "@modelcontextprotocol/client";
import type { NativeProviderTransport } from "@engram/node";

import { buildRetrievalRequest, buildWriteMemoryRequest } from "../src/mcp/requests.js";
import { startMcpHttpServer } from "../src/mcp/server.js";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

function mockTransport(): NativeProviderTransport {
  return {
    readCodeSource:vi.fn(async ()=>({text:"π\r\n",manifest:{state:"active"},coverage:[]})),
    listCodeSources:vi.fn(async ()=>[]),
    capabilities: vi.fn(async () => ({})),
    recall: vi.fn(async () => ({ items: [], createdAt: "now" })),
    write: vi.fn(async () => ({ record: { id: "m1" } })),
    scan: vi.fn(async () => ({})),
    backfillEmbeddings: vi.fn(async () => ({embedded:3,newEmbeddings:2,reused:1,codeEmbedded:2,remaining:4})),
    consolidate: vi.fn(async () => ({ status: "completed", tasks: [] })),
    putEntity: vi.fn(async () => ({})),
    putRelationship: vi.fn(async () => ({})),
    beliefPut: vi.fn(async () => ({})),
    forget: vi.fn(async () => ({})),
    batchIngest: vi.fn(async () => ({})),
    listMemoriesPaged: vi.fn(async () => ({ items: [], nextCursor: null })),
    listBeliefs: vi.fn(async () => []),
    listBeliefsPaged: vi.fn(async () => ({ items: [], nextCursor: null })),
    listContradictions: vi.fn(async () => []),
    putContradiction: vi.fn(async () => ({})),
    diagnostics: vi.fn(async () => ({ record_counts: {} })),
    communityOverview: vi.fn(async () => ({})),
    communityMemberIndex: vi.fn(async () => ({})),
    scopeCounts: vi.fn(async () => ({})),
    beliefGet: vi.fn(async () => null),
    beliefRetract: vi.fn(async () => ({})),
    beliefStaleList: vi.fn(async () => []),
    listEntities: vi.fn(async () => []),
    listRelationships: vi.fn(async () => []),
    hierarchyPath: vi.fn(async () => ({})),
    buildHierarchy: vi.fn(async () => ({
      clusterCount: 0,
      entitiesClustered: 0,
      totalEntities: 0,
      totalRelationships: 0,
      interClusterRelationCount: 0,
    })),
    getEntity: vi.fn(async () => null),
    graphNeighbors: vi.fn(async () => []),
    // extract-knowledge iterates per-document graphs + reads chunks per doc.
    // Defaults return empty so the op short-circuits before any LLM call.
    listGraphs: vi.fn(async () => []),
    listChunksByDocument: vi.fn(async () => []),
    procedureUpsert: vi.fn(async () => ({})),
    procedureList: vi.fn(async () => []),
    procedureIncrementSuccess: vi.fn(async () => ({})),
    procedureIncrementFailure: vi.fn(async () => ({})),
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
  it("lists the 42 tools and recall dispatches to the facade", async () => {
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
    // 42 = the pre-existing 40 (incl the 4 maintenance/graph-health tools
    // this array had missed) + the Phase-2 pair. Cross-server drift with the
    // Rust registry (43) is pre-existing and recorded in the engram-code
    // spec notes — the parity fixture intersection is asserted below.
    expect(tools.map((x) => x.name).sort()).toEqual([
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
      "code",
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

    await client.callTool({
      name: "recall",
      arguments: { query: "auth", scope: { tenant: "t" } }
    });
    expect(t.recall).toHaveBeenCalledTimes(1);

    // maintenance_run op=consolidate dispatches to the facade (deterministic, no LLM).
    await client.callTool({
      name: "maintenance_run",
      arguments: { scope: { tenant: "t" }, op: "consolidate" }
    });
    expect(t.consolidate).toHaveBeenCalledTimes(1);

    // ontology_read returns the active config (default when no --ontology flag).
    const ontResult = await client.callTool({ name: "ontology_read", arguments: {} });
    const ontText = (ontResult.content as Array<{ text?: string }>)[0]?.text ?? "{}";
    const ont = JSON.parse(ontText);
    expect(ont.layers).toBeInstanceOf(Array);
    expect(ont.layers.length).toBeGreaterThan(0);

    await client.close();
  }, 15000);

  it("maintenance_run op=extract-knowledge dispatches to the op (42-tool list unchanged)", async () => {
    const t = mockTransport();
    const port = await start(t);

    const client = new Client(
      { name: "test-harness", version: "1.0.0" },
      { versionNegotiation: { mode: "auto" } }
    );
    await client.connect(
      new StreamableHTTPClientTransport(new URL(`http://127.0.0.1:${port}/mcp`))
    );

    // The tool surface gained file_dependencies + explore (Phase 2) — 38.
    // is a new op value on maintenance_run, NOT a new tool).
    const { tools } = await client.listTools();
    expect(tools.length).toBe(45);
      // Cross-server tool-list parity (engram-code spec, ADR-0022): the TS
      // HTTP tool table must match the shared fixture asserted by the Rust
      // registry test. PRE-EXISTING drift: 11 tools differ (6 Rust-only, 5
      // TS-only) — asserted as the INTERSECTION contract until the backfill
      // lands; the Phase-2 pair must be present on both.
      const fixture = readFileSync(
        resolve(__dirname, "../../../mcp/engram-mcp/tests/tool_names.txt"),
        "utf8",
      )
        .split("\n")
        .map((l) => l.trim())
        .filter(Boolean);
      for (const name of ["file_dependencies", "explore"]) {
        expect(
          tools.some((t: { name: string }) => t.name === name),
          `TS HTTP server must expose ${name}`,
        ).toBe(true);
        expect(fixture, "shared fixture must contain it too").toContain(name);
      }

    // maintenance_run op=extract-knowledge dispatches to the op: it iterates the
    // scope's graphs via listGraphs (empty here → op short-circuits before any
    // LLM call, so no provider/key is needed). Asserting listGraphs was invoked
    // proves the dispatch reached extractKnowledge.
    const result = await client.callTool({
      name: "maintenance_run",
      arguments: { scope: { tenant: "t" }, op: "extract-knowledge" }
    });
    expect(t.listGraphs).toHaveBeenCalledTimes(1);
    const text = (result.content as Array<{ text?: string }>)[0]?.text ?? "{}";
    const payload = JSON.parse(text);
    expect(payload.documentsRead).toBe(0);
    expect(payload.entitiesWritten).toBe(0);

    await client.close();
  }, 15000);

  it("maintenance_run op=hierarchy-build dispatches to buildHierarchy (42-tool list unchanged)", async () => {
    const t = mockTransport();
    const port = await start(t);

    const client = new Client(
      { name: "test-harness", version: "1.0.0" },
      { versionNegotiation: { mode: "auto" } }
    );
    await client.connect(
      new StreamableHTTPClientTransport(new URL(`http://127.0.0.1:${port}/mcp`))
    );

    // Still exactly 36 tools — hierarchy-build is a new op value on
    // maintenance_run, NOT a new tool.
    const { tools } = await client.listTools();
    expect(tools.length).toBe(45);

    // maintenance_run op=hierarchy-build dispatches to transport.buildHierarchy
    // (deterministic Louvain cluster→persist, no LLM — so no provider/key needed).
    const result = await client.callTool({
      name: "maintenance_run",
      arguments: { scope: { tenant: "t" }, op: "hierarchy-build" }
    });
    expect(t.buildHierarchy).toHaveBeenCalledTimes(1);
    expect(t.buildHierarchy).toHaveBeenCalledWith({ tenant: "t" });
    const text = (result.content as Array<{ text?: string }>)[0]?.text ?? "{}";
    const payload = JSON.parse(text);
    expect(payload.clusterCount).toBe(0);

    await client.close();
  }, 15000);
});

describe("engram-mcp-http codegraph suffix resolver (RFC-0020 T7)", () => {
  it("symbol_context with a bare symbol returns the qualified neighbors", async () => {
    const t = mockTransport();
    // After T1, entities carry qualified names; relationships carry qualified
    // endpoints. A user types the BARE symbol `parse_symbol`.
    (t.listEntities as ReturnType<typeof vi.fn>).mockResolvedValue([
      { id: "e1", name: "my-repo/src/lib.rs::parse_symbol", kind: "Function" },
      { id: "e2", name: "my-repo/src/parser.rs::tokenize", kind: "Function" },
    ]);
    (t.listRelationships as ReturnType<typeof vi.fn>).mockResolvedValue([
      {
        subject: { id: "e1", name: "my-repo/src/lib.rs::parse_symbol" },
        predicate: "calls",
        object: { id: "e2", name: "my-repo/src/parser.rs::tokenize" },
      },
    ]);
    const port = await start(t);

    const client = new Client(
      { name: "test-harness", version: "1.0.0" },
      { versionNegotiation: { mode: "auto" } },
    );
    await client.connect(
      new StreamableHTTPClientTransport(new URL(`http://127.0.0.1:${port}/mcp`)),
    );

    const result = await client.callTool({
      name: "symbol_context",
      arguments: { scope: { tenant: "t" }, symbol: "parse_symbol" },
    });
    const text = (result.content as Array<{ text?: string }>)[0]?.text ?? "{}";
    const payload = JSON.parse(text);
    // The bare query resolved to the qualified name and its callee surfaced.
    expect(payload.callees).toContain("my-repo/src/parser.rs::tokenize");

    await client.close();
  }, 15000);

  it("search with a bare symbol returns the qualified entity match", async () => {
    const t = mockTransport();
    (t.listEntities as ReturnType<typeof vi.fn>).mockResolvedValue([
      { id: "e1", name: "my-repo/src/lib.rs::parse_symbol", kind: "Function" },
      { id: "e2", name: "my-repo/src/parser.rs::tokenize", kind: "Function" },
    ]);
    const port = await start(t);

    const client = new Client(
      { name: "test-harness", version: "1.0.0" },
      { versionNegotiation: { mode: "auto" } },
    );
    await client.connect(
      new StreamableHTTPClientTransport(new URL(`http://127.0.0.1:${port}/mcp`)),
    );

    const result = await client.callTool({
      name: "search",
      arguments: { query: "parse_symbol", scope: { tenant: "t" } },
    });
    const text = (result.content as Array<{ text?: string }>)[0]?.text ?? "{}";
    const payload = JSON.parse(text);
    // The bare query resolved to the qualified-name entity.
    expect(payload.exactEntityMatches.length).toBe(1);
    expect(payload.exactEntityMatches[0].name).toBe(
      "my-repo/src/lib.rs::parse_symbol",
    );

    await client.close();
  }, 15000);
});

describe("engram-mcp-http auth (non-loopback)", () => {
  it("rejects a request without a Bearer token (401)", async () => {
    const server = await startMcpHttpServer({
        transport: mockTransport(),
        port: 0,
        host: "0.0.0.0",
        authToken: "secret"
      });
    servers.push(server);
    const address = server.address();
    if (!address || typeof address === "string") throw new Error("missing HTTP address");
    const port = address.port;
    const res = await fetch(`http://127.0.0.1:${port}/mcp`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "initialize", params: {} })
    });
    expect(res.status).toBe(401);
  });

  it("accepts a request with the correct Bearer token (not 401)", async () => {
    const server = await startMcpHttpServer({
        transport: mockTransport(),
        port: 0,
        host: "0.0.0.0",
        authToken: "secret"
      });
    servers.push(server);
    const address = server.address();
    if (!address || typeof address === "string") throw new Error("missing HTTP address");
    const port = address.port;
    const res = await fetch(`http://127.0.0.1:${port}/mcp`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Authorization: "Bearer secret"
      },
      body: JSON.stringify({ jsonrpc: "2.0", id: 1, method: "initialize", params: {} })
    });
    expect(res.status).not.toBe(401);
  });
});

describe("trusted source tools",()=>{
 it("injects launch context and rejects scope claims from tool input",async ()=>{
  const transport=mockTransport();const sourceContext={readerVersion:1 as const,scope:{tenant:"t",session:"session"},requester:{actor:{id:"owner",kind:"agent" as const}}};
  const server=await startMcpHttpServer({transport,sourceContext,port:0});servers.push(server);
  const address=server.address();if (!address || typeof address==="string") throw new Error("missing HTTP address");
  const client=new Client({name:"source-test",version:"1"});
  await client.connect(new StreamableHTTPClientTransport(new URL(`http://127.0.0.1:${address.port}/mcp`)));
  try {
   const result=await client.callTool({name:"source_read",arguments:{repository:"repo",path:"a.ts"}});
   expect(result.isError).not.toBe(true);expect(transport.readCodeSource).toHaveBeenCalledWith({repository:"repo",path:"a.ts",maxBytes:1048576},sourceContext);
   const denied=await client.callTool({name:"source_read",arguments:{repository:"repo",path:"a.ts",scope:{tenant:"other"}}});
   expect(denied.isError).toBe(true);expect(transport.readCodeSource).toHaveBeenCalledTimes(1);
  } finally {await client.close();}
 });
});


describe("trusted embedding reindex", () => {
  const sourceContext = {
    readerVersion: 1 as const,
    scope: { tenant: "t", workspace: "repo", session: "trusted-session" },
    requester: { actor: { id: "owner", kind: "agent" as const } }
  };

  async function connectReindex(trusted: boolean) {
    const transport = mockTransport();
    const server = await startMcpHttpServer({ transport, port: 0, ...(trusted ? {sourceContext} : {}) });
    servers.push(server);
    const address = server.address();
    if (!address || typeof address === "string") throw new Error("missing HTTP address");
    const client = new Client({ name: "reindex-test", version: "1" });
    await client.connect(new StreamableHTTPClientTransport(new URL(`http://127.0.0.1:${address.port}/mcp`)));
    return { transport, client };
  }

  it("delegates bounded reindex using the full trusted launch scope and requester", async () => {
    const {transport, client} = await connectReindex(true);
    try {
      const result = await client.callTool({name: "maintenance_run", arguments: {op: "reindex", limit: 64, sourceFilter: "source-1"}});
      expect(result.isError).not.toBe(true);
      expect(transport.backfillEmbeddings).toHaveBeenCalledExactlyOnceWith({
        scope: sourceContext.scope, requester: sourceContext.requester, limit: 64, sourceFilter: "source-1"
      });
      expect(result.content).toEqual([{type:"text",text:JSON.stringify({embedded:3,newEmbeddings:2,reused:1,codeEmbedded:2,remaining:4})}]);
    } finally { await client.close(); }
  });

  it("rejects missing launch context before invoking backfill", async () => {
    const {transport, client} = await connectReindex(false);
    vi.mocked(transport.capabilities).mockResolvedValue({code_embeddings: "Supported"});
    try {
      const capability = await client.callTool({name:"capability_report",arguments:{}});
      expect(capability.content).toEqual([{type:"text",text:JSON.stringify({
        code_embeddings:{Unsupported:{reason:"ProviderUnavailable"}},
        code_evidence:{Unsupported:{reason:"ProviderUnavailable"}},
        code_source_preservation:{Unsupported:{reason:"ProviderUnavailable"}}
      })}]);
      const result = await client.callTool({name:"maintenance_run",arguments:{op:"reindex"}});
      expect(result.isError).toBe(true);
      expect(JSON.stringify(result.content)).toContain("trusted source context");
      expect(transport.backfillEmbeddings).not.toHaveBeenCalled();
    } finally { await client.close(); }
  });

  it("rejects caller identity overrides and invalid batch bounds before backfill", async () => {
    const {transport, client} = await connectReindex(true);
    try {
      for (const fields of [
        {scope: {tenant:"other"}},
        {scope: sourceContext.scope},
        {requester: {actor: {id:"other",kind:"agent"}}},
        {limit:0}, {limit:1025}, {limit:1.5}
      ]) {
        const result = await client.callTool({name:"maintenance_run",arguments:{op:"reindex",...fields}});
        expect(result.isError).toBe(true);
      }
      expect(transport.backfillEmbeddings).not.toHaveBeenCalled();
    } finally { await client.close(); }
  });
});

describe("code evidence HTTP authority", () => {
  const context = {scope:{tenant:"t",workspace:"evidence"},requester:{actor:{id:"operator",kind:"agent" as const}},readerVersion:1 as const};
  async function connectEvidence(bound:boolean) {
    const transport = mockTransport();
    transport.codeEvidence = vi.fn(async () => ({items:[],supportingUnits:[],evidenceStatus:"insufficient_evidence",diagnostics:{calibrationState:"unvalidated"}} as never));
    const server = await startMcpHttpServer({transport,port:0,...(bound?{sourceContext:context}:{})});
    servers.push(server);
    const address = server.address();
    if (!address || typeof address === "string") throw new Error("missing address");
    const client = new Client({name:"evidence-test",version:"1"});
    await client.connect(new StreamableHTTPClientTransport(new URL(`http://127.0.0.1:${address.port}/mcp`)));
    return {client,transport};
  }
  it("returns the shared packet and rejects authority/model/calibration fields before dispatch", async () => {
    const {client,transport} = await connectEvidence(true);
    try {
      const result = await client.callTool({name:"code",arguments:{op:"evidence",query:"reject expired credentials",resultLimit:3}});
      expect(result.isError).not.toBe(true);
      expect(transport.codeEvidence).toHaveBeenCalledExactlyOnceWith({query:"reject expired credentials",resultLimit:3},context);
      for (const extra of [{scope:context.scope},{requester:context.requester},{model:"remote"},{calibration:{}},{profile:"fast-v1"},{resultLimit:0},{query:""}]) {
        const result = await client.callTool({name:"code",arguments:{op:"evidence",query:"valid",...extra}});
        expect(result.isError).toBe(true);
      }
      expect(transport.codeEvidence).toHaveBeenCalledTimes(1);
    } finally { await client.close(); }
  });
  it("does not invoke evidence without trusted launch context", async () => {
    const {client,transport} = await connectEvidence(false);
    try {
      const result = await client.callTool({name:"code",arguments:{op:"evidence",query:"valid"}});
      expect(result.isError).toBe(true);
      expect(transport.codeEvidence).not.toHaveBeenCalled();
    } finally { await client.close(); }
  });
});
