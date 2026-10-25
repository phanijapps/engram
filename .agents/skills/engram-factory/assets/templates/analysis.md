---
schema_version: "1.0"
artifact_type: "analysis"
job_id: "{{job_id}}"
cycle: {{cycle}}
status: "draft"
author_role: "engram-analyzer"
profiles: {{profiles_json}}
source_snapshot: null
input_artifacts: []
updated_at: "{{timestamp}}"
required_reviewers: ["code", "security", "platform"]
---

# Analysis: {{feature_title}}

## Problem and outcome

Business problem: {{problem}}
Users/operators and decision owner: {{personas_and_owner}}
Current behavior and evidence: {{observed_baseline}}
Expected behavior and measurable outcome: {{target_outcome}}
Failure cost and criticality: {{business_impact}}

## Evidence and assumptions

| ID | Fact / assumption / open question | Evidence, revision and environment | Owner / resolution before |
| --- | --- | --- | --- |
| EVD-001 | {{classification_and_statement}} | {{evidence_reference}} | {{resolution}}

## Scope and boundaries

In scope: {{included_behavior}}
Out of scope: {{excluded_behavior}}
Existing constraints and organization standards: {{constraints_with_sources}}
Systems/teams owned elsewhere and dependency commitments: {{dependencies}}
Compatibility or contractual obligations: {{compatibility}}

## Product and environment inventory

| System / component | Product, edition, release / API | Source of truth / export method | Environment alias, owner and permitted access | Drift / dependency evidence |
| --- | --- | --- | --- | --- |
| {{system}} | {{version}} | {{source_and_export}} | {{environment_and_authorization}} | {{baseline}}

## Requirements and acceptance

| Requirement | Actor and trigger | Observable business behavior / rules | Acceptance IDs | Priority and source |
| --- | --- | --- | --- | --- |
| REQ-001 | {{actor_trigger}} | {{behavior}} | AC-001 | {{priority_source}}

| Acceptance ID | Given | When | Then, including denied / failure behavior | Verification method |
| --- | --- | --- | --- | --- |
| AC-001 | {{precondition}} | {{action}} | {{observable_expected_result}} | {{test_method}}

## Quality requirements

| NFR ID | Attribute | Workload / operating conditions | Measurable threshold | Evidence / policy source | Verifier |
| --- | --- | --- | --- | --- | --- |
| NFR-001 | {{latency_capacity_availability_accessibility_recovery_or_other}} | {{conditions}} | {{threshold_or_explicit_decision_dependency}} | {{source}} | {{verification}}

## Identity, data and integrations

| Persona / service identity | Operation / workflow transition | Object / field / record scope | Allow / deny rule and enforcement point | Audit / privacy requirement |
| --- | --- | --- | --- | --- |
| {{identity}} | {{operation}} | {{scope}} | {{rule_and_server_enforcement}} | {{audit}}

Data classification, residency, retention and deletion: {{data_policy}}
Systems of record, data ownership, migration volume and reconciliation: {{data_impacts}}
API/event contracts, versions, consumers and failure expectations: {{integration_impacts}}
Licenses, entitlements and third-party dependency constraints: {{entitlements}}

## Reviewer applicability

Add every applicable discipline to required_reviewers in frontmatter. The initial
list is the mandatory baseline, not a completed applicability decision.

| Discipline | Applicable | Scope evidence and rationale |
| --- | --- | --- |
| code | yes | {{code_or_declarative_logic}}
| security | yes | {{security_boundary}}
| platform | yes | {{configuration_delivery_and_compatibility}}
| ux | {{yes_or_no}} | {{ux_reason}}
| ui | {{yes_or_no}} | {{ui_reason}}
| backend | {{yes_or_no}} | {{backend_reason}}
| data | {{yes_or_no}} | {{data_reason}}
| operations | {{yes_or_no}} | {{operations_reason}}

## Risks and readiness

| Risk ID | Scenario and impact | Likelihood / evidence | Mitigation / detection | Owner and escalation trigger |
| --- | --- | --- | --- | --- |
| RISK-001 | {{risk}} | {{likelihood}} | {{mitigation}} | {{owner_trigger}}

Material open decisions and their deadlines: {{open_decisions_or_none}}
Ready for planning because: {{evidence_that_scope_and_acceptance_are_actionable}}
