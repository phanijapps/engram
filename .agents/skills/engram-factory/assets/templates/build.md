---
schema_version: "1.0"
artifact_type: "build"
job_id: "{{job_id}}"
cycle: {{cycle}}
status: "draft"
author_role: "engram-builder"
profiles: {{profiles_json}}
source_snapshot: "{{snapshot_path}}"
input_artifacts: []
updated_at: "{{timestamp}}"
---

# Build evidence: {{feature_title}}

## Implementation identity

Cycle and upcoming review round: {{cycle_round}}
Worktree, branch, base and committed head: {{git_identity}}
Source snapshot path/hash and external package digests: {{snapshot_identity}}
Platform environment/export provenance and drift check: {{platform_identity}}
Task, verification and release input hashes: {{artifact_hashes}}

## Delivered behavior

Behavior change and scope: {{delivered_behavior}}
Code, configuration, metadata, infrastructure and data changes: {{change_inventory}}
Deviations from plan, rationale and accepted decision references: {{deviations_or_none}}

## Completion and evidence

| AC / NFR | Completed task IDs | Implementation / native object evidence | Test IDs and run evidence | Result / limitation |
| --- | --- | --- | --- | --- |
| {{criterion}} | {{tasks}} | {{implementation}} | {{tests}} | {{result}}

Checks executed, exact procedures, exit status and run IDs: {{check_summary}}
Required checks not executed and gate consequence: {{unexecuted_checks_or_none}}
Migration/transport rehearsal and reconciliation: {{rehearsal_results}}
Secrets/data scan and clean worktree confirmation: {{hygiene}}

## Finding resolutions

| Finding ID | Resolution / rejected claim with evidence | Commit / exported object | Verification evidence | Remaining risk |
| --- | --- | --- | --- | --- |
| {{finding_or_first_build_na}} | {{resolution}} | {{change}} | {{verification}} | {{risk}}

## Review and release readiness

Reviewer applicability changes since analysis and reason: {{reviewer_changes_or_none}}
Package completeness, import order and target compatibility evidence: {{packaging}}
Recovery readiness and known irreversible changes: {{recovery}}
Open findings, authorized exceptions and owners: {{remaining_items_or_none}}
Source merged? Platform promoted? State each separately: {{delivery_state}}
