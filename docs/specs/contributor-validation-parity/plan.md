# Plan: Contributor Validation Parity

## Scope

Update contributor-facing documentation so local setup instructions match the
gates already required in PR and release workflow docs.

## Steps

- [x] Update `README.md` validation commands.
- [x] Update `CONTRIBUTING.md` development setup commands.
- [x] Update roadmap/changelog/phase status.
- [x] Run documentation and diff validation.

## Validation

```bash
.codex/hooks/check-docs.sh
git diff --check
```
