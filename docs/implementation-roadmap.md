# Implementation Roadmap

## Purpose

This roadmap sequences Engram from accepted contracts to a production-ready
agentic memory layer. Use it as the outer loop for spec-driven development:
finish one phase, validate it, record the decision or gap, then start the next
small slice.

The project should stay contract-first, modular, and composable. Rust owns the
domain behavior and deterministic engine boundaries. TypeScript owns generated
contracts, ergonomic clients, native binding packaging, and framework
integrations. Storage, vector search, model providers, and runtimes stay behind
ports until their contracts are proven.

## Development Loop

Apply this loop to every feature slice.

1. Update or add the implementation spec under `docs/specs/<feature>/`.
2. Confirm whether the public contract changes.
3. If the public contract changes, update `docs/domain-data-model.md`,
   `contracts/v1/`, `specs/v1/`, examples, invalid examples, and generated
   TypeScript.
4. Add conformance tests that prove Rust serialization still matches the
   accepted schema.
5. Implement the smallest vertical slice behind existing traits or a new
   narrowly scoped port.
6. Add deterministic tests for success, denial, invalid input, idempotency, and
   scope isolation where applicable.
7. Run the full repository checks.
8. Update `CHANGELOG.md`, `ROADMAP.md`, and ADRs or RFCs when the design
   changes.

Required checks before a slice is considered done:

```bash
python3 scripts/validate_contracts.py
.codex/hooks/check-contracts.sh
.codex/hooks/check-docs.sh
.codex/hooks/pre-implementation-check.sh
cargo fmt --all --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm run contracts:check-generated
pnpm run typecheck
pnpm run test
pnpm run build
```

## Phase 0: Repository Contract Baseline

Status: complete.

Goal: keep the accepted v1 contract coherent before implementation grows.

Deliverables:

- Accepted domain model for the v1 memory surface.
- JSON Schema, examples, invalid examples, and specs under versioned paths.
- Generated TypeScript contracts with drift checks in CI.
- Rust domain projections with schema conformance tests.
- Repository instructions, ADRs, RFCs, changelog, and validation hooks.

Acceptance gate:

- Contract validation passes.
- Generated TypeScript has no drift.
- Rust domain serialization validates against the accepted v1 schema.
- Documentation hooks pass.

## Phase 1: Core Crate Boundaries

Status: complete.

Goal: keep `engram-core` clean while implementation code moves into focused
crates.

Crates:

- `engram-domain`: accepted domain types, serde behavior, invariants, version
  markers.
- `engram-core`: public traits, service contracts, orchestration interfaces,
  typed errors.
- `engram-store-memory`: in-memory implementations for specs, tests, examples,
  and deterministic local development.

Implementation work:

- Move `InMemoryMemoryService` out of `engram-core` into
  `engram-store-memory`.
- Keep `engram-core` free of concrete storage state.
- Add event read/query contracts required for audit, evaluation, consolidation,
  and debugging.
- Introduce injectable `Clock` and `IdGenerator` use in the write path.
- Document crate responsibilities in each crate README.

Acceptance gate:

- `engram-core` exposes ports, not concrete adapters.
- In-memory write tests pass from the adapter crate.
- Event history can be queried without exposing private storage internals.
- No storage, vector, model, Node, or TypeScript dependency enters
  `engram-domain` or `engram-core`.

## Phase 2: Write Memory Completion

Status: complete for in-memory baseline; SQL conformance remains future work.

Goal: make writing memories complete enough to become the foundation for later
retrieval, forgetting, evaluation, and persistence.

Crates:

- `engram-domain`
- `engram-core`
- `engram-store-memory`
- later `engram-store-sql`

Implementation work:

- Convert `specs/v1/write-memory.md` acceptance checks into executable fixtures.
- Validate required fields and contract-level forbidden behavior.
- Enforce policy before durable mutation.
- Preserve provenance, policy, content, links, timestamps, and metadata.
- Support deterministic idempotency scoped by tenant, subject, workspace, and
  idempotency key.
- Append a lifecycle event for every accepted write.
- Add repository-level event reads for memory ID and scope.
- Define transaction semantics for durable adapters before SQL is implemented.

Acceptance gate:

