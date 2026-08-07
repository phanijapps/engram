import { z } from "zod";

import type { NativeProviderTransport } from "@engram/node";
import type { McpServer } from "@modelcontextprotocol/server";

import { buildScope } from "../shared/config.js";
import { buildRetrievalRequest, buildWriteMemoryRequest } from "./requests.js";

const scopeSchema = z.object({
  tenant: z.string(),
  workspace: z.string().optional()
});

const textResult = (payload: unknown) => ({
  content: [{ type: "text" as const, text: JSON.stringify(payload) }]
});

// ---- Graph-edge flattening helpers (Tier-3 codegraph tools).
// `listRelationships` returns `[KnowledgeRelationship]`; each endpoint is an
// `EntityRef` (`{id, kind, name, aliases}` in camelCase). The codegraph
// algorithms operate on a flat `{subject, predicate, object}` edge list keyed by
// the endpoint's symbol name (falling back to its id).

interface RelationshipEndpoint {
  id?: string;
  name?: string;
  kind?: string;
  aliases?: string[];
}

function endpointSymbol(endpoint: unknown): string {
  if (!endpoint || typeof endpoint !== "object") return "";
  const e = endpoint as RelationshipEndpoint;
  return e.name ?? e.id ?? "";
}

/** Flattens a `listRelationships` result into a flat edge list for the
 *  codegraph algorithms. Edges missing a resolvable subject or object are
 *  dropped (a half-formed edge has no traversal meaning). */
function flattenEdges(relationships: unknown[]): Array<{
  subject: string;
  predicate: string;
  object: string;
}> {
  const edges: Array<{ subject: string; predicate: string; object: string }> = [];
  for (const rel of relationships) {
    const r = rel as { subject?: RelationshipEndpoint; predicate?: string; object?: RelationshipEndpoint };
    const subject = endpointSymbol(r.subject);
    const object = endpointSymbol(r.object);
    if (subject && object) {
      edges.push({ subject, object, predicate: r.predicate ?? "relates_to" });
    }
  }
  return edges;
}

function entityName(entity: unknown): string {
  const e = entity as { name?: string; id?: string } | null | undefined;
  return e?.name ?? e?.id ?? "";
}

function entityKind(entity: unknown): string {
  const e = entity as { kind?: string } | null | undefined;
  return e?.kind ?? "unknown";
}

/** Built-in defaults matching the Rust engram-mcp OntologyConfig::default() /
 *  TaxonomyConfig::default() — a generic 3-layer technology ontology + SKOS
 *  tree. Overridden by the `--ontology` / `--taxonomy` launch flags. */
const DEFAULT_ONTOLOGY = {
  layers: [
    { name: "technical", classes: ["Module", "Function", "Struct", "Trait", "Store", "Adapter", "Gateway", "Repository"] },
    { name: "domain", classes: ["Agent", "MemoryFact", "KnowledgeEntity", "Belief", "Procedure", "Goal", "Episode"] },
    { name: "business", classes: ["Workflow", "Task", "Stakeholder", "Capability", "Service", "Integration"] },
  ],
  predicates: { within: ["depends_on", "implements", "contains", "part_of", "uses"], across: ["realized_by", "describes", "governs", "distilled_from"] },
};
const DEFAULT_TAXONOMY = {
  name: "default",
  concepts: [
    { label: "System" },
    { label: "Code", broader: "System" },
    { label: "Knowledge", broader: "System" },
    { label: "Memory", broader: "Knowledge" },
    { label: "Beliefs", broader: "Knowledge" },
  ],
};

/** Registers the Module-2 tools (query + light mutation) on the MCP server,
 *  each backed by the held-provider facade. Handlers are thin translators:
 *  simple MCP input → full domain request (default requester/policy) → facade. */
