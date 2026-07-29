# Plan: engram-mcp-core

- **Spec:** [`spec.md`](spec.md)
- **Status:** Drafting

> **Plan contract:** implementation strategy for Phase 1 (generic core) of RFC-0015. Allowed to change as we learn; substantial approach changes are noted in the Changelog.

## Approach

Build a new workspace-member binary crate `mcp/engram-mcp` that owns a thin JSON-RPC-over-stdio loop and a tool registry, bootstraps one `EngramProvider` (sqlite feature, embedding-`none` by default), and exposes the Phase-1 tool set in focused per-group modules. Order the work bottom-up: (1) crate skeleton + registry + loop, (2) provider bootstrap + scope, (3) the config layer (ontology/taxonomy), (4) the generic write/recall tools, (5) the `MarkdownChunker` in `adapters/ingest` + the `index_docs` tool, (6) the companion `engram-distill` agent skill that drives the server end-to-end. Riskiest part: the multi-layer ontology/taxonomy config → `OntologyRepository`/`TaxonomyRepository` mapping, and proving fused recall + scope isolation through one provider — both are integration-level, proved by tests against a real provider.

## Constraints

- RFC-0015 — the parent design (one server, fused-per-project, ontology-as-config, agent-side distillation, deprecation of the two interim servers in Phase 3).
- ADR-0022 — engine neutrality + surface parity: route through `EngramProvider`, no engine types in the MCP.
- ADR-0008 — `OntologyRepository` is durable; advisory validation only.
- ADR-0009 — retrieval-composition seam; `recall` reuses `UnifiedRecall` (do not reimplement fusion).
- ADR-0020 — entity-kind vocabulary extension point (not needed in Phase 1).
- ADR-0025 — framework/content boundary: ship mechanism, not domain ontology content (ontology/taxonomy are consumer-loaded config, not core content).
- `docs/architecture/reference.md` — LLM is TypeScript-only; embeddings behind the feature gate; typed `CoreError`; scope/policy on every path.

## Construction tests

**Integration tests (cross-cutting):**
- One `EngramProvider` opened by the server: `index_docs` a fixture `.md` → `store_knowledge` extracted concepts + cross-layer relationships → `recall` returns doc chunks + concepts + memories fused; restrict with `lanes`; assert an unrelated project workspace is invisible (owned by T7).
- `store_knowledge` partial-failure path surfaces `BatchStatus::Partial` / `BestEffort` (no ACID claim) — owned by T6.
- Zero-config bootstrap: launch with no `--ontology`/`--taxonomy` → server runs on the baked-in default; `ontology_read` returns it — owned by T4.

**Manual verification:**
- Run `engram-mcp` over stdio from a real MCP client config (Claude Code / Codex) and drive `engram-distill` against a sample doc; observe writes via `recall`. Captured at `mcp/engram-mcp/extensions/engram-distill/examples/recording.md` (owned by T11).

## Design (LLD)

### Design decisions

- **One provider, registry-dispatched tools** (not a god-`main.rs`) — traces to AC: "registry is single source of truth", "every tool routes through `EngramProvider`". Implements: in-crate tool surface (no `contracts/` file).
- **`recall` reuses `UnifiedRecall`; `lanes` is a request-construction / post-filter concern** at the MCP edge (no core change) — traces to AC: "`recall` fused + `lanes`".
- **`store_knowledge` → `BatchIngest`** (the only bulk write surface); `BestEffort` surfaced verbatim — traces to AC: "maps onto `BatchIngest`".
- **Ontology/taxonomy = MCP-side config layer** writing through `provider.require_ontology()` / `require_taxonomy()`; multi-layer = a `Vec<Layer>` where each layer is classes + predicates; cross-layer predicates are just allowed `KnowledgeRelationship` predicates (ADR-0025: content is consumer-loaded, not core) — traces to AC: "multi-layer ontology + taxonomy from launch config".
- **`MarkdownChunker` in `adapters/ingest`** implementing `Chunker`, emitting `DocumentSection` / `Paragraph` / `CodeBlock` with `SourceLocation` line spans — traces to AC: "`MarkdownChunker` implements `Chunker`".

### Component / module decomposition

