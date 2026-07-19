# Spec: associative-graph-wiring

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** RFC-0005, [`associative-graph-retrieval/spec.md`](../associative-graph-retrieval/spec.md) (this is its follow-up wiring slice), ADR-0022
- **Brief:** none
- **Contract:** none — adds additive N-API + TS transport surface; no v1 domain-contract change
- **Shape:** integration

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Associative graph retrieval is reachable from the TypeScript / N-API binding so
an agent (Claude Code, Cursor, or any N-API client) can invoke it: a caller asks
`associativeGraphCandidates(request)` and receives Personalized-PageRank-ranked
knowledge-graph entities, fused alongside the lexical graph / vector / lexical
candidates via the existing Rust `fuse_rrf_json` (exposed to TS as `fuseRrf`).
This wires the
`AssociativeGraphIndex` (shipped in `associative-graph-retrieval`) into the live
binding pipeline behind a `SqlKnowledgeStore`-backed edge source, and clears the
pre-existing `packages/node` typecheck failure that blocks the TS gate. Success:
a retrieval request through the binding returns associative candidates
deterministically, the TS workspace typechecks and builds, and no v1 public
contract changes.

## Boundaries

The three-tier guard that keeps an implementing agent inside the lines.
*Always do* applies without asking; *Ask first* requires human sign-off
before proceeding; *Never do* is a hard rule, even under time pressure.

### Always do

- Compose `AssociativeGraphIndex` exactly as the lexical `GraphRetrievalIndex` is
  composed in the binding: a `SqlKnowledgeStore`-backed source, a plain
  `associative_graph_candidates_json` fn, and a `#[napi]` method mirroring
  `graph_candidates_json`.
- Keep the edge source scope-safe: the `SqlKnowledgeStore`-backed source returns
  only `list_entities` / `list_relationships` rows that pass `scope_allows` for
  the request scope — inherited from the store; do not add a second scope gate.
- Stamp associative results with `FusionTrace.source = "associative_graph"` (the
  adapter already does) so the Rust `fuse_rrf_json` per-source `weights` map
  (`knowledge_fusion.rs:49-57`) keys them correctly (default 1.0 when unset);
  make no fusion change.
- Keep the binding additive: new `#[napi]` method + new TS interface/transport
  members; do not alter `graphCandidatesJson` or any existing binding.

### Ask first

- Wiring the Rust SDK facade (`core/integration` `EngramProvider` / sqlite
  bootstrap) — deferred to a follow-up; this slice wires the binding only (unlike
  the lexical `GraphRetrievalIndex`, which is already wired into the facade at
  `core/integration/src/sqlite/bootstrap.rs:266`).
- Moving `GraphRelationshipSource` into a core port (the option-(c) alternative)
  to eliminate the newtype wrapper — deferred; this slice keeps the trait where
  it ships.
- Anything beyond the touch points listed in the plan's Component decomposition.

### Never do

- Change any v1 domain contract (`contracts/v1` schema, `RetrievalMode`, domain
  types) — this slice adds transport surface only.
- Add SQL or an engine type to `core/` or to `engram-store-associative-graph`;
  the `SqlKnowledgeStore`-backed source lives in `bindings/node` (the composition
  layer), never in core or the engine-neutral adapter.
- Mutate the lexical `GraphRetrievalIndex`, `graph_candidates_json`, or the
  existing transport methods.
- Introduce a second scope-filtering pass inside the associative path — scope
  isolation is the store's responsibility at the read boundary.

## Testing Strategy

- **`associative_graph_candidates_json` plain-fn behavior: TDD** — a Rust test
  in `bindings/node` constructs a file-backed `SqlKnowledgeStore`, seeds entities
  + relationships, calls the plain fn, and asserts PPR-ranked `Entity` candidates
  with `source = "associative_graph"` and no cross-scope leakage. This exercises
  the wired path (store → source wrapper → `AssociativeGraphIndex` → results)
  end-to-end at the Rust binding layer.
- **`GraphRelationshipSource` wrapper delegation: TDD** — assert the
  `SqlKnowledgeStore`-backed newtype returns exactly the store's
  scope-filtered `list_entities` / `list_relationships` output.
- **TS surface compiles: goal-based check** — `pnpm --filter @engram/node
  typecheck` is green (the new interface/transport members typecheck AND the
  pre-existing mock gap is fixed); `pnpm run build` succeeds (the additive
  binding + TS surface compiles, no generated-contract drift).
- **End-to-end through the native addon: out of scope this slice** — the
  Rust plain-fn test is the behavioral evidence; a full native-addon TS E2E is a
  follow-up (heavy toolchain build, deferred).

