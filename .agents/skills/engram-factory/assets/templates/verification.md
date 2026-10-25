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

<!--
  v2 template. House rules: references/templates.md §Writing rules.
  Executed results live ONLY here — no other artifact restates gate outcomes.
  Negative evidence is mandatory: what was deliberately not verified, and why.
-->

# Verification: {{feature_title}}

| Field | Value |
|---|---|
| Reader / question | QA, reviewers, approver asking "what was actually executed, and what was deliberately not?" |
| Status | cycle {{cycle}} · planned checks start NOT_RUN · executed results recorded once, here |
| Budget | One line per check. |

## Acceptance verification

> Links analysis AC ids — never restates them. Results are recorded only after execution.
> Enforcement: an unexecuted check recorded as PASS is a blocker.

| AC | Command | Expected observable | Result |
|---|---|---|---|
| AC-001 | {{ac_verification}} | one sentence | NOT_RUN |
| AC-00n | `<command>` | one sentence | NOT_RUN |

## Gates

> The repo validation block, verbatim commands. Results land only here.

| Gate | Command | Result |
|---|---|---|
| {{gates}} | `<command>` | NOT_RUN |

## Negative evidence

> REQUIRED. Every deliberate omission: `omitted: <check> — reason — owner — revisit trigger`.
> Enforcement: QA and the approver check every N/A carries a reason and an owner.

{{negative_evidence}}

## Execution environment

> Where the commands ran: machine shape, toolchain, store paths, revision. One line each.

{{environment}}

## Recording rules

> Imperatives. Enforcement: reviewers compare recorded results against raw output.

{{recording_rules}}

*(Write: record command, result, and reference for every executed check · never pre-record a result · a failed check stays FAIL until re-executed · partial runs say which part ran.)*
