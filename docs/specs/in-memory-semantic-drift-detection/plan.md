# Plan: In-Memory Semantic Drift Detection

## Scope

Implement deterministic semantic-drift detection for in-memory time-window
consolidation using explicit memory assertions only.

## Steps

- [x] Add a focused `semantic_drift` consolidation module.
- [x] Wire `ConsolidationTaskKind::SemanticDriftDetection` in
  `InMemoryConsolidationExecutor`.
- [x] Detect scoped active assertion changes by subject, predicate, object, and
  effective timestamp.
- [x] Write temporal contradiction records and contradiction-detected events.
- [x] Preserve idempotency for existing open assertion-pair drift records.
- [x] Add consolidation tests for detection, scope filtering, skipping, and
  idempotency.
- [x] Update roadmap/changelog/phase status.
- [x] Run Rust, TypeScript, contract, docs, and diff validation.

## Validation

```bash
cargo fmt --all --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo check -p engram-store-vector --features fastembed-tests --tests
cargo clippy -p engram-store-vector --features fastembed-tests --tests -- -D warnings
pnpm run contracts:check-generated
pnpm run typecheck
pnpm run test
pnpm run build
python3 scripts/validate_contracts.py
.codex/hooks/check-contracts.sh
.codex/hooks/pre-implementation-check.sh
.codex/hooks/check-code-docs.sh
.codex/hooks/check-docs.sh
git diff --check
```
