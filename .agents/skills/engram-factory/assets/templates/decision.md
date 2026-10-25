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

# Approval recommendation: {{feature_title}}

## Exact decision scope

Source snapshot/hash, base/head and package/export identities: {{source_identity}}
Cycle, round and required-reviewer set: {{review_scope}}
Requested action: source merge to {{target_branch}}
Deployment/promotion authorization is separate: {{promotion_boundary}}

## Gate evidence

| Gate | Result | Evidence / hash | Limitation / unresolved item |
| --- | --- | --- | --- |
| Acceptance and quality criteria covered | {{result}} | {{evidence}} | {{gap_or_none}}
| Required tests executed successfully | {{result}} | {{evidence}} | {{gap_or_none}}
| All required independent reviews PASS | {{result}} | {{review_paths_and_hashes}} | {{gap_or_none}}
| Source/package/configuration identity reconciled | {{result}} | {{evidence}} | {{gap_or_none}}
| Migration, rollout and recovery feasible | {{result}} | {{evidence}} | {{gap_or_none}}
| Organization sign-offs and risk exceptions valid | {{result}} | {{evidence}} | {{gap_or_none}}

## Risk and recommendation

Outstanding findings, residual risks and EXC owners/expiry: {{risks_or_none}}
Contradictory or missing evidence and resolution: {{contradictions_or_none}}
Recommendation APPROVE or ESCALATE, with concrete reasons: {{recommendation_reason}}
Conditions that invalidate this recommendation: source, package, environment,
accepted evidence or target-base changes; {{additional_conditions}}

## Human decision handoff

Present the exact snapshot, review outcome, executed checks, risks and intended merge.
Do not fill in a human approval on the user's behalf. The controller records the
actual user command and actor separately in board.json.
