# Phase 05 Spec: Evaluation Harness

## Status

Done for the Rust fixture-runner baseline.

## Scope

Run portable evaluation fixtures against a memory service and report actionable
case failures.

## Acceptance

- Fixture setup writes memories through normal service behavior.
- Each case executes retrieval and checks required/forbidden targets.
- Missing explanations, policy leaks, and max-result violations are distinct
  failure messages.
