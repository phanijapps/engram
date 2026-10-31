<!--
PROPOSED (not landed) — owner verification copy from 06-redesigned-templates.md.
Integration: frontmatter must keep the union keys factory.py validates
(schema_version 2, artifact/job_id/cycle/status/author_role/profiles/
source_snapshot/input_artifacts/updated_at) with {{render}} placeholders;
REQ IDs in EARS + AC/GATE tables satisfy check_job_artifacts.py regexes.
-->

---
schema_version: 2
artifact: "spec"
job_id: "{{job_id}}"
cycle: {{cycle}}
status: "draft"
author_role: "engram-analyzer"
profiles: {{profiles_json}}
source_snapshot: null
input_artifacts: []
updated_at: "FILL: ISO-8601 with timezone"
size_tier: "S"
constitution_rules_touched: []
brief_ref: null
required_reviewers: ["code", "security", "platform"]
---

# Spec: <short observable outcome>

## What problem are we solving, and why will this work?

<Open with the brief’s problem, the narrow behavioral bet, and the consequence of being wrong. Cite the brief IDs; do not repeat the brief wholesale.>
For example, consuming `MOS-001` by rejecting a missing declared path closes the silent-enforcement gap without expanding the set of gated layers.

## What did we learn, and what did we decide?

`EVD-001` proves <one falsifiable claim> at `<path:line-range or immutable source>`.
For example, `EVD-001` proves `scan()` skips a nonexistent declared path at `tools/ci/check_engine_neutrality.py:74`.
`DEC-001` [DELEGATED: <brief authority>] chooses <choice> over <rejected alternative> because <evidence-based reason>; the observable consequence is <effect>.
If the brief made the choice, say that no new `DEC-*` is needed rather than inventing one.

## What behavior MUST hold?

Requirements are the canonical behavior source. Use one EARS sentence per row and BCP 14 keywords only for normative obligations.

| ID | Priority | Testable statement | Rules | Evidence |
|---|---|---|---|---|
| REQ-001 | MUST | When a declared `GATED_PATH` does not exist on disk, the gate SHALL exit 1 and name the missing path. | EN-004 | EVD-001 |

Explain any interaction, ordering, or denial semantics the row cannot safely carry in prose: `REQ-001` fails before a missing path can be silently excluded from scanning.

## How will a reviewer recognize success or failure?

`AC-001` links `REQ-001`: given a path list containing a missing directory, when the gate runs, then it exits 1 and names that directory; with present-only paths, it exits 0.
Do not hide failure behavior: each `AC-*` MUST state the relevant denial, error, or regression case.

| Gate ID | Command or fixture | Proves AC IDs | Expected evidence |
|---|---|---|---|
| GATE-001 | `python3 tools/ci/tests/test_engine_neutrality.py` | AC-001 | two passing fixture cases, including missing-path failure |

## What remains outside this change?

**In:** `REQ-001` in `<named paths>`.
**Out:** <named behavior, path, or `MOS-*` ID>; `none` is valid.
**Deferred:** <owner, trigger, and `DEV-*` or `LED-*` ID>; never use an unnamed omission.
State compatibility directly: <public/internal/none> is <compatible/breaking/none> because <consumer or migration consequence>.

## What could invalidate the plan, and who must review it?

`RISK-001`: <scenario>; owner <role>; escalate when <observable trigger>.
Declare `required_reviewers` in frontmatter and justify every applicable discipline in prose.
For example, code reviews the changed gate, security confirms fail-closed enforcement, and platform confirms the fixture works in supported checkouts.
Add terms or environment facts only when they change requirement interpretation or verification.
