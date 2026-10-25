---
schema_version: "1.0"
artifact_type: "plan"
job_id: "{{job_id}}"
cycle: {{cycle}}
status: "draft"
author_role: "engram-planner"
profiles: {{profiles_json}}
source_snapshot: null
input_artifacts: []
updated_at: "{{timestamp}}"
---

# Plan: {{feature_title}}

## Solution and boundaries

Accepted analysis revision/hash: {{analysis_identity}}
Approach and why it meets the outcome: {{approach}}
Components, ownership and trust boundaries: {{components}}
Interaction sequence including asynchronous paths: {{sequence_or_compact_mermaid}}
Native configuration versus extension/custom code: {{extension_strategy}}

## Design decisions

| Decision | Requirement / constraint | Options and tradeoffs | Selected option and reason | Consequence / reversibility |
| --- | --- | --- | --- | --- |
| DEC-001 | {{requirement}} | {{alternatives}} | {{choice}} | {{consequence}}

## Change inventory

| Component / native object ID | Type: code / metadata / config / data / infrastructure | Current -> proposed behavior | Source of truth and export / package | AC / NFR |
| --- | --- | --- | --- | --- |
| {{component}} | {{type}} | {{delta}} | {{source_package}} | {{coverage}}

## Data design and migration

Entities/records, keys, relationships and validation rules: {{model}}
Ownership, tenancy, classification and retention: {{data_boundaries}}
Schema compatibility, expand/contract order and old-client behavior: {{compatibility}}
Migration/backfill volume, batching, locking and restart checkpoints: {{migration}}
Reconciliation queries, invariants and repair procedure: {{reconciliation}}
Irreversible operations, backup/restore limits and authorization: {{irreversibility}}

## Contracts and business execution

| API / event / workflow | Producer and consumers | Versioned request / response / transition | Authorization and validation | Errors, compatibility and evidence |
| --- | --- | --- | --- | --- |
| {{contract}} | {{participants}} | {{schema_or_reference}} | {{enforcement}} | {{error_contract}}

Execution identity/context and transaction boundary: {{execution_context}}
Bulk/volume limits, concurrency and resource budget: {{limits_with_version_source}}
Retries, timeouts, idempotency, duplicates and event ordering: {{delivery_semantics}}
Compensation for external side effects and partial failure: {{compensation}}

## User and operator experience

Journeys and state transitions: {{journeys}}
Loading, empty, success, validation, permission-denied and failure states: {{states}}
Accessibility, keyboard/focus, localization and responsive needs: {{accessibility}}
Manual steps, administrator/support experience and training: {{operator_experience}}

## Security and operations

Server-side permission enforcement and least privilege: {{permission_design}}
Secrets references, sensitive-data controls and audit events: {{security_controls}}
Threat/abuse cases and mitigations linked to tests: {{threats}}
Logs, metrics, tracing, correlation IDs and redaction: {{observability}}
Alert thresholds, owners, runbooks and support handoff: {{operations}}

## Verification approach

| AC / NFR / risk | Test layer and scenario | Fixture / identity / environment | Expected evidence | Gap and resolution |
| --- | --- | --- | --- | --- |
| {{criterion}} | {{test}} | {{context}} | {{evidence}} | {{gap_or_none}}

## Packaging, rollout and recovery

Export/package manifest and source-to-package reconciliation: {{artifact_strategy}}
Dependency and transport/import order across environments: {{promotion_order}}
Environment substitutions and manual steps excluded from packages: {{environment_config}}
Preflight, staged rollout/flags, go/no-go checks and approval owners: {{rollout}}
Rollback versus roll-forward, recovery trigger and data repair: {{recovery}}
Target release/product upgrade compatibility: {{upgrade_assessment}}
Source merge and platform promotion boundaries: {{authorization_boundaries}}

## Delivery sequence and readiness

Dependency-ordered implementation slices: {{slices}}
Open decisions, spikes, access or external dependencies: {{blockers_or_none}}
Ready for task decomposition because: {{readiness_evidence}}
