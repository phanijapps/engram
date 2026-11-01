---
schema_version: "1.0"
artifact_type: "decision"
job_id: "{{job_id}}"
cycle: {{cycle}}
status: "draft"
author_role: "engram-approver"
profiles: {{profiles_json}}
source_snapshot: "{{snapshot_path}}"
input_artifacts: []
updated_at: "{{timestamp}}"
recommendation: "ESCALATE"
---

# Decision: {{feature_title}}

<!--
  Decision v3: the human gate artifact — answer-first, one screen. The owner
  reads this ONCE and either authorizes or rejects; every claim links its
  evidence. recommendation: APPROVE|ESCALATE per the controller contract.
-->

**Recommendation**: {{recommendation}} · snapshot `{{snapshot_path}}`

## What you authorize

{{authorize}}

## What was verified — and by whom

{{verified}}

## What was NOT verified

{{not_verified}}

## What you accept

{{accepted_risks}}

## What this does not authorize

<Explicit exclusions carried from the spec's out-list — or `nothing beyond
the merged diff`>

## Why

{{rationale}}
