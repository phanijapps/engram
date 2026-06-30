# Architecture

The memory layer is designed as a composable system with stable ports and
replaceable adapters.

The domain contract is drafted separately in `docs/domain-data-model.md`. Once
accepted, implementation types, JSON schemas, bindings, and storage schemas
should derive from that model.

## System Responsibilities

- Ingest observations, user facts, task traces, artifacts, and feedback.
- Normalize memories into durable records with provenance and permissions.
- Retrieve relevant memories for an agent context with explainable ranking.
- Consolidate noisy event history into durable knowledge.
- Forget, redact, or downgrade memories according to policy.
- Evaluate quality and safety across realistic agent workflows.

## Core Modules

### Domain Contract

Owns portable data shapes and storage-neutral semantics.

- `MemoryRecord` and `MemoryEvent`: agent state and lifecycle.
- `KnowledgeSource`, `SourceDocument`, and `KnowledgeChunk`: source-grounded
  corpus content.
- `KnowledgeGraph`, `KnowledgeEntity`, and `KnowledgeRelationship`: typed graph
  content.
- `Ontology`, `OntologyClass`, `OntologyProperty`, and `OntologyAxiom`: graph
  governance and validation vocabulary.
- `Provenance` and `Policy`: source, actor, confidence, permission, retention,
  and sensitivity controls.

### Memory Ports

Own memory service and repository interfaces.

- `MemoryRecord`: canonical persisted memory unit.
- `MemoryEvent`: append-only event describing creation, update, access, or
  deletion.
- `MemoryService`: write, retrieve, and forget operations.
- `MemoryRepository`: memory record persistence.
- `MemoryEventRepository`: lifecycle event reads.

### Knowledge Ports

Own source-grounded knowledge, graph, ontology, and ingestion interfaces.

- `KnowledgeRepository`: sources, documents, chunks, entities, and
  relationships.
- `KnowledgeGraphRepository`: named graph identity and graph traversal.
- `OntologyRepository`: ontology classes, properties, axioms, and validation.
- `SourceReader`, `Chunker`, and `IngestionService`: source extraction and
  normalization.

### Ingestion

Transforms external events into candidate memories.

- Agent messages and tool calls.
- User profile facts and preferences.
- Project artifacts and code context.
- Feedback signals and corrections.

### Retrieval

Returns context for a task using multiple retrieval strategies.

- Semantic vector search.
- Keyword and structured metadata filters.
- Graph traversal for related entities and episodes.
- Recency, confidence, and policy-aware ranking.

### Consolidation

Turns traces into durable knowledge.

- Deduplicate overlapping facts.
- Merge repeated preferences and stable behavior.
- Decay or archive stale memories.
- Track confidence changes over time.

### Storage Adapters

Storage should be swappable behind ports.

- Memory stores for records, lifecycle events, idempotency, and replay.
- Knowledge stores for documents, chunks, entities, relationships, and
  ontologies.
- Vector index for semantic recall.
- Graph index for relationships, ontology-backed graph traversal, and GraphRAG.
- Process-local in-memory adapters are conformance fixtures only. Memory,
  knowledge graph, and ontology fixtures stay in separate crates so production
  backends can diverge by technology.

### Knowledge Source Extension

Code repositories and unstructured documents should enter the system as
knowledge sources, not as special cases inside core memory. Source connectors
scan and read content; ingestion adapters normalize that content into
source-grounded knowledge chunks; retrieval can then compose memories and
knowledge together with provenance.

See `docs/rfcs/0002-knowledge-source-extension.md`.

### Connectors

Connectors adapt external runtimes without leaking framework-specific concepts
into core.

- Agent runtime connector.
- API connector.
- CLI connector.
- Evaluation harness connector.

## Initial Crate Boundaries

```text
crates/engram-domain
  Portable domain types and serialization contracts.

crates/engram-runtime
  Shared errors, result type, clocks, id generation, scope matching, policy gates.

crates/engram-memory
  Memory service, memory repository, lifecycle event repository.

crates/engram-knowledge
  Knowledge repositories, graph repositories, ontology repositories, source
  readers, chunkers, ingestion.

crates/engram-core
  Orchestration facade, retrieval, consolidation, hierarchy, belief, evaluation.

crates/engram-store-memory
  Quick in-memory memory fixture for tests and examples only.

crates/engram-store-knowledge-memory
  Quick in-memory knowledge, graph, and ontology fixture for tests and examples.
```

## First Vertical Slice

The first implementation slice should prove the end-to-end contract:

1. Accept a memory write request.
2. Persist the record with provenance and policy metadata.
3. Retrieve relevant records for a query.
4. Explain why each result was selected.
5. Run a small evaluation fixture against expected recall behavior.
