# Spec: Ingestion Source-Type Split

- **Status:** Draft
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** RFC-0010 (decompose god modules by responsibility), AGENTS.md (god-module prohibition), ADR-0010 (behavior-port split)
- **Brief:** none
- **Contract:** none (internal adapter refactor, no domain contract change)
- **Shape:** integration

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Decompose the ingestion adapter by source type — introduce a `SourceExtractor` trait that encapsulates source-type-varying extraction logic (code symbols, prose concepts, OpenAPI contracts, structured data) while keeping file walking, git detection, persistence, and reconciliation as a shared source-agnostic spine. This makes adding a new source type (Excel, DB rows) a matter of implementing a trait behind the crate facade, not editing shared scanner/extractor branches.

## Boundaries

### Always do

- Put source-type-varying extraction logic behind the `SourceExtractor` trait.
- Keep file walking, git detection, persistence, and reconciliation source-agnostic in the shared spine.
- Preserve the ingestion crate's public facade (scan_repository, GraphExtractor, DocumentIngestRequest).
- Use the existing `Chunker` trait abstraction; do not re-abstract chunking.
- Maintain all existing tests without behavior change.
- Keep the split within the `adapters/ingest` crate as focused modules behind the trait.

### Ask first

- Promote any source-type module to a sub-crate before the set grows large enough to warrant a hard compile/ownership boundary.
- Add new extraction kinds beyond the four specified (code/docs/structured/contract).
- Modify the shared spine's responsibilities (file walk, git detection, persistence, reconciliation).

### Never do

- Leak source-type-specific logic into the shared spine or orchestration loop.
- Edit scanner.rs or extractor.rs match statements when adding a new source type.
- Expose the `SourceExtractor` trait outside the ingestion crate.
- Break the existing public API or change caller-facing behavior.
- Re-abstract the already-abstracted `Chunker` trait.

## Testing Strategy

- **TDD:** `SourceExtractor` trait methods, per-extractor implementation correctness, trait dispatch logic.
- **Goal-based:** crate compiles and all existing tests pass without behavior change.
- **Integration:** full ingestion pipeline end-to-end with code/docs/contract sources produces the same entities/relationships as before.

## Acceptance Criteria

- [ ] `SourceExtractor` trait exists in `adapters/ingest` with methods for extraction and chunk strategy selection.
- [ ] Three focused source-type modules exist: code (tree-sitter symbols), docs (prose concepts), contract (OpenAPI parser), plus a structured stub placeholder (Excel/DB) demonstrating extensibility pattern.
- [ ] Source-type dispatch is consolidated from scattered branches into a unified dispatch pattern: (a) FileKind-based ingestor selection (Code vs Text) in scan_repository loop, (b) file-extension-based tree-sitter enablement for code, (c) document-kind-based extractor dispatch (SourceExtractor trait), and (d) extension-based contract detection — consolidated so the spine routes to extractors via document kind where applicable.
- [ ] Shared spine (file walk, git detection, persistence, reconciliation) remains source-agnostic and unchanged in responsibility.
- [ ] `contract.rs` (1047 lines) is split into parse/entity-build/persist+reconcile within the contract source-type unit.
- [ ] All existing ingestion tests pass without behavior change (84 tests verified at baseline).
- [ ] Public crate facade (scan_repository, GraphExtractor, DocumentIngestRequest) is preserved.
- [ ] Adding a new source type is a trait impl addition, not an edit to shared orchestration code.

## Assumptions

- Technical: Ingestion god modules exist with line counts matching RFC-0010 (contract.rs: 1047, scanner.rs: 994, extractor.rs: 519) (source: `wc -l adapters/ingest/src/*.rs`)
- Technical: Source-type dispatch is scattered across multiple files (scanner, extractor, filesystem) (source: RFC-0010 spike verification)
- Technical: `Chunker` trait already exists at `chunker.rs:24` (source: grep verification)
- Technical: `SourceDocumentKind` enum exists in `engram_domain` and is used for classification (source: grep verification)
- Process: RFC-0010 is Draft and provides the doctrine for this decomposition (source: RFC-0010 status)
- Process: AGENTS.md prohibits god modules and requires splitting when a file mixes multiple reasons to change (source: AGENTS.md god-module prohibition)
- Product: Future structured ingestion (Excel, DB rows) requires adding a new source type without editing existing branches (source: RFC-0010 complication section)
