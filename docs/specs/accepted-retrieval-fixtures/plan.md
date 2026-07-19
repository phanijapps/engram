# Plan: Accepted Retrieval Fixtures

## Scope

Add accepted retrieval fixture JSON examples and runner coverage without
changing contract schemas or adapter behavior.

## Steps

- [x] Add accepted positive recall, forbidden recall, budget omission, and
  no-result fixture files.
- [x] Add `engram-eval` test coverage that runs each fixture.
- [x] Update roadmap/changelog/phase status.
- [x] Run fixture, contract, docs, and diff validation.

## Validation

```bash
cargo fmt --all --check
cargo test -p engram-eval
pnpm run contracts:generate
.codex/hooks/check-contracts.sh
.codex/hooks/check-docs.sh
git diff --check
```
