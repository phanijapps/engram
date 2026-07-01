# Spec: Demo UI polish + durable shared state (demo Slice 4)

- **Status:** Shipped
- **Owner:** phanijapps
- **Plan:** [`plan.md`](plan.md)
- **Constrained by:** RFC-0003, ADR-0006
- **Contract:** none
- **Shape:** mixed

## Objective
Make the demo coherent and durable: memory, knowledge, and ingest engines share
one SQLite file so writes persist across restarts and a graph extracted by ingest
is visible to the knowledge engine. Document the full four-panel demo. (Vectors
for semantic search stay in-memory; full `engram-retrieval` multi-source fusion
remains deferred.)

## Boundaries
### Always do
- Keep engines behind their ports; the shared file is a backend composition
  detail (each adapter still owns only its tables).
- Leave the retrieval engine in-memory (vectors re-indexed each session).
### Ask first
- Adding an optional `path` to the memory/knowledge/ingest engine constructors
  (a binding-surface change; pre-authorized by RFC-0003 Slice 4).
### Never do
- Change v1 contracts or couple adapters (the forbidden-import gate still holds).

## Testing Strategy
- **Goal-based / integration:** durability + cross-engine sharing smoke (write
  with one engine, read with a fresh engine on the same file; ingest→knowledge
  neighbor visibility). Plus the full workspace gate suite.

## Acceptance Criteria
- [x] Memory/knowledge/ingest engines accept an optional file path; the backend
  opens one shared `demo-engram.db` when `ENGRAM_DB` is set.
- [x] Memory written by one engine is read by a fresh engine on the same file.
- [x] A graph extracted by the ingest engine is readable by the knowledge engine.
- [x] `demo/README.md` documents all four panels + durability.
- [x] Default + `--features fastembed` clippy, workspace tests, `pnpm
  typecheck/test`, isolation gate, and contract/docs hooks pass.

## Assumptions
- Technical: multiple `rusqlite` connections to one file see committed writes
  (rollback journal; demo is single-user) (source: durability smoke 2026-06-30).
