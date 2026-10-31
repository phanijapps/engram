---
schema_version: 2
artifact: brief
job_id: "JOB-ID"
status: "draft"
size_tier: "S"
owner: "OWNER-REF"
ratified_at: null
constitution_rules_touched: []
---
<!--
  Brief template v2 (constitution adoption 2026-10-31) — authored by engram-frame
  WITH the human owner, BEFORE a factory job exists. Registered by the controller
  at create (--brief-file), archived content-addressed, and REQUIRED as a spec
  input while pinned.
  v2 changes: machine-checkable frontmatter (size_tier scales downstream
  ceremony), problem separated from symptom, constitution rules declared.
  Verified by tools/ci/check_job_artifacts.py (advisory → blocking 2026-11-28).
-->


# Brief: <one-line title, the owner's words>

## The ask (verbatim)

> Owner words only, any length. Becomes the factory feature text unchanged; do
> not paraphrase elsewhere. If the owner replaced an earlier ask, quote the
> replacement instruction here.

## Problem, not symptom

- **Problem:** affected user/system and the harmful outcome.
- **Observed symptom/evidence:** concrete signal; do not prescribe a mechanism.
- **Why now:** consequence of not acting.

## Intended outcome

- OUT-001: <observable owner success signal and judgment moment — checkable
  behavior, no mechanism names. Without a signal and judgment moment the
  outcome is OPEN and the brief is not factory-ready.>

## Priorities (MoSCoW)

> Owner-edited. Behavior terms only — no mechanisms. Must feeds the outcome
> core, Should the scope, Could the follow-ups, Won't the non-goals.

| Priority | Outcome / boundary | ID |
|---|---|---|
| Must | <behavior required for OUT-001> | MOS-001 |
| Should | <in-scope behavior, not required for success> | MOS-002 |
| Could | <follow-up only> | MOS-003 |
| Won't | <explicit non-goal> | MOS-004 |

## Constraints and constitution impact

| Constraint or rule | Source / authority | Effect |
|---|---|---|
| <fixed standard, deadline, boundary> | <source> | <effect> |
| <constitution rule ID> | constitution/<rail>.md | <rule touched or `none`> |

## Decision ledger

| ID | Choice | Authority | Bound decision / verbatim owner statement | Revisit trigger |
|---|---|---|---|---|
| LED-001 | <consequential choice> | HUMAN-DECIDED | <message reference and quote> | <trigger> |

## Scope

- **In:** <outcome IDs / MoSCoW IDs>.
- **Out:** <explicit exclusions — the Won't bucket, restated by ID>.
- **Open:** `none`, or LED IDs that block ratification.

## Owner ratification

| Owner reference | Date | Verbatim confirmation |
|---|---|---|
| <channel/message ID> | YYYY-MM-DD | <quote> |

## Handoff

- Feature source: `<path>` and content hash.
- Spec may decide: only explicitly delegated LED IDs and boundaries — nothing else.
