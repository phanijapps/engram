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

## Policy, environment and test data

Acceptance/test policy and evidence source: {{policy}}
Product/version, environment alias, component versions and baseline: {{environment}}
Nonproduction authorization and test identities/permission sets: {{authorization}}
Synthetic/masked fixture design, isolation and cleanup: {{fixtures}}
External dependencies, mocks versus real contracts and known gaps: {{dependencies}}
Required checks and rationale for omitted layers: {{applicability}}

## Case inventory

Use complete, explicit identifiers for every linked criterion and test; do not
compress them into numeric ranges or omit prefixes.

| Test ID | AC / NFR / risk | Layer | Scenario / identity | Procedure or automation reference | Expected observable result | Required gate |
| --- | --- | --- | --- | --- | --- | --- |
| TEST-001 | {{criterion}} | {{unit_component_contract_integration_e2e_or_other}} | {{scenario}} | {{procedure}} | {{expected}} | {{gate}}

Cover applicable happy path, invalid input, denied access, role/tenant/site isolation,
bulk/limits, concurrent updates, duplicate/replayed events, downstream failure,
timeouts, migration/reconciliation, regression, accessibility and recovery. Explicitly
record reasoned N/A instead of silently deleting relevant risk coverage.

## Execution records

Use PASS, FAIL, BLOCKED, NOT_RUN, or NA. Record NA reasons. Do not equate a ready
verification plan with successful test execution.

| Test ID | Status | Executed at / run ID | Source commit / package digest | Environment fingerprint | Actual result / metrics | Evidence reference |
| --- | --- | --- | --- | --- | --- | --- |
| TEST-001 | NOT_RUN | {{run_or_not_run}} | {{source_or_pending}} | {{fingerprint_or_pending}} | {{result_or_not_run}} | {{evidence_or_pending}}

## Defects, exceptions and gaps

| Finding / gap | Impacted criterion | Resolution or authorized EXC ID | Owner / expiry | Gate consequence |
| --- | --- | --- | --- | --- |
| {{gap_or_none}} | {{criterion_or_na}} | {{resolution_or_na}} | {{owner_or_na}} | {{gate_effect}}

## Verification conclusion

Acceptance coverage and missing cases: {{coverage}}
Executed versus planned checks and required remaining checks: {{execution_summary}}
Performance/limit measurements versus agreed thresholds: {{measurements}}
Package/configuration reconciliation and drift assessment: {{reconciliation}}
Recovery rehearsal results and retained artifacts: {{recovery}}
Gate readiness and evidence limitations: {{conclusion}}
