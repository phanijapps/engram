# Plan: CI Vector Feature Gates

## Scope

Make vector feature validation visible in CI and release workflow documents.

## Steps

- [x] Add FastEmbed feature `cargo check` to the Rust CI job.
- [x] Add FastEmbed feature clippy to the Rust CI job.
- [x] Update the release checklist.
- [x] Update the pull request template.
- [x] Update roadmap/changelog/phase status.
- [x] Run focused CI command validation and docs checks.

## Validation

```bash
cargo check -p engram-store-vector --features fastembed-tests --tests
cargo clippy -p engram-store-vector --features fastembed-tests --tests -- -D warnings
.codex/hooks/check-docs.sh
git diff --check
```
