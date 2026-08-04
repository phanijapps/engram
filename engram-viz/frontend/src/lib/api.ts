//! BFF client — the browser's only data path to engram. All calls go to the
//! in-process Hono backend (Vite proxies /api → :3001); the browser never
//! speaks engram-mcp. Types mirror `contracts/openapi/engram-viz-bff.yaml`.

const BASE = "/api";

export interface Health {
  status: "ok" | "degraded";
  scope: { tenant: string; workspace: string };
  capabilities?: unknown;
}

export interface GraphStats {
  entities: number;
  relationships: number;
  communities: number;
  memories: number;
  beliefs: number;
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
}

export interface ProcedureView {
  id: string;
  text: string;
}

async function getJson<T>(path: string): Promise<T> {
  const res = await fetch(`${BASE}${path}`);
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
};
