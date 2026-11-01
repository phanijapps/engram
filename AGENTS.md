# Repository Instructions

Engram is a contract-first agentic memory engine (Rust core, TS SDK, MCP
surface). **All project knowledge, law, deviations, and registries live in
ONE place: `docs/constitution/`** — start at `docs/constitution/00-INDEX.md`.

## The one rule before working here

Read the pack that owns your change before writing:

| Touching... | Read first |
|---|---|
| behavior, scope, product surfaces | `docs/constitution/product/` |
| structure, seams, dependencies | `docs/constitution/architecture/` |
| toolchain, builds, packaging | `docs/constitution/dev/` |
| tests, fixtures, proof | `docs/constitution/testing/` |
| CI gates | `docs/constitution/ci/` |
| authz, supply chain, audit | `docs/constitution/security/` |
| shipping, versions, publication | `docs/constitution/release/` |

Every pack: `00-INDEX.md` (charter + rule table) · `knowledge.md` ·
`rules/<ID>.md` (one rule per file, evidence-cited, counterexample-named).

## Validation (run before handoff)

```bash
scripts="tools/ci"
python3 $scripts/check_constitution.py && python3 $scripts/check_contracts.py
python3 $scripts/check_engine_neutrality.py && python3 $scripts/check_surface_parity.py
python3 $scripts/check_docs.py && python3 $scripts/check_job_artifacts.py
cargo fmt --all && cargo check --workspace
pnpm run contracts:generate && pnpm run typecheck && pnpm run test
python3 $scripts/ste_lint.py   # ASD-STE100 style, advisory
```

Pre-implementation gate: `check_contracts.py && check_engine_neutrality.py`
must pass before any implementation work.

## Factory

SDD work runs through engram-factory (`.agents/skills/engram-factory/`):
brief (engram-frame) → spec → plan → tasks (TDD) → build → review → owner
approval → merge with L3 evidence. Job receipts live under `runs/`.

## Index mining

Before authoring analysis/plan artifacts, run
`python3 tools/hooks/index-mining.py` and cite actual index results
(or state "index empty for <query>").

## Constitution changes

Rules change only via owner ratification; amendments preserve rule IDs and
update gates in the same change. Use the `engram-constitution` skill to
draft. Deviations are owner-approved and time-boxed (`DEVIATIONS.md`,
`DECAY-WINDOWS.md`).
