# Plan: Ingestion Source-Type Split

- **Spec:** [`spec.md`](spec.md)
- **Status:** Draft

## Approach

Decompose the ingestion adapter by source type following RFC-0010's doctrine: split modules by their *reasons to change* (the decisions that vary), not by processing steps. The strategy is:

1. **Introduce `SourceExtractor` trait** - define the interface for source-type-varying extraction logic
2. **Create focused source-type modules** - implement code/docs/structured/contract extractors behind the trait
3. **Consolidate scattered dispatch** - replace kind branches across scanner/extractor/filesystem with single trait dispatch
4. **Split contract.rs internally** - parse vs entity-build vs persist+reconcile within the contract module
5. **Preserve public facade** - keep crate exports unchanged; all changes are internal

This is a behavior-preserving internal refactor. No domain model changes, no public API changes, and no changes to host application code. All existing tests must pass without modification.

## Constraints

- **RFC-0010** governs the decomposition doctrine (reason-to-change splitting) and target sequence
- **AGENTS.md** prohibits god modules and requires facade preservation at crate roots
- **ADR-0010** (behavior-port split) provides precedent for port/trait refactoring
- **Existing tests must pass** - 84 unit/integration tests depend on current behavior
- **No public API change** - crate facade (scan_repository, GraphExtractor, DocumentIngestRequest) is preserved
- **No new dependencies** - use existing traits (Chunker) and domain types (SourceDocumentKind)

## Construction tests

- **Integration tests:** all 84 existing ingestion tests pass without behavior change
- **Manual verification:** adding a new source type is a trait impl, not a scanner/extractor edit

## Design (LLD)

### Design decisions

- **Decomposition axis: source type** (not pipeline stage) → satisfies OCP; new type = new impl
- **Boundary mechanism: focused modules behind trait** → satisfies AGENTS.md "prefer small crates"; `adapters/ingest` is already focused
- **Trait consolidates dispatch** → replaces scattered branches with single polymorphic call
- **Contract.rs split internally** → keeps extraction cohesive while separating persistence concerns
- **Preserve crate facade** → no caller-facing changes; all re-exports stay the same

### Data & schema

- `SourceExtractor` trait: `extract_entities(document, chunk_strategy) -> Result<Vec<Entity>>`, `select_chunker(kind) -> Box<dyn Chunker>`
- Four implementation modules: `code`, `docs`, `structured` (stub), `contract`
- Shared spine: file walk, git detection, persistence, reconciliation (unchanged in responsibility)

### Interfaces & contracts

- `SourceExtractor` trait with methods for entity extraction and chunker selection
- Per-source-type modules implement the trait: `CodeExtractor`, `DocsExtractor`, `StructuredExtractor` (stub), `ContractExtractor`
- Scanner orchestration consolidated to dispatch by `SourceDocumentKind` once

### Component / module decomposition

**New in `adapters/ingest`:**
- `extractors/mod.rs` - trait definition and dispatch consolidation
- `extractors/code.rs` - tree-sitter symbol extraction (refactored from extractor.rs)
- `extractors/docs.rs` - prose concept extraction (refactored from extractor.rs)
- `extractors/contract.rs` - OpenAPI contract extraction (refactored from contract.rs)
- `extractors/structured.rs` - Excel/DB row extraction stub

**Modified:**
- `scanner.rs` - consolidate kind dispatch to single `SourceExtractor` call
- `extractor.rs` - remove kind-specific logic; moved to extractors/
- `contract.rs` - internal split into parse/entity/persist modules

### Dependencies & integration

- Depends on existing `Chunker` trait (chunker.rs:24)
- Uses existing `SourceDocumentKind` enum from `engram_domain`
- No new external dependencies

## Tasks

### T1: Define SourceExtractor trait

**Depends on:** none

**Touches:** `adapters/ingest/src/extractors/mod.rs` (new file)

**Tests:**
- Trait compiles with `extract` and `select_chunker` methods
- Trait has generic parameter for document type
- Methods return appropriate result types

**Approach:**
- Create `extractors/mod.rs` with trait definition
- Define `extract(&self, document: SourceDocument, chunks: Vec<Chunk>) -> Result<Vec<Entity>>`
- Define `select_chunker(&self) -> Box<dyn Chunker>` for chunk strategy selection
- Add docstring explaining trait purpose and OCP extensibility

**Done when:** trait exists and compiles

### T2: Refactor code extraction into CodeExtractor

**Depends on:** T1

**Touches:** `adapters/ingest/src/extractors/code.rs` (new file), `extractor.rs`

**Tests:**
- Code extractor implements `SourceExtractor`
- Tree-sitter chunker is selected for code documents
- Existing code symbol extraction tests pass

**Approach:**
- Move tree-sitter logic from extractor.rs to new code.rs module
- Implement `SourceExtractor` for code symbols
- Select tree-sitter chunker in `select_chunker`
- Keep entity/relationship extraction logic intact

**Done when:** code extraction works through trait

### T3: Refactor docs extraction into DocsExtractor

**Depends on:** T1, T2

**Touches:** `adapters/ingest/src/extractors/docs.rs` (new file), `extractor.rs`

