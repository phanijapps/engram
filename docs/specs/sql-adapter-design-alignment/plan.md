# Plan: SQL Adapter Design Alignment

## Scope

Document current SQL adapter design and clean up stale roadmap/README wording.
Do not change SQL implementation.

## Steps

- [x] Add `docs/sql-adapter-design.md`.
- [x] Update `crates/engram-store-sql/README.md`.
- [x] Update implementation roadmap/changelog/phase status.
- [x] Run documentation and diff validation.

## Validation

```bash
.codex/hooks/check-docs.sh
git diff --check
```
