---
schema_version: 2
artifact: "plan"
job_id: "{{job_id}}"
cycle: {{cycle}}
status: "draft"
author_role: "engram-planner"
profiles: {{profiles_json}}
source_snapshot: null
input_artifacts: []
updated_at: "FILL: ISO-8601 with timezone"
size_tier: "S"
spec_ref: null
constitution_rules_touched: []
---
<!--
  Plan template v2 (constitution adoption 2026-10-31).
  v2 changes: every step carries REQ IDs + constitution rule IDs + concrete
  paths + verification + rollback — no orphan steps, no orphan REQs; blast
  radius must be paths/counts, not adjectives. Approach links DEC-* without
  restating REQ prose. Verified by tools/ci/check_job_artifacts.py.
-->


# Plan: <short outcome>

## Approach

One concise design narrative. Link consumed `DEC-*`; do not restate `REQ-*` —
reference by ID.

## New decisions

| ID | Authority / rule | Decision | Why now | Consequence |
|---|---|---|---|---|
| DEC-002 | DELEGATED / <rule ID> | <choice> | <new fact> | <effect> |

## Steps

| ID | Implement | REQ IDs | Rule IDs | Concrete paths | Verify (GATE-ID / command) | Rollback |
|---|---|---|---|---|---|---|
| STEP-001 | <imperative action> | REQ-001 | <rule ID> | `src/a.rs`; `new-path: tests/a.rs` | GATE-001 | <exact revert/recovery> |

## Contracts and compatibility

| Surface | Classification | REQ IDs | Consumer / migration / documentation action |
|---|---|---|---|
| public/internal/none | compatible/breaking/none | REQ-001 | <concrete action> |

## Blast radius

| Area or symbol | Concrete dependent paths/count | Risk | Owning STEP IDs |
|---|---|---|---|
| <area> | <paths or measured count> | <failure shape> | STEP-001 |

## Verification map

| AC ID | Step IDs | Gate IDs | Omission / deviation |
|---|---|---|---|
| AC-001 | STEP-001 | GATE-001 | none |

## Rollback and recovery

- STEP-001: <exact rollback command/action and partial-failure state>.
