import {
  createNativeIngestTransport,
  createNativeKnowledgeTransport,
  createNativeMemoryTransport,
  createNativeRetrievalTransport,
  type NativeIngestTransport,
  type NativeKnowledgeTransport,
  type NativeMemoryTransport,
  type NativeRetrievalTransport,
} from "@engram/node";

// One Rust-backed engine is held for the process lifetime so write, retrieve,
// and forget observe the same in-memory SQLite state.
let transport: NativeMemoryTransport | null = null;

export function getTransport(): NativeMemoryTransport {
  if (transport === null) {
    transport = createNativeMemoryTransport();
  }
  return transport;
}

// One Rust-backed knowledge + taxonomy engine for graph and taxonomy state.
let knowledge: NativeKnowledgeTransport | null = null;

export function getKnowledgeTransport(): NativeKnowledgeTransport {
  if (knowledge === null) {
    knowledge = createNativeKnowledgeTransport();
  }
  return knowledge;
}

// One Rust-backed ingest + extract engine.
let ingest: NativeIngestTransport | null = null;

export function getIngestTransport(): NativeIngestTransport {
  if (ingest === null) {
    ingest = createNativeIngestTransport();
  }
  return ingest;
}

// One Rust-backed semantic-retrieval engine (FastEmbed + sqlite-vec). The first
// call constructs the BGE-small model, which may download assets on first run.
let retrieval: NativeRetrievalTransport | null = null;

export function getRetrievalTransport(): NativeRetrievalTransport {
  if (retrieval === null) {
    retrieval = createNativeRetrievalTransport();
  }
  return retrieval;
}
