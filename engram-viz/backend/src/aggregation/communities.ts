//! Community-overview projection — the Graph tab's level-of-detail payload.
//!
//! `call_communities` (Louvain, on the standalone knowledge engine) returns a
//! flat `Record<entityName, communityLabel>`. This module projects that into
//! bounded `CommunityMetaNode[]` (≤ 2000, ranked by membership, deterministic
//! layout) + `CommunityMetaEdge[]` (≤ 4000 inter-community, top by weight,
//! tallied by streaming relationship names — no 227 k materialization). The
//! compute is cached by the store file's mtime; Louvain runs once per version.

import { statSync } from "node:fs";
import { loadNativeBinding } from "@engram/node";
import type { Scope } from "@engram/contracts";

import { dbPath, type VizConfig } from "../config.ts";
import { decodeCursor } from "../db/keyset.ts";
import { openReader, paginate } from "../db/reader.ts";
import type { CommunityMetaEdge, CommunityMetaNode } from "../views/types.ts";

export const MAX_COMMUNITY_NODES = 2000;
export const MAX_COMMUNITY_EDGES = 4000;

/** Pure: group names by label, rank by membership, truncate, deterministic layout. */
export function rankCommunities(
  nameToLabel: Record<string, number>,
  maxNodes: number = MAX_COMMUNITY_NODES,
): { nodes: CommunityMetaNode[]; topLabels: Set<number> } {
  const counts = new Map<number, number>();
  for (const label of Object.values(nameToLabel)) {
    counts.set(label, (counts.get(label) ?? 0) + 1);
  }
  const ranked = [...counts.entries()]
    .sort((a, b) => b[1] - a[1])
    .slice(0, maxNodes);
  const total = ranked.length;
  const nodes: CommunityMetaNode[] = ranked.map(([label, memberCount], i) => {
    const angle = (i / Math.max(total, 1)) * 2 * Math.PI;
    const radius = Math.sqrt(i + 1) * 6; // deterministic spread (no RNG)
    return {
      id: `c${label}`,
      name: `Community ${label}`,
      memberCount,
      x: Math.round(Math.cos(angle) * radius * 100) / 100,
      y: Math.round(Math.sin(angle) * radius * 100) / 100,
    };
  });
  return { nodes, topLabels: new Set(ranked.map(([label]) => label)) };
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

interface OverviewCache {
  mtimeMs: number;
  nodes: CommunityMetaNode[];
  edges: CommunityMetaEdge[];
}

/** Cache key: max of the main db + WAL mtimes. In WAL mode, writes append to the
 * `-wal` file and the main file only bumps at checkpoint, so a main-only key
 * would serve stale communities until checkpoint. */
function cacheKeyMtime(cfg: VizConfig): number {
  const mainPath = dbPath(cfg);
  let key = statSync(mainPath).mtimeMs;
  try {
    key = Math.max(key, statSync(`${mainPath}-wal`).mtimeMs);
  } catch {
    // no WAL file yet — main mtime alone is correct
  }
  return key;
}

let cache: OverviewCache | null = null;

/** A name-pair stream over the scope's relationships (record_json → names). */
function* relationNamePairs(
  cfg: VizConfig,
  scope: Scope,
): Generator<{ subjectName: string; objectName: string }> {
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
        // call_communities keys by entity_key() = name-then-id, so fall back to
        // id when name is absent (else id-keyed endpoints would be clustered by
        // Louvain but contribute zero weight to the meta-edge tally).
        const s = rel.subject?.name ?? rel.subject?.id;
        const o = rel.object?.name ?? rel.object?.id;
        if (s && o) yield { subjectName: s, objectName: o };
      }
      if (!page.nextCursor) break;
      cursorRowid = decodeCursor(page.nextCursor);
    }
  } finally {
    db.close();
  }
}

/**
 * The cached overview. Louvain runs once per store-mtime version; relationship
 * streaming tallies inter-community edges. Returns `built:false` when the graph
 * is too small to cluster (no communities).
 */
export function computeOverview(
  cfg: VizConfig,
  scope: Scope,
): { nodes: CommunityMetaNode[]; edges: CommunityMetaEdge[]; built: boolean } {
  const mtimeMs = cacheKeyMtime(cfg);
  if (cache && cache.mtimeMs === mtimeMs) {
    return { nodes: cache.nodes, edges: cache.edges, built: true };
  }
  // call_communities lives on the standalone knowledge engine (not the typed
  // transport) — reach it via the raw binding, same pattern the prior backend
  // used. `loadNativeBinding` is exported from @engram/node.
  const binding = loadNativeBinding() as unknown as {
    NativeKnowledgeEngine: new (path: string | null) => {
      callCommunitiesJson: (req: string) => string;
    };
  };
  const engine = new binding.NativeKnowledgeEngine(dbPath(cfg));
  const raw = engine.callCommunitiesJson(
    JSON.stringify({ scope, maxPasses: 2 }),
  );
  const nameToLabel = JSON.parse(raw) as Record<string, number>;
  if (Object.keys(nameToLabel).length === 0) {
    return { nodes: [], edges: [], built: false };
  }
  const { nodes, topLabels } = rankCommunities(nameToLabel);
  const edges = tallyMetaEdges(relationNamePairs(cfg, scope), nameToLabel, topLabels);
  cache = { mtimeMs, nodes, edges };
  return { nodes, edges, built: true };
}

/** Test-only: clear the cache. */
export function _resetCommunityCacheForTests(): void {
  cache = null;
}