- Valid write creates exactly one active memory and one written event.
- Invalid writes create no memory and no event.
- Denied writes create no memory and no event.
- Repeated idempotent writes return the original record.
- Scope isolation is tested.
- Fixtures can be reused by future SQL and native binding implementations.

## Phase 3: Retrieve Context Slice

Status: complete for in-memory baseline; advanced retrieval remains future work.

Goal: retrieve useful, explainable context from written memories without adding
embeddings or external services yet.

Crates:

- `engram-core`
- `engram-store-memory`
- `engram-retrieval` if retrieval logic becomes large enough to split.

Implementation work:

- Implement exact and keyword retrieval over in-memory records.
- Apply scope matching before ranking.
- Apply policy authorization before returning results.
- Add deterministic scoring with a documented baseline.
- Compose `ContextPayload` with result explanations, source failures, omitted
  results, and budget behavior.
- Add fixture cases for positive recall, forbidden recall, no-result behavior,
  and budget truncation.

Acceptance gate:

- Retrieval never crosses tenant boundaries.
- Retrieval does not return denied memories.
- Results include enough explanation to debug why they were selected.
- Budgeted retrieval reports omitted results instead of silently hiding them.

## Phase 4: Forget, Redact, Archive, and Tombstone

Status: complete for in-memory baseline; durable adapter conformance remains
future work.

Goal: make lifecycle control first-class before adding more stores.

Crates:

- `engram-core`
- `engram-store-memory`

Implementation work:

- Implement `ForgetRequest` behavior for accepted delete modes.
- Add policy checks for forget operations.
- Record lifecycle events for tombstone, archive, redact, and hard delete where
  supported.
- Define retrieval behavior for inactive, archived, redacted, and deleted
  records.
- Add retention and expiration evaluation hooks.

Acceptance gate:

- Forget operations are policy-checked.
- Every lifecycle mutation is auditable.
- Retrieval respects status and delete mode.
- Hard delete behavior is explicit and tested.

## Phase 5: Evaluation Harness

Status: complete for Rust fixture-runner baseline; TypeScript helpers remain
future work.

Goal: make quality and safety measurable before introducing embeddings,
hierarchy, beliefs, or production stores.

Crates and packages:

- `engram-eval`
- `packages/eval`

Implementation work:

- Define fixture file format for writes, retrievals, expected recalls, forbidden
  recalls, and policy expectations.
- Implement a deterministic Rust fixture runner.
- Add TypeScript helpers for authoring and running fixtures.
- Report failures by category: missing recall, forbidden recall, policy leak,
  ranking regression, missing explanation, and unexpected source failure.
- Add CI coverage for core fixtures.

Acceptance gate:

- Write, retrieve, and forget specs run as fixtures.
- Fixture reports are stable enough for CI.
- Evaluation results can be compared across in-memory and future SQL/vector
  adapters.

## Phase 6: SQL Persistence Adapter

Status: complete for SQLite in-memory service baseline; file-backed and server
database adapters remain future work.

Goal: add durable storage without changing the public domain contract.

Crates:

- `engram-store-sql`

Implementation work:

- Choose the first supported SQL engine through an ADR.
- Add migrations for memory records, events, idempotency keys, policy,
  provenance, knowledge sources, and chunks.
- Preserve contract payloads losslessly.
- Implement repository traits with transaction boundaries.
- Enforce idempotency atomically.
- Add adapter conformance tests by reusing the write, retrieve, forget, and
  evaluation fixtures.

Acceptance gate:

- SQL adapter passes the same fixtures as the in-memory adapter.
- Idempotent concurrent writes do not duplicate records.
- Event ordering is stable and queryable.
- Migrations are reversible or have an explicit forward-only policy.

## Phase 7: TypeScript Client and Native Binding

Status: complete for local NAPI bridge and package surface; release packaging
matrix remains future work.

Goal: expose the Rust behavior ergonomically to TypeScript without creating a
second implementation.

Crates and packages:

- `engram-node`
- `packages/node`
- `packages/client`
- `packages/contracts`

Implementation work:

- Add N-API binding for core service operations.
- Keep generated contracts as the TypeScript source for public payload types.
- Wrap native calls with a small client API.
- Add runtime validation at the package boundary where payloads enter from
  JavaScript.
