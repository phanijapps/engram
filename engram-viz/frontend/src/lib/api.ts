//! Typed REST client for the engram-viz backend. All paths are relative and
//! proxied to :3001 by Vite in dev.

import type {
  BlastRadiusResponse,
  GraphResponse,
  InsightsResponse,
  NodeDetail,
  OntologyResponse,
  PathResponse,
  SearchReadyResponse,
  SearchResponse,
  SourceListResponse,
  StatsResponse,
  TaxonomyResponse,
  TimelineResponse,
} from "./types";

const BASE = "/api";

// Concurrent identical GETs share one in-flight request. This primarily
// neutralizes React 18 StrictMode's dev double-invoke of mount effects
// (every fetch-bearing effect fires twice on load) so the backend isn't
// hit twice and the large /api/graph payload isn't parsed twice. The entry
// is cleared on settle, so a later genuine refetch (e.g. repo switch) still
// hits the network.
const inflight = new Map<string, Promise<unknown>>();

async function getJson<T>(path: string): Promise<T> {
  const existing = inflight.get(path);
  if (existing) return existing as Promise<T>;
  const p = fetch(path).then(async (res) => {
    if (!res.ok) {
      throw new Error(`${res.status} ${res.statusText} on ${path}`);
    }
    return (await res.json()) as T;
  });
  inflight.set(path, p);
  try {
    return await p;
  } finally {
    inflight.delete(path);
  }
}

export const api = {
  stats: () => getJson<StatsResponse>(`${BASE}/stats`),
  // `source` = stable_source_key of one repo, or undefined for all repos.
  graph: (source?: string | null) =>
    getJson<GraphResponse>(
      source
        ? `${BASE}/graph?source=${encodeURIComponent(source)}`
        : `${BASE}/graph`,
    ),
  sources: () => getJson<SourceListResponse>(`${BASE}/sources`),
  insights: (limit = 20) =>
    getJson<InsightsResponse>(`${BASE}/insights?limit=${limit}`),
  node: (id: string) =>
    getJson<NodeDetail>(`${BASE}/node/${encodeURIComponent(id)}`),
  search: (q: string, limit = 20) =>
    getJson<SearchResponse>(
      `${BASE}/search?q=${encodeURIComponent(q)}&limit=${limit}`,
    ),
  searchReady: () => getJson<SearchReadyResponse>(`${BASE}/search/ready`),
  timeline: () => getJson<TimelineResponse>(`${BASE}/timeline`),
  taxonomy: () => getJson<TaxonomyResponse>(`${BASE}/taxonomy`),
  ontology: () => getJson<OntologyResponse>(`${BASE}/ontology`),
  blastRadius: (id: string, depth = 5) =>
    getJson<BlastRadiusResponse>(
      `${BASE}/node/${encodeURIComponent(id)}/blast-radius?depth=${depth}`,
    ),
  path: (from: string, to: string) =>
    getJson<PathResponse>(
      `${BASE}/path?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`,
    ),
  scan: (path: string) =>
    fetch(`${BASE}/scan`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ path }),
    }).then((r) => r.json()),
};
