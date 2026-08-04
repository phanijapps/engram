//! T3 — keyset cursor logic (pure TDD) + paginate() on a demo fixture table
//! (validates the keyset mechanism: disjoint pages, cap enforcement, nextCursor
//! without coupling to engram's schema). Counts smoke against the live store
//! are covered by the T1 smoke + `countTable` unit-tested here on the fixture.

import { describe, it, expect } from "vitest";
import { DatabaseSync } from "node:sqlite";
import { mkdtempSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

import {
  clampLimit,
  decodeCursor,
  encodeCursor,
  CursorError,
  DEFAULT_PAGE_SIZE,
  MAX_PAGE_SIZE,
} from "../src/db/keyset.ts";
import { countTable, paginate } from "../src/db/reader.ts";

describe("keyset cursor logic", () => {
  it("defaults to DEFAULT_PAGE_SIZE, clamps to [1, MAX]", () => {
    expect(DEFAULT_PAGE_SIZE).toBe(100);
    expect(MAX_PAGE_SIZE).toBe(500);
    expect(clampLimit(undefined)).toBe(100);
    expect(clampLimit(0)).toBe(1);
    expect(clampLimit(1000)).toBe(500);
    expect(clampLimit(50)).toBe(50);
  });

  it("encodeCursor is null for start / non-positive; round-trips a rowid", () => {
    expect(encodeCursor(0)).toBeNull();
    expect(encodeCursor(null)).toBeNull();
    expect(decodeCursor(encodeCursor(42))).toBe(42);
    expect(decodeCursor(null)).toBe(0);
    expect(decodeCursor("")).toBe(0);
  });

  it("decodeCursor throws CursorError on a non-numeric payload", () => {
    const bad = Buffer.from("not-a-number", "utf8").toString("base64url");
    expect(() => decodeCursor(bad)).toThrow(CursorError);
    expect(() => decodeCursor(123)).toThrow(CursorError); // not a string
  });
});

function fixtureDb(): DatabaseSync {
  const dir = mkdtempSync(join(tmpdir(), "engram-viz-t3-"));
  const db = new DatabaseSync(join(dir, "t.db"));
  db.exec("CREATE TABLE demo (id TEXT, kind TEXT)");
  const ins = db.prepare("INSERT INTO demo (id, kind) VALUES (?, ?)");
  for (let i = 0; i < 25; i++) ins.run(`e${i}`, "function");
  return db;
}

describe("paginate (demo fixture)", () => {
  it("returns disjoint pages + nextCursor; final page has null nextCursor", () => {
    const db = fixtureDb();
    const p1 = paginate(db, { table: "demo", cursorRowid: 0, limit: 10, proj: (r) => r.id as string });
    expect(p1.items).toHaveLength(10);
    expect(p1.nextCursor).not.toBeNull();

    const p2 = paginate(db, { table: "demo", cursorRowid: decodeCursor(p1.nextCursor), limit: 10, proj: (r) => r.id as string });
    expect(p2.items).toHaveLength(10);

    // pages are disjoint
    const seen = new Set([...p1.items, ...p2.items]);
    expect(seen.size).toBe(20);

    const p3 = paginate(db, { table: "demo", cursorRowid: decodeCursor(p2.nextCursor), limit: 10, proj: (r) => r.id as string });
    expect(p3.items).toHaveLength(5);
    expect(p3.nextCursor).toBeNull();
    db.close();
  });

  it("enforces the cap below the requested limit", () => {
    const db = fixtureDb();
    const page = paginate(db, { table: "demo", cursorRowid: 0, limit: 1000, proj: (r) => r.id as string });
    // 1000 requested, only 25 exist; returns all 25, nextCursor null (no full page).
    expect(page.items).toHaveLength(25);
    expect(page.nextCursor).toBeNull();
    db.close();
  });
});

describe("countTable (demo fixture)", () => {
  it("counts rows, with an optional filter", () => {
    const db = fixtureDb();
    expect(countTable(db, "demo")).toBe(25);
    expect(countTable(db, "demo", "kind = ?", ["function"])).toBe(25);
    expect(countTable(db, "demo", "kind = ?", ["class"])).toBe(0);
    db.close();
  });
});