- Add type tests and integration tests for write, retrieve, forget, and fixture
  execution.

Acceptance gate:

- TypeScript calls use generated contract types.
- Native binding payloads round-trip through Rust contract types.
- No TypeScript package redefines domain truth.
- The client can run the same acceptance fixtures against a local engine.

## Phase 8: Knowledge Ingestion for Code and Documents

Status: complete for deterministic text ingestion and in-memory repository
baseline; filesystem, Git, code-symbol, and SQL persistence adapters remain
future work.

Goal: store code and unstructured documents as source-grounded knowledge without
confusing them with agent memories.

Crates and packages:

- `engram-ingest`
- `engram-store-memory`
- `engram-store-sql`
- `packages/adapters`

Implementation work:

- Implement source readers for local files and Git repositories first.
- Add document extraction and hashing.
- Add chunkers for plain text, Markdown, and code-aware file/symbol chunks.
- Store `KnowledgeSource`, `SourceDocument`, and `KnowledgeChunk` records.
- Link chunks to provenance, source locations, and optional embeddings.
- Add ingestion dry-run behavior.
- Add fixtures for re-ingestion, changed files, deleted files, and scope
  isolation.

Acceptance gate:

- Code and documents are queryable as knowledge, not memory.
- Re-ingestion is idempotent when content hashes match.
- Source provenance survives chunking.
- Retrieval can compose memory and knowledge with clear result types.

## Phase 9: Vector and Hybrid Retrieval

Status: complete for sqlite-vec vector index baseline, opt-in FastEmbed
BGE-small smoke path, and deterministic weighted fusion.

Goal: add semantic retrieval as a replaceable index, not a dependency baked into
the core.

Crates:

- `engram-store-vector`
- `engram-retrieval` if split earlier.

Implementation work:

- Define embedding provider and vector index ports.
- Store embedding references separately from canonical records.
- Implement a first vector adapter through an ADR.
- Add hybrid fusion across keyword, metadata, vector, recency, and provenance
  signals.
- Preserve `FusionTrace` for explainability.
- Add evaluation fixtures for semantic recall and ranking regressions.

Acceptance gate:

- Canonical records remain usable without vectors.
- Vector adapter failures are reported as source failures, not silent empty
  results.
- Hybrid retrieval improves recall without policy leaks.

## Phase 10: Hierarchy Navigation

Status: complete for in-memory hierarchy repository and parent-chain navigation;
construction algorithms and retrieval expansion remain future work.

Goal: add hierarchical memory organization for navigation, compression, and
retrieval expansion.

Crates:

- `engram-hierarchy`

Implementation work:

- Implement hierarchy build configs and versioned hierarchy nodes.
- Build paths from memories, knowledge chunks, concepts, or entities.
- Add explainable parent-child and related-to relations.
- Support hierarchy-assisted retrieval expansion.
- Add fixtures for path generation, scope isolation, and stale hierarchy
  handling.

Acceptance gate:

- Hierarchy nodes are derived and auditable.
- Retrieval can explain when hierarchy affected results.
- Rebuilds do not corrupt older hierarchy versions.

## Phase 11: Belief Network

Status: complete for in-memory belief and contradiction repository baseline;
synthesis, detection, and retrieval remain future work.

Goal: derive reviewable beliefs and contradictions from evidence without making
beliefs indistinguishable from source facts.

Crates:

- `engram-belief`

Implementation work:

- Implement belief synthesis from memories and knowledge evidence.
- Track belief status, confidence, evidence links, and derivation method.
- Detect contradictions as review records.
- Add manual review and resolution records.
- Add retrieval paths that can include beliefs only when requested or allowed by
  policy.

Acceptance gate:

- Beliefs remain traceable to evidence.
- Contradictions do not silently mutate target records.
- Retrieval distinguishes memory, knowledge, and belief results.

## Phase 12: Consolidation and Sleep Cycle

Status: done for the dry-run run-reporting slice.

Goal: make background consolidation auditable and reversible enough to trust.

Crates:

- `engram-belief`
- `engram-hierarchy`
- `engram-eval`

Implementation work:

