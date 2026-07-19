# Plan: associative-graph-wiring

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog
> at the bottom.

## Approach

Compose the shipped `AssociativeGraphIndex` into the live TS/N-API binding,
mirroring the lexical `GraphRetrievalIndex` path exactly, and clear the
pre-existing `packages/node` typecheck failure. Three pieces, in order:

1. A 4-line test-only fix to `transport.test.ts` (two mock classes gain the
   `listEntitiesBySourceJson` / `listRelationshipsBySourceJson` stubs they already
   declare) — unblocks `pnpm typecheck`.
2. `bindings/node`: a `SqlKnowledgeStore`-backed newtype implementing
   `GraphRelationshipSource` (the orphan-rule-safe wrapper), a plain
   `associative_graph_candidates_json` fn mirroring `graph_candidates_json`, and a
   `#[napi(js_name = "associativeGraphCandidatesJson")]` method on
   `NativeKnowledgeEngine`.
3. `packages/node`: the matching `NativeKnowledgeEngineBinding` member and
   `NativeKnowledgeTransport.associativeGraphCandidates`, plus the associative
   stub on the two test mocks.

The riskiest part is the cross-crate composition: a bare
`impl GraphRelationshipSource for SqlKnowledgeStore` is forbidden by the orphan
rule (neither is local to `bindings/node`), so a newtype wrapper is required.
The wrapper is ~8 lines, introduced because the orphan rule (E0117) forbids a
bare `impl GraphRelationshipSource for SqlKnowledgeStore` in `bindings/node`
(`GraphRelationshipSource` is from `engram-store-associative-graph`,
`SqlKnowledgeStore` from `engram-store-knowledge-sqlite`). The lexical path needs
no wrapper only because `SqlKnowledgeStore` implements `GraphCandidateSource` in
its own crate (`adapters/knowledge/sqlite/src/retrieval.rs:227`). Fusion runs in
the Rust `fuse_rrf_json` and is untouched — associative candidates are just
another `Vec<RetrievalResult>` with `source = "associative_graph"`. The Rust SDK
facade (`core/integration`) is deliberately out of scope (deferred to a
follow-up); this slice wires the binding — the primary agent path.

## Constraints

- RFC-0005 — backend-agnostic retrieval composition; the binding composes a
  `RetrievalIndex`; fusion runs in the Rust `fuse_rrf_json` plain fn (exposed to
  TS as `fuseRrf`).
- `associative-graph-retrieval/spec.md` — this is its follow-up; the adapter unit
  (`engram-store-associative-graph`) is unchanged.
- ADR-0022 engine-neutrality — no SQL or engine type in `core/` or the
  engine-neutral adapter; the `SqlKnowledgeStore`-backed wrapper lives in
  `bindings/node` (the composition/transport layer), the documented place for
  engine-specific wiring.

## Construction tests

**Integration tests:** one Rust test of `associative_graph_candidates_json` over
a file-backed `SqlKnowledgeStore` seeded with entities + relationships (the
wired path end-to-end at the binding layer).
**Manual verification:** none — `pnpm typecheck` + `pnpm build` are the
goal-based checks for the TS surface.

## Design (LLD)

### Design decisions

- **Option (a) newtype wrapper, binding-only** (not option (c) core-port move).
  The orphan rule forbids a bare impl in `bindings/node`; a newtype
  `KnowledgeRelationshipSource(Arc<SqlKnowledgeStore>)` is the orphan-rule-idiomatic
  fix (the lexical path needs no wrapper only because `SqlKnowledgeStore`
  implements `GraphCandidateSource` in its own crate). Chosen over (c) because it touches no core
  crate, no v1 contract, and no just-shipped adapter — strictly lower-risk on the
  stability axes. *Rejected:* (c) moving `GraphRelationshipSource` to
  `core/retrieval` — grows a core port and refactors committed code to save one
  ~8-line wrapper.
- **Binding path only** (facade deferred). The agent reachability path is the
  TS/N-API binding; the Rust SDK facade (`core/integration`) wiring is a
  follow-up. Traces to: AC1–AC3.
- **No fusion change.** Associative candidates carry `source =
  "associative_graph"`; the Rust `fuse_rrf_json` per-source `weights` BTreeMap
  keys by source (default 1.0). Traces to: AC1, AC(fusion-survival).

