---
schema_version: "1.0"
artifact_type: "verification"
job_id: "{{job_id}}"
cycle: {{cycle}}
status: "draft"
author_role: "engram-tasks"
profiles: {{profiles_json}}
source_snapshot: null
input_artifacts: []
updated_at: "{{timestamp}}"
---

# Verification: {{feature_title}}

<!--
  Verification v3 (2026-11-01): the evidence ledger. The SOLE mutable result
  record — tasks.md links here, never duplicates. Report per-AC, per-GATE,
  with raw output. NOT_RUN is a pre-execution state; PASS requires pasted
  evidence; an authorized exception is not PASS.
-->

## Acceptance verification (mandatory)

One row per AC from the spec's Gherkin blocks — verdict follows the
scenarios, negative case included or justified:

| AC | Scenario result (happy / negative) | GATE | Evidence below |
|---|---|---|---|
| AC-001 | missing-path-fails PASS / present-path-clean PASS | GATE-001 | `#gates` row |

## Gates (mandatory — raw evidence, per run)

| Gate ID | Command | Result | Raw output (kept, incl. failures) |
|---|---|---|---|
| GATE-001 | `python3 tools/ci/tests/test_engine_neutrality.py` | PASS | OK — 2 cases |
| GATE-002 | `python3 tools/ci/check_engine_neutrality.py` | PASS | gated surface clean |

Failed and superseded runs stay recorded — a wall of green with no failures
anywhere is a smell, not a virtue.

## NFR evidence (mandatory-when-NFRs-declared)

- NFR-001 (reliability): proven by GATE-001 missing-path case — see row above.
- Or `no NFRs declared in spec`.

## Negative evidence (mandatory — never silently absent)

<What was tried that correctly failed: denial paths, rollback, boundary
inputs>. Or `none applies: <reason>`.

## Execution environment (one line, only if it changes interpretation)

<OS/toolchain/versions>. Or `standard CI runner`.

## Recording rules (mandatory)

Raw evidence stays HERE; other artifacts link to it. A PASS without pasted
output is a finding. Profile evidence: {{profiles_json}}-driven sections
appended below by the controller.
