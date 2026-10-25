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

<!--
  v2 template. This IS the human gate artifact — answer-first, one page.
  House rules: references/templates.md §Writing rules.
-->

# Decision: {{feature_title}}

> **The human authorizes exactly one action by approving this: merge the reviewed
> source to the target branch. Nothing else — no deployment, no promotion.**

| Field | Value |
|---|---|
| Recommendation | See frontmatter (`APPROVE` or `ESCALATE`) |
| Identity | snapshot `{{snapshot_path}}` · cycle {{cycle}} |

## What you authorize

> Exact bytes: branch, commit, snapshot, base. No movable refs. If a brief is
> registered, display its Owner ratification quote and decision ledger here —
> the human reconciles what they supposedly decided against what ships.

{{authorize}}

## What was verified — and by whom

> Executed evidence only: which gates ran, who executed them, where results live (link verification.md and the review reports — do not restate).

{{verified}}

## What was NOT verified

> The negative evidence, carried forward verbatim from verification §Negative evidence. If this list is empty, say why that is credible.

{{not_verified}}

## What you accept

> Every shipped limitation and follow-up, with owners. Approving means owning these.

{{accepted_risks}}

## What this does not authorize

> Deployment, platform promotion, configuration changes outside Git — each requires its own authorization.

## Why

> The recommendation rationale in ≤ 100 words. If evidence contradicts itself anywhere, name it here or escalate.

{{rationale}}
