# Planner-led shared state

The protocol and trust/recovery boundaries are the same as spec-session.md.
Both phases delegate to one author_session.py engine; neither owns board.json.
Planning uses compact-v1. Open the author session before its first review;
never retrofit or replace a frozen candidate/roster.

| Actor | Operation | Purpose |
| --- | --- | --- |
| Supervisor | plan-session-open | Pin planner and independent roster once for this cycle |
| Planner | plan-context | Read accepted brief/spec and review evidence, revision/context identity |
| Planner | plan-check | Receipt-free design/trace/dependency/ownership/support validation |
| Planner | plan-submit | Exact revision-bound preparation, freeze and durable review request |
| Planner (supervisor fallback) | Native nested delegation | Spawn pinned independent reviewers once and wait |
| Supervisor | review-record / complete | Ingest actual verdicts; accept PASS plan to tasks |

The planner authors only plan.md and declared plain supporting documents.
plan-check returns the plan hash AND a draft fingerprint covering plan/support
bytes. Supply both, along with context identity/current board revision, on submit:

`factory.py --repo REPO --expected-revision N plan-submit --job ID --author PLANNER --operation-id OP --context-sha256 CONTEXT --plan-sha256 PLAN --draft-sha256 DRAFT`

These machine identities come from context/check, never authored arrays. Changed
support bytes invalidate submission even when plan.md is unchanged. The receipt
and every declared support are frozen together with the accepted upstream spec.
The author cannot edit/reaccept the spec or select/drop the review roster.

Correct local structural errors before submission. Freeze consumes a candidate
only after checks; CHANGES findings are read by role from plan-context, then a
new operation creates the next candidate within the existing limit. Pending,
PASS, ERROR/exhaustion and changed inputs retain existing supervisor recovery.
Exact retries return the same request and forbid redispatch. Request creation
is not launch evidence. The planner spawns and waits for the pinned independent
reviewers through native nested delegation, relays actual report references for
supervisor ingestion, and reads shared context before correction. Supervisor
dispatch is the fallback if nested delegation is unavailable. Deduplicate by
request identity; do not spawn again on replay or uncertain launch state.

PASS is visible to the planner but does not advance the stage. Only the supervisor
accepts to tasks. Exact-plan owner acceptance and Start occur after accepted
tasks; owner Finish/merge remains separate. No author complete, review-record,
session-open, rework, approve, Start or merge commands. Invocation identities
remain caller assertions; trusted runtime restrictions, not this local CLI,
provide role isolation. Crash orphans fail closed as described in spec-session.md.

Send periodic and milestone parent messages under shared.md for parent progress
updates throughout drafting, review waits and correction. Messages carry compact
status/evidence references; persisted controller evidence still governs acceptance.
