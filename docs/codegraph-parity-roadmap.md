# Codegraph Parity Roadmap (build memtrace-equivalent capability on top of engram)

Governing proposal: `docs/rfcs/0012-code-structural-graph-layer.md`.
Research basis: `docs/research/memtrace-survey.md`.
Reconciled against: `docs/implementation-roadmap.md` (phases 0–22, 52–64) and
`docs/specs/` status as of 2026-07-08.

> **What changed from RFC 0012.** RFC 0012 assumed more greenfield than reality.
> Engram has already shipped most of the structural substrate (AST symbols,
> code-symbol + document graph extractor, hybrid fusion, sqlite-vec + FastEmbed,
> hierarchy, temporal+cue retrieval, belief + bi-temporal, SQLite knowledge
> graph, repo-scale ingestion, a 3D graph UI, LLM relationship extraction, Q&A).
> This roadmap therefore **maps each memtrace capability to current state and
> scopes only the genuine gaps as micro-specs** — no duplication of shipped work.

## Principles

- **Micro.** Each item is one PR, one context window, independently shippable.
- **Independent + verifiable.** Each item states `Depends on:` and a concrete
  acceptance check a test/audit can pass or fail.
- **Base = a reusable knowledge layer beyond coding.** Base-layer items
  (`B*`) are framed coding-agnostically (lexical retrieval over any chunk, graph
  analytics over any `KnowledgeRelationship`, bi-temporal any entity). Code-only
  intelligence lives in `C*`. Consumer surfaces in `D*`.
- **One at a time.** Pick the first not-done, not-blocked item; run it through
  `new-spec` → `work-loop`; only then start the next.
- **Spike before scope.** The first item (A1) confirms borderline states before
  the build items commit to a shape.

## Capability → current state

Legend: ✅ shipped · 🟡 partial · ❌ gap (work item below).

| Memtrace capability | State | Evidence / work item |
|---|---|---|
| AST symbol extraction + code-symbol chunks | ✅ | `ast-symbol-extraction`, `code-symbol-chunker` (Phase 17) |
| Code-symbol + document **graph extractor** (entities + CALLS/etc edges) | 🟡 | `knowledge-graph-extractor` (Phase 54) — intra-file edges ship; **cross-file/cross-lang resolution = C1/C2** |
| Knowledge graph SQLite persistence + retraction | ✅ | `sqlite-knowledge-graph`, `knowledge-graph-retraction` |
| Repo-scale + incremental ingestion | ✅ | `scale-repo-ingestion`, `background-repo-indexer`, `structured-repo-identity` |
| Hybrid retrieval **fusion** (weighted) | 🟡 | `hybrid-retrieval-fusion` (Phase 18) — weighted only; **cross-encoder rerank = B2** |
| Vector retrieval (sqlite-vec + FastEmbed BGE-small) | ✅ | `vector-retrieval-candidates`, `fastembed-*` (Phase 22) |
| **Lexical BM25/Tantivy** leg | ❌ | only RRF present; **B1** |
| **Graph analytics** (PageRank / betweenness / Louvain) | ❌ | grep-confirmed absent; **B3–B5** |
| Hierarchy (nav, aggregate, cluster-build) | 🟡 | `durable-hierarchy`, `hierarchy-navigation` — build/nav ship; **graph-metric algorithms = B3–B5** |
| **Temporal scoring modes** (recent/impact/novel/directional/compound/overview) | ❌ | `temporal-cue-retrieval` is temporal+cue retrieval, not the 6 modes; **C6** |
| Bi-temporal belief + contradiction | ✅ | `belief-contradiction-bitemporal` (entity-level `as_of` still **B6**) |
| **HTTP API topology** (framework endpoint/call-site scanners) | ❌ | grep-confirmed absent; **C7–C8** |
| **Complexity / dead-code / blast-radius / dependency-path** | ❌ | grep-confirmed absent; **C3–C5** |
| 3D graph UI (demo) | 🟡 | `enterprise-3d-graph` ships; **node CODE/INFO/HISTORY + timeline + insights = D5** |
| Q&A over knowledge | ✅ | `qa-over-knowledge` |
| **MCP server** (agent surface, ~38 tools) | ❌ | `mcp-server` Draft only (proposes TS-over-demo, 4 tools); **D3–D4** |
| **Agent skills** (~40) | ❌ | none; **D6** |
| **Fleet coordination** | ❌ | none; maps onto consolidation/contradiction; **D7** |
| **CapabilityReport** for consumers | 🟡 | model accepted in domain; not exposed via MCP; **D8** |

