# Spec: associative-graph-retrieval

- **Status:** Shipped <!-- Draft | Approved | Implementing | Shipped | Archived -->
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** RFC-0005, `docs/domain-data-model.md` (contract-freeze policy), ADR-0022 (engine-neutrality)
- **Brief:** none
- **Contract:** none — implements the already-accepted `RetrievalMode::Graph`; no public contract change
- **Shape:** service

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Associative graph retrieval returns knowledge-graph entities that are
graph-proximate to the entities named in a retrieval query, ranked by
Personalized PageRank seeded from those query entities and confined to the
requester's scope. It gives an agent associative / multi-hop recall — "what
else is connected to what I asked about" — that lexical and vector retrieval
cannot produce, supplied as a candidate producer behind the existing
`RetrievalIndex` port for the already-accepted `RetrievalMode::Graph`. Success
looks like: a retrieval request whose query names an in-scope entity returns
that entity and its graph-proximate neighbors ranked by graph proximity,
deterministically, with provenance and a fusion trace, and with no change to any
v1 public contract and no cross-scope leakage.

## Boundaries

The three-tier guard that keeps an implementing agent inside the lines.
*Always do* applies without asking; *Ask first* requires human sign-off
before proceeding; *Never do* is a hard rule, even under time pressure.

### Always do

- Confine the Personalized PageRank walk to the in-scope subgraph by reading
  edges only through a `scope`-filtered `GraphRelationshipSource`; never admit
  an edge whose own scope the request scope does not allow.
- Keep the new crate engine-neutral: no SQL, no engine type (`Sql*`,
  `pgvector`, …), no storage internals, no vector or embedding dependency in
  `adapters/retrieval/associative-graph/`; all graph data enters through the
  injected `GraphRelationshipSource` trait.
- Keep retrieval deterministic and model-free: seed resolution is lexical
  (query-token match against in-scope entity names), scoring is pure
  Personalized PageRank; no LLM, no embedding call, no clock, no randomness on
  the path.
- Emit a `FusionTrace` / explanation carrying `source = "associative_graph"`,
  the resolved seed entities, and the ranking basis; reuse the existing
  `RetrievalResult`, `RetrievalScore`, and `Provenance` shapes unchanged.
- Return an empty candidate set when no seed entities resolve — no graph
  traversal is started without an anchor.

### Ask first

- Wiring the index into the live N-API bindings / provider pipeline or
  registering it in `RetrievalRouter` (deferred to a separate `associative-graph-
  wiring` spec; explicitly out of scope for this slice).
- Emitting `Relationship` or `Memory` result targets (this slice emits `Entity`
  targets only).
- Making the set of relationship predicates that become walk edges configurable
  (this slice walks every predicate).
- Adding a `GraphRelationshipSource` implementation backed by `SqlKnowledgeStore`
  (that impl belongs with the wiring slice, not this adapter-unit slice).

### Never do

- Change any v1 public contract: no new `RetrievalMode` variant, no
  schema/enum/field rename or semantic change (`Graph` is already frozen v1).
- Add SQL, a storage-engine type, or a vector/embedding/model dependency to
  `core/` or to the new crate.
- Cache edges across requests or share edge state between scopes — the edge set
  is read fresh per request through the scope-filtered source, or cross-scope
  leakage becomes possible.
- Add a new top-level directory or a new core port method to ship this slice;
  the edge source is an adapter-local trait, mirroring `GraphCandidateSource`.
- Mutate the existing lexical `GraphRetrievalIndex`; this index coexists with it
  as a separate `RetrievalIndex` implementation.

## Testing Strategy

- **Personalized PageRank algorithm: TDD.** Pure function with a compressible
  invariant (convergence, mass conservation, seed bias, determinism). Mirrors
  the inline `#[cfg(test)] mod tests` style of `core/graph-analytics/src/pagerank.rs`.
- **Associative retrieval ranking, seed resolution, and edge-boundedness: TDD**
  with a stub `GraphRelationshipSource`, SQL-free. The pure ranking helper
  (`rank_associative`) and seed resolution are asserted on directly — the same
  pure-helper-test pattern `adapters/knowledge/sqlite/src/retrieval.rs` uses for
  `rank_graph_candidates` (the precedent does not test through a stub source; this
  slice additionally adds one async stub-source test through the `RetrievalIndex`
  port).
- **No-contract-change, engine-neutrality, and workspace build: goal-based
  check.** `pnpm run contracts:check-generated` shows zero drift;
  `.codex/hooks/check-engine-neutrality.sh` is clean; `grep` confirms no
  SQL/engine type in the new crate; `cargo check --workspace` and the full gate
  suite pass.
- **End-to-end through the live provider: out of scope this slice** (deferred to
  the wiring follow-up spec), so no manual-QA criterion here.

## Acceptance Criteria

- [x] `engram_graph_analytics::personalized_pagerank` exists, is pure (no
  dependencies beyond `std`), is deterministic across runs, and its tests
  assert: convergence to a known fixture, seed-biased ordering (seeds and their
  direct neighbors outrank distant nodes at equal structure), scores sum to
  approximately one, empty seeds yields an empty map, and a seed
  absent from every edge still appears in the output (seeds are added to the node
  set so their teleport mass counts).
