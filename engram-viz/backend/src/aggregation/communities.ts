//! Community-overview projection — the Graph tab's level-of-detail payload.
//!
//! `call_communities` (Louvain, on the standalone knowledge engine) returns a
//! flat `Record<entityName, communityLabel>`. This module projects that into:
//!   - the **top-N** communities by membership (default 150, hard-cap 2000) —
//!     an *overview* is legible only at a bounded, significant count, not all
//!     communities (which is a dense unreadable blob);
//!   - **inter-community meta-edges** (top by weight, bounded);
//!   - **deterministic concentric-ring positions** — see `layoutGraph`.
//!
//! Louvain + the relationship stream run once per store-mtime version; the
//! result is cached per (mtime, limit).

import { statSync } from "node:fs";
import { loadNativeBinding, type CommunityMemberIndex, type CommunityOverviewData } from "@engram/node";
import type { Scope } from "@engram/contracts";

import { dbPath, type VizConfig } from "../config.ts";
import { getProvider } from "../engram/provider.ts";
import { decodeCursor } from "../db/keyset.ts";
import { openReader, paginate } from "../db/reader.ts";
import type { CommunityMetaEdge, CommunityMetaNode } from "../views/types.ts";

export const MAX_COMMUNITY_NODES = 2000;
export const MAX_COMMUNITY_EDGES = 4000;
/** Default number of communities the overview renders. The 2000 hard cap is the
 * safety bound; an *overview* is legible only at a bounded, significant count. */
export const DEFAULT_COMMUNITY_LIMIT = 150;

/**
 * Pure: group names by label, rank by membership, take the top `limit`, and
 * return them **without** positions (`layoutGraph` assigns x/y). Also reports
 * `totalCommunities` (pre-truncation) so the legend can say "N of M".
 */
export function rankCommunities(
  nameToLabel: Record<string, number>,
  limit: number = DEFAULT_COMMUNITY_LIMIT,
): {
  nodes: CommunityMetaNode[];
  topLabels: Set<number>;
  totalCommunities: number;
} {
  const counts = new Map<number, number>();
  for (const label of Object.values(nameToLabel)) {
    counts.set(label, (counts.get(label) ?? 0) + 1);
  }
  const totalCommunities = counts.size;
  const ranked = [...counts.entries()]
    .sort((a, b) => b[1] - a[1])
    .slice(0, Math.max(0, limit));
  const nodes: CommunityMetaNode[] = ranked.map(([label, memberCount]) => ({
    id: `c${label}`,
    name: `Community ${label}`,
    memberCount,
  }));
  return {
    nodes,
    topLabels: new Set(ranked.map(([label]) => label)),
    totalCommunities,
  };
}

/**
 * Pure: tally inter-community edges from an iterable of name pairs. Edges within
 * the same community or outside `topLabels` are skipped; undirected, top by
 * weight. Accepts an iterable so the integration path can stream relationships.
 */
export function tallyMetaEdges(
  pairs: Iterable<{ subjectName: string; objectName: string }>,
  nameToLabel: Record<string, number>,
  topLabels: Set<number>,
  maxEdges: number = MAX_COMMUNITY_EDGES,
): CommunityMetaEdge[] {
  const weight = new Map<string, number>();
  for (const { subjectName, objectName } of pairs) {
    const ls = nameToLabel[subjectName];
    const lo = nameToLabel[objectName];
    if (ls === undefined || lo === undefined || ls === lo) continue;
    if (!topLabels.has(ls) || !topLabels.has(lo)) continue;
    const key = ls < lo ? `${ls}|${lo}` : `${lo}|${ls}`;
    weight.set(key, (weight.get(key) ?? 0) + 1);
  }
  return [...weight.entries()]
    .map(([k, w]) => {
      const [a, b] = k.split("|").map(Number);
      return { source: `c${a}`, target: `c${b}`, weight: w } satisfies CommunityMetaEdge;
    })
    .sort((a, b) => b.weight - a.weight)
    .slice(0, maxEdges);
}

/**
 * Deterministic **concentric-ring** layout. The inter-community meta-graph is
 * densely connected (a codebase's modules interlink into one big component), so
 * force-directed collapses it into an unreadable central blob — rings are
 * always legible (zero node overlap, organized, scale-free). Communities are
 * ordered by a breadth-first traversal of the meta-edges from the
 * most-connected node, so the connectivity core sits on the inner ring(s), the
 * periphery on the outer, and connected communities land adjacent → inter-
 * community edges read as short chords rather than a crisscross. Each ring is
 * golden-angle-twisted so rings don't align into spokes. No `Math.random`/`Date`
 * → reproducible across runs. Mutates `nodes` in place; no-op on empty.
 */