**Tests:**
- Docs extractor implements `SourceExtractor`
- Plain-text chunker is selected for text documents
- Existing prose concept extraction tests pass

**Approach:**
- Move prose concept logic from extractor.rs to new docs.rs module
- Implement `SourceExtractor` for docs concepts
- Select plain-text chunker in `select_chunker`
- Keep entity/relationship extraction logic intact

**Done when:** docs extraction works through trait

### T4: Create ContractExtractor with modular internals

**Depends on:** T1, T2, T3

**Touches:** `adapters/ingest/src/extractors/contract/` (new directory with parse.rs, entities.rs, persist.rs), `contract.rs`

**Tests:**
- Contract parsing logic moved to `extractors/contract/parse.rs`
- Entity building logic moved to `extractors/contract/entities.rs`
- Persistence/reconciliation in `extractors/contract/persist.rs` or main contract.rs
- Contract extractor implements `SourceExtractor`
- All existing OpenAPI extraction tests pass

**Approach:**
- Create `extractors/contract/` directory with organized modules
- Move OpenAPI parsing from contract.rs to `parse.rs`
- Move entity/relationship building to `entities.rs`
- Move persistence/reconciliation logic to `persist.rs`
- Implement `SourceExtractor` for API contracts
- Re-export types at contract.rs root to preserve facade

**Done when:** contract extraction is modularized, tests pass, and facade preserved

### T5: Create StructuredExtractor stub

**Depends on:** T1

**Touches:** `adapters/ingest/src/extractors/structured.rs` (new file)

**Tests:**
- Structured extractor implements `SourceExtractor`
- Stub returns empty entities for now
- Module compiles

**Approach:**
- Create stub module with trait implementation
- Return `Ok(vec![])` from extract method
- Add a follow-up comment marker for Excel/DB row implementation

**Done when:** stub compiles and trait is implemented

### T6: Consolidate scanner dispatch

**Depends on:** T1, T2, T3, T4, T5

**Touches:** `scanner.rs` (consolidate kind dispatch at lines 456,489,497,518,529,583)

**Tests:**
- Scanner dispatches to `SourceExtractor` by kind
- All existing ingestion tests pass
- No kind-specific branches remain in scanner orchestration

**Approach:**
- Identify all `match kind` branches in scanner orchestration loop
- Replace with single `extractor.extract(document, chunks)?` call
- Consolidate file-type classification logic
- Verify no branch scattering remains

**Done when:** dispatch is consolidated and tests pass

### T7: Remove extractor.rs kind branches

**Depends on:** T6

**Touches:** `extractor.rs` (remove is_code branch at line 108)

**Tests:**
- No kind-specific logic remains in extractor.rs
- All existing extraction tests pass
- File is significantly smaller

**Approach:**
- Move any remaining shared utility functions to appropriate modules
- Remove `is_code` kind branch
- Delete or stub unused functions

**Done when:** extractor.rs is cleaned up


### T9: Update extractors/mod.rs exports

**Depends on:** T1-T7

**Touches:** `extractors/mod.rs` (add trait and impl re-exports)

**Tests:**
- Crate facade preserves all public exports
- External callers see no API change
- Scanner can still instantiate and use extractors

**Approach:**
- Add `pub use` for `SourceExtractor` trait
- Re-export all extractor implementations
- Ensure scanner can dispatch without breaking

**Done when:** facade is preserved

### T10: Verify all ingestion tests pass

**Depends on:** T1-T7, T9

**Touches:** test files (no changes, run-only)

**Tests:**
- All 84 unit/integration tests pass
- No behavior changes detected
- Code/docs/contract extraction produce same entities as before

**Approach:**
- Run full test suite: `cargo test --package engram-ingest`
- Verify test counts match or exceed baseline
- Spot-check extraction output for known sources

**Done when:** all tests pass with same behavior

### T11: Verify adding new source type is trait impl

**Depends on:** T10

**Touches:** `extractors/structured.rs` (update stub to real impl)

**Tests:**
- New Excel extractor can be added by implementing trait
- No scanner/extractor edits required
- Trait dispatch picks up new impl automatically

**Approach:**
- Document the pattern: add new module, implement trait, register in dispatch
- Verify structured stub can be expanded without breaking existing code
- Confirm OCP: new type is additive, not editing shared code

**Done when:** extensibility pattern is verified

## Rollout

- **Delivery:** as a single internal refactor PR - no public API change
- **Infrastructure:** none - pure internal refactor
- **External-system integration:** none - no caller-visible changes
- **Deployment sequencing:** internal refactor lands independently; sequenced targets (knowledge/sqlite, bindings/node) are follow-on specs

## Risks

- **Refactor regression risk:** Large internal refactor could break behavior → Mitigated: 84 tests guard behavior; split is incremental per task
- **Facade drift risk:** Splitting could leak new public API → Mitigated: T9 explicitly verifies facade preservation
- **Over-splitting risk:** Decomposing past cohesion → Mitigated: doctrine is reason-to-change, not line count; stop when each unit answers to one reason
- **Test gap risk:** New trait might have untested paths → Mitigated: T10 runs full test suite; T11 verifies extensibility

## Changelog

- 2026-07-05: initial plan for RFC-0010 ingestion source-type split