### Interfaces & contracts

- New `#[napi]` method `associativeGraphCandidatesJson` + plain fn
  `associative_graph_candidates_json` — additive transport surface, no v1
  domain-contract change.
- New TS `NativeKnowledgeEngineBinding.associativeGraphCandidatesJson` +
  `NativeKnowledgeTransport.associativeGraphCandidates`.

### Component / module decomposition

- `bindings/node/src/knowledge_fusion.rs` — newtype
  `KnowledgeRelationshipSource(Arc<SqlKnowledgeStore>)` impl
  `GraphRelationshipSource` (delegates to `list_entities` / `list_relationships`);
  plain fn `associative_graph_candidates_json(store, request_json)`.
- `bindings/node/src/knowledge.rs` — `#[napi(js_name =
  "associativeGraphCandidatesJson")]` method on `NativeKnowledgeEngine`.
- `bindings/node/Cargo.toml` — add `engram-store-associative-graph` dep.
- `packages/node/src/binding.ts` — interface member.
- `packages/node/src/transport.ts` — transport method.
- `packages/node/test/transport.test.ts` — fix the two mock classes (precondition)
  + add the associative stub.

### State & control flow

Stateless per request. `associativeGraphCandidatesJson(requestJson)` → decode
`RetrievalRequest` → wrap the engine's `store` in `KnowledgeRelationshipSource`
→ `AssociativeGraphIndex::new(source).retrieve_candidates(&request)` (block_on)
→ encode results. The store is shared (`NativeKnowledgeEngine.store`).

### Failure, edge cases & resilience

- No seed resolves → empty candidates (the adapter returns `[]`); the binding
  returns `"[]"`.
- Store read error → propagated as a napi error (mirror `graph_candidates_json`).
- Out-of-scope edges → never walked (the store scope-filters at the read
  boundary).

### Quality attributes (NFRs)

- **Scope safety:** inherited from `SqlKnowledgeStore::list_*` `scope_allows`
  filtering; no second gate.
- **Engine-neutrality:** no SQL/engine type enters `core/` or the adapter; the
  wrapper is in `bindings/node`.
- **Contract stability:** zero v1 drift.

## Tasks

### T1: Clear the `packages/node` typecheck failure (test-only)

**Depends on:** none

**Tests:**
- Goal-based: `pnpm --filter @engram/node typecheck` is green (the two
  `*BySourceJson` errors gone; nothing new introduced).

**Approach:**
- In `packages/node/test/transport.test.ts`, add to each of the two inline mock
  classes (≈ lines 54 and 134), next to the existing `listEntitiesJson` /
  `listRelationshipsJson` stubs: `listEntitiesBySourceJson(): string { return
  "[]"; }` and `listRelationshipsBySourceJson(): string { return "[]"; }`.
- Note: this is a blocking precondition for the slice's TS gate (T4) and a
  same-mock-class ride-along with T3 (which also edits these two mock classes) —
  in scope under the bundled-fixes carve-out, not a separate PR.

### T2: `bindings/node` — wrapper, plain fn, `#[napi]` method (TDD)

**Depends on:** none

**Tests:**
- The `KnowledgeRelationshipSource` newtype's `entities` / `relationships` return
  exactly the store's scope-filtered `list_entities` / `list_relationships`
  output (construct a file-backed `SqlKnowledgeStore`, seed it, assert).
- `associative_graph_candidates_json` over a seeded store returns PPR-ranked
  `Entity` candidates with `FusionTrace.source == "associative_graph"` for a
  query naming a seed entity (assert ordering on a fixture), and omits
  out-of-scope entities.

**Approach:**
- `bindings/node/Cargo.toml`: add
  `engram-store-associative-graph = { path = "../../adapters/retrieval/associative-graph" }`.
- `knowledge_fusion.rs`: `pub(crate) struct KnowledgeRelationshipSource(pub(crate)
  Arc<SqlKnowledgeStore>);` `#[async_trait] impl GraphRelationshipSource` delegating
  to `self.0.list_entities(scope)` / `list_relationships(scope)`; plain fn
  `associative_graph_candidates_json(store: &Arc<SqlKnowledgeStore>,
  request_json: String) -> Result<String>` mirroring `graph_candidates_json`
  (`decode` → `AssociativeGraphIndex::new(Arc::new(KnowledgeRelationshipSource(
  store.clone())))` → `block_on(retrieve_candidates)` → `encode`).
