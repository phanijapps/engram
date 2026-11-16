---
schema_version: 2
spec_format: "compact-v1"
size_tier: "S"
scope: "single"
required_reviewers: ["code", "security", "platform"]
---
# Spec: Safe CSV import

## Outcome
Valid unique IDs are imported once; malformed rows are reported and source data is unchanged.

## Scenarios
| ID | Priority | Actor | Journey | Outcome | Requirements | Acceptance | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| U-1 | P1 | operator | exercise the scoped workflow | the accepted behavior is observable | REQ-001 | AC-001, AC-002 | repository source and ratified fixture brief |

## Requirements
| ID | Behavior | Rules |
| --- | --- | --- |
| REQ-001 | Valid unique IDs are imported once; malformed rows are reported and source data is unchanged. | EVAL-001 |

## Acceptance
| ID | Requirement | Given | When | Then |
| --- | --- | --- | --- | --- |
| AC-001 | REQ-001 | duplicate valid rows | import runs | one output per unique ID is retained |
| AC-002 | REQ-001 | malformed row | import runs | row is reported and source bytes remain unchanged |

## Boundaries
Only the fixture request is in scope. No production access, deployment or unrelated cleanup is authorized.

## Decisions and evidence
LED-001 fixes scope and allowed writes in the supplied fixture brief. All authority here is synthetic test authority, never authority over real projects.
