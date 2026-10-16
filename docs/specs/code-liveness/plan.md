# Plan: code-liveness

- **Spec:** [`spec.md`](spec.md)
- **Status:** Executing

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not a re-ordering), note why in the changelog at the
> bottom.

## Approach

Fix the three liveness blind spots in dependency order — resolution first
(cheapest, unblocks two known false positives), then registration edges (the
structural fix the agentzero class needs), then dispatch liveness — and fold
all three into a classifying `dead_code` v2 that surfaces through MCP + N-API
together. Every phase is guarded by fixtures derived from the four verified
real-world cases before the fix lands, so a regression is a red test, not a
rediscovery. The riskiest part is the qualified-suffix ladder: it must not
degrade the existing bare-name/ambiguity behavior, so the full existing
resolution test suite must stay green unchanged.

## Constraints

- ADR-0028: `engram-code` stays pure (no I/O, storage, engines, async).
- ADR-0022: engine neutrality (no engine names outside adapters) and surface
  parity (Rust facade ↔ N-API ↔ MCP for anything new).
- RFC-0020: ambiguity is recorded in the ledger, never silently dropped or
  arbitrarily picked.
- Domain contract changes are additive (`CodeEdgeKind::Registers`) with
  regenerated contracts + TS types in the same change.

## Construction tests

**Integration tests:** dead-list diff before/after re-scan of the
mem-alpha-self store (false positives out: `FastEmbedEmbeddingProvider`,
`MemoryLifecycleExecutor::execute`, `CompositeConsolidationExecutor`,
`KnowledgeIngestor::ingest`); agentzero store re-scan flips
`DelegateToZbotTool` to live.

**Manual verification:** `mcp_driver.py` drives the rebuilt server against
`~/.engram/mem-alpha-self` (and a throwaway agentzero-scope store if the
shared store is busy); grep the `code op=health` output for the six buckets.

## Design (LLD)

### Design decisions

- ~~Suffix ladder~~ → **declaration-side qualified relaxation** (review pass
  1 correction): the receiver-hint rung already tries `Type::name`; the
  failing seam was `extractor.rs` dropping qualified constructor DECLARATIONS
  (bare tail `new` = bare-generic noise) so resolution had no target. The
  corpus pinned it: entity absent, ledger row pending forever. Fix: a
  qualified name (`Foo::new`) is never bare-generic noise.
  Traces to: AC1, AC2 · contracts/v1 regenerated outputs.
- `registers` as a new `CodeEdgeKind` (not a `calls` overload): registration
  is a distinct relation — blast radius and health treat it as liveness for
  the target type, but it is not a call. Traces to: AC3 · contracts/v1.
- Constructed-impl ⇒ live (extends contains-liveness): a type with an
  incoming `registers` edge marks its `impl Trait` methods live without
  inventing dispatch edges. Traces to: AC4.
- Classification lives in a focused module
  (`codegraph/queries/src/liveness.rs`, re-exported from the crate facade —
  review pass 1 finding 8), not in the MCP text formatter — so N-API and MCP
  share one classifier. Buckets are the evidence-backed four;
  `generated`/`inconclusive` deferred (backlog `code-liveness-followups`).
  N-API mechanism: SIBLING binding `deadCodeClassifiedJson`; the default
  `deadCodeJson` array stays byte-identical (review finding 3).
  Traces to: AC5, AC6.

### Data & schema

`CodeEdgeKind` gains `Registers` (additive; serde round-trips; regenerated
contracts + TS). Extraction emits `registers` relationships with the
registration-site symbol as subject and the constructed type as object.

### Interfaces & contracts

- MCP `code op=health`: additive `classification` in the output (six buckets);
  raw dead list remains the default view.
- N-API `dead_code_json`: same classifier, same bucket names.

### Component / module decomposition

- `core/code/src/resolution.rs` — suffix-ladder rung (+ tests).
- `core/code/src/edges.rs` + parser — `registers` extraction for
  `register*(Arc::new(T::new()))`, `builder.with_*(T…)` patterns.
- `core/domain/src/knowledge.rs` — `CodeEdgeKind::Registers`.
- `codegraph/queries/src/queries.rs` — liveness inputs (registers,
  constructed-impl) + `classify_dead` buckets.
- `bindings/node/src/codegraph.rs` + `mcp/engram-mcp/src/codegraph.rs` —
  surface parity.

## Tasks

### T1: Evidence fixtures pin today's wrong verdicts

**Depends on:** none

**Tests:**
- Fixture snippets for the four verified cases (qualified-path constructor
  call; `register(Arc::new(T::new()))`; `Arc<dyn Trait>` construction +
  method dispatch; test-double symbol) extract + resolve today with the
  WRONG verdict — pinned as `#[ignore]`-free expected-wrong tests that T2-T5
  flip one by one.

