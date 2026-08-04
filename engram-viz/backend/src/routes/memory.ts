//! Memory routes — keyset-paginated lists over the memory / belief / procedure
//! tables via the read-only `node:sqlite` secondary path (the binding's memory
//! list transports are firehoses; the belief transport's listBeliefs is too, so
//! the viz paginates via sqlite — the foundation's established Boundary). Scope-
//! filtered, capped, fail-closed. Beliefs/procedures/contradictions are empty in
//! the agentzero store today → honest empty pages (contradictions are synthesized
//! from beliefs by the belief engine; 0 beliefs → 0 contradictions).

import { Hono, type Context } from "hono";
import { DatabaseSync } from "node:sqlite";

import { dbPath, type VizConfig } from "../config.ts";
import { resolveScope } from "../scope.ts";
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
  ): Response => {
    try {
      const limit = queryLimit(c.req.query("limit"));
      const cursorRowid = decodeCursor(c.req.query("cursor"));
      const db = new DatabaseSync(dbPath(cfg), { readOnly: true });
      try {
        const page = paginate(db, {
          table,
          columns: "record_json",
          where: scopeWhere,
          params: [...scopeParams],
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

  app.get("/memory", (c) => list(c, "memories", projectMemory));
  app.get("/beliefs", (c) => list(c, "beliefs", projectBelief));
  app.get("/procedures", (c) => list(c, "procedures", projectProcedure));
  // Contradictions have no table — they are synthesized from beliefs. 0 beliefs
  // today → an honest empty page (no fabricated records).
  app.get("/contradictions", (c) => c.json({ items: [], nextCursor: null }));

  return app;
}
