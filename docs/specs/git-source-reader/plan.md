# Plan: Git Source Reader

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

> **Plan contract:** this is the implementation strategy. Unlike the spec, this
> document is allowed to change as you learn. When it changes substantially
> (a different approach, not just a re-ordering), note why in the changelog at
> the bottom.

## Approach

Add a Git worktree source reader to `engram-ingest` that invokes the local Git
CLI for tracked-file discovery. Keep reads rooted in the worktree and reuse the
filesystem reader's path validation and file classification helpers.

Tempted to add a Rust Git library; declining because tracked-file discovery is
enough for this slice and a dependency would expand the adapter surface.
Tempted to inspect commit history; declining because historical provenance
needs separate contract and fixture work.

## Constraints

- `engram-core` remains trait-only.
- No public v1 schema changes.
- No remote clone, network, branch, tag, or diff behavior.

## Construction tests

**Integration tests:** Git reader tests for tracked-only discovery, UTF-8 reads,
untracked rejection, and path traversal rejection.

**Manual verification:** none.

## Design (LLD)

### Interfaces & contracts

Implement `engram_core::SourceReader` for `GitSourceReader`. The reader accepts
a local repository root and builds `SourceDocument` values from tracked paths.

### Component / module decomposition

- `git.rs` owns Git CLI invocation and tracked path filtering.
- `filesystem.rs` exposes crate-local path safety and classification helpers.
- `lib.rs` re-exports only public reader/options types.

### Failure, edge cases & resilience

Git command failures are adapter errors. Untracked, absolute, parent-relative,
oversized, and non-UTF-8 reads fail before content is returned.

## Tasks

### T1: Git tracked-file SourceReader implementation

**Depends on:** PHASE15 filesystem source reader.

**Tests:**
- Tracked supported files are discovered in sorted order.
- Untracked supported files are not discovered and cannot be read.
- Traversal paths are rejected.
- Read document returns UTF-8 text.

**Approach:**
- Add `git.rs` in `adapters/ingest/src`.
- Reuse filesystem helper functions for path and file behavior.
- Add integration tests under `adapters/ingest/tests`.

**Done when:** Git reader tests and full repository gates pass.

## Rollout

Library code only. Remote repositories, history, diffs, and symbol extraction
remain future slices.

## Risks

- Git command behavior can vary by environment; tests use only `git init` and
  `git ls-files`, avoiding user identity and commits.

## Changelog

- 2026-06-30: initial plan for Git tracked-file discovery.
- 2026-06-30: shipped `GitSourceReader` in `engram-ingest`.