- [x] A new crate `engram-store-associative-graph` at
  `adapters/retrieval/associative-graph/` implements
  `engram_retrieval::RetrievalIndex` and produces `RetrievalResult` candidates
  with `RetrievalTargetType::Entity`, ranked by Personalized PageRank, for
  `RetrievalMode::Graph`.
- [x] Given a query whose tokens match in-scope entity names, retrieval returns
  the seed entities and their graph-proximate entities ranked by PPR score,
  deterministically — a fixture graph asserts the exact returned ordering.
- [x] Given a query that resolves no seed entities, retrieval returns an empty
  candidate set (no traversal without an anchor).
- [x] The ranking helper adds no edges beyond those the `GraphRelationshipSource`
  returns — entities reachable only through edges the source did not supply never
  appear in results. (The source's own `scope_allows` filtering is exercised in
  the deferred wiring slice; this slice's stub source is test-controlled, so this
  AC asserts the walk invents no edges.)
- [x] No v1 public contract changes: the `RetrievalMode` enum is unchanged,
  `contracts/v1` regenerates with zero drift, and
  `pnpm run contracts:check-generated` is clean.
- [x] Engine-neutrality holds: `.codex/hooks/check-engine-neutrality.sh` is clean
  (it scans `core/`), AND a recursive grep over
  `adapters/retrieval/associative-graph/` finds no `Sql*` / `Pg*` / `Tantivy*` /
  SQL keyword (the hook does not scan adapter crates, so the grep is the gate for
  the new crate).
- [x] All repository gates are green: `cargo fmt --all`,
  `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D
  warnings`, `cargo test --workspace`, `pnpm run typecheck`, `pnpm run test`,
  `.codex/hooks/check-contracts.sh`, `.codex/hooks/check-docs.sh`.
- [x] The new crate is listed in the root `Cargo.toml` workspace members and in
  the AGENTS.md `adapters/` target-shape entry.

## Assumptions

- Technical: `RetrievalMode::Graph` is a frozen v1 value (`core/domain/src/retrieval.rs:16-23`; `contracts/v1` enum), so associative retrieval needs no contract change. (source: recon `core/domain/src/retrieval.rs`, `contracts/v1/schemas/engram-v1.schema.json:374`)
- Technical: `RetrievalIndex` (`core/retrieval/src/ports.rs:22`) is the port to implement; new `RetrievalIndex` implementations are contract-additive per the domain freeze policy. (source: recon `core/retrieval/src/ports.rs`, `docs/domain-data-model.md:16-45`)
- Technical: `personalized_pagerank` does not exist; only global `pagerank` (`core/graph-analytics/src/pagerank.rs`). It is a ~50-70 line pure-Rust addition to a zero-dependency crate. (source: recon `core/graph-analytics/src/{pagerank.rs,lib.rs,Cargo.toml}`)
- Technical: no in-memory `KnowledgeGraphRepository` exists (retired — `docs/specs/retire-knowledge-inmem`, Shipped); only `SqlKnowledgeStore`. The adapter is therefore unit-tested with a stub edge source, mirroring `GraphRetrievalIndex`'s stub `GraphCandidateSource` tests. (source: recon `adapters/knowledge/sqlite/src/retrieval.rs:28-32,236-322`)
- Technical: no port exposes all in-scope relationships (`neighbors` is per-node and outgoing-only; `GraphCandidateSource` is entities+chunks), so the feature defines an adapter-local `GraphRelationshipSource` trait exposing BOTH in-scope `entities` and `relationships` — entities are needed for lexical seed resolution (`KnowledgeEntity` names) and to clone `RetrievalResult` content/provenance, faithfully mirroring `GraphCandidateSource`'s two-method shape. (source: recon `core/knowledge/src/graph.rs:14-32`, `adapters/knowledge/sqlite/src/retrieval.rs:28-32,74-81`, `core/domain/src/types.rs:108-114`)
- Technical: scope isolation is enforced at the adapter read boundary by `scope_allows` (`adapters/knowledge/sqlite/src/scope.rs:8-20`); a PPR walk over scope-filtered edges cannot cross scopes by construction. (source: recon `adapters/knowledge/sqlite/src/scope.rs`, `core/retrieval/src/composer.rs:17-18`)
- Process: retrieval adapters are spec-only under RFC-0012 + RFC-0005 (umbrella composition RFC) + `docs/codegraph-parity-roadmap.md`; no per-adapter RFC. House precedent separates adapter-unit (`lexical-keyword-retrieval`, Shipped) from wiring (`lexical-wiring`, Draft). (source: recon `docs/specs/`, `docs/rfcs/0005-*.md`, `bindings/node/src/knowledge_fusion.rs`)
- Product: the first slice is the adapter unit only (algorithm + `AssociativeGraphIndex` + trait + unit tests); SQL/provider wiring is deferred to a follow-up spec, per the "lowest risk" directive and the adapter-unit/wiring precedent. (source: user instruction 2026-07-14 + precedent)
