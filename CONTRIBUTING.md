# Contributing

Engram is contract-first. Public behavior starts in `contracts/v1/` and
`specs/v1/`, then implementation follows.

## Development Setup

Requirements:

- Rust stable with edition 2024 support.
- Node.js 22 or newer.
- pnpm 10.
- Python 3.
- Python dev dependencies from `requirements-dev.txt`.

```bash
python3 -m pip install -r requirements-dev.txt
pnpm install
pnpm run contracts:generate
pnpm run typecheck
cargo check --workspace
.codex/hooks/check-contracts.sh
.codex/hooks/check-docs.sh
```

Optional local Git hook:

```bash
git config core.hooksPath .githooks
```

## Contribution Flow

1. Open an issue or discussion for non-trivial contract changes.
2. Update `docs/domain-data-model.md` when changing accepted semantics.
3. Update `contracts/v1/` for accepted wire contracts.
4. Update `specs/v1/` before implementation behavior.
5. Add valid and invalid examples for contract changes.
6. Run validation before opening a pull request.

See `GOVERNANCE.md` for maintainer decision rules and conflict resolution.
Release candidates must satisfy `docs/release-checklist.md`.

## Contract Rules

- Do not break v1 in place.
- Do not add storage, provider, gateway, or language-specific details to the
  portable contract.
- Do not hand-write TypeScript models that diverge from accepted schemas.
- Keep identifiers opaque.
- Keep policy and provenance explicit.

Breaking changes require a future versioned contract package and an ADR.
