<!--
  Brief template — authored by engram-frame WITH the human owner, BEFORE a
  factory job exists. Registered by the controller at create (--brief-file),
  archived content-addressed, and REQUIRED as an analysis input while pinned.
  House rules: references/templates.md §Writing rules. Reasoning genre —
  narrative license, no EARS here.
-->

# Brief: <one-line title, the owner's words>

> **Functional nature**: this brief describes what the system must do, for
> whom, and why it matters — in language a functional reader can verify.
> How it is built belongs to analysis and plan. Owner-fixed technical
> constraints are allowed but labeled; owner suggestions are non-binding.

| Field | Value |
|---|---|
| Reader / question | Analyzer asking "what does the human want, and what may I decide?" · owner ratifying their own intent |
| Status | drafted <date> by engram-frame with <owner> · supersedes: none |
| Budget | ≤ 300 narrative words. Quoted owner text and ledger rows are exempt — never compress authority to fit a page. |

## The ask

> <The owner's request, verbatim, any length. Becomes the factory feature text
> unchanged. If the owner replaced an earlier ask, quote the replacement
> instruction here — never shorten or paraphrase.>

## Problem

<What hurts, for whom, since when — functional impact in behavior terms,
no tool or mechanism names. Owner's words in quotes where they matter.>

## Outcome measures

- <How we will know it worked — checkable behavior, no mechanism names, or
  `[UNMEASURABLE: the owner's pass/fail signal + when and how the owner will
  judge]`. Without a signal and a judgment moment, the outcome is OPEN — the
  brief is not factory-ready.>

## Non-goals

- <What explicitly does NOT change — the Won't bucket from the MoSCoW round.>

## Priorities (MoSCoW)

> From the scope round; owner-edited. Behavior terms only — no mechanisms.
> Must feeds the outcome core, Should the scope, Could the follow-ups,
> Won't the non-goals.

| Bucket | Items |
|---|---|
| Must | <items> |
| Should | <items> |
| Could | <items — follow-up candidates, not in this job> |
| Won't | <items> |

## Known constraints and standards

- <Fixed standards, deadlines, regulatory or business boundaries — one line
  each, with source. Owner-fixed technical constraints are the exception, and
  allowed ONLY for externally imposed boundaries (compatibility, regulatory,
  interface, operational), each with a source and why it is necessary:
  `owner-fixed: <mechanism> (imposed by <source>; necessary because <reason>).`
  Internal implementation preferences are NOT constraints — they belong in
  Owner suggestions or a delegated ledger row.>

## Decision ledger

> Every consequential choice, one row per LED id. `[HUMAN-DECIDED]` requires
> an explicit owner pick (recorded as "owner picked B: <option text>") or
> verbatim statement, traceable to a retrievable owner message. `[DELEGATED]`
> requires an explicitly granted boundary and a revisit trigger — never
> inferred. `[OPEN]` is undecided and **material by default**: it blocks
> analysis until the owner decides or delegates. Downgrading to nonmaterial
> requires a verbatim owner-approved deferral written INTO the row
> (`deferred: owner quote — rationale — trigger`).

| ID | Choice | Authority | Statement / boundary | Revisit trigger |
|---|---|---|---|---|
| LED-1 | <choice> | HUMAN-DECIDED · DELEGATED · OPEN | quote · boundary | when to revisit |

## Delegated to analysis (explicit only)

> Only rows the owner delegated. Shape: `analysis may decide <x>, within
> <boundary>, because <owner's rationale>.` Analysis may not exceed a stated
> boundary; a decision outside it blocks, not ships.

## Open — owner must decide

> OPEN rows block analysis, full stop. Analysis never converts, downgrades, or
> decides an open row. A row leaves OPEN only by the owner's explicit decision
> or delegation recorded back in the ledger — or by a verbatim owner-approved
> deferral written into the row.

## Owner ratification

> The owner's verbatim confirmation of this brief, traceable to a retrievable
> owner-authored message (channel + message reference or date). Forgery
> control: the approval gate displays this quote and the ledger back to the
> human — invented decisions surface there. A fast-path interview satisfies
> this with its single confirm-and-ratify turn.

| Actor | Channel | Date | Confirmation (verbatim) |
|---|---|---|---|
| <owner> | <session/ticket> | <date> | <quote> |

## Owner suggestions (non-binding)

> Mechanisms the owner offered — parked for analysis to consider or ignore.
> A suggestion here is NOT a requirement and NOT authority.

- <suggestion — none is a valid entry>

## Handoff

- Feature text: the ask above, verbatim → `factory.py create --feature-file <ask> --brief-file <this brief>`.
- The controller registers the brief, requires it as an analysis input, and
  requires every LED row to be addressed by the analysis body.
- **Consumption contract**: Must → outcome core; Should → scope; Could →
  excluded from this job (follow-ups only); Won't → non-goals; suggestions →
  no inherited authority, independent justification required; material OPEN
  rows → analysis blocks.
- **Rework supersession**: a later human rework note is the newer owner
  authority — ledger rows it explicitly reverse are superseded; analysis
  records both (old row verbatim + reversal), never silently drops either.
