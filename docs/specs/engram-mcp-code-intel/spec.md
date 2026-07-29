# Spec: engram-mcp-code-intel

- **Status:** Implementing
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** RFC-0015, ADR-0008, ADR-0009, ADR-0022, ADR-0020, ADR-0025
- **Brief:** none
- **Contract:** none — MCP tool input schemas live in-crate (consistent with Phase 1 + the two interim servers).
- **Shape:** service — adds code-intelligence tools + new `engram-integration` exposure to the existing MCP server.

> **Spec contract:** this document defines what "done" means for Phase 2 (code intelligence) of RFC-0015. The implementing PR must match this spec, or update it. Verification must be derivable from it. Phase 1 (`engram-mcp-core`) is the foundation this builds on.

## Objective

The `engram-mcp` server gains code intelligence: an agent can `scan_repo` a codebase (treesitter ingestion, routed through one `EngramProvider`) and then ask goal-oriented code questions — `symbol_context`, `change_impact`, `code_health`, `architecture`, `api_topology`, `whats_changed` — plus keyword `search` — all returning answers composed from the code graph that lives in the same fused-per-project store as the docs and concepts. Success: after `scan_repo`, an agent gets structured code-intel answers and `search`/`recall` surface code symbols alongside docs and memories, with no `SqlKnowledgeStore`/engine types in the MCP and no provider bypass (the old `codegraph/mcp-server` bypassed the provider; Phase 2 does not).

`get_context` (RFC-0013 `ContextSubgraph`), unified `capability_report`, deprecation of the two interim servers, and N-API parity for the new handles are Phase 3 / deferred.

## Boundaries

### Always do

