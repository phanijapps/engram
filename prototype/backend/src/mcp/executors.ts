// The four engram MCP tool executors.
//
// Single source of truth for tool behavior — reused by the SDK registration
// (mcp-tools.ts) and, through it, the /mcp Streamable HTTP route (app.ts).
// Dependencies are injected so tests can pass stubs. See docs/specs/demo-reimagine.

import { basename, resolve } from "node:path";

export type ToolScope = { tenant: string; workspace: string; environment: string };

export type ToolDeps = {
  ingest: {
    startScanJob(input: Record<string, unknown>): Promise<unknown>;
    getScanJob(jobId: string): Promise<unknown>;
  };
  knowledge: {
    listEntities(scope: ToolScope): Promise<unknown>;
  };
  answerQuestion(question: string, scope: ToolScope): Promise<unknown>;
  scope: ToolScope;
  /** Non-null Policy the Rust ingest engine requires (mirrors the HTTP route). */
  policy: unknown;
  /** Actor recorded as the scan's provenance (mirrors the HTTP route). */
  actor: unknown;
  /** Optional scan-manifest sidecar path for incremental resume. */
  manifestPath?: string;
};

export async function indexRepo(
  deps: ToolDeps,
  args: { path: string; force?: boolean },
): Promise<unknown> {
  // Mirror the /ingest/jobs HTTP route's request shape — the ingest engine
  // requires a non-null policy (and expects actor + sourceName), which the MCP
  // path previously omitted, causing "invalid type: null, expected struct Policy".
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

export async function search(
  deps: ToolDeps,
  args: { query: string; limit?: number },
): Promise<unknown> {
  const entities = (await deps.knowledge.listEntities(deps.scope)) as Array<
    Record<string, unknown>
  >;
  const q = String(args.query ?? "").toLowerCase();
  const matched = entities
    .filter((e) => String(e.name ?? "").toLowerCase().includes(q))
    .slice(0, args.limit ?? 20)
    .map((e) => ({
      name: String(e.name ?? ""),
      kind: String(e.kind ?? ""),
      file: firstSourcePath(e),
    }));
  return { entities: matched, total: entities.length };
}

export async function agenticSearch(
  deps: ToolDeps,
  args: { question: string },
): Promise<unknown> {
  return deps.answerQuestion(String(args.question ?? ""), deps.scope);
}

function firstSourcePath(entity: Record<string, unknown>): string {
  const refs = entity.sourceRefs as Array<Record<string, unknown>> | undefined;
  const loc = refs?.[0]?.location as Record<string, unknown> | undefined;
  const path = loc?.path;
  return typeof path === "string" ? path : "";
}
