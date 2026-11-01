# {{RUNBOOK_ID}}: {{TITLE}}

<!-- Procedural register: numbered steps, one instruction per step.
     Version: {{VERSION}} | Validated against: {{REVISION}} | Last incident: {{LAST_INCIDENT or "none"}} -->

## Trigger

{{TRIGGER: the observable condition that fires this runbook — metric, alert,
log pattern, or user report. Include severity if known.}}

## Blast radius (check first)

{{BLAST_RADIUS: which components, surfaces, and users are affected. Query the
components registry and distribution surfaces for the touched paths.}}

## Steps

{{STEPS: numbered, imperative, one instruction per step. Each step either
diagnoses further or mitigates. Mark decision points clearly.}}

1. {{STEP_1: first diagnostic or mitigation action}}
2. {{STEP_2}}
3. {{STEP_3}}

## If steps don't resolve

{{ESCALATION: who to escalate to, what evidence to attach (logs, L3 events,
gate output), what to try next.}}

## Post-incident (mandatory)

- [ ] Root cause identified and cited as `file:line` or config path
- [ ] The violated rule's counterexample matches this incident (or a new rule is needed)
- [ ] Runbook updated with what actually worked (increment `version`)
- [ ] If a new rule is needed: invoke engram-constitution to draft it (scar-born rule)

## Change log

| Version | Date | What changed | Source |
|---|---|---|---|
| {{VERSION}} | {{DATE}} | Initial draft | {{SOURCE: incident, review, or proactive}} |
