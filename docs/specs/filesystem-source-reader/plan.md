# Plan: Filesystem Source Reader

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog at
> the bottom.

## Approach

Add a focused filesystem reader module to `engram-ingest` that implements the
existing `SourceReader` trait. It discovers supported files, computes content
hashes, and reads UTF-8 document content. It does not persist chunks; callers
can pass the returned documents into the existing deterministic ingestor.

Tempted to add Git support in the same slice; declining because repository
state, ignored files, and revisions need separate acceptance criteria. Tempted
to add symbol parsing; declining because code-aware chunks need language-specific
boundaries and tests.

## Constraints

- `engram-core` keeps only the port.
- `engram-ingest` owns filesystem-specific access.
- No v1 JSON schema changes.
- No symlink traversal in the first slice.

## Construction tests

**Integration tests:** filesystem reader tests for stable discovery, metadata,
UTF-8 reads, path traversal rejection, and max-size rejection.

**Manual verification:** none.

## Design (LLD)

### Interfaces & contracts

Implement `engram_core::SourceReader` for `FilesystemSourceReader`. The reader
accepts a configured root path and uses `KnowledgeSource` policy/provenance when
building `SourceDocument` records.

### Component / module decomposition

- `filesystem.rs` owns root validation, discovery, file classification, and
  document reads.
- `lib.rs` re-exports only the public reader and options.
- Tests own temporary filesystem fixtures.

### Failure, edge cases & resilience

Absolute paths, `..` components, oversized files, non-UTF-8 content, and
non-files are rejected or skipped according to the spec. Symlinks are skipped.

## Tasks

### T1: Filesystem SourceReader implementation

**Depends on:** PHASE08 deterministic ingestion baseline.

**Tests:**
- Supported files are discovered in sorted relative path order.
- Source document metadata includes content hash and source policy/provenance.
- Read document returns UTF-8 text.
- Path traversal and oversized reads fail.

**Approach:**
- Add `filesystem.rs` in `crates/engram-ingest/src`.
- Implement path normalization and extension-based classification.
- Add integration tests under `crates/engram-ingest/tests`.

**Done when:** filesystem reader tests and full repository gates pass.

## Rollout

Library code only. Git readers, code-symbol extraction, and SQL persistence stay
future slices.

## Risks

- Recursive file discovery can accidentally read too much; keep a max-file-size
  guard and skip unsupported file types.
- Relative path handling is security-sensitive; reject absolute and parent
  components before any read.

## Changelog

- 2026-06-30: initial plan for filesystem source discovery.
- 2026-06-30: shipped `FilesystemSourceReader` in `engram-ingest`.
