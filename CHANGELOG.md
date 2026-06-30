# Changelog

This project follows a contract-first changelog while pre-1.0.

## Unreleased

- Accepted the v1 core memory contract.
- Added Rust 2024 workspace scaffolding for domain and core ports.
- Added contract validation, examples, invalid examples, and spec templates.
- Added generated TypeScript contract drift checks.
- Added Rust schema conformance tests for accepted v1 payloads.
- Added the first spec-driven write-memory slice with an in-memory service.
- Added the `engram-store-memory` crate for process-local adapter behavior.
- Moved concrete in-memory write state out of `engram-core`.
- Added memory event query contracts and deterministic clock/ID injection for
  the in-memory write path.
- Added executable write-memory fixture tests against accepted v1 examples.
- Added exact and keyword retrieval behavior to the in-memory adapter.
- Added retrieval tests for scope isolation, policy omission, explanations, and
  budget omissions.
- Documented storage adapter transaction, idempotency, event, and scope
  semantics before SQL implementation.
- Split `engram-store-memory` into focused modules to avoid god-module
  service, retrieval, write, state, scope, dependency, and validation code.
- Added loop-da-loop phase state, phase specs, phase plans, and a roadmap status
  updater.
- Added in-memory forget lifecycle behavior for delete, redact, tombstone, and
  archive modes.
- Marked in-memory retrieve and forget baselines complete in the implementation
  roadmap.
- Added `engram-eval` with a deterministic Rust fixture runner for
  `MemoryService` implementations.
- Added `engram-store-sql` with a SQLite-backed memory/event repository slice.
- Accepted SQLite as the first SQL adapter target in ADR 0006.
- Added SQL-backed `MemoryService` orchestration for write, retrieve, forget,
  and evaluation fixture behavior.
- Added `engram-node`, `@engram/node`, and native client helpers for a narrow
  TypeScript binding surface over Rust memory behavior.
- Added `engram-ingest` with deterministic source/document/chunk ingestion and
  in-memory knowledge repository support.
- Added `engram-store-vector` with SQLite `sqlite-vec` fixed-vector tests and
  an opt-in FastEmbed BGE-small smoke test.
- Added in-memory hierarchy repository and scoped parent-chain navigation.
- Added in-memory belief and contradiction repository support.
- Added a dry-run consolidation service that returns auditable
  `ConsolidationRun` records without durable mutations.
- Added `@engram/adapters` with framework-neutral observed transport utilities.
- Added public governance and release-checklist documentation for pre-1.0
  hardening.
- Scoped documentation checks to tracked repository docs and tracked repository
  skills.
- Added `FilesystemSourceReader` for deterministic local source discovery.
- Added `GitSourceReader` for tracked-file local worktree discovery.
- Added `CodeSymbolChunker` for deterministic declaration-oriented code chunks.
- Added `engram-retrieval` with deterministic weighted hybrid retrieval fusion.
- Added gated mutating consolidation orchestration with pre/post evaluation
  checks.
- Wired in-memory retrieval through injectable deterministic fusion before final
  context truncation.
- Added source-grounded in-memory knowledge chunk retrieval through the shared
  fusion path.
- Added sqlite-vec retrieval candidates through injected query-vector and target
  resolver ports.
- Added exact-text in-memory consolidation compaction with scoped archive
  events and skipped unsupported task reporting.
- Added in-memory policy-expiry decay with legal-hold protection and expired
  lifecycle events.
- Added deterministic in-memory hierarchy base-node construction with
  hierarchy-built events and path navigation coverage.
- Added assertion-backed in-memory belief synthesis with duplicate prevention
  and belief-synthesized lifecycle events.
- Added explicit assertion-pair in-memory contradiction detection with
  duplicate-open-record prevention and contradiction-detected lifecycle events.
- Added in-memory belief retrieval with scoped active belief candidates,
  policy omissions, explanations, and shared retrieval fusion.
- Added hierarchy-mode retrieval context for in-memory memory results with
  scoped path evidence and hierarchical-fit scoring.
- Added hierarchy-mode retrieval expansion from matched memory base nodes to
  scoped sibling memory base nodes with policy-safe omissions and fusion trace
  evidence.
- Added deterministic first-entity aggregate hierarchy construction with
  parent links, memberships, hierarchy-built events, and idempotency coverage.
- Added in-memory retrieval-index composition with external candidate fusion,
  budget omissions, and degraded source-failure reporting.
- Added scoped in-memory contradiction lookup and explicit contradiction
  resolution records without target mutation.
- Added contradiction-aware in-memory belief ranking using explicit open review
  records.
- Added a feature-gated FastEmbed BGE-small query provider for sqlite-vec
  retrieval tests.
- Added checked local runtime examples for in-memory, SQLite, and TypeScript
  client usage.
- Added deterministic in-memory semantic drift detection for time-window
  consolidation.
- Added deterministic member-derived summaries for in-memory hierarchy
  aggregate nodes.
- Added CI and release checklist gates for vector FastEmbed feature compilation.
- Added top-level open-source governance documentation.
- Aligned contributor validation documentation with PR and release gates.
- Added file-backed SQLite construction for the SQL memory service and store.
- Added a manual release verification workflow for release gates.
- Added a local in-memory benchmark smoke example and benchmark documentation.
