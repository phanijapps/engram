// Shared Q&A domain types.
//
// Both the evidence builder (services) and the embeddings client (adapters)
// depend on these shapes. Keeping them in a dedicated module breaks what would
// otherwise be a cycle between the embeddings adapter and the Q&A service:
// the adapter type-imports QaChunk from here, the services import the same
// types plus the value exports.

export type QaSource = {
  kind: "memory" | "belief" | "entity" | "relationship" | "chunk";
  id: string;
  text: string;
  source: string;
};

export type QaResult = {
  answer: string;
  sources: QaSource[];
  llm: "ok" | "unavailable" | "error";
};

export type MemoryItem = {
  targetId?: string;
  content?: { text?: string };
  provenance?: { source?: string };
};
export type QaBelief = {
  id: string;
  subject: { key: string };
  content: string;
  provenance?: { source?: string };
};
export type QaEntity = {
  id: string;
  graphId?: string;
  kind?: string;
  name: string;
  provenance?: { source?: string };
  sourceRefs?: { location?: { path?: string } }[];
};
export type QaRelationship = {
  id: string;
  graphId?: string;
  subject: { id?: string; name?: string; kind?: string };
  predicate: string;
  object: { id?: string; name?: string; kind?: string };
  provenance?: { source?: string };
};
export type QaChunk = {
  id: string;
  text: string;
  documentId?: string;
  entities?: { id?: string }[];
};
