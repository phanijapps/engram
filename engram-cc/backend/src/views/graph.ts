//! Graph projections — pick the UI-facing fields from engram entity /
//! relationship records. Grounded in the real `record_json` shapes:
//!  - entity:  { id, kind, name, metadata?: { stableSourceKey? }, graph_id? }
//!  - relationship: { subject: { id }, predicate, object: { id }, confidence? }
//! `graph_id` is a column on `knowledge_entities`, not always inside the record;
//! the projection takes it from either spot when present.

import type { GraphEntityView, GraphRelationshipView } from "./types.ts";

interface EntityRecord {
  id: string;
  kind: string;
  name: string;
  metadata?: { stableSourceKey?: string };
  graph_id?: string;
  graphId?: string;
}

interface RelationshipRecord {
  subject: { id: string; name?: string; kind?: string };
  predicate: string;
  object: { id: string; name?: string; kind?: string };
  confidence?: number;
}

export function projectEntity(record: unknown): GraphEntityView {
  const r = record as EntityRecord;
  const view: GraphEntityView = { id: r.id, name: r.name, kind: r.kind };
  const graphId = r.graph_id ?? r.graphId;
  if (graphId !== undefined) view.graphId = graphId;
  const stableSourceKey = r.metadata?.stableSourceKey;
  if (stableSourceKey !== undefined) view.stableSourceKey = stableSourceKey;
  return view;
}

export function projectRelationship(record: unknown): GraphRelationshipView {
  const r = record as RelationshipRecord;
  const view: GraphRelationshipView = {
    source: r.subject.id,
    predicate: r.predicate,
    target: r.object.id,
  };
  if (r.confidence !== undefined) view.confidence = r.confidence;
  return view;
}

/** A neighbor entry on the drill-down: the other endpoint + the edge + direction. */
export interface NeighborEntry {
  entity: { id: string; name?: string; kind?: string };
  relationship: GraphRelationshipView;
  direction: "outgoing" | "incoming";
}

/**
 * Project an outgoing neighbor from a relationship record (the queried node is
 * the subject; the neighbor is the object). Used by `/graph/node/:id/neighbors`,
 * which queries the indexed `subject_id` column. (Incoming neighbors need an
 * unindexed `record_json` scan — deferred.)
 */
export function projectOutgoingNeighbor(record: unknown): NeighborEntry {
  const r = record as RelationshipRecord;
  const entity: NeighborEntry["entity"] = { id: r.object.id };
  if (r.object.name !== undefined) entity.name = r.object.name;
  if (r.object.kind !== undefined) entity.kind = r.object.kind;
  return {
    entity,
    relationship: projectRelationship(r),
    direction: "outgoing",
  };
}