**Approach:**
- Add `core/code/tests/liveness_fixtures/` snippets + a
  `core/code/tests/liveness_corpus.rs` harness asserting current (wrong)
  outcomes, each assertion annotated with the task that will flip it.

**Done when:** `cargo test -p engram-code --test liveness_corpus` green with
wrong-verdict expectations; corpus covers all four cases.

### T2: Qualified-suffix resolution rung

**Depends on:** T1

**Tests:**
- `a::b::Type::name` (≥3 segments) resolves when `Type::name` is declared
  (AC1, agentzero shape).
- Bare-name and dotted-receiver behavior unchanged (existing suite green).
- Ambiguous suffix stays in the ledger with candidates (RFC-0020).

**Approach:**
- `core/code/src/resolution.rs`: before the bare-name ladder, try the
  longest `Type::name` suffix of the as-written reference, shortening toward
  the tail; first unique resolution wins.

**Done when:** corpus qualified-path case flips to resolved; full
`engram-code` suite green.

### T3: `registers` edge extraction

**Depends on:** T1

**Tests:**
- `registry.register(Arc::new(T::new(…)))` yields subject→T `registers` edge
  (AC3).
- `builder.with_x(T::new())` yields the same.
- No `registers` edge for plain `T::new()` without a registration receiver.

**Approach:**
- `core/domain`: add `CodeEdgeKind::Registers` (+ contracts/TS regen).
- `core/code/src/edges.rs` + parser: detect register-flavored method names
  (`register*`) and builder `with_*` receivers wrapping a constructor call
  argument; emit the edge.

**Done when:** corpus registration case flips; `pnpm run contracts:generate`
+ typecheck green.

### T4: Constructed-impl dispatch liveness

**Depends on:** T3

**Tests:**
- Type with incoming `registers` edge ⇒ its `impl Trait` methods classify
  live (AC4, `MemoryLifecycleExecutor` shape).
- No infinite regress on self-registrations.

**Approach:**
- `codegraph/queries`: liveness inputs = call edges (existing) + `registers`
  edges + contains-liveness; constructed type marks contained impl methods
  live.

**Done when:** corpus dispatch case flips; `codegraph-queries` tests green.

### T5: `dead_code` v2 classification + surface parity

**Depends on:** T2, T4

**Tests:**
- Classifier buckets: test doubles / `#[cfg(test)]`-only → `test-only`;
  entry-point-shaped → `external`; unresolvable-provenance → `inconclusive`;
  remainder with zero liveness → `confirmed-unused` (AC5).
- N-API `dead_code_json` carries the same buckets (AC6); parity lint green.

**Approach:**
- `codegraph/queries`: `classify_dead` over the graph using extraction
  provenance (cfg-test/pub/fixture tags if present) + liveness inputs.
- Surface through `code op=health` (additive) and `dead_code_json`.

**Done when:** all buckets observable at both surfaces; AC8 gate list green.

### T6: Cross-repo live verification + spec close

**Depends on:** T5

**Tests:**
- Rebuild release server; re-scan `~/.engram/mem-alpha-self`; assert the
  four mem-alpha false positives left the dead list (AC2, goal-based).
- Re-scan agentzero store; `DelegateToZbotTool` reports live via
  `code op=impact`.

**Approach:**
- `scripts/dev/mcp_driver.py` battery; diff dead lists; update backlog +
  spec checkboxes; store summary via harness engram MCP.

**Done when:** AC2 verified live; spec checkboxes ticked; backlog updated.

## Rollout

Pure-logic + additive-contract change; no infra, no external systems, no
deployment sequencing. Rollback = revert; stores re-scan idempotently (edges
retract and re-ingest on next scan).

## Risks

- Suffix ladder over-resolves (a short suffix matching an unrelated symbol):
  mitigated by longest-first + uniqueness requirement + ledger on ambiguity.
- Register-pattern extraction false positives (`register`-named methods that
  aren't registration): acceptable — a spurious `registers` edge only marks
  the target live, and the pattern requires a constructor argument.
- Existing consumers parse the raw dead list: unchanged default shape; the
  classification is additive.

## Changelog

- 2026-09-03: initial plan.
- 2026-09-03: review pass 1 (adversarial-reviewer, spec mode) — T2 seam
  corrected to the declaration-side noise drop (corpus-verified); N-API
  sibling-binding mechanism recorded; `generated`/`inconclusive` buckets
  deferred with backlog anchor; classifier moved to `liveness.rs`; found +
  fixed en route: consolidated `scan` dispatcher dropped `force`/`scan_config`.
  All ACs verified live 2026-09-03 (mem-alpha-self + zbot stores).
