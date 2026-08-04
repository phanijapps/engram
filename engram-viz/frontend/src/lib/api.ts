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

async function getJson<T>(path: string): Promise<T> {
  const res = await fetch(`${BASE}${path}`);
  if (!res.ok) throw new Error(`${path} → ${res.status}`);
  return (await res.json()) as T;
}

export const api = {
  health: () => getJson<Health>("/health"),
  capabilities: () => getJson<unknown>("/capabilities"),
  stats: () => getJson<GraphStats>("/graph/stats"),
  communities: (limit?: number) =>
    getJson<CommunitiesResponse>(`/graph/communities${limit ? `?limit=${limit}` : ""}`),
};
