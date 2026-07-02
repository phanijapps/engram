# Plan: Workspace Architecture Alignment

- **Spec:** [`spec.md`](spec.md)
- **Status:** Done

## Approach

Make low-risk repository-organization changes that remove stale guidance without
renaming crates or moving historical specs. The current root/code layout already
matches the architecture at the top level (`core`, `adapters`, `bindings`,
`packages`, `contracts`, `demo`); the immediate problem is stale documentation
and tooling scattered between root and `scripts/`.

## Tasks

### T1: Refresh architecture authority

**Touches:** `docs/architecture.md`, `docs/architecture/overview.md`

**Tests:** `.codex/hooks/check-docs.sh`

**Done when:** older links still resolve, but contributors are directed to
`docs/architecture/reference.md` for normative rules and `overview.md` for the
current map.

### T2: Refresh planning map

**Touches:** `.codex/skills/engram-plan/references/crate-map.md`

**Tests:** `.codex/hooks/check-docs.sh`

**Done when:** the plan skill no longer recommends retired in-memory adapters or
pre-split behavior ownership.

### T3: Consolidate shared scripts

**Touches:** `tools/scripts/**`, `packages/contracts/package.json`,
`.codex/hooks/check-contracts.sh`, current contributor/release/contract docs

**Tests:**
- `pnpm run contracts:generate`
- `.codex/hooks/check-contracts.sh`

**Done when:** reusable Python automation is under `tools/scripts/` and all live
commands use that path.

### T4: Validate

**Tests:**
- `cargo fmt --all --check`
- `cargo check --workspace`
- `pnpm run contracts:generate`
- `pnpm run typecheck`
- `.codex/hooks/check-contracts.sh`
- `.codex/hooks/check-docs.sh`

**Done when:** gates pass and no generated TypeScript contract diff appears.
