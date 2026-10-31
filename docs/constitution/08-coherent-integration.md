# Rail 08 — Coherent integration (no orphans)

Rule class: RULE-6 (coherent integration / no orphans).
Gate: `tools/ci/check_coherent_integration.py` (planned, month 1).
Registry: `docs/contracts/docs/constitution/v1/components/registry.json`.

## Rules

- **OR-001 (MUST)**: Every named component — Rust workspace crate, TS package, transport
  binding, gate script, registry — MUST be declared in the component registry, and every
  declaration MUST resolve to an existing path (bidirectional bijection).
- **OR-002 (MUST)**: A component that exists on disk but is referenced by no registry is
  an orphan and a violation; a registry entry pointing at nothing on disk is a dangling
  declaration and equally a violation.
- **OR-003 (MUST)**: Documentation that declares repository shape (AGENTS.md tree
  diagrams) MUST agree with the registry inventory; drift between them is a violation of
  both this rail and documentation truth (tracked in DECAY-WINDOWS at adoption).
- **OR-004 (SHOULD)**: Factory job directories SHOULD contain only the canonical
  artifact set at top level; orchestration receipts (dispatch results, batch outputs,
  recovery files, rerun churn) MUST live under `runs/` subdirectories (see job-layout
  policy in the engram-factory skill; enforced by `check_job_artifacts.py` layout check).

## Enforcement boundary

Component registry seeded at adoption with the 35 workspace crates + 6 TS packages +
transports + gates. The bidirectional gate (planned) diffs registry ↔ disk. The
job-layout check ships now with `check_job_artifacts.py` (advisory on historical jobs —
see DECAY-WINDOWS for the job-010 sprawl window).

## Deviation path

None: an orphan is deleted or declared in the same change that introduces it.

## History

- 2026-10-31: rail locked at adoption; registry seeded.
