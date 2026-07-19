# Plan: Documentation overhaul — functional README + architecture + guides

- **Spec:** [`spec.md`](spec.md)
- **Status:** Shipped

> **Strategy — audience-first, concepts before code.** The README today is
> code-heavy. The fix is not to delete the technical sections (they stay) but to
> **layer a functional, audience-aware entry point on top** and **link out** to a
> set of focused guides that the README cannot hold. Each deliverable has one
> canonical home (no duplication); the README is the hub, the guides are the
> spokes. Every doc backlinks to the README and to the research/ADR that
> grounds it. Tasks T1–T5 write the spokes; T6 rewrites the hub once the spokes
> exist so every backlink lands.

## Constraints
- Two audiences: **80% techno-functional** (understand tech + need functional
  context), **20% functional** (BAs, data scientists, POs, AI strategists — need
  what engram DOES, not Rust). Write for the 80% first; carry the 20% with
  concept framing + use cases.
- One canonical home per fact (no duplication across docs). The README links
  out; the guides own their topic. The per-section disposition of existing
  README sections is decided in T6.
- No `markdown-link-check` config exists in the repo — the link gate is **manual
  grep across `README.md` + the new doc files, recorded in the PR body**, plus a
  **walkthrough checklist** (entry point → link navigated) in the PR body.
- Diagrams use **image placeholders** (`![description](path/to/image.png)`) the
  user fills later, with an ASCII fallback in the doc so it renders today.
- Present tense, as-built — describe what exists, not what "will be."
- Do not edit `docs/domain-data-model.md` or ADRs (sources of truth).

## Existing docs to reconcile (not duplicate)
- `docs/architecture/overview.md` **exists** → UPDATE it (Memora-style pipeline
  + layer responsibilities). `docs/architecture/reference.md` stays as the
  reference architecture; this task enriches the overview.
- `docs/guides/how-to/build-a-surrealdb-store.md` **exists** → the general
  extension guide (T2) backlinks to it as the concrete worked example.
- `docs/research/README.md` **exists** (currently an inbox) → REWRITE as a
  synthesized index. `docs/research/synthesis.md` stays as the deep synthesis.

## Tasks

### T1: Architecture overview — Memora-style pipeline + layer docs
**Depends on:** none · **Mode:** manual QA (goal-based link gate)
- UPDATE `docs/architecture/overview.md`:
  - A **Memora-style pipeline flow diagram** (image placeholder + ASCII fallback)
    showing data flow: **agent input** (write memory / ingest source) →
    **engram processing** (extraction, memory lifecycle, knowledge graph,
    bi-temporal belief, hierarchy) → **storage cells** (engine-swappable) →
    **retrieval composition** (6 modes + RRF fusion + cross-encoder rerank) →
    **context packet out**. Model the visual on Microsoft Research's Memora
    Figure 1 (input → segmentation → memory entries + graph → policy-guided
    retrieval → output).
  - A **conceptual narrative** touching the synthesized research: memory
    lifecycle, source-grounded knowledge graph, bi-temporal belief synthesis,
    hierarchy for context compression, multi-mode retrieval + fusion,
    consolidation (reflection + decay), policy/provenance/scope governance,
    contract-first design. Cite the `docs/research/` files that ground each.
  - A **layer responsibilities** section: one paragraph each for the layers the
    user named — **core libs** (domain/runtime/memory/knowledge/belief/hierarchy/
    consolidation/reflection/retrieval/integration/eval/graph-analytics),
    **adapters** (engine cells + engine-agnostic retrieval/consolidation/ingest),
    **integration facade** (`EngramProvider`/`EngramConfig`/`CapabilityReport`),
    **N-API binding** (`bindings/node` → TS transport). For each: what it owns,
    the key type/trait, how it fits the pipeline.
- **Done when:** `overview.md` carries the pipeline diagram (placeholder + ASCII
  fallback), the concept narrative cites the research files, every layer is
  documented, it links back to the README + the relevant ADRs, **and** the
  README's existing `## Architecture` ASCII pipeline diagram has moved into
  `overview.md` (README section thinned to a one-paragraph summary + backlink —
  disposition settled in T6).

