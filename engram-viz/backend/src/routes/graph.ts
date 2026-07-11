//! GET /api/graph — the single hero payload: nodes + links + community labels.
//!
//! The graph is the *call graph*: only entities that participate in at least one
//! `calls` edge appear as nodes. Nodes are colored by Louvain community and sized
//! by degree (number of incident calls edges). Links are calls edges keyed by the
//! stable entity id (falling back to name when the relationship ref lacks an id).

import { Hono } from "hono";
import {
  engine,
  type GraphLink,
  type GraphNode,
  type EntityRef,
} from "../lib/engine.ts";

export const graphRoute = new Hono();

/** entity_key mirror: prefer id, fall back to name. */
function keyOf(ref: EntityRef): string | null {
  if (ref.id) return ref.id;
  return ref.name ?? null;
}

graphRoute.get("/", (c) => {
  const relationships = engine.relationships();
  const communities = engine.communities();

  // Build the calls edge list and the set of node ids it spans.
  const links: GraphLink[] = [];
  const degree = new Map<string, number>();
  const nodeIds = new Set<string>();

  for (const r of relationships) {
    if (r.predicate !== "calls") continue;
    const source = keyOf(r.subject);
    const target = keyOf(r.object);
    if (!source || !target || source === target) continue;
    links.push({ source, target });
    nodeIds.add(source);
    nodeIds.add(target);
    degree.set(source, (degree.get(source) ?? 0) + 1);
    degree.set(target, (degree.get(target) ?? 0) + 1);
  }

  const nodes: GraphNode[] = [];
  let communityCount = 0;
  for (const id of nodeIds) {
    const entity = engine.entityByKey(id);
    const loc = entity?.sourceRefs?.find((r) => r.location)?.location;
    const community = communities[id];
    if (typeof community === "number") communityCount++;
    nodes.push({
      id,
      name: entity?.name ?? id,
      kind: entity?.kind ?? "unknown",
      file: loc?.path,
      line: loc?.startLine,
      endLine: loc?.endLine,
      community,
      degree: degree.get(id) ?? 0,
    });
  }

  return c.json({
    nodeCount: nodes.length,
    edgeCount: links.length,
    communityCount,
    nodes,
    links,
  });
});
