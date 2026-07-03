# Plan: Local Runtime Examples

## Scope

Add checked usage examples for local Rust adapters and the TypeScript client
facade. Keep examples contract-backed and focused on write/retrieve flows.

## Steps

- [x] Add the retired memory in-memory adapter (see `docs/specs/retire-memory-inmem/spec.md`).
- [x] Add `adapters/memory/sqlite/examples/sql_memory.rs`.
- [x] Add `packages/client/examples/injected-transport.ts`.
- [x] Include client examples in TypeScript typechecking.
- [x] Add a client test that executes the injected-transport example.
- [x] Update `examples/README.md` with commands and boundaries.
- [x] Update roadmap/changelog/phase status.
- [x] Run Rust, TypeScript, contract, docs, and diff validation.

## Validation

```bash
cargo check -p engram-store-memory --examples
cargo check -p engram-store-sql --examples
cargo check --workspace
cargo test --workspace
cargo check -p engram-store-vector --features fastembed-tests --tests
pnpm run contracts:check-generated
pnpm run typecheck
pnpm run test
pnpm run build
python3 tools/scripts/validate_contracts.py
.codex/hooks/check-contracts.sh
.codex/hooks/check-docs.sh
git diff --check
```