### T2: Storage extension guide — add `engram-store-<engine>`
**Depends on:** none · **Mode:** manual QA (goal-based link gate)
- NEW `docs/guides/how-to/extend-storage.md`:
  - The contract: a new engine is **one adapter cell** per capability behind the
    existing core port (memory/knowledge/belief/hierarchy/vectors), composed by a
    **bootstrap recipe** in `core/integration`. No engine type may leak into
    neutral layers (ADR-0022 + the neutrality lint).
  - Step-by-step (with file paths): scaffold `engram-store-<engine>`; implement
    each port (point at the SQLite cell as the template, the Surreal cell as the
    graph-native variant); add a `bootstrap_<engine>` recipe + a `BackendProfile`
    arm; declare the feature in `core/integration/Cargo.toml`; wire capability
    states into `CapabilityReport`; satisfy the neutrality + parity lints.
  - The "no cross-engine migration" contract (fresh store on switch) + the
    bi-temporal pattern (explicit valid_from/valid_until vs engine VERSION).
- Backlink the existing `docs/guides/how-to/build-a-surrealdb-store.md` as the
  concrete worked example; link to ADR-0022.
- **Update the stale framing in `build-a-surrealdb-store.md`** (line 12): it
  currently opens *"Engram ships no SurrealDB adapter, so it cannot be run
  verbatim here"* — now false (`engram-store-surreal` + `bootstrap_surreal`
  ship). Rewrite the callout to: *"SurrealDB is engram's reference second
  engine — `engram-store-surreal` ships. This guide covers the SURQL specifics;
  `extend-storage.md` covers the general add-an-engine contract."* The contract
  steps + port table below it are accurate and stay.
- **Done when:** a reader can enumerate every file to touch to add a backend;
  links to the template crates + ADR-0022 resolve; links back to the README.

### T3: Build guide — prerequisites, build, test, demo, MCP
**Depends on:** none · **Mode:** manual QA (goal-based link gate)
- NEW `docs/guides/how-to/build-and-run.md`:
  - Prerequisites (Rust toolchain, pnpm/Node, SQLite libs, optional model runtime
    for fastembed).
  - Build commands: **`default = []` (no backend)** — `cargo build` builds
    nothing storage-backed. The real commands: `--features sqlite` (SQLite),
    `--features surreal` (SurrealDB), `--features fastembed` (FastEmbed-backed
    SQLite vector provider — implies `sqlite`). Verify each against
    `core/integration/Cargo.toml` `[features]`. Then the N-API build
    (`bindings/node`) + `pnpm run build`.
  - Test commands: `cargo test --workspace`, the feature-gated Surreal tests,
    `pnpm run test` + `pnpm run typecheck` + `pnpm run contracts:generate`.
  - Demo setup (the RFC-0003 demo program) + MCP server startup (one line each,
    linking out to the MCP guide).
  - Validation hooks (`.codex/hooks/*`) the user runs before handoff.
- **Done when:** the commands are the actual ones from the repo (verified, not
  invented); links to the MCP guide + README resolve.

### T4: MCP guide — connect agents to engram + codegraph
**Depends on:** none · **Mode:** manual QA (goal-based link gate)
- NEW `docs/guides/how-to/connect-via-mcp.md`:
  - The **two MCP servers**: `memory/mcp-server` (`write_memory`, `recall`,
    `forget`, `put_entity`, `put_relationship`) and `codegraph/mcp-server`. For
    each: what it exposes, its stdio/HTTP transport, the config to launch it.
  - **Codegraph tool list — enumerate verbatim from
    `codegraph/mcp-server/src/main.rs` `tool_list()`** (snake_case, grouped):
    *indexing* — `scan_repo`, `search_code`, `repository_stats`,
    `capability_report`; *query* — `dead_code`, `blast_radius`, `dependency_path`,
    `symbol_context`, `process_flow`, `find_entry_points`; *ranking* —
    `central_symbols`, `bridge_symbols`, `call_communities`,
    `cyclomatic_complexity`, `most_complex`; *http/endpoint* — `find_endpoints`,
    `find_api_calls`, `match_api_topology`; *temporal* — `temporal_recent`,
    `temporal_impact`, `temporal_compound`, `temporal_overview`,
    `temporal_directional`. (23 tools — verify against source at write time; the
    list may have grown.) Note the index-then-query flow: `scan_repo` first, then
    query/rank.
  - Client configs for **Copilot, Claude Desktop, Cursor** (the MCP JSON block +
    command), with the `npx`/binary invocation.
  - When to use MCP vs the N-API library binding vs the Rust facade (one table).
- **Done when:** both servers' tool lists are accurate to source (verified by
  grep against `tool_list()`); the client config blocks are real; links back to
  the README resolve.

