# Local Hooks

These hooks keep Engram contract-first while implementation is still forming.

## Commands

- `.codex/hooks/check-contracts.sh`: verifies required contract files, required domain-model sections, and JSON schema syntax when `jq` is installed.
- `tools/scripts/validate_contracts.py`: validates accepted v1 examples and invalid
  negative fixtures against the v1 schema package.
- `.codex/hooks/check-code-docs.sh`: reviews Rust and TypeScript code documentation and builds rustdoc with warnings denied.
- `.codex/hooks/check-docs.sh`: rejects unresolved placeholder markers in docs and skills, validates local Codex skills, then runs code documentation checks.
- `.codex/hooks/check-research-parity-docs.sh`: scans architecture and research docs against the research parity drift registry so obsolete implementation claims and missing supersession markers fail locally.
- `.codex/hooks/check-knowledge-inmem-retired.sh`: verifies the retired dedicated knowledge in-memory adapter has not re-entered active manifests, code, or current docs.
- `.codex/hooks/check-memory-inmem-retired.sh`: verifies the retired broad memory in-memory adapter has not re-entered active manifests, code, or current docs.
- `.codex/hooks/check-knowledge-sqlite-isolation.sh`: verifies the SQLite knowledge adapter stays independent from sibling store adapters.
- `.codex/hooks/pre-implementation-check.sh`: runs contract checks and blocks runtime manifests until `docs/adr/0003-implementation-stack.md` exists.

## Install Git Hook

Run:

```bash
git config core.hooksPath .githooks
```

The pre-commit hook runs contract and documentation checks.
