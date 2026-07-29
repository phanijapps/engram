# Spec: engram-mcp-core

- **Status:** Implementing
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** RFC-0015, ADR-0008, ADR-0009, ADR-0022, ADR-0020, ADR-0025
- **Brief:** none
- **Contract:** none — MCP tool input schemas live in-crate (consistent with the existing `codegraph/mcp-server` and `memory/mcp-server`); no `contracts/<type>/` artifact.
- **Shape:** service — an MCP server exposing a JSON-RPC tool surface over stdio.

> **Spec contract:** this document defines what "done" means for Phase 1 (the generic core) of RFC-0015. The implementing PR must match this spec, or update it. Verification must be derivable from it.

## Objective

The `engram-mcp` server's generic core gives an AI agent a single MCP connection to write and recall generic memory plus a multi-layer knowledge graph over one SQLite-backed `EngramProvider`, with Markdown doc ingestion and a configurable multi-layer ontology/taxonomy loaded at launch. A companion agent skill (`mcp/engram-mcp/extensions/engram-distill`) extracts entities, relationships, and facts from docs and free text and writes them through the server. Success: an agent indexes a project's Markdown docs, writes extracted concepts against a technical + business + domain ontology supplied as MCP launch config, and recalls a fused result spanning docs + concepts + memories — with the server never calling an LLM and every capability reached through `engram-integration`.

Code ingestion (`scan_repo` / treesitter), the consolidated code-intelligence tools, and `get_context` are Phase 2/3 (deferred; tracked under RFC-0015).

## Boundaries

### Always do

- Route every tool through one `EngramProvider::open` (`engram-integration`); never reach into adapter internals.
- Keep the crate root a facade — module declarations + a thin `main`; behavior lives in focused modules (registry, bootstrap, scope, config, one module per tool group).
- Use typed `CoreError` / `CoreResult` end to end; translate to JSON-RPC error objects only at the protocol edge.
- Stamp `Scope` + `Policy` + `Provenance` on every write path.
- Fall back to a baked-in minimal generic ontology + taxonomy so the server runs zero-config.

### Ask first

- Add any new `EntityKind` / `KnowledgeChunkKind` variant to `engram-domain` (Phase 1 targets none; if unavoidable, gate by ADR-0020 and ADR-0025 — the framework/content boundary).
- Change the `EngramConfig` or any `EngramProvider` handle signature.
- Enable the `fastembed` feature by default (it stays opt-in).

### Never do

- **Do not call an LLM or an embedding HTTP client from the server** — `reference.md` pins the LLM to TypeScript-only and embeddings behind the feature gate.
- **Do not bypass `EngramProvider`** — no direct `SqlKnowledgeStore::open*` in the MCP (engine-neutrality + surface parity, ADR-0022).
- **Do not wire or depend on the SurrealDB backend** (`adapters/surreal`) — out of scope for v1 (RFC-0015 Q5).
- **Do not change `engram-domain` contracts in Phase 1** — reuse existing `KnowledgeChunkKind::{DocumentSection, Paragraph, CodeBlock}`, `EntityKind`, `Scope`, `Policy`, `Provenance`.
- **Do not claim cross-store ACID for `store_knowledge`** — it is `BestEffort`, surfaced (`atomic-batch-ingest` invariant).
- **Do not create a god-`main.rs`** — the registry + per-group modules own behavior; `tools/list` and `capability_report` read from the registry.

## Testing Strategy

- **Tool dispatch + JSON-RPC framing:** TDD — unit tests over the tool registry (registration, lookup, schema emission) and the protocol envelope (request/response, notification skip, error objects).
- **Scope resolution, ontology/taxonomy config parsing, MarkdownChunker splitting:** TDD — pure functions / compact invariants (project→workspace mapping; multi-layer TOML parse; ATX-header splits, line-span provenance, fenced-code + front-matter handling).
- **Fused recall, scope isolation, `store_knowledge`→`BatchIngest`:** goal-based, exercised by integration tests against one real `EngramProvider` (write docs + concepts + memories; recall fused; assert an unrelated-project workspace never blends; assert `BestEffort`/`Partial` status is surfaced).
- **Agent skill driving the server end-to-end:** manual / E2E — a recorded run of `engram-distill` against a running server on a sample Markdown doc, with observable writes.
- **Engine-neutrality + no-LLM invariants:** goal-based — `.codex/hooks/check-engine-neutrality.sh` passes; a grep proves no model/embedding HTTP client in `mcp/engram-mcp`.

## Acceptance Criteria