- `mcp/engram-mcp/src/main.rs` — thin: parse argv → `config::McpConfig` → `server::run()`.
- `server.rs` — JSON-RPC 2.0 stdio loop (hand-rolled, mirroring the `codegraph/mcp-server` `Option<Value>` notification-skip pattern); dispatch via `registry::ToolRegistry`.
- `registry.rs` — `ToolRegistry` (name → schema + handler); single source of truth for `tools/list` and (Phase 3) `capability_report`.
- `bootstrap.rs` — build `EngramConfig` + `EngramProvider::open`; hold the `EngramProvider` for handlers.
- `config.rs` — `McpConfig` (storage path, project, scope policy, embedding provider, ontology/taxonomy file-or-inline); argv + env parsing.
- `scope.rs` — project → `Scope { workspace, tenant, ... }`; lane helpers.
- `ontology.rs` — multi-layer ontology/taxonomy config model + loader (writes through `require_ontology` / `require_taxonomy`); baked-in default.
- `tools/{scan,docs,knowledge,memory,recall,consolidate,config_tools,capability}.rs` — one module per group; Phase 1 fills `docs`, `knowledge`, `memory`, `recall`, `consolidate`, `config_tools`.
- `adapters/ingest/src/markdown_chunker.rs` — new `MarkdownChunker`.
- `mcp/engram-mcp/extensions/engram-distill/SKILL.md` — companion agent skill.

### Interfaces & contracts

- The MCP tool surface (JSON-RPC `tools/list` + `tools/call`) is the interface. Tool input schemas are emitted by the registry and live in-crate — no `contracts/<type>/` artifact (consistent with the two existing servers). Phase-1 tools: `index_docs`, `store_knowledge`, `put_entity`, `put_relationship`, `write_memory`, `forget`, `recall`, `consolidate`, `ontology_read`, `taxonomy_read`.

### Data & schema

- **Scope model:** `workspace = <project>` (from `--project` or the indexed path), `tenant = <host>`; one searchable space per project. No schema change — reuses `engram_domain::Scope`.
- **Ontology config (TOML):** `[[layer]] name + classes`; `[predicates] within / across`. Taxonomy config: per-layer broader/narrower + labels. Both map onto `OntologyClass` / `OntologyProperty` (ADR-0008) and taxonomy concepts via the provider handles.
- **Doc chunks:** `KnowledgeChunkKind::{DocumentSection, Paragraph, CodeBlock}` + `SourceLocation { path, start_line, end_line }`.

### Behavior & rules

- **Fused-per-project isolation:** every write carries the project `Scope`; recall is scoped to it; unrelated project workspaces never blend (conformance test in T7).
- **Zero-config default:** missing ontology/taxonomy config → baked-in minimal generic layers.
- **`store_knowledge` is `BestEffort`:** `BatchStatus::Partial` surfaced; no rollback of succeeded steps.
- **Policy + provenance on every write** (including agent-skill-driven `store_knowledge`).

### Failure, edge cases & resilience

- Malformed JSON-RPC line → skip (no crash); unknown method → JSON-RPC error; missing required tool arg → typed error object.
- Missing/invalid ontology config file → fall back to default + log (do not fail to boot).
- Unsupported capability (e.g. embeddings `none`) → recall degrades (lexical + graph + memory + beliefs; no vector lane); never throws on the read path (fail-closed to a safe baseline per `reference.md`).

### Quality attributes (NFRs)

- **Engine-neutrality:** the neutrality gate covers `mcp/engram-mcp/src` and passes; no `Sql*` / engine types in the MCP.
- **No-LLM-in-server:** grep-provable absence of model/embedding HTTP clients; embeddings opt-in via feature.
- **Modularity:** no god-module; crate root is a facade.

## Tasks

### T1: Crate skeleton, workspace member, JSON-RPC loop, tool registry

**Depends on:** none

**Tests (TDD):**
- Registry: register N tools → `tools/list` emits exactly those names + schemas; lookup-by-name returns the handler.
- Protocol: a well-formed `initialize` returns capabilities; a `tools/call` returns a result; a notification (no `id`) produces no response; a malformed line is skipped without panic.

**Approach:**
- Add `mcp/engram-mcp` to the workspace `members`; `Cargo.toml` (bin `engram-mcp`; deps: `engram-integration`/sqlite, `engram-domain`, `engram-runtime`, `serde`/`serde_json`, `futures`; add `engram-memory`/`engram-knowledge`/`engram-belief`/`engram-hierarchy` only if a handler names one of their concrete types directly — trait objects usually arrive via `engram-integration`). No `surreal` feature.
- `src/main.rs` (thin), `src/server.rs` (stdio JSON-RPC loop), `src/registry.rs` (`ToolRegistry`), `src/protocol.rs` (envelope + error objects). No tools wired yet beyond a stub.

**Done when:** `cargo build -p engram-mcp` succeeds; registry + protocol unit tests green.

### T2: Provider bootstrap + McpConfig (storage, project, scope, embedding-none)

**Depends on:** T1

**Tests (goal-based integration):**
- `McpConfig` parses argv (`--storage`, `--project`, `--ontology`, `--taxonomy`) and inline env; defaults applied.
- `bootstrap::open_provider` returns an `EngramProvider` with memory/knowledge/ontology/taxonomy/hierarchy/recall/batch/consolidation handles `Some` under the sqlite feature (`identity` is also wired by default but not exposed by Phase-1 tools).

