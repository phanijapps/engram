# Plan: Memory Contract Fixture Runners

## Scope

Extract portable accepted write and retrieval fixture execution into
`engram-eval` while keeping store-specific assertions in their owning test
crates.

## Steps

- [x] Add accepted example loaders in a focused `engram-eval` module.
- [x] Add a focused `MemoryContractRunner` over `MemoryService`.
- [x] Migrate in-memory write and retrieval fixture tests to the shared runner.
- [x] Migrate SQL accepted write/retrieval service tests to the shared runner.
- [x] Update roadmap/changelog/phase status.
- [x] Run targeted tests, docs checks, and diff validation.

## Validation

```bash
cargo fmt --all --check
cargo test -p engram-eval
cargo test -p engram-store-memory --test write_memory_fixtures --test retrieve_context_fixtures
cargo test -p engram-store-sql --test service
.codex/hooks/check-docs.sh
git diff --check
```