## Acceptance Criteria

- [x]`bindings/node` exposes `associative_graph_candidates_json` as a plain fn
  that builds an `AssociativeGraphIndex` over a `SqlKnowledgeStore`-backed
  `GraphRelationshipSource` and returns its candidates, mirroring
  `graph_candidates_json`.
- [x]A `#[napi(js_name = "associativeGraphCandidatesJson")]` method on
  `NativeKnowledgeEngine` exposes it; `packages/node` adds the matching
  `NativeKnowledgeEngineBinding` member and `NativeKnowledgeTransport.`
  `associativeGraphCandidates`.
- [x]Given a `SqlKnowledgeStore` seeded with entities + relationships, the plain
  fn returns PPR-ranked `Entity` candidates for a query naming a seed entity,
  deterministically (a Rust test asserts ordering on a fixture).
- [x]Out-of-scope entities never appear: the source returns only `scope_allows`
  rows, so a relationship whose scope the request does not allow is not walked.
- [x]An associative candidate survives fusion: `fuse_rrf_json` with one
  associative and one lexical-graph candidate returns a fused list containing
  the associative candidate — the per-source `weights` map defaults unset sources
  to 1.0.
- [x]The pre-existing `packages/node` typecheck failure is fixed: the two
  `transport.test.ts` mock classes implement `listEntitiesBySourceJson` /
  `listRelationshipsBySourceJson`, and `pnpm --filter @engram/node typecheck` is
  green.
- [x]No v1 public contract changes: `contracts/v1` regenerates with zero drift
  and `pnpm run contracts:check-generated` is clean.
- [x]All repository gates are green: `cargo fmt --all`, `cargo check --workspace`,
  `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`,
  `pnpm run typecheck`, `pnpm run build`, `.codex/hooks/check-contracts.sh`,
  `.codex/hooks/check-docs.sh`, `.codex/hooks/check-engine-neutrality.sh`.

## Assumptions

- Technical: the `packages/node` typecheck failure is a 4-line test-only gap — two inline mock classes in `transport.test.ts:54,134` omit `listEntitiesBySourceJson`/`listRelationshipsBySourceJson`; the Rust napi fns, service, binding interface, and TS transport are already implemented (commit 482fd71, predates RFC-0013). (source: recon `packages/node/test/transport.test.ts`, `bindings/node/src/knowledge.rs:287-312`, git blame)
- Technical: the composition point is the N-API binding `graphCandidatesJson` → plain fn `graph_candidates_json` (`bindings/node/src/knowledge_fusion.rs:21-30`) → `GraphRetrievalIndex::new(store.clone())`; fusion runs in the Rust `fuse_rrf_json` (exposed to TS as `fuseRrf`, `transport.ts:254-262`); there is no `RetrievalMode` dispatch in the binding. (source: recon `bindings/node/src/{knowledge.rs,knowledge_fusion.rs}`, `packages/node/src/transport.ts`)
- Technical: `SqlKnowledgeStore::list_entities(scope)` (`service.rs:241`) and `list_relationships(scope)` (`service.rs:405`) exist and scope-filter via `scope_allows` (`scope.rs:9-16`). (source: recon `adapters/knowledge/sqlite/src/{service.rs,scope.rs}`)
- Technical: the orphan rule (E0117) forbids a bare `impl GraphRelationshipSource for SqlKnowledgeStore` in any third crate, so the binding uses a newtype wrapper (option a) rather than a bare impl or a core-port move (option c). (source: recon cross-crate analysis)
- Technical: fusion runs in the Rust `fuse_rrf_json` plain fn (`knowledge_fusion.rs:35-63`); the TS `fuseRrf` is its JSON-RPC wrapper. Associative candidates are `Vec<RetrievalResult>` with `FusionTrace.source = "associative_graph"`, and the Rust per-source `weights` BTreeMap (`knowledge_fusion.rs:49-57`) keys by source, defaulting unset sources to 1.0 — so associative candidates fuse unchanged. (source: recon `bindings/node/src/knowledge_fusion.rs`, `packages/node/src/transport.ts`)
- Process: this is the deferred `associative-graph-wiring` slice registered in `docs/backlog.md`; spec-only under RFC-0005; no v1 domain-contract change (additive transport surface). (source: recon + `docs/backlog.md`)
- Product: scope is the TS/N-API binding path only (the agent reachability path); the Rust SDK facade (`core/integration`) wiring is deferred to a follow-up, per the user's "agent-usable" framing and the binding being the primary agent path. (source: user direction 2026-07-15 + recon)