- [ ] `mcp/engram-mcp` is a workspace-member binary (`engram-mcp`); `cargo build -p engram-mcp` succeeds and the binary answers `initialize` + `tools/list` over stdio JSON-RPC.
- [ ] Every tool routes through one `EngramProvider::open`; the engine-neutrality gate covers `mcp/engram-mcp/src` and passes (no `Sql*`/engine-crate imports, no raw SQL); the `surreal` feature is not enabled.
- [ ] The tool registry is the single source of truth for `tools/list`; the Phase-1 tool set is exposed: `index_docs`, `store_knowledge`, `put_entity`, `put_relationship`, `write_memory`, `forget`, `recall`, `consolidate`, `ontology_read`, `taxonomy_read`.
- [ ] `recall` returns fused results across memory + knowledge + beliefs within one project workspace and honors a `lanes` parameter that restricts the result to a subset.
- [ ] Scope is fused-per-project: writes under `workspace = <project>` are visible to recall in the same project; an isolation test proves records from an unrelated project workspace never blend.
- [ ] Multi-layer ontology + taxonomy load from `--ontology` / `--taxonomy` launch config (file path or inline env); a missing config falls back to the baked-in minimal generic default; `ontology_read` / `taxonomy_read` return the active layers, classes, and predicates.
- [ ] `MarkdownChunker` in `adapters/ingest` implements `Chunker`, splits by ATX headers/sections with line-span `SourceLocation` provenance, and handles fenced code + front-matter; `index_docs` persists chunks retrievable via `recall`.
- [ ] `store_knowledge` maps onto `BatchIngest` and surfaces `BestEffort` / `Partial` status without claiming ACID.
- [ ] No LLM or embedding HTTP client is called from the server (grep-provable); embeddings default to `provider_type: "none"`, FastEmbed behind the compile-time feature.
- [ ] A companion agent skill exists at `mcp/engram-mcp/extensions/engram-distill/SKILL.md` and, against a running server, extracts entities/relationships/facts from a sample Markdown doc and writes them via `store_knowledge` / `index_docs`.
- [ ] `cargo fmt --all`, `cargo check --workspace`, and `cargo test -p engram-mcp -p engram-ingest` pass.

Deferred to Phase 2: `scan_repo` (treesitter code ingestion) + the six consolidated code-intelligence composites + `search`. Deferred to Phase 3: `get_context` (RFC-0013 `ContextSubgraph`) + unified `capability_report` + deprecation of `codegraph/mcp-server` and `memory/mcp-server`.

## Assumptions

- Technical: `EngramProvider` exposes `ontology()`, `taxonomy()`, and `hierarchy()` handles (plus memory/knowledge/graph/beliefs/retrieval/vectors/recall/batch/consolidation/identity/…), all `Option<&Arc<dyn T>>` with `require_*()` variants — ontology-as-config is reachable through the facade. (source: `core/integration/src/provider.rs:212-304`)
- Technical: bootstrap uses `EngramProvider::open(&EngramConfig)` (not `EngramProvider::bootstrap`, which is validation-only); `EngramConfig::new` has 6 positional params; `sqlite_storage_layout` defaults to `MultiFileDirectory`, overridable via `.with_sqlite_storage_layout()`. (source: `core/integration/src/config.rs:186-218`, `provider.rs:178-204`)
- Technical: `bootstrap_sqlite` wires ontology/taxonomy/hierarchy; `RetrievalIndex` is not wired by design but `UnifiedRecall` is — Phase-1 `recall` uses `provider.recall()`. (source: `core/integration/src/sqlite/bootstrap.rs`)
- Technical: bulk write maps onto `BatchIngest` (`provider.batch()`, cap `atomic_batch`); guarantee is `BestEffort`, not ACID. (source: `core/integration/src/batch.rs:33-47,214-220`)
- Technical: no `MarkdownChunker` exists; a new one implements `Chunker` (`fn chunk(&self, &str) -> CoreResult<Vec<ChunkCandidate>>`); `KnowledgeChunkKind::{DocumentSection, Paragraph, CodeBlock}` already exist → no `engram-domain` contract touch. (source: `adapters/ingest/src/chunker.rs`, `core/domain/src/knowledge.rs:114-125`)
- Technical: the JSON-RPC stdio loop is hand-rolled in both existing servers; no shared helper — the new crate owns its own loop + registry. (source: `memory/mcp-server` + `codegraph/mcp-server` `main.rs`)
- Technical: `mcp/` does not exist; MCP servers are workspace-member binaries; the SQLite adapter lives at `adapters/sqlite` (`adapters/surreal` present, out of scope). (source: root `Cargo.toml`)
- Technical (stack): Rust edition 2024, SQLite WAL + sqlite-vec, FastEmbed feature-gated/opt-in, no LLM in Rust, typed `CoreError`/`CoreResult`, scope/tenant isolation + policy on every path. (source: `docs/architecture/reference.md`)
- Process: spec lifecycle `Draft→Implementing→Shipped`; TDD/goal-based/E2E modes; risk triggers fire (structural/public-interface, new dependency, multi-feature) → full work-loop; solo project, `phanijapps` is author + approver. (source: `docs/CONVENTIONS.md §4-6`, RFC-0013 precedent)
- Product: the primary caller is an agent skill (`engram-distill`) consumed by Claude Code / Codex / Cursor over stdio MCP; the skill ships with the server under `mcp/engram-mcp/extensions/`. (source: user confirmation 2026-07-28)
- Product: Phase scope is the generic core only; `scan_repo`/code-intel (Phase 2) and `get_context`/deprecation (Phase 3) are deferred. (source: user confirmation 2026-07-28)