## Open decisions (gate later items; resolve when reached)

| # | Decision | Recommended default | Gates |
|---|---|---|---|
| Q1 | Repo strategy: new layer in this monorepo vs separate repo | Monorepo new layer | D1, D2 |
| Q2 | MCP host: Rust crate vs TS-over-`@engram/node` | TS over existing backend (matches the `mcp-server` Draft) | D3 |
| Q3 | Embedding provider for code: jina-code vs bge-code vs keep BGE-small | Add jina-code as a provider option | B7 |
| Q4 | v1 language breadth | Rust + TS + Python + priority HTTP frameworks | C2, C7, D6 |
| Q5 | Fleet in v1 or defer | Defer (weakest-grounded) | D7 |
| — | Target consumers | Claude Code + Cursor + Codex | D6 |

---

## Micro-specs (chronological)

Each item: `layer · depends · gates · objective · acceptance check`. Status is
`draft` until that item is opened through `new-spec`. Full spec bodies
(Objective / Boundaries / Testing Strategy / Acceptance Criteria) are elaborated
one-at-a-time when the item starts — this file is the sequenced catalog.

### Phase A — Audit & vocabulary

- **A1 — Capability audit (spike)** · base · depends: none · **gates: precise scope of B/C** · **✅ DONE 2026-07-08**
  - Objective: Confirm exact implementation state of borderline capabilities
    (BM25 depth, graph analytics, temporal modes, topology, MCP, code embeddings,
    cross-file edge resolution) by reading code, not guessing.
  - Acceptance: `docs/research/codegraph-parity-audit.md` exists; every row
    cites a code path or spec; each "partial" names what's missing.
  - Result: confirmed B1/B2/B3–B5/B6/B7/B8/C3–C8/D3–D8 as gaps; **re-scoped C1
    smaller** (name-based cross-file resolution already exists in `extractor.rs`);
    confirmed temporal *retrieval* is shipped (only the 6 *scoring modes* = C6).

- **A2 — Additive domain vocabulary** · base · **SHIPPED 2026-07-08 (ADR-0020)**
  - Objective: Add `EntityKind` values (`struct`, `interface`, `trait`, `enum`,
    `type_alias`, `endpoint`) and edge predicates (`overrides`, `annotated_with`)
    as compatible, freeze-safe enum additions.
  - Status: ADR-0020 + 6 `EntityKind` variants added (Struct, Interface, Trait,
    TypeAlias, Enum, Endpoint; snake_case). Draft-extension enum (not in frozen
    v1 schema) → no v1 schema regen; `KnowledgeRelationship.predicate` is already
    a free-form String so edge predicates need no enum change. Ripple-free (the
    only `EntityKind` match uses a wildcard); snake_case round-trip test green;
    `cargo check --workspace` + `check-contracts.sh` pass. Follow-up: wire the
    AST extractor to emit the new kinds.

### Phase B — Base fills (generic, reusable beyond coding)

- **B1 — Lexical retrieval leg (BM25/Tantivy)** · base · depends: none · **adapter SHIPPED 2026-07-08; wiring → `lexical-wiring` spec**
  - Objective: Implement `RetrievalMode::keyword` as a ranked BM25 full-text
    index over any `KnowledgeChunk`, behind the retrieval port.
  - Status: `engram-store-lexical` crate shipped (LexicalIndex + LexicalRetrievalIndex
    + LexicalTargetResolver + identifier normalizer; 10 tests, fmt/clippy clean).
    Live-pipeline composition (RRF), store-backed resolver, eval, and full
    workspace gates split to `docs/specs/lexical-wiring` (the composition layer is
    bindings-layer RRF fusion, not the unused `RetrievalRouter`).

- **B2 — Cross-encoder rerank stage** · base · depends: none · **adapter SHIPPED 2026-07-08; real model (T2) + compose_context wiring deferred**
  - Objective: Implement `RerankStrategy::cross_encoder` (ONNX/cross-encoder behind a trait).
  - Status: `engram-rerank-cross-encoder` shipped — `RerankScorer` port +
    `CrossEncoderReranker::rerank` (stable sort, `FusionTrace` stamp, identity
    preserved), 4 tests. Feature-gated real model (T2) and the `compose_context`
    hook (a `RetrievalReranker` port) are follow-ups.