export function layoutGraph(
  nodes: CommunityMetaNode[],
  edges: CommunityMetaEdge[],
): void {
  const n = nodes.length;
  if (n === 0) return;
  if (n === 1) {
    nodes[0].x = 0;
    nodes[0].y = 0;
    return;
  }
  const idIndex = new Map<string, number>();
  nodes.forEach((nd, i) => idIndex.set(nd.id, i));

  // adjacency + degree over the meta-edges (undirected).
  const adj: number[][] = Array.from({ length: n }, () => []);
  const deg = new Array<number>(n).fill(0);
  for (const e of edges) {
    const a = idIndex.get(e.source);
    const b = idIndex.get(e.target);
    if (a !== undefined && b !== undefined && a !== b) {
      adj[a].push(b);
      adj[b].push(a);
      deg[a]++;
      deg[b]++;
    }
  }

  // BFS from the highest-degree node → connectivity-core-first ordering (ties +
  // isolated nodes fall to the periphery). Connected nodes land adjacent, so
  // edges become short chords.
  const order: number[] = [];
  const seen = new Uint8Array(n);
  let start = 0;
  for (let i = 1; i < n; i++) if (deg[i] > deg[start]) start = i;
  const queue: number[] = [start];
  seen[start] = 1;
  while (queue.length > 0) {
    const u = queue.shift()!;
    order.push(u);
    const ns = adj[u].slice().sort((x, y) => deg[y] - deg[x]);
    for (const v of ns) {
      if (!seen[v]) {
        seen[v] = 1;
        queue.push(v);
      }
    }
  }
  for (let i = 0; i < n; i++) if (!seen[i]) order.push(i); // isolates last

  // place slots onto concentric rings: slot k → ring floor(k / perRing).
  const perRing = Math.max(8, Math.round(Math.sqrt(n) * 2.2));
  const ringCount = Math.max(1, Math.ceil(n / perRing));
  const innerR = 70;
  const outerR = 380;
  const ringGap = ringCount > 1 ? (outerR - innerR) / (ringCount - 1) : 0;
  const twist = 2.39996323; // golden angle — offsets rings so they don't align into spokes.
  for (let k = 0; k < n; k++) {
    const ring = Math.min(ringCount - 1, Math.floor(k / perRing));
    const ringStart = ring * perRing;
    const inRing = Math.min(n, ringStart + perRing) - ringStart;
    const posInRing = k - ringStart;
    const angle = (posInRing / inRing) * Math.PI * 2 + ring * twist;
    const r = innerR + ring * ringGap;
    const nd = nodes[order[k]];
    nd.x = Math.round(Math.cos(angle) * r * 100) / 100;
    nd.y = Math.round(Math.sin(angle) * r * 100) / 100;
  }
}

interface OverviewCache {
  key: string;
  nodes: CommunityMetaNode[];
  edges: CommunityMetaEdge[];
  totalCommunities: number;
}

/** The main db file mtime (numeric). The viz is read-only (DryRun, no writes),
 * so the WAL doesn't change from its perspective — the main mtime is the right
 * cache key (it bumps only when the store is re-indexed out of band). Checking
 * the WAL mtime would invalidate the cache every time the provider opens the
 * store (SQLite touches the WAL on open), defeating the cache. */
export function storeMtimeMs(cfg: VizConfig): number {
  return statSync(dbPath(cfg)).mtimeMs;
}

/** Cached Louvain label-map (`name → label`), keyed by store mtime. Shared by
 * the overview projection and the drill member-index. Returns `{}` when the
 * graph is too small to cluster. */
interface LabelMapCache {
  mtimeMs: number;
  map: Record<string, number>;
}
let labelMapCache: LabelMapCache | null = null;

export function getLabelMap(cfg: VizConfig, scope: Scope): Record<string, number> {
  const mtimeMs = storeMtimeMs(cfg);
  if (labelMapCache && labelMapCache.mtimeMs === mtimeMs) return labelMapCache.map;
  // call_communities lives on the standalone knowledge engine (not the typed
  // transport) — reach it via the raw binding. `loadNativeBinding` is exported
  // from @engram/node.
  const binding = loadNativeBinding() as unknown as {
    NativeKnowledgeEngine: new (path: string | null) => {
      callCommunitiesJson: (req: string) => string;
    };
  };
  const engine = new binding.NativeKnowledgeEngine(dbPath(cfg));
  const raw = engine.callCommunitiesJson(JSON.stringify({ scope, maxPasses: 2 }));
  const map = JSON.parse(raw) as Record<string, number>;
  labelMapCache = { mtimeMs, map };
  return map;
}

