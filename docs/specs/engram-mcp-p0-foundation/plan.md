# Plan — engram-mcp P0 Foundation (RFC-0016)

Implements [`spec.md`](./spec.md). Cites the research doc for file:level detail.
Branch: `feat/engram-mcp-core`.

## Tasks

### T1 — Default the MCP to single-file storage  · goal-based · Depends on: none
- **Tests:** regression test `open_provider_produces_single_file` — open a provider under
  a temp dir, assert `engram_data.db` exists and `memory.db`/`knowledge.db`/`belief.db`/
  `hierarchy.db`/`vectors.db` do not (mirrors `agentzero/.../persistence_factory.rs:460-477`).
- **Approach:** in `mcp/engram-mcp/src/bootstrap.rs::open_provider`, chain
  `.with_sqlite_storage_layout(SqliteStorageLayout::SingleFile { file_name: <name> })`
  onto `EngramConfig::new(...)`, where `<name>` comes from the new `McpConfig` field (T2).
  Import `SqliteStorageLayout` from `engram_integration`.

### T2 — `--layout` / `--db-file` CLI flags  · TDD · Depends on: none
- **Tests:** `from_args` parses `--layout single|multi` + `--db-file <name>`; rejects
  `--layout bogus`; defaults `layout=single`, `db_file="engram_data.db"`.
- **Approach:** add `sqlite_layout` (enum `single|multi`) + `db_file: String` to
  `McpConfig` (`mcp/engram-mcp/src/config.rs`); parse in `from_args`; map to
  `SqliteStorageLayout` in `open_provider`. Update the `usage:` string in `main.rs`.

### T3 — Enrich ontology + taxonomy defaults  · TDD · Depends on: none
- **Tests:** `OntologyConfig::default()` has 3 layers (technical/domain/business) with
  non-empty classes + `within`+`across` predicates; `TaxonomyConfig::default()` has >1
  concept with at least one `broader` link. Existing parse tests still pass.
- **Approach:** replace the placeholder defaults in `mcp/engram-mcp/src/ontology.rs`
  (`Default for OntologyConfig` L47-66, `Default for TaxonomyConfig` L90-100) with a
  3-layer generic-technology ontology + small SKOS-aligned taxonomy. Additive serde fields
  only where needed (keep `#[serde(default)]`).

### T4 — Wire `MarkdownChunker` into the scanner  · goal-based · Depends on: none
- **Tests:** `scan_repo` over a `.md` fixture yields chunks with heading anchors (a
  `DocumentSection`-kind chunk whose text/anchor reflects the `#` heading).
- **Approach:** in `adapters/ingest/src/scanner.rs` (builds `text_ingestor` at L153-155),
  route `.md`/`.markdown` through `MarkdownChunker` and keep `PlainTextChunker` for other
  text. Confirm the chunker's `chunk()` output feeds `KnowledgeIngestor` unchanged.

### T5 — Generalize the cross-file resolver past `calls`  · TDD · Depends on: T4
- **Tests:** two text docs whose concepts co-reference each other (concept in doc A named
  in doc B's body) yield a cross-document `mentions` edge with a resolved `object.id`.
- **Approach:** at `adapters/ingest/src/scanner.rs:428-442`, extend the resolver to also
  fill name-only `object` refs for `predicate == "mentions"` against the global
  `name_index`. Additive — `calls` resolution unchanged.

### T6 — Concept → code `describes` bridge  · TDD · Depends on: T4, T5
- **Tests:** a fixture with `fn foo()` and a markdown section `# foo` yields
  `concept(foo) -[describes]-> function(foo)`; both are in one connected component.
- **Approach:** after per-file extraction + `calls`/`mentions` resolution, add a pass that,
  for each concept entity, checks the global code-symbol `name_index` for an exact
  word-boundary / heading-anchor match and emits a `describes` relationship. High-precision
  only (exact match); prose co-occurrence is explicitly *not* added now (documented).

### T7 — Gates + manual QA  · manual QA · Depends on: T1–T6
- **Tests:** `cargo fmt --all`; `cargo check --workspace`; `cargo test -p engram-mcp
  -p engram-ingest`; `check-engine-neutrality.sh`; `check-docs.sh`. Then rebuild the MCP,
  back up + clear `~/.engram/agentzero/`, re-index agentzero, confirm one `engram_data.db`
  and that `search`/`capability_report` work.
- **Approach:** run the mechanical gates; re-index the live deployment; record observations.

## Sequencing

T1+T2 land together (single-file). T3 independent (defaults). T4→T5→T6 is the bridge chain
(T6 depends on the chunker + the generalized resolver). T7 last. Commit per task.
