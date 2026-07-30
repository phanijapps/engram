# Spec — engram-mcp P0 Foundation (RFC-0016)

- **Status:** Implemented (P0 complete 2026-07-29; T5 cross-doc mentions deferred)
- **Mode:** full (structural: scanner/extractor changes + new CLI surface)
- **Constrained by:** RFC-0016 (D3, D4, ontology defaults), ADR-0008 (OntologyRepository advisory), ADR-0022 (engine neutrality / surface parity)
- **Plan:** [`plan.md`](./plan.md)

## Objective

Lay the foundation for RFC-0016 so the engram-mcp server writes **one shared
`engram_data.db`** consumable by the Zbot gateway, ships **real generic ontology +
SKOS taxonomy defaults** so the vocabulary is meaningful zero-config, and makes
**documentation connect to code** in a single unified knowledge graph (the code graph
becomes a layer of the KG, not a parallel silo).

## Background

Detailed root-cause analysis and file:line citations live in
[`docs/research/engram-mcp-connectivity-and-single-file-storage.md`](../../research/engram-mcp-connectivity-and-single-file-storage.md).
The decisions are ratified in
[`docs/rfcs/0016-...md`](../../rfcs/0016-zbot-class-memory-kg-code-as-final-layer.md) (D3, D4, ontology defaults).

## Assumptions (verified)

- `SqliteStorageLayout::SingleFile { file_name }` + `.with_sqlite_storage_layout(layout)` exist in `engram_integration` (`core/integration/src/config.rs:44,215`). `file_name` is validated to one path component with `.db`/`.sqlite`/`.sqlite3` — `"engram_data.db"` is valid.
- The Zbot adapter default is `SingleFile { "engram_data.db" }` (`agentzero/stores/zbot-engram-adapter/src/config.rs:336`); matching it makes the DB cross-consumer.
- The scanner's cross-file resolver is gated `predicate == "calls"` (`adapters/ingest/src/scanner.rs:428-442`); the co-occurrence loop sees only a per-document index (`adapters/ingest/src/extractor.rs:224-253`); `MarkdownChunker` exists but is not wired in.
- engram domain has `ConceptScheme`/`Concept`/`concept_relations`; the taxonomy config model `{ label, broader }` is already SKOS-aligned.

## Boundaries

**Always do**
- Additive only: new edges/predicates/classes are added; never drop existing `calls`/`belongs_to`/`mentions` semantics or existing tools.
- Route every storage change through `EngramProvider` / `EngramConfig`; keep engram core's default `MultiFileDirectory` (override at the MCP boundary only).
- Keep ontology/taxonomy read-only launch config (D6).

**Ask first**
- Changing the predicate a cross-file resolver resolves (broadens graph semantics).

**Never do**
- No `core/domain` contract break, no generated-TypeScript-type change.
- No in-place cross-store merge of the existing 5 files (regenerable; re-index instead).
- No server-side LLM. No enforced write-rejecting ontology validation.
- No SurrealDB changes (out of scope).

## Testing Strategy

- **TDD** for the deterministic bridge (P0b): seed a fixture mixing code + a markdown section whose heading matches a symbol; assert a `describes` edge + one connected component. Goal-based for the storage + config wiring.
- **Visual / manual QA** at the end: re-index agentzero into one `engram_data.db`, confirm one file + `search`/`capability_report` work, and (if feasible) that concept↔code links render.

## Acceptance Criteria

1. **AC1 (single-file)** — opening the MCP provider produces exactly one `engram_data.db`; the names `memory.db`, `knowledge.db`, `belief.db`, `hierarchy.db`, `vectors.db` do **not** exist under `storage_path`.
2. **AC2 (config flag)** — `--layout single|multi` (default `single`) and `--db-file <name>` (default `engram_data.db`) are parsed by `McpConfig`; unknown layout values are rejected (`-32602`-equivalent).
3. **AC3 (ontology/taxonomy defaults)** — `OntologyConfig::default()` is a 3-layer generic-technology ontology (technical rich + light domain/business) with non-empty `within` + `across` predicates; `TaxonomyConfig::default()` is a small SKOS-aligned multi-concept tree (broader/narrower), not a single concept.
4. **AC4 (doc↔code bridge)** — a `scan_repo` over a fixture with a code symbol `foo` and a markdown section `# foo` yields a `concept -[describes]-> function` edge; the concept and the function are in the same connected component.
5. **AC5 (cross-doc mentions)** — a concept that names a concept defined in a *different* document yields a resolved cross-document `mentions` edge (object `id` populated from the global name index).
6. **AC6 (MarkdownChunker wired)** — `.md`/`.markdown` files in `scan_repo` are chunked by the `MarkdownChunker` (heading anchors present), not `PlainTextChunker`.
7. **AC7 (gates)** — `cargo fmt --all`, `cargo check --workspace`, `cargo test -p engram-mcp -p engram-ingest`, `.codex/hooks/check-engine-neutrality.sh`, `.codex/hooks/check-docs.sh` all pass.
