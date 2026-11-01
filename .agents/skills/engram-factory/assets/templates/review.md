---
schema_version: "1.0"
artifact_type: "review"
job_id: "{{job_id}}"
cycle: {{cycle}}
status: "draft"
author_role: "{{review_author}}"
profiles: {{profiles_json}}
source_snapshot: "{{snapshot_path}}"
input_artifacts: []
updated_at: "{{timestamp}}"
round: {{round}}
discipline: "{{discipline}}"
verdict: "ERROR"
---

# Review — {{discipline}}, round {{round}}: {{feature_title}}

<!--
  Review v3: verdict artifact. Findings are ID'd (FIND-*) and actionable;
  PASS / CHANGES / ERROR per the controller contract — never soft language.
-->

## Scope inspected

{{scope}}

## Findings

{{first_finding}}

{{finding_detail}}

## Prior findings status

{{prior_findings}}

## Exceptions

{{exceptions}}

## Verdict

**{{verdict_basis}}**
