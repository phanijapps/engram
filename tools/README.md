# tools/ — repository operations and agent hooks

All repository operation scripts are **Python, snake_case, stdlib-only,
cross-platform** (no bash, no node). One responsibility per module; shared
helpers live in `lib/`. The wrapper/implementation split across two trees was
consolidated 2026-10-31 and unified under `tools/` the same day (see
`docs/research/software-constitution/00-sdd-constitution-plan.md` §13–14).

```
tools/
  lib/repo.py          shared helpers: ROOT resolution, git(), fail/ok reporting
  hooks/               agent-session hooks (index-mining, evidence validator)
  ci/                  CI gates (the constitution's gates-root; registry:
                       docs/contracts/constitution/v1/gates/registry.json)
    check_constitution.py          RULE-5 + L1 structural + registries
    check_contracts.py             CI-004/PR-001 required files/sections/JSON
    check_engine_neutrality.py     EN-001 (ADR-0022 rule-1)
    check_surface_parity.py        SP-001/SP-002 (ADR-0022)
    check_implementation_registry.py  RULE-1 canonical implementations
    check_coherent_integration.py  RULE-6 orphans (bidirectional)
    check_production_completeness.py  RULE-3 markers + PR-002 LLM-SDK lint
    check_distribution_drift.py    RULE-2 published-surface reconciliation
    check_canonical_evidence.py    RULE-4 L3 evidence events
    check_job_artifacts.py         PC-002/OR-004 SDD artifacts (--strict)
    check_docs.py                  PC-003 orchestrator — IMPORTS:
      check_architecture_guards.py   PR-003 crate/module guards
      check_ts_native_delegation.py  TS-must-call-native
      check_research_parity_docs.py  doc drift (ci/data/doc-drift-registry.json)
    check_code_docs.py             doc coverage (rust half DW-6 advisory;
                                   CHECK_CODE_DOCS_STRICT=1 enforces)
    check_bound_code_matrix.py     operation×backend×transport matrix
    prepare_acquisition_fixtures.py
    tests/                         unit tests for the checkers
    data/                          checker data (doc-drift registry)
  contracts/           codegen + schema validation
    generate_ts_contracts.py       pnpm contracts:generate source
    validate_contracts.py          schema-level validation (check_contracts)
  dev/                 developer workflows (embedding evals, golden, fixtures;
                       pack_catalogue_check.sh pending port — DW-9)
  release/             release packaging (stamp/verify platform package)
```

Conventions:
- Every gate: `from lib.repo import ROOT, fail, ok` (files live in
  `tools/<area>/`, so `parents[1]` is `tools/` and the import resolves).
- Exit 0/1; name-prefixed messages; `main()` + `if __name__ == "__main__"`
  guard so orchestrators can import them (`check_docs.py` does exactly that).
- Orchestration imports sub-checkers as modules — never subprocess to a peer.