- **B3 — Graph-analytics port + PageRank centrality** · base · depends: none · **PageRank SHIPPED 2026-07-08 (betweenness B4 / Louvain B5 follow)**
  - Objective: A focused graph-analytics crate; PageRank first.
  - Status: `engram-graph-analytics` shipped — std-only, generic
    `pagerank(edges, damping, iterations, tol)` over a decoupled `(source,
    target)` edge list (not coupled to `KnowledgeRelationship`/A2), 5 tests.
    Betweenness (B4) and Louvain→hierarchy clusters (B5) are follow-on
    micro-specs in the same crate.

- **B4 — Betweenness centrality (bridges)** · base · depends: B3 · **SHIPPED 2026-07-08**
  - Objective: Add betweenness centrality.
  - Status: `betweenness(edges)` (Brandes' algorithm) added to
    `engram-graph-analytics`; bridge-carries-traffic + parallel-paths-split
    (0.5) + empty + deterministic tests green (4 tests).

- **B5 — Community detection (Louvain) → hierarchy clusters** · base · depends: B3 · **single-level SHIPPED 2026-07-08; multi-level + HierarchyNode wiring deferred**
  - Objective: Implement Louvain; wire communities to `HierarchyNode(kind=cluster)`.
  - Status: `communities(edges, max_passes)` (single-level modularity-greedy
    local-moving) added to `engram-graph-analytics`; triangle→1,
    disconnected-cliques→2, single-edge→merge, empty tests green. Multi-level
    aggregation and the `HierarchyNode` wiring are follow-ups.

- **B6 — Bi-temporal entities** · base · **Part 1 SHIPPED 2026-07-08 (ADR-0019); `as_of` retrieval deferred to its own v1-contract micro-spec**
  - Objective: Add optional `validFrom`/`validUntil` to `KnowledgeEntity` (+ an
    `as_of` retrieval filter, deferred).
  - Status: ADR-0019 + optional `validFrom`/`validUntil` on `KnowledgeEntity`
    (draft-extension type — not in the frozen v1 schema, so no v1 schema regen);
    round-trip conformance test green; 7 constructors updated; `cargo check
    --workspace` + `.codex/hooks/check-contracts.sh` pass. The `as_of` retrieval
    filter touches the frozen-v1 `QueryFilter` → its own micro-spec; ingest
    stamping of `valid_from`/`valid_until` is a follow-up.

- **B7 — Pluggable embedding-provider registry + code model** · base · depends: none · **gated by Q3**
  - Objective: Promote the embedding provider to a registry; add a
    code-specialised option (jina-code / bge-code) alongside BGE-small.
  - Acceptance: register the code provider, embed chunks, retrieve; provider is
    config, not core.

- **B8 — Cross-source workspace fusion** · base · extends RFC 0008 · depends: none
  - Objective: Multi-source fusion — several sources under one workspace resolve
    cross-source edges (generic, not code-specific).
  - Acceptance: two sources under one workspace link across boundaries; scope
    isolation holds.

### Phase C — Code-specific data ops (on top of base)

- **C1 — Cross-file symbol edge resolution** · data · depends: A1
  - Objective: Extend the graph extractor so caller→callee `calls` edges resolve
    across files within a repo (today intra-file).
  - Acceptance: a CALLS edge connects symbols declared in two different files.

- **C2 — Code node taxonomy (priority languages)** · data · depends: A2 · **gated by Q4**
  - Objective: Map Tree-sitter kinds to the enriched `EntityKind` for Rust/TS/Python.
  - Acceptance: symbols carry correct kinds on a fixture repo per language.

- **C3 — Cyclomatic complexity** · data · depends: A2
  - Objective: Per-symbol cyclomatic complexity from the AST.
  - Acceptance: known functions return expected complexity values.

- **C4 — Dead-code detection** · data · depends: C1, B3
  - Objective: Symbols with zero in-degree on `calls`.
  - Acceptance: dead-code list matches a golden fixture.

- **C5 — Blast-radius + dependency-path** · data · depends: B3, B4, C1
  - Objective: Transitive-caller traversal (blast radius) and from→to path queries
    over the call graph.
  - Acceptance: `depth=N` callers and a from→to path match the fixture graph.

- **C6 — Temporal scoring engine (6 modes + significance budget)** · data · depends: B6
  - Objective: The six scoring modes (recent/impact/novel/directional/compound/
    overview) + structural-significance budgeting over symbol versions.
  - Acceptance: each mode ranks a git-history fixture as specified; budget covers
    ≥80% of significance with the minimum set.

- **C7 — HTTP endpoint detection (framework scanners)** · data · depends: C2 · **gated by Q4**
  - Objective: Framework-aware endpoint extraction for priority frameworks.
  - Acceptance: endpoints extracted with method + path + handler.

- **C8 — HTTP call-site detection + cross-service topology** · data · depends: C7, C1, B8
  - Objective: Detect call sites and resolve cross-repo HTTP edges.
  - Acceptance: a frontend call links to a backend route across two indexed repos.

- **C9 — Process / flow auto-detection** · data · depends: C1
  - Objective: Named execution flows from entry points.
  - Acceptance: a known flow returns the expected ordered path.

### Phase D — Integration (consumer surface)

- **D1 — engram-node bindings: base capabilities** · integration · **gated by Q1** · depends: B1, B2, B3, B6
  - Objective: N-API surface for lexical mode, rerank, analytics, `as_of`.
  - Acceptance: TS calls the new APIs; type tests + round-trip tests pass.

- **D2 — engram-node bindings: code ops** · integration · depends: D1, C3, C4, C5, C6, C8
  - Objective: N-API surface for complexity, dead-code, blast-radius, topology,
    scoring.
  - Acceptance: TS calls the new APIs; round-trip tests pass.

- **D3 — MCP server** · integration · **gated by Q2** · depends: D1
  - Objective: Realise the `mcp-server` spec — an MCP server (stdio + HTTP)
    exposing engram + codegraph over the chosen host, with owner/attach model.
  - Acceptance: an MCP client connects and calls tools end-to-end.

- **D4 — MCP tool surface (~38 tools, phased)** · integration · depends: D3, C-specs
  - Objective: Discovery, impact, temporal, topology, quality, index-mgmt tool
    groups (delivered as sub-PRs per group).
  - Acceptance: each tool group has a fixture-driven MCP integration test;
    `CapabilityReport` advertises implemented tools and marks the rest `deferred`.

- **D5 — Dashboard UI (graph + node panels + timeline + insights)** · integration · depends: D3/D4
  - Objective: Extend the demo/`enterprise-3d-graph` UI with node CODE/INFO/HISTORY
    panels, timeline, and computed insights.
  - Acceptance: visual QA — graph renders, node drill-down shows the three tabs,
    timeline shows version history for a symbol.

- **D6 — Agent skills (priority set)** · integration · **gated by Q4, consumers** · depends: D4
  - Objective: Engram-flavoured `SKILL.md` set (search, relationships, impact,
    evolution, topology, first-workflow).
  - Acceptance: skill evals pass; a guided query chains tools correctly.

- **D7 — Fleet coordination (minimal)** · integration · **gated by Q5 (defer recommended)** · depends: D3
  - Objective: Minimal publish-intent / record-episode / resolve, mapped onto
    engram's consolidation + contradiction model.
  - Acceptance: two simulated agents publishing conflicting intents produce a
    reviewable contradiction, not a silent overwrite.

- **D8 — CapabilityReport for consumers** · integration · depends: D3
  - Objective: Expose the accepted `CapabilityReport` model through MCP so
    consumers discover supported tools/modes.
  - Acceptance: a probe returns active/deferred/unsupported per capability key.

## Suggested start

**A1 (capability audit)** — independent, low-risk, resolves every borderline
state before any build item commits to a shape, and directly feeds B/C scoping.
Confirm and I'll open it through `new-spec` (its own assumption-gate) then
`work-loop`.

## Verification gates (per `AGENTS.md` / `implementation-roadmap.md`)

```bash
python3 tools/scripts/validate_contracts.py
.codex/hooks/check-contracts.sh
.codex/hooks/check-docs.sh
cargo fmt --all --check && cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm run contracts:check-generated && pnpm run typecheck && pnpm run test && pnpm run build
```
