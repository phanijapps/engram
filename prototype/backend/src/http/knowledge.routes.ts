// Knowledge-graph routes — entity/relationship/graph CRUD, traversal, the
// explorer overview + graph-data projection, and per-repo stats.

import type { Hono } from "hono";
import { getKnowledgeTransport } from "../adapters/engram.client.js";
import { computeGraphData } from "../services/graph.service.js";
import { SCAN_SCOPE } from "../data/scan-defaults.js";

// Default to structural kinds (repo/module/class/function) — less noisy.
// Pass kinds: ["*"] or omit to see everything.
const DEFAULT_KINDS = ["function", "method", "class", "struct", "module", "repository", "project", "organization", "trait", "enum", "interface"];

export function registerKnowledgeRoutes(app: Hono): void {
  app.post("/knowledge/entity", async (c) => {
    const request = await c.req.json();
    return c.json(await getKnowledgeTransport().putEntity(request));
  });
  app.post("/knowledge/relationship", async (c) => {
    const request = await c.req.json();
    return c.json(await getKnowledgeTransport().putRelationship(request));
  });
  app.post("/knowledge/graph", async (c) => {
    const request = await c.req.json();
    return c.json(await getKnowledgeTransport().putGraph(request));
  });
  app.post("/knowledge/neighbors", async (c) => {
    const { graphId, nodeId, scope, limit } = await c.req.json();
    return c.json(await getKnowledgeTransport().neighbors(graphId, nodeId, scope, limit));
  });

  // Whole-graph overview for the explorer: every graph (source/repo), entity,
  // and relationship visible to `scope`. Clustering + cross-repo linking are
  // computed client-side from these lists.
  app.post("/knowledge/overview", async (c) => {
    const { scope } = await c.req.json();
    const transport = getKnowledgeTransport();
    const [graphs, entities, relationships] = await Promise.all([
      transport.listGraphs(scope),
      transport.listEntities(scope),
      transport.listRelationships(scope),
    ]);
    return c.json({ graphs, entities, relationships });
  });

  // Lightweight graph data for rendering: only id/name/kind/graphId for
  // entities and subject/predicate/object for relationships. Server-side degree
  // computation + top-N filtering for performance.
  app.post("/knowledge/graph-data", async (c) => {
    const body = await c.req.json().catch(() => ({}));
    const reqScope = body.scope ?? SCAN_SCOPE;
    const maxNodes = typeof body.limit === "number" ? body.limit : 500;
    const kinds: string[] | null = Array.isArray(body.kinds) && body.kinds.length > 0
      ? (body.kinds as string[]).includes("*") ? null : (body.kinds as string[])
      : DEFAULT_KINDS;
    const transport = getKnowledgeTransport();
    // Load entities + relationships. These can be large — handle errors gracefully.
    let entList: Array<Record<string, unknown>> = [];
    let relList: Array<Record<string, unknown>> = [];
    try {
      [entList, relList] = (await Promise.all([
        transport.listEntities(reqScope),
        transport.listRelationships(reqScope),
      ])) as Array<Record<string, unknown>>[];
    } catch (e) {
      return c.json({ nodes: [], edges: [], total: 0, capped: false, error: String(e) });
    }
    return c.json(computeGraphData(entList, relList, { kinds, maxNodes }));
  });

  // Stats: per-repo summary from KnowledgeSource records (O(repos), not O(entities)).
  app.post("/knowledge/stats", async (c) => {
    const { scope } = await c.req.json();
    const reqScope = scope ?? SCAN_SCOPE;
    const transport = getKnowledgeTransport();
    const sources = (await transport.listSources(reqScope)) as Array<Record<string, unknown>>;
    const repos = sources.map((s) => {
      const name = String(s.name ?? "");
      // Parse git metadata from enriched name: "scan:repo [remote@branch:sha]"
      const gitMatch = name.match(/\[(.+?)@(.+?):(.+?)\]/);
      return {
        id: String(s.id ?? name),
        name: name.replace(/\s*\[.+$/, "").replace(/^scan:/, ""),
        gitRemote: gitMatch?.[1] ?? null,
        gitBranch: gitMatch?.[2] ?? null,
        gitSha: gitMatch?.[3] ?? null,
        lastUpdated: (s.updatedAt ?? s.createdAt) ?? null,
      };
    });
    return c.json({
      tenant: reqScope,
      repos,
      totalRepos: repos.length,
    });
  });
}
