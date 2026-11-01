# The memory pipeline

<!-- prerequisite: summary -->

## How engram works — the memory pipeline

Data flows through engram in one direction. An agent **writes** (or ingests);
engram **processes and stores** across swappable engine cells; a separate
**retrieval composition** layer fuses many recall modes into one policy-filtered
**context packet** returned to the agent. Storage and retrieval are deliberately
decoupled — exactly Memora's "decouple what is stored from how it is retrieved."

<p align="center"><img src="images/engram-pipeline.svg" alt="Engram — agentic memory pipeline" width="100%"></p>

```
                Agent / host app / MCP client
                   │  write_memory · ingest source · put_entity/relationship
                   ▼
   ┌─────────────────────────────────────────────────────────────────┐
   │  WRITE & INGEST PATH   (policy + provenance stamped on every row)│
   │  extract → chunk → embed → persist memory / knowledge graph       │
   └─────────────────────────────────────────────────────────────────┘
        │ memory lifecycle        │ knowledge graph       │ belief synth   │ hierarchy
        ▼
   ┌─────────────────────────────────────────────────────────────────┐
   │  STORAGE CELLS   — one crate per engine, swapped by config       │
   │  engram-store-sqlite ◄────► engram-store-surreal ◄────► …        │
   │  memory │ knowledge/taxonomy/ontology │ belief │ hierarchy │ vec │
   └─────────────────────────────────────────────────────────────────┘
        │  consolidation (reflection → derived beliefs  ·  decay → expiry)
        │  runs as an explicit offline pipeline over the store
        ▼
   ┌─────────────────────────────────────────────────────────────────┐
   │  RETRIEVAL COMPOSITION   (never one mode)                         │
   │  graph + lexical + vector + cue + beliefs  ─►  RRF fusion         │
   │  ─► cross-encoder rerank  ─►  context packet (with provenance)     │
   └─────────────────────────────────────────────────────────────────┘
        │
        ▼
   Context packet ─► agent   (via Rust facade · N-API/TS · MCP server)
```

The pipeline is the architecture; the sections below name the crates that
implement each stage and the boundaries that keep them swappable.

