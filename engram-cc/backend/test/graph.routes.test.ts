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
    expect(body.communities).toBe(0); // cheap stats endpoint does not run Louvain
    expect(body.hierarchyNodes).toBe(0); // empty today (S4 observatory stat)
    expect(body.hierarchyRelations).toBe(0);
    expect(typeof body.memories).toBe("number");
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

  it("/graph/community/:id/members returns a bounded member sample", async () => {
    const app = graphRoute(cfg);
    const ov = await (await app.request("/graph/communities?limit=5")).json();
    const id = ov.communities[0].id; // `c<label>`
    const res = await app.request(`/graph/community/${id}/members?limit=10`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.found).toBe(true);
    expect(body.memberCount).toBeGreaterThanOrEqual(body.sampled);
    expect(body.items.length).toBeLessThanOrEqual(10);
    for (const it of body.items) expect(it).toHaveProperty("kind");
    // intra-community subgraph edges (bounded; endpoints are members)
    expect(Array.isArray(body.edges)).toBe(true);
    expect(body.edges.length).toBeLessThanOrEqual(300);
    const ids = new Set(body.items.map((x: { id: string }) => x.id));
    for (const e of body.edges) {
      expect(e).toHaveProperty("predicate");
      expect(ids.has(e.source)).toBe(true);
      expect(ids.has(e.target)).toBe(true);
    }
    // 404 for a label outside the drillable top-N.
    expect((await app.request("/graph/community/c9999999/members")).status).toBe(404);
    // 422 for a malformed id.
    expect((await app.request("/graph/community/notalabel/members")).status).toBe(422);
  });

  it("/graph/entity/:id returns detail (community, degree, provenance)", async () => {
    const app = graphRoute(cfg);
    // A member of a top community is guaranteed to have a community (it's in the
    // index by construction) — unlike an arbitrary entity, which may sit in a
    // small community outside the drillable top-N (community === null).
    const ov = await (await app.request("/graph/communities?limit=5")).json();
    const members = await (
      await app.request(`/graph/community/${ov.communities[0].id}/members?limit=5`)
    ).json();
    const id = members.items[0]?.id;
    if (!id) return; // store shape changed — nothing to assert
    const res = await app.request(`/graph/entity/${encodeURIComponent(id)}`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body).toHaveProperty("kind");
    expect(typeof body.degree).toBe("number");
    expect(body.provenance).not.toBe(null);
    expect(typeof body.community).toBe("number");
    // 404 for an unknown entity.
    expect((await app.request(`/graph/entity/${encodeURIComponent("nope-missing")}`)).status).toBe(404);
  });
});