- `knowledge.rs`: `#[napi(js_name = "associativeGraphCandidatesJson")] pub fn
  associative_graph_candidates_json(&self, request_json: String) -> Result<String>`
  delegating to the plain fn.
- The crate is `crate-type = ["cdylib", "rlib"]`, so in-crate `#[cfg(test)]` tests
  run normally under `cargo test -p engram-node`; construct the store with
  `SqlKnowledgeStore::open_in_memory()` (`service.rs:36`) and seed entities +
  relationships directly. **No fallback** — the plain-fn E2E over the real store
  is the test, and it is what verifies AC3/AC4 (including `scope_allows` through
  the wired store, which the prior slice deferred to here).

**Done when:** the new Rust tests pass and `cargo test -p engram-node` is green.

### T3: `packages/node` — binding interface + transport method

**Depends on:** T2 (the `js_name`)

**Tests:**
- Goal-based: `pnpm --filter @engram/node typecheck` stays green with the new
  members; the transport method round-trips through `graphCandidates`'s
  decode/encode shape.

**Approach:**
- `binding.ts`: add `associativeGraphCandidatesJson(requestJson: string): string;`
  to `NativeKnowledgeEngineBinding`.
- `transport.ts`: add `associativeGraphCandidates(request: RetrievalRequest)` to
  `NativeKnowledgeTransport`, implemented like `graphCandidates` (`decode(
  this.engine.associativeGraphCandidatesJson(encode(request)))`).
- `transport.test.ts`: add an `associativeGraphCandidatesJson` stub to the two
  mock classes (so typecheck stays green).

**Done when:** `pnpm --filter @engram/node typecheck` is green and the transport
method is wired.

### T4: Full gates + no-drift verification

**Depends on:** T1, T2, T3

**Tests:**
- Goal-based: `cargo fmt --all --check`; `cargo check --workspace`;
  `cargo clippy --workspace --all-targets -- -D warnings`;
  `cargo test --workspace`; `pnpm run typecheck`; `pnpm run build`;
  `pnpm run contracts:check-generated` (zero drift);
  `.codex/hooks/check-contracts.sh`; `.codex/hooks/check-docs.sh`;
  `.codex/hooks/check-engine-neutrality.sh`.

**Approach:**
- Run the full sweep; fix lint/clippy nits in the new code only.
- Confirm `contracts/v1` regenerates with no diff (transport surface is not a v1
  domain contract).
- Confirm engine-neutrality is clean (no SQL/engine type in `core/` or the
  adapter).

**Done when:** every gate is green and the spec's Acceptance Criteria are all
checked.

## Rollout

- **Delivery:** additive — a new binding method + TS members + a test fix. No
  existing path changes; reversible by removing the additions. Ships
  unconditionally (opt-in: callers must call `associativeGraphCandidates` to use
  it).
- **Infrastructure:** none.
- **External-system integration:** none.
- **Deployment sequencing:** none — the Rust SDK facade wiring is a separate
  follow-up.

## Risks

- **`SqlKnowledgeStore` construction in a `bindings/node` test** may be awkward
  (cdylib). Mitigated by the fallback (wrapper-delegation test + adapter unit
  tests) and confirming the construction pattern during T2.
- **N-API build flakiness** under `pnpm build`. Mitigated by mirroring
  `graph_candidates_json` exactly and running `pnpm build` in T4.
- **Accidental v1 contract touch.** Mitigated by `contracts:check-generated`
  (zero drift is an AC).

## Changelog

- 2026-07-15: initial plan — option (a) newtype wrapper, binding path only;
  facade wiring deferred; typecheck fix included as T1.
- 2026-07-15: SHIPPED — adversarial (scope-leak test made meaningful via an
  in-scope `w` reachable only through a cross-scope relationship; delegation
  test added) + quality-engineer passes clean. Orphan-rule newtype + plain fn +
  `#[napi]` method + TS interface/transport + the pre-existing `*BySource`
  typecheck-debt fix across two fakes. 3 `engram-node` tests; workspace + TS
  gates green. Rust SDK facade wiring deferred.
