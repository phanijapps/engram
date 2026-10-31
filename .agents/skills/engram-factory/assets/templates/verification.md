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
  Verification v2 (2026-10-31): the gate-evidence ledger. Every GATE-ID from
  the tasks gate inventory MUST have a row with raw evidence. Acceptance rows
  bind to AC-IDs from spec.md — no orphan gates, no unproven ACs.
-->

## Acceptance verification

| AC ID | GATE IDs | Result | Evidence reference |
|---|---|---|---|
| AC-001 | GATE-001 | PASS | `#gates` row below |

## Gates

| Gate ID | Command | Status | Raw evidence |
|---|---|---|---|
| GATE-001 | `<exact command>` | PASS | exit 0, output summary (first/last lines) |

Every gate from the tasks gate inventory appears above with real output —
NOT_RUN is a finding, not a state to hand to review.

## Negative evidence

- <What was tried that correctly failed (denial, error path, rollback)>.

## Execution environment

- <OS/toolchain/versions — only if it changes interpretation>.

## Recording rules

- Raw evidence stays here; summaries reference it. A PASS without pasted
  output is a finding.