### T5: Synthesized research index
**Depends on:** none · **Mode:** manual QA (goal-based link gate)
- REWRITE `docs/research/README.md` (currently an inbox) as a **synthesized
  index**: a short synthesized summary of where engram sits relative to prior art
  (Mem0/Zep/Letta/GraphRAG/Memora) + a table mapping each `docs/research/` file
  to the engram concept it informs (memory lifecycle, knowledge graph, belief,
  hierarchy, retrieval composition, consolidation, governance) with backlinks.
  The existing `synthesis.md` stays as the deep synthesis; the README is the map.
- **Done when:** every research file is linked with a one-line "what it informs"
  note; the synthesized summary cites the comparison; links back to the README.

### T6: Functional README — techno-functional hub with backlinks
**Depends on:** T1–T5 (paths fixed in spec; written last so backlinks land)
**Mode:** manual QA (goal-based link gate)
- REWRITE the README's top sections for the **techno-functional audience**:
  - Lead with **what engram is + the functional use cases** (long-horizon agent
    memory, source-grounded knowledge, governed recall) — concepts first, less
    code. Enough framing for the 20% functional reader.
  - A **concepts at a glance** block (the 8 pillars, one line each) linking to
    the architecture overview (T1).
  - A **documentation map** linking to every guide: architecture overview (T1),
    storage extension (T2), build (T3), MCP (T4), research synthesis (T5).
- **Canonical-home disposition — existing README sections vs the new guides**
  (each row: keep / thin-to-backlink / delete; no content lives in two places):

  | Existing README section | New guide that owns it | Disposition |
  | --- | --- | --- |
  | `## The conceptual model` (8 pillars, L46) | T1 overview.md (pipeline + layers) | **Keep** — the functional hook for both audiences; overview.md does NOT re-explain the pillars, it shows the pipeline + layers + research links |
  | `## Storage backends` + port table (L172) | T2 extend-storage.md | **Thin** — keep the "one crate per engine, swap by config" idea + a 3-line backend-selection example; move the port-trait table into extend-storage.md; backlink |
  | `## Architecture` ASCII pipeline (L217) | T1 overview.md | **Thin** — the ASCII diagram MOVES to overview.md; README keeps a one-paragraph "how data flows" summary + backlink |
  | `## Quick start` (L350) | T3 build-and-run.md | **Keep minimal** — the 3 essential commands stay inline (first thing a reader runs); backlink to build-and-run.md for demo/MCP-startup/validation-hooks/all-feature combos |
  | `## Connect via MCP` (L429) | T4 connect-via-mcp.md | **Thin** — keep a one-paragraph "two MCP servers" summary + one client config snippet; link to the guide for the full tool list + all client configs |
  | Status codegraph list (L290–298) | T4 | **Fix** — stale tool names (`dead-code`…`communities`); rewrite to the verbatim snake_case tool families from T4 |

  KEEP `## Contracts`, `## Development workflow`, `## Contributing`, `## License`
  unchanged (no guide owns them; they stay canonical in the README).
- **Done when:** README opens with functional framing + use cases; links to all
  five guides + the architecture overview; the disposition table is executed
  (no section duplicated with a guide; the stale codegraph list is corrected); a
  techno-functional reader can reach any guide from the README; all backlinks
  resolve; the walkthrough checklist + grep link-result are in the PR body.

## Rollout
- One PR per task is ideal; T1–T5 are independent and can land in any order.
  T6 lands last (it backlinks to all spokes + executes the disposition table).
- Final gate: manual grep across `README.md` + the new doc files confirms no
  broken internal links before merge; the walkthrough checklist (entry point →
  link navigated) is recorded in the PR body.

## Changelog
- 2026-07-16: drafted. Shape `mixed` (docs + structure). Confirmed assumptions:
  audience 80/20 techno-functional/functional; Memora-style architecture diagram
  with image placeholders; document core libs/adapters/integration/N-API;
  storage-extension guide; build + MCP guides; README backlinks all.
- 2026-07-19: shipped (T1–T6). **T2 scope expanded during EXECUTE:**
  `build-a-surrealdb-store.md` was more deeply stale than the spec/review caught
  (pre-consolidation: one-crate-per-family, remote ws client, `bootstrap_provider`
  in `adapters/integration/wiring.rs`). Rewrote it to match the shipped
  `engram-store-surreal` (one-crate-per-backend, SurrealKV embedded,
  `bootstrap_surreal`) rather than just fixing the callout — leaving it as-is
  would have been context-poisoning. Adversarial review (impl mode) found + fixed
  5 issues: duplicate `## Why engram exists` heading, overview layout tree
  inventing `adapters/rerank/`, unverified "ICML 2026" venue claim, inconsistent
  Surreal test command, and a `EngramProvider::consolidate()` API reference (real
  API is `require_consolidation()`).
