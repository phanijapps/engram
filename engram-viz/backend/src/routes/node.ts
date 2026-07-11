//! GET /api/node/:id — entity detail + callers/callees + community + source.
//!
//! The :id may be an entity id or (for the occasional name-keyed analytics
//! entry) a bare name; name keys are resolved to an entity id first.
//! Callers/callees are computed from the calls edges that touch this entity.

import { Hono } from "hono";
import { engine } from "../lib/engine.ts";

export const nodeRoute = new Hono();

interface Neighbor {
  id: string;
  name: string;
  kind: string;
  file?: string;
}

function neighborOf(id: string): Neighbor {
  const entity = engine.entityByKey(id);
  const loc = entity?.sourceRefs?.find((r) => r.location)?.location;
  return {
    id: entity?.id ?? id,
    name: entity?.name ?? id,
    kind: entity?.kind ?? "unknown",
    file: loc?.path,
  };
}

nodeRoute.get("/:id", (c) => {
  const rawId = c.req.param("id");
  const resolvedId = engine.resolveEntityId(rawId);
  const entity = engine.getEntity(resolvedId);
  if (!entity) {
    return c.json({ error: `entity not found: ${rawId}` }, 404);
  }

  // All call-graph keys that refer to this entity: its id and its name.
  const selfKeys = new Set<string>([resolvedId]);
  if (entity.name) selfKeys.add(entity.name);

  const relationships = engine.relationships();
  const callers: Neighbor[] = [];
  const callees: Neighbor[] = [];
  const seenCaller = new Set<string>();
  const seenCallee = new Set<string>();

  for (const r of relationships) {
    if (r.predicate !== "calls") continue;
    const subjectId = r.subject.id ?? r.subject.name ?? null;
    const objectId = r.object.id ?? r.object.name ?? null;
    if (objectId && selfKeys.has(objectId) && subjectId && !seenCaller.has(subjectId)) {
      seenCaller.add(subjectId);
      callers.push(neighborOf(subjectId));
    }
    if (subjectId && selfKeys.has(subjectId) && objectId && !seenCallee.has(objectId)) {
      seenCallee.add(objectId);
      callees.push(neighborOf(objectId));
    }
  }

  const communities = engine.communities();
  const community =
    communities[resolvedId] ?? (entity.name ? communities[entity.name] : undefined) ?? null;

  const chunk = engine.chunkForEntity(resolvedId);
  const source = chunk?.text ?? null;
  const complexity = source ? engine.cyclomaticComplexity(source) : null;

  const loc = entity.sourceRefs?.find((r) => r.location)?.location;

  return c.json({
    entity: {
      id: entity.id,
      name: entity.name,
      kind: entity.kind,
      file: loc?.path,
      line: loc?.startLine,
      endLine: loc?.endLine,
      validFrom: entity.validFrom ?? entity.createdAt,
      validUntil: entity.validUntil ?? null,
      provenance: entity.provenance,
    },
    source,
    complexity,
    community,
    callers,
    callees,
  });
});
