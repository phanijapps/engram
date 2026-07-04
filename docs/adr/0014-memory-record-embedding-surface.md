# ADR-0014: Memory record embedding surface: summary over full text

- **Status:** Proposed
- **Date:** 2026-07-03
- **Decision-makers:** @phanijapps
- **Supersedes:** none
- **Related:** ADR-0009 (retrieval composition seam), ADR-0006 (SQLite adapter)

## Context

`SqliteVectorIndex` in `adapters/retrieval/sqlite-vec` currently embeds
`KnowledgeChunk.text` — the full chunk content — as the passage vector. No
memory-level vector indexing exists yet; `MemoryRecord` is retrieved today
via keyword scan only.

`MemoryContent` carries two text fields: `text` (required, full content) and
`summary` (optional, purpose currently undefined in the domain model). When
memory-level vector indexing is added, the choice of what to embed is a
durable contract decision: it determines what `EmbeddingRef.contentHash`
covers for a `MemoryRecord`, constrains how the adapter builds its index, and
affects retrieval precision.

Embedding full text is the path of least resistance — it requires no
precondition. But it embeds noise: filler, caveats, qualifications, and
repeated phrases that weaken nearest-neighbor quality. Memora (Microsoft
Research, ICML 2026) shows empirically that embedding only a short abstraction
phrase — not the full content — improves multi-hop recall and halves the
effective memory footprint compared to full-text approaches (344 vs 651
entries; up to 98% fewer tokens than full-context inference on LoCoMo at
86.3% LLM-judge accuracy).

The `summary` field on `MemoryContent` is the natural carrier for this
abstraction. Its role needs to be made explicit before memory embedding is
built so the implementation doesn't default to full text.

## Decision

> When memory-level vector indexing is implemented, the vector adapter MUST
> embed `MemoryContent.summary`, not `MemoryContent.text`.

Specifically:

- `summary` carries a short abstraction phrase (target: ≤ 15 words) capturing
  what the memory is fundamentally about, not a truncation of `text`.
- The `EmbeddingRef` recorded for a `MemoryRecord` MUST set `contentHash` to
  the hash of `summary`, not `text`, so the embedding is auditable and the
  embedded surface is unambiguous.
- If `summary` is absent at embedding time, the adapter MUST NOT embed the
  full `text`. It should log a skip and defer until summary is populated —
  either by the write caller or by a consolidation pass.
- `MemoryContent.text` is stored and returned to callers but is not the
  embedding surface. It is fetched after retrieval surfaces the record via
  its summary vector.

This decision scopes to `MemoryRecord` vector indexing only. `KnowledgeChunk`
embedding (which uses chunk text as the passage) is not changed by this ADR.

## Decision drivers

- **Retrieval signal quality** — shorter, denser abstractions produce
  better nearest-neighbor separation than long, noisy full-text vectors.
- **Provenance clarity** — `EmbeddingRef.contentHash` must unambiguously
  identify what was embedded; mixing text and summary hashes in the same
  field creates silent inconsistency.
- **Pre-emptive correctness** — memory embedding doesn't exist yet; locking
  the surface now prevents the obvious wrong default being built in.

## Consequences

**Positive:**
- Memory vector index is smaller and higher-precision than full-text indexing.
- `EmbeddingRef.contentHash` reliably identifies the embedded surface.
- Aligns memory embedding with the Hierarchy layer, where `HierarchyNode.summary`
  already serves as the aggregate navigation surface.

**Negative:**
- Write callers must populate `summary` or accept that the record won't be
  vector-indexed until a consolidation pass fills it.
- A consolidation task (or write-path helper) is needed to generate summaries
  for records written without one — an additional operational dependency.

**Revisit if:** Embedding models become capable enough that full-text passage
retrieval consistently outperforms abstraction-only retrieval at the
memory-record granularity used in Engram; or if the operational cost of
requiring summary population proves prohibitive.

## Confirmation

- **Mode:** reviewer-checked
- **Signal:** Any PR adding `MemoryRecord` vector indexing uses `content.summary`
  as the embed input and records `summary` hash in `EmbeddingRef.contentHash`.
- **Owner:** PR reviewer on the memory vector indexing implementation.

## Alternatives considered

**Embed `MemoryContent.text` (full content)**
Rejected on retrieval signal quality: full-text vectors for memory records
carry noise that weakens nearest-neighbor precision. Memora benchmarks
validate this — embedding only the abstraction phrase outperforms full-text
approaches on multi-hop tasks.

**Embed both `summary` and `text` as separate vectors**
Rejected: adds adapter complexity (two index entries per record), requires a
fusion strategy to merge two vectors for the same record, and the incremental
benefit over summary-only is unproven. Can be revisited if query-time
evidence shows a gap.

**Let the adapter choose at runtime**
Rejected on provenance grounds: `EmbeddingRef.contentHash` would be
ambiguous — callers could not determine what was embedded without inspecting
adapter internals. The embedding surface is a contract decision, not an
adapter detail.

## References

- Memora: A Harmonic Memory Representation Balancing Abstraction and
  Specificity — Microsoft Research, ICML 2026.
  https://arxiv.org/abs/2602.03315
- ADR-0009: retrieval composition seam (establishes `RetrievalIndex` port
  that memory vector indexing will implement)
