# Plan: SQLite File-Backed Construction

## Scope

Add durable local SQLite construction paths to `engram-store-sql` without
changing service behavior or accepted contracts.

## Steps

- [x] Add `SqlMemoryStore::open_file`.
- [x] Add `SqlMemoryService::open_file`.
- [x] Add a service test proving persistence across reopen.
- [x] Update roadmap/changelog/phase status.
- [x] Run Rust, contract, docs, and diff validation.

## Validation

```bash
cargo fmt --all --check
cargo check -p engram-store-sql --tests
cargo test -p engram-store-sql
cargo check --workspace
cargo test --workspace
pnpm run contracts:check-generated
.codex/hooks/check-docs.sh
git diff --check
```
