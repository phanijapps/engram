# Plan: engram-code — dedicated code-indexing crate with codegraph-parity extraction

- **Spec:** [`spec.md`](spec.md)
- **Status:** Drafting

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog
> at the bottom.

## Approach

Carve the deterministic code-extraction intelligence out of `adapters/ingest`
into a new behavior crate `engram-code` (`core/code`), then upgrade it with the
codegraph-parity features in dependency order: contracts first (typed edges
incl. `routes_to`, the Phase-2 receiver-qualified identity rule, multi-candidate
symbol table, ledger record), then the crate with parsing + identity, then
resolution + ledger, then framework patterns, then ingest wiring, then queries
and the lexical lane, and finally the benchmark run. The riskiest part is the
identity change (Phase-2 receiver-qualified names + a multi-candidate symbol
table replacing the last-write-wins bare index) while keeping bare/`::suffix`
query compatibility on both MCP servers — it lands early (T3) behind tests,
before anything builds on it. The relocation follows the `sqlite-consolidation`
precedent: behavior must not change while code moves; existing suites are the
regression net.

Stack (per `docs/architecture/reference.md`, normative): Rust behavior crate
stereotype at `core/code` — depends on `engram-domain`/`engram-runtime` only,
tree-sitter grammars in-crate as pure parsing libraries (the ADR-0028
precedent decision); `adapters/ingest` keeps filesystem/git/hash/reconcile and
calls the crate; ledger + typed-edge persistence extend the knowledge store
(`adapters/knowledge/sqlite`); queries compose existing seams (`RetrievalIndex`
lexical lane, bounded graph traversal) and ship through the facade → N-API →
`@engram/node` → both MCP servers (Rust stdio `engram-mcp`, TS HTTP
`engram-mcp-http`) parity chain.

## Constraints

- ADR-0022: neutral layers never name engines; every capability reaches all
  surfaces (incl. both MCP servers, pinned equal by the cross-server
  tool-list parity check added in T10) + `CapabilityReport`.
