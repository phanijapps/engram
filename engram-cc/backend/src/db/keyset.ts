//! Keyset (cursor) pagination primitives — pure, no I/O.
//!
//! Cursor = base64url of the last `rowid` seen; `null` means "from the start"
//! (request) or "no more pages" (response `nextCursor`). The actual keyset SQL
//! (`WHERE rowid > ? ORDER BY rowid LIMIT ?`) lives in `reader.ts`; this module
//! owns the compressible invariant — encode/decode/clamp — so it is TDD-able.

export const DEFAULT_PAGE_SIZE = 100;
export const MAX_PAGE_SIZE = 500;

/** Clamp a caller-supplied limit to `[1, MAX_PAGE_SIZE]` with a sane default. */
export function clampLimit(limit: unknown): number {
  const n =
    typeof limit === "number" && Number.isFinite(limit)
      ? Math.floor(limit)
      : DEFAULT_PAGE_SIZE;
  if (n < 1) return 1;
  return Math.min(n, MAX_PAGE_SIZE);
}

/** A malformed cursor (bad base64url, or decodes to a non-integer). Maps to 422. */
export class CursorError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "CursorError";
  }
}

/** Opaque cursor for a rowid; `null` for start / no-more. */
export function encodeCursor(rowid: number | null | undefined): string | null {
  if (rowid === null || rowid === undefined || rowid <= 0) return null;
  return Buffer.from(String(rowid), "utf8").toString("base64url");
}

/** Decode an opaque cursor token to a rowid (0 = start). Throws on malformed. */
export function decodeCursor(token: unknown): number {
  if (token === null || token === undefined || token === "") return 0;
  if (typeof token !== "string") {
    throw new CursorError("cursor must be a string");
  }
  const decoded = Buffer.from(token, "base64url").toString("utf8");
  const n = Number(decoded);
  if (!Number.isInteger(n) || n < 0) {
    throw new CursorError("malformed cursor");
  }
  return n;
}
