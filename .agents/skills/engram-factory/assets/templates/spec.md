---
schema_version: 2
spec_format: "compact-v1"
size_tier: "S"
scope: "single"
required_reviewers: ["code", "security", "platform"]
---

# Spec: <observable outcome>

## Outcome
<Who needs this, what fails today, and what success means.>

## Scenarios
| ID | Priority | Actor | Journey | Outcome | Requirements | Acceptance | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| U-1 | P1 | <actor> | <journey> | <observable outcome> | REQ-001 | AC-001, AC-002 | <source and revision> |

## Requirements
| ID | Behavior | Rules |
| --- | --- | --- |
| REQ-001 | When <trigger>, the system shall <response>. | <rule IDs or justified N/A> |

## Acceptance
| ID | Requirement | Given | When | Then |
| --- | --- | --- | --- | --- |
| AC-001 | REQ-001 | <normal condition> | <action> | <observable result> |
| AC-002 | REQ-001 | <failure or boundary condition> | <action> | <observable result> |

## Boundaries
<In scope, exclusions with authority, and affected interfaces or compatibility.>

## Decisions and evidence
<Brief LED dispositions, consequential decisions, source evidence and assumptions.>