- Implement `ConsolidationService`.
- Add consolidation task types for deduplication, summarization, decay,
  hierarchy rebuild, belief synthesis, contradiction detection, and retention
  cleanup.
- Record `ConsolidationRun` and task-level outcomes.
- Support dry runs and bounded scopes.
- Run evaluation before and after consolidation to catch regressions.

Acceptance gate:

- Every durable consolidation mutation is represented in a run report.
- Failed tasks are recoverable and inspectable.
- Consolidation improves or preserves evaluation results for protected fixtures.

Shipped slice:

- Added a dry-run `ConsolidationService` implementation in `engram-core`.
- Validates scope, requester, dry-run mode, and time-window ordering before
  planning tasks.
- Returns auditable `ConsolidationRun` records with zero-mutation stats and no
  scheduler, model provider, or repository dependency.

## Phase 13: Integrations and Runtime Adapters

Status: done for the framework-neutral observed transport slice.

Goal: make Engram useful from real agent runtimes without contaminating core
contracts.

Packages:

- `packages/adapters`
- `packages/client`

Implementation work:

- Add adapter APIs for agent runtimes, gateways, CLIs, and service frameworks.
- Keep framework-specific concepts outside Rust domain contracts.
- Add examples for local engine, SQL-backed engine, and native TypeScript usage.
- Add observability hooks for policy denials, retrieval traces, consolidation
  runs, and adapter failures.

Acceptance gate:

- Integrations consume stable client APIs.
- Framework adapters can be added or removed without touching domain crates.
- Examples run in CI or a documented smoke-test path.

Shipped slice:

- Added `@engram/adapters` with an observed transport wrapper over
  `EngramTransport`.
- Emits operation, retrieval trace, transport error, and policy-denial-shaped
  events without changing operation results.
- Leaves framework-specific adapters, telemetry exporters, and examples for
  later slices.

## Phase 14: Production Hardening

Status: done for the public-repository hygiene slice.

Goal: prepare the project for external users and operational use.

Workstreams:

- Security review of policy, scope matching, deletion, and adapter boundaries.
- Performance benchmarks for write, retrieval, ingestion, and consolidation.
- Load tests for SQL and vector adapters.
- Compatibility policy for contract versions and migrations.
- Release automation for crates and npm packages.
- Documentation site or published book.
- Contributor guide, issue templates, examples, and governance notes.

Acceptance gate:

- Public APIs have compatibility guarantees.
- Benchmarks exist before performance claims are made.
- Releases can be reproduced from CI.
- New contributors can run tests and examples from documented commands.

Shipped slice:

- Updated public README status to reflect implemented pre-1.0 behavior.
- Added governance and release-checklist documentation.
- Scoped documentation checks to tracked repository docs and tracked repository
  skills so untracked local Codex assets do not block release gates.
- Preserved benchmark, security-audit, production-readiness, and release
  automation claims as future work until backed by evidence and CI automation.

## Phase 15: Filesystem Source Reader

Status: done for local filesystem discovery.

Goal: discover local code and unstructured text documents as source-grounded
knowledge inputs without adding Git history, embeddings, or persistence
concerns.

Crates:

- `engram-ingest`

Implementation work:

- Implement a local filesystem `SourceReader`.
- Discover supported text, Markdown, and code files in deterministic order.
- Preserve relative source paths, content hashes, policy, and provenance.
- Reject path traversal and oversized reads.
- Keep symlink traversal, Git readers, and code-symbol extraction out of this
  slice.

Acceptance gate:

- Filesystem documents can be discovered and read through the `SourceReader`
  port.
- File reads cannot escape the configured root.
- Discovery is deterministic and does not change public v1 contracts.

Shipped slice:

- Added `FilesystemSourceReader` in `engram-ingest`.
- Discovers supported text, Markdown, structured-data, and code files in sorted
  relative-path order.
- Reads UTF-8 document content while rejecting path traversal, absolute paths,
  symlinks, and oversized files.

## Phase 16: Git Source Reader

Status: done for local tracked-file discovery.

Goal: discover tracked files from a local Git worktree as source-grounded
knowledge inputs without cloning remotes, reading history, or adding Git details
to portable contracts.

Crates:

- `engram-ingest`

Implementation work:

