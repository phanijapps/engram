# Spec: benchmark-lazy-embeddings (embed at runtime, not index time)

- **Status:** Draft
- **Shape:** mixed (eval + service)
- **Constrained by:** `engram-eval` (deterministic eval harness); IT-org ontology as baseline
- **Contract:** none

## Objective

Prove (or disprove) the hypothesis that **lazy embeddings** — embedding text at
Q&A/retrieval time rather than during indexing — produce comparable retrieval
quality to eager embeddings (embed at index time) while keeping indexing fast +
dependency-free. The benchmark indexes the Microsoft Terminal repo (500K+ lines),
runs recall + Q&A evals with and without embeddings, times the indexing, and
documents results in `PERFORMANCE.md` with charts stored in `docs/perf/images`.

## Decision

Use the research-pack to first draft the hypothesis, then the eval harness to
test it. The baseline is the deterministic knowledge graph (entities +
relationships + chunk text + agentic Q&A) WITHOUT any vector embeddings. The
comparison adds FastEmbed BGE-small at Q&A time (embed the query + chunks
on-the-fly, cosine-rank). The benchmark measures: (1) indexing speed, (2) Q&A
answer quality (eval rubric), (3) recall@5 for entity search.

## Acceptance Criteria

- [ ] Microsoft Terminal (500K+ lines) indexed with timing measurements.
- [ ] Eval suite: 10+ Q&A questions scored by rubric (with + without embeddings).
- [ ] `docs/perf/PERFORMANCE.md` with: machine specs, indexing time, entity/relationship counts, eval results table, charts (bar/line) stored in `docs/perf/images/`.
- [ ] Honest conclusion: does lazy embedding work or not?
