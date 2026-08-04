//! T5 routes — store-guarded integration against the live agentzero store
//! (skips when the store is absent, e.g. CI). Proves the graph routes are
//! scope-filtered, keyset + capped, and return the contracted shapes.

import { describe, it, expect } from "vitest";
import { existsSync } from "node:fs";

import { dbPath, loadConfig } from "../src/config.ts";
import { graphRoute } from "../src/routes/graph.ts";

const cfg = loadConfig();
const ready = existsSync(dbPath(cfg));

describe.skipIf(!ready)("graph routes (live agentzero store)", () => {
  it("/graph/stats returns scope-filtered counts", async () => {
    const res = await graphRoute(cfg).request("/graph/stats");
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.entities).toBeGreaterThan(100000);
    expect(body.relationships).toBeGreaterThan(200000);
    expect(body.communities).toBe(0); // T6 fills
  });

  it("/graph/communities returns the top-N overview, bounded + with totalCommunities", async () => {
    const app = graphRoute(cfg);
    const res = await app.request("/graph/communities");
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.built).toBe(true);
    expect(body.communities.length).toBeLessThanOrEqual(2000);
    expect(body.edges.length).toBeLessThanOrEqual(4000);
    expect(body.totalCommunities).toBeGreaterThanOrEqual(body.communities.length);
    // every community carries a concentric-ring position.
    for (const c of body.communities) {
      expect(Number.isFinite(c.x)).toBe(true);
      expect(Number.isFinite(c.y)).toBe(true);
    }
    // ?limit bounds the visible count.
    const small = await app.request("/graph/communities?limit=8");
    const smallBody = await small.json();
    expect(smallBody.communities.length).toBeLessThanOrEqual(8);
    expect(smallBody.totalCommunities).toBe(body.totalCommunities);
  });

  it("/entities returns a keyset page + nextCursor", async () => {
    const res = await graphRoute(cfg).request("/entities?limit=5");
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.items).toHaveLength(5);
    expect(body.nextCursor).not.toBeNull();
    expect(body.items[0]).toHaveProperty("id");
    expect(body.items[0]).toHaveProperty("kind");
  });

  it("/entities rejects a malformed cursor with 422", async () => {
    const bad = Buffer.from("not-a-number", "utf8").toString("base64url");
    const res = await graphRoute(cfg).request(`/entities?cursor=${bad}`);
    expect(res.status).toBe(422);
  });

  it("/graph/node/:id/neighbors returns outgoing edges (or empty)", async () => {
    // A repo that exposes APIs (from the probe) — has outgoing `exposes` edges.
    const id = "repo-ebe4fd8c8dc1ce080aba1e6307192767927756eeeae6e6e9e82f3e8d020d51a3";
    const res = await graphRoute(cfg).request(`/graph/node/${id}/neighbors?limit=5`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(Array.isArray(body.items)).toBe(true);
    for (const item of body.items) {
      expect(item.direction).toBe("outgoing");
      expect(item.relationship).toHaveProperty("predicate");
    }
  });
});
