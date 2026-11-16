# Artifact contracts

Use one human-readable source per concern. Keep spec behavior/acceptance in
spec.md, design/proof in plan.md, execution assignments in tasks/task-NNN.md,
results in indexed evidence and the round build handoff. Do not repeat accepted
content, receipt hashes, review transcripts or readiness JSON across documents.

## Authoring

Spec and plan use assets/templates/spec.md and plan.md with compact-v1 headers.
Controller preparation derives their machine identity and receipts. Optional
supporting documents are plain Markdown declared in the plan, at canonical
context/ paths; split only for a distinct purpose. See spec-readiness.md and
plan-readiness.md for their table contracts and independent review obligations.

Each task uses assets/templates/task.md. Give an owner-readable outcome, bounded
owned files/platform objects, accepted input/REQ/AC/rule references, dependencies,
precise proof and evidence destinations. A task may cover multiple ACs and an AC
may require multiple tasks; verify substantive delivery, not only ID presence.
The candidate manifest groups canonical task paths and dependency edges; the
controller alone creates the accepted manifest and mutable task index.

Build, review, release, decision and rework use their named templates. Common
metadata uses one JSON value per frontmatter line: identity, job/cycle, role,
profiles, actual direct input references, status and timezone timestamp. Build
and later evidence uses the actual snapshot path. A template's schema version
is specific to that artifact type; do not infer workflow age from the number.
Use ready only after the artifact meets its contract. Unknown material evidence
is blocked, not a reasonless N/A or fabricated PASS.

Build reports name the actual builder invocation. The canonical round handoff is
records/operations/build-handoff-cC-rR.md; E2E-enabled build attempts add -aA.
Task completion evidence stays indexed and immutable. E2E has its own structured
report and raw evidence per e2e-phase.md. Final reviewer reports live under
reviews/cycles/cycle-C/round-R/ and the approver decision is reviews/final-review.md.

## Writing

Use clear active sentences, one instruction per imperative step and concrete
observable outcomes. Owner summaries explain the change, proof and recovery
without internal bookkeeping. Task summaries use RED/GREEN when that proof shape
matters. Put mechanisms and paths in executor detail, not duplicated owner lists.
Use diagrams when they clarify a material boundary/flow or the owner requests one.

Use stable complete IDs and explicit references. One requirement describes one
behavior. Define each AC once; reference it elsewhere. Tables contain one idea
per row. State decisions with actual authority and source evidence. Preserve
HUMAN-DECIDED intent and delegated bounds; formatting cannot resolve authority.

Record actual commands, exits, environment and raw output. Keep failed runs and
negative evidence: what was unverified, why, who owns it and the revisit trigger.
Manual proof needs a reproducible protocol and observation; it has no invented
shell exit. Planned tests are NOT_RUN. Exceptions require actual owner authority
and never become PASS by relabeling. Review uses PASS/CHANGES/ERROR; checks use
PASS/FAIL/NOT_RUN/BLOCKED. Show unknowns rather than guessing measurements.

## Ownership and preservation

Authors write only their assigned outputs. Controller owns board, receipts,
accepted archives, manifest/index and state transitions. QA/reviewers write only
assigned reports/evidence and authorized disposable test output. The builder
never edits the accepted spec/plan to fit implementation. Scope or contract
changes follow supervised review and actual owner authority when required.
Preserve candidate, cycle, failed and superseded evidence; do not repair hashes,
rewrite an accepted record or infer an approval from a prior conversation.
