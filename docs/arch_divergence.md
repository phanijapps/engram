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
| Memory/knowledge separation | Memory storage and knowledge storage are separate replaceable concerns. Retrieval composes them without merging persistence. | `engram-memory` and `engram-knowledge` own separate ports. `engram-store-sql` is memory-only. `engram-store-knowledge-memory` owns graph/ontology test storage. `engram-store-memory` keeps source/document/chunk records only for quick retrieval fixtures. | The memory fixture still carries source chunk state for retrieval smoke tests, so full production retrieval composition is not yet isolated in a dedicated orchestrator. Compatibility re-exports remain in `engram-core`. | Retrieval composition moves out of the memory fixture; durable knowledge graph/document adapters exist; compatibility imports are no longer needed by production crates. | `85%` |
| Rust crate modularity | Small crates own one reason to change: domain, runtime primitives, memory ports, knowledge ports, retrieval, graph/ontology adapters, SQL memory adapters, vector adapters. | Split crates exist for domain, runtime, memory, knowledge, core, retrieval, SQL, vector, ingest, eval, node, memory fixture, and knowledge fixture. Production memory and ingestion adapters import canonical boundary crates directly where possible. | `engram-core` still owns belief, hierarchy, consolidation, retrieval, and evaluation ports. `engram-store-memory` is still broad because it proves memory, hierarchy, belief, consolidation, and retrieval fixtures. | Later specs split belief, hierarchy, consolidation, and retrieval ports from `engram-core`; in-memory fixtures can then be split further by behavior. | `80%` |

## Current Alignment Snapshot

| v2 Architecture Item | Implementation State | Gap |
|----------------------|----------------------|-----|
| Memory and knowledge are separate but composable | Separate memory and knowledge port crates exist; graph/ontology test storage is outside the memory fixture. | Retrieval composition still partly lives in in-memory memory service tests. |
| Knowledge graph with ontology semantics | `KnowledgeGraph`, ontology domain records, and repository ports exist. | No durable graph backend yet. |
| Storage layer supports SQL/vector/graph separation | SQL memory and vector adapters exist; graph ports exist. | No `engram-store-graph` durable adapter yet. |
| SKOS taxonomy evolution | Taxonomy contract exists. | Evolution pipeline is not implemented as governed workflow. |
| Hierarchical memory and HiRAG | Hierarchy contracts and in-memory hierarchy slices exist. | Construction/navigation are not yet split into dedicated crates. |
| Belief network and sleep cycle | Belief, contradiction, and consolidation slices exist. | Still mostly adapter-local and `engram-core`-anchored. |

## Immediate Closure Plan

1. Move memory adapter imports to `engram-memory` and `engram-runtime` instead
   of relying on `engram-core` re-exports.
2. Move knowledge ingestion and knowledge adapter imports to `engram-knowledge`
   and `engram-runtime`.
3. Keep `engram-store-knowledge-memory` focused on knowledge, graph, and
   ontology conformance tests until a durable graph/document backend is added.
4. Keep `engram-store-memory` as a quick memory test fixture and stop adding
   graph/ontology behavior to it.
5. Move retrieval composition out of `engram-store-memory` when the next
   orchestration slice is specified.
6. Track future splits for `engram-belief`, `engram-hierarchy`, and
   `engram-consolidation` before adding production-grade belief or sleep-cycle
   behavior.
