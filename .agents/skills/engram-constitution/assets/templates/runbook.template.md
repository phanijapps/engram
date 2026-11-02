---
type: runbook
id: {{RUNBOOK_ID}}
title: "{{TITLE}}"
description: "{{ONE_LINE_SUMMARY}}"
pack: {{PACK}}
status: draft
tags: []
version: 1
created_on: {{DATE}}
last_incident: null
edges:
  mitigates: []
  triggered_by: []
  uses: []
---

## Trigger

{{TRIGGER}}

## Blast radius

{{BLAST_RADIUS}}

## Steps

{{STEPS}}

## If steps don't resolve

{{ESCALATION}}

## Post-incident

- [ ] Root cause identified and cited as `file:line`
- [ ] The violated rule's counterexample matches this incident
- [ ] Runbook updated with what actually worked (increment `version`)
- [ ] If a new rule is needed: invoke the constitution skill

## Change log

| Version | Date | What changed | Source |
|---|---|---|---|
| 1 | {{DATE}} | Initial draft | {{SOURCE}} |
