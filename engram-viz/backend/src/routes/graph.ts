//! GET /api/graph — the single hero payload: nodes + links + community labels.
//!
//! The graph is the *call graph*: only entities that participate in at least one
//! `calls` edge appear as nodes. Nodes are colored by Louvain community and sized
//! by degree (number of incident calls edges). Links are calls edges keyed by the
//! stable entity id (falling back to name when the relationship ref lacks an id).
//!
//! Query params:
//!   source   — stable_source_key of one repo, to filter to that repo.
//!   maxNodes — cap the payload to the top-N nodes by degree (default 2000).
//!              Leaf noise is pruned; links whose endpoints are pruned are dropped.
//!              `degree` on each returned node reflects the *visible* (post-cap)
//!              edge count, so canvas sizing matches what is drawn.

import { Hono } from "hono";
import {
  engine,
  type GraphLink,
  type GraphNode,
  type EntityRef,
} from "../lib/engine.ts";

export const graphRoute = new Hono();

const DEFAULT_MAX_NODES = 2000;

/** entity_key mirror: prefer id, fall back to name. */
function keyOf(ref: EntityRef): string | null {
  if (ref.id) return ref.id;
  return ref.name ?? null;
}

graphRoute.get("/", (c) => {
  const sourceFilter = c.req.query("source"); // optional source ID filter
  const maxNodes = (() => {
    const raw = c.req.query("maxNodes");
    if (!raw) return DEFAULT_MAX_NODES;
    const n = Number.parseInt(raw, 10);
    return Number.isFinite(n) && n > 0 ? n : DEFAULT_MAX_NODES;
  })();

  const relationships = engine.relationships();
  const communities = engine.communities();

  // If filtering by source, build the set of entity IDs for that source.
  let sourceEntityIds: Set<string> | null = null;
  if (sourceFilter) {
    sourceEntityIds = new Set(
      engine.entitiesBySource(sourceFilter).map((e) => e.id),
    );
  }

  // Build the calls edge list and per-node degree.
  const links: GraphLink[] = [];
  const degree = new Map<string, number>();
  const nodeIds = new Set<string>();

  for (const r of relationships) {
    if (r.predicate !== "calls") continue;
    const source = keyOf(r.subject);
    const target = keyOf(r.object);
    if (!source || !target || source === target) continue;
    if (sourceEntityIds) {
      if (!sourceEntityIds.has(source) && !sourceEntityIds.has(target)) continue;
    }
    links.push({ source, target });
    nodeIds.add(source);
    nodeIds.add(target);
    degree.set(source, (degree.get(source) ?? 0) + 1);
    degree.set(target, (degree.get(target) ?? 0) + 1);
  }

  // Slim node payload: id, name, kind, file, community, degree only.
  // (line / endLine / complexity / conceptRefs are served by /api/node/:id.)
  // `degree` here is the pre-cap structural degree, used only to rank nodes
  // for the cap; it is recomputed from visible edges after pruning below.
  const allNodes: GraphNode[] = [];
  for (const id of nodeIds) {
    const entity = engine.entityByKey(id);
    const loc = entity?.sourceRefs?.find((r) => r.location)?.location;
    const community = communities[id];
    const deg = degree.get(id) ?? 0;
    allNodes.push({
      id,
      name: entity?.name ?? id,
      kind: entity?.kind ?? "unknown",
      file: loc?.path,
      community,
      degree: deg,
    });
  }

  // Degree cap: keep the top-N structural hubs, prune leaf noise. Then keep
  // only links whose both endpoints survived the cap.
  let nodes = allNodes;
  let finalLinks = links;
  if (allNodes.length > maxNodes) {
    const kept = new Set(
      [...allNodes]
        .sort((a, b) => b.degree - a.degree)
        .slice(0, maxNodes)
        .map((n) => n.id),
    );
    nodes = allNodes.filter((n) => kept.has(n.id));
    finalLinks = links.filter(
      (l) => kept.has(l.source) && kept.has(l.target),
    );
  }

  // Recompute each node's `degree` from the surviving edges so canvas sizing
  // reflects what is actually drawn (a hub whose leaves were pruned shrinks).
  const visibleDeg = new Map<string, number>();
  for (const l of finalLinks) {
    visibleDeg.set(l.source, (visibleDeg.get(l.source) ?? 0) + 1);
    visibleDeg.set(l.target, (visibleDeg.get(l.target) ?? 0) + 1);
  }
  for (const n of nodes) n.degree = visibleDeg.get(n.id) ?? 0;

  // Tally communityCount over the final (post-cap) node set.
  const communityCount = nodes.filter(
    (n) => typeof n.community === "number",
  ).length;

  return c.json({
    nodeCount: nodes.length,
    edgeCount: finalLinks.length,
    communityCount,
    sourceFilter: sourceFilter || null,
    capped: allNodes.length > maxNodes,
    originalNodeCount: allNodes.length,
    nodes,
    links: finalLinks,
  });
});
