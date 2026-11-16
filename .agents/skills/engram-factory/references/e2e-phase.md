# Optional E2E phase

Route: brief → spec → plan → tasks → build → optional E2E → independent review
→ owner approval/Finish/merge. Compact plans select the phase explicitly; unselected plans route build directly
to independent review. Do not edit accepted plans or enable/disable checks after Start.

## Select in the reviewed plan

Add `e2e: true` to the compact authored header only when the phase is selected.
Add one `## E2E` table with these columns:

| ID | Acceptance | Command or protocol | Expected | Environment prerequisites |
| --- | --- | --- | --- | --- |
| E2E-001 | AC-001 | MANUAL: exercise the accepted full workflow and retain its observations | The accepted outcome is observable across the complete workflow | Disposable environment and required test identities provisioned |

For a Proof gate executed in the E2E phase, its `Command or protocol` cell is
`E2E: E2E-001` (or a comma-separated list of declared scenario IDs). This is a
phase reference, not a shell command. The scenario table holds the actual
command/protocol once. Other Proof gates remain implementation checks. The
builder defers only explicitly referenced E2E execution, never arbitrary gates.

Use scenario IDs once and cite real accepted ACs. Name executable commands or
precise manual protocols, observable outcomes, environment/configuration,
credentials/access prerequisites and cleanup responsibilities. Keep raw secrets
out of artifacts. E2E is domain-neutral: API workflows, UI journeys, Salesforce
business processes, ServiceNow/SAP/Maximo workflows and other integrated paths.
Do not add empty E2E sections to every plan. Omission is not authority to waive
required acceptance proof or constitutional checks; reviewers verify applicability.

## Build and task boundary

Implementation tasks cover bounded source/platform changes and test authoring.
Their focused unit/component/contract/integration checks remain build proof.
The separate scenario table owns full E2E execution; do not add pending E2E-only
execution tasks to the implementation manifest and then claim they are done just
to snapshot the build. Tasks explicitly identify deferred E2E scenario references.
A task-level test required to prove an implementation change is not automatically
reclassified as E2E to bypass its gate.

The builder returns implementation-ready evidence on a clean committed candidate,
not a claim of overall acceptance. For opted-in jobs the controller routes the
accepted build snapshot to E2E before any final reviewer executes. No routine
whole-system battery is implicitly added to each implementation task. If a
planned E2E prerequisite cannot be created inside assigned implementation scope,
report it to the supervisor rather than expanding builder work.

## Execution and corrections

The orchestrator dispatches a separate engram-qa invocation with the accepted
plan, exact source snapshot, environment fingerprint, declared scenarios,
allowed disposable resources and report/evidence destinations. QA may execute
those scenarios and write assigned evidence only; it cannot edit application
source, repair tests, change acceptance thresholds or mutate the controller.
Platform/live writes require explicit dispatch authority and disposable targets.

QA records actual commands/protocol observations, outcomes, exits when available,
NOT_RUN/BLOCKED reasons and raw evidence references. Missing access or unexecuted
scenarios never count as PASS. The supervisor ingests the structured report;
metadata checks bind evidence but cannot prove the claimed execution occurred.
Independent reviewers still inspect implementation and verification evidence.

Product defects return to the builder as bounded corrections, not a new open-ended
job. Instrument defects return to the appropriate author; changing an accepted
contract or threshold uses existing supervised review/owner rules. Preserve prior
attempts, blocked reports and raw evidence; a changed candidate must go through
fresh E2E. Follow controller retry bounds, not unlimited autonomous retries.

Parent status follows references/shared.md. Do not fabricate executed scenarios.

PASS for every declared scenario on the current candidate permits independent
review. It does not grant owner approval, Finish, merge or deployment authority.

## Controller handoffs

For opted-in builds the round handoff is
records/operations/build-handoff-cC-rR-aA.md (attempt suffix preserves each build
correction before a review round). Unselected builds use the round path without an attempt suffix.
Submit each QA result with `complete --job ID --stage e2e --artifacts
records/e2e/report-cC-aA.json`. The report is a JSON object with exactly these
keys: schema_version (integer 1), job_id, cycle, attempt, author_role (engram-qa),
invocation_id, source_snapshot and plan references (path/sha256), head_sha,
environment_sha256, observed_at (timezone timestamp), and scenarios (one row per
declared scenario). Source and environment identities come from the exact candidate.

Executed command PASS requires exit 0 and hashed raw evidence. Manual PASS
requires the exact declared protocol, observed result and evidence, without a
fabricated exit. Rows cannot be missing/duplicated/unknown. The controller
archives the report/evidence and derives the overall disposition. PASS starts
final independent review; FAIL returns to bounded build correction; BLOCKED or
NOT_RUN retains a blocked candidate. Three recorded reports per cycle, including
blocked runs, is the separate E2E attempt bound. Exhaustion preserves all
evidence for existing owner recovery. Review rounds retain their own bound.

Literal scenario keys (no additional row fields): `id`, `verdict`, `method`,
`command`, `exit_code`, `observed`, `evidence`. For example:

```json
{"id":"E2E-001","verdict":"PASS","method":"command","command":"python3 tests/full_workflow.py","exit_code":0,"observed":"The declared workflow completed with the expected outcome.","evidence":[{"path":"runs/e2e/attempt-1-workflow.log","sha256":"<digest of the actual retained log>"}]}
```

This is illustrative shape, not an execution receipt. Obtain all identities
from the actual controller candidate and evidence bytes.
