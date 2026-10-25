---
schema_version: "1.0"
artifact_type: "release"
job_id: "{{job_id}}"
cycle: {{cycle}}
status: "draft"
author_role: "engram-tasks"
profiles: {{profiles_json}}
source_snapshot: null
input_artifacts: []
updated_at: "{{timestamp}}"
---

# Release and recovery: {{feature_title}}

## Release boundary

Source merge objective and target branch: {{merge_scope}}
Platform promotion scope and separate authorization reference: {{promotion_scope}}
Change/release record and owners: {{change_record}}
Product, edition, release/API and target environment aliases: {{targets}}
Approved window, user impact, communications and support owner: {{release_window}}

## Immutable delivery manifest

| Artifact / native object group | Source revision / export time | Package or transport identifier | SHA-256 / immutable version | Dependencies / import order | Target mapping |
| --- | --- | --- | --- | --- | --- |
| {{artifact}} | {{source}} | {{package}} | {{digest}} | {{dependencies}} | {{mapping}}

Source-to-package completeness and excluded items: {{reconciliation}}
Environment settings, endpoints and secret references: {{substitutions}}
Manual changes not captured by export and evidence plan: {{manual_changes}}
Drift/conflict check, target baseline and concurrent-change coordination: {{drift_check}}

## Preconditions and gates

| Gate | Evidence / expected condition | Owner | Status / blocker |
| --- | --- | --- | --- |
| Required reviews and source approval | {{snapshot_and_decision}} | {{owner}} | {{status}}
| Target compatibility, dependencies and permissions | {{preflight}} | {{owner}} | {{status}}
| Verification, data reconciliation and recovery rehearsal | {{evidence}} | {{owner}} | {{status}}
| Environment-specific promotion authorization | {{authorization_reference_or_not_requested}} | {{owner}} | {{status}}

## Ordered execution runbook

| Step ID | Environment | Action / exact procedure reference | Preconditions | Expected result and evidence | Executor / authorization | Failure action |
| --- | --- | --- | --- | --- | --- | --- |
| REL-001 | {{environment}} | {{action}} | {{preconditions}} | {{expected}} | {{executor}} | {{failure_action}}

Include necessary backup, dependency import, permissions, schema/configuration,
data migration, activation, scheduled jobs, cache/index work and smoke checks.
State N/A with reasons. Never assume a package includes data, secrets, or every
manual/native configuration change.

## Data and side-effect safety

Migration order, batching, pause/restart and idempotency: {{migration}}
Record-count/value/business-invariant reconciliation: {{reconciliation_queries}}
External messages, payments, emails or irreversible side effects: {{side_effects}}
Data backup/restore and retention/security constraints: {{backup_and_restore}}

## Recovery runbook

| Trigger / threshold | Stop or containment action | Rollback / roll-forward procedure | Data repair / reconciliation | Owner / decision authority | Expected recovery evidence |
| --- | --- | --- | --- | --- | --- |
| {{trigger}} | {{containment}} | {{procedure}} | {{data_repair}} | {{owner}} | {{evidence}}

Irreversible steps and why source revert cannot restore state: {{limitations}}
Recovery objectives and policy source: {{recovery_targets}}
Rehearsal environment, result and evidence: {{rehearsal}}

## Post-release validation and handoff

Smoke checks by identity/persona: {{smoke_checks}}
Monitoring period, metrics/alerts and thresholds: {{monitoring}}
Operational owner, runbook and support escalation: {{handoff}}
Actual deployment run IDs, or explicitly NOT_DEPLOYED: {{deployment_state}}
Cleanup and follow-up tasks: {{followups}}
