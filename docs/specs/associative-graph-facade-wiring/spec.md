# Spec: associative-graph-facade-wiring

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** AGENTS.md surface-parity rule, RFC-0005, [`associative-graph-retrieval/spec.md`](../associative-graph-retrieval/spec.md), ADR-0022
- **Brief:** none
- **Contract:** none — additive unified-recall lane; `RetrievalMode::Graph` is already frozen, no v1 change
- **Shape:** integration

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it.

## Objective

Associative graph retrieval is reachable through the Rust SDK facade: a Rust
embedder calling `provider.recall(request)` receives Personalized-PageRank-ranked
knowledge-graph entities fused alongside the lexical / graph / vector / belief
candidates via the existing `UnifiedRecall` + RRF. This closes the surface-parity
gap named by the new AGENTS.md rule — associative retrieval becomes available via
`engram-integration` (the `EngramProvider` facade), not only the N-API binding.
Success: a `recall` over a seeded knowledge graph returns `associative_graph`-
tagged `Entity` candidates, deterministically, with no v1 contract change and no
cross-scope leakage.

## Boundaries

### Always do

- Add associative retrieval as a **unified-recall lane** — push
  `AssociativeGraphIndex` into the existing `retrieval_lanes` Vec in
  `bootstrap.rs`, mirroring the lexical `GraphRetrievalIndex` lane exactly.
- Keep the `KnowledgeRelationshipSource` newtype and the `AssociativeGraphIndex`
  construction in `core/integration/src/sqlite/` (`recall_lanes.rs` +
  `bootstrap.rs`) — the ADR-0022 engine-neutrality exempt zone.
- Reuse the shipped `GraphRelationshipSource` trait as-is; the newtype delegates
  to `SqlKnowledgeStore::list_entities` / `list_relationships` (scope-filtered).

### Ask first

- A standalone `EngramProvider` associative op (heavier — needs a new
  engine-neutral port trait; the facade's retrieval surface is `UnifiedRecall`,
  so the lane is the parity-closing move).
- A new `CapabilityReport` key (associative strengthens the existing
  `unified_recall` capability; no new key needed).

### Never do

- Change any v1 contract (`RetrievalMode`, `UnifiedRecall`, `ContextPayload`,
  schema) — associative is an additive lane behind an already-frozen mode.
- Put the `SqlKnowledgeStore`-naming newtype or `engram-store-*` imports in any
  file gated by `.codex/hooks/check-engine-neutrality.sh` (`provider.rs`,
  `capability.rs`, `recall.rs`, `provenance.rs`, `batch.rs`, `export_import.rs`,
  `observability.rs`) — the lint would fire; they stay in `src/sqlite/`.
- A bare `impl GraphRelationshipSource for SqlKnowledgeStore` — orphan-forbidden
  in `core/integration` (neither is local); use the newtype.

## Testing Strategy

- **Facade recall E2E: TDD** — construct a sqlite-backed `EngramProvider`, seed
  entities + relationships, call `recall`, and assert `associative_graph`-tagged
  `Entity` candidates appear in the `ContextPayload`, ranked by PPR, with an
  out-of-scope entity absent (scope isolation through the lane's scope-filtered
  source). This exercises the wired facade path end-to-end.
- **Engine-neutrality: goal-based** — `check-engine-neutrality.sh` stays clean
  (newtype + construction in the exempt `src/sqlite/` zone, no engine type/SQL in
  gated files).

## Acceptance Criteria

- [x] `core/integration` exposes associative retrieval via `UnifiedRecall`: the
  facade's `SqlUnifiedRecall` (built by the sqlite `bootstrap` with the
  associative lane) returns `associative_graph`-tagged `Entity` candidates on
  `recall(request)` when the query names a seed entity. Tested at the
  `SqlUnifiedRecall` level in `engram-conformance` (`adapters/integration/tests/`),
  which owns the sqlite test harness — `core/integration` has no sqlite test
  infra by design (ADR-0022 stub proof).
- [x] A `KnowledgeRelationshipSource` newtype lives in
  `core/integration/src/sqlite/recall_lanes.rs` and an `AssociativeGraphIndex`
  is constructed + pushed into `retrieval_lanes` in
  `core/integration/src/sqlite/bootstrap.rs`, mirroring the lexical lane.
- [x] Out-of-scope entities never appear in `recall` results (the lane's source
  is scope-filtered via `scope_allows`).
- [x] An entity returned by BOTH the lexical and associative graph lanes appears
  at most once in the `ContextPayload` (the fusion merges same-
  `(target_type, target_id)` candidates); a test pins the observed dedup
  outcome.
- [x] No v1 contract change: `contracts/v1` regenerates with zero drift and
  `pnpm run contracts:check-generated` is clean.
- [x] Engine-neutrality holds: `.codex/hooks/check-engine-neutrality.sh` is clean
  and no `Sql*` / engine type / SQL appears in the files it gates under
  `core/integration/src/`.
- [x] All repository gates are green: `cargo fmt --all`, `cargo check --workspace`,
  `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`,
  `pnpm run typecheck`, `pnpm run build`, `.codex/hooks/check-contracts.sh`,
  `.codex/hooks/check-docs.sh`, `.codex/hooks/check-engine-neutrality.sh`.

## Assumptions

- Technical: the facade exposes retrieval only via `recall()` → `UnifiedRecall` (`core/integration/src/provider.rs:267`); there is no standalone per-mode retrieve op, and `retrieval()` is dead (None at bootstrap). (source: recon `core/integration/src/{provider.rs,sqlite/bootstrap.rs}`)
- Technical: unified-recall lanes are composed in code (`retrieval_lanes: Vec<Arc<dyn RetrievalIndex>>`, `sqlite/recall.rs:62`), iterated with synthetic tags — no lane enum in the domain contract. (source: recon `core/integration/src/sqlite/recall.rs`)
- Technical: adding an associative lane is NOT a v1 contract change — `RetrievalMode::Graph` is already frozen; associative is another `RetrievalIndex` pushed into the existing Vec. (source: recon `core/domain/src/retrieval.rs:21`, `associative-graph/src/index.rs:79-80`)
- Technical: the lexical `GraphRetrievalIndex` lane is constructed at `bootstrap.rs:263-267`; associative mirrors it with an orphan-rule newtype (the binding's `KnowledgeRelationshipSource` at `bindings/node/src/knowledge_fusion.rs:43-53` is the verbatim precedent). (source: recon)
- Technical: the newtype + construction belong in `core/integration/src/sqlite/` (the ADR-0022 exempt zone); gated files are `provider.rs`/`capability.rs`/`recall.rs`. (source: recon `.codex/hooks/check-engine-neutrality.sh`)
- Process: this is the deferred facade-wiring item in `docs/backlog.md`, required to satisfy the new AGENTS.md surface-parity rule. (source: user direction 2026-07-15)
