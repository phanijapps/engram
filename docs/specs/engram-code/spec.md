# Spec: engram-code — dedicated code-indexing crate with codegraph-parity extraction

- **Status:** Draft
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0022 (engine neutrality + surface parity), ADR-0009 (retrieval seam, read-path only), RFC-0012 (codegraph on-top layer), RFC-0020 (unified knowledge indexing — this spec is the Phase 2 vehicle; identity conforms to §Phase 2 `{receiver}::{name}`), `extraction-quality` spec (Phase 1 compat surface), RFC-0009/ADR-0018 (re-ingest retraction convergence), ADR-0017 (multi-repo scope model), ADR-0028 (new — `engram-code` crate placement)
- **Brief:** none
- **Contract:** `docs/domain-data-model.md` gains a code-indexing section extending the Knowledge Model (typed code edge kinds incl. `routes_to`, the receiver-qualified identity rule + scope-wide multi-candidate symbol table, `UnresolvedReference` record) → generated `contracts/v1` types; no separate API contract file (the surface is the Rust facade + N-API + both MCP servers, covered by surface parity). **Classification (AGENTS.md doc standards):** additive for the edge kinds and `UnresolvedReference`; the receiver-qualified `entity.name` formation changes persisted values for method entities (bare → `Receiver::name`) — classified compatible-with-convergence: stores converge via re-scan, and query compatibility is held by the suffix resolver (AC3).
- **Shape:** mixed (data model + extraction engine + query tools)

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Code indexing lives in one dedicated behavior crate, `engram-code` (`core/code`),
which `adapters/ingest` uses to build the code graph. The crate owns the full
deterministic extraction pipeline — tree-sitter parsing, the Phase-2
receiver-qualified identity model with a scope-wide multi-candidate symbol
table, typed structural edges, cross-file resolution with an honest
unresolved-reference ledger, and framework-aware pattern resolution — informed
by `@colbymchenry/codegraph`'s design but implemented natively per engram's
architecture. Agents get a materially more answerable code graph: name
resolution never silently overwrites a collision (every same-named symbol
coexists as a candidate, disambiguated by repo/path), structural edges
(`imports`, `contains`, `extends`, `implements`, `calls`, `routes_to`) make
dependency and impact questions structural rather than name-guessed, cross-file
edges either resolve or are recorded as pending (and heal when the target file
lands), framework routes and callbacks appear as first-class edges, and two new
query surfaces — file-level dependencies and a natural-language `explore` entry
point — reachable from the Rust facade, the N-API binding, `@engram/node`, the
Rust stdio MCP (`engram-mcp`), and the TS HTTP MCP (`engram-mcp-http`), all
reflected in `CapabilityReport`.

## Boundaries

The three-tier guard that keeps an implementing agent inside the lines.
*Always do* applies without asking; *Ask first* requires human sign-off
before proceeding; *Never do* is a hard rule, even under time pressure.

### Always do

- Keep `engram-code` pure and deterministic: no filesystem I/O, no git, no
  storage, no async runtime, no LLM — its public API maps source text to
  domain objects (`parse → CodeModel`, `resolve(CodeModel[]) → ResolvedCodeGraph`),
  matching the behavior-crate stereotype in `docs/architecture/reference.md`.
- Record every unresolvable cross-file reference in the unresolved-reference
  ledger (with candidate names and status) instead of silently dropping it;
  the orphan sweep re-attempts pending references whenever new symbols land.
- Update `docs/domain-data-model.md` and regenerate contracts before changing
  any Rust/TS public type (`pnpm run contracts:generate`).
- Wire every new query or capability through **all five surfaces** —
  `engram-integration` facade, N-API binding, `@engram/node`, the Rust stdio
  MCP (`mcp/engram-mcp`), and the TS HTTP MCP (`packages/runtime`
  `engram-mcp-http`) — keeping the two MCP servers' tool lists pinned equal by
  a cross-server tool-list parity check (added with this spec's first query
  task; a shared tool-name fixture asserted against both the TS HTTP tool
  table and the Rust stdio `registry.list()`), and reflect the capability in
  `CapabilityReport` (ADR-0022 parity rule).

### Ask first

- Adding any tree-sitter grammar beyond the set carried over from
  `adapters/ingest` (dependency-surface growth).
