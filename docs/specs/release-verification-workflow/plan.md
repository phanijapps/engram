# Plan: Release Verification Workflow

## Scope

Add a manual release verification workflow that runs release gates without
publishing artifacts.

## Steps

- [x] Add `.github/workflows/release-verify.yml`.
- [x] Mirror release checklist Rust, contract, docs, TypeScript, and vector
  feature gates.
- [x] Update `docs/release-checklist.md`.
- [x] Update roadmap/changelog/phase status.
- [x] Run documentation and diff validation.

## Validation

```bash
.codex/hooks/check-docs.sh
git diff --check
```
