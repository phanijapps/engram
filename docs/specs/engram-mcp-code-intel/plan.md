# Plan: engram-mcp-code-intel

- **Spec:** [`spec.md`](spec.md)
- **Status:** Drafting

> **Plan contract:** implementation strategy for Phase 2 (code intelligence) of RFC-0015. Built on Phase 1 (`engram-mcp-core`). Allowed to change as we learn; substantial approach changes are noted in the Changelog.

## Approach

Add code intelligence to `engram-mcp` in three layers: (1) **new `engram-integration` exposure** that closes the three gaps blocking provider-routed code-intel — a `KnowledgeQuery` port (list entities/relationships), a lexical-feed surface, and a fan-in adapter combining the knowledge + graph handles for `scan_repository`; (2) **`scan_repo` + `search`** tools that route through the provider; (3) the **six consolidated composites** that fetch the edge/entity list via `KnowledgeQuery` and compose the pure `engram-codegraph-*` math. Riskiest part: the new `engram-integration` ports + their backing (E1/E2) — these are the substrate change that makes "no provider bypass" true; proved by integration tests under `cargo test -p engram-integration`.

## Constraints

- RFC-0015 — parent design; Phase 2 = scan_repo + six composites + search.
- ADR-0022 — engine neutrality + surface parity: route through `EngramProvider`; no engine types in the MCP. (N-API parity for the new handles is deferred — see T9/backlog.)
- User rule — new exposure via `engram-integration`. **No core `engram-knowledge` port-trait change in Phase 2.**
- Phase 1 (`engram-mcp-core`) — the `App`, registry, tools module, and helpers are reused.

## Construction tests

**Integration (cross-cutting):**
- Open a provider (`cargo test -p engram-integration`); the new `KnowledgeQuery` + lexical-feed handles are `Some`; `list_entities`/`list_relationships` round-trip after a `put_entity`; lexical feed → `search` returns the fed symbol.
- `scan_repo` a fixture repo → `recall`/`search` return its symbols fused with an indexed doc **and** a written concept (`put_entity` kind `concept`); scope isolation holds.
- Each composite returns a structured answer for a fixture edge/entity set.

