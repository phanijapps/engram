//! Read-only secondary data path.
//!
//! The `@engram/node` binding lacks pagination + bulk counts; this reader is the
//! *sole* supplementary read path for those, opened `readOnly` against the same
//! single-file store (never writes, never re-implements domain semantics). A
//! backlog item may nativize pagination/aggregation into the binding so this
//! retires.

import { DatabaseSync, type SQLInputValue } from "node:sqlite";

import { dbPath, type VizConfig } from "../config.ts";
import { encodeCursor } from "./keyset.ts";

/** Open the store read-only. Caller owns + closes the handle. */
export function openReader(cfg: VizConfig): DatabaseSync {
  return new DatabaseSync(dbPath(cfg), { readOnly: true });
}

export interface Page<T> {
  items: T[];
  nextCursor: string | null;
}

/**
 * Keyset-paginate a table over `rowid`. `cursorRowid` 0 starts from the
 * beginning; `proj` maps each row to its view. Optional `where` (with `params`)
 * filters — e.g. a neighborhood's `subject_id = ? OR object_id = ?`.
 *
 * NOTE: `table`/`where` are server-authored constants (never user input), so
 * string interpolation here is not an injection surface; caller values go
 * through `?` placeholders.
 */
export function paginate<T>(
  db: DatabaseSync,
  opts: {
    table: string;
    columns?: string;
    cursorRowid: number;
    limit: number;
    where?: string;
    params?: SQLInputValue[];
    proj: (row: Record<string, unknown>) => T;
  },
): Page<T> {
  const columns = opts.columns ?? "*";
  const where = opts.where
    ? `WHERE (${opts.where}) AND rowid > ?`
    : "WHERE rowid > ?";
  const params: SQLInputValue[] = opts.where
    ? [...(opts.params ?? []), opts.cursorRowid, opts.limit]
    : [opts.cursorRowid, opts.limit];
  const rows = db
    .prepare(
      `SELECT rowid, ${columns} FROM ${opts.table} ${where} ORDER BY rowid LIMIT ?`,
    )
    .all(...params) as Record<string, unknown>[];
  const items = rows.map(opts.proj);
  const lastRowid = rows.length
    ? (rows[rows.length - 1].rowid as number)
    : null;
  const nextCursor = rows.length >= opts.limit ? encodeCursor(lastRowid) : null;
  return { items, nextCursor };
}

/** `COUNT(*)` for a table (optionally filtered). */
export function countTable(
  db: DatabaseSync,
  table: string,
  where?: string,
  params?: SQLInputValue[],
): number {
  const sql = where ? `SELECT COUNT(*) AS n FROM ${table} WHERE ${where}` : `SELECT COUNT(*) AS n FROM ${table}`;
  const row = db.prepare(sql).get(...(params ?? [])) as { n: number };
  return row.n;
}