**Approach:**
- `src/config.rs` (`McpConfig` + parsing), `src/bootstrap.rs` (`EngramConfig::new(...)` with `provider_type:"none"`, `MigrationMode::DryRun`, `CapabilityPolicy::FailClosed`; `EngramProvider::open`). Use `open`, not `bootstrap`.

**Done when:** server boots an `EngramProvider`; bootstrap test asserts the handles are present.

### T3: Scope resolution (project → workspace) + lane helpers

**Depends on:** T2

**Tests (TDD):**
- `--project foo` → `Scope.workspace == "foo"`; default tenant applied.
- Lane helper maps a `lanes` request to the recall selection.

**Approach:**
- `src/scope.rs`: resolve project → `Scope`; lane helpers for recall filtering.

**Done when:** scope-mapping unit test green.

### T4: Multi-layer ontology/taxonomy launch config + loader + read tools

**Depends on:** T2, T3

**Tests (goal-based integration):**
- Parse a multi-layer TOML (technical + business + domain) into the config model.
- Loader writes classes/predicates through `require_ontology()` / `require_taxonomy()`; `ontology_read` / `taxonomy_read` return them.
- Missing config → baked-in default loads; `ontology_read` returns the default (zero-config bootstrap).

**Approach:**
- `src/ontology.rs`: config model (`Vec<Layer>`, predicates), TOML/inline parser, baked-in default, loader (idempotent upserts via the provider handles), `ontology_read` / `taxonomy_read` tool handlers.

**Done when:** parse + load + readback integration test green; zero-config default test green.

### T5: Generic write tools — put_entity, put_relationship, write_memory, forget

**Depends on:** T2, T3

**Tests (goal-based integration):**
- `put_entity` honors the `kind` arg (no hard-coded `Concept`); write → readback through the provider.
- `put_relationship` writes an arbitrary predicate; `write_memory` writes a `MemoryRecord`; `forget` removes/tombstones.

**Approach:**
- `src/tools/knowledge.rs` (`put_entity`, `put_relationship`) and `src/tools/memory.rs` (`write_memory`, `forget`); each handler pulls the handle via `require_*()` and stamps `Scope` + `Policy` + `Provenance`.

**Done when:** write/readback tests green for all four.

### T6: store_knowledge → BatchIngest (facts + entities + relationships), BestEffort surfaced

**Depends on:** T5

**Tests (goal-based integration):**
- A `store_knowledge` call with facts + entities + relationships maps onto `BatchIngestRequest` and returns the `BatchOutcome`; a partial failure surfaces `BatchStatus::Partial` and the `BestEffort` guarantee; no ACID wording in the response.

**Approach:**
- `src/tools/knowledge.rs::store_knowledge`: build `BatchIngestRequest` (idempotency key + scope + slices), call `provider.require_batch().ingest(...)`, translate `BatchOutcome` to the MCP result.
- Force a partial deterministically with an empty-text `MemoryRecord` in the Facts slice (reuse the trigger pattern at `adapters/integration/tests/batch_ingest.rs:342` — Entities/Relationships still land).

**Done when:** bulk-write + partial-status test green.

### T7: recall (fused, lanes) via UnifiedRecall + scope isolation

**Depends on:** T5

