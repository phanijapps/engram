//! Typed REST client for the engram-viz backend. All paths are relative and
//! proxied to :3001 by Vite in dev.

import type {
  GraphResponse,
  InsightsResponse,
  NodeDetail,
  SearchResponse,
  StatsResponse,
  TimelineResponse,
} from "./types";

const BASE = "/api";

async function getJson<T>(path: string): Promise<T> {
  const res = await fetch(path);
  if (!res.ok) {
    throw new Error(`${res.status} ${res.statusText} on ${path}`);
  }
  return (await res.json()) as T;
}

export const api = {
  stats: () => getJson<StatsResponse>(`${BASE}/stats`),
  graph: () => getJson<GraphResponse>(`${BASE}/graph`),
  insights: (limit = 20) =>
    getJson<InsightsResponse>(`${BASE}/insights?limit=${limit}`),
  node: (id: string) =>
    getJson<NodeDetail>(`${BASE}/node/${encodeURIComponent(id)}`),
  search: (q: string, limit = 20) =>
    getJson<SearchResponse>(
      `${BASE}/search?q=${encodeURIComponent(q)}&limit=${limit}`,
    ),
  timeline: () => getJson<TimelineResponse>(`${BASE}/timeline`),
  scan: (path: string) =>
    fetch(`${BASE}/scan`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ path }),
    }).then((r) => r.json()),
};