- `extraction-quality` spec Ask-first ("any change to the cross-file
  call-resolution index semantics (`name_index`)") is consciously superseded
  here: this spec is the RFC-0020 Phase 2 vehicle with user sign-off
  (2026-08-23), and `extractor.rs:479-480` already promises the Phase-2
  scope-wide symbol table this plan builds.
- ADR-0009: retrieval seam is read-path only; `explore` composes existing
  lanes, adds no write path.
- RFC-0012: analytics (PageRank, communities, temporal scoring) stay in
  `codegraph/` on-top crates; `engram-code` does not absorb them.
- RFC-0020: this is Phase 2 — entity names follow `{receiver}::{name}` (§58);
  path-qualified `entity.name` stays reverted; the Phase 1 bare/`::suffix`
  compatibility surface stays intact on both transports.
- RFC-0009/ADR-0018: retraction convergence is untouched; ledger rows join the
  cascade.
- ADR-0017: multi-repo scopes exist; the symbol table keys candidates with
  repo/path discriminators so cross-repo same-name symbols coexist.
- AGENTS.md: crate roots are facades (whitelist `pub use`); no god modules —
  extraction, resolution, identity, patterns are focused modules.
- The `.codex/hooks/` check scripts are deleted in this tree; gates are the
  remaining AGENTS.md validation set (`cargo fmt/check`, contracts, typecheck,
  test, build) plus the cross-server tool-list parity check (T10).

## Construction tests

**Integration tests (cross-task):** a multi-file polyglot fixture repo under
`engram-code/tests/fixtures/` (Rust + TypeScript + Python sources with
cross-file calls, an import cycle, same-named symbols in two files and across
two repos, an Express route file, a React component) drives the whole stack:
ingest via `adapters/ingest` → knowledge store → queries. Used by T3, T4, T5,
T6, T8, T10.
**Manual verification:** T11 benchmark run (recorded numbers in `docs/perf/`).

## Design (LLD)

- **Data & schema:** `docs/domain-data-model.md` gains a code-indexing section
  extending the Knowledge Model: code edge kinds (`imports`, `contains`,
  `extends`, `implements`, `routes_to` alongside `calls`), the RFC-0020
  Phase-2 receiver-qualified identity rule plus the scope-wide multi-candidate
  symbol table, and an `UnresolvedReference` record (from_symbol,
  reference_name, candidates, status). Knowledge store adds
  `knowledge_unresolved_refs` keyed `(source, name, status)` with the same
  lifecycle as relationships.
- **Module decomposition (`engram-code/src/`):** `lib.rs` facade;
  `code_model.rs` (pure parse output); `parser/` (tree-sitter per-language,
  moved from `adapters/ingest`); `identity.rs` (receiver-qualified names,
  document symbol table, multi-candidate global index); `edges.rs`
  (typed-edge extraction); `resolution/` (scoped/qualified/dotted-receiver
  matching, ledger emission, orphan sweep queries); `frameworks/` (route +
  callback pattern resolvers); `noise.rs` (moved filter). Public API:
  `parse(path, lang, source) -> CodeModel` and
  `resolve(models) -> ResolvedCodeGraph`.
- **Interfaces & contracts:** facade methods `file_dependencies(scope)`,
  `explore(query, budget)` (defaults: 24 nodes, expansion depth 2, ≤64 edges);
  N-API JSON commands; MCP tools `file_dependencies`, `explore` added to BOTH
  `mcp/engram-mcp` (Rust stdio) and `packages/runtime/src/mcp/tools.ts`
  (TS HTTP), keeping a cross-server tool-list parity check green (new —
  see T10); `CapabilityReport` entry `code_graph`.
- **State & control flow:** ingest scan (adapter) → per-file `parse` (rayon,
  unchanged) → global `resolve` → store upsert with retraction of
  `(stable_source_key, path)` priors incl. ledger rows → orphan sweep for
  pending refs against the new symbol table.
- **Resilience:** parse errors are per-file (file skipped, error recorded in
  document metadata — current behavior); resolution never fails a scan;
  ledger makes best-effort failures visible instead of silent.

## Tasks

### T0 — ADR-0028: `engram-code` crate placement
Depends on: none. Mode: goal-based.
Tests: ADR file exists at `docs/adr/0028-engram-code-crate.md` with status;
ADR index updated; AGENTS.md target-shape tree updated.
Approach: Write the ADR (Accepted with this spec): behavior crate at
`core/code`; tree-sitter grammars as pure in-crate parsing libraries (not
infra — the deviation from "parsing lives in adapters" and why);
`adapters/ingest` remains the I/O orchestrator. Verify the
`docs/specs/README.md` `extraction-quality` entry reflects the shipped
Phase-1 state (already corrected in this spec's authoring PR).

### T1 — Domain contracts: typed edges, Phase-2 identity, ledger record
Depends on: T0. Mode: TDD + goal-based.
Tests: unit tests for new domain invariants (edge-kind set
`calls|imports|contains|extends|implements|routes_to` closed under serde;
`UnresolvedReference` status transitions);
`pnpm run contracts:generate` clean; `pnpm run typecheck` compiles.
Approach: Update `docs/domain-data-model.md` first (contract-first), then
`core/domain` types (code edge kinds on the relationship model, the
receiver-qualified identity rule + multi-candidate symbol-table contract,
`UnresolvedReference`), regenerate `contracts/v1` + TS types. No behavior
change yet.

### T2 — `engram-code` scaffold + parser relocation
Depends on: T1. Mode: goal-based (relocation; regression net).
Tests: existing `adapters/ingest` + knowledge-adapter suites pass unchanged;
`cargo tree -p engram-ingest --depth 1` shows no tree-sitter crate in the
direct dependency list (tree-sitter arrives transitively via `engram-code`,
which is the design).
Approach: Create `core/code` crate (facade root, focused modules); move
`tree_sitter_chunker.rs`, `code_symbol.rs`, noise filter from `adapters/ingest`
verbatim into `parser/` + `noise.rs`; `adapters/ingest` depends on
`engram-code` and re-points call sites. Pure move, zero behavior delta (the
`sqlite-consolidation` precedent).

### T3 — Phase-2 identity + multi-candidate symbol table
Depends on: T2. Mode: TDD.
Tests: receiver-qualified names form where a receiver exists, bare otherwise;
two same-named symbols in different files (and across the two fixture repos)
coexist in the global index as candidates keyed with repo/path discriminators
— no overwrite; local-callee qualification uses the document symbol table;
bare and `::suffix` lookups still resolve in-crate.
Approach: `identity.rs`: per-document symbol table during parse; global index
becomes `name → Vec<candidate{id, repo, path}>` (replaces
`register_in_name_index`'s last-write-wins insert); disambiguation rules:
same-document > same-repo > multi-candidate (resolution returns ambiguous →
ledger records candidates).

### T4 — Typed structural edges
Depends on: T3. Mode: TDD.
Tests: fixture repo yields `imports` (file→file), `contains` (parent→member),
`extends`/`implements` edges with expected counts; `calls` edges unchanged.
Approach: `edges.rs` extracts import statements (per-language patterns),
containment from AST parentage, inheritance clauses; emits T1 edge kinds.
Calls extraction untouched. (`routes_to` arrives with T7's framework
patterns.)

### T5 — Resolution upgrade + unresolved ledger (pure)
Depends on: T4. Mode: TDD.
Tests: exact receiver-qualified match resolves; bare last-segment match
resolves when unique in scope; ambiguous bare names resolve to the
same-document/same-repo candidate or emit a ledger record with all
candidates; dotted-receiver chains resolve link-by-link; conformance-chained
calls (`this.delegate.method()` via protocol type) resolve.
Approach: `resolution/`: scoped-chain and dotted-receiver matching (informed
by codegraph's name-matcher), import-scoped resolution first, ledger records
for failures and ambiguity. Pure output — persistence is T6.

### T6 — Ledger persistence + orphan sweep in knowledge store
Depends on: T5. Mode: goal-based, integration.
Tests: ingest file A referencing absent symbol X → pending row; ingest file
B defining X → row resolves without re-ingesting A; re-ingest of A retracts
its ledger rows; reconcile suite passes unmodified.
Approach: `knowledge_unresolved_refs` table in `adapters/knowledge/sqlite`
behind an `engram-knowledge` repository port method; orphan sweep runs
post-ingest over pending rows against the fresh symbol table; retraction
cascade extended to ledger rows.

### T7 — Framework resolvers (starter set)
Depends on: T5. Mode: TDD.
Tests: Express fixture yields a route entity + `routes_to` edge to its
handler; NestJS/FastAPI/Flask/Spring decorators likewise; React fixture
yields `calls` edges from `onClick`-style props to handlers.
Approach: `frameworks/` — deterministic pattern extractors per framework
(decorator/attribute/router-method patterns), emitting route entities and
`routes_to` edges via the T5 resolution machinery. (.js is already parsed by
the TypeScript grammar — no new grammar dependency.)

### T8 — Ingest end-to-end on `engram-code`; remove old extraction
Depends on: T6, T7. Mode: goal-based.
Tests: full ingest of the polyglot fixture produces entities with
receiver-qualified identities, all six edge kinds, resolved-or-pending
ledger; old extractor paths deleted; workspace `cargo test` green; the
existing suffix-resolver fixtures pass on BOTH MCP servers (Rust stdio +
TS HTTP) against the new identity model.
Approach: `adapters/ingest` pipeline: scan → parse (rayon) → resolve → upsert
with retraction → sweep. Delete superseded extraction code paths. Update MCP
scan tools' doc text only (shapes unchanged).

### T9 — Lexical lane: signatures + docstrings
Depends on: T8. Mode: goal-based, integration.
Tests: keyword query hits a code symbol by signature fragment and by
docstring phrase on the fixture; existing lexical fixtures unmodified.
Approach: code-symbol chunk text includes signature + docstring when indexed
into `adapters/retrieval/tantivy-lexical`; no new lane (ADR-0009 seam reuse).

### T10 — Queries: `file_dependencies` + `explore` across all surfaces
Depends on: T8. Mode: goal-based, integration + tool-parity.
Tests: facade returns the file import graph from `imports` edges; `explore`
returns a relevance-seeded bounded subgraph (defaults 24 nodes / depth 2 /
≤64 edges) for an NL query; N-API command + `@engram/node` method answer
identically; BOTH MCP servers expose `file_dependencies` + `explore`; a NEW
cross-server tool-list parity check passes — a shared tool-name fixture
(e.g. `mcp/engram-mcp/tests/tool_names.txt`, consumed by both suites)
asserted against the Rust stdio `registry.list()` in a Rust test and
against the TS HTTP tool table in `packages/runtime/test/mcp.test.ts` (which
today pins only the TS list's length — no cross-server comparison exists);
`CapabilityReport` exposes `code_graph` covering both queries.
Approach: facade methods over the knowledge graph (file-level rollup of
`imports`; lexical seed + bounded BFS expansion reusing `graph_neighbors`);
bind through `bindings/node`, `packages/node`, both MCP servers' tool tables;
author the shared tool-name fixture + both assertions; update the capability
report.

### T11 — Scale benchmark, documented
Depends on: T8. Mode: manual QA.
Tests: `docs/perf/engram-code-benchmark.md` exists with repo, hardware,
full-index time, symbol/edge counts, one-file-edit sync time.
Approach: Run ingest against a large public repo (e.g. Linux or Swift-scale
clone), record numbers; note the full-load analytics ceiling stays
(RFC-0012).

### T12 — Gates + docs close-out
Depends on: T0–T11. Mode: goal-based.
Tests: the AGENTS.md validation set that exists in this tree is green —
`cargo fmt --all`, `cargo check --workspace`, `pnpm run contracts:generate`,
`pnpm run typecheck`, `pnpm run test`, `pnpm run build`, and the dual-MCP
tool-parity fixture check; `docs/product/engram.md` code-indexing section
updated to as-built.
Approach: final sweep — product doc, specs README status, any drift fixes.

## Changelog

- 2026-08-23: initial plan from the §7 comparison deltas (all eight), scoped
  with user sign-off on architecture ("ingest uses the new crate").
- 2026-08-23: spec-mode adversarial review pass 1 (2 Critical, 6 Major,
  4 Minor) — identity conformed to RFC-0020 Phase 2 (`{receiver}::{name}`,
  path as disambiguator; collision AC re-anchored to the multi-candidate
  resolution index, the actual defect at `extractor.rs:477`); `routes_to`
  edge kind added to T1/T4/T7 and ACs (was ad-hoc `references`); TS HTTP MCP
  added to the parity chain everywhere; `.codex/hooks` gate references
  replaced with real artifacts (tool-parity test + contracts/typecheck);
  `cargo tree -p engram-ingest` fixed; `explore` budget bars named (24
  nodes / depth 2 / ≤64 edges); capability entry renamed `code_graph`;
  stale extraction-quality README entry fix moved into T0; both-MCP compat
  verification added to T8/AC3.
- 2026-08-23: pass 2 (1 Major, 3 Minor) — the "existing tool-parity test"
  claim corrected: no cross-server comparison exists today, so T10 now ADDS
  one (shared tool-name fixture asserted by a Rust registry test and the TS
  mcp test); Contract field gains the AGENTS.md compatible-with-convergence
  classification; Constraints record extraction-quality's `name_index`
  Ask-first as consciously superseded (user sign-off 2026-08-23); T0's
  stale README step reworded to verify-only.