let cache: OverviewCache | null = null;

/** A stream over the scope's relationships: names (for community clustering) AND
 * the subject/object entity ids (for the drill member-index). `call_communities`
 * keys by entity_key() = name-then-id, so fall back to id when name is absent. */
export function* relationEdges(
  cfg: VizConfig,
  scope: Scope,
): Generator<{
  subjectName: string;
  objectName: string;
  subjectId?: string;
  objectId?: string;
}> {
  const db = openReader(cfg);
  try {
    const where = "tenant = ? AND workspace = ?";
    const params = [scope.tenant, scope.workspace ?? ""];
    let cursorRowid = 0;
    for (;;) {
      const page = paginate(db, {
        table: "knowledge_relationships",
        columns: "record_json",
        where,
        params,
        cursorRowid,
        limit: 1000,
        proj: (row) => row.record_json as string,
      });
      for (const rec of page.items) {
        const rel = JSON.parse(rec) as {
          subject?: { name?: string; id?: string };
          object?: { name?: string; id?: string };
        };
        const s = rel.subject?.name ?? rel.subject?.id;
        const o = rel.object?.name ?? rel.object?.id;
        if (s && o) {
          yield {
            subjectName: s,
            objectName: o,
            subjectId: rel.subject?.id,
            objectId: rel.object?.id,
          };
        }
      }
      if (!page.nextCursor) break;
      cursorRowid = decodeCursor(page.nextCursor);
    }
  } finally {
    db.close();
  }
}

// ---- Community data via the Rust facade (P3.4) ----------------------------

/** Shared community data (cached per store-mtime). The Rust facade computes
 *  Louvain + meta-edges + the member index; both run once per store version. */
interface CommunityData {
  mtimeMs: number;
  overview: CommunityOverviewData;
  memberIndex: CommunityMemberIndex;
}

let communityDataCache: CommunityData | null = null;

/** Fetches (and caches per mtime) the community overview + member index from
 *  the Rust facade. Shared by computeOverview + getMemberIndex so Louvain runs
 *  at most twice per store version (once per facade method). */
export async function getCommunityData(
  cfg: VizConfig,
  scope: Scope,
): Promise<CommunityData> {
  // The viz is read-only (DryRun, no writes) → the community data doesn't change
  // during a session. Cache once; the user restarts to pick up store changes.
  // (A mtime-based key invalidated on every read-only `node:sqlite` open, which
  // touches the WAL file + bumps the main db's read counter.)
  if (communityDataCache) {
    return communityDataCache;
  }
  const [overview, memberIndex] = await Promise.all([
    getProvider(cfg).communityOverview(scope, 200),
    getProvider(cfg).communityMemberIndex(scope),
  ]);
  communityDataCache = { mtimeMs: 0, overview, memberIndex };
  return communityDataCache;
}

/**
 * The cached overview. The Rust facade (CommunityQuery) computes Louvain +
 * meta-edges; this maps the neutral result to the viz display shape + applies
 * the concentric-ring layout. Returns `built:false` when too small to cluster.
 */
export async function computeOverview(
  cfg: VizConfig,
  scope: Scope,
  limit: number = DEFAULT_COMMUNITY_LIMIT,
): Promise<{
  nodes: CommunityMetaNode[];
  edges: CommunityMetaEdge[];
  built: boolean;
  totalCommunities: number;
}> {
  const data = await getCommunityData(cfg, scope);
  const ov = data.overview;
  if (ov.totalCommunities === 0) {
    return { nodes: [], edges: [], built: false, totalCommunities: 0 };
  }
  // Truncate the top-200 overview to the requested display limit + map to viz shape.
  const communities = ov.communities.slice(0, limit);
  const topLabels = new Set(communities.map((c) => c.label));
  const nodes: CommunityMetaNode[] = communities.map((c) => ({
    id: `c${c.label}`,
    name: `Community ${c.label}`,
    memberCount: c.memberCount,
  }));
  const edges: CommunityMetaEdge[] = ov.edges
    .filter((e) => topLabels.has(e.sourceLabel) && topLabels.has(e.targetLabel))
    .map((e) => ({
      source: `c${e.sourceLabel}`,
      target: `c${e.targetLabel}`,
      weight: e.weight,
    }));
  layoutGraph(nodes, edges);
  return { nodes, edges, built: true, totalCommunities: ov.totalCommunities };
}

/** Test-only: clear the caches. */
export function _resetCommunityCacheForTests(): void {
  communityDataCache = null;
  cache = null;
  labelMapCache = null;
}
