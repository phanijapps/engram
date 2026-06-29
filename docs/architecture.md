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

### Memory Core

Owns the domain model and use-case interfaces.

- `MemoryRecord`: canonical persisted memory unit.
- `MemoryEvent`: append-only event describing creation, update, access, or
  deletion.
- `Provenance`: source, timestamp, actor, confidence, and derivation chain.
- `Policy`: scope, permission, retention, and sensitivity controls.
- `MemoryPort`: write, retrieve, update, consolidate, and forget operations.

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

- Event log for auditability and replay.
- Document store for canonical records.
- Vector index for semantic recall.
- Graph index for relationships and entity memory.

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

## Initial Package Boundaries

```text
packages/core
  Domain types, ports, policies, ranking interfaces.

packages/stores
  Storage adapters and migrations.

packages/connectors
  Integrations with agent frameworks and apps.

packages/evaluations
  Golden datasets, retrieval tests, safety checks, and metrics.
```

## First Vertical Slice

The first implementation slice should prove the end-to-end contract:

1. Accept a memory write request.
2. Persist the record with provenance and policy metadata.
3. Retrieve relevant records for a query.
4. Explain why each result was selected.
5. Run a small evaluation fixture against expected recall behavior.
