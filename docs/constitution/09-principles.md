# Rail 09 — Principles (tie-breakers)

Rule classes: values rail — cross-class (RULE-1, RULE-4 flavored).
Owner: core maintainers. Origin: absorbed from the retired `docs/CHARTER.md`
(retired 2026-10-31 after scope drift; the principles below are the surviving,
corrected core). Principles resolve ties when reasonable people disagree; they
are constitution rules like any other — with enforcement stated honestly.

## Rules

- **PR-001 (MUST) — Contract first.** The domain model and its JSON contracts
  are the source of truth; Rust types and generated TypeScript conform to them,
  never the reverse. `docs/architecture/domain-data-model.md` outranks a
  convenient implementation. Generated artifacts MUST be reproducible from
  source and MUST NOT be hand-edited.
  Gate: `tools/ci/check_contracts.py` + `tools/contracts/validate_contracts.py`
  (existing, blocking) + the CI generated-drift diff.
- **PR-002 (MUST) — Rust owns the deterministic core; TypeScript owns
  ergonomics.** Behavior that must be reproducible lives in Rust behind traits;
  integration glue and any LLM client live in TypeScript. Neither re-implements
  the other. LLM/model SDK crates MUST NOT appear in any `core/*` or
  `adapters/*` Cargo manifest.
  Gate: review today; dependency-direction lint is the planned month-1 gap
  (candidate: fold into `check_production_completeness.py`).
- **PR-003 (MUST) — Small crates, explicit responsibilities.** No god modules
  or god packages; a file mixing construction, validation, state, scoring, and
  persistence is split before handoff. Crate roots are facades; behavior lives
  in focused modules.
  Gate: `tools/ci/check_architecture_guards.py` via `check_docs.py`
  (existing, blocking — crate-root line caps and module responsibilities).
- **PR-004 (MUST) — Infrastructure lives behind adapters.** Storage, vectors,
  embeddings, and models sit behind traits so tests use deterministic stubs and
  engines are swappable cells on the capability × engine grid (ADR-0022);
  pgvector is a shipped engine, not an exception.
  Gate: `tools/ci/check_engine_neutrality.py` (existing, blocking) +
  `docs/contracts/constitution/v1/implementation/registry.json` drift gate
  (planned).
- **PR-005 (MUST) — Policy is visible on every path.** Scope, retention, and
  allowed-uses checks appear on write, retrieve, ingest, consolidate, and forget
  — never hidden inside a generic manager.
  Gate: review + conformance suites today; a policy-presence lint over the five
  paths is a candidate month-1 addition.
- **PR-006 (SHOULD) — Measured over intuited.** Scheduling and performance
  decisions (embedding generation timing, reindex batching per ADR-0024, fusion
  weighting) follow benchmarks and fixtures, not intuition. The original
  charter's "lazy embeddings" phrasing is retired with it: the principle is the
  discipline, not any single mechanism.

## Deviation path

Same as every rail: fix first; otherwise a scoped, owner-approved, expiring
DEVIATIONS entry while RULE-5 stands. A principle that has stopped resolving
ties — ignored, or contradicting another unacknowledged — is raised to the
owner, not silently edited.

## History

- 2026-10-31: rail created, absorbing the surviving six principles from the
  retired charter (scope drift documented in docs/README.md Housekeeping).
