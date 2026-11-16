# Workflow and recovery

## State model

queued → analyze → plan → tasks → build/awaiting-start → build/running →
optional e2e → review → approve → merge → merged.

Board schema 2.0 and task layout 3 are the supported execution contract. Specs
and plans are compact-v1. The controller rejects unsupported input formats
before state mutation. It does not convert records or recover lost authority.

create registers a ratified brief and queued intent. activate captures the clean
actual target and starts the single execution lease. Queue priority is explicit;
blocked work releases its lease only after invocation quiescence is established.
All source changes occur in the assigned worktree. Runtime state is not committed.

Spec/plan author sessions pin an independent roster before candidate1. Local
draft checks do not create receipts or spend candidates. Scoped submission checks
revision, accepted inputs and exact authored bytes, prepares/freezes the candidate
and records an idempotent request. Request existence is not reviewer execution.
Actual runtime delegation is required and replay must not duplicate dispatch.
See spec-session.md and plan-session.md.

Spec and plan each allow at most three candidates per cycle. All required PASS
receipts must apply to current bytes. CHANGES routes to the author; third CHANGES
or ERROR exhausts the phase. ERROR is incomplete review, never acceptance.
Unchanged ERROR recovery may retry only within its bound. Preserve findings,
actual receipts and invocation evidence. Do not reset counters through resume.

Task generation produces tasks/task-NNN.md, a candidate manifest under
records/operations/ and release.md. Supervisor initializes tasks/manifest.json
and tasks/task.json, runs preflight and freezes/reviews every task phase.
Reinitialization is a supervised correction of a fully-pending index; preserve
superseded manifests and reports and review every phase again. Task phases do
not inherit the spec/plan candidate limit. See tasks-review.md.

After current phase reviews PASS, task completion enters awaiting-start. Owner
acceptance and Start bind the exact plan/manifest before implementation. The
orchestrator records task transitions, actor, dependency order and evidence via
the controller. A worker result is not a state transition. No task is done solely
because source changed or an unexecuted command was listed in a report.

Build snapshot requires completed indexed implementation work, clean committed
source, unchanged target/base, actual environment fingerprint and applicable
platform exports. The handoff is records/operations/build-handoff-cC-rR.md (with
-aA for E2E-enabled attempts). Focused task proof belongs to build; full workflows
explicitly selected by the reviewed plan belong to E2E.

E2E does not consume a final-review round. Separate QA executes the exact scenario
set and returns records/e2e/report-cC-aA.json. Current all-PASS permits final
review; FAIL returns bounded corrections/new snapshot; BLOCKED/NOT_RUN preserves
a blocked candidate. Three reports per cycle include blocked reports. Exhaustion
requires owner recovery. A source or environment change invalidates applicability;
old reports remain historical evidence. See e2e-phase.md.

Final review allows two rounds. Reviewers remain distinct from builder and E2E
QA, and full-mode discipline invocations differ. CHANGES returns to build only
when a round remains; selected E2E must run again on corrected bytes. ERROR blocks
for recovery. Only all current PASS results allow the approver recommendation.
That recommendation does not grant separate owner Finish/merge authority.

## Recovery

Read integrity, actual Git state, owner records, invocation state and evidence.
Preserve all assets. Board missing plus remnants is not permission to init.
recover --board-file previews consistency only; no automatic board adoption is
supported. Checksums do not authenticate lost owner decisions or missing history.

recover-activation completes only the unchanged controller-recorded activation
intent under its lease, branch/base, queue, clean-state and evidence checks.
Other interrupted operations remain inspectable and fail closed. Do not delete
orphan worktrees/branches or overwrite archives simply to make a retry pass.

Resume requires the recorded failure to be resolved, current evidence and source
to match, and invocations to be quiescent when required. It does not reset limits,
change scope or grant approval. Explicit owner rework preserves prior-cycle
records, starts a new cycle and repeats acceptance stages. Changed acceptance is
not a small builder fix. Source integration uses the approved worktree and review
path; no force reset/push or edited hash can substitute for that evidence.

The controller's merge implementation is guarded fast-forward only. Repository
PR/CI/server policies still apply; unsupported merge integrations must stop at
that boundary. Source merge never authorizes platform deployment.
