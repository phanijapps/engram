//! Memory routes — keyset-paginated lists over the memory / belief / procedure
//! tables via the read-only `node:sqlite` secondary path (the binding's memory
//! list transports are firehoses; the belief transport's listBeliefs is too, so
//! the viz paginates via sqlite — the foundation's established Boundary). Scope-
//! filtered, capped, fail-closed. Beliefs/procedures/contradictions are empty in
//! the agentzero store today → honest empty pages (contradictions are synthesized
//! from beliefs by the belief engine; 0 beliefs → 0 contradictions).

import { Hono, type Context } from "hono";
import { DatabaseSync } from "node:sqlite";

import type { RetrievalRequest } from "@engram/contracts";
import { dbPath, type VizConfig } from "../config.ts";
import { resolveScope } from "../scope.ts";
import { getProvider } from "../engram/provider.ts";
import { CursorError, clampLimit, decodeCursor } from "../db/keyset.ts";
import { paginate } from "../db/reader.ts";
import { projectBelief, projectMemory, projectProcedure } from "../views/memory.ts";

function msg(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

function queryLimit(raw: string | undefined): number {
  return clampLimit(raw === undefined ? undefined : Number(raw));
}

export function memoryRoute(cfg: VizConfig): Hono {
  const app = new Hono();
  const scope = resolveScope(cfg);
  const scopeWhere = "tenant = ? AND workspace = ?";
  const scopeParams: string[] = [scope.tenant, scope.workspace ?? ""];

  const list = (
    c: Context,
    table: string,
    proj: (record: unknown) => unknown,
    kindFilter?: string,
    searchQuery?: string,
  ): Response => {
    try {
      const limit = queryLimit(c.req.query("limit"));
      const cursorRowid = decodeCursor(c.req.query("cursor"));
      const db = new DatabaseSync(dbPath(cfg), { readOnly: true });
      try {
        let where = scopeWhere;
        const params: string[] = [...scopeParams];
        if (kindFilter) {
          where += " AND json_extract(record_json, '$.kind') = ?";
          params.push(kindFilter);
        }
        if (searchQuery) {
          where += " AND json_extract(record_json, '$.content.text') LIKE ?";
          params.push(`%${searchQuery}%`);
        }
        const page = paginate(db, {
          table,
          columns: "record_json",
          where,
          params,
          cursorRowid,
          limit,
          proj: (row) => proj(JSON.parse(row.record_json as string)),
        });
        return c.json(page);
      } finally {
        db.close();
      }
    } catch (err) {
      if (err instanceof CursorError) {
        return c.json({ error: "malformed cursor" }, 422);
      }
      return c.json({ error: msg(err), degraded: true }, 503);
    }
  };

  // /memory is served by the Rust facade (P1.6) — keyset + cursor live behind
  // `list_memories_paged` in engram-integration; this route holds no SQL. (Beliefs,
  // procedures, contradictions still use the read-only node:sqlite path until P2.)
  app.get("/memory", async (c) => {
    const q = c.req.query("q");
    if (q) {
      return list(c, "memories", projectMemory, undefined, q);
    }
    try {
      const limit = queryLimit(c.req.query("limit"));
      const cursor = c.req.query("cursor") ?? null;
      const page = await getProvider(cfg).listMemoriesPaged(scope, cursor, limit);
      return c.json({
        items: page.items.map((m) => projectMemory(m)),
        nextCursor: page.nextCursor,
      });
    } catch (err) {
      return c.json({ error: msg(err), degraded: true }, 503);
    }
  });
  app.get("/beliefs", (c) => list(c, "beliefs", projectBelief));
  // Procedures live in their OWN table (stored via procedure_put / the
  // consolidated `remember kind=procedure`), not as memories with kind=procedure.
  // The old route queried the memories table — silently empty.
  app.get("/procedures", (c) =>
    list(c, "procedures", (record: unknown) => {
      const r = record as { id?: string; name?: string; steps?: string[]; successCount?: number; failureCount?: number };
      return {
        id: r.id ?? "",
        name: r.name ?? "",
        text: (r.steps ?? []).join(" "),  // procedures store content as steps
        successCount: r.successCount ?? 0,
        failureCount: r.failureCount ?? 0,
      };
    }));
  // Contradictions have no table — they are synthesized from beliefs. 0 beliefs
  // today → an honest empty page (no fabricated records).
  app.get("/contradictions", (c) => c.json({ items: [], nextCursor: null }));

  // Hybrid recall — vector + graph + associative + temporal fusion (the real
  // search). Returns a ContextPayload with scored items. Requires vectors on
  // (ENGRAM_ENABLE_VECTOR=true) for the vector lane; other lanes always work.
  app.get("/recall", async (c) => {
    const q = c.req.query("q");
    if (!q) return c.json({ error: "query required (?q=text)" }, 422);
    try {
      const result = await getProvider(cfg).recall({
        query: q,
        scope,
        requester: { actor: { id: "engram-cc", kind: "agent", displayName: "engram-cc" } },
      } as unknown as RetrievalRequest);
      return c.json(result);
    } catch (err) {
      return c.json({ error: msg(err), degraded: true }, 503);
    }
  });

  return app;
}
