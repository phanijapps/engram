// MCP tool executors for the stdio server.
//
// agenticSearch returns the raw evidence bundle (context + sources + graph
// slices) so the calling coding agent drives synthesis — no internal LLM.

import { basename, resolve } from "node:path";
import { getTransport, getBeliefTransport } from "./adapters/engram.client.js";
import { fetchGraph, buildEvidence } from "./services/evidence.service.js";
import type { MemoryItem, QaBelief } from "./services/qa.types.js";

export type ToolScope = { tenant: string; workspace: string; environment: string };

export type ToolDeps = {
  ingest: {
    startScanJob(input: Record<string, unknown>): Promise<unknown>;
    getScanJob(jobId: string): Promise<unknown>;
  };
  knowledge: {
    listEntities(scope: ToolScope): Promise<unknown>;
    listEntitiesBySource(stableSourceKey: string, scope: ToolScope): Promise<unknown>;
    listRelationshipsBySource(stableSourceKey: string, scope: ToolScope): Promise<unknown>;
  };
  scope: ToolScope;
  policy: unknown;
  actor: unknown;
  manifestPath?: string;
};

const QA_REQUESTER = {
  actor: { id: "actor-mcp", kind: "agent" as const, displayName: "MCP stdio" },
  roles: ["maintainer"],
  permissions: ["memory.retrieve"],
};

export async function indexRepo(
  deps: ToolDeps,
  args: { path: string; force?: boolean },
): Promise<unknown> {
  return deps.ingest.startScanJob({
    root: args.path,
    scope: deps.scope,
    policy: deps.policy,
    actor: deps.actor,
    sourceName: `scan:${basename(resolve(args.path))}`,
    maxBytes: 0,
    manifestPath: deps.manifestPath,
    force: args.force === true,
  });
}

export async function getJob(deps: ToolDeps, args: { jobId: string }): Promise<unknown> {
  return deps.ingest.getScanJob(args.jobId);
}

function normalizeSourceKey(source: string): string {
  return source.startsWith("scan:") ? source : `scan:${source}`;
}

export async function search(
  deps: ToolDeps,
  args: { query: string; limit?: number; source?: string },
): Promise<unknown> {
  let entities: Array<Record<string, unknown>>;
  if (args.source) {
    const key = normalizeSourceKey(args.source);
    entities = (await deps.knowledge.listEntitiesBySource(key, deps.scope)) as Array<Record<string, unknown>>;
  } else {
    entities = (await deps.knowledge.listEntities(deps.scope)) as Array<Record<string, unknown>>;
  }
  // Split into terms and OR-match against name + file path + source repo.
  const terms = String(args.query ?? "").toLowerCase().split(/\s+/).filter((t) => t.length > 1);
  const matched = entities
    .filter((e) => {
      const file = firstSourcePath(e);
      const src = String((e as Record<string, unknown>).provenance
        ? ((e as Record<string, unknown>).provenance as Record<string, unknown>)?.source ?? ""
        : "").toLowerCase();
      const hay = `${String(e.name ?? "")} ${file} ${src}`.toLowerCase();
      return terms.some((t) => hay.includes(t));
    })
    .slice(0, args.limit ?? 20)
    .map((e) => ({
      name: String(e.name ?? ""),
      kind: String(e.kind ?? ""),
      file: firstSourcePath(e),
    }));
  return { entities: matched, total: entities.length };
}

/**
 * Returns the grounded evidence bundle for a question — memory, beliefs,
 * knowledge-graph entities, relationships, and code chunks — so the calling
 * coding agent synthesizes the answer using its own LLM.
 */
export async function agenticSearch(
  deps: ToolDeps,
  args: { question: string; source?: string },
): Promise<unknown> {
  const question = String(args.question ?? "");

  const [memoryResponse, beliefs, graph] = await Promise.all([
    getTransport().retrieve({
      query: question,
      scope: deps.scope,
      requester: QA_REQUESTER,
      modes: ["keyword"],
      limit: 8,
      budget: { maxItems: 8, maxTokens: 2000 },
    }),
    getBeliefTransport().listBeliefs(deps.scope),
    fetchGraph(deps.scope, args.source),
  ]);

  const memories = ((memoryResponse as { items?: MemoryItem[] }).items ?? []);
  const { context, sources } = buildEvidence(
    question,
    memories,
    beliefs as QaBelief[],
    graph.entities,
    graph.relationships,
    graph.chunks,
  );

  return {
    question,
    context,
    sources,
    _debug: {
      graphSize: { entities: graph.entities.length, relationships: graph.relationships.length, chunks: graph.chunks.length },
      matchedSources: sources.length,
    },
  };
}

function firstSourcePath(entity: Record<string, unknown>): string {
  const refs = entity.sourceRefs as Array<Record<string, unknown>> | undefined;
  const loc = refs?.[0]?.location as Record<string, unknown> | undefined;
  const path = loc?.path;
  return typeof path === "string" ? path : "";
}
