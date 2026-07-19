# Research Synthesis: Engram

## Purpose

This document distills the research in this folder into a working architecture
direction for the Agentic Memory layer. It is intentionally shorter than the
source notes and focuses on decisions, design implications, and open questions.

Primary source notes:

- `academic-research-findings.md`
- `philosophical-research-findings.md`
- `hierarchical-taxonomy-research.md`
- `memory-knowledge-architecture.md`
- `architecture-design-v2.md`
- `architecture-diagrams-v2.html`

## Core Thesis

The research converges on a memory layer that is not a single vector database or
conversation history table. It should be a composable memory and knowledge
platform with separate but connected subsystems.

The strongest recurring distinction is:

- **Memory** is the process and structure for managing agent state across time:
  what is active now, what happened before, what has been learned, and how the
  agent should act.
- **Knowledge** is the represented content: facts, concepts, documents,
  taxonomies, entities, relationships, and inferred structure.

This distinction should exist at the interface boundary even when concrete
storage overlaps.

## Canonical Memory Model

The research consistently uses a CoALA-style functional taxonomy rather than a
simple short-term/long-term split.

| Subsystem | Role | Persistence | Retrieval Shape |
|-----------|------|-------------|-----------------|
| Working memory | Active context and task state | Volatile | Read/write current context, evict when over budget |
| Episodic memory | Timestamped events and experience traces | Persistent | Temporal, contextual, cue-based, hierarchical |
| Semantic memory | Durable facts and generalized knowledge | Persistent | Predicate, entity, cue, semantic, hierarchical |
| Procedural memory | Skills, action patterns, tool-selection routines | Persistent | Match current state to action templates |

The key design move is keeping these roles separate, then linking them through
retrieval, provenance, and consolidation.

## Canonical Knowledge Model

Knowledge representation should be layered from simple to structured:

| Representation | Best For | Limitation |
|----------------|----------|------------|
| Flat facts | Low-overhead exact knowledge | No relationships or inference |
| SKOS taxonomy | Hierarchical concepts, facets, controlled vocabulary | Needs governance and evolution policy |
| Vector store | Natural-language similarity over noisy text | Weak at relationships and multi-hop reasoning |
| Knowledge graph | Typed entities, relationships, inference, GraphRAG | More expensive to maintain |

The research argues against treating vector search as the entire memory layer.
Vector retrieval is useful, but it must be composed with structured facts,
taxonomy, graph traversal, and provenance.

## Architecture Direction

The most stable architecture from the research is a five-layer system:

```text
Orchestration Layer
  Agent loop, central executive, internal/external action selection.

Retrieval Layer
  Query routing, ranking, provenance tracking, context composition.

Memory Subsystems
  Working, episodic, semantic, procedural memory.

Knowledge Subsystems
  Flat facts, SKOS taxonomy, vector store, knowledge graph.

Storage Layer
  Volatile context, structured DB, vector index, graph store, archive.
```

The retrieval layer is the integration point. It should fan out to the right
subsystems, merge results, preserve source attribution, and compose a payload
within the working-memory budget.

## v2 Enhancements To Preserve

The v2 architecture adds several ideas that are worth carrying forward into
contracts and implementation.

### 1. Hierarchical Episodic Memory

Episodic memory should not remain a flat event log. The proposed hierarchy is:

```text
Raw event -> Episode -> Schema -> Domain ontology
```

This supports retrieval at the right granularity. A task may need a raw tool
result, an episode summary, a workflow schema, or a domain-level pattern.

### 2. Cue-Based Retrieval

The research uses ACT-R as support for cue-based retrieval: locate chunks by
slot-value cues rather than by opaque ids alone.

Example cue query:

```json
{
  "cues": [
    { "slot": "tool", "value": "search" },
    { "slot": "outcome", "value": "success" }
  ],
  "matchMode": "partial"
}
```

This should complement semantic search, not replace it.

### 3. Construction vs. Navigation

The system should separate:

- **Construction**: how events become episodes, schemas, facts, concepts, and
  graph links.
- **Navigation**: how queries traverse those structures to retrieve minimal
  useful context.

This separation matters because retrieval failures may mean different things:
missing information, weak structure, overloaded context, or bad ranking.

### 4. Dual Indexing

Semantic and episodic memory need at least two structural views:

- Temporal order for recency, chronology, and decay.
- Hierarchical organization for granularity and concept navigation.

Vector similarity and graph traversal become additional views, not replacements
for temporal and hierarchical access.

### 5. SKOS-Aligned Taxonomy

The taxonomy subsystem should follow SKOS concepts where practical:

- URI-identified concepts.
- Concept schemes for domain vocabularies.
- Direct `broader` and `narrower` links only.
- `related` links for associative relationships.
- Collections for facets and non-hierarchical groupings.
- Cross-scheme mappings such as exact or close matches.

The research strongly suggests governance here. Taxonomy evolution should not be
fully autonomous by default.

### 6. Dynamic Taxonomy Evolution

The lifecycle for new or changed concepts should be:

```text
Discovery -> Proposal -> Validation -> Merge
```

Proposal formation breaks down into:

```text
Extract -> Group -> Hierarchize -> Relate
```

Every accepted taxonomy change should carry provenance, including source,
proposal id, validation status, actor or agent, and timestamp.

