# Testing — knowledge

<!-- ASD-STE100 descriptive register: short sentences, one topic per paragraph. -->

## Knowledge — how proof works here

Deterministic fixtures (no network, no LLM): acceptance fixtures per contract
(`docs/contracts/v1/acceptance/`), conformance per backend, adapter tests,
factory evals (48) + frame evals (23). The bound-code matrix
(operation×backend×transport) is fail-closed. Behavior batteries: cargo test
--workspace, pnpm test, eval fixtures. CI gates in `tools/ci/` carry unit
tests (`tools/ci/tests/`). TDD spine: fixtures land RED before GREEN
(tasks Phase 1).
