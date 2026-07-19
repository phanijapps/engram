# Research — synthesized index

> This folder is engram's research base: academic notes, prior-art surveys, and
> applied studies that ground the architecture. This README is the **map** — a
> one-line "what each note informs" per file, plus where engram sits relative to
> prior art. The deep synthesis is [`synthesis.md`](./synthesis.md); start there
> for the working architecture direction, come here to find a source.

## Where engram sits relative to prior art

Engram's thesis — **decouple what is stored (rich memory + a source-grounded
knowledge graph) from how it is retrieved (a multi-mode, fused, policy-filtered
pipeline)** — is the same separation Microsoft Research's
[Memora](https://www.microsoft.com/en-us/research/blog/memora-a-harmonic-memory-representation-balancing-abstraction-and-specificity/)
argues resolves the abstraction-vs-specificity tension in agent memory. The
prior-art surveys below map the field:

- **Content-fragmentation systems** (RAG, [Mem0](https://github.com/mem0ai/mem0))
  embed facts/text fragments — preserves detail, loses narrative coherence.
- **Coarse-abstraction systems** compress into summaries — efficient, loses
  fine-grained constraints and numbers.
- **Graph-based systems** ([Zep](https://github.com/getzep/zep),
  GraphRAG, [Letta](https://github.com/letta-ai/letta)) add structure on top of
  content but typically rely on the content itself for retrieval and often need
  rigid ontologies.

Engram's answer: a **contract-first** core that keeps memory, knowledge graph,
bi-temporal belief, and hierarchy as **distinct first-class subsystems**, linked
through retrieval, provenance, and an explicit consolidation pipeline — with
policy/provenance/scope governance baked into every path. See
[`graphmind-prior-art-survey.md`](./graphmind-prior-art-survey.md) and
[`memtrace-survey.md`](./memtrace-survey.md) for the full comparison, and
[`engram-framing-synthesis.md`](./engram-framing-synthesis.md) for the framing.

## Concept → research map

| Research note | Informs | Status |
| --- | --- | --- |
| [`synthesis.md`](./synthesis.md) | the working architecture direction (canonical deep synthesis) | living doc |
| [`academic-research-findings.md`](./academic-research-findings.md) | memory vs knowledge distinction; CoALA functional taxonomy (working/episodic/semantic/procedural) | implemented |
| [`philosophical-research-findings.md`](./philosophical-research-findings.md) | the memory/knowledge domain boundary (what each subsystem *is*) | implemented |
| [`memory-knowledge-architecture.md`](./memory-knowledge-architecture.md) | the five-layer architecture (orchestration / retrieval / memory / knowledge / storage) | implemented |
| [`architecture-design-v2.md`](./architecture-design-v2.md) | v2 modular layer responsibilities + hierarchical episodic memory | implemented |
| [`architecture-diagrams-v2.html`](./architecture-diagrams-v2.html) | visual companion to the v2 architecture | reference |
| [`hierarchical-taxonomy-research.md`](./hierarchical-taxonomy-research.md) | hierarchy for context compression; SKOS taxonomy; cue-based retrieval | implemented |
| [`zbot-engram-belief-bitemporal-cutover.md`](./zbot-engram-belief-bitemporal-cutover.md) | bi-temporal belief synthesis (valid-time vs record-time) | implemented |
| [`cross-repo-linkage.md`](./cross-repo-linkage.md) | source-grounded knowledge graph; stable source keys; re-ingest convergence | implemented |
| [`codegraph-parity-audit.md`](./codegraph-parity-audit.md) | the on-top codegraph layer ([RFC-0012](../rfcs/)) capability parity | implemented |
| [`graphmind-prior-art-survey.md`](./graphmind-prior-art-survey.md) | competitive positioning vs Mem0/Zep/Letta/GraphRAG | reference |
| [`memtrace-survey.md`](./memtrace-survey.md) | prior-art survey (memory trace + visualization) | reference |
| [`engram-framing-synthesis.md`](./engram-framing-synthesis.md) | how engram is framed for techno-functional + functional audiences | reference |
| [`agentzero-engram-memory-integration-comparison-matrix.md`](./agentzero-engram-memory-integration-comparison-matrix.md) | the AgentZero adapter integration contract | applied study |


## How to use this folder

- **Designing a contract or ADR?** Read [`synthesis.md`](./synthesis.md) first,
  then the specific note that grounds the subsystem you are changing.
- **Adding a note?** One markdown file per source or theme — citation, claims,
  implementation ideas, risks, and relevance to engram. Add a row to the table
  above so it is discoverable.
- **Promote, don't accumulate.** When a note's decisions are settled, promote
  them into an ADR (`docs/adr/`) or RFC (`docs/rfcs/`); the research note stays
  as the trace, not the source of truth.

## See also

- [`synthesis.md`](./synthesis.md) — the deep synthesis.
- [Architecture overview](../architecture/overview.md) — the implemented pipeline.
- [`docs/adr/`](../adr/) — frozen architecture decisions.
- [`docs/rfcs/`](../rfcs/) — design proposals.
- [`README`](../../README.md) — project overview, use cases, and the doc map.
