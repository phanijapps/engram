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

<!--
  v2 template. House rules (full text: references/templates.md §Writing rules):
  one sentence per line · short sentences, active voice, present tense · approved
  terms from the job glossary · never break a path or file:line citation · one
  idea per table row; paragraph-sized detail goes in a named block below the
  table. Every placeholder below is a required element — replace all before
  setting status to ready. The controller rejects any left unresolved.
-->

# Analysis: {{feature_title}}

| Field | Value |
|---|---|
| Reader / question | Planner asking "what must be true, and how will we know?" · human owner ratifying scope |
| Status | cycle {{cycle}}, round 0 · supersedes: none · superseded-by: — |
| Budget | ≤ 600 narrative words (tables excluded); state a reason if over |
| Out of scope | Verbatim feature (feature.md is authoritative) · design (plan.md owns it) |

## Intent

> Human-owned input. If a registered brief exists (input_artifacts), the
> ledger is BINDING: every DEC block below traces to a LED row and the
> controller requires each LED id addressed in this body; HUMAN-DECIDED rows
> are preserved verbatim; DELEGATED decisions stay inside their quoted
> boundary — a decision outside it blocks. OPEN rows block analysis until the
> owner decides or delegates — never downgrade or convert one. Consume the
> whole brief: Must → outcome core, Should → scope (a dropped Should is named,
> never silent), Could → excluded from this job, Won't → non-goals, owner
> suggestions carry no authority (independent justification or drop). A later
> human rework note is the newer owner authority: ledger rows it explicitly
> reverses are superseded — record old row verbatim plus the reversal. If no
> brief exists and this section cannot be stated confidently, STOP and
> interview the owner before analyzing.
> Enforcement: approver and human gate check ledger fidelity; the gate shows the
> ratification back to the human.

{{intent}}

*(Shape: problem in the owner's words · outcome measures · explicit non-goals · decision authority. Tag every consequential choice `[HUMAN-DECIDED]` or `[DELEGATED: rationale]`.)*

## Story

> Four short paragraphs. Enforcement: reviewers check every claim here appears in Evidence with a label.

{{story}}

*(Shape: what happens today and why it hurts → why now → what changes on success → what failure costs.)*

## Evidence

> One claim per row, one sentence, evidence-labeled. Detail goes in named blocks below — never wrapped cells.
> Enforcement: reviewers spot-check citations against the frozen source.

| ID | Claim (one sentence) | Label | Source |
|---|---|---|---|
| EVD-001 | {{first_claim}} | proven/likely/assumed | file:line · doc · tool+query |

*(Labels: proven = verified against source · likely = strong signal, unverified · assumed = stated, unverified. Every load-bearing unverified claim needs a Decision or a Risk.)*

## Decisions

> ADR blocks, authority-tagged. This section is the only home for choices.
> Enforcement: the planner must consume every DEC block; an orphan decision fails plan validation.

{{decisions}}

*(Shape per block: `DEC-n [HUMAN-DECIDED | DELEGATED: why]` — context → decision → alternative rejected → consequence.)*

## Requirements

> EARS sentences with RFC-2119 modality. One requirement, one sentence.
> Enforcement: every REQ appears in Acceptance; an unreferenced REQ fails review.

{{requirements}}

*(Shape: `REQ-n (MUST | SHOULD): When <trigger> [<precondition>], the system shall <response>`. An NFR without a measurable threshold is not a requirement — cut it or give it a number, and place it in Acceptance's map.)*

## Acceptance

> Given/When/Then. This table is THE traceability map — tasks and tests link here; no other artifact restates it.
> Enforcement: cross-artifact link check during tasks validation.

| AC | Given | When | Then (incl. denied/failure) | REQ |
|---|---|---|---|---|
| AC-001 | {{first_acceptance}} | — | — | REQ-001 |

## Scope boundaries

> One line each. "Out" is as load-bearing as "In". Enforcement: reviewer applicability and plan scope must trace here.

- In: {{in_scope}}
- Out: {{out_of_scope}}

## Verification stance

> Positive AND negative evidence. Silence is not allowed.
> Enforcement: QA/approver checks every N/A carries a reason and an owner.

{{verification_stance}}

*(Shape: will-verify list mapped to ACs · will-NOT-verify list: `omitted: <check> — reason — owner — revisit trigger`.)*

## Reviewer applicability

> Drives required_reviewers in frontmatter — update it to match this table. Apply rubric criteria as written; do not narrow them without a stated reason.
> Enforcement: analysis validation requires the declared set to cover every yes-row.

| Discipline | Applicable | One-line reason |
|---|---|---|
| code | yes | {{code_reason}} |
| security | yes | {{security_reason}} |
| platform | yes | {{platform_reason}} |
| others | yes/no | one line each |

## Risks

| ID | Scenario (one sentence) | Owner · escalation trigger |
|---|---|---|
| RISK-001 | {{top_risk}} | owner · trigger |

## Glossary

> Terms this spec uses with distinct meanings — declare the distinctions once.
> Enforcement: reviewers flag recurring domain terms that are absent.

{{glossary}}

*(Shape: `term — meaning (≠ what it is NOT, when the confusion is real)`.)*

## Environment inventory

> Platform profiles only. For purely local work, omit the table and record the omission in Verification stance.
> Enforcement: platform reviewer demands it when a platform profile is active.

{{environment_inventory}}
