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

<!--
  v2 template. House rules: references/templates.md §Writing rules.
  Findings: five columns, one idea per row. Repro detail goes in named blocks
  below the table — never wrapped cells. Verdict semantics: PASS = no changes
  required · CHANGES = implementation changes required · ERROR = could not
  conclude. Judge independently; do not rely on another reviewer's verdict.
-->

# Review — {{discipline}}, round {{round}}: {{feature_title}}

| Field | Value |
|---|---|
| Scope | snapshot `{{snapshot_path}}` · the delta base..head · artifacts as hashed at freeze |
| Lens | Apply the discipline rubric (references/reviews.md) as written — do not narrow it |

## Scope inspected

> What you actually read, ran, and compared. Name the commands you executed. If you could not execute a check, say so here — that is evidence, not weakness.

{{scope}}

## Findings

| ID | Severity | What breaks (one sentence) | Evidence | Fix direction |
|---|---|---|---|---|
| FIND-{{discipline}}-001 | critical/high/medium/low | {{first_finding}} | file:line | one sentence |

*Detail blocks — one per finding that needs more than one sentence:*

**FIND-{{discipline}}-001 — reproduction and impact.**
{{finding_detail}}

## Prior findings status

> REQUIRED from round 2 on: every finding from your earlier rounds, resolved or still open. Silence here fails.

| Prior finding | Status | Verified by |
|---|---|---|
| {{prior_findings}} | resolved/open | commit · test · line |

## Exceptions

> Authorized exceptions with owner, scope, and expiry — or the single word: none.

{{exceptions}}

## Verdict

> One line per discipline rule you applied and what it showed. Then the verdict that matches the frontmatter.

{{verdict_basis}}