### 7. Semantic Drift Detection

Drift should be tracked at two levels:

- Concept-level drift: labels, definitions, extensions, identifiers, examples.
- Structural drift: parent-child changes, cross-links, graph relationships.

This should start as an evaluation and reporting concern before becoming an
automated restructuring mechanism.

## Retrieval Model

The retrieval layer should support four first-class modes:

| Mode | Target | Example |
|------|--------|---------|
| Temporal | Episodic memory | What happened in session X? |
| Cue-based | Episodic, semantic, procedural | Find successful runs with tool Y |
| Hierarchical | Episodic, semantic, taxonomy | Retrieve this concept at schema level |
| Semantic | Vector, semantic, graph | Find similar prior situations |

Ranking should account for:

- Relevance to the query or task state.
- Recency.
- Confidence.
- Cue match score.
- Hierarchical granularity.
- Policy and permission filtering.
- Provenance quality.

The context composer should return not only content, but also explanations for
why each memory was included.

## Consolidation Model

The recurring lifecycle is:

```text
Working memory eviction
  -> episodic event record
  -> consolidation into semantic facts
  -> optional taxonomy or graph integration
  -> optional procedural skill update
```

Consolidation should be treated as a separate pipeline, not an incidental side
effect of writes. It needs explicit inputs, outputs, conflict handling, and
evaluation.

## Storage Implications

The research favors a tiered model:

| Tier | Purpose |
|------|---------|
| T1 Context/working | In-process active state |
| T2 Structured DB | events, facts, metadata, policies, taxonomy versions |
| T3 Vector store | embeddings and similarity search |
| T4 Graph store | entities, relationships, ontology/taxonomy edges |
| T5 Archive | raw logs, historical versions, replay data |

For the first implementation, the architecture does not require separate
physical databases for every tier. It requires separate interfaces so the
physical storage can evolve.

## Design Principles

1. **Interfaces before engines**: define ports for memory, retrieval,
   consolidation, taxonomy, and stores before picking concrete databases.
2. **Provenance everywhere**: every memory, fact, retrieval result, and taxonomy
   change needs source, time, actor, confidence, and derivation data.
3. **Policy as a retrieval concern**: visibility, retention, sensitivity, and
   deletion rules must filter retrieval, not only writes.
4. **Composable adapters**: agent frameworks, storage engines, model providers,
   and evaluation harnesses should be replaceable.
5. **Evaluations are part of the architecture**: memory quality is a system
   behavior, not a unit-test afterthought.
6. **Start with one vertical slice**: avoid implementing all cognitive modules
   at once. Prove write, retrieve, explain, evaluate, and forget first.

## Proposed First Vertical Slice

The first build should validate the smallest useful memory loop:

1. Accept an observation or explicit fact with scope, provenance, and policy.
2. Persist it as an event-backed memory record.
3. Retrieve by a combination of text query and structured cues.
4. Rank and return explanations with provenance.
5. Apply policy filtering before composing context.
6. Run a tiny evaluation fixture that checks expected recall and forbidden
   recall.

This slice exercises the architecture without requiring full taxonomy evolution,
GraphRAG, procedural learning, or predictive retrieval.

## Decisions Ready For ADRs

These are sufficiently supported by the research and can be promoted into ADRs:

- Adopt the four memory subsystems: working, episodic, semantic, procedural.
- Treat memory and knowledge as separate interface axes.
- Make retrieval a dedicated layer with routing, ranking, provenance, and
  composition.
- Require provenance and policy metadata on memory records.
- Use cue-based retrieval as a first-class mode alongside semantic search.
- Represent taxonomy concepts in a SKOS-aligned shape.
- Keep storage adapters behind ports rather than binding the architecture to a
  single database.

## Open Decisions

These should remain unresolved until implementation constraints are clearer:

- First language and framework.
- Whether the first runtime is a library, HTTP service, CLI, or agent-framework
  plugin.
- Whether event sourcing is mandatory in the first slice or introduced later.
- Local development storage: SQLite-only, Postgres, embedded vector index, or a
  multi-store setup.
- Consolidation trigger policy: time-based, event-count-based, failure-driven,
  explicit command, or hybrid.
- Taxonomy validation policy: human review, rule-based checks, confidence
  thresholds, or staged approval.
- How procedural memory should update without corrupting useful skills.
- Multi-agent or shared-memory semantics.

## Immediate Next Steps

1. Convert the ready decisions into ADRs.
2. Expand the existing JSON schemas for cue queries, retrieval results,
   provenance, policy, and taxonomy concepts.
3. Choose the first implementation surface and language using the ADR criteria.
4. Build the first vertical slice with evaluation fixtures before adding more
   storage engines or agent connectors.

## Future Knowledge Source Extension

Code repositories and unstructured documents should extend the knowledge layer
through source and ingestion adapters. They should not be baked into core memory.

The future model is:

```text
KnowledgeSource
  -> SourceDocument
  -> KnowledgeChunk
  -> Embedding
  -> Entity / Relationship
  -> RetrievedKnowledge
```

This keeps `MemoryRecord` focused on agent experience while `KnowledgeChunk`
represents source-grounded content from files, documents, repositories, URLs, or
other corpora. The two connect through provenance and retrieval composition.

See `docs/rfcs/0002-knowledge-source-extension.md`.
