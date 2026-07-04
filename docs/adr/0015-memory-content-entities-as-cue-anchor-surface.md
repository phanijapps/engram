# ADR-0015: Memory cue anchor surface: MemoryContent.entities

- **Status:** Accepted
- **Date:** 2026-07-03
- **Decision-makers:** @phanijapps
- **Supersedes:** none
- **Related:** ADR-0009 (retrieval composition seam), ADR-0014 (memory embedding surface)

## Context

`RetrievalMode::Cue` and the `Cue` / `CueOperator` domain types are fully
defined in `core/domain`. The `temporal-cue-retrieval` spec was shipped
against the now-retired in-memory adapter. No durable adapter currently
dispatches `RetrievalMode::Cue`: `SqlMemoryService::retrieve()` ignores
`request.modes` and runs keyword scan only; `score.cue_match` is always
`None`.

`MemoryContent` already carries `entities: Vec<EntityRef>` — defined in the
domain model but never populated anywhere in the codebase (write path,
ingestor, and consolidation stubs all leave it empty).

Memora (Microsoft Research, ICML 2026) identifies "cue anchors" — organically
extracted entity/topic tags from memory content — as the mechanism that enables
multi-hop retrieval without rigid ontologies. A query about a person, project,
or topic finds related memories not because they are semantically similar but
because they share a named anchor. This is exactly what `Cue`-mode retrieval
in Engram is designed to do; what is missing is the extraction side that
populates the anchors.

The question is where extracted anchors live in the domain model. Candidates
are: a new dedicated table, the `metadata` map, or `content.entities`.

## Decision

> `MemoryContent.entities` is the canonical backing store for memory cue
> anchors. Entity extraction at write time (or by a consolidation pass) MUST
> populate this field so that `RetrievalMode::Cue` can dispatch against it in
> the SQLite adapter.

Specifically:

- Entity extraction runs on `MemoryContent.text` at write time, producing
  `EntityRef` values (at minimum: `name`, inferred `kind`) deposited into
  `content.entities`.
- The SQLite adapter's cue dispatch MUST match `Cue.slot = "entity"` against
  `content.entities[].name` using the specified `CueOperator`. Additional
  slots (e.g. `"kind"`) map to `content.entities[].kind`.
- Extraction that requires a model call MAY be deferred to a consolidation
  pass; a rule-based first pass (noun phrases, capitalized tokens, known-entity
  patterns) is sufficient for the initial implementation.
- `MemoryLink` remains the mechanism for linking a `MemoryRecord` to a
  canonical `KnowledgeEntity` record. `content.entities` carries lightweight
  inline anchors for cue retrieval; it is not a replacement for graph-backed
  entity resolution.

## Decision drivers

- **Reuse over new storage** — `content.entities` is already in the accepted
  v1 schema; no migration, no new table, no schema version bump required.
- **Retrieval symmetry** — `KnowledgeChunk.entities` already carries entity
  refs for knowledge retrieval. Using the same field on `MemoryContent` keeps
  the pattern consistent across memory and knowledge records.
- **Avoiding metadata bypass** — storing anchors in `metadata` would make them
  untyped and invisible to the query planner; a named field is indexable and
  auditable.

## Consequences

**Positive:**
- `RetrievalMode::Cue` gains a concrete, durable backing store in the SQLite
  adapter without a schema change.
- Multi-hop retrieval (e.g. "find all memories mentioning Project Orion") works
  via `Cue` matching against `content.entities`, complementing semantic
  similarity.
- The pattern is consistent: chunk entities, memory entities, and cue dispatch
  all use the same `EntityRef` shape.

**Negative:**
- Write callers that omit entity extraction produce records with no cue anchors,
  silently degrading cue-mode retrieval for those records.
- A rule-based extractor will miss entities not matching its patterns; precision
  improves only when an LLM-backed extractor is added later.
- `content.entities` serves double duty as an inline anchor store and a
  lightweight entity mention list; if entity resolution becomes load-bearing,
  the field semantics may need tightening in a future ADR.

**Revisit if:** Cue retrieval demand grows to require dedicated indexing
(e.g. a full-text index or inverted index on entity names) that cannot be
served efficiently by scanning `content.entities` in the SQLite adapter; or if
inline entity refs and graph-backed `KnowledgeEntity` records need cleaner
separation.

## Confirmation

- **Mode:** reviewer-checked
- **Signal:** Any PR implementing cue retrieval in the SQLite adapter reads
  from `content.entities`; any PR adding write-path or consolidation entity
  extraction writes to `content.entities`. Neither uses `metadata` as a bypass.
- **Owner:** PR reviewer on cue retrieval and entity extraction implementations.

## Alternatives considered

**Dedicated `cue_anchor` table in the SQLite adapter**
Rejected: adds a new adapter-local table with no domain-model representation,
making anchors invisible to portable contract consumers. `content.entities`
is already in the v1 schema and serves the same purpose without schema drift.

**Store anchors in `MemoryContent.metadata`**
Rejected on driver "avoiding metadata bypass": metadata values are untyped
strings, cannot be indexed declaratively, and violate the domain model rule
that core semantics use typed fields rather than metadata conventions.

**Require callers to supply entities at write time (no extraction)**
Rejected: cue anchors must be organically extracted to be useful — callers
writing memories don't know in advance which entity slots will be needed for
future retrieval. A system-side extraction step (even rule-based) is necessary.

## References

- Memora: A Harmonic Memory Representation Balancing Abstraction and
  Specificity — Microsoft Research, ICML 2026.
  https://arxiv.org/abs/2602.03315
- ADR-0009: retrieval composition seam (establishes `RetrievalIndex` port
  that cue dispatch will extend)
- ADR-0014: memory record embedding surface (companion decision on the
  write-time enrichment pattern)
