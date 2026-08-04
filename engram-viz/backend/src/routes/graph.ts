//! Graph routes: aggregate stats, keyset entity list, keyset one-hop neighbors.
//! All read-only via the `node:sqlite` secondary path, scope-filtered
//! (tenant/workspace from the seam), keyset + capped. 422 on a bad cursor,
//! 503 on a degraded store. The community-overview route lands in T6.

import { Hono } from "hono";
import { DatabaseSync } from "node:sqlite";

import { dbPath, type VizConfig } from "../config.ts";
import { resolveScope } from "../scope.ts";
import { CursorError, clampLimit, decodeCursor } from "../db/keyset.ts";
import { countTable, paginate } from "../db/reader.ts";
import {
  computeOverview,
  DEFAULT_COMMUNITY_LIMIT,
  MAX_COMMUNITY_NODES,
} from "../aggregation/communities.ts";
import { projectEntity, projectOutgoingNeighbor } from "../views/graph.ts";

function msg(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

function queryLimit(raw: string | undefined): number {
  return clampLimit(raw === undefined ? undefined : Number(raw));
}

/** Overview community count: default 150 (a legible overview), hard-cap 2000. */
function queryCommunityLimit(raw: string | undefined): number {
  const n = raw === undefined ? DEFAULT_COMMUNITY_LIMIT : Math.floor(Number(raw));
  if (!Number.isFinite(n) || n < 1) return DEFAULT_COMMUNITY_LIMIT;
  return Math.min(n, MAX_COMMUNITY_NODES);
}

export function graphRoute(cfg: VizConfig): Hono {
  const app = new Hono();
  const scope = resolveScope(cfg);
  const scopeWhere = "tenant = ? AND workspace = ?";
  const scopeParams: string[] = [scope.tenant, scope.workspace ?? ""];

  const withReader = <T>(fn: (db: DatabaseSync) => T): T => {
    const db = new DatabaseSync(dbPath(cfg), { readOnly: true });
    try {
      return fn(db);
    } finally {
      db.close();
    }
  };

  app.get("/graph/stats", (c) => {
    try {
      return c.json(
        withReader((db) => ({
          entities: countTable(db, "knowledge_entities", scopeWhere, [...scopeParams]),
          relationships: countTable(db, "knowledge_relationships", scopeWhere, [
            ...scopeParams,
          ]),
          communities: 0, // filled by the community projection (T6)
          memories: countTable(db, "memories", scopeWhere, [...scopeParams]),
          beliefs: countTable(db, "beliefs", scopeWhere, [...scopeParams]),
        })),
      );
    } catch (err) {
      return c.json({ error: msg(err), degraded: true }, 503);
    }
  });

  app.get("/graph/communities", (c) => {
    try {
      const limit = queryCommunityLimit(c.req.query("limit"));
      const overview = computeOverview(cfg, scope, limit);
      return c.json({
        communities: overview.nodes,
        edges: overview.edges,
        built: overview.built,
        totalCommunities: overview.totalCommunities,
      });
    } catch (err) {
      return c.json({ error: msg(err), degraded: true }, 503);
    }
  });

  app.get("/entities", (c) => {
    try {
      const limit = queryLimit(c.req.query("limit"));
      const cursorRowid = decodeCursor(c.req.query("cursor"));
      const page = withReader((db) =>
        paginate(db, {
          table: "knowledge_entities",
          columns: "record_json",
          where: scopeWhere,
          params: [...scopeParams],
          cursorRowid,
          limit,
          proj: (row) => projectEntity(JSON.parse(row.record_json as string)),
        }),
      );
      return c.json(page);
    } catch (err) {
      if (err instanceof CursorError) {
        return c.json({ error: "malformed cursor" }, 422);
      }
      return c.json({ error: msg(err), degraded: true }, 503);
    }
  });

  app.get("/graph/node/:id/neighbors", (c) => {
    const nodeId = c.req.param("id");
    try {
      const limit = queryLimit(c.req.query("limit"));
      const cursorRowid = decodeCursor(c.req.query("cursor"));
      // Outgoing edges only: `subject_id` is the indexed endpoint column.
      // (`object_id` is not a column — incoming edges need an unindexed
      // record_json scan; deferred.) See spec viz-graph-explorer plan.
      const page = withReader((db) =>
        paginate(db, {
          table: "knowledge_relationships",
          columns: "record_json",
          where: "subject_id = ? AND tenant = ? AND workspace = ?",
          params: [nodeId, ...scopeParams],
          cursorRowid,
          limit,
          proj: (row) =>
            projectOutgoingNeighbor(JSON.parse(row.record_json as string)),
        }),
      );
      return c.json(page);
    } catch (err) {
      if (err instanceof CursorError) {
        return c.json({ error: "malformed cursor" }, 422);
      }
      return c.json({ error: msg(err), degraded: true }, 503);
    }
  });

  return app;
}