**Manual:** drive `scan_repo` + a composite + `search` over stdio against the built binary on a tiny repo (Phase-2 recording, mirroring Phase 1's `engram-distill/examples/recording.md`).

## Design (LLD)

### Design decisions

- **New ports live in `engram-integration`, not core trait changes.** `KnowledgeQuery` (`list_entities`/`list_relationships`) and `LexicalFeed` (upsert into the in-RAM Tantivy lane) are new traits in `engram-integration`, backed by the SQLite knowledge store / the bootstrap lexical index, exposed as new `EngramProvider` handles. This follows the user's "expose via engram-integration" rule and avoids a core `engram-knowledge` port-trait change. Traces to AC: "`KnowledgeQuery` exposure in `engram-integration`".
- **Fan-in adapter for `scan_repository`** — `KnowledgeRepoGraph { knowledge, graph }` implementing both `KnowledgeRepository` + `KnowledgeGraphRepository` by delegation, at the MCP edge. Avoids changing `scan_repository`'s single-`R` signature. Traces to AC: "`scan_repo` routed through the provider".
- **Composites are thin composition** over `engram-codegraph-queries`/`temporal` pure fns; the MCP fetches `Vec<KnowledgeRelationship>`/`Vec<KnowledgeEntity>` via `KnowledgeQuery` then calls the math. Traces to AC: "six consolidated tools".
- **`search` uses the lexical lane** fed by `scan_repo` through `LexicalFeed`, reached via `recall` (keyword mode) — no direct `LexicalIndex` in the MCP. Traces to AC: "`search` routes through the provider".

### Component / module decomposition

- `core/integration/src/knowledge_query.rs` — `KnowledgeQuery` port; `core/integration/src/sqlite/knowledge_query.rs` — SQLite-backed impl (delegating to `SqlKnowledgeStore`'s inherent `list_*`).
- `core/integration/src/lexical_feed.rs` — `LexicalFeed` port; `core/integration/src/sqlite/lexical_feed.rs` — impl over the in-RAM Tantivy `LexicalIndex` from `bootstrap_sqlite`.
- Provider fields + accessors + builders + `bootstrap_sqlite` wiring for both.
- `mcp/engram-mcp/src/codegraph.rs` — fan-in adapter + `scan_repo`/`search` handlers + the six composite handlers.

### Interfaces & contracts

- New provider handles: `provider.knowledge_query()`, `provider.lexical_feed()`.
- New MCP tools (Phase 2): `scan_repo`, `search`, `symbol_context`, `change_impact`, `code_health`, `architecture`, `api_topology`, `whats_changed`. Tool schemas in-crate.

### Behavior & rules

- `scan_repo` stamps the project `Scope` on every code entity/relationship (fused-per-project); feeds code-symbol names to the lexical lane via `LexicalFeed`.
- Composites read the project-scoped edge/entity list; a missing/empty graph returns an empty structured answer (not an error).
- `whats_changed` builds `VersionedSymbol` from `KnowledgeEntity.valid_from/until` + in/out degree; **`directional` is deferred** (needs per-project scan-baseline retention, not in Phase 2).

### Failure, edge cases & resilience

- Empty/unscanned repo → composites return empty results; `search` returns "No results."
- The lexical feed is in-RAM (rebuilt per process); a restart requires a re-scan to re-feed. Acceptable for Phase 2; documented.
- Missing `KnowledgeQuery`/`LexicalFeed` handle (capability unsupported) → typed error.

### Quality attributes (NFRs)

- **No provider bypass:** grep proves no `Sql*`/`LexicalIndex` in `mcp/engram-mcp/src`; neutrality gate passes for the crate + the two new integration files.
- **Reuse, not reimplementation:** composites call `engram-codegraph-*`; no duplicated graph math.

## Tasks

### E1: KnowledgeQuery exposure (list entities/relationships)

**Depends on:** none

**Tests (goal-based integration, `-p engram-integration`):**
- `KnowledgeQuery` port; SQLite impl delegates to `SqlKnowledgeStore::list_entities`/`list_relationships`; provider handle is `Some` after bootstrap; round-trips after `put_entity`/`put_relationship`.

**Approach:**
- `core/integration/src/knowledge_query.rs` (port + provider field/accessor/builder/`require_knowledge_query()`); `core/integration/src/sqlite/knowledge_query.rs` (impl delegating to the store's inherent `list_*`); wire in `bootstrap_sqlite`.

**Done when:** handle wired + round-trip test green.

### E2: Lexical-feed exposure

**Depends on:** none

**Tests (goal-based integration, `-p engram-integration`):**
- `LexicalFeed` port (`upsert_symbols`); backed by the in-RAM Tantivy `LexicalIndex`; after feeding, a keyword `recall` returns the fed symbol; handle `Some` after bootstrap.

**Approach:**
- `core/integration/src/lexical_feed.rs` (port + handle + builder); `core/integration/src/sqlite/lexical_feed.rs` (impl over the bootstrap lexical index, exposing an `upsert` path — the unified-recall lane is currently unfed); wire in `bootstrap_sqlite`.

**Done when:** feed → search round-trip green.

### T1: scan_repo tool (fan-in adapter + scan_repository + lexical feed)

**Depends on:** E1, E2

**Tests (goal-based integration):**
- `scan_repo` on a fixture repo lands code entities/relationships in the project scope (recallable); feeds the lexical lane; no `SqlKnowledgeStore` in the MCP.

**Approach:**
- `mcp/engram-mcp/src/codegraph.rs`: `KnowledgeRepoGraph { knowledge, graph }` implementing both traits by delegation; `scan_repo` handler builds `ScanOptions` (project scope), calls `scan_repository(path, opts, &adapter, |_| ())`, then feeds code-symbol names via `lexical_feed`.

**Done when:** scan → recall integration green; no engine types in the MCP.

### T2: search tool (keyword, provider-routed)

**Depends on:** E2, T1

**Tests (goal-based integration):**
- After `scan_repo`, `search("Foo")` returns code symbols; routes through the provider (no direct `LexicalIndex`).

**Approach:**
- `codegraph.rs::search`: keyword `RetrievalRequest` through `provider.recall()`, lane-filtered to entities (the lexical lane is fed by T1).

**Done when:** search returns scanned symbols.

### T3: symbol_context composite

**Depends on:** E1

**Tests (TDD — hardcoded expected JSON):**
- For a fixture edge set, the JSON contains the expected callers/callees/community (assert on fields, not equivalence to `cgq::symbol_context`).

**Approach:** `codegraph.rs::symbol_context` — fetch edges via `knowledge_query`, call `cgq::symbol_context`, shape JSON.

**Done when:** fixture test green.

### T4: change_impact composite

**Depends on:** E1

**Tests (TDD — hardcoded expected JSON):** fixture edges → JSON contains the expected blast-radius + path members.

**Approach:** `cgq::blast_radius` + `dependency_path` + `process_flow` composed into one structured answer.

**Done when:** fixture test green.

### T5: code_health composite

**Depends on:** E1

**Tests (TDD — hardcoded expected JSON):** fixture → JSON contains expected dead-code + complexity entries.

**Approach:** `cgq::dead_code` + `cyclomatic_complexity`/`most_complex` (over fetched entity source where available).

**Done when:** fixture test green.

### T6: architecture composite

**Depends on:** E1

**Tests (TDD — hardcoded expected JSON):** fixture → JSON contains expected central/bridge symbols + community count + stats.

**Approach:** `cgq::central_symbols` + `bridge_symbols` + `call_communities` + `repository_stats`.

**Done when:** fixture test green.

### T7: api_topology composite

**Depends on:** E1

**Tests (TDD — hardcoded expected JSON):** fixture source → JSON contains expected endpoints + call-site matches.

**Approach:** `cgq::find_endpoints` + `find_api_calls` + `match_api_topology` over fetched source text.

**Done when:** fixture test green.

### T8: whats_changed composite (temporal; directional deferred)

**Depends on:** E1

**Tests (TDD — hardcoded expected JSON):** fixture `VersionedSymbol` set → JSON contains expected recent/impact/compound rankings + overview stats.

**Approach:** build `VersionedSymbol` from `KnowledgeEntity.valid_from/until` + in/out degree (mirror old MCP `build_versions`); call `cgt::{recent, impact, compound, overview}`. **`directional` is NOT composed** (needs per-project scan-baseline retention — deferred; see Changelog).

**Done when:** fixture test green.

### T9: Docs + gates

**Depends on:** E1, E2, T1–T8

**Tests (goal-based):**
- `cargo fmt --all`, `cargo check --workspace`, `cargo test -p engram-mcp -p engram-integration`, `.codex/hooks/check-engine-neutrality.sh`, `check-docs.sh`, `check-contracts.sh`.
- Extend `GATED_PATHS` to include the two new files (`core/integration/src/knowledge_query.rs`, `lexical_feed.rs`).
- Assert `mcp/engram-mcp/Cargo.toml` pins `engram-integration` to `features = ["sqlite"]` (no `surreal`).
- Grep: no `Sql*`/`LexicalIndex` in `mcp/engram-mcp/src`.
- Add a `docs/backlog.md` entry `phase-2-napi-parity` (N-API exposure for the new handles) + `phase-2-directional-temporal` (scan-baseline retention for `whats_changed` directional mode).

**Approach:** update `mcp/engram-mcp/README.md` (Phase-2 tools) + `docs/specs/README.md`; add a Phase-2 recording; create/extend `docs/backlog.md` with the two deferral anchors.

**Done when:** all gates green; backlog entries present.

## Rollout

- **Delivery:** additive — new tools + new `engram-integration` ports; the two interim servers stay until Phase 3. No data migration.
- **Sequencing:** E1/E2 (substrate) unblock T1/T2 and the composites (T3–T8); composites are independent of each other once E1 lands.

## Risks

- **`KnowledgeQuery`/`LexicalFeed` ports may surface capability-policy nuance** (FailClosed). Mitigation: mirror existing handle patterns; prove `Some` under sqlite in E1/E2 tests.
- **Fan-in adapter must satisfy both traits fully** (every method `scan_repository` calls). Mitigation: delegate every trait method to the matching handle; test against `scan_repository`.
- **`whats_changed` VersionedSymbol construction** depends on `valid_from/until` being populated by ingest. Mitigation: mirror the old MCP's `build_versions`; degrade gracefully when timestamps are absent.
- **Lexical feed is in-RAM** (lost on restart). Mitigation: documented; re-scan re-feeds; a durable feed is a follow-up.

## Changelog

- 2026-07-29: initial Phase-2 plan; architecture = new `engram-integration` exposure (SQLite/Tantivy-backed) per the user's rule, following the contract-grounding finding (scan_repository needs a combined handle; composites need list methods; search needs a lexical feed — none provider-exposed today).
- 2026-07-29: spec-review fixes — AC8/test scope adds `-p engram-integration`; `whats_changed` drops `directional` (needs scan-baseline; deferred as `phase-2-directional-temporal`); T8 split into its own task; lexical-feed corrected to in-RAM Tantivy (not SQLite); neutrality gate extended to the two new integration files; Cargo.toml `surreal`-feature pin check added; composite TDD tests pinned to hardcoded expected JSON (not production-fn equivalence); N-API parity for new handles deferred (`phase-2-napi-parity`); SQLite impls located in `core/integration/src/sqlite/`; design decisions moved out of the spec Assumptions into plan Design decisions.