**Tests (goal-based integration):**
- After writes, `recall` returns fused memory + knowledge + beliefs for the project scope; `lanes` restricts the result; embeddings-`none` degrades gracefully (no vector lane, no panic).
- Scope isolation (owns spec AC #5): records written under a different project `workspace` are invisible to `recall` in this project.

**Approach:**
- `src/tools/recall.rs`: build the recall request from `provider.require_recall()`; apply `lanes` as request construction where supported, else post-filter by source; fail-closed on the read path.
- Isolation test: write to `workspace=projectA` and `workspace=projectB`; `recall` scoped to A returns nothing from B.

**Done when:** fused-recall + lanes integration test green, and the unrelated-project scope-isolation test (AC #5) green.

### T8: consolidate via ConsolidationService

**Depends on:** T2

**Tests (goal-based integration):**
- `consolidate` runs reflection + decay through `provider.require_consolidation()` and returns a summary (beliefs synthesized / expired).

**Approach:**
- `src/tools/consolidate.rs`: call the consolidation handle; translate the result.

**Done when:** consolidate integration test green.

### T9: MarkdownChunker in adapters/ingest

**Depends on:** none

**Tests (TDD):**
- Splits a Markdown sample by ATX headers into `DocumentSection` chunks with correct `start_line`/`end_line`; fenced code → `CodeBlock`; YAML front-matter skipped/preserved; paragraphs → `Paragraph`; empty input → no chunks (no panic).

**Approach:**
- `adapters/ingest/src/markdown_chunker.rs`: implement `Chunker` (`fn chunk(&self, &str) -> CoreResult<Vec<ChunkCandidate>>`); line-scan headers (`^#{1,6}\s`), fenced blocks (```` ``` ````), front-matter (`---`); export from `adapters/ingest/src/lib.rs`.

**Done when:** MarkdownChunker unit tests green; `cargo test -p engram-ingest` passes.

### T10: index_docs tool wiring MarkdownChunker → knowledge store

**Depends on:** T7, T9

**Tests (goal-based integration):**
- `index_docs` on a fixture `.md` persists chunks (with doc path + section provenance) retrievable via `recall`.

**Approach:**
- `src/tools/docs.rs::index_docs`: read path/text, run `MarkdownChunker`, persist chunks through the knowledge handle with `Scope` + provenance; return counts.

**Done when:** index_docs → recall integration test green.

### T11: Companion agent skill — engram-distill

**Depends on:** T6, T10

**Tests (manual / E2E):**
- The skill, against a running server, extracts entities/relationships/facts from a sample Markdown doc and writes them via `store_knowledge` / `index_docs`; `recall` returns them.

**Approach:**
- `mcp/engram-mcp/extensions/engram-distill/SKILL.md`: a `SKILL.md` describing when to invoke (distilling docs/transcripts into the KG), the extraction contract (entities with ontology class, relationships with configured predicates, facts), and the MCP calls (`index_docs`, `store_knowledge`, `put_entity`/`put_relationship`, `recall` to verify). Classify concepts against the configured layers. No server-side LLM — extraction is the agent's reasoning.
- Capture the verifying run at `mcp/engram-mcp/extensions/engram-distill/examples/recording.md` (sample doc input, MCP calls, `recall` output, date).

**Done when:** an E2E run captured at `mcp/engram-mcp/extensions/engram-distill/examples/recording.md` writes recoverable knowledge; skill file present and referenced.

### T12: Docs + gates

**Depends on:** T1–T11

**Tests (goal-based):**
- `cargo fmt --all`, `cargo check --workspace`, `cargo test -p engram-mcp -p engram-ingest`, `.codex/hooks/check-engine-neutrality.sh`, `.codex/hooks/check-docs.sh` all pass; grep proves no LLM/embedding HTTP client in `mcp/engram-mcp`.

**Approach:**
- Add `mcp/engram-mcp/src` to the default `GATED_PATHS` in `.codex/hooks/check-engine-neutrality.sh` so the gate genuinely covers the new crate (then running it with no args proves neutrality, satisfying AC #2); run the hook.
- Note `mcp/engram-mcp` in `AGENTS.md` repo-shape (additive; the two interim servers stay until Phase 3) and **correct the stale `OntologyRepository is deferred (taxonomy only)` line under the `sqlite-knowledge-graph` invariant** (ADR-0008 supersedes it); update `docs/specs/README.md`; add a short `mcp/engram-mcp/README.md` (launch config + tool list).

**Done when:** all gates green; docs updated.

## Rollout

- **Delivery:** additive — a new server binary; the two interim servers are untouched in Phase 1 (deprecation is Phase 3). No flag, no migration, fully reversible (delete the crate).
- **Infrastructure:** none beyond a SQLite file (WAL) at the configured storage path.
- **External-system integration:** none (no LLM, no embedding service by default).
- **Deployment sequencing:** `MarkdownChunker` (T9) can land independently of the server; the server tasks are ordered T1→T2→…→T10; the skill (T11) lands last.

## Risks

- **`recall` `lanes` may not be a first-class `UnifiedRecall` toggle.** Mitigation: implement `lanes` as request construction where the API allows, else a source post-filter at the MCP edge (no core change).
- **Multi-layer ontology → `OntologyRepository` mapping surface area.** Mitigation: keep the config model minimal (layers = classes + predicates), use advisory validation only (ADR-0008), and prove round-trip with a readback test.
- **Embeddings-`none` makes recall vector-less.** Mitigation: acceptable for Phase 1 (lexical + graph + memory + beliefs still fuse); FastEmbed is a config/feature flip, not a redesign.
- **Agent skill quality is subjective.** Mitigation: gate on an observable E2E write captured to a named recording artifact, not on prose quality; iterate the skill in work-loop.

## Changelog

- 2026-07-28: initial Phase-1 plan (spec `engram-mcp-core`), derived from RFC-0015; agent skill folded into scope per user direction (lives in `mcp/engram-mcp/extensions/`).
- 2026-07-28: adversarial-review fixes — added ADR-0025 to Constraints; per-task verification-mode labels; T1 dep-pruning note; T2 `identity`-wired note; T6 partial-trigger cite (`adapters/integration/tests/batch_ingest.rs:342`); T7 owns the scope-isolation test (AC #5); T11 names its recording artifact; T12 extends the neutrality hook's `GATED_PATHS` to cover `mcp/engram-mcp/src` and corrects the stale AGENTS.md `OntologyRepository` line.
