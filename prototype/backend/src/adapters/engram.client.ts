import {
  createNativeBeliefTransport,
  createNativeConsolidationTransport,
  createNativeEvalTransport,
  createNativeHierarchyTransport,
  createNativeIngestTransport,
  createNativeKnowledgeTransport,
  createNativeMemoryTransport,
  createNativeRetrievalTransport,
  type NativeBeliefTransport,
  type NativeConsolidationTransport,
  type NativeEvalTransport,
  type NativeHierarchyTransport,
  type NativeIngestTransport,
  type NativeKnowledgeTransport,
  type NativeMemoryTransport,
  type NativeRetrievalTransport,
} from "@engram/node";

// When ENGRAM_DB is set (the demo server sets it to a shared file), the memory,
// knowledge, and ingest engines open the SAME SQLite file so state persists
// across restarts and the ingest + knowledge engines share graph data. When
// unset (e.g. tests), engines are in-memory.
const dbPath = (): string | null => process.env.ENGRAM_DB ?? null;

/** Sidecar path for the Rust scan manifest (incremental resume), next to the DB. */
export function scanManifestPath(): string | null {
  const db = dbPath();
  return db ? `${db}.scan-manifest.json` : null;
}

// One Rust-backed engine is held for the process lifetime so write, retrieve,
// and forget observe the same SQLite state.
let transport: NativeMemoryTransport | null = null;

export function getTransport(): NativeMemoryTransport {
  if (transport === null) {
    transport = createNativeMemoryTransport({ dbPath: dbPath() });
  }
  return transport;
}

// One Rust-backed knowledge + taxonomy engine for graph and taxonomy state.
let knowledge: NativeKnowledgeTransport | null = null;

export function getKnowledgeTransport(): NativeKnowledgeTransport {
  if (knowledge === null) {
    knowledge = createNativeKnowledgeTransport({ dbPath: dbPath() });
  }
  return knowledge;
}

// One Rust-backed ingest + extract engine. Shares the knowledge file so extracted
// graphs are visible to the knowledge engine.
let ingest: NativeIngestTransport | null = null;

export function getIngestTransport(): NativeIngestTransport {
  if (ingest === null) {
    ingest = createNativeIngestTransport({ dbPath: dbPath() });
  }
  return ingest;
}

// One Rust-backed semantic-retrieval engine (FastEmbed + sqlite-vec). The first
// call constructs the BGE-small model, which may download assets on first run.
// Vectors persist to a durable sqlite-vec file (alongside the knowledge DB) so
// embeddings survive restarts and are reused on re-index.
let retrieval: NativeRetrievalTransport | null = null;

const embeddingsDbPath = (): string | null =>
  process.env.ENGRAM_EMBEDDINGS_DB ??
  (process.env.ENGRAM_DB ? `${process.env.ENGRAM_DB}.embeddings.db` : null);

export function getRetrievalTransport(): NativeRetrievalTransport {
  if (retrieval === null) {
    retrieval = createNativeRetrievalTransport({
      embeddingsDbPath: embeddingsDbPath() ?? undefined,
    });
  }
  return retrieval;
}

// One Rust-backed belief + contradiction engine. Shares the durable SQLite file
// (ENGRAM_DB) so beliefs/contradictions persist across restarts alongside memory
// and knowledge. Belief storage is distinct from knowledge + memory (derived
// stance, not source-grounded evidence).
let belief: NativeBeliefTransport | null = null;

export function getBeliefTransport(): NativeBeliefTransport {
  if (belief === null) {
    belief = createNativeBeliefTransport({ dbPath: dbPath() });
  }
  return belief;
}

let hierarchy: NativeHierarchyTransport | null = null;

export function getHierarchyTransport(): NativeHierarchyTransport {
  if (hierarchy === null) {
    hierarchy = createNativeHierarchyTransport();
  }
  return hierarchy;
}

let consolidation: NativeConsolidationTransport | null = null;

export function getConsolidationTransport(): NativeConsolidationTransport {
  if (consolidation === null) {
    consolidation = createNativeConsolidationTransport();
  }
  return consolidation;
}

let evalTransport: NativeEvalTransport | null = null;

export function getEvalTransport(): NativeEvalTransport {
  if (evalTransport === null) {
    evalTransport = createNativeEvalTransport();
  }
  return evalTransport;
}
