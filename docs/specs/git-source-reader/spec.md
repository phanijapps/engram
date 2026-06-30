# Spec: Git Source Reader

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** ADR-0003, ADR-0004
- **Brief:** none
- **Contract:** none
- **Shape:** integration

> **Spec contract:** this document defines what "done" means. The implementing
> PR must match this spec, or update it. Verification must be derivable from it.

## Objective

Engram can discover tracked text, Markdown, structured-data, and code documents
from a local Git worktree through the `SourceReader` port without reading
untracked files, shelling out from core crates, or changing public domain
contracts.

## Boundaries

### Always do

- Keep Git CLI integration inside `engram-ingest`.
- Use tracked Git paths as discovery input.
- Reuse filesystem path safety for reads.
- Preserve source policy and provenance on returned documents.

### Ask first

- Clone remote repositories.
- Read historical revisions, diffs, branches, or tags.
- Add libgit2/gitoxide dependencies.
- Parse code symbols.

### Never do

- Read untracked files as Git knowledge.
- Let document paths escape the worktree root.
- Add Git concepts to v1 portable schemas.
- Treat Git documents as memory records.

## Testing Strategy

- TDD: integration tests create a temporary Git repository, add tracked files,
  and assert deterministic discovery.
- TDD: untracked supported files and traversal paths are rejected.
- Goal-based: full repository gates prove no contract drift.

## Acceptance Criteria

- [x] A Git source reader discovers only tracked supported files in sorted path
  order.
- [x] Discovered documents include source ID, kind, relative path, content hash,
  policy, provenance, and optional version.
- [x] Reading a tracked document returns UTF-8 text.
- [x] Untracked paths, absolute paths, and traversal paths are rejected.
- [x] No public v1 schema changes are introduced.

## Assumptions

- Technical: local `git` CLI is available for tests (source: `git --version`
  returned `git version 2.51.0`).
- Technical: Git source reading belongs behind `SourceReader` (source:
  `crates/engram-core/src/lib.rs`).
- Process: public contracts do not change for this adapter slice (source:
  ADR-0004).
