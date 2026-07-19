# Roadmap

> Direction for the next 2-4 quarters. **Not** commitments. The whole point
> of writing this down is that it can change.

**Last updated:** 2026-07-01
**Reviewed:** quarterly. Next review: 2026-10-01.

If the current date is more than 90 days past "Last updated", treat this
file as stale and ask before relying on it.

## Now (current quarter)

What we're actively working on. Each item should reference its capability in
`docs/product/engram.md`.

- **Backend-agnostic retrieval composition.** RRF-fused graph + vector hybrid
  over the `RetrievalIndex` seam, durable sqlite-vec, configurable RRF — shipped
  SQLite-only. [spec: `backend-agnostic-retrieval`; RFC-0005; ADR-0009]
- **Demo polish.** Friendlier graph (meaningful labels, source files, neighbors),
  MCP server, benchmark harness. [spec: `benchmark-lazy-embeddings`]
- **Codegraph parity (on top of engram).** BM25 lexical retrieval, cross-encoder
  rerank, graph analytics (PageRank / betweenness / communities / reachability),
  and bi-temporal knowledge entities — building the codegraph layer on top of
  engram rather than into core. Adapters shipped behind ports; live-pipeline
  wiring in progress. [RFC-0012; `codegraph-parity-roadmap`; specs:
  `lexical-keyword-retrieval`, `cross-encoder-rerank`, `graph-analytics`]

## Next (following 1-2 quarters)

What we expect to pick up after Now. These are intentions, not promises.
Items here should have at least an RFC or a one-paragraph problem
statement somewhere — if there's nothing written down, it's not yet
ready to be on the roadmap.

- **Postgres + pgvector adapter.** First non-SQLite backend behind the existing
  `RetrievalIndex` port — the pgvector(graph+vector) deployment. [RFC-0005 §Target deployments; intent only]
- **Neo4j graph adapter.** The split deployment (pgvector(vector) + neo4j(graph))
  as a second graph backend. [RFC-0005; intent only]
- **`content_hash` upsert + vector GC.** Re-index dedup so durable embeddings are
  reused when chunk content is unchanged; reclaim dead vectors. [spec: `backend-agnostic-retrieval` O2]

## Later

Things we believe matter but aren't actively planning. Items here serve
two purposes: signal to contributors that we'd accept a PR, and let us
say "not now" without saying "never."

- Entity-embedding "semantic graph" (embed entities, retrieve via pgvector) —
  enabled by the mechanism-agnostic port, not yet built.
- Learned / cross-encoder reranker behind `RerankStrategy`.
- A Rust composition orchestrator in `core/orchestration` once a second backend
  makes TS-side orchestration insufficient.

## Not in scope

Things that have come up and that we've explicitly decided are *not*
in scope. This is the most valuable section for AI agents and new
contributors — it prevents wasted exploration of dead ends.

- **Being an LLM host.** Engram is model-agnostic; LLM calls stay in TypeScript
  behind the pi SDK. [Charter §Scope]
- **Eager index-time embeddings.** Indexing stays embedding-free; embeddings are
  lazy at query time, cached, and durable. [RFC-0005 D4; PERFORMANCE.md]
- **Distributed cross-store write consistency (sagas/outbox).** The retrieval
  seam is read-path only. [RFC-0005 §Non-goals]
- **A second TypeScript implementation of the Rust core.** The binding is a
  transport. [Charter §Principles; AGENTS.md]

## How this file is maintained

- **Owners:** the maintainers (or the steering committee, if one exists).
- **Updates:** roadmap items move between sections via small PRs. Substantive
  additions or deletions go through an RFC.
- **Review cadence:** quarterly. The review updates the "Last updated" date
  even if no items change — fresh eyes, fresh dates.
- **Drift signal:** if items in "Now" haven't moved in two consecutive
  reviews, either they're not actually being worked on (move them out)
  or the roadmap doesn't reflect what the team is doing (rewrite it to
  match).