- Route every tool through `EngramProvider`. Close the three Phase-2 exposure gaps as **new ports in `engram-integration`** — a `KnowledgeQuery` port (`list_entities`/`list_relationships`) and a lexical-feed surface over the **in-RAM Tantivy `LexicalIndex`** that `bootstrap_sqlite` constructs for unified recall — exposed as new provider handles. (Per the user's rule: expose via engram-integration.)
- Compose the six code-intel tools from the **pure** `engram-codegraph-queries` / `engram-codegraph-temporal` functions (they take `&[KnowledgeRelationship]` / `&[VersionedSymbol]` / `&str`); fetch the edge/entity list via `KnowledgeQuery`, then call the pure math.
- Land code entities in the **project workspace** (fused-per-project) so `recall` sees code + docs + concepts together.
- Use typed `CoreError`/`CoreResult`; `ToolError` at the MCP edge.

### Ask first

- Any change to the core `engram-knowledge` port traits (`KnowledgeRepository`/`KnowledgeGraphRepository`). Phase 2 adds exposure in `engram-integration` instead; a core-trait change would need an ADR.
- Adding `engram-store-lexical` or `engram-codegraph-*` as direct MCP dependencies beyond the minimum needed.

### Never do

- **Do not bypass `EngramProvider`** — no direct `SqlKnowledgeStore`, `LexicalIndex`, or engine types in `mcp/engram-mcp/src` (the old codegraph MCP's pattern is explicitly rejected here). Engine-neutrality (ADR-0022).
- **Do not call an LLM** in the server.
- **Do not wire or depend on the SurrealDB backend** — out of scope (RFC-0015 Q5).
- **Do not change `engram-domain` contracts** in Phase 2.
- **Do not reimplement** the graph/temporal math — reuse `engram-codegraph-queries` / `engram-codegraph-temporal`.
- **Do not change `scan_repository`'s signature.**

## Testing Strategy

- **Pure composite composition:** TDD — for each composite, assert against a **named fixture with hardcoded expected JSON** (e.g. `result.callers.contains("foo")`), not equivalence to a production-fn call (the underlying `engram-codegraph-*` math is already tested; a round-trip assertion would mirror it, not test the contract).
- **New `engram-integration` exposure:** goal-based integration (under `cargo test -p engram-integration`) — open a provider; assert the new handles are `Some` and `list_entities`/`list_relationships`/lexical-feed round-trip.
- **`scan_repo` end-to-end + fused recall:** goal-based integration — scan a fixture repo, then `recall`/`search` returns its symbols fused with an indexed doc **and** a written concept (`put_entity` kind `concept`); scope isolation holds.
- **Engine-neutrality + no-bypass + feature-pinning:** goal-based — the neutrality gate covers `mcp/engram-mcp/src` **and** the new `core/integration/src/knowledge_query.rs` + `lexical_feed.rs`; grep proves no `SqlKnowledgeStore`/`LexicalIndex`/engine types in the MCP; `mcp/engram-mcp/Cargo.toml` pins `engram-integration` to `features = ["sqlite"]`.

## Acceptance Criteria

- [ ] `scan_repo` ingests a fixture repo via `scan_repository` routed through the provider (no direct `SqlKnowledgeStore`); afterward the code symbols are recallable in the project workspace.
- [ ] A `KnowledgeQuery` exposure in `engram-integration` provides `list_entities(scope)` / `list_relationships(scope)` reachable as a provider handle; the composites use it to obtain their edge/entity input.
- [ ] (deferred: phase-2-search-entity-resolver) A lexical-feed exposure lets `scan_repo` feed code-symbol names so `search` returns code symbols. **Deferred**: the unified-recall lexical resolver is chunk-based (`KnowledgeLexicalResolver` calls `get_chunk`); entity-id hits are dropped. Needs an entity-id resolver lane.
- [ ] Five of six consolidated tools are exposed and return structured answers: `symbol_context`, `change_impact`, `code_health`, `architecture`, `whats_changed` (composing `engram-codegraph-queries` + `engram-codegraph-temporal`). `api_topology` is deferred (needs chunk-text access).
- [ ] No `Sql*`/`Surreal*`/`LexicalIndex`/engine types in `mcp/engram-mcp/src`; the engine-neutrality gate passes for `mcp/engram-mcp/src` **and** the new `core/integration/src/knowledge_query.rs` + `lexical_feed.rs`; `mcp/engram-mcp/Cargo.toml` pins `engram-integration` to `features = ["sqlite"]` (no `surreal`).
- [ ] `recall`/`search` fuse code symbols with docs + a written concept within the project scope (integration test against a scanned fixture + an indexed doc + a `put_entity` concept).
- [ ] No LLM in the server; no `engram-domain` contract change.
- [ ] `cargo fmt --all`, `cargo check --workspace`, `cargo test -p engram-mcp -p engram-integration` pass.
- [ ] (deferred: phase-2-napi-parity) N-API (`bindings/node`) parity for the new `knowledge_query` / `lexical_feed` handles — surface parity (ADR-0022) requires they reach TS too; deferred to a follow-up with a `docs/backlog.md` entry.

Deferred to Phase 3 / follow-up: `get_context` (`ContextSubgraph`), unified `capability_report`, deprecation of `codegraph/mcp-server` + `memory/mcp-server`, the `directional` temporal mode of `whats_changed` (needs per-project scan-baseline retention), and N-API parity for the new handles.

## Assumptions

- Technical: `engram-codegraph-queries` functions are pure over `&[KnowledgeRelationship]` (and `&str` for code-text helpers); `engram-codegraph-temporal` over `&[VersionedSymbol]` — no store handle. (source: `codegraph/queries/src/queries.rs`, `codegraph/temporal/src/scoring.rs`)
- Technical: `scan_repository` needs one `R: KnowledgeRepository + KnowledgeGraphRepository + Send + Sync`; the provider exposes two separate handles, so a fan-in adapter is required at the MCP edge. (source: `adapters/ingest/src/scanner.rs:108`, `core/integration/src/provider.rs:321-336`)
- Technical: `list_entities`/`list_relationships` are inherent on `SqlKnowledgeStore` only, not on the port traits, and `EngramProvider` is extended additively with new handles — so new `engram-integration` exposure is the path. (source: `adapters/sqlite/.../service.rs:248,412`)
- Technical: the unified-recall lexical lane is an in-RAM Tantivy `LexicalIndex` built in `bootstrap_sqlite` and currently unfed; `search` needs a new feed exposure. (source: `core/integration/src/sqlite/bootstrap.rs:293-301`)
- Technical: `ScanOptions { scope, policy, actor, source_name, max_bytes, manifest }`. (source: `adapters/ingest/src/scanner.rs:54`)
- Process: CONVENTIONS §4–6; full work-loop; solo approver. (source: `docs/CONVENTIONS.md`)
