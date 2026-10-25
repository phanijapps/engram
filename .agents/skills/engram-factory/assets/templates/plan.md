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

<!--
  v2 template. House rules: references/templates.md §Writing rules.
  Steps use imperative STE-style sentences. Requirements stay in EARS — the plan
  never restates them; it links REQ/AC ids from analysis.md.
-->

# Plan: {{feature_title}}

| Field | Value |
|---|---|
| Reader / question | Builder asking "what do I do, in what order, and how do I prove each step?" · reviewers checking design soundness |
| Status | cycle {{cycle}} · consumes analysis (DEC blocks) · supersedes: none |
| Budget | ≤ 500 narrative words (tables excluded) |
| Out of scope | Requirements and ACs (analysis.md owns them) · task decomposition (tasks.md owns it) |

## Approach

> The design story in plain words. Start by naming which analysis DEC blocks this plan follows.
> Enforcement: every DEC-n from analysis appears here or in New decisions; an unconsumed DEC fails validation.

{{approach}}

*(≤ 200 words. If a reader cannot retell the design from this paragraph alone, it is not done.)*

## New decisions

> Only choices NEW since analysis — ADR blocks, authority-tagged, same shape as analysis DEC blocks.

{{new_decisions}}

## Steps

> One sentence per step, imperative, each naming its own proof. Small steps beat large ones.
> Enforcement: builder verification and reviewer traceability follow this table; a step without a verify command fails review.

| Step | Do (one sentence) | Verify with | Touches |
|---|---|---|---|
| 1 | {{first_step}} | `<command>` | files / crates |
| 2 | {{steps}} | `<command>` | files / crates |

## Contracts and compatibility

> What changes in contracts, schemas, or public surface — classified compatible or breaking, per repo docs standard (docs first).
> Enforcement: platform reviewer checks the classification against the actual diff.

{{contract_changes}}

## Blast radius

> Quantified from the graph, not adjectives. Enforcement: code reviewer checks the numbers against change_impact output.

{{blast_radius}}

*(Shape: `<symbol/area> — <change_impact number> dependents — <risk shape>`; name every crate boundary crossed.)*

## Verification map

> Which step proves which AC. Link — do not restate — analysis §Acceptance.
> Enforcement: every AC id from analysis appears exactly once here or is named in a reasoned omission.

{{verification_map}}

*(Shape: `AC-001 ← step 3 gate output` · `AC-005 ← steps 6-8 matrix`.)*

## Recovery and rollback

> How to undo this change and how a partial failure recovers. Imperative steps, exact commands.
> Enforcement: release.md consumes this section verbatim.

{{rollback}}

## Glossary deltas

> New terms this plan introduces beyond the analysis glossary. If none, write: none.

{{glossary_deltas}}
