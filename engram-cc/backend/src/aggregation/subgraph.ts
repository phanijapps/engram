//! Actual-graph subgraph: the real entity graph (symbols + call edges), not
//! the community meta-graph. `/api/graph/subgraph` serves it — degree-ranked,
//! bounded, and resolved-only by default so the rendered edges are the
//! meaningful ones (cross-file resolution output), not ledger noise.
//!
//! Read path mirrors the other aggregations: read-only `node:sqlite`,
//! scope-filtered, predicate/resolution filtering pushed into SQL
//! (`json_extract`) so a large store is not fully materialized in JS.

import { DatabaseSync } from "node:sqlite";

import type { Scope } from "@engram/contracts";

import { dbPath, type VizConfig } from "../config.ts";

export interface SubgraphNode {
  id: string;
  name: string;
  kind: string;
  /** total matching edges touching this node (pre-selection degree). */
  degree: number;
}

export interface SubgraphEdge {
  source: string;
  target: string;
  predicate: string;
}

export interface SubgraphResult {
  nodes: SubgraphNode[];
  edges: SubgraphEdge[];
  /** distinct endpoints across ALL matching edges in scope (pre-selection). */
  totalNodes: number;
  /** all matching edges in scope (pre-selection). */
  totalEdges: number;
  resolvedOnly: boolean;
  predicates: string[];
}

export const DEFAULT_SUBGRAPH_NODES = 300;
export const MAX_SUBGRAPH_NODES = 800;
/** default edge predicates for the actual-graph view — the call topology. */
export const DEFAULT_PREDICATES = ["calls", "routes_to"];

interface RelationshipRecord {
  subject: { id?: string; name?: string; kind?: string };
  predicate: string;
  object: { id?: string; name?: string; kind?: string };
}

function clampNodeLimit(raw: string | undefined): number {
  if (raw === undefined) return DEFAULT_SUBGRAPH_NODES;
  const n = Math.floor(Number(raw));
  if (!Number.isFinite(n) || n < 1) return DEFAULT_SUBGRAPH_NODES;
  return Math.min(n, MAX_SUBGRAPH_NODES);
}

function parsePredicates(raw: string | undefined): string[] {
  if (raw === undefined || raw.trim() === "") return DEFAULT_PREDICATES;
  const list = raw
    .split(",")
    .map((p) => p.trim())
    .filter((p) => p.length > 0)
    .slice(0, 8);
  return list.length > 0 ? list : DEFAULT_PREDICATES;
}

/**
 * Computes the bounded actual-graph subgraph for the scope.
 *
 * Selection: rank endpoint entities by matching-edge degree, keep the top
 * `limit`, and keep edges whose BOTH endpoints survived (so the rendered
 * topology is honest — no dangling edges to dropped nodes).
 */
export function computeSubgraph(
  cfg: VizConfig,
  scope: Scope,
  query: { limit?: string; predicates?: string; resolved?: string },
): SubgraphResult {
  const limit = clampNodeLimit(query.limit);
  const predicates = parsePredicates(query.predicates);
  // Default true: unresolved (name-only) endpoints are ledger noise — the
  // actual-graph view shows resolved topology.
  const resolvedOnly = query.resolved !== "false";

  const db = new DatabaseSync(dbPath(cfg), { readOnly: true });
  try {
    const predicatesSql = predicates.map(() => "?").join(",");
    const resolutionSql = resolvedOnly
      ? " AND json_extract(record_json, '$.subject.id') IS NOT NULL" +
        " AND json_extract(record_json, '$.object.id') IS NOT NULL"
      : "";
    const stmt = db.prepare(
      `SELECT record_json FROM knowledge_relationships
       WHERE tenant = ? AND workspace = ?
         AND json_extract(record_json, '$.predicate') IN (${predicatesSql})${resolutionSql}`,
    );
    const params: (string | number)[] = [scope.tenant, scope.workspace ?? "", ...predicates];
    const rows = stmt.all(...params) as { record_json: string }[];

    const degree = new Map<string, SubgraphNode>();
    const allEdges: { rel: RelationshipRecord; s: string; t: string }[] = [];
    for (const row of rows) {
      const rel = JSON.parse(row.record_json) as RelationshipRecord;
      const s = rel.subject.id;
      const t = rel.object.id;
      if (!s || !t || s === t) continue;
      allEdges.push({ rel, s, t });
      for (const [id, endpoint] of [
        [s, rel.subject],
        [t, rel.object],
      ] as const) {
        const existing = degree.get(id);
        if (existing) {
          existing.degree += 1;
        } else {
          degree.set(id, {
            id,
            name: endpoint.name ?? id,
            kind: endpoint.kind ?? "unknown",
            degree: 1,
          });
        }
      }
    }

    const ranked = [...degree.values()].sort((a, b) => {
      // Noise penalty: minified-bundle symbols (1–2 chars, e.g. `M`, `s`, `tn`)
      // come from vendored/bundled assets and crowd out real API surface.
      // They are ranked last within their degree band, not hidden.
      const noise = (n: SubgraphNode) => (n.name.length < 3 ? 0 : 1);
      return noise(b) - noise(a) || b.degree - a.degree || a.name.localeCompare(b.name);
    });
    const totalNodes = ranked.length;
    const selected = ranked.slice(0, limit);
    const selectedIds = new Set(selected.map((n) => n.id));

    const edges: SubgraphEdge[] = [];
    for (const { rel, s, t } of allEdges) {
      if (selectedIds.has(s) && selectedIds.has(t)) {
        edges.push({ source: s, target: t, predicate: rel.predicate });
      }
    }

    return {
      nodes: selected,
      edges,
      totalNodes,
      totalEdges: allEdges.length,
      resolvedOnly,
      predicates,
    };
  } finally {
    db.close();
  }
}
