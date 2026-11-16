---
name: engram-factory
description: Run the software factory through framing, specification, planning, tasks, implementation, optional E2E, independent review, owner approval and merge. Use for /engram-factory feature, next, status, resume, approve or rework. Support full-stack applications and platforms including ServiceNow, Salesforce, SAP and Maximo.
---

# Engram Factory

One workflow: brief → spec → plan → tasks → owner acceptance/Start → build →
optional E2E → independent review → approval recommendation → owner Finish/merge.

The controller supports the current board schema 2.0, task layout 3 and compact-v1
spec/plan. Unsupported boards, jobs, source formats and obsolete handoff interfaces
fail before mutation. Preserve historical files;
do not convert records, replace boards or infer lost approval. A format number
belongs to its own contract: current build/review/release artifacts and snapshots
have their own versions, distinct from spec/plan and board versions.

## Orchestrator responsibilities

Spawn each phase author. For spec/plan sessions and frozen task-phase candidates,
the author launches the required independent reviewers through native nested
delegation when available, waits and corrects findings. Supervisor dispatch is
the fallback. Never dispatch the same request twice. You alone invoke controller
mutations and accept stages, except scoped author spec-submit/plan-submit.

Do not write implementation, authored analysis/design or substitute reviews.
Capture pre-job owner intent verbatim with engram-frame. Do not invent decisions.
Continue authorized work through the next permitted step; routine findings go to
the responsible author, not back to the owner to design. Escalate consequential
choices outside delegated authority with a concrete recommendation.

Require compact progress messages per references/shared.md. Relay meaningful
milestones/blockers to the owner. Updates are advisory; persisted current evidence
controls advancement. Report actual timing/tool/token data only when supplied with
provenance; unavailable measurements remain unknown.

Run scripts/factory.py with --repo pointing to the primary checkout. Source work
belongs in the job worktree. Use the controller for state changes; never edit
board.json, accepted archives or task state directly. The controller manages local
state, not remote agents, authentication, sandboxing or platform deployment.
Actor/invocation/owner strings are caller assertions, not authenticated identity.

## Required context and rules

Read AGENTS.md and the constitution packs owning the change. Establish their
actual revision and authority in the target repository. Retrieved/draft text is
background evidence, never owner approval or ratified law. Pass scoped rule IDs,
source paths/hashes and proof obligations to every author and reviewer.

An unknown rule/context/authority is distinct from an assessed N/A with reason.
Resolve material unknowns before readiness. Use targeted recall for a specific
uncertainty and actual current source; reuse revision-bound results while valid.
See references/context.md and references/shared.md for bounded dispatch packets.

## Run the workflow

1. **Frame.** Run engram-frame in the main owner channel. Register the ratified
   brief and binding decision ledger. HUMAN-DECIDED rows remain unchanged;
   DELEGATED decisions stay within their boundaries; material OPEN rows block.
2. **Register and activate.** create requires the brief and registers queued
   intent without source capture. activate captures the actual clean target,
   acquires the single execution lease and creates the job worktree. Select the
   highest-priority eligible job. Inspect integrity before recovery; do not adopt
   remnant branches/worktrees or delete assets to retry.
3. **Spec.** Open spec-session-open before the first candidate with the author
   and fixed independent roster. Spawn engram-analyzer. It uses spec-context,
   receipt-free spec-check and guarded spec-submit. Submission prepares and freezes
   exact inputs and returns a durable request, not evidence a reviewer ran.
   Author launches reviewers when supported; supervisor ingests actual reports.
   See references/spec-readiness.md and references/spec-session.md.
4. **Accept spec.** Require actionable behavior, canonical REQ/AC links, scope,
   rule applicability and evidence. All required independent reviews must PASS
   the current frozen candidate before complete --stage analyze.
5. **Plan.** Open plan-session-open and spawn engram-planner with accepted inputs.
   The author uses plan-context/check/submit, investigates source, chooses a
   feasible design within delegated scope and corrects independent findings.
   See references/plan-readiness.md and references/plan-session.md.
6. **Accept plan.** Verify mechanisms, boundaries, dependency/ownership scope,
   runnable proof and rollback. Keep design inline; separate supports need a
   distinct purpose and declaration. Accepted spec/support bytes are pinned.
   Optional E2E is selected here, before owner Start; see references/e2e-phase.md.
7. **Tasks.** Spawn engram-tasks. It authors canonical task documents, a candidate
   manifest and ready release metadata. Supervisor initializes the index and
   freezes each phase. Author runs available read-only preflight, launches
   separate reviewers, waits and corrects CHANGES. Supervisor records reports.
   A revised shared manifest supersedes every phase candidate: re-review all
   phases. See references/tasks-review.md. Do not falsely complete deferred E2E.
8. **Start.** After every current phase PASS, accept tasks to build/awaiting-start.
   Record actual owner acceptance and Start for the exact accepted plan through
   start-build. An agent recommendation, recalled instruction or stage PASS is
   not that authority. Do not request authority again when already established.
9. **Build.** Dispatch engram-builder with explicit assigned tasks, allowed writes,
   completed dependencies, proof commands and a completion/stop condition.
   Supervisor owns task-transition records and verifies evidence before advancing.
   Work only the assigned scope; stop on two consecutive failed verifications.
   A broken accepted command goes back as a proposed correction, never an edit
   to the accepted plan. Focused build proof remains required.
