---
schema_version: 2
spec_format: "compact-v1"
size_tier: "S"
scope: "single"
required_reviewers: ["code", "security", "platform"]
---
# Spec: Orders reject nonpositive quantities

## Outcome
Nonpositive quantity is rejected without persistence; valid quantity creates one order.

## Scenarios
| ID | Priority | Actor | Journey | Outcome | Requirements | Acceptance | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| U-1 | P1 | operator | exercise the scoped workflow | the accepted behavior is observable | REQ-001 | AC-001, AC-002 | repository source and ratified fixture brief |

## Requirements
| ID | Behavior | Rules |
| --- | --- | --- |
| REQ-001 | Nonpositive quantity is rejected without persistence; valid quantity creates one order. | EVAL-001 |

## Acceptance
| ID | Requirement | Given | When | Then |
| --- | --- | --- | --- | --- |
| AC-001 | REQ-001 | nonpositive quantity | order workflow executes | rejected result with no persisted order |
| AC-002 | REQ-001 | positive quantity | order workflow executes | one accepted order is persisted |

## Boundaries
Only the fixture request is in scope. No production access, deployment or unrelated cleanup is authorized.

## Decisions and evidence
LED-001 fixes scope and allowed writes in the supplied fixture brief. All authority here is synthetic test authority, never authority over real projects.
