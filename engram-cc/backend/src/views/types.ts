//! View-types — the typed projections the BFF ships to the browser. These are
//! PROJECTIONS of engram's `record_json` shapes (not a parallel domain model):
//! each picks the fields the UI needs from the real record. Graph views are
//! fully projected here in S1; BeliefView / HierarchyNodeView / OntologyView /
//! TaxonomyConceptView are minimal stubs deepened in S3 / S4.

export interface GraphEntityView {
  id: string;
  name: string;
  kind: string;
  graphId?: string;
  stableSourceKey?: string;
  /** Louvain community label — filled by the aggregation layer (T6), not the record. */
  community?: number;
  /** Edge degree — filled downstream, not the record. */
  degree?: number;
}

export interface GraphRelationshipView {
  /** subject entity id */
  source: string;
  predicate: string;
  /** object entity id */
  target: string;
  confidence?: number;
}

export interface CommunityMetaNode {
  id: string;
  name: string;
  memberCount: number;
  /** Precomputed layout coordinate (deterministic). */
  x?: number;
  y?: number;
}

export interface CommunityMetaEdge {
  source: string;
  target: string;
  weight: number;
}

// ---- Minimal stubs (deepened in S3 / S4) -------------------------------

export interface MemoryView {
  id: string;
  kind: string;
  text: string;
  status?: string;
  createdAt?: string;
  source?: string;
  confidence?: number;
}

export interface BeliefView {
  id: string;
  text?: string;
  subject?: string;
  status?: string;
}

export interface ProcedureView {
  id: string;
  text: string;
}

export interface HierarchyNodeView {
  id: string;
  layer?: number;
}

export interface OntologyView {
  id: string;
}

export interface TaxonomyConceptView {
  id: string;
  label?: string;
}
