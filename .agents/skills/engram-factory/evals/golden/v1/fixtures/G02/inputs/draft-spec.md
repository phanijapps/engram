---
schema_version: 2
spec_format: "compact-v1"
size_tier: "S"
scope: "single"
required_reviewers: ["code", "security", "platform"]
---
# Spec: Report export preview

## Outcome
Allow admins to preview export while preserving denied access and listing behavior.

## Scenarios
| ID | Priority | Actor | Journey | Outcome | Requirements | Acceptance | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| U-1 | P1 | operator | exercise the scoped workflow | the accepted behavior is observable | REQ-001 | AC-001, AC-002 | repository source and ratified fixture brief |

## Requirements
| ID | Behavior | Rules |
| --- | --- | --- |
| REQ-001 | Allow admins to preview export while preserving denied access and listing behavior. | EVAL-001 |

## Acceptance
| ID | Requirement | Given | When | Then |
| --- | --- | --- | --- | --- |
| AC-001 | REQ-001 | admin request | preview runs | the intended output is shown with no writes |
| AC-002 | REQ-001 | valid existing list request | listing runs | existing listing behavior remains unchanged |

## Boundaries
Only the fixture request is in scope. No production access, deployment or unrelated cleanup is authorized.

## Decisions and evidence
LED-001 fixes scope and allowed writes in the supplied fixture brief. All authority here is synthetic test authority, never authority over real projects.