- Implement a Git worktree `SourceReader`.
- Use tracked Git paths for discovery.
- Preserve relative paths, content hashes, source policy, and provenance.
- Reject untracked paths, traversal, absolute paths, and oversized reads.
- Keep remote clone, history, diffs, and symbol extraction out of this slice.

Acceptance gate:

- Git documents can be discovered and read through the `SourceReader` port.
- Only tracked supported files are discovered.
- Reads cannot escape the worktree root and do not change public v1 contracts.

Shipped slice:

- Added `GitSourceReader` in `engram-ingest`.
- Uses `git ls-files` to discover tracked supported files in sorted path order.
- Reuses filesystem-safe reads to reject untracked paths, traversal, absolute
  paths, symlinks, oversized files, and non-UTF-8 content.

## Phase 17: Code Symbol Chunker

Status: done for deterministic declaration chunking.

Goal: split source-code documents into deterministic symbol-oriented chunks
without adding parser dependencies or changing public contracts.

Crates:

- `engram-ingest`

Implementation work:

- Implement a `CodeSymbolChunker`.
- Recognize common declaration lines for Rust, TypeScript/JavaScript, Python,
  Go, and JVM/C-like languages.
- Preserve symbol anchors and line ranges in `SourceLocation`.
- Fall back to a file chunk when no declaration is recognized.
- Keep AST parsing, symbol graphs, and relationship extraction out of this
  slice.

Acceptance gate:

- Code-symbol chunks preserve source line ranges and anchors.
- No-symbol files do not disappear from ingestion.
- The chunker composes with `KnowledgeIngestor` and does not change public v1
  contracts.

Shipped slice:

- Added `CodeSymbolChunker` in `engram-ingest`.
- Recognizes common declaration forms for Rust, TypeScript/JavaScript, Python,
  Go, and JVM/C-like languages.
- Emits `CodeSymbol` chunks with anchors and line ranges, with a file-level
  fallback when no declaration is recognized.

## Phase 18: Hybrid Retrieval Fusion

Status: complete for deterministic weighted fusion. Advanced rerankers and
service wiring remain future work.

Goal: merge candidate results from multiple retrieval sources with
deterministic scoring, duplicate collapse, and explainable fusion traces.

Crates:

- `engram-retrieval`

Implementation work:

- Implement `RetrievalFusion` for a weighted deterministic fusion strategy.
- Preserve candidate policy, provenance, content, and explanations.
- Collapse duplicate targets by type and ID.
- Populate `FusionTrace` with strategy, scores, rank, and deduplication
  evidence.
- Keep learned rerankers, vector calls, and service wiring out of this slice.

Acceptance gate:

- Hybrid fusion ranks by deterministic weighted score.
- Duplicate candidates collapse without hiding trace evidence.
- Public v1 retrieval contracts do not change.

Shipped slice:

- Added `engram-retrieval` as a focused crate for retrieval collaborators.
- Added `WeightedRetrievalFusion` over existing `RetrievalResult` candidates.
- Added source weights, duplicate collapse, request-limit handling, and
  `FusionTrace` evidence without store, vector, embedding, model, runtime, or
  TypeScript dependencies.

## Stop Conditions

Do not move to a later phase when any of these are true:

- The accepted contract and Rust serialization disagree.
- Generated TypeScript contracts drift.
- A behavior slice has no executable spec or fixture.
- Policy checks are missing from write, retrieve, forget, ingest, or
  consolidation paths.
- A concrete adapter leaks storage-specific fields into the portable domain
  contract.
- TypeScript reimplements behavior that belongs in Rust core.
- Evaluation cannot distinguish quality failures from policy failures.

## Near-Term Queue

The next implementation loop should be:

1. Extract the current write and retrieval tests into reusable fixture-runner
   utilities for future SQL and native bindings.
2. Add accepted retrieval fixture files for positive recall, forbidden recall,
   budget omission, and no-result behavior.
3. Add forget-memory behavior for archive, tombstone, redaction, and hard
   delete policy outcomes.
4. Add evaluation report generation over the executable fixture set.
5. Start SQL adapter design from `docs/adr/0005-storage-adapter-semantics.md`.
6. Update docs and run the full validation suite.
