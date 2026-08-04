//! Drill membership — backs the "click a community → see its entities" drill.
//!
//! `call_communities` keys by entity *name*, but entities are stored by *id*
//! (knowledge_entities has no `name` column), so a community's members cannot be
//! queried by name. The relationship stream carries both `subject.name`+`subject.id`
//! and `object.name`+`object.id`, so a **label→[entityId]** index built from it is
//! the efficient membership path. The index covers the **top-N most populous**
//! communities (the drillable ones shown in the overview), capped per community —
//! bounded memory. Cached per store-mtime (Louvain + the stream run once/version).

import type { Scope } from "@engram/contracts";

import { type VizConfig } from "../config.ts";
import { encodeCursor } from "../db/keyset.ts";
import { openReader } from "../db/reader.ts";
import { projectEntity } from "../views/graph.ts";
import type { GraphEntityView, GraphRelationshipView } from "../views/types.ts";
import {
  getLabelMap,
  rankCommunities,
  relationEdges,
  storeMtimeMs,
} from "./communities.ts";

/** How many of the largest communities are drillable (matches the overview's
 * default top-150 + headroom). */
export const DRILL_TOP_LABELS = 200;
/** Max entity ids retained per community (the drill shows a bounded sample). */
export const MEMBER_INDEX_CAP = 1000;
/** Max intra-community relationships returned per drill page (bounded subgraph). */
export const MAX_DRILL_EDGES = 300;

interface MemberIndex {
  mtimeMs: number;
  /** community label → sample of member entity ids (relationship-participating). */
  ids: Map<number, string[]>;
  /** community label → full member count (all names with that label). */
  counts: Map<number, number>;
  /** entity id → community label (reliable — built from relationship ids, which
   * ARE entity ids; unlike a name lookup, which is keying-fragile). */
  idToLabel: Map<string, number>;
}

let memberCache: MemberIndex | null = null;

/**
 * Cached label→[entityId] index for the top `DRILL_TOP_LABELS` communities. The
 * ids are the entities that participate in a relationship (the connected — and
 * therefore interesting — ones); `counts` carries the *full* membership (from the
 * Louvain label-map) so the UI can say "sample of N"; `idToLabel` is the reliable
 * reverse lookup (entity id → community) used by the entity-detail route.
 */
export function getMemberIndex(cfg: VizConfig, scope: Scope): MemberIndex {
  const mtimeMs = storeMtimeMs(cfg);
  if (memberCache && memberCache.mtimeMs === mtimeMs) return memberCache;

  const map = getLabelMap(cfg, scope);
  const ranked = rankCommunities(map, DRILL_TOP_LABELS);
  const counts = new Map<number, number>(
    ranked.nodes.map((n) => [Number(n.id.slice(1)), n.memberCount]),
  );
  const topLabels = ranked.topLabels;

  const sets = new Map<number, Set<string>>();
  const idToLabel = new Map<string, number>();
  const push = (label: number, id: string | undefined) => {
    if (id === undefined) return;
    let s = sets.get(label);
    if (!s) {
      s = new Set();
      sets.set(label, s);
    }
    if (s.size < MEMBER_INDEX_CAP) s.add(id);
    if (!idToLabel.has(id)) idToLabel.set(id, label);
  };
  for (const e of relationEdges(cfg, scope)) {
    const ls = map[e.subjectName];
    const lo = map[e.objectName];
    if (ls !== undefined && topLabels.has(ls)) push(ls, e.subjectId);
    if (lo !== undefined && topLabels.has(lo)) push(lo, e.objectId);
  }
  const ids = new Map<number, string[]>();
  for (const [label, s] of sets) ids.set(label, [...s]);

  memberCache = { mtimeMs, ids, counts, idToLabel };
  return memberCache;
}

/** Reliable entity-id → community-label lookup (null if the entity is not in a
 * drillable top-N community). Built from relationship ids, not name keying. */
export function entityCommunity(cfg: VizConfig, scope: Scope, id: string): number | null {
  return getMemberIndex(cfg, scope).idToLabel.get(id) ?? null;
}

export interface CommunityMembersPage {
  items: GraphEntityView[];
  /** intra-community relationships among this page's members (bounded subgraph). */
  edges: GraphRelationshipView[];
  nextCursor: string | null;
  memberCount: number;
  /** how many ids are indexable for this community (≤ MEMBER_INDEX_CAP). */
  sampled: number;
  found: boolean;
}

/**
 * A keyset (offset) page of a community's member entities. `offset` is the cursor
 * (decoded by the route); the index is stable per store-mtime. Hydrates each id to
 * a `GraphEntityView` via the read-only reader. `found:false` for a community
 * outside the drillable top-N (→ route 404).
 */
export function communityMembers(
  cfg: VizConfig,
  scope: Scope,
  label: number,
  offset: number,
  limit: number,
): CommunityMembersPage {
  const { ids, counts } = getMemberIndex(cfg, scope);
  if (!counts.has(label)) {
    return { items: [], edges: [], nextCursor: null, memberCount: 0, sampled: 0, found: false };
  }
  const all = ids.get(label) ?? [];
  const pageIds = all.slice(offset, offset + limit);
  const idSet = new Set(pageIds);
  const ws = scope.workspace ?? "";
  const db = openReader(cfg);
  const items: GraphEntityView[] = [];
  const edges: GraphRelationshipView[] = [];
  try {
    const entStmt = db.prepare(
      "SELECT record_json FROM knowledge_entities WHERE id = ? AND tenant = ? AND workspace = ?",
    );
    for (const id of pageIds) {
      const row = entStmt.get(id, scope.tenant, ws) as { record_json?: string } | undefined;
      if (row?.record_json) items.push(projectEntity(JSON.parse(row.record_json)));
    }
    // Intra-community relationships among this page's members: relationships whose
    // subject is in the page, kept when the object is also in the page (a bounded
    // subgraph so the drill shows how the members connect). `subject_id` is indexed.
    if (pageIds.length > 0) {
      const placeholders = pageIds.map(() => "?").join(",");
      const relStmt = db.prepare(
        `SELECT record_json FROM knowledge_relationships WHERE subject_id IN (${placeholders}) AND tenant = ? AND workspace = ?`,
      );
      const rows = relStmt.all(...pageIds, scope.tenant, ws) as { record_json?: string }[];
      for (const row of rows) {
        if (edges.length >= MAX_DRILL_EDGES) break;
        const rel = JSON.parse(row.record_json as string) as {
          subject?: { id?: string };
          predicate?: string;
          object?: { id?: string };
        };
        const s = rel.subject?.id;
        const o = rel.object?.id;
        const p = rel.predicate;
        if (s && o && p && idSet.has(s) && idSet.has(o)) {
          edges.push({ source: s, predicate: p, target: o });
        }
      }
    }
  } finally {
    db.close();
  }
  const nextOffset = offset + limit;
  return {
    items,
    edges,
    nextCursor: nextOffset < all.length ? encodeCursor(nextOffset) : null,
    memberCount: counts.get(label) ?? all.length,
    sampled: all.length,
    found: true,
  };
}

/** Test-only: clear the cache. */
export function _resetMemberCacheForTests(): void {
  memberCache = null;
}
