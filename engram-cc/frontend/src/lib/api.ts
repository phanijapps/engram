//! BFF client — the browser's only data path to engram. All calls go to the
//! in-process Hono backend (Vite proxies /api → :3001); the browser never
//! speaks engram-mcp. Types mirror `contracts/openapi/engram-cc-bff.yaml`.

const BASE = "/api";

export interface Health {
  status: "ok" | "degraded";
  scope: { tenant: string; workspace: string };
  capabilities?: unknown;
  mcp?: "up" | "down";
}

export interface GraphStats {
  entities: number;
  relationships: number;
  communities: number;
  memories: number;
  beliefs: number;
  hierarchyNodes: number;
  hierarchyRelations: number;
}

export interface CommunityMetaNode {
  id: string;
  name: string;
  memberCount: number;
  x?: number;
  y?: number;
}

export interface CommunityMetaEdge {
  source: string;
  target: string;
  weight: number;
}

export interface CommunitiesResponse {
  communities: CommunityMetaNode[];
  edges: CommunityMetaEdge[];
  built: boolean;
  totalCommunities?: number;
}

export interface GraphEntityView {
  id: string;
  name: string;
  kind: string;
  graphId?: string;
}

export interface GraphRelationshipView {
  source: string;
  predicate: string;
  target: string;
}

export interface EntityDetail extends GraphEntityView {
  community: number | null;
  degree: number;
  provenance: unknown | null;
}

export interface Page<T> {
  items: T[];
  nextCursor: string | null;
}

export interface CommunityMembersPage {
  items: GraphEntityView[];
  edges: GraphRelationshipView[];
  nextCursor: string | null;
  memberCount: number;
  sampled: number;
  found: boolean;
}

export interface NeighborEntry {
  entity: { id: string; name?: string; kind?: string };
  relationship: { source: string; predicate: string; target: string };
  direction: "outgoing" | "incoming";
}

export interface MemoryView {
  id: string;
  kind: string;
  text: string;
  status?: string;
  createdAt?: string;
  source?: string;
  confidence?: number;
}

export interface BeliefView {
  id: string;
  text?: string;
  subject?: string;
  status?: string;
  confidence?: number;
}

export interface ProcedureView {
  id: string;
  text: string;
}

export interface ScanSummary {
  scanned?: number;
  ingested?: number;
  unchanged?: number;
  skipped?: number;
  entities?: number;
  relationships?: number;
  errors?: number;
  git_remote?: string | null;
  git_branch?: string | null;
  git_sha?: string | null;
}

export interface IngestJob {
  jobId: string;
  status: "running" | "done" | "error";
  startedAt?: number;
  summary?: ScanSummary | null;
  error?: string | null;
}

export interface IngestCounts {
  entities: number;
  relationships: number;
  memories: number;
  beliefs: number;
  hierarchyNodes: number;
  hierarchyRelations: number;
}

export type MaintainOp = "reflect-llm" | "contradict-llm" | "consolidate";

export interface MaintainResult {
  memoriesRead?: number;
  beliefsRead?: number;
  beliefsWritten?: number;
  contradictionsWritten?: number;
  skipped?: number;
  status?: string;
  [key: string]: unknown;
}

export interface MaintainJob {
  jobId: string;
  op: MaintainOp;
  status: "running" | "done" | "error";
  startedAt?: number;
  result?: MaintainResult | null;
  error?: string | null;
}

async function getJson<T>(path: string): Promise<T> {
  const res = await fetch(`${BASE}${path}`);
  if (!res.ok) throw new Error(`${path} → ${res.status}`);
  return (await res.json()) as T;
}

async function postJson<T>(path: string, body: unknown): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`${path} → ${res.status}`);
  return (await res.json()) as T;
}

/** Query-string for an optional keyset cursor + limit. */
function pageQs(cursor?: string | null, limit?: number): string {
  const p = new URLSearchParams();
  if (cursor) p.set("cursor", cursor);
  if (limit) p.set("limit", String(limit));
  const s = p.toString();
  return s ? `?${s}` : "";
}

export const api = {
  health: () => getJson<Health>("/health"),
  capabilities: () => getJson<unknown>("/capabilities"),
  stats: () => getJson<GraphStats>("/graph/stats"),
  communities: (limit?: number) =>
    getJson<CommunitiesResponse>(`/graph/communities${limit ? `?limit=${limit}` : ""}`),

  // Drill — entity ids contain slashes (e.g. "endpoint-post-/api/..."), so they
  // MUST be URL-encoded in the path or Hono's single-segment :id won't match.
  communityMembers: (communityId: string, cursor?: string | null, limit?: number) =>
    getJson<CommunityMembersPage>(
      `/graph/community/${encodeURIComponent(communityId)}/members${pageQs(cursor, limit)}`,
    ),
  entityDetail: (id: string) =>
    getJson<EntityDetail>(`/graph/entity/${encodeURIComponent(id)}`),
  neighbors: (id: string, cursor?: string | null, limit?: number) =>
    getJson<Page<NeighborEntry>>(
      `/graph/node/${encodeURIComponent(id)}/neighbors${pageQs(cursor, limit)}`,
    ),

  // Memory tab — keyset lists over the read-only secondary path.
  memory: (cursor?: string | null, limit?: number) =>
    getJson<Page<MemoryView>>(`/memory${pageQs(cursor, limit)}`),
  beliefs: (cursor?: string | null, limit?: number) =>
    getJson<Page<BeliefView>>(`/beliefs${pageQs(cursor, limit)}`),
  procedures: (cursor?: string | null, limit?: number) =>
    getJson<Page<ProcedureView>>(`/procedures${pageQs(cursor, limit)}`),
  contradictions: () => getJson<Page<unknown>>("/contradictions"),

  // Ingest tab — scan runs in a child process (the `engram-ingest` CLI) via the BFF.
  startScan: (root: string, kind: "code" | "doc" | "auto") =>
    postJson<{ jobId: string }>("/ingest/scan", { root, kind }),
  scanJob: (jobId: string) =>
    getJson<IngestJob>(`/ingest/jobs/${encodeURIComponent(jobId)}`),
  ingestCounts: () => getJson<IngestCounts>("/ingest/counts"),

  // Maintain tab — LLM maintenance runs in a child process via the BFF; beliefs +
  // contradictions read over the facade. reflect-llm/contradict-llm route scope
  // data to a cloud LLM (Anthropic default; PI_PROVIDER=ollama for local).
  runMaintain: (op: MaintainOp) => postJson<{ jobId: string }>("/maintain/run", { op }),
  maintainJob: (jobId: string) =>
    getJson<MaintainJob>(`/maintain/jobs/${encodeURIComponent(jobId)}`),
  maintainBeliefs: () => getJson<unknown[]>("/maintain/beliefs"),
  maintainContradictions: () => getJson<unknown[]>("/maintain/contradictions"),
};