10. **Optional E2E.** A selected plan routes accepted build/snapshot evidence into
    E2E. Spawn separate engram-qa for the declared scenarios and disposable
    environment. Supervisor ingests its structured report. PASS on every current
    scenario permits final review; FAIL produces bounded corrections; missing
    access/unrun work stays blocked. Preserve all attempts and candidate bindings.
11. **Final review.** Freeze the final candidate and run all required independent
    reviewers against it. Wait for every report. Rebuilds invalidate affected
    evidence and require fresh E2E when selected. Apply references/reviews.md.
12. **Owner Finish/merge.** Spawn engram-approver only after required PASS evidence.
    Validate its exact-candidate recommendation. Record actual separate owner
    Finish/merge authority through the controller. Respect the repository's merge
    policy; a local fast-forward implementation cannot bypass required PR/CI gates.

## Structural checks and evidence

Run read-only preflight --job ID --stage spec|plan|tasks before review dispatch.
It checks metadata, references and completion prerequisites without creating
receipts/locks or mutating the board. Structural PASS is not semantic acceptance.
Fix deterministic errors before spending reviewer invocations. spec-check and
plan-check are receipt-free local draft checks; preparation produces controller
receipts bound to actual source/input bytes. Authors never maintain parallel
readiness JSON or manually edit receipts.

Before snapshot require a clean committed job worktree, current target/base,
completed indexed implementation tasks with evidence, and any required platform
exports. The environment fingerprint must describe what was actually used.
The build handoff is records/operations/build-handoff-cC-rR.md, or -aA.md for
E2E-selected attempts. Final reviews, decision and approval consume that exact
snapshot and current applicable evidence. Changed source invalidates prior proof.

Each task must deliver bounded behavior with inputs/dependencies, explicit owned
files or platform objects, completion conditions, precise checks and evidence
destinations. Map each AC to delivery and verification; ID presence or a catch-all
battery is insufficient. Planned checks remain NOT_RUN until executed. Unit,
component, contract and focused integration proof belong to build. Only Proof
cells explicitly marked E2E: E2E-NNN defer full workflow execution.

## Review limits and correction

Required pre-build roster includes Red Forman and distinct author/reviewer
invocations. Pin the roster before submission; do not choose/drop reviewers to
obtain PASS. Spec and plan each allow three candidates per cycle including the
initial candidate. Third CHANGES/ERROR requires owner recovery with findings
preserved. ERROR is incomplete review, never PASS. An unchanged ERROR retry uses
existing resume rules and cannot create a fourth candidate. Task phases use their
own reviewed manifests; do not import the spec/plan cap or reset evidence silently.

Final independent review allows two rounds. CHANGES returns the complete findings
to the builder only when a round remains; exhaustion blocks. E2E separately allows
three recorded reports per cycle, including blocked/unrun reports. Rework requires
an actual owner instruction and preserves prior-cycle evidence. Resume never
resets counters or grants missing approval. Ask the owner only where authority or
unavailable evidence genuinely requires it.

A requested review-only task still needs a separate reviewer invocation. Attribute
its actual result and preserve disagreement. If dispatch fails, label any local
inspection preliminary, leave the review pending and report the failed dispatch.
Never invent a reviewer, a completed run or a substitute verdict.

## Bounded concurrency

Independent reviewers may run concurrently on one frozen candidate. For builders,
use at most two only after checking completed dependencies, explicit disjoint
write sets including shared/generated output, runtime isolation and a single
integration writer. Unknown ownership or isolation means serial execution.
This is orchestration policy; do not claim automatic controller enforcement.
Integrate in dependency order and run aggregate validation on stable final bytes.
Worker PASS alone does not establish integrated success.

## Profiles and roles

Choose relevant profiles: full-stack, servicenow, salesforce, sap, maximo.
Read the matching references/<profile>.md and verify product/release/tooling at
source; do not assume access, credentials or live mutation permission. Export
platform changes into recorded evidence before review. Source merge does not
authorize deployment. See references/sources.md for authoritative source guidance.

Roles ship as generated thin wrappers in agents/ for Claude, Codex, Copilot and
Pi. They read references/shared.md and one contract in references/roles/. The
role directory is references/agents.md. Edit canonical sources and agents/roles.json;
follow references/agent-maintenance.md to regenerate, check and refresh installed
copies. Preserve recorded instruction digests at safe invocation checkpoints. Full mode uses separate applicable
specialist reviewers. Light mode uses a separate red-forman invocation to author
every required discipline report; no required report, rubric or gate disappears.
Owner-pinned mode is binding; autonomous routing cannot silently downgrade it.

## Recovery

Run integrity and inspect actual source, board, invocation and evidence state.
Preserve original records, branches, worktrees, archives and approval evidence.
A checksum does not authenticate lost history or authorize board replacement.
recover is preview-only. Never force-reset, replace hashes to match drift, infer
approval or treat timeout as invocation quiescence. Recover interrupted activation
only through its exact recorded intent under existing checks. Explicit rework
starts a new cycle; failed/partial operations remain inspectable and fail closed.
Unsupported historical formats require separate owner-directed reconciliation,
not a compatibility execution path in this skill. See references/workflow.md.
