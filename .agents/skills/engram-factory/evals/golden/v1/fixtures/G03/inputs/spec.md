---
schema_version: 2
spec_format: "compact-v1"
size_tier: "S"
scope: "single"
required_reviewers: ["code", "security", "platform"]
---
# Spec: Dry-run export

## Outcome
A dry-run reports the output path without writing, and invalid destinations return a named error.

## Scenarios
| ID | Priority | Actor | Journey | Outcome | Requirements | Acceptance | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| U-1 | P1 | operator | exercise the scoped workflow | the accepted behavior is observable | REQ-001 | AC-001, AC-002 | repository source and ratified fixture brief |

## Requirements
| ID | Behavior | Rules |
| --- | --- | --- |
| REQ-001 | A dry-run reports the output path without writing, and invalid destinations return a named error. | EVAL-001 |

## Acceptance
| ID | Requirement | Given | When | Then |
| --- | --- | --- | --- | --- |
| AC-001 | REQ-001 | valid destination | dry-run executes | path is displayed and no destination is written |
| AC-002 | REQ-001 | invalid destination | dry-run executes | nonzero result names the invalid destination |

## Boundaries
Only the fixture request is in scope. No production access, deployment or unrelated cleanup is authorized.

## Decisions and evidence
LED-001 fixes scope and allowed writes in the supplied fixture brief. All authority here is synthetic test authority, never authority over real projects.
