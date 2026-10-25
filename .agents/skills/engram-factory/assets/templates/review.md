---
schema_version: "1.0"
artifact_type: "review"
job_id: "{{job_id}}"
cycle: {{cycle}}
status: "draft"
author_role: "reviewer-{{discipline}}"
profiles: {{profiles_json}}
source_snapshot: "{{snapshot_path}}"
input_artifacts: []
updated_at: "{{timestamp}}"
round: {{round}}
discipline: "{{discipline}}"
verdict: "ERROR"
---

# {{discipline}} review: {{feature_title}}

## Scope and evidence inspected

Snapshot path/hash, base/head and external packages: {{source_identity}}
Accepted input artifacts and evidence hashes: {{inputs}}
Actual delta, native objects and related existing behavior inspected: {{inspection_scope}}
Independent checks run, identities/environments and results: {{checks}}
Applicable rubric/profile and any scope limitations: {{rubric_and_limitations}}

## Findings

Use a distinct discipline-scoped finding ID. State NONE explicitly if no findings.

| Finding ID | Severity | AC / NFR / rule | Location / native object and revision | Reproduction / evidence | Impact | Required resolution and retest |
| --- | --- | --- | --- | --- | --- | --- |
| FIND-{{discipline}}-001 | {{severity_or_none}} | {{criterion}} | {{location}} | {{evidence}} | {{impact}} | {{remediation}}

## Prior findings and exceptions

| Prior finding / EXC | Current verification | Resolution evidence / authorization / expiry | Status |
| --- | --- | --- | --- |
| {{prior_or_na}} | {{check}} | {{evidence}} | {{state}}

## Verdict

Set frontmatter verdict to PASS, CHANGES, or ERROR. All blocking findings require
CHANGES. Incomplete review requires ERROR, even when no defect was observed.

Decision and evidence: {{verdict_reason}}
Blocking findings and required next action: {{blocking_findings_or_none}}
Nonblocking follow-up and accountable owner: {{followup_or_none}}