- Extending the framework-resolver set beyond the starter five (Express,
  NestJS, FastAPI, Flask, Spring routes + React callback synthesis).
- Any new table outside the knowledge store (e.g. persisting ledger state in
  another adapter's schema).

### Never do

- No LLM anywhere in `engram-code` — the LLM client stays TypeScript-only
  (pi SDK), per `docs/architecture/reference.md` constraints.
- No god crate: `engram-code` must not own scanning, git plumbing, hashing, or
  persistence — those stay in `adapters/ingest` and the knowledge store
  adapters. Structural rule; a diff that moves them in is wrong.
- Do not depend on, wrap, or vendor `@colbymchenry/codegraph` — `engram-code`
  is a native implementation informed by its design; no npm or crate
  dependency on it.
- Do not re-introduce path-qualified `entity.name` (`{repo}/{path}::{name}`
  was reverted by RFC-0020's revision note): entity names stay bare (Phase 1)
  → receiver-qualified (this spec, Phase 2), with repo/path as disambiguators
  in provenance and the resolution index — never in the name.
- Do not break bare-name/suffix query compatibility: existing `::{suffix}`
  resolution keeps answering on **both** MCP servers.
- Do not weaken or special-case retraction semantics to make extraction
  easier (no skipping the embeddings→chunks→document→graph cascade).

## Testing Strategy

- **Extraction, identity, resolution invariants — TDD.** Pure functions over
  fixture sources: receiver-qualified name formation, multi-candidate symbol
  table behavior (no silent overwrite), typed-edge emission, resolution
  outcomes (resolved / pending-with-candidates / rejected), noise filtering,
  framework patterns. These compress to unit tests inside `engram-code`.
- **Domain contract changes — goal-based.** `pnpm run contracts:generate` is
  clean and `pnpm run typecheck` compiles the regenerated TS types.
  (The `.codex/hooks/` check scripts are deleted in this tree; contracts are
  gated by generation + typecheck, not the legacy hooks.)
- **Relocation of extraction out of `adapters/ingest` — goal-based.** The
  existing `adapters/ingest` and knowledge-adapter test suites pass unchanged
  as the regression net (the `sqlite-consolidation` relocation precedent:
  behavior must not change while code moves).
- **Cross-file behaviors (orphan sweep, retraction with ledger, re-ingest
  convergence) — goal-based, exercised by integration tests** over a
  multi-file polyglot fixture repo, since they only prove out across the
  ingest → store boundary.
- **New query surfaces (`file_dependencies`, `explore`) and identity
  compatibility — goal-based integration tests** over the same fixture, with
  surface parity gated by a cross-server tool-list parity check (added in
  T10: a shared tool-name fixture asserted against both the TS HTTP tool
  table and the Rust stdio `registry.list()`; no such cross-server
  comparison exists today) plus facade/binding tests for the N-API path.
- **Scale benchmark — manual QA.** A recorded run against a large public
  repository with numbers documented in `docs/perf/`.

## Acceptance Criteria

- [ ] `engram-code` exists at `core/code` as a behavior crate with no I/O,
      git, storage, async-runtime, or LLM dependencies; `adapters/ingest`
      consumes it to build the code graph, and no tree-sitter dependency
      remains in `adapters/ingest` (`cargo tree -p engram-ingest` audit).
- [ ] Entity names follow the RFC-0020 Phase 2 receiver-qualified form
      (`{receiver}::{name}` where a receiver exists, bare otherwise), and the
      global name-resolution index is a scope-wide multi-candidate symbol
      table: two same-named symbols from different files (or repos, per the
      ADR-0017 multi-repo scope model) coexist as candidates keyed with their
      repo/path discriminators — no silent last-write-wins overwrite.
- [ ] Existing bare-name and `::suffix` queries still resolve on **both** MCP
      servers (the Rust stdio server and the TS HTTP server) against the new
      identity model, verified by the existing suffix-resolver fixtures.
- [ ] Extraction emits typed structural edges — `imports`, `contains`,
      `extends`, `implements`, `calls`, and `routes_to` — with counts
      assertable on the fixture repo.
- [ ] A cross-file reference to a not-yet-ingested symbol produces a
      pending ledger record carrying candidates; ingesting the target file
      later resolves it via the orphan sweep without re-ingesting the
      referring file.
- [ ] The framework resolver starter set extracts route entities wired to
      their handlers via `routes_to` edges, and React event-handler callbacks
      appear as `calls` edges, on dedicated fixtures.
- [ ] `file_dependencies` returns the file-level import graph, and `explore`
      answers a natural-language query with a relevance-seeded bounded
      subgraph (default budget: 24 nodes, expansion depth 2, at most 64
      edges) — both reachable from the Rust facade, the N-API binding,
      `@engram/node`, both MCP servers (tool lists pinned equal by the
      cross-server tool-list parity check), and listed in `CapabilityReport`
      as `code_graph`.
- [ ] The lexical lane indexes code-symbol signatures and docstrings; a
      keyword query matches a symbol by a signature fragment.
- [ ] Re-ingest convergence is unchanged: changing or removing a fixture
      file retracts prior entities, edges, and ledger rows (existing
      reconcile tests pass without modification).
- [ ] A scale benchmark run against a large public repository is documented
      in `docs/perf/` (index time, symbol and edge counts, sync time after a
      one-file edit).

## Assumptions

- Technical: code indexing lives in `adapters/ingest/src/` (`extractor.rs`,
  `tree_sitter_chunker.rs`, `code_symbol.rs`, `scanner.rs`, `reconcile.rs`)
  with query tools in `mcp/engram-mcp/src/codegraph.rs` + the TS HTTP MCP
  (`packages/runtime/src/mcp/tools.ts`) and analytics in
  `codegraph/{queries,temporal}` (source: directory listing + subagent
  report, 2026-08-23).
- Technical: RFC-0020 Phase 1 shipped bare logical names; Phase 2 names
  `{receiver}::{name}` identity and typed code edges as the next step, and
  the shipped extractor documents the bare-name index as
  last-write-wins pending "a Phase 2 scope-wide symbol table"
  (source: `docs/rfcs/0020-unified-knowledge-indexing.md:58,188`;
  `adapters/ingest/src/extractor.rs:475-487`).
- Technical: a BM25 lexical lane exists at
  `adapters/retrieval/tantivy-lexical` for signature/docstring indexing
  (source: `adapters/retrieval/tantivy-lexical/src/`).
- Technical: contract changes flow `docs/domain-data-model.md` →
  `pnpm run contracts:generate` → generated TS types (source: AGENTS.md,
  root `package.json:16-19`).
- Technical: `@colbymchenry/codegraph`'s framework resolvers and callback
  synthesis are deterministic pattern-matching, not LLM — eligible for
  Rust-core placement (source: cg-pkg subagent report on v1.5.0 source,
  2026-08-23).
- Product: scope is all eight deltas from
  `docs/research/tencentdb-agent-memory-comparison.md` §7 (typed edges,
  receiver-qualified identity + multi-candidate symbol table,
  unresolved-refs ledger, framework resolvers, file-level dependencies,
  signature/docstring lexical lane, NL `explore`, scale benchmarks), with
  benchmarks as a documented run rather than a performance bar (source:
  user confirmation 2026-08-23, "I want them all").
- Product: architecture — `engram-code` is the new crate;
  `adapters/ingest` uses it to build the code graph; extraction and
  resolution intelligence move into the crate, ingest keeps scan/git/retract
  orchestration, analytics stay on-top in `codegraph/` (source: user
  confirmation 2026-08-23, "ingest will use this new crate build codegraph").
- Process: new-crate boundary decisions are ADR-gated; ADR-0028 records the
  `engram-code` placement and the tree-sitter-in-behavior-crate precedent
  (source: `docs/adr/` precedent — 27 ADRs incl. ADR-0022, ADR-0027; 0028 is
  free).

Revision note (2026-08-23, spec-mode adversarial review pass 1): identity
originally re-introduced path-qualified `entity.name`; corrected to conform
to RFC-0020's Accepted model (Phase 2 receiver-qualified names, path as
disambiguator) and the collision AC re-anchored to the resolution index —
the actual defect. Route edges renamed `references` → `routes_to` per
RFC-0020 §Phase 2; TS HTTP MCP added to the parity chain; legacy `.codex`
hook references replaced with the real gate artifacts.
