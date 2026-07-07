# Plan: Open Source Governance

## Scope

Add the missing top-level governance document and record the phase in roadmap
status artifacts.

## Steps

- [x] Add `GOVERNANCE.md` with maintainer decision rules.
- [x] Cover spec, ADR, RFC, contract, release, dispute, security, and conduct
  routing.
- [x] Update roadmap/changelog/phase status.
- [x] Run documentation and diff validation.

## Validation

```bash
.codex/hooks/check-docs.sh
git diff --check
test -f GOVERNANCE.md
```
