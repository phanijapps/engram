# Plan: Forget Mode Contract Examples

## Scope

Add mode-specific forget request/result examples and validation coverage without
changing runtime behavior or public schemas.

## Steps

- [x] Add delete, redact, and archive request examples.
- [x] Add delete, redact, and archive result examples.
- [x] Extend Python contract validation to schema-check the examples.
- [x] Add Rust domain deserialization coverage for accepted forget examples.
- [x] Update roadmap/changelog/phase status.
- [x] Run contract, Rust, docs, and diff validation.

## Validation

```bash
cargo fmt --all --check
cargo test -p engram-domain --test schema_conformance
.codex/hooks/check-contracts.sh
.codex/hooks/check-docs.sh
git diff --check
```
