# Spec: Filesystem Source Reader

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

Engram can discover source-grounded text, Markdown, and code documents from a
local filesystem root and read their UTF-8 content through the `SourceReader`
port without turning those documents into memories or leaking filesystem
details into domain contracts.

## Boundaries

### Always do

- Keep filesystem access inside `engram-ingest`.
- Return `SourceDocument` records with relative paths, content hashes, policy,
  provenance, and deterministic ordering.
- Reject path traversal before reading document content.
- Keep unsupported file kinds out of the first slice.

### Ask first

- Follow symlinks.
- Add Git history, code-symbol parsing, embeddings, or SQL persistence.
- Add watchers, background scans, or recursive runtime services.

### Never do

- Store filesystem paths as canonical domain identity.
- Read outside the configured root.
- Treat local files as agent memories.
- Silently include binary files as text documents.

## Testing Strategy

- TDD: filesystem reader tests create a temporary source tree and assert stable
  discovery order, document metadata, and content hashes.
- TDD: path traversal and oversized reads fail before content is returned.
- Goal-based: repository gates prove the new adapter stays behind existing
  `engram-core` ports and does not change public v1 contracts.

## Acceptance Criteria

- [x] A filesystem reader discovers supported files below a configured root in
  deterministic relative-path order.
- [x] Discovered documents include source ID, kind, relative path, content hash,
  policy, provenance, and created timestamp.
- [x] Reading a discovered document returns UTF-8 text content.
- [x] Path traversal and absolute document paths are rejected.
- [x] Unsupported or hidden files are not silently converted into knowledge
  records.
- [x] No public v1 schema changes are introduced.

## Assumptions

- Technical: source reading belongs behind the `SourceReader` port (source:
  `core/orchestration/src/lib.rs`).
- Technical: filesystem knowledge remains distinct from memory records (source:
  `docs/implementation-roadmap.md` Phase 8).
- Process: public contracts do not change for this adapter slice (source:
  ADR-0004).
