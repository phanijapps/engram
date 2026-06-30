# Architecture Divergence Tracker

This document tracks where the implementation diverges from
`docs/research/architecture-design-v2.md` and what closes each gap. It is a
working engineering ledger, not a replacement for ADRs or specs.

## Scale

- `100%`: the architecture boundary is enforced by code, docs, and tests.
- `75%`: the main contract exists and at least one adapter proves the path, but
  some callers still rely on compatibility shims or test-only composition.
- `50%`: the concept exists in contracts and partial implementation, but the
  boundary is not yet enforced.
- `<50%`: mostly research, draft model, or isolated prototype behavior.

## Selected Divergence Areas

| Area | Target | Current | Divergence | Closing condition | Status |
|------|--------|---------|------------|-------------------|--------|
| Memory/knowledge separation | Memory storage and knowledge storage are separate replaceable concerns. Retrieval composes them without merging persistence. | `engram-memory` and `engram-knowledge` own separate ports. `adapters/memory/sqlite` is memory-only. `adapters/knowledge/inmem` owns graph/ontology test storage. `core/retrieval` owns shared fan-in, fusion, final limit, omission, and degraded-source composition. | The memory fixture still carries source/document/chunk state for quick retrieval smoke tests, and no durable knowledge document or graph backend exists yet. `engram-core` keeps compatibility re-exports from `core/orchestration`. | Durable knowledge graph/document adapters exist and compatibility imports are no longer needed by downstream crates. | `90%` |
| Rust crate modularity | Small crates own one reason to change: domain, runtime primitives, memory ports, knowledge ports, retrieval, graph/ontology adapters, SQL memory adapters, vector adapters. | Split crates exist under responsibility groups: `core/` for storage-neutral crates, `adapters/` for replaceable infrastructure, and `bindings/` for native bridges. Production memory, ingestion, vector, and retrieval code import canonical boundary crates directly where possible. | `core/orchestration` still owns belief, hierarchy, consolidation, and evaluation ports. `adapters/memory/inmem` is still broad because it proves memory, hierarchy, belief, consolidation, and retrieval fixtures. Crate package names still use their pre-move names. | Later specs split belief, hierarchy, consolidation, and evaluation ports from `core/orchestration`; in-memory fixtures can then be split further by behavior, and package names can be renamed once compatibility is planned. | `90%` |

## Current Alignment Snapshot

| v2 Architecture Item | Implementation State | Gap |
|----------------------|----------------------|-----|
| Memory and knowledge are separate but composable | Separate memory and knowledge port crates exist; graph/ontology test storage is outside the memory fixture; shared retrieval composition lives in `engram-retrieval`. | Durable knowledge document and graph backends are not implemented yet. |
| Knowledge graph with ontology semantics | `KnowledgeGraph`, ontology domain records, and repository ports exist. | No durable graph backend yet. |
| Storage layer supports SQL/vector/graph separation | SQL memory and vector adapters exist under `adapters/`; graph ports exist. | No durable graph adapter yet. |
| SKOS taxonomy evolution | Taxonomy contract exists. | Evolution pipeline is not implemented as governed workflow. |
| Hierarchical memory and HiRAG | Hierarchy contracts and in-memory hierarchy slices exist. | Construction/navigation are not yet split into dedicated crates. |
| Belief network and sleep cycle | Belief, contradiction, and consolidation slices exist. | Still mostly adapter-local and `engram-core`-anchored. |

## Immediate Closure Plan

1. Rename adapter packages after compatibility planning, for example
   `engram-store-sql` to a memory-SQLite name and `engram-store-vector` to a
   retrieval-sqlite-vec name.
2. Split deterministic ingest orchestration from concrete filesystem and Git
   readers so pure ingest behavior can move from `adapters/ingest` to core.
3. Keep `engram-store-knowledge-memory` focused on knowledge, graph, and
   ontology conformance tests until a durable graph/document backend is added.
4. Keep `engram-store-memory` as a quick memory test fixture and stop adding
   graph/ontology behavior to it.
5. Add durable knowledge document and graph adapters behind `engram-knowledge`
   ports when the graph storage spec is accepted.
6. Track future splits for `engram-belief`, `engram-hierarchy`, and
   `engram-consolidation` before adding production-grade belief or sleep-cycle
   behavior.
