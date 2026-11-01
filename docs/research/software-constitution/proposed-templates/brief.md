<!--
PROPOSED (not landed) — owner verification copy, extracted verbatim from
06-redesigned-templates.md (gpt-5.6-terra proposal, 2026-10-31).
Integration constraint before landing: factory.py brief registration requires
the exact substrings "## The ask", "## Owner ratification", "## Decision
ledger" (job 011 hit this live). Use hybrid headings that contain them, e.g.
"## The ask, in the owner's words", "## Decision ledger - already-bound
choices", "## Owner ratification - the human gate".
-->

---
schema_version: 2
artifact: "brief"
job_id: "{{job_id}}"
cycle: {{cycle}}
status: "draft"
author_role: "engram-frame"
profiles: {{profiles_json}}
source_snapshot: null
input_artifacts: []
updated_at: "FILL: ISO-8601 with timezone"
size_tier: "S"
owner: "OWNER-REF"
ratified_at: null
constitution_rules_touched: []
brief_ref: null
---

# Brief: <the owner’s outcome in one line>

## Why act now?

<Open with the affected user or system, the harmful outcome, and the bet this work makes. State the problem rather than prescribing a mechanism.>

For example: a declared engine-neutrality path can disappear and silently stop being scanned; fail-closed validation makes that loss visible in CI before a prohibited engine reference can ship.

## The ask — in the owner's words

> <Quote the owner’s words verbatim. Do not paraphrase or merge an earlier request with a replacement request.>

The judgment moment is OUT-001: <state one observable success signal and who can judge it>.
For example, OUT-001 is satisfied when deleting a declared gated path makes the gate exit non-zero and name that path.

## What matters, and what does not?

Write priorities as a short argument, with literal IDs embedded in the sentences.
`MOS-001` is MUST because <required observable behavior>; `MOS-002` is SHOULD because <valuable but non-essential behavior>.
`MOS-003` is WON'T in this job: <explicit excluded behavior>.
For example, `MOS-001` requires a missing declared path to fail the gate, `MOS-002` requires a fixture proving absent and present paths, and `MOS-003` excludes adding newly gated layers.

**In scope:** <MOS/OUT IDs and bounded areas>.
**Out of scope:** <MOS IDs and explicit exclusions>.
**Open:** `none`, or the blocking `LED-*` IDs.

## Decision ledger — already-bound choices

`LED-001` [HUMAN-DECIDED]: <choice>; the owner said “<verbatim bound>”. Revisit only when <concrete trigger>.
For example, `LED-001` [HUMAN-DECIDED] permits light review because this is an S-tier, one-file gate change; revisit on any reviewer escalation.
State each delegated boundary as `LED-*` [DELEGATED: <reason>], including what the later spec MAY decide and what it MUST NOT decide.

## What constrains the bet?

<Name the authority, deadline, or constitution rule and its practical effect in prose.>
For example, `EN-004` applies because engine-neutrality must remain fail-closed; `DW-7` makes the path-validation gap due in its stated window.
If no constitution rule is touched, write `constitution_rules_touched: []` and explain why the boundary is unaffected.

## Owner ratification — the human gate

<Before ratification, state the missing owner action and keep `status` as `draft`.>
After ratification: owner reference `<channel/message>` on `<YYYY-MM-DD>` confirmed “<verbatim confirmation>”.
The spec receives this brief by content hash and MAY decide only the explicitly delegated `LED-*` boundaries.