export function registerTools(
  server: McpServer,
  transport: NativeProviderTransport,
  opts?: { ontology?: unknown; taxonomy?: unknown },
): void {
  server.registerTool(
    "recall",
    {
      description:
        "Unified recall (fused across lanes) over the engram knowledge + memory layer. Returns excerpted items with a total char budget (prevents oversized responses).",
      inputSchema: z.object({
        query: z.string(),
        scope: scopeSchema,
        limit: z.number().int().min(1).max(100).optional(),
      }),
    },
    async ({ query, scope, limit }) => {
      const result = await transport.recall(
        buildRetrievalRequest(query, buildScope(scope), limit ?? 10),
      );
      // Output bounding — mirrors the Rust stdio MCP (mcp/engram-mcp/src/tools.rs).
      // Prevents multi-million-token dumps + N-API IPC overflow.
      const ITEM_EXCERPT = 1000;
      const TOTAL_BUDGET = 20000;
      const items = (result as { items?: Array<{ content?: string }> }).items ?? [];
      let joined = "";
      let skipped = 0;
      for (const item of items) {
        const content = item.content ?? "";
        const excerpt = content.length <= ITEM_EXCERPT
          ? content
          : content.slice(0, ITEM_EXCERPT) + "\n... [truncated]";
        const wouldBe = joined.length + excerpt.length + 5;
        if (joined.length > 0 && wouldBe > TOTAL_BUDGET) {
          skipped++;
          continue;
        }
        if (joined) joined += "\n---\n";
        joined += excerpt;
      }
      if (skipped > 0) {
        joined += `\n... [budget reached: ${skipped} more items omitted]`;
      }
      return textResult(joined);
    },
  );

  server.registerTool(
    "write_memory",
    {
      description: "Write a memory (observation by default) into the project scope.",
      inputSchema: z.object({
        text: z.string(),
        scope: scopeSchema,
        kind: z
          .enum([
            "observation",
            "fact",
            "preference",
            "episode",
            "artifact",
            "relationship",
            "procedure"
          ])
          .optional()
      })
    },
    async ({ text, scope, kind }) => {
      const result = await transport.write(
        buildWriteMemoryRequest({
          text,
          scope: buildScope(scope),
          ...(kind !== undefined ? { kind } : {})
        })
      );
      return textResult(result);
    }
  );

  server.registerTool(
    "put_entity",
    {
      description: "Upsert a knowledge entity (full KnowledgeEntity JSON object).",
      inputSchema: z.object({ entity: z.record(z.string(), z.unknown()) })
    },
    async ({ entity }) => {
      const result = await transport.putEntity(entity);
      return textResult(result);
    }
  );

  server.registerTool(
    "put_relationship",
    {
      description: "Upsert a knowledge relationship (full KnowledgeRelationship JSON object).",
      inputSchema: z.object({ relationship: z.record(z.string(), z.unknown()) })
    },
    async ({ relationship }) => {
      const result = await transport.putRelationship(relationship);
      return textResult(result);
    }
  );

  server.registerTool(
    "belief_put",
    {
      description: "Upsert a belief (full Belief JSON object).",
      inputSchema: z.object({ belief: z.record(z.string(), z.unknown()) })
    },
    async ({ belief }) => {
      const result = await transport.beliefPut(belief);
      return textResult(result);
    }
  );

  server.registerTool(
    "forget",
    {
      description: "Forget (delete/redact/tombstone/archive) a memory — full ForgetRequest JSON.",
      inputSchema: z.object({ request: z.record(z.string(), z.unknown()) })
    },
    async ({ request }) => {
      const result = await transport.forget(request);
      return textResult(result);
    }
  );

  // ---- Friendly READ tools (P3.6) — the consumer surface for extracting info.

  server.registerTool(
    "graph_overview",
    {
      description:
        "Community-level graph overview: the top-N Louvain communities + inter-community meta-edges. Good first look at the graph shape. Returns { communities: [{label, memberCount}], edges: [{sourceLabel, targetLabel, weight}], totalCommunities }.",
      inputSchema: z.object({
        scope: scopeSchema,
        limit: z.number().int().min(1).max(2000).optional(),
      }),
    },
    async ({ scope, limit }) => {
      const result = await transport.communityOverview(buildScope(scope), limit);
      return textResult(result);
    },
  );

  server.registerTool(
    "list_memories",
    {
      description:
        "List memory facts (observations) in scope, keyset-paged. `after` is the opaque nextCursor from a prior page. Returns { items: [MemoryRecord], nextCursor }.",
      inputSchema: z.object({
        scope: scopeSchema,
        after: z.string().nullable().optional(),
        limit: z.number().int().min(1).max(500).optional(),
      }),
    },
    async ({ scope, after, limit }) => {
      const result = await transport.listMemoriesPaged(
        buildScope(scope),
        after ?? null,
        limit ?? 50,
      );
      return textResult(result);
    },
  );

  // ---- Maintenance tools (pi-mono LLM) — the agent surface for belief synthesis
  // + contradiction detection, plus the belief/contradiction read tools. The LLM
  // modules are imported LAZILY inside the handlers so listTools / recall never
  // load pi-mono (the server stays light; the LLM only loads on a maintenance
  // call). The LLM runs TS-side (Rust stays LLM-free).

  server.registerTool(
    "belief_list",
    {
      description:
        "List beliefs in scope (Rust-backed; all statuses). Returns [Belief, …].",
      inputSchema: z.object({ scope: scopeSchema }),
    },
    async ({ scope }) => {
      const result = await transport.listBeliefs(buildScope(scope));
      return textResult(result);
    },
  );

  server.registerTool(
    "contradiction_list",
    {
      description:
        "List contradiction review records in scope (Rust-backed; all statuses). Returns [Contradiction, …].",
      inputSchema: z.object({ scope: scopeSchema }),
    },
    async ({ scope }) => {
      const result = await transport.listContradictions(buildScope(scope));
      return textResult(result);
    },
  );

  server.registerTool(
    "maintenance_run",
    {
      description:
        "Run a maintenance op over scope. PRIVACY: reflect-llm / contradict-llm route the scope's memories/beliefs to a third-party LLM (Anthropic by default; set PI_PROVIDER=ollama for local) and incur token cost — only call with consent. Ops: reflect-llm (synthesize beliefs), contradict-llm (detect contradictions), consolidate (deterministic, no LLM). Default reflect-llm.",
      inputSchema: z.object({
        scope: scopeSchema,
        op: z.enum(["reflect-llm", "contradict-llm", "consolidate"]).optional(),
      }),
    },
    async ({ scope, op }) => {
      const theScope = buildScope(scope);
      const theOp = op ?? "reflect-llm";
      if (theOp === "consolidate") {
        return textResult(await transport.consolidate({ scope: theScope }));
      }
      const { createLlmProvider } = await import("../maintenance/llm.js");
      const llm = createLlmProvider();
      if (theOp === "contradict-llm") {
        const { contradictLlm } = await import("../maintenance/contradict.js");
        return textResult(await contradictLlm({ transport, scope: theScope, llm }));
      }
      const { reflectLlm } = await import("../maintenance/reflect.js");
      return textResult(await reflectLlm({ transport, scope: theScope, llm }));
    },
  );

  server.registerTool(
    "contradiction_detect",
    {
      description:
        "Detect semantic contradictions across a scope's beliefs via pi-mono. LLM-only in this slice (the rule-based same-subject pre-filter is deferred — see backlog). Routes belief text to a third-party LLM (Anthropic default; PI_PROVIDER=ollama for local). Returns { beliefsRead, contradictionsWritten, skipped } (emitted records; the store dedupes by canonical pair key).",
      inputSchema: z.object({ scope: scopeSchema }),
    },
    async ({ scope }) => {
      const { createLlmProvider } = await import("../maintenance/llm.js");
      const { contradictLlm } = await import("../maintenance/contradict.js");
      return textResult(
        await contradictLlm({
          transport,
          scope: buildScope(scope),
          llm: createLlmProvider(),
        }),
      );
    },
  );

  // ---- Ontology + taxonomy config (read-only) — the agent surface for
  // classification. Loaded from JSON at launch (--ontology/--taxonomy) or the
  // built-in defaults. Mirrors the Rust engram-mcp's ontology_read/taxonomy_read.

  const ontology = opts?.ontology ?? DEFAULT_ONTOLOGY;
  const taxonomy = opts?.taxonomy ?? DEFAULT_TAXONOMY;

  server.registerTool(
    "ontology_read",
    {
      description:
        "The active multi-layer ontology config: layers (name + classes) + predicates (within/across). Used by the distill skill for entity classification.",
      inputSchema: z.object({}),
    },
    async () => textResult(ontology),
  );

  server.registerTool(
    "taxonomy_read",
    {
      description:
        "The active taxonomy config: a SKOS-style concept tree (name + concepts with broader links). Used by the distill skill for concept classification.",
      inputSchema: z.object({}),
    },
    async () => textResult(taxonomy),
  );

  // ---- Parity tools (Tier 1) — the agent surface that mirrors the Rust stdio
  // engram-mcp. Thin translators over the existing transport methods.

  server.registerTool(
    "ping",
    {
      description: "Liveness probe. Always returns { pong: true }. No store access.",
      inputSchema: z.object({}),
    },
    async () => textResult({ pong: true, server: "engram-mcp-http" }),
  );

  server.registerTool(
    "capability_report",
    {
      description:
        "The serialized CapabilityReport for the open provider — which capabilities are supported vs. unsupported, and per-engine conformance.",
      inputSchema: z.object({}),
    },
    async () => textResult(await transport.capabilities()),
  );

  server.registerTool(
    "consolidate",
    {
      description:
        "Run a consolidation cycle (reflection + decay). A system requester is injected. Set dryRun to plan without mutating.",
      inputSchema: z.object({
        scope: scopeSchema,
        dryRun: z.boolean().optional(),
      }),
    },
    async ({ scope, dryRun }) => {
      const result = await transport.consolidate({
        scope: buildScope(scope),
        ...(dryRun !== undefined ? { dryRun } : {}),
      });
      return textResult(result);
    },
  );

  server.registerTool(
    "store_knowledge",
    {
      description:
        "Best-effort batch ingest of entities + relationships + chunks. Pass the full BatchIngestRequest JSON. Surfaces the BestEffort guarantee — no cross-store rollback.",
      inputSchema: z.object({ request: z.record(z.string(), z.unknown()) }),
    },
    async ({ request }) => {
      const result = await transport.batchIngest(request);
      return textResult(result);
    },
  );

  server.registerTool(
    "scan_repo",
    {
      description:
        "Treesitter-index a repository into the project scope (entities + call-edge relationships). Returns a ScanSummary { scanned, ingested, entities, relationships, … }.",
      inputSchema: z.object({
        path: z.string(),
        scope: scopeSchema,
        scanFilter: z.record(z.string(), z.unknown()).optional(),
      }),
    },
    async ({ path, scope, scanFilter }) => {
      const result = await transport.scan({
        path,
        scope: buildScope(scope),
        ...(scanFilter !== undefined ? { scanFilter } : {}),
      });
      return textResult(result);
    },
  );

  server.registerTool(
    "graph_neighbors",
    {
      description:
        "One-hop graph neighbors of a node within a graph + scope (does not cross scope boundaries). Returns [KnowledgeRelationship, …].",
      inputSchema: z.object({
        graphId: z.string(),
        nodeId: z.string(),
        scope: scopeSchema,
        limit: z.number().int().min(1).max(1000).optional(),
      }),
    },
    async ({ graphId, nodeId, scope, limit }) => {
      const result = await transport.graphNeighbors({
        graphId,
        nodeId,
        scope: buildScope(scope),
        ...(limit !== undefined ? { limit } : {}),
      });
      return textResult(result);
    },
  );

  server.registerTool(
    "graph_subgraph",
    {
      description:
        "Subgraph around a symbol: the BFS neighborhood (callers + callees) up to `depth` hops, built from the scope's relationships. Use for a local picture around one node.",
      inputSchema: z.object({
        scope: scopeSchema,
        target: z.string(),
        depth: z.number().int().min(1).max(10).optional(),
        cap: z.number().int().min(1).max(500).optional(),
      }),
    },
    async ({ scope, target, depth, cap }) => {
      const relationships = await transport.listRelationships(buildScope(scope));
      const edges = flattenEdges(relationships);
      const { symbolContextBFS } = await import("./codegraph.js");
      const result = symbolContextBFS(edges, target, depth ?? 2, cap ?? 50);
      return textResult({ target, ...result });
    },
  );

  server.registerTool(
    "resolve_entity",
    {
      description:
        "Resolve an entity by id, or fuzzy-match by name across scope. When `id` is given, returns the single KnowledgeEntity (or null). Otherwise returns the entities whose name contains `name` (case-insensitive).",
      inputSchema: z.object({
        scope: scopeSchema,
        id: z.string().optional(),
        name: z.string().optional(),
      }),
    },
    async ({ scope, id, name }) => {
      const theScope = buildScope(scope);
      if (id) {
        const result = await transport.getEntity(id, theScope);
        return textResult(result);
      }
      const entities = await transport.listEntities(theScope);
      const needle = (name ?? "").toLowerCase();
      const matches = needle
        ? entities.filter((e) => entityName(e).toLowerCase().includes(needle))
        : entities;
      return textResult(matches);
    },
  );

  // ---- Parity tools (Tier 2) — belief lifecycle, hierarchy navigation,
  // procedures. Each dispatches through a Rust-backed transport wrapper.

  server.registerTool(
    "belief_get",
    {
      description: "Get a single belief by id in scope (Rust-backed). Returns the Belief JSON or null.",
      inputSchema: z.object({ id: z.string(), scope: scopeSchema }),
    },
    async ({ id, scope }) => {
      const result = await transport.beliefGet({ id, scope: buildScope(scope) });
      return textResult(result);
    },
  );

  server.registerTool(
    "belief_retract",
    {
      description: "Retract (soft-delete) a belief by id in scope (Rust-backed). Returns the retracted belief.",
      inputSchema: z.object({ id: z.string(), scope: scopeSchema }),
    },
    async ({ id, scope }) => {
      const result = await transport.beliefRetract({ id, scope: buildScope(scope) });
      return textResult(result);
    },
  );

  server.registerTool(
    "belief_stale_list",
    {
      description: "List beliefs marked stale in scope (Rust-backed). Returns [Belief, …].",
      inputSchema: z.object({ scope: scopeSchema }),
    },
    async ({ scope }) => {
      const result = await transport.beliefStaleList(buildScope(scope));
      return textResult(result);
    },
  );

  server.registerTool(
    "hierarchy_path",
    {
      description:
        "Hierarchy navigation path for seed entity ids: the nodes, relations, and lowest-common-ancestor on the way up the hierarchy tree.",
      inputSchema: z.object({
        seeds: z.array(z.string()),
        scope: scopeSchema,
        maxLayer: z.number().int().min(0).optional(),
      }),
    },
    async ({ seeds, scope, maxLayer }) => {
      const result = await transport.hierarchyPath({
        seeds,
        scope: buildScope(scope),
        ...(maxLayer !== undefined ? { maxLayer } : {}),
      });
      return textResult(result);
    },
  );

  server.registerTool(
    "procedure_put",
    {
      description:
        "Upsert a replayable procedure (Layer 6 runbook) by id. Pass the full Procedure JSON. Returns the persisted Procedure.",
      inputSchema: z.object({ procedure: z.record(z.string(), z.unknown()) }),
    },
    async ({ procedure }) => {
      const result = await transport.procedureUpsert(procedure);
      return textResult(result);
    },
  );

  server.registerTool(
    "procedure_list",
    {
      description: "List procedures in scope (Layer 6). Returns [Procedure, …].",
      inputSchema: z.object({ scope: scopeSchema }),
    },
    async ({ scope }) => {
      const result = await transport.procedureList(buildScope(scope));
      return textResult(result);
    },
  );

  server.registerTool(
    "procedure_increment",
    {
      description:
        "Bump a procedure's success or failure counter by id. `outcome` defaults to success. Returns the updated Procedure.",
      inputSchema: z.object({
        id: z.string(),
        scope: scopeSchema,
        outcome: z.enum(["success", "failure"]).optional(),
      }),
    },
    async ({ id, scope, outcome }) => {
      const request = { id, scope: buildScope(scope) };
      const result =
        outcome === "failure"
          ? await transport.procedureIncrementFailure(request)
          : await transport.procedureIncrementSuccess(request);
      return textResult(result);
    },
  );

  // ---- Parity tools (Tier 3) — codegraph analyses over listRelationships.
  // The algorithms are imported LAZILY inside the handlers (mirrors the
  // maintenance-tool pattern) so listTools / recall never load the codegraph
  // module and the server stays light.

  server.registerTool(
    "search",
    {
      description:
        "Semantic search over the memory + knowledge layer (unified recall), plus exact entity-name matches from the knowledge graph. Returns { recall, exactEntityMatches }.",
      inputSchema: z.object({
        query: z.string(),
        scope: scopeSchema,
        limit: z.number().int().min(1).max(100).optional(),
      }),
    },
    async ({ query, scope, limit }) => {
      const theScope = buildScope(scope);
      const [recall, entities] = await Promise.all([
        transport.recall(buildRetrievalRequest(query, theScope, limit ?? 10)),
        transport.listEntities(theScope),
      ]);
      const exactEntityMatches = entities.filter((e) => entityName(e) === query);
      return textResult({ recall, exactEntityMatches });
    },
  );

  server.registerTool(
    "symbol_context",
    {
      description:
        "BFS around a code symbol: its callers (who points to it) and callees (what it points to), up to `depth` hops. Operates over the scope's call-edge relationships.",
      inputSchema: z.object({
        scope: scopeSchema,
        symbol: z.string(),
        depth: z.number().int().min(1).max(10).optional(),
        cap: z.number().int().min(1).max(500).optional(),
      }),
    },
    async ({ scope, symbol, depth, cap }) => {
      const relationships = await transport.listRelationships(buildScope(scope));
      const edges = flattenEdges(relationships);
      const { symbolContextBFS } = await import("./codegraph.js");
      const result = symbolContextBFS(edges, symbol, depth ?? 2, cap ?? 50);
      return textResult(result);
    },
  );

  server.registerTool(
    "change_impact",
    {
      description:
        "Reverse BFS — who depends on `target` (upstream callers), the change-impact / blast-radius of editing it. One row per caller with hop depth and the predicate reached via.",
      inputSchema: z.object({
        scope: scopeSchema,
        target: z.string(),
        depth: z.number().int().min(1).max(10).optional(),
        cap: z.number().int().min(1).max(500).optional(),
      }),
    },
    async ({ scope, target, depth, cap }) => {
      const relationships = await transport.listRelationships(buildScope(scope));
      const edges = flattenEdges(relationships);
      const { changeImpactBFS } = await import("./codegraph.js");
      const result = changeImpactBFS(edges, target, depth ?? 3, cap ?? 100);
      return textResult(result);
    },
  );

  server.registerTool(
    "code_health",
    {
      description:
        "Code-health snapshot: dead code (entities with zero incoming references) + the most central symbols (degree centrality). Operates over scope entities + relationships.",
      inputSchema: z.object({
        scope: scopeSchema,
        limit: z.number().int().min(1).max(500).optional(),
      }),
    },
    async ({ scope, limit }) => {
      const theScope = buildScope(scope);
      const [entities, relationships] = await Promise.all([
        transport.listEntities(theScope),
        transport.listRelationships(theScope),
      ]);
      const edges = flattenEdges(relationships);
      const namedEntities = entities.map((e) => ({
        name: entityName(e),
        kind: entityKind(e),
      }));
      const { deadCode, centralSymbols } = await import("./codegraph.js");
      const result = {
        deadCode: deadCode(edges, namedEntities),
        centralSymbols: centralSymbols(edges, limit ?? 25),
      };
      return textResult(result);
    },
  );

  server.registerTool(
    "architecture",
    {
      description:
        "Architecture overview: the most central symbols by degree centrality (in-degree + out-degree). The high-traffic hubs of the scope's call graph.",
      inputSchema: z.object({
        scope: scopeSchema,
        limit: z.number().int().min(1).max(500).optional(),
      }),
    },
    async ({ scope, limit }) => {
      const relationships = await transport.listRelationships(buildScope(scope));
      const edges = flattenEdges(relationships);
      const { centralSymbols } = await import("./codegraph.js");
      const result = centralSymbols(edges, limit ?? 25);
      return textResult(result);
    },
  );

  server.registerTool(
    "get_context",
    {
      description:
        "Assemble a single context packet for a query: [Recall] memory/knowledge excerpts, [Code] symbol_context around an optional symbol, [Graph] the central symbols. The unified read surface for grounding an agent.",
      inputSchema: z.object({
        query: z.string(),
        scope: scopeSchema,
        symbol: z.string().optional(),
        limit: z.number().int().min(1).max(100).optional(),
      }),
    },
    async ({ query, scope, symbol, limit }) => {
      const theScope = buildScope(scope);
      const [recall, relationships] = await Promise.all([
        transport.recall(buildRetrievalRequest(query, theScope, limit ?? 10)),
        transport.listRelationships(theScope),
      ]);
      const { symbolContextBFS, centralSymbols } = await import("./codegraph.js");
      const edges = flattenEdges(relationships);

      const sections: string[] = [];

      const items = (recall as { items?: Array<{ content?: string }> }).items ?? [];
      const ITEM_EXCERPT = 1000;
      const RECALL_BUDGET = 10000;
      let recallText = "";
      let skipped = 0;
      for (const item of items) {
        const content = item.content ?? "";
        const excerpt =
          content.length <= ITEM_EXCERPT ? content : content.slice(0, ITEM_EXCERPT) + "… [truncated]";
        if (recallText && recallText.length + excerpt.length > RECALL_BUDGET) {
          skipped++;
          continue;
        }
        recallText += (recallText ? "\n---\n" : "") + excerpt;
      }
      if (skipped > 0) recallText += `\n… [budget reached: ${skipped} more items omitted]`;
      sections.push(`[Recall]\n${recallText || "(no items)"}`);

      if (symbol) {
        const ctx = symbolContextBFS(edges, symbol, 2, 50);
        const callers = ctx.callers.length ? ctx.callers.join(", ") : "(none)";
        const callees = ctx.callees.length ? ctx.callees.join(", ") : "(none)";
        sections.push(`[Code] symbol: ${symbol}\ncallers: ${callers}\ncallees: ${callees}`);
      }

      const hubs = centralSymbols(edges, 10)
        .map((s) => `${s.name} (${s.totalDegree})`)
        .join(", ");
      sections.push(`[Graph] hubs (degree): ${hubs || "(none)"}`);

      return textResult(sections.join("\n\n"));
    },
  );

  server.registerTool(
    "whats_changed",
    {
      description:
        "Temporal view: what changed recently in scope. NOT YET SUPPORTED in the TS HTTP MCP — needs temporal scoring (RFC-0012 codegraph/temporal), which is not yet wired. Registered for surface parity with the Rust stdio MCP.",
      inputSchema: z.object({ scope: scopeSchema }),
    },
    async () =>
      textResult({
        supported: false,
        reason:
          "whats_changed requires temporal scoring (RFC-0012 codegraph/temporal layer), not yet wired in the TS HTTP MCP",
      }),
  );
}
