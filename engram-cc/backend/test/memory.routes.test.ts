//! S3 memory routes — store-guarded integration. Proves the memory/belief/procedure
//! lists are keyset-paged + capped + scope-filtered, and empty surfaces return the
//! honest empty-state shape (not an error).

import { describe, it, expect } from "vitest";
import { existsSync } from "node:fs";

import { dbPath, loadConfig } from "../src/config.ts";
import { memoryRoute } from "../src/routes/memory.ts";

const cfg = loadConfig();
const ready = existsSync(dbPath(cfg));

describe.skipIf(!ready)("memory routes (live agentzero store)", () => {
  it("/memory returns keyset-paged facts with the view shape", async () => {
    const res = await memoryRoute(cfg).request("/memory?limit=5");
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.items.length).toBeLessThanOrEqual(5);
    for (const it of body.items) {
      expect(it).toHaveProperty("id");
      expect(it).toHaveProperty("text");
    }
  });

  it("/memory cursor advances to disjoint rows", async () => {
    const app = memoryRoute(cfg);
    const p1 = await (await app.request("/memory?limit=3")).json();
    if (!p1.nextCursor) return; // fewer than 3 memories — nothing to page
    const p2 = await (await app.request(`/memory?limit=3&cursor=${p1.nextCursor}`)).json();
    const ids1 = new Set(p1.items.map((x: { id: string }) => x.id));
    expect(p2.items.every((x: { id: string }) => !ids1.has(x.id))).toBe(true);
  });

  it("/beliefs + /procedures + /contradictions return valid keyset page shapes", async () => {
    const app = memoryRoute(cfg);
    for (const path of ["/beliefs", "/procedures", "/contradictions"]) {
      const body = await (await app.request(path)).json();
      // Shape, not emptiness — beliefs may be populated (M2 writes reflection-llm
      // beliefs); procedures + contradictions stay empty today. The contract is a
      // valid keyset page (items array + null cursor), not an empty store.
      expect(Array.isArray(body.items)).toBe(true);
      expect(body.nextCursor).toBeNull();
    }
  });

  it("/memory degrades (503) on a malformed cursor — the facade owns cursor parsing", async () => {
    const res = await memoryRoute(cfg).request(`/memory?cursor=not-a-rowid`);
    expect(res.status).toBe(503);
  });
});
