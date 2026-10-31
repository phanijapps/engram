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
required_reviewers: ["code"]
---
<!--
  Spec template v2 (constitution adoption 2026-10-31). The artifact filename is spec.md (renamed from analysis.md, 2026-10-31).
  v2 changes: intent trace pins the brief by hash; requirements are the
  canonical behavior source (EARS); acceptance binds to named gate IDs;
  evidence cites path:line; scope is an explicit out-list. NFR-* is removed:
  measurable behavior is REQ-*, everything else is a risk, constraint, or gate.
  Verified by tools/ci/check_job_artifacts.py (ID integrity + coverage
  totality + no TBD in ratified sections).
-->


# Spec: <short outcome>

## Intent trace

| Brief ID | Interpretation / disposition | Authority |
|---|---|---|
| MOS-001 | included as REQ-001 | HUMAN-DECIDED |
| LED-001 | resolved by DEC-001 | DELEGATED |

## Evidence

| ID | Claim | Status | Citation |
|---|---|---|---|
| EVD-001 | <one falsifiable claim> | proven | `path/to/file:12-18` |

## Decisions

| ID | LED / authority | Decision | Rejected alternative | Consequence |
|---|---|---|---|---|
| DEC-001 | LED-001 / DELEGATED | <one choice> | <one alternative> | <observable effect> |

## Requirements (canonical behavior)

| ID | Priority | Testable statement | Rules | Evidence |
|---|---|---|---|---|
| REQ-001 | MUST | When <trigger>, the system shall <observable response>. | <rule ID> | EVD-001 |

## Acceptance and named gates

| ID | REQ IDs | Given / When / Then | Gate IDs | Negative case |
|---|---|---|---|---|
| AC-001 | REQ-001 | <concise executable behavior> | GATE-001 | <denial/failure behavior> |

## Scope boundaries

- **In:** named REQ IDs and paths/areas.
- **Out:** named behavior/paths; `none` is valid.
- **Deferred:** DEV IDs or follow-up owner + trigger; never an unnamed omission.

## Verification stance

| Gate ID | Command or fixture | Proves AC IDs | Expected evidence |
|---|---|---|---|
| GATE-001 | `<command>` | AC-001 | <exit/result/artifact> |

## Risks and reviewer applicability

| ID | Risk / reviewer discipline | Trigger | Owner |
|---|---|---|---|
| RISK-001 | <scenario> / code=yes | <signal> | <role> |

## Terms and environment (only when needed)

- TERM-001: <definition that disambiguates a requirement>.
- Environment: <platform/version only if it changes verification or behavior>.
