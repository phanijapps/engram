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
import type { GraphEntityView } from "../views/types.ts";
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
    return { items: [], nextCursor: null, memberCount: 0, sampled: 0, found: false };
  }
  const all = ids.get(label) ?? [];
  const pageIds = all.slice(offset, offset + limit);
  const ws = scope.workspace ?? "";
  const db = openReader(cfg);
  const items: GraphEntityView[] = [];
  try {
    const stmt = db.prepare(
      "SELECT record_json FROM knowledge_entities WHERE id = ? AND tenant = ? AND workspace = ?",
    );
    for (const id of pageIds) {
      const row = stmt.get(id, scope.tenant, ws) as { record_json?: string } | undefined;
      if (row?.record_json) items.push(projectEntity(JSON.parse(row.record_json)));
    }
  } finally {
    db.close();
  }
  const nextOffset = offset + limit;
  return {
    items,
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
